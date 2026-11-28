//! The search: its pure matcher over a table of patterns, the component's
//! keys and kept matches, and the search row at 80 and 100 columns, as a
//! screen at 100×30 and 80×25 with the screen map's fonts and with ten
//! thousand fonts.
//!
//! References:
//! - ADR-1206 D3, "One search component": one component owns the row,
//!   the entry, the help and the keys, and one pure function turns the
//!   entry into a matcher.
//! - ADR-1403 D11, "Search, on ?" and "Find, on Ctrl+F": the patterns,
//!   the row's place and keys, and Find's prompt.

mod screens;

use std::borrow::Cow;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_widgets::keys::{Code, Key};
use vtium_config_widgets::search::{self, Outcome, Prompt, Search, matcher};
use vtium_config_widgets::tab::{FindItem, Searchable};

use screens::{Fonts, rows_of};

fn keys(search: &mut Search, typed: &str) {
    for c in typed.chars() {
        assert_eq!(search.key(&Key::char(c)), Outcome::Typed, "{c}");
    }
}

fn names(fonts: &Fonts, matches: &[usize]) -> Vec<&'static str> {
    matches.iter().map(|&i| fonts.items[i].1).collect()
}

// The matcher.

#[test]
fn the_matcher_reads_each_word_s_mark_and_ignores_case() {
    let fonts = Fonts::screen_map();
    let hay = fonts.haystack();
    for (entry, want) in [
        ("^mono", &["Monoid-Regular.ttf", "mononoki-Regular.ttf"][..]),
        ("^MONO", &["Monoid-Regular.ttf", "mononoki-Regular.ttf"]),
        ("$vga", &[]),
        ("$OTF", &["cullet-mono.otf"]),
        ("*nerd", &[""]),
        ("nerd", &[""]),
        ("NeRd", &[""]),
        (
            "*mono",
            &[
                "DejaVuSansMono.ttf",
                "JetBrainsMono-Regular.ttf",
                "Monoid-Regular.ttf",
                "mononoki-Regular.ttf",
                "cullet-mono.otf",
                "MyMono-Regular.ttf",
                "",
            ],
        ),
        ("^dejavu mono", &["DejaVuSansMono.ttf"]),
        (
            "mono $ttf",
            &[
                "DejaVuSansMono.ttf",
                "JetBrainsMono-Regular.ttf",
                "Monoid-Regular.ttf",
                "mononoki-Regular.ttf",
                "MyMono-Regular.ttf",
            ],
        ),
        ("^ter $otb", &["ter-u16n.otb"]),
    ] {
        let p = matcher::pattern(entry);
        let files: Vec<&str> = p.filter(&hay).iter().map(|&i| fonts.items[i].1).collect();
        assert_eq!(files, want, "{entry}");
    }
}

#[test]
fn an_empty_entry_or_a_bare_mark_matches_every_item() {
    let fonts = Fonts::screen_map();
    let hay = fonts.haystack();
    for entry in ["", "  ", "^", "$", "*", "^ *"] {
        let p = matcher::pattern(entry);
        assert!(p.is_empty(), "{entry:?}");
        assert_eq!(p.filter(&hay).len(), fonts.items.len(), "{entry:?}");
    }
    assert!(!matcher::pattern("^m").is_empty());
}

#[test]
fn one_field_matches_every_word_of_an_item() {
    // "^terminus" matches Terminus's family and "$otb" its file, but no
    // one field matches both, so the item does not match
    let fonts = Fonts::screen_map();
    let hay = fonts.haystack();
    assert_eq!(
        matcher::pattern("^terminus $otb").filter(&hay),
        Vec::<usize>::new()
    );
    assert_eq!(
        names(&fonts, &matcher::pattern("^terminus").filter(&hay)),
        ["ter-u16n.otb"]
    );
    assert_eq!(
        names(&fonts, &matcher::pattern("^dejavu $mono").filter(&hay)),
        ["DejaVuSansMono.ttf"]
    );
}

#[test]
fn a_longer_pattern_narrows_the_matches_unless_an_end_grows() {
    let narrows =
        |later: &str, earlier: &str| matcher::pattern(later).narrows(&matcher::pattern(earlier));
    assert!(narrows("^mon", "^mo"));
    assert!(narrows("*mono", "*mon"));
    assert!(narrows("mono x", "mono"));
    assert!(narrows("^m", ""));
    // a longer end does not narrow: "xvga" ends with "vga", not "vg"
    assert!(!narrows("$vga", "$vg"));
    assert!(narrows("$xvga", "$vga"));
    assert!(!narrows("^mo", "^mon"));
    assert!(!narrows("^mo", "*mo"));
    // refining the earlier matches gives what a full pass gives
    let fonts = Fonts::large(10_000);
    let hay = fonts.haystack();
    let mut earlier = matcher::pattern("").filter(&hay);
    let mut entry = String::new();
    for c in "*mono 1".chars() {
        entry.push(c);
        let p = matcher::pattern(&entry);
        let refined = p.refine(&hay, &earlier);
        assert_eq!(refined, p.filter(&hay), "{entry}");
        earlier = refined;
    }
}

// The component.

#[test]
fn typing_filters_the_list_and_keeps_the_matching_indexes() {
    let fonts = Fonts::screen_map();
    let mut s = Search::open(&fonts, Some(0));
    assert!(s.typing());
    assert_eq!(s.prompt(), Prompt::Search);
    assert_eq!(s.matches().len(), fonts.items.len());
    keys(&mut s, "^mono");
    assert_eq!(s.entry(), "^mono");
    assert_eq!(
        names(&fonts, s.matches()),
        ["Monoid-Regular.ttf", "mononoki-Regular.ttf"]
    );
    // DejaVu was highlighted and no longer matches: the first match is
    assert_eq!(s.highlighted(), Some(2));
    assert!(s.is_match(3) && !s.is_match(0));
    // Down and Up move the highlight through the matches, and stop at
    // the ends
    assert_eq!(s.key(&Key::plain(Code::Down)), Outcome::Moved);
    assert_eq!(s.highlighted(), Some(3));
    assert_eq!(s.key(&Key::plain(Code::Down)), Outcome::Moved);
    assert_eq!(s.highlighted(), Some(3));
    assert_eq!(s.key(&Key::plain(Code::Up)), Outcome::Moved);
    assert_eq!(s.highlighted(), Some(2));
    // Backspace widens the filter again
    assert_eq!(s.key(&Key::plain(Code::Backspace)), Outcome::Typed);
    assert_eq!(s.entry(), "^mon");
    assert_eq!(s.highlighted(), Some(2));
    // Enter keeps the filter, and the row still shows the pattern
    assert_eq!(s.key(&Key::plain(Code::Enter)), Outcome::Kept);
    assert!(!s.typing());
    assert_eq!(s.entry(), "^mon");
    assert_eq!(s.matches().len(), 2);
    // a kept filter passes on the list's keys, and `?` goes back to the
    // entry
    assert_eq!(s.key(&Key::char('j')), Outcome::Ignored);
    assert_eq!(s.key(&Key::char('?')), Outcome::Resumed);
    assert!(s.typing());
    // `?` and `q` are typed into the entry
    keys(&mut s, "?q");
    assert_eq!(s.entry(), "^mon?q");
    assert_eq!(s.matches(), &[] as &[usize]);
    assert_eq!(s.highlighted(), None);
}

#[test]
fn esc_or_backspace_on_an_empty_entry_closes_the_row() {
    let fonts = Fonts::screen_map();
    let mut s = Search::open(&fonts, None);
    keys(&mut s, "ab");
    assert_eq!(s.key(&Key::ESCAPE), Outcome::Closed);
    let mut s = Search::open(&fonts, None);
    keys(&mut s, "a");
    assert_eq!(s.key(&Key::plain(Code::Backspace)), Outcome::Typed);
    assert_eq!(s.key(&Key::plain(Code::Backspace)), Outcome::Closed);
    // with the filter kept, Esc clears it and closes the row
    let mut s = Search::open(&fonts, None);
    keys(&mut s, "mono");
    assert_eq!(s.key(&Key::plain(Code::Enter)), Outcome::Kept);
    assert_eq!(s.key(&Key::ESCAPE), Outcome::Closed);
    // a chord is not typed
    let mut s = Search::open(&fonts, None);
    assert_eq!(s.key(&Key::ctrl('s')), Outcome::Ignored);
    assert_eq!(s.key(&Key::alt('o')), Outcome::Ignored);
    assert_eq!(s.entry(), "");
}

#[test]
fn find_searches_names_and_descriptions_and_pages_through_its_matches() {
    let items: Vec<FindItem> = (0..30)
        .map(|i| FindItem {
            name: format!("font-{i:02}"),
            description: (i % 2 == 0).then(|| "Monospaced".to_owned()),
            place: "Fonts".to_owned(),
            fact: String::new(),
            index: i,
        })
        .collect();
    let mut s = Search::find(&items);
    assert_eq!(s.prompt(), Prompt::Find);
    assert_eq!(s.help(), "");
    keys(&mut s, "*mono");
    assert_eq!(s.matches().len(), 15);
    assert_eq!(s.highlighted(), Some(0));
    assert_eq!(s.key(&Key::plain(Code::PageDown)), Outcome::Moved);
    assert_eq!(s.highlighted(), Some(16));
    assert_eq!(s.key(&Key::plain(Code::PageUp)), Outcome::Moved);
    assert_eq!(s.highlighted(), Some(0));
    keys(&mut s, "x");
    // with nothing found, Enter does nothing
    assert_eq!(s.key(&Key::plain(Code::Enter)), Outcome::Ignored);
}

// The row.

fn row_text(s: &Search, width: u16) -> String {
    let area = Rect::new(0, 0, width, 1);
    let mut buf = Buffer::empty(area);
    search::row::draw(s, &mut buf, area, true);
    rows_of(&buf).remove(0)
}

#[test]
fn the_row_names_the_fields_where_there_is_room() {
    let fonts = Fonts::screen_map();
    let mut s = Search::open(&fonts, None);
    keys(&mut s, "*mono");
    for width in [100, 80] {
        let row = row_text(&s, width);
        assert_eq!(row.chars().count(), usize::from(width), "{row}");
        assert!(row.starts_with(" ? *mono_ "), "{row}");
        assert!(
            row.ends_with("^start  $end  *anywhere · case ignored · family, file "),
            "{row}"
        );
    }
    // a long entry leaves room only for the syntax, then for nothing
    let mut s = Search::open(&fonts, None);
    keys(&mut s, "*mono ^dejavu $ttf *sans *regular x");
    let row = row_text(&s, 80);
    assert!(
        row.ends_with(" ^start  $end  *anywhere · case ignored "),
        "{row}"
    );
    assert!(!row.contains("family"), "{row}");
    keys(&mut s, " *a-pattern-longer-than-the-help-leaves-room-for");
    let row = row_text(&s, 80);
    assert_eq!(row.chars().count(), 80);
    assert!(!row.contains("case ignored"), "{row}");
    // the entry scrolls to show its end and the cursor
    assert!(row.trim_end().ends_with("room-for_"), "{row}");
    assert!(row.starts_with(" ? …"), "{row}");
}

#[test]
fn a_kept_filter_shows_its_pattern_without_the_cursor_and_find_its_prompt() {
    let fonts = Fonts::screen_map();
    let mut s = Search::open(&fonts, None);
    keys(&mut s, "^mono");
    s.key(&Key::plain(Code::Enter));
    assert!(row_text(&s, 100).starts_with(" ? ^mono "));
    let items = vec![FindItem {
        name: "Cullet Mono".into(),
        description: None,
        place: "Fonts".into(),
        fact: "cullet-mono.otf".into(),
        index: 0,
    }];
    let mut s = Search::find(&items);
    keys(&mut s, "*mono");
    for width in [100, 80] {
        let row = row_text(&s, width);
        assert!(row.starts_with(" Find *mono_ "), "{row}");
        assert!(
            row.ends_with(" ^start  $end  *anywhere · case ignored "),
            "{row}"
        );
    }
}

#[test]
fn the_row_draws_its_help_through_the_search_help_role_and_its_cursor_where_the_entry_ends() {
    let fonts = Fonts::screen_map();
    let mut s = Search::open(&fonts, None);
    keys(&mut s, "ab");
    let area = Rect::new(0, 5, 80, 1);
    let mut buf = Buffer::empty(area);
    let cursor = search::row::draw(&s, &mut buf, area, true);
    assert_eq!((cursor.x, cursor.y), (5, 5));
    assert_eq!(buf[(5u16, 5u16)].symbol(), "_");
    let help_x = 80 - 2;
    let grey = vtium_config_widgets::roles::Role::SearchHelp
        .style(true)
        .cell();
    assert_eq!(buf[(help_x, 5u16)].style().fg, grey.fg);
    // without color, the help is the terminal's own color
    search::row::draw(&s, &mut buf, area, false);
    assert_eq!(
        buf[(help_x, 5u16)].style().fg,
        Some(ratatui_core::style::Color::Reset)
    );
}

// Screens: the search row under a list, at both sizes, with both fixtures.

#[test]
fn the_search_screens_match_their_snapshots() {
    let small = Fonts::screen_map();
    let large = Fonts::large(10_000);
    for (name, fonts, typed) in [
        ("search-fonts-mono", &small, "^mono"),
        ("search-fonts-none", &small, "$vga"),
        ("search-large-mono", &large, "*mono 1"),
    ] {
        for (cols, rows) in [(100, 30), (80, 25)] {
            let mut s = Search::open(fonts, Some(0));
            keys(&mut s, typed);
            let screen = screens::search_screen(fonts, &s, cols, rows, true);
            screens::check_rows(&screen, cols, rows);
            // without color the highlight is still reverse video, and the
            // text is the same
            let mono = screens::search_screen(fonts, &s, cols, rows, false);
            assert_eq!(rows_of(&mono), rows_of(&screen));
            if s.highlighted().is_some() {
                screens::check_highlight_without_color(&mono);
            }
            screens::snapshot(&format!("{name}-{cols}x{rows}"), &screen);
        }
    }
}

// The bound of decision 144: ten thousand items filter per key with no
// rescan. Each key's filter is a pass over the haystack built once, so no
// key reads the list again.
#[test]
fn ten_thousand_fonts_filter_as_each_key_is_typed() {
    let fonts = Fonts::large(10_000);
    let mut s = Search::open(&fonts, None);
    let reads = fonts.reads.get();
    assert_eq!(reads, 20_000);
    for c in "*mono 12".chars() {
        s.key(&Key::char(c));
    }
    for _ in 0..3 {
        s.key(&Key::plain(Code::Backspace));
    }
    assert_eq!(fonts.reads.get(), reads, "a key read the list again");
    assert_eq!(s.entry(), "*mono");
    assert_eq!(s.matches().len(), 5_000);
}

/// A list whose `text` the search must never call after it opens.
#[test]
fn the_haystack_is_built_once_when_the_row_opens() {
    struct Counted(std::cell::Cell<usize>);
    impl Searchable for Counted {
        fn search_node(&self) -> &'static str {
            "help"
        }
        fn count(&self) -> usize {
            3
        }
        fn text(&self, item: usize, _field: usize) -> Option<Cow<'_, str>> {
            self.0.set(self.0.get() + 1);
            Some(Cow::Owned(format!("Topic {item}")))
        }
    }
    let list = Counted(std::cell::Cell::new(0));
    let mut s = Search::open(&list, None);
    assert_eq!(list.0.get(), 3);
    keys(&mut s, "topic 2");
    assert_eq!(list.0.get(), 3);
    assert_eq!(s.matches(), &[2]);
    assert_eq!(s.help(), "topic");
}
