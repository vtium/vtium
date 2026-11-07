//! The heads' chords (ADR-1401 D1, D5, vtium/vtium#82): the console
//! chords, which switch among the consoles of the head the keyboard
//! delivers to and, under an open policy, open and close them, and the
//! focus chord, which moves the focus between the heads. vtium.conf binds
//! each to its keys (ADR-1005 D10, [`crate::grab`]), and they are matched
//! after the VT's keys and the power keys and before the terminal's
//! grabs. The console chords' default keys are the keys `kmscon` binds by
//! default to its next, previous, new and closed session, so a user who
//! knows them finds them in vtium. The focus chord's default keys are the
//! operator's (vtium/vtium#156, 2026-10-30).

/// What a chord does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadChord {
    /// The head's next console.
    NextConsole,
    /// The head's previous console.
    PreviousConsole,
    /// A new console after the one the head shows, under an open policy.
    OpenConsole,
    /// The console the head shows closes, under an open policy.
    CloseConsole,
    /// The focus to the next head with a lit display.
    NextHead,
    /// The focus to the previous head with a lit display.
    PreviousHead,
}

impl HeadChord {
    /// Whether the chord acts on the consoles of the head the keyboard
    /// delivers to, and so is taken from every keyboard; the focus chord
    /// is taken from a roaming keyboard alone.
    pub(crate) fn is_console(self) -> bool {
        !matches!(self, HeadChord::NextHead | HeadChord::PreviousHead)
    }
}

/// The head after `from` of `count` heads, or before it going `back`,
/// that `lit` holds, wrapping; `from` itself never, and `None` when no
/// other head is lit.
pub(crate) fn step(
    from: Option<usize>,
    count: usize,
    back: bool,
    lit: impl Fn(usize) -> bool,
) -> Option<usize> {
    let order: Vec<usize> = if back {
        (0..count).rev().collect()
    } else {
        (0..count).collect()
    };
    let after = from
        .and_then(|f| order.iter().position(|&h| h == f))
        .map_or(0, |i| i.saturating_add(1));
    order
        .iter()
        .cycle()
        .skip(after)
        .take(count)
        .copied()
        .find(|&h| Some(h) != from && lit(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_skips_the_heads_with_no_lit_display_and_wraps() {
        let lit = |h: usize| h != 2;
        assert_eq!(step(Some(1), 4, false, lit), Some(3));
        assert_eq!(step(Some(3), 4, false, lit), Some(0));
        assert_eq!(step(Some(0), 4, true, lit), Some(3));
        assert_eq!(step(Some(3), 4, true, lit), Some(1));
        assert_eq!(step(None, 4, false, lit), Some(0));
        assert_eq!(step(Some(0), 1, false, |_| true), None, "no other head");
        assert_eq!(step(Some(0), 3, false, |h| h == 0), None, "none lit");
    }

    #[test]
    fn only_the_focus_chord_is_no_console_chord() {
        assert!(HeadChord::NextConsole.is_console());
        assert!(HeadChord::CloseConsole.is_console());
        assert!(!HeadChord::NextHead.is_console());
        assert!(!HeadChord::PreviousHead.is_console());
    }
}
