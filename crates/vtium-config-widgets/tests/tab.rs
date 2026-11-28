//! The seams' traits with only their required methods: what each default
//! answers, and that the shell can hold a tab as a trait object.
//!
//! References:
//! - ADR-1206 D9: the `Tab` trait, `Searchable` and the choices trait,
//!   each with defaults, so a tab writes only what differs.

use std::borrow::Cow;

use ratatui_core::backend::TestBackend;
use ratatui_core::layout::Rect;
use ratatui_core::terminal::{Frame, Terminal};
use vtium_config_widgets::contract;
use vtium_config_widgets::keys::Key;
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::Search;
use vtium_config_widgets::tab::{Choices, Haystack, Previews, Searchable, Tab};

/// The shared state a test tab reads.
struct Shared {
    fonts: Vec<(&'static str, &'static str)>,
}

/// A test tab previews a font by its family.
impl Previews for Shared {
    type Preview<'a> = &'a str;
}

/// An action, and the command a test tab returns for it.
#[derive(Debug, PartialEq, Eq)]
struct Action(&'static str);
#[derive(Debug, PartialEq, Eq)]
struct Command(&'static str);

/// A tab that writes only the required methods.
struct Bare;

impl Tab<Shared, Action, Command> for Bare {
    fn name(&self) -> &'static str {
        "Fonts"
    }
    fn letter(&self) -> char {
        'n'
    }
    fn node(&self) -> &'static str {
        "fonts"
    }
    fn draw(&self, shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        let line = shared.fonts.first().map_or("", |f| f.0);
        frame
            .buffer_mut()
            .set_string(area.x, area.y, line, ratatui_core::style::Style::new());
    }
    fn update(&mut self, _shared: &Shared, action: Action) -> Vec<Command> {
        vec![Command(action.0)]
    }
}

fn shared() -> Shared {
    Shared {
        fonts: vec![
            (
                "Cullet Mono",
                "/usr/share/fonts/cullet/CulletMono-Regular.otf",
            ),
            ("Terminus", "/usr/share/fonts/misc/ter-u16n.pcf.gz"),
            (
                "DejaVu Sans Mono",
                "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            ),
        ],
    }
}

#[test]
fn a_tab_with_only_the_required_methods_answers_every_default_empty() {
    let tab = Bare;
    let s = shared();
    assert!(tab.menu_items(&s).is_empty());
    assert_eq!(tab.full_text(&s), None);
    assert!(tab.find_items(&s).is_empty());
    assert!(tab.searchable().is_none());
    assert!(tab.search().is_none());
    assert_eq!(tab.own_font(&s), None);
    assert!(tab.bar_labels(&s).is_empty());
    assert_eq!(tab.preview(&s), None);
    assert_eq!(tab.keys_help("fonts", 80), None);
}

#[test]
fn the_search_matches_and_a_name_to_show_reach_a_tab_that_takes_them() {
    /// A tab that keeps what the shell hands it.
    #[derive(Default)]
    struct Keeping {
        matches: Option<Vec<usize>>,
        shown: Option<String>,
    }
    impl Tab<Shared, Action, Command> for Keeping {
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
        fn update(&mut self, _shared: &Shared, _action: Action) -> Vec<Command> {
            Vec::new()
        }
        fn filtered(&mut self, _shared: &Shared, search: Option<&Search>) {
            self.matches = search.map(|s| s.matches().to_vec());
        }
        fn show(&mut self, _shared: &Shared, name: &str) {
            self.shown = Some(name.to_owned());
        }
        fn own_font(&self, shared: &Shared) -> Option<std::path::PathBuf> {
            // the highlighted font, while the search's matches hold it
            let name = self.shown.as_ref()?;
            let at = shared.fonts.iter().position(|f| f.0 == name)?;
            self.matches.as_ref().filter(|m| m.contains(&at))?;
            shared.fonts.get(at).map(|f| std::path::PathBuf::from(f.1))
        }
    }
    let s = shared();
    // the shell holds the tab as a trait object and reaches it through the
    // trait alone
    let mut tab: Box<dyn Tab<Shared, Action, Command>> = Box::new(Keeping::default());
    let list = Searching {
        fonts: s.fonts.clone(),
    };
    let mut search = Search::open(&list, None);
    for c in "mono".chars() {
        search.key(&Key::char(c));
    }
    assert_eq!(search.matches(), [0, 2], "Cullet Mono and DejaVu Sans Mono");
    tab.filtered(&s, Some(&search));
    tab.show(&s, "Terminus");
    assert_eq!(tab.own_font(&s), None, "Terminus is not among the matches");
    let mut search = Search::open(&list, None);
    for c in "ter".chars() {
        search.key(&Key::char(c));
    }
    assert_eq!(search.matches(), [1]);
    tab.filtered(&s, Some(&search));
    assert_eq!(
        tab.own_font(&s),
        Some(std::path::PathBuf::from(
            "/usr/share/fonts/misc/ter-u16n.pcf.gz"
        ))
    );
    // a bare tab takes both and keeps nothing
    let mut bare: Box<dyn Tab<Shared, Action, Command>> = Box::new(Bare);
    bare.filtered(&s, Some(&search));
    bare.filtered(&s, None);
    bare.show(&s, "Terminus");
    assert_eq!(bare.own_font(&s), None);
}

#[test]
fn the_key_line_and_the_bar_come_from_the_contract_for_the_tab_s_node() {
    let tab = Bare;
    let ids = |rows: Vec<&contract::Row>| rows.iter().map(|r| r.id).collect::<Vec<_>>();
    assert_eq!(ids(tab.key_line()), ids(contract::labelled("fonts")));
    let s = shared();
    assert_eq!(ids(tab.bar(&s)), ids(contract::buttons("fonts")));
    assert_eq!(
        tab.bar(&s).first().map(|r| r.id),
        Some("space.view.resources")
    );
}

#[test]
fn the_shell_holds_a_tab_as_a_trait_object_and_draws_and_updates_through_it() -> Result<(), String>
{
    let mut tabs: Vec<Box<dyn Tab<Shared, Action, Command>>> = vec![Box::new(Bare)];
    let s = shared();
    let mut terminal = Terminal::new(TestBackend::new(80, 25)).map_err(|e| e.to_string())?;
    let tab = tabs.first_mut().ok_or("no tab")?;
    terminal
        .draw(|f| tab.draw(&s, f, Rect::new(2, 1, 40, 10)))
        .map_err(|e| e.to_string())?;
    let row: String = (2..13)
        .filter_map(|x| {
            terminal
                .backend()
                .buffer()
                .cell((x, 1))
                .map(|c| c.symbol().to_owned())
        })
        .collect();
    assert_eq!(row, "Cullet Mono");
    assert_eq!(
        tab.update(&s, Action("space.view.resources")),
        [Command("space.view.resources")]
    );
    assert_eq!(
        (tab.name(), tab.letter(), tab.node()),
        ("Fonts", 'n', "fonts")
    );
    Ok(())
}

#[test]
fn a_tab_answers_its_bar_s_counts_its_preview_and_f1_s_keys_through_the_trait() {
    /// A tab that counts a button, previews its first font, and gives the
    /// keys of the fonts node.
    struct Answering;
    impl Tab<Shared, Action, Command> for Answering {
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
        fn update(&mut self, _shared: &Shared, _action: Action) -> Vec<Command> {
            Vec::new()
        }
        fn bar_labels(&self, shared: &Shared) -> Vec<(&'static str, String)> {
            vec![(
                "shift-a.view.fonts",
                format!("Activate missing ({})", shared.fonts.len()),
            )]
        }
        fn preview<'a>(&self, shared: &'a Shared) -> Option<&'a str> {
            shared.fonts.first().map(|f| f.0)
        }
        fn keys_help(&self, node: &'static str, width: u16) -> Option<Vec<(String, Role)>> {
            (node == "fonts").then(|| vec![(format!("{width} columns"), Role::Row)])
        }
    }
    let s = shared();
    let tab: Box<dyn Tab<Shared, Action, Command>> = Box::new(Answering);
    assert_eq!(
        tab.bar_labels(&s),
        [("shift-a.view.fonts", "Activate missing (3)".to_owned())]
    );
    assert_eq!(tab.preview(&s), Some("Cullet Mono"));
    assert_eq!(
        tab.keys_help("fonts", 76),
        Some(vec![("76 columns".to_owned(), Role::Row)])
    );
    assert_eq!(tab.keys_help("looks", 76), None);
}

#[test]
fn the_search_shares_its_haystack_and_puts_its_highlight_on_a_match() {
    let list = Searching {
        fonts: shared().fonts,
    };
    let mut search = Search::open(&list, Some(1));
    assert_eq!(search.highlighted(), Some(1));
    search.key(&Key::char('m'));
    search.key(&Key::char('o'));
    assert_eq!(search.matches(), [0, 2]);
    search.highlight(2);
    assert_eq!(search.highlighted(), Some(2));
    search.highlight(1);
    assert_eq!(search.highlighted(), Some(2), "Terminus does not match");
    let copy = search.clone();
    assert_eq!(copy.matches(), search.matches());
    assert_eq!(copy.count(), 3);
}

/// A tab that offers the `?` search over its fonts.
struct Searching {
    fonts: Vec<(&'static str, &'static str)>,
}

impl Tab<Shared, Action, Command> for Searching {
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
    fn update(&mut self, _shared: &Shared, _action: Action) -> Vec<Command> {
        Vec::new()
    }
    fn searchable(&self) -> Option<&dyn Searchable> {
        Some(self)
    }
}

impl Searchable for Searching {
    fn search_node(&self) -> &'static str {
        "fonts"
    }
    fn count(&self) -> usize {
        self.fonts.len()
    }
    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>> {
        let (family, file) = self.fonts.get(item)?;
        match field {
            0 => Some(Cow::Borrowed(family)),
            1 => Some(Cow::Borrowed(file)),
            _ => None,
        }
    }
}

#[test]
fn a_searchable_tab_answers_its_search_with_the_node_s_fields() -> Result<(), String> {
    let tab = Searching {
        fonts: shared().fonts,
    };
    let search = tab.searchable().ok_or("the tab offers no search")?;
    assert_eq!(search.fields(), ["family", "file"]);
    assert_eq!(search.help(), "family, file");
    let Haystack { items } = search.haystack();
    assert_eq!(items.len(), 3);
    assert_eq!(
        items.first(),
        Some(&vec![
            "cullet mono".to_owned(),
            "/usr/share/fonts/cullet/culletmono-regular.otf".to_owned()
        ])
    );
    Ok(())
}

#[test]
fn a_search_on_a_node_with_no_search_column_has_no_fields() {
    struct Flow;
    impl Searchable for Flow {
        fn search_node(&self) -> &'static str {
            "flow"
        }
        fn count(&self) -> usize {
            2
        }
        fn text(&self, _item: usize, _field: usize) -> Option<Cow<'_, str>> {
            Some(Cow::Borrowed("x"))
        }
    }
    assert!(Flow.fields().is_empty());
    assert_eq!(Flow.help(), "");
    assert_eq!(Flow.haystack().items, [Vec::<String>::new(), Vec::new()]);
}

/// A line's choices: the sizes a look's card offers.
struct Sizes(Vec<&'static str>);

impl Choices<Command> for Sizes {
    fn count(&self) -> usize {
        self.0.len()
    }
    fn text(&self, choice: usize) -> Cow<'_, str> {
        Cow::Borrowed(self.0.get(choice).copied().unwrap_or_default())
    }
    fn current(&self) -> Option<usize> {
        Some(2)
    }
    fn keep(&self, choice: usize) -> Command {
        Command(self.0.get(choice).copied().unwrap_or_default())
    }
}

#[test]
fn the_list_box_shows_every_choice_that_fits_inside_its_borders() {
    let sizes = Sizes(vec!["automatic", "8 pt", "9 pt", "10 pt", "11 pt", "12 pt"]);
    // room for all six and the two borders
    assert_eq!(sizes.shown(20), 6);
    assert_eq!(sizes.height(20), 8);
    assert!(!sizes.more_than_shown(20));
    assert!(!sizes.more_than_shown(8));
    // one row short: the box scrolls, and the line offers the search
    assert_eq!(sizes.shown(7), 5);
    assert_eq!(sizes.height(7), 7);
    assert!(sizes.more_than_shown(7));
    // no room at all
    assert_eq!(sizes.shown(1), 0);
    assert_eq!(sizes.height(1), 1);
    assert_eq!(sizes.keep(3), Command("10 pt"));
    assert_eq!(sizes.text(9), "");
}
