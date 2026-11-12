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
/// unless it names `on` (ADR-1500 D8). `make install-build` passes it
/// from make's `HWACCEL`.
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

/// Why vtium refuses a [`Kind::Path`] value that is not an absolute
/// path, as the console's reading and vtium-config say it.
pub const NOT_ABSOLUTE: &str = "the path must be absolute, starting with /";

/// Whether `text` is a [`Kind::Path`] value's path: one that starts with
/// `/`.
pub fn absolute(text: &str) -> bool {
    std::path::Path::new(text).is_absolute()
}

/// The smallest and largest side of a cell that a frit's `cell=` line
/// and a look's `cell` state, in pixels: from minspec's 8 pixels to an 8K
/// display's large sizes, as far as a glyph two cells wide still fits a
/// glyph set's 16 KiB chunk (ADR-1200 D1). vtium-config's frit reader
/// and the console take them from here.
pub const FRIT_CELL_MIN: u32 = 6;
pub const FRIT_CELL_MAX: u32 = 96;

/// The smallest and largest size a frit's `size=` line names, in points.
/// vtium-config's frit reader and the console take them from here, so the
/// two programs read `size=` within the same bounds.
pub const FRIT_SIZE_MIN: u32 = 6;
pub const FRIT_SIZE_MAX: u32 = 96;

/// A cell as a frit's `cell=` line and a look's `cell` write it,
/// `WIDTHxHEIGHT` in pixels: each side digits alone, with no leading zero,
/// from [`FRIT_CELL_MIN`] to [`FRIT_CELL_MAX`]. The width and the height;
/// `None` for any other text.
pub fn parse_cell(text: &str) -> Option<(u32, u32)> {
    let side = |s: &str| {
        s.parse::<u32>().ok().filter(|n| {
            (FRIT_CELL_MIN..=FRIT_CELL_MAX).contains(n)
                && s.bytes().all(|b| b.is_ascii_digit())
                && !s.starts_with('0')
        })
    };
    let (w, h) = text.split_once('x')?;
    Some((side(w)?, side(h)?))
}

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
    /// An absolute path: text that starts with `/`, of at most
    /// [`TEXT_MAX`] bytes, with no control character. vtium reads the path
    /// the same way whatever directory it was started in, which is `/`
    /// under a systemd unit and anywhere else otherwise, so it refuses a
    /// relative path and an empty one ([`NOT_ABSOLUTE`]).
    Path,
    /// Absolute paths, separated by `:` as `PATH` separates them, each
    /// of at most [`TEXT_MAX`] bytes with no control character. vtium
    /// ignores a folder that is not an absolute path, and keeps the rest.
    Paths,
    /// One of a fixed set of words.
    Choice(&'static [&'static str]),
    /// A color, `r,g,b`, each 0 to 255.
    Rgb,
    /// A display mode, `[GPU][CONNECTOR]WxH[@RATE]`. Each line adds one,
    /// and the command line's replace the file's.
    Mode,
    /// A command and its arguments. In a file, one line split like shell
    /// words; on the command line, `--login -- ARGS...`, the words after
    /// `--`.
    Command,
    /// A key binding: key combinations such as `<Ctrl><Alt>Delete`,
    /// separated by commas, as the console's key binding parser reads them.
    /// Empty, it binds none.
    KeyBinding,
    /// One character, no control character or space.
    Char,
    /// A size in pixels, `WxH`, each side from 1 to `max`.
    Size { max: u32 },
    /// A cell in pixels, `WIDTHxHEIGHT`, as a frit's `cell=` line writes
    /// it: each side a whole number from [`FRIT_CELL_MIN`] to
    /// [`FRIT_CELL_MAX`], with no leading zero ([`parse_cell`]).
    Cell,
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
    /// In a section of vtium.conf only, at the levels its `levels` name:
    /// neither the top level nor the command line sets it.
    Section,
}

/// A part of vtium.conf that may set a key (ADR-1403 D5): its top level,
/// above the first section heading, a `[look NAME]` section or a
/// `[head NAME]` section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// The top level, whose keys are the defaults every head inherits.
    Top,
    /// A `[look NAME]` section, which a head names with `look =`.
    Look,
    /// A `[head NAME]` section.
    Head,
}

/// A set of [`Level`]s: where in vtium.conf a key may be set. One byte,
/// so the key table stays small enough to be a constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Levels(u8);

impl Levels {
    /// No level: a key the command line alone sets.
    pub const NONE: Levels = Levels(0);
    /// The top level alone.
    pub const TOP: Levels = Levels(1);
    /// A look alone: a key of a look's that has no default at the top
    /// level, such as its cell.
    pub const LOOK: Levels = Levels(2);
    /// The top level and a look: a look's frit, its size and the other keys
    /// ADR-1200 D16 gives a look.
    pub const TOP_LOOK: Levels = Levels(1 | 2);
    /// The top level and a head: of ADR-1401 D3's head keys, the one the
    /// key table also has.
    pub const TOP_HEAD: Levels = Levels(1 | 4);

    fn bit(level: Level) -> u8 {
        match level {
            Level::Top => 1,
            Level::Look => 2,
            Level::Head => 4,
        }
    }

    /// Whether a key may be set at `level`.
    pub fn contains(self, level: Level) -> bool {
        self.0 & Levels::bit(level) != 0
    }

    /// The levels in the set, the top level first.
    pub fn iter(self) -> impl Iterator<Item = Level> {
        [Level::Top, Level::Look, Level::Head]
            .into_iter()
            .filter(move |&l| self.contains(l))
    }
}

const NONE: Levels = Levels::NONE;
const TOP: Levels = Levels::TOP;
const LOOK: Levels = Levels::LOOK;
const TOP_LOOK: Levels = Levels::TOP_LOOK;
const TOP_HEAD: Levels = Levels::TOP_HEAD;

/// One row of the key table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub name: &'static str,
    /// The one-letter command-line option, if it has one.
    pub short: Option<char>,
    pub kind: Kind,
    pub default: Default,
    pub scope: Scope,
    /// Where in vtium.conf the key may be set (ADR-1403 D5): the top
    /// level, and for a few keys a look's or a head's section as well. A
    /// key set on the command line alone has none. The section reader takes
    /// its lists of a section's keys from this field, so letting a section
    /// set one more key is an edit to that key's row.
    pub levels: Levels,
    /// The crate that reads the value.
    pub reader: &'static str,
    /// What it does. The first paragraph is one sentence saying what the
    /// key does; further paragraphs give its values and details. A blank
    /// line, `\n\n`, separates two paragraphs, and the text holds no other
    /// newline.
    pub what: &'static str,
    /// Where it comes from: the decision or specification that defines it,
    /// or the released kmscon's key it came from.
    pub cite: &'static str,
}

/// The mark that opens a paragraph of [`Key::what`] meant for docs/config.md
/// alone. The example vtium.conf leaves such a paragraph out, to keep its
/// size. The key's author marks a paragraph that only elaborates, gives an
/// example, or tells background. A paragraph that says when a change takes
/// effect, what vtium requires or refuses, or what a value means stays
/// unmarked, so that a person editing the line in a terminal sees it.
pub const DOCS_ONLY: &str = "[docs] ";

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

    /// The paragraphs of [`Key::what`], each without its [`DOCS_ONLY`] mark.
    pub fn paragraphs(&self) -> impl Iterator<Item = &'static str> {
        self.what
            .split("\n\n")
            .map(|p| p.strip_prefix(DOCS_ONLY).unwrap_or(p))
    }

    /// The paragraphs of [`Key::what`] that the example vtium.conf shows:
    /// all but those marked [`DOCS_ONLY`], which only docs/config.md shows.
    /// The first paragraph is never marked.
    pub fn example_paragraphs(&self) -> impl Iterator<Item = &'static str> {
        self.what
            .split("\n\n")
            .filter(|p| !p.starts_with(DOCS_ONLY))
    }

    /// [`Key::what`] on one line, its paragraphs joined by a space, for a
    /// reader that shows the text on one line or wraps it itself.
    pub fn what_line(&self) -> String {
        self.paragraphs().collect::<Vec<_>>().join(" ")
    }
}

/// The values of `glyph-source` and its per-range keys.
const GLYPH_SOURCES: &[&str] = &["font", "sprites", "auto"];

/// The bound of a number with no bound of its own: the largest value a
/// 32-bit unsigned field holds.
const UINT_MAX: u64 = u32::MAX as u64;

pub const CONFIGDIR: Key = Key {
    name: "configdir",
    short: Some('c'),
    kind: Kind::Text,
    default: Default::Text("/etc/vtium"),
    scope: Scope::CommandLine,
    levels: NONE,
    reader: "vtium",
    what: "The directory that holds `vtium.conf`, vtium's configuration file.",
    cite: "kmscon's `configdir` key",
};

pub const HELP: Key = Key {
    name: "help",
    short: Some('h'),
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::CommandLine,
    levels: NONE,
    reader: "vtium",
    what: "Print the options and exit.",
    cite: "kmscon's `help` key",
};

pub const VERSION: Key = Key {
    name: "version",
    short: Some('V'),
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::CommandLine,
    levels: NONE,
    reader: "vtium",
    what: "Print the version, the source revision of the build, and the optional features it was built with, then exit.",
    cite: "kmscon's `version` key",
};

pub const DEBUG: Key = Key {
    name: "debug",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Log debug messages too, to help find what went wrong. When off, vtium logs information messages, warnings and errors.",
    cite: "kmscon's `debug` key",
};

pub const VT: Key = Key {
    name: "vt",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "The VT to run on: a number N for `/dev/ttyN`, a name under `/dev`, or a path. When unset, vtium runs on the VT it was started on, or else the next free one.",
    cite: "kmscon's `vt` key",
};

pub const SWITCHVT: Key = Key {
    name: "switchvt",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "Switch to vtium's VT when vtium starts. When off, vtium waits until something else switches to it.",
    cite: "kmscon's `switchvt` key",
};

pub const LOGIN: Key = Key {
    name: "login",
    short: Some('l'),
    kind: Kind::Command,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP_HEAD,
    reader: "vtium",
    what: "The command each console runs, such as a login prompt or a shell.\n\n\
           When unset, the command is `/bin/login -p` with `reset-env` on and `/bin/login` with it off. A word written `$NAME` is replaced by that environment variable's value, and `$$` writes a literal `$` at the start of a word.",
    cite: "kmscon's `login` key",
};

pub const ONESHOT: Key = Key {
    name: "oneshot",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Exit when the console's command exits, instead of starting it again.",
    cite: "kmscon's `oneshot` key",
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
    levels: TOP,
    reader: "vtium",
    what: "The most consoles one head can have. A head's `consoles` cannot open more than this.",
    cite: "[the heads issue](https://github.com/vtium/vtium/issues/156)",
};

pub const OSD: Key = Key {
    name: "osd",
    short: None,
    kind: Kind::Choice(&["off", "minimal", "full"]),
    default: Default::Text("minimal"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "What the on-screen display shows at the top of each screen after a change, such as a console switch.\n\n\
           `minimal` shows a dot in the top-right character cell: filled on the active head, hollow on the others. `full` shows a line across the top, such as `● desk · console 2 of 3`. `off` shows nothing.\n\n\
           A notice about a problem appears whatever `osd` says.",
    cite: "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
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
    levels: TOP,
    reader: "vtium",
    what: "How long the on-screen display stays visible after a change. `never` keeps it visible all the time.",
    cite: "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
};

pub const FOCUS_MARK: Key = Key {
    name: "focus-mark",
    short: None,
    kind: Kind::Choice(&["auto", "always", "off"]),
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The old name for `osd` and `osd-timeout`.\n\n\
           vtium reads it only when `osd` and `osd-timeout` are not set: `auto` means `osd = minimal`, `always` means `osd = minimal` with `osd-timeout = never`, and `off` means `osd = off`. When it is set, vtium logs the new keys.",
    cite: "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
};

pub const NOTICE_FOREGROUND: Key = Key {
    name: "notice-foreground",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([31, 41, 51]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The text color of a notice. A notice is a box that reports a problem, such as errors in vtium.conf. It appears whatever `osd` says.",
    cite: "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
};

pub const NOTICE_BACKGROUND: Key = Key {
    name: "notice-background",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([254, 226, 226]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The background color of a notice. While a notice is showing, the on-screen display's line appears in the same box, in the notice's colors.",
    cite: "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
};

pub const NOTICE_BORDER: Key = Key {
    name: "notice-border",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([185, 28, 28]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The border color of a notice.",
    cite: "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
};

pub const KERNEL_MESSAGES: Key = Key {
    name: "kernel-messages",
    short: None,
    kind: Kind::Choice(&["after-plymouth", "always"]),
    default: Default::Text("after-plymouth"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "When vtium shows the kernel's warnings and errors on the screen.\n\n\
           vtium shows each new kernel message whose level is warning or more severe, on top of the console, as long as the kernel's console log level lets it through. `quiet` and `loglevel=` on the kernel command line set that level. vtium never writes them into a console's session.\n\n\
           `after-plymouth` leaves the messages to Plymouth, the boot splash program, while it runs, and vtium shows them once Plymouth has quit. Once Plymouth has quit, vtium does not go back and show the messages that came while it ran. On a machine without Plymouth, vtium shows them from its start. `always` shows them whether Plymouth runs or not.",
    cite: "[the boot messages issue](https://github.com/vtium/vtium/issues/266)",
};

pub const OSD_FOREGROUND: Key = Key {
    name: "osd-foreground",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([229, 231, 235]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The text color of the on-screen display: the line of `osd = full` and the dot of `osd = minimal`.",
    cite: "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
};

pub const OSD_BACKGROUND: Key = Key {
    name: "osd-background",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([31, 41, 51]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The background color of the on-screen display: behind the line of `osd = full`, and in the character cell behind the dot of `osd = minimal`.",
    cite: "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
};

pub const OSD_BORDER: Key = Key {
    name: "osd-border",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([97, 110, 124]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The border color of the on-screen display with `osd = full`. The dot of `osd = minimal` has no border.",
    cite: "[the on-screen display issue](https://github.com/vtium/vtium/issues/169)",
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
    levels: TOP,
    reader: "vtium",
    what: "The most consoles all heads can have together.\n\n\
           Every head keeps its first console. More consoles open, in the order the heads appear in vtium.conf, until the total reaches this number.",
    cite: "[the heads issue](https://github.com/vtium/vtium/issues/156)",
};

pub const TERM: Key = Key {
    name: "term",
    short: Some('t'),
    kind: Kind::Text,
    default: Default::Text(TERM_NAME),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The value of `TERM` that programs on the console see.\n\n\
           The default is vtium's own terminfo entry, `xterm-vtium`, which is also named `vtium`. With either name, vtium answers capability queries (XTGETTCAP) from that entry. With `kmscon`, vtium answers from kmscon's entry. With any other value, vtium answers every query as unknown.",
    cite: "kmscon's `term` key",
};

pub const RESET_ENV: Key = Key {
    name: "reset-env",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Start each console's command with only the environment variables vtium sets. When off, the command also gets vtium's own environment.",
    cite: "kmscon's `reset-env` key",
};

pub const BACKSPACE_DELETE: Key = Key {
    name: "backspace-delete",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "Backspace sends DEL (0x7f). When off, it sends BS (0x08).",
    cite: "kmscon's `backspace-delete` key",
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
    levels: TOP,
    reader: "vtium-term",
    what: "How many lines of scrollback each console keeps, up to `sb-size-cap`.",
    cite: "kmscon's `sb-size` key",
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
    levels: TOP,
    reader: "vtium-term",
    what: "The most lines `sb-size` can ask for.\n\n\
           When unset, it depends on the machine's memory: 10,000 lines below 2 GiB, 30,000 from 2 GiB and 100,000 from 4 GiB.",
    cite: "[the configuration issue](https://github.com/vtium/vtium/issues/21)",
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
    levels: TOP,
    reader: "vtium-term",
    what: "The most memory each console's scrollback uses. When it is full, the oldest lines are dropped, as they are past `sb-size`.\n\n\
           The value is in bytes, or in KiB, MiB or GiB with `K`, `M` or `G` (such as `6M`), up to a quarter of the machine's memory (or up to the default when `/proc/meminfo` cannot be read).\n\n\
           When unset, vtium sizes it to fit each console's size in characters, and sizes it again when that changes.\n\n\
           [docs] A console up to 170x48 characters gets what is left of a 16 MiB budget after the console's screens and the rest of vtium, so a smaller console keeps more lines. A larger console gets the 170x48 value shown here.\n\n\
           [docs] The default does not grow with the display or the machine's memory. On a machine with memory to spare, set `sb-memory` for a deeper scrollback; a set value stays fixed.",
    cite: "[the scrollback memory issue](https://github.com/vtium/vtium/issues/131)",
};

pub const BELL: Key = Key {
    name: "bell",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Ring the console's bell when a program sends BEL.\n\n\
           A program can silence its own bells by resetting mode 1042, which starts set. When off, no bell rings.",
    cite: "kmscon's `bell` key",
};

pub const BLINK: Key = Key {
    name: "blink",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Blink blinking text (SGR 5) and the cursor, every 500 ms. When off, neither blinks.",
    cite: "kmscon's `blink` key",
};

pub const BLINK_TEXT: Key = Key {
    name: "blink-text",
    short: None,
    kind: Kind::Choice(&["full", "partial", "off"]),
    default: Default::Text("partial"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-render",
    what: "How blinking text (SGR 5 or 6) looks while a blink is off.\n\n\
           `full` hides it, `partial` dims it halfway to its background, as faint text (SGR 2) is, and `off` keeps it steady. The cursor follows `blink` either way.",
    cite: "[the blink options issue](https://github.com/vtium/vtium/issues/81); the maintainer's decision of 2026-11-03",
};

pub const BLINK_DECORATIONS: Key = Key {
    name: "blink-decorations",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-render",
    what: "Blink the underline and strikethrough of blinking text with it.\n\n\
           They hide while a blink is off with `blink-text = full`, and dim with `partial`. When off, they stay visible while the text blinks.",
    cite: "[the blink options issue](https://github.com/vtium/vtium/issues/81)",
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
    levels: TOP,
    reader: "vtium",
    what: "The longest vtium waits for a program to finish a synchronized update (mode 2026) before it shows the screen as it is. 0 turns mode 2026 off.",
    cite: "[the modes issue](https://github.com/vtium/vtium/issues/12)",
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
    levels: TOP,
    reader: "vtium-platform",
    what: "Turn the displays off after this long with no key press or mouse action (a move, a click or the wheel). 0 never turns them off.",
    cite: "kmscon's `dpms-timeout` key",
};

pub const PROGRAM_BLANK: Key = Key {
    name: "program-blank",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Let a program set the blanking timeout and wake the displays, as `setterm --blank` does on the Linux console.\n\n\
           A program sets the timeout with `CSI 9 ; n ]` (n minutes, up to 60; 0 never blanks) and wakes the displays with `CSI 13 ]`. A terminal reset (RIS) returns the timeout to `dpms-timeout`. When off, vtium ignores these sequences.\n\n\
           A key press or mouse action wakes the displays either way.",
    cite: "[the screen blanking issue](https://github.com/vtium/vtium/issues/79)",
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
    levels: TOP,
    reader: "vtium",
    what: "How long vtium waits for a display when it starts, counted from when its VT is shown.\n\n\
           If no display appears in that time, vtium exits with status 1, so the unit's `OnFailure=` can start a getty. When vtium.conf's policy is closed, vtium waits as long as it takes, as with `forever`.\n\n\
           vtium counts only while the VT is its own. If another program takes the VT before vtium's first frame is on screen, vtium waits for the VT to come back and then starts the full wait again.\n\n\
           [docs] X does this at boot when it starts on another VT.\n\n\
           With 0, vtium does not wait: only a display that is ready when the VT is shown counts. `forever` waits as long as it takes.\n\n\
           [docs] `forever` suits a machine whose only display is a USB adapter (DisplayLink, `udl`) that may be plugged in at any time.\n\n\
           Once a display has appeared, vtium never exits for lack of one.",
    cite: "[the configuration issue](https://github.com/vtium/vtium/issues/21); [the issue about X starting during vtium's first frame](https://github.com/vtium/vtium/issues/270)",
};

pub const DISPLAY_BUFFERS: Key = Key {
    name: "display-buffers",
    short: None,
    kind: Kind::Choice(&["1", "2", "3"]),
    default: buffers_default(DISPLAY_BUFFERS_BUILD),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "How many frame buffers each display keeps (DRM displays only; every display gets the same number).\n\n\
           `3` is triple buffering: no frame waits for the previous one to reach the screen, and each display needs one more frame of memory. `2` is double buffering. `1` is single buffering, which uses the least memory, but vtium then draws into the buffer on screen, so a half-drawn frame can show.\n\n\
           A framebuffer device (fbdev) always keeps one buffer. When unset, the build decides: the number it was built with (`VTIUM_DISPLAY_BUFFERS`), or else 3 in a build with optional features, 2 in the smallest build (minspec), and 1 only in a build smaller than that.\n\n\
           [docs] `vtium config get display-buffers` prints the draft's value followed by `draft`, or the build's default followed by `default` (`unset` when the build sets no number, in which case the rule above decides).",
    cite: "[the displays issue](https://github.com/vtium/vtium/issues/18)",
};

pub const HWACCEL: Key = Key {
    name: "hwaccel",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(HWACCEL_DEFAULT),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Draw with the GPU, in a build that supports it.\n\n\
           It is off unless vtium was built with it on (`VTIUM_HWACCEL`).\n\n\
           [docs] `vtium config get hwaccel` prints the draft's value followed by `draft`, or the build's default followed by `default`. `vtium config set hwaccel on` turns it on in the draft.",
    cite: "kmscon's `hwaccel` key",
};

pub const USE_ORIGINAL_MODE: Key = Key {
    name: "use-original-mode",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "Keep each display's current resolution. When off, vtium sets each display's preferred resolution.",
    cite: "kmscon's `use-original-mode` key; [the native mode issue](https://github.com/vtium/vtium/issues/5)",
};

pub const MODE: Key = Key {
    name: "mode",
    short: None,
    kind: Kind::Mode,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "A display's resolution and refresh rate.\n\n\
           `WxH[@RATE]` sets it for every display, `[CONNECTOR]WxH` for one connector, and `GPU[CONNECTOR]WxH` for one connector on one graphics card, named by its udev `ID_PATH`. Write one `mode` line per display; the most specific match wins. When unset, each display uses its preferred resolution. Setting it turns `use-original-mode` off.",
    cite: "kmscon's `mode` key",
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
    levels: TOP,
    reader: "vtium-term",
    what: "The sixteen basic colors, and the default text and background colors.\n\n\
           The value is the name of a palette, or `custom`, which takes the colors from the `palette-*` keys. Colors 16 to 255 are xterm's in every palette.",
    cite: "kmscon's `palette` key",
};

pub const PALETTE_BLACK: Key = Key {
    name: "palette-black",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 0, 0]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 0 (black) when `palette = custom`.",
    cite: "kmscon's `palette-black` key",
};

pub const PALETTE_RED: Key = Key {
    name: "palette-red",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([205, 0, 0]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 1 (red) when `palette = custom`.",
    cite: "kmscon's `palette-red` key",
};

pub const PALETTE_GREEN: Key = Key {
    name: "palette-green",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 205, 0]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 2 (green) when `palette = custom`.",
    cite: "kmscon's `palette-green` key",
};

pub const PALETTE_YELLOW: Key = Key {
    name: "palette-yellow",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([205, 205, 0]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 3 (yellow) when `palette = custom`.",
    cite: "kmscon's `palette-yellow` key",
};

pub const PALETTE_BLUE: Key = Key {
    name: "palette-blue",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 0, 238]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 4 (blue) when `palette = custom`.",
    cite: "kmscon's `palette-blue` key",
};

pub const PALETTE_MAGENTA: Key = Key {
    name: "palette-magenta",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([205, 0, 205]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 5 (magenta) when `palette = custom`.",
    cite: "kmscon's `palette-magenta` key",
};

pub const PALETTE_CYAN: Key = Key {
    name: "palette-cyan",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 205, 205]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 6 (cyan) when `palette = custom`.",
    cite: "kmscon's `palette-cyan` key",
};

pub const PALETTE_LIGHT_GREY: Key = Key {
    name: "palette-light-grey",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([229, 229, 229]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 7 (light grey) when `palette = custom`.",
    cite: "kmscon's `palette-light-grey` key",
};

pub const PALETTE_DARK_GREY: Key = Key {
    name: "palette-dark-grey",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([127, 127, 127]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 8 (dark grey) when `palette = custom`.",
    cite: "kmscon's `palette-dark-grey` key",
};

pub const PALETTE_LIGHT_RED: Key = Key {
    name: "palette-light-red",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 0, 0]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 9 (light red) when `palette = custom`.",
    cite: "kmscon's `palette-light-red` key",
};

pub const PALETTE_LIGHT_GREEN: Key = Key {
    name: "palette-light-green",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 255, 0]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 10 (light green) when `palette = custom`.",
    cite: "kmscon's `palette-light-green` key",
};

pub const PALETTE_LIGHT_YELLOW: Key = Key {
    name: "palette-light-yellow",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 255, 0]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 11 (light yellow) when `palette = custom`.",
    cite: "kmscon's `palette-light-yellow` key",
};

pub const PALETTE_LIGHT_BLUE: Key = Key {
    name: "palette-light-blue",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([92, 92, 255]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 12 (light blue) when `palette = custom`.",
    cite: "kmscon's `palette-light-blue` key",
};

pub const PALETTE_LIGHT_MAGENTA: Key = Key {
    name: "palette-light-magenta",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 0, 255]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 13 (light magenta) when `palette = custom`.",
    cite: "kmscon's `palette-light-magenta` key",
};

pub const PALETTE_LIGHT_CYAN: Key = Key {
    name: "palette-light-cyan",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 255, 255]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 14 (light cyan) when `palette = custom`.",
    cite: "kmscon's `palette-light-cyan` key",
};

pub const PALETTE_WHITE: Key = Key {
    name: "palette-white",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([255, 255, 255]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Color 15 (white) when `palette = custom`.",
    cite: "kmscon's `palette-white` key",
};

pub const PALETTE_FOREGROUND: Key = Key {
    name: "palette-foreground",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([229, 229, 229]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "The default text color when `palette = custom`.",
    cite: "kmscon's `palette-foreground` key",
};

pub const PALETTE_BACKGROUND: Key = Key {
    name: "palette-background",
    short: None,
    kind: Kind::Rgb,
    default: Default::Rgb([0, 0, 0]),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "The default background color when `palette = custom`.",
    cite: "kmscon's `palette-background` key",
};

pub const UNDERLINE_COLOR: Key = Key {
    name: "underline-color",
    short: None,
    kind: Kind::DecoColor,
    default: Default::Text("fg"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-render",
    what: "The color of underlines (SGR 4 in every style, and SGR 21).\n\n\
           `fg` uses the text's color, a palette number uses that palette color, and `#rrggbb` uses that color. A color a program sets with SGR 58 wins, unless `force-decoration-colors` is on.\n\n\
           [docs] A program can change a palette color with OSC 4.",
    cite: "[the decoration colors issue](https://github.com/vtium/vtium/issues/127)",
};

pub const STRIKE_COLOR: Key = Key {
    name: "strike-color",
    short: None,
    kind: Kind::DecoColor,
    default: Default::Text("fg"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-render",
    what: "The color of strikethrough (SGR 9). It takes the same values as `underline-color`.",
    cite: "[the decoration colors issue](https://github.com/vtium/vtium/issues/127)",
};

pub const FORCE_DECORATION_COLORS: Key = Key {
    name: "force-decoration-colors",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-render",
    what: "Use `underline-color` even when a program sets its own underline color (SGR 58).",
    cite: "[the decoration colors issue](https://github.com/vtium/vtium/issues/127)",
};

pub const CLIPBOARD_WRITE: Key = Key {
    name: "clipboard-write",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Let programs copy text to vtium's clipboard (OSC 52). When off, vtium ignores what they send.\n\n\
           vtium keeps the last text copied, by a program or by you, and the middle mouse button and the `key-paste` key paste it.\n\n\
           [docs] The console has no system clipboard.",
    cite: "[the OSC 52 copy issue](https://github.com/vtium/vtium/issues/39)",
};

pub const XKB_MODEL: Key = Key {
    name: "xkb-model",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "The keyboard model, as xkeyboard-config names it.",
    cite: "kmscon's `xkb-model` key",
};

pub const XKB_LAYOUT: Key = Key {
    name: "xkb-layout",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "The keyboard layout, such as `us` or `de`.",
    cite: "kmscon's `xkb-layout` key",
};

pub const XKB_VARIANT: Key = Key {
    name: "xkb-variant",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "The keyboard layout's variant, such as `dvorak`.",
    cite: "kmscon's `xkb-variant` key",
};

pub const XKB_OPTIONS: Key = Key {
    name: "xkb-options",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "Keyboard options for xkb, separated by commas, such as `ctrl:nocaps`.",
    cite: "kmscon's `xkb-options` key",
};

pub const XKB_KEYMAP: Key = Key {
    name: "xkb-keymap",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "A keymap file. When set, vtium uses it instead of the model, layout, variant and options, and falls back to them if the file cannot be used.",
    cite: "kmscon's `xkb-keymap` key",
};

pub const XKB_COMPOSE_FILE: Key = Key {
    name: "xkb-compose-file",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "A compose table file, used instead of the locale's.",
    cite: "kmscon's `xkb-compose-file` key",
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
    levels: TOP,
    reader: "vtium-platform",
    what: "How long a held key waits before it starts to repeat. 0 means 250 ms.",
    cite: "kmscon's `xkb-repeat-delay` key",
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
    levels: TOP,
    reader: "vtium-platform",
    what: "The time between the repeats of a held key. 0 means 50 ms.",
    cite: "kmscon's `xkb-repeat-rate` key",
};

pub const MOUSE: Key = Key {
    name: "mouse",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-platform",
    what: "Use mice, touchpads and touchscreens.",
    cite: "kmscon's `mouse` key",
};

pub const NATURAL_SCROLLING: Key = Key {
    name: "natural-scrolling",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Reverse the mouse wheel.\n\n\
           The wheel then scrolls the other way, both in vtium's scrollback and for programs.\n\n\
           [docs] This includes the arrow keys vtium sends in full-screen programs (mode 1007). Ctrl with the wheel turned up still zooms in.",
    cite: "kmscon's `natural-scrolling` key",
};

pub const POINTER_CHAR: Key = Key {
    name: "pointer-char",
    short: None,
    kind: Kind::Char,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-render",
    what: "Show the mouse pointer as this character, drawn with the console's font.\n\n\
           When unset, or when the font cannot display the character, the pointer is an arrow.\n\n\
           [docs] The character fills the character cell under the pointer, white with a dark outline, like the arrow.",
    cite: "[the pointer options issue](https://github.com/vtium/vtium/issues/102)",
};

pub const SOFT_CURSOR: Key = Key {
    name: "soft-cursor",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Draw the mouse pointer in software on every display, never with the display's hardware cursor.",
    cite: "kmscon's `soft-cursor` key",
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
    levels: TOP,
    reader: "vtium-platform",
    what: "Hide the mouse pointer after this long without mouse activity, until it moves again. 0 never hides it.",
    cite: "[the pointer options issue](https://github.com/vtium/vtium/issues/102)",
};

pub const POINTER_HIDE_WHILE_TYPING: Key = Key {
    name: "pointer-hide-while-typing",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Hide the mouse pointer while you type, until it moves again. A key vtium handles itself, such as a shortcut, does not hide it.",
    cite: "[the pointer options issue](https://github.com/vtium/vtium/issues/102)",
};

pub const FONT_ENGINE: Key = Key {
    name: "font-engine",
    short: None,
    kind: Kind::Choice(&["freetype", "pango", "psf", "unifont", "8x16"]),
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP_LOOK,
    reader: "vtium-font",
    what: "Which kind of font to use. When unset, vtium tries each kind the build supports, in the order listed.",
    cite: "kmscon's `font-engine` key",
};

pub const FONT_NAME: Key = Key {
    name: "font-name",
    short: None,
    kind: Kind::Text,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP_LOOK,
    reader: "vtium-font",
    what: "The console's font.\n\n\
           The value is the name of a font set (a frit file, `NAME.vtfrit` in `/etc/vtium/fonts`). It can also be the name of a kbd console font, or with `font-engine = psf` the path of a PSF font; vtium then uses that one font.\n\n\
           When unset, vtium uses the font from vconsole.conf. If there is none, or vtium cannot find the font, vtium uses its built-in font and logs why.",
    cite: "kmscon's `font-name` key; [the issue that loads the configured font set](https://github.com/vtium/vtium/issues/126)",
};

pub const FONT_FRITS: Key = Key {
    name: "font-frits",
    short: None,
    kind: Kind::Path,
    default: Default::Text(FRITS),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The directory that holds the font sets (the `NAME.vtfrit` files that `font-name` names).\n\n\
           It must be an absolute path; vtium ignores a relative or empty one and logs why.\n\n\
           [docs] The default is the directory vtium was built with.",
    cite: "[the issue that loads the configured font set](https://github.com/vtium/vtium/issues/126); the maintainer's decision of 2026-11-01",
};

pub const FONT_PATHS: Key = Key {
    name: "font-paths",
    short: None,
    kind: Kind::Paths,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The folders where vtium looks, in order, for installed fonts that a font set's `font:` lines name.\n\n\
           Separate the folders with `:`, as in `PATH`; each must be an absolute path. The list replaces the built-in list. vtium skips a folder that is not an absolute path, uses the rest, and logs why.\n\n\
           [docs] The list can reorder or leave out the system's folders.\n\n\
           When unset, vtium searches `/usr/share/fonts` and `/usr/local/share/fonts`. A user's own folder, such as `~/.local/share/fonts`, is searched only when the list names it, and the font helper cannot read it before someone logs in, because it then runs as `nobody`.",
    cite: "[the configuration flow issue](https://github.com/vtium/vtium/issues/299)",
};

pub const FONT_HELPER: Key = Key {
    name: "font-helper",
    short: None,
    kind: Kind::Text,
    default: Default::Text(HELPER_PROGRAM),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The font helper: the program vtium starts to draw a font set's characters. It runs as `nobody` when vtium runs as root.\n\n\
           vtium opens the program once, when it starts, and runs every helper from that file, so a newly installed helper takes effect when vtium restarts.\n\n\
           vtium refuses a helper whose path is not absolute, that is not a regular file, that is setuid or setgid, that anyone but root or vtium's own account owns, or that its group or other users can write. It then logs why and uses its built-in font. vtium starts the helper through `/proc/self/fd`, so `/proc` must be mounted.\n\n\
           [docs] The default is the path vtium was built with.",
    cite: "[the issue that renders glyphs in the font helper](https://github.com/vtium/vtium/issues/145); the maintainer's decision of 2026-11-01",
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
    levels: TOP_LOOK,
    reader: "vtium-font",
    what: "The font size in points (1/72 inch).\n\n\
           The size in pixels is points × DPI ÷ 72, at the display's density (`display-dpi`). It overrides a font set's `cell=` line. 0 picks a size to suit each display.",
    cite: "kmscon's `font-size` key; vtium-font's `CELL_SIDE_MAX`; [the issue that renders glyphs in the font helper](https://github.com/vtium/vtium/issues/145)",
};

pub const FONT_HINTING: Key = Key {
    name: "font-hinting",
    short: None,
    kind: Kind::Choice(&["light", "full"]),
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP_LOOK,
    reader: "vtium-font",
    what: "How outline fonts are fitted to the pixel grid (hinting). It overrides a font set's `hinting=` line.\n\n\
           `light` fits each character vertically only. `full` fits a TrueType font both ways, and treats a CFF font as `light`. Bitmap fonts are not hinted.\n\n\
           This key has no effect yet: each font set's `hinting=` line decides, and `light` is used where it has none.",
    cite: "[the configuration flow issue](https://github.com/vtium/vtium/issues/299)",
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
    levels: TOP_LOOK,
    reader: "vtium-font",
    what: "Extra space between lines of text. It overrides a font set's `line-spacing=` line.\n\n\
           The value is a whole percentage of the text height, rounded to whole pixels and split above and below each line, so the console has fewer lines. When `font-size` overrides a font set's `cell=` size, the character cell grows by the spacing. When unset, the font set's `line-spacing=` decides, and 0 where it has none.\n\n\
           [docs] Letters keep their size in the middle of the taller line, and line and block characters stretch to fill it. A font set's `fit=` lines choose this for each set of characters.",
    cite: "[the line spacing issue](https://github.com/vtium/vtium/issues/277)",
};

pub const CELL: Key = Key {
    name: "cell",
    short: None,
    kind: Kind::Cell,
    default: Default::Unset,
    scope: Scope::Section,
    levels: LOOK,
    reader: "vtium",
    what: "The size of a look's character cell, written as in a font set's `cell=` line (WIDTHxHEIGHT).\n\n\
           It overrides the `cell=` and `size=` lines of the look's font set, and the top-level `font-size`. A `font-size` other than 0 in the same look overrides `cell`. When unset, the font set's `cell=` decides.",
    cite: "[the configuration flow issue](https://github.com/vtium/vtium/issues/299)",
};

pub const ALLOW_FONT_OPS: Key = Key {
    name: "allow-font-ops",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Let programs switch the console's font, and ask which font is in use (xterm's OSC 50).\n\n\
           A program switches with `OSC 50 ; NAME ST` and asks with `OSC 50 ; ? ST`. A switch lasts until that program's session ends; then vtium returns to the font and size that `font-name` and `font-size` set.\n\n\
           [docs] `vtium-config use-frit NAME` switches the font with the same sequence.\n\n\
           When off, vtium ignores OSC 50, queries included.\n\n\
           [docs] It is on by default, as xterm's `allowFontOps` is, because a program can choose only the installed font sets, the built-in fonts and the font its console started in, never another file.",
    cite: "[the issue that loads the configured font set](https://github.com/vtium/vtium/issues/126)",
};

pub const ALLOW_WINDOW_OPS: Key = Key {
    name: "allow-window-ops",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(false),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Let programs set the size of the console's text area in rows and columns (xterm's window operation `CSI 8 ; ROWS ; COLUMNS t`).\n\n\
           vtium meets the request by changing the font size. It draws at the largest font size at which the displays hold that many rows and columns, and the console's grid becomes exactly that size. When the displays cannot hold the size even at the smallest font size, the grid gets as many rows and columns as they hold there. A number left out keeps that side as it is, and 0 fills the display on that side.\n\n\
           The grid is exact, but a display may draw it in larger character cells than that font size gives, to fill more of the screen. A program that asks for the cell size, as with `CSI 16 t`, is told the cell of that font size.\n\n\
           The size lasts until the session of the program that asked for it ends, or until you zoom. Then vtium returns to the size that `font-size` sets.\n\n\
           When off, vtium ignores the request and changes nothing. It is off by default, as xterm's `allowWindowOps` is. A program can ask for the size in any case, with `CSI 18 t`.",
    cite: "[the font and window operations issue](https://github.com/vtium/vtium/issues/44)",
};

pub const GLYPH_SOURCE: Key = Key {
    name: "glyph-source",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-font",
    what: "Who draws box-drawing, block and similar characters (six ranges): the font, or vtium itself.\n\n\
           `font` uses the font's characters, `sprites` uses vtium's own drawings, and `auto` uses the font's where the font has the character. When unset, vtium uses the font for each range the font covers completely and its own drawings for the rest; with the built-in unifont, vtium always draws them itself.",
    cite: "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
};

pub const GLYPH_SOURCE_BOX: Key = Key {
    name: "glyph-source-box",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-font",
    what: "Overrides `glyph-source` for box drawing (U+2500 to U+257F).",
    cite: "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
};

pub const GLYPH_SOURCE_BLOCK: Key = Key {
    name: "glyph-source-block",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-font",
    what: "Overrides `glyph-source` for block elements (U+2580 to U+259F).",
    cite: "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
};

pub const GLYPH_SOURCE_BRAILLE: Key = Key {
    name: "glyph-source-braille",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-font",
    what: "Overrides `glyph-source` for Braille (U+2800 to U+28FF).",
    cite: "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
};

pub const GLYPH_SOURCE_SEXTANTS: Key = Key {
    name: "glyph-source-sextants",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-font",
    what: "Overrides `glyph-source` for sextants (U+1FB00 to U+1FB3B).",
    cite: "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
};

pub const GLYPH_SOURCE_OCTANTS: Key = Key {
    name: "glyph-source-octants",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-font",
    what: "Overrides `glyph-source` for octants (U+1CD00 to U+1CDE5).",
    cite: "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
};

pub const GLYPH_SOURCE_LEGACY: Key = Key {
    name: "glyph-source-legacy",
    short: None,
    kind: Kind::Choice(GLYPH_SOURCES),
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-font",
    what: "Overrides `glyph-source` for the other block shapes of Symbols for Legacy Computing.",
    cite: "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
};

pub const MISSING: Key = Key {
    name: "missing",
    short: None,
    kind: Kind::Choice(&["box", "hex"]),
    default: Default::Text("box"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-font",
    what: "What vtium shows for a character the font cannot display: `box` shows □ (U+25A1), and `hex` shows the character's code in hex digits.",
    cite: "[the glyph source issue](https://github.com/vtium/vtium/issues/85)",
};

pub const TEXT_DIRECTION: Key = Key {
    name: "text-direction",
    short: None,
    kind: Kind::Choice(&["ltr", "rtl", "auto"]),
    default: Default::Text("ltr"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Which way a paragraph of text runs when the program writing it does not say: `ltr`, left to right, `rtl`, right to left, or `auto`, which takes each paragraph's direction from its first letter.\n\n\
           A paragraph is the text between two line ends. `ltr`, the default, starts each paragraph at the left edge, and `rtl` at the right edge. Under `auto`, a paragraph that starts with an Arabic or Hebrew letter runs right to left, and any other runs left to right.\n\n\
           [docs] A paragraph keeps its direction however many rows it wraps across. `rtl` suits a console used mostly in Arabic, Hebrew or another right-to-left language.\n\n\
           vtium reads this key when it starts, so a change takes effect when vtium restarts, and it never changes text already on the screen.\n\n\
           [docs] A program can change the direction with escape sequences, and a terminal reset returns it to this setting.\n\n\
           A build without right-to-left support (the `bidi` feature) accepts this key and does nothing with it.",
    cite: "[the right-to-left text issue](https://github.com/vtium/vtium/issues/43)",
};

pub const GLYPH_CACHE: Key = Key {
    name: "glyph-cache",
    short: None,
    kind: Kind::Choice(&["minspec", "performance"]),
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-render",
    what: "How much memory vtium uses to cache drawn characters. When unset, `minspec` (small), or `performance` in a build with the `performance-cache` feature.",
    cite: "[the CPU compositor issue](https://github.com/vtium/vtium/issues/16)",
};

pub const DISPLAY_DEPTH: Key = Key {
    name: "display-depth",
    short: None,
    kind: Kind::Choice(&["auto", "1", "gray4", "gray16", "color"]),
    default: Default::Text("auto"),
    scope: Scope::Anywhere,
    levels: TOP_LOOK,
    reader: "vtium-render",
    what: "How many colors or shades each display can show.\n\n\
           `1` is black and white: each character cell is lit or dark by its colors, never by their brightness. `gray4` and `gray16` are for gray panels of 4 or 16 shades; vtium sends color and the panel converts it. `color` is full color. `auto` uses what the display reports.\n\n\
           [docs] For `auto`, an fbdev device that reports 1 bit gets `1`, and a known DRM panel driver gives its levels.",
    cite: "[the 1-bit displays issue](https://github.com/vtium/vtium/issues/135)",
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
    levels: TOP_LOOK,
    reader: "vtium",
    what: "The pixel density (DPI) used to turn `font-size` and a font set's `size=` from points into pixels, instead of the density each display reports.\n\n\
           When unset, vtium measures each display's density from the physical size it reports. If a display reports none, vtium assumes 96 DPI and logs a warning once for that display.",
    cite: "[the issue that renders glyphs in the font helper](https://github.com/vtium/vtium/issues/145)",
};

pub const RENDER_TIMER: Key = Key {
    name: "render-timer",
    short: None,
    kind: Kind::Choice(&["none", "osd", "log", "both"]),
    default: Default::Text("none"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-render",
    what: "Measure each display's frame times once a second, and write the figures to the log, show them on screen, or both.",
    cite: "[the CPU compositor issue](https://github.com/vtium/vtium/issues/16)",
};

pub const KEY_PASTE: Key = Key {
    name: "key-paste",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Shift>Insert"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The key that pastes the copied text, as the middle mouse button does; it pastes once per press.\n\n\
           With Shift, write the character Shift makes: `<Ctrl><Shift>V`, not `<Ctrl><Shift>v`. When empty, no key pastes.",
    cite: "xterm's `Shift <KeyPress> Insert` translation; [the selection issue](https://github.com/vtium/vtium/issues/95)",
};

pub const KEY_REBOOT: Key = Key {
    name: "key-reboot",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The key that reboots the machine.\n\n\
           When unset, no key reboots.\n\n\
           [docs] When `/proc/sys/kernel/ctrl-alt-del` is 0, vtium asks init to reboot, as Ctrl+Alt+Del does, so the services stop first; otherwise the kernel restarts at once.",
    cite: "the `grab-reboot` key that kmscon had",
};

pub const KEY_POWER_OFF: Key = Key {
    name: "key-power-off",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The key that powers the machine off.\n\n\
           When unset, no key powers off.\n\n\
           [docs] vtium asks init with `SIGRTMIN+4`, which systemd treats as `poweroff.target`, so the services stop and the filesystems are unmounted first.",
    cite: "[the configuration issue](https://github.com/vtium/vtium/issues/21)",
};

pub const KEY_SUSPEND: Key = Key {
    name: "key-suspend",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Unset,
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The key that suspends the machine to RAM.\n\n\
           vtium asks logind to suspend. When logind is not running, or vtium was built without `logind`, vtium writes `mem` to `/sys/power/state`. When unset, no key suspends.\n\n\
           [docs] Because vtium asks logind, anything that holds off sleep through logind can still do so.",
    cite: "[the configuration issue](https://github.com/vtium/vtium/issues/21)",
};

/// Where the keys vtium takes for itself come from (ADR-1005 D10).
const OWN_KEYS_CITE: &str = "[the kitty keyboard issue](https://github.com/vtium/vtium/issues/42)";

/// Where the console and focus keys come from (ADR-1005 D10, ADR-1401).
const HEAD_KEYS_CITE: &str = "[the heads issue](https://github.com/vtium/vtium/issues/156); [the kitty keyboard issue](https://github.com/vtium/vtium/issues/42)";

pub const KEY_SCROLL_UP: Key = Key {
    name: "key-scroll-up",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Shift>Up"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Scroll the view back one line.",
    cite: OWN_KEYS_CITE,
};

pub const KEY_SCROLL_DOWN: Key = Key {
    name: "key-scroll-down",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Shift>Down"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Scroll the view forward one line.",
    cite: OWN_KEYS_CITE,
};

pub const KEY_PAGE_UP: Key = Key {
    name: "key-page-up",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Shift>Prior"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Scroll the view back one page. `Prior` is Page Up.",
    cite: OWN_KEYS_CITE,
};

pub const KEY_PAGE_DOWN: Key = Key {
    name: "key-page-down",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Shift>Next"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Scroll the view forward one page. `Next` is Page Down.",
    cite: OWN_KEYS_CITE,
};

pub const KEY_PROMPT_UP: Key = Key {
    name: "key-prompt-up",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Ctrl><Shift>Up"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Scroll the view back to the shell prompt above, which OSC 133 marks.",
    cite: "[the OSC 133 prompt marks issue](https://github.com/vtium/vtium/issues/40); [the kitty keyboard issue](https://github.com/vtium/vtium/issues/42)",
};

pub const KEY_PROMPT_DOWN: Key = Key {
    name: "key-prompt-down",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Ctrl><Shift>Down"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Scroll the view forward to the shell prompt below, or to the live screen.",
    cite: "[the OSC 133 prompt marks issue](https://github.com/vtium/vtium/issues/40); [the kitty keyboard issue](https://github.com/vtium/vtium/issues/42)",
};

pub const KEY_ZOOM_IN: Key = Key {
    name: "key-zoom-in",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Ctrl>plus,<Ctrl>equal"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Make the font of the console on screen larger.",
    cite: OWN_KEYS_CITE,
};

pub const KEY_ZOOM_OUT: Key = Key {
    name: "key-zoom-out",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Ctrl>minus"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Make the font of the console on screen smaller.",
    cite: OWN_KEYS_CITE,
};

pub const KEY_ROTATE_CW: Key = Key {
    name: "key-rotate-cw",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Logo>plus,<Logo>equal"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Turn the head's displays a quarter turn clockwise. `Logo` is Super.",
    cite: OWN_KEYS_CITE,
};

pub const KEY_ROTATE_CCW: Key = Key {
    name: "key-rotate-ccw",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Logo>minus"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Turn the head's displays a quarter turn counterclockwise.",
    cite: OWN_KEYS_CITE,
};

pub const KEY_RESET: Key = Key {
    name: "key-reset",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Ctrl><Shift>Delete"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Reset the console on screen, as the `reset` command does.\n\n\
           It clears the screen and the scrollback, and turns off every mode a program set, the keyboard's too: use it when a program left the keyboard sending escape codes.",
    cite: OWN_KEYS_CITE,
};

pub const KEY_CONSOLE_NEXT: Key = Key {
    name: "key-console-next",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Ctrl><Logo>Right"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Show the head's next console.",
    cite: HEAD_KEYS_CITE,
};

pub const KEY_CONSOLE_PREVIOUS: Key = Key {
    name: "key-console-previous",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Ctrl><Logo>Left"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Show the head's previous console.",
    cite: HEAD_KEYS_CITE,
};

pub const KEY_CONSOLE_OPEN: Key = Key {
    name: "key-console-open",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Ctrl><Logo>Return"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Open a console after the one on screen, unless the policy is `closed`.",
    cite: HEAD_KEYS_CITE,
};

pub const KEY_CONSOLE_CLOSE: Key = Key {
    name: "key-console-close",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Ctrl><Logo>BackSpace"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Close the console on screen, unless it is the head's last or the policy is `closed`.",
    cite: HEAD_KEYS_CITE,
};

pub const KEY_FOCUS_NEXT: Key = Key {
    name: "key-focus-next",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Ctrl><Logo>Up"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Make the next head active, from a keyboard that roams among the heads.",
    cite: HEAD_KEYS_CITE,
};

pub const KEY_FOCUS_PREVIOUS: Key = Key {
    name: "key-focus-previous",
    short: None,
    kind: Kind::KeyBinding,
    default: Default::Text("<Ctrl><Logo>Down"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "Make the previous head active, from a keyboard that roams among the heads.",
    cite: HEAD_KEYS_CITE,
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
    levels: TOP,
    reader: "vtium-image",
    what: "The most memory each console uses for images, on both its screens together.\n\n\
           The value is in bytes, or in KiB, MiB or GiB with `K`, `M` or `G` (such as `64M`), up to 1 TiB. `auto` gives each console enough for four screenfuls of its head's displays, from 16 MiB to 192 MiB. `0` turns sixel and kitty graphics off.\n\n\
           `auto` recalculates when a display is added, removed or resized.\n\n\
           A build without image support accepts this key and shows no images.",
    cite: "[the image pipeline issue](https://github.com/vtium/vtium/issues/28); Ghostty's `image-storage-limit`",
};

pub const MAX_GRAPHIC_SIZE: Key = Key {
    name: "max-graphic-size",
    short: None,
    kind: Kind::Size { max: 65_535 },
    default: Default::Text("10000x10000"),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-image",
    what: "The largest image a program can send, in pixels (`WxH`), for sixel and kitty graphics alike. vtium refuses a larger one.\n\n\
           A build without image support accepts this key and shows no images.",
    cite: "[the image pipeline issue](https://github.com/vtium/vtium/issues/28); xterm's `maxGraphicSize`",
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
    levels: TOP,
    reader: "vtium-term",
    what: "How many colors a sixel image can use (its color registers), and the most a program can ask for with XTSMGRAPHICS.\n\n\
           A build without image support accepts this key and shows no images.",
    cite: "[the image pipeline issue](https://github.com/vtium/vtium/issues/28); xterm's `numColorRegisters`",
};

pub const PRIVATE_COLOR_REGISTERS: Key = Key {
    name: "private-color-registers",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Give each sixel image its own colors (mode 1070, after a reset).\n\n\
           When on, each sixel image starts from the default colors. When off, images share their colors. A build without image support accepts this key and shows no images.",
    cite: "[the image pipeline issue](https://github.com/vtium/vtium/issues/28); xterm's `privateColorRegisters`",
};

pub const IMAGE_HELPER: Key = Key {
    name: "image-helper",
    short: None,
    kind: Kind::Text,
    default: Default::Text(IMAGE_PROGRAM),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium",
    what: "The image helper: the program vtium starts, as the logged-in user, to read image files and shared memory for kitty graphics and to decode PNG and zlib-compressed images. vtium runs as root and does neither itself.\n\n\
           vtium opens the program once, when it starts, and runs every helper from that file, so a newly installed helper takes effect when vtium restarts.\n\n\
           vtium refuses a helper whose path is not absolute, that is not a regular file, that is setuid or setgid, that anyone but root or vtium's own account owns, or that its group or other users can write. It then logs why and refuses images: `EBADF` for files and shared memory, `EINVAL` for PNG and zlib. vtium starts the helper through `/proc/self/fd`, so `/proc` must be mounted.\n\n\
           A build without image support accepts this key and starts no helper.\n\n\
           [docs] The default is the path vtium was built with.",
    cite: "[the image helper issue](https://github.com/vtium/vtium/issues/31); the maintainer's decision of 2026-11-01",
};

pub const SIXEL: Key = Key {
    name: "sixel",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Show sixel images.\n\n\
           When off, vtium neither offers nor shows sixel: its device attributes (DA1) leave out attribute 4, and it ignores sixel data. vtium offers sixel only while `image-storage-limit` is above 0 and every display of the console's head can show images. A build without image support never offers it.",
    cite: "[the sixel issue](https://github.com/vtium/vtium/issues/48)",
};

pub const KITTY_GRAPHICS: Key = Key {
    name: "kitty-graphics",
    short: None,
    kind: Kind::Switch,
    default: Default::Switch(true),
    scope: Scope::Anywhere,
    levels: TOP,
    reader: "vtium-term",
    what: "Show images sent with the kitty graphics protocol.\n\n\
           When off, vtium neither offers nor shows them: it ignores graphics commands and does not answer queries. vtium offers it only while `image-storage-limit` is above 0 and every display of the console's head can show images. A build without image support never offers it.",
    cite: "[the kitty graphics issue](https://github.com/vtium/vtium/issues/49)",
};

/// Every key, in the order the generated files list them. A static, so
/// each use reads the one table instead of a copy of it.
pub static KEYS: [Key; 127] = [
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
    KERNEL_MESSAGES,
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
    KEY_PASTE,
    KEY_REBOOT,
    KEY_POWER_OFF,
    KEY_SUSPEND,
    KEY_SCROLL_UP,
    KEY_SCROLL_DOWN,
    KEY_PAGE_UP,
    KEY_PAGE_DOWN,
    KEY_PROMPT_UP,
    KEY_PROMPT_DOWN,
    KEY_ZOOM_IN,
    KEY_ZOOM_OUT,
    KEY_ROTATE_CW,
    KEY_ROTATE_CCW,
    KEY_RESET,
    KEY_CONSOLE_NEXT,
    KEY_CONSOLE_PREVIOUS,
    KEY_CONSOLE_OPEN,
    KEY_CONSOLE_CLOSE,
    KEY_FOCUS_NEXT,
    KEY_FOCUS_PREVIOUS,
    FONT_ENGINE,
    FONT_NAME,
    FONT_FRITS,
    FONT_PATHS,
    FONT_HELPER,
    FONT_SIZE,
    FONT_HINTING,
    FONT_LINE_SPACING,
    CELL,
    ALLOW_FONT_OPS,
    ALLOW_WINDOW_OPS,
    GLYPH_SOURCE,
    GLYPH_SOURCE_BOX,
    GLYPH_SOURCE_BLOCK,
    GLYPH_SOURCE_BRAILLE,
    GLYPH_SOURCE_SEXTANTS,
    GLYPH_SOURCE_OCTANTS,
    GLYPH_SOURCE_LEGACY,
    MISSING,
    TEXT_DIRECTION,
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
pub const SECTIONS: [Section; 11] = [
    Section {
        title: "Fonts and text",
        about: "The console's font and its size, who draws box-drawing and block characters, how blinking text looks, and which way text runs.",
        groups: &[
            Group {
                title: "Font",
                keys: &[&FONT_NAME, &FONT_FRITS, &FONT_PATHS, &FONT_ENGINE],
            },
            Group {
                title: "Font size and spacing",
                keys: &[&FONT_SIZE, &CELL, &FONT_HINTING, &FONT_LINE_SPACING],
            },
            Group {
                title: "Box drawing and block characters",
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
            Group {
                title: "Text area size set by programs",
                keys: &[&ALLOW_WINDOW_OPS],
            },
            Group {
                title: "Right-to-left text",
                keys: &[&TEXT_DIRECTION],
            },
        ],
    },
    Section {
        title: "Colors",
        about: "The sixteen basic colors, the default text and background colors, and the colors of underlines and strikethrough.",
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
                title: "Underline and strikethrough colors",
                keys: &[&UNDERLINE_COLOR, &STRIKE_COLOR, &FORCE_DECORATION_COLORS],
            },
        ],
    },
    Section {
        title: "Keyboard and mouse",
        about: "The keyboard layout and key repeat, the mouse and its pointer, and copy and paste.",
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
                title: "Mouse pointer",
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
                keys: &[&CLIPBOARD_WRITE, &KEY_PASTE],
            },
        ],
    },
    Section {
        title: "Key bindings",
        about: "The keys vtium acts on itself instead of sending them to the program. Each value lists key combinations, separated by commas, and an empty value hands the key to the program. The scroll keys and the prompt jumps are vtium's only on the main screen: on the alternate screen, where editors run, they reach the program. Ctrl+Alt+F1 to F12 always switch the VT.",
        groups: &[
            Group {
                title: "Scrolling and prompt jumps",
                keys: &[
                    &KEY_SCROLL_UP,
                    &KEY_SCROLL_DOWN,
                    &KEY_PAGE_UP,
                    &KEY_PAGE_DOWN,
                    &KEY_PROMPT_UP,
                    &KEY_PROMPT_DOWN,
                ],
            },
            Group {
                title: "Zoom, rotation and reset",
                keys: &[
                    &KEY_ZOOM_IN,
                    &KEY_ZOOM_OUT,
                    &KEY_ROTATE_CW,
                    &KEY_ROTATE_CCW,
                    &KEY_RESET,
                ],
            },
            Group {
                title: "Consoles and heads",
                keys: &[
                    &KEY_CONSOLE_NEXT,
                    &KEY_CONSOLE_PREVIOUS,
                    &KEY_CONSOLE_OPEN,
                    &KEY_CONSOLE_CLOSE,
                    &KEY_FOCUS_NEXT,
                    &KEY_FOCUS_PREVIOUS,
                ],
            },
        ],
    },
    Section {
        title: "Session and terminal",
        about: "The command each console runs, the terminal type it sees, and how much of its output vtium keeps.",
        groups: &[
            Group {
                title: "The console's command",
                keys: &[&LOGIN, &ONESHOT],
            },
            Group {
                title: "The console's terminal type",
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
                title: "Synchronized updates",
                keys: &[&SYNC_TIMEOUT],
            },
        ],
    },
    Section {
        title: "Consoles and the VT",
        about: "How many consoles the heads can have, and the VT vtium runs on.",
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
        about: "What vtium shows on top of the console, and in which colors: which head and console you are looking at, a notice about a problem, and the kernel's warnings.",
        groups: &[
            Group {
                title: "On-screen display",
                keys: &[&OSD, &OSD_TIMEOUT, &FOCUS_MARK],
            },
            Group {
                title: "Kernel messages",
                keys: &[&KERNEL_MESSAGES],
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
        about: "Each display's resolution, colors and density, how long vtium waits for a display when it starts, and how it draws to them.",
        groups: &[
            Group {
                title: "Display resolution",
                keys: &[&USE_ORIGINAL_MODE, &MODE],
            },
            Group {
                title: "Display colors and density",
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
                title: "Drawing with the GPU",
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
                keys: &[&KEY_REBOOT, &KEY_POWER_OFF, &KEY_SUSPEND],
            },
        ],
    },
    Section {
        title: "Images",
        about: "Sixel and kitty graphics, how much memory images use, which protocols vtium offers, and the helper that reads and decodes them.",
        groups: &[
            Group {
                title: "Image memory and protocols",
                keys: &[
                    &IMAGE_STORAGE_LIMIT,
                    &SIXEL,
                    &KITTY_GRAPHICS,
                    &MAX_GRAPHIC_SIZE,
                ],
            },
            Group {
                title: "Sixel colors",
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
        about: "Where vtium finds its configuration, its log, the font helper that draws a font's characters, and the character cache.",
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
                title: "Font helper",
                keys: &[&FONT_HELPER],
            },
            Group {
                title: "Character cache",
                keys: &[&GLYPH_CACHE],
            },
        ],
    },
];

/// The keys vtium takes and does not act on yet, which the crate
/// README's "Not yet built" lists; `check::semantics` warns when one is
/// set.
pub const NOT_BUILT: [&Key; 1] = [&FONT_HINTING];

/// The names four key bindings had before the maintainer renamed them to
/// more accepted terms (ADR-1005 D10): vtium still reads each old name as
/// its key binding, and reports each line or option that uses it. The
/// keys that never shipped under a `grab-` name have none.
pub const RENAMED: [(&str, &Key); 4] = [
    ("grab-paste", &KEY_PASTE),
    ("grab-reboot", &KEY_REBOOT),
    ("grab-power-off", &KEY_POWER_OFF),
    ("grab-suspend", &KEY_SUSPEND),
];

/// The key binding the old name `old` names now, with that old name.
pub fn renamed(old: &str) -> Option<(&'static str, &'static Key)> {
    RENAMED
        .iter()
        .find(|(o, _)| *o == old)
        .map(|&(o, k)| (o, k))
}

/// The keys a section of `level` may set, in the table's order.
pub fn at(level: Level) -> impl Iterator<Item = &'static Key> {
    KEYS.iter().filter(move |k| k.levels.contains(level))
}

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

    // ADR-1005 D10: the seventeen keys vtium takes for itself, apart from
    // the VT keys and the rescue key, are key bindings of vtium.conf's top
    // level, each with the combination vtium took before the keys were
    // configurable as its built-in default, so vtium with no vtium.conf
    // takes the same keys.
    #[test]
    fn vtiums_own_keys_are_key_bindings_with_their_built_in_defaults() {
        let want = [
            ("key-scroll-up", "<Shift>Up"),
            ("key-scroll-down", "<Shift>Down"),
            ("key-page-up", "<Shift>Prior"),
            ("key-page-down", "<Shift>Next"),
            ("key-prompt-up", "<Ctrl><Shift>Up"),
            ("key-prompt-down", "<Ctrl><Shift>Down"),
            ("key-zoom-in", "<Ctrl>plus,<Ctrl>equal"),
            ("key-zoom-out", "<Ctrl>minus"),
            ("key-rotate-cw", "<Logo>plus,<Logo>equal"),
            ("key-rotate-ccw", "<Logo>minus"),
            ("key-reset", "<Ctrl><Shift>Delete"),
            ("key-console-next", "<Ctrl><Logo>Right"),
            ("key-console-previous", "<Ctrl><Logo>Left"),
            ("key-console-open", "<Ctrl><Logo>Return"),
            ("key-console-close", "<Ctrl><Logo>BackSpace"),
            ("key-focus-next", "<Ctrl><Logo>Up"),
            ("key-focus-previous", "<Ctrl><Logo>Down"),
        ];
        for (name, default) in want {
            let key = find(name).unwrap_or(&HELP);
            assert_eq!(key.name, name);
            assert_eq!(key.kind, Kind::KeyBinding, "{name}");
            assert_eq!(key.default, Default::Text(default), "{name}");
            assert_eq!(key.scope, Scope::Anywhere, "{name}");
            assert_eq!(key.levels, TOP, "{name}");
            assert_eq!(key.reader, "vtium", "{name}");
        }
        // with key-paste and the power keys, every key binding there is
        let bindings = KEYS.iter().filter(|k| k.kind == Kind::KeyBinding).count();
        assert_eq!(bindings, want.len() + 4);
        // The VT keys and the rescue key have no key to bind or empty.
        for k in &KEYS {
            assert!(
                !k.name.contains("vt-") && !k.name.contains("rescue"),
                "{}",
                k.name
            );
        }
    }

    // ADR-1004 D5, D13: `text-direction` takes `ltr`, the default, `rtl`
    // and `auto`, at vtium.conf's top level and on the command line, and
    // says that a build without the bidi feature accepts it and does
    // nothing with it.
    #[test]
    fn text_direction_takes_ltr_rtl_or_auto_and_defaults_to_ltr() {
        let key = find("text-direction").unwrap_or(&HELP);
        assert_eq!(key.name, "text-direction");
        assert_eq!(key.kind, Kind::Choice(&["ltr", "rtl", "auto"]));
        assert_eq!(key.default, Default::Text("ltr"));
        assert_eq!(key.tree_default(), Default::Text("ltr"));
        assert_eq!(key.scope, Scope::Anywhere);
        assert_eq!(key.levels, Levels::TOP);
        assert_eq!(key.reader, "vtium-term");
        assert!(
            key.what
                .contains("A build without right-to-left support (the `bidi` feature) accepts this key and does nothing with it."),
            "{}",
            key.what
        );
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
    fn a_paragraph_marked_for_the_docs_is_left_out_of_the_example_only() {
        let mut marked = 0;
        for k in &KEYS {
            assert!(
                !k.what.starts_with(DOCS_ONLY),
                "{}: the first paragraph says what the key does and is never docs-only",
                k.name
            );
            let all: Vec<&str> = k.paragraphs().collect();
            let shown: Vec<&str> = k.example_paragraphs().collect();
            assert!(shown.len() <= all.len());
            marked += all.len() - shown.len();
            for p in &all {
                assert!(!p.starts_with(DOCS_ONLY), "{}: a doubled mark", k.name);
            }
        }
        assert!(marked > 0, "no key marks a paragraph for the docs");
    }

    #[test]
    fn a_description_on_one_line_joins_its_paragraphs_with_a_space() {
        assert_eq!(
            BELL.what_line(),
            "Ring the console's bell when a program sends BEL. A program can silence its own bells by resetting mode 1042, which starts set. When off, no bell rings."
        );
        assert_eq!(BELL.paragraphs().count(), 2);
        assert_eq!(MOUSE.what_line(), MOUSE.what);
    }

    // ADR-1403 D5: a look takes the font and display keys ADR-1200 D16
    // gives a look and its cell, a head takes `login`, every other key is
    // the top level's alone, and a key the command line alone sets has no
    // level. `cell` is the one key only a section sets.
    #[test]
    fn each_key_names_the_levels_that_may_set_it() {
        let names = |level| at(level).map(|k| k.name).collect::<Vec<_>>();
        assert_eq!(
            names(Level::Look),
            [
                "font-engine",
                "font-name",
                "font-size",
                "font-hinting",
                "font-line-spacing",
                "cell",
                "display-depth",
                "display-dpi",
            ]
        );
        assert_eq!(names(Level::Head), ["login"]);
        for k in &KEYS {
            match k.scope {
                Scope::CommandLine => assert_eq!(k.levels, NONE, "{}", k.name),
                Scope::Anywhere => assert!(k.levels.contains(Level::Top), "{}", k.name),
                Scope::Section => {
                    assert!(!k.levels.contains(Level::Top), "{}", k.name);
                    assert_ne!(k.levels, NONE, "{}", k.name);
                }
            }
        }
        assert_eq!(CELL.scope, Scope::Section);
        assert_eq!(CELL.levels, LOOK);
        // the process's own keys stay at the top level (D5)
        for k in [
            &DISPLAY_BUFFERS,
            &CONSOLES_MAX,
            &HWACCEL,
            &SB_MEMORY,
            &FONT_FRITS,
        ] {
            assert_eq!(k.levels, TOP, "{}", k.name);
        }
    }

    // ADR-1200 D1, ADR-1403 D5: a look's cell takes a frit's `cell=`
    // syntax and bounds, 6 to 96 pixels a side with no leading zero.
    #[test]
    fn a_cell_is_read_as_a_frits_cell_line() {
        assert_eq!(parse_cell("8x16"), Some((8, 16)));
        assert_eq!(parse_cell("6x96"), Some((6, 96)));
        for bad in [
            "", "8", "8x", "x16", "5x16", "8x97", "08x16", "8x016", "+8x16", "8X16", "8x16x2",
            " 8x16", "8 x16", "-8x16",
        ] {
            assert_eq!(parse_cell(bad), None, "{bad:?}");
        }
    }

    // The section reader checks a section key's value by its kind, and
    // reads text, a command, a path, a choice, a number and a cell.
    #[test]
    fn every_section_key_is_of_a_kind_the_section_reader_checks() {
        for k in at(Level::Look).chain(at(Level::Head)) {
            assert!(
                matches!(
                    k.kind,
                    Kind::Text
                        | Kind::Command
                        | Kind::Path
                        | Kind::Choice(_)
                        | Kind::Number { .. }
                        | Kind::Cell
                ),
                "{}",
                k.name
            );
        }
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
