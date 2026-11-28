//! The menus and sheets as they are drawn: a menu's items, separators,
//! check marks, submenus and greyed items, the places a click maps to,
//! and the File menu, its submenus, the Actions menu, More's menu and a
//! sheet over the frame at 100×30 and 80×25, with the screen map's
//! configurations and with a large machine's dozens of them.
//!
//! Each screen's text is kept under `tests/snapshots/`, with a grid of
//! the attributes that show where no color does.
//! `VTIUM_CONFIG_BLESS=1 cargo test -p vtium-config-widgets --test menu`
//! writes the snapshots again.
//!
//! References:
//! - ADR-1403 D11, "The menus", "The mouse" and the nodes
//!   `any.menu.file`, `any.menu.file.open`, `any.menu.file.history` and
//!   `any.menu.actions`.
//! - ADR-1206 D10: 80×25, `NO_COLOR`, a monochrome console, and lists that
//!   stay readable with dozens of items.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};
use vtium_config_widgets::contract;
use vtium_config_widgets::frame::{
    Bar, Count, Facts, FrameLayout, MenuName, Mode, Options, TabName, TopRow,
};
use vtium_config_widgets::menu::{
    self, Entry, Item, MenuHit, Open, Opens, Pick, Pull, rows_shown, sheet,
};
use vtium_config_widgets::tab::MenuItem;

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

/// What a screen of the menus is drawn from: the screen map's machine,
/// padnoir, or a large machine with dozens of configurations and hundreds
/// of saved versions.
struct Fixture {
    name: &'static str,
    facts: Facts<'static>,
    undo: String,
    configurations: Vec<(String, String)>,
    versions: Vec<String>,
}

fn map() -> Fixture {
    Fixture {
        name: "map",
        facts: Facts {
            name: "padnoir",
            mode: None,
            not_saved: Count::opening(35),
            problems: Count::opening(3),
        },
        undo: "Undo font-size = 11 in My-Look".to_owned(),
        configurations: [
            ("padnoir", "● 35 not saved"),
            ("travel", "saved"),
            ("kiosk-test", "● 2 not saved"),
        ]
        .map(|(n, s)| (n.to_owned(), s.to_owned()))
        .to_vec(),
        versions: [
            "2026-11-12 08:53   saved by aaron",
            "2026-11-04 17:10   saved by aaron",
            "2026-10-30 21:42   saved by aaron",
        ]
        .map(str::to_owned)
        .to_vec(),
    }
}

fn large() -> Fixture {
    Fixture {
        name: "large",
        facts: Facts {
            name: "threadripper-ten-gpus-lab-bench-07",
            mode: Some(Mode::Live),
            not_saved: Count::opening(1284),
            problems: Count::opening(57),
        },
        undo: "Undo font-size = 14 in Lab-Wall-Display-Seven-Of-Ten-With-A-Long-Name".to_owned(),
        configurations: (0..64)
            .map(|i| {
                (
                    format!("lab-bench-{i:02}-display-wall-with-ten-gpus"),
                    if i % 3 == 0 {
                        format!("● {} not saved", i * 7 + 1)
                    } else {
                        "saved".to_owned()
                    },
                )
            })
            .collect(),
        versions: (0..240)
            .map(|i| {
                format!(
                    "2026-{:02}-{:02} {:02}:{:02}   saved by aaron",
                    11 - i / 28,
                    28 - i % 28,
                    23 - i % 24,
                    (i * 7) % 60
                )
            })
            .collect(),
    }
}

/// The File menu as ADR-1403 D11's table gives it, on the Flow tab, where
/// the draft's letters n, v, r and o show in the column of shortcuts.
fn file_menu(f: &Fixture) -> Vec<Entry> {
    vec![
        Entry::Item(Item::new("New…").letter('n').shortcut("n")),
        Entry::Item(Item::new("Open").letter('o').submenu()),
        Entry::Separator,
        Entry::Item(Item::new(&f.undo).letter('u').shortcut("Ctrl+Z")),
        Entry::Item(
            Item::new("Redo")
                .letter('d')
                .shortcut("Ctrl+Y")
                .greyed("nothing to redo"),
        ),
        Entry::Separator,
        Entry::Item(Item::new("Save…").letter('s').shortcut("Ctrl+S")),
        Entry::Item(Item::new("Show changes").letter('c').shortcut("v")),
        Entry::Item(Item::new("Edit as text…").letter('e')),
        Entry::Separator,
        Entry::Item(Item::new("Revert to saved…").letter('r').shortcut("r")),
        Entry::Item(Item::new("History").letter('h').shortcut("o").submenu()),
        Entry::Separator,
        Entry::Item(Item::new("Quit").letter('q').shortcut("q")),
    ]
}

const OPEN: usize = 1;
const HISTORY: usize = 11;

fn open_menu(f: &Fixture) -> Vec<Entry> {
    let width = f
        .configurations
        .iter()
        .map(|(n, _)| n.chars().count())
        .max()
        .unwrap_or(0);
    let mut out: Vec<Entry> = f
        .configurations
        .iter()
        .enumerate()
        .map(|(i, (name, state))| {
            Entry::Item(Item::new(format!("{name:<width$}   {state}")).check(i == 0))
        })
        .collect();
    out.push(Entry::Separator);
    out.push(Entry::Item(Item::new("New…").letter('n')));
    out.push(Entry::Item(Item::new("Manage configurations…").letter('m')));
    out
}

fn history_menu(f: &Fixture) -> Vec<Entry> {
    f.versions
        .iter()
        .map(|v| Entry::Item(Item::new(v)))
        .collect()
}

fn no_history() -> Vec<Entry> {
    vec![Entry::Item(
        Item::new("No earlier version").greyed("no save replaced one yet"),
    )]
}

/// The Actions menu, built from the items a tab hands it, so the menu
/// draws what the `Tab` trait gives.
fn actions_menu(try_live: Option<&str>, own_font: bool) -> Vec<Entry> {
    let items = [
        MenuItem {
            label: "Try live".to_owned(),
            letter: Some('t'),
            shortcut: Some("t"),
            action: "t.view",
            greyed: try_live.map(str::to_owned),
        },
        MenuItem {
            label: "Read devices again".to_owned(),
            letter: Some('r'),
            shortcut: Some("d"),
            action: "d.view",
            greyed: None,
        },
        MenuItem {
            label: "Install as a system font…".to_owned(),
            letter: Some('i'),
            shortcut: None,
            action: "install",
            greyed: (!own_font).then(|| "on a font of your own".to_owned()),
        },
    ];
    items.iter().map(|i| Entry::Item(Item::from(i))).collect()
}

fn rows(buf: &Buffer) -> Vec<String> {
    let a = buf.area;
    (a.y..a.bottom())
        .map(|y| {
            (a.x..a.right())
                .map(|x| buf.cell((x, y)).map_or(" ", |c| c.symbol()).to_owned())
                .collect()
        })
        .collect()
}

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

#[test]
fn a_menu_draws_its_separators_checks_submenus_and_greyed_items() {
    let entries = vec![
        Entry::Item(Item::new("New…").letter('n').shortcut("n")),
        Entry::Item(Item::new("Open").letter('o').submenu()),
        Entry::Separator,
        Entry::Item(
            Item::new("Whole file")
                .letter('w')
                .shortcut("w")
                .check(true),
        ),
        Entry::Item(Item::new("Changes only").letter('c').check(false)),
        Entry::Item(
            Item::new("Redo")
                .letter('d')
                .shortcut("Ctrl+Y")
                .greyed("nothing to redo"),
        ),
    ];
    let area = Rect::new(0, 0, 60, 20);
    let open = Open {
        at: Position::new(2, 1),
        opens: Opens::Down,
        menu: Pull::new(&entries).highlight(3),
        sub: None,
    };
    let layout = menu::layout(area, &open);
    let mut buf = Buffer::empty(area);
    layout.draw(&mut buf, false);
    let text = rows(&buf);
    let want = [
        "  ┌────────────────────────────────────┐",
        "  │   New…                         n   │",
        "  │   Open                           › │",
        "  ├────────────────────────────────────┤",
        "  │ ✓ Whole file                   w   │",
        "  │   Changes only                     │",
        "  │   Redo · nothing to redo  Ctrl+Y   │",
        "  └────────────────────────────────────┘",
    ];
    for (i, w) in want.iter().enumerate() {
        assert_eq!(text[i + 1].trim_end(), *w, "row {}", i + 1);
    }
    let marks = attributes(&buf);
    // The highlighted item is reverse video across the box's whole
    // width, its letter bold and underlined over it.
    assert_eq!(marks[5][3..39], format!("rrrX{}", "r".repeat(32)));
    assert_eq!(&marks[5][..3], "...");
    assert_eq!(&marks[5][39..40], ".");
    // Each item's letter is bold and underlined; a greyed item's is not.
    assert_eq!(&marks[2][3..10], "...B...");
    assert_eq!(&marks[3][3..10], "...B...");
    assert!(!marks[7].contains('B'), "{}", marks[7]);
    // The places: one row for each item, across the box, none for the
    // separator, and a click on each maps to its item.
    let places = layout.places();
    let picks: Vec<(u16, Pick)> = places.items.iter().map(|(r, p)| (r.y, *p)).collect();
    assert_eq!(
        picks,
        [
            (2, Pick::Item(0)),
            (3, Pick::Item(1)),
            (5, Pick::Item(3)),
            (6, Pick::Item(4)),
            (7, Pick::Item(5)),
        ]
    );
    for (r, p) in &places.items {
        assert_eq!((r.x, r.width), (3, 36));
        for x in r.x..r.right() {
            assert_eq!(places.hit(Position::new(x, r.y)), MenuHit::Item(*p));
        }
    }
    assert_eq!(places.hit(Position::new(10, 4)), MenuHit::Inside);
    assert_eq!(places.hit(Position::new(2, 2)), MenuHit::Inside);
    assert_eq!(places.hit(Position::new(40, 4)), MenuHit::Outside);
    assert_eq!(places.hit(Position::new(39, 5)), MenuHit::Inside);
    assert_eq!(places.hit(Position::new(10, 12)), MenuHit::Outside);
}

#[test]
fn a_submenu_opens_at_the_right_of_its_item_with_its_first_entry_on_the_item_s_row() {
    let f = map();
    let file = file_menu(&f);
    let open = open_menu(&f);
    let area = Rect::new(0, 0, 100, 30);
    let layout = menu::layout(
        area,
        &Open {
            at: Position::new(0, 1),
            opens: Opens::Down,
            menu: Pull::new(&file).highlight(OPEN),
            sub: Some((OPEN, Pull::new(&open).highlight(1))),
        },
    );
    let places = layout.places();
    let [main, sub] = places.boxes[..] else {
        panic!("two boxes: {:?}", places.boxes);
    };
    assert_eq!(sub.x, main.right());
    let parent = places
        .items
        .iter()
        .find(|(_, p)| *p == Pick::Item(OPEN))
        .map(|(r, _)| r.y);
    let first = places
        .items
        .iter()
        .find(|(_, p)| *p == Pick::Sub(0))
        .map(|(r, _)| r.y);
    assert_eq!(parent, first);
    let mut buf = Buffer::empty(area);
    layout.draw(&mut buf, true);
    let text = rows(&buf);
    let travel = places
        .items
        .iter()
        .find(|(_, p)| *p == Pick::Sub(1))
        .map(|(r, _)| *r)
        .expect("travel is shown");
    let row: String = text[usize::from(travel.y)]
        .chars()
        .skip(usize::from(travel.x))
        .take(usize::from(travel.width))
        .collect();
    assert_eq!(row.trim(), "travel       saved");
    assert_eq!(
        places.hit(Position::new(travel.x + 3, travel.y)),
        MenuHit::Item(Pick::Sub(1))
    );
    // The open configuration carries the check, in its own column.
    assert!(text[usize::from(parent.unwrap_or(0))].contains("│ ✓ padnoir"));
}

#[test]
fn a_submenu_with_no_room_at_the_right_opens_at_the_left_and_stays_on_the_screen() {
    let f = map();
    let file = file_menu(&f);
    let history = history_menu(&f);
    let area = Rect::new(0, 0, 100, 30);
    let layout = menu::layout(
        area,
        &Open {
            at: Position::new(50, 1),
            opens: Opens::Down,
            menu: Pull::new(&file).highlight(HISTORY),
            sub: Some((HISTORY, Pull::new(&history))),
        },
    );
    let [main, sub] = layout.places().boxes[..] else {
        panic!("two boxes");
    };
    assert!(main.right() <= area.right(), "{main:?}");
    assert_eq!(sub.right(), main.x, "{sub:?} beside {main:?}");
    // A menu anchored near the screen's right edge moves left to fit.
    let narrow = Rect::new(0, 1, 80, 23);
    let layout = menu::layout(
        narrow,
        &Open {
            at: Position::new(70, 1),
            opens: Opens::Down,
            menu: Pull::new(&file),
            sub: None,
        },
    );
    let main = layout.places().boxes[0];
    assert_eq!(main.right(), narrow.right(), "{main:?}");
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 25));
    layout.draw(&mut buf, false);
    let text = rows(&buf);
    assert!(text[1].ends_with('┐') && text[1].contains('┌'), "{}", text[1]);
    assert!(text[2].ends_with('│'), "{}", text[2]);
    // Where neither side has room, as at 80 columns, the submenu keeps to
    // the screen's right edge, over the right of the menu, so the
    // menu's names still show.
    let area = Rect::new(0, 0, 80, 25);
    let layout = menu::layout(
        area,
        &Open {
            at: Position::new(0, 1),
            opens: Opens::Down,
            menu: Pull::new(&file).highlight(HISTORY),
            sub: Some((HISTORY, Pull::new(&history))),
        },
    );
    let [main, sub] = layout.places().boxes[..] else {
        panic!("two boxes");
    };
    assert_eq!(sub.right(), area.right());
    assert!(sub.x > main.x + main.width / 2, "{sub:?} over {main:?}");
    for b in &layout.places().boxes {
        assert!(
            b.right() <= area.right() && b.bottom() <= area.bottom(),
            "{b:?}"
        );
    }
}

#[test]
fn a_menu_longer_than_the_screen_shows_a_window_and_says_how_many_more() {
    let f = large();
    let history = history_menu(&f);
    let area = Rect::new(0, 0, 80, 25);
    let at = Position::new(0, 1);
    let shown = rows_shown(area, at, Opens::Down, history.len(), 0);
    assert_eq!(shown, 21);
    let layout = menu::layout(
        area,
        &Open {
            at,
            opens: Opens::Down,
            menu: Pull {
                entries: &history,
                highlight: Some(120),
                offset: 110,
            },
            sub: None,
        },
    );
    let mut buf = Buffer::empty(area);
    layout.draw(&mut buf, false);
    let text = rows(&buf);
    assert!(text[2].contains("↑ 110 more"), "{}", text[2]);
    assert!(text[23].contains("↓ 110 more"), "{}", text[23]);
    assert_eq!(rows_shown(area, at, Opens::Down, history.len(), 110), 20);
    let picks: Vec<Pick> = layout.places().items.iter().map(|(_, p)| *p).collect();
    assert_eq!(picks.first(), Some(&Pick::Item(110)));
    assert_eq!(picks.last(), Some(&Pick::Item(129)));
    let lit = layout
        .places()
        .items
        .iter()
        .find(|(_, p)| *p == Pick::Item(120))
        .map(|(r, _)| r.y)
        .expect("the highlight is in view");
    assert!(attributes(&buf)[usize::from(lit)].contains("rrrrr"));
}

#[test]
fn more_s_menu_opens_up_over_the_bar_and_lists_what_the_bar_left_out() {
    let bar = Bar::from_rows(contract::buttons("fonts"));
    let area = Rect::new(0, 0, 80, 25);
    let top = TopRow {
        tabs: &TABS,
        open_tab: 0,
        open_menu: None,
        facts: map().facts,
    };
    let frame = FrameLayout::new(area, Options::default(), &top, &bar);
    let p = frame.places();
    let more = p.more.expect("More shows at 80 columns");
    let entries = bar.more_menu(&p.hidden);
    let layout = menu::layout(
        p.menu_area(),
        &Open {
            at: Position::new(more.x, p.regions.bottom_rule.y),
            opens: Opens::Up,
            menu: Pull::new(&entries),
            sub: None,
        },
    );
    let b = layout.places().boxes[0];
    assert_eq!(b.bottom(), p.regions.bottom_rule.y + 1);
    let mut buf = Buffer::empty(area);
    frame.draw(&mut buf, None, false);
    layout.draw(&mut buf, false);
    let text = rows(&buf);
    let inside: Vec<String> = text[usize::from(b.y)..usize::from(b.bottom())]
        .iter()
        .map(|r| {
            r.chars()
                .skip(usize::from(b.x))
                .take(usize::from(b.width))
                .collect()
        })
        .collect();
    assert_eq!(inside.len(), p.hidden.len() + 2);
    for i in &p.hidden {
        let button = &bar.buttons[*i];
        let line = inside
            .iter()
            .find(|r| r.contains(&format!(" {} ", button.label)))
            .unwrap_or_else(|| panic!("{} is not in More's menu", button.label));
        assert!(line.contains(&button.key), "{line}");
        assert!(!line.contains(&format!("[{}]", button.key)), "{line}");
    }
}

#[test]
fn a_sheet_has_a_title_a_close_mark_and_room_for_its_lines() {
    let area = Rect::new(0, 0, 100, 30);
    let s = sheet::layout(
        sheet::centered(Rect::new(0, 2, 100, 26), 90, 8),
        "Problems",
        true,
    );
    let mut buf = Buffer::empty(area);
    s.draw(&mut buf, false);
    let text = rows(&buf);
    let top = &text[usize::from(s.area().y)];
    assert!(top.trim().starts_with("┌ Problems ─"), "{top}");
    assert!(top.trim().ends_with("─ × ┐"), "{top}");
    let close = s.close().expect("a close mark");
    assert_eq!(close.y, s.area().y);
    let mark: String = top
        .chars()
        .skip(usize::from(close.x))
        .take(usize::from(close.width))
        .collect();
    assert_eq!(mark, " × ");
    assert!(s.hits_close(Position::new(close.x + 1, close.y)));
    assert!(!s.hits_close(Position::new(close.x, close.y + 1)));
    assert_eq!(s.inner(), Rect::new(7, 12, 86, 6));
    let bottom = &text[usize::from(s.area().bottom() - 1)];
    assert!(bottom.trim().starts_with('└') && bottom.trim().ends_with('┘'));
}

/// The screens of the menus, each over the frame on the Flow tab, or on
/// the Fonts tab for Actions as the catalog draws it.
fn menu_screen(f: &Fixture, which: &str, w: u16, h: u16, color: bool) -> (Buffer, Vec<String>) {
    let area = Rect::new(0, 0, w, h);
    let file = file_menu(f);
    let open = open_menu(f);
    let history = history_menu(f);
    let empty = no_history();
    let actions = actions_menu(None, false);
    let actions_greyed = actions_menu(Some("not a vtium console"), true);
    let (menu_name, tab, node) = match which {
        "actions" | "actions-greyed" => (Some(MenuName::Actions), 0, "any.menu.actions"),
        "more" | "sheet" => (None, 0, "fonts"),
        "file-open" => (Some(MenuName::File), 3, "any.menu.file.open"),
        "file-history" | "file-history-empty" => (Some(MenuName::File), 3, "any.menu.file.history"),
        _ => (Some(MenuName::File), 3, "any.menu.file"),
    };
    let top = TopRow {
        tabs: &TABS,
        open_tab: tab,
        open_menu: menu_name,
        facts: f.facts,
    };
    let bar = Bar::from_rows(contract::buttons(node));
    let frame = FrameLayout::new(area, Options::default(), &top, &bar);
    let mut buf = Buffer::empty(area);
    frame.draw(&mut buf, None, color);
    let p = frame.places();
    let file_at = Position::new(p.menus[0].x, p.regions.top_rule.y);
    let actions_at = Position::new(p.menus[1].x, p.regions.top_rule.y);
    let under = p.menu_area();
    let mut problems = Vec::new();
    match which {
        "sheet" => {
            let s = sheet::layout(sheet::centered(p.regions.viewer, 90, 8), "Problems", true);
            s.draw(&mut buf, color);
            let inner = s.inner();
            buf.set_stringn(
                inner.x,
                inner.y,
                "Looks › My-Look › Scripts   the layer nerd-symbols names a font that is not installed",
                usize::from(inner.width),
                ratatui_core::style::Style::new(),
            );
            if inner.width < 40 {
                problems.push(format!("the sheet's lines have {} cells", inner.width));
            }
        }
        "more" => {
            let entries = bar.more_menu(&p.hidden);
            let at = Position::new(p.more.map_or(0, |m| m.x), p.regions.bottom_rule.y);
            menu::layout(
                under,
                &Open {
                    at,
                    opens: Opens::Up,
                    menu: Pull::new(&entries).highlight(0),
                    sub: None,
                },
            )
            .draw(&mut buf, color);
        }
        _ => {
            let (at, main, sub) = match which {
                "actions" => (actions_at, Pull::new(&actions).highlight(0), None),
                "actions-greyed" => (actions_at, Pull::new(&actions_greyed).highlight(2), None),
                "file-open" => (
                    file_at,
                    Pull::new(&file).highlight(OPEN),
                    Some((OPEN, Pull::new(&open).highlight(1))),
                ),
                "file-history" => (
                    file_at,
                    Pull::new(&file).highlight(HISTORY),
                    Some((HISTORY, Pull::new(&history).highlight(1))),
                ),
                "file-history-empty" => (
                    file_at,
                    Pull::new(&file).highlight(HISTORY),
                    Some((HISTORY, Pull::new(&empty))),
                ),
                _ => (file_at, Pull::new(&file).highlight(0), None),
            };
            let layout = menu::layout(
                under,
                &Open {
                    at,
                    opens: Opens::Down,
                    menu: main,
                    sub,
                },
            );
            for b in &layout.places().boxes {
                if b.right() > under.right() || b.bottom() > under.bottom() || b.y < under.y {
                    problems.push(format!("a box leaves the screen: {b:?}"));
                }
            }
            if layout.places().items.is_empty() {
                problems.push("no item shows".to_owned());
            }
            layout.draw(&mut buf, color);
        }
    }
    (buf, problems)
}

#[test]
fn every_menu_screen_at_both_sizes_with_both_fixtures() {
    let screens = [
        "file",
        "file-open",
        "file-history",
        "file-history-empty",
        "actions",
        "actions-greyed",
        "more",
        "sheet",
    ];
    for which in screens {
        let mut text = String::new();
        for f in [map(), large()] {
            for (w, h) in [(100u16, 30u16), (80, 25)] {
                let what = format!("{which} {} {w}x{h}", f.name);
                let (color, problems) = menu_screen(&f, which, w, h, true);
                let (mono, _) = menu_screen(&f, which, w, h, false);
                assert!(problems.is_empty(), "{what}: {problems:?}");
                let all = rows(&mono);
                assert_eq!(all, rows(&color), "{what}: the text differs without color");
                for row in &all {
                    assert_eq!(row.chars().count(), usize::from(w), "{what}: {row}");
                }
                // A greyed item says why in words, so it reads the same
                // where no grey shows.
                for (y, row) in all.iter().enumerate() {
                    let greyed = (0..w).any(|x| {
                        color.cell((x, y as u16)).is_some_and(|c| {
                            c.fg == ratatui_core::style::Color::DarkGray
                                && c.symbol().chars().any(char::is_alphabetic)
                        })
                    });
                    // The rows that say how many more entries a long
                    // menu holds are grey words of their own.
                    let marker = row.contains("↑ ") || row.contains("↓ ");
                    if greyed && row.contains('│') && !marker {
                        assert!(row.contains(" · "), "{what}: greyed with no reason: {row}");
                    }
                }
                for y in 0..h {
                    for x in 0..w {
                        let m = mono.cell((x, y)).map(|c| c.fg);
                        assert_eq!(m, Some(ratatui_core::style::Color::Reset), "{what}");
                    }
                }
                text.push_str(&format!("== {} {w}x{h} ==\n", f.name));
                text.push_str(&all.join("\n"));
                text.push_str("\n-- attributes without color --\n");
                text.push_str(&attributes(&mono).join("\n"));
                text.push('\n');
            }
        }
        snapshot(&format!("menu-{which}"), &text);
    }
}
