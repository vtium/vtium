//! How a topic is laid out for a width: keys in a column with what they do
//! wrapped under it, the grid kept to whole cells, the logo only where it
//! fits and color shows, and a window's step that keeps an entry whole.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Help bullet: PgUp and PgDn
//!   scroll a topic longer than the pane.
//! - ADR-1206 D10: usable at 80×25, and readable without color.

use vtium_config_tab_help::page::{self, Look, Page};
use vtium_config_tab_help::topics::{Cell, Entry};
use vtium_config_widgets::canvas::cells;

/// The text of each line of `page`.
fn text(page: &Page) -> Vec<String> {
    page.lines
        .iter()
        .map(|l| l.iter().map(|r| r.text.as_str()).collect())
        .collect()
}

#[test]
fn a_key_s_meaning_wraps_under_its_column_and_a_wide_key_takes_its_own_line() {
    let entries = [
        Entry::Key {
            keys: vec!["[Up]".to_owned()],
            means: "Moves to the topic above, and the page starts at its top again.",
            rows: vec!["up.view.help"],
        },
        Entry::Key {
            keys: vec!["Click on a button of the action bar".to_owned()],
            means: "Does what the button's key does.",
            rows: vec!["click-button.any"],
        },
    ];
    let page = page::lay(&entries, None, 40, true);
    assert_eq!(
        text(&page),
        [
            "  [Up]  Moves to the topic above, and",
            "        the page starts at its top",
            "        again.",
            "  Click on a button of the action bar",
            "        Does what the button's key does.",
        ]
    );
    assert_eq!(page.starts, [0, 3]);
    for line in text(&page) {
        assert!(cells(&line) <= 40, "{line}");
    }
}

#[test]
fn the_grid_keeps_a_cell_whole_where_it_fits_on_a_line() {
    let cell = |keys: &[&str], label: &str| Cell {
        keys: keys.iter().map(|k| (*k).to_owned()).collect(),
        label: label.to_owned(),
        rows: vec![],
    };
    let entries = [Entry::Grid(vec![
        cell(&["[F10]"], "Menu"),
        cell(&["[q]", "[Ctrl+C]"], "Quit"),
        cell(&["[Shift+Tab]"], "View before"),
    ])];
    // 29 cells after the indent hold the first two cells and the gap
    let page = page::lay(&entries, None, 31, true);
    assert_eq!(
        text(&page),
        [
            "  [F10] Menu  [q] [Ctrl+C] Quit",
            "  [Shift+Tab] View before"
        ]
    );
}

#[test]
fn the_logo_shows_in_its_own_colors_only_where_it_fits_and_color_shows() {
    let entries = [Entry::Art("\x1b[38;5;208m▄▀▀▄\x1b[0m".to_owned())];
    let page = page::lay(&entries, None, 20, true);
    assert_eq!(text(&page), ["▄▀▀▄"]);
    assert!(matches!(page.lines[0][0].look, Look::Own(_)));
    assert!(page::lay(&entries, None, 20, false).lines.is_empty());
    assert!(page::lay(&entries, None, 3, true).lines.is_empty());
}

#[test]
fn a_window_s_step_keeps_an_entry_whole_and_stops_at_the_last_window() {
    // six entries of three lines each: 18 lines
    let page = Page {
        lines: vec![Vec::new(); 18],
        starts: vec![0, 3, 6, 9, 12, 15],
    };
    // a window of 7 cuts the entry at 6, so PgDn starts there
    assert_eq!(page::step(&page, 0, 7, true), 6);
    assert_eq!(page::step(&page, 6, 7, true), 11);
    assert_eq!(page::step(&page, 11, 7, true), 11);
    // PgUp starts at the first entry that fits whole above the top
    assert_eq!(page::step(&page, 11, 7, false), 6);
    assert_eq!(page::step(&page, 6, 7, false), 0);
    assert_eq!(page::step(&page, 0, 7, false), 0);
    // a page that fits never scrolls
    assert_eq!(page::step(&page, 0, 30, true), 0);
}
