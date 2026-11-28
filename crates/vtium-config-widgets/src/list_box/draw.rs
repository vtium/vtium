//! The list box as it is drawn: a box of light lines, a choice on each
//! row inside, `●` before the line's choice now and `○` before the rest,
//! and the highlighted row in reverse video. Where choices are hidden
//! above the rows shown, the top border says how many, as `↑ 12 more`,
//! and the foot says how many are below, as `↓ 3 more`. Where the box
//! offers the search, the foot ends with `[?] Search`, which a click
//! opens. Without color the marks, the counts and the reverse video say
//! all of it.
//!
//! References:
//! - ADR-1206 D8: the list box draws in the place the tab gives it, and
//!   returns the places of its rows.
//! - ADR-1403 D11, "The list box": the foot's `[?] Search` where the
//!   line's choices are many.
//! - ADR-1206 D10: each mark a color would carry is a word or an
//!   attribute.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;

use super::{AROUND, ListBox, Places};
use crate::canvas;
use crate::color::Style;
use crate::roles::Role;
use crate::tab::Choices;

/// The foot's offer of the search, which a click on it opens.
const SEARCH: &str = "[?] Search";

/// Where the foot's `[?] Search` sits in a box drawn in `rect`: at the
/// right of the bottom border, before a rule cell and the corner. `None`
/// where the box is too narrow to hold it.
pub(super) fn search_place(rect: Rect) -> Option<Rect> {
    let width = u16::try_from(SEARCH.len()).ok()?;
    let inner = rect.width.saturating_sub(2);
    (width.saturating_add(1) < inner && rect.height >= 2).then(|| {
        Rect::new(
            rect.right().saturating_sub(width.saturating_add(2)),
            rect.bottom().saturating_sub(1),
            width,
            1,
        )
    })
}

/// The cells inside the border that the foot of a box with `count`
/// choices needs for the most it can count and its `[?] Search`.
pub(super) fn foot_cells(count: usize) -> usize {
    canvas::cells(&format!(" ↓ {count} more "))
        .saturating_add(SEARCH.len())
        .saturating_add(2)
}

impl ListBox {
    /// Draws the box in `rect` and returns the place of each of its parts.
    /// `color` is whether the TUI draws color ([`crate::color::enabled`]).
    pub fn draw<C>(
        &self,
        choices: &dyn Choices<C>,
        buf: &mut Buffer,
        rect: Rect,
        color: bool,
    ) -> Places {
        let places = self.places(choices, rect);
        let rect = rect.intersection(buf.area);
        if rect.width < 2 || rect.height < 2 {
            return places;
        }
        let row = Role::Row.style(color);
        let highlight = Role::RowHighlight.style(color);
        let inner = usize::from(rect.width.saturating_sub(2));
        for y in rect.top()..rect.bottom() {
            for x in rect.left()..rect.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.reset();
                    cell.set_style(row.cell());
                }
            }
        }
        let first = self.settled(choices, rect);
        let below = self
            .showable(choices)
            .saturating_sub(first.saturating_add(places.rows.len()));
        let above = (first > 0).then(|| format!("↑ {first} more"));
        let below = (below > 0).then(|| format!("↓ {below} more"));
        let top = border('┌', '┐', inner, above.as_deref(), None);
        put(buf, rect.x, rect.y, &top, row);
        let search = places.search.is_some().then_some(SEARCH);
        let bottom = border('└', '┘', inner, below.as_deref(), search);
        put(buf, rect.x, rect.bottom().saturating_sub(1), &bottom, row);
        for y in rect.top().saturating_add(1)..rect.bottom().saturating_sub(1) {
            put(buf, rect.x, y, "│", row);
            put(buf, rect.right().saturating_sub(1), y, "│", row);
        }
        let current = choices.current();
        for (place, choice) in &places.rows {
            let mark = if current == Some(*choice) {
                '●'
            } else {
                '○'
            };
            let text = canvas::fit(&choices.text(*choice), inner.saturating_sub(AROUND));
            let line = format!(" {mark} {text}");
            let line = format!("{line:<inner$}");
            let style = if *choice == self.highlight {
                highlight
            } else {
                row
            };
            put(buf, place.x, place.y, &line, style);
        }
        places
    }
}

/// A border `inner` cells wide between its corners, with `left` near its
/// left end and `right` near its right end, each where it fits.
fn border(start: char, end: char, inner: usize, left: Option<&str>, right: Option<&str>) -> String {
    let piece = |t: &str| format!(" {t} ");
    let left = left.map(piece).filter(|l| canvas::cells(l) < inner);
    let right = right.filter(|r| canvas::cells(r).saturating_add(1) < inner);
    let used = left.as_deref().map_or(0, canvas::cells)
        + right.map_or(0, |r| canvas::cells(r).saturating_add(1));
    let left = if used > inner { None } else { left };
    let used = left.as_deref().map_or(0, canvas::cells)
        + right.map_or(0, |r| canvas::cells(r).saturating_add(1));
    let mut out = String::new();
    out.push(start);
    out.push_str(left.as_deref().unwrap_or(""));
    out.push_str(&"─".repeat(inner.saturating_sub(used)));
    if let Some(r) = right {
        out.push_str(r);
        out.push('─');
    }
    out.push(end);
    out
}

/// Writes `text` at `x`, `y` in `style`, at most to the buffer's right
/// edge.
fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style) {
    let room = usize::from(buf.area.right().saturating_sub(x));
    buf.set_stringn(x, y, text, room, style.cell());
}
