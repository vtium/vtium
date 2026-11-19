//! The Fonts tab driven headlessly through the TUI's real loop: the list
//! of fonts and its marks, the live trial of a font, the specimen, the
//! removal of a frit and the new-frit form. `tui.rs` says how the tests
//! drive the TUI, and `tui_common/` holds the helpers.

mod tui_common;
use tui_common::*;

/// Moving the highlight sends the live console nothing (ADR-1201 D4): no
/// switch goes, and leaving has no font to give back.
#[test]
fn each_highlighted_font_leaves_the_live_console_alone() {
    let mut c = console().keys(&[UP, &[DOWN, DOWN].concat(), DOWN, b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.switches(), Vec::<String>::new(), "{:?}", c.log);
    assert!(!c.log.iter().any(|l| l.starts_with("set ")), "{:?}", c.log);
    // and the console draws the preview's characters itself, so no font
    // highlighted is drawn as a picture, though the console draws them
    assert!(c.specimens.is_empty(), "{:?}", c.specimens);
    assert!(c.shots.is_empty() && c.summaries.is_empty());
    assert_eq!(count(&c.written, b"\x1b_Ga=T"), 0);
}

/// `t` switches the console to the highlighted font, one switch at a
/// time: a key that puts the font back while the trial's switch is still
/// in flight sends the switch back only once the console has replied.
#[test]
fn t_tries_the_font_live_and_one_switch_is_in_flight_at_a_time() {
    let mut c = console().keys(&[UP, &[b"t".as_slice(), b"x"].concat(), b"q"]);
    assert_eq!(run(&mut c), 0);
    let mut outstanding = false;
    for entry in &c.log {
        if entry.starts_with("set ") || entry.starts_with("refuse ") {
            assert!(!outstanding, "a second switch before a reply: {:?}", c.log);
            outstanding = true;
        } else if entry.starts_with("reply ") {
            outstanding = false;
        }
    }
    assert_eq!(c.switches(), ["tiny5", "unifont"], "{:?}", c.log);
}

#[test]
fn a_font_the_console_refuses_is_marked_and_the_tui_goes_on() {
    let mut c = Console::vtium(80, 25, true, "unifont")
        .takes("unifont", (80, 25))
        .takes("proof", (80, 25))
        .keys(&[UP, b"t", UP, b"t", b"k", b"q"]);
    assert_eq!(run(&mut c), 0);
    assert!(c.log.contains(&"refuse tiny5".to_owned()), "{:?}", c.log);
    assert!(c.log.contains(&"set proof".to_owned()), "{:?}", c.log);
}

#[test]
fn the_refusal_mark_is_on_the_screen() {
    let mut c = Console::vtium(80, 25, true, "unifont")
        .takes("unifont", (80, 25))
        .keys(&[UP, b"t"]);
    let mut inputs = inputs();
    inputs.conf = None;
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("tiny5")
            && screen.contains("not used: the console kept its font (see its log)"),
        "{screen}"
    );
    assert!(screen.contains("The console refused tiny5"), "{screen}");
}

#[test]
fn a_new_frit_is_typed_in_its_view_and_installed_into_the_frits() {
    let mut c = console().keys(&[
        b"n",
        b"quiet",
        b"\r",
        b"builtin:unscii-8",
        b"\r",
        b"Some Family",
        b"\r",
        b"\r",
        b"\r",
        b"\r",
        b"\r",
        b"i",
        b"y",
        b"q",
    ]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert_eq!(plan.files.len(), 1);
    assert_eq!(
        plan.files[0].path,
        PathBuf::from("/etc/vtium/fonts/quiet.vtfrit")
    );
    assert_eq!(
        plan.files[0].new,
        "# quiet: made by vtium-config new-frit\nname=quiet\nsource=builtin:unscii-8\nsource=font:Some Family\n"
    );
}

/// A new frit named as an installed one replaces it, the save showing the
/// installed text it replaces, as the TUI gathered it at start; one whose
/// installed file is not UTF-8 text is not replaced, and says why.
#[test]
fn a_new_frit_replaces_the_installed_one_of_its_name() {
    let to_new = [b"n".as_slice()];
    let mut keys: Vec<&[u8]> = to_new.to_vec();
    keys.extend([b"proof".as_slice(), b"\r", b"builtin:unifont"]);
    // five fields to the add row, the add, and the install
    keys.extend([b"\r".as_slice(); 6]);
    keys.extend([b"i".as_slice(), b"y", b"q"]);
    let mut c = console().keys(&keys);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let plan = d.plans.first().expect("an install");
    assert_eq!(
        plan.files[0].path,
        PathBuf::from("/etc/vtium/fonts/proof.vtfrit")
    );
    assert_eq!(
        plan.files[0].old.as_deref(),
        Some("source=font:JetBrains Mono\n")
    );
    let mut keys: Vec<&[u8]> = to_new.to_vec();
    keys.extend([b"latin1".as_slice(), b"\r", b"builtin:unifont"]);
    keys.extend([b"\r".as_slice(); 6]);
    let mut c = console().keys(&keys);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("A new font set cannot replace latin1.vtfrit, because the installed file is not UTF-8 text."),
        "{screen}"
    );
    assert!(d.plans.is_empty());
}

/// `r` in the fonts view removes the highlighted frit when the user
/// saves: the font list marks it, the save view lists its file, and the
/// save's plan removes it with the bytes the TUI read at start, as
/// `remove-frit`'s draft does. A second `r` keeps it.
/// `r` in the fonts view removes the highlighted frit at the next
/// install: the font list marks it, and the install's plan removes it with
/// the bytes the TUI read at start, as `remove-frit`'s draft does. A
/// second `r` keeps it.
#[test]
fn r_removes_the_highlighted_frit_at_the_next_install() {
    // unifont is highlighted at start; Up reaches tiny5
    let mut c = console().keys(&[UP, b"r", b"i"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("and 1 font set"), "{screen}");
    let mut c = console().keys(&[UP, b"r", b"i", b"y", b"q"]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert!(plan.files.is_empty(), "{plan:?}");
    assert_eq!(
        plan.removals,
        [vtium_config::draft::Removal {
            path: PathBuf::from("/etc/vtium/fonts/tiny5.vtfrit"),
            old: b"source=bdf:/x/Tiny5Mono.bdf\n".to_vec(),
        }]
    );
    // marked in the list, and kept by a second r
    let mut c = console().keys(&[UP, b"r"]);
    run_desk(&mut c, &mut desk());
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("to remove"), "{screen}");
    let mut c = console().keys(&[UP, b"r", b"r", b"i", b"y", b"q"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    assert!(d.plans.is_empty());
}

/// The remove key refuses what `remove-frit` refuses, saying why, and
/// adds nothing to the save: a built-in font, the frit vtium.conf's
/// `font-name` names, as it is or as the save leaves it, and the frit a
/// look's `font-name` names.
#[test]
fn r_refuses_what_remove_frit_refuses() {
    for (keys, says) in [
        // unifont, highlighted at start
        (vec![b"r".as_slice()], "built-in font"),
        // proof is the frit look proof draws
        (vec![UP, UP, b"r"], "`[look proof]`"),
        // minspec-16 kept as font-name, then r
        (vec![UP, UP, UP, b"\r", b"r"], "font-name"),
    ] {
        let mut all = keys.clone();
        all.extend([b"i".as_slice(), b"y", b"q"]);
        let mut c = console().keys(&keys);
        run_desk(&mut c, &mut desk());
        let screen = c.seen.clone().expect("the screen");
        let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(text.contains("cannot be removed:"), "{says}: {screen}");
        assert!(text.contains(says), "{says}: {screen}");
        let mut c = console().keys(&all);
        let mut d = desk();
        run_desk(&mut c, &mut d);
        assert!(
            d.plans.iter().all(|p| p.removals.is_empty()),
            "{says}: {:?}",
            d.plans
        );
    }
    // a frit marked for removal and then kept as font-name is not removed:
    // the save says why and saves nothing
    let mut c = console().keys(&[UP, b"r", b"\r", b"i", b"y", b"q"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    assert!(d.plans.is_empty(), "{:?}", d.plans);
}

/// A save that changes font-frits and also writes or removes a frit is
/// refused: the frits would go to the directory font-frits named before
/// the save, so the user saves the new directory first. A save that
/// changes only one of the two is planned.
/// An install that moves font-frits and also writes or removes a frit is
/// refused: the frits would go to the directory font-frits named before,
/// so the user installs the new directory first. An install that changes
/// only one of the two is planned.
#[test]
fn an_install_that_moves_the_frits_and_changes_a_frit_is_refused() {
    use vtium_config::tui::desk::Opened;
    use vtium_config::tui::model::App;
    let conf = inputs().conf.expect("the fixture");
    let drafted = |text: &str| Opened {
        name: "mine".into(),
        text: text.to_owned(),
        ..Opened::default()
    };
    let moved = conf.replace(
        "font-name=unifont\n",
        "font-name=unifont\nfont-frits=/srv/frits\n",
    );
    let (mut app, _) = App::new(inputs(), 80, 25);
    app.shared.draft = Some(drafted(&moved));
    assert!(app.frit_plan().is_ok());
    app.shared.pending.removals.push("tiny5".to_owned());
    let why = app.frit_plan().expect_err("a removal and a new directory");
    assert!(why.contains("font-frits"), "{why}");
    app.shared.pending.removals.clear();
    app.shared
        .pending
        .frits
        .push(("made".to_owned(), "source=builtin:unifont\n".to_owned()));
    let why = app.frit_plan().expect_err("a new frit and a new directory");
    assert!(why.contains("font-frits"), "{why}");
    // a draft that leaves font-frits alone is no move
    let (mut app, _) = App::new(inputs(), 80, 25);
    app.shared.draft = Some(drafted(
        &conf.replace("font-name=unifont", "font-name=proof"),
    ));
    app.shared.pending.removals.push("tiny5".to_owned());
    assert!(app.frit_plan().is_ok());
    // nor is one that names the directory the installed file names
    let mut named = inputs();
    named.conf = Some(moved.clone());
    let (mut app, _) = App::new(named, 80, 25);
    app.shared.draft = Some(drafted(&moved));
    app.shared.pending.removals.push("tiny5".to_owned());
    assert!(app.frit_plan().is_ok(), "{:?}", app.frit_plan());
}

#[test]
fn a_new_frit_the_reader_refuses_stays_in_its_view_and_says_why() {
    let mut c = console().keys(&[b"n", b"\r", b"\r", b"\r", b"\r", b"\r", b"\r", b"\r"]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("This font set cannot be read"), "{screen}");
    assert!(screen.contains("Font 1"), "{screen}");
    assert!(c.saves.is_empty());
}

#[test]
fn new_opens_the_tui_at_its_new_frit_view() {
    let mut inputs = inputs();
    inputs.view = model::View::New;
    let mut c = console();
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Add this font set to the install"),
        "{screen}"
    );
}

/// Leaving while a frit's removal waits for an install asks first and
/// names it; `y` installs the removal and leaves, `n` leaves without
/// installing, a stray key such as `l` leaves the question open, and the
/// top row counts the font set waiting.
#[test]
fn leaving_with_a_frit_waiting_asks_and_y_installs_it() {
    let mut c = console().keys(&[UP, b"r", b"q"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains(
            "Install before you quit? Quitting without installing loses the removal of tiny5."
        ),
        "{screen}"
    );
    // the status line says the font set waiting; the next test checks
    // its words at each width
    let status = screen.lines().nth(1).unwrap_or_default();
    assert!(status.contains("· 1 to install"), "{screen}");
    let mut c = console().keys(&[UP, b"r", b"q", b"y"]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert_eq!(plan.removals.len(), 1, "{plan:?}");
    let mut c = console().keys(&[UP, b"r", b"q", b"n"]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert!(d.plans.is_empty());
    let mut c = console().keys(&[UP, b"r", b"q", b"l"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(text.contains("Install before you quit?"), "{screen}");
}

/// Esc goes back with the frit still waiting, after a stray `x` and `i`
/// left the question open, and `i` in the view then installs it.
#[test]
fn leaving_with_a_frit_waiting_goes_back_or_installs() {
    let mut c = console().keys(&[UP, b"r", b"q", b"x", b"i", b"\x1b", b"i", b"y", b"q"]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert_eq!(
        plan.removals,
        [vtium_config::draft::Removal {
            path: PathBuf::from("/etc/vtium/fonts/tiny5.vtfrit"),
            old: b"source=bdf:/x/Tiny5Mono.bdf\n".to_vec(),
        }]
    );
}

/// The Fonts view at 80x25 in a vtium console with `allow-font-ops` off,
/// where the TUI cannot switch the font: the written description, as in
/// any terminal that shows no images, and the notice that the live trial
/// is off (ADR-1201 D4).
#[test]
fn the_fonts_view_without_font_operations_at_80x25() {
    let (screen, bytes) = reach_on(false, model::View::Fonts, 80, 25, &[]);
    assert!(screen.contains("does not support images"), "{screen}");
    assert!(screen.contains(words::KEYS_FONTS), "{screen}");
    golden("fonts-nolive-80x25", &screen, &bytes);
}

#[test]
fn tab_does_nothing_in_the_new_frit_view_and_esc_goes_back_to_the_looks_tab() {
    for key in [TAB, BACKTAB] {
        let (screen, _) = reach_from(model::View::Fonts, 80, 25, &[b"n", b"abc", key]);
        assert_eq!(
            foot(&screen, words::KEYS_NEW),
            words::KEYS_NEW,
            "{key:?}: {screen}"
        );
        assert!(screen.contains("abc"), "{screen}");
    }
    // the form belongs to the Looks tab, so Esc goes back there
    let line = key_line(model::View::Fonts, &[b"n", b"abc", TAB, b"\x1b"]);
    assert_eq!(foot(&line, keys_of("looks")), keys_of("looks"));
}

/// `s` in the Fonts view switches the picture area to the detailed
/// specimen of the highlighted font, at the draft's size and in the same
/// room, and `s` again back to the picture of the console; each highlight
/// asks for the specimen of the font it reaches (ADR-1201 D4). The Fonts
/// view starts on the specimen.
#[test]
fn s_switches_the_fonts_view_between_the_console_and_the_detailed_specimen() {
    let mut c = outside().keys(&[b"s", UP, b"s", b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    // below the list and the line beside the specimen: rows 14 to 20 at
    // 80x25
    let room = (79 * 8, 7 * 16);
    assert_eq!(
        c.specimens,
        [
            ("unifont".to_owned(), 16, room),
            ("tiny5".to_owned(), 16, room)
        ]
    );
    // the console's picture after the first s, until the second
    let fonts: Vec<&str> = c.pictures.iter().map(|(f, _, _)| f.as_str()).collect();
    assert_eq!(fonts, ["unifont", "tiny5"]);
    // the line beside the specimen gives the cell, the grid on the display
    // and how many characters the font draws itself, and its marks
    let mut c = outside().keys(&[]);
    c.graphics = true;
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("unifont at 12 pt: each character cell is 8x16 pixels, and the console will be 80x30 characters on this 640x480 display. The font can display 300 of the 323 characters in the preview."),
        "{screen}"
    );
    assert!(
        text.contains("a box marks a character no font can display"),
        "{screen}"
    );
    assert!(screen.contains("s preview"), "{screen}");
}

/// The font view shows each highlighted font as its detailed specimen, at
/// 12 points where nothing names a size, below the list and the line that
/// describes it, where the console cannot be switched live.
#[test]
fn each_highlighted_font_is_shown_as_its_specimen() {
    let mut c = Console::vtium(80, 25, false, "unifont").keys(&[UP, UP, UP, UP, b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    let fonts: Vec<(&str, u32)> = c
        .specimens
        .iter()
        .map(|(f, h, _)| (f.as_str(), *h))
        .collect();
    assert_eq!(
        fonts,
        [
            ("unifont", 16),
            ("tiny5", 16),
            ("proof", 16),
            ("minspec-16", 16)
        ]
    );
    // below the six fonts' rows and the line's rows, to the room above the
    // footer
    let (_, _, (w, h)) = c.specimens[0];
    assert_eq!((w, h), (79 * 8, 7 * 16));
    // the broken frit, which does not read, gets none: each picture is
    // deleted as the next font is highlighted
    assert_eq!(count(&c.written, &kitty::delete(kitty::IMAGE_ID)), 4);
    let mut live = console().keys(&[UP, b"q"]);
    live.graphics = true;
    run(&mut live);
    // a vtium console it can switch draws the preview as text instead
    assert!(live.specimens.is_empty(), "{:?}", live.specimens);
    assert_eq!(live.switches(), Vec::<String>::new());
}

/// The sources query does not hold the keys up: while the helper has not
/// answered, a key switches the console and a signal leaves (ADR-1201 D3).
#[test]
fn keys_reach_the_tui_while_the_helper_finds_the_sources() {
    let mut c = console().keys(&[UP, b"t", b"k"]);
    c.hold_sources = true;
    // the script runs out with the answer still to come, and SIGTERM leaves
    assert_eq!(run(&mut c), 128 + 15);
    assert_eq!(
        c.sources_asked,
        [["minspec-16", "proof", "tiny5"].map(String::from).to_vec()]
    );
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("proof      reads\n"), "{screen}");
}

/// Each frit that reads is noted with the sources the helper says this
/// machine lacks, or that none of its sources is here.
#[test]
fn the_font_list_names_the_sources_a_frit_lacks() {
    let mut c = console().keys(&[]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("proof      reads, 1 font file missing"),
        "{screen}"
    );
    assert!(
        screen.contains("tiny5      no font file on this machine"),
        "{screen}"
    );
    assert!(screen.contains("minspec-16 reads\n"), "{screen}");
}

/// In a vtium console the TUI can switch, the Fonts view prints the
/// detailed specimen's characters, its lines and the locale's line, as
/// the console's own text, in the font the console draws now, and asks
/// the helper for no picture; after `t` the console draws them in the font
/// tried, and the caption names it (ADR-1201 D4).
#[test]
fn a_vtium_console_prints_the_specimen_as_its_own_text_and_t_redraws_it() {
    let grid = (100, 50);
    let mut c = Console::vtium(grid.0, grid.1, true, "unifont")
        .takes("tiny5", grid)
        .takes("unifont", grid)
        .keys(&[UP, b"t", b"k"]);
    let mut i = inputs();
    i.locale = "ru_RU.UTF-8".into();
    drive(&mut c, i).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    // every line the picture of the specimen draws, in its order, with the
    // Russian locale's own line after the samples, then its line of
    // superscripts and subscripts
    let mut want: Vec<String> = vtium_config::specimen::detail_lines("ru_RU.UTF-8")
        .iter()
        .map(|l| l.trim().to_owned())
        .collect();
    assert!(
        want.iter().any(|l| l.starts_with("Съешь же ещё")),
        "{want:?}"
    );
    let script: String = vtium_config::specimen::SCRIPT_LINE
        .iter()
        .map(|(run, _)| *run)
        .collect();
    want.push(script.trim().to_owned());
    // the console's grid gives a wide character two cells, which the
    // screen's text shows as the character and an empty cell
    let shown: Vec<String> = preview_rows(&screen, "!\"#$%")
        .into_iter()
        .take(want.len())
        .map(|l| l.replace(' ', ""))
        .collect();
    let want: Vec<String> = want.iter().map(|l| l.replace(' ', "")).collect();
    assert_eq!(shown, want, "{screen}");
    // the superscripts and subscripts are SGR 73 and 74
    let text = String::from_utf8_lossy(&c.written).into_owned();
    assert!(text.contains("\x1b[73m2"), "a superscript");
    assert!(text.contains("\x1b[74m2"), "a subscript");
    // no picture, and no written description in its place
    assert!(c.specimens.is_empty() && c.summaries.is_empty() && c.cells.is_empty());
    assert!(!screen.contains("does not support images"), "{screen}");
    // the console draws tiny5 now, and the caption says so; leaving gives
    // unifont back
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    let flat = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains(&words::console_font_caption(Some("tiny5"))),
        "{screen}"
    );
    assert!(screen.contains(words::KEYS_FONTS_LIVE), "{screen}");
}

/// The Fonts view's key line fits on one row at 80 columns, the key that
/// leaves included.
#[test]
fn the_fonts_key_line_is_one_row_at_80_columns() {
    let screen = reach(80, 25, &[]);
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows.len(), 25);
    assert!(rows[24].starts_with(" Enter choose"), "{screen}");
    assert!(rows[24].ends_with("q quit"), "{screen}");
    assert!(rows[23].starts_with("---"), "the rule just above: {screen}");
}

#[test]
fn a_refused_frit_s_long_reason_wraps_by_words() {
    let reason = "line 2: unknown key colour, which a frit spells color, as vtium.conf spells it in every key";
    let mut inputs = inputs();
    inputs.fonts[0] = font("broken", FontState::Refused(reason.into()));
    let mut c = console();
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    let joined = screen.lines().map(str::trim).collect::<Vec<_>>().join(" ");
    assert!(
        joined.contains(&format!("cannot be read: {reason}")),
        "{screen}"
    );
    assert!(screen.lines().all(|l| l.chars().count() <= 80));
    // the reason goes on below its name's row, under its column
    let continued: Vec<&str> = screen
        .lines()
        .filter(|l| l.starts_with("              ") && !l.trim().is_empty())
        .collect();
    assert_eq!(continued.len(), 1, "{screen}");
}
