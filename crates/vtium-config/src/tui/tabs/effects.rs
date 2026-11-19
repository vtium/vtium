//! The Effects tab, the second tab of the bar, which today says only that
//! it is coming. Tab, Shift-Tab, the digit 2 and a click on its name reach
//! it like any other tab, and Save and Install act there as in the other
//! tabs. The work that builds the tab lists the color maps and shaders
//! here.
//!
//! References:
//! - ADR-1205 D1: the six tabs and their order, the Effects tab among
//!   them.
//! - ADR-1206 D6, slice 5: the Effects tab is built as a module of the
//!   TUI's structure.

use super::super::input::Key;
use super::super::listbox::{Body, prose};
use super::super::model::{Shared, Size};
use super::super::words;
use super::{Command, Room, Tab};

/// The Effects tab's state, which holds nothing yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects;

impl Tab for Effects {
    fn name(&self) -> &'static str {
        words::EFFECTS
    }

    fn node(&self) -> &'static str {
        "effects"
    }

    /// The one line that says the tab is coming.
    fn body(&self, _shared: &Shared, size: Size) -> Body {
        Body {
            list: prose(words::EFFECTS_COMING, size.cols),
            keep: 0,
            tail: Vec::new(),
        }
    }

    /// The keys that act on the tab while it lists nothing.
    fn planned_keys(&self, _shared: &Shared) -> &'static str {
        words::KEYS_EFFECTS
    }

    /// Save and Install act here as in the other tabs, and `o` does
    /// nothing, since it opens Restore in the Flow view alone.
    ///
    /// References:
    /// - ADR-1403 D7, "The same keys in every view": the `o.view` row.
    fn takes_action(&self, action: char) -> bool {
        action != 'o'
    }

    fn update(&mut self, _shared: &Shared, _room: Room, _key: Key) -> Vec<Command> {
        Vec::new()
    }
}
