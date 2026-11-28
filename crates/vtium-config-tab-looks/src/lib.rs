#![forbid(unsafe_code)]
// The tab reads keys, clicks, the configuration's text and the resources
// found on the machine, and nothing it is given can panic the TUI.
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
//! vtium-config-tab-looks: the Looks tab of vtium-config's TUI.
//!
//! The tab's list holds the looks of the open configuration, then the
//! font sets, the color schemes and the pointer themes. Beside the list
//! sits the highlighted look's card, a summary of its font set, size,
//! color scheme, vtlook, pointer theme and fonts, above the preview of
//! how the console draws it. On a font set, a color scheme or a pointer
//! theme, the same place shows the facts its detail lists. Enter opens a
//! look's card to edit it, and each line of the card opens its choices in
//! the list box under the line. A look whose fonts are not all active
//! says so in its row and on its card, which offers `[A] Activate`.
//!
//! The tab draws only into the viewer the frame gives it and reads the
//! shared state only. Each change is one command, which the shell's one
//! change function makes and records as one step that Undo takes back.
//! [`list`] holds the list, [`card`] a look's lines and their choices,
//! [`sizes`] the sizes a look's fonts supply, [`resources`] the font
//! sets', color schemes' and pointer themes' facts, [`missing`] the fonts
//! a look uses and Activate, [`view`] the screens, and [`draw`] where
//! each part goes. [`forms`] holds `n`'s new look and new font set's
//! form, `r`'s removal of a font set, and the questions before a look is
//! deleted or something a look uses is deactivated.
//!
//! References:
//! - ADR-1206 D9: each tab is a crate that implements the `Tab` trait over the shared state, the action and the commands.
//! - ADR-1206 D2, D4: a tab's update returns commands, and its view draws from the shared state without changing it.
//! - ADR-1403 D11, "The tabs' layouts", the Looks bullet: the card beside the list, above the preview.
//! - ADR-1403 D11, "The list box", and "Fonts the configuration uses but has not activated".

pub mod card;
pub mod draw;
pub mod forms;
pub mod list;
pub mod missing;
pub mod resources;
pub mod sizes;
pub mod view;
pub mod words;

mod act;

use std::borrow::Cow;
use std::cell::{Cell, Ref, RefCell};

use ratatui_core::layout::Rect;
use ratatui_core::terminal::Frame;
use vtium_config_core::fontuse::Used;
use vtium_config_core::preview::{Place, Source};
use vtium_config_core::ui::{Action, Command, Shared};
use vtium_config_widgets::list_box::{self, ListBox};
use vtium_config_widgets::search::Search;
use vtium_config_widgets::tab::{FindItem, Searchable, Tab};

use card::Line;
use list::Item;
use missing::Walked;

/// The list with a look or a font set highlighted.
const NODE_LIST: &str = "looks";
/// The list with a color scheme or a pointer theme highlighted.
const NODE_RESOURCES: &str = "looks-resources";
/// A look's card.
const NODE_CARD: &str = "looks.look";
/// The list box under a line of the card.
const NODE_CARD_LIST: &str = "looks.look.list";
/// The look's name being typed.
const NODE_TYPING: &str = "looks.look.typing";
/// A font set's detail.
const NODE_FONT_SET: &str = "looks.font-set";
/// A color scheme's detail.
const NODE_PALETTE: &str = "looks.palette";
/// A pointer theme's detail.
const NODE_POINTER: &str = "looks.pointer";
/// The search row while the person types in it.
const NODE_SEARCH: &str = "any.search";

/// What a look's card has open over its lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardOpen {
    /// Nothing: Up and Down move between the lines and the fonts.
    Nothing,
    /// The look's name, being typed.
    Typing(String),
    /// A line's choices, in the list box under the line.
    List { list: Box<ListBox>, line: Line },
    /// What the look still misses, scrolled down `top` lines.
    Missing { top: usize },
}

/// A look's card, open over the viewer: the look, the highlighted row,
/// and what is open over the rows. Rows 0 to 5 are the card's lines, and
/// the rows after them are the look's fonts by role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub name: String,
    pub row: usize,
    pub open: CardOpen,
}

impl Card {
    /// The highlighted line, while the highlight is on one of the lines.
    pub fn line(&self) -> Option<Line> {
        Line::ALL.get(self.row).copied()
    }
}

/// What is open over the list.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Open {
    Nothing,
    Card(Card),
    /// A font set's, a color scheme's or a pointer theme's detail,
    /// scrolled down `top` rows.
    Detail {
        item: Item,
        top: usize,
    },
}

/// The Looks tab: the highlighted item, what is open over the list, the
/// `?` filter, and whether the preview shows a picture of the console or
/// sample characters.
#[derive(Debug)]
pub struct Looks {
    color: bool,
    /// The locale whose lines the detailed specimen shows, in a vtium
    /// console with the picture on.
    locale: String,
    /// The highlighted item, by its place in [`list::items`].
    highlight: usize,
    /// The first row of the list shown.
    offset: usize,
    picture: bool,
    open: Open,
    filter: Option<Search>,
    /// The viewer the tab was last drawn in, which a click and a page's
    /// step are read against.
    seen: Cell<Rect>,
    /// Whether the highlight was on a color scheme or a pointer theme when
    /// the tab last saw the shared state, which decides the list's node.
    on_resource: Cell<bool>,
    /// The items' names when the tab last saw the shared state, which the
    /// shell's search reads through [`Searchable`].
    names: RefCell<Vec<String>>,
    /// The fonts the document uses, walked again when it changes.
    walked: RefCell<Walked>,
    /// The new font set's form, open over the tab while it is shown.
    form: Option<forms::new_frit::Form>,
}

/// Where each part of the tab is drawn, for clicks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Places {
    /// Each item's row in the list, and the item's place.
    pub items: Vec<(Rect, usize)>,
    /// Each row of an open card, and the row's number.
    pub card: Vec<(Rect, usize)>,
    /// The list box's parts, while one is open.
    pub list: Option<list_box::Places>,
    /// Each row of the new font set's form, and the row's number.
    pub form: Vec<(Rect, usize)>,
}

/// What the preview shows, which the shell hands the pictures with the
/// facts of the terminal it knows: the font, whether as a picture of the
/// console or as sample characters at a size, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Previewed {
    pub font: Option<String>,
    pub picture: bool,
    pub points: u32,
    /// The font set, where only the document adds it, so the console
    /// cannot draw it before Save.
    pub in_document: Option<String>,
    pub place: Option<Place>,
}

impl Previewed {
    /// The preview's source, as the previews read it.
    pub fn source(&self) -> Source<'_> {
        let font = self.font.as_deref();
        if self.picture {
            Source::Font {
                font,
                detail: false,
            }
        } else {
            Source::Size {
                font,
                points: self.points,
            }
        }
    }
}

impl Looks {
    /// The tab on the list, with the first item highlighted. `color` is
    /// whether the TUI draws color
    /// ([`vtium_config_widgets::color::enabled`]).
    pub fn new(color: bool) -> Looks {
        Looks {
            color,
            locale: String::new(),
            highlight: 0,
            offset: 0,
            picture: false,
            open: Open::Nothing,
            filter: None,
            seen: Cell::new(Rect::default()),
            on_resource: Cell::new(false),
            names: RefCell::new(Vec::new()),
            walked: RefCell::new(Walked::default()),
            form: None,
        }
    }

    /// This tab, showing the detailed specimen's lines for `locale` in a
    /// vtium console.
    pub fn with_locale(self, locale: impl Into<String>) -> Looks {
        Looks {
            locale: locale.into(),
            ..self
        }
    }

    /// The highlighted item.
    pub fn highlighted(&self, shared: &Shared) -> Option<Item> {
        list::items(shared).into_iter().nth(self.highlight)
    }

    /// The new font set's form, if it is open.
    pub fn form(&self) -> Option<&forms::new_frit::Form> {
        self.form.as_ref()
    }

    /// The look card open over the list, if one is.
    pub fn card(&self) -> Option<&Card> {
        match &self.open {
            Open::Card(c) => Some(c),
            _ => None,
        }
    }

    /// Whether the preview shows a picture of the console in place of
    /// sample characters.
    pub fn picture(&self) -> bool {
        self.picture
    }

    /// The search whose row shows above the action bar: an open list
    /// box's, or the tab's over its list. The shell draws its row with
    /// `vtium_config_widgets::search::row::draw`.
    pub fn search(&self) -> Option<&Search> {
        if let Some(form) = &self.form {
            return form.search();
        }
        match &self.open {
            Open::Card(Card {
                open: CardOpen::List { list, .. },
                ..
            }) => list.filter(),
            Open::Nothing => self.filter.as_ref(),
            _ => None,
        }
    }

    /// The fonts the document uses, walked again where the document or
    /// the fonts found changed since the last walk.
    pub fn used(&self, shared: &Shared) -> Ref<'_, Used> {
        {
            let mut walked = self.walked.borrow_mut();
            let last = std::mem::take(&mut *walked);
            *walked = Walked::of(shared, Some(last));
        }
        Ref::map(self.walked.borrow(), |w| &w.used)
    }

    /// Takes what the list's node and the shell's search read from the
    /// shared state.
    fn follow(&self, shared: &Shared) {
        let items = list::items(shared);
        self.on_resource.set(
            matches!(self.open, Open::Nothing)
                && items.get(self.highlight).is_some_and(Item::is_resource),
        );
        *self.names.borrow_mut() = items.iter().map(|i| i.name(shared)).collect();
    }

    /// What the preview shows: the highlighted look's font set at its
    /// size, or the highlighted font set at vtium.conf's size, while the
    /// list is the screen. Nothing on a color scheme or a pointer theme,
    /// and while a card or a detail fills the viewer.
    pub fn preview(&self, shared: &Shared) -> Option<Previewed> {
        if !matches!(self.open, Open::Nothing) || self.form.is_some() {
            return None;
        }
        let text = shared.document().text();
        let size = |s: Option<String>| {
            s.or_else(|| vtium_settings::setting(text, "font-size"))
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0)
        };
        let (font, points) = match self.highlighted(shared)? {
            Item::Look(name) => {
                let look = list::look(text, &name)?;
                (card::font_set(shared, &look), size(look.size.clone()))
            }
            Item::FontSet(name) => (Some(name), size(None)),
            _ => return None,
        };
        let document = shared.document();
        let in_document = font.clone().filter(|f| {
            document.content().font_sets.kept.contains_key(f)
                && !document.installed().frits.contains_key(f)
        });
        Some(Previewed {
            font,
            picture: self.picture,
            points,
            in_document,
            place: draw::Panes::of(self.seen.get()).place(),
        })
    }

    /// Where each part of the tab is drawn on `shared`, in the viewer
    /// last drawn.
    pub fn places(&self, shared: &Shared) -> Places {
        match &self.form {
            Some(form) => {
                let (rows, list) = forms::new_frit::places(form, shared, self.seen.get());
                Places {
                    form: rows,
                    list,
                    ..Places::default()
                }
            }
            None => view::places(self, shared, self.seen.get()),
        }
    }
}

impl Tab<Shared, Action, Command> for Looks {
    fn name(&self) -> &'static str {
        words::LOOKS
    }

    fn letter(&self) -> char {
        'l'
    }

    /// The list, `looks` on a look or a font set and `looks-resources` on
    /// a color scheme or a pointer theme; a card and what is open on it;
    /// a detail; the new font set's form and what is open on it; or the
    /// search row while the person types in the list's or a list box's
    /// search.
    fn node(&self) -> &'static str {
        if let Some(form) = &self.form {
            return form.node();
        }
        match &self.open {
            Open::Nothing if self.filter.as_ref().is_some_and(Search::typing) => NODE_SEARCH,
            Open::Nothing if self.on_resource.get() => NODE_RESOURCES,
            Open::Nothing => NODE_LIST,
            Open::Card(c) => match &c.open {
                CardOpen::Typing(_) => NODE_TYPING,
                CardOpen::List { list, .. } if list.filter().is_some_and(Search::typing) => {
                    NODE_SEARCH
                }
                CardOpen::List { .. } => NODE_CARD_LIST,
                CardOpen::Nothing | CardOpen::Missing { .. } => NODE_CARD,
            },
            Open::Detail { item, .. } => match item {
                Item::Scheme(_) => NODE_PALETTE,
                Item::Pointer(_) => NODE_POINTER,
                _ => NODE_FONT_SET,
            },
        }
    }

    fn draw(&self, shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        self.seen.set(area);
        self.follow(shared);
        match &self.form {
            Some(form) => forms::new_frit::draw(form, shared, frame, area, self.color),
            None => view::draw(self, shared, frame, area),
        }
    }

    fn update(&mut self, shared: &Shared, action: Action) -> Vec<Command> {
        let out = match forms::update(self, shared, &action) {
            Some(out) => out,
            None => act::update(self, shared, action),
        };
        self.follow(shared);
        out
    }

    /// F1's text: the full facts of what is highlighted, a card line's
    /// choices, a role's fonts and why each is not active, or a whole
    /// detail; on the new font set's form, what a font set is and what
    /// each role holds. Nothing while a name is typed, where F1 does
    /// nothing.
    fn full_text(&self, shared: &Shared) -> Option<String> {
        match &self.form {
            Some(form) => forms::new_frit::full_text(form, shared),
            None => view::full_text(self, shared),
        }
    }

    /// Every look, font set, color scheme and pointer theme, for Find.
    fn find_items(&self, shared: &Shared) -> Vec<FindItem> {
        let used = self.used(shared);
        list::items(shared)
            .iter()
            .enumerate()
            .filter(|(_, i)| !matches!(i, Item::NoLooks))
            .map(|(index, item)| view::find_item(shared, item, index, &used))
            .collect()
    }

    fn searchable(&self) -> Option<&dyn Searchable> {
        Some(self)
    }
}

/// The `?` search over the list, by each item's name.
impl Searchable for Looks {
    fn search_node(&self) -> &'static str {
        if self.on_resource.get() {
            NODE_RESOURCES
        } else {
            NODE_LIST
        }
    }

    fn count(&self) -> usize {
        self.names.borrow().len()
    }

    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>> {
        (field == 0)
            .then(|| self.names.borrow().get(item).cloned())
            .flatten()
            .map(Cow::Owned)
    }
}
