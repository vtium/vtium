//! What the Fonts tab's tests share: the machines and sizes, the actions
//! that drive the tab, a document that uses a font in each of the five
//! states, a screen of the whole frame with the tab drawn in its viewer,
//! the rows and attributes of a drawn screen, the checks every screen
//! passes, and the snapshots.
//!
//! The frame is vtium-config-widgets' own, with the Fonts tab open and the
//! tab's own action bar, so a screen reads as the rebuilt TUI draws it.
//! The search row is drawn where the frame puts it, above the action bar,
//! from the tab's search, and the message line from the shared state's.
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
use vtium_config_core::batch::Change as Batch;
use vtium_config_core::document::{Change, Edit, FontSetEdit};
use vtium_config_core::fixture;
use vtium_config_core::ui::{Action, Command, Shared};
use vtium_config_tab_fonts::Fonts;
use vtium_config_widgets::backend::console_width;
use vtium_config_widgets::canvas;
use vtium_config_widgets::frame::{Count, Facts, FrameLayout, Message, Options, TabName, TopRow};
use vtium_config_widgets::input::{Button, Key, Mouse};
use vtium_config_widgets::roles::Role;
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
pub fn act(tab: &mut Fonts, shared: &Shared, row: &'static str) -> Vec<Command> {
    tab.update(shared, Action::of(row))
}

/// `n` actions of the row `row`, none of which may make a command.
pub fn repeat(tab: &mut Fonts, shared: &Shared, row: &'static str, n: usize) {
    for _ in 0..n {
        let made = act(tab, shared, row);
        assert!(made.is_empty(), "{row} made {made:?}");
    }
}

/// The characters of `text` typed through the row `row`, each carrying
/// its key.
pub fn type_in(tab: &mut Fonts, shared: &Shared, row: &'static str, text: &str) {
    for c in text.chars() {
        let made = tab.update(shared, Action::key(row, Key::Char(c)));
        assert!(made.is_empty(), "typing {c:?} made {made:?}");
    }
}

/// A left click at `x`, `y`, through the row `row`.
pub fn click(tab: &mut Fonts, shared: &Shared, row: &'static str, x: u16, y: u16) -> Vec<Command> {
    let mouse = Mouse {
        button: Button::Press(0),
        col: usize::from(x),
        row: usize::from(y),
    };
    tab.update(shared, Action::key(row, Key::Mouse(mouse)))
}

/// Makes `commands`' changes to `shared`, as the shell's one place does:
/// a message goes on the message line, and a change to the document
/// through the one change function.
pub fn apply(shared: &mut Shared, commands: Vec<Command>) {
    use vtium_config_core::ui::Change as Ui;
    for c in commands {
        match c {
            Command::Change(Ui::Say(m)) => shared.message = Some(m),
            Command::Change(Ui::Document(change)) => {
                let made = shared.change(change);
                assert!(made.is_ok(), "the document refused the change: {made:?}");
            }
            Command::Change(Ui::Undo) => shared.undo(),
            Command::Change(Ui::Redo) => shared.redo(),
            _ => {}
        }
    }
}

/// The paths of the fonts that the five states' document uses, on the
/// screen map's machine: one each turned off on purpose, not active,
/// failing the trust rule and of your own, and a family no file is
/// installed for.
pub const MAP_SET: &str = "source=font:JetBrains Mono\nsource=font:Terminus\nsource=font:Noto Color Emoji\nsource=font:Mine\nsource=font:DejaVu Sans Mono\n";

/// DejaVu Sans Mono's file on the screen map's machine, which the batch
/// file activates.
pub const DEJAVU: &str = "/usr/share/fonts/TTF/DejaVuSansMono.ttf";

/// The same five states on the large machine: Family 0004, which the
/// batch file activates and the document turns off; Family 0005, which
/// nothing activates; Local-0013, which fails the trust rule; Mine-0002,
/// in the person's own folder; and a family no file is installed for.
pub const LARGE_SET: &str = "source=font:Nowhere Mono\nsource=font:Family 0005\nsource=font:Family 0013\nsource=font:Family 0002\nsource=font:Family 0004\n";

/// Family 0004's file on the large machine.
pub const LARGE_OFF: &str = "/usr/share/fonts/family-000/Family0004-Regular.ttf";

/// Changes the open document so its font set `proof` uses a font in each
/// of the five states, one of them deactivated by the document itself.
pub fn every_state(shared: &mut Shared) {
    let large = shared.document().name() == fixture::large::NAME;
    let (set, off) = if large {
        (LARGE_SET, LARGE_OFF)
    } else {
        (MAP_SET, DEJAVU)
    };
    let change = Change::new(
        "every state",
        Edit::All(vec![
            Edit::FontSet {
                frit: "proof".to_owned(),
                edit: FontSetEdit::Keep(set.to_owned()),
            },
            Edit::Active {
                path: PathBuf::from(off),
                change: Some(Batch::Deactivate),
            },
        ]),
    );
    let made = shared.change(change);
    assert!(made.is_ok(), "{made:?}");
    shared.message = None;
}

/// The screen of `w`×`h` cells: the frame on the Fonts tab, its action
/// bar from the tab, and the tab drawn in the viewer, with the search row
/// above the bar while the tab has a search, and the message line where
/// the shared state holds one. The bar shows Undo and Redo only while
/// there is a step, as the shell's does.
pub fn screen(tab: &Fonts, shared: &Shared, w: u16, h: u16, color: bool) -> Buffer {
    let area = Rect::new(0, 0, w, h);
    let document = shared.document();
    let top = TopRow {
        tabs: &TABS,
        open_tab: 0,
        open_menu: None,
        facts: Facts {
            name: document.name(),
            mode: None,
            not_saved: Count::opening(document.count()),
            problems: Count::opening(0),
        },
    };
    let history = shared.documents.history();
    let (undo, redo) = (history.to_undo().is_some(), history.to_redo().is_some());
    let mut bar = tab.action_bar(shared);
    bar.buttons.retain(|b| {
        let id = b.action.unwrap_or("");
        !(id.starts_with("ctrl-z.") && !undo || id.starts_with("ctrl-y.") && !redo)
    });
    let options = Options {
        search_rows: u16::from(tab.search().is_some()),
        message: shared.message.is_some(),
    };
    let frame = FrameLayout::new(area, options, &top, &bar);
    let message = shared.message.as_ref().map(|m| Message {
        text: m.line.clone(),
        role: Role::Row,
    });
    let Ok(mut terminal) = Terminal::new(TestBackend::new(w, h));
    let drawn = terminal.draw(|f| {
        tab.draw(shared, f, frame.regions().viewer);
        frame.draw(f.buffer_mut(), message.as_ref(), color);
        if let (Some(s), Some(r)) = (tab.search(), frame.regions().search) {
            search::row::draw(s, f.buffer_mut(), r, color);
        }
    });
    assert!(drawn.is_ok(), "the screen did not draw");
    terminal.backend().buffer().clone()
}

/// The viewer of a `w`×`h` screen with nothing over the bar.
pub fn viewer(shared: &Shared, tab: &Fonts, w: u16, h: u16) -> Rect {
    let area = Rect::new(0, 0, w, h);
    let document = shared.document();
    let top = TopRow {
        tabs: &TABS,
        open_tab: 0,
        open_menu: None,
        facts: Facts {
            name: document.name(),
            mode: None,
            not_saved: Count::opening(document.count()),
            problems: Count::opening(0),
        },
    };
    let bar = tab.action_bar(shared);
    let options = Options {
        search_rows: u16::from(tab.search().is_some()),
        message: shared.message.is_some(),
    };
    FrameLayout::new(area, options, &top, &bar).regions().viewer
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
/// with color, and keeps a highlight in reverse video where `lit` says
/// the screen highlights a row.
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
    if lit {
        assert!(reversed, "no highlight in reverse video");
    }
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
