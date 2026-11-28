//! The Help tab's screens: each topic, the `?` search and a scrolled
//! topic, drawn with the frame at 100×30 and 80×25 on the screen map's
//! machine and on a large one. Each screen is checked for rows of the
//! terminal's width, for what it must still show at 80×25, and for marks
//! that read without color, and is kept as a snapshot under
//! `tests/snapshots/`. `VTIUM_CONFIG_BLESS=1 cargo test -p
//! vtium-config-tab-help --test screens` writes the snapshots again.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Help bullet: the topics at the
//!   left and the highlighted topic at the right.
//! - ADR-1206 D10: designed for 100×30, usable at 80×25, and readable in
//!   sixteen colors, under `NO_COLOR` and on a monochrome console.

mod common;

use common::{
    SIZES, after, check_rows, check_without_color, fixtures, open_search, rows_of, screen,
    search_keys, snapshot, snapshot_text, tab, typed,
};
use ratatui_core::buffer::Buffer;
use ratatui_core::style::Modifier;
use vtium_config_core::ui::{Action, Shared};
use vtium_config_tab_help::HelpTab;
use vtium_config_tab_help::page::Layout;
use vtium_config_tab_help::topics::TOPICS;
use vtium_config_widgets::tab::Tab;

/// A topic's name as a snapshot's file names it.
fn slug(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// Draws the tab that `set` puts in place on every fixture at every size,
/// checks each screen, and returns the snapshot of all four.
fn screens(
    set: &dyn Fn(&mut HelpTab, &Shared),
    what: &str,
    must: &dyn Fn(&[String], &str),
) -> String {
    let mut text = String::new();
    for (fixture, shared) in fixtures() {
        for (cols, rows) in SIZES {
            let mut color = tab(true);
            let mut mono = tab(false);
            set(&mut color, &shared);
            set(&mut mono, &shared);
            let c = screen(&color, &shared, cols, rows, true);
            let m = screen(&mono, &shared, cols, rows, false);
            let what = format!("{what} {fixture} {cols}x{rows}");
            check_rows(&c, cols, rows, &what);
            check_without_color(&c, &m, &what);
            must(&rows_of(&m), &what);
            highlighted_in_reverse(&m, TOPICS[mono.topic()].name, &what);
            text.push_str(&format!("== {fixture} {cols}x{rows} ==\n"));
            text.push_str(&snapshot_text(&m));
        }
    }
    text
}

/// Without color, one row of the column of topics is in reverse video, the
/// highlighted topic's, so the highlight reads on a monochrome console.
fn highlighted_in_reverse(mono: &Buffer, name: &str, what: &str) {
    let viewer = common::viewer(mono.area.width, mono.area.height);
    let column = Layout::of(viewer).topics;
    let reversed: Vec<u16> = (column.top()..column.bottom())
        .filter(|&y| {
            (column.left()..column.right())
                .any(|x| mono[(x, y)].modifier.contains(Modifier::REVERSED))
        })
        .collect();
    let rows = rows_of(mono);
    match reversed.as_slice() {
        [] if rows[2].contains("No topic matches") => {}
        [y] => {
            assert!(
                rows[usize::from(*y)].contains(&format!(" {name} ")),
                "{what}: the row in reverse video is not {name}: {}",
                rows[usize::from(*y)]
            );
            for x in column.left() + 1..column.right() {
                assert!(
                    mono[(x, *y)].modifier.contains(Modifier::REVERSED),
                    "{what}: the highlight stops at {x}"
                );
            }
        }
        _ => panic!("{what}: rows {reversed:?} of the topics are in reverse video"),
    }
}

/// Every topic's name shows in the column at the left, and the highlighted
/// one heads the right pane.
fn shows_every_topic(rows: &[String], topic: usize, what: &str) {
    for t in TOPICS {
        assert!(
            rows.iter()
                .any(|r| r.contains(&format!("│ {} ", t.name))
                    || r.contains(&format!(" {} ", t.name))),
            "{what}: no topic {}",
            t.name
        );
    }
    let name = TOPICS[topic].name;
    assert!(
        rows[2].contains(&format!("│ {name}")),
        "{what}: the right pane is not headed {name}: {}",
        rows[2]
    );
}

#[test]
fn each_topic_at_both_sizes_with_both_fixtures() {
    for (i, t) in TOPICS.iter().enumerate() {
        let text = screens(
            &|tab, shared| {
                tab.update(shared, Action::item("6.view", i));
            },
            t.name,
            &|rows, what| {
                shows_every_topic(rows, i, what);
                // the topic's first line of text or keys shows under its name
                assert!(
                    rows[4].trim_end().len() > 25,
                    "{what}: the page's first line is empty: {}",
                    rows[4]
                );
            },
        );
        snapshot(&format!("help-{}", slug(t.name)), &text);
    }
}

#[test]
fn a_long_topic_scrolls_by_a_window_and_says_which_lines_show() {
    let text = screens(
        &|tab, shared| {
            tab.update(shared, Action::item("6.view", 8));
            after(tab, shared, &["pgdn.view.help"]);
        },
        "Commands, scrolled",
        &|rows, what| {
            assert!(
                rows[2].contains("lines ")
                    && rows[2].contains(" of ")
                    && rows[2].contains("[PgUp] [PgDn]"),
                "{what}: {}",
                rows[2]
            );
            assert!(
                !rows[2].contains("lines 1 to"),
                "{what}: not scrolled: {}",
                rows[2]
            );
        },
    );
    snapshot("help-commands-scrolled", &text);
}

#[test]
fn the_search_keeps_the_lines_that_match_under_their_topic() {
    let text = screens(
        &|tab, shared| {
            let mut search = open_search(tab);
            search_keys(tab, shared, &mut search, &typed("*activ"));
        },
        "search *activ",
        &|rows, what| {
            assert!(
                rows[2].contains("│ Fonts") && rows[2].contains("match"),
                "{what}: {}",
                rows[2]
            );
            assert!(
                rows.iter().any(|r| r.starts_with(" ? *activ_")),
                "{what}: no search row"
            );
            for r in &rows[4..rows.len() - 4] {
                let right = r.split_once('│').map_or("", |(_, b)| b).trim();
                assert!(
                    right.is_empty()
                        || right.to_lowercase().contains("activ")
                        || r.starts_with(' '),
                    "{what}: a line that does not match: {r}"
                );
            }
        },
    );
    snapshot("help-search-activ", &text);
}
