//! Show changes and Edit as text. Each test drives the App with a key or a
//! click, over the screen map's machine, the large machine or the machine
//! of ADR-1403 D11's figure, and checks the state, the screen and the
//! effects that come out. The editor's own path runs real scripts as the
//! editor and as `vtium check-config`, under the test's own folder.
//!
//! The stand-in tabs here are a Flow tab, whose node gives the Flow view's
//! `v`, `r` and `o`, and a Looks tab, each drawing one line.
//!
//! The screens are kept as snapshots under `tests/snapshots/`, each with
//! four screens: the screen map's machine and the large machine, each at
//! 100×30 and at 80×25.

#![expect(clippy::expect_used, reason = "a test fails by panicking")]

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier};
use ratatui_core::terminal::Frame;
use vtium_config_core::document::{
    self, Base, Content, Document, Documents, Edit, InMemory, Installed,
};
use vtium_config_core::ui::{Action, Command, Effect, Shared};
use vtium_config_tui::BoxTab;
use vtium_config_tui::app::{App, Event, Size};
use vtium_config_tui::changes::draw::{self as shown, Shown};
use vtium_config_tui::changes::{Changes, words};
use vtium_config_tui::editor::{self, Edited};
use vtium_config_tui::port::Wake;
use vtium_config_tui::sheets::Sheet;
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::tab::Tab;

use common::{Machine, SIZES, click, draw, keys, locate, press, rows, text};

/// The stand-in Flow tab: its node gives `v`, `r` and `o`.
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

/// The App over `shared` at `size`, on the Flow tab.
fn app_on(shared: Shared, (cols, rows): (u16, u16), color: bool) -> App {
    let tabs: Vec<BoxTab> = vec![Box::new(Flow), Box::new(Looks)];
    App::new(tabs, shared, Size::grid(cols, rows), color)
}

fn app_in(machine: Machine, size: (u16, u16), color: bool) -> App {
    app_on(machine.shared(), size, color)
}

/// ADR-1403 D11's figure of Show changes: the installed file, and the
/// document with three settings added at the top level, the look's size
/// changed from 10 to 11, a head added, and JetBrains Mono activated.
const FIGURE_OLD: &str = concat!(
    "# This is vtium's configuration file, /etc/vtium/vtium.conf. It holds only\n",
    "# the settings you change, and vtium uses its default for every other setting.\n",
    "#\n",
    "# /etc/vtium/vtium.conf.example describes every setting and shows its default.\n",
    "# vtium --help lists every setting with a one-line description.\n",
    "\n",
    "\n",
    "# My-Look: the laptop's own panel.\n",
    "# It draws at 10 pt.\n",
    "[look My-Look]\n",
    "palette = vga\n",
    "font-size = 10\n",
    "font-name = cullet-mono\n",
    "\n",
);

const JETBRAINS: &str = "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf";

fn figure_new() -> String {
    FIGURE_OLD
        .replacen(
            "\n\n\n",
            "\n\nfont-size=11\nfont-name=unifont\nosd=full\n\n",
            1,
        )
        .replace("font-size = 10", "font-size = 11")
        + "[head console]\nlook = My-Look\n"
}

/// The figure's machine: `north` holds the figure's text and activates
/// JetBrains Mono, against the figure's installed file.
fn figure() -> Shared {
    let content = Content {
        text: figure_new(),
        base: Base::of(Some(FIGURE_OLD)),
        ..Content::default()
    };
    let desk = InMemory::new().with_draft("north", content.clone());
    let document = Document::new(
        "north",
        content,
        Installed {
            text: Some(FIGURE_OLD.to_owned()),
            frits: BTreeMap::new(),
        },
    );
    let mut shared = Shared::new(
        Documents::new(document, Box::new(desk)),
        vtium_config_core::flowmodel::Present::default(),
    );
    shared
        .change(document::Change::new(
            "JetBrains Mono active",
            Edit::Active {
                path: PathBuf::from(JETBRAINS),
                change: Some(vtium_config_core::batch::Change::Activate),
            },
        ))
        .expect("activated");
    shared
}

/// The viewer of ADR-1403 D11's figure, rows 2 to 27 of 100×30.
const FIGURE: &str =
    " Changes not saved  against /etc/vtium/vtium.conf                 ✓ Changes only    Whole file

 @@ settings
   7 +font-size=11
   8 +font-name=unifont
   9 +osd=full

 @@ [look My-Look]
    13  [look My-Look]
    14  palette = vga
  15 -font-size = 10
  15 +font-size = 11
    16  font-name = cullet-mono

 @@ [head console]
  18 +[head console]
  19 +look = My-Look

 Active fonts
     +/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf






";

fn top(app: &App) -> Option<&Sheet> {
    app.nav().sheets().last()
}

fn changes(app: &App) -> &Changes {
    match top(app) {
        Some(Sheet::Changes(c)) => c,
        other => panic!("Show changes is not on top: {other:?}"),
    }
}

fn message(app: &App) -> Option<String> {
    app.shared().message.as_ref().map(|m| m.line.clone())
}

/// File › Show changes, by the menu's letter.
fn show_changes(app: &mut App) {
    keys(app, &["Alt-F", "c"]);
    assert_eq!(app.node(), "any.changes");
}

fn trimmed(buf: &Buffer) -> Vec<String> {
    rows(buf)
        .into_iter()
        .map(|r| r.trim_end().to_owned())
        .collect()
}

// Show changes.

#[test]
fn show_changes_draws_adr_1403_d11_s_figure_line_for_line() {
    let mut app = app_on(figure(), (100, 30), true);
    show_changes(&mut app);
    let buf = draw(&app);
    let shown = trimmed(&buf);
    let viewer: Vec<&str> = shown.iter().skip(2).take(26).map(String::as_str).collect();
    let want: Vec<&str> = FIGURE.split('\n').take(26).collect();
    assert_eq!(viewer, want, "\n{}", shown.join("\n"));
    assert_eq!(
        shown.last().map(String::as_str),
        Some(
            " [w] Whole file  [e] Edit as text  [Esc] Back  [Ctrl+S] Save                               [q] Quit"
        )
    );
    // the colors: context plain, the removed line red, the added green,
    // and only the changed word bold
    let fg = |x: u16, y: u16| buf.cell((x, y)).expect("a cell").fg;
    let bold = |x: u16, y: u16| {
        buf.cell((x, y))
            .expect("a cell")
            .modifier
            .contains(Modifier::BOLD)
    };
    let (x, y) = locate(&app, "  15 -font-size = 10");
    assert_eq!(fg(x + 6, y), Color::Red);
    assert_eq!(fg(x + 6, y + 1), Color::Green);
    assert!(bold(x + 18, y) && bold(x + 18, y + 1), "10 and 11");
    assert_eq!(
        fg(x + 18, y),
        Color::Red,
        "a changed word keeps its line's color"
    );
    assert!(!bold(x + 6, y), "font-size is no changed word");
    let (x, y) = locate(&app, "    13  [look My-Look]");
    assert_eq!(fg(x + 8, y), Color::Reset, "context");
    assert_eq!(fg(locate(&app, "   7 +").0 + 3, 5), Color::Green);
}

#[test]
fn w_and_a_click_on_the_pair_switch_to_the_whole_file_and_back() {
    let mut app = app_on(figure(), (100, 30), true);
    show_changes(&mut app);
    keys(&mut app, &["w"]);
    assert!(changes(&app).whole());
    let screen = text(&app);
    assert!(
        screen.contains(" north's vtium.conf  as it will be saved"),
        "{screen}"
    );
    assert!(screen.contains("  Changes only  ✓ Whole file"), "{screen}");
    assert!(screen.contains("[w] Changes only"), "{screen}");
    // every line of the document, the removed one where it stood
    let file: Vec<&str> = changes(&app)
        .lines()
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(file.len(), figure_new().lines().count() + 1);
    assert!(file.contains(&"  15 -font-size = 10"));
    assert!(file.contains(
        &"     1  # This is vtium's configuration file, /etc/vtium/vtium.conf. It holds only"
    ));
    // a click on Changes only goes back, and one on Whole file again
    let (x, y) = locate(&app, "Changes only  ✓");
    click(&mut app, x + 4, y);
    assert!(!changes(&app).whole());
    let (x, y) = locate(&app, "Whole file");
    click(&mut app, x + 2, y);
    assert!(changes(&app).whole());
    // a click on the title does nothing
    click(&mut app, 3, y);
    assert!(changes(&app).whole());
}

#[test]
fn the_file_view_s_keys_scroll_and_the_last_row_says_which_lines_show() {
    let mut app = app_in(Machine::Large, (80, 25), true);
    show_changes(&mut app);
    keys(&mut app, &["w"]);
    let _ = draw(&app);
    let len = changes(&app).lines().len();
    // 21 rows of viewer: the title, a blank row, 18 lines and the count
    let rows = 18;
    assert!(len > rows, "the large machine's file runs past one screen");
    let screen = text(&app);
    assert!(
        screen.contains(&format!("lines 1 to {rows} of {len}")),
        "{screen}"
    );
    let at = |app: &App| changes(app).top();
    for (key, want) in [
        ("Down", 1),
        ("j", 2),
        ("k", 1),
        ("Up", 0),
        ("Up", 0),
        ("PgDn", rows),
        ("PgUp", 0),
        ("Ctrl-D", rows / 2),
        ("Ctrl-U", 0),
        ("End", len - rows),
        ("Down", len - rows),
        ("Home", 0),
        ("G", len - rows),
        ("g", 0),
    ] {
        keys(&mut app, &[key]);
        let _ = draw(&app);
        assert_eq!(at(&app), want, "after {key}");
    }
    keys(&mut app, &["End"]);
    let screen = text(&app);
    assert!(
        screen.contains(&format!("lines {} to {len} of {len}", len - rows + 1)),
        "{screen}"
    );
}

#[test]
fn ctrl_z_in_show_changes_leaves_it_open_with_one_change_fewer() {
    for machine in Machine::BOTH {
        let mut app = app_in(machine, (100, 30), true);
        // a second change, so one remains after the undo
        app.event(Event::Edited(Ok(Edited {
            text: format!("{}# a note\n", app.shared().document().text()),
            ..Edited::default()
        })));
        show_changes(&mut app);
        assert!(text(&app).contains("+# a note"));
        keys(&mut app, &["Ctrl-Z"]);
        assert_eq!(app.node(), "any.changes");
        let screen = text(&app);
        assert!(!screen.contains("+# a note"), "{screen}");
        assert!(screen.contains("@@"), "the fixture's own change stays");
        keys(&mut app, &["Ctrl-Y"]);
        assert!(text(&app).contains("+# a note"));
    }
}

#[test]
fn e_runs_the_editor_esc_goes_back_and_ctrl_s_acts_by_save_s_row() {
    let mut app = app_in(Machine::Map, (100, 30), true);
    show_changes(&mut app);
    let document = app.shared().document();
    let want = Effect::Editor {
        name: document.name().to_owned(),
        text: document.text().to_owned(),
    };
    assert_eq!(keys(&mut app, &["e"]), [want]);
    assert_eq!(
        app.node(),
        "any.changes",
        "Show changes stays under the editor"
    );
    assert_eq!(
        keys(&mut app, &["Ctrl-S"]),
        [Effect::Desk(vtium_config_core::ui::Op::Prepare)],
        "Save's row hands the loop the read its review starts from"
    );
    keys(&mut app, &["Esc"]);
    assert_eq!(app.node(), "flow");
}

#[test]
fn v_in_flow_and_a_click_on_the_count_not_saved_open_show_changes() {
    let mut app = app_in(Machine::Map, (100, 30), true);
    keys(&mut app, &["v"]);
    assert_eq!(app.node(), "any.changes");
    keys(&mut app, &["Esc"]);
    let (x, y) = locate(&app, "1 not saved");
    click(&mut app, x, y);
    assert_eq!(app.node(), "any.changes");
    assert!(!changes(&app).whole());
}

#[test]
fn a_line_of_vtium_conf_opens_the_whole_file_at_that_line() {
    let shared = Machine::Large.shared();
    let at = Changes::of_document(&shared).at_line(40);
    assert!(at.whole());
    let line = at.lines().get(at.top() + 2).expect("the line");
    assert_eq!(line.line, Some(40));
}

#[test]
fn the_whole_file_marks_each_line_the_check_complains_about() {
    let lines = vtium_config_core::versions::diff::whole_file(FIGURE_OLD, &figure_new());
    let file = shown::file(&lines, &[15]);
    let marked: Vec<&Shown> = file.iter().filter(|s| s.complained).collect();
    assert_eq!(marked.len(), 1);
    assert_eq!(
        marked.first().map(|s| s.text.as_str()),
        Some("! 15 +font-size = 11")
    );
    // the mark is a character, and its number is in the problems' color
    let area = Rect::new(0, 0, 40, 4);
    let mut buf = Buffer::empty(area);
    let page = shown::Page {
        title: "t",
        grey: "",
        whole: true,
        lines: &[(*marked.first().expect("marked")).clone()],
        top: 0,
    };
    shown::draw(&mut buf, area, &page, true);
    let cell = |x| buf.cell((x, 2)).expect("a cell").clone();
    assert_eq!(cell(0).symbol(), "!");
    assert_eq!(cell(3).fg, Color::LightRed);
    assert_eq!(cell(6).fg, Color::Green);
}

#[test]
fn the_font_set_changes_and_the_active_fonts_follow_the_hunks() {
    let mut review = document::Review::default();
    assert_eq!(
        shown::fonts(&review, 3)
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>(),
        ["", " Fonts unchanged"]
    );
    review.font_sets = vec![
        ("mine".to_owned(), vtium_config_core::drafts::FritEdit::Add),
        (
            "old".to_owned(),
            vtium_config_core::drafts::FritEdit::Remove,
        ),
        (
            "set".to_owned(),
            vtium_config_core::drafts::FritEdit::Replace,
        ),
    ];
    review.active = vec![(
        PathBuf::from("/usr/share/fonts/x.otf"),
        vtium_config_core::batch::Change::Deactivate,
    )];
    assert_eq!(
        shown::fonts(&review, 3)
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>(),
        [
            "",
            " Font sets",
            "     +mine",
            "     -old",
            "     +set · replaces the installed one",
            "",
            " Active fonts",
            "     -/usr/share/fonts/x.otf",
        ]
    );
}

// Edit as text.

/// A folder of the test's own, emptied first.
fn folder(test: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("changes-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the folder");
    dir
}

/// An executable shell script at `path`.
fn script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).expect("the script");
    let mut perms = std::fs::metadata(path).expect("the script").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("executable");
}

/// A machine under `dir`: an installed vtium.conf, an empty store, and a
/// stand-in `vtium` whose `check-config` fails on a size with a unit, as
/// vtium's own check does, and passes everything else.
fn machine(
    dir: &Path,
) -> (
    vtium_config_core::places::Paths,
    vtium_config_core::store::Store,
) {
    let etc = dir.join("etc");
    std::fs::create_dir_all(&etc).expect("etc");
    std::fs::write(etc.join("vtium.conf"), "version = 1\n").expect("vtium.conf");
    let vtium = dir.join("vtium");
    script(
        &vtium,
        r#"for last; do :; done
if grep -q 'pt$' "$last"; then
  echo "vtium.conf line 2: font-size = 12pt needs a number of points" >&2
  exit 1
fi
exit 0"#,
    );
    let paths = vtium_config_core::places::Paths {
        configdir: etc,
        frits: Err("no font sets on this machine".to_owned()),
        drafts: dir.join("bundles"),
        sudo: "sudo".into(),
        vtium: vtium.into_os_string(),
        helper: vtium_config_core::specimen::Helper::at(dir.join("no-helper")),
        sys: dir.join("sys"),
    };
    let store = vtium_config_core::store::Store {
        config: dir.join("config"),
        state: dir.join("state"),
    };
    (paths, store)
}

/// Edits "version = 1\n" in the editor `body`, a script that gets the
/// copy's path as `$1`.
fn edit_with(test: &str, body: &str) -> Result<Edited, String> {
    let dir = folder(test);
    let (paths, store) = machine(&dir);
    let editor = dir.join("editor");
    script(&editor, body);
    let session = vtium_config_core::drafts::Session {
        store,
        paths: &paths,
    };
    let shown = editor.display().to_string();
    let result = editor::run(&session, "north", "version = 1\n", &|k| {
        (k == "EDITOR").then(|| shown.clone())
    });
    // the copy is gone, whatever the editor did
    let left: Vec<_> = std::fs::read_dir(dir.join("state"))
        .map(|d| d.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "the copy is left: {left:?}");
    result
}

#[test]
fn edit_as_text_with_an_editor_that_writes_a_known_text() {
    let edited =
        edit_with("known", r#"printf 'version = 1\nfont-size = 12\n' > "$1""#).expect("edited");
    assert_eq!(
        edited,
        Edited {
            text: "version = 1\nfont-size = 12\n".to_owned(),
            problems: Vec::new(),
            count: 0,
        }
    );
}

#[test]
fn edit_as_text_with_an_editor_that_leaves_an_error() {
    let edited = edit_with(
        "error",
        r#"printf 'version = 1\nfont-size = 12pt\n' > "$1""#,
    )
    .expect("edited");
    assert_eq!(edited.text, "version = 1\nfont-size = 12pt\n");
    assert_eq!(edited.count, 1);
    assert_eq!(
        edited.problems,
        ["vtium.conf line 2: font-size = 12pt needs a number of points"]
    );
}

#[test]
fn an_editor_that_fails_or_changes_nothing_leaves_the_text_unchecked() {
    let failed = edit_with("fails", "exit 3");
    assert_eq!(
        failed,
        Err(editor::words::failed(&format!(
            "{}/editor",
            folder_name("fails")
        )))
    );
    let same = edit_with("same", "exit 0").expect("ran");
    assert_eq!(same.text, "version = 1\n");
    assert!(same.problems.is_empty());
}

fn folder_name(test: &str) -> String {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("changes-{test}-{}", std::process::id()))
        .display()
        .to_string()
}

#[test]
fn the_text_the_editor_brings_back_is_one_change_and_ctrl_z_takes_it_back() {
    for machine in Machine::BOTH {
        let mut app = app_in(machine, (100, 30), true);
        let before = app.shared().document().content().clone();
        let made = app.shared().documents.history().made();
        let text = format!("{}# edited\n", before.text);
        app.event(Event::Edited(Ok(Edited {
            text: text.clone(),
            ..Edited::default()
        })));
        assert_eq!(app.shared().document().text(), text);
        assert_eq!(app.shared().documents.history().made(), made + 1);
        assert_eq!(
            message(&app).as_deref(),
            Some("Edit as text: 1 line added, not saved. [Ctrl+Z] takes it back.")
        );
        assert_eq!(app.node(), "flow", "no question when the check passes");
        keys(&mut app, &["Ctrl-Z"]);
        assert_eq!(app.shared().document().content(), &before);
        assert_eq!(
            message(&app).as_deref(),
            Some("Undid Edit as text. [Ctrl+Y] redoes it.")
        );
    }
}

#[test]
fn a_failed_check_asks_to_edit_again_and_the_document_keeps_the_text_either_way() {
    let mut app = app_in(Machine::Map, (100, 30), true);
    let bad = "version = 1\nfont-size = 12pt\n".to_owned();
    let returned = |text: &str| {
        Event::Edited(Ok(Edited {
            text: text.to_owned(),
            problems: vec!["vtium.conf line 2: font-size = 12pt needs a number".to_owned()],
            count: 1,
        }))
    };
    app.event(returned(&bad));
    assert_eq!(app.node(), editor::QUESTION);
    assert_eq!(app.shared().document().text(), bad, "kept while it asks");
    let screen = text(&app);
    assert!(
        screen.contains("The check found 1 problem: vtium.conf line 2: font-size = 12pt needs a number. Edit again?"),
        "{screen}"
    );
    // y opens the editor again on the text kept
    let again = keys(&mut app, &["y"]);
    assert_eq!(
        again,
        [Effect::Editor {
            name: "workstation".to_owned(),
            text: bad.clone(),
        }]
    );
    // a second return is a step of its own; Esc keeps it
    let worse = format!("{bad}osd = loud\n");
    app.event(returned(&worse));
    assert_eq!(app.node(), editor::QUESTION);
    keys(&mut app, &["Esc"]);
    assert_eq!(app.node(), "flow");
    assert_eq!(app.shared().document().text(), worse);
    keys(&mut app, &["Ctrl-Z"]);
    assert_eq!(app.shared().document().text(), bad);
}

#[test]
fn an_editor_that_did_not_run_or_changed_nothing_only_says_so() {
    let mut app = app_in(Machine::Map, (100, 30), true);
    let made = app.shared().documents.history().made();
    app.event(Event::Edited(Err(editor::words::failed("vi"))));
    assert_eq!(
        message(&app).as_deref(),
        Some("The editor vi failed, so the configuration is unchanged.")
    );
    let same = app.shared().document().text().to_owned();
    app.event(Event::Edited(Ok(Edited {
        text: same,
        ..Edited::default()
    })));
    assert_eq!(message(&app).as_deref(), Some(editor::words::UNCHANGED));
    assert_eq!(app.shared().documents.history().made(), made);
}

/// A machine whose editor writes `text` and whose check passes, which
/// keeps what it was handed.
struct Writes {
    text: String,
    handed: Vec<(String, String)>,
}

impl vtium_config_tui::live::machine::Machine for Writes {
    fn devices(&mut self) -> Option<vtium_config_core::flowmodel::Present> {
        None
    }
    fn ready_font(
        &mut self,
        _source: &Path,
    ) -> Result<vtium_config_tui::live::machine::Ready, String> {
        Err("none".to_owned())
    }
    fn install_font(&mut self) -> Result<(), String> {
        Err("none".to_owned())
    }
    fn edit(&mut self, name: &str, text: &str) -> Result<Edited, String> {
        self.handed.push((name.to_owned(), text.to_owned()));
        Ok(Edited {
            text: self.text.clone(),
            ..Edited::default()
        })
    }
}

#[test]
fn the_loop_runs_the_editor_while_the_tui_steps_aside_and_draws_the_next_frame_whole() {
    let app = app_in(Machine::Map, (100, 30), true);
    let start = app.shared().document().text().to_owned();
    let mut machine = Writes {
        text: format!("{start}# from the editor\n"),
        handed: Vec::new(),
    };
    let size = Size::grid(100, 30);
    // File › Edit as text…, then q
    let mut port = common::Script::new(
        size,
        vec![
            vec![Wake::Bytes(b"\x1bfe".to_vec())],
            vec![Wake::Bytes(b"q".to_vec())],
        ],
    );
    let left = vtium_config_tui::port::drive_on(&mut port, &mut machine, app).expect("ran");
    assert_eq!(port.asides, 1);
    assert_eq!(machine.handed, [("workstation".to_owned(), start)]);
    // the frame after the editor starts from a cleared screen and draws
    // the frame whole again
    let cleared = common::last_at(&port.written, b"\x1b[2J").expect("a clear");
    let after = port.written.get(cleared..).expect("the bytes after");
    assert!(common::at(after, b"ctions").is_some());
    assert!(common::at(after, b"diagram").is_some());
    assert_eq!(
        left.line.as_deref(),
        Some(
            "vtium-config: workstation has 2 changes, not saved. vtium config tui opens them again."
        )
    );
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
fn show_changes_at_both_sizes_with_both_fixtures() {
    screens(
        "changes-only",
        &|app| show_changes(app),
        &[
            "Changes not saved",
            "✓ Changes only",
            "@@ ",
            "Fonts unchanged",
            "[w] Whole file",
            "[Esc] Back",
        ],
    );
}

#[test]
fn show_changes_whole_file_at_both_sizes_with_both_fixtures() {
    screens(
        "changes-whole-file",
        &|app| {
            show_changes(app);
            keys(app, &["w"]);
        },
        &[
            "'s vtium.conf",
            "✓ Whole file",
            "[w] Changes only",
            "+font-size = 11",
        ],
    );
}

#[test]
fn show_changes_with_nothing_changed_at_both_sizes_with_both_fixtures() {
    screens(
        "changes-nothing",
        &|app| keys_then(app, &["r", "y", "v"]),
        &[words::NOTHING],
    );
}

#[test]
fn the_edit_again_question_at_both_sizes_with_both_fixtures() {
    screens(
        "changes-edit-again",
        &|app| {
            app.event(Event::Edited(Ok(Edited {
                text: "version = 1\nfont-size = 12pt\n".to_owned(),
                problems: vec!["vtium.conf line 2: font-size = 12pt needs a number".to_owned()],
                count: 1,
            })));
        },
        &["The check found 1 problem", "[y] Yes"],
    );
}

#[test]
fn every_row_of_show_changes_is_the_screen_s_width_with_a_long_line() {
    let mut shared = Machine::Map.shared();
    let long = format!("{}# {}\n", shared.document().text(), "x".repeat(300));
    shared
        .change(document::Change::new("long", Edit::Text(long)))
        .expect("made");
    for size in SIZES {
        let mut app = app_on(shared_clone(&shared), size, true);
        show_changes(&mut app);
        common::rows_fit(&draw(&app), "a long line");
        let _ = press(&mut app, Key::plain(Code::Down));
    }
}

/// Presses `names`, dropping the effects.
fn keys_then(app: &mut App, names: &[&str]) {
    let _ = keys(app, names);
}

/// A shared state with the document of `shared`, on a desk of its own.
fn shared_clone(shared: &Shared) -> Shared {
    let document = shared.document().clone();
    let desk = InMemory::new().with_draft(document.name(), document.content().clone());
    Shared::new(
        Documents::new(document, Box::new(desk)),
        vtium_config_core::flowmodel::Present::default(),
    )
}
