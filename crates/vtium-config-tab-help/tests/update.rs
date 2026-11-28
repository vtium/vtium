//! The Help tab driven through its `Tab` methods, on the screen map's
//! machine and on a large one: every row of the Help node, the clicks on
//! the places the tab draws, the shell's `?` search handed to the tab,
//! and what the tab answers Find, F1 and the search.
//!
//! References:
//! - ADR-1403 D11, "The tabs' layouts", the Help bullet, and "The keys and
//!   nodes these add": Up and Down move between the topics, PgUp and PgDn
//!   scroll a topic, Left and Right switch to About and back.
//! - ADR-1206 D2: a tab's update returns commands, and changes the
//!   document only through them.
//! - ADR-1206 D3: the navigation state holds the search, and each tab
//!   keeps the indexes that match.
//! - ADR-1403 D11, "F1, help for what is highlighted": F1's list of every
//!   key of a screen is the Help tab's topic for that tab.

mod common;

use common::{after, fixtures, open_search, rows_of, screen, search_keys, tab, typed, viewer};
use ratatui_core::layout::Rect;
use vtium_config_core::ui::{Action, Shared};
use vtium_config_tab_help::HelpTab;
use vtium_config_tab_help::page::{self, Layout, Look};
use vtium_config_tab_help::topics::{ABOUT, EVERYWHERE, TOPICS};
use vtium_config_widgets::canvas;
use vtium_config_widgets::contract;
use vtium_config_widgets::input::{Button, Key, Mouse};
use vtium_config_widgets::keys::{Code, Key as SearchKey};
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::tab::{Searchable, Tab};

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

/// A key of the search row that types nothing.
fn plain(code: Code) -> SearchKey {
    SearchKey::plain(code)
}

/// The topics the column of `t` lists at 100×30, each with its count.
fn listed(t: &HelpTab, shared: &Shared) -> Vec<String> {
    rows_of(&screen(t, shared, 100, 30, false))[4..]
        .iter()
        .map(|r| r.split('│').next().unwrap_or("").trim().to_owned())
        .take_while(|l| !l.is_empty())
        .collect()
}

#[test]
fn the_shell_s_search_filters_the_topics_lines_and_esc_or_backspace_closes_it() {
    for (fixture, shared) in fixtures() {
        let mut t = tab(true);
        t.update(&shared, Action::item("6.view", 5));
        let mut s = open_search(&t);
        search_keys(&mut t, &shared, &mut s, &typed("*activ"));
        assert!(t.search().is_some_and(|s| s.typing()));
        // Help has no line with "activ", so the highlight goes to Fonts
        assert_eq!(t.topic(), 0, "{fixture}");
        let rows = rows_of(&screen(&t, &shared, 100, 30, false));
        assert!(
            rows[2].contains("Fonts  5 lines match"),
            "{fixture}: {}",
            rows[2]
        );
        assert_eq!(
            listed(&t, &shared),
            [
                "Fonts            5",
                "Effects          2",
                "Looks            1",
                "Commands         4"
            ]
        );
        // Down moves the search's highlight among the lines that match
        // while the cursor stays, and the highlighted line's topic shows
        search_keys(&mut t, &shared, &mut s, &[plain(Code::Down); 4]);
        assert_eq!(t.topic(), 0, "{fixture}: Fonts has five lines");
        search_keys(&mut t, &shared, &mut s, &[plain(Code::Down)]);
        assert_eq!(t.topic(), 1, "{fixture}");
        search_keys(&mut t, &shared, &mut s, &[plain(Code::Up)]);
        assert_eq!(t.topic(), 0, "{fixture}");
        search_keys(&mut t, &shared, &mut s, &[plain(Code::Down)]);
        assert_eq!(t.topic(), 1, "{fixture}");
        search_keys(&mut t, &shared, &mut s, &[plain(Code::Enter)]);
        assert!(t.search().is_some_and(|s| !s.typing()));
        after(&mut t, &shared, &["down.view.help", "down.view.help"]);
        assert_eq!(t.topic(), 8, "{fixture}: Down moves among what matches");
        // `?` resumes with the search's highlight on the topic's line
        search_keys(&mut t, &shared, &mut s, &typed("?"));
        assert!(t.search().is_some_and(|s| s.typing()));
        let at = s.as_ref().and_then(|s| s.highlighted());
        assert_eq!(at, t.highlighted(), "{fixture}");
        assert!(at.is_some(), "{fixture}");
        search_keys(&mut t, &shared, &mut s, &[plain(Code::Escape)]);
        assert!(t.search().is_none() && s.is_none());
        assert_eq!(t.topic(), 8);
        assert_eq!(listed(&t, &shared).len(), TOPICS.len(), "{fixture}");
        // Backspace on an empty entry closes the row
        let mut s = open_search(&t);
        search_keys(&mut t, &shared, &mut s, &typed("q"));
        search_keys(&mut t, &shared, &mut s, &[plain(Code::Backspace)]);
        assert!(t.search().is_some());
        search_keys(&mut t, &shared, &mut s, &[plain(Code::Backspace)]);
        assert!(t.search().is_none());
        // a kept filter clears on Esc
        let mut s = open_search(&t);
        search_keys(&mut t, &shared, &mut s, &typed("z"));
        search_keys(
            &mut t,
            &shared,
            &mut s,
            &[plain(Code::Enter), plain(Code::Escape)],
        );
        assert!(t.search().is_none());
    }
}

#[test]
fn a_typed_entry_keeps_a_topic_that_still_has_a_line_that_matches() {
    for (fixture, shared) in fixtures() {
        let mut t = tab(true);
        // Commands, whose first line does not hold "activ", so the search
        // highlights a line of Fonts
        t.update(&shared, Action::item("6.view", 8));
        after(&mut t, &shared, &["pgdn.view.help"]);
        let mut s = open_search(&t);
        search_keys(&mut t, &shared, &mut s, &typed("*activ"));
        assert_ne!(s.as_ref().and_then(|s| s.highlighted()), t.highlighted());
        assert_eq!(t.topic(), 8, "{fixture}");
        assert_eq!(t.top(), 0, "{fixture}");
    }
}

#[test]
fn the_search_opens_on_the_highlighted_topic_s_line() {
    let [(_, shared), _] = fixtures();
    assert_eq!(tab(true).highlighted(), Some(0));
    for (n, topic) in TOPICS.iter().enumerate() {
        let mut t = tab(true);
        t.update(&shared, Action::item("6.view", n));
        let line = t.highlighted().expect("a line of every topic");
        assert!(
            t.text(line, 0).is_some_and(|l| l.starts_with(topic.name)),
            "{}",
            topic.name
        );
    }
}

#[test]
fn the_shell_s_empty_hand_lists_every_topic_again() {
    for (_, shared) in fixtures() {
        let mut t = tab(true);
        let mut s = open_search(&t);
        search_keys(&mut t, &shared, &mut s, &typed("*activ"));
        assert_eq!(listed(&t, &shared).len(), 4);
        t.filtered(&shared, None);
        assert!(t.search().is_none());
        assert_eq!(listed(&t, &shared).len(), TOPICS.len());
    }
}

#[test]
fn a_search_that_matches_nothing_says_so() {
    for (_, shared) in fixtures() {
        let mut t = tab(true);
        let mut s = open_search(&t);
        search_keys(&mut t, &shared, &mut s, &typed("$qqqq"));
        let rows = rows_of(&screen(&t, &shared, 80, 25, false));
        assert!(rows[2].contains("No topic matches"), "{}", rows[2]);
    }
}

/// The page of `topic` laid out `width` columns wide without color, as
/// each line's text.
fn page_text(t: &HelpTab, topic: usize, width: usize) -> Vec<String> {
    page::lay(&t.entries()[topic], None, width, false)
        .lines
        .iter()
        .map(|l| l.iter().map(|r| r.text.as_str()).collect())
        .collect()
}

#[test]
fn f1_lists_a_level_s_keys_from_its_tab_s_topic() {
    let t = tab(true);
    for width in [76, 56] {
        let lines = t.keys_help("looks.look", width).expect("Looks' keys");
        let text: Vec<String> = lines.iter().map(|(l, _)| l.clone()).collect();
        assert_eq!(text, page_text(&t, 2, usize::from(width)), "{width}");
        let what = contract::node("looks").expect("the Looks node").what;
        let first = what.split_whitespace().next().expect("a word");
        assert!(text[0].starts_with(first), "{width}: {}", text[0]);
        let heading = lines.iter().find(|(l, _)| l == EVERYWHERE);
        assert_eq!(heading.map(|(_, r)| *r), Some(Role::Heading), "{width}");
        // a key's line takes its first run's role
        let page = page::lay(&t.entries()[2], None, usize::from(width), false);
        for (line, (_, role)) in page.lines.iter().zip(&lines) {
            let want = match line.first().map(|r| r.look) {
                Some(Look::Role(r)) => r,
                _ => Role::Row,
            };
            assert_eq!(*role, want, "{width}");
        }
    }
    // the tab's own node, and a level over the resources' list
    let looks = t.keys_help("looks", 76);
    assert_eq!(t.keys_help("looks.palette", 76), looks);
    assert_eq!(t.keys_help("looks.look", 76), looks);
}

#[test]
fn f1_on_a_node_over_no_tab_lists_nothing() {
    let t = tab(true);
    for node in ["any.menu.file", "any.menu.actions", "any.search", "no.such"] {
        assert_eq!(t.keys_help(node, 76), None, "{node}");
    }
}

#[test]
fn f1_lists_each_tab_s_topic_and_every_line_fits_its_width() {
    let t = tab(true);
    let nodes = ["fonts", "effects", "looks", "flow", "settings", "help"];
    for (topic, node) in nodes.iter().enumerate() {
        for width in [76, 56] {
            let lines = t.keys_help(node, width).expect("a tab's keys");
            let text: Vec<String> = lines.iter().map(|(l, _)| l.clone()).collect();
            assert_eq!(text, page_text(&t, topic, usize::from(width)), "{node}");
            assert!(lines.len() > 5, "{node} {width}");
            for (line, _) in &lines {
                assert!(
                    canvas::cells(line) <= usize::from(width),
                    "{node} at {width}: {} cells: {line:?}",
                    canvas::cells(line)
                );
            }
        }
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
        assert_eq!(ids(t.bar(&shared)), ids(contract::buttons("help")));
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
