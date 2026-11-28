//! A look's card: its name, font set, size, color scheme, vtlook and
//! pointer theme, one line each, and below them its fonts by role, each
//! with its mark. Beside the list the card is a summary of the
//! highlighted look; Enter opens it over the viewer to edit it.
//!
//! Each line with choices opens them in the list box under itself. Left
//! and Right step the line to the choice before or after, and Enter or a
//! click keeps a pick from the list box. Either is one change to the look,
//! which the shell's one change function makes and Undo takes back. The
//! first choice of each line leaves the look's own key unset, so the look
//! takes vtium.conf's value. A look with no `vtlook` line draws the
//! vtlook vtium.conf's top level sets, and No filter where it sets none,
//! so No filter writes `vtlook = default` only where the top level sets
//! another. A value the look sets that the choices lack joins them, so
//! stepping from it never drops the look's key.
//!
//! References:
//! - ADR-1205 D6: a look is assembled by role, and picks its effects through its vtlook and one active pointer theme.
//! - ADR-1205 D7: a look offers the sizes its fonts supply.
//! - ADR-1205 D10: a look picks its own color scheme, a built-in one or an active scheme file.
//! - ADR-1304 D5: the look's vtlook wins, then the top level's, then the built-in one, which `vtlook = default` names.
//! - ADR-1403 D11, "The list box": a line's choices open under it, and a kept pick is one change.

use std::borrow::Cow;

use vtium_config_core::commands::value_text;
use vtium_config_core::document::{Change, ConfEdit, Edit};
use vtium_config_core::found::effects::From;
use vtium_config_core::found::{Activation, Origin, Resource};
use vtium_config_core::ui::{Change as UiChange, Command, Shared};
use vtium_config_widgets::tab::Choices;
use vtium_settings::keys::Default as KeyDefault;

use crate::list::{self, Look};
use crate::{sizes, words};

/// The card's lines, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    Name,
    FontSet,
    Size,
    Palette,
    Vtlook,
    Pointer,
}

impl Line {
    pub const ALL: [Line; 6] = [
        Line::Name,
        Line::FontSet,
        Line::Size,
        Line::Palette,
        Line::Vtlook,
        Line::Pointer,
    ];

    /// The look's key the line sets, for a line with choices.
    pub fn key(self) -> Option<&'static str> {
        match self {
            Line::FontSet => Some("font-name"),
            Line::Size => Some("font-size"),
            Line::Palette => Some("palette"),
            Line::Vtlook => Some("vtlook"),
            Line::Pointer => Some("pointer-theme"),
            Line::Name => None,
        }
    }

    /// The line's label on the card.
    pub fn label(self) -> &'static str {
        match self {
            Line::Name => words::LINE_NAME,
            Line::FontSet => words::LINE_FONT_SET,
            Line::Size => words::LINE_SIZE,
            Line::Palette => words::LINE_PALETTE,
            Line::Vtlook => words::LINE_VTLOOK,
            Line::Pointer => words::LINE_POINTER,
        }
    }
}

/// One choice of a line: what the list box shows, and the look's value it
/// writes, `None` to leave the key unset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub text: String,
    pub value: Option<String>,
}

impl Choice {
    fn new(text: impl Into<String>, value: Option<String>) -> Choice {
        Choice {
            text: text.into(),
            value,
        }
    }
}

/// A line's choices, which the list box under the line shows, with the
/// look's own among them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineChoices {
    pub line: Line,
    pub choices: Vec<Choice>,
    pub current: Option<usize>,
}

impl Choices<Option<String>> for LineChoices {
    fn count(&self) -> usize {
        self.choices.len()
    }

    fn text(&self, choice: usize) -> Cow<'_, str> {
        self.choices
            .get(choice)
            .map_or(Cow::Borrowed(""), |c| Cow::Borrowed(c.text.as_str()))
    }

    fn current(&self) -> Option<usize> {
        self.current
    }

    fn keep(&self, choice: usize) -> Option<String> {
        self.choices.get(choice).and_then(|c| c.value.clone())
    }
}

/// vtium.conf's top-level value of `key`: what the document sets, else
/// the key's default, or `None` for a key whose default is to be unset.
pub fn top(shared: &Shared, key: &str) -> Option<String> {
    let k = vtium_settings::keys::find(key)?;
    let (value, set) = value_text(k, Some(shared.document().text()));
    (set || k.default != KeyDefault::Unset).then_some(value)
}

/// The vtlook vtium.conf's top level sets, where it names one other than
/// the built-in one, which every look with no `vtlook` line draws.
fn top_vtlook(shared: &Shared) -> Option<String> {
    vtium_settings::setting(shared.document().text(), "vtlook")
        .filter(|v| v != vtium_settings::vtlook::DEFAULT && !v.is_empty())
}

/// The font set the look draws: its own, else vtium.conf's.
pub fn font_set(shared: &Shared, look: &Look) -> Option<String> {
    look.font_set
        .clone()
        .or_else(|| vtium_settings::setting(shared.document().text(), "font-name"))
}

/// A size as the card shows it: automatic for 0, else in points.
pub fn size_name(value: &str) -> String {
    match value.parse::<u32>() {
        Ok(0) => words::AUTOMATIC.to_owned(),
        Ok(n) => words::points(n),
        Err(_) => value.to_owned(),
    }
}

/// Whether `r` is active once the document's own changes count.
pub fn active(shared: &Shared, r: &Resource) -> bool {
    match r.activation(&shared.document().content().active) {
        Activation::BuiltIn => true,
        Activation::Draft(c) => c == vtium_config_core::batch::Change::Activate,
        Activation::Installed(origin) => matches!(
            origin,
            Origin::BatchFile | Origin::FreshInstall | Origin::List { .. }
        ),
    }
}

/// The look's value of `line`'s key, `None` where it leaves the key to
/// vtium.conf. Where the top level sets no vtlook, a look that names the
/// built-in one draws what a look that names none draws, so it is `None`
/// too.
pub fn value(shared: &Shared, look: &Look, line: Line) -> Option<String> {
    match line {
        Line::FontSet => look.font_set.clone(),
        Line::Size => look.size.clone(),
        Line::Palette => look.palette.clone(),
        Line::Vtlook => look
            .vtlook
            .clone()
            .filter(|v| v != vtium_settings::vtlook::DEFAULT || top_vtlook(shared).is_some()),
        Line::Pointer => look.pointer.clone(),
        Line::Name => None,
    }
}

/// The choices of `line` for `look`, in the list box's order, with the
/// look's own value among them.
pub fn choices(shared: &Shared, look: &Look, line: Line) -> LineChoices {
    let mut out = match line {
        Line::Name => Vec::new(),
        Line::FontSet => font_set_choices(shared),
        Line::Size => size_choices(shared, look),
        Line::Palette => palette_choices(shared),
        Line::Vtlook => vtlook_choices(shared),
        Line::Pointer => pointer_choices(shared),
    };
    let now = value(shared, look, line);
    let mut current = out.iter().position(|c| c.value == now);
    if current.is_none()
        && line != Line::Name
        && let Some(own) = &now
    {
        current = Some(out.len());
        let shown = match line {
            Line::Size => size_name(own),
            Line::Pointer => pointer_value_name(own),
            _ => own.clone(),
        };
        out.push(Choice::new(words::own_choice(&shown), now.clone()));
    }
    LineChoices {
        line,
        choices: out,
        current,
    }
}

/// The font sets a look can name: vtium.conf's first, which leaves the
/// look's own unset, then each font set the document sees that reads,
/// which leaves out one the document removes, then the fonts built into
/// vtium.
fn font_set_choices(shared: &Shared) -> Vec<Choice> {
    let top = vtium_settings::setting(shared.document().text(), "font-name");
    let mut out = vec![Choice::new(
        words::from_conf(top.as_deref().unwrap_or(words::NONE)),
        None,
    )];
    out.extend(
        list::font_sets(shared)
            .into_iter()
            .filter(|n| matches!(sizes::font_set(shared, n), Some(Ok(_))))
            .map(|n| Choice::new(n.clone(), Some(n))),
    );
    out.extend(
        vtium_font_helper::frit::RESERVED
            .iter()
            .map(|n| Choice::new(words::built_in_font(n), Some((*n).to_owned()))),
    );
    out
}

/// The sizes a look can take: vtium.conf's first, automatic, then the
/// sizes the look's text font supplies.
fn size_choices(shared: &Shared, look: &Look) -> Vec<Choice> {
    let top = top(shared, "font-size").unwrap_or_default();
    let mut out = vec![
        Choice::new(words::from_conf(&size_name(&top)), None),
        Choice::new(words::AUTOMATIC, Some("0".to_owned())),
    ];
    let supplied = sizes::supplied(
        shared,
        font_set(shared, look).as_deref(),
        sizes::dpi(shared),
    );
    out.extend(
        supplied
            .points()
            .into_iter()
            .map(|n| Choice::new(words::points(n), Some(n.to_string()))),
    );
    out
}

/// The color schemes a look can name: vtium.conf's first, then the
/// built-in ones, then each active scheme file by the name a look gives
/// it. A packaged file that the administrator's hides, a file that takes
/// a built-in scheme's name and a name a look cannot name are left out.
fn palette_choices(shared: &Shared) -> Vec<Choice> {
    let top = top(shared, "palette").unwrap_or_default();
    let palettes = &shared.found.palettes;
    let mut out = vec![Choice::new(words::from_conf(&top), None)];
    out.extend(
        palettes
            .built_in
            .iter()
            .map(|n| Choice::new(*n, Some((*n).to_owned()))),
    );
    for f in &palettes.files {
        let nameable =
            vtium_settings::palette::named(&f.name) && !vtium_settings::palette::built_in(&f.name);
        if nameable && !f.resource.hidden && active(shared, &f.resource) {
            out.push(Choice::new(f.name.clone(), Some(f.name.clone())));
        }
    }
    out
}

/// The vtlooks a look can name, in the Effects tab's order: vtium.conf's
/// first where the top level sets one, then No filter, the presets and
/// the administrator's. A vtlook that does not read says so, and can
/// still be picked.
fn vtlook_choices(shared: &Shared) -> Vec<Choice> {
    let top = top_vtlook(shared);
    let mut out = Vec::new();
    if let Some(name) = &top {
        out.push(Choice::new(words::from_conf(name), None));
    }
    for v in shared.found.effects.vtlooks.iter().filter(|v| !v.hidden) {
        let text = match (&v.from, &v.reads) {
            (From::BuiltIn, _) => words::NO_FILTER.to_owned(),
            (_, Err(_)) => words::does_not_read(&v.name),
            (_, Ok(())) => v.name.clone(),
        };
        let value = match v.from {
            From::BuiltIn => top
                .as_ref()
                .map(|_| vtium_settings::vtlook::DEFAULT.to_owned()),
            _ => Some(v.name.clone()),
        };
        out.push(Choice::new(text, value));
    }
    out
}

/// The pointer themes a look can name: vtium.conf's first, which leaves
/// the look's own unset, then each active theme.
fn pointer_choices(shared: &Shared) -> Vec<Choice> {
    let top = top(shared, "pointer-theme");
    let shown = top
        .as_deref()
        .map_or_else(|| words::POINTER_OWN.to_owned(), pointer_value_name);
    let mut out = vec![Choice::new(words::from_conf(&shown), None)];
    out.extend(
        shared
            .found
            .pointers
            .iter()
            .filter(|r| !r.hidden && r.trust.is_ok() && active(shared, r))
            .map(|r| {
                Choice::new(
                    list::pointer_name(&r.name()),
                    Some(r.path.to_string_lossy().into_owned()),
                )
            }),
    );
    out
}

/// A `pointer-theme` value by the name its theme's list row has.
fn pointer_value_name(value: &str) -> String {
    let file = std::path::Path::new(value)
        .file_name()
        .map_or_else(|| value.to_owned(), |n| n.to_string_lossy().into_owned());
    list::pointer_name(&file)
}

/// What `line` shows for `look`: its value, or vtium.conf's where the
/// look leaves the key unset.
pub fn shown(shared: &Shared, look: &Look, line: Line) -> String {
    match line {
        Line::Name => look.name.clone(),
        Line::FontSet => match &look.font_set {
            Some(name) => name.clone(),
            None => words::from_conf(
                vtium_settings::setting(shared.document().text(), "font-name")
                    .as_deref()
                    .unwrap_or(words::NONE),
            ),
        },
        Line::Size => look.size.as_deref().map_or_else(
            || words::from_conf(&size_name(&top(shared, "font-size").unwrap_or_default())),
            size_name,
        ),
        Line::Palette => look
            .palette
            .clone()
            .unwrap_or_else(|| words::from_conf(&top(shared, "palette").unwrap_or_default())),
        Line::Vtlook => match value(shared, look, line) {
            Some(name) if name == vtium_settings::vtlook::DEFAULT => words::NO_FILTER.to_owned(),
            Some(name) => name,
            None => top_vtlook(shared).map_or_else(
                || words::NO_FILTER.to_owned(),
                |name| words::from_conf(&name),
            ),
        },
        Line::Pointer => match &look.pointer {
            Some(own) => pointer_value_name(own),
            None => words::from_conf(
                &top(shared, "pointer-theme")
                    .as_deref()
                    .map_or_else(|| words::POINTER_OWN.to_owned(), pointer_value_name),
            ),
        },
    }
}

/// The change that sets `line`'s key of the look `name` to `value`, or
/// unsets it with `None`.
pub fn pick(name: &str, line: Line, value: Option<String>) -> Vec<Command> {
    let Some(key) = line.key() else {
        return Vec::new();
    };
    let change = Change::new(
        words::change_name(key, value.as_deref(), name),
        Edit::Conf(vec![ConfEdit::LookKey {
            name: name.to_owned(),
            key,
            value,
        }]),
    );
    vec![Command::Change(UiChange::Document(change))]
}

/// Left or Right on `line`: the change that steps it `by` one choice, or
/// nothing at either end or on a line with no choices.
pub fn step(shared: &Shared, look: &Look, line: Line, by: isize) -> Vec<Command> {
    let c = choices(shared, look, line);
    let Some(last) = c.choices.len().checked_sub(1) else {
        return Vec::new();
    };
    let to = match c.current {
        Some(at) => at.saturating_add_signed(by).min(last),
        None => 0,
    };
    if Some(to) == c.current {
        return Vec::new();
    }
    pick(&look.name, line, c.keep(to))
}

/// The change that renames the look `old` to the typed `text`, with every
/// terminal that draws it, or why the document refuses the name. A name
/// left as it was makes no change.
pub fn rename(shared: &Shared, old: &str, text: &str) -> Result<Vec<Command>, String> {
    let new = text.trim();
    if new == old {
        return Ok(Vec::new());
    }
    let edit = Edit::Conf(vec![ConfEdit::LookRename {
        name: old.to_owned(),
        new: new.to_owned(),
    }]);
    let document = shared.document();
    document
        .content()
        .edited(&edit, document.installed())
        .map(|_| {
            vec![Command::Change(UiChange::Document(Change::new(
                words::renamed(old, new),
                edit,
            )))]
        })
}
