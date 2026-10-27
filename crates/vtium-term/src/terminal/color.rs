//! The colour OSC strings: OSC 4 and 104 for the palette, OSC 10 to 19 and
//! 110 to 119 for the defaults and the cursor, pointer and selection
//! colours, as xterm's ctlseqs defines them, and OSC 21, kitty's colour
//! protocol (`color-stack`).
//!
//! A set changes the terminal's palette, the copy of the configured one
//! that frames draw through; a reset returns a colour to the configured
//! palette's. A string that changes any colour marks every cell changed,
//! as each may draw in it. Each query's reply follows the reply rule
//! (`reply.rs`): a colour vtium holds, a number parsed and written again,
//! and a key from a fixed table.

use vtium_parse::StringEnd;

use super::State;
use super::reply::{Query, decimal, osc_end, xcolor};
use crate::bounds::REPLIES_PENDING_MAX;
use crate::palette::Special;
use crate::style::Rgb;

/// A colour OSC 10 to 19 or a kitty key names, past the 256.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dynamic {
    Fg,
    Bg,
    Special(Special),
}

impl Dynamic {
    /// The colour OSC `number` names: none for Tektronix's 15, 16 and 18,
    /// which vtium has no mode for, and none outside 10 to 19.
    fn osc(number: u32) -> Option<Self> {
        Some(match number {
            10 => Self::Fg,
            11 => Self::Bg,
            12 => Self::Special(Special::Cursor),
            13 => Self::Special(Special::PointerFg),
            14 => Self::Special(Special::PointerBg),
            17 => Self::Special(Special::SelectionBg),
            19 => Self::Special(Special::SelectionFg),
            _ => return None,
        })
    }

    /// The query an OSC 10 to 19 `?` for the colour is.
    fn query(self) -> Query {
        match self {
            Self::Fg | Self::Bg => Query::DefaultColor,
            Self::Special(Special::Cursor) => Query::CursorColor,
            Self::Special(Special::PointerFg | Special::PointerBg) => Query::PointerColor,
            Self::Special(Special::SelectionBg | Special::SelectionFg) => Query::SelectionColor,
        }
    }
}

/// A key of OSC 21 that vtium knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Index(u8),
    Dynamic(Dynamic),
}

/// kitty's names for the colours past the 256 that vtium has.
const KEYS: [(&str, Dynamic); 5] = [
    ("foreground", Dynamic::Fg),
    ("background", Dynamic::Bg),
    ("cursor", Dynamic::Special(Special::Cursor)),
    (
        "selection_background",
        Dynamic::Special(Special::SelectionBg),
    ),
    (
        "selection_foreground",
        Dynamic::Special(Special::SelectionFg),
    ),
];

impl Key {
    fn parse(text: &[u8]) -> Option<Self> {
        if let Some(n) = decimal(text) {
            return u8::try_from(n).ok().map(Self::Index);
        }
        KEYS.iter()
            .find(|(name, _)| name.as_bytes() == text)
            .map(|&(_, d)| Self::Dynamic(d))
    }

    /// The key as a reply writes it: the index re-encoded, or the name
    /// from [`KEYS`].
    fn write(self, out: &mut String) {
        match self {
            Self::Index(i) => *out += &i.to_string(),
            Self::Dynamic(d) => {
                if let Some((name, _)) = KEYS.iter().find(|(_, k)| *k == d) {
                    *out += name;
                }
            }
        }
    }
}

/// An X11 colour specification, as XParseColor reads the two forms vtium
/// takes: `rgb:r/g/b` with one to four hex digits a channel, each scaled
/// to the channel's range, and `#rgb` with one to four hex digits a
/// channel, each the channel's most significant bits. The prefix and the
/// digits are read in either case. Each channel keeps its top 8 bits.
pub(super) fn parse_color(spec: &[u8]) -> Option<Rgb> {
    if let Some(rest) = spec
        .get(..4)
        .filter(|p| p.eq_ignore_ascii_case(b"rgb:"))
        .and(spec.get(4..))
    {
        let mut channels = rest.split(|&b| b == b'/');
        let mut next = || channel(channels.next()?, true);
        let rgb = Rgb::new(next()?, next()?, next()?);
        return channels.next().is_none().then_some(rgb);
    }
    let digits = spec.strip_prefix(b"#")?;
    let n = digits.len() / 3;
    if n == 0 || digits.len() % 3 != 0 {
        return None;
    }
    let mut channels = digits.chunks(n);
    let mut next = || channel(channels.next()?, false);
    Some(Rgb::new(next()?, next()?, next()?))
}

/// One to four hex digits as a channel's top 8 bits: of the value scaled
/// to 16 bits, or with `scaled` false, of the digits as a 16-bit value's
/// most significant ones.
fn channel(digits: &[u8], scaled: bool) -> Option<u8> {
    let value = hex(digits)?;
    let bits = u32::try_from(digits.len()).ok()?.checked_mul(4)?;
    let wide = if scaled {
        let max = 1u32.checked_shl(bits)?.checked_sub(1)?;
        value.checked_mul(0xffff)?.checked_div(max)?
    } else {
        value.checked_shl(16u32.checked_sub(bits)?)?
    };
    u8::try_from(wide.checked_shr(8)?).ok()
}

/// One to four hex digits, with nothing else.
fn hex(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() || digits.len() > 4 {
        return None;
    }
    digits.iter().try_fold(0u32, |acc, &b| {
        let d = char::from(b).to_digit(16)?;
        Some((acc << 4) | d)
    })
}

/// The text between spaces, which OSC 21 ignores.
fn trim(text: &[u8]) -> &[u8] {
    text.trim_ascii_start().trim_ascii_end()
}

impl State {
    /// The colour OSC strings; false for a string that is not one.
    pub(super) fn osc_color(&mut self, data: &[u8], end: StringEnd) -> bool {
        let (number, rest) = match data.iter().position(|&b| b == b';') {
            Some(at) => (data.get(..at), data.get(at.saturating_add(1)..)),
            None => (Some(data), None),
        };
        let Some(number) = number.and_then(decimal) else {
            return false;
        };
        if !matches!(number, 4 | 10..=19 | 21 | 104 | 110..=119) {
            return false;
        }
        let before = self.colors.clone();
        match (number, rest) {
            (4, Some(list)) => self.osc_palette(list, end),
            (10..=19, Some(list)) => self.osc_dynamic(number, list, end),
            (21, Some(list)) => self.osc_kitty(list, end),
            (104, list) => self.reset_palette(list),
            (110..=119, _) => {
                if let Some(d) = Dynamic::osc(number.saturating_sub(100)) {
                    self.reset_dynamic(d);
                }
            }
            _ => {}
        }
        if self.colors != before {
            self.screen.recolour();
        }
        true
    }

    /// OSC 4: `index ; spec` pairs, each `?` answered with the palette's
    /// colour and each spec vtium reads set; an index past 255 and a spec
    /// it does not read are skipped. A pair that is not a number and a
    /// field stops the list, as in the C reference. Once the replies
    /// waiting for the host reach [`REPLIES_PENDING_MAX`], the list's
    /// further queries are skipped, as the input stops at the byte whose
    /// reply reaches it: one string at vtium-parse's `OSC_MAX` holds a
    /// quarter of a million queries. Its sets still apply, so what a string
    /// sets does not depend on when the host took the replies.
    fn osc_palette(&mut self, list: &[u8], end: StringEnd) {
        let mut fields = list.split(|&b| b == b';');
        while let (Some(index), Some(spec)) = (fields.next(), fields.next()) {
            let Some(index) = decimal(index) else {
                return;
            };
            let Ok(i) = u8::try_from(index) else {
                continue;
            };
            if spec == b"?" {
                if self.out.len() >= REPLIES_PENDING_MAX {
                    continue;
                }
                let rgb = self.colors.color(i);
                let reply = format!("\x1b]4;{i};{}{}", xcolor(rgb), osc_end(end));
                self.reply(Query::PaletteColor, reply.as_bytes());
            } else if let Some(rgb) = parse_color(spec) {
                self.colors.set_color(i, rgb);
            }
        }
    }

    /// OSC 10 to 19: each field the next colour from `first`, as xterm
    /// takes them, each `?` answered and each spec vtium reads set.
    fn osc_dynamic(&mut self, first: u32, list: &[u8], end: StringEnd) {
        for (number, spec) in (first..=19).zip(list.split(|&b| b == b';')) {
            let Some(d) = Dynamic::osc(number) else {
                continue;
            };
            if spec == b"?" {
                let rgb = self.xterm_color(d);
                let reply = format!("\x1b]{number};{}{}", xcolor(rgb), osc_end(end));
                self.reply(d.query(), reply.as_bytes());
            } else if let Some(rgb) = parse_color(spec) {
                self.set_dynamic(d, Some(rgb));
            }
        }
    }

    /// The colour reset of DECSTR, the 256 returned to the configured
    /// palette's as xterm's soft reset returns them, and with `every`, for
    /// RIS, every colour.
    pub(super) fn reset_colors(&mut self, every: bool) {
        let before = self.colors.clone();
        if every {
            self.colors = self.host.palette.clone();
        } else {
            self.reset_palette(None);
        }
        if self.colors != before {
            self.screen.recolour();
        }
    }

    /// OSC 104: each index listed returns to the configured palette's
    /// colour, and none listed returns all 256; a field that is not an
    /// index is skipped.
    fn reset_palette(&mut self, list: Option<&[u8]>) {
        let configured = &self.host.palette;
        match list {
            None | Some(b"") => {
                for i in 0..=u8::MAX {
                    self.colors.set_color(i, configured.color(i));
                }
            }
            Some(list) => {
                for field in list.split(|&b| b == b';') {
                    if let Some(i) = decimal(field).and_then(|n| u8::try_from(n).ok()) {
                        self.colors.set_color(i, configured.color(i));
                    }
                }
            }
        }
    }

    /// OSC 21: `key=spec` sets, `key=?` queries, `key=` makes a colour
    /// dynamic, which unsets a special colour and is refused for the
    /// defaults and the 256, and a bare key resets. The queries are
    /// answered in one reply, in order; a key vtium does not know is
    /// skipped. Once the reply and the replies waiting for the host reach
    /// [`REPLIES_PENDING_MAX`], further queries are skipped and the sets
    /// still apply.
    fn osc_kitty(&mut self, list: &[u8], end: StringEnd) {
        let mut reply = String::new();
        for field in list.split(|&b| b == b';') {
            let full = self.out.len().saturating_add(reply.len()) >= REPLIES_PENDING_MAX;
            let (key, value) = match field.iter().position(|&b| b == b'=') {
                Some(at) => (
                    field.get(..at).unwrap_or_default(),
                    field.get(at.saturating_add(1)..).map(trim),
                ),
                None => (field, None),
            };
            let Some(key) = Key::parse(trim(key)) else {
                continue;
            };
            match (key, value) {
                (Key::Index(i), None) => {
                    let rgb = self.host.palette.color(i);
                    self.colors.set_color(i, rgb);
                }
                (Key::Dynamic(d), None) => self.reset_dynamic(d),
                (_, Some(b"?")) if full => {}
                (_, Some(b"?")) => {
                    reply.push(';');
                    key.write(&mut reply);
                    reply.push('=');
                    let rgb = match key {
                        Key::Index(i) => Some(self.colors.color(i)),
                        Key::Dynamic(d) => self.kitty_color(d),
                    };
                    if let Some(rgb) = rgb {
                        reply += &xcolor(rgb);
                    }
                }
                (Key::Dynamic(d @ Dynamic::Special(_)), Some(b"")) => self.set_dynamic(d, None),
                (_, Some(b"")) => {}
                (Key::Index(i), Some(spec)) => {
                    if let Some(rgb) = parse_color(spec) {
                        self.colors.set_color(i, rgb);
                    }
                }
                (Key::Dynamic(d), Some(spec)) => {
                    if let Some(rgb) = parse_color(spec) {
                        self.set_dynamic(d, Some(rgb));
                    }
                }
            }
        }
        if !reply.is_empty() {
            let reply = format!("\x1b]21{reply}{}", osc_end(end));
            self.reply(Query::KittyColor, reply.as_bytes());
        }
    }

    /// The colour as xterm answers it: an unset special colour as the
    /// colour it draws in, the cursor's and the selection's background as
    /// the foreground and the selection's foreground as the background.
    fn xterm_color(&self, d: Dynamic) -> Rgb {
        let c = &self.colors;
        match d {
            Dynamic::Fg => c.fg(),
            Dynamic::Bg => c.bg(),
            Dynamic::Special(s @ (Special::PointerBg | Special::SelectionFg)) => {
                c.special(s).unwrap_or(c.bg())
            }
            Dynamic::Special(s) => c.special(s).unwrap_or(c.fg()),
        }
    }

    /// The colour as kitty answers it: none for an unset special colour.
    fn kitty_color(&self, d: Dynamic) -> Option<Rgb> {
        match d {
            Dynamic::Fg => Some(self.colors.fg()),
            Dynamic::Bg => Some(self.colors.bg()),
            Dynamic::Special(s) => self.colors.special(s),
        }
    }

    /// Sets a colour; `None` unsets a special colour and leaves a default.
    fn set_dynamic(&mut self, d: Dynamic, rgb: Option<Rgb>) {
        match (d, rgb) {
            (Dynamic::Fg, Some(rgb)) => self.colors.set_fg(rgb),
            (Dynamic::Bg, Some(rgb)) => self.colors.set_bg(rgb),
            (Dynamic::Special(s), rgb) => self.colors.set_special(s, rgb),
            (_, None) => {}
        }
    }

    /// Returns a colour to the configured palette's.
    fn reset_dynamic(&mut self, d: Dynamic) {
        let configured = &self.host.palette;
        match d {
            Dynamic::Fg => self.colors.set_fg(configured.fg()),
            Dynamic::Bg => self.colors.set_bg(configured.bg()),
            Dynamic::Special(s) => self.colors.set_special(s, configured.special(s)),
        }
    }
}
