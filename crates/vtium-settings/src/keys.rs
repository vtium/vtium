//! The key table: every configuration key and command-line option vtium
//! reads, with its type, default, bound and the crate that reads it.
//!
//! The console reads its configuration by this table, and vtium-config
//! shows and edits vtium.conf by it (ADR-1201 D1), so the two read one
//! table. `docs/config.md` and the installed example,
//! `crates/vtium/install/vtium.conf.example`, are generated from [`KEYS`]
//! and [`SECTIONS`], which sorts the keys into sections and groups;
//! the vtium crate's test `generated_files_match_the_table` fails when
//! either differs, and `VTIUM_BLESS=1` rewrites them.

use vtium_font::CELL_SIDE_MAX;
use vtium_term::{SB_MEMORY_DEFAULT, SB_MEMORY_MAX, SB_MEMORY_MIN, SB_SIZE_DEFAULT, SB_SIZE_MAX};

use crate::{CONSOLES_MAX as CONSOLES_BOUND, CONSOLES_TOTAL_DEFAULT, HEAD_CONSOLES_DEFAULT};

/// The frits' directory vtium is built with, `font-frits`'s default: set
/// at build time with `VTIUM_FONT_FRITS`. The vtium crate's
/// `font::FRITS` is this.
pub const FRITS: &str = match option_env!("VTIUM_FONT_FRITS") {
    Some(dir) => dir,
    None => "/etc/vtium/fonts",
};

/// The glyph helper's program vtium is built with, `font-helper`'s
/// default: set at build time with `VTIUM_FONT_HELPER`. The vtium crate's
/// `helper::PROGRAM` is this.
pub const HELPER_PROGRAM: &str = match option_env!("VTIUM_FONT_HELPER") {
    Some(path) => path,
    None => "/usr/bin/vtium-config",
};

/// The image helper's program vtium is built with, `image-helper`'s
/// default: set at build time with `VTIUM_IMAGE`. The vtium crate's
/// `head::images::PROGRAM` is this.
pub const IMAGE_PROGRAM: &str = match option_env!("VTIUM_IMAGE") {
    Some(path) => path,
    None => "/usr/bin/vtium-image",
};

/// `hwaccel`'s default, which the build sets with `VTIUM_HWACCEL`: off
/// unless it names `on` (ADR-1500 D8). The `vtium` packages build it on.
pub const HWACCEL_DEFAULT: bool = build_hwaccel(option_env!("VTIUM_HWACCEL"));

/// The buffers each display keeps that the build names with
/// `VTIUM_DISPLAY_BUFFERS`, 1, 2 or 3, and `display-buffers`' default
/// (ADR-1500 D8). `None` when the build names none: the key is then unset,
/// and the console keeps three buffers in a build with features on top of
/// minspec and two in the minspec build (ADR-1401 D2). The vtium crate's
/// `video::BUILD_BUFFERS` takes this count first.
pub const DISPLAY_BUFFERS_BUILD: Option<u8> =
    build_display_buffers(option_env!("VTIUM_DISPLAY_BUFFERS"));

/// `hwaccel`'s default from the build's `VTIUM_HWACCEL`: `on` or `off`,
/// off when the build names none. Any other value stops the build, which
/// evaluates this while it compiles the table.
#[expect(
    clippy::panic,
    reason = "evaluated only to compile the key table, where a panic stops the build with its message"
)]
const fn build_hwaccel(value: Option<&str>) -> bool {
    match value {
        None => false,
        Some(v) => match v.as_bytes() {
            b"on" => true,
            b"off" => false,
            _ => panic!("VTIUM_HWACCEL is on or off"),
        },
    }
}

/// The count the build's `VTIUM_DISPLAY_BUFFERS` names: `1`, `2` or `3`,
/// and `None` when the build names none. Any other value stops the build.
#[expect(
    clippy::panic,
    reason = "evaluated only to compile the key table, where a panic stops the build with its message"
)]
const fn build_display_buffers(value: Option<&str>) -> Option<u8> {
    match value {
        None => None,
        Some(v) => match v.as_bytes() {
            b"1" => Some(1),
            b"2" => Some(2),
            b"3" => Some(3),
            _ => panic!("VTIUM_DISPLAY_BUFFERS is 1, 2 or 3"),
        },
    }
}

/// `display-buffers`' default for a count the build names, unset for
/// none.
const fn buffers_default(count: Option<u8>) -> Default {
    match count {
        Some(1) => Default::Text("1"),
        Some(2) => Default::Text("2"),
        Some(3) => Default::Text("3"),
        _ => Default::Unset,
    }
}

/// The terminal type vtium sets, `term`'s default, the name of its own
/// terminfo entry. The vtium crate's `termcap::NAME` is this.
pub const TERM_NAME: &str = "xterm-vtium";

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
    /// commas, as the console's grab parser reads them. Empty, it takes none.
    Grab,
    /// One character, no control character or space.
    Char,
    /// A size in pixels, `WxH`, each side from 1 to `max`.
    Size { max: u32 },
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
    /// What it does. The first paragraph is one sentence saying what the
    /// key does; further paragraphs give its values and details. A blank
    /// line, `\n\n`, separates two paragraphs, and the text holds no other
    /// newline.
    pub what: &'static str,
    /// Where it comes from: the C reference's lines or the decision.
    pub cite: &'static str,
}

impl Key {
    /// The default the source tree gives this key, before a build setting
    /// changes it (ADR-1500 D8): `hwaccel` off and `display-buffers`
    /// unset, whatever `VTIUM_HWACCEL` and `VTIUM_DISPLAY_BUFFERS` name.
    /// `docs/config.md` and the example configuration show this default,
    /// so they read the same from every build; `default` is this build's.
    pub fn tree_default(&self) -> Default {
        if self.name == HWACCEL.name {
            Default::Switch(build_hwaccel(None))
        } else if self.name == DISPLAY_BUFFERS.name {
            buffers_default(build_display_buffers(None))
        } else {
            self.default
        }
    }

    /// The paragraphs of [`Key::what`].
    pub fn paragraphs(&self) -> impl Iterator<Item = &'static str> {
        self.what.split("\n\n")
    }

    /// [`Key::what`] on one line, its paragraphs joined by a space, for a
    /// reader that shows the text on one line or wraps it itself.
    pub fn what_line(&self) -> String {
        self.paragraphs().collect::<Vec<_>>().join(" ")
    }
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
    default: Default::Text("/etc/vtium"),
    scope: Scope::CommandLine,
    reader: "vtium",
    what: "The directory holding `vtium.conf` and `<seat>.seat.conf`.",
    cite: "src/config.c:1349, ADR-1500 D4",
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
    what: "Print three lines and exit: the version, the revision the build was made from, and the features it was built with on top of minspec, or none.",
    cite: "src/config.c:1343",
};

pub const DEBUG: Key = Key {
    name: "debug",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Log debug messages as well, to find what went wrong. Off, vtium logs its information messages, warnings and errors.",
    cite: "src/config.c:1346, 1515-1539",
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
    what: "The session's command.\n\n\
           Unset, `/bin/login -p` with `reset-env` on and `/bin/login` with it off. A word that is `$NAME` becomes that variable of the session's environment, and `$$` starts a word with `$`.",
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
        max: CONSOLES_BOUND as u64,
        unit: "consoles",
        word: None,
    },
    default: Default::Number(HEAD_CONSOLES_DEFAULT as u64),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The most consoles one head holds: the head file's `consoles` opens no more.",
    cite: "ADR-1401 D10; the C reference's `session-max` (src/config.c:1357) bounded the seat's sessions",
};

pub const OSD: Key = Key {
    name: "osd",
    short: None,
    kind: Kind::Choice(&["off", "minimal", "full"]),
    default: Default::Text("minimal"),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "What vtium's on-screen display draws at the top of each head after a change.\n\n\
           With `minimal`, it draws a dot in the top-right cell, filled on the active head and hollow on the others. With `full`, it draws a line across the top, such as `● desk · console 2 of 3`. With `off`, it draws nothing.\n\n\
           A notice of a problem shows whatever `osd` says.",
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
    what: "The older name of `osd` and `osd-timeout`.\n\n\
           vtium reads it only where `osd` or `osd-timeout` is not set: `auto` reads as `osd = minimal`, `always` as `osd = minimal` with `osd-timeout = never`, and `off` as `osd = off`. vtium logs a line naming the new keys when a file or the command line sets it.",
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

/// The example's scheme for a light terminal background.
pub const LIGHT: Scheme = Scheme {
    name: "light",
    what: "For a light terminal background: the on-screen display is a pale grey box with dark text and a grey edge, and a notice a dark red box with white text and a darker edge, so a problem differs from a status in brightness as well as in hue.",
    notice: [[255, 255, 255], [153, 27, 27], [69, 10, 10]],
    osd: [[31, 41, 51], [243, 244, 246], [156, 163, 175]],
};

/// The example's scheme for a dark terminal background.
pub const DARK: Scheme = Scheme {
    name: "dark",
    what: "Both boxes are dark, for a dark terminal background: a notice is deep red with pale text and a light red edge, and the on-screen display near black with light grey text and a grey edge.",
    notice: [[254, 226, 226], [185, 28, 28], [248, 113, 113]],
    osd: [[209, 213, 219], [17, 24, 39], [75, 85, 99]],
};

/// The example's scheme for color blindness.
pub const COLOR_BLIND_SAFE: Scheme = Scheme {
    name: "color-blind-safe",
    what: "For color blindness, a problem differs from a status in brightness and in an orange and blue pair, never in red against green: a notice is a bright orange box with near-black text, and the on-screen display a dark blue box with near-white text.",
    notice: [[28, 25, 23], [255, 176, 0], [254, 97, 0]],
    osd: [[238, 242, 255], [11, 42, 91], [100, 143, 255]],
};

/// The other schemes, in the order the example lists them.
pub const SCHEMES: [Scheme; 3] = [LIGHT, DARK, COLOR_BLIND_SAFE];

pub const CONSOLES_MAX: Key = Key {
    name: "consoles-max",
    short: None,
    kind: Kind::Number {
        min: 1,
        max: CONSOLES_BOUND as u64,
        unit: "consoles",
        word: None,
    },
    default: Default::Number(CONSOLES_TOTAL_DEFAULT as u64),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The most consoles every head holds together.\n\n\
           Each head keeps its first console; the others open, in the head file's order, while the total is below this.",
    cite: "ADR-1401 D10; the C reference's `session-max` (src/config.c:1357)",
};

pub const TERM: Key = Key {
    name: "term",
    short: Some('t'),
    kind: Kind::Text,
    default: Default::Text(TERM_NAME),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "`TERM` in the session's environment.\n\n\
           The default names vtium's own terminfo entry, and XTGETTCAP answers from that entry's capabilities; `kmscon` names the C reference's entry, and XTGETTCAP answers from the C reference's. Under any other name every name XTGETTCAP asks for is unknown.",
    cite: "src/config.c:1368; meson.options:51-52; src/terminal_vte.c:36-56; ADR-1000, \"The terminfo entry and TERM\"",
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
    what: "The most lines `sb-size` may ask for.\n\n\
           Unset, it follows the machine's memory: 10,000 lines below 2 GiB, 30,000 from 2 GiB and 100,000 from 4 GiB.",
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
    what: "The most bytes the scrollback holds: its oldest lines go past it, as past `sb-size`.\n\n\
           It takes bytes, or KiB, MiB or GiB with `K`, `M` or `G` (`6M`), at most a quarter of the machine's memory, or at most the default when `/proc/meminfo` cannot be read.\n\n\
           Left unset, the default follows each console's grid, worked out again when the grid changes: on a grid up to minspec's 170 by 48 it is what the 16 MiB minspec peak leaves after the console's two screens at their worst and the rest of vtium, so a smaller display keeps more, and a larger display keeps the 170 by 48 value shown here.\n\n\
           The default does not grow with the display or the machine's memory; on a machine that can spare more, set `sb-memory` for a deeper scrollback, and it stays fixed.",
    cite: "vtium/vtium#131; ADR-1001, \"Memory\"",
};

pub const BELL: Key = Key {
    name: "bell",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Rings the VT's bell when a program sends BEL.\n\n\
           A program stops its own BELs ringing by resetting mode 1042, which starts set. Off, no BEL rings.",
    cite: "src/config.c:125-126, 1372; src/terminal.c:872-879",
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
    default: Default::Text("partial"),
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "How text with SGR 5 or 6 draws in the blink's off phase.\n\n\
           `full` hides it, `partial` draws it halfway to its background as SGR 2 does, and `off` draws it steady. The cursor blinks as `blink` says either way.",
    cite: "src/render/bbulk.c:1521, which hid the glyph; the operator on vtium/vtium#81; the maintainer's decision of 2026-11-03",
};

pub const BLINK_DECORATIONS: Key = Key {
    name: "blink-decorations",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "The underline and strike of blinking text blink with it.\n\n\
           They hide in the off phase with `blink-text=full`, and dim with `partial`. Off, they stay while the text blinks.",
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
    what: "Let a program set the blank's timeout and wake the displays, as `setterm --blank` does under `TERM=linux`.\n\n\
           A program sets the timeout with the Linux console's `CSI 9 ; n ]` (n minutes, at most 60; 0 never blanks) and wakes the displays with `CSI 13 ]`. RIS returns the timeout to `dpms-timeout`. Off, the sequences are ignored. A key press or pointer action wakes the displays either way.",
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
    what: "At start, how long to wait for a display, counted from when the VT is shown.\n\n\
           With no display by then, vtium exits with status 1, so the unit's `OnFailure=` can start a getty. Under a head file whose policy is closed vtium waits however long it takes, as under `forever` (ADR-1401 D8).\n\n\
           The wait runs only while the VT is vtium's. When another program takes the VT before vtium's first frame is on screen, as X starting on another VT does at boot, vtium waits for the VT to come back, and when it does, the wait starts again from the full `display-wait`.\n\n\
           0 waits for no display to come: a display set up when the VT is shown counts once its first frame is on screen. `forever` waits for the first display however long it takes, for a machine whose only display is a USB adapter (DisplayLink, `udl`) plugged in at any time.\n\n\
           Once a display has come, vtium never exits for want of one.",
    cite: "the operator on vtium/vtium#21 and #270",
};

pub const DISPLAY_BUFFERS: Key = Key {
    name: "display-buffers",
    short: None,
    kind: Kind::Choice(&["1", "2", "3"]),
    default: buffers_default(DISPLAY_BUFFERS_BUILD),
    scope: Scope::Anywhere,
    reader: "vtium-platform",
    what: "How many buffers each DRM display keeps, the same for every display.\n\n\
           `3` is triple buffering, so no frame waits for the last flip, at the cost of one more whole frame a display (8 MB at 1080p). `2` is double buffering. `1` is single buffering, which holds the least memory, but vtium then draws into the buffer being shown, so a frame can be seen half drawn.\n\n\
           A framebuffer (fbdev) keeps one buffer whatever this key says. Unset, the build decides: the count it was built with (`VTIUM_DISPLAY_BUFFERS`), or else 3 in a build with features on top of minspec, 2 in the minspec build, 1 only in a build smaller than minspec. `vtium config display-buffers` shows the build's default.",
    cite: "ADR-1401 D2; the C reference's drm2d kept 2, src/video/drm2d_internal.h:49",
};

pub const HWACCEL: Key = Key {
    name: "hwaccel",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(HWACCEL_DEFAULT),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Draw through the GPU, in a build that has it.\n\n\
           Off unless vtium was built to turn it on (`VTIUM_HWACCEL`). `vtium config hwaccel` shows the build's default.",
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
    what: "A display's mode.\n\n\
           `WxH[@RATE]` sets it for every display, `[CONNECTOR]WxH` for one connector, and `GPU[CONNECTOR]WxH` for one connector on one card, the card named by its udev `ID_PATH`. A key per line; the most specific match wins. Unset, the panel's preferred mode. It turns `use-original-mode` off.",
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
    what: "The sixteen colors and the default foreground and background.\n\n\
           Its value is a named palette, or `custom`, which takes the colors from the `palette-*` keys. Colors 16 to 255 are xterm's in every palette.",
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
    what: "The color every underline draws in, SGR 4 in each of its styles and SGR 21.\n\n\
           `fg` draws it in the text's color, a palette index in that palette color, which follows a program's OSC 4, and `#rrggbb` in that color. A color a program sets with SGR 58 wins unless `force-decoration-colors` is on.",
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
    what: "Take the selection a program sets with OSC 52; off, it is dropped.\n\n\
           The console has no system clipboard: vtium holds the last selection taken, as it holds one the user copies, and the middle button and `grab-paste` paste it.",
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

pub const NATURAL_SCROLLING: Key = Key {
    name: "natural-scrolling",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Invert the mouse wheel.\n\n\
           The wheel scrolls the view and reports to a program the other way, and so do the cursor keys it sends on the alternate screen under mode 1007. Ctrl and the wheel still zoom in when the wheel turns up.",
    cite: "src/config.c:149-150, 1386; src/terminal.c:1474-1476, 1646",
};

pub const POINTER_CHAR: Key = Key {
    name: "pointer-char",
    short: None,
    kind: Kind::Char,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-render",
    what: "Draw the pointer as this character's glyph from the font.\n\n\
           The glyph draws one cell wide, on the cell under the pointer, white with a dark outline as the arrow is. Unset, or a character the font lacks or whose glyph is blank, the pointer is the arrow.",
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
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "The font vtium draws text with.\n\n\
           It is a frit's name, `NAME.vtfrit` in `/etc/vtium/fonts`, whose glyphs the helper generates at run time. A kbd font's name, or with `font-engine=psf` a PSF font's path, that no frit bears becomes a one-source frit.\n\n\
           Unset, vconsole's font; with none, or a name that is neither, vtium draws its built-in fonts and logs the name and why.",
    cite: "src/config.c:1424; ADR-1200 (D9); vtium/vtium#126",
};

pub const FONT_FRITS: Key = Key {
    name: "font-frits",
    short: None,
    kind: Kind::Text,
    default: Default::Text(FRITS),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The directory of the frits, the `NAME.vtfrit` files that `font-name` and a head's profile name. The default is the directory vtium was built with.",
    cite: "ADR-1200 (D1, D15); the maintainer's decision of 2026-11-01",
};

pub const FONT_HELPER: Key = Key {
    name: "font-helper",
    short: None,
    kind: Kind::Text,
    default: Default::Text(HELPER_PROGRAM),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The glyph helper's program, which vtium starts to draw a frit's glyphs, as `nobody` when vtium runs as root.\n\n\
           vtium opens it once at start and starts every helper from that file, so a helper installed later reaches a console when the console starts again.\n\n\
           vtium refuses a path that is not absolute, a file that is not a regular file, a setuid or setgid file, a file owned by an account other than root or the account vtium runs as, and a file its group or others may write; it logs why and draws its built-in font. It starts the helper through `/proc/self/fd`, so `/proc` must be mounted.\n\n\
           The default is the path vtium was built with.",
    cite: "ADR-1200 (D15); the maintainer's decision of 2026-11-01",
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
    what: "The font's size in points, 1/72 inch.\n\n\
           The font draws at points × DPI ÷ 72 pixels at the displays' density (`display-dpi`), and the size wins over a frit's `cell=`. 0 picks a size from the display's size.",
    cite: "src/config.c:1423; vtium-font's `CELL_SIDE_MAX`; the operator on vtium/vtium#145",
};

pub const FONT_HINTING: Key = Key {
    name: "font-hinting",
    short: None,
    kind: Kind::Choice(&["light", "full"]),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "How the font helper is to hint outline glyphs, over a frit's `hinting=`.\n\n\
           `light` fits each glyph to the pixel grid up and down only; `full` fits a TrueType font both ways, and draws a CFF font as `light` does. Bitmap fonts take no hinting.\n\n\
           vtium does not yet hand this key to the helper, so for now each frit's `hinting=` decides, `light` where it states none.",
    cite: "ADR-1200 D1; the C reference hinted every glyph lightly, src/font/font_freetype.c:119",
};

pub const FONT_LINE_SPACING: Key = Key {
    name: "font-line-spacing",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: 100,
        unit: "%",
        word: None,
    },
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "Height added to each row, over a frit's `line-spacing=`.\n\n\
           It is a whole percentage of the text's height, rounded to whole pixels and split evenly above and below the text, so the grid has fewer rows. It adds nothing to a frit's own `cell=`, but it applies when `font-size` sets the size over that `cell=`: the cell grows by the spacing, split evenly above and below each row. Unset, the frit's `line-spacing=` decides, 0 where it states none.\n\n\
           vtium does not build line spacing yet: it accepts the key, and the key has no effect for now.",
    cite: "ADR-1302 D13; ADR-1200 D1",
};

pub const ALLOW_FONT_OPS: Key = Key {
    name: "allow-font-ops",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Let a program switch the font among the frits and the built-in fonts with xterm's OSC 50, and ask which is in use.\n\n\
           A program switches with `OSC 50 ; NAME ST`, as `vtium-config set NAME` sends it, and asks with `OSC 50 ; ? ST`. A switch lasts until the session it came from ends, when vtium returns the console to the font and size `font-name` and `font-size` give.\n\n\
           It is on by default, as xterm's `allowFontOps` is, since OSC 50 reaches only the frits root wrote and the built-in fonts, never a file. Off, vtium ignores OSC 50, the query included.",
    cite: "ADR-1200 (D5); vtium/vtium#126",
};

pub const GLYPH_SOURCE: Key = Key {
    name: "glyph-source",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium-font",
    what: "Where the six sprite ranges draw from.\n\n\
           `font` draws the font's glyphs; `sprites`, vtium's; `auto`, the font's where it has one. Unset, a named font's glyphs for each range it covers whole and the sprites for the rest; the built-in unifont draws the sprites.",
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
    what: "Each display's depth.\n\n\
           `1` draws it in black and white, each cell lit or unlit by its colors, never by their brightness; `gray4` and `gray16` record a gray panel of 4 or 16 levels, drawn as color, which the panel converts; `color` draws its colors; `auto` takes the levels the display says it shows: an fbdev device at 1 bit, or a DRM panel whose driver vtium-platform lists.",
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
    what: "Each display's density, which converts `font-size` and a frit's `size=` from points to pixels, over the density a display measures.\n\n\
           Unset, the density is measured from the physical size the display reports, else assumed at 96 with one logged warning per display.",
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
    what: "Paste the selection buffer to the program, once a press, as the middle button does.\n\n\
           A chord with Shift names the keysym Shift makes: `<Ctrl><Shift>V`, not `<Ctrl><Shift>v`. Empty, no key pastes.",
    cite: "xterm's `Shift <KeyPress> Insert` translation; vtium/vtium#95",
};

pub const GRAB_REBOOT: Key = Key {
    name: "grab-reboot",
    short: None,
    kind: Kind::Grab,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Reboot the machine.\n\n\
           With `/proc/sys/kernel/ctrl-alt-del` at 0, vtium asks init, as for Ctrl+Alt+Del, so the services stop first; otherwise the kernel restarts at once. Unset, no key reboots.",
    cite: "src/config.c:1409; src/seat.c:479-513, 631-636",
};

pub const GRAB_POWER_OFF: Key = Key {
    name: "grab-power-off",
    short: None,
    kind: Kind::Grab,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Power the machine off.\n\n\
           vtium asks init with `SIGRTMIN+4`, which systemd takes as `poweroff.target`, so the services stop and the filesystems are unmounted first. Unset, no key powers off.",
    cite: "vtium/vtium#21",
};

pub const GRAB_SUSPEND: Key = Key {
    name: "grab-suspend",
    short: None,
    kind: Kind::Grab,
    default: Default::Unset,
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "Suspend the machine to RAM.\n\n\
           vtium asks logind with its `Suspend` method, so its inhibitors hold the sleep; where logind is not running, or vtium is built without `logind`, vtium writes `mem` to `/sys/power/state`. Unset, no key suspends.",
    cite: "vtium/vtium#21",
};

pub const IMAGE_STORAGE_LIMIT: Key = Key {
    name: "image-storage-limit",
    short: None,
    kind: Kind::Number {
        min: 0,
        max: 1 << 40,
        unit: "bytes",
        word: Some("auto"),
    },
    default: Default::Text("auto"),
    scope: Scope::Anywhere,
    reader: "vtium-image",
    what: "The most bytes each console's pictures hold, its main and alternate screens together.\n\n\
           It takes bytes, or KiB, MiB or GiB with `K`, `M` or `G` (`64M`), at most 1 TiB. `auto` gives each console four frames of its head's displays, 16 MiB to 192 MiB, and works it out again when a display joins or leaves the head or changes size. `0` turns sixel and kitty graphics off.\n\n\
           A build without images reads it and draws no picture.",
    cite: "ADR-1100 D5, D11; Ghostty's `image-storage-limit`; src/config.c:1023-1064, 1462",
};

pub const MAX_GRAPHIC_SIZE: Key = Key {
    name: "max-graphic-size",
    short: None,
    kind: Kind::Size { max: 65_535 },
    default: Default::Text("10000x10000"),
    scope: Scope::Anywhere,
    reader: "vtium-image",
    what: "The largest picture a program may send, in pixels, `WxH`, for sixel and kitty graphics alike; a larger one is refused.\n\n\
           A build without images reads it and draws no picture.",
    cite: "ADR-1100 D11; xterm's `maxGraphicSize`; src/config.c:1071-1107, 1467",
};

pub const NUM_COLOR_REGISTERS: Key = Key {
    name: "num-color-registers",
    short: None,
    kind: Kind::Number {
        min: 2,
        max: 65_535,
        unit: "registers",
        word: None,
    },
    default: Default::Number(1024),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "How many color registers a sixel picture has, and the most XTSMGRAPHICS lets a program set.\n\n\
           A build without images reads it and draws no picture.",
    cite: "ADR-1100 D11; xterm's `numColorRegisters`; src/config.c:1114-1145, 1469",
};

pub const PRIVATE_COLOR_REGISTERS: Key = Key {
    name: "private-color-registers",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Mode 1070's state at a reset.\n\n\
           On, each sixel picture starts from the default colors; off, the pictures share their color registers. A build without images reads it and draws no picture.",
    cite: "ADR-1100 D11; xterm's `privateColorRegisters`; src/config.c:1471",
};

pub const IMAGE_HELPER: Key = Key {
    name: "image-helper",
    short: None,
    kind: Kind::Text,
    default: Default::Text(IMAGE_PROGRAM),
    scope: Scope::Anywhere,
    reader: "vtium",
    what: "The image helper's program, which vtium starts as the session's user to read the files and shared memory a kitty graphics command names and to decode PNG and zlib-compressed pictures, since vtium itself, running as root, does neither.\n\n\
           vtium opens it once at start and starts every helper from that file, so a helper installed later reaches the consoles when vtium starts again.\n\n\
           vtium refuses a path that is not absolute, a file that is not a regular file, a setuid or setgid file, a file owned by an account other than root or the account vtium runs as, and a file its group or others may write; it logs why, and every console then answers those pictures itself, a file, a temporary file or shared memory with `EBADF` and PNG or zlib with `EINVAL`. It starts the helper through `/proc/self/fd`, so `/proc` must be mounted.\n\n\
           The default is the path vtium was built with. A build without images reads it and starts no helper.",
    cite: "ADR-1101 (D1); the maintainer's decision of 2026-11-01",
};

pub const SIXEL: Key = Key {
    name: "sixel",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "Sixel graphics.\n\n\
           Off, vtium neither offers nor decodes sixel: DA1 reports no attribute 4 and a sixel string is ignored. vtium offers sixel only while `image-storage-limit` is above 0 and every display of the console's head draws pictures. A build without images never offers it.",
    cite: "ADR-1100 D10, D11; src/config.c:1221-1234, 1464",
};

pub const KITTY_GRAPHICS: Key = Key {
    name: "kitty-graphics",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    reader: "vtium-term",
    what: "The kitty graphics protocol.\n\n\
           Off, vtium neither offers nor decodes it: a graphics command is ignored and a query gets no reply. vtium offers it only while `image-storage-limit` is above 0 and every display of the console's head draws pictures. A build without images never offers it.",
    cite: "ADR-1100 D10, D11; src/config.c:1236-1247, 1465",
};

/// Every key, in the order the generated files list them.
pub const KEYS: [Key; 105] = [
    CONFIGDIR,
    HELP,
    VERSION,
    DEBUG,
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
    BELL,
    BLINK,
    BLINK_TEXT,
    BLINK_DECORATIONS,
    SYNC_TIMEOUT,
    DPMS_TIMEOUT,
    PROGRAM_BLANK,
    DISPLAY_WAIT,
    DISPLAY_BUFFERS,
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
    NATURAL_SCROLLING,
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
    FONT_FRITS,
    FONT_HELPER,
    FONT_SIZE,
    FONT_HINTING,
    FONT_LINE_SPACING,
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
    IMAGE_STORAGE_LIMIT,
    SIXEL,
    KITTY_GRAPHICS,
    MAX_GRAPHIC_SIZE,
    NUM_COLOR_REGISTERS,
    PRIVATE_COLOR_REGISTERS,
    IMAGE_HELPER,
];

/// A group of the example vtium.conf: a heading, then one description that
/// covers each of its keys, then the keys' lines together. A group is one
/// key, or keys that belong together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Group {
    /// The heading, a few words.
    pub title: &'static str,
    pub keys: &'static [&'static Key],
}

/// A main section of the example vtium.conf and of `docs/config.md`: a
/// heading, a sentence saying what the section covers, then its groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    pub title: &'static str,
    /// What the section covers, one or two sentences.
    pub about: &'static str,
    pub groups: &'static [Group],
}

/// The sections, in the order a reader meets them: the settings people
/// change most come first, and those of vtium's own running last. Each key
/// is in one group of one section. The example leaves out the keys only
/// the command line takes, and a group or section left with none.
pub const SECTIONS: [Section; 10] = [
    Section {
        title: "Fonts and text",
        about: "The font vtium draws text with, its size, where box drawing and block characters come from, and how blinking text draws.",
        groups: &[
            Group {
                title: "Font",
                keys: &[&FONT_NAME, &FONT_FRITS, &FONT_ENGINE],
            },
            Group {
                title: "Font size and spacing",
                keys: &[&FONT_SIZE, &FONT_HINTING, &FONT_LINE_SPACING],
            },
            Group {
                title: "Box drawing, blocks and other sprites",
                keys: &[
                    &GLYPH_SOURCE,
                    &GLYPH_SOURCE_BOX,
                    &GLYPH_SOURCE_BLOCK,
                    &GLYPH_SOURCE_BRAILLE,
                    &GLYPH_SOURCE_SEXTANTS,
                    &GLYPH_SOURCE_OCTANTS,
                    &GLYPH_SOURCE_LEGACY,
                ],
            },
            Group {
                title: "Missing characters",
                keys: &[&MISSING],
            },
            Group {
                title: "Blinking",
                keys: &[&BLINK, &BLINK_TEXT, &BLINK_DECORATIONS],
            },
            Group {
                title: "Font switching by programs",
                keys: &[&ALLOW_FONT_OPS],
            },
        ],
    },
    Section {
        title: "Colors",
        about: "The sixteen colors programs draw with, the default foreground and background, and the colors of underlines and strikes.",
        groups: &[
            Group {
                title: "Palette",
                keys: &[
                    &PALETTE,
                    &PALETTE_BLACK,
                    &PALETTE_RED,
                    &PALETTE_GREEN,
                    &PALETTE_YELLOW,
                    &PALETTE_BLUE,
                    &PALETTE_MAGENTA,
                    &PALETTE_CYAN,
                    &PALETTE_LIGHT_GREY,
                    &PALETTE_DARK_GREY,
                    &PALETTE_LIGHT_RED,
                    &PALETTE_LIGHT_GREEN,
                    &PALETTE_LIGHT_YELLOW,
                    &PALETTE_LIGHT_BLUE,
                    &PALETTE_LIGHT_MAGENTA,
                    &PALETTE_LIGHT_CYAN,
                    &PALETTE_WHITE,
                    &PALETTE_FOREGROUND,
                    &PALETTE_BACKGROUND,
                ],
            },
            Group {
                title: "Underline and strike colors",
                keys: &[&UNDERLINE_COLOR, &STRIKE_COLOR, &FORCE_DECORATION_COLORS],
            },
        ],
    },
    Section {
        title: "Keyboard and pointer",
        about: "The keyboard's layout and repeat, the mouse and the pointer, and copying and pasting.",
        groups: &[
            Group {
                title: "Keyboard",
                keys: &[
                    &XKB_MODEL,
                    &XKB_LAYOUT,
                    &XKB_VARIANT,
                    &XKB_OPTIONS,
                    &XKB_KEYMAP,
                    &XKB_COMPOSE_FILE,
                    &XKB_REPEAT_DELAY,
                    &XKB_REPEAT_RATE,
                ],
            },
            Group {
                title: "Pointer",
                keys: &[
                    &MOUSE,
                    &NATURAL_SCROLLING,
                    &SOFT_CURSOR,
                    &POINTER_CHAR,
                    &POINTER_HIDE_TIMEOUT,
                    &POINTER_HIDE_WHILE_TYPING,
                ],
            },
            Group {
                title: "Clipboard and paste",
                keys: &[&CLIPBOARD_WRITE, &GRAB_PASTE],
            },
        ],
    },
    Section {
        title: "Session and terminal",
        about: "The program each console runs, the terminal it sees, and how much of its output vtium keeps.",
        groups: &[
            Group {
                title: "The session's command",
                keys: &[&LOGIN, &ONESHOT],
            },
            Group {
                title: "The session's terminal",
                keys: &[&TERM, &RESET_ENV, &BACKSPACE_DELETE],
            },
            Group {
                title: "Scrollback",
                keys: &[&SB_SIZE, &SB_SIZE_CAP, &SB_MEMORY],
            },
            Group {
                title: "Bell",
                keys: &[&BELL],
            },
            Group {
                title: "Synchronized frames",
                keys: &[&SYNC_TIMEOUT],
            },
        ],
    },
    Section {
        title: "Consoles and the VT",
        about: "How many consoles the heads hold, and the VT vtium runs on.",
        groups: &[
            Group {
                title: "Consoles",
                keys: &[&HEAD_CONSOLES_MAX, &CONSOLES_MAX],
            },
            Group {
                title: "The VT",
                keys: &[&VT, &SWITCHVT],
            },
        ],
    },
    Section {
        title: "On-screen display and notices",
        about: "What vtium draws over the console to show which head and console are in front, and to tell of a problem, and the colors it draws them in.",
        groups: &[
            Group {
                title: "On-screen display",
                keys: &[&OSD, &OSD_TIMEOUT, &FOCUS_MARK],
            },
            Group {
                title: "Notice colors",
                keys: &[&NOTICE_FOREGROUND, &NOTICE_BACKGROUND, &NOTICE_BORDER],
            },
            Group {
                title: "On-screen display colors",
                keys: &[&OSD_FOREGROUND, &OSD_BACKGROUND, &OSD_BORDER],
            },
        ],
    },
    Section {
        title: "Displays",
        about: "The mode, depth and density of each display, how vtium waits for one at start, and how it draws to them.",
        groups: &[
            Group {
                title: "Display modes",
                keys: &[&USE_ORIGINAL_MODE, &MODE],
            },
            Group {
                title: "Display depth and density",
                keys: &[&DISPLAY_DEPTH, &DISPLAY_DPI],
            },
            Group {
                title: "Waiting for a display",
                keys: &[&DISPLAY_WAIT],
            },
            Group {
                title: "Display buffers",
                keys: &[&DISPLAY_BUFFERS],
            },
            Group {
                title: "Drawing through the GPU",
                keys: &[&HWACCEL],
            },
            Group {
                title: "Frame timer",
                keys: &[&RENDER_TIMER],
            },
        ],
    },
    Section {
        title: "Power and blanking",
        about: "When the displays turn off, and the keys that reboot, power off or suspend the machine.",
        groups: &[
            Group {
                title: "Blanking the displays",
                keys: &[&DPMS_TIMEOUT, &PROGRAM_BLANK],
            },
            Group {
                title: "Power keys",
                keys: &[&GRAB_REBOOT, &GRAB_POWER_OFF, &GRAB_SUSPEND],
            },
        ],
    },
    Section {
        title: "Pictures",
        about: "Sixel and kitty graphics: how much memory pictures take, which protocols vtium offers, and the helper that reads and decodes them.",
        groups: &[
            Group {
                title: "Picture memory and protocols",
                keys: &[
                    &IMAGE_STORAGE_LIMIT,
                    &SIXEL,
                    &KITTY_GRAPHICS,
                    &MAX_GRAPHIC_SIZE,
                ],
            },
            Group {
                title: "Sixel color registers",
                keys: &[&NUM_COLOR_REGISTERS, &PRIVATE_COLOR_REGISTERS],
            },
            Group {
                title: "Image helper",
                keys: &[&IMAGE_HELPER],
            },
        ],
    },
    Section {
        title: "Running vtium",
        about: "Where vtium finds its configuration, its log, the glyph helper that draws a font's glyphs, and the glyph cache.",
        groups: &[
            Group {
                title: "Command line",
                keys: &[&CONFIGDIR, &HELP, &VERSION],
            },
            Group {
                title: "Debug log",
                keys: &[&DEBUG],
            },
            Group {
                title: "Glyph helper",
                keys: &[&FONT_HELPER],
            },
            Group {
                title: "Glyph cache",
                keys: &[&GLYPH_CACHE],
            },
        ],
    },
];

/// The keys vtium takes and does not act on yet, which the crate
/// README's "Not yet built" lists; `check::semantics` warns when one is
/// set.
pub const NOT_BUILT: [&Key; 3] = [&HWACCEL, &FONT_HINTING, &FONT_LINE_SPACING];

/// The key named `name`.
pub fn find(name: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.name == name)
}

/// The key with the one-letter option `short`.
pub fn find_short(short: char) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.short == Some(short))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_key_is_in_one_group_of_one_section() {
        let mut grouped: Vec<&str> = SECTIONS
            .iter()
            .flat_map(|s| s.groups.iter())
            .flat_map(|g| g.keys.iter().map(|k| k.name))
            .collect();
        let count = grouped.len();
        grouped.sort_unstable();
        grouped.dedup();
        assert_eq!(grouped.len(), count, "a key is in two groups");
        let mut all: Vec<&str> = KEYS.iter().map(|k| k.name).collect();
        all.sort_unstable();
        assert_eq!(grouped, all);
        for s in &SECTIONS {
            assert!(!s.groups.is_empty(), "{}", s.title);
            assert!(!s.about.is_empty(), "{}", s.title);
            for g in s.groups {
                assert!(!g.keys.is_empty(), "{}", g.title);
            }
        }
    }

    #[test]
    fn each_description_is_paragraphs_separated_by_one_blank_line() {
        for k in &KEYS {
            for p in k.paragraphs() {
                assert!(!p.is_empty(), "{}: an empty paragraph", k.name);
                assert!(!p.contains('\n'), "{}: a newline inside {p:?}", k.name);
                assert_eq!(p, p.trim(), "{}: blanks around {p:?}", k.name);
            }
        }
    }

    #[test]
    fn a_description_on_one_line_joins_its_paragraphs_with_a_space() {
        assert_eq!(
            BELL.what_line(),
            "Rings the VT's bell when a program sends BEL. A program stops its own BELs ringing by resetting mode 1042, which starts set. Off, no BEL rings."
        );
        assert_eq!(BELL.paragraphs().count(), 2);
        assert_eq!(MOUSE.what_line(), MOUSE.what);
    }

    // ADR-1500 D8: a build turns `hwaccel` on or off by default with
    // VTIUM_HWACCEL, and leaves it off when it names neither.
    #[test]
    fn the_build_sets_hwaccels_default() {
        assert!(!build_hwaccel(None));
        assert!(build_hwaccel(Some("on")));
        assert!(!build_hwaccel(Some("off")));
        assert_eq!(
            HWACCEL.default,
            Default::Switch(build_hwaccel(option_env!("VTIUM_HWACCEL")))
        );
    }

    // ADR-1500 D8: the generated docs show the source tree's defaults,
    // whatever the build names, and every other key's default is its own.
    #[test]
    fn the_tree_default_is_the_build_settings_unset() {
        assert_eq!(HWACCEL.tree_default(), Default::Switch(false));
        assert_eq!(DISPLAY_BUFFERS.tree_default(), Default::Unset);
        for k in KEYS
            .iter()
            .filter(|k| ![HWACCEL.name, DISPLAY_BUFFERS.name].contains(&k.name))
        {
            assert_eq!(k.tree_default(), k.default, "{}", k.name);
        }
    }

    // ADR-1500 D8: a value other than `on` or `off` fails the build, which
    // evaluates the same function where it compiles the table.
    #[test]
    fn the_build_refuses_any_other_hwaccel() {
        for value in ["", "On", "yes", "1", " on"] {
            let refused = std::panic::catch_unwind(|| build_hwaccel(Some(value)));
            assert!(refused.is_err(), "{value:?}");
        }
    }

    // ADR-1500 D8: a build sets `display-buffers`' default to 1, 2 or 3
    // with VTIUM_DISPLAY_BUFFERS, and when it names none the key stays
    // unset, so the console's own rule by its features decides.
    #[test]
    fn the_build_sets_display_buffers_default() {
        assert_eq!(build_display_buffers(None), None);
        assert_eq!(buffers_default(None), Default::Unset);
        for (text, n) in [("1", 1), ("2", 2), ("3", 3)] {
            assert_eq!(build_display_buffers(Some(text)), Some(n));
            assert_eq!(buffers_default(Some(n)), Default::Text(text));
        }
        assert_eq!(
            DISPLAY_BUFFERS_BUILD,
            build_display_buffers(option_env!("VTIUM_DISPLAY_BUFFERS"))
        );
        assert_eq!(
            DISPLAY_BUFFERS.default,
            buffers_default(DISPLAY_BUFFERS_BUILD)
        );
        for value in ["", "0", "4", "two", "3 "] {
            let refused = std::panic::catch_unwind(|| build_display_buffers(Some(value)));
            assert!(refused.is_err(), "{value:?}");
        }
    }
}
