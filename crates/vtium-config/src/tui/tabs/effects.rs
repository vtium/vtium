//! The Effects tab, the second tab of the bar. It lists the effects a look
//! can use: the vtlooks, the color maps and the shaders. A filter line
//! above the list shows one group or all three. The vtlooks come first:
//! No filter, the built-in vtlook, then the presets the package ships, then
//! the administrator's vtlooks, where one that takes a preset's name hides
//! the preset. Each color map and shader shows whether it is active and
//! whether it passes the trust rule, and Space activates or deactivates it
//! with the controls the resource tabs share. Enter on the filter line
//! opens the list of groups, and Enter on an item opens its detail, which
//! changes nothing.
//!
//! Under the list, the tab previews the highlighted item: the sample
//! screen of the Looks tab, in the draft's font and size, drawn through
//! the vtlook or color map, with a line that says what the picture shows.
//! Where vtium cannot use the item, such as a color map that is not active
//! or a shader, the picture shows the console without it and the line
//! says why.
//!
//! References:
//! - ADR-1205 D1: the Effects tab, and the controls it shares with the
//!   Fonts tab.
//! - ADR-1205 D3: the folders the tab finds color maps and shaders in.
//! - ADR-1205 D8: the detail of an effect, the warning before one a look
//!   uses is deactivated, and the preview of an effect as a picture of the
//!   console, which leaves the live console alone.
//! - ADR-1304 D4, its note of 2026-11-19: No filter first, then the
//!   presets, then the administrator's vtlooks.
//! - ADR-1403 D10: the tab's filter line, its one list in groups, and its
//!   keys.

use super::super::canvas::{Row, span};
use super::super::color::Style;
use super::super::effects_found::From;
use super::super::input::Key;
use super::super::layout::PICTURE_ROW;
use super::super::listbox::{Body, Item, ListBox, Marks, plain, prose, rows, step, tail};
use super::super::model::{Font, FontState, Shared, Size};
use super::super::resources::{self, Resource};
use super::super::shot::{Applied, Preview, Source};
use super::super::words;
use super::{Command, Room, Tab};
use crate::batch::Use;

/// Which groups the list shows: all three, or one of
/// [`words::EFFECTS_GROUPS`] by its place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Show {
    #[default]
    All,
    Group(usize),
}

impl Show {
    /// The choices of the list of groups, in its order: all, then each
    /// group.
    const CHOICES: [Show; 4] = [Show::All, Show::Group(0), Show::Group(1), Show::Group(2)];

    fn name(self) -> &'static str {
        match self {
            Show::All => words::EFFECTS_ALL,
            Show::Group(g) => words::EFFECTS_GROUPS.get(g).copied().unwrap_or(""),
        }
    }

    fn shows(self, group: usize) -> bool {
        self == Show::All || self == Show::Group(group)
    }
}

/// One item of the list: a vtlook, a color map or a shader, by its place
/// in what the tab found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Vtlook(usize),
    Map(usize),
    Shader(usize),
}

/// What is open over the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    /// The list of groups, which the filter line opens.
    Filter(ListBox),
    /// An item's detail, with its first row in sight.
    Detail { item: Effect, top: usize },
}

/// The Effects tab's state: the highlighted row, where row 0 is the filter
/// line and each item follows; the window of the list; the groups shown;
/// and what is open over the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    list: ListBox,
    show: Show,
    open: Option<Open>,
}

impl Effects {
    /// The items the list shows, in its order.
    pub fn items(&self, shared: &Shared) -> Vec<Effect> {
        let found = &shared.inputs.effects;
        let mut out = Vec::new();
        if self.show.shows(0) {
            out.extend((0..found.vtlooks.len()).map(Effect::Vtlook));
        }
        if self.show.shows(1) {
            out.extend((0..found.maps.len()).map(Effect::Map));
        }
        if self.show.shows(2) {
            out.extend((0..found.shaders.len()).map(Effect::Shader));
        }
        out
    }

    /// The highlighted item, `None` on the filter line.
    pub fn highlighted(&self, shared: &Shared) -> Option<Effect> {
        let at = self.list.at.checked_sub(1)?;
        self.items(shared).get(at).copied()
    }

    /// The color map or shader `item` is, if it is one.
    pub fn resource(shared: &Shared, item: Effect) -> Option<&Resource> {
        let found = &shared.inputs.effects;
        match item {
            Effect::Map(i) => found.maps.get(i),
            Effect::Shader(i) => found.shaders.get(i),
            Effect::Vtlook(_) => None,
        }
    }

    /// `item` as the preview applies it.
    fn applied(shared: &Shared, item: Option<Effect>) -> Applied<'_> {
        let found = &shared.inputs.effects;
        match item {
            Some(Effect::Vtlook(i)) => found
                .vtlooks
                .get(i)
                .map_or(Applied::Nothing, Applied::Vtlook),
            Some(Effect::Map(i)) => found.maps.get(i).map_or(Applied::Nothing, Applied::Map),
            Some(Effect::Shader(i)) => found
                .shaders
                .get(i)
                .map_or(Applied::Nothing, Applied::Shader),
            None => Applied::Nothing,
        }
    }

    /// The font the preview draws the sample screen in: the font the
    /// draft gives the console's head, from its look or else the top
    /// level, else the font the console shows, else the first font that
    /// reads.
    fn font(shared: &Shared) -> Option<&Font> {
        let fonts = &shared.inputs.fonts;
        let named = |name: Option<String>| name.and_then(|n| fonts.iter().find(|f| f.name == n));
        named(shared.head_font())
            .or_else(|| named(shared.shown_font.clone()))
            .or_else(|| {
                fonts
                    .iter()
                    .find(|f| !matches!(f.state, FontState::Refused(_)))
            })
    }

    /// The looks of the draft that use the color map or shader `r`, and
    /// the vtlook each uses it through.
    fn uses(shared: &Shared, r: &Resource) -> Vec<Use> {
        let conf = shared.conf_text().unwrap_or_default();
        shared.inputs.effects.uses(conf, &r.path)
    }

    /// The names of the looks in `uses`, as a detail lists them.
    fn names(uses: &[Use]) -> Vec<String> {
        uses.iter()
            .map(|u| words::look_name(u.look.as_deref()))
            .collect()
    }

    /// `item` as an item of the list: its name and its marks, and the
    /// heading of its group when it is the group's first.
    fn item(shared: &Shared, item: Effect) -> Item {
        let found = &shared.inputs.effects;
        let (name, detail) = match item {
            Effect::Vtlook(i) => found.vtlooks.get(i).map_or_else(Default::default, |v| {
                let mut marks = vec![match v.from {
                    From::BuiltIn => words::VTLOOK_BUILT_IN,
                    From::Preset => words::VTLOOK_PRESET,
                    From::Administrator => words::VTLOOK_ADMIN,
                }];
                if v.hidden {
                    marks.push(words::VTLOOK_HIDDEN);
                }
                if v.read.is_err() {
                    marks.push(words::VTLOOK_BROKEN);
                }
                let name = match v.from {
                    From::BuiltIn => words::NO_FILTER.to_owned(),
                    _ => v.name.clone(),
                };
                (name, marks.join(", "))
            }),
            Effect::Map(_) | Effect::Shader(_) => {
                Self::resource(shared, item).map_or_else(Default::default, |r| (r.name(), r.row()))
            }
        };
        let mut out = Item::new(name, detail);
        let first = matches!(item, Effect::Vtlook(0) | Effect::Map(0) | Effect::Shader(0));
        if first {
            let group = match item {
                Effect::Vtlook(_) => 0,
                Effect::Map(_) => 1,
                Effect::Shader(_) => 2,
            };
            out.heading = words::EFFECTS_GROUPS.get(group).map(|g| (*g).to_owned());
        }
        out
    }

    /// The detail of `item`, one line a paragraph.
    pub fn detail(shared: &Shared, item: Effect) -> Vec<String> {
        let found = &shared.inputs.effects;
        let conf = shared.conf_text().unwrap_or_default();
        if let Effect::Vtlook(i) = item {
            let Some(v) = found.vtlooks.get(i) else {
                return Vec::new();
            };
            let name = match v.from {
                From::BuiltIn => words::NO_FILTER,
                _ => &v.name,
            };
            let file = v.file.as_ref().map(|f| f.display().to_string());
            let mut out = vec![
                words::vtlook_title(name),
                words::detail_file(file.as_deref()),
                words::vtlook_reads(v.read.as_ref().err().map(String::as_str)),
            ];
            match &v.read {
                Ok(filters) if filters.is_empty() => out.push(words::VTLOOK_NO_FILTERS.to_owned()),
                Ok(filters) => {
                    out.push(words::VTLOOK_FILTERS.to_owned());
                    out.extend(filters.iter().map(|f| format!("  {f}")));
                }
                Err(_) => {}
            }
            if !v.files.is_empty() {
                out.push(words::VTLOOK_FILES.to_owned());
                out.extend(v.files.iter().map(|f| {
                    let named = match found.resource(f) {
                        None => words::Named::Missing,
                        Some(r) if !r.active() => words::Named::Inactive,
                        Some(r) if r.trust.is_err() => words::Named::Untrusted,
                        Some(_) => words::Named::Usable,
                    };
                    format!("  {}", words::vtlook_file(&f.display().to_string(), named))
                }));
            }
            let users: Vec<String> = crate::batch::drawing_vtlook(conf, &v.name)
                .iter()
                .map(|look| words::look_name(look.as_deref()))
                .collect();
            out.push(words::detail_users(&users));
            return out;
        }
        let Some(r) = Self::resource(shared, item) else {
            return Vec::new();
        };
        let mut out = vec![
            words::resource_title(r.kind, &r.name()),
            words::detail_file(Some(&r.path.display().to_string())),
        ];
        if r.own {
            out.push(words::detail_own(r.kind));
        } else {
            out.push(words::detail_state(words::active_mark(r.state)));
            out.push(words::detail_trust(
                r.kind,
                r.trust.as_ref().err().map(String::as_str),
            ));
        }
        out.push(words::detail_users(&Self::names(&Self::uses(shared, r))));
        out
    }

    /// The tab's body while nothing is open: the filter line, the list in
    /// its groups, and the sentences under it.
    fn list_body(&self, shared: &Shared, size: Size) -> Body {
        let cols = size.cols;
        let filter = format!(" {}", words::effects_filter(self.show.name()));
        let style = if self.list.at == 0 {
            Style::PLAIN.reversed()
        } else {
            Style::PLAIN
        };
        let mut list: Vec<Row> = vec![vec![span(filter, style)]];
        let items: Vec<Item> = self
            .items(shared)
            .into_iter()
            .map(|i| Self::item(shared, i))
            .collect();
        let at = self.list.at.checked_sub(1).unwrap_or(usize::MAX);
        let (shown, keep) = rows(&items, at, Marks::None, cols);
        list.extend(shown);
        let mut sentence = String::new();
        if let Some(note) = &shared.inputs.effects.note {
            sentence.push_str(note);
            sentence.push(' ');
        }
        for (g, group) in words::EFFECTS_GROUPS.iter().enumerate() {
            let found = &shared.inputs.effects;
            let empty = match g {
                1 => found.maps.is_empty(),
                2 => found.shaders.is_empty(),
                _ => false,
            };
            if empty && self.show.shows(g) {
                sentence.push_str(&words::effects_none(group));
                sentence.push(' ');
            }
        }
        sentence.push_str(words::EFFECTS_HINT);
        let mut under = tail(&sentence, cols);
        if let Some(line) = Tab::preview(self, shared, size).preview_line() {
            under.extend(tail(&line, cols).into_iter().skip(1));
        }
        Body {
            list,
            keep: if self.list.at == 0 {
                0
            } else {
                keep.saturating_add(1)
            },
            tail: under,
        }
    }
}

impl Tab for Effects {
    fn name(&self) -> &'static str {
        words::EFFECTS
    }

    fn node(&self) -> &'static str {
        match self.open {
            None => "effects",
            Some(Open::Filter(_)) => "effects.filter",
            Some(Open::Detail { .. }) => "effects.effect",
        }
    }

    fn body(&self, shared: &Shared, size: Size) -> Body {
        match self.open {
            None => self.list_body(shared, size),
            Some(Open::Filter(lb)) => {
                let items: Vec<Item> = Show::CHOICES
                    .iter()
                    .map(|s| Item {
                        on: *s == self.show,
                        ..Item::new(s.name(), "")
                    })
                    .collect();
                let (list, keep) = rows(&items, lb.at, Marks::Choice, size.cols);
                Body {
                    list,
                    keep,
                    tail: tail(words::EFFECTS_FILTER_HINT, size.cols),
                }
            }
            Some(Open::Detail { item, top }) => {
                // the title stays at the top, so the detail always says
                // what it describes, and the lines under it scroll
                let lines = Self::detail(shared, item);
                let mut list: Vec<Row> = Vec::new();
                for line in lines.iter().take(1).chain(lines.iter().skip(1).skip(top)) {
                    list.extend(prose(line, size.cols));
                }
                if list.is_empty() {
                    list.push(plain(""));
                }
                Body {
                    list,
                    keep: 0,
                    tail: tail(words::EFFECTS_DETAIL_HINT, size.cols),
                }
            }
        }
    }

    /// Under the list and the sentences under it, from the left edge,
    /// since the list's marks reach across the screen.
    fn picture_at(&self, shared: &Shared, size: Size) -> (usize, usize) {
        let body = self.list_body(shared, size);
        (
            PICTURE_ROW
                .saturating_add(body.list.len())
                .saturating_add(body.tail.len())
                .saturating_add(1),
            2,
        )
    }

    /// The highlighted item applied to the sample screen, while the list
    /// shows; nothing while the list of groups or a detail is open.
    fn preview<'a>(&self, shared: &'a Shared, size: Size) -> Preview<'a> {
        if self.open.is_some() {
            return Preview::nothing(shared, size);
        }
        Preview::of(
            shared,
            size,
            Source::Effect {
                font: Self::font(shared),
                effect: Self::applied(shared, self.highlighted(shared)),
            },
        )
    }

    fn offset(&self) -> usize {
        match self.open {
            None => self.list.offset,
            _ => 0,
        }
    }

    fn show_from(&mut self, row: usize) {
        if self.open.is_none() {
            self.list.offset = row;
        }
    }

    /// The action keys act while nothing is open over the list, and `o`
    /// does nothing, since it opens Restore in the Flow view alone.
    ///
    /// References:
    /// - ADR-1403 D7: the action keys do nothing while a layer is open, and
    ///   the `o.view` row.
    fn takes_action(&self, action: char) -> bool {
        self.open.is_none() && action != 'o'
    }

    fn update(&mut self, shared: &Shared, _room: Room, key: Key) -> Vec<Command> {
        match self.open {
            Some(Open::Filter(mut lb)) => {
                match lb.key(key, Show::CHOICES.len()) {
                    super::super::listbox::Act::Picked(i) => {
                        self.show = Show::CHOICES.get(i).copied().unwrap_or_default();
                        self.list = ListBox::default();
                        self.open = None;
                    }
                    super::super::listbox::Act::Closed => self.open = None,
                    _ => self.open = Some(Open::Filter(lb)),
                }
                Vec::new()
            }
            Some(Open::Detail { item, top }) => {
                // the title stays, so the lines under it scroll as far as
                // the last one
                let last = Self::detail(shared, item).len().saturating_sub(2);
                self.open = match key {
                    Key::Escape => None,
                    Key::Up => Some(Open::Detail {
                        item,
                        top: top.saturating_sub(1),
                    }),
                    Key::Down => Some(Open::Detail {
                        item,
                        top: top.saturating_add(1).min(last),
                    }),
                    _ => self.open,
                };
                Vec::new()
            }
            None => match key {
                Key::Enter => {
                    self.open = Some(match self.highlighted(shared) {
                        Some(item) => Open::Detail { item, top: 0 },
                        None => Open::Filter(ListBox::at(
                            Show::CHOICES
                                .iter()
                                .position(|s| *s == self.show)
                                .unwrap_or(0),
                        )),
                    });
                    Vec::new()
                }
                Key::Char(' ') => match self
                    .highlighted(shared)
                    .and_then(|item| Self::resource(shared, item))
                {
                    Some(r) => resources::space(r, &Self::uses(shared, r)),
                    None => Vec::new(),
                },
                _ => {
                    let n = self.items(shared).len().saturating_add(1);
                    self.list.at = step(self.list.at, n, key);
                    Vec::new()
                }
            },
        }
    }
}
