//! The key names a scenario's `key` command takes, and the modifiers its
//! `mods=` names. A key name is an X11 keysym name (the X Window System's
//! keysym list, as xkbcommon's `xkbcommon-keysyms.h` names it), one
//! printable ASCII character, or `0x` and a keysym in hex.

/// The keysyms a scenario names, with the character the key makes when it
/// makes one that its keysym number does not already say.
const NAMED: &[(&str, u32, Option<char>)] = &[
    ("BackSpace", 0xff08, None),
    ("Tab", 0xff09, None),
    ("Linefeed", 0xff0a, None),
    ("Clear", 0xff0b, None),
    ("Return", 0xff0d, None),
    ("Pause", 0xff13, None),
    ("Scroll_Lock", 0xff14, None),
    ("Sys_Req", 0xff15, None),
    ("Escape", 0xff1b, None),
    ("Home", 0xff50, None),
    ("Left", 0xff51, None),
    ("Up", 0xff52, None),
    ("Right", 0xff53, None),
    ("Down", 0xff54, None),
    ("Prior", 0xff55, None),
    ("Page_Up", 0xff55, None),
    ("Next", 0xff56, None),
    ("Page_Down", 0xff56, None),
    ("End", 0xff57, None),
    ("Begin", 0xff58, None),
    ("Select", 0xff60, None),
    ("Print", 0xff61, None),
    ("Execute", 0xff62, None),
    ("Insert", 0xff63, None),
    ("Undo", 0xff65, None),
    ("Redo", 0xff66, None),
    ("Menu", 0xff67, None),
    ("Find", 0xff68, None),
    ("Cancel", 0xff69, None),
    ("Help", 0xff6a, None),
    ("Break", 0xff6b, None),
    ("Num_Lock", 0xff7f, None),
    ("KP_Space", 0xff80, None),
    ("KP_Tab", 0xff89, None),
    ("KP_Enter", 0xff8d, None),
    ("KP_F1", 0xff91, None),
    ("KP_F2", 0xff92, None),
    ("KP_F3", 0xff93, None),
    ("KP_F4", 0xff94, None),
    ("KP_Home", 0xff95, None),
    ("KP_Left", 0xff96, None),
    ("KP_Up", 0xff97, None),
    ("KP_Right", 0xff98, None),
    ("KP_Down", 0xff99, None),
    ("KP_Prior", 0xff9a, None),
    ("KP_Page_Up", 0xff9a, None),
    ("KP_Next", 0xff9b, None),
    ("KP_Page_Down", 0xff9b, None),
    ("KP_End", 0xff9c, None),
    ("KP_Begin", 0xff9d, None),
    ("KP_Insert", 0xff9e, None),
    ("KP_Delete", 0xff9f, None),
    ("KP_Equal", 0xffbd, None),
    ("KP_Multiply", 0xffaa, None),
    ("KP_Add", 0xffab, None),
    ("KP_Separator", 0xffac, None),
    ("KP_Subtract", 0xffad, None),
    ("KP_Decimal", 0xffae, None),
    ("KP_Divide", 0xffaf, None),
    ("KP_0", 0xffb0, None),
    ("KP_1", 0xffb1, None),
    ("KP_2", 0xffb2, None),
    ("KP_3", 0xffb3, None),
    ("KP_4", 0xffb4, None),
    ("KP_5", 0xffb5, None),
    ("KP_6", 0xffb6, None),
    ("KP_7", 0xffb7, None),
    ("KP_8", 0xffb8, None),
    ("KP_9", 0xffb9, None),
    ("Shift_L", 0xffe1, None),
    ("Shift_R", 0xffe2, None),
    ("Control_L", 0xffe3, None),
    ("Control_R", 0xffe4, None),
    ("Caps_Lock", 0xffe5, None),
    ("Meta_L", 0xffe7, None),
    ("Meta_R", 0xffe8, None),
    ("Alt_L", 0xffe9, None),
    ("Alt_R", 0xffea, None),
    ("Super_L", 0xffeb, None),
    ("Super_R", 0xffec, None),
    ("Delete", 0xffff, None),
    ("ISO_Left_Tab", 0xfe20, None),
    ("space", 0x20, None),
    ("exclam", 0x21, None),
    ("quotedbl", 0x22, None),
    ("numbersign", 0x23, None),
    ("dollar", 0x24, None),
    ("percent", 0x25, None),
    ("ampersand", 0x26, None),
    ("apostrophe", 0x27, None),
    ("parenleft", 0x28, None),
    ("parenright", 0x29, None),
    ("asterisk", 0x2a, None),
    ("plus", 0x2b, None),
    ("comma", 0x2c, None),
    ("minus", 0x2d, None),
    ("period", 0x2e, None),
    ("slash", 0x2f, None),
    ("colon", 0x3a, None),
    ("semicolon", 0x3b, None),
    ("less", 0x3c, None),
    ("equal", 0x3d, None),
    ("greater", 0x3e, None),
    ("question", 0x3f, None),
    ("at", 0x40, None),
    ("bracketleft", 0x5b, None),
    ("backslash", 0x5c, None),
    ("bracketright", 0x5d, None),
    ("asciicircum", 0x5e, None),
    ("underscore", 0x5f, None),
    ("grave", 0x60, None),
    ("braceleft", 0x7b, None),
    ("bar", 0x7c, None),
    ("braceright", 0x7d, None),
    ("asciitilde", 0x7e, None),
    ("nobreakspace", 0xa0, None),
    ("sterling", 0xa3, None),
    ("eacute", 0xe9, None),
    ("Cyrillic_a", 0x6c1, Some('\u{430}')),
    ("Cyrillic_es", 0x6d3, Some('\u{441}')),
    ("Cyrillic_A", 0x6e1, Some('\u{410}')),
    ("Cyrillic_ES", 0x6f3, Some('\u{421}')),
    ("Greek_alpha", 0x7e1, Some('\u{3b1}')),
];

/// The modifiers `mods=` names, with their bits in the interface's `mods`.
pub const MODS: [(&str, u8); 8] = [
    ("shift", 1),
    ("lock", 2),
    ("ctrl", 4),
    ("alt", 8),
    ("logo", 16),
    ("num", 32),
    ("hyper", 64),
    ("meta", 128),
];

/// The keysym a key name stands for, or None.
pub fn keysym(name: &str) -> Option<u32> {
    if let Some(hex) = name.strip_prefix("0x") {
        return (!hex.is_empty() && hex.len() <= 8 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| u32::from_str_radix(hex, 16).ok())
            .flatten()
            .filter(|&s| s <= 0x1fff_ffff);
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && (' '..='~').contains(&c)
    {
        return Some(u32::from(c));
    }
    if let Some(digit) = name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok())
        && (1..=35).contains(&digit)
        && !name.starts_with("F0")
    {
        return Some(0xffbe + digit - 1);
    }
    NAMED.iter().find(|(n, ..)| *n == name).map(|&(_, s, _)| s)
}

/// The keysym an ASCII layout gives the same key: the keysym itself for a
/// printable ASCII one, and 0, none, for every other.
pub fn ascii_of(sym: u32) -> u32 {
    if (0x20..=0x7e).contains(&sym) { sym } else { 0 }
}

/// The character a key with this keysym makes: printable ASCII and
/// Latin-1 keysyms make their own number, a Unicode keysym (0x1000000 and
/// the code point) its code point, a named keysym its character, and the
/// function, keypad and modifier keysyms none.
pub fn char_of(sym: u32) -> Option<char> {
    match sym {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(sym),
        0x0100_0000..=0x0110_ffff => char::from_u32(sym - 0x0100_0000),
        _ => NAMED
            .iter()
            .find(|(_, s, _)| *s == sym)
            .and_then(|(.., c)| *c),
    }
}

/// The keysym a key that makes `c` has on a layout that has the character
/// on a key of its own: its own number for printable ASCII and Latin-1,
/// and a Unicode keysym for every other character.
pub fn keysym_of_char(c: char) -> u32 {
    match u32::from(c) {
        n @ (0x20..=0x7e | 0xa0..=0xff) => n,
        n => 0x0100_0000 + n,
    }
}

/// The bits of `mods=` written as names joined by `+`, or None.
pub fn mods(text: &str) -> Option<u8> {
    let mut bits = 0u8;
    for name in text.split('+') {
        let (_, bit) = MODS.iter().find(|(n, _)| *n == name)?;
        if bits & bit != 0 {
            return None;
        }
        bits |= bit;
    }
    Some(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_resolve_to_their_keysyms() {
        assert_eq!(keysym("Up"), Some(0xff52));
        assert_eq!(keysym("KP_Enter"), Some(0xff8d));
        assert_eq!(keysym("F1"), Some(0xffbe));
        assert_eq!(keysym("F12"), Some(0xffc9));
        assert_eq!(keysym("F35"), Some(0xffe0));
        assert_eq!(keysym("F36"), None);
        assert_eq!(keysym("F0"), None);
        assert_eq!(keysym("F01"), None);
        assert_eq!(keysym("a"), Some(0x61));
        assert_eq!(keysym("#"), Some(0x23));
        assert_eq!(keysym("space"), Some(0x20));
        assert_eq!(keysym("Cyrillic_es"), Some(0x6d3));
        assert_eq!(keysym("0x1000441"), Some(0x0100_0441));
        assert_eq!(keysym("0x"), None);
        assert_eq!(keysym("0xffffffff"), None);
        assert_eq!(keysym("Upp"), None);
        assert_eq!(keysym("é"), None, "only ASCII stands for itself");
    }

    #[test]
    fn a_key_makes_its_character() {
        assert_eq!(char_of(0x61), Some('a'));
        assert_eq!(char_of(0xe9), Some('é'));
        assert_eq!(char_of(0x6d3), Some('с'));
        assert_eq!(char_of(0x0100_0441), Some('с'));
        assert_eq!(char_of(0xff52), None);
        assert_eq!(ascii_of(0x61), 0x61);
        assert_eq!(ascii_of(0x6d3), 0);
        assert_eq!(keysym_of_char('с'), 0x0100_0441);
        assert_eq!(keysym_of_char('A'), 0x41);
    }

    #[test]
    fn modifiers_join_with_plus_once_each() {
        assert_eq!(mods("shift"), Some(1));
        assert_eq!(mods("ctrl+alt"), Some(12));
        assert_eq!(mods("shift+lock+ctrl+alt+logo"), Some(31));
        assert_eq!(mods("num+hyper+meta"), Some(224));
        assert_eq!(mods("shift+shift"), None);
        assert_eq!(mods("super"), None);
        assert_eq!(mods(""), None);
    }
}
