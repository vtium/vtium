//! What the search and list box tests share: the screen map's fonts and a
//! large list of fonts as a `Searchable`, the rows of a drawn buffer as
//! text, the checks every screen passes, and the snapshots.
//!
//! The screens these tests draw put the component where the rebuilt TUI
//! puts it: the search row above the action bar's rule, under the list it
//! filters, and the list box under its line or above it. The top row, the
//! list's own rows and the action bar are the tests' own stand-ins for
//! the frame and the tabs, which other cards build.
//!
//! References:
//! - ADR-1206 D10: each screen at 100×30 and 80×25, with the screen
//!   map's fixture and a large one, readable without color.

#![allow(dead_code, reason = "each test binary uses a part of the helpers")]

use std::borrow::Cow;
use std::cell::Cell;
use std::path::PathBuf;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::Modifier;
use vtium_config_widgets::backend::console_width;
use vtium_config_widgets::canvas;
use vtium_config_widgets::search::{self, Search};
use vtium_config_widgets::tab::Searchable;

/// A list of fonts: each font's family and its file, empty for a font
/// that is not installed. `reads` counts the calls of `text`.
pub struct Fonts {
    pub items: Vec<(&'static str, &'static str)>,
    pub reads: Cell<usize>,
}

impl Fonts {
    /// The Fonts tab's list in the screen map and the mock-up catalog.
    pub fn screen_map() -> Fonts {
        Fonts {
            items: vec![
                ("DejaVu Sans Mono", "DejaVuSansMono.ttf"),
                ("JetBrains Mono", "JetBrainsMono-Regular.ttf"),
                ("Monoid", "Monoid-Regular.ttf"),
                ("mononoki", "mononoki-Regular.ttf"),
                ("Noto Color Emoji", "NotoColorEmoji.ttf"),
                ("Terminus", "ter-u16n.otb"),
                ("Cullet Mono", "cullet-mono.otf"),
                ("Unifont", "unifont"),
                ("MyMono", "MyMono-Regular.ttf"),
                ("Symbols Nerd Font Mono", ""),
            ],
            reads: Cell::new(0),
        }
    }

    /// `n` fonts with names of four styles, as a large machine's fonts.
    pub fn large(n: usize) -> Fonts {
        const STYLES: [(&str, &str); 4] = [
            ("Mono", "mono"),
            ("Sans", "sans"),
            ("Serif", "serif"),
            ("Nerd Font Mono", "nerd-mono"),
        ];
        Fonts {
            items: (0..n)
                .map(|i| {
                    let (style, file) = STYLES[i % STYLES.len()];
                    let family: &'static str = format!("Family {i:05} {style}").leak();
                    let path: &'static str = format!("family-{i:05}-{file}.ttf").leak();
                    (family, path)
                })
                .collect(),
            reads: Cell::new(0),
        }
    }
}

impl Searchable for Fonts {
    fn search_node(&self) -> &'static str {
        "fonts"
    }
    fn count(&self) -> usize {
        self.items.len()
    }
    fn text(&self, item: usize, field: usize) -> Option<Cow<'_, str>> {
        self.reads.set(self.reads.get() + 1);
        let (family, file) = self.items.get(item)?;
        let text = match field {
            0 => family,
            1 => file,
            _ => return None,
        };
        (!text.is_empty()).then_some(Cow::Borrowed(*text))
    }
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

/// Drawn without color, the screen sets no color, and the highlighted row
/// still shows, in reverse video.
pub fn check_highlight_without_color(buf: &Buffer) {
    let mut reversed = false;
    for cell in buf.content() {
        let style = cell.style();
        assert!(
            style
                .fg
                .is_none_or(|c| c == ratatui_core::style::Color::Reset),
            "a color without color: {style:?}"
        );
        assert!(
            style
                .bg
                .is_none_or(|c| c == ratatui_core::style::Color::Reset),
            "a background without color: {style:?}"
        );
        reversed |= style.add_modifier.contains(Modifier::REVERSED);
    }
    assert!(reversed, "no highlighted row in reverse video");
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

/// Writes `text` at `x`, `y`, at most to the area's right edge.
pub fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, style: ratatui_core::style::Style) {
    let room = buf.area.right().saturating_sub(x);
    buf.set_stringn(x, y, text, usize::from(room), style);
}

/// A screen of the Fonts list under the search: a stand-in top row, the
/// fonts that match, the highlighted one in reverse video, then the
/// search row, the action bar's rule and a stand-in action bar.
pub fn search_screen(fonts: &Fonts, s: &Search, cols: u16, rows: u16, color: bool) -> Buffer {
    use vtium_config_widgets::roles::Role;
    let mut buf = Buffer::empty(Rect::new(0, 0, cols, rows));
    let plain = Role::Row.style(color).cell();
    let rule = "─".repeat(usize::from(cols));
    put(&mut buf, 0, 0, " Fonts", plain);
    let count = format!(
        "{} of {} fonts match ",
        s.matches().len(),
        fonts.items.len()
    );
    put(
        &mut buf,
        cols.saturating_sub(u16::try_from(canvas::cells(&count)).unwrap_or(0)),
        0,
        &count,
        plain,
    );
    put(&mut buf, 0, 1, &rule, plain);
    let list_rows = usize::from(rows.saturating_sub(5));
    let at = s
        .highlighted()
        .and_then(|h| s.matches().iter().position(|&m| m == h))
        .unwrap_or(0);
    let first = at.saturating_sub(list_rows.saturating_sub(1));
    for (y, &item) in s.matches().iter().skip(first).take(list_rows).enumerate() {
        let (family, file) = fonts.items[item];
        let line = format!(
            "   {:<26} {family}",
            if file.is_empty() { "-" } else { file }
        );
        let line = canvas::fit(&line, usize::from(cols));
        let style = if s.highlighted() == Some(item) {
            Role::RowHighlight.style(color).cell()
        } else {
            plain
        };
        let y = u16::try_from(y).unwrap_or(0).saturating_add(2);
        put(
            &mut buf,
            0,
            y,
            &format!("{line:<width$}", width = usize::from(cols)),
            style,
        );
    }
    search::row::draw(s, &mut buf, Rect::new(0, rows - 3, cols, 1), color);
    put(&mut buf, 0, rows - 2, &rule, plain);
    put(
        &mut buf,
        0,
        rows - 1,
        " [Enter] Keep  [↑↓] Move  [Esc] Clear",
        plain,
    );
    buf
}
