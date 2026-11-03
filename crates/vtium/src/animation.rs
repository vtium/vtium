//! The animation clock (ADR-1102 D3, D4): the one place time enters kitty
//! animation, and animated GIFs, which the image store keeps the same way.
//!
//! The image store records only what the commands set: each image's
//! frames and their gaps, whether it runs, the loops it was asked for, the
//! frame a command last made current, and two stamps that move when a
//! command changes the current frame or the run state. The clock reads
//! those and picks the frame each animated image of its console shows. It
//! keeps that choice outside the store, so the store, the replies and a
//! replay never depend on time.
//!
//! Each console's terminal has a clock. The loop gives it the time at each
//! turn ([`AnimationClock::turn`]); the clock answers the commands that
//! moved a stamp, as D3's table says, and on its timer moves each running
//! animation on by its frames' gaps. When a head's snapshot is taken, the
//! clock puts the frame it chose on each placement of an image whose entry
//! shows another frame ([`AnimationClock::show`]), asking the store for
//! that frame's pixels once per image. A turn that moves a frame asks the
//! head to draw; the renderer sees the new frame's number and pixel
//! revision and draws the cells under the placements again.
//!
//! The clock arms its timer for the soonest frame change, at most once
//! every [`ANIMATION_FLOOR_MS`], and arms none while nothing plays, while
//! its console is hidden, or while the VT is another's or the displays are
//! blanked. When its console is shown again, the clock works out from the
//! time where each animation has got to and shows that frame, rather than
//! playing the frames it missed.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use vtium_image::{Animation, FramePixels, FramePlacement, ImageKey, Run, Store};

/// The shortest time between two firings of the clock's timer, in
/// milliseconds (ADR-1102 D3). A frame whose gap is shorter shows at the
/// next firing, or is passed over when its time has already gone.
pub const ANIMATION_FLOOR_MS: u64 = 10;

const FLOOR: Duration = Duration::from_millis(ANIMATION_FLOOR_MS);

/// What the clock reads of the image store: an image's animation, its
/// frames' gaps and a frame's whole pixels.
pub trait Frames {
    fn animation(&self, image: ImageKey) -> Option<Animation>;
    /// Frame `frame`'s gap in milliseconds, negative for a gapless frame.
    fn gap(&self, image: ImageKey, frame: u32) -> Option<i32>;
    /// Frame `frame`'s whole pixels, composed by the store when it must.
    fn pixels(&self, image: ImageKey, frame: u32) -> Option<FramePixels>;
}

impl Frames for Store {
    fn animation(&self, image: ImageKey) -> Option<Animation> {
        Store::animation(self, image)
    }

    fn gap(&self, image: ImageKey, frame: u32) -> Option<i32> {
        self.frame_gap(image, frame)
    }

    fn pixels(&self, image: ImageKey, frame: u32) -> Option<FramePixels> {
        self.frame_pixels(image, frame)
    }
}

/// What the clock plays of one animated image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Play {
    /// The frame on show, from 1.
    frame: u32,
    /// When that frame began: the time it was due, which can be earlier
    /// than the turn that moved to it.
    since: Instant,
    /// The loops played since the animation last started or stopped.
    played: u32,
    /// How the clock plays it: the store's run state as the clock last
    /// answered it.
    run: Run,
    /// The animation as the store recorded it at the last turn: its frame
    /// count, loops and stamps.
    seen: Animation,
    /// The last step was blocked: a running animation waits on its last
    /// frame for more, has played its loops, or has only gapless frames.
    /// The step that ends a wait starts its frame at that turn, so the
    /// time spent waiting plays no frames.
    held: bool,
}

impl Play {
    /// An animation the clock meets at `now`: it shows the store's current
    /// frame, playing or stopped as the store says.
    fn start(seen: Animation, now: Instant) -> Play {
        Play {
            frame: seen.current.max(1),
            since: now,
            played: 0,
            run: seen.run,
            seen,
            held: false,
        }
    }

    /// The gap of frame `frame` as a time: 0 for a gapless frame.
    fn gap(frames: &impl Frames, key: ImageKey, frame: u32) -> u64 {
        frames
            .gap(key, frame)
            .and_then(|g| u64::try_from(g).ok())
            .unwrap_or(0)
    }

    /// The next frame to show after the one on show, passing over gapless
    /// frames, and whether it goes back to the first: `None` while the
    /// step is blocked (D4). A looping animation goes back to frame 1
    /// after its last unless that would pass its loops: `v` asks for `v`
    /// − 1 loops, 1 for no end. A loading one waits on its last frame.
    fn step(&self, frames: &impl Frames, key: ImageKey) -> Option<(u32, bool)> {
        let count = self.seen.frames.max(1);
        let mut at = self.frame;
        let mut wrapped = false;
        for _ in 0..count {
            if at >= count {
                if self.run != Run::Looping || wrapped {
                    return None;
                }
                let limit = self.seen.loops.saturating_sub(1);
                if limit > 0 && self.played.saturating_add(1) >= limit {
                    return None;
                }
                at = 1;
                wrapped = true;
            } else {
                at = at.saturating_add(1);
            }
            if Play::gap(frames, key, at) > 0 {
                return Some((at, wrapped));
            }
        }
        None
    }

    /// When the frame on show next changes, while the animation runs and
    /// its next step is not blocked.
    fn next(&self, frames: &impl Frames, key: ImageKey) -> Option<Instant> {
        if self.run == Run::Stopped {
            return None;
        }
        self.step(frames, key)?;
        Some(after(self.since, Play::gap(frames, key, self.frame)))
    }

    /// Moves the animation on to the frame the time `now` puts it on.
    /// Returns whether the frame on show changed.
    fn advance(&mut self, frames: &impl Frames, key: ImageKey, now: Instant) -> bool {
        if self.run == Run::Stopped {
            return false;
        }
        let from = self.frame;
        self.skip_loops(frames, key, now);
        // After the whole loops are skipped, at most two loops are left to
        // walk, a step at a time.
        let bound = usize::try_from(self.seen.frames)
            .unwrap_or(usize::MAX)
            .saturating_mul(2)
            .saturating_add(2);
        for _ in 0..bound {
            let due = after(self.since, Play::gap(frames, key, self.frame));
            if now < due {
                return self.frame != from;
            }
            let Some((next, wrapped)) = self.step(frames, key) else {
                self.held = true;
                return self.frame != from;
            };
            self.since = if self.held { due.max(now) } else { due };
            self.held = false;
            self.frame = next;
            if wrapped {
                self.played = self.played.saturating_add(1);
            }
        }
        // Not reached with the loops skipped; a bound for the walk alone.
        self.since = now;
        self.frame != from
    }

    /// Skips the whole loops a looping animation played by `now`, keeping
    /// the last one or two to walk, within the loops it may still play:
    /// one loop from a frame that is not gapless takes the sum of the
    /// frames' gaps and comes back to that frame.
    fn skip_loops(&mut self, frames: &impl Frames, key: ImageKey, now: Instant) {
        if self.run != Run::Looping || self.held || Play::gap(frames, key, self.frame) == 0 {
            return;
        }
        let cycle = (1..=self.seen.frames)
            .map(|f| Play::gap(frames, key, f))
            .fold(0u64, u64::saturating_add);
        let elapsed = millis(now.saturating_duration_since(self.since));
        let Some(loops) = elapsed.checked_div(cycle) else {
            return;
        };
        let mut skip = loops.saturating_sub(1);
        let limit = self.seen.loops.saturating_sub(1);
        if limit > 0 {
            let left = limit.saturating_sub(1).saturating_sub(self.played);
            skip = skip.min(u64::from(left));
        }
        if skip == 0 {
            return;
        }
        self.since = after(self.since, skip.saturating_mul(cycle));
        self.played = self
            .played
            .saturating_add(u32::try_from(skip).unwrap_or(u32::MAX));
    }

    /// Answers what the store records now, `seen`, at `now`, by D3's
    /// table, after playing up to `now` as before the command. Returns
    /// whether the frame on show changed.
    fn answer(
        &mut self,
        seen: Animation,
        frames: &impl Frames,
        key: ImageKey,
        now: Instant,
    ) -> bool {
        let frame_moved = seen.frame_stamp != self.seen.frame_stamp;
        let run_moved = seen.run_stamp != self.seen.run_stamp;
        if !frame_moved && !run_moved && seen == self.seen {
            return false;
        }
        let from = self.frame;
        self.advance(frames, key, now);
        if frame_moved {
            // `a=a` with `c`, or the frames deleted or freed: the store's
            // current frame from now.
            self.frame = seen.current.max(1);
            self.since = now;
            self.held = false;
        }
        if run_moved {
            match (seen.run, self.run) {
                // `s=1`: it stops on the frame on show, and stopping sets
                // the loops played back.
                (Run::Stopped, _) => {
                    self.run = Run::Stopped;
                    self.played = 0;
                }
                // `s=2` or `s=3` while stopped: it runs from the frame on
                // show.
                (run, Run::Stopped) => {
                    self.run = run;
                    self.since = now;
                    self.held = false;
                }
                // While running, and `v` alone: it keeps its place and the
                // loops played, and takes the new mode or limit.
                (run, _) => self.run = run,
            }
        }
        self.seen = seen;
        if self.frame > seen.frames {
            self.frame = seen.current.max(1);
        }
        // A frame that arrived ends a loading wait, a larger `v` a played
        // out loop, and a gapless frame made current moves on at once.
        self.advance(frames, key, now);
        self.frame != from
    }
}

/// A console's animation clock: what it plays of each image the head's
/// last snapshot of the console listed.
#[derive(Clone, Debug, Default)]
pub struct AnimationClock {
    /// Each image the last snapshot listed, with what the clock plays of
    /// it once a command gave it an animation.
    images: BTreeMap<ImageKey, Option<Play>>,
    /// The last turn's time: where an animation the snapshot finds starts.
    now: Option<Instant>,
    /// When the timer last fired.
    fired: Option<Instant>,
    /// The console shows on a display that is lit, the VT being ours.
    shown: bool,
}

impl AnimationClock {
    /// A turn of the loop at `now`, the console `shown` or not: the clock
    /// answers each command a stamp records, then, when its timer is due
    /// and the console shows, moves each running animation on. Returns
    /// whether a frame on show changed while the console shows, when the
    /// head draws.
    pub fn turn(&mut self, frames: &impl Frames, now: Instant, shown: bool) -> bool {
        self.now = Some(now);
        self.shown = shown;
        let mut changed = false;
        for (&key, slot) in &mut self.images {
            match (slot.as_mut(), frames.animation(key)) {
                (None, None) => {}
                (None, Some(seen)) => *slot = Some(Play::start(seen, now)),
                // The image lost its animation, as when it was replaced:
                // its entries show what the store gives them.
                (Some(_), None) => *slot = None,
                (Some(play), Some(seen)) => changed |= play.answer(seen, frames, key, now),
            }
        }
        if self.due(frames).is_some_and(|due| due <= now) {
            for (&key, play) in &mut self.images {
                if let Some(play) = play {
                    changed |= play.advance(frames, key, now);
                }
            }
            self.fired = Some(now);
        }
        changed && shown
    }

    /// When the timer fires next: the soonest frame change among the
    /// running animations, no sooner than [`ANIMATION_FLOOR_MS`] after the
    /// last firing; `None` while nothing plays or the console does not
    /// show.
    pub fn due(&self, frames: &impl Frames) -> Option<Instant> {
        if !self.shown {
            return None;
        }
        let soonest = self
            .images
            .iter()
            .filter_map(|(&key, play)| play.as_ref()?.next(frames, key))
            .min()?;
        Some(match self.fired {
            Some(fired) => soonest.max(fired.checked_add(FLOOR).unwrap_or(fired)),
            None => soonest,
        })
    }

    /// The frame the clock shows of `image`, while it plays that image.
    pub fn frame(&self, image: ImageKey) -> Option<u32> {
        self.images.get(&image)?.as_ref().map(|p| p.frame)
    }

    /// A head's snapshot of the console lists `placements`: the clock plays
    /// the images they show from now on, and drops the others, and puts the
    /// frame it chose on each placement whose entry shows another, with
    /// its pixels asked of the store once per image.
    pub fn show(&mut self, placements: &mut [FramePlacement], frames: &impl Frames) {
        let listed: BTreeSet<ImageKey> = placements.iter().map(|p| p.image).collect();
        self.images.retain(|key, _| listed.contains(key));
        for key in listed {
            let now = self.now;
            self.images.entry(key).or_insert_with(|| {
                let seen = frames.animation(key)?;
                Some(Play::start(seen, now?))
            });
        }
        let mut chosen: BTreeMap<ImageKey, Option<FramePixels>> = BTreeMap::new();
        for p in placements {
            let Some(frame) = self.frame(p.image).filter(|&f| f != p.frame) else {
                continue;
            };
            let pixels = chosen
                .entry(p.image)
                .or_insert_with(|| frames.pixels(p.image, frame));
            if let Some(pixels) = pixels {
                p.show(pixels.clone());
            }
        }
    }
}

/// `t` and `ms` milliseconds.
fn after(t: Instant, ms: u64) -> Instant {
    t.checked_add(Duration::from_millis(ms)).unwrap_or(t)
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
