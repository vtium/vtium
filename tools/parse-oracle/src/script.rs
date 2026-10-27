//! Streams of events, both sides' input, and the trace they write, with the
//! driver that plays a stream through either side.

use vtium_parse::InputMode;

/// Appends formatted text to a `String`, which cannot fail.
macro_rules! out {
    ($dst:expr, $($arg:tt)*) => {
        ::std::fmt::Write::write_fmt(&mut $dst, format_args!($($arg)*))
            .expect("writing to a String")
    };
}
pub(crate) use out;

/// One event of a stream.
#[derive(Clone, Debug)]
pub enum Record {
    /// Bytes fed to the parser.
    Data(Vec<u8>),
    /// The parser reset between calls.
    Reset,
    /// The mode switched between calls.
    Mode(InputMode),
    /// A pause after the `after`-th counted trace line from here, with a
    /// reset at it when `reset`, as a consumer handles RIS.
    Pause { after: u32, reset: bool },
    /// The mode switched after the `after`-th ESC or CSI dispatch from here,
    /// inside a call, as DECSCL switches it.
    ModeAfter { after: u32, mode: InputMode },
}

/// A fresh parser in `mode`, then the records. The number seeds the Rust
/// side's cuts.
#[derive(Clone, Debug)]
pub struct Stream {
    pub label: String,
    pub number: u64,
    pub mode: InputMode,
    pub records: Vec<Record>,
}

impl Stream {
    pub fn new(label: String, mode: InputMode, records: Vec<Record>) -> Self {
        Stream {
            label,
            number: 0,
            mode,
            records,
        }
    }
}

pub fn mode_char(mode: InputMode) -> u8 {
    match mode {
        InputMode::Utf8 => b'u',
        InputMode::EightBit => b'8',
        InputMode::SevenBit => b'7',
    }
}

pub fn mode_of(b: u8) -> Result<InputMode, String> {
    match b {
        b'u' => Ok(InputMode::Utf8),
        b'8' => Ok(InputMode::EightBit),
        b'7' => Ok(InputMode::SevenBit),
        _ => Err(format!("no mode '{}'", b.escape_ascii())),
    }
}

/// The streams as a script file: per stream `S` mode number, then per
/// record `D`
/// length bytes, `R`, `M` mode, `P` or `Q` count (`Q` resets at the
/// pause), or `T` count mode, with counts as 4-byte little-endian.
pub fn encode(streams: &[&Stream]) -> Vec<u8> {
    let mut out = Vec::new();
    for s in streams {
        out.extend([b'S', mode_char(s.mode)]);
        out.extend(s.number.to_le_bytes());
        for r in &s.records {
            match r {
                Record::Data(d) => {
                    out.push(b'D');
                    out.extend((d.len() as u32).to_le_bytes());
                    out.extend(d);
                }
                Record::Reset => out.push(b'R'),
                Record::Mode(m) => out.extend([b'M', mode_char(*m)]),
                Record::Pause { after, reset } => {
                    out.push(if *reset { b'Q' } else { b'P' });
                    out.extend(after.to_le_bytes());
                }
                Record::ModeAfter { after, mode } => {
                    out.push(b'T');
                    out.extend(after.to_le_bytes());
                    out.push(mode_char(*mode));
                }
            }
        }
    }
    out
}

pub fn decode(mut script: &[u8], label: &str) -> Result<Vec<Stream>, String> {
    fn take<'a>(s: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
        if s.len() < n {
            return Err("the script ends inside a record".into());
        }
        let (head, rest) = s.split_at(n);
        *s = rest;
        Ok(head)
    }
    fn count(s: &mut &[u8]) -> Result<u32, String> {
        let b = take(s, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    let mut streams: Vec<Stream> = Vec::new();
    while let Some(&kind) = script.first() {
        script = &script[1..];
        if kind == b'S' {
            let mode = mode_of(take(&mut script, 1)?[0])?;
            let n = take(&mut script, 8)?;
            let mut stream = Stream::new(format!("{label} #{}", streams.len()), mode, Vec::new());
            stream.number = u64::from_le_bytes(n.try_into().expect("8 bytes"));
            streams.push(stream);
            continue;
        }
        let record = match kind {
            b'D' => {
                let n = count(&mut script)? as usize;
                Record::Data(take(&mut script, n)?.to_vec())
            }
            b'R' => Record::Reset,
            b'M' => Record::Mode(mode_of(take(&mut script, 1)?[0])?),
            b'P' | b'Q' => Record::Pause {
                after: count(&mut script)?,
                reset: kind == b'Q',
            },
            b'T' => Record::ModeAfter {
                after: count(&mut script)?,
                mode: mode_of(take(&mut script, 1)?[0])?,
            },
            _ => return Err(format!("no record kind '{}'", kind.escape_ascii())),
        };
        streams
            .last_mut()
            .ok_or("the script starts without a stream")?
            .records
            .push(record);
    }
    Ok(streams)
}

/// What kind of trace line ended: a counted line moves the pause trigger
/// on, a dispatch (ESC or CSI) also moves the mode trigger on, and string
/// data moves neither, since the Rust side checks for a pause only after a
/// whole run of it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Counted,
    Dispatch,
    Data,
}

/// A trace being written, with the stream's triggers.
pub struct Trace {
    pub out: String,
    mode: InputMode,
    pause_left: u32,
    reset_at_pause: bool,
    paused: bool,
    mode_left: u32,
    mode_to: InputMode,
}

impl Trace {
    pub fn new(mode: InputMode) -> Self {
        let mut t = Trace {
            out: String::new(),
            mode,
            pause_left: 0,
            reset_at_pause: false,
            paused: false,
            mode_left: 0,
            mode_to: mode,
        };
        t.set_mode(mode);
        t
    }

    pub fn mode(&self) -> InputMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: InputMode) {
        self.mode = mode;
        out!(self.out, "mode {}\n", char::from(mode_char(mode)));
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Ends a trace line, moving the triggers on.
    pub fn end(&mut self, line: Line) {
        self.out.push('\n');
        if line != Line::Data && self.pause_left > 0 {
            self.pause_left -= 1;
            self.paused = self.pause_left == 0;
        }
        if line == Line::Dispatch && self.mode_left > 0 {
            self.mode_left -= 1;
            if self.mode_left == 0 {
                self.set_mode(self.mode_to);
            }
        }
    }

    /// A string's end: `st`, `bel` or `cancel:` the byte that cancelled it.
    pub fn string_end(&mut self, end: u32) {
        match end {
            0x9c => self.out.push_str("st"),
            0x07 => self.out.push_str("bel"),
            b => out!(self.out, "cancel:{b:x}"),
        }
    }

    /// A private marker and intermediates as hex bytes, split by `/`
    /// (`3f/24`) so a byte on the wrong side shows, or `-` for none.
    pub fn intermediates(&mut self, marker: Option<u32>, bytes: &[u32]) {
        if marker.is_none() && bytes.is_empty() {
            self.out.push_str(" -");
            return;
        }
        self.out.push(' ');
        if let Some(m) = marker {
            out!(self.out, "{m:02x}");
        }
        self.out.push('/');
        for b in bytes {
            out!(self.out, "{b:02x}");
        }
    }

    /// Parameters, each a value (-1 when omitted) and its sub-parameters.
    pub fn params<S: IntoIterator<Item = i64>>(
        &mut self,
        params: impl IntoIterator<Item = (i64, S)>,
    ) {
        self.out.push_str(" [");
        for (i, (value, subs)) in params.into_iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            out!(self.out, "{value}");
            for sub in subs {
                out!(self.out, ":{sub}");
            }
        }
        self.out.push(']');
    }

    pub fn hex(&mut self, bytes: &[u8]) {
        for b in bytes {
            out!(self.out, "{b:02x}");
        }
    }
}

/// A parser driven by a stream: one side of the differential.
pub trait Side {
    /// The trace, for the driver's own lines and the triggers.
    fn trace(&mut self) -> &mut Trace;
    /// Feeds some of `data`, returning how much was taken.
    fn feed(&mut self, data: &[u8]) -> usize;
    /// Resets the parser; the side writes the `reset` line.
    fn reset(&mut self);
    /// The trace's mode changed outside a call.
    fn mode_changed(&mut self) {}
}

/// Plays a stream through a side and returns its trace.
pub fn play(side: &mut impl Side, stream: &Stream) -> String {
    for record in &stream.records {
        match record {
            Record::Data(data) => {
                let mut off = 0;
                while off < data.len() {
                    let n = side.feed(&data[off..]);
                    off += n;
                    let t = side.trace();
                    if t.paused {
                        t.paused = false;
                        let reset = t.reset_at_pause;
                        out!(t.out, "pause {off}\n");
                        if reset {
                            side.reset();
                        }
                    } else if n == 0 {
                        panic!("{}: input stopped without a pause", stream.label);
                    }
                }
            }
            Record::Reset => side.reset(),
            Record::Mode(mode) => {
                side.trace().set_mode(*mode);
                side.mode_changed();
            }
            Record::Pause { after, reset } => {
                let t = side.trace();
                t.pause_left = *after;
                t.reset_at_pause = *reset;
            }
            Record::ModeAfter { after, mode } => {
                let t = side.trace();
                t.mode_left = *after;
                t.mode_to = *mode;
            }
        }
    }
    std::mem::take(&mut side.trace().out)
}
