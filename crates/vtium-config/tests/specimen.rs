//! A size's specimen (ADR-1201 D4), drawn by the glyph helper the console
//! starts, `vtium-config serve`, under its own sandbox, and laid out in
//! the TUI's process from the glyphs it answers with: no font is parsed in
//! the TUI's process.

use std::path::Path;

use vtium_config::serve::FritText;
use vtium_config::specimen::{LINES, draw, pixel_height};

fn frit(text: &str) -> FritText {
    FritText {
        name: "t".to_owned(),
        text: text.to_owned(),
    }
}

fn helper() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_vtium-config"))
}

/// The widest specimen line, in characters.
fn widest() -> u32 {
    LINES.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u32
}

#[test]
fn a_bitmap_frit_draws_at_whole_multiples_of_its_cell() {
    // unscii-8's cell is 8x8; 16 pixels draws it twice as large
    let p = draw(helper(), &frit("source=builtin:unscii-8\n"), 16).expect("a picture");
    assert_eq!(
        (p.width, p.height),
        (16 * widest(), 16 * LINES.len() as u32)
    );
    assert_eq!(p.rgba.len(), (p.width * p.height * 4) as usize);
    let lit = p.rgba.chunks(4).filter(|px| px[0] > 128).count();
    assert!(lit > 100, "the glyphs drew ink: {lit}");
    assert!(
        p.rgba.chunks(4).all(|px| px[3] == 255),
        "every pixel is opaque"
    );
}

#[test]
fn a_larger_size_draws_a_larger_picture() {
    let small = draw(helper(), &frit("source=builtin:unscii-8\n"), 8).expect("a picture");
    let large = draw(helper(), &frit("source=builtin:unscii-8\n"), 24).expect("a picture");
    assert_eq!(small.height, 8 * LINES.len() as u32);
    assert_eq!(large.height, 24 * LINES.len() as u32);
    assert!(large.width > small.width);
}

#[test]
fn a_frit_that_does_not_read_draws_nothing_and_says_why() {
    let why = draw(helper(), &frit("source=nosuch:x\n"), 16).expect_err("no picture");
    assert!(!why.is_empty());
}

#[test]
fn points_become_pixels_at_the_density() {
    assert_eq!(pixel_height(12, 96), 16);
    assert_eq!(pixel_height(10, 96), 13);
    assert_eq!(pixel_height(12, 144), 24);
    assert_eq!(pixel_height(0, 96), 0);
}

#[test]
fn the_painter_draws_the_built_in_fonts_and_a_frit() {
    let frits = Path::new(env!("CARGO_MANIFEST_DIR")).join("frits");
    let mut paint = vtium_config::tui::painter(frits, helper().to_path_buf());
    for font in ["unifont", "unscii-8", "minspec-16"] {
        let p = paint(font, 16).unwrap_or_else(|e| panic!("{font}: {e}"));
        assert_eq!(p.height, 16 * LINES.len() as u32, "{font}");
    }
}
