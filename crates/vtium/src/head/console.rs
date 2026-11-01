//! A head's consoles (ADR-1401 D1): each a session with its own pty,
//! screen, scrollback and grid, one shown at a time on every display of
//! the head. The consoles keep the order they were opened in, a ring the
//! console chords walk, and none opens or closes while the head runs.

use vtium_platform::pty::Pty;

use crate::respawn::Respawn;
use crate::term::Term;

/// One console: its session's terminal and pty.
pub struct Console {
    /// Never reused within the head: the pty's watch finds its console by
    /// it.
    pub(crate) id: u64,
    pub(crate) term: Term,
    pub(crate) pty: Option<Pty>,
    /// Its command's starts and quick exits, once it has started: each
    /// console counts its own (the operator's answer on vtium/vtium#156).
    pub(crate) respawn: Option<Respawn>,
    /// Its program ended under `oneshot`, so it runs no session any more
    /// and keeps its place, showing that its session ended (ADR-1401 D1).
    pub(crate) ended: bool,
    /// How the console fits the head's displays while it is hidden; the
    /// head holds the shown console's.
    pub(crate) fit: Fit,
}

/// How a console fits its head's displays: its grid, the base font height
/// it draws at and that font's cell, which a zoom changes for the console
/// shown alone. A hidden console's program starts again at its own grid
/// and cell (ADR-1401 D1), as the C reference opened a terminal's pty at
/// that terminal's own size (`terminal_open` at kmscon b1f7a7c5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fit {
    pub(crate) grid: (usize, usize),
    pub(crate) font_size: u32,
    /// The base font's cell at `font_size`, in pixels.
    pub(crate) cell: (u32, u32),
    /// `font-size` in points, until a zoom.
    pub(crate) points: Option<u32>,
    /// `font-size=0`, until a zoom.
    pub(crate) auto_size: bool,
}

impl Console {
    pub(crate) fn new(id: u64, term: Term, fit: Fit) -> Console {
        Console {
            id,
            term,
            pty: None,
            respawn: None,
            ended: false,
            fit,
        }
    }

    /// Its terminal's replies go to its program.
    pub(crate) fn send_replies(&mut self) {
        let bytes = self.term.terminal.take_replies();
        if let Some(pty) = &mut self.pty {
            if !bytes.is_empty() {
                pty.write(&bytes);
            }
        }
    }

    /// Whether its command has started.
    pub(crate) fn started(&self) -> bool {
        self.respawn.is_some()
    }
}

/// The ring position after `at`, or before it going `back`, of `len`.
pub(crate) fn step(at: usize, len: usize, back: bool) -> usize {
    if len == 0 {
        return 0;
    }
    if back {
        at.checked_sub(1).unwrap_or(len.saturating_sub(1))
    } else {
        at.saturating_add(1).checked_rem(len).unwrap_or(0)
    }
}

/// The index in the hidden list, beside the shown console at ring
/// position `at`, of the console at ring position `to`.
pub(crate) fn hidden_index(to: usize, at: usize) -> usize {
    if to < at { to } else { to.saturating_sub(1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_wraps_both_ways() {
        assert_eq!(step(0, 3, false), 1);
        assert_eq!(step(2, 3, false), 0);
        assert_eq!(step(0, 3, true), 2);
        assert_eq!(step(2, 3, true), 1);
        assert_eq!(step(0, 1, false), 0);
        assert_eq!(step(0, 0, true), 0);
    }

    #[test]
    fn a_ring_position_maps_into_the_hidden_list_around_the_shown() {
        // The ring a b C d, C shown at 2: a and b keep their index, d is 2.
        assert_eq!(hidden_index(0, 2), 0);
        assert_eq!(hidden_index(1, 2), 1);
        assert_eq!(hidden_index(3, 2), 2);
    }
}
