//! Sized runs, the blocks of cells that kitty's text sizing protocol
//! (OSC 66) writes: the drawing of a run's text at its scale, and the
//! piece of that drawing each cell of the run draws.
//!
//! vtium-term stores a sized run as one character that covers a block of
//! cells, `s` rows high and as many columns wide as the run takes. The
//! first cell of each of the run's rows holds the run's text. The renderer
//! draws the whole text once into a block the size of the run's cells,
//! then cuts the block into one piece for each cell. Every piece is the
//! size of a normal narrow glyph, so the glyph cache, the GPU path's glyph
//! pools and the cell shader draw a piece as they draw any narrow glyph,
//! and both paths draw the same bytes. This is how a line of double size
//! draws its doubled glyphs ([`super::double`]).
//!
//! The text is drawn from the font's glyphs at the normal size, one
//! grapheme cluster after another, then scaled into the block. A mask is
//! scaled by taking the nearest pixel, so a bitmap font's pixels are
//! multiplied by whole factors and stay fully on or off, as the DEC
//! terminals drew double-size text from their character ROM. A color glyph
//! is scaled smoothly. The text is drawn at `s` times its size, or at `s`
//! times n/d of it when the run has a fraction. Text wider than its block
//! is drawn smaller, keeping its shape, until it fits. Text drawn at a
//! fraction sits at the top, bottom or middle of its block and at its
//! left, right or middle, as the run's `v` and `h` say. Text without a
//! fraction is centred across its block, and text drawn smaller to fit
//! stands on the block's baseline.
//!
//! References:
//! - ADR-1308 D9: how a sized run is drawn, its key, the bitmap rule, the
//!   color rule and the rule for text wider than its block.
//! - ADR-1308 D2: the keys s, w, n, d, v and h, and what each does.

use vtium_font::{Cell, Face, Font, Format, Glyph, Variant, Width};
use vtium_term::SizedRun;

use crate::mono::Fallback;

/// A sized run's size, as its cells draw it: the keys that decide the
/// drawing of its text, and its block's width in columns. The block is
/// `scale` rows high.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RunSize {
    /// `s`, 1 to 7: the block's rows, and how many times larger than its
    /// own size the text draws.
    pub(crate) scale: u8,
    /// The block's columns, 1 to 49.
    pub(crate) cols: u8,
    /// `n`, 0 to 15.
    pub(crate) numerator: u8,
    /// `d`, 0 to 15: 0 when the text fills its block, and otherwise
    /// above `n`.
    pub(crate) denominator: u8,
    /// `v`: 0 top, 1 bottom, 2 middle.
    pub(crate) vertical: u8,
    /// `h`: 0 left, 1 right, 2 middle.
    pub(crate) horizontal: u8,
}

impl RunSize {
    /// The size of the run whose share a line holds in `run`.
    pub(crate) fn of(run: SizedRun) -> RunSize {
        let size = run.size();
        RunSize {
            scale: size.scale.clamp(1, 7),
            cols: u8::try_from(run.cols().clamp(1, 49)).unwrap_or(49),
            numerator: size.numerator,
            denominator: size.denominator,
            vertical: size.vertical,
            horizontal: size.horizontal,
        }
    }

    /// Whether the text draws at a fraction of the block's scale, so that
    /// `v` and `h` place it.
    fn fraction(self) -> bool {
        self.denominator != 0
    }
}

/// One cell's piece of a sized run: the run's size, and the cell's row
/// and column in the run's block, both counted from 0 at its top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RunPiece {
    pub(crate) size: RunSize,
    pub(crate) row: u8,
    pub(crate) column: u8,
}

/// The bit each field of a packed [`RunPiece`] starts at, and its width:
/// the column, the row, then the size's scale, columns, numerator,
/// denominator, vertical and horizontal alignment. 30 bits in all.
const FIELDS: [(u32, u32); 8] = [
    (0, 6),
    (6, 3),
    (9, 3),
    (12, 6),
    (18, 4),
    (22, 4),
    (26, 2),
    (28, 2),
];

impl RunPiece {
    /// The piece the cell at `column` of the run's share `run` draws, on
    /// a line that holds the run's row `run.row()`; `None` when the column
    /// lies outside the run.
    pub(crate) fn at(run: SizedRun, column: usize) -> Option<RunPiece> {
        let column = column.checked_sub(run.col())?;
        if column >= run.cols() {
            return None;
        }
        Some(RunPiece {
            size: RunSize::of(run),
            row: u8::try_from(run.row().min(6)).unwrap_or(6),
            column: u8::try_from(column.min(48)).unwrap_or(48),
        })
    }

    /// The piece in 30 bits, each field within its width.
    pub(crate) fn bits(self) -> u32 {
        let s = self.size;
        let values = [
            self.column,
            self.row,
            s.scale,
            s.cols,
            s.numerator,
            s.denominator,
            s.vertical,
            s.horizontal,
        ];
        FIELDS
            .into_iter()
            .zip(values)
            .fold(0, |bits, ((shift, width), value)| {
                let max = (1u32 << width).saturating_sub(1);
                bits | (u32::from(value) & max) << shift
            })
    }

    /// The piece that [`RunPiece::bits`] packed into `bits`.
    pub(crate) fn from_bits(bits: u32) -> RunPiece {
        let [
            column,
            row,
            scale,
            cols,
            numerator,
            denominator,
            vertical,
            horizontal,
        ] = FIELDS.map(|(shift, width)| {
            let max = (1u32 << width).saturating_sub(1);
            u8::try_from(bits >> shift & max).unwrap_or(0)
        });
        RunPiece {
            size: RunSize {
                scale,
                cols,
                numerator,
                denominator,
                vertical,
                horizontal,
            },
            row,
            column,
        }
    }
}

/// A rectangle of pixels in a run's block: where the text lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Area {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) w: u32,
    pub(crate) h: u32,
}

/// `v * num / den`, rounded to the nearest whole pixel, half up; 0 when
/// `den` is 0.
fn ratio(v: u32, num: u32, den: u32) -> u32 {
    let wide = u64::from(v)
        .saturating_mul(u64::from(num))
        .saturating_mul(2)
        .saturating_add(u64::from(den));
    let out = wide
        .checked_div(u64::from(den).saturating_mul(2))
        .unwrap_or(0);
    u32::try_from(out).unwrap_or(u32::MAX)
}

/// Where a run of size `size` draws its text in its block, in cells of
/// `cell`, for text `natural` columns wide at its own size, whose baseline
/// lies `baseline` pixels below the top of a cell. `None` when the text
/// draws at no size, as at a fraction of 0.
///
/// The text draws at `s` times its own size, times n/d with a fraction.
/// Where that is wider than the block, the text draws smaller, keeping its
/// shape, until it is as wide as the block. With a fraction, `h` places the
/// text at the block's left, right or middle and `v` at its top, bottom or
/// middle. Without one, the text is centred across the block, and text
/// drawn smaller to fit stands on the block's baseline.
///
/// References:
/// - ADR-1308 D9, "Text wider than its block": drawn smaller until it
///   fits, on the block's baseline, and centred when d is 0.
/// - ADR-1308 D2: n/d scales the text inside the block, and v and h place
///   it.
pub(crate) fn place(size: RunSize, cell: Cell, natural: u32, baseline: u32) -> Option<Area> {
    let scale = u32::from(size.scale.max(1));
    let bw = u32::from(size.cols).saturating_mul(cell.width);
    let bh = scale.saturating_mul(cell.height);
    let (num, den) = match size.fraction() {
        true => (
            scale.saturating_mul(u32::from(size.numerator)),
            u32::from(size.denominator),
        ),
        false => (scale, 1),
    };
    let nw = natural.saturating_mul(cell.width);
    let mut w = ratio(nw, num, den);
    let mut h = ratio(cell.height, num, den);
    if w == 0 || h == 0 {
        return None;
    }
    // text drawn smaller to fit keeps at least one pixel on each side
    if w > bw {
        h = ratio(h, bw, w).max(1);
        w = bw;
    }
    if h > bh {
        w = ratio(w, bh, h).max(1);
        h = bh;
    }
    let (spare_w, spare_h) = (bw.saturating_sub(w), bh.saturating_sub(h));
    let x = match (size.fraction(), size.horizontal) {
        (true, 0) => 0,
        (true, 1) => spare_w,
        _ => spare_w / 2,
    };
    let y = match (size.fraction(), size.vertical) {
        (true, 0) => 0,
        (true, 1) => spare_h,
        (true, _) => spare_h / 2,
        // the block's baseline, less the baseline of the text at the size
        // it draws at
        (false, _) => scale
            .saturating_mul(baseline)
            .saturating_sub(ratio(baseline, h, cell.height))
            .min(spare_h),
    };
    Some(Area { x, y, w, h })
}

/// `text` cut into the grapheme clusters it draws as: a character starts a
/// new cluster unless it takes no cells of its own, such as a combining
/// mark or a variation selector, or it follows U+200D ZERO WIDTH JOINER.
/// vtium-term cuts the run's text by UAX #29 when it writes the run, and a
/// run without `w` holds one cluster, so this cut differs from that one
/// only for a run with `w` whose text holds clusters this cut splits.
pub(crate) fn clusters(text: &[char]) -> impl Iterator<Item = &[char]> {
    let mut rest = text;
    core::iter::from_fn(move || {
        let (&first, tail) = rest.split_first()?;
        let mut len = 1usize;
        let mut joined = first == '\u{200d}';
        for &c in tail {
            if !joined && vtium_term::cluster_width(&[c]) != 0 {
                break;
            }
            joined = c == '\u{200d}';
            len = len.saturating_add(1);
        }
        let (cluster, after) = rest.split_at_checked(len)?;
        rest = after;
        Some(cluster)
    })
}

/// A run's block: its text drawn at its size, in the pixels of all its
/// cells, `cols` cells wide and `scale` cells high.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Block {
    width: u32,
    height: u32,
    format: Format,
    pixels: Vec<u8>,
    /// Whether any of its clusters drew the font's missing glyph.
    pub(crate) missing: bool,
}

/// Copies `glyph`'s pixels into `canvas`, a buffer `cw` pixels wide in
/// `format`, at `x` pixels from its left, clipped to the canvas. A color
/// glyph on a mask canvas gives its alpha.
fn blit(glyph: &Glyph, (canvas, cw, format): (&mut [u8], u32, Format), x: u32) {
    let bpp = usize::try_from(format.bytes_per_pixel()).unwrap_or(1);
    let gbpp = usize::try_from(glyph.format().bytes_per_pixel()).unwrap_or(1);
    let row_bytes = usize::try_from(cw).unwrap_or(0).saturating_mul(bpp);
    let x = usize::try_from(x).unwrap_or(usize::MAX);
    for (src, dst) in glyph.rows().zip(canvas.chunks_exact_mut(row_bytes.max(1))) {
        for (i, px) in src.chunks_exact(gbpp.max(1)).enumerate() {
            let at = x.saturating_add(i).saturating_mul(bpp);
            let Some(out) = dst.get_mut(at..at.saturating_add(bpp)) else {
                break;
            };
            match (glyph.format(), format) {
                (Format::Rgba, Format::Mask) => {
                    if let (Some(o), Some(&a)) = (out.first_mut(), px.get(3)) {
                        *o = a;
                    }
                }
                _ => out.copy_from_slice(px.get(..bpp).unwrap_or(px)),
            }
        }
    }
}

/// The pixel at `(x, y)` of an image `w` pixels wide of `bpp` bytes each.
fn pixel(src: &[u8], (w, bpp): (u32, usize), x: u32, y: u32) -> &[u8] {
    let at = usize::try_from(
        u64::from(y)
            .saturating_mul(u64::from(w))
            .saturating_add(u64::from(x)),
    )
    .unwrap_or(usize::MAX)
    .saturating_mul(bpp);
    src.get(at..at.saturating_add(bpp)).unwrap_or_default()
}

/// Scales `src`, `sw` by `sh` pixels, into `area` of `dst`, an image `dw`
/// pixels wide, both in `format`. A mask takes the nearest pixel, so a
/// whole factor repeats each pixel and a 1-bit glyph stays 1-bit. A color
/// image is sampled between its four nearest pixels, so it scales
/// smoothly.
fn scale_into(
    (src, sw, sh): (&[u8], u32, u32),
    (dst, dw): (&mut [u8], u32),
    area: Area,
    format: Format,
) {
    let bpp = usize::try_from(format.bytes_per_pixel()).unwrap_or(1);
    let row_bytes = usize::try_from(dw).unwrap_or(0).saturating_mul(bpp);
    // the source position of a target pixel's centre, in 256ths of a pixel
    let at = |t: u32, tn: u32, sn: u32| -> u64 {
        u64::from(t)
            .saturating_mul(2)
            .saturating_add(1)
            .saturating_mul(u64::from(sn))
            .saturating_mul(256)
            .checked_div(u64::from(tn).saturating_mul(2))
            .unwrap_or(0)
    };
    let last = |n: u32| u64::from(n.saturating_sub(1));
    for ty in 0..area.h {
        let sy = at(ty, area.h, sh);
        let dy = usize::try_from(area.y.saturating_add(ty)).unwrap_or(usize::MAX);
        let Some(line) = dst.get_mut(dy.saturating_mul(row_bytes)..) else {
            break;
        };
        for tx in 0..area.w {
            let sx = at(tx, area.w, sw);
            let dx = usize::try_from(area.x.saturating_add(tx))
                .unwrap_or(usize::MAX)
                .saturating_mul(bpp);
            let Some(out) = line.get_mut(dx..dx.saturating_add(bpp)) else {
                break;
            };
            match format {
                Format::Mask => {
                    let x = u32::try_from((sx / 256).min(last(sw))).unwrap_or(0);
                    let y = u32::try_from((sy / 256).min(last(sh))).unwrap_or(0);
                    out.copy_from_slice(pixel(src, (sw, bpp), x, y).get(..bpp).unwrap_or(&[0]));
                }
                Format::Rgba => {
                    // the centres of the four nearest pixels, and the
                    // weight of the far ones, in 256ths
                    let (sx, sy) = (sx.saturating_sub(128), sy.saturating_sub(128));
                    let (x0, y0) = ((sx / 256).min(last(sw)), (sy / 256).min(last(sh)));
                    let (x1, y1) = (
                        x0.saturating_add(1).min(last(sw)),
                        y0.saturating_add(1).min(last(sh)),
                    );
                    let (fx, fy) = (sx % 256, sy % 256);
                    let (gx, gy) = (256u64.saturating_sub(fx), 256u64.saturating_sub(fy));
                    let corner = |x: u64, y: u64| {
                        pixel(
                            src,
                            (sw, bpp),
                            u32::try_from(x).unwrap_or(0),
                            u32::try_from(y).unwrap_or(0),
                        )
                    };
                    let quads = [
                        (corner(x0, y0), gx.saturating_mul(gy)),
                        (corner(x1, y0), fx.saturating_mul(gy)),
                        (corner(x0, y1), gx.saturating_mul(fy)),
                        (corner(x1, y1), fx.saturating_mul(fy)),
                    ];
                    for (i, o) in out.iter_mut().enumerate() {
                        let sum = quads.iter().fold(32_768u64, |n, (p, w)| {
                            n.saturating_add(
                                u64::from(p.get(i).copied().unwrap_or(0)).saturating_mul(*w),
                            )
                        });
                        *o = u8::try_from(sum >> 16).unwrap_or(u8::MAX);
                    }
                }
            }
        }
    }
}

impl Block {
    /// The block of a run of size `size` whose text is `text`, drawn in
    /// `variant` from `font`'s glyphs at the normal size, or the stand-in
    /// that `fallback` gives a color glyph on a display that is not full
    /// color. `None` when the block would pass the bounds of its pixels.
    ///
    /// Each grapheme cluster draws in the cells its width gives it, one
    /// after the other. The block is a color image when every cluster
    /// draws a color glyph, and a mask otherwise, in which a color glyph
    /// gives its alpha.
    pub(crate) fn draw<F: Face>(
        font: &Font<F>,
        (text, variant): (&[char], Variant),
        size: RunSize,
        fallback: Option<&Fallback>,
    ) -> Option<Block> {
        let cell = font.cell();
        let mut missing = false;
        let mut glyphs = Vec::new();
        let mut cells = 0u32;
        for cluster in clusters(text) {
            let width = match vtium_term::cluster_width(cluster) {
                2 => Width::Wide,
                _ => Width::Narrow,
            };
            if let Some(drawn) = font.drawn_variant(cluster, width, variant) {
                missing |= drawn.missing;
                let glyph = fallback
                    .and_then(|f| f.glyph(&drawn.glyph, drawn.scale, cluster, width, variant))
                    .unwrap_or(drawn.glyph);
                glyphs.push((glyph, cells.saturating_mul(cell.width)));
            }
            cells = cells.saturating_add(width.cells());
        }
        let format =
            match glyphs.iter().all(|(g, _)| g.format() == Format::Rgba) && !glyphs.is_empty() {
                true => Format::Rgba,
                false => Format::Mask,
            };
        let bpp = usize::try_from(format.bytes_per_pixel()).ok()?;
        let len = |w: u32, h: u32| -> Option<usize> {
            usize::try_from(w)
                .ok()?
                .checked_mul(usize::try_from(h).ok()?)?
                .checked_mul(bpp)
        };
        let nw = cells.checked_mul(cell.width)?;
        let mut natural = vec![0u8; len(nw, cell.height)?];
        for (glyph, x) in &glyphs {
            blit(glyph, (&mut natural, nw, format), *x);
        }
        let width = u32::from(size.cols).checked_mul(cell.width)?;
        let height = u32::from(size.scale.max(1)).checked_mul(cell.height)?;
        let mut pixels = vec![0u8; len(width, height)?];
        let baseline = font.decorations().underline.min(cell.height);
        if let Some(area) = place(size, cell, cells, baseline) {
            scale_into(
                (&natural, nw, cell.height),
                (&mut pixels, width),
                area,
                format,
            );
        }
        Some(Block {
            width,
            height,
            format,
            pixels,
            missing,
        })
    }

    /// The piece of the block at the cell `piece` names, as one narrow
    /// glyph of `cell`; `None` when the piece lies outside the block.
    pub(crate) fn piece(&self, cell: Cell, piece: RunPiece) -> Option<Glyph> {
        let (cw, ch) = (cell.width, cell.height);
        let bpp = usize::try_from(self.format.bytes_per_pixel()).ok()?;
        let x = u32::from(piece.column).checked_mul(cw)?;
        let y = u32::from(piece.row).checked_mul(ch)?;
        if x.checked_add(cw)? > self.width || y.checked_add(ch)? > self.height {
            return None;
        }
        let row = usize::try_from(self.width).ok()?.checked_mul(bpp)?;
        let out_row = usize::try_from(cw).ok()?.checked_mul(bpp)?;
        let left = usize::try_from(x).ok()?.checked_mul(bpp)?;
        let mut out = Vec::with_capacity(out_row.checked_mul(usize::try_from(ch).ok()?)?);
        for line in y..y.checked_add(ch)? {
            let at = usize::try_from(line)
                .ok()?
                .checked_mul(row)?
                .checked_add(left)?;
            out.extend_from_slice(self.pixels.get(at..at.checked_add(out_row)?)?);
        }
        Some(Glyph::from_pixels(cw, ch, self.format, out)?.with_cells(Width::Narrow))
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking, on small known sizes"
)]
mod tests {
    use super::*;

    const CELL: Cell = Cell {
        width: 8,
        height: 16,
    };

    fn size(scale: u8, cols: u8, (n, d): (u8, u8), (v, h): (u8, u8)) -> RunSize {
        RunSize {
            scale,
            cols,
            numerator: n,
            denominator: d,
            vertical: v,
            horizontal: h,
        }
    }

    fn area(x: u32, y: u32, w: u32, h: u32) -> Area {
        Area { x, y, w, h }
    }

    // Every field of a piece survives its packing, at the largest value
    // each can take
    #[test]
    fn a_piece_packs_into_30_bits_and_back() {
        for piece in [
            RunPiece {
                size: size(7, 49, (14, 15), (2, 2)),
                row: 6,
                column: 48,
            },
            RunPiece {
                size: size(1, 1, (0, 0), (0, 0)),
                row: 0,
                column: 0,
            },
        ] {
            assert!(piece.bits() < 1 << 30);
            assert_eq!(RunPiece::from_bits(piece.bits()), piece);
        }
    }

    // A whole scale fills the block, and a fraction of it sits where v
    // and h put it
    // covers: text.kitty-sizing
    #[test]
    fn a_run_places_its_text_by_its_scale_fraction_and_alignment() {
        assert_eq!(
            place(size(2, 2, (0, 0), (0, 0)), CELL, 1, 15),
            Some(area(0, 0, 16, 32))
        );
        assert_eq!(
            place(size(3, 3, (0, 0), (0, 0)), CELL, 1, 15),
            Some(area(0, 0, 24, 48))
        );
        // s=2 at 1/2: the text at its own size, 8 by 16, in a 16 by 32 block
        for ((v, h), at) in [
            ((0, 0), (0, 0)),
            ((1, 1), (8, 16)),
            ((2, 2), (4, 8)),
            ((2, 1), (8, 8)),
        ] {
            let got = place(size(2, 2, (1, 2), (v, h)), CELL, 1, 15);
            assert_eq!(got, Some(area(at.0, at.1, 8, 16)), "v={v} h={h}");
        }
        // s=1 at 1/2 with w=1: two letters at half size fill the cell's
        // width and half its height
        assert_eq!(
            place(size(1, 1, (1, 2), (0, 0)), CELL, 2, 15),
            Some(area(0, 0, 8, 8))
        );
        // s=3 at 2/3: twice the size, in a 24 by 48 block, at the bottom
        assert_eq!(
            place(size(3, 3, (2, 3), (1, 0)), CELL, 1, 15),
            Some(area(0, 16, 16, 32))
        );
    }

    // A fraction of 0 draws the text at no size
    #[test]
    fn a_fraction_of_0_draws_nothing() {
        assert_eq!(place(size(2, 2, (0, 3), (0, 0)), CELL, 1, 15), None);
    }

    // Text wider than its block draws smaller, keeping its shape, as wide
    // as the block, and stands on the block's baseline: four letters at
    // s=1 in one cell draw a quarter of their size, 8 by 4 pixels, whose
    // baseline at 15 / 4, rounded to 4, meets the cell's at 15
    // covers: text.kitty-sizing
    #[test]
    fn text_wider_than_its_block_draws_smaller_on_its_baseline() {
        assert_eq!(
            place(size(1, 1, (0, 0), (0, 0)), CELL, 4, 15),
            Some(area(0, 11, 8, 4))
        );
        // at s=2, three letters in 2 columns: 16 pixels wide, a third of
        // their double size and 11 rows tall, whose baseline lies 15 * 11
        // / 16, rounded to 10, rows down; the block's lies at 30
        assert_eq!(
            place(size(2, 2, (0, 0), (0, 0)), CELL, 3, 15),
            Some(area(0, 20, 16, 11))
        );
    }

    // Text narrower than its block, as a narrow letter given w=2, is
    // centred across it
    // covers: text.kitty-sizing
    #[test]
    fn text_narrower_than_its_block_is_centred() {
        assert_eq!(
            place(size(1, 2, (0, 0), (0, 0)), CELL, 1, 15),
            Some(area(4, 0, 8, 16))
        );
        assert_eq!(
            place(size(2, 4, (0, 0), (0, 0)), CELL, 1, 15),
            Some(area(8, 0, 16, 32))
        );
    }

    // Every size a program can ask for places its text inside its block,
    // for text from one column to far wider than any block
    #[test]
    fn every_size_places_its_text_inside_its_block() {
        for scale in 1..=7u8 {
            for cols in [1u8, 2, 7, 49] {
                for (n, d) in (0..=15u8).flat_map(|d| (0..=15u8).map(move |n| (n, d))) {
                    if d != 0 && n >= d {
                        continue;
                    }
                    for (v, h) in [(0, 0), (1, 1), (2, 2), (2, 0)] {
                        let size = size(scale, cols, (n, d), (v, h));
                        for natural in [1, 2, 49, 4096] {
                            let Some(a) = place(size, CELL, natural, 15) else {
                                assert_eq!((n, d != 0), (0, true), "{size:?} draws at no size");
                                continue;
                            };
                            assert!(
                                a.x + a.w <= u32::from(cols) * 8,
                                "{size:?} {natural}: {a:?}"
                            );
                            assert!(
                                a.y + a.h <= u32::from(scale) * 16,
                                "{size:?} {natural}: {a:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    // A combining mark and a joined emoji sequence stay with the
    // character before them; other characters each start a cluster
    #[test]
    fn the_text_cuts_into_clusters_where_a_character_takes_cells() {
        let text: Vec<char> = "ae\u{301}\u{1f469}\u{200d}\u{1f467}b".chars().collect();
        let got: Vec<&[char]> = clusters(&text).collect();
        assert_eq!(
            got,
            [
                &['a'][..],
                &['e', '\u{301}'],
                &['\u{1f469}', '\u{200d}', '\u{1f467}'],
                &['b']
            ]
        );
    }

    // A mask scaled by a whole factor repeats each pixel, and a color
    // image scaled up blends between its pixels
    #[test]
    fn a_mask_repeats_its_pixels_and_a_color_image_scales_smoothly() {
        let mut out = vec![0u8; 4 * 2];
        scale_into(
            (&[0, 255], 2, 1),
            (&mut out, 4),
            area(0, 0, 4, 2),
            Format::Mask,
        );
        assert_eq!(out, [0, 0, 255, 255, 0, 0, 255, 255]);
        let src = [0, 0, 0, 0, 200, 200, 200, 200];
        let mut out = vec![0u8; 4 * 4];
        scale_into((&src, 2, 1), (&mut out, 4), area(0, 0, 4, 1), Format::Rgba);
        let reds: Vec<u8> = out.chunks(4).map(|p| p[0]).collect();
        assert_eq!(reds, [0, 50, 150, 200]);
    }

    // Each piece is one cell of the block, at its row and column
    #[test]
    fn a_piece_is_one_cell_of_the_block() {
        let block = Block {
            width: 4,
            height: 4,
            format: Format::Mask,
            pixels: (0..16).collect(),
            missing: false,
        };
        let cell = Cell {
            width: 2,
            height: 2,
        };
        let piece = |row, column| RunPiece {
            size: size(2, 2, (0, 0), (0, 0)),
            row,
            column,
        };
        let g = block.piece(cell, piece(1, 1)).unwrap();
        assert_eq!(g.pixels(), [10, 11, 14, 15]);
        assert_eq!(g.cells(), Width::Narrow);
        assert_eq!(block.piece(cell, piece(2, 0)), None);
    }
}
