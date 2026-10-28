//! The host's side of the replay interface: what a host sends for keys,
//! the pointer, pastes and focus; the host settings a program reads back;
//! and what a host learns of frames. Each step ends at a cut, so its
//! replies are a line of their own.

use super::kitty::kitty;
use super::{CSI, CUT, Case, DCS, ESC, Inputs, Made, OSC, ST, hex, sgr, sized, test_command};

const SHIFT: u32 = 1;
const CTRL: u32 = 4;
const ALT: u32 = 8;

/// A key by X11 keysym, as a host passes it, with the ASCII keysym of a
/// key from a non-Latin layout and the character it makes.
fn key(sym: u32, mods: u32, ascii: Option<u32>, unicode: Option<u32>) -> String {
    let mut keys = vec![("sym", format!("{sym:x}"))];
    if let Some(a) = ascii {
        keys.push(("ascii", format!("{a:x}")));
    }
    if let Some(u) = unicode {
        keys.push(("unicode", format!("{u:x}")));
    }
    if mods != 0 {
        keys.push(("mods", mods.to_string()));
    }
    test_command("key", &keys)
}

/// A key that makes the character `c`, its keysym the Latin-1 code.
fn char_key(c: char, mods: u32) -> String {
    key(u32::from(c), mods, None, Some(u32::from(c)))
}

/// The Cyrillic layout's key whose ASCII keysym is c.
fn es(mods: u32) -> String {
    key(0x6d3, mods, Some(0x63), Some(0x441))
}

/// A pointer event at `col`, `row`, with any further keys.
fn mouse(col: u32, row: u32, button: &str, event: &str, more: &[(&str, String)]) -> String {
    let mut keys = vec![
        ("col", col.to_string()),
        ("row", row.to_string()),
        ("button", button.to_owned()),
        ("event", event.to_owned()),
    ];
    keys.extend_from_slice(more);
    test_command("mouse", &keys)
}

fn mods(m: u32) -> [(&'static str, String); 1] {
    [("mods", m.to_string())]
}

fn paste(text: &str) -> String {
    tt!("paste", hex = hex(text))
}

const PALETTES: [&str; 9] = [
    "legacy",
    "vga",
    "nord",
    "solarized",
    "solarized-black",
    "solarized-white",
    "soft-black",
    "base16-dark",
    "base16-light",
];

pub(super) const CASES: &[Case] = &[
    sized("input-key", 20, 2, input_key),
    sized("input-mouse", 20, 4, input_mouse),
    // pastes, as a host passes them: plain, then bracketed (mode 2004) with
    // a newline, UTF-8, nothing at all, and the bytes of the closing
    // bracket itself, which go through as they are; then plain again
    sized("input-paste", 20, 2, |_| {
        [
            &paste("hi"),
            CUT,
            CSI,
            "?2004h",
            &paste("a\nb"),
            CUT,
            &paste("\u{e9}"),
            &paste(""),
            CUT,
            &paste(&[CSI, "201~x"].concat()),
            CUT,
            CSI,
            "?2004l",
            &paste("z"),
        ]
        .concat()
        .into()
    }),
    sized("input-focus", 20, 2, input_focus),
    sized("host-palette", 18, 2, host_palette),
    sized("host-identify", 20, 2, host_identify),
    sized("query-frames", 10, 2, query_frames),
];

/// Keys by X11 keysym, as a host passes them: characters, a key from a
/// Cyrillic layout whose ASCII keysym is c, Ctrl and Alt each alone and
/// together, editing and function keys, cursor keys in both DECCKM
/// states, the keypad in DECKPAM, Return under LNM, then modifyOtherKeys
/// at levels 1 and 2, with XTQMODKEYS reporting the level.
fn input_key(_: &Inputs) -> Made {
    let k = |sym: u32| key(sym, 0, None, None);
    let km = |sym: u32, m: u32| key(sym, m, None, None);
    [
        char_key('a', 0),
        char_key('A', SHIFT),
        key(0x20ac, 0, None, Some(0x20ac)),
        es(0),
        CUT.into(),
        km(0x61, CTRL),
        es(CTRL),
        km(0x20, CTRL),
        CUT.into(),
        char_key('a', ALT),
        char_key('a', CTRL | ALT),
        km(0xff0d, ALT),
        CUT.into(),
        k(0xff0d),
        k(0xff08),
        k(0xff09),
        km(0xfe20, SHIFT),
        k(0xff1b),
        k(0xffff),
        k(0xffbe),
        k(0xffc2),
        km(0xffc2, SHIFT),
        CUT.into(),
        k(0xff52),
        km(0xff52, CTRL),
        km(0xff52, ALT),
        km(0xff52, SHIFT),
        [CSI, "?1h"].concat(),
        k(0xff52),
        k(0xff50),
        [CSI, "?1l", CUT].concat(),
        k(0xff8d),
        [ESC, "="].concat(),
        k(0xff8d),
        k(0xffb1),
        [ESC, ">", CSI, "20h"].concat(),
        k(0xff0d),
        [CSI, "20l", CUT, CSI, ">4;1m"].concat(),
        char_key('1', CTRL),
        char_key('a', CTRL),
        char_key('a', ALT),
        [CUT, CSI, ">4;2m"].concat(),
        km(0xfe20, SHIFT),
        char_key('a', CTRL),
        char_key('1', CTRL | ALT),
        [CSI, "?4m", CSI, ">4;0m"].concat(),
    ]
    .concat()
    .into()
}

/// Pointer events, as a host passes them, under each tracking mode and
/// encoding: none set, X10 (presses only, no modifiers, no wheel), 1000
/// (releases, the wheel, modifiers), 1002 (drags), 1003 (all motion, one
/// report per cell entered), SGR, and pixels under 1016, each reported
/// move; then every mode reset.
fn input_mouse(_: &Inputs) -> Made {
    let m = |col, row, button, event| mouse(col, row, button, event, &[]);
    let px = |x: u32| [("px", x.to_string()), ("py", "40".to_owned())];
    [
        m(1, 1, "left", "press"),
        [CUT, CSI, "?9h"].concat(),
        m(2, 1, "left", "press"),
        m(2, 1, "left", "release"),
        mouse(2, 1, "right", "press", &mods(SHIFT)),
        m(2, 1, "wheel-up", "press"),
        [CSI, "?9l", CUT, CSI, "?1000h"].concat(),
        m(0, 0, "left", "press"),
        m(0, 0, "left", "release"),
        m(1, 0, "none", "move"),
        m(0, 0, "wheel-up", "press"),
        m(0, 0, "wheel-up", "release"),
        mouse(3, 2, "middle", "press", &mods(CTRL)),
        [CUT, CSI, "?1002h"].concat(),
        m(3, 1, "left", "move"),
        m(4, 1, "none", "move"),
        [CSI, "?1002l", CUT, CSI, "?1003h"].concat(),
        m(5, 2, "none", "move"),
        m(5, 2, "none", "move"),
        m(6, 2, "none", "move"),
        [CUT, CSI, "?1006h"].concat(),
        m(4, 2, "left", "press"),
        m(4, 2, "left", "release"),
        m(4, 2, "wheel-down", "press"),
        m(7, 3, "middle", "move"),
        mouse(19, 3, "right", "press", &mods(SHIFT | ALT)),
        [CUT, CSI, "?1016h"].concat(),
        mouse(0, 0, "left", "press", &px(100)),
        mouse(0, 0, "none", "move", &px(101)),
        mouse(0, 0, "none", "move", &px(102)),
        [
            CUT, CSI, "?1016l", CSI, "?1006l", CSI, "?1003l", CSI, "?1000l",
        ]
        .concat(),
        m(1, 1, "left", "press"),
    ]
    .concat()
    .into()
}

/// Focus changes, as a host reports them: nothing before mode 1004,
/// nothing on setting it, then CSI O and CSI I for each change, a repeat
/// included; DECSTR, RIS and RM each reset the mode. DECRQM reports it.
fn input_focus(_: &Inputs) -> Made {
    let q = [CSI, "?1004$p"].concat();
    let (focus_in, out) = (tt!("focus", in = 1), tt!("focus", in = 0));
    [
        &focus_in, &out, &q, CUT, CSI, "?1004h", &q, CUT, &focus_in, &out, &out, CUT, CSI, "!p",
        &q, &focus_in, CUT, CSI, "?1004h", ESC, "c", &q, &out, CUT, CSI, "?1004h", CSI, "?1004l",
        &q, &out,
    ]
    .concat()
    .into()
}

/// Each palette the host can choose, in turn and back to the first: the
/// sixteen colors as foregrounds and as backgrounds, a default cell and an
/// inverse one, and OSC 4, 10 and 11 answering from the palette in force.
/// Choosing a palette erases the screen and resets the pen, so text
/// written in red before a change is gone and the text after it takes the
/// default colors.
fn host_palette(_: &Inputs) -> Made {
    let run = |codes: [std::ops::Range<u32>; 2]| -> String {
        codes
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, c)| format!("{}{i:x}", sgr(&[c])))
            .collect()
    };
    let off = sgr(&[0]);
    let text = [
        CSI,
        "H",
        &run([30..38, 90..98]),
        &off,
        "d",
        &sgr(&[7]),
        "i",
        &off,
        "\r\n",
        &run([40..48, 100..108]),
        &off,
        OSC,
        "4;1;?",
        ST,
        OSC,
        "10;?",
        ST,
        OSC,
        "11;?",
        ST,
    ]
    .concat();
    let mut out = [
        &text,
        CUT,
        &sgr(&[31]),
        "x",
        &tt!("palette", name = "legacy"),
        "y",
        CUT,
    ]
    .concat();
    for p in &PALETTES[1..] {
        out += &[&tt!("palette", name = p), &text, CUT].concat();
    }
    (out + &tt!("palette", name = "legacy") + &text).into()
}

/// What a program can learn of the host's settings: the XTVERSION text,
/// set, kept across DECSTR, and empty; XTGETTCAP from no table, from a
/// table fixture (a string, a number, a boolean, a parameterised string,
/// and an unknown name ending the list), and from an empty one; and DA1
/// and a kitty query as sixel and kitty graphics go off and on.
fn host_identify(_: &Inputs) -> Made {
    let caps = format!(
        "TN={}\nCo={}\nbce\nSmulx={}\n",
        hex("kmscon"),
        hex("256"),
        hex(&[CSI, "4:%p1%dm"].concat())
    );
    let xtversion = [CSI, ">q"].concat();
    let xtgettcap = |names: &[&str]| {
        let names: Vec<String> = names.iter().map(|n| hex(n)).collect();
        [DCS, "+q", &names.join(";"), ST].concat()
    };
    let kq = kitty("i=31,a=q,s=1,v=1,f=24", b"\0\0\0");
    let da1 = [CSI, "c"].concat();
    Made::with_files(
        [
            &xtversion,
            CUT,
            &tt!("version", text = "Other 1.0"),
            &xtversion,
            CSI,
            "!p",
            &xtversion,
            CUT,
            &tt!("version", text = ""),
            &xtversion,
            CUT,
            &xtgettcap(&["TN"]),
            CUT,
            &tt!("termcap", file = "caps"),
            &xtgettcap(&["TN", "Co", "bce", "Smulx"]),
            &xtgettcap(&["TN", "xx", "Co"]),
            CUT,
            &tt!("termcap", file = "empty"),
            &xtgettcap(&["TN"]),
            CUT,
            &da1,
            &kq,
            &tt!("sixel", on = 0),
            &tt!("kitty-graphics", on = 0),
            &da1,
            &kq,
            CUT,
            &tt!("sixel", on = 1),
            &tt!("kitty-graphics", on = 1),
            &da1,
            &kq,
        ]
        .concat(),
        &[("caps", caps.into_bytes()), ("empty", Vec::new())],
    )
}

/// What a host learns of frames: each frame-since caller's changes since
/// its own last frame, a fresh caller and a resize seeing every cell, and
/// the count of mode 2026 holds ended by RM, RIS and the host turning sync
/// off.
fn query_frames(_: &Inputs) -> Made {
    let since = |n: u32| tt!("frame-since", caller = n);
    let frames = tt!("sync-frames");
    let (bsu, esu) = ([CSI, "?2026h"].concat(), [CSI, "?2026l"].concat());
    [
        "ab",
        &since(0),
        &since(0),
        &since(7),
        CUT,
        "c",
        &since(0),
        &since(1),
        &since(0),
        CUT,
        &frames,
        &bsu,
        "d",
        &esu,
        &frames,
        &bsu,
        ESC,
        "c",
        &frames,
        &bsu,
        &tt!("sync", on = 0),
        &frames,
        &tt!("sync", on = 1),
        CUT,
        &tt!("resize", c = 12, r = 2),
        &since(0),
        &since(1),
    ]
    .concat()
    .into()
}
