//! The terminal of a head's session: the terminal state the child's
//! output drives, when it presents (mode 2026 and coalescing), the blink
//! phases, and a [`View`] per display of the head that draws it.
//!
//! A view's frame is drawn in three steps (ADR-1401 D2): the loop takes
//! it ([`View::prepare`]), the head's render thread composes it into the
//! display's shadow buffer ([`Job::compose`]), and the loop copies what
//! changed into the display's buffer and presents it
//! ([`View::present`]). The display's device, its buffers, its cursor
//! plane and its flips stay with the loop.

use std::ops::Range;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use vtium_font::{Font, Missing, SpritePolicy};
use vtium_platform::display::{DisplayId, Rect as DisplayRect, SwapError, Swapped};
use vtium_platform::pty::{Consumer, Replies};
use vtium_render::{
    Arrow, Canvas, Config as RenderConfig, Depth, DrawState, Mapped, RenderTimer, Renderer, Stats,
    Target,
};
use vtium_term::{Age, Cursor, Damage, Frame, Line, LinkId, Palette, Terminal};

use crate::blink::Blink;
use crate::font::DisplayFace;
use crate::head::DrawHook;
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

/// The damage of the frames last drawn into a display's buffers that the
/// view keeps, newest first: a buffer older than that takes the whole
/// shadow.
pub const AGES_KEPT: usize = 3;

/// The rectangles of damage a view gathers before it copies the whole
/// shadow in their place.
pub const RECTS_MAX: usize = 64;

/// The screen as a head's frame shows it, owned, so the head's render
/// thread reads it while the session runs on: the visible lines, their
/// change ages, the palette, the cursor, the selection and the hovered
/// link.
pub struct Snapshot {
    lines: Vec<Line>,
    damage: Damage,
    palette: Palette,
    cursor: Cursor,
    reverse: bool,
    selection: Vec<Range<usize>>,
    hovered: Option<LinkId>,
    overlay: Option<(usize, String)>,
}

impl Snapshot {
    /// What `terminal`'s view shows now.
    pub fn take(terminal: &Terminal) -> Snapshot {
        let screen = terminal.screen();
        let palette = terminal.palette().clone();
        let frame = screen.frame(&palette, Age::NEVER);
        let rows = frame.rows();
        let lines = (0..rows).filter_map(|r| frame.line(r)).cloned().collect();
        let selection = if screen.has_selection() {
            (0..rows)
                .map(|r| {
                    let mut at = (0..frame.cols())
                        .filter(|&c| frame.cell(r, c).is_some_and(|fc| fc.selected));
                    let first = at.next().unwrap_or(0);
                    let last = at.next_back().unwrap_or(first);
                    if frame.cell(r, first).is_some_and(|fc| fc.selected) {
                        first..last.saturating_add(1)
                    } else {
                        0..0
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        let (cursor, hovered) = (frame.cursor(), frame.hovered());
        let overlay = frame.overlay().map(|o| (o.row, o.uri.to_owned()));
        Snapshot {
            lines,
            damage: screen.damage().clone(),
            cursor,
            reverse: screen.reverse_video(),
            selection,
            hovered,
            overlay,
            palette,
        }
    }

    /// The frame a display that last drew at `since` reads.
    fn frame(&self, since: Age) -> Frame<'_> {
        Frame::new(
            self.lines.iter().collect(),
            &self.damage,
            &self.palette,
            since,
            self.cursor,
            self.reverse,
        )
        .with_selection(self.selection.clone())
        .with_hover(
            self.hovered,
            self.overlay.as_ref().map(|(row, uri)| (*row, uri.as_str())),
        )
    }
}

/// What a display's frames are drawn with: its renderer, and the shadow
/// buffer the renderer composes into. The view holds it between frames,
/// and the head's render thread while it composes one.
pub struct Drawing {
    pub renderer: Renderer<DisplayFace>,
    /// The display's last frame, made at the first compose and again when
    /// the display's size changes.
    shadow: Option<Canvas>,
}

/// A frame for a head's render thread to compose: a display's drawing,
/// the screen and what the frame shows beyond it.
pub struct Job {
    pub key: (PathBuf, DisplayId),
    generation: u64,
    drawing: Box<Drawing>,
    snapshot: Arc<Snapshot>,
    since: Age,
    size: (u32, u32),
    state: DrawState,
    /// The pointer's tip, wherever it is drawn.
    tip: Option<(u32, u32)>,
    hook: Option<DrawHook>,
}

impl Job {
    /// Composes the frame into the drawing's shadow, which the renderer
    /// draws whole when the shadow is new and else from the frame before.
    pub fn compose(self) -> Composed {
        if let Some(hook) = &self.hook {
            hook();
        }
        let mut drawing = self.drawing;
        let (w, h) = self.size;
        let kept = drawing
            .shadow
            .take()
            .filter(|c| (c.width(), c.height()) == (w, h));
        let whole = kept.is_none();
        let mut shadow = kept.unwrap_or_else(|| Canvas::new(w, h, 0));
        let frame = self.snapshot.frame(self.since);
        let age = u32::from(!whole);
        let drawn = drawing.renderer.draw(&frame, &mut shadow, age, &self.state);
        let stats = drawn.stats;
        let damage = drawn
            .damage
            .iter()
            .map(|r| DisplayRect {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            })
            .collect();
        let at = self.tip.and_then(|t| drawing.renderer.pointer_at(t));
        drawing.shadow = Some(shadow);
        Composed {
            key: self.key,
            generation: self.generation,
            drawing,
            damage,
            whole,
            stats,
            at,
            tip: self.tip,
        }
    }
}

/// A frame composed, back from the render thread.
pub struct Composed {
    pub key: (PathBuf, DisplayId),
    generation: u64,
    drawing: Box<Drawing>,
    damage: Vec<DisplayRect>,
    /// The shadow was drawn whole.
    whole: bool,
    pub stats: Stats,
    /// Where the display's cursor plane shows the pointer's tip.
    at: Option<(i32, i32)>,
    tip: Option<(u32, u32)>,
}

/// What [`View::prepare`] made of a draw.
pub enum Prepared {
    /// A hold, a flip or a frame in flight keeps it for later.
    Later,
    /// The display lends no buffer.
    Nothing,
    Job(Job),
}

/// What [`View::present`] did with a frame composed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Presented {
    /// A frame or a cursor change was queued.
    pub queued: bool,
    /// The time the copy into the display's buffer took, when it copied
    /// anything.
    pub copy: Option<std::time::Duration>,
}

/// One display drawing the terminal.
pub struct View {
    pub id: DisplayId,
    /// Here between frames; `None` while the render thread composes one.
    drawing: Option<Box<Drawing>>,
    /// Counts the drawings the view made: a frame composed with an older
    /// one is dropped.
    generation: u64,
    /// The terminal's checkpoint after this view's last frame was taken.
    since: Age,
    pub wait: Wait,
    /// A draw waits for the flip in flight, or the frame composing.
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
    /// The drawing's font cell, arrow, depth and font policy, read while
    /// the render thread holds it.
    cell: vtium_font::Cell,
    arrow: Arrow,
    depth: Depth,
    policy: (SpritePolicy, Missing),
    /// The glyphs to forget, and those that arrived, while the drawing
    /// was away.
    forgotten: Vec<char>,
    arrived: Vec<char>,
    /// What changed in the shadow since its last copy into a buffer; the
    /// whole shadow when `fresh_whole`.
    fresh: Vec<DisplayRect>,
    fresh_whole: bool,
    /// What each of the last frames drawn into a buffer changed, newest
    /// first, at most [`AGES_KEPT`].
    history: Vec<Vec<DisplayRect>>,
    /// When the frame composing was taken.
    sent: Option<Instant>,
}

impl View {
    pub fn new(
        id: DisplayId,
        font: Font<DisplayFace>,
        size: (u32, u32),
        refresh_mhz: Option<u32>,
        config: RenderConfig,
        measure: bool,
    ) -> View {
        let renderer = Renderer::new(font, size.0, size.1, config);
        let mut view = View {
            id,
            height: 0,
            orientation: config.orientation,
            cell: renderer.font().cell(),
            arrow: renderer.arrow().clone(),
            depth: renderer.depth(),
            policy: (renderer.font().policy(), renderer.font().missing()),
            drawing: None,
            generation: 0,
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
            forgotten: Vec::new(),
            arrived: Vec::new(),
            fresh: Vec::new(),
            fresh_whole: true,
            history: Vec::new(),
            sent: None,
        };
        view.install(renderer);
        view
    }

    /// The drawing's renderer, the one frames are drawn with from now on.
    fn install(&mut self, renderer: Renderer<DisplayFace>) {
        self.cell = renderer.font().cell();
        self.arrow = renderer.arrow().clone();
        self.depth = renderer.depth();
        self.policy = (renderer.font().policy(), renderer.font().missing());
        self.drawing = Some(Box::new(Drawing {
            renderer,
            shadow: None,
        }));
        self.generation = self.generation.wrapping_add(1);
        self.forgotten.clear();
        self.arrived.clear();
        self.fresh.clear();
        self.fresh_whole = true;
    }

    /// Draws with `font`, asked for at `height`, turned as `config` says,
    /// from here on: a new renderer, which draws the next frame whole. A
    /// frame composing with the renderer before is dropped.
    pub fn rebuild(&mut self, font: Font<DisplayFace>, height: u32, config: RenderConfig) {
        let config = RenderConfig {
            depth: self.depth,
            ..config
        };
        self.install(Renderer::new(font, self.size.0, self.size.1, config));
        self.height = height;
        self.orientation = config.orientation;
        self.since = Age::NEVER;
        self.lost = true;
        self.pointer.reset();
    }

    /// The font's cell.
    pub fn cell(&self) -> vtium_font::Cell {
        self.cell
    }

    /// The pointer's arrow, as the renderer draws it.
    pub fn arrow(&self) -> &Arrow {
        &self.arrow
    }

    pub fn depth(&self) -> Depth {
        self.depth
    }

    /// The font's sprite policy and missing glyph.
    pub fn font_policy(&self) -> (SpritePolicy, Missing) {
        self.policy
    }

    /// Whether a frame of this view is composing.
    pub fn away(&self) -> bool {
        self.drawing.is_none()
    }

    /// Forgets the glyphs cached for `chars`, now or when the drawing
    /// comes back.
    pub fn forget_glyphs(&mut self, chars: &[char]) {
        match &mut self.drawing {
            Some(d) => d.renderer.forget_glyphs(chars),
            None => self.forgotten.extend_from_slice(chars),
        }
    }

    /// The glyphs of `chars` arrived: the cells showing them draw again,
    /// now or when the drawing comes back.
    pub fn glyphs_changed(&mut self, chars: &[char]) {
        match &mut self.drawing {
            Some(d) => d.renderer.glyphs_changed(chars),
            None => self.arrived.extend_from_slice(chars),
        }
    }

    /// Takes a frame of `term` for the render thread, unless a hold keeps
    /// the last frame, a flip is in flight or a frame is composing. The
    /// pointer goes on the display's cursor plane where it has one. The
    /// head's frames share `snapshot`, taken at the first that needs it.
    #[expect(clippy::too_many_arguments, reason = "a frame's parts")]
    pub fn prepare(
        &mut self,
        key: &(PathBuf, DisplayId),
        term: &mut Term,
        snapshot: &mut Option<(Arc<Snapshot>, Age)>,
        video: &mut dyn Video,
        state: &DrawState,
        hook: Option<&DrawHook>,
        clock: &dyn Fn() -> Instant,
    ) -> Prepared {
        if term.sync.holding(&term.terminal) && !self.lost {
            self.owed = true;
            return Prepared::Later;
        }
        if self.away() || video.is_flipping(self.id) {
            self.pending = true;
            return Prepared::Later;
        }
        let tip = state.pointer;
        let state = DrawState {
            pointer: self.pointer.before_draw(video, self.id, &self.arrow, tip),
            ..*state
        };
        let Some(drawing) = self.drawing.take() else {
            return Prepared::Nothing;
        };
        let (snapshot, upto) = snapshot.get_or_insert_with(|| {
            let taken = Snapshot::take(&term.terminal);
            (Arc::new(taken), term.terminal.screen_mut().checkpoint())
        });
        let since = std::mem::replace(&mut self.since, *upto);
        // a display that lost its picture takes the whole shadow
        self.fresh_whole |= self.lost;
        self.pending = false;
        self.owed = false;
        self.lost = false;
        self.sent = Some(clock());
        Prepared::Job(Job {
            key: key.clone(),
            generation: self.generation,
            drawing,
            snapshot: Arc::clone(snapshot),
            since,
            size: self.size,
            state,
            tip,
            hook: hook.cloned(),
        })
    }

    /// A job the render thread could not take: its drawing comes back,
    /// and the frame waits for the next draw.
    pub fn refused(&mut self, job: Job) {
        if job.generation == self.generation {
            self.drawing = Some(job.drawing);
            self.since = job.since;
            self.pending = true;
        }
    }

    /// Takes back the drawing of `composed`, and, while `awake`, copies
    /// what changed into `video`'s buffer and presents it: what the frame
    /// changed, with what the frames before it changed where the buffer
    /// is older, and the whole shadow where the buffer is new or the
    /// shadow is. A frame composed with a drawing the view no longer
    /// uses is dropped. The cursor plane follows the pointer, and a
    /// frame that changed nothing is still presented when the plane did.
    pub fn present(
        &mut self,
        composed: Composed,
        term: &mut Term,
        video: &mut dyn Video,
        awake: bool,
        clock: &dyn Fn() -> Instant,
    ) -> Presented {
        if composed.generation != self.generation {
            return Presented::default();
        }
        let mut drawing = composed.drawing;
        if !self.forgotten.is_empty() {
            drawing
                .renderer
                .forget_glyphs(&std::mem::take(&mut self.forgotten));
        }
        if !self.arrived.is_empty() {
            drawing
                .renderer
                .glyphs_changed(&std::mem::take(&mut self.arrived));
        }
        self.fresh_whole |= composed.whole;
        self.fresh.extend(composed.damage);
        if self.fresh.len() > RECTS_MAX {
            self.fresh_whole = true;
            self.fresh.clear();
        }
        let shadow = drawing.shadow.as_ref().map(|s| (s.width(), s.height()));
        let drawing = &**self.drawing.insert(drawing);
        if !awake {
            self.lost = true;
            return Presented::default();
        }
        let start = clock();
        let Some(canvas) = video.canvas(self.id) else {
            return Presented::default();
        };
        let (width, height, stride, age) = (canvas.width, canvas.height, canvas.stride, canvas.age);
        if shadow != Some((width, height)) {
            self.lost = true;
            self.pending = true;
            return Presented::default();
        }
        let whole_rect = DisplayRect {
            x: 0,
            y: 0,
            width,
            height,
        };
        let older = usize::try_from(age).unwrap_or(usize::MAX).saturating_sub(1);
        let changed = self.fresh_whole || !self.fresh.is_empty();
        let rects: Vec<DisplayRect> = if !changed {
            Vec::new()
        } else if self.fresh_whole || age == 0 || older > self.history.len() {
            vec![whole_rect]
        } else {
            self.fresh
                .iter()
                .chain(self.history.iter().take(older).flatten())
                .copied()
                .collect()
        };
        let stride = usize::try_from(stride).unwrap_or(0);
        let copied = match (
            Mapped::new(canvas.pixels, width, height, stride),
            &drawing.shadow,
        ) {
            (Some(mut target), Some(shadow)) => {
                copy(shadow, &mut target, &rects);
                true
            }
            _ => false,
        };
        if !copied {
            return Presented::default();
        }
        if changed {
            let fresh = if self.fresh_whole {
                vec![whole_rect]
            } else {
                std::mem::take(&mut self.fresh)
            };
            self.fresh.clear();
            self.fresh_whole = false;
            self.history.insert(0, fresh);
            self.history.truncate(AGES_KEPT);
        }
        let copy = clock().saturating_duration_since(start);
        self.pointer
            .after_draw(video, self.id, composed.tip, composed.at);
        let queued = if rects.is_empty() && !video.need_commit(self.id) {
            false
        } else {
            match video.swap(self.id, &rects) {
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
        let stats = composed.stats;
        if let Some(timer) = &mut self.timer {
            timer.frame(stats.cpu, queued, stats.damage_px, stats.misses);
        }
        let end = clock();
        let start = self.sent.take().unwrap_or(start);
        term.coalesce.drawn(&mut self.wait, start, end, queued);
        Presented {
            queued,
            copy: (!rects.is_empty()).then_some(copy),
        }
    }

    /// Draws `term` into `video`'s canvas and presents it at once, the
    /// frame composed on the calling thread: [`View::prepare`],
    /// [`Job::compose`] and [`View::present`] in a row. Returns whether a
    /// frame was queued.
    pub fn draw(
        &mut self,
        term: &mut Term,
        video: &mut dyn Video,
        state: &DrawState,
        clock: &dyn Fn() -> Instant,
    ) -> bool {
        let key = (PathBuf::new(), self.id);
        let mut snapshot = None;
        match self.prepare(&key, term, &mut snapshot, video, state, None, clock) {
            Prepared::Job(job) => self.present(job.compose(), term, video, true, clock).queued,
            Prepared::Later | Prepared::Nothing => false,
        }
    }
}

/// Copies `rects` of `shadow` into `target`, each clipped to the shadow.
fn copy(shadow: &Canvas, target: &mut impl Target, rects: &[DisplayRect]) {
    let (w, h) = (shadow.width(), shadow.height());
    let pixels = shadow.pixels();
    let width = usize::try_from(w).unwrap_or(0);
    for r in rects {
        let x0 = r.x.min(w);
        let x1 = r.x.saturating_add(r.width).min(w);
        let y1 = r.y.saturating_add(r.height).min(h);
        let (from, to) = (
            usize::try_from(x0).unwrap_or(0),
            usize::try_from(x1).unwrap_or(0),
        );
        for y in r.y.min(h)..y1 {
            let row = usize::try_from(y).unwrap_or(0).saturating_mul(width);
            if let Some(run) = pixels.get(row.saturating_add(from)..row.saturating_add(to)) {
                target.write(x0, y, run);
            }
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::disallowed_methods,
    clippy::indexing_slicing,
    clippy::panic,
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
        let face: DisplayFace = Box::new(Unifont::new(16));
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

    fn draw(term: &mut Term, view: &mut View, video: &mut dyn Video, pointer: Option<(u32, u32)>) {
        let state = DrawState {
            pointer,
            ..DrawState::default()
        };
        view.draw(term, video, &state, &Instant::now);
    }

    /// A display with two buffers, flipped at each swap: the buffer lent
    /// is the back one, its age the frames drawn since it was last.
    struct Flip {
        bufs: [Vec<u8>; 2],
        back: usize,
        drawn: [Option<u32>; 2],
        count: u32,
    }

    impl Flip {
        fn new() -> Flip {
            let n = (SIZE.0 * SIZE.1 * 4) as usize;
            Flip {
                bufs: [vec![0; n], vec![0; n]],
                back: 0,
                drawn: [None, None],
                count: 0,
            }
        }

        /// Whether buffer `b` shows anything in the grid's cell `col` of
        /// row 0, the grid at 2,1 in 8x16 cells.
        fn shows(&self, b: usize, col: u32) -> bool {
            (1..17).any(|y| {
                (2 + col * 8..10 + col * 8).any(|x| {
                    let i = ((y * SIZE.0 + x) * 4) as usize;
                    self.bufs[b][i..i + 3].iter().any(|&p| p != 0)
                })
            })
        }
    }

    impl Video for Flip {
        fn backend(&self) -> &'static str {
            "flip"
        }
        fn events(&mut self) -> Vec<Event> {
            Vec::new()
        }
        fn displays(&self) -> Vec<DisplayId> {
            vec![ID]
        }
        fn name(&self, _: DisplayId) -> String {
            "FLIP-1".into()
        }
        fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
            Some(SIZE)
        }
        fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
            None
        }
        fn canvas(&mut self, _: DisplayId) -> Option<Canvas<'_>> {
            let age = self.drawn[self.back].map_or(0, |d| self.count - d + 1);
            Some(Canvas {
                pixels: &mut self.bufs[self.back],
                width: SIZE.0,
                height: SIZE.1,
                stride: SIZE.0 * 4,
                age,
            })
        }
        fn swap(&mut self, _: DisplayId, damage: &[DisplayRect]) -> Result<Swapped, SwapError> {
            if damage.is_empty() {
                return Ok(Swapped::Nothing);
            }
            self.count += 1;
            self.drawn[self.back] = Some(self.count);
            self.back ^= 1;
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
    }

    // The shadow's copy into a buffer two frames old carries what the
    // frame before changed too: A goes into one buffer, B into the other,
    // and C's buffer, which last held A, shows A, B and C.
    #[test]
    fn a_buffer_two_frames_old_takes_the_frame_befores_change_too() {
        let (mut term, mut view) = view();
        let mut video = Flip::new();
        for text in [b"A", b"B", b"C"] {
            term.terminal.input(text);
            draw(&mut term, &mut view, &mut video, None);
        }
        assert_eq!(video.back, 1, "three frames swapped");
        let shown: Vec<bool> = (0..3).map(|col| video.shows(0, col)).collect();
        assert_eq!(shown, [true, true, true], "the buffer two frames old");
        let shown: Vec<bool> = (0..3).map(|col| video.shows(1, col)).collect();
        assert_eq!(shown, [true, true, false], "the buffer before");
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
        assert_eq!(video.arrow.as_ref(), Some(view.arrow()));
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

    // A frame composed while the VT was ours that comes back after its
    // release, or after the blank, is not shown, on a device that would
    // still lend its buffer: nothing is swapped, and the display draws
    // whole at the next frame.
    #[test]
    fn a_frame_back_once_asleep_is_not_shown() {
        let (mut term, mut view) = view();
        let mut video = Plane::new(false);
        let key = (PathBuf::new(), ID);
        let mut snapshot = None;
        let state = DrawState::default();
        let Prepared::Job(job) = view.prepare(
            &key,
            &mut term,
            &mut snapshot,
            &mut video,
            &state,
            None,
            &Instant::now,
        ) else {
            panic!("a job");
        };
        let shown = view.present(job.compose(), &mut term, &mut video, false, &Instant::now);
        assert_eq!(shown, Presented::default());
        assert_eq!(video.swaps, [], "nothing swapped");
        assert!(!lit(&video), "nothing copied");
        assert!(view.lost, "drawn whole at the next frame");
        assert!(!view.away(), "the drawing is back");
    }

    // A frame that changed nothing copies nothing, and the frame figures
    // count no copy for it.
    #[test]
    fn a_frame_that_changed_nothing_copies_nothing() {
        let (mut term, mut view) = view();
        let mut video = Plane::new(false);
        draw(&mut term, &mut view, &mut video, None);
        let key = (PathBuf::new(), ID);
        let mut snapshot = None;
        let state = DrawState::default();
        let Prepared::Job(job) = view.prepare(
            &key,
            &mut term,
            &mut snapshot,
            &mut video,
            &state,
            None,
            &Instant::now,
        ) else {
            panic!("a job");
        };
        let shown = view.present(job.compose(), &mut term, &mut video, true, &Instant::now);
        assert_eq!(shown, Presented::default());
        assert_eq!(video.swaps, [1], "the first frame alone");
    }

    // A zoom or a rotation keeps the display's depth, which the device
    // gave when the view was made.
    #[test]
    fn a_new_renderer_keeps_the_displays_depth() {
        let face: DisplayFace = Box::new(Unifont::new(16));
        let one = RenderConfig {
            depth: vtium_render::Depth::One,
            ..RenderConfig::default()
        };
        let mut view = View::new(ID, Font::new(face), SIZE, None, one, false);
        let face: DisplayFace = Box::new(Unifont::new(16));
        view.rebuild(Font::new(face), 32, RenderConfig::default());
        assert_eq!(view.depth(), vtium_render::Depth::One);
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
        let face: DisplayFace = Box::new(Unifont::new(16));
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
