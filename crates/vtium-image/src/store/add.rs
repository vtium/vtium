//! Storing a new image in the buffer in use, with its reservation.

use crate::bounds::{IMAGE_OVERHEAD, IMAGE_SIDE_MAX};
use crate::image::{ImageKey, NewImage, Pixels, Refusal};
use crate::rules::{Kept, rules};

use super::{Image, Indices, Reservation, Screens, Store, all_opaque};

impl Store {
    /// Stores an image in the buffer in use, reserving its whole cost first
    /// (D5): what `reservation` already holds, which a handler reserved as
    /// its payload arrived, and the rest now. An image of a kitty id already
    /// in the buffer replaces it, with its placements, once the new one has
    /// its bytes; refused, the old one stays, and so does nothing of the
    /// reservation.
    pub fn add_image(
        &mut self,
        new: NewImage,
        reservation: Option<Reservation>,
        screens: Screens<'_>,
    ) -> Result<ImageKey, Refusal> {
        self.add_image_kept(new, None, reservation, screens)
    }

    /// [`add_image`](Self::add_image), for a picture that also keeps the
    /// palette `indices` it arrived as, whose bytes its charge counts too:
    /// one a pixel, or two with alpha. Indices that are not one group for
    /// each pixel are refused as pixels of the wrong count are.
    ///
    /// References:
    /// - ADR-1100 D15: what a palette-index picture costs the budget.
    pub(crate) fn add_image_kept(
        &mut self,
        new: NewImage,
        indices: Option<Indices>,
        reservation: Option<Reservation>,
        screens: Screens<'_>,
    ) -> Result<ImageKey, Refusal> {
        self.add_image_from(new, None, indices, reservation, screens)
    }

    /// [`add_image`](Self::add_image) for a picture whose pixels are
    /// `pixels`, already shared with another store, in place of
    /// `new.pixels`, which stays empty. The store charges the picture's
    /// whole cost, as for any image, and keeps no copy of its own.
    ///
    /// References:
    /// - ADR-1100 D17: the consoles of a head share one copy of the login
    ///   picture's pixels, and each console's store charges it.
    pub fn add_shared_image(
        &mut self,
        new: NewImage,
        pixels: Pixels,
        screens: Screens<'_>,
    ) -> Result<ImageKey, Refusal> {
        self.add_image_from(new, Some(pixels), None, None, screens)
    }

    /// The one way an image is stored: from `new.pixels`, or from `shared`
    /// when it is given.
    fn add_image_from(
        &mut self,
        new: NewImage,
        shared: Option<Pixels>,
        indices: Option<Indices>,
        reservation: Option<Reservation>,
        screens: Screens<'_>,
    ) -> Result<ImageKey, Refusal> {
        let b = self.active();
        // the image a reservation held for its replacement, when it is
        // still in the buffer in use with the id; it is held no more. A copy
        // of a reservation already spent holds nothing, so the image
        // reserves its whole cost
        // a frame reservation holds the frame allowance, which pays for no
        // image: it is given back, and the image reserves its whole cost
        let reservation = match reservation {
            Some(r) if r.frames => {
                self.unreserve(r);
                None
            }
            other => other,
        };
        let spent = reservation.and_then(|r| self.spend(&r).map(|bytes| (r, bytes)));
        let (mut reserved, held) = match spent {
            Some((r, bytes)) => {
                self.hold(r.buffer, r.old, false);
                let still = r
                    .old
                    .filter(|&o| r.buffer == b && self.image(b, o).is_some_and(|i| i.id == new.id));
                (bytes, still)
            }
            None => (0, None),
        };
        let refuse = |s: &mut Self, reserved: u64, why: Refusal| {
            s.used = s.used.saturating_sub(reserved);
            Err(why)
        };
        let side = |s: u32| s > 0 && u64::from(s) <= IMAGE_SIDE_MAX;
        let n = u64::from(new.width).saturating_mul(u64::from(new.height));
        let kept = indices
            .as_ref()
            .map_or(Some(0), |i| u64::try_from(i.data.len()).ok());
        let bytes = n
            .checked_mul(4)
            .and_then(|b| b.checked_add(IMAGE_OVERHEAD))
            .zip(kept)
            .and_then(|(b, k)| b.checked_add(k));
        let fits = side(new.width)
            && side(new.height)
            && u64::try_from(
                shared
                    .as_ref()
                    .map_or(new.pixels.len(), |p| p.as_slice().len()),
            )
            .ok()
                == Some(n)
            && indices.as_ref().is_none_or(|i| i.fits(n));
        let Some(bytes) = bytes.filter(|_| fits) else {
            return refuse(self, reserved, Refusal::Invalid);
        };
        let old = match held {
            Some(o) => Some(o),
            None if rules(new.protocol).ids && new.id != 0 => {
                let count = self.buffers.get(b).map_or(0, |buf| buf.images.len());
                self.count(count);
                self.buffers.get(b).and_then(|buf| buf.find_id(new.id))
            }
            None => None,
        };
        if reserved > bytes {
            self.used = self.used.saturating_sub(reserved.saturating_sub(bytes));
            reserved = bytes;
        }
        self.hold(b, old, true);
        self.settle(screens);
        let made = match old {
            Some(_) => Ok(()),
            None => self.make_image_room(b, screens),
        };
        let added = made.and_then(|()| {
            self.reserve_bytes(bytes.saturating_sub(reserved), old.map(|o| (b, o)), screens)
        });
        self.hold(b, old, false);
        let added = match added {
            Ok(added) => added,
            Err(why) => return refuse(self, reserved, why),
        };
        let credit = reserved.saturating_add(added);
        let key = ImageKey(self.next_image);
        self.next_image = self.next_image.saturating_add(1);
        let stamp = self.tick();
        let revision = self.bump();
        let opaque = match new.opaque {
            Some(opaque) => {
                debug_assert_eq!(
                    opaque,
                    all_opaque(shared.as_ref().map_or(&new.pixels, |p| p.as_slice())),
                    "the handler's opacity disagrees with the pixels"
                );
                opaque
            }
            None => {
                self.count(
                    shared
                        .as_ref()
                        .map_or(new.pixels.len(), |p| p.as_slice().len()),
                );
                all_opaque(shared.as_ref().map_or(&new.pixels, |p| p.as_slice()))
            }
        };
        if let Some(o) = old {
            // the old image's bytes beyond what was reserved pass to the new
            // one; freeing it gives back only what is left
            if let Some(image) = self.image_mut(b, o) {
                image.bytes = image.bytes.saturating_sub(bytes.saturating_sub(credit));
            }
            self.free_image(b, o);
        }
        let image = Image {
            protocol: Kept(new.protocol),
            id: new.id,
            number: new.number,
            width: new.width,
            height: new.height,
            pixels: shared.unwrap_or_else(|| Pixels::new(new.pixels)),
            opaque,
            bytes,
            owed: 0,
            used_at: stamp,
            sent_at: stamp,
            transient: new.transient,
            indices,
            placements: 0,
            hold: 0,
            revision,
            anim: None,
        };
        if let Some(buf) = self.buffers.get_mut(b) {
            buf.images.insert(key, image);
        }
        Ok(key)
    }
}
