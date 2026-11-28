//! The seams the shell drives every tab through: the `?` search it holds
//! and hands the tab, the labels a tab gives its buttons, a list box's own
//! search row, F1's list of keys from the Help tab's topic, the preview it
//! asks the pictures for, and the check and the walk's reads that the
//! loop performs.
//!
//! References:
//! - ADR-1206 D3, "One search component": the navigation state holds the
//!   search, and each tab keeps the indexes that match.
//! - ADR-1206 D9: the shell reaches a tab only through the `Tab` trait.
//! - ADR-1403 D11, "F1, help for what is highlighted": F1's list of every
//!   key of a screen is the Help tab's topic for that tab.
//! - ADR-1201 D4: the preview of a font as a picture of the console.
//! - ADR-1205 D4: the walk of the fonts in use, never on a key.

#![expect(clippy::expect_used, reason = "a test fails by panicking")]

mod common;

use std::borrow::Cow;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use ratatui_core::layout::Rect;
use ratatui_core::terminal::Frame;
use vtium_config_core::document::{self, Document, Edit};
use vtium_config_core::drafts::{Checked, Verdict};
use vtium_config_core::flowmodel::Present;
use vtium_config_core::preview::{Place, Previewed, Shows};
use vtium_config_core::ui::{Action, Change, Command, Effect, Live, Shared};
use vtium_config_tui::BoxTab;
use vtium_config_tui::app::{App, Event, Size};
use vtium_config_tui::live::machine::{Machine as Desk, Ready};
use vtium_config_tui::port::{self, Port, Wake};
use vtium_config_tui::sheets::Sheet;
use vtium_config_tui::view;
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::Search;
use vtium_config_widgets::tab::{Searchable, Tab};

use common::{FONTS, Machine, plain, press, text};

/// What the shell handed the seamed tab, and what the test makes it answer.
#[derive(Default)]
struct Seen {
    /// Each search the shell handed, as its matches and whether the
    /// person still typed, `None` for a cleared filter.
    handed: Vec<Option<(Vec<usize>, bool)>>,
    /// The item the list highlights, which the search opens and resumes on.
    highlighted: Option<usize>,
    /// What the tab previews.
    preview: Option<(&'static str, Place)>,
}

/// A Fonts tab that answers every seam from what the test sets in `seen`.
struct Seamed {
    seen: Rc<RefCell<Seen>>,
}

impl Tab<Shared, Action, Command> for Seamed {
    fn name(&self) -> &'static str {
        "Fonts"
    }
    fn letter(&self) -> char {
        'n'
    }
    fn node(&self) -> &'static str {
        "fonts"
    }
    fn draw(&self, _shared: &Shared, _frame: &mut Frame<'_>, _area: Rect) {}
    fn update(&mut self, shared: &Shared, action: Action) -> Vec<Command> {
        match action.row {
            "space.view.resources" => {
                let text = format!("{}# a change\n", shared.document().text());
                vec![Command::Change(Change::Document(document::Change::new(
                    "a change",
                    Edit::Text(text),
                )))]
            }
            _ => Vec::new(),
        }
    }
    fn bar_labels(&self, shared: &Shared) -> Vec<(&'static str, String)> {
        vec![(
            "space.view.resources",
            format!("Mark {}", shared.document().name()),
        )]
    }
    fn searchable(&self) -> Option<&dyn Searchable> {
        Some(self)
    }
    fn filtered(&mut self, _shared: &Shared, search: Option<&Search>) {
        self.seen
            .borrow_mut()
            .handed
            .push(search.map(|s| (s.matches().to_vec(), s.typing())));
    }
    fn preview<'a>(&self, _shared: &'a Shared) -> Option<Previewed<'a>> {
        let (font, place) = self.seen.borrow().preview?;
        Some(Previewed {
            font: Some(font.to_owned()),
            shows: Shows::Font { detail: false },
            in_document: None,
            place: Some(place),
        })
    }
    fn keys_help(&self, node: &'static str, width: u16) -> Option<Vec<(String, Role)>> {
        (node == "fonts").then(|| {
            vec![
                ("Fonts".to_owned(), Role::Heading),
                (format!("[x] the sample key, in {width} columns"), Role::Row),
            ]
        })
    }
}

impl Searchable for Seamed {
    fn search_node(&self) -> &'static str {
        "fonts"
    }
    fn count(&self) -> usize {
        FONTS.len()
    }
    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>> {
        let (family, file) = FONTS.get(item)?;
        match field {
            0 => Some(Cow::Borrowed(family)),
            1 => Some(Cow::Borrowed(file)),
            _ => None,
        }
    }
    fn highlighted(&self) -> Option<usize> {
        self.seen.borrow().highlighted
    }
}

/// A Looks tab whose list box has a search of its own open.
struct ListBoxed {
    search: Search,
}

impl Tab<Shared, Action, Command> for ListBoxed {
    fn name(&self) -> &'static str {
        "Looks"
    }
    fn letter(&self) -> char {
        'l'
    }
    fn node(&self) -> &'static str {
        "looks"
    }
    fn draw(&self, _shared: &Shared, _frame: &mut Frame<'_>, _area: Rect) {}
    fn update(&mut self, _shared: &Shared, _action: Action) -> Vec<Command> {
        Vec::new()
    }
    fn search(&self) -> Option<&Search> {
        Some(&self.search)
    }
}

/// The App on the screen map's machine at 100×30 with the seamed tab, a
/// Looks tab with a list box's search open, and what the seamed tab saw.
fn app() -> (App, Rc<RefCell<Seen>>) {
    app_on(Machine::Map.shared())
}

fn app_on(shared: Shared) -> (App, Rc<RefCell<Seen>>) {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let mut search = Search::open(
        &Seamed {
            seen: Rc::clone(&seen),
        },
        None,
    );
    for c in "uni".chars() {
        search.key(&Key::char(c));
    }
    let tabs: Vec<BoxTab> = vec![
        Box::new(Seamed {
            seen: Rc::clone(&seen),
        }),
        Box::new(ListBoxed { search }),
    ];
    let app = App::new(tabs, shared, Size::grid(100, 30), true);
    (app, seen)
}

fn typed(app: &mut App, entry: &str) {
    for c in entry.chars() {
        press(app, Key::char(c));
    }
}

fn last_handed(seen: &Rc<RefCell<Seen>>) -> Option<(Vec<usize>, bool)> {
    seen.borrow().handed.last().cloned().flatten()
}

#[test]
fn the_shell_s_search_hands_the_tab_its_matches_after_each_key_and_none_once_it_closes() {
    let (mut app, seen) = app();
    press(&mut app, Key::char('?'));
    assert_eq!(app.node(), "any.search");
    assert_eq!(last_handed(&seen), Some((vec![0, 1, 2, 3], true)));
    typed(&mut app, "mono");
    assert_eq!(
        last_handed(&seen),
        Some((vec![1, 2], true)),
        "Cullet Mono and Noto Sans Mono"
    );
    let handed = seen.borrow().handed.len();
    press(&mut app, plain(Code::Enter));
    assert_eq!(app.node(), "any.filter");
    assert_eq!(last_handed(&seen), Some((vec![1, 2], false)), "kept");
    assert_eq!(seen.borrow().handed.len(), handed + 1);
    // a key that changes nothing of the search hands nothing again
    press(&mut app, Key::char('j'));
    assert_eq!(seen.borrow().handed.len(), handed + 1);
    press(&mut app, plain(Code::Escape));
    assert_eq!(seen.borrow().handed.last(), Some(&None));
    assert!(app.nav().search().is_none());
}

#[test]
fn going_to_another_tab_closes_the_search_and_clears_the_filter_it_kept() {
    let (mut app, seen) = app();
    press(&mut app, Key::char('?'));
    typed(&mut app, "uni");
    press(&mut app, plain(Code::Enter));
    assert_eq!(last_handed(&seen), Some((vec![3], false)));
    press(&mut app, plain(Code::Tab));
    assert_eq!(app.nav().tab(), 1);
    assert!(app.nav().search().is_none(), "the search filtered Fonts");
    assert_eq!(seen.borrow().handed.last(), Some(&None));
}

#[test]
fn the_search_opens_and_resumes_on_the_item_the_tab_highlights() {
    let (mut app, seen) = app();
    seen.borrow_mut().highlighted = Some(2);
    press(&mut app, Key::char('?'));
    assert_eq!(app.nav().search().and_then(Search::highlighted), Some(2));
    typed(&mut app, "o");
    press(&mut app, plain(Code::Enter));
    // the list moved its highlight among the matches while the filter was
    // kept
    seen.borrow_mut().highlighted = Some(1);
    press(&mut app, Key::char('?'));
    assert_eq!(app.node(), "any.search");
    assert_eq!(app.nav().search().and_then(Search::highlighted), Some(1));
}

#[test]
fn the_bar_shows_the_labels_the_tab_gives_its_buttons() {
    let (app, _) = app();
    let shown = view::layout(&app);
    let label = shown
        .bar
        .buttons
        .iter()
        .find(|b| b.action == Some("space.view.resources"))
        .map(|b| b.label.clone());
    assert_eq!(label.as_deref(), Some("Mark workstation"));
    assert!(text(&app).contains("Mark workstation"));
}

#[test]
fn a_list_box_s_own_search_shows_in_the_search_row() {
    let (mut app, _) = app();
    assert!(!text(&app).contains("? uni"), "Fonts has no list box open");
    press(&mut app, plain(Code::Tab));
    let screen = text(&app);
    assert!(screen.contains("? uni"), "{screen}");
}

#[test]
fn f1_lists_the_keys_of_a_screen_from_the_topic_a_tab_gives_for_it() {
    let (mut app, _) = app();
    press(&mut app, plain(Code::F(1)));
    let Some(Sheet::About(about)) = app.nav().sheets().last() else {
        panic!("F1 opens its sheet");
    };
    assert_eq!(about.title, "Keys of Fonts");
    let screen = text(&app);
    assert!(
        screen.contains("[x] the sample key, in 94 columns"),
        "{screen}"
    );
}

#[test]
fn the_shell_asks_the_pictures_for_what_the_open_tab_previews() {
    let mut shared = Machine::Map.shared();
    shared.console.live = Live::NotVtium;
    shared.console.graphics = false;
    let (mut app, seen) = app_on(shared);
    assert!(
        press(&mut app, Key::char('j')).is_empty(),
        "the tab previews nothing"
    );
    seen.borrow_mut().preview = Some(("unifont", Place::new(2, 3, 40, 10)));
    let effects = press(&mut app, Key::char('j'));
    assert!(
        matches!(effects.as_slice(), [Effect::Summary { font, .. }] if font == "unifont"),
        "{effects:?}"
    );
    assert_eq!(
        app.shared()
            .pictures
            .asked
            .as_ref()
            .map(|a| a.wanted.font.as_str()),
        Some("unifont")
    );
}

#[test]
fn a_sheet_over_the_tab_asks_for_no_preview() {
    let mut shared = Machine::Map.shared();
    shared.console.live = Live::NotVtium;
    let (mut app, seen) = app_on(shared);
    press(&mut app, plain(Code::F(1)));
    seen.borrow_mut().preview = Some(("unifont", Place::new(2, 3, 40, 10)));
    assert!(press(&mut app, plain(Code::Down)).is_empty());
    assert_eq!(app.shared().pictures.asked, None);
}

/// What `vtium check-config` says of a document: one warning.
fn checked() -> (Checked, PathBuf) {
    let shown = PathBuf::from(format!(
        "{}/workstation.conf",
        vtium_config_core::fixture::DRAFTS
    ));
    let checked = Checked {
        verdict: Verdict::Failed,
        problems: vec![format!("{}:1: a problem", shown.display())],
        notes: Vec::new(),
    };
    (checked, shown)
}

#[test]
fn the_check_s_answer_counts_for_the_document_it_checked_alone() {
    let (mut app, _) = app();
    assert!(app.check_due(), "the open document was never checked");
    let document = app.shared().document().clone();
    app.event(Event::Checked {
        document: document.clone(),
        found: Ok(checked()),
    });
    assert!(!app.check_due());
    assert_eq!(app.shared().findings().map(|f| f.len()), Some(1));
    press(&mut app, Key::char(' '));
    assert!(app.check_due(), "the change waits for its check");
    assert_eq!(app.shared().findings(), None);
    // an answer for the document before the change does not stand for it
    app.event(Event::Checked {
        document,
        found: Ok(checked()),
    });
    assert!(app.check_due());
    assert_eq!(app.shared().findings(), None);
    let now = app.shared().document().clone();
    app.event(Event::Checked {
        document: now,
        found: Err("the check could not run".to_owned()),
    });
    assert!(
        !app.check_due(),
        "a failed check runs once for the document"
    );
}

/// A terminal that plays its keys, says input is pending while
/// `pending` holds, and counts its waits.
struct Pausing {
    wakes: std::collections::VecDeque<Vec<Wake>>,
    pending: Rc<RefCell<bool>>,
}

impl Port for Pausing {
    fn size(&mut self) -> Size {
        Size::grid(100, 30)
    }
    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<()> {
        Ok(())
    }
    fn arm(&mut self, _after: Duration) {}
    fn wait(&mut self) -> std::io::Result<Vec<Wake>> {
        Ok(self
            .wakes
            .pop_front()
            .unwrap_or_else(|| vec![Wake::Signal(1)]))
    }
    fn pending(&mut self, within: Duration) -> std::io::Result<bool> {
        assert_eq!(within, port::CHECK_QUIET);
        Ok(*self.pending.borrow())
    }
    fn step_aside(
        &mut self,
        aside: &mut dyn FnMut() -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        aside()
    }
}

/// A machine that keeps the text of each document it checked and counts
/// the reads of the walk.
#[derive(Default)]
struct Checking {
    checked: Vec<String>,
    reads: usize,
}

impl Desk for Checking {
    fn devices(&mut self) -> Option<Present> {
        Some(Present::default())
    }
    fn ready_font(&mut self, _source: &std::path::Path) -> Result<Ready, String> {
        Err(String::new())
    }
    fn install_font(&mut self) -> Result<(), String> {
        Err(String::new())
    }
    fn check(&mut self, document: &Document) -> Result<(Checked, PathBuf), String> {
        self.checked.push(document.text().to_owned());
        Ok(checked())
    }
    fn font_reads(&mut self) -> Option<vtium_config_core::ui::shared::FontReads> {
        self.reads += 1;
        Machine::Map.shared().in_use.reads().cloned()
    }
}

/// The bytes of a space, which makes one change, and of `q`, which quits.
fn keys() -> Vec<Vec<Wake>> {
    vec![
        vec![Wake::Bytes(b" ".to_vec())],
        vec![Wake::Bytes(b" ".to_vec())],
        vec![Wake::Bytes(b"q".to_vec())],
    ]
}

#[test]
fn the_loop_checks_each_document_once_the_person_pauses() {
    let (app, _) = app();
    let mut port = Pausing {
        wakes: keys().into(),
        pending: Rc::new(RefCell::new(false)),
    };
    let mut machine = Checking::default();
    port::drive_on(&mut port, &mut machine, app).expect("the loop runs");
    let changes: Vec<usize> = machine
        .checked
        .iter()
        .map(|t| t.matches("# a change").count())
        .collect();
    assert_eq!(
        changes,
        [0, 1, 2],
        "the document at start and after each change"
    );
}

#[test]
fn the_loop_runs_no_check_while_keys_keep_coming() {
    let (app, _) = app();
    let mut port = Pausing {
        wakes: keys().into(),
        pending: Rc::new(RefCell::new(true)),
    };
    let mut machine = Checking::default();
    port::drive_on(&mut port, &mut machine, app).expect("the loop runs");
    assert!(machine.checked.is_empty(), "{:?}", machine.checked);
}

#[test]
fn the_loop_reads_the_walk_s_reads_at_start_and_on_read_devices_again() {
    let (app, _) = app();
    let mut port = Pausing {
        // Alt+A opens Actions, and r is Read devices again
        wakes: vec![
            vec![Wake::Bytes(b"\x1ba".to_vec())],
            vec![Wake::Bytes(b"r".to_vec())],
        ]
        .into(),
        pending: Rc::new(RefCell::new(true)),
    };
    let mut machine = Checking::default();
    port::drive_on(&mut port, &mut machine, app).expect("the loop runs");
    assert_eq!(machine.reads, 2, "at start, then on Read devices again");
}

#[test]
fn the_walk_s_reads_reach_the_shared_state() {
    let reads = Machine::Map
        .shared()
        .in_use
        .reads()
        .cloned()
        .expect("the fixture's reads");
    let mut bare = Machine::Map.shared();
    bare.in_use = Default::default();
    let (mut app, _) = app_on(bare);
    assert!(app.shared().batch().is_none());
    app.event(Event::FontReads(reads.clone()));
    assert_eq!(app.shared().batch(), Some(&reads.view));
}
