//! The sheet of a version chosen from File › History: when the version was
//! replaced, and what its two buttons do. It counts nothing and asks
//! nothing. `[v] View changes` opens the version against the open
//! document, read-only, and `[a] Use this version` loads it as one change,
//! which Ctrl+Z takes back. Both are buttons on the action bar as well as
//! keys, and a click on either line acts as its key. Esc closes the sheet.
//!
//! References:
//! - ADR-1403 D11, "One flow for every change": the sheet's two buttons,
//!   no counts, and Esc.
//! - ADR-1403 D11, "The action bar": a sheet's buttons are on the bar.

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use vtium_config_core::document::Version;
use vtium_config_core::ui::Action;
use vtium_config_widgets::contract;
use vtium_config_widgets::frame::Bar;
use vtium_config_widgets::keys::Key;
use vtium_config_widgets::roles::Role;

use crate::sheets::Act;

/// The node of the sheet.
pub const NODE: &str = "any.history.version";

/// The widest the sheet's box grows.
const WIDTH: u16 = 86;

/// The sheet's state: the version chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sheet {
    version: Version,
}

impl Sheet {
    /// The sheet of `version`.
    pub fn new(version: Version) -> Sheet {
        Sheet { version }
    }

    /// The version chosen.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// The sheet's title, by when the version was replaced.
    pub fn title(&self) -> String {
        words::title(&crate::menus::file::when(self.version.when))
    }

    /// The action bar: View changes, Use this version and Close.
    pub fn bar(&self) -> Bar {
        Bar::from_rows(
            [super::VIEW, super::USE, "esc.layer.history-version"]
                .iter()
                .filter_map(|id| contract::row(id)),
        )
    }

    /// Acts on `key`, which chose the row `id`: Esc closes the sheet, and
    /// `v` and `a` act by their rows, which the shell takes.
    pub fn key(&mut self, _key: &Key, id: &str) -> Act {
        match id {
            "esc.layer.history-version" => Act::Close,
            _ => Act::Pass,
        }
    }

    /// A click on the sheet's line `line`: the line of a button acts as
    /// its key.
    pub fn click(&self, line: usize) -> Act {
        match line {
            BUTTONS => Act::Shell(Action::of(super::VIEW)),
            l if l == BUTTONS + 1 => Act::Shell(Action::of(super::USE)),
            _ => Act::Stay,
        }
    }

    /// The sheet's lines: when the version was replaced, a blank line,
    /// then each button and what it does.
    fn lines(&self) -> [(String, Option<&'static str>); 4] {
        let when = crate::menus::file::when(self.version.when);
        [
            (words::replaced(&self.version.draft, &when), None),
            (String::new(), None),
            (words::VIEW.to_owned(), Some("[v]")),
            (words::USE.to_owned(), Some("[a]")),
        ]
    }

    /// The sheet's box in `viewer`: as wide as its lines need up to 86
    /// cells, two rows down from the viewer's top.
    pub fn area(&self, viewer: Rect) -> Rect {
        let lines = self.lines();
        let height = u16::try_from(lines.len())
            .unwrap_or(u16::MAX)
            .saturating_add(2)
            .min(viewer.height);
        let w = WIDTH.min(viewer.width.saturating_sub(4));
        let down = 2.min(viewer.height.saturating_sub(height));
        Rect::new(
            viewer.x.saturating_add(viewer.width.saturating_sub(w) / 2),
            viewer.y.saturating_add(down),
            w,
            height,
        )
    }

    /// Draws the sheet's lines into `inner`, a button's key bold.
    pub fn draw(&self, buf: &mut Buffer, inner: Rect, color: bool) {
        for (y, (text, key)) in self.lines().iter().enumerate() {
            let Ok(y) = u16::try_from(y) else { break };
            if y >= inner.height {
                break;
            }
            let row = inner.y.saturating_add(y);
            let width = usize::from(inner.width);
            let x = match key {
                Some(key) => {
                    let (x, _) =
                        buf.set_stringn(inner.x, row, key, width, Role::BarKey.style(color).cell());
                    x.saturating_add(1)
                }
                None => inner.x,
            };
            let left = width.saturating_sub(usize::from(x.saturating_sub(inner.x)));
            buf.set_stringn(x, row, text, left, Role::Row.style(color).cell());
        }
    }
}

/// The line of the first button among the sheet's lines.
const BUTTONS: usize = 2;

/// The words of the version's sheet.
pub mod words {
    /// View changes' line, after its key.
    pub const VIEW: &str =
        "View changes       what it would change in the open document, read-only";

    /// Use this version's line, after its key.
    pub const USE: &str = "Use this version   loads it as one change; [Ctrl+Z] takes it back";

    /// The sheet's title.
    pub fn title(when: &str) -> String {
        format!("The version of {when}")
    }

    /// The sheet's first line.
    pub fn replaced(draft: &str, when: &str) -> String {
        format!("Installed until {when}, when a save of {draft} replaced it.")
    }
}
