//! Lays out and draws an open menu and its submenu: each box as wide as
//! its widest item and kept on the screen, with the area under it cleared.
//! Where the screen holds fewer rows than a menu has entries, the box
//! shows a window of them, and a row at each end that has more beyond it
//! says how many.
//!
//! References:
//! - ADR-1403 D11, "The menus": the check marks, separators, the column of
//!   shortcuts, the full-width highlight, the greyed items with their
//!   reasons, and the submenu to the right of its item.
//! - ADR-1206 D3: vtium-config draws its menus from ratatui's parts, with
//!   the area under them cleared as ratatui-widgets' `Clear` clears it.
//! - ADR-1206 D10: lists stay readable with dozens of items.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};

use super::{Entry, Item, MenuPlaces, Open, Opens, Pick, Pull};
use crate::frame::{Run, clear, cut, lettered, paint, width};
use crate::roles::Role;

/// An open menu as it is drawn, and its places.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuLayout {
    runs: Vec<Run>,
    places: MenuPlaces,
}

impl MenuLayout {
    /// Where the menu's boxes and items are.
    pub fn places(&self) -> &MenuPlaces {
        &self.places
    }

    /// Draws the menu and its submenu into `buf`, over what is there,
    /// clearing the boxes first.
    pub fn draw(&self, buf: &mut Buffer, color: bool) {
        for b in &self.places.boxes {
            clear(buf, *b);
        }
        paint(buf, &self.runs, color);
    }
}

/// The rows a menu opening at `at` in `area` can take, its borders
/// included.
pub(super) fn room(area: Rect, at: Position, opens: Opens) -> u16 {
    match opens {
        Opens::Down => area.bottom().saturating_sub(at.y.max(area.y)),
        Opens::Up => {
            at.y.min(area.bottom().saturating_sub(1))
                .saturating_add(1)
                .saturating_sub(area.y)
        }
    }
}

/// Which entries a box of at most `rows` rows shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Window {
    /// A row above the entries says how many come before them.
    pub up: bool,
    /// The first entry shown.
    pub start: usize,
    /// How many entries show.
    pub count: usize,
    /// A row below the entries says how many come after them.
    pub down: bool,
}

impl Window {
    /// The box's rows: its entries, the rows that say how many more, and
    /// its two borders.
    fn height(self) -> u16 {
        u16::try_from(self.count)
            .unwrap_or(u16::MAX)
            .saturating_add(u16::from(self.up))
            .saturating_add(u16::from(self.down))
            .saturating_add(2)
    }
}

/// The window of `len` entries from `offset` in a box of at most `rows`
/// rows.
pub(super) fn window(rows: u16, len: usize, offset: usize) -> Window {
    let slots = usize::from(rows.saturating_sub(2));
    if len <= slots || slots == 0 {
        return Window {
            up: false,
            start: 0,
            count: len.min(slots),
            down: false,
        };
    }
    let start = offset.min(len.saturating_sub(1));
    let up = start > 0;
    let room = slots.saturating_sub(usize::from(up));
    if start.saturating_add(room) >= len {
        return Window {
            up,
            start,
            count: len.saturating_sub(start),
            down: false,
        };
    }
    Window {
        up,
        start,
        count: room.saturating_sub(1),
        down: true,
    }
}

/// The width of the check column and of the column of shortcuts.
#[derive(Clone, Copy)]
struct Columns {
    check: u16,
    shortcut: u16,
}

impl Columns {
    fn of(entries: &[Entry]) -> Columns {
        let mut cols = Columns {
            check: 0,
            shortcut: 0,
        };
        for item in items(entries) {
            if item.check.is_some() {
                cols.check = 2;
            }
            cols.shortcut = cols.shortcut.max(item.shortcut.as_deref().map_or(0, width));
        }
        cols
    }

    /// The cells of an item's line besides its name: a space, the check,
    /// at least two spaces, the shortcut, then a space, › or a space, and
    /// a space.
    fn fixed(self) -> u16 {
        self.check.saturating_add(self.shortcut).saturating_add(6)
    }
}

fn items(entries: &[Entry]) -> impl Iterator<Item = &Item> {
    entries.iter().filter_map(|e| match e {
        Entry::Item(i) => Some(i),
        Entry::Separator => None,
    })
}

/// The item's name and, where it is greyed out, its reason after a dot.
fn left_text(item: &Item) -> String {
    match item.greyed.as_deref() {
        Some(reason) if !reason.is_empty() => format!("{} · {reason}", item.label),
        _ => item.label.clone(),
    }
}

/// The cells inside the borders of a box that holds `entries`, at most
/// `most`.
fn inner_width(entries: &[Entry], most: u16) -> u16 {
    let widest = items(entries)
        .map(|i| width(&left_text(i)))
        .max()
        .unwrap_or(0);
    Columns::of(entries)
        .fixed()
        .saturating_add(widest)
        .min(most)
}

/// What a menu's layout gathers as it lays out its boxes.
struct Builder {
    area: Rect,
    runs: Vec<Run>,
    items: Vec<(Rect, Pick)>,
}

impl Builder {
    /// Lays out the box of `pull` at column `x` and row `top`, in the
    /// window `win`, each item's place marked by `pick`. Returns the box.
    fn add(
        &mut self,
        x: u16,
        top: u16,
        pull: &Pull<'_>,
        win: Window,
        pick: fn(usize) -> Pick,
    ) -> Rect {
        let cols = Columns::of(pull.entries);
        let inner = inner_width(pull.entries, self.area.width.saturating_sub(2));
        let w = inner.saturating_add(2);
        let x = x.min(self.area.right().saturating_sub(w)).max(self.area.x);
        let ix = x.saturating_add(1);
        let rule = |y: u16, l: &str, r: &str| {
            Run::new(
                x,
                y,
                format!("{l}{}{r}", "─".repeat(usize::from(inner))),
                &[Role::Menu],
            )
        };
        self.runs.push(rule(top, "┌", "┐"));
        let mut y = top.saturating_add(1);
        if win.up {
            self.more(x, y, inner, format!("↑ {} more", win.start));
            y = y.saturating_add(1);
        }
        for (i, entry) in pull
            .entries
            .iter()
            .enumerate()
            .skip(win.start)
            .take(win.count)
        {
            match entry {
                Entry::Separator => self.runs.push(rule(y, "├", "┤")),
                Entry::Item(item) => {
                    self.sides(x, y, inner);
                    self.line(ix, y, inner, cols, item, pull.highlight == Some(i));
                    self.items
                        .push((Rect::new(ix, y, inner, 1).intersection(self.area), pick(i)));
                }
            }
            y = y.saturating_add(1);
        }
        if win.down {
            let after = pull
                .entries
                .len()
                .saturating_sub(win.start.saturating_add(win.count));
            self.more(x, y, inner, format!("↓ {after} more"));
            y = y.saturating_add(1);
        }
        self.runs.push(rule(y, "└", "┘"));
        Rect::new(x, top, w, win.height()).intersection(self.area)
    }

    /// The borders of a line of the box at `x`.
    fn sides(&mut self, x: u16, y: u16, inner: u16) {
        self.runs.push(Run::new(x, y, "│", &[Role::Menu]));
        self.runs.push(Run::new(
            x.saturating_add(inner).saturating_add(1),
            y,
            "│",
            &[Role::Menu],
        ));
    }

    /// A line that says how many more entries lie beyond the window.
    fn more(&mut self, x: u16, y: u16, inner: u16, text: String) {
        self.sides(x, y, inner);
        let ix = x.saturating_add(1);
        self.runs.push(Run::new(
            ix,
            y,
            " ".repeat(usize::from(inner)),
            &[Role::Menu],
        ));
        self.runs.push(Run::new(
            ix.saturating_add(1),
            y,
            cut(&text, inner.saturating_sub(2)),
            &[Role::Menu, Role::Dim],
        ));
    }

    /// An item's line inside its box, `inner` cells from `x`.
    fn line(&mut self, x: u16, y: u16, inner: u16, cols: Columns, item: &Item, lit: bool) {
        let greyed = item.greyed.is_some();
        let first = if lit { Role::MenuHighlight } else { Role::Menu };
        let base: &[Role] = if greyed {
            &[first, Role::MenuGreyed]
        } else {
            &[first]
        };
        self.runs
            .push(Run::new(x, y, " ".repeat(usize::from(inner)), base));
        let mut at = x.saturating_add(1);
        if cols.check > 0 {
            let mark = if item.check == Some(true) { "✓" } else { " " };
            self.runs.push(Run::new(at, y, mark, base));
            at = at.saturating_add(cols.check);
        }
        let text = cut(&left_text(item), inner.saturating_sub(cols.fixed()));
        match item.letter {
            // A greyed item's letter chooses nothing, so it is not marked.
            Some(letter) if !greyed => self.runs.extend(lettered(at, y, &text, letter, first)),
            _ => self.runs.push(Run::new(at, y, text, base)),
        }
        let end = x.saturating_add(inner);
        if let Some(key) = item.shortcut.as_deref() {
            let kx = end.saturating_sub(3).saturating_sub(width(key));
            self.runs.push(Run::new(kx, y, key, base));
        }
        if item.submenu {
            self.runs
                .push(Run::new(end.saturating_sub(2), y, "›", base));
        }
    }
}

/// The open menu `open` in `area`, the rows it may cover, which
/// [`crate::frame::Places::menu_area`] gives as the rows from the rule
/// under the top row to the rule over the action bar.
pub fn layout(area: Rect, open: &Open<'_>) -> MenuLayout {
    let mut b = Builder {
        area,
        runs: Vec::new(),
        items: Vec::new(),
    };
    let win = window(
        room(area, open.at, open.opens),
        open.menu.entries.len(),
        open.menu.offset,
    );
    let top = match open.opens {
        Opens::Down => open.at.y.max(area.y),
        Opens::Up => open
            .at
            .y
            .saturating_add(1)
            .saturating_sub(win.height())
            .max(area.y),
    };
    let main = b.add(open.at.x, top, &open.menu, win, Pick::Item);
    let mut boxes = vec![main];
    if let Some((parent, sub)) = &open.sub {
        // The submenu's first entry sits on its parent's row, at the
        // menu's right, or at its left where the right has no room. Where
        // neither side has room, it keeps to the screen's right edge, over
        // the right of the menu, so the menu's names still show. The box
        // moves up where the screen ends below it.
        let row = b
            .items
            .iter()
            .find(|(_, p)| *p == Pick::Item(*parent))
            .map_or(main.y, |(r, _)| r.y);
        let w = inner_width(sub.entries, area.width.saturating_sub(2)).saturating_add(2);
        let x = if main.right().saturating_add(w) <= area.right() {
            main.right()
        } else if main.x.saturating_sub(area.x) >= w {
            main.x.saturating_sub(w)
        } else {
            area.right().saturating_sub(w)
        };
        let win = window(area.height, sub.entries.len(), sub.offset);
        let top = row
            .saturating_sub(1)
            .min(area.bottom().saturating_sub(win.height()))
            .max(area.y);
        boxes.push(b.add(x, top, sub, win, Pick::Sub));
    }
    MenuLayout {
        runs: b.runs,
        places: MenuPlaces {
            boxes,
            items: b.items,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_that_holds_every_entry_shows_them_all() {
        let w = window(10, 8, 3);
        assert_eq!(
            (w.up, w.start, w.count, w.down, w.height()),
            (false, 0, 8, false, 10)
        );
    }

    #[test]
    fn a_long_menu_says_how_many_more_lie_beyond_each_end() {
        let w = window(10, 50, 0);
        assert_eq!((w.up, w.start, w.count, w.down), (false, 0, 7, true));
        let w = window(10, 50, 20);
        assert_eq!((w.up, w.start, w.count, w.down), (true, 20, 6, true));
        let w = window(10, 50, 45);
        assert_eq!((w.up, w.start, w.count, w.down), (true, 45, 5, false));
        assert!(window(10, 50, 20).height() <= 10);
    }
}
