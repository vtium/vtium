//! The shared list: the one component the TUI uses wherever it shows a set
//! of items or asks the person to pick one. It holds which item is
//! highlighted, reads the keys that move the highlight, pick an item or
//! close the list, and lays the items out as rows of the screen.
//!
//! A list has items, each a name and a grey detail. An item can start a
//! titled group, whose heading shows in bold on the row above it. A list
//! the person picks one item from marks each item with a circle, filled on
//! the item that is chosen now, as the Flow view's picker does. Up and
//! Down move the highlight by one item, Home and End go to the first and
//! the last, PgUp and PgDn move by a page, Enter picks and Esc closes.
//!
//! References:
//! - ADR-1403 D7, "One list for every set of items": one component for
//!   every set of items, its keys, and the picker's look.
//! - ADR-1201 D3: the Settings view and the Looks tab show their sets in it.

use super::canvas::{Row, span, wrap};
use super::color::{Color, Style};
use super::input::Key;

/// One item of a list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Item {
    /// What the item is called, the text the highlight covers.
    pub name: String,
    /// More about the item, in grey after the name.
    pub detail: String,
    /// Whether the item is the one chosen now, in a list that marks it.
    pub on: bool,
    /// The heading of the group this item starts, shown above it.
    pub heading: Option<String>,
}

impl Item {
    pub fn new(name: impl Into<String>, detail: impl Into<String>) -> Item {
        Item {
            name: name.into(),
            detail: detail.into(),
            ..Item::default()
        }
    }
}

/// How a list shows its items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marks {
    /// Each item as its name alone: a list of things to open.
    None,
    /// Each item with a circle, filled on the item chosen now: a list to
    /// pick one item from.
    Choice,
}

/// What a key did to a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// The key moved the highlight, or would have at an end of the list.
    Moved,
    /// Enter picked the item at this index.
    Picked(usize),
    /// Esc closed the list.
    Closed,
    /// The list does not read this key.
    Ignored,
}

/// The items PgUp and PgDn move by.
pub const PAGE: usize = 10;

/// A list's state: the highlighted item, and the first of the list's rows
/// that its window on the screen shows. Update keeps the offset, and the
/// view draws ratatui's List from a copy of it, so drawing never changes
/// the model.
///
/// References:
/// - ADR-1206 D4, "A list's selection lives in the tab's state": the tab
///   keeps the selection and the offset, and the view draws from a copy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListBox {
    pub at: usize,
    /// The list's first row in its window, counted in rows, since an item
    /// can take several.
    pub offset: usize,
}

impl ListBox {
    /// A list with the item `at` highlighted.
    pub const fn at(at: usize) -> ListBox {
        ListBox { at, offset: 0 }
    }

    /// Reads `key` in a list of `len` items.
    pub fn key(&mut self, key: Key, len: usize) -> Act {
        match key {
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                self.at = step(self.at, len, key);
                Act::Moved
            }
            Key::Enter if len > 0 => Act::Picked(self.at.min(len.saturating_sub(1))),
            Key::Escape => Act::Closed,
            _ => Act::Ignored,
        }
    }
}

/// `at` moved by `key` in a list of `n` items: by one, by a page, or to
/// an end, and never past an end.
pub fn step(at: usize, n: usize, key: Key) -> usize {
    let last = n.saturating_sub(1);
    match key {
        Key::Up => at.saturating_sub(1),
        Key::Down => at.saturating_add(1).min(last),
        Key::PageUp => at.saturating_sub(PAGE),
        Key::PageDown => at.saturating_add(PAGE).min(last),
        Key::Home => 0,
        Key::End => last,
        _ => at.min(last),
    }
}

/// The widest the column of names grows: a name wider than this puts its
/// detail on the rows below it.
const NAME_MAX: usize = 32;

/// The list's rows on a screen `cols` wide, with the item `at`
/// highlighted, and the index of the last row of that item, which the
/// screen keeps in sight. Each detail lines up in a column after the
/// names, and wraps by words under that column. A name wider than the
/// column has its detail start on the row below.
pub fn rows(items: &[Item], at: usize, marks: Marks, cols: usize) -> (Vec<Row>, usize) {
    let width = items
        .iter()
        .map(|i| i.name.chars().count())
        .max()
        .unwrap_or(0)
        .min(NAME_MAX)
        .min((cols / 2).saturating_sub(4));
    let lead: usize = match marks {
        Marks::None => 3,
        Marks::Choice => 5,
    };
    let column = lead.saturating_add(width).saturating_add(2);
    let room = cols.saturating_sub(column).saturating_sub(1).max(8);
    let heading = Style {
        bold: true,
        ..Style::text(Color::BrightWhite)
    };
    let grey = Style::text(Color::Grey);
    let mut out: Vec<Row> = Vec::new();
    let mut keep = 0;
    for (i, item) in items.iter().enumerate() {
        if let Some(title) = &item.heading {
            if !out.is_empty() {
                out.push(Vec::new());
            }
            out.push(vec![span(format!(" {title}"), heading)]);
        }
        let cursor = i == at;
        let name_style = if cursor {
            Style::PLAIN.reversed()
        } else {
            Style::PLAIN
        };
        let long = item.name.chars().count() > width;
        let mut details = wrap(&item.detail, room, usize::MAX).into_iter();
        let mut first: Row = Vec::new();
        match marks {
            Marks::None => first.push(span("   ", Style::PLAIN)),
            Marks::Choice => {
                let (mark, style) = if item.on {
                    ("●", Style::text(Color::Green))
                } else {
                    ("○", grey)
                };
                first.push(span("   ", Style::PLAIN));
                first.push(span(mark, style));
                first.push(span(" ", Style::PLAIN));
            }
        }
        let name = if long {
            item.name.clone()
        } else {
            format!("{:<width$}", item.name)
        };
        first.push(span(name, name_style));
        if !long && let Some(d) = details.next().filter(|d| !d.is_empty()) {
            first.push(span("  ", Style::PLAIN));
            first.push(span(d, grey));
        }
        out.push(first);
        for d in details.filter(|d| !d.is_empty()) {
            out.push(vec![span(" ".repeat(column), Style::PLAIN), span(d, grey)]);
        }
        if cursor {
            keep = out.len().saturating_sub(1);
        }
    }
    (out, keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(row: &Row) -> String {
        row.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn the_keys_move_the_highlight_and_stop_at_each_end() {
        let mut list = ListBox::default();
        assert_eq!(list.key(Key::Up, 30), Act::Moved);
        assert_eq!(list.at, 0);
        list.key(Key::Down, 30);
        assert_eq!(list.at, 1);
        list.key(Key::PageDown, 30);
        assert_eq!(list.at, 11);
        list.key(Key::End, 30);
        assert_eq!(list.at, 29);
        list.key(Key::Down, 30);
        assert_eq!(list.at, 29);
        list.key(Key::PageUp, 30);
        assert_eq!(list.at, 19);
        list.key(Key::Home, 30);
        assert_eq!(list.at, 0);
    }

    #[test]
    fn enter_picks_the_highlighted_item_and_esc_closes() {
        let mut list = ListBox::at(2);
        assert_eq!(list.key(Key::Enter, 5), Act::Picked(2));
        assert_eq!(list.key(Key::Escape, 5), Act::Closed);
        assert_eq!(list.key(Key::Char('x'), 5), Act::Ignored);
        // an empty list has nothing to pick
        assert_eq!(ListBox::default().key(Key::Enter, 0), Act::Ignored);
    }

    #[test]
    fn a_heading_starts_its_group_and_the_highlight_is_kept_in_sight() {
        let items = [
            Item {
                heading: Some("Font".into()),
                ..Item::new("font-name", "unifont")
            },
            Item::new("font-frits", "/etc/vtium/fonts"),
            Item {
                heading: Some("Blinking".into()),
                ..Item::new("blink", "on")
            },
        ];
        let (rows, keep) = rows(&items, 2, Marks::None, 80);
        let lines: Vec<String> = rows.iter().map(text).collect();
        assert_eq!(
            lines,
            [
                " Font",
                "   font-name   unifont",
                "   font-frits  /etc/vtium/fonts",
                "",
                " Blinking",
                "   blink       on",
            ]
        );
        assert_eq!(keep, 5);
        let reversed = |row: &Row| row.iter().any(|s| s.style == Style::PLAIN.reversed());
        assert!(rows.get(5).is_some_and(reversed));
        assert!(!rows.get(1).is_some_and(reversed));
    }

    #[test]
    fn a_choice_list_fills_the_circle_of_the_item_chosen_now() {
        let items = [
            Item::new("on", ""),
            Item {
                on: true,
                ..Item::new("off", "the default")
            },
        ];
        let (rows, _) = rows(&items, 0, Marks::Choice, 80);
        let lines: Vec<String> = rows.iter().map(text).collect();
        assert_eq!(lines, ["   ○ on ", "   ● off  the default"]);
    }

    #[test]
    fn a_long_detail_wraps_under_its_column_and_a_long_name_puts_it_below() {
        let long = "word ".repeat(30);
        let items = [
            Item::new("a", long.trim()),
            Item::new("a-name-longer-than-the-column", "detail"),
        ];
        let (rows, _) = rows(&items, 0, Marks::None, 40);
        let lines: Vec<String> = rows.iter().map(text).collect();
        assert!(lines.len() > 3, "{lines:#?}");
        // the names' column is 16 wide at 40 columns, after a margin of 3
        // and before a gap of 2
        let line = |i: usize| lines.get(i).map_or("", String::as_str);
        assert!(line(0).starts_with(&format!("   a{}word", " ".repeat(17))));
        assert!(
            line(1).starts_with(&format!("{}word", " ".repeat(21))),
            "{lines:#?}"
        );
        let name = lines.iter().position(|l| l.contains("a-name"));
        assert_eq!(
            name.map(|n| line(n.saturating_add(1)).trim()),
            Some("detail")
        );
        assert!(lines.iter().all(|l| l.chars().count() <= 40), "{lines:#?}");
    }
}
