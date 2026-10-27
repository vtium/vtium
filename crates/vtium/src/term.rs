//! The terminal of the seat's one session: the terminal state the child's
//! output drives, when it presents (mode 2026 and coalescing), the blink
//! phases, and a [`View`] per display that draws it.

use std::process::ExitStatus;
use std::rc::Rc;
use std::time::Instant;

use vtium_font::{Face, Font};
use vtium_platform::display::{DisplayId, Rect as DisplayRect, SwapError, Swapped};
use vtium_platform::pty::{Consumer, Replies};
use vtium_render::{Config as RenderConfig, DrawState, Mapped, RenderTimer, Renderer};
use vtium_term::{Age, Terminal};

use crate::blink::Blink;
use crate::sync::{Action, Coalesce, Sync, Wait};
use crate::video::Video;

/// The clock the terminal reads while it parses: the loop's, or a test's.
pub type Clock = Rc<dyn Fn() -> Instant>;

/// The session's terminal and the rules for when it presents.
pub struct Term {
    pub terminal: Terminal,
    pub sync: Sync,
    pub coalesce: Coalesce,
    pub blink: Blink,
    clock: Clock,
    /// The pty holds output the last input call left.
    pub unparsed: bool,
    /// Output changed the screen this turn.
    pub changed: bool,
    /// A frame end or a timeout asks every display to present.
    pub present: bool,
    /// The child exited and its output has been read.
    pub exited: Option<ExitStatus>,
}

impl Term {
    pub fn new(terminal: Terminal, sync: Sync, blink: Blink, clock: Clock) -> Term {
        Term {
            terminal,
            sync,
            coalesce: Coalesce::default(),
            blink,
            clock,
            unparsed: false,
            changed: false,
            present: false,
            exited: None,
        }
    }

    /// What a hold's rules asked for.
    pub fn act(&mut self, action: Action) {
        match action {
            Action::Present | Action::Expired => self.present = true,
            Action::None | Action::Arm => {}
        }
    }
}

impl Consumer for Term {
    fn output(&mut self, output: &[u8], replies: &mut Replies<'_>) -> usize {
        let start = (self.clock)();
        let taken = self.terminal.input(output);
        let out = self.terminal.take_replies();
        if !out.is_empty() {
            replies.write(&out);
        }
        let end = (self.clock)();
        self.coalesce.input(start, end, taken);
        self.unparsed = taken < output.len();
        self.changed |= taken > 0;
        let action = self.sync.input(&mut self.terminal, end, self.unparsed);
        self.act(action);
        taken
    }

    fn exited(&mut self, status: ExitStatus) {
        self.exited = Some(status);
    }
}

/// Why a display draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The child's output changed the screen: coalescing may wait.
    Output,
    /// A key, the blink, the pointer or a frame end: at once.
    Now,
}

/// One display drawing the terminal.
pub struct View {
    pub id: DisplayId,
    pub renderer: Renderer<Box<dyn Face>>,
    /// The terminal's checkpoint after this view's last draw.
    since: Age,
    pub wait: Wait,
    /// A draw waits for the flip in flight.
    pub pending: bool,
    /// A draw waits for the hold's end.
    pub owed: bool,
    /// The display lost its picture and draws whole, through a hold.
    pub lost: bool,
    pub timer: Option<RenderTimer>,
    pub size: (u32, u32),
    pub refresh: Option<std::time::Duration>,
}

impl View {
    pub fn new(
        id: DisplayId,
        font: Font<Box<dyn Face>>,
        size: (u32, u32),
        refresh_mhz: Option<u32>,
        config: RenderConfig,
        measure: bool,
    ) -> View {
        View {
            id,
            renderer: Renderer::new(font, size.0, size.1, config),
            since: Age::NEVER,
            wait: Wait::default(),
            pending: false,
            owed: false,
            lost: true,
            timer: measure.then(RenderTimer::new),
            size,
            refresh: refresh_mhz
                .filter(|&m| m > 0)
                .and_then(|m| 1_000_000_000_000_u64.checked_div(u64::from(m)))
                .map(std::time::Duration::from_nanos),
        }
    }

    /// Draws `term` into `video`'s canvas and presents it, unless a hold
    /// keeps the last frame or a flip is in flight. Returns whether a
    /// frame was queued.
    pub fn draw(
        &mut self,
        term: &mut Term,
        video: &mut dyn Video,
        state: &DrawState,
        clock: &dyn Fn() -> Instant,
    ) -> bool {
        if term.sync.holding(&term.terminal) && !self.lost {
            self.owed = true;
            return false;
        }
        if video.is_flipping(self.id) {
            self.pending = true;
            return false;
        }
        let start = clock();
        let Some(canvas) = video.canvas(self.id) else {
            return false;
        };
        let (width, height, stride, age) = (canvas.width, canvas.height, canvas.stride, canvas.age);
        let stride = usize::try_from(stride).unwrap_or(0);
        let Some(mut target) = Mapped::new(canvas.pixels, width, height, stride) else {
            return false;
        };
        let palette = term.terminal.palette().clone();
        let frame = term.terminal.screen().frame(&palette, self.since);
        let drawn = self.renderer.draw(&frame, &mut target, age, state);
        let stats = drawn.stats;
        let damage: Vec<DisplayRect> = drawn
            .damage
            .iter()
            .map(|r| DisplayRect {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            })
            .collect();
        self.since = term.terminal.screen_mut().checkpoint();
        self.pending = false;
        self.owed = false;
        self.lost = false;
        let queued = if damage.is_empty() {
            false
        } else {
            match video.swap(self.id, &damage) {
                Ok(Swapped::Frame | Swapped::Cursor) => true,
                Ok(Swapped::Nothing) => false,
                Err(SwapError::Busy) => {
                    self.pending = true;
                    false
                }
                Err(SwapError::Off) => false,
                Err(SwapError::Failed(e)) => {
                    log::warn!("cannot swap display {}: {e}", self.id.0);
                    false
                }
            }
        };
        if let Some(timer) = &mut self.timer {
            timer.frame(stats.cpu, queued, stats.damage_px, stats.misses);
        }
        let end = clock();
        term.coalesce.drawn(&mut self.wait, start, end, queued);
        queued
    }
}
