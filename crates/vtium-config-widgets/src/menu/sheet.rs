//! The frame of a sheet: a titled box over the viewer, such as the
//! Problems list, Manage configurations or a version from History, with a
//! close mark at the right of its top border. The sheet's own buttons are
//! on the action bar, so the box holds only its lines.
//!
//! References:
//! - ADR-1403 D11, "The mouse": a click on a sheet's close mark closes it.
//! - ADR-1403 D11, "The action bar": the buttons inside sheets move to
//!   the bar.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};

use crate::frame::{Run, clear, cut, paint, width};
use crate::roles::Role;

/// The close mark at the right of a sheet's top border, with a space on
/// each side, all three of which a click may land on.
const CLOSE: &str = " × ";

/// A sheet's frame as it is drawn, and its places.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetLayout {
    area: Rect,
    inner: Rect,
    close: Option<Rect>,
    runs: Vec<Run>,
}

impl SheetLayout {
    /// The sheet's box, which [`SheetLayout::draw`] clears.
    pub fn area(&self) -> Rect {
        self.area
    }

    /// Where the sheet's lines go: inside the borders, a cell in from each
    /// side.
    pub fn inner(&self) -> Rect {
        self.inner
    }

    /// The close mark's place, where the sheet has one.
    pub fn close(&self) -> Option<Rect> {
        self.close
    }

    /// Whether a click at `at` lands on the close mark.
    pub fn hits_close(&self, at: Position) -> bool {
        self.close.is_some_and(|r| r.contains(at))
    }

    /// Clears the sheet's box in `buf` and draws its frame.
    pub fn draw(&self, buf: &mut Buffer, color: bool) {
        clear(buf, self.area);
        paint(buf, &self.runs, color);
    }
}

/// The frame of a sheet in `area`, titled `title`, with a close mark where
/// `closes` holds.
pub fn layout(area: Rect, title: &str, closes: bool) -> SheetLayout {
    let Rect { x, y, .. } = area;
    let w = area.width;
    let inner_w = w.saturating_sub(2);
    let end = if closes { CLOSE } else { "" };
    let title = if title.is_empty() {
        String::new()
    } else {
        format!(
            " {} ",
            cut(title, inner_w.saturating_sub(width(end)).saturating_sub(3))
        )
    };
    let fill = inner_w
        .saturating_sub(width(&title))
        .saturating_sub(width(end));
    let mut runs = vec![
        Run::new(x, y, "┌", &[Role::Menu]),
        Run::new(
            x.saturating_add(1),
            y,
            title.clone(),
            &[Role::Menu, Role::Heading],
        ),
        Run::new(
            x.saturating_add(1).saturating_add(width(&title)),
            y,
            "─".repeat(usize::from(fill)),
            &[Role::Menu],
        ),
    ];
    let cx = x
        .saturating_add(1)
        .saturating_add(width(&title))
        .saturating_add(fill);
    let close = (closes && w > 2).then(|| {
        runs.push(Run::new(cx, y, end, &[Role::Menu]));
        Rect::new(cx, y, width(end), 1)
    });
    runs.push(Run::new(
        x.saturating_add(w.saturating_sub(1)),
        y,
        "┐",
        &[Role::Menu],
    ));
    let bottom = area.bottom().saturating_sub(1);
    for row in y.saturating_add(1)..bottom {
        runs.push(Run::new(x, row, "│", &[Role::Menu]));
        runs.push(Run::new(
            x.saturating_add(w.saturating_sub(1)),
            row,
            "│",
            &[Role::Menu],
        ));
    }
    if bottom > y {
        runs.push(Run::new(
            x,
            bottom,
            format!("└{}┘", "─".repeat(usize::from(inner_w))),
            &[Role::Menu],
        ));
    }
    SheetLayout {
        area,
        inner: Rect::new(
            x.saturating_add(2),
            y.saturating_add(1),
            w.saturating_sub(4),
            area.height.saturating_sub(2),
        ),
        close,
        runs,
    }
}

/// A box of `width` by `height` cells in the middle of `within`, made
/// smaller where `within` is.
pub fn centered(within: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(within.width);
    let h = height.min(within.height);
    Rect::new(
        within.x.saturating_add(within.width.saturating_sub(w) / 2),
        within.y.saturating_add(within.height.saturating_sub(h) / 2),
        w,
        h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sheet_is_centred_and_never_larger_than_where_it_sits() {
        let v = Rect::new(0, 2, 100, 26);
        assert_eq!(centered(v, 60, 10), Rect::new(20, 10, 60, 10));
        assert_eq!(centered(v, 200, 40), v);
    }
}
