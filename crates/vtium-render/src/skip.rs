//! Which ink a cell's underline skips, and how wide a gap it keeps: the
//! one rule that the CPU path's `compose.rs`, the ink beside the pieces of
//! magnified characters (`beside.rs`) and the GPU path's cell quads and
//! shader all follow.
//!
//! A text glyph's ink is every pixel with a coverage above 0. The line
//! leaves out each pixel within the gap of that ink along the cell's row,
//! so it stops short of a descender and goes on after it.
//!
//! A color glyph, such as an emoji, draws a picture whose square is mostly
//! a transparent margin. Scaling the picture to the cell leaves faint
//! ringing in that margin, so a color glyph's ink is only the pixels whose
//! alpha is at least [`COLOR_INK`]. The underline lies behind the picture,
//! and the picture's drawn pixels cover it. In a cell at least
//! [`COLOR_GAP_HEIGHT`] pixels tall, the line also keeps the text's gap
//! around the picture's ink, which sets the picture apart from the line.
//! In a shorter cell, a gap of one pixel takes too large a bite out of
//! the line beside a small picture, so the line runs whole behind the
//! picture, right up to its ink.
//!
//! A magnified cell keeps what the normal cell keeps, magnified, so the
//! rule reads the normal cell's height, never the magnified one.
//!
//! References:
//! - ADR-1302 D7: the underline skips its glyph's ink and lies under the
//!   glyph, and the strike-through and overline lie over it.

/// The value of [`COLOR_INK`], as a macro so that the cell shader's
/// `#define` is built from the same literal.
macro_rules! color_ink {
    () => {
        16
    };
}

/// The least alpha, of 255, at which a color glyph's pixel counts as ink
/// that an underline skips. The font helper scales a color bitmap with a
/// Lanczos filter, which leaves faint pixels in the margin around the
/// picture. Of those faint pixels around 65 Noto Color Emoji pictures,
/// scaled to 15, 19 and 38 rows, 98 per cent have an alpha below 16. A
/// pixel at 16 changes the color beneath it by about 6 per cent.
pub(crate) const COLOR_INK: u8 = color_ink!();

/// The cell shader's piece that gives it [`COLOR_INK`].
#[cfg(feature = "gpu")]
pub(crate) const COLOR_INK_DEFINE: &str = concat!("#define COLOR_INK ", color_ink!(), ".0\n");

/// The least height, in pixels, of a normal cell whose underline keeps a
/// gap around a color glyph's ink. At 32 rows and over, a one-pixel gap is
/// at most a thirty-second of the picture's height and reads as a margin
/// around it. The 19-row cells of tier 3's 1280×800, 1366×768 and
/// 7680×2160 displays, and the 16-row cells of tier 2, fall below it; the
/// 38-row cell of 7680×2160 at 32 points keeps the gap.
pub(crate) const COLOR_GAP_HEIGHT: u32 = 32;

/// Whether the underline of a cell whose normal cell is `cell_height`
/// pixels tall skips a color glyph's ink, keeping the text's gap around
/// it. Where it does not, the line runs whole behind the picture.
pub(crate) fn skips_color(cell_height: u32) -> bool {
    cell_height >= COLOR_GAP_HEIGHT
}

/// Whether a glyph's pixel of coverage, or alpha, `a` is ink an underline
/// skips: for a text glyph any coverage above 0; for a color glyph, when
/// `color` says the line skips color ink at all ([`skips_color`]), an
/// alpha of at least [`COLOR_INK`].
pub(crate) fn inked(is_color: bool, a: u8, color: bool) -> bool {
    if is_color {
        color && a >= COLOR_INK
    } else {
        a != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_glyphs_faintest_coverage_is_ink() {
        assert!(inked(false, 1, false));
        assert!(!inked(false, 0, true));
    }

    #[test]
    fn a_color_glyphs_ink_starts_at_the_threshold_and_only_where_it_counts() {
        assert!(!inked(true, COLOR_INK - 1, true));
        assert!(inked(true, COLOR_INK, true));
        assert!(!inked(true, 255, false));
    }

    #[test]
    fn the_gap_around_color_ink_starts_at_32_rows() {
        assert!(!skips_color(19));
        assert!(skips_color(32));
        assert!(skips_color(38));
    }

    #[cfg(feature = "gpu")]
    #[test]
    fn the_shader_reads_the_same_threshold() {
        assert_eq!(
            COLOR_INK_DEFINE,
            format!("#define COLOR_INK {COLOR_INK}.0\n")
        );
    }
}
