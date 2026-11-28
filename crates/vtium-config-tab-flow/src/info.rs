//! A display or an input of a path, read-only: what it matches and where
//! it goes. Enter opens it on a part of a path in CONFIGURATION, of the
//! roaming inputs, or of the path to Nowhere. What a path joins changes
//! only when the person revises the path, so the screen says so in one
//! line.
//!
//! References:
//! - ADR-1403 D7, "Editing an added path, and Revise": a path's parts
//!   change only through Revise.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::flow::{Flow, Kind};
use vtium_config_widgets::canvas::fit;
use vtium_config_widgets::roles::Role;

use crate::page::draw::Sheet;
use crate::page::layout::{Obj, Slot};
use crate::paths::{self, Doc};
use crate::words;

/// The part an information screen is about: its kind, its match, and the
/// heading that names its kind.
pub fn target(obj: &Obj) -> Option<(Kind, &str, &'static str)> {
    match obj {
        Obj::Doc(_, Slot::Display(s)) | Obj::Nothing(Slot::Display(s)) => {
            Some((Kind::Display, s, words::DISPLAY))
        }
        Obj::Doc(_, Slot::Input(s)) | Obj::Roam(Slot::Input(s)) | Obj::Nothing(Slot::Input(s)) => {
            Some((Kind::Input, s, words::INPUT))
        }
        _ => None,
    }
}

/// Where the part goes, in words: into a terminal, on its VTs; turned
/// off or set aside; or roaming.
pub fn goes(doc: &Doc, obj: &Obj) -> String {
    match obj {
        Obj::Doc(name, _) => {
            let vts = doc.path(name).map(paths::Path::vts).unwrap_or_default();
            format!("{}, {}", words::in_terminal(name), words::vt_words(&vts))
        }
        Obj::Nothing(Slot::Display(_)) => words::TURNED_OFF.to_owned(),
        Obj::Nothing(_) => words::SET_ASIDE.to_owned(),
        _ => words::ROAMS.to_owned(),
    }
}

/// Draws the part's information into `area`: the breadcrumb, the part's
/// kind, name and what the system calls it, what matches it, where it
/// goes, and how its path changes.
pub fn draw(flow: &Flow, doc: &Doc, obj: &Obj, buf: &mut Buffer, area: Rect, color: bool) {
    let Some((kind, sel, heading)) = target(obj) else {
        return;
    };
    let width = usize::from(area.width);
    let (title, detail) = paths::title(flow, kind, sel);
    let title = doc.label(kind, sel).map_or(title, str::to_owned);
    let role = match kind {
        Kind::Display => Role::Display,
        Kind::Input => Role::Input,
    };
    let mut s = Sheet::new(buf, area, color);
    s.crumb(&title);
    let x = s.put(1, 2, heading, Role::Heading);
    let x = s.put_bold(x.saturating_add(2), 2, &title, role);
    s.put(x.saturating_add(3), 2, &detail, Role::Dim);
    let value_x = 19usize;
    let room = width.saturating_sub(value_x).saturating_sub(1);
    for (y, label, value) in [
        (3, words::MATCHED_BY, sel.to_owned()),
        (4, words::WHERE_IT_GOES, goes(doc, obj)),
    ] {
        s.put(3, y, label, Role::Dim);
        s.put(value_x, y, &fit(&value, room), Role::Row);
    }
    let note = match obj {
        Obj::Roam(_) => words::REVISE_ROAMING_NOTE,
        _ => words::REVISE_NOTE,
    };
    s.put(1, 6, &fit(note, width.saturating_sub(2)), Role::Dim);
}
