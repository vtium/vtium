//! The Looks tab: one list in four groups, the looks of the draft, the
//! font sets, the color schemes and the installed pointer themes, with a
//! preview of the highlighted look, font set or color scheme. Enter on a
//! look opens its card, and Enter on any other item opens its detail. The
//! card's list of sizes shows the preview of the highlighted size below
//! it. `n` makes a new look among the looks and opens the form for a new
//! font set among the font sets. `r` deletes a look after a question and marks a
//! font set for removal at the next install. `t` tries the look's font set
//! or the font set on the console, and `p` switches the preview between
//! sample characters and a picture of the console. Space activates or
//! deactivates a color scheme file or a pointer theme, with the controls
//! the resource tabs share, and asks first when a look uses it. Among the
//! color schemes and the pointer themes, `n`, `r`, `t` and `p` do nothing,
//! so the tab is in a node of its own there, `looks-resources`, whose key
//! line leaves them out and offers Space. Space on a scheme built into
//! vtium says that the scheme is always active.
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
//! - ADR-1403 D7, its note of 2026-11-18: each key line names only keys
//!   that act where it is shown.

pub mod card;
pub mod fontsets;
pub mod list;
pub mod newfrit;
pub mod palettes;
pub mod pointers;

use super::super::canvas::Row;
use super::super::color::Style;
use super::super::edit::Change as DraftChange;
use super::super::input::Key;
use super::super::layout::{MESSAGE_ROWS, PICTURE_COL, PICTURE_ROW};
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
    /// A font set, by its place among the font sets the draft sees
    /// ([`fontsets::all`]).
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

/// Where the Looks tab's highlight and an open font set's detail are, with
/// each font set by its name, which [`Looks::spot`] takes before the list
/// of font sets changes and [`Looks::restore`] puts back after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spot {
    entry: Option<Entry>,
    font_set: Option<String>,
    open: Option<String>,
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
    /// The highlight is on a color scheme or a pointer theme, which puts
    /// the list in the node `looks-resources`. [`Looks::follow`] sets it
    /// each time the highlight or the list can have moved.
    resources: bool,
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
        out.extend((0..fontsets::all(shared).len()).map(Entry::FontSet));
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

    /// Highlights the font set `name` in the list of font sets, with
    /// nothing open over the list, as Enter on a font set in the Flow tab
    /// asks. A name the list does not hold leaves the highlight as it was.
    pub fn show_font_set(&mut self, shared: &Shared, name: &str) {
        let Some(font) = fontsets::place(shared, name) else {
            return;
        };
        if let Some(at) = Self::entries(shared)
            .iter()
            .position(|e| *e == Entry::FontSet(font))
        {
            self.open = None;
            self.list.at = at;
            self.follow(shared);
        }
    }

    /// Records whether the highlight is on a color scheme or a pointer
    /// theme, which decides the list's node. The tab's node is read
    /// without the shared state, so the tab records the highlight's kind
    /// after each key it takes and each change of the list it hears of: a
    /// key, a font set shown from the Flow tab, and an install that adds
    /// or removes a font set. A look deleted after its question leaves the
    /// highlight among the looks and the font sets, because the list
    /// always holds the built-in font set between the looks and the
    /// color schemes.
    pub fn follow(&mut self, shared: &Shared) {
        self.resources = matches!(
            self.highlighted(shared),
            Some(Entry::Palette(_) | Entry::Pointer(_))
        );
    }

    /// Where the highlight and an open font set's detail are. The list
    /// holds each font set by its place among the font sets, which an
    /// install that adds or removes a font set moves, so the spot names
    /// the font set instead.
    pub fn spot(&self, shared: &Shared) -> Spot {
        let name = |at: usize| fontsets::font(shared, at).map(|f| f.name);
        let entry = self.highlighted(shared);
        Spot {
            font_set: match entry {
                Some(Entry::FontSet(at)) => name(at),
                _ => None,
            },
            entry,
            open: match self.open {
                Some(Open::FontSet { at, .. }) => name(at),
                _ => None,
            },
        }
    }

    /// Puts the highlight back on the item of `spot`, and an open font
    /// set's detail back on its font set, once the list has changed. A
    /// font set that left the list closes its detail, and a highlight
    /// whose item left the list stays at its place.
    pub fn restore(&mut self, shared: &Shared, spot: Spot) {
        let place = |name: &str| fontsets::place(shared, name);
        let entry = match (spot.entry, spot.font_set) {
            (Some(Entry::FontSet(_)), Some(name)) => place(&name).map(Entry::FontSet),
            (entry, _) => entry,
        };
        if let Some(at) = entry.and_then(|e| Self::entries(shared).iter().position(|x| *x == e)) {
            self.list.at = at;
        }
        if let Some(Open::FontSet { at, .. }) = &mut self.open {
            match spot.open.as_deref().and_then(place) {
                Some(now) => *at = now,
                None => self.open = None,
            }
        }
        self.follow(shared);
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
            return fontsets::font(shared, at).map(|f| f.name);
        }
        if let Some(name) = self.look_shown(shared) {
            return list::look(shared, &name)
                .and_then(|l| l.frit)
                .or_else(|| shared.drafted("font-name"));
        }
        match self.highlighted(shared) {
            Some(Entry::FontSet(at)) => fontsets::font(shared, at).map(|f| f.name),
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
        for words in preview_words(shared, &preview, self.points(shared), !self.picture) {
            sentence.push(' ');
            sentence.push_str(&words);
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

    /// A look card's size list with the preview of the highlighted size
    /// under it, or `None` where the size list is not open or the screen
    /// leaves the preview no row. The list takes the rows it needs, up to
    /// half the body's rows, and scrolls in that window to keep the
    /// highlight in view. Under it come the sentence that says what the
    /// keys do and the line that describes the preview, then a blank row,
    /// then the rows of a picture or of the console's own text, from the
    /// left edge. A written description is the whole preview, so it needs
    /// no rows of its own. Where the sentences are long, the list's window
    /// gives up rows to them, down to [`SIZE_LIST_MIN`] rows, so a long
    /// reason why a size cannot be used still shows.
    ///
    /// References:
    /// - ADR-1201 D4, "Sizes, live on a vtium console": the preview
    ///   follows the highlight over the sizes, and its note of 2026-11-21
    ///   puts it below the card's size list.
    fn size_view(&self, shared: &Shared, size: Size) -> Option<SizeView> {
        let (list, keep, points) = self.card()?.size_list(shared, size.cols)?;
        let room = body_rows(size);
        let mut sentence = words::CARD_LIST_HINT.to_owned();
        // in a vtium console the preview is the console's own text, at the
        // size the console draws now, so the caption says what Enter does
        // to that size; elsewhere the caption describes the preview
        let parts = if shared.text_preview() {
            vec![
                if shared.window_ops() {
                    words::CARD_CONSOLE_SIZE_TRY
                } else {
                    words::CARD_CONSOLE_SIZE_FIXED
                }
                .to_owned(),
            ]
        } else {
            preview_words(
                shared,
                &self.size_preview(shared, size, points),
                points,
                true,
            )
        };
        for part in parts {
            sentence.push(' ');
            sentence.push_str(&part);
        }
        let tail = tail(&sentence, size.cols);
        // a picture or the console's text needs a blank row and one row of
        // its own below the sentences; a written description needs none
        let drawn = shared.text_preview() || shared.graphics;
        let below = tail.len().saturating_add(usize::from(drawn));
        let fits = room.saturating_sub(below).saturating_sub(1);
        let window = list.len().min(room / 2).min(fits);
        if window < list.len().min(SIZE_LIST_MIN) {
            return None;
        }
        let first = keep.saturating_add(1).saturating_sub(window);
        let list: Vec<Row> = list.into_iter().skip(first).take(window).collect();
        let keep = keep.saturating_sub(first);
        let used = list.len().saturating_add(tail.len());
        Some(SizeView {
            body: Body { list, keep, tail },
            at: (PICTURE_ROW.saturating_add(used).saturating_add(1), 2),
            points,
        })
    }

    /// The preview of the look card's font set at `points`, on the
    /// sample screen. The font helper draws only installed font sets, so
    /// a font set that only the draft adds has no picture until the
    /// install.
    fn size_preview<'a>(&self, shared: &'a Shared, size: Size, points: u32) -> Preview<'a> {
        let font = self
            .font_set(shared)
            .and_then(|name| shared.inputs.fonts.iter().find(|f| f.name == name));
        Preview::of(shared, size, Source::Size { font, points })
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
    /// cannot: there is none, the console does not read it, only the
    /// draft adds it, or this machine does not have it. The console reads
    /// font sets only from the frits' directory, so a font set the draft
    /// adds can be tried once it is installed. The live trial switches the
    /// console's font alone, so a look's size, color scheme and vtlook
    /// take effect only once vtium starts with them.
    ///
    /// References:
    /// - ADR-1403 D8, "The live trial": `t` on a frit only the draft holds says it can be tried once installed.
    fn try_font(shared: &Shared, font: Option<String>) -> Vec<Command> {
        let say = |line: String| vec![Command::Change(Change::Say(line))];
        let Some(name) = font else {
            return say(words::TRY_NO_FONT.to_owned());
        };
        if fontsets::only_drafted(shared, &name) {
            return say(words::try_in_draft(&name));
        }
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

    /// `w`: asks whether to let programs resize the console's grid, and
    /// on `y` writes `allow-window-ops = on` into the draft. Where the
    /// draft already turns it on, the question says so, and `y` leaves the
    /// draft as it is.
    ///
    /// References:
    /// - ADR-1201 D4, "Sizes, live on a vtium console": the notice line
    ///   shows `w`, which asks `Let programs resize the grid?`, and only
    ///   `y` writes the key.
    fn window_ops(shared: &Shared) -> Vec<Command> {
        let question = if shared.window_ops_drafted() {
            format!("{} {}", words::WINDOW_OPS_ON, words::WINDOW_OPS_QUESTION)
        } else {
            words::WINDOW_OPS_QUESTION.to_owned()
        };
        vec![Command::Request(Request::Ask {
            question,
            then: Then::Change(DraftChange::Top {
                key: vtium_settings::keys::ALLOW_WINDOW_OPS.name,
                value: Some("on".to_owned()),
            }),
        })]
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
            (Key::Char(' '), Some(Entry::Palette(palettes::Scheme::BuiltIn(_)))) => {
                vec![Command::Change(Change::Say(
                    words::BUILT_IN_SCHEME_SPACE.to_owned(),
                ))]
            }
            (Key::Char(' '), Some(e @ (Entry::Palette(_) | Entry::Pointer(_)))) => {
                Self::resource(shared, &e)
                    .map(|(r, uses)| resources::space(&r, &uses))
                    .unwrap_or_default()
            }
            (Key::Enter | Key::Char('r'), Some(Entry::NoLooks)) => {
                vec![Command::Change(Change::Say(words::NO_LOOK_YET.to_owned()))]
            }
            // the keys that act on a look or a font set do nothing among
            // the color schemes and the pointer themes
            (Key::Char('n' | 'r' | 't' | 'p'), Some(Entry::Palette(_) | Entry::Pointer(_))) => {
                Vec::new()
            }
            (Key::Char('w'), Some(Entry::Look(_) | Entry::NoLooks | Entry::FontSet(_))) => {
                Self::window_ops(shared)
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
            (Key::Char('r'), Some(Entry::FontSet(at))) => fontsets::font(shared, at)
                .map(|f| fontsets::remove(shared, &f.name))
                .unwrap_or_default(),
            (Key::Char('t'), Some(Entry::Look(_))) => Self::try_font(shared, self.font_set(shared)),
            (Key::Char('t'), Some(Entry::NoLooks)) => {
                vec![Command::Change(Change::Say(words::TRY_NO_LOOK.to_owned()))]
            }
            (Key::Char('t'), Some(Entry::FontSet(at))) => {
                Self::try_font(shared, fontsets::font(shared, at).map(|f| f.name))
            }
            (Key::Char('p'), entry) => {
                self.picture = !self.picture;
                // a font set vtium cannot read has no preview, so the
                // switch would show nothing new; the tab says so
                match entry {
                    Some(Entry::FontSet(at))
                        if fontsets::font(shared, at)
                            .is_some_and(|f| !fontsets::usable(shared, &f)) =>
                    {
                        vec![Command::Change(Change::Say(
                            words::NO_PREVIEW_TO_SWITCH.to_owned(),
                        ))]
                    }
                    _ => Vec::new(),
                }
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

    /// The list's node is `looks` on a look or a font set, and
    /// `looks-resources` on a color scheme or a pointer theme, where `n`,
    /// `r`, `t` and `p` do nothing and Space acts. Both are the tab with
    /// nothing open over it, so the node's id has no dot.
    fn node(&self) -> &'static str {
        match &self.open {
            None if self.resources => "looks-resources",
            None => "looks",
            Some(Open::Card(c)) => c.node(),
            Some(Open::FontSet { .. }) => "looks.font-set",
            Some(Open::Palette { .. }) => "looks.palette",
            Some(Open::Pointer { .. }) => "looks.pointer",
        }
    }

    fn body(&self, shared: &Shared, size: Size) -> Body {
        if let Some(view) = self.size_view(shared, size) {
            return view.body;
        }
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
    /// keeps its size too. Under a look card's size list, below the list
    /// and its sentences, from the left edge.
    fn picture_at(&self, shared: &Shared, size: Size) -> (usize, usize) {
        match self.size_view(shared, size) {
            Some(view) => view.at,
            None => (PICTURE_ROW.saturating_add(2), PICTURE_COL),
        }
    }

    /// The previewed font set: sample characters at the look's size, or a
    /// picture of the console in it. Under a look card's size list, the
    /// sample characters at the highlighted size. Any other card or detail
    /// fills the screen with its lines, so the preview waits for the list.
    fn preview<'a>(&self, shared: &'a Shared, size: Size) -> Preview<'a> {
        if let Some(view) = self.size_view(shared, size) {
            return self.size_preview(shared, size, view.points);
        }
        if self.open.is_some()
            || matches!(
                self.highlighted(shared),
                Some(Entry::Palette(_) | Entry::Pointer(_))
            )
        {
            return Preview::nothing(shared, size);
        }
        // the font helper draws only installed font sets, as for a size
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
    /// detailed specimen's lines for the locale. Under a look card's size
    /// list, the sample screen's lines where the screen leaves the preview
    /// a row, as [`Looks::size_view`] lays it out; none while another card
    /// or a detail is open, or where the size list leaves no row.
    fn console_lines(&self, shared: &Shared, size: Size) -> Vec<Vec<(&'static str, Style)>> {
        if self
            .card()
            .is_some_and(|c| c.size_list(shared, 0).is_some())
        {
            return match self.size_view(shared, size) {
                Some(_) => super::super::shot::sample_rows(),
                None => Vec::new(),
            };
        }
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

    /// The action keys act while nothing is open over the list.
    ///
    /// References:
    /// - ADR-1403 D7: the action keys do nothing while a layer is open.
    fn takes_action(&self, _action: char) -> bool {
        self.open.is_none()
    }

    fn update(&mut self, shared: &Shared, room: Room, key: Key) -> Vec<Command> {
        let commands = self.update_open(shared, room, key);
        self.follow(shared);
        commands
    }
}

impl Looks {
    /// A key, to the list or to what is open over it.
    fn update_open(&mut self, shared: &Shared, room: Room, key: Key) -> Vec<Command> {
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
                // the last row under the title the detail can scroll to,
                // counted as the screen wraps the detail's paragraphs
                let cols = room.size.cols;
                let (rows, top) = match &open {
                    Open::FontSet { at, top } => (
                        detail_rest(&fontsets::detail(shared, *at), Vec::new(), cols).len(),
                        *top,
                    ),
                    Open::Palette { scheme, top } => {
                        let (lines, colors) = palettes::detail(shared, *scheme);
                        (detail_rest(&lines, colors, cols).len(), *top)
                    }
                    Open::Pointer { at, top } => (
                        detail_rest(&pointers::detail(shared, *at), Vec::new(), cols).len(),
                        *top,
                    ),
                    Open::Card(_) => (0, 0),
                };
                let last = rows.saturating_sub(1);
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

/// The fewest rows a look card's size list keeps when the sentences under
/// it are long. With fewer rows than this, the card shows its whole list
/// and no preview.
const SIZE_LIST_MIN: usize = 3;

/// A look card's size list laid out with the preview under it: the body,
/// where the picture or the console's own text goes, row and column from
/// 1, and the highlighted size in points.
struct SizeView {
    body: Body,
    at: (usize, usize),
    points: u32,
}

/// The rows a tab's body has on a screen of `size` when the message is at
/// its longest: the screen less the tab bar, the status line, the two
/// rules, a key line of one row and the message's rows.
fn body_rows(size: Size) -> usize {
    size.rows.saturating_sub(5).saturating_sub(MESSAGE_ROWS)
}

/// What a preview at `points` adds to the sentences under a list. Where
/// the preview is a picture of a size, which `sized` says, the density it
/// is drawn at comes first, or that automatic has no preview, since vtium
/// picks the size for each display. Then comes the line that describes
/// the preview.
fn preview_words(shared: &Shared, preview: &Preview<'_>, points: u32, sized: bool) -> Vec<String> {
    let mut out = Vec::new();
    if sized && shared.graphics && !shared.text_preview() {
        if points == 0 {
            out.push(words::AUTOMATIC_PICTURE.to_owned());
        } else {
            let (dpi, from) = shared.inputs.dpi;
            out.push(words::density(
                dpi,
                match from {
                    DpiFrom::Conf => words::DPI_FROM_CONF,
                    DpiFrom::Edid => words::DPI_FROM_EDID,
                    DpiFrom::Assumed => words::DPI_ASSUMED,
                },
            ));
        }
    }
    out.extend(preview.preview_line());
    out
}

/// The rows of a detail of `lines` under its title, each paragraph
/// wrapped to `cols`, then the `extra` rows. These are the rows the
/// detail scrolls through.
fn detail_rest(
    lines: &[String],
    extra: Vec<super::super::canvas::Row>,
    cols: usize,
) -> Vec<super::super::canvas::Row> {
    let mut rest: Vec<super::super::canvas::Row> = Vec::new();
    for line in lines.iter().skip(1) {
        rest.extend(prose(line, cols));
    }
    rest.extend(extra);
    rest
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
    let rest = detail_rest(lines, extra, size.cols);
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
