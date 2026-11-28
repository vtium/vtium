//! The highlighted row's facts, beside the list: a font's file, folder,
//! kind, coverage, sizes, state and trust, where its activation comes
//! from, a duplicate mark and the looks that use it. On the filter line
//! the facts say what the filters do, and on the configuration's own row
//! they list what Activate missing would activate and what it leaves.
//!
//! Each fact is one line, cut where the pane is narrower than the fact,
//! and F1 opens the facts uncut. The same lines serve the pane and F1, so
//! the two never disagree.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Fonts bullet: the facts
//!   beside the list, and moving through the fonts shows each one's
//!   facts without Enter.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated":
//!   the marks, and the fonts Activate missing leaves, named apart.
//! - ADR-1205 D4, as amended on 2026-11-27: the line that says which
//!   list activates a package's font.
//! - crates/AGENTS.md, "Strings on screen", rule 7: one line where a
//!   paragraph was, the rest behind F1.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::batch;
use vtium_config_core::discover::Described;
use vtium_config_core::fontuse::State;
use vtium_config_core::ui::Shared;
use vtium_config_widgets::canvas;
use vtium_config_widgets::roles::Role;

use crate::list::{Sel, put_spans};
use crate::states::{Item, Known};
use crate::words;

/// One line of the facts: its spans, each with its role.
pub type FactLine = Vec<(String, Role)>;

/// A labelled fact: the label padded to its column, then the value.
fn fact(label: &str, value: impl Into<String>) -> FactLine {
    vec![
        (format!("{label:<width$}", width = words::LABEL), Role::Dim),
        (value.into(), Role::Row),
    ]
}

/// A line that goes on from the fact above it, under its value.
fn more(value: impl Into<String>, role: Role) -> FactLine {
    vec![(" ".repeat(words::LABEL), Role::Row), (value.into(), role)]
}

/// The mark of `state` as a fact.
fn state_fact(state: &State) -> FactLine {
    let mut line = vec![(
        format!("{:<width$}", words::STATE, width = words::LABEL),
        Role::Dim,
    )];
    line.extend(state.mark().into_iter().map(|(w, r)| (w.to_owned(), r)));
    line
}

/// The facts of `sel`.
pub fn lines(known: &Known, shared: &Shared, sel: Sel, width: usize) -> Vec<FactLine> {
    match sel {
        Sel::Filter => {
            let fonts = &shared.found.fonts;
            let mut out: Vec<FactLine> = Vec::new();
            for l in words::filter_facts(fonts.files().len(), fonts.waiting().count()) {
                // the one sentence on the filter line wraps, as nothing
                // else holds it
                if l == words::FILTER_LEAVES_OUT {
                    for part in canvas::wrap(&l, width.max(8), usize::MAX) {
                        out.push(vec![(part, Role::Row)]);
                    }
                } else {
                    out.push(vec![(l, Role::Row)]);
                }
            }
            if let Some(note) = &fonts.note {
                out.push(Vec::new());
                out.push(vec![(note.clone(), Role::Warning)]);
            }
            out
        }
        Sel::Activate => activate(known),
        Sel::Item(item) => font(known, shared, item),
    }
}

/// What Activate missing activates, what it leaves off on purpose, and
/// what it cannot activate, with why.
fn activate(known: &Known) -> Vec<FactLine> {
    let (mark, role) = vtium_config_core::fontuse::NOT_ALL_ACTIVE;
    let mut out: Vec<FactLine> = vec![vec![(mark.to_owned(), role)], Vec::new()];
    let mut group = |title: &str, fonts: Vec<FactLine>| {
        if fonts.is_empty() {
            return;
        }
        out.push(vec![(title.to_owned(), Role::Heading)]);
        out.extend(fonts);
        out.push(Vec::new());
    };
    let named = |pick: &dyn Fn(&State) -> bool| -> Vec<FactLine> {
        known
            .used
            .fonts
            .iter()
            .filter(|f| pick(&f.state))
            .map(|f| {
                let name = f.name();
                let name = name.strip_prefix("font:").unwrap_or(&name);
                let mut line = vec![(format!("  {name}"), Role::Row)];
                if let Some(why) = f.state.reason() {
                    line.push((format!(": {why}"), Role::Dim));
                }
                line
            })
            .collect()
    };
    group(words::ACTIVATES, named(&|s: &State| *s == State::NotActive));
    group(
        words::STAYS_OFF,
        named(&|s: &State| matches!(s, State::TurnedOff(_))),
    );
    group(words::CANNOT, named(&|s: &State| s.reason().is_some()));
    if known.counts.not_active > 0 {
        out.push(vec![(words::ACTIVATE_FOOT.to_owned(), Role::Dim)]);
    }
    out
}

/// A font's facts.
fn font(known: &Known, shared: &Shared, item: Item) -> Vec<FactLine> {
    let Some(at) = known.position(item) else {
        return Vec::new();
    };
    let state = known.state(at);
    let users = known.users(shared, item);
    let mut out: Vec<FactLine> = Vec::new();
    match item {
        Item::File(i) => {
            let Some(f) = shared.found.fonts.files().get(i) else {
                return out;
            };
            let name = f.resource.name();
            let title = f.faces().first().map_or_else(
                || name.clone(),
                |face| words::face_name(&face.family, &face.style),
            );
            out.push(vec![(title, Role::Heading)]);
            out.push(Vec::new());
            out.push(fact(words::FILE, name));
            let folder = f
                .path()
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            out.push(fact(words::FOLDER, folder));
            match &f.described {
                None => out.push(vec![(words::DESCRIBING.to_owned(), Role::Dim)]),
                Some(Described::Font(d)) => {
                    if let Some(face) = d.faces.first() {
                        let mut kind = words::drawn(face.outline, face.monospaced);
                        for g in &face.groups {
                            kind.push_str(", ");
                            kind.push_str(g.name());
                        }
                        out.push(fact(words::KIND, kind));
                        out.push(fact(words::COVERS, words::covers(&face.scripts)));
                        let strikes: Vec<String> = face
                            .strikes
                            .iter()
                            .map(|s| words::strike(s.width, s.height))
                            .collect();
                        out.push(fact(words::SIZES, words::sizes(face.outline, &strikes)));
                    }
                }
                Some(Described::NotAFont(why) | Described::Unknown(why)) => {
                    out.push(vec![(words::not_described(why), Role::Dim)]);
                }
            }
            if let Some(state) = state {
                out.push(state_fact(state));
                match state {
                    State::FailsTrust { own: true, .. } => {
                        out.push(more(words::OWN_STATE, Role::Row));
                    }
                    State::FailsTrust { check, .. } => {
                        out.push(fact(words::TRUST, words::fails(check)));
                    }
                    _ => out.push(fact(words::TRUST, words::TRUSTED)),
                }
            }
            let changed = shared.document().content().active.get(f.path()).is_some();
            if changed {
                out.push(more("changed, not saved", Role::NotSaved));
            } else if let Some(from) = batch::activated_by(&f.resource.origin) {
                // the state line already says active or not
                let from = from
                    .strip_prefix("Active: ")
                    .or_else(|| from.strip_prefix("Not active: "))
                    .map_or_else(|| from.clone(), str::to_owned);
                out.push(fact(words::FROM, from));
            }
            if let Some(dup) = &f.duplicate {
                out.push(fact(words::DUPLICATE, dup.words()));
            }
            out.push(fact(words::USED_BY, words::users(users, 3)));
            let hint = match state {
                Some(State::FailsTrust { own: true, .. }) => Some(words::OWN_HINT),
                Some(State::NotActive) if !users.is_empty() => Some(words::ACTIVATE_HINT),
                Some(State::NotActive | State::TurnedOff(_)) => Some(words::SPACE_HINT),
                _ => None,
            };
            if let Some(hint) = hint {
                out.push(Vec::new());
                out.push(vec![(hint.to_owned(), Role::Dim)]);
            }
        }
        Item::BuiltIn(name) => {
            out.push(vec![(name.to_owned(), Role::Heading)]);
            out.push(Vec::new());
            out.push(fact(words::KIND, words::BUILT_IN_KIND));
            let mut line = vec![(
                format!("{:<width$}", words::STATE, width = words::LABEL),
                Role::Dim,
            )];
            line.push((words::ALWAYS.0.to_owned(), words::ALWAYS.1));
            out.push(line);
            out.push(more(words::BUILT_IN_STATE, Role::Row));
        }
        Item::Named(u) => {
            let Some(font) = known.used.fonts.get(u) else {
                return out;
            };
            let title = known
                .fields
                .get(at)
                .map_or_else(|| font.name(), |f| f[0].clone());
            out.push(vec![(title, Role::Heading)]);
            out.push(Vec::new());
            out.push(fact(words::NAMED, font.named.clone()));
            if let Some(path) = &font.path {
                out.push(fact(words::FILE, path.display().to_string()));
            }
            out.push(state_fact(&font.state));
            if let Some(why) = font.state.reason() {
                out.push(more(why, Role::Row));
            }
            out.push(fact(words::USED_BY, words::users(users, 3)));
            if font.state == State::Missing {
                out.push(Vec::new());
                out.push(vec![(words::missing_hint(&font.named), Role::Dim)]);
            }
        }
    }
    out
}

/// The text of `line`, its spans joined.
pub fn text(line: &FactLine) -> String {
    line.iter().map(|(t, _)| t.as_str()).collect()
}

/// Whether any of `lines` is wider than `cells`, so the pane cuts it.
pub fn cut(lines: &[FactLine], cells: usize) -> bool {
    lines.iter().any(|l| canvas::cells(&text(l)) > cells)
}

/// Draws `lines` in `rect`, each cut at the pane's right edge.
pub fn draw(lines: &[FactLine], buf: &mut Buffer, rect: Rect, color: bool) {
    for (i, line) in lines.iter().take(usize::from(rect.height)).enumerate() {
        let Ok(dy) = u16::try_from(i) else { break };
        let y = rect.y.saturating_add(dy);
        put_spans(buf, (rect.x, y), line, None, color, rect.right());
    }
}
