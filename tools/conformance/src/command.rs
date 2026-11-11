//! Test commands: `ESC _ tsm-test;` NAME, then `;KEY=VALUE` fields, then
//! `ESC \` (INTERFACE.md, "Test commands"). Parsing checks every rule the
//! interface gives for the text and its values; a command that breaks one
//! is a usage failure, exit 2.

use vtium_term::PALETTE_NAMES;

use crate::{Failure, usage};

/// The test command's introducer.
pub const INTRO: &[u8] = b"\x1b_tsm-test;";
/// The test command's terminator, ST.
pub const END: &[u8] = b"\x1b\\";
/// The most bytes a command's text holds.
pub const TEXT_MAX: usize = 511;

/// A pointer button, as the `mouse` command names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    None,
}

/// A pointer event, as the `mouse` command names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Press,
    Release,
    Move,
}

/// What the `select` command does, as its `how` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    Start,
    To,
    Word,
    Line,
    Output,
    Clear,
}

/// A test command vtium runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Resize {
        cols: usize,
        rows: usize,
    },
    SbMax(u64),
    SbUp(u64),
    SbDown(u64),
    SbReset,
    Sync(bool),
    SyncFrames,
    Version(String),
    Termcap(String),
    Key {
        sym: u32,
        ascii: u32,
        unicode: Option<u32>,
        mods: u8,
    },
    Mouse {
        col: u64,
        row: u64,
        px: u32,
        py: u32,
        button: Button,
        event: Event,
        mods: u8,
    },
    Paste(Vec<u8>),
    Focus(bool),
    Palette(&'static str),
    FrameSince(u8),
    Clipboard,
    /// A selection at a cell of the view; `clear` names no cell.
    Select {
        how: How,
        col: u64,
        row: u64,
    },
    Copy,
    Prompt {
        up: bool,
    },
    /// The link of a cell of the view.
    Link {
        col: u64,
        row: u64,
    },
    /// The pointer over a cell of the view, with Ctrl held or not; `None`
    /// takes the pointer away.
    Hover {
        at: Option<(u64, u64)>,
        ctrl: bool,
    },
    Backspace {
        del: bool,
    },
    State,
    Cell {
        width: u32,
        height: u32,
    },
    Limit(u64),
    Image(Image),
    Place(Place),
    Sixel(bool),
    GraphicMax {
        width: u32,
        height: u32,
    },
    Registers {
        count: u32,
        private: bool,
    },
    KittyGraphics(bool),
    Host(bool),
}

/// The `image` command's keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Image {
    /// The number later commands call it by, 0 to 63.
    pub n: u8,
    pub width: u32,
    pub height: u32,
    pub kitty: bool,
    pub id: u32,
    pub transient: bool,
    /// The premultiplied color of every pixel, AARRGGBB.
    pub fill: u32,
}

/// The `place` command's keys; `col` and `row` are the cursor's when
/// absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub img: u8,
    pub col: Option<u32>,
    pub row: Option<u32>,
    pub x: u32,
    pub y: u32,
    pub cols: u32,
    pub rows: u32,
    pub src_x: u32,
    pub src_y: u32,
    pub src_width: u32,
    pub src_height: u32,
    pub z: i32,
    pub id: u32,
}

/// Whether vtium-replay is built with the image pipeline, which offers the
/// image commands and their features.
pub const IMAGES: bool = cfg!(feature = "images");

/// A command of interface 1: its name, the feature it needs (none for the
/// commands every implementation has), its keys, and whether vtium runs it.
pub struct Spec {
    pub name: &'static str,
    pub feature: Option<&'static str>,
    keys: &'static [&'static str],
    pub offered: bool,
}

/// Every command of interface 1.6, in the order of its describe blocks:
/// INTERFACE.md's, then vtium's additions (INTERFACE-vtium.md). The image
/// commands are offered in a build with images.
pub const SPECS: &[Spec] = &[
    spec("resize", None, &["c", "r"], true),
    spec("sb-max", None, &["n"], true),
    spec("sb-up", None, &["n"], true),
    spec("sb-down", None, &["n"], true),
    spec("sb-reset", None, &[], true),
    spec("cell", Some("images"), &["w", "h"], IMAGES),
    spec("limit", Some("images"), &["b"], IMAGES),
    spec(
        "image",
        Some("images"),
        &["n", "w", "h", "kitty", "id", "tr", "fill"],
        IMAGES,
    ),
    spec(
        "place",
        Some("images"),
        &[
            "img", "col", "row", "x", "y", "c", "r", "sx", "sy", "sw", "sh", "z", "p",
        ],
        IMAGES,
    ),
    spec("sixel", Some("sixel"), &["on"], IMAGES),
    spec("graphic-max", Some("sixel"), &["w", "h"], IMAGES),
    spec("registers", Some("sixel"), &["n", "private"], IMAGES),
    spec("kitty-graphics", Some("kitty-graphics"), &["on"], IMAGES),
    spec("host", Some("image-host"), &["on"], IMAGES),
    spec("sync", Some("sync"), &["on"], true),
    spec("sync-frames", Some("sync"), &[], true),
    spec("version", Some("identify"), &["text"], true),
    spec("termcap", Some("identify"), &["file"], true),
    spec(
        "key",
        Some("input"),
        &["sym", "ascii", "unicode", "mods"],
        true,
    ),
    spec(
        "mouse",
        Some("input"),
        &["col", "row", "px", "py", "button", "event", "mods"],
        true,
    ),
    spec("paste", Some("input"), &["hex"], true),
    spec("focus", Some("focus"), &["in"], true),
    spec("palette", Some("palette"), &["name"], true),
    spec("frame-since", Some("query"), &["caller"], true),
    spec("clipboard", Some("clipboard"), &[], true),
    spec("select", Some("selection"), &["how", "col", "row"], true),
    spec("copy", Some("selection"), &[], true),
    spec("prompt", Some("marks"), &["dir"], true),
    spec("link", Some("links"), &["col", "row"], true),
    spec("hover", Some("links"), &["col", "row", "ctrl"], true),
    spec("backspace", Some("backspace"), &["del"], true),
    spec("state", Some("state"), &[], true),
];

const fn spec(
    name: &'static str,
    feature: Option<&'static str>,
    keys: &'static [&'static str],
    offered: bool,
) -> Spec {
    Spec {
        name,
        feature,
        keys,
        offered,
    }
}

/// A command's fields, checked against its spec.
struct Fields<'a> {
    name: &'static str,
    fields: Vec<(&'a str, &'a str)>,
}

impl<'a> Fields<'a> {
    fn fail(&self, key: &str, what: &str) -> Failure {
        usage(format!("test command {}: {key}: {what}", self.name))
    }

    fn get(&self, key: &str) -> Option<&'a str> {
        self.fields.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    fn required(&self, key: &str) -> Result<&'a str, Failure> {
        self.get(key).ok_or_else(|| self.fail(key, "missing"))
    }

    /// KEY as a number from 0 to `max` in `radix`, `default` when absent
    /// (`None` makes it required).
    fn number(
        &self,
        key: &str,
        radix: u32,
        max: u64,
        default: Option<u64>,
    ) -> Result<u64, Failure> {
        let Some(v) = self.get(key) else {
            return default.ok_or_else(|| self.fail(key, "missing"));
        };
        number(v, radix, max).ok_or_else(|| self.fail(key, "not a number in range"))
    }

    fn decimal(&self, key: &str, max: u64, default: Option<u64>) -> Result<u64, Failure> {
        self.number(key, 10, max, default)
    }

    fn hex(&self, key: &str, max: u64, default: Option<u64>) -> Result<u64, Failure> {
        self.number(key, 16, max, default)
    }

    /// KEY as a signed decimal: an optional `-`, then digits with no
    /// leading zero, from -2147483648 to 2147483647, `-0` refused.
    fn signed(&self, key: &str, default: i32) -> Result<i32, Failure> {
        let Some(v) = self.get(key) else {
            return Ok(default);
        };
        let bad = || self.fail(key, "not a signed number in range");
        let (negative, digits) = match v.strip_prefix('-') {
            Some(d) => (true, d),
            None => (false, v),
        };
        let magnitude = number(digits, 10, 2_147_483_648).ok_or_else(bad)?;
        if negative && magnitude == 0 {
            return Err(bad());
        }
        let n = if negative {
            0i64.checked_sub(i64::try_from(magnitude).map_err(|_| bad())?)
        } else {
            i64::try_from(magnitude).ok()
        };
        n.and_then(|n| i32::try_from(n).ok()).ok_or_else(bad)
    }

    fn flag(&self, key: &str, default: Option<bool>) -> Result<bool, Failure> {
        Ok(self.decimal(key, 1, default.map(u64::from))? == 1)
    }

    fn name_of<T: Copy>(&self, key: &str, names: &[(&str, T)]) -> Result<T, Failure> {
        let v = self.required(key)?;
        names
            .iter()
            .find(|(n, _)| *n == v)
            .map(|(_, t)| *t)
            .ok_or_else(|| self.fail(key, "not a name the command takes"))
    }
}

/// `s` as a number from 0 to `max` in `radix`: decimal with no sign or
/// leading zero, or hex digits of either case.
pub fn number(s: &str, radix: u32, max: u64) -> Option<u64> {
    if s.is_empty() || (radix == 10 && s.len() > 1 && s.starts_with('0')) {
        return None;
    }
    let mut n: u64 = 0;
    for c in s.chars() {
        let d = c.to_digit(radix)?;
        n = n.checked_mul(u64::from(radix))?.checked_add(u64::from(d))?;
    }
    (n <= max).then_some(n)
}

/// The bytes a hex value spells, two digits each, none of them 00.
pub fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    s.as_bytes()
        .chunks(2)
        .map(|pair| {
            let pair = core::str::from_utf8(pair).ok()?;
            u8::from_str_radix(pair, 16).ok().filter(|&b| b != 0)
        })
        .collect()
}

/// Parses a command's text, the bytes between `tsm-test;` and `ESC \`.
pub fn parse(text: &[u8]) -> Result<Command, Failure> {
    if text.len() > TEXT_MAX {
        return Err(usage(format!("test command over {TEXT_MAX} bytes")));
    }
    if let Some(b) = text.iter().find(|b| !(0x20..=0x7e).contains(*b)) {
        return Err(usage(format!("test command holds byte 0x{b:02x}")));
    }
    let text = core::str::from_utf8(text).map_err(|_| usage("test command is not ASCII"))?;
    if text.starts_with(';') {
        return Err(usage("test command starts with ;"));
    }
    let mut parts = text.split(';').filter(|p| !p.is_empty());
    let name = parts.next().ok_or_else(|| usage("empty test command"))?;
    let spec = SPECS
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| usage(format!("unknown test command {name}")))?;
    if !spec.offered {
        return Err(usage(format!(
            "test command {name} needs feature {}, which vtium lacks",
            spec.feature.unwrap_or("-")
        )));
    }
    let mut f = Fields {
        name: spec.name,
        fields: Vec::new(),
    };
    for part in parts {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| f.fail(part, "has no value"))?;
        if value.contains('=') {
            return Err(f.fail(key, "has = in its value"));
        }
        if !spec.keys.contains(&key) {
            return Err(f.fail(key, "is not a key the command takes"));
        }
        if f.get(key).is_some() {
            return Err(f.fail(key, "given twice"));
        }
        f.fields.push((key, value));
    }
    command(&f)
}

fn command(f: &Fields<'_>) -> Result<Command, Failure> {
    const U32: u64 = 4_294_967_295;
    Ok(match f.name {
        "resize" => Command::Resize {
            cols: to_usize(f.decimal("c", 2048, None)?)
                .filter(|&c| c > 0)
                .ok_or_else(|| f.fail("c", "out of range"))?,
            rows: to_usize(f.decimal("r", 1024, None)?)
                .filter(|&r| r > 0)
                .ok_or_else(|| f.fail("r", "out of range"))?,
        },
        "sb-max" => Command::SbMax(f.decimal("n", U32, None)?),
        "sb-up" => Command::SbUp(f.decimal("n", U32, Some(1))?),
        "sb-down" => Command::SbDown(f.decimal("n", U32, Some(1))?),
        "sb-reset" => Command::SbReset,
        "sync" => Command::Sync(f.flag("on", Some(true))?),
        "sync-frames" => Command::SyncFrames,
        "clipboard" => Command::Clipboard,
        "version" => {
            let text = f.required("text")?;
            if text.len() > 64 {
                return Err(f.fail("text", "over 64 bytes"));
            }
            Command::Version(text.to_owned())
        }
        "termcap" => Command::Termcap(fixture(f, "file")?.to_owned()),
        "key" => Command::Key {
            sym: to_u32(f.hex("sym", 0x1fff_ffff, None)?),
            ascii: to_u32(f.hex("ascii", 0x1fff_ffff, Some(0))?),
            unicode: match f.get("unicode") {
                Some(_) => Some(to_u32(f.hex("unicode", 0x10_ffff, None)?)),
                None => None,
            },
            mods: to_u8(f.decimal("mods", 255, Some(0))?),
        },
        "mouse" => {
            let button = f.name_of(
                "button",
                &[
                    ("left", Button::Left),
                    ("middle", Button::Middle),
                    ("right", Button::Right),
                    ("wheel-up", Button::WheelUp),
                    ("wheel-down", Button::WheelDown),
                    ("none", Button::None),
                ],
            )?;
            let event = f.name_of(
                "event",
                &[
                    ("press", Event::Press),
                    ("release", Event::Release),
                    ("move", Event::Move),
                ],
            )?;
            match (event, button) {
                (Event::Move, Button::WheelUp | Button::WheelDown) => {
                    return Err(f.fail("button", "a wheel in a move"));
                }
                (Event::Press | Event::Release, Button::None) => {
                    return Err(f.fail("button", "none in a press or release"));
                }
                _ => {}
            }
            Command::Mouse {
                // the screen's size bounds these when the command runs
                col: f.decimal("col", u64::MAX, None)?,
                row: f.decimal("row", u64::MAX, None)?,
                px: to_u32(f.decimal("px", 65535, Some(0))?),
                py: to_u32(f.decimal("py", 65535, Some(0))?),
                button,
                event,
                mods: to_u8(f.decimal("mods", 31, Some(0))?),
            }
        }
        "paste" => Command::Paste(
            hex_bytes(f.required("hex")?)
                .ok_or_else(|| f.fail("hex", "not hex bytes, or holds 00"))?,
        ),
        "focus" => Command::Focus(f.flag("in", None)?),
        "select" => {
            let how = f.name_of(
                "how",
                &[
                    ("start", How::Start),
                    ("to", How::To),
                    ("word", How::Word),
                    ("line", How::Line),
                    ("output", How::Output),
                    ("clear", How::Clear),
                ],
            )?;
            if how == How::Clear {
                if let Some(key) = ["col", "row"].into_iter().find(|k| f.get(k).is_some()) {
                    return Err(f.fail(key, "names a cell, which clear does not take"));
                }
                Command::Select {
                    how,
                    col: 0,
                    row: 0,
                }
            } else {
                Command::Select {
                    how,
                    // the view's size bounds these when the command runs
                    col: f.decimal("col", u64::MAX, None)?,
                    row: f.decimal("row", u64::MAX, None)?,
                }
            }
        }
        "copy" => Command::Copy,
        "prompt" => Command::Prompt {
            up: f.name_of("dir", &[("up", true), ("down", false)])?,
        },
        "link" => Command::Link {
            // the view's size bounds these when the command runs
            col: f.decimal("col", u64::MAX, None)?,
            row: f.decimal("row", u64::MAX, None)?,
        },
        "hover" => {
            let ctrl = f.flag("ctrl", Some(false))?;
            match (f.get("col"), f.get("row")) {
                (None, None) => Command::Hover { at: None, ctrl },
                _ => Command::Hover {
                    // the view's size bounds these when the command runs
                    at: Some((
                        f.decimal("col", u64::MAX, None)?,
                        f.decimal("row", u64::MAX, None)?,
                    )),
                    ctrl,
                },
            }
        }
        "backspace" => Command::Backspace {
            del: f.flag("del", None)?,
        },
        "state" => Command::State,
        "palette" => Command::Palette(f.name_of("name", &PALETTE_NAMES.map(|n| (n, n)))?),
        "frame-since" => Command::FrameSince(to_u8(f.decimal("caller", 7, None)?)),
        "cell" => Command::Cell {
            width: to_u32(f.decimal("w", U32, None)?),
            height: to_u32(f.decimal("h", U32, None)?),
        },
        "limit" => Command::Limit(f.decimal("b", u64::MAX, None)?),
        "image" => {
            let n = f.decimal("n", 63, Some(0))?;
            let width = f.decimal("w", 65535, Some(0))?;
            let height = f.decimal("h", 65535, Some(0))?;
            if width.saturating_mul(height) > 16_777_216 {
                return Err(f.fail("w", "times h is over 16777216"));
            }
            Command::Image(Image {
                n: to_u8(n),
                width: to_u32(width),
                height: to_u32(height),
                kitty: f.flag("kitty", Some(false))?,
                id: to_u32(f.decimal("id", U32, Some(n))?),
                transient: f.flag("tr", Some(false))?,
                fill: to_u32(f.hex("fill", U32, Some(0xff80_8080))?),
            })
        }
        "place" => {
            let at = |key: &str| -> Result<Option<u32>, Failure> {
                match f.get(key) {
                    Some(_) => Ok(Some(to_u32(f.decimal(key, U32, None)?))),
                    None => Ok(None),
                }
            };
            let n = |key: &str| f.decimal(key, U32, Some(0)).map(to_u32);
            Command::Place(Place {
                img: to_u8(f.decimal("img", 63, Some(0))?),
                col: at("col")?,
                row: at("row")?,
                x: n("x")?,
                y: n("y")?,
                cols: n("c")?,
                rows: n("r")?,
                src_x: n("sx")?,
                src_y: n("sy")?,
                src_width: n("sw")?,
                src_height: n("sh")?,
                z: f.signed("z", 0)?,
                id: n("p")?,
            })
        }
        "sixel" => Command::Sixel(f.flag("on", Some(true))?),
        "graphic-max" => Command::GraphicMax {
            width: to_u32(f.decimal("w", U32, None)?),
            height: to_u32(f.decimal("h", U32, None)?),
        },
        "registers" => Command::Registers {
            count: to_u32(f.decimal("n", U32, None)?),
            private: f.flag("private", Some(true))?,
        },
        "kitty-graphics" => Command::KittyGraphics(f.flag("on", Some(true))?),
        "host" => Command::Host(f.flag("on", Some(true))?),
        _ => return Err(usage(format!("unknown test command {}", f.name))),
    })
}

/// KEY as a fixture's name: 1 to 255 bytes of letters, digits, `.`, `_`
/// and `-`, not starting with `.`, so it names a file in the fixture
/// directory and nothing outside it.
fn fixture<'a>(f: &Fields<'a>, key: &str) -> Result<&'a str, Failure> {
    let v = f.required(key)?;
    let ok = (1..=255).contains(&v.len())
        && !v.starts_with('.')
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if ok {
        Ok(v)
    } else {
        Err(f.fail(key, "not a fixture name"))
    }
}

// Each value was checked against a range that fits the target type.
fn to_u32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn to_u8(n: u64) -> u8 {
    u8::try_from(n).unwrap_or(u8::MAX)
}

fn to_usize(n: u64) -> Option<usize> {
    usize::try_from(n).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(text: &str) -> Option<u8> {
        parse(text.as_bytes()).err().map(|f| f.code)
    }

    #[test]
    fn a_well_formed_command_parses() {
        assert_eq!(
            parse(b"key;sym=62;unicode=62"),
            Ok(Command::Key {
                sym: 0x62,
                ascii: 0,
                unicode: Some(0x62),
                mods: 0
            })
        );
        assert_eq!(parse(b"sb-up;;"), Ok(Command::SbUp(1)));
        assert_eq!(
            parse(b"resize;c=2048;r=1024"),
            Ok(Command::Resize {
                cols: 2048,
                rows: 1024
            })
        );
        assert_eq!(parse(b"palette;name=nord"), Ok(Command::Palette("nord")));
    }

    // INTERFACE-vtium.md: the selection, the prompt jumps, the links, the
    // BackSpace setting and the state
    #[test]
    fn the_host_commands_of_interface_1_5_parse() {
        assert_eq!(
            parse(b"select;how=word;col=3;row=1"),
            Ok(Command::Select {
                how: How::Word,
                col: 3,
                row: 1
            })
        );
        assert_eq!(
            parse(b"select;how=clear"),
            Ok(Command::Select {
                how: How::Clear,
                col: 0,
                row: 0
            })
        );
        for (how, want) in [
            ("start", How::Start),
            ("to", How::To),
            ("line", How::Line),
            ("output", How::Output),
        ] {
            let text = format!("select;how={how};col=0;row=0");
            assert_eq!(
                parse(text.as_bytes())
                    .map(|c| matches!(c, Command::Select { how, .. } if how == want)),
                Ok(true),
                "{text}"
            );
        }
        assert_eq!(parse(b"copy"), Ok(Command::Copy));
        assert_eq!(parse(b"prompt;dir=up"), Ok(Command::Prompt { up: true }));
        assert_eq!(parse(b"prompt;dir=down"), Ok(Command::Prompt { up: false }));
        assert_eq!(
            parse(b"link;col=4;row=1"),
            Ok(Command::Link { col: 4, row: 1 })
        );
        assert_eq!(
            parse(b"hover;col=4;row=1;ctrl=1"),
            Ok(Command::Hover {
                at: Some((4, 1)),
                ctrl: true
            })
        );
        assert_eq!(
            parse(b"hover;col=0;row=0"),
            Ok(Command::Hover {
                at: Some((0, 0)),
                ctrl: false
            })
        );
        assert_eq!(
            parse(b"hover"),
            Ok(Command::Hover {
                at: None,
                ctrl: false
            })
        );
        assert_eq!(
            parse(b"backspace;del=1"),
            Ok(Command::Backspace { del: true })
        );
        assert_eq!(
            parse(b"backspace;del=0"),
            Ok(Command::Backspace { del: false })
        );
        assert_eq!(parse(b"state"), Ok(Command::State));
    }

    #[cfg(feature = "images")]
    #[test]
    fn the_image_commands_parse_with_their_defaults() {
        assert_eq!(
            parse(b"image;n=2;w=24;h=32;kitty=1;fill=ff00ff00"),
            Ok(Command::Image(Image {
                n: 2,
                width: 24,
                height: 32,
                kitty: true,
                id: 2,
                transient: false,
                fill: 0xff00_ff00,
            }))
        );
        let Ok(Command::Place(p)) = parse(b"place;img=1;col=3;z=-1073741825") else {
            panic!("place parses");
        };
        assert_eq!(
            (p.img, p.col, p.row, p.z),
            (1, Some(3), None, -1_073_741_825)
        );
        assert_eq!(
            parse(b"place;z=-2147483648").map(|c| matches!(c, Command::Place(_))),
            Ok(true)
        );
    }

    #[test]
    fn each_broken_rule_is_a_usage_failure() {
        for text in [
            ";resize;c=1;r=1",
            "",
            "bogus",
            "resize;c=1",
            "resize;c=1;r=1;r=1",
            "resize;c=01;r=1",
            "resize;c=0;r=1",
            "resize;c=2049;r=1",
            "resize;c=1;r=1;x=1",
            "sb-up;n",
            "sb-up;n=1=2",
            "sync;on=2",
            "key;sym=g",
            "key;sym=62;unicode=110000",
            "mouse;col=0;row=0;button=wheel-up;event=move",
            "mouse;col=0;row=0;button=none;event=press",
            "paste;hex=00",
            "paste;hex=abc",
            "termcap;file=.hidden",
            "termcap;file=a/b",
            "frame-since;caller=8",
            "palette;name=nope",
            "image;w=65535;h=65535",
            "place;z=-0",
            "place;z=2147483648",
            "place;z=01",
            "image;n=64",
            "select;how=word",
            "select;how=word;col=1",
            "select;how=clear;col=0;row=0",
            "select;how=drag;col=0;row=0",
            "select;col=0;row=0",
            "copy;n=1",
            "prompt",
            "prompt;dir=left",
            "link",
            "link;col=1",
            "link;col=1;row=1;ctrl=1",
            "hover;col=1",
            "hover;row=1",
            "hover;col=1;row=1;ctrl=2",
            "backspace",
            "backspace;del=2",
            "state;x=1",
        ] {
            assert_eq!(code(text), Some(2), "{text:?}");
        }
        // without images the image commands are absent
        for text in ["image;w=1;h=1", "cell;w=8;h=16"] {
            assert_eq!(code(text), (!IMAGES).then_some(2), "{text:?}");
        }
        assert_eq!(code("sb-up;n=1\u{1}"), Some(2));
        assert_eq!(code(&"x".repeat(TEXT_MAX + 1)), Some(2));
    }

    #[test]
    fn numbers_follow_their_forms() {
        assert_eq!(number("0", 10, 5), Some(0));
        assert_eq!(number("05", 10, 5), None);
        assert_eq!(number("6", 10, 5), None);
        assert_eq!(number("-1", 10, 5), None);
        assert_eq!(number("0F", 16, 15), Some(15));
        assert_eq!(number("18446744073709551616", 10, u64::MAX), None);
        assert_eq!(hex_bytes("1b5b"), Some(vec![0x1b, 0x5b]));
    }
}
