//! The host hook, through the replay's fixture host: each request is
//! printed among the replies as a `host:` line, and answered from the
//! case's fixtures.

use super::{BLUE3, GREEN, RED, chunks, kitty, rgb, rgba};
use crate::generate::{Case, DSR, Inputs, Made, cell, sized};

pub(in crate::generate) const CASES: &[Case] = &[
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
