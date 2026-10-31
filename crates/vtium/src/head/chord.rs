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
    /// The head's next console.
    NextConsole,
    /// The head's previous console.
    PreviousConsole,
    /// A new console in the head, shown.
    NewConsole,
    /// The shown console closed, the next shown.
    CloseConsole,
    /// The focus to the next head with a lit display.
    NextHead,
    /// The focus to the previous head with a lit display.
    PreviousHead,
}

use crate::grab::{ALT, CTRL, LOGO, SHIFT};

/// Ctrl and Super, in the C reference's bits.
const CTRL_SUPER: u8 = CTRL | LOGO;

/// Each chord and the key that makes it, held with [`CTRL_SUPER`]: the one
/// binding the matcher reads and the notices name.
const BINDINGS: [(HeadChord, u32); 6] = [
    (HeadChord::NextConsole, 0xff53),
    (HeadChord::PreviousConsole, 0xff51),
    (HeadChord::NewConsole, 0xff0d),
    (HeadChord::CloseConsole, 0xff08),
    (HeadChord::NextHead, 0xff52),
    (HeadChord::PreviousHead, 0xff54),
];

/// The modifiers' names, by their bits, in the order a chord's keys are
/// written: Ctrl, Alt, Shift, then Super.
const MOD_NAMES: [(u8, &str); 4] = [
    (CTRL, "Ctrl"),
    (ALT, "Alt"),
    (SHIFT, "Shift"),
    (LOGO, "Super"),
];

/// A key as a chord's keys write it: its xkb name, and an arrow key as
/// its arrow.
fn key_name(keysym: u32) -> String {
    let name = xkbcommon::xkb::keysym_get_name(keysym.into());
    match name.as_str() {
        "Up" => "\u{2191}".to_owned(),
        "Down" => "\u{2193}".to_owned(),
        "Left" => "\u{2190}".to_owned(),
        "Right" => "\u{2192}".to_owned(),
        _ => name,
    }
}

/// The keys of `chord` as a person presses them, such as `Ctrl+Super+↑`:
/// the modifiers it is held with, in the order Ctrl, Alt, Shift, Super,
/// then its key, joined by `+`, for the notices that tell a person which
/// keys to press.
pub(crate) fn keys(chord: HeadChord) -> String {
    let key = BINDINGS
        .iter()
        .find(|&&(c, _)| c == chord)
        .map_or(0, |&(_, key)| key);
    let mut keys: Vec<String> = MOD_NAMES
        .iter()
        .filter(|&&(bit, _)| CTRL_SUPER & bit != 0)
        .map(|&(_, name)| name.to_owned())
        .collect();
    keys.push(key_name(key));
    keys.join("+")
}

/// The chord a key held with `mods` (the C reference's bits) and making
/// `keysyms` is, if any.
pub(crate) fn chord(mods: u8, keysyms: &[u32]) -> Option<HeadChord> {
    if mods != CTRL_SUPER {
        return None;
    }
    let [keysym] = keysyms else {
        return None;
    };
    BINDINGS
        .iter()
        .find(|&&(_, key)| key == *keysym)
        .map(|&(chord, _)| chord)
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

    /// The keysym xkb names `name`.
    fn sym(name: &str) -> u32 {
        xkbcommon::xkb::keysym_from_name(name, xkbcommon::xkb::KEYSYM_NO_FLAGS).raw()
    }

    #[test]
    fn a_chord_is_its_keys_with_ctrl_and_super_exactly() {
        let ctrl_super = 4 | 16;
        assert_eq!(chord(ctrl_super, &[sym("Up")]), Some(HeadChord::NextHead));
        assert_eq!(
            chord(ctrl_super, &[sym("Down")]),
            Some(HeadChord::PreviousHead)
        );
        assert_eq!(
            chord(ctrl_super, &[sym("Right")]),
            Some(HeadChord::NextConsole)
        );
        assert_eq!(
            chord(ctrl_super, &[sym("Left")]),
            Some(HeadChord::PreviousConsole)
        );
        assert_eq!(
            chord(ctrl_super, &[sym("Return")]),
            Some(HeadChord::NewConsole)
        );
        assert_eq!(
            chord(ctrl_super, &[sym("BackSpace")]),
            Some(HeadChord::CloseConsole)
        );
        assert_eq!(chord(4, &[sym("Up")]), None, "Ctrl alone");
        assert_eq!(chord(ctrl_super | 1, &[sym("Up")]), None, "with Shift");
    }

    #[test]
    fn a_chords_keys_name_its_modifiers_and_its_key() {
        assert_eq!(keys(HeadChord::NextHead), "Ctrl+Super+↑");
        assert_eq!(keys(HeadChord::PreviousHead), "Ctrl+Super+↓");
        assert_eq!(keys(HeadChord::NextConsole), "Ctrl+Super+→");
        assert_eq!(keys(HeadChord::PreviousConsole), "Ctrl+Super+←");
        assert_eq!(keys(HeadChord::NewConsole), "Ctrl+Super+Return");
        assert_eq!(keys(HeadChord::CloseConsole), "Ctrl+Super+BackSpace");
    }

    // The keys a notice names are the keys the matcher takes: each
    // chord's written keys, read back into modifiers and a keysym, make
    // that chord and no other.
    #[test]
    fn the_keys_a_notice_names_make_the_chord() {
        for &(want, _) in &BINDINGS {
            let written = keys(want);
            let mut parts: Vec<&str> = written.split('+').collect();
            let key = parts.pop().unwrap_or_default();
            let mods = parts.iter().fold(0, |m, part| {
                m | MOD_NAMES
                    .iter()
                    .find(|&&(_, name)| name == *part)
                    .map_or(0, |&(bit, _)| bit)
            });
            let name = match key {
                "\u{2191}" => "Up",
                "\u{2193}" => "Down",
                "\u{2190}" => "Left",
                "\u{2192}" => "Right",
                other => other,
            };
            assert_eq!(chord(mods, &[sym(name)]), Some(want), "{written}");
        }
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
