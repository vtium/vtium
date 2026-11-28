//! The rebuilt TUI's decoder: the terminal's bytes in, keys, mouse reports
//! and replies out. It reads what the old decoder in [`crate::input`]
//! reads, the replies by that module's own functions, and adds the keys
//! the rebuilt TUI's menus, help, Find and undo need.
//!
//! A terminal sends Alt and a key as ESC followed by the key, in one write.
//! So ESC and a key in one read, where the key begins no sequence, is Alt
//! and that key: `ESC o` is Alt+o and `ESC CR` is Alt+Enter. ESC followed
//! by `[`, `O`, `P`, `]` or `_` begins a control sequence, SS3, a DCS, an
//! OSC or an APC. Alt+Shift+O, Alt+Shift+P, Alt+] and Alt+Shift+_ send
//! those same two bytes. When the two bytes end a read and no reply is
//! due, nothing will continue them, so they are Alt and that key at once.
//! While a reply is due they wait. A reply's ESC after them shows that no
//! sequence followed, so they are Alt and the key then.
//!
//! The control bytes 0x01 to 0x1A are Ctrl and a letter, such as 0x13 for
//! Ctrl+S, except Tab, Enter and Backspace, which keep their names. The
//! function keys arrive as SS3 `P` to `S` for F1 to F4 and as `CSI n ~`
//! for the rest, F10 as `CSI 21 ~`. The Linux console sends F1 to F5 as
//! `ESC [ [` and a letter from `A` to `E`, which the decoder reads as one
//! key, where a reading by ECMA-48 alone would end the sequence at the
//! second `[`. A modifier rides in xterm's form, as `CSI 1 ; 5 A` for
//! Ctrl+Up and `CSI 21 ; 2 ~` for Shift+F10.
//!
//! A lone ESC at the end of a read stays the Esc key, as the old decoder
//! reads it. With no reply due it is the Esc key at once. While a reply is
//! due it waits for the next byte: `[`, `]`, `_` or `P` begins a reply,
//! and any other byte makes the ESC the Esc key. ESC and a byte in two
//! reads are never Alt and the byte. An ESC inside a sequence cancels the
//! sequence, unless it begins ST, and a C0 control inside a control
//! sequence acts at once while the sequence goes on.
//!
//! References:
//! - ADR-1206 D2, the notes of 2026-11-27: Alt and a key, `ESC O`, `ESC
//!   P`, `ESC ]` and `ESC _` at the end of a read, F10, Ctrl+S, F1 in
//!   three forms, Ctrl+Z, Ctrl+Y, Ctrl+F and Delete.
//! - ADR-1206 D2, "Esc stays the key that goes back", and the notes of
//!   2026-11-19: the lone ESC, and the ESC that cancels a sequence.

use super::{Code, Event, Key, Mods};
use crate::input::{self, StringEnd};

/// The decoder: bytes in, [`Event`]s out, a sequence cut by a read kept
/// for the next.
#[derive(Debug, Default)]
pub struct Decoder {
    held: Vec<u8>,
    /// The ESC at the front of `held` ended an earlier read while a reply
    /// was awaited, so the byte after it decides what it is.
    lone: bool,
    /// The sequence at the front of `held` begins with an ESC that ended
    /// an earlier read and that the decoder took as the start of a reply.
    /// If another ESC cancels the sequence, a C0 control lands inside it,
    /// or no reply is due before it ends, that ESC was the Esc key.
    guessed: bool,
}

/// What the bytes at the front hold.
enum Front {
    /// So many bytes made this event, or nothing the TUI reads.
    Took(usize, Option<Event>),
    /// The bytes end before the sequence does.
    Short,
    /// An ESC this far in, which does not begin ST, cancels the sequence:
    /// the bytes before it are dropped, and decoding starts again at it.
    Cancelled(usize),
    /// A C0 control this far in, inside a control sequence: it acts at
    /// once, and the sequence goes on without it.
    Control(usize),
    /// ESC and `O`, `P`, `]` or `_`, then an ESC that begins no ST: no
    /// sequence followed the two bytes, so they are Alt and the key.
    Unbegun(Key),
}

impl Decoder {
    /// A decoder with nothing held.
    pub fn new() -> Decoder {
        Decoder::default()
    }

    /// Decodes `bytes` after what was held, with no reply awaited, and
    /// holds a sequence they end in the middle of.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        self.push(bytes);
        std::iter::from_fn(|| self.next(false)).collect()
    }

    /// Adds the bytes of a read after what is held, for [`Decoder::next`].
    pub fn push(&mut self, bytes: &[u8]) {
        self.held.extend_from_slice(bytes);
    }

    /// The next whole event in what is held, if there is one. `awaiting`
    /// says whether the TUI awaits a reply now, which decides what a lone
    /// ESC, or ESC and a byte that begins a sequence, at the end is. The
    /// loop asks again after each event it hands on, because the event
    /// may settle the last query outstanding.
    pub fn next(&mut self, awaiting: bool) -> Option<Event> {
        loop {
            if self.lone {
                match self.held.get(1) {
                    None if awaiting => return None,
                    // the ESC begins a reply
                    Some(b'[' | b']' | b'_' | b'P') => {
                        self.lone = false;
                        self.guessed = true;
                    }
                    // the queries are settled, or the byte after the ESC
                    // begins no reply: the ESC was the Esc key
                    _ => {
                        self.lone = false;
                        return Some(self.escape_key());
                    }
                }
            }
            if self.held.is_empty() {
                return None;
            }
            match front(&self.held) {
                // a reply's ESC cancelled the sequence that the guess began,
                // a C0 control landed inside it, which no reply holds, or no
                // reply is due to end it: the guess was wrong
                Front::Cancelled(_) | Front::Control(_) if self.guessed => {
                    return Some(self.escape_key());
                }
                // ESC and a byte that begins a sequence, with no sequence
                // after it, began with a guessed ESC: the Esc key and a key
                Front::Unbegun(_) if self.guessed => return Some(self.escape_key()),
                Front::Unbegun(k) => {
                    self.held.drain(..2);
                    return Some(Event::Key(k));
                }
                Front::Short if self.guessed && !awaiting => return Some(self.escape_key()),
                Front::Short if self.held.len() > input::STRING_MAX => {
                    self.guessed = false;
                    self.held.clear();
                }
                Front::Short if self.held == [0x1b] => {
                    if awaiting {
                        self.lone = true;
                        return None;
                    }
                    return Some(self.escape_key());
                }
                // with no reply due, nothing continues SS3 or a string that
                // has not ended: Alt and `O`, `P`, `]` or `_` began it, and
                // what follows is keys
                Front::Short if !awaiting => match self.held.as_slice() {
                    [0x1b, b @ (b'O' | b'P' | b']' | b'_'), ..] => {
                        let k = Key::alt(char::from(*b));
                        self.held.drain(..2);
                        return Some(Event::Key(k));
                    }
                    _ => return None,
                },
                Front::Short => return None,
                Front::Cancelled(n) => {
                    self.held.drain(..n.clamp(1, self.held.len()));
                }
                Front::Control(i) => {
                    if i < self.held.len() {
                        let control = self.held.remove(i);
                        if let Some(k) = control_key(control) {
                            return Some(Event::Key(k));
                        }
                    }
                }
                Front::Took(n, event) => {
                    self.guessed = false;
                    self.held.drain(..n.clamp(1, self.held.len()));
                    if event.is_some() {
                        return event;
                    }
                }
            }
        }
    }

    /// The ESC at the front was the Esc key: the decoder drops it, and
    /// decodes the bytes after it afresh.
    fn escape_key(&mut self) -> Event {
        self.guessed = false;
        self.held.drain(..1);
        Event::Key(Key::ESCAPE)
    }
}

/// The key a C0 control byte is, alone: Tab, Enter and Backspace by their
/// names, Ctrl and a letter for the rest from 0x01 to 0x1A, and `None` for
/// NUL and the four after ESC.
fn control_key(b: u8) -> Option<Key> {
    match b {
        b'\t' => Some(Key::plain(Code::Tab)),
        b'\r' | b'\n' => Some(Key::plain(Code::Enter)),
        0x08 | 0x7f => Some(Key::plain(Code::Backspace)),
        0x01..=0x1a => Some(Key::ctrl(char::from(b.saturating_add(0x60)))),
        _ => None,
    }
}

/// The event at the front of `bytes`.
fn front(bytes: &[u8]) -> Front {
    let Some((&first, rest)) = bytes.split_first() else {
        return Front::Short;
    };
    match first {
        0x1b => escape(rest),
        b if b < 0x20 || b == 0x7f => Front::Took(1, control_key(b).map(Event::Key)),
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
                    Some(c) => Front::Took(len, Some(Event::Key(Key::char(c)))),
                    None => Front::Took(1, None),
                },
            }
        }
    }
}

/// The event after an ESC; `rest` is what follows it.
fn escape(rest: &[u8]) -> Front {
    let Some((&kind, body)) = rest.split_first() else {
        return Front::Short;
    };
    let alt = |c: u8| Front::Unbegun(Key::alt(char::from(c)));
    match kind {
        // the first ESC is the Esc key, and the second begins what follows
        0x1b => Front::Took(1, Some(Event::Key(Key::ESCAPE))),
        b'[' => csi(body),
        b'O' => match body.first() {
            None => Front::Short,
            // SS3 holds no ESC: Alt+O, then whatever the ESC begins
            Some(0x1b) => alt(b'O'),
            Some(&b) if b < 0x20 => Front::Control(2),
            Some(&b) => Front::Took(3, ss3(b).map(|c| Event::Key(Key::plain(c)))),
        },
        b'P' | b']' | b'_' => match input::string_end(body, kind == b']') {
            None => Front::Short,
            // an ESC at once after the introducer, so no string began:
            // Alt and the key, then whatever the ESC begins
            Some(StringEnd::Cancelled(0)) => alt(kind),
            Some(StringEnd::Cancelled(i)) => Front::Cancelled(i.saturating_add(2)),
            Some(StringEnd::At(end, end_len)) => {
                let content = body.get(..end).unwrap_or_default();
                let reply = input::string_reply(kind, content).map(Event::Reply);
                Front::Took(end.saturating_add(end_len).saturating_add(2), reply)
            }
        },
        // Alt and the key that follows
        _ => match front(rest) {
            Front::Took(n, Some(Event::Key(k))) => {
                Front::Took(n.saturating_add(1), Some(Event::Key(k.with(Mods::ALT))))
            }
            Front::Took(n, _) => Front::Took(n.saturating_add(1), None),
            other => other,
        },
    }
}

/// The key SS3 and `b` send: F1 to F4, the arrows, Home and End.
fn ss3(b: u8) -> Option<Code> {
    match b {
        b'P' => Some(Code::F(1)),
        b'Q' => Some(Code::F(2)),
        b'R' => Some(Code::F(3)),
        b'S' => Some(Code::F(4)),
        b => lettered(b),
    }
}

/// The key `CSI b`, or SS3 `b`, sends among the arrows, Home and End.
fn lettered(b: u8) -> Option<Code> {
    match b {
        b'A' => Some(Code::Up),
        b'B' => Some(Code::Down),
        b'C' => Some(Code::Right),
        b'D' => Some(Code::Left),
        b'H' => Some(Code::Home),
        b'F' => Some(Code::End),
        _ => None,
    }
}

/// The key `CSI n ~` sends.
fn numbered(n: u16) -> Option<Code> {
    Some(match n {
        1 | 7 => Code::Home,
        2 => Code::Insert,
        3 => Code::Delete,
        4 | 8 => Code::End,
        5 => Code::PageUp,
        6 => Code::PageDown,
        11..=15 => Code::F(u8::try_from(n - 10).ok()?),
        17..=21 => Code::F(u8::try_from(n - 11).ok()?),
        23 | 24 => Code::F(u8::try_from(n - 12).ok()?),
        _ => return None,
    })
}

/// A control sequence after `ESC [`: parameters, intermediates, then a
/// final byte from `@` to `~`. The Linux console's `ESC [ [` and a letter
/// is read first. An ESC before the final byte cancels the sequence, and
/// any other C0 control there acts at once while the sequence goes on.
fn csi(body: &[u8]) -> Front {
    if body.first() == Some(&b'[') {
        return match body.get(1) {
            None => Front::Short,
            Some(0x1b) => Front::Cancelled(3),
            Some(&b) if b < 0x20 => Front::Control(3),
            Some(&b @ b'A'..=b'E') => {
                Front::Took(4, Some(Event::Key(Key::plain(Code::F(b - b'A' + 1)))))
            }
            // a sequence the TUI does not read, and the byte after it
            Some(_) => Front::Took(3, None),
        };
    }
    let Some(end) = body
        .iter()
        .position(|&b| b < 0x20 || (0x40..=0x7e).contains(&b))
    else {
        return if body.len() > input::STRING_MAX {
            Front::Took(body.len().saturating_add(2), None)
        } else {
            Front::Short
        };
    };
    let params = body.get(..end).unwrap_or_default();
    let last = body.get(end).copied().unwrap_or(0);
    let whole = end.saturating_add(3);
    if last == 0x1b {
        return Front::Cancelled(end.saturating_add(2));
    }
    if last < 0x20 {
        return Front::Control(end.saturating_add(2));
    }
    if let Some(sgr) = params.strip_prefix(b"<")
        && matches!(last, b'M' | b'm')
    {
        return Front::Took(whole, input::sgr_mouse(sgr, last == b'm').map(Event::Mouse));
    }
    if params.is_empty() && last == b'M' {
        // mode 1000 without 1006: three bytes, each its value plus 32. None
        // of them can be ESC, so an ESC among them begins what follows and
        // cancels the report.
        let three = body.get(end.saturating_add(1)..).unwrap_or_default();
        if let Some(i) = three.iter().take(3).position(|&b| b == 0x1b) {
            return Front::Cancelled(end.saturating_add(i).saturating_add(3));
        }
        let Some(&[b, x, y]) = three.get(..3) else {
            return Front::Short;
        };
        return Front::Took(
            end.saturating_add(6),
            input::x10_mouse(b, x, y).map(Event::Mouse),
        );
    }
    if last == b'c' && params.first() == Some(&b'?') {
        return Front::Took(whole, Some(Event::Reply(input::Reply::Attributes)));
    }
    if last == b't' {
        let reply = input::text_area(params).map(|wh| Event::Reply(input::Reply::TextArea(wh)));
        return Front::Took(whole, reply);
    }
    Front::Took(whole, csi_key(params, last).map(Event::Key))
}

/// The key a control sequence with `params` and the final byte `last`
/// sends, in xterm's form: `CSI 1 ; m A` for an arrow with the modifiers
/// `m`, `CSI n ; m ~` for a numbered key, `CSI 1 ; m P` for F1 to F4 with
/// modifiers, and `CSI Z` for Shift+Tab.
fn csi_key(params: &[u8], last: u8) -> Option<Key> {
    let text = std::str::from_utf8(params).ok()?;
    let mut numbers = text.split(';').map(|p| {
        if p.is_empty() {
            Some(1)
        } else if p.bytes().all(|b| b.is_ascii_digit()) {
            p.parse::<u16>().ok()
        } else {
            None
        }
    });
    let first = numbers.next().flatten()?;
    let mods = match numbers.next() {
        None => Mods::NONE,
        Some(m) => Mods::of_param(m?)?,
    };
    if numbers.next().is_some() {
        return None;
    }
    let modified = text.contains(';');
    let code = match last {
        b'~' => numbered(first)?,
        b'Z' if text.is_empty() => return Some(Key::plain(Code::Tab).with(Mods::SHIFT)),
        b'P'..=b'S' if first == 1 && modified => ss3(last)?,
        b if first == 1 => lettered(b)?,
        _ => return None,
    };
    Some(Key { mods, code })
}
