//! The store's side of an iTerm2 picture's animated GIF (ADR-1103, D2): the
//! room the frame allowance could give its frames, asked before the helper
//! decodes them, and a reservation of the allowance made before the image
//! exists, so every frame is reserved before any of its pixels is copied.

use super::anim::Anim;
use super::{Edit, Reservation, Store};
use crate::image::{ImageKey, Refusal};

impl Store {
    /// The most bytes the frame allowance could give an animation to come:
    /// what it has not granted, and what freeing the frames of every other
    /// animation would give back (ADR-1102, D1).
    pub(crate) fn frame_room(&self) -> u64 {
        let freeable = self
            .buffers
            .iter()
            .flat_map(|b| b.images.values())
            .map(Anim::charge_of)
            .fold(0u64, u64::saturating_add);
        self.frame_limit()
            .saturating_sub(self.frame_used.saturating_sub(freeable))
    }

    /// Reserves `bytes` of the frame allowance for the frames of an image
    /// not stored yet, freeing other animations' frames as a frame command
    /// does, or refuses with ENOSPC and frees nothing.
    pub(super) fn frames_ahead(&mut self, bytes: u64) -> Result<Reservation, Refusal> {
        let b = self.active();
        // no image has this key, so every animation's frames may go
        self.reserve_frames(bytes, (b, ImageKey(u64::MAX)))?;
        let mut r = self.grant_frames(bytes, ImageKey(u64::MAX), b);
        r.old = None;
        Ok(r)
    }

    /// Splits `ahead` into `count` frame reservations of `each` bytes for
    /// image `key` of the buffer in use, giving back what is left over;
    /// `None`, with everything given back, when it holds too little or was
    /// already spent.
    pub(super) fn split_frames(
        &mut self,
        ahead: Reservation,
        key: ImageKey,
        each: u64,
        count: usize,
    ) -> Option<Vec<Reservation>> {
        let b = self.active();
        let held = self.spend(&ahead)?;
        let need = each.saturating_mul(u64::try_from(count).unwrap_or(u64::MAX));
        if need > held || self.image(b, key).is_none() {
            self.frame_used = self.frame_used.saturating_sub(held);
            return None;
        }
        self.frame_used = self.frame_used.saturating_sub(held.saturating_sub(need));
        Some(
            (0..count)
                .map(|_| self.grant_frames(each, key, b))
                .collect(),
        )
    }
}

impl Edit<'_> {
    /// Reserves `bytes` of the frame allowance for the frames of an image
    /// not stored yet; [`unreserve`](Self::unreserve) gives it back.
    pub(crate) fn frames_ahead(&mut self, bytes: u64) -> Result<Reservation, Refusal> {
        self.store.frames_ahead(bytes)
    }

    /// Turns a reservation [`frames_ahead`](Self::frames_ahead) made into
    /// `count` frame reservations of `each` bytes for image `key`, each for
    /// one [`add_frame`](Self::add_frame).
    pub(crate) fn split_frames(
        &mut self,
        ahead: Reservation,
        key: ImageKey,
        each: u64,
        count: usize,
    ) -> Option<Vec<Reservation>> {
        self.store.split_frames(ahead, key, each, count)
    }
}
