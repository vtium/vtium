//! What the Help tab's tests share: the two fixtures, a whole screen with
//! the frame around the tab, the rows of a drawn buffer as text, the
//! checks every screen passes, and the snapshots.
//!
//! References:
//! - ADR-1206 D10: each screen at 100×30 and 80×25, with the screen map's
//!   fixture and a large one, readable without color.

#![allow(dead_code, reason = "each test binary uses a part of the helpers")]

use std::path::PathBuf;

use ratatui_core::backend::TestBackend;
use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier};
use ratatui_core::terminal::Terminal;
use vtium_config_core::fixture;
use vtium_config_core::ui::{Action, Shared};
use vtium_config_tab_help::HelpTab;
use vtium_config_widgets::backend::console_width;
use vtium_config_widgets::canvas;
use vtium_config_widgets::frame::{Bar, Count, Facts, FrameLayout, Options, TabName, TopRow};
use vtium_config_widgets::search;
use vtium_config_widgets::tab::Tab;

/// The tabs of the top row.
pub const TABS: [TabName<'static>; 6] = [
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

/// The two fixtures, by name: the screen map's machine and a large one.
pub fn fixtures() -> [(&'static str, Shared); 2] {
    [
        ("map", fixture::padnoir::shared()),
        ("large", fixture::large::shared()),
    ]
}

/// The two sizes every screen is drawn at.
pub const SIZES: [(u16, u16); 2] = [(100, 30), (80, 25)];

/// The About text the screens show: what `vtium-config --help about`
/// prints where no logo is drawn, with the version a word that does not
/// change at each release.
pub const ABOUT: &str = "vtium-config VERSION\nvtium-config sets up the vtium console: its fonts and frits, its settings and heads, checked before they are saved.\n";

/// The Help tab with the About text of the screens, in color or not.
pub fn tab(color: bool) -> HelpTab {
    HelpTab::with_about(color, ABOUT)
}

/// The tab after `rows`, each an action carrying nothing.
pub fn after(tab: &mut HelpTab, shared: &Shared, rows: &[&'static str]) {
    for row in rows {
        let commands = tab.update(shared, Action::of(row));
        assert!(commands.is_empty(), "{row} returned {commands:?}");
    }
}

/// The whole screen of `cols` by `rows` with the Help tab open: the
/// frame's top row and action bar from `shared` and the contract, the
/// tab in the viewer, and the search row while the tab's search is open.
pub fn screen(tab: &HelpTab, shared: &Shared, cols: u16, rows: u16, color: bool) -> Buffer {
    let area = Rect::new(0, 0, cols, rows);
    let doc = shared.document();
    let top = TopRow {
        tabs: &TABS,
        open_tab: 5,
        open_menu: None,
        facts: Facts {
            name: doc.name(),
            mode: None,
            not_saved: Count::opening(doc.count()),
            problems: Count::opening(0),
        },
    };
    let node = if tab.search().is_some_and(|s| s.typing()) {
        "any.search"
    } else if tab.search().is_some() {
        "any.filter"
    } else {
        tab.node()
    };
    let bar = Bar::from_rows(vtium_config_widgets::contract::buttons(node));
    let options = Options {
        search_rows: u16::from(tab.search().is_some()),
        message: false,
    };
    let frame = FrameLayout::new(area, options, &top, &bar);
    let regions = *frame.regions();
    let mut terminal = Terminal::new(TestBackend::new(cols, rows)).expect("a terminal");
    terminal
        .draw(|f| {
            frame.draw(f.buffer_mut(), None, color);
            tab.draw(shared, f, regions.viewer);
            if let (Some(s), Some(r)) = (tab.search(), regions.search) {
                search::row::draw(s, f.buffer_mut(), r, color);
            }
        })
        .expect("a frame");
    terminal.backend().buffer().clone()
}

/// The viewer of a screen of `cols` by `rows` with no search row.
pub fn viewer(cols: u16, rows: u16) -> Rect {
    Rect::new(0, 2, cols, rows.saturating_sub(4))
}

/// The text of each row of `buf`. A cell that a wide character covers, by
/// the console's width rule, adds nothing.
pub fn rows_of(buf: &Buffer) -> Vec<String> {
    let area = buf.area;
    (area.top()..area.bottom())
        .map(|y| {
            let mut row = String::new();
            let mut x = area.left();
            while x < area.right() {
                let symbol = buf[(x, y)].symbol();
                row.push_str(symbol);
                x = x.saturating_add(console_width(symbol).max(1));
            }
            row
        })
        .collect()
}

/// Every row of the screen is the terminal's width and no wider, and the
/// screen has the terminal's rows.
pub fn check_rows(buf: &Buffer, cols: u16, rows: u16, what: &str) {
    let text = rows_of(buf);
    assert_eq!(text.len(), usize::from(rows), "{what}");
    for (y, row) in text.iter().enumerate() {
        assert_eq!(
            canvas::cells(row),
            usize::from(cols),
            "{what}: row {y} is {} cells: {row:?}",
            canvas::cells(row)
        );
    }
}

/// Drawn without color, the screen keeps the same text and sets no color,
/// and every cell that the color look draws in a color keeps an attribute
/// without color, or belongs to a word, is a blank or the dot between two words, or is a
/// line
/// that draws the frame, so no mark is carried by color alone.
pub fn check_without_color(color: &Buffer, mono: &Buffer, what: &str) {
    let a = color.area;
    for y in a.top()..a.bottom() {
        for x in a.left()..a.right() {
            let (c, m) = (&color[(x, y)], &mono[(x, y)]);
            assert_eq!(
                c.symbol(),
                m.symbol(),
                "{what}: the text differs at {x},{y}"
            );
            assert!(
                m.fg == Color::Reset && m.bg == Color::Reset,
                "{what}: a color without color at {x},{y}"
            );
            if c.fg != Color::Reset || c.bg != Color::Reset {
                let s = c.symbol();
                let attribute = !m.modifier.is_empty();
                let word = in_word(color, x, y);
                // a blank, or the dot that separates two words, as the
                // search row's help writes `case ignored · topic`
                let blank = s == " " || s == "·";
                let frame = s.chars().all(|ch| ('\u{2500}'..='\u{257f}').contains(&ch));
                assert!(
                    attribute || word || blank || frame,
                    "{what}: {s:?} at {x},{y} is marked by color alone"
                );
            }
        }
    }
}

/// Whether the cell at `x`, `y` belongs to a word: a run of cells with no
/// blank between them, such as `^start` or `[PgDn]`, that holds a letter
/// or a digit.
fn in_word(buf: &Buffer, x: u16, y: u16) -> bool {
    let a = buf.area;
    let filled = |x: u16| buf[(x, y)].symbol() != " ";
    let mut from = x;
    while from > a.left() && filled(from - 1) {
        from -= 1;
    }
    let mut to = x;
    while to + 1 < a.right() && filled(to + 1) {
        to += 1;
    }
    (from..=to).any(|x| buf[(x, y)].symbol().chars().any(char::is_alphanumeric))
}

/// The attributes of each cell: `.` none, `b` bold, `u` underline, `r`
/// reverse video, and a capital for each pair or more: `B` bold and
/// underline, `R` reverse and bold, `U` reverse and underline, `X` all
/// three.
pub fn attributes(buf: &Buffer) -> Vec<String> {
    let a = buf.area;
    (a.top()..a.bottom())
        .map(|y| {
            (a.left()..a.right())
                .map(|x| {
                    let m = buf[(x, y)].modifier;
                    match (
                        m.contains(Modifier::BOLD),
                        m.contains(Modifier::UNDERLINED),
                        m.contains(Modifier::REVERSED),
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

/// A screen as its snapshot reads it: the text, then the attributes each
/// cell keeps without color.
pub fn snapshot_text(mono: &Buffer) -> String {
    let mut out = rows_of(mono).join("\n");
    out.push_str("\n-- attributes without color --\n");
    out.push_str(&attributes(mono).join("\n"));
    out.push('\n');
    out
}

/// Compares `text` with `tests/snapshots/NAME.txt`, or writes the file
/// with `VTIUM_CONFIG_BLESS=1`.
pub fn snapshot(name: &str, text: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("{name}.txt"));
    if std::env::var_os("VTIUM_CONFIG_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("the snapshots' folder");
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
