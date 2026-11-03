//! What the terminal sends the TUI, decoded: keys as a terminal of the
//! VT100 line sends them, as vtium-term encodes them, and the replies to
//! the TUI's requests (ADR-1201 D4, D7).
//!
//! A sequence split across reads waits in the decoder for its end. The
//! escape key alone is never a key: a lone ESC cannot be told from the
//! start of a sequence without waiting, so the TUI binds nothing to it.

/// A key the TUI acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Tab,
    BackTab,
    Backspace,
    /// Ctrl-C, a byte in raw mode.
    Interrupt,
    Char(char),
}

/// A reply to one of the TUI's requests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// XTVERSION's, `DCS > | TEXT ST`.
    Version(String),
    /// The OSC 50 query's, `OSC 50 ; NAME ST`: the font in use.
    Font(String),
    /// A kitty graphics reply, `APC G ... ; MESSAGE ST`: `true` for `OK`.
    Graphics(bool),
    /// DA1's, `CSI ? ... c`.
    Attributes,
}

/// One thing the terminal sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Key(Key),
    Reply(Reply),
}

/// The most bytes a string sequence (DCS, OSC, APC) holds before the
/// decoder drops it.
const STRING_MAX: usize = 4096;

/// The decoder: bytes in, [`Input`]s out, a sequence cut by a read kept for
/// the next.
#[derive(Debug, Default)]
pub struct Decoder {
    held: Vec<u8>,
}

/// What the bytes at the front hold.
enum Front {
    /// So many bytes made this input.
    Took(usize, Option<Input>),
    /// The bytes end before the sequence does.
    Short,
}

impl Decoder {
    /// Decodes `bytes` after what was held, and holds a sequence they end
    /// in the middle of.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Input> {
        self.held.extend_from_slice(bytes);
        let mut out = Vec::new();
        let mut at = 0;
        while let Some(rest) = self.held.get(at..).filter(|r| !r.is_empty()) {
            match front(rest) {
                Front::Took(n, input) => {
                    out.extend(input);
                    at = at.saturating_add(n.max(1));
                }
                Front::Short if rest.len() > STRING_MAX => at = self.held.len(),
                Front::Short => break,
            }
        }
        self.held.drain(..at.min(self.held.len()));
        out
    }
}

/// Where the string that starts at `body` ends: its length and the end's
/// length, ST (`ESC \`) or, for OSC, BEL.
fn string_end(body: &[u8], bel: bool) -> Option<(usize, usize)> {
    body.iter().enumerate().find_map(|(i, &b)| match b {
        0x07 if bel => Some((i, 1)),
        0x1b if body.get(i.saturating_add(1)) == Some(&b'\\') => Some((i, 2)),
        _ => None,
    })
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The input at the front of `bytes`, which is not empty.
fn front(bytes: &[u8]) -> Front {
    let Some((&first, rest)) = bytes.split_first() else {
        return Front::Short;
    };
    let key = |n, k| Front::Took(n, Some(Input::Key(k)));
    match first {
        0x1b => escape(rest),
        0x03 => key(1, Key::Interrupt),
        b'\r' | b'\n' => key(1, Key::Enter),
        b'\t' => key(1, Key::Tab),
        0x7f | 0x08 => key(1, Key::Backspace),
        b if b < 0x20 => Front::Took(1, None),
        _ => {
            // a UTF-8 character, whole
            let len = match first {
                0xc0..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf7 => 4,
                _ => 1,
            };
            match bytes.get(..len) {
                None => Front::Short,
                Some(c) => match std::str::from_utf8(c).ok().and_then(|s| s.chars().next()) {
                    Some(c) => key(len, Key::Char(c)),
                    None => Front::Took(1, None),
                },
            }
        }
    }
}

/// The input after an ESC; `rest` is what follows it.
fn escape(rest: &[u8]) -> Front {
    let Some((&kind, body)) = rest.split_first() else {
        return Front::Short;
    };
    let took = |body_len: usize, input| Front::Took(body_len.saturating_add(2), input);
    match kind {
        b'[' => csi(body),
        b'O' => match body.first() {
            None => Front::Short,
            Some(&b) => took(1, arrow(b).map(Input::Key)),
        },
        b'P' | b']' | b'_' => {
            let Some((end, end_len)) = string_end(body, kind == b']') else {
                return Front::Short;
            };
            let content = body.get(..end).unwrap_or_default();
            let input = match kind {
                b'P' => content
                    .strip_prefix(b">|")
                    .map(|t| Input::Reply(Reply::Version(text(t)))),
                b']' => content
                    .strip_prefix(b"50;")
                    .map(|t| Input::Reply(Reply::Font(text(t)))),
                _ => content.strip_prefix(b"G").map(|g| {
                    let message = g.iter().position(|&b| b == b';').map_or(&[][..], |i| {
                        g.get(i.saturating_add(1)..).unwrap_or_default()
                    });
                    Input::Reply(Reply::Graphics(message == b"OK"))
                }),
            };
            took(end.saturating_add(end_len), input)
        }
        // another escape sequence, such as Alt and a key: dropped
        _ => took(0, None),
    }
}

fn arrow(b: u8) -> Option<Key> {
    match b {
        b'A' => Some(Key::Up),
        b'B' => Some(Key::Down),
        b'C' => Some(Key::Right),
        b'D' => Some(Key::Left),
        b'H' => Some(Key::Home),
        b'F' => Some(Key::End),
        _ => None,
    }
}

/// A control sequence after `ESC [`: parameters, intermediates, then a
/// final byte from `@` to `~`.
fn csi(body: &[u8]) -> Front {
    let Some(end) = body.iter().position(|&b| (0x40..=0x7e).contains(&b)) else {
        return if body.len() > STRING_MAX {
            Front::Took(body.len().saturating_add(2), None)
        } else {
            Front::Short
        };
    };
    let params = body.get(..end).unwrap_or_default();
    let last = body.get(end).copied().unwrap_or(0);
    let key = match (params, last) {
        (p, b'c') if p.first() == Some(&b'?') => {
            return Front::Took(end.saturating_add(3), Some(Input::Reply(Reply::Attributes)));
        }
        (b"" | b"1", b) if arrow(b).is_some() => arrow(b),
        (b"", b'Z') => Some(Key::BackTab),
        (b"5", b'~') => Some(Key::PageUp),
        (b"6", b'~') => Some(Key::PageDown),
        (b"1" | b"7", b'~') => Some(Key::Home),
        (b"4" | b"8", b'~') => Some(Key::End),
        _ => None,
    };
    Front::Took(end.saturating_add(3), key.map(Input::Key))
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn keys(bytes: &[u8]) -> Vec<Input> {
        Decoder::default().feed(bytes)
    }

    #[test]
    fn keys_decode_as_a_vt100_line_terminal_sends_them() {
        assert_eq!(
            keys(b"\x1b[A\x1bOB\x1b[C\x1b[D\r\t\x1b[Z\x7f\x03q\x1b[5~\x1b[6~\x1b[H\x1b[4~"),
            [
                Key::Up,
                Key::Down,
                Key::Right,
                Key::Left,
                Key::Enter,
                Key::Tab,
                Key::BackTab,
                Key::Backspace,
                Key::Interrupt,
                Key::Char('q'),
                Key::PageUp,
                Key::PageDown,
                Key::Home,
                Key::End,
            ]
            .map(Input::Key)
        );
        assert_eq!(keys("é".as_bytes()), [Input::Key(Key::Char('é'))]);
    }

    #[test]
    fn replies_decode_with_either_ending() {
        assert_eq!(
            keys(b"\x1bP>|vtium(0.1.0)\x1b\\\x1b]50;tiny5\x07\x1b]50;unifont\x1b\\\x1b_Gi=31;OK\x1b\\\x1b_Gi=31;ENOENT:no\x1b\\\x1b[?62;4c"),
            [
                Reply::Version("vtium(0.1.0)".into()),
                Reply::Font("tiny5".into()),
                Reply::Font("unifont".into()),
                Reply::Graphics(true),
                Reply::Graphics(false),
                Reply::Attributes,
            ]
            .map(Input::Reply)
        );
    }

    #[test]
    fn a_sequence_cut_by_a_read_waits_for_its_end() {
        let mut d = Decoder::default();
        assert_eq!(d.feed(b"\x1b"), []);
        assert_eq!(d.feed(b"]50;ti"), []);
        assert_eq!(d.feed(b"ny\x1b"), []);
        assert_eq!(
            d.feed(b"\\\x1b["),
            [Input::Reply(Reply::Font("tiny".into()))]
        );
        assert_eq!(d.feed(b"B"), [Input::Key(Key::Down)]);
        assert_eq!(d.feed(&"é".as_bytes()[..1]), []);
        assert_eq!(d.feed(&"é".as_bytes()[1..]), [Input::Key(Key::Char('é'))]);
    }

    #[test]
    fn an_endless_string_is_dropped_at_its_bound() {
        let mut d = Decoder::default();
        let mut long = b"\x1b]50;".to_vec();
        long.extend(std::iter::repeat_n(b'x', STRING_MAX + 10));
        assert_eq!(d.feed(&long), []);
        assert_eq!(d.feed(b"q"), [Input::Key(Key::Char('q'))]);
    }

    #[test]
    fn other_sequences_and_controls_are_dropped() {
        assert_eq!(keys(b"\x1b[200~\x1bx\x01\x1b[?1;2$y"), []);
    }
}
