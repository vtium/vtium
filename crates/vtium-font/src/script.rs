//! Superscript and subscript: the small glyph variants that SGR 73 and 74
//! ask for, and SGR 75 ends (ADR-1202).
//!
//! The codes come from mintty, whose wiki lists them in the table of text
//! attributes on its Tips page
//! (<https://github.com/mintty/mintty/wiki/Tips#text-attributes-and-rendering>)
//! and whose changelog adds them in 3.0.1. 73 starts superscript, 74
//! starts subscript, and 75 returns to the baseline. mintty leaves the size
//! and the place of the small glyph to the terminal, and vtium chooses them
//! here. This module holds the rules that the console and the glyph helper
//! both use, so the two never compute different boxes (ADR-1202 D11).
//!
//! **The box (D1).** A glyph's own box is the rows from its font's ascent
//! to its descent at the size it is drawn, which for a bitmap font is the
//! whole height of its strike. A script glyph is two thirds of the own
//! box's width and height, rounded to whole pixels ([`size`]). It is
//! centred across its cells, with an odd spare column on its right. A
//! superscript's top lies on the top row of the text band, and a
//! subscript's bottom on the bottom row of the text band ([`place`]). The
//! text band is the cell less the rows that line spacing adds above and
//! below it (ADR-1302 D13).
//!
//! **How the small glyph is made.** The face says what kind of source a
//! glyph comes from ([`Origin`]):
//!
//! - A 1-bit bitmap glyph ([`Kind::Bitmap`]) stays 1-bit (D4). Each pixel
//!   of the normal glyph goes to the small pixel that holds its centre,
//!   and a small pixel is ink when any pixel that goes to it is ink. This
//!   keeps every stroke, one pixel wide, so the glyph is crisp on any
//!   display, and a 1-bit display shows it exactly (D10).
//! - Any other glyph ([`Kind::Smooth`]) is averaged: each small pixel is
//!   the average of the pixels it covers, weighted by how much of each it
//!   covers (D5). A color glyph stays premultiplied.
//!
//! **Bold and italic (D7).** Where a face adds bold or italic to a bitmap
//! glyph itself, by widening and shearing, the engine adds them to the
//! small glyph instead, after it is made small. Bold widens each row by a
//! pixel to the right. Italic shears the small glyph about its own
//! baseline, by whole pixels, as ADR-1302 D4 shears a bitmap font of the
//! small glyph's height ([`crate::italic::shear_shift`]).
//!
//! **Ink (D8).** A script glyph's ink stays inside its row, so ink above
//! and below the row is cut. Ink to the side of its cells stays in its ink
//! box ([`Glyph::ink`]), within its reach, as a bitmap glyph's sheared ink
//! does.
//!
//! [`crate::Font`] draws a sprite in a script cell at the small box's size
//! (D6), and draws every other script glyph from the face that draws the
//! cell's normal glyph (D15).

use crate::face::Style;
use crate::glyph::{Format, Glyph};
use crate::spacing::Band;

/// Where a glyph sits in its cell: SGR 73, 74 and 75.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, PartialOrd, Ord, Hash)]
pub enum Script {
    /// The normal glyph: SGR 75, and every cell no SGR 73 or 74 marks.
    #[default]
    Baseline,
    /// The small glyph against the top of the text band: SGR 73.
    Super,
    /// The small glyph against the bottom of the text band: SGR 74.
    Sub,
}

/// The size of a superscript or subscript glyph, as a fraction of the
/// normal glyph's own width and height: two thirds.
pub const SCRIPT_SCALE: (u32, u32) = (2, 3);

/// Everything a glyph is drawn in beyond its text and width: its style and
/// its place in the cell. A glyph cache keeps each variant of a text apart
/// under this, so a superscript glyph never stands in for a normal one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Variant {
    pub style: Style,
    pub script: Script,
}

impl From<Style> for Variant {
    /// The normal glyph in `style`.
    fn from(style: Style) -> Variant {
        Variant {
            style,
            script: Script::Baseline,
        }
    }
}

/// The kind of source a glyph comes from, which decides how its script
/// glyph is made (ADR-1202 D4, D5).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Kind {
    /// A 1-bit bitmap source: a bitmap font, a set's 1-bit strike, the
    /// built-in fonts, or the glyph that shows a missing character's
    /// codepoint. Its script glyph is scaled with each pixel fully on or
    /// off (D4).
    Bitmap,
    /// Any other source: a color glyph, and a gray mask such as an outline
    /// font's glyph. Its script glyph is scaled by averaging (D5).
    #[default]
    Smooth,
}

/// A box of a glyph's pixels: `width` columns from `left` and `height`
/// rows from `top`, counted from the glyph's top left corner.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

/// What a face's glyph is made from, which a script glyph is made by
/// ([`Glyph::scripted`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Origin {
    /// The kind of source the glyph comes from.
    pub kind: Kind,
    /// The glyph's own box in its pixels (D1); `None` for the text band
    /// across the whole glyph.
    pub own: Option<Rect>,
    /// The row under the glyph's baseline, counted from its top, which
    /// italic shears the small glyph about; `None` for the own box's bottom
    /// edge.
    pub baseline: Option<u32>,
    /// The part of the style that the engine adds to the small glyph
    /// (D7): bold widened and italic sheared after the glyph is made
    /// small. The glyph is drawn without it. Only a [`Kind::Bitmap`]
    /// glyph's style is added this way, so a smooth glyph's `synth` is
    /// ignored.
    pub synth: Style,
}

impl Origin {
    /// A 1-bit bitmap glyph whose own box is the whole glyph, sheared
    /// about `baseline`, with the face leaving `synth` to the engine.
    pub fn bitmap(baseline: u32, synth: Style) -> Origin {
        Origin {
            kind: Kind::Bitmap,
            own: None,
            baseline: Some(baseline),
            synth,
        }
    }

    /// This origin for a glyph `from` pixels wide and high, after a face
    /// centres the glyph in a box `to` wide and high, which lies `above`
    /// rows below the top of its cell. The offsets round toward zero, and
    /// `None` for [`Origin::own`] stands for the whole glyph `from`. A box
    /// that the centring cuts is cut to `to`.
    pub fn boxed(self, from: (u32, u32), to: (u32, u32), above: u32) -> Origin {
        let own = self.own.unwrap_or(Rect {
            left: 0,
            top: 0,
            width: from.0,
            height: from.1,
        });
        let offset = |to: u32, from: u32| i64::from(to).saturating_sub(i64::from(from)) / 2;
        let (dx, dy) = (offset(to.0, from.0), offset(to.1, from.1));
        // the box's edges moved, cut to the box it is centred in
        let span = |start: u32, len: u32, by: i64, limit: u32| -> (u32, u32) {
            let a = i64::from(start)
                .saturating_add(by)
                .clamp(0, i64::from(limit));
            let b = i64::from(start)
                .saturating_add(i64::from(len))
                .saturating_add(by)
                .clamp(0, i64::from(limit));
            let a = u32::try_from(a).unwrap_or(0);
            (a, u32::try_from(b).unwrap_or(0).saturating_sub(a))
        };
        let (left, width) = span(own.left, own.width, dx, to.0);
        let (top, height) = span(own.top, own.height, dy, to.1);
        let baseline = self.baseline.map(|b| {
            let b = i64::from(b).saturating_add(dy).clamp(0, i64::from(to.1));
            u32::try_from(b).unwrap_or(0).saturating_add(above)
        });
        Origin {
            own: Some(Rect {
                left,
                top: top.saturating_add(above),
                width,
                height,
            }),
            baseline,
            ..self
        }
    }
}

/// `n` times [`SCRIPT_SCALE`], rounded to the nearest pixel, and at least
/// 1.
fn scaled(n: u32) -> u32 {
    let (num, den) = SCRIPT_SCALE;
    n.saturating_mul(num)
        .saturating_add(den / 2)
        .checked_div(den)
        .unwrap_or(n)
        .max(1)
}

/// The size of the script glyph whose normal glyph's own box is `w` by
/// `h` pixels: two thirds of each, rounded to whole pixels, at least 1
/// (D1).
pub fn size((w, h): (u32, u32)) -> (u32, u32) {
    (scaled(w), scaled(h))
}

/// Where a script glyph `sw` by `sh` pixels goes in its cells, `span`
/// pixels across and `height` high with `band` added by line spacing: the
/// column and row of its top left corner (D1). It is centred across the
/// cells, the odd spare column on its right. A superscript's top lies on
/// the text band's top row, and a subscript's bottom on the text band's
/// bottom row. The baseline lies on the band's top row as a superscript
/// does.
pub fn place(
    script: Script,
    (sw, sh): (u32, u32),
    (span, height): (u32, u32),
    band: Band,
) -> (u32, u32) {
    let x = span.saturating_sub(sw) / 2;
    let y = match script {
        Script::Baseline | Script::Super => band.above,
        Script::Sub => height.saturating_sub(band.below).saturating_sub(sh),
    };
    (x, y)
}

/// `c`'s sprite as `script` draws it in cells `span` pixels across and
/// `height` high, with `band` added by line spacing (D6): drawn to fill a
/// box two thirds of the text band's height and of the cells' width, with
/// the light line of that height ([`crate::sprite::light_line`]), and
/// placed as [`place`] says, so its edges stay on whole pixels. `None` for
/// the baseline, a character that is no sprite, or a size the sprites
/// refuse.
pub fn sprite(c: char, script: Script, (span, height): (u32, u32), band: Band) -> Option<Glyph> {
    if script == Script::Baseline {
        return None;
    }
    let text = height.saturating_sub(band.added()).max(1);
    let (sw, sh) = size((span, text));
    let small = crate::sprite::draw(c, sw, sh, crate::sprite::light_line(sh))?;
    let (x, y) = place(script, (sw, sh), (span, height), band);
    let mut out = Glyph::mask(span, height)?;
    for (dy, row) in (0u32..).zip(small.rows()) {
        for (dx, &a) in (0u32..).zip(row) {
            if let Some(px) = out.coverage_mut(x.saturating_add(dx), y.saturating_add(dy)) {
                *px = a;
            }
        }
    }
    Some(out)
}

/// Where source position `s`, counted from the own box's edge, goes in a
/// side `from` pixels long made `to` pixels long: the small pixel that
/// holds the centre of source pixel `s` (D4). Positions before the box
/// map before it.
fn centre(s: i64, from: u32, to: u32) -> i64 {
    let num = s
        .saturating_mul(2)
        .saturating_add(1)
        .saturating_mul(i64::from(to));
    num.checked_div_euclid(i64::from(from).saturating_mul(2))
        .unwrap_or(0)
}

/// How much of the source pixel `s` the scaled pixel `d` covers, in units
/// where a source pixel is `to` wide and a scaled pixel `from` wide: the
/// overlap of `[d * from, (d + 1) * from)` and `[s * to, (s + 1) * to)`.
fn overlap(d: u32, s: u32, from: u32, to: u32) -> u64 {
    let (d, s, from, to) = (u64::from(d), u64::from(s), u64::from(from), u64::from(to));
    let lo = d.saturating_mul(from).max(s.saturating_mul(to));
    let hi = d
        .saturating_add(1)
        .saturating_mul(from)
        .min(s.saturating_add(1).saturating_mul(to));
    hi.saturating_sub(lo)
}

/// The source pixels `[first, last]` that the scaled pixel `d` covers, for
/// a side `from` source pixels long scaled to `to`.
fn covered(d: u32, from: u32, to: u32) -> (u32, u32) {
    let first = d.saturating_mul(from).checked_div(to).unwrap_or(0);
    let end = d
        .saturating_add(1)
        .saturating_mul(from)
        .saturating_add(to.saturating_sub(1))
        .checked_div(to)
        .unwrap_or(0);
    (first, end.saturating_sub(1).min(from.saturating_sub(1)))
}

/// A 1-bit picture over columns `left..left + width` and every row of a
/// glyph, which may reach past the glyph's cells to the side.
struct Canvas {
    left: i64,
    width: usize,
    height: usize,
    lit: Vec<bool>,
}

impl Canvas {
    fn new(left: i64, right: i64, height: u32) -> Option<Canvas> {
        let width = usize::try_from(right.saturating_sub(left)).ok()?;
        let height = usize::try_from(height).ok()?;
        Some(Canvas {
            left,
            width,
            height,
            lit: vec![false; width.checked_mul(height)?],
        })
    }

    fn index(&self, x: i64, y: i64) -> Option<usize> {
        let x = usize::try_from(x.checked_sub(self.left)?).ok()?;
        let y = usize::try_from(y).ok()?;
        (x < self.width && y < self.height).then(|| y.saturating_mul(self.width).saturating_add(x))
    }

    fn get(&self, x: i64, y: i64) -> bool {
        self.index(x, y)
            .and_then(|i| self.lit.get(i).copied())
            .unwrap_or(false)
    }

    fn set(&mut self, x: i64, y: i64) {
        if let Some(px) = self.index(x, y).and_then(|i| self.lit.get_mut(i)) {
            *px = true;
        }
    }

    fn right(&self) -> i64 {
        self.left
            .saturating_add(i64::try_from(self.width).unwrap_or(0))
    }

    fn rows(&self) -> i64 {
        i64::try_from(self.height).unwrap_or(0)
    }

    /// Each row widened by a pixel to the right: a pixel lit when it or
    /// its left neighbour is (D7, ADR-1302 D4's bold).
    fn widened(&self) -> Option<Canvas> {
        let mut out = Canvas::new(
            self.left,
            self.right().saturating_add(1),
            u32::try_from(self.height).ok()?,
        )?;
        for y in 0..self.rows() {
            for x in out.left..out.right() {
                if self.get(x, y) || self.get(x.saturating_sub(1), y) {
                    out.set(x, y);
                }
            }
        }
        Some(out)
    }

    /// Each row moved right by [`crate::italic::shear_shift`] about the
    /// row under the baseline `baseline`, for a glyph `height` rows tall
    /// (D7, ADR-1302 D4's italic).
    fn sheared(&self, baseline: u32, height: u32) -> Option<Canvas> {
        let shift = |y: i64| -> i64 {
            u32::try_from(y)
                .map(|y| i64::from(crate::italic::shear_shift(y, baseline, height)))
                .unwrap_or(0)
        };
        let shifts: Vec<i64> = (0..self.rows()).map(shift).collect();
        let lo = shifts.iter().copied().min().unwrap_or(0).min(0);
        let hi = shifts.iter().copied().max().unwrap_or(0).max(0);
        let mut out = Canvas::new(
            self.left.saturating_add(lo),
            self.right().saturating_add(hi),
            u32::try_from(self.height).ok()?,
        )?;
        for (y, by) in (0..self.rows()).zip(shifts) {
            for x in self.left..self.right() {
                if self.get(x, y) {
                    out.set(x.saturating_add(by), y);
                }
            }
        }
        Some(out)
    }
}

impl Glyph {
    /// This glyph as `script` draws it, for a face whose glyph comes from
    /// `origin` in a cell that line spacing makes taller by `band`: itself
    /// for the baseline, else made small, two thirds of its own box, and
    /// placed against the text band as the module's documentation says.
    /// The result has this glyph's size, cells and format. `self` is the
    /// normal glyph without [`Origin::synth`], which the small glyph then
    /// takes. `None` only past the bounds, which a glyph of this size never
    /// is.
    pub fn scripted(&self, script: Script, origin: &Origin, band: Band) -> Option<Glyph> {
        if script == Script::Baseline {
            return Some(self.clone());
        }
        let (w, h) = (self.width(), self.height());
        let own = origin
            .own
            .filter(|r| r.width > 0 && r.height > 0)
            .unwrap_or(Rect {
                left: 0,
                top: band.above.min(h.saturating_sub(1)),
                width: w,
                height: h.saturating_sub(band.added()).max(1),
            });
        let small = size((own.width, own.height));
        let at = place(script, small, (w, h), band);
        match (origin.kind, self.format()) {
            (Kind::Bitmap, Format::Mask) => self.one_bit(own, small, at, origin),
            _ => self.averaged(own, small, at),
        }
    }

    /// The pixel of this glyph or its ink box at column `x` and row `y`,
    /// counted from its first cell's top left corner, lit at half coverage
    /// or more.
    fn lit_at(&self, x: i64, y: i64) -> bool {
        let lit = |px: Option<&u8>| px.is_some_and(|&p| p >= 0x80);
        if let Some(ink) = self.ink() {
            let bx = x.saturating_sub(i64::from(ink.left()));
            let by = y.saturating_sub(i64::from(ink.top()));
            let (Ok(bx), Ok(by)) = (u32::try_from(bx), u32::try_from(by)) else {
                return false;
            };
            if bx >= ink.width() || by >= ink.height() {
                return false;
            }
            let i = u64::from(by)
                .checked_mul(u64::from(ink.width()))
                .and_then(|i| i.checked_add(u64::from(bx)))
                .and_then(|i| usize::try_from(i).ok());
            return lit(i.and_then(|i| ink.pixels().get(i)));
        }
        match (u32::try_from(x), u32::try_from(y)) {
            (Ok(x), Ok(y)) => lit(self.pixel(x, y).and_then(<[u8]>::first)),
            _ => false,
        }
    }

    /// D4: the own box `own` made `small`, each pixel fully on or off,
    /// placed with its top left at `at`; then `origin`'s style added (D7),
    /// and the ink cut to the row and kept beside the cells (D8).
    fn one_bit(
        &self,
        own: Rect,
        small: (u32, u32),
        at: (u32, u32),
        origin: &Origin,
    ) -> Option<Glyph> {
        let (w, h) = (self.width(), self.height());
        // the source: the glyph, or its ink box where it has one
        let (src_left, src_top, src_w, src_h) = match self.ink() {
            Some(ink) => (
                i64::from(ink.left()),
                i64::from(ink.top()),
                i64::from(ink.width()),
                i64::from(ink.height()),
            ),
            None => (0, 0, i64::from(w), i64::from(h)),
        };
        let map_x = |x: i64| {
            i64::from(at.0).saturating_add(centre(
                x.saturating_sub(i64::from(own.left)),
                own.width,
                small.0,
            ))
        };
        let map_y = |y: i64| {
            i64::from(at.1).saturating_add(centre(
                y.saturating_sub(i64::from(own.top)),
                own.height,
                small.1,
            ))
        };
        let right = src_left.saturating_add(src_w);
        let mut canvas = Canvas::new(
            map_x(src_left).min(0),
            map_x(right.saturating_sub(1))
                .saturating_add(1)
                .max(i64::from(w)),
            h,
        )?;
        for y in src_top..src_top.saturating_add(src_h) {
            let to_y = map_y(y);
            for x in src_left..right {
                if self.lit_at(x, y) {
                    canvas.set(map_x(x), to_y);
                }
            }
        }
        if origin.synth.bold {
            canvas = canvas.widened()?;
        }
        if origin.synth.italic {
            // the row under the small glyph's baseline: under the small row
            // that the normal glyph's last row above its baseline goes to
            let bottom = own.top.saturating_add(own.height);
            let baseline = origin.baseline.unwrap_or(bottom);
            let small_base = if baseline > own.top {
                map_y(i64::from(baseline).saturating_sub(1)).saturating_add(1)
            } else {
                i64::from(at.1)
            };
            let small_base = u32::try_from(small_base.max(0)).unwrap_or(0);
            canvas = canvas.sheared(small_base, small.1)?;
        }
        self.of_canvas(&canvas)
    }

    /// A mask of this glyph's size and cells holding `canvas`: its pixels
    /// inside the cells, and the ink beside them in an ink box of the
    /// row's height, cut to the reach, one cell past the cells on each
    /// side (D8, ADR-1302 D6).
    fn of_canvas(&self, canvas: &Canvas) -> Option<Glyph> {
        let (w, h) = (self.width(), self.height());
        let pixels: Vec<u8> = (0..i64::from(h))
            .flat_map(|y| (0..i64::from(w)).map(move |x| if canvas.get(x, y) { 0xff } else { 0 }))
            .collect();
        let out = Glyph::from_pixels(w, h, Format::Mask, pixels)?.with_cells(self.cells());
        let cell = w.checked_div(self.cells().cells())?;
        let across = i64::from(cell).saturating_mul(i64::from(crate::REACH_CELLS));
        let reach = (across.saturating_neg(), i64::from(w).saturating_add(across));
        let inked = |x: i64| (0..i64::from(h)).any(|y| canvas.get(x, y));
        let first = (reach.0..reach.1).find(|&x| inked(x));
        let last = (reach.0..reach.1).rev().find(|&x| inked(x));
        let (Some(first), Some(last)) = (first, last) else {
            return Some(out);
        };
        let (from, to) = (first.min(0), last.saturating_add(1).max(i64::from(w)));
        if (from, to) == (0, i64::from(w)) {
            return Some(out);
        }
        let box_pixels: Vec<u8> = (0..i64::from(h))
            .flat_map(|y| (from..to).map(move |x| if canvas.get(x, y) { 0xff } else { 0 }))
            .collect();
        let width = u32::try_from(to.saturating_sub(from)).ok()?;
        let left = i32::try_from(from).ok()?;
        out.clone()
            .with_ink((left, 0), (width, h), box_pixels)
            .or(Some(out))
    }

    /// D5: the own box `own` made `small` by averaging, placed with its top
    /// left at `at`, its ink cut to its cells.
    fn averaged(&self, own: Rect, small: (u32, u32), at: (u32, u32)) -> Option<Glyph> {
        let (w, h) = (self.width(), self.height());
        let (ow, oh) = (own.width, own.height);
        let (sw, sh) = small;
        let mut out = Glyph::new(w, h, self.format())?.with_cells(self.cells());
        let bpp = usize::try_from(self.format().bytes_per_pixel()).unwrap_or(4);
        // each scaled pixel's area, in the units `overlap` counts in
        let area = u64::from(ow).saturating_mul(u64::from(oh)).max(1);
        let mut sums = [0u64; 4];
        for dy in 0..sh {
            let (ys, ye) = covered(dy, oh, sh);
            for dx in 0..sw {
                let (xs, xe) = covered(dx, ow, sw);
                sums.fill(0);
                for sy in ys..=ye {
                    let oy = overlap(dy, sy, oh, sh);
                    for sx in xs..=xe {
                        let weight = oy.saturating_mul(overlap(dx, sx, ow, sw));
                        let from = (own.left.saturating_add(sx), own.top.saturating_add(sy));
                        let Some(px) = self.pixel(from.0, from.1) else {
                            continue;
                        };
                        for (sum, &byte) in sums.iter_mut().zip(px) {
                            *sum = sum.saturating_add(u64::from(byte).saturating_mul(weight));
                        }
                    }
                }
                let to = (at.0.saturating_add(dx), at.1.saturating_add(dy));
                let Some(px) = out.pixel_mut(to.0, to.1) else {
                    continue;
                };
                // rounding each channel alike keeps a color glyph's
                // colors at or below its alpha: the sums are in order, and
                // rounding keeps their order
                for (byte, sum) in px.iter_mut().zip(&sums).take(bpp) {
                    let v = sum.saturating_add(area / 2).checked_div(area).unwrap_or(0);
                    *byte = u8::try_from(v).unwrap_or(u8::MAX);
                }
            }
        }
        Some(out)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::face::Width;
    use crate::glyph::Format;

    /// A `w` by `h` mask full from row `top` down, every column.
    fn rows_from(w: u32, h: u32, top: u32) -> Glyph {
        let mut g = Glyph::mask(w, h).unwrap();
        g.fill(0, top, w, h, 255);
        g
    }

    fn ink_rows(g: &Glyph) -> Vec<u32> {
        (0..g.height())
            .filter(|&y| (0..g.width()).any(|x| g.pixel(x, y).unwrap()[0] > 0))
            .collect()
    }

    fn ink_cols(g: &Glyph) -> Vec<u32> {
        (0..g.width())
            .filter(|&x| (0..g.height()).any(|y| g.pixel(x, y).unwrap()[0] > 0))
            .collect()
    }

    /// The columns of row `y` that hold ink.
    fn row_cols(g: &Glyph, y: u32) -> Vec<u32> {
        (0..g.width())
            .filter(|&x| g.pixel(x, y).unwrap()[0] > 0)
            .collect()
    }

    fn bitmap() -> Origin {
        Origin {
            kind: Kind::Bitmap,
            ..Origin::default()
        }
    }

    fn only_on_and_off(g: &Glyph) -> bool {
        g.pixels().iter().all(|&p| p == 0 || p == 255)
    }

    #[test]
    fn the_baseline_is_the_glyph_itself() {
        let g = rows_from(8, 16, 4);
        assert_eq!(
            g.scripted(Script::Baseline, &bitmap(), Band::NONE).unwrap(),
            g
        );
    }

    // D1: with no line spacing the text band is the cell, so a full 8x16
    // cell scales to 5x11, centred from side to side, against its top or
    // its bottom
    // covers: sgr.script
    #[test]
    fn superscript_sits_at_the_top_and_subscript_at_the_bottom() {
        let full = rows_from(8, 16, 0);
        for origin in [bitmap(), Origin::default()] {
            let sup = full.scripted(Script::Super, &origin, Band::NONE).unwrap();
            let sub = full.scripted(Script::Sub, &origin, Band::NONE).unwrap();
            assert_eq!((sup.width(), sup.height()), (8, 16));
            assert_eq!(ink_rows(&sup), (0..11).collect::<Vec<_>>());
            assert_eq!(ink_rows(&sub), (5..16).collect::<Vec<_>>());
            assert_eq!(ink_cols(&sup), (1..6).collect::<Vec<_>>());
            assert_eq!(ink_cols(&sub), ink_cols(&sup));
            assert!(sup.pixel(3, 5).unwrap() == [255]);
            assert!(sub.pixel(3, 10).unwrap() == [255]);
        }
    }

    // D1: with line spacing, the small glyph is two thirds of the text
    // band, against the band's top or bottom, not the cell's
    // covers: sgr.script
    #[test]
    fn a_script_glyph_sits_against_the_text_band_not_the_cell() {
        let band = Band { above: 3, below: 3 };
        // a 16-row text band in a 22-row cell, the band full of ink
        let mut g = Glyph::mask(8, 22).unwrap();
        g.fill(0, 3, 8, 19, 255);
        for origin in [bitmap(), Origin::default()] {
            let sup = g.scripted(Script::Super, &origin, band).unwrap();
            let sub = g.scripted(Script::Sub, &origin, band).unwrap();
            assert_eq!(ink_rows(&sup), (3..14).collect::<Vec<_>>(), "{origin:?}");
            assert_eq!(ink_rows(&sub), (8..19).collect::<Vec<_>>(), "{origin:?}");
        }
    }

    // D1: a glyph's own box is what scales, not the blank rows around it
    // covers: sgr.script
    #[test]
    fn the_glyphs_own_box_scales_not_its_cell() {
        // a 12-row bitmap glyph centred in a 16-row cell, 2 blank rows
        // above and below it
        let mut g = Glyph::mask(8, 16).unwrap();
        g.fill(0, 2, 8, 14, 255);
        let own = Origin {
            own: Some(Rect {
                left: 0,
                top: 2,
                width: 8,
                height: 12,
            }),
            ..bitmap()
        };
        // 12 rows scale to 8, 8 columns to 5
        let sup = g.scripted(Script::Super, &own, Band::NONE).unwrap();
        assert_eq!(ink_rows(&sup), (0..8).collect::<Vec<_>>());
        let sub = g.scripted(Script::Sub, &own, Band::NONE).unwrap();
        assert_eq!(ink_rows(&sub), (8..16).collect::<Vec<_>>());
        assert_eq!(ink_cols(&sub), (1..6).collect::<Vec<_>>());
    }

    // D4: a one-pixel stroke stays one pixel wide at each of the three
    // phases two thirds gives, across and down, and every pixel is fully
    // on or off
    // covers: sgr.script
    #[test]
    fn a_one_pixel_stroke_stays_one_pixel_wide_at_each_phase() {
        for at in 0..9 {
            let mut across = Glyph::mask(9, 9).unwrap();
            across.fill(at, 0, at + 1, 9, 255);
            let mut down = Glyph::mask(9, 9).unwrap();
            down.fill(0, at, 9, at + 1, 255);
            for script in [Script::Super, Script::Sub] {
                let a = across.scripted(script, &bitmap(), Band::NONE).unwrap();
                assert!(only_on_and_off(&a), "column {at}");
                assert_eq!(ink_cols(&a).len(), 1, "column {at}");
                assert_eq!(ink_rows(&a).len(), 6, "column {at}");
                let d = down.scripted(script, &bitmap(), Band::NONE).unwrap();
                assert!(only_on_and_off(&d), "row {at}");
                assert_eq!(ink_rows(&d).len(), 1, "row {at}");
                assert_eq!(ink_cols(&d).len(), 6, "row {at}");
            }
        }
    }

    // D4: each pixel goes to the small pixel that holds its centre, so
    // 8 columns go to 5 as 0 1 | 2 | 3 4 | 5 | 6 7
    #[test]
    fn each_pixel_goes_to_the_small_pixel_holding_its_centre() {
        let mut g = Glyph::mask(8, 16).unwrap();
        g.fill(2, 0, 3, 16, 255);
        g.fill(6, 0, 7, 16, 255);
        let s = g.scripted(Script::Super, &bitmap(), Band::NONE).unwrap();
        // small columns 1 and 4, placed one column in
        assert_eq!(ink_cols(&s), [2, 5]);
    }

    // D7: bold widens the small glyph by a screen pixel, after scaling
    // covers: sgr.script
    #[test]
    fn bold_widens_the_small_glyph() {
        // a stroke in column 3, rows 0 to 13
        let mut g = Glyph::mask(8, 16).unwrap();
        g.fill(3, 0, 4, 14, 255);
        let bold = Origin {
            synth: Style {
                bold: true,
                italic: false,
            },
            ..bitmap()
        };
        let s = g.scripted(Script::Super, &bold, Band::NONE).unwrap();
        assert!(only_on_and_off(&s));
        for y in ink_rows(&s) {
            assert_eq!(row_cols(&s, y), [3, 4], "row {y}");
        }
    }

    // D7: italic shears the small glyph about its own baseline, row by
    // row by whole pixels, as ADR-1302 D4 shears a bitmap font of the
    // small glyph's height
    // covers: sgr.script
    #[test]
    fn italic_shears_the_small_glyph_about_its_baseline() {
        // a stroke in column 3, rows 0 to 13, the baseline under row 13
        let mut g = Glyph::mask(8, 16).unwrap();
        g.fill(3, 0, 4, 14, 255);
        let italic = Origin {
            baseline: Some(14),
            synth: Style {
                bold: false,
                italic: true,
            },
            ..bitmap()
        };
        let s = g.scripted(Script::Super, &italic, Band::NONE).unwrap();
        // the stroke scales to column 3, rows 0 to 9, the baseline under
        // row 9; an 11-row glyph moves each row by 0.2 of its middle's
        // height above the baseline
        let want: Vec<(u32, Vec<u32>)> = (0..10)
            .map(|y| {
                let by = match y {
                    0..=2 => 2,
                    3..=7 => 1,
                    _ => 0,
                };
                (y, vec![3 + by])
            })
            .collect();
        let got: Vec<(u32, Vec<u32>)> = ink_rows(&s)
            .into_iter()
            .map(|y| (y, row_cols(&s, y)))
            .collect();
        assert_eq!(got, want);
    }

    // D8: ink past the glyph's cells stays beside them in its ink box, and
    // ink above and below its row is cut
    // covers: sgr.script
    #[test]
    fn script_ink_is_cut_above_and_below_its_row_only() {
        // an 8x16 glyph whose ink box reaches a cell to the right and 4
        // rows above and below, with a stroke in its column 12 down the
        // whole box
        let g = Glyph::mask(8, 16).unwrap();
        let (bw, bh) = (16u32, 24u32);
        let mut px = vec![0u8; (bw * bh) as usize];
        for y in 0..bh {
            px[(y * bw + 12) as usize] = 255;
        }
        let g = g.with_ink((0, -4), (bw, bh), px).unwrap();
        let s = g.scripted(Script::Super, &bitmap(), Band::NONE).unwrap();
        let ink = s.ink().expect("the ink beside the cell is kept");
        assert_eq!((ink.top(), ink.height()), (0, 16));
        // column 12 goes to small column 7, one in: column 8, past the cell
        let col = u32::try_from(8 - ink.left()).unwrap();
        let rows: Vec<u32> = (0..16)
            .filter(|&y| ink.pixels()[(y * ink.width() + col) as usize] > 0)
            .collect();
        // the stroke's 24 rows, from 4 above the row, scale to rows -3 to
        // 13, cut to the row
        assert_eq!(rows, (0..14).collect::<Vec<_>>());
        assert!(ink_cols(&s).is_empty(), "no ink inside the cell");
    }

    #[test]
    fn a_wide_glyph_scales_across_both_cells() {
        let wide = rows_from(16, 16, 0).with_cells(Width::Wide);
        let sub = wide.scripted(Script::Sub, &bitmap(), Band::NONE).unwrap();
        assert_eq!(sub.cells(), Width::Wide);
        assert_eq!((sub.width(), sub.height()), (16, 16));
        // 16 columns scale to 11, centred: 2 to 12
        assert_eq!(ink_cols(&sub), (2..13).collect::<Vec<_>>());
    }

    // D5: a color glyph scales smoothly and stays premultiplied
    #[test]
    fn a_color_glyph_stays_premultiplied() {
        let mut px = Vec::new();
        for i in 0..8u32 * 16 {
            let a = u8::try_from(i * 2 % 256).unwrap();
            px.extend_from_slice(&[a / 3, a / 2, a, a]);
        }
        let g = Glyph::from_pixels(8, 16, Format::Rgba, px).unwrap();
        for s in [Script::Super, Script::Sub] {
            for origin in [bitmap(), Origin::default()] {
                let out = g.scripted(s, &origin, Band::NONE).unwrap();
                // from_pixels refuses a color byte above its alpha
                let again = Glyph::from_pixels(8, 16, Format::Rgba, out.pixels().to_vec());
                assert!(again.is_some(), "{s:?}");
            }
        }
    }

    // D5: a smooth glyph's small pixel is the average of what it covers
    #[test]
    fn a_smooth_glyph_averages_and_keeps_its_area() {
        // the bottom 8 rows of 16, scaled to 11 rows: 5.33 rows of ink
        let g = rows_from(6, 16, 8)
            .scripted(Script::Super, &Origin::default(), Band::NONE)
            .unwrap();
        let col: Vec<u8> = (0..16).map(|y| g.pixel(2, y).unwrap()[0]).collect();
        assert_eq!(&col[..5], &[0; 5]);
        assert!(col[5] > 0 && col[5] < 255, "row 5 is partly ink: {col:?}");
        assert_eq!(&col[6..11], &[255; 5]);
        assert_eq!(&col[11..], &[0; 5]);
    }

    #[test]
    fn a_one_pixel_glyph_scales_to_one_pixel() {
        let g = rows_from(1, 1, 0);
        for origin in [bitmap(), Origin::default()] {
            assert_eq!(g.scripted(Script::Sub, &origin, Band::NONE).unwrap(), g);
        }
    }

    // D1, D3: two thirds, rounded, and the place against the text band
    #[test]
    fn the_box_is_two_thirds_against_the_text_band() {
        assert_eq!(size((8, 16)), (5, 11));
        assert_eq!(size((16, 32)), (11, 21));
        assert_eq!(size((1, 1)), (1, 1));
        let band = Band { above: 2, below: 1 };
        assert_eq!(place(Script::Super, (5, 11), (8, 19), band), (1, 2));
        assert_eq!(place(Script::Sub, (5, 11), (8, 19), band), (1, 7));
        // the odd spare column goes to the right
        assert_eq!(place(Script::Super, (5, 11), (6, 16), Band::NONE), (0, 0));
    }
}
