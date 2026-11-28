//! The macro player: the two example macros of the record played to their
//! end, a failure's report and its keys from the start, the keys a terminal
//! cannot send refused at parse, the live run and its log, the scratch
//! copy, a key from the terminal that stops the player, debug mode, and
//! headless play.
//!
//! The macros play against two tabs of this file's own, which draw the
//! screens the example macros walk through: Looks lists the looks
//! and opens a look's card, whose Font set and Size open a list to pick
//! from, and Fonts lists the fonts, of which the built-in one is always
//! active. Each test plays on the screen map's machine or the large
//! machine, at 100×30 or 80×25, through the loop the binary runs, with
//! the port of headless play.
//!
//! The screens the player adds, "scratch copy" and LIVE on the top row
//! and its lines on the message row, are kept as snapshots under
//! `tests/snapshots/`, each with four screens: the screen map's machine
//! and the large machine, each at 100×30 and at 80×25.
//!
//! References:
//! - ADR-1208 D2: the two example macros, which tests/macros/ holds.
//! - ADR-1208 D8: the scenario tests play macros, and a failure names the
//!   macro, the line and the keys from the start.

#![expect(clippy::expect_used, reason = "a test fails by panicking")]

mod common;

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ratatui_core::layout::Rect;
use ratatui_core::style::{Modifier, Style};
use ratatui_core::terminal::Frame;
use vtium_config_core::document::{self, Edit, Installed};
use vtium_config_core::draft::Plan;
use vtium_config_core::flowmodel::Present;
use vtium_config_core::ui::{Action, Change, Command, Message, Request, Shared};
use vtium_config_macro::parse;
use vtium_config_tui::BoxTab;
use vtium_config_tui::app::{App, Event, Size};
use vtium_config_tui::live::machine::{Machine as Loop, Ready};
use vtium_config_tui::player::debug::Debug;
use vtium_config_tui::player::headless::{self, Headless};
use vtium_config_tui::player::{Log, Options, Outcome, Pace, Player};
use vtium_config_tui::port::{self, Port, Wake};
use vtium_config_tui::save::Saved;
use vtium_config_widgets::frame::Mode;
use vtium_config_widgets::tab::Tab;

use common::{
    Machine, SIZES, draw, marks_survive_without_color, rows_fit, snapshot, snapshot_text,
};

/// The rows a highlighted line is drawn in, as the list box draws it.
fn row_style(highlighted: bool) -> Style {
    if highlighted {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    }
}

/// Draws `lines` into `area`, the line `highlight` in reverse video.
fn lines(frame: &mut Frame<'_>, area: Rect, lines: &[String], highlight: usize) {
    let buf = frame.buffer_mut();
    for (i, line) in lines.iter().enumerate() {
        let Some(y) = area.y.checked_add(u16::try_from(i).expect("a row")) else {
            break;
        };
        if y >= area.bottom() {
            break;
        }
        let text = format!("  {line:<40}");
        buf.set_stringn(
            area.x,
            y,
            text,
            usize::from(area.width),
            row_style(i == highlight),
        );
    }
}

/// The looks the Looks tab lists.
const LOOKS: [&str; 3] = ["console", "big", "proof"];

/// The font sets a look's Font set lists.
const FONT_SETS: [&str; 3] = ["terminus", "cullet-mono", "proof"];

/// The sizes a look's Size lists.
const SIZES_PT: [&str; 4] = ["8 pt", "10 pt", "11 pt", "16 pt"];

/// The Looks tab: the looks, a look's card, and the list a line of the
/// card opens.
#[derive(Default)]
struct Looks {
    look: usize,
    /// The card's highlight, where a card is open.
    card: Option<usize>,
    /// The list open over the card, Font set's (0) or Size's (1), and its
    /// highlight.
    list: Option<(usize, usize)>,
    font_set: String,
    size: String,
}

impl Looks {
    fn new() -> Looks {
        Looks {
            font_set: "terminus".to_owned(),
            size: "16 pt".to_owned(),
            ..Looks::default()
        }
    }

    fn card_lines(&self) -> Vec<String> {
        let name = LOOKS.get(self.look).copied().unwrap_or_default();
        vec![
            format!("Name      {name}"),
            format!("Font set  {}", self.font_set),
            format!("Size      {}", self.size),
        ]
    }

    fn choices(which: usize) -> &'static [&'static str] {
        if which == 0 { &FONT_SETS } else { &SIZES_PT }
    }

    /// The change that sets the look's `key` to `value`, in the document.
    fn set(shared: &Shared, key: &str, value: &str) -> Command {
        let text = format!("{}# {key} = {value}\n", shared.document().text());
        Command::Change(Change::Document(document::Change::new(
            format!("{key} = {value} in console"),
            Edit::Text(text),
        )))
    }
}

impl Tab<Shared, Action, Command> for Looks {
    fn name(&self) -> &'static str {
        "Looks"
    }

    fn letter(&self) -> char {
        'l'
    }

    fn node(&self) -> &'static str {
        "looks"
    }

    fn draw(&self, _shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        match (self.card, self.list) {
            (_, Some((which, at))) => {
                let names: Vec<String> = Looks::choices(which)
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect();
                lines(frame, area, &names, at);
            }
            (Some(at), None) => lines(frame, area, &self.card_lines(), at),
            (None, None) => {
                let names: Vec<String> = LOOKS.iter().map(|s| (*s).to_owned()).collect();
                lines(frame, area, &names, self.look);
            }
        }
    }

    fn update(&mut self, shared: &Shared, action: Action) -> Vec<Command> {
        let step = |at: &mut usize, down: bool, len: usize| {
            *at = if down {
                (*at + 1).min(len - 1)
            } else {
                at.saturating_sub(1)
            };
        };
        match action.row {
            "down.view" | "up.view" => {
                step(&mut self.look, action.row == "down.view", LOOKS.len());
                Vec::new()
            }
            "enter.view.looks" => {
                self.card = Some(0);
                vec![Command::Request(Request::Open("looks.look"))]
            }
            "down.layer.look" | "up.layer.look" => {
                if let Some(at) = &mut self.card {
                    step(at, action.row == "down.layer.look", 3);
                }
                Vec::new()
            }
            "enter.layer.look" => match self.card {
                Some(at @ (1 | 2)) => {
                    self.list = Some((at - 1, 0));
                    vec![Command::Request(Request::Open("looks.look.list"))]
                }
                _ => Vec::new(),
            },
            "esc.layer.look" => {
                self.card = None;
                Vec::new()
            }
            "down.layer.list" | "up.layer.list" => {
                if let Some((which, at)) = &mut self.list {
                    step(
                        at,
                        action.row == "down.layer.list",
                        Looks::choices(*which).len(),
                    );
                }
                Vec::new()
            }
            "enter.layer.list" => {
                let Some((which, at)) = self.list.take() else {
                    return Vec::new();
                };
                let value = Looks::choices(which).get(at).copied().unwrap_or_default();
                let key = if which == 0 { "font-set" } else { "font-size" };
                if which == 0 {
                    self.font_set = value.to_owned();
                } else {
                    self.size = value.to_owned();
                }
                vec![
                    Looks::set(shared, key, value),
                    Command::Request(Request::Close),
                ]
            }
            "esc.layer.list" => {
                self.list = None;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

/// The fonts the Fonts tab lists, and what each says.
const FONTS: [(&str, &str); 5] = [
    ("terminus", "active"),
    ("built-in", "always active"),
    ("cullet-mono", "active"),
    ("unifont", "not active"),
    ("cullet-mono bold", "not active"),
];

/// The Fonts tab: the fonts, of which Space on the built-in one changes
/// nothing and says why.
#[derive(Default)]
struct Fonts {
    at: usize,
}

impl Tab<Shared, Action, Command> for Fonts {
    fn name(&self) -> &'static str {
        "Fonts"
    }

    fn letter(&self) -> char {
        'n'
    }

    fn node(&self) -> &'static str {
        "fonts"
    }

    fn draw(&self, _shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        let mut rows: Vec<String> = FONTS
            .iter()
            .map(|(name, state)| format!("{name:<17} {state}"))
            .collect();
        // a line that no highlight reaches
        rows.push(String::new());
        rows.push("5 fonts, 3 active".to_owned());
        lines(frame, area, &rows, self.at);
    }

    fn update(&mut self, shared: &Shared, action: Action) -> Vec<Command> {
        match action.row {
            "down.view" => {
                self.at = (self.at + 1).min(FONTS.len() - 1);
                Vec::new()
            }
            "up.view" => {
                self.at = self.at.saturating_sub(1);
                Vec::new()
            }
            "space.view.resources" if self.at == 1 => {
                vec![Command::Change(Change::Say(Message {
                    line: "The built-in font is always active, so nothing changes.".to_owned(),
                    more: None,
                }))]
            }
            "space.view.resources" => vec![Looks::set(shared, "font-active", "toggled")],
            _ => Vec::new(),
        }
    }
}

/// The two tabs, Fonts first.
fn tabs() -> Vec<BoxTab> {
    vec![Box::new(Fonts::default()), Box::new(Looks::new())]
}

/// `machine`'s shared state with its changes saved, as a run that must
/// start from a saved document needs.
fn saved(machine: Machine) -> Shared {
    let mut shared = machine.shared();
    let content = shared.document().content().clone();
    let installed =
        vtium_config_tui::player::scratch::would_install(shared.document().installed(), &content);
    shared.documents.saved(installed);
    assert_eq!(shared.document().count(), 0, "the fixture saved");
    shared
}

/// What a stand-in machine and a stand-in log did, in order.
type Did = Rc<RefCell<Vec<String>>>;

/// A machine that runs nothing: it reads no device, and its Save records
/// the call and says it saved, as a privileged step that went through.
struct StandIn {
    did: Did,
}

impl Loop for StandIn {
    fn devices(&mut self) -> Option<Present> {
        None
    }

    fn ready_font(&mut self, _source: &Path) -> Result<Ready, String> {
        Err("no font here".to_owned())
    }

    fn install_font(&mut self) -> Result<(), String> {
        self.did.borrow_mut().push("install".to_owned());
        Err("no font here".to_owned())
    }

    fn installed(&mut self) -> Result<Installed, String> {
        Ok(Installed::default())
    }

    fn save(&mut self, name: &str, _plan: &Plan) -> Result<Saved, String> {
        self.did.borrow_mut().push(format!("save {name}"));
        Err("the stand-in saves nothing".to_owned())
    }
}

/// A log that records each line, or refuses every line.
struct Logged {
    did: Did,
    refuse: bool,
}

impl Log for Logged {
    fn line(&mut self, line: &str) -> Result<(), String> {
        if self.refuse {
            return Err("the journal is gone".to_owned());
        }
        self.did.borrow_mut().push(format!("log {line}"));
        Ok(())
    }
}

/// Standard output, kept for the test.
#[derive(Clone, Default)]
struct Out(Rc<RefCell<Vec<u8>>>);

impl Write for Out {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Out {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.borrow()).into_owned()
    }
}

/// The macro `name` under tests/macros/.
fn macro_file(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/macros")
        .join(name);
    std::fs::read_to_string(&path).expect("the macro")
}

/// A folder of the test's own, empty, under the target's temporary
/// folder.
fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("player-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the folder");
    dir
}

/// How one play went: the player, what it wrote on standard output, and
/// the run's exit status.
struct Run {
    player: Player,
    out: Out,
    status: u8,
}

/// Plays `text`, the macro `file`, headless on `shared` at `size`, with
/// `options` changed by `set`, on `machine`.
fn play_with(
    file: &str,
    text: &str,
    shared: Shared,
    size: (u16, u16),
    machine: &mut dyn Loop,
    set: impl FnOnce(&mut Options),
) -> Run {
    let m = parse(text).expect("the macro parses");
    let out = Out::default();
    let mut options = Options::new(file, Pace::Off);
    options.out = Some(Box::new(out.clone()));
    set(&mut options);
    let mut player = Player::new(&m, options).expect("the macro is played");
    let mut port = Headless::at(Size::grid(size.0, size.1));
    let scratch = folder(&format!("scratch-{file}-{}x{}", size.0, size.1));
    let played = headless::visible(
        &mut port,
        tabs(),
        shared,
        false,
        machine,
        &mut player,
        &scratch,
    )
    .expect("the loop runs");
    Run {
        player,
        out,
        status: played.status,
    }
}

/// Plays `text` headless on `machine`'s state at `size`, on a machine
/// that runs nothing.
fn play(file: &str, text: &str, machine: Machine, size: (u16, u16)) -> Run {
    let did = Did::default();
    let mut stand_in = StandIn { did };
    play_with(file, text, machine.shared(), size, &mut stand_in, |_| {})
}

/// The report of a failed run.
fn report(run: &Run) -> String {
    match run.player.outcome() {
        Outcome::Failed { report, .. } => report.clone(),
        other => panic!("the run did not fail: {other:?}\n{}", run.out.text()),
    }
}

/// Checks that the run passed.
fn passed(run: &Run, what: &str) {
    assert_eq!(
        run.player.outcome(),
        &Outcome::Passed,
        "{what}:\n{}",
        run.out.text()
    );
    assert_eq!(run.status, 0, "{what}");
}

#[test]
fn the_first_macro_of_adr_1208_plays_to_its_end_on_a_scratch_copy() {
    for machine in Machine::BOTH {
        for size in SIZES {
            let what = format!("{} {}x{}", machine.name(), size.0, size.1);
            let did = Did::default();
            let mut stand_in = StandIn {
                did: Rc::clone(&did),
            };
            let run = play_with(
                "cullet-11.vtmacro",
                &macro_file("cullet-11.vtmacro"),
                machine.shared(),
                size,
                &mut stand_in,
                |_| {},
            );
            passed(&run, &what);
            let out = run.out.text();
            assert!(
                out.contains("cullet-11.vtmacro:22: Expect Saved: ok"),
                "{what}: {out}"
            );
            // the scratch copy's Save ran nothing on the machine
            assert!(did.borrow().is_empty(), "{what}: {:?}", did.borrow());
            let screen = run.player.last_screen().join("\n");
            assert!(screen.contains("scratch copy"), "{what}:\n{screen}");
            assert!(
                screen.contains("On a scratch copy, Save changes nothing.")
                    || screen.contains("Saved"),
                "{what}:\n{screen}"
            );
        }
    }
}

#[test]
fn the_second_macro_of_adr_1208_plays_to_its_end() {
    for machine in Machine::BOTH {
        for size in SIZES {
            let what = format!("{} {}x{}", machine.name(), size.0, size.1);
            let did = Did::default();
            let mut stand_in = StandIn { did };
            let run = play_with(
                "refuse-builtin.vtmacro",
                &macro_file("refuse-builtin.vtmacro"),
                saved(machine),
                size,
                &mut stand_in,
                |_| {},
            );
            passed(&run, &what);
            // Click reached the built-in font with Down, as a person would
            assert_eq!(run.player.path().to_string(), "Alt+N Down Space", "{what}");
        }
    }
}

#[test]
fn a_failing_expect_names_its_line_and_the_keys_that_replay_to_its_screen() {
    let text = "Alt+L\nClick \"console\"\nEnter\nClick \"Size\"\nEnter\nClick \"11 pt\"\nExpect \"12 pt\"\n";
    for machine in Machine::BOTH {
        for size in SIZES {
            let what = format!("{} {}x{}", machine.name(), size.0, size.1);
            let run = play("fail.vtmacro", text, machine, size);
            let report = report(&run);
            let mut lines = report.lines();
            assert_eq!(
                lines.next(),
                Some("fail.vtmacro:7: Expect \"12 pt\": the screen does not show it."),
                "{what}"
            );
            assert_eq!(
                lines.next(),
                Some("Keys from the start: Alt+L Enter Down Down Enter Down Down"),
                "{what}"
            );
            assert_eq!(lines.next(), Some("The screen:"), "{what}");
            assert_eq!(run.status, 1, "{what}");
            // headless play prints the report on standard output
            assert!(run.out.text().contains(&report), "{what}");
            // the keys from the start, played as a macro, reach the same
            // screen
            let replay = run.player.path().to_macro().expect("a macro").to_string();
            let again = play("replay.vtmacro", &replay, machine, size);
            passed(&again, &what);
            assert_eq!(
                again.player.last_screen(),
                run.player.last_screen(),
                "{what}"
            );
        }
    }
}

#[test]
fn a_click_on_a_greyed_item_presses_its_letter_and_the_menu_shows_why() {
    for machine in Machine::BOTH {
        let text =
            "Alt+F\nClick \"Redo\"\nExpect At any.menu.file\nExpect \"Redo · nothing to redo\"\n";
        let run = play("greyed.vtmacro", text, machine, (100, 30));
        passed(&run, machine.name());
        assert_eq!(
            run.player.path().to_string(),
            "Alt+F d",
            "{}",
            machine.name()
        );
    }
}

#[test]
fn a_key_the_tui_cannot_get_from_a_terminal_is_refused_before_the_first_step() {
    // Alt+[ opens a control sequence: the format's parser refuses it
    let err = parse("Alt+N\nAlt+[\n").expect_err("Alt+[ refused");
    assert_eq!(err.line(), 2);
    assert!(err.message().contains("ESC ["), "{}", err.message());
    // Alt+Escape parses, and the TUI's decoder reads its bytes as two keys
    let m = parse("Alt+N\nAlt+Escape\n").expect("it parses");
    let refused = Player::new(&m, Options::new("nav.vtmacro", Pace::Off)).err();
    let refused = refused.expect("the player refuses it");
    assert_eq!(refused.status, 2);
    assert_eq!(
        refused.why,
        "nav.vtmacro:2: Alt+Escape cannot be pressed: a terminal sends it as ESC ESC, which the TUI reads as 2 inputs, Esc then Esc."
    );
    // a character no terminal sends as a key, in a Type
    let m = parse("Type \"a\u{a0}b\"\n").expect("it parses");
    let refused = Player::new(&m, Options::new("t.vtmacro", Pace::Off)).err();
    assert_eq!(refused.map(|r| r.status), Some(2));
}

#[test]
fn a_live_run_needs_set_live_and_the_flag_both() {
    let live = parse("Set Live\nAlt+N\n").expect("it parses");
    let plain = parse("Alt+N\n").expect("it parses");
    let refused = Player::new(&live, Options::new("l.vtmacro", Pace::Off)).err();
    assert_eq!(
        refused.map(|r| (r.status, r.why)),
        Some((
            1,
            "vtium-config: l.vtmacro asks for a live run with Set Live, but --live is missing."
                .to_owned()
        ))
    );
    let mut flagged = Options::new("p.vtmacro", Pace::Off);
    flagged.live = true;
    let refused = Player::new(&plain, flagged).err();
    assert_eq!(
        refused.map(|r| (r.status, r.why)),
        Some((
            1,
            "vtium-config: --live asks for a live run, but p.vtmacro does not start with Set Live."
                .to_owned()
        ))
    );
}

#[test]
fn a_live_run_refuses_a_document_with_changes_not_saved() {
    let did = Did::default();
    let mut stand_in = StandIn {
        did: Rc::clone(&did),
    };
    // the screen map's machine holds one change not saved
    let run = play_with(
        "cullet-11-live.vtmacro",
        &macro_file("cullet-11-live.vtmacro"),
        Machine::Map.shared(),
        (100, 30),
        &mut stand_in,
        |o| {
            o.live = true;
            o.log = Some(Box::new(Logged {
                did: Rc::clone(&did),
                refuse: false,
            }));
        },
    );
    assert_eq!(run.status, 1);
    assert_eq!(
        run.out.text(),
        "vtium-config: the configuration has changes that are not saved: save or revert them before a live run.\n"
    );
    assert!(did.borrow().is_empty(), "{:?}", did.borrow());
}

#[test]
fn a_live_run_logs_each_real_command_before_it_runs_it() {
    for machine in Machine::BOTH {
        let did = Did::default();
        let mut stand_in = StandIn {
            did: Rc::clone(&did),
        };
        let debug = folder(&format!("live-debug-{}", machine.name())).join("run");
        let run = play_with(
            "cullet-11-live.vtmacro",
            &macro_file("cullet-11-live.vtmacro"),
            saved(machine),
            (100, 30),
            &mut stand_in,
            |o| {
                o.live = true;
                o.debug = Some(Debug::create(&debug).expect("the folder"));
                o.log = Some(Box::new(Logged {
                    did: Rc::clone(&did),
                    refuse: false,
                }));
            },
        );
        passed(&run, machine.name());
        let name = machine.shared().document().name().to_owned();
        let line = format!(
            "live macro cullet-11-live.vtmacro line 13 runs: sudo vtium-config apply, saving {name} to /etc/vtium/vtium.conf"
        );
        assert_eq!(
            *did.borrow(),
            vec![format!("log {line}"), format!("save {name}")],
            "{}",
            machine.name()
        );
        assert_eq!(
            std::fs::read_to_string(debug.join("live.log")).expect("live.log"),
            format!("{line}\n")
        );
        let screen = run.player.last_screen().join("\n");
        assert!(screen.contains("LIVE"), "{screen}");
    }
}

#[test]
fn a_real_command_that_cannot_be_logged_does_not_run_and_fails_its_step() {
    let did = Did::default();
    let mut stand_in = StandIn {
        did: Rc::clone(&did),
    };
    let run = play_with(
        "cullet-11-live.vtmacro",
        &macro_file("cullet-11-live.vtmacro"),
        saved(Machine::Map),
        (100, 30),
        &mut stand_in,
        |o| {
            o.live = true;
            o.log = Some(Box::new(Logged {
                did: Rc::clone(&did),
                refuse: true,
            }));
        },
    );
    let report = report(&run);
    assert!(
        report.starts_with("cullet-11-live.vtmacro:13: Enter: vtium-config cannot log the command, so it does not run it: the journal is gone"),
        "{report}"
    );
    assert!(did.borrow().is_empty(), "{:?}", did.borrow());
}

#[test]
fn debug_mode_writes_each_step_each_screenshot_and_the_failure() {
    let dir = folder("debug").join("run");
    let text = "Alt+N\nScreenshot fonts\nDown\n# a comment\nExpect \"nothing here\"\n";
    let did = Did::default();
    let mut stand_in = StandIn { did };
    let run = play_with(
        "d.vtmacro",
        text,
        Machine::Map.shared(),
        (100, 30),
        &mut stand_in,
        |o| o.debug = Some(Debug::create(&dir).expect("the folder")),
    );
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .expect("the folder")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            "failure.txt",
            "fonts.txt",
            "step-0001.txt",
            "step-0002.txt",
            "step-0003.txt"
        ]
    );
    let step = std::fs::read_to_string(dir.join("step-0001.txt")).expect("a step");
    assert_eq!(step.lines().count(), 30);
    assert!(step.contains("terminus"), "{step}");
    let failure = std::fs::read_to_string(dir.join("failure.txt")).expect("the failure");
    assert_eq!(failure, format!("{}\n", report(&run)));
    // a folder that exists is refused, so two runs never mix
    let again = Debug::create(&dir).expect_err("the folder exists");
    assert!(again.contains("exists already"), "{again}");
}

#[test]
fn headless_play_prints_each_step_s_line_and_its_result() {
    let run = play(
        "h.vtmacro",
        "# open Fonts\nAlt+N\nExpect At fonts\nSleep 2s\nDown 2\nExpect Unsaved\n",
        Machine::Map,
        (100, 30),
    );
    passed(&run, "map");
    assert_eq!(
        run.out.text(),
        "h.vtmacro:2: Alt+N: ok\nh.vtmacro:3: Expect At fonts: ok\nh.vtmacro:4: Sleep 2s: ok\nh.vtmacro:5: Down 2: ok\nh.vtmacro:6: Expect Unsaved: ok\n"
    );
}

#[test]
fn headless_play_is_100_by_30_on_a_terminal_that_is_no_vtium_console() {
    let m = parse("Expect At fonts\n").expect("it parses");
    let out = Out::default();
    let mut options = Options::new("s.vtmacro", Pace::Off);
    options.out = Some(Box::new(out.clone()));
    let mut player = Player::new(&m, options).expect("a player");
    let mut stand_in = StandIn {
        did: Did::default(),
    };
    let scratch = folder("headless-size");
    let played = headless::headless(
        tabs(),
        Machine::Map.shared(),
        &mut stand_in,
        &mut player,
        &scratch,
    )
    .expect("it runs");
    assert_eq!(played.status, 0, "{}", out.text());
    // the screen map's change waits unsaved, but the scratch copy goes
    // with the run, so no line says it waits
    assert_eq!(played.line, None);
    let screen = player.last_screen();
    assert_eq!(screen.len(), 30);
    assert!(screen.iter().all(|r| common::unicode_width(r) == 100));
    // the scratch copy's folder goes when the run ends
    assert_eq!(std::fs::read_dir(&scratch).expect("the folder").count(), 0);
}

#[test]
fn a_step_after_the_tui_exits_fails_unless_it_expects_the_exit() {
    let run = play("q.vtmacro", "q\nExpect Exited 0\n", Machine::Map, (100, 30));
    passed(&run, "q then Expect Exited 0");
    let run = play("q.vtmacro", "q\nAlt+N\n", Machine::Map, (100, 30));
    assert!(
        report(&run).starts_with("q.vtmacro:2: Alt+N: the TUI has exited, with status 0."),
        "{}",
        report(&run)
    );
    assert_eq!(run.status, 1);
}

#[test]
fn click_mouse_clicks_the_label_s_cell() {
    let run = play(
        "m.vtmacro",
        "Click Mouse \"Looks\"\nExpect At looks\nClick Mouse \"File\"\nExpect At any.menu.file\n",
        Machine::Map,
        (80, 25),
    );
    passed(&run, "map");
    // a click is no key, so the path holds none
    assert_eq!(run.player.path().to_string(), "");
}

#[test]
fn a_click_reaches_a_tab_a_menu_a_button_and_a_submenu_s_line_by_their_keys() {
    for machine in Machine::BOTH {
        for size in SIZES {
            let what = format!("{} {}x{}", machine.name(), size.0, size.1);
            // a configuration of the Open submenu, which has no letter
            let other = if machine == Machine::Map {
                "spare"
            } else {
                "lab-05"
            };
            let text = format!(
                "Click \"Looks\"\nExpect At looks\nClick \"File\"\nExpect At any.menu.file\nEscape\nClick \"Save\"\nExpect At any.save\nEscape\nAlt+F\nClick \"Open\"\nClick \"{other}\"\nExpect \"{other} · scratch copy\"\n"
            );
            let run = play("k.vtmacro", &text, machine, size);
            passed(&run, &what);
            let path = run.player.path().to_string();
            assert!(
                path.starts_with("Alt+l Alt+f Escape Ctrl+S Escape Alt+F o"),
                "{what}: {path}"
            );
            assert!(path.ends_with("Enter"), "{what}: {path}");
        }
    }
}

#[test]
fn two_lines_that_begin_with_the_label_fail_the_click_and_name_both() {
    let run = play(
        "two.vtmacro",
        "Click \"cullet-mono\"\n",
        Machine::Map,
        (100, 30),
    );
    assert!(
        report(&run).starts_with("two.vtmacro:1: Click \"cullet-mono\": the viewer shows \"cullet-mono\" twice, at row 4 column 2 and at row 6 column 2."),
        "{}",
        report(&run)
    );
}

#[test]
fn a_highlight_that_stops_before_the_label_fails_the_click() {
    let run = play(
        "stop.vtmacro",
        "Alt+N\nClick \"5 fonts\"\n",
        Machine::Map,
        (100, 30),
    );
    let report = report(&run);
    let mut lines = report.lines();
    assert_eq!(
        lines.next(),
        Some(
            "stop.vtmacro:2: Click \"5 fonts\": the highlight stops before it reaches \"5 fonts\"."
        )
    );
    // four moves to the last font, and the one that did not move
    assert_eq!(
        lines.next(),
        Some("Keys from the start: Alt+N Down Down Down Down Down")
    );
}

/// Plays the stream `text` headless, through the player's reader of
/// standard input, on the thread of its own or as headless play asks.
fn play_stream(text: &'static str, threaded: bool) -> Run {
    let out = Out::default();
    let mut options = Options::new("-", Pace::Off);
    options.out = Some(Box::new(out.clone()));
    let reader = std::io::Cursor::new(text.as_bytes());
    let mut player = if threaded {
        Player::stream(reader, options)
    } else {
        Player::blocking(reader, options)
    };
    let mut stand_in = StandIn {
        did: Did::default(),
    };
    let scratch = folder(&format!("stream-{threaded}"));
    let played = headless::headless(
        tabs(),
        Machine::Map.shared(),
        &mut stand_in,
        &mut player,
        &scratch,
    )
    .expect("it runs");
    Run {
        player,
        out,
        status: played.status,
    }
}

#[test]
fn a_stream_is_parsed_line_by_line_and_a_bad_line_ends_the_run_there() {
    for threaded in [true, false] {
        let run = play_stream("Alt+N\nExpect At fonts\n", threaded);
        passed(&run, "a good stream");
        assert_eq!(run.out.text(), "-:1: Alt+N: ok\n-:2: Expect At fonts: ok\n");
        let run = play_stream("Alt+N\nClik \"x\"\nAlt+L\n", threaded);
        assert_eq!(run.status, 2);
        let out = run.out.text();
        assert!(
            out.starts_with("-:1: Alt+N: ok\n-:2: Clik is not a step."),
            "{out}"
        );
        // the key the decoder cannot make ends a stream at its line too, as
        // a line that does not parse
        let run = play_stream("Alt+N\nAlt+Escape\nAlt+L\n", threaded);
        assert_eq!(run.status, 2, "{}", run.out.text());
        assert_eq!(
            run.out.text(),
            "-:1: Alt+N: ok\n-:2: Alt+Escape cannot be pressed: a terminal sends it as ESC ESC, which the TUI reads as 2 inputs, Esc then Esc.\n"
        );
        // Set Live in a stream without the flag
        let run = play_stream("Set Live\nAlt+N\n", threaded);
        assert_eq!(run.status, 1);
        assert_eq!(
            run.out.text(),
            "vtium-config: - asks for a live run with Set Live, but --live is missing.\n"
        );
    }
}

#[test]
fn a_click_on_a_label_shown_nowhere_fails_and_says_so() {
    let run = play("n.vtmacro", "Click \"nowhere\"\n", Machine::Map, (100, 30));
    assert!(
        report(&run).starts_with(
            "n.vtmacro:1: Click \"nowhere\": the screen shows no \"nowhere\" to click."
        ),
        "{}",
        report(&run)
    );
}

/// A port that plays `wakes` one batch at a time, then answers the
/// player's pause, then the update's wait, then hangs up.
struct Paced {
    wakes: std::collections::VecDeque<Vec<Wake>>,
    armed: bool,
    step: bool,
}

impl Port for Paced {
    fn size(&mut self) -> Size {
        Size::grid(100, 30)
    }

    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<()> {
        Ok(())
    }

    fn arm(&mut self, _after: std::time::Duration) {
        self.armed = true;
    }

    fn wait(&mut self) -> std::io::Result<Vec<Wake>> {
        if std::mem::take(&mut self.armed) {
            return Ok(vec![Wake::Timeout]);
        }
        if let Some(w) = self.wakes.pop_front() {
            return Ok(w);
        }
        if std::mem::take(&mut self.step) {
            return Ok(vec![Wake::Step]);
        }
        Ok(vec![Wake::Signal(1)])
    }

    fn step_aside(
        &mut self,
        aside: &mut dyn FnMut() -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        aside()
    }

    fn arm_step(&mut self, _after: std::time::Duration) -> bool {
        self.step = true;
        true
    }
}

#[test]
fn a_key_from_the_terminal_stops_the_player_and_the_tui_does_not_act_on_it() {
    let m = parse("Alt+L\nDown\nDown\n").expect("it parses");
    let mut player = Player::new(&m, Options::new("s.vtmacro", Pace::Visible)).expect("a player");
    // the first key plays; during its pause, q comes from the terminal
    let mut port = Paced {
        wakes: vec![vec![Wake::Bytes(b"q".to_vec())]].into(),
        armed: false,
        step: false,
    };
    let mut app = App::new(tabs(), saved(Machine::Map), Size::grid(100, 30), false);
    player.start(&mut app).expect("it starts");
    let mut stand_in = StandIn {
        did: Did::default(),
    };
    let left = port::drive_with(&mut port, &mut stand_in, app, Some(&mut player)).expect("ran");
    assert_eq!(player.outcome(), &Outcome::Stopped { line: 1 });
    assert_eq!(player.path().to_string(), "Alt+L");
    // q did not quit: the TUI left on the hangup that followed
    assert_eq!(left.status, 1, "a stopped run exits 1");
}

/// The App on `machine` at `size` with the message `said` and the macro's
/// mode `mode`, as the player leaves it.
fn shown(machine: Machine, size: (u16, u16), color: bool, mode: Mode, said: &str) -> App {
    let mut app = App::new(tabs(), machine.shared(), Size::grid(size.0, size.1), color);
    app.set_macro_mode(Some(mode));
    app.event(Event::Said(Message {
        line: said.to_owned(),
        more: Some("the report".to_owned()),
    }));
    app
}

/// The snapshot `name` of the player's screen in `mode` with `said`.
fn screens(name: &str, mode: Mode, said: &str, must: &str) {
    let mut out = String::new();
    for machine in Machine::BOTH {
        for size in SIZES {
            let what = format!("{name} {} {}x{}", machine.name(), size.0, size.1);
            let color = draw(&shown(machine, size, true, mode, said));
            let mono = draw(&shown(machine, size, false, mode, said));
            rows_fit(&color, &what);
            marks_survive_without_color(&color, &mono, &what);
            let text = common::rows(&color).join("\n");
            assert!(text.contains(must), "{what}:\n{text}");
            out.push_str(&format!("== {what}\n"));
            out.push_str(&snapshot_text(&mono));
        }
    }
    snapshot(name, &out);
}

#[test]
fn the_top_row_says_scratch_copy_and_the_message_row_where_the_macro_ended() {
    screens(
        "player-scratch",
        Mode::ScratchCopy,
        &vtium_config_tui::player::words::ended("cullet-11.vtmacro", 22),
        "scratch copy",
    );
}

#[test]
fn the_top_row_says_live_during_a_live_run() {
    screens(
        "player-live",
        Mode::Live,
        &vtium_config_tui::player::words::stopped(13, "Enter"),
        "LIVE",
    );
}

#[test]
fn the_message_row_names_a_failure_and_f1_opens_its_report() {
    screens(
        "player-failure",
        Mode::ScratchCopy,
        &vtium_config_tui::player::words::failed(
            "fail.vtmacro:7: Expect \"12 pt\": the screen does not show it.",
        ),
        "fail.vtmacro:7",
    );
}
