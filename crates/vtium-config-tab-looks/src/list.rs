//! The Looks tab's list: the looks of the open configuration, then the
//! font sets, the color schemes and the pointer themes, each group under
//! its heading, and the pane at the left that draws them.
//!
//! A look is a `[look NAME]` section of vtium.conf. Each look's row
//! carries the mark `· 2 fonts not active` where its fonts are not all
//! active, so the gap shows before the console draws without them. The
//! font sets are the installed ones and those the document adds; one the
//! document removes keeps its row with the word that says so. The color
//! schemes are the built-in ones, then the scheme files found, and the
//! pointer themes are those found. Every item is read from the document
//! and from the resources found when vtium-config started, so moving
//! through the list reads no folder.
//!
//! References:
//! - ADR-1403 D4: a look is a `[look NAME]` section, which a terminal names with `look =`.
//! - ADR-1403 D11, "The tabs' layouts", the Looks bullet: the list in four groups at the left.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated": a look's own mark in its row.

use std::collections::BTreeSet;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::ui::Shared;
use vtium_config_widgets::canvas;
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::Search;

use crate::draw::put;
use crate::words;

/// One look of the document, with the keys of it that its card shows,
/// each `None` where the look leaves the key to vtium.conf's top level.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Look {
    pub name: String,
    pub font_set: Option<String>,
    pub size: Option<String>,
    pub palette: Option<String>,
    pub vtlook: Option<String>,
    /// The look's own `pointer-theme`: the SVG file or cursor theme folder
    /// whose pictures its pointer takes.
    pub pointer: Option<String>,
}

/// The looks of vtium.conf's text `text`, in the file's order. A text
/// whose sections do not read has none.
pub fn looks(text: &str) -> Vec<Look> {
    let Some(reading) = vtium_settings::sections(text.as_bytes()) else {
        return Vec::new();
    };
    reading
        .parsed
        .config
        .looks
        .into_iter()
        .map(|l| {
            let value = |key: &str| {
                l.settings
                    .iter()
                    .rev()
                    .find(|s| s.key.name == key)
                    .map(|s| s.value.clone())
            };
            Look {
                font_set: value("font-name"),
                size: value("font-size"),
                palette: value("palette"),
                vtlook: l.vtlook.clone(),
                pointer: value("pointer-theme"),
                name: l.name,
            }
        })
        .collect()
}

/// The look named `name` in `text`.
pub fn look(text: &str, name: &str) -> Option<Look> {
    looks(text).into_iter().find(|l| l.name == name)
}

/// The terminals of `text` that draw the look `name`: those whose
/// `look =` names it, and for `default` those that name none. A text that
/// runs vtium's built-in terminal has its terminal `console`, which draws
/// `default`.
pub fn users(text: &str, name: &str) -> Vec<String> {
    vtium_config_core::ops::heads_run(text)
        .into_iter()
        .filter(|h| h.look == name)
        .map(|h| h.name)
        .collect()
}

/// A color scheme of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// A scheme built into vtium, by its name.
    BuiltIn(&'static str),
    /// A scheme file, by its place among the files found.
    File(usize),
}

/// One item of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// A look of the document, by its name.
    Look(String),
    /// The looks group's one row while the document has no look.
    NoLooks,
    /// A font set, by its name.
    FontSet(String),
    /// A color scheme.
    Scheme(Scheme),
    /// A pointer theme, by its place among the themes found.
    Pointer(usize),
}

/// The four groups of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Looks,
    FontSets,
    Schemes,
    Pointers,
}

impl Item {
    /// The group the item stands in.
    pub fn group(&self) -> Group {
        match self {
            Item::Look(_) | Item::NoLooks => Group::Looks,
            Item::FontSet(_) => Group::FontSets,
            Item::Scheme(_) => Group::Schemes,
            Item::Pointer(_) => Group::Pointers,
        }
    }

    /// Whether the item is a color scheme or a pointer theme, the items
    /// Space activates, which put the list in the node `looks-resources`.
    pub fn is_resource(&self) -> bool {
        matches!(self.group(), Group::Schemes | Group::Pointers)
    }

    /// The item's name as its row and the search show it.
    pub fn name(&self, shared: &Shared) -> String {
        match self {
            Item::Look(name) | Item::FontSet(name) => name.clone(),
            Item::NoLooks => words::NO_LOOKS.to_owned(),
            Item::Scheme(Scheme::BuiltIn(name)) => (*name).to_owned(),
            Item::Scheme(Scheme::File(at)) => shared
                .found
                .palettes
                .files
                .get(*at)
                .map(|f| f.name.clone())
                .unwrap_or_default(),
            Item::Pointer(at) => shared
                .found
                .pointers
                .get(*at)
                .map(|r| pointer_name(&r.name()))
                .unwrap_or_default(),
        }
    }
}

/// A pointer theme's name as the list shows it: vtium's own pictures by
/// that name, and a folder by its name.
pub fn pointer_name(file: &str) -> String {
    if file.ends_with(".svg") {
        words::POINTER_OWN.to_owned()
    } else {
        canvas::printable(file)
    }
}

/// The font sets the document sees, by name, in order: the installed
/// ones and those the document adds.
pub fn font_sets(shared: &Shared) -> Vec<String> {
    let document = shared.document();
    let mut names: BTreeSet<String> = document.installed().frits.keys().cloned().collect();
    names.extend(document.content().font_sets.kept.keys().cloned());
    names.into_iter().collect()
}

/// Every item of the list, in its order: the looks, the font sets, the
/// color schemes, then the pointer themes.
pub fn items(shared: &Shared) -> Vec<Item> {
    let mut out: Vec<Item> = looks(shared.document().text())
        .into_iter()
        .map(|l| Item::Look(l.name))
        .collect();
    if out.is_empty() {
        out.push(Item::NoLooks);
    }
    out.extend(font_sets(shared).into_iter().map(Item::FontSet));
    out.extend(
        shared
            .found
            .palettes
            .built_in
            .iter()
            .map(|n| Item::Scheme(Scheme::BuiltIn(n))),
    );
    out.extend(
        (0..shared.found.palettes.files.len())
            .filter(|&at| {
                shared
                    .found
                    .palettes
                    .files
                    .get(at)
                    .is_some_and(|f| !f.resource.hidden)
            })
            .map(|at| Item::Scheme(Scheme::File(at))),
    );
    out.extend((0..shared.found.pointers.len()).map(Item::Pointer));
    out
}

/// One row the list pane draws: a group's heading, or an item by its
/// place in [`items`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Heading(Group),
    Item(usize),
}

/// The rows of the list: each group's heading before its first item, then
/// the items, and only those `filter` keeps where it is given, with the
/// headings of the groups that keep one.
pub fn rows(items: &[Item], filter: Option<&Search>) -> Vec<Row> {
    let mut out = Vec::new();
    let mut last = None;
    for (i, item) in items.iter().enumerate() {
        if filter.is_some_and(|f| !f.is_match(i)) {
            continue;
        }
        let group = item.group();
        if last != Some(group) {
            out.push(Row::Heading(group));
            last = Some(group);
        }
        out.push(Row::Item(i));
    }
    out
}

/// What the list pane draws.
pub struct Pane<'a> {
    pub shared: &'a Shared,
    pub items: &'a [Item],
    pub rows: &'a [Row],
    /// The highlighted item, by its place in `items`.
    pub highlight: Option<usize>,
    /// The first row shown.
    pub offset: usize,
    /// Each look's mark where its fonts are not all active.
    pub marks: &'a dyn Fn(&str) -> Option<String>,
}

impl Pane<'_> {
    /// Draws the pane into `area`, and returns where each item's row went.
    pub fn draw(&self, buf: &mut Buffer, area: Rect, color: bool) -> Vec<(Rect, usize)> {
        let mut places = Vec::new();
        let width = usize::from(area.width);
        for (dy, row) in (0..area.height).zip(self.rows.iter().skip(self.offset)) {
            let y = area.y.saturating_add(dy);
            match *row {
                Row::Heading(g) => {
                    let style = Role::Heading.style(color).cell();
                    put(
                        buf,
                        area.x.saturating_add(1),
                        y,
                        words::group(g),
                        style,
                        area,
                    );
                }
                Row::Item(i) => {
                    let Some(item) = self.items.get(i) else {
                        continue;
                    };
                    let lit = self.highlight == Some(i);
                    let role = if lit { Role::RowHighlight } else { Role::Row };
                    let line = Rect::new(area.x, y, area.width, 1);
                    let style = role.style(color).cell();
                    put(buf, area.x, y, &" ".repeat(width), style, line);
                    let mut text = item.name(self.shared);
                    let mark = match item {
                        Item::Look(name) => (self.marks)(name),
                        Item::FontSet(name) => font_set_mark(self.shared, name),
                        _ => None,
                    };
                    if let Some(mark) = mark {
                        text = format!("{text} · {mark}");
                    }
                    let shown = canvas::fit(&text, width.saturating_sub(4));
                    put(buf, area.x.saturating_add(3), y, &shown, style, line);
                    places.push((line, i));
                }
            }
        }
        places
    }
}

/// The word beside a font set that the document adds or removes.
fn font_set_mark(shared: &Shared, name: &str) -> Option<String> {
    let document = shared.document();
    let sets = &document.content().font_sets;
    if sets.removed.contains(name) {
        Some(words::REMOVED_AT_SAVE.to_owned())
    } else if sets.kept.contains_key(name) && !document.installed().frits.contains_key(name) {
        Some(words::NEW_NOT_SAVED.to_owned())
    } else {
        None
    }
}

/// The first row the pane shows so that `at`, a row's place, stays in
/// sight in `height` rows, from the offset `offset` it showed last.
pub fn settled(at: Option<usize>, offset: usize, height: usize, len: usize) -> usize {
    let height = height.max(1);
    let mut offset = offset;
    if let Some(at) = at {
        if at < offset {
            // the heading above the first item of a group shows with it
            offset = at.saturating_sub(1);
        } else if at >= offset.saturating_add(height) {
            offset = at.saturating_add(1).saturating_sub(height);
        }
    }
    offset.min(len.saturating_sub(height))
}
