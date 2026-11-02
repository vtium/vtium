//! Reservations, and the requests a protocol handler makes of the store
//! beside storing and placing (ADR-1100, D5 and D6).
//!
//! A handler reserves an image's bytes before it allocates them: a sixel
//! as its picture grows, a kitty transmission for the size its keys give
//! or each growth of a payload buffered for the helper. A reservation for
//! a kitty id already in the buffer counts what that image holds and keeps
//! it from eviction until the new image is stored or the reservation is
//! given back, so a refused replacement keeps the old image.

use super::{Screens, Store};
use crate::image::{ImageKey, Refusal};
use crate::rules::{Protocol, rules};

/// Bytes a handler reserved for an image to come. It is spent once, by
/// [`Store::add_image`] or [`Store::unreserve`], which take it by value. It
/// is `Clone` only so a handler holding one can be cloned with the
/// terminal whose store holds its bytes; a clone belongs to the cloned
/// store, and spending one reservation twice in one store gives back bytes
/// that store never granted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reservation {
    pub(super) bytes: u64,
    /// The image of the same kitty id a replacement will free, held.
    pub(super) old: Option<ImageKey>,
    pub(super) buffer: usize,
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
        match self.reserve_bytes(bytes, old.map(|o| (b, o)), screens) {
            Ok(added) => Ok(Reservation {
                bytes: added,
                old,
                buffer: b,
            }),
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
        Reservation {
            bytes,
            old,
            buffer: b,
        }
    }

    /// Reserves `more` bytes for a reservation's payload as it grows.
    pub fn grow(
        &mut self,
        reservation: &mut Reservation,
        more: u64,
        screens: Screens<'_>,
    ) -> Result<(), Refusal> {
        self.settle(screens);
        let added = self.reserve_bytes(more, None, screens)?;
        reservation.bytes = reservation.bytes.saturating_add(added);
        Ok(())
    }

    /// Gives a reservation back, unused: its bytes return to the budget and
    /// the image it held may go again.
    pub fn unreserve(&mut self, reservation: Reservation) {
        self.hold(reservation.buffer, reservation.old, false);
        self.used = self.used.saturating_sub(reservation.bytes);
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
