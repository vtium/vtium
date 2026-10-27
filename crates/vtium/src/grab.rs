//! The terminal's grabs: keys it takes for itself, third in the key order
//! after the VT's and the seat's. Each is the C reference's default
//! (`src/config.c:1275-1303` at kmscon b1f7a7c5), matched as it matched
//! them (`src/misc/conf.c:1103-1131`): the held modifiers exactly, and the
//! key's keysyms. The C reference had no key to paste; Shift+Insert pastes,
//! as xterm's `insert-selection` translation does.

/// What a grab does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    ZoomIn,
    ZoomOut,
    RotateCw,
    RotateCcw,
    Paste,
}

const SHIFT: u8 = 1;
const CTRL: u8 = 4;
const LOGO: u8 = 16;

const UP: u32 = 0xff52;
const DOWN: u32 = 0xff54;
const PRIOR: u32 = 0xff55;
const NEXT: u32 = 0xff56;
const PLUS: u32 = 0x2b;
const EQUAL: u32 = 0x3d;
const MINUS: u32 = 0x2d;
const INSERT: u32 = 0xff63;

/// Each grab: its modifiers and the one keysym it takes.
const GRABS: [(Grab, u8, u32); 11] = [
    (Grab::ScrollUp, SHIFT, UP),
    (Grab::ScrollDown, SHIFT, DOWN),
    (Grab::PageUp, SHIFT, PRIOR),
    (Grab::PageDown, SHIFT, NEXT),
    (Grab::ZoomIn, CTRL, PLUS),
    (Grab::ZoomIn, CTRL, EQUAL),
    (Grab::ZoomOut, CTRL, MINUS),
    (Grab::RotateCw, LOGO, PLUS),
    (Grab::RotateCw, LOGO, EQUAL),
    (Grab::RotateCcw, LOGO, MINUS),
    (Grab::Paste, SHIFT, INSERT),
];

/// The grab a key held with `mods` (the C reference's bits) and making
/// `keysyms` takes, if any.
pub fn grab(mods: u8, keysyms: &[u32]) -> Option<Grab> {
    GRABS
        .iter()
        .find(|&&(_, m, sym)| m == mods && keysyms == [sym])
        .map(|&(g, _, _)| g)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_c_references_defaults() {
        assert_eq!(grab(SHIFT, &[UP]), Some(Grab::ScrollUp));
        assert_eq!(grab(SHIFT, &[NEXT]), Some(Grab::PageDown));
        assert_eq!(grab(CTRL, &[EQUAL]), Some(Grab::ZoomIn));
        assert_eq!(grab(CTRL, &[MINUS]), Some(Grab::ZoomOut));
        assert_eq!(grab(LOGO, &[PLUS]), Some(Grab::RotateCw));
    }

    #[test]
    fn shift_insert_pastes_as_in_xterm() {
        assert_eq!(grab(SHIFT, &[INSERT]), Some(Grab::Paste));
        assert_eq!(grab(0, &[INSERT]), None);
    }

    #[test]
    fn the_modifiers_match_exactly() {
        assert_eq!(grab(0, &[UP]), None);
        assert_eq!(grab(SHIFT | CTRL, &[UP]), None);
        // Shift held for '+' is not Ctrl alone: Ctrl+= is the zoom.
        assert_eq!(grab(CTRL | SHIFT, &[PLUS]), None);
        assert_eq!(grab(SHIFT, &[UP, DOWN]), None);
    }
}
