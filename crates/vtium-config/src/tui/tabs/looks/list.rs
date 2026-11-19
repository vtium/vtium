//! The Looks tab's list. Until looks are built, it lists the sizes the
//! console's font takes, automatic first, and Enter sets the highlighted
//! size as the draft's `font-size`.
//!
//! References:
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.

use super::super::super::input::Key;
use super::super::super::listbox::{Body, ListBox, columns, plain, prose, step, tail};
use super::super::super::model::{DpiFrom, SIZES, Shared, Size};
use super::super::super::shot::Preview;
use super::super::super::words;
use super::super::{Change, Command};

/// Where the list of sizes stands: the highlighted size, counted from 0
/// for automatic, and the window of the list on screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sizes {
    list: ListBox,
}

impl Sizes {
    /// The highlighted row, counted from 0 for automatic.
    pub fn at(&self) -> usize {
        self.list.at
    }

    /// The first row the window of the list shows.
    pub fn offset(&self) -> usize {
        self.list.offset
    }

    /// Keeps the list in view from its row `row`, which the layout gives
    /// after every event.
    pub fn show_from(&mut self, row: usize) {
        self.list.offset = row;
    }

    /// The highlighted size, in points, 0 for automatic.
    pub fn points(&self) -> u32 {
        match self.list.at {
            0 => 0,
            i => SIZES.get(i.saturating_sub(1)).copied().unwrap_or(0),
        }
    }

    /// The list's body on a screen of `size`: the sentence above the
    /// sizes, the sizes, automatic first, each marked where it is saved or
    /// to save; and under them, the line that describes `preview`, the
    /// tab's preview of the highlighted size.
    pub fn body(&self, shared: &Shared, size: Size, preview: Preview<'_>) -> Body {
        let cols = size.cols;
        let configured = shared
            .configured("font-size")
            .and_then(|s| s.parse::<u32>().ok());
        let items: Vec<(String, String)> = std::iter::once(0)
            .chain(SIZES)
            .map(|points| {
                let name = match points {
                    0 => words::AUTOMATIC.to_owned(),
                    n => words::points(n),
                };
                let drafted = shared
                    .drafted("font-size")
                    .and_then(|s| s.parse::<u32>().ok());
                let note = if shared.changed("font-size") && drafted.unwrap_or(0) == points {
                    words::TO_SAVE
                } else if configured.unwrap_or(0) == points {
                    words::SAVED
                } else {
                    ""
                };
                (name, note.to_owned())
            })
            .collect();
        let (list, keep) = columns(&items, self.list.at, 10, cols);
        let sentence = if shared.text_preview() || !shared.graphics {
            preview
                .preview_line()
                .unwrap_or_else(|| words::NO_PICTURES.to_owned())
        } else if self.points() == 0 {
            words::AUTOMATIC_PICTURE.to_owned()
        } else {
            let (dpi, from) = shared.inputs.dpi;
            let from = match from {
                DpiFrom::Conf => words::DPI_FROM_CONF,
                DpiFrom::Edid => words::DPI_FROM_EDID,
                DpiFrom::Assumed => words::DPI_ASSUMED,
            };
            match preview.preview_line() {
                Some(line) => format!("{} {line}", words::density(dpi, from)),
                None => words::density(dpi, from),
            }
        };
        let mut list_rows = prose(words::LOOKS_SIZES, cols);
        list_rows.push(plain(""));
        let keep = keep.saturating_add(list_rows.len());
        list_rows.extend(list);
        Body {
            list: list_rows,
            keep,
            tail: tail(&sentence, cols),
        }
    }

    /// A key the navigation module hands to the tab: the moving keys move
    /// the highlight, and Enter sets the highlighted size as the draft's
    /// `font-size`.
    pub fn update(&mut self, key: Key) -> Vec<Command> {
        self.list.at = step(self.list.at, SIZES.len().saturating_add(1), key);
        if key != Key::Enter {
            return Vec::new();
        }
        vec![Command::Change(Change::Top {
            key: "font-size",
            value: Some(self.points().to_string()),
        })]
    }
}
