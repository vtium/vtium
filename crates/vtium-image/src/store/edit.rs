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
//! need. The fields are visible only under `store`, so a handler outside
//! it never reaches the store itself.
//!
//! The terminal makes an `Edit` for each call of
//! [`Context::edit_store`](crate::Context::edit_store) and, once the
//! handler's function returns, settles the store and marks the cells under
//! every box it changed, as it does after a screen event.

use super::{Compose, Control, Link, NewFrame, Placing, Screens, Store};
use crate::image::{ImageKey, NewImage, Place, PlacementKey, Refusal};
use crate::rules::Protocol;
use crate::store::Reservation;

/// The store, with the screen's rows, for one edit.
#[derive(Debug)]
pub struct Edit<'a> {
    pub(super) store: &'a mut Store,
    pub(super) screens: Screens<'a>,
    pub(super) cols: usize,
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

    /// Places an image of the buffer in use as [`place`](Self::place) does,
    /// the source drawn at exactly `width` by `height` pixels from the box's
    /// top left and the box covering the cells the picture reaches, as an
    /// iTerm2 picture's keys size it (ADR-1103, D3). The place's box in
    /// cells is ignored.
    pub(crate) fn place_drawn(
        &mut self,
        image: ImageKey,
        place: Place,
        (width, height): (u32, u32),
    ) -> Result<PlacementKey, Refusal> {
        let link = Link::Drawn { width, height };
        self.store
            .place_at(image, place, link, self.screens, self.cols)
            .map_err(Placing::refusal)
    }

    /// The buffer of the image the store freed last, to fill again, when it
    /// holds `len` pixels and no frame still holds it, with its charge as a
    /// reservation for an image of `protocol` with kitty id `id` (0 for
    /// none): a handler that receives frame after frame of one size, as
    /// kitty's video does, takes it in place of a new allocation, and the
    /// kernel neither maps nor zeroes new pages for it. Its contents are the
    /// old image's. The reservation holds the buffer's bytes, which the
    /// spare held of the budget, so a handler grows it by what the image
    /// needs beyond them, and no reservation of its own can evict the
    /// buffer it is about to fill; like [`reserve`](Self::reserve), it
    /// holds an image of that id, which a replacement frees.
    pub fn take_spare(
        &mut self,
        protocol: Protocol,
        id: u32,
        len: usize,
    ) -> Option<(Vec<u32>, Reservation)> {
        if self.store.spare.as_ref()?.as_slice().len() != len {
            return None;
        }
        let bytes = self.store.spare_bytes();
        let spare = self.store.spare.take()?;
        let buffer = match spare.into_vec() {
            Ok(buffer) => buffer,
            Err(held) => {
                self.store.spare = Some(held);
                return None;
            }
        };
        Some((buffer, self.store.spare_reservation(protocol, id, bytes)))
    }

    /// Reserves `bytes` of the frame allowance for a frame's data on
    /// `image`, before the data is allocated, holding the image until the
    /// reservation is spent (ADR-1102, D1).
    pub(crate) fn frame_reserve(
        &mut self,
        image: ImageKey,
        bytes: u64,
    ) -> Result<Reservation, Refusal> {
        self.store.frame_reserve(image, bytes)
    }

    /// Stores a frame's data, `a=f`, on the image `reservation` was made
    /// for, and returns the frame's number.
    pub(crate) fn add_frame(
        &mut self,
        frame: NewFrame,
        reservation: Reservation,
    ) -> Result<u32, Refusal> {
        self.store.add_frame(frame, reservation)
    }

    /// Marks `image` as one that arrived as palette indices, which takes no
    /// frames and no composition (ADR-1100, D15).
    pub(crate) fn set_indexed(&mut self, image: ImageKey) {
        self.store.set_indexed(image);
    }

    /// A composition of frames, `a=c`.
    pub(crate) fn compose_frames(&mut self, image: ImageKey, c: Compose) -> Result<(), Refusal> {
        self.store.compose_frames(image, c)
    }

    /// An animation control, `a=a`.
    pub(crate) fn control_animation(&mut self, image: ImageKey, c: Control) -> Result<(), Refusal> {
        self.store.control_animation(image, c)
    }

    /// `d=f` and `d=F`: every frame of `image` but its root goes.
    pub(crate) fn delete_frames(&mut self, image: ImageKey) {
        self.store.delete_frames(image);
    }

    /// The box of a placement of the buffer in use, in columns and rows,
    /// which the cursor rules after a placement read.
    pub fn extent(&self, placement: PlacementKey) -> Option<(u32, u32)> {
        let buffer = self.store.buffers.get(self.store.active())?;
        let p = buffer.placements.get(&placement)?;
        Some((p.cols, p.rows))
    }
}
