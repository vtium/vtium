//! The heads' chords (ADR-1401 D5, vtium/vtium#82): the console chords,
//! which switch the consoles of the head the keyboard delivers to, and
//! the focus chord, which moves the focus between the heads. Each is held
//! with Ctrl and Super exactly, as the grabs match their modifiers, and is
//! matched after the VT's keys and the power keys and before the
//! terminal's grabs, the C reference's place for its session keys
//! (`src/seat.c:584-640` at kmscon b1f7a7c5). The console chords are the
//! C reference's defaults (`src/config.c:1288-1299`); the focus chord's
//! keys are the operator's (vtium/vtium#156, 2026-10-30).

/// What a chord does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeadChord {
    /// The focus to the next head with a lit display.
    NextHead,
    /// The focus to the previous head with a lit display.
    PreviousHead,
}

/// Ctrl and Super, in the C reference's bits.
const CTRL_SUPER: u8 = 4 | 16;

const UP: u32 = 0xff52;
const DOWN: u32 = 0xff54;

/// The chord a key held with `mods` (the C reference's bits) and making
/// `keysyms` is, if any.
pub(crate) fn chord(mods: u8, keysyms: &[u32]) -> Option<HeadChord> {
    if mods != CTRL_SUPER {
        return None;
    }
    match keysyms {
        [UP] => Some(HeadChord::NextHead),
        [DOWN] => Some(HeadChord::PreviousHead),
        _ => None,
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
    fn a_chord_is_its_keys_with_ctrl_and_super_exactly() {
        assert_eq!(chord(CTRL_SUPER, &[UP]), Some(HeadChord::NextHead));
        assert_eq!(chord(CTRL_SUPER, &[DOWN]), Some(HeadChord::PreviousHead));
        assert_eq!(chord(4, &[UP]), None, "Ctrl alone");
        assert_eq!(chord(CTRL_SUPER | 1, &[UP]), None, "with Shift");
    }

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
}
