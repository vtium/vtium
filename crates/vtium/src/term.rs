//! The terminal of a head's session: the terminal state the child's
//! output drives, when it presents (mode 2026 and coalescing), the blink
//! phases, and a [`View`] per display of the head that draws it.
//!
//! A view's frame is drawn in three steps (ADR-1401 D2): the loop takes
//! it, with the buffer token the display lends ([`View::prepare`]); the
//! head's render thread composes it straight into that buffer, writing
//! only ([`Job::compose`]); and the loop gives the token back to the
//! display, which presents the buffer with the frame's damage
//! ([`View::present`]). The display's device, its buffers, its cursor
//! plane and its flips stay with the loop, and no view keeps a copy of
//! its display's frame.

use std::ops::Range;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use vtium_font::{Face, Font, Missing, SpritePolicy};
use vtium_platform::display::{DisplayId, Rect as DisplayRect, SwapError, Swapped, Token};
use vtium_platform::pty::{Consumer, Replies};
use vtium_render::{
    Arrow, Canvas, Colors, Config as RenderConfig, Depth, DrawState, Mapped, Mark, Notice,
    Orientation, Rect as RenderRect, RenderTimer, Renderer, Stats, Surface, Target,
};
use vtium_term::{Age, Cursor, Damage, Frame, Line, LinkId, Palette, Rgb, Terminal};

use crate::blink::Blink;
use crate::config::Config;
use crate::config::keys::{OVERLAY_COLORS, Scheme};
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
    /// Bytes of the child's output parsed since the start.
    pub parsed: u64,
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
            parsed: 0,
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
        self.parsed = self
            .parsed
            .saturating_add(u64::try_from(taken).unwrap_or(u64::MAX));
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

/// What a display's frames are drawn with: its renderer, which keeps the
/// history of each cell, so it knows what a buffer of any age lacks. The
/// view holds it between frames, and the head's render thread while it
/// composes one.
pub struct Drawing {
    pub renderer: Renderer<DisplayFace>,
}

impl Drawing {
    /// The bytes the drawing holds for its display's frames beyond the
    /// glyph cache: the row scratch. No drawing holds a whole frame.
    pub fn frame_bytes(&self) -> usize {
        self.renderer.row_scratch_bytes()
    }
}

/// The colors of what vtium draws of its own (ADR-1401 D5): a notice's
/// box, and the on-screen display's line and dot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayColors {
    pub notice: Colors,
    pub osd: Colors,
}

impl OverlayColors {
    /// The colors kmscon.conf's six color keys set, each unset key at its
    /// default, which is the color vtium draws with none set.
    pub fn configured(config: &Config) -> OverlayColors {
        let [notice, osd] =
            OVERLAY_COLORS.map(|keys| colors(keys.map(|k| config.rgb(k).unwrap_or_default())));
        OverlayColors { notice, osd }
    }

    /// The colors of `scheme`, one of the example configuration's.
    pub fn scheme(scheme: &Scheme) -> OverlayColors {
        OverlayColors {
            notice: colors(scheme.notice),
            osd: colors(scheme.osd),
        }
    }
}

/// The colors of a text, a box and its edge, in that order.
fn colors([ink, paper, edge]: [[u8; 3]; 3]) -> Colors {
    let rgb = |[r, g, b]: [u8; 3]| Rgb::new(r, g, b);
    Colors {
        ink: rgb(ink),
        paper: rgb(paper),
        edge: rgb(edge),
    }
}

impl Default for OverlayColors {
    /// The colors with none of the keys set: the keys' defaults, as a head
    /// draws them.
    fn default() -> OverlayColors {
        OverlayColors::configured(&Config::default())
    }
}

/// What vtium draws of its own over a display's frame, outside the
/// terminal's cells (ADR-1401 D5, D7): a box across the top, of the
/// on-screen display's line and the notices, and the on-screen display's
/// dot over the top-right cell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overlay {
    pub notice: Option<Notice>,
    /// The box holds a notice of a problem, and is drawn in the notice's
    /// colors; else in the on-screen display's.
    pub problem: bool,
    pub mark: Option<Mark>,
    pub colors: OverlayColors,
}

impl Overlay {
    /// What a head draws of its own: one box holding the on-screen
    /// display's full line `status` and the `notices` beneath it, each
    /// starting a line, in the notices' colors while a notice is in it and
    /// else in the on-screen display's, and the on-screen display's dot
    /// `mark` in the on-screen display's colors, all from `colors`. Tier 2
    /// and the specimens build their pictures of the overlay with it too.
    pub fn new(
        status: Option<&str>,
        notices: &[&str],
        mark: Option<Mark>,
        colors: OverlayColors,
    ) -> Overlay {
        let lines: Vec<&str> = status.into_iter().chain(notices.iter().copied()).collect();
        Overlay {
            notice: (!lines.is_empty()).then(|| Notice::new(lines.join("\n"))),
            problem: !notices.is_empty(),
            mark,
            colors,
        }
    }

    /// What the screen the display guard lights for a notice alone draws
    /// (ADR-1401 D7, D9): `notice` alone, in the notice's colors from
    /// `colors`, the panic screen's report among them. Tier 2 builds its
    /// pictures of that screen with it too.
    pub fn alone(notice: Notice, colors: OverlayColors) -> Overlay {
        Overlay {
            notice: Some(notice),
            problem: true,
            mark: None,
            colors,
        }
    }

    /// Draws the overlay over the `grid` that `renderer` drew into
    /// `target`, turned as `orientation` says, and returns the areas it
    /// covers. Tier 2 draws its pictures of the overlay through it.
    pub fn draw<F: Face>(
        &self,
        renderer: &Renderer<F>,
        grid: (usize, usize),
        orientation: Orientation,
        target: &mut Canvas,
    ) -> Vec<DisplayRect> {
        let size = (target.width(), target.height());
        self.draw_into(renderer, grid, orientation, size, target)
    }

    /// Draws the overlay into `target`, a display buffer of `size` that is
    /// only written, as [`Overlay::draw`] does.
    pub fn draw_into<F: Face>(
        &self,
        renderer: &Renderer<F>,
        grid: (usize, usize),
        orientation: Orientation,
        (width, height): (u32, u32),
        target: &mut impl Target,
    ) -> Vec<DisplayRect> {
        let font = renderer.font();
        let surface = Surface {
            width,
            height,
            orientation,
            depth: renderer.depth(),
        };
        let cell = self
            .mark
            .and_then(|_| top_right(surface, grid, font.cell()));
        let OverlayColors {
            notice: problem,
            osd,
        } = self.colors;
        let boxed = if self.problem { problem } else { osd };
        let notice = self
            .notice
            .as_ref()
            .and_then(|n| n.draw(font, surface, boxed, cell, target));
        let mark = self
            .mark
            .zip(cell)
            .and_then(|(m, c)| m.draw(font, surface, c, osd, target));
        [notice, mark]
            .into_iter()
            .flatten()
            .map(|r| DisplayRect {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            })
            .collect()
    }
}

/// The display rectangle of the top-right cell of a `grid` of `cell`s,
/// centred on `surface` and turned with it as the renderer places it.
fn top_right(surface: Surface, grid: (usize, usize), cell: vtium_font::Cell) -> Option<RenderRect> {
    let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
    let (cols, rows) = (n(grid.0), n(grid.1));
    let (cw, ch) = (cell.width, cell.height);
    if cols == 0 || rows == 0 || cw == 0 || ch == 0 {
        return None;
    }
    let turned = matches!(surface.orientation, Orientation::Right | Orientation::Left);
    let (gw, gh) = (cols.saturating_mul(cw), rows.saturating_mul(ch));
    let (across, down) = if turned { (gh, gw) } else { (gw, gh) };
    let off_x = surface.width.saturating_sub(across) / 2;
    let off_y = surface.height.saturating_sub(down) / 2;
    let (max_x, max_y) = (off_x.saturating_add(across), off_y.saturating_add(down));
    // the cell's left edge, along the grid's rows
    let lx = cols.saturating_sub(1).saturating_mul(cw);
    let (x, y) = match surface.orientation {
        Orientation::Normal => (off_x.saturating_add(lx), off_y),
        Orientation::UpsideDown => (
            max_x.saturating_sub(lx.saturating_add(cw)),
            max_y.saturating_sub(ch),
        ),
        Orientation::Right => (max_x.saturating_sub(ch), off_y.saturating_add(lx)),
        Orientation::Left => (off_x, max_y.saturating_sub(lx.saturating_add(cw))),
    };
    let (width, height) = if turned { (ch, cw) } else { (cw, ch) };
    Some(RenderRect {
        x,
        y,
        width,
        height,
    })
}

/// A frame for a head's render thread to compose: a display's drawing,
/// the token of the display's buffer it is drawn into, the screen and what
/// the frame shows beyond it.
pub struct Job {
    pub key: (PathBuf, DisplayId),
    generation: u64,
    drawing: Box<Drawing>,
    token: Token,
    snapshot: Arc<Snapshot>,
    since: Age,
    state: DrawState,
    /// The pointer's tip, wherever it is drawn.
    tip: Option<(u32, u32)>,
    hook: Option<DrawHook>,
    overlay: Overlay,
    orientation: Orientation,
    /// The areas the overlay last covered, where it changed since: the
    /// renderer draws them again in every buffer.
    gone: Vec<DisplayRect>,
    /// The display lost its picture: the frame is drawn whole, whatever
    /// the buffer's age.
    whole: bool,
}

impl Job {
    /// Composes the frame straight into the token's buffer, writing only:
    /// the renderer draws the cells the buffer lacks at its age, the whole
    /// picture at age 0, and the overlay goes over them.
    pub fn compose(self) -> Composed {
        if let Some(hook) = &self.hook {
            hook();
        }
        let mut drawing = self.drawing;
        let mut token = self.token;
        for r in &self.gone {
            drawing.renderer.redraw_area(RenderRect {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            });
        }
        let frame = self.snapshot.frame(self.since);
        let grid = (frame.cols(), frame.rows());
        let size = token.size();
        let drawn = token
            .draw(|canvas| {
                let stride = usize::try_from(canvas.stride).ok()?;
                let mut target = Mapped::new(canvas.pixels, size.0, size.1, stride)?;
                let age = if self.whole { 0 } else { canvas.age };
                let drawn = drawing.renderer.draw(&frame, &mut target, age, &self.state);
                let stats = drawn.stats;
                let mut damage: Vec<DisplayRect> = drawn
                    .damage
                    .iter()
                    .map(|r| DisplayRect {
                        x: r.x,
                        y: r.y,
                        width: r.width,
                        height: r.height,
                    })
                    .collect();
                let (most_cols, most_rows) = drawing.renderer.grid_size();
                let grid = (grid.0.min(most_cols), grid.1.min(most_rows));
                let overlaid = self.overlay.draw_into(
                    &drawing.renderer,
                    grid,
                    self.orientation,
                    size,
                    &mut target,
                );
                damage.extend_from_slice(&overlaid);
                Some((damage, overlaid, stats))
            })
            .flatten();
        let (damage, overlaid, stats) = drawn.unwrap_or_default();
        let at = self.tip.and_then(|t| drawing.renderer.pointer_at(t));
        Composed {
            key: self.key,
            generation: self.generation,
            drawing,
            token,
            damage,
            overlaid,
            stats,
            at,
            tip: self.tip,
        }
    }
}

/// A frame composed, back from the render thread with its token.
pub struct Composed {
    pub key: (PathBuf, DisplayId),
    generation: u64,
    drawing: Box<Drawing>,
    token: Token,
    damage: Vec<DisplayRect>,
    /// The areas the overlay drew over.
    overlaid: Vec<DisplayRect>,
    pub stats: Stats,
    /// Where the display's cursor plane shows the pointer's tip.
    at: Option<(i32, i32)>,
    tip: Option<(u32, u32)>,
}

/// What [`View::prepare`] made of a draw.
pub enum Prepared {
    /// A hold, a display that lends no token now or a frame in flight
    /// keeps it for later.
    Later,
    /// The display's buffer does not fit the view, which is made again.
    Nothing,
    Job(Box<Job>),
}

/// What [`View::present`] did with a frame composed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Presented {
    /// A frame or a cursor change was queued.
    pub queued: bool,
}

/// The interval between a display's refreshes at `refresh_mhz`, `None`
/// where the display reports no rate.
fn refresh(refresh_mhz: Option<u32>) -> Option<std::time::Duration> {
    refresh_mhz
        .filter(|&m| m > 0)
        .and_then(|m| 1_000_000_000_000_u64.checked_div(u64::from(m)))
        .map(std::time::Duration::from_nanos)
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
    /// A draw waits for the display to take a frame, as after its flip in
    /// flight, or for the frame composing.
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
    /// The areas the overlay covered in the last frame composed.
    overlaid: Vec<DisplayRect>,
    /// When the frame composing was taken.
    sent: Option<Instant>,
    /// What vtium draws over the next frame.
    pub overlay: Overlay,
    /// The overlay the last frame took, `None` when unknown.
    overlay_sent: Option<Overlay>,
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
            refresh: refresh(refresh_mhz),
            pointer: Pointer::default(),
            forgotten: Vec::new(),
            arrived: Vec::new(),
            overlaid: Vec::new(),
            sent: None,
            overlay: Overlay::default(),
            overlay_sent: None,
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
        self.drawing = Some(Box::new(Drawing { renderer }));
        self.generation = self.generation.wrapping_add(1);
        self.forgotten.clear();
        self.arrived.clear();
        self.overlaid.clear();
        self.overlay_sent = None;
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

    /// The display's new mode, which a modeset set in place on the same
    /// connector: the view takes `size` and `refresh_mhz` and draws from
    /// here on with a new renderer at that size, with `font` at the height
    /// it had, so the next frame is drawn whole. The view's size is always
    /// its renderer's: it changes only with the renderer.
    pub fn resize(
        &mut self,
        font: Font<DisplayFace>,
        size: (u32, u32),
        refresh_mhz: Option<u32>,
        config: RenderConfig,
    ) {
        self.size = size;
        self.refresh = refresh(refresh_mhz);
        self.rebuild(font, self.height, config);
    }

    /// Draws another screen from the next frame, as a console switch
    /// shows: the checkpoint the view kept is the other screen's history,
    /// so the next frame draws every cell, in every buffer.
    pub fn show_another(&mut self) {
        self.since = Age::NEVER;
        self.lost = true;
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

    /// The prepare step: takes a frame of `term` for the render thread,
    /// with the token of the display's buffer it is drawn into, unless a
    /// hold keeps the last frame, the display takes no frame now or a frame
    /// is composing. The pointer goes on the display's cursor plane where
    /// it has one. The head's frames share `snapshot`, taken at the first
    /// that needs it.
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
        if self.away() {
            self.pending = true;
            return Prepared::Later;
        }
        // The display lends a token only while it is ready for a frame.
        let Some(token) = video.lend(self.id) else {
            self.pending = true;
            return Prepared::Later;
        };
        if token.size() != self.size {
            // The view is made again at the display's new size.
            video.give_back(token);
            self.lost = true;
            self.pending = true;
            return Prepared::Nothing;
        }
        let tip = state.pointer;
        let state = DrawState {
            pointer: self.pointer.before_draw(video, self.id, &self.arrow, tip),
            ..*state
        };
        let Some(drawing) = self.drawing.take() else {
            video.give_back(token);
            return Prepared::Nothing;
        };
        let (snapshot, upto) = snapshot.get_or_insert_with(|| {
            let taken = Snapshot::take(&term.terminal);
            (Arc::new(taken), term.terminal.screen_mut().checkpoint())
        });
        let since = std::mem::replace(&mut self.since, *upto);
        let whole = self.lost;
        self.pending = false;
        self.owed = false;
        self.lost = false;
        self.sent = Some(clock());
        // An overlay that changed leaves its old areas to the renderer.
        let gone = if self.overlay_sent.as_ref() == Some(&self.overlay) {
            Vec::new()
        } else {
            std::mem::take(&mut self.overlaid)
        };
        self.overlay_sent = Some(self.overlay.clone());
        Prepared::Job(Box::new(Job {
            key: key.clone(),
            generation: self.generation,
            drawing,
            token,
            snapshot: Arc::clone(snapshot),
            since,
            state,
            tip,
            hook: hook.cloned(),
            overlay: self.overlay.clone(),
            orientation: self.orientation,
            gone,
            whole,
        }))
    }

    /// A job the render thread could not take: its token goes back to
    /// `video` unused, its drawing comes back, and the frame waits for the
    /// next draw.
    pub fn refused(&mut self, job: Job, video: &mut dyn Video) {
        video.give_back(job.token);
        if job.generation == self.generation {
            self.drawing = Some(job.drawing);
            self.since = job.since;
            self.pending = true;
            self.lost |= job.whole;
            self.overlaid.extend(job.gone);
            self.overlay_sent = None;
        }
    }

    /// The swap: takes back the drawing of `composed` and gives its token
    /// back to `video` with the frame's damage, so the display presents
    /// the buffer the frame was drawn into. A frame composed with a drawing
    /// the view no longer uses goes back unshown, and so does one that
    /// comes back after the VT was let go (`awake` false), which the
    /// display drops. The cursor plane follows the pointer, and a frame
    /// that changed nothing is still presented when the plane did.
    pub fn present(
        &mut self,
        composed: Composed,
        term: &mut Term,
        video: &mut dyn Video,
        awake: bool,
        clock: &dyn Fn() -> Instant,
    ) -> Presented {
        let Composed {
            generation,
            mut drawing,
            token,
            damage,
            overlaid,
            stats,
            at,
            tip,
            ..
        } = composed;
        if generation != self.generation {
            // A new renderer draws every buffer whole at first, so the
            // buffer the old one drew into needs no damage.
            video.give_back(token);
            return Presented::default();
        }
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
        self.drawing = Some(drawing);
        self.overlaid = overlaid;
        if !awake {
            self.lost = true;
            let _ = video.swap(token, &damage);
            return Presented::default();
        }
        self.pointer.after_draw(video, self.id, tip, at);
        let queued = match video.swap(token, &damage) {
            Ok(Swapped::Frame | Swapped::Cursor) => true,
            Ok(Swapped::Nothing) => false,
            // The display draws the frame after the flip, or whole once it
            // is ready again.
            Err(SwapError::Busy | SwapError::Dropped) => {
                self.pending = true;
                false
            }
            Err(SwapError::Off) => false,
            Err(SwapError::Failed(e)) => {
                log::warn!("cannot swap display {}: {e}", video.name(self.id));
                false
            }
        };
        // the display gave the plane up: the renderer draws the pointer
        self.pending |= self.pointer.after_swap(video, self.id);
        if let Some(timer) = &mut self.timer {
            timer.frame(stats.cpu, queued, stats.damage_px, stats.misses);
        }
        let end = clock();
        let start = self.sent.take().unwrap_or(end);
        term.coalesce.drawn(&mut self.wait, start, end, queued);
        Presented { queued }
    }

    /// Draws `term` into the buffer `video` lends and presents it at once,
    /// the frame composed on the calling thread: [`View::prepare`],
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

    use std::sync::Mutex;

    use vtium_font::Unifont;
    use vtium_platform::display::{Dpms, Event, Memory};
    use vtium_render::Arrow;

    use super::*;

    const ID: DisplayId = DisplayId(1);
    const SIZE: (u32, u32) = (36, 34);

    /// A display in memory, with a cursor plane or without: what each swap
    /// carried, and where the plane shows the pointer.
    struct Plane {
        has_plane: bool,
        pixels: Arc<Mutex<Vec<u8>>>,
        age: u32,
        /// A token is out.
        out: bool,
        /// The VT was let go: a token back now is dropped.
        asleep: bool,
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
                pixels: Arc::new(Mutex::new(vec![0; (SIZE.0 * SIZE.1 * 4) as usize])),
                age: 0,
                out: false,
                asleep: false,
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
            u32::from_ne_bytes(self.pixels.lock().unwrap()[i..i + 4].try_into().unwrap())
        }
    }

    /// A token for `memory`, a buffer of `SIZE` at `age`.
    fn token(memory: &Arc<Mutex<Vec<u8>>>, age: u32) -> Token {
        let memory: Arc<Memory> = memory.clone();
        Token::new(ID, SIZE, SIZE.0 * 4, age, memory)
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
        fn lend(&mut self, _: DisplayId) -> Option<Token> {
            if std::mem::replace(&mut self.out, true) {
                return None;
            }
            Some(token(&self.pixels, self.age))
        }
        fn give_back(&mut self, _: Token) {
            self.out = false;
        }
        fn swap(&mut self, _: Token, damage: &[DisplayRect]) -> Result<Swapped, SwapError> {
            self.out = false;
            if self.asleep {
                self.age = 0;
                return Err(SwapError::Dropped);
            }
            if damage.is_empty() && !self.need_commit(ID) {
                return Ok(Swapped::Nothing);
            }
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
        fn ready_for_frame(&self, id: DisplayId) -> bool {
            !self.is_flipping(id) && !self.out
        }
        fn set_dpms(&mut self, _: DisplayId, _: Dpms) -> io::Result<()> {
            Ok(())
        }
        fn sleep(&mut self) {
            self.asleep = true;
        }
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

    // ADR-1401 D5: with no color key set, the notices and the on-screen
    // display draw in vtium-render's own colors for them, which its tests
    // and figures use.
    #[test]
    fn unset_color_keys_are_vtium_renders_notice_and_status_colors() {
        assert_eq!(
            OverlayColors::default(),
            OverlayColors {
                notice: Colors::NOTICE,
                osd: Colors::STATUS,
            }
        );
    }

    // The focus mark's cell is where the renderer draws the grid's top-right
    // cell, in each orientation: a full block there, drawn, lights exactly
    // the rectangle `top_right` gives.
    #[test]
    fn top_right_is_the_cell_the_renderer_draws_top_right() {
        for (o, w, h) in [
            (Orientation::Normal, 100, 60),
            (Orientation::UpsideDown, 100, 60),
            (Orientation::Right, 60, 100),
            (Orientation::Left, 60, 100),
        ] {
            let mut terminal = Terminal::new(10, 3, 0);
            terminal.input("\x1b[1;10H\u{2588}".as_bytes());
            let face: DisplayFace = Box::new(Unifont::new(16));
            let config = RenderConfig {
                orientation: o,
                ..RenderConfig::default()
            };
            let mut renderer = Renderer::new(Font::new(face), w, h, config);
            let mut canvas = vtium_render::Canvas::new(w, h, 0);
            let snapshot = Snapshot::take(&terminal);
            renderer.draw(
                &snapshot.frame(Age::NEVER),
                &mut canvas,
                0,
                &DrawState::default(),
            );
            let bg = canvas.pixel(0, 0);
            let lit: Vec<(u32, u32)> = (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|&(x, y)| canvas.pixel(x, y) != bg)
                .collect();
            let (x0, y0) = (lit.iter().map(|p| p.0).min(), lit.iter().map(|p| p.1).min());
            let (x1, y1) = (lit.iter().map(|p| p.0).max(), lit.iter().map(|p| p.1).max());
            let drawn = RenderRect {
                x: x0.unwrap(),
                y: y0.unwrap(),
                width: x1.unwrap() - x0.unwrap() + 1,
                height: y1.unwrap() - y0.unwrap() + 1,
            };
            let surface = Surface {
                width: w,
                height: h,
                orientation: o,
                depth: renderer.depth(),
            };
            let cell = renderer.font().cell();
            assert_eq!(top_right(surface, (10, 3), cell), Some(drawn), "{o:?}");
        }
        let surface = Surface {
            width: 100,
            height: 60,
            orientation: Orientation::Normal,
            depth: Depth::Color,
        };
        let cell = vtium_font::Cell {
            width: 8,
            height: 16,
        };
        assert_eq!(top_right(surface, (0, 3), cell), None, "an empty grid");
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

    /// A display with `n` buffers, taking turns at each swap: the buffer
    /// lent is the back one, its age the frames drawn since it was last.
    struct Flip {
        bufs: Vec<Arc<Mutex<Vec<u8>>>>,
        back: usize,
        drawn: Vec<Option<u32>>,
        count: u32,
        out: bool,
    }

    impl Flip {
        fn new() -> Flip {
            Flip::with(2)
        }

        fn with(n: usize) -> Flip {
            let len = (SIZE.0 * SIZE.1 * 4) as usize;
            Flip {
                bufs: (0..n).map(|_| Arc::new(Mutex::new(vec![0; len]))).collect(),
                back: 0,
                drawn: vec![None; n],
                count: 0,
                out: false,
            }
        }

        /// Buffer `b`'s pixels.
        fn pixels(&self, b: usize) -> Vec<u8> {
            self.bufs[b].lock().unwrap().clone()
        }

        /// Whether buffer `b` shows anything in the grid's cell `col` of
        /// row 0, the grid at 2,1 in 8x16 cells.
        fn shows(&self, b: usize, col: u32) -> bool {
            (1..17).any(|y| {
                (2 + col * 8..10 + col * 8).any(|x| {
                    let i = ((y * SIZE.0 + x) * 4) as usize;
                    self.bufs[b].lock().unwrap()[i..i + 3]
                        .iter()
                        .any(|&p| p != 0)
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
        fn lend(&mut self, _: DisplayId) -> Option<Token> {
            if std::mem::replace(&mut self.out, true) {
                return None;
            }
            let age = self.drawn[self.back].map_or(0, |d| self.count - d + 1);
            Some(token(&self.bufs[self.back], age))
        }
        fn give_back(&mut self, _: Token) {
            self.out = false;
        }
        fn swap(&mut self, _: Token, damage: &[DisplayRect]) -> Result<Swapped, SwapError> {
            self.out = false;
            if damage.is_empty() {
                return Ok(Swapped::Nothing);
            }
            self.count += 1;
            self.drawn[self.back] = Some(self.count);
            self.back = (self.back + 1) % self.bufs.len();
            Ok(Swapped::Frame)
        }
        fn is_flipping(&self, _: DisplayId) -> bool {
            false
        }
        fn ready_for_frame(&self, id: DisplayId) -> bool {
            !self.is_flipping(id) && !self.out
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

    // A buffer two frames old is drawn with what the frame before changed
    // too: A goes into one buffer, B into the other, and C's buffer, which
    // last held A, shows A, B and C.
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

    // ADR-1401 D2: the renderer draws into the token's buffer what it lacks
    // at its age. With three buffers D's buffer, which last held A, shows
    // A to D; with one, every frame draws into the buffer shown.
    #[test]
    fn each_buffer_is_drawn_what_it_lacks_at_its_age() {
        for (n, want) in [
            (1, vec![vec![true, true, true, true]]),
            (
                3,
                vec![
                    vec![true, true, true, true],
                    vec![true, true, false, false],
                    vec![true, true, true, false],
                ],
            ),
        ] {
            let (mut term, mut view) = view();
            let mut video = Flip::with(n);
            for text in [b"A", b"B", b"C", b"D"] {
                term.terminal.input(text);
                draw(&mut term, &mut view, &mut video, None);
            }
            let shown: Vec<Vec<bool>> = (0..n)
                .map(|b| (0..4).map(|col| video.shows(b, col)).collect())
                .collect();
            assert_eq!(shown, want, "{n} buffers");
        }
    }

    // A notice drawn over the frame and gone is gone from every buffer: the
    // renderer draws the cells under it again in the buffer two frames old
    // too, so both equal a display that never showed it.
    #[test]
    fn a_notice_gone_leaves_no_buffer_showing_it() {
        let (mut plain_term, mut plain) = view();
        let mut never = Flip::new();
        plain_term.terminal.input(b"AB");
        draw(&mut plain_term, &mut plain, &mut never, None);
        let notice = Overlay::new(None, &["a notice"], None, OverlayColors::default());
        let (mut term, mut view) = view();
        let mut video = Flip::new();
        term.terminal.input(b"AB");
        view.overlay = notice;
        for _ in 0..2 {
            draw(&mut term, &mut view, &mut video, None);
        }
        assert_ne!(video.pixels(0), never.pixels(0), "the notice shows");
        assert_ne!(video.pixels(1), never.pixels(0), "in both buffers");
        view.overlay = Overlay::default();
        for _ in 0..2 {
            draw(&mut term, &mut view, &mut video, None);
        }
        assert_eq!(video.pixels(0), never.pixels(0), "the buffer drawn first");
        assert_eq!(
            video.pixels(1),
            never.pixels(0),
            "the buffer two frames old"
        );
    }

    // ADR-1300 D5: the token's buffer is only written. Drawn into a buffer
    // full of garbage at age 0, the frame equals one drawn into zeros.
    #[test]
    fn what_is_drawn_does_not_depend_on_what_the_buffer_held() {
        let mut shown = Vec::new();
        for fill in [0x00, 0xa5] {
            let (mut term, mut view) = view();
            let mut video = Plane::new(false);
            video.pixels.lock().unwrap().fill(fill);
            term.terminal.input(b"\x1b[4;31mA\x1b[mB");
            draw(&mut term, &mut view, &mut video, Some((5, 6)));
            shown.push(video.pixels.lock().unwrap().clone());
        }
        assert_eq!(shown[0], shown[1]);
    }

    // ADR-1401 D2: no view keeps a copy of its display's frame. At
    // 1920x1080 the frame is 8,294,400 bytes; the drawing holds the row
    // scratch alone.
    #[test]
    fn a_views_drawing_holds_no_whole_frame() {
        let face: DisplayFace = Box::new(Unifont::new(16));
        let size = (1920, 1080);
        let mut view = View::new(
            ID,
            Font::new(face),
            size,
            None,
            RenderConfig::default(),
            false,
        );
        let clock: Clock = Rc::new(Instant::now);
        let mut term = Term::new(
            Terminal::new(240, 67, 0),
            Sync::new(Duration::ZERO, 0),
            Blink::new(false),
            clock,
        );
        let memory = Arc::new(Mutex::new(vec![0u8; 1920 * 1080 * 4]));
        let mut video = Large(memory, 0);
        term.terminal.input(b"a full screen");
        assert!(view.draw(&mut term, &mut video, &DrawState::default(), &Instant::now));
        let held = view.drawing.as_ref().unwrap().frame_bytes();
        assert!(held < 1920 * 1080 * 4 / 16, "{held} bytes");
    }

    /// A display of 1920x1080 in memory.
    struct Large(Arc<Mutex<Vec<u8>>>, u32);

    impl Video for Large {
        fn backend(&self) -> &'static str {
            "large"
        }
        fn events(&mut self) -> Vec<Event> {
            Vec::new()
        }
        fn displays(&self) -> Vec<DisplayId> {
            vec![ID]
        }
        fn name(&self, _: DisplayId) -> String {
            "LARGE-1".into()
        }
        fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
            Some((1920, 1080))
        }
        fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
            None
        }
        fn lend(&mut self, id: DisplayId) -> Option<Token> {
            let memory: Arc<Memory> = self.0.clone();
            Some(Token::new(id, (1920, 1080), 1920 * 4, 0, memory))
        }
        fn give_back(&mut self, _: Token) {}
        fn swap(&mut self, _: Token, damage: &[DisplayRect]) -> Result<Swapped, SwapError> {
            self.1 += u32::from(!damage.is_empty());
            Ok(Swapped::Frame)
        }
        fn is_flipping(&self, _: DisplayId) -> bool {
            false
        }
        fn ready_for_frame(&self, _: DisplayId) -> bool {
            true
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

    // A frame composed with a renderer the view has since replaced, as a
    // zoom replaces it, is not shown, and its token goes back to the
    // display, which then lends again.
    #[test]
    fn a_frame_of_an_older_renderer_gives_its_token_back() {
        let (mut term, mut view) = view();
        let mut video = Plane::new(false);
        let key = (PathBuf::new(), ID);
        let mut snapshot = None;
        let Prepared::Job(job) = view.prepare(
            &key,
            &mut term,
            &mut snapshot,
            &mut video,
            &DrawState::default(),
            None,
            &Instant::now,
        ) else {
            panic!("a job");
        };
        let face: DisplayFace = Box::new(Unifont::new(16));
        view.rebuild(Font::new(face), 32, RenderConfig::default());
        let shown = view.present(job.compose(), &mut term, &mut video, true, &Instant::now);
        assert_eq!(shown, Presented::default());
        assert_eq!(video.swaps, [], "not shown");
        assert!(!video.out, "the token went back");
        draw(&mut term, &mut view, &mut video, None);
        assert_eq!(video.swaps, [1], "the display lends again");
    }

    // A job refused for a display that lost its picture keeps it lost, so
    // the frame drawn at the next turn is drawn whole and through a hold.
    #[test]
    fn a_job_refused_keeps_its_display_lost() {
        let (mut term, mut view) = view();
        let mut video = Plane::new(false);
        assert!(view.lost, "a new view draws whole");
        let key = (PathBuf::new(), ID);
        let mut snapshot = None;
        let Prepared::Job(job) = view.prepare(
            &key,
            &mut term,
            &mut snapshot,
            &mut video,
            &DrawState::default(),
            None,
            &Instant::now,
        ) else {
            panic!("a job");
        };
        assert!(!view.lost);
        view.refused(*job, &mut video);
        assert!(view.lost, "still lost");
    }

    // A job the render thread could not take gives its token back, so the
    // display lends again at the next draw.
    #[test]
    fn a_job_refused_gives_its_token_back() {
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
        assert!(video.out && !video.ready_for_frame(ID), "one token out");
        view.refused(*job, &mut video);
        assert!(!video.out && !view.away() && view.pending);
        draw(&mut term, &mut view, &mut video, None);
        assert_eq!(video.swaps, [1], "the next draw lends and shows");
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
    // release is not shown: its token goes back to the display, which
    // drops it, nothing is swapped, and the display draws whole at the next
    // frame.
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
        video.sleep();
        let shown = view.present(job.compose(), &mut term, &mut video, false, &Instant::now);
        assert_eq!(shown, Presented::default());
        assert_eq!(video.swaps, [], "nothing swapped");
        assert_eq!(video.age, 0, "the display drew its next frame whole");
        assert!(!video.out, "the token went back");
        assert!(view.lost, "drawn whole at the next frame");
        assert!(!view.away(), "the drawing is back");
    }

    // A frame that changed nothing presents nothing, and its token goes
    // back.
    #[test]
    fn a_frame_that_changed_nothing_presents_nothing() {
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
        assert!(!video.out);
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

    // The pointer's log line names the display as the video device names
    // it, by its connector.
    #[test]
    fn the_pointer_line_names_the_display_by_its_connector() {
        let (mut term, mut view) = view();
        let mut video = Plane::new(true);
        crate::pointer::said();
        draw(&mut term, &mut view, &mut video, Some((5, 6)));
        assert_eq!(
            crate::pointer::said(),
            [(
                log::Level::Info,
                "display PLANE-1 shows it on its cursor plane".to_owned()
            )]
        );
    }

    // Each zoom rebuilds the view, and the next draw offers the new arrow
    // to the plane again. The decision, the same each time, is logged at
    // info level once and after that at debug level.
    #[test]
    fn a_zoom_twice_logs_the_pointers_route_at_info_once() {
        let (mut term, mut view) = view();
        let mut video = Plane::new(true);
        crate::pointer::said();
        draw(&mut term, &mut view, &mut video, Some((5, 6)));
        for height in [32, 16] {
            let face: DisplayFace = Box::new(Unifont::new(height));
            view.rebuild(Font::new(face), height, RenderConfig::default());
            draw(&mut term, &mut view, &mut video, Some((5, 6)));
            assert_eq!(
                video.arrow.as_ref(),
                Some(view.arrow()),
                "the plane took the new arrow"
            );
        }
        let line = "display PLANE-1 shows it on its cursor plane".to_owned();
        assert_eq!(
            crate::pointer::said(),
            [
                (log::Level::Info, line.clone()),
                (log::Level::Debug, line.clone()),
                (log::Level::Debug, line),
            ]
        );
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
