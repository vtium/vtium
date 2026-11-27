//! A look's card: its name, font set, size, color scheme, vtlook and
//! pointer theme, one line each, and below them the fonts of its font set
//! by role and what they still miss. Enter on a line shows the line's
//! choices as a list, Left and Right step the line through them, and Enter
//! on the name lets the person type a new one. Each change goes into the
//! draft at once and waits for Save, as every change does, so Esc closes
//! the card and keeps them.
//!
//! The first choice of the font set, size and color scheme lines leaves
//! the look's own key unset, so the look takes vtium.conf's value. A look
//! with no `vtlook` line draws the vtlook that vtium.conf's top level
//! sets, and the built-in one, No filter, where the top level sets none.
//! So where the top level sets a vtlook, the vtlook list starts with it,
//! which leaves the key unset, and No filter writes `vtlook = default`.
//! Where it sets none, No filter comes first and leaves the key unset.
//! The other vtlooks follow in the Effects tab's order, and a vtlook that
//! does not read shows why and can still be picked: the terminal then
//! draws No filter until the file is fixed. The sizes come from the look's
//! text font, as the font helper described it for the Fonts tab: a bitmap
//! font offers the point sizes that draw each of its strikes, and an
//! outline font, or a font not described yet, the usual sizes. A value the
//! look sets that the choices lack, such as a color scheme file or a font
//! set that does not read, is listed too, so stepping from it never drops
//! the look's key. The pointer theme line offers vtium.conf's theme first,
//! which leaves the look's `pointer-theme` unset, then each active pointer
//! theme, so a look picks only a theme that passed the trust rule.
//!
//! References:
//! - ADR-1205 D6: a look is assembled by role, and picks its effects
//!   through its vtlook.
//! - ADR-1205 D7: a look offers the sizes its fonts supply.
//! - ADR-1205 D8: within a look, what it still misses, by role.
//! - ADR-1205 D10: a look picks its own color scheme, the built-in schemes
//!   or an active scheme file, and vtium.conf's `palette` stays the default.
//! - ADR-1403 D10, the row `list.palettes`: the scheme vtium.conf sets,
//!   then the built-in schemes, then the active scheme files.
//! - ADR-1304 D4, its note of 2026-11-19: No filter first and chosen for a
//!   new look, and a vtlook that does not read may still be picked.
//! - ADR-1304 D5: the look's vtlook wins, then the top level's, then the
//!   built-in one, which `vtlook = default` names.
//! - ADR-1205 D6: each look picks one active pointer theme, and
//!   vtium.conf's `pointer-theme` stays the default for a look that picks
//!   none.
//! - ADR-1403 D10: the card, its shared list and Esc keeping its changes.

use super::super::super::canvas::Row;
use super::super::super::edit::Change as DraftChange;
use super::super::super::effects_found::From;
use super::super::super::fonts_found::{Supplied, sizes_density, strike_points, supplied_by};
use super::super::super::input::Key;
use super::super::super::listbox::{Act, Body, Item, ListBox, Marks, rows, tail};
use super::super::super::model::{Font, FontState, SIZES, Shared, Size};
use super::super::super::words;
use super::super::{Change, Command};
use super::list::{self, Look};

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
    fn key(self) -> Option<&'static str> {
        match self {
            Line::FontSet => Some("font-name"),
            Line::Size => Some("font-size"),
            Line::Palette => Some("palette"),
            Line::Vtlook => Some("vtlook"),
            Line::Pointer => Some("pointer-theme"),
            Line::Name => None,
        }
    }
}

/// One choice of a line: what the list shows, the look's value it writes,
/// `None` to leave the key unset, and a note beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    pub value: Option<String>,
    pub note: String,
}

impl Choice {
    fn new(name: impl Into<String>, value: Option<String>, note: impl Into<String>) -> Choice {
        Choice {
            name: name.into(),
            value,
            note: note.into(),
        }
    }
}

/// What is open over the card's lines.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Open {
    /// The look's name being typed.
    Typing(String),
    /// The highlighted line's choices, as the shared list.
    List(ListBox),
}

/// A look's card: the look it shows, by name, the highlighted line and
/// what is open over the lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    name: String,
    line: usize,
    open: Option<Open>,
}

/// What a key did to the card: kept it open, with what update does for
/// it, or closed it.
pub enum Step {
    Stay(Vec<Command>),
    Close,
}

/// vtium.conf's top-level value of `key` as the card shows it: the
/// draft's value, else the key's default, or `None` for a key whose
/// default is to be unset.
fn top(shared: &Shared, key: &str) -> Option<String> {
    let k = vtium_settings::keys::find(key)?;
    match (shared.value(k), k.default) {
        ((_, true), vtium_settings::keys::Default::Unset) => None,
        ((value, _), _) => Some(value),
    }
}

/// The sizes the text font of the font set `look` draws supplies.
///
/// References:
/// - ADR-1205 D7: a look offers the sizes its fonts supply.
fn supplied(shared: &Shared, look: &Look) -> Supplied {
    let name = look.frit.clone().or_else(|| top(shared, "font-name"));
    supplied_by(shared, name.as_deref())
}

/// The vtlook that vtium.conf's top level sets, where it names one other
/// than the built-in one, which every look with no `vtlook` line draws.
fn top_vtlook(shared: &Shared) -> Option<String> {
    shared
        .drafted("vtlook")
        .filter(|v| v != vtium_settings::vtlook::DEFAULT)
}

/// The note of a vtlook that does not read, empty for one that reads.
fn vtlook_note(shared: &Shared, name: &str) -> Option<String> {
    match shared.inputs.effects.vtlook(name).map(|v| &v.read) {
        Some(Err(why)) => Some(words::vtlook_broken(why)),
        _ => None,
    }
}

/// A size as the card shows it: automatic for 0, else in points.
fn size_name(value: &str) -> String {
    match value.parse::<u32>() {
        Ok(0) => words::AUTOMATIC.to_owned(),
        Ok(n) => words::points(n),
        Err(_) => value.to_owned(),
    }
}

/// The choices of `line` for the look `look`, in the list's order. The
/// look's own value is always among them: where the usual choices lack
/// it, it joins them with a note that says so. The font set line offers
/// the font sets the draft sees, so a font set the draft adds can be
/// picked before the install, and one it removes cannot.
///
/// References:
/// - ADR-1403 D8, "The frits a draft sees": a look card offers them for `font-name`.
pub fn choices(shared: &Shared, look: &Look, line: Line) -> Vec<Choice> {
    let mut out = match line {
        Line::FontSet => {
            let first = match top(shared, "font-name") {
                Some(name) => Choice::new(words::top_choice(words::TOP_FRIT, name), None, ""),
                None => Choice::new(
                    words::top_choice(words::TOP_FRIT, words::TOP_NONE),
                    None,
                    words::FRIT_NONE_NOTE,
                ),
            };
            // the font sets the draft sees: the installed ones less those
            // the draft removes, then those only the draft adds, marked so
            let removed = &shared.frits().removed;
            let mut out = vec![first];
            out.extend(
                super::fontsets::all(shared)
                    .into_iter()
                    .filter(|f| super::fontsets::usable(shared, f) && !removed.contains(&f.name))
                    .map(|f| {
                        // a set whose text font is left out draws its text
                        // in vtium's built-in font, which the list says
                        let note = match super::fontsets::text_font_left_out(shared, &f.name) {
                            _ if super::fontsets::only_drafted(shared, &f.name) => words::IN_DRAFT,
                            Some(super::fontsets::TextFont::Missing(_)) => {
                                words::CHOICE_TEXT_MISSING
                            }
                            Some(super::fontsets::TextFont::Inactive(_)) => {
                                words::CHOICE_TEXT_INACTIVE
                            }
                            Some(super::fontsets::TextFont::Refused(..)) => {
                                words::CHOICE_TEXT_REFUSED
                            }
                            None => "",
                        };
                        Choice::new(f.name.clone(), Some(f.name), note)
                    }),
            );
            out
        }
        Line::Size => {
            let top = top(shared, "font-size").unwrap_or_default();
            let mut out = vec![
                Choice::new(
                    words::top_choice(words::TOP_SIZE, size_name(&top)),
                    None,
                    "",
                ),
                Choice::new(words::AUTOMATIC, Some("0".to_owned()), ""),
            ];
            // a bitmap text font offers the sizes that draw its strikes,
            // and any other the usual sizes
            let mut sizes: Vec<(u32, String, &str)> = match supplied(shared, look) {
                Supplied::Strikes { strikes, .. } => {
                    strike_points(&strikes, sizes_density(shared).0)
                        .iter()
                        .map(|(n, strike)| (*n, words::strike_size(*n, strike), ""))
                        .collect()
                }
                _ => SIZES.iter().map(|n| (*n, words::points(*n), "")).collect(),
            };
            // a size the look sets outside those takes its place among
            // them, so Left and Right step to its neighbours
            if let Some(n) = look.size.as_deref().and_then(|v| v.parse::<u32>().ok())
                && n > 0
                && !sizes.iter().any(|(s, _, _)| *s == n)
            {
                sizes.push((n, words::points(n), words::CHOICE_OWN));
                sizes.sort_unstable_by_key(|(n, _, _)| *n);
            }
            out.extend(
                sizes
                    .into_iter()
                    .map(|(n, name, note)| Choice::new(name, Some(n.to_string()), note)),
            );
            out
        }
        Line::Palette => {
            let top = top(shared, "palette").unwrap_or_default();
            let mut out = vec![Choice::new(
                words::top_choice(words::TOP_PALETTE, top),
                None,
                "",
            )];
            out.extend(
                vtium_term::PALETTE_NAMES
                    .iter()
                    .map(|n| Choice::new(*n, Some((*n).to_owned()), words::PALETTE_BUILT_IN)),
            );
            out.push(Choice::new(
                words::PALETTE_CUSTOM,
                Some(words::PALETTE_CUSTOM.to_owned()),
                words::PALETTE_CUSTOM_NOTE,
            ));
            // then the active scheme files, each once by the name a look
            // gives it: a packaged file that the administrator's file
            // hides is the same name, a file that takes a built-in
            // scheme's name is never read, and a name a look cannot name,
            // such as one with a space, is not offered
            for f in &shared.inputs.effects.palettes {
                let r = &f.resource;
                let nameable = vtium_settings::palette::named(&f.name)
                    && !vtium_settings::palette::built_in(&f.name);
                if r.active() && !r.hidden && nameable {
                    let note = match &f.colors {
                        Ok(_) => words::PALETTE_FILE_NOTE.to_owned(),
                        Err(why) => words::palette_unread(why),
                    };
                    out.push(Choice::new(f.name.clone(), Some(f.name.clone()), note));
                }
            }
            out
        }
        Line::Vtlook => {
            let top = top_vtlook(shared);
            let mut out = Vec::new();
            if let Some(name) = &top {
                let note = vtlook_note(shared, name).unwrap_or_default();
                out.push(Choice::new(
                    words::top_choice(words::TOP_VTLOOK, name),
                    None,
                    note,
                ));
            }
            out.extend(
                shared
                    .inputs
                    .effects
                    .vtlooks
                    .iter()
                    .filter(|v| !v.hidden)
                    .map(|v| {
                        let mut notes = vec![match v.from {
                            From::BuiltIn => words::VTLOOK_BUILT_IN.to_owned(),
                            From::Preset => words::VTLOOK_PRESET.to_owned(),
                            From::Administrator => words::VTLOOK_ADMIN.to_owned(),
                        }];
                        notes.extend(vtlook_note(shared, &v.name));
                        match v.from {
                            // No filter leaves the key unset where that
                            // draws the built-in vtlook, and else names it
                            From::BuiltIn => Choice::new(
                                words::NO_FILTER,
                                top.as_ref()
                                    .map(|_| vtium_settings::vtlook::DEFAULT.to_owned()),
                                notes.join(", "),
                            ),
                            _ => {
                                Choice::new(v.name.clone(), Some(v.name.clone()), notes.join(", "))
                            }
                        }
                    }),
            );
            out
        }
        Line::Pointer => {
            // vtium.conf's theme first, which leaves the look's key unset,
            // then each active theme, vtium's own pictures first as the
            // list of pointer themes holds them
            let top = top(shared, "pointer-theme");
            let mut out = vec![Choice::new(
                words::top_choice(
                    words::TOP_POINTER,
                    top.as_deref().unwrap_or(words::POINTER_OWN),
                ),
                None,
                "",
            )];
            out.extend(
                shared
                    .inputs
                    .effects
                    .pointers
                    .iter()
                    .filter(|r| r.active() && !r.hidden)
                    .map(|r| {
                        let path = r.path.to_string_lossy().into_owned();
                        Choice::new(
                            crate::onscreen::text(&r.name()),
                            Some(path),
                            words::POINTER_ACTIVE_NOTE,
                        )
                    }),
            );
            out
        }
        Line::Name => return Vec::new(),
    };
    let now = value(shared, look, line);
    if now.is_some() && !out.iter().any(|c| c.value == now) {
        let name = now.clone().unwrap_or_default();
        let note = match line {
            Line::FontSet => own_font_set_note(shared, &name),
            _ => words::CHOICE_OWN.to_owned(),
        };
        out.push(Choice::new(name, now, note));
    }
    out
}

/// The note of a font set that the look names and the usual choices lack:
/// one the console does not read, one the draft removes, or one that
/// neither this machine nor the draft has.
fn own_font_set_note(shared: &Shared, name: &str) -> String {
    if shared.refused.iter().any(|n| n == name) {
        return words::choice_refused(words::KEPT);
    }
    if shared.frits().removed.contains(name) {
        return words::CHOICE_REMOVED.to_owned();
    }
    match super::fontsets::all(shared)
        .into_iter()
        .find(|f| f.name == name)
    {
        Some(Font {
            state: FontState::Refused(why),
            ..
        }) => words::choice_refused(why),
        Some(_) => words::CHOICE_OWN.to_owned(),
        None => words::CHOICE_ABSENT.to_owned(),
    }
}

/// The look's value of `line`'s key, `None` where the look leaves it to
/// vtium.conf. Where vtium.conf's top level sets no vtlook, a look that
/// names the built-in vtlook `default` draws what a look that names none
/// draws, so its value is `None` too.
fn value(shared: &Shared, look: &Look, line: Line) -> Option<String> {
    match line {
        Line::FontSet => look.frit.clone(),
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

/// The size of the size line's choice `pick` in points, 0 for automatic.
/// The first choice leaves the look's size to vtium.conf, so it stands for
/// vtium.conf's size.
fn size_points(shared: &Shared, look: &Look, pick: usize) -> Option<u32> {
    let choice = choices(shared, look, Line::Size).into_iter().nth(pick)?;
    Some(
        choice
            .value
            .or_else(|| top(shared, "font-size"))
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0),
    )
}

/// The place of the look's value among `line`'s choices, where they have
/// it.
fn chosen(shared: &Shared, look: &Look, line: Line) -> Option<usize> {
    let now = value(shared, look, line);
    choices(shared, look, line)
        .iter()
        .position(|c| c.value == now)
}

/// What `line` shows for the look: its value, or vtium.conf's where the
/// look leaves the key unset, with a note where the value needs one.
fn shown(shared: &Shared, look: &Look, line: Line) -> String {
    match line {
        Line::Name => look.name.clone(),
        Line::FontSet => match (&look.frit, top(shared, "font-name")) {
            (Some(name), _) => name.clone(),
            (None, Some(top)) => words::from_top(top),
            (None, None) => words::FRIT_NONE.to_owned(),
        },
        Line::Size => {
            let size = look.size.as_deref().map_or_else(
                || words::from_top(size_name(&top(shared, "font-size").unwrap_or_default())),
                size_name,
            );
            let supplied = supplied(shared, look);
            let (dpi, mixed) = sizes_density(shared);
            let offered = match &supplied {
                Supplied::Strikes { strikes, .. } => strike_points(strikes, dpi),
                _ => Vec::new(),
            };
            format!("{size}. {}", words::sizes_note(&supplied, &offered, mixed))
        }
        Line::Palette => look
            .palette
            .clone()
            .unwrap_or_else(|| words::from_top(top(shared, "palette").unwrap_or_default())),
        Line::Vtlook => {
            let (name, from_top) = match value(shared, look, line) {
                Some(name) if name == vtium_settings::vtlook::DEFAULT => {
                    return words::NO_FILTER.to_owned();
                }
                Some(name) => (name, false),
                None => match top_vtlook(shared) {
                    Some(name) => (name, true),
                    None => return words::NO_FILTER.to_owned(),
                },
            };
            let shown = if from_top {
                words::from_top(&name)
            } else {
                name.clone()
            };
            match vtlook_note(shared, &name) {
                Some(note) => format!("{shown}, {note}"),
                None => shown,
            }
        }
        Line::Pointer => match &look.pointer {
            Some(own) => words::pointer_line(Some(own), false),
            None => words::pointer_line(top(shared, "pointer-theme").as_deref(), true),
        },
    }
}

impl Card {
    /// The card of the look `name`, on its first line.
    pub fn new(name: &str) -> Card {
        Card {
            name: name.to_owned(),
            line: 0,
            open: None,
        }
    }

    /// The card of the new look `name`, with its name being typed.
    pub fn typing(name: &str) -> Card {
        Card {
            open: Some(Open::Typing(name.to_owned())),
            ..Card::new(name)
        }
    }

    /// The name of the look the card shows.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The highlighted line.
    pub fn line(&self) -> Line {
        Line::ALL.get(self.line).copied().unwrap_or(Line::Name)
    }

    /// The node of the interface tree the card is at.
    pub fn node(&self) -> &'static str {
        match self.open {
            None => "looks.look",
            Some(Open::Typing(_)) => "looks.look.typing",
            Some(Open::List(_)) => "looks.look.list",
        }
    }

    /// Whether the look's name is being typed.
    pub fn is_typing(&self) -> bool {
        matches!(self.open, Some(Open::Typing(_)))
    }

    /// The name as it is typed, while it is.
    pub fn typed(&self) -> Option<&str> {
        match &self.open {
            Some(Open::Typing(text)) => Some(text),
            _ => None,
        }
    }

    /// A character or Backspace typed into the name.
    pub fn type_key(&mut self, key: Key) {
        if let Some(Open::Typing(text)) = &mut self.open {
            super::super::type_into(text, key);
        }
    }

    /// The card's body on a screen of `size`: its lines, or the open
    /// line's choices, and under them the fonts by role and what they
    /// miss, or the sentence that says what the keys do.
    pub fn body(&self, shared: &Shared, size: Size) -> Body {
        let cols = size.cols;
        let Some(look) = list::look(shared, &self.name) else {
            return Body {
                list: Vec::new(),
                keep: 0,
                tail: tail(&words::look_gone(&self.name), cols),
            };
        };
        if let Some(Open::List(lb)) = &self.open {
            let (list, keep) = self.choice_rows(shared, &look, lb, cols);
            return Body {
                list,
                keep,
                tail: tail(words::CARD_LIST_HINT, cols),
            };
        }
        let items: Vec<Item> = Line::ALL
            .iter()
            .zip(words::CARD_LINES)
            .map(|(line, name)| {
                let value = match (&self.open, line) {
                    (Some(Open::Typing(text)), Line::Name) => format!("{text}_"),
                    _ => shown(shared, &look, *line),
                };
                Item::new(name, value)
            })
            .collect();
        let (list, keep) = rows(&items, self.line, Marks::None, cols);
        let mut sentences = vec![if self.is_typing() {
            words::CARD_TYPING_HINT.to_owned()
        } else {
            words::CARD_HINT.to_owned()
        }];
        // on the size line, a vtium console that does not let programs
        // resize its grid takes no size live, and the card says why; with
        // the live trial on, the preview is the console's own text at its
        // current size, so a size waits for vtium's next start
        //
        // References:
        // - ADR-1201 D4, "Sizes, live on a vtium console", its note of 2026-11-21: the console's own text decides over the picture.
        if self.line() == Line::Size
            && shared.in_vtium()
            && !shared.window_ops()
            && !shared.window_ops_drafted()
        {
            sentences.push(
                if shared.text_preview() {
                    words::WINDOW_OPS_NOTICE_NEXT_START
                } else {
                    words::WINDOW_OPS_NOTICE
                }
                .to_owned(),
            );
        }
        sentences.push(words::ROLES_HEADING.to_owned());
        match look.frit.clone().or_else(|| shared.drafted("font-name")) {
            Some(frit) => {
                sentences.extend(super::fontsets::roles(shared, &frit));
                sentences.push(words::MISSES_HEADING.to_owned());
                sentences.extend(super::fontsets::missing(shared, &frit));
            }
            // vtium-config cannot read vconsole.conf's font, so it knows
            // nothing that the font misses
            None => sentences.push(words::role_line(words::ROLE_TEXT, words::ROLE_TEXT_NONE)),
        }
        let mut out = Vec::new();
        for s in sentences {
            out.extend(
                tail(&s, cols)
                    .into_iter()
                    .skip(usize::from(!out.is_empty())),
            );
        }
        Body {
            list,
            keep,
            tail: out,
        }
    }

    /// The rows of the open list of the highlighted line's choices, the
    /// look's value ticked, and the last row of the highlighted choice.
    fn choice_rows(
        &self,
        shared: &Shared,
        look: &Look,
        lb: &ListBox,
        cols: usize,
    ) -> (Vec<Row>, usize) {
        let now = value(shared, look, self.line());
        let items: Vec<Item> = choices(shared, look, self.line())
            .into_iter()
            .map(|c| Item {
                on: c.value == now,
                ..Item::new(c.name, c.note)
            })
            .collect();
        rows(&items, lb.at, Marks::Choice, cols)
    }

    /// While the size line's list is open: its rows on a screen `cols`
    /// wide, the last row of the highlighted size, and that size in
    /// points, 0 for automatic.
    pub fn size_list(&self, shared: &Shared, cols: usize) -> Option<(Vec<Row>, usize, u32)> {
        let Some(Open::List(lb)) = &self.open else {
            return None;
        };
        if self.line() != Line::Size {
            return None;
        }
        let look = list::look(shared, &self.name)?;
        let points = size_points(shared, &look, lb.at)?;
        let (list, keep) = self.choice_rows(shared, &look, lb, cols);
        Some((list, keep, points))
    }

    /// The change that sets `line`'s key of the look to the choice `pick`.
    fn pick(&self, shared: &Shared, look: &Look, line: Line, pick: usize) -> Vec<Command> {
        let (Some(key), Some(choice)) = (
            line.key(),
            choices(shared, look, line).into_iter().nth(pick),
        ) else {
            return Vec::new();
        };
        let mut changes = vec![DraftChange::LookKey {
            name: self.name.clone(),
            key,
            value: choice.value.clone(),
        }];
        // a font set or a size for a look that no terminal uses reaches
        // the console only once the console's terminal uses the look, so
        // a console whose terminal names no look takes this one
        let head = (matches!(line, Line::FontSet | Line::Size)
            && choice.value.is_some()
            && self.name != words::DEFAULT_LOOK
            && list::heads(shared, &self.name).is_empty())
        .then(|| list::console_head(shared))
        .flatten()
        .filter(|h| list::names_no_look(shared, h));
        let Some(head) = head else {
            return vec![Command::Change(Change::Make {
                changes,
                select: None,
            })];
        };
        changes.push(DraftChange::HeadKey {
            name: head.clone(),
            key: "look",
            value: Some(self.name.clone()),
        });
        vec![
            Command::Change(Change::Make {
                changes,
                select: None,
            }),
            Command::Change(Change::Say(words::look_given(&head, &self.name))),
        ]
    }

    /// Enter on the size `pick` of the size line's list: the size tried on
    /// the console's grid, in points, where the console can take it. The
    /// first choice leaves the look's size to vtium.conf, so it tries
    /// vtium.conf's size.
    ///
    /// References:
    /// - ADR-1201 D4, "Sizes, live on a vtium console": Enter applies the
    ///   highlighted size to the console as a timed trial.
    fn try_size(&self, shared: &Shared, look: &Look, pick: usize) -> Vec<Command> {
        let Some(points) = size_points(shared, look, pick) else {
            return Vec::new();
        };
        vec![Command::Change(Change::TrySize {
            font: self.font_set(shared),
            points,
        })]
    }

    /// The values the highlighted line's choices write, in the list's
    /// order, and the look's value of the line's key now, `None` where it
    /// leaves the key unset. A line with no choices has no values.
    pub fn values(&self, shared: &Shared) -> (Vec<Option<String>>, Option<String>) {
        let Some(look) = list::look(shared, &self.name) else {
            return (Vec::new(), None);
        };
        let line = self.line();
        (
            choices(shared, &look, line)
                .into_iter()
                .map(|c| c.value)
                .collect(),
            value(shared, &look, line),
        )
    }

    /// The font set the look draws, its own or vtium.conf's, for `t`.
    pub fn font_set(&self, shared: &Shared) -> Option<String> {
        list::look(shared, &self.name)
            .and_then(|l| l.frit)
            .or_else(|| shared.drafted("font-name"))
    }

    /// A key the navigation module hands to the card. While the name is
    /// typed, Enter renames the look and Esc keeps the name it had. While
    /// a list is open, Enter picks and Esc closes it. Otherwise Up and Down
    /// move between the lines, Left and Right step the line through its
    /// choices, Enter opens the line's list or types the name, and Esc
    /// closes the card.
    pub fn update(&mut self, shared: &Shared, key: Key) -> Step {
        let Some(look) = list::look(shared, &self.name) else {
            return Step::Close;
        };
        let line = self.line();
        match self.open.take() {
            Some(Open::Typing(text)) => Step::Stay(match key {
                Key::Escape => Vec::new(),
                Key::Enter => self.rename(shared, text),
                _ => {
                    self.open = Some(Open::Typing(text));
                    Vec::new()
                }
            }),
            Some(Open::List(mut lb)) => {
                let n = choices(shared, &look, line).len();
                Step::Stay(match lb.key(key, n) {
                    Act::Picked(i) if line == Line::Size => {
                        let mut out = self.pick(shared, &look, line, i);
                        out.extend(self.try_size(shared, &look, i));
                        out
                    }
                    Act::Picked(i) => self.pick(shared, &look, line, i),
                    Act::Closed => Vec::new(),
                    _ => {
                        self.open = Some(Open::List(lb));
                        Vec::new()
                    }
                })
            }
            None => match key {
                Key::Escape => Step::Close,
                Key::Up | Key::Down => {
                    self.line = super::super::super::listbox::step(self.line, Line::ALL.len(), key);
                    Step::Stay(Vec::new())
                }
                Key::Enter if line == Line::Name => {
                    self.open = Some(Open::Typing(self.name.clone()));
                    Step::Stay(Vec::new())
                }
                Key::Enter if !choices(shared, &look, line).is_empty() => {
                    let at = chosen(shared, &look, line).unwrap_or(0);
                    self.open = Some(Open::List(ListBox::at(at)));
                    Step::Stay(Vec::new())
                }
                Key::Left | Key::Right => {
                    let n = choices(shared, &look, line).len();
                    if n == 0 {
                        return Step::Stay(Vec::new());
                    }
                    let at = chosen(shared, &look, line);
                    let to = match (key, at) {
                        (Key::Left, Some(at)) => at.saturating_sub(1),
                        (_, Some(at)) => at.saturating_add(1).min(n.saturating_sub(1)),
                        (_, None) => 0,
                    };
                    if Some(to) == at {
                        return Step::Stay(Vec::new());
                    }
                    Step::Stay(self.pick(shared, &look, line, to))
                }
                _ => Step::Stay(Vec::new()),
            },
        }
    }

    /// Enter on the typed name: the look takes the name `text`, with every
    /// head that draws it, once the draft takes the change. A name the
    /// draft refuses stays typed, and the message line says why.
    fn rename(&mut self, shared: &Shared, text: String) -> Vec<Command> {
        let new = text.trim().to_owned();
        if new == self.name {
            return Vec::new();
        }
        let change = DraftChange::LookRename {
            name: self.name.clone(),
            new: new.clone(),
        };
        match shared.apply(std::slice::from_ref(&change)) {
            Ok(_) => {
                self.name = new;
                vec![Command::Change(Change::Make {
                    changes: vec![change],
                    select: None,
                })]
            }
            Err(why) => {
                self.open = Some(Open::Typing(text));
                vec![Command::Change(Change::Say(why))]
            }
        }
    }
}
