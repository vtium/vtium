//! The chords a key step may press: those a terminal can send as a key.
//! Ctrl and I, M, H or [ send the same byte as Tab, Enter, Backspace and
//! Escape, so they parse as those keys and the canonical text writes them
//! that way. Alt and [, ], _ or a capital P send ESC and a byte that opens
//! a control sequence or a control string, so no terminal can send them as
//! a key, and the parser refuses them with the reason.
//!
//! References:
//! - ADR-1208 D3: a key reaches the TUI as a terminal sends it, and a key
//!   that no terminal can send fails when the macro is parsed.
//! - ADR-1208 D10: the key type, a set of modifiers and a key.

use vtium_config_macro::{
    Character, Count, Key, KeyPath, KeyPress, KeyStep, Mods, Step, Unpressable, parse,
};

const CTRL: Mods = Mods {
    ctrl: true,
    alt: false,
    shift: false,
};
const ALT: Mods = Mods {
    ctrl: false,
    alt: true,
    shift: false,
};
const ALT_SHIFT: Mods = Mods {
    ctrl: false,
    alt: true,
    shift: true,
};

fn ch(mods: Mods, c: char) -> KeyPress {
    KeyPress {
        mods,
        key: Key::Char(Character::new(c).unwrap()),
    }
}

fn one(line: &str) -> Step {
    let m = parse(line).unwrap_or_else(|e| panic!("{line:?}: {e}"));
    assert_eq!(m.steps().len(), 1, "{line:?}");
    m.steps()[0].clone()
}

fn plain(key: Key, n: u8) -> Step {
    Step::Key(KeyStep::new(KeyPress::plain(key), Count::new(n).unwrap()).unwrap())
}

fn msg(line: &str) -> String {
    let e = parse(line).unwrap_err();
    assert_eq!(e.line(), 1, "{line:?}: {e}");
    e.message().to_owned()
}

#[test]
fn ctrl_chords_that_share_a_byte_with_a_key_are_that_key() {
    for (chord, key) in [
        ("Ctrl+I", Key::Tab),
        ("Ctrl+M", Key::Enter),
        ("Ctrl+H", Key::Backspace),
        ("Ctrl+[", Key::Escape),
        ("Ctrl+i", Key::Tab),
        ("Ctrl+m", Key::Enter),
        ("Ctrl+h", Key::Backspace),
    ] {
        assert_eq!(one(chord), plain(key, 1), "{chord}");
        assert_eq!(one(&format!("{chord} 3")), plain(key, 3), "{chord} 3");
    }
    // The canonical text writes the key the TUI gets.
    assert_eq!(
        parse("Ctrl+I\nCtrl+[ 2\n").unwrap().to_string(),
        "Tab\nEscape 2\n"
    );
    // A step built from the chord holds the same key.
    let built = KeyStep::new(ch(CTRL, 'M'), Count::ONE).unwrap();
    assert_eq!(built.press(), KeyPress::plain(Key::Enter));
    // Other Ctrl chords, and these letters with more modifiers, stay as written.
    assert_eq!(
        one("Ctrl+S"),
        Step::Key(KeyStep::new(ch(CTRL, 'S'), Count::ONE).unwrap())
    );
    assert_eq!(parse("Ctrl+Alt+I\n").unwrap().to_string(), "Ctrl+Alt+I\n");
}

#[test]
fn alt_chords_that_open_a_sequence_are_refused_with_the_reason() {
    assert_eq!(
        msg("Alt+["),
        "Alt+[ cannot be pressed: a terminal sends it as ESC [, which opens a control sequence."
    );
    assert_eq!(
        msg("Alt+Shift+P"),
        "Alt+Shift+P cannot be pressed: a terminal sends it as ESC P, which opens a device control string."
    );
    assert_eq!(
        msg("Alt+]"),
        "Alt+] cannot be pressed: a terminal sends it as ESC ], which opens an operating system command."
    );
    assert_eq!(
        msg("Alt+Shift+_"),
        "Alt+Shift+_ cannot be pressed: a terminal sends it as ESC _, which opens an application program command."
    );
    // The same bytes, however the chord is written, and with a count.
    for line in ["Alt+P", "Shift+Alt+p", "Alt+_", "Alt+Shift+[ 2", "Alt+] 5"] {
        assert!(
            msg(line).contains("cannot be pressed: a terminal sends it as ESC"),
            "{line}"
        );
    }
    // The error names the line it is on.
    let e = parse("Enter\n# nav\n\nAlt+[\n").unwrap_err();
    assert_eq!(
        e.in_file("nav.vtmacro").to_string(),
        "nav.vtmacro:4: Alt+[ cannot be pressed: a terminal sends it as ESC [, which opens a control sequence."
    );
}

#[test]
fn alt_chords_a_terminal_sends_as_a_key_parse() {
    for line in [
        "Alt+Shift+L",
        "Alt+L",
        "Alt+p",
        "Alt+{",
        "Alt+N",
        "Alt+Enter",
    ] {
        assert!(parse(line).is_ok(), "{line}");
    }
}

#[test]
fn a_step_built_from_a_chord_no_terminal_sends_is_refused() {
    assert_eq!(
        KeyStep::new(ch(ALT, '['), Count::ONE),
        Err(Unpressable::Opens('['))
    );
    assert_eq!(
        KeyStep::new(ch(ALT_SHIFT, 'p'), Count::ONE),
        Err(Unpressable::Opens('P'))
    );
    assert_eq!(
        KeyStep::new(ch(Mods::NONE, '#'), Count::ONE),
        Err(Unpressable::Hash)
    );
}

#[test]
fn a_key_path_with_such_a_chord_does_not_become_a_macro() {
    let mut p = KeyPath::new();
    p.push(KeyPress::plain(Key::Down));
    p.push(ch(ALT, ']'));
    let e = p.to_macro().unwrap_err();
    assert_eq!(e.line(), 2);
    assert_eq!(
        e.message(),
        "Alt+] cannot be pressed: a terminal sends it as ESC ], which opens an operating system command."
    );
    // Ctrl+I in a path plays back as Tab.
    let mut p = KeyPath::new();
    p.push(ch(CTRL, 'I'));
    assert_eq!(p.to_macro().unwrap().to_string(), "Tab\n");
}
