//! The kitty keyboard protocol on real keyboard layouts (ADR-1005, D9):
//! xkeyboard-config's US, Russian, German and French AZERTY layouts, as the
//! committed fixtures of `crates/vtium-platform/tests/keymaps/` hold them.
//! Each key goes through vtium-platform's keymap, which reports its key
//! events, through the binary's conversion of an event into vtium-term's
//! description of a key, and through vtium-term's encoder.
//!
//! The expected bytes follow the specification, "Comprehensive keyboard
//! handling in terminals" (<https://sw.kovidgoyal.net/kitty/keyboard-protocol/>),
//! "Key codes": the key code is the key in the current layout with no
//! modifier, the shifted key its shifted version in the current layout, and
//! the base layout key the key in the standard PC-101 US layout. A test that
//! needs a fixture fails when the file is missing or does not compile.

use std::path::PathBuf;

use vtium::app::term_key;
use vtium_platform::input::{Compiled, InputEvent, Keymap, KeymapConfig};
use vtium_term::{KeyEvent, Terminal};

// evdev's key codes, `linux/input-event-codes.h`.
const KEY_1: u16 = 2;
const KEY_E: u16 = 18;
const KEY_Y: u16 = 21;
const KEY_Q: u16 = 16;
const KEY_LEFTCTRL: u16 = 29;
const KEY_GRAVE: u16 = 41;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_C: u16 = 46;
const KEY_CAPSLOCK: u16 = 58;
const KEY_RIGHTALT: u16 = 100;

/// The committed fixture `name`.xkb, compiled as vtium.conf's `xkb-keymap`
/// compiles a keymap file. It panics when the file is missing or does not
/// compile, so a test never runs on another keymap in its place.
fn layout(name: &str) -> Keymap {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../vtium-platform/tests/keymaps")
        .join(format!("{name}.xkb"));
    assert!(path.is_file(), "the fixture {} exists", path.display());
    let config = KeymapConfig {
        keymap_file: Some(path.clone()),
        ..KeymapConfig::default()
    };
    let keymap = Keymap::new(&config).expect("a keymap");
    assert_eq!(
        keymap.compiled(),
        &Compiled::File(path),
        "the fixture {name}.xkb compiles"
    );
    keymap
}

/// What a program reads when `keys`, evdev's key codes pressed (`true`) or
/// released (`false`), are typed on `keymap` after it wrote `setup`.
fn typed(keymap: &Keymap, setup: &[u8], keys: &[(u16, bool)]) -> String {
    let mut t = Terminal::new(80, 4, 1000);
    t.input(setup);
    t.take_replies();
    for event in keymap.key_events(keys) {
        let key = match event {
            InputEvent::Key(k) => term_key(&k, KeyEvent::Press),
            InputEvent::Release(k) => term_key(&k, KeyEvent::Release),
            _ => None,
        };
        if let Some(key) = key {
            t.key(&key);
        }
    }
    String::from_utf8_lossy(&t.take_replies()).into_owned()
}

/// `code` pressed and released while each of `held` is held.
fn chord(held: &[u16], code: u16) -> Vec<(u16, bool)> {
    let mut keys: Vec<(u16, bool)> = held.iter().map(|&h| (h, true)).collect();
    keys.extend([(code, true), (code, false)]);
    keys.extend(held.iter().rev().map(|&h| (h, false)));
    keys
}

/// Flags 5: disambiguate escape codes and report alternate keys.
const FLAGS_5: &[u8] = b"\x1b[>5u";
/// Flags 13: flags 5 and report all keys as escape codes.
const FLAGS_13: &[u8] = b"\x1b[>13u";

// Russian: the key at US `c` makes с, and the base layout key c lets a
// program match Ctrl+С as Ctrl+C. Its unshifted key is с, 1089, and its
// shifted key С, 1057. Caps Lock makes С with no shifted key, since Shift
// is not held.
// covers: input.kitty-keyboard
#[test]
fn the_russian_layout_sends_its_keys_forms() {
    let ru = layout("ru");
    let ctrl = [KEY_LEFTCTRL];
    let ctrl_shift = [KEY_LEFTCTRL, KEY_LEFTSHIFT];
    assert_eq!(
        typed(&ru, FLAGS_5, &chord(&ctrl, KEY_C)),
        "\x1b[1089::99;5u"
    );
    assert_eq!(
        typed(&ru, FLAGS_5, &chord(&ctrl_shift, KEY_C)),
        "\x1b[1089:1057:99;6u"
    );
    assert_eq!(
        typed(&ru, FLAGS_5, &chord(&[KEY_LEFTSHIFT], KEY_C)),
        "\u{421}"
    );
    assert_eq!(
        typed(&ru, FLAGS_13, &chord(&[], KEY_C)),
        "\x1b[1089::99u",
        "flag 8 sends the text key as its code"
    );
    let caps = [
        (KEY_CAPSLOCK, true),
        (KEY_CAPSLOCK, false),
        (KEY_C, true),
        (KEY_C, false),
    ];
    assert_eq!(typed(&ru, FLAGS_5, &caps), "\u{421}");
    assert_eq!(
        typed(&ru, FLAGS_13, &caps),
        "\x1b[57358;65u\x1b[1089::99;65u",
        "Caps Lock's own key, then с with Caps Lock on"
    );
}

// German, QWERTZ: Ctrl+Z on the key at US `y` is the code z, 122, with the
// base layout key y, 121. AltGr+Q makes @, which a modifier the protocol
// cannot encode chose, so the key reports no unshifted key and the code is
// the @ it made, 64; it sends its text with flag 1, and its code with the
// base layout key q, 113, under flag 8. The dead circumflex sends nothing
// on its own press, and the composed ê is sent as the platform reports it:
// its text, or under flag 8 the code of the key that ended the sequence, e.
// covers: input.kitty-keyboard
#[test]
fn the_german_layout_sends_its_keys_forms() {
    let de =
        layout("de").with_compose_table(b"<dead_circumflex> <e> : \"\xc3\xaa\" ecircumflex\n", "C");
    assert_eq!(
        typed(&de, FLAGS_5, &chord(&[KEY_LEFTCTRL], KEY_Y)),
        "\x1b[122::121;5u"
    );
    let alt_gr_q = chord(&[KEY_RIGHTALT], KEY_Q);
    assert_eq!(typed(&de, FLAGS_5, &alt_gr_q), "@");
    assert_eq!(
        typed(&de, FLAGS_13, &alt_gr_q),
        "\x1b[57453u\x1b[64::113u",
        "AltGr's own key, ISO_Level3_Shift, then @"
    );
    let dead = [(KEY_GRAVE, true), (KEY_GRAVE, false)];
    assert_eq!(typed(&de, FLAGS_5, &dead), "");
    assert_eq!(typed(&de, FLAGS_13, &dead), "");
    let composed = [
        (KEY_GRAVE, true),
        (KEY_GRAVE, false),
        (KEY_E, true),
        (KEY_E, false),
    ];
    assert_eq!(typed(&de, FLAGS_5, &composed), "\u{ea}");
    assert_eq!(typed(&de, FLAGS_13, &composed), "\x1b[101u");
}

// French AZERTY: the key at US `1` makes & and with Shift 1, so its
// unshifted key is &, 38, its shifted key 1, 49, and its base layout key
// 1, 49. Ctrl+A on the key at US `q` is the code a, 97, with the base
// layout key q, 113.
// covers: input.kitty-keyboard
#[test]
fn the_french_azerty_layout_sends_its_keys_forms() {
    let fr = layout("fr");
    assert_eq!(
        typed(&fr, FLAGS_5, &chord(&[KEY_LEFTCTRL], KEY_1)),
        "\x1b[38::49;5u"
    );
    assert_eq!(
        typed(&fr, FLAGS_5, &chord(&[KEY_LEFTCTRL, KEY_LEFTSHIFT], KEY_1)),
        "\x1b[38:49:49;6u"
    );
    assert_eq!(
        typed(&fr, FLAGS_5, &chord(&[KEY_LEFTCTRL], KEY_Q)),
        "\x1b[97::113;5u"
    );
    assert_eq!(typed(&fr, FLAGS_5, &chord(&[], KEY_1)), "&");
}

// US, the control: every base layout key equals the key's code, so none is
// sent, and Ctrl+Shift+1 is the code 1 with the shifted key !.
// covers: input.kitty-keyboard
#[test]
fn the_us_layout_sends_its_keys_forms() {
    let us = layout("us");
    let ctrl = [KEY_LEFTCTRL];
    assert_eq!(typed(&us, FLAGS_5, &chord(&ctrl, KEY_C)), "\x1b[99;5u");
    assert_eq!(typed(&us, FLAGS_5, &chord(&ctrl, KEY_Y)), "\x1b[121;5u");
    assert_eq!(typed(&us, FLAGS_5, &chord(&ctrl, KEY_Q)), "\x1b[113;5u");
    assert_eq!(
        typed(&us, FLAGS_5, &chord(&[KEY_LEFTCTRL, KEY_LEFTSHIFT], KEY_1)),
        "\x1b[49:33;6u"
    );
    assert_eq!(typed(&us, FLAGS_13, &chord(&[], KEY_C)), "\x1b[99u");
}
