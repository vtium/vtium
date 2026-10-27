//! The Rust side of the parse oracle: vtium-parse driven by an event script
//! on stdin, writing one trace line per action to stdout in the format
//! `harness/oracle.c` writes for the C reference.
//!
//! Each data record is fed in pieces of varying size, cut by a generator
//! seeded with the stream's number, while the C side takes each record
//! whole. The script and the trace are described in this directory's
//! README.

use std::fmt::Write as _;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use vtium_parse::{Chunk, Consumer, InputMode, Intermediates, Params, Parser, StringEnd};

/// Appends formatted text to a `String`, which cannot fail.
macro_rules! out {
    ($dst:expr, $($arg:tt)*) => {
        $dst.write_fmt(format_args!($($arg)*)).expect("writing to a String")
    };
}

/// The consumer: writes each action as a trace line and carries the
/// script's triggers.
struct Trace {
    out: String,
    mode: InputMode,
    /// Counted lines left until the parser pauses, 0 for none armed.
    pause_left: u32,
    /// The pause is followed by a reset.
    reset_at_pause: bool,
    paused: bool,
    /// ESC and CSI dispatches left until the mode switches, 0 for none
    /// armed.
    mode_left: u32,
    mode_to: InputMode,
}

impl Trace {
    fn new(mode: InputMode) -> Self {
        Trace {
            out: String::new(),
            mode,
            pause_left: 0,
            reset_at_pause: false,
            paused: false,
            mode_left: 0,
            mode_to: mode,
        }
    }

    fn set_mode(&mut self, mode: InputMode) {
        self.mode = mode;
        out!(self.out, "mode {}\n", mode_char(mode));
    }

    /// Ends a trace line. A counted line moves the pause trigger on; a
    /// dispatch line (ESC or CSI) moves the mode trigger on.
    fn line_end(&mut self, counted: bool, dispatch: bool) {
        self.out.push('\n');
        if counted && self.pause_left > 0 {
            self.pause_left -= 1;
            self.paused = self.pause_left == 0;
        }
        if dispatch && self.mode_left > 0 {
            self.mode_left -= 1;
            if self.mode_left == 0 {
                self.set_mode(self.mode_to);
            }
        }
    }

    fn intermediates(&mut self, marker: Option<u8>, intermediates: Intermediates) {
        if marker.is_none() && intermediates.is_empty() {
            self.out.push_str(" -");
            return;
        }
        self.out.push(' ');
        for b in marker.iter().chain(intermediates.as_bytes()) {
            out!(self.out, "{b:02x}");
        }
    }

    fn params(&mut self, params: &Params) {
        self.out.push_str(" [");
        for i in 0..params.count() {
            if i > 0 {
                self.out.push(',');
            }
            out!(self.out, "{}", value(params.get(i)));
            for sub in params.subparams(i) {
                out!(self.out, ":{}", value(sub));
            }
        }
        self.out.push(']');
    }

    fn end(&mut self, end: StringEnd) {
        match end {
            StringEnd::St => self.out.push_str("st"),
            StringEnd::Bel => self.out.push_str("bel"),
            StringEnd::Cancelled(b) => out!(self.out, "cancel:{b:x}"),
        }
    }

    fn hex(&mut self, bytes: &[u8]) {
        for b in bytes {
            out!(self.out, "{b:02x}");
        }
    }

    /// A DCS or APC string's data, one uncounted line per character.
    fn put(&mut self, kind: &str, data: Chunk<'_>) {
        match data {
            Chunk::Bytes(run) => {
                for &b in run {
                    out!(self.out, "{kind} {b:x}");
                    self.line_end(false, false);
                }
            }
            Chunk::Char(c) => {
                out!(self.out, "{kind} {:x}", u32::from(c));
                self.line_end(false, false);
            }
        }
    }
}

/// A parameter as the C side prints it, -1 for one omitted.
fn value(v: Option<u32>) -> i64 {
    v.map_or(-1, i64::from)
}

impl Consumer for Trace {
    fn print(&mut self, c: char) {
        out!(self.out, "print {:x}", u32::from(c));
        self.line_end(true, false);
    }

    fn execute(&mut self, control: u8) {
        out!(self.out, "exec {control:x}");
        self.line_end(true, false);
    }

    fn esc_dispatch(&mut self, intermediates: Intermediates, final_byte: u8) {
        out!(self.out, "esc {final_byte:x}");
        self.intermediates(None, intermediates);
        self.line_end(true, true);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8) {
        out!(self.out, "csi {final_byte:x}");
        self.intermediates(params.marker(), intermediates);
        self.params(params);
        self.line_end(true, true);
    }

    fn osc_end(&mut self, data: &[u8], truncated: bool, end: StringEnd) {
        self.out.push_str("osc ");
        self.end(end);
        self.out.push_str(if truncated { " t " } else { " " });
        self.hex(data);
        self.line_end(true, false);
    }

    fn dcs_hook(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8) {
        out!(self.out, "dcs_hook {final_byte:x}");
        self.intermediates(params.marker(), intermediates);
        self.params(params);
        self.line_end(true, false);
    }

    fn dcs_put(&mut self, data: Chunk<'_>) {
        self.put("dcs_put", data);
    }

    fn dcs_unhook(&mut self, end: StringEnd) {
        self.out.push_str("dcs_unhook ");
        self.end(end);
        self.line_end(true, false);
    }

    fn apc_start(&mut self) {
        self.out.push_str("apc_start");
        self.line_end(true, false);
    }

    fn apc_put(&mut self, data: Chunk<'_>) {
        self.put("apc_put", data);
    }

    fn apc_end(&mut self, end: StringEnd) {
        self.out.push_str("apc_end ");
        self.end(end);
        self.line_end(true, false);
    }

    fn xtgettcap(&mut self, payload: Option<&[u8]>) {
        match payload {
            None => self.out.push_str("tcap none"),
            Some(payload) => {
                self.out.push_str("tcap ");
                self.hex(payload);
            }
        }
        self.line_end(true, false);
    }

    fn input_mode(&self) -> InputMode {
        self.mode
    }

    fn paused(&self) -> bool {
        self.paused
    }
}

fn mode_char(mode: InputMode) -> char {
    match mode {
        InputMode::Utf8 => 'u',
        InputMode::EightBit => '8',
        InputMode::SevenBit => '7',
    }
}

fn mode_of(b: u8) -> Result<InputMode, String> {
    match b {
        b'u' => Ok(InputMode::Utf8),
        b'8' => Ok(InputMode::EightBit),
        b'7' => Ok(InputMode::SevenBit),
        _ => Err(format!("no mode '{}'", b.escape_ascii())),
    }
}

/// The piece sizes a data record is cut into: xorshift64, seeded with the
/// stream's number. A quarter of the pieces are one byte, so cuts fall
/// inside UTF-8 characters and sequences.
struct Cuts(u64);

impl Cuts {
    fn new(stream: u64) -> Self {
        Cuts(stream.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }

    fn next(&mut self, left: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        let r = usize::try_from(self.0 >> 8).unwrap_or(usize::MAX);
        let len = match self.0 & 7 {
            0 | 1 => 1,
            2..=5 => 2 + r % 15,
            6 => 17 + r % 240,
            _ => left,
        };
        len.min(left)
    }
}

/// One stream: a parser, its consumer and its cuts.
struct Stream {
    parser: Parser,
    trace: Trace,
    cuts: Cuts,
}

impl Stream {
    fn new(n: u64, mode: InputMode) -> Self {
        let mut trace = Trace::new(mode);
        out!(trace.out, "stream {n}\n");
        trace.set_mode(mode);
        Stream {
            parser: Parser::new(),
            trace,
            cuts: Cuts::new(n),
        }
    }

    fn reset(&mut self) {
        self.parser.reset();
        self.trace.out.push_str("reset\n");
    }

    /// Feeds one data record in pieces, resuming after each pause.
    fn feed(&mut self, data: &[u8]) -> Result<(), String> {
        let mut off = 0;
        while off < data.len() {
            let len = self.cuts.next(data.len() - off);
            let piece = &data[off..off + len];
            let n = self.parser.advance(&mut self.trace, piece);
            off += n;
            if self.trace.paused {
                self.trace.paused = false;
                out!(self.trace.out, "pause {off}\n");
                if self.trace.reset_at_pause {
                    self.reset();
                }
            } else if n < len {
                return Err("input stopped without a pause".into());
            }
        }
        Ok(())
    }
}

/// Reads the script's records in turn.
struct Records<'a>(&'a [u8]);

impl<'a> Records<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.0.len() < n {
            return Err("the script ends inside a record".into());
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

fn run(script: &[u8], out: &mut impl Write) -> Result<(), String> {
    let mut records = Records(script);
    let mut streams = 0;
    let mut stream: Option<Stream> = None;
    while !records.0.is_empty() {
        let kind = records.byte()?;
        if kind == b'S' {
            if let Some(done) = stream.take() {
                out.write_all(done.trace.out.as_bytes())
                    .map_err(|e| e.to_string())?;
            }
            stream = Some(Stream::new(streams, mode_of(records.byte()?)?));
            streams += 1;
            continue;
        }
        let s = stream
            .as_mut()
            .ok_or("the script starts without a stream")?;
        match kind {
            b'D' => {
                let len = records.u32()? as usize;
                s.feed(records.take(len)?)?;
            }
            b'R' => s.reset(),
            b'M' => {
                let mode = mode_of(records.byte()?)?;
                s.trace.set_mode(mode);
            }
            b'P' | b'Q' => {
                s.trace.pause_left = records.u32()?;
                s.trace.reset_at_pause = kind == b'Q';
            }
            b'T' => {
                s.trace.mode_left = records.u32()?;
                s.trace.mode_to = mode_of(records.byte()?)?;
            }
            _ => return Err(format!("no record kind '{}'", kind.escape_ascii())),
        }
    }
    if let Some(done) = stream {
        out.write_all(done.trace.out.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    let mut script = Vec::new();
    if let Err(e) = io::stdin().read_to_end(&mut script) {
        eprintln!("parse-oracle: {e}");
        return ExitCode::from(2);
    }
    let mut out = io::BufWriter::new(io::stdout().lock());
    match run(&script, &mut out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("parse-oracle: {e}");
            ExitCode::from(2)
        }
    }
}
