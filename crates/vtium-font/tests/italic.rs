//! Synthetic italic of the bitmap faces the console draws itself
//! (ADR-1302 D4): a PSF font and the built-in sets shear a glyph by 0.2
//! about the baseline, one pixel row at a time, at the font's own size,
//! and the sheared glyph carries its ink box where its ink passes its
//! cell (ADR-1302 D6). Bold widens the rows first, and the shear moves the
//! widened rows. Regular and bold glyphs stay upright.

#![cfg(feature = "psf")]

use vtium_font::{Face, Font, Psf, Style, Width};

const ITALIC: Style = Style {
    bold: false,
    italic: true,
};

/// A 256-glyph 8x16 PSF1 font with no table, so each codepoint is its
/// glyph: `H` inked on rows 2 to 11, as kbd's default8x16 and Terminus
/// ter-v16n draw it, so its baseline lies under row 11; `|` a column
/// down every row at the cell's left edge; and `\\` a column down every
/// row at x 6, next to the cell's right edge.
fn font() -> Psf {
    let mut b = vec![0x36, 0x04, 0, 16];
    b.resize(4 + 256 * 16, 0);
    for row in 2..12 {
        b[4 + 0x48 * 16 + row] = 0b1100_0110;
    }
    for row in 0..16 {
        b[4 + 0x7c * 16 + row] = 0b1000_0000;
        b[4 + 0x5c * 16 + row] = 0b0000_0010;
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

// an 8x16 PSF glyph sheared for italic moves its top five rows 2 pixels,
// the next five 1 pixel, the five around the baseline none and its last
// row 1 pixel left (ADR-1302 D4)
// covers: sgr.italic
#[test]
fn an_8x16_psf_glyph_leans_2_1_0_and_back_1() {
    let psf = font();
    assert_eq!(psf.baseline(), 12, "the row under H's lowest ink");
    let bar = psf.draw(&['|'], ITALIC).expect("the bar");
    let ink = bar.ink().expect("the last row passes the cell's left edge");
    assert_eq!((ink.left(), ink.width()), (-1, 9));
    let mut want = vec![Some(3); 5];
    want.extend([Some(2); 5]);
    want.extend([Some(1); 5]);
    want.push(Some(0));
    assert_eq!(first_ink(&bar), want, "column 0 at shifts 2, 1, 0 and -1");
    // the glyph's own pixels stay its ink cut to its cell
    let cell: Vec<Option<usize>> = bar.rows().map(|r| r.iter().position(|&p| p != 0)).collect();
    assert_eq!(
        (cell[0], cell[9], cell[14], cell[15]),
        (Some(2), Some(1), Some(0), None)
    );
    // regular stays upright and carries no box
    let upright = psf.draw(&['|'], Style::default()).expect("the bar");
    assert!(upright.ink().is_none());
    assert!(first_ink(&upright).iter().all(|&c| c == Some(0)));
}

// the shear runs at the font's own size, so at a scale of 2 each row
// moves twice as far, two pixel rows at a time
// covers: sgr.italic
#[test]
fn a_scaled_psf_glyph_leans_by_twice_the_pixels() {
    let mut psf = font();
    psf.set_height(32);
    assert_eq!(psf.scale(), 2);
    let bar = psf.draw(&['|'], ITALIC).expect("the bar");
    assert_eq!((bar.width(), bar.height()), (16, 32));
    let ink = bar.ink().expect("a box");
    assert_eq!((ink.left(), ink.width()), (-2, 18));
    let rows = first_ink(&bar);
    // column 0 is pixels 0 and 1, and the box starts 2 pixels left
    assert_eq!((rows[0], rows[9]), (Some(6), Some(6)), "moved 4");
    assert_eq!((rows[10], rows[19]), (Some(4), Some(4)), "moved 2");
    assert_eq!((rows[20], rows[29]), (Some(2), Some(2)), "not moved");
    assert_eq!((rows[30], rows[31]), (Some(0), Some(0)), "moved 2 left");
    // drawn unscaled for a renderer that scales, it is the glyph at 1
    let font = Font::new(psf);
    let (small, scale) = font
        .glyph_unscaled(&['|'], Width::Narrow, ITALIC)
        .expect("the bar at 1");
    assert_eq!(
        (scale, small.width(), small.ink().map(|i| i.left())),
        (2, 8, Some(-1))
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
    let ink = bar.ink().expect("a box");
    let row = |y: usize| -> Vec<usize> {
        ink.pixels()[y * ink.width() as usize..][..ink.width() as usize]
            .iter()
            .enumerate()
            .filter(|&(_, &p)| p != 0)
            .map(|(x, _)| x)
            .collect()
    };
    // columns 0 and 1, at shifts 2 and -1 from the box's left of -1
    assert_eq!(row(0), [3, 4]);
    assert_eq!(row(15), [0, 1]);
}

// the built-in unifont, a set of 1-bit strikes, leans about the baseline
// its strike states, row 14 (vtium-font's build.rs), so its row 0 moves 3
// and its rows 12 to 15 stay
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
    // rows 0 and 1 move 3, 2 to 6 move 2, 7 to 11 move 1, 12 to 15 none
    let shift = |y: usize| (14 - y as i64 + 2).div_euclid(5) as usize;
    for y in [top, bottom] {
        assert_eq!(at(&leaning, y), Some(x + shift(y)), "row {y}");
    }
}

// ink sheared past the cell's right edge widens the box to the right: x 6
// moves to 8 in the top rows, so the box is the cell and 1 column more;
// bold widens x 6 into 7 first, and the box takes both
// covers: sgr.italic
#[test]
fn ink_sheared_past_the_right_edge_widens_the_box_right() {
    let psf = font();
    let italic = psf.draw(&['\\'], ITALIC).expect("the column");
    let ink = italic.ink().expect("the top rows pass the right edge");
    assert_eq!((ink.left(), ink.width()), (0, 9));
    let mut want = vec![Some(8); 5];
    want.extend([Some(7); 5]);
    want.extend([Some(6); 5]);
    want.push(Some(5));
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
// out, and the glyph is cut to its cells: at a scale of 32, an 8x16
// glyph's box of 9 columns is 288 pixels by 512 rows, 18,432 bytes
#[test]
fn a_box_past_a_chunk_is_left_out() {
    let mut psf = font();
    psf.set_height(512);
    assert_eq!(psf.scale(), 32);
    let bar = psf.draw(&['|'], ITALIC).expect("the bar");
    assert_eq!((bar.width(), bar.height()), (256, 512));
    assert!(bar.ink().is_none(), "no box past a chunk");
    let first: Vec<Option<usize>> = bar.rows().map(|r| r.iter().position(|&p| p != 0)).collect();
    assert_eq!(
        (first[0], first[511]),
        (Some(64), None),
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
