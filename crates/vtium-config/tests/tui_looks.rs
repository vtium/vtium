//! The Looks tab driven headlessly through the TUI's real loop: the list
//! of looks and font sets, a look's card and its lists, a font set's
//! detail, the picture of a look's size and its sample screen, and the
//! console's own preview. `tui.rs` says how the tests drive the TUI, and
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
    let mut wide = sized();
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
    assert_eq!(drive(&mut c, sized()).expect("the loop"), 0);
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

/// Where the terminal draws no pictures, a look's size shows what it
/// gives: its cell, its grid and how much of the specimen the font draws
/// itself.
#[test]
fn without_pictures_a_size_shows_its_cell_grid_and_coverage() {
    // a look that draws unifont at 12 pt
    let mut twelve = inputs();
    twelve.conf = Some(
        "# mine\nfont-name=unifont\nversion = 1\n[look twelve]\nfont-name = unifont\nfont-size = 12\n".to_owned(),
    );
    let mut c = outside().keys(&[TO_LOOKS]);
    drive(&mut c, twelve).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains(
            "With this font and size, each character cell is 8x16 pixels, the console will be 80x30 characters, and the font can display 40 of the 51 characters in the preview."
        ),
        "{screen}"
    );
    // the Fonts view's line at 12 pt, then one summary a size, at 96 dpi:
    // the Fonts tab's highlighted font, then the look at 12 pt
    let heights: Vec<u32> = c.summaries.iter().map(|(_, h)| *h).collect();
    assert_eq!(heights, [16, 16]);
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
    drive(&mut c, sized()).expect("the loop");
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
    // the caption keeps to the names' column, beside the console's text
    let column: Vec<String> = screen
        .lines()
        .map(|r| r.chars().take(24).collect::<String>())
        .collect();
    let flat = column
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
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
    drive(&mut c, sized()).expect("the loop");
    let heights: Vec<u32> = c.pictures.iter().map(|(_, h, _)| *h).collect();
    assert_eq!(heights, [8], "6 pt only");
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("Automatic lets vtium pick the size"),
        "{screen}"
    );
}

/// The Esc key, alone.
const ESC: &[u8] = b"\x1b";

use vtium_config::tui::words;

/// The text of the names' column of `screen`, beside the console's own
/// preview, joined into one line.
fn column(screen: &str) -> String {
    screen
        .lines()
        .map(|r| r.chars().take(24).collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The rows of `screen` under the tab bar's rule, trimmed at the end.
fn body(screen: &str) -> Vec<String> {
    screen
        .lines()
        .skip(3)
        .map(|r| r.trim_end().to_owned())
        .collect()
}

/// The Looks tab lists the draft's looks under their heading, then the
/// font sets under theirs, and says what the highlighted item is.
///
/// References:
/// - ADR-1403 D10: the tab's one list in groups, the looks first.
#[test]
fn the_looks_tab_lists_the_looks_then_the_font_sets() {
    let screen = reach(100, 30, &[TO_LOOKS]);
    let rows = body(&screen);
    let names: Vec<String> = rows
        .iter()
        .take(11)
        .map(|r| r.chars().take(24).collect::<String>().trim_end().to_owned())
        .collect();
    assert_eq!(
        names,
        [
            " Looks",
            "   proof",
            "",
            " Font sets",
            "   broken",
            "   minspec-16",
            "   proof",
            "   tiny5",
            "   unifont",
            "   unscii-8",
            "",
        ],
        "{screen}"
    );
    assert!(
        column(&screen)
            .contains("proof uses the font set proof at vtium.conf's size. Terminal desk uses it."),
        "{screen}"
    );
}

/// Enter on a look opens its card: its six lines with the look's values,
/// vtium.conf's where the look leaves a key unset, and below them its font
/// set's fonts by role and what they miss. The size line says that the
/// sizes are vtium-config's usual ones.
///
/// References:
/// - ADR-1205 D7: a look offers the sizes its fonts supply.
/// - ADR-1205 D8: a look shows what it still misses, by role.
/// - ADR-1403 D10: the card's lines.
#[test]
fn enter_on_a_look_opens_its_card_with_its_lines_and_its_fonts_by_role() {
    let screen = reach(100, 30, &[TO_LOOKS, b"\r"]);
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    for line in [
        "Name proof",
        "Font set proof",
        "Size automatic, from vtium.conf.",
        "Color scheme legacy, from vtium.conf",
        "vtlook sepia",
        "Pointer theme vtium's own pictures, from vtium.conf.",
        words::SIZES_USUAL,
        "Its fonts, by role:",
        "Text: the font family JetBrains Mono, not on this machine",
        "Box drawing: vtium's own lines, unless the text font has them all",
        "What it still misses:",
        "Text: the font family JetBrains Mono is not on this machine: no such family.",
    ] {
        assert!(text.contains(line), "{line:?}:\n{screen}");
    }
}

/// The color scheme line's list starts with the scheme vtium.conf sets,
/// which leaves the look's own unset, then the nine built-in schemes, then
/// `custom`, which takes vtium.conf's palette-0 to palette-15.
///
/// References:
/// - ADR-1205 D10: a look picks a built-in scheme or a scheme file, and
///   vtium.conf's `palette` stays the default.
#[test]
fn the_color_scheme_list_starts_with_vtium_conf_s_scheme_then_the_built_in_ones() {
    let screen = reach(100, 30, &[TO_LOOKS, b"\r", DOWN, DOWN, DOWN, b"\r"]);
    let rows: Vec<String> = body(&screen)
        .into_iter()
        .filter(|r| r.starts_with("   ● ") || r.starts_with("   ○ "))
        .collect();
    assert_eq!(rows.len(), 13, "{screen}");
    assert!(rows[10].starts_with("   ○ custom "), "{screen}");
    assert!(
        rows[0].starts_with("   ● The color scheme vtium.conf sets (legacy)"),
        "{screen}"
    );
    for (row, name) in rows.iter().skip(1).zip(vtium_term::PALETTE_NAMES) {
        assert!(
            row.starts_with(&format!("   ○ {name} ")),
            "{row:?}: {screen}"
        );
    }
    // then the active scheme files, each once: the package's dusk, which
    // the administrator's dusk hides, and the files that are not active
    // are left out
    assert!(rows[11].starts_with("   ○ paper "), "{screen}");
    assert!(rows[12].starts_with("   ○ dusk "), "{screen}");
    assert!(rows[12].contains("scheme file"), "{screen}");
}

/// The vtlook line's list comes in the Effects tab's order, No filter
/// first, leaves out the preset that the administrator's vtlook of its name
/// hides, and says why a vtlook that does not read cannot draw.
///
/// References:
/// - ADR-1304 D4, its note of 2026-11-19: No filter first, and a vtlook
///   that does not read may still be picked, with the reason shown.
#[test]
fn the_vtlook_list_puts_no_filter_first_and_says_why_a_vtlook_does_not_read() {
    let screen = reach(100, 30, &[TO_LOOKS, b"\r", DOWN, DOWN, DOWN, DOWN, b"\r"]);
    let rows: Vec<String> = body(&screen)
        .into_iter()
        .filter(|r| r.starts_with("   ● ") || r.starts_with("   ○ "))
        .collect();
    let names: Vec<String> = rows
        .iter()
        .map(|r| r.chars().skip(5).take_while(|c| *c != ' ').collect())
        .collect();
    assert_eq!(names, ["No", "amber", "green", "sepia"], "{screen}");
    assert!(rows[3].starts_with("   ● sepia"), "{screen}");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("vtium cannot read this file, so the terminal draws No filter until the file is fixed: Line 2: there is no color filter named glow."),
        "{screen}"
    );
}

/// Picking a color scheme in the list writes the look's own `palette`,
/// and Esc closes the card with the change kept for Save.
///
/// References:
/// - ADR-1403 D10: Esc keeps a card's changes, which wait for Save.
#[test]
fn picking_a_color_scheme_writes_the_look_s_palette_and_esc_keeps_it() {
    let mut d = desk();
    let mut c = console().keys(&[
        TO_LOOKS, b"\r", DOWN, DOWN, DOWN, b"\r", DOWN, b"\r", ESC, b"s",
    ]);
    run_desk(&mut c, &mut d);
    let first = vtium_term::PALETTE_NAMES[0];
    assert!(
        d.draft
            .contains(&format!("[look proof]\npalette = {first}\n")),
        "{}",
        d.draft
    );
}

/// Right steps the font set line to its next choice, which the look's
/// `font-name` takes, and Esc keeps it.
#[test]
fn right_steps_the_font_set_line_and_esc_keeps_the_change() {
    let mut d = desk();
    let mut c = console().keys(&[TO_LOOKS, b"\r", DOWN, RIGHT, ESC, b"s"]);
    run_desk(&mut c, &mut d);
    // the choices: vtium.conf's font set, then each that reads, in order:
    // minspec-16, proof, tiny5, unifont, unscii-8
    assert!(
        d.draft.contains("[look proof]\nfont-name = tiny5\n"),
        "{}",
        d.draft
    );
}

/// `n` among the looks makes a new look with every key left to vtium.conf
/// and no vtlook, so it starts on No filter, and opens its card with its
/// name being typed; Enter renames it.
///
/// References:
/// - ADR-1304 D4, its note of 2026-11-19: a new look starts on No filter.
/// - ADR-1403 D10: `n` makes a look and lets you type its name.
#[test]
fn n_makes_a_look_on_no_filter_whose_name_is_typed() {
    let mut d = desk();
    let backspace: &[u8] = b"\x7f";
    let mut c = console().keys(&[
        TO_LOOKS,
        b"n",
        &backspace.repeat(8),
        b"wide",
        b"\r",
        ESC,
        b"s",
    ]);
    run_desk(&mut c, &mut d);
    assert!(d.draft.contains("[look wide]\n"), "{}", d.draft);
    let section: String = d
        .draft
        .split("[look wide]\n")
        .nth(1)
        .and_then(|rest| rest.split('[').next())
        .unwrap_or_default()
        .to_owned();
    assert!(!section.contains("vtlook"), "{}", d.draft);
    assert!(!d.draft.contains("[look new-look]"), "{}", d.draft);
}

/// `r` on a look asks first, and `y` deletes it; the terminal that drew it
/// goes back to the default look.
#[test]
fn r_deletes_a_look_after_the_question() {
    let mut d = desk();
    let mut c = console().keys(&[TO_LOOKS, b"r", b"y", b"s"]);
    run_desk(&mut c, &mut d);
    assert!(!d.draft.contains("[look proof]"), "{}", d.draft);
    assert!(!d.draft.contains("look = proof"), "{}", d.draft);
    // any other answer keeps it
    let mut d = desk();
    let mut c = console().keys(&[TO_LOOKS, b"r", b"x", b"s"]);
    run_desk(&mut c, &mut d);
    assert!(d.draft.contains("[look proof]"), "{}", d.draft);
}

/// Enter on a font set opens its detail: its fonts by role, what it
/// misses, and the looks that use it.
///
/// References:
/// - ADR-1205 D8: a resource's view shows what works.
#[test]
fn a_font_set_s_detail_shows_its_fonts_by_role_and_the_looks_that_use_it() {
    let screen = reach(100, 30, &[TO_LOOKS, DOWN, DOWN, DOWN, b"\r"]);
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    for line in [
        "Font set proof",
        "The console reads it without errors.",
        "Text: the font family JetBrains Mono, not on this machine",
        "What it still misses:",
        "Used by look proof.",
    ] {
        assert!(text.contains(line), "{line:?}:\n{screen}");
    }
}

/// `r` on a font set marks it for removal at the next install.
#[test]
fn r_on_a_font_set_marks_it_for_removal() {
    let screen = reach(100, 30, &[TO_LOOKS, DOWN, DOWN, b"r"]);
    assert!(
        column(&screen).contains("minspec-16: reads, to remove."),
        "{screen}"
    );
}

/// `t` on a look tries the look's font set on the console, and the trial
/// puts the console's font back.
#[test]
fn t_on_a_look_tries_its_font_set() {
    let mut c = console().keys(&[TO_LOOKS, &[b"t".as_slice(), b"x"].concat(), b"q"]);
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.switches(), ["proof", "unifont"], "{:?}", c.log);
}

/// `p` switches the preview between sample characters and a picture of
/// the console, as the sentence under the list says.
#[test]
fn p_switches_the_preview_between_sample_characters_and_a_picture() {
    use vtium_config::tui::input::{Input, Key};
    use vtium_config::tui::model::{App, Event};
    let (mut app, _) = App::new(inputs(), 80, 25);
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    assert!(!app.looks.picture());
    app.event(Event::Input(Input::Key(Key::Char('p'))));
    assert!(app.looks.picture());
}

/// The fixture's inputs with `conf` as its vtium.conf.
fn with_conf(conf: &str) -> Inputs {
    let mut i = inputs();
    i.conf = Some(conf.to_owned());
    i
}

/// The screen that `keys` reach at 100x30 in a vtium console with
/// `allow-font-ops` on, from `inputs`.
fn reach_inputs(inputs: Inputs, keys: &[&[u8]]) -> String {
    let grid = (100, 30);
    let mut c = Console::vtium(100, 30, true, "unifont")
        .takes("minspec-16", grid)
        .takes("proof", grid)
        .takes("tiny5", grid)
        .takes("unifont", grid)
        .takes("unscii-8", grid)
        .keys(keys);
    drive(&mut c, inputs).expect("the loop");
    c.seen.clone().expect("the screen")
}

/// The draft that `keys` leave, from a desk whose draft and installed
/// file are `conf`.
fn draft_after(conf: &str, keys: &[&[u8]]) -> String {
    let mut d = desk();
    d.installed = conf.to_owned();
    d.draft = conf.to_owned();
    let mut c = console().keys(keys);
    vtium_config::tui::port::drive_with(&mut c, with_conf(conf), Some(&mut d)).expect("the loop");
    d.draft
}

/// The text of `screen` with its runs of blanks made one space.
fn flat(screen: &str) -> String {
    screen.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The rows of a list of choices on `screen`, ticked or not.
fn choice_rows(screen: &str) -> Vec<String> {
    body(screen)
        .into_iter()
        .filter(|r| r.starts_with("   ● ") || r.starts_with("   ○ "))
        .collect()
}

/// The section `[look NAME]` of `draft`, up to the next section.
fn look_section(draft: &str, name: &str) -> String {
    draft
        .split(&format!("[look {name}]\n"))
        .nth(1)
        .and_then(|rest| rest.split('[').next())
        .unwrap_or_default()
        .to_owned()
}

/// A vtium.conf whose top level sets the vtlook amber, with the look
/// proof that names no vtlook.
const TOP_AMBER: &str = "# mine\nfont-name=unifont\nvtlook=amber\nversion = 1\n[look proof]\nfont-name = proof\n[head desk]\nlook = proof\n[head panel]\n[rules]\ndisplay * -> desk\n";

/// A look with no vtlook line draws the vtlook that vtium.conf's top
/// level sets, and its card says so.
///
/// References:
/// - ADR-1304 D5: the look's vtlook wins, then the top level's, then the
///   built-in one.
#[test]
fn a_look_with_no_vtlook_line_shows_the_vtlook_vtium_conf_sets() {
    let screen = reach_inputs(with_conf(TOP_AMBER), &[TO_LOOKS, b"\r"]);
    assert!(
        flat(&screen).contains("vtlook amber, from vtium.conf"),
        "{screen}"
    );
}

/// Where vtium.conf sets a vtlook, the vtlook list starts with it, which
/// leaves the look's key unset, and No filter follows and writes
/// `vtlook = default`, since an unset key would draw amber.
///
/// References:
/// - ADR-1304 D5: `vtlook = default` in a look gives the built-in vtlook
///   whatever the top level says.
#[test]
fn no_filter_writes_vtlook_default_where_vtium_conf_sets_another_vtlook() {
    let to_list: &[&[u8]] = &[TO_LOOKS, b"\r", DOWN, DOWN, DOWN, DOWN, b"\r"];
    let screen = reach_inputs(with_conf(TOP_AMBER), to_list);
    let rows = choice_rows(&screen);
    assert!(
        rows[0].starts_with("   ● The vtlook vtium.conf sets (amber)"),
        "{screen}"
    );
    assert!(rows[1].starts_with("   ○ No filter"), "{screen}");
    let draft = draft_after(TOP_AMBER, &[to_list, &[DOWN, b"\r", ESC, b"s"]].concat());
    let proof = look_section(&draft, "proof");
    assert!(proof.contains("vtlook = default\n"), "{draft}");
    assert!(proof.contains("font-name = proof\n"), "{draft}");
    // a look that names default shows No filter, ticked in the list
    let named = TOP_AMBER.replace(
        "font-name = proof\n",
        "font-name = proof\nvtlook = default\n",
    );
    let screen = reach_inputs(with_conf(&named), to_list);
    let rows = choice_rows(&screen);
    assert!(rows[1].starts_with("   ● No filter"), "{screen}");
}

/// `n` writes `vtlook = default` into the new look where vtium.conf sets
/// another vtlook, so the new look starts on No filter.
///
/// References:
/// - ADR-1304 D4, its note of 2026-11-19: a new look starts on No filter.
#[test]
fn n_starts_a_new_look_on_no_filter_where_vtium_conf_sets_a_vtlook() {
    let draft = draft_after(TOP_AMBER, &[TO_LOOKS, b"n", b"\r", ESC, b"s"]);
    assert_eq!(
        look_section(&draft, "new-look").trim(),
        "vtlook = default",
        "{draft}"
    );
}

/// A look whose font set, size or color scheme is not among the line's
/// usual choices lists that value, ticked, and Left and Right step from
/// it: they never drop the look's key.
#[test]
fn a_value_the_usual_choices_lack_is_listed_and_steps_from_where_it_is() {
    let conf = "# mine\nfont-name=unifont\nversion = 1\n[look proof]\nfont-name = broken\nfont-size = 15\npalette = catppuccin-mocha\n";
    // the color schemes end with custom and the active scheme files, then
    // the look's own scheme, which no file of the fixture holds
    let draft = draft_after(conf, &[TO_LOOKS, b"\r", DOWN, DOWN, DOWN, LEFT, ESC, b"s"]);
    assert!(
        look_section(&draft, "proof").contains("palette = dusk\n"),
        "{draft}"
    );
    // 15 pt sits between 14 pt and 16 pt
    let draft = draft_after(conf, &[TO_LOOKS, b"\r", DOWN, DOWN, RIGHT, ESC, b"s"]);
    assert!(
        look_section(&draft, "proof").contains("font-size = 16\n"),
        "{draft}"
    );
    let screen = reach_inputs(with_conf(conf), &[TO_LOOKS, b"\r", DOWN, DOWN, b"\r"]);
    let rows = choice_rows(&screen);
    let ticked: Vec<&String> = rows.iter().filter(|r| r.starts_with("   ● ")).collect();
    assert_eq!(ticked.len(), 1, "{screen}");
    assert!(ticked[0].starts_with("   ● 15 pt"), "{screen}");
    // the font set that does not read is listed, ticked, with why
    let screen = reach_inputs(with_conf(conf), &[TO_LOOKS, b"\r", DOWN, b"\r"]);
    let rows = choice_rows(&screen);
    let ticked: Vec<&String> = rows.iter().filter(|r| r.starts_with("   ● ")).collect();
    assert_eq!(ticked.len(), 1, "{screen}");
    assert!(ticked[0].starts_with("   ● broken"), "{screen}");
    assert!(
        flat(&screen).contains("the console does not read it: line 2: unknown key colour"),
        "{screen}"
    );
}

/// Left steps the font set line to its choice before.
#[test]
fn left_steps_the_font_set_line_back() {
    let mut d = desk();
    let mut c = console().keys(&[TO_LOOKS, b"\r", DOWN, LEFT, ESC, b"s"]);
    run_desk(&mut c, &mut d);
    // proof's choice before is minspec-16
    assert!(
        d.draft.contains("[look proof]\nfont-name = minspec-16\n"),
        "{}",
        d.draft
    );
}

/// The look default is drawn by every terminal that names no look, so its
/// row counts them, and `r` names them and says what they draw after.
#[test]
fn the_look_default_counts_the_terminals_that_name_no_look() {
    let conf = "# mine\nfont-name=unifont\nversion = 1\n[look default]\nfont-name = proof\n[head desk]\n[head panel]\n[rules]\ndisplay * -> desk\n";
    let screen = reach_inputs(with_conf(conf), &[TO_LOOKS]);
    assert!(
        column(&screen).contains("Terminals desk and panel use it."),
        "{screen}"
    );
    let screen = reach_inputs(with_conf(conf), &[TO_LOOKS, b"r"]);
    assert!(
        flat(&screen).contains("Delete the look default? Terminals desk and panel use it, and then use vtium.conf's top-level settings."),
        "{screen}"
    );
}

/// A look's card names its fonts by role in the words a person uses, as
/// the font set file reads: the text font with its bold face, box drawing
/// as the file's glyph-source lines choose, the other fonts and the
/// layers; and what it misses, by role.
///
/// References:
/// - ADR-1205 D6: the roles.
/// - ADR-1205 D8: what a look still misses, by role.
#[test]
fn a_look_s_card_names_its_fonts_by_role() {
    let mut i = inputs();
    i.installed.insert(
        "proof".to_owned(),
        b"source=font:JetBrains Mono\nsource=font:JetBrains Mono Bold\nstyle=bold\nsource=kbd:lat2-16\nglyph-source=sprites\nglyph-source-box=font\nlayer=noto-emoji\n".to_vec(),
    );
    let screen = reach_inputs(i, &[TO_LOOKS, b"\r"]);
    let text = flat(&screen);
    for line in [
        "Text: the font family JetBrains Mono, not on this machine",
        "Text, bold: the font family JetBrains Mono Bold",
        "Box drawing: the text font's own lines",
        "More fonts, in the order vtium tries them: the console font lat2-16",
        "Scripts, symbols, emoji, and fallback: the fonts of the layer noto-emoji",
        "Text: the font family JetBrains Mono is not on this machine: no such family.",
    ] {
        assert!(text.contains(line), "{line:?}:\n{screen}");
    }
}

/// Where neither the look nor vtium.conf names a font set, the font set
/// line says what vtium uses, and the roles name it too.
#[test]
fn a_look_with_no_font_set_anywhere_says_what_vtium_uses() {
    let conf = "# mine\nversion = 1\n[look bare]\n";
    let screen = reach_inputs(with_conf(conf), &[TO_LOOKS, b"\r"]);
    let text = flat(&screen);
    assert!(
        text.contains("Font set none, so vtium uses the font that vconsole.conf names, or else its built-in font"),
        "{screen}"
    );
    assert!(
        text.contains("Text: the font that vconsole.conf names, or else vtium's built-in font"),
        "{screen}"
    );
    assert!(!text.contains("vtium's own choice"), "{screen}");
    let screen = reach_inputs(with_conf(conf), &[TO_LOOKS, b"\r", DOWN, b"\r"]);
    assert!(
        choice_rows(&screen)[0].starts_with("   ● The font set vtium.conf sets (none)"),
        "{screen}"
    );
}

/// `n` names the new look new-look, and the message line says so.
#[test]
fn n_names_the_new_look_new_look() {
    let mut d = desk();
    let mut c = console().keys(&[TO_LOOKS, b"n"]);
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        flat(&screen).contains("Made the look new-look. Type its name, then press Enter."),
        "{screen}"
    );
}

/// `t` says why it tries nothing: on the row that says there are no
/// looks yet, and on a look whose font set does not read.
#[test]
fn t_says_why_it_has_nothing_to_try() {
    let conf = "# mine\nfont-name=unifont\nversion = 1\n";
    let screen = reach_inputs(with_conf(conf), &[TO_LOOKS, b"t"]);
    assert!(
        flat(&screen)
            .contains("There is no look yet, so t has nothing to try. Press n to make a look."),
        "{screen}"
    );
    let conf = "# mine\nfont-name=unifont\nversion = 1\n[look odd]\nfont-name = broken\n";
    let screen = reach_inputs(with_conf(conf), &[TO_LOOKS, b"t"]);
    assert!(
        flat(&screen).contains("The console does not read the font set broken, so t cannot try it: line 2: unknown key colour"),
        "{screen}"
    );
}

/// `n` in the Fonts tab opens the form for a new font set, which belongs
/// to the Looks tab, and the message line says where Esc goes.
#[test]
fn n_in_the_fonts_tab_says_the_form_belongs_to_the_looks_tab() {
    let screen = reach(100, 30, &[TO_FONTS, b"n"]);
    assert!(
        flat(&screen).contains("The form for a new font set is part of the Looks tab, so Esc or adding the font set goes to the Looks tab."),
        "{screen}"
    );
}

/// The app on the Looks tab at `cols` by `rows`, from `inputs`.
fn looks_app(inputs: Inputs, cols: usize, rows: usize) -> vtium_config::tui::model::App {
    use vtium_config::tui::input::Key;
    let mut i = inputs;
    i.view = model::View::Flow;
    let mut a = vtium_config::tui::model::App::new(i, cols, rows).0;
    press(&mut a, Key::Char('3'));
    assert_eq!(a.nav.view, model::View::Looks);
    a
}

/// Moves the highlight of the Looks tab's list to `entry`.
fn to_entry(a: &mut vtium_config::tui::model::App, entry: &vtium_config::tui::tabs::looks::Entry) {
    use vtium_config::tui::input::Key;
    press(a, Key::Home);
    for _ in 0..40 {
        if a.looks.highlighted(&a.shared).as_ref() == Some(entry) {
            return;
        }
        press(a, Key::Down);
    }
    panic!("never reached {entry:?}:\n{}", text(a));
}

/// The fixture's color scheme file `name` in `dir`, as the list names it.
fn scheme_file(
    a: &vtium_config::tui::model::App,
    dir: &str,
    name: &str,
) -> vtium_config::tui::tabs::looks::Entry {
    use vtium_config::tui::tabs::looks::{Entry, palettes::Scheme};
    let at = a
        .shared
        .inputs
        .effects
        .palettes
        .iter()
        .position(|f| f.name == name && f.resource.path.starts_with(dir))
        .unwrap_or_else(|| panic!("no scheme file {dir}/{name}"));
    Entry::Palette(Scheme::File(at))
}

/// The rows of the Looks tab's body below the tab bar, as text.
fn rows_text(screen: &str) -> Vec<String> {
    body(screen)
}

/// The Looks tab lists the color schemes after the font sets, the nine
/// built-in schemes first and always active, then the package's files
/// and the administrator's, each with its marks; then the pointer themes.
///
/// References:
/// - ADR-1205 D10: the built-in schemes, the two folders, and the
///   administrator's file hiding the package's.
/// - ADR-1205 D6: the installed pointer themes.
/// - ADR-1403 D10: the tab's one list in four groups.
#[test]
fn the_color_schemes_and_the_pointer_themes_follow_the_font_sets() {
    use vtium_config::tui::input::Key;
    let mut a = looks_app(inputs(), 100, 30);
    // the list scrolls, so the rows come from each group's first item
    let mut names = Vec::new();
    let n = vtium_config::tui::tabs::looks::Looks::entries(&a.shared).len();
    press(&mut a, Key::Home);
    for _ in 0..n {
        let screen = text(&a);
        let about = flat(&screen);
        names.push(about);
        press(&mut a, Key::Down);
    }
    let all = names.join("\n");
    for (heading, after) in [
        (" Font sets", " Color schemes"),
        (" Color schemes", " Pointer themes"),
    ] {
        let screen = text(&a);
        let rows = rows_text(&screen);
        let at = |h: &str| rows.iter().position(|r| r == h);
        if let (Some(x), Some(y)) = (at(heading), at(after)) {
            assert!(x < y, "{screen}");
        }
    }
    for want in [
        "legacy: built in, always active.",
        "vga: built in, always active.",
        "dusk: active, trusted, hidden by the administrator's color scheme of this name.",
        "paper: active, trusted.",
        "dusk: active, trusted.",
        "loose: not active, fails the trust rule.",
        "broken: not active, trusted, has an error.",
        "pointers.svg: active, trusted.",
        "Adwaita: not active, trusted.",
        "stray: not active, fails the trust rule.",
    ] {
        assert!(all.contains(want), "{want}:\n{all}");
    }
    // the last screen shows both groups' headings, the schemes first
    let screen = text(&a);
    let rows = rows_text(&screen);
    let schemes = rows.iter().position(|r| r == " Color schemes");
    let pointers = rows.iter().position(|r| r == " Pointer themes");
    assert!(pointers.is_some(), "{screen}");
    if let Some(schemes) = schemes {
        assert!(Some(schemes) < pointers, "{screen}");
    }
    let order: Vec<String> = vtium_config::tui::tabs::looks::Looks::entries(&a.shared)
        .iter()
        .filter_map(|e| match e {
            vtium_config::tui::tabs::looks::Entry::Palette(s) => Some(format!("{s:?}")),
            vtium_config::tui::tabs::looks::Entry::Pointer(at) => Some(format!("Pointer({at})")),
            _ => None,
        })
        .collect();
    assert_eq!(
        order,
        [
            "BuiltIn(\"legacy\")",
            "BuiltIn(\"vga\")",
            "BuiltIn(\"nord\")",
            "BuiltIn(\"solarized\")",
            "BuiltIn(\"solarized-black\")",
            "BuiltIn(\"solarized-white\")",
            "BuiltIn(\"soft-black\")",
            "BuiltIn(\"base16-dark\")",
            "BuiltIn(\"base16-light\")",
            "File(0)",
            "File(1)",
            "File(2)",
            "File(3)",
            "File(4)",
            "Pointer(0)",
            "Pointer(1)",
            "Pointer(2)",
        ]
    );
}

/// A highlighted color scheme shows its 16 colors under the list, in its
/// own 24-bit colors where the terminal shows them, and a line of text in
/// its foreground on its background. The preview picture waits, and a
/// terminal without 24-bit color gets a sentence that says so.
///
/// References:
/// - ADR-1205 D10: a color scheme's preview is its 16 colors beside text
///   in its foreground on its background.
#[test]
fn a_color_scheme_s_preview_shows_its_colors_beside_text_in_its_own_colors() {
    use vtium_config::tui::tabs::Tab;
    let mut i = inputs();
    i.flow_colors.truecolor = true;
    let mut a = looks_app(i, 100, 30);
    let paper = scheme_file(&a, "/usr/share/vtium/palettes", "paper");
    to_entry(&mut a, &paper);
    let palette = a.shared.inputs.effects.palettes[1]
        .colors
        .clone()
        .expect("paper reads");
    let size = model::Size {
        cols: 100,
        rows: 30,
        cell_px: None,
    };
    let body = a.looks.body(&a.shared, size);
    let row = |label: &str| {
        body.tail
            .iter()
            .find(|r| r.first().is_some_and(|s| s.text.trim() == label))
            .unwrap_or_else(|| panic!("no row {label}: {:?}", body.tail))
    };
    let fills = |label: &str| -> Vec<[u8; 3]> {
        row(label)
            .iter()
            .filter_map(|s| s.style.back.and_then(|b| b.rgb))
            .collect()
    };
    let rgb = |c: vtium_term::Rgb| [c.r, c.g, c.b];
    assert_eq!(
        fills("0-7"),
        (0..8).map(|n| rgb(palette.color(n))).collect::<Vec<_>>()
    );
    assert_eq!(
        fills("8-15"),
        (8..16).map(|n| rgb(palette.color(n))).collect::<Vec<_>>()
    );
    // the sample text sits on the background, in the foreground and in the
    // colors its words name
    let text_row = row("Text");
    assert!(
        text_row
            .iter()
            .skip(1)
            .all(|s| s.style.back.and_then(|b| b.rgb) == Some(rgb(palette.bg()))),
        "{text_row:?}"
    );
    let ink: Vec<Option<[u8; 3]>> = text_row.iter().skip(1).map(|s| s.style.rgb).collect();
    assert!(ink.contains(&Some(rgb(palette.fg()))), "{ink:?}");
    assert!(ink.contains(&Some(rgb(palette.color(1)))), "{ink:?}");
    // no picture is asked for while a color scheme is highlighted
    assert!(a.looks.preview(&a.shared, size).font().is_none());
    // without 24-bit color, the sentence says the terminal's own colors
    // may stand in
    let mut a = looks_app(inputs(), 100, 30);
    to_entry(&mut a, &paper);
    let screen = flat(&text(&a));
    assert!(
        screen.contains("This terminal does not say it shows 24-bit color"),
        "{screen}"
    );
}

/// Enter on a color scheme opens its detail: its file, or built in, its
/// marks and the check it fails, the looks that use it, and its colors by
/// value. Esc goes back to the list.
///
/// References:
/// - ADR-1403 D10: the color scheme's read-only detail.
#[test]
fn enter_on_a_color_scheme_opens_its_detail() {
    use vtium_config::tui::input::Key;
    use vtium_config::tui::tabs::looks::{Entry, palettes::Scheme};
    let conf = "# mine\nfont-name=unifont\nversion = 1\n[look proof]\nfont-name = proof\npalette = nord\n[look plain]\n";
    let mut a = looks_app(with_conf(conf), 100, 30);
    to_entry(&mut a, &Entry::Palette(Scheme::BuiltIn("nord")));
    press(&mut a, Key::Enter);
    assert_eq!(vtium_config::tui::nav::node(&a), "looks.palette");
    let screen = flat(&text(&a));
    for want in [
        "Color scheme nord",
        "File: none, because it is built into vtium",
        "State: always active",
        "Used by look proof.",
        "The scheme's 16 colors, then its foreground and background:",
        "foreground #",
    ] {
        assert!(screen.contains(want), "{want}: {screen}");
    }
    press(&mut a, Key::Escape);
    assert_eq!(vtium_config::tui::nav::node(&a), "looks");
    // a look with no palette draws legacy, the default
    to_entry(&mut a, &Entry::Palette(Scheme::BuiltIn("legacy")));
    press(&mut a, Key::Enter);
    assert!(
        flat(&text(&a)).contains("Used by look plain."),
        "{}",
        text(&a)
    );
    press(&mut a, Key::Escape);
    // a file shows its path, its marks and the check it fails
    let loose = scheme_file(&a, "/etc/vtium/palettes", "loose");
    to_entry(&mut a, &loose);
    press(&mut a, Key::Enter);
    let screen = flat(&text(&a));
    for want in [
        "Color scheme loose",
        "File: /etc/vtium/palettes/loose.palette",
        "State: not active",
        "This color scheme fails the trust rule",
        "the file belongs to user 1000",
        "No look uses it.",
    ] {
        assert!(screen.contains(want), "{want}: {screen}");
    }
    press(&mut a, Key::Escape);
    // a file that does not read says why, and shows no colors
    let broken = scheme_file(&a, "/etc/vtium/palettes", "broken");
    to_entry(&mut a, &broken);
    press(&mut a, Key::Enter);
    let screen = flat(&text(&a));
    assert!(
        screen.contains("vtium cannot use this file, so a look that names this color scheme renders with the one vtium.conf names: /etc/vtium/palettes/broken.palette:3:"),
        "{screen}"
    );
    assert!(!screen.contains("foreground #"), "{screen}");
    press(&mut a, Key::Escape);
    // the packaged file that the administrator's hides says so
    let dusk = scheme_file(&a, "/usr/share/vtium/palettes", "dusk");
    to_entry(&mut a, &dusk);
    press(&mut a, Key::Enter);
    let screen = flat(&text(&a));
    assert!(
        screen.contains("The administrator's folder holds a color scheme named dusk too, so vtium reads that file in place of this one."),
        "{screen}"
    );
}

/// A folder of scheme files on the disk, with the package's dusk and the
/// administrator's mine, active, and the administrator's spare, not
/// active, as the Looks tab finds them; and the inputs that list them.
fn schemes_on_disk(conf: &str) -> (PathBuf, Inputs) {
    use vtium_config::batch::State;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!(
            "tui-looks-schemes-{}-{}",
            std::process::id(),
            conf.len()
        ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a folder");
    // vtium's gate takes only a path without `..`
    let root = root.canonicalize().expect("the folder's path");
    let (etc, pkg) = (root.join("etc"), root.join("pkg"));
    for d in [&etc, &pkg] {
        std::fs::create_dir_all(d).expect("a folder");
    }
    let text = "background = #101010\nforeground = #e0e0e0\n";
    for f in [
        pkg.join("dusk.palette"),
        etc.join("mine.palette"),
        etc.join("spare.palette"),
    ] {
        std::fs::write(f, text).expect("a scheme file");
    }
    let folders = vec![etc.clone(), pkg];
    let spare = etc.join("spare.palette");
    let found = vtium_config::tui::effects_found::find_palettes(
        &folders,
        &|p| {
            if p == spare {
                State::Inactive
            } else {
                State::Active
            }
        },
        &|_| Ok(()),
        rustix::process::geteuid().as_raw(),
    );
    let mut i = with_conf(conf);
    i.effects.palettes = found;
    i.effects.palette_folders = folders;
    (root, i)
}

/// Space activates a scheme file that is not active and deactivates one
/// that is, through the desk, and asks first when a look uses the file,
/// naming the look. Space does nothing on a built-in scheme, and says why
/// it cannot activate a file that fails the trust rule.
///
/// References:
/// - ADR-1205 D4: Space activates or deactivates a resource in the draft,
///   through the desk.
/// - ADR-1205 D8: deactivating warns and names the looks that use it.
/// - ADR-1205 D10: the built-in schemes are always active.
#[test]
fn space_activates_or_deactivates_a_scheme_file_and_asks_when_a_look_uses_it() {
    use vtium_config::batch::Change;
    use vtium_config::tui::desk::Op;
    use vtium_config::tui::input::Key;
    use vtium_config::tui::model::Effect;
    use vtium_config::tui::tabs::looks::{Entry, palettes::Scheme};
    let conf = "# mine\nfont-name=unifont\nversion = 1\n[look proof]\nfont-name = proof\npalette = mine\n[head desk]\nlook = proof\n";
    let (root, i) = schemes_on_disk(conf);
    let etc = root.join("etc");
    let mut a = looks_app(i, 100, 30);
    let mine = scheme_file(&a, &etc.display().to_string(), "mine");
    to_entry(&mut a, &mine);
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    assert_eq!(vtium_config::tui::nav::node(&a), "any.question.deactivate");
    let question = a
        .nav
        .confirm
        .as_ref()
        .map(|c| c.question.clone())
        .unwrap_or_default();
    assert!(
        question.contains("Look proof uses it. From the next login after the install, look proof renders without it."),
        "{question}"
    );
    let out = press(&mut a, Key::Char('y'));
    assert_eq!(
        out,
        [Effect::Desk(Op::Resource {
            path: etc.join("mine.palette"),
            change: Change::Deactivate,
        })]
    );
    // a file no look uses deactivates at once, and one that is not active
    // activates
    let dusk = scheme_file(&a, &root.join("pkg").display().to_string(), "dusk");
    to_entry(&mut a, &dusk);
    assert_eq!(
        press(&mut a, Key::Char(' ')),
        [Effect::Desk(Op::Resource {
            path: root.join("pkg/dusk.palette"),
            change: Change::Deactivate,
        })]
    );
    let spare = scheme_file(&a, &etc.display().to_string(), "spare");
    to_entry(&mut a, &spare);
    assert_eq!(
        press(&mut a, Key::Char(' ')),
        [Effect::Desk(Op::Resource {
            path: etc.join("spare.palette"),
            change: Change::Activate,
        })]
    );
    let _ = std::fs::remove_dir_all(&root);
    // a built-in scheme takes no Space, and the fixture's loose says why
    // it cannot be activated
    let mut a = looks_app(inputs(), 100, 30);
    to_entry(&mut a, &Entry::Palette(Scheme::BuiltIn("solarized")));
    let before = a.shared.message.clone();
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    assert_eq!(a.shared.message, before);
    assert_eq!(vtium_config::tui::nav::node(&a), "looks");
    let loose = scheme_file(&a, "/etc/vtium/palettes", "loose");
    to_entry(&mut a, &loose);
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    assert!(
        flat(&text(&a))
            .contains("vtium cannot activate this color scheme, because it fails the trust rule"),
        "{}",
        text(&a)
    );
}

/// Space on a pointer theme activates its folder through the desk, and
/// the desk's answer marks the theme. Enter opens its detail, which names
/// its folder and says that every look draws the theme vtium.conf's
/// pointer-theme names.
///
/// References:
/// - ADR-1205 D4, its note of 2026-11-19: a pointer theme is activated by
///   its folder.
/// - ADR-1205 D6: the Looks tab lists the installed pointer themes.
#[test]
fn space_activates_a_pointer_theme_by_its_folder_and_its_detail_says_who_draws_it() {
    use vtium_config::batch::{Change, State};
    use vtium_config::tui::desk::Op;
    use vtium_config::tui::input::Key;
    use vtium_config::tui::model::{Effect, Event};
    use vtium_config::tui::tabs::looks::Entry;
    let conf = "# mine\nfont-name=unifont\npointer-theme = /usr/share/icons/Adwaita/\nversion = 1\n[look proof]\n";
    let mut a = looks_app(with_conf(conf), 100, 30);
    to_entry(&mut a, &Entry::Pointer(1));
    let adwaita = PathBuf::from("/usr/share/icons/Adwaita");
    let op = Op::Resource {
        path: adwaita.clone(),
        change: Change::Activate,
    };
    assert_eq!(press(&mut a, Key::Char(' ')), [Effect::Desk(op.clone())]);
    let mut states = active();
    let done = vtium_config::tui::fixture::resource_op(&mut states, &adwaita, Change::Activate)
        .expect("an activation");
    a.event(Event::Desked(op, Ok(done)));
    assert_eq!(a.shared.inputs.effects.pointers[1].state, State::Activates);
    let screen = flat(&text(&a));
    assert!(
        screen.contains("The draft activates this pointer theme."),
        "{screen}"
    );
    assert!(
        screen.contains("Adwaita: activates on install, trusted."),
        "{screen}"
    );
    press(&mut a, Key::Enter);
    assert_eq!(vtium_config::tui::nav::node(&a), "looks.pointer");
    let screen = flat(&text(&a));
    for want in [
        "Pointer theme Adwaita",
        "Folder: /usr/share/icons/Adwaita. Every file in the folder is active while the folder is.",
        "State: activates on install",
        "This pointer theme passes the trust rule.",
        "Every look uses it, because vtium.conf's pointer-theme names it.",
    ] {
        assert!(screen.contains(want), "{want}: {screen}");
    }
    press(&mut a, Key::Escape);
    // vtium's own pictures are a file, which no look uses here
    to_entry(&mut a, &Entry::Pointer(0));
    press(&mut a, Key::Enter);
    let screen = flat(&text(&a));
    assert!(
        screen.contains("File: /usr/share/vtium/pointers.svg"),
        "{screen}"
    );
    assert!(screen.contains("No look uses it."), "{screen}");
}

/// Space on the pointer theme that vtium.conf's pointer-theme names asks
/// first, because every head draws that theme, and says what every head
/// shows once it is deactivated.
///
/// References:
/// - ADR-1205 D8: deactivating a resource warns and names what uses it.
#[test]
fn space_on_the_pointer_theme_vtium_conf_names_asks_first() {
    use vtium_config::batch::Change;
    use vtium_config::tui::desk::Op;
    use vtium_config::tui::input::Key;
    use vtium_config::tui::model::Effect;
    use vtium_config::tui::tabs::looks::Entry;
    let conf = "# mine\nfont-name=unifont\npointer-theme = /usr/share/vtium/pointers.svg\nversion = 1\n[look proof]\n";
    let mut a = looks_app(with_conf(conf), 100, 30);
    to_entry(&mut a, &Entry::Pointer(0));
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    assert_eq!(vtium_config::tui::nav::node(&a), "any.question.deactivate");
    let question = a
        .nav
        .confirm
        .as_ref()
        .map(|c| c.question.clone())
        .unwrap_or_default();
    assert!(
        question.contains("vtium.conf's pointer-theme names it. From the next login after the install, every head shows vtium's own pointer pictures in its place."),
        "{question}"
    );
    assert_eq!(
        press(&mut a, Key::Char('y')),
        [Effect::Desk(Op::Resource {
            path: PathBuf::from("/usr/share/vtium/pointers.svg"),
            change: Change::Deactivate,
        })]
    );
}

/// `n`, `r`, `t` and `p` do nothing among the color schemes and the
/// pointer themes: nothing changes, nothing opens and nothing is said.
///
/// References:
/// - ADR-1403 D10, its notes of 2026-11-19: those keys do nothing there,
///   and `p` does nothing on a color scheme.
#[test]
fn n_r_t_and_p_do_nothing_among_the_color_schemes_and_the_pointer_themes() {
    use vtium_config::tui::input::Key;
    use vtium_config::tui::tabs::looks::{Entry, palettes::Scheme};
    let mut a = looks_app(inputs(), 100, 30);
    for entry in [
        Entry::Palette(Scheme::BuiltIn("nord")),
        scheme_file(&a, "/etc/vtium/palettes", "dusk"),
        Entry::Pointer(0),
    ] {
        to_entry(&mut a, &entry);
        for key in ['n', 'r', 't', 'p'] {
            let picture = a.looks.picture();
            let message = a.shared.message.clone();
            let draft = a.shared.conf_text().map(str::to_owned);
            assert!(
                press(&mut a, Key::Char(key)).is_empty(),
                "{key} on {entry:?}"
            );
            assert_eq!(
                vtium_config::tui::nav::node(&a),
                "looks",
                "{key} on {entry:?}"
            );
            assert_eq!(a.looks.picture(), picture, "{key} on {entry:?}");
            assert_eq!(a.shared.message, message, "{key} on {entry:?}");
            assert_eq!(
                a.shared.conf_text().map(str::to_owned),
                draft,
                "{key} on {entry:?}"
            );
            assert_eq!(a.looks.highlighted(&a.shared).as_ref(), Some(&entry));
        }
    }
}

/// The screens of the color schemes group, a color scheme's detail with
/// its preview, and the pointer themes group, at 80×25 and 100×30, in a
/// terminal that shows 24-bit color, for `make tui-shots`.
#[test]
fn the_color_schemes_and_the_pointer_themes_at_80x25_and_100x30() {
    // the list from the Looks tab's first row: Down to the scheme paper,
    // to the pointer theme Adwaita, and Enter on paper
    let paper: Vec<&[u8]> = [TO_LOOKS]
        .into_iter()
        .chain(std::iter::repeat_n(DOWN, 17))
        .collect();
    let mut detail = paper.clone();
    detail.push(b"\r");
    let pointers: Vec<&[u8]> = [TO_LOOKS]
        .into_iter()
        .chain(std::iter::repeat_n(DOWN, 22))
        .collect();
    for (cols, rows) in [(80, 25), (100, 30)] {
        for (name, keys) in [
            ("looks-palettes", &paper),
            ("looks-palette", &detail),
            ("looks-pointers", &pointers),
        ] {
            let mut i = inputs();
            i.flow_colors.truecolor = true;
            let grid = (cols, rows);
            let mut c = Console::vtium(cols, rows, true, "unifont")
                .takes("minspec-16", grid)
                .takes("proof", grid)
                .takes("tiny5", grid)
                .takes("unifont", grid)
                .takes("unscii-8", grid)
                .keys(keys);
            drive(&mut c, i).expect("the loop");
            let screen = c.seen.clone().expect("the screen");
            let bytes = c.written.get(..c.seen_at).unwrap_or_default().to_vec();
            let lines: Vec<&str> = screen.lines().collect();
            assert_eq!(lines.len(), rows, "{name} at {cols}x{rows}");
            assert!(lines.iter().all(|l| l.chars().count() <= cols), "{name}");
            let want = match name {
                "looks-palettes" => "paper: active, trusted.",
                "looks-palette" => "Color scheme paper",
                _ => "Adwaita: not active, trusted.",
            };
            assert!(
                flat(&screen).contains(want),
                "{name} at {cols}x{rows}: {screen}"
            );
            golden(&format!("{name}-{cols}x{rows}"), &screen, &bytes);
        }
    }
}
