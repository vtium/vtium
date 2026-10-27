// Ported from libtsm's src/tsm/tsm-vte.c (the parser state machine and its
// collection handlers), which carries this notice:
//
// Copyright (c) 2018 Aetf <aetf@unlimitedcodeworks.xyz>
// Copyright (c) 2011-2013 David Herrmann <dh.herrmann@gmail.com>
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files
// (the "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be included
// in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! The state machine, with the collection of parameters, intermediates and
//! strings. Its behaviour follows the C reference's `tsm-vte.c`, cited rule
//! by rule, but for the departures the crate's README lists. Every dispatch
//! goes to the [`Consumer`].

use crate::bounds::{OSC_MAX, XTGETTCAP_MAX};

/// The OSC buffer's first allocation, which holds a title, an OSC 133 mark
/// or an OSC 8 URI with no growth; a longer string doubles it up to
/// [`OSC_MAX`].
const OSC_FLOOR: usize = 4096;

/// A buffer grown past this is released when its string ends, so a long
/// OSC 52 leaves no megabyte behind; one within it is kept for the next
/// string.
const OSC_KEEP: usize = 64 * 1024;
use crate::consumer::{Chunk, Consumer, InputMode, StringEnd};
use crate::params::{Intermediates, Params};
use crate::utf8::{self, Decoder};

/// The parser's states (tsm-vte.c:65-83).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Ground,
    Esc,
    EscInt,
    CsiEntry,
    CsiParam,
    CsiInt,
    CsiIgnore,
    DcsEntry,
    DcsParam,
    DcsInt,
    DcsPass,
    DcsIgnore,
    OscString,
    StIgnore,
    ApcString,
}

/// The parser's actions (tsm-vte.c:86-106). The C reference's two actions
/// that do nothing are one here, `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    None,
    Print,
    Execute,
    Clear,
    Collect,
    /// The private marker of a parameter string; the C reference collected
    /// it with the intermediates.
    Marker,
    Param,
    EscDispatch,
    CsiDispatch,
    DcsStart,
    DcsCollect,
    OscStart,
    OscCollect,
    ApcStart,
    ApcCollect,
    /// The exit action of every string state, DCS, OSC and APC.
    EndString,
}

/// What the DCS string in progress goes to, picked when its data starts
/// (tsm-vte.c:124-128).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dcs {
    None,
    Hooked,
    Xtgettcap,
}

/// `c` as a byte, when it is one of `range`: the bytes an action takes.
/// The state machine sends each action only characters in its range, so
/// the `None` arm is never reached, and does nothing if it is.
fn byte_in(c: char, range: core::ops::RangeInclusive<u8>) -> Option<u8> {
    u8::try_from(c).ok().filter(|b| range.contains(b))
}

/// `c` as a C0 or C1 control to execute, ESC aside.
fn control(c: char) -> Option<u8> {
    byte_in(c, 0x00..=0x1a)
        .or_else(|| byte_in(c, 0x1c..=0x1f))
        .or_else(|| byte_in(c, 0x80..=0x9f))
}

/// How the character that left a string state ended the string. Only CAN,
/// SUB, ESC and the C1 controls cancel one, so a character the parser
/// cannot name as one of those bytes ends it cancelled by CAN.
fn string_end(c: char) -> StringEnd {
    match c {
        '\u{9c}' => StringEnd::St,
        '\x07' => StringEnd::Bel,
        _ => StringEnd::Cancelled(control(c).or(byte_in(c, 0x1b..=0x1b)).unwrap_or(0x18)),
    }
}

/// The action on entering a state (tsm-vte.c:3239-3247).
fn entry_action(state: State) -> Action {
    match state {
        State::CsiEntry | State::DcsEntry | State::Esc => Action::Clear,
        State::DcsPass => Action::DcsStart,
        State::OscString => Action::OscStart,
        State::ApcString => Action::ApcStart,
        _ => Action::None,
    }
}

/// The action on leaving a state (tsm-vte.c:3250-3255).
fn exit_action(state: State) -> Action {
    match state {
        State::DcsPass | State::OscString | State::ApcString => Action::EndString,
        _ => Action::None,
    }
}

/// A byte DCS passthrough collects as it stands, in every input mode
/// (tsm-vte-image.c:365-368).
fn dcs_run_byte(b: u8) -> bool {
    b < 0x7f && b != 0x18 && b != 0x1a && b != 0x1b
}

/// A byte an APC string collects as it stands, in every input mode
/// (tsm-vte-image.c:401-404).
fn apc_run_byte(b: u8) -> bool {
    (0x20..0x7f).contains(&b)
}

/// The events that may occur in any state (tsm-vte.c:3289-3319).
fn any_state(c: u32) -> Option<(State, Action)> {
    use Action as A;
    use State as S;

    match c {
        0x18 | 0x1a | 0x80..=0x8f | 0x91..=0x97 | 0x99 | 0x9a | 0x9c => {
            Some((S::Ground, A::Execute))
        }
        0x1b => Some((S::Esc, A::None)),
        0x98 | 0x9e => Some((S::StIgnore, A::None)),
        0x9f => Some((S::ApcString, A::None)),
        0x90 => Some((S::DcsEntry, A::None)),
        0x9d => Some((S::OscString, A::None)),
        0x9b => Some((S::CsiEntry, A::None)),
        _ => None,
    }
}

/// The events that depend on the current state (tsm-vte.c:3321-3639):
/// the new state, if any, and the action. C1 controls never reach here,
/// since [`any_state`] takes them first, so their arms in the C reference's
/// cases are left out.
fn transition(state: State, c: u32) -> (Option<State>, Action) {
    use Action as A;
    use State as S;

    match state {
        S::Ground => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (None, A::Execute),
            // 0x20 to 0x7F print, DEL included (tsm-vte.c:3333), and
            // so does everything else (tsm-vte.c:3337)
            _ => (None, A::Print),
        },
        S::Esc => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (None, A::Execute),
            0x7f => (None, A::None),
            0x20..=0x2f => (Some(S::EscInt), A::Collect),
            0x30..=0x4f | 0x51..=0x57 | 0x59 | 0x5a | 0x5c | 0x60..=0x7e => {
                (Some(S::Ground), A::EscDispatch)
            }
            0x5b => (Some(S::CsiEntry), A::None),
            0x5d => (Some(S::OscString), A::None),
            0x50 => (Some(S::DcsEntry), A::None),
            0x58 | 0x5e => (Some(S::StIgnore), A::None),
            0x5f => (Some(S::ApcString), A::None),
            // A character past ASCII drops the sequence (a departure; the
            // C reference collected it as an intermediate)
            _ => (Some(S::Ground), A::None),
        },
        S::EscInt => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (None, A::Execute),
            0x20..=0x2f => (None, A::Collect),
            0x7f => (None, A::None),
            0x30..=0x7e => (Some(S::Ground), A::EscDispatch),
            _ => (Some(S::Ground), A::None),
        },
        S::CsiEntry => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (None, A::Execute),
            0x7f => (None, A::None),
            0x20..=0x2f => (Some(S::CsiInt), A::Collect),
            0x30..=0x3b => (Some(S::CsiParam), A::Param),
            0x3c..=0x3f => (Some(S::CsiParam), A::Marker),
            0x40..=0x7e => (Some(S::Ground), A::CsiDispatch),
            _ => (Some(S::CsiIgnore), A::None),
        },
        S::CsiParam => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (None, A::Execute),
            0x30..=0x3b => (None, A::Param),
            0x7f => (None, A::None),
            0x3c..=0x3f => (Some(S::CsiIgnore), A::None),
            0x20..=0x2f => (Some(S::CsiInt), A::Collect),
            0x40..=0x7e => (Some(S::Ground), A::CsiDispatch),
            _ => (Some(S::CsiIgnore), A::None),
        },
        S::CsiInt => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (None, A::Execute),
            0x20..=0x2f => (None, A::Collect),
            0x7f => (None, A::None),
            0x30..=0x3f => (Some(S::CsiIgnore), A::None),
            0x40..=0x7e => (Some(S::Ground), A::CsiDispatch),
            _ => (Some(S::CsiIgnore), A::None),
        },
        S::CsiIgnore => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f => (None, A::Execute),
            0x20..=0x3f | 0x7f => (None, A::None),
            0x40..=0x7e => (Some(S::Ground), A::None),
            _ => (None, A::None),
        },
        S::DcsEntry => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x7f => (None, A::None),
            0x3a => (Some(S::DcsIgnore), A::None),
            0x20..=0x2f => (Some(S::DcsInt), A::Collect),
            0x30..=0x39 | 0x3b => (Some(S::DcsParam), A::Param),
            0x3c..=0x3f => (Some(S::DcsParam), A::Marker),
            0x40..=0x7e => (Some(S::DcsPass), A::None),
            // A character past ASCII ignores the string, as CSI's rule does
            // (a departure; the C reference took it as the final)
            _ => (Some(S::DcsIgnore), A::None),
        },
        S::DcsParam => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x7f => (None, A::None),
            0x30..=0x39 | 0x3b => (None, A::Param),
            0x3a | 0x3c..=0x3f => (Some(S::DcsIgnore), A::None),
            0x20..=0x2f => (Some(S::DcsInt), A::Collect),
            0x40..=0x7e => (Some(S::DcsPass), A::None),
            // A character past ASCII ignores the string, as CSI's rule does
            // (a departure; the C reference took it as the final)
            _ => (Some(S::DcsIgnore), A::None),
        },
        S::DcsInt => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x7f => (None, A::None),
            0x20..=0x2f => (None, A::Collect),
            0x30..=0x3f => (Some(S::DcsIgnore), A::None),
            0x40..=0x7e => (Some(S::DcsPass), A::None),
            // A character past ASCII ignores the string, as CSI's rule does
            // (a departure; the C reference took it as the final)
            _ => (Some(S::DcsIgnore), A::None),
        },
        S::DcsPass => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x20..=0x7e => (None, A::DcsCollect),
            0x7f => (None, A::None),
            _ => (None, A::DcsCollect),
        },
        S::DcsIgnore => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x20..=0x7f => (None, A::None),
            _ => (None, A::None),
        },
        S::OscString => match c {
            0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1c..=0x1f => (None, A::None),
            // DEL is collected (tsm-vte.c:3598)
            0x20..=0x7f => (None, A::OscCollect),
            0x07 => (Some(S::Ground), A::None),
            _ => (None, A::OscCollect),
        },
        S::StIgnore => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x20..=0x7f => (None, A::None),
            _ => (None, A::None),
        },
        S::ApcString => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x7f => (None, A::None),
            0x20..=0x7e => (None, A::ApcCollect),
            _ => (None, A::ApcCollect),
        },
    }
}

/// The escape-sequence parser: bytes in, [`Consumer`] calls out.
///
/// It holds every buffer it uses inline (about 1.7 KiB) but the OSC
/// string's, which grows by doubling from [`OSC_FLOOR`] to at most
/// [`OSC_MAX`], is released after a string past [`OSC_KEEP`], and is
/// emptied as each string ends. Its `Debug` shows only the bytes its
/// buffers hold.
#[derive(Clone)]
pub struct Parser {
    state: State,
    utf8: Decoder,
    params: Params,
    intermediates: Intermediates,
    /// More than [`INTERMEDIATES_MAX`](crate::INTERMEDIATES_MAX) arrived,
    /// so the sequence dispatches nothing.
    too_many_intermediates: bool,
    /// The OSC string's bytes: no allocation until the first OSC string,
    /// and empty between strings.
    osc: Vec<u8>,
    /// A character failed to fit, so the OSC string takes no more.
    osc_full: bool,
    dcs: Dcs,
    tcap: [u8; XTGETTCAP_MAX],
    tcap_len: usize,
    /// The payload passed [`XTGETTCAP_MAX`].
    tcap_over: bool,
    /// The string state an ESC left, whose end waits for the next
    /// character: `\` makes the ESC an ST, anything else cancels the
    /// string. The C reference waited so only for XTGETTCAP.
    pending_end: Option<State>,
    /// A reset came while an APC string's end waited, so a `\` right after
    /// it is dropped (aaronsb/libtsm#53).
    drop_backslash: bool,
}

/// Bytes as an escaped string, for `Debug`.
struct Escaped<'a>(&'a [u8]);

impl core::fmt::Debug for Escaped<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "\"{}\"", self.0.escape_ascii())
    }
}

impl core::fmt::Debug for Parser {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Parser")
            .field("state", &self.state)
            .field("utf8", &self.utf8)
            .field("params", &self.params)
            .field("intermediates", &self.intermediates)
            .field("too_many_intermediates", &self.too_many_intermediates)
            .field("osc", &Escaped(&self.osc))
            .field("osc_full", &self.osc_full)
            .field("dcs", &self.dcs)
            .field(
                "tcap",
                &Escaped(self.tcap.get(..self.tcap_len).unwrap_or(&[])),
            )
            .field("tcap_over", &self.tcap_over)
            .field("pending_end", &self.pending_end)
            .field("drop_backslash", &self.drop_backslash)
            .finish()
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// A parser in ground state.
    pub const fn new() -> Self {
        Parser {
            state: State::Ground,
            utf8: Decoder::new(),
            params: Params::new(),
            intermediates: Intermediates::NONE,
            too_many_intermediates: false,
            osc: Vec::new(),
            osc_full: false,
            dcs: Dcs::None,
            tcap: [0; XTGETTCAP_MAX],
            tcap_len: 0,
            tcap_over: false,
            pending_end: None,
            drop_backslash: false,
        }
    }

    /// Back to ground state, with the UTF-8 decoder between characters,
    /// as the C reference's reset leaves it (tsm-vte.c:1043-1047). A string
    /// in progress is dropped with no end call, one waiting at the ESC of
    /// its ST included (libtsm's `test_vte_xtgettcap_esc_reset`). The
    /// consumer clears its own string state, such as a sixel picture or a
    /// kitty command in progress, when it resets the parser.
    ///
    /// After a reset between the ESC of an APC string's ST and its `\`,
    /// that `\` is dropped where the C reference printed it
    /// (aaronsb/libtsm#53, a departure).
    pub fn reset(&mut self) {
        self.utf8.reset();
        self.dcs = Dcs::None;
        self.release_osc();
        self.drop_backslash = self.pending_end == Some(State::ApcString);
        self.pending_end = None;
        self.state = State::Ground;
    }

    /// Parses `bytes`, driving `consumer`, and returns how many it
    /// consumed: all of them unless [`Consumer::paused`] stopped it
    /// (tsm-vte.c:3646-3687). Input may be cut anywhere,
    /// inside a UTF-8 character included: the actions are the same as for
    /// the input whole, but for where [`Chunk::Bytes`] runs are cut.
    pub fn advance<C: Consumer>(&mut self, consumer: &mut C, bytes: &[u8]) -> usize {
        let mut rest = bytes;
        while let Some((&b, after_one)) = rest.split_first() {
            let mode = consumer.input_mode();
            let between = mode != InputMode::Utf8 || self.utf8.idle();
            let run_of: Option<fn(u8) -> bool> = match self.state {
                State::DcsPass if between => Some(dcs_run_byte),
                State::ApcString if between => Some(apc_run_byte),
                _ => None,
            };
            match run_of.filter(|is_run| is_run(b)) {
                Some(is_run) => {
                    let n = rest.iter().take_while(|&&b| is_run(b)).count();
                    let (run, after) = rest.split_at_checked(n).unwrap_or((rest, &[]));
                    if self.state == State::DcsPass {
                        self.dcs_collect_run(consumer, run);
                    } else {
                        consumer.apc_put(Chunk::Bytes(run));
                    }
                    rest = after;
                }
                None => {
                    rest = after_one;
                    match mode {
                        InputMode::SevenBit => self.parse(consumer, char::from(b & 0x7f)),
                        InputMode::EightBit => self.parse(consumer, self.gr_as_gl(char::from(b))),
                        InputMode::Utf8 => {
                            // U+0080 to U+009F do nothing in UTF-8 mode, a
                            // departure: the C reference took them as C1
                            // controls, a way past filters that strip ESC.
                            for c in self.utf8.feed(b).into_iter().flatten() {
                                if !('\u{80}'..='\u{9f}').contains(&c) {
                                    self.parse(consumer, c);
                                }
                            }
                        }
                    }
                }
            }
            if consumer.paused() {
                break;
            }
        }
        bytes.len().saturating_sub(rest.len())
    }

    /// In 8-bit mode, a GR byte (A0-FF) inside a sequence acts as its GL
    /// byte, a departure that follows vt100.net ("In all states, these
    /// codes are treated identically to GL codes 20-7F"). In ground and in
    /// strings it stays itself, to be printed or collected.
    fn gr_as_gl(&self, c: char) -> char {
        let in_sequence = matches!(
            self.state,
            State::Esc
                | State::EscInt
                | State::CsiEntry
                | State::CsiParam
                | State::CsiInt
                | State::CsiIgnore
                | State::DcsEntry
                | State::DcsParam
                | State::DcsInt
        );
        match u8::try_from(c) {
            Ok(b @ 0xa0..=0xff) if in_sequence => char::from(b & 0x7f),
            _ => c,
        }
    }

    /// One character through the state machine (tsm-vte.c:3280-3642).
    fn parse<C: Consumer>(&mut self, consumer: &mut C, c: char) {
        // A string an ESC left ends now: `\` makes the ESC an ST, and
        // anything else cancels the string (tsm-vte.c:3282-3287 for
        // XTGETTCAP; a departure for every other string)
        if let Some(string) = self.pending_end.take() {
            let end = if c == '\\' {
                StringEnd::St
            } else {
                StringEnd::Cancelled(0x1b)
            };
            self.end_string(consumer, string, end);
        }

        // aaronsb/libtsm#53: the `\` of an APC's ST, after a reset
        if self.drop_backslash {
            self.drop_backslash = false;
            if c == '\\' {
                return;
            }
        }

        match any_state(u32::from(c)) {
            Some((state, action)) => self.trans(consumer, c, Some(state), action),
            None => {
                let (next, action) = transition(self.state, u32::from(c));
                self.trans(consumer, c, next, action);
            }
        }
    }

    /// A transition (tsm-vte.c:3258-3273): with a new state, even the
    /// current one, the exit action, the transition's action, then the
    /// entry action.
    fn trans<C: Consumer>(
        &mut self,
        consumer: &mut C,
        c: char,
        next: Option<State>,
        action: Action,
    ) {
        match next {
            Some(next) => {
                let exit = exit_action(self.state);
                if exit == Action::EndString && c == '\x1b' {
                    self.pending_end = Some(self.state);
                } else {
                    self.act(consumer, c, exit);
                }
                self.act(consumer, c, action);
                self.act(consumer, c, entry_action(next));
                self.state = next;
            }
            None => self.act(consumer, c, action),
        }
    }

    /// One action (tsm-vte.c:3173-3236). Each action that takes a byte
    /// takes only the range the state machine sends it, and does nothing
    /// with any other character.
    fn act<C: Consumer>(&mut self, consumer: &mut C, c: char, action: Action) {
        match action {
            Action::None => {}
            Action::Print => consumer.print(c),
            Action::Execute => {
                if let Some(b) = control(c) {
                    consumer.execute(b);
                }
            }
            Action::Clear => self.clear(),
            Action::Collect => {
                if let Some(b) = byte_in(c, 0x20..=0x2f)
                    && !self.intermediates.push(b)
                {
                    self.too_many_intermediates = true;
                }
            }
            Action::Marker => {
                if let Some(b) = byte_in(c, 0x3c..=0x3f) {
                    self.params.set_marker(b);
                }
            }
            Action::Param => {
                if let Some(b) = byte_in(c, 0x30..=0x3b) {
                    self.params.put(b);
                }
            }
            Action::EscDispatch if self.too_many_intermediates => {}
            Action::EscDispatch => {
                if let Some(b) = byte_in(c, 0x30..=0x7e) {
                    consumer.esc_dispatch(self.intermediates, b);
                }
            }
            Action::CsiDispatch if self.too_many_intermediates => {}
            Action::CsiDispatch => {
                if let Some(b) = byte_in(c, 0x40..=0x7e) {
                    consumer.csi_dispatch(&self.params, self.intermediates, b);
                }
            }
            Action::DcsStart => self.dcs_start(consumer, c),
            Action::DcsCollect => self.dcs_collect(consumer, c),
            Action::OscStart => {
                self.osc.clear();
                self.osc_full = false;
            }
            Action::OscCollect => self.osc_collect(c),
            Action::EndString => self.end_string(consumer, self.state, string_end(c)),
            Action::ApcStart => consumer.apc_start(),
            Action::ApcCollect => match byte_in(c, 0x00..=0x7f) {
                Some(b) => consumer.apc_put(Chunk::Bytes(&[b])),
                None => consumer.apc_put(Chunk::Char(c)),
            },
        }
    }

    /// Empties the OSC buffer once its string is handed on or dropped, so
    /// the string (an OSC 52 selection's base64 among them) does not wait
    /// for the next one, and releases it when it grew past [`OSC_KEEP`].
    fn release_osc(&mut self) {
        if self.osc.capacity() > OSC_KEEP {
            self.osc = Vec::new();
        } else {
            self.osc.clear();
        }
    }

    /// Clears the sequence collected so far (tsm-vte.c:1216-1230), but for
    /// the OSC buffer, which only
    /// an OSC's start clears, so that an OSC string waiting at an ESC keeps
    /// it.
    fn clear(&mut self) {
        self.params.clear();
        self.intermediates = Intermediates::NONE;
        self.too_many_intermediates = false;
    }

    /// Collects an OSC character (tsm-vte.c:2912-2921): as UTF-8, or
    /// nothing for a noncharacter. Once a character would pass
    /// [`OSC_MAX`], the string takes no more, so what it keeps is a prefix
    /// (a departure: the C reference kept a shorter character after it).
    fn osc_collect(&mut self, c: char) {
        if self.osc_full {
            return;
        }
        if utf8::is_noncharacter(c) {
            return;
        }
        let len = self.osc.len().saturating_add(c.len_utf8());
        if len > OSC_MAX {
            self.osc_full = true;
            return;
        }
        if len > self.osc.capacity() {
            // double, from OSC_FLOOR to at most OSC_MAX, so a string at the
            // bound reallocates the buffer a few times and not once a
            // piece; memory the system refuses truncates the string, which
            // the consumer then refuses, where a panic would end the terminal
            let want = self
                .osc
                .capacity()
                .saturating_mul(2)
                .clamp(OSC_FLOOR, OSC_MAX)
                .max(len);
            let more = want.saturating_sub(self.osc.len());
            if self.osc.try_reserve_exact(more).is_err() {
                self.osc_full = true;
                return;
            }
        }
        self.osc
            .extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
    }

    /// Starts a DCS string's data (tsm-vte.c:3128-3140). `DCS + q` with no
    /// parameter is XTGETTCAP (tsm-vte.c:3134-3135), collected here; every
    /// other DCS string goes to the consumer, which picks its handler.
    fn dcs_start<C: Consumer>(&mut self, consumer: &mut C, c: char) {
        let final_byte = byte_in(c, 0x40..=0x7e);
        match final_byte {
            // a null operation: the data goes nowhere (vt100.net)
            _ if self.too_many_intermediates => self.dcs = Dcs::None,
            None => self.dcs = Dcs::None,
            Some(b'q')
                if self.intermediates.as_bytes() == b"+"
                    && self.params.marker().is_none()
                    && self.params.count() == 1
                    && self.params.get(0).is_none() =>
            {
                self.dcs = Dcs::Xtgettcap;
                self.tcap_len = 0;
                self.tcap_over = false;
            }
            Some(b) => {
                self.dcs = Dcs::Hooked;
                consumer.dcs_hook(&self.params, self.intermediates, b);
            }
        }
    }

    /// Collects one DCS character (tsm-vte.c:3142-3154).
    fn dcs_collect<C: Consumer>(&mut self, consumer: &mut C, c: char) {
        match self.dcs {
            Dcs::Hooked => match byte_in(c, 0x00..=0x7f) {
                Some(b) => consumer.dcs_put(Chunk::Bytes(&[b])),
                None => consumer.dcs_put(Chunk::Char(c)),
            },
            // the payload is ASCII; anything else is kept as a byte no
            // name holds, so its name is refused
            Dcs::Xtgettcap => self.tcap_push(byte_in(c, 0x00..=0x7f).unwrap_or(0)),
            Dcs::None => {}
        }
    }

    /// Collects a run of [`dcs_run_byte`]s at once, as the C reference's
    /// sixel fast path takes them (tsm-vte-image.c:376-398).
    fn dcs_collect_run<C: Consumer>(&mut self, consumer: &mut C, run: &[u8]) {
        match self.dcs {
            Dcs::Hooked => consumer.dcs_put(Chunk::Bytes(run)),
            Dcs::Xtgettcap => {
                for &b in run {
                    self.tcap_push(b);
                }
            }
            Dcs::None => {}
        }
    }

    /// tsm-vte.c:3149-3152
    fn tcap_push(&mut self, b: u8) {
        match self.tcap.get_mut(self.tcap_len) {
            Some(slot) => {
                *slot = b;
                self.tcap_len = self.tcap_len.saturating_add(1);
            }
            None => self.tcap_over = true,
        }
    }

    /// Ends the string in state `string` (tsm-vte.c:3004-3015, 3157-3170).
    /// ST answers an XTGETTCAP request, and anything else drops it.
    fn end_string<C: Consumer>(&mut self, consumer: &mut C, string: State, end: StringEnd) {
        match string {
            State::OscString => {
                consumer.osc_end(&self.osc, self.osc_full, end);
                self.release_osc();
            }
            State::DcsPass => {
                match self.dcs {
                    Dcs::Hooked => consumer.dcs_unhook(end),
                    Dcs::Xtgettcap if end == StringEnd::St => {
                        let payload = self.tcap.get(..self.tcap_len).unwrap_or(&[]);
                        consumer.xtgettcap((!self.tcap_over).then_some(payload));
                    }
                    Dcs::Xtgettcap | Dcs::None => {}
                }
                self.dcs = Dcs::None;
            }
            State::ApcString => consumer.apc_end(end),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records how many times each kind of action reached the consumer.
    #[derive(Default)]
    struct Count {
        executes: Vec<u8>,
        dispatches: usize,
        hooks: usize,
        bytes: Vec<u8>,
        chars: Vec<char>,
    }

    impl Consumer for Count {
        fn print(&mut self, _: char) {}
        fn execute(&mut self, control: u8) {
            self.executes.push(control);
        }
        fn esc_dispatch(&mut self, _: Intermediates, _: u8) {
            self.dispatches = self.dispatches.saturating_add(1);
        }
        fn csi_dispatch(&mut self, _: &Params, _: Intermediates, _: u8) {
            self.dispatches = self.dispatches.saturating_add(1);
        }
        fn osc_end(&mut self, _: &[u8], _: bool, _: StringEnd) {}
        fn dcs_hook(&mut self, _: &Params, _: Intermediates, _: u8) {
            self.hooks = self.hooks.saturating_add(1);
        }
        fn dcs_put(&mut self, data: Chunk<'_>) {
            self.apc_put(data);
        }
        fn dcs_unhook(&mut self, _: StringEnd) {}
        fn apc_start(&mut self) {}
        fn apc_put(&mut self, data: Chunk<'_>) {
            match data {
                Chunk::Bytes(run) => self.bytes.extend_from_slice(run),
                Chunk::Char(c) => self.chars.push(c),
            }
        }
        fn apc_end(&mut self, _: StringEnd) {}
        fn xtgettcap(&mut self, _: Option<&[u8]>) {}
    }

    // Each action takes a character only in the range the state machine
    // sends it, so a character past that range does nothing, where a
    // truncating cast would have made it another byte: U+0107 is not BEL,
    // U+0128 not `(`, U+013F not `?`, U+0135 not `5`, U+0163 not `c`,
    // U+016D not `m` and U+0171 not `q`.
    #[test]
    fn characters_past_each_actions_range_do_nothing() {
        let mut parser = Parser::new();
        let mut count = Count::default();

        parser.act(&mut count, '\u{107}', Action::Execute);
        assert!(count.executes.is_empty(), "{:?}", count.executes);

        parser.act(&mut count, '\u{128}', Action::Collect);
        assert!(parser.intermediates.is_empty());
        parser.act(&mut count, '\u{13f}', Action::Marker);
        assert_eq!(parser.params.marker(), None);
        parser.act(&mut count, '\u{135}', Action::Param);
        assert_eq!(parser.params.get(0), None);

        parser.act(&mut count, '\u{163}', Action::EscDispatch);
        parser.act(&mut count, '\u{16d}', Action::CsiDispatch);
        assert_eq!(count.dispatches, 0);

        parser.act(&mut count, '\u{171}', Action::DcsStart);
        assert_eq!(count.hooks, 0);
        assert_eq!(parser.dcs, Dcs::None);

        parser.act(&mut count, '\u{141}', Action::ApcCollect);
        parser.dcs = Dcs::Hooked;
        parser.act(&mut count, '\u{141}', Action::DcsCollect);
        assert!(count.bytes.is_empty(), "{:?}", count.bytes);
        assert_eq!(count.chars, ['\u{141}', '\u{141}']);

        parser.dcs = Dcs::Xtgettcap;
        parser.act(&mut count, '\u{141}', Action::DcsCollect);
        assert_eq!(parser.tcap.get(..parser.tcap_len), Some(&[0][..]));
    }

    /// The buffers the OSC string has had, by address and capacity, as a
    /// stream is fed in pieces of 4 KiB.
    fn buffers(parser: &mut Parser, stream: &[u8]) -> Vec<(usize, usize)> {
        let mut count = Count::default();
        let mut seen: Vec<(usize, usize)> = Vec::new();
        for piece in stream.chunks(4096) {
            parser.advance(&mut count, piece);
            let now = (parser.osc.as_ptr() as usize, parser.osc.capacity());
            if seen.last() != Some(&now) {
                seen.push(now);
            }
        }
        seen
    }

    // A string grows the OSC buffer by doubling from OSC_FLOOR, never past
    // OSC_MAX, so a string at the bound reallocates it a few times and not
    // once a piece; a string past OSC_KEEP releases it as it ends.
    #[test]
    fn the_osc_buffer_doubles_to_the_bound_and_is_released_after_a_long_string() {
        let mut parser = Parser::new();
        assert_eq!(parser.osc.capacity(), 0, "a new parser allocates nothing");
        let mut long = b"\x1b]52;c;".to_vec();
        long.resize(OSC_MAX.saturating_mul(2), b'A');
        let seen = buffers(&mut parser, &long);
        let capacities: Vec<usize> = seen.iter().map(|&(_, c)| c).collect();
        assert_eq!(capacities.first(), Some(&OSC_FLOOR));
        assert_eq!(capacities.last(), Some(&OSC_MAX));
        assert!(capacities.len() <= 9, "{capacities:?}");
        assert!(capacities.iter().all(|&c| c <= OSC_MAX), "{capacities:?}");
        let mut count = Count::default();
        parser.advance(&mut count, b"\x07");
        assert_eq!(parser.osc.capacity(), 0, "the buffer is not released");
    }

    // A string within OSC_KEEP keeps the buffer, and the next string reuses
    // it with no allocation.
    #[test]
    fn a_short_string_keeps_the_osc_buffer() {
        let mut parser = Parser::new();
        let mut count = Count::default();
        parser.advance(&mut count, b"\x1b]0;title\x07");
        let buffer = (parser.osc.as_ptr(), parser.osc.capacity());
        let mut medium = b"\x1b]8;;".to_vec();
        medium.resize(OSC_KEEP, b'u');
        medium.push(0x07);
        parser.advance(&mut count, &medium);
        parser.advance(&mut count, b"\x1b]2;title\x07");
        assert!(parser.osc.capacity() <= OSC_KEEP);
        assert_eq!(parser.osc.as_ptr(), buffer.0, "the buffer moved");
    }

    // The string is gone from the parser once its end is handed on, so a
    // selection's base64 does not wait in memory for the next string.
    #[test]
    fn the_osc_string_is_cleared_as_it_ends() {
        let mut parser = Parser::new();
        let mut count = Count::default();
        parser.advance(&mut count, b"\x1b]52;c;aGk=\x07");
        assert!(parser.osc.is_empty());
        parser.advance(&mut count, b"\x1b]52;c;aGk=\x1b");
        assert_eq!(parser.osc, b"52;c;aGk=", "an ST waiting at its ESC");
        parser.advance(&mut count, b"\\");
        assert!(parser.osc.is_empty());
        assert!(!format!("{parser:?}").contains("aGk"));
    }

    // A string ends whatever ends it; one ended by a character the parser
    // cannot name as a byte ends cancelled by CAN, never by the byte a
    // truncating cast would make (U+011B is not ESC).
    #[test]
    fn a_string_end_past_the_byte_range_is_a_cancel() {
        assert_eq!(string_end('\u{11b}'), StringEnd::Cancelled(0x18));
        assert_eq!(string_end('\u{9c}'), StringEnd::St);
        assert_eq!(string_end('\x07'), StringEnd::Bel);
        assert_eq!(string_end('\x1b'), StringEnd::Cancelled(0x1b));
    }
}
