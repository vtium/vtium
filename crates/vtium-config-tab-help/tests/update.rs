//! The Help tab driven through its `Tab` methods, on the screen map's
//! machine and on a large one: every row of the Help node, the clicks on
//! the places the tab draws, the search row's keys, and what the tab
//! answers Find, F1 and the search.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Help bullet, and "The keys and
//!   nodes these add": Up and Down move between the topics, PgUp and PgDn
//!   scroll a topic, Left and Right switch to About and back.
//! - ADR-1206 D2: a tab's update returns commands, and changes the
//!   document only through them.

mod common;

use common::{after, fixtures, rows_of, screen, tab, viewer};
use ratatui_core::layout::Rect;
use vtium_config_core::ui::{Action, Shared};
use vtium_config_tab_help::HelpTab;
use vtium_config_tab_help::page::Layout;
use vtium_config_tab_help::topics::{ABOUT, TOPICS};
use vtium_config_widgets::contract;
use vtium_config_widgets::input::{Button, Key, Mouse};
use vtium_config_widgets::tab::Tab;

/// A press of the left button at `x`, `y`, on a row of a list.
fn click(x: u16, y: u16) -> Action {
    Action::key(
        "click-row.view",
        Key::Mouse(Mouse {
            button: Button::Press(0),
            col: usize::from(x),
            row: usize::from(y),
        }),
    )
}

/// The screen's row that heads the right pane.
fn head(tab: &HelpTab, shared: &Shared, cols: u16, rows: u16) -> String {
    rows_of(&screen(tab, shared, cols, rows, false))[2].clone()
}

#[test]
fn up_and_down_move_between_the_topics_and_stop_at_the_ends() {
    for (_, shared) in fixtures() {
        let mut t = tab(true);
        after(&mut t, &shared, &["up.view.help"]);
        assert_eq!(t.topic(), 0);
        for want in 1..TOPICS.len() {
            after(&mut t, &shared, &["down.view.help"]);
            assert_eq!(t.topic(), want);
        }
        after(&mut t, &shared, &["down.view.help"]);
        assert_eq!(t.topic(), TOPICS.len() - 1);
        after(&mut t, &shared, &["up.view.help"]);
        assert_eq!(t.topic(), TOPICS.len() - 2);
        after(&mut t, &shared, &["home.view.help"]);
        assert_eq!(t.topic(), 0);
        after(&mut t, &shared, &["end.view.help"]);
        assert_eq!(t.topic(), ABOUT);
    }
}

#[test]
fn pgup_and_pgdn_scroll_a_long_topic_at_80x25_and_a_new_topic_starts_at_its_top() {
    for (fixture, shared) in fixtures() {
        let mut t = tab(true);
        t.update(&shared, Action::item("6.view", 8));
        // the draw at 80×25 gives update the window it scrolls by
        assert!(
            head(&t, &shared, 80, 25).contains("lines 1 to 19 of"),
            "{fixture}"
        );
        let window = Layout::of(viewer(80, 25)).window();
        assert_eq!(window, 19);
        after(&mut t, &shared, &["pgdn.view.help"]);
        let first = t.top();
        assert!(first > 0 && first <= window, "{fixture}: {first}");
        assert!(
            head(&t, &shared, 80, 25).contains(&format!("lines {} to", first + 1)),
            "{fixture}"
        );
        // PgDn stops where the last line is on the window's last row
        let mut last = first;
        for _ in 0..50 {
            after(&mut t, &shared, &["pgdn.view.help"]);
            last = t.top();
        }
        let h = head(&t, &shared, 80, 25);
        let total: usize = h
            .split(" of ")
            .nth(1)
            .and_then(|r| r.split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .expect("the count of lines");
        assert_eq!(last, total - window, "{fixture}: {h}");
        assert!(
            h.contains(&format!("to {total} of {total}")),
            "{fixture}: {h}"
        );
        for _ in 0..50 {
            after(&mut t, &shared, &["pgup.view.help"]);
        }
        assert_eq!(t.top(), 0, "{fixture}");
        after(&mut t, &shared, &["pgdn.view.help", "up.view.help"]);
        assert_eq!((t.topic(), t.top()), (7, 0), "{fixture}");
    }
}

#[test]
fn a_topic_that_fits_does_not_scroll() {
    for (_, shared) in fixtures() {
        let mut t = tab(true);
        t.update(&shared, Action::item("6.view", 5));
        screen(&t, &shared, 100, 30, true);
        after(&mut t, &shared, &["pgdn.view.help"]);
        assert_eq!(t.top(), 0);
        assert!(!head(&t, &shared, 100, 30).contains("lines"));
    }
}

#[test]
fn left_and_right_switch_to_about_and_back_to_the_topic_before() {
    for (_, shared) in fixtures() {
        let mut t = tab(true);
        t.update(&shared, Action::item("6.view", 2));
        after(&mut t, &shared, &["right.view.help"]);
        assert_eq!(t.topic(), ABOUT);
        after(&mut t, &shared, &["left.view.help"]);
        assert_eq!(t.topic(), 2);
        after(&mut t, &shared, &["left.view.help", "right.view.help"]);
        assert_eq!(t.topic(), 2);
    }
}

#[test]
fn a_click_on_a_topic_highlights_it_where_the_view_drew_it() {
    for (fixture, shared) in fixtures() {
        for (cols, rows) in [(100, 30), (80, 25)] {
            let mut t = tab(true);
            let drawn = rows_of(&screen(&t, &shared, cols, rows, false));
            let layout = Layout::of(viewer(cols, rows));
            for (n, topic) in TOPICS.iter().enumerate() {
                let place = layout.topic(n).expect("room for every topic");
                let row = &drawn[usize::from(place.y)];
                let at: String = row
                    .chars()
                    .skip(usize::from(place.x))
                    .take(usize::from(place.width))
                    .collect();
                assert!(at.contains(topic.name), "{fixture} {cols}x{rows}: {at:?}");
                // the right end of the place, too
                let commands = t.update(&shared, click(place.right() - 1, place.y));
                assert!(commands.is_empty());
                assert_eq!(t.topic(), n, "{fixture} {cols}x{rows}: {}", topic.name);
            }
            // a click on the rule, the page or below the last topic moves nothing
            let before = t.topic();
            let below = layout.topic(TOPICS.len()).expect("a row below");
            for (x, y) in [
                (layout.rule.x, 5),
                (layout.page.x + 3, 6),
                (below.x + 2, below.y),
            ] {
                t.update(&shared, click(x, y));
                assert_eq!(
                    t.topic(),
                    before,
                    "{fixture} {cols}x{rows}: a click at {x},{y}"
                );
            }
        }
    }
}

#[test]
fn find_s_match_and_the_tab_the_person_came_from_carry_the_topic() {
    for (_, shared) in fixtures() {
        let mut t = tab(true);
        let items = t.find_items(&shared);
        assert_eq!(items.len(), TOPICS.len());
        for (i, item) in items.iter().enumerate() {
            assert_eq!(item.index, i);
            assert_eq!(item.name, TOPICS[i].name);
            assert_eq!(item.place, "Help");
            assert!(!item.fact.is_empty());
        }
        assert_eq!(items[0].fact, "32 keys".to_owned());
        assert_eq!(items[8].fact, "29 commands".to_owned());
        assert!(
            items[0]
                .description
                .as_deref()
                .is_some_and(|d| d.starts_with("The Fonts tab"))
        );
        t.update(&shared, Action::item("enter.typing.find", 7));
        assert_eq!(t.topic(), 7);
        // Alt+H from Looks, the tab numbered 2
        t.update(&shared, Action::item("alt-tab.view", 2));
        assert_eq!(t.topic(), 2);
        // an index past the topics moves nothing
        t.update(&shared, Action::item("enter.typing.find", 99));
        assert_eq!(t.topic(), 2);
    }
}

#[test]
fn every_row_of_the_help_node_returns_no_command() {
    let node = contract::node("help").expect("the Help node");
    let rows: Vec<&'static str> = node
        .rows()
        .chain(contract::node("any.search").expect("the search").rows())
        .chain(contract::node("any.filter").expect("the filter").rows())
        .map(|r| r.id)
        .collect();
    assert!(rows.len() > 40);
    for (_, shared) in fixtures() {
        let mut t = tab(true);
        screen(&t, &shared, 80, 25, true);
        for row in &rows {
            for action in [
                Action::of(row),
                Action::key(row, Key::Char('x')),
                click(3, 5),
            ] {
                let commands = t.update(&shared, action);
                assert!(commands.is_empty(), "{row}: {commands:?}");
            }
        }
    }
}

#[test]
fn the_search_filters_the_topics_lines_and_esc_or_backspace_closes_it() {
    for (fixture, shared) in fixtures() {
        let mut t = tab(true);
        t.update(&shared, Action::item("6.view", 5));
        after(&mut t, &shared, &["question.view"]);
        assert!(t.search().is_some_and(|s| s.typing()));
        for c in "*activ".chars() {
            t.update(&shared, Action::key("char.typing.search", Key::Char(c)));
        }
        // Help has no line with "activ", so the highlight goes to Fonts
        assert_eq!(t.topic(), 0, "{fixture}");
        let s = rows_of(&screen(&t, &shared, 100, 30, false));
        assert!(s[2].contains("Fonts  5 lines match"), "{fixture}: {}", s[2]);
        let listed: Vec<&str> = s[4..]
            .iter()
            .map(|r| r.split('│').next().unwrap_or("").trim())
            .take_while(|l| !l.is_empty())
            .collect();
        assert_eq!(
            listed,
            [
                "Fonts            5",
                "Effects          2",
                "Looks            1",
                "Commands         4"
            ]
        );
        // Down moves among the topics that match while the cursor stays
        t.update(&shared, Action::key("updown.typing.search", Key::Down));
        assert_eq!(t.topic(), 1);
        after(&mut t, &shared, &["enter.typing.search"]);
        assert!(t.search().is_some_and(|s| !s.typing()));
        after(&mut t, &shared, &["down.view.help", "down.view.help"]);
        assert_eq!(t.topic(), 8, "{fixture}: Down moves among what matches");
        after(&mut t, &shared, &["question.layer.filter"]);
        assert!(t.search().is_some_and(|s| s.typing()));
        after(&mut t, &shared, &["esc.typing.search"]);
        assert!(t.search().is_none());
        assert_eq!(t.topic(), 8);
        // Backspace on an empty entry closes the row
        after(&mut t, &shared, &["question.view"]);
        t.update(&shared, Action::key("char.typing.search", Key::Char('q')));
        after(&mut t, &shared, &["backspace.typing.search"]);
        assert!(t.search().is_some());
        after(&mut t, &shared, &["backspace.typing.search"]);
        assert!(t.search().is_none());
        // a kept filter clears on Esc
        after(&mut t, &shared, &["question.view"]);
        t.update(&shared, Action::key("char.typing.search", Key::Char('z')));
        after(
            &mut t,
            &shared,
            &["enter.typing.search", "esc.layer.filter"],
        );
        assert!(t.search().is_none());
    }
}

#[test]
fn a_search_that_matches_nothing_says_so() {
    for (_, shared) in fixtures() {
        let mut t = tab(true);
        after(&mut t, &shared, &["question.view"]);
        for c in "$qqqq".chars() {
            t.update(&shared, Action::key("char.typing.search", Key::Char(c)));
        }
        let s = rows_of(&screen(&t, &shared, 80, 25, false));
        assert!(s[2].contains("No topic matches"), "{}", s[2]);
    }
}

#[test]
fn the_tab_answers_the_shell_s_queries() {
    for (_, shared) in fixtures() {
        let t = tab(true);
        assert_eq!(t.name(), "Help");
        assert_eq!(t.letter(), 'h');
        assert_eq!(t.node(), "help");
        assert_eq!(t.full_text(&shared), None);
        assert!(t.menu_items(&shared).is_empty());
        let ids = |rows: Vec<&contract::Row>| rows.iter().map(|r| r.id).collect::<Vec<_>>();
        assert_eq!(ids(t.key_line()), ids(contract::labelled("help")));
        assert_eq!(ids(t.bar()), ids(contract::buttons("help")));
        let s = t.searchable().expect("Help offers the search");
        assert_eq!(s.fields(), ["topic"]);
        assert_eq!(s.help(), "topic");
        assert_eq!(s.haystack().items.len(), s.count());
        assert!(s.text(0, 0).is_some_and(|f| f.starts_with("Fonts ")));
        assert!(s.text(0, 1).is_none());
        assert!(s.text(s.count(), 0).is_none());
    }
}

#[test]
fn the_tab_draws_only_into_the_viewer_it_is_given() {
    for (_, shared) in fixtures() {
        let t = tab(true);
        let area = Rect::new(0, 0, 100, 30);
        let inner = Rect::new(3, 4, 70, 18);
        let mut terminal = ratatui_core::terminal::Terminal::new(
            ratatui_core::backend::TestBackend::new(area.width, area.height),
        )
        .expect("a terminal");
        terminal
            .draw(|f| {
                for y in 0..30 {
                    for x in 0..100 {
                        f.buffer_mut()[(x, y)].set_symbol("x");
                    }
                }
                t.draw(&shared, f, inner);
            })
            .expect("a frame");
        let buf = terminal.backend().buffer();
        for y in 0..30 {
            for x in 0..100 {
                if !inner.contains((x, y).into()) {
                    assert_eq!(buf[(x, y)].symbol(), "x", "{x},{y}");
                }
            }
        }
    }
}
