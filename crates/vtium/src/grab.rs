//! The keys vtium takes for itself, before the encoder: the [`Chord`]s a
//! grab key binds, which the power keys ([`crate::power`]) take, and the
//! terminal's grabs, third in the key order. Each grab is the C
//! reference's default (`src/config.c:1275-1303` at
//! kmscon b1f7a7c5), matched as it matched them
//! (`src/misc/conf.c:1103-1131`): the held modifiers exactly, and the
//! key's keysyms. The C reference had no key to paste; `grab-paste`
//! binds one, Shift+Insert by default, as xterm's `insert-selection`
//! translation does.

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
}

const SHIFT: u8 = 1;
const LOCK: u8 = 2;
const CTRL: u8 = 4;
const ALT: u8 = 8;
const LOGO: u8 = 16;

const UP: u32 = 0xff52;
const DOWN: u32 = 0xff54;
const PRIOR: u32 = 0xff55;
const NEXT: u32 = 0xff56;
const PLUS: u32 = 0x2b;
const EQUAL: u32 = 0x3d;
const MINUS: u32 = 0x2d;

/// Each grab: its modifiers and the one keysym it takes.
const GRABS: [(Grab, u8, u32); 10] = [
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
];

/// The grab a key held with `mods` (the C reference's bits) and making
/// `keysyms` takes, if any.
pub fn grab(mods: u8, keysyms: &[u32]) -> Option<Grab> {
    GRABS
        .iter()
        .find(|&&(_, m, sym)| m == mods && keysyms == [sym])
        .map(|&(g, _, _)| g)
}

/// A key a configuration key binds: the modifiers held, exactly, and the
/// keysyms the key makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub mods: u8,
    pub keysyms: Vec<u32>,
}

/// The modifiers a chord names, in any case.
const MODIFIERS: [(&str, u8); 6] = [
    ("shift", SHIFT),
    ("lock", LOCK),
    ("control", CTRL),
    ("ctrl", CTRL),
    ("alt", ALT),
    ("logo", LOGO),
];

/// The chords `text` binds, in the C reference's syntax
/// (`src/misc/conf.c:859-1012`): a comma-separated list, each chord
/// `<Mod>...` and then keysym names joined by `+`, such as
/// `<Ctrl><Alt>Delete`. Spaces around a name are dropped. A chord with no
/// keysym, and so an empty list, binds nothing.
pub fn parse(text: &str) -> Result<Vec<Chord>, &'static str> {
    let mut chords = Vec::new();
    for item in text.split(',').filter(|s| !s.is_empty()) {
        let mut chord = Chord {
            mods: 0,
            keysyms: Vec::new(),
        };
        for (i, part) in item.split('+').filter(|s| !s.is_empty()).enumerate() {
            let mut rest = part.trim_start_matches(' ');
            // Only the first part takes modifiers.
            while let Some(after) = rest.strip_prefix('<').filter(|_| i == 0) {
                let (name, tail) = after.split_once('>').ok_or("a '<' with no '>'")?;
                let (_, bit) = MODIFIERS
                    .iter()
                    .find(|(m, _)| m.eq_ignore_ascii_case(name))
                    .ok_or("a modifier other than Shift, Lock, Control, Ctrl, Alt or Logo")?;
                chord.mods |= bit;
                rest = tail.trim_start_matches(' ');
            }
            let name = rest.trim_end_matches(' ');
            if !name.is_empty() {
                chord.keysyms.push(keysym(name)?);
            }
        }
        if !chord.keysyms.is_empty() {
            chords.push(chord);
        }
    }
    Ok(chords)
}

/// The keysym xkb names `name`, case-sensitively, as the C reference
/// looked it up.
fn keysym(name: &str) -> Result<u32, &'static str> {
    use xkbcommon::xkb;
    if name.contains('\0') {
        return Err("a key name xkb does not know");
    }
    let sym = xkb::keysym_from_name(name, xkb::KEYSYM_NO_FLAGS).raw();
    if sym != 0 {
        return Ok(sym);
    }
    if xkb::keysym_from_name(name, xkb::KEYSYM_CASE_INSENSITIVE).raw() != 0 {
        Err("a key name in the wrong case; key names are case-sensitive")
    } else {
        Err("a key name xkb does not know")
    }
}

/// A key held with `mods` and making `keysyms` is one of `chords`.
pub fn matches(chords: &[Chord], mods: u8, keysyms: &[u32]) -> bool {
    chords
        .iter()
        .any(|c| c.mods == mods && c.keysyms == keysyms)
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
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
    fn the_modifiers_match_exactly() {
        assert_eq!(grab(0, &[UP]), None);
        assert_eq!(grab(SHIFT | CTRL, &[UP]), None);
        // Shift held for '+' is not Ctrl alone: Ctrl+= is the zoom.
        assert_eq!(grab(CTRL | SHIFT, &[PLUS]), None);
        assert_eq!(grab(SHIFT, &[UP, DOWN]), None);
    }

    const DELETE: u32 = 0xffff;
    const END: u32 = 0xff57;

    fn chord(mods: u8, keysyms: &[u32]) -> Chord {
        Chord {
            mods,
            keysyms: keysyms.to_vec(),
        }
    }

    #[test]
    fn a_chord_reads_as_the_c_reference_read_it() {
        assert_eq!(
            parse("<Ctrl><Alt>Delete"),
            Ok(vec![chord(CTRL | ALT, &[DELETE])])
        );
        // Modifier names in any case, spaces around the names, `+`
        // joining parts, a list.
        assert_eq!(
            parse(" <control> <ALT> Delete ,<Logo>+End,<Shift>Up+Down"),
            Ok(vec![
                chord(CTRL | ALT, &[DELETE]),
                chord(LOGO, &[END]),
                chord(SHIFT, &[UP, DOWN]),
            ])
        );
        assert_eq!(parse("<Lock>plus"), Ok(vec![chord(LOCK, &[PLUS])]));
        // Nothing to bind: an empty value, a chord of modifiers alone.
        assert_eq!(parse(""), Ok(vec![]));
        assert_eq!(parse(",,<Ctrl>"), Ok(vec![]));
    }

    #[test]
    fn a_chord_that_does_not_read_is_refused() {
        assert_eq!(
            parse("<Ctrl>NoSuchKey"),
            Err("a key name xkb does not know")
        );
        assert_eq!(
            parse("<Ctrl>delete"),
            Err("a key name in the wrong case; key names are case-sensitive")
        );
        assert_eq!(parse("<Ctrl Delete"), Err("a '<' with no '>'"));
        assert_eq!(
            parse("<Hyper>Delete"),
            Err("a modifier other than Shift, Lock, Control, Ctrl, Alt or Logo")
        );
        // Modifiers only before the first key.
        assert_eq!(
            parse("Delete+<Ctrl>End"),
            Err("a key name xkb does not know")
        );
        assert_eq!(parse("<Ctrl>\0"), Err("a key name xkb does not know"));
    }

    #[test]
    fn a_chord_matches_its_modifiers_and_keysyms_exactly() {
        let chords = parse("<Ctrl><Alt>Delete,<Logo>End").expect("chords");
        assert!(matches(&chords, CTRL | ALT, &[DELETE]));
        assert!(matches(&chords, LOGO, &[END]));
        assert!(!matches(&chords, CTRL | ALT | SHIFT, &[DELETE]));
        assert!(!matches(&chords, CTRL, &[DELETE]));
        assert!(!matches(&chords, CTRL | ALT, &[END]));
        assert!(!matches(&chords, CTRL | ALT, &[DELETE, END]));
        assert!(!matches(&[], CTRL | ALT, &[DELETE]));
    }
}
