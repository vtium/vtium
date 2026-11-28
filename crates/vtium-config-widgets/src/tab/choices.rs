//! The opt-in trait for a line with choices, which opens the list box
//! under itself. A tab hands the list box the line's choices, the one the
//! line has now and the change that keeping a choice makes. The list box
//! draws them and knows nothing of fonts, looks or settings.
//!
//! References:
//! - ADR-1206 D8: the list box, its height, its scrolling, and the search
//!   it offers where the line has more choices than it shows.
//! - ADR-1206 D9: the choices trait and its methods with defaults.

use std::borrow::Cow;

/// The choices of one line, which the list box shows. `C` is the command
/// that keeping a choice returns, which the shell applies as one change.
pub trait Choices<C> {
    /// How many choices the line has.
    fn count(&self) -> usize;

    /// The text of the choice numbered `choice`, as the list box shows
    /// it and the search matches it.
    fn text(&self, choice: usize) -> Cow<'_, str>;

    /// The choice the line has now, which the list box marks and first
    /// highlights. `None` where the line holds a value none of the
    /// choices names.
    fn current(&self) -> Option<usize>;

    /// The change that keeping the choice numbered `choice` makes.
    fn keep(&self, choice: usize) -> C;

    /// How many choices the list box shows at once in `room` rows: every
    /// choice, unless the room less the box's top and bottom border holds
    /// fewer.
    fn shown(&self, room: u16) -> usize {
        self.count()
            .min(usize::from(room.saturating_sub(BORDER_ROWS)))
    }

    /// The list box's height in `room` rows, its borders included.
    fn height(&self, room: u16) -> u16 {
        u16::try_from(self.shown(room))
            .unwrap_or(u16::MAX)
            .saturating_add(BORDER_ROWS)
            .min(room)
    }

    /// Whether the line has more choices than the list box shows at once
    /// in `room` rows, so that the box scrolls and offers the `?` search.
    fn more_than_shown(&self, room: u16) -> bool {
        self.count() > self.shown(room)
    }
}

/// The rows of the list box's top and bottom border.
const BORDER_ROWS: u16 = 2;
