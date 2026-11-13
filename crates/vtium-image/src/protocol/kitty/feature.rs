//! The specification's feature queries: a query (`a=q`) that carries the
//! key `e`, which names the action the program asks about (ADR-1100,
//! D14).
//!
//! vtium answers `ESC _ G i=ID,e=E,KEYS;OK ESC \`. KEYS lists each key
//! of the query whose value is not the key's default and is one a real
//! command of action E takes, with that value. Each key is checked on its
//! own, not against the query's other keys, since the specification says
//! a feature query only checks whether keys and values are understood.
//! The keys follow `i` and `e` in the byte order of their letters, capitals
//! first, which puts the specification's own example reply in its order.
//! An `e` that names no action is answered `i=ID;OK`, with no `e`. A
//! feature query stores nothing and never touches an upload in chunks.
//!
//! Every value in the answer is re-encoded: a number is written from the
//! number read, and a letter is one of a fixed set, so no byte the program
//! sent reaches the answer as it was sent.
//!
//! References: ADR-1100 D6 (Replies), D13, D14, D15; ADR-1102 D6.

use core::fmt::Write as _;
use std::collections::BTreeMap;

use crate::bounds::BOX_CELLS_MAX;
use crate::handler::{Context, ReplyKind};

use super::command::{Command, graphic, number, signed};

/// A value vtium reports for a key, re-encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    Number(u32),
    Signed(i32),
    Letter(u8),
}

/// What an action reads its keys as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// `t` and `q`: a transmission's keys.
    Transmit,
    /// `T`: a transmission's keys and a put's.
    TransmitPut,
    /// `p`: a put's keys.
    Put,
    /// `d`: a delete's keys.
    Delete,
    /// `f`: a frame's keys.
    Frame,
    /// `a`: an animation control's keys.
    Control,
    /// `c`: a composition's keys.
    Compose,
}

fn kind(action: u8) -> Option<Kind> {
    Some(match action {
        b't' | b'q' => Kind::Transmit,
        b'T' => Kind::TransmitPut,
        b'p' => Kind::Put,
        b'd' => Kind::Delete,
        b'f' => Kind::Frame,
        b'a' => Kind::Control,
        b'c' => Kind::Compose,
        _ => return None,
    })
}

/// A number other than 0, the default of every number key.
fn set(value: &[u8]) -> Option<u32> {
    number(value).filter(|&n| n != 0)
}

/// A signed number other than 0.
fn set_signed(value: &[u8]) -> Option<i32> {
    signed(value).filter(|&n| n != 0)
}

/// One letter of `allowed`.
fn letter(value: &[u8], allowed: &[u8]) -> Option<u8> {
    match value {
        [c] if allowed.contains(c) => Some(*c),
        _ => None,
    }
}

/// A number equal to `want`.
fn exactly(value: &[u8], want: u32) -> Option<Value> {
    (number(value) == Some(want)).then_some(Value::Number(want))
}

/// What a transmission takes (`t`, `T`, `q`): the formats vtium decodes,
/// the named media, zlib, a side within `max-graphic-size`, a size and
/// offset into a named medium, and the transient hint.
fn transmit(ctx: &dyn Context, key: u8, value: &[u8]) -> Option<Value> {
    let (max_w, max_h) = graphic(ctx);
    match key {
        b'f' => set(value)
            .filter(|f| matches!(f, 8 | 16 | 24 | 100))
            .map(Value::Number),
        b't' => letter(value, b"fts").map(Value::Letter),
        b'o' => letter(value, b"z").map(Value::Letter),
        b's' => set(value).filter(|&s| s <= max_w).map(Value::Number),
        b'v' => set(value).filter(|&v| v <= max_h).map(Value::Number),
        b'S' | b'O' => set(value).map(Value::Number),
        b'N' => exactly(value, 1),
        _ => None,
    }
}

/// What a put takes (`p`, `T`): the source rectangle, an offset inside
/// the cell, a box within `BOX_CELLS_MAX`, the cursor policy, Unicode
/// placeholders, the depth, a relative placement and a placement id.
fn put(ctx: &dyn Context, key: u8, value: &[u8]) -> Option<Value> {
    let (cw, ch) = ctx.cell_size();
    let boxed = |n: &u32| u64::from(*n) <= BOX_CELLS_MAX;
    match key {
        b'x' | b'y' | b'w' | b'h' | b'P' | b'Q' | b'p' => set(value).map(Value::Number),
        b'X' => set(value).filter(|&x| x < cw).map(Value::Number),
        b'Y' => set(value).filter(|&y| y < ch).map(Value::Number),
        b'c' | b'r' => set(value).filter(boxed).map(Value::Number),
        b'C' | b'U' => exactly(value, 1),
        b'z' | b'H' | b'V' => set_signed(value).map(Value::Signed),
        _ => None,
    }
}

/// Whether vtium takes `value` for `key` on a command of kind `kind`.
fn supported(ctx: &dyn Context, kind: Kind, key: u8, value: &[u8]) -> Option<Value> {
    let (max_w, max_h) = graphic(ctx);
    match kind {
        Kind::Transmit => transmit(ctx, key, value),
        Kind::TransmitPut => transmit(ctx, key, value).or_else(|| put(ctx, key, value)),
        Kind::Put => put(ctx, key, value),
        Kind::Delete => match key {
            // every selector of the specification but `a`, the default
            b'd' => letter(value, b"AiInNcCfFpPqQrRxXyYzZ").map(Value::Letter),
            b'x' | b'y' | b'p' => set(value).map(Value::Number),
            b'z' => set_signed(value).map(Value::Signed),
            _ => None,
        },
        // a frame takes RGB, RGBA and PNG, and refuses palette indices
        // (ADR-1100, D15)
        Kind::Frame => match key {
            b'f' => set(value)
                .filter(|f| matches!(f, 24 | 100))
                .map(Value::Number),
            b't' => letter(value, b"fts").map(Value::Letter),
            b'o' => letter(value, b"z").map(Value::Letter),
            b's' => set(value).filter(|&s| s <= max_w).map(Value::Number),
            b'v' => set(value).filter(|&v| v <= max_h).map(Value::Number),
            b'x' | b'y' | b'c' | b'r' | b'Y' => set(value).map(Value::Number),
            b'z' => set_signed(value).map(Value::Signed),
            b'X' | b'N' => exactly(value, 1),
            _ => None,
        },
        Kind::Control => match key {
            b's' => set(value).filter(|s| matches!(s, 1..=3)).map(Value::Number),
            b'r' | b'c' | b'v' => set(value).map(Value::Number),
            b'z' => set_signed(value).map(Value::Signed),
            _ => None,
        },
        Kind::Compose => match key {
            b'r' | b'c' | b'x' | b'y' | b'w' | b'h' | b'X' | b'Y' => set(value).map(Value::Number),
            b'C' => exactly(value, 1),
            _ => None,
        },
    }
}

/// Answers the feature query `cmd`, whose control keys are `keys` as the
/// program sent them.
pub(super) fn answer(ctx: &mut dyn Context, cmd: &Command, keys: &[u8]) {
    // the specification requires i on a feature query, as on any query
    if cmd.id == 0 {
        return;
    }
    // the answer is always OK, which q=1 suppresses
    if cmd.quiet >= 1 {
        return;
    }
    let mut text = String::from("\x1b_G");
    let _ = write!(text, "i={}", cmd.id);
    if let Some(kind) = kind(cmd.ask) {
        text.push_str(",e=");
        text.push(char::from(cmd.ask));
        let mut found = BTreeMap::new();
        for item in keys.split(|&b| b == b',') {
            let [key, b'=', value @ ..] = item else {
                continue;
            };
            // never reported, as the specification says
            if matches!(key, b'a' | b'e' | b'i' | b'I' | b'm' | b'q') {
                continue;
            }
            match supported(ctx, kind, *key, value) {
                Some(v) => {
                    found.insert(*key, v);
                }
                None => {
                    found.remove(key);
                }
            }
        }
        for (key, value) in found {
            text.push(',');
            text.push(char::from(key));
            text.push('=');
            let _ = match value {
                Value::Number(n) => write!(text, "{n}"),
                Value::Signed(n) => write!(text, "{n}"),
                Value::Letter(c) => write!(text, "{}", char::from(c)),
            };
        }
    }
    text.push_str(";OK\x1b\\");
    ctx.reply(ReplyKind::KittyGraphics, text.as_bytes());
}
