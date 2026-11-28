//! File › History and File › Revert to saved…. Each test drives the App
//! with a key or a click, over the screen map's machine, the large machine
//! with its thirty versions, or a machine with no earlier version, and
//! checks the state, the screen and the effects that come out.
//!
//! The stand-in tabs here are a Flow tab, whose node gives the Flow view's
//! `r`, `v` and `o`, and a Looks tab, each drawing one line.
//!
//! The screens are kept as snapshots under `tests/snapshots/`, each with
//! four screens: the screen map's machine and the large machine, each at
//! 100×30 and at 80×25.

#![expect(clippy::expect_used, reason = "a test fails by panicking")]

mod common;

use std::collections::BTreeMap;

use ratatui_core::layout::Rect;
use ratatui_core::terminal::Frame;
use vtium_config_core::document::{Base, Content, Document, Documents, InMemory, Installed};
use vtium_config_core::fixture;
use vtium_config_core::ui::{Action, Command, Shared};
use vtium_config_tui::BoxTab;
use vtium_config_tui::app::{App, Size};
use vtium_config_tui::changes::Against;
use vtium_config_tui::history;
use vtium_config_tui::nav::menus::Which;
use vtium_config_tui::sheets::Sheet;
use vtium_config_widgets::tab::Tab;

use common::{Machine, SIZES, click, draw, keys, locate, rows, text};

/// The stand-in Flow tab: its node gives `r`, `v` and `o`.
struct Flow;

/// The stand-in Looks tab.
struct Looks;

fn one_line(frame: &mut Frame<'_>, area: Rect, line: &str) {
    frame.buffer_mut().set_stringn(
        area.x,
        area.y,
        line,
        usize::from(area.width),
        ratatui_core::style::Style::default(),
    );
}

impl Tab<Shared, Action, Command> for Flow {
    fn name(&self) -> &'static str {
        "Flow"
    }
    fn letter(&self) -> char {
        'o'
    }
    fn node(&self) -> &'static str {
        "flow"
    }
    fn draw(&self, _shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        one_line(frame, area, "the Flow tab's diagram");
    }
    fn update(&mut self, _shared: &Shared, _action: Action) -> Vec<Command> {
        Vec::new()
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
        one_line(frame, area, "the Looks tab's list");
    }
    fn update(&mut self, _shared: &Shared, _action: Action) -> Vec<Command> {
        Vec::new()
    }
}

fn app_on(shared: Shared, (cols, rows): (u16, u16), color: bool) -> App {
    let tabs: Vec<BoxTab> = vec![Box::new(Flow), Box::new(Looks)];
    App::new(tabs, shared, Size::grid(cols, rows), color)
}

fn app_in(machine: Machine, size: (u16, u16), color: bool) -> App {
    app_on(machine.shared(), size, color)
}

/// A machine where nothing was saved over: the screen map's document and
/// installed file, on a desk with no version.
fn no_history() -> Shared {
    let content = Content {
        text: fixture::padnoir::DRAFT.to_owned(),
        base: Base::of(Some(fixture::padnoir::INSTALLED)),
        ..Content::default()
    };
    let desk = InMemory::new().with_draft(fixture::padnoir::NAME, content.clone());
    let document = Document::new(
        fixture::padnoir::NAME,
        content,
        Installed {
            text: Some(fixture::padnoir::INSTALLED.to_owned()),
            frits: BTreeMap::new(),
        },
    );
    Shared::new(
        Documents::new(document, Box::new(desk)),
        vtium_config_core::flowmodel::Present::default(),
    )
}

fn message(app: &App) -> Option<String> {
    app.shared().message.as_ref().map(|m| m.line.clone())
}

fn top(app: &App) -> Option<&Sheet> {
    app.nav().sheets().last()
}

/// File › History › and Enter on the entry `down` lines below the first.
fn pick(app: &mut App, down: usize) {
    keys(app, &["Alt-F", "h"]);
    for _ in 0..down {
        keys(app, &["Down"]);
    }
    keys(app, &["Enter"]);
    assert_eq!(app.node(), "any.history.version");
}

// History.

#[test]
fn history_lists_the_versions_newest_first_and_enter_opens_the_one_chosen() {
    let mut app = app_in(Machine::Large, (100, 30), true);
    keys(&mut app, &["Alt-F", "h"]);
    let screen = text(&app);
    let first = screen.find("2026-11-02 06:00").expect("the newest");
    let older = screen.find("2026-11-02 05:00").expect("one older");
    assert!(first < older, "{screen}");
    keys(&mut app, &["Esc", "Esc"]);
    pick(&mut app, 2);
    let Some(Sheet::Version(sheet)) = top(&app) else {
        panic!("no version's sheet");
    };
    assert_eq!(sheet.title(), "The version of 2026-11-02 04:00");
    let versions = app.shared().documents.desk().versions().expect("versions");
    assert_eq!(Some(sheet.version()), versions.get(2));
    // the sheet counts nothing
    let screen = text(&app);
    assert!(!screen.contains("lines"), "{screen}");
    assert!(screen.contains("[v] View changes"), "{screen}");
    keys(&mut app, &["Esc"]);
    assert_eq!(app.node(), "flow");
}

#[test]
fn with_no_earlier_version_history_holds_one_greyed_line() {
    let mut app = app_on(no_history(), (100, 30), true);
    keys(&mut app, &["Alt-F", "h"]);
    assert_eq!(app.node(), "any.menu.file.history");
    let screen = text(&app);
    assert!(
        screen.contains("No earlier version · no save replaced one yet"),
        "{screen}"
    );
    keys(&mut app, &["Enter"]);
    assert_eq!(
        app.node(),
        "any.menu.file.history",
        "the line chooses nothing"
    );
}

#[test]
fn o_in_flow_opens_file_on_history_s_submenu() {
    let mut app = app_in(Machine::Map, (100, 30), true);
    keys(&mut app, &["o"]);
    let open = app.nav().menu().expect("a menu");
    assert_eq!(open.which, Which::File);
    assert_eq!(app.node(), "any.menu.file.history");
    keys(&mut app, &["Enter"]);
    assert_eq!(app.node(), "any.history.version");
}

#[test]
fn use_this_version_loads_it_as_one_change_and_ctrl_z_brings_the_document_back() {
    for machine in Machine::BOTH {
        let mut app = app_in(machine, (100, 30), true);
        let before = app.shared().document().content().clone();
        let made = app.shared().documents.history().made();
        pick(&mut app, 0);
        let Some(Sheet::Version(sheet)) = top(&app) else {
            panic!("no version's sheet");
        };
        let version = sheet.version().clone();
        keys(&mut app, &["a"]);
        assert_eq!(app.node(), "flow", "back to the tab");
        assert_eq!(app.shared().document().text(), version.text);
        assert_eq!(app.shared().documents.history().made(), made + 1);
        let when = vtium_config_tui::menus::file::when(version.when);
        assert_eq!(
            message(&app),
            Some(format!(
                "Using the version of {when}: one change, not saved. [Ctrl+Z] takes it back."
            ))
        );
        keys(&mut app, &["Ctrl-Z"]);
        assert_eq!(app.shared().document().content(), &before);
        assert_eq!(app.shared().documents.history().made(), made);
    }
}

#[test]
fn view_changes_shows_the_version_read_only_and_esc_goes_back_to_its_sheet() {
    for machine in Machine::BOTH {
        let mut app = app_in(machine, (100, 30), true);
        let before = app.shared().document().content().clone();
        pick(&mut app, 0);
        keys(&mut app, &["v"]);
        assert_eq!(app.node(), "any.history.view");
        let Some(Sheet::Changes(view)) = top(&app) else {
            panic!("no view");
        };
        assert!(matches!(view.against(), Against::Version(_)));
        let screen = text(&app);
        assert!(screen.contains("Version 2026-"), "{screen}");
        assert!(screen.contains("[w] Whole file  [Esc] Back"), "{screen}");
        // the keys that edit elsewhere do nothing here
        keys(&mut app, &["e", "Ctrl-S", "a", "w"]);
        assert_eq!(app.node(), "any.history.view");
        assert_eq!(app.shared().document().content(), &before);
        assert!(text(&app).contains("[w] Changes only"));
        keys(&mut app, &["Esc"]);
        assert_eq!(app.node(), "any.history.version");
        keys(&mut app, &["Esc"]);
        assert_eq!(app.node(), "flow");
        assert_eq!(app.shared().document().content(), &before);
    }
}

#[test]
fn the_view_s_red_lines_leave_the_document_and_its_green_ones_come_with_the_version() {
    let mut app = app_in(Machine::Map, (100, 30), true);
    pick(&mut app, 0);
    keys(&mut app, &["v"]);
    let shown: Vec<String> = rows(&draw(&app));
    // the document's look big goes, and the version's look proof comes
    assert!(
        shown.iter().any(|r| r.contains("-[look big]")),
        "{shown:#?}"
    );
    assert!(
        shown.iter().any(|r| r.contains("+[look proof]")),
        "{shown:#?}"
    );
}

#[test]
fn a_click_on_a_button_s_line_or_on_the_bar_acts_as_its_key() {
    let mut app = app_in(Machine::Map, (100, 30), true);
    pick(&mut app, 0);
    let (x, y) = locate(&app, "[v] View changes  ");
    click(&mut app, x + 5, y);
    assert_eq!(app.node(), "any.history.view");
    keys(&mut app, &["Esc"]);
    let (x, y) = locate(&app, "[a] Use this version");
    click(&mut app, x + 5, y);
    assert_eq!(app.node(), "flow");
    assert_eq!(app.shared().document().text(), fixture::padnoir::VERSION);
}

// Revert to saved.

#[test]
fn revert_asks_first_counting_what_is_lost_and_is_one_step() {
    for machine in Machine::BOTH {
        let mut app = app_in(machine, (100, 30), true);
        let before = app.shared().document().content().clone();
        keys(&mut app, &["Alt-F", "r"]);
        assert_eq!(app.node(), "any.question.confirm");
        let screen = text(&app);
        assert!(
            screen.contains("Drop every change since the last save? 1 change is lost:"),
            "{screen}"
        );
        keys(&mut app, &["y"]);
        let document = app.shared().document();
        assert_eq!(Some(document.text()), document.installed().text.as_deref());
        assert_eq!(document.count(), 0);
        assert_eq!(
            message(&app).as_deref(),
            Some(
                "Reverted to saved, as /etc/vtium/vtium.conf holds it. [Ctrl+Z] brings back the change."
            )
        );
        keys(&mut app, &["Ctrl-Z"]);
        assert_eq!(app.shared().document().content(), &before);
    }
}

#[test]
fn r_in_flow_asks_and_esc_drops_nothing() {
    let mut app = app_in(Machine::Map, (100, 30), true);
    let before = app.shared().document().content().clone();
    keys(&mut app, &["r"]);
    assert_eq!(app.node(), "any.question.confirm");
    keys(&mut app, &["Esc"]);
    assert_eq!(app.node(), "flow");
    assert_eq!(app.shared().document().content(), &before);
}

#[test]
fn with_nothing_to_drop_revert_says_so_and_the_menu_greys_it_out() {
    let mut app = app_in(Machine::Map, (100, 30), true);
    keys(&mut app, &["r", "y"]);
    keys(&mut app, &["Alt-F"]);
    let screen = text(&app);
    assert!(
        screen.contains("Revert to saved… · nothing to drop"),
        "{screen}"
    );
    assert!(screen.contains("Show changes · none"), "{screen}");
    keys(&mut app, &["Esc", "r"]);
    assert_eq!(app.node(), "flow", "nothing is asked");
    assert_eq!(
        message(&app).as_deref(),
        Some(vtium_config_tui::revert::words::NOTHING)
    );
}

#[test]
fn the_routes_of_history_and_revert_are_the_shell_s() {
    use vtium_config_tui::app::update::{Route, route};
    assert_eq!(route(history::PICK), Route::Version);
    assert_eq!(route(history::VIEW), Route::ViewVersion);
    assert_eq!(route(history::USE), Route::UseVersion);
    assert_eq!(route(history::MENU), Route::HistoryMenu);
    assert_eq!(route(vtium_config_tui::revert::OPEN), Route::Revert);
    assert_eq!(route(vtium_config_tui::revert::YES), Route::Reverted);
}

// The screens.

fn screens(name: &str, setup: &dyn Fn(&mut App), musts: &[&str]) {
    let mut out = String::new();
    for machine in Machine::BOTH {
        for size in SIZES {
            let mut color = app_in(machine, size, true);
            setup(&mut color);
            let mut mono = app_in(machine, size, false);
            setup(&mut mono);
            let (color, mono) = (draw(&color), draw(&mono));
            let what = format!("{name} on {} at {}×{}", machine.name(), size.0, size.1);
            common::rows_fit(&color, &what);
            common::marks_survive_without_color(&color, &mono, &what);
            let screen = rows(&color).join("\n");
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
fn a_version_s_sheet_at_both_sizes_with_both_fixtures() {
    screens(
        "history-version",
        &|app| pick(app, 0),
        &[
            "The version of 2026-",
            "[v] View changes",
            "[a] Use this version",
            "[Esc] Close",
        ],
    );
}

#[test]
fn a_version_s_view_at_both_sizes_with_both_fixtures() {
    screens(
        "history-view",
        &|app| {
            pick(app, 0);
            keys(app, &["v"]);
        },
        &["Version 2026-", "✓ Changes only", "[Esc] Back"],
    );
}

#[test]
fn a_version_s_view_of_the_whole_file_at_both_sizes_with_both_fixtures() {
    screens(
        "history-view-whole-file",
        &|app| {
            pick(app, 0);
            keys(app, &["v", "w"]);
        },
        &["Version 2026-", "✓ Whole file", "[w] Changes only"],
    );
}

#[test]
fn use_this_version_s_result_at_both_sizes_with_both_fixtures() {
    screens(
        "history-used",
        &|app| {
            pick(app, 0);
            keys(app, &["a"]);
        },
        &["Using the version of 2026-", "not saved"],
    );
}

#[test]
fn revert_s_question_at_both_sizes_with_both_fixtures() {
    screens(
        "history-revert",
        &|app| {
            keys(app, &["r"]);
        },
        &["Drop every change since the last save?", "[y] Yes"],
    );
}

#[test]
fn history_with_no_earlier_version_at_both_sizes() {
    let mut out = String::new();
    for size in SIZES {
        let mut color = app_on(no_history(), size, true);
        let mut mono = app_on(no_history(), size, false);
        for app in [&mut color, &mut mono] {
            keys(app, &["Alt-F", "h"]);
        }
        let (color, mono) = (draw(&color), draw(&mono));
        let what = format!("history-none at {}×{}", size.0, size.1);
        common::rows_fit(&color, &what);
        common::marks_survive_without_color(&color, &mono, &what);
        out.push_str(&format!("== {what}\n"));
        out.push_str(&common::snapshot_text(&color));
    }
    common::snapshot("history-none", &out);
}
