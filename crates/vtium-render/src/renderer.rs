//! [`Renderer`]: draws frames into a [`Target`], redrawing only what
//! changed for the buffer being drawn.

use std::time::{Duration, Instant};

use vtium_font::{Face, Font, Style, Width as FontWidth};
use vtium_term::{Attrs, Cursor, CursorShape, Frame, FrameCell, Glyph, Rgb, Width};

use crate::blend::{argb_over, mask, over_bg, pixel, tint_over};
use crate::bounds::ROW_SCRATCH_MAX;
use crate::cache::{Budgets, GlyphCache, Profile, Storage};
use crate::deco::{Kind, Sprites};
use crate::stored::Stored;
use crate::target::{Orientation, Rect, Target};

/// The bar cursor: U+258F, LEFT ONE EIGHTH BLOCK, a sprite.
const BAR: char = '\u{258f}';

/// The pointer's glyph.
const POINTER: char = 'I';

/// Damaged cells of a row at most this many cells apart share one damage
/// rectangle.
const DAMAGE_MERGE: u8 = 3;

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
    /// The display's buffers, drawn in turn: a cell that changed is drawn
    /// into each of them once. 0 is taken as 1.
    pub buffers: u8,
    pub decorations: DecorationColors,
}

impl Default for Config {
    /// Unturned, the default profile, both storage savings, and two
    /// buffers, as a display that flips between two dumb buffers has.
    fn default() -> Config {
        Config {
            orientation: Orientation::default(),
            profile: Profile::default(),
            budgets: None,
            storage: Storage::default(),
            buffers: 2,
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
    /// The pointer's centre in the grid's pixels, unturned, when shown.
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
/// nothing and needs no flip.
#[derive(Debug)]
pub struct Drawn<'a> {
    pub damage: &'a [Rect],
    pub stats: Stats,
}

/// A rectangle in display pixels that may reach past the display.
#[derive(Clone, Copy, Debug, Default)]
struct Area {
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}

impl Area {
    fn bottom(&self) -> i64 {
        self.y.saturating_add(self.h)
    }
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

/// What the renderer last drew at a cell, for the changes the frame cannot
/// know.
#[derive(Clone, Copy, Debug, Default)]
struct CellState {
    /// Buffers that still lack the cell's newest pixels.
    pending: u8,
    cursor: bool,
    hidden: bool,
    /// The cells it drew over: 0 as a trailing half, 1 or 2.
    span: u8,
    /// Drawn on this frame: in its damage.
    drawn: bool,
}

/// One cell to draw.
#[derive(Clone, Copy, Debug)]
struct Job {
    posx: usize,
    ncells: usize,
    /// Where it draws.
    r: Area,
    /// Where its glyph lies, which a wide glyph cut at the margin passes.
    g: Area,
    tiles: [Area; 2],
    fg: Rgb,
    bg: Rgb,
    underline: Option<(Kind, Rgb)>,
    strike: Option<Rgb>,
    /// The glyph's text in the row's text buffer, its width and style.
    text: Option<(usize, usize, FontWidth, Style)>,
}

impl Job {
    fn layered(&self) -> bool {
        self.underline.is_some() || self.strike.is_some()
    }
}

/// Lines a cell is composed in.
#[derive(Debug, Default)]
struct Lines {
    cov: Vec<u8>,
    prem: Vec<u32>,
    src: Vec<u32>,
    out: Vec<u32>,
}

/// Draws terminal frames with a font into a display buffer.
#[derive(Debug)]
pub struct Renderer<F> {
    font: Font<F>,
    config: Config,
    width: u32,
    height: u32,
    cw: i64,
    ch: i64,
    max_cols: usize,
    max_rows: usize,
    grid: Option<Grid>,
    cells: Vec<CellState>,
    rows_pending: Vec<bool>,
    /// Buffers whose border still lacks the background.
    borders: u8,
    background: Option<Rgb>,
    blink_hidden: bool,
    cursor_at: Option<(usize, usize)>,
    cache: GlyphCache,
    sprites: Sprites,
    lines: Lines,
    jobs: Vec<Job>,
    text: Vec<char>,
    scratch: Vec<u32>,
    damage: Vec<Rect>,
}

fn int(v: impl TryInto<i64>) -> i64 {
    v.try_into().unwrap_or(i64::MAX)
}

/// How many cells the cell at `col` draws over: 0 for the trailing half of
/// a wide character, which the leading half draws; 2 for a leading half
/// with its trailing half on the screen; else 1. A half whose partner is
/// missing draws as one cell, so no frame leaves a cell undrawn.
fn span(frame: &Frame<'_>, row: usize, col: usize, cols: usize) -> usize {
    let width = |c: usize| frame.cell(row, c).map(|f| f.width);
    let next = col.saturating_add(1);
    match width(col) {
        Some(Width::WideTail) if col.checked_sub(1).and_then(width) == Some(Width::Wide) => 0,
        Some(Width::Wide) if next < cols && width(next) == Some(Width::WideTail) => 2,
        _ => 1,
    }
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
            cache: GlyphCache::new(budgets, config.storage, config.orientation),
            sprites,
            font,
            config: Config {
                buffers: config.buffers.max(1),
                ..config
            },
            width,
            height,
            cw: int(cw),
            ch: int(ch),
            max_cols,
            max_rows,
            grid: None,
            cells: Vec::new(),
            rows_pending: Vec::new(),
            borders: 0,
            background: None,
            blink_hidden: false,
            cursor_at: None,
            lines: Lines::default(),
            jobs: Vec::new(),
            text: Vec::new(),
            scratch: Vec::new(),
            damage: Vec::new(),
        }
    }

    pub fn font(&self) -> &Font<F> {
        &self.font
    }

    /// The columns and rows the display holds.
    pub fn grid_size(&self) -> (usize, usize) {
        (self.max_cols, self.max_rows)
    }

    /// Bytes of glyph pixels the cache holds.
    pub fn cache_bytes(&self) -> u64 {
        self.cache.pixel_bytes()
    }

    /// Bytes the row scratch holds once a row of several layers is drawn:
    /// one row of cells across the display, at most `ROW_SCRATCH_MAX`.
    pub fn row_scratch_bytes(&self) -> usize {
        let (line, lines) = self.strip();
        line.saturating_mul(lines).saturating_mul(4)
    }

    /// Redraw everything into every buffer, as after the display lost its
    /// picture.
    pub fn redraw(&mut self) {
        self.grid = None;
    }

    /// The row scratch's line length and lines.
    fn strip(&self) -> (usize, usize) {
        let (line, lines) = if self.config.orientation.turned() {
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
        let (across, down) = if self.config.orientation.turned() {
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
        let (x, y, w, h) = match self.config.orientation {
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

    /// Draws `frame` into `target`: the cells that changed since this
    /// buffer last had them, the border when it needs the background, and
    /// the pointer.
    #[expect(
        clippy::disallowed_methods,
        reason = "the render timer's CPU time per frame"
    )]
    pub fn draw(
        &mut self,
        frame: &Frame<'_>,
        target: &mut impl Target,
        state: &DrawState,
    ) -> Drawn<'_> {
        let start = Instant::now();
        self.damage.clear();
        self.cache.begin_frame();
        let grid = self.grid_for(frame);
        if self.grid != Some(grid) || self.background != Some(state.background) {
            self.full_redraw(grid, state.background);
        }
        let mut full = false;
        if self.borders > 0 {
            self.borders = self.borders.saturating_sub(1);
            self.fill_border(&grid, target, state.background);
            full = true;
        }
        let cells = self.draw_cells(&grid, frame, target, state);
        if let Some(p) = state.pointer {
            self.draw_pointer(&grid, target, state, p);
        }
        self.collect_damage(&grid, full);
        let counts = self.cache.take_counts();
        let damage_px = self
            .damage
            .iter()
            .fold(0u64, |n, r| n.saturating_add(r.area()));
        Drawn {
            damage: &self.damage,
            stats: Stats {
                cells,
                misses: counts.misses,
                uncached: counts.uncached,
                damage_px,
                cpu: start.elapsed(),
            },
        }
    }

    fn full_redraw(&mut self, grid: Grid, background: Rgb) {
        let n = grid.cols.saturating_mul(grid.rows);
        self.cells.clear();
        self.cells.resize(
            n,
            CellState {
                pending: self.config.buffers,
                ..CellState::default()
            },
        );
        self.rows_pending.clear();
        self.rows_pending.resize(grid.rows, true);
        self.borders = self.config.buffers;
        self.grid = Some(grid);
        self.background = Some(background);
        self.cursor_at = None;
    }

    fn fill_border(&mut self, g: &Grid, target: &mut impl Target, bg: Rgb) {
        let width = usize::try_from(self.width).unwrap_or(0);
        self.lines.out.clear();
        self.lines.out.resize(width, pixel(bg));
        let line = &self.lines.out;
        let (x0, x1) = (
            u32::try_from(g.off_x).unwrap_or(0),
            u32::try_from(g.max_x).unwrap_or(0),
        );
        let left = line.get(..usize::try_from(x0).unwrap_or(0)).unwrap_or(&[]);
        let right = line.get(usize::try_from(x1).unwrap_or(0)..).unwrap_or(&[]);
        for y in 0..self.height {
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
        match c.col.checked_sub(1) {
            Some(prev) if span(frame, c.row, c.col, cols) == 0 => (prev, c.row),
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
        let mut drawn = 0u32;
        for row in 0..g.rows {
            let pending = self.rows_pending.get(row).copied().unwrap_or(true);
            let scan = pending
                || blink_toggled
                || frame.row_changed(row)
                || at.is_some_and(|(_, r)| r == row)
                || last_cursor.is_some_and(|(_, r)| r == row);
            if scan {
                drawn =
                    drawn.saturating_add(self.draw_row(g, frame, target, state, row, at, &cursor));
            }
        }
        drawn
    }

    fn state(&mut self, g: &Grid, col: usize, row: usize) -> Option<&mut CellState> {
        self.cells
            .get_mut(row.checked_mul(g.cols)?.checked_add(col)?)
    }

    /// Scans one row, updates what each cell last drew, and draws the cells
    /// some buffer lacks; returns how many.
    #[expect(clippy::too_many_arguments, reason = "one frame's inputs")]
    fn draw_row(
        &mut self,
        g: &Grid,
        frame: &Frame<'_>,
        target: &mut impl Target,
        state: &DrawState,
        row: usize,
        at: Option<(usize, usize)>,
        cursor: &Cursor,
    ) -> u32 {
        self.jobs.clear();
        self.text.clear();
        let buffers = self.config.buffers;
        let mut row_pending = false;
        let mut count = 0u32;
        let mut col = 0;
        while col < g.cols {
            let Some(fc) = frame.cell(row, col) else {
                break;
            };
            let here = col;
            col = col.saturating_add(1);
            let ncells = span(frame, row, here, g.cols);
            // a cell that joins or leaves a wide pair draws differently
            let role = u8::try_from(ncells).unwrap_or(1);
            let mut changed = fc.changed;
            if let Some(s) = self.state(g, here, row) {
                changed |= core::mem::replace(&mut s.span, role) != role;
            }
            if ncells == 0 {
                continue;
            }
            let is_cursor = at == Some((here, row));
            let blinks = fc.attrs.contains(Attrs::BLINK) || fc.attrs.contains(Attrs::RAPID_BLINK);
            let hidden = !is_cursor && blinks && state.blink_hidden;
            let mut tail_pending = 0;
            if ncells == 2 {
                changed |= frame.cell(row, col).is_some_and(|t| t.changed);
                if let Some(t) = self.state(g, col, row) {
                    tail_pending = t.pending;
                    changed |= core::mem::replace(&mut t.span, 0) != 0;
                }
            }
            let Some(s) = self.state(g, here, row) else {
                continue;
            };
            changed |= s.cursor != is_cursor || s.hidden != hidden;
            s.cursor = is_cursor;
            s.hidden = hidden;
            s.pending = if changed {
                buffers
            } else {
                s.pending.max(tail_pending)
            };
            if s.pending == 0 {
                continue;
            }
            s.pending = s.pending.saturating_sub(1);
            s.drawn = true;
            let left = s.pending;
            row_pending |= left > 0;
            if ncells == 2 {
                if let Some(t) = self.state(g, col, row) {
                    t.pending = left;
                    t.drawn = true;
                }
            }
            count = count.saturating_add(1);
            self.push_jobs(
                g,
                &fc,
                here,
                row,
                ncells,
                is_cursor.then_some(cursor),
                hidden,
            );
        }
        if let Some(p) = self.rows_pending.get_mut(row) {
            *p = row_pending;
        }
        self.widen_to_span(g, frame, row, at, cursor, state);
        self.draw_jobs(g, row, target);
        count
    }

    /// The jobs for one cell: its own, and for a bar cursor on a wide cell,
    /// its second half's.
    #[expect(clippy::too_many_arguments, reason = "one cell's inputs")]
    fn push_jobs(
        &mut self,
        g: &Grid,
        fc: &FrameCell<'_>,
        col: usize,
        row: usize,
        ncells: usize,
        cursor: Option<&Cursor>,
        hidden: bool,
    ) {
        let d = self.config.decorations;
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
        let start = self.text.len();
        let (width, mut ncells) = if bar {
            self.text.push(BAR);
            (FontWidth::Narrow, ncells)
        } else {
            if !hidden {
                match fc.glyph {
                    Glyph::Empty => {}
                    Glyph::Char(c) => self.text.push(c),
                    Glyph::Cluster(s) => self.text.extend(s.chars()),
                }
            }
            let w = if fc.width == Width::Wide {
                FontWidth::Wide
            } else {
                FontWidth::Narrow
            };
            (w, ncells)
        };
        let len = self.text.len().saturating_sub(start);
        let text = (len > 0).then_some((start, len, width, style));
        if bar && ncells == 2 {
            // the bar takes the left half; the right is the cell without it
            let tail = col.saturating_add(1);
            self.jobs
                .push(self.job(g, tail, row, 1, 1, fc.fg, fc.bg, underline, strike, None));
            ncells = 1;
        }
        let fg = if bar { cursor_color.unwrap_or(fg) } else { fg };
        let glyph_cells = if width == FontWidth::Wide { 2 } else { 1 };
        let job = self.job(
            g,
            col,
            row,
            ncells,
            glyph_cells,
            fg,
            bg,
            underline,
            strike,
            text,
        );
        self.jobs.push(job);
    }

    #[expect(clippy::too_many_arguments, reason = "a job's fields")]
    fn job(
        &self,
        g: &Grid,
        col: usize,
        row: usize,
        ncells: usize,
        glyph_cells: usize,
        fg: Rgb,
        bg: Rgb,
        underline: Option<(Kind, Rgb)>,
        strike: Option<Rgb>,
        text: Option<(usize, usize, FontWidth, Style)>,
    ) -> Job {
        Job {
            posx: col,
            ncells,
            r: self.cells_area(g, col, row, ncells),
            g: self.cells_area(g, col, row, glyph_cells),
            tiles: [
                self.cells_area(g, col, row, 1),
                self.cells_area(g, col.saturating_add(1), row, 1),
            ],
            fg,
            bg,
            underline,
            strike,
            text,
        }
    }

    /// When the row draws a cell of several layers, every cell between the
    /// first and the last of them is composed with them in the row scratch,
    /// so those not yet drawn join the jobs.
    fn widen_to_span(
        &mut self,
        g: &Grid,
        frame: &Frame<'_>,
        row: usize,
        at: Option<(usize, usize)>,
        cursor: &Cursor,
        state: &DrawState,
    ) {
        let layered = self.jobs.iter().filter(|j| j.layered());
        let Some(first) = layered.clone().map(|j| j.posx).min() else {
            return;
        };
        let end = layered
            .map(|j| j.posx.saturating_add(j.ncells))
            .max()
            .unwrap_or(first);
        let mut col = first;
        while col < end {
            let Some(fc) = frame.cell(row, col) else {
                break;
            };
            let here = col;
            col = col.saturating_add(1);
            let ncells = span(frame, row, here, g.cols);
            if ncells == 0 {
                continue;
            }
            let covered = self
                .jobs
                .iter()
                .any(|j| here >= j.posx && here < j.posx.saturating_add(j.ncells));
            if covered {
                continue;
            }
            let is_cursor = at == Some((here, row));
            let blinks = fc.attrs.contains(Attrs::BLINK) || fc.attrs.contains(Attrs::RAPID_BLINK);
            let hidden = !is_cursor && blinks && state.blink_hidden;
            for c in here..here.saturating_add(ncells) {
                if let Some(s) = self.state(g, c, row) {
                    s.drawn = true;
                }
            }
            self.push_jobs(
                g,
                &fc,
                here,
                row,
                ncells,
                is_cursor.then_some(cursor),
                hidden,
            );
        }
        self.jobs.sort_by_key(|j| j.posx);
    }

    /// Draws the row's jobs: a cell of one layer straight into the target,
    /// the span of cells of several layers through the row scratch.
    fn draw_jobs(&mut self, g: &Grid, row: usize, target: &mut impl Target) {
        let span = {
            let mut layered = self.jobs.iter().filter(|j| j.layered());
            layered.next().map(|first| {
                let end = layered
                    .chain(core::iter::once(first))
                    .map(|j| j.posx.saturating_add(j.ncells))
                    .max()
                    .unwrap_or(first.posx);
                (first.posx, end)
            })
        };
        let in_span = |j: &Job| span.is_some_and(|(a, b)| j.posx >= a && j.posx < b);
        let Renderer {
            font,
            cache,
            sprites,
            lines,
            jobs,
            text,
            ..
        } = self;
        for job in jobs.iter().filter(|j| !in_span(j)) {
            let glyph = glyph(cache, font, text, job);
            for y in job.r.y..job.r.bottom() {
                let w = usize::try_from(job.r.w).unwrap_or(0);
                let mut out = core::mem::take(&mut lines.out);
                out.clear();
                out.resize(w, 0);
                cell_line(job, glyph, sprites, lines, y, &mut out);
                if let (Ok(x), Ok(y)) = (u32::try_from(job.r.x), u32::try_from(y)) {
                    target.write(x, y, &out);
                }
                lines.out = out;
            }
        }
        if let Some((a, b)) = span {
            let area = self.cells_area(g, a, row, b.saturating_sub(a));
            self.draw_span(area, target);
        }
    }

    /// Composes the jobs inside `area` in the row scratch, a band of lines
    /// at a time, and copies each line out once.
    fn draw_span(&mut self, area: Area, target: &mut impl Target) {
        let (line_max, band_max) = self.strip();
        let w = usize::try_from(area.w).unwrap_or(0).min(line_max);
        let band = band_max.max(1);
        let need = line_max.saturating_mul(band);
        if self.scratch.len() < need {
            self.scratch.resize(need, 0);
        }
        let Renderer {
            font,
            cache,
            sprites,
            lines,
            jobs,
            text,
            scratch,
            ..
        } = self;
        let mut top = area.y;
        while top < area.bottom() {
            let lines_here = int(band).min(area.bottom().saturating_sub(top));
            let bottom = top.saturating_add(lines_here);
            for job in jobs.iter() {
                if job.r.x < area.x
                    || job.r.x.saturating_add(job.r.w) > area.x.saturating_add(area.w)
                {
                    continue;
                }
                let glyph = glyph(cache, font, text, job);
                let dx = usize::try_from(job.r.x.saturating_sub(area.x)).unwrap_or(0);
                let jw = usize::try_from(job.r.w).unwrap_or(0);
                for y in job.r.y.max(top)..job.r.bottom().min(bottom) {
                    let at = usize::try_from(y.saturating_sub(top))
                        .unwrap_or(0)
                        .saturating_mul(w)
                        .saturating_add(dx);
                    if let Some(out) = scratch.get_mut(at..at.saturating_add(jw)) {
                        cell_line(job, glyph, sprites, lines, y, out);
                    }
                }
            }
            for (i, y) in (top..bottom).enumerate() {
                let at = i.saturating_mul(w);
                if let (Some(line), Ok(x), Ok(y)) = (
                    scratch.get(at..at.saturating_add(w)),
                    u32::try_from(area.x),
                    u32::try_from(y),
                ) {
                    target.write(x, y, line);
                }
            }
            top = bottom;
        }
    }

    /// The pointer: the font's `I` in the default colours, a cell's box
    /// centred on it, clamped inside the grid; the cells under it are drawn
    /// again on the next frames.
    fn draw_pointer(
        &mut self,
        g: &Grid,
        target: &mut impl Target,
        state: &DrawState,
        (px, py): (u32, u32),
    ) {
        if g.cols == 0 || g.rows == 0 {
            return;
        }
        let (cw, ch) = (self.cw, self.ch);
        let (gw, gh) = (
            int(g.cols).saturating_mul(cw),
            int(g.rows).saturating_mul(ch),
        );
        let px = int(px).clamp(cw / 2, gw.saturating_sub(cw / 2).max(cw / 2));
        let py = int(py).clamp(ch / 2, gh.saturating_sub(ch / 2).max(ch / 2));
        let turned = self.config.orientation.turned();
        let (half_w, half_h) = if turned {
            ((ch.saturating_add(1)) / 2, (cw.saturating_add(1)) / 2)
        } else {
            ((cw.saturating_add(1)) / 2, (ch.saturating_add(1)) / 2)
        };
        let (x, y) = match self.config.orientation {
            Orientation::Normal => (px.saturating_add(g.off_x), py.saturating_add(g.off_y)),
            Orientation::UpsideDown => (g.max_x.saturating_sub(px), g.max_y.saturating_sub(py)),
            Orientation::Right => (g.max_x.saturating_sub(py), px.saturating_add(g.off_y)),
            Orientation::Left => (py.saturating_add(g.off_x), g.max_y.saturating_sub(px)),
        };
        let x = x.clamp(half_w, g.max_x.saturating_sub(half_w).max(half_w));
        let y = y.clamp(half_h, g.max_y.saturating_sub(half_h).max(half_h));
        let (w, h) = if turned { (ch, cw) } else { (cw, ch) };
        let at = Area {
            x: x.saturating_sub(half_w),
            y: y.saturating_sub(half_h),
            w,
            h,
        };
        self.text.clear();
        self.text.push(POINTER);
        let job = Job {
            posx: 0,
            ncells: 1,
            r: at,
            g: at,
            tiles: [Area::default(); 2],
            fg: state.foreground,
            bg: state.background,
            underline: None,
            strike: None,
            text: Some((0, 1, FontWidth::Narrow, Style::default())),
        };
        let (sw, sh) = (int(self.width), int(self.height));
        let Renderer {
            font,
            cache,
            sprites,
            lines,
            text,
            ..
        } = self;
        let glyph = glyph(cache, font, text, &job);
        let x0 = at.x.max(0);
        let x1 = at.x.saturating_add(at.w).min(sw);
        for y in at.y.max(0)..at.bottom().min(sh) {
            let mut out = core::mem::take(&mut lines.out);
            out.clear();
            out.resize(usize::try_from(at.w).unwrap_or(0), 0);
            cell_line(&job, glyph, sprites, lines, y, &mut out);
            let from = usize::try_from(x0.saturating_sub(at.x)).unwrap_or(0);
            let to = usize::try_from(x1.saturating_sub(at.x)).unwrap_or(0);
            if let (Some(run), Ok(x), Ok(y)) =
                (out.get(from..to), u32::try_from(x0), u32::try_from(y))
            {
                target.write(x, y, run);
            }
            lines.out = out;
        }
        // the cells under the pointer, as the C reference marked them
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
        let col = usize::try_from(col)
            .unwrap_or(0)
            .min(g.cols.saturating_sub(1));
        let row = usize::try_from(row)
            .unwrap_or(0)
            .min(g.rows.saturating_sub(1));
        let buffers = self.config.buffers;
        for (c, r) in [
            (col, row),
            (col.saturating_add(1), row),
            (col, row.saturating_add(1)),
            (col.saturating_add(1), row.saturating_add(1)),
        ] {
            if c >= g.cols || r >= g.rows {
                continue;
            }
            if let Some(s) = self.state(g, c, r) {
                s.pending = buffers;
                s.drawn = true;
            }
            if let Some(p) = self.rows_pending.get_mut(r) {
                *p = true;
            }
        }
    }

    /// This frame's damage: the whole display after a border fill, else
    /// each row's drawn cells, those at most `DAMAGE_MERGE` cells apart in
    /// one rectangle.
    fn collect_damage(&mut self, g: &Grid, full: bool) {
        for row in 0..g.rows {
            let mut gap = 0u8;
            for col in 0..g.cols {
                let Some(s) = self.state(g, col, row) else {
                    continue;
                };
                if !s.drawn {
                    gap = gap.saturating_sub(1);
                    continue;
                }
                s.drawn = false;
                if full {
                    continue;
                }
                let a = self.cells_area(g, col, row, 1);
                let r = Rect {
                    x: u32::try_from(a.x).unwrap_or(0),
                    y: u32::try_from(a.y).unwrap_or(0),
                    width: u32::try_from(a.w).unwrap_or(0),
                    height: u32::try_from(a.h).unwrap_or(0),
                };
                match self.damage.last_mut() {
                    Some(last) if gap > 0 => {
                        let x = last.x.min(r.x);
                        let y = last.y.min(r.y);
                        let right = last
                            .x
                            .saturating_add(last.width)
                            .max(r.x.saturating_add(r.width));
                        let bottom = last
                            .y
                            .saturating_add(last.height)
                            .max(r.y.saturating_add(r.height));
                        *last = Rect {
                            x,
                            y,
                            width: right.saturating_sub(x),
                            height: bottom.saturating_sub(y),
                        };
                    }
                    _ => self.damage.push(r),
                }
                gap = DAMAGE_MERGE;
            }
        }
        if full {
            self.damage.clear();
            self.damage.push(Rect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            });
        }
    }
}

/// The job's glyph from the cache, if it has one.
fn glyph<'c, F: Face>(
    cache: &'c mut GlyphCache,
    font: &Font<F>,
    text: &[char],
    job: &Job,
) -> Option<&'c Stored> {
    let (start, len, width, style) = job.text?;
    let chars = text.get(start..start.saturating_add(len))?;
    cache.get(font, chars, width, style)
}

/// One display line `y` of `job`, across its area, into `out`.
fn cell_line(
    job: &Job,
    glyph: Option<&Stored>,
    sprites: &Sprites,
    lines: &mut Lines,
    y: i64,
    out: &mut [u32],
) {
    let w = out.len();
    let x0 = job.r.x;
    lines.cov.resize(w, 0);
    lines.prem.resize(w, 0);
    lines.src.resize(w, 0);
    let cov = lines.cov.get_mut(..w).unwrap_or_default();
    let prem = lines.prem.get_mut(..w).unwrap_or_default();
    let src = lines.src.get_mut(..w).unwrap_or_default();
    let (gx, gy) = (x0.saturating_sub(job.g.x), y.saturating_sub(job.g.y));
    if !job.layered() {
        match glyph {
            None => out.fill(pixel(job.bg)),
            Some(g) if g.is_rgba() => {
                g.argb_line(gx, gy, src);
                for (o, &s) in out.iter_mut().zip(src.iter()) {
                    *o = over_bg(job.bg, s);
                }
            }
            Some(g) => {
                g.mask_line(gx, gy, cov);
                for (o, &a) in out.iter_mut().zip(cov.iter()) {
                    *o = mask(job.fg, job.bg, a);
                }
            }
        }
        return;
    }
    prem.fill(0);
    let tiles = job.tiles.get(..job.ncells).unwrap_or_default();
    let deco = |kind: Kind, c: Rgb, prem: &mut [u32], cov: &mut [u8]| {
        let Some(s) = sprites.get(kind) else { return };
        for t in tiles {
            s.mask_line(x0.saturating_sub(t.x), y.saturating_sub(t.y), cov);
            for (p, &a) in prem.iter_mut().zip(cov.iter()) {
                *p = tint_over(*p, a, c);
            }
        }
    };
    if let Some((kind, c)) = job.underline {
        deco(kind, c, prem, cov);
    }
    match glyph {
        Some(g) if g.is_rgba() => {
            g.argb_line(gx, gy, src);
            for (p, &s) in prem.iter_mut().zip(src.iter()) {
                *p = argb_over(*p, s);
            }
        }
        Some(g) => {
            g.mask_line(gx, gy, cov);
            for (p, &a) in prem.iter_mut().zip(cov.iter()) {
                *p = tint_over(*p, a, job.fg);
            }
        }
        None => {}
    }
    if let Some(c) = job.strike {
        deco(Kind::Strike, c, prem, cov);
    }
    for (o, &p) in out.iter_mut().zip(prem.iter()) {
        *o = over_bg(job.bg, p);
    }
}
