//! Superscript and subscript: the scaled glyph variants that SGR 73 and 74
//! ask for, and SGR 75 ends.
//!
//! The codes come from mintty's control sequences
//! (<https://github.com/mintty/mintty/wiki/CtrlSeqs>, "Text attributes and
//! rendering"): 73 starts superscript, 74 starts subscript, and 75 returns
//! to the baseline. The page names the codes and leaves the size and the
//! place of the small glyph to the terminal. vtium chooses them here.
//!
//! A superscript or subscript glyph is the cell's normal glyph scaled to
//! [`SCRIPT_SCALE`], two thirds of its width and two thirds of its height,
//! and centred from side to side in its cells. A superscript glyph's top
//! row is the cell's top row, which raises its baseline by a third of the
//! normal glyph's baseline. A subscript glyph's bottom row is the cell's
//! bottom row, so its descenders end where the normal glyph's do and its
//! letters sit lower than the normal letters. Two thirds is the size the
//! small glyph needs to stay legible in a 16-row cell, where its letters
//! keep about 7 rows.
//!
//! The scaled glyph is drawn from the normal glyph's pixels inside its
//! cells, so a bitmap font, whose glyphs have no outline to draw at
//! another size, scales the same way an outline font's glyph does. Each
//! pixel of the scaled glyph is the average of the pixels of the normal
//! glyph that it covers, weighted by how much of each it covers. So a
//! 1-bit bitmap glyph becomes a mask with gray edges, and a color glyph
//! stays premultiplied. The normal glyph's ink past its cells, an italic
//! glyph's lean, is not scaled: the small glyph fits well inside its cells
//! and needs none.

use crate::face::Style;
use crate::glyph::Glyph;

/// Where a glyph sits in its cell: SGR 73, 74 and 75.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, PartialOrd, Ord, Hash)]
pub enum Script {
    /// The normal glyph: SGR 75, and every cell no SGR 73 or 74 marks.
    #[default]
    Baseline,
    /// The glyph scaled and raised to the top of its cell: SGR 73.
    Super,
    /// The glyph scaled and lowered to the bottom of its cell: SGR 74.
    Sub,
}

/// The scale of a superscript or subscript glyph, as a fraction of the
/// normal glyph's width and height: two thirds.
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

impl Glyph {
    /// This glyph as `script` draws it: itself for the baseline, else
    /// scaled to [`SCRIPT_SCALE`] and placed in the same cells as the
    /// module's documentation says, with no ink past its cells. `None` only
    /// past the bounds, which a glyph of this size never is.
    pub fn scripted(&self, script: Script) -> Option<Glyph> {
        let (w, h) = (self.width(), self.height());
        let (sw, sh) = (scaled(w), scaled(h));
        let x0 = w.saturating_sub(sw) / 2;
        let y0 = match script {
            Script::Baseline => return Some(self.clone()),
            Script::Super => 0,
            Script::Sub => h.saturating_sub(sh),
        };
        let mut out = Glyph::new(w, h, self.format())?.with_cells(self.cells());
        let bpp = usize::try_from(self.format().bytes_per_pixel()).unwrap_or(4);
        // each scaled pixel's area, in the units `overlap` counts in
        let area = u64::from(w).saturating_mul(u64::from(h)).max(1);
        let mut sums = [0u64; 4];
        for dy in 0..sh {
            let (ys, ye) = covered(dy, h, sh);
            for dx in 0..sw {
                let (xs, xe) = covered(dx, w, sw);
                sums.fill(0);
                for sy in ys..=ye {
                    let oy = overlap(dy, sy, h, sh);
                    for sx in xs..=xe {
                        let weight = oy.saturating_mul(overlap(dx, sx, w, sw));
                        let Some(px) = self.pixel(sx, sy) else {
                            continue;
                        };
                        for (sum, &byte) in sums.iter_mut().zip(px) {
                            *sum = sum.saturating_add(u64::from(byte).saturating_mul(weight));
                        }
                    }
                }
                let at = (x0.saturating_add(dx), y0.saturating_add(dy));
                let Some(px) = out.pixel_mut(at.0, at.1) else {
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

    #[test]
    fn the_baseline_is_the_glyph_itself() {
        let g = rows_from(8, 16, 4);
        assert_eq!(g.scripted(Script::Baseline).unwrap(), g);
    }

    // covers: sgr.script
    #[test]
    fn superscript_sits_at_the_top_and_subscript_at_the_bottom() {
        // a full 8x16 cell scales to 5x11, centred from side to side
        let full = rows_from(8, 16, 0);
        let sup = full.scripted(Script::Super).unwrap();
        let sub = full.scripted(Script::Sub).unwrap();
        assert_eq!((sup.width(), sup.height()), (8, 16));
        assert_eq!(ink_rows(&sup), (0..11).collect::<Vec<_>>());
        assert_eq!(ink_rows(&sub), (5..16).collect::<Vec<_>>());
        assert_eq!(ink_cols(&sup), (1..6).collect::<Vec<_>>());
        assert_eq!(ink_cols(&sub), ink_cols(&sup));
        // a full cell stays full where it is scaled to
        assert!(sup.pixel(3, 5).unwrap() == [255]);
        assert!(sub.pixel(3, 10).unwrap() == [255]);
    }

    #[test]
    fn a_partly_covered_pixel_is_gray_and_the_area_is_kept() {
        // the bottom 8 rows of 16, scaled to 11 rows: 5.33 rows of ink
        let g = rows_from(6, 16, 8).scripted(Script::Super).unwrap();
        let col: Vec<u8> = (0..16).map(|y| g.pixel(2, y).unwrap()[0]).collect();
        assert_eq!(&col[..5], &[0; 5]);
        assert!(col[5] > 0 && col[5] < 255, "row 5 is partly ink: {col:?}");
        assert_eq!(&col[6..11], &[255; 5]);
        assert_eq!(&col[11..], &[0; 5]);
        let ink: u32 = col.iter().map(|&c| u32::from(c)).sum();
        // 8 rows of 16 scale to 5.5 rows of 11
        assert!((5 * 255..=6 * 255).contains(&ink), "{ink}");
    }

    #[test]
    fn a_wide_glyph_scales_across_both_cells() {
        let wide = rows_from(16, 16, 0).with_cells(Width::Wide);
        let sub = wide.scripted(Script::Sub).unwrap();
        assert_eq!(sub.cells(), Width::Wide);
        assert_eq!((sub.width(), sub.height()), (16, 16));
        // 16 columns scale to 11, centred: 2 to 12
        assert_eq!(ink_cols(&sub), (2..13).collect::<Vec<_>>());
    }

    #[test]
    fn a_color_glyph_stays_premultiplied() {
        let mut px = Vec::new();
        for i in 0..8u32 * 16 {
            let a = u8::try_from(i * 2 % 256).unwrap();
            px.extend_from_slice(&[a / 3, a / 2, a, a]);
        }
        let g = Glyph::from_pixels(8, 16, Format::Rgba, px).unwrap();
        for s in [Script::Super, Script::Sub] {
            let out = g.scripted(s).unwrap();
            // from_pixels refuses a color byte above its alpha
            let again = Glyph::from_pixels(8, 16, Format::Rgba, out.pixels().to_vec());
            assert!(again.is_some(), "{s:?}");
        }
    }

    #[test]
    fn a_one_pixel_glyph_scales_to_one_pixel() {
        let g = rows_from(1, 1, 0);
        assert_eq!(g.scripted(Script::Sub).unwrap(), g);
    }
}
