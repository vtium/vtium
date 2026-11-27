//! What the terminal sends the TUI, decoded: keys as a terminal of the
//! VT100 line sends them, as vtium-term encodes them, the mouse as the
//! flow view asks for it (ADR-1403 D7), and the replies to the TUI's
//! requests (ADR-1201 D4, D7).
//!
//! A sequence split across reads waits in the decoder for its end, so a
//! reply cut by a read comes out whole once the rest arrives.
//!
//! An ESC inside a string or a control sequence cancels that sequence,
//! unless it begins ST (`ESC \`), which ends a control string in ECMA-48.
//! Decoding starts again at that ESC, as a VT's parser does. So a reply
//! always comes out whole even when the Esc key or a broken sequence comes
//! before it. In `ESC ESC`, the first ESC is the Esc key and the second
//! begins whatever follows.
//!
//! Any other C0 control inside a control sequence acts at once as its key,
//! and the sequence goes on without it, as ECMA-48 and a VT's parser have
//! it. Alt+[ sends `ESC [`, which opens a control sequence, so Ctrl-C
//! typed after it still interrupts. No reply holds a C0 control, so one
//! inside a sequence that a lone ESC was guessed to begin shows that the
//! ESC was the Esc key.
//!
//! An ESC that is the last byte of a read can be the Esc key or the first
//! byte of a reply. The TUI's decoder ([`Decoder::keys`]) decides by
//! whether the TUI awaits a reply. With no query outstanding, the ESC is
//! the Esc key at once: a terminal writes a sequence in one write, and the
//! Esc key alone arrives alone. While a query is outstanding, the decoder
//! keeps the ESC until the next byte arrives, which it always does,
//! because DA1's reply is still due. If that byte continues a reply or a
//! key's sequence (`[`, `]`, `_` or `P`), the ESC begins it, and any other
//! byte makes the ESC the Esc key. The guess can still turn out wrong:
//! the person may press Esc and then `]`. Every reply begins with ESC, so
//! the reply that is still due cancels the sequence the guess began. The
//! decoder then knows the ESC was the Esc key, and decodes the bytes after
//! it as keys. Once no reply is due, a sequence the guess began and that
//! has not ended is taken as keys too. So a real Esc waits only while
//! replies are in flight. The decoder that only reads replies, as `set`
//! does, always keeps a lone ESC for the next read.
//!
//! A vtium console sends Alt and a key as ESC and the key, so Alt+Shift+P,
//! Alt+] and Alt+Shift+_ begin a DCS, an OSC and an APC. With no reply
//! due, nothing ends such a string, so the TUI's decoder takes the string
//! that has not ended as Alt and a key, which it drops, and decodes the
//! bytes after it as keys. Ctrl-C and Esc then still act. A reply that
//! arrives after the deadline at start cleared the queue, cut by a read
//! inside its string, decodes as keys too.
//!
//! References:
//! - ADR-1206 D2, "Esc stays the key that goes back": a lone ESC waits
//!   for its next byte only while a batch of queries is outstanding.
//! - ADR-1206 D2, the note of 2026-11-19: an ESC that does not begin ST
//!   cancels the sequence in progress, and a wrong guess about a lone ESC
//!   ends as the Esc key.

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
    /// The escape key, alone at the end of a read.
    Escape,
    /// A mouse event, which only the Flow view asks for.
    Mouse(Mouse),
    Char(char),
    /// Ctrl-D or Ctrl-U, by its letter, which the pager scrolls by half a
    /// page with (ADR-1403 D7).
    Ctrl(char),
}

/// A mouse event, as modes 1000 and 1006 report it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mouse {
    pub button: Button,
    /// The cell, counted from 0 at the top left.
    pub col: usize,
    pub row: usize,
}

/// What the mouse did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// A button went down: 0 the left, 1 the middle, 2 the right.
    Press(u8),
    /// A button came up.
    Release,
    /// The wheel turned up, away from the user.
    WheelUp,
    WheelDown,
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
    /// XTWINOPS 14's, `CSI 4 ; HEIGHT ; WIDTH t`: the text area's width
    /// and height in pixels.
    TextArea((u32, u32)),
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
    /// A lone ESC at the end of a read is the escape key, unless a reply
    /// is awaited.
    escape: bool,
    /// The ESC at the front of `held` ended an earlier read while a reply
    /// was awaited, so the byte after it decides what it is.
    lone: bool,
    /// The sequence at the front of `held` begins with an ESC that ended
    /// an earlier read and that the decoder took as the start of a reply.
    /// If another ESC cancels the sequence, or no reply is due before it
    /// ends, that ESC was the Esc key.
    guessed: bool,
}

/// What the bytes at the front hold.
enum Front {
    /// So many bytes made this input.
    Took(usize, Option<Input>),
    /// The bytes end before the sequence does.
    Short,
    /// An ESC this far in, which does not begin ST, cancels the sequence:
    /// the bytes before it are dropped, and decoding starts again at it.
    Cancelled(usize),
    /// A C0 control this far in, inside a control sequence: it acts at
    /// once, and the sequence goes on without it.
    Control(usize),
}

impl Decoder {
    /// The TUI's decoder, which takes an ESC that ends a read as the
    /// escape key unless a reply is awaited.
    pub fn keys() -> Decoder {
        Decoder {
            escape: true,
            ..Decoder::default()
        }
    }

    /// Decodes `bytes` after what was held, with no reply awaited, and
    /// holds a sequence they end in the middle of.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Input> {
        self.push(bytes);
        std::iter::from_fn(|| self.next(false)).collect()
    }

    /// Adds the bytes of a read after what is held, for [`Decoder::next`].
    pub fn push(&mut self, bytes: &[u8]) {
        self.held.extend_from_slice(bytes);
    }

    /// The next whole input in what is held, if there is one. `awaiting`
    /// says whether the TUI awaits a reply now, which decides what a lone
    /// ESC at the end is. The loop asks again after each input it hands
    /// on, because the input may settle the last query outstanding.
    pub fn next(&mut self, awaiting: bool) -> Option<Input> {
        loop {
            if self.lone {
                match self.held.get(1) {
                    None if awaiting => return None,
                    // the ESC begins a reply, or a key's sequence
                    Some(b'[' | b']' | b'_' | b'P') => {
                        self.lone = false;
                        self.guessed = true;
                    }
                    // the queries are settled, or the byte after the ESC
                    // begins nothing: the ESC was the Esc key
                    _ => {
                        self.lone = false;
                        self.held.drain(..1);
                        return Some(Input::Key(Key::Escape));
                    }
                }
            }
            if self.held.is_empty() {
                return None;
            }
            match front(&self.held) {
                // a reply's ESC cancelled the sequence that the guess began,
                // or no reply is due to end it: the guess was wrong
                Front::Cancelled(_) if self.guessed => return Some(self.escape_key()),
                // no reply holds a C0 control, so one inside the sequence
                // that the guess began shows the guess was wrong too
                Front::Control(_) if self.guessed => return Some(self.escape_key()),
                Front::Short if self.guessed && !awaiting => return Some(self.escape_key()),
                // with no reply due, nothing will end a string that has not
                // ended: Alt and `P`, `]` or `_` began it, which the decoder
                // drops as it drops any Alt chord, and what follows is keys
                Front::Short
                    if self.escape
                        && !awaiting
                        && matches!(self.held.as_slice(), [0x1b, b'P' | b']' | b'_', ..]) =>
                {
                    self.held.drain(..2);
                }
                Front::Cancelled(n) => {
                    self.held.drain(..n.clamp(1, self.held.len()));
                }
                Front::Control(i) => {
                    if i < self.held.len() {
                        let control = self.held.remove(i);
                        if let Front::Took(_, input @ Some(_)) = front(&[control]) {
                            return input;
                        }
                    }
                }
                Front::Took(n, input) => {
                    self.guessed = false;
                    self.held.drain(..n.clamp(1, self.held.len()));
                    if input.is_some() {
                        return input;
                    }
                }
                Front::Short if self.held.len() > STRING_MAX => {
                    self.guessed = false;
                    self.held.clear();
                }
                Front::Short if self.escape && self.held == [0x1b] => {
                    if awaiting {
                        self.lone = true;
                        return None;
                    }
                    self.held.clear();
                    return Some(Input::Key(Key::Escape));
                }
                Front::Short => return None,
            }
        }
    }

    /// The ESC at the front was the Esc key after all: the decoder drops
    /// it, and decodes the bytes after it afresh.
    fn escape_key(&mut self) -> Input {
        self.guessed = false;
        self.held.drain(..1);
        Input::Key(Key::Escape)
    }
}

/// How the string that starts at `body` ends.
enum StringEnd {
    /// At ST (`ESC \`) or, for OSC, BEL: the string's length and the end's.
    At(usize, usize),
    /// At an ESC, this far in, that does not begin ST.
    Cancelled(usize),
}

/// Where the string that starts at `body` ends, or `None` while it has not
/// ended, an ESC at the very end included, since it may begin ST.
fn string_end(body: &[u8], bel: bool) -> Option<StringEnd> {
    body.iter().enumerate().find_map(|(i, &b)| match b {
        0x07 if bel => Some(StringEnd::At(i, 1)),
        0x1b => match body.get(i.saturating_add(1)) {
            None => None,
            Some(b'\\') => Some(StringEnd::At(i, 2)),
            Some(_) => Some(StringEnd::Cancelled(i)),
        },
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
        0x04 => key(1, Key::Ctrl('d')),
        0x15 => key(1, Key::Ctrl('u')),
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
        // the first ESC is the Esc key, and the second begins what follows
        0x1b => Front::Took(1, Some(Input::Key(Key::Escape))),
        b'[' => csi(body),
        b'O' => match body.first() {
            None => Front::Short,
            // Alt and O, then whatever the ESC begins
            Some(0x1b) => Front::Cancelled(2),
            Some(&b) => took(1, arrow(b).map(Input::Key)),
        },
        b'P' | b']' | b'_' => {
            let (end, end_len) = match string_end(body, kind == b']') {
                None => return Front::Short,
                Some(StringEnd::Cancelled(i)) => return Front::Cancelled(i.saturating_add(2)),
                Some(StringEnd::At(end, end_len)) => (end, end_len),
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
/// final byte from `@` to `~`. An ESC before the final byte cancels the
/// sequence, and any other C0 control there acts at once while the
/// sequence goes on.
fn csi(body: &[u8]) -> Front {
    let Some(end) = body
        .iter()
        .position(|&b| b < 0x20 || (0x40..=0x7e).contains(&b))
    else {
        return if body.len() > STRING_MAX {
            Front::Took(body.len().saturating_add(2), None)
        } else {
            Front::Short
        };
    };
    let params = body.get(..end).unwrap_or_default();
    let last = body.get(end).copied().unwrap_or(0);
    if last == 0x1b {
        return Front::Cancelled(end.saturating_add(2));
    }
    if last < 0x20 {
        return Front::Control(end.saturating_add(2));
    }
    if let Some(sgr) = params.strip_prefix(b"<")
        && matches!(last, b'M' | b'm')
    {
        let mouse = sgr_mouse(sgr, last == b'm').map(|m| Input::Key(Key::Mouse(m)));
        return Front::Took(end.saturating_add(3), mouse);
    }
    if params.is_empty() && last == b'M' {
        // mode 1000 without 1006: three bytes, each its value plus 32. None
        // of them can be ESC, so an ESC among them begins what follows and
        // cancels the report.
        let mut three = body
            .get(end.saturating_add(1)..)
            .unwrap_or_default()
            .iter()
            .take(3);
        if let Some(i) = three.position(|&b| b == 0x1b) {
            return Front::Cancelled(end.saturating_add(i).saturating_add(3));
        }
        let Some(&[b, x, y]) = body.get(end.saturating_add(1)..end.saturating_add(4)) else {
            return Front::Short;
        };
        let mouse = button(u16::from(b.saturating_sub(32)), false).map(|button| Mouse {
            button,
            col: usize::from(x.saturating_sub(33)),
            row: usize::from(y.saturating_sub(33)),
        });
        return Front::Took(
            end.saturating_add(6),
            mouse.map(|m| Input::Key(Key::Mouse(m))),
        );
    }
    let key = match (params, last) {
        (p, b'c') if p.first() == Some(&b'?') => {
            return Front::Took(end.saturating_add(3), Some(Input::Reply(Reply::Attributes)));
        }
        (p, b't') => {
            let reply = text_area(p).map(|wh| Input::Reply(Reply::TextArea(wh)));
            return Front::Took(end.saturating_add(3), reply);
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

/// The width and height in XTWINOPS 14's reply, from its parameters
/// `4;HEIGHT;WIDTH`; `None` for any other report.
fn text_area(params: &[u8]) -> Option<(u32, u32)> {
    let text = std::str::from_utf8(params).ok()?;
    let mut parts = text.split(';');
    if parts.next()? != "4" {
        return None;
    }
    let side = |s: Option<&str>| {
        s.filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))?
            .parse::<u32>()
            .ok()
    };
    let height = side(parts.next())?;
    let width = side(parts.next())?;
    parts.next().is_none().then_some((width, height))
}

/// What a mouse report's button number says: its low two bits and the
/// wheel's bit, with the modifier bits (4, 8, 16) and motion (32) left out;
/// `None` for motion, which the TUI never asks for.
fn button(cb: u16, released: bool) -> Option<Button> {
    if cb & 32 != 0 {
        return None;
    }
    let low = u8::try_from(cb & 3).unwrap_or(0);
    Some(match (cb & 64 != 0, low) {
        (true, 0) => Button::WheelUp,
        (true, 1) => Button::WheelDown,
        (true, _) => return None,
        (false, 3) => Button::Release,
        (false, _) if released => Button::Release,
        (false, b) => Button::Press(b),
    })
}

/// A mode 1006 report's parameters, `Cb;Cx;Cy`, the cell counted from 1.
fn sgr_mouse(params: &[u8], released: bool) -> Option<Mouse> {
    let text = std::str::from_utf8(params).ok()?;
    let mut n = text.split(';').map(|p| p.parse::<u16>().ok());
    let (cb, x, y) = (n.next()??, n.next()??, n.next()??);
    if n.next().is_some() {
        return None;
    }
    Some(Mouse {
        button: button(cb, released)?,
        col: usize::from(x.checked_sub(1)?),
        row: usize::from(y.checked_sub(1)?),
    })
}

#[cfg(test)]
#[expect(clippy::indexing_slicing, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn keys(bytes: &[u8]) -> Vec<Input> {
        Decoder::keys().feed(bytes)
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
    fn the_text_area_s_reply_decodes_width_first() {
        assert_eq!(
            keys(b"\x1b[4;4320;7680t"),
            [Input::Reply(Reply::TextArea((7680, 4320)))]
        );
        // another report, or a text area's reply that does not read, is
        // no reply the TUI asked for
        for other in [
            b"\x1b[8;24;80t".as_slice(),
            b"\x1b[4;4320t",
            b"\x1b[4;4320;7680;1t",
            b"\x1b[4;;7680t",
            b"\x1b[5;4320;7680t",
        ] {
            assert_eq!(keys(other), [], "{other:?}");
        }
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
        assert_eq!(d.feed(b"\x1b]"), []);
        assert_eq!(d.feed(b"50;ti"), []);
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
    fn an_esc_that_ends_a_read_is_the_escape_key() {
        assert_eq!(keys(b"\x1b"), [Input::Key(Key::Escape)]);
        assert_eq!(
            keys(b"q\x1b"),
            [Input::Key(Key::Char('q')), Input::Key(Key::Escape)]
        );
        // an ESC followed by more in the same read starts a sequence
        assert_eq!(keys(b"\x1b[A"), [Input::Key(Key::Up)]);
        let mut d = Decoder::keys();
        assert_eq!(d.feed(b"\x1b"), [Input::Key(Key::Escape)]);
        // the decoder that reads replies keeps a lone ESC for the next read
        let mut replies = Decoder::default();
        assert_eq!(replies.feed(b"\x1b"), []);
        assert_eq!(replies.feed(b"[B"), [Input::Key(Key::Down)]);
        assert_eq!(
            d.feed(b"[B"),
            [Input::Key(Key::Char('[')), Input::Key(Key::Char('B'))]
        );
    }

    #[test]
    fn a_lone_esc_waits_for_its_next_byte_while_a_reply_is_awaited() {
        let font = |n: &str| Some(Input::Reply(Reply::Font(n.into())));
        let esc = Some(Input::Key(Key::Escape));
        let mut d = Decoder::keys();
        // the ESC begins DA1's reply, which the read cut
        d.push(b"\x1b]50;tiny5\x1b\\\x1b");
        assert_eq!(d.next(true), font("tiny5"));
        assert_eq!(d.next(true), None);
        d.push(b"[?62;4c");
        assert_eq!(d.next(true), Some(Input::Reply(Reply::Attributes)));
        assert_eq!(d.next(false), None);
        // the Esc key, then a reply: the ESC after it makes it the key
        d.push(b"\x1b");
        assert_eq!(d.next(true), None);
        d.push(b"\x1b]50;tiny5\x07");
        assert_eq!(d.next(true), esc);
        assert_eq!(d.next(true), font("tiny5"));
        // the Esc key, then a letter
        d.push(b"\x1b");
        assert_eq!(d.next(true), None);
        d.push(b"q");
        assert_eq!(d.next(true), esc);
        assert_eq!(d.next(true), Some(Input::Key(Key::Char('q'))));
        // the queries are settled and nothing more came: the Esc key
        d.push(b"\x1b");
        assert_eq!(d.next(true), None);
        assert_eq!(d.next(false), esc);
        assert_eq!(d.next(false), None);
        // with no reply awaited, the Esc key at once
        d.push(b"\x1b");
        assert_eq!(d.next(false), esc);
        // the decoder that reads replies only never takes it as a key
        let mut replies = Decoder::default();
        replies.push(b"\x1b");
        assert_eq!(replies.next(false), None);
    }

    /// Everything the decoder holds whole, with `awaiting` fixed.
    fn drain(d: &mut Decoder, awaiting: bool) -> Vec<Input> {
        std::iter::from_fn(|| d.next(awaiting)).collect()
    }

    const ESC: Input = Input::Key(Key::Escape);
    const DA1: Input = Input::Reply(Reply::Attributes);

    fn font(name: &str) -> Input {
        Input::Reply(Reply::Font(name.into()))
    }

    #[test]
    fn in_esc_esc_the_first_is_the_escape_key_and_the_second_begins_a_reply() {
        // the Esc key and a whole batch's replies in one read
        let mut d = Decoder::keys();
        d.push(b"\x1b\x1b]50;tiny5\x1b\\\x1b[?62;4c");
        assert_eq!(drain(&mut d, true), [ESC, font("tiny5"), DA1]);
        // the Esc key between a batch's replies, in one read
        d.push(b"\x1b]50;tiny5\x1b\\\x1b\x1b[?62;4c");
        assert_eq!(drain(&mut d, true), [font("tiny5"), ESC, DA1]);
        // the decoder that reads replies only keeps every reply too
        let mut replies = Decoder::default();
        assert_eq!(
            replies.feed(b"\x1b\x1bP>|vtium(0.1.0)\x1b\\\x1b\x1b[?62;4c"),
            [
                ESC,
                Input::Reply(Reply::Version("vtium(0.1.0)".into())),
                ESC,
                DA1
            ]
        );
    }

    #[test]
    fn a_lone_esc_then_a_reply_in_the_next_read_is_the_escape_key() {
        let replies: [(&[u8], Input); 3] = [
            (b"\x1b[?62;4c", DA1),
            (
                b"\x1bP>|vtium(0.1.0)\x1b\\",
                Input::Reply(Reply::Version("vtium(0.1.0)".into())),
            ),
            (b"\x1b_Gi=31;OK\x1b\\", Input::Reply(Reply::Graphics(true))),
        ];
        for (reply, heard) in replies {
            let mut d = Decoder::keys();
            d.push(b"\x1b");
            assert_eq!(drain(&mut d, true), []);
            d.push(reply);
            assert_eq!(drain(&mut d, true), [ESC, heard], "{reply:?}");
        }
    }

    /// The Esc key, then a key whose byte could continue a reply, each in
    /// a read of its own, then a reply. The reply's ESC shows that the
    /// first ESC began no reply, so it is the Esc key, and the byte after
    /// it is a key of its own.
    #[test]
    fn a_reply_after_the_esc_key_and_a_bracket_shows_the_esc_was_the_key() {
        for typed in *b"[]_P" {
            for (reply, heard) in [
                (&b"\x1b[?62;4c"[..], vec![DA1]),
                (b"\x1b]50;tiny5\x1b\\\x1b[?62;4c", vec![font("tiny5"), DA1]),
                (
                    b"\x1bP>|vtium(0.1.0)\x1b\\\x1b[?62;4c",
                    vec![Input::Reply(Reply::Version("vtium(0.1.0)".into())), DA1],
                ),
                (
                    b"\x1b_Gi=31;OK\x1b\\\x1b[?62;4c",
                    vec![Input::Reply(Reply::Graphics(true)), DA1],
                ),
            ] {
                let mut d = Decoder::keys();
                d.push(b"\x1b");
                assert_eq!(drain(&mut d, true), []);
                d.push(&[typed]);
                assert_eq!(drain(&mut d, true), [], "{}", char::from(typed));
                d.push(reply);
                let mut want = vec![ESC, Input::Key(Key::Char(char::from(typed)))];
                want.extend(heard);
                assert_eq!(drain(&mut d, true), want, "{}", char::from(typed));
            }
        }
    }

    /// The Esc key and a bracket, and then the queries are settled with no
    /// reply after them: no reply is due, so the ESC was the Esc key.
    #[test]
    fn an_esc_and_a_bracket_are_keys_once_no_reply_is_due() {
        let mut d = Decoder::keys();
        d.push(b"\x1b");
        assert_eq!(drain(&mut d, true), []);
        d.push(b"]");
        assert_eq!(drain(&mut d, true), []);
        assert_eq!(drain(&mut d, false), [ESC, Input::Key(Key::Char(']'))]);
    }

    /// An ESC that does not begin ST cancels the string or control
    /// sequence in progress, and decoding starts again at that ESC.
    #[test]
    fn an_esc_inside_a_sequence_cancels_it() {
        assert_eq!(keys(b"\x1b]50;tin\x1b[?62;4c"), [DA1]);
        assert_eq!(keys(b"\x1bP>|vt\x1b]50;tiny5\x07"), [font("tiny5")]);
        assert_eq!(keys(b"\x1b_Gi=31\x1b[A"), [Input::Key(Key::Up)]);
        assert_eq!(keys(b"\x1b[1;\x1b[?62;4c"), [DA1]);
        assert_eq!(keys(b"\x1bO\x1b[B"), [Input::Key(Key::Down)]);
        // ST still ends a string, and a read may cut it
        let mut d = Decoder::default();
        assert_eq!(d.feed(b"\x1b]50;tiny5\x1b"), []);
        assert_eq!(d.feed(b"\\"), [font("tiny5")]);
    }

    #[test]
    fn mouse_reports_decode_in_both_encodings() {
        let at = |button, col, row| Input::Key(Key::Mouse(Mouse { button, col, row }));
        assert_eq!(
            keys(b"\x1b[<0;10;5M\x1b[<0;10;5m\x1b[<2;1;1M\x1b[<64;3;4M\x1b[<65;3;4M"),
            [
                at(Button::Press(0), 9, 4),
                at(Button::Release, 9, 4),
                at(Button::Press(2), 0, 0),
                at(Button::WheelUp, 2, 3),
                at(Button::WheelDown, 2, 3),
            ]
        );
        // Ctrl held (16) still clicks; motion (32) is dropped
        assert_eq!(keys(b"\x1b[<16;2;2M"), [at(Button::Press(0), 1, 1)]);
        assert_eq!(keys(b"\x1b[<32;2;2M"), []);
        // mode 1000 alone: each byte its value plus 32, the cell from 1
        assert_eq!(
            keys(b"\x1b[M #%\x1b[M##%"),
            [at(Button::Press(0), 2, 4), at(Button::Release, 2, 4)]
        );
        // cut by a read, the three bytes wait
        let mut d = Decoder::keys();
        assert_eq!(d.feed(b"\x1b[M "), []);
        assert_eq!(d.feed(b"#%"), [at(Button::Press(0), 2, 4)]);
        // a report with a cell of 0 or too many numbers is dropped
        assert_eq!(keys(b"\x1b[<0;0;5M\x1b[<0;1;2;3M"), []);
    }

    /// A mode 1000 report's three bytes are each a value plus 32, so an
    /// ESC among them is the start of what follows, and it cancels the
    /// report as it cancels any other sequence.
    #[test]
    fn an_esc_among_a_mouse_reports_three_bytes_cancels_the_report() {
        // ESC [ M, then DA1's reply, in one read
        let mut d = Decoder::keys();
        d.push(b"\x1b[M\x1b[?62;4c");
        assert_eq!(drain(&mut d, true), [DA1]);
        // ESC [ M, then the font reply and DA1's, in one read
        d.push(b"\x1b[M\x1b]50;tiny5\x1b\\\x1b[?62;4c");
        assert_eq!(drain(&mut d, true), [font("tiny5"), DA1]);
        // the ESC as the second or third of the three bytes
        d.push(b"\x1b[M \x1b[?62;4c\x1b[M #\x1b[?62;4c");
        assert_eq!(drain(&mut d, true), [DA1, DA1]);
        // the Esc key, `[` and `M`, each in a read of its own, then DA1's
        // reply: the ESC was the Esc key, and `[` and `M` are keys
        let mut d = Decoder::keys();
        for typed in [&b"\x1b"[..], b"[", b"M"] {
            d.push(typed);
            assert_eq!(drain(&mut d, true), []);
        }
        d.push(b"\x1b[?62;4c");
        assert_eq!(
            drain(&mut d, true),
            [
                ESC,
                Input::Key(Key::Char('[')),
                Input::Key(Key::Char('M')),
                DA1
            ]
        );
    }

    /// A vtium console sends Alt and a key as ESC and the key, so Alt+Shift+P,
    /// Alt+] and Alt+Shift+_ begin a DCS, an OSC and an APC. With no reply
    /// due, no reply can end such a string: it is Alt and a key, which the
    /// decoder drops as it drops `ESC x`, and the bytes after it are keys.
    #[test]
    fn with_no_reply_due_an_unfinished_string_is_alt_and_a_key() {
        const INTERRUPT: Input = Input::Key(Key::Interrupt);
        for alt in [&b"\x1bP"[..], b"\x1b]", b"\x1b_"] {
            let mut d = Decoder::keys();
            d.push(alt);
            assert_eq!(drain(&mut d, false), [], "{alt:?}");
            d.push(b"q");
            assert_eq!(drain(&mut d, false), [Input::Key(Key::Char('q'))]);
            d.push(b"\x03");
            assert_eq!(drain(&mut d, false), [INTERRUPT]);
            d.push(b"\x1b");
            assert_eq!(drain(&mut d, false), [ESC]);
            // the key and the bytes after it in one read
            let mut d = Decoder::keys();
            d.push(&[alt, b"q\x03\x1b"].concat());
            assert_eq!(
                drain(&mut d, false),
                [Input::Key(Key::Char('q')), INTERRUPT, ESC],
                "{alt:?}"
            );
        }
        // while a reply is due, the string waits for the rest of the reply
        let mut d = Decoder::keys();
        d.push(b"\x1b]50;tin");
        assert_eq!(drain(&mut d, true), []);
        d.push(b"y5\x1b\\");
        assert_eq!(drain(&mut d, true), [font("tiny5")]);
    }

    /// Alt+[ sends `ESC [`, which opens a control sequence. A C0 control
    /// inside a control sequence acts at once as its key, and the sequence
    /// goes on, as ECMA-48 and a VT's parser have it.
    #[test]
    fn a_c0_control_inside_a_control_sequence_acts_at_once() {
        const INTERRUPT: Input = Input::Key(Key::Interrupt);
        for awaiting in [false, true] {
            // Alt+[, then Ctrl-C in a read of its own, then the final byte
            let mut d = Decoder::keys();
            d.push(b"\x1b[");
            assert_eq!(drain(&mut d, awaiting), []);
            d.push(b"\x03");
            assert_eq!(drain(&mut d, awaiting), [INTERRUPT], "{awaiting}");
            d.push(b"A");
            assert_eq!(drain(&mut d, awaiting), [Input::Key(Key::Up)], "{awaiting}");
            // all in one read, with parameters on both sides of the control
            let mut d = Decoder::keys();
            d.push(b"\x1b[1\x03~");
            assert_eq!(
                drain(&mut d, awaiting),
                [INTERRUPT, Input::Key(Key::Home)],
                "{awaiting}"
            );
        }
        // the other C0 controls act as the decoder maps them, and a control
        // with no key is dropped, while the sequence goes on
        assert_eq!(
            keys(b"\x1b[\r\t\x08\x04\x15\x01A"),
            [
                Key::Enter,
                Key::Tab,
                Key::Backspace,
                Key::Ctrl('d'),
                Key::Ctrl('u'),
                Key::Up
            ]
            .map(Input::Key)
        );
        // a reply goes on around the control too
        assert_eq!(keys(b"\x1b[?62\x03;4c"), [INTERRUPT, DA1]);
        // a mouse report: a control among its parameters acts, and its three
        // bytes after `ESC [ M`, each 0x20 or above, are left as they are
        let at = |button, col, row| Input::Key(Key::Mouse(Mouse { button, col, row }));
        assert_eq!(
            keys(b"\x1b[<0;\x0310;5M"),
            [INTERRUPT, at(Button::Press(0), 9, 4)]
        );
        let mut d = Decoder::keys();
        assert_eq!(d.feed(b"\x1b["), []);
        assert_eq!(d.feed(b"M"), []);
        assert_eq!(d.feed(b" #%"), [at(Button::Press(0), 2, 4)]);
    }

    /// The Esc key and `[`, each in a read of its own while a reply is
    /// due, then Ctrl-C. No reply holds a C0 control, so the ESC was the
    /// Esc key, and the keys come out in the order they were typed.
    #[test]
    fn a_c0_control_after_a_guessed_esc_shows_the_esc_was_the_key() {
        let mut d = Decoder::keys();
        for typed in [&b"\x1b"[..], b"["] {
            d.push(typed);
            assert_eq!(drain(&mut d, true), []);
        }
        d.push(b"\x03");
        assert_eq!(
            drain(&mut d, true),
            [ESC, Input::Key(Key::Char('[')), Input::Key(Key::Interrupt)]
        );
        d.push(b"\x1b[?62;4c");
        assert_eq!(drain(&mut d, true), [DA1]);
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
