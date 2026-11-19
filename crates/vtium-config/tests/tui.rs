//! The TUI driven headlessly through its real loop (ADR-1201 D8): scripted
//! keys go in as bytes, the console's own terminal, vtium-term, answers on
//! the other side as a vtium console does, and each test reads the state's
//! effects in the bytes the TUI wrote and in the screen the terminal holds.
//!
//! The screens at 80x25 and 100x30 are vtium's own goldens in
//! `tests/screens/`, provisional until the maintainer reviews the layout:
//! `tests/screens/provisional.tsv` lists them, and a provisional golden
//! that drifts is reported and judges nothing.
//!
//! The tests of one tab are in `tui_<tab>.rs`, and the helpers every test
//! file shares are in `tui_common/`. This file holds what every tab shares:
//! the start, the live console, leaving, the pictures, the tab bar, the key
//! line, the lists' windows and each view's screens.

mod tui_common;
use tui_common::*;

/// The TUI on a vtium console drawing unifont that answered the start
/// requests, with tiny5 highlighted and tried by `t`, its switch answered.
fn trying_tiny5() -> vtium_config::tui::model::App {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event};
    let (mut app, _) = App::new(inputs(), 80, 25);
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Font("unifont".into()),
        Reply::Attributes,
    ] {
        app.event(Event::Input(Input::Reply(r)));
    }
    app.event(Event::Input(Input::Key(Key::Up)));
    let tried = app.event(Event::Input(Input::Key(Key::Char('t'))));
    assert_eq!(
        tried,
        [
            Effect::Write(model::switch("tiny5")),
            Effect::Wait(vtium_config::tui::shot::TICK)
        ]
    );
    for reply in [Reply::Font("tiny5".into()), Reply::Attributes] {
        assert_eq!(app.event(Event::Input(Input::Reply(reply))), []);
    }
    app
}

/// Whether `screen` shows the Flow view's key line.
fn is_flow_keys(screen: &str) -> bool {
    foot(screen, keys_of("flow")) == keys_of("flow")
}

/// The status line with a font set waiting to be installed, on a screen
/// `cols` wide.
fn status_with_a_font_set_waiting(cols: u16) -> String {
    let grid = (usize::from(cols), 25);
    let mut c = Console::vtium(usize::from(cols), 25, true, "unifont")
        .takes("minspec-16", grid)
        .takes("proof", grid)
        .takes("tiny5", grid)
        .takes("unifont", grid)
        .takes("unscii-8", grid)
        .keys(&[UP, b"r", b"q"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    screen.lines().nth(1).unwrap_or_default().to_owned()
}

#[test]
fn the_four_requests_go_first_in_their_order() {
    let mut c = console().keys(&[b"q"]);
    run(&mut c);
    assert!(
        c.written.starts_with(&model::probe()),
        "{:?}",
        String::from_utf8_lossy(&c.written[..60])
    );
    assert_eq!(
        model::probe(),
        b"\x1b[>q\x1b]50;?\x1b\\\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c"
    );
}

#[test]
fn leaving_by_q_gives_the_start_font_back() {
    let mut c = console().keys(&[UP, b"t", b"k", b"q"]);
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    assert_eq!(c.log.last().map(String::as_str), Some("reply unifont"));
    assert!(
        c.panic_bytes.is_empty(),
        "the font is back, so a panic writes nothing"
    );
}

/// Ctrl-C while the trial counts down leaves, and leaving gives the start
/// font back.
#[test]
fn leaving_by_ctrl_c_gives_the_start_font_back() {
    let mut c = console().keys(&[UP, b"t", b"\x03"]);
    assert_eq!(run(&mut c), 130);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
}

#[test]
fn leaving_by_sigterm_gives_the_start_font_back() {
    let mut c = console().keys(&[UP, b"t", b"!15"]);
    assert_eq!(run(&mut c), 143);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
}

#[test]
fn leaving_with_a_switch_in_flight_waits_for_both_replies() {
    // the switch to tiny5 and the leave come in one read
    let mut c = console().keys(&[&[UP, b"t", b"\x03"].concat()]);
    assert_eq!(run(&mut c), 130);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    assert_eq!(c.log.last().map(String::as_str), Some("reply unifont"));
    assert!(
        c.panic_bytes.is_empty(),
        "the font is back, so nothing is written again"
    );
}

/// The replies to the live trial's switch reach the TUI in two reads, the
/// first ending in the ESC that begins DA1's reply. While DA1's reply is
/// awaited, that ESC waits for the next read and begins the reply, so it
/// is not the Esc key, which would put the font back.
#[test]
fn an_esc_that_ends_a_read_while_da1_is_awaited_begins_its_reply() {
    let mut c = console().keys(&[UP, b"t", b"k"]);
    c.cut_replies = true;
    assert_eq!(run(&mut c), 143);
    assert_eq!(c.cuts, 2, "both switches' replies came in two reads");
    let before = String::from_utf8_lossy(&c.written[..c.seen_at]).into_owned();
    assert!(
        before.contains("\x1b]50;tiny5\x1b\\\x1b]50;?\x1b\\\x1b[c"),
        "{before:?}"
    );
    assert!(
        !before.contains("\x1b]50;unifont"),
        "k kept tiny5, and no Esc put unifont back: {before:?}"
    );
    // leaving gives the start font back
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
}

#[test]
fn another_terminal_gets_no_osc_50_but_the_start_query() {
    let mut c = Console::da1_only(80, 25).keys(&[DOWN, DOWN, UP, b"\r", b"q"]);
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.switches(), Vec::<String>::new());
    let text = String::from_utf8_lossy(&c.written);
    assert_eq!(
        text.matches("\x1b]50;").count(),
        1,
        "the query at start only"
    );
}

#[test]
fn with_font_operations_off_the_tui_says_so_and_offers_the_key() {
    let mut c = Console::vtium(80, 25, false, "unifont")
        .takes("tiny5", (80, 25))
        .keys(&[UP, UP]);
    run(&mut c);
    assert_eq!(c.switches(), Vec::<String>::new());
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("text preview · no live trial"), "{screen}");
    assert!(screen.contains("allow-font-ops turned off"), "{screen}");
    // t says why it does nothing, and sends nothing
    let mut c = Console::vtium(80, 25, false, "unifont")
        .takes("tiny5", (80, 25))
        .keys(&[UP, b"t"]);
    run(&mut c);
    assert_eq!(c.switches(), Vec::<String>::new());
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains(vtium_config::tui::words::TRIAL_OFF),
        "{screen}"
    );
}

#[test]
fn turning_font_operations_on_is_a_change_installed_like_any_other() {
    let mut d = desk();
    let mut c = Console::vtium(80, 25, false, "unifont").keys(&[b"o", b"i", b"y", b"q"]);
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert_eq!(plan.files.len(), 1);
    assert!(
        plan.files[0].new.contains("\nallow-font-ops\n"),
        "{:?}",
        plan.files[0].new
    );
}

#[test]
fn a_font_switch_that_changes_the_grid_redraws_at_the_new_grid() {
    let mut c = console().takes("tiny5", (100, 30)).keys(&[UP, b"t", b"k"]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows.len(), 30);
    assert!(
        rows[29].contains("Enter choose"),
        "the keys on the last row: {screen}"
    );
    assert!(
        rows[0].len() > 80,
        "the title reaches the new edge: {:?}",
        rows[0]
    );
}

/// A font kept from the live trial that leaves a grid too small for any
/// view draws the screen that says so, and keeps the keys to leave and to
/// undo the font on it however few its rows (ADR-1201 D3).
#[test]
fn a_grid_too_small_draws_the_way_out() {
    let mut c = console().takes("tiny5", (30, 8)).keys(&[UP, b"t", b"k"]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows.len(), 8);
    // the whole of what is wrong, then the keys, in the 8 rows
    let joined = rows.iter().map(|r| r.trim()).collect::<Vec<_>>().join(" ");
    assert!(
        joined.starts_with(&vtium_config::tui::words::too_small(30, 8)),
        "{screen}"
    );
    // the generated key line of the screen below the minimum size, and
    // the sentence about u under it
    let keys = vtium_config::tui::contract::key_pages("any.too-small")
        .and_then(|p| p.first())
        .expect("the key line below the minimum size");
    assert!(
        joined.ends_with(&format!(
            "{keys} {}",
            vtium_config::tui::words::TOO_SMALL_UNDO
        )),
        "{screen}"
    );
    assert!(rows.iter().all(|r| r.chars().count() <= 30), "{screen}");
}

#[test]
fn keeping_a_font_and_a_size_writes_each_into_the_draft_and_install_counts_them() {
    let mut d = desk();
    let mut c = console().keys(&[
        UP, b"\r", TO_LOOKS, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, b"\r", b"i",
    ]);
    run_desk(&mut c, &mut d);
    // the top level's last line is `version = 1`, so the new key follows it
    assert_eq!(
        d.draft,
        format!("# mine\nfont-name=tiny5\n{SECTIONS}")
            .replace("version = 1\n", "version = 1\nfont-size=12\n")
    );
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Install the draft? 2 lines added, 1 removed."),
        "{screen}"
    );
    assert!(d.plans.is_empty());
}

#[test]
fn installing_runs_one_install_and_the_console_keeps_the_installed_font() {
    let mut d = desk();
    let mut c = console().keys(&[UP, b"\r", b"i", b"y", b"q"]);
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert_eq!(d.plans.len(), 1);
    assert_eq!(c.saves.len(), 1);
    let file = &d.plans[0].files[0];
    assert_eq!(file.new, format!("# mine\nfont-name=tiny5\n{SECTIONS}"));
    // after installing, leaving does not switch back
    assert_eq!(c.switches(), ["tiny5"]);
}

/// After an install of the system's vtium.conf, the message line says
/// where vtium now runs with it, and each VT a head names that vtium
/// console would not enable (ADR-1201 D5).
#[test]
fn the_message_line_says_what_the_install_did_on_the_vts() {
    use vtium_config::vts::{Applied, Outcome};
    let mut d = desk();
    d.after = Some(Outcome {
        refused: vec![(7, vec!["terminal-1".to_owned()])],
        applied: Applied::Done(vec![2, 3, 12]),
    });
    let mut c = console().keys(&[UP, b"\r", b"i", b"y"]);
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    let joined = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        joined.contains("Installed draft mine. vtium now runs with it on VTs 2, 3, and 12. vtium is not enabled on VT 7, so head terminal-1 does not appear there."),
        "{screen}"
    );
}

#[test]
fn a_failed_install_keeps_the_draft_and_says_why() {
    let mut d = desk();
    let mut c = console().keys(&[UP, b"\r", b"i", b"y"]);
    c.save_result = Err("sudo: a password is required".into());
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Not installed: sudo: a password is required"),
        "{screen}"
    );
    assert!(d.draft.contains("font-name=tiny5"), "{}", d.draft);
    assert_eq!(d.installed, inputs().conf.expect("the fixture"));
}

/// The status line keeps its long words where both sides fit; else the
/// font sets waiting take their short words; else the preview's words
/// shorten too, so the left side fits whole.
#[test]
fn the_status_line_shortens_its_left_side_then_its_right_side() {
    let draft = " draft mine · no changes · no problems · ";
    for (cols, left, right) in [
        (100, "1 font set to install", "text preview · t: try live"),
        (90, "1 to install", "text preview · t: try live"),
        (82, "1 to install", "text preview · t: try live"),
        (81, "1 to install", "text · t: try live"),
        (80, "1 to install", "text · t: try live"),
    ] {
        let status = status_with_a_font_set_waiting(cols);
        assert!(
            status.starts_with(&format!("{draft}{left} ")),
            "{cols}: {status:?}"
        );
        assert!(
            status.ends_with(&format!("  {right}")),
            "{cols}: {status:?}"
        );
    }
}

#[test]
fn nothing_changed_means_nothing_to_install() {
    let mut c = console().keys(&[b"i", b"y", b"q"]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert!(d.plans.is_empty());
    assert!(c.saves.is_empty());
}

#[test]
fn each_view_at_80x25_and_100x30() {
    use model::View::Fonts;
    let scripts: [(&str, model::View, &[&[u8]]); 5] = [
        ("looks", Fonts, &[TO_LOOKS]),
        ("fonts", Fonts, &[]),
        ("settings", Fonts, &[b"\t"]),
        ("help", Fonts, &[b"\t", b"\t"]),
        ("new", Fonts, &[b"n"]),
    ];
    for (cols, rows) in [(80, 25), (100, 30)] {
        for (name, view, keys) in scripts {
            let (screen, bytes) = reach_from(view, cols, rows, keys);
            let lines: Vec<&str> = screen.lines().collect();
            assert_eq!(lines.len(), rows, "{name} at {cols}x{rows}");
            assert!(lines.iter().all(|l| l.chars().count() <= cols), "{name}");
            assert!(lines[0].contains(" Flow "), "{name}: {screen}");
            // the last row holds the key that quits
            let last = lines[rows - 1];
            assert!(last.contains(" quit"), "{name}: {screen}");
            golden(&format!("{name}-{cols}x{rows}"), &screen, &bytes);
        }
    }
}

#[test]
fn tab_and_shift_tab_step_the_views_from_every_column_of_the_flow_through_the_loop() {
    for column in 0..6 {
        let mut keys: Vec<&[u8]> = vec![RIGHT; column];
        keys.push(TAB);
        let line = key_line(model::View::Flow, &keys);
        assert_eq!(
            foot(&line, keys_of("settings")),
            keys_of("settings"),
            "Tab from column {column}"
        );
        keys.pop();
        keys.push(BACKTAB);
        let line = key_line(model::View::Flow, &keys);
        assert_eq!(
            foot(&line, words::KEYS_LOOKS),
            words::KEYS_LOOKS,
            "Shift-Tab from column {column}"
        );
    }
}

#[test]
fn tab_and_shift_tab_step_every_other_view_and_the_arrows_stay_in_it() {
    use model::View::{Effects, Flow, Fonts, Help, Looks, Settings};
    let own = |view| match view {
        Effects => words::KEYS_EFFECTS,
        Looks => words::KEYS_LOOKS,
        Fonts => words::KEYS_FONTS_LIVE,
        Settings => keys_of("settings"),
        _ => keys_of("help"),
    };
    let shown = |line: &str, view| match view {
        Flow => is_flow_keys(line),
        _ => foot(line, own(view)) == own(view),
    };
    for (view, next, before) in [
        (Fonts, Effects, Help),
        (Effects, Looks, Fonts),
        (Looks, Flow, Effects),
        (Settings, Help, Flow),
        (Help, Fonts, Settings),
    ] {
        let line = key_line(view, &[TAB]);
        assert!(shown(&line, next), "Tab from {view:?}: {line}");
        let line = key_line(view, &[BACKTAB]);
        assert!(shown(&line, before), "Shift-Tab from {view:?}: {line}");
        let arrows = [LEFT, LEFT, RIGHT, RIGHT, RIGHT, UP, DOWN, DOWN];
        let line = key_line(view, &arrows);
        assert_eq!(foot(&line, own(view)), own(view), "the arrows on {view:?}");
    }
}

/// The key line is one row in pages: `.` steps through the Flow view's
/// pages and back to the first, and every node the TUI moves to shows its
/// first page, whatever page the node before showed.
///
/// References:
/// - ADR-1403 D7, "The same keys in every view": the key line is one row
///   that shows its keys in pages, and `.` shows the next page.
#[test]
fn the_key_line_shows_its_pages_on_period_and_its_first_page_in_each_node() {
    use model::View::{Flow, Help};
    let flow = vtium_config::tui::contract::key_pages("flow").expect("the Flow view's pages");
    assert!(flow.len() > 1, "{flow:?}");
    let mut keys: Vec<&[u8]> = Vec::new();
    for page in flow.iter().chain(flow.first()) {
        assert_eq!(
            last_row(&key_line(Flow, &keys)),
            *page,
            "after {} presses",
            keys.len()
        );
        keys.push(b".");
    }
    // the second page, then a move inside the view keeps it
    assert_eq!(last_row(&key_line(Flow, &[b".", DOWN])), flow[1]);
    // the next view and Help's own pages start at 1, and so does the
    // Flow view's when the TUI comes back to it
    assert_eq!(last_row(&key_line(Flow, &[b".", TAB])), keys_of("settings"));
    assert_eq!(
        last_row(&key_line(Flow, &[b".", BACKTAB])),
        words::KEYS_LOOKS
    );
    assert_eq!(
        last_row(&key_line(Flow, &[b".", BACKTAB, TAB])),
        keys_of("flow")
    );
    // a key line of one page takes `.` as a key that does nothing
    let looks = key_line(Flow, &[BACKTAB]);
    assert_eq!(key_line(Flow, &[BACKTAB, b"."]), looks);
    let help = vtium_config::tui::contract::key_pages("help").expect("Help's pages");
    assert_eq!(last_row(&key_line(Help, &[b"."])), help[1]);
}

/// Tab leaves a question open on any view, and Esc answers it no.
#[test]
fn tab_leaves_a_question_open_on_any_view_and_esc_answers_no() {
    // a size kept on Looks, then i asks to install it
    let ask: [&[u8]; 4] = [TO_LOOKS, DOWN, b"\r", b"i"];
    let mut c = console().keys(&ask);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("Install the draft?"), "{screen}");
    assert_eq!(
        foot(&screen, keys_of("any.question.confirm")),
        keys_of("any.question.confirm"),
        "{screen}"
    );
    let mut c = console().keys(&[&ask[..], &[TAB]].concat());
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert_eq!(
        foot(&screen, keys_of("any.question.confirm")),
        keys_of("any.question.confirm"),
        "{screen}"
    );
    let mut c = console().keys(&[&ask[..], &[TAB, b"\x1b"]].concat());
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains(words::NOT_DONE), "{screen}");
    assert_eq!(
        foot(&screen, words::KEYS_LOOKS),
        words::KEYS_LOOKS,
        "{screen}"
    );
    assert!(d.plans.is_empty());
    // the question before leaving with a frit waiting: Tab leaves it
    // open, and Esc goes back
    let mut c = console().keys(&[UP, b"r", b"q"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert_eq!(
        foot(&screen, keys_of("any.question.leave")),
        keys_of("any.question.leave"),
        "{screen}"
    );
    let mut c = console().keys(&[UP, b"r", b"q", TAB]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert_eq!(
        foot(&screen, keys_of("any.question.leave")),
        keys_of("any.question.leave"),
        "{screen}"
    );
    let mut c = console().keys(&[UP, b"r", b"q", TAB, b"\x1b"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains(words::NOT_LEFT), "{screen}");
    assert_eq!(
        foot(&screen, words::KEYS_FONTS_LIVE),
        words::KEYS_FONTS_LIVE,
        "{screen}"
    );
}

#[test]
fn a_console_that_never_answers_the_give_back_is_left_after_the_wait() {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event};
    let (mut app, _) = App::new(inputs(), 80, 25);
    let mut started = Vec::new();
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Font("unifont".into()),
        Reply::Attributes,
    ] {
        started.extend(app.event(Event::Input(Input::Reply(r))));
    }
    // once the terminal is known, the helper is asked which sources each
    // frit that reads lacks, and for no preview, which the console draws
    let frits = ["minspec-16", "proof", "tiny5"].map(String::from).to_vec();
    assert_eq!(started, [Effect::Sources(frits)]);
    assert_eq!(app.event(Event::Input(Input::Key(Key::Up))), []);
    let tried = app.event(Event::Input(Input::Key(Key::Char('t'))));
    assert_eq!(
        tried,
        [
            Effect::Write(model::switch("tiny5")),
            Effect::Wait(vtium_config::tui::shot::TICK)
        ]
    );
    for reply in [Reply::Font("tiny5".into()), Reply::Attributes] {
        assert_eq!(app.event(Event::Input(Input::Reply(reply))), []);
    }
    assert_eq!(app.event(Event::Input(Input::Key(Key::Char('k')))), []);
    let leave = app.event(Event::Input(Input::Key(Key::Char('q'))));
    assert_eq!(
        leave,
        [
            Effect::Write(model::switch("unifont")),
            Effect::Wait(model::WAIT)
        ]
    );
    // keys while leaving do nothing; the wait's end leaves
    assert_eq!(app.event(Event::Input(Input::Key(Key::Down))), []);
    assert_eq!(app.event(Event::Timeout), [Effect::Exit(0)]);
}

/// A terminal that does not answer DA1 by the deadline at start leaves no
/// query outstanding, so an Esc after it is the Esc key at once, and a DA1
/// reply that comes late changes nothing.
#[test]
fn the_deadline_at_start_settles_the_four_requests() {
    use vtium_config::tui::input::{Input, Reply};
    use vtium_config::tui::model::{App, Event, Live};
    let (mut app, _) = App::new(inputs(), 80, 25);
    assert!(app.awaiting(), "the four requests are outstanding");
    app.event(Event::Input(Input::Reply(Reply::Version(
        "vtium(test)".into(),
    ))));
    app.event(Event::Timeout);
    assert!(!app.awaiting());
    assert_eq!(app.shared.live, Live::OpsOff);
    assert_eq!(app.event(Event::Input(Input::Reply(Reply::Attributes))), []);
    assert_eq!(app.shared.live, Live::OpsOff);
}

/// Each font switch is one batch, which DA1 ends. A console that answers
/// DA1 and not the switch's query leaves the switch unanswered: the TUI
/// marks the font refused, ends the trial, and sends it no second time.
#[test]
fn a_switch_whose_query_goes_unanswered_marks_its_font_refused_at_da1() {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{Effect, Event};
    assert_eq!(
        model::switch("tiny5"),
        b"\x1b]50;tiny5\x1b\\\x1b]50;?\x1b\\\x1b[c"
    );
    let (mut app, _) = vtium_config::tui::model::App::new(inputs(), 80, 25);
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Font("unifont".into()),
        Reply::Attributes,
    ] {
        app.event(Event::Input(Input::Reply(r)));
    }
    assert!(!app.awaiting(), "DA1's reply settled the start");
    app.event(Event::Input(Input::Key(Key::Up)));
    let tried = app.event(Event::Input(Input::Key(Key::Char('t'))));
    assert_eq!(
        tried,
        [
            Effect::Write(b"\x1b]50;tiny5\x1b\\\x1b]50;?\x1b\\\x1b[c".to_vec()),
            Effect::Wait(vtium_config::tui::shot::TICK)
        ]
    );
    assert!(app.awaiting());
    assert_eq!(app.event(Event::Input(Input::Reply(Reply::Attributes))), []);
    assert!(!app.awaiting());
    assert!(
        app.shared.refused.contains("tiny5"),
        "{:?}",
        app.shared.refused
    );
    assert!(app.shared.in_flight.is_none() && app.nav.trial.is_none());
    assert_eq!(
        app.shared.message.as_deref(),
        Some(vtium_config::tui::words::trial_refused("tiny5").as_str())
    );
    assert_eq!(app.event(Event::Input(Input::Key(Key::Char('t')))), []);
}

/// One preview is drawn at a time: a highlight that moves while one is
/// being drawn asks for the next once it comes, for whatever is
/// highlighted then.
#[test]
fn one_preview_is_drawn_at_a_time() {
    use vtium_config::specimen::Summary;
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event};
    let (mut app, _) = App::new(inputs(), 80, 25);
    let mut out = Vec::new();
    for r in [Reply::Version("vtium(test)".into()), Reply::Attributes] {
        out.extend(app.event(Event::Input(Input::Reply(r))));
    }
    assert!(out.contains(&Effect::Summary {
        font: "unifont".into(),
        height: 16
    }));
    assert_eq!(app.event(Event::Input(Input::Key(Key::Up))), []);
    assert_eq!(app.event(Event::Input(Input::Key(Key::Up))), []);
    let summary = Summary {
        cell: Cell {
            width: 8,
            height: 16,
        },
        grid: Some((80, 30)),
        covered: 1,
        of: 1,
        script: None,
    };
    let next = app.event(Event::Summarized {
        font: "unifont".into(),
        height: 16,
        summary: Ok(summary),
    });
    assert_eq!(
        next,
        [Effect::Summary {
            font: "proof".into(),
            height: 16
        }]
    );
}

/// The picture of the console in the Fonts view shows the TUI's own screen
/// at the grid the font's cell gives. Update asks the loop to draw that
/// screen, so the effect carries no screen of its own, and the view draws
/// it at the picture's size while the model keeps the terminal's size.
///
/// References:
/// - ADR-1206 D4, "Drawing takes the size as an argument".
#[test]
fn the_picture_of_the_console_is_drawn_at_its_own_size_and_the_model_keeps_its_size() {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event, Size};
    use vtium_config::tui::view::{as_text, screen, screen_at};
    let (mut app, _) = App::new(inputs(), 80, 25);
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Graphics(true),
        Reply::Attributes,
    ] {
        app.event(Event::Input(Input::Reply(r)));
    }
    app.event(Event::Detailed {
        font: "unifont".into(),
        height: 16,
        result: Err("none".into()),
    });
    // `s` turns the detailed specimen off, so the Fonts view asks for the
    // font's cell toward a picture of the console
    let asked = app.event(Event::Input(Input::Key(Key::Char('s'))));
    assert!(
        matches!(asked.as_slice(), [Effect::Cell { .. }]),
        "{asked:?}"
    );
    let cell = Ok((
        Cell {
            width: 8,
            height: 16,
        },
        16,
    ));
    let picture = app.event(Event::Celled {
        font: "unifont".into(),
        height: 16,
        cell,
    });
    let [Effect::ScreenPicture(shot)] = picture.as_slice() else {
        panic!("a picture of the TUI's screen: {picture:?}");
    };
    assert!(shot.screen.is_empty(), "the loop draws the screen");
    let (cols, rows) = shot.grid;
    assert_ne!((cols, rows), (80, 25), "the picture's grid is its own");
    let before = as_text(&screen(&app));
    let size = Size {
        cols,
        rows,
        ..app.size
    };
    let drawn = screen_at(&app, size);
    assert_eq!(drawn.len(), rows);
    assert_eq!((app.size.cols, app.size.rows), (80, 25));
    assert_eq!(as_text(&screen(&app)), before);
}

/// A picture that comes after the highlight moved is not shown: no
/// picture is sent, and the next is asked for, for the size highlighted
/// now (ADR-1201 D4).
#[test]
fn a_picture_that_comes_after_the_highlight_moved_is_not_shown() {
    use vtium_config::specimen::Picture;
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event};
    let (mut app, _) = App::new(inputs(), 80, 25);
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Graphics(true),
        Reply::Attributes,
    ] {
        app.event(Event::Input(Input::Reply(r)));
    }
    // the specimen of unifont comes, then the Looks view at 6 pt
    let shown = app.event(Event::Detailed {
        font: "unifont".into(),
        height: 16,
        result: Err("none".into()),
    });
    assert!(!shown.iter().any(|e| matches!(e, Effect::Write(_))));
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    let asked = app.event(Event::Input(Input::Key(Key::Down)));
    assert_eq!(
        asked,
        [Effect::Cell {
            font: "unifont".into(),
            height: 8
        }]
    );
    let cell = Ok((
        Cell {
            width: 4,
            height: 8,
        },
        8,
    ));
    let picture = app.event(Event::Celled {
        font: "unifont".into(),
        height: 8,
        cell,
    });
    assert!(
        matches!(picture.as_slice(), [Effect::Picture(_)]),
        "{picture:?}"
    );
    // the highlight moves to 7 pt while 6 pt's picture is drawn
    assert_eq!(app.event(Event::Input(Input::Key(Key::Down))), []);
    let came = app.event(Event::Pictured(Ok(Picture {
        width: 2,
        height: 1,
        rgba: vec![0; 8],
    })));
    let sent: Vec<u8> = came
        .iter()
        .filter_map(|e| match e {
            Effect::Write(b) => Some(b.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(!sent.windows(3).any(|w| w == b"a=T"), "{came:?}");
    assert_eq!(
        came.last(),
        Some(&Effect::Cell {
            font: "unifont".into(),
            height: 9
        })
    );
}

/// The cells under a picture on the screen are never written: the model
/// keeps the cells the picture covers, and each frame leaves them alone,
/// so the text under the picture never draws over it. The same frame
/// without the picture writes text into some of those cells.
///
/// References:
/// - ADR-1206 D5, "ratatui draws what changed": the cells under a kitty
///   graphics picture are marked to skip.
#[test]
fn no_frame_writes_over_the_cells_under_a_picture() {
    use vtium_config::specimen::Picture;
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event};
    use vtium_config::tui::shot::Covered;
    let (mut app, _) = App::new(inputs(), 80, 25);
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Graphics(true),
        Reply::Attributes,
    ] {
        app.event(Event::Input(Input::Reply(r)));
    }
    // the Looks view at 6 pt, whose picture goes beside the sizes
    app.event(Event::Input(Input::Key(Key::Char('3'))));
    app.event(Event::Input(Input::Key(Key::Down)));
    let cell = Ok((
        Cell {
            width: 4,
            height: 8,
        },
        8,
    ));
    app.event(Event::Celled {
        font: "unifont".into(),
        height: 8,
        cell,
    });
    // a picture wider and taller than its area covers the area, which
    // reaches the sentence under the sizes
    let came = app.event(Event::Pictured(Ok(Picture {
        width: 480,
        height: 320,
        rgba: vec![0; 480 * 320 * 4],
    })));
    assert!(
        came.iter()
            .any(|e| matches!(e, Effect::Write(b) if b.windows(3).any(|w| w == b"a=T"))),
        "{came:?}"
    );
    // the picture goes at row 6 and column 26, from 1, in the Looks view
    // at 80 by 25, and its area reaches the right edge and the row above
    // the message line at its longest
    let covered = Covered {
        row: 5,
        col: 25,
        cols: 55,
        rows: 15,
    };
    assert_eq!(app.shared.picture, Some(covered));
    // what a frame writes into each cell, on a console that showed
    // nothing
    let written = |app: &App| {
        let mut t = Terminal::new(80, 25, 0);
        let mut rest: &[u8] = &vtium_config::tui::view::draw(app);
        while !rest.is_empty() {
            let taken = t.input(rest);
            rest = &rest[taken..];
        }
        let s = t.screen();
        let mut cells = Vec::new();
        for r in covered.row..covered.row + covered.rows {
            let line = s.line(r).expect("a row");
            let row: Vec<char> = line
                .cells()
                .iter()
                .map(|&c| match line.glyph(c) {
                    Glyph::Char(ch) => ch,
                    Glyph::Cluster(_) => '*',
                    Glyph::Empty => ' ',
                })
                .collect();
            cells.extend(row.into_iter().skip(covered.col).take(covered.cols));
        }
        cells
    };
    assert!(
        written(&app).iter().all(|c| *c == ' '),
        "a frame wrote over the picture"
    );
    // positive control: without the picture, the frame writes the
    // sentence under the sizes into those cells
    let mut bare = app.clone();
    bare.shared.picture = None;
    assert!(written(&bare).iter().any(|c| *c != ' '));
}

/// A picture that replaces another stays on the screen: the loop clears
/// the screen when the old picture leaves, before the new one is shown,
/// and never between a picture's show and its delete. ED 2 erases the
/// pictures on the screen, so a clear written with the next frame would
/// take the new picture with it.
///
/// References:
/// - ADR-1206 D5, "ratatui draws what changed": after a picture is
///   deleted the loop clears the screen, so the next frame draws whole.
#[test]
fn a_picture_that_replaces_another_is_not_erased_by_the_clear() {
    let mut c = outside().keys(&[TO_LOOKS, DOWN, DOWN]);
    c.graphics = true;
    drive(&mut c, inputs()).expect("the loop");
    let at = |pat: &[u8]| -> Vec<usize> {
        c.written
            .windows(pat.len())
            .enumerate()
            .filter(|(_, w)| *w == pat)
            .map(|(i, _)| i)
            .collect()
    };
    let (shows, deletes, clears) = (at(b"a=T"), at(b"a=d"), at(b"\x1b[2J"));
    // positive control: a picture replaces another, so the screen is
    // cleared after a delete with a picture shown after it
    assert!(
        deletes
            .iter()
            .any(|d| shows.iter().any(|s| s > d) && clears.iter().any(|e| e > d)),
        "shows {shows:?}, deletes {deletes:?}, clears {clears:?}"
    );
    for show in &shows {
        let gone = deletes.iter().find(|d| *d > show).copied();
        assert!(
            !clears
                .iter()
                .any(|e| e > show && gone.is_none_or(|d| *e < d)),
            "a clear erased the picture shown at byte {show}: shows {shows:?}, deletes {deletes:?}, clears {clears:?}"
        );
    }
}

/// A picture that fails while the live trial counts down leaves the
/// countdown on the message line.
#[test]
fn a_failed_picture_leaves_the_live_trial_s_countdown() {
    use vtium_config::tui::model::Event;
    let mut app = trying_tiny5();
    let countdown = app.shared.message.clone();
    app.event(Event::Pictured(Err("the helper failed".into())));
    app.event(Event::Detailed {
        font: "tiny5".into(),
        height: 16,
        result: Err("the helper failed".into()),
    });
    assert_eq!(app.shared.message, countdown);
}

/// The live trial counts its ticks down on the message line and, with no
/// keep, puts the font back on the last one (ADR-1201 D4).
#[test]
fn the_live_trial_puts_the_font_back_after_its_ticks() {
    use vtium_config::tui::model::{Effect, Event};
    use vtium_config::tui::shot::{TICK, TRIAL_TICKS};
    let mut app = trying_tiny5();
    assert_eq!(TRIAL_TICKS, 10);
    for left in (1..TRIAL_TICKS).rev() {
        assert_eq!(
            app.event(Event::Timeout),
            [Effect::Wait(TICK)],
            "{left} left"
        );
        assert_eq!(
            app.shared.message.as_deref(),
            Some(vtium_config::tui::words::trial_countdown("tiny5", left).as_str())
        );
    }
    assert_eq!(
        app.event(Event::Timeout),
        [Effect::Write(model::switch("unifont"))]
    );
    assert!(app.nav.trial.is_none());
    // a tick after the trial does nothing
    assert_eq!(app.event(Event::Timeout), []);
}

/// `k` keeps the tried font: no tick puts it back.
#[test]
fn the_keep_key_stops_the_live_trial_s_revert() {
    use vtium_config::tui::input::{Input, Key};
    use vtium_config::tui::model::Event;
    let mut app = trying_tiny5();
    assert_eq!(app.event(Event::Input(Input::Key(Key::Char('k')))), []);
    for _ in 0..20 {
        assert_eq!(app.event(Event::Timeout), []);
    }
    assert_eq!(app.shared.shown_font.as_deref(), Some("tiny5"));
}

/// Any other key during the countdown puts the font back and does nothing
/// else: the highlight stays, and `q` does not leave.
#[test]
fn any_other_key_during_the_live_trial_puts_the_font_back() {
    use vtium_config::tui::input::{Input, Key};
    use vtium_config::tui::model::{Effect, Event};
    for key in [Key::Down, Key::Char('q'), Key::Enter, Key::Char('t')] {
        let mut app = trying_tiny5();
        let at = app.fonts.at();
        assert_eq!(
            app.event(Event::Input(Input::Key(key))),
            [Effect::Write(model::switch("unifont"))],
            "{key:?}"
        );
        assert_eq!(app.fonts.at(), at, "{key:?} moved the highlight");
        assert!(
            app.nav.trial.is_none() && app.shared.leaving.is_none(),
            "{key:?}"
        );
        assert_eq!(app.drafted("font-name").as_deref(), Some("unifont"));
    }
}

/// Leaving while the switch that puts the start font back is still in
/// flight sends it no second time: the TUI waits for that switch's reply
/// and leaves, since the console ends in the start font either way.
#[test]
fn leaving_with_the_start_font_in_flight_sends_it_once() {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{Effect, Event};
    let mut app = trying_tiny5();
    // a key puts unifont back; its reply has not come when q arrives
    assert_eq!(
        app.event(Event::Input(Input::Key(Key::Down))),
        [Effect::Write(model::switch("unifont"))]
    );
    let leave = app.event(Event::Input(Input::Key(Key::Char('q'))));
    assert!(
        !leave
            .iter()
            .any(|e| matches!(e, Effect::Write(b) if b.windows(3).any(|w| w == b"]50"))),
        "no second switch: {leave:?}"
    );
    assert!(leave.contains(&Effect::Wait(model::WAIT)), "{leave:?}");
    // the TUI leaves at DA1's reply, which settles the switch, so no reply
    // reaches the shell after it
    assert_eq!(
        app.event(Event::Input(Input::Reply(Reply::Font("unifont".into())))),
        []
    );
    assert_eq!(
        app.event(Event::Input(Input::Reply(Reply::Attributes))),
        [Effect::Exit(0)]
    );
}

/// Leaving during the countdown, by Ctrl-C or a signal, gives the start
/// font back and leaves once the console has answered.
#[test]
fn leaving_during_the_live_trial_gives_the_start_font_back() {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{Effect, Event};
    for event in [Event::Input(Input::Key(Key::Interrupt)), Event::Signal(15)] {
        let mut app = trying_tiny5();
        let out = app.event(event.clone());
        assert_eq!(
            out.first(),
            Some(&Effect::Write(model::switch("unifont"))),
            "{event:?}"
        );
        assert!(app.nav.trial.is_none());
        let font = app.event(Event::Input(Input::Reply(Reply::Font("unifont".into()))));
        assert_eq!(font, [], "{event:?}");
        let done = app.event(Event::Input(Input::Reply(Reply::Attributes)));
        assert!(matches!(done.as_slice(), [Effect::Exit(_)]), "{done:?}");
    }
}

/// While the trial counts down on a grid too small for any view, the small
/// screen says first how to keep the font or put it back.
#[test]
fn the_small_screen_shows_the_live_trial_s_countdown() {
    use vtium_config::tui::model::{Event, Size};
    use vtium_config::tui::view::{as_text, screen};
    let mut app = trying_tiny5();
    app.event(Event::Resize(Size {
        cols: 40,
        rows: 12,
        cell_px: app.size.cell_px,
    }));
    let text = as_text(&screen(&app));
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        joined.starts_with("Trying tiny5 on the console for 10 s. Press k to keep it"),
        "{text}"
    );
    assert!(joined.contains("vtium-config needs at least"), "{text}");
}

/// The picture shows the whole display at its resolution, laid out at the
/// grid the font's cell gives on it: the TUI's own screen at that grid,
/// the screen that says the grid is too small included (ADR-1201 D4).
#[test]
fn the_picture_shows_the_display_at_the_grid_the_font_gives() {
    let mut c = outside().keys(&[b"s", UP]);
    c.graphics = true;
    run(&mut c);
    // unifont's 8x16 and tiny5's 4x6 on the fixture's 640x480 display
    let fonts: Vec<&str> = c.shots.iter().map(|s| s.font.as_str()).collect();
    assert_eq!(fonts, ["unifont", "tiny5"]);
    let displays: Vec<(u32, u32)> = c.shots.iter().map(|s| s.display).collect();
    assert_eq!(displays, [(640, 480), (640, 480)]);
    let grids: Vec<(usize, usize)> = c.shots.iter().map(|s| s.grid).collect();
    assert_eq!(grids, [(80, 30), (160, 80)]);
    for shot in &c.shots {
        assert_eq!(shot.screen.len(), shot.grid.1);
        let first = vtium_config::tui::view::text(&shot.screen[0]);
        assert!(first.contains("Fonts"), "the TUI's own tab bar: {first}");
    }
    // the line beside it says what it shows
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("tiny5 at 12 pt on this 640x480 display: each character cell is 4x6 pixels, and the console will be 160 characters wide and 80 lines high."),
        "{screen}"
    );
    // on a display too small for the TUI at that cell, the picture is the
    // screen that says so
    let mut c = outside().keys(&[b"s"]);
    c.graphics = true;
    let mut small = inputs();
    small.display = Some((320, 240));
    drive(&mut c, small).expect("the loop");
    let shot = c.shots.first().expect("a picture");
    assert_eq!(shot.grid, (40, 15));
    let first = vtium_config::tui::view::text(&shot.screen[0]);
    assert!(first.starts_with("This terminal is too small"), "{first}");
}

#[test]
fn a_terminal_that_answers_nothing_is_settled_by_the_wait() {
    use vtium_config::tui::model::{App, Event, Live};
    let (mut app, first) = App::new(inputs(), 80, 25);
    assert_eq!(first[1], model::Effect::Wait(model::WAIT));
    assert_eq!(app.shared.live, Live::Asking);
    app.event(Event::Timeout);
    assert_eq!(app.shared.live, Live::NotVtium);
}

#[test]
fn a_failed_command_s_words_show_on_the_message_line() {
    let mut c = console().keys(&[UP, b"\r", b"i", b"y"]);
    c.save_result = Err(
        "vtium-config: vtium check-config refused the draft:\n/x/vtium.conf:2: font-size=\"lots\" is not a whole number, ignored".into(),
    );
    run_desk(&mut c, &mut desk());
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("Not installed: vtium-config: vtium check-config refused the draft:"),
        "{screen}"
    );
    assert!(text.contains("is not a whole number"), "{screen}");
}

/// Outside a vtium console the TUI can switch, the Fonts view keeps
/// today's preview: the written description where the terminal shows no
/// images, the helper's picture where it does, and `s` switches the
/// picture. It never prints the specimen as text.
#[test]
fn outside_a_vtium_console_the_preview_is_a_picture_or_a_description() {
    for graphics in [false, true] {
        let mut c = outside().keys(&[b"s"]);
        c.graphics = graphics;
        run(&mut c);
        let screen = c.seen.clone().expect("the screen");
        assert!(!screen.contains("@ABCDEFGHIJ"), "{screen}");
        assert!(screen.contains(words::KEYS_FONTS), "{screen}");
        if graphics {
            assert!(!c.specimens.is_empty() && !c.shots.is_empty());
        } else {
            assert!(screen.contains("does not support images"), "{screen}");
            assert!(!c.summaries.is_empty());
        }
    }
    // a terminal that is not vtium at all asks for the description too
    let mut c = Console::da1_only(80, 25).keys(&[]);
    run(&mut c);
    assert_eq!(c.summaries, [("unifont".to_owned(), 16)]);
}

/// A vtium console the TUI can switch gets no picture, even where it
/// draws kitty graphics: the preview is its own text (ADR-1201 D4), and a
/// font tried and kept still comes back when the TUI leaves.
#[test]
fn a_vtium_console_that_draws_pictures_gets_no_picture() {
    let mut c = console().keys(&[UP, b"t", b"k", TO_LOOKS, DOWN, b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    assert!(c.pictures.is_empty() && c.specimens.is_empty() && c.cells.is_empty());
    assert_eq!(count(&c.written, b"\x1b_Ga=T"), 0);
    let text = String::from_utf8_lossy(&c.written).into_owned();
    assert!(text.contains("\x1b]50;unifont\x1b\\"), "the font back");
}

#[test]
fn without_kitty_graphics_no_picture_is_sent_and_the_view_says_so() {
    let mut c = outside().keys(&[TO_LOOKS, DOWN, DOWN]);
    run(&mut c);
    assert!(c.pictures.is_empty());
    assert_eq!(count(&c.written, b"\x1b_Ga=T"), 0);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("does not support images"), "{screen}");
}

#[test]
fn the_picture_never_reaches_the_footer_and_never_passes_the_cap() {
    // 80x25 at 8x16: rows 7 to 20 are the area, above the longest message
    let mut c = outside().keys(&[TO_LOOKS, DOWN]);
    c.graphics = true;
    run(&mut c);
    let (_, _, (w, h)) = c.pictures.last().cloned().expect("6 pt's picture");
    assert!(h <= (25 - 8) * 16 && w <= (80 - 25) * 8, "{w}x{h}");
    // a terminal of very large cells is capped
    let mut c = outside().keys(&[TO_LOOKS, DOWN]);
    c.graphics = true;
    c.cell_px = Some((200, 400));
    run(&mut c);
    let (_, _, max) = c.pictures.last().cloned().expect("6 pt's picture");
    assert_eq!(max, vtium_config::specimen::PICTURE_MAX);
}

/// Where the preview area cannot show one line of text at the size's
/// cell, the message line says so, and no sliver of a picture is sent.
#[test]
fn a_preview_area_less_than_one_line_high_says_so_on_the_message_line() {
    // cells of 2x1 pixels leave the Looks view's area 14 pixels high, and
    // the stand-in's 8x16 cell needs 16
    let mut c = outside().keys(&[TO_LOOKS, DOWN]);
    c.graphics = true;
    c.cell_px = Some((2, 1));
    run(&mut c);
    let (_, _, (_, h)) = c
        .pictures
        .last()
        .cloned()
        .expect("6 pt's picture asked for");
    assert!(h < 16, "{h}");
    // the Fonts view's detailed specimen, which the loop opens on, is the
    // only picture sent
    assert_eq!(count(&c.written, b"\x1b_Ga=T"), c.specimens.len());
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("No preview image of the console: the preview area is too small to show one line of text at this size. Make this terminal larger, or choose a smaller size."),
        "{screen}"
    );
}

#[test]
fn display_dpi_outside_the_console_s_bounds_is_not_used() {
    use vtium_config::tui::density;
    let none = &[];
    assert_eq!(
        density(Some("display-dpi=150\n"), none),
        (150, DpiFrom::Conf)
    );
    assert_eq!(
        density(Some("display-dpi=1000\n"), none),
        (1000, DpiFrom::Conf)
    );
    assert_eq!(
        density(Some("display-dpi=1001\n"), none),
        (96, DpiFrom::Assumed)
    );
    assert_eq!(
        density(Some("display-dpi=19\n"), none),
        (96, DpiFrom::Assumed)
    );
    assert_eq!(
        density(Some("display-dpi=lots\n"), none),
        (96, DpiFrom::Assumed)
    );
    assert_eq!(density(None, none), (96, DpiFrom::Assumed));
    // a display's EDID comes after display-dpi and before the assumption:
    // the first display that states its size
    use vtium_config::display::Display;
    let displays = [
        Display {
            pixels: (1024, 768),
            mm: None,
        },
        Display {
            pixels: (2560, 1440),
            mm: Some((600, 340)),
        },
    ];
    assert_eq!(density(None, &displays), (108, DpiFrom::Edid));
    assert_eq!(
        density(Some("display-dpi=150\n"), &displays),
        (150, DpiFrom::Conf)
    );
}

#[test]
fn after_installing_a_font_leaving_puts_the_console_in_the_installed_font() {
    // try and keep tiny5 on the console, keep it in the draft, try proof
    // and keep it on the console, install, leave
    let mut c = console().keys(&[UP, b"t", b"k", b"\r", UP, b"t", b"k", b"i", b"y", b"q"]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert_eq!(
        d.plans[0].files[0].new,
        format!("# mine\nfont-name=tiny5\n{SECTIONS}")
    );
    assert_eq!(c.switches(), ["tiny5", "proof", "tiny5"]);
}

/// Below 80 by 25 cells no view is drawn: the screen names the size the
/// TUI needs and this one's, the commands, which work at any size, and the
/// keys; `u` puts back the font a preview changed, and the views come back
/// with the grid.
#[test]
fn below_the_minimum_the_screen_says_so_and_u_undoes_the_preview() {
    use vtium_config::tui::words;
    for grid in [(79, 30), (80, 24)] {
        let mut c = Console::vtium(80, 25, true, "unifont")
            .takes("unifont", (80, 25))
            .takes("tiny5", grid)
            .keys(&[UP, b"t", b"k"]);
        run(&mut c);
        let screen = c.seen.clone().expect("the screen");
        let joined = screen.lines().map(str::trim).collect::<Vec<_>>().join(" ");
        assert!(
            joined.contains(&words::too_small(grid.0, grid.1)),
            "{grid:?}: {screen}"
        );
        assert!(joined.contains("vtium-config fonts"), "{screen}");
        assert!(joined.contains("vtium-config set"), "{screen}");
        let keys = vtium_config::tui::contract::key_pages("any.too-small")
            .and_then(|p| p.first())
            .expect("the key line below the minimum size");
        assert!(joined.contains(keys), "{screen}");
        assert!(joined.contains(words::TOO_SMALL_UNDO), "{screen}");
        // keys that would move are ignored below the minimum; u gives the
        // console its start font back, and the views come back
        let mut c = Console::vtium(80, 25, true, "unifont")
            .takes("unifont", (80, 25))
            .takes("tiny5", grid)
            .takes("proof", (80, 25))
            .keys(&[UP, b"t", b"k", UP, b"u"]);
        run(&mut c);
        assert_eq!(c.switches(), ["tiny5", "unifont"], "{grid:?}");
        let screen = c.seen.clone().expect("the screen");
        assert!(screen.contains("Fonts"), "{screen}");
        assert!(!screen.contains("vtium-config needs"), "{screen}");
    }
}

#[test]
fn at_80x25_no_key_or_sentence_is_cut() {
    use vtium_config::tui::words;
    // the keys that reach a view, its key line, and its sentences
    type Case<'a> = (&'a [&'a [u8]], &'a str, &'a [&'a str]);
    let views: [Case; 7] = [
        (&[], words::KEYS_FONTS_LIVE, &[]),
        (&[TO_EFFECTS], words::KEYS_EFFECTS, &[words::EFFECTS_COMING]),
        (
            &[TO_LOOKS],
            words::KEYS_LOOKS,
            &[words::CONSOLE_SIZE_CAPTION],
        ),
        (&[TO_SETTINGS], keys_of("settings"), &[words::SECTIONS_HINT]),
        (
            &[TO_SETTINGS, b"\r"],
            keys_of("settings.section"),
            &[words::SETTINGS_HINT],
        ),
        (
            &[TO_SETTINGS, b"\r", b"\x1b[F", b"\r"],
            keys_of("settings.section.list"),
            &[words::VALUES_HINT],
        ),
        (&[TO_HELP], keys_of("help"), &[]),
    ];
    for (keys, key_line, sentences) in views {
        let screen = reach(80, 25, keys);
        let rows: Vec<&str> = screen.lines().collect();
        assert_eq!(rows.len(), 25);
        for item in key_line.split("  ") {
            assert!(
                rows.iter().any(|r| r.contains(item)),
                "{item:?} whole:\n{screen}"
            );
        }
        // a sentence wrapped by words reads whole once its rows are joined
        let joined = rows.iter().map(|r| r.trim()).collect::<Vec<_>>().join(" ");
        for sentence in sentences {
            assert!(joined.contains(sentence), "{sentence:?}:\n{screen}");
        }
        // the views and the actions fit the tab bar
        for name in ["Fonts", "Effects", "Looks", "Flow", "Settings", "Help"] {
            assert!(rows[0].contains(name), "{name}: {:?}", rows[0]);
        }
        // the Help view shows no actions
        //
        // References:
        // - ADR-1403 D7, "The same keys in every view": Help's tab bar
        //   shows no action.
        if key_line != keys_of("help") {
            for (_, short) in words::ACTIONS {
                assert!(rows[0].contains(short), "{short}: {:?}", rows[0]);
            }
        }
    }
}

/// The window of a tab's list: update keeps the highlighted item in view
/// after every event, and the view draws ratatui's List from a copy.
///
/// References:
/// - ADR-1206 D4, "A list's selection lives in the tab's state".
mod list_window {
    use vtium_config::tui::desk::Kept;
    use vtium_config::tui::input::{Input, Key};
    use vtium_config::tui::layout;
    use vtium_config::tui::model::{App, Event, Size, View};
    use vtium_config::tui::view;

    fn press(app: &mut App, key: Key) {
        app.event(Event::Input(Input::Key(key)));
    }

    /// The Settings tab at `cols` × `rows`, with its second section open:
    /// Colors, whose keys are longer than the window at 80 × 25.
    fn colors(cols: usize, rows: usize) -> App {
        let mut i = super::inputs();
        i.view = View::Settings;
        let mut app = App::new(i, cols, rows).0;
        press(&mut app, Key::Down);
        press(&mut app, Key::Enter);
        app
    }

    /// The text of the highlighted row on the screen, which the list
    /// draws in reverse video, and the row it stands on.
    fn highlighted(app: &App) -> Option<(usize, String)> {
        view::screen(app).iter().enumerate().find_map(|(y, row)| {
            row.iter()
                .find(|s| s.style.reverse && y > 2)
                .map(|s| (y, s.text.trim().to_owned()))
        })
    }

    #[test]
    fn the_highlighted_key_s_last_row_stays_at_the_window_s_bottom_and_a_resize_fits_the_window() {
        let mut app = colors(80, 25);
        assert_eq!(app.settings.offset(), 0);
        press(&mut app, Key::End);
        let end = app.settings.offset();
        assert!(end > 0, "the section is longer than the window");
        // the highlighted key is the window's last row, above the blank
        // row that starts the sentences under the list
        let (row, _) = highlighted(&app).expect("a highlighted key");
        let blank_below = |app: &App, row: usize| {
            view::screen(app)
                .get(row + 1)
                .is_some_and(|r| r.iter().all(|s| s.text.trim().is_empty()))
        };
        assert!(
            blank_below(&app, row),
            "{}",
            view::as_text(&view::screen(&app))
        );
        press(&mut app, Key::Up);
        assert_eq!(app.settings.offset(), end - 1);
        assert_eq!(highlighted(&app).map(|(y, _)| y), Some(row));
        // a terminal 5 rows taller shows 5 more rows of the list, so the
        // window starts 5 rows higher
        let size = Size {
            rows: 30,
            ..app.size
        };
        app.event(Event::Resize(size));
        assert_eq!(app.settings.offset(), end - 1 - 5);
        assert_eq!(highlighted(&app).map(|(y, _)| y), Some(row + 5));
    }

    /// The Flow view at 80 × 25 with Restore's list open on a history of
    /// 40 files, longer than the list's window.
    fn long_history() -> App {
        let mut i = super::inputs();
        i.view = View::Flow;
        let mut app = App::new(i, 80, 25).0;
        let kept = (0..40)
            .map(|n| Kept {
                name: format!("DRAFT-{n}.conf"),
                text: String::new(),
            })
            .collect();
        app.flow.show_history(kept);
        app.event(Event::Resize(app.size));
        app
    }

    /// Restore's window ends above the message and the key line, so the
    /// key line stays whole whichever entry is highlighted, and the
    /// highlighted entry is always on the screen.
    #[test]
    fn restore_s_list_of_a_long_history_keeps_the_key_line_whole_and_the_highlight_in_view() {
        let mut app = long_history();
        let keys = format!(" {}", layout::key_line(&app));
        let mut at = 0usize;
        let moves = [Key::Down; 25].into_iter().chain([
            Key::PageDown,
            Key::End,
            Key::Up,
            Key::PageUp,
            Key::Home,
        ]);
        for key in moves {
            press(&mut app, key);
            at = match key {
                Key::Down => at + 1,
                Key::Up => at - 1,
                Key::PageDown => (at + 10).min(39),
                Key::PageUp => at.saturating_sub(10),
                Key::End => 39,
                _ => 0,
            };
            let screen = view::screen(&app);
            assert_eq!(
                view::text(&screen[24]).trim_end(),
                keys.trim_end(),
                "{key:?}: {}",
                view::as_text(&screen)
            );
            let (row, entry) = highlighted(&app).expect("a highlighted entry");
            assert_eq!(entry, format!("{}  DRAFT-{at}.conf", at + 1), "{key:?}");
            // 19 entries show on rows 5 to 23, under the title on row 4,
            // and the highlighted one is the window's last once the list
            // has scrolled
            assert_eq!(row, 5 + at.min(18), "{key:?}");
            let (_, list) = app.flow.restore().expect("Restore's list");
            assert_eq!(list.offset, at.saturating_sub(18), "{key:?}");
        }
        // a message of three rows takes the rows above the key line, and
        // the window ends above the message
        app.shared.message = Some("word ".repeat(40));
        press(&mut app, Key::End);
        let screen = view::screen(&app);
        let (row, entry) = highlighted(&app).expect("a highlighted entry");
        assert_eq!(entry, "40  DRAFT-39.conf");
        assert_eq!(row, 20, "{}", view::as_text(&screen));
        assert!(view::text(&screen[21]).starts_with(" word word"));
        assert_eq!(view::text(&screen[24]).trim_end(), keys.trim_end());
    }

    #[test]
    fn a_frame_leaves_the_model_as_it_was() {
        let mut app = colors(80, 25);
        press(&mut app, Key::End);
        let (_, key) = highlighted(&app).expect("a highlighted key");
        // a long message arrives after update settled the window, so the
        // window no longer fits the rows left for the list, and ratatui's
        // List moves the window on the copy the view draws from
        app.shared.message = Some("word ".repeat(60));
        let (tab, before) = (app.settings.clone(), format!("{app:?}"));
        // the model is lent mutably, as a frame that took it so would need
        let model: &mut App = &mut app;
        let mut painter = view::Painter::new().expect("a painter");
        painter.frame(model).expect("a frame");
        assert_eq!(app.settings, tab);
        assert!(format!("{app:?}") == before, "the frame changed the model");
        assert_eq!(highlighted(&app).map(|(_, k)| k), Some(key));
    }
}
