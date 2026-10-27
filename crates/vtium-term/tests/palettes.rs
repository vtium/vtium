//! The named palettes against the C reference's tables.
//!
//! `palettes.c.txt` is lines 254-451 of `src/tsm/tsm-vte.c` at aaronsb/libtsm
//! eeaf2931, the commit `tools/conformance/pin.tsv` pins, copied verbatim
//! under the MIT notice COPYING carries for libtsm. This prints nothing:
//!
//! ```sh
//! git -C ../libtsm show eeaf2931985051585bf50979e0c3f588b4a51ef9:src/tsm/tsm-vte.c \
//!   | sed -n 254,451p | diff - crates/vtium-term/tests/palettes.c.txt
//! ```

use vtium_term::{PALETTE_NAMES, Palette, Rgb};

const C_TABLES: &str = include_str!("palettes.c.txt");

/// The C names of the sixteen colours and the two defaults, in palette
/// order.
const C_COLOURS: [&str; 18] = [
    "BLACK",
    "RED",
    "GREEN",
    "YELLOW",
    "BLUE",
    "MAGENTA",
    "CYAN",
    "LIGHT_GREY",
    "DARK_GREY",
    "LIGHT_RED",
    "LIGHT_GREEN",
    "LIGHT_YELLOW",
    "LIGHT_BLUE",
    "LIGHT_MAGENTA",
    "LIGHT_CYAN",
    "WHITE",
    "FOREGROUND",
    "BACKGROUND",
];

/// One C table: its name as the `palette` key spells it, and its eighteen
/// colours in palette order.
struct CTable {
    name: String,
    colours: [Option<Rgb>; 18],
}

/// A C numeral, decimal or `0x` hex.
fn numeral(s: &str) -> u8 {
    let s = s.trim();
    let n = match s.strip_prefix("0x") {
        Some(hex) => u8::from_str_radix(hex, 16),
        None => s.parse(),
    };
    n.unwrap_or_else(|_| panic!("{s:?} is no numeral"))
}

/// Each `color_palette_<name>` table in the excerpt, read line by line.
fn c_tables() -> Vec<CTable> {
    let mut tables: Vec<CTable> = Vec::new();
    for line in C_TABLES.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("static uint8_t color_palette_") {
            let name = rest.split('[').next().unwrap_or_default();
            tables.push(CTable {
                name: name.replace('_', "-"),
                colours: [None; 18],
            });
        } else if let Some(rest) = line.strip_prefix("[TSM_COLOR_") {
            let (colour, rest) = rest.split_once(']').expect("an entry names its colour");
            let at = C_COLOURS
                .iter()
                .position(|c| *c == colour)
                .unwrap_or_else(|| panic!("TSM_COLOR_{colour} is no colour"));
            let body = rest
                .split_once('{')
                .and_then(|(_, b)| b.split_once('}'))
                .map(|(b, _)| b)
                .expect("an entry holds { r, g, b }");
            let v: Vec<u8> = body.split(',').map(numeral).collect();
            let [r, g, b] = v[..] else {
                panic!("{body:?} is not three values")
            };
            let table = tables.last_mut().expect("an entry follows its table");
            assert!(
                table.colours[at].replace(Rgb::new(r, g, b)).is_none(),
                "{} sets TSM_COLOR_{colour} twice",
                table.name
            );
        }
    }
    tables
}

/// The table, as the C reference's tables hold it: the sixteen, then the
/// default foreground and background.
fn eighteen(p: &Palette) -> [Option<Rgb>; 18] {
    let mut out = [None; 18];
    for (i, c) in out.iter_mut().take(16).enumerate() {
        *c = Some(p.color(u8::try_from(i).unwrap()));
    }
    out[16] = Some(p.fg());
    out[17] = Some(p.bg());
    out
}

// covers: colour.palette.named
#[test]
fn each_named_palette_is_the_c_references_table() {
    let tables = c_tables();
    let c_names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        c_names,
        [
            "legacy",
            "vga",
            "nord",
            "solarized",
            "solarized-black",
            "solarized-white",
            "soft-black",
            "base16-dark",
            "base16-light",
        ],
        "the excerpt's tables"
    );
    assert_eq!(PALETTE_NAMES, c_names[..], "the names vtium holds");
    for t in &tables {
        assert!(t.colours.iter().all(Option::is_some), "{} is whole", t.name);
        let p = Palette::named(&t.name).unwrap_or_else(|| panic!("{} is held", t.name));
        for (i, (got, want)) in eighteen(&p).iter().zip(&t.colours).enumerate() {
            assert_eq!(got, want, "{} TSM_COLOR_{}", t.name, C_COLOURS[i]);
        }
    }
}

// covers: colour.palette.named
#[test]
fn every_palette_shares_the_cube_and_the_grey_ramp() {
    // xterm's 256 colours (the C reference's tsm-render.c:54-66).
    let cube = |i: u8| {
        let levels = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
        let n = usize::from(i - 16);
        Rgb::new(levels[n / 36], levels[n / 6 % 6], levels[n % 6])
    };
    for name in PALETTE_NAMES {
        let p = Palette::named(name).unwrap();
        for i in 16..=231 {
            assert_eq!(p.color(i), cube(i), "{name} colour {i}");
        }
        for i in 232..=255 {
            let grey = (i - 232) * 10 + 8;
            assert_eq!(p.color(i), Rgb::new(grey, grey, grey), "{name} colour {i}");
        }
    }
    assert_eq!(
        Palette::named("vga").unwrap().color(196),
        Rgb::new(255, 0, 0)
    );
    assert_eq!(
        Palette::named("nord").unwrap().color(255),
        Rgb::new(238, 238, 238)
    );
}

#[test]
fn the_default_palette_is_legacy() {
    assert_eq!(Palette::named("legacy"), Some(Palette::default()));
}

#[test]
fn only_the_names_vtium_holds_are_palettes() {
    for name in [
        "custom",
        "",
        "Legacy",
        "nord ",
        "solarized_black",
        "default",
    ] {
        assert_eq!(Palette::named(name), None, "{name:?}");
    }
}

// covers: colour.palette.named
#[test]
fn a_custom_palette_takes_its_sixteen_and_its_defaults_over_the_cube() {
    let mut sixteen = [Rgb::default(); 16];
    for (i, c) in sixteen.iter_mut().enumerate() {
        let i = u8::try_from(i).unwrap();
        *c = Rgb::new(i, i * 2, i * 3);
    }
    let fg = Rgb::new(1, 2, 3);
    let bg = Rgb::new(4, 5, 6);
    let p = Palette::custom(sixteen, fg, bg);
    for i in 0..16 {
        assert_eq!(p.color(i), Rgb::new(i, i * 2, i * 3), "colour {i}");
    }
    assert_eq!((p.fg(), p.bg()), (fg, bg));
    let legacy = Palette::default();
    for i in 16..=255 {
        assert_eq!(p.color(i), legacy.color(i), "colour {i}");
    }
}
