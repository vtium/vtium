//! The color OSC strings: OSC 4 and 104 for the palette, OSC 10 to 19 and
//! 110 to 119 for the defaults and the cursor, pointer and selection
//! colors, as xterm's ctlseqs defines them, and OSC 21, kitty's color
//! protocol (`color-stack`); and the color stack, which XTPUSHCOLORS,
//! XTPOPCOLORS and XTREPORTCOLORS (`CSI Pm # P`, `Q` and `R`, ctlseqs)
//! and kitty's OSC 30001 and 30101 (`color-stack`) share.
//!
//! A set changes the terminal's palette, the copy of the configured one
//! that frames draw through; a reset returns a color to the configured
//! palette's. A string that changes any color marks every cell changed,
//! as each may draw in it. Each query's reply follows the reply rule
//! (`reply.rs`): a color vtium holds, a number parsed and written again,
//! and a key from a fixed table.

use vtium_parse::{Params, StringEnd};

use super::State;
use super::reply::{Query, decimal, osc_end, xcolor};
use crate::bounds::{COLOR_STACK_MAX, REPLIES_PENDING_MAX};
use crate::palette::{Palette, Special};
use crate::style::Rgb;

mod names;

/// A color OSC 10 to 19 or a kitty key names, past the 256.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dynamic {
    Fg,
    Bg,
    Special(Special),
}

impl Dynamic {
    /// The color OSC `number` names: none for Tektronix's 15, 16 and 18,
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

    /// The query an OSC 10 to 19 `?` for the color is.
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

/// kitty's names for the colors past the 256 that vtium has.
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

/// The color stack: [`COLOR_STACK_MAX`] entries, each empty or holding a
/// palette, and the current entry, the number of the one the next push
/// stores into less one.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Stack {
    entries: [Option<Box<Palette>>; COLOR_STACK_MAX],
    current: usize,
}

impl Stack {
    /// The entry a push or pop parameter names: 0 or omitted the next push
    /// stores into or the next pop restores from, and 1 to
    /// [`COLOR_STACK_MAX`] that entry; none past it, or for a pop of an
    /// empty stack.
    fn entry(&self, param: Option<u32>, pop: bool) -> Option<usize> {
        match param.unwrap_or(0) {
            0 if pop => self.current.checked_sub(1),
            0 => Some(self.current),
            n => usize::try_from(n).ok()?.checked_sub(1),
        }
        .filter(|&i| i < COLOR_STACK_MAX)
    }

    /// The entries stored: the number of the highest that holds a palette.
    fn stored(&self) -> usize {
        self.entries
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |i| i.saturating_add(1))
    }
}

/// An X11 color specification, as XParseColor reads the three forms
/// vtium takes: `rgb:r/g/b` with one to four hex digits a channel, each
/// scaled to the channel's range; `#rgb` with one to four hex digits a
/// channel, each the channel's most significant bits; and a color name
/// ([`named`]). The prefix and the digits are read in either case. Each
/// channel keeps its top 8 bits.
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
    let Some(digits) = spec.strip_prefix(b"#") else {
        return named(spec);
    };
    let n = digits.len() / 3;
    if n == 0 || digits.len() % 3 != 0 {
        return None;
    }
    let mut channels = digits.chunks(n);
    let mut next = || channel(channels.next()?, false);
    Some(Rgb::new(next()?, next()?, next()?))
}

/// A color name from X11's rgb.txt, read in any case and with any spaces,
/// so `DarkSlateGray` and `dark slate gray` name one color.
fn named(spec: &[u8]) -> Option<Rgb> {
    let mut key = [0u8; names::NAME_MAX];
    let mut len = 0usize;
    for &b in spec.iter().filter(|&&b| b != b' ') {
        *key.get_mut(len)? = b.to_ascii_lowercase();
        len = len.saturating_add(1);
    }
    let key = key.get(..len)?;
    let text = names::TEXT.as_bytes();
    let at = names::NAMES
        .binary_search_by(|&(start, n, _)| {
            let start = usize::from(start);
            let name = text.get(start..start.saturating_add(usize::from(n)));
            name.unwrap_or_default().cmp(key)
        })
        .ok()?;
    let &(_, _, [r, g, b]) = names::NAMES.get(at)?;
    Some(Rgb::new(r, g, b))
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
    /// The color OSC strings; false for a string that is not one.
    pub(super) fn osc_color(&mut self, data: &[u8], end: StringEnd) -> bool {
        let (number, rest) = match data.iter().position(|&b| b == b';') {
            Some(at) => (data.get(..at), data.get(at.saturating_add(1)..)),
            None => (Some(data), None),
        };
        let Some(number) = number.and_then(decimal) else {
            return false;
        };
        if !matches!(number, 4 | 10..=19 | 21 | 104 | 110..=119 | 30001 | 30101) {
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
            (30001, None) => self.push_colors(None),
            (30101, None) => self.pop_colors(None),
            _ => {}
        }
        if self.colors != before {
            self.screen.recolor();
        }
        true
    }

    /// OSC 4: `index ; spec` pairs, each `?` answered with the palette's
    /// color and each spec vtium reads set; an index past 255 and a spec
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

    /// OSC 10 to 19: each field the next color from `first`, as xterm
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

    /// XTPUSHCOLORS, `CSI Pm # P`: each parameter in turn, the palette
    /// stored in the entry it names, which becomes the current one; 0 or
    /// none pushes. A push with the stack full and a parameter past
    /// [`COLOR_STACK_MAX`] do nothing, as in xterm.
    pub(super) fn push_colors_params(&mut self, params: &Params) {
        if params.count() == 0 {
            return self.push_colors(None);
        }
        for i in 0..params.count() {
            self.push_colors(params.get(i));
        }
    }

    /// XTPOPCOLORS, `CSI Pm # Q`: each parameter in turn, the palette
    /// restored from the entry it names, which the current one then
    /// precedes; 0 or none pops. A pop of an empty stack, of an entry
    /// never stored and past [`COLOR_STACK_MAX`] does nothing, as in xterm.
    pub(super) fn pop_colors_params(&mut self, params: &Params) {
        if params.count() == 0 {
            return self.pop_colors(None);
        }
        for i in 0..params.count() {
            self.pop_colors(params.get(i));
        }
    }

    fn push_colors(&mut self, param: Option<u32>) {
        let stack = &mut self.color_stack;
        if let Some(i) = stack.entry(param, false)
            && let Some(slot) = stack.entries.get_mut(i)
        {
            *slot = Some(Box::new(self.colors.clone()));
            stack.current = i.saturating_add(1);
        }
    }

    fn pop_colors(&mut self, param: Option<u32>) {
        let stack = &mut self.color_stack;
        let Some(i) = stack.entry(param, true) else {
            return;
        };
        if let Some(Some(palette)) = stack.entries.get(i) {
            let palette = Palette::clone(palette);
            stack.current = i;
            if self.colors != palette {
                self.colors = palette;
                self.screen.recolor();
            }
        }
    }

    /// XTREPORTCOLORS, `CSI # R`: `CSI ? Pc ; Ps # Q`, the current entry
    /// and the entries stored.
    pub(super) fn reply_color_stack(&mut self) {
        let stack = &self.color_stack;
        let reply = format!("\x1b[?{};{}#Q", stack.current, stack.stored());
        self.reply(Query::ColorStack, reply.as_bytes());
    }

    /// The color reset of DECSTR, the 256 returned to the configured
    /// palette's as xterm's soft reset returns them, and with `every`, for
    /// RIS, every color.
    pub(super) fn reset_colors(&mut self, every: bool) {
        let before = self.colors.clone();
        if every {
            self.colors = self.host.palette.clone();
        } else {
            self.reset_palette(None);
        }
        if self.colors != before {
            self.screen.recolor();
        }
    }

    /// OSC 104: each index listed returns to the configured palette's
    /// color, and none listed returns all 256; a field that is not an
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

    /// OSC 21: `key=spec` sets, `key=?` queries, `key=` makes a color
    /// dynamic, which unsets a special color and is refused for the
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

    /// The color as xterm answers it: an unset special color as the
    /// color it draws in, the cursor's and the selection's background as
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

    /// The color as kitty answers it: none for an unset special color.
    fn kitty_color(&self, d: Dynamic) -> Option<Rgb> {
        match d {
            Dynamic::Fg => Some(self.colors.fg()),
            Dynamic::Bg => Some(self.colors.bg()),
            Dynamic::Special(s) => self.colors.special(s),
        }
    }

    /// Sets a color; `None` unsets a special color and leaves a default.
    fn set_dynamic(&mut self, d: Dynamic, rgb: Option<Rgb>) {
        match (d, rgb) {
            (Dynamic::Fg, Some(rgb)) => self.colors.set_fg(rgb),
            (Dynamic::Bg, Some(rgb)) => self.colors.set_bg(rgb),
            (Dynamic::Special(s), rgb) => self.colors.set_special(s, rgb),
            (_, None) => {}
        }
    }

    /// Returns a color to the configured palette's.
    fn reset_dynamic(&mut self, d: Dynamic) {
        let configured = &self.host.palette;
        match d {
            Dynamic::Fg => self.colors.set_fg(configured.fg()),
            Dynamic::Bg => self.colors.set_bg(configured.bg()),
            Dynamic::Special(s) => self.colors.set_special(s, configured.special(s)),
        }
    }
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    /// Only case and spaces fold: other blanks, non-ASCII bytes and a name
    /// run on are not a name, and spaces do not count to the longest.
    #[test]
    fn names_fold_only_case_and_spaces() {
        let goldenrod = Some(Rgb::new(250, 250, 210));
        assert_eq!(named(b"l i g h t g o l d e n r o d y e l l o w"), goldenrod);
        assert_eq!(named(b"  LightGoldenrodYellow  "), goldenrod);
        for bad in [
            &b"red\t"[..],
            b"\tred",
            b"red\0",
            b"r\xc3\xa9d",
            b"red\xa0",
            b"lightgoldenrodyellowx",
            b"xlightgoldenrodyellow",
            b"",
            b"   ",
        ] {
            assert_eq!(named(bad), None, "{bad:?}");
        }
    }

    /// The table against the rgb.txt it was generated from: each line's
    /// name, as written, in capitals and with its spaces removed, reads as
    /// the line's color, and the table holds no name rgb.txt does not.
    #[test]
    fn every_name_in_rgb_txt_reads_as_its_color() {
        let text = include_str!("color/rgb.txt");
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 782, "rgb.txt is not rgb-1.1.1's");
        let mut folded = std::collections::BTreeSet::new();
        for line in lines {
            let mut fields = line.split_whitespace();
            let mut channel = || fields.next().and_then(|c| c.parse().ok());
            let (r, g, b) = (channel(), channel(), channel());
            let want = Some(Rgb::new(
                r.expect("a channel"),
                g.expect("a channel"),
                b.expect("a channel"),
            ));
            let words: Vec<&str> = fields.collect();
            let name = words.join(" ");
            let joined = words.concat();
            for spelling in [name.clone(), name.to_ascii_uppercase(), joined.clone()] {
                assert_eq!(named(spelling.as_bytes()), want, "{spelling:?}");
                assert_eq!(parse_color(spelling.as_bytes()), want, "{spelling:?}");
            }
            folded.insert(joined.to_ascii_lowercase());
        }
        assert_eq!(names::NAMES.len(), folded.len());
        assert_eq!(
            names::TEXT.len(),
            folded.iter().map(String::len).sum::<usize>()
        );
    }
}
