//! A consumer that records every action and checks, at each call, the
//! bounds and ranges the parser promises.
#![allow(dead_code)]

pub use vtium_parse::StringEnd::{self, Bel, Cancelled, St};
use vtium_parse::{
    Chunk, Consumer, INTERMEDIATES_MAX, InputMode, Intermediates, OSC_MAX, PARAMS_MAX, Params,
    Parser, SUBPARAMS_MAX, XTGETTCAP_MAX,
};

/// The largest value a parameter reaches: digits stop once it is past
/// 0xffff (tsm-vte.c:1314-1316).
pub const VALUE_MAX: i32 = 0xffff * 10 + 9;

/// One recorded action. String data is recorded a character at a time, so
/// that runs cut at different places record the same.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    Print(u32),
    Execute(u8),
    /// The intermediates as text.
    Esc(String, u8),
    /// The values, the private marker if any then the intermediates as
    /// text, the final byte, and the sub-parameters of each parameter that
    /// has any, by parameter.
    Csi(Vec<i32>, String, u8, Vec<(usize, Vec<i32>)>),
    /// The kept bytes, whether the string was truncated, and its end.
    Osc(Vec<u8>, bool, StringEnd),
    DcsHook(Vec<i32>, String, u32),
    DcsByte(u8),
    DcsChar(u32),
    DcsUnhook(StringEnd),
    ApcStart,
    ApcByte(u8),
    ApcChar(u32),
    ApcEnd(StringEnd),
    /// The names, or `None` when they passed the bound.
    Xtgettcap(Option<Vec<u8>>),
}

/// A recording consumer.
#[derive(Default)]
pub struct Recorder {
    pub acts: Vec<Act>,
    pub mode: InputMode,
    /// Switch the input mode as the C reference's DECSCL, DECSTR and RIS
    /// do (tsm-vte.c:1032-1036, 1784-1823, 2692-2709).
    pub follow_decscl: bool,
    /// Pause after each action that matches.
    pub pause_on: Option<fn(&Act) -> bool>,
    pub paused: bool,
    pub in_dcs: bool,
    pub in_apc: bool,
}

/// A sequence's marker and intermediates as text, for comparing.
fn text(marker: Option<u8>, intermediates: Intermediates) -> String {
    let ims = intermediates.as_bytes();
    assert!(ims.len() <= INTERMEDIATES_MAX);
    assert!(
        ims.iter().all(|b| (0x20..=0x2f).contains(b)),
        "intermediates {ims:?}"
    );
    if let Some(m) = marker {
        assert!((0x3c..=0x3f).contains(&m), "marker {m:#x}");
    }
    marker.iter().chain(ims).map(|&b| char::from(b)).collect()
}

/// A parameter as the tests write it: -1 for omitted.
fn num(v: Option<u32>) -> i32 {
    v.map_or(-1, |v| i32::try_from(v).expect("parameter fits i32"))
}

fn values(params: &Params) -> Vec<i32> {
    (0..params.count()).map(|i| num(params.get(i))).collect()
}

fn check_params(params: &Params) {
    assert!(
        (1..=PARAMS_MAX).contains(&params.count()),
        "count {}",
        params.count()
    );
    for i in 0..PARAMS_MAX {
        let v = num(params.get(i));
        assert!((-1..=VALUE_MAX).contains(&v), "param {i} = {v}");
        let subs = params.subparams(i);
        assert!(subs.len() <= SUBPARAMS_MAX, "{} subparams", subs.len());
        assert!(subs.len() == 0 || i < params.count());
        for s in subs.map(num) {
            assert!((-1..=VALUE_MAX).contains(&s), "subparam {s}");
        }
    }
    assert_eq!(params.get(PARAMS_MAX), None);
    assert_eq!(params.subparams(PARAMS_MAX).len(), 0);
}

fn check_end(end: StringEnd, bel_allowed: bool) {
    match end {
        St => {}
        Bel => assert!(bel_allowed, "BEL ends only an OSC string"),
        Cancelled(b) => assert!(
            matches!(b, 0x18 | 0x1a | 0x1b | 0x80..=0x9b | 0x9d..=0x9f),
            "cancelled by {b:#x}"
        ),
    }
}

fn is_printable(c: u32) -> bool {
    (0x20..=0x7f).contains(&c) || c >= 0xa0
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, act: Act) {
        if let Some(pause) = self.pause_on {
            self.paused |= pause(&act);
        }
        self.acts.push(act);
    }

    /// Feeds `bytes` whole, resuming after each pause.
    pub fn feed(&mut self, parser: &mut Parser, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            self.paused = false;
            let n = parser.advance(self, bytes);
            assert!(
                n >= 1 && n <= bytes.len(),
                "advance took {n} of {}",
                bytes.len()
            );
            assert!(
                n == bytes.len() || self.paused,
                "stopped early without a pause"
            );
            bytes = &bytes[n..];
        }
    }
}

impl Consumer for Recorder {
    fn print(&mut self, c: char) {
        let c = u32::from(c);
        assert!(is_printable(c), "print {c:#x}");
        self.push(Act::Print(c));
    }

    fn execute(&mut self, control: u8) {
        // tsm-vte.c:3291-3298, 3325-3331
        assert!(
            matches!(control, 0x00..=0x1a | 0x1c..=0x1f | 0x80..=0x8f | 0x91..=0x97 | 0x99 | 0x9a | 0x9c),
            "execute {control:#x}"
        );
        self.push(Act::Execute(control));
    }

    fn esc_dispatch(&mut self, intermediates: Intermediates, final_byte: u8) {
        assert!(
            (0x30..=0x7e).contains(&final_byte),
            "esc final {final_byte:#x}"
        );
        if self.follow_decscl && final_byte == b'c' && intermediates.is_empty() {
            self.mode = InputMode::Utf8;
        }
        self.push(Act::Esc(text(None, intermediates), final_byte));
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8) {
        assert!(
            (0x40..=0x7e).contains(&final_byte),
            "csi final {final_byte:#x}"
        );
        check_params(params);
        if self.follow_decscl && final_byte == b'p' && !params.has_subparams() {
            // tsm-vte.c:2692-2709, with intermediates matched exactly
            let ims = intermediates.as_bytes();
            if params.marker().is_some() {
            } else if ims == b"!" {
                self.mode = InputMode::Utf8;
            } else if ims.is_empty() || ims == b"\"" {
                self.mode = match num(params.get(0)) {
                    61 => InputMode::SevenBit,
                    62..=64 => InputMode::EightBit,
                    _ => InputMode::Utf8,
                };
            }
        }
        self.push(Act::Csi(
            values(params),
            text(params.marker(), intermediates),
            final_byte,
            (0..params.count())
                .map(|i| (i, params.subparams(i).map(num).collect::<Vec<_>>()))
                .filter(|(_, subs)| !subs.is_empty())
                .collect(),
        ));
    }

    fn osc_end(&mut self, data: &[u8], truncated: bool, end: StringEnd) {
        assert!(data.len() <= OSC_MAX, "osc of {} bytes", data.len());
        assert!(std::str::from_utf8(data).is_ok(), "osc not UTF-8");
        check_end(end, true);
        self.push(Act::Osc(data.to_vec(), truncated, end));
    }

    fn dcs_hook(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8) {
        let final_char = u32::from(final_byte);
        assert!(!self.in_dcs && !self.in_apc);
        assert!(
            !params.has_subparams(),
            "a DCS with sub-parameters is ignored"
        );
        check_params(params);
        assert!(
            (0x40..=0x7e).contains(&final_char),
            "dcs final {final_char:#x}"
        );
        self.in_dcs = true;
        self.push(Act::DcsHook(
            values(params),
            text(params.marker(), intermediates),
            final_char,
        ));
    }

    fn dcs_put(&mut self, data: Chunk<'_>) {
        assert!(self.in_dcs, "dcs_put outside a DCS string");
        match data {
            Chunk::Bytes(run) => {
                assert!(!run.is_empty());
                for &b in run {
                    assert!(
                        b < 0x7f && !matches!(b, 0x18 | 0x1a | 0x1b),
                        "dcs byte {b:#x}"
                    );
                    self.push(Act::DcsByte(b));
                }
            }
            Chunk::Char(c) => {
                let c = u32::from(c);
                assert!(c >= 0xa0, "dcs char {c:#x}");
                self.push(Act::DcsChar(c));
            }
        }
    }

    fn dcs_unhook(&mut self, end: StringEnd) {
        assert!(self.in_dcs, "dcs_unhook outside a DCS string");
        check_end(end, false);
        self.in_dcs = false;
        self.push(Act::DcsUnhook(end));
    }

    fn apc_start(&mut self) {
        assert!(!self.in_dcs && !self.in_apc);
        self.in_apc = true;
        self.push(Act::ApcStart);
    }

    fn apc_put(&mut self, data: Chunk<'_>) {
        assert!(self.in_apc, "apc_put outside an APC string");
        match data {
            Chunk::Bytes(run) => {
                assert!(!run.is_empty());
                for &b in run {
                    assert!((0x20..0x7f).contains(&b), "apc byte {b:#x}");
                    self.push(Act::ApcByte(b));
                }
            }
            Chunk::Char(c) => {
                let c = u32::from(c);
                assert!(c >= 0xa0, "apc char {c:#x}");
                self.push(Act::ApcChar(c));
            }
        }
    }

    fn apc_end(&mut self, end: StringEnd) {
        assert!(self.in_apc, "apc_end outside an APC string");
        check_end(end, false);
        self.in_apc = false;
        self.push(Act::ApcEnd(end));
    }

    fn xtgettcap(&mut self, payload: Option<&[u8]>) {
        if let Some(payload) = payload {
            assert!(
                payload.len() <= XTGETTCAP_MAX,
                "payload of {}",
                payload.len()
            );
            assert!(payload.iter().all(|&b| b < 0x80));
        }
        self.push(Act::Xtgettcap(payload.map(<[u8]>::to_vec)));
    }

    fn input_mode(&self) -> InputMode {
        self.mode
    }

    fn paused(&self) -> bool {
        self.paused
    }
}

/// The actions `bytes` gives a fresh parser in UTF-8 mode.
pub fn parse(bytes: &[u8]) -> Vec<Act> {
    parse_in(InputMode::Utf8, bytes)
}

/// The actions `bytes` gives a fresh parser in 8-bit mode, where C1
/// controls are single bytes.
pub fn parse8(bytes: &[u8]) -> Vec<Act> {
    parse_in(InputMode::EightBit, bytes)
}

/// The actions `bytes` gives a fresh parser in `mode`.
pub fn parse_in(mode: InputMode, bytes: &[u8]) -> Vec<Act> {
    let mut parser = Parser::new();
    let mut rec = Recorder {
        mode,
        ..Recorder::new()
    };
    rec.feed(&mut parser, bytes);
    rec.acts
}

/// The actions `bytes` gives a fresh parser fed one byte at a time.
pub fn parse_bytewise(bytes: &[u8]) -> Vec<Act> {
    let mut parser = Parser::new();
    let mut rec = Recorder::new();
    for b in bytes.chunks(1) {
        rec.feed(&mut parser, b);
    }
    rec.acts
}

pub fn prints(s: &str) -> Vec<Act> {
    s.chars().map(|c| Act::Print(u32::from(c))).collect()
}

/// A CSI dispatch: `prefix` is the marker, if any, then the intermediates.
pub fn csi(params: &[i32], prefix: &str, final_byte: u8) -> Act {
    Act::Csi(params.to_vec(), prefix.to_string(), final_byte, Vec::new())
}

/// A CSI dispatch with sub-parameters: `subs` has each parameter that
/// carries any, by index.
pub fn csi_subs(params: &[i32], prefix: &str, final_byte: u8, subs: &[(usize, &[i32])]) -> Act {
    Act::Csi(
        params.to_vec(),
        prefix.to_string(),
        final_byte,
        subs.iter().map(|&(i, s)| (i, s.to_vec())).collect(),
    )
}

/// A DCS hook: `prefix` is the marker, if any, then the intermediates.
pub fn hook(params: &[i32], prefix: &str, final_char: u32) -> Act {
    Act::DcsHook(params.to_vec(), prefix.to_string(), final_char)
}

pub fn esc(intermediates: &str, final_byte: u8) -> Act {
    Act::Esc(intermediates.to_string(), final_byte)
}

pub fn osc(data: &[u8], end: StringEnd) -> Act {
    Act::Osc(data.to_vec(), false, end)
}

/// An OSC string truncated at [`OSC_MAX`]: `data` is the prefix it kept.
pub fn osc_truncated(data: &[u8], end: StringEnd) -> Act {
    Act::Osc(data.to_vec(), true, end)
}

pub fn dcs_bytes(data: &[u8]) -> Vec<Act> {
    data.iter().map(|&b| Act::DcsByte(b)).collect()
}

pub fn apc_bytes(data: &[u8]) -> Vec<Act> {
    data.iter().map(|&b| Act::ApcByte(b)).collect()
}

pub fn tcap(payload: &[u8]) -> Act {
    Act::Xtgettcap(Some(payload.to_vec()))
}

/// Concatenates action lists.
#[macro_export]
macro_rules! acts {
    ($($part:expr),* $(,)?) => {{
        let mut v: Vec<common::Act> = Vec::new();
        $(common::Extend::extend_into($part, &mut v);)*
        v
    }};
}

/// An action or a list of them, for [`acts!`].
pub trait Extend {
    fn extend_into(self, v: &mut Vec<Act>);
}

impl Extend for Act {
    fn extend_into(self, v: &mut Vec<Act>) {
        v.push(self);
    }
}

impl Extend for Vec<Act> {
    fn extend_into(self, v: &mut Vec<Act>) {
        v.extend(self);
    }
}
