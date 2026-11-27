//! Each display's buffers: the new buffers a modeset stages, the pan's
//! tall buffers and the test commit that checks them, and the buffers a
//! display lets go, freed once their tokens come back.

use std::io;
use std::sync::Arc;

#[cfg(doc)]
use super::super::Buffers;
#[cfg(feature = "scroll-pan")]
use super::super::DisplayId;
use super::super::kms::Kms;
#[cfg(feature = "scroll-pan")]
use super::super::kms::Request;
use super::super::{Ages, lock};
use super::buffers::{Buffer, Source};
use super::output::Output;
use super::source::Clock;
use super::{Drm2d, drop_cursor};
#[cfg(feature = "scroll-pan")]
use super::{Staged, card_removed};

#[expect(private_bounds, reason = "the card is a Card outside the crate")]
impl<K: Kms, S: Source<K>> Drm2d<K, S> {
    /// New buffers for `o` where its mode needs them: as many as
    /// [`Buffers`] says, cleared, the size of its mode, or taller where the
    /// display makes tall buffers. `None` where its buffers already fit, so
    /// a display online keeps what it shows.
    pub(super) fn stage(
        &self,
        o: &Output<S::Memory>,
    ) -> io::Result<Option<Vec<Buffer<S::Memory>>>> {
        let count = self.buffers.count();
        if o.buffers.len() == count && self.buffer_size_matches(o) {
            return Ok(None);
        }
        let (w, h) = o.buffer_size();
        let mut made = Vec::with_capacity(count);
        for _ in 0..count {
            match self.source.create(&self.card, w, h) {
                Ok(b) => {
                    lock(&b.map).as_mut().fill(0);
                    made.push(b);
                }
                Err(e) => {
                    self.release(made);
                    return Err(e);
                }
            }
        }
        Ok(Some(made))
    }

    /// The new buffers of `o`, as [`Self::stage`] makes them, where the
    /// display makes tall buffers that fit the card. Tall buffers past the
    /// largest framebuffer the card makes are refused before any is made,
    /// and tall buffers the card fails to make are refused too. Either way
    /// the display then makes buffers the screen's size.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn stage_tall(
        &self,
        o: &mut Output<S::Memory>,
    ) -> io::Result<Option<Vec<Buffer<S::Memory>>>> {
        if let Some(t) = o.tall() {
            let (w, h) = t.buffer(o.size());
            let (most_w, most_h) = self.max_fb;
            if w > most_w || h > most_h {
                Self::refuse_pan(
                    o,
                    &format!(
                        "its buffers would be {w}×{h}, past the largest framebuffer the card makes, {most_w}×{most_h}"
                    ),
                );
            }
        }
        match self.stage(o) {
            Err(e) if o.tall().is_some() && !card_removed(&e) => {
                Self::refuse_pan(o, &format!("the card cannot make its buffers: {e}"));
                self.stage(o)
            }
            made => made,
        }
    }

    /// Gives up the tall buffers of `o` for good, until its owner asks for
    /// other ones, and logs why once.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn refuse_pan(o: &mut Output<S::Memory>, why: &str) {
        o.pan.refused = true;
        o.pan.proven = None;
        Self::log_pan_refused(o, why);
    }

    /// Logs why `o` gives up its tall buffers.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn log_pan_refused(o: &Output<S::Memory>, why: &str) {
        log::warn!(
            "Display {} cannot scroll by moving its window over a tall buffer: {why}. Its buffers stay the size of its screen.",
            o.name
        );
    }

    /// Tests the pan of each display in the modeset `req` that is about to
    /// show tall buffers whose pan is not yet proven: a test-only commit of
    /// `req` with that display's window at its furthest start. Returns each
    /// display tested, with why the card refused its pan or `None` where
    /// the card took it. Where the card refuses `req` without the pan too,
    /// the pan is not to blame, so that display is left out, and the
    /// modeset fails or takes by itself; where it fails, the modeset runs
    /// again with buffers the screen's size ([`Self::without_tall`]).
    #[cfg(feature = "scroll-pan")]
    pub(super) fn test_pans(
        &self,
        outputs: &[Output<S::Memory>],
        staged: &[Staged<S::Memory>],
        req: &Request,
    ) -> Vec<(DisplayId, Option<String>)> {
        let mut tested = Vec::new();
        for (o, (made, _)) in outputs.iter().zip(staged) {
            let Some(t) = o.tall() else {
                continue;
            };
            let shown = made.as_deref().unwrap_or(&o.buffers);
            let size = t.buffer(o.size());
            if shown.first().is_none_or(|b| b.size != size) || o.pan.proven == Some(size) {
                continue;
            }
            let mut test = req.clone();
            if let Err(e) = o.prepare_origin(&mut test, o.origin(t.ring)) {
                tested.push((o.id, Some(e.to_string())));
                continue;
            }
            match self.test_commit(&test, true) {
                Ok(()) => tested.push((o.id, None)),
                Err(e) if self.test_commit(req, true).is_ok() => {
                    let e = e.into_inner();
                    tested.push((
                        o.id,
                        Some(format!(
                            "the card refused a window that starts {} pixels into the buffer: {e}",
                            t.ring
                        )),
                    ));
                }
                Err(_) => {}
            }
        }
        tested
    }

    /// Whether the modeset is about to show `o` in tall buffers whose pan no
    /// test-only commit has proved: `made`, its new buffers, or else those
    /// it holds.
    #[cfg(feature = "scroll-pan")]
    pub(super) fn untested_tall(
        o: &Output<S::Memory>,
        made: Option<&Vec<Buffer<S::Memory>>>,
    ) -> bool {
        let Some(t) = o.tall() else {
            return false;
        };
        let size = t.buffer(o.size());
        let shown = made.map_or(o.buffers.as_slice(), Vec::as_slice);
        shown.first().is_some_and(|b| b.size == size) && o.pan.proven != Some(size)
    }

    /// The framebuffer a modeset puts on the primary plane: the first of
    /// the display's new buffers, or else the front buffer it shows.
    pub(super) fn staged_fb(o: &Output<S::Memory>, made: Option<&Vec<Buffer<S::Memory>>>) -> u32 {
        match made {
            Some(made) => made.first().map_or(0, |b| b.fb),
            None => o.buffers.get(o.front()).map_or(0, |b| b.fb),
        }
    }

    /// Gives `o` the new buffers of a modeset that took: the first is on
    /// screen, and none holds anything drawn. A token out for an old buffer
    /// comes back too late, and the old buffer stays until it does. A
    /// display that keeps its buffers at a new mode, as one whose refresh
    /// rate alone changed, is drawn whole too: its buffers are age 0 and it
    /// is told to redraw, so its owner reads the new rate.
    pub(super) fn install(&self, o: &mut Output<S::Memory>, made: Option<Vec<Buffer<S::Memory>>>) {
        let was = o.set.replace(o.mode);
        // Set to another mode than before, the display owes nothing.
        if was != Some(o.mode) {
            o.owed = None;
        }
        if let Some(made) = made {
            let old = std::mem::replace(&mut o.buffers, made);
            self.release(old);
            o.back = 1usize.checked_rem(o.buffers.len()).unwrap_or(0);
            o.ages = Ages::default();
            o.lease = o.lease.wrapping_add(1);
            o.clear_owed = false;
            o.fresh = false;
            o.redraw = true;
        } else if was.is_some_and(|was| was != o.mode) {
            o.ages = Ages::default();
            o.redraw = true;
        }
    }

    fn buffer_size_matches(&self, o: &Output<S::Memory>) -> bool {
        o.buffers.first().is_some_and(|b| b.size == o.buffer_size())
    }

    fn free_buffers(&self, o: &mut Output<S::Memory>) {
        self.release(std::mem::take(&mut o.buffers));
    }

    /// Frees `buffers`, keeping each whose memory a token holds until the
    /// token comes back, so the display never waits for the render thread.
    /// A card the kernel removed frees them with its file, so they are
    /// only unmapped, including the rest of them once freeing one finds
    /// the card removed.
    pub(super) fn release(&self, buffers: Vec<Buffer<S::Memory>>) {
        for b in buffers {
            if self.card.removed() {
                return;
            }
            if Arc::strong_count(&b.map) > 1 {
                self.retired.borrow_mut().push(b);
            } else {
                self.source.destroy(&self.card, b);
            }
        }
    }

    /// Frees the buffers let go of whose tokens have come back.
    pub(super) fn reap(&self) {
        let back: Vec<Buffer<S::Memory>> = {
            let mut retired = self.retired.borrow_mut();
            let (free, kept) = std::mem::take(&mut *retired)
                .into_iter()
                .partition(|b| Arc::strong_count(&b.map) == 1);
            *retired = kept;
            free
        };
        for b in back {
            self.source.destroy(&self.card, b);
        }
    }

    /// Frees display `o`'s buffers, cursor and mode. Where a flip of its
    /// own is in flight, the CRTC it was committed on owes an event that no
    /// display waits for, bounded by a timer of its own ([`Self::stale`]).
    pub(super) fn free(&mut self, mut o: Output<S::Memory>) {
        if let (Some(link), Some((t, _))) = (&self.link, o.flip_timer.take()) {
            link.disarm(t);
        }
        if o.flipping {
            let timer = self
                .link
                .as_ref()
                .and_then(|l| l.arm(Clock::Stale(o.flip_crtc)));
            self.stale.push((o.flip_crtc, timer));
        }
        self.free_buffers(&mut o);
        drop_cursor(&self.card, &mut o);
        if o.mode_blob != 0 && !self.card.removed() {
            let _ = self.card.destroy_blob(o.mode_blob);
        }
    }
}
