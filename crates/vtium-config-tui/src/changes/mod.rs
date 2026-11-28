//! Show changes: the document's vtium.conf against the installed file, as
//! hunks with their line numbers and a little context, removed lines red
//! and added lines green, the words that changed inside a line marked, and
//! the font set changes and the active fonts after the hunks. A checked
//! pair at the top right, Changes only and Whole file, switches to the
//! whole file, which marks each changed line and each line the check
//! complains about; `w` or a click on the pair toggles it. The view
//! scrolls with the file view's keys. `e` opens Edit as text, and Ctrl+S
//! Save's review, through their rows.
//!
//! File › Show changes, a click on the count not saved, and `v` in the
//! Flow tab open it, as a sheet over the whole viewer. The same screen,
//! read-only, shows a version from History against the open document
//! ([`Against::Version`], which [`crate::history::view`] opens).
//!
//! The screen keeps its lines as they were last read from the shared
//! state, since a sheet draws without it; update reads them again after
//! every change to the document ([`refresh`]), so Ctrl+Z here shows one
//! change fewer at once. The hunks and the changed words come from
//! vtium-config-core's [`vtium_config_core::versions::diff`], the one
//! caller of the diff crate. [`draw`] lays the lines out and draws them.
//!
//! References:
//! - ADR-1403 D11, "Show changes and Edit as text": the hunks, the colors
//!   and the changed words, the font set changes and the active fonts, the
//!   checked pair, the complaints marked in the whole file, the file
//!   view's keys and the action bar.
//! - ADR-1403 D11, "One flow for every change": a click on the count not
//!   saved opens Show changes, and History's View changes shows a version
//!   in its form.
//! - ADR-1403 D11, "Undo and redo": Ctrl+Z in Show changes leaves it open,
//!   and its diff shows one change fewer.

pub mod draw;

use std::cell::Cell;

use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Position, Rect};
use vtium_config_core::document::Version;
use vtium_config_core::ui::Shared;
use vtium_config_core::versions::diff::{diff, whole_file};
use vtium_config_widgets::contract;
use vtium_config_widgets::frame::Bar;
use vtium_config_widgets::keys::{Code, Key, Mods};

use crate::sheets::{Act, Sheet};
use draw::Shown;

/// The installed file Show changes compares the document against.
pub const INSTALLED: &str = "/etc/vtium/vtium.conf";

/// The row that opens Show changes: `v` in the Flow tab, which File ›
/// Show changes and a click on the count not saved also take. Carrying a
/// line of vtium.conf, it opens the whole file at that line.
pub const OPEN: &str = "v.view.flow";

/// What the screen compares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Against {
    /// The document against the installed file: Show changes.
    Saved,
    /// A version from History against the document, read-only: its red
    /// lines leave the document and its green ones come with the version.
    Version(Version),
}

/// Show changes' state: what it compares, the form it shows, where it is
/// scrolled to, and its lines as last read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changes {
    against: Against,
    whole: bool,
    top: usize,
    /// The document's name, which the whole file's title names.
    name: String,
    /// The lines of Changes only and of Whole file, under the title.
    changes: Vec<Shown>,
    file: Vec<Shown>,
    /// The rows of lines the last frame showed, which paging moves by.
    seen: Cell<usize>,
}

impl Changes {
    /// Show changes of the open document, in Changes only.
    pub fn of_document(shared: &Shared) -> Changes {
        Changes::new(Against::Saved, shared)
    }

    /// `version` against the open document, read-only, in Changes only.
    pub fn of_version(shared: &Shared, version: Version) -> Changes {
        Changes::new(Against::Version(version), shared)
    }

    fn new(against: Against, shared: &Shared) -> Changes {
        let mut changes = Changes {
            against,
            whole: false,
            top: 0,
            name: String::new(),
            changes: Vec::new(),
            file: Vec::new(),
            seen: Cell::new(0),
        };
        changes.refresh(shared);
        changes
    }

    /// The same screen in Whole file, scrolled so that line `line` of the
    /// document's vtium.conf is near the top, as a problem on that line
    /// opens it.
    pub fn at_line(mut self, line: usize) -> Changes {
        self.whole = true;
        self.top = self
            .file
            .iter()
            .position(|s| s.line == Some(line))
            .map_or(0, |at| at.saturating_sub(2));
        self
    }

    /// Reads the lines again from `shared`, after a change to the
    /// document.
    pub fn refresh(&mut self, shared: &Shared) {
        let document = shared.document();
        self.name = document.name().to_owned();
        let (old, new) = match &self.against {
            Against::Saved => (
                document.installed().text.as_deref().unwrap_or_default(),
                document.text(),
            ),
            Against::Version(v) => (document.text(), v.text.as_str()),
        };
        let shown = diff(old, new);
        self.changes = draw::hunks(&shown);
        match &self.against {
            Against::Saved => {
                let review = document.review();
                if self.changes.is_empty()
                    && review.font_sets.is_empty()
                    && review.active.is_empty()
                {
                    self.changes = vec![Shown::note(words::NOTHING)];
                } else {
                    // with no hunk above them, the fonts start on the first line
                    let skip = usize::from(self.changes.is_empty());
                    let fonts = draw::fonts(&review, draw::width_of(&shown));
                    self.changes.extend(fonts.into_iter().skip(skip));
                }
            }
            Against::Version(_) if self.changes.is_empty() => {
                self.changes = vec![Shown::note(words::SAME_VERSION)];
            }
            Against::Version(_) => {}
        }
        let complaints = match self.against {
            Against::Saved => complaints(shared),
            Against::Version(_) => Vec::new(),
        };
        self.file = draw::file(&whole_file(old, new), &complaints);
    }

    /// What the screen compares.
    pub fn against(&self) -> &Against {
        &self.against
    }

    /// Whether it shows the whole file rather than the changes only.
    pub fn whole(&self) -> bool {
        self.whole
    }

    /// The first of its lines it shows.
    pub fn top(&self) -> usize {
        self.top
    }

    /// The lines of the form it shows, under the title.
    pub fn lines(&self) -> &[Shown] {
        if self.whole {
            &self.file
        } else {
            &self.changes
        }
    }

    /// The node of the interface tree it is.
    pub fn node(&self) -> &'static str {
        match self.against {
            Against::Saved => "any.changes",
            Against::Version(_) => "any.history.view",
        }
    }

    /// The title and the grey words after it, on the screen's first row.
    pub fn title(&self) -> (String, &'static str) {
        match (&self.against, self.whole) {
            (Against::Saved, false) => (words::CHANGES.to_owned(), words::AGAINST),
            (Against::Saved, true) => (words::file(&self.name), words::AS_SAVED),
            (Against::Version(v), _) => (
                words::version(&crate::menus::file::when(v.when)),
                words::READ_ONLY,
            ),
        }
    }

    /// The action bar: Whole file or Changes only, then Edit as text, Back
    /// and Save, with Quit at the right. The read-only view offers only
    /// the toggle and Back.
    pub fn bar(&self) -> Bar {
        let ids: &[&str] = match self.against {
            Against::Saved => &[
                "w.layer.changes",
                "e.layer.changes",
                "esc.layer",
                "ctrl-s.layer.changes",
                "q.layer",
            ],
            Against::Version(_) => &[
                "w.layer.history-view",
                "esc.layer.history-view",
                "ctrl-c.any",
            ],
        };
        let mut bar = Bar::from_rows(ids.iter().filter_map(|id| contract::row(id)));
        if self.whole
            && let Some(toggle) = bar.buttons.first_mut()
        {
            words::CHANGES_ONLY.clone_into(&mut toggle.label);
        }
        bar
    }

    /// Acts on `key`, which chose the row `id`: the toggle, scrolling and
    /// Esc are the screen's, and every other key acts by its row.
    pub fn key(&mut self, key: &Key, id: &str) -> Act {
        match id {
            "w.layer.changes" | "w.layer.history-view" => {
                self.show_whole(!self.whole);
                Act::Stay
            }
            "scroll.layer.changes" | "scroll.layer.history-view" => {
                self.scroll(key);
                Act::Stay
            }
            "esc.layer" | "esc.layer.history-view" => Act::Close,
            _ => Act::Pass,
        }
    }

    /// A click at `at` on the screen drawn in `area`: on the checked pair,
    /// it picks the form clicked.
    pub fn click(&mut self, at: Position, area: Rect) -> Act {
        if at.y == area.y {
            let (title, grey) = self.title();
            let pair = draw::header(area.width, &title, grey).pair;
            let x = at.x.saturating_sub(area.x);
            if (pair..pair.saturating_add(draw::CHANGES_ONLY_CELLS)).contains(&x) {
                self.show_whole(false);
            } else if (pair.saturating_add(draw::WHOLE_FILE_AT)
                ..pair.saturating_add(draw::PAIR_CELLS))
                .contains(&x)
            {
                self.show_whole(true);
            }
        }
        Act::Stay
    }

    fn show_whole(&mut self, whole: bool) {
        if whole != self.whole {
            self.whole = whole;
            self.top = 0;
        }
    }

    /// Up and Down, `k` and `j`, move one line; PgUp and PgDn a screen;
    /// Ctrl-U and Ctrl-D half a screen; Home and `g` go to the first line,
    /// End and `G` to the last screen.
    fn scroll(&mut self, key: &Key) {
        let rows = self.seen.get().max(1);
        let last = self.lines().len().saturating_sub(rows);
        let top = self.top.min(last);
        let ctrl = key.mods.contains(Mods::CTRL);
        self.top = match key.code {
            Code::Char('u') if ctrl => top.saturating_sub(rows / 2),
            Code::Char('d') if ctrl => top.saturating_add(rows / 2),
            Code::Up | Code::Char('k') => top.saturating_sub(1),
            Code::Down | Code::Char('j') => top.saturating_add(1),
            Code::PageUp => top.saturating_sub(rows),
            Code::PageDown => top.saturating_add(rows),
            Code::Home | Code::Char('g') => 0,
            Code::End | Code::Char('G') => last,
            _ => top,
        }
        .min(last);
    }

    /// Draws the screen into `area`, the whole viewer.
    pub fn draw(&self, buf: &mut Buffer, area: Rect, color: bool) {
        let (title, grey) = self.title();
        let shown = draw::draw(
            buf,
            area,
            &draw::Page {
                title: &title,
                grey,
                whole: self.whole,
                lines: self.lines(),
                top: self.top,
            },
            color,
        );
        self.seen.set(shown);
    }
}

/// The lines of vtium.conf the check complains about, as the Problems
/// list places them.
fn complaints(shared: &Shared) -> Vec<usize> {
    crate::sheets::problems::gather(shared)
        .into_iter()
        .filter_map(|p| match p.goes {
            crate::sheets::problems::Goes::Line(n) => Some(n),
            _ => None,
        })
        .collect()
}

/// Reads every Show changes in `sheets` again from `shared`, after a
/// change to the document.
pub fn refresh(sheets: &mut [Sheet], shared: &Shared) {
    for sheet in sheets {
        if let Sheet::Changes(c) = sheet {
            c.refresh(shared);
        }
    }
}

/// The words Show changes puts on the screen.
pub mod words {
    /// The title of Changes only.
    pub const CHANGES: &str = "Changes not saved";

    /// The grey words after it.
    pub const AGAINST: &str = "against /etc/vtium/vtium.conf";

    /// The grey words after the whole file's title.
    pub const AS_SAVED: &str = "as it will be saved";

    /// The grey words after a version's title.
    pub const READ_ONLY: &str = "against the open document, read-only";

    /// The toggle's label while the whole file shows.
    pub const CHANGES_ONLY: &str = "Changes only";

    /// Changes only, with nothing changed.
    pub const NOTHING: &str =
        "Nothing is changed: the configuration is what /etc/vtium/vtium.conf holds.";

    /// A version with the open document's text.
    pub const SAME_VERSION: &str = "This version holds the text the open document holds.";

    /// The whole file's title.
    pub fn file(name: &str) -> String {
        format!("{name}'s vtium.conf")
    }

    /// A version's title, by when it was replaced.
    pub fn version(when: &str) -> String {
        format!("Version {when}")
    }
}
