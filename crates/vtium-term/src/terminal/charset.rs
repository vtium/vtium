//! The character sets: what G0 to G3 hold, which of them GL and GR hold,
//! and the single shift waiting for the next character (VT220 4.4).
//!
//! GL maps the ASCII graphic characters, 0x21 to 0x7E. GR maps the bytes
//! 0xA1 to 0xFE of 8-bit input, through the same set as the byte 0x80
//! below would through GL. A character decoded from UTF-8 prints as
//! itself: in UTF-8 input GR plays no part, as in xterm's UTF-8 mode.

/// A graphic set a G slot holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Charset {
    Ascii,
    /// VT220 table 2-4.
    DecSpecial,
    /// VT220 table 2-3.
    DecSupplemental,
}

impl Charset {
    /// The set an SCS final designates, for a 94-character set (`ESC (`,
    /// `)`, `*` or `+`). DEC supplemental graphics is a set of 8-bit input,
    /// the VT220's default for GR; elsewhere `<` designates ASCII, as xterm
    /// does. The national
    /// replacement sets designate ASCII: ADR-1000 refuses DECNRCM, and
    /// xterm draws them as ASCII while it is reset.
    pub(super) fn from_final(final_byte: u8, eight_bit: bool) -> Option<Self> {
        match final_byte {
            b'B' => Some(Self::Ascii),
            b'0' => Some(Self::DecSpecial),
            b'<' if eight_bit => Some(Self::DecSupplemental),
            b'<' => Some(Self::Ascii),
            b'A' | b'4' | b'C' | b'5' | b'R' | b'Q' | b'K' | b'Y' | b'E' | b'6' | b'Z' | b'H'
            | b'7' | b'=' => Some(Self::Ascii),
            _ => None,
        }
    }

    /// The character the set draws for a GL code, 0x21 to 0x7E; `None`
    /// where it has none.
    fn map(self, code: u8) -> Option<char> {
        match self {
            Self::Ascii => Some(char::from(code)),
            Self::DecSpecial => match code {
                // 5/15 is a blank.
                0x5f => Some(' '),
                0x60..=0x7e => DEC_SPECIAL
                    .get(usize::from(code.wrapping_sub(0x60)))
                    .copied(),
                _ => Some(char::from(code)),
            },
            Self::DecSupplemental => match code {
                0x28 => Some('\u{A4}'),
                0x57 => Some('\u{152}'),
                0x5d => Some('\u{178}'),
                0x77 => Some('\u{153}'),
                0x7d => Some('\u{FF}'),
                // Reserved.
                0x24 | 0x26 | 0x2c..=0x2f | 0x34 | 0x38 | 0x3e | 0x50 | 0x5e | 0x70 | 0x7e => None,
                _ => Some(char::from(code | 0x80)),
            },
        }
    }
}

/// DEC special graphics from 6/0 to 7/14, as xterm, Konsole, Ghostty and
/// the Linux console draw them.
const DEC_SPECIAL: [char; 31] = [
    '\u{25C6}', '\u{2592}', '\u{2409}', '\u{240C}', '\u{240D}', '\u{240A}', '\u{B0}', '\u{B1}',
    '\u{2424}', '\u{240B}', '\u{2518}', '\u{2510}', '\u{250C}', '\u{2514}', '\u{253C}', '\u{23BA}',
    '\u{23BB}', '\u{2500}', '\u{23BC}', '\u{23BD}', '\u{251C}', '\u{2524}', '\u{2534}', '\u{252C}',
    '\u{2502}', '\u{2264}', '\u{2265}', '\u{3C0}', '\u{2260}', '\u{A3}', '\u{B7}',
];

/// G0 to G3, the slots GL and GR hold, and a pending single shift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Charsets {
    g: [Charset; 4],
    gl: usize,
    gr: usize,
    single: Option<usize>,
}

impl Charsets {
    /// The default mapping (VT220 4.4): ASCII in GL, and in 8-bit input
    /// DEC supplemental graphics in GR. GL holds G0 and GR holds G2.
    pub(super) fn initial(eight_bit: bool) -> Self {
        let right = if eight_bit {
            Charset::DecSupplemental
        } else {
            Charset::Ascii
        };
        Self {
            g: [Charset::Ascii, Charset::Ascii, right, right],
            gl: 0,
            gr: 2,
            single: None,
        }
    }

    /// SCS: `set` into G`slot`.
    pub(super) fn designate(&mut self, slot: usize, set: Charset) {
        if let Some(g) = self.g.get_mut(slot) {
            *g = set;
        }
    }

    /// SI, SO, LS2 and LS3: G`slot` into GL.
    pub(super) fn lock_left(&mut self, slot: usize) {
        self.gl = slot;
    }

    /// LS1R, LS2R and LS3R: G`slot` into GR.
    pub(super) fn lock_right(&mut self, slot: usize) {
        self.gr = slot;
    }

    /// SS2 and SS3: G`slot` into GL for the next character.
    pub(super) fn single_shift(&mut self, slot: usize) {
        self.single = Some(slot);
    }

    /// The character to print for `c`, or `None` to print nothing.
    /// `eight_bit` says `c` came from a byte of 8-bit input, so U+00A1 to
    /// U+00FE are GR. Every printed character takes the single shift; DEL,
    /// which prints nothing, leaves it.
    pub(super) fn map(&mut self, c: char, eight_bit: bool) -> Option<char> {
        if c == '\u{7F}' {
            return None;
        }
        let single = self.single.take();
        let (slot, code) = match u32::from(c) {
            n @ 0x21..=0x7e => (single.unwrap_or(self.gl), n),
            n @ 0xa1..=0xfe if eight_bit => (self.gr, n & 0x7f),
            _ => return Some(c),
        };
        let code = u8::try_from(code).ok()?;
        self.g
            .get(slot)
            .copied()
            .unwrap_or(Charset::Ascii)
            .map(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every 94-character set maps each GL code to one character or none,
    // and ASCII and the DEC special graphics below 5/15 to itself.
    #[test]
    fn every_set_maps_every_code() {
        for code in 0x21..=0x7e_u8 {
            assert_eq!(Charset::Ascii.map(code), Some(char::from(code)));
            let special = Charset::DecSpecial.map(code);
            assert!(special.is_some());
            if code < 0x5f {
                assert_eq!(special, Some(char::from(code)));
            }
            let supplemental = Charset::DecSupplemental.map(code);
            assert!(supplemental.is_none_or(|c| u32::from(c) >= 0xa0));
        }
    }
}
