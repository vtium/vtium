//! Seeded random and adversarial byte streams. For each stream: no panic,
//! every bound the recorder checks holds at every call, and the stream cut
//! at any point and fed in two pieces gives the same actions and leaves the
//! parser in the same state as fed whole, as does the stream fed a byte at
//! a time or with pauses.
//!
//! This is vtium-parse's chunking-invariance gate, the parser's part of the
//! "Chunking invariance" rule in docs/architecture.md, "Determinism"
//! (vtium/vtium#50). A failure names the stream it failed on.

mod common;

use common::{Act, Recorder};
use std::collections::BTreeSet;
use vtium_parse::{InputMode, OSC_MAX, PARAMS_MAX, Parser};

/// splitmix64: a seeded generator with no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Pieces of sequences, controls, and well- and ill-formed UTF-8.
const TOKENS: &[&[u8]] = &[
    b"\x1b",
    b"[",
    b"]",
    b"P",
    b"_",
    b"X",
    b"^",
    b"\\",
    b"\x07",
    b";",
    b":",
    b"?",
    b">",
    b"<",
    b"=",
    b"!",
    b"$",
    b" ",
    b"\"",
    b"+",
    b"#",
    b"(",
    b"%",
    b"q",
    b"m",
    b"p",
    b"h",
    b"c",
    b"G",
    b"0",
    b"1",
    b"4",
    b"9",
    b"38:2::255:0:0",
    b"65535",
    b"99999999",
    b"a=T,f=32;",
    b"AAAA",
    b"\x18",
    b"\x1a",
    b"\x7f",
    b"\r",
    b"\n",
    b"\x00",
    b"\x9b",
    b"\x9c",
    b"\x9d",
    b"\x90",
    b"\x9f",
    b"\x98",
    b"\x85",
    b"\xc2\x9b",
    b"\xc2\x9c",
    b"\xc2\x9d",
    b"\xc2\x90",
    b"\xc2\x9f",
    b"\xc2\x98",
    b"\xc3\xa9",
    b"\xe2\x82\xac",
    b"\xe2\x82",
    b"\xf0\x9f\x98\x80",
    b"\xf0\x9f",
    b"\xc0\x9b",
    b"\xe0\x80\x9b",
    b"\xed\xa0\x80",
    b"\xef\xbf\xbf",
    b"\xf7\xbf\xbf\xbf",
    b"\xff",
    b"\x80",
    b"\x1bP+q",
    b"544e;436f",
    b"\x1b[62\"p",
    b"\x1b[61\"p",
    b"\x1b[!p",
    b"\x1bc",
    b"\x1b]",
    b"\x1b_G",
    b"\x1bPq",
    b"\x1b[?",
    b"text",
];

fn tokens(rng: &mut Rng, n: usize) -> Vec<u8> {
    let mut v = Vec::new();
    for _ in 0..n {
        v.extend_from_slice(rng.pick(TOKENS));
    }
    v
}

fn random_bytes(rng: &mut Rng, n: usize) -> Vec<u8> {
    (0..n).map(|_| rng.next() as u8).collect()
}

/// A sequence at or past one of the parser's bounds.
fn bound_stress(rng: &mut Rng) -> Vec<u8> {
    let mut v = Vec::new();
    match rng.below(5) {
        0 => {
            v.extend_from_slice(b"\x1b]");
            let pieces: [&[u8]; 4] = [b"a", "é".as_bytes(), "€".as_bytes(), "😀".as_bytes()];
            for _ in 0..100 + rng.below(100) {
                v.extend_from_slice(rng.pick(&pieces));
            }
            v.extend_from_slice(rng.pick(&[&b"\x07"[..], b"\x1b\\", b"\xc2\x9c", b"\x18"]));
        }
        1 => {
            v.extend_from_slice(rng.pick(&[&b"\x1b["[..], b"\x1bP"]));
            for _ in 0..rng.below(40) {
                v.extend_from_slice(rng.pick(&[
                    &b"1"[..],
                    b"23",
                    b"999999",
                    b";",
                    b";",
                    b":",
                    b":5",
                ]));
            }
            v.extend_from_slice(rng.pick(&[&b"m"[..], b"q", b"H"]));
            v.extend_from_slice(b"\x1b\\");
        }
        2 => {
            v.extend_from_slice(b"\x1bP+q");
            for _ in 0..1030 + rng.below(20) {
                v.push(*rng.pick(b"0123456789abcdef;"));
            }
            v.extend_from_slice(rng.pick(&[&b"\x1b\\"[..], b"\xc2\x9c", b"\x1bx", b"\x1a"]));
        }
        3 => {
            v.extend_from_slice(b"\x1b_Ga=T;");
            v.extend((0..500 + rng.below(500)).map(|_| *rng.pick(b"ABCD\r\xc3\xa9")));
            v.extend_from_slice(b"\x1b\\");
        }
        _ => {
            v.extend_from_slice(b"\x1b[");
            v.extend((0..rng.below(2000)).map(|_| *rng.pick(b"!$%' ")));
            v.push(b'p');
        }
    }
    v
}

/// The actions `pieces` give, and the parser's state, as its `Debug`
/// shows it, after the first piece and at the end.
fn run(
    mode: InputMode,
    pieces: &[&[u8]],
    pause_every: Option<usize>,
) -> (Vec<Act>, String, String) {
    let mut parser = Parser::new();
    let mut rec = Recorder {
        mode,
        follow_decscl: true,
        ..Recorder::new()
    };
    if let Some(n) = pause_every {
        // A predicate sees only the action, so pause on those whose debug
        // text has a length divisible by n: a varied, repeatable subset.
        rec.pause_on = Some(match n {
            2 => |a: &Act| format!("{a:?}").len() % 2 == 0,
            _ => |a: &Act| format!("{a:?}").len() % 3 == 0,
        });
    }
    let mut first = String::new();
    for (i, piece) in pieces.iter().enumerate() {
        rec.feed(&mut parser, piece);
        if i == 0 {
            first = format!("{parser:?}");
        }
    }
    (rec.acts, first, format!("{parser:?}"))
}

/// The actions and the end state of `pieces`.
fn run_to_end(mode: InputMode, pieces: &[&[u8]], pause_every: Option<usize>) -> (Vec<Act>, String) {
    let (acts, _, end) = run(mode, pieces, pause_every);
    (acts, end)
}

/// What the streams reached, so that a generator that stops reaching a
/// path or a bound fails the test.
#[derive(Default)]
struct Coverage {
    kinds: BTreeSet<&'static str>,
    osc_full: bool,
    params_full: bool,
    tcap_over: bool,
    /// Cuts inside a UTF-8 character.
    cuts_in_utf8: usize,
    /// Cuts inside an ESC, CSI or DCS header.
    cuts_in_header: usize,
    /// Cuts between the ESC and the `\` of a string's ST.
    cuts_in_st: usize,
}

impl Coverage {
    fn note(&mut self, acts: &[Act]) {
        for act in acts {
            let kind = match act {
                Act::Print(_) => "print",
                Act::Execute(_) => "execute",
                Act::Esc(..) => "esc",
                Act::Csi(p, ..) => {
                    self.params_full |= p.len() == PARAMS_MAX;
                    "csi"
                }
                Act::Osc(d, ..) => {
                    self.osc_full |= d.len() == OSC_MAX;
                    "osc"
                }
                Act::DcsHook(..) => "dcs_hook",
                Act::DcsByte(_) => "dcs_byte",
                Act::DcsChar(_) => "dcs_char",
                Act::DcsUnhook(_) => "dcs_unhook",
                Act::ApcStart => "apc_start",
                Act::ApcByte(_) => "apc_byte",
                Act::ApcChar(_) => "apc_char",
                Act::ApcEnd(_) => "apc_end",
                Act::Xtgettcap(payload) => {
                    self.tcap_over |= payload.is_none();
                    "xtgettcap"
                }
            };
            self.kinds.insert(kind);
        }
    }

    /// Counts a cut by where it fell: `state` is the parser's state after
    /// the bytes before it, and `after` the bytes after it.
    fn note_cut(&mut self, mode: InputMode, state: &str, after: &[u8]) {
        if mode == InputMode::Utf8 && !state.contains("need: 0,") {
            self.cuts_in_utf8 += 1;
        }
        const HEADERS: [&str; 7] = [
            "state: EscInt,",
            "state: CsiEntry,",
            "state: CsiParam,",
            "state: CsiInt,",
            "state: DcsEntry,",
            "state: DcsParam,",
            "state: DcsInt,",
        ];
        if HEADERS.iter().any(|h| state.contains(h)) {
            self.cuts_in_header += 1;
        }
        if state.contains("pending_end: Some(") && after.first() == Some(&b'\\') {
            self.cuts_in_st += 1;
        }
    }

    /// Fails unless the cuts fell inside a UTF-8 character, inside a
    /// header and inside an ST.
    fn assert_cuts(&self) {
        assert!(
            self.cuts_in_utf8 > 0,
            "no cut fell inside a UTF-8 character"
        );
        assert!(self.cuts_in_header > 0, "no cut fell inside a header");
        assert!(
            self.cuts_in_st > 0,
            "no cut fell between the ESC and the \\ of an ST"
        );
    }
}

const ALL_KINDS: [&str; 14] = [
    "apc_byte",
    "apc_char",
    "apc_end",
    "apc_start",
    "csi",
    "dcs_byte",
    "dcs_char",
    "dcs_hook",
    "dcs_unhook",
    "esc",
    "execute",
    "osc",
    "print",
    "xtgettcap",
];

fn check_stream(rng: &mut Rng, stream: &[u8], cov: &mut Coverage) {
    let mode = *rng.pick(&[
        InputMode::Utf8,
        InputMode::Utf8,
        InputMode::EightBit,
        InputMode::SevenBit,
    ]);
    let whole = run_to_end(mode, &[stream], None);
    cov.note(&whole.0);

    let cuts: Vec<usize> = if stream.len() <= 64 {
        (0..=stream.len()).collect()
    } else {
        let mut c: Vec<usize> = (0..16).map(|_| rng.below(stream.len() + 1)).collect();
        c.extend([0, 1, stream.len() - 1, stream.len()]);
        c
    };
    for cut in cuts {
        let (a, b) = stream.split_at(cut);
        let (acts, at_cut, end) = run(mode, &[a, b], None);
        cov.note_cut(mode, &at_cut, b);
        assert_eq!(
            (acts, end),
            whole,
            "cut at {cut} of {stream:02x?} in {mode:?}"
        );
    }

    if stream.len() <= 512 {
        let bytewise: Vec<&[u8]> = stream.chunks(1).collect();
        assert_eq!(
            run_to_end(mode, &bytewise, None),
            whole,
            "bytewise {stream:02x?} in {mode:?}"
        );
    }
    let every = *rng.pick(&[2, 3]);
    assert_eq!(
        run_to_end(mode, &[stream], Some(every)),
        whole,
        "paused {stream:02x?} in {mode:?}"
    );
}

#[test]
fn random_bytes_split_anywhere() {
    let mut rng = Rng(0x7674_6975_6d01);
    let mut cov = Coverage::default();
    for _ in 0..5000 {
        let n = rng.below(96);
        let stream = random_bytes(&mut rng, n);
        check_stream(&mut rng, &stream, &mut cov);
    }
    for kind in [
        "print",
        "execute",
        "esc",
        "csi",
        "osc",
        "dcs_hook",
        "apc_start",
    ] {
        assert!(
            cov.kinds.contains(kind),
            "random bytes never reached {kind}"
        );
    }
    // Random bytes rarely open a string and end it with ESC `\`, so only
    // the tokens below are held to cut inside an ST.
    assert!(cov.cuts_in_utf8 > 0, "no cut fell inside a UTF-8 character");
    assert!(cov.cuts_in_header > 0, "no cut fell inside a header");
}

#[test]
fn adversarial_tokens_split_anywhere() {
    let mut rng = Rng(0x7674_6975_6d02);
    let mut cov = Coverage::default();
    for _ in 0..10_000 {
        let n = rng.below(40);
        let stream = tokens(&mut rng, n);
        check_stream(&mut rng, &stream, &mut cov);
    }
    assert_eq!(cov.kinds, BTreeSet::from(ALL_KINDS));
    cov.assert_cuts();
}

#[test]
fn bound_stress_split_anywhere() {
    let mut rng = Rng(0x7674_6975_6d03);
    let mut cov = Coverage::default();
    for _ in 0..1000 {
        let mut stream = tokens(&mut rng, 4);
        stream.extend(bound_stress(&mut rng));
        stream.extend(tokens(&mut rng, 4));
        check_stream(&mut rng, &stream, &mut cov);
    }
    assert!(cov.osc_full, "no OSC string reached its bound");
    assert!(cov.params_full, "no CSI reached 16 parameters");
    assert!(cov.tcap_over, "no XTGETTCAP payload passed its bound");
    cov.assert_cuts();
}

// Cut at every byte, a header past each of the bounds that drop bytes: a
// ninth sub-parameter colon and a seventeenth parameter. Random streams
// seldom put a cut and a following digit there.
#[test]
fn params_past_their_bounds_split_at_every_byte() {
    let params: Vec<String> = (1..=18).map(|i| i.to_string()).collect();
    let streams = [
        b"\x1b[1:1:1:1:1:1:1:1:1:1:99;7m".to_vec(),
        format!("\x1b[{}m", params.join(";")).into_bytes(),
    ];
    let mut rng = Rng(0x7674_6975_6d04);
    let mut cov = Coverage::default();
    for stream in &streams {
        assert!(stream.len() <= 64, "a longer stream is cut at random");
        for _ in 0..8 {
            check_stream(&mut rng, stream, &mut cov);
        }
    }
    assert!(cov.params_full, "no CSI reached 16 parameters");
    assert!(cov.cuts_in_header > 0, "no cut fell inside a header");
}
