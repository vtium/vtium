// Ported from libtsm's src/tsm/tsm-unicode.c (the UTF-8 state machine and
// tsm_ucs4_to_utf8), which carries these notices:
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
//
// The tsm-utf8-state-machine is based on the wayland-compositor demos:
//
// Copyright © 2008 Kristian Høgsberg
//
// Permission to use, copy, modify, distribute, and sell this software and
// its documentation for any purpose is hereby granted without fee, provided
// that the above copyright notice appear in all copies and that both that
// copyright notice and this permission notice appear in supporting
// documentation, and that the name of the copyright holders not be used in
// advertising or publicity pertaining to distribution of the software
// without specific, written prior permission.  The copyright holders make
// no representations about the suitability of this software for any
// purpose.  It is provided "as is" without express or implied warranty.
//
// THE COPYRIGHT HOLDERS DISCLAIM ALL WARRANTIES WITH REGARD TO THIS
// SOFTWARE, INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND
// FITNESS, IN NO EVENT SHALL THE COPYRIGHT HOLDERS BE LIABLE FOR ANY
// SPECIAL, INDIRECT OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER
// RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF
// CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
// CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

//! The UTF-8 decoder in front of the state machine, and the encoder that
//! writes OSC strings back as UTF-8.
//!
//! Both reproduce the C reference exactly, its laxness included: the
//! decoder takes overlong forms, surrogates and values past U+10FFFF, and
//! a malformed sequence consumes the byte that broke it.

/// What the C reference's `tsm_utf8_mach_get` returns for a rejected
/// sequence (`TSM_UCS4_REPLACEMENT`, libtsm.h:116).
pub(crate) const REPLACEMENT: u32 = 0xfffd;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Start,
    Accept,
    Reject,
    Expect1,
    Expect2,
    Expect3,
}

/// The C reference's `tsm_utf8_mach` (tsm-unicode.c:707-860).
#[derive(Clone, Debug)]
pub(crate) struct Decoder {
    state: State,
    ch: u32,
}

impl Decoder {
    pub(crate) const fn new() -> Self {
        Decoder {
            state: State::Start,
            ch: 0,
        }
    }

    /// `tsm_utf8_mach_reset` (tsm-unicode.c:853).
    pub(crate) fn reset(&mut self) {
        self.state = State::Start;
    }

    /// `tsm_utf8_mach_idle` (tsm-unicode.c:838): no character is part way.
    pub(crate) fn idle(&self) -> bool {
        matches!(self.state, State::Start | State::Accept | State::Reject)
    }

    /// Feeds one byte: `tsm_utf8_mach_feed` followed, on ACCEPT or REJECT,
    /// by `tsm_utf8_mach_get`, as `vte_input` calls them
    /// (tsm-vte.c:3668-3673). A rejected sequence yields U+FFFD.
    ///
    /// The C function takes the byte as a `char` and widens it to
    /// `uint32_t` (tsm-unicode.c:742). On the reference's x86-64 build
    /// `char` is signed, so a byte from 0x80 up is sign-extended and the
    /// overlong check `c == 0xC0 || c == 0xC1` (tsm-unicode.c:748) never
    /// matches: 0xC0 and 0xC1 lead two-byte sequences like 0xC2 to 0xDF.
    /// The masks every other test applies make the sign-extension
    /// invisible to them, so the arms below test the byte directly.
    pub(crate) fn feed(&mut self, b: u8) -> Option<u32> {
        let c = u32::from(b);
        self.state = match self.state {
            State::Start | State::Accept | State::Reject => match b {
                // single byte, accept
                0x00..=0x7f => {
                    self.ch = c;
                    State::Accept
                }
                // parser out of sync, ignore byte
                0x80..=0xbf => State::Start,
                // start of two byte sequence, 0xC0 and 0xC1 included
                0xc0..=0xdf => {
                    self.ch = (c & 0x1f) << 6;
                    State::Expect1
                }
                // start of three byte sequence
                0xe0..=0xef => {
                    self.ch = (c & 0x0f) << 12;
                    State::Expect2
                }
                // start of four byte sequence
                0xf0..=0xf7 => {
                    self.ch = (c & 0x07) << 18;
                    State::Expect3
                }
                // overlong encoding, reject
                0xf8..=0xff => State::Reject,
            },
            // A byte that is not a continuation rejects the sequence and is
            // consumed with it (tsm-unicode.c:801-822).
            State::Expect3 => {
                self.ch |= (c & 0x3f) << 12;
                if b & 0xc0 == 0x80 {
                    State::Expect2
                } else {
                    State::Reject
                }
            }
            State::Expect2 => {
                self.ch |= (c & 0x3f) << 6;
                if b & 0xc0 == 0x80 {
                    State::Expect1
                } else {
                    State::Reject
                }
            }
            State::Expect1 => {
                self.ch |= c & 0x3f;
                if b & 0xc0 == 0x80 {
                    State::Accept
                } else {
                    State::Reject
                }
            }
        };
        match self.state {
            State::Accept => Some(self.ch),
            State::Reject => Some(REPLACEMENT),
            _ => None,
        }
    }
}

/// `tsm_ucs4_to_utf8` (tsm-unicode.c:600-630): writes `g` as UTF-8 into
/// `out` and returns the length, 0 for a surrogate, a value past U+10FFFF
/// or a noncharacter (U+FDD0 to U+FDEF, and the last two of every plane).
pub(crate) fn encode(g: u32, out: &mut [u8; 4]) -> usize {
    if (0xd800..=0xdfff).contains(&g) {
        return 0;
    }
    if g > 0x10ffff || g & 0xffff == 0xffff || g & 0xffff == 0xfffe {
        return 0;
    }
    if (0xfdd0..=0xfdef).contains(&g) {
        return 0;
    }
    // Each shift and mask leaves a value below 0x100.
    let byte = |v: u32| (v & 0xff) as u8;
    if g < 1 << 7 {
        out[0] = byte(g);
        1
    } else if g < 1 << 11 {
        out[0] = byte(0xc0 | ((g >> 6) & 0x1f));
        out[1] = byte(0x80 | (g & 0x3f));
        2
    } else if g < 1 << 16 {
        out[0] = byte(0xe0 | ((g >> 12) & 0x0f));
        out[1] = byte(0x80 | ((g >> 6) & 0x3f));
        out[2] = byte(0x80 | (g & 0x3f));
        3
    } else {
        out[0] = byte(0xf0 | ((g >> 18) & 0x07));
        out[1] = byte(0x80 | ((g >> 12) & 0x3f));
        out[2] = byte(0x80 | ((g >> 6) & 0x3f));
        out[3] = byte(0x80 | (g & 0x3f));
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(bytes: &[u8]) -> Vec<u32> {
        let mut d = Decoder::new();
        bytes.iter().filter_map(|&b| d.feed(b)).collect()
    }

    #[test]
    fn decodes_each_length() {
        assert_eq!(decode(b"A"), [0x41]);
        assert_eq!(decode("é".as_bytes()), [0xe9]);
        assert_eq!(decode("€".as_bytes()), [0x20ac]);
        assert_eq!(decode("😀".as_bytes()), [0x1f600]);
    }

    // tsm-unicode.c:754-756: a stray continuation byte is ignored, with no
    // replacement.
    #[test]
    fn ignores_stray_continuation() {
        assert_eq!(decode(b"\x80\xbfA"), [0x41]);
    }

    // tsm-unicode.c:801-822: the byte that breaks a sequence is consumed
    // with it, so the 'A' after a truncated sequence is lost.
    #[test]
    fn rejected_sequence_consumes_the_breaking_byte() {
        assert_eq!(decode(b"\xe2\x82A"), [REPLACEMENT]);
        assert_eq!(decode(b"\xc3AB"), [REPLACEMENT, 0x42]);
    }

    // tsm-unicode.c:769-772: 0xF8 to 0xFF reject, one replacement each.
    #[test]
    fn rejects_f8_to_ff() {
        assert_eq!(decode(b"\xf8\xff"), [REPLACEMENT, REPLACEMENT]);
    }

    // tsm-unicode.c:742,748: signed `char` on x86-64 makes 0xC0 and 0xC1
    // two-byte leads, so overlong forms decode.
    #[test]
    fn decodes_overlong_forms_as_the_reference_does() {
        assert_eq!(decode(b"\xc0\x80"), [0x00]);
        assert_eq!(decode(b"\xc1\xbf"), [0x7f]);
        assert_eq!(decode(b"\xe0\x80\x9b"), [0x1b]);
        assert_eq!(decode(b"\xf0\x80\x80\x80"), [0x00]);
    }

    // tsm-unicode.c has no range check: surrogates and values past
    // U+10FFFF decode.
    #[test]
    fn decodes_surrogates_and_values_past_unicode() {
        assert_eq!(decode(b"\xed\xa0\x80"), [0xd800]);
        assert_eq!(decode(b"\xf4\x90\x80\x80"), [0x110000]);
        assert_eq!(decode(b"\xf7\xbf\xbf\xbf"), [0x1f_ffff]);
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

    // tsm-unicode.c:600-630
    #[test]
    fn encode_matches_the_reference() {
        let mut out = [0; 4];
        assert_eq!(encode(0x41, &mut out), 1);
        assert_eq!(out[..1], *b"A");
        assert_eq!(encode(0xe9, &mut out), 2);
        assert_eq!(out[..2], *"é".as_bytes());
        assert_eq!(encode(0x20ac, &mut out), 3);
        assert_eq!(out[..3], *"€".as_bytes());
        assert_eq!(encode(0x1f600, &mut out), 4);
        assert_eq!(out, *"😀".as_bytes());
        for g in [
            0xd800, 0xdfff, 0xfdd0, 0xfdef, 0xfffe, 0xffff, 0x1fffe, 0x110000,
        ] {
            assert_eq!(encode(g, &mut out), 0, "{g:#x}");
        }
    }
}
