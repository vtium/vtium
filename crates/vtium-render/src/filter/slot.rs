//! The CPU path's color slot: a target that looks each pixel it writes up
//! in the head's color table, then writes it on.
//!
//! The slot sits on the renderer's last write to the display's buffer,
//! after the pointer is blended over the frame and before a 1-bit
//! display's cut, so the border, the cells and the pointer all pass
//! through it. vtium's notices and the focus mark reach the buffer through
//! the same slot. Without a table the slot writes each run through as it
//! is.
//!
//! References:
//! - ADR-1303 D4: where the color slot sits on the CPU path.
//! - ADR-1304 D2: each pixel the CPU path writes is looked up in the table.

use super::Lookup;
use crate::target::Target;

/// A target that writes each run through `lookup`, when there is one, into
/// `target`. `line` holds the looked-up run.
pub(crate) struct Colored<'a, T: Target + ?Sized> {
    target: &'a mut T,
    lookup: Option<&'a mut Lookup>,
    line: &'a mut Vec<u32>,
}

impl<'a, T: Target + ?Sized> Colored<'a, T> {
    pub(crate) fn new(
        target: &'a mut T,
        lookup: Option<&'a mut Lookup>,
        line: &'a mut Vec<u32>,
    ) -> Self {
        Colored {
            target,
            lookup,
            line,
        }
    }
}

impl<T: Target + ?Sized> Target for Colored<'_, T> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        let Some(lookup) = self.lookup.as_deref_mut() else {
            return self.target.write(x, y, pixels);
        };
        self.line.clear();
        self.line.extend_from_slice(pixels);
        lookup.map_all(self.line);
        self.target.write(x, y, self.line);
    }
}
