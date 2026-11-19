//! The Effects tab, which today says only that it is coming. The tab is
//! wired into update, the layout and the view like every other tab, but it
//! is not in the tab bar yet, so no key reaches it. The work that builds
//! the tab lists the filters, color maps and shaders here.
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

    fn update(&mut self, _shared: &Shared, _room: Room, _key: Key) -> Vec<Command> {
        Vec::new()
    }
}
