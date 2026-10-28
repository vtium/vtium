//! The kitty graphics protocol, its commands written from the protocol's
//! published description. Each case asks for the cursor where the cursor
//! matters, so its replies record where it went beside the replies to the
//! commands. Cells are 8x16.
//!
//! The `kit-host-` cases go through the replay's fixture host: each
//! request is printed among the replies as a `host:` line, and answered
//! from the case's fixtures.

use super::sixel::{BLUE, six_at};
use super::{APC, CSI, Case, DSR, ESC, Inputs, Made, ST, base64, cell, cup, sized};

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

/// Images 1, 2 and 3 placed at columns 0, 4 and 8, with placement ids 1;
/// `lower` should remove image 1's placement and `upper` image 2's, and
/// placing each again shows which kept its data (OK) and which lost it
/// (ENOENT); image 3 stays.
fn delete_case(lower: &str, upper: &str) -> String {
    [
        &cell(8, 16),
        &kitty("a=T,f=32,s=16,v=16,i=1,p=1,z=1,C=1", &rgba(16, 16, RED)),
        &cup(1, 5),
        &kitty("a=T,f=32,s=16,v=16,i=2,p=1,z=2,C=1", &rgba(16, 16, GREEN)),
        &cup(1, 9),
        &kitty("a=T,f=32,s=16,v=16,i=3,p=1,z=3,C=1", &rgba(16, 16, BLUE4)),
        lower,
        upper,
        &cup(4, 1),
        &kitty("a=p,i=1,C=1", b""),
        &cup(4, 5),
        &kitty("a=p,i=2,C=1", b""),
    ]
    .concat()
}

/// A `.rgba` fixture: width and height, little-endian, then the pixels.
fn rgba_fixture(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
    [
        w.to_le_bytes().as_slice(),
        &h.to_le_bytes(),
        &rgba(w as usize, h as usize, px),
    ]
    .concat()
}

/// 47 distinct bytes, byte i being (37 i + 11) mod 256, as a probe of
/// kitty's length rules held on each medium.
fn pix47() -> Vec<u8> {
    (0..47_u32).map(|i| ((37 * i + 11) % 256) as u8).collect()
}

/// What the fixture host answers: an outcome named in a `.error` fixture.
fn error(outcome: &str) -> Vec<u8> {
    format!("{outcome}\n").into_bytes()
}

const OUTCOMES: [&str; 5] = [
    "UNREADABLE",
    "TOO_SHORT",
    "CORRUPT",
    "TOO_LARGE",
    "OVER_BUDGET",
];

pub(super) const CASES: &[Case] = &[
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
    sized("kit-place-size", 24, 6, place_size),
    // z chooses the band: below cell backgrounds, below text, above it; at
    // one z the lower id draws first
    sized("kit-bands", 12, 4, |_| {
        [
            cell(8, 16),
            kt(3, 8, 8, RED, ",z=-1073741825,C=1"),
            kt(2, 8, 8, RED, ",z=-1,C=1"),
            kt(5, 8, 8, RED, ",z=4,C=1"),
            kt(9, 8, 8, GREEN, ",C=1"),
            kt(4, 8, 8, BLUE4, ",C=1"),
        ]
        .concat()
        .into()
    }),
    // C=1 leaves the cursor; otherwise it moves past the box: three rows
    // from the last row scroll the screen twice; a box reaching the right
    // edge sends the cursor to column 0 of the next row; inside a region
    // the scroll is the region's
    sized("kit-cursor", 12, 4, |_| {
        [
            &cell(8, 16),
            &kt(1, 8, 32, RED, ",C=1"),
            DSR,
            &cup(4, 1),
            &kt(2, 8, 48, RED, ""),
            DSR,
            &cup(1, 11),
            &kt(3, 16, 16, RED, ""),
            DSR,
            CSI,
            "2;3r",
            &cup(3, 5),
            &kt(4, 8, 32, RED, ""),
            DSR,
            CSI,
            "r",
        ]
        .concat()
        .into()
    }),
    // a and A: every placement on the screen; A frees what it leaves
    // unplaced; image 3 is placed again before A so A has something to do
    sized("kit-delete-a", 12, 4, |_| {
        delete_case(
            &kitty("a=d,d=a", b""),
            &[kitty("a=p,i=2,C=1", b""), kitty("a=d,d=A", b"")].concat(),
        )
        .into()
    }),
    sized("kit-delete-i", 12, 4, delete_i),
    sized("kit-delete-n", 12, 4, delete_n),
    // c and C: the placements under the cursor
    sized("kit-delete-c", 12, 4, |_| {
        delete_case(
            &[cup(1, 2), kitty("a=d,d=c", b"")].concat(),
            &[cup(1, 6), kitty("a=d,d=C", b"")].concat(),
        )
        .into()
    }),
    // p and P: the placements over cell x, y (1-based)
    sized("kit-delete-p", 12, 4, |_| {
        delete_case(
            &kitty("a=d,d=p,x=2,y=1", b""),
            &kitty("a=d,d=P,x=5,y=1", b""),
        )
        .into()
    }),
    // q and Q: over a cell and with a z; a wrong z deletes nothing
    sized("kit-delete-q", 12, 4, |_| {
        delete_case(
            &[
                kitty("a=d,d=q,x=1,y=1,z=1", b""),
                kitty("a=d,d=Q,x=9,y=1,z=1", b""),
            ]
            .concat(),
            &kitty("a=d,d=Q,x=6,y=1,z=2", b""),
        )
        .into()
    }),
    // r and R: images whose id lies in x to y; an inverted range deletes
    // nothing
    sized("kit-delete-r", 12, 4, |_| {
        delete_case(
            &[kitty("a=d,d=r,x=1,y=1", b""), kitty("a=d,d=R,x=3,y=1", b"")].concat(),
            &kitty("a=d,d=R,x=2,y=2", b""),
        )
        .into()
    }),
    // x and X: the placements over a column (1-based)
    sized("kit-delete-x", 12, 4, |_| {
        delete_case(&kitty("a=d,d=x,x=2", b""), &kitty("a=d,d=X,x=5", b"")).into()
    }),
    // y and Y: the placements over a row (1-based); image 1 is moved to
    // row 3 and image 2 to row 4 first, its placement id replacing them
    sized("kit-delete-y", 12, 4, |_| {
        delete_case(
            &[
                cup(3, 1),
                kitty("a=p,i=1,p=1,C=1", b""),
                kitty("a=d,d=y,y=3", b""),
            ]
            .concat(),
            &[
                cup(4, 5),
                kitty("a=p,i=2,p=1,C=1", b""),
                kitty("a=d,d=Y,y=4", b""),
            ]
            .concat(),
        )
        .into()
    }),
    // z and Z: the placements with a z
    sized("kit-delete-z", 12, 4, |_| {
        delete_case(&kitty("a=d,d=z,z=1", b""), &kitty("a=d,d=Z,z=2", b"")).into()
    }),
    // f and F: animation frames, which do not exist yet: nothing goes
    sized("kit-delete-f", 12, 4, |_| {
        delete_case(&kitty("a=d,d=f,i=1", b""), &kitty("a=d,d=F,i=2", b"")).into()
    }),
    sized("kit-delete-sb", 12, 4, delete_sb),
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
    sized("kit-evict", 12, 4, evict),
    sized("kit-evict-stamp", 12, 4, evict_stamp),
    // ED 2 removes kitty placements and keeps their images, so a=p shows
    // one again; the alternate screen's images go when it is left; RIS
    // empties both stores
    sized("kit-ed2", 12, 4, |_| {
        [
            &cell(8, 16),
            &kt(1, 8, 16, RED, ""),
            CSI,
            "2J",
            &kitty("a=p,i=1,C=1", b""),
            CSI,
            "?1049h",
            &kt(2, 8, 8, RED, ""),
            CSI,
            "?1049l",
            &kitty("a=p,i=2", b""),
            &cup(3, 1),
            &kt(3, 8, 8, RED, ""),
            ESC,
            "c",
            &kitty("a=p,i=3", b""),
            &kitty("a=p,i=1", b""),
        ]
        .concat()
        .into()
    }),
    sized("kit-host-media", 12, 4, host_media),
    // a name past 4095 bytes, or holding NUL, answers EBADF with no
    // request; a name with a space, a backslash and a newline reaches the
    // host as received, its host line escaping them
    sized("kit-host-names", 12, 4, |_| {
        [
            cell(8, 16),
            kitty(
                "a=t,t=f,f=100,i=1",
                &[b"/".as_slice(), &[b'a'; 4095]].concat(),
            ),
            kitty("a=t,t=f,f=100,i=2", b"/tmp/a\x00b"),
            kitty("a=t,t=f,f=100,i=3", b"/tmp/a b\\c\nd"),
            kitty(
                "a=t,t=f,f=100,i=4",
                &[b"/".as_slice(), &[b'a'; 4094]].concat(),
            ),
        ]
        .concat()
        .into()
    }),
    // mpv 0.41.0's shared memory frames: each whole t=s command carries m=1
    // and no m=0 follows, and each is taken whole, a request of its own;
    // the name has no leading /, which the image helper refuses
    // (UNREADABLE, EBADF), as the fixture does here, and q=2 keeps it quiet
    sized("kit-host-mpv", 12, 4, |i| {
        Made::with_files(
            [&cell(8, 16), &i.latin1("recorded/mpv-shm-frames.bin"), DSR].concat(),
            &[("mpv-kitty-0x5645ad83d520.error", error("UNREADABLE"))],
        )
    }),
    // kitten icat's detection: a=q direct, a=q with t=t and a=q with t=s,
    // then DA1; the three OK replies come before DA1's. icat's t=t and t=s
    // queries hold 3 bytes and give S as the length of their name (47 and
    // 19), which as kitty's maximum reads to the end, so each query loads.
    sized("kit-host-icat", 12, 4, |i| {
        Made::with_files(
            cell(8, 16) + &i.latin1("recorded/icat-detect.bin"),
            &[
                ("kitty-tty-graphics-protocol-2526776889", rgb(1, 1, BLUE3)),
                ("icat-KMIKYT5GUOEHE", rgb(1, 1, BLUE3)),
            ],
        )
    }),
    // kitten icat's GIF after its first frame: a=a, answered EINVAL for its
    // I (no q); a=f with t=s, refused, and handed to the host as a discard
    // (discard=1) so its object is removed, q=2 keeping the refusal quiet
    sized("kit-host-icat-gif", 12, 4, |i| {
        (cell(8, 16) + &i.latin1("recorded/icat-gif-frames.bin")).into()
    }),
    // the pause: a put right after a file transmission, and text and DSR
    // after it, are parsed only after the host's reply, in order: the put
    // finds the image, its OK follows the transmission's, and the cursor
    // report comes last
    sized("kit-host-pause", 12, 4, |_| {
        Made::with_files(
            [
                &cell(8, 16),
                &kitty("a=t,t=f,f=100,i=1", b"/tmp/pic"),
                &kitty("a=p,i=1", b""),
                "ab",
                DSR,
            ]
            .concat(),
            &[("pic.rgba", rgba_fixture(8, 16, RED))],
        )
    }),
    sized("kit-host-errors", 12, 4, host_errors),
    sized("kit-host-requests", 12, 4, host_requests),
    sized("kit-host-outcomes", 12, 4, host_outcomes),
    sized("kit-host-lengths", 12, 4, host_lengths),
];

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

/// c and r together letterbox or pillarbox the picture; one alone keeps
/// the aspect; x, y, w, h choose the source rectangle, cut to the image;
/// X, Y offset it in its first cell, and an offset outside the cell is
/// refused.
fn place_size(_: &Inputs) -> Made {
    [
        &cell(8, 16),
        &kitty("a=t,f=32,s=32,v=16,i=1", &rgba(32, 16, RED)),
        &kitty("a=p,i=1,c=2,r=2,C=1", b""),
        &cup(1, 4),
        &kitty("a=p,i=1,c=3,C=1", b""),
        &cup(1, 8),
        &kitty("a=p,i=1,r=2,C=1", b""),
        &cup(1, 14),
        &kitty("a=p,i=1,x=8,y=4,w=16,h=100,C=1", b""),
        &cup(4, 1),
        &kitty("a=p,i=1,X=3,Y=5,C=1", b""),
        &cup(4, 8),
        &kitty("a=p,i=1,X=8,C=1", b""),
        &kitty("a=p,i=1,x=32", b""),
        DSR,
    ]
    .concat()
    .into()
}

/// i and I, by id, and with p only that placement; an uppercase delete
/// whose p matches none of an image's placements frees nothing, even an
/// image with no placement at all (image 4, placed again at the end).
fn delete_i(_: &Inputs) -> Made {
    let lower = [
        kitty("a=t,f=32,s=8,v=8,i=4", &rgba(8, 8, [255, 0, 255, 255])),
        kitty("a=d,d=i,i=1", b""),
    ]
    .concat();
    let upper = [
        kitty("a=d,d=I,i=2,p=1", b""),
        kitty("a=d,d=I,i=3,p=2", b""),
        kitty("a=d,d=I,i=4,p=9", b""),
    ]
    .concat();
    (delete_case(&lower, &upper) + &cup(4, 9) + &kitty("a=p,i=4,C=1", b"")).into()
}

/// n and N: the newest image with the number.
fn delete_n(_: &Inputs) -> Made {
    [
        cell(8, 16),
        kitty("a=T,f=32,s=16,v=16,I=4,C=1", &rgba(16, 16, RED)),
        cup(1, 5),
        kitty("a=T,f=32,s=16,v=16,I=4,C=1", &rgba(16, 16, GREEN)),
        cup(1, 9),
        kitty("a=T,f=32,s=16,v=16,I=6,C=1", &rgba(16, 16, BLUE4)),
        kitty("a=d,d=n,I=4", b""),
        kitty("a=d,d=N,I=6", b""),
        cup(4, 1),
        kitty("a=p,i=2,C=1", b""),
        cup(4, 5),
        kitty("a=p,i=3,C=1", b""),
    ]
    .concat()
    .into()
}

/// a deletes only on the screen and I in scrollback too: images 1 and 3
/// scroll into scrollback, and the view shows that a left image 1 and I
/// took image 3; neither touches a sixel.
fn delete_sb(_: &Inputs) -> Made {
    [
        &cell(8, 16),
        &tt!("sb-max", n = 10),
        &kt(1, 8, 16, RED, ",C=1"),
        &cup(1, 4),
        &kt(3, 8, 16, RED, ",C=1"),
        &cup(4, 1),
        "\n",
        &kt(2, 8, 16, RED, ",C=1"),
        &six_at(4, 2, 8, 16, BLUE),
        &kitty("a=d,d=a", b""),
        &kitty("a=d,d=I,i=3", b""),
        &tt!("sb-up", n = 1),
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

/// Past the limit, images go unplaced first, then placed only in
/// scrollback, then kitty images placed on the screen, oldest transmission
/// first; a sixel on the screen is never evicted, so a new image that does
/// not fit beside it is refused with ENOSPC.
fn evict(_: &Inputs) -> Made {
    let cost = 8 * 16 * 4 + 512;
    let mut out = [
        cell(8, 16),
        tt!("sb-max", n = 10),
        tt!("limit", b = 4 * cost),
        kt(1, 8, 16, RED, ""),
        "\r\n\n\n\n".to_owned(),
        six_at(2, 0, 8, 16, BLUE),
        kt(2, 8, 16, RED, ",C=1"),
        cup(1, 7),
        kt(3, 8, 16, RED, ",C=1"),
    ]
    .concat();
    for i in 4..9 {
        out += &kitty(&format!("a=t,f=32,s=8,v=16,i={i}"), &rgba(8, 16, RED));
    }
    out += &kitty("a=t,f=32,s=8,v=32,i=9", &rgba(8, 32, RED));
    out += &kitty("a=t,f=32,s=8,v=96,i=10", &rgba(8, 96, RED));
    (out + &tt!("sb-up", n = 4)).into()
}

/// Kitty images on the screen are evicted oldest transmission first: an
/// old image placed again with a=p keeps its old stamp and goes before a
/// newer one placed earlier. The limit holds two images and image 1's
/// second placement.
fn evict_stamp(_: &Inputs) -> Made {
    let cost = 8 * 16 * 4 + 512;
    [
        cell(8, 16),
        tt!("limit", b = 2 * cost + 256),
        kt(1, 8, 16, RED, ",C=1"),
        cup(1, 4),
        kt(2, 8, 16, RED, ",C=1"),
        cup(3, 1),
        kitty("a=p,i=1,C=1", b""),
        cup(1, 8),
        kt(3, 8, 16, RED, ",C=1"),
        kitty("a=p,i=1", b""),
        kitty("a=p,i=2", b""),
    ]
    .concat()
    .into()
}

/// Each medium reaches the host: t=f with raw f=24 read from the fixture,
/// t=t with f=100 (a PNG the .rgba stands in for), t=s with f=32, O and
/// S, then inline PNG and inline zlib data, buffered and handed over whole
/// (the inflated bytes from .bytes); inline raw data stays in the terminal
/// (no host line).
fn host_media(_: &Inputs) -> Made {
    let png = [
        b"\x89PNG\r\n\x1a\n".as_slice(),
        &(0..40).collect::<Vec<u8>>(),
    ]
    .concat();
    Made::with_files(
        [
            &cell(8, 16),
            &kitty("a=T,t=f,f=24,s=8,v=16,i=1", b"/srv/img/picture.rgb"),
            &kitty("a=T,t=t,f=100,i=2", b"/tmp/tty-graphics-protocol-pic.png"),
            &kitty("a=T,t=s,f=32,s=8,v=8,O=4,S=256,i=3", b"/frame"),
            &chunks("a=T,f=100,i=4", &png, 12),
            &kitty(
                "a=T,f=32,s=8,v=8,o=z,i=5",
                &[b"x\x9c".as_slice(), &[0; 30]].concat(),
            ),
            &kitty("a=T,f=24,s=8,v=8,i=6", &rgb(8, 8, BLUE3)),
            DSR,
        ]
        .concat(),
        &[
            ("picture.rgb", rgb(8, 16, BLUE3)),
            (
                "tty-graphics-protocol-pic.png.rgba",
                rgba_fixture(8, 8, GREEN),
            ),
            (
                "frame",
                [b"skip".as_slice(), &rgba(8, 8, [255, 255, 0, 255]), b"tail"].concat(),
            ),
            ("direct-1.rgba", rgba_fixture(8, 8, [255, 0, 255, 255])),
            ("direct-2.bytes", rgba(8, 8, [0, 255, 255, 255])),
        ],
    )
}

/// The host's outcomes from .error fixtures for t=f PNG (UNREADABLE EBADF,
/// CORRUPT EINVAL, OVER_BUDGET ENOSPC, TOO_LARGE EINVAL); a missing fixture
/// (UNREADABLE, EBADF); a=q through the host storing nothing; a reply past
/// max-graphic-size (EINVAL); a raw f=32 file too short for s x v
/// (TOO_SHORT, EBADF); ENOSPC from the reply's reservation, and from the
/// inline buffer's.
fn host_errors(_: &Inputs) -> Made {
    Made::with_files(
        [
            cell(8, 16),
            kitty("a=t,t=f,f=100,i=1", b"/e1"),
            kitty("a=t,t=f,f=100,i=2", b"/e2"),
            kitty("a=t,t=f,f=100,i=3", b"/e3"),
            kitty("a=t,t=f,f=100,i=10", b"/e4"),
            kitty("a=t,t=f,f=100,i=4", b"/none"),
            kitty("a=q,t=f,f=100,i=5", b"/q"),
            kitty("a=p,i=5", b""),
            tt!("graphic-max", w = 32, h = 32),
            kitty("a=t,t=f,f=100,i=6", b"/big"),
            tt!("graphic-max", w = 0, h = 0),
            kitty("a=t,t=f,f=32,s=2,v=2,i=7", b"/short"),
            tt!("limit", b = 8192),
            kitty("a=t,t=f,f=100,i=8", b"/huge"),
            kitty("a=t,f=100,i=9", &[0; 9000]),
        ]
        .concat(),
        &[
            ("e1.error", error("UNREADABLE")),
            ("e2.error", error("CORRUPT")),
            ("e3.error", error("OVER_BUDGET")),
            ("e4.error", error("TOO_LARGE")),
            ("q.rgba", rgba_fixture(1, 1, RED)),
            ("big.rgba", rgba_fixture(40, 1, RED)),
            ("short", rgba(3, 1, RED)),
            ("huge.rgba", rgba_fixture(64, 64, RED)),
        ],
    )
}

/// The request each kind of transmission produces, its host line judged:
/// t=f raw (O and S as offset and read_max, s x v x bpp as min); t=t raw
/// with o=z (inflate); t=s PNG (png, min 0); t=f PNG with o=z and S
/// (inflate,png; S a read bound, no exact); inline raw with o=z (inflate,
/// min s x v x bpp); inline PNG, S ignored; inline PNG with o=z and S=30
/// (inflate,png,exact, min 30); without S and with S=0, no exact; a
/// refused t=t and a refused t=s handed over as discards (EINVAL); a
/// refused t=f and a refused inline PNG never reaching the host (EINVAL).
/// No fixture exists: the file media answer UNREADABLE (EBADF) and inline
/// data CORRUPT (EINVAL).
fn host_requests(_: &Inputs) -> Made {
    [
        cell(8, 16),
        kitty("a=t,t=f,f=24,s=2,v=1,O=5,S=9,i=1", b"/r1"),
        kitty(
            "a=t,t=t,f=32,s=2,v=2,o=z,O=1,S=100,i=2",
            b"/tmp/tty-graphics-protocol-r2",
        ),
        kitty("a=t,t=s,f=100,O=2,S=50,i=3", b"/r3"),
        kitty("a=t,t=f,f=100,o=z,S=40,i=4", b"/r4"),
        kitty("a=t,f=24,s=2,v=2,o=z,i=5", b"x\x9c"),
        kitty("a=t,f=100,S=99,i=6", b"\x89PNG"),
        kitty("a=t,f=100,o=z,S=30,i=7", b"x\x9c"),
        kitty("a=t,f=100,o=z,i=8", b"x\x9c"),
        kitty("a=t,f=100,o=z,S=0,i=9", b"x\x9c"),
        kitty("a=t,t=t,f=24,i=10", b"/tmp/tty-graphics-protocol-r10"),
        kitty("a=f,t=s,s=1,v=1,i=11", b"/r11"),
        kitty("a=t,t=f,f=24,i=12", b"/r12"),
        kitty("a=t,f=100,i=13,I=1", b"\x89PNG"),
    ]
    .concat()
    .into()
}

/// The reply each outcome produces, in its three columns: for t=f raw
/// without o=z (i=1 to 5) UNREADABLE and TOO_SHORT are EBADF; with o=z
/// (i=11 to 15) and inline PNG (i=21 to 25) TOO_SHORT is EINVAL; in all
/// three CORRUPT and TOO_LARGE are EINVAL and OVER_BUDGET ENOSPC. t=f PNG's
/// TOO_SHORT (i=6) is EBADF, no inflate. Then OK in each: raw from the file
/// (i=7), inflated from .bytes (i=16), inline PNG from .rgba (i=26), each
/// stored.
fn host_outcomes(_: &Inputs) -> Made {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut out = cell(8, 16);
    for (n, name) in (1..).zip(OUTCOMES) {
        for prefix in ["f", "z", "direct-"] {
            files.push((format!("{prefix}{n}.error"), error(name)));
        }
        out += &kitty(
            &format!("a=t,t=f,f=24,s=1,v=1,i={n}"),
            format!("/f{n}").as_bytes(),
        );
    }
    for n in 1..=OUTCOMES.len() {
        out += &kitty(
            &format!("a=t,t=f,f=24,s=1,v=1,o=z,i={}", n + 10),
            format!("/z{n}").as_bytes(),
        );
    }
    for n in 1..=OUTCOMES.len() {
        out += &kitty(&format!("a=t,f=100,i={}", n + 20), b"\x89PNG");
    }
    files.push(("p6.error".into(), error("TOO_SHORT")));
    files.push(("f7".into(), rgb(1, 1, BLUE3)));
    files.push(("z16.bytes".into(), rgb(1, 1, [0, 255, 0])));
    files.push((
        "direct-6.rgba".into(),
        rgba_fixture(1, 1, [255, 255, 0, 255]),
    ));
    out += &[
        kitty("a=t,t=f,f=100,i=6", b"/p6"),
        kitty("a=T,t=f,f=24,s=1,v=1,C=1,i=7", b"/f7"),
        kitty("a=T,t=f,f=24,s=1,v=1,o=z,C=1,i=16", b"/z16"),
        kitty("a=T,f=100,C=1,i=26", b"\x89PNG"),
    ]
    .concat();
    let files: Vec<(&str, Vec<u8>)> = files
        .iter()
        .map(|(name, data)| (name.as_str(), data.clone()))
        .collect();
    Made::with_files(out, &files)
}

/// kitty's length rules, applied by the fixture host as the image helper
/// applies them: S past the end of a 3-byte file reads to the end (OK,
/// i=1); no S reads to the end and takes the first 3 bytes, the excess
/// ignored (OK, i=2, color of bytes 0 to 2); O=10 with S=3 and O=10 alone
/// take bytes 10 to 12 (OK, i=3 and 4); S=2 for 3 bytes, and S=3 for a
/// 2x1 that needs 6, too few (TOO_SHORT, EBADF, i=5 and 6); an offset at
/// the end and one past it (UNREADABLE, EBADF, i=7 and 8); the same on
/// t=t (OK, i=9); inline, too few (EINVAL, i=10) and past s x v (OK, the
/// excess ignored, i=11).
fn host_lengths(_: &Inputs) -> Made {
    let pix = pix47();
    Made::with_files(
        [
            cell(8, 16),
            kitty("a=T,t=f,f=24,s=1,v=1,S=47,C=1,i=1", b"/pix3"),
            kitty("a=T,t=f,f=24,s=1,v=1,C=1,i=2", b"/pix47"),
            kitty("a=T,t=f,f=24,s=1,v=1,O=10,S=3,C=1,i=3", b"/pix47"),
            kitty("a=T,t=f,f=24,s=1,v=1,O=10,C=1,i=4", b"/pix47"),
            kitty("a=T,t=f,f=24,s=1,v=1,S=2,C=1,i=5", b"/pix47"),
            kitty("a=T,t=f,f=24,s=2,v=1,S=3,C=1,i=6", b"/pix47"),
            kitty("a=T,t=f,f=24,s=1,v=1,O=47,C=1,i=7", b"/pix47"),
            kitty("a=T,t=f,f=24,s=1,v=1,O=50,C=1,i=8", b"/pix3"),
            kitty(
                "a=T,t=t,f=24,s=1,v=1,S=47,C=1,i=9",
                b"/tmp/tty-graphics-protocol-pix47",
            ),
            kitty("a=T,f=24,s=1,v=1,C=1,i=10", &pix[..2]),
            kitty("a=T,f=24,s=1,v=1,C=1,i=11", &pix[..6]),
        ]
        .concat(),
        &[
            ("pix3", pix[..3].to_vec()),
            ("pix47", pix.clone()),
            ("tty-graphics-protocol-pix47", pix.clone()),
        ],
    )
}
