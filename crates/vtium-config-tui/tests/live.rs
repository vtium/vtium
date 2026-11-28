//! The live console's tests: the App driven over the screen map's machine
//! and the large machine, with two stand-in tabs of this file's own that
//! try a font and a look on `t`, and a stand-in terminal that answers as a
//! vtium console, a vtium console with `allow-font-ops` off, or another
//! terminal. A key, a reply or the end of a wait goes in, and the test
//! reads the console's state, the document, the screen and the effects
//! that come out.
//!
//! The screens are kept as snapshots under `tests/snapshots/`, each with
//! a grid of the attributes every cell keeps without color.
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-tui --test live`
//! writes them again.
//!
//! References:
//! - ADR-1206 D2: replies in order to whoever asked, DA1 at the end of
//!   every batch, and Esc while replies are due.
//! - ADR-1201 D4: the questions at start, the live trial, and the font
//!   given back on leaving.
//! - ADR-1403 D11, "The menus": the Actions menu's items and their
//!   reasons.

#![expect(clippy::expect_used, reason = "a test fails by panicking")]

mod common;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use common::{Machine, SIZES, click, draw, keys, locate, press, text};
use ratatui_core::layout::Rect;
use ratatui_core::terminal::Frame;
use vtium_config_core::apply::Placement;
use vtium_config_core::flowmodel::Present;
use vtium_config_core::ui::{Action, Change, Command, Effect, Live, Op, Shared};
use vtium_config_tui::BoxTab;
use vtium_config_tui::app::{App, Event, Size};
use vtium_config_tui::live::machine::{self, Ready};
use vtium_config_tui::live::{WAIT, trial};
use vtium_config_tui::port::Wake;
use vtium_config_widgets::keys::{Decoder, Event as Input, Key, Reply};
use vtium_config_widgets::tab::Tab;

const MAP: (u16, u16) = (100, 30);

/// The font a vtium console in these tests draws at start.
const START: &str = "unifont";

/// The font file of the person's own that the stand-in Fonts tab
/// highlights where a test says so.
const OWN: &str = "/home/user/.local/share/fonts/Mine-Regular.ttf";

/// The stand-in Fonts tab: `t` tries Terminus, and the tab highlights a
/// font file of the person's own where `own` is set.
struct Fonts {
    own: Rc<RefCell<bool>>,
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
        let line = if *self.own.borrow() {
            "▸ Mine Regular       Mine-Regular.ttf   your folder"
        } else {
            "▸ Terminus           ter-u16n.psf.gz"
        };
        frame.buffer_mut().set_stringn(
            area.x,
            area.y,
            line,
            usize::from(area.width),
            ratatui_core::style::Style::default(),
        );
    }
    fn update(&mut self, _shared: &Shared, action: Action) -> Vec<Command> {
        match action.row {
            "t.view.fonts" => vec![Command::Change(Change::Try("Terminus".to_owned()))],
            _ => Vec::new(),
        }
    }
    fn own_font(&self, _shared: &Shared) -> Option<PathBuf> {
        self.own.borrow().then(|| PathBuf::from(OWN))
    }
}

/// The stand-in Looks tab: `t` tries the document's first look, big in
/// the screen map's document, whose font set is proof.
struct Looks;

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
        frame.buffer_mut().set_stringn(
            area.x,
            area.y,
            "▸ big        font set proof · 11 pt",
            usize::from(area.width),
            ratatui_core::style::Style::default(),
        );
    }
    fn update(&mut self, shared: &Shared, action: Action) -> Vec<Command> {
        let text = shared.document().text();
        let flow = vtium_config_core::flowmodel::build(text, &shared.present, None);
        match (action.row, flow.looks.first()) {
            ("t.view.looks", Some(look)) => {
                vec![Command::Change(Change::TryLook(look.name.clone()))]
            }
            _ => Vec::new(),
        }
    }
}

/// The App on `machine` at `size` with the two stand-in tabs, and the
/// switch that makes the Fonts tab highlight a font of the person's own.
fn app_in(machine: Machine, (cols, rows): (u16, u16), color: bool) -> (App, Rc<RefCell<bool>>) {
    let own = Rc::new(RefCell::new(false));
    let tabs: Vec<BoxTab> = vec![
        Box::new(Fonts {
            own: Rc::clone(&own),
        }),
        Box::new(Looks),
    ];
    let app = App::new(tabs, machine.shared(), Size::grid(cols, rows), color);
    (app, own)
}

fn app(machine: Machine, size: (u16, u16)) -> App {
    app_in(machine, size, true).0
}

/// What the terminal answers to the questions at start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Terminal {
    /// A vtium console that draws [`START`].
    Vtium,
    /// A vtium console with `allow-font-ops` off, which leaves OSC 50's
    /// query unanswered.
    OpsOff,
    /// Another terminal, which answers DA1 alone.
    Other,
}

/// Starts `app` and answers its questions at start as `terminal` does.
fn started(app: &mut App, terminal: Terminal) {
    let size = app.size();
    let out = app.event(Event::Resize(size));
    assert!(
        matches!(out.as_slice(), [Effect::Write(_), Effect::Wait(w)] if *w == WAIT),
        "{out:?}"
    );
    if terminal != Terminal::Other {
        reply(app, Reply::Version("vtium(0.1.0-alpha.12)".into()));
    }
    if terminal == Terminal::Vtium {
        reply(app, Reply::Font(START.into()));
    }
    reply(app, Reply::Attributes);
}

fn reply(app: &mut App, reply: Reply) -> Vec<Effect> {
    app.event(Event::Reply(reply))
}

fn live(machine: Machine, size: (u16, u16)) -> App {
    let mut app = app(machine, size);
    started(&mut app, Terminal::Vtium);
    app
}

fn message(app: &App) -> Option<String> {
    app.shared().message.as_ref().map(|m| m.line.clone())
}

/// The bytes of a switch to `font` and its query, DA1 last.
fn switch(font: &str) -> Vec<u8> {
    format!("\x1b]50;{font}\x1b\\\x1b]50;?\x1b\\\x1b[c").into_bytes()
}

// The questions at start and the replies.

#[test]
fn the_answers_at_start_say_what_the_terminal_is() {
    for (terminal, want) in [
        (Terminal::Vtium, Live::On),
        (Terminal::OpsOff, Live::OpsOff),
        (Terminal::Other, Live::NotVtium),
    ] {
        let mut app = app(Machine::Map, MAP);
        assert_eq!(app.shared().console.live, Live::Asking);
        started(&mut app, terminal);
        assert_eq!(app.shared().console.live, want, "{terminal:?}");
        assert!(!app.awaiting(), "{terminal:?}: DA1 settled the batch");
    }
    // a terminal that answers nothing is settled at the end of the wait
    let mut app = app(Machine::Map, MAP);
    app.event(Event::Resize(app.size()));
    assert!(app.awaiting());
    assert!(app.event(Event::Timeout).is_empty());
    assert_eq!(app.shared().console.live, Live::NotVtium);
    assert!(!app.awaiting());
}

#[test]
fn replies_reach_their_askers_in_order() {
    // the trial's switch and the switch back on leaving are both
    // outstanding: each reply goes to the asker at the front
    let mut app = live(Machine::Map, MAP);
    assert_eq!(
        press(&mut app, Key::char('t')),
        [Effect::Write(switch("Terminus")), Effect::Wait(trial::TICK)]
    );
    assert_eq!(
        press(&mut app, Key::ctrl('c')),
        [Effect::Write(switch(START)), Effect::Wait(WAIT)],
        "Ctrl+C gives the start font back"
    );
    assert!(reply(&mut app, Reply::Font("Terminus".into())).is_empty());
    assert_eq!(app.shared().console.in_flight, None, "the switch heard it");
    assert!(
        !app.shared().console.refused.contains("Terminus"),
        "the console took Terminus"
    );
    assert!(reply(&mut app, Reply::Attributes).is_empty());
    assert_eq!(app.shared().console.shown_font.as_deref(), Some("Terminus"));
    assert!(reply(&mut app, Reply::Font(START.into())).is_empty());
    assert_eq!(app.shared().console.shown_font.as_deref(), Some(START));
    assert_eq!(reply(&mut app, Reply::Attributes), [Effect::Exit(0)]);
}

#[test]
fn a_reply_lost_before_da1_is_heard_as_unanswered() {
    // the start's font reply is lost: a vtium console with allow-font-ops
    // off
    let mut app = app(Machine::Map, MAP);
    started(&mut app, Terminal::OpsOff);
    assert_eq!(app.shared().console.live, Live::OpsOff);
    // the trial's font reply is lost: DA1's reply settles the switch, and
    // the console did not take the font
    let mut app = live(Machine::Map, MAP);
    press(&mut app, Key::char('t'));
    assert!(app.awaiting());
    reply(&mut app, Reply::Attributes);
    assert!(!app.awaiting());
    assert_eq!(app.shared().console.in_flight, None);
    assert!(app.shared().console.refused.contains("Terminus"));
    assert!(app.live().trial.is_none(), "the trial ended");
    assert_eq!(
        message(&app).as_deref(),
        Some("The console refused Terminus: it takes only trusted system fonts. [F1] says more.")
    );
}

#[test]
fn a_reply_that_answers_nothing_due_is_dropped() {
    let mut app = live(Machine::Map, MAP);
    assert!(reply(&mut app, Reply::Font("tiny5".into())).is_empty());
    assert_eq!(app.shared().console.shown_font.as_deref(), Some(START));
    assert!(!app.awaiting());
}

/// Decodes `bytes` and hands each input to `app` as the loop does, asking
/// before each whether a reply is due.
fn feed(app: &mut App, decoder: &mut Decoder, bytes: &[u8]) {
    decoder.push(bytes);
    while let Some(input) = decoder.next(app.awaiting()) {
        let event = match input {
            Input::Key(key) => Event::Key(key),
            Input::Mouse(mouse) => Event::Mouse(mouse),
            Input::Reply(reply) => Event::Reply(reply),
        };
        app.event(event);
    }
}

#[test]
fn esc_pressed_while_replies_arrive_stays_the_esc_key() {
    let mut app = app(Machine::Map, MAP);
    let mut decoder = Decoder::new();
    app.event(Event::Resize(app.size()));
    // a lone ESC at the end of a read waits while the answers are due
    feed(&mut app, &mut decoder, b"\x1bP>|vtium(0.1.0)");
    feed(&mut app, &mut decoder, b"\x1b\\");
    assert_eq!(
        app.shared().console.live,
        Live::Asking,
        "the version came, the batch is open"
    );
    // the person presses Esc as the font reply arrives: ESC ESC begins
    // with the Esc key, and the reply after it reaches its asker whole
    keys(&mut app, &["Enter"]);
    assert!(app.nav().levels(0).is_empty(), "the stand-in opens nothing");
    feed(&mut app, &mut decoder, b"\x1b");
    assert!(app.awaiting());
    feed(&mut app, &mut decoder, b"\x1b]50;unifont\x1b\\\x1b[?62;22c");
    assert_eq!(app.shared().console.live, Live::On);
    assert_eq!(app.shared().console.start_font.as_deref(), Some(START));
    assert!(!app.awaiting());
    // with nothing due, a lone ESC is the Esc key at once, which closes
    // an open menu
    keys(&mut app, &["Alt-F"]);
    assert_eq!(app.node(), "any.menu.file");
    feed(&mut app, &mut decoder, b"\x1b");
    assert_eq!(app.node(), "fonts", "Esc closed the menu at once");
}

// The live trial.

#[test]
fn t_tries_the_font_for_ten_seconds_and_puts_it_back() {
    let mut app = live(Machine::Map, MAP);
    let text_before = app.shared().document().text().to_owned();
    press(&mut app, Key::char('t'));
    assert_eq!(app.node(), "any.trial");
    assert!(text(&app).contains("Trying Terminus on this console: 10 s left."));
    assert_eq!(
        message(&app),
        None,
        "the sheet counts down, not the message"
    );
    reply(&mut app, Reply::Font("Terminus".into()));
    reply(&mut app, Reply::Attributes);
    for left in (1..10).rev() {
        assert_eq!(app.event(Event::Timeout), [Effect::Wait(trial::TICK)]);
        let screen = text(&app);
        assert!(
            screen.contains(&format!("Trying Terminus on this console: {left} s left.")),
            "{screen}"
        );
    }
    assert_eq!(app.event(Event::Timeout), [Effect::Write(switch(START))]);
    assert!(app.live().trial.is_none());
    assert_eq!(app.node(), "fonts");
    assert_eq!(
        message(&app).as_deref(),
        Some("The console shows unifont again.")
    );
    assert_eq!(app.shared().document().text(), text_before);
    assert_eq!(app.shared().document().count(), 1, "the change it had");
}

#[test]
fn any_other_key_or_a_click_puts_the_font_back_and_does_nothing_else() {
    for put_back in ["q", "Esc", "Tab", "Space", "Alt-F", "click"] {
        let mut app = live(Machine::Map, MAP);
        press(&mut app, Key::char('t'));
        reply(&mut app, Reply::Font("Terminus".into()));
        reply(&mut app, Reply::Attributes);
        let out = if put_back == "click" {
            click(&mut app, 5, 1)
        } else {
            keys(&mut app, &[put_back])
        };
        assert_eq!(out, [Effect::Write(switch(START))], "{put_back}");
        assert_eq!(app.node(), "fonts", "{put_back} did nothing else");
        assert_eq!(app.leaving(), None, "{put_back}");
        assert_eq!(app.nav().tab(), 0, "{put_back}");
        assert_eq!(app.shared().document().count(), 1, "{put_back}");
    }
}

#[test]
fn a_put_back_asked_while_the_switch_is_in_flight_goes_when_it_is_answered() {
    let mut app = live(Machine::Map, MAP);
    press(&mut app, Key::char('t'));
    assert!(
        keys(&mut app, &["Esc"]).is_empty(),
        "the switch is in flight"
    );
    assert_eq!(
        reply(&mut app, Reply::Font("Terminus".into())),
        [Effect::Write(switch(START))]
    );
}

#[test]
fn k_keeps_the_font_for_the_session_and_leaving_gives_the_start_font_back() {
    let mut app = live(Machine::Map, MAP);
    let text_before = app.shared().document().text().to_owned();
    press(&mut app, Key::char('t'));
    reply(&mut app, Reply::Font("Terminus".into()));
    reply(&mut app, Reply::Attributes);
    assert!(press(&mut app, Key::char('k')).is_empty());
    assert!(app.live().trial.is_none());
    assert_eq!(
        message(&app).as_deref(),
        Some("The console keeps Terminus until you quit. To keep it after, choose it and save.")
    );
    assert_eq!(app.event(Event::Timeout), [], "the countdown ended");
    assert_eq!(app.shared().console.shown_font.as_deref(), Some("Terminus"));
    assert_eq!(app.shared().document().text(), text_before);
    // q gives the start font back, then waits for the console's answer
    assert_eq!(
        press(&mut app, Key::char('q')),
        [Effect::Write(switch(START)), Effect::Wait(WAIT)]
    );
    assert_eq!(app.leaving(), Some(0));
    assert_eq!(
        message(&app).as_deref(),
        Some("Giving the console its font back…")
    );
    // keys no longer count, and the answer lets it leave
    assert!(press(&mut app, Key::char('t')).is_empty());
    assert!(reply(&mut app, Reply::Font(START.into())).is_empty());
    assert_eq!(reply(&mut app, Reply::Attributes), [Effect::Exit(0)]);
    assert_eq!(app.shared().console.shown_font.as_deref(), Some(START));
}

#[test]
fn leaving_waits_at_most_a_second_for_a_console_that_does_not_answer() {
    let mut app = live(Machine::Map, MAP);
    press(&mut app, Key::char('t'));
    press(&mut app, Key::char('k'));
    press(&mut app, Key::char('q'));
    assert_eq!(app.event(Event::Timeout), [Effect::Exit(0)]);
    // a signal while it waits leaves at once
    let mut app = live(Machine::Map, MAP);
    press(&mut app, Key::char('t'));
    assert_eq!(
        app.event(Event::Signal(15)),
        [Effect::Write(switch(START)), Effect::Wait(WAIT)]
    );
    assert_eq!(app.event(Event::Signal(1)), [Effect::Exit(143)]);
}

#[test]
fn leaving_with_the_start_font_shown_sends_nothing() {
    let mut app = live(Machine::Map, MAP);
    assert_eq!(press(&mut app, Key::char('q')), [Effect::Exit(0)]);
    // a trial put back and answered leaves nothing to give back
    let mut app = live(Machine::Map, MAP);
    press(&mut app, Key::char('t'));
    reply(&mut app, Reply::Font("Terminus".into()));
    reply(&mut app, Reply::Attributes);
    keys(&mut app, &["Esc"]);
    reply(&mut app, Reply::Font(START.into()));
    reply(&mut app, Reply::Attributes);
    assert_eq!(press(&mut app, Key::char('q')), [Effect::Exit(0)]);
}

#[test]
fn t_on_a_look_tries_its_font_set() {
    let mut app = live(Machine::Map, MAP);
    keys(&mut app, &["Alt-L"]);
    assert_eq!(
        press(&mut app, Key::char('t')),
        [Effect::Write(switch("proof")), Effect::Wait(trial::TICK)]
    );
    assert!(text(&app).contains("Trying big (proof) on this console: 10 s left."));
}

#[test]
fn t_says_why_where_the_trial_cannot_run_and_sends_nothing() {
    for (terminal, line) in [
        (
            Terminal::Other,
            "Try live is off: this terminal is not a vtium console. [F1] says more.",
        ),
        (
            Terminal::OpsOff,
            "Try live is off: allow-font-ops is off. Settings › Running vtium turns it on.",
        ),
    ] {
        let mut app = app(Machine::Map, MAP);
        started(&mut app, terminal);
        assert!(press(&mut app, Key::char('t')).is_empty(), "{terminal:?}");
        assert_eq!(message(&app).as_deref(), Some(line));
        assert!(app.live().trial.is_none());
        assert!(
            app.shared()
                .message
                .as_ref()
                .is_some_and(|m| m.more.is_some())
        );
    }
    // a font the console refused before is not sent again
    let mut app = live(Machine::Map, MAP);
    press(&mut app, Key::char('t'));
    reply(&mut app, Reply::Font(START.into()));
    reply(&mut app, Reply::Attributes);
    assert!(press(&mut app, Key::char('t')).is_empty());
    assert!(message(&app).is_some_and(|m| m.starts_with("The console refused Terminus")));
}

// The Actions menu.

#[test]
fn the_actions_menu_greys_each_item_with_its_reason() {
    for (terminal, reason) in [
        (Terminal::Other, Some("not a vtium console")),
        (Terminal::OpsOff, Some("allow-font-ops is off")),
        (Terminal::Vtium, None),
    ] {
        let mut app = app(Machine::Map, MAP);
        started(&mut app, terminal);
        keys(&mut app, &["Alt-A"]);
        let screen = text(&app);
        match reason {
            Some(why) => assert!(
                screen.contains(&format!("Try live · {why}")),
                "{terminal:?}:\n{screen}"
            ),
            None => assert!(!screen.contains("Try live ·"), "{screen}"),
        }
        assert!(screen.contains("Install as a system font… · on a font of your own"));
    }
}

#[test]
fn actions_try_live_tries_what_the_screen_highlights() {
    let mut app = live(Machine::Map, MAP);
    assert_eq!(
        keys(&mut app, &["Alt-A", "t"]),
        [Effect::Write(switch("Terminus")), Effect::Wait(trial::TICK)]
    );
    assert_eq!(app.node(), "any.trial");
}

#[test]
fn actions_read_devices_again_says_what_came_and_what_went() {
    let mut app = live(Machine::Map, MAP);
    let old = app.shared().present.clone();
    assert_eq!(keys(&mut app, &["Alt-A", "r"]), [Effect::Desk(Op::Devices)]);
    assert!(app.event(Event::Devices(Some(old.clone()))).is_empty());
    assert_eq!(
        message(&app).as_deref(),
        Some("Read devices again: nothing came or went.")
    );
    let mut fewer = old.clone();
    let gone = fewer.devices.pop().expect("a device");
    app.event(Event::Devices(Some(fewer)));
    let name = gone
        .connector
        .clone()
        .or(gone.name.clone())
        .unwrap_or_else(|| "a device".to_owned());
    assert_eq!(
        message(&app),
        Some(format!("Read devices again: disconnected {name}."))
    );
    app.event(Event::Devices(Some(old)));
    assert_eq!(
        message(&app),
        Some(format!("Read devices again: connected {name}."))
    );
    app.event(Event::Devices(None));
    assert_eq!(
        message(&app).as_deref(),
        Some("Read devices again: vtium-config cannot read the devices here.")
    );
}

/// A machine that stands in for this one: it lists `devices`, readies any
/// font, and counts the reads and the installs it ran.
#[derive(Default)]
struct StandIn {
    devices: Present,
    reads: usize,
    installs: usize,
}

impl machine::Machine for StandIn {
    fn devices(&mut self) -> Option<Present> {
        self.reads += 1;
        Some(self.devices.clone())
    }
    fn ready_font(&mut self, source: &Path) -> Result<Ready, String> {
        Ok(Ready {
            source: source.to_path_buf(),
            target: PathBuf::from("/usr/local/share/fonts/Mine-Regular.ttf"),
            placement: Placement::New,
            command: "sudo /usr/bin/vtium-config apply /home/user/.config/vtium/drafts/d1"
                .to_owned(),
        })
    }
    fn install_font(&mut self) -> Result<(), String> {
        self.installs += 1;
        Ok(())
    }
}

#[test]
fn actions_install_as_a_system_font_asks_with_the_command_then_installs_through_the_step_aside() {
    let (mut app, own) = app_in(Machine::Map, MAP, true);
    started(&mut app, Terminal::Vtium);
    // greyed on a system font: its letter does nothing
    assert!(keys(&mut app, &["Alt-A", "i"]).is_empty());
    assert_eq!(app.node(), "any.menu.actions");
    keys(&mut app, &["Esc"]);
    *own.borrow_mut() = true;
    assert_eq!(
        keys(&mut app, &["Alt-A", "i"]),
        [Effect::Desk(Op::SystemFont(PathBuf::from(OWN)))]
    );
    let mut stand_in = StandIn::default();
    let ready = machine::Machine::ready_font(&mut stand_in, Path::new(OWN));
    app.event(Event::FontReady(ready));
    let question = app.nav().question().map(|q| q.text.clone());
    assert_eq!(
        question.as_deref(),
        Some("Install Mine-Regular.ttf as a system font?")
    );
    let screen = text(&app);
    for line in [
        "Copy Mine-Regular.ttf to /usr/local/share/fonts/Mine-Regular.ttf? vtium-config runs:",
        "sudo /usr/bin/vtium-config apply /home/user/.config/vtium/drafts/d1",
        "sudo asks for your password.",
    ] {
        assert!(screen.contains(line), "{line:?}:\n{screen}");
    }
    assert_eq!(press(&mut app, Key::char('y')), [Effect::InstallFont]);
    app.event(Event::FontInstalled(Ok(())));
    assert_eq!(
        message(&app).as_deref(),
        Some("Installed /usr/local/share/fonts/Mine-Regular.ttf. Activate it in Fonts to use it.")
    );
    // Esc goes back without acting
    keys(&mut app, &["Alt-A", "i"]);
    app.event(Event::FontReady(machine::Machine::ready_font(
        &mut stand_in,
        Path::new(OWN),
    )));
    assert!(keys(&mut app, &["Esc"]).is_empty());
    assert!(app.nav().question().is_none());
}

#[test]
fn the_loop_installs_a_system_font_while_the_tui_steps_aside() {
    use vtium_config_tui::port::drive_on;
    let (app, own) = app_in(Machine::Map, MAP, true);
    *own.borrow_mut() = true;
    // Alt+A, i, then y to the question, then a hangup
    let mut port = common::Script::new(
        Size::grid(100, 30),
        vec![
            vec![Wake::Bytes(b"\x1b[?62;22c".to_vec())],
            vec![Wake::Bytes(b"\x1ba".to_vec())],
            vec![Wake::Bytes(b"i".to_vec())],
            vec![Wake::Bytes(b"y".to_vec())],
            vec![Wake::Bytes(b"\x1ba".to_vec())],
            vec![Wake::Bytes(b"r".to_vec())],
        ],
    );
    let mut stand_in = StandIn {
        devices: Machine::Large.shared().present,
        ..StandIn::default()
    };
    let left = drive_on(&mut port, &mut stand_in, app).expect("the loop runs");
    assert_eq!(left.status, 129);
    assert_eq!(stand_in.installs, 1);
    assert_eq!(stand_in.reads, 1, "Read devices again read them once");
    assert_eq!(
        port.asides, 1,
        "the install ran while the TUI stepped aside"
    );
    // the frame after the step aside is drawn whole, with the message
    let written = String::from_utf8_lossy(&port.written);
    assert!(written.contains("Installed"), "{written}");
}

// The screens.

/// Draws the screen that `setup` reaches, on both machines at both sizes,
/// checks that each row is the screen's width, that no mark is carried by
/// color alone, and that `musts` show at both sizes, and compares the four
/// screens with the snapshot `name`.
fn screens(name: &str, setup: &dyn Fn(&mut App, &Rc<RefCell<bool>>), musts: &[&str]) {
    let mut out = String::new();
    for machine in Machine::BOTH {
        for size in SIZES {
            let (mut app, own) = app_in(machine, size, true);
            setup(&mut app, &own);
            let color = draw(&app);
            let (mut mono, own) = app_in(machine, size, false);
            setup(&mut mono, &own);
            let mono = draw(&mono);
            let what = format!("{name} on {} at {}×{}", machine.name(), size.0, size.1);
            common::rows_fit(&color, &what);
            common::marks_survive_without_color(&color, &mono, &what);
            let screen = common::rows(&color).join("\n");
            for must in musts {
                assert!(
                    screen.contains(must),
                    "{what}: {must:?} is missing:\n{screen}"
                );
            }
            out.push_str(&format!("== {what}\n"));
            out.push_str(&common::snapshot_text(&color));
        }
    }
    common::snapshot(name, &out);
}

#[test]
fn the_actions_menu_on_a_vtium_console() {
    screens(
        "live-actions-menu",
        &|app, _| {
            started(app, Terminal::Vtium);
            keys(app, &["Alt-A"]);
        },
        &["Try live", "Read devices again", "on a font of your own"],
    );
}

#[test]
fn the_actions_menu_where_try_live_cannot_run() {
    screens(
        "live-actions-greyed",
        &|app, own| {
            started(app, Terminal::Other);
            *own.borrow_mut() = true;
            keys(app, &["Alt-A"]);
        },
        &[
            "Try live · not a vtium console",
            "Install as a system font…",
        ],
    );
}

#[test]
fn the_countdown_of_the_trial() {
    screens(
        "live-countdown",
        &|app, _| {
            started(app, Terminal::Vtium);
            keys(app, &["Alt-L", "t"]);
            for _ in 0..3 {
                app.event(Event::Timeout);
            }
        },
        &["(proof) on this console: 7 s left.", "[k] Keep"],
    );
}

#[test]
fn the_font_kept_for_the_session() {
    screens(
        "live-kept",
        &|app, _| {
            started(app, Terminal::Vtium);
            keys(app, &["t"]);
            reply(app, Reply::Font("Terminus".into()));
            reply(app, Reply::Attributes);
            keys(app, &["k"]);
        },
        &["The console keeps Terminus until you quit."],
    );
}

#[test]
fn the_console_refused_the_font() {
    screens(
        "live-refused",
        &|app, _| {
            started(app, Terminal::Vtium);
            keys(app, &["t"]);
            reply(app, Reply::Font(START.into()));
        },
        &["The console refused Terminus"],
    );
}

#[test]
fn try_live_refused_outside_a_vtium_console() {
    screens(
        "live-not-vtium",
        &|app, _| {
            started(app, Terminal::Other);
            keys(app, &["Alt-L", "t"]);
        },
        &["Try live is off: this terminal is not a vtium console."],
    );
}

#[test]
fn try_live_refused_with_allow_font_ops_off() {
    screens(
        "live-ops-off",
        &|app, _| {
            started(app, Terminal::OpsOff);
            keys(app, &["Alt-L", "t"]);
        },
        &["Try live is off: allow-font-ops is off."],
    );
}

#[test]
fn read_devices_again_names_what_came() {
    screens(
        "live-read-devices",
        &|app, _| {
            started(app, Terminal::Vtium);
            keys(app, &["Alt-A", "r"]);
            let mut now = app.shared().present.clone();
            let mut more = now.devices.first().cloned().expect("a device");
            more.connector = Some("HDMI-A-9".to_owned());
            more.name = Some("Logitech USB Receiver".to_owned());
            now.devices.push(more);
            app.event(Event::Devices(Some(now)));
        },
        &["Read devices again: connected HDMI-A-9."],
    );
}

#[test]
fn install_as_a_system_font_asks_first() {
    screens(
        "live-system-font",
        &|app, own| {
            started(app, Terminal::Vtium);
            *own.borrow_mut() = true;
            keys(app, &["Alt-A", "i"]);
            let mut stand_in = StandIn::default();
            let ready = machine::Machine::ready_font(&mut stand_in, Path::new(OWN));
            app.event(Event::FontReady(ready));
        },
        &[
            "Install Mine-Regular.ttf as a system font?",
            "sudo /usr/bin/vtium-config apply /home/user/.config/vtium/drafts/d1",
            "[y]",
        ],
    );
}

#[test]
fn the_bar_names_the_trial_s_keys_at_the_minimum_size() {
    let mut app = live(Machine::Map, (80, 25));
    keys(&mut app, &["t"]);
    let (_, y) = locate(&app, "[k] Keep");
    assert_eq!(y, 24, "the bar names k");
    assert_eq!(app.node(), "any.trial");
}
