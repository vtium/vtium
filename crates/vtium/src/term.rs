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
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use vtium_font::{Face, Font, Missing, SpritePolicy};
use vtium_platform::display::{DisplayId, Rect as DisplayRect, SwapError, Swapped, Token};
use vtium_platform::pty::{Consumer, Replies};
use vtium_render::{
    Arrow, Canvas, Colors, Config as RenderConfig, Depth, DrawState, Mapped, Mark, Notice,
    Orientation, Rect as RenderRect, RenderTimer, Renderer, Stats, Surface, Target,
};
use vtium_term::{Age, Cursor, CursorStyle, Damage, Frame, Line, LinkId, Palette, Rgb, Terminal};

use crate::blink::Blink;
use crate::config::Config;
use crate::config::keys::{OVERLAY_COLORS, Scheme};
use crate::font::DisplayFace;
use crate::head::DrawHook;
use crate::pointer::Pointer;
use crate::sync::{Action, Coalesce, Draw, Sync, Wait};
use crate::video::Video;

/// The clock the terminal reads while it parses: the loop's, or a test's.
pub type Clock = Rc<dyn Fn() -> Instant>;

/// A session's terminal and the rules for when it presents.
pub struct Term {
    pub terminal: Terminal,
    pub sync: Sync,
    pub coalesce: Coalesce,
    pub blink: Blink,
    /// Which frame each animated picture of the console shows (ADR-1102
    /// D3).
    #[cfg(feature = "images")]
    pub animation: crate::animation::AnimationClock,
    clock: Clock,
    /// The pty holds output the last input call left.
    pub unparsed: bool,
    /// Output changed the screen this turn: a cell, a picture or the
    /// cursor. Output that changes none of them, such as a sixel picture's
    /// data before its end, asks no display to draw.
    pub changed: bool,
    /// A frame end or a timeout asks every display of the head to present.
    pub present: bool,
    /// The child exited and its output has been read.
    pub exited: Option<ExitStatus>,
    /// Bytes of the child's output parsed since the start.
    pub parsed: u64,
    /// The keys sent to the program since the start ([`Term::key_sent`]).
    keys: u64,
}

impl Term {
    pub fn new(terminal: Terminal, sync: Sync, blink: Blink, clock: Clock) -> Term {
        Term {
            terminal,
            sync,
            coalesce: Coalesce::default(),
            blink,
            #[cfg(feature = "images")]
            animation: crate::animation::AnimationClock::default(),
            clock,
            unparsed: false,
            changed: false,
            present: false,
            exited: None,
            parsed: 0,
            keys: 0,
        }
    }

    /// A key went to the program. It opens a key's window on each display:
    /// every change the program's output makes to the screen draws at once
    /// until the display's next flip completes ([`View::flipped`],
    /// ADR-1401 D2).
    pub fn key_sent(&mut self) {
        self.keys = self.keys.wrapping_add(1);
    }

    /// The keys sent to the program since the start. A view keeps the
    /// count from when its display's last flip completed, and while the
    /// two differ, a key's window is open on that display.
    pub fn keys(&self) -> u64 {
        self.keys
    }

    /// What a hold's rules asked for.
    pub fn act(&mut self, action: Action) {
        match action {
            Action::Present | Action::Expired => self.present = true,
            Action::None | Action::Arm => {}
        }
    }

    /// Parses what the terminal takes of `output`, the child's output, and
    /// passes the terminal's replies to `reply`. Returns the bytes taken,
    /// as [`Consumer::output`] does, which reads the pty through it.
    pub(crate) fn parse(&mut self, output: &[u8], reply: &mut dyn FnMut(&[u8])) -> usize {
        // The output after an OSC 50 request waits for the turn that
        // switches the font, so a query that follows names the new set,
        // the output after an XTWINOPS 8 request for the turn that
        // resizes, so a size report that follows answers the new size, and
        // the output after setBackground or setForeground for the turn that
        // takes it, so the seat takes each in order.
        if self.terminal.font_requested()
            || self.terminal.grid_requested()
            || self.terminal.display_requested()
        {
            return 0;
        }
        let start = (self.clock)();
        let before = Shown::now(&mut self.terminal);
        let taken = self.terminal.input(output);
        let out = self.terminal.take_replies();
        if !out.is_empty() {
            reply(&out);
        }
        let end = (self.clock)();
        self.coalesce.input(start, end, taken);
        self.unparsed = taken < output.len();
        self.changed |= before.changed(&self.terminal);
        self.parsed = self
            .parsed
            .saturating_add(u64::try_from(taken).unwrap_or(u64::MAX));
        let action = self.sync.input(&mut self.terminal, end, self.unparsed);
        self.act(action);
        taken
    }
}

/// What a frame shows of the screen before an input call, to tell
/// whether the call changed it.
struct Shown {
    /// A checkpoint of the cells' damage: a cell marked after it changed,
    /// written, erased, scrolled, or under a picture added, moved or
    /// removed, or every cell at once, as a resize, a palette change or a
    /// switch of screens marks them.
    since: Age,
    /// The cursor's place, DECTCEM and DECSCUSR. A frame carries the
    /// cursor, and moving it marks no cell, so a cursor change changes the
    /// screen without any cell's damage. OSC 12's color is the palette's,
    /// and a palette change marks every cell.
    cursor: ((usize, usize), bool, CursorStyle),
}

impl Shown {
    fn now(terminal: &mut Terminal) -> Shown {
        Shown {
            since: terminal.screen_mut().checkpoint(),
            cursor: Shown::cursor(terminal),
        }
    }

    fn cursor(terminal: &Terminal) -> ((usize, usize), bool, CursorStyle) {
        let screen = terminal.screen();
        (
            screen.cursor(),
            screen.cursor_visible(),
            terminal.cursor_style(),
        )
    }

    /// Whether `terminal`'s screen changed since: a cell or the cursor.
    fn changed(&self, terminal: &Terminal) -> bool {
        let damage = terminal.screen().damage();
        self.cursor != Shown::cursor(terminal)
            || (0..damage.rows()).any(|row| damage.row_changed(row, self.since))
    }
}

impl Consumer for Term {
    fn output(&mut self, output: &[u8], replies: &mut Replies<'_>) -> usize {
        self.parse(output, &mut |reply| replies.write(reply))
    }

    fn exited(&mut self, status: ExitStatus) {
        // the next session starts with no picture upload left open
        self.terminal.session_ended();
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
/// change ages, the palette, the cursor, the selection, the hovered link,
/// and in a build with images the pictures, each sharing its image's
/// pixels (ADR-1100 D9), and in a build with right-to-left text the order
/// each row is drawn in (ADR-1004 D8).
pub struct Snapshot {
    lines: Vec<Line>,
    damage: Damage,
    palette: Palette,
    cursor: Cursor,
    reverse: bool,
    selection: Vec<Range<usize>>,
    hovered: Option<LinkId>,
    overlay: Option<(usize, String)>,
    #[cfg(feature = "images")]
    placements: Vec<vtium_image::FramePlacement>,
    /// The order each row is drawn in, which the frame the snapshot
    /// rebuilds reads its cells through.
    #[cfg(feature = "bidi")]
    order: vtium_term::Order,
}

impl Snapshot {
    /// What `terminal`'s view shows now.
    pub fn take(terminal: &Terminal) -> Snapshot {
        let screen = terminal.screen();
        let palette = terminal.palette().clone();
        let frame = screen.frame(&palette, Age::NEVER);
        let rows = frame.rows();
        let lines = (0..rows).filter_map(|r| frame.line(r)).cloned().collect();
        // the selected columns of each row, as the screen keeps them:
        // logical columns, which a row drawn in another order shows in
        // pieces (ADR-1004 D10)
        let selection = frame.selection().to_vec();
        #[cfg(feature = "bidi")]
        let order = frame.order().clone();
        let (cursor, hovered) = (frame.cursor(), frame.hovered());
        let overlay = frame.overlay().map(|o| (o.row, o.uri.to_owned()));
        #[cfg(feature = "images")]
        let placements = frame.placements().to_vec();
        Snapshot {
            lines,
            damage: screen.damage().clone(),
            cursor,
            reverse: screen.reverse_video(),
            selection,
            hovered,
            overlay,
            palette,
            #[cfg(feature = "images")]
            placements,
            #[cfg(feature = "bidi")]
            order,
        }
    }

    /// The frame a display that last drew at `since` reads.
    fn frame(&self, since: Age) -> Frame<'_> {
        let frame = Frame::new(
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
        );
        #[cfg(feature = "images")]
        let frame = frame.with_placements(self.placements.clone());
        // the cursor came from the screen's frame already drawn where its
        // cell is, so only the cells take the order
        #[cfg(feature = "bidi")]
        let frame = frame.with_order(self.order.clone());
        frame
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
    /// The colors vtium.conf's six color keys set, each unset key at its
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
    /// only written, as [`Overlay::draw`] does. Every character of a notice
    /// and the mark comes from the built-in fonts in the renderer's cell
    /// (ADR-1401 D5, D7): from the face's [`Face::builtins`] where it draws
    /// a font set's glyphs over them, as a frit's face does, and otherwise
    /// from the renderer's face, a built-in font itself. The renderer's
    /// sprite policy and missing glyph hold for either.
    pub fn draw_into<F: Face>(
        &self,
        renderer: &Renderer<F>,
        grid: (usize, usize),
        orientation: Orientation,
        size: (u32, u32),
        target: &mut impl Target,
    ) -> Vec<DisplayRect> {
        let own = renderer.font();
        let depth = renderer.depth();
        match own.face().builtins() {
            Some(face) => {
                let font = Font::new(face)
                    .with_policy(own.policy())
                    .with_missing(own.missing());
                self.draw_with(&font, depth, grid, orientation, size, target)
            }
            None => self.draw_with(own, depth, grid, orientation, size, target),
        }
    }

    /// Draws the overlay into `target` with `font`, as
    /// [`Overlay::draw_into`] does.
    fn draw_with<F: Face>(
        &self,
        font: &Font<F>,
        depth: Depth,
        grid: (usize, usize),
        orientation: Orientation,
        (width, height): (u32, u32),
        target: &mut impl Target,
    ) -> Vec<DisplayRect> {
        let surface = Surface {
            width,
            height,
            orientation,
            depth,
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
    /// picture at age 0, and the overlay goes over them. With `gpu`, the
    /// head's GPU path, the frame is drawn through the display's pipeline
    /// and what the buffer lacks is read back into it; a frame the GPU path
    /// fails, or does not draw, is drawn on the CPU path whole.
    pub fn compose(self, gpu: OnGpu<'_>) -> Composed {
        #[cfg(not(feature = "gpu"))]
        let _ = gpu;
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
        let mut failed_gpu = None;
        let mut refused_gpu = None;
        let drawn = token
            .draw(|canvas| {
                let stride = usize::try_from(canvas.stride).ok()?;
                let mut target = Mapped::new(canvas.pixels, size.0, size.1, stride)?;
                let age = if self.whole { 0 } else { canvas.age };
                #[cfg(feature = "gpu")]
                let on_gpu = gpu.and_then(|g| {
                    g.draw(
                        &self.key,
                        &mut drawing.renderer,
                        &frame,
                        (&mut target, size),
                        self.orientation,
                        age,
                        &self.state,
                    )
                });
                #[cfg(not(feature = "gpu"))]
                let on_gpu: Option<OnGpuFrame> = None;
                let (rects, stats) = match on_gpu {
                    Some(OnGpuFrame::Drawn(damage, stats)) => (damage, stats),
                    other => {
                        match other {
                            // after a failed GPU frame the buffer may hold
                            // part of it, and the renderer may have recorded
                            // it as drawn: the renderer forgets its history,
                            // so the CPU path draws the frame whole at any age
                            Some(OnGpuFrame::Failed(why)) => {
                                failed_gpu = Some(why);
                                drawing.renderer.forget_history();
                            }
                            Some(OnGpuFrame::Refused(why)) => refused_gpu = Some(why),
                            Some(OnGpuFrame::Drawn(..)) | None => {}
                        }
                        let drawn = drawing.renderer.draw(&frame, &mut target, age, &self.state);
                        (drawn.damage.to_vec(), drawn.stats)
                    }
                };
                let mut damage: Vec<DisplayRect> = rects
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
            failed_gpu,
            refused_gpu,
            back_on_gpu: false,
            left_gpu: None,
        }
    }
}

/// What the head's GPU path did with a display's frame (ADR-1303 D9).
pub enum OnGpuFrame {
    /// It drew the frame into the display's buffer: the damage and the
    /// figures.
    Drawn(Vec<RenderRect>, Stats),
    /// The frame failed, for this reason, and the CPU path draws it.
    Failed(String),
    /// The display's set-up failed at its size, for this reason, and the
    /// CPU path draws the whole head while the display keeps that size.
    Refused(String),
}

/// The head's GPU path, which a frame is drawn through when there is one.
#[cfg(feature = "gpu")]
pub type OnGpu<'a> = Option<&'a mut crate::gpu::head::HeadGpu>;

/// A build without `gpu` has no GPU path: always `None`.
#[cfg(not(feature = "gpu"))]
pub type OnGpu<'a> = Option<&'a mut std::convert::Infallible>;

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
    /// Why the GPU path failed this frame, which the CPU path drew
    /// instead (ADR-1303 D9).
    pub failed_gpu: Option<String>,
    /// Why the GPU path could not set up the display at this frame's size:
    /// the CPU path draws the whole head until the display's size changes
    /// or it leaves the head (ADR-1303 D9).
    pub refused_gpu: Option<String>,
    /// The head draws on the GPU path again from this frame, since no
    /// display of it is at a size the GPU path could not set up (ADR-1303
    /// D9).
    pub back_on_gpu: bool,
    /// Why the head's GPU path ended with this frame: the head draws on
    /// the CPU path from now on (ADR-1303 D9).
    pub left_gpu: Option<String>,
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

/// What [`View::present`] did with a frame composed, or
/// [`View::send_kept`] with a frame the display kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Presented {
    /// A frame or a cursor change was queued.
    pub queued: bool,
    /// The time the frames queued took to compose: the frame, with the
    /// frame kept from a busy swap that it was drawn over.
    pub compose: std::time::Duration,
}

/// What a frame cost to compose, as the render timer counts it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Cost {
    cpu: std::time::Duration,
    damage_px: u64,
    misses: u32,
}

impl Cost {
    /// This cost with `kept`'s, the cost of a frame the display kept from
    /// a busy swap, which the frame now swapped was drawn over and carries.
    fn with(self, kept: Option<Cost>) -> Cost {
        let Some(kept) = kept else {
            return self;
        };
        Cost {
            cpu: self.cpu.saturating_add(kept.cpu),
            damage_px: self.damage_px.saturating_add(kept.damage_px),
            misses: self.misses.saturating_add(kept.misses),
        }
    }
}

/// The interval between a display's refreshes at `refresh_mhz`, `None`
/// where the display reports no rate.
fn refresh(refresh_mhz: Option<u32>) -> Option<std::time::Duration> {
    refresh_mhz
        .filter(|&m| m > 0)
        .and_then(|m| 1_000_000_000_000_u64.checked_div(u64::from(m)))
        .map(std::time::Duration::from_nanos)
}

/// The number the next drawing any view makes takes. Numbering every
/// drawing apart, across views, keeps a frame composing for a view that
/// is gone from being shown by the view made after it for the same
/// display, as when a monitor swapped on its connector is removed and
/// added in one turn.
static NEXT_DRAWING: AtomicU64 = AtomicU64::new(1);

/// One display drawing the terminal.
pub struct View {
    pub id: DisplayId,
    /// Here between frames; `None` while the render thread composes one.
    drawing: Option<Box<Drawing>>,
    /// The number of the drawing the view draws with, which no other
    /// drawing has: a frame composed with another is dropped.
    generation: u64,
    /// The terminal's checkpoint after this view's last frame was taken.
    since: Age,
    pub wait: Wait,
    /// The term's [`Term::keys`] when the display's last flip completed,
    /// or `None` after a switch to another console's term until the next
    /// flip completes. While the term has counted a key since, a key's
    /// window is open on this display ([`View::flipped`], ADR-1401 D2).
    flipped_keys: Option<u64>,
    /// A draw waits for the display to take a frame, as after its flip in
    /// flight, or for the frame composing.
    pub pending: bool,
    /// The cost of the frame the display refused as busy while a flip was
    /// in flight. The display keeps that frame, and the view sends it once
    /// the flip ends ([`View::send_kept`]), unless a newer frame is drawn
    /// over it first and carries it.
    kept: Option<Cost>,
    /// A draw waits for the hold's end.
    pub owed: bool,
    /// The display lost its picture and draws whole, through a hold.
    pub lost: bool,
    /// The log said once that the GPU path failed one of the display's
    /// frames.
    pub gpu_failure_logged: bool,
    pub timer: Option<RenderTimer>,
    /// The render timer's line for the display, which the console draws
    /// in the on-screen display's box under `render-timer=osd` or `both`:
    /// the figures of the last second the timer reported.
    pub timer_line: Option<String>,
    pub size: (u32, u32),
    pub refresh: Option<std::time::Duration>,
    /// The height the display's font was asked for.
    pub height: u32,
    pub orientation: vtium_render::Orientation,
    /// Who draws the pointer: the display's cursor plane or the renderer.
    pub pointer: Pointer,
    /// The drawing's font cell, the rows line spacing adds to it, arrow,
    /// depth and font policy, read while the render thread holds it.
    cell: vtium_font::Cell,
    band: vtium_font::spacing::Band,
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
            band: renderer.font().face().band(),
            arrow: renderer.arrow().clone(),
            depth: renderer.depth(),
            policy: (renderer.font().policy(), renderer.font().missing()),
            drawing: None,
            generation: 0,
            since: Age::NEVER,
            wait: Wait::default(),
            flipped_keys: Some(0),
            pending: false,
            kept: None,
            owed: false,
            lost: true,
            gpu_failure_logged: false,
            timer: measure.then(RenderTimer::new),
            timer_line: None,
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
        self.band = renderer.font().face().band();
        self.arrow = renderer.arrow().clone();
        self.depth = renderer.depth();
        self.policy = (renderer.font().policy(), renderer.font().missing());
        self.drawing = Some(Box::new(Drawing { renderer }));
        self.generation = NEXT_DRAWING.fetch_add(1, Ordering::Relaxed);
        self.forgotten.clear();
        self.arrived.clear();
        self.overlaid.clear();
        self.overlay_sent = None;
        self.drop_kept();
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

    /// The display's new size, which a modeset set in place on the same
    /// connector: the view takes `size` and draws from here on with a new
    /// renderer at that size, with `font` at the height it had, so the
    /// next frame is drawn whole. The view's size is always its
    /// renderer's: it changes only with the renderer.
    pub fn resize(&mut self, font: Font<DisplayFace>, size: (u32, u32), config: RenderConfig) {
        self.size = size;
        self.rebuild(font, self.height, config);
    }

    /// The display's refresh rate, `refresh_mhz`, read again at a set-up
    /// that may have given it a new mode: the view's frames are paced at
    /// its interval.
    pub fn set_refresh(&mut self, refresh_mhz: Option<u32>) {
        self.refresh = refresh(refresh_mhz);
    }

    /// Draws another screen from the next frame, as a console switch
    /// shows: the checkpoint the view kept is the other screen's history,
    /// so the next frame draws every cell, in every buffer.
    pub fn show_another(&mut self) {
        self.since = Age::NEVER;
        self.lost = true;
        self.drop_kept();
        // The other screen's term counts its own keys.
        self.flipped_keys = None;
    }

    /// The display's flip completed, as its flip event says, while it shows
    /// `term`. A key opens a window on each display, and the window closes
    /// at the first flip of that display that completes after the key was
    /// sent: this one. A key sent while the window is open closes with it
    /// (ADR-1401 D2).
    pub fn flipped(&mut self, term: &Term) {
        self.flipped_keys = Some(term.keys());
    }

    /// Whether a key's window is open on the display: a key went to the
    /// program since the display's last flip completed. Inside the window,
    /// every change of the screen draws at once.
    pub(crate) fn after_key(&self, term: &Term) -> bool {
        self.flipped_keys != Some(term.keys())
    }

    /// Whether the view's renderer draws pictures, which a head needs of
    /// every display before it offers sixel and kitty graphics (ADR-1100
    /// D10): the CPU compositor draws them in a build with images.
    pub fn draws_placements(&self) -> bool {
        vtium_render::DRAWS_PLACEMENTS
    }

    /// The font's cell.
    pub fn cell(&self) -> vtium_font::Cell {
        self.cell
    }

    /// The rows line spacing adds to the font's cell above and below its
    /// text band (ADR-1302 D13).
    pub fn band(&self) -> vtium_font::spacing::Band {
        self.band
    }

    /// The font's cell with the rows line spacing adds to it: what a
    /// frit's glyphs for this view are asked for and held by.
    pub fn spaced(&self) -> vtium_font::spacing::SpacedCell {
        vtium_font::spacing::SpacedCell {
            cell: self.cell,
            band: self.band,
        }
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

    /// Whether the next frame draws again cells that showed the font's
    /// missing glyph after an arrival ([`Renderer::redraws_missing`]).
    /// While a frame is composing, whether an arrival waits to reach the
    /// drawing, which may have such cells.
    pub fn redraws_missing(&self) -> bool {
        match &self.drawing {
            Some(d) => d.renderer.redraws_missing(),
            None => !self.forgotten.is_empty() || !self.arrived.is_empty(),
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

    /// Whether the loop's pass at `now` draws this view of `term` on
    /// `video`. `now_all`: vtium asks every display of the head to draw at
    /// once for something other than the program's output, such as a key,
    /// the pointer, a paste, a blink timer, glyphs that arrived for a font,
    /// an animated picture's next frame, a notice or the on-screen display;
    /// `frame_end`: a synchronized update ended, or its timeout ended the
    /// hold; `holding`: a hold keeps the last frame.
    ///
    /// vtium draws output that no key caused at most once a refresh, and
    /// anything a key caused as soon as it can (ADR-1401 D2). A draw that
    /// cannot wait goes ahead at once: the display lost its picture,
    /// `now_all` asked, the output came inside a key's window, or the
    /// display takes a frame that such a draw waited for. With three
    /// buffers it composes into the third while a flip is in flight. A key
    /// opens a window on each display, which closes at the first flip of
    /// the display that completes after the key was sent ([`View::flipped`]).
    /// Inside the window, every change of the screen draws at once: a short
    /// write, the end of a synchronized update and the draw a hold owed.
    /// Outside it they go ahead as a draw the coalescing paces during a
    /// flood does at its deadline: at once, unless the display's flip is
    /// in flight and the display keeps two or three buffers, and then the
    /// draw waits for the flip to end and composes nothing. A frame
    /// composed during the flip could not reach the screen before the flip
    /// ends, and the next change could draw over it before it got there. A
    /// paced draw that waited for the display to take a frame, as for a
    /// frame composing, waits for the flip in flight too. With two buffers
    /// the display lends no buffer while it flips, so a paced draw waited
    /// for the flip already; with one buffer, or on a framebuffer, no draw
    /// waits for a flip. Inside a hold, no draw asked at once makes a
    /// waiting draw go ahead at once, since the hold's end decides how its
    /// owed draw goes. A synchronized update that waits stays whole: a hold
    /// in force when the flip ends keeps the frame for its own end.
    pub fn draws(
        &mut self,
        term: &Term,
        video: &dyn Video,
        now_all: bool,
        frame_end: bool,
        holding: bool,
        now: Instant,
    ) -> bool {
        let due = self.wait.due().is_some_and(|d| d <= now);
        if due {
            self.wait.end();
        }
        // A pending view waits for its display to take a frame, so a
        // display that lends no token never turns the loop.
        let taken = self.pending && video.ready_for_frame(self.id);
        let after_key = self.after_key(term);
        // A synchronized update ended, or a hold that owed a draw is over.
        let ended = frame_end || (self.owed && !holding);
        // A hold's draw is owed and decided at the hold's end, so a draw
        // asked inside the hold leaves a waiting draw paced.
        let held = holding && !self.lost;
        if now_all || self.lost || (ended && after_key) || (taken && !self.wait.paced()) {
            if !held {
                self.wait.ask(false);
            }
            return true;
        }
        let mut paced = due || taken || ended || self.wait.for_flip();
        if term.changed {
            match term.coalesce.decide(
                holding,
                &mut self.wait,
                term.unparsed,
                after_key,
                now,
                self.refresh,
            ) {
                Draw::Now => {
                    if !held {
                        self.wait.ask(false);
                    }
                    return true;
                }
                Draw::Paced => paced = true,
                Draw::Later(_) => {}
            }
        }
        if !paced {
            return false;
        }
        if video.is_flipping(self.id) && video.paced_draw_waits_for_flip(self.id) {
            self.wait.wait_for_flip();
            return false;
        }
        self.wait.ask(true);
        true
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
            #[cfg_attr(not(feature = "images"), expect(unused_mut, reason = "no pictures"))]
            let mut taken = Snapshot::take(&term.terminal);
            // Each animated picture shows the frame the clock chose.
            #[cfg(feature = "images")]
            term.animation
                .show(&mut taken.placements, term.terminal.screen().images());
            (Arc::new(taken), term.terminal.screen_mut().checkpoint())
        });
        let since = std::mem::replace(&mut self.since, *upto);
        let whole = self.lost;
        if whole {
            self.drop_kept();
        }
        self.pending = false;
        self.wait.taken();
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
            self.drop_kept();
            let _ = video.swap(token, &damage);
            return Presented::default();
        }
        self.pointer.after_draw(video, self.id, tip, at);
        // The frame is drawn over any frame the display kept, and goes out
        // in its place.
        let cost = Cost {
            cpu: stats.cpu,
            damage_px: stats.damage_px,
            misses: stats.misses,
        }
        .with(self.kept.take());
        let queued = match video.swap(token, &damage) {
            Ok(Swapped::Frame | Swapped::Cursor) => true,
            Ok(Swapped::Nothing) => false,
            // The display keeps the frame and sends it when the flip ends:
            // nothing composes it again.
            Err(SwapError::Busy) => {
                self.kept = Some(cost);
                false
            }
            // The display draws the frame whole once it is ready again.
            Err(SwapError::Dropped) => {
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
        // A frame kept is counted when it goes out.
        if self.kept.is_none()
            && let Some(timer) = &mut self.timer
        {
            timer.frame(cost.cpu, queued, cost.damage_px, cost.misses);
        }
        let end = clock();
        let start = self.sent.take().unwrap_or(end);
        term.coalesce.drawn(&mut self.wait, start, end, queued);
        Presented {
            queued,
            compose: if queued { cost.cpu } else { Default::default() },
        }
    }

    /// Lets go of the frame the display kept from a busy swap, which no
    /// longer shows what the view shows: the view lost its picture, as at
    /// a resize, a mode change, a wake-up after the VT came back or a
    /// switch to another screen, or the VT was let go. The frame is counted
    /// as a pass that presented nothing, and the next frame is drawn whole
    /// over it, so the display sends none of its damage on its own.
    fn drop_kept(&mut self) {
        if let Some(cost) = self.kept.take()
            && let Some(timer) = &mut self.timer
        {
            timer.frame(cost.cpu, false, cost.damage_px, cost.misses);
        }
    }

    /// Sends the frame the display kept from a busy swap, once the flip it
    /// waited for has ended, so a draw that composes no newer frame shows
    /// it without composing it again. A frame that is still waiting stays
    /// kept, and so does one while a newer frame is composing, which is
    /// drawn over it and carries it. A commit that sends nothing leaves a
    /// draw pending, so the swap of that draw sends whatever the display
    /// still holds and no frame is lost. The cursor plane follows the
    /// commit as it follows a swap.
    pub fn send_kept(&mut self, video: &mut dyn Video) -> Presented {
        if self.lost {
            self.drop_kept();
        }
        let Some(cost) = self.kept else {
            return Presented::default();
        };
        if self.away() || video.is_flipping(self.id) {
            return Presented::default();
        }
        let queued = match video.commit(self.id) {
            Ok(Swapped::Frame | Swapped::Cursor) => true,
            // The display still waits, as for the vblank after a commit the
            // card refused as busy.
            Err(SwapError::Busy) => return Presented::default(),
            Ok(Swapped::Nothing) | Err(SwapError::Off | SwapError::Dropped) => false,
            Err(SwapError::Failed(e)) => {
                log::warn!("cannot swap display {}: {e}", video.name(self.id));
                false
            }
        };
        self.kept = None;
        self.pending |= !queued;
        self.pending |= self.pointer.after_swap(video, self.id);
        if let Some(timer) = &mut self.timer {
            timer.frame(cost.cpu, queued, cost.damage_px, cost.misses);
        }
        Presented {
            queued,
            compose: if queued { cost.cpu } else { Default::default() },
        }
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
            Prepared::Job(job) => {
                self.present(job.compose(None), term, video, true, clock)
                    .queued
            }
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

    // ADR-1004 D8 to D10: the frame a head's render thread draws from the
    // snapshot shows a mixed line in the order vtium-term draws it, the
    // cursor on the cell its logical cell is drawn in, and the selection
    // on the logical cells it holds, wherever they are drawn.
    #[cfg(feature = "bidi")]
    #[test]
    fn a_snapshot_draws_a_mixed_line_in_its_drawn_order() {
        // abc, shalom, (1), def; the cursor back on the shin, logical
        // column 4, which the row draws at column 11
        let mut terminal = Terminal::new(20, 1, 0);
        terminal.input("abc \u{5E9}\u{5DC}\u{5D5}\u{5DD} (1) def\x1b[5G".as_bytes());
        // a drag from the drawn 1 (logical 10) to the drawn shin (logical 4)
        terminal.screen_mut().select_start(5, 0);
        terminal.screen_mut().select_to(11, 0);
        let snapshot = Snapshot::take(&terminal);
        let frame = snapshot.frame(Age::NEVER);
        let row: String = frame
            .cells(0)
            .map(|c| match c.glyph {
                vtium_term::Glyph::Char(ch) => ch,
                _ => '.',
            })
            .collect();
        assert_eq!(row, "abc (1) \u{5DD}\u{5D5}\u{5DC}\u{5E9} def....");
        let cursor = frame.cursor();
        assert_eq!((cursor.col, cursor.rtl), (11, true));
        let selected: String = frame
            .cells(0)
            .map(|c| if c.selected { '#' } else { '.' })
            .collect();
        assert_eq!(selected, ".....#######........");
    }

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
    /// While a flip is in flight it lends a buffer as a DRM display does:
    /// none with two buffers, and with one or three the back one, until a
    /// frame drawn into it waits for the flip's end; that swap is refused
    /// as busy and the frame goes with the next swap.
    struct Flip {
        bufs: Vec<Arc<Mutex<Vec<u8>>>>,
        back: usize,
        drawn: Vec<Option<u32>>,
        count: u32,
        out: bool,
        /// A flip is in flight.
        flipping: bool,
        /// A frame drawn into the back buffer waits for a swap that takes.
        fresh: bool,
        /// The frames composed and handed back, each swap, whatever it
        /// drew.
        composed: u32,
        /// The frames sent to the screen, by a swap or by a commit.
        sent: u32,
        /// The next commit is refused as busy with no flip in flight, as a
        /// commit the card refuses with EBUSY waits for the next vblank.
        busy_once: bool,
        /// Commits send nothing, as a device without its own commit does.
        no_commit: bool,
        /// The display says no draw waits for its flip, as a framebuffer
        /// says.
        framebuffer: bool,
    }

    impl Flip {
        fn new() -> Flip {
            Flip::with(2)
        }

        /// Sends the frame in the back buffer, refused as busy while a
        /// flip is in flight.
        fn send(&mut self) -> Result<Swapped, SwapError> {
            if self.flipping {
                return Err(SwapError::Busy);
            }
            if !std::mem::take(&mut self.fresh) {
                return Ok(Swapped::Nothing);
            }
            self.back = (self.back + 1) % self.bufs.len();
            self.sent += 1;
            Ok(Swapped::Frame)
        }

        fn with(n: usize) -> Flip {
            let len = (SIZE.0 * SIZE.1 * 4) as usize;
            Flip {
                bufs: (0..n).map(|_| Arc::new(Mutex::new(vec![0; len]))).collect(),
                back: 0,
                drawn: vec![None; n],
                count: 0,
                out: false,
                flipping: false,
                fresh: false,
                composed: 0,
                sent: 0,
                busy_once: false,
                no_commit: false,
                framebuffer: false,
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
            let free = !self.flipping || (self.bufs.len() != 2 && !self.fresh);
            if !free || std::mem::replace(&mut self.out, true) {
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
            self.composed += 1;
            if !damage.is_empty() {
                self.count += 1;
                self.drawn[self.back] = Some(self.count);
                self.fresh = true;
            }
            self.send()
        }
        fn commit(&mut self, _: DisplayId) -> Result<Swapped, SwapError> {
            // While a token is out, its swap carries the frame kept.
            if self.out || self.no_commit {
                return Ok(Swapped::Nothing);
            }
            if std::mem::take(&mut self.busy_once) {
                return Err(SwapError::Busy);
            }
            self.send()
        }
        fn is_flipping(&self, _: DisplayId) -> bool {
            self.flipping
        }
        fn ready_for_frame(&self, id: DisplayId) -> bool {
            !self.out && (!self.is_flipping(id) || (self.bufs.len() != 2 && !self.fresh))
        }
        fn paced_draw_waits_for_flip(&self, _: DisplayId) -> bool {
            !self.framebuffer && self.bufs.len() > 1
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
        let shown = view.present(
            job.compose(None),
            &mut term,
            &mut video,
            true,
            &Instant::now,
        );
        assert_eq!(shown, Presented::default());
        assert_eq!(video.swaps, [], "not shown");
        assert!(!video.out, "the token went back");
        draw(&mut term, &mut view, &mut video, None);
        assert_eq!(video.swaps, [1], "the display lends again");
    }

    // vtium/vtium#216: a frame composing while its view is replaced by a new
    // one for the same display, as when a monitor swapped on its connector
    // is removed and added in one turn, is not shown, and does not give the
    // new view the old view's renderer: the new view draws with its own.
    #[test]
    fn a_frame_of_a_view_replaced_is_not_shown() {
        let (mut term, mut old) = view();
        let mut video = Plane::new(false);
        let key = (PathBuf::new(), ID);
        let mut snapshot = None;
        let Prepared::Job(job) = old.prepare(
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
        drop(old);
        let face: DisplayFace = Box::new(Unifont::new(32));
        let mut new = View::new(
            ID,
            Font::new(face),
            SIZE,
            None,
            RenderConfig::default(),
            false,
        );
        let cell = new.cell();
        let shown = new.present(
            job.compose(None),
            &mut term,
            &mut video,
            true,
            &Instant::now,
        );
        assert_eq!(shown, Presented::default());
        assert_eq!(video.swaps, [], "not shown");
        assert!(!video.out, "the token went back");
        assert!(!new.away(), "the new view keeps its drawing");
        let drawn = new.drawing.as_ref().map(|d| d.renderer.font().cell());
        assert_eq!(drawn, Some(cell), "with its own renderer");
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
        let shown = view.present(
            job.compose(None),
            &mut term,
            &mut video,
            false,
            &Instant::now,
        );
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
        let shown = view.present(
            job.compose(None),
            &mut term,
            &mut video,
            true,
            &Instant::now,
        );
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

    // ADR-1100 D16: when the console's session ends, its open uploads are
    // dropped silently, so the child's exit gives back an upload it left
    // open and the next session's first chunk-like command is its own.
    // covers: image.kitty
    #[cfg(feature = "images")]
    #[test]
    fn the_childs_exit_drops_a_kitty_upload_it_left_open() {
        use std::os::unix::process::ExitStatusExt as _;
        let (mut term, _view) = view();
        offer_pictures(&mut term);
        term.terminal
            .input(b"\x1b_Ga=T,f=24,s=2,v=1,i=1,m=1;/wAA\x1b\\");
        assert_eq!(term.terminal.screen().images().bytes(), 8 + 512);
        Consumer::exited(&mut term, ExitStatus::from_raw(0));
        assert_eq!(term.terminal.screen().images().bytes(), 0);
        term.terminal.input(b"\x1b_Gm=0;AP8A\x1b\\");
        assert!(term.terminal.screen().placements().is_empty());
        assert_eq!(term.terminal.screen().images().bytes(), 0);
        assert_eq!(term.terminal.take_replies(), b"");
    }

    // ADR-1102 D3: the snapshot carries the frame the animation clock
    // chose, so the display draws that frame's pixels: a picture of one
    // cell whose root frame is red and whose frame 2 is green shows red,
    // then green once the clock moves on, though the store's current frame
    // stays 1.
    // covers: image.kitty-animation
    #[cfg(feature = "images")]
    #[test]
    fn the_display_draws_the_frame_the_clock_chose() {
        let (mut term, mut view) = view();
        offer_pictures(&mut term);
        let fill = kitty_fill;
        let t = &mut term.terminal;
        let mut s = String::from("\x1b[?25l");
        for apc in [
            format!("a=T,f=32,s=8,v=16,i=1,q=2;{}", fill([255, 0, 0, 255])),
            format!("a=f,i=1,f=32,s=8,v=16,z=40,q=2;{}", fill([0, 255, 0, 255])),
            "a=a,i=1,r=1,z=30,q=2".to_owned(),
            "a=a,i=1,s=3,q=2".to_owned(),
        ] {
            s.push_str(&format!("\x1b_G{apc}\x1b\\"));
        }
        t.input(s.as_bytes());
        let mut video = Plane::new(false);
        let t0 = Instant::now();
        let color = |video: &Plane| video.pixel(5, 8) & 0xff_ff_ff;
        assert!(
            !term
                .animation
                .turn(term.terminal.screen().images(), t0, true)
        );
        draw(&mut term, &mut view, &mut video, None);
        assert_eq!(color(&video), 0xff_00_00, "the root frame");
        let later = t0 + Duration::from_millis(30);
        assert!(
            term.animation
                .turn(term.terminal.screen().images(), later, true)
        );
        draw(&mut term, &mut view, &mut video, None);
        assert_eq!(color(&video), 0x00_ff_00, "frame 2, the clock's");
        let key = term.terminal.screen().placements()[0].image;
        let store = term.terminal.screen().images();
        assert_eq!(store.animation(key).map(|a| a.current), Some(1));
    }

    /// One turn of the loop after the child's output in `parts`: each part
    /// is parsed as the pty reads it, then the display draws when
    /// [`View::draws`] asks it to, as `draw_head` in app.rs does, at the
    /// time `term`'s clock reads. The turn then ends as the loop's does.
    fn turn(term: &mut Term, view: &mut View, video: &mut dyn Video, parts: &[&[u8]]) {
        pass(term, view, video, parts, false);
    }

    /// [`turn`], with `now_all` as a key or the pointer asks every display
    /// to draw at once.
    fn pass(
        term: &mut Term,
        view: &mut View,
        video: &mut dyn Video,
        parts: &[&[u8]],
        now_all: bool,
    ) {
        for part in parts {
            assert_eq!(term.parse(part, &mut |_| {}), part.len());
        }
        let clock = Rc::clone(&term.clock);
        // As the loop draws a display: the end of a synchronized update
        // asks once, a frame the display kept goes out when no newer frame
        // composes, and a hold keeps the last frame.
        let frame_end = std::mem::take(&mut term.present);
        let holding = term.sync.holding(&term.terminal);
        let draws = view.draws(term, &*video, now_all, frame_end, holding, clock());
        if !draws || (holding && !view.lost) {
            view.send_kept(video);
        }
        if draws {
            view.draw(term, video, &DrawState::default(), &*clock);
        }
        term.coalesce.turn_end();
        term.changed = false;
    }

    /// A clock the test moves by hand, in milliseconds from its origin.
    struct Hand(Rc<std::cell::Cell<Instant>>, Instant);

    impl Hand {
        fn new() -> Hand {
            let t0 = Instant::now();
            Hand(Rc::new(std::cell::Cell::new(t0)), t0)
        }

        fn clock(&self) -> Clock {
            let at = Rc::clone(&self.0);
            Rc::new(move || at.get())
        }

        /// Sets the clock to `ms` milliseconds from its origin.
        fn at(&self, ms: u64) {
            self.0.set(self.1 + Duration::from_millis(ms));
        }
    }

    /// [`view`], with `term` reading `hand`'s clock and the display
    /// refreshing every 16 ms.
    fn view_at(hand: &Hand) -> (Term, View) {
        let (mut term, mut view) = view();
        term.clock = hand.clock();
        view.refresh = Some(Duration::from_millis(16));
        (term, view)
    }

    /// A flood's worth of output, [`COALESCE_BYTES`] of `c`, which fills
    /// the grid with it.
    fn flood(c: u8) -> Vec<u8> {
        vec![c; crate::sync::COALESCE_BYTES]
    }

    /// A flood that begins while frame A's flip is in flight, with `n`
    /// buffers: A goes out at 0 ms, and the flood writes a screenful every
    /// 4 ms from 4 ms, so its pacing deadline comes at 16 ms, a refresh
    /// after A's present began, and the turn then draws as the loop's
    /// timer asks it to.
    fn flood_during_a_flip(n: usize) -> (Hand, Term, View, Flip) {
        let hand = Hand::new();
        let (mut term, mut view) = view_at(&hand);
        let mut video = Flip::with(n);
        turn(&mut term, &mut view, &mut video, &[b"A"]);
        assert_eq!((video.composed, video.sent), (1, 1), "{n} buffers");
        video.flipping = true;
        for (ms, c) in [(4, b'b'), (8, b'c'), (12, b'd')] {
            hand.at(ms);
            turn(&mut term, &mut view, &mut video, &[&flood(c)]);
        }
        assert_eq!(
            view.wait.due(),
            Some(hand.1 + Duration::from_millis(16)),
            "{n} buffers: the deadline"
        );
        hand.at(16);
        turn(&mut term, &mut view, &mut video, &[]);
        (hand, term, view, video)
    }

    // ADR-1401 D2: a draw the coalescing paces during a flood waits for the
    // flip in flight to end, and composes then: none while the flip is
    // pending, at the deadline or at the flood's turns after it, and one
    // when the flip ends, which is sent at once. With two buffers the
    // display lends no buffer while it flips, so the counts are the same;
    // with three the third buffer is free, and the draw waits all the same.
    #[test]
    fn a_paced_draw_waits_for_the_flip_in_flight() {
        for n in [2, 3] {
            let (hand, mut term, mut view, mut video) = flood_during_a_flip(n);
            for (ms, c) in [(16, None), (18, Some(b'e')), (20, Some(b'f'))] {
                if let Some(c) = c {
                    hand.at(ms);
                    turn(&mut term, &mut view, &mut video, &[&flood(c)]);
                }
                assert_eq!(
                    (video.composed, video.sent),
                    (1, 1),
                    "{n} buffers at {ms} ms: nothing composes while the flip is pending"
                );
                assert!(
                    view.wait.for_flip(),
                    "{n} buffers at {ms} ms: the draw waits"
                );
                assert_eq!(
                    view.wait.due(),
                    None,
                    "{n} buffers at {ms} ms: not for a time"
                );
            }
            hand.at(24);
            video.flipping = false;
            turn(&mut term, &mut view, &mut video, &[]);
            assert_eq!(
                (video.composed, video.sent),
                (2, 2),
                "{n} buffers: one frame when the flip ends, sent once"
            );
            assert!(!view.wait.for_flip(), "{n} buffers");
            assert!(video.shows((video.back + n - 1) % n, 0), "{n} buffers: f");
            turn(&mut term, &mut view, &mut video, &[]);
            assert_eq!((video.composed, video.sent), (2, 2), "{n} buffers");
        }
    }

    // ADR-1401 D2: with three buffers a draw that cannot wait composes into
    // the third buffer while a flip is in flight, though a paced draw waits
    // for that flip: a key's echo, as a short write, and a key itself.
    // Either frame carries the flood too, and goes out when the flip ends.
    #[test]
    fn a_draw_at_once_composes_ahead_while_a_paced_draw_waits() {
        for now_all in [false, true] {
            let (hand, mut term, mut view, mut video) = flood_during_a_flip(3);
            assert_eq!((video.composed, video.sent), (1, 1));
            assert!(view.wait.for_flip());
            hand.at(18);
            let echo: &[&[u8]] = if now_all {
                &[]
            } else {
                term.key_sent();
                &[b"\x1b[1;1Hk"]
            };
            pass(&mut term, &mut view, &mut video, echo, now_all);
            assert_eq!(
                (video.composed, video.sent),
                (2, 1),
                "composed at once (key {now_all}), kept for the flip"
            );
            assert!(!view.wait.for_flip(), "the frame carries the flood");
            hand.at(24);
            video.flipping = false;
            turn(&mut term, &mut view, &mut video, &[]);
            assert_eq!((video.composed, video.sent), (2, 2), "sent as it is");
        }
    }

    // ADR-1401 D2: a paced draw that meets a frame composing on the render
    // thread waits for that frame to come back, and when the frame's flip
    // is then in flight, it waits for the flip too: with three buffers the
    // display lends the third during the flip, and a frame composed into it
    // would be drawn over at the flip's end.
    #[test]
    fn a_paced_draw_that_met_a_frame_composing_waits_for_its_flip() {
        let hand = Hand::new();
        let (mut term, mut view) = view_at(&hand);
        let mut video = Flip::with(3);
        let key = (PathBuf::new(), ID);
        let clock = hand.clock();
        // A at 0, its flip until 16.
        turn(&mut term, &mut view, &mut video, &[b"A"]);
        video.flipping = true;
        hand.at(4);
        turn(&mut term, &mut view, &mut video, &[&flood(b'b')]);
        hand.at(16);
        video.flipping = false;
        // The pass at the flip's end hands B to the render thread.
        assert!(view.draws(&term, &video, false, false, false, clock()));
        let Prepared::Job(job) = view.prepare(
            &key,
            &mut term,
            &mut None,
            &mut video,
            &DrawState::default(),
            None,
            &*clock,
        ) else {
            panic!("B is taken")
        };
        term.coalesce.turn_end();
        // A flood turn while B composes: its paced draw waits for B.
        hand.at(18);
        assert_eq!(term.parse(&flood(b'c'), &mut |_| {}), flood(b'c').len());
        if view.draws(&term, &video, false, false, false, clock()) {
            let later = view.prepare(
                &key,
                &mut term,
                &mut None,
                &mut video,
                &DrawState::default(),
                None,
                &*clock,
            );
            assert!(matches!(later, Prepared::Later), "B is composing");
        }
        term.coalesce.turn_end();
        term.changed = false;
        // B comes back at 20, and its flip is in flight until 32.
        hand.at(20);
        view.present(job.compose(None), &mut term, &mut video, true, &*clock);
        assert_eq!((video.composed, video.sent), (2, 2));
        video.flipping = true;
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!(
            (video.composed, video.sent),
            (2, 2),
            "nothing composes during B's flip"
        );
        hand.at(32);
        video.flipping = false;
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (3, 3), "C at B's flip's end");
        assert!(video.shows((video.back + 2) % 3, 0), "c");
    }

    // ADR-1401 D2: with one buffer no draw waits for a flip, a paced one
    // included. vtium draws into the buffer shown, so the frame composed
    // at the deadline goes out with the next commit, and the display keeps
    // it while the flip lasts.
    #[test]
    fn with_one_buffer_a_paced_draw_composes_at_its_deadline() {
        let (hand, mut term, mut view, mut video) = flood_during_a_flip(1);
        assert_eq!((video.composed, video.sent), (2, 1), "composed at 16 ms");
        assert!(!view.wait.for_flip());
        hand.at(24);
        video.flipping = false;
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (2, 2), "sent as it is");
    }

    /// Frame A, `A`, goes out at 0 ms with `n` buffers, and its flip is
    /// then in flight.
    fn a_in_flight(n: usize) -> (Hand, Term, View, Flip) {
        let hand = Hand::new();
        let (mut term, mut view) = view_at(&hand);
        let mut video = Flip::with(n);
        turn(&mut term, &mut view, &mut video, &[b"A"]);
        assert_eq!((video.composed, video.sent), (1, 1), "{n} buffers: A");
        video.flipping = true;
        (hand, term, view, video)
    }

    // ADR-1401 D2: with two or three buffers, a short write inside the
    // window of a key sent to the program, such as the key's echo, composes
    // ahead at once: with three into the free third buffer while the flip
    // is in flight, and with two as soon as the display lends a buffer, at
    // the flip's end. It does not wait for the flip as a paced draw does.
    #[test]
    fn a_short_write_after_a_key_composes_ahead_during_a_flip() {
        for n in [2, 3] {
            let (hand, mut term, mut view, mut video) = a_in_flight(n);
            term.key_sent();
            hand.at(4);
            turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2Hk"]);
            assert!(
                !view.wait.for_flip(),
                "{n} buffers: the echo waits for no flip"
            );
            let composed = if n == 3 { 2 } else { 1 };
            assert_eq!(
                (video.composed, video.sent),
                (composed, 1),
                "{n} buffers: the echo composed at once where a buffer is free"
            );
            hand.at(16);
            video.flipping = false;
            turn(&mut term, &mut view, &mut video, &[]);
            assert_eq!((video.composed, video.sent), (2, 2), "{n} buffers");
            assert!(video.shows((video.back + n - 1) % n, 1), "{n} buffers: k");
        }
    }

    // ADR-1401 D2: a short write with no key's window open, as when no key
    // went to the program since the display's last flip completed, waits
    // for the flip in flight, as a paced draw does, and composes once when
    // the flip ends.
    #[test]
    fn a_short_write_no_key_caused_waits_for_the_flip() {
        for n in [2, 3] {
            let (hand, mut term, mut view, mut video) = a_in_flight(n);
            hand.at(4);
            turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2Hx"]);
            hand.at(8);
            turn(&mut term, &mut view, &mut video, &[]);
            assert_eq!(
                (video.composed, video.sent),
                (1, 1),
                "{n} buffers: nothing composes while the flip is pending"
            );
            assert!(view.wait.for_flip(), "{n} buffers: the write waits");
            assert_eq!(view.wait.due(), None, "{n} buffers: not for a time");
            hand.at(16);
            video.flipping = false;
            turn(&mut term, &mut view, &mut video, &[]);
            assert_eq!(
                (video.composed, video.sent),
                (2, 2),
                "{n} buffers: one frame when the flip ends"
            );
            assert!(video.shows((video.back + n - 1) % n, 1), "{n} buffers: x");
        }
    }

    /// The display's flip in flight completes, and the loop hands the
    /// display's flip event to the view.
    fn flip_ends(term: &Term, view: &mut View, video: &mut Flip) {
        video.flipping = false;
        view.flipped(term);
    }

    // ADR-1401 D2: a key opens a window on each display, and every change
    // the program's output makes to the screen draws at once until the
    // display's next flip completes. A program that redraws its status
    // line just after a key, before it echoes the key, does not make the
    // echo wait for the status line's flip.
    #[test]
    fn every_change_in_a_keys_window_draws_at_once() {
        let hand = Hand::new();
        let (mut term, mut view) = view_at(&hand);
        let mut video = Flip::with(3);
        turn(&mut term, &mut view, &mut video, &[b"A"]);
        // A key, whose own frame sent nothing, as a frame with no damage
        // sends nothing (`the_frame_a_key_draws_itself_leaves_its_window_open`
        // shows that frame leaves the window open).
        hand.at(2);
        term.key_sent();
        // The program redraws its status line, and that frame's flip is in
        // flight.
        hand.at(3);
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;5Hs"]);
        assert_eq!(
            (video.composed, video.sent),
            (2, 2),
            "the status line at once"
        );
        video.flipping = true;
        // The echo comes during that flip, inside the key's window.
        hand.at(4);
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2Hk"]);
        assert!(!view.wait.for_flip(), "the echo waits for no flip");
        assert_eq!(
            (video.composed, video.sent),
            (3, 2),
            "the echo composed at once into the third buffer"
        );
        hand.at(16);
        flip_ends(&term, &mut view, &mut video);
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (3, 3), "the echo went out");
        let b = (video.back + 2) % 3;
        assert!(
            video.shows(b, 1) && video.shows(b, 4),
            "the echo and the status line"
        );
    }

    // ADR-1401 D2: a key's window closes when the first flip of the
    // display that completes after the key was sent completes. A second
    // key sent while the window is open keeps it open no longer: both
    // close at that flip. Output after it, with no new key, waits for the
    // flip in flight again.
    #[test]
    fn a_keys_window_closes_when_the_next_flip_completes() {
        let (hand, mut term, mut view, mut video) = a_in_flight(3);
        term.key_sent();
        hand.at(4);
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2Hk"]);
        assert_eq!(
            (video.composed, video.sent),
            (2, 1),
            "the echo composed at once"
        );
        // A second key, inside the window: its echo waits only for a free
        // buffer.
        hand.at(6);
        term.key_sent();
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;3Hl"]);
        assert_eq!((video.composed, video.sent), (2, 1), "no buffer is free");
        assert!(
            view.pending && !view.wait.for_flip(),
            "l waits for a buffer"
        );
        // A's flip completes and closes the window; l composes at once over
        // the kept frame, and its flip is in flight.
        hand.at(16);
        flip_ends(&term, &mut view, &mut video);
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (3, 2), "k and l went out");
        video.flipping = true;
        hand.at(20);
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;4Hm"]);
        assert_eq!(
            (video.composed, video.sent),
            (3, 2),
            "the write after the window waits"
        );
        assert!(view.wait.for_flip(), "m waits for the flip");
        hand.at(32);
        flip_ends(&term, &mut view, &mut video);
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (4, 3), "m at the flip's end");
    }

    // ADR-1401 D2: the frame a key draws for itself, before the program has
    // answered, leaves the key's window open, so the echo that follows
    // composes ahead at once.
    #[test]
    fn the_frame_a_key_draws_itself_leaves_its_window_open() {
        let hand = Hand::new();
        let (mut term, mut view) = view_at(&hand);
        let mut video = Flip::with(3);
        turn(&mut term, &mut view, &mut video, &[b"A"]);
        // The key's own pass composes a frame at once.
        hand.at(2);
        term.key_sent();
        pass(&mut term, &mut view, &mut video, &[], true);
        let after_key = video.composed;
        assert_eq!(after_key, 2, "the key's own frame");
        video.flipping = true;
        hand.at(4);
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2Hk"]);
        assert!(!view.wait.for_flip(), "the echo waits for no flip");
        assert_eq!(video.composed, after_key + 1, "the echo composed at once");
    }

    // ADR-1401 D2: with one buffer, or on a display that says so, as a
    // framebuffer does, no short write waits for a flip, whether a key
    // caused it or not.
    #[test]
    fn with_one_buffer_or_a_framebuffer_no_short_write_waits() {
        for framebuffer in [false, true] {
            let n = if framebuffer { 3 } else { 1 };
            let hand = Hand::new();
            let (mut term, mut view) = view_at(&hand);
            let mut video = Flip::with(n);
            video.framebuffer = framebuffer;
            turn(&mut term, &mut view, &mut video, &[b"A"]);
            video.flipping = true;
            hand.at(4);
            turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2Hx"]);
            assert!(!view.wait.for_flip(), "framebuffer {framebuffer}");
            assert_eq!(
                video.composed, 2,
                "framebuffer {framebuffer}: composed at once"
            );
        }
    }

    /// Offers mode 2026 to `term`'s program, with a hold's timeout of
    /// 200 ms, as a head does.
    fn offer_sync(term: &mut Term) {
        term.terminal.set_sync_enabled(true);
        term.sync = Sync::new(Duration::from_millis(200), term.terminal.sync_frames());
    }

    /// [`a_in_flight`], with mode 2026 offered.
    fn synced_in_flight(n: usize) -> (Hand, Term, View, Flip) {
        let hand = Hand::new();
        let (mut term, mut view) = view_at(&hand);
        offer_sync(&mut term);
        let mut video = Flip::with(n);
        turn(&mut term, &mut view, &mut video, &[b"A"]);
        assert_eq!((video.composed, video.sent), (1, 1), "{n} buffers: A");
        video.flipping = true;
        (hand, term, view, video)
    }

    /// A synchronized update of `body`: mode 2026 set, `body`, mode 2026
    /// reset.
    fn update(body: &[u8]) -> Vec<u8> {
        [b"\x1b[?2026h", body, b"\x1b[?2026l"].concat()
    }

    /// A synchronized update of a flood's worth of `c`, so that its end is
    /// not also a short write.
    fn big_update(c: u8) -> Vec<u8> {
        update(&flood(c))
    }

    // ADR-1401 D2: with two or three buffers, the end of a synchronized
    // update that follows a key sent to the program composes at once: with
    // three into the free third buffer while the flip is in flight, and
    // with two as soon as the display lends a buffer, at the flip's end.
    #[test]
    fn a_synchronized_update_after_a_key_composes_at_once_during_a_flip() {
        for n in [2, 3] {
            let (hand, mut term, mut view, mut video) = synced_in_flight(n);
            term.key_sent();
            hand.at(4);
            turn(&mut term, &mut view, &mut video, &[&big_update(b'k')]);
            assert!(!view.wait.for_flip(), "{n} buffers: it waits for no flip");
            let composed = if n == 3 { 2 } else { 1 };
            assert_eq!(
                (video.composed, video.sent),
                (composed, 1),
                "{n} buffers: composed at once where a buffer is free"
            );
            hand.at(16);
            video.flipping = false;
            turn(&mut term, &mut view, &mut video, &[]);
            assert_eq!((video.composed, video.sent), (2, 2), "{n} buffers");
            assert!(video.shows((video.back + n - 1) % n, 0), "{n} buffers: k");
        }
    }

    // ADR-1401 D2: the end of a synchronized update that no key caused
    // waits for the flip in flight, as a paced draw does, and composes once
    // when the flip ends.
    #[test]
    fn a_synchronized_update_no_key_caused_waits_for_the_flip() {
        for n in [2, 3] {
            let (hand, mut term, mut view, mut video) = synced_in_flight(n);
            hand.at(4);
            turn(&mut term, &mut view, &mut video, &[&big_update(b'x')]);
            hand.at(8);
            turn(&mut term, &mut view, &mut video, &[]);
            assert_eq!(
                (video.composed, video.sent),
                (1, 1),
                "{n} buffers: nothing composes while the flip is pending"
            );
            assert!(view.wait.for_flip(), "{n} buffers: the update waits");
            hand.at(16);
            video.flipping = false;
            turn(&mut term, &mut view, &mut video, &[]);
            assert_eq!(
                (video.composed, video.sent),
                (2, 2),
                "{n} buffers: one frame when the flip ends"
            );
            assert!(video.shows((video.back + n - 1) % n, 0), "{n} buffers: x");
        }
    }

    // ADR-1401 D2: the draw a hold owed follows the same rule when the hold
    // ends without a frame's end, as when mode 2026 is turned off: after a
    // key it composes at once during the flip, and otherwise it waits for
    // the flip to end.
    #[test]
    fn an_owed_draw_composes_at_once_only_after_a_key() {
        for key in [true, false] {
            let (hand, mut term, mut view, mut video) = synced_in_flight(3);
            if key {
                term.key_sent();
            }
            hand.at(4);
            turn(
                &mut term,
                &mut view,
                &mut video,
                &[b"\x1b[?2026h\x1b[1;2Hk"],
            );
            assert!(view.owed, "key {key}: the hold owes a draw");
            assert_eq!(video.composed, 1, "key {key}: nothing composed in the hold");
            term.terminal.set_sync_enabled(false);
            hand.at(8);
            turn(&mut term, &mut view, &mut video, &[]);
            if key {
                assert!(!view.wait.for_flip(), "the owed draw waits for no flip");
                assert_eq!((video.composed, video.sent), (2, 1), "composed at once");
            } else {
                assert!(view.wait.for_flip(), "the owed draw waits for the flip");
                assert_eq!((video.composed, video.sent), (1, 1), "nothing composed");
            }
            hand.at(16);
            video.flipping = false;
            turn(&mut term, &mut view, &mut video, &[]);
            assert_eq!((video.composed, video.sent), (2, 2), "key {key}");
            assert!(video.shows((video.back + 2) % 3, 1), "key {key}: k");
        }
    }

    // ADR-1401 D2: output inside a hold asks for no draw at once, since
    // the hold's end decides, and neither does a draw that every display
    // is asked for at once inside the hold, as when a blink timer fires. A
    // draw no key caused that met a frame composing keeps waiting as a
    // paced draw through a hold that begins meanwhile, so when the hold
    // ends during that frame's flip, it waits for the flip, and composes
    // the update with it when the flip ends.
    #[test]
    fn a_hold_does_not_make_a_waiting_draw_compose_at_once() {
        for blink in [false, true] {
            let hand = Hand::new();
            let (mut term, mut view) = view_at(&hand);
            offer_sync(&mut term);
            let mut video = Flip::with(3);
            let key = (PathBuf::new(), ID);
            let clock = hand.clock();
            turn(&mut term, &mut view, &mut video, &[b"A"]);
            // B goes to the render thread.
            hand.at(2);
            assert_eq!(term.parse(b"B", &mut |_| {}), 1);
            assert!(view.draws(&term, &video, false, false, false, clock()));
            let Prepared::Job(job) = view.prepare(
                &key,
                &mut term,
                &mut None,
                &mut video,
                &DrawState::default(),
                None,
                &*clock,
            ) else {
                panic!("B is taken")
            };
            term.coalesce.turn_end();
            term.changed = false;
            // C, no key's answer, meets B composing.
            hand.at(4);
            turn(&mut term, &mut view, &mut video, &[b"\x1b[1;3HC"]);
            assert!(view.pending, "blink {blink}: C waits for B");
            // An update begins, and its content changes the screen.
            hand.at(6);
            turn(
                &mut term,
                &mut view,
                &mut video,
                &[b"\x1b[?2026h\x1b[1;4HD"],
            );
            if blink {
                // The cursor's blink timer fires inside the hold.
                hand.at(7);
                pass(&mut term, &mut view, &mut video, &[], true);
            }
            // B comes back, and its flip is in flight.
            hand.at(8);
            view.present(job.compose(None), &mut term, &mut video, true, &*clock);
            assert_eq!((video.composed, video.sent), (2, 2), "blink {blink}");
            video.flipping = true;
            hand.at(10);
            turn(&mut term, &mut view, &mut video, &[b"\x1b[?2026l"]);
            assert_eq!(
                (video.composed, video.sent),
                (2, 2),
                "blink {blink}: the update's end waits for B's flip"
            );
            assert!(view.wait.for_flip(), "blink {blink}");
            hand.at(16);
            flip_ends(&term, &mut view, &mut video);
            turn(&mut term, &mut view, &mut video, &[]);
            assert_eq!(
                (video.composed, video.sent),
                (3, 3),
                "blink {blink}: C and D at the flip's end"
            );
            let b = (video.back + 2) % 3;
            assert!(
                video.shows(b, 2) && video.shows(b, 3),
                "blink {blink}: C and D"
            );
        }
    }

    // ADR-1401 D2: with one buffer, or on a display that says no draw waits
    // for its flip, as a framebuffer does, the end of a synchronized update
    // no key caused composes at once.
    #[test]
    fn with_one_buffer_or_a_framebuffer_no_synchronized_update_waits() {
        for framebuffer in [false, true] {
            let n = if framebuffer { 3 } else { 1 };
            let hand = Hand::new();
            let (mut term, mut view) = view_at(&hand);
            offer_sync(&mut term);
            let mut video = Flip::with(n);
            video.framebuffer = framebuffer;
            turn(&mut term, &mut view, &mut video, &[b"A"]);
            video.flipping = true;
            hand.at(4);
            turn(&mut term, &mut view, &mut video, &[&big_update(b'x')]);
            assert!(!view.wait.for_flip(), "framebuffer {framebuffer}");
            assert_eq!(
                video.composed, 2,
                "framebuffer {framebuffer}: composed at once"
            );
        }
    }

    // ADR-1401 D2: an update that waits for the flip is never drawn in part.
    // When the program has begun another update by the time the flip ends,
    // the waiting one goes out with the next one's end, in one frame.
    #[test]
    fn a_synchronized_update_that_waits_is_never_drawn_in_part() {
        let (hand, mut term, mut view, mut video) = synced_in_flight(3);
        hand.at(4);
        turn(&mut term, &mut view, &mut video, &[&update(b"\x1b[1;1Hp")]);
        assert!(view.wait.for_flip(), "p waits for the flip");
        hand.at(8);
        turn(
            &mut term,
            &mut view,
            &mut video,
            &[b"\x1b[?2026h\x1b[1;2Hq"],
        );
        hand.at(16);
        video.flipping = false;
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!(
            (video.composed, video.sent),
            (1, 1),
            "nothing composes while the next update is open"
        );
        hand.at(20);
        turn(
            &mut term,
            &mut view,
            &mut video,
            &[b"\x1b[1;3Hr\x1b[?2026l"],
        );
        assert_eq!((video.composed, video.sent), (2, 2), "one frame at its end");
        let b = (video.back + 2) % 3;
        assert!(
            (0..3).all(|col| video.shows(b, col)),
            "p, q and r in the frame sent"
        );
    }

    /// The vertical blanks from a change to the vblank whose flip puts it
    /// on screen, with three buffers and a refresh of 16 ms. Frame A goes
    /// out at 10 ms, and its flip is in flight until the vblank at 32 ms,
    /// as a flip that just missed the vblank at 16 ms is; the flip of each
    /// frame sent later ends at the next vblank. A flood of spaces runs
    /// every 2 ms from 12 ms, so its pacing deadline, a refresh after A's
    /// present began, comes at 26 ms, while A's flip is in flight. At
    /// `change` ms, `parts` writes `X` in the grid's second cell.
    fn flips_to_screen(change: u64, parts: &[&[u8]]) -> u32 {
        let hand = Hand::new();
        let (mut term, mut view) = view_at(&hand);
        let mut video = Flip::with(3);
        hand.at(10);
        turn(&mut term, &mut view, &mut video, &[b"A"]);
        video.flipping = true;
        let mut vblank = 32;
        let mut flips = 0;
        for ms in 11..200 {
            hand.at(ms);
            let sent = video.sent;
            if ms == vblank {
                vblank += 16;
                if video.flipping {
                    video.flipping = false;
                    if ms > change {
                        flips += 1;
                        if video.shows((video.back + 2) % 3, 1) {
                            return flips;
                        }
                    }
                }
                // The loop's turn at the flip's event.
                turn(&mut term, &mut view, &mut video, &[]);
            } else if ms == change {
                turn(&mut term, &mut view, &mut video, parts);
            } else if ms < change && ms % 2 == 0 {
                turn(&mut term, &mut view, &mut video, &[&flood(b' ')]);
            } else if view.wait.due().is_some_and(|d| d <= hand.0.get()) {
                // The turn at the pacing deadline.
                turn(&mut term, &mut view, &mut video, &[]);
            }
            if video.sent > sent {
                video.flipping = true;
            }
        }
        panic!("the change never reached the screen")
    }

    // ADR-1401 D2: waiting for the flip costs no latency. A change made
    // at 30 ms, while A's flip is in flight, reaches the screen at the
    // second vblank after it: the first ends A's flip, and the second ends
    // the flip of the frame that carries the change. That holds for a
    // paced draw, the flood's, and for a draw at once, a key's echo.
    #[test]
    fn a_paced_draw_reaches_the_screen_as_soon_as_one_at_once() {
        let mut flood = flood(b' ');
        flood.extend(b"\x1b[1;2HX");
        assert_eq!(flips_to_screen(30, &[&flood]), 2, "paced");
        assert_eq!(flips_to_screen(30, &[b"\x1b[1;2HX"]), 2, "at once");
    }

    /// A sixel picture of 32 by 12 pixels, its two bands each drawn over
    /// 320 times, about 21 KB, cut in parts of 4 KB as tier 3's flood
    /// screen sends its pictures. Only its last part, which ends it, can
    /// change the screen.
    fn sixel_parts() -> Vec<Vec<u8>> {
        let mut pic = b"\x1bP0;1q\"1;1;32;12#1;2;0;60;100#1".to_vec();
        for band in 0..2 {
            if band > 0 {
                pic.push(b'-');
            }
            for _ in 0..320 {
                pic.extend(b"~^".repeat(16));
                pic.push(b'$');
            }
        }
        pic.extend(b"\x1b\\");
        pic.chunks(4096).map(<[u8]>::to_vec).collect()
    }

    /// `term` offering sixel and kitty pictures on cells of 8 by 16
    /// pixels, through the handlers a head installs.
    #[cfg(feature = "images")]
    fn offer_pictures(term: &mut Term) {
        use vtium_image::{Kitty, Sixel, Switches};
        use vtium_term::ImageString;
        let t = &mut term.terminal;
        t.screen_mut().set_image_limit(1 << 24);
        t.screen_mut().set_image_cell(8, 16).unwrap();
        t.set_image_switches(Switches {
            sixel: true,
            kitty: true,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Sixel, Some(Box::new(Sixel::default())));
        t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
    }

    /// 8 by 16 pixels of `rgba`, in base64, a kitty picture of one cell.
    #[cfg(feature = "images")]
    fn kitty_fill(rgba: [u8; 4]) -> String {
        let bytes = rgba.repeat(128);
        let abc = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in bytes.chunks(3) {
            let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                out.push(if i <= c.len() {
                    char::from(abc[((n >> (18 - 6 * i)) & 63) as usize])
                } else {
                    '='
                });
            }
        }
        out
    }

    /// Output that changes nothing on the screen: a pen change, a title,
    /// and the cursor moved to where it already is.
    const UNSEEN: &[u8] = b"\x1b[31m\x1b[m\x1b]2;a title\x07\x1b[2;1H";

    /// The frames `view`'s render timer counted: those queued, and those
    /// that queued nothing.
    fn counted(view: &mut View) -> (u32, u32) {
        let timer = view.timer.as_mut().unwrap();
        let r = timer.report(Duration::from_secs(1), 1).unwrap();
        (r.frames, r.skipped)
    }

    // The child's output that changes nothing on the screen, as a sixel
    // picture's parts before its end, composes no frame (vtium/vtium#242):
    // a display draws only a screen that changed since its last frame.
    #[test]
    fn output_that_changes_nothing_on_screen_composes_no_frame() {
        let (mut term, mut view) = view();
        view.timer = Some(RenderTimer::new());
        let mut video = Flip::with(1);
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;1Hp1\x1b[2;1H"]);
        assert_eq!(video.composed, 1, "the caption is drawn");
        let parts = sixel_parts();
        let (body, _end) = parts.split_at(parts.len() - 1);
        assert!(body.len() >= 4, "a picture of several parts");
        for part in body {
            turn(&mut term, &mut view, &mut video, &[part]);
        }
        turn(&mut term, &mut view, &mut video, &[UNSEEN]);
        assert_eq!(video.composed, 1, "no frame for the parts before the end");
        assert_eq!(counted(&mut view), (1, 0));
    }

    // Output that changes nothing while a flip is in flight leaves no draw
    // waiting for the flip's end, so no frame is composed then; output that
    // changes the screen meanwhile is drawn once the flip ends. With two
    // buffers the display lends none while it flips.
    #[test]
    fn output_during_a_flip_composes_only_what_changed() {
        let (mut term, mut view) = view();
        view.timer = Some(RenderTimer::new());
        let mut video = Flip::new();
        turn(&mut term, &mut view, &mut video, &[b"A\x1b[2;1H"]);
        assert_eq!(video.composed, 1);
        video.flipping = true;
        for _ in 0..3 {
            turn(&mut term, &mut view, &mut video, &[UNSEEN]);
        }
        video.flipping = false;
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!(video.composed, 1, "nothing changed during the flip");
        video.flipping = true;
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2HB"]);
        assert!(view.wait.for_flip(), "the draw waits for the flip");
        video.flipping = false;
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!(video.composed, 2, "B is drawn once the flip ends");
        assert!(video.shows(1, 1), "B");
        assert_eq!(counted(&mut view), (2, 0));
    }

    // With three buffers the display lends the third while the second
    // flips, as tier 3's flood screen ran on: output that changes nothing
    // composes no frame into it, where each such frame drew what the
    // buffer lacked, was refused as busy and counted as skipped.
    #[test]
    fn output_during_a_flip_draws_nothing_ahead_with_three_buffers() {
        let (mut term, mut view) = view();
        view.timer = Some(RenderTimer::new());
        let mut video = Flip::with(3);
        turn(&mut term, &mut view, &mut video, &[b"A\x1b[2;1H"]);
        video.flipping = true;
        assert!(video.ready_for_frame(ID), "the third buffer is free");
        for _ in 0..3 {
            turn(&mut term, &mut view, &mut video, &[UNSEEN]);
        }
        video.flipping = false;
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!(video.composed, 1, "nothing changed during the flip");
        assert_eq!(counted(&mut view), (1, 0));
    }

    // vtium/vtium#246: with three buffers a frame drawn while a flip is in
    // flight is refused as busy, and the display keeps it. When the flip
    // ends that frame goes out as it is, composed once and sent once, and
    // counted as a frame, not as a pass that drew nothing.
    #[test]
    fn a_frame_refused_as_busy_goes_out_after_the_flip_without_composing_again() {
        let (mut term, mut view) = view();
        view.timer = Some(RenderTimer::new());
        let mut video = Flip::with(3);
        turn(&mut term, &mut view, &mut video, &[b"A\x1b[2;1H"]);
        assert_eq!((video.composed, video.sent), (1, 1));
        video.flipping = true;
        // B comes inside a key's window, so it composes ahead during the
        // flip.
        term.key_sent();
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2HB\x1b[2;1H"]);
        assert_eq!((video.composed, video.sent), (2, 1), "B waits for the flip");
        for _ in 0..3 {
            turn(&mut term, &mut view, &mut video, &[UNSEEN]);
        }
        assert_eq!(
            (video.composed, video.sent),
            (2, 1),
            "nothing new to compose"
        );
        video.flipping = false;
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (2, 2), "B goes out as it is");
        assert!(video.shows(1, 1), "B in the buffer sent");
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (2, 2), "and only once");
        assert_eq!(counted(&mut view), (2, 0));
    }

    // vtium/vtium#246: a screen that changes while a kept frame waits for
    // the flip composes one newer frame once the flip ends, drawn over the
    // kept one in the same buffer, and that frame goes out in its place:
    // three changes during the wait compose once more, not once each, and
    // the kept frame is not sent on its own as well.
    #[test]
    fn changes_while_a_kept_frame_waits_compose_one_frame_that_carries_it() {
        let (mut term, mut view) = view();
        view.timer = Some(RenderTimer::new());
        let mut video = Flip::with(3);
        turn(&mut term, &mut view, &mut video, &[b"A\x1b[2;1H"]);
        video.flipping = true;
        // B, C, D and E come inside a key's window, so each draws at once,
        // and B composes ahead during the flip.
        term.key_sent();
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2HB"]);
        assert_eq!((video.composed, video.sent), (2, 1));
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;3HC"]);
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;4HD"]);
        turn(&mut term, &mut view, &mut video, &[b"\x1b[2;1HE"]);
        assert_eq!((video.composed, video.sent), (2, 1), "the kept frame waits");
        assert!(view.pending, "a newer frame waits for the flip");
        video.flipping = false;
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (3, 2), "one frame, sent once");
        assert!((1..4).all(|col| video.shows(1, col)), "B, C and D");
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (3, 2));
        // B's cost is counted with the frame drawn over it.
        assert_eq!(counted(&mut view), (2, 0));
    }

    // vtium/vtium#246: a frame kept from a busy swap shows the screen the
    // view showed. When the view loses its picture before the flip ends, as
    // at a switch to another screen, the kept frame is not sent: the next
    // frame is drawn whole over it, and the kept frame counts as a pass that
    // presented nothing.
    #[test]
    fn a_view_that_loses_its_picture_drops_the_kept_frame() {
        let (mut term, mut view) = view();
        view.timer = Some(RenderTimer::new());
        let mut video = Flip::with(3);
        turn(&mut term, &mut view, &mut video, &[b"A\x1b[2;1H"]);
        video.flipping = true;
        // B comes inside a key's window, so it composes ahead during the
        // flip.
        term.key_sent();
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2HB\x1b[2;1H"]);
        assert_eq!((video.composed, video.sent), (2, 1));
        view.show_another();
        assert!(view.send_kept(&mut video) == Presented::default());
        video.flipping = false;
        assert!(
            view.send_kept(&mut video) == Presented::default(),
            "not sent"
        );
        assert_eq!(video.sent, 1);
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!(
            (video.composed, video.sent),
            (3, 2),
            "drawn whole, sent once"
        );
        assert_eq!(counted(&mut view), (2, 1));
        // The loop marks the view lost itself after a mode change, a
        // wake-up or the VT's return: the whole frame drops the kept one.
        video.flipping = true;
        term.key_sent();
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;3HC\x1b[2;1H"]);
        assert_eq!((video.composed, video.sent), (4, 2));
        view.lost = true;
        video.flipping = false;
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (5, 3));
        assert_eq!(counted(&mut view), (1, 1));
    }

    // A display whose commit sends nothing while it still holds the kept
    // frame does not lose that frame: the view draws again, and the swap
    // of that draw sends what the display holds.
    #[test]
    fn a_kept_frame_the_commit_does_not_send_is_drawn_again() {
        let (mut term, mut view) = view();
        let mut video = Flip::with(3);
        video.no_commit = true;
        turn(&mut term, &mut view, &mut video, &[b"A\x1b[2;1H"]);
        video.flipping = true;
        // B comes inside a key's window, so it composes ahead during the
        // flip.
        term.key_sent();
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2HB\x1b[2;1H"]);
        video.flipping = false;
        turn(&mut term, &mut view, &mut video, &[]);
        assert!(view.pending, "a draw waits");
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!(video.sent, 2, "B reaches the screen");
        assert!(video.shows(1, 1), "B");
    }

    // A commit refused as busy after the flip ended, as one that waits for
    // the next vblank, keeps the frame: it goes out at the next try, and
    // nothing composes it again.
    #[test]
    fn a_kept_frame_whose_commit_is_refused_as_busy_goes_out_at_the_next_try() {
        let (mut term, mut view) = view();
        view.timer = Some(RenderTimer::new());
        let mut video = Flip::with(3);
        turn(&mut term, &mut view, &mut video, &[b"A\x1b[2;1H"]);
        video.flipping = true;
        // B comes inside a key's window, so it composes ahead during the
        // flip.
        term.key_sent();
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2HB\x1b[2;1H"]);
        video.flipping = false;
        video.busy_once = true;
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (2, 1), "refused as busy");
        turn(&mut term, &mut view, &mut video, &[]);
        assert_eq!((video.composed, video.sent), (2, 2), "sent at the next try");
        assert_eq!(counted(&mut view), (2, 0));
    }

    // On the render thread's path, a pass that composes nothing while a
    // newer frame is composing leaves the kept frame alone: the newer frame
    // is drawn over it and carries it, and the timer counts one frame.
    #[test]
    fn a_kept_frame_waits_while_a_newer_frame_is_composing() {
        let (mut term, mut view) = view();
        view.timer = Some(RenderTimer::new());
        let mut video = Flip::with(3);
        turn(&mut term, &mut view, &mut video, &[b"A\x1b[2;1H"]);
        video.flipping = true;
        // B comes inside a key's window, so it composes ahead during the
        // flip.
        term.key_sent();
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;2HB\x1b[2;1H"]);
        video.flipping = false;
        let c = b"\x1b[1;3HC\x1b[2;1H";
        assert_eq!(term.parse(c, &mut |_| {}), c.len());
        let key = (PathBuf::new(), ID);
        let mut snapshot = None;
        let state = DrawState::default();
        let clock = &Instant::now;
        let Prepared::Job(job) = view.prepare(
            &key,
            &mut term,
            &mut snapshot,
            &mut video,
            &state,
            None,
            clock,
        ) else {
            panic!("a job for the render thread");
        };
        assert!(view.away());
        let sent = view.send_kept(&mut video);
        assert_eq!((sent.queued, video.sent), (false, 1), "nothing sent");
        let shown = view.present(job.compose(None), &mut term, &mut video, true, clock);
        assert!(shown.queued);
        assert_eq!((video.composed, video.sent), (3, 2));
        assert_eq!(
            counted(&mut view),
            (2, 0),
            "B with the frame that carries it"
        );
    }

    // With sixel offered, the parts of a picture before its end compose
    // no frame, and the part that ends it composes exactly one, which
    // shows the picture.
    #[cfg(feature = "images")]
    #[test]
    fn a_sixel_picture_composes_one_frame_at_its_end() {
        let (mut term, mut view) = view();
        offer_pictures(&mut term);
        view.timer = Some(RenderTimer::new());
        let mut video = Flip::with(1);
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;1Hp1\x1b[2;1H"]);
        let parts = sixel_parts();
        let (end, body) = parts.split_last().unwrap();
        assert!(body.len() >= 4, "a picture of several parts");
        for part in body {
            turn(&mut term, &mut view, &mut video, &[part]);
        }
        assert_eq!(video.composed, 1, "no frame for the parts before the end");
        turn(&mut term, &mut view, &mut video, &[end]);
        assert_eq!(term.terminal.screen().placements().len(), 1, "placed");
        assert_eq!(video.composed, 2, "one frame for the end");
        assert_eq!(counted(&mut view), (2, 0));
    }

    // A kitty edit to the frame a placed picture shows marks the cells under
    // the picture's placements, so one turn of the loop after it draws the
    // new pixels.
    #[cfg(feature = "images")]
    #[test]
    fn an_edit_to_the_frame_a_picture_shows_is_drawn() {
        let (mut term, mut view) = view();
        offer_pictures(&mut term);
        let mut video = Plane::new(false);
        let color = |video: &Plane| video.pixel(5, 8) & 0xff_ff_ff;
        let place = format!(
            "\x1b[?25l\x1b_Ga=T,f=32,s=8,v=16,i=1,q=2;{}\x1b\\",
            kitty_fill([255, 0, 0, 255])
        );
        turn(&mut term, &mut view, &mut video, &[place.as_bytes()]);
        assert_eq!(color(&video), 0xff_00_00, "placed red");
        let edit = format!(
            "\x1b_Ga=f,i=1,r=1,f=32,s=8,v=16,q=2;{}\x1b\\",
            kitty_fill([0, 255, 0, 255])
        );
        turn(&mut term, &mut view, &mut video, &[edit.as_bytes()]);
        assert_eq!(color(&video), 0x00_ff_00, "the edit is drawn");
    }

    // A relative placement whose parent is a virtual placement has no box
    // of its own: it draws where its parent's placeholder cell puts it, two
    // cells right of it. Placing it, editing the frame it shows and
    // deleting it each draw the screen again (ADR-1100, D13).
    #[cfg(feature = "images")]
    #[test]
    fn a_virtual_parents_child_is_drawn_when_placed_edited_and_deleted() {
        let (mut term, mut view) = view();
        offer_pictures(&mut term);
        let mut video = Plane::new(false);
        // the middle of cell 2 of the first row
        let color = |video: &Plane| video.pixel(2 + 2 * 8 + 3, 8) & 0xff_ff_ff;
        let parent = format!(
            "\x1b[?25l\x1b_Ga=t,f=32,s=8,v=16,i=42,q=2;{}\x1b\\\
             \x1b_Ga=p,U=1,i=42,p=5,c=1,r=1,q=2\x1b\\\
             \x1b[1;1H\x1b[38;5;42m\x1b[58;5;5m\u{10EEEE}\u{0305}\u{0305}\x1b[m",
            kitty_fill([255, 0, 0, 255])
        );
        turn(&mut term, &mut view, &mut video, &[parent.as_bytes()]);
        let empty = color(&video);
        let child = format!(
            "\x1b_Ga=t,f=32,s=8,v=16,i=43,q=2;{}\x1b\\",
            kitty_fill([0, 255, 0, 255])
        );
        turn(&mut term, &mut view, &mut video, &[child.as_bytes()]);
        assert_eq!(color(&video), empty, "transmitted, not placed");
        let place = b"\x1b_Ga=p,i=43,p=1,P=42,Q=5,H=2,q=2\x1b\\";
        turn(&mut term, &mut view, &mut video, &[place]);
        assert_eq!(color(&video), 0x00_ff_00, "the child is drawn");
        let edit = format!(
            "\x1b_Ga=f,i=43,r=1,f=32,s=8,v=16,q=2;{}\x1b\\",
            kitty_fill([0, 0, 255, 255])
        );
        turn(&mut term, &mut view, &mut video, &[edit.as_bytes()]);
        assert_eq!(color(&video), 0x00_00_ff, "the edit is drawn");
        let delete = b"\x1b_Ga=d,d=i,i=43,q=2\x1b\\";
        turn(&mut term, &mut view, &mut video, &[delete]);
        assert_eq!(color(&video), empty, "the delete is drawn");
    }

    // A kitty transmission with no placement, `a=t`, changes nothing on the
    // screen, so it composes no frame.
    #[cfg(feature = "images")]
    #[test]
    fn a_transmission_alone_composes_no_frame() {
        let (mut term, mut view) = view();
        offer_pictures(&mut term);
        view.timer = Some(RenderTimer::new());
        let mut video = Flip::with(1);
        turn(&mut term, &mut view, &mut video, &[b"\x1b[1;1Hp1\x1b[2;1H"]);
        assert_eq!(video.composed, 1);
        let send = format!(
            "\x1b_Ga=t,f=32,s=8,v=16,i=1,q=2;{}\x1b\\",
            kitty_fill([255, 0, 0, 255])
        );
        turn(&mut term, &mut view, &mut video, &[send.as_bytes()]);
        assert_eq!(term.terminal.screen().images().image_count(), 1, "stored");
        assert_eq!(video.composed, 1, "no frame for the transmission");
        assert_eq!(counted(&mut view), (1, 0));
    }

    // What output changes of a frame beyond its cells is the cursor: its
    // place, DECTCEM and DECSCUSR each change the screen, while a pen
    // change, a title and a cursor moved where it is change nothing. A
    // palette change, OSC 12's cursor color among them, marks every cell.
    #[test]
    fn the_cursor_alone_changes_the_screen() {
        let (mut term, _) = view();
        for (output, changed) in [
            (&b"\x1b[2;2H"[..], true),
            (b"\x1b[?25l", true),
            (b"\x1b[?25h", true),
            (b"\x1b[5 q", true),
            (b"\x1b]12;#ff0000\x07", true),
            (b"x", true),
            (b"\x1b[1;1H\x1b[2;1H", false),
            (UNSEEN, false),
        ] {
            term.changed = false;
            term.parse(b"\x1b[2;1H", &mut |_| {});
            term.changed = false;
            term.parse(output, &mut |_| {});
            assert_eq!(
                term.changed,
                changed,
                "{:?}",
                String::from_utf8_lossy(output)
            );
        }
    }
}
