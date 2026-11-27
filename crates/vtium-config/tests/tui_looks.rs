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
    // the Fonts tab opens on its filter line, which previews nothing
    assert!(c.specimens.is_empty(), "{:?}", c.specimens);
    assert_eq!(
        kitty::show(kitty::IMAGE_ID, 2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]),
        b"\x1b_Ga=T,f=32,t=d,i=1201,s=2,v=1,C=1,q=2,m=0;/wAA/wAA//8=\x1b\\"
    );
    assert_eq!(count(&c.written, &shown()), 2);
    // deleted when the next size comes, and when the Fonts tab shows its
    // filter line, which previews nothing, so none is left when the TUI
    // leaves
    assert_eq!(count(&c.written, &kitty::delete(kitty::IMAGE_ID)), 2);
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
    // one summary, the look's at 12 pt and 96 dpi; the Fonts tab opens on
    // its filter line, which previews nothing
    let heights: Vec<u32> = c.summaries.iter().map(|(_, h)| *h).collect();
    assert_eq!(heights, [16]);
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
        "Pointer theme vtium's own pictures, from vtium.conf",
        "vtium-config offers its usual sizes, because it found no font file for the text font JetBrains Mono.",
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
    // the question says what its keys do: any other key leaves it open,
    // and Esc goes back with the look kept
    use vtium_config::tui::input::Key;
    let mut a = looks_app(inputs(), 100, 30);
    press(&mut a, Key::Char('r'));
    let question = flat(&text(&a));
    assert!(
        question.contains("y deletes it, and Esc goes back."),
        "{question}"
    );
    press(&mut a, Key::Char('x'));
    assert_eq!(vtium_config::tui::nav::node(&a), "any.question.confirm");
    assert!(
        flat(&text(&a)).contains("Delete the look proof?"),
        "{}",
        text(&a)
    );
    press(&mut a, Key::Escape);
    assert_eq!(vtium_config::tui::nav::node(&a), "looks");
    assert!(
        a.shared
            .conf_text()
            .is_some_and(|t| t.contains("[look proof]")),
        "{:?}",
        a.shared.conf_text()
    );
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

/// `r` on a font set marks it for removal at the next install, which the
/// draft's folder keeps.
#[test]
fn r_on_a_font_set_marks_it_for_removal() {
    let mut c = console().keys(&[TO_LOOKS, DOWN, DOWN, b"r"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    assert!(d.frits.removed.contains("minspec-16"), "{:?}", d.frits);
    let screen = c.seen.clone().expect("the screen");
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
        text.contains("Font set none, so vtium uses the font that vconsole.conf names, else the font set terminus where it is installed, or else its built-in font"),
        "{screen}"
    );
    assert!(
        text.contains("Text: the font that vconsole.conf names, else the font set terminus where it is installed, or else vtium's built-in font"),
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

/// A look whose text font is a bitmap font offers the point sizes that
/// draw its strikes, from the font helper's description of the font's
/// file, and the size line says so; an outline text font offers the
/// usual sizes and says it draws at any size.
///
/// References:
/// - ADR-1205 D7: a look offers the sizes its fonts supply.
#[test]
fn a_look_s_size_line_offers_the_sizes_its_text_font_supplies() {
    let conf = "font-name=unifont\nversion = 1\n[look proof]\nfont-name = term\n[head desk]\nlook = proof\n";
    let mut i = with_conf(conf);
    i.installed.insert(
        "term".to_owned(),
        b"source=outline:/usr/share/fonts/misc/ter-u16n.otb\n".to_vec(),
    );
    let card = reach_inputs(i.clone(), &[TO_LOOKS, b"\r"]);
    assert!(
        flat(&card).contains(
            "The text font, Terminus, is a bitmap font, so the look offers the point sizes that draw its bitmap sizes: 8×16."
        ),
        "{card}"
    );
    assert!(!flat(&card).contains(words::SIZES_LOWEST_DENSITY), "{card}");
    // the size line's list: vtium.conf's size, automatic, then 12 points,
    // which draw the 8×16 strike at 96 DPI; the console draws only the
    // file the source names, so no other Terminus file adds a size
    let list = reach_inputs(i.clone(), &[TO_LOOKS, b"\r", DOWN, DOWN, b"\r"]);
    assert!(flat(&list).contains("12 pt, bitmap size 8×16"), "{list}");
    assert!(!flat(&list).contains("14 pt"), "{list}");
    assert!(!flat(&list).contains("9 pt"), "{list}");
    // with displays of 96 and 192 DPI, the sizes are for the lowest
    // density, and the size line says so
    i.densities = vec![192, 96];
    let card = reach_inputs(i.clone(), &[TO_LOOKS, b"\r"]);
    assert!(flat(&card).contains(words::SIZES_LOWEST_DENSITY), "{card}");
    let list = reach_inputs(i, &[TO_LOOKS, b"\r", DOWN, DOWN, b"\r"]);
    assert!(flat(&list).contains("12 pt, bitmap size 8×16"), "{list}");
    let mut i = with_conf(&conf.replace("term", "dejavu"));
    i.installed.insert(
        "dejavu".to_owned(),
        b"source=font:DejaVu Sans Mono\n".to_vec(),
    );
    let card = reach_inputs(i, &[TO_LOOKS, b"\r"]);
    assert!(
        flat(&card).contains(
            "The text font, DejaVu Sans Mono, has outlines, so the look can take any size."
        ),
        "{card}"
    );
}

/// A look with no font set, under a vtium.conf that names none, draws
/// vconsole's font, here a kbd font, else the default set `terminus`, and
/// its size line offers that font's sizes. It never says the text font is
/// built in.
///
/// References:
/// - ADR-1204 D2: the default set comes after vconsole's font.
/// - ADR-1205 D7: a look offers the sizes its fonts supply.
#[test]
fn a_look_with_no_font_set_offers_the_sizes_of_the_font_the_console_draws() {
    let conf = "version = 1\n[look proof]\n[head desk]\nlook = proof\n";
    let bitmap = "The text font, Terminus, is a bitmap font, so the look offers the point sizes that draw its bitmap sizes: 8×16.";
    let mut i = with_conf(conf);
    i.installed.insert(
        "terminus".to_owned(),
        b"source=outline:/usr/share/fonts/misc/ter-u16n.otb\n".to_vec(),
    );
    let card = reach_inputs(i.clone(), &[TO_LOOKS, b"\r"]);
    assert!(flat(&card).contains(bitmap), "{card}");
    assert!(!flat(&card).contains("built into vtium"), "{card}");
    // vconsole names a kbd font, which no font set is named after
    i.installed.clear();
    i.vconsole_font = Some("ter-u16n".to_owned());
    let card = reach_inputs(i.clone(), &[TO_LOOKS, b"\r"]);
    assert!(flat(&card).contains(bitmap), "{card}");
    // with neither, the card says no default set is installed
    i.vconsole_font = None;
    let card = reach_inputs(i, &[TO_LOOKS, b"\r"]);
    assert!(
        flat(&card).contains("because no font set named terminus is installed"),
        "{card}"
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
    // stand in
    let mut a = looks_app(inputs(), 100, 30);
    to_entry(&mut a, &paper);
    let screen = flat(&text(&a));
    assert!(
        screen.contains("This terminal does not report 24-bit color, so the preview shows the terminal's own eight colors in place of the scheme's. Enter opens the scheme's detail, which gives each color's value."),
        "{screen}"
    );
}

/// With color off in this terminal, as with NO_COLOR, a color scheme's
/// preview leaves out its blocks of color, which would show blank, and
/// says why; the detail still gives each color's value.
///
/// References:
/// - ADR-1205 D10: a color scheme's preview is its 16 colors beside text
///   in its foreground on its background.
#[test]
fn with_color_off_a_color_scheme_s_preview_says_so_and_shows_no_blocks() {
    use vtium_config::tui::input::Key;
    for truecolor in [false, true] {
        let mut i = inputs();
        i.color = false;
        i.flow_colors.truecolor = truecolor;
        let mut a = looks_app(i, 100, 30);
        let paper = scheme_file(&a, "/usr/share/vtium/palettes", "paper");
        to_entry(&mut a, &paper);
        let screen = text(&a);
        let rows = rows_text(&screen);
        for label in ["0-7", "8-15", "Text"] {
            assert!(
                !rows.iter().any(|r| r.trim_start().starts_with(label)),
                "{label}: {screen}"
            );
        }
        let said = flat(&screen);
        assert!(
            said.contains("Colors are off in this terminal, so the preview cannot show the scheme's colors. Enter opens the scheme's detail, which gives each color's value."),
            "{said}"
        );
        assert!(!said.contains("24-bit"), "{said}");
        press(&mut a, Key::Enter);
        let said = flat(&text(&a));
        assert!(said.contains("foreground #"), "{said}");
        assert!(
            !rows_text(&text(&a))
                .iter()
                .any(|r| r.trim_start().starts_with("0-7")),
            "{}",
            text(&a)
        );
    }
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
    assert_eq!(vtium_config::tui::nav::node(&a), "looks-resources");
    // a look with no palette draws legacy, the default, and so does the
    // top level, which the console draws since the text has no heads
    to_entry(&mut a, &Entry::Palette(Scheme::BuiltIn("legacy")));
    press(&mut a, Key::Enter);
    assert!(
        flat(&text(&a)).contains("Used by the top level of vtium.conf, look plain."),
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

/// A built-in scheme that vtium.conf's top level names is used by the top
/// level, which every head draws when the draft has no look of its own,
/// as a scheme file named there is.
///
/// References:
/// - ADR-1205 D10: a look that names no palette takes the top level's,
///   and with neither set a head draws `legacy`.
#[test]
fn a_built_in_scheme_the_top_level_names_is_used_by_the_top_level() {
    use vtium_config::tui::input::Key;
    use vtium_config::tui::tabs::looks::{Entry, palettes::Scheme};
    for conf in [
        "# mine\nfont-name=unifont\npalette = nord\n",
        "# mine\nfont-name=unifont\npalette = nord\nversion = 1\n[head desk]\nlook = big\n",
    ] {
        let mut a = looks_app(with_conf(conf), 100, 30);
        to_entry(&mut a, &Entry::Palette(Scheme::BuiltIn("nord")));
        press(&mut a, Key::Enter);
        let screen = flat(&text(&a));
        assert!(
            screen.contains("Used by the top level of vtium.conf."),
            "{conf}: {screen}"
        );
        press(&mut a, Key::Escape);
        to_entry(&mut a, &Entry::Palette(Scheme::BuiltIn("legacy")));
        press(&mut a, Key::Enter);
        let screen = flat(&text(&a));
        assert!(screen.contains("No look uses it."), "{conf}: {screen}");
    }
}

/// A detail whose paragraphs wrap to many rows scrolls down to its last
/// row, since the limit counts the rows on the screen, not paragraphs.
///
/// References:
/// - ADR-1403 D10: the color scheme's read-only detail.
#[test]
fn a_detail_that_wraps_scrolls_to_its_last_row() {
    use vtium_config::batch::State;
    use vtium_config::tui::effects_found::SchemeFile;
    use vtium_config::tui::input::Key;
    use vtium_config::tui::resources::Resource;
    // a file whose check and error each wrap to many rows, in a detail of
    // few paragraphs
    let mut i = inputs();
    i.effects.palettes.push(SchemeFile {
        name: "long".to_owned(),
        resource: Resource {
            path: PathBuf::from("/etc/vtium/palettes/long.palette"),
            kind: "color scheme",
            state: State::Inactive,
            trust: Err(format!("{}first-end.", "The check says so. ".repeat(60))),
            own: false,
            hidden: false,
        },
        colors: Err(format!("{}last-end.", "The error says so. ".repeat(60))),
    });
    let mut a = looks_app(i, 80, 25);
    let long = scheme_file(&a, "/etc/vtium/palettes", "long");
    to_entry(&mut a, &long);
    press(&mut a, Key::Enter);
    assert_eq!(vtium_config::tui::nav::node(&a), "looks.palette");
    for _ in 0..80 {
        press(&mut a, Key::Down);
    }
    let screen = text(&a);
    // the last row of the last paragraph is in sight, under the title
    assert!(flat(&screen).contains("last-end."), "{screen}");
    assert!(flat(&screen).contains("Color scheme long"), "{screen}");
}

/// A folder of scheme files on the disk, with the package's dusk and the
/// administrator's mine and Catppuccin Mocha, active, and the
/// administrator's spare, not active, as the Looks tab finds them; and
/// the inputs that list them.
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
        etc.join("Catppuccin Mocha.palette"),
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
    use vtium_config::batch::{Change, State};
    use vtium_config::tui::desk::Op;
    use vtium_config::tui::input::Key;
    use vtium_config::tui::model::{Effect, Event};
    use vtium_config::tui::tabs::looks::{Entry, card, list, palettes::Scheme};
    let conf = "# mine\nfont-name=unifont\nversion = 1\n[look proof]\nfont-name = proof\npalette = mine\n[look other]\n[head desk]\nlook = proof\n";
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
    let op = Op::Resource {
        path: etc.join("mine.palette"),
        change: Change::Deactivate,
    };
    assert_eq!(press(&mut a, Key::Char('y')), [Effect::Desk(op.clone())]);
    // the color schemes the card of a look that names none offers, by the
    // name a look gives: the active files, but never one whose name a look
    // cannot name
    let offered = |a: &vtium_config::tui::model::App| -> Vec<String> {
        let look = list::look(&a.shared, "other").expect("the look other");
        card::choices(&a.shared, &look, card::Line::Palette)
            .into_iter()
            .filter_map(|c| c.value)
            .collect()
    };
    let offers = offered(&a);
    assert!(offers.iter().any(|n| n == "mine"), "{offers:?}");
    assert!(
        !offers.iter().any(|n| n.contains("Catppuccin")),
        "{offers:?}"
    );
    // the desk's answer marks the file, and the card no longer offers it
    let mut states = active();
    states.extend([
        (etc.join("mine.palette"), State::Active),
        (etc.join("Catppuccin Mocha.palette"), State::Active),
        (root.join("pkg/dusk.palette"), State::Active),
    ]);
    let done = vtium_config::tui::fixture::resource_op(
        &mut states,
        &etc.join("mine.palette"),
        Change::Deactivate,
    )
    .expect("a deactivation");
    a.event(Event::Desked(op, Ok(done)));
    let state = |a: &vtium_config::tui::model::App, path: &Path| {
        a.shared
            .inputs
            .effects
            .palettes
            .iter()
            .find(|f| f.resource.path == path)
            .map(|f| f.resource.state)
    };
    assert_eq!(
        state(&a, &etc.join("mine.palette")),
        Some(State::Deactivates)
    );
    let screen = flat(&text(&a));
    assert!(
        screen.contains("mine: deactivates on install, trusted."),
        "{screen}"
    );
    assert!(
        !offered(&a).iter().any(|n| n == "mine"),
        "{:?}",
        offered(&a)
    );
    // a file no look uses deactivates at once, and one that is not active
    // activates, and is offered once the desk answers
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
    let op = Op::Resource {
        path: etc.join("spare.palette"),
        change: Change::Activate,
    };
    assert_eq!(press(&mut a, Key::Char(' ')), [Effect::Desk(op.clone())]);
    assert!(!offered(&a).iter().any(|n| n == "spare"));
    let done = vtium_config::tui::fixture::resource_op(
        &mut states,
        &etc.join("spare.palette"),
        Change::Activate,
    )
    .expect("an activation");
    a.event(Event::Desked(op, Ok(done)));
    assert_eq!(
        state(&a, &etc.join("spare.palette")),
        Some(State::Activates)
    );
    let screen = flat(&text(&a));
    assert!(
        screen.contains("spare: activates on install, trusted."),
        "{screen}"
    );
    assert!(
        offered(&a).iter().any(|n| n == "spare"),
        "{:?}",
        offered(&a)
    );
    let _ = std::fs::remove_dir_all(&root);
    // a built-in scheme is always active, and Space says so; the
    // fixture's loose says why it cannot be activated
    let mut a = looks_app(inputs(), 100, 30);
    to_entry(&mut a, &Entry::Palette(Scheme::BuiltIn("solarized")));
    let draft = a.shared.conf_text().map(str::to_owned);
    assert!(press(&mut a, Key::Char(' ')).is_empty());
    assert!(
        a.shared
            .message
            .as_deref()
            .is_some_and(|m| m.contains("always active")),
        "{:?}",
        a.shared.message
    );
    assert_eq!(a.shared.conf_text().map(str::to_owned), draft);
    assert_eq!(vtium_config::tui::nav::node(&a), "looks-resources");
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
        "Every look that names no pointer theme of its own uses it, because vtium.conf's pointer-theme names it.",
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
                "looks-resources",
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

/// The Looks tab highlights a font set it is asked to show, as Enter on a
/// font set in the Flow tab asks, and a name it does not list moves
/// nothing.
#[test]
fn the_looks_tab_highlights_the_font_set_it_is_asked_to_show() {
    use vtium_config::tui::tabs::looks::Entry;
    let mut a = looks_app(inputs(), 100, 30);
    let at = a
        .shared
        .inputs
        .fonts
        .iter()
        .position(|f| f.name == "proof")
        .expect("the font set proof");
    a.looks.show_font_set(&a.shared, "proof");
    assert_eq!(a.looks.highlighted(&a.shared), Some(Entry::FontSet(at)));
    a.looks.show_font_set(&a.shared, "nowhere");
    assert_eq!(a.looks.highlighted(&a.shared), Some(Entry::FontSet(at)));
}

/// The Looks tab's list is in the node `looks-resources` while a color
/// scheme or a pointer theme is highlighted, and in `looks` on a look or a
/// font set. Each key line names only the keys that act on its items: `n`,
/// `r`, `t` and `p` among the looks and the font sets, and Space among the
/// color schemes and the pointer themes. Space on a built-in scheme says
/// that the scheme is always active.
///
/// References:
/// - ADR-1403 D7, its note of 2026-11-18: each key line names only keys
///   that act where it is shown.
#[test]
fn the_key_line_names_only_the_keys_that_act_on_the_highlighted_item() {
    use vtium_config::tui::input::Key;
    use vtium_config::tui::tabs::looks::{Entry, palettes::Scheme};
    let mut a = looks_app(inputs(), 100, 30);
    let line =
        |a: &vtium_config::tui::model::App| vtium_config::tui::layout::key_line(a).to_owned();
    to_entry(&mut a, &Entry::Look("proof".into()));
    assert_eq!(vtium_config::tui::nav::node(&a), "looks");
    assert!(line(&a).contains("n new"), "{}", line(&a));
    assert!(!line(&a).contains("Space"), "{}", line(&a));
    to_entry(&mut a, &Entry::Palette(Scheme::BuiltIn("nord")));
    assert_eq!(vtium_config::tui::nav::node(&a), "looks-resources");
    for gone in ["n new", "t try live", "r remove", "p picture"] {
        assert!(!line(&a).contains(gone), "{gone}: {}", line(&a));
    }
    assert!(line(&a).contains("Space activate"), "{}", line(&a));
    press(&mut a, Key::Char(' '));
    assert!(
        flat(&text(&a)).contains("A color scheme built into vtium is always active"),
        "{}",
        text(&a)
    );
    // Up from the first color scheme goes back to the font sets
    to_entry(&mut a, &Entry::Palette(Scheme::BuiltIn("legacy")));
    while matches!(a.looks.highlighted(&a.shared), Some(Entry::Palette(_))) {
        press(&mut a, Key::Up);
    }
    assert_eq!(vtium_config::tui::nav::node(&a), "looks");
}

/// An install that adds a font set before the highlighted one keeps the
/// highlight, and an open font set's detail, on the same font set, though
/// the new set moves the others down the list.
#[test]
fn an_install_that_adds_a_font_set_keeps_the_highlight_on_its_font_set() {
    use vtium_config::tui::desk::{Done, Opened};
    use vtium_config::tui::input::Key;
    use vtium_config::tui::model::Event;
    use vtium_config::tui::tabs::looks::Entry;
    let name_of = |a: &vtium_config::tui::model::App| match a.looks.highlighted(&a.shared) {
        Some(Entry::FontSet(at)) => a.shared.inputs.fonts[at].name.clone(),
        other => panic!("{other:?}"),
    };
    let mut a = looks_app(inputs(), 100, 30);
    let at = a
        .shared
        .inputs
        .fonts
        .iter()
        .position(|f| f.name == "tiny5")
        .expect("tiny5");
    to_entry(&mut a, &Entry::FontSet(at));
    a.shared.draft = Some(Opened {
        frits: vtium_config::store::FritChanges {
            kept: [("aaa".to_owned(), "source=builtin:unifont\n".to_owned())].into(),
            ..Default::default()
        },
        ..Opened::default()
    });
    a.event(Event::Installed(Ok(Done::Draft(Opened::default(), None))));
    assert_eq!(name_of(&a), "tiny5");
    // the detail open over the list stays on its font set too
    press(&mut a, Key::Enter);
    a.shared.draft = Some(Opened {
        frits: vtium_config::store::FritChanges {
            kept: [("aab".to_owned(), "source=builtin:unifont\n".to_owned())].into(),
            ..Default::default()
        },
        ..Opened::default()
    });
    a.event(Event::Installed(Ok(Done::Draft(Opened::default(), None))));
    assert!(text(&a).contains("tiny5"), "{}", text(&a));
    press(&mut a, Key::Escape);
    assert_eq!(name_of(&a), "tiny5");
}

/// A pointer theme's folder name is the administrator's text, so the list
/// and the detail show it with a direction override replaced, as the
/// Fonts tab shows a font's name.
#[test]
fn a_pointer_theme_s_name_shows_no_direction_override() {
    use vtium_config::tui::input::Key;
    use vtium_config::tui::tabs::looks::Entry;
    let mut a = looks_app(inputs(), 100, 30);
    let mut theme = a.shared.inputs.effects.pointers[0].clone();
    theme.path = theme.path.with_file_name("evil\u{202e}txt");
    a.shared.inputs.effects.pointers.insert(0, theme);
    to_entry(&mut a, &Entry::Pointer(0));
    assert!(text(&a).contains("evil"), "{}", text(&a));
    assert!(!text(&a).contains('\u{202e}'), "{}", text(&a));
    press(&mut a, Key::Enter);
    assert_eq!(vtium_config::tui::nav::node(&a), "looks.pointer");
    assert!(!text(&a).contains('\u{202e}'), "{}", text(&a));
}

/// The fixture's vtium.conf with `allow-window-ops` on, so the console
/// lets programs resize its grid.
fn window_ops_on() -> String {
    format!(
        "# mine\nfont-name=unifont\nallow-window-ops\n{}",
        vtium_config::tui::fixture::SECTIONS
    )
}

/// The console of [`console`], driven with the stand-in desk on `conf`,
/// with what it wrote and the draft it left. The console takes grid
/// requests where `conf` turns `allow-window-ops` on. Besides its grid at
/// the console's height, the font set proof reaches 80×30 at 8 pixels, the
/// grid that the stand-in helper's 8×16 cell gives on the 640×480 display,
/// while unifont reaches only its own 80×25.
fn run_on(conf: &str, keys: &[&[u8]]) -> (Console, String) {
    run_with(console(), conf, keys)
}

/// The console `c`, driven as [`run_on`] drives [`console`].
fn run_with(c: Console, conf: &str, keys: &[&[u8]]) -> (Console, String) {
    let mut d = desk();
    d.installed = conf.to_owned();
    d.draft = conf.to_owned();
    let mut c = c.reaches("proof", 8, (80, 30)).keys(keys);
    if conf.contains("\nallow-window-ops\n") {
        c = c.resizes();
    }
    vtium_config::tui::port::drive_with(&mut c, with_conf(conf), Some(&mut d)).expect("the loop");
    (c, d.draft)
}

/// The keys that open the look proof's card, its size line's list, and
/// pick 6 pt, the first size after vtium.conf's and automatic.
const PICK_SIX: &[&[u8]] = &[TO_LOOKS, b"\r", DOWN, DOWN, b"\r", DOWN, DOWN, b"\r"];

/// `w` in the Looks tab asks whether to let programs resize the grid, and
/// `y` writes `allow-window-ops = on` into the draft.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": `w` asks, and only `y`
///   writes the key.
#[test]
fn w_asks_whether_to_let_programs_resize_the_grid_and_y_turns_it_on() {
    let seen = reach(80, 25, &[TO_LOOKS, b"w"]);
    assert!(seen.contains("Let programs resize the grid?"), "{seen}");
    let draft = draft_after(
        &inputs().conf.expect("the conf"),
        &[TO_LOOKS, b"w", b"y", b"s"],
    );
    assert!(draft.contains("\nallow-window-ops\n"), "{draft}");
}

/// `n` and Esc answer the question with no, and write nothing.
#[test]
fn n_or_esc_at_the_resize_question_writes_nothing() {
    let conf = inputs().conf.expect("the conf");
    for no in [b"n".as_slice(), ESC] {
        let draft = draft_after(&conf, &[TO_LOOKS, b"w", no, b"s"]);
        assert!(!draft.contains("allow-window-ops"), "{draft}");
    }
}

/// In a vtium console that does not let programs resize its grid, a look
/// card's size line says so and names `w`, and Enter on a size says why
/// the console stays as it is and sends no grid request. With the live
/// trial on, the preview is the console's own text at its current size,
/// so Enter says that the size takes effect when vtium starts again. With
/// `allow-font-ops` off, the preview is a picture or a description, so
/// Enter says that the size shows only in the preview.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console", its note of 2026-11-21: with the live trial on and `allow-window-ops` off, the preview is the console's own text and shows no picture.
#[test]
fn without_window_ops_the_tab_says_when_a_size_takes_effect() {
    let seen = flat(&reach(100, 30, &[TO_LOOKS, b"\r", DOWN, DOWN]));
    assert!(seen.contains(words::WINDOW_OPS_NOTICE_NEXT_START), "{seen}");
    let conf = inputs().conf.expect("the conf");
    let (c, _) = run_on(&conf, PICK_SIX);
    assert_eq!(grid_requests(&c.written), Vec::<String>::new());
    let seen = flat(&c.seen.expect("the screen"));
    assert!(seen.contains(words::SIZE_AT_NEXT_START), "{seen}");
    assert!(!seen.contains("shows only in the preview"), "{seen}");
    // with allow-font-ops off the console answers no font query, so the
    // live trial is off and the preview is a picture or a description
    let mut c = Console::vtium(80, 25, false, "unifont").keys(PICK_SIX);
    let mut d = desk();
    vtium_config::tui::port::drive_with(&mut c, with_conf(&conf), Some(&mut d)).expect("the loop");
    assert_eq!(grid_requests(&c.written), Vec::<String>::new());
    let seen = flat(&c.seen.expect("the screen"));
    assert!(seen.contains(words::SIZE_PICTURE_ONLY), "{seen}");
    assert!(seen.contains(words::WINDOW_OPS_NOTICE), "{seen}");
}

/// In a vtium console that lets programs resize its grid but has
/// `allow-font-ops` off, Enter on a size tries nothing and sends no grid
/// request. vtium meets a grid request with the zoom steps of the font it
/// draws, and with font operations off vtium-config can neither switch the
/// console to the look's font set nor learn which font set the console
/// draws. A grid measured in the look's font set could then land on the
/// console's own font, which may not reach it and would draw its old
/// glyphs in a smaller grid in the middle of the screen. The message says
/// that trying a size needs `allow-font-ops` too, and how to turn it on.
/// `t` on the look, whose trial would ask for its size's grid too, says
/// that the live trial is off and sends nothing either.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": vtium meets the grid
///   request with the zoom step of the font it draws.
#[test]
fn without_font_ops_enter_on_a_size_tries_nothing_and_says_why() {
    let ops_off = || {
        Console::vtium(80, 25, false, "unifont")
            .reaches("proof", 8, (80, 30))
            .resizes()
    };
    let (c, _) = run_with(ops_off(), &window_ops_on(), &[TO_LOOKS, b"t"]);
    assert_eq!(
        grid_requests(&c.written),
        Vec::<String>::new(),
        "{:?}",
        c.log
    );
    assert_eq!(c.switches(), Vec::<String>::new(), "{:?}", c.log);
    let seen = flat(&c.seen.expect("the screen"));
    assert!(seen.contains(words::TRIAL_OFF), "{seen}");
    let mut keys = PICK_SIX.to_vec();
    keys.push(b"x");
    let (c, draft) = run_with(ops_off(), &window_ops_on(), &keys);
    assert_eq!(
        grid_requests(&c.written),
        Vec::<String>::new(),
        "{:?}",
        c.log
    );
    assert_eq!(c.switches(), Vec::<String>::new(), "{:?}", c.log);
    assert_eq!(
        (c.shown.as_str(), c.grid(), c.pinned),
        ("unifont", (80, 25), None),
        "{:?}",
        c.log
    );
    let seen = flat(&c.seen.expect("the screen"));
    assert!(seen.contains(words::SIZE_NEEDS_FONT_OPS), "{seen}");
    assert!(!seen.contains("Trying 6 pt"), "{seen}");
    // the pick is still a change to the look, kept for Save
    assert!(draft.contains("[look proof]\nfont-size = 6\n"), "{draft}");
}

/// Enter on a size in the look card's list tries it on the console: the
/// font helper measures the font set's cell at that size, and the grid
/// request asks for the grid that cell gives on the 640×480 display, 80×30
/// for an 8×16 cell. Any key but `k` puts the grid of 80×25 back, and then
/// lets the grid fill the display again.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": Enter applies the
///   highlighted size as a timed trial, and any other key puts the grid
///   back.
#[test]
fn enter_on_a_size_tries_its_grid_on_the_console_and_a_key_puts_it_back() {
    let mut keys = PICK_SIX.to_vec();
    keys.push(b"x");
    let (c, draft) = run_on(&window_ops_on(), &keys);
    assert!(c.cells.contains(&("proof".to_owned(), 8)), "{:?}", c.cells);
    // the trial's grid, then the grid put back, which is the starting
    // grid, and the request that lets it fill the display, so leaving
    // sends no other
    assert_eq!(grid_requests(&c.written), ["30;80", "25;80", "0;0"]);
    // the pick is a change to the look like any other, kept for Save
    assert!(draft.contains("[look proof]\nfont-size = 6\n"), "{draft}");
}

/// Enter on a size tries the look's font set at that size. The console
/// draws unifont and the look proof's font set is proof, so vtium-config
/// switches the console to proof first, and asks for proof's grid at 6 pt
/// once the console draws proof. unifont alone cannot reach that grid.
/// Any key but `k` then gives the console back its font and then its
/// grid, and lets the grid fill the display again, so no grid that the
/// trial asked for outlives it.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": the preview shows the
///   look at that size, and vtium meets the grid request with the zoom
///   step of the font it draws.
/// - ADR-1201 D4, "The live trial, on a key": any other key puts the
///   console back as it was before the trial.
#[test]
fn enter_on_a_size_tries_the_look_s_font_set_at_that_size() {
    let mut keys = PICK_SIX.to_vec();
    keys.push(b"x");
    let (c, _) = run_on(&window_ops_on(), &keys);
    let at = |needle: &[u8]| find(&c.written, needle);
    let font = at(b"\x1b]50;proof").expect("the look's font set");
    let grid = at(b"\x1b[8;30;80t").expect("the size's grid");
    let back = at(b"\x1b]50;unifont").expect("the console's font given back");
    let grid_back = at(b"\x1b[8;25;80t").expect("the console's grid given back");
    assert!(
        font < grid && grid < back && back < grid_back,
        "{:?}",
        c.log
    );
    // the console took the grid whole in proof, and the put-back leaves
    // it filling the display in unifont
    let tried = c.log.iter().position(|l| l == "set proof").expect("proof");
    assert_eq!(
        c.log.get(tried + 2).map(String::as_str),
        Some("grid 30;80"),
        "{:?}",
        c.log
    );
    assert_eq!(
        (c.shown.as_str(), c.grid(), c.pinned),
        ("unifont", (80, 25), None),
        "{:?}",
        c.log
    );
}

/// After `k` keeps a size tried in the look's font set, and while such a
/// trial still counts down, leaving vtium-config gives the console back
/// its font and then its grid, and the grid fills the display again.
///
/// References:
/// - ADR-1201 D4, "The live trial, on a key": a kept font and a kept
///   look's grid stay on the console until the TUI leaves, which gives
///   the start ones back.
/// - ADR-1201 D4, "Giving the console back": leaving gives back the start
///   font, then the start grid.
#[test]
fn leaving_after_a_size_trial_gives_the_console_its_font_and_grid_back() {
    let kept: &[&[u8]] = &[b"k", ESC, ESC, b"q", b"n"];
    // a signal leaves while the trial still counts down
    let running: &[&[u8]] = &[b"!15"];
    for (how, then) in [("kept", kept), ("running", running)] {
        let mut keys = PICK_SIX.to_vec();
        keys.extend_from_slice(then);
        let (c, _) = run_on(&window_ops_on(), &keys);
        let tried = find(&c.written, b"\x1b[8;30;80t").expect("the size's grid request");
        let rest = &c.written[tried..];
        let font = find(rest, b"\x1b]50;unifont").expect("the start font");
        let grid = find(rest, b"\x1b[8;25;80t").expect("the start grid");
        assert!(font < grid, "{how}: {:?}", c.log);
        assert_eq!(
            (c.shown.as_str(), c.grid(), c.pinned),
            ("unifont", (80, 25), None),
            "{how}: {:?}",
            c.log
        );
    }
}

/// A size tried in the font set the console already draws changes only
/// the grid, and putting it back lets the grid fill the display again.
/// Otherwise the grid stays pinned at the start grid's rows and columns,
/// and the next font set tried draws in a small grid in the middle of the
/// screen.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": any other key puts
///   the console's grid back.
#[test]
fn putting_back_a_size_in_the_console_s_font_set_unpins_the_grid() {
    let mut keys = PICK_SIX.to_vec();
    keys.push(b"x");
    let proof = Console::vtium(80, 25, true, "proof")
        .takes("proof", (80, 25))
        .takes("unifont", (80, 25));
    let (c, _) = run_with(proof, &window_ops_on(), &keys);
    assert_eq!(c.switches(), Vec::<String>::new());
    assert_eq!(grid_requests(&c.written), ["30;80", "25;80", "0;0"]);
    assert_eq!(
        (c.shown.as_str(), c.grid(), c.pinned),
        ("proof", (80, 25), None),
        "{:?}",
        c.log
    );
}

/// A vtium console whose grid vtium-config cannot guess: 213 columns by
/// 60 rows of unifont, as a wide display holds them. unifont can also
/// draw at 8 pixels high, where it fills the display with 426×120. The
/// look proof's font set has a larger cell, so proof fills the display
/// with 64×20 at the height the console starts at, and switching the
/// console to proof moves its grid. [`run_with`] gives proof the step at
/// 8 pixels where it fills 80×30, the grid of the trial's 6 pt.
fn wide() -> Console {
    Console::vtium(213, 60, true, "unifont")
        .takes("unifont", (213, 60))
        .reaches("unifont", 8, (426, 120))
        .takes("proof", (64, 20))
}

/// A size trial that ends while vtium-config keeps running gives the
/// console back the grid it had before the trial: unifont's own 213×60
/// at the height the console started at, filling the display with no
/// grid pinned. The grid that proof gave the console between the font
/// switch and the grid request is not the console's grid before the
/// trial, so the put-back never asks for it. vtium-config's own screen
/// follows the console back to 213×60.
///
/// References:
/// - ADR-1201 D4, "The live trial, on a key": any other key puts the
///   console back as it was before the trial.
#[test]
fn a_size_trial_put_back_leaves_the_console_filling_its_display() {
    let mut keys = PICK_SIX.to_vec();
    keys.push(b"x");
    let (c, _) = run_with(wide(), &window_ops_on(), &keys);
    let tried = c.log.iter().position(|l| l == "set proof").expect("proof");
    assert_eq!(
        c.log.get(tried + 2).map(String::as_str),
        Some("grid 30;80"),
        "{:?}",
        c.log
    );
    assert_eq!(
        c.seen_console,
        Some(Drew {
            font: "unifont".to_owned(),
            height: 16,
            grid: (213, 60),
            pinned: None,
        }),
        "{:?}",
        c.log
    );
    let seen = c.seen.expect("the screen");
    assert_eq!(seen.lines().count(), 60, "{seen}");
    assert!(
        seen.lines().any(|l| l.chars().count() > 200),
        "vtium-config draws across the console's 213 columns: {seen}"
    );
}

/// A vtium console whose displays are 7680×4320. unifont fills them at
/// 480×180, with a 16×24 cell. proof fills them at 480×180 too at the
/// height the console starts at, and at 960×270 at 8 pixels, with the
/// stand-in helper's 8×16 cell. The displays' list says that the first
/// display prefers 5120×1440, a mode the console does not run at.
fn eight_k() -> (Console, Inputs) {
    let c = Console::vtium(480, 180, true, "unifont")
        .takes("unifont", (480, 180))
        .takes("proof", (480, 180))
        .reaches("proof", 8, (960, 270))
        .fills((7680, 4320));
    let mut i = with_conf(&window_ops_on());
    i.display = Some((5120, 1440));
    (c, i)
}

/// The stand-in console answers XTWINOPS 14 as vtium does: with the
/// text area in pixels, the grid times the cell of the font it draws. A
/// grid request gives the terminal the cell of the zoom step it reaches,
/// and a pinned grid smaller than the displays gives a smaller text area.
#[test]
fn the_stand_in_console_reports_its_text_area_as_vtium_does() {
    let (c, _) = eight_k();
    let mut c = c.resizes();
    c.write(b"\x1b[14t").expect("the query");
    c.write(b"\x1b]50;proof\x1b\\\x1b[8;270;960t\x1b[14t")
        .expect("the switch and the grid");
    c.write(b"\x1b[8;100;900t\x1b[14t").expect("a smaller grid");
    let replies: Vec<u8> = c
        .pending
        .iter()
        .filter_map(|w| match w {
            Wake::Bytes(b) => Some(b.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    let text = String::from_utf8_lossy(&replies);
    assert_eq!(text.matches("\x1b[4;4320;7680t").count(), 2, "{text:?}");
    assert_eq!(text.matches("\x1b[4;1600;7200t").count(), 1, "{text:?}");
    assert_eq!((c.height, c.grid()), (8, (900, 100)), "{:?}", c.log);
}

/// In a vtium console, a size's live trial sizes its grid from the text
/// area the console reports in pixels (XTWINOPS 14), divided by the look's
/// cell, and not from the mode that the displays' list says the display
/// prefers. On a console that runs at 7680×4320 while its display prefers
/// 5120×1440, proof's 8×16 cell asks for 960×270, the grid that fills the
/// console, so the console draws no bars around it.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": the size is tried on
///   the console with a grid request for the grid the look's cell gives.
#[test]
fn a_size_trial_asks_for_the_grid_the_consoles_text_area_holds() {
    let (c, i) = eight_k();
    let mut d = desk();
    d.installed = window_ops_on();
    d.draft = window_ops_on();
    let mut c = c.resizes().keys(PICK_SIX);
    vtium_config::tui::port::drive_with(&mut c, i, Some(&mut d)).expect("the loop");
    let tried = c.log.iter().position(|l| l == "set proof").expect("proof");
    let grid = c.log[tried..].iter().find(|l| l.starts_with("grid "));
    assert_eq!(
        grid.map(String::as_str),
        Some("grid 270;960"),
        "{:?}",
        c.log
    );
    assert!(
        String::from_utf8_lossy(&c.written).contains("\x1b[14t"),
        "vtium-config asks for the text area"
    );
}

/// An 8K display on DP-1: it prefers 5120×1440 and offers 7680×4320
/// too.
fn eight_k_display() -> vtium_config::display::Display {
    vtium_config::display::Display {
        pixels: (5120, 1440),
        mm: None,
        connector: "DP-1".to_owned(),
        card: None,
        modes: vec![(5120, 1440), (7680, 4320)],
    }
}

/// A vtium console reports its text area cut down to whole cells: 426×120
/// cells of 18×36 on the 7680×4320 display is 7668×4320. Where the
/// draft's vtium.conf sets the display's mode to 7680×4320, which that
/// text area is cut from, a size's trial counts its grid on the whole
/// mode, as vtium lays the look out on it: 960 columns of proof's 8×16
/// cell, where the text area alone holds 958.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": the grid request.
#[test]
fn a_size_trial_counts_the_grid_on_the_mode_the_text_area_is_cut_from() {
    let conf = format!("mode=[DP-1]7680x4320@120\n{}", window_ops_on());
    let c = Console::vtium(426, 120, true, "unifont")
        .takes("unifont", (426, 120))
        .takes("proof", (426, 120))
        .reaches("proof", 8, (960, 270))
        .fills((7680, 4320))
        .resizes()
        .keys(PICK_SIX);
    let mut i = with_conf(&conf);
    i.display = Some((5120, 1440));
    i.displays = vec![eight_k_display()];
    let mut d = desk();
    d.installed = conf.clone();
    d.draft = conf.clone();
    let mut c = c;
    vtium_config::tui::port::drive_with(&mut c, i, Some(&mut d)).expect("the loop");
    let tried = c.log.iter().position(|l| l == "set proof").expect("proof");
    let grid = c.log[tried..].iter().find(|l| l.starts_with("grid "));
    assert_eq!(
        grid.map(String::as_str),
        Some("grid 270;960"),
        "{:?}",
        c.log
    );
}

/// Outside a vtium console, the Looks preview counts a size's grid on the
/// mode that the draft's vtium.conf sets for the first display, and not
/// on the mode the display prefers.
///
/// References:
/// - ADR-1201 D4: the line beside the preview counts the display's grid.
#[test]
fn the_looks_preview_counts_the_grid_on_the_mode_the_draft_sets() {
    let mut c = outside().keys(&[TO_LOOKS, DOWN]);
    c.graphics = true;
    let mut i = sized();
    i.conf = i.conf.map(|c| format!("mode=[DP-1]7680x4320@120\n{c}"));
    i.display = Some((5120, 1440));
    i.displays = vec![eight_k_display()];
    drive(&mut c, i).expect("the loop");
    let shot = c.shots.last().expect("a picture");
    let (cw, ch) = (shot.cell.width, shot.cell.height);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    let grid = format!(
        "the console will be {} characters wide and {} lines high",
        7680 / cw,
        4320 / ch
    );
    assert!(text.contains(&grid), "{grid}: {screen}");
}

/// After `k` keeps a size, the console's grid stays pinned at the kept
/// grid until vtium-config leaves. A later look's trial in another font
/// set gives the console back that kept grid when it ends, with the kept
/// font set's height, and leaves the grid pinned there, not at the grid
/// the trial's font set gave the console. Leaving then gives the console
/// its start grid, filling the display.
///
/// References:
/// - ADR-1201 D4, "The live trial, on a key": a kept look's grid stays on
///   the console until the TUI leaves, which gives the start ones back.
#[test]
fn a_trial_after_a_kept_size_gives_back_the_kept_grid() {
    let conf = "# mine\nfont-name=unifont\nallow-window-ops\nversion = 1\n[look proof]\nfont-name = proof\n[look small]\nfont-name = tiny5\nfont-size = 6\n[head desk]\nlook = proof\n[head panel]\n[rules]\ndisplay * -> desk\n";
    let mut keys = PICK_SIX.to_vec();
    keys.extend_from_slice(&[b"k".as_slice(), ESC, ESC, DOWN, b"t", b"x"]);
    let (c, _) = run_with(wide().takes("tiny5", (40, 20)), conf, &keys);
    // the look small's trial asked for tiny5's grid at 6 pt
    let small = c.log.iter().position(|l| l == "set tiny5").expect("tiny5");
    assert!(
        c.log[small..].iter().any(|l| l.starts_with("grid 80;160")),
        "{:?}",
        c.log
    );
    assert_eq!(
        c.seen_console,
        Some(Drew {
            font: "proof".to_owned(),
            height: 8,
            grid: (80, 30),
            pinned: Some((80, 30)),
        }),
        "{:?}",
        c.log
    );
    assert_eq!(
        (c.shown.as_str(), c.height, c.grid(), c.pinned),
        ("unifont", 16, (213, 60), None),
        "{:?}",
        c.log
    );
}

/// An install that turns `allow-window-ops` on does not change the
/// running console, which read the key when vtium made its head. So a
/// size picked after the install sends no grid request and says that the
/// console resizes its grid only once vtium starts again.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": the size is tried
///   only where the console lets programs resize its grid.
#[test]
fn installing_window_ops_does_not_turn_them_on_in_the_running_console() {
    let conf = inputs().conf.expect("the conf");
    let mut d = desk();
    d.installed = conf.clone();
    d.draft = conf.clone();
    let mut keys: Vec<&[u8]> = vec![TO_LOOKS, b"w", b"y", b"i", b"y"];
    keys.extend_from_slice(&[b"\r", DOWN, DOWN, b"\r", DOWN, DOWN, b"\r"]);
    let mut c = console().keys(&keys);
    vtium_config::tui::port::drive_with(&mut c, with_conf(&conf), Some(&mut d)).expect("the loop");
    assert!(d.installed.contains("allow-window-ops"), "{}", d.installed);
    let seen = flat(&c.seen.expect("the screen"));
    assert_eq!(grid_requests(&c.written), Vec::<String>::new(), "{seen}");
    assert!(!seen.contains("Trying 6 pt on the console"), "{seen}");
    assert!(
        seen.contains("The console does not let programs resize its grid until vtium starts again, because vtium reads allow-window-ops when it starts."),
        "{seen}"
    );
}

/// Putting a tried size back gives the console its grid back, and the
/// look keeps the size it was given, as a change to save. The message
/// says both, so a person does not take the key for an undo of the pick.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": any key but `k` puts
///   the grid back.
#[test]
fn putting_a_tried_size_back_says_the_look_still_holds_it() {
    let mut keys = PICK_SIX.to_vec();
    keys.push(b"x");
    let (c, _) = run_on(&window_ops_on(), &keys);
    let seen = flat(&c.seen.expect("the screen"));
    assert!(
        seen.contains("The console is back to unifont at 80×25. The look still holds 6 pt as a change to save. Pick another size to change it."),
        "{seen}"
    );
}

/// Enter on Automatic, which sets no size of its own, tries nothing on
/// the console, and the message says why.
#[test]
fn enter_on_automatic_says_there_is_no_size_to_try() {
    let keys: &[&[u8]] = &[TO_LOOKS, b"\r", DOWN, DOWN, b"\r", DOWN, b"\r"];
    let (c, _) = run_on(&window_ops_on(), keys);
    assert_eq!(grid_requests(&c.written), Vec::<String>::new());
    let seen = flat(&c.seen.expect("the screen"));
    assert!(
        seen.contains(
            "This choice sets no size of its own, so there is nothing to try on the console."
        ),
        "{seen}"
    );
}

/// `k` keeps the size on the console, and quitting gives the console the
/// grid it had when vtium-config started.
///
/// References:
/// - ADR-1201 D4, "Giving the console back": leaving gives the console
///   its starting grid back, with the grid request.
#[test]
fn k_keeps_a_tried_size_and_quitting_gives_the_starting_grid_back() {
    let mut keys = PICK_SIX.to_vec();
    keys.extend_from_slice(&[b"k".as_slice(), ESC, ESC, b"q", b"n"]);
    let (c, _) = run_on(&window_ops_on(), &keys);
    let tried = find(&c.written, b"\x1b[8;30;80t").expect("the size's grid request");
    let rest = &c.written[tried..];
    // the keep sends nothing, and the leaving asks for the starting grid
    assert!(
        find(rest, b"\x1b[8;25;80t").is_some(),
        "the starting grid given back"
    );
    assert_eq!(grid_requests(&c.written), ["30;80", "25;80", "0;0"]);
}

/// Each grid request, `CSI 8 ; rows ; cols t`, in `bytes`, as its
/// parameters.
fn grid_requests(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    text.split("\x1b[8;")
        .skip(1)
        .filter_map(|rest| {
            let end = rest.find(|c: char| !c.is_ascii_digit() && c != ';')?;
            (rest[end..].starts_with('t')).then(|| rest[..end].to_owned())
        })
        .collect()
}

/// The position of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The keys that open the look proof's card on its pointer theme line,
/// the card's sixth.
const TO_POINTER: &[&[u8]] = &[TO_LOOKS, b"\r", DOWN, DOWN, DOWN, DOWN, DOWN];

/// The pointer theme line's list offers vtium.conf's theme first, then
/// each active pointer theme: in the fixture, vtium's own pictures, while
/// Adwaita, which is not active, and stray, which fails the trust rule,
/// stay out.
///
/// References:
/// - ADR-1205 D6: a look picks only an active pointer theme.
/// - ADR-1403 D10, the row `list.pointer-themes`.
#[test]
fn the_pointer_theme_list_offers_vtium_conf_s_theme_then_the_active_ones() {
    let mut keys = TO_POINTER.to_vec();
    keys.push(b"\r");
    let seen = flat(&reach(100, 30, &keys));
    assert!(
        seen.contains("The pointer theme vtium.conf sets (vtium's own pictures)"),
        "{seen}"
    );
    assert!(seen.contains("pointers.svg active"), "{seen}");
    assert!(!seen.contains("Adwaita"), "{seen}");
    assert!(!seen.contains("stray"), "{seen}");
}

/// Picking a pointer theme writes the look's own `pointer-theme`, and the
/// first choice takes it out again, so the look draws vtium.conf's theme.
///
/// References:
/// - ADR-1205 D6: a look's choice takes priority over vtium.conf's
///   `pointer-theme`, which stays the default for a look that picks none.
#[test]
fn picking_a_pointer_theme_writes_the_look_s_own_pointer_theme() {
    let conf = inputs().conf.expect("the conf");
    let mut keys = TO_POINTER.to_vec();
    keys.extend_from_slice(&[b"\r".as_slice(), DOWN, b"\r", ESC, b"s"]);
    let draft = draft_after(&conf, &keys);
    assert!(
        draft.contains("[look proof]\npointer-theme = /usr/share/vtium/pointers.svg\n"),
        "{draft}"
    );
    let mut keys = TO_POINTER.to_vec();
    keys.extend_from_slice(&[b"\r".as_slice(), DOWN, b"\r", b"\r", UP, b"\r", ESC, b"s"]);
    let draft = draft_after(&conf, &keys);
    assert!(!draft.contains("pointer-theme"), "{draft}");
}

/// The keys that open the card of the look seven, which draws unifont at
/// 7 pt, and its size line's list, highlighted on 7 pt; then Down
/// highlights 8 pt.
const SEVEN_SIZES: &[&[u8]] = &[TO_LOOKS, DOWN, DOWN, b"\r", DOWN, DOWN, b"\r", DOWN];

/// The pixel height of `points` at the fixture's 96 dpi.
fn height(points: u32) -> u32 {
    vtium_config::specimen::pixel_height(points, 96)
}

/// In a terminal that shows images, a look card's size list shows the
/// picture of the highlighted size below the list, and moving the
/// highlight draws the next size. Esc back to the card deletes the
/// picture, since the card's other lines have no preview.
///
/// References:
/// - ADR-1201 D4, "Sizes, live on a vtium console": the preview follows the
///   highlight over the sizes, and its note of 2026-11-21 puts it below the
///   card's size list.
#[test]
fn a_look_card_s_size_list_shows_the_picture_of_the_highlighted_size_below_it() {
    let mut keys = SEVEN_SIZES.to_vec();
    keys.push(ESC);
    let mut c = outside().keys(&keys);
    c.graphics = true;
    drive(&mut c, sized()).expect("the loop");
    // below the list of 8 rows and its 5 rows of sentences, from the left
    // edge: 79 columns by the 3 rows above the longest message, in the
    // fixture's 8x16 cells
    let area = (79 * 8, 3 * 16);
    let below: Vec<(u32, (u32, u32))> = c
        .pictures
        .iter()
        .filter(|(f, _, a)| f == "unifont" && *a == area)
        .map(|(_, h, a)| (*h, *a))
        .collect();
    assert_eq!(
        below,
        [(height(7), area), (height(8), area)],
        "{:?}",
        c.pictures
    );
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
    assert!(last_delete > last_show, "Esc deletes the picture");
}

/// The screen a look card's size list shows at 80x25 in a terminal that
/// shows images: the list, then what the keys do and the line beside the
/// picture, which names the highlighted size.
#[test]
fn the_line_under_a_look_card_s_size_list_names_the_highlighted_size() {
    let mut c = outside().keys(SEVEN_SIZES);
    c.graphics = true;
    drive(&mut c, sized()).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    let rows = body(&screen);
    let last = rows
        .iter()
        .rposition(|r| r.starts_with("   ○ ") || r.starts_with("   ● "))
        .expect("the list's rows");
    let under = flat(&rows[last + 1..].join(" "));
    assert!(under.starts_with(words::CARD_LIST_HINT), "{screen}");
    assert!(under.contains("unifont at 8 pt"), "{screen}");
}

/// Where the terminal shows no images, a look card's size list describes
/// the highlighted size under the list, and moving the highlight
/// describes the next size.
#[test]
fn without_pictures_a_look_card_s_size_list_describes_the_highlighted_size() {
    let mut c = outside().keys(SEVEN_SIZES);
    drive(&mut c, sized()).expect("the loop");
    let heights: Vec<u32> = c.summaries.iter().map(|(_, h)| *h).collect();
    assert_eq!(heights.last(), Some(&height(8)), "{heights:?}");
    assert!(heights.contains(&height(7)), "{heights:?}");
    let screen = c.seen.clone().expect("the screen");
    let rows = body(&screen);
    let last = rows
        .iter()
        .rposition(|r| r.starts_with("   ○ ") || r.starts_with("   ● "))
        .expect("the list's rows");
    let under = flat(&rows[last + 1..].join(" "));
    assert!(under.contains("With this font and size"), "{screen}");
    assert!(c.pictures.is_empty(), "{:?}", c.pictures);
}

/// In a vtium console a look card's size list shows the console's own
/// text below the list, at the console's current size, and the caption
/// says what Enter does to the size: it tries the size where the console
/// lets programs resize its grid, and otherwise the size waits for vtium's
/// next start.
///
/// References:
/// - ADR-1201 D4, "In a vtium console, the preview is the console's own
///   text": no picture, and a chosen size changes the console's grid.
#[test]
fn in_a_vtium_console_a_look_card_s_size_list_prints_the_sample_below_it() {
    let keys: &[&[u8]] = &[TO_LOOKS, b"\r", DOWN, DOWN, b"\r"];
    let conf = inputs().conf.expect("the conf");
    for (conf, caption) in [
        (conf, words::CARD_CONSOLE_SIZE_FIXED),
        (window_ops_on(), words::CARD_CONSOLE_SIZE_TRY),
    ] {
        let (c, _) = run_on(&conf, keys);
        let screen = c.seen.clone().expect("the screen");
        let rows = body(&screen);
        let last = rows
            .iter()
            .rposition(|r| r.starts_with("   ○ ") || r.starts_with("   ● "))
            .expect("the list's rows");
        let under = rows[last + 1..].join("\n");
        assert!(flat(&under).contains(caption), "{screen}");
        let sample: Vec<String> = vtium_config::tui::shot::sample_rows()
            .iter()
            .map(|line| line.iter().map(|(t, _)| *t).collect::<String>())
            .collect();
        let first = sample.first().expect("a sample row");
        let printed = preview_rows(&under, first.trim());
        assert!(printed.len() >= 3, "{screen}");
        for (row, line) in printed.iter().take(3).zip(&sample) {
            assert_eq!(row, line.trim(), "{screen}");
        }
        assert!(c.pictures.is_empty() && c.cells.is_empty());
    }
}

/// A long written description keeps its place under a look card's size
/// list: the list's window gives up rows first, so the sentence that says
/// why a size cannot be used still shows at 80x25 when the font helper's
/// reason names a path of 300 characters.
#[test]
fn a_long_reason_under_the_size_list_shrinks_the_list_before_it_goes() {
    use vtium_config::tui::input::{Input, Key};
    use vtium_config::tui::model::{App, Effect, Event, Live, Size, View};
    use vtium_config::tui::view;
    let mut i = sized();
    i.view = View::Looks;
    let (mut app, _) = App::start(
        i,
        Size {
            cols: 80,
            rows: 25,
            cell_px: Some((8, 16)),
        },
    );
    app.shared.live = Live::OpsOff;
    app.shared.graphics = false;
    // the card of look seven, its size list, and Down to 8 pt
    let mut asked = None;
    for key in [
        Key::Down,
        Key::Down,
        Key::Enter,
        Key::Down,
        Key::Down,
        Key::Enter,
        Key::Down,
    ] {
        for effect in app.event(Event::Input(Input::Key(key))) {
            if let Effect::Summary { font, height } = effect {
                asked = Some((font, height));
            }
        }
        app.shared.drawing = false;
    }
    let (font, height) = asked.expect("the description of a size asked for");
    let why = format!(
        "cannot read /usr/share/fonts/{}/x.otb: No such file or directory",
        "d".repeat(300)
    );
    app.event(Event::Summarized {
        font,
        height,
        summary: Err(why),
    });
    let screen = view::as_text(&view::screen(&app));
    let said = flat(&screen);
    assert!(
        said.contains("This size cannot be used: cannot read"),
        "{screen}"
    );
    assert!(said.contains("No such file or directory"), "{screen}");
    // the list still shows the highlighted size, in a window of at least
    // three rows
    let rows = body(&screen);
    let listed = rows
        .iter()
        .filter(|r| r.starts_with("   ○ ") || r.starts_with("   ● "))
        .count();
    assert!(listed >= 3, "{screen}");
}

/// A written description too long to leave the size list three rows makes
/// the card give up the preview under its size list: the card shows its
/// list and the hint under it, as it does with no preview, rather than a
/// window of the list over blank rows. The reason here names a path of 800
/// characters, which leaves the list fewer than three rows at 80x25.
#[test]
fn a_reason_too_long_for_the_size_list_leaves_the_list_and_its_hint() {
    use vtium_config::tui::input::{Input, Key};
    use vtium_config::tui::model::{App, Effect, Event, Live, Size, View};
    use vtium_config::tui::view;
    let mut i = sized();
    i.view = View::Looks;
    let (mut app, _) = App::start(
        i,
        Size {
            cols: 80,
            rows: 25,
            cell_px: Some((8, 16)),
        },
    );
    app.shared.live = Live::OpsOff;
    app.shared.graphics = false;
    // the card of look seven, its size list, and Down to 8 pt
    let mut asked = None;
    for key in [
        Key::Down,
        Key::Down,
        Key::Enter,
        Key::Down,
        Key::Down,
        Key::Enter,
        Key::Down,
    ] {
        for effect in app.event(Event::Input(Input::Key(key))) {
            if let Effect::Summary { font, height } = effect {
                asked = Some((font, height));
            }
        }
        app.shared.drawing = false;
    }
    let (font, height) = asked.expect("the description of a size asked for");
    let why = format!(
        "cannot read /usr/share/fonts/{}/x.otb: No such file or directory",
        "d".repeat(800)
    );
    app.event(Event::Summarized {
        font,
        height,
        summary: Err(why),
    });
    let screen = view::as_text(&view::screen(&app));
    let said = flat(&screen);
    assert!(
        said.contains(
            "Enter picks the highlighted choice, and Esc goes back to the card without a change."
        ),
        "{screen}"
    );
    assert!(!said.contains("dddd"), "{screen}");
    assert!(body(&screen).iter().any(|r| r == "   ● 7 pt"), "{screen}");
}

/// In a vtium console, a look card's size list prints the sample screen
/// as its preview only where the screen leaves the preview a row, as the
/// layout of the list and its sentences decides. On a screen too short
/// for that, the card shows its list alone, and no sample lines would
/// print over the list.
#[test]
fn the_console_s_text_under_the_size_list_follows_the_layout() {
    use vtium_config::tui::input::{Input, Key};
    use vtium_config::tui::model::{App, Event, Live, Size, View};
    let mut i = sized();
    i.view = View::Looks;
    let size = |rows| Size {
        cols: 80,
        rows,
        cell_px: Some((8, 16)),
    };
    let (mut app, _) = App::start(i, size(25));
    app.shared.live = Live::On;
    // the card of look seven and its size list
    for key in [
        Key::Down,
        Key::Down,
        Key::Enter,
        Key::Down,
        Key::Down,
        Key::Enter,
    ] {
        app.event(Event::Input(Input::Key(key)));
    }
    let sample = vtium_config::tui::shot::sample_rows();
    assert_eq!(app.tab().console_lines(&app.shared, size(25)), sample);
    assert!(app.tab().console_lines(&app.shared, size(12)).is_empty());
}

/// `u` on the screen below the minimum size puts back the font and the
/// grid the console had when vtium-config started. Here a size in the
/// look's font set proof and then the font set tiny5 were kept from their
/// trials, and tiny5 leaves a grid of 30×8. `u` switches the console back
/// to its start font first, then asks for the start grid, because the
/// font switch changes the grid too.
///
/// References:
/// - ADR-1201 D3: `u` on the screen below the minimum size puts back the
///   console's start font and grid.
/// - ADR-1201 D4, "Giving the console back": the start font and the
///   start grid that leaving gives back, which `u` gives back too.
#[test]
fn u_below_the_minimum_puts_back_the_start_font_then_the_start_grid() {
    let conf = window_ops_on();
    let mut keys = PICK_SIX.to_vec();
    // keep the size, close the list and the card, then try tiny5, the
    // fifth entry of the list, and keep it too
    keys.extend_from_slice(&[
        b"k".as_slice(),
        ESC,
        ESC,
        DOWN,
        DOWN,
        DOWN,
        DOWN,
        b"t",
        b"k",
    ]);
    keys.push(b"u");
    let mut d = desk();
    d.installed = conf.clone();
    d.draft = conf.clone();
    let mut c = Console::vtium(80, 25, true, "unifont")
        .takes("proof", (80, 25))
        .takes("tiny5", (30, 8))
        .takes("unifont", (80, 25))
        .keys(&keys);
    vtium_config::tui::port::drive_with(&mut c, with_conf(&conf), Some(&mut d)).expect("the loop");
    assert_eq!(c.switches(), ["proof", "tiny5", "unifont"]);
    let switch = find(&c.written, b"\x1b]50;unifont").expect("the start font's switch");
    let grid = c
        .written
        .windows(b"\x1b[8;25;80t".len())
        .rposition(|w| w == b"\x1b[8;25;80t")
        .expect("the start grid's request");
    assert!(
        switch < grid,
        "u asked for the start grid before the start font"
    );
}

/// A vtium.conf whose look `nordic` sets the font set proof, a size and the
/// color scheme nord, with `top` at its top level and `head` as the head
/// desk's section.
use vtium_config::tui::input::{Input, Key, Reply};
use vtium_config::tui::model::{App, Effect, Event};

fn nordic_conf(top: &str, head: &str) -> String {
    format!(
        "# mine\nfont-name=unifont\n{top}version = 1\n[look nordic]\nfont-name = proof\nfont-size = 16\npalette = nord\n[head desk]\n{head}[rules]\ndisplay * -> desk\n"
    )
}

/// The TUI started on `inputs` in a vtium console at 80×25 that answered
/// the start's queries with unifont as its font.
fn started(inputs: Inputs) -> (vtium_config::tui::model::App, Vec<model::Effect>) {
    started_on(inputs, "unifont")
}

/// The TUI started on `inputs` in a vtium console at 80×25 that answered
/// the start's queries with `font` as its font.
fn started_on(inputs: Inputs, font: &str) -> (vtium_config::tui::model::App, Vec<model::Effect>) {
    let (mut app, mut effects) = App::new(inputs, 80, 25);
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Font(font.into()),
        Reply::Attributes,
    ] {
        effects.extend(app.event(Event::Input(Input::Reply(r))));
    }
    (app, effects)
}

/// While the TUI is still finding out what this terminal is, `t` on a
/// look in the Looks tab's list, and `t` in the look's card, say so and
/// send nothing.
#[test]
fn t_in_the_looks_tab_while_the_tui_still_asks_says_it_is_still_finding_out() {
    use vtium_config::tui::model::Live;
    use vtium_config::tui::words;
    let conf = nordic_conf("", "look = nordic\n");
    let (mut app, _) = App::new(with_conf(&conf), 80, 25);
    assert_eq!(app.shared.live, Live::Asking);
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    // on the look in the list, and in its card, which Enter opens
    for key in [None, Some(Key::Enter)] {
        if let Some(key) = key {
            app.event(Event::Input(Input::Key(key)));
        }
        app.shared.message = None;
        let tried = app.event(Event::Input(Input::Key(Key::Char('t'))));
        assert!(
            !tried.iter().any(|e| matches!(e, Effect::Write(_))),
            "{key:?}: {tried:?}"
        );
        assert_eq!(
            app.shared.message.as_deref(),
            Some(words::TRIAL_ASKING),
            "{key:?}"
        );
        assert!(app.nav.trial.is_none(), "{key:?}");
    }
    assert_eq!(app.tab().node(), "looks.look");
}

/// The palette that `t` leaves on the stand-in console.
fn console_palette(c: &Console) -> vtium_term::Palette {
    match &c.answerer {
        Answerer::Vtium(t) => t.palette().clone(),
        Answerer::Da1Only { .. } => panic!("not a vtium console"),
    }
}

/// `t` on a look tries the whole look: its font set through OSC 50, its
/// color scheme through OSC 4, 10 and 11, and the end of the trial puts
/// the font back and the colors back with XTPOPCOLORS. With
/// `allow-window-ops` off, the countdown says that the size waits for the
/// next start.
///
/// References:
/// - ADR-1201 D4, "The live trial, on a key": `t` on a look tries it on
///   the live console, and any other key puts the console back.
#[test]
fn t_on_a_look_tries_its_font_set_and_its_colors() {
    let conf = nordic_conf("", "look = nordic\n");
    let nord = vtium_term::Palette::named("nord").expect("nord");
    let red = nord.color(1);
    let set_red = format!(
        "\x1b]4;0;rgb:{:02x}/{:02x}/{:02x};1;rgb:{:02x}/{:02x}/{:02x}",
        nord.color(0).r,
        nord.color(0).g,
        nord.color(0).b,
        red.r,
        red.g,
        red.b
    );
    // during the countdown the console draws the scheme's colors
    let mut c = console().keys(&[TO_LOOKS, b"t"]);
    drive(&mut c, with_conf(&conf)).expect("the loop");
    assert_eq!(c.switches().first().map(String::as_str), Some("proof"));
    assert!(
        find(&c.written, set_red.as_bytes()).is_some(),
        "no OSC 4 for color 1"
    );
    // the bytes the trial sends are ones vtium's terminal takes
    let mut t = Terminal::new(80, 25, 0);
    t.input(&vtium_config::tui::shot::colors_request(
        &nord,
        &vtium_term::Palette::default(),
    ));
    assert_eq!(t.palette().color(1), red);
    assert_eq!(t.palette().bg(), nord.bg());
    // the countdown says that the size waits for the next start
    let (mut app, _) = started(with_conf(&conf));
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    let tried = app.event(Event::Input(Input::Key(Key::Char('t'))));
    let request = [
        vtium_config::tui::shot::COLORS_SAVE,
        &vtium_config::tui::shot::colors_request(&nord, &vtium_term::Palette::default()),
    ]
    .concat();
    assert!(tried.contains(&Effect::Write(request)), "{tried:?}");
    assert!(
        tried.contains(&Effect::Write(model::switch("proof"))),
        "{tried:?}"
    );
    assert_eq!(
        app.shared.message.as_deref(),
        Some(
            "Trying the look nordic for 10 s. Press k to keep it, or any other key to put the console back. Its size, 16 pt, takes effect when vtium starts again, because the console does not let programs resize its grid."
        )
    );
    // any other key puts the font and the colors back
    let mut c = console().keys(&[TO_LOOKS, &[b"t".as_slice(), b"x"].concat(), b"q"]);
    drive(&mut c, with_conf(&conf)).expect("the loop");
    assert_eq!(c.switches(), ["proof", "unifont"], "{:?}", c.log);
    assert!(find(&c.written, b"\x1b[#Q").is_some());
    assert_eq!(
        console_palette(&c).color(1),
        vtium_term::Palette::named("legacy")
            .expect("legacy")
            .color(1)
    );
}

/// The colors a look's trial sends carry the whole color scheme: the
/// cursor's color and the selection's colors that a color scheme file
/// sets, and a color from 16 to 255 that it sets. A color the scheme
/// leaves unset is unset on the console, which then inverts the cell.
///
/// References:
/// - ADR-1000, the row `color.kitty-protocol`: OSC 21 sets a color by its
///   key, and an empty value unsets a special color.
#[test]
fn a_look_s_colors_carry_the_cursor_the_selection_and_the_256() {
    use vtium_term::{Palette, Rgb, Special};
    let legacy = Palette::default();
    let mut scheme = Palette::named("nord").expect("nord");
    let cursor = Rgb::new(0xf8, 0xf8, 0xf2);
    let selection = Rgb::new(0x44, 0x47, 0x5a);
    let ink = Rgb::new(0xf8, 0xf8, 0xf1);
    let hundred = Rgb::new(0x10, 0x20, 0x30);
    scheme.set_special(Special::Cursor, Some(cursor));
    scheme.set_special(Special::SelectionBg, Some(selection));
    scheme.set_special(Special::SelectionFg, Some(ink));
    scheme.set_color(100, hundred);
    let mut t = Terminal::new(80, 25, 0);
    t.input(&vtium_config::tui::shot::colors_request(&scheme, &legacy));
    assert_eq!(t.palette().special(Special::Cursor), Some(cursor));
    assert_eq!(t.palette().special(Special::SelectionBg), Some(selection));
    assert_eq!(t.palette().special(Special::SelectionFg), Some(ink));
    assert_eq!(t.palette().color(100), hundred);
    assert_eq!(t.palette().color(101), legacy.color(101));
    // back to a scheme that sets none of them
    t.input(&vtium_config::tui::shot::colors_request(&legacy, &scheme));
    assert_eq!(t.palette().special(Special::Cursor), None);
    assert_eq!(t.palette().special(Special::SelectionBg), None);
    assert_eq!(t.palette().color(100), legacy.color(100));
}

/// `t` on a look whose color scheme is a file the batch does not list
/// tries nothing, since the console would draw `legacy` in its place, and
/// offers to activate the file in the draft.
///
/// References:
/// - ADR-1205 D4: a palette file draws only while the batch holds it, and
///   activating it is an act of trust.
#[test]
fn t_on_a_look_whose_color_scheme_is_not_active_offers_to_activate_it() {
    use vtium_config::batch::Change;
    use vtium_config::tui::desk::Op;
    let conf = "# mine\nfont-name=unifont\nversion = 1\n[look nordic]\nfont-name = proof\npalette = spare\n[head desk]\nlook = nordic\n[rules]\ndisplay * -> desk\n";
    let (root, inputs) = schemes_on_disk(conf);
    let spare = root.join("etc/spare.palette");
    let (mut app, _) = started(inputs);
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    let tried = app.event(Event::Input(Input::Key(Key::Char('t'))));
    assert!(
        !tried.iter().any(|e| matches!(e, Effect::Write(_))),
        "{tried:?}"
    );
    assert_eq!(app.nav.trial, None);
    let confirm = app.nav.confirm.clone().expect("the question");
    assert_eq!(
        confirm.question,
        format!(
            "The color scheme spare is not active, so t cannot try the look nordic. Activate it in the draft? y activates {}, and n or Esc leaves it as it is. It takes effect once you install the draft and log in again.",
            spare.display()
        )
    );
    assert_eq!(
        confirm.then,
        model::Then::Desks(vec![Op::Resource {
            path: spare,
            change: Change::Activate,
        }])
    );
    let _ = std::fs::remove_dir_all(root);
}

/// A draft that defines no head and no rule runs vtium's built-in
/// terminal, whose head `console` draws the look `default`, so `t` on that
/// look never says that no terminal uses it.
///
/// References:
/// - ADR-1401 D3: vtium runs the built-in default terminal when the file
///   defines no head and no rule.
#[test]
fn the_built_in_terminal_uses_the_look_default() {
    use vtium_config::tui::tabs::looks::list;
    let conf = "# mine\nfont-name=unifont\nversion = 1\n[look default]\npalette = legacy\n";
    let (mut app, _) = started(with_conf(conf));
    assert_eq!(list::heads(&app.shared, "default"), ["console"]);
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    app.event(Event::Input(Input::Key(Key::Char('t'))));
    let said = app.shared.message.clone().unwrap_or_default();
    assert!(!said.contains("No terminal uses"), "{said}");
}

/// A look whose size is the size the installed vtium.conf already gives
/// the console says nothing of a size that waits for the next start.
#[test]
fn a_look_s_trial_at_the_installed_size_says_nothing_of_the_size() {
    let conf = nordic_conf("font-size = 16\n", "look = nordic\n");
    let (mut app, _) = started(with_conf(&conf));
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    app.event(Event::Input(Input::Key(Key::Char('t'))));
    assert_eq!(
        app.shared.message.as_deref(),
        Some(
            "Trying the look nordic for 10 s. Press k to keep it, or any other key to put the console back."
        )
    );
}

/// A stand-in console in which a program has set color 1 and the
/// background before vtium-config started.
fn console_with_program_colors(keys: &[&[u8]]) -> Console {
    let mut c = console().keys(keys);
    match &mut c.answerer {
        Answerer::Vtium(t) => {
            t.input(b"\x1b]4;1;rgb:12/34/56\x1b\\\x1b]11;rgb:01/02/03\x1b\\");
        }
        Answerer::Da1Only { .. } => panic!("not a vtium console"),
    }
    c
}

/// A look's trial saves the console's colors with XTPUSHCOLORS before it
/// sets the look's, and restores them with XTPOPCOLORS, so colors that a
/// program set before vtium-config started come back as they were: when
/// any other key puts the trial back, and when vtium-config leaves after
/// `k` kept the look.
///
/// References:
/// - ADR-1000, the row `color.stack`: vtium keeps a color stack with
///   XTPUSHCOLORS and XTPOPCOLORS.
#[test]
fn a_look_s_trial_gives_back_the_colors_a_program_set() {
    let conf = nordic_conf("", "look = nordic\n");
    let program_red = vtium_term::Rgb::new(0x12, 0x34, 0x56);
    let program_bg = vtium_term::Rgb::new(0x01, 0x02, 0x03);
    let nord = vtium_term::Palette::named("nord").expect("nord");
    for keys in [&[TO_LOOKS, b"t", b"x", b"q"], &[TO_LOOKS, b"t", b"k", b"q"]] {
        let mut c = console_with_program_colors(keys);
        drive(&mut c, with_conf(&conf)).expect("the loop");
        let saved = find(&c.written, b"\x1b[#P").expect("XTPUSHCOLORS");
        let set = find(
            &c.written,
            &vtium_config::tui::shot::colors_request(&nord, &vtium_term::Palette::default()),
        )
        .expect("the look's colors");
        assert!(
            saved < set,
            "the colors were saved after the trial set its own"
        );
        let palette = console_palette(&c);
        assert_eq!(palette.color(1), program_red, "{keys:?}");
        assert_eq!(palette.bg(), program_bg, "{keys:?}");
    }
}

/// After `k` kept a look's colors, a crash or an I/O error would make the
/// terminal's restore write the bytes that give the console back its
/// colors, as it does for the font and the grid.
#[test]
fn a_crash_after_k_kept_a_look_s_colors_gives_the_colors_back() {
    let conf = nordic_conf("", "look = nordic\n");
    let mut c = console().keys(&[TO_LOOKS, b"t", b"k"]);
    drive(&mut c, with_conf(&conf)).expect("the loop");
    let restore = c.seen_panic.clone().expect("the restore");
    assert!(
        find(&restore, b"\x1b[#Q").is_some(),
        "{:?}",
        String::from_utf8_lossy(&restore)
    );
}

/// After `k` kept a look and Install made that look the console's
/// configured one, quitting leaves the look on the console: its font set,
/// its colors and the grid its size gives. The colors still come off the
/// console's color stack, so the stack ends as it started.
///
/// References:
/// - ADR-1201 D4, "The live trial, on a key": a kept font stays on the
///   console when Install made it the configured one.
#[test]
fn k_then_install_keeps_the_look_s_font_colors_and_grid_after_quitting() {
    let installed = "# mine\nfont-name=unifont\nallow-window-ops\nversion = 1\n[look nordic]\nfont-name = unifont\nfont-size = 12\npalette = legacy\n[head desk]\nlook = nordic\n[rules]\ndisplay * -> desk\n";
    let draft = nordic_conf("allow-window-ops\n", "look = nordic\n");
    let nord = vtium_term::Palette::named("nord").expect("nord");
    let mut d = desk();
    d.installed = installed.to_owned();
    d.draft.clone_from(&draft);
    let mut inputs = with_conf(installed);
    inputs.head = Some("desk".to_owned());
    let mut c = console().keys(&[TO_LOOKS, b"t", b"k", b"i", b"y", b"q"]);
    vtium_config::tui::port::drive_with(&mut c, inputs, Some(&mut d)).expect("the loop");
    assert_eq!(d.plans.len(), 1, "the install ran");
    assert_eq!(c.switches(), ["proof"], "{:?}", c.log);
    let palette = console_palette(&c);
    assert_eq!(palette.bg(), nord.bg());
    assert_eq!(palette.color(1), nord.color(1));
    // the stack is balanced: XTPOPCOLORS took off what XTPUSHCOLORS saved
    assert_eq!(count(&c.written, b"\x1b[#P"), 1);
    assert_eq!(count(&c.written, b"\x1b[#Q"), 1);
    // 16 pt at 96 dpi gives the stand-in's 8×16 cell, and the grid it
    // gives stays: leaving asks for no other grid
    let grids = grid_requests(&c.written);
    assert_eq!(grids.len(), 1, "{grids:?}");
}

/// With `allow-window-ops` on, `t` on a look also tries its size: once
/// the console draws the look's font set, the font helper measures the
/// cell at the look's size and the grid request follows.
#[test]
fn t_on_a_look_tries_its_size_where_the_console_lets_programs_resize_it() {
    let conf = nordic_conf("allow-window-ops\n", "look = nordic\n");
    let mut c = console().keys(&[TO_LOOKS, b"t"]);
    drive(&mut c, with_conf(&conf)).expect("the loop");
    assert_eq!(c.switches().first().map(String::as_str), Some("proof"));
    // 16 pt at the fixture's 96 dpi is 21 pixels high
    assert!(c.cells.contains(&("proof".to_owned(), 21)), "{:?}", c.cells);
    assert!(find(&c.written, b"\x1b[8;").is_some(), "no grid request");
}

/// `k` pressed during a look's trial before the font helper measured the
/// look's size keeps the size with the look: when the cell comes, the grid
/// request goes to the console with no countdown of its own, and leaving
/// gives the start grid back.
#[test]
fn k_before_the_look_s_size_is_measured_keeps_the_size_with_no_second_trial() {
    let conf = nordic_conf("allow-window-ops\n", "look = nordic\n");
    let (mut app, _) = started(with_conf(&conf));
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    app.event(Event::Input(Input::Key(Key::Char('t'))));
    let switched = app.event(Event::Input(Input::Reply(Reply::Font("proof".into()))));
    assert!(
        switched.contains(&Effect::Cell {
            font: "proof".into(),
            height: 21
        }),
        "{switched:?}"
    );
    app.event(Event::Input(Input::Key(Key::Char('k'))));
    let kept = app.shared.message.clone();
    let celled = app.event(Event::Celled {
        font: "proof".into(),
        height: 21,
        cell: Ok((
            Cell {
                width: 10,
                height: 21,
            },
            21,
        )),
    });
    assert!(
        celled
            .iter()
            .any(|e| matches!(e, Effect::Write(b) if b.starts_with(b"\x1b[8;"))),
        "{celled:?}"
    );
    assert!(
        !celled.iter().any(|e| matches!(e, Effect::Wait(_))),
        "{celled:?}"
    );
    assert_eq!(app.nav.trial, None);
    assert_eq!(app.shared.message, kept);
    assert!(app.shared.grid_moved, "leaving gives the start grid back");
}

/// A look's trial that starts after `k` kept another font set gives the
/// console back the kept font set and then the grid it had before the
/// look's size. The grid request waits for the kept font set's switch,
/// not for the font vtium-config started with.
///
/// References:
/// - ADR-1201 D4, "The live trial, on a key": any other key puts the
///   console back as it was before the trial.
#[test]
fn a_look_s_trial_after_a_kept_font_set_gives_back_that_font_set_and_then_the_grid() {
    let conf = nordic_conf("allow-window-ops\n", "look = nordic\n");
    let (mut app, _) = started(with_conf(&conf));
    let key =
        |app: &mut vtium_config::tui::model::App, k: Key| app.event(Event::Input(Input::Key(k)));
    let reply = |app: &mut vtium_config::tui::model::App, font: &str| {
        let mut out = app.event(Event::Input(Input::Reply(Reply::Font(font.into()))));
        out.extend(app.event(Event::Input(Input::Reply(Reply::Attributes))));
        out
    };
    // a font set below the look, tried and kept
    key(&mut app, Key::Char('3'));
    for _ in 0..4 {
        key(&mut app, Key::Down);
    }
    key(&mut app, Key::Char('t'));
    let kept = app.shared.want.clone().expect("a font set tried");
    assert_ne!(kept, "unifont");
    reply(&mut app, &kept);
    key(&mut app, Key::Char('k'));
    // the look, tried at its size
    for _ in 0..4 {
        key(&mut app, Key::Up);
    }
    key(&mut app, Key::Char('t'));
    assert_eq!(
        app.shared.want.as_deref(),
        Some("proof"),
        "{:?}",
        app.shared.message
    );
    reply(&mut app, "proof");
    let celled = app.event(Event::Celled {
        font: "proof".into(),
        height: 21,
        cell: Ok((
            Cell {
                width: 10,
                height: 21,
            },
            21,
        )),
    });
    assert!(
        celled
            .iter()
            .any(|e| matches!(e, Effect::Write(b) if b.starts_with(b"\x1b[8;"))),
        "{celled:?}"
    );
    // any other key puts the look back
    let mut out = key(&mut app, Key::Char('x'));
    out.extend(reply(&mut app, &kept));
    let bytes: Vec<u8> = out
        .iter()
        .filter_map(|e| match e {
            Effect::Write(b) => Some(b.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    let font = find(&bytes, format!("\x1b]50;{kept}").as_bytes()).expect("the kept font set");
    let grid = find(&bytes, b"\x1b[8;25;80t").expect("the grid before the look's size");
    assert!(font < grid, "{:?}", String::from_utf8_lossy(&bytes));
    assert!(
        !app.shared.grid_moved,
        "the console is back at its start grid"
    );
}

/// `t` on a look that no terminal uses, whose font set and color scheme
/// the console already shows, says that no terminal uses it in place of
/// "already uses", and `y` gives it to the draft's terminal. A look's
/// size that waits for the next start is no reason to say the console
/// already shows the look.
///
/// References:
/// - ADR-1403 D4: a terminal draws the look its `look =` line names.
#[test]
fn t_on_a_look_no_terminal_uses_offers_it_to_the_terminal() {
    let conf = "# mine\nfont-name=unifont\nversion = 1\n[look nordic]\nfont-name = proof\nfont-size = 16\npalette = legacy\n[head desk]\n[rules]\ndisplay * -> desk\n";
    let mut d = desk();
    d.installed = conf.to_owned();
    d.draft = conf.to_owned();
    let mut c = Console::vtium(80, 25, true, "proof")
        .takes("proof", (80, 25))
        .takes("unifont", (80, 25))
        .keys(&[TO_LOOKS, b"t", b"y"]);
    vtium_config::tui::port::drive_with(&mut c, with_conf(conf), Some(&mut d)).expect("the loop");
    // nothing to switch: the console draws proof already
    assert_eq!(c.switches(), Vec::<String>::new());
    let head = d.draft.split("[head desk]\n").nth(1).unwrap_or_default();
    assert!(head.starts_with("look = nordic\n"), "{}", d.draft);
    let (mut app, _) = started_on(with_conf(conf), "proof");
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    app.event(Event::Input(Input::Key(Key::Char('t'))));
    assert_eq!(
        app.shared.message.as_deref(),
        Some(
            "No terminal uses the look nordic, and the console already shows its font set and color scheme. Use it for the terminal desk? y sets it in the draft, and n or Esc leaves the draft as it is."
        )
    );
}

/// On a draft that defines no head and no rule, vtium runs its built-in
/// terminal, whose head `console` draws the look `default`. `t` on
/// another look offers it to `console`, and `y` writes out the built-in
/// terminal's head and rules with the look, so the draft goes on running
/// the same terminal.
///
/// References:
/// - ADR-1401 D3: vtium runs the built-in default terminal when the file
///   defines no head and no rule.
#[test]
fn t_on_the_built_in_terminal_offers_the_look_to_its_head_console() {
    let conf = "# mine\nfont-name=unifont\nversion = 1\n[look nordic]\nfont-name = proof\nfont-size = 16\npalette = legacy\n";
    let mut d = desk();
    d.installed = conf.to_owned();
    d.draft = conf.to_owned();
    let mut c = Console::vtium(80, 25, true, "proof")
        .takes("proof", (80, 25))
        .takes("unifont", (80, 25))
        .keys(&[TO_LOOKS, b"t", b"y"]);
    vtium_config::tui::port::drive_with(&mut c, with_conf(conf), Some(&mut d)).expect("the loop");
    let config = vtium_settings::sections(d.draft.as_bytes())
        .expect("sections")
        .parsed
        .config;
    assert_eq!(
        config
            .heads
            .iter()
            .map(|h| (&h.name[..], &h.look[..]))
            .collect::<Vec<_>>(),
        [("console", "nordic")],
        "{}",
        d.draft
    );
    assert!(
        vtium_settings::complete_route(&config, None).complete(),
        "{}",
        d.draft
    );
    let (mut app, _) = started_on(with_conf(conf), "proof");
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    app.event(Event::Input(Input::Key(Key::Char('t'))));
    assert_eq!(
        app.shared.message.as_deref(),
        Some(
            "No terminal uses the look nordic, and the console already shows its font set and color scheme. Use it for the terminal console? y sets it in the draft, and n or Esc leaves the draft as it is."
        )
    );
}

/// The screen as text at `cols` by `rows`, its rows joined by spaces and
/// its runs of spaces cut to one, so a sentence the message line wraps
/// reads whole.
fn screen_words(app: &App, cols: usize, rows: usize) -> String {
    let size = model::Size {
        cols,
        rows,
        cell_px: None,
    };
    let text = vtium_config::tui::view::as_text(&vtium_config::tui::view::screen_at(app, size));
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The countdown of a look whose text font is missing and whose size waits
/// for the next start shows whole at 80×25: it says why vtium's built-in
/// font draws the text, and leaves the size out, since the message line
/// holds three rows.
#[test]
fn a_look_s_countdown_with_a_missing_text_font_shows_whole_at_80x25() {
    let (mut app, _) = started(with_conf(&nordic_conf("", "look = nordic\n")));
    app.event(Event::Sourced(vec![(
        "proof".into(),
        Sources::LeftOut(vec![LeftOut {
            missing: true,
            line: 27,
            spelling: "font:JetBrains Mono".into(),
            why: "no such family".into(),
        }]),
    )]));
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    app.event(Event::Input(Input::Key(Key::Char('t'))));
    let screen = screen_words(&app, 80, 25);
    assert!(
        screen.contains("Trying the look nordic for 10 s. Press k to keep it, or any other key to put the console back. Its text font, the font family JetBrains Mono, is not installed, so vtium's built-in font draws the text until you install it."),
        "{screen}"
    );
    assert!(!screen.contains('…'), "{screen}");
}

/// The question that offers a look no terminal uses to the console's
/// terminal shows whole on the screen, what `y` does included, at 80×25
/// and on the screen below the minimum that a terminal shrunk to 79×24
/// shows. That holds for the question after `t`, where the console already
/// shows the look and a size waits for the next start, and for the
/// question after `k`.
#[test]
fn the_question_that_offers_an_unused_look_shows_whole_at_80x25_and_79x24() {
    let answer = "Use it for the terminal desk? y sets it in the draft, and n or Esc leaves the draft as it is.";
    // t, where the console already draws proof in legacy's colors
    let shown = "# mine\nfont-name=unifont\nversion = 1\n[look nordic]\nfont-name = proof\nfont-size = 16\npalette = legacy\n[head desk]\n[rules]\ndisplay * -> desk\n";
    let (mut app, _) = started_on(with_conf(shown), "proof");
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    app.event(Event::Input(Input::Key(Key::Char('t'))));
    // k, after a trial of nord's colors and proof
    let (mut kept, _) = started(with_conf(&nordic_conf("", "")));
    kept.event(Event::Input(Input::Key(Key::Char('3'))));
    kept.event(Event::Input(Input::Key(Key::Char('t'))));
    kept.event(Event::Input(Input::Reply(Reply::Font("proof".into()))));
    kept.event(Event::Input(Input::Key(Key::Char('k'))));
    for (app, first) in [
        (
            &app,
            "No terminal uses the look nordic, and the console already shows its font set and color scheme.",
        ),
        (
            &kept,
            "No terminal uses the look nordic, so the console shows it only until you quit vtium-config.",
        ),
    ] {
        for (cols, rows) in [(80, 25), (79, 24)] {
            let screen = screen_words(app, cols, rows);
            assert!(
                screen.contains(&format!("{first} {answer}")),
                "{cols}×{rows}: {screen}"
            );
            assert!(!screen.contains('…'), "{cols}×{rows}: {screen}");
        }
    }
}

/// A vtium.conf with the look `nordic`, which no terminal uses, and the
/// terminal desk, which names no look.
const UNUSED_LOOK: &str = "# mine\nfont-name=unifont\nversion = 1\n[look nordic]\nfont-size = 16\n[head desk]\n[rules]\ndisplay * -> desk\n";

/// Choosing a font set on the card of a look that no terminal uses, while
/// the console's terminal names no look, gives the terminal the look, and
/// the message line says so. Without that, the font set saved in the look
/// reaches no console, and quitting puts the console back.
///
/// References:
/// - ADR-1201 D3, "What it writes": when the console's head names no
///   look, the tab sets the head's `look =` line and says on screen that
///   it did.
#[test]
fn a_font_set_chosen_for_a_look_no_terminal_uses_gives_the_console_s_terminal_the_look() {
    // Enter opens the card, Down reaches its font set line, and Right
    // steps from vtium.conf's font set to the first font set
    let keys: &[&[u8]] = &[TO_LOOKS, b"\r", DOWN, b"\x1b[C", ESC, b"s"];
    let draft = draft_after(UNUSED_LOOK, keys);
    let head = draft.split("[head desk]\n").nth(1).unwrap_or_default();
    assert!(head.starts_with("look = nordic\n"), "{draft}");
    let mut d = desk();
    d.installed = UNUSED_LOOK.to_owned();
    d.draft = UNUSED_LOOK.to_owned();
    let mut c = console().keys(&[TO_LOOKS, b"\r", DOWN, b"\x1b[C"]);
    vtium_config::tui::port::drive_with(&mut c, with_conf(UNUSED_LOOK), Some(&mut d))
        .expect("the loop");
    let screen = flat(&c.seen.clone().expect("the screen"));
    assert!(
        screen.contains(
            "The terminal desk named no look, so vtium-config set it to use the look nordic."
        ),
        "{screen}"
    );
}

/// `q` with changes that are not saved says that the draft is not
/// installed yet, so the console goes back to its installed font, and
/// what installs the draft.
#[test]
fn q_says_the_draft_is_not_installed_and_what_installs_it() {
    let mut d = desk();
    d.installed = UNUSED_LOOK.to_owned();
    d.draft = UNUSED_LOOK.to_owned();
    let mut c = console().keys(&[TO_LOOKS, b"\r", DOWN, b"\x1b[C", ESC, b"q"]);
    vtium_config::tui::port::drive_with(&mut c, with_conf(UNUSED_LOOK), Some(&mut d))
        .expect("the loop");
    let screen = flat(&c.seen.clone().expect("the screen"));
    assert!(
        screen.contains("The console then shows what is installed until you install the draft, with i or vtium-config install."),
        "{screen}"
    );
}

/// The card's list of font sets marks a set whose text font this machine
/// lacks, so a set that would draw in vtium's built-in font is not chosen
/// by mistake.
#[test]
fn the_card_marks_a_font_set_whose_text_font_is_missing() {
    let screen = reach_inputs(with_conf(UNUSED_LOOK), &[TO_LOOKS, b"\r", DOWN, b"\r"]);
    let rows: Vec<String> = body(&screen)
        .into_iter()
        .filter(|r| r.contains("proof"))
        .collect();
    assert!(
        rows.iter()
            .any(|r| r.contains("its text font is not on this machine")),
        "{rows:?}\n{screen}"
    );
}

/// Picking the font set a look already has changes nothing in the draft,
/// so the status line counts no change and the message line names no
/// command, and `s` then says there is nothing to save.
///
/// References:
/// - ADR-1403 D7: a change waits for Save, and Save says what it saved.
#[test]
fn picking_the_value_a_look_already_has_counts_no_change() {
    let conf = inputs().conf.expect("the conf");
    let (c, draft) = run_on(&conf, &[TO_LOOKS, b"\r", DOWN, b"\r", b"\r", ESC]);
    let seen = flat(&c.seen.expect("the screen"));
    assert_eq!(draft, conf, "the draft is unchanged");
    assert!(!seen.contains("not saved"), "{seen}");
    assert!(!seen.contains("look set proof"), "{seen}");
    let (c, draft) = run_on(&conf, &[TO_LOOKS, b"\r", DOWN, b"\r", b"\r", ESC, b"s"]);
    let seen = flat(&c.seen.expect("the screen"));
    assert_eq!(draft, conf, "the draft is unchanged");
    assert!(!seen.contains("Saved 1 change"), "{seen}");
    assert!(seen.contains(words::NOTHING_TO_SAVE), "{seen}");
}

/// `w`, where the draft already lets programs resize the grid, asks
/// nothing, because a yes would change nothing. It says when the console
/// lets programs resize its grid, and the draft stays as it is.
#[test]
fn w_when_the_draft_already_allows_window_ops_says_so_and_asks_nothing() {
    let conf = window_ops_on();
    let (c, draft) = run_on(&conf, &[TO_LOOKS, b"w", b"y"]);
    let seen = flat(&c.seen.expect("the screen"));
    assert_eq!(draft, conf, "the draft is unchanged");
    assert!(!seen.contains("not saved"), "{seen}");
    assert!(seen.contains(words::WINDOW_OPS_ON), "{seen}");
    assert!(!seen.contains("Let programs resize the grid?"), "{seen}");
}
