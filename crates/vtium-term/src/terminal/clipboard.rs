//! OSC 52: a program sets the selection, which the host takes with
//! [`Terminal::take_clipboard_write`](crate::Terminal::take_clipboard_write).
//!
//! `OSC 52 ; Pc ; Pd ST` (ctlseqs, OSC 52): Pc names the selections to set,
//! and Pd is the data in base64. vtium has one selection, which `c`, `p`
//! and `s` name, and the empty Pc too, ctlseqs's `s 0`; `q` and the cut
//! buffers `0` to `7` name none of vtium's, and a character outside the set
//! is skipped, as xterm skips it (`misc.c`, `ManipulateSelectionData`). The
//! data is decoded strictly and held to [`CLIPBOARD_MAX`]: anything else is
//! dropped whole, and the selection keeps what it held. The query, `Pd` of
//! `?`, gets no reply (ADR-1000, osc.52.read, refused).

use super::State;
use crate::bounds::CLIPBOARD_MAX;

impl State {
    /// OSC 52 from its `Pc ; Pd`, the string past `52;`.
    pub(super) fn set_clipboard(&mut self, args: &[u8]) {
        let mut fields = args.splitn(2, |&b| b == b';');
        let (Some(targets), Some(data)) = (fields.next(), fields.next()) else {
            return;
        };
        let names_ours =
            targets.is_empty() || targets.iter().any(|t| matches!(t, b'c' | b'p' | b's'));
        if !names_ours || data == b"?" {
            return;
        }
        if let Some(bytes) = decode(data, CLIPBOARD_MAX) {
            self.clipboard = Some(bytes);
        }
    }
}

/// The bytes `text` encodes in base64 (RFC 4648, section 4), or `None`
/// when it is not base64 or they would pass `max`. Every character is in
/// the alphabet, `=` pads only the end and only to a multiple of four, and
/// the bits past the last byte are zero (section 3.5). The padding may be
/// left off, as Ghostty accepts (`src/Surface.zig:2299-2306` at `a732580`).
fn decode(text: &[u8], max: usize) -> Option<Vec<u8>> {
    let body = text
        .strip_suffix(b"==")
        .or_else(|| text.strip_suffix(b"="))
        .unwrap_or(text);
    let padded = body.len() != text.len();
    // a last group of one character holds no byte
    if body.len() % 4 == 1 || (padded && text.len() % 4 != 0) {
        return None;
    }
    // each group of four is three bytes, and a last group of two or three
    // is one or two
    let len = (body.len() / 4)
        .checked_mul(3)?
        .checked_add((body.len() % 4).saturating_sub(1))?;
    if len > max {
        return None;
    }
    let mut out = Vec::with_capacity(len);
    for group in body.chunks(4) {
        let mut n: u32 = 0;
        for &c in group {
            n = n.wrapping_shl(6) | u32::from(sextet(c)?);
        }
        for _ in group.len()..4 {
            n = n.wrapping_shl(6);
        }
        let [_, bytes @ ..] = n.to_be_bytes();
        let (kept, past) = bytes.split_at(group.len().saturating_sub(1));
        if past.iter().any(|&b| b != 0) {
            return None;
        }
        out.extend_from_slice(kept);
    }
    Some(out)
}

/// A base64 character's value (RFC 4648, table 1).
fn sextet(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c.wrapping_sub(b'A')),
        b'a'..=b'z' => Some(c.wrapping_sub(b'a').wrapping_add(26)),
        b'0'..=b'9' => Some(c.wrapping_sub(b'0').wrapping_add(52)),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Terminal;
    use vtium_parse::{Consumer, StringEnd};

    // vtium-parse's README, "Notes for vtium-term": a string the parser
    // truncated is refused wherever part of it would do harm. A truncated
    // OSC 52 sets nothing, though its kept bytes are a valid set; a string
    // past the bound cannot be built small enough to show this through the
    // parser, whose kept prefix decodes past CLIPBOARD_MAX.
    #[test]
    fn a_truncated_osc_52_sets_nothing() {
        let mut t = Terminal::new(8, 2, 0);
        t.state.osc_end(b"52;c;aGk=", true, StringEnd::St);
        assert_eq!(t.take_clipboard_write(), None);
        t.state.osc_end(b"52;c;aGk=", false, StringEnd::St);
        assert_eq!(t.take_clipboard_write(), Some(b"hi".to_vec()));
    }

    // The cap is checked on the decoded length before anything is
    // allocated: 4 bytes of base64 are 1, 2 or 3 bytes by their padding.
    #[test]
    fn the_cap_counts_decoded_bytes() {
        assert_eq!(decode(b"Zm9v", 3), Some(b"foo".to_vec()));
        assert_eq!(decode(b"Zm9v", 2), None);
        assert_eq!(decode(b"Zm8=", 2), Some(b"fo".to_vec()));
        assert_eq!(decode(b"Zm8", 2), Some(b"fo".to_vec()));
        assert_eq!(decode(b"Zg==", 1), Some(b"f".to_vec()));
        assert_eq!(decode(b"Zg==", 0), None);
        assert_eq!(decode(b"", 0), Some(Vec::new()));
    }

    #[test]
    fn every_base64_character_has_its_value() {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        for (value, &c) in (0u8..).zip(alphabet) {
            assert_eq!(sextet(c), Some(value));
        }
        for c in 0..=255u8 {
            assert_eq!(sextet(c).is_some(), alphabet.contains(&c), "{c:#x}");
        }
    }
}
