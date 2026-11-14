//! The console's own drawing over a display's frame (ADR-1401): a
//! [`Notice`], vtium's message in a box (D7, D8, D9), and a head's focus
//! [`Mark`] (D5). Neither passes through the terminal: no cell, pty,
//! scrollback or reply sees them, and the renderer's record of the cells
//! stays as it was.
//!
//! Each is drawn with the font it is given, which the console makes from
//! the built-in sets at the head's cell size (ADR-1200 D7), and turns with
//! the display. Drawing is stateless, so the owner decides how long a
//! notice shows, until a key or for a time (ADR-1401's probe 5): it draws
//! the notice again over each frame that writes beneath it, adds the area
//! returned to the frame's damage, and once the notice ends draws the next
//! frame whole (age 0), which restores every cell beneath it.
//!
//! The notice sits across the top of the text's view, where a getty prints
//! its banner. The mark fills the grid cell its owner gives it, so the
//! owner chooses the cell and whether the grid gives it up. The owner
//! draws the mark after the notice and gives the notice the mark's cell
//! to keep its text clear of.

use vtium_font::{Face, Font, Format, Glyph, Style, Variant, Width};
use vtium_term::Rgb;

use crate::blend::{mask, over_bg, pixel};
use crate::mono::{Depth, Fallback, Ink, Threshold};
use crate::target::{Orientation, Rect, Target};

/// A display's buffer as the console's own drawing sees it: its size, how
/// it is turned and its depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub orientation: Orientation,
    pub depth: Depth,
}

impl Surface {
    /// The text's view, unturned: its width and height.
    fn view(&self) -> (u32, u32) {
        if self.orientation.turned() {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// The display pixel that draws the view's pixel `x`, `y`, as the
    /// renderer turns the grid.
    pub fn place(&self, x: u32, y: u32) -> (u32, u32) {
        let last = |n: u32, v: u32| n.saturating_sub(1).saturating_sub(v);
        match self.orientation {
            Orientation::Normal => (x, y),
            Orientation::UpsideDown => (last(self.width, x), last(self.height, y)),
            Orientation::Right => (last(self.width, y), x),
            Orientation::Left => (y, last(self.height, x)),
        }
    }

    /// The display area that draws the view's area `r`.
    fn area(&self, r: Rect) -> Rect {
        self.map(r, Surface::place)
    }

    /// The view's area the display area `r` draws: [`Surface::area`]
    /// undone.
    pub fn view_area(&self, r: Rect) -> Rect {
        self.map(r, Surface::unplace)
    }

    /// `r` with its corners taken through `to`, a quarter turn swapping
    /// its sides.
    fn map(&self, r: Rect, to: fn(&Surface, u32, u32) -> (u32, u32)) -> Rect {
        let (x0, y0) = to(self, r.x, r.y);
        let far = |v: u32, n: u32| v.saturating_add(n).saturating_sub(1);
        let (x1, y1) = to(self, far(r.x, r.width), far(r.y, r.height));
        let (width, height) = if self.orientation.turned() {
            (r.height, r.width)
        } else {
            (r.width, r.height)
        };
        Rect {
            x: x0.min(x1),
            y: y0.min(y1),
            width,
            height,
        }
    }

    /// Writes `picture`, the view's area at its `x`, `y`, to `target`,
    /// turned and, at 1 bit, each pixel lit or unlit. Returns the display
    /// area it covers.
    fn put(&self, at: (u32, u32), picture: &Picture, target: &mut impl Target) -> Rect {
        let view = Rect {
            x: at.0,
            y: at.1,
            width: picture.width,
            height: picture.height,
        };
        let area = self.area(view);
        let mut line = Vec::new();
        let mut target = Threshold::new(target, self.depth, &mut line);
        let mut row = Vec::with_capacity(usize::try_from(area.width).unwrap_or(0));
        for dy in 0..area.height {
            row.clear();
            for dx in 0..area.width {
                let (vx, vy) = self.unplace(area.x.saturating_add(dx), area.y.saturating_add(dy));
                row.push(picture.get(vx.saturating_sub(view.x), vy.saturating_sub(view.y)));
            }
            target.write(area.x, area.y.saturating_add(dy), &row);
        }
        area
    }

    /// The view's pixel the display pixel `x`, `y` draws: [`Surface::place`]
    /// undone.
    fn unplace(&self, x: u32, y: u32) -> (u32, u32) {
        let last = |n: u32, v: u32| n.saturating_sub(1).saturating_sub(v);
        match self.orientation {
            Orientation::Normal => (x, y),
            Orientation::UpsideDown => (last(self.width, x), last(self.height, y)),
            Orientation::Right => (y, last(self.width, x)),
            Orientation::Left => (last(self.height, y), x),
        }
    }
}

/// The colors the console draws in: the text, what it stands on, and a
/// notice's edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colors {
    pub ink: Rgb,
    pub paper: Rgb,
    pub edge: Rgb,
}

impl Colors {
    /// A notice's colors with none of the console's color keys set
    /// (ADR-1401 D5): dark text in a pale red box with a red edge.
    pub const NOTICE: Colors = Colors {
        ink: Rgb::new(0x1f, 0x29, 0x33),
        paper: Rgb::new(0xfe, 0xe2, 0xe2),
        edge: Rgb::new(0xb9, 0x1c, 0x1c),
    };

    /// The on-screen display's colors with none of the console's color
    /// keys set, for its line alone and its dot, which report no problem
    /// (ADR-1401 D5): light text in a dark box with a grey edge.
    pub const STATUS: Colors = Colors {
        ink: Rgb::new(0xe5, 0xe7, 0xeb),
        paper: Rgb::new(0x1f, 0x29, 0x33),
        edge: Rgb::new(0x61, 0x6e, 0x7c),
    };

    /// These colors at `depth`: at 1 bit the paper lit where it is light
    /// and the text and edge its opposite, as a cell's are.
    fn at(self, depth: Depth) -> Colors {
        let ink = Ink::new(depth, self.paper);
        Colors {
            ink: ink.fg(self.ink),
            paper: ink.bg(),
            edge: ink.fg(self.edge),
        }
    }
}

/// `text` from a file, such as a head's name, in double quotes, escaped as
/// Rust's `escape_debug` escapes it: a backslash, a quote, every control
/// character (C0, DEL and C1), and the characters Unicode leaves unprinted,
/// the bidirectional and zero-width ones among them, so no control
/// character reaches the notice or the log and the file cannot add a line
/// to vtium's message. A character that looks like another, such as U+2800,
/// a combining mark after a letter or a Cyrillic `е`, passes as it is.
pub fn quote(text: &str) -> String {
    format!("\"{}\"", text.escape_debug())
}

/// A message of vtium's own, drawn in a box over the display's frame, as a
/// getty prints its banner (ADR-1401 D7). A newline starts a line, and
/// each line wraps at a space to the box's width, a word longer than the
/// box breaking inside it. Text quoted from a file goes through [`quote`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    text: String,
    /// The report the text was made from, which keeps its count and its
    /// next step on a display too small for all of it
    /// ([`Notice::report`]).
    report: Option<Box<NoticeReport>>,
}

/// A report vtium shows, such as the panic screen (ADR-1401 D7): what
/// happened, a list of items such as a file's errors, how many more there
/// were, and what to do next, each part starting a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoticeReport {
    /// What happened, shown first, each a line of its own.
    pub happened: Vec<String>,
    pub items: Vec<String>,
    /// How many items there were past those in `items`.
    pub more: usize,
    /// The words a count of the items not shown follows, such as
    /// `More errors:`: a count is written as a label and a number, so a
    /// translation needs no plural forms (ADR-1402).
    pub label: String,
    /// What to do next, shown last.
    pub next: String,
    /// The drawing shown beside or above the text, where the text keeps
    /// every line it shows without it.
    pub drawing: Option<Drawing>,
}

/// A drawing a report shows with its text (ADR-1401 D7), the panic
/// screen's Tux. The text comes first: the notice draws the drawing only
/// where the text still shows every line it shows without it. The full
/// drawing goes to the left of the text where the text keeps
/// [`TEXT_BESIDE_MIN`] columns beside it, or else above the text; the small
/// one goes above the text where only it fits; and where neither fits, the
/// text shows alone. A drawing above the text is centered across the box,
/// with an empty row under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drawing {
    full: &'static str,
    small: &'static str,
    /// The sprite drawn in place of `full`, then the one drawn in place
    /// of `small`.
    #[cfg(feature = "images")]
    sprite: Option<(Sprite, Sprite)>,
}

/// The fewest columns the text keeps beside a drawing: 40, the width of
/// minspec's 320-pixel screen in an 8-pixel font, which the notices are
/// written to read at.
pub const TEXT_BESIDE_MIN: u32 = 40;

/// The columns between a drawing and the text beside it.
const DRAWING_GAP: u32 = 2;

impl Drawing {
    /// A drawing in text: each line of `full` and of `small`, the version
    /// for a screen where `full` and the text do not both fit, drawn a
    /// character to a cell in the notice's font.
    pub const fn text(full: &'static str, small: &'static str) -> Drawing {
        Drawing {
            full,
            small,
            #[cfg(feature = "images")]
            sprite: None,
        }
    }

    /// The drawing with `full` drawn in place of its full text and
    /// `small` in place of its small text, each at the largest
    /// whole-number scale that fits the cells its text would take, so
    /// each of its pixels stays sharp. The panic screen's `small` is its
    /// sprite cropped with [`Sprite::crop`], as its small text is cropped
    /// from its full one.
    #[cfg(feature = "images")]
    pub const fn with_sprite(self, full: Sprite, small: Sprite) -> Drawing {
        Drawing {
            sprite: Some((full, small)),
            ..self
        }
    }
}

/// A picture of a few colors that vtium carries in its own code, such as
/// the panic screen's Tux: its RGB samples, uncompressed, each pixel drawn
/// as the entry of its palette that it matches. A sprite is the whole
/// picture its samples hold, or a rectangle cut from it by
/// [`Sprite::crop`].
#[cfg(feature = "images")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sprite {
    width: u32,
    height: u32,
    /// The pixel of the samples' picture at the sprite's top left.
    at: (u32, u32),
    /// The width and height of the picture the samples hold.
    picture: (u32, u32),
    palette: &'static [Rgb],
    samples: &'static [u8],
}

#[cfg(feature = "images")]
impl Sprite {
    /// A sprite `width` by `height` pixels of `samples`: row by row, three
    /// bytes a pixel, red, green and blue, as a binary PPM holds them past
    /// its header. Each pixel draws as the entry of `palette` it matches.
    pub const fn new(
        width: u32,
        height: u32,
        palette: &'static [Rgb],
        samples: &'static [u8],
    ) -> Sprite {
        Sprite {
            width,
            height,
            at: (0, 0),
            picture: (width, height),
            palette,
            samples,
        }
    }

    /// The rectangle `width` by `height` pixels of this sprite whose top
    /// left pixel is its pixel at `x`, `y`, as a sprite of its own over
    /// the same samples. A rectangle that runs past this sprite's edge is
    /// cut at that edge.
    pub const fn crop(self, x: u32, y: u32, width: u32, height: u32) -> Sprite {
        // the pixels of this sprite from `x` and from `y` to its edges
        let (right, below) = (self.width.saturating_sub(x), self.height.saturating_sub(y));
        Sprite {
            width: if width < right { width } else { right },
            height: if height < below { height } else { below },
            at: (self.at.0.saturating_add(x), self.at.1.saturating_add(y)),
            ..self
        }
    }

    /// The sprite's width and height in pixels.
    pub const fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The entry of the palette that the pixel at `x`, `y` matches: `None`
    /// past the sprite's edge or its samples' end, and for a color that is
    /// no entry of its palette.
    pub fn entry(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let (x, y) = (self.at.0.checked_add(x)?, self.at.1.checked_add(y)?);
        if x >= self.picture.0 || y >= self.picture.1 {
            return None;
        }
        let at = u64::from(y)
            .checked_mul(u64::from(self.picture.0))?
            .checked_add(u64::from(x))?
            .checked_mul(3)?;
        let at = usize::try_from(at).ok()?;
        let rgb = self.samples.get(at..at.checked_add(3)?)?;
        self.palette.iter().position(|c| [c.r, c.g, c.b] == rgb)
    }

    /// The color of the pixel at `x`, `y`: its entry of the palette, as
    /// [`Sprite::entry`] finds it.
    pub fn get(&self, x: u32, y: u32) -> Option<Rgb> {
        self.palette.get(self.entry(x, y)?).copied()
    }

    /// Whether the samples hold exactly the pixels of the picture the
    /// sprite is cut from, and every pixel of the sprite matches an entry
    /// of the palette.
    pub fn is_whole(&self) -> bool {
        let pixels = u64::from(self.picture.0).saturating_mul(u64::from(self.picture.1));
        u64::try_from(self.samples.len()).ok() == Some(pixels.saturating_mul(3))
            && (0..self.height).all(|y| (0..self.width).all(|x| self.entry(x, y).is_some()))
    }
}

impl NoticeReport {
    /// The report with its first `shown` items, then the count of the
    /// rest when any are left out.
    fn lines(&self, shown: usize) -> String {
        let shown = shown.min(self.items.len());
        let left = self
            .items
            .len()
            .saturating_sub(shown)
            .saturating_add(self.more);
        let count = (left > 0).then(|| format!("{} {left}", self.label));
        self.happened
            .iter()
            .chain(self.items.iter().take(shown))
            .chain(&count)
            .chain([&self.next])
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The report with every item it holds.
    pub fn text(&self) -> String {
        self.lines(self.items.len())
    }
}

impl Notice {
    pub fn new(text: impl Into<String>) -> Notice {
        Notice {
            text: text.into(),
            report: None,
        }
    }

    /// The notice of `report`, which shows every item while they fit. On a
    /// display too small for all of it, the items go from the last, and the
    /// count after them grows by each one that goes, so what happened, the
    /// count and what to do next still show; only when those do not fit is
    /// the rest cut, as any notice's is.
    pub fn report(report: NoticeReport) -> Notice {
        Notice {
            text: report.text(),
            report: Some(Box::new(report)),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Draws the notice across the top of `display`'s view with `font`, its
    /// lines as many as fit and the rest cut, and returns the display area
    /// it covers. A line that crosses `clear`, a rectangle in the display's
    /// pixels such as the focus mark's cell, ends before it. `None`,
    /// writing nothing, when the display holds no line.
    pub fn draw<F: Face>(
        &self,
        font: &Font<F>,
        display: Surface,
        colors: Colors,
        clear: Option<Rect>,
        target: &mut impl Target,
    ) -> Option<Rect> {
        let placed = self.upright(font, display, colors, clear)?;
        Some(display.put(placed.at(), &placed.picture, target))
    }

    /// The notice as [`Notice::draw`] draws it, upright, in the text's view
    /// of the display: its picture and its place in the view, for the GPU
    /// path, which uploads it into the upright frame (ADR-1303 D3).
    pub fn upright<F: Face>(
        &self,
        font: &Font<F>,
        display: Surface,
        colors: Colors,
        clear: Option<Rect>,
    ) -> Option<Placed> {
        let cell = font.cell();
        let (cw, ch) = (cell.width.max(1), cell.height.max(1));
        let (vw, vh) = display.view();
        // an edge of an eighth of the cell, inside it half a cell across
        // and a quarter down
        let edge = (ch / 8).max(1);
        let inset = (edge.saturating_add(cw / 2), edge.saturating_add(ch / 4));
        let cols = vw
            .saturating_sub(inset.0.saturating_mul(2))
            .checked_div(cw)
            .unwrap_or(0);
        let room = vh
            .saturating_sub(inset.1.saturating_mul(2))
            .checked_div(ch)
            .unwrap_or(0);
        if cols == 0 || room == 0 {
            return None;
        }
        let room = usize::try_from(room).unwrap_or(usize::MAX);
        // a line crossing `clear` ends before it, so the mark drawn over
        // the box hides no text
        let clear = clear.map(|r| display.view_area(r));
        let cols_at = |i: usize| {
            let top = u32::try_from(i)
                .unwrap_or(u32::MAX)
                .saturating_mul(ch)
                .saturating_add(inset.1);
            match clear {
                Some(r)
                    if r.y < top.saturating_add(ch)
                        && r.y.saturating_add(r.height) > top
                        && r.x.saturating_add(r.width) > inset.0 =>
                {
                    cols.min(r.x.saturating_sub(inset.0).checked_div(cw).unwrap_or(0))
                }
                _ => cols,
            }
        };
        // a report's count, which shows only once an item goes, can take
        // characters its full text lacks
        // characters its full text lacks, and a drawing in text has its own
        let count = self
            .report
            .as_ref()
            .map(|r| format!("{} 0123456789", r.label))
            .unwrap_or_default();
        let drawn = self
            .report
            .as_ref()
            .and_then(|r| r.drawing)
            .map(|d| format!("{}{}", d.full, d.small))
            .unwrap_or_default();
        let glyphs = Glyphs::new(font, display.depth, &format!("{}{count}{drawn}", self.text));
        let width = |c| glyphs.width(c);
        let laid = match &self.report {
            Some(report) => arrange(report, Room { cols, rows: room }, (cw, ch), cols_at, width),
            None => Laid::text(wrap(&self.text, room, cols_at, width)),
        };
        let colors = colors.at(display.depth);
        let mut picture = Picture::new(
            vw,
            inset
                .1
                .saturating_mul(2)
                .saturating_add(laid.rows.max(1).saturating_mul(ch)),
            colors.paper,
        );
        picture.frame(edge, colors.edge);
        // the top left pixel of the cell `col`, `row` inside the box
        let at = |col: u32, row: u32| {
            (
                inset.0.saturating_add(col.saturating_mul(cw)),
                inset.1.saturating_add(row.saturating_mul(ch)),
            )
        };
        for (row, line) in (laid.text.1..).zip(&laid.lines) {
            let (mut x, y) = at(laid.text.0, row);
            for &(c, width) in line {
                if let Some(g) = glyphs.glyph(c) {
                    picture.glyph(x, y, g, colors);
                }
                x = x.saturating_add(cw.saturating_mul(width.cells()));
            }
        }
        if let Some(shown) = &laid.drawing {
            match shown.art {
                Art::Text(text) => {
                    for (row, line) in (0u32..).zip(text.lines()) {
                        for (col, c) in (0u32..).zip(line.chars()) {
                            if let Some(g) = glyphs.glyph(c).filter(|_| c != ' ') {
                                let (gx, gy) = at(
                                    shown.at.0.saturating_add(col),
                                    shown.at.1.saturating_add(row),
                                );
                                picture.glyph(gx, gy, g, colors);
                            }
                        }
                    }
                }
                #[cfg(feature = "images")]
                Art::Sprite(sprite, scale) => {
                    // centered in the cells it takes
                    let side = |cells: u32, cell: u32, px: u32| {
                        cells
                            .saturating_mul(cell)
                            .saturating_sub(px.saturating_mul(scale))
                            / 2
                    };
                    let (x, y) = at(shown.at.0, shown.at.1);
                    let x = x.saturating_add(side(shown.cells.0, cw, sprite.width));
                    let y = y.saturating_add(side(shown.cells.1, ch, sprite.height));
                    picture.sprite(x, y, &sprite, scale);
                }
            }
        }
        Some(Placed {
            x: 0,
            y: 0,
            picture,
        })
    }
}

/// A head's focus mark (ADR-1401 D5): a filled circle on the head that
/// holds the focus and a hollow one on every other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Focus,
    NoFocus,
}

impl Mark {
    /// The character it draws as.
    fn char(self) -> char {
        match self {
            Mark::Focus => '\u{25cf}',
            Mark::NoFocus => '\u{25cb}',
        }
    }

    /// Draws the mark in `cell`, a cell's rectangle in the display's
    /// pixels as the renderer placed the grid, with `font`, its circle in
    /// `colors.ink` on `colors.paper` and cut to the cell, and returns the
    /// display area it covers. A font that lacks the circle draws a square
    /// half the cell's shorter side, filled for [`Mark::Focus`] and hollow
    /// for [`Mark::NoFocus`], where its stand-in for a missing character
    /// would draw both alike. `None`, writing nothing, when the cell is
    /// empty or not wholly on the display.
    pub fn draw<F: Face>(
        self,
        font: &Font<F>,
        display: Surface,
        cell: Rect,
        colors: Colors,
        target: &mut impl Target,
    ) -> Option<Rect> {
        let placed = self.upright(font, display, cell, colors)?;
        Some(display.put(placed.at(), &placed.picture, target))
    }

    /// The mark as [`Mark::draw`] draws it, upright, in the text's view of
    /// the display: its picture and its place in the view, for the GPU path
    /// (ADR-1303 D3).
    pub fn upright<F: Face>(
        self,
        font: &Font<F>,
        display: Surface,
        cell: Rect,
        colors: Colors,
    ) -> Option<Placed> {
        let (vw, vh) = display.view();
        let view = display.view_area(cell);
        if view.width == 0
            || view.height == 0
            || view.x.saturating_add(view.width) > vw
            || view.y.saturating_add(view.height) > vh
        {
            return None;
        }
        let c = self.char();
        let colors = colors.at(display.depth);
        let mut picture = Picture::new(view.width, view.height, colors.paper);
        match font
            .has_glyph(c)
            .then(|| standin(font, display.depth, c, Width::Narrow))
            .flatten()
        {
            Some(g) => picture.glyph(0, 0, &g, colors),
            None => picture.square(self == Mark::Focus, colors.ink),
        }
        Some(Placed {
            x: view.x,
            y: view.y,
            picture,
        })
    }
}

/// `c`'s glyph `width` cells wide in `font`, as a cell at `depth` draws
/// it: with the stand-in [`Fallback`] gives a color glyph, and at 1 bit a
/// sprite of partial coverage.
fn standin<F: Face>(font: &Font<F>, depth: Depth, c: char, width: Width) -> Option<Glyph> {
    let g = font.glyph(&[c], width, Style::default())?;
    Some(
        Fallback::new(depth, font.cell())
            .and_then(|f| f.glyph(&g, 1, &[c], width, Variant::default()))
            .unwrap_or(g),
    )
}

/// The glyphs of a notice's characters, each drawn once however often it
/// appears, with the cells it takes: its face's advance for it, which the
/// face gives without drawing the glyph, and one cell for a character the
/// face lacks.
struct Glyphs {
    /// Sorted by character.
    glyphs: Vec<(char, Width, Option<Glyph>)>,
}

impl Glyphs {
    fn new<F: Face>(font: &Font<F>, depth: Depth, text: &str) -> Glyphs {
        let mut chars: Vec<char> = text.chars().filter(|&c| c != '\n').collect();
        chars.sort_unstable();
        chars.dedup();
        let glyphs = chars
            .into_iter()
            .map(|c| {
                let width = font.face().advance(c).unwrap_or(Width::Narrow);
                (c, width, standin(font, depth, c, width))
            })
            .collect();
        Glyphs { glyphs }
    }

    fn get(&self, c: char) -> Option<&(char, Width, Option<Glyph>)> {
        let i = self.glyphs.binary_search_by_key(&c, |g| g.0).ok()?;
        self.glyphs.get(i)
    }

    fn width(&self, c: char) -> Width {
        self.get(c).map_or(Width::Narrow, |g| g.1)
    }

    fn glyph(&self, c: char) -> Option<&Glyph> {
        self.get(c).and_then(|g| g.2.as_ref())
    }
}

/// `text`'s first `lines` lines, line `i` wrapped to `cols(i)` cells by
/// `width`: broken at its last space that fits, or else after the last
/// character that fits, at least one a line, and the spaces at a break
/// dropped. A line of no cells is left empty.
fn wrap(
    text: &str,
    lines: usize,
    cols: impl Fn(usize) -> u32,
    width: impl Fn(char) -> Width,
) -> Vec<Vec<(char, Width)>> {
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        let chars: Vec<(char, Width)> = paragraph.chars().map(|c| (c, width(c))).collect();
        let mut rest = chars.as_slice();
        loop {
            if out.len() >= lines {
                return out;
            }
            let cols = cols(out.len());
            if cols == 0 {
                out.push(Vec::new());
                if rest.is_empty() {
                    break;
                }
                continue;
            }
            let mut used = 0u32;
            let fit = rest
                .iter()
                .take_while(|(_, w)| {
                    used = used.saturating_add(w.cells());
                    used <= cols
                })
                .count();
            if fit == rest.len() {
                out.push(rest.to_vec());
                break;
            }
            let fit = fit.max(1);
            let (line, next) = match rest.get(..=fit).and_then(|head| {
                head.iter()
                    .rposition(|&(c, _)| c == ' ')
                    .filter(|&at| at > 0)
            }) {
                Some(at) => (rest.get(..at), rest.get(at..)),
                None => (rest.get(..fit), rest.get(fit..)),
            };
            let space = |&(c, _): &(char, Width)| c == ' ';
            let line = line.unwrap_or_default();
            let kept = line
                .len()
                .saturating_sub(line.iter().rev().take_while(|p| space(p)).count());
            out.push(line.get(..kept).unwrap_or_default().to_vec());
            rest = next.unwrap_or_default();
            rest = rest
                .get(rest.iter().take_while(|p| space(p)).count()..)
                .unwrap_or_default();
            if rest.is_empty() {
                break;
            }
        }
    }
    out
}

/// How many of `report`'s items show on a display of `room` lines
/// (ADR-1401 D7): every item while they fit, else as many as fit from the
/// first, with the count of the rest. Only when no item fits beside what
/// happened, the count and the next step is the rest cut, as [`wrap`]
/// cuts.
fn kept(
    report: &NoticeReport,
    room: usize,
    cols: impl Fn(usize) -> u32,
    width: impl Fn(char) -> Width,
) -> usize {
    // one line past the room shows whether a choice fits
    let over = room.saturating_add(1);
    (0..=report.items.len())
        .rev()
        .find(|&n| wrap(&report.lines(n), over, &cols, &width).len() <= room)
        .unwrap_or(0)
}

/// The cells inside a notice's box: its columns, and the lines it holds.
#[derive(Clone, Copy, Debug)]
struct Room {
    cols: u32,
    rows: usize,
}

/// Where a notice's parts go, in cells inside its box.
#[derive(Debug)]
struct Laid {
    lines: Vec<Vec<(char, Width)>>,
    /// The column and the row of the text's first line.
    text: (u32, u32),
    drawing: Option<Shown>,
    /// The rows the box holds.
    rows: u32,
}

impl Laid {
    /// `lines`, alone from the top left.
    fn text(lines: Vec<Vec<(char, Width)>>) -> Laid {
        Laid {
            rows: u32::try_from(lines.len()).unwrap_or(u32::MAX),
            lines,
            text: (0, 0),
            drawing: None,
        }
    }
}

/// A report's drawing as the notice shows it: what it draws, the cells it
/// takes and its top left cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shown {
    art: Art,
    cells: (u32, u32),
    at: (u32, u32),
}

/// What a drawing draws: lines of text, or a sprite at a whole-number
/// scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Art {
    Text(&'static str),
    #[cfg(feature = "images")]
    Sprite(Sprite, u32),
}

impl Drawing {
    /// The full drawing, or the small one, with the cells it takes in a
    /// font of `cell`: its text's longest line and its lines, or the
    /// scale and the pixels, in whole cells, of the sprite drawn in its
    /// place. `None` for a sprite that
    /// does not fit those cells at its own size.
    fn art(&self, full: bool, cell: (u32, u32)) -> Option<(Art, (u32, u32))> {
        let text = if full { self.full } else { self.small };
        let cols = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
        let cells = (
            u32::try_from(cols).unwrap_or(u32::MAX),
            u32::try_from(text.lines().count()).unwrap_or(u32::MAX),
        );
        #[cfg(feature = "images")]
        if let Some((whole_sprite, small_sprite)) = self.sprite {
            let sprite = if full { whole_sprite } else { small_sprite };
            let fit = |cells: u32, cell: u32, px: u32| {
                cells.saturating_mul(cell).checked_div(px).unwrap_or(0)
            };
            let scale = fit(cells.0, cell.0, sprite.width).min(fit(cells.1, cell.1, sprite.height));
            if scale == 0 {
                return None;
            }
            let whole = |px: u32, cell: u32| px.saturating_mul(scale).div_ceil(cell.max(1));
            return Some((
                Art::Sprite(sprite, scale),
                (whole(sprite.width, cell.0), whole(sprite.height, cell.1)),
            ));
        }
        #[cfg(not(feature = "images"))]
        let _ = cell;
        Some((Art::Text(text), cells))
    }
}

/// `report` laid out in `room`, in a font of `cell` whose characters are
/// `width` cells wide, line `i` of the box holding `cols(i)` columns from
/// its left: the items [`kept`] keeps, with its drawing where the
/// text still shows each line it shows alone (ADR-1401 D7). The full
/// drawing goes to the left of the text where [`TEXT_BESIDE_MIN`] columns
/// are left beside it, else the full or the small one centered above it,
/// else none.
fn arrange(
    report: &NoticeReport,
    room: Room,
    cell: (u32, u32),
    cols: impl Fn(usize) -> u32,
    width: impl Fn(char) -> Width,
) -> Laid {
    let kept = kept(report, room.rows, &cols, &width);
    let text = report.lines(kept);
    let alone = || Laid::text(wrap(&text, room.rows, &cols, &width));
    let Some(drawing) = report.drawing else {
        return alone();
    };
    let row = |r: u32| usize::try_from(r).unwrap_or(usize::MAX);
    // the text's lines from row `top` and column `left`, when every one
    // fits below the box's last row
    let whole = |top: u32, left: u32| {
        let room_left = room.rows.checked_sub(row(top))?;
        let lines = wrap(
            &text,
            room_left.saturating_add(1),
            |i| cols(i.saturating_add(row(top))).saturating_sub(left),
            &width,
        );
        (lines.len() <= room_left).then_some(lines)
    };
    // every row of a drawing at column `x`, `w` wide and `h` tall, clear
    // of what the box keeps its text clear of
    let clear = |x: u32, (w, h): (u32, u32)| {
        row(h) <= room.rows && (0..row(h)).all(|r| cols(r) >= x.saturating_add(w))
    };
    let rows = |lines: &Vec<_>| u32::try_from(Vec::len(lines)).unwrap_or(u32::MAX);
    if let Some((art, cells)) = drawing.art(true, cell) {
        let left = cells.0.saturating_add(DRAWING_GAP);
        if room.cols.saturating_sub(left) >= TEXT_BESIDE_MIN
            && clear(0, cells)
            && let Some(lines) = whole(0, left)
        {
            return Laid {
                rows: rows(&lines).max(cells.1),
                lines,
                text: (left, 0),
                drawing: Some(Shown {
                    art,
                    cells,
                    at: (0, 0),
                }),
            };
        }
    }
    for full in [true, false] {
        let Some((art, cells)) = drawing.art(full, cell) else {
            continue;
        };
        let x = room.cols.saturating_sub(cells.0) / 2;
        let top = cells.1.saturating_add(1);
        if cells.0 <= room.cols
            && clear(x, cells)
            && let Some(lines) = whole(top, 0)
        {
            return Laid {
                rows: top.saturating_add(rows(&lines)),
                lines,
                text: (0, top),
                drawing: Some(Shown {
                    art,
                    cells,
                    at: (x, 0),
                }),
            };
        }
    }
    alone()
}

/// What the console draws, upright, at its place in the text's view of
/// the display: what the GPU path uploads into the upright frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    /// Its top left in the view.
    pub x: u32,
    pub y: u32,
    pub picture: Picture,
}

impl Placed {
    fn at(&self) -> (u32, u32) {
        (self.x, self.y)
    }

    /// The view's area it covers.
    pub fn rect(&self) -> Rect {
        Rect {
            x: self.x,
            y: self.y,
            width: self.picture.width,
            height: self.picture.height,
        }
    }

    /// The part `r` of its picture, in its own pixels, as the bytes of an
    /// RGBA8 texture, opaque, and their stride.
    #[cfg(feature = "gpu")]
    pub(crate) fn rgba(&self, r: Rect) -> (Vec<u8>, usize) {
        let mut out = Vec::with_capacity(usize::try_from(r.area()).unwrap_or(0).saturating_mul(4));
        for y in r.y..r.y.saturating_add(r.height) {
            for x in r.x..r.x.saturating_add(r.width) {
                let [_, red, green, blue] = self.picture.get(x, y).to_be_bytes();
                out.extend_from_slice(&[red, green, blue, 255]);
            }
        }
        (out, usize::try_from(r.width).unwrap_or(0).saturating_mul(4))
    }
}

/// The view's pixels of what the console draws, row by row, each
/// `0x00RRGGBB` and opaque.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl Picture {
    /// A picture `width` by `height` of `pixels`, row by row, each
    /// `0x00RRGGBB`: what the console drew, read back from a buffer, as
    /// tier 2 reads its overlay. `None` when their count is not its area.
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<u32>) -> Option<Picture> {
        let area = u64::from(width).saturating_mul(u64::from(height));
        (u64::try_from(pixels.len()).ok() == Some(area)).then_some(Picture {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Every pixel, row by row.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    fn new(width: u32, height: u32, fill: Rgb) -> Picture {
        let n = u64::from(width).saturating_mul(u64::from(height));
        Picture {
            width,
            height,
            pixels: vec![pixel(fill); usize::try_from(n).unwrap_or(0)],
        }
    }

    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = u64::from(y)
            .saturating_mul(u64::from(self.width))
            .saturating_add(u64::from(x));
        usize::try_from(i).ok()
    }

    fn get(&self, x: u32, y: u32) -> u32 {
        self.index(x, y)
            .and_then(|i| self.pixels.get(i))
            .copied()
            .unwrap_or(0)
    }

    fn set(&mut self, x: u32, y: u32, p: u32) {
        if let Some(px) = self.index(x, y).and_then(|i| self.pixels.get_mut(i)) {
            *px = p;
        }
    }

    /// An edge `thick` pixels wide around it, in `color`.
    fn frame(&mut self, thick: u32, color: Rgb) {
        let p = pixel(color);
        for y in 0..self.height {
            for x in 0..self.width {
                let inside = x >= thick
                    && y >= thick
                    && x < self.width.saturating_sub(thick)
                    && y < self.height.saturating_sub(thick);
                if !inside {
                    self.set(x, y, p);
                }
            }
        }
    }

    /// A square half its shorter side, centred, in `color`: filled, or an
    /// edge an eighth of that side thick, at least a pixel.
    fn square(&mut self, filled: bool, color: Rgb) {
        let side = self.width.min(self.height) / 2;
        let x0 = self.width.saturating_sub(side) / 2;
        let y0 = self.height.saturating_sub(side) / 2;
        let thick = (side / 8).max(1);
        let p = pixel(color);
        for y in 0..side {
            for x in 0..side {
                let edge = x < thick
                    || y < thick
                    || x >= side.saturating_sub(thick)
                    || y >= side.saturating_sub(thick);
                if filled || edge {
                    self.set(x0.saturating_add(x), y0.saturating_add(y), p);
                }
            }
        }
    }

    /// `sprite` at `x`, `y`, each of its pixels a square `scale` pixels
    /// on a side; a pixel that names no color of its palette leaves the
    /// square as it was.
    #[cfg(feature = "images")]
    fn sprite(&mut self, x: u32, y: u32, sprite: &Sprite, scale: u32) {
        for sy in 0..sprite.height {
            for sx in 0..sprite.width {
                let Some(color) = sprite.get(sx, sy) else {
                    continue;
                };
                let p = pixel(color);
                let (x, y) = (
                    x.saturating_add(sx.saturating_mul(scale)),
                    y.saturating_add(sy.saturating_mul(scale)),
                );
                for dy in 0..scale {
                    for dx in 0..scale {
                        self.set(x.saturating_add(dx), y.saturating_add(dy), p);
                    }
                }
            }
        }
    }

    /// `g` at `x`, `y`, in `colors.ink` on `colors.paper`.
    fn glyph(&mut self, x: u32, y: u32, g: &Glyph, colors: Colors) {
        for gy in 0..g.height() {
            for gx in 0..g.width() {
                let p = match (g.format(), g.pixel(gx, gy)) {
                    (Format::Mask, Some(&[a])) => mask(colors.ink, colors.paper, a),
                    (Format::Rgba, Some(&[r, gr, b, a])) => {
                        over_bg(colors.paper, u32::from_be_bytes([a, r, gr, b]))
                    }
                    _ => continue,
                };
                self.set(x.saturating_add(gx), y.saturating_add(gy), p);
            }
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::{Canvas, Config, DrawState, Renderer};
    use vtium_font::set::{SetFace, builtin};
    use vtium_term::{Age, Cursor, Damage, Frame, Glyph, Line, LineId, Palette};

    const FIGURE: &str =
        "vtium: heads.conf:5: rule names undefined head \"desc\"; running the shipped rules";

    fn font(name: &str, height: u32) -> Font<SetFace> {
        let set = builtin(name).expect("a built-in set");
        Font::new(SetFace::new(set, height).expect("a cell"))
    }

    fn display(width: u32, height: u32) -> Surface {
        Surface {
            width,
            height,
            orientation: Orientation::Normal,
            depth: Depth::Color,
        }
    }

    /// Rows of `cols` cells of `c`.
    fn lines(cols: usize, rows: usize, c: char) -> Vec<Line> {
        (0..rows)
            .map(|r| {
                let mut line = Line::new(LineId::new(r as u64 + 1), cols);
                for col in 0..cols {
                    line.write(
                        col,
                        Glyph::Char(c),
                        vtium_term::Width::Narrow,
                        &vtium_term::Style::DEFAULT,
                        None,
                    );
                }
                line
            })
            .collect()
    }

    fn frame<'a>(
        rows: &'a [Line],
        damage: &'a Damage,
        palette: &'a Palette,
        since: Age,
    ) -> Frame<'a> {
        Frame::new(
            rows.iter().collect(),
            damage,
            palette,
            since,
            Cursor::default(),
            false,
        )
    }

    fn palette() -> Palette {
        Palette::new(
            [Rgb::default(); 256],
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
        )
    }

    // ADR-1401 D7: the notice is vtium's own, drawn over the display's
    // frame and never through the terminal's cells
    #[test]
    fn a_notice_draws_above_the_grid_and_leaves_the_terminals_cells_unchanged() {
        let (w, h) = (320, 240);
        let rows = lines(40, 30, 'x');
        let before = rows.clone();
        let mut damage = Damage::new(40, 30);
        let palette = palette();
        let mut r = Renderer::new(font("unscii-8", 8), w, h, Config::default());
        let mut canvas = Canvas::new(w, h, 0);
        r.draw(
            &frame(&rows, &damage, &palette, Age::NEVER),
            &mut canvas,
            0,
            &DrawState::default(),
        );
        let since = damage.checkpoint();
        let grid = canvas.clone();

        let area = Notice::new(FIGURE)
            .draw(
                &font("unscii-8", 8),
                display(w, h),
                Colors::NOTICE,
                None,
                &mut canvas,
            )
            .expect("a notice fits 320x240");
        assert_eq!((area.x, area.y, area.width), (0, 0, w));
        assert!(area.height > 0 && area.height < h, "{area:?}");
        let c = Colors::NOTICE;
        let drawn = [pixel(c.ink), pixel(c.paper), pixel(c.edge)];
        for y in 0..h {
            for x in 0..w {
                let now = canvas.pixel(x, y).expect("inside");
                if y < area.height {
                    assert!(drawn.contains(&now), "the box at {x},{y}: {now:06x}");
                } else {
                    assert_eq!(Some(now), grid.pixel(x, y), "outside the box at {x},{y}");
                }
            }
        }
        assert_eq!(canvas.pixel(0, 0), Some(pixel(c.edge)));
        assert_eq!(rows, before, "the terminal's cells");
        // the renderer's own record of the cells is untouched: a frame
        // that changes nothing draws nothing, and the box stays
        let shown = canvas.clone();
        let drawn = r.draw(
            &frame(&rows, &damage, &palette, since),
            &mut canvas,
            1,
            &DrawState::default(),
        );
        assert!(drawn.damage.is_empty(), "{:?}", drawn.damage);
        assert_eq!(canvas, shown);
    }

    fn text(lines: &[Vec<(char, Width)>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|&(c, _)| c).collect())
            .collect()
    }

    fn narrow(_: char) -> Width {
        Width::Narrow
    }

    #[test]
    fn a_long_message_wraps_at_its_spaces_to_the_box() {
        // 38 cells: 320 pixels of 8, less the edge and half a cell each side
        assert_eq!(
            text(&wrap(FIGURE, usize::MAX, |_| 38, narrow)),
            [
                "vtium: heads.conf:5: rule names",
                "undefined head \"desc\"; running the",
                "shipped rules",
            ]
        );
        assert_eq!(
            text(&wrap("one\n\ntwo", usize::MAX, |_| 8, narrow)),
            ["one", "", "two"]
        );
        // a continuation line starts at its first word: a run of spaces at
        // the break goes with it
        assert_eq!(
            text(&wrap("abcd  efgh", usize::MAX, |_| 4, narrow)),
            ["abcd", "efgh"]
        );
        assert_eq!(
            text(&wrap("ab   cd", usize::MAX, |_| 3, narrow)),
            ["ab", "cd"]
        );
        // a break on the last space leaves no empty line after it
        assert_eq!(
            text(&wrap("abcd efgh ", usize::MAX, |_| 4, narrow)),
            ["abcd", "efgh"]
        );
    }

    #[test]
    fn a_word_longer_than_the_box_breaks_inside_it() {
        assert_eq!(
            text(&wrap("abcdefghij kl", usize::MAX, |_| 4, narrow)),
            ["abcd", "efgh", "ij", "kl"]
        );
        // a wide character that does not fit goes to the next line, and one
        // wider than the box takes a line of its own
        let wide = |c: char| {
            if c == '\u{6f22}' {
                Width::Wide
            } else {
                Width::Narrow
            }
        };
        assert_eq!(
            text(&wrap("ab\u{6f22}", usize::MAX, |_| 3, wide)),
            ["ab", "\u{6f22}"]
        );
        assert_eq!(
            text(&wrap("\u{6f22}a", usize::MAX, |_| 1, wide)),
            ["\u{6f22}", "a"]
        );
    }

    /// The pixels of the `w` by `h` block at `x`, `y`.
    fn block(c: &Canvas, x: u32, y: u32, w: u32, h: u32) -> Vec<u32> {
        (y..y + h)
            .flat_map(|y| (x..x + w).map(move |x| (x, y)))
            .map(|(x, y)| c.pixel(x, y).expect("inside"))
            .collect()
    }

    // legible down to minspec's floor: each line in the built-in font's
    // glyphs, one a cell, in the box's colors
    #[test]
    fn the_notice_draws_each_line_in_the_fonts_glyphs() {
        for (w, h, name, ch, lines) in [(320, 240, "unscii-8", 8, 3), (640, 480, "unifont", 16, 2)]
        {
            let font = font(name, ch);
            let mut canvas = Canvas::new(w, h, 0);
            let area = Notice::new(FIGURE)
                .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
                .expect("it fits");
            let (edge, cw) = ((ch / 8).max(1), 8);
            let inset = (edge + cw / 2, edge + ch / 4);
            assert_eq!(area.height, 2 * inset.1 + lines * ch, "{name}");
            let cols = (w - 2 * inset.0) / cw;
            let c = Colors::NOTICE;
            for (row, line) in wrap(FIGURE, usize::MAX, |_| cols, narrow)
                .iter()
                .enumerate()
            {
                for (col, &(ch_, _)) in line.iter().enumerate() {
                    let g = font
                        .glyph(&[ch_], Width::Narrow, Style::default())
                        .expect("a glyph");
                    let want: Vec<u32> = g
                        .pixels()
                        .iter()
                        .map(|&a| mask(c.ink, c.paper, a))
                        .collect();
                    let (x, y) = (inset.0 + col as u32 * cw, inset.1 + row as u32 * ch);
                    assert_eq!(block(&canvas, x, y, cw, ch), want, "{name} {ch_:?}");
                }
            }
        }
    }

    #[test]
    fn a_notice_taller_than_the_display_is_cut_to_whole_lines() {
        let mut canvas = Canvas::new(320, 240, 0);
        let area = Notice::new("x\n".repeat(100))
            .draw(
                &font("unscii-8", 8),
                display(320, 240),
                Colors::NOTICE,
                None,
                &mut canvas,
            )
            .expect("it fits");
        // 29 lines of 8 between the edges and the quarter-cell insets
        assert_eq!(area.height, 6 + 29 * 8);
        // a display that holds no line draws nothing
        let mut small = Canvas::new(16, 8, 7);
        let none = Notice::new("x").draw(
            &font("unscii-8", 8),
            display(16, 8),
            Colors::NOTICE,
            None,
            &mut small,
        );
        assert_eq!(none, None);
        assert!(small.pixels().iter().all(|&p| p == 7));
    }

    fn report(happened: &[&str], items: &[&str], more: usize) -> NoticeReport {
        NoticeReport {
            happened: happened.iter().map(|s| (*s).to_owned()).collect(),
            items: items.iter().map(|s| (*s).to_owned()).collect(),
            more,
            label: "More:".to_owned(),
            next: "next step".to_owned(),
            drawing: None,
        }
    }

    /// `report` laid out in `cols` columns and `rows` lines of a font of
    /// 8x16 cells whose characters are each one cell wide.
    fn laid(report: &NoticeReport, cols: u32, rows: usize) -> Laid {
        arrange(report, Room { cols, rows }, (8, 16), |_| cols, narrow)
    }

    /// A stand-in for the panic screen's drawing, the sizes of the
    /// maintainer's: a full one of 32 columns by 20 rows, and a small one of
    /// 21 by 10.
    fn drawing() -> Drawing {
        let art = |cols: usize, rows: usize, c: char| -> &'static str {
            let line: String = std::iter::repeat_n(c, cols).collect();
            String::leak(format!("{line}\n").repeat(rows))
        };
        Drawing::text(art(32, 20, 'F'), art(21, 10, 'S'))
    }

    /// The panic screen's text, in the console's words, with its file's
    /// first `errors` of three.
    fn panic_report(errors: usize, drawing: Option<Drawing>) -> NoticeReport {
        let all = [
            "/etc/vtium/vtium.conf:4: This section has no key named \"focs\". Check the spelling, or remove the line.",
            "/etc/vtium/vtium.conf:5: This section has no key named \"font-weight\". Check the spelling, or remove the line.",
            "/etc/vtium/vtium.conf:7: This rule sends devices to head \"k\\u{202e}x\", but the file has no head of that name. Correct the name, or add a `[head NAME]` section for it.",
        ];
        NoticeReport {
            happened: vec![
                "vtium.conf has errors and counts as closed: all input is off.".to_owned(),
                "No head can run because of the errors in /etc/vtium/vtium.conf, listed below."
                    .to_owned(),
            ],
            items: all.iter().take(errors).map(|s| (*s).to_owned()).collect(),
            more: 0,
            label: "More errors:".to_owned(),
            next: "This console takes no input. For the full report, run vtium check-config /etc/vtium/vtium.conf over ssh or from other boot media.".to_owned(),
            drawing,
        }
    }

    /// The report's whole text wrapped to `cols` columns: every line it
    /// shows with no drawing and no limit on its lines.
    fn whole(report: &NoticeReport, cols: u32) -> Vec<String> {
        text(&wrap(&report.text(), usize::MAX, |_| cols, narrow))
    }

    // ADR-1401 D7: on an 80x25 screen, 78 columns by 24 lines inside the
    // box, the full drawing goes to the left of the text, which keeps 44
    // columns beside it and shows every line, its three errors among them
    #[test]
    fn the_full_drawing_goes_left_of_the_whole_text_on_80x25() {
        let report = panic_report(3, Some(drawing()));
        let got = laid(&report, 78, 24);
        let full = drawing().full;
        assert_eq!(
            got.drawing,
            Some(Shown {
                art: Art::Text(full),
                cells: (32, 20),
                at: (0, 0),
            })
        );
        assert_eq!(got.text, (34, 0));
        assert_eq!(text(&got.lines), whole(&report, 44));
        assert_eq!(got.rows, 20.max(got.lines.len() as u32));
        // with 39 columns left beside it, the full drawing goes above the
        // text instead where it fits, and else the small one
        let narrower = arrange(
            &report,
            Room { cols: 73, rows: 24 },
            (8, 16),
            |_| 73,
            narrow,
        );
        assert_ne!(narrower.text.0, 34, "{narrower:?}");
        assert!(
            narrower
                .drawing
                .is_none_or(|d| d.at.1 == 0 && narrower.text.0 == 0)
        );
    }

    // ADR-1401 D7: where the full drawing fits neither beside the text nor
    // above it, the small one goes centered above the text, an empty row
    // between them, as on a 40x25 screen: 38 columns by 24 lines
    #[test]
    fn the_small_drawing_goes_centered_above_the_text_on_40x25() {
        let report = panic_report(1, Some(drawing()));
        let got = laid(&report, 38, 24);
        assert_eq!(
            got.drawing,
            Some(Shown {
                art: Art::Text(drawing().small),
                cells: (21, 10),
                at: (8, 0),
            })
        );
        assert_eq!(got.text, (0, 11));
        assert_eq!(text(&got.lines), whole(&report, 38));
        assert_eq!(got.rows, 11 + got.lines.len() as u32);
        // a screen tall enough takes the full drawing above the text
        let tall = laid(&report, 38, 48);
        assert_eq!(
            tall.drawing.map(|d| (d.cells, d.at)),
            Some(((32, 20), (3, 0)))
        );
        assert_eq!(tall.text, (0, 21));
        assert_eq!(text(&tall.lines), whole(&report, 38));
    }

    // ADR-1401 D7: the text comes first. On a 40x25 screen the three
    // errors leave no room for even the small drawing, and on 40x15,
    // minspec's 320x240 screen in an 8x16 font, one error leaves none:
    // each shows the text alone, exactly as a report with no drawing
    #[test]
    fn the_text_shows_alone_where_no_drawing_fits_beside_it() {
        for (errors, cols, rows) in [(3, 38, 24), (1, 38, 14), (3, 38, 14)] {
            let with = panic_report(errors, Some(drawing()));
            let without = panic_report(errors, None);
            let got = laid(&with, cols, rows);
            assert_eq!(got.drawing, None, "{errors} errors on {cols}x{rows}");
            assert_eq!(got.text, (0, 0));
            assert_eq!(text(&got.lines), text(&laid(&without, cols, rows).lines));
        }
        // where all of the text fits, all of it shows
        assert_eq!(
            text(&laid(&panic_report(3, Some(drawing())), 38, 24).lines),
            whole(&panic_report(3, None), 38)
        );
    }

    /// A stand-in sprite 80 by 80 in two colors: every pixel on its
    /// diagonal red, and the rest blue.
    #[cfg(feature = "images")]
    fn sprite() -> Sprite {
        static PALETTE: [Rgb; 2] = [Rgb::new(0, 0, 0xff), Rgb::new(0xff, 0, 0)];
        let samples: Vec<u8> = (0..80)
            .flat_map(|y| {
                (0..80).flat_map(move |x| if x == y { [0xff, 0, 0] } else { [0, 0, 0xff] })
            })
            .collect();
        Sprite::new(80, 80, &PALETTE, samples.leak())
    }

    /// The stand-in sprite cropped as the panic screen's is for the small
    /// drawing's place: 50 by 44 pixels from its pixel at 15, 6.
    #[cfg(feature = "images")]
    fn monitor() -> Sprite {
        sprite().crop(15, 6, 50, 44)
    }

    // ADR-1401 D7: a build with image support draws the sprite in the
    // place of the drawing in text, at the largest whole-number scale that
    // fits the cells the text would take: in 8×16 cells, the whole sprite
    // at three times its size in the full drawing's 32 by 20 cells, 30 by
    // 15 cells, and the 50×44 crop at three times its size in the small
    // one's 21 by 10, 19 by 9 cells
    #[cfg(feature = "images")]
    #[test]
    fn the_sprite_takes_the_drawings_place_at_a_whole_scale() {
        let (sprite, monitor) = (sprite(), monitor());
        assert!(sprite.is_whole());
        assert!(monitor.is_whole());
        assert_eq!(monitor.size(), (50, 44));
        let with = Some(drawing().with_sprite(sprite, monitor));
        let report = panic_report(3, with);
        let got = laid(&report, 78, 24);
        assert_eq!(
            got.drawing,
            Some(Shown {
                art: Art::Sprite(sprite, 3),
                cells: (30, 15),
                at: (0, 0),
            })
        );
        assert_eq!(got.text, (32, 0));
        assert_eq!(text(&got.lines), whole(&report, 46));
        let one = panic_report(1, with);
        let got = laid(&one, 38, 24);
        assert_eq!(
            got.drawing,
            Some(Shown {
                art: Art::Sprite(monitor, 3),
                cells: (19, 9),
                at: (9, 0),
            })
        );
        assert_eq!(got.text, (0, 10));
        // too small for even the small one, the text shows alone
        assert_eq!(laid(&one, 38, 14).drawing, None);
        // in 4x6 cells the small drawing's 84x60 pixels cannot hold the
        // sprite at its own size, and the full one's 128x120 hold it once
        let tiny = arrange(
            &one,
            Room {
                cols: 200,
                rows: 100,
            },
            (4, 6),
            |_| 200,
            narrow,
        );
        assert_eq!(
            tiny.drawing.map(|d| (d.art, d.cells)),
            Some((Art::Sprite(sprite, 1), (20, 14)))
        );
        let none = Drawing::text("", "").with_sprite(sprite, monitor);
        assert_eq!(none.art(false, (8, 16)), None);
    }

    // a crop is the rectangle of its sprite's pixels it names, and a
    // crop that runs past the sprite's edge is cut there
    #[cfg(feature = "images")]
    #[test]
    fn a_crop_draws_its_rectangle_of_the_sprite() {
        let (sprite, monitor) = (sprite(), monitor());
        for (x, y) in [(0, 0), (49, 0), (0, 43), (49, 43), (0, 9), (34, 43)] {
            assert_eq!(monitor.get(x, y), sprite.get(x + 15, y + 6), "{x},{y}");
        }
        assert_eq!(monitor.get(50, 0), None);
        assert_eq!(monitor.get(0, 44), None);
        let past = sprite.crop(70, 75, 20, 20);
        assert_eq!(past.size(), (10, 5));
        assert!(past.is_whole());
        assert_eq!(past.get(9, 4), sprite.get(79, 79));
        assert_eq!(sprite.crop(80, 0, 4, 4).size(), (0, 4));
        assert_eq!(monitor.crop(1, 2, 3, 4).get(0, 0), sprite.get(16, 8));
    }

    // ADR-1401 D7: on 40×25 with one error, the small drawing's place
    // draws the sprite cropped to its monitor, each pixel a 3×3 square,
    // centered in its 19 by 9 cells; on 80×25 the full one's place draws
    // the whole sprite (the test after this one)
    #[cfg(feature = "images")]
    #[test]
    fn the_small_place_draws_the_cropped_sprite() {
        let (w, h) = (320, 400);
        let font = font("unifont", 16);
        let mut canvas = Canvas::new(w, h, 0);
        let (sprite, monitor) = (sprite(), monitor());
        Notice::report(panic_report(
            1,
            Some(drawing().with_sprite(sprite, monitor)),
        ))
        .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
        .expect("it fits");
        // the box's inset of 6 pixels, then column 9 of 8 pixels; 19 by 9
        // cells hold 152×144 pixels, so 1 pixel across and 6 down
        let (x0, y0) = (6 + 9 * 8 + 1, 6 + 6);
        // the stand-in's diagonal crosses the crop at 0,9, which the whole
        // sprite has blue
        assert_ne!(monitor.get(0, 9), sprite.get(0, 9));
        for (sx, sy) in [(0, 0), (0, 9), (1, 10), (34, 43), (49, 0), (49, 43)] {
            let want = pixel(monitor.get(sx, sy).expect("a pixel"));
            assert_eq!(
                block(&canvas, x0 + sx * 3, y0 + sy * 3, 3, 3),
                vec![want; 9],
                "{sx},{sy}"
            );
        }
        // the paper around it is the box's
        for (x, y) in [(x0 - 1, y0), (x0 + 150, y0), (x0, y0 - 1), (x0, y0 + 132)] {
            assert_eq!(
                canvas.pixel(x, y),
                Some(pixel(Colors::NOTICE.paper)),
                "{x},{y}"
            );
        }
    }

    // ADR-1401 D7: each pixel of the sprite is a square of the scale's
    // side, so it stays sharp, and the sprite is centered in its cells
    #[cfg(feature = "images")]
    #[test]
    fn the_notice_draws_each_sprite_pixel_as_a_whole_square() {
        let (w, h) = (640, 400);
        let font = font("unifont", 16);
        let mut canvas = Canvas::new(w, h, 0);
        let sprite = sprite();
        Notice::report(panic_report(
            3,
            Some(drawing().with_sprite(sprite, monitor())),
        ))
        .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
        .expect("it fits");
        // 30 by 15 cells of 8x16 hold 240x240 pixels: no space across,
        // and 0 down
        let (x0, y0) = (2 + 4, 2 + 4);
        for (sx, sy) in [(0, 0), (1, 0), (0, 1), (40, 40), (79, 79), (79, 0)] {
            let want = pixel(sprite.get(sx, sy).expect("a pixel"));
            assert_eq!(
                block(&canvas, x0 + sx * 3, y0 + sy * 3, 3, 3),
                vec![want; 9],
                "{sx},{sy}"
            );
        }
        // the paper around it is the box's
        assert_eq!(
            canvas.pixel(x0 + 240, y0),
            Some(pixel(Colors::NOTICE.paper))
        );
        assert_eq!(
            canvas.pixel(x0, y0 + 240),
            Some(pixel(Colors::NOTICE.paper))
        );
    }

    // a sprite whose samples end early, or whose pixel matches no entry
    // of its palette, draws nothing there and is not whole
    #[cfg(feature = "images")]
    #[test]
    fn a_sprite_draws_only_what_its_palette_names() {
        static PALETTE: [Rgb; 1] = [Rgb::new(1, 2, 3)];
        let short = Sprite::new(2, 2, &PALETTE, &[1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2]);
        assert!(!short.is_whole());
        assert_eq!(short.get(1, 0), Some(Rgb::new(1, 2, 3)));
        assert_eq!(short.get(0, 1), Some(Rgb::new(1, 2, 3)));
        assert_eq!(short.get(1, 1), None);
        assert_eq!(short.get(2, 0), None);
        let unnamed = Sprite::new(2, 1, &PALETTE, &[1, 2, 3, 1, 2, 4]);
        assert!(!unnamed.is_whole());
        assert_eq!(unnamed.entry(0, 0), Some(0));
        assert_eq!(unnamed.get(1, 0), None);
        assert!(Sprite::new(2, 1, &PALETTE, &[1, 2, 3, 1, 2, 3]).is_whole());
        assert!(!Sprite::new(2, 1, &PALETTE, &[1, 2, 3, 1, 2, 3, 1, 2, 3]).is_whole());
    }

    // ADR-1401 D7: the notice draws the drawing's characters in its font,
    // each in its cell, with the text beside it
    #[test]
    fn the_notice_draws_the_drawing_in_the_fonts_glyphs() {
        let (w, h) = (640, 400);
        let font = font("unifont", 16);
        let mut canvas = Canvas::new(w, h, 0);
        Notice::report(panic_report(3, Some(drawing())))
            .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
            .expect("it fits");
        let (cw, ch) = (8, 16);
        let inset = (2 + cw / 2, 2 + ch / 4);
        let c = Colors::NOTICE;
        let cell = |ch_: char| -> Vec<u32> {
            let g = font
                .glyph(&[ch_], Width::Narrow, Style::default())
                .expect("a glyph");
            g.pixels()
                .iter()
                .map(|&a| mask(c.ink, c.paper, a))
                .collect()
        };
        for (col, row) in [(0, 0), (31, 0), (0, 19), (31, 19)] {
            assert_eq!(
                block(&canvas, inset.0 + col * cw, inset.1 + row * ch, cw, ch),
                cell('F'),
                "{col},{row}"
            );
        }
        // the gap is paper, and the text's first letter is past it
        assert_eq!(
            block(&canvas, inset.0 + 32 * cw, inset.1, 2 * cw, ch),
            vec![pixel(c.paper); 2 * 8 * 16]
        );
        assert_eq!(
            block(&canvas, inset.0 + 34 * cw, inset.1, cw, ch),
            cell('v')
        );
    }

    // ADR-1401 D7: a report too tall for its display drops its items from
    // the last and counts each one dropped, so what happened, the count and
    // its next step still show
    #[test]
    fn a_report_too_tall_for_its_display_counts_the_items_it_drops() {
        let three = report(&["happened"], &["error one", "error two", "error three"], 0);
        let font = font("unscii-8", 8);
        // 4 lines of 8 between the edges and the quarter-cell insets
        let (w, h) = (320, 6 + 4 * 8);
        let drawn = |notice: Notice| {
            let mut canvas = Canvas::new(w, h, 0);
            notice
                .draw(&font, display(w, h), Colors::NOTICE, None, &mut canvas)
                .expect("it fits");
            canvas
        };
        // the count's digit is in no line of the report's own text
        assert_eq!(
            drawn(Notice::report(three.clone())),
            drawn(Notice::new("happened\nerror one\nMore: 2\nnext step"))
        );
        assert_eq!(
            drawn(Notice::new(three.text())),
            drawn(Notice::new("happened\nerror one\nerror two\nerror three")),
            "any other notice is cut"
        );
        let lines = |r: &NoticeReport, room| text(&laid(r, 38, room).lines);
        // with room for every item, nothing goes and nothing is counted
        assert_eq!(
            lines(&three, 9),
            [
                "happened",
                "error one",
                "error two",
                "error three",
                "next step"
            ]
        );
        // a count the report already has grows by each item dropped
        let more = report(&["happened", "why"], &["error one", "error two"], 2);
        assert_eq!(
            lines(&more, 9),
            [
                "happened",
                "why",
                "error one",
                "error two",
                "More: 2",
                "next step"
            ]
        );
        assert_eq!(
            lines(&more, 5),
            ["happened", "why", "error one", "More: 3", "next step"]
        );
        assert_eq!(lines(&more, 4), ["happened", "why", "More: 4", "next step"]);
        // a next step that wraps keeps every line of its own
        let long = NoticeReport {
            next: ["word"; 10].join(" "),
            ..three.clone()
        };
        assert_eq!(
            lines(&long, 4),
            [
                "happened",
                "More: 3",
                "word word word word word word word",
                "word word word"
            ]
        );
        // with room for less than the parts that stay, the rest is cut
        assert_eq!(lines(&three, 2), ["happened", "More: 3"]);
        // a report with no items is its parts that stay
        assert_eq!(
            lines(&report(&["happened"], &[], 0), 3),
            ["happened", "next step"]
        );
    }

    // the notice and the mark turn with the display, as the grid does
    #[test]
    fn every_orientation_draws_the_same_notice_and_mark_turned() {
        const UNTOUCHED: u32 = 0x0012_3456;
        let font = font("unscii-8", 8);
        let (w, h) = (320, 240);
        let draw = |o: Orientation, dw: u32, dh: u32| {
            let mut c = Canvas::new(dw, dh, UNTOUCHED);
            let d = Surface {
                orientation: o,
                ..display(dw, dh)
            };
            let a = Notice::new(FIGURE)
                .draw(&font, d, Colors::NOTICE, None, &mut c)
                .expect("it fits");
            // the view's top right cell, where the display draws it
            let cell = match o {
                Orientation::Normal => (312, 0),
                Orientation::UpsideDown => (0, 232),
                Orientation::Right => (232, 312),
                Orientation::Left => (0, 0),
            };
            let cell = Rect {
                x: cell.0,
                y: cell.1,
                width: 8,
                height: 8,
            };
            let b = Mark::Focus
                .draw(&font, d, cell, Colors::NOTICE, &mut c)
                .expect("it fits");
            (c, a, b)
        };
        let (normal, _, _) = draw(Orientation::Normal, w, h);
        for o in [
            Orientation::UpsideDown,
            Orientation::Right,
            Orientation::Left,
        ] {
            let (dw, dh) = if o.turned() { (h, w) } else { (w, h) };
            let (turned, a, b) = draw(o, dw, dh);
            for y in 0..h {
                for x in 0..w {
                    let (tx, ty) = match o {
                        Orientation::UpsideDown => (w - 1 - x, h - 1 - y),
                        Orientation::Right => (h - 1 - y, x),
                        Orientation::Left => (y, w - 1 - x),
                        Orientation::Normal => (x, y),
                    };
                    assert_eq!(turned.pixel(tx, ty), normal.pixel(x, y), "{o:?} {x},{y}");
                }
            }
            // the areas returned are exactly the pixels written
            let inside = |r: Rect, x: u32, y: u32| {
                x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height
            };
            for y in 0..dh {
                for x in 0..dw {
                    let written = turned.pixel(x, y) != Some(UNTOUCHED);
                    assert_eq!(written, inside(a, x, y) || inside(b, x, y), "{o:?} {x},{y}");
                }
            }
        }
    }

    // vtium/vtium#135: at 1 bit the box is lit where its paper is light, its
    // text and edge unlit, and every pixel black or white
    #[test]
    fn a_1_bit_display_draws_the_notice_in_black_and_white() {
        let mut canvas = Canvas::new(320, 240, 0x0080_8080);
        let d = Surface {
            depth: Depth::One,
            ..display(320, 240)
        };
        // a rounded corner and a diagonal, sprites of partial coverage
        let area = Notice::new(format!("{FIGURE} \u{256d}\u{2571}"))
            .draw(&font("unscii-8", 8), d, Colors::NOTICE, None, &mut canvas)
            .expect("it fits");
        let box_ = block(&canvas, 0, 0, area.width, area.height);
        assert!(box_.iter().all(|&p| p == 0 || p == 0x00ff_ffff));
        assert_eq!(canvas.pixel(0, 0), Some(0), "the edge");
        assert_eq!(canvas.pixel(1, 1), Some(0x00ff_ffff), "the paper");
        assert!(box_.iter().filter(|&&p| p == 0).count() > 2 * 320 + 2 * area.height as usize);
        // text on its paper's side of the cut still shows, as a cell's does
        let pale = Colors {
            ink: Rgb::new(0xc0, 0xc0, 0xc0),
            paper: Rgb::new(0xff, 0xff, 0xff),
            edge: Rgb::new(0xe0, 0xe0, 0xe0),
        };
        let mut canvas = Canvas::new(320, 240, 0);
        Notice::new("x").draw(&font("unscii-8", 8), d, pale, None, &mut canvas);
        assert_eq!(canvas.pixel(0, 0), Some(0), "the edge");
        assert!(block(&canvas, 5, 3, 8, 8).contains(&0), "the text");
    }

    // ADR-1401 D5: every head shows whether it holds the focus, filled where
    // it does and hollow where it does not, legible at 8 pixels
    #[test]
    fn the_mark_shows_whether_its_head_holds_the_focus() {
        for (name, ch) in [("unscii-8", 8), ("unifont", 16), ("unifont", 32)] {
            let font = font(name, ch);
            let ink = |mark: Mark| {
                assert!(font.face().has_glyph(mark.char()), "{name} {mark:?}");
                let mut canvas = Canvas::new(320, 240, 0);
                let cell = font.cell();
                let cell = Rect {
                    x: 320 - cell.width,
                    y: 0,
                    width: cell.width,
                    height: cell.height,
                };
                let area = mark
                    .draw(&font, display(320, 240), cell, Colors::NOTICE, &mut canvas)
                    .expect("it fits");
                assert_eq!(area, cell);
                block(&canvas, area.x, 0, area.width, area.height)
                    .iter()
                    .filter(|&&p| p == pixel(Colors::NOTICE.ink))
                    .count()
            };
            let (held, not) = (ink(Mark::Focus), ink(Mark::NoFocus));
            assert!(
                not > 0 && held > not,
                "{name} at {ch}: {held} against {not}"
            );
        }
    }

    // ADR-1401 D7: text quoted from a file is escaped first, so it cannot
    // add a line to the notice or hide a character in it
    #[test]
    fn quote_escapes_what_a_file_could_hide() {
        assert_eq!(quote("desc"), "\"desc\"");
        assert_eq!(quote("a\nb"), "\"a\\nb\"");
        assert_eq!(quote("\u{1b}[2J"), "\"\\u{1b}[2J\"");
        assert_eq!(quote("\u{202e}csed"), "\"\\u{202e}csed\"");
        assert_eq!(quote("x\u{200b}y"), "\"x\\u{200b}y\"");
        assert_eq!(quote("say \"hi\" \\"), "\"say \\\"hi\\\" \\\\\"");
        assert_eq!(quote("caf\u{e9} \u{6f22}"), "\"caf\u{e9} \u{6f22}\"");
    }

    /// The renderer's drawing of `w` by `h` turned as `o`, its cells blank
    /// but for a full block in the top right one, and that cell's display
    /// rectangle as the renderer placed it.
    fn top_right_cell(font: &Font<SetFace>, w: u32, h: u32, o: Orientation) -> (Canvas, Rect) {
        let cell = font.cell();
        let (vw, vh) = if o.turned() { (h, w) } else { (w, h) };
        let (cols, rows) = ((vw / cell.width) as usize, (vh / cell.height) as usize);
        let mut rows_ = lines(cols, rows, ' ');
        rows_[0].write(
            cols - 1,
            Glyph::Char('\u{2588}'),
            vtium_term::Width::Narrow,
            &vtium_term::Style::DEFAULT,
            None,
        );
        let damage = Damage::new(cols, rows);
        let palette = palette();
        let config = Config {
            orientation: o,
            ..Config::default()
        };
        let mut r = Renderer::new(font.clone(), w, h, config);
        let mut canvas = Canvas::new(w, h, 0);
        r.draw(
            &frame(&rows_, &damage, &palette, Age::NEVER),
            &mut canvas,
            0,
            &DrawState::default(),
        );
        let lit: Vec<(u32, u32)> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| canvas.pixel(x, y) != Some(0))
            .collect();
        let (x0, y0) = (
            lit.iter().map(|p| p.0).min().expect("lit"),
            lit.iter().map(|p| p.1).min().expect("lit"),
        );
        let (x1, y1) = (
            lit.iter().map(|p| p.0).max().expect("lit"),
            lit.iter().map(|p| p.1).max().expect("lit"),
        );
        let rect = Rect {
            x: x0,
            y: y0,
            width: x1 - x0 + 1,
            height: y1 - y0 + 1,
        };
        (canvas, rect)
    }

    // the mark takes the cell it is given, wherever the renderer centred
    // the grid, and touches nothing beside it
    #[test]
    fn the_mark_fills_the_grid_cell_it_is_given() {
        for (name, ch, w, h, o) in [
            ("unscii-8", 8, 1366, 768, Orientation::Normal),
            ("unscii-8", 8, 1366, 768, Orientation::Right),
            ("unifont", 16, 640, 480, Orientation::Normal),
            ("unifont", 16, 1366, 768, Orientation::UpsideDown),
            ("unifont", 32, 1366, 768, Orientation::Left),
        ] {
            let font = font(name, ch);
            let (grid, cell) = top_right_cell(&font, w, h, o);
            let mut canvas = grid.clone();
            let d = Surface {
                orientation: o,
                ..display(w, h)
            };
            let area = Mark::Focus
                .draw(&font, d, cell, Colors::NOTICE, &mut canvas)
                .expect("it fits");
            assert_eq!(area, cell, "{name} {w}x{h} {o:?}");
            let c = Colors::NOTICE;
            for y in 0..h {
                for x in 0..w {
                    let now = canvas.pixel(x, y).expect("inside");
                    let inside = x >= cell.x
                        && y >= cell.y
                        && x < cell.x + cell.width
                        && y < cell.y + cell.height;
                    if inside {
                        assert!(
                            now == pixel(c.ink) || now == pixel(c.paper),
                            "{name} {w}x{h} {o:?}: the mark at {x},{y}"
                        );
                    } else {
                        assert_eq!(
                            Some(now),
                            grid.pixel(x, y),
                            "{name} {w}x{h} {o:?}: beside it at {x},{y}"
                        );
                    }
                }
            }
        }
    }

    // widths at which the notice's text could run under the mark, and a
    // height whose centred grid puts the mark across the second line
    #[test]
    fn the_notice_keeps_its_text_clear_of_the_mark() {
        let filler = "m".repeat(400);
        for (name, ch, w, h) in [
            ("unscii-8", 8, 320, 240),
            ("unscii-8", 8, 1050, 480),
            ("unifont", 16, 324, 480),
            ("unifont", 16, 900, 480),
            ("unifont", 32, 600, 480),
            ("unifont", 32, 1080, 480),
            ("unifont", 32, 1080, 511),
        ] {
            let font = font(name, ch);
            let (grid, cell) = top_right_cell(&font, w, h, Orientation::Normal);
            for text in [FIGURE, filler.as_str()] {
                let mut canvas = grid.clone();
                let area = Notice::new(text)
                    .draw(
                        &font,
                        display(w, h),
                        Colors::NOTICE,
                        Some(cell),
                        &mut canvas,
                    )
                    .expect("it fits");
                let ink = pixel(Colors::NOTICE.ink);
                let under = block(&canvas, cell.x, cell.y, cell.width, cell.height);
                assert!(
                    !under.contains(&ink),
                    "{name} {w}x{h}: text under the mark at {cell:?}"
                );
                let shown = block(&canvas, 0, 0, w, area.height);
                assert!(shown.contains(&ink), "{name} {w}x{h}: the text");
                // the mark, drawn last, shows whole over the box
                let mark = Mark::Focus
                    .draw(&font, display(w, h), cell, Colors::NOTICE, &mut canvas)
                    .expect("it fits");
                assert_eq!(mark, cell);
            }
        }
    }

    // the bound of quote: no control character passes, and look-alikes do
    #[test]
    fn quote_lets_no_control_character_through() {
        for c in ('\u{0}'..='\u{1f}')
            .chain(['\u{7f}'])
            .chain('\u{80}'..='\u{9f}')
        {
            let q = quote(&format!("a{c}b"));
            assert!(
                !q.chars().any(char::is_control),
                "{:?} gave {q:?}",
                u32::from(c)
            );
        }
        for text in ["\u{2800}", "e\u{301}", "d\u{435}sc"] {
            assert_eq!(quote(text), format!("\"{text}\""));
        }
    }

    /// A face with no circles: it holds `?` alone, drawn solid.
    struct Bare;

    impl Face for Bare {
        fn cell(&self) -> vtium_font::Cell {
            vtium_font::Cell {
                width: 8,
                height: 8,
            }
        }
        fn size_step(&self) -> u32 {
            8
        }
        fn has_glyph(&self, c: char) -> bool {
            c == '?'
        }
        fn draw(&self, cluster: &[char], _: Style) -> Option<vtium_font::Glyph> {
            (cluster == ['?'])
                .then(|| vtium_font::Glyph::from_pixels(8, 8, Format::Mask, vec![255; 64]))
                .flatten()
        }
    }

    // a font without the circles still tells the focus apart: a filled
    // box where the head holds it and an empty one where it does not,
    // never the font's stand-in for a character it lacks
    #[test]
    fn a_font_without_the_circles_draws_the_mark_as_boxes() {
        let font = Font::new(Bare);
        assert!(!font.has_glyph(Mark::Focus.char()) && !font.has_glyph(Mark::NoFocus.char()));
        let cell = Rect {
            x: 312,
            y: 0,
            width: 8,
            height: 8,
        };
        let draw = |mark: Mark| {
            let mut canvas = Canvas::new(320, 240, 0);
            mark.draw(&font, display(320, 240), cell, Colors::NOTICE, &mut canvas)
                .expect("it fits");
            block(&canvas, 312, 0, 8, 8)
                .iter()
                .map(|&p| u8::from(p == pixel(Colors::NOTICE.ink)))
                .collect::<Vec<u8>>()
        };
        #[rustfmt::skip]
        let filled = [
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 1, 1, 1, 1, 0, 0,
            0, 0, 1, 1, 1, 1, 0, 0,
            0, 0, 1, 1, 1, 1, 0, 0,
            0, 0, 1, 1, 1, 1, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
        ];
        #[rustfmt::skip]
        let empty = [
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 1, 1, 1, 1, 0, 0,
            0, 0, 1, 0, 0, 1, 0, 0,
            0, 0, 1, 0, 0, 1, 0, 0,
            0, 0, 1, 1, 1, 1, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(draw(Mark::Focus), filled);
        assert_eq!(draw(Mark::NoFocus), empty);
    }

    // vtium/vtium#135: the notice's glyphs take the 1-bit stand-ins a
    // cell's do, so a shade keeps its pattern where the cut would empty it
    #[test]
    fn a_1_bit_notice_draws_a_shade_as_its_1_bit_pattern() {
        let font = font("unscii-8", 8);
        let d = Surface {
            depth: Depth::One,
            ..display(320, 240)
        };
        let mut canvas = Canvas::new(320, 240, 0);
        Notice::new("\u{2591}")
            .draw(&font, d, Colors::NOTICE, None, &mut canvas)
            .expect("it fits");
        let thick = vtium_font::sprite::light_line(8);
        let want: Vec<u32> = vtium_font::sprite::one_bit('\u{2591}', 8, 8, thick)
            .expect("a 1-bit shade")
            .pixels()
            .iter()
            .map(|&a| if a >= 128 { 0 } else { 0x00ff_ffff })
            .collect();
        assert!(want.contains(&0) && want.contains(&0x00ff_ffff));
        assert_eq!(block(&canvas, 5, 3, 8, 8), want);
    }

    /// A face of a narrow `x` and a wide U+4E00, solid, that counts the
    /// glyphs it draws. It knows each glyph's width without drawing it,
    /// as a face that keeps the widths in its map does.
    struct Counting(std::cell::Cell<u32>);

    const WIDE: char = '\u{4e00}';

    impl Face for Counting {
        fn cell(&self) -> vtium_font::Cell {
            vtium_font::Cell {
                width: 8,
                height: 8,
            }
        }
        fn size_step(&self) -> u32 {
            8
        }
        fn has_glyph(&self, c: char) -> bool {
            c == 'x' || c == WIDE
        }
        fn advance(&self, c: char) -> Option<Width> {
            match c {
                'x' => Some(Width::Narrow),
                WIDE => Some(Width::Wide),
                _ => None,
            }
        }
        fn draw(&self, cluster: &[char], _: Style) -> Option<vtium_font::Glyph> {
            self.0.set(self.0.get() + 1);
            match cluster {
                ['x'] => vtium_font::Glyph::from_pixels(8, 8, Format::Mask, vec![255; 64]),
                [WIDE] => vtium_font::Glyph::from_pixels(16, 8, Format::Mask, vec![255; 128])
                    .map(|g| g.with_cells(Width::Wide)),
                _ => None,
            }
        }
    }

    // vtium/vtium#159: the notice measures its text by the face's advance
    // and draws each character it shows once, however often it appears,
    // so its cost follows the characters and not the text; the wide
    // character takes two cells, so the `x` after it starts three cells
    // from the line's start
    #[test]
    fn a_notice_measures_by_the_advance_and_draws_each_character_once() {
        let draw = |text: String| {
            let font = Font::new(Counting(std::cell::Cell::new(0)));
            let mut canvas = Canvas::new(320, 240, 0);
            Notice::new(text)
                .draw(&font, display(320, 240), Colors::NOTICE, None, &mut canvas)
                .expect("it fits");
            (font.face().0.get(), canvas)
        };
        let (draws, _) = draw(format!("x{WIDE}x").repeat(100));
        assert_eq!(draws, 2, "one draw for each character");
        let (_, canvas) = draw(format!("x{WIDE}x"));
        // the text starts an eighth of the cell plus half a cell across and
        // an eighth plus a quarter down: at 5 and 3
        let ink = pixel(Colors::NOTICE.ink);
        assert!(block(&canvas, 5, 3, 32, 8).iter().all(|&p| p == ink));
        assert!(block(&canvas, 37, 3, 1, 8).iter().all(|&p| p != ink));
    }
}
