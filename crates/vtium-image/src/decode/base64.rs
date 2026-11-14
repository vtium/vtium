//! The base64 decoder a kitty string's payload goes through as it streams
//! (ADR-1100, D7).
//!
//! The decoder never collects a payload whole. It keeps at most three
//! sextets (six-bit groups) between the pieces the parser hands out, and
//! hands what it decodes to a [`Sink`] a block of up to 3 KiB at a time.
//! It reads base64 as RFC 4648, section 4, defines it: the alphabet
//! `A-Z a-z 0-9 + /`, four characters to three bytes. Whole groups of
//! four are decoded a block at a time; a group split across two pieces,
//! or one holding a byte outside the alphabet, is decoded a byte at a
//! time.
//!
//! - A pad, `=`, ends the group it is in: two or three characters decode
//!   to one or two bytes, and one character alone is an error. Decoding
//!   goes on after it, so a payload may hold several padded groups.
//! - Any other byte outside the alphabet is an error.
//! - An error tells the sink, which then decides whether it takes more.
//!   The decoder stops feeding a sink that no longer takes data.

/// A byte outside the alphabet; no sextet has its top bits.
const BAD: u8 = 0xff;

/// Each byte's sextet, or [`BAD`].
static SEXTET: [u8; 256] = sextets();

#[expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "a const table built over 0 to 63, each index and value in range"
)]
const fn sextets() -> [u8; 256] {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut table = [BAD; 256];
    let mut i = 0;
    while i < 64 {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
}

fn sextet(byte: u8) -> u8 {
    SEXTET.get(usize::from(byte)).copied().unwrap_or(BAD)
}

/// Whole groups of four decoded a block at a time: 1024 groups, 3 KiB.
const BLOCK_GROUPS: usize = 1024;

/// Where decoded bytes go.
pub(crate) trait Sink {
    /// Whether the sink still takes data. Once it does not, the decoder
    /// reads nothing more for it.
    fn open(&self) -> bool;
    /// Bytes decoded, in order.
    fn bytes(&mut self, data: &[u8]);
    /// The payload held a byte outside the alphabet, or a pad after one
    /// character of a group.
    fn error(&mut self);
}

/// The decoder's state between pieces: the sextets of a group not yet
/// whole, and nothing else, so a payload cut anywhere leaves the same
/// state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Base64 {
    acc: u32,
    /// Sextets held in `acc`, 0 to 3.
    held: u8,
}

impl Base64 {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Decodes `input`, the next piece of a payload, into `sink`.
    pub(crate) fn feed(&mut self, mut input: &[u8], sink: &mut impl Sink) {
        while !input.is_empty() && sink.open() {
            if self.held == 0 && input.len() >= 4 {
                let taken = groups(input, sink);
                input = input.get(taken..).unwrap_or_default();
                if input.is_empty() || !sink.open() {
                    break;
                }
            }
            let Some((&byte, rest)) = input.split_first() else {
                break;
            };
            input = rest;
            let value = sextet(byte);
            if value == BAD {
                if byte == b'=' {
                    self.flush(sink);
                } else {
                    sink.error();
                }
                continue;
            }
            self.acc = (self.acc.wrapping_shl(6) | u32::from(value)) & 0x00ff_ffff;
            self.held = self.held.saturating_add(1);
            if self.held == 4 {
                sink.bytes(&three(core::mem::take(&mut self.acc)));
                self.held = 0;
            }
        }
    }

    /// One kitty chunk's payload ended: the sextets held decode as
    /// unpadded base64, two to one byte and three to two, and one alone
    /// decodes to nothing, with no error (ADR-1100, D16).
    pub(crate) fn end_unpadded(&mut self, sink: &mut impl Sink) {
        if self.held == 1 {
            self.held = 0;
            self.acc = 0;
            return;
        }
        self.flush(sink);
    }

    /// The payload ended, or a pad came: the sextets held decode to what
    /// they hold, and one alone is an error.
    pub(crate) fn flush(&mut self, sink: &mut impl Sink) {
        let held = core::mem::take(&mut self.held);
        let acc = core::mem::take(&mut self.acc);
        if !sink.open() {
            return;
        }
        match held {
            1 => sink.error(),
            2 | 3 => {
                let shift = 6u32.saturating_mul(4u32.saturating_sub(u32::from(held)));
                let bytes = three(acc.wrapping_shl(shift));
                let n = usize::from(held.saturating_sub(1));
                sink.bytes(bytes.get(..n).unwrap_or_default());
            }
            _ => {}
        }
    }
}

/// The three bytes of a group's 24 bits.
fn three(acc: u32) -> [u8; 3] {
    let [_, a, b, c] = acc.to_be_bytes();
    [a, b, c]
}

/// The four sextets of a group as its three bytes, or `None` when a byte
/// is outside the alphabet.
fn group(chars: [u8; 4]) -> Option<[u8; 3]> {
    let [a, b, c, d] = chars.map(sextet);
    if (a | b | c | d) & 0xc0 != 0 {
        return None;
    }
    let v = u32::from(a).wrapping_shl(18)
        | u32::from(b).wrapping_shl(12)
        | u32::from(c).wrapping_shl(6)
        | u32::from(d);
    Some(three(v))
}

/// The whole groups at the start of `input`, up to the first that holds a
/// byte outside the alphabet, decoded into `sink` a block at a time;
/// returns the bytes taken, a multiple of four.
fn groups(input: &[u8], sink: &mut impl Sink) -> usize {
    let mut block = [0u8; 3 * BLOCK_GROUPS];
    let mut taken = 0usize;
    for chunk in input.chunks(4 * BLOCK_GROUPS) {
        if !sink.open() {
            break;
        }
        let mut made = 0usize;
        let mut whole = true;
        for (&chars, out) in chunk
            .as_chunks::<4>()
            .0
            .iter()
            .zip(block.as_chunks_mut::<3>().0)
        {
            let Some(bytes) = group(chars) else {
                whole = false;
                break;
            };
            *out = bytes;
            made = made.saturating_add(1);
        }
        if made > 0 {
            sink.bytes(block.get(..made.saturating_mul(3)).unwrap_or_default());
        }
        taken = taken.saturating_add(made.saturating_mul(4));
        if !whole || chunk.len() % 4 != 0 {
            break;
        }
    }
    taken
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    /// A sink that keeps what it is given, and closes after an error.
    #[derive(Default)]
    struct Keep {
        data: Vec<u8>,
        errors: u32,
        /// It closes once it holds this many bytes, when given.
        cap: Option<usize>,
    }

    impl Sink for Keep {
        fn open(&self) -> bool {
            self.errors == 0 && self.cap.is_none_or(|c| self.data.len() < c)
        }
        fn bytes(&mut self, data: &[u8]) {
            self.data.extend_from_slice(data);
        }
        fn error(&mut self) {
            self.errors = self.errors.saturating_add(1);
        }
    }

    fn decode(pieces: &[&[u8]]) -> Keep {
        let mut d = Base64::new();
        let mut k = Keep::default();
        for p in pieces {
            d.feed(p, &mut k);
        }
        d.flush(&mut k);
        k
    }

    #[test]
    fn it_decodes_rfc_4648s_test_vectors() {
        // RFC 4648, section 10
        let vectors: &[(&[u8], &[u8])] = &[
            (b"", b""),
            (b"Zg==", b"f"),
            (b"Zm8=", b"fo"),
            (b"Zm9v", b"foo"),
            (b"Zm9vYg==", b"foob"),
            (b"Zm9vYmE=", b"fooba"),
            (b"Zm9vYmFy", b"foobar"),
        ];
        for (text, want) in vectors {
            let k = decode(&[text]);
            assert_eq!(k.data, *want, "{}", String::from_utf8_lossy(text));
            assert_eq!(k.errors, 0);
        }
    }

    #[test]
    fn a_payload_cut_anywhere_decodes_the_same() {
        let text: Vec<u8> = b"AAD/gAAA/4AAAP+AAAD/Zm9vYmE=Zg==Zm9v".repeat(50);
        let whole = decode(&[&text]).data;
        for cut in 0..text.len() {
            let (a, b) = text.split_at(cut);
            assert_eq!(decode(&[a, b]).data, whole, "cut at {cut}");
        }
        let bytes: Vec<&[u8]> = text.chunks(1).collect();
        assert_eq!(decode(&bytes).data, whole);
        let threes: Vec<&[u8]> = text.chunks(3).collect();
        assert_eq!(decode(&threes).data, whole);
    }

    #[test]
    fn unpadded_ends_decode_what_they_hold() {
        assert_eq!(decode(&[b"Zg"]).data, b"f");
        assert_eq!(decode(&[b"Zm8"]).data, b"fo");
        let lone = decode(&[b"Zm9vY"]);
        assert_eq!((lone.data.as_slice(), lone.errors), (&b"foo"[..], 1));
    }

    #[test]
    fn a_byte_outside_the_alphabet_is_an_error_where_it_stands() {
        let k = decode(&[b"Zm9vYmFy", b"Zm*v"]);
        assert_eq!((k.data.as_slice(), k.errors), (&b"foobar"[..], 1));
        // in the block path
        let mut text = b"Zm9v".repeat(2000);
        text.extend_from_slice(b"Zm9!Zm9v");
        let k = decode(&[&text]);
        assert_eq!(k.data.len(), 6000);
        assert_eq!(k.errors, 1);
    }

    #[test]
    fn a_closed_sink_is_fed_nothing_more() {
        let mut d = Base64::new();
        let mut k = Keep {
            cap: Some(3),
            ..Keep::default()
        };
        d.feed(&b"Zm9v".repeat(3000), &mut k);
        // the first block is handed over whole, and nothing after it
        assert_eq!(k.data.len(), 3 * BLOCK_GROUPS);
        d.feed(b"Zm9v", &mut k);
        assert_eq!(k.data.len(), 3 * BLOCK_GROUPS);
        assert_eq!(k.data.get(..3).unwrap(), b"foo");
    }
}
