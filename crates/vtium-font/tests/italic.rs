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
/// ter-v16n draw it, so its baseline lies under row 11; and `|` a column
/// down every row at the cell's left edge.
fn font() -> Psf {
    let mut b = vec![0x36, 0x04, 0, 16];
    b.resize(4 + 256 * 16, 0);
    for row in 2..12 {
        b[4 + 0x48 * 16 + row] = 0b1100_0110;
    }
    for row in 0..16 {
        b[4 + 0x7c * 16 + row] = 0b1000_0000;
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
// and its rows 13 to 15 stay
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
    // rows 0 to 2 move 3, 3 to 7 move 2, 8 to 12 move 1, 13 to 15 none
    let shift = |y: usize| (14 - y as i64 + 2).div_euclid(5) as usize;
    for y in [top, bottom] {
        assert_eq!(at(&leaning, y), Some(x + shift(y)), "row {y}");
    }
}
