//! XTGETTCAP's table and its lookup (aaronsb/libtsm#80): the capabilities
//! the host gives the terminal, generated from vtium's terminfo entry
//! (vtium/vtium#21), and the reply to `DCS + q names ST`.
//!
//! The reply is xterm's: `DCS 1 + r name=value;name=value ST`, each value
//! hex encoded and a boolean its name alone. Each name is printed in
//! uppercase hex from the table's entry, never in the request's spelling,
//! by the reply rule (docs/architecture.md, "Determinism"); the C reference
//! echoed the request's spelling. A name the table lacks, or that is not
//! hex for a name, ends the list there, after the `;` before it, and when
//! it is the first the reply is `DCS 0 + r ST`. So does the name past
//! [`XTGETTCAP_NAMES`], or one whose pair would take the reply past
//! [`XTGETTCAP_REPLY_MAX`] bytes.

use crate::bounds::{
    TERMCAP_ENTRIES_MAX, TERMCAP_NAME_MAX, TERMCAP_VALUE_MAX, XTGETTCAP_NAMES, XTGETTCAP_REPLY_MAX,
};

/// One capability: its name, and its value, none for a boolean.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    name: Box<[u8]>,
    value: Option<Box<[u8]>>,
}

/// The capabilities XTGETTCAP answers from, sorted by name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Termcap {
    entries: Vec<Entry>,
}

/// Why [`Termcap::new`] refused a table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TermcapError {
    /// More than [`TERMCAP_ENTRIES_MAX`] capabilities.
    TooMany,
    /// A name that is empty, longer than [`TERMCAP_NAME_MAX`] bytes, or
    /// holds a byte outside 0x21 to 0x7E.
    BadName(Vec<u8>),
    /// A value longer than [`TERMCAP_VALUE_MAX`] bytes, or holding a NUL.
    BadValue(Vec<u8>),
    /// A name given twice.
    Duplicate(Vec<u8>),
}

fn name_ok(name: &[u8]) -> bool {
    !name.is_empty()
        && name.len() <= TERMCAP_NAME_MAX
        && name.iter().all(|b| (0x21..=0x7e).contains(b))
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn push_hex(out: &mut Vec<u8>, bytes: &[u8]) {
    for &b in bytes {
        out.push(HEX.get(usize::from(b >> 4)).copied().unwrap_or(b'0'));
        out.push(HEX.get(usize::from(b & 0xf)).copied().unwrap_or(b'0'));
    }
}

fn hex_digit(b: u8) -> Option<u8> {
    char::from(b)
        .to_digit(16)
        .and_then(|d| u8::try_from(d).ok())
}

impl Termcap {
    /// A table of `(name, value)` pairs, a boolean with no value.
    pub fn new<'a>(
        entries: impl IntoIterator<Item = (&'a [u8], Option<&'a [u8]>)>,
    ) -> Result<Self, TermcapError> {
        let mut table = Vec::new();
        for (name, value) in entries {
            if table.len() >= TERMCAP_ENTRIES_MAX {
                return Err(TermcapError::TooMany);
            }
            if !name_ok(name) {
                return Err(TermcapError::BadName(name.to_vec()));
            }
            if let Some(v) = value {
                if v.len() > TERMCAP_VALUE_MAX || v.contains(&0) {
                    return Err(TermcapError::BadValue(name.to_vec()));
                }
            }
            table.push(Entry {
                name: name.into(),
                value: value.map(Into::into),
            });
        }
        table.sort_by(|a, b| a.name.cmp(&b.name));
        if let Some(pair) = table
            .windows(2)
            .find(|w| matches!(w, [a, b] if a.name == b.name))
        {
            let name = pair.first().map(|e| e.name.to_vec()).unwrap_or_default();
            return Err(TermcapError::Duplicate(name));
        }
        Ok(Self { entries: table })
    }

    /// The entry a hex-encoded name names.
    fn lookup(&self, hex: &[u8]) -> Option<&Entry> {
        if hex.is_empty() || hex.len() % 2 != 0 || hex.len() / 2 > TERMCAP_NAME_MAX {
            return None;
        }
        let mut name = [0_u8; TERMCAP_NAME_MAX];
        for (slot, pair) in name.iter_mut().zip(hex.chunks_exact(2)) {
            let (hi, lo) = (hex_digit(*pair.first()?)?, hex_digit(*pair.get(1)?)?);
            *slot = hi << 4 | lo;
        }
        let name = name.get(..hex.len() / 2)?;
        if !name_ok(name) {
            return None;
        }
        let at = self
            .entries
            .binary_search_by(|e| e.name.as_ref().cmp(name))
            .ok()?;
        self.entries.get(at)
    }

    /// The reply to an XTGETTCAP request: `payload` is its names, or
    /// `None` when they passed the parser's bound, which is refused whole.
    pub(crate) fn answer(&self, payload: Option<&[u8]>) -> Vec<u8> {
        const REFUSED: &[u8] = b"\x1bP0+r\x1b\\";
        let Some(payload) = payload else {
            return REFUSED.to_vec();
        };
        let mut names = payload.split(|&b| b == b';').peekable();
        let mut out = b"\x1bP1+r".to_vec();
        let mut answered = 0;
        while let Some(hex) = names.next() {
            let Some(entry) = self.lookup(hex) else {
                break;
            };
            if answered >= XTGETTCAP_NAMES {
                break;
            }
            let value_len = entry
                .value
                .as_ref()
                .map_or(0, |v| v.len().saturating_mul(2).saturating_add(1));
            let more = names.peek().is_some();
            // The pair, the `;` when a name follows, and the ST that ends
            // the reply must fit.
            let need = out
                .len()
                .saturating_add(entry.name.len().saturating_mul(2))
                .saturating_add(value_len)
                .saturating_add(usize::from(more))
                .saturating_add(2);
            if need > XTGETTCAP_REPLY_MAX {
                break;
            }
            push_hex(&mut out, &entry.name);
            if let Some(value) = &entry.value {
                out.push(b'=');
                push_hex(&mut out, value);
            }
            if more {
                out.push(b';');
            }
            answered = answered.saturating_add(1);
        }
        if answered == 0 {
            return REFUSED.to_vec();
        }
        out.extend_from_slice(b"\x1b\\");
        out
    }
}
