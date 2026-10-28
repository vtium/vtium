//! The test fonts read back through vtium-font's PSF loader, glyph for
//! glyph.

#![expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "a test fails by panicking"
)]

use vtium_font::{Face, Psf, Style, Unifont};
use vtium_testfont::{Bitmap, GLYPHS, files, fold, repertoire, small8x8, small8x16};

/// Each glyph of `font` as the loader draws it, row bytes from its mask.
fn drawn(psf: &Psf, c: char) -> Vec<u8> {
    let g = psf.draw(&[c], Style::default()).unwrap();
    assert_eq!(g.width(), 8);
    g.rows()
        .map(|row| row.iter().fold(0u8, |b, &px| (b << 1) | u8::from(px != 0)))
        .collect()
}

fn check(font: &Bitmap, psf: &Psf) {
    assert_eq!(psf.glyph_count() as usize, GLYPHS);
    assert_eq!(psf.glyph_size(), (8, font.height as u32));
    assert!(psf.has_table());
    for (i, c) in repertoire().enumerate() {
        assert_eq!(psf.glyph_index(c), Some(i as u32), "U+{:04X}", u32::from(c));
        assert_eq!(drawn(psf, c), font.glyphs[i], "U+{:04X}", u32::from(c));
    }
    // the blank glyphs at the end map from nothing
    assert!(!psf.has_glyph('\u{2500}'));
    assert!(!psf.has_glyph('\u{ad}'));
}

#[test]
fn the_8x8_font_is_psf2_and_reads_back() {
    let font = small8x8().unwrap();
    let bytes = font.psf2().unwrap();
    assert_eq!(bytes[..4], [0x72, 0xb5, 0x4a, 0x86]);
    check(&font, &Psf::parse(&bytes).unwrap());
}

#[test]
fn the_8x16_font_is_psf1_and_reads_back() {
    let font = small8x16().unwrap();
    let bytes = font.psf1().unwrap();
    assert_eq!(bytes[..4], [0x36, 0x04, 0x02, 16]);
    check(&font, &Psf::parse(&bytes).unwrap());
}

// The 8x16 font draws as unifont does; the 8x8 one is its fold.
#[test]
fn the_glyphs_are_unifonts() {
    let unifont = Unifont::new(16);
    let (big, small) = (small8x16().unwrap(), small8x8().unwrap());
    for (i, c) in repertoire().enumerate() {
        let g = unifont.draw(&[c], Style::default()).unwrap();
        let rows: Vec<u8> = g
            .rows()
            .map(|row| row.iter().fold(0u8, |b, &px| (b << 1) | u8::from(px != 0)))
            .collect();
        assert_eq!(big.glyphs[i], rows);
        assert_eq!(small.glyphs[i], fold(&rows));
    }
}

// The fold keeps a capital's rows 4 to 13 in rows 0 to 6 and merges the
// pairs it names.
#[test]
fn the_fold_keeps_capitals_and_descenders() {
    let rows: Vec<u8> = (0..16).map(|r| 1 << (r % 8)).collect();
    assert_eq!(
        fold(&rows),
        [0x1f, 0x20, 0xc0, 0x01, 0x02, 0x0c, 0x30, 0xc0]
    );
}

#[test]
fn every_run_writes_the_same_bytes() {
    assert_eq!(files().unwrap(), files().unwrap());
}
