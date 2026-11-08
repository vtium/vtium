//! Synthetic italic of the bitmap faces the console draws itself
//! (ADR-1302 D4): a PSF font and the built-in sets shear a glyph by 0.2
//! about the baseline, one pixel row at a time, at the font's own size,
//! and the sheared glyph carries its ink box where its ink passes its
//! cell (ADR-1302 D6). Bold widens the rows first, and the shear moves the
//! widened rows. Regular and bold glyphs stay upright.

#![cfg(feature = "psf")]

use vtium_font::{Drawn, Face, Font, Psf, Style, Width};

const ITALIC: Style = Style {
    bold: false,
    italic: true,
};

/// A 256-glyph 8x16 PSF1 font with no table, so each codepoint is its
/// glyph: `H` inked on rows 2 to 11, as an 8x16 console font draws it,
/// so its baseline lies under row 11; `|` a column
/// down every row at the cell's left edge; `\\` a column down every row
/// at x 6, next to the cell's right edge; and `/` a column down every row
/// at x 7, the cell's last.
fn font() -> Psf {
    let mut b = vec![0x36, 0x04, 0, 16];
    b.resize(4 + 256 * 16, 0);
    for row in 2..12 {
        b[4 + 0x48 * 16 + row] = 0b1100_0110;
    }
    for row in 0..16 {
        b[4 + 0x7c * 16 + row] = 0b1000_0000;
        b[4 + 0x5c * 16 + row] = 0b0000_0010;
        b[4 + 0x2f * 16 + row] = 0b0000_0001;
    }
    Psf::parse(&b).expect("the crafted font")
}

/// The first inked column of each row, counted from the ink box's left
/// edge where the glyph has a box, else from its cell's.
fn first_ink(g: &vtium_font::Glyph) -> Vec<Option<usize>> {
    match g.ink() {
        Some(ink) => ink
            .pixels()
            .chunks(ink.width() as usize)
            .map(|r| r.iter().position(|&p| p != 0))
            .collect(),
        None => g.rows().map(|r| r.iter().position(|&p| p != 0)).collect(),
    }
}

// an 8x16 PSF glyph sheared for italic moves rows 0 to 3 2 pixels and
// rows 4 to 8 1 pixel, and the three rows above the baseline and every
// row below it stay (ADR-1302 D4); a column at the cell's left edge stays
// inside the cell, and one at its right edge passes it
// covers: sgr.italic
#[test]
fn an_8x16_psf_glyph_leans_2_1_and_0() {
    let psf = font();
    assert_eq!(psf.baseline(), 12, "the row under H's lowest ink");
    let bands = |at: [usize; 3]| -> Vec<Option<usize>> {
        let mut want = vec![Some(at[0]); 4];
        want.extend([Some(at[1]); 5]);
        want.extend([Some(at[2]); 7]);
        want
    };
    let bar = psf.draw(&['|'], ITALIC).expect("the bar");
    assert!(bar.ink().is_none(), "no row passes the cell's left edge");
    assert_eq!(first_ink(&bar), bands([2, 1, 0]), "shifts 2, 1 and 0");
    let edge = psf.draw(&['/'], ITALIC).expect("the column");
    let ink = edge.ink().expect("the top rows pass the right edge");
    assert_eq!((ink.left(), ink.width()), (0, 10));
    assert_eq!(first_ink(&edge), bands([9, 8, 7]));
    // the glyph's own pixels stay its ink cut to its cell
    let cell: Vec<Option<usize>> = edge
        .rows()
        .map(|r| r.iter().position(|&p| p != 0))
        .collect();
    assert_eq!((cell[0], cell[8], cell[9]), (None, None, Some(7)));
    // regular stays upright and carries no box
    let upright = psf.draw(&['|'], Style::default()).expect("the bar");
    assert!(upright.ink().is_none());
    assert!(first_ink(&upright).iter().all(|&c| c == Some(0)));
}

/// A 256-glyph PSF1 font 8 pixels wide and `height` tall whose `H` ends
/// in row `baseline - 1`, so the font is sheared about `baseline`, drawn
/// with two-pixel strokes as a console font such as Terminus draws them:
/// `o` has stems in columns 1 and 2 and 5 and 6 over the seven rows above
/// the baseline, closed by its last two rows, and `p` has a stem in
/// columns 1 and 2 from the same top row down to row `descent`.
fn stroked(height: u8, baseline: usize, descent: usize) -> Psf {
    let h = usize::from(height);
    let mut b = vec![0x36, 0x04, 0, height];
    b.resize(4 + 256 * h, 0);
    let mut put = |c: u8, row: usize, bits: u8| b[4 + usize::from(c) * h + row] = bits;
    for row in 2..baseline {
        put(b'H', row, 0b1100_0110);
    }
    for row in baseline - 7..baseline - 2 {
        put(b'o', row, 0b0110_0110);
    }
    for row in baseline - 2..baseline {
        put(b'o', row, 0b0111_1110);
    }
    for row in baseline - 7..=descent {
        put(b'p', row, 0b0110_0000);
    }
    Psf::parse(&b).expect("the crafted font")
}

// from 16 pixels tall, a sheared letter that stands on the baseline moves
// its last two rows of ink and the row above them together, and a
// descender does not move, so no band edge falls on a glyph's last two
// rows and a font with two-pixel strokes shows no notch one row up from
// the bottom of a letter (ADR-1302 D4). Each height takes the baseline of
// the Terminus font of that height, ter-v16n to ter-v32n, and its `g`'s
// lowest row, and the glyph's last row.
// covers: sgr.italic
#[test]
fn from_16_pixels_a_letters_last_rows_lean_together() {
    for (height, baseline, g_low) in [
        (16, 12, 14),
        (18, 15, 17),
        (20, 16, 18),
        (24, 19, 22),
        (32, 26, 30),
    ] {
        for descent in [g_low, usize::from(height) - 1] {
            let psf = stroked(height, baseline, descent);
            assert_eq!(psf.baseline() as usize, baseline, "{height} rows");
            let o = first_ink(&psf.draw(&['o'], ITALIC).expect("o"));
            let last = &o[baseline - 3..baseline];
            assert!(
                last.iter().all(|&x| x.is_some() && x == last[0]),
                "{height} rows: o's last three rows {last:?}"
            );
            let p = first_ink(&psf.draw(&['p'], ITALIC).expect("p"));
            let low = &p[baseline - 3..=descent];
            assert!(
                low.iter().all(|&x| x.is_some() && x == low[0]),
                "{height} rows: p's rows from {} to {descent} {low:?}",
                baseline - 3
            );
        }
    }
}

// the shear runs at the font's own size, so at a scale of 2 each row
// moves twice as far, two pixel rows at a time
// covers: sgr.italic
#[test]
fn a_scaled_psf_glyph_leans_by_twice_the_pixels() {
    let mut psf = font();
    psf.set_height(32);
    assert_eq!(psf.scale(), 2);
    let edge = psf.draw(&['/'], ITALIC).expect("the column");
    assert_eq!((edge.width(), edge.height()), (16, 32));
    let ink = edge.ink().expect("a box");
    assert_eq!((ink.left(), ink.width()), (0, 20));
    let rows = first_ink(&edge);
    // column 7 is pixels 14 and 15
    assert_eq!((rows[0], rows[7]), (Some(18), Some(18)), "moved 4");
    assert_eq!((rows[8], rows[17]), (Some(16), Some(16)), "moved 2");
    assert_eq!((rows[18], rows[31]), (Some(14), Some(14)), "not moved");
    // drawn unscaled for a renderer that scales, it is the glyph at 1
    let font = Font::new(psf);
    let Drawn {
        glyph: small,
        scale,
        ..
    } = font
        .glyph_unscaled(&['/'], Width::Narrow, ITALIC)
        .expect("the column at 1");
    assert_eq!(
        (scale, small.width(), small.ink().map(|i| i.width())),
        (2, 8, Some(10))
    );
}

// bold widens each row first, inside the cell, and the shear moves the
// widened rows
// covers: sgr.bold
#[test]
fn bold_italic_widens_then_leans() {
    let psf = font();
    let both = Style {
        bold: true,
        italic: true,
    };
    let bar = psf.draw(&['|'], both).expect("the bar");
    assert!(bar.ink().is_none(), "the leaning bar stays in its cell");
    let row = |y: usize| -> Vec<usize> {
        bar.rows()
            .nth(y)
            .expect("a row")
            .iter()
            .enumerate()
            .filter(|&(_, &p)| p != 0)
            .map(|(x, _)| x)
            .collect()
    };
    // columns 0 and 1, at shifts 2 and 0
    assert_eq!(row(0), [2, 3]);
    assert_eq!(row(15), [0, 1]);
}

// bold italic widens before it shears: a column in the cell's last
// column has no room to widen inside the cell, so each row of its bold
// italic glyph holds one pixel, as its italic glyph's does. Sheared
// first, the column would have room in the ink box and widen there.
// covers: sgr.bold
#[test]
fn bold_italic_widens_inside_the_cell_before_it_leans() {
    let psf = font();
    let both = Style {
        bold: true,
        italic: true,
    };
    let edge = psf.draw(&['/'], both).expect("the column");
    let ink = edge.ink().expect("the top rows pass the right edge");
    let counts: Vec<usize> = ink
        .pixels()
        .chunks(ink.width() as usize)
        .map(|r| r.iter().filter(|&&p| p != 0).count())
        .collect();
    assert_eq!(counts, [1; 16], "one pixel in each row");
    let italic = psf.draw(&['/'], ITALIC).expect("the column");
    assert_eq!((edge.pixels(), edge.ink()), (italic.pixels(), italic.ink()));
}

// the built-in unifont, a set of 1-bit strikes 16 pixels tall, leans
// about the baseline its strike states, row 14 (vtium-font's build.rs),
// so its row 0 moves 3 and its rows 11 to 15 stay
// covers: sgr.italic
#[cfg(feature = "unifont")]
#[test]
fn the_builtin_unifont_leans_about_its_baseline() {
    let u = vtium_font::Unifont::new(16);
    let upright = u.draw(&['|'], Style::default()).expect("the bar");
    let leaning = u.draw(&['|'], ITALIC).expect("the bar");
    let at = |g: &vtium_font::Glyph, y: usize| {
        g.rows().nth(y).and_then(|r| r.iter().position(|&p| p != 0))
    };
    let x = (0..16).find_map(|y| at(&upright, y)).expect("ink");
    let top = (0..16).find(|&y| at(&upright, y).is_some()).expect("ink");
    let bottom = (0..16)
        .rev()
        .find(|&y| at(&upright, y).is_some())
        .expect("ink");
    // row 0 moves 3, 1 to 5 move 2, 6 to 10 move 1, 11 to 15 none
    let shift = |y: usize| [3, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0][y];
    for y in [top, bottom] {
        assert_eq!(at(&leaning, y), Some(x + shift(y)), "row {y}");
    }
}

// ink sheared past the cell's right edge widens the box to the right: x 6
// moves to 8 in the top four rows, so the box is the cell and 1 column
// more;
// bold widens x 6 into 7 first, and the box takes both
// covers: sgr.italic
#[test]
fn ink_sheared_past_the_right_edge_widens_the_box_right() {
    let psf = font();
    let italic = psf.draw(&['\\'], ITALIC).expect("the column");
    let ink = italic.ink().expect("the top rows pass the right edge");
    assert_eq!((ink.left(), ink.width()), (0, 9));
    let mut want = vec![Some(8); 4];
    want.extend([Some(7); 5]);
    want.extend([Some(6); 7]);
    assert_eq!(first_ink(&italic), want);
    let both = Style {
        bold: true,
        italic: true,
    };
    let bold = psf.draw(&['\\'], both).expect("the column");
    let ink = bold.ink().expect("a box");
    assert_eq!((ink.left(), ink.width()), (0, 10));
    let w = ink.width() as usize;
    let top: Vec<usize> = (0..w).filter(|&x| ink.pixels()[x] != 0).collect();
    assert_eq!(top, [8, 9], "x 6 and 7, moved 2");
}

// ADR-1302 D6: a box whose 1-bit rows would pass a set's chunk is left
// out, and the glyph is cut to its cells: at a scale of 32, the box of an
// 8x16 glyph in the cell's last column, 10 columns, is 320 pixels by 512
// rows, 20,480 bytes
#[test]
fn a_box_past_a_chunk_is_left_out() {
    let mut psf = font();
    psf.set_height(512);
    assert_eq!(psf.scale(), 32);
    let edge = psf.draw(&['/'], ITALIC).expect("the column");
    assert_eq!((edge.width(), edge.height()), (256, 512));
    assert!(edge.ink().is_none(), "no box past a chunk");
    let first: Vec<Option<usize>> = edge
        .rows()
        .map(|r| r.iter().position(|&p| p != 0))
        .collect();
    assert_eq!(
        (first[0], first[511]),
        (None, Some(224)),
        "still sheared, cut"
    );
}

/// A set at 8x16 whose regular 1-bit strike draws `x` as a column at the
/// cell's left edge, and, when `bold` is given, a bold strike that draws
/// it as a solid block.
fn set_with_bold(bold: bool) -> vtium_font::set::SetFace {
    use vtium_font::Cell;
    use vtium_font::set::{PixelFormat, Set, SetBuilder, StrikeBuilder};
    let cell = Cell {
        width: 8,
        height: 16,
    };
    let mut regular = StrikeBuilder::new(PixelFormat::Bits, cell, Style::default()).baseline(12);
    let g = regular.glyph(Width::Narrow, vec![0x80; 16]);
    regular.map('x', g);
    let mut set = SetBuilder::new("t").strike(regular);
    if bold {
        let mut b = StrikeBuilder::new(
            PixelFormat::Bits,
            cell,
            Style {
                bold: true,
                italic: false,
            },
        )
        .baseline(12);
        let g = b.glyph(Width::Narrow, vec![0xff; 16]);
        b.map('x', g);
        set = set.strike(b);
    }
    let set = Set::from_bytes(set.write().expect("written")).expect("read");
    vtium_font::set::SetFace::new(std::sync::Arc::new(set), 16).expect("a face")
}

// the maintainer, 2026-11-04: bold italic with no strike of its own is the
// regular glyph widened and then sheared, and never the bold strike
// sheared
// covers: sgr.bold
#[test]
fn a_sets_bold_italic_comes_from_its_regular_strike() {
    let both = Style {
        bold: true,
        italic: true,
    };
    let with_bold = set_with_bold(true).draw(&['x'], both).expect("x");
    let regular_only = set_with_bold(false).draw(&['x'], both).expect("x");
    assert_eq!(with_bold, regular_only);
    let top: Vec<usize> = with_bold
        .rows()
        .next()
        .expect("a row")
        .iter()
        .enumerate()
        .filter(|&(_, &p)| p != 0)
        .map(|(x, _)| x)
        .collect();
    assert_eq!(top, [2, 3], "columns 0 and 1, moved 2");
}
