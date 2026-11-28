//! A font's full detail, the level Enter opens over the list: every fact
//! as a labelled line, the whole path, each face the font helper found
//! with its format, kind, coverage and sizes, the state and the trust
//! check it fails, where its activation comes from, a duplicate mark and
//! the looks that use it. A long fact wraps under its label, so nothing
//! is cut here, and Up and Down scroll the detail where it is taller than
//! the viewer. The detail changes nothing; only Enter on a font of your
//! own acts, by asking to install it as a system font.
//!
//! References:
//! - ADR-1205 D8: a font's view shows what it covers and the sizes it
//!   supplies.
//! - ADR-1403 D10: a detail is read-only, and Enter on a font from the
//!   person's own folders asks to install it as a system font.
//! - ADR-1403 D11, "Fonts the configuration uses but has not activated":
//!   the marks, and a font that fails the trust rule naming its check.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::batch;
use vtium_config_core::discover::Described;
use vtium_config_core::fontuse::State;
use vtium_config_core::ui::Shared;
use vtium_config_widgets::canvas;
use vtium_config_widgets::roles::Role;

use crate::list::put;
use crate::states::{Item, Known};
use crate::words;

/// One line of the detail before it wraps: its label, or none for a line
/// of its own, and its value with its role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub label: Option<&'static str>,
    pub value: String,
    pub role: Role,
}

fn entry(label: &'static str, value: impl Into<String>) -> Entry {
    Entry {
        label: Some(label),
        value: value.into(),
        role: Role::Row,
    }
}

fn plain(value: impl Into<String>, role: Role) -> Entry {
    Entry {
        label: None,
        value: value.into(),
        role,
    }
}

/// The detail of `item`: its title, then its labelled lines.
pub fn entries(known: &Known, shared: &Shared, item: Item) -> (String, Vec<Entry>) {
    let Some(at) = known.position(item) else {
        return (String::new(), Vec::new());
    };
    let state = known.state(at);
    let users = words::users(known.users(shared, item), usize::MAX);
    let mut out = Vec::new();
    let title = match item {
        Item::File(i) => {
            let Some(f) = shared.found.fonts.files().get(i) else {
                return (String::new(), out);
            };
            let place = words::place_words(f.place);
            out.push(entry("File", format!("{}   {place}", f.path().display())));
            match &f.described {
                None => out.push(plain(words::DESCRIBING, Role::Dim)),
                Some(Described::Font(d)) => {
                    let several = d.faces.len() > 1;
                    for face in &d.faces {
                        if several {
                            out.push(entry("Face", words::face_name(&face.family, &face.style)));
                        }
                        let mut kind = format!(
                            "{}, {}",
                            words::format(d.format),
                            words::drawn(face.outline, face.monospaced)
                        );
                        if !face.groups.is_empty() {
                            let groups: Vec<&str> = face.groups.iter().map(|g| g.name()).collect();
                            kind.push_str(&format!(" · belongs to {}", groups.join(", ")));
                        }
                        out.push(entry("Kind", kind));
                        let covers = if face.scripts.is_empty() {
                            format!("{} characters, and no script in full", face.codepoints)
                        } else {
                            format!(
                                "{} characters: {}",
                                face.codepoints,
                                words::covers(&face.scripts)
                            )
                        };
                        out.push(entry("Covers", covers));
                        let strikes: Vec<String> = face
                            .strikes
                            .iter()
                            .map(|s| words::strike(s.width, s.height))
                            .collect();
                        let sizes = match (face.outline, strikes.is_empty()) {
                            (true, true) => "any size, since it has outlines".to_owned(),
                            (true, false) => format!(
                                "any size, since it has outlines, and bitmaps for {}",
                                strikes.join(", ")
                            ),
                            (false, _) => strikes.join(", "),
                        };
                        out.push(entry("Sizes", sizes));
                    }
                }
                Some(Described::NotAFont(why) | Described::Unknown(why)) => {
                    out.push(plain(words::not_described(why), Role::Dim));
                }
            }
            if let Some(state) = state {
                out.push(entry("State", state.words()));
                match state {
                    State::FailsTrust { own: true, .. } => {
                        out.push(entry("", words::OWN_STATE));
                    }
                    State::FailsTrust { check, .. } => {
                        out.push(entry("Trust", words::fails(check)));
                    }
                    _ => out.push(entry("Trust", words::TRUSTED)),
                }
            }
            if shared.document().content().active.get(f.path()).is_some() {
                out.push(entry("From", "the change not saved, which Save makes real"));
            } else if let Some(from) = batch::activated_by(&f.resource.origin) {
                out.push(entry("From", from));
            }
            if let Some(dup) = &f.duplicate {
                out.push(entry("Duplicate", dup.words()));
            }
            out.push(entry("Used by", users));
            if matches!(state, Some(State::FailsTrust { own: false, .. })) {
                out.push(plain("", Role::Row));
                out.push(plain(words::TRUST_RULE, Role::Dim));
            }
            let name = f.resource.name();
            match f.faces().first() {
                Some(face) => {
                    let mut t = format!("{name}  {}", words::face_name(&face.family, &face.style));
                    if let Some(v) = &face.version {
                        t.push_str(&format!(", version {v}"));
                    }
                    t
                }
                None => name,
            }
        }
        Item::BuiltIn(name) => {
            out.push(entry("Kind", words::BUILT_IN_KIND));
            out.push(entry("State", words::ALWAYS.0));
            out.push(entry("", words::BUILT_IN_STATE));
            name.to_owned()
        }
        Item::Named(u) => {
            let Some(font) = known.used.fonts.get(u) else {
                return (String::new(), out);
            };
            out.push(entry("Named", font.named.clone()));
            if let Some(path) = &font.path {
                out.push(entry("File", path.display().to_string()));
            }
            out.push(entry("State", font.state.words()));
            if let Some(why) = font.state.reason() {
                out.push(entry("", why));
            }
            out.push(entry("Used by", users));
            known
                .fields
                .get(at)
                .map_or_else(|| font.name(), |f| f[0].clone())
        }
    };
    (title, out)
}

/// The detail's rows in a viewer `width` cells wide: the title, a blank,
/// then each entry wrapped under its label.
pub fn rows(title: &str, entries: &[Entry], width: usize) -> Vec<(String, Role)> {
    let mut out = vec![
        (format!(" {title}"), Role::Heading),
        (String::new(), Role::Row),
    ];
    let indent = 3 + words::DETAIL_LABEL;
    for e in entries {
        let (lead, room) = match e.label {
            Some(label) => (
                format!("   {label:<width$}", width = words::DETAIL_LABEL),
                width.saturating_sub(indent).max(8),
            ),
            None => (" ".to_owned(), width.saturating_sub(2).max(8)),
        };
        let wrapped = canvas::wrap(&e.value, room, usize::MAX);
        if wrapped.is_empty() {
            out.push((lead.trim_end().to_owned(), e.role));
            continue;
        }
        for (i, part) in wrapped.into_iter().enumerate() {
            let pad = if i == 0 {
                lead.clone()
            } else {
                " ".repeat(canvas::cells(&lead))
            };
            out.push((format!("{pad}{part}"), e.role));
        }
    }
    out
}

/// The first row the detail can scroll to: the title stays, and the rows
/// under it scroll as far as the last one shows.
pub fn last_top(rows: usize, height: usize) -> usize {
    rows.saturating_sub(height.max(1))
}

/// Draws the detail's `rows` in `rect`, from `top`, its title kept at the
/// top.
pub fn draw(rows: &[(String, Role)], top: usize, buf: &mut Buffer, rect: Rect, color: bool) {
    let height = usize::from(rect.height);
    let shown = rows
        .iter()
        .take(1)
        .chain(rows.iter().skip(1).skip(top))
        .take(height);
    for (i, (text, role)) in shown.enumerate() {
        let Ok(dy) = u16::try_from(i) else { break };
        let y = rect.y.saturating_add(dy);
        let text = canvas::fit(text, usize::from(rect.width));
        put(
            buf,
            rect.x,
            y,
            &text,
            role.style(color).cell(),
            rect.right(),
        );
    }
}
