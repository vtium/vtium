//! The sheets the shell opens over a tab: F1's help, Ctrl+F's Find, the
//! Problems list, File › New…, File › Open › and Manage configurations….
//! Each test drives the App with a key or a click, over the screen map's
//! machine or the large machine, and checks the state, the screen and the
//! effects that come out.
//!
//! The stand-in tabs here answer what the sheets ask a tab: Fonts lists
//! four fonts for Find and has a full text behind its second line for F1,
//! and Flow lists the machine's displays and inputs for Find, so the large
//! machine's forty displays are searched as a Flow tab would offer them.
//!
//! The screens are kept as snapshots under `tests/snapshots/`, each with
//! four screens: the screen map's machine and the large machine, each at
//! 100×30 and at 80×25.

#![expect(clippy::expect_used, reason = "a test fails by panicking")]

mod common;

use std::rc::Rc;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::terminal::Frame;
use vtium_config_core::ui::{Action, Carries, Change, Command, Message, Request, Shared};
use vtium_config_tui::BoxTab;
use vtium_config_tui::app::{App, Size};
use vtium_config_tui::sheets::problems::{Goes, Problem, inactive_fonts};
use vtium_config_tui::sheets::{Act, Problems, Sheet};
use vtium_config_widgets::frame::{Options, Regions};
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::tab::{FindItem, Tab};

use common::{Log, Machine, SIZES, draw, keys, plain, press, text};

/// The stand-in Fonts tab: four fonts, the second with a full text.
struct Fonts {
    log: Log,
    highlight: usize,
}

const FONTS: [(&str, &str); 4] = [
    ("Terminus", "ter-u16n.psf.gz"),
    ("Cullet Mono", "CulletMono-Regular.otf"),
    ("Noto Sans Mono", "NotoSansMono-Regular.ttf"),
    ("Unifont", "unifont.otf"),
];

/// The full text behind the Fonts tab's second line.
const FULL: &str = "Cullet Mono Regular, in /usr/share/fonts/vtium.\nIt covers Latin, Greek and Cyrillic, and box drawing.";

/// The longer half of a message whose line was cut.
const MORE: &str =
    "The console refused DejaVuSansMono.ttf: it takes only fonts root installed and trusts.";

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
        for (i, (family, file)) in FONTS.iter().enumerate() {
            let y = area.y + u16::try_from(i).expect("a row");
            if y < area.bottom() {
                let mark = if i == self.highlight { "▸" } else { " " };
                frame.buffer_mut().set_stringn(
                    area.x,
                    y,
                    format!("{mark} {family:<18} {file}"),
                    usize::from(area.width),
                    ratatui_core::style::Style::default(),
                );
            }
        }
    }

    fn update(&mut self, _shared: &Shared, action: Action) -> Vec<Command> {
        self.log.borrow_mut().push(action.clone());
        match action.row {
            "down.view" => {
                self.highlight = (self.highlight + 1).min(FONTS.len() - 1);
                Vec::new()
            }
            "enter.view.fonts" => vec![Command::Request(Request::Open("fonts.font"))],
            "t.view.fonts" => vec![Command::Change(Change::Say(Message {
                line: "The console refused DejaVuSansMono.ttf (F1).".to_owned(),
                more: Some(MORE.to_owned()),
            }))],
            _ => Vec::new(),
        }
    }

    fn full_text(&self, _shared: &Shared) -> Option<String> {
        (self.highlight == 1).then(|| FULL.to_owned())
    }

    fn find_items(&self, _shared: &Shared) -> Vec<FindItem> {
        FONTS
            .iter()
            .enumerate()
            .map(|(index, (family, file))| FindItem {
                name: (*family).to_owned(),
                description: Some((*file).to_owned()),
                place: "Fonts".to_owned(),
                fact: format!("{file} · active"),
                index,
            })
            .collect()
    }
}

/// The stand-in Flow tab: the machine's displays and inputs for Find.
struct Flow {
    log: Log,
}

/// The name Find shows for a device: a display's connector and its
/// monitor's serial, or an input's name.
fn device_name(d: &vtium_config_core::flowmodel::Present, i: usize) -> Option<(String, String)> {
    let d = d.devices.get(i)?;
    match (&d.connector, &d.edid, &d.name) {
        (Some(c), Some(e), _) => Some((format!("{c} {}", e.serial), "Flow › Displays".to_owned())),
        (_, _, Some(n)) => Some((n.clone(), "Flow › Inputs".to_owned())),
        _ => None,
    }
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
        frame.buffer_mut().set_stringn(
            area.x,
            area.y,
            " CONFIGURATION",
            usize::from(area.width),
            ratatui_core::style::Style::default(),
        );
    }

    fn update(&mut self, _shared: &Shared, action: Action) -> Vec<Command> {
        self.log.borrow_mut().push(action);
        Vec::new()
    }

    fn find_items(&self, shared: &Shared) -> Vec<FindItem> {
        (0..shared.present.devices.len())
            .filter_map(|i| {
                let (name, place) = device_name(&shared.present, i)?;
                Some(FindItem {
                    name,
                    description: None,
                    place,
                    fact: "in no path yet".to_owned(),
                    index: i,
                })
            })
            .collect()
    }
}

/// The App over `machine` at `size`, with the stand-in Fonts, Looks and
/// Flow tabs, in color where `color`, and each tab's log.
fn app_in(machine: Machine, size: (u16, u16), color: bool) -> (App, Log, Log, Log) {
    let (fonts, looks, flow) = (Log::default(), Log::default(), Log::default());
    let tabs: Vec<BoxTab> = vec![
        Box::new(Fonts {
            log: Rc::clone(&fonts),
            highlight: 0,
        }),
        Box::new(common::Looks {
            log: Rc::clone(&looks),
        }),
        Box::new(Flow {
            log: Rc::clone(&flow),
        }),
    ];
    let app = App::new(tabs, machine.shared(), Size::grid(size.0, size.1), color);
    (app, fonts, looks, flow)
}

fn app(machine: Machine) -> (App, Log, Log, Log) {
    app_in(machine, (100, 30), true)
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, Key::char(c));
    }
}

fn message(app: &App) -> Option<String> {
    app.shared().message.as_ref().map(|m| m.line.clone())
}

fn last(log: &Log) -> Option<Action> {
    log.borrow().last().cloned()
}

fn top(app: &App) -> Option<&Sheet> {
    app.nav().sheets().last()
}

// Find.

#[test]
fn find_over_the_large_machine_finds_a_display_by_its_name_among_forty() {
    let (mut app, _, _, flow) = app(Machine::Large);
    press(&mut app, Key::ctrl('f'));
    assert_eq!(app.node(), "any.find");
    let Some(Sheet::Find(find)) = top(&app) else {
        panic!("Find is open");
    };
    let displays = find
        .search()
        .count()
        .saturating_sub(FONTS.len())
        .saturating_sub(60);
    assert_eq!(displays, 40, "Find holds the forty displays");
    typed(&mut app, "panel0703");
    let Some(Sheet::Find(find)) = top(&app) else {
        panic!("Find is open");
    };
    assert_eq!(find.search().matches().len(), 1);
    assert_eq!(find.count_line(), "1 found in Fonts and Flow");
    let (tab, item) = find.highlighted().expect("a match");
    assert_eq!((tab, item.name.as_str()), (2, "DP-3 PANEL0703"));
    let index = item.index;
    assert!(text(&app).contains("DP-3 PANEL0703"));
    press(&mut app, plain(Code::Enter));
    assert_eq!(app.nav().tab(), 2, "Enter opens the item's tab");
    assert!(app.nav().sheets().is_empty(), "and closes Find");
    assert_eq!(app.node(), "flow");
    let action = last(&flow).expect("the tab is handed the item");
    assert_eq!(
        (action.row, action.carries),
        ("enter.typing.find", Carries::Item(index))
    );
}

#[test]
fn ctrl_f_over_a_menu_closes_the_menu_first() {
    for menu in ['f', 'a'] {
        let (mut app, ..) = app(Machine::Map);
        press(&mut app, Key::alt(menu));
        assert!(app.nav().menu().is_some());
        press(&mut app, Key::ctrl('f'));
        assert_eq!(app.nav().menu(), None, "the menu closes");
        assert_eq!(app.node(), "any.find");
        assert!(!text(&app).contains("Save…"), "no menu is drawn");
    }
}

#[test]
fn find_over_a_level_goes_back_to_it_on_esc_and_closes_it_on_enter() {
    let (mut app, fonts, ..) = app(Machine::Map);
    press(&mut app, plain(Code::Enter));
    assert_eq!(app.node(), "fonts.font");
    press(&mut app, Key::ctrl('f'));
    assert_eq!(app.node(), "any.find");
    press(&mut app, plain(Code::Escape));
    assert_eq!(app.node(), "fonts.font", "Esc goes back to the level");
    press(&mut app, Key::ctrl('f'));
    typed(&mut app, "^noto");
    press(&mut app, plain(Code::Enter));
    assert_eq!(
        app.nav().levels(0),
        [] as [&str; 0],
        "Enter closes the level"
    );
    assert_eq!(app.node(), "fonts");
    let action = last(&fonts).expect("an action");
    assert_eq!(action.carries, Carries::Item(2));
}

#[test]
fn find_with_nothing_typed_shows_no_match_and_backspace_closes_it() {
    let (mut app, fonts, ..) = app(Machine::Map);
    press(&mut app, Key::ctrl('f'));
    let screen = text(&app);
    assert!(
        screen.contains("Find searches the names in Fonts and Flow."),
        "{screen}"
    );
    assert!(!screen.contains("ter-u16n.psf.gz · active"), "{screen}");
    press(&mut app, plain(Code::Enter));
    assert_eq!(app.node(), "any.find", "Enter with nothing typed stays");
    typed(&mut app, "q");
    assert_eq!(app.leaving(), None, "q is typed into Find");
    keys(&mut app, &["Backspace", "Backspace"]);
    assert_eq!(app.node(), "fonts");
    assert!(fonts.borrow().is_empty(), "nothing reached the tab");
}

#[test]
fn a_click_on_a_match_goes_to_it() {
    let (mut app, fonts, ..) = app(Machine::Map);
    press(&mut app, Key::ctrl('f'));
    typed(&mut app, "mono");
    let (x, y) = common::locate(&app, "NotoSansMono-Regular.ttf · active");
    common::click(&mut app, x, y);
    assert_eq!(app.node(), "fonts");
    assert_eq!(last(&fonts).map(|a| a.carries), Some(Carries::Item(2)));
}

// F1.

#[test]
fn f1_opens_the_rest_of_the_message_first() {
    let (mut app, ..) = app(Machine::Map);
    press(&mut app, Key::char('t'));
    assert!(message(&app).is_some_and(|m| m.ends_with("(F1).")));
    press(&mut app, plain(Code::F(1)));
    assert_eq!(app.node(), "any.about");
    assert!(text(&app).contains(MORE), "{}", text(&app));
}

#[test]
fn f1_opens_the_full_text_behind_the_highlighted_line_else_every_key() {
    let (mut app, ..) = app(Machine::Map);
    press(&mut app, plain(Code::F(1)));
    let screen = text(&app);
    assert!(screen.contains("Keys of Fonts"), "{screen}");
    assert!(screen.contains("Everywhere"), "{screen}");
    assert!(screen.contains("[Ctrl+F]"), "{screen}");
    press(&mut app, plain(Code::Escape));
    assert_eq!(app.node(), "fonts", "Esc closes the help");
    press(&mut app, plain(Code::Down));
    press(&mut app, plain(Code::F(1)));
    assert!(text(&app).contains("It covers Latin, Greek and Cyrillic"));
}

#[test]
fn f1_does_nothing_in_a_menu_or_over_its_own_help() {
    let (mut app, ..) = app(Machine::Map);
    keys(&mut app, &["Alt-F", "F1"]);
    assert_eq!(app.node(), "any.menu.file");
    keys(&mut app, &["Esc", "F1", "F1"]);
    assert_eq!(
        app.nav().sheets().len(),
        1,
        "F1 over its help opens no second"
    );
}

// The Problems list.

fn problem(stops: bool, tab: &'static str, place: &str, line: &str) -> Problem {
    Problem {
        stops,
        place: place.to_owned(),
        line: line.to_owned(),
        more: None,
        goes: Goes::Item { tab, item: Some(1) },
    }
}

/// A stop in Looks and two warnings, in Flow and in Fonts, given out of
/// the list's order.
fn three() -> Vec<Problem> {
    vec![
        problem(
            false,
            "flow",
            "Flow › PARTS",
            "1 keyboard and 2 pointers are in no path",
        ),
        problem(
            true,
            "looks",
            "Looks › big",
            "the font set proof is not installed",
        ),
        problem(
            false,
            "fonts",
            "Fonts › NotoColorEmoji.ttf",
            "it fails the trust rule",
        ),
    ]
}

#[test]
fn the_problems_list_puts_a_stop_first_then_the_warnings_in_tab_order() {
    let list = Problems::new(three());
    let places: Vec<&str> = list.problems().iter().map(|p| p.place.as_str()).collect();
    assert_eq!(
        places,
        ["Looks › big", "Fonts › NotoColorEmoji.ttf", "Flow › PARTS"]
    );
}

#[test]
fn enter_on_a_problem_goes_to_its_place_and_on_fonts_offers_activate_missing() {
    let mut list = Sheet::Problems(Problems::new(three()));
    let enter = Key::plain(Code::Enter);
    let row = vtium_config_widgets::contract::row("enter.layer.problems");
    assert_eq!(
        list.key(&enter, row),
        Act::GoNode {
            tab: "looks",
            action: Some(Action::item("enter.layer.problems", 1)),
        }
    );
    let mut fonts = Sheet::Problems(Problems::new(vec![inactive_fonts(3)]));
    assert_eq!(fonts.key(&enter, row), Act::Activate(3));
    let mut line = Sheet::Problems(Problems::new(vec![Problem {
        goes: Goes::Line(32),
        ..problem(false, "flow", "vtium.conf line 32", "x")
    }]));
    assert_eq!(line.key(&enter, row), Act::Shell(Action::of("v.view.flow")));
}

#[test]
fn alt_p_opens_the_problems_list_and_esc_closes_it() {
    for machine in Machine::BOTH {
        let (mut app, ..) = app(machine);
        press(&mut app, Key::alt('p'));
        assert_eq!(app.node(), "any.problems");
        assert!(text(&app).contains("No problems"));
        press(&mut app, plain(Code::Escape));
        assert_eq!(app.node(), "fonts");
    }
}

// File › New….

/// Opens File › New… and types `name` over the hostname it offers.
fn new_named(app: &mut App, name: &str) {
    keys(app, &["Alt-F", "n"]);
    for _ in 0..70 {
        press(app, plain(Code::Backspace));
    }
    typed(app, name);
}

#[test]
fn new_starts_from_what_is_saved_or_from_vtium_s_defaults() {
    for machine in Machine::BOTH {
        let (mut app, ..) = app(machine);
        let was = app.shared().document().name().to_owned();
        let installed = app.shared().document().installed().text.clone();
        new_named(&mut app, "kiosk-test");
        assert_eq!(app.node(), "any.new");
        press(&mut app, plain(Code::Enter));
        let doc = app.shared().document();
        assert_eq!(doc.name(), "kiosk-test");
        assert_eq!(Some(doc.text().to_owned()), installed, "from what is saved");
        assert_eq!(doc.count(), 0);
        assert!(message(&app).is_some_and(|m| m.contains(&was)));

        new_named(&mut app, "fresh");
        keys(&mut app, &["Down", "Down", "Enter"]);
        let doc = app.shared().document();
        assert_eq!(doc.name(), "fresh");
        assert!(doc.text().contains("[look default]"), "{}", doc.text());
        assert!(
            doc.count() > 0,
            "the defaults count against the installed file"
        );
    }
}

#[test]
fn new_refuses_a_name_another_configuration_has() {
    let (mut app, ..) = app(Machine::Large);
    new_named(&mut app, "lab-03");
    press(&mut app, plain(Code::Enter));
    assert_eq!(app.node(), "any.new");
    assert_eq!(app.shared().document().name(), "lab");
    assert!(text(&app).contains("lab-03 is taken"));
    press(&mut app, plain(Code::Escape));
    assert_eq!(app.node(), "fonts");
}

// File › Open › and Manage configurations….

#[test]
fn file_open_opens_the_one_chosen_and_keeps_the_one_open() {
    let (mut app, ..) = app(Machine::Large);
    keys(&mut app, &["Alt-F", "o", "Enter"]);
    assert_eq!(app.shared().document().name(), "lab", "lab is open already");
    assert_eq!(message(&app), None);
    keys(&mut app, &["Alt-F", "o", "Down", "Enter"]);
    assert_eq!(app.shared().document().name(), "lab-01");
    assert_eq!(app.node(), "fonts", "the menu closed");
    assert_eq!(
        message(&app).as_deref(),
        Some("Opened lab-01. lab keeps its 1 change, not saved.")
    );
    keys(&mut app, &["Alt-F", "o"]);
    assert!(text(&app).contains("✓ lab-01"), "the one open is marked");
}

/// Opens Manage configurations…, the last entry of File › Open ›.
fn manage(app: &mut App) {
    keys(app, &["Alt-F", "o", "Up", "Enter"]);
    assert_eq!(app.node(), "any.menu.file.manage");
}

#[test]
fn manage_never_deletes_the_open_configuration_and_never_touches_etc() {
    for machine in Machine::BOTH {
        let (mut app, ..) = app(machine);
        let open = app.shared().document().name().to_owned();
        let installed = app.shared().document().installed().clone();
        let kept = app.shared().documents.desk().configurations();
        manage(&mut app);
        let Some(Sheet::Manage(m)) = top(&app) else {
            panic!("the sheet is open");
        };
        assert_eq!(m.highlighted().map(|k| k.name.clone()), Some(open.clone()));
        for key in ["d", "Delete"] {
            keys(&mut app, &[key]);
            assert_eq!(app.node(), "any.menu.file.manage", "{key} asks nothing");
            assert!(text(&app).contains("open another first"), "{key}");
        }
        if machine == Machine::Large {
            keys(&mut app, &["Down", "d"]);
            assert_eq!(app.node(), "any.question.confirm");
            assert!(text(&app).contains("/etc is untouched"), "{}", text(&app));
            press(&mut app, Key::char('y'));
        }
        assert_eq!(app.shared().document().name(), open);
        assert_eq!(app.shared().document().installed(), &installed);
        assert_eq!(app.shared().documents.desk().configurations(), kept);
    }
}

#[test]
fn manage_opens_the_highlighted_one_and_rename_refuses_a_name_in_use() {
    let (mut app, ..) = app(Machine::Large);
    manage(&mut app);
    keys(&mut app, &["Down", "r"]);
    assert_eq!(app.node(), "any.menu.file.manage.typing");
    keys(&mut app, &["Backspace", "Backspace"]);
    typed(&mut app, "03");
    press(&mut app, plain(Code::Enter));
    assert!(text(&app).contains("lab-03 is taken"), "{}", text(&app));
    keys(&mut app, &["Esc", "Enter"]);
    assert_eq!(app.shared().document().name(), "lab-01");
    assert!(app.nav().sheets().is_empty());
}

// The screens.

/// Draws the screen that `setup` reaches on both machines at both sizes,
/// checks that each row is the screen's width, that no mark is carried by
/// color alone and that `musts` show, and compares the four with the
/// snapshot `name`.
fn screens(name: &str, setup: &dyn Fn(&mut App), musts: &[&str]) {
    let mut out = String::new();
    for machine in Machine::BOTH {
        for size in SIZES {
            let (mut color, ..) = app_in(machine, size, true);
            setup(&mut color);
            let (mut mono, ..) = app_in(machine, size, false);
            setup(&mut mono);
            let (color, mono) = (draw(&color), draw(&mono));
            check(name, machine, size, &color, &mono, musts, &mut out);
        }
    }
    common::snapshot(name, &out);
}

fn check(
    name: &str,
    machine: Machine,
    size: (u16, u16),
    color: &Buffer,
    mono: &Buffer,
    musts: &[&str],
    out: &mut String,
) {
    let what = format!("{name} on {} at {}×{}", machine.name(), size.0, size.1);
    common::rows_fit(color, &what);
    common::marks_survive_without_color(color, mono, &what);
    let screen = common::rows(color).join("\n");
    for must in musts {
        assert!(
            screen.contains(must),
            "{what}: {must:?} is missing:\n{screen}"
        );
    }
    out.push_str(&format!("== {what}\n"));
    out.push_str(&common::snapshot_text(color));
}

#[test]
fn f1_s_list_of_keys_at_both_sizes_with_both_fixtures() {
    screens(
        "sheets-about-keys",
        &|app| {
            press(app, plain(Code::F(1)));
        },
        &["Keys of Fonts", "[Esc] Close"],
    );
}

#[test]
fn f1_s_full_text_at_both_sizes_with_both_fixtures() {
    screens(
        "sheets-about-text",
        &|app| {
            keys(app, &["Down", "F1"]);
        },
        &["Fonts", "It covers Latin", "[Esc] Close"],
    );
}

#[test]
fn find_at_both_sizes_with_both_fixtures() {
    screens(
        "sheets-find",
        &|app| {
            press(app, Key::ctrl('f'));
            typed(app, "*mono");
        },
        &["Find *mono", "Cullet Mono", "found in Fonts", "[Enter] Go"],
    );
}

#[test]
fn find_with_nothing_typed_at_both_sizes_with_both_fixtures() {
    screens(
        "sheets-find-empty",
        &|app| {
            press(app, Key::ctrl('f'));
        },
        &["Find searches the names in Fonts and Flow.", "[Esc] Close"],
    );
}

#[test]
fn the_problems_list_with_none_at_both_sizes_with_both_fixtures() {
    screens(
        "sheets-problems-none",
        &|app| {
            press(app, Key::alt('p'));
        },
        &["Problems", "No problems", "[Esc] Close"],
    );
}

#[test]
fn the_problems_list_in_its_order_at_both_sizes() {
    let mut out = String::new();
    for size in SIZES {
        let draw_it = |color: bool| {
            let area = Rect::new(0, 0, size.0, size.1);
            let regions = Regions::of(
                area,
                Options {
                    search_rows: 0,
                    message: false,
                },
            );
            let mut list = vec![inactive_fonts(1)];
            list.extend(three());
            let sheet = Sheet::Problems(Problems::new(list));
            let mut buf = Buffer::empty(area);
            sheet.draw(&mut buf, &sheet.place(&regions), color);
            buf
        };
        let (color, mono) = (draw_it(true), draw_it(false));
        check(
            "sheets-problems",
            Machine::Map,
            size,
            &color,
            &mono,
            &[
                "1 stops Save",
                "Stop Save",
                "Warnings",
                "Looks › big",
                "tab order",
            ],
            &mut out,
        );
    }
    common::snapshot("sheets-problems", &out);
}

#[test]
fn file_new_at_both_sizes_with_both_fixtures() {
    screens(
        "sheets-new",
        &|app| {
            new_named(app, "kiosk-test");
            press(app, plain(Code::Down));
        },
        &[
            "New configuration",
            "kiosk-test",
            "from what is saved",
            "[Enter] Make",
        ],
    );
}

#[test]
fn manage_configurations_at_both_sizes_with_both_fixtures() {
    screens(
        "sheets-manage",
        &|app| {
            manage(app);
            keys(app, &["d"]);
        },
        &[
            "Manage configurations",
            "open now",
            "open another first",
            "[Del] Delete…",
        ],
    );
}

#[test]
fn manage_s_rename_at_both_sizes_with_both_fixtures() {
    screens(
        "sheets-manage-rename",
        &|app| {
            manage(app);
            keys(app, &["r"]);
            typed(app, "-2026");
        },
        &["-2026_", "[Enter] Keep", "[Esc] Cancel"],
    );
}

#[test]
fn manage_s_delete_question_at_both_sizes() {
    let mut out = String::new();
    for size in SIZES {
        let reach = |app: &mut App| {
            manage(app);
            keys(app, &["Down", "Delete"]);
        };
        let (mut color, ..) = app_in(Machine::Large, size, true);
        reach(&mut color);
        let (mut mono, ..) = app_in(Machine::Large, size, false);
        reach(&mut mono);
        check(
            "sheets-manage-delete",
            Machine::Large,
            size,
            &draw(&color),
            &draw(&mono),
            &["Delete lab-01?", "[y] Yes"],
            &mut out,
        );
    }
    common::snapshot("sheets-manage-delete", &out);
}
