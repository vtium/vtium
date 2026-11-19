//! The Looks tab. Until looks are built, it lists the sizes the console's
//! font takes, and its preview draws the sample screen in the font the
//! Fonts tab highlights, at the highlighted size.
//!
//! The tab is made of groups, each in a module of its own: the list, the
//! look card, the font sets, the new-font-set form, the pointer themes and
//! the color schemes. This module holds the tab's state and composes the
//! groups, with one call into each group for each part of the tab. Today
//! only the list holds anything, so each part calls the list alone, and
//! the work that fills another group adds its call here.
//!
//! References:
//! - ADR-1205: the Looks tab and its groups.
//! - ADR-1206 D2: a tab's state and its update live in the tab's module.

mod card;
mod fontsets;
mod list;
mod newfrit;
mod palettes;
mod pointers;

use super::super::input::Key;
use super::super::layout::{PICTURE_COL, PICTURE_ROW};
use super::super::listbox::{Body, prose};
use super::super::model::{Shared, Size};
use super::super::shot::{Preview, Source};
use super::super::words;
use super::{Command, Room, Tab};

/// Where the Looks tab stands: its list, and the font its preview shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Looks {
    /// The list of sizes.
    sizes: list::Sizes,
    /// The font the preview shows, counted from 0 in the list of fonts.
    /// Update hands the tab the font the Fonts tab highlights each time
    /// that highlight moves, until looks name fonts of their own.
    font: usize,
}

impl Looks {
    /// The tab at its start, previewing the font at `font` in the list of
    /// fonts.
    pub fn new(font: usize) -> Looks {
        Looks {
            sizes: list::Sizes::default(),
            font,
        }
    }

    /// The highlighted row of the list, counted from 0 for automatic.
    pub fn at(&self) -> usize {
        self.sizes.at()
    }

    /// The first row the window of the list shows.
    pub fn offset(&self) -> usize {
        self.sizes.offset()
    }

    /// Keeps the list in view from its row `row`, which the layout gives
    /// after every event.
    pub fn show_from(&mut self, row: usize) {
        self.sizes.show_from(row);
    }

    /// The highlighted size, in points, 0 for automatic.
    pub fn points(&self) -> u32 {
        self.sizes.points()
    }

    /// The font the preview shows, counted from 0 in the list of fonts.
    pub fn font(&self) -> usize {
        self.font
    }

    /// Previews the font at `at` in the list of fonts from now on.
    pub fn show_font(&mut self, at: usize) {
        self.font = at;
    }

    /// The tab's body on a screen of `size`: the list, with the line that
    /// describes the preview under it.
    pub fn body(&self, shared: &Shared, size: Size) -> Body {
        self.sizes
            .body(shared, size, Tab::preview(self, shared, size))
    }

    /// The node of the interface tree the tab is at, which has no level
    /// that opens over it.
    pub fn node(&self) -> &'static str {
        "looks"
    }

    /// A key the navigation module hands to the tab, which the list takes.
    pub fn update(&mut self, key: Key) -> Vec<Command> {
        self.sizes.update(key)
    }
}

impl Tab for Looks {
    fn name(&self) -> &'static str {
        words::LOOKS
    }

    fn node(&self) -> &'static str {
        Looks::node(self)
    }

    fn body(&self, shared: &Shared, size: Size) -> Body {
        Looks::body(self, shared, size)
    }

    fn offset(&self) -> usize {
        Looks::offset(self)
    }

    fn show_from(&mut self, row: usize) {
        Looks::show_from(self, row);
    }

    /// Beside the list of sizes, below the sentence above the sizes and
    /// its blank row.
    fn picture_at(&self, _shared: &Shared, size: Size) -> (usize, usize) {
        (
            PICTURE_ROW
                .saturating_add(prose(words::LOOKS_SIZES, size.cols).len())
                .saturating_add(1),
            PICTURE_COL,
        )
    }

    /// The tab's font at the highlighted size, on the sample screen.
    fn preview<'a>(&self, shared: &'a Shared, size: Size) -> Preview<'a> {
        Preview::of(
            shared,
            size,
            Source::Size {
                font: shared.inputs.fonts.get(self.font),
                points: self.points(),
            },
        )
    }

    /// The sample screen's lines, in their colors.
    fn console_lines(
        &self,
        _shared: &Shared,
    ) -> Vec<Vec<(&'static str, super::super::color::Style)>> {
        super::super::shot::sample_rows()
    }

    fn planned_keys(&self, _shared: &Shared) -> &'static str {
        words::KEYS_LOOKS
    }

    fn update(&mut self, _shared: &Shared, _room: Room, key: Key) -> Vec<Command> {
        Looks::update(self, key)
    }
}
