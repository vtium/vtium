// The noncharacter rule is ported from libtsm's src/tsm/tsm-unicode.c,
// which carries this notice:
//
// Copyright (c) 2011-2013 David Herrmann <dh.herrmann@gmail.com>
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files
// (the "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be included
// in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! The UTF-8 decoder in front of the state machine, and the characters an
//! OSC string drops.
//!
//! The decoder follows the Unicode Standard. This is vtium's contract for
//! it, and the C reference's decoder follows the same contract
//! (aaronsb/libtsm#83):
//!
//! 1. Well-formed sequences (Table 3-7) decode to their scalar value.
//! 2. Each maximal subpart of an ill-formed sequence becomes one U+FFFD
//!    (§3.9, "U+FFFD Substitution of Maximal Subparts"). A byte that can
//!    neither start nor continue a sequence (80-BF alone, C0, C1, F5-FF)
//!    is one U+FFFD each.
//! 3. The byte that ends an ill-formed sequence is not consumed; it is
//!    processed again from the start state.
//! 4. Overlongs, surrogates and values above U+10FFFF are ill-formed, so
//!    no ill-formed input yields ESC or any other control.
//! 5. The result does not depend on the signedness of C's `char`: the
//!    decoder takes `u8`.
//! 6. An incomplete sequence survives a cut in the input: the bytes on
//!    either side of the cut decode as they do whole.
//! 7. A reset drops an incomplete sequence with no U+FFFD.
//!
//! The noncharacter rule is the C reference's encoder's.

/// What an ill-formed sequence becomes.
pub(crate) const REPLACEMENT: char = '\u{fffd}';

/// The characters one byte yields: none while a sequence is part way, one
/// most of the time, and two when the byte ends an ill-formed sequence and
/// then decodes on its own (contract point 3).
pub(crate) type Decoded = [Option<char>; 2];

/// A streaming UTF-8 decoder: the state between bytes, so input can be cut
/// anywhere (contract point 6).
#[derive(Clone, Debug)]
pub(crate) struct Decoder {
    /// Continuation bytes still to come; 0 between characters.
    need: u8,
    /// The scalar value so far.
    ch: u32,
    /// The range the next continuation byte must fall in. It is narrower
    /// than 80-BF only right after E0, ED, F0 and F4 (Table 3-7), which is
    /// how overlongs, surrogates and values past U+10FFFF are refused.
    lower: u8,
    upper: u8,
}

impl Decoder {
    pub(crate) const fn new() -> Self {
        Decoder {
            need: 0,
            ch: 0,
            lower: 0x80,
            upper: 0xbf,
        }
    }

    /// Between characters, with a sequence part way dropped and no U+FFFD
    /// for it (contract point 7).
    pub(crate) fn reset(&mut self) {
        self.need = 0;
        self.lower = 0x80;
        self.upper = 0xbf;
    }

    /// Whether no character is part way.
    pub(crate) fn idle(&self) -> bool {
        self.need == 0
    }

    /// Feeds one byte.
    pub(crate) fn feed(&mut self, b: u8) -> Decoded {
        if self.need == 0 {
            return [self.start(b), None];
        }
        if !(self.lower..=self.upper).contains(&b) {
            // The bytes so far are a maximal subpart; `b` starts over.
            self.reset();
            return [Some(REPLACEMENT), self.start(b)];
        }
        self.ch = (self.ch << 6) | u32::from(b & 0x3f);
        self.lower = 0x80;
        self.upper = 0xbf;
        self.need = self.need.saturating_sub(1);
        if self.need > 0 {
            return [None, None];
        }
        // Table 3-7's ranges leave only scalar values here.
        [char::from_u32(self.ch), None]
    }

    /// A byte from the start state (Table 3-7's first column).
    fn start(&mut self, b: u8) -> Option<char> {
        let (need, lower, upper) = match b {
            0x00..=0x7f => return Some(char::from(b)),
            0xc2..=0xdf => (1, 0x80, 0xbf),
            0xe0 => (2, 0xa0, 0xbf),
            0xe1..=0xec | 0xee | 0xef => (2, 0x80, 0xbf),
            0xed => (2, 0x80, 0x9f),
            0xf0 => (3, 0x90, 0xbf),
            0xf1..=0xf3 => (3, 0x80, 0xbf),
            0xf4 => (3, 0x80, 0x8f),
            // 80-BF alone, C0, C1, F5-FF
            _ => return Some(REPLACEMENT),
        };
        self.need = need;
        self.lower = lower;
        self.upper = upper;
        // The lead's payload bits: 5, 4 or 3 of them.
        self.ch = u32::from(b) & (0x3f >> need);
        None
    }
}

/// Whether `c` is a noncharacter, U+FDD0 to U+FDEF or the last two of a
/// plane, which the C reference's encoder writes nothing for
/// (tsm-unicode.c:604-607), so an OSC string drops it.
pub(crate) fn is_noncharacter(c: char) -> bool {
    let c = u32::from(c);
    (0xfdd0..=0xfdef).contains(&c) || c & 0xfffe == 0xfffe
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: u32 = REPLACEMENT as u32;

    fn decode(bytes: &[u8]) -> Vec<u32> {
        let mut d = Decoder::new();
        bytes
            .iter()
            .flat_map(|&b| d.feed(b))
            .flatten()
            .map(u32::from)
            .collect()
    }

    /// What Rust's standard library makes of `bytes`: its lossy decoding
    /// substitutes maximal subparts, as the contract asks.
    fn lossy(bytes: &[u8]) -> Vec<u32> {
        String::from_utf8_lossy(bytes)
            .chars()
            .map(u32::from)
            .collect()
    }

    #[test]
    fn decodes_each_length() {
        assert_eq!(decode(b"A"), [0x41]);
        assert_eq!(decode("é".as_bytes()), [0xe9]);
        assert_eq!(decode("€".as_bytes()), [0x20ac]);
        assert_eq!(decode("😀".as_bytes()), [0x1f600]);
    }

    // Contract 1: Unicode Table 3-7, at the edge of each row.
    #[test]
    fn decodes_the_edges_of_table_3_7() {
        let cases: [(&[u8], u32); 10] = [
            (b"\x00", 0x0000),
            (b"\x7f", 0x007f),
            (b"\xc2\x80", 0x0080),
            (b"\xdf\xbf", 0x07ff),
            (b"\xe0\xa0\x80", 0x0800),
            (b"\xed\x9f\xbf", 0xd7ff),
            (b"\xee\x80\x80", 0xe000),
            (b"\xef\xbf\xbf", 0xffff),
            (b"\xf0\x90\x80\x80", 0x1_0000),
            (b"\xf4\x8f\xbf\xbf", 0x10_ffff),
        ];
        for (bytes, want) in cases {
            assert_eq!(decode(bytes), [want], "{bytes:02x?}");
        }
    }

    // Contract 2: Unicode Table 3-8, the worked example of maximal subparts.
    #[test]
    fn substitutes_maximal_subparts_as_table_3_8() {
        assert_eq!(
            decode(b"\x61\xf1\x80\x80\xe1\x80\xc2\x62\x80\x63\x80\xbf\x64"),
            [0x61, R, R, R, 0x62, R, 0x63, R, R, 0x64]
        );
    }

    // Contract 2: a byte that can neither start nor continue a sequence is
    // one U+FFFD.
    #[test]
    fn lone_bytes_that_cannot_start_a_sequence() {
        for b in [0x80, 0xbf, 0xc0, 0xc1, 0xf5, 0xff] {
            assert_eq!(decode(&[b, b'A']), [R, 0x41], "{b:#x}");
        }
    }

    // Contract 3: the byte that ends an ill-formed sequence is processed
    // again from the start state.
    #[test]
    fn breaking_byte_is_processed_again() {
        assert_eq!(decode(b"\xe2\x1b"), [R, 0x1b]);
        assert_eq!(decode(b"\xe2\x41"), [R, 0x41]);
        assert_eq!(decode(b"\xe2\x82\x41"), [R, 0x41]);
        assert_eq!(decode(b"\xf0\x9f\x98\x41"), [R, 0x41]);
        assert_eq!(decode(b"\xc3\xc3\xa9"), [R, 0xe9]);
    }

    // Contract 4: overlongs are ill-formed, and none makes ESC.
    #[test]
    fn overlongs_are_ill_formed() {
        assert_eq!(decode(b"\xc0\x9b"), [R, R]);
        assert_eq!(decode(b"\xc1\xbf"), [R, R]);
        assert_eq!(decode(b"\xe0\x80\x9b"), [R, R, R]);
        assert_eq!(decode(b"\xe0\x9f\xbf"), [R, R, R]);
        assert_eq!(decode(b"\xf0\x80\x80\x9b"), [R, R, R, R]);
        assert_eq!(decode(b"\xf0\x8f\xbf\xbf"), [R, R, R, R]);
    }

    // Contract 4: surrogates are ill-formed.
    #[test]
    fn surrogates_are_ill_formed() {
        assert_eq!(decode(b"\xed\xa0\x80"), [R, R, R]);
        assert_eq!(decode(b"\xed\xbf\xbf"), [R, R, R]);
    }

    // Contract 4: values above U+10FFFF are ill-formed.
    #[test]
    fn values_past_10ffff_are_ill_formed() {
        assert_eq!(decode(b"\xf4\x90\x80\x80"), [R, R, R, R]);
        assert_eq!(decode(b"\xf5\x80\x80\x80"), [R, R, R, R]);
        assert_eq!(decode(b"\xf7\xbf\xbf\xbf"), [R, R, R, R]);
    }

    // Contracts 1 to 3, exhaustively: every lead byte, every two-byte
    // prefix, and every three-byte prefix of a multi-byte lead, each
    // followed by `A` so that a truncated sequence ends, decode as the
    // standard library decodes them.
    #[test]
    fn matches_lossy_decoding_for_every_prefix() {
        for a in 0..=0xffu8 {
            let one = [a, b'A'];
            assert_eq!(decode(&one), lossy(&one), "{one:02x?}");
            for b in 0..=0xffu8 {
                let two = [a, b, b'A'];
                assert_eq!(decode(&two), lossy(&two), "{two:02x?}");
                if (0xe0..=0xf4).contains(&a) {
                    for c in 0..=0xffu8 {
                        let three = [a, b, c, b'A'];
                        assert_eq!(decode(&three), lossy(&three), "{three:02x?}");
                    }
                }
            }
        }
    }

    // Contracts 1 to 3 at the fourth byte: for each four-byte lead, after a
    // well-formed pair of continuations, every possible fourth byte decodes
    // as the standard library decodes it.
    #[test]
    fn matches_lossy_decoding_for_every_fourth_byte() {
        for a in 0xf0..=0xf4u8 {
            let b = if a == 0xf0 { 0x90 } else { 0x80 };
            for d in 0..=0xffu8 {
                let four = [a, b, 0x80, d, b'A'];
                assert_eq!(decode(&four), lossy(&four), "{four:02x?}");
            }
        }
    }

    // Contract 4: over every two-byte input, a control comes out only from
    // its own byte (C0 and DEL) or from its well-formed C2 sequence (C1), so
    // no ill-formed input yields ESC or any other control.
    #[test]
    fn no_ill_formed_input_yields_a_control() {
        for a in 0..=0xffu8 {
            for b in 0..=0xffu8 {
                for c in decode(&[a, b]) {
                    match c {
                        0x00..=0x1f | 0x7f => {
                            assert!(
                                c == u32::from(a) || c == u32::from(b),
                                "[{a:#x}, {b:#x}] gave {c:#x}"
                            );
                        }
                        0x80..=0x9f => {
                            assert!(
                                a == 0xc2 && u32::from(b) == c,
                                "[{a:#x}, {b:#x}] gave {c:#x}"
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    #[test]
    fn idle_only_between_characters() {
        let mut d = Decoder::new();
        assert!(d.idle());
        d.feed(0xe2);
        assert!(!d.idle());
        d.feed(0x82);
        assert!(!d.idle());
        d.feed(0xac);
        assert!(d.idle());
    }

    // tsm-unicode.c:604-607
    #[test]
    fn noncharacters_as_the_reference_has_them() {
        for c in [
            '\u{fdd0}',
            '\u{fdef}',
            '\u{fffe}',
            '\u{ffff}',
            '\u{1fffe}',
            '\u{10ffff}',
        ] {
            assert!(is_noncharacter(c), "{c:?}");
        }
        for c in [
            'A',
            '\u{fdcf}',
            '\u{fdf0}',
            '\u{fffd}',
            '\u{10000}',
            '\u{10fffd}',
        ] {
            assert!(!is_noncharacter(c), "{c:?}");
        }
    }
}
