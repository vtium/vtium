//! The File menu: what a person does with the configuration as a
//! document. Undo and Redo name the step they would take, and are greyed
//! out with "nothing to undo" or "nothing to redo" where there is none.
//! The shortcuts `n`, `v`, `r` and `o` show only where the open screen's
//! rows give them, which is the Flow tab, since on the Looks tab `n` and
//! `r` make and remove a look.
//!
//! References:
//! - ADR-1403 D11, "The menus": File's items, letters, shortcuts and
//!   separators.
//! - ADR-1403 D11, "Undo and redo": the menu names each step and greys an
//!   item with nothing to act on.

use vtium_config_widgets::menu::Item;

use super::{Choice, Context, Line, NOT_BUILT, Submenu, Then};
use crate::nav::levels;

/// Undo with nothing to undo.
pub const NOTHING_TO_UNDO: &str = "nothing to undo";

/// Redo with nothing to redo.
pub const NOTHING_TO_REDO: &str = "nothing to redo";

/// The File menu's lines.
pub fn lines(cx: &Context<'_>) -> Vec<Line> {
    let history = cx.shared.documents.history();
    let undo = history.to_undo().map(|s| s.name.clone());
    let redo = history.to_redo().map(|s| s.name.clone());
    let step = |verb: &str, name: &Option<String>| match name {
        Some(name) => format!("{verb} {name}"),
        None => verb.to_owned(),
    };
    let mut undo_item = Item::new(step("Undo", &undo))
        .letter('u')
        .shortcut("Ctrl+Z");
    if undo.is_none() {
        undo_item = undo_item.greyed(NOTHING_TO_UNDO);
    }
    let mut redo_item = Item::new(step("Redo", &redo))
        .letter('d')
        .shortcut("Ctrl+Y");
    if redo.is_none() {
        redo_item = redo_item.greyed(NOTHING_TO_REDO);
    }
    vec![
        acting(cx, Item::new("New…").letter('n'), "n.view.flow"),
        Line::Choice(Choice {
            item: Item::new("Open").letter('o').submenu(),
            then: Then::Sub(Submenu::Open),
        }),
        Line::Separator,
        stepping(undo_item, "ctrl-z.view"),
        stepping(redo_item, "ctrl-y.view"),
        Line::Separator,
        Line::Choice(Choice::acting(
            Item::new("Save…").letter('s').shortcut("Ctrl+S"),
            "ctrl-s.view",
        )),
        acting(cx, Item::new("Show changes").letter('c'), "v.view.flow"),
        Line::Choice(Choice::acting(
            Item::new("Edit as text…").letter('e'),
            "e.layer.changes",
        )),
        Line::Separator,
        acting(cx, Item::new("Revert to saved…").letter('r'), "r.view.flow"),
        Line::Choice(Choice {
            item: Item::new("History").letter('h').submenu(),
            then: Then::Sub(Submenu::History),
        }),
        Line::Separator,
        Line::Choice(Choice::acting(
            Item::new("Quit").letter('q').shortcut("q"),
            "q.view",
        )),
    ]
}

/// File › Open's lines: each configuration, the open one marked, and
/// Manage configurations… last.
pub fn open(cx: &Context<'_>) -> Vec<Line> {
    let open = cx.shared.document().name();
    let names = cx
        .shared
        .documents
        .desk()
        .configurations()
        .unwrap_or_default();
    let mut lines: Vec<Line> = names
        .iter()
        .map(|name| {
            let mut item = Item::new(name.as_str());
            if name == open {
                item = item.check(true);
            }
            Line::Choice(Choice::acting(item, "enter.layer.menu.open"))
        })
        .collect();
    lines.push(Line::Separator);
    lines.push(Line::Choice(Choice::acting(
        Item::new("Manage configurations…").letter('m'),
        "enter.layer.menu.open",
    )));
    lines
}

/// File › History's lines: the versions installed before, newest first,
/// by the date and time each was installed.
pub fn history(cx: &Context<'_>) -> Vec<Line> {
    let versions = cx.shared.documents.desk().versions().unwrap_or_default();
    versions
        .iter()
        .map(|v| {
            Line::Choice(Choice::acting(
                Item::new(when(v.when)),
                "enter.layer.menu.history",
            ))
        })
        .collect()
}

/// An item whose shortcut shows only where the open screen lists the row
/// that the shortcut is, such as `n` for New on the Flow tab.
fn acting(cx: &Context<'_>, item: Item, row: &'static str) -> Line {
    let item = match levels::listed(cx.node, row) {
        Some(r) => item.shortcut(r.bar_key),
        None => item,
    };
    Line::Choice(Choice::acting(item, row))
}

/// Undo's or Redo's line, already greyed out where there is no step; the
/// item is not built while the shell does not handle its row.
fn stepping(item: Item, row: &'static str) -> Line {
    let mut choice = Choice::acting(item.clone(), row);
    if item.greyed.is_some() && choice.item.greyed.as_deref() == Some(NOT_BUILT) {
        choice.item = item;
    }
    Line::Choice(choice)
}

/// A time in seconds since 1970 as `2026-11-27 20:24`, in UTC.
pub fn when(seconds: u64) -> String {
    let days = seconds / 86_400;
    let rest = seconds % 86_400;
    let (y, m, d) = civil(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rest / 3_600,
        rest % 3_600 / 60
    )
}

/// The year, month and day of the day `days` after 1970-01-01, by Howard
/// Hinnant's `civil_from_days`.
fn civil(days: u64) -> (u64, u64, u64) {
    let z = days.saturating_add(719_468);
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + u64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_is_written_as_its_date_and_minute() {
        assert_eq!(when(0), "1970-01-01 00:00");
        // 2026-11-27 20:24:00 UTC
        assert_eq!(when(1_795_811_040), "2026-11-27 20:24");
        assert_eq!(when(951_782_400), "2000-02-29 00:00");
    }
}
