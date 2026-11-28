//! The search row, as it is drawn: the prompt and the entry at the left,
//! as ` ? *mono_`, and the syntax help right-justified on the same row,
//! as `^start  $end  *anywhere · case ignored · family, file`, with one
//! blank at each end. The help names the fields the list matches where
//! the row has room for them, and leaves them out where it does not. A
//! long entry pushes the help off the row altogether. An entry longer
//! than the row shows its end after `…`, so the cursor and what was typed
//! last stay in sight. The cursor `_` follows the entry while the person
//! types, and a kept filter shows its pattern without it.
//!
//! References:
//! - ADR-1403 D11, "Search, on ?", "Where it sits": the entry at the
//!   left after the prompt, the help right-justified, naming the fields
//!   when there is room.
//! - ADR-1206 D4: the roles `search` and `search-help`.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};

use super::Search;
use crate::canvas;
use crate::roles::Role;

/// The syntax help, which every search row shows where it has room.
pub const HELP: &str = "^start  $end  *anywhere · case ignored";

/// The blanks the row keeps between the entry and the help.
const GAP: usize = 2;

/// The row's text, laid out for `width` cells: the prompt, the entry as
/// it shows, and the help, or `None` where the help has no room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The prompt, such as `?` or `Find`.
    pub prompt: &'static str,
    /// The entry as it shows, cut at its start with `…` where it is too
    /// long, and with the cursor `_` after it while the person types.
    pub entry: String,
    /// The help, with the fields where they fit.
    pub help: Option<String>,
}

/// The row for `search` in `width` cells.
pub fn line(search: &Search, width: u16) -> Line {
    let width = usize::from(width);
    let prompt = search.prompt().text();
    let cursor = if search.typing() { "_" } else { "" };
    // ` PROMPT ENTRY_ `: a blank at each end and one after the prompt
    let room = width
        .saturating_sub(canvas::cells(prompt))
        .saturating_sub(3)
        .saturating_sub(cursor.len());
    let mut entry = tail(search.entry(), room);
    entry.push_str(cursor);
    let used = canvas::cells(prompt)
        .saturating_add(canvas::cells(&entry))
        .saturating_add(2);
    let with_fields = (!search.help().is_empty()).then(|| format!("{HELP} · {}", search.help()));
    let help = with_fields
        .into_iter()
        .chain(std::iter::once(HELP.to_owned()))
        .find(|h| used.saturating_add(GAP).saturating_add(canvas::cells(h)) < width);
    Line {
        prompt,
        entry,
        help,
    }
}

/// The end of `text` that fits in `room` cells, after `…` where the whole
/// does not.
fn tail(text: &str, room: usize) -> String {
    if canvas::cells(text) <= room {
        return text.to_owned();
    }
    let mut kept: Vec<&str> = Vec::new();
    let mut used = 1usize;
    let shown = canvas::printable(text);
    let clusters = canvas::clusters(&shown);
    for c in clusters.iter().rev() {
        let w = usize::from(canvas::shown(c).1);
        if used.saturating_add(w) > room {
            break;
        }
        used = used.saturating_add(w);
        kept.push(c.text);
    }
    kept.reverse();
    format!("…{}", kept.concat())
}

/// Draws `search`'s row in the first row of `area`, and returns the cell
/// where the cursor stands: the `_` after the entry while the person
/// types, else the cell after the entry. `color` is whether the TUI draws
/// color ([`crate::color::enabled`]).
pub fn draw(search: &Search, buf: &mut Buffer, area: Rect, color: bool) -> Position {
    let line = line(search, area.width);
    let entry_style = Role::Search.style(color);
    let prompt_style = crate::color::Style {
        bold: true,
        ..entry_style
    };
    let width = usize::from(area.width);
    let y = area.y;
    // the row starts afresh, so no attribute of what was drawn there
    // before stays under it
    for x in area.left()..area.right() {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.reset();
            cell.set_style(entry_style.cell());
        }
    }
    let mut x = area.x.saturating_add(1);
    let put = |buf: &mut Buffer, x: u16, text: &str, style: crate::color::Style| {
        let room = usize::from(area.right().saturating_sub(x));
        buf.set_stringn(x, y, text, room, style.cell()).0
    };
    x = put(buf, x, line.prompt, prompt_style).saturating_add(1);
    let entry_x = x;
    x = put(buf, x, &line.entry, entry_style);
    if let Some(help) = &line.help {
        let at = width.saturating_sub(1).saturating_sub(canvas::cells(help));
        let at = area.x.saturating_add(u16::try_from(at).unwrap_or(0));
        put(buf, at, help, Role::SearchHelp.style(color));
    }
    let cursor = if search.typing() {
        x.saturating_sub(1).max(entry_x)
    } else {
        x
    };
    Position::new(cursor.min(area.right().saturating_sub(1)), y)
}
