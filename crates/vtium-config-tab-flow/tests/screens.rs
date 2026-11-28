//! The Flow tab's screens, drawn into a buffer in the frame the shell
//! draws around them, at 100×30 and at 80×25, on the screen map's machine,
//! on the large machine and on the catalog's six example pages. Each
//! screen's text is kept as a snapshot under tests/snapshots/, and each is
//! drawn again without color, where every mark a color carries still shows
//! as a word, a glyph or an attribute.
//!
//! References:
//! - ADR-1206 D10: every screen at 100×30 and 80×25, with the screen
//!   map's fixture and a large one, readable without color.
//! - ADR-1403 D7, "Colors, borders and lines": every state also has its
//!   words, so the view works without color.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "a test fails by panicking"
)]

mod common;

use common::*;
use ratatui_core::buffer::Buffer;
use ratatui_core::style::Modifier;
use vtium_config_core::fixture;
use vtium_config_core::ui::Shared;
use vtium_config_tab_flow::page::layout::{Obj, Slot};
use vtium_config_tab_flow::{FlowTab, Level};
use vtium_config_widgets::tab::Tab;

/// Draws the screen at `cols`×`rows`, after the tab knows its viewer at
/// that size, in color and without; checks both, and keeps the text as
/// the snapshot `name`.
fn check(shared: &Shared, tab: &mut FlowTab, name: &str, cols: u16, rows: u16) -> Buffer {
    // a first draw gives the tab its viewer, as the shell's first frame
    // does before any key
    let _ = screen(shared, tab, cols, rows, true);
    let colored = screen(shared, tab, cols, rows, true);
    check_rows(&colored, cols, rows);
    tab.set_color(false);
    let plain = screen(shared, &*tab, cols, rows, false);
    tab.set_color(true);
    check_rows(&plain, cols, rows);
    check_no_color(&plain);
    assert_eq!(
        text_of(&colored),
        text_of(&plain),
        "{name}: the text differs without color"
    );
    snapshot(&format!("{name}-{cols}x{rows}"), &colored);
    plain
}

/// A fresh tab drawn once at `cols`×`rows`, so its keys land on that
/// screen.
fn tab_at(shared: &Shared, cols: u16, rows: u16) -> FlowTab {
    let tab = FlowTab::new(true);
    let _ = screen(shared, &tab, cols, rows, true);
    tab
}

#[test]
fn the_page_shows_its_three_sections_and_the_highlight_on_each_machine() {
    for (name, shared) in [
        ("page-workstation", fixture::padnoir::shared()),
        ("page-large", fixture::large::shared()),
    ] {
        for (cols, rows) in SIZES {
            let mut tab = tab_at(&shared, cols, rows);
            let plain = check(&shared, &mut tab, name, cols, rows);
            let text = text_of(&plain);
            for must in [
                "CONFIGURATION",
                "DISPLAYS",
                "TERMINALS",
                "INPUTS",
                "[Space] Pick",
                "[Enter] Open",
            ] {
                assert!(
                    text.contains(must),
                    "{name} {cols}x{rows} lacks {must}:\n{text}"
                );
            }
            // the highlight shows without color: a heavy border, and a bold,
            // underlined title inside it
            let (x, y) = find(&plain, "┏").expect("a heavy border");
            let title = plain[(x + 2, y + 1)].style().add_modifier;
            assert!(
                title.contains(Modifier::BOLD | Modifier::UNDERLINED),
                "{name}: the highlighted title is {title:?}"
            );
        }
    }
}

#[test]
fn the_six_example_pages_draw_as_the_catalog_does() {
    for (name, mut shared) in examples::all() {
        for (cols, rows) in SIZES {
            let mut tab = tab_at(&shared, cols, rows);
            shared.message = None;
            if name == "vt-clash" {
                // eDP-1 is in console on VT 1, and NEW PATH's terminal runs
                // on every VT, so Space on the part says why it cannot join
                assert!(go_to(&shared, &mut tab, "eDP-1", "PARTS"));
                let said = press(&shared, &mut tab, "space.view.flow");
                with_message(&mut shared, &said);
                let line = shared
                    .message
                    .as_ref()
                    .map(|m| m.line.clone())
                    .unwrap_or_default();
                assert_eq!(
                    line,
                    "eDP-1 is already in the terminal console on VT 1: one path for each VT."
                );
            }
            let plain = check(&shared, &mut tab, &format!("example-{name}"), cols, rows);
            let text = text_of(&plain);
            let must = if name == "vt-clash" {
                "already in the terminal console"
            } else {
                "CONFIGURATION"
            };
            assert!(text.contains(must), "{name}:\n{text}");
        }
    }
}

#[test]
fn the_large_page_scrolls_to_its_last_display() {
    let shared = fixture::large::shared();
    let last = Obj::Doc(
        "gpu9".to_owned(),
        Slot::Display("connector:card9/DP-4".to_owned()),
    );
    for (cols, rows) in SIZES {
        let mut tab = tab_at(&shared, cols, rows);
        for _ in 0..200 {
            if tab.state().at.as_ref() == Some(&last) {
                break;
            }
            press(&shared, &mut tab, "down.view.flow");
        }
        assert_eq!(
            tab.state().at.as_ref(),
            Some(&last),
            "Down never reached gpu9's last display"
        );
        assert!(tab.state().top > 0, "the page did not scroll");
        let plain = check(&shared, &mut tab, "page-large-last-display", cols, rows);
        let text = text_of(&plain);
        assert!(
            text.contains("┃ DP-4"),
            "the last display is not in sight:\n{text}"
        );
        assert!(
            text.contains("gpu9"),
            "its terminal is not in sight:\n{text}"
        );
    }
}

#[test]
fn every_box_of_the_large_page_comes_into_sight_as_the_highlight_reaches_it() {
    let shared = fixture::large::shared();
    for (cols, rows) in SIZES {
        let mut tab = tab_at(&shared, cols, rows);
        let mut seen = 0usize;
        let mut before = None;
        tab.set_color(false);
        for _ in 0..400 {
            let plain = screen(&shared, &tab, cols, rows, false);
            assert!(
                text_of(&plain).contains('┏'),
                "the highlighted box {:?} is out of sight",
                tab.state().at
            );
            seen += 1;
            press(&shared, &mut tab, "right.view.flow");
            if tab.state().at == before {
                break;
            }
            before = tab.state().at.clone();
        }
        assert!(seen > 150, "Right reached only {seen} boxes");
    }
}

#[test]
fn a_parts_detail_shows_its_label_and_its_choices() {
    for (name, shared, part) in [
        ("detail-workstation", fixture::padnoir::shared(), "DP-8"),
        ("detail-large", fixture::large::shared(), "Keyboard 07"),
    ] {
        for (cols, rows) in SIZES {
            let mut tab = tab_at(&shared, cols, rows);
            assert!(
                go_to(&shared, &mut tab, part, "PARTS"),
                "{part} is not in PARTS"
            );
            press(&shared, &mut tab, "enter.view.flow");
            assert_eq!(tab.node(), "flow.detail");
            let plain = check(&shared, &mut tab, name, cols, rows);
            let text = text_of(&plain);
            assert!(text.contains(&format!("Flow › {part}")), "{text}");
            assert!(text.contains("Label"), "{text}");
            assert!(text.contains("○ "), "no choice:\n{text}");
            // the label's line is highlighted in reverse video
            assert!(has_modifier(&plain, Modifier::REVERSED));
            // the cursor on the last choice keeps it in sight
            press(&shared, &mut tab, "end.layer.flow-detail");
            assert_eq!(tab.node(), "flow.detail.choices");
            let plain = check(
                &shared,
                &mut tab,
                &format!("{name}-last-choice"),
                cols,
                rows,
            );
            assert!(has_modifier(&plain, Modifier::REVERSED));
        }
    }
}

#[test]
fn a_part_of_a_path_says_what_matches_it_and_where_it_goes() {
    for (name, shared, part) in [
        (
            "info-workstation",
            fixture::padnoir::shared(),
            "All displays",
        ),
        ("info-large", fixture::large::shared(), "Mouse 42"),
    ] {
        for (cols, rows) in SIZES {
            let mut tab = tab_at(&shared, cols, rows);
            assert!(go_to(&shared, &mut tab, part, "CONFIGURATION"), "{part}");
            press(&shared, &mut tab, "enter.view.flow");
            assert_eq!(tab.node(), "flow.info");
            let plain = check(&shared, &mut tab, name, cols, rows);
            let text = text_of(&plain);
            for must in ["Matched by", "Where it goes", "[e]", "[Esc] Back"] {
                assert!(text.contains(must), "{name} lacks {must}:\n{text}");
            }
        }
    }
}

#[test]
fn the_other_inputs_list_each_with_the_rule_that_catches_it() {
    for (name, shared, count) in [
        ("others-workstation", fixture::padnoir::shared(), 1),
        ("others-large", large_with_others(40), 40),
    ] {
        for (cols, rows) in SIZES {
            let mut tab = tab_at(&shared, cols, rows);
            let title = if count == 1 {
                "1 other input".to_owned()
            } else {
                format!("{count} other inputs")
            };
            assert!(go_to(&shared, &mut tab, &title, "PARTS"), "{title}");
            press(&shared, &mut tab, "enter.view.flow");
            assert_eq!(tab.level(), &Level::Others);
            let plain = check(&shared, &mut tab, name, cols, rows);
            let text = text_of(&plain);
            assert!(text.contains("OTHER INPUTS"), "{text}");
            assert!(text.contains("Power Button"), "{text}");
            if count > 20 {
                assert!(
                    text.contains("more"),
                    "the list does not say what it leaves out:\n{text}"
                );
            }
        }
    }
}

#[test]
fn without_color_each_state_of_a_box_keeps_its_word_or_its_glyph() {
    let mut shared = examples::roaming();
    let mut tab = tab_at(&shared, 100, 30);
    tab.set_color(false);
    // the page as it opens: the active terminal's dotted border, and NEW
    // PATH's empty places drawn as templates
    let plain = screen(&shared, &tab, 100, 30, false);
    let text = text_of(&plain);
    assert!(
        text.contains('┆'),
        "no dotted border:
{text}"
    );
    assert!(
        text.contains("╔") && text.contains("Choose… [Enter]"),
        "{text}"
    );
    // a connected keyboard in no path says so, and once picked its title
    // gains ✓
    assert!(go_to(&shared, &mut tab, "ThinkPad Extra Buttons", "PARTS"));
    let said = press(&shared, &mut tab, "space.view.flow");
    with_message(&mut shared, &said);
    let plain = screen(&shared, &tab, 100, 30, false);
    check_no_color(&plain);
    let text = text_of(&plain);
    assert!(text.contains("✓ ThinkPad Extra Buttons"), "{text}");
    assert!(text.contains("in no path"), "{text}");
    // a part a path holds on every VT is refused for NEW PATH, in words
    assert!(go_to(&shared, &mut tab, "eDP-1", "PARTS"));
    let said = press(&shared, &mut tab, "space.view.flow");
    with_message(&mut shared, &said);
    let plain = screen(&shared, &tab, 100, 30, false);
    assert!(text_of(&plain).contains("already in the terminal console on every VT"));
}

#[test]
fn a_display_a_path_matches_by_a_glob_is_in_that_path() {
    // the rule names eDP-1 on any card, as a person writes it by hand
    let text = "version = 1\n[head console]\n[rules]\ndisplay connector:*/eDP-1 -> console\ninput * -> console\n";
    let shared = machine("padnoir", text, examples::roaming().present.clone());
    let mut tab = tab_at(&shared, 100, 30);
    assert!(go_to(&shared, &mut tab, "eDP-1", "PARTS"));
    let text = text_of(&screen(&shared, &tab, 100, 30, true));
    assert!(text.contains("in the terminal console"), "{text}");
    assert!(!text.contains("display is in no path"), "{text}");
}
