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
    /// A whole number from `min` to `max`, in `unit`.
    Number {
        min: u64,
        max: u64,
        unit: &'static str,
    },
    /// Text of at most [`TEXT_MAX`] bytes with no control character.
    Text,
    /// One of a fixed set of words.
    Choice(&'static [&'static str]),
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
    },
    default: Default::Number(600),
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "Turn the displays off after this long with no key or pointer event; 0 never does.",
    cite: "src/config.c:1388",
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
    },
    default: Default::Number(0),
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "The font's height; 0 picks one from the display's size.",
    cite: "src/config.c:1423; vtium-font's `CELL_SIDE_MAX`",
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
pub const KEYS: [Key; 22] = [
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
    HWACCEL,
    USE_ORIGINAL_MODE,
    FONT_ENGINE,
    FONT_NAME,
    FONT_SIZE,
    GLYPH_CACHE,
    RENDER_TIMER,
];

/// The key named `name`.
pub fn find(name: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.name == name)
}

/// The key with the one-letter option `short`.
pub fn find_short(short: char) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.short == Some(short))
}
