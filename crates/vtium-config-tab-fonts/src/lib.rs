#![forbid(unsafe_code)]
// The tab reads keys, clicks, font files' names and what the font helper
// says of them, and nothing it is given can panic the TUI.
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! vtium-config-tab-fonts: the Fonts tab of vtium-config's TUI.
//!
//! The tab lists the fonts found on this system in groups by the folder
//! each was found in, with the fonts built into vtium and the fonts a
//! font set of the configuration names that no file is installed for. A
//! filter line above the list keeps every font, or one kind, group or
//! script, and `?` searches the list by family and file name. Each font
//! carries its kind and one mark, `● active`, `○ not active`, `○ turned
//! off on purpose`, `○ not active · fails trust`, `not usable yet` or `✗
//! missing`, the same words every screen gives it. The highlighted font's
//! facts sit beside the list, so moving through the fonts shows each
//! one's facts without Enter, and the picture of the console sits under
//! the list.
//!
//! Space activates or deactivates one font as one change, and asks first
//! when a look uses a font it would deactivate. While the configuration
//! uses fonts that are installed and trusted but not active, the list's
//! first item and the action bar's `[A] Activate missing (3)` activate
//! them all as one change, leaving the fonts turned off on purpose off.
//! Enter opens a font's full detail, where Enter on a font of your own
//! starts its install as a system font. `t` returns the font's path for
//! the shell's live trial, or says why the console would refuse it.
//!
//! The tab draws only into the viewer the frame gives it and reads the
//! shared state only. [`states`] works out every font's state once for
//! each change of the document or of the fonts found, so a key that
//! moves, filters or searches reads no folder. [`list`] holds the list,
//! [`facts`] the facts beside it, [`detail`] a font's detail, [`own`] a
//! font of your own, [`trial`] the path a live trial sends, and
//! [`layout`] where each part goes.
//!
//! References:
//! - ADR-1206 D9: each tab is a crate that implements the `Tab` trait
//!   over the shared state, the action and the commands.
//! - ADR-1206 D2, D4: a tab's update returns commands, and its view
//!   draws from the shared state without changing it.
//! - ADR-1403 D11, "The tabs' layouts", the Fonts bullet: the facts
//!   beside the list, and the picture under it.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated":
//!   the marks, Activate missing and its count.
//! - ADR-1403 D11, "Search, on ?": Fonts matches a font's family and its
//!   file's name.

pub mod detail;
pub mod facts;
pub mod layout;
pub mod list;
pub mod own;
pub mod states;
pub mod trial;
pub mod words;

mod act;

use std::borrow::Cow;
use std::cell::{Cell, Ref, RefCell};
use std::path::PathBuf;

use ratatui_core::layout::Rect;
use ratatui_core::terminal::Frame;
use ratatui_core::widgets::Widget;
use ratatui_widgets::block::Block;
use ratatui_widgets::borders::Borders;
use vtium_config_core::found::fonts::{Filter, FontFile};
use vtium_config_core::ui::{Action, Command, Live, Shared};
use vtium_config_widgets::contract::{self, Row};
use vtium_config_widgets::frame::Bar;
use vtium_config_widgets::list_box::{self, ListBox};
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::Search;
use vtium_config_widgets::tab::{Choices, FindItem, Searchable, Tab};

use layout::Panes;
use list::{ListPane, Sel};
use states::{Item, Known};

/// The node of the list.
const NODE: &str = "fonts";
/// The node of the list box of the filters.
const NODE_FILTERS: &str = "fonts.filter";
/// The node of a font's detail.
const NODE_FONT: &str = "fonts.font";
/// The node of the search row while the person types in it.
const NODE_SEARCH: &str = "any.search";
/// The row of Activate missing.
const ROW_ACTIVATE: &str = "shift-a.view.fonts";
/// The row of Space.
const ROW_SPACE: &str = "space.view.resources";
/// The row of Enter in a font's detail, which installs a font of your
/// own as a system font.
const ROW_INSTALL: &str = "enter.layer.font";

/// What is open over the list.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Open {
    /// Nothing: the person moves through the list.
    Nothing,
    /// The filters, in the list box under the filter line.
    Filters(Box<ListBox>),
    /// A font's detail, scrolled to `top`; `own` for a font of your own,
    /// which Enter installs as a system font.
    Detail { item: Item, top: usize, own: bool },
}

/// The Fonts tab: the highlighted row, the list's window, the filter, the
/// `?` search, what is open over the list, whether the picture shows the
/// console or the sample characters, and what it knows of every font.
#[derive(Debug)]
pub struct Fonts {
    color: bool,
    sel: Sel,
    /// The first row of the list's body the window shows.
    top: usize,
    filter: Filter,
    open: Open,
    /// The search over the fonts, while its row is open or its filter
    /// kept.
    search: Option<Search>,
    /// Whether the picture shows the console drawn in the font, rather
    /// than its sample characters, which `p` switches.
    picture: bool,
    /// What the tab knows of every font, for the document and the fonts
    /// found as they were when it was built.
    known: RefCell<Known>,
    /// The viewer the tab was last drawn in, which a click and a page's
    /// step are read against.
    seen: Cell<Rect>,
    /// How many fonts Activate missing would activate, as last drawn,
    /// which the action bar's button counts.
    missing: Cell<usize>,
}

/// What the picture under the list shows, for the shell's preview: the
/// font by its file's path or its name, whether its sample characters
/// show rather than the console drawn in it, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub font: String,
    pub detail: bool,
    pub place: Rect,
}

/// The filters, as the list box's choices.
struct FilterChoices {
    all: Vec<Filter>,
    current: Option<usize>,
}

impl FilterChoices {
    fn new(current: Filter) -> FilterChoices {
        let all = Filter::choices();
        let current = all.iter().position(|f| *f == current);
        FilterChoices { all, current }
    }
}

impl Choices<Filter> for FilterChoices {
    fn count(&self) -> usize {
        self.all.len()
    }

    fn text(&self, choice: usize) -> Cow<'_, str> {
        Cow::Owned(
            self.all
                .get(choice)
                .map_or_else(String::new, |f| words::filter_name(*f)),
        )
    }

    fn current(&self) -> Option<usize> {
        self.current
    }

    fn keep(&self, choice: usize) -> Filter {
        self.all.get(choice).copied().unwrap_or_default()
    }
}

impl Fonts {
    /// The tab on the filter line, showing every font. `color` is whether
    /// the TUI draws color ([`vtium_config_widgets::color::enabled`]).
    pub fn new(color: bool) -> Fonts {
        Fonts {
            color,
            sel: Sel::Filter,
            top: 0,
            filter: Filter::All,
            open: Open::Nothing,
            search: None,
            picture: false,
            known: RefCell::new(Known::default()),
            seen: Cell::new(Rect::default()),
            missing: Cell::new(0),
        }
    }

    /// What the tab knows of every font, built again first where the
    /// document or the fonts found changed since.
    pub fn known(&self, shared: &Shared) -> Ref<'_, Known> {
        if !self.known.borrow().holds(shared) {
            let fresh = Known::of(shared);
            self.missing.set(fresh.counts.not_active);
            *self.known.borrow_mut() = fresh;
        }
        self.known.borrow()
    }

    /// What the highlight stands on.
    pub fn sel(&self) -> Sel {
        self.sel
    }

    /// The filter the list shows.
    pub fn filter(&self) -> Filter {
        self.filter
    }

    /// Whether the picture shows the console rather than the sample
    /// characters.
    pub fn picture(&self) -> bool {
        self.picture
    }

    /// The font whose detail is open.
    pub fn detail(&self) -> Option<Item> {
        match self.open {
            Open::Detail { item, .. } => Some(item),
            _ => None,
        }
    }

    /// The search whose row shows above the action bar while it is open
    /// or its filter kept. The shell draws its row with
    /// `vtium_config_widgets::search::row::draw`.
    pub fn search(&self) -> Option<&Search> {
        self.search.as_ref()
    }

    /// The rows the highlight can stand on, in the list's order, while
    /// nothing is open over the list.
    fn order(&self, known: &Known, shared: &Shared) -> Vec<Sel> {
        let shown = list::shown(known, shared, self.filter, self.search.as_ref());
        list::header(known)
            .iter()
            .chain(list::body(known, &shown, self.search.as_ref()).iter())
            .filter_map(|l| l.sel(known))
            .collect()
    }

    /// The highlight, kept where it stands while the list still shows
    /// it, else on the filter line.
    fn settled_sel(&self, known: &Known, shared: &Shared) -> Sel {
        if self.order(known, shared).contains(&self.sel) {
            self.sel
        } else {
            Sel::Filter
        }
    }

    /// The list as it shows on `shared`.
    fn pane<'a>(&'a self, known: &'a Known, shared: &'a Shared) -> ListPane<'a> {
        ListPane {
            known,
            shared,
            filter: self.filter,
            search: self.search.as_ref(),
            sel: self.settled_sel(known, shared),
            top: self.top,
            color: self.color,
        }
    }

    /// The file of the highlighted font, or of the font whose detail is
    /// open.
    fn file<'s>(&self, shared: &'s Shared) -> Option<&'s FontFile> {
        let item = match (&self.open, self.sel) {
            (Open::Detail { item, .. }, _) => *item,
            (_, Sel::Item(item)) => item,
            _ => return None,
        };
        match item {
            Item::File(i) => shared.found.fonts.files().get(i),
            _ => None,
        }
    }

    /// The font the install as a system font acts on: the font of your
    /// own whose detail is open or that the list highlights, or why there
    /// is none. Actions › Install as a system font… asks this.
    pub fn system_font(&self, shared: &Shared) -> Result<PathBuf, String> {
        own::target(self.file(shared))
    }

    /// What the picture under the list shows, and where, while the list
    /// highlights a font: the font by its file's path or its name. `None`
    /// on the filter line, the configuration's row, a font no file is
    /// installed for, and while a level is open over the list.
    pub fn preview(&self, shared: &Shared) -> Option<Preview> {
        if self.open != Open::Nothing {
            return None;
        }
        let font = match self.sel {
            Sel::Item(Item::File(i)) => shared
                .found
                .fonts
                .files()
                .get(i)?
                .path()
                .to_str()?
                .to_owned(),
            Sel::Item(Item::BuiltIn(name)) => name.to_owned(),
            _ => return None,
        };
        let place = Panes::of(self.seen.get()).picture;
        (place.height > 2).then(|| Preview {
            font,
            detail: !self.picture,
            place: inner(place),
        })
    }

    /// The action bar's buttons on `shared`: the node's rows, with
    /// Activate missing counting its fonts while there are any, and
    /// Space's label saying what it does to the highlighted font.
    pub fn action_bar(&self, shared: &Shared) -> Bar {
        let known = self.known(shared);
        let n = known.counts.not_active;
        let rows = self.rows_for(n);
        let mut bar = Bar::from_rows(rows);
        let deactivates = self.open == Open::Nothing
            && matches!(self.sel, Sel::Item(item) if known.position(item).is_some_and(|at| {
                matches!(item, Item::File(_)) && known.active(at)
            }));
        for b in &mut bar.buttons {
            match b.action {
                Some(ROW_ACTIVATE) => {
                    b.label = vtium_config_core::fontuse::activate::words::button(n);
                }
                Some(ROW_SPACE) if deactivates => b.label = "Deactivate".to_owned(),
                _ => {}
            }
        }
        bar
    }

    /// The node's buttons, without Activate missing while no font waits,
    /// and without the install as a system font in the detail of a font
    /// that is not your own.
    fn rows_for(&self, missing: usize) -> Vec<&'static Row> {
        let own = matches!(self.open, Open::Detail { own: true, .. });
        contract::buttons(self.node())
            .into_iter()
            .filter(|r| r.id != ROW_ACTIVATE || missing > 0)
            .filter(|r| r.id != ROW_INSTALL || own)
            .collect()
    }

    /// The list box of the filters' place, under the filter line.
    fn filters_rect(&self, choices: &FilterChoices) -> Rect {
        let panes = Panes::of(self.seen.get());
        let area = Rect::new(
            panes.list.x,
            panes.list.y,
            panes.list.width,
            self.seen.get().height,
        );
        match &self.open {
            Open::Filters(lb) => {
                lb.place(choices, area, panes.list.y, panes.list.x.saturating_add(1))
            }
            _ => Rect::default(),
        }
    }

    /// Where each part of the tab is drawn on `shared`, in the viewer last
    /// drawn: each row of the list the highlight can stand on, and the
    /// list box's parts while it is open.
    pub fn places(&self, shared: &Shared) -> Places {
        let panes = Panes::of(self.seen.get());
        let known = self.known(shared);
        let rows = match self.open {
            Open::Detail { .. } => Vec::new(),
            _ => self.pane(&known, shared).places(panes.list),
        };
        let filters = match &self.open {
            Open::Filters(lb) => {
                let choices = FilterChoices::new(self.filter);
                Some(lb.places(&choices, self.filters_rect(&choices)))
            }
            _ => None,
        };
        Places { rows, filters }
    }
}

/// Where each part of the tab is drawn, for clicks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// Each row the highlight can stand on, and what it stands on there.
    pub rows: Vec<(Rect, Sel)>,
    /// The list box of the filters' parts, while it is open.
    pub filters: Option<list_box::Places>,
}

/// The cells inside a box's border.
fn inner(rect: Rect) -> Rect {
    Rect::new(
        rect.x.saturating_add(1),
        rect.y.saturating_add(1),
        rect.width.saturating_sub(2),
        rect.height.saturating_sub(2),
    )
}

/// Whether this terminal has said it shows no pictures.
fn no_images(shared: &Shared) -> bool {
    shared.console.live != Live::Asking && !shared.console.graphics
}

impl Tab<Shared, Action, Command> for Fonts {
    fn name(&self) -> &'static str {
        words::FONTS
    }

    fn letter(&self) -> char {
        'n'
    }

    /// The list, the list box of the filters, a font's detail, or the
    /// search row while the person types in it. A kept filter leaves the
    /// list's node, whose keys act on what matches.
    fn node(&self) -> &'static str {
        match &self.open {
            Open::Detail { .. } => NODE_FONT,
            Open::Filters(_) => NODE_FILTERS,
            Open::Nothing if self.search.as_ref().is_some_and(Search::typing) => NODE_SEARCH,
            Open::Nothing => NODE,
        }
    }

    fn draw(&self, shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        self.seen.set(area);
        let known = self.known(shared);
        self.missing.set(known.counts.not_active);
        let color = self.color;
        let buf = frame.buffer_mut();
        if let Open::Detail { item, top, .. } = &self.open {
            let (title, entries) = detail::entries(&known, shared, *item);
            let rows = detail::rows(&title, &entries, usize::from(area.width));
            let top = (*top).min(detail::last_top(rows.len(), usize::from(area.height)));
            detail::draw(&rows, top, buf, area, color);
            return;
        }
        let panes = Panes::of(area);
        let pane = self.pane(&known, shared);
        pane.draw(buf, panes.list);
        Block::new()
            .borders(Borders::LEFT)
            .border_style(Role::Row.style(color).cell())
            .render(panes.rule, buf);
        let lines = facts::lines(&known, shared, pane.sel, usize::from(panes.facts.width));
        facts::draw(&lines, buf, panes.facts, color);
        if panes.picture.height > 2 {
            let title = if self.picture {
                words::PICTURE
            } else {
                words::SAMPLE
            };
            Block::new()
                .borders(Borders::ALL)
                .border_style(Role::Row.style(color).cell())
                .title(format!(" {title} "))
                .render(panes.picture, buf);
        }
        if let Open::Filters(lb) = &self.open {
            let choices = FilterChoices::new(self.filter);
            let rect = self.filters_rect(&choices);
            lb.draw(&choices, buf, rect, color);
        }
    }

    fn update(&mut self, shared: &Shared, action: Action) -> Vec<Command> {
        act::update(self, shared, action)
    }

    /// The action bar's buttons: the node's, without Activate missing
    /// while no font waits to be activated, as last drawn. The shell
    /// gets the button's count through [`Fonts::action_bar`].
    fn bar(&self) -> Vec<&'static Row> {
        self.rows_for(self.missing.get())
    }

    /// F1's text: the highlighted row's facts uncut, where the pane cut
    /// one of them. Nothing in a detail, which cuts nothing, or in the
    /// list box.
    fn full_text(&self, shared: &Shared) -> Option<String> {
        if self.open != Open::Nothing {
            return None;
        }
        let known = self.known(shared);
        let sel = self.settled_sel(&known, shared);
        let width = usize::from(Panes::of(self.seen.get()).facts.width);
        let lines = facts::lines(&known, shared, sel, width);
        facts::cut(&lines, width)
            .then(|| lines.iter().map(facts::text).collect::<Vec<_>>().join("\n"))
    }

    /// Every font, for Find: its file's name and its family, its group,
    /// and its mark.
    fn find_items(&self, shared: &Shared) -> Vec<FindItem> {
        let known = self.known(shared);
        (0..known.items.len())
            .filter_map(|at| {
                let [family, file] = known.fields.get(at)?.clone();
                let fact = match known.state(at) {
                    Some(s) => s.words(),
                    None => words::ALWAYS.0.to_owned(),
                };
                // a font no file is installed for is known by its family
                let (name, description) = if file.is_empty() {
                    (family, None)
                } else {
                    (file, (!family.is_empty()).then_some(family))
                };
                Some(FindItem {
                    name,
                    description,
                    place: words::find_place(*known.groups.get(at)?),
                    fact,
                    index: at,
                })
            })
            .collect()
    }

    fn searchable(&self) -> Option<&dyn Searchable> {
        Some(self)
    }
}

/// The `?` search over every font the tab lists, by its family and its
/// file's name.
impl Searchable for Fonts {
    fn search_node(&self) -> &'static str {
        NODE
    }

    fn count(&self) -> usize {
        self.known.borrow().items.len()
    }

    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>> {
        let known = self.known.borrow();
        let fields = known.fields.get(item)?;
        fields.get(field).map(|t| Cow::Owned(t.clone()))
    }
}
