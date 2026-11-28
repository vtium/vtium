//! The filter picker: a part maker's choice that matches by one key, such
//! as USB port or This monitor, lists every value the connected devices
//! give for that key, each with the devices it matches now, and Other…
//! last, where the person types a value. vtium-config checks a typed
//! value as vtium.conf's parser checks a rule's match.
//!
//! References:
//! - ADR-1403 D7, "Templates and the part makers": the picker, its
//!   heading and its line, Other…, and the check of a typed value.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::flow::{Flow, Kind};
use vtium_config_widgets::canvas::{cells, fit};
use vtium_config_widgets::roles::Role;

use super::words;
use crate::page::draw::Sheet;
use crate::parts::Stage;
use crate::paths;

/// An open picker: the match key it lists the values of, the line under
/// the cursor, and the value being typed after Other…, while it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picker {
    pub key: &'static str,
    pub label: &'static str,
    pub line: usize,
    pub typing: Option<String>,
}

/// One line of a picker: a value with the devices it matches, or Other…,
/// where the person types a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Value { sel: String, devices: Vec<String> },
    Typed,
}

/// The kind of device a part of `stage` is.
pub const fn kind_of(stage: Stage) -> Kind {
    match stage {
        Stage::Display => Kind::Display,
        _ => Kind::Input,
    }
}

/// The picker of the key `key` for parts of `stage`: every value the
/// connected devices give for that key, each with the devices it matches,
/// in the order they first come, then Other…. A USB port is a path
/// without its interface, and a name is written with `?` for each space,
/// since a rule's match holds no space.
pub fn values(flow: &Flow, stage: Stage, key: &str) -> Vec<Pick> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let prefix = format!("{key}:");
    for d in flow.devices(kind_of(stage)).iter().filter(|d| d.found) {
        let values: Vec<String> = if key == "name" {
            vec![format!("name:{}", d.name.replace(' ', "?"))]
        } else {
            d.keys
                .iter()
                .filter(|k| k.starts_with(&prefix))
                .map(|k| {
                    if key == "path" {
                        paths::port_of(k)
                    } else {
                        k.clone()
                    }
                })
                .collect()
        };
        for v in values {
            match out.iter_mut().find(|(s, _)| *s == v) {
                Some((_, names)) if !names.contains(&d.name) => names.push(d.name.clone()),
                Some(_) => {}
                None => out.push((v, vec![d.name.clone()])),
            }
        }
    }
    out.into_iter()
        .map(|(sel, devices)| Pick::Value { sel, devices })
        .chain(std::iter::once(Pick::Typed))
        .collect()
}

/// The match a value typed into the picker of `key` stands for: the value
/// as typed when it names a key of its own, else the value under `key`.
pub fn keyed(key: &str, typed: &str) -> String {
    let bare = typed.trim();
    let has_key = vtium_settings::Token::KEYS
        .iter()
        .any(|k| bare.strip_prefix(k).is_some_and(|r| r.starts_with(':')));
    if has_key || bare.is_empty() || bare == "*" {
        bare.to_owned()
    } else {
        format!("{key}:{bare}")
    }
}

/// Checks a match the person typed for a part of `stage`, as vtium.conf's
/// parser checks a rule's match, and gives it back with single spaces, or
/// says in one sentence what is wrong with it, for the picker of `key`.
pub fn check_typed(stage: Stage, key: &str, typed: &str) -> Result<String, String> {
    let sel = typed.split_whitespace().collect::<Vec<_>>().join(" ");
    if sel.is_empty() {
        return Err(words::TYPED_EMPTY.to_owned());
    }
    if sel.contains('#') || sel.contains("->") {
        return Err(words::typed_bad_mark(&sel));
    }
    let kind = match kind_of(stage) {
        Kind::Display => "display",
        Kind::Input => "input",
    };
    let text = format!("version = 1\n[head t]\n[rules]\n{kind} {sel} -> t\n");
    let read = vtium_settings::reading(text.as_bytes());
    // the parser's sentence names a rule, and the person typed a value of
    // the picker's kind, so the refusal names that
    if read.errors.errors.is_empty() {
        Ok(sel)
    } else {
        Err(words::typed_refused(key))
    }
}

/// The rows above the picker's first value: the breadcrumb, a blank row
/// and the heading.
pub const TOP: usize = 3;

/// The first line the picker shows in a viewer `rows` rows tall with the
/// cursor on `line`.
pub fn first_shown(line: usize, rows: usize) -> usize {
    let room = rows.saturating_sub(TOP).max(1);
    line.saturating_add(1).saturating_sub(room)
}

/// Draws the picker `p` for parts of `stage` into `area`: the heading,
/// each value with the devices it matches, and Other… last, or the value
/// being typed there.
pub fn draw(flow: &Flow, stage: Stage, p: &Picker, buf: &mut Buffer, area: Rect, color: bool) {
    let list = values(flow, stage, p.key);
    let width = usize::from(area.width);
    let rows = usize::from(area.height);
    let mut s = Sheet::new(buf, area, color);
    s.crumb(stage.name());
    let x = s.put_bold(1, 2, &p.label.to_uppercase(), Role::Heading);
    let about = words::picker_about(p.key, p.label);
    s.put(
        x.saturating_add(3),
        2,
        &fit(&about, width.saturating_sub(x).saturating_sub(4)),
        Role::Dim,
    );
    let words_w = list
        .iter()
        .map(|v| match v {
            Pick::Value { sel, .. } => cells(sel),
            Pick::Typed => cells(words::OTHER),
        })
        .max()
        .unwrap_or(0)
        .min(width / 2);
    let first = first_shown(p.line, rows);
    for (i, v) in list.iter().enumerate().skip(first) {
        let y = TOP.saturating_add(i.saturating_sub(first));
        if y >= rows {
            break;
        }
        let cursor = i == p.line;
        if cursor {
            s.highlight(y);
        }
        let role = if cursor {
            Role::RowHighlight
        } else {
            Role::Row
        };
        let dim = if cursor {
            Role::RowHighlight
        } else {
            Role::Dim
        };
        s.put(3, y, "○", dim);
        let (text, about) = match v {
            Pick::Value { sel, devices } => (sel.clone(), devices.join(", ")),
            Pick::Typed => (
                words::OTHER.to_owned(),
                match &p.typing {
                    Some(t) => super::entry(t, width.saturating_sub(words_w).saturating_sub(9)),
                    None => words::TYPE_A_VALUE.to_owned(),
                },
            ),
        };
        s.put(5, y, &fit(&text, words_w), role);
        let dx = 5usize.saturating_add(words_w).saturating_add(3);
        s.put(
            dx,
            y,
            &fit(&about, width.saturating_sub(dx).saturating_sub(1)),
            if p.typing.is_some() && *v == Pick::Typed {
                role
            } else {
                dim
            },
        );
    }
    if list.len() == 1 && rows > TOP.saturating_add(2) {
        s.put(3, TOP.saturating_add(2), words::NO_VALUES, Role::Dim);
    }
}
