//! A consumer that records every action and checks, at each call, the
//! bounds and ranges the parser promises.
#![allow(dead_code)]

use vtium_parse::{
    Chunk, Consumer, InputMode, Intermediates, OSC_MAX, PARAMS_MAX, Params, Parser, SUBPARAMS_MAX,
    XTGETTCAP_MAX,
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
    Esc(Intermediates, u8),
    Csi(Vec<i32>, Intermediates, u8),
    Osc(Vec<u8>, u8),
    DcsHook(Vec<i32>, Intermediates, u32),
    DcsByte(u8),
    DcsChar(u32),
    DcsUnhook(u8),
    ApcStart,
    ApcByte(u8),
    ApcChar(u32),
    ApcEnd(u8),
    Xtgettcap(Vec<u8>, bool),
}

/// A recording consumer.
#[derive(Default)]
pub struct Recorder {
    pub acts: Vec<Act>,
    /// Sub-parameters of each CSI dispatch, by parameter.
    pub subs: Vec<Vec<Vec<i32>>>,
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

fn values(params: &Params) -> Vec<i32> {
    (0..params.count()).map(|i| params.get(i)).collect()
}

fn check_params(params: &Params) {
    assert!(
        (1..=PARAMS_MAX).contains(&params.count()),
        "count {}",
        params.count()
    );
    for i in 0..PARAMS_MAX {
        let v = params.get(i);
        assert!((-1..=VALUE_MAX).contains(&v), "param {i} = {v}");
        let subs = params.subparams(i);
        assert!(subs.len() <= SUBPARAMS_MAX, "{} subparams", subs.len());
        assert!(subs.is_empty() || i < params.count());
        for &s in subs {
            assert!((-1..=VALUE_MAX).contains(&s), "subparam {s}");
        }
    }
    assert_eq!(params.get(PARAMS_MAX), -1);
    assert!(params.subparams(PARAMS_MAX).is_empty());
}

fn check_terminator(t: u8) {
    assert!(
        matches!(t, 0x07 | 0x18 | 0x1a | 0x1b | 0x80..=0x9f),
        "terminator {t:#x}"
    );
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
        self.push(Act::Esc(intermediates, final_byte));
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8) {
        assert!(
            (0x40..=0x7e).contains(&final_byte),
            "csi final {final_byte:#x}"
        );
        check_params(params);
        if self.follow_decscl && final_byte == b'p' && !params.has_subparams() {
            if intermediates.contains(Intermediates::GREATER) {
            } else if intermediates.contains(Intermediates::BANG) {
                self.mode = InputMode::Utf8;
            } else if intermediates.contains(Intermediates::DOLLAR) {
            } else if intermediates.difference(Intermediates::DQUOTE).is_empty() {
                self.mode = match params.get(0) {
                    61 => InputMode::SevenBit,
                    62..=64 => InputMode::EightBit,
                    _ => InputMode::Utf8,
                };
            }
        }
        self.subs.push(
            (0..params.count())
                .map(|i| params.subparams(i).to_vec())
                .collect(),
        );
        self.push(Act::Csi(values(params), intermediates, final_byte));
    }

    fn osc_dispatch(&mut self, data: &[u8], terminator: u8) {
        assert!(data.len() <= OSC_MAX, "osc of {} bytes", data.len());
        assert!(std::str::from_utf8(data).is_ok(), "osc not UTF-8");
        check_terminator(terminator);
        self.push(Act::Osc(data.to_vec(), terminator));
    }

    fn dcs_hook(&mut self, params: &Params, intermediates: Intermediates, final_char: char) {
        let final_char = u32::from(final_char);
        assert!(!self.in_dcs && !self.in_apc);
        assert!(
            !params.has_subparams(),
            "a DCS with sub-parameters is ignored"
        );
        check_params(params);
        assert!(
            (0x40..=0x7e).contains(&final_char) || final_char >= 0xa0,
            "dcs final {final_char:#x}"
        );
        self.in_dcs = true;
        self.push(Act::DcsHook(values(params), intermediates, final_char));
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

    fn dcs_unhook(&mut self, terminator: u8) {
        assert!(self.in_dcs, "dcs_unhook outside a DCS string");
        check_terminator(terminator);
        assert_ne!(terminator, 0x07);
        self.in_dcs = false;
        self.push(Act::DcsUnhook(terminator));
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

    fn apc_end(&mut self, terminator: u8) {
        assert!(self.in_apc, "apc_end outside an APC string");
        check_terminator(terminator);
        assert_ne!(terminator, 0x07);
        self.in_apc = false;
        self.push(Act::ApcEnd(terminator));
    }

    fn xtgettcap(&mut self, payload: &[u8], overflowed: bool) {
        assert!(
            payload.len() <= XTGETTCAP_MAX,
            "payload of {}",
            payload.len()
        );
        assert!(!overflowed || payload.len() == XTGETTCAP_MAX);
        assert!(payload.iter().all(|&b| b < 0x80));
        self.push(Act::Xtgettcap(payload.to_vec(), overflowed));
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

pub fn csi(params: &[i32], intermediates: Intermediates, final_byte: u8) -> Act {
    Act::Csi(params.to_vec(), intermediates, final_byte)
}

pub fn esc(intermediates: Intermediates, final_byte: u8) -> Act {
    Act::Esc(intermediates, final_byte)
}

pub fn osc(data: &[u8], terminator: u8) -> Act {
    Act::Osc(data.to_vec(), terminator)
}

pub fn dcs_bytes(data: &[u8]) -> Vec<Act> {
    data.iter().map(|&b| Act::DcsByte(b)).collect()
}

pub fn apc_bytes(data: &[u8]) -> Vec<Act> {
    data.iter().map(|&b| Act::ApcByte(b)).collect()
}

pub fn tcap(payload: &[u8]) -> Act {
    Act::Xtgettcap(payload.to_vec(), false)
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
