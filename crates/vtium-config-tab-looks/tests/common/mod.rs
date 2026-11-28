//! What the Looks tab's tests share: the actions that drive the tab, a
//! screen of the whole frame with the tab drawn in its viewer, the rows
//! and attributes of a drawn screen, the checks every screen passes, and
//! the snapshots.
//!
//! The frame is vtium-config-widgets' own, with the Looks tab open and
//! the action bar of the tab's node, so a screen reads as the rebuilt TUI
//! draws it. The search row is drawn where the frame puts it, above the
//! action bar, from the tab's search.
//!
//! The tab is driven to an item through Find's request to highlight it,
//! so each test names the item and not its place, which differs between
//! the two machines.
//!
//! References:
//! - ADR-1206 D10: each screen at 100×30 and 80×25, with the screen
//!   map's fixture and a large one, readable without color.

#![allow(dead_code, reason = "each test binary uses a part of the helpers")]

use std::path::PathBuf;

use ratatui_core::backend::TestBackend;
use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier};
use ratatui_core::terminal::Terminal;
use vtium_config_core::fixture;
use vtium_config_core::ui::{Action, Change, Command, Shared};
use vtium_config_tab_looks::Looks;
use vtium_config_tab_looks::list::{self, Item};
use vtium_config_widgets::backend::console_width;
use vtium_config_widgets::canvas;
use vtium_config_widgets::frame::{Bar, Count, Facts, FrameLayout, Options, TabName, TopRow};
use vtium_config_widgets::input::{Button, Key, Mouse};
use vtium_config_widgets::search;
use vtium_config_widgets::tab::Tab;

/// A machine's name in the snapshots, and what builds its shared state.
pub type Machine = (&'static str, fn() -> Shared);

/// The two machines every screen is drawn on.
pub fn machines() -> [Machine; 2] {
    [
        ("map", fixture::padnoir::shared),
        ("large", fixture::large::shared),
    ]
}

/// The two sizes every screen is drawn at.
pub const SIZES: [(u16, u16); 2] = [(100, 30), (80, 25)];

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

/// The action of the row `row`, carrying nothing.
pub fn act(tab: &mut Looks, shared: &Shared, row: &'static str) -> Vec<Command> {
    tab.update(shared, Action::of(row))
}

/// `n` actions of the row `row`.
pub fn repeat(tab: &mut Looks, shared: &Shared, row: &'static str, n: usize) {
    for _ in 0..n {
        let made = act(tab, shared, row);
        assert!(made.is_empty(), "{row} made {made:?}");
    }
}

/// The characters of `text` typed through the row `row`, each carrying
/// its key.
pub fn type_in(tab: &mut Looks, shared: &Shared, row: &'static str, text: &str) {
    for c in text.chars() {
        let made = tab.update(shared, Action::key(row, Key::Char(c)));
        assert!(made.is_empty(), "typing {c:?} made {made:?}");
    }
}

/// A left click at `x`, `y`, through the row `row`.
pub fn click(tab: &mut Looks, shared: &Shared, row: &'static str, x: u16, y: u16) -> Vec<Command> {
    let mouse = Mouse {
        button: Button::Press(0),
        col: usize::from(x),
        row: usize::from(y),
    };
    tab.update(shared, Action::key(row, Key::Mouse(mouse)))
}

/// The screen of `w`×`h` cells: the frame on the Looks tab, its action
/// bar from the tab's node, and the tab drawn in the viewer, with the
/// search row above the bar while the tab has a search.
pub fn screen(tab: &Looks, shared: &Shared, w: u16, h: u16, color: bool) -> Buffer {
    let area = Rect::new(0, 0, w, h);
    let document = shared.document();
    let top = TopRow {
        tabs: &TABS,
        open_tab: 2,
        open_menu: None,
        facts: Facts {
            name: document.name(),
            mode: None,
            not_saved: Count::opening(document.count()),
            problems: Count::opening(0),
        },
    };
    let bar = Bar::from_rows(tab.bar());
    let options = Options {
        search_rows: u16::from(tab.search().is_some()),
        message: false,
    };
    let frame = FrameLayout::new(area, options, &top, &bar);
    let Ok(mut terminal) = Terminal::new(TestBackend::new(w, h));
    let drawn = terminal.draw(|f| {
        frame.draw(f.buffer_mut(), None, color);
        tab.draw(shared, f, frame.regions().viewer);
        if let (Some(s), Some(r)) = (tab.search(), frame.regions().search) {
            search::row::draw(s, f.buffer_mut(), r, color);
        }
    });
    assert!(drawn.is_ok(), "the screen did not draw");
    terminal.backend().buffer().clone()
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
                let symbol = buf.cell((x, y)).map_or(" ", |c| c.symbol());
                row.push_str(symbol);
                x = x.saturating_add(console_width(symbol).max(1));
            }
            row
        })
        .collect()
}

/// The attributes of each cell: `.` none, `b` bold, `u` underline, `r`
/// reverse video, and a capital for each pair or more: `B` bold and
/// underline, `R` reverse and bold, `U` reverse and underline, `X` all
/// three.
pub fn attributes(buf: &Buffer) -> Vec<String> {
    let a = buf.area;
    (a.y..a.bottom())
        .map(|y| {
            (a.x..a.right())
                .map(|x| {
                    let m = buf.cell((x, y)).map_or(Modifier::empty(), |c| c.modifier);
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

/// Every row of the screen is the terminal's width and no wider, and the
/// screen has the terminal's rows.
pub fn check_rows(buf: &Buffer, w: u16, h: u16) {
    let text = rows_of(buf);
    assert_eq!(text.len(), usize::from(h));
    for (y, row) in text.iter().enumerate() {
        assert_eq!(
            canvas::cells(row),
            usize::from(w),
            "row {y} is {} cells: {row:?}",
            canvas::cells(row)
        );
    }
}

/// Drawn without color, the screen sets no color, shows the same text as
/// with color, and keeps the highlight in reverse video where `lit` says
/// the screen has one.
pub fn check_without_color(with: &Buffer, without: &Buffer, lit: bool) {
    assert_eq!(
        rows_of(with),
        rows_of(without),
        "the text differs without color"
    );
    let mut reversed = false;
    for cell in without.content() {
        let style = cell.style();
        assert!(
            style.fg.is_none_or(|c| c == Color::Reset),
            "a color without color: {style:?}"
        );
        assert!(
            style.bg.is_none_or(|c| c == Color::Reset),
            "a background without color: {style:?}"
        );
        reversed |= style.add_modifier.contains(Modifier::REVERSED);
    }
    assert!(reversed || !lit, "no highlight in reverse video");
}

/// The screen's text, then the grid of what each cell keeps without color.
pub fn snapshot_text(buf: &Buffer) -> String {
    let mut out = rows_of(buf).join("\n");
    out.push_str("\n-- attributes without color --\n");
    out.push_str(&attributes(buf).join("\n"));
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
        let written = std::fs::write(&path, text);
        assert!(written.is_ok(), "cannot write {}", path.display());
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        want == text,
        "{name} differs from {}; rerun with VTIUM_CONFIG_BLESS=1 to write it again\n{text}",
        path.display()
    );
}

/// Highlights the first item `pick` keeps, through Find's request, and
/// returns its place.
pub fn go_to(tab: &mut Looks, shared: &Shared, pick: impl Fn(&Item) -> bool) -> usize {
    let at = list::items(shared)
        .iter()
        .position(pick)
        .unwrap_or_default();
    let made = tab.update(shared, Action::item("enter.typing.find", at));
    assert!(made.is_empty(), "going to an item made {made:?}");
    at
}

/// The one change `commands` hold for the document, which a kept pick
/// and every other change of the tab make.
pub fn one_change(commands: &[Command]) -> &vtium_config_core::document::Change {
    let changes: Vec<_> = commands
        .iter()
        .filter_map(|c| match c {
            Command::Change(Change::Document(d)) => Some(d),
            _ => None,
        })
        .collect();
    assert_eq!(changes.len(), 1, "not one change: {commands:?}");
    changes[0]
}

/// Makes each change `commands` hold on `shared`, as the shell's one
/// change function does.
pub fn apply(shared: &mut Shared, commands: &[Command]) {
    for c in commands {
        if let Command::Change(Change::Document(d)) = c {
            let done = shared.change(d.clone());
            assert!(done.is_ok(), "the document refused {d:?}: {done:?}");
        }
    }
}
