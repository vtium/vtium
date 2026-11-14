//! Double-width and double-height lines (DECDWL and DECDHL): the pieces
//! a doubled glyph or decoration is cut into.
//!
//! A line of double size draws each of its cells across two cells of the
//! display. A double-width line draws each glyph at twice its width and
//! at its height. A double-height line draws each glyph at twice its width
//! and twice its height: the top line of the pair draws the glyph's upper
//! half, and the bottom line draws its lower half.
//!
//! The renderer draws such a line as cells of the normal size. Each
//! character cell of the line becomes two display cells, and a wide
//! character becomes four. Each display cell draws one piece of the
//! doubled glyph: one cell wide and one cell tall, cut from the doubled
//! glyph at its column, and from its upper or lower half on a
//! double-height line. Every piece is the size of a normal narrow glyph,
//! so the glyph cache, the GPU path's glyph pools, the decoration sprites
//! and the cell shader draw a piece as they draw any narrow glyph. The CPU
//! path and the GPU path draw the same pieces, so they draw the same bytes.
//!
//! The glyph is doubled by doubling each of its pixels, which is how the
//! DEC terminals drew double-size text from their character ROM. That is
//! the final look for a bitmap font. For an outline font, the font helper
//! is to render the glyph at true double size, so that its strokes stay
//! sharp; [`piece_glyph`] is the one place where that glyph takes the
//! doubled pixels' place.
//!
//! References:
//! - The maintainer's direction on the double-size lines work, 2026-11-14:
//!   bitmap fonts double each pixel, outline fonts draw at true double
//!   size through the font helper, and the VT100's dot stretching stays
//!   out of the text renderer.

use vtium_font::{Face, Font, Glyph, Variant, Width};
use vtium_term::LineSize;

/// Which piece of a glyph or decoration a display cell draws: the glyph
/// itself, or one cell of it doubled. A doubled piece holds the line's
/// size and the piece's column in the doubled glyph, from 0 at its left:
/// 0 and 1 for a narrow glyph, 0 to 3 for a wide one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Piece(u8);

/// The kinds of doubled piece, as the top bits of a [`Piece`] hold them.
const WIDTH: u8 = 1;
const TOP: u8 = 2;
const BOTTOM: u8 = 3;

/// How many sets of decoration sprites there are: the normal set, then
/// the left and right pieces of each of the three doubled kinds.
pub(crate) const SPRITE_SETS: usize = 7;

impl Piece {
    /// The glyph itself, at its own size.
    pub(crate) const WHOLE: Piece = Piece(0);

    /// Column `column` of a glyph doubled on a line of size `size`, or the
    /// whole glyph on a line of the normal size.
    pub(crate) fn of(size: LineSize, column: usize) -> Piece {
        let kind = match size {
            LineSize::Single => return Piece::WHOLE,
            LineSize::DoubleWidth => WIDTH,
            LineSize::DoubleHeightTop => TOP,
            LineSize::DoubleHeightBottom => BOTTOM,
        };
        let column = u8::try_from(column.min(3)).unwrap_or(3);
        Piece(kind << 2 | column)
    }

    /// Whether this is the glyph itself.
    pub(crate) fn is_whole(self) -> bool {
        self.0 == 0
    }

    fn kind(self) -> u8 {
        self.0 >> 2
    }

    fn column(self) -> u32 {
        u32::from(self.0 & 3)
    }

    /// Its own number, which the glyph cache keys and hashes on: 0 for the
    /// whole glyph, and below 16 for every piece.
    pub(crate) fn bits(self) -> u64 {
        u64::from(self.0)
    }

    /// The set of decoration sprites this piece draws from: 0 for the
    /// normal set; otherwise its kind's left or right piece, by the parity
    /// of its column. A decoration spans one cell, so a wide cell's
    /// second cell draws the same sprite as its first, as on a line of the
    /// normal size.
    pub(crate) fn sprite_set(self) -> usize {
        match self.kind() {
            0 => 0,
            kind => usize::from(kind.saturating_sub(1))
                .saturating_mul(2)
                .saturating_add(usize::from(self.0 & 1))
                .saturating_add(1),
        }
    }

    /// The doubled piece that sprite set `set` holds, for `set` from 1 to
    /// [`SPRITE_SETS`] less one.
    pub(crate) fn of_sprite_set(set: usize) -> Piece {
        let index = set.saturating_sub(1);
        let kind = u8::try_from(index / 2)
            .unwrap_or(0)
            .saturating_add(1)
            .min(3);
        let column = u8::try_from(index % 2).unwrap_or(0);
        Piece(kind << 2 | column)
    }
}

/// The piece `piece` of an image `width` by `height` pixels of `bpp` bytes
/// each that spans `cells` cells: one cell, `width / cells` pixels wide
/// and `height` tall, cut from the image with each of its pixels doubled.
/// A double-width piece keeps the image's rows. A top piece shows the
/// upper half of the doubled image, and a bottom piece its lower half; for
/// an odd height the middle row shows once in each. A column past the
/// doubled image is blank.
pub(crate) fn cut(
    src: &[u8],
    (width, height, bpp): (u32, u32, u32),
    cells: u32,
    piece: Piece,
) -> Vec<u8> {
    let cw = width.checked_div(cells.max(1)).unwrap_or(0);
    let bpp = usize::try_from(bpp).unwrap_or(1);
    let row_bytes = usize::try_from(width).unwrap_or(0).saturating_mul(bpp);
    let out_row = usize::try_from(cw).unwrap_or(0).saturating_mul(bpp);
    let mut out = vec![0u8; out_row.saturating_mul(usize::try_from(height).unwrap_or(0))];
    let first = piece.column().saturating_mul(cw);
    for (y, line) in (0..height).zip(out.chunks_exact_mut(out_row.max(1))) {
        let sy = match piece.kind() {
            TOP => y / 2,
            BOTTOM => y.saturating_add(height) / 2,
            _ => y,
        };
        let row_at = usize::try_from(sy).unwrap_or(0).saturating_mul(row_bytes);
        let Some(row) = src.get(row_at..row_at.saturating_add(row_bytes)) else {
            continue;
        };
        for (x, px) in (0..cw).zip(line.chunks_exact_mut(bpp.max(1))) {
            let sx = usize::try_from(first.saturating_add(x) / 2).unwrap_or(usize::MAX);
            let at = sx.saturating_mul(bpp);
            if let Some(s) = row.get(at..at.saturating_add(bpp)) {
                px.copy_from_slice(s);
            }
        }
    }
    out
}

/// `glyph`'s piece `piece` as a one-cell glyph, with each pixel doubled
/// ([`cut`]). The glyph's own pixels are its ink cut to its cells, so the
/// piece holds no ink past its cell, and a line of double size sends no
/// ink to its neighbours.
fn doubled(glyph: &Glyph, piece: Piece) -> Option<Glyph> {
    let cells = match glyph.cells() {
        Width::Wide => 2,
        Width::Narrow => 1,
    };
    let format = glyph.format();
    let (w, h) = (glyph.width(), glyph.height());
    let pixels = cut(
        glyph.pixels(),
        (w, h, format.bytes_per_pixel()),
        cells,
        piece,
    );
    let cw = w.checked_div(cells).unwrap_or(0);
    let piece = Glyph::from_pixels(cw, h, format, pixels)?;
    Some(piece.with_cells(Width::Narrow))
}

/// The glyph that a display cell of a line of double size draws for
/// `text` in `variant`, `width` cells wide: its piece `piece`, from
/// `whole`, the glyph that the font drew at the normal size, each of its
/// pixels `scale` pixels square. Gives the piece and its scale.
///
/// This is the one place where a glyph rendered at double size takes the
/// doubled pixels' place. Today every font's piece is cut from `whole`
/// with each pixel doubled, which is the final look for a bitmap font.
/// The work that renders outline glyphs at true double size in the font
/// helper asks `font` here for the glyph at twice the cell's width, and at
/// twice its height on a double-height line, and cuts the piece from that
/// glyph, at scale 1, when the font has it. The cache keys the piece on
/// its text, variant, width and piece, so nothing else changes.
pub(crate) fn piece_glyph<F: Face>(
    _font: &Font<F>,
    (_text, _width, _variant): (&[char], Width, Variant),
    (whole, scale): (&Glyph, u32),
    piece: Piece,
) -> Option<(Glyph, u32)> {
    Some((doubled(whole, piece)?, scale))
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking, on small known sizes"
)]
mod tests {
    use super::*;
    use vtium_font::Format;

    /// A 2 by 4 mask whose pixel at column x, row y is 10 * y + x + 1.
    fn ramp() -> Vec<u8> {
        (0..4u8)
            .flat_map(|y| (0..2u8).map(move |x| 10 * y + x + 1))
            .collect()
    }

    // A double-width piece doubles each pixel sideways and keeps the rows:
    // the left piece shows the glyph's left column twice, then its right
    // column's first half
    #[test]
    fn a_double_width_piece_doubles_each_pixel_sideways() {
        let left = cut(&ramp(), (2, 4, 1), 1, Piece::of(LineSize::DoubleWidth, 0));
        let right = cut(&ramp(), (2, 4, 1), 1, Piece::of(LineSize::DoubleWidth, 1));
        assert_eq!(left, [1, 1, 11, 11, 21, 21, 31, 31]);
        assert_eq!(right, [2, 2, 12, 12, 22, 22, 32, 32]);
    }

    // A double-height pair shows the glyph's upper half on the top line
    // and its lower half on the bottom line, each row twice
    #[test]
    fn a_double_height_pair_shows_one_half_of_the_glyph_on_each_line() {
        let top = cut(
            &ramp(),
            (2, 4, 1),
            1,
            Piece::of(LineSize::DoubleHeightTop, 0),
        );
        let bottom = cut(
            &ramp(),
            (2, 4, 1),
            1,
            Piece::of(LineSize::DoubleHeightBottom, 1),
        );
        assert_eq!(top, [1, 1, 1, 1, 11, 11, 11, 11]);
        assert_eq!(bottom, [22, 22, 22, 22, 32, 32, 32, 32]);
    }

    // An odd height shows its middle row once in each half, so the two
    // halves together are the glyph at twice its height
    #[test]
    fn an_odd_height_shows_its_middle_row_once_in_each_half() {
        let src = [1u8, 2, 3];
        let top = cut(&src, (1, 3, 1), 1, Piece::of(LineSize::DoubleHeightTop, 0));
        let bottom = cut(
            &src,
            (1, 3, 1),
            1,
            Piece::of(LineSize::DoubleHeightBottom, 0),
        );
        assert_eq!([top, bottom].concat(), [1, 1, 2, 2, 3, 3]);
    }

    // A wide glyph doubles into four pieces, each one cell of it
    #[test]
    fn a_wide_glyph_doubles_into_four_pieces() {
        let src: Vec<u8> = (1..=4).collect();
        let pieces: Vec<Vec<u8>> = (0..4)
            .map(|c| cut(&src, (4, 1, 1), 2, Piece::of(LineSize::DoubleWidth, c)))
            .collect();
        assert_eq!(pieces, [[1, 1], [2, 2], [3, 3], [4, 4]]);
    }

    // A color glyph's pixels move whole, all four bytes of each
    #[test]
    fn a_color_pixel_moves_whole() {
        let src = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let left = cut(&src, (2, 1, 4), 1, Piece::of(LineSize::DoubleWidth, 1));
        assert_eq!(left, [5, 6, 7, 8, 5, 6, 7, 8]);
    }

    // Each sprite set names its piece back, and a wide cell's second cell
    // takes the same set as its first
    #[test]
    fn each_sprite_set_names_its_piece() {
        for set in 1..SPRITE_SETS {
            assert_eq!(Piece::of_sprite_set(set).sprite_set(), set);
        }
        assert_eq!(Piece::WHOLE.sprite_set(), 0);
        let size = LineSize::DoubleHeightBottom;
        assert_eq!(
            Piece::of(size, 2).sprite_set(),
            Piece::of(size, 0).sprite_set()
        );
        assert_eq!(Piece::of(LineSize::Single, 3), Piece::WHOLE);
    }

    // A piece's glyph is one narrow cell of the doubled glyph
    #[test]
    fn a_pieces_glyph_is_one_narrow_cell() {
        let wide = Glyph::from_pixels(4, 1, Format::Mask, vec![1, 2, 3, 4])
            .unwrap()
            .with_cells(Width::Wide);
        let g = doubled(&wide, Piece::of(LineSize::DoubleWidth, 3)).unwrap();
        assert_eq!((g.width(), g.height(), g.cells()), (2, 1, Width::Narrow));
        assert_eq!(g.pixels(), [4, 4]);
    }
}
