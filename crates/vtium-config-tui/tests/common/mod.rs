//! What the shell's tests drive the App with: two small stand-in tabs, the
//! screen map's machine and the large machine from vtium-config-core's
//! fixtures, each over the desk kept in memory, and a stand-in terminal
//! that plays wakes and keeps what the loop writes.
//!
//! The stand-in tabs act as a tab crate's tab does, through the `Tab`
//! trait alone: Fonts returns a change to the document on Space, asks to
//! open its detail on Enter, offers the `?` search over its fonts, and
//! tries a font live on `t`; Looks asks a question on `n` and adds an item
//! to the Actions menu. Each keeps a log of every action it was handed,
//! so a test sees what reached the tab.

#![allow(dead_code)]
#![expect(clippy::expect_used, reason = "a test fails by panicking")]

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use ratatui_core::backend::TestBackend;
use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::terminal::{Frame, Terminal};
use vtium_config_core::document::{self, Edit};
use vtium_config_core::fixture;
use vtium_config_core::ui::{Action, Carries, Change, Command, Effect, Message, Request, Shared};
use vtium_config_tui::BoxTab;
use vtium_config_tui::app::{App, Event, Size};
use vtium_config_tui::port::{Port, Wake};
use vtium_config_widgets::input;
use vtium_config_widgets::keys::{Button, Code, Key, Mouse};
use vtium_config_widgets::tab::{MenuItem, Searchable, Tab};

/// Every action a stand-in tab was handed, in order.
pub type Log = Rc<RefCell<Vec<Action>>>;

/// The machine a test runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine {
    /// The screen map's machine: the configuration `workstation`, with one
    /// change not saved.
    Map,
    /// The large machine: ten cards of four displays, sixty inputs, twelve
    /// configurations and thirty versions.
    Large,
}

impl Machine {
    pub const BOTH: [Machine; 2] = [Machine::Map, Machine::Large];

    pub fn shared(self) -> Shared {
        match self {
            Machine::Map => fixture::padnoir::shared(),
            Machine::Large => fixture::large::shared(),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Machine::Map => "map",
            Machine::Large => "large",
        }
    }
}

/// The two sizes every screen is checked at: the one vtium-config is
/// designed for, and the minimum.
pub const SIZES: [(u16, u16); 2] = [(100, 30), (80, 25)];

/// The stand-in Fonts tab.
pub struct Fonts {
    pub log: Log,
    highlight: usize,
    marks: usize,
}

/// The fonts the stand-in Fonts tab lists: each family and its file.
pub const FONTS: [(&str, &str); 4] = [
    ("Terminus", "ter-u16n.psf.gz"),
    ("Cullet Mono", "CulletMono-Regular.otf"),
    ("Noto Sans Mono", "NotoSansMono-Regular.ttf"),
    ("Unifont", "unifont.otf"),
];

impl Tab<Shared, Action, Command> for Fonts {
    fn name(&self) -> &'static str {
        "Fonts"
    }

    fn letter(&self) -> char {
        'n'
    }

    fn node(&self) -> &'static str {
        "fonts"
    }

    fn draw(&self, _shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        let buf = frame.buffer_mut();
        for (i, (family, file)) in FONTS.iter().enumerate() {
            let Some(y) = area.y.checked_add(u16::try_from(i).expect("a row")) else {
                break;
            };
            if y >= area.bottom() {
                break;
            }
            let mark = if i == self.highlight { "▸" } else { " " };
            buf.set_stringn(
                area.x,
                y,
                format!("{mark} {family:<18} {file}"),
                usize::from(area.width),
                ratatui_core::style::Style::default(),
            );
        }
    }

    fn update(&mut self, shared: &Shared, action: Action) -> Vec<Command> {
        self.log.borrow_mut().push(action.clone());
        match action.row {
            "down.view" => {
                self.highlight = (self.highlight + 1).min(FONTS.len() - 1);
                Vec::new()
            }
            "enter.view.fonts" => vec![Command::Request(Request::Open("fonts.font"))],
            "esc.layer" => Vec::new(),
            "space.view.resources" => {
                self.marks += 1;
                let text = format!("{}# mark {}\n", shared.document().text(), self.marks);
                vec![Command::Change(Change::Document(document::Change::new(
                    format!("mark {}", self.marks),
                    Edit::Text(text),
                )))]
            }
            "t.view.fonts" => vec![Command::Change(Change::Try("Terminus".to_owned()))],
            "n.view.fonts" => {
                let installed = shared.document().installed().text.clone();
                vec![Command::Change(Change::Document(document::Change::new(
                    "the installed text",
                    Edit::Text(installed.unwrap_or_default()),
                )))]
            }
            _ => Vec::new(),
        }
    }

    fn searchable(&self) -> Option<&dyn Searchable> {
        Some(self)
    }
}

impl Searchable for Fonts {
    fn search_node(&self) -> &'static str {
        "fonts"
    }

    fn count(&self) -> usize {
        FONTS.len()
    }

    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>> {
        let (family, file) = FONTS.get(item)?;
        match field {
            0 => Some(Cow::Borrowed(family)),
            1 => Some(Cow::Borrowed(file)),
            _ => None,
        }
    }
}

/// The stand-in Looks tab.
pub struct Looks {
    pub log: Log,
}

impl Tab<Shared, Action, Command> for Looks {
    fn name(&self) -> &'static str {
        "Looks"
    }

    fn letter(&self) -> char {
        'l'
    }

    fn node(&self) -> &'static str {
        "looks"
    }

    fn draw(&self, _shared: &Shared, frame: &mut Frame<'_>, area: Rect) {
        frame.buffer_mut().set_stringn(
            area.x,
            area.y,
            "  big        font set proof · 11 pt",
            usize::from(area.width),
            ratatui_core::style::Style::default(),
        );
    }

    fn update(&mut self, _shared: &Shared, action: Action) -> Vec<Command> {
        self.log.borrow_mut().push(action.clone());
        match (action.row, &action.carries) {
            ("n.view.looks", Carries::Item(1)) => vec![Command::Change(Change::Say(Message {
                line: "Made the look new-1.".to_owned(),
                more: None,
            }))],
            ("n.view.looks", _) => vec![Command::Request(Request::Ask {
                question: "Make a new look?".to_owned(),
                then: Action::item("n.view.looks", 1),
            })],
            ("enter.view.looks", _) => vec![Command::Request(Request::Open("looks.look"))],
            _ => Vec::new(),
        }
    }

    fn menu_items(&self, _shared: &Shared) -> Vec<MenuItem<Action>> {
        vec![MenuItem {
            label: "Picture of the console".to_owned(),
            letter: Some('p'),
            shortcut: Some("p"),
            action: Action::of("p.view.looks"),
            greyed: None,
        }]
    }
}

/// The App on `machine` at `cols` by `rows`, with the two stand-in tabs,
/// and the log each keeps.
pub fn app(machine: Machine, size: (u16, u16)) -> (App, Log, Log) {
    app_in(machine, size, true)
}

/// The same App, drawn without color, as under `NO_COLOR`.
pub fn app_mono(machine: Machine, size: (u16, u16)) -> (App, Log, Log) {
    app_in(machine, size, false)
}

fn app_in(machine: Machine, (cols, rows): (u16, u16), color: bool) -> (App, Log, Log) {
    let fonts_log = Log::default();
    let looks_log = Log::default();
    let tabs: Vec<BoxTab> = vec![
        Box::new(Fonts {
            log: Rc::clone(&fonts_log),
            highlight: 0,
            marks: 0,
        }),
        Box::new(Looks {
            log: Rc::clone(&looks_log),
        }),
    ];
    let app = App::new(tabs, machine.shared(), Size::grid(cols, rows), color);
    (app, fonts_log, looks_log)
}

/// Presses `key`.
pub fn press(app: &mut App, key: Key) -> Vec<Effect> {
    app.event(Event::Key(key))
}

/// Presses each key named in the key table's form, such as `Alt-F`.
pub fn keys(app: &mut App, names: &[&str]) -> Vec<Effect> {
    names
        .iter()
        .flat_map(|n| press(app, Key::named(n).expect("a key")))
        .collect()
}

/// Clicks the left button at column `x`, row `y`.
pub fn click(app: &mut App, x: u16, y: u16) -> Vec<Effect> {
    app.event(Event::Mouse(Mouse {
        button: Button::Press(0),
        col: usize::from(x),
        row: usize::from(y),
    }))
}

/// The key `c` with Alt.
pub fn alt(c: char) -> Key {
    Key::alt(c)
}

/// The key `code` with no modifier.
pub fn plain(code: Code) -> Key {
    Key::plain(code)
}

/// The screen `app` draws, as ratatui's buffer.
pub fn draw(app: &App) -> Buffer {
    let size = app.size();
    let mut terminal = Terminal::new(TestBackend::new(size.cols, size.rows)).expect("a terminal");
    terminal
        .draw(|f| vtium_config_tui::view::draw(app, f))
        .expect("a frame");
    terminal.backend().buffer().clone()
}

/// Each row of `buf` as text, one symbol for each cell, so a wide
/// character's second cell adds nothing.
pub fn rows(buf: &Buffer) -> Vec<String> {
    let a = buf.area;
    (a.y..a.bottom())
        .map(|y| {
            (a.x..a.right())
                .map(|x| buf.cell((x, y)).map_or(" ", |c| c.symbol()))
                .collect()
        })
        .collect()
}

/// The cells `s` takes, as ratatui measures it.
pub fn unicode_width(s: &str) -> usize {
    ratatui_core::text::Span::raw(s).width()
}

/// A grid of the attributes each cell of `buf` keeps without color: `.`
/// none, `b` bold, `u` underline, `r` reverse, `B` bold and underline,
/// `R` bold and reverse, `U` underline and reverse, `X` all three.
pub fn attributes(buf: &Buffer) -> Vec<String> {
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

/// Whether the cell at `x`, `y` is part of a run of cells of one color,
/// on its row, that holds a letter or a digit.
fn in_a_worded_run(buf: &Buffer, x: u16, y: u16) -> bool {
    let fg = |x: u16| buf.cell((x, y)).map(|c| c.fg);
    let here = fg(x);
    let word = |x: u16| {
        buf.cell((x, y))
            .is_some_and(|c| c.symbol().chars().any(char::is_alphanumeric))
    };
    let mut left = x;
    while left > buf.area.x && fg(left - 1) == here {
        left -= 1;
    }
    let mut right = x;
    while right + 1 < buf.area.right() && fg(right + 1) == here {
        right += 1;
    }
    (left..=right).any(word)
}

/// A screen's snapshot: its text, then the grid of its attributes without
/// color.
pub fn snapshot_text(buf: &Buffer) -> String {
    let mut out = rows(buf).join("\n");
    out.push_str("\n-- attributes without color --\n");
    out.push_str(&attributes(buf).join("\n"));
    out.push('\n');
    out
}

/// Checks that every row of `buf` takes exactly its width in cells.
pub fn rows_fit(buf: &Buffer, what: &str) {
    for (y, row) in rows(buf).iter().enumerate() {
        assert_eq!(
            unicode_width(row),
            usize::from(buf.area.width),
            "{what}: row {y} is not the screen's width: {row:?}"
        );
    }
}

/// Checks that `mono`, the screen drawn without color, has the text of
/// `color`, the same screen in color, and no color, and that every cell
/// that `color` colors has an attribute or belongs to a run of that color
/// holding a word, so no mark, such as a lone ● or ✗, is carried by color
/// alone.
pub fn marks_survive_without_color(color: &Buffer, mono: &Buffer, what: &str) {
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
            if c.fg != ratatui_core::style::Color::Reset {
                let attribute = !m.modifier.is_empty();
                let blank = c.symbol() == " " || c.symbol().is_empty();
                assert!(
                    attribute || blank || in_a_worded_run(color, x, y),
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

/// The screen's text, as one string with a row on each line.
pub fn text(app: &App) -> String {
    rows(&draw(app)).join("\n")
}

/// The column of the first cell of `needle` on row `y` of `app`'s screen.
pub fn find(app: &App, y: u16, needle: &str) -> u16 {
    let row = rows(&draw(app))
        .into_iter()
        .nth(usize::from(y))
        .expect("the row");
    let at = row.find(needle).expect("the text on the row");
    u16::try_from(unicode_width(row.get(..at).expect("a prefix"))).expect("a column")
}

/// The row a test finds `needle` on, and its column.
pub fn locate(app: &App, needle: &str) -> (u16, u16) {
    let rows = rows(&draw(app));
    for (y, row) in rows.iter().enumerate() {
        if let Some(at) = row.find(needle) {
            let x = unicode_width(row.get(..at).expect("a prefix"));
            return (
                u16::try_from(x).expect("a column"),
                u16::try_from(y).expect("a row"),
            );
        }
    }
    panic!("{needle:?} is not on the screen:\n{}", rows.join("\n"));
}

/// The old key type that an action carries, for a mouse press at the
/// viewer's cell `col`, `row`.
pub fn viewer_click(col: usize, row: usize) -> Carries {
    Carries::Key(input::Key::Mouse(input::Mouse {
        button: input::Button::Press(0),
        col,
        row,
    }))
}

/// A stand-in terminal: it plays `wakes` one batch per wait, then hangs
/// up, and keeps what the loop writes and how often it stepped aside.
pub struct Script {
    pub size: Size,
    pub wakes: VecDeque<Vec<Wake>>,
    pub written: Vec<u8>,
    pub asides: usize,
}

impl Script {
    pub fn new(size: Size, wakes: Vec<Vec<Wake>>) -> Script {
        Script {
            size,
            wakes: wakes.into(),
            written: Vec::new(),
            asides: 0,
        }
    }
}

impl Port for Script {
    fn size(&mut self) -> Size {
        self.size
    }

    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.written.extend_from_slice(bytes);
        Ok(())
    }

    fn arm(&mut self, _after: Duration) {}

    fn wait(&mut self) -> std::io::Result<Vec<Wake>> {
        Ok(self
            .wakes
            .pop_front()
            .unwrap_or_else(|| vec![Wake::Signal(1)]))
    }

    fn step_aside(
        &mut self,
        aside: &mut dyn FnMut() -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.asides += 1;
        aside()
    }
}

/// The snapshot `name` under tests/snapshots/, compared with `text`, or
/// written from it under `VTIUM_CONFIG_BLESS`.
pub fn snapshot(name: &str, text: &str) {
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

/// Where `needle` first starts in `bytes`.
pub fn at(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes.windows(needle.len()).position(|w| w == needle)
}

/// Where `needle` last starts in `bytes`.
pub fn last_at(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes.windows(needle.len()).rposition(|w| w == needle)
}

/// A pty of the test's own, 100×30: the TUI's port runs on its terminal
/// side, and the test reads and writes its other side.
pub struct Pty {
    master: std::os::fd::OwnedFd,
    path: std::path::PathBuf,
}

impl Pty {
    pub fn open() -> Pty {
        use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
        let master =
            openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC).expect("a pty");
        grantpt(&master).expect("grantpt");
        unlockpt(&master).expect("unlockpt");
        let name = ptsname(&master, Vec::new()).expect("the pty's name");
        let path = std::path::PathBuf::from(name.to_string_lossy().into_owned());
        let size = rustix::termios::Winsize {
            ws_row: 30,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        rustix::termios::tcsetwinsize(&master, size).expect("the pty's size");
        Pty { master, path }
    }

    fn terminal(&self) -> std::fs::File {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.path)
            .expect("the pty's terminal side")
    }

    /// The TUI's port on the pty's terminal side.
    pub fn tty(&self) -> vtium_config_tui::port::tty::Tty {
        let t = self.terminal();
        let input = std::os::fd::OwnedFd::from(t.try_clone().expect("a second handle"));
        vtium_config_tui::port::tty::Tty::on(input, t.into()).expect("the port")
    }

    /// A writer on the pty's terminal side, as a program the TUI steps
    /// aside for writes.
    pub fn writer(&self) -> std::fs::File {
        self.terminal()
    }

    /// Types `bytes` as keys.
    pub fn type_keys(&self, bytes: &[u8]) {
        rustix::io::write(&self.master, bytes).expect("the keys");
    }

    /// Reads everything the terminal side writes, until every handle on it
    /// is closed.
    pub fn reader(&self) -> std::thread::JoinHandle<Vec<u8>> {
        let master = self.master.try_clone().expect("a second handle");
        std::thread::spawn(move || {
            let mut out = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                match rustix::io::read(&master, &mut buf) {
                    Ok(0) | Err(rustix::io::Errno::IO) => break,
                    Ok(n) => out.extend_from_slice(buf.get(..n).expect("the bytes read")),
                    Err(rustix::io::Errno::INTR) => {}
                    Err(e) => panic!("the pty's read: {e}"),
                }
            }
            out
        })
    }
}
