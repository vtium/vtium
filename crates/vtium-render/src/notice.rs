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
use crate::sprite::hollow_box;
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

    /// The view's area the display area `r` draws: `Surface::area`
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
    Some(at_depth(font, depth, c, width, g))
}

/// `g`, the glyph of `c` `width` cells wide in `font`, as a cell at
/// `depth` draws it, as [`standin`] says.
fn at_depth<F: Face>(font: &Font<F>, depth: Depth, c: char, width: Width, g: Glyph) -> Glyph {
    Fallback::new(depth, font.cell())
        .and_then(|f| f.glyph(&g, 1, &[c], width, Variant::default()))
        .unwrap_or(g)
}

/// How a notice draws one of its characters.
enum Shape {
    /// The character's own glyph, or nothing where the font draws none.
    Glyph(Option<Glyph>),
    /// The hollow box as wide as the character's cells.
    Box,
}

/// The glyphs of a notice's characters, each drawn once however often it
/// appears, with the cells it takes: its face's advance for it, which the
/// face gives without drawing the glyph. A character of a range that
/// vtium draws as sprites, such as box drawing, draws as its sprite where
/// the font lacks it, whatever the font's policy says, so a dashed line
/// in a notice stays a dashed line. Any other character the font lacks
/// takes the cells the terminal gives it and draws as a hollow box as
/// wide as them ([`hollow_box`]). Every narrow box is the same, and so is
/// every wide one, so the notice draws each at most once.
///
/// References:
/// - ADR-1204 D1: a character the built-in font lacks draws as a hollow
///   box as wide as its cells, and the sprites draw the box-drawing
///   characters the built-in font lacks.
struct Glyphs {
    /// Sorted by character.
    glyphs: Vec<(char, Width, Shape)>,
    /// The narrow box and the wide one, where a character draws as one.
    boxes: [Option<Glyph>; 2],
}

impl Glyphs {
    fn new<F: Face>(font: &Font<F>, depth: Depth, text: &str) -> Glyphs {
        let mut chars: Vec<char> = text.chars().filter(|&c| c != '\n').collect();
        chars.sort_unstable();
        chars.dedup();
        let cell = font.cell();
        let mut boxes = [None, None];
        let glyphs = chars
            .into_iter()
            .map(|c| {
                if font.has_glyph(c) {
                    let width = font.face().advance(c).unwrap_or(Width::Narrow);
                    return (c, width, Shape::Glyph(standin(font, depth, c, width)));
                }
                let width = match vtium_term::cluster_width(&[c]) {
                    2 => Width::Wide,
                    _ => Width::Narrow,
                };
                let thick = vtium_font::sprite::light_line(cell.height);
                let w = cell.width.saturating_mul(width.cells());
                if let Some(g) = vtium_font::sprite::draw(c, w, cell.height, thick) {
                    let g = at_depth(font, depth, c, width, g.with_cells(width));
                    return (c, width, Shape::Glyph(Some(g)));
                }
                let slot = usize::from(width == Width::Wide);
                if let Some(empty @ None) = boxes.get_mut(slot) {
                    *empty = hollow_box(cell, width);
                }
                (c, width, Shape::Box)
            })
            .collect();
        Glyphs { glyphs, boxes }
    }

    fn get(&self, c: char) -> Option<&(char, Width, Shape)> {
        let i = self.glyphs.binary_search_by_key(&c, |g| g.0).ok()?;
        self.glyphs.get(i)
    }

    fn width(&self, c: char) -> Width {
        self.get(c).map_or(Width::Narrow, |g| g.1)
    }

    fn glyph(&self, c: char) -> Option<&Glyph> {
        match self.get(c)? {
            (_, _, Shape::Glyph(g)) => g.as_ref(),
            (_, width, Shape::Box) => self.boxes.get(usize::from(*width == Width::Wide))?.as_ref(),
        }
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
mod tests;
