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

#[test]
fn a_new_frit_is_typed_in_its_view_and_installed_into_the_frits() {
    let mut c = console().keys(&on_set(
        1,
        &[
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
        ],
    ));
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
    let to_new = on_set(1, &[b"n"]);
    let mut keys: Vec<&[u8]> = to_new.clone();
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
    let mut keys: Vec<&[u8]> = to_new;
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
            path: PathBuf::from("/etc/vtium/fonts/tiny5.vtfrit"),
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

/// The remove key refuses what `remove-frit` refuses, saying why, and
/// adds nothing to the save: a built-in font, the frit vtium.conf's
/// `font-name` names, as it is or as the save leaves it, and the frit a
/// look's `font-name` names.
#[test]
fn r_refuses_what_remove_frit_refuses() {
    use vtium_config::tui::desk::Opened;
    use vtium_config::tui::model::App;
    let conf = inputs().conf.expect("the fixture");
    let names_minspec = conf.replace("font-name=unifont", "font-name=minspec-16");
    for (keys, draft, says) in [
        (on_set(5, &[b"r"]), conf.clone(), "built-in font"),
        // proof is the frit look proof draws
        (on_set(3, &[b"r"]), conf.clone(), "`[look proof]`"),
        // the draft names minspec-16 as vtium.conf's font-name
        (on_set(2, &[b"r"]), names_minspec.clone(), "font-name"),
    ] {
        let mut all = keys.clone();
        all.extend([b"i".as_slice(), b"y", b"q"]);
        let mut c = console().keys(&keys);
        let mut d = desk();
        d.draft.clone_from(&draft);
        run_desk(&mut c, &mut d);
        let screen = c.seen.clone().expect("the screen");
        let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(text.contains("cannot be removed:"), "{says}: {screen}");
        assert!(text.contains(says), "{says}: {screen}");
        let mut c = console().keys(&all);
        let mut d = desk();
        d.draft = draft;
        run_desk(&mut c, &mut d);
        assert!(
            d.plans.iter().all(|p| p.removals.is_empty()),
            "{says}: {:?}",
            d.plans
        );
    }
    // a frit marked for removal that the draft then names as font-name is
    // not removed: the install says why and plans nothing
    let (mut app, _) = App::new(inputs(), 80, 25);
    app.shared.pending.removals.push("tiny5".to_owned());
    app.shared.draft = Some(Opened {
        name: "mine".into(),
        text: conf.replace("font-name=unifont", "font-name=tiny5"),
        ..Opened::default()
    });
    let why = app.frit_plan().expect_err("the draft names tiny5");
    assert!(why.contains("font-name"), "{why}");
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
    let mut c = console().keys(&on_set(
        1,
        &[b"n", b"\r", b"\r", b"\r", b"\r", b"\r", b"\r", b"\r"],
    ));
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
    let mut c = console().keys(&on_set(4, &[b"r", b"q"]));
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
    let mut c = console().keys(&on_set(4, &[b"r", b"q", b"y"]));
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert_eq!(plan.removals.len(), 1, "{plan:?}");
    let mut c = console().keys(&on_set(4, &[b"r", b"q", b"n"]));
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert!(d.plans.is_empty());
    let mut c = console().keys(&on_set(4, &[b"r", b"q", b"l"]));
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
    let mut c = console().keys(&on_set(
        4,
        &[b"r", b"q", b"x", b"i", b"\x1b", b"i", b"y", b"q"],
    ));
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

#[test]
fn tab_does_nothing_in_the_new_frit_view_and_esc_goes_back_to_the_looks_tab() {
    for key in [TAB, BACKTAB] {
        let (screen, _) = reach_from(model::View::Looks, 80, 25, &[DOWN, b"n", b"abc", key]);
        assert_eq!(
            foot(&screen, words::KEYS_NEW),
            words::KEYS_NEW,
            "{key:?}: {screen}"
        );
        assert!(screen.contains("abc"), "{screen}");
    }
    // the form belongs to the Looks tab, so Esc goes back there
    let line = key_line(model::View::Looks, &[DOWN, b"n", b"abc", TAB, b"\x1b"]);
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
    app.shared
        .pending
        .frits
        .push(("fresh".to_owned(), "source=builtin:unifont\n".to_owned()));
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
