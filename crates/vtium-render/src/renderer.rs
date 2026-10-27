//! [`Renderer`]: draws frames into a [`Target`], redrawing only what
//! changed for the buffer being drawn.

use std::time::{Duration, Instant};

use vtium_font::{Face, Font, Style, Width as FontWidth};
use vtium_term::{Attrs, Cursor, CursorShape, Frame, FrameCell, Glyph, Rgb, Width};

use crate::blend::pixel;
use crate::bounds::ROW_SCRATCH_MAX;
use crate::cache::{Budgets, GlyphCache, Profile, Storage};
use crate::compose::{Area, Composer, Job, int};
use crate::damage::Cells;
use crate::deco::{Kind, Sprites};
use crate::target::{Orientation, Rect, Target};

/// The bar cursor: U+258F, LEFT ONE EIGHTH BLOCK, a sprite.
const BAR: char = '\u{258f}';

/// The pointer's glyph.
const POINTER: char = 'I';

/// The colour a decoration draws in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecoColor {
    /// The cell's foreground as drawn.
    #[default]
    Foreground,
    Rgb(Rgb),
}

/// The configured decoration colours: `underline-color`, `strike-color` and
/// `force-decoration-colors`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecorationColors {
    pub underline: DecoColor,
    pub strike: DecoColor,
    /// The configured underline colour wins over the one SGR 58 set.
    pub force: bool,
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
        }
    }
}

/// What a frame needs beyond the terminal's state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawState {
    /// The cursor's blink phase shows it.
    pub cursor_on: bool,
    /// The text blink phase hides blinking text.
    pub blink_hidden: bool,
    /// The palette's default foreground, which the pointer draws in.
    pub foreground: Rgb,
    /// The palette's default background, around the grid and behind the
    /// pointer.
    pub background: Rgb,
    /// The pointer's centre in the grid's pixels, unturned, for the
    /// renderer to draw: set only on a display without a cursor plane
    /// (fbdev, legacy drm2d, a plane that refused the cursor), which the
    /// display tells its owner. `None` when the pointer is hidden or the
    /// display's cursor plane shows it.
    pub pointer: Option<(u32, u32)>,
}

/// One frame's figures, for the render timer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Cells drawn.
    pub cells: u32,
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
}

/// A cell's place: its column, row and the cells it draws over.
#[derive(Clone, Copy, Debug)]
struct Place {
    col: usize,
    row: usize,
    ncells: usize,
}

/// What a job paints: its colours, decorations and text.
#[derive(Clone, Copy, Debug)]
struct Paint {
    fg: Rgb,
    bg: Rgb,
    underline: Option<(Kind, Rgb)>,
    strike: Option<Rgb>,
    text: Option<(usize, usize, FontWidth, Style)>,
}

/// Where the pointer draws, the top left of the two by two cells under it,
/// and its colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pointer {
    at: Area,
    col: usize,
    row: usize,
    colors: (Rgb, Rgb),
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
    pointer: Option<Pointer>,
    compose: Composer,
}

/// How many cells the cell at `col`, `width` wide, draws over: 0 for the
/// trailing half of a wide character, which the leading half draws; 2 for
/// a leading half with its trailing half on the screen; else 1. A half
/// whose partner is missing draws as one cell, so no frame leaves a cell
/// undrawn.
fn span(frame: &Frame<'_>, row: usize, col: usize, width: Width, cols: usize) -> usize {
    let width_at = |c: usize| frame.cell(row, c).map(|f| f.width);
    let next = col.saturating_add(1);
    match width {
        Width::WideTail if col.checked_sub(1).and_then(width_at) == Some(Width::Wide) => 0,
        Width::Wide if next < cols && width_at(next) == Some(Width::WideTail) => 2,
        _ => 1,
    }
}

/// Whether `fc` is hidden by the text blink phase: blinking, not under
/// the cursor, in the hidden phase.
fn blink_hides(fc: &FrameCell<'_>, is_cursor: bool, state: &DrawState) -> bool {
    let blinks = fc.attrs.contains(Attrs::BLINK) || fc.attrs.contains(Attrs::RAPID_BLINK);
    !is_cursor && blinks && state.blink_hidden
}

impl<F: Face> Renderer<F> {
    /// A renderer drawing with `font` on a display `width` by `height`
    /// pixels.
    pub fn new(font: Font<F>, width: u32, height: u32, config: Config) -> Renderer<F> {
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
        let sprites = Sprites::new(
            cw,
            ch,
            &font.decorations(),
            config.orientation,
            config.storage.bits,
        );
        Renderer {
            compose: Composer::new(
                GlyphCache::new(budgets, config.storage, config.orientation),
                sprites,
            ),
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
            pointer: None,
        }
    }

    pub fn font(&self) -> &Font<F> {
        &self.font
    }

    /// The columns and rows the display holds.
    pub fn grid_size(&self) -> (usize, usize) {
        (self.layout.max_cols, self.layout.max_rows)
    }

    /// Bytes of glyph pixels the cache holds.
    pub fn cache_bytes(&self) -> u64 {
        self.compose.cache.pixel_bytes()
    }

    /// Bytes the row scratch holds once a row of several layers is drawn:
    /// one row of cells across the display, at most `ROW_SCRATCH_MAX`.
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
        self.compose.cache.begin_frame();
        self.cells.begin(age);
        let grid = self.layout.grid_for(frame);
        if self.grid != Some(grid) || self.background != Some(state.background) {
            self.cells.reset(grid.cols, grid.rows);
            self.grid = Some(grid);
            self.background = Some(state.background);
            self.cursor_at = None;
            self.pointer = None;
        }
        let full = self.cells.border_stale();
        if full {
            self.fill_border(&grid, target, state.background);
        }
        let pointer = state
            .pointer
            .and_then(|p| self.place_pointer(&grid, state, p));
        if pointer != self.pointer {
            // it appeared, moved, hid or changed colour: the cells it left
            // and the cells it covers change
            for p in [self.pointer, pointer].into_iter().flatten() {
                self.stamp_pointer(&p);
            }
            self.pointer = pointer;
        }
        let cells = self.draw_cells(&grid, frame, target, state);
        if let Some(p) = pointer.filter(|p| self.pointer_cells_drawn(p)) {
            self.draw_pointer(&p, target);
        }
        let layout = self.layout;
        let whole = full.then_some(Rect {
            x: 0,
            y: 0,
            width: layout.width,
            height: layout.height,
        });
        let counts = self.compose.cache.take_counts();
        let damage = self
            .cells
            .collect(whole, |col, row| layout.cell_rect(&grid, col, row));
        let damage_px = damage.iter().fold(0u64, |n, r| n.saturating_add(r.area()));
        Drawn {
            damage,
            stats: Stats {
                cells,
                misses: counts.misses,
                uncached: counts.uncached,
                damage_px,
                cpu: start.elapsed(),
            },
        }
    }

    fn fill_border(&mut self, g: &Grid, target: &mut impl Target, bg: Rgb) {
        let height = self.layout.height;
        let width = usize::try_from(self.layout.width).unwrap_or(0);
        let line = self.compose.solid(width, pixel(bg));
        let (x0, x1) = (
            u32::try_from(g.off_x).unwrap_or(0),
            u32::try_from(g.max_x).unwrap_or(0),
        );
        let left = line.get(..usize::try_from(x0).unwrap_or(0)).unwrap_or(&[]);
        let right = line.get(usize::try_from(x1).unwrap_or(0)..).unwrap_or(&[]);
        for y in 0..height {
            let yi = int(y);
            if yi < g.off_y || yi >= g.max_y || g.cols == 0 || g.rows == 0 {
                target.write(0, y, line);
            } else {
                if !left.is_empty() {
                    target.write(0, y, left);
                }
                if !right.is_empty() {
                    target.write(x1, y, right);
                }
            }
        }
    }

    /// Where the cursor draws: the leading cell of a wide character whose
    /// trailing half it sits on.
    fn cursor_cell(frame: &Frame<'_>, c: &Cursor, cols: usize) -> (usize, usize) {
        let tail = frame
            .cell(c.row, c.col)
            .is_some_and(|fc| span(frame, c.row, c.col, fc.width, cols) == 0);
        match c.col.checked_sub(1) {
            Some(prev) if tail => (prev, c.row),
            _ => (c.col, c.row),
        }
    }

    fn draw_cells(
        &mut self,
        g: &Grid,
        frame: &Frame<'_>,
        target: &mut impl Target,
        state: &DrawState,
    ) -> u32 {
        let cursor = frame.cursor();
        let at =
            (cursor.visible && state.cursor_on).then(|| Self::cursor_cell(frame, &cursor, g.cols));
        let blink_toggled = state.blink_hidden != self.blink_hidden;
        self.blink_hidden = state.blink_hidden;
        let last_cursor = core::mem::replace(&mut self.cursor_at, at);
        let pass = Pass {
            g: *g,
            frame,
            state,
            cursor,
            at,
        };
        let mut drawn = 0u32;
        for row in 0..g.rows {
            let scan = self.cells.row_stale(row)
                || blink_toggled
                || frame.row_changed(row)
                || at.is_some_and(|(_, r)| r == row)
                || last_cursor.is_some_and(|(_, r)| r == row);
            if scan {
                drawn = drawn.saturating_add(self.draw_row(&pass, row, target));
            }
        }
        drawn
    }

    /// Scans one row, updates what each cell last drew, and draws the cells
    /// some buffer lacks; returns how many.
    fn draw_row(&mut self, p: &Pass<'_, '_>, row: usize, target: &mut impl Target) -> u32 {
        let g = &p.g;
        self.compose.jobs.clear();
        self.compose.text.clear();
        let mut count = 0u32;
        let mut col = 0;
        while col < g.cols {
            let Some(fc) = p.frame.cell(row, col) else {
                break;
            };
            let here = col;
            col = col.saturating_add(1);
            let ncells = span(p.frame, row, here, fc.width, g.cols);
            // a cell that joins or leaves a wide pair draws differently
            let role = u8::try_from(ncells).unwrap_or(1);
            let mut changed = fc.changed;
            if let Some(s) = self.cells.get_mut(here, row) {
                changed |= core::mem::replace(&mut s.span, role) != role;
            }
            if ncells == 0 {
                continue;
            }
            let is_cursor = p.at == Some((here, row));
            let hidden = blink_hides(&fc, is_cursor, p.state);
            if ncells == 2 {
                changed |= p.frame.cell(row, col).is_some_and(|t| t.changed);
                if let Some(t) = self.cells.get_mut(col, row) {
                    changed |= core::mem::replace(&mut t.span, 0) != 0;
                }
            }
            let Some(s) = self.cells.get_mut(here, row) else {
                continue;
            };
            let look = is_cursor.then_some((p.cursor.shape, p.cursor.color));
            changed |= s.cursor != look || s.hidden != hidden;
            s.cursor = look;
            s.hidden = hidden;
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
                hidden,
            );
        }
        self.widen_to_span(p, row);
        let span = self.compose.layered_span().map(|(a, b)| {
            (
                (a, b),
                self.layout.cells_area(g, a, row, b.saturating_sub(a)),
            )
        });
        self.compose
            .draw(&self.font, span, self.layout.strip(), target);
        count
    }

    /// The jobs for one cell: its own, and for a bar cursor on a wide cell,
    /// its second half's.
    fn push_jobs(
        &mut self,
        g: &Grid,
        fc: &FrameCell<'_>,
        at: Place,
        cursor: Option<&Cursor>,
        hidden: bool,
    ) {
        let d = self.config.decorations;
        // the frame has dimmed the foreground; the cursor draws over it
        let (mut fg, mut bg) = (fc.fg, fc.bg);
        let mut underline = fc.attrs.underline();
        let mut ul_cursor = false;
        let mut bar = false;
        match cursor.map(|c| (c.shape, c.color)) {
            Some((CursorShape::Block, color)) => {
                core::mem::swap(&mut fg, &mut bg);
                if let Some(c) = color {
                    bg = c;
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
        let cursor_color = cursor.and_then(|c| c.color);
        let ul_color = if ul_cursor {
            cursor_color.unwrap_or(fg)
        } else {
            match (fc.underline_color, d.force, d.underline) {
                (Some(c), false, _) => c,
                (_, _, DecoColor::Rgb(c)) => c,
                (_, _, DecoColor::Foreground) => fg,
            }
        };
        let strike = fc.attrs.contains(Attrs::STRIKE).then_some(match d.strike {
            DecoColor::Foreground => fg,
            DecoColor::Rgb(c) => c,
        });
        let underline = Kind::underline(underline).map(|k| (k, ul_color));
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
            if !hidden {
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
            };
            let job = self.job(g, tail, 1, paint);
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
        };
        let job = self.job(g, at, glyph_cells, paint);
        self.compose.jobs.push(job);
    }

    fn job(&self, g: &Grid, at: Place, glyph_cells: usize, paint: Paint) -> Job {
        let l = &self.layout;
        Job {
            posx: at.col,
            ncells: at.ncells,
            r: l.cells_area(g, at.col, at.row, at.ncells),
            g: l.cells_area(g, at.col, at.row, glyph_cells),
            tiles: [
                l.cells_area(g, at.col, at.row, 1),
                l.cells_area(g, at.col.saturating_add(1), at.row, 1),
            ],
            fg: paint.fg,
            bg: paint.bg,
            underline: paint.underline,
            strike: paint.strike,
            text: paint.text,
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
        let mut col = first;
        while col < end {
            let Some(fc) = p.frame.cell(row, col) else {
                break;
            };
            let here = col;
            col = col.saturating_add(1);
            let ncells = span(p.frame, row, here, fc.width, g.cols);
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
            let hidden = blink_hides(&fc, is_cursor, p.state);
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
                hidden,
            );
        }
        self.compose.jobs.sort_by_key(|j| j.posx);
    }

    /// Where the pointer centred at `px`, `py` in the grid's pixels draws:
    /// the font's `I` in the default colours, a cell's box centred on it,
    /// clamped inside the grid.
    fn place_pointer(&self, g: &Grid, state: &DrawState, (px, py): (u32, u32)) -> Option<Pointer> {
        if g.cols == 0 || g.rows == 0 {
            return None;
        }
        let l = self.layout;
        let (cw, ch) = (l.cw, l.ch);
        let (gw, gh) = (
            int(g.cols).saturating_mul(cw),
            int(g.rows).saturating_mul(ch),
        );
        let px = int(px).clamp(cw / 2, gw.saturating_sub(cw / 2).max(cw / 2));
        let py = int(py).clamp(ch / 2, gh.saturating_sub(ch / 2).max(ch / 2));
        let turned = l.orientation.turned();
        let (half_w, half_h) = if turned {
            ((ch.saturating_add(1)) / 2, (cw.saturating_add(1)) / 2)
        } else {
            ((cw.saturating_add(1)) / 2, (ch.saturating_add(1)) / 2)
        };
        let (x, y) = match l.orientation {
            Orientation::Normal => (px.saturating_add(g.off_x), py.saturating_add(g.off_y)),
            Orientation::UpsideDown => (g.max_x.saturating_sub(px), g.max_y.saturating_sub(py)),
            Orientation::Right => (g.max_x.saturating_sub(py), px.saturating_add(g.off_y)),
            Orientation::Left => (py.saturating_add(g.off_x), g.max_y.saturating_sub(px)),
        };
        let x = x.clamp(half_w, g.max_x.saturating_sub(half_w).max(half_w));
        let y = y.clamp(half_h, g.max_y.saturating_sub(half_h).max(half_h));
        let (w, h) = if turned { (ch, cw) } else { (cw, ch) };
        // the cells under it, as the C reference found them
        let (fw, fh) = ((cw.saturating_add(1)) / 2, (ch.saturating_add(1)) / 2);
        let col = if px > fw {
            px.saturating_sub(fw).checked_div(cw).unwrap_or(0)
        } else {
            0
        };
        let row = if py > fh {
            py.saturating_sub(fh).checked_div(ch).unwrap_or(0)
        } else {
            0
        };
        Some(Pointer {
            at: Area {
                x: x.saturating_sub(half_w),
                y: y.saturating_sub(half_h),
                w,
                h,
            },
            col: usize::try_from(col)
                .unwrap_or(0)
                .min(g.cols.saturating_sub(1)),
            row: usize::try_from(row)
                .unwrap_or(0)
                .min(g.rows.saturating_sub(1)),
            colors: (state.foreground, state.background),
        })
    }

    /// The cells under `p` changed on this frame.
    fn stamp_pointer(&mut self, p: &Pointer) {
        for r in p.row..p.row.saturating_add(2) {
            for c in p.col..p.col.saturating_add(2) {
                self.cells.stamp(c, r);
            }
        }
    }

    /// Whether this frame drew a cell under `p`, and so drew over it.
    fn pointer_cells_drawn(&self, p: &Pointer) -> bool {
        (p.row..p.row.saturating_add(2)).any(|r| self.cells.any_drawn(p.col, r, 2))
    }

    /// Draws the pointer over the cells under it, which join the damage.
    fn draw_pointer(&mut self, p: &Pointer, target: &mut impl Target) {
        let l = self.layout;
        self.compose.draw_alone(
            &self.font,
            POINTER,
            p.colors,
            p.at,
            (l.width, l.height),
            target,
        );
        for r in p.row..p.row.saturating_add(2) {
            self.cells.mark_drawn(p.col, r, 2);
        }
    }
}
