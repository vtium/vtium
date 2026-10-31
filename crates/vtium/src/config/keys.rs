//! The key table: every configuration key and command-line option vtium
//! reads, with its type, default, bound and the crate that reads it.
//!
//! `docs/config.md` and the installed example, `install/kmscon.conf.example`,
//! are generated from [`KEYS`]; the test `generated_files_match_the_table`
//! fails when either differs, and `VTIUM_BLESS=1` rewrites them.

use vtium_font::CELL_SIDE_MAX;
use vtium_term::{SB_MEMORY_DEFAULT, SB_MEMORY_MAX, SB_MEMORY_MIN, SB_SIZE_DEFAULT, SB_SIZE_MAX};

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
    /// A color, `r,g,b`, each 0 to 255.
    Rgb,
    /// A display mode, `[GPU][CONNECTOR]WxH[@RATE]`. Each line adds one:
    /// the seat's file adds to the main file's, and the command line's
    /// replace both.
    Mode,
    /// A command and its arguments. In a file, one line split like shell
    /// words; on the command line, `--login -- ARGS...`, the words after
    /// `--`.
    Command,
    /// Keys a grab takes: chords such as `<Ctrl><Alt>Delete`, separated by
    /// commas ([`crate::grab::parse`]). Empty, it takes none.
    Grab,
    /// One character, no control character or space.
    Char,
    /// A decoration's color: `fg`, the cell's foreground; a palette index
    /// from 0 to 255, which follows the palette a program changes; or
    /// `#rrggbb`.
    DecoColor,
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

pub const HEAD_CONSOLES_MAX: Key = Key {
    name: "head-consoles-max",
    short: None,
    kind: Kind::Number {
        min: 1,
        max: vtium_config::CONSOLES_MAX as u64,
        unit: "consoles",
        word: None,
    },
    default: Default::Number(4),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The most consoles one head holds: the head file's `consoles` and Ctrl-Super-Return open no more.",
    cite: "ADR-1401 D10; the C reference's `session-max` (src/config.c:1357) bounded the seat's sessions",
};

pub const OSD: Key = Key {
    name: "osd",
    short: None,
    kind: Kind::Choice(&["off", "minimal", "full"]),
    default: Default::Text("minimal"),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "What vtium's on-screen display draws at the top of each head after a change. With `minimal`, it draws a dot in the top-right cell, filled on the active head and hollow on the others. With `full`, it draws a line across the top, such as `● desk · console 2 of 3`. With `off`, it draws nothing. A notice of a problem shows whatever this key says.",
    cite: "ADR-1401 D5; the operator on vtium/vtium#169",
};

pub const OSD_TIMEOUT: Key = Key {
    name: "osd-timeout",
    short: None,
    kind: Kind::Number {
        min: 1,
        max: 600,
        unit: "s",
        word: Some("never"),
    },
    default: Default::Number(3),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "How long the on-screen display stays after a change, counted from the first frame that draws it. `never` keeps it up from the start.",
    cite: "ADR-1401 D5; the operator on vtium/vtium#169",
};

pub const FOCUS_MARK: Key = Key {
    name: "focus-mark",
    short: None,
    kind: Kind::Choice(&["auto", "always", "off"]),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The older name of `osd` and `osd-timeout`. vtium reads it only where `osd` or `osd-timeout` is not set: `auto` reads as `osd = minimal`, `always` as `osd = minimal` with `osd-timeout = never`, and `off` as `osd = off`. vtium logs a line naming the new keys when a file or the command line sets it.",
    cite: "ADR-1401 D5; the operator on vtium/vtium#156, renamed on vtium/vtium#169",
};

pub const NOTICE_FOREGROUND: Key = Key {
    name: "notice-foreground",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([31, 41, 51]),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The color of a notice's text. A notice tells of a problem, such as an invalid head file, and shows whatever `osd` says.",
    cite: "ADR-1401 D5",
};

pub const NOTICE_BACKGROUND: Key = Key {
    name: "notice-background",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([254, 226, 226]),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The color of a notice's box. While a notice shows, the on-screen display's full line shares its box and takes the notice's colors.",
    cite: "ADR-1401 D5",
};

pub const NOTICE_BORDER: Key = Key {
    name: "notice-border",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([185, 28, 28]),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The color of the edge around a notice's box.",
    cite: "ADR-1401 D5",
};

pub const OSD_FOREGROUND: Key = Key {
    name: "osd-foreground",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([229, 231, 235]),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The color of the on-screen display's text: the words of `osd = full`'s line, and the dot of `osd = minimal`.",
    cite: "ADR-1401 D5",
};

pub const OSD_BACKGROUND: Key = Key {
    name: "osd-background",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([31, 41, 51]),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The color of the on-screen display's box behind `osd = full`'s line, and of the cell behind `osd = minimal`'s dot.",
    cite: "ADR-1401 D5",
};

pub const OSD_BORDER: Key = Key {
    name: "osd-border",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([97, 110, 124]),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The color of the edge around `osd = full`'s box. The dot of `osd = minimal` has no edge.",
    cite: "ADR-1401 D5",
};

/// The six keys of the notices' and the on-screen display's colors, each
/// row a text, a box and its edge (ADR-1401 D5).
pub const OVERLAY_COLORS: [[&Key; 3]; 2] = [
    [&NOTICE_FOREGROUND, &NOTICE_BACKGROUND, &NOTICE_BORDER],
    [&OSD_FOREGROUND, &OSD_BACKGROUND, &OSD_BORDER],
];

/// Another set of values for [`OVERLAY_COLORS`], which the example
/// configuration carries commented out (ADR-1401 D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scheme {
    pub name: &'static str,
    /// What it looks like and who it is for, one sentence.
    pub what: &'static str,
    /// The notice's text, box and edge.
    pub notice: [[u8; 3]; 3],
    /// The on-screen display's text, box and edge.
    pub osd: [[u8; 3]; 3],
}

/// The other schemes, in the order the example lists them.
pub const SCHEMES: [Scheme; 3] = [
    Scheme {
        name: "light",
        what: "Both boxes are light, for a light terminal background: a notice is pale red with dark red text and a red edge, and the on-screen display pale grey with dark text and a grey edge.",
        notice: [[69, 10, 10], [254, 202, 202], [185, 28, 28]],
        osd: [[31, 41, 51], [243, 244, 246], [156, 163, 175]],
    },
    Scheme {
        name: "dark",
        what: "Both boxes are dark, for a dark terminal background: a notice is deep red with pale text and a light red edge, and the on-screen display near black with light grey text and a grey edge.",
        notice: [[254, 226, 226], [127, 29, 29], [248, 113, 113]],
        osd: [[209, 213, 219], [17, 24, 39], [75, 85, 99]],
    },
    Scheme {
        name: "color-blind-safe",
        what: "For color blindness, a problem differs from a status in brightness and in an orange and blue pair, never in red against green: a notice is a bright orange box with near-black text, and the on-screen display a dark blue box with near-white text.",
        notice: [[28, 25, 23], [255, 176, 0], [254, 97, 0]],
        osd: [[238, 242, 255], [11, 42, 91], [100, 143, 255]],
    },
];

pub const CONSOLES_MAX: Key = Key {
    name: "consoles-max",
    short: None,
    kind: Kind::Number {
        min: 1,
        max: vtium_config::CONSOLES_MAX as u64,
        unit: "consoles",
        word: None,
    },
    default: Default::Number(8),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The most consoles every head holds together. Each head keeps its first console; the others open, in the head file's order, while the total is below this.",
    cite: "ADR-1401 D10; the C reference's `session-max` (src/config.c:1357)",
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

pub const SB_MEMORY: Key = Key {
    name: "sb-memory",
    short: None,
    kind: Kind::Number {
        min: SB_MEMORY_MIN as u64,
        max: SB_MEMORY_MAX as u64,
        unit: "bytes",
        word: None,
    },
    default: Default::Number(SB_MEMORY_DEFAULT as u64),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "The most bytes the scrollback holds, or KiB, MiB or GiB with `K`, `M` or `G` (`6M`), at most a quarter of the machine's memory, or the default when `/proc/meminfo` cannot be read; its oldest lines go past it, as past `sb-size`. The default keeps the worst scrollback a program can write inside the 16 MiB minspec peak.",
    cite: "vtium/vtium#131; ADR-1001, \"Memory\"",
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

pub const BLINK_TEXT: Key = Key {
    name: "blink-text",
    short: None,
    kind: Kind::Choice(&["full", "partial", "off"]),
    default: Default::Text("full"),
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "How text with SGR 5 or 6 draws in the blink's off phase: `full` hides it, `partial` draws it halfway to its background as SGR 2 does, and `off` draws it steady. The cursor blinks as `blink` says either way.",
    cite: "src/render/bbulk.c:1521; the operator on vtium/vtium#81",
};

pub const BLINK_DECORATIONS: Key = Key {
    name: "blink-decorations",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "The underline and strike of blinking text blink with it: hidden in the off phase with `blink-text=full`, dimmed with `partial`. Off, they stay while the text blinks.",
    cite: "src/render/bbulk.c:1521; the operator on vtium/vtium#81",
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
    what: "Turn the displays off after this long with no key press or pointer action (a move, a button or the wheel); 0 never does.",
    cite: "src/config.c:1388",
};

pub const PROGRAM_BLANK: Key = Key {
    name: "program-blank",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Let a program set the blank's timeout with the Linux console's `CSI 9 ; n ]` (n minutes, at most 60; 0 never blanks) and wake the displays with `CSI 13 ]`, as `setterm --blank` does under `TERM=linux`; RIS returns the timeout to `dpms-timeout`. Off, the sequences are ignored. A key press or pointer action wakes the displays either way.",
    cite: "ADR-1000 (`ctl.linux-blank`, `ctl.linux-unblank`); vtium/vtium#79",
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
    what: "At start, how long to wait for a display, counted from when the VT is first shown; with none by then vtium exits with status 1, so the unit's `OnFailure=` can start a getty. 0 waits for no display to come: a display set up when the VT is first shown counts once its first frame is on screen. `forever` waits for the first display however long it takes, for a machine whose only display is a USB adapter (DisplayLink, `udl`) plugged in at any time. Once a display has come, vtium never exits for want of one.",
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
    what: "The sixteen colors and the default foreground and background: a named palette, or `custom`, which takes the `palette-*` keys. Colors 16 to 255 are xterm's in every palette.",
    cite: "src/config.c:1427; libtsm src/tsm/tsm-vte.c:254-478",
};

pub const PALETTE_BLACK: Key = Key {
    name: "palette-black",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 0, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 0 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_RED: Key = Key {
    name: "palette-red",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([205, 0, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 1 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_GREEN: Key = Key {
    name: "palette-green",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 205, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 2 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_YELLOW: Key = Key {
    name: "palette-yellow",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([205, 205, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 3 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_BLUE: Key = Key {
    name: "palette-blue",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 0, 238]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 4 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_MAGENTA: Key = Key {
    name: "palette-magenta",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([205, 0, 205]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 5 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_CYAN: Key = Key {
    name: "palette-cyan",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 205, 205]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 6 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_GREY: Key = Key {
    name: "palette-light-grey",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([229, 229, 229]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 7 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_DARK_GREY: Key = Key {
    name: "palette-dark-grey",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([127, 127, 127]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 8 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_RED: Key = Key {
    name: "palette-light-red",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 0, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 9 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_GREEN: Key = Key {
    name: "palette-light-green",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 255, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 10 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_YELLOW: Key = Key {
    name: "palette-light-yellow",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 255, 0]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 11 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_BLUE: Key = Key {
    name: "palette-light-blue",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([92, 92, 255]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 12 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_MAGENTA: Key = Key {
    name: "palette-light-magenta",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 0, 255]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 13 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_LIGHT_CYAN: Key = Key {
    name: "palette-light-cyan",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 255, 255]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 14 with `palette=custom`.",
    cite: "src/config.c:1305-1324, 1428-1448",
};

pub const PALETTE_WHITE: Key = Key {
    name: "palette-white",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 255, 255]),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Color 15 with `palette=custom`.",
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

pub const UNDERLINE_COLOR: Key = Key {
    name: "underline-color",
    short: None,
    kind: Kind::DecoColor,
    default: Default::Text("fg"),
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "The color every underline draws in, SGR 4 in each of its styles and SGR 21: `fg` draws it in the text's color, a palette index in that palette color, which follows a program's OSC 4, and `#rrggbb` in that color. A color a program sets with SGR 58 wins unless `force-decoration-colors` is on.",
    cite: "src/config.c:263-265, 868-933, 1453-1454",
};

pub const STRIKE_COLOR: Key = Key {
    name: "strike-color",
    short: None,
    kind: Kind::DecoColor,
    default: Default::Text("fg"),
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "The color the strike, SGR 9, draws in, as `underline-color` takes it.",
    cite: "src/config.c:266-267, 1455-1456",
};

pub const FORCE_DECORATION_COLORS: Key = Key {
    name: "force-decoration-colors",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "`underline-color` wins over the underline color a program sets with SGR 58.",
    cite: "src/config.c:268-270, 1457-1458",
};

pub const CLIPBOARD_WRITE: Key = Key {
    name: "clipboard-write",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Take the selection a program sets with OSC 52; off, it is dropped. The console has no system clipboard: vtium holds the last selection taken, as it holds one the user copies, and the middle button and `grab-paste` paste it.",
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

pub const POINTER_CHAR: Key = Key {
    name: "pointer-char",
    short: None,
    kind: Kind::Char,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "Draw the pointer as this character's glyph from the font, one cell wide, on the cell under the pointer, white with a dark outline as the arrow is. Unset, or a character the font lacks or whose glyph is blank, the pointer is the arrow.",
    cite: "the operator on vtium/vtium#102",
};

pub const SOFT_CURSOR: Key = Key {
    name: "soft-cursor",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Draw the pointer in software on every display, never on a display's cursor plane.",
    cite: "src/config.c:151-152, 1387; src/terminal.c:1269-1270",
};

pub const POINTER_HIDE_TIMEOUT: Key = Key {
    name: "pointer-hide-timeout",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: 86_400,
        unit: "s",
        word: None,
    },
    default: Default::Number(20),
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "Hide the pointer after this long with no pointer event, until it next moves; 0 never hides it.",
    cite: "src/input/input_pointer.c:9-18, the C reference's fixed 20 s; the operator on vtium/vtium#102",
};

pub const POINTER_HIDE_WHILE_TYPING: Key = Key {
    name: "pointer-hide-while-typing",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Hide the pointer at each key sent to the program, until it next moves. A key vtium takes, such as a grab, leaves it.",
    cite: "the operator on vtium/vtium#102",
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
    what: "The font: a frit's name, `NAME.vtfrit` in `/etc/vtium/fonts`, whose glyphs the helper generates at run time; a kbd font's name, or with `font-engine=psf` a PSF font's path, that no frit bears becomes a one-source frit. Unset, vconsole's font; with none, or a name that is neither, vtium draws its built-in fonts and logs the name and why.",
    cite: "src/config.c:1424; ADR-1200 (D9); vtium/vtium#126",
};

pub const FONT_SIZE: Key = Key {
    name: "font-size",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: CELL_SIDE_MAX as u64,
        unit: "pt",
        word: None,
    },
    default: Default::Number(0),
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "The font's size in points, 1/72 inch, drawn at points × DPI ÷ 72 pixels at the displays' density (`display-dpi`), and over a frit's `cell=`; 0 picks one from the display's size.",
    cite: "src/config.c:1423; vtium-font's `CELL_SIDE_MAX`; the operator on vtium/vtium#145",
};

pub const ALLOW_FONT_OPS: Key = Key {
    name: "allow-font-ops",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Let a program switch the font among the frits and the built-in fonts with xterm's OSC 50 (`OSC 50 ; NAME ST`, as `vtium-vitrify set NAME` sends it) and ask which is in use (`OSC 50 ; ? ST`). On by default, as xterm's `allowFontOps` is, since OSC 50 reaches only the frits root wrote and the built-in fonts, never a file. A switch changes the live console only; `font-name` decides the font at the next start. Off, vtium ignores OSC 50, the query included.",
    cite: "ADR-1200 (D5); vtium/vtium#126",
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

pub const DISPLAY_DEPTH: Key = Key {
    name: "display-depth",
    short: None,
    kind: Kind::Choice(&["auto", "1", "gray4", "gray16", "color"]),
    default: Default::Text("auto"),
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "Each display's depth. `1` draws it in black and white, each cell lit or unlit by its colors, never by their brightness; `gray4` and `gray16` record a gray panel of 4 or 16 levels, drawn as color, which the panel converts; `color` draws its colors; `auto` takes the levels the display says it shows: an fbdev device at 1 bit, or a DRM panel whose driver vtium-platform lists.",
    cite: "the operator on vtium/vtium#135",
};

pub const DISPLAY_DPI: Key = Key {
    name: "display-dpi",
    short: None,
    // vtium-platform's DPI_MIN and DPI_MAX, the densities it believes
    kind: Kind::Number {
        min: 20,
        max: 1000,
        unit: "dpi",
        word: None,
    },
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Each display's density, which converts `font-size` and a frit's `size=` from points to pixels, over the density a display measures. Unset, the density is measured from the physical size the display reports, else assumed at 96 with one logged warning per display.",
    cite: "the operator on vtium/vtium#145",
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

pub const GRAB_PASTE: Key = Key {
    name: "grab-paste",
    short: None,
    kind: Kind::Grab,
    default: Default::Text("<Shift>Insert"),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Paste the selection buffer to the program, once a press, as the middle button does. A chord with Shift names the keysym Shift makes: `<Ctrl><Shift>V`, not `<Ctrl><Shift>v`. Empty, no key pastes.",
    cite: "xterm's `Shift <KeyPress> Insert` translation; vtium/vtium#95",
};

pub const GRAB_REBOOT: Key = Key {
    name: "grab-reboot",
    short: None,
    kind: Kind::Grab,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Reboot the machine. With `/proc/sys/kernel/ctrl-alt-del` at 0, init is asked, as for Ctrl+Alt+Del, so the services stop first; otherwise the kernel restarts at once. Unset, no key reboots.",
    cite: "src/config.c:1409; src/seat.c:479-513, 631-636",
};

pub const GRAB_POWER_OFF: Key = Key {
    name: "grab-power-off",
    short: None,
    kind: Kind::Grab,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Power the machine off: init is asked with `SIGRTMIN+4`, which systemd takes as `poweroff.target`, so the services stop and the filesystems are unmounted first. Unset, no key powers off.",
    cite: "vtium/vtium#21",
};

pub const GRAB_SUSPEND: Key = Key {
    name: "grab-suspend",
    short: None,
    kind: Kind::Grab,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Suspend the machine to RAM: logind is asked with its `Suspend` method, so its inhibitors hold the sleep; where logind is not running, or vtium is built without `logind`, `mem` is written to `/sys/power/state`. Unset, no key suspends.",
    cite: "vtium/vtium#21",
};

/// Every key, in the order the generated files list them.
pub const KEYS: [Key; 90] = [
    CONFIGDIR,
    HELP,
    VERSION,
    VT,
    SWITCHVT,
    LOGIN,
    ONESHOT,
    HEAD_CONSOLES_MAX,
    CONSOLES_MAX,
    OSD,
    OSD_TIMEOUT,
    FOCUS_MARK,
    NOTICE_FOREGROUND,
    NOTICE_BACKGROUND,
    NOTICE_BORDER,
    OSD_FOREGROUND,
    OSD_BACKGROUND,
    OSD_BORDER,
    TERM,
    RESET_ENV,
    BACKSPACE_DELETE,
    SB_SIZE,
    SB_SIZE_CAP,
    SB_MEMORY,
    BLINK,
    BLINK_TEXT,
    BLINK_DECORATIONS,
    SYNC_TIMEOUT,
    DPMS_TIMEOUT,
    PROGRAM_BLANK,
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
    UNDERLINE_COLOR,
    STRIKE_COLOR,
    FORCE_DECORATION_COLORS,
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
    SOFT_CURSOR,
    POINTER_CHAR,
    POINTER_HIDE_TIMEOUT,
    POINTER_HIDE_WHILE_TYPING,
    GRAB_PASTE,
    GRAB_REBOOT,
    GRAB_POWER_OFF,
    GRAB_SUSPEND,
    FONT_ENGINE,
    FONT_NAME,
    FONT_SIZE,
    ALLOW_FONT_OPS,
    GLYPH_SOURCE,
    GLYPH_SOURCE_BOX,
    GLYPH_SOURCE_BLOCK,
    GLYPH_SOURCE_BRAILLE,
    GLYPH_SOURCE_SEXTANTS,
    GLYPH_SOURCE_OCTANTS,
    GLYPH_SOURCE_LEGACY,
    MISSING,
    GLYPH_CACHE,
    DISPLAY_DEPTH,
    DISPLAY_DPI,
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
