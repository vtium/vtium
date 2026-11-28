#![forbid(unsafe_code)]
// The tab reads what the person types into the search, and nothing it is
// handed can panic the tool.
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! vtium-config-tab-help: the Help tab of vtium-config's TUI.
//!
//! The tab lists its topics at the left: the six tabs, then Menus and
//! Save, Mouse, Commands and About. The right shows the highlighted topic.
//! A tab's topic gives every key of the tab's screen and what each does,
//! from the compiled key table, so the page cannot say what the keys do
//! not do. Up and Down move between the topics, PgUp and PgDn scroll a
//! topic longer than the pane, and Left and Right switch to About and
//! back.
//!
//! [`HelpTab`] implements vtium-config-widgets' `Tab` over
//! vtium-config-core's shared state, action and commands, and `Searchable`,
//! so the shell's `?` search filters the topics' lines. The tab also lays
//! out F1's list of every key of a tab's screen, from that tab's topic.
//! [`topics`] holds what each topic says, and [`page`] how the tab is
//! drawn.
//!
//! References:
//! - ADR-1206 D9: the shell holds each tab as a trait object, and a tab
//!   gains the search by implementing `Searchable`.
//! - ADR-1206 D3: the navigation state holds the search, and each tab
//!   keeps the indexes that match.
//! - ADR-1403 D11, "F1, help for what is highlighted": F1's list of every
//!   key of a screen is the Help tab's topic for that tab.
//! - ADR-1513 D4: each tab of the rebuilt TUI is a crate of its own.
//! - ADR-1403 D11, "The tabs' layouts", the Help bullet: the topics and
//!   the keys that move between and through them.

pub mod page;
pub mod topics;

use std::borrow::Cow;
use std::cell::Cell;

use ratatui_core::layout::{Position, Rect};
use ratatui_core::terminal::Frame;
use ratatui_widgets::clear::Clear;
use vtium_config_core::ui::{Action, Carries, Command, Shared};
use vtium_config_widgets::input::{Button, Key as InputKey};
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::Search;
use vtium_config_widgets::tab::{FindItem, Searchable, Tab};

use page::{Kept, Layout, Look, Page, View};
use topics::{ABOUT, Entry, Item, Kind, TOPICS};

/// The viewer that update reads the places of before the first draw: the
/// viewer of a 100×30 screen, under the top row and its rule and over the
/// action bar and its rule.
const FIRST_VIEWER: Rect = Rect::new(0, 2, 100, 26);

/// The Help tab's state.
#[derive(Debug)]
pub struct HelpTab {
    /// Whether the TUI draws color.
    color: bool,
    /// Each topic's entries, built once.
    entries: Vec<Vec<Entry>>,
    /// Every line the `?` search matches, built once.
    items: Vec<Item>,
    /// The highlighted topic.
    topic: usize,
    /// The topic Left or Right goes back to from About.
    before_about: usize,
    /// The page's first line in sight.
    top: usize,
    /// The copy of the shell's `?` search over the lines, while it is
    /// open or its filter is kept, as the shell last handed it.
    search: Option<Search>,
    /// The viewer the tab last drew in, from which update reads where a
    /// click lands and how far a window scrolls.
    viewer: Cell<Rect>,
}

impl HelpTab {
    /// The Help tab on its first topic, in color where `color` holds. The
    /// About topic gives what `vtium-config --help about` prints, read
    /// once here.
    pub fn new(color: bool) -> HelpTab {
        let mut about = Vec::new();
        vtium_config_core::banner::about(&mut about);
        HelpTab::with_about(color, &String::from_utf8_lossy(&about))
    }

    /// The Help tab whose About topic gives `about`, as
    /// `vtium-config --help about` prints it.
    pub fn with_about(color: bool, about: &str) -> HelpTab {
        let entries = topics::entries(about);
        let items = topics::items(&entries);
        HelpTab {
            color,
            entries,
            items,
            topic: 0,
            before_about: 0,
            top: 0,
            search: None,
            viewer: Cell::new(FIRST_VIEWER),
        }
    }

    /// The highlighted topic, by its index in [`TOPICS`].
    pub fn topic(&self) -> usize {
        self.topic
    }

    /// The page's first line in sight.
    pub fn top(&self) -> usize {
        self.top
    }

    /// The copy of the shell's `?` search, while it is open or its filter
    /// kept.
    pub fn search(&self) -> Option<&Search> {
        self.search.as_ref()
    }

    /// Each topic's entries.
    pub fn entries(&self) -> &[Vec<Entry>] {
        &self.entries
    }

    /// The lines the search keeps of each topic, or `None` while no
    /// filter applies.
    fn kept(&self) -> Option<Vec<Kept>> {
        let search = self.search.as_ref()?;
        let mut kept = vec![Kept::new(); TOPICS.len()];
        for m in search.matches() {
            if let Some(item) = self.items.get(*m)
                && let Some(k) = kept.get_mut(item.topic)
            {
                k.insert((item.entry, item.cell));
            }
        }
        Some(kept)
    }

    /// The topics the column lists: every topic, or, while the search
    /// filters, those with a line it keeps, each with its count.
    fn listed(&self) -> (Vec<usize>, Option<Vec<usize>>) {
        match self.kept() {
            None => ((0..TOPICS.len()).collect(), None),
            Some(kept) => {
                let listed: Vec<usize> = (0..TOPICS.len())
                    .filter(|t| kept.get(*t).is_some_and(|k| !k.is_empty()))
                    .collect();
                let counts = listed
                    .iter()
                    .map(|t| kept.get(*t).map_or(0, Kept::len))
                    .collect();
                (listed, Some(counts))
            }
        }
    }

    /// The highlighted topic, laid out for a page `width` cells wide.
    fn page(&self, width: u16) -> Page {
        let kept = self.kept();
        let topic_kept = kept.as_ref().and_then(|k| k.get(self.topic));
        let entries = self.entries.get(self.topic).map_or(&[][..], Vec::as_slice);
        page::lay(entries, topic_kept, usize::from(width), self.color)
    }

    /// Highlights `topic`, at the top of its page.
    fn go(&mut self, topic: usize) {
        if topic < TOPICS.len() {
            self.topic = topic;
            self.top = 0;
        }
    }

    /// Moves the highlight `by` topics among those listed, and stops at
    /// the first and the last.
    fn step_topic(&mut self, by: isize) {
        let (listed, _) = self.listed();
        let Some(last) = listed.len().checked_sub(1) else {
            return;
        };
        let at = listed.iter().position(|t| *t == self.topic).unwrap_or(0);
        if let Some(t) = listed.get(at.saturating_add_signed(by).min(last)) {
            self.go(*t);
        }
    }

    /// Goes to the first or the last topic listed.
    fn end(&mut self, last: bool) {
        let (listed, _) = self.listed();
        let t = if last { listed.last() } else { listed.first() };
        if let Some(t) = t {
            self.go(*t);
        }
    }

    /// Scrolls the page by a window, down for PgDn.
    fn scroll(&mut self, down: bool) {
        let layout = Layout::of(self.viewer.get());
        let page = self.page(layout.page.width);
        self.top = page::step(&page, self.top, layout.window(), down);
    }

    /// Left and Right: to About, or from About back to the topic before.
    fn about(&mut self) {
        if self.topic == ABOUT {
            let back = self.before_about;
            self.go(back);
        } else {
            self.before_about = self.topic;
            self.go(ABOUT);
        }
    }

    /// The topic a click at `at` lands on, by the places the view drew.
    fn clicked(&self, at: Position) -> Option<usize> {
        let layout = Layout::of(self.viewer.get());
        let (listed, _) = self.listed();
        listed
            .iter()
            .enumerate()
            .find(|(n, _)| layout.topic(*n).is_some_and(|r| r.contains(at)))
            .map(|(_, t)| *t)
    }

    /// The topic of the line numbered `item`, by its index in [`TOPICS`].
    fn topic_of(&self, item: usize) -> Option<usize> {
        self.items.get(item).map(|i| i.topic)
    }

    /// Keeps `search`, the shell's search after a key that changed it.
    /// A changed entry starts the page at its top and keeps the topic
    /// while it has a line that matches, and otherwise the topic of the
    /// line the search highlights takes the highlight. With the entry as
    /// it was, the highlight moved among the matches, and the topic of
    /// the highlighted line takes it.
    fn keep(&mut self, search: &Search) {
        let typed = self
            .search
            .as_ref()
            .is_none_or(|s| s.entry() != search.entry());
        let moved = self
            .search
            .as_ref()
            .is_some_and(|s| s.highlighted() != search.highlighted());
        self.search = Some(search.clone());
        let to = search.highlighted().and_then(|h| self.topic_of(h));
        if typed {
            self.top = 0;
            let (listed, _) = self.listed();
            if !listed.contains(&self.topic)
                && let Some(t) = to.or_else(|| listed.first().copied())
            {
                self.go(t);
            }
        } else if moved
            && let Some(t) = to
            && t != self.topic
        {
            self.go(t);
        }
    }
}

impl Tab<Shared, Action, Command> for HelpTab {
    fn name(&self) -> &'static str {
        "Help"
    }

    fn letter(&self) -> char {
        'h'
    }

    fn node(&self) -> &'static str {
        "help"
    }

    fn draw(&self, _shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        self.viewer.set(area);
        frame.render_widget(Clear, area);
        let layout = Layout::of(area);
        let page = self.page(layout.page.width);
        let (listed, counts) = self.listed();
        let topic = listed.contains(&self.topic).then_some(self.topic);
        let empty = Page::default();
        let view = View {
            listed: &listed,
            topic,
            counts: counts.as_deref(),
            page: if topic.is_some() { &page } else { &empty },
            top: self
                .top
                .min(page.lines.len().saturating_sub(layout.window())),
            color: self.color,
        };
        page::draw(frame.buffer_mut(), &layout, &view);
    }

    /// Acts on a row of the Help node. Up and Down move between the topics
    /// and Home and End to the first and the last, PgUp and PgDn scroll
    /// the topic by a window, and Left and Right switch to About and back.
    /// A click on a topic, Find's match and the tab the person came from
    /// carry the topic's index, which is a tab's own index for the six
    /// tabs, and the tab highlights it. While the shell's search filters,
    /// these keys move among the topics with a line that matches. The
    /// Help tab changes nothing in the document, so it returns no command.
    fn update(&mut self, _shared: &Shared, action: Action) -> Vec<Command> {
        match (action.row, &action.carries) {
            (_, Carries::Item(topic)) => self.go(*topic),
            ("click-row.view", Carries::Key(InputKey::Mouse(m)))
                if m.button == Button::Press(0) =>
            {
                let at = Position::new(
                    u16::try_from(m.col).unwrap_or(u16::MAX),
                    u16::try_from(m.row).unwrap_or(u16::MAX),
                );
                if let Some(t) = self.clicked(at) {
                    self.go(t);
                }
            }
            ("up.view.help", _) => self.step_topic(-1),
            ("down.view.help", _) => self.step_topic(1),
            ("home.view.help", _) => self.end(false),
            ("end.view.help", _) => self.end(true),
            ("pgup.view.help", _) => self.scroll(false),
            ("pgdn.view.help", _) => self.scroll(true),
            ("left.view.help" | "right.view.help", _) => self.about(),
            _ => {}
        }
        Vec::new()
    }

    /// Each topic, as Find lists it: its name, what it is, where it lives
    /// and how many lines it gives. A match's index is the topic's.
    fn find_items(&self, _shared: &Shared) -> Vec<FindItem> {
        TOPICS
            .iter()
            .zip(&self.entries)
            .enumerate()
            .map(|(index, (t, entries))| FindItem {
                name: t.name.to_owned(),
                description: entries.iter().find_map(|e| match e {
                    Entry::Text(s) => Some(s.clone()),
                    _ => None,
                }),
                place: "Help".to_owned(),
                fact: fact(t.kind, entries),
                index,
            })
            .collect()
    }

    fn searchable(&self) -> Option<&dyn Searchable> {
        Some(self)
    }

    /// Keeps the shell's search, and lists only the topics with a line
    /// that matches; `None` lists every topic again.
    fn filtered(&mut self, _shared: &Shared, search: Option<&Search>) {
        match search {
            Some(s) => self.keep(s),
            None => self.search = None,
        }
    }

    /// The topic of the tab that `node` belongs to, laid out `width`
    /// columns wide without color, as the tab's page shows it: each line's
    /// text with the role of its first run. A node over no tab, such as a
    /// menu or a sheet, has no topic.
    fn keys_help(&self, node: &'static str, width: u16) -> Option<Vec<(String, Role)>> {
        let entries = topics::for_node(node).and_then(|t| self.entries.get(t))?;
        let page = page::lay(entries, None, usize::from(width), false);
        Some(
            page.lines
                .iter()
                .map(|line| {
                    let text: String = line.iter().map(|r| r.text.as_str()).collect();
                    let role = match line.first().map(|r| r.look) {
                        Some(Look::Role(role)) => role,
                        _ => Role::Row,
                    };
                    (text, role)
                })
                .collect(),
        )
    }
}

/// The one fact Find shows beside a topic: how many keys, clicks or
/// commands it gives.
fn fact(kind: Kind, entries: &[Entry]) -> String {
    let rows = topics::rows_shown(entries).len();
    let commands = vtium_config_core::help::topics().count();
    match kind {
        Kind::Tab(_) | Kind::Menus => format!("{rows} keys"),
        Kind::Mouse => format!("{rows} clicks"),
        Kind::Commands => format!("{commands} commands"),
        Kind::About => "the version".to_owned(),
    }
}

impl Searchable for HelpTab {
    fn search_node(&self) -> &'static str {
        "help"
    }

    fn count(&self) -> usize {
        self.items.len()
    }

    /// The highlighted topic's line that the search highlights, or else
    /// its first line that matches, or its first line where no search is
    /// kept.
    fn highlighted(&self) -> Option<usize> {
        let in_topic = |i: &usize| self.topic_of(*i) == Some(self.topic);
        match self.search.as_ref() {
            None => self.items.iter().position(|i| i.topic == self.topic),
            Some(s) => s
                .highlighted()
                .filter(in_topic)
                .or_else(|| s.matches().iter().copied().find(in_topic)),
        }
    }

    /// The `topic` field of a line: the topic's name, then the line.
    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>> {
        (field == 0)
            .then(|| self.items.get(item))
            .flatten()
            .map(|i| Cow::Borrowed(i.text.as_str()))
    }
}
