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
//! A built-in sprite, such as a box drawing line, a block or a DEC special
//! graphics character, is drawn again in the doubled cell
//! ([`vtium_font::sprite::doubled`]). Its lines keep the weight the sprite
//! rule gives that cell and still meet the cell's edges, so they join the
//! sprite in the next cell. Where the font's face holds the glyph rendered
//! at double size ([`Face::doubled`]), each piece is cut from that glyph as
//! it is. The font helper renders an outline font's glyphs that way, color
//! emoji and clusters among them, so their strokes stay sharp at the
//! larger size. Otherwise each piece is cut from the glyph at the normal
//! size with each of its pixels doubled, which is how the DEC terminals
//! drew double-size text from their character ROM. That is the look of a
//! bitmap font, and of any other glyph until the helper's double-size
//! glyph arrives. [`doubled_glyph`] and [`piece_glyph`] are the one place
//! that chooses among the three.
//!
//! References:
//! - The maintainer's direction on the double-size lines work, 2026-11-14:
//!   bitmap fonts double each pixel, outline fonts draw at true double
//!   size through the font helper, and the VT100's dot stretching stays
//!   out of the text renderer.

use vtium_font::{Doubled, Face, Font, Format, Glyph, Script, Variant, Width, sprite};
use vtium_term::LineSize;

use crate::mono::Fallback;

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

    /// The double size the piece's glyph is drawn at: `None` for the whole
    /// glyph, and the same size for the top and the bottom pieces, which
    /// cut the two halves of one glyph.
    pub(crate) fn doubled(self) -> Option<Doubled> {
        match self.kind() {
            WIDTH => Some(Doubled::Width),
            TOP | BOTTOM => Some(Doubled::Height),
            _ => None,
        }
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

/// Piece `piece` of `glyph`, a glyph `width` cells wide drawn at the
/// piece's double size in cells of `cell`, as one narrow glyph of `cell`:
/// the cell at the piece's column, and on a line of double height the
/// glyph's upper or lower half. The glyph's pixels are its ink cut to its
/// cells, so the piece holds no ink past its cell. `None` when the glyph
/// is not the size that double size gives `width` cells of `cell`.
fn cut_drawn(
    glyph: &Glyph,
    (cell, width): (vtium_font::Cell, Width),
    piece: Piece,
) -> Option<Glyph> {
    let doubled = piece.doubled()?;
    let (cw, ch) = (cell.width, cell.height);
    let size = (
        cw.checked_mul(2)?.checked_mul(width.cells())?,
        ch.checked_mul(doubled.rows())?,
    );
    if (glyph.width(), glyph.height()) != size || glyph.cells() != width {
        return None;
    }
    let bpp = usize::try_from(glyph.format().bytes_per_pixel()).ok()?;
    let row = usize::try_from(size.0).ok()?.checked_mul(bpp)?;
    let out_row = usize::try_from(cw).ok()?.checked_mul(bpp)?;
    let left = usize::try_from(piece.column().checked_mul(cw)?)
        .ok()?
        .checked_mul(bpp)?;
    let first = match piece.kind() {
        BOTTOM => ch,
        _ => 0,
    };
    let mut out = Vec::with_capacity(out_row.checked_mul(usize::try_from(ch).ok()?)?);
    for y in first..first.checked_add(ch)? {
        let at = usize::try_from(y)
            .ok()?
            .checked_mul(row)?
            .checked_add(left)?;
        out.extend_from_slice(glyph.pixels().get(at..at.checked_add(out_row)?)?);
    }
    let piece = Glyph::from_pixels(cw, ch, glyph.format(), out)?;
    Some(piece.with_cells(Width::Narrow))
}

/// The glyph that a line of double size draws for `text` in `variant`,
/// `width` cells wide, at the double size `size`, which each of its pieces
/// is cut from ([`piece_glyph`]). `None` where the line doubles the pixels
/// of the glyph at the normal size instead.
///
/// A character that `font` draws as a built-in sprite, such as a box
/// drawing line or a block, is drawn again at the double size
/// ([`sprite::doubled`]), so its lines keep their weight and join the next
/// cell. Otherwise it is the glyph that `font`'s face holds rendered at the
/// double size ([`Face::doubled`]), as the font helper renders an outline
/// font's, a color glyph's and a cluster's.
///
/// On a display that is not full color, `fallback` stands in for the
/// glyph at the double size as it stands in for the glyph at the normal
/// size, so a line of double size shows what a line of the normal size
/// shows. A sprite that a 1-bit display would show as gray is drawn as its
/// 1-bit shape at the double size, such as a shade's ordered pattern. A
/// color glyph that unifont has gives `None`: the glyph at the normal size
/// is already unifont's bitmap, and the line doubles its pixels.
pub(crate) fn doubled_glyph<F: Face>(
    font: &Font<F>,
    (text, width, variant): (&[char], Width, Variant),
    size: Doubled,
    fallback: Option<&Fallback>,
) -> Option<Glyph> {
    let sprite = (variant.script == Script::Baseline)
        .then(|| sprite::doubled(font, text, (width, variant.style), size))
        .flatten();
    let glyph = sprite.or_else(|| font.face().doubled(text, variant, size))?;
    match fallback.and_then(|f| f.glyph(&glyph, 1, text, width, variant)) {
        Some(_) if glyph.format() == Format::Rgba => None,
        Some(stand_in) => Some(stand_in.with_cells(glyph.cells())),
        None => Some(glyph),
    }
}

/// The piece `piece` of a glyph `width` cells wide on a line of double
/// size, in a font of cell `cell`, and the scale of its pixels. The piece
/// is cut at scale 1 from `drawn`, the glyph drawn at the double size that
/// [`doubled_glyph`] gives. Without it, the piece is cut from `whole`, the
/// glyph drawn at the normal size, each of its pixels `scale` pixels
/// square, with each pixel doubled. The glyph cache keys the piece on its
/// text, variant, width and piece, and forgets it when a glyph for its text
/// arrives, so the piece is cut again from the helper's glyph once that
/// glyph is there.
pub(crate) fn piece_glyph(
    drawn: Option<&Glyph>,
    (whole, scale): (&Glyph, u32),
    (cell, width): (vtium_font::Cell, Width),
    piece: Piece,
) -> Option<(Glyph, u32)> {
    match drawn.and_then(|glyph| cut_drawn(glyph, (cell, width), piece)) {
        Some(glyph) => Some((glyph, 1)),
        None => Some((doubled(whole, piece)?, scale)),
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking, on small known sizes"
)]
mod tests {
    use super::*;

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

    /// Which rows and which columns of `glyph`, a mask, hold any ink.
    fn inked(glyph: &Glyph) -> (Vec<u32>, Vec<u32>) {
        let lit = |x: u32, y: u32| {
            glyph
                .pixel(x, y)
                .is_some_and(|p| p.first().is_some_and(|&a| a != 0))
        };
        let rows = (0..glyph.height())
            .filter(|&y| (0..glyph.width()).any(|x| lit(x, y)))
            .collect();
        let columns = (0..glyph.width())
            .filter(|&x| (0..glyph.height()).any(|y| lit(x, y)))
            .collect();
        (rows, columns)
    }

    /// The piece `piece` of `c`'s glyph in unifont's 8x16 cell, which
    /// draws box drawing as sprites, and its scale.
    fn unifont_piece(c: char, piece: Piece) -> (Glyph, u32) {
        let font = Font::new(vtium_font::Unifont::new(16));
        let key = (&[c][..], Width::Narrow, Variant::default());
        let whole = font.glyph(&[c], Width::Narrow, Default::default()).unwrap();
        let drawn = doubled_glyph(&font, key, piece.doubled().unwrap(), None);
        piece_glyph(
            drawn.as_ref(),
            (&whole, 1),
            (font.cell(), Width::Narrow),
            piece,
        )
        .unwrap()
    }

    // A box drawing line on a line of double width is drawn again in the
    // doubled cell, 16 pixels wide, so its vertical line stays one pixel
    // thick, at the doubled cell's middle, column 7 of the left piece. A
    // piece of the line at the normal size with its pixels doubled would
    // be two pixels thick. The line runs through every row, so it joins
    // the line above and below.
    // covers: text.line-size
    #[test]
    fn a_box_drawing_line_on_a_double_width_line_keeps_its_weight() {
        let (left, scale) = unifont_piece('│', Piece::of(LineSize::DoubleWidth, 0));
        let (right, _) = unifont_piece('│', Piece::of(LineSize::DoubleWidth, 1));
        assert_eq!(scale, 1);
        assert_eq!((left.width(), left.height()), (8, 16));
        assert_eq!(inked(&left), ((0..16).collect(), vec![7]));
        assert_eq!(inked(&right), (vec![], vec![]));
    }

    // A horizontal line on a double-height pair is drawn in the doubled
    // cell, 16 by 32 pixels, with the light line that cell's height gives:
    // two rows, 15 and 16, which fall on the last row of the top piece and
    // the first row of the bottom piece. The line runs across every column
    // of both pieces, so it joins the next cell's line.
    // covers: text.line-size
    #[test]
    fn a_box_drawing_line_on_a_double_height_pair_takes_the_taller_cells_weight() {
        for column in [0, 1] {
            let (top, _) = unifont_piece('─', Piece::of(LineSize::DoubleHeightTop, column));
            let (bottom, _) = unifont_piece('─', Piece::of(LineSize::DoubleHeightBottom, column));
            assert_eq!(inked(&top), (vec![15], (0..8).collect()));
            assert_eq!(inked(&bottom), (vec![0], (0..8).collect()));
        }
    }

    // A full block fills both pieces of a double-width cell to their edges
    #[test]
    fn a_block_fills_each_piece_of_its_doubled_cell() {
        for column in [0, 1] {
            let (piece, _) = unifont_piece('█', Piece::of(LineSize::DoubleWidth, column));
            assert_eq!(inked(&piece), ((0..16).collect(), (0..8).collect()));
        }
    }

    // A letter is no sprite, so a bitmap font's letter still doubles its
    // pixels at the scale of the glyph it is cut from
    #[test]
    fn a_letter_of_a_bitmap_font_still_doubles_its_pixels() {
        let font = Font::new(vtium_font::Unifont::new(16));
        let whole = font
            .glyph(&['a'], Width::Narrow, Default::default())
            .unwrap();
        let key = (&['a'][..], Width::Narrow, Variant::default());
        let piece = Piece::of(LineSize::DoubleWidth, 0);
        let drawn = doubled_glyph(&font, key, Doubled::Width, None);
        assert_eq!(
            drawn, None,
            "a letter of a bitmap font is drawn at its normal size"
        );
        let (glyph, _) = piece_glyph(
            drawn.as_ref(),
            (&whole, 1),
            (font.cell(), Width::Narrow),
            piece,
        )
        .unwrap();
        assert_eq!(Some(glyph), doubled(&whole, piece));
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
