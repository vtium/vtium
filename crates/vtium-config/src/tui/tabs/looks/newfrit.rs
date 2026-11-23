//! The form that makes a new font set (a frit) by role. It opens over the
//! Looks tab's font sets, on `n`, and its rows follow the roles a font set
//! fills: the text font, box drawing, the scripts, the symbols and icons,
//! the emoji and the fallback. The font set's name comes after them, and
//! the last row adds the font set to the install, once the frit reader
//! takes it, and goes back to the Looks tab.
//!
//! Enter on a role shows, in the shared list, what can fill the role: the
//! active fonts whose faces the font helper described as fitting the role,
//! and for the scripts, symbols, emoji and fallback the layers in the
//! frits' `layers/` folder of that role. A font that is not active, or that
//! the helper has not described, is not offered. Other… at the end of a
//! list lets the person type a font that the list lacks. Box drawing offers
//! where its lines come from, which the font set's `glyph-source-box=` line
//! chooses. The text font brings its bold and italic faces with it: each
//! active file of the same family whose face is bold, italic or both is
//! written after it with its `style=` line.
//!
//! The form writes the font set's lines in the order of the roles, so the
//! fonts and the layers come in the order vtium tries them. A font set holds
//! one symbols, one emoji and one fallback source at most, and any number of
//! scripts, one for each writing system the set adds.
//!
//! References:
//! - ADR-1201 D9: `new`'s guided flow, a frit made from fonts and
//!   installed like any change.
//! - ADR-1201 D3, "A new font set": the layers offered as choices, written
//!   in the order of their roles, and typing kept for a font that the list
//!   lacks.
//! - ADR-1205 D6: the roles and their order, box drawing apart from it.
//! - ADR-1205 D4: only active fonts are offered.
//! - ADR-1204 D5: the layers' folder, and `new-frit --layer`.
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.

use super::super::super::desk::{FontSetEdit, Op};
use super::super::super::fonts_found::{source_of, squashed};
use super::super::super::input::Key;
use super::super::super::listbox::{Act, Body, Item, ListBox, Marks, columns, rows, step, tail};
use super::super::super::model::{Effect, FontState, Shared, Size, View};
use super::super::super::words;
use super::super::{Change, Command, Request, Room, Tab};
use crate::describe::{Face, Group};
use vtium_font::set::layer::Role;

/// The form's fields, in order: a row for each role, then the name. The
/// row after them adds the frit to the install.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Text,
    Box,
    Scripts,
    Symbols,
    Emoji,
    Fallback,
    Name,
}

impl Field {
    pub const ALL: [Field; 7] = [
        Field::Text,
        Field::Box,
        Field::Scripts,
        Field::Symbols,
        Field::Emoji,
        Field::Fallback,
        Field::Name,
    ];

    /// The layer role the field takes layers of, for a field that does.
    fn role(self) -> Option<Role> {
        match self {
            Field::Scripts => Some(Role::Script),
            Field::Symbols => Some(Role::Symbols),
            Field::Emoji => Some(Role::Emoji),
            Field::Fallback => Some(Role::Fallback),
            Field::Text | Field::Box | Field::Name => None,
        }
    }
}

/// The new-frit form's fields, counted: the rows before the add row.
pub const NEW_FIELDS: usize = Field::ALL.len();

/// One font or layer picked for a role: how the form shows it, and the
/// lines it writes into the font set, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub shown: String,
    pub lines: Vec<String>,
}

impl Pick {
    /// A font named by its source spelling, on a `source=` line of its own.
    fn source(shown: impl Into<String>, spelling: &str) -> Pick {
        Pick {
            shown: shown.into(),
            lines: vec![format!("source={spelling}")],
        }
    }

    /// The layer `name`, on its `layer=` line.
    fn layer(name: &str) -> Pick {
        Pick {
            shown: words::new_layer_shown(name),
            lines: vec![format!("layer={name}")],
        }
    }
}

/// The new frit being made: a pick for each role the person filled, where
/// box drawing comes from, and the name typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewFrit {
    pub text: Option<Pick>,
    /// The value of the `glyph-source-box=` line, `None` for no line.
    pub boxes: Option<&'static str>,
    pub scripts: Vec<Pick>,
    pub symbols: Option<Pick>,
    pub emoji: Option<Pick>,
    pub fallback: Option<Pick>,
    pub name: String,
    /// The highlighted row: a field, or [`NEW_FIELDS`] for the add row.
    pub at: usize,
}

impl NewFrit {
    /// The frit's name: the name typed, else one made from the text font.
    pub fn name(&self) -> String {
        if self.name.trim().is_empty() {
            self.text
                .as_ref()
                .map(|t| crate::commands::name_from(&t.shown))
                .unwrap_or_default()
        } else {
            self.name.trim().to_owned()
        }
    }

    /// The frit's text, from its picks, in the order of the roles.
    pub fn text(&self) -> String {
        let name = self.name();
        let mut text = format!("# {name}: made by vtium-config new-frit\nname={name}\n");
        let picks = self
            .text
            .iter()
            .chain(&self.scripts)
            .chain(&self.symbols)
            .chain(&self.emoji)
            .chain(&self.fallback);
        for line in picks.flat_map(|p| &p.lines) {
            text.push_str(line);
            text.push('\n');
        }
        if let Some(value) = self.boxes {
            text.push_str(&format!("glyph-source-box={value}\n"));
        }
        text
    }

    /// What the field `field` holds, as the form's row shows it.
    fn shown(&self, field: Field) -> String {
        let one = |p: &Option<Pick>| {
            p.as_ref()
                .map_or_else(|| words::NEW_NONE_CHOSEN.to_owned(), |p| p.shown.clone())
        };
        match field {
            Field::Text => one(&self.text),
            Field::Box => box_shown(self.boxes).to_owned(),
            Field::Scripts if self.scripts.is_empty() => words::NEW_NONE_CHOSEN.to_owned(),
            Field::Scripts => self
                .scripts
                .iter()
                .map(|p| p.shown.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            Field::Symbols => one(&self.symbols),
            Field::Emoji => one(&self.emoji),
            Field::Fallback => one(&self.fallback),
            Field::Name if self.name.trim().is_empty() => words::new_name_from(&self.name()),
            Field::Name => self.name.clone(),
        }
    }

    /// The pick the single-pick field `field` holds.
    fn slot(&mut self, field: Field) -> Option<&mut Option<Pick>> {
        match field {
            Field::Text => Some(&mut self.text),
            Field::Symbols => Some(&mut self.symbols),
            Field::Emoji => Some(&mut self.emoji),
            Field::Fallback => Some(&mut self.fallback),
            Field::Box | Field::Scripts | Field::Name => None,
        }
    }

    /// Puts `pick` into `field`: in place of the field's pick, or, for the
    /// scripts, beside the others, and out again when it is there already.
    fn put(&mut self, field: Field, pick: Option<Pick>) {
        if field == Field::Scripts {
            match pick {
                None => self.scripts.clear(),
                Some(p) => match self.scripts.iter().position(|s| s.lines == p.lines) {
                    Some(at) => {
                        self.scripts.remove(at);
                    }
                    None => self.scripts.push(p),
                },
            }
        } else if let Some(slot) = self.slot(field) {
            *slot = pick;
        }
    }

    /// Whether the field `field` holds `pick`, or holds nothing when
    /// `pick` is `None`.
    fn holds(&self, field: Field, pick: Option<&Pick>) -> bool {
        let same = |p: &Option<Pick>| p.as_ref().map(|p| &p.lines) == pick.map(|p| &p.lines);
        match field {
            Field::Text => same(&self.text),
            Field::Symbols => same(&self.symbols),
            Field::Emoji => same(&self.emoji),
            Field::Fallback => same(&self.fallback),
            Field::Scripts => match pick {
                None => self.scripts.is_empty(),
                Some(p) => self.scripts.iter().any(|s| s.lines == p.lines),
            },
            Field::Box | Field::Name => false,
        }
    }
}

/// How the form shows where box drawing comes from.
fn box_shown(value: Option<&str>) -> &'static str {
    match value {
        Some("sprites") => words::ROLE_BOX_OWN,
        Some("font") => words::ROLE_BOX_FONT,
        Some("auto") => words::ROLE_BOX_AUTO,
        _ => words::ROLE_BOX_DEFAULT,
    }
}

/// What a choice of a role's list does when it is picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Leaves the role empty, or for the scripts empties it.
    Leave,
    /// Fills the role with a font or a layer.
    Pick(Pick),
    /// Sets where box drawing comes from, `None` for no line.
    Box(Option<&'static str>),
    /// Lets the person type a font that the list lacks.
    Other,
}

/// One choice of a role's list: what the list shows, a note beside it, and
/// what picking it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    pub note: String,
    pub value: Value,
}

/// Whether a face's style names bold, and whether it names italic.
fn styled(face: &Face) -> (bool, bool) {
    let style = face.style.to_ascii_lowercase();
    (
        style.contains("bold"),
        style.contains("italic") || style.contains("oblique"),
    )
}

/// A face as a list shows it: its family, and its style unless that is
/// the regular one.
fn face_name(face: &Face) -> String {
    let style = face.style.trim();
    if style.is_empty() || style.eq_ignore_ascii_case("regular") {
        face.family.clone()
    } else {
        format!("{} {style}", face.family)
    }
}

/// Whether a face can fill `field`'s role: a monospaced upright face for
/// the text, a face that covers a script beyond Latin, Greek and Cyrillic
/// for the scripts, a Nerd Fonts or Powerline face for the symbols, a
/// color emoji face for the emoji, and any face for the fallback.
fn fits(face: &Face, field: Field) -> bool {
    match field {
        Field::Text => face.monospaced && styled(face) == (false, false),
        Field::Scripts => face.scripts.iter().any(|s| !TEXT_SCRIPTS.contains(s)),
        Field::Symbols => face
            .groups
            .iter()
            .any(|g| matches!(g, Group::NerdFonts | Group::Powerline)),
        Field::Emoji => face.groups.contains(&Group::ColorEmoji),
        Field::Fallback => true,
        Field::Box | Field::Name => false,
    }
}

/// The scripts a text font usually covers itself, so a font that covers
/// only these is not offered for the scripts.
const TEXT_SCRIPTS: [&str; 3] = ["Latin", "Greek", "Cyrillic"];

/// The active font files vtium can use, each with the faces the font
/// helper described and the `source=` spelling that names the file.
fn active_fonts(shared: &Shared) -> Vec<(&std::path::Path, &[Face], String)> {
    let fonts = &shared.inputs.font_files;
    fonts
        .files
        .iter()
        .filter(|f| f.resource.active() && f.resource.trust.is_ok() && !f.resource.own)
        .filter_map(|f| {
            let spelling = source_of(f.path())?;
            Some((f.path(), fonts.faces(f.path()), spelling))
        })
        .collect()
}

/// The text font's pick for the file `spelling` names, whose first face is
/// `face`: its `source=` line, then each active file of the same family
/// whose face is bold, italic or both, with its `style=` line.
fn text_pick(shared: &Shared, face: &Face, spelling: &str) -> Pick {
    let mut pick = Pick::source(face_name(face), spelling);
    let family = squashed(&face.family);
    for (_, faces, other) in active_fonts(shared) {
        let Some(f) = faces.first() else { continue };
        let style = match styled(f) {
            (true, true) => "bold-italic",
            (true, false) => "bold",
            (false, true) => "italic",
            (false, false) => continue,
        };
        if squashed(&f.family) == family && other != spelling {
            pick.lines.push(format!("source={other}"));
            pick.lines.push(format!("style={style}"));
        }
    }
    pick
}

/// What can fill `field`'s role, as its list shows it. The roles but the
/// text start with a choice that leaves the role empty. Then come the
/// layers of the role, the active fonts that fit it, and Other…. The text
/// role offers the built-in fonts too, which are always active.
pub fn choices(shared: &Shared, field: Field) -> Vec<Choice> {
    let choice = |name: String, note: String, value| Choice { name, note, value };
    if field == Field::Box {
        return [None, Some("sprites"), Some("font"), Some("auto")]
            .into_iter()
            .map(|v| choice(box_shown(v).to_owned(), String::new(), Value::Box(v)))
            .collect();
    }
    let mut out = Vec::new();
    if field != Field::Text {
        out.push(choice(
            words::NEW_NONE.to_owned(),
            String::new(),
            Value::Leave,
        ));
    }
    if let Some(role) = field.role() {
        out.extend(
            shared
                .inputs
                .layers
                .iter()
                .filter(|(_, r)| *r == role)
                .map(|(name, _)| {
                    choice(
                        name.clone(),
                        words::NEW_LAYER.to_owned(),
                        Value::Pick(Pick::layer(name)),
                    )
                }),
        );
    }
    for (path, faces, spelling) in active_fonts(shared) {
        let Some(face) = faces.iter().find(|f| fits(f, field)) else {
            continue;
        };
        let pick = if field == Field::Text {
            text_pick(shared, face, &spelling)
        } else {
            Pick::source(face_name(face), &spelling)
        };
        let note = if field == Field::Scripts {
            let more: Vec<&str> = face
                .scripts
                .iter()
                .copied()
                .filter(|s| !TEXT_SCRIPTS.contains(s))
                .collect();
            words::new_covers(&more, path)
        } else {
            path.display().to_string()
        };
        out.push(choice(face_name(face), note, Value::Pick(pick)));
    }
    if field == Field::Text {
        out.extend(
            shared
                .inputs
                .fonts
                .iter()
                .filter(|f| f.state == FontState::BuiltIn)
                .map(|f| {
                    choice(
                        f.name.clone(),
                        words::BUILT_IN.to_owned(),
                        Value::Pick(Pick::source(&f.name, &format!("builtin:{}", f.name))),
                    )
                }),
        );
    }
    out.push(choice(
        words::NEW_OTHER.to_owned(),
        String::new(),
        Value::Other,
    ));
    out
}

/// What is open over the form's rows.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Open {
    /// The highlighted role's choices, as the shared list.
    List(ListBox),
    /// The name being typed, or a font typed after Other….
    Typing(String),
}

/// The new-frit form, which opens over the Looks tab's font sets: the new
/// frit being made, what is open over its rows, and the first row of the
/// form that its window shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Form {
    frit: NewFrit,
    open: Option<Open>,
    offset: usize,
}

impl Form {
    /// The new frit being made.
    pub fn frit(&self) -> &NewFrit {
        &self.frit
    }

    /// The highlighted field, `None` on the add row.
    fn field(&self) -> Option<Field> {
        Field::ALL.get(self.frit.at).copied()
    }

    /// The node of the interface tree the form is at: a role's list, a
    /// value being typed, or the form's rows.
    pub fn node(&self) -> &'static str {
        match self.open {
            Some(Open::List(_)) => "looks.new-frit.list",
            Some(Open::Typing(_)) => "looks.new-frit.typing",
            None => "looks.new-frit",
        }
    }

    /// Whether a value is being typed, whose characters go into it.
    pub fn typing(&self) -> bool {
        matches!(self.open, Some(Open::Typing(_)))
    }

    /// The value as it is typed, while one is.
    pub fn typed(&self) -> Option<&str> {
        match &self.open {
            Some(Open::Typing(text)) => Some(text),
            _ => None,
        }
    }

    /// A character or Backspace typed into the value being typed.
    pub fn type_key(&mut self, key: Key) {
        if let Some(Open::Typing(text)) = &mut self.open {
            super::super::type_into(text, key);
        }
    }

    /// The form's body on a screen of `size`: the open role's choices, or
    /// each field with what it holds and then the row that adds the frit,
    /// and under them what a frit is made of.
    pub fn body(&self, shared: &Shared, size: Size) -> Body {
        if let (Some(Open::List(lb)), Some(field)) = (&self.open, self.field()) {
            let items: Vec<Item> = choices(shared, field)
                .into_iter()
                .map(|c| Item {
                    on: match &c.value {
                        Value::Leave => self.frit.holds(field, None),
                        Value::Pick(p) => self.frit.holds(field, Some(p)),
                        Value::Box(v) => self.frit.boxes == *v,
                        Value::Other => false,
                    },
                    ..Item::new(c.name, c.note)
                })
                .collect();
            let (list, keep) = rows(&items, lb.at, Marks::Choice, size.cols);
            return Body {
                list,
                keep,
                tail: tail(words::NEW_LIST_HINT, size.cols),
            };
        }
        let mut items: Vec<(String, String)> = words::NEW_FIELD_NAMES
            .iter()
            .zip(Field::ALL)
            .enumerate()
            .map(|(i, (name, field))| {
                let value = match &self.open {
                    Some(Open::Typing(text)) if i == self.frit.at => format!("{text}_"),
                    _ => self.frit.shown(field),
                };
                ((*name).to_owned(), value)
            })
            .collect();
        items.push((words::NEW_ADD.to_owned(), String::new()));
        let (list, keep) = columns(&items, self.frit.at.min(NEW_FIELDS), 18, size.cols);
        let hint = if self.typing() {
            words::NEW_TYPING_HINT
        } else {
            words::NEW_HINT
        };
        Body {
            list,
            keep,
            tail: tail(hint, size.cols),
        }
    }

    /// A key the navigation module hands to the new-frit form.
    ///
    /// While a value is typed, Enter keeps it and moves to the next row, and
    /// Esc drops it. On the name, the value is the name; on a role, it is a
    /// font typed after Other…, which fills the role. While a role's list is
    /// open, Enter picks and Esc closes it.
    ///
    /// Otherwise Up, Down, Home, End, PgUp and PgDn move between the rows.
    /// Enter on a role opens its list, on the name types it, and on the
    /// last row adds the frit to the install, once the frit reader takes
    /// it, and goes back to the Looks tab. Esc goes back to the Looks tab
    /// with the fields kept, and the form goes back to its first row, so
    /// `n` opens it there again.
    pub fn update(&mut self, shared: &Shared, key: Key) -> Vec<Command> {
        let field = self.field();
        match (self.open.take(), field) {
            (Some(Open::Typing(text)), _) => {
                match key {
                    Key::Enter => {
                        self.keep_typed(field, text);
                        self.frit.at = self.frit.at.saturating_add(1).min(NEW_FIELDS);
                    }
                    Key::Escape => {}
                    _ => self.open = Some(Open::Typing(text)),
                }
                Vec::new()
            }
            (Some(Open::List(mut lb)), Some(field)) => {
                let choices = choices(shared, field);
                match lb.key(key, choices.len()) {
                    Act::Picked(i) => match choices.into_iter().nth(i).map(|c| c.value) {
                        Some(Value::Other) => self.open = Some(Open::Typing(String::new())),
                        Some(Value::Box(v)) => self.frit.boxes = v,
                        Some(Value::Leave) => self.frit.put(field, None),
                        Some(Value::Pick(p)) => self.frit.put(field, Some(p)),
                        None => {}
                    },
                    Act::Closed => {}
                    Act::Moved | Act::Ignored => self.open = Some(Open::List(lb)),
                }
                Vec::new()
            }
            (Some(Open::List(_)), None) | (None, _) => self.rows_key(shared, field, key),
        }
    }

    /// The value typed on the field `field`, kept: the name, or a font
    /// that fills the role. An empty font fills nothing.
    fn keep_typed(&mut self, field: Option<Field>, text: String) {
        match field {
            Some(Field::Name) => self.frit.name = text,
            Some(field) if !text.trim().is_empty() => {
                let spelling = crate::commands::source_spelling(text.trim());
                self.frit
                    .put(field, Some(Pick::source(text.trim(), &spelling)));
            }
            _ => {}
        }
    }

    /// A key on the form's rows, with nothing open over them.
    fn rows_key(&mut self, shared: &Shared, field: Option<Field>, key: Key) -> Vec<Command> {
        match (key, field) {
            (Key::Escape, _) => {
                self.frit.at = 0;
                self.offset = 0;
                vec![Command::Request(Request::Go(View::Looks))]
            }
            (Key::Enter, Some(Field::Name)) => {
                self.open = Some(Open::Typing(self.frit.name.clone()));
                Vec::new()
            }
            (Key::Enter, Some(field)) => {
                let at = choices(shared, field)
                    .iter()
                    .position(|c| match &c.value {
                        Value::Leave => self.frit.holds(field, None),
                        Value::Pick(p) => self.frit.holds(field, Some(p)),
                        Value::Box(v) => self.frit.boxes == *v,
                        Value::Other => false,
                    })
                    .unwrap_or(0);
                self.open = Some(Open::List(ListBox::at(at)));
                Vec::new()
            }
            (Key::Enter, None) => self.add(shared),
            _ => {
                self.frit.at = step(self.frit.at, NEW_FIELDS.saturating_add(1), key);
                Vec::new()
            }
        }
    }

    /// The add row: the frit added to the install, once the frit reader
    /// takes it, and the Looks tab shown again; or why not, on the
    /// message line.
    fn add(&mut self, shared: &Shared) -> Vec<Command> {
        let name = self.frit.name();
        let text = self.frit.text();
        let path = shared.inputs.frits.join(format!("{name}.vtfrit"));
        let read = if self.frit.text.is_none() {
            Err(words::NEW_NO_TEXT.to_owned())
        } else if !crate::frit::valid_name(&name) || crate::frit::RESERVED.contains(&name.as_str())
        {
            Err(words::NEW_BAD_NAME.to_owned())
        } else if shared
            .inputs
            .installed
            .get(&name)
            .is_some_and(|b| std::str::from_utf8(b).is_err())
        {
            Err(words::new_replaces_binary(&name))
        } else {
            crate::frit::Frit::parse(text.as_bytes(), &path, &name)
                .map(|_| ())
                .map_err(|e| e.why)
        };
        match read {
            // the draft's folder keeps the new font set at once, in place of
            // a removal of the installed one of its name, so leaving the
            // TUI before Install loses nothing
            //
            // References:
            // - ADR-1403 D8, "A draft's font set changes": the folder, and that quitting loses nothing.
            Ok(()) => {
                let Some(draft) = &shared.draft else {
                    return vec![Command::Change(Change::Say(words::NO_DESK.to_owned()))];
                };
                self.frit = NewFrit::default();
                self.offset = 0;
                vec![
                    Command::Effect(Effect::Desk(Op::FontSet {
                        frit: name.clone(),
                        edit: FontSetEdit::Keep(text),
                        text: draft.text.clone(),
                    })),
                    Command::Change(Change::Say(words::new_added(&name))),
                    Command::Request(Request::Go(View::Looks)),
                ]
            }
            Err(why) => vec![Command::Change(Change::Say(words::new_refused(why)))],
        }
    }
}

impl Tab for Form {
    /// The form shows under the Looks tab's name.
    fn name(&self) -> &'static str {
        words::LOOKS
    }

    fn node(&self) -> &'static str {
        Form::node(self)
    }

    fn body(&self, shared: &Shared, size: Size) -> Body {
        Form::body(self, shared, size)
    }

    fn offset(&self) -> usize {
        self.offset
    }

    fn show_from(&mut self, row: usize) {
        self.offset = row;
    }

    fn planned_keys(&self, _shared: &Shared) -> &'static str {
        words::KEYS_NEW
    }

    fn typing(&self) -> bool {
        Form::typing(self)
    }

    fn type_key(&mut self, key: Key) {
        Form::type_key(self, key);
    }

    /// The form is open over the Looks tab, so no action key acts in it.
    fn takes_action(&self, _action: char) -> bool {
        false
    }

    fn update(&mut self, shared: &Shared, _room: Room, key: Key) -> Vec<Command> {
        Form::update(self, shared, key)
    }
}

#[cfg(test)]
#[expect(clippy::panic, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::batch::State;
    use crate::describe::{Description, Format};
    use crate::discover::{Described, Folder, Found, Place};
    use std::path::PathBuf;

    /// An active system font at `path` whose one face is `family` in
    /// `style`, monospaced and covering Latin.
    fn font(path: &str, family: &str, style: &str) -> (Found, Described) {
        let found = Found {
            path: PathBuf::from(path),
            folder: Folder {
                path: PathBuf::from("/usr/share/fonts"),
                place: Place::System,
            },
            trust: Ok(()),
            state: State::Active,
            duplicate: None,
        };
        let face = Face {
            family: family.into(),
            style: style.into(),
            version: None,
            outline: true,
            monospaced: true,
            scripts: vec!["Latin"],
            groups: Vec::new(),
            strikes: Vec::new(),
            codepoints: 95,
            coverage: vec![(0x20, 0x7e)],
            coverage_cut: false,
        };
        let described = Described::Font(Description {
            format: Format::OpenType,
            faces: vec![face],
        });
        (found, described)
    }

    /// The text font's pick carries each active file of its family whose
    /// face is bold, italic or both, with its `style=` line, and the list
    /// of text fonts offers only the upright face. A face of another
    /// family stays out of the pick.
    #[test]
    fn the_text_font_brings_its_bold_and_italic_faces() {
        let fonts = [
            font("/f/Mono-Regular.ttf", "Mono", "Regular"),
            font("/f/Mono-Bold.ttf", "Mono", "Bold"),
            font("/f/Mono-Oblique.ttf", "Mono", "Oblique"),
            font("/f/Mono-BoldItalic.ttf", "Mono", "Bold Italic"),
            font("/f/Other-Bold.ttf", "Other", "Bold"),
        ];
        let mut inputs = crate::tui::fixture::inputs();
        inputs.font_files = super::super::super::super::fonts_found::Fonts::new(
            fonts.iter().map(|(f, _)| f.clone()).collect(),
        );
        inputs.font_files.describe(
            fonts
                .iter()
                .map(|(f, d)| (f.path.clone(), d.clone()))
                .collect(),
        );
        let (app, _) = super::super::super::super::model::App::new(inputs, 80, 25);
        let text = choices(&app.shared, Field::Text);
        let fonts: Vec<&str> = text
            .iter()
            .filter(|c| matches!(c.value, Value::Pick(_)) && c.note != words::BUILT_IN)
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(fonts, ["Mono"], "{text:?}");
        let Some(Value::Pick(pick)) = text.first().map(|c| c.value.clone()) else {
            panic!("no pick: {text:?}");
        };
        assert_eq!(
            pick.lines,
            [
                "source=outline:/f/Mono-Regular.ttf",
                "source=outline:/f/Mono-Bold.ttf",
                "style=bold",
                "source=outline:/f/Mono-Oblique.ttf",
                "style=italic",
                "source=outline:/f/Mono-BoldItalic.ttf",
                "style=bold-italic",
            ]
        );
    }
}
