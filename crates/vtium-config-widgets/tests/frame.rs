//! The frame every screen shares: its regions, the top row as the screen
//! narrows, the action bar of every node of the interface tree, what a
//! click on each place hits, and the frame on each of the six tabs at
//! 100×30 and 80×25, with and without color.
//!
//! Each screen's text is kept under `tests/snapshots/`, with a grid of
//! the attributes that show where no color does. A test fails when a
//! drawn screen differs from its snapshot, and
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-widgets --test frame`
//! writes the snapshots again.
//!
//! References:
//! - ADR-1403 D11, "The frame", "Alt letters" and "The action bar".
//! - ADR-1206 D4, "One frame for every screen", and D10: 100×30, 80×25,
//!   `NO_COLOR` and a monochrome console, and a large machine's names and
//!   counts.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};
use vtium_config_widgets::contract;
use vtium_config_widgets::frame::{
    Bar, Button, Count, Facts, FrameLayout, Hit, MenuName, Message, Mode, Options, Side, TabName,
    TopRow,
};
use vtium_config_widgets::roles::Role;

const TABS: [TabName<'static>; 6] = [
    TabName {
        name: "Fonts",
        letter: 'n',
    },
    TabName {
        name: "Effects",
        letter: 'e',
    },
    TabName {
        name: "Looks",
        letter: 'l',
    },
    TabName {
        name: "Flow",
        letter: 'o',
    },
    TabName {
        name: "Settings",
        letter: 's',
    },
    TabName {
        name: "Help",
        letter: 'h',
    },
];

/// The tabs' nodes, in the top row's order.
const NODES: [&str; 6] = ["fonts", "effects", "looks", "flow", "settings", "help"];

/// The facts of the screen map's machine: padnoir, as the catalog draws it.
fn map_facts() -> Facts<'static> {
    Facts {
        name: "padnoir",
        mode: None,
        not_saved: Count::opening(35),
        problems: Count::opening(3),
    }
}

/// The facts of a large machine: a long name, counts in the thousands,
/// and a live macro run.
fn large_facts() -> Facts<'static> {
    Facts {
        name: "threadripper-ten-gpus-lab-bench-07",
        mode: Some(Mode::Live),
        not_saved: Count::opening(1284),
        problems: Count::opening(57),
    }
}

fn top(facts: Facts<'static>, open_tab: usize) -> TopRow<'static> {
    TopRow {
        tabs: &TABS,
        open_tab,
        open_menu: None,
        facts,
    }
}

/// The text of each row of `buf`.
fn rows(buf: &Buffer) -> Vec<String> {
    let a = buf.area;
    (a.y..a.bottom())
        .map(|y| {
            let mut row = String::new();
            let mut x = a.x;
            while x < a.right() {
                let s = buf.cell((x, y)).map_or(" ", |c| c.symbol());
                row.push_str(s);
                x += 1;
            }
            row
        })
        .collect()
}

/// The top row alone, `w` cells wide.
fn top_row(top: &TopRow<'_>, w: u16) -> (String, FrameLayout) {
    let area = Rect::new(0, 0, w, 25);
    let frame = FrameLayout::new(area, Options::default(), top, &Bar::default());
    let mut buf = Buffer::empty(area);
    frame.draw(&mut buf, None, true);
    let row = rows(&buf).into_iter().next().unwrap_or_default();
    (row.trim_end().to_owned(), frame)
}

#[test]
fn the_top_row_shortens_its_facts_as_the_screen_narrows() {
    let t = top(map_facts(), 3);
    let cases = [
        (
            100,
            " File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help padnoir ● 35 not saved · 3 problems",
        ),
        (
            90,
            " File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help         ● 35 · 3 problems",
        ),
        (
            80,
            " File ▾  Actions ▾ │ Fonts Effects Looks Flow Settings Help    ● 35 · 3 problems",
        ),
        (
            60,
            " File ▾  Actions ▾ │ Fonts Effects Looks Flow Settings Help",
        ),
    ];
    for (w, want) in cases {
        let (row, _) = top_row(&t, w);
        assert_eq!(row, want, "at {w} columns");
    }
}

#[test]
fn the_top_row_names_a_saved_document_a_scratch_copy_and_a_live_run() {
    let saved = Facts {
        not_saved: Count::default(),
        ..map_facts()
    };
    let scratch = Facts {
        mode: Some(Mode::ScratchCopy),
        not_saved: Count::opening(1),
        ..map_facts()
    };
    let rehearsal = Facts {
        mode: Some(Mode::ScratchCopy),
        not_saved: Count::default(),
        ..map_facts()
    };
    let live = Facts {
        mode: Some(Mode::Live),
        not_saved: Count::opening(1),
        ..map_facts()
    };
    let clean = Facts {
        problems: Count::default(),
        not_saved: Count::default(),
        ..map_facts()
    };
    let cases = [
        (saved, 100, "padnoir · 3 problems"),
        (saved, 80, "3 problems"),
        (saved, 60, ""),
        (rehearsal, 100, "padnoir · scratch copy · 3 problems"),
        (scratch, 100, "scratch copy ● 1 · 3 problems"),
        (scratch, 90, "scratch copy ● 1 · 3 problems"),
        (scratch, 80, "scratch copy ●"),
        (live, 100, "LIVE ● 1 not saved · 3 problems"),
        (live, 90, "LIVE ● 1 · 3 problems"),
        (live, 80, "LIVE ● · 3 problems"),
        (live, 60, ""),
        (clean, 100, "padnoir"),
        (clean, 60, ""),
    ];
    for (facts, w, want) in cases {
        let (row, _) = top_row(&top(facts, 0), w);
        let at = row.find(" Help").map_or(0, |i| i + " Help".len());
        assert_eq!(
            row.get(at..).unwrap_or("").trim(),
            want,
            "at {w} columns: {row}"
        );
        assert!(row.chars().count() <= usize::from(w), "{row}");
    }
}

#[test]
fn a_count_that_cannot_be_opened_is_not_drawn() {
    let facts = Facts {
        not_saved: Count {
            n: 35,
            opens: false,
        },
        problems: Count { n: 3, opens: false },
        ..map_facts()
    };
    let (row, frame) = top_row(&top(facts, 0), 100);
    assert!(
        row.ends_with("Help                           padnoir ●"),
        "{row}"
    );
    assert_eq!(frame.places().not_saved, None);
    assert_eq!(frame.places().problems, None);
}

#[test]
fn a_click_on_each_place_of_the_top_row_hits_what_is_drawn_there() {
    for w in [100, 90, 80] {
        let (row, frame) = top_row(&top(map_facts(), 0), w);
        let p = frame.places();
        let text = |r: Rect| -> String {
            row.chars()
                .skip(usize::from(r.x))
                .take(usize::from(r.width))
                .collect()
        };
        assert_eq!(text(p.menus[0]).trim(), "File ▾");
        assert_eq!(text(p.menus[1]).trim(), "Actions ▾");
        for (i, tab) in TABS.iter().enumerate() {
            let r = p.tabs[i];
            assert_eq!(text(r).trim(), tab.name, "at {w}");
            for x in r.x..r.right() {
                assert_eq!(p.hit(Position::new(x, 0)), Some(Hit::Tab(i)), "at {w}, {x}");
            }
        }
        assert_eq!(
            p.hit(Position::new(p.menus[0].x, 0)),
            Some(Hit::Menu(MenuName::File))
        );
        assert_eq!(
            p.hit(Position::new(p.menus[1].right() - 1, 0)),
            Some(Hit::Menu(MenuName::Actions))
        );
        let saved = p.not_saved.expect("the count not saved shows");
        let problems = p.problems.expect("the problems count shows");
        assert!(text(saved).starts_with("35"), "at {w}: {}", text(saved));
        assert_eq!(text(problems), "3 problems");
        assert_eq!(p.hit(Position::new(saved.x, 0)), Some(Hit::NotSaved));
        assert_eq!(
            p.hit(Position::new(problems.right() - 1, 0)),
            Some(Hit::Problems)
        );
        assert_eq!(p.hit(Position::new(0, 5)), None);
    }
}

/// The bar of a node's labelled rows, laid out in a row `w` cells wide.
fn bar_of(node: &str, w: u16) -> (Bar, FrameLayout, String) {
    let bar = Bar::from_rows(contract::buttons(node));
    let area = Rect::new(0, 0, w, 25);
    let frame = FrameLayout::new(area, Options::default(), &top(map_facts(), 0), &bar);
    let mut buf = Buffer::empty(area);
    frame.draw(&mut buf, None, true);
    let row = rows(&buf).pop().unwrap_or_default();
    (bar, frame, row)
}

#[test]
fn every_labelled_row_is_on_the_bar_or_behind_more_at_100_and_80_columns() {
    for node in contract::nodes().iter().filter(|n| n.supported) {
        for w in [100u16, 80] {
            let (bar, frame, row) = bar_of(node.id, w);
            let p = frame.places();
            assert!(row.chars().count() <= usize::from(w), "{}: {row}", node.id);
            let shown: Vec<&str> = p
                .buttons
                .iter()
                .filter_map(|(i, _)| bar.buttons.get(*i))
                .map(|b| b.label.as_str())
                .collect();
            let more: Vec<&str> = p
                .hidden
                .iter()
                .filter_map(|i| bar.buttons.get(*i))
                .map(|b| b.label.as_str())
                .collect();
            assert_eq!(
                shown.len() + more.len(),
                bar.buttons.len(),
                "{} at {w}: every button is shown or behind More",
                node.id
            );
            assert_eq!(
                p.more.is_some(),
                !more.is_empty(),
                "{} at {w}: More shows when, and only when, a button is left out",
                node.id
            );
            let menu = bar.more_menu(&p.hidden);
            assert_eq!(menu.len(), more.len(), "{} at {w}", node.id);
            for r in node.buttons().filter(|r| r.bar_key != ".") {
                let reached = shown
                    .iter()
                    .chain(more.iter())
                    .any(|l| l.eq_ignore_ascii_case(r.label));
                assert!(
                    reached,
                    "{} at {w}: {} [{}] is out of reach",
                    node.id, r.id, r.bar_key
                );
            }
            for (i, r) in &p.buttons {
                let b = &bar.buttons[*i];
                let drawn: String = row
                    .chars()
                    .skip(usize::from(r.x))
                    .take(usize::from(r.width))
                    .collect();
                assert_eq!(drawn, format!("[{}] {} ", b.key, b.label), "{}", node.id);
                assert_eq!(p.hit(Position::new(r.x, r.y)), Some(Hit::Button(*i)));
                assert_eq!(
                    p.hit(Position::new(r.right() - 1, r.y)),
                    Some(Hit::Button(*i))
                );
            }
            if let Some(m) = p.more {
                assert_eq!(p.hit(Position::new(m.x + 1, m.y)), Some(Hit::More));
            }
        }
    }
}

#[test]
fn save_menu_and_quit_stay_at_every_width_where_their_rows_act() {
    let mut checked = 0;
    for node in contract::nodes().iter().filter(|n| n.supported) {
        for w in [100u16, 80] {
            let (bar, frame, row) = bar_of(node.id, w);
            for key in ["Ctrl+S", "F10", "q"] {
                if let Some(i) = bar.buttons.iter().position(|b| b.key == key) {
                    let shown = frame.places().buttons.iter().any(|(j, _)| *j == i);
                    assert!(shown, "{} at {w}: [{key}] left out: {row}", node.id);
                    checked += 1;
                }
            }
            let menu_quit: Vec<&Button> = bar
                .buttons
                .iter()
                .filter(|b| b.side == Side::Right)
                .collect();
            if let [.., last] = menu_quit.as_slice() {
                assert!(
                    row.trim_end()
                        .ends_with(&format!("[{}] {}", last.key, last.label)),
                    "{}: {row}",
                    node.id
                );
            }
        }
    }
    assert!(checked > 100, "{checked}");
}

#[test]
fn the_bar_drops_the_lowest_first_as_the_book_draws_it() {
    let bar = |items: &[(&str, &str)]| {
        let mut buttons: Vec<Button> = items.iter().map(|(k, l)| Button::new(*k, *l)).collect();
        for b in &mut buttons {
            b.keep = b.key == "Ctrl+S";
        }
        buttons.push(Button {
            side: Side::Right,
            ..Button::new("F10", "Menu")
        });
        buttons.push(Button {
            side: Side::Right,
            ..Button::new("q", "Quit")
        });
        Bar { buttons }
    };
    let fonts = bar(&[
        ("Space", "Activate"),
        ("A", "Activate missing (1)"),
        ("?", "Search"),
        ("Enter", "Detail"),
        ("t", "Try live"),
        ("Ctrl+S", "Save"),
    ]);
    let looks = bar(&[
        ("Enter", "Edit"),
        ("n", "New"),
        ("r", "Remove"),
        ("?", "Search"),
        ("t", "Try live"),
        ("p", "Picture"),
        ("w", "Resize grid"),
        ("Ctrl+S", "Save"),
    ]);
    let cases = [
        (
            &fonts,
            100u16,
            " [Space] Activate  [A] Activate missing (1)  [Ctrl+S] Save  [.] More           [F10] Menu  [q] Quit",
            vec!["Search", "Detail", "Try live"],
        ),
        (
            &fonts,
            80,
            " [Space] Activate  [Ctrl+S] Save  [.] More                 [F10] Menu  [q] Quit",
            vec!["Activate missing (1)", "Search", "Detail", "Try live"],
        ),
        (
            &looks,
            100,
            " [Enter] Edit  [n] New  [r] Remove  [?] Search  [Ctrl+S] Save  [.] More        [F10] Menu  [q] Quit",
            vec!["Try live", "Picture", "Resize grid"],
        ),
        (
            &looks,
            80,
            " [Enter] Edit  [n] New  [Ctrl+S] Save  [.] More            [F10] Menu  [q] Quit",
            vec!["Remove", "Search", "Try live", "Picture", "Resize grid"],
        ),
    ];
    for (bar, w, want, hidden) in cases {
        let area = Rect::new(0, 0, w, 25);
        let frame = FrameLayout::new(area, Options::default(), &top(map_facts(), 0), bar);
        let mut buf = Buffer::empty(area);
        frame.draw(&mut buf, None, true);
        let row = rows(&buf).pop().unwrap_or_default();
        assert_eq!(row.trim_end(), want.trim_end());
        let names: Vec<&str> = frame
            .places()
            .hidden
            .iter()
            .map(|i| bar.buttons[*i].label.as_str())
            .collect();
        assert_eq!(names, hidden);
    }
}

#[test]
fn the_bar_takes_its_buttons_from_a_node_s_rows() {
    let bar = Bar::from_rows(contract::buttons("fonts"));
    let keys: Vec<(&str, &str, Side)> = bar
        .buttons
        .iter()
        .map(|b| (b.key.as_str(), b.label.as_str(), b.side))
        .collect();
    assert_eq!(keys.first(), Some(&("Space", "Activate", Side::Left)));
    assert!(keys.contains(&("Ctrl+S", "Save", Side::Left)));
    assert!(
        !keys.iter().any(|k| k.0 == "." || k.0 == "Ctrl+C"),
        "{keys:?}"
    );
    let right: Vec<&str> = keys
        .iter()
        .filter(|k| k.2 == Side::Right)
        .map(|k| k.0)
        .collect();
    assert_eq!(right, ["F10", "q"]);
    assert!(bar.buttons.iter().any(|b| b.keep && b.key == "Ctrl+S"));
    // While a pattern is typed, q types, so Ctrl+C is the Quit at the right.
    let search = Bar::from_rows(contract::buttons("any.search"));
    let right: Vec<(&str, &str)> = search
        .buttons
        .iter()
        .filter(|b| b.side == Side::Right)
        .map(|b| (b.key.as_str(), b.label.as_str()))
        .collect();
    assert_eq!(right, [("Ctrl+C", "Quit")]);
    // The five Enter rows of a look's card are one button.
    let card = Bar::from_rows(contract::buttons("looks.look"));
    assert_eq!(card.buttons.iter().filter(|b| b.key == "Enter").count(), 1);
}

#[test]
fn a_message_takes_the_viewer_s_last_row_and_is_cut_to_one_line() {
    let area = Rect::new(0, 0, 80, 25);
    let opts = Options {
        search_rows: 0,
        message: true,
    };
    let frame = FrameLayout::new(area, opts, &top(map_facts(), 0), &Bar::default());
    let mut buf = Buffer::empty(area);
    let long = "Activated ter-u16n.otb, Terminus Regular, for My-Look's fallback; Save makes it real, and Ctrl+Z takes it back.";
    frame.draw(
        &mut buf,
        Some(&Message {
            text: long.to_owned(),
            role: Role::Row,
        }),
        true,
    );
    let r = frame.regions();
    assert_eq!(r.message.map(|m| m.y), Some(r.viewer.bottom()));
    let row = &rows(&buf)[usize::from(r.viewer.bottom())];
    assert!(row.starts_with(" Activated ter-u16n.otb"), "{row}");
    assert!(row.ends_with('…'), "{row}");
    assert_eq!(row.chars().count(), 80);
}

/// One screen of the frame on a tab, as a snapshot reads it: the text,
/// then a grid of the attributes each cell keeps without color.
fn screen(facts: Facts<'static>, tab: usize, w: u16, h: u16, opts: Options, color: bool) -> Buffer {
    let area = Rect::new(0, 0, w, h);
    let bar = Bar::from_rows(contract::buttons(NODES[tab]));
    let frame = FrameLayout::new(area, opts, &top(facts, tab), &bar);
    let mut buf = Buffer::empty(area);
    let message = opts.message.then(|| Message {
        text:
            "Activated ter-u16n.otb as one change: Save makes it real, and [Ctrl+Z] takes it back."
                .to_owned(),
        role: Role::Row,
    });
    frame.draw(&mut buf, message.as_ref(), color);
    buf
}

/// The attributes of each cell: `.` none, `b` bold, `u` underline, `r`
/// reverse video, and a capital for each pair or more: `B` bold and
/// underline, `R` reverse and bold, `U` reverse and underline, `X` all
/// three.
fn attributes(buf: &Buffer) -> Vec<String> {
    use ratatui_core::style::Modifier as M;
    let a = buf.area;
    (a.y..a.bottom())
        .map(|y| {
            (a.x..a.right())
                .map(|x| {
                    let m = buf.cell((x, y)).map_or(M::empty(), |c| c.modifier);
                    match (
                        m.contains(M::BOLD),
                        m.contains(M::UNDERLINED),
                        m.contains(M::REVERSED),
                    ) {
                        (false, false, false) => '.',
                        (true, false, false) => 'b',
                        (false, true, false) => 'u',
                        (false, false, true) => 'r',
                        (true, true, false) => 'B',
                        (true, false, true) => 'R',
                        (false, true, true) => 'U',
                        (true, true, true) => 'X',
                    }
                })
                .collect()
        })
        .collect()
}

fn snapshot_text(buf: &Buffer) -> String {
    let mut out = rows(buf).join("\n");
    out.push_str("\n-- attributes without color --\n");
    out.push_str(&attributes(buf).join("\n"));
    out.push('\n');
    out
}

/// Compares `text` with the snapshot `name`, or writes it under
/// `VTIUM_CONFIG_BLESS`.
fn snapshot(name: &str, text: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("{name}.txt"));
    if std::env::var_os("VTIUM_CONFIG_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the folder");
        std::fs::write(&path, text).expect("the snapshot");
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        want == text,
        "{name} differs from {}; rerun with VTIUM_CONFIG_BLESS=1 to write it again\n{text}",
        path.display()
    );
}

/// Every cell the color look draws in a color keeps an attribute without
/// color, or belongs to a word, such as `3 problems`, so the mark reads
/// the same on a monochrome console.
fn marks_survive_without_color(color: &Buffer, mono: &Buffer, what: &str) {
    let a = color.area;
    for y in a.y..a.bottom() {
        for x in a.x..a.right() {
            let (Some(c), Some(m)) = (color.cell((x, y)), mono.cell((x, y))) else {
                continue;
            };
            assert_eq!(
                c.symbol(),
                m.symbol(),
                "{what}: the text differs at {x},{y}"
            );
            let colored = c.fg != ratatui_core::style::Color::Reset;
            if colored {
                let attribute = !m.modifier.is_empty();
                let word = c.symbol().chars().any(char::is_alphanumeric);
                let blank = c.symbol() == " ";
                assert!(
                    attribute || word || blank,
                    "{what}: {:?} at {x},{y} is marked by color alone",
                    c.symbol()
                );
            }
            assert_eq!(
                m.fg,
                ratatui_core::style::Color::Reset,
                "{what}: color at {x},{y}"
            );
        }
    }
}

#[test]
fn the_frame_on_each_tab_at_both_sizes_with_both_fixtures() {
    let fixtures = [("map", map_facts()), ("large", large_facts())];
    for (tab, node) in NODES.iter().enumerate() {
        let mut text = String::new();
        for (fixture, facts) in fixtures {
            for (w, h) in [(100u16, 30u16), (80, 25)] {
                // The Settings tab shows its search row, and the Fonts tab
                // a message, so both rows of the frame are in a snapshot.
                let opts = Options {
                    search_rows: u16::from(*node == "settings"),
                    message: *node == "fonts",
                };
                let color = screen(facts, tab, w, h, opts, true);
                let mono = screen(facts, tab, w, h, opts, false);
                let what = format!("{node} {fixture} {w}x{h}");
                for row in rows(&color) {
                    assert_eq!(row.chars().count(), usize::from(w), "{what}: {row}");
                }
                marks_survive_without_color(&color, &mono, &what);
                let all = rows(&color);
                let (first, last) = (&all[0], &all[all.len() - 1]);
                for tab in TABS {
                    assert!(first.contains(tab.name), "{what}: {first}");
                }
                assert!(
                    first.contains("File ▾") && first.contains("Actions ▾"),
                    "{what}"
                );
                assert!(first.contains("problems"), "{what}: {first}");
                if fixture == "large" {
                    assert!(first.contains("LIVE"), "{what}: {first}");
                }
                for key in ["[Ctrl+S] Save", "[F10] Menu", "[q] Quit"] {
                    let acts = Bar::from_rows(contract::buttons(node))
                        .buttons
                        .iter()
                        .any(|b| format!("[{}] {}", b.key, b.label) == key);
                    if acts {
                        assert!(last.contains(key), "{what}: {last}");
                    }
                }
                text.push_str(&format!("== {fixture} {w}x{h} ==\n"));
                text.push_str(&snapshot_text(&mono));
            }
        }
        snapshot(&format!("frame-{node}"), &text);
    }
}

#[test]
fn the_open_tab_and_every_alt_letter_keep_their_marks_without_color() {
    let buf = screen(map_facts(), 3, 100, 30, Options::default(), false);
    let marks = &attributes(&buf)[0];
    let row = &rows(&buf)[0];
    let letters: Vec<(char, char)> = row
        .chars()
        .zip(marks.chars())
        .filter(|(_, m)| matches!(m, 'B' | 'X'))
        .collect();
    assert_eq!(
        letters,
        [
            ('F', 'B'),
            ('A', 'B'),
            ('n', 'B'),
            ('E', 'B'),
            ('L', 'B'),
            ('o', 'X'),
            ('S', 'B'),
            ('H', 'B')
        ]
    );
    let flow = row
        .find(" Flow ")
        .map(|i| row[..i].chars().count())
        .unwrap_or(0);
    let lit: String = marks.chars().skip(flow).take(6).collect();
    assert_eq!(lit, "rrrXrr");
}

#[test]
fn an_open_menu_s_name_is_highlighted() {
    let area = Rect::new(0, 0, 100, 30);
    let t = TopRow {
        open_menu: Some(MenuName::Actions),
        ..top(map_facts(), 0)
    };
    let frame = FrameLayout::new(area, Options::default(), &t, &Bar::default());
    let mut buf = Buffer::empty(area);
    frame.draw(&mut buf, None, false);
    let marks = &attributes(&buf)[0];
    let actions = frame.places().menus[1];
    let lit: String = marks
        .chars()
        .skip(usize::from(actions.x))
        .take(usize::from(actions.width))
        .collect();
    assert_eq!(lit, "rXrrrrrrrrr");
    assert!(marks.starts_with(".B......"), "{marks}");
}
