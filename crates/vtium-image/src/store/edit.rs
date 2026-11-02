//! The one way a protocol handler changes the store (ADR-1100, D6).
//!
//! An [`Edit`] holds the store with the lines of both buffers' screens
//! already bound, so a handler names cells by screen row and column and
//! never sees a line key. It offers the operations a handler makes, each
//! one the store's own method, so the budget, the eviction order and the
//! anchors stay the store's to enforce. It never offers the screen events
//! (a cut, a scroll, a line dropped, an anchor carried, a reset), which
//! only the screen sends. A protocol module adds the operations of its own
//! in its own file, as `impl Edit<'_>` blocks beside the store code they
//! need.
//!
//! The terminal makes an `Edit` for each call of
//! [`Context::edit_store`](crate::Context::edit_store) and, once the
//! handler's function returns, settles the store and marks the cells under
//! every box it changed, as it does after a screen event.

use super::{Screens, Store};
use crate::image::{ImageKey, NewImage, Place, PlacementKey, Refusal};
use crate::rules::Protocol;
use crate::store::Reservation;

/// The store, with the screen's rows, for one edit.
#[derive(Debug)]
pub struct Edit<'a> {
    pub(crate) store: &'a mut Store,
    pub(crate) screens: Screens<'a>,
    pub(crate) cols: usize,
}

impl<'a> Edit<'a> {
    /// An edit of `store`, whose buffers show `screens`'s rows on a screen
    /// `cols` wide.
    pub fn new(store: &'a mut Store, screens: Screens<'a>, cols: usize) -> Self {
        Self {
            store,
            screens,
            cols,
        }
    }

    /// The store, to read.
    pub fn store(&self) -> &Store {
        self.store
    }

    /// Reserves `bytes` of the budget before they are allocated, for an
    /// image of `protocol` with kitty id `id` (0 for none) to come; an
    /// image of that id is held until the reservation is used or given back
    /// (D5). See [`Store::reserve`].
    pub fn reserve(
        &mut self,
        protocol: Protocol,
        id: u32,
        bytes: u64,
    ) -> Result<Reservation, Refusal> {
        self.store.reserve(protocol, id, bytes, self.screens)
    }

    /// Reserves `more` as a buffered payload grows.
    pub fn grow(&mut self, reservation: &mut Reservation, more: u64) -> Result<(), Refusal> {
        self.store.grow(reservation, more, self.screens)
    }

    /// Gives a reservation back unused.
    pub fn unreserve(&mut self, reservation: Reservation) {
        self.store.unreserve(reservation);
    }

    /// Stores an image in the buffer in use, taking what `reservation`
    /// holds toward its cost. See [`Store::add_image`].
    pub fn add_image(
        &mut self,
        image: NewImage,
        reservation: Option<Reservation>,
    ) -> Result<ImageKey, Refusal> {
        self.store.add_image(image, reservation, self.screens)
    }

    /// Frees an image of the buffer in use, as a picture that could not be
    /// placed is.
    pub fn drop_image(&mut self, image: ImageKey) {
        self.store.drop_image(image);
    }

    /// The image a kitty command names: by its id, or by its number.
    pub fn find_image(&mut self, id: u32, number: u32) -> Option<ImageKey> {
        self.store.find_image(id, number)
    }

    /// A command named an existing image: the use clock moves (D5).
    pub fn touch(&mut self, image: ImageKey) {
        self.store.touch(image);
    }

    /// The lowest kitty id no image of the buffer in use has.
    pub fn free_id(&mut self) -> u32 {
        self.store.free_id()
    }

    /// Places an image of the buffer in use on its screen, with the box's
    /// top left at screen cell `place.col`, `place.row`. See
    /// [`Store::place`].
    pub fn place(&mut self, image: ImageKey, place: Place) -> Result<PlacementKey, Refusal> {
        self.store.place(image, place, self.screens, self.cols)
    }

    /// The box of a placement of the buffer in use, in columns and rows,
    /// which the cursor rules after a placement read.
    pub fn extent(&self, placement: PlacementKey) -> Option<(u32, u32)> {
        let buffer = self.store.buffers.get(self.store.active())?;
        let p = buffer.placements.get(&placement)?;
        Some((p.cols, p.rows))
    }
}
