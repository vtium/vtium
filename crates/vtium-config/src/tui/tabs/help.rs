//! The Help tab: the Help page, which says what each tab does and lists
//! its keys and the commands, and the About page. Left and Right switch
//! between the two pages, and the moving keys scroll the page.
//!
//! References:
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.

use super::super::input::Key;
use super::Command;

/// Where the Help tab stands: the page it shows, and the page's first row
/// in sight.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Help {
    about: bool,
    top: usize,
}

/// How long the page is, and how many of its rows the screen shows at
/// once, which update reads from the screen's layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    /// The page's rows.
    pub rows: usize,
    /// The rows the screen shows at once.
    pub room: usize,
}

impl Help {
    /// Whether the tab shows the About page.
    pub fn about(&self) -> bool {
        self.about
    }

    /// The page's first row in sight.
    pub fn top(&self) -> usize {
        self.top
    }

    /// The node of the interface tree the tab is at, which has no level
    /// that opens over it.
    pub fn node(&self) -> &'static str {
        "help"
    }

    /// A key the navigation module hands to the tab. The top row of the
    /// page moves by a line, or by the rows the screen shows, and stops
    /// where the page's last row is at the bottom of the screen.
    pub fn update(&mut self, key: Key, page: Page) -> Vec<Command> {
        match key {
            Key::Left | Key::Right => {
                self.about = !self.about;
                self.top = 0;
            }
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End => {
                let last = page.rows.saturating_sub(page.room);
                let step = page.room.max(1);
                let at = self.top.min(last);
                self.top = match key {
                    Key::Up => at.saturating_sub(1),
                    Key::Down => at.saturating_add(1),
                    Key::PageUp => at.saturating_sub(step),
                    Key::PageDown => at.saturating_add(step),
                    Key::Home => 0,
                    _ => last,
                }
                .min(last);
            }
            _ => {}
        }
        Vec::new()
    }
}
