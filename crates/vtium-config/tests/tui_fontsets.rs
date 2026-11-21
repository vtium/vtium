//! The Looks tab's font sets driven headlessly through the TUI's real
//! loop: the live trial of a font set, the console's refusal of one, the
//! removal of a frit (a font set file) and the new-frit form. The Looks
//! tab lists its looks first, the fixture's one look `proof`, then the
//! font sets: broken, minspec-16, proof, tiny5, unifont and unscii-8. So
//! from the tab's first row, Down once reaches broken and Down four times
//! reaches tiny5. `tui.rs` says how the tests drive the TUI, and
//! `tui_common/` holds the helpers.

mod tui_common;
use tui_common::*;

/// The keys from the start that highlight the font set `n` rows below the
/// Looks tab's first look.
fn to_set(n: usize) -> Vec<&'static [u8]> {
    let mut keys = vec![TO_LOOKS];
    keys.extend(std::iter::repeat_n(DOWN, n));
    keys
}

/// The keys of `to_set(n)`, then `more`.
fn on_set(n: usize, more: &[&'static [u8]]) -> Vec<&'static [u8]> {
    let mut keys = to_set(n);
    keys.extend_from_slice(more);
    keys
}

const ENTER: &[u8] = b"\r";
const END: &[u8] = b"\x1b[F";

/// The keys that fill the highlighted role of the new-frit form with the
/// typed `font`: Enter opens the role's list, End reaches Other… at its
/// end, Enter types, and the last Enter keeps the font and moves to the
/// next row.
fn other(font: &'static [u8]) -> [&'static [u8]; 5] {
    [ENTER, END, ENTER, font, ENTER]
}

/// The keys from the text font row of the new-frit form that type `name`
/// and add the font set: Down five times reaches the name from box
/// drawing, Enter types the name and Enter keeps it, and Enter on the add
/// row adds the font set.
fn named(name: &'static [u8]) -> Vec<&'static [u8]> {
    let mut keys = vec![DOWN; 5];
    keys.extend([ENTER, name, ENTER, ENTER]);
    keys
}

/// `t` switches the console to the highlighted font, one switch at a
/// time: a key that puts the font back while the trial's switch is still
/// in flight sends the switch back only once the console has replied.
#[test]
fn t_tries_the_font_live_and_one_switch_is_in_flight_at_a_time() {
    let mut c = console().keys(&on_set(4, &[b"tx", b"q"]));
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
        .keys(&on_set(4, &[b"t", UP, b"t", b"k", b"q"]));
    assert_eq!(run(&mut c), 0);
    assert!(c.log.contains(&"refuse tiny5".to_owned()), "{:?}", c.log);
    assert!(c.log.contains(&"set proof".to_owned()), "{:?}", c.log);
}

#[test]
fn the_refusal_mark_is_on_the_screen() {
    let mut c = Console::vtium(80, 25, true, "unifont")
        .takes("unifont", (80, 25))
        .keys(&on_set(4, &[b"t"]));
    let mut inputs = inputs();
    inputs.conf = None;
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    assert!(
        column(&screen).contains("tiny5: not used: the console kept its font (see its log)"),
        "{screen}"
    );
    assert!(screen.contains("The console refused tiny5"), "{screen}");
}

/// The new-frit form writes a font set from its roles: a text font and a
/// fallback, each typed after Other…, named in the name row, in the order
/// of the roles.
#[test]
fn a_new_frit_is_built_by_role_and_installed_into_the_frits() {
    let mut keys = on_set(1, &[b"n"]);
    keys.extend(other(b"builtin:unscii-8"));
    // from box drawing, four rows down to the fallback
    keys.extend([DOWN; 4]);
    keys.extend(other(b"Some Family"));
    keys.extend([ENTER, b"quiet", ENTER, ENTER, b"i", b"y", b"q"]);
    let mut c = console().keys(&keys);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert_eq!(plan.files.len(), 1);
    assert_eq!(
        plan.files[0].path,
        PathBuf::from("/nonexistent/vtium/fonts/quiet.vtfrit")
    );
    assert_eq!(
        plan.files[0].new,
        "# quiet: made by vtium-config new-frit\nname=quiet\nsource=builtin:unscii-8\nsource=font:Some Family\n"
    );
}

/// Enter on a role lists what can fill it: the layers of its role, then
/// the active fonts whose faces fit it, as the font helper described them.
/// A font that is not active, fails the trust rule or sits in the
/// person's own folder is not offered. The text font's list adds the
/// built-in fonts, and every list ends with Other….
#[test]
fn each_role_lists_its_layers_and_the_active_fonts_that_fit_it() {
    let shown = |rows: usize| {
        let mut keys = on_set(1, &[b"n"]);
        keys.extend(std::iter::repeat_n(DOWN, rows));
        keys.push(ENTER);
        let mut c = console().keys(&keys);
        run(&mut c);
        c.seen.clone().expect("the screen")
    };
    let has = |screen: &str, names: &[&str], not: &[&str]| {
        for name in names {
            assert!(screen.contains(name), "{name} is not offered: {screen}");
        }
        for name in not {
            assert!(!screen.contains(name), "{name} is offered: {screen}");
        }
    };
    let gone = ["Terminus", "Mine", "Noto Color Emoji"];
    let text = shown(0);
    has(
        &text,
        &[
            "DejaVu Sans Mono",
            "Cullet Mono",
            "unifont",
            "unscii-8",
            "Other…",
        ],
        &gone,
    );
    has(&text, &[], &["arabic", "None"]);
    has(
        &shown(2),
        &["None", "arabic", "cjk-jp", "Other…"],
        &[&gone[..], &["nerd-symbols", "DejaVu Sans Mono"]].concat(),
    );
    has(
        &shown(3),
        &["nerd-symbols", "DejaVu Sans Mono", "Cullet Mono"],
        &[&gone[..], &["arabic"]].concat(),
    );
    has(
        &shown(4),
        &["noto-emoji"],
        &[&gone[..], &["Cullet Mono"]].concat(),
    );
    has(
        &shown(5),
        &["unifont", "DejaVu Sans Mono", "Cullet Mono"],
        &gone,
    );
}

/// Picks from the lists go into the font set in the order of the roles:
/// the text font, the scripts in the order they were picked, the symbols,
/// the emoji, the fallback, and box drawing on its own line. Enter on a
/// script already picked takes it out again, and picking it once more puts
/// it after the others.
#[test]
fn picks_from_the_lists_go_into_the_font_set_in_the_order_of_the_roles() {
    let mut keys = on_set(1, &[b"n"]);
    keys.extend([
        // the text font: DejaVu Sans Mono, the list's first
        ENTER, ENTER, // box drawing: the text font's own lines, the third choice
        DOWN, ENTER, DOWN, DOWN, ENTER,
        // the scripts: arabic, then cjk-jp, then arabic out and in again
        DOWN, ENTER, DOWN, ENTER, ENTER, DOWN, ENTER, ENTER, ENTER, ENTER, UP, ENTER,
        // the symbols, the emoji and the fallback: each role's layer
        DOWN, ENTER, DOWN, ENTER, DOWN, ENTER, DOWN, ENTER, DOWN, ENTER, DOWN, ENTER,
        // past the name to the add row
        DOWN, DOWN, ENTER, b"i", b"y", b"q",
    ]);
    let mut c = console().keys(&keys);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0, "{:?}", c.seen);
    let plan = d.plans.first().expect("an install");
    assert_eq!(
        plan.files[0].path,
        PathBuf::from("/nonexistent/vtium/fonts/dejavu-sans-mono.vtfrit")
    );
    assert_eq!(
        plan.files[0].new,
        "# dejavu-sans-mono: made by vtium-config new-frit\n\
         name=dejavu-sans-mono\n\
         source=outline:/usr/share/fonts/TTF/DejaVuSansMono.ttf\n\
         layer=cjk-jp\n\
         layer=arabic\n\
         layer=nerd-symbols\n\
         layer=noto-emoji\n\
         layer=unifont\n\
         glyph-source-box=font\n"
    );
}

/// A new frit named as an installed one replaces it, the save showing the
/// installed text it replaces, as the TUI gathered it at start; one whose
/// installed file is not UTF-8 text is not replaced, and says why.
#[test]
fn a_new_frit_replaces_the_installed_one_of_its_name() {
    let to_new = on_set(1, &[b"n"]);
    let mut keys: Vec<&[u8]> = to_new.clone();
    keys.extend(other(b"builtin:unifont"));
    keys.extend(named(b"proof"));
    keys.extend([b"i".as_slice(), b"y", b"q"]);
    let mut c = console().keys(&keys);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let plan = d.plans.first().expect("an install");
    assert_eq!(
        plan.files[0].path,
        PathBuf::from("/nonexistent/vtium/fonts/proof.vtfrit")
    );
    assert_eq!(
        plan.files[0].old.as_deref(),
        Some("source=font:JetBrains Mono\n")
    );
    let mut keys: Vec<&[u8]> = to_new;
    keys.extend(other(b"builtin:unifont"));
    keys.extend(named(b"latin1"));
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
    let mut c = console().keys(&on_set(4, &[b"r", b"i"]));
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("and 1 font set"), "{screen}");
    let mut c = console().keys(&on_set(4, &[b"r", b"i", b"y", b"q"]));
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert!(plan.files.is_empty(), "{plan:?}");
    assert_eq!(
        plan.removals,
        [vtium_config::draft::Removal {
            path: PathBuf::from("/nonexistent/vtium/fonts/tiny5.vtfrit"),
            old: b"source=bdf:/x/Tiny5Mono.bdf\n".to_vec(),
        }]
    );
    // marked in the list, and kept by a second r
    let mut c = console().keys(&on_set(4, &[b"r"]));
    run_desk(&mut c, &mut desk());
    let screen = c.seen.clone().expect("the screen");
    assert!(
        column(&screen).contains("tiny5: no font file on this machine, to remove."),
        "{screen}"
    );
    let mut c = console().keys(&on_set(4, &[b"r", b"r", b"i", b"y", b"q"]));
    let mut d = desk();
    run_desk(&mut c, &mut d);
    assert!(d.plans.is_empty());
}

/// The remove key does what `remove-frit` does. It keeps the removal of
/// a font set that the draft's vtium.conf names, in a look or at its top
/// level, in the draft's folder, so the check can list each line that
/// names it. It refuses a built-in font, saying why, and keeps nothing.
///
/// References:
/// - ADR-1403 D8, "Every font set change is kept at once": a removal is written even when the draft's looks name the frit.
#[test]
fn r_keeps_a_removal_the_draft_names_and_refuses_a_built_in_font() {
    let conf = inputs().conf.expect("the fixture");
    let names_minspec = conf.replace("font-name=unifont", "font-name=minspec-16");
    for (keys, draft, name) in [
        // proof is the frit look proof draws
        (on_set(3, &[b"r"]), conf.clone(), "proof"),
        // the draft names minspec-16 as vtium.conf's font-name
        (on_set(2, &[b"r"]), names_minspec, "minspec-16"),
    ] {
        let mut c = console().keys(&keys);
        let mut d = desk();
        d.draft = draft;
        run_desk(&mut c, &mut d);
        assert!(d.frits.removed.iter().eq([name]), "{name}: {:?}", d.frits);
        let screen = c.seen.clone().expect("the screen");
        let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(!text.contains("cannot be removed:"), "{name}: {screen}");
    }
    let mut c = console().keys(&on_set(5, &[b"r"]));
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(text.contains("cannot be removed:"), "{screen}");
    assert!(text.contains("built-in font"), "{screen}");
    assert!(d.frits.is_empty(), "{:?}", d.frits);
}

#[test]
fn a_new_frit_the_reader_refuses_stays_in_its_view_and_says_why() {
    // the add row with no text font
    let mut c = console().keys(&on_set(1, &[b"n", END, ENTER]));
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("This font set cannot be read: A font set needs a text font."),
        "{screen}"
    );
    assert!(screen.contains("Text font"), "{screen}");
    // a text font the frit reader refuses, typed after Other…
    let mut keys = on_set(1, &[b"n"]);
    keys.extend(other(b"kbd:"));
    keys.extend([END, ENTER]);
    let mut c = console().keys(&keys);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("This font set cannot be read"), "{screen}");
    assert!(screen.contains("Text font"), "{screen}");
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

/// Leaving while a frit's removal waits in the draft's folder for an
/// install asks nothing, since the folder keeps the removal: `q` quits at
/// once, installs nothing, and the folder still holds the removal. The top
/// row counts the font set waiting.
#[test]
fn leaving_with_a_frit_waiting_quits_at_once_and_the_folder_keeps_it() {
    let mut c = console().keys(&on_set(4, &[b"r"]));
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    // the status line says the font set waiting; the next test checks
    // its words at each width
    let status = screen.lines().nth(1).unwrap_or_default();
    assert!(status.contains("· 1 to install"), "{screen}");
    let mut c = console().keys(&on_set(4, &[b"r", b"q"]));
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    // the TUI left before the keys ran out, so no question kept it open
    assert_eq!(c.seen, None);
    assert!(d.plans.is_empty(), "{:?}", d.plans);
    assert!(d.frits.removed.iter().eq(["tiny5"]), "{:?}", d.frits);
}

/// `i` installs the frit's removal that waits in the draft's folder, after
/// its question, and `q` then quits.
#[test]
fn i_installs_a_frit_waiting_and_q_then_quits() {
    let mut c = console().keys(&on_set(4, &[b"r", b"i", b"y", b"q"]));
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert_eq!(
        plan.removals,
        [vtium_config::draft::Removal {
            path: PathBuf::from("/nonexistent/vtium/fonts/tiny5.vtfrit"),
            old: b"source=bdf:/x/Tiny5Mono.bdf\n".to_vec(),
        }]
    );
}

#[test]
fn tab_does_nothing_in_the_new_frit_view_and_esc_goes_back_to_the_looks_tab() {
    // the name row, typed
    let typed: [&[u8]; 5] = [DOWN, b"n", END, UP, ENTER];
    for key in [TAB, BACKTAB] {
        let keys = [&typed[..], &[b"abc", key]].concat();
        let (screen, _) = reach_from(model::View::Looks, 80, 25, &keys);
        let typing = keys_of("looks.new-frit.typing");
        assert_eq!(foot(&screen, typing), typing, "{key:?}: {screen}");
        assert!(screen.contains("abc_"), "{screen}");
    }
    // Esc drops the name typed and goes back to the form's rows, and Esc
    // there goes back to the Looks tab, which the form belongs to
    let line = key_line(
        model::View::Looks,
        &[&typed[..], &[b"abc", TAB, b"\x1b"]].concat(),
    );
    let rows = keys_of("looks.new-frit");
    assert_eq!(foot(&line, rows), rows);
    let line = key_line(
        model::View::Looks,
        &[&typed[..], &[b"abc", TAB, b"\x1b", b"\x1b"]].concat(),
    );
    assert_eq!(foot(&line, keys_of("looks")), keys_of("looks"));
}

/// The sources query does not hold the keys up: while the helper has not
/// answered, a key switches the console and a signal leaves (ADR-1201 D3).
#[test]
fn keys_reach_the_tui_while_the_helper_finds_the_sources() {
    let mut c = console().keys(&on_set(4, &[b"t", b"k"]));
    c.hold_sources = true;
    // the script runs out with the answer still to come, and SIGTERM leaves
    assert_eq!(run(&mut c), 128 + 15);
    assert_eq!(
        c.sources_asked,
        [["minspec-16", "proof", "tiny5"].map(String::from).to_vec()]
    );
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    // with no answer yet, the font set's note names no missing file
    let screen = c.seen.clone().expect("the screen");
    assert!(column(&screen).contains("tiny5: reads."), "{screen}");
}

/// Each frit that reads is noted with the sources the helper says this
/// machine lacks, or that none of its sources is here.
#[test]
fn the_font_list_names_the_sources_a_frit_lacks() {
    for (n, says) in [
        (3, "proof: reads, 1 font file missing."),
        (4, "tiny5: no font file on this machine."),
        (2, "minspec-16: reads."),
    ] {
        let mut c = console().keys(&to_set(n));
        run(&mut c);
        let screen = c.seen.clone().expect("the screen");
        assert!(column(&screen).contains(says), "{says}: {screen}");
    }
}

#[test]
fn a_refused_frit_s_long_reason_wraps_by_words() {
    let reason = "line 2: unknown key colour, which a frit spells color, as vtium.conf spells it in every key";
    let mut inputs = inputs();
    inputs.fonts[0] = font("broken", FontState::Refused(reason.into()));
    let mut c = console().keys(&to_set(1));
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    assert!(
        column(&screen).contains(&format!("cannot be read: {reason}")),
        "{screen}"
    );
    assert!(screen.lines().all(|l| l.chars().count() <= 80));
}

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

/// A font set made with the form joins the Looks tab's font sets once the
/// install puts it in place, without a restart of vtium-config.
#[test]
fn a_font_set_installed_joins_the_font_sets_at_once() {
    use vtium_config::tui::desk::{Done, Opened};
    use vtium_config::tui::model::{App, Event};
    let (mut app, _) = App::new(inputs(), 100, 30);
    app.shared.draft = Some(Opened {
        frits: vtium_config::store::FritChanges {
            kept: [("fresh".to_owned(), "source=builtin:unifont\n".to_owned())].into(),
            ..Default::default()
        },
        ..Opened::default()
    });
    app.event(Event::Installed(Ok(Done::Draft(Opened::default(), None))));
    let fresh = app
        .shared
        .inputs
        .fonts
        .iter()
        .find(|f| f.name == "fresh")
        .expect("the new font set among the font sets");
    assert_eq!(fresh.state, vtium_config::tui::model::FontState::Reads);
}

/// A font set that only the draft adds keeps the highlight through the
/// install that puts it in place: the Looks tab lists it after the
/// installed font sets until then, and the install moves it among them.
#[test]
fn the_highlight_stays_on_a_font_set_the_install_puts_in_place() {
    use vtium_config::tui::desk::{Done, Opened};
    use vtium_config::tui::model::{App, Event};
    use vtium_config::tui::tabs::looks::{Entry, fontsets};
    let (mut app, _) = App::new(inputs(), 100, 30);
    app.shared.draft = Some(Opened {
        frits: vtium_config::store::FritChanges {
            // a name that sorts among the installed font sets
            kept: [("pale".to_owned(), "source=builtin:unifont\n".to_owned())].into(),
            ..Default::default()
        },
        ..Opened::default()
    });
    app.looks.show_font_set(&app.shared, "pale");
    let name = |app: &App| match app.looks.highlighted(&app.shared) {
        Some(Entry::FontSet(at)) => fontsets::font(&app.shared, at).map(|f| f.name),
        other => panic!("{other:?}"),
    };
    assert_eq!(name(&app).as_deref(), Some("pale"));
    app.event(Event::Installed(Ok(Done::Draft(Opened::default(), None))));
    assert_eq!(name(&app).as_deref(), Some("pale"));
}
