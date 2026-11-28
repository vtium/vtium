//! What the Flow tab's tests share: the machines they draw, a screen drawn
//! whole in the frame the shell draws around the tab, the rows of a drawn
//! buffer as text, the checks every screen passes, and the snapshots.
//!
//! The screens put the tab where the rebuilt TUI puts it: the top row and
//! its rule, the tab in the viewer, the message on the viewer's last row,
//! and the action bar from the key table's rows for the tab's node. The
//! top row's facts are the open document's own.
//!
//! References:
//! - ADR-1206 D10: each screen at 100×30 and 80×25, with the screen map's
//!   fixture and a large one, readable without color.

#![allow(dead_code, reason = "each test binary uses a part of the helpers")]
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "a test fails by panicking"
)]

pub mod examples;

use std::path::PathBuf;

use ratatui_core::backend::TestBackend;
use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier};
use ratatui_core::terminal::Terminal;
use vtium_config_core::document::{Base, Content, Document, Documents, InMemory, Installed};
use vtium_config_core::flowmodel::Present;
use vtium_config_core::ui::{Action, Command, Shared};
use vtium_config_tab_flow::FlowTab;
use vtium_config_widgets::backend::console_width;
use vtium_config_widgets::canvas;
use vtium_config_widgets::frame::{
    Bar, Count, Facts, FrameLayout, Message, Options, TabName, TopRow,
};
use vtium_config_widgets::input::{Button, Key, Mouse};
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::tab::Tab;

/// The two sizes every screen is checked at: the one vtium-config is
/// designed for, and its minimum.
pub const SIZES: [(u16, u16); 2] = [(100, 30), (80, 25)];

/// The six tabs of the top row, in its order, with their Alt letters.
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

/// The shared state of a machine whose open configuration `name` holds
/// `text`, installed as it is, with the devices `present`.
pub fn machine(name: &str, text: &str, present: Present) -> Shared {
    let content = Content {
        text: text.to_owned(),
        base: Base::of(Some(text)),
        ..Content::default()
    };
    let desk = InMemory::new().with_draft(name, content.clone());
    let document = Document::new(
        name,
        content,
        Installed {
            text: Some(text.to_owned()),
            frits: Default::default(),
        },
    );
    Shared::new(Documents::new(document, Box::new(desk)), present)
}

/// Draws the screen of `tab` over `shared` at `cols`×`rows`: the frame,
/// with the top row, the message and the action bar, and the tab in the
/// viewer. In color where `color` holds.
pub fn screen(shared: &Shared, tab: &FlowTab, cols: u16, rows: u16, color: bool) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(cols, rows)).expect("a terminal");
    let document = shared.document();
    let name = document.name().to_owned();
    let top = TopRow {
        tabs: &TABS,
        open_tab: 3,
        open_menu: None,
        facts: Facts {
            name: &name,
            mode: None,
            not_saved: Count::opening(document.count()),
            problems: Count::default(),
        },
    };
    let bar = Bar::from_rows(tab.bar(shared));
    let message = shared.message.as_ref().map(|m| Message {
        text: m.line.clone(),
        role: Role::Row,
    });
    let options = Options {
        search_rows: 0,
        message: message.is_some(),
    };
    terminal
        .draw(|f| {
            let layout = FrameLayout::new(f.area(), options, &top, &bar);
            layout.draw(f.buffer_mut(), message.as_ref(), color);
            let viewer = layout.regions().viewer;
            tab.draw(shared, f, viewer);
        })
        .expect("a frame");
    terminal.backend().buffer().clone()
}

/// The viewer of a screen of `cols`×`rows` with no message and no search
/// open.
pub fn viewer(cols: u16, rows: u16) -> Rect {
    vtium_config_widgets::frame::Regions::of(Rect::new(0, 0, cols, rows), Options::default()).viewer
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

/// The screen's text, one row a line.
pub fn text_of(buf: &Buffer) -> String {
    rows_of(buf).join("\n")
}

/// Every row of the screen is the terminal's width and no wider, and the
/// screen has the terminal's rows.
pub fn check_rows(buf: &Buffer, cols: u16, rows: u16) {
    let text = rows_of(buf);
    assert_eq!(text.len(), usize::from(rows));
    for (y, row) in text.iter().enumerate() {
        assert_eq!(
            canvas::cells(row),
            usize::from(cols),
            "row {y} is {} cells: {row:?}",
            canvas::cells(row)
        );
    }
}

/// Drawn without color, the screen sets no color.
pub fn check_no_color(buf: &Buffer) {
    for cell in buf.content() {
        let style = cell.style();
        assert!(
            style.fg.is_none_or(|c| c == Color::Reset),
            "a color without color: {style:?} on {:?}",
            cell.symbol()
        );
        assert!(
            style.bg.is_none_or(|c| c == Color::Reset),
            "a background without color: {style:?} on {:?}",
            cell.symbol()
        );
    }
}

/// Whether any cell of the screen is drawn with `modifier`.
pub fn has_modifier(buf: &Buffer, modifier: Modifier) -> bool {
    buf.content()
        .iter()
        .any(|c| c.style().add_modifier.contains(modifier))
}

/// Compares the screen's text with `tests/snapshots/NAME.txt`, or writes
/// the file with `VTIUM_CONFIG_BLESS=1`.
pub fn snapshot(name: &str, buf: &Buffer) {
    let text = rows_of(buf).join("\n") + "\n";
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("{name}.txt"));
    if std::env::var_os("VTIUM_CONFIG_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("the snapshots' folder");
        std::fs::write(&path, &text).expect("the snapshot");
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        want == text,
        "{name} differs from {}; rerun with VTIUM_CONFIG_BLESS=1 to write it again\n{text}",
        path.display()
    );
}

/// Hands `tab` the action of the row `row`, after drawing it at
/// `cols`×`rows` so it knows its viewer, and returns the commands.
pub fn press(shared: &Shared, tab: &mut FlowTab, row: &'static str) -> Vec<Command> {
    tab.update(shared, Action::of(row))
}

/// A left click at the screen's cell `col`, `row`, on the row `on`.
pub fn click(
    shared: &Shared,
    tab: &mut FlowTab,
    on: &'static str,
    col: u16,
    row: u16,
) -> Vec<Command> {
    let m = Mouse {
        button: Button::Press(0),
        col: usize::from(col),
        row: usize::from(row),
    };
    tab.update(shared, Action::key(on, Key::Mouse(m)))
}

/// The screen cell of the first place on screen where `needle` is drawn.
pub fn find(buf: &Buffer, needle: &str) -> Option<(u16, u16)> {
    for (y, row) in rows_of(buf).iter().enumerate() {
        if let Some(at) = row.find(needle) {
            let x = canvas::cells(&row[..at]);
            return Some((u16::try_from(x).ok()?, u16::try_from(y).ok()?));
        }
    }
    None
}

/// Goes to the box Find lists as `name` in the section `section` of the
/// page, as Enter on Find's match does, and says whether Find listed it.
pub fn go_to(shared: &Shared, tab: &mut FlowTab, name: &str, section: &str) -> bool {
    let place = format!("Flow › {section}");
    let Some(item) = tab
        .find_items(shared)
        .into_iter()
        .find(|i| i.name == name && i.place == place)
    else {
        return false;
    };
    tab.update(shared, Action::item("enter.typing.find", item.index));
    true
}

/// The screen with the message the last commands put on the message line,
/// as the shell shows it, and those commands.
pub fn with_message(shared: &mut Shared, commands: &[Command]) {
    use vtium_config_core::ui::Change;
    for c in commands {
        if let Command::Change(Change::Say(m)) = c {
            shared.message = Some(m.clone());
        }
    }
}

/// The large machine with `n` inputs that are neither keyboards nor
/// pointers added, each a power button of its own.
pub fn large_with_others(n: usize) -> Shared {
    use vtium_config_core::fixture::large;
    let mut present = large::present();
    for k in 0..n {
        let mut d = vtium_settings::Identity::input().with_class(vtium_settings::Class::Key);
        d.name = Some(format!("Power Button {k:02}"));
        d.path = Some(format!("LNXPWRBN/button/input{k}"));
        present.devices.push(d);
    }
    machine(large::NAME, &large::installed(), present)
}
