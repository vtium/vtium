//! The Looks tab driven headlessly through the TUI's real loop: the list
//! of sizes, the picture of a size and its sample screen, and the console's
//! own preview. `tui.rs` says how the tests drive the TUI, and
//! `tui_common/` holds the helpers.

mod tui_common;
use tui_common::*;

/// The Looks view's preview draws a fixed screen of sample content,
/// whatever the display's size, at the cell the look gets on that display;
/// the line beside it still counts the real display's grid.
#[test]
fn the_looks_preview_draws_a_fixed_screen_of_sample_content() {
    let mut c = outside().keys(&[TO_LOOKS, DOWN]);
    c.graphics = true;
    let mut wide = inputs();
    wide.display = Some((7680, 2160));
    drive(&mut c, wide).expect("the loop");
    let shot = c.shots.last().expect("a picture");
    assert_eq!(shot.display, vtium_config::tui::shot::PREVIEW_SCREEN);
    let (cw, ch) = (shot.cell.width as usize, shot.cell.height as usize);
    assert_eq!(shot.grid, (1024 / cw, 768 / ch));
    let first = vtium_config::tui::view::text(&shot.screen[0]);
    assert!(first.starts_with("user@desk:~/notes$ ls -l"), "{first}");
    // the line beside it counts the real display's characters and lines
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    let wide_grid = format!(
        "the console will be {} characters wide and {} lines high",
        7680 / cw,
        2160 / ch
    );
    assert!(text.contains(&wide_grid), "{wide_grid}: {screen}");
    // with VTIUM_CONFIG_PREVIEW_PPM=FILE, the picture as the glyph helper
    // draws it, for the maintainer's review
    if let Some(out) = std::env::var_os("VTIUM_CONFIG_PREVIEW_PPM") {
        let helper = vtium_config::specimen::Helper::at(env!("CARGO_BIN_EXE_vtium-config"));
        let frit = vtium_config::tui::frit_text(Path::new("/nonexistent"), &shot.font)
            .expect("a built-in font's frit");
        // the whole screen, where the TUI shows the part of it that fits
        let mut full = shot.clone();
        full.max = vtium_config::tui::shot::PREVIEW_SCREEN;
        let picture =
            vtium_config::specimen::screen(&helper, &frit, &full, "C").expect("the picture");
        let mut ppm = format!("P6\n{} {}\n255\n", picture.width, picture.height).into_bytes();
        for px in picture.rgba.chunks(4) {
            ppm.extend_from_slice(px.get(..3).unwrap_or_default());
        }
        std::fs::write(out, ppm).expect("the picture's file");
    }
}

#[test]
fn each_highlighted_size_shows_its_specimen_and_deletes_the_last() {
    let mut c = outside().keys(&[TO_LOOKS, DOWN, DOWN, TO_FONTS, b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    // 6 pt and 7 pt at 96 dpi, unifont highlighted in the font view, in
    // the area beside the list, 55 columns by 15 rows of 8x16 cells: down
    // to the room the keys and the longest message leave
    let area = (55 * 8, 15 * 16);
    assert_eq!(
        c.pictures,
        [
            ("unifont".to_owned(), 8, area),
            ("unifont".to_owned(), 9, area)
        ]
    );
    // the Fonts view's specimen of unifont at 12 points comes first and
    // last
    let fonts: Vec<(&str, u32)> = c
        .specimens
        .iter()
        .map(|(f, h, _)| (f.as_str(), *h))
        .collect();
    assert_eq!(fonts, [("unifont", 16), ("unifont", 16)]);
    assert_eq!(
        kitty::show(kitty::IMAGE_ID, 2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]),
        b"\x1b_Ga=T,f=32,t=d,i=1201,s=2,v=1,C=1,q=2,m=0;/wAA/wAA//8=\x1b\\"
    );
    assert_eq!(count(&c.written, &shown()), 2);
    // deleted at automatic, when the next size comes, when the Fonts view
    // draws its own, and when the TUI leaves
    assert_eq!(count(&c.written, &kitty::delete(kitty::IMAGE_ID)), 4);
    let last_show = c
        .written
        .windows(shown().len())
        .rposition(|w| w == shown())
        .expect("shown");
    let last_delete = c
        .written
        .windows(kitty::delete(kitty::IMAGE_ID).len())
        .rposition(|w| w == kitty::delete(kitty::IMAGE_ID))
        .expect("deleted");
    assert!(last_delete > last_show, "the last picture is deleted");
}

/// Where the terminal draws no pictures, a size shows what it gives: its
/// cell, its grid and how much of the specimen the font draws itself.
#[test]
fn without_pictures_a_size_shows_its_cell_grid_and_coverage() {
    let mut c = outside().keys(&[TO_LOOKS, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains(
            "With this font and size, each character cell is 8x16 pixels, the console will be 80x30 characters, and the font can display 40 of the 51 characters in the preview."
        ),
        "{screen}"
    );
    // the Fonts view's line at 12 pt, then one summary a size, at 96 dpi:
    // 6 to 12 pt and the highlighted font
    let heights: Vec<u32> = c.summaries.iter().map(|(_, h)| *h).collect();
    assert_eq!(heights, [16, 8, 9, 11, 12, 13, 15, 16]);
    assert!(c.summaries.iter().all(|(f, _)| f == "unifont"));
    assert!(c.pictures.is_empty());
}

/// In a vtium console the Looks view prints the sample screen the picture
/// of a size draws, in its colors, at the console's current size, and its
/// caption says that a size takes effect when vtium starts again.
#[test]
fn a_vtium_console_prints_the_looks_sample_at_its_current_size() {
    let mut c = console().keys(&[TO_LOOKS, DOWN, DOWN]);
    c.graphics = true;
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let rows = preview_rows(&screen, "user@desk:~/notes$ ls -l");
    let sample: Vec<String> = vtium_config::tui::shot::sample_rows()
        .iter()
        .map(|line| line.iter().map(|(t, _)| *t).collect::<String>())
        .collect();
    for (i, line) in sample.iter().enumerate() {
        let row = rows.get(i).map(String::as_str).unwrap_or_default();
        assert!(
            row.ends_with(line.trim()),
            "row {i}: {row:?} for {line:?}\n{screen}"
        );
    }
    let flat = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains(words::CONSOLE_SIZE_CAPTION), "{screen}");
    // the colors are the console's own: the prompt in bright green
    let text = String::from_utf8_lossy(&c.written).into_owned();
    assert!(text.contains("\x1b[92muser@desk"), "the prompt's color");
    assert!(c.pictures.is_empty() && c.cells.is_empty());
}

#[test]
fn automatic_has_no_picture() {
    let mut c = outside().keys(&[TO_LOOKS, DOWN, UP]);
    c.graphics = true;
    run(&mut c);
    let heights: Vec<u32> = c.pictures.iter().map(|(_, h, _)| *h).collect();
    assert_eq!(heights, [8], "6 pt only");
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Automatic lets vtium pick the size"),
        "{screen}"
    );
}

#[test]
fn at_80x25_the_size_list_scrolls_and_its_sentence_shows_whole_or_not_at_all() {
    use vtium_config::tui::words;
    let joined = |screen: &str| screen.lines().map(str::trim).collect::<Vec<_>>().join(" ");
    let top = reach(80, 25, &[TO_LOOKS]);
    assert!(joined(&top).contains(words::CONSOLE_SIZE_CAPTION), "{top}");
    assert!(
        !top.contains("32 pt"),
        "the list is longer than its room: {top}"
    );
    // the last size scrolls into sight, and the sentence stays whole
    let bottom = reach(80, 25, &[TO_LOOKS, b"\x1b[4~"]);
    assert!(bottom.contains("> 32 pt"), "{bottom}");
    let first = words::CONSOLE_SIZE_CAPTION.split(". ").next().unwrap_or("");
    assert_eq!(
        joined(&bottom).contains(first),
        joined(&bottom).contains(words::CONSOLE_SIZE_CAPTION),
        "whole or not at all:\n{bottom}"
    );
}
