//! The Help tab driven by its events: the page of keys from the contract,
//! About, and how the page scrolls. `tui.rs` says how the tests drive the
//! TUI, and `tui_common/` holds the helpers.

mod tui_common;
use tui_common::*;
use vtium_config::tui::flow::Item;
use vtium_config::tui::input::Key;
use vtium_config::tui::model::{App, View};

#[test]
fn the_help_view_draws_the_keys_from_the_contract_and_about_from_its_source() {
    let mut a = app(80, 25);
    assert_eq!(a.flow.at(), Item::Display(0));
    press(&mut a, Key::BackTab);
    assert_eq!(a.nav.view, View::Help);
    let joined = |a: &App| text(a).lines().map(str::trim).collect::<Vec<_>>().join(" ");
    assert!(
        joined(&a).contains("The Flow view: CONFIGURATION, NEW PATH and PARTS"),
        "{}",
        text(&a)
    );
    press(&mut a, Key::End);
    let end = joined(&a);
    // a command's line wider than the screen goes on under it
    assert!(
        end.contains("report [--save[=PATH]] [--lines N] [--previous-boot]"),
        "{end}"
    );
    assert!(
        end.contains("collect what a bug report needs into one document"),
        "{end}"
    );
    assert!(text(&a).lines().all(|l| l.chars().count() <= 80));
    press(&mut a, Key::Right);
    assert!(a.help.about());
    assert!(
        joined(&a).contains("vtium-config sets up the vtium console"),
        "{}",
        text(&a)
    );
}

/// The Help page is generated from the contract: it shows, for each
/// supported node of the interface tree, what the node is, then every
/// key of the node's help with what the key does, in the contract's
/// order, at each width the TUI draws.
///
/// References:
/// - ADR-1403 D7, "The contract": the Help tab is generated from the
///   key table and the tree.
#[test]
fn the_help_page_shows_every_key_of_every_node_the_contract_gives() {
    use vtium_config::tui::contract::NODES;
    let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    for (cols, rows) in [(80, 25), (100, 30)] {
        let mut a = app(cols, rows);
        press(&mut a, Key::BackTab);
        assert_eq!(a.nav.view, View::Help);
        let page = words(
            &a.help
                .page(&a.shared, a.size)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" "),
        );
        let mut shown = 0;
        for node in NODES.iter().filter(|n| n.supported && !n.help.is_empty()) {
            let what = words(node.what);
            let mut at = page
                .find(&what)
                .unwrap_or_else(|| panic!("{cols}: the page leaves out {what:?}"));
            for (keys, does) in node.help {
                let entry = words(&format!("{keys} {does}"));
                let found = page.get(at..).and_then(|rest| rest.find(&entry));
                let Some(i) = found else {
                    panic!("{cols}: {}'s help leaves out {entry:?}", node.id);
                };
                at = at.saturating_add(i).saturating_add(entry.len());
                shown += 1;
            }
        }
        assert!(shown > 100, "{cols}: only {shown} keys");
    }
}

/// Help's page scrolls by a line with Up and Down, and by at most the
/// rows the screen shows with PgUp and PgDn, which stop at the start
/// of an entry. Its top row stops where the page's last row is at the
/// bottom of the screen, so Up moves the page at once after End.
///
/// References:
/// - ADR-1403 D7, "The contract": the key table's rows for the Help
///   view's keys.
#[test]
fn the_help_page_scrolls_by_its_window_and_up_moves_it_at_once_from_the_bottom() {
    let mut a = app(80, 25);
    press(&mut a, Key::BackTab);
    let (rows, room) = vtium_config::tui::layout::help_window(&a);
    let len = rows.len;
    assert!(len > room.saturating_mul(2), "{len} rows, {room} shown");
    // the screen's first row of the page, the row under the tab bar's
    // rule, is the page's row `at`
    let top = |a: &App, at: usize| {
        let rows = a.help.page(&a.shared, a.size);
        let want = rows.get(at).map(ToString::to_string).unwrap_or_default();
        let got = text(a).lines().nth(3).unwrap_or_default().to_owned();
        assert_eq!(got.trim_end(), want.trim_end(), "row {at}");
    };
    press(&mut a, Key::PageDown);
    let first = a.help.top();
    assert!(
        first > 0 && first <= room,
        "{first} after PgDn, {room} shown"
    );
    top(&a, first);
    press(&mut a, Key::PageDown);
    let second = a.help.top();
    assert!(
        second > first && second <= first.saturating_add(room),
        "{second} after PgDn from {first}"
    );
    top(&a, second);
    press(&mut a, Key::PageUp);
    let back = a.help.top();
    assert!(
        back < second && back >= second.saturating_sub(room),
        "{back} after PgUp from {second}"
    );
    top(&a, back);
    press(&mut a, Key::End);
    top(&a, len.saturating_sub(room));
    press(&mut a, Key::Down);
    top(&a, len.saturating_sub(room));
    press(&mut a, Key::Up);
    top(&a, len.saturating_sub(room).saturating_sub(1));
    press(&mut a, Key::Home);
    top(&a, 0);
}

/// PgDn and PgUp never cut an entry of the Help page across two
/// screens: no screen starts on a row that goes on from the row above
/// it. Such a row is a key's meaning wrapped under its key, indented
/// six columns or more, or a further row of a paragraph or of a list
/// of keys, at the same indent as the row above it. A key's own row,
/// indented three columns, starts an entry of its own. The last screen
/// is the exception: it ends on the page's last row, and the entry it
/// cuts at its top showed whole on the screen before it.
#[test]
fn page_up_and_page_down_keep_each_entry_of_the_help_page_on_one_screen() {
    let indent = |l: &str| l.len().saturating_sub(l.trim_start().len());
    let goes_on = |above: &str, row: &str| {
        !above.trim().is_empty()
            && !row.trim().is_empty()
            && (indent(row) >= 6 || (indent(row) == indent(above) && indent(row) != 3))
    };
    for (cols, rows) in [(80, 25), (100, 30)] {
        let mut a = app(cols, rows);
        press(&mut a, Key::BackTab);
        let page: Vec<String> = a
            .help
            .page(&a.shared, a.size)
            .iter()
            .map(ToString::to_string)
            .collect();
        let (_, room) = vtium_config::tui::layout::help_window(&a);
        let last = page.len().saturating_sub(room);
        let mut tops = vec![a.help.top()];
        for key in [Key::PageDown, Key::PageUp] {
            for _ in 0..page.len() {
                press(&mut a, key);
                if tops.last() == Some(&a.help.top()) {
                    break;
                }
                tops.push(a.help.top());
            }
        }
        assert!(tops.len() > 4, "{cols}×{rows}: {tops:?}");
        assert!(tops.contains(&last), "{cols}×{rows}: {tops:?}");
        for t in tops.into_iter().filter(|t| *t > 0 && *t != last) {
            let (above, row) = (&page[t - 1], &page[t]);
            assert!(
                !goes_on(above, row),
                "{cols}×{rows}: a screen starts at row {t}, inside an entry:\n{above}\n{row}"
            );
        }
    }
}
