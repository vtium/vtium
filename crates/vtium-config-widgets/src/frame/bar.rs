//! The action bar along the bottom row. Each action of the open screen is
//! a button that names its key: the key in brackets, bold, then the
//! action's name in reverse video, as `[Ctrl+S] Save`, and a click on
//! either does what the key does. `[F10] Menu` and `[q] Quit` sit at the
//! right.
//!
//! The buttons come in an order of priority. Where they do not all fit,
//! the bar leaves out the lowest first and puts `[.] More` last before
//! Menu and Quit, which opens the buttons left out as a small menu, so no
//! action is out of reach of the bar. `[Ctrl+S] Save` stays wherever Save
//! acts.
//!
//! References:
//! - ADR-1403 D11, "The action bar": the compact buttons, `[.] More`, and
//!   Save, Menu and Quit kept; keys in brackets, a chord in one bracket.
//! - ADR-1206 D3, the action bar: the bar's buttons are the labelled rows
//!   of the node the person is on, in order of priority, and a click on a
//!   button becomes the action of that button's row.
//! - ADR-1206 D4, the roles `bar-key` and `bar-button`.

use ratatui_core::layout::Rect;

use super::{Run, width};
use crate::contract::Row;
use crate::menu::{Entry, Item};
use crate::roles::Role;

/// Where a button sits on the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// With the screen's own actions, from the left, in order of priority.
    Left,
    /// At the right, as Menu and Quit are.
    Right,
}

/// A button of the action bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    /// The key, as the bar writes it inside its brackets, such as `Ctrl+S`.
    pub key: String,
    /// The action's name, such as `Save` or `Activate missing (3)`.
    pub label: String,
    /// The id of the key table's row the button acts as, such as
    /// `ctrl-s.view`, where it comes from one.
    pub action: Option<&'static str>,
    /// Whether the bar keeps the button at every width, as it keeps Save.
    pub keep: bool,
    /// Where the button sits.
    pub side: Side,
}

impl Button {
    /// A button of the screen's own actions, on `[key] label`.
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Button {
        Button {
            key: key.into(),
            label: label.into(),
            action: None,
            keep: false,
            side: Side::Left,
        }
    }

    /// The cells the button takes: its key in brackets, a space, its
    /// label and a space, the last two in reverse video.
    fn width(&self) -> u16 {
        width(&self.key)
            .saturating_add(width(&self.label))
            .saturating_add(4)
    }
}

/// The key and label of the button that opens what the bar has no room
/// for.
const MORE: (&str, &str) = (".", "More");

/// The action bar's buttons, in order of priority on each side.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bar {
    pub buttons: Vec<Button>,
}

impl Bar {
    /// The bar of a node's rows of the key table, in the order of the
    /// node's priority, as [`crate::contract::Node::buttons`] gives them.
    ///
    /// Each button names its row's key in the bar's form and its label
    /// with a capital, as in `[Enter] Open`. Rows that share a label act
    /// as one button, the first, so the five Enter rows of a look's card
    /// make one `[Enter] Choose`. F10's row and `q`'s go to the right as
    /// Menu and Quit. Where the node has no `q`, as while a value is
    /// typed, Ctrl+C's Quit is the Quit at the right, and elsewhere it is
    /// left out, since `[q] Quit` does the same. The row of `.` is
    /// left out, because the bar shows `[.] More` itself whenever a button
    /// does not fit. Save is kept at every width.
    pub fn from_rows<'a>(rows: impl IntoIterator<Item = &'a Row>) -> Bar {
        let rows: Vec<&Row> = rows.into_iter().collect();
        let has_q = rows.iter().any(|r| r.bar_key == "q");
        let mut buttons: Vec<Button> = Vec::new();
        for row in rows {
            let quit = row.label.eq_ignore_ascii_case("quit");
            // Ctrl+C's Quit is the same as q's where both act.
            if row.bar_key == MORE.0 || row.bar_key == "Ctrl+C" && quit && has_q {
                continue;
            }
            let label = capital(row.label);
            let same = |b: &Button| b.label.to_lowercase() == label.to_lowercase();
            if buttons.iter().any(same) {
                continue;
            }
            let right = matches!(row.bar_key, "F10" | "q") || row.bar_key == "Ctrl+C" && quit;
            buttons.push(Button {
                key: row.bar_key.to_owned(),
                label,
                action: Some(row.id),
                keep: row.bar_key == "Ctrl+S",
                side: if right { Side::Right } else { Side::Left },
            });
        }
        // Menu comes before Quit at the right, whatever the rows' order.
        buttons.sort_by_key(|b| {
            (
                b.side == Side::Right && b.key != "F10",
                b.side == Side::Right,
            )
        });
        Bar { buttons }
    }

    /// The small menu that `[.] More` opens: each button of `hidden`, by
    /// its index in [`Bar::buttons`], with its key in the menu's column of
    /// shortcuts, written bare.
    pub fn more_menu(&self, hidden: &[usize]) -> Vec<Entry> {
        hidden
            .iter()
            .filter_map(|&i| self.buttons.get(i))
            .map(|b| Entry::Item(Item::new(&b.label).shortcut(&b.key)))
            .collect()
    }
}

/// `label` with its first letter a capital, as the bar names an action.
fn capital(label: &str) -> String {
    let mut chars = label.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// The bar as it is drawn, and its places.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BarLayout {
    pub runs: Vec<Run>,
    pub shown: Vec<(usize, Rect)>,
    pub more: Option<Rect>,
    pub hidden: Vec<usize>,
}

/// The cells of a group of buttons, a space apart.
fn group_width(widths: impl Iterator<Item = u16>) -> u16 {
    let mut n = 0u16;
    let mut count = 0u16;
    for w in widths {
        n = n.saturating_add(w);
        count = count.saturating_add(1);
    }
    n.saturating_add(count.saturating_sub(1))
}

/// The bar in `row`.
pub(crate) fn layout(row: Rect, bar: &Bar) -> BarLayout {
    let all = &bar.buttons;
    let more = Button::new(MORE.0, MORE.1);
    let mut left: Vec<usize> = (0..all.len())
        .filter(|&i| all.get(i).is_some_and(|b| b.side == Side::Left))
        .collect();
    let mut right: Vec<usize> = (0..all.len())
        .filter(|&i| all.get(i).is_some_and(|b| b.side == Side::Right))
        .collect();
    let mut hidden: Vec<usize> = Vec::new();
    let mut with_more = false;
    let w = |i: &usize| all.get(*i).map_or(0, Button::width);
    let fits = |left: &[usize], right: &[usize], with_more: bool| {
        let l = group_width(left.iter().map(w).chain(with_more.then(|| more.width())));
        let r = group_width(right.iter().map(w));
        let gap = u16::from(l > 0 && r > 0);
        1u16.saturating_add(l).saturating_add(gap).saturating_add(r) <= row.width
    };
    while !fits(&left, &right, with_more) {
        // The lowest button that is not kept goes first, then the kept
        // ones, then the right's, and More last of all on a screen too
        // narrow for anything.
        let drop = left
            .iter()
            .rposition(|i| all.get(*i).is_some_and(|b| !b.keep))
            .or_else(|| left.len().checked_sub(1));
        if let Some(at) = drop {
            hidden.push(left.remove(at));
            with_more = true;
        } else if !right.is_empty() {
            hidden.push(right.remove(0));
            with_more = true;
        } else if with_more {
            with_more = false;
            break;
        } else {
            break;
        }
    }
    // More lists what is left out in the bar's own order of priority.
    hidden.sort_unstable();
    let mut runs = Vec::new();
    let mut shown = Vec::new();
    let mut put = |x: u16, b: &Button| -> Rect {
        runs.push(Run::new(x, row.y, format!("[{}]", b.key), &[Role::BarKey]));
        let kx = x.saturating_add(width(&b.key)).saturating_add(2);
        runs.push(Run::new(
            kx,
            row.y,
            format!(" {} ", b.label),
            &[Role::BarButton],
        ));
        Rect::new(x, row.y, b.width(), 1)
    };
    let mut x = row.x.saturating_add(1);
    for i in &left {
        if let Some(b) = all.get(*i) {
            shown.push((*i, put(x, b)));
            x = x.saturating_add(b.width()).saturating_add(1);
        }
    }
    let more_place = with_more.then(|| put(x, &more));
    let r = group_width(right.iter().map(w));
    let mut x = row.right().saturating_sub(r);
    for i in &right {
        if let Some(b) = all.get(*i) {
            shown.push((*i, put(x, b)));
            x = x.saturating_add(b.width()).saturating_add(1);
        }
    }
    BarLayout {
        runs,
        shown,
        more: more_place,
        hidden,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_takes_a_capital() {
        assert_eq!(capital("add path"), "Add path");
        assert_eq!(capital("Save"), "Save");
        assert_eq!(capital(""), "");
        assert_eq!(capital("état"), "État");
    }

    #[test]
    fn a_button_is_its_key_in_brackets_and_its_label_with_a_space_each_side() {
        assert_eq!(Button::new("Ctrl+S", "Save").width(), 14);
        assert_eq!(group_width([14u16, 10].into_iter()), 25);
        assert_eq!(group_width(std::iter::empty()), 0);
    }
}
