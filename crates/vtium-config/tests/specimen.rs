//! A size's specimen (ADR-1201 D4), drawn by the glyph helper the console
//! starts, `vtium-config serve`, under its own sandbox, and laid out in
//! the TUI's process from the glyphs it answers with: no font is parsed in
//! the TUI's process.

use std::path::Path;

use vtium_config::serve::FritText;
use vtium_config::specimen::{Helper, LINES, PICTURE_MAX, Picture, pixel_height};

fn frit(text: &str) -> FritText {
    FritText {
        name: "t".to_owned(),
        text: text.to_owned(),
    }
}

fn helper() -> Helper {
    Helper::at(env!("CARGO_BIN_EXE_vtium-config"))
}

/// The specimen in a locale with no line of its own.
fn draw(helper: Helper, frit: &FritText, height: u32, max: (u32, u32)) -> Result<Picture, String> {
    vtium_config::specimen::draw(&helper, frit, height, max, "C")
}

/// The widest specimen line, in characters.
fn widest() -> u32 {
    LINES.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u32
}

#[test]
fn a_bitmap_frit_draws_at_whole_multiples_of_its_cell() {
    // unscii-8's cell is 8x8; 16 pixels draws it twice as large
    let p = draw(
        helper(),
        &frit("source=builtin:unscii-8\n"),
        16,
        PICTURE_MAX,
    )
    .expect("a picture");
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
    let small =
        draw(helper(), &frit("source=builtin:unscii-8\n"), 8, PICTURE_MAX).expect("a picture");
    let large = draw(
        helper(),
        &frit("source=builtin:unscii-8\n"),
        24,
        PICTURE_MAX,
    )
    .expect("a picture");
    assert_eq!(small.height, 8 * LINES.len() as u32);
    assert_eq!(large.height, 24 * LINES.len() as u32);
    assert!(large.width > small.width);
}

#[test]
fn a_frit_that_does_not_read_draws_nothing_and_says_why() {
    let why = draw(helper(), &frit("source=nosuch:x\n"), 16, PICTURE_MAX).expect_err("no picture");
    assert!(!why.is_empty());
}

#[test]
fn points_become_pixels_at_the_density() {
    assert_eq!(pixel_height(12, 96), 16);
    assert_eq!(pixel_height(10, 96), 13);
    assert_eq!(pixel_height(12, 144), 24);
    assert_eq!(pixel_height(0, 96), 0);
}

/// The TUI's measurer and painter, through the real helper, draw a
/// picture of a screen in the built-in fonts and a frit: the font's cell
/// at the height, the grid it gives on the display, and the display scaled
/// down to the room, its shape kept (ADR-1201 D4).
#[test]
fn the_painter_draws_a_picture_of_the_console_in_the_built_in_fonts_and_a_frit() {
    use vtium_config::specimen::Shot;
    use vtium_config::tui::canvas::Span;
    use vtium_config::tui::color::Style;
    let frits = Path::new(env!("CARGO_MANIFEST_DIR")).join("frits");
    let measure = vtium_config::tui::measurer(frits.clone(), helper());
    let paint = vtium_config::tui::painter(frits, helper(), "C".to_owned());
    for font in ["unifont", "unscii-8", "minspec-16"] {
        let (cell, face_height) = measure(font, 16).unwrap_or_else(|e| panic!("{font}: {e}"));
        let display = (640, 480);
        let grid = ((640 / cell.width) as usize, (480 / cell.height) as usize);
        let line = vec![Span {
            text: "vtium ┌─┐ 0123".to_owned(),
            style: Style::PLAIN,
            raw: false,
        }];
        let shot = Shot {
            font: font.to_owned(),
            height: 16,
            cell,
            face_height,
            display,
            grid,
            screen: vec![line; grid.1],
            palette: "legacy".to_owned(),
            color: true,
            max: (320, 300),
        };
        let p = paint(&shot).unwrap_or_else(|e| panic!("{font}: {e}"));
        // 640x480 into 320x300 keeps the display's 4:3: 320x240
        assert_eq!((p.width, p.height), (320, 240), "{font}");
        assert_eq!(p.rgba.len(), 320 * 240 * 4);
        let lit = p.rgba.chunks(4).filter(|px| px[0] > 0x40).count();
        assert!(lit > 0, "{font}: the text has ink");
    }
}

#[test]
fn a_picture_is_clipped_to_its_area_in_whole_cells() {
    // unscii-8 at 16 pixels: 16x16 cells; 100x40 pixels hold 6 by 2
    let p = draw(helper(), &frit("source=builtin:unscii-8\n"), 16, (100, 40)).expect("a picture");
    assert_eq!((p.width, p.height), (96, 32));
    assert_eq!(p.rgba.len(), 96 * 32 * 4);
}

#[test]
fn a_size_taller_than_its_area_has_no_picture() {
    let why =
        draw(helper(), &frit("source=builtin:unscii-8\n"), 24, (400, 20)).expect_err("too tall");
    assert!(why.contains("preview area"), "{why}");
}

#[test]
fn no_picture_is_larger_than_the_cap() {
    let (w, h) = PICTURE_MAX;
    assert!(
        u64::from(w) * u64::from(h) * 4 <= 4 << 20,
        "at most 4 MiB of pixels"
    );
    // the largest size at the densest display the console believes
    let height = pixel_height(32, 1000);
    match draw(
        helper(),
        &frit("source=builtin:unscii-8\n"),
        height,
        PICTURE_MAX,
    ) {
        Ok(p) => assert!(p.width <= w && p.height <= h),
        Err(why) => assert!(why.contains("preview area"), "{why}"),
    }
}
