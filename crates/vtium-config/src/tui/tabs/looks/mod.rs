//! The Looks tab: one list in four groups, the looks of the draft, the
//! font sets, the color schemes and the installed pointer themes, with a
//! preview of the highlighted look, font set or color scheme. Enter on a
//! look opens its card, and Enter on any other item opens its detail. `n`
//! makes a new look among the looks and opens the form for a new font set
//! among the font sets. `r` deletes a look after a question and marks a
//! font set for removal at the next install. `t` tries the look's font set
//! or the font set on the console, and `p` switches the preview between
//! sample characters and a picture of the console. Space activates or
//! deactivates a color scheme file or a pointer theme, with the controls
//! the resource tabs share, and asks first when a look uses it. Among the
//! color schemes and the pointer themes, `n`, `r`, `t` and `p` do nothing,
//! and Space does nothing on a scheme built into vtium.
//!
//! The tab is made of groups, each in a module of its own: the looks, the
//! look card, the font sets, the new-font-set form, the color schemes and
//! the pointer themes. This module holds the tab's state and composes the
//! groups. [`Entry`] and [`Looks::entries`] list the groups in the list's
//! order.
//!
//! References:
//! - ADR-1205 D1: the Looks tab assembles each look and keeps the form
//!   for a new font set.
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.
//! - ADR-1205 D6, D10: the Looks tab lists the pointer themes and the
//!   color schemes, and activates them.
//! - ADR-1403 D10: the tab's list in groups, its keys, and Enter going
//!   down a level while Esc comes back up.
//! - ADR-1403 D10, its notes of 2026-11-19: what each key does among the
//!   color schemes and the pointer themes.

pub mod card;
pub mod fontsets;
pub mod list;
pub mod newfrit;
pub mod palettes;
pub mod pointers;

use super::super::color::Style;
use super::super::edit::Change as DraftChange;
use super::super::input::Key;
use super::super::layout::{PICTURE_COL, PICTURE_ROW};
use super::super::listbox::{Body, Item, ListBox, Marks, plain, prose, rows, step, tail};
use super::super::model::{DpiFrom, Font, FontState, Shared, Size, Then, View};
use super::super::resources;
use super::super::shot::{Preview, Source};
use super::super::words;
use super::{Change, Command, Request, Room, Tab};
use crate::batch::Use;
use resources::Resource;

/// One item of the tab's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A look of the draft, by its name.
    Look(String),
    /// The looks group's one row while the draft has no look.
    NoLooks,
    /// A font set, by its place in the list of fonts.
    FontSet(usize),
    /// A color scheme, built in or a file.
    Palette(palettes::Scheme),
    /// A pointer theme, by its place in the list of pointer themes.
    Pointer(usize),
}

/// What is open over the list.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Open {
    /// A look's card.
    Card(card::Card),
    /// A font set's detail, with its first row under the title in sight.
    FontSet { at: usize, top: usize },
    /// A color scheme's detail, scrolled the same way.
    Palette {
        scheme: palettes::Scheme,
        top: usize,
    },
    /// A pointer theme's detail, scrolled the same way.
    Pointer { at: usize, top: usize },
}

/// Where the Looks tab stands: the highlighted item and the window of the
/// list, whether the preview shows a picture of the console or sample
/// characters, what is open over the list, and the form for a new font
/// set, which keeps what its fields hold while it is closed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Looks {
    list: ListBox,
    picture: bool,
    open: Option<Open>,
    form: newfrit::Form,
}

impl Looks {
    /// The items of the list, in its order: the looks, the font sets, the
    /// color schemes, then the pointer themes.
    pub fn entries(shared: &Shared) -> Vec<Entry> {
        let mut out: Vec<Entry> = list::looks(shared)
            .into_iter()
            .map(|l| Entry::Look(l.name))
            .collect();
        if out.is_empty() {
            out.push(Entry::NoLooks);
        }
        out.extend((0..shared.inputs.fonts.len()).map(Entry::FontSet));
        out.extend(palettes::entries(shared).into_iter().map(Entry::Palette));
        out.extend((0..shared.inputs.effects.pointers.len()).map(Entry::Pointer));
        out
    }

    /// The resource of `entry` that Space acts on, with the looks that
    /// use it: a color scheme file or a pointer theme. A scheme built
    /// into vtium is always active, and the other items are no resource.
    pub fn resource(shared: &Shared, entry: &Entry) -> Option<(Resource, Vec<Use>)> {
        match entry {
            Entry::Palette(palettes::Scheme::File(at)) => palettes::file(shared, *at).map(|f| {
                let uses = palettes::uses(shared, &f.resource.path);
                (f.resource.clone(), uses)
            }),
            Entry::Pointer(at) => {
                pointers::theme(shared, *at).map(|r| (r.clone(), pointers::uses(shared, &r.path)))
            }
            _ => None,
        }
    }

    /// The highlighted item.
    pub fn highlighted(&self, shared: &Shared) -> Option<Entry> {
        let entries = Self::entries(shared);
        let at = self.list.at.min(entries.len().saturating_sub(1));
        entries.into_iter().nth(at)
    }

    /// The look card open over the list, if one is.
    pub fn card(&self) -> Option<&card::Card> {
        match &self.open {
            Some(Open::Card(c)) => Some(c),
            _ => None,
        }
    }

    /// The new frit the form for a new font set holds.
    pub fn form(&self) -> &newfrit::NewFrit {
        self.form.frit()
    }

    /// The form for a new font set, which answers for the form's level as
    /// a tab does.
    pub fn new_frit(&self) -> &newfrit::Form {
        &self.form
    }

    /// The form for a new font set to change.
    pub fn new_frit_mut(&mut self) -> &mut newfrit::Form {
        &mut self.form
    }

    /// Whether the preview shows a picture of the console in place of
    /// sample characters.
    pub fn picture(&self) -> bool {
        self.picture
    }

    /// The look the tab previews, by its name: the look whose card is open,
    /// else the highlighted look.
    fn look_shown(&self, shared: &Shared) -> Option<String> {
        if let Some(c) = self.card() {
            return Some(c.name().to_owned());
        }
        match self.highlighted(shared) {
            Some(Entry::Look(name)) => Some(name),
            _ => None,
        }
    }

    /// The font set the tab previews: the font set of the look it shows,
    /// else vtium.conf's, or the highlighted or opened font set.
    fn font_set(&self, shared: &Shared) -> Option<String> {
        if let Some(Open::FontSet { at, .. }) = self.open {
            return shared.inputs.fonts.get(at).map(|f| f.name.clone());
        }
        if let Some(name) = self.look_shown(shared) {
            return list::look(shared, &name)
                .and_then(|l| l.frit)
                .or_else(|| shared.drafted("font-name"));
        }
        match self.highlighted(shared) {
            Some(Entry::FontSet(at)) => shared.inputs.fonts.get(at).map(|f| f.name.clone()),
            _ => shared.drafted("font-name"),
        }
    }

    /// The size the tab previews, in points, 0 for automatic: the size of
    /// the look it shows, else vtium.conf's.
    pub fn points(&self, shared: &Shared) -> u32 {
        self.look_shown(shared)
            .and_then(|name| list::look(shared, &name))
            .and_then(|l| l.size)
            .or_else(|| shared.drafted("font-size"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    /// The list's body on a screen of `size`: the names of the looks and
    /// the font sets, each group under its heading, so the preview fits
    /// beside them; and under them what the highlighted item is and what
    /// the preview shows.
    fn list_body(&self, shared: &Shared, size: Size) -> Body {
        let looks = list::looks(shared);
        let entries = Self::entries(shared);
        let at = self.list.at.min(entries.len().saturating_sub(1));
        let mut about = String::new();
        let items: Vec<Item> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let full = match e {
                    Entry::Look(name) => looks
                        .iter()
                        .find(|l| l.name == *name)
                        .map(|l| list::item(shared, l))
                        .unwrap_or_default(),
                    Entry::NoLooks => Item::new(words::NO_LOOKS, words::NO_LOOKS_NOTE),
                    Entry::FontSet(at) => fontsets::item(shared, *at),
                    Entry::Palette(scheme) => palettes::item(shared, *scheme),
                    Entry::Pointer(at) => pointers::item(shared, *at),
                };
                if i == at {
                    about = match e {
                        // a look's detail is a whole sentence of its own
                        Entry::Look(_) => full.detail.clone(),
                        _ => words::item_about(&full.name, &full.detail),
                    };
                }
                let mut item = Item::new(full.name, "");
                let group = |e: Option<&Entry>| match e {
                    Some(Entry::Look(_) | Entry::NoLooks) => Some(words::LOOKS_GROUP),
                    Some(Entry::FontSet(_)) => Some(words::FONT_SETS_GROUP),
                    Some(Entry::Palette(_)) => Some(words::COLOR_SCHEMES_GROUP),
                    Some(Entry::Pointer(_)) => Some(words::POINTERS_GROUP),
                    None => None,
                };
                let heading = group(Some(e));
                if i == 0 || group(entries.get(i.wrapping_sub(1))) != heading {
                    item.heading = heading.map(str::to_owned);
                }
                item
            })
            .collect();
        let (list, keep) = rows(&items, at, Marks::None, size.cols);
        let mut sentence = about;
        // a color scheme's preview is its colors under the list, and a
        // pointer theme has none
        let highlighted = entries.get(at);
        if let Some(Entry::Palette(scheme)) = highlighted {
            let swatches = palettes::preview(shared, *scheme);
            if let Some(note) = palettes::preview_note(shared, *scheme) {
                sentence.push(' ');
                sentence.push_str(note);
            }
            let mut tail = vec![plain("")];
            if !swatches.is_empty() {
                tail.extend(swatches);
                tail.push(plain(""));
            }
            tail.extend(prose(&sentence, size.cols));
            return Body { list, keep, tail };
        }
        if let Some(Entry::Pointer(_)) = highlighted {
            return Body {
                list,
                keep,
                tail: tail(&sentence, size.cols),
            };
        }
        let preview = Tab::preview(self, shared, size);
        let line = preview.preview_line();
        // a picture says the density it is drawn at, and automatic has
        // none, since vtium picks the size for each display
        if shared.graphics && !shared.text_preview() && !self.picture {
            sentence.push(' ');
            if self.points(shared) == 0 {
                sentence.push_str(words::AUTOMATIC_PICTURE);
            } else {
                let (dpi, from) = shared.inputs.dpi;
                sentence.push_str(&words::density(
                    dpi,
                    match from {
                        DpiFrom::Conf => words::DPI_FROM_CONF,
                        DpiFrom::Edid => words::DPI_FROM_EDID,
                        DpiFrom::Assumed => words::DPI_ASSUMED,
                    },
                ));
            }
        }
        if let Some(line) = line {
            sentence.push(' ');
            sentence.push_str(&line);
        }
        // the console's own preview draws its text beside the names, so the
        // sentences keep to the names' column there; a picture leaves the
        // cells under it alone, and a written description has nothing
        // beside it, so both take the whole width
        let cols = if shared.text_preview() {
            PICTURE_COL.saturating_sub(2)
        } else {
            size.cols
        };
        Body {
            list,
            keep,
            tail: tail(&sentence, cols),
        }
    }

    /// `n` among the looks: a new look, with No filter and every other key
    /// left to vtium.conf, goes into the draft, and its card opens with its
    /// name being typed. A look with no `vtlook` line draws the vtlook of
    /// vtium.conf's top level, so where the top level sets one other than
    /// the built-in one, the new look names the built-in one.
    ///
    /// References:
    /// - ADR-1304 D4, its note of 2026-11-19: a new look starts on No
    ///   filter.
    /// - ADR-1304 D5: the look's vtlook wins, then the top level's.
    fn new_look(&mut self, shared: &Shared) -> Vec<Command> {
        let name = list::new_name(shared);
        let default = vtium_settings::vtlook::DEFAULT;
        let change = DraftChange::LookAdd {
            name: name.clone(),
            frit: None,
            size: None,
            vtlook: shared
                .drafted("vtlook")
                .is_some_and(|v| v != default)
                .then(|| default.to_owned()),
        };
        if let Err(why) = shared.apply(std::slice::from_ref(&change)) {
            return vec![Command::Change(Change::Say(why))];
        }
        self.open = Some(Open::Card(card::Card::typing(&name)));
        vec![
            Command::Change(Change::Make {
                changes: vec![change],
                select: None,
            }),
            Command::Change(Change::Say(words::look_added(&name))),
        ]
    }

    /// `t`: tries the font set `font` on the console, or says why it
    /// cannot: there is none, the console does not read it, or this
    /// machine does not have it. The live trial switches the console's
    /// font alone, so a look's size, color scheme and vtlook take effect
    /// only once vtium starts with them.
    fn try_font(shared: &Shared, font: Option<String>) -> Vec<Command> {
        let say = |line: String| vec![Command::Change(Change::Say(line))];
        let Some(name) = font else {
            return say(words::TRY_NO_FONT.to_owned());
        };
        match shared.inputs.fonts.iter().find(|f| f.name == name) {
            Some(f) if fontsets::usable(shared, f) => vec![Command::Change(Change::Try(name))],
            Some(Font {
                state: FontState::Refused(why),
                ..
            }) => say(words::try_refused(&name, why)),
            Some(_) => say(words::try_refused(&name, words::KEPT)),
            None => say(words::try_absent(&name)),
        }
    }

    /// A key while nothing is open over the list.
    fn list_key(&mut self, shared: &Shared, key: Key) -> Vec<Command> {
        let entry = self.highlighted(shared);
        match (key, entry) {
            (Key::Enter, Some(Entry::Look(name))) => {
                self.open = Some(Open::Card(card::Card::new(&name)));
                Vec::new()
            }
            (Key::Enter, Some(Entry::FontSet(at))) => {
                self.open = Some(Open::FontSet { at, top: 0 });
                Vec::new()
            }
            (Key::Enter, Some(Entry::Palette(scheme))) => {
                self.open = Some(Open::Palette { scheme, top: 0 });
                Vec::new()
            }
            (Key::Enter, Some(Entry::Pointer(at))) => {
                self.open = Some(Open::Pointer { at, top: 0 });
                Vec::new()
            }
            (Key::Char(' '), Some(e @ (Entry::Palette(_) | Entry::Pointer(_)))) => {
                Self::resource(shared, &e)
                    .map(|(r, uses)| resources::space(&r, &uses))
                    .unwrap_or_default()
            }
            // the keys that act on a look or a font set do nothing among
            // the color schemes and the pointer themes
            (Key::Char('n' | 'r' | 't' | 'p'), Some(Entry::Palette(_) | Entry::Pointer(_))) => {
                Vec::new()
            }
            (Key::Char('n'), Some(Entry::Look(_) | Entry::NoLooks)) => self.new_look(shared),
            (Key::Char('n'), Some(Entry::FontSet(_))) => {
                vec![Command::Request(Request::Go(View::New))]
            }
            (Key::Char('r'), Some(Entry::Look(name))) => {
                let heads = list::heads(shared, &name);
                vec![Command::Request(Request::Ask {
                    question: words::look_delete_question(&name, &heads),
                    then: Then::Change(DraftChange::LookRm(name)),
                })]
            }
            (Key::Char('r'), Some(Entry::FontSet(at))) => shared
                .inputs
                .fonts
                .get(at)
                .map(|f| fontsets::remove(shared, &f.name))
                .unwrap_or_default(),
            (Key::Char('t'), Some(Entry::Look(_))) => Self::try_font(shared, self.font_set(shared)),
            (Key::Char('t'), Some(Entry::NoLooks)) => {
                vec![Command::Change(Change::Say(words::TRY_NO_LOOK.to_owned()))]
            }
            (Key::Char('t'), Some(Entry::FontSet(at))) => {
                Self::try_font(shared, shared.inputs.fonts.get(at).map(|f| f.name.clone()))
            }
            (Key::Char('p'), _) => {
                self.picture = !self.picture;
                Vec::new()
            }
            _ => {
                let n = Self::entries(shared).len();
                self.list.at = step(self.list.at.min(n.saturating_sub(1)), n, key);
                Vec::new()
            }
        }
    }
}

impl Tab for Looks {
    fn name(&self) -> &'static str {
        words::LOOKS
    }

    fn node(&self) -> &'static str {
        match &self.open {
            None => "looks",
            Some(Open::Card(c)) => c.node(),
            Some(Open::FontSet { .. }) => "looks.font-set",
            Some(Open::Palette { .. }) => "looks.palette",
            Some(Open::Pointer { .. }) => "looks.pointer",
        }
    }

    fn body(&self, shared: &Shared, size: Size) -> Body {
        match &self.open {
            None => self.list_body(shared, size),
            Some(Open::Card(c)) => c.body(shared, size),
            Some(Open::FontSet { at, top }) => {
                detail_body(&fontsets::detail(shared, *at), Vec::new(), *top, size)
            }
            Some(Open::Palette { scheme, top }) => {
                let (lines, colors) = palettes::detail(shared, *scheme);
                detail_body(&lines, colors, *top, size)
            }
            Some(Open::Pointer { at, top }) => {
                detail_body(&pointers::detail(shared, *at), Vec::new(), *top, size)
            }
        }
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

    /// Beside the list of names, two rows below the looks group's
    /// heading. The preview's area keeps the size it had beside the list of
    /// sizes this tab held before it held looks, so a picture drawn for it
    /// keeps its size too.
    fn picture_at(&self, _shared: &Shared, _size: Size) -> (usize, usize) {
        (PICTURE_ROW.saturating_add(2), PICTURE_COL)
    }

    /// The previewed font set: sample characters at the look's size, or a
    /// picture of the console in it. A card or a detail fills the screen
    /// with its lines, so the preview waits for the list.
    fn preview<'a>(&self, shared: &'a Shared, size: Size) -> Preview<'a> {
        if self.open.is_some()
            || matches!(
                self.highlighted(shared),
                Some(Entry::Palette(_) | Entry::Pointer(_))
            )
        {
            return Preview::nothing(shared, size);
        }
        let font = self
            .font_set(shared)
            .and_then(|name| shared.inputs.fonts.iter().find(|f| f.name == name));
        let source = if self.picture {
            Source::Font {
                font,
                detail: false,
            }
        } else {
            Source::Size {
                font,
                points: self.points(shared),
            }
        };
        Preview::of(shared, size, source)
    }

    /// In a vtium console, the sample screen's lines, or with `p` the
    /// detailed specimen's lines for the locale; none while a card or a
    /// detail is open.
    fn console_lines(&self, shared: &Shared) -> Vec<Vec<(&'static str, Style)>> {
        if self.open.is_some()
            || matches!(
                self.highlighted(shared),
                Some(Entry::Palette(_) | Entry::Pointer(_))
            )
        {
            return Vec::new();
        }
        if !self.picture {
            return super::super::shot::sample_rows();
        }
        crate::specimen::detail_lines(&shared.inputs.locale)
            .into_iter()
            .map(|line| vec![(line, Style::PLAIN)])
            .collect()
    }

    fn typing(&self) -> bool {
        self.card().is_some_and(card::Card::is_typing)
    }

    fn type_key(&mut self, key: Key) {
        if let Some(Open::Card(c)) = &mut self.open {
            c.type_key(key);
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
        match self.open.take() {
            None => self.list_key(shared, key),
            Some(Open::Card(mut c)) => {
                let font = c.font_set(shared);
                if key == Key::Char('t') && !c.is_typing() && c.node() == "looks.look" {
                    self.open = Some(Open::Card(c));
                    return Self::try_font(shared, font);
                }
                match c.update(shared, key) {
                    card::Step::Stay(commands) => {
                        self.open = Some(Open::Card(c));
                        commands
                    }
                    card::Step::Close => {
                        // the highlight goes to the look the card showed,
                        // which a rename may have moved in the list
                        if let Some(at) = Self::entries(shared)
                            .iter()
                            .position(|e| *e == Entry::Look(c.name().to_owned()))
                        {
                            self.list.at = at;
                        }
                        Vec::new()
                    }
                }
            }
            Some(open) => {
                // the last line under the title the detail can scroll to
                let (last, top) = match &open {
                    Open::FontSet { at, top } => (fontsets::detail(shared, *at).len(), *top),
                    Open::Palette { scheme, top } => {
                        let (lines, colors) = palettes::detail(shared, *scheme);
                        (lines.len().saturating_add(colors.len()), *top)
                    }
                    Open::Pointer { at, top } => (pointers::detail(shared, *at).len(), *top),
                    Open::Card(_) => (0, 0),
                };
                let last = last.saturating_sub(2);
                let top = match key {
                    Key::Escape => return Vec::new(),
                    Key::Up => top.saturating_sub(1),
                    Key::Down => top.saturating_add(1).min(last),
                    _ => top,
                };
                self.open = Some(match open {
                    Open::FontSet { at, .. } => Open::FontSet { at, top },
                    Open::Palette { scheme, .. } => Open::Palette { scheme, top },
                    Open::Pointer { at, .. } => Open::Pointer { at, top },
                    card @ Open::Card(_) => card,
                });
                Vec::new()
            }
        }
    }
}

/// The body of a detail of `lines`, one paragraph each, with `extra` rows
/// after them, scrolled `top` rows down. The title stays at the top, so
/// the detail always says what it describes, and the rest scrolls.
fn detail_body(
    lines: &[String],
    extra: Vec<super::super::canvas::Row>,
    top: usize,
    size: Size,
) -> Body {
    let mut rest: Vec<super::super::canvas::Row> = Vec::new();
    for line in lines.iter().skip(1) {
        rest.extend(prose(line, size.cols));
    }
    rest.extend(extra);
    let mut list = Vec::new();
    for line in lines.iter().take(1) {
        list.extend(prose(line, size.cols));
    }
    list.extend(rest.into_iter().skip(top));
    if list.is_empty() {
        list.push(plain(""));
    }
    Body {
        list,
        keep: 0,
        tail: tail(words::FONT_SET_HINT, size.cols),
    }
}
