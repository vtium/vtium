//! [`Renderer`]: draws frames into a [`Target`], redrawing only what
//! changed for the buffer being drawn.

use std::time::{Duration, Instant};

use vtium_font::{Decorations, Face, Font, Style, Width as FontWidth};
use vtium_term::{Attrs, Cursor, CursorShape, Frame, FrameCell, Glyph, Palette, Rgb, Width, dim};

use crate::arrow::{Arrow, Over};
use crate::blend::pixel;
use crate::bounds::ROW_SCRATCH_MAX;
use crate::cache::{Budgets, GlyphCache, Profile, Storage};
use crate::compose::{Area, Composer, Job, int};
use crate::damage::Cells;
use crate::deco::{Kind, Sprites};
use crate::mono::{self, Depth, Fallback, Ink, Threshold};
#[cfg(feature = "images")]
use crate::picture::{Mark, PictureCounts, Under};
use crate::target::{Orientation, Rect, Target};

#[cfg(feature = "gpu")]
mod gpu;

/// The bar cursor: U+258F, LEFT ONE EIGHTH BLOCK, a sprite.
const BAR: char = '\u{258f}';

/// The color a decoration draws in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecoColor {
    /// The cell's foreground as drawn.
    #[default]
    Foreground,
    /// A palette color, resolved through each frame's palette, so a
    /// program that changes it (OSC 4) recolors the decorations.
    Indexed(u8),
    Rgb(Rgb),
}

impl DecoColor {
    /// The color through `palette`; `None` follows the foreground.
    fn resolve(self, palette: &Palette) -> Option<Rgb> {
        match self {
            DecoColor::Foreground => None,
            DecoColor::Indexed(i) => Some(palette.color(i)),
            DecoColor::Rgb(c) => Some(c),
        }
    }
}

/// The configured decoration colors: `underline-color`, `strike-color` and
/// `force-decoration-colors`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecorationColors {
    pub underline: DecoColor,
    pub strike: DecoColor,
    /// The configured underline color wins over the one SGR 58 set.
    pub force: bool,
}

/// How blinking text, SGR 5 and 6, draws in the blink's off phase:
/// `blink-text`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlinkStyle {
    /// The glyph hides.
    Full,
    /// The glyph draws halfway to its background, as SGR 2 draws it. The
    /// default, the maintainer's decision of 2026-11-03.
    #[default]
    Partial,
    /// The glyph stays: blinking text draws steady.
    Off,
}

/// The text blink's options: `blink-text` and `blink-decorations`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextBlink {
    pub style: BlinkStyle,
    /// The underline and the strike blink with the glyph: hidden with it
    /// or dimmed with it. Otherwise they stay as the on phase draws them.
    pub decorations: bool,
}

/// What the pointer draws as: `pointer-char`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointerShape {
    /// The arrow, its tip on the pointer's pixel.
    #[default]
    Arrow,
    /// The font's glyph for the character, one cell wide, on the cell
    /// under the pointer, filled and outlined as the arrow is.
    Glyph(char),
}

/// How a renderer draws, fixed for its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub orientation: Orientation,
    /// The cache's profile, `glyph-cache`.
    pub profile: Profile,
    /// Budgets that replace the profile's.
    pub budgets: Option<Budgets>,
    pub storage: Storage,
    pub decorations: DecorationColors,
    pub blink: TextBlink,
    pub pointer: PointerShape,
    /// The display's depth: at [`Depth::One`] it is drawn in black and
    /// white (`src/mono.rs`).
    pub depth: Depth,
}

impl Default for Config {
    /// Unturned, the default profile and both storage savings.
    fn default() -> Config {
        Config {
            orientation: Orientation::default(),
            profile: Profile::default(),
            budgets: None,
            storage: Storage::default(),
            decorations: DecorationColors::default(),
            blink: TextBlink::default(),
            pointer: PointerShape::default(),
            depth: Depth::default(),
        }
    }
}

/// What a frame needs beyond the terminal's state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawState {
    /// The cursor's blink phase shows it.
    pub cursor_on: bool,
    /// The text blink is in its off phase, which draws blinking text as
    /// [`Config::blink`] says.
    pub blink_hidden: bool,
    /// The palette's default background, around the grid.
    pub background: Rgb,
    /// The pointer's tip in the grid's pixels, unturned, for the renderer
    /// to draw the arrow at: set only on a display without a cursor plane
    /// (fbdev, legacy drm2d, a plane that refused the arrow), which the
    /// display tells its owner. `None` when the pointer is hidden or the
    /// display's cursor plane shows it.
    pub pointer: Option<(u32, u32)>,
}

/// One frame's figures, for the render timer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Cells drawn.
    pub cells: u32,
    /// Rows scanned for cells to draw.
    pub rows: u32,
    /// Glyphs the font drew.
    pub misses: u32,
    /// Of those, the glyphs drawn without being cached.
    pub uncached: u32,
    /// Pixels in the damage rectangles.
    pub damage_px: u64,
    /// The time the frame took to draw.
    pub cpu: Duration,
}

/// What drawing a frame did: the rectangles it wrote, to hand the display
/// as damage clips, and its figures. A frame with no damage changed
/// nothing, and is not swapped.
#[derive(Debug)]
pub struct Drawn<'a> {
    pub damage: &'a [Rect],
    pub stats: Stats,
}

/// Where the grid sits on the display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Grid {
    cols: usize,
    rows: usize,
    off_x: i64,
    off_y: i64,
    max_x: i64,
    max_y: i64,
}

/// The display's size, the cell's and the orientation: where the grid's
/// pixels land on the display.
#[derive(Clone, Copy, Debug)]
struct Layout {
    orientation: Orientation,
    width: u32,
    height: u32,
    cw: i64,
    ch: i64,
    max_cols: usize,
    max_rows: usize,
}

impl Layout {
    /// The row scratch's line length and lines.
    fn strip(&self) -> (usize, usize) {
        let (line, lines) = if self.orientation.turned() {
            (self.ch, int(self.height))
        } else {
            (int(self.width), self.ch)
        };
        let line = usize::try_from(line).unwrap_or(0).max(1);
        let lines = usize::try_from(lines).unwrap_or(0);
        let fit = (ROW_SCRATCH_MAX / 4).checked_div(line).unwrap_or(0).max(1);
        (line, lines.min(fit))
    }

    fn grid_for(&self, frame: &Frame<'_>) -> Grid {
        let cols = frame.cols().min(self.max_cols);
        let rows = frame.rows().min(self.max_rows);
        let gw = int(cols).saturating_mul(self.cw);
        let gh = int(rows).saturating_mul(self.ch);
        let (sw, sh) = (int(self.width), int(self.height));
        let (across, down) = if self.orientation.turned() {
            (gh, gw)
        } else {
            (gw, gh)
        };
        let off_x = sw.saturating_sub(across) / 2;
        let off_y = sh.saturating_sub(down) / 2;
        Grid {
            cols,
            rows,
            off_x,
            off_y,
            max_x: off_x.saturating_add(across),
            max_y: off_y.saturating_add(down),
        }
    }

    /// The display area of the grid's pixels `lx`, `ly`, `lw` by `lh`.
    fn area(&self, g: &Grid, lx: i64, ly: i64, lw: i64, lh: i64) -> Area {
        let (x, y, w, h) = match self.orientation {
            Orientation::Normal => (
                g.off_x.saturating_add(lx),
                g.off_y.saturating_add(ly),
                lw,
                lh,
            ),
            Orientation::UpsideDown => (
                g.max_x.saturating_sub(lx.saturating_add(lw)),
                g.max_y.saturating_sub(ly.saturating_add(lh)),
                lw,
                lh,
            ),
            Orientation::Right => (
                g.max_x.saturating_sub(ly.saturating_add(lh)),
                g.off_y.saturating_add(lx),
                lh,
                lw,
            ),
            Orientation::Left => (
                g.off_x.saturating_add(ly),
                g.max_y.saturating_sub(lx.saturating_add(lw)),
                lh,
                lw,
            ),
        };
        Area { x, y, w, h }
    }

    /// The grid's pixels under the display area `a`: [`Layout::area`]
    /// undone.
    fn grid_area(&self, g: &Grid, a: Area) -> Area {
        let (lx, ly, lw, lh) = match self.orientation {
            Orientation::Normal => (
                a.x.saturating_sub(g.off_x),
                a.y.saturating_sub(g.off_y),
                a.w,
                a.h,
            ),
            Orientation::UpsideDown => (
                g.max_x.saturating_sub(a.x.saturating_add(a.w)),
                g.max_y.saturating_sub(a.bottom()),
                a.w,
                a.h,
            ),
            Orientation::Right => (
                a.y.saturating_sub(g.off_y),
                g.max_x.saturating_sub(a.x.saturating_add(a.w)),
                a.h,
                a.w,
            ),
            Orientation::Left => (
                g.max_y.saturating_sub(a.bottom()),
                a.x.saturating_sub(g.off_x),
                a.h,
                a.w,
            ),
        };
        Area {
            x: lx,
            y: ly,
            w: lw,
            h: lh,
        }
    }

    /// The display area of `n` cells from `col` in `row`.
    fn cells_area(&self, g: &Grid, col: usize, row: usize, n: usize) -> Area {
        self.area(
            g,
            int(col).saturating_mul(self.cw),
            int(row).saturating_mul(self.ch),
            int(n).saturating_mul(self.cw),
            self.ch,
        )
    }

    /// The display rectangle of the cell at `col`, `row`.
    fn cell_rect(&self, g: &Grid, col: usize, row: usize) -> Rect {
        let a = self.cells_area(g, col, row, 1);
        Rect {
            x: u32::try_from(a.x).unwrap_or(0),
            y: u32::try_from(a.y).unwrap_or(0),
            width: u32::try_from(a.w).unwrap_or(0),
            height: u32::try_from(a.h).unwrap_or(0),
        }
    }
}

/// One frame's inputs, as each row reads them.
struct Pass<'p, 'f> {
    g: Grid,
    frame: &'p Frame<'f>,
    state: &'p DrawState,
    cursor: Cursor,
    /// Where the cursor draws, when it shows.
    at: Option<(usize, usize)>,
    /// At 1 bit, the frame without its selection, whose cells say what a
    /// selected cell draws unselected.
    plain: Option<Frame<'f>>,
}

/// A cell's place: its column, row and the cells it draws over.
#[derive(Clone, Copy, Debug)]
struct Place {
    col: usize,
    row: usize,
    ncells: usize,
}

/// What a job paints: its colors, decorations and text.
#[derive(Clone, Copy, Debug)]
struct Paint {
    fg: Rgb,
    bg: Rgb,
    underline: Option<(Kind, Rgb)>,
    strike: Option<Rgb>,
    text: Option<(usize, usize, FontWidth, Style)>,
    /// At 1 bit, whether the background is lit, where vtium marks the cell
    /// (`mono::marked`).
    lit: Option<bool>,
}

/// Where the pointer's arrow draws and the cells it changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pointer {
    at: Area,
    /// The first column and row of the cells it changes, and the column
    /// and row past the last.
    cells: (usize, usize, usize, usize),
    /// It reaches past the grid into the border.
    outside: bool,
}

/// What a frame draws, decided before anything is drawn.
#[derive(Debug)]
struct Plan {
    grid: Grid,
    pointer: Option<Pointer>,
    /// The buffer lacks the whole border.
    full: bool,
    /// The areas of the border it lacks, when not the whole.
    border: Vec<Rect>,
}

/// How a frame's pixels are made: the border and each row's cell jobs.
/// The CPU path writes them into the display's buffer ([`Cpu`]); the GPU
/// path turns them into clears and quads for the upright frame.
pub(crate) trait Back {
    /// Fills the border inside the display area `r` with `bg`. `display`
    /// is the whole display and `grid` the grid's area on it, `None` for
    /// an empty grid: the border is what the grid leaves.
    fn border(&mut self, compose: &mut Composer, display: (Rect, Option<Area>), bg: Rgb, r: Rect);

    /// Draws the row's jobs, which `compose` holds with their text.
    fn row<F: Face>(&mut self, compose: &mut Composer, font: &Font<F>, strip: (usize, usize));
}

/// The CPU path's [`Back`]: the row scratch, written into a target.
struct Cpu<'a, T: Target>(&'a mut T);

impl<T: Target> Back for Cpu<'_, T> {
    fn border(
        &mut self,
        compose: &mut Composer,
        (display, grid): (Rect, Option<Area>),
        bg: Rgb,
        r: Rect,
    ) {
        let (width, height) = (display.width, display.height);
        let (x0, x1) = (r.x.min(width), r.x.saturating_add(r.width).min(width));
        let y1 = r.y.saturating_add(r.height).min(height);
        let line = compose.solid(
            usize::try_from(x1.saturating_sub(x0)).unwrap_or(0),
            pixel(bg),
        );
        // the grid's columns, as a part of the run from x0
        let at = |v: i64| usize::try_from(v.clamp(int(x0), int(x1)).saturating_sub(int(x0)));
        let g = grid.unwrap_or_default();
        let (gl, gr) = (at(g.x).unwrap_or(0), at(g.right()).unwrap_or(0));
        for y in r.y..y1 {
            let yi = int(y);
            if grid.is_none() || yi < g.y || yi >= g.bottom() {
                self.0.write(x0, y, line);
                continue;
            }
            if let Some(left) = line.get(..gl).filter(|l| !l.is_empty()) {
                self.0.write(x0, y, left);
            }
            if let Some(right) = line.get(gr..).filter(|l| !l.is_empty()) {
                let x = x0.saturating_add(u32::try_from(gr).unwrap_or(u32::MAX));
                self.0.write(x, y, right);
            }
        }
    }

    fn row<F: Face>(&mut self, compose: &mut Composer, font: &Font<F>, strip: (usize, usize)) {
        compose.draw(font, strip, &mut *self.0);
    }
}

/// Draws terminal frames with a font into a display buffer.
#[derive(Debug)]
pub struct Renderer<F> {
    font: Font<F>,
    config: Config,
    layout: Layout,
    grid: Option<Grid>,
    cells: Cells,
    background: Option<Rgb>,
    blink_hidden: bool,
    cursor_at: Option<(usize, usize)>,
    arrow: Arrow,
    /// The pointer's image upright, which the GPU path draws.
    upright_arrow: Arrow,
    pointer: Option<Pointer>,
    /// The line a run under the pointer is blended in.
    pointer_line: Vec<u32>,
    /// The line a run is thresholded in, on a 1-bit display.
    mono_line: Vec<u32>,
    /// The configured underline and strike colors through the frame being
    /// drawn's palette; `None` follows the foreground.
    deco: (Option<Rgb>, Option<Rgb>),
    compose: Composer,
    /// The font's underline and strike-through lines the sprites and the
    /// glyph cache's stand-ins were made from.
    lines: Decorations,
    /// Characters the font draws differently since the last frame, sorted:
    /// the cells showing one draw again ([`Renderer::glyphs_changed`]).
    changed: Vec<char>,
    /// Display areas something other than the renderer drew over since the
    /// last frame ([`Renderer::redraw_area`]).
    touched: Vec<Rect>,
    /// What the GPU path keeps of this renderer's.
    #[cfg(feature = "gpu")]
    gpu: gpu::State,
}

/// The pointer's image for cells `cell_height` tall, upright: the arrow,
/// or the glyph `config.pointer` names. The arrow stands in for a character
/// the font lacks, which would draw as a fallback (the missing box, U+FFFD
/// or `?`), and for a blank one, whose glyph covers no pixel.
fn pointer_image<F: Face>(font: &Font<F>, cell_height: u32, config: Config) -> Arrow {
    match config.pointer {
        PointerShape::Glyph(c) if font.has_glyph(c) => {
            font.glyph(&[c], FontWidth::Narrow, Style::default())
        }
        _ => None,
    }
    .map(|g| Arrow::upright_glyph(&g))
    .filter(Arrow::covers)
    .unwrap_or_else(|| Arrow::upright(cell_height))
}

/// The decoration sprites for cells `cw` by `ch` in the font's `lines`,
/// turned as `orientation` says, stored as `config` says.
fn sprites(
    cw: u32,
    ch: u32,
    lines: &Decorations,
    orientation: Orientation,
    config: Config,
) -> Sprites {
    Sprites::new(
        cw,
        ch,
        lines,
        orientation,
        config.storage.bits,
        config.depth == Depth::One,
    )
}

/// How many cells a cell `width` wide, after a cell `before` wide, draws
/// over: 0 for the trailing half of a wide character, which the leading
/// half draws; 2 for a leading half with its trailing half next on the
/// screen, whose width `next` reads; else 1. A half whose partner is
/// missing draws as one cell, so no frame leaves a cell undrawn.
fn span(width: Width, before: Option<Width>, next: impl FnOnce() -> Option<Width>) -> usize {
    match width {
        Width::WideTail if before == Some(Width::Wide) => 0,
        Width::Wide if next() == Some(Width::WideTail) => 2,
        _ => 1,
    }
}

/// `chars` sorted, each once.
fn sorted(chars: &[char]) -> Vec<char> {
    let mut out = chars.to_vec();
    out.sort_unstable();
    out.dedup();
    out
}

/// The width of the cell after `col` in `row`, among the grid's `cols`.
fn next_width(frame: &Frame<'_>, row: usize, col: usize, cols: usize) -> Option<Width> {
    let next = col.saturating_add(1);
    (next < cols)
        .then(|| frame.cell(row, next))
        .flatten()
        .map(|fc| fc.width)
}

/// Whether `fc` draws in the text blink's off phase: blinking, not under
/// the cursor, in the off phase, with `blink` not off.
fn blink_off(fc: &FrameCell<'_>, is_cursor: bool, state: &DrawState, blink: TextBlink) -> bool {
    let blinks = fc.attrs.contains(Attrs::BLINK) || fc.attrs.contains(Attrs::RAPID_BLINK);
    !is_cursor && blinks && state.blink_hidden && blink.style != BlinkStyle::Off
}

impl<F: Face> Renderer<F> {
    /// A renderer drawing with `font` on a display `width` by `height`
    /// pixels.
    pub fn new(font: Font<F>, width: u32, height: u32, config: Config) -> Renderer<F> {
        let config = Config {
            blink: mono::blink(config.depth, config.blink),
            ..config
        };
        let cell = font.cell();
        let (cw, ch) = (cell.width.max(1), cell.height.max(1));
        let (across, down) = if config.orientation.turned() {
            (height, width)
        } else {
            (width, height)
        };
        let max_cols = usize::try_from(across.checked_div(cw).unwrap_or(0)).unwrap_or(0);
        let max_rows = usize::try_from(down.checked_div(ch).unwrap_or(0)).unwrap_or(0);
        let cells = u32::try_from(max_cols.saturating_mul(max_rows)).unwrap_or(u32::MAX);
        let budgets = config
            .budgets
            .unwrap_or_else(|| Budgets::for_profile(config.profile, cells, width, height));
        let lines = font.decorations();
        let upright_arrow = pointer_image(&font, ch, config);
        let arrow = upright_arrow.clone().turned(config.orientation);
        #[cfg(feature = "gpu")]
        let gpu = gpu::State::new(
            budgets,
            sprites(cw, ch, &lines, Orientation::Normal, config),
        );
        Renderer {
            compose: Composer::new(
                GlyphCache::new(budgets, config.storage, config.orientation).with_fallback(
                    Fallback::new(config.depth, font.cell(), lines.underline_thickness),
                ),
                sprites(cw, ch, &lines, config.orientation, config),
            ),
            lines,
            font,
            config,
            layout: Layout {
                orientation: config.orientation,
                width,
                height,
                cw: int(cw),
                ch: int(ch),
                max_cols,
                max_rows,
            },
            grid: None,
            cells: Cells::default(),
            background: None,
            blink_hidden: false,
            cursor_at: None,
            arrow,
            upright_arrow,
            pointer: None,
            pointer_line: Vec::new(),
            mono_line: Vec::new(),
            deco: (None, None),
            changed: Vec::new(),
            touched: Vec::new(),
            #[cfg(feature = "gpu")]
            gpu,
        }
    }

    /// This renderer on the same display turned as `orientation` says, with
    /// its font and the rest of its configuration. On the CPU path it is a
    /// new renderer: its cache holds glyphs turned for the old orientation,
    /// and its budgets follow the turned grid's cells. On the GPU path,
    /// whose glyphs are upright, it keeps its identity, so a pipeline that
    /// holds its glyphs keeps them, and the turn uploads and compiles
    /// nothing (ADR-1303 D3); those glyphs keep the budgets their pools were
    /// made with until the pipeline makes them anew. The font's lines are
    /// taken first, as [`Renderer::glyphs_changed`] takes them, so lines
    /// that changed before the turn reach the GPU path's sprites too. Its
    /// next frame draws whole.
    pub fn turned(mut self, orientation: Orientation) -> Renderer<F> {
        self.lines_changed();
        let config = Config {
            orientation,
            ..self.config
        };
        let turned = Renderer::new(self.font, self.layout.width, self.layout.height, config);
        #[cfg(feature = "gpu")]
        let turned = Renderer {
            gpu: turned.gpu.keeping_identity(self.gpu),
            ..turned
        };
        turned
    }

    /// The font now draws each of `chars` differently, as when the deferred
    /// pipeline's generated glyphs arrive (ADR-1301 D2): the cache drops
    /// the glyphs whose text holds one of `chars`, alone or in a cluster,
    /// and the next frame draws again each cell that shows one (ADR-1200
    /// D23). Every other glyph stays cached, and every other cell keeps its
    /// pixels.
    ///
    /// When the font's underline and strike-through lines changed too, as
    /// an outline font's do once the glyph helper has measured them, the
    /// renderer takes the new lines ([`Renderer::lines_changed`]).
    pub fn glyphs_changed(&mut self, chars: &[char]) {
        self.lines_changed();
        let arrived = sorted(chars);
        self.compose.cache.forget(&arrived);
        #[cfg(feature = "gpu")]
        self.gpu.forget(&arrived);
        self.changed.extend_from_slice(&arrived);
        self.changed.sort_unstable();
        self.changed.dedup();
    }

    /// Forgets what the renderer drew into each buffer: the next frame
    /// places the grid anew and stamps every cell and the border with its
    /// own serial, so it, and the first later frame into each other
    /// buffer, draws them all. A head calls it when a frame the renderer
    /// recorded did not reach its buffer whole, as when the GPU path drew a
    /// frame and reading it back into the buffer failed, before it draws
    /// the frame again on the CPU path.
    pub fn forget_history(&mut self) {
        self.grid = None;
    }

    /// Something other than the renderer drew over the display area `area`
    /// in the buffers it drew before, as vtium's overlay, a notice box,
    /// does. The next frame draws again every cell `area` touches and the
    /// border inside it, and so does each later frame drawn into a buffer
    /// older than the next one, so every buffer loses what was drawn over
    /// it whatever order the buffers come in.
    pub fn redraw_area(&mut self, area: Rect) {
        self.touched.push(area);
    }

    /// The font now draws each of `chars` differently, and no cell shows
    /// one: the cache drops the glyphs whose text holds one, alone or in a
    /// cluster, so a cell that shows one later draws the font's new glyph
    /// (ADR-1200 D7), and no cell draws again now. Every other glyph stays
    /// cached.
    ///
    /// When the font's lines changed too, the renderer takes them, as
    /// [`Renderer::glyphs_changed`] does.
    pub fn forget_glyphs(&mut self, chars: &[char]) {
        self.lines_changed();
        self.compose.cache.forget(&sorted(chars));
        #[cfg(feature = "gpu")]
        self.gpu.forget(chars);
    }

    /// Takes the font's underline and strike-through lines when they are
    /// no longer the ones the renderer made its sprites from: the sprites
    /// are made again, the glyph cache is emptied, since the font draws its
    /// line sprites and the glyph stand-ins in the underline's thickness,
    /// and the next frame draws every cell and the border. On the GPU path
    /// the renderer takes a new identity, so the pipeline makes its glyphs
    /// and sprites anew and composes whole. Unchanged lines change nothing.
    fn lines_changed(&mut self) {
        let lines = self.font.decorations();
        if lines == self.lines {
            return;
        }
        self.lines = lines;
        let config = self.config;
        let (cw, ch) = (
            u32::try_from(self.layout.cw).unwrap_or(1),
            u32::try_from(self.layout.ch).unwrap_or(1),
        );
        self.compose.sprites = sprites(cw, ch, &lines, config.orientation, config);
        self.compose.cache.reset(Fallback::new(
            config.depth,
            self.font.cell(),
            lines.underline_thickness,
        ));
        // the grid is placed anew, so every cell and the border draw
        self.grid = None;
        #[cfg(feature = "gpu")]
        self.gpu
            .lines_changed(sprites(cw, ch, &lines, Orientation::Normal, config));
    }

    pub fn font(&self) -> &Font<F> {
        &self.font
    }

    /// The display's depth this renderer draws at.
    pub fn depth(&self) -> Depth {
        self.config.depth
    }

    /// The pointer's arrow at this renderer's cell size and orientation,
    /// for a display's cursor plane.
    pub fn arrow(&self) -> &Arrow {
        &self.arrow
    }

    /// The pointer's arrow at this renderer's cell size, upright, which the
    /// GPU path draws into the upright frame (ADR-1303 D3).
    pub fn upright_arrow(&self) -> &Arrow {
        &self.upright_arrow
    }

    /// Where on the display the arrow's hotspot goes for the pointer's tip
    /// at `x`, `y` in the grid's pixels: the grid's pixel kept inside the
    /// grid, turned and placed as the last frame placed the grid. `None`
    /// before the first frame, or with no grid.
    pub fn pointer_at(&self, (x, y): (u32, u32)) -> Option<(i32, i32)> {
        let g = self.grid?;
        let (px, py) = self.pointer_pixel(&g, (x, y))?;
        let a = self.layout.area(&g, px, py, 1, 1);
        Some((
            i32::try_from(a.x).unwrap_or(i32::MAX),
            i32::try_from(a.y).unwrap_or(i32::MAX),
        ))
    }

    /// The columns and rows the display holds.
    pub fn grid_size(&self) -> (usize, usize) {
        (self.layout.max_cols, self.layout.max_rows)
    }

    /// Bytes of glyph pixels the cache holds.
    pub fn cache_bytes(&self) -> u64 {
        self.compose.cache.pixel_bytes()
    }

    /// What this display's scaled copies of pictures did and hold: the
    /// times a picture was scaled, the copies kept, evicted or not kept,
    /// and their bytes, at most [`crate::PICTURE_CACHE_SCREENS`] screens.
    #[cfg(feature = "images")]
    pub fn picture_counts(&self) -> PictureCounts {
        self.compose.pictures.counts()
    }

    /// Bytes the row scratch holds once a row is drawn: one row of cells
    /// across the display, at most `ROW_SCRATCH_MAX`.
    pub fn row_scratch_bytes(&self) -> usize {
        let (line, lines) = self.layout.strip();
        line.saturating_mul(lines).saturating_mul(4)
    }

    /// Draws `frame` into `target`, a buffer `age` frames old: 1 when it
    /// holds the last frame this renderer drew damage for, n when it holds
    /// the frame n back, 0 when its contents are unknown. The cells that
    /// changed in the last `age` frames are drawn, or at age 0, or at an age
    /// past this renderer's history, the whole picture with its border;
    /// another buffer's history is never disturbed.
    ///
    /// `frame` is read since the checkpoint taken after this renderer's
    /// previous `draw` (`Age::NEVER` before the first), whatever buffer that
    /// drew into and whether or not it was swapped: the renderer keeps each
    /// buffer's history itself. An older checkpoint draws more than needed;
    /// a newer one loses changes.
    ///
    /// A frame whose damage is empty changed nothing: it is not swapped,
    /// and does not count as a frame for the ages that follow.
    #[expect(
        clippy::disallowed_methods,
        reason = "the render timer's CPU time per frame"
    )]
    pub fn draw(
        &mut self,
        frame: &Frame<'_>,
        target: &mut impl Target,
        age: u32,
        state: &DrawState,
    ) -> Drawn<'_> {
        let start = Instant::now();
        #[cfg(feature = "gpu")]
        self.gpu.drew_on_cpu();
        let plan = self.plan(frame, age, state);
        let arrow = core::mem::take(&mut self.arrow);
        let mut line = core::mem::take(&mut self.pointer_line);
        let mut mono_line = core::mem::take(&mut self.mono_line);
        let mut target = Threshold::new(target, self.config.depth, &mut mono_line);
        let mut out = Over::new(&mut target, plan.pointer.map(|p| (&arrow, p.at)), &mut line);
        let (cells, rows) = self.paint(&plan, frame, state, &mut Cpu(&mut out));
        self.arrow = arrow;
        self.pointer_line = line;
        self.mono_line = mono_line;
        self.finish(&plan, (cells, rows), start)
    }

    /// The first half of a frame: where the grid sits, what the pointer
    /// changed, and which parts of the border the buffer lacks.
    fn plan(&mut self, frame: &Frame<'_>, age: u32, state: &DrawState) -> Plan {
        self.compose.cache.begin_frame();
        self.cells.begin(age);
        let d = self.config.decorations;
        let palette = frame.palette();
        self.deco = (d.underline.resolve(palette), d.strike.resolve(palette));
        let grid = self.layout.grid_for(frame);
        if self.grid != Some(grid) || self.background != Some(state.background) {
            self.cells.reset(grid.cols, grid.rows);
            self.grid = Some(grid);
            self.background = Some(state.background);
            self.cursor_at = None;
            self.pointer = None;
        }
        let pointer = state.pointer.and_then(|p| self.place_pointer(&grid, p));
        if pointer != self.pointer {
            // it appeared, moved or hid: the cells it left
            // and the cells it covers change, and the border where it
            // reaches past the grid
            for p in [self.pointer, pointer].into_iter().flatten() {
                self.stamp_pointer(&p);
            }
            self.pointer = pointer;
        }
        for area in core::mem::take(&mut self.touched) {
            self.stamp_area(&grid, area);
        }
        #[cfg(feature = "images")]
        self.plan_pictures(frame, &grid);
        let full = self.cells.border_stale();
        let border: Vec<Rect> = if full {
            Vec::new()
        } else {
            self.cells.stale_border_areas().collect()
        };
        Plan {
            grid,
            pointer,
            full,
            border,
        }
    }

    /// The second half: the border and the cells the buffer lacks, through
    /// `back`. Returns how many cells it drew and how many rows it scanned.
    fn paint(
        &mut self,
        plan: &Plan,
        frame: &Frame<'_>,
        state: &DrawState,
        back: &mut impl Back,
    ) -> (u32, u32) {
        let all = Rect {
            x: 0,
            y: 0,
            width: self.layout.width,
            height: self.layout.height,
        };
        let whole_border = [all];
        let bg = mono::border(self.config.depth, state.background);
        let g = &plan.grid;
        let grid = (g.cols > 0 && g.rows > 0).then_some(Area {
            x: g.off_x,
            y: g.off_y,
            w: g.max_x.saturating_sub(g.off_x),
            h: g.max_y.saturating_sub(g.off_y),
        });
        for &r in if plan.full {
            &whole_border[..]
        } else {
            &plan.border[..]
        } {
            back.border(&mut self.compose, (all, grid), bg, r);
        }
        self.draw_cells(g, frame, back, state)
    }

    /// The frame's damage and figures, once it is drawn.
    fn finish(&mut self, plan: &Plan, (cells, rows): (u32, u32), start: Instant) -> Drawn<'_> {
        #[cfg(feature = "images")]
        self.compose.pictures.end_frame();
        self.changed.clear();
        let layout = self.layout;
        let grid = plan.grid;
        let whole = plan.full.then_some(Rect {
            x: 0,
            y: 0,
            width: layout.width,
            height: layout.height,
        });
        let counts = self.compose.cache.take_counts();
        let damage = self.cells.collect(whole, &plan.border, |col, row| {
            layout.cell_rect(&grid, col, row)
        });
        let damage_px = damage.iter().fold(0u64, |n, r| n.saturating_add(r.area()));
        Drawn {
            damage,
            stats: Stats {
                cells,
                rows,
                misses: counts.misses,
                uncached: counts.uncached,
                damage_px,
                cpu: start.elapsed(),
            },
        }
    }

    /// The frame's pictures: the cells of each one that moved or changed
    /// since the last frame are stamped, and each one's place on the
    /// display is planned for the rows to draw.
    #[cfg(feature = "images")]
    fn plan_pictures(&mut self, frame: &Frame<'_>, g: &Grid) {
        let placements = frame.placements();
        let cells = &mut self.cells;
        self.compose
            .pictures
            .damage(placements, (g.cols, g.rows), |c, r| cells.stamp(c, r));
        let l = self.layout;
        let grid_px = (
            int(g.cols).saturating_mul(l.cw),
            int(g.rows).saturating_mul(l.ch),
        );
        let cell = (
            u32::try_from(l.cw).unwrap_or(1),
            u32::try_from(l.ch).unwrap_or(1),
        );
        let g = *g;
        let pictures = &mut self.compose.pictures;
        pictures.display(l.width, l.height, l.orientation);
        pictures.plan(placements, cell, grid_px, g.rows, |a| {
            l.area(&g, a.x, a.y, a.w, a.h)
        });
    }

    /// Where the cursor draws: the leading cell of a wide character whose
    /// trailing half it sits on.
    fn cursor_cell(frame: &Frame<'_>, c: &Cursor) -> (usize, usize) {
        let width_at = |col: usize| frame.cell(c.row, col).map(|fc| fc.width);
        let before = c.col.checked_sub(1).and_then(width_at);
        let tail = width_at(c.col).is_some_and(|w| span(w, before, || None) == 0);
        match c.col.checked_sub(1) {
            Some(prev) if tail => (prev, c.row),
            _ => (c.col, c.row),
        }
    }

    /// Draws the cells this buffer lacks; returns how many, and how many
    /// rows it scanned for them.
    fn draw_cells(
        &mut self,
        g: &Grid,
        frame: &Frame<'_>,
        back: &mut impl Back,
        state: &DrawState,
    ) -> (u32, u32) {
        let cursor = frame.cursor();
        let at = (cursor.visible && state.cursor_on).then(|| Self::cursor_cell(frame, &cursor));
        let blink_toggled = state.blink_hidden != self.blink_hidden;
        self.blink_hidden = state.blink_hidden;
        let last_cursor = core::mem::replace(&mut self.cursor_at, at);
        let pass = Pass {
            g: *g,
            frame,
            state,
            cursor,
            at,
            plain: (self.config.depth == Depth::One)
                .then(|| frame.clone().with_selection(Vec::new())),
        };
        let (mut drawn, mut scanned) = (0u32, 0u32);
        for row in 0..g.rows {
            let scan = self.cells.row_stale(row)
                || self.row_shows_changed(frame, row, g.cols)
                || blink_toggled
                || frame.row_changed(row)
                || at.is_some_and(|(_, r)| r == row)
                || last_cursor.is_some_and(|(_, r)| r == row);
            if scan {
                scanned = scanned.saturating_add(1);
                drawn = drawn.saturating_add(self.draw_row(&pass, row, back));
            }
        }
        (drawn, scanned)
    }

    /// Whether `row` shows a character [`Renderer::glyphs_changed`] named,
    /// read from its line's glyphs alone.
    fn row_shows_changed(&self, frame: &Frame<'_>, row: usize, cols: usize) -> bool {
        !self.changed.is_empty()
            && frame.line(row).is_some_and(|line| {
                line.cells()
                    .iter()
                    .take(cols)
                    .any(|&c| self.shows_changed(&line.glyph(c)))
            })
    }

    /// Whether `glyph` shows a character [`Renderer::glyphs_changed`] named.
    fn shows_changed(&self, glyph: &Glyph<'_>) -> bool {
        let changed = |c: char| self.changed.binary_search(&c).is_ok();
        match *glyph {
            _ if self.changed.is_empty() => false,
            Glyph::Empty => false,
            Glyph::Char(c) => changed(c),
            Glyph::Cluster(text) => text.chars().any(changed),
        }
    }

    /// Scans one row, updates what each cell last drew, and draws the cells
    /// some buffer lacks; returns how many.
    fn draw_row(&mut self, p: &Pass<'_, '_>, row: usize, back: &mut impl Back) -> u32 {
        let g = &p.g;
        self.compose.jobs.clear();
        self.compose.text.clear();
        let mut count = 0u32;
        let mut before = None;
        for (here, fc) in p.frame.cells(row).take(g.cols).enumerate() {
            let ncells = span(fc.width, before, || next_width(p.frame, row, here, g.cols));
            before = Some(fc.width);
            // a cell that joins or leaves a wide pair draws differently
            let role = u8::try_from(ncells).unwrap_or(1);
            let mut changed = fc.changed || self.shows_changed(&fc.glyph);
            if let Some(s) = self.cells.get_mut(here, row) {
                changed |= core::mem::replace(&mut s.span, role) != role;
            }
            if ncells == 0 {
                continue;
            }
            let is_cursor = p.at == Some((here, row));
            let off = blink_off(&fc, is_cursor, p.state, self.config.blink);
            if ncells == 2 {
                let tail = here.saturating_add(1);
                changed |= p.frame.cell(row, tail).is_some_and(|t| t.changed);
                if let Some(t) = self.cells.get_mut(tail, row) {
                    changed |= core::mem::replace(&mut t.span, 0) != 0;
                }
            }
            let Some(s) = self.cells.get_mut(here, row) else {
                continue;
            };
            let look = is_cursor.then_some((p.cursor.shape, p.cursor.color));
            changed |= s.cursor != look || s.blink_off != off;
            s.cursor = look;
            s.blink_off = off;
            if changed {
                self.cells.stamp(here, row);
            }
            if !self.cells.stale_cells(here, row, ncells) {
                continue;
            }
            self.cells.mark_drawn(here, row, ncells);
            count = count.saturating_add(1);
            self.push_jobs(
                g,
                &fc,
                Place {
                    col: here,
                    row,
                    ncells,
                },
                is_cursor.then_some(&p.cursor),
                off,
                Self::one_bit_lit(p, &fc, row, here),
            );
        }
        self.widen_to_span(p, row);
        #[cfg(feature = "images")]
        {
            self.compose.pictures.row = row;
        }
        back.row(&mut self.compose, &self.font, self.layout.strip());
        count
    }

    /// At 1 bit, whether `fc`, at `col` in `row`, draws its background lit
    /// (`mono::marked`): the selection inverts what the cell draws
    /// unselected. `None` at any other depth.
    fn one_bit_lit(p: &Pass<'_, '_>, fc: &FrameCell<'_>, row: usize, col: usize) -> Option<bool> {
        let plain = p.plain.as_ref()?;
        let natural =
            |c: &FrameCell<'_>| mono::marked(c.bg, c.fg, c.attrs.contains(Attrs::INVERSE));
        Some(if fc.selected {
            !plain
                .cell(row, col)
                .map_or_else(|| natural(fc), |c| natural(&c))
        } else {
            natural(fc)
        })
    }

    /// The jobs for one cell: its own, and for a bar cursor on a wide cell,
    /// its second half's. `off` draws it in the text blink's off phase.
    fn push_jobs(
        &mut self,
        g: &Grid,
        fc: &FrameCell<'_>,
        at: Place,
        cursor: Option<&Cursor>,
        off: bool,
        lit: Option<bool>,
    ) {
        let force = self.config.decorations.force;
        let (deco_ul, deco_strike) = self.deco;
        // the frame has dimmed the foreground; the cursor draws over it
        let (mut fg, mut bg) = (fc.fg, fc.bg);
        let placeholder = draws_no_text(fc.glyph);
        // A cell of the link under the pointer draws a dashed underline in
        // place of its own style, in its own underline color (ADR-1002,
        // "Drawing").
        let mut underline = if placeholder {
            vtium_term::UnderlineStyle::None
        } else if fc.hovered {
            vtium_term::UnderlineStyle::Dashed
        } else {
            fc.attrs.underline()
        };
        let mut ul_cursor = false;
        let mut bar = false;
        let mut cell_lit = lit;
        // at 1 bit a mark's text always shows: the selection's and a block
        // cursor's ink is never their background
        if lit.is_some() && fc.selected {
            fg = mono::ink_on(bg);
        }
        match cursor.map(|c| (c.shape, c.color)) {
            Some((CursorShape::Block, color)) => {
                core::mem::swap(&mut fg, &mut bg);
                if let Some(c) = color {
                    bg = c;
                }
                cell_lit = lit.map(|l| !l);
                if lit.is_some() {
                    fg = mono::ink_on(bg);
                }
            }
            Some((CursorShape::Underline, _)) => {
                underline = match underline {
                    vtium_term::UnderlineStyle::None => vtium_term::UnderlineStyle::Single,
                    _ => vtium_term::UnderlineStyle::None,
                };
                ul_cursor = true;
            }
            Some((CursorShape::Bar, _)) => bar = true,
            None => {}
        }
        // at 1 bit a line cursor is the cell's ink, whatever its color
        let cursor_color = match lit {
            Some(_) if !matches!(cursor.map(|c| c.shape), Some(CursorShape::Block)) => {
                cursor.map(|_| mono::ink_on(bg))
            }
            _ => cursor.and_then(|c| c.color),
        };
        let ul_color = if ul_cursor {
            cursor_color.unwrap_or(fg)
        } else {
            match (fc.underline_color, force, deco_ul) {
                (Some(c), false, _) => c,
                (_, _, Some(c)) => c,
                (_, _, None) => fg,
            }
        };
        let strike =
            (fc.attrs.contains(Attrs::STRIKE) && !placeholder).then_some(deco_strike.unwrap_or(fg));
        let mut underline = Kind::underline(underline).map(|k| (k, ul_color));
        let mut strike = strike;
        let blink = self.config.blink;
        let (hidden, dimmed) = match blink.style {
            BlinkStyle::Full => (off, false),
            BlinkStyle::Partial => (false, off),
            BlinkStyle::Off => (false, false),
        };
        if off && blink.decorations {
            if hidden {
                (underline, strike) = (None, None);
            } else if dimmed {
                underline = underline.map(|(k, c)| (k, dim(c, bg)));
                strike = strike.map(|c| dim(c, bg));
            }
        }
        if dimmed {
            fg = dim(fg, bg);
        }
        let style = Style {
            bold: fc.attrs.contains(Attrs::BOLD),
            italic: fc.attrs.contains(Attrs::ITALIC),
        };
        let text = &mut self.compose.text;
        let start = text.len();
        let width = if bar {
            text.push(BAR);
            FontWidth::Narrow
        } else {
            if !hidden && !placeholder {
                match fc.glyph {
                    Glyph::Empty => {}
                    Glyph::Char(c) => text.push(c),
                    Glyph::Cluster(s) => text.extend(s.chars()),
                }
            }
            if fc.width == Width::Wide {
                FontWidth::Wide
            } else {
                FontWidth::Narrow
            }
        };
        let len = text.len().saturating_sub(start);
        let text = (len > 0).then_some((start, len, width, style));
        // vtium's mark, which draws above every picture (ADR-1100 D4)
        #[cfg(feature = "images")]
        let under = |mark: Mark| Under {
            default_bg: fc.default_bg,
            selected: fc.selected,
            mark,
        };
        let mut at = at;
        if bar && at.ncells == 2 {
            // the bar takes the left half; the right is the cell without it
            let tail = Place {
                col: at.col.saturating_add(1),
                ncells: 1,
                ..at
            };
            let paint = Paint {
                fg: fc.fg,
                bg: fc.bg,
                underline,
                strike,
                text: None,
                lit,
            };
            let job = self.job(g, tail, 1, paint);
            #[cfg(feature = "images")]
            let job = Job {
                under: under(Mark::None),
                ..job
            };
            self.compose.jobs.push(job);
            at.ncells = 1;
        }
        let fg = if bar { cursor_color.unwrap_or(fg) } else { fg };
        let glyph_cells = if width == FontWidth::Wide { 2 } else { 1 };
        let paint = Paint {
            fg,
            bg,
            underline,
            strike,
            text,
            lit: cell_lit,
        };
        let job = self.job(g, at, glyph_cells, paint);
        #[cfg(feature = "images")]
        let job = Job {
            under: under(match cursor.map(|c| c.shape) {
                Some(CursorShape::Block) => Mark::Opaque,
                Some(CursorShape::Bar) => Mark::Bar,
                Some(CursorShape::Underline) => Mark::Underline,
                None => Mark::None,
            }),
            ..job
        };
        self.compose.jobs.push(job);
    }

    fn job(&self, g: &Grid, at: Place, glyph_cells: usize, paint: Paint) -> Job {
        let l = &self.layout;
        let depth = self.config.depth;
        let ink = match paint.lit {
            Some(lit) => Ink::with_state(depth, paint.bg, lit),
            None => Ink::new(depth, paint.bg),
        };
        Job {
            posx: at.col,
            ncells: at.ncells,
            r: l.cells_area(g, at.col, at.row, at.ncells),
            g: l.cells_area(g, at.col, at.row, glyph_cells),
            tiles: [
                l.cells_area(g, at.col, at.row, 1),
                l.cells_area(g, at.col.saturating_add(1), at.row, 1),
            ],
            fg: ink.fg(paint.fg),
            bg: ink.bg(),
            underline: paint.underline.map(|(k, c)| (k, ink.fg(c))),
            strike: paint.strike.map(|c| ink.fg(c)),
            text: paint.text,
            silhouette: depth == Depth::One,
            #[cfg(feature = "images")]
            under: Under::default(),
        }
    }

    /// When the row draws a cell of several layers, every cell between the
    /// first and the last of them is composed with them in the row scratch,
    /// so those not yet drawn join the jobs.
    fn widen_to_span(&mut self, p: &Pass<'_, '_>, row: usize) {
        let Some((first, end)) = self.compose.layered_span() else {
            return;
        };
        let g = &p.g;
        let mut before = first
            .checked_sub(1)
            .and_then(|c| p.frame.cell(row, c))
            .map(|fc| fc.width);
        let run = p.frame.cells(row).take(end.min(g.cols)).enumerate();
        for (here, fc) in run.skip(first) {
            let ncells = span(fc.width, before, || next_width(p.frame, row, here, g.cols));
            before = Some(fc.width);
            if ncells == 0 {
                continue;
            }
            let covered = self
                .compose
                .jobs
                .iter()
                .any(|j| here >= j.posx && here < j.posx.saturating_add(j.ncells));
            if covered {
                continue;
            }
            let is_cursor = p.at == Some((here, row));
            let off = blink_off(&fc, is_cursor, p.state, self.config.blink);
            self.cells.mark_drawn(here, row, ncells);
            self.push_jobs(
                g,
                &fc,
                Place {
                    col: here,
                    row,
                    ncells,
                },
                is_cursor.then_some(&p.cursor),
                off,
                Self::one_bit_lit(p, &fc, row, here),
            );
        }
        self.compose.jobs.sort_by_key(|j| j.posx);
    }

    /// The grid's pixel the pointer's tip at `x`, `y` names, kept inside
    /// the grid; for a glyph pointer, the corner of the cell it is in.
    fn pointer_pixel(&self, g: &Grid, (x, y): (u32, u32)) -> Option<(i64, i64)> {
        if g.cols == 0 || g.rows == 0 {
            return None;
        }
        let l = self.layout;
        let last = |n: usize, side: i64| int(n).saturating_mul(side).saturating_sub(1).max(0);
        let (px, py) = (
            int(x).min(last(g.cols, l.cw)),
            int(y).min(last(g.rows, l.ch)),
        );
        let corner = |p: i64, side: i64| p.saturating_sub(p.checked_rem(side).unwrap_or(0));
        Some(match self.config.pointer {
            PointerShape::Arrow => (px, py),
            PointerShape::Glyph(_) => (corner(px, l.cw), corner(py, l.ch)),
        })
    }

    /// Where the arrow draws for the pointer's tip at `x`, `y` in the
    /// grid's pixels, and the cells it covers. Its hotspot lands on the
    /// grid's pixel, kept inside the grid; the rest may reach into the
    /// border and past the display, where nothing is drawn.
    fn place_pointer(&self, g: &Grid, at: (u32, u32)) -> Option<Pointer> {
        let (px, py) = self.pointer_pixel(g, at)?;
        let l = self.layout;
        let tip = l.area(g, px, py, 1, 1);
        let (hx, hy) = self.arrow.hot();
        let at = Area {
            x: tip.x.saturating_sub(int(hx)),
            y: tip.y.saturating_sub(int(hy)),
            w: int(self.arrow.width()),
            h: int(self.arrow.height()),
        };
        let (gw, gh) = (
            int(g.cols).saturating_mul(l.cw),
            int(g.rows).saturating_mul(l.ch),
        );
        let b = l.grid_area(g, at);
        let first = |v: i64, side: i64, n: usize| {
            usize::try_from(v.max(0).checked_div(side).unwrap_or(0))
                .unwrap_or(0)
                .min(n)
        };
        let past = |v: i64, side: i64, n: usize| {
            let end = v.saturating_add(side).saturating_sub(1).max(0);
            usize::try_from(end.checked_div(side).unwrap_or(0))
                .unwrap_or(0)
                .min(n)
        };
        let cells = (
            first(b.x, l.cw, g.cols),
            first(b.y, l.ch, g.rows),
            past(b.x.saturating_add(b.w), l.cw, g.cols),
            past(b.bottom(), l.ch, g.rows),
        );
        let outside = b.x < 0 || b.y < 0 || b.x.saturating_add(b.w) > gw || b.bottom() > gh;
        Some(Pointer { at, cells, outside })
    }

    /// The cells `area` touches changed on this frame, and the border
    /// inside it.
    fn stamp_area(&mut self, g: &Grid, area: Rect) {
        let meets = |r: Rect| {
            r.x < area.x.saturating_add(area.width)
                && area.x < r.x.saturating_add(r.width)
                && r.y < area.y.saturating_add(area.height)
                && area.y < r.y.saturating_add(r.height)
        };
        for row in 0..g.rows {
            for col in 0..g.cols {
                if meets(self.layout.cell_rect(g, col, row)) {
                    self.cells.stamp(col, row);
                }
            }
        }
        self.cells.stamp_border_area(area);
    }

    /// The cells under `p` changed on this frame, and the border inside
    /// its area, clipped to the display, when it reaches into it.
    fn stamp_pointer(&mut self, p: &Pointer) {
        let (c0, r0, c1, r1) = p.cells;
        for r in r0..r1 {
            for c in c0..c1 {
                self.cells.stamp(c, r);
            }
        }
        if p.outside {
            let (w, h) = (int(self.layout.width), int(self.layout.height));
            let (x0, y0) = (p.at.x.clamp(0, w), p.at.y.clamp(0, h));
            let x1 = p.at.x.saturating_add(p.at.w).clamp(0, w);
            let side = |v: i64| u32::try_from(v).unwrap_or(0);
            self.cells.stamp_border_area(Rect {
                x: side(x0),
                y: side(y0),
                width: side(x1.saturating_sub(x0)),
                height: side(p.at.bottom().clamp(0, h).saturating_sub(y0)),
            });
        }
    }
}

/// Whether a cell holding `glyph` draws no text and no decoration of its own:
/// a kitty Unicode placeholder, a cluster whose first scalar value is
/// U+10EEEE, marks where a virtual placement's picture draws (ADR-1100 D13).
/// Its background, the cursor and the selection draw as on any cell. In a
/// build without `images`, U+10EEEE is a private-use character like any
/// other.
fn draws_no_text(glyph: Glyph<'_>) -> bool {
    const PLACEHOLDER: char = '\u{10EEEE}';
    cfg!(feature = "images")
        && match glyph {
            Glyph::Empty => false,
            Glyph::Char(c) => c == PLACEHOLDER,
            Glyph::Cluster(s) => s.starts_with(PLACEHOLDER),
        }
}
