//! The Rust side: vtium-parse writing the trace the C side writes.
//!
//! Each data record is fed in pieces of varying size, cut by a generator
//! seeded with the stream's number, while the C side takes each record
//! whole, so the comparison also checks that the actions do not depend on
//! where the input is cut.

use vtium_parse::{Chunk, Consumer, InputMode, Intermediates, Params, Parser, StringEnd};

use crate::script::{Line, Side, Trace, out};

/// The consumer: one trace line per action.
struct Writer(Trace);

impl Writer {
    fn intermediates(&mut self, marker: Option<u8>, intermediates: Intermediates) {
        let bytes: Vec<u32> = intermediates
            .as_bytes()
            .iter()
            .map(|&b| u32::from(b))
            .collect();
        self.0.intermediates(marker.map(u32::from), &bytes);
    }

    fn params(&mut self, params: &Params) {
        let value = |v: Option<u32>| v.map_or(-1, i64::from);
        self.0.params(
            (0..params.count()).map(|i| (value(params.get(i)), params.subparams(i).map(value))),
        );
    }

    fn end(&mut self, end: StringEnd) {
        self.0.string_end(match end {
            StringEnd::St => 0x9c,
            StringEnd::Bel => 0x07,
            StringEnd::Cancelled(b) => u32::from(b),
        });
    }

    /// A DCS or APC string's data, one line per character.
    fn put(&mut self, kind: &str, data: Chunk<'_>) {
        let mut line = |c: u32| {
            out!(self.0.out, "{kind} {c:x}");
            self.0.end(Line::Data);
        };
        match data {
            Chunk::Bytes(run) => run.iter().for_each(|&b| line(u32::from(b))),
            Chunk::Char(c) => line(u32::from(c)),
        }
    }
}

impl Consumer for Writer {
    fn print(&mut self, c: char) {
        out!(self.0.out, "print {:x}", u32::from(c));
        self.0.end(Line::Counted);
    }

    fn execute(&mut self, control: u8) {
        out!(self.0.out, "exec {control:x}");
        self.0.end(Line::Counted);
    }

    fn esc_dispatch(&mut self, intermediates: Intermediates, final_byte: u8) {
        out!(self.0.out, "esc {final_byte:x}");
        self.intermediates(None, intermediates);
        self.0.end(Line::Dispatch);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8) {
        out!(self.0.out, "csi {final_byte:x}");
        self.intermediates(params.marker(), intermediates);
        self.params(params);
        self.0.end(Line::Dispatch);
    }

    fn osc_end(&mut self, data: &[u8], truncated: bool, end: StringEnd) {
        self.0.out.push_str("osc ");
        self.end(end);
        self.0.out.push_str(if truncated { " t " } else { " " });
        self.0.hex(data);
        self.0.end(Line::Counted);
    }

    fn dcs_hook(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8) {
        out!(self.0.out, "dcs_hook {final_byte:x}");
        self.intermediates(params.marker(), intermediates);
        self.params(params);
        self.0.end(Line::Counted);
    }

    fn dcs_put(&mut self, data: Chunk<'_>) {
        self.put("dcs_put", data);
    }

    fn dcs_unhook(&mut self, end: StringEnd) {
        self.0.out.push_str("dcs_unhook ");
        self.end(end);
        self.0.end(Line::Counted);
    }

    fn apc_start(&mut self) {
        self.0.out.push_str("apc_start");
        self.0.end(Line::Counted);
    }

    fn apc_put(&mut self, data: Chunk<'_>) {
        self.put("apc_put", data);
    }

    fn apc_end(&mut self, end: StringEnd) {
        self.0.out.push_str("apc_end ");
        self.end(end);
        self.0.end(Line::Counted);
    }

    fn xtgettcap(&mut self, payload: Option<&[u8]>) {
        match payload {
            None => self.0.out.push_str("tcap none"),
            Some(payload) => {
                self.0.out.push_str("tcap ");
                self.0.hex(payload);
            }
        }
        self.0.end(Line::Counted);
    }

    fn input_mode(&self) -> InputMode {
        self.0.mode()
    }

    fn paused(&self) -> bool {
        self.0.paused()
    }
}

/// The piece sizes a data record is cut into: xorshift64, seeded with the
/// stream's number. A quarter of the pieces are one byte, so cuts fall
/// inside UTF-8 characters and sequences.
struct Cuts(u64);

impl Cuts {
    fn next(&mut self, left: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        let r = (self.0 >> 8) as usize;
        let len = match self.0 & 7 {
            0 | 1 => 1,
            2..=5 => 2 + r % 15,
            6 => 17 + r % 240,
            _ => left,
        };
        len.min(left)
    }
}

/// The Rust side of one stream.
pub struct RustSide {
    parser: Parser,
    writer: Writer,
    cuts: Cuts,
}

impl RustSide {
    pub fn new(mode: InputMode, number: u64) -> Self {
        RustSide {
            parser: Parser::new(),
            writer: Writer(Trace::new(mode)),
            cuts: Cuts(number.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1),
        }
    }
}

impl Side for RustSide {
    fn trace(&mut self) -> &mut Trace {
        &mut self.writer.0
    }

    /// Feeds one piece of the data.
    fn feed(&mut self, data: &[u8]) -> usize {
        let piece = &data[..self.cuts.next(data.len())];
        let n = self.parser.advance(&mut self.writer, piece);
        assert!(
            n == piece.len() || self.writer.paused(),
            "vtium-parse stopped without a pause"
        );
        n
    }

    fn reset(&mut self) {
        self.parser.reset();
        self.writer.0.out.push_str("reset\n");
    }
}
