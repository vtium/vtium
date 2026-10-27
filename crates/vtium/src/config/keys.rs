//! The key table: every configuration key and command-line option vtium
//! reads, with its type, default, bound and the crate that reads it.
//!
//! `docs/config.md` and the installed example, `install/kmscon.conf.example`,
//! are generated from [`KEYS`]; the test `generated_files_match_the_table`
//! fails when either differs, and `VTIUM_BLESS=1` rewrites them.

use vtium_font::CELL_SIDE_MAX;
use vtium_term::{SB_SIZE_DEFAULT, SB_SIZE_MAX};

/// The most bytes a text value or a `login` command holds.
pub const TEXT_MAX: usize = 4095;

/// What a key holds, and its bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// On or off: `name` turns it on and `no-name` off.
    Switch,
    /// A whole number from `min` to `max`, in `unit`, or `word` in its
    /// place where the key takes one.
    Number {
        min: u64,
        max: u64,
        unit: &'static str,
        word: Option<&'static str>,
    },
    /// Text of at most [`TEXT_MAX`] bytes with no control character.
    Text,
    /// One of a fixed set of words.
    Choice(&'static [&'static str]),
    /// A colour, `r,g,b`, each 0 to 255.
    Rgb,
    /// A display mode, `[GPU][CONNECTOR]WxH[@RATE]`. Each line adds one:
    /// the seat's file adds to the main file's, and the command line's
    /// replace both.
    Mode,
    /// A command and its arguments. In a file, one line split like shell
    /// words; on the command line, `--login -- ARGS...`, the words after
    /// `--`.
    Command,
}

/// A key's value before any source sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Default {
    Switch(bool),
    Number(u64),
    Text(&'static str),
    Command(&'static [&'static str]),
    Rgb([u8; 3]),
    /// No value: the reader decides, as `what` says.
    Unset,
}

/// Where a key may be set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// In a configuration file or on the command line.
    Anywhere,
    /// On the command line only.
    CommandLine,
}

/// One row of the key table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub name: &'static str,
    /// The one-letter command-line option, if it has one.
    pub short: Option<char>,
    pub kind: Kind,
    pub default: Default,
    pub scope: Scope,
    /// The crate that reads the value.
    pub reader: &'static str,
    /// What it does, one sentence or two.
    pub what: &'static str,
    /// Where it comes from: the C reference's lines or the decision.
    pub cite: &'static str,
}

/// The values of `glyph-source` and its per-range keys.
const GLYPH_SOURCES: &[&str] = &["font", "sprites", "auto"];

/// The C reference's `unsigned int`, the bound of a number it left
/// unbounded.
const UINT_MAX: u64 = u32::MAX as u64;

pub const CONFIGDIR: Key = Key {
    name: "configdir",
    short: Some('c'),
    kind: Kind::Text,
    default: Default::Text("/etc/kmscon"),
    scope: Scope::CommandLine,
    reader: "vtium",
    what: "The directory holding `kmscon.conf` and `<seat>.seat.conf`.",
    cite: "src/config.c:1349",
};

pub const HELP: Key = Key {
    name: "help",
    short: Some('h'),
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::CommandLine,
    reader: "vtium",
    what: "Print the options and exit.",
    cite: "src/config.c:1342",
};

pub const VERSION: Key = Key {
    name: "version",
    short: Some('V'),
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::CommandLine,
    reader: "vtium",
    what: "Print the version and exit.",
    cite: "src/config.c:1343",
};

pub const VT: Key = Key {
    name: "vt",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "The VT to run on: a number N for `/dev/ttyN`, a name under `/dev`, or a path. Unset, the VT vtium was started on, or the next free one.",
    cite: "src/config.c:329-361, 1352",
};

pub const SWITCHVT: Key = Key {
    name: "switchvt",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "Switch to vtium's VT at start; off, wait until something else switches to it.",
    cite: "src/config.c:1353",
};

pub const LOGIN: Key = Key {
    name: "login",
    short: Some('l'),
    kind: Kind::Command,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The session's command. Unset, `/bin/login -p` with `reset-env` on and `/bin/login` with it off. A word that is `$NAME` becomes that variable of the session's environment, and `$$` starts a word with `$`.",
    cite: "src/config.c:372-494, 1365; src/misc/pty.c:252-302",
};

pub const ONESHOT: Key = Key {
    name: "oneshot",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Exit when the session's command exits, in place of starting it again.",
    cite: "src/config.c:1367; src/misc/pty.c:752-756",
};

pub const TERM: Key = Key {
    name: "term",
    short: Some('t'),
    kind: Kind::Text,
    default: Default::Text("kmscon"),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "`TERM` in the session's environment.",
    cite: "src/config.c:1368; meson.options:51-52",
};

pub const RESET_ENV: Key = Key {
    name: "reset-env",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Start the session with an empty environment and the variables vtium sets; off, with vtium's own environment as well.",
    cite: "src/config.c:1369; src/misc/pty.c:261-272",
};

pub const BACKSPACE_DELETE: Key = Key {
    name: "backspace-delete",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "The pty's erase character is DEL (0x7f); off, BS (0x08).",
    cite: "src/config.c:1370; src/misc/pty.c:360-366",
};

pub const SB_SIZE: Key = Key {
    name: "sb-size",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: SB_SIZE_MAX as u64,
        unit: "lines",
        word: None,
    },
    default: Default::Number(SB_SIZE_DEFAULT as u64),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Lines of scrollback, at most `sb-size-cap`.",
    cite: "src/config.c:1371; ADR-1001, \"Scrollback size\"",
};

pub const SB_SIZE_CAP: Key = Key {
    name: "sb-size-cap",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: SB_SIZE_MAX as u64,
        unit: "lines",
        word: None,
    },
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "The most lines `sb-size` may ask for. Unset, it follows the machine's memory: 10,000 lines below 2 GiB, 30,000 from 2 GiB and 100,000 from 4 GiB.",
    cite: "ADR-1001, \"Scrollback size\"; the operator on vtium/vtium#21",
};

pub const BLINK: Key = Key {
    name: "blink",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Blink text with SGR 5 and a blinking cursor, every 500 ms; off, neither blinks.",
    cite: "src/config.c:1373; src/terminal.c:163, 715-724",
};

pub const SYNC_TIMEOUT: Key = Key {
    name: "sync-timeout",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: 10_000,
        unit: "ms",
        word: None,
    },
    default: Default::Number(200),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The longest a synchronized frame (mode 2026) is held before it is shown as drawn; 0 offers no mode 2026.",
    cite: "src/config.c:1374; src/terminal_sync.h:62-66, `TERMINAL_SYNC_MAX_MS`",
};

pub const DPMS_TIMEOUT: Key = Key {
    name: "dpms-timeout",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: UINT_MAX,
        unit: "s",
        word: None,
    },
    default: Default::Number(600),
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "Turn the displays off after this long with no key or pointer event; 0 never does.",
    cite: "src/config.c:1388",
};

pub const DISPLAY_WAIT: Key = Key {
    name: "display-wait",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: 600,
        unit: "s",
        word: Some("forever"),
    },
    default: Default::Number(10),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "At start, how long to wait for a display, counted from when the VT is first shown; with none by then vtium exits with status 1, so the unit's `OnFailure=` can start a getty. 0 does not wait. `forever` waits for the first display however long it takes, for a machine whose only display is a USB adapter (DisplayLink, `udl`) plugged in at any time. Once a display has come, vtium never exits for want of one.",
    cite: "the operator on vtium/vtium#21",
};

pub const HWACCEL: Key = Key {
    name: "hwaccel",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Draw through the GPU, in a build that has it.",
    cite: "src/config.c:1413",
};

pub const USE_ORIGINAL_MODE: Key = Key {
    name: "use-original-mode",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "Keep the mode each display is in; off, its preferred mode.",
    cite: "src/config.c:1416; vtium/vtium#5",
};

pub const MODE: Key = Key {
    name: "mode",
    short: None,
    kind: Kind::Mode,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "A display's mode, `WxH[@RATE]`, for every display, `[CONNECTOR]WxH` for one connector, or `GPU[CONNECTOR]WxH` for one on one card, its udev `ID_PATH`. A key per line; the most specific match wins. Unset, the panel's preferred mode. It turns `use-original-mode` off.",
    cite: "src/config.c:546-660, 1417, 1541-1548",
};

pub const PALETTE: Key = Key {
    name: "palette",
    short: None,
    kind: Kind::Choice(&[
        "legacy",
        "custom",
        "nord",
        "solarized",
        "solarized-black",
        "solarized-white",
        "soft-black",
        "base16-dark",
        "base16-light",
        "vga",
    ]),
    default: Default::Text("legacy"),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "The sixteen colours and the default foreground and background: a named palette, or `custom`, which takes the `palette-*` keys. Colours 16 to 255 are xterm's in every palette.",
    cite: "src/config.c:1427; libtsm src/tsm/tsm-vte.c:254-478",
};

pub const PALETTE_BLACK: Key = Key {
    name: "palette-black",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 0, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 0 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_RED: Key = Key {
    name: "palette-red",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([205, 0, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 1 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_GREEN: Key = Key {
    name: "palette-green",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 205, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 2 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_YELLOW: Key = Key {
    name: "palette-yellow",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([205, 205, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 3 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_BLUE: Key = Key {
    name: "palette-blue",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 0, 238]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 4 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_MAGENTA: Key = Key {
    name: "palette-magenta",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([205, 0, 205]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 5 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_CYAN: Key = Key {
    name: "palette-cyan",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 205, 205]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 6 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_GREY: Key = Key {
    name: "palette-light-grey",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([229, 229, 229]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 7 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_DARK_GREY: Key = Key {
    name: "palette-dark-grey",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([127, 127, 127]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 8 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_RED: Key = Key {
    name: "palette-light-red",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 0, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 9 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_GREEN: Key = Key {
    name: "palette-light-green",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 255, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 10 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_YELLOW: Key = Key {
    name: "palette-light-yellow",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 255, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 11 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_BLUE: Key = Key {
    name: "palette-light-blue",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([92, 92, 255]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 12 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_MAGENTA: Key = Key {
    name: "palette-light-magenta",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 0, 255]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 13 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_CYAN: Key = Key {
    name: "palette-light-cyan",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 255, 255]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 14 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_WHITE: Key = Key {
    name: "palette-white",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 255, 255]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Colour 15 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_FOREGROUND: Key = Key {
    name: "palette-foreground",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([229, 229, 229]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "The default foreground with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_BACKGROUND: Key = Key {
    name: "palette-background",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 0, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "The default background with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const CLIPBOARD_WRITE: Key = Key {
    name: "clipboard-write",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Take the selection a program sets with OSC 52; off, it is dropped. The console has no system clipboard: vtium holds the last selection taken for a paste, which is not built yet.",
    cite: "ADR-1000 (`osc.52.write`); vtium/vtium#39",
};

pub const XKB_MODEL: Key = Key {
    name: "xkb-model",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "The keyboard model, as xkeyboard-config names it.",
    cite: "src/config.c:1377-1382",
};

pub const XKB_LAYOUT: Key = Key {
    name: "xkb-layout",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "The keyboard layout, such as `us` or `de`.",
    cite: "src/config.c:1377-1382",
};

pub const XKB_VARIANT: Key = Key {
    name: "xkb-variant",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "The layout's variant.",
    cite: "src/config.c:1377-1382",
};

pub const XKB_OPTIONS: Key = Key {
    name: "xkb-options",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "xkb options, comma-separated.",
    cite: "src/config.c:1377-1382",
};

pub const XKB_KEYMAP: Key = Key {
    name: "xkb-keymap",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "A keymap file, taken before the names above.",
    cite: "src/config.c:1377-1382",
};

pub const XKB_COMPOSE_FILE: Key = Key {
    name: "xkb-compose-file",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "A compose table, taken before the locale's.",
    cite: "src/config.c:1377-1382",
};

pub const XKB_REPEAT_DELAY: Key = Key {
    name: "xkb-repeat-delay",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: 999,
        unit: "ms",
        word: None,
    },
    default: Default::Number(250),
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "How long a held key waits before it repeats; 0 takes 250.",
    cite: "src/config.c:1383; src/input/input.c:570-585",
};

pub const XKB_REPEAT_RATE: Key = Key {
    name: "xkb-repeat-rate",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: 999,
        unit: "ms",
        word: None,
    },
    default: Default::Number(50),
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "The time between a held key's repeats; 0 takes 50.",
    cite: "src/config.c:1384; src/input/input.c:570-585",
};

pub const MOUSE: Key = Key {
    name: "mouse",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "Take pointers: mice, touchpads and touchscreens.",
    cite: "src/config.c:1385",
};

pub const FONT_ENGINE: Key = Key {
    name: "font-engine",
    short: None,
    kind: Kind::Choice(&["freetype", "pango", "psf", "unifont", "8x16"]),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "The font source to use. Unset, each the build has, in this order.",
    cite: "src/config.c:1422; src/font/font.c:331-368",
};

pub const FONT_NAME: Key = Key {
    name: "font-name",
    short: None,
    kind: Kind::Text,
    default: Default::Text("monospace"),
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "The font's name, or with `font-engine=psf` the path of a PSF font.",
    cite: "src/config.c:1424",
};

pub const FONT_SIZE: Key = Key {
    name: "font-size",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: CELL_SIDE_MAX as u64,
        unit: "px",
        word: None,
    },
    default: Default::Number(0),
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "The font's height; 0 picks one from the display's size.",
    cite: "src/config.c:1423; vtium-font's `CELL_SIDE_MAX`",
};

pub const GLYPH_SOURCE: Key = Key {
    name: "glyph-source",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "Where the six sprite ranges draw from: `font`, the font's glyphs; `sprites`, vtium's; `auto`, the font's where it has one. Unset, a named font's glyphs for each range it covers whole and the sprites for the rest; the built-in unifont draws the sprites.",
    cite: "vtium/vtium#85",
};

pub const GLYPH_SOURCE_BOX: Key = Key {
    name: "glyph-source-box",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "`glyph-source` for box drawing, U+2500-U+257F, over it.",
    cite: "vtium/vtium#85",
};

pub const GLYPH_SOURCE_BLOCK: Key = Key {
    name: "glyph-source-block",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "`glyph-source` for block elements, U+2580-U+259F, over it.",
    cite: "vtium/vtium#85",
};

pub const GLYPH_SOURCE_BRAILLE: Key = Key {
    name: "glyph-source-braille",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "`glyph-source` for Braille, U+2800-U+28FF, over it.",
    cite: "vtium/vtium#85",
};

pub const GLYPH_SOURCE_SEXTANTS: Key = Key {
    name: "glyph-source-sextants",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "`glyph-source` for sextants, U+1FB00-U+1FB3B, over it.",
    cite: "vtium/vtium#85",
};

pub const GLYPH_SOURCE_OCTANTS: Key = Key {
    name: "glyph-source-octants",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "`glyph-source` for octants, U+1CD00-U+1CDE5, over it.",
    cite: "vtium/vtium#85",
};

pub const GLYPH_SOURCE_LEGACY: Key = Key {
    name: "glyph-source-legacy",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "`glyph-source` for the rest of Symbols for Legacy Computing's block shapes, over it.",
    cite: "vtium/vtium#85",
};

pub const MISSING: Key = Key {
    name: "missing",
    short: None,
    kind: Kind::Choice(&["box", "hex"]),
    default: Default::Text("box"),
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "What a character the font lacks draws as: `box`, U+25A1; `hex`, its codepoint in hex digits.",
    cite: "src/font/font.c:345-366; vtium/vtium#85",
};

pub const GLYPH_CACHE: Key = Key {
    name: "glyph-cache",
    short: None,
    kind: Kind::Choice(&["minspec", "performance"]),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "The glyph cache's budgets. Unset, `minspec`, or `performance` in a build with vtium-render's `performance-cache` feature.",
    cite: "ADR-1300; vtium/vtium#16",
};

pub const RENDER_TIMER: Key = Key {
    name: "render-timer",
    short: None,
    kind: Kind::Choice(&["none", "osd", "log", "both"]),
    default: Default::Text("none"),
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "Measure each display's frames once a second, and write the figures to the log, draw them on screen, or both.",
    cite: "src/config.c:972-1015, 1475",
};

/// Every key, in the order the generated files list them.
pub const KEYS: [Key; 61] = [
    CONFIGDIR,
    HELP,
    VERSION,
    VT,
    SWITCHVT,
    LOGIN,
    ONESHOT,
    TERM,
    RESET_ENV,
    BACKSPACE_DELETE,
    SB_SIZE,
    SB_SIZE_CAP,
    BLINK,
    SYNC_TIMEOUT,
    DPMS_TIMEOUT,
    DISPLAY_WAIT,
    HWACCEL,
    USE_ORIGINAL_MODE,
    MODE,
    PALETTE,
    PALETTE_BLACK,
    PALETTE_RED,
    PALETTE_GREEN,
    PALETTE_YELLOW,
    PALETTE_BLUE,
    PALETTE_MAGENTA,
    PALETTE_CYAN,
    PALETTE_LIGHT_GREY,
    PALETTE_DARK_GREY,
    PALETTE_LIGHT_RED,
    PALETTE_LIGHT_GREEN,
    PALETTE_LIGHT_YELLOW,
    PALETTE_LIGHT_BLUE,
    PALETTE_LIGHT_MAGENTA,
    PALETTE_LIGHT_CYAN,
    PALETTE_WHITE,
    PALETTE_FOREGROUND,
    PALETTE_BACKGROUND,
    CLIPBOARD_WRITE,
    XKB_MODEL,
    XKB_LAYOUT,
    XKB_VARIANT,
    XKB_OPTIONS,
    XKB_KEYMAP,
    XKB_COMPOSE_FILE,
    XKB_REPEAT_DELAY,
    XKB_REPEAT_RATE,
    MOUSE,
    FONT_ENGINE,
    FONT_NAME,
    FONT_SIZE,
    GLYPH_SOURCE,
    GLYPH_SOURCE_BOX,
    GLYPH_SOURCE_BLOCK,
    GLYPH_SOURCE_BRAILLE,
    GLYPH_SOURCE_SEXTANTS,
    GLYPH_SOURCE_OCTANTS,
    GLYPH_SOURCE_LEGACY,
    MISSING,
    GLYPH_CACHE,
    RENDER_TIMER,
];

/// The keys vtium takes and does not act on yet, which the crate
/// README's "Not yet built" lists; `check::semantics` warns when one is
/// set.
pub const NOT_BUILT: [&Key; 1] = [&HWACCEL];

/// The key named `name`.
pub fn find(name: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.name == name)
}

/// The key with the one-letter option `short`.
pub fn find_short(short: char) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.short == Some(short))
}
