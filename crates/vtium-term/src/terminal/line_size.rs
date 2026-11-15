//! The line size sequences, which make the cursor's line single size or
//! double size. A double-size line draws each character twice as wide, so
//! it holds half as many columns as the screen. A double-height line is
//! drawn in two halves on two adjacent rows, and the program prints the
//! same text on both.
//!
//! - `ESC # 3` (DECDHL): the top half of a double-height, double-width
//!   line.
//! - `ESC # 4` (DECDHL): the bottom half of a double-height, double-width
//!   line.
//! - `ESC # 5` (DECSWL): a single-width, single-height line, the size
//!   every new line has.
//! - `ESC # 6` (DECDWL): a double-width, single-height line.
//!
//! The screen applies the size: what the line loses, where the cursor
//! goes, and why DECLRMM keeps every line single size (`Screen`'s
//! `set_line_size`). `ESC # 8`, DECALN, has its own handler.
//!
//! References:
//! - ADR-1000, `text.line-size`: vtium supports the four sequences.

use crate::LineSize;

use super::State;

impl State {
    /// `ESC # 3` to `ESC # 6`: the cursor's line takes the size that
    /// `final_byte` names. Any other final byte changes nothing.
    pub(super) fn line_size(&mut self, final_byte: u8) {
        let size = match final_byte {
            b'3' => LineSize::DoubleHeightTop,
            b'4' => LineSize::DoubleHeightBottom,
            b'5' => LineSize::Single,
            b'6' => LineSize::DoubleWidth,
            _ => return,
        };
        self.screen.set_line_size(size);
    }
}
