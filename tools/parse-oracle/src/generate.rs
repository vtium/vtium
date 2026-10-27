//! The streams a run compares: the C reference's conformance inputs, the
//! written cases, and a seeded generator.

use vtium_parse::InputMode;

use crate::script::{Record, Stream};

include!(concat!(env!("OUT_DIR"), "/corpus.rs"));

/// splitmix64: a small seeded generator, so a seed names the same streams
/// on every machine.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number below `n`.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo)
    }

    /// True `per_mille` times in a thousand.
    fn chance(&mut self, per_mille: usize) -> bool {
        self.below(1000) < per_mille
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const MODES: [InputMode; 3] = [InputMode::Utf8, InputMode::EightBit, InputMode::SevenBit];

/// The parse README's OSC bound, 1 MiB, written here and not taken from
/// vtium-parse so that a wrong `OSC_MAX` there moves no case.
const OSC_BOUND: usize = 1 << 20;

fn repeat(prefix: &[u8], b: &[u8], n: usize, suffix: &[u8]) -> Vec<u8> {
    [prefix, &b.repeat(n), suffix].concat()
}

/// Pieces of input the token generator joins: controls, introducers, UTF-8
/// well- and ill-formed, and runs at each bound of the parse README's
/// bounds table.
fn tokens() -> Vec<Vec<u8>> {
    let mut t: Vec<Vec<u8>> = [
        &b"\x1b"[..],
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
        b"\xc1\x81",
        b"\xe0\x80\x9b",
        b"\xed\xa0\x80",
        b"\xef\xbf\xbf",
        b"\xef\xb7\x90",
        b"\xf4\x90\x80\x80",
        b"\xf7\xbf\xbf\xbf",
        b"\xff",
        b"\x80",
        b"\xa0",
        b"\xbb",
        b"\xdc",
        b"\xe9",
        b"\x1bP+q",
        b"\x1bP++q",
        b"544e;436f",
        b"\x1b]",
        b"\x1b_G",
        b"\x1bPq",
        b"\x1b[?",
        b"\x1b\\",
        b"text",
        b"a=T,f=32;",
        b"AAAA",
        // PARAM_VALUE_CAP
        b"65535",
        b"65536",
        b"655359",
        b"6553599",
        b"99999999",
        // INTERMEDIATES_MAX
        b"\x1b[!!p",
        b"\x1b[ !\"p",
        b"\x1b(%%0",
        b"\x1b[1 !\"m",
        b"\x1bP !\"q",
    ]
    .iter()
    .map(|b| b.to_vec())
    .collect();
    t.extend([
        // PARAMS_MAX and SUBPARAMS_MAX
        repeat(b"", b"1;", 15, b""),
        repeat(b"", b"1;", 16, b""),
        repeat(b"", b";", 17, b""),
        repeat(b"", b"1:", 8, b""),
        repeat(b"", b"1:", 9, b""),
        repeat(b"", b":", 9, b""),
        // the C reference's OSC_MAX_LEN, which osc-bound passes: a byte
        // short, at and past it, and a character across it
        repeat(b"\x1b]", b"x", 126, b"\x07"),
        repeat(b"\x1b]", b"x", 127, b"\x07"),
        repeat(b"\x1b]", b"x", 128, b"\x07"),
        repeat(b"\x1b]", b"x", 126, b"\xc3\xa9y\x07"),
        repeat(b"\x1b]", b"x", 125, b"\xe2\x82\xacy\x1b\\"),
        // XTGETTCAP_MAX
        repeat(b"\x1bP+q", b"5", 1038, b"\x1b\\"),
        repeat(b"\x1bP+q", b"5", 1039, b"\x1b\\"),
        repeat(b"\x1bP+q", b"5", 1040, b"\x9c"),
    ]);
    t
}

const TERMINATORS: &[&[u8]] = &[
    b"\x1b\\",
    b"\x07",
    b"\x9c",
    b"\xc2\x9c",
    b"\x18",
    b"\x1a",
    b"\x1b[",
    b"\x1bX",
    b"\x1b\x1b\\",
    b"\x85",
    b"\xc2\x85",
    b"",
];
const CSI: &[&[u8]] = &[b"\x1b[", b"\x9b", b"\xc2\x9b"];
const DCS: &[&[u8]] = &[b"\x1bP", b"\x90", b"\xc2\x90"];
const OSC: &[&[u8]] = &[b"\x1b]", b"\x9d", b"\xc2\x9d"];
const APC: &[&[u8]] = &[b"\x1b_", b"\x9f", b"\xc2\x9f"];
const SOS: &[&[u8]] = &[b"\x1bX", b"\x1b^", b"\x98", b"\x9e"];
const TEXT: &[&[u8]] = &[
    b"a",
    b"Z",
    b" ",
    b"0",
    b";",
    b"\x7f",
    b"\t",
    b"\n",
    b"\x05",
    b"\xc3\xa9",
    b"\xe2\x82\xac",
    b"\xf0\x9f\x98\x80",
    b"\xef\xbf\xbe",
    b"\xe2\x82",
    b"\xa9",
    b"\xff",
    b"\xc0\x80",
];
const INTERMEDIATES: &[u8] = b" !\"#$%&'()*+,-./";

fn params(rng: &mut Rng, out: &mut Vec<u8>) {
    const VALUES: &[&[u8]] = &[
        b"", b"", b"0", b"1", b"5", b"38", b"65535", b"65536", b"655359", b"00012", b"9999999",
    ];
    let n = *rng.pick(&[0, 1, 1, 2, 3, 5, 15, 16, 17, 20]);
    for i in 0..n {
        if i > 0 {
            out.push(b';');
        }
        out.extend(*rng.pick(VALUES));
        for _ in 0..*rng.pick(&[0, 0, 0, 1, 2, 8, 9]) {
            out.push(b':');
            out.extend(*rng.pick(VALUES));
        }
    }
}

/// A CSI or DCS header after its introducer: marker, parameters,
/// intermediates and final, sometimes malformed.
fn header(rng: &mut Rng, dcs: bool, out: &mut Vec<u8>) {
    if rng.chance(300) {
        out.push(*rng.pick(b"<=>?"));
    }
    params(rng, out);
    if rng.chance(100) {
        out.push(*rng.pick(b"<=>?:"));
    }
    for _ in 0..*rng.pick(&[0, 0, 0, 1, 1, 2, 3]) {
        out.push(*rng.pick(INTERMEDIATES));
    }
    if rng.chance(50) {
        out.extend(*rng.pick(TEXT));
    }
    let finals: &[u8] = if dcs { b"qpq|" } else { b"mHJKhlrptcqn@~" };
    out.push(if rng.chance(800) {
        *rng.pick(finals)
    } else {
        rng.range(0x40, 0x7f) as u8
    });
}

fn payload(rng: &mut Rng, n: usize, out: &mut Vec<u8>) {
    for _ in 0..n {
        out.extend(*rng.pick(TEXT));
    }
}

fn sequence(rng: &mut Rng, out: &mut Vec<u8>) {
    match rng.below(10) {
        0 => {
            let n = rng.range(1, 20);
            payload(rng, n, out);
        }
        1 => out.push(*rng.pick(b"\x00\x05\x07\x08\x0a\x0d\x0e\x0f\x18\x1a\x84\x85\x9c")),
        2 => {
            out.push(0x1b);
            for _ in 0..*rng.pick(&[0, 0, 1, 2, 3]) {
                out.push(*rng.pick(INTERMEDIATES));
            }
            out.push(rng.range(0x30, 0x7f) as u8);
        }
        3 | 4 => {
            out.extend(*rng.pick(CSI));
            header(rng, false, out);
        }
        5 => {
            out.extend(*rng.pick(DCS));
            if rng.chance(300) {
                out.extend(b"+q");
                for i in 0..rng.range(1, 4) {
                    if i > 0 {
                        out.push(b';');
                    }
                    out.extend(b"544e".repeat(rng.range(1, 4)));
                }
            } else {
                header(rng, true, out);
                let n = rng.below(60);
                payload(rng, n, out);
            }
            out.extend(*rng.pick(TERMINATORS));
        }
        6 | 7 => {
            out.extend(*rng.pick(OSC));
            out.extend(b"0;");
            out.extend(
                b"x".repeat(*rng.pick(&[0, 5, 40, 120, 125, 126, 127, 128, 140, 300, 5000])),
            );
            let n = rng.below(4);
            payload(rng, n, out);
            out.extend(*rng.pick(TERMINATORS));
        }
        8 => {
            out.extend(*rng.pick(APC));
            out.extend(b"Ga=T,f=32;");
            let n = rng.below(40);
            payload(rng, n, out);
            out.extend(*rng.pick(TERMINATORS));
        }
        _ => {
            out.extend(*rng.pick(SOS));
            let n = rng.below(20);
            payload(rng, n, out);
            out.extend(*rng.pick(TERMINATORS));
        }
    }
}

fn body(rng: &mut Rng, tokens: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    match rng.below(5) {
        0 => (0..rng.range(1, 3000)).for_each(|_| out.push(rng.below(256) as u8)),
        1 | 2 => (0..rng.range(1, 1500)).for_each(|_| out.extend(rng.pick(tokens))),
        _ => (0..rng.range(1, 200)).for_each(|_| sequence(rng, &mut out)),
    }
    out
}

/// `data` cut into records, with resets, mode switches, pauses and mode
/// triggers between them; half the cuts fall right after an ESC, where a
/// string's end waits.
fn with_events(rng: &mut Rng, label: String, mode: InputMode, data: &[u8]) -> Stream {
    let escs: Vec<usize> = (0..data.len())
        .filter(|&i| data[i] == 0x1b)
        .map(|i| i + 1)
        .collect();
    let mut cuts: Vec<usize> = (0..rng.below(12))
        .map(|_| {
            if !escs.is_empty() && rng.chance(500) {
                *rng.pick(&escs)
            } else {
                rng.below(data.len() + 1)
            }
        })
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts.push(data.len());
    let mut records = Vec::new();
    let mut start = 0;
    for cut in cuts {
        for _ in 0..*rng.pick(&[0, 0, 1, 1, 2]) {
            records.push(match rng.below(100) {
                0..30 => Record::Reset,
                30..45 => Record::Mode(*rng.pick(&MODES)),
                45..70 => Record::Pause {
                    after: rng.range(1, 30) as u32,
                    reset: rng.chance(300),
                },
                _ => Record::ModeAfter {
                    after: rng.range(1, 10) as u32,
                    mode: *rng.pick(&MODES),
                },
            });
        }
        records.push(Record::Data(data[start..cut].to_vec()));
        start = cut;
    }
    Stream::new(label, mode, records)
}

/// FNV-1a, to seed each corpus input's events from its name.
fn name_seed(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}

/// Every conformance input of the C reference at the pin, whole in each
/// mode, and once more cut and with events.
pub fn corpus() -> impl Iterator<Item = Stream> {
    CORPUS.iter().flat_map(|&(name, data)| {
        let whole = MODES.map(|mode| {
            Stream::new(
                format!("{name} mode {}", char::from(crate::script::mode_char(mode))),
                mode,
                vec![Record::Data(data.to_vec())],
            )
        });
        let events = with_events(
            &mut Rng::new(name_seed(name)),
            format!("{name} with events"),
            InputMode::Utf8,
            data,
        );
        whole.into_iter().chain([events])
    })
}

pub fn corpus_len() -> usize {
    CORPUS.len()
}

/// `n` streams from `seed`: random bytes, tokens and grammar-built
/// sequences, most cut into records with events.
pub fn generated(seed: u64, n: usize) -> impl Iterator<Item = Stream> {
    let tokens = tokens();
    let mut rng = Rng::new(seed);
    (0..n).map(move |i| {
        let mode = *rng.pick(&[
            InputMode::Utf8,
            InputMode::Utf8,
            InputMode::EightBit,
            InputMode::SevenBit,
        ]);
        let data = body(&mut rng, &tokens);
        let label = format!("generated seed {seed} #{i}");
        if rng.chance(300) {
            Stream::new(label, mode, vec![Record::Data(data)])
        } else {
            with_events(&mut rng, label, mode, &data)
        }
    })
}

/// Streams written for the edges a random cut reaches rarely.
pub fn cases() -> Vec<Stream> {
    use Record::{Data, Mode, ModeAfter, Pause, Reset};
    let d = |b: &[u8]| Data(b.to_vec());
    let case = |name: &str, records| Stream::new(format!("case {name}"), InputMode::Utf8, records);
    vec![
        case(
            "apc-reset-at-st",
            vec![d(b"\x1b_Ga=T;AAAA\x1b"), Reset, d(b"\\x")],
        ),
        case(
            "osc-reset-at-st",
            vec![d(b"\x1b]0;title\x1b"), Reset, d(b"\\x")],
        ),
        case(
            "dcs-reset-at-st",
            vec![d(b"\x1bPqdata\x1b"), Reset, d(b"\\x")],
        ),
        case(
            "xtgettcap-reset-at-st",
            vec![d(b"\x1bP+q544e\x1b"), Reset, d(b"\\x")],
        ),
        case(
            "apc-reset-twice",
            vec![d(b"\x1b_Gx\x1b"), Reset, Reset, d(b"\\x")],
        ),
        case(
            "decscl-inside-a-call",
            vec![
                ModeAfter {
                    after: 1,
                    mode: InputMode::EightBit,
                },
                d(&b"\x1b[61\"p\xe9\x9b1m\xc3\xa9".repeat(3)),
            ],
        ),
        case(
            "mode-mid-character",
            vec![
                d(b"\xc3"),
                Mode(InputMode::EightBit),
                d(b"\xa9"),
                Mode(InputMode::Utf8),
                d(b"\xa9\xe2\x82"),
                Mode(InputMode::SevenBit),
                d(b"\xac\xe2"),
                Mode(InputMode::Utf8),
                d(b"\x82\xac"),
            ],
        ),
        case(
            "pause-and-resume",
            vec![
                Pause {
                    after: 2,
                    reset: false,
                },
                d(b"\x1b[1mab\x1b_Gx\x1b\\cd"),
                Pause {
                    after: 1,
                    reset: true,
                },
                d(b"\x1b]0;t\x07\x1b[2m"),
            ],
        ),
        // OSC_MAX, a megabyte, too long for the tokens: a byte short, at
        // and past it, a character across it, and an ST waiting at its ESC
        // across a call, then a string that starts clean
        case(
            "osc-at-the-bound",
            vec![
                Data(repeat(b"\x1b]", b"x", OSC_BOUND - 1, b"\x07")),
                Data(repeat(b"\x1b]", b"x", OSC_BOUND, b"\x07")),
                Data(repeat(b"\x1b]", b"x", OSC_BOUND + 1, b"\x07")),
            ],
        ),
        case(
            "osc-character-across-the-bound",
            vec![
                Data(repeat(b"\x1b]", b"x", OSC_BOUND - 1, b"\xc3\xa9y\x07")),
                Data(repeat(
                    b"\x1b]",
                    b"x",
                    OSC_BOUND - 2,
                    b"\xe2\x82\xacy\x1b\\",
                )),
            ],
        ),
        case(
            "osc-st-across-a-call-at-the-bound",
            vec![
                Data(repeat(b"\x1b]", b"x", OSC_BOUND, b"\x1b")),
                d(b"\\\x1b]0;z\x07"),
            ],
        ),
    ]
}
