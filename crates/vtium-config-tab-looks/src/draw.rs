//! Where the tab's parts go in the viewer the frame gives it, and the
//! helpers that write text and the preview into the buffer.
//!
//! With nothing open, the list sits at the left and a rule divides it from
//! the right, which holds the highlighted item's card or facts, the
//! preview's box under them and one line under the box. A card, a detail
//! or the look's missing fonts fill the whole viewer.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Looks bullet: the card beside the list, above the preview.
//! - ADR-1206 D10: 100×30 is the design size, and 80×25 the smallest, where the panes keep their order with fewer columns.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::Style;
use ratatui_core::widgets::Widget;
use ratatui_widgets::block::Block;
use ratatui_widgets::borders::Borders;
use vtium_config_core::preview::{Place, console};
use vtium_config_widgets::canvas::{self, Row};
use vtium_config_widgets::roles::Role;

/// The list's width at 100 columns and more.
pub const LIST_WIDTH: u16 = 34;

/// The narrowest the list gets.
pub const LIST_MIN: u16 = 24;

/// The rows the card or the facts take above the preview.
pub const CARD_ROWS: u16 = 10;

/// The fewest rows inside the preview's box worth drawing it for.
const PREVIEW_MIN: u16 = 3;

/// Where each part of the list's screen goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Panes {
    pub list: Rect,
    pub rule: Rect,
    /// The card or the facts.
    pub card: Rect,
    /// The preview's box, its border included, where the viewer has room.
    pub preview: Option<Rect>,
    /// The line under the preview.
    pub caption: Rect,
}

impl Panes {
    /// The panes of the viewer `area`.
    pub fn of(area: Rect) -> Panes {
        let list_width = if area.width >= 100 {
            LIST_WIDTH
        } else {
            (area.width / 3).max(LIST_MIN).min(area.width)
        };
        let list = Rect::new(area.x, area.y, list_width, area.height);
        let rule = Rect::new(area.x.saturating_add(list_width), area.y, 1, area.height);
        let right_x = rule.x.saturating_add(2);
        let right_w = area.right().saturating_sub(right_x).saturating_sub(1);
        let card_rows = CARD_ROWS.min(area.height);
        let card = Rect::new(right_x, area.y, right_w, card_rows);
        let caption_y = area.bottom().saturating_sub(1);
        let box_top = area.y.saturating_add(card_rows);
        let box_rows = caption_y.saturating_sub(box_top);
        let preview = (box_rows >= PREVIEW_MIN.saturating_add(2))
            .then(|| Rect::new(right_x, box_top, right_w, box_rows));
        Panes {
            list,
            rule,
            card,
            preview,
            caption: Rect::new(right_x, caption_y, right_w, 1),
        }
    }

    /// The preview's inside, where the picture or the console's own text
    /// goes, as the previews place it.
    pub fn place(&self) -> Option<Place> {
        self.preview.map(|r| {
            Place::new(
                r.x.saturating_add(1),
                r.y.saturating_add(1),
                r.width.saturating_sub(2),
                r.height.saturating_sub(2),
            )
        })
    }
}

/// Writes `text` at `x`, `y` in `style`, cut at `rect`'s right edge, and
/// returns the column after it.
pub fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style, rect: Rect) -> u16 {
    if y < rect.top() || y >= rect.bottom() || x >= rect.right() {
        return x;
    }
    let room = usize::from(rect.right().saturating_sub(x));
    buf.set_stringn(x, y, canvas::printable(text), room, style)
        .0
}

/// Writes the parts of a line, each in its role, from `x`, `y`, and
/// returns the column after them.
pub fn put_parts(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    parts: &[(String, Role)],
    color: bool,
    rect: Rect,
) -> u16 {
    let mut at = x;
    for (text, role) in parts {
        at = put(buf, at, y, text, role.style(color).cell(), rect);
    }
    at
}

/// Draws the rule between the list and the right.
pub fn rule(buf: &mut Buffer, rect: Rect, color: bool) {
    Block::new()
        .borders(Borders::LEFT)
        .border_style(Role::Row.style(color).cell())
        .render(rect, buf);
}

/// Draws the preview's box in `rect` with `title` on its top border, and
/// inside it `lines`, the console's own text, where it has any.
pub fn preview(buf: &mut Buffer, rect: Rect, title: &str, lines: Vec<Row>, color: bool) {
    let style = Role::Row.style(color).cell();
    Block::bordered()
        .border_style(style)
        .title(format!(" {title} "))
        .render(rect, buf);
    let inside = Place::new(
        rect.x.saturating_add(1),
        rect.y.saturating_add(1),
        rect.width.saturating_sub(2),
        rect.height.saturating_sub(2),
    );
    for placed in console::placed(lines, inside) {
        let mut x = placed.col;
        for span in placed.line {
            let cell = if color {
                span.style.cell()
            } else {
                Role::Row.style(false).cell()
            };
            x = put(buf, x, placed.row, &span.text, cell, rect);
        }
    }
}

/// `text` wrapped to `width` cells, at most `most` lines.
pub fn wrap(text: &str, width: usize, most: usize) -> Vec<String> {
    canvas::wrap(text, width.max(1), most)
}
