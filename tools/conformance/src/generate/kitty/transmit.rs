//! Transmitting images: direct, chunked and aborted uploads, ids and
//! numbers, replacement, the replies q chooses, queries, and the commands
//! refused or cancelled.

use super::{BLUE3, GREEN, RED, chunks, kitty, kt, raw, rgb, rgba};
use crate::generate::sixel::{BLUE, six_at};
use crate::generate::{APC, Case, DSR, Inputs, Made, ST, cell, cup, sized};

pub(in crate::generate) const CASES: &[Case] = &[
    sized("kit-transmit", 12, 4, transmit),
    sized("kit-chunked", 12, 4, chunked),
    // a chunk of 131072 base64 characters, the size kitten icat sends, 32
    // times the protocol's "no larger than 4096 bytes": the payload is
    // streamed, so the chunk is accepted whole and the image is the one
    // sent. 160x206 pixels of f=24 are 131840 characters, the big chunk
    // (m=1) and a tail of 768 (m=0). FNV-1a over the pixels as the replay
    // reads them, A R G B with A=255, computed from ramp() alone: 5ce2ce65.
    sized("kit-chunk-large", 12, 4, |_| {
        [
            &cell(8, 16),
            &chunks("a=T,f=24,s=160,v=206,i=1,c=4,r=2", &ramp(160, 206), 131_072),
            DSR,
        ]
        .concat()
        .into()
    }),
    sized("kit-chunk-abort", 12, 4, chunk_abort),
    sized("kit-ids", 12, 4, ids),
    sized("kit-replace", 12, 4, replace),
    sized("kit-replace-refused", 12, 4, replace_refused),
    // q=0 answers OK and errors, q=1 errors only, q=2 nothing; a command
    // with neither i nor I gets no reply
    sized("kit-replies", 12, 4, |_| {
        [
            cell(8, 16),
            kt(1, 8, 8, RED, ",C=1"),
            kt(2, 8, 8, RED, ",q=1,C=1"),
            kt(3, 8, 8, RED, ",q=2,C=1"),
            kitty("a=p,i=7", b""),
            kitty("a=p,i=7,q=1", b""),
            kitty("a=p,i=7,q=2", b""),
            kitty("a=T,f=32,s=8,v=8", &rgba(8, 8, RED)),
        ]
        .concat()
        .into()
    }),
    // a=q loads and checks without storing or replacing: OK, then EINVAL
    // for data short of s x v, and a=p of its id finds nothing; an a=q with
    // the id of a stored image leaves that image
    sized("kit-query", 12, 4, |_| {
        [
            cell(8, 16),
            kitty("a=q,f=24,s=1,v=1,i=31", &rgb(1, 1, BLUE3)),
            kitty("a=q,f=24,s=2,v=1,i=32", &rgb(1, 1, BLUE3)),
            kitty("a=p,i=31", b""),
            kt(5, 8, 8, RED, ",C=1"),
            kitty("a=q,f=24,s=1,v=1,i=5", &rgb(1, 1, BLUE3)),
            cup(2, 1),
            kitty("a=p,i=5,C=1", b""),
        ]
        .concat()
        .into()
    }),
    sized("kit-query-number", 12, 4, query_number),
    sized("kit-refused", 12, 4, refused),
    // CAN and SUB abort a command: no image, no reply
    sized("kit-cancel", 12, 4, |_| {
        [
            &cell(8, 16),
            APC,
            "Ga=T,f=24,s=1,v=1,i=1;AAAA\x18",
            APC,
            "Ga=T,f=24,s=1,v=1,i=2;AAAA\x1a",
            &kitty("a=p,i=1", b""),
        ]
        .concat()
        .into()
    }),
];

/// A chunked transmission's first chunk, its ST included, and the rest.
fn split_first(chunks: &str) -> (String, &str) {
    match chunks.split_once(ST) {
        Some((first, rest)) => ([first, ST].concat(), rest),
        None => (chunks.to_owned(), ""),
    }
}

/// `w` x `h` pixels of f=24 whose color encodes their position, so a
/// lost, doubled or misplaced byte changes the image's digest.
fn ramp(w: usize, h: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            out.extend([x as u8, y as u8, (7 * x + 13 * y) as u8]);
        }
    }
    out
}

/// a=T with f=32 (half its pixels transparent) and f=24, each replying OK
/// for its id and moving the cursor past its box; an anonymous a=T, which
/// gets no reply; a=t stores without placing, and a=p places it.
fn transmit(_: &Inputs) -> Made {
    let data = [rgba(16, 8, RED), rgba(16, 8, [0, 255, 0, 0])].concat();
    [
        &cell(8, 16),
        &kitty("a=T,f=32,s=16,v=16,i=1", &data),
        DSR,
        &kitty("a=T,f=24,s=8,v=16,i=2", &rgb(8, 16, BLUE3)),
        DSR,
        &kitty("a=T,f=24,s=8,v=8", &rgb(8, 8, [255, 255, 0])),
        DSR,
        &kitty("a=t,f=32,s=8,v=8,i=3", &rgba(8, 8, [0, 0, 255, 128])),
        DSR,
        &cup(3, 1),
        &kitty("a=p,i=3", b""),
        DSR,
    ]
    .concat()
    .into()
}

/// Chunked direct transmission: the keys on the first chunk, m=1 until the
/// last; a chunk may change q. Text between chunks is not a graphics
/// command and leaves the upload pending; the placement is at the cursor
/// when the last chunk arrives.
fn chunked(_: &Inputs) -> Made {
    let first = chunks("a=T,f=24,s=8,v=16,i=1", &rgb(8, 16, BLUE3), 8);
    let (one, rest) = split_first(&first);
    [
        &cell(8, 16),
        &one,
        "ab",
        rest,
        DSR,
        &chunks("a=T,f=32,s=8,v=8,i=2,q=1", &rgba(8, 8, GREEN), 12),
        DSR,
    ]
    .concat()
    .into()
}

/// A pending upload is aborted by a delete, by any other graphics command,
/// and by CAN; its later chunks then arrive as commands of their own,
/// which are refused (no s and v) without a reply (no id).
fn chunk_abort(_: &Inputs) -> Made {
    let c = |i: u32| chunks(&format!("a=T,f=24,s=8,v=16,i={i}"), &rgb(8, 16, BLUE3), 8);
    let (c1, c2, c3) = (c(1), c(2), c(3));
    let (f1, r1) = split_first(&c1);
    let (f2, r2) = split_first(&c2);
    let (f3, r3) = split_first(&c3);
    [
        &cell(8, 16),
        &f1,
        &kitty("a=d", b""),
        r1,
        &f2,
        &kitty("a=p,i=9", b""),
        r2,
        &f3,
        APC,
        "Gm=1;AAAA\x18",
        r3,
        &kitty("a=p,i=1", b""),
        &kitty("a=p,i=2", b""),
        &kitty("a=p,i=3", b""),
        DSR,
    ]
    .concat()
    .into()
}

/// I: the terminal picks the lowest unused id, and a=p with I places the
/// newest image with that number; i with I is refused; a=p of an id never
/// sent is ENOENT.
fn ids(_: &Inputs) -> Made {
    [
        cell(8, 16),
        kitty("a=t,f=24,s=8,v=8,i=1", &rgb(8, 8, BLUE3)),
        kitty("a=t,f=24,s=8,v=8,I=7", &rgb(8, 8, [255, 0, 0])),
        kitty("a=t,f=24,s=8,v=8,I=7", &rgb(8, 8, [0, 255, 0])),
        kitty("a=p,I=7,p=3", b""),
        kitty("a=T,f=24,s=8,v=8,i=5,I=7", &rgb(8, 8, BLUE3)),
        kitty("a=p,i=4", b""),
        kitty("a=p,I=8", b""),
    ]
    .concat()
    .into()
}

/// A placement id names a placement to replace, which moves it: image 1
/// ends with two placements, one moved from column 2 to row 1, column 6.
/// Transmitting an id again replaces the image and deletes its placements:
/// image 2's two placements go, and the new image is not shown until
/// placed.
fn replace(_: &Inputs) -> Made {
    [
        &cell(8, 16),
        &kt(1, 16, 16, RED, ""),
        &kitty("a=p,i=1,p=5", b""),
        &cup(2, 7),
        &kitty("a=p,i=1,p=5", b""),
        DSR,
        &cup(3, 1),
        &kt(2, 8, 8, RED, ""),
        &kitty("a=p,i=2,C=1", b""),
        &kitty("a=t,f=24,s=8,v=8,i=2", &rgb(8, 8, BLUE3)),
    ]
    .concat()
    .into()
}

/// Under a tight limit, a replacement that does not fit beside a sixel is
/// refused with ENOSPC, and the old image and its placement stay: placed
/// again, it is still the red one. The limit holds two images and one
/// more placement.
fn replace_refused(_: &Inputs) -> Made {
    let cost = 16 * 16 * 4 + 512;
    [
        cell(8, 16),
        tt!("limit", b = 2 * cost + 256),
        kt(1, 16, 16, RED, ""),
        six_at(2, 0, 16, 16, BLUE),
        kitty(
            "a=t,f=32,s=16,v=32,i=1",
            &rgba(16, 32, [255, 255, 255, 255]),
        ),
        cup(3, 1),
        kitty("a=p,i=1", b""),
    ]
    .concat()
    .into()
}

/// An a=q without i, with I=52 or with neither, is dropped at its ST, as
/// kitty 0.49.2 drops it: no reply under q absent, 0, 1 or 2, on t=d, t=t
/// and t=s, whether its data would load or is too short, and a chunked
/// one's continuation is swallowed; a=p of number 52 then finds nothing
/// (ENOENT), and the same a=q with i=51 answers OK.
fn query_number(_: &Inputs) -> Made {
    let (tmp, shm) = (b"/tmp/tty-graphics-protocol-q", b"/kitty-q");
    let mut out = cell(8, 16);
    for ids in [",I=52", ""] {
        for q in ["", ",q=0", ",q=1", ",q=2"] {
            out += &kitty(&format!("a=q,f=24,s=1,v=1{ids}{q}"), &rgb(1, 1, BLUE3));
            out += &kitty(&format!("a=q,f=24,s=2,v=1{ids}{q}"), &rgb(1, 1, BLUE3));
            out += &kitty(&format!("a=q,t=t,f=24,s=1,v=1{ids}{q}"), tmp);
            out += &kitty(&format!("a=q,t=s,f=24,s=1,v=1{ids}{q}"), shm);
        }
        out += &raw(&format!("a=q,f=24,s=1,v=1,m=1{ids}"), "/w==");
        out += &raw("m=0", "AAA=");
    }
    out += &kitty("a=p,I=52", b"");
    out += &kitty("a=q,f=24,s=1,v=1,i=51", &rgb(1, 1, BLUE3));
    out.into()
}

/// EINVAL: animation, Unicode placeholders, relative placements, a raw
/// format without s and v, data short of s x v x bytes per pixel, a byte
/// outside base64, a key it cannot read, an unknown action; PNG and zlib
/// with no host hook; EBADF: files, temporary files and shared memory with
/// no host hook, and an empty name. Data past s x v x bytes per pixel
/// (i=4) is ignored, as kitty ignores it, an unknown key is ignored, and a
/// key section past 1 KiB ignores the whole APC. The fixture host is
/// removed first, so no command reaches a hook.
fn refused(_: &Inputs) -> Made {
    let px = rgb(1, 1, BLUE3);
    [
        cell(8, 16),
        tt!("host", on = 0),
        kitty("a=f,i=1,s=1,v=1", &px),
        kitty("a=a,i=1", b""),
        kitty("a=c,i=1", b""),
        kitty("a=T,U=1,f=24,s=1,v=1,i=2", &px),
        kitty("a=p,i=2,P=1,Q=1", b""),
        kitty("a=t,f=24,i=3", &px),
        kitty("a=t,f=24,s=1,v=1,i=4", &rgb(2, 1, BLUE3)),
        kitty("a=t,f=24,s=2,v=1,i=5", &px),
        raw("a=t,f=24,s=1,v=1,i=6", "AA*A"),
        kitty("a=t,f=24,s=x,v=1,i=7", &px),
        kitty("a=Z,i=8", b""),
        kitty("a=t,f=100,i=9", b"\x89PNG"),
        kitty("a=t,f=24,s=1,v=1,o=z,i=10", b"x\x9c"),
        kitty("a=t,t=f,f=100,i=11", b"/tmp/picture.png"),
        kitty("a=t,t=t,f=24,s=1,v=1,i=12", b"/tmp/tty-graphics-protocol-1"),
        kitty("a=t,t=s,s=1,v=1,i=13", b"/kitty-shm"),
        kitty("a=t,t=f,f=100,i=14", b""),
        kitty("a=T,f=24,s=1,v=1,i=15,K=9", &px),
        kitty(
            &format!("a=T,f=24,s=1,v=1,i=16,{}", "N=0,".repeat(260)),
            &px,
        ),
    ]
    .concat()
    .into()
}
