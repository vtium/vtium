//! A display's frames: the token that lends the render thread the buffer
//! the next frame is drawn into, the token given back unused, and the
//! swap or commit that shows the drawn buffer with a non-blocking commit.

use std::sync::Arc;

use drm_ffi::{DRM_MODE_ATOMIC_NONBLOCK, DRM_MODE_PAGE_FLIP_EVENT};
use rustix::io::Errno;

use super::super::kms::{Kms, Request};
use super::super::{DisplayId, Loan, Memory, Rect, SwapError, Swapped, Token, lock};
use super::buffers::Source;
use super::output::{Output, clip_bytes};
use super::{Drm2d, card_removed, drop_cursor, errno};

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// The prepare step: lends the buffer the next frame is drawn into, as
    /// a token with its age, while the display is ready for a frame
    /// ([`Drm2d::ready_for_frame`]). While the token is out the display
    /// lends no other. Give it back with [`Drm2d::swap`], or unused with
    /// [`Drm2d::give_back`]; a token dropped instead leaves every buffer of
    /// the display at age 0.
    pub fn lend(&mut self, id: DisplayId) -> Option<Token> {
        self.reap();
        if let Some(o) = self.output_mut(id)
            && o.loan.as_ref().is_some_and(|l| !l.out())
        {
            o.loan = None;
            o.forget();
        }
        if !self.ready_for_frame(id) {
            return None;
        }
        let o = self.output_mut(id)?;
        let buffer = o.buffers.get(o.back)?;
        let memory: Arc<Memory> = buffer.map.clone();
        let (token, loan) = Loan::lend(
            id,
            o.back,
            o.ages.age(o.back),
            o.lease,
            buffer.size,
            buffer.pitch,
            memory,
        );
        #[cfg(feature = "scroll-pan")]
        let token = match o.tall().filter(|_| o.holds_tall()) {
            Some(t) => token.showing(o.size(), o.pan.window, t.turned),
            None => token,
        };
        o.loan = Some(loan);
        Some(token)
    }

    /// Takes back a token with nothing drawn into its buffer, as one the
    /// owner could not send to the render thread: the buffer holds what it
    /// held, and nothing is presented.
    pub fn give_back(&mut self, token: Token) {
        if let Some(o) = self.outputs.iter_mut().find(|o| o.id == token.id())
            && o.loan.as_ref().is_some_and(|l| l.lent(&token))
        {
            o.loan = None;
            if o.clear_owed {
                o.clear_owed = false;
                if let Some(memory) = token.memory() {
                    lock(memory).as_mut().fill(0);
                }
            }
        }
        drop(token);
        self.reap();
    }

    /// The swap: takes back the token lent for this frame and presents what
    /// changed, the frame drawn into its buffer with its `damage`, or else
    /// the cursor plane's state alone, keeping the front buffer on screen,
    /// or else nothing. A rectangle covering the screen is the whole
    /// screen. A failed swap does not count as presented: its error
    /// reaches the caller, and the frame, its damage and the cursor's state
    /// go with the next swap.
    ///
    /// A token that comes back too late, to a display that has gone, has
    /// been given new buffers, or was let go for a VT release or powered
    /// down meanwhile, is dropped with [`SwapError::Dropped`]: its frame is
    /// not shown, and every buffer of the display is age 0, so its next
    /// frame is drawn whole.
    ///
    /// On a display with tall buffers, the swap shows the window the token
    /// carries. A window that starts past the display's ring is clamped to
    /// the ring's end, the furthest window that lies inside the buffer.
    pub fn swap(&mut self, token: Token, damage: &[Rect]) -> Result<Swapped, SwapError> {
        let id = token.id();
        let Some(o) = self.outputs.iter_mut().find(|o| o.id == id && o.announced) else {
            drop(token);
            self.reap();
            return Err(SwapError::Dropped);
        };
        let Some(loan) = o.loan.take_if(|l| l.lent(&token)) else {
            return Err(SwapError::Dropped);
        };
        if !loan.current(&token, o.lease) {
            // The buffer holds part of a frame nobody counts, over any frame
            // kept from a failed swap: it goes whole.
            o.forget();
            o.damage.set_whole();
            if std::mem::take(&mut o.clear_owed)
                && let Some(memory) = token.memory()
            {
                lock(memory).as_mut().fill(0);
            }
            drop(token);
            self.reap();
            return Err(SwapError::Dropped);
        }
        // A frame that moves the window is presented even where nothing
        // was drawn, since the screen shows another part of the buffer. The
        // display bounds the window by its own ring, whatever the token
        // says, so the plane never reads past the buffer's end.
        #[cfg(feature = "scroll-pan")]
        let moved = match o.tall().filter(|_| o.holds_tall()) {
            Some(t) => {
                let window = token.window().min(t.ring);
                o.pan.window = window;
                window != o.pan.shown
            }
            None => false,
        };
        #[cfg(not(feature = "scroll-pan"))]
        let moved = false;
        drop(token);
        // The frame drawn into the token's buffer is kept whatever the
        // swap's outcome: the back buffer holds it, so it counts as a frame.
        // Its damage is in the buffer's coordinates, which on a tall buffer
        // reach past the screen.
        if !damage.is_empty() || moved {
            let (w, h) = o.buffer_size();
            o.damage.add(damage, w, h);
            o.ages.drawn(o.back);
            o.fresh = true;
        }
        self.reap();
        self.send(id)
    }

    /// Presents what changed with no new frame: a frame kept from a failed
    /// swap, or else the cursor plane's state alone, keeping the front
    /// buffer on screen, or else nothing. While a token is out, its frame
    /// carries the frame kept, and this sends the cursor plane alone.
    pub fn commit(&mut self, id: DisplayId) -> Result<Swapped, SwapError> {
        self.send(id)
    }

    fn send(&mut self, id: DisplayId) -> Result<Swapped, SwapError> {
        let (awake, waiting) = (self.awake, self.waiting.is_some());
        let (legacy, hotspot) = (self.legacy, self.hotspot);
        let Some(o) = self.outputs.iter_mut().find(|o| o.id == id && o.announced) else {
            return Err(SwapError::Off);
        };
        if !awake || !o.online || !o.dpms_on() {
            return Err(SwapError::Off);
        }
        if waiting || o.flipping {
            return Err(SwapError::Busy);
        }
        let swapped = if o.fresh && !o.lent() {
            Swapped::Frame
        } else if !legacy && o.cursor_changed() {
            Swapped::Cursor
        } else {
            return Ok(Swapped::Nothing);
        };
        let sent = if legacy {
            self.card.page_flip(o.crtc, o.back_fb())
        } else {
            let clips = match o.damage.clips() {
                Some(rects)
                    if swapped == Swapped::Frame && o.planes.as_ref().is_some_and(|p| p.damage) =>
                {
                    match self.card.create_blob(&clip_bytes(rects)) {
                        Ok(blob) => Some(blob),
                        // The commit after it fails the same way.
                        Err(e) if card_removed(&e) => None,
                        Err(e) => {
                            log::warn!("cannot create the damage blob: {e}");
                            None
                        }
                    }
                }
                _ => None,
            };
            let card = &self.card;
            let commit = |o: &Output<S::Memory>| {
                let mut req = Request::default();
                // A cursor-only commit leaves the primary plane as it is,
                // so the kernel sees no damage to it.
                match swapped {
                    // The damage clips are in the framebuffer's coordinates.
                    // A commit that moves the source rectangle updates the
                    // whole plane whatever its clips say
                    // (`drm_atomic_helper_damage_iter_init`).
                    Swapped::Frame => {
                        let origin = o.frame_origin();
                        o.prepare(&mut req, o.back_fb(), origin, o.mode_blob, clips, hotspot)?;
                    }
                    _ => o.prepare_cursor(&mut req, hotspot)?,
                }
                card.commit(&req, DRM_MODE_PAGE_FLIP_EVENT | DRM_MODE_ATOMIC_NONBLOCK)
            };
            let mut sent = commit(o);
            // A commit showing the cursor that failed is tried without it.
            // When that takes, the cursor plane is what failed: it is given
            // up, and the owner draws the pointer itself.
            let mut nothing = false;
            if let Err(e) = &sent
                && errno(e) != Some(Errno::BUSY)
                && !card_removed(e)
                && o.cursor_shown().is_some()
            {
                if let Some(c) = o.cursor.as_mut() {
                    c.visible = false;
                }
                // with the plane off already, a cursor-only commit has
                // nothing left to send
                nothing = swapped == Swapped::Cursor && o.cursor_set.is_none();
                let again = if nothing { Ok(()) } else { commit(o) };
                if again.is_ok() {
                    log::warn!(
                        "the cursor plane of {} failed a commit, so the pointer is drawn in software: {e}",
                        o.name
                    );
                    drop_cursor(card, o);
                    sent = Ok(());
                } else {
                    nothing = false;
                    if let Some(c) = o.cursor.as_mut() {
                        c.visible = true;
                    }
                }
            }
            if let Some(blob) = clips
                && let Err(e) = card.destroy_blob(blob)
            {
                log::warn!("cannot destroy the damage blob: {e}");
            }
            if nothing {
                return Ok(Swapped::Nothing);
            }
            sent
        };
        if let Err(e) = sent {
            if card_removed(&e) {
                // The card is gone with every display on it, and the owner
                // hears each removed.
                self.settle();
                return Err(SwapError::Off);
            }
            if errno(&e) == Some(Errno::BUSY) {
                // Another commit holds the CRTC while vtium knows of no flip
                // of its own in flight: a commit vtium does not track, or a
                // flip of vtium's that the bound took as done while the
                // kernel still had it. When the first lands, the kernel sends
                // vtium no event. The display's next vblank stands in for
                // that event: it is waited for as a flip in flight is, and
                // the owner draws again after it.
                match self.card.wait_vblank(o.pipe) {
                    Ok(()) => {
                        o.start_flip();
                        o.vblank = true;
                    }
                    // A card with no vblank interrupt and no software vblank
                    // timer refuses the wait with EOPNOTSUPP, and nothing
                    // then asks for the frame again
                    // until the next draw (vtium/vtium#176).
                    Err(v) => log::debug!("cannot wait for the next vblank of {}: {v}", o.name),
                }
                self.settle();
            } else {
                log::warn!("cannot flip {}: {e}", o.name);
            }
            return Err(e.into());
        }
        o.start_flip();
        if swapped == Swapped::Frame {
            o.back = o.after_back();
            o.damage.clear();
            o.fresh = false;
            #[cfg(feature = "scroll-pan")]
            {
                o.pan.shown = o.pan.window;
            }
        }
        o.cursor_committed();
        self.settle();
        Ok(swapped)
    }
}
