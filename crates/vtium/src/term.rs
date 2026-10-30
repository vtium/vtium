//! The terminal of a head's session: the terminal state the child's
//! output drives, when it presents (mode 2026 and coalescing), the blink
//! phases, and a [`View`] per display of the head that draws it.

use std::process::ExitStatus;
use std::rc::Rc;
use std::time::Instant;

use vtium_font::{Face, Font};
use vtium_platform::display::{DisplayId, Rect as DisplayRect, SwapError, Swapped};
use vtium_platform::pty::{Consumer, Replies};
use vtium_render::{Config as RenderConfig, DrawState, Mapped, RenderTimer, Renderer};
use vtium_term::{Age, Terminal};

use crate::blink::Blink;
use crate::pointer::Pointer;
use crate::sync::{Action, Coalesce, Sync, Wait};
use crate::video::Video;

/// The clock the terminal reads while it parses: the loop's, or a test's.
pub type Clock = Rc<dyn Fn() -> Instant>;

/// A session's terminal and the rules for when it presents.
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
    /// A frame end or a timeout asks every display of the head to present.
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
        // The output after an OSC 50 request waits for the turn that
        // switches the font, so a query that follows names the new set.
        if self.terminal.font_requested() {
            return 0;
        }
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
    /// The height the display's font was asked for.
    pub height: u32,
    pub orientation: vtium_render::Orientation,
    /// Who draws the pointer: the display's cursor plane or the renderer.
    pub pointer: Pointer,
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
            height: 0,
            orientation: config.orientation,
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
            pointer: Pointer::default(),
        }
    }

    /// Draws with `font`, asked for at `height`, turned as `config` says,
    /// from here on: a new renderer, which draws the next frame whole.
    pub fn rebuild(&mut self, font: Font<Box<dyn Face>>, height: u32, config: RenderConfig) {
        let config = RenderConfig {
            depth: self.renderer.depth(),
            ..config
        };
        self.renderer = Renderer::new(font, self.size.0, self.size.1, config);
        self.height = height;
        self.orientation = config.orientation;
        self.since = Age::NEVER;
        self.lost = true;
        self.pointer.reset();
    }

    /// Draws `term` into `video`'s canvas and presents it, unless a hold
    /// keeps the last frame or a flip is in flight. The pointer goes on
    /// the display's cursor plane where it has one, and a frame that drew
    /// nothing is still presented when the plane changed. Returns whether
    /// a frame was queued.
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
        let tip = state.pointer;
        let state = &DrawState {
            pointer: self
                .pointer
                .before_draw(video, self.id, &self.renderer, tip),
            ..*state
        };
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
        self.pointer.after_draw(video, self.id, &self.renderer, tip);
        let queued = if damage.is_empty() && !video.need_commit(self.id) {
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
        // the display gave the plane up: the renderer draws the pointer
        self.pending |= self.pointer.after_swap(video, self.id);
        if let Some(timer) = &mut self.timer {
            timer.frame(stats.cpu, queued, stats.damage_px, stats.misses);
        }
        let end = clock();
        term.coalesce.drawn(&mut self.wait, start, end, queued);
        queued
    }
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::disallowed_methods,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a test's sizes are small, its clock is the host's, and a test fails by panicking"
)]
mod tests {
    use std::any::Any;
    use std::io;
    use std::time::Duration;

    use vtium_font::Unifont;
    use vtium_platform::display::{Canvas, Dpms, Event};
    use vtium_render::Arrow;

    use super::*;

    const ID: DisplayId = DisplayId(1);
    const SIZE: (u32, u32) = (36, 34);

    /// A display in memory, with a cursor plane or without: what each swap
    /// carried, and where the plane shows the pointer.
    struct Plane {
        has_plane: bool,
        pixels: Vec<u8>,
        age: u32,
        arrow: Option<Arrow>,
        shown: Option<(i32, i32)>,
        /// The plane's state the last swap sent.
        sent: Option<(i32, i32)>,
        /// The number of damage rectangles each swap carried.
        swaps: Vec<usize>,
        /// The next swap showing the cursor fails with it and takes
        /// without it, so the plane is given up.
        fails_with_cursor: bool,
    }

    impl Plane {
        fn new(has_plane: bool) -> Plane {
            Plane {
                has_plane,
                pixels: vec![0; (SIZE.0 * SIZE.1 * 4) as usize],
                age: 0,
                arrow: None,
                shown: None,
                sent: None,
                swaps: Vec::new(),
                fails_with_cursor: false,
            }
        }

        /// The pixel at `x`, `y`.
        fn pixel(&self, x: u32, y: u32) -> u32 {
            let i = ((y * SIZE.0 + x) * 4) as usize;
            u32::from_ne_bytes(self.pixels[i..i + 4].try_into().unwrap())
        }
    }

    impl Video for Plane {
        fn backend(&self) -> &'static str {
            "plane"
        }
        fn events(&mut self) -> Vec<Event> {
            Vec::new()
        }
        fn displays(&self) -> Vec<DisplayId> {
            vec![ID]
        }
        fn name(&self, _: DisplayId) -> String {
            "PLANE-1".into()
        }
        fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
            Some(SIZE)
        }
        fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
            None
        }
        fn canvas(&mut self, _: DisplayId) -> Option<Canvas<'_>> {
            Some(Canvas {
                pixels: &mut self.pixels,
                width: SIZE.0,
                height: SIZE.1,
                stride: SIZE.0 * 4,
                age: self.age,
            })
        }
        fn swap(&mut self, _: DisplayId, damage: &[DisplayRect]) -> Result<Swapped, SwapError> {
            self.swaps.push(damage.len());
            if self.fails_with_cursor && self.shown.is_some() {
                self.arrow = None;
                self.shown = None;
            }
            self.sent = self.shown;
            if damage.is_empty() {
                return Ok(Swapped::Cursor);
            }
            self.age = 1;
            Ok(Swapped::Frame)
        }
        fn is_flipping(&self, _: DisplayId) -> bool {
            false
        }
        fn set_dpms(&mut self, _: DisplayId, _: Dpms) -> io::Result<()> {
            Ok(())
        }
        fn sleep(&mut self) {}
        fn wake_up(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn unblank(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn input(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn hotplug(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn as_any(&mut self) -> &mut dyn Any {
            self
        }
        fn setup_cursor(&mut self, _: DisplayId, arrow: &Arrow) -> io::Result<()> {
            if !self.has_plane {
                return Err(io::ErrorKind::Unsupported.into());
            }
            self.arrow = Some(arrow.clone());
            self.shown = None;
            Ok(())
        }
        fn show_cursor(&mut self, _: DisplayId, at: (i32, i32)) -> io::Result<()> {
            self.shown = Some(at);
            Ok(())
        }
        fn hide_cursor(&mut self, _: DisplayId) {
            self.shown = None;
        }
        fn hardware_cursor(&self, _: DisplayId) -> bool {
            self.arrow.is_some()
        }
        fn need_commit(&self, _: DisplayId) -> bool {
            self.shown != self.sent
        }
    }

    /// A terminal of 4 by 2 cells and its view on a display of 36 by 34,
    /// the grid at 2,1.
    fn view() -> (Term, View) {
        let clock: Clock = Rc::new(Instant::now);
        let term = Term::new(
            Terminal::new(4, 2, 0),
            Sync::new(Duration::ZERO, 0),
            Blink::new(false),
            clock,
        );
        let face: Box<dyn Face> = Box::new(Unifont::new(16));
        let view = View::new(
            ID,
            Font::new(face),
            SIZE,
            None,
            RenderConfig::default(),
            false,
        );
        (term, view)
    }

    fn draw(term: &mut Term, view: &mut View, video: &mut Plane, pointer: Option<(u32, u32)>) {
        let state = DrawState {
            pointer,
            ..DrawState::default()
        };
        view.draw(term, video, &state, &Instant::now);
    }

    /// Whether the display shows anything but black.
    fn lit(video: &Plane) -> bool {
        (0..SIZE.1).any(|y| (0..SIZE.0).any(|x| video.pixel(x, y) != 0))
    }

    // The pointer on a display's cursor plane (vtium/vtium#92): the plane
    // takes the renderer's arrow, and moving or hiding the pointer draws
    // nothing and swaps the plane's change alone.
    #[test]
    fn a_cursor_plane_shows_the_pointer_and_nothing_is_drawn() {
        let (mut term, mut view) = view();
        let mut video = Plane::new(true);
        draw(&mut term, &mut view, &mut video, Some((5, 6)));
        assert_eq!(video.arrow.as_ref(), Some(view.renderer.arrow()));
        assert_eq!(video.sent, Some((7, 7)), "the first frame carries it");
        assert!(!lit(&video), "the frame draws no arrow");
        draw(&mut term, &mut view, &mut video, Some((9, 9)));
        assert_eq!(video.sent, Some((11, 10)));
        assert_eq!(video.swaps, [1, 0], "the move swaps the plane alone");
        draw(&mut term, &mut view, &mut video, Some((9, 9)));
        assert_eq!(video.swaps, [1, 0], "nothing changed, nothing swapped");
        draw(&mut term, &mut view, &mut video, None);
        assert_eq!(video.sent, None);
        assert_eq!(video.swaps, [1, 0, 0]);
    }

    // A zoom or a rotation keeps the display's depth, which the device
    // gave when the view was made.
    #[test]
    fn a_new_renderer_keeps_the_displays_depth() {
        let face: Box<dyn Face> = Box::new(Unifont::new(16));
        let one = RenderConfig {
            depth: vtium_render::Depth::One,
            ..RenderConfig::default()
        };
        let mut view = View::new(ID, Font::new(face), SIZE, None, one, false);
        let face: Box<dyn Face> = Box::new(Unifont::new(16));
        view.rebuild(Font::new(face), 32, RenderConfig::default());
        assert_eq!(view.renderer.depth(), vtium_render::Depth::One);
    }

    // A new renderer, as a zoom or a rotation makes, gives the plane its
    // arrow at the next draw.
    #[test]
    fn a_new_renderer_gives_the_plane_its_arrow() {
        let (mut term, mut view) = view();
        let mut video = Plane::new(true);
        draw(&mut term, &mut view, &mut video, Some((5, 6)));
        let config = RenderConfig {
            orientation: vtium_render::Orientation::Right,
            ..RenderConfig::default()
        };
        let face: Box<dyn Face> = Box::new(Unifont::new(16));
        view.rebuild(Font::new(face), 16, config);
        draw(&mut term, &mut view, &mut video, Some((5, 6)));
        let turned = Arrow::new(16, vtium_render::Orientation::Right);
        assert_eq!(video.arrow, Some(turned));
        // turned right, the grid's pixel 5,6 lands at 2 + 32 - 1 - 6, 1 + 5
        assert_eq!(video.sent, Some((27, 6)));
    }

    // A plane the display gave up after a failed commit: the view asks to
    // draw again, and draws the arrow in software.
    #[test]
    fn a_plane_given_up_hands_the_arrow_to_the_renderer() {
        let (mut term, mut view) = view();
        let mut video = Plane::new(true);
        video.fails_with_cursor = true;
        draw(&mut term, &mut view, &mut video, Some((5, 6)));
        assert_eq!(video.arrow, None, "the plane is given up");
        assert!(!lit(&video));
        assert!(view.pending, "a draw is owed");
        draw(&mut term, &mut view, &mut video, Some((5, 6)));
        assert!(lit(&video), "the arrow is drawn in software");
        assert_eq!(video.pixel(7, 8) & 0xff_ff_ff, 0xff_ff_ff);
    }

    // A display without a cursor plane has the renderer draw the arrow.
    #[test]
    fn without_a_cursor_plane_the_arrow_is_drawn() {
        let (mut term, mut view) = view();
        let mut video = Plane::new(false);
        draw(&mut term, &mut view, &mut video, Some((5, 6)));
        assert_eq!(video.arrow, None);
        assert!(lit(&video), "the frame draws the arrow");
        // its tip's fill, one pixel below the hotspot at 7,7
        assert_eq!(video.pixel(7, 8) & 0xff_ff_ff, 0xff_ff_ff);
        assert_eq!(video.swaps, [1]);
    }
}
