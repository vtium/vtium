//! The sheets the shell opens over a tab: F1's help ([`about`]), Ctrl+F's
//! Find ([`find`]), the Problems list ([`problems`]), File › New…
//! ([`new`]) and Manage configurations… ([`manage`]); [`open`] opens a
//! configuration from File › Open › or the Manage sheet.
//!
//! The navigation state keeps the open sheets as a stack, the top last,
//! so F1 on a problem opens its whole line over the Problems list and Esc
//! goes back to the list. A sheet answers each key with an [`Act`]: most
//! keys it handles itself, a key it leaves to the navigation module acts
//! as it does on any level, such as `q` or Ctrl+Z, and the rest are acts
//! that update performs in [`crate::app::update`], since only update
//! changes the shared state and the tabs.
//!
//! References:
//! - ADR-1206 D3: the navigation module owns F1's help and Find, and the
//!   state of what is open over a tab.
//! - ADR-1403 D11, "F1, help for what is highlighted", "Find, on Ctrl+F",
//!   "The frame" (the Problems list), "New…: what is saved, or vtium's
//!   defaults" and "Managing configurations".

pub mod about;
pub mod find;
pub mod manage;
pub mod new;
pub mod open;
pub mod problems;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};
use vtium_config_core::ui::Action;
use vtium_config_widgets::contract::{self, Row};
use vtium_config_widgets::frame::{Bar, Regions, SEARCH_ROWS_MAX};
use vtium_config_widgets::keys::Key;
use vtium_config_widgets::menu::sheet::{self, SheetLayout};
use vtium_config_widgets::roles::Role;
use vtium_config_widgets::search::Search;

pub use about::About;
pub use find::Find;
pub use manage::Manage;
pub use new::New;
pub use problems::Problems;

/// A sheet over the open tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sheet {
    About(About),
    Find(Find),
    Problems(Problems),
    New(New),
    Manage(Manage),
    /// Save's review, or its screens for a file changed on disk.
    Save(Box<crate::save::Save>),
}

/// What a key or a click on a sheet asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// The key is not the sheet's: it acts by its row, as on any level.
    Pass,
    /// The sheet moved or typed, and stays open.
    Stay,
    /// The sheet closes, and the one under it, if any, is open again.
    Close,
    /// Goes to the tab at `tab`: every sheet, the search row and the
    /// levels open over that tab close, the tab opens, and it is handed
    /// `action`, which names the item to highlight.
    Go { tab: usize, action: Option<Action> },
    /// Goes to the tab of the node `tab`, as a problem's place names it.
    GoNode {
        tab: &'static str,
        action: Option<Action>,
    },
    /// Asks `question`; a yes hands `then` to whoever acts on its row.
    Ask { question: String, then: Action },
    /// The shell acts on this row, as File › Show changes for a line of
    /// vtium.conf.
    Shell(Action),
    /// Goes to the Fonts tab and asks whether to activate the `n` fonts
    /// the configuration uses that are not active.
    Activate(usize),
    /// Opens the configuration of this name, keeping the one open.
    Open(String),
    /// Renames the configuration `from` to `to`.
    Rename { from: String, to: String },
    /// Makes the configuration `name` from `start` and opens it.
    New { name: String, start: new::Start },
}

impl Sheet {
    /// The sheet's node of the interface tree, whose rows act on it. New…
    /// has no node of its own in the tree, so its keys are its own.
    pub fn node(&self) -> &'static str {
        match self {
            Sheet::About(_) => "any.about",
            Sheet::Find(_) => "any.find",
            Sheet::Problems(_) => "any.problems",
            Sheet::New(_) => new::NODE,
            Sheet::Manage(m) => m.node(),
            Sheet::Save(s) => s.node(),
        }
    }

    /// The action bar while the sheet is open: its node's buttons, or the
    /// sheet's own where its node has none.
    pub fn bar(&self) -> Bar {
        match self {
            Sheet::New(n) => n.bar(),
            Sheet::Save(s) => s.bar(),
            _ => Bar::from_rows(contract::buttons(self.node())),
        }
    }

    /// Acts on `key`, which chose `row` of the sheet's node.
    pub fn key(&mut self, key: &Key, row: Option<&'static Row>) -> Act {
        let id = row.map_or("", |r| r.id);
        match self {
            Sheet::About(a) => a.key(key, id),
            Sheet::Find(f) => f.key(key),
            Sheet::Problems(p) => p.key(key, id),
            Sheet::New(n) => n.key(key),
            Sheet::Manage(m) => m.key(key, id),
            Sheet::Save(s) => s.key(key, id),
        }
    }

    /// Acts on a click at `at`, the sheet placed as `placed`.
    pub fn click(&mut self, at: Position, placed: &Placed) -> Act {
        if placed.frame.as_ref().is_some_and(|f| f.hits_close(at)) {
            return Act::Close;
        }
        let Some(line) = placed.line_at(at) else {
            return Act::Stay;
        };
        let rows = usize::from(placed.inner.height);
        match self {
            Sheet::Find(f) => f.click(line),
            Sheet::Problems(p) => p.click(line, rows),
            Sheet::Manage(m) => m.click(line, rows),
            Sheet::About(_) | Sheet::New(_) | Sheet::Save(_) => Act::Stay,
        }
    }

    /// The sheet's name, as F1's list of its keys names the screen.
    pub fn title(&self) -> &str {
        match self {
            Sheet::About(a) => &a.title,
            Sheet::Find(_) => "Find",
            Sheet::Problems(_) => problems::TITLE,
            Sheet::New(_) => new::TITLE,
            Sheet::Manage(_) => manage::TITLE,
            Sheet::Save(_) => crate::save::TITLE,
        }
    }

    /// The full text behind the sheet's highlighted line, which F1 opens
    /// over it, where the sheet cut the line.
    pub fn full_text(&self) -> Option<(String, String)> {
        match self {
            Sheet::Problems(p) => p.full_text(),
            _ => None,
        }
    }

    /// Find's search row, while the sheet is Find.
    pub fn search(&self) -> Option<&Search> {
        match self {
            Sheet::Find(f) => Some(f.search()),
            _ => None,
        }
    }

    /// The rows the sheet takes over the action bar: Find's matches and
    /// its search row, and none for a sheet over the viewer.
    pub fn search_rows(&self) -> u16 {
        match self {
            Sheet::Find(_) => SEARCH_ROWS_MAX,
            _ => 0,
        }
    }

    /// Where the sheet goes on a screen of `regions`.
    pub fn place(&self, regions: &Regions) -> Placed {
        let viewer = regions.viewer;
        match self {
            Sheet::Find(_) => Placed::rows(regions.search.unwrap_or_default()),
            Sheet::About(a) => Placed::sheet(a.area(viewer), &a.title),
            Sheet::Problems(p) => Placed::sheet(p.area(viewer), problems::TITLE),
            Sheet::New(n) => Placed::sheet(n.area(viewer), new::TITLE),
            Sheet::Manage(m) => Placed::sheet(m.area(viewer), manage::TITLE),
            Sheet::Save(s) => Placed::sheet(s.area(viewer), crate::save::TITLE),
        }
    }

    /// Draws the sheet into `buf` where `placed` says.
    pub fn draw(&self, buf: &mut Buffer, placed: &Placed, color: bool) {
        if let Some(frame) = &placed.frame {
            frame.draw(buf, color);
        }
        match self {
            Sheet::Find(f) => f.draw(buf, placed.inner, color),
            Sheet::About(a) => a.draw(buf, placed.inner, color),
            Sheet::Problems(p) => p.draw(buf, placed.inner, color),
            Sheet::New(n) => n.draw(buf, placed.inner, color),
            Sheet::Manage(m) => m.draw(buf, placed.inner, color),
            Sheet::Save(s) => s.draw(buf, placed.inner, color),
        }
    }
}

/// Where a sheet is drawn: its frame, where it is a box over the viewer,
/// and the rows its lines take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub frame: Option<SheetLayout>,
    pub inner: Rect,
}

impl Placed {
    /// A box over the viewer in `area`, titled `title`, with a close mark.
    fn sheet(area: Rect, title: &str) -> Placed {
        let frame = sheet::layout(area, title, true);
        let inner = frame.inner();
        Placed {
            frame: Some(frame),
            inner,
        }
    }

    /// Rows with no box, as Find's above the action bar.
    fn rows(area: Rect) -> Placed {
        Placed {
            frame: None,
            inner: area,
        }
    }

    /// The line of the sheet's rows a click at `at` lands on, counted from
    /// the first row inside it.
    fn line_at(&self, at: Position) -> Option<usize> {
        self.inner
            .contains(at)
            .then(|| usize::from(at.y.saturating_sub(self.inner.y)))
    }
}

/// A box of `lines` lines and its borders, as wide as `width` allows,
/// at the top of `viewer` and never larger than it.
fn top_box(viewer: Rect, width: u16, lines: usize) -> Rect {
    let height = u16::try_from(lines)
        .unwrap_or(u16::MAX)
        .saturating_add(2)
        .min(viewer.height);
    let w = width.min(viewer.width);
    Rect::new(
        viewer.x.saturating_add(viewer.width.saturating_sub(w) / 2),
        viewer.y,
        w,
        height,
    )
}

/// The first of `len` lines a window of `rows` shows so that `at` stays
/// in view.
fn window(len: usize, rows: usize, at: Option<usize>) -> usize {
    match at {
        Some(at) if rows > 0 && at >= rows => (at + 1 - rows).min(len.saturating_sub(rows)),
        _ => 0,
    }
}

/// Writes `text` at the start of row `y` of `area`, cut to the area's
/// width, in `role`'s style, and returns the column after it.
pub(crate) fn put(
    buf: &mut Buffer,
    area: Rect,
    x: u16,
    y: u16,
    text: &str,
    role: Role,
    color: bool,
) -> u16 {
    if y >= area.height || x >= area.width {
        return x;
    }
    let width = usize::from(area.width.saturating_sub(x));
    let (end, _) = buf.set_stringn(
        area.x.saturating_add(x),
        area.y.saturating_add(y),
        text,
        width,
        role.style(color).cell(),
    );
    end.saturating_sub(area.x)
}

/// Fills row `y` of `area` with `role`'s style, as a highlighted row is a
/// bar across the whole width.
fn bar_row(buf: &mut Buffer, area: Rect, y: u16, role: Role, color: bool) {
    if y < area.height {
        buf.set_style(
            Rect::new(area.x, area.y.saturating_add(y), area.width, 1),
            role.style(color).cell(),
        );
    }
}

/// `text` cut to `width` characters, ending with `…` where it was cut.
pub(crate) fn cut(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_keeps_the_highlight_in_view() {
        assert_eq!(window(20, 5, None), 0);
        assert_eq!(window(20, 5, Some(4)), 0);
        assert_eq!(window(20, 5, Some(5)), 1);
        assert_eq!(window(20, 5, Some(19)), 15);
        assert_eq!(window(3, 5, Some(2)), 0);
    }

    #[test]
    fn a_long_line_is_cut_with_an_ellipsis() {
        assert_eq!(cut("abc", 3), "abc");
        assert_eq!(cut("abcdef", 4), "abc…");
    }
}
