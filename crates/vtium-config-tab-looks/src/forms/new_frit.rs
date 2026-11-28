//! The form that makes a new font set. It opens over the Looks tab on
//! `n` among the font sets. Its rows follow the roles a font set fills,
//! in the order vtium tries them: the text font, box drawing, the
//! scripts, the symbols and icons, the emoji and the fallback. The font
//! set's name comes after them, and the last row adds the font set.
//!
//! Enter on a role opens its choices in the list box under the role's
//! line: the active fonts whose faces the font helper described as
//! fitting the role, then Other…, which lets the person type a font the
//! list lacks. A font that is not active, or that the font helper has not
//! described, is not offered, and the list reads only the fonts found
//! when vtium-config started. The text role offers the fonts built into
//! vtium too, and box drawing offers where its lines come from, which
//! the font set's `glyph-source-box=` line chooses. The text font brings
//! its bold and italic faces: each active file of the same family whose
//! face is bold, italic or both follows it with its `style=` line. The
//! scripts take any number of fonts, and Enter on a script already
//! picked takes it out.
//!
//! The last row checks the font set as the console reads it, then puts it
//! into the document as one change, which Undo takes back and Save
//! installs. Esc leaves the form, and nothing is made.
//!
//! References:
//! - ADR-1201 D3, "A new font set": the roles' choices, written in the order of the roles, and typing kept for a font the list lacks.
//! - ADR-1205 D6: the roles and their order, box drawing apart from it.
//! - ADR-1205 D4: only active fonts are offered.
//! - ADR-1403 D8, "A draft's font set changes": a new font set is a change of the document, kept at once.
//! - ADR-1403 D11, "The list box": a role's list opens under its line, and the search shows where its choices are many.

use std::borrow::Cow;
use std::path::Path;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};
use ratatui_core::terminal::Frame;
use vtium_config_core::document::{Change, Edit, FontSetEdit};
use vtium_config_core::ui::{Action, Change as UiChange, Command, Shared};
use vtium_config_widgets::canvas;
use vtium_config_widgets::list_box::{self, ListBox, Outcome};
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::Search;
use vtium_config_widgets::tab::Choices;
use vtium_font_helper::describe::{Face, Group};

use super::{click, pressed, say, verb, words};
use crate::draw::put;
use crate::list;
use crate::{Looks, card};

/// The form's node.
const NODE: &str = "looks.new-frit";
/// A role's list box.
const NODE_LIST: &str = "looks.new-frit.list";
/// The name, or a font after Other…, being typed.
const NODE_TYPING: &str = "looks.new-frit.typing";
/// The search row while the person types in a list box's search.
const NODE_SEARCH: &str = "any.search";

/// The columns of a row's label, the longest label and two blanks.
const LABEL: usize = 19;

/// The form's first row, below its title and a blank row.
const ROWS_AT: u16 = 2;

/// The columns before a row's label inside the viewer.
const INDENT: u16 = 3;

/// The form's fields, in order. The row after them adds the font set.
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

    /// The field's label on the form.
    pub fn label(self) -> &'static str {
        match self {
            Field::Text => words::FIELD_TEXT,
            Field::Box => words::FIELD_BOX,
            Field::Scripts => words::FIELD_SCRIPTS,
            Field::Symbols => words::FIELD_SYMBOLS,
            Field::Emoji => words::FIELD_EMOJI,
            Field::Fallback => words::FIELD_FALLBACK,
            Field::Name => words::FIELD_NAME,
        }
    }
}

/// The form's rows: a row for each field, then the add row.
pub const ROWS: usize = Field::ALL.len() + 1;

/// One font picked for a role: how the form shows it, and the lines it
/// writes into the font set, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub shown: String,
    /// The font's own name, which a font set's name is made from.
    pub base: String,
    pub lines: Vec<String>,
}

impl Pick {
    /// A font named by its source spelling, on a `source=` line of its
    /// own.
    fn source(shown: impl Into<String>, spelling: &str) -> Pick {
        let shown = shown.into();
        Pick {
            base: shown.clone(),
            shown,
            lines: vec![format!("source={spelling}")],
        }
    }
}

/// The new font set being made: a pick for each role the person filled,
/// where box drawing comes from, and the name typed.
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
}

impl NewFrit {
    /// The font set's name: the name typed, else one made from the text
    /// font.
    pub fn name(&self) -> String {
        if self.name.trim().is_empty() {
            self.text
                .as_ref()
                .map(|t| vtium_config_core::commands::name_from(&t.base))
                .unwrap_or_default()
        } else {
            self.name.trim().to_owned()
        }
    }

    /// The font set's text, from its picks, in the order of the roles.
    pub fn text(&self) -> String {
        let name = self.name();
        let mut text = format!("# {name}: made by vtium-config's new font set form\nname={name}\n");
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

    /// What `field` holds, as the form's row shows it.
    pub fn shown(&self, field: Field) -> String {
        let one = |p: &Option<Pick>| {
            p.as_ref()
                .map_or_else(|| words::NONE_CHOSEN.to_owned(), |p| p.shown.clone())
        };
        match field {
            Field::Text => one(&self.text),
            Field::Box => box_shown(self.boxes).to_owned(),
            Field::Scripts if self.scripts.is_empty() => words::NONE_CHOSEN.to_owned(),
            Field::Scripts => self
                .scripts
                .iter()
                .map(|p| p.shown.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            Field::Symbols => one(&self.symbols),
            Field::Emoji => one(&self.emoji),
            Field::Fallback => one(&self.fallback),
            Field::Name if self.name.trim().is_empty() => words::name_from(&self.name()),
            Field::Name => self.name.clone(),
        }
    }

    /// The pick of a field that holds one.
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

    /// Whether `field` holds what `value` puts there.
    fn holds(&self, field: Field, value: &Value) -> bool {
        let same = |p: &Option<Pick>, want: Option<&Pick>| {
            p.as_ref().map(|p| &p.lines) == want.map(|p| &p.lines)
        };
        let want = match value {
            Value::Leave => None,
            Value::Pick(p) => Some(p),
            Value::Box(v) => return field == Field::Box && self.boxes == *v,
            Value::Other => return false,
        };
        match field {
            Field::Text => same(&self.text, want),
            Field::Symbols => same(&self.symbols, want),
            Field::Emoji => same(&self.emoji, want),
            Field::Fallback => same(&self.fallback, want),
            Field::Scripts => match want {
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
        Some("sprites") => words::BOX_OWN,
        Some("font") => words::BOX_FONT,
        Some("auto") => words::BOX_AUTO,
        _ => words::BOX_DEFAULT,
    }
}

/// What a choice of a role's list does when it is kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// Leaves the role empty, or for the scripts empties it.
    Leave,
    /// Fills the role with a font.
    Pick(Pick),
    /// Sets where box drawing comes from, `None` for no line.
    Box(Option<&'static str>),
    /// Lets the person type a font that the list lacks.
    Other,
}

/// One choice of a role's list: its name, the note beside it, and what
/// keeping it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    pub note: String,
    pub value: Value,
}

/// A role's choices, which the list box under the role's line shows, each
/// as its name with its note in a column beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleChoices {
    pub field: Field,
    pub choices: Vec<Choice>,
    pub current: Option<usize>,
    /// Each choice's text, its name padded to the widest name, then its
    /// note.
    texts: Vec<String>,
}

impl RoleChoices {
    fn new(field: Field, choices: Vec<Choice>, frit: &NewFrit) -> RoleChoices {
        let widest = choices
            .iter()
            .filter(|c| !c.note.is_empty())
            .map(|c| canvas::cells(&c.name))
            .max()
            .unwrap_or(0);
        let texts = choices
            .iter()
            .map(|c| {
                if c.note.is_empty() {
                    c.name.clone()
                } else {
                    let pad = widest.saturating_sub(canvas::cells(&c.name));
                    format!("{}{}  {}", c.name, " ".repeat(pad), c.note)
                }
            })
            .collect();
        let current = choices.iter().position(|c| frit.holds(field, &c.value));
        RoleChoices {
            field,
            choices,
            current,
            texts,
        }
    }
}

impl Choices<Value> for RoleChoices {
    fn count(&self) -> usize {
        self.choices.len()
    }

    fn text(&self, choice: usize) -> Cow<'_, str> {
        self.texts
            .get(choice)
            .map_or(Cow::Borrowed(""), |t| Cow::Borrowed(t.as_str()))
    }

    fn current(&self) -> Option<usize> {
        self.current
    }

    fn keep(&self, choice: usize) -> Value {
        self.choices
            .get(choice)
            .map_or(Value::Leave, |c| c.value.clone())
    }
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
    let name = if style.is_empty() || style.eq_ignore_ascii_case("regular") {
        face.family.clone()
    } else {
        format!("{} {style}", face.family)
    };
    canvas::printable(&name)
}

/// A family's name with only its letters and digits, in lowercase, so
/// that `Cullet Mono` and `CulletMono` are one family.
fn squashed(family: &str) -> String {
    family
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The scripts a text font usually covers itself, so a font that covers
/// only these is not offered for the scripts.
const TEXT_SCRIPTS: [&str; 3] = ["Latin", "Greek", "Cyrillic"];

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

/// The active font files vtium can use, each with the faces the font
/// helper described and the `source=` spelling that names the file.
fn active_fonts(shared: &Shared) -> Vec<(&Path, &[Face], String)> {
    shared
        .found
        .fonts
        .files()
        .iter()
        .filter(|f| {
            let r = &f.resource;
            !r.own && !r.hidden && r.trust.is_ok() && card::active(shared, r)
        })
        .filter_map(|f| {
            let spelling = vtium_config_core::discover::source_of(f.path())?;
            Some((f.path(), f.faces(), spelling))
        })
        .collect()
}

/// The text font's pick for the file `spelling` names, whose face is
/// `face`: its `source=` line, then each active file of the same family
/// whose face is bold, italic or both, with its `style=` line.
fn text_pick(fonts: &[(&Path, &[Face], String)], face: &Face, spelling: &str) -> Pick {
    let mut lines = vec![format!("source={spelling}")];
    let mut styles = Vec::new();
    let family = squashed(&face.family);
    for (_, faces, other) in fonts {
        let Some(f) = faces.first() else { continue };
        let style = match styled(f) {
            (true, true) => "bold-italic",
            (true, false) => "bold",
            (false, true) => "italic",
            (false, false) => continue,
        };
        if squashed(&f.family) == family && other != spelling {
            lines.push(format!("source={other}"));
            lines.push(format!("style={style}"));
            styles.push(style);
        }
    }
    Pick {
        shown: words::with_styles(&face_name(face), &styles),
        base: face_name(face),
        lines,
    }
}

/// What can fill `field`'s role, as its list shows it. The roles but the
/// text start with a choice that leaves the role empty, then come the
/// active fonts that fit the role, and Other…. The text role offers the
/// fonts built into vtium too, which are always active.
pub fn choices(shared: &Shared, field: Field, frit: &NewFrit) -> RoleChoices {
    let choice = |name: String, note: String, value| Choice { name, note, value };
    if field == Field::Box {
        let out = [None, Some("sprites"), Some("font"), Some("auto")]
            .into_iter()
            .map(|v| choice(box_shown(v).to_owned(), String::new(), Value::Box(v)))
            .collect();
        return RoleChoices::new(field, out, frit);
    }
    let mut out = Vec::new();
    if field != Field::Text {
        out.push(choice(
            words::LEAVE_EMPTY.to_owned(),
            String::new(),
            Value::Leave,
        ));
    }
    let fonts = active_fonts(shared);
    for (path, faces, spelling) in &fonts {
        let Some(face) = faces.iter().find(|f| fits(f, field)) else {
            continue;
        };
        let pick = if field == Field::Text {
            text_pick(&fonts, face, spelling)
        } else {
            Pick::source(face_name(face), spelling)
        };
        let note = canvas::printable(&path.display().to_string());
        out.push(choice(face_name(face), note, Value::Pick(pick)));
    }
    if field == Field::Text {
        out.extend(shared.found.fonts.built_in.iter().map(|name| {
            choice(
                (*name).to_owned(),
                words::BUILT_IN.to_owned(),
                Value::Pick(Pick::source(*name, &format!("builtin:{name}"))),
            )
        }));
    }
    out.push(choice(words::OTHER.to_owned(), String::new(), Value::Other));
    RoleChoices::new(field, out, frit)
}

/// What is open over the form's rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormOpen {
    /// Nothing: Up and Down move between the rows.
    Nothing,
    /// The highlighted role's choices, in the list box under its line.
    List { list: Box<ListBox>, field: Field },
    /// The name, or a font typed after Other… for a role.
    Typing { field: Field, text: String },
}

/// The new font set's form: the font set being made, the highlighted row,
/// and what is open over the rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    pub frit: NewFrit,
    /// The highlighted row: a field, or the add row after them.
    pub row: usize,
    pub open: FormOpen,
}

impl Default for Form {
    fn default() -> Form {
        Form {
            frit: NewFrit::default(),
            row: 0,
            open: FormOpen::Nothing,
        }
    }
}

impl Form {
    /// The highlighted field, `None` on the add row.
    pub fn field(&self) -> Option<Field> {
        Field::ALL.get(self.row).copied()
    }

    /// The node the form is on: a role's list box, its search row while
    /// the person types in it, a value being typed, or the form's rows.
    pub fn node(&self) -> &'static str {
        match &self.open {
            FormOpen::List { list, .. } if list.filter().is_some_and(Search::typing) => NODE_SEARCH,
            FormOpen::List { .. } => NODE_LIST,
            FormOpen::Typing { .. } => NODE_TYPING,
            FormOpen::Nothing => NODE,
        }
    }

    /// The search row of the open list box, while it has one.
    pub fn search(&self) -> Option<&Search> {
        match &self.open {
            FormOpen::List { list, .. } => list.filter(),
            _ => None,
        }
    }
}

/// The open form of `tab`.
fn form_of(tab: &mut Looks) -> Option<&mut Form> {
    tab.form.as_mut()
}

/// Acts on `action` while the form is open.
pub fn update(tab: &mut Looks, shared: &Shared, action: &Action) -> Vec<Command> {
    let area = tab.seen.get();
    let Some(form) = form_of(tab) else {
        return Vec::new();
    };
    match &form.open {
        FormOpen::Typing { .. } => typing(form, action),
        FormOpen::List { .. } => list_keys(form, shared, action, area),
        FormOpen::Nothing => rows_keys(tab, shared, action, area),
    }
}

/// A key on the form's rows, with nothing open over them.
fn rows_keys(tab: &mut Looks, shared: &Shared, action: &Action, area: Rect) -> Vec<Command> {
    if let Some(at) = click(action) {
        let Some(form) = form_of(tab) else {
            return Vec::new();
        };
        let Some(&(_, row)) = rows(area).iter().find(|(r, _)| r.contains(at)) else {
            return Vec::new();
        };
        if row != form.row {
            form.row = row;
            return Vec::new();
        }
        return enter(tab, shared);
    }
    let last = ROWS.saturating_sub(1);
    let Some(form) = form_of(tab) else {
        return Vec::new();
    };
    match verb(action) {
        "up" => form.row = form.row.saturating_sub(1),
        "down" => form.row = form.row.saturating_add(1).min(last),
        "home" | "pgup" => form.row = 0,
        "end" | "pgdn" => form.row = last,
        "esc" => tab.form = None,
        "enter" | "list" => return enter(tab, shared),
        _ => {}
    }
    Vec::new()
}

/// Enter on the highlighted row: a role's list box, the name typed, or
/// the font set added.
fn enter(tab: &mut Looks, shared: &Shared) -> Vec<Command> {
    let Some(form) = form_of(tab) else {
        return Vec::new();
    };
    match form.field() {
        Some(Field::Name) => {
            form.open = FormOpen::Typing {
                field: Field::Name,
                text: form.frit.name.clone(),
            };
            say(words::TYPE_NAME)
        }
        Some(field) => {
            let c = choices(shared, field, &form.frit);
            form.open = FormOpen::List {
                list: Box::new(ListBox::open(NODE_LIST, &c)),
                field,
            };
            Vec::new()
        }
        None => add(tab, shared),
    }
}

/// The keys and clicks of a role's list box. A kept choice fills the role,
/// or lets the person type a font after Other…; Esc closes the box and
/// changes nothing.
fn list_keys(form: &mut Form, shared: &Shared, action: &Action, area: Rect) -> Vec<Command> {
    let FormOpen::List { list, field } = &mut form.open else {
        return Vec::new();
    };
    let field = *field;
    let c = choices(shared, field, &form.frit);
    let rect = list_rect(list, &c, form.row, area);
    let outcome = match (click(action), pressed(action)) {
        (Some(at), _) => list.click(at, &c, rect),
        (None, Some(key)) => list.key(&key, &c, rect),
        (None, None) => Outcome::Ignored,
    };
    match outcome {
        Outcome::Kept(Value::Other) => {
            form.open = FormOpen::Typing {
                field,
                text: String::new(),
            };
            return say(words::TYPE_FONT);
        }
        Outcome::Kept(Value::Box(v)) => form.frit.boxes = v,
        Outcome::Kept(Value::Leave) => form.frit.put(field, None),
        Outcome::Kept(Value::Pick(p)) => form.frit.put(field, Some(p)),
        Outcome::Closed => {}
        Outcome::Moved | Outcome::Searched | Outcome::Ignored | Outcome::Outside => {
            return Vec::new();
        }
    }
    form.open = FormOpen::Nothing;
    Vec::new()
}

/// The keys of a value being typed: a character or Backspace edits it,
/// Enter keeps it and moves to the next row, and Esc drops it. On the
/// name the value is the name; on a role it is a font, which fills the
/// role unless it is empty.
fn typing(form: &mut Form, action: &Action) -> Vec<Command> {
    let FormOpen::Typing { field, text } = &mut form.open else {
        return Vec::new();
    };
    match action.row {
        "enter.typing.new-frit" | "enter.typing" => {
            let (field, text) = (*field, std::mem::take(text));
            match field {
                Field::Name => form.frit.name = text,
                _ if text.trim().is_empty() => {}
                _ => {
                    let spelling = vtium_config_core::commands::source_spelling(text.trim());
                    form.frit
                        .put(field, Some(Pick::source(text.trim(), &spelling)));
                }
            }
            form.open = FormOpen::Nothing;
            form.row = form.row.saturating_add(1).min(ROWS.saturating_sub(1));
        }
        "esc.typing" => form.open = FormOpen::Nothing,
        "backspace.typing" => {
            text.pop();
        }
        "char.typing" | "q.typing" => {
            if let Some(ch) = pressed(action).and_then(|k| k.typed()) {
                text.push(ch);
            }
        }
        _ => {}
    }
    Vec::new()
}

/// Why the form cannot add `frit` as the font set `name`, `None` where it
/// can: it has no text font, its name is not one a font set can have, the
/// installed file of that name is not text, or the console would not read
/// it.
fn refusal(shared: &Shared, frit: &NewFrit, name: &str, text: &str) -> Option<String> {
    if frit.text.is_none() {
        return Some(words::NEEDS_TEXT.to_owned());
    }
    if !vtium_font_helper::frit::valid_name(name)
        || vtium_font_helper::frit::RESERVED.contains(&name)
    {
        return Some(words::BAD_NAME.to_owned());
    }
    if shared
        .document()
        .installed()
        .frits
        .get(name)
        .is_some_and(|b| std::str::from_utf8(b).is_err())
    {
        return Some(words::replaces_binary(name));
    }
    let file = Path::new(name).with_extension("vtfrit");
    vtium_font_helper::frit::Frit::parse(text.as_bytes(), &file, name)
        .err()
        .map(|e| words::does_not_read(&e.why))
}

/// The add row: the font set put into the document as one change, the
/// form closed and the new font set highlighted in the list; or why not,
/// on the message line.
fn add(tab: &mut Looks, shared: &Shared) -> Vec<Command> {
    let Some(form) = tab.form.as_ref() else {
        return Vec::new();
    };
    let name = form.frit.name();
    let text = form.frit.text();
    if let Some(why) = refusal(shared, &form.frit, &name, &text) {
        return say(why);
    }
    tab.form = None;
    tab.highlight = place_after_add(shared, &name);
    vec![Command::Change(UiChange::Document(Change::new(
        words::added(&name),
        Edit::FontSet {
            frit: name,
            edit: FontSetEdit::Keep(text),
        },
    )))]
}

/// The place in the list the font set `name` takes once the document
/// adds it: after the looks, among the font sets in the order of their
/// names.
fn place_after_add(shared: &Shared, name: &str) -> usize {
    let looks = list::items(shared)
        .iter()
        .filter(|i| matches!(i, list::Item::Look(_) | list::Item::NoLooks))
        .count();
    let before = list::font_sets(shared)
        .iter()
        .filter(|n| n.as_str() < name)
        .count();
    looks.saturating_add(before)
}

/// The viewer `area` less a column at each side.
fn inset(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(1),
        area.y,
        area.width.saturating_sub(2),
        area.height,
    )
}

/// Each row of the form in `area`, by its number.
pub fn rows(area: Rect) -> Vec<(Rect, usize)> {
    let r = inset(area);
    (0..ROWS)
        .filter_map(|i| {
            let y =
                r.y.saturating_add(ROWS_AT)
                    .saturating_add(u16::try_from(i).ok()?);
            (y < r.bottom()).then(|| (Rect::new(r.x, y, r.width, 1), i))
        })
        .collect()
}

/// The column where a row's value starts.
fn value_x(area: Rect) -> u16 {
    inset(area)
        .x
        .saturating_add(INDENT.saturating_sub(1))
        .saturating_add(u16::try_from(LABEL).unwrap_or(0))
}

/// Where the list box of the row `row` goes, under the row's line, its
/// border a column before the value.
fn list_rect(list: &ListBox, c: &RoleChoices, row: usize, area: Rect) -> Rect {
    let y = rows(area)
        .iter()
        .find(|(_, i)| *i == row)
        .map_or(area.y, |(r, _)| r.y);
    list.place(c, area, y, value_x(area).saturating_sub(1))
}

/// Draws the form in `area`, the viewer, with the open list box over its
/// rows and the cursor in a value being typed.
pub fn draw(form: &Form, shared: &Shared, frame: &mut Frame<'_>, area: Rect, color: bool) {
    let buf = frame.buffer_mut();
    let r = inset(area);
    put(
        buf,
        r.x,
        r.y,
        words::FORM_TITLE,
        Role::Heading.style(color).cell(),
        r,
    );
    let mut cursor = None;
    for (rect, i) in rows(area) {
        let lit = i == form.row && !matches!(form.open, FormOpen::List { .. });
        let cursor_here = draw_row(form, buf, rect, i, lit, color);
        cursor = cursor.or(cursor_here);
    }
    let note_y =
        r.y.saturating_add(ROWS_AT)
            .saturating_add(u16::try_from(ROWS).unwrap_or(0))
            .saturating_add(1);
    put(
        buf,
        r.x,
        note_y,
        words::ROW_ORDER,
        Role::Dim.style(color).cell(),
        r,
    );
    if let FormOpen::List { list, field } = &form.open {
        let c = choices(shared, *field, &form.frit);
        list.draw(&c, buf, list_rect(list, &c, form.row, area), color);
    }
    if let Some(at) = cursor {
        frame.set_cursor_position(at);
    }
}

/// Draws the form's row `i` in `rect`, highlighted where `lit` says, and
/// returns where the cursor goes while its value is typed.
fn draw_row(
    form: &Form,
    buf: &mut Buffer,
    rect: Rect,
    i: usize,
    lit: bool,
    color: bool,
) -> Option<Position> {
    let base = if lit { Role::RowHighlight } else { Role::Row };
    let style = base.style(color).cell();
    put(
        buf,
        rect.x,
        rect.y,
        &" ".repeat(usize::from(rect.width)),
        style,
        rect,
    );
    let x = rect.x.saturating_add(INDENT.saturating_sub(1));
    let Some(field) = Field::ALL.get(i).copied() else {
        let add = if lit {
            Role::RowHighlight
        } else {
            Role::Heading
        };
        put(buf, x, rect.y, words::ADD, add.style(color).cell(), rect);
        return None;
    };
    let mut cursor = None;
    let value = match &form.open {
        FormOpen::Typing { text, .. } if i == form.row => {
            let at = x
                .saturating_add(u16::try_from(LABEL).unwrap_or(0))
                .saturating_add(u16::try_from(canvas::cells(text)).unwrap_or(0));
            cursor = Some(Position::new(
                at.min(rect.right().saturating_sub(1)),
                rect.y,
            ));
            text.clone()
        }
        _ => form.frit.shown(field),
    };
    let label = field.label();
    let pad = LABEL.saturating_sub(canvas::cells(label));
    let text = format!("{label}{}{value}", " ".repeat(pad));
    put(buf, x, rect.y, &text, style, rect);
    cursor
}

/// Where each row of the form and the open list box's parts are drawn in
/// `area`.
pub fn places(
    form: &Form,
    shared: &Shared,
    area: Rect,
) -> (Vec<(Rect, usize)>, Option<list_box::Places>) {
    let list = match &form.open {
        FormOpen::List { list, field } => {
            let c = choices(shared, *field, &form.frit);
            Some(list.places(&c, list_rect(list, &c, form.row, area)))
        }
        _ => None,
    };
    (rows(area), list)
}

/// F1's text on the form: what a font set is and what each role holds,
/// and in a role's list the highlighted choice in full. Nothing while a
/// value is typed, where F1 does nothing.
pub fn full_text(form: &Form, shared: &Shared) -> Option<String> {
    let mut out = words::ABOUT_FONT_SETS.to_owned();
    match &form.open {
        FormOpen::Typing { .. } => return None,
        FormOpen::List { list, field } => {
            let c = choices(shared, *field, &form.frit);
            if let Some(choice) = c.choices.get(list.highlight()) {
                out.push_str("\n\n");
                out.push_str(words::THIS_CHOICE);
                out.push_str("\n  ");
                out.push_str(&choice.name);
                if !choice.note.is_empty() {
                    out.push_str("\n  ");
                    out.push_str(&choice.note);
                }
                if let Value::Pick(p) = &choice.value {
                    for line in &p.lines {
                        out.push_str("\n  ");
                        out.push_str(line);
                    }
                }
            }
        }
        FormOpen::Nothing => {
            if let Some(field) = form.field() {
                out.push_str(&format!(
                    "\n\n{}: {}",
                    field.label(),
                    form.frit.shown(field)
                ));
            }
        }
    }
    Some(out)
}
