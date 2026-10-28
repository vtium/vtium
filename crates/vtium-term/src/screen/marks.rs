//! OSC 133's prompt marks (ADR-1002, sections 5 and 6): the line flags
//! each mark sets, the fresh line A, N and L start, and the prompt jumps.
//!
//! The marks are ADR-1001's three line flags, so they travel with their
//! line into the scrollback and leave with it, and a rewrap gives a
//! rejoined line the marks of all its rows (`reflow.rs`). ED 2, ED 3 and
//! RIS clear the marks of the lines they erase; EL and the partial erases
//! keep them, since shells redraw a prompt with them (`edit.rs`).

use super::Screen;
use crate::line::{Line, LineFlags};

impl Screen {
    /// OSC 133, the string after `133;`: a mark's letter, then its options
    /// after a `;`, which are ignored.
    ///
    /// | Mark | What it does |
    /// |---|---|
    /// | A, N | A fresh line, then the cursor's line takes the prompt flag |
    /// | L | A fresh line |
    /// | B, I | The cursor's line takes the input flag |
    /// | C | The cursor's line takes the output flag |
    /// | P, D | Nothing: continuation prompts are no jump targets, and the next prompt ends a command's output |
    pub(crate) fn shell_mark(&mut self, args: &[u8]) {
        let (kind, rest) = match args.split_first() {
            Some((&kind, rest)) => (kind, rest),
            None => return,
        };
        if !(rest.is_empty() || rest.starts_with(b";")) {
            return;
        }
        let flag = match kind {
            b'A' | b'N' => {
                self.fresh_line();
                LineFlags::PROMPT
            }
            b'L' => {
                self.fresh_line();
                return;
            }
            b'B' | b'I' => LineFlags::INPUT,
            b'C' => LineFlags::OUTPUT,
            _ => return,
        };
        let y = self.cursor_y;
        if let Some(line) = self.line_mut(y) {
            line.flags_mut().insert(flag);
        }
    }

    /// A carriage return and a line feed when the cursor is not in the
    /// first column, a pending wrap included, and nothing when it is, as
    /// the semantic prompts proposal defines a fresh line.
    fn fresh_line(&mut self) {
        if self.cursor_x != 0 {
            self.move_line_home();
            self.move_down(1, true);
        }
    }

    /// Moves the view to the nearest prompt above its top row (`up`) or
    /// below it, which becomes the view's top row; down past the newest
    /// prompt in the scrollback returns the view to the screen. On the
    /// alternate screen, which has no scrollback of prompts, nothing
    /// moves. Returns whether the view moved; a move marks everything, as
    /// any move of the view does.
    pub fn jump_to_prompt(&mut self, up: bool) -> bool {
        if self.flags.alternate {
            return false;
        }
        let is_prompt = |line: &Line| line.flags().contains(LineFlags::PROMPT);
        let view = self.scrollback.view_index();
        let top = view.unwrap_or(self.scrollback.len());
        match (up, view) {
            (true, _) => match self.scrollback.find_above(top, is_prompt) {
                Some(i) => self.scrollback.set_view(Some(i)),
                None => return false,
            },
            (false, None) => return false,
            (false, Some(_)) => {
                let below = self.scrollback.find_below(top, is_prompt);
                self.scrollback.set_view(below);
            }
        }
        self.damage.mark_all();
        self.refresh_hover();
        true
    }
}
