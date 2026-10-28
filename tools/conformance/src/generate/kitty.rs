//! The kitty graphics protocol, its commands written from the protocol's
//! published description. Each case asks for the cursor where the cursor
//! matters, so its replies record where it went beside the replies to the
//! commands. Cells are 8x16.

use super::{APC, Case, ST, base64};

mod delete;
mod host;
mod place;
mod transmit;

pub(super) const GROUPS: [&[Case]; 4] = [transmit::CASES, place::CASES, delete::CASES, host::CASES];

/// `ESC _ G <keys> ; <base64 of data> ST`, with no `;` when there is no
/// data.
pub(super) fn kitty(keys: &str, data: &[u8]) -> String {
    raw(keys, &base64(data))
}

/// A graphics command whose payload is `payload` as it is.
fn raw(keys: &str, payload: &str) -> String {
    let sep = if payload.is_empty() { "" } else { ";" };
    [APC, "G", keys, sep, payload, ST].concat()
}

/// `w` x `h` pixels of one straight-alpha RGBA color, f=32.
fn rgba(w: usize, h: usize, px: [u8; 4]) -> Vec<u8> {
    px.repeat(w * h)
}

/// `w` x `h` pixels of one RGB color, f=24.
fn rgb(w: usize, h: usize, px: [u8; 3]) -> Vec<u8> {
    px.repeat(w * h)
}

const RED: [u8; 4] = [255, 0, 0, 255];

const GREEN: [u8; 4] = [0, 255, 0, 255];

const BLUE4: [u8; 4] = [0, 0, 255, 255];

const BLUE3: [u8; 3] = [0, 0, 255];

/// The base64 of `data` cut into chunks of `size` characters (a multiple
/// of 4), m=1 on all but the last, the keys on the first only.
fn chunks(keys: &str, data: &[u8], size: usize) -> String {
    let enc = base64(data);
    let parts: Vec<&str> = enc
        .as_bytes()
        .chunks(size)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect();
    let mut out = String::new();
    for (n, part) in parts.iter().enumerate() {
        let more = if n + 1 == parts.len() { "m=0" } else { "m=1" };
        let head = if n == 0 {
            format!("{keys},{more}")
        } else {
            more.to_owned()
        };
        out += &raw(&head, part);
    }
    out
}

/// Transmits and places image `i`, f=32, at the cursor.
fn kt(i: u32, w: usize, h: usize, px: [u8; 4], keys: &str) -> String {
    kitty(
        &format!("a=T,f=32,s={w},v={h},i={i}{keys}"),
        &rgba(w, h, px),
    )
}
