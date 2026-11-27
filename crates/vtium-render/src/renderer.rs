//! [`Renderer`]: draws frames into a [`Target`], redrawing only what
//! changed for the buffer being drawn.

use std::time::Duration;

use vtium_font::{Decorations, Face, Font, Style, Width as FontWidth};
use vtium_term::{LineSize, Palette, Rgb};

use std::sync::Arc;

use crate::arrow::Arrow;
use crate::bounds::MISSING_CHARS;
use crate::cache::{Budgets, GlyphCache, Profile, Storage};
use crate::compose::{Composer, int};
use crate::damage::Cells;
use crate::deco::{SCRIPTS, Sprites};
use crate::mono::{self, Depth, Fallback};
#[cfg(feature = "images")]
use crate::picture::PictureCounts;
use crate::pointer_file::PointerPictures;
use crate::reach::Bound;
#[cfg(feature = "bidi")]
use crate::syllable::Runs;
#[cfg(feature = "filters")]
use crate::target::Target;
use crate::target::{Orientation, Rect};

pub(crate) mod double;
#[cfg(feature = "gpu")]
mod gpu;
// the ring lives beside the renderer, whose record of drawn cells it moves
#[cfg(feature = "scroll-pan")]
#[path = "ring.rs"]
pub(crate) mod ring;
pub(crate) mod sized;
#[cfg(feature = "scroll-pan")]
#[path = "strip.rs"]
mod strip;
mod testing;
#[cfg(test)]
mod wider;

pub use testing::TestInk;

/// What the change reads of a cell take: the row's pass, which counts the
/// rows of history above the screen, in a build with `scroll-pan`, and the
/// frame in a build without it.
#[cfg(feature = "scroll-pan")]
macro_rules! changed_arg {
    ($p:expr) => {
        $p
    };
}

/// What the change reads of a cell take: the frame.
#[cfg(not(feature = "scroll-pan"))]
macro_rules! changed_arg {
    ($p:expr) => {
        $p.frame
    };
}

/// The cell of the frame without its selection that the ink of a glyph is
/// drawn as, through the pass's rows in a build with `scroll-pan`.
#[cfg(feature = "scroll-pan")]
macro_rules! plain_cell {
    ($p:expr, $r:expr, $k:expr) => {
        $p.plain_cell($r, $k)
    };
}

/// The cell of the frame without its selection.
#[cfg(not(feature = "scroll-pan"))]
macro_rules! plain_cell {
    ($p:expr, $r:expr, $k:expr) => {
        $p.plain().cell($r, $k)
    };
}

mod changes;
mod cpu;
mod frame;
mod jobs;
mod layout;
mod pass;
mod pointer;
mod rows;

#[cfg(feature = "gpu")]
pub(crate) use cpu::Back;
pub use layout::grid_origin;
use layout::{Grid, Layout};
#[cfg(doc)]
use pass::Pass;
use pointer::Pointer;

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
    /// `font-ligatures`: whether a run of ASCII symbols draws as the font's
    /// ligature across its cells, which the renderer finds in a build with
    /// `bidi`.
    pub ligatures: bool,
    /// Where the grid sits on the display, and what its spare pixel lines
    /// show at rest.
    pub placement: Placement,
}

/// Where the grid sits on the display, which the console's configured
/// `smooth-scroll` chooses: centred on both axes while smooth scrolling is
/// off, the default, or with its last row against the text's bottom edge
/// while smooth scrolling is on.
///
/// References:
/// - ADR-1310 D10: the grid flush to the text's bottom edge while smooth scrolling is on.
/// - ADR-1310 D12, "With smooth scrolling off": the grid centred on both axes, with blank bars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    /// The grid is centred on both axes in the display's own pixels. Of
    /// the spare lines, the top gets half, rounded down, and the bottom the
    /// rest, and the spare columns split the same way between the left and
    /// the right. Every spare line and column shows the border's color,
    /// and the screen's top row takes no ink from the history row above.
    #[default]
    Centred,
    /// The grid's last row touches the text's bottom edge, so the spare
    /// lines lie above its first row and show what `PartialRow` says.
    /// Across the rows the grid stays centred.
    Bottom(PartialRow),
}

/// What the spare lines above a grid placed at the bottom show at rest, as
/// vtium.conf's `partial-row` asks. In a build without `scroll-pan` the
/// renderer keeps no row of history above the screen, so every value shows
/// the border's color there.
///
/// References:
/// - ADR-1310 D12, "The key `partial-row`": the three values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PartialRow {
    /// The bottom of the row of history above the grid, drawn partly off
    /// the screen.
    #[default]
    History,
    /// The border's color.
    Blank,
    /// The row above, mixed in linear light more toward the border's color
    /// the farther each line lies from the grid's first row, so the line at
    /// the screen's edge is the border's color.
    Fade,
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
            ligatures: false,
            placement: Placement::default(),
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
    /// The rapid blink, SGR 6's, is in its off phase, which draws rapidly
    /// blinking text as [`Config::blink`] says. It has a timer of its own.
    pub rapid_hidden: bool,
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

/// Draws terminal frames with a font into a display buffer.
#[derive(Debug)]
pub struct Renderer<F> {
    font: Font<F>,
    config: Config,
    layout: Layout,
    grid: Option<Grid>,
    cells: Cells,
    background: Option<Rgb>,
    /// The off phases of the text blink and the rapid blink last frame.
    blink_hidden: (bool, bool),
    cursor_at: Option<(usize, usize)>,
    /// Where the visible cursor stood last frame, in either blink phase.
    held_at: Option<(usize, usize)>,
    /// Whether every cell's reach is current: false once a row is scanned
    /// without reading reaches, as the GPU path scans and as every path
    /// scans while the font has drawn no glyph whose ink passes its cells.
    reaches_kept: bool,
    /// Whether the row being drawn holds any cell of a sized run, decided
    /// once for the row, so a row with no run spends nothing on runs.
    row_runs: bool,
    /// How far a glyph's ink may reach past its cells.
    bound: Bound,
    /// A cell's text, read for its glyph's reach.
    chars: Vec<char>,
    /// The text of the cell being pushed, as it draws (ADR-1004 D6).
    #[cfg(feature = "bidi")]
    drawn: Vec<char>,
    /// The syllables and ligature runs of the row being drawn, which each
    /// frame's [`Pass`] borrows, kept here so that their memory is reused.
    #[cfg(feature = "bidi")]
    runs: Runs,
    /// The blocks of cells the reach of the row being scanned stamped.
    reached: Vec<crate::reach::Block>,
    /// The size each row of the grid last drew at (DECDWL, DECDHL and
    /// DECSWL). A row whose size changes draws again whole.
    sizes: Vec<LineSize>,
    /// The cells of the double-size row being drawn that draw, each with
    /// the cells it draws over.
    doubled: Vec<(usize, usize)>,
    arrow: Arrow,
    /// The pointer's image upright, which the GPU path draws.
    upright_arrow: Arrow,
    /// The shape a program asked the pointer to take with OSC 22, which
    /// the pointer's image shows ([`Renderer::set_pointer_shape`]).
    shape: vtium_term::PointerShape,
    /// A pointer file's pictures, which come before vtium's own
    /// ([`Renderer::set_pointer_pictures`]).
    pictures: Option<Arc<PointerPictures>>,
    pointer: Option<Pointer>,
    /// The line a run under the pointer is blended in.
    pointer_line: Vec<u32>,
    /// The line a run is thresholded in, on a 1-bit display.
    mono_line: Vec<u32>,
    /// The lookup in the head's color table, `None` with no color filter
    /// ([`Renderer::set_color_table`]).
    #[cfg(feature = "filters")]
    color: Option<crate::filter::Lookup>,
    /// The line a run is looked up in, on its way through the color slot.
    #[cfg(feature = "filters")]
    color_line: Vec<u32>,
    /// The configured underline and strike colors through the frame being
    /// drawn's palette; `None` follows the foreground.
    deco: (Option<Rgb>, Option<Rgb>),
    compose: Composer,
    /// The font's lines in each of [`SCRIPTS`] that the decoration sprites
    /// were made from (ADR-1202 D8).
    lines: [Decorations; 3],
    /// Characters the font draws differently since the last frame, with
    /// the characters of each text drawn as the font's missing glyph before
    /// an arrival since then, sorted: the cells showing one draw again
    /// ([`Renderer::glyphs_changed`]).
    changed: Vec<char>,
    /// The characters of each text a frame drew as the font's missing
    /// glyph ([`vtium_font::Drawn::missing`]), or drew nothing for, since
    /// the last arrival, sorted. The next arrival moves them to `changed`,
    /// since the set that arrived may hold U+25A1 or `?`, so each cell that
    /// shows the missing glyph draws again. A cell scrolled away since keeps
    /// its characters here until that arrival.
    shown_missing: Vec<char>,
    /// Whether the frames since the last arrival drew more characters as
    /// the missing glyph than `shown_missing` keeps ([`MISSING_CHARS`]), so
    /// the next arrival draws every cell again.
    shown_missing_overflowed: bool,
    /// Whether the next frame draws again cells that showed the font's
    /// missing glyph before an arrival ([`Renderer::redraws_missing`]).
    owes_missing: bool,
    /// Display areas something other than the renderer drew over since the
    /// last frame ([`Renderer::redraw_area`]).
    touched: Vec<Rect>,
    /// What the GPU path keeps of this renderer's.
    #[cfg(feature = "gpu")]
    gpu: gpu::State,
    /// The ring of drawn rows that the pan and the copy show a scroll
    /// with.
    #[cfg(feature = "scroll-pan")]
    ring: ring::Ring,
    /// The scratch a run written into the spare lines is covered in,
    /// under `partial-row = blank` or `fade`.
    #[cfg(feature = "scroll-pan")]
    strip_line: Vec<u32>,
}

/// The pointer's image for cells `cell_height` tall, upright. A shape a
/// program asked for with OSC 22 draws as its own picture. The default
/// shape draws as the arrow, or as the glyph `config.pointer` names. The
/// arrow stands in for a character the font lacks, which would draw as a
/// fallback (the missing box, U+FFFD or `?`), and for a blank one, whose
/// glyph covers no pixel.
///
/// With a pointer file's pictures, the file's picture of the shape comes
/// before vtium's own. `pointer-char` names one shape and the file names
/// all ten, so the narrower key wins where they overlap: the glyph draws
/// the default shape, and the file's picture draws it only when the font
/// cannot draw the character. A shape the file lacks, and a height an SVG
/// file was not drawn at yet, draw vtium's own picture.
///
/// References:
/// - ADR-1309 D1: `pointer-char` keeps the default shape, then the file's picture, then vtium's arrow.
fn pointer_image<F: Face>(
    font: &Font<F>,
    cell_height: u32,
    config: Config,
    shape: vtium_term::PointerShape,
    pictures: Option<&PointerPictures>,
) -> Arrow {
    let glyph = match config.pointer {
        PointerShape::Glyph(c)
            if shape == vtium_term::PointerShape::Default && font.has_glyph(c) =>
        {
            font.glyph(&[c], FontWidth::Narrow, Style::default())
        }
        _ => None,
    }
    .map(|g| Arrow::upright_glyph(&g))
    .filter(Arrow::covers);
    glyph
        .or_else(|| pictures.and_then(|p| p.pick(shape, cell_height)).cloned())
        .unwrap_or_else(|| Arrow::upright_shape(shape, cell_height))
}

/// The font's lines in each of [`SCRIPTS`] in turn: the normal lines, and
/// the lines of a superscript and of a subscript cell (ADR-1202 D8).
fn line_sets<F: Face>(font: &Font<F>) -> [Decorations; 3] {
    SCRIPTS.map(|script| font.script_decorations(script))
}

/// The decoration sprites for cells `cw` by `ch` in the font's `lines`,
/// one set for each of [`SCRIPTS`], turned as `orientation` says, stored
/// as `config` says.
fn sprites(
    cw: u32,
    ch: u32,
    lines: &[Decorations; 3],
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

/// `chars` sorted, each once.
fn sorted(chars: &[char]) -> Vec<char> {
    let mut out = chars.to_vec();
    out.sort_unstable();
    out.dedup();
    out
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
        let lines = line_sets(&font);
        let shape = vtium_term::PointerShape::Default;
        let upright_arrow = pointer_image(&font, ch, config, shape, None);
        let arrow = upright_arrow.clone().turned(config.orientation);
        #[cfg(feature = "gpu")]
        let gpu = gpu::State::new(
            budgets,
            sprites(cw, ch, &lines, Orientation::Normal, config),
        );
        Renderer {
            compose: Composer::new(
                GlyphCache::new(budgets, config.storage, config.orientation)
                    .with_fallback(Fallback::new(config.depth, font.cell())),
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
                placement: config.placement,
            },
            grid: None,
            cells: Cells::default(),
            background: None,
            blink_hidden: (false, false),
            cursor_at: None,
            held_at: None,
            reaches_kept: true,
            row_runs: false,
            bound: Bound::DRAWN,
            chars: Vec::new(),
            #[cfg(feature = "bidi")]
            drawn: Vec::new(),
            #[cfg(feature = "bidi")]
            runs: Runs::default(),
            reached: Vec::new(),
            sizes: Vec::new(),
            doubled: Vec::new(),
            arrow,
            upright_arrow,
            shape,
            pictures: None,
            pointer: None,
            pointer_line: Vec::new(),
            mono_line: Vec::new(),
            #[cfg(feature = "filters")]
            color: None,
            #[cfg(feature = "filters")]
            color_line: Vec::new(),
            deco: (None, None),
            changed: Vec::new(),
            shown_missing: Vec::new(),
            shown_missing_overflowed: false,
            owes_missing: false,
            touched: Vec::new(),
            #[cfg(feature = "gpu")]
            gpu,
            #[cfg(feature = "scroll-pan")]
            ring: ring::Ring::default(),
            #[cfg(feature = "scroll-pan")]
            strip_line: Vec::new(),
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
        // the look's colors do not turn with the display
        #[cfg(feature = "filters")]
        let turned = Renderer {
            color: self.color,
            ..turned
        };
        turned
    }

    /// The font now draws each of `chars` differently, as when the deferred
    /// pipeline's generated glyphs arrive (ADR-1301 D2): the cache drops
    /// the glyphs whose text holds one of `chars`, alone or in a cluster,
    /// and the next frame draws again each cell that shows one (ADR-1200
    /// D23). The cache drops too each glyph the font drew as its missing
    /// glyph, and the next frame draws again each cell that shows one, so a
    /// set that brings U+25A1 replaces the drawing of every character no
    /// font holds ([`Renderer::redraws_missing`]). Every other glyph stays
    /// cached, and every other cell keeps its pixels.
    ///
    /// When the font's underline and strike-through lines changed too, as
    /// an outline font's do once the glyph helper has measured them, the
    /// renderer takes the new lines (`Renderer::lines_changed`).
    pub fn glyphs_changed(&mut self, chars: &[char]) {
        self.lines_changed();
        let arrived = sorted(chars);
        self.compose.cache.forget(&arrived);
        #[cfg(feature = "gpu")]
        self.gpu.forget(&arrived);
        self.changed.extend_from_slice(&arrived);
        self.missing_arrival(!arrived.is_empty());
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

    /// Sets the head's color table, the look's color filters merged, or
    /// `None` for a look with no color filter. Every pixel the CPU path
    /// writes afterwards, the pointer's and those that
    /// [`Renderer::colored`] writes included, is looked up in it, and on
    /// the GPU path present's color step samples it. A change of table
    /// changes every pixel, so the next frame draws whole, as does the
    /// first frame into each other buffer. A turned renderer keeps the
    /// table. The renderer shares `table`'s nodes with every other holder
    /// of the table, so a head's displays hold one copy between them, and
    /// the GPU path reads the same nodes. The only failure is a want of
    /// memory for the nodes as 32-bit floats, which the first lookup in the
    /// table makes; it leaves the table as it was.
    ///
    /// References:
    /// - ADR-1304 D2: a change of the look's color filters makes the CPU path draw its frame whole, and the table and its 32-bit values are held once for each head.
    #[cfg(feature = "filters")]
    pub fn set_color_table(
        &mut self,
        table: Option<&crate::filter::Table>,
    ) -> Result<(), std::collections::TryReserveError> {
        let lookup = table.map(crate::filter::Lookup::new).transpose()?;
        #[cfg(feature = "gpu")]
        self.gpu.set_table(table.cloned());
        self.color = lookup;
        self.forget_history();
        Ok(())
    }

    /// Whether the head's look holds a color filter, so that every pixel
    /// passes through the color slot.
    #[cfg(feature = "filters")]
    pub fn colors(&self) -> bool {
        self.color.is_some()
    }

    /// `target` with the color slot in front of it: vtium's notices and the
    /// focus mark, which reach the display's buffer beside the renderer's
    /// frame, write through it so that the look's color filters reach them
    /// too. With no color table it writes through unchanged.
    #[cfg(feature = "filters")]
    pub fn colored<'a, T: Target + ?Sized>(&'a mut self, target: &'a mut T) -> impl Target + 'a {
        crate::filter::Colored::new(target, self.color.as_mut(), &mut self.color_line)
    }

    /// [`Renderer::colored`] beside the renderer's font and depth, which
    /// vtium's notices are drawn with while they write through the color
    /// slot.
    #[cfg(feature = "filters")]
    pub fn colored_with_font<'a, T: Target + ?Sized>(
        &'a mut self,
        target: &'a mut T,
    ) -> (&'a Font<F>, Depth, impl Target + 'a) {
        let colored =
            crate::filter::Colored::new(target, self.color.as_mut(), &mut self.color_line);
        (&self.font, self.config.depth, colored)
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
    /// (ADR-1200 D7), and no cell draws again now. The glyphs the font
    /// drew as its missing glyph go too, as [`Renderer::glyphs_changed`]
    /// drops them, and the next frame draws again each cell that shows one.
    /// Every other glyph stays cached.
    ///
    /// When the font's lines changed too, the renderer takes them, as
    /// [`Renderer::glyphs_changed`] does.
    pub fn forget_glyphs(&mut self, chars: &[char]) {
        self.lines_changed();
        self.compose.cache.forget(&sorted(chars));
        #[cfg(feature = "gpu")]
        self.gpu.forget(chars);
        self.missing_arrival(!chars.is_empty());
    }

    /// Finishes an arrival through [`Renderer::glyphs_changed`] or
    /// [`Renderer::forget_glyphs`]. When the font `gained` glyphs, the
    /// characters of each text drawn as the missing glyph since the last
    /// arrival join `changed`, so the next frame draws those cells again.
    /// When more of them drew than the renderer keeps, the next frame draws
    /// every cell again. Then `changed` is sorted.
    fn missing_arrival(&mut self, gained: bool) {
        if gained {
            if self.shown_missing_overflowed {
                self.grid = None;
            }
            self.owes_missing |= self.shown_missing_overflowed || !self.shown_missing.is_empty();
            self.shown_missing_overflowed = false;
            self.changed.append(&mut self.shown_missing);
        }
        self.changed.sort_unstable();
        self.changed.dedup();
    }

    /// Keeps `drawn`, the characters of each text a frame drew as the
    /// missing glyph, for the next arrival ([`Renderer::missing_arrival`]),
    /// or keeps none once more drew than [`MISSING_CHARS`].
    fn shown_missing_drew(&mut self, mut drawn: Vec<char>) {
        if self.shown_missing_overflowed || drawn.is_empty() {
            return;
        }
        self.shown_missing.append(&mut drawn);
        self.shown_missing.sort_unstable();
        self.shown_missing.dedup();
        if self.shown_missing.len() > MISSING_CHARS {
            self.shown_missing = Vec::new();
            self.shown_missing_overflowed = true;
        }
    }

    /// Whether the next frame draws again cells that showed the font's
    /// missing glyph, after an arrival through [`Renderer::glyphs_changed`]
    /// or [`Renderer::forget_glyphs`]. The console asks for that frame at
    /// once, since until it comes those cells show the drawing from before
    /// the arrival.
    pub fn redraws_missing(&self) -> bool {
        self.owes_missing
    }

    /// Takes the font's lines, in each script, when they are no longer the
    /// ones the renderer made its decoration sprites from:
    /// the sprites are made again, and the next frame draws every cell and
    /// the border. The glyph cache keeps its glyphs, because the font draws
    /// its box-drawing sprites in a weight that follows the cell, not the
    /// lines ([`vtium_font::sprite::light_line`]). On the GPU path the
    /// renderer takes a new identity, so the pipeline makes its glyphs and
    /// sprites anew and composes whole. Unchanged lines change nothing.
    fn lines_changed(&mut self) {
        let lines = line_sets(&self.font);
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

    /// Shows `shape`, which a program asked for with OSC 22, as the
    /// pointer's image from the next frame on. That frame draws again the
    /// cells the old image covered and the cells the new one covers,
    /// because the pointer's placement names its shape.
    pub fn set_pointer_shape(&mut self, shape: vtium_term::PointerShape) {
        if shape == self.shape {
            return;
        }
        self.shape = shape;
        self.redraw_pointer_image();
    }

    /// Takes `pictures`, a pointer file's, in place of those it held, and
    /// shows the pointer with them from the next frame on. `None` goes back
    /// to vtium's own pictures.
    pub fn set_pointer_pictures(&mut self, pictures: Option<Arc<PointerPictures>>) {
        self.pictures = pictures;
        self.redraw_pointer_image();
    }

    /// The pointer file's pictures this renderer draws with, if any.
    pub fn pointer_pictures(&self) -> Option<&Arc<PointerPictures>> {
        self.pictures.as_ref()
    }

    /// The cell height the pointer's pictures are picked at: the height an
    /// SVG pointer file is drawn for.
    pub fn pointer_cell_height(&self) -> u32 {
        u32::try_from(self.layout.ch).unwrap_or(0)
    }

    /// The pointer's image again, for the shape and the pictures held.
    fn redraw_pointer_image(&mut self) {
        let ch = self.pointer_cell_height();
        self.upright_arrow = pointer_image(
            &self.font,
            ch,
            self.config,
            self.shape,
            self.pictures.as_deref(),
        );
        self.arrow = self.upright_arrow.clone().turned(self.config.orientation);
    }

    /// The shape the pointer's image shows: the default shape until
    /// [`Renderer::set_pointer_shape`] names another.
    pub fn pointer_shape(&self) -> vtium_term::PointerShape {
        self.shape
    }

    /// Where on the display the arrow's hotspot goes for the pointer's tip
    /// at `x`, `y` in the grid's pixels: the grid's pixel kept inside the
    /// grid, turned and placed as the last frame placed the grid. `None`
    /// before the first frame, or with no grid.
    pub fn pointer_at(&self, (x, y): (u32, u32)) -> Option<(i32, i32)> {
        let g = self.grid?;
        #[cfg(feature = "scroll-pan")]
        let g = self.layout.screen_of(&g);
        let (px, py) = self.pointer_pixel(&g, (x, y))?;
        let a = self.layout.area(&g, px, py, 1, 1);
        Some((
            i32::try_from(a.x).unwrap_or(i32::MAX),
            i32::try_from(a.y).unwrap_or(i32::MAX),
        ))
    }

    /// The display rectangle of the cell at `col`, `row` in a grid of
    /// `grid` columns and rows, placed and turned where a frame of that
    /// size draws it. vtium draws its focus mark over the grid's top-right
    /// cell through it. `None` for a cell outside the grid, whose size the
    /// display's columns and rows bound.
    pub fn cell_rect(&self, grid: (usize, usize), col: usize, row: usize) -> Option<Rect> {
        let g = self.layout.grid_of(grid.0, grid.1);
        (col < g.cols && row < g.rows).then(|| self.layout.cell_rect(&g, col, row))
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
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::expect_used,
    reason = "a test fails by panicking, on small numbers it picks"
)]
mod tests;
