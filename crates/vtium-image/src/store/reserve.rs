//! Reservations, and the requests a protocol handler makes of the store
//! beside storing and placing (ADR-1100, D5 and D6).
//!
//! A handler reserves an image's bytes before it allocates them: a sixel
//! as its picture grows, a kitty transmission for the size its keys give
//! or each growth of a payload buffered for the helper. A reservation for
//! a kitty id already in the buffer counts what that image holds and keeps
//! it from eviction until the new image is stored or the reservation is
//! given back, so a refused replacement keeps the old image.
//!
//! The store keeps a ledger of the reservations it granted and has not
//! seen spent. Spending one, to store an image or to give it back, takes
//! it out of the ledger, so a copy of a reservation already spent gives
//! back nothing, holds nothing and pays for nothing. Without the ledger, a
//! handler that gave back a copy of a reservation it had already spent
//! would give its bytes back twice, and the budget would then count less
//! than the images it holds.

use super::{Screens, Store};
use crate::image::{ImageKey, Refusal};
use crate::rules::{Protocol, rules};

/// Bytes a handler reserved for an image to come. It is spent once, by
/// [`Store::add_image`] or [`Store::unreserve`], which take it by value.
///
/// It is `Clone` so that a handler holding one can be cloned with the
/// terminal whose store holds its bytes; the clone belongs to the cloned
/// store, whose ledger the store's clone copied. Within one store, the
/// ledger spends each reservation once: a second copy given back returns
/// no bytes and releases no hold, a second copy passed to
/// [`Store::add_image`] counts as no reservation, so the image reserves its
/// whole cost, and [`Store::grow`] refuses to grow a copy already spent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reservation {
    /// The reservation's entry in the store's ledger.
    pub(super) id: u64,
    pub(super) bytes: u64,
    /// The image of the same kitty id a replacement will free, held; for
    /// a frame reservation, the image the frame goes to, held.
    pub(super) old: Option<ImageKey>,
    pub(super) buffer: usize,
    /// It holds bytes of the frame allowance, for an animation frame's
    /// data (ADR-1102, D1), where every other reservation holds bytes of
    /// the image budget.
    pub(super) frames: bool,
}

impl Reservation {
    /// The bytes it holds of the budget.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl Store {
    /// Reserves `bytes` for an image of `protocol` with kitty id `id` (0
    /// for none) to come in the buffer in use. An image of that id there is
    /// the one it will replace: what it holds counts towards the new one,
    /// and it is held until the reservation is used or given back. A request
    /// that cannot fit evicts nothing and is refused.
    pub fn reserve(
        &mut self,
        protocol: Protocol,
        id: u32,
        bytes: u64,
        screens: Screens<'_>,
    ) -> Result<Reservation, Refusal> {
        self.reserve_as(protocol, id, bytes, true, screens)
    }

    /// Reserves `bytes` for a kitty upload in chunks of id `id` in the
    /// buffer in use, beside the image of that id there, which stays
    /// counted and held until the reservation is used or given back. The
    /// budget then holds both the old image and the whole upload, so
    /// keeping the old image while the upload is open never takes the store
    /// past the budget. A request that cannot fit beside it evicts nothing
    /// and is refused, and the old image stays (ADR-1100, D16).
    pub fn reserve_beside(
        &mut self,
        protocol: Protocol,
        id: u32,
        bytes: u64,
        screens: Screens<'_>,
    ) -> Result<Reservation, Refusal> {
        self.reserve_as(protocol, id, bytes, false, screens)
    }

    /// [`reserve`](Self::reserve), with the old image's bytes counted
    /// toward the new one when `credit` is set, and beside it otherwise.
    fn reserve_as(
        &mut self,
        protocol: Protocol,
        id: u32,
        bytes: u64,
        credit: bool,
        screens: Screens<'_>,
    ) -> Result<Reservation, Refusal> {
        let b = self.active();
        let old = if rules(protocol).ids && id != 0 {
            let count = self.buffers.get(b).map_or(0, |buf| buf.images.len());
            self.count(count);
            self.buffers.get(b).and_then(|buf| buf.find_id(id))
        } else {
            None
        };
        self.hold(b, old, true);
        self.settle(screens);
        let reserved = if credit {
            self.reserve_bytes(bytes, old.map(|o| (b, o)), screens)
        } else {
            self.reserve_bytes_beside(bytes, old.map(|o| (b, o)), screens)
        };
        match reserved {
            Ok(added) => Ok(self.grant(added, old, b)),
            Err(why) => {
                self.hold(b, old, false);
                Err(why)
            }
        }
    }

    /// A reservation of `bytes` the store already holds, as the spare
    /// buffer's charge passes to a handler: an image of `protocol`'s kitty
    /// id `id` in the buffer in use is held as [`reserve`](Self::reserve)
    /// holds it.
    pub(super) fn spare_reservation(
        &mut self,
        protocol: Protocol,
        id: u32,
        bytes: u64,
    ) -> Reservation {
        let b = self.active();
        let old = if rules(protocol).ids && id != 0 {
            self.buffers.get(b).and_then(|buf| buf.find_id(id))
        } else {
            None
        };
        self.hold(b, old, true);
        self.grant(bytes, old, b)
    }

    /// Enters a reservation of `bytes`, which `used` already counts and
    /// which holds image `old` of buffer `buffer`, in the ledger.
    fn grant(&mut self, bytes: u64, old: Option<ImageKey>, buffer: usize) -> Reservation {
        let id = self.next_reservation;
        self.next_reservation = self.next_reservation.wrapping_add(1);
        self.reservations.insert(id, bytes);
        Reservation {
            id,
            bytes,
            old,
            buffer,
            frames: false,
        }
    }

    /// Enters a reservation of `bytes` of the frame allowance, which
    /// `frame_used` already counts, for a frame of image `image` of buffer
    /// `buffer`, which it holds, in the ledger.
    pub(super) fn grant_frames(
        &mut self,
        bytes: u64,
        image: ImageKey,
        buffer: usize,
    ) -> Reservation {
        self.hold(buffer, Some(image), true);
        let mut r = self.grant(bytes, Some(image), buffer);
        r.frames = true;
        r
    }

    /// Takes `reservation` out of the ledger and returns the bytes it
    /// holds of the budget, or `None` when the ledger no longer has it
    /// because a copy of it was already spent.
    pub(super) fn spend(&mut self, reservation: &Reservation) -> Option<u64> {
        self.reservations.remove(&reservation.id)
    }

    /// Adds `added` bytes, which `used` already counts, to a reservation
    /// the ledger holds.
    pub(super) fn add_to(&mut self, reservation: &mut Reservation, added: u64) {
        if let Some(bytes) = self.reservations.get_mut(&reservation.id) {
            *bytes = bytes.saturating_add(added);
            reservation.bytes = *bytes;
        }
    }

    /// Reserves `more` bytes for a reservation's payload as it grows: of
    /// the image budget, or for a frame reservation of the frame allowance,
    /// whose freeing never takes the frames of the image the frame goes to
    /// (ADR-1102, D1). A copy of a reservation already spent is refused as
    /// [`Refusal::Invalid`], and nothing is reserved.
    pub fn grow(
        &mut self,
        reservation: &mut Reservation,
        more: u64,
        screens: Screens<'_>,
    ) -> Result<(), Refusal> {
        if !self.reservations.contains_key(&reservation.id) {
            return Err(Refusal::Invalid);
        }
        if reservation.frames {
            let image = reservation.old.ok_or(Refusal::Invalid)?;
            self.reserve_frames(more, (reservation.buffer, image))?;
            self.add_to(reservation, more);
            return Ok(());
        }
        self.settle(screens);
        let added = self.reserve_bytes(more, None, screens)?;
        self.add_to(reservation, added);
        Ok(())
    }

    /// Gives a reservation back, unused: its bytes return to the budget and
    /// the image it held may go again. A copy of a reservation already
    /// spent gives back nothing.
    pub fn unreserve(&mut self, reservation: Reservation) {
        let Some(bytes) = self.spend(&reservation) else {
            return;
        };
        self.hold(reservation.buffer, reservation.old, false);
        if reservation.frames {
            self.frame_used = self.frame_used.saturating_sub(bytes);
        } else {
            self.used = self.used.saturating_sub(bytes);
        }
    }

    /// Frees an image of the buffer in use, with any placements it has, as
    /// a handler does with a picture it could not place. A held image
    /// stays.
    pub fn drop_image(&mut self, key: ImageKey) {
        let b = self.active();
        if self.image(b, key).is_some_and(|i| i.hold == 0) {
            self.free_image(b, key);
        }
    }

    /// The image a kitty command names in the buffer in use: by its id, or
    /// with no id by its number, the newest image of that number. Neither
    /// names nothing.
    pub fn find_image(&mut self, id: u32, number: u32) -> Option<ImageKey> {
        let b = self.active();
        let count = self.buffers.get(b).map_or(0, |buf| buf.images.len());
        if id == 0 && number == 0 {
            return None;
        }
        self.count(count);
        let buf = self.buffers.get(b)?;
        if id != 0 {
            return buf.find_id(id);
        }
        buf.images
            .iter()
            .filter(|(_, i)| rules(i.protocol.0).ids && i.number == number)
            .map(|(&k, _)| k)
            .next_back()
    }

    /// A command named an existing image, such as a kitty put or query:
    /// the stream event moves the use clock and stamps the image (D5).
    pub fn touch(&mut self, key: ImageKey) {
        let b = self.active();
        if self.image(b, key).is_some() {
            let stamp = self.tick();
            if let Some(image) = self.image_mut(b, key) {
                image.used_at = stamp;
            }
        }
    }

    /// The lowest kitty id no image of the buffer in use has, for an image
    /// sent with a number alone.
    pub fn free_id(&mut self) -> u32 {
        let b = self.active();
        let Some(buf) = self.buffers.get(b) else {
            return 1;
        };
        let mut ids: Vec<u32> = buf
            .images
            .values()
            .filter(|i| rules(i.protocol.0).ids && i.id != 0)
            .map(|i| i.id)
            .collect();
        let visited = buf.images.len().saturating_mul(2);
        self.count(visited);
        ids.sort_unstable();
        let mut id = 1u32;
        for n in ids {
            if n == id {
                id = id.saturating_add(1);
            } else if n > id {
                break;
            }
        }
        id
    }
}
