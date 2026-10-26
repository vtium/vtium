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

//! The state machine: `parse_data`, `do_trans` and `do_action` of the C
//! reference, with the collection half of each handler. Every dispatch
//! goes to the [`Consumer`].

use crate::bounds::{OSC_MAX, XTGETTCAP_MAX};
use crate::consumer::{Chunk, Consumer, InputMode};
use crate::params::{Intermediates, Params};
use crate::utf8::{self, Decoder};

/// `enum parser_state` (tsm-vte.c:65-83).
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

/// `enum parser_action` (tsm-vte.c:86-106). `ACTION_NONE` and
/// `ACTION_IGNORE` both do nothing, so `None` stands for both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    None,
    Print,
    Execute,
    Clear,
    Collect,
    Param,
    EscDispatch,
    CsiDispatch,
    DcsStart,
    DcsCollect,
    DcsEnd,
    OscStart,
    OscCollect,
    OscEnd,
    ApcStart,
    ApcCollect,
    ApcEnd,
}

/// What the DCS string in progress goes to, picked when its data starts
/// (`enum dcs_kind`, tsm-vte.c:124-128).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dcs {
    None,
    Hooked,
    Xtgettcap,
}

/// `entry_action` (tsm-vte.c:3239-3247).
fn entry_action(state: State) -> Action {
    match state {
        State::CsiEntry | State::DcsEntry | State::Esc => Action::Clear,
        State::DcsPass => Action::DcsStart,
        State::OscString => Action::OscStart,
        State::ApcString => Action::ApcStart,
        _ => Action::None,
    }
}

/// `exit_action` (tsm-vte.c:3250-3255).
fn exit_action(state: State) -> Action {
    match state {
        State::DcsPass => Action::DcsEnd,
        State::OscString => Action::OscEnd,
        State::ApcString => Action::ApcEnd,
        _ => Action::None,
    }
}

/// A byte DCS passthrough collects as it stands, in every input mode
/// (`sixel_byte`, tsm-vte-image.c:365-368).
fn dcs_run_byte(b: u8) -> bool {
    b < 0x7f && b != 0x18 && b != 0x1a && b != 0x1b
}

/// A byte an APC string collects as it stands, in every input mode
/// (`kitty_byte`, tsm-vte-image.c:400-403).
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
/// the new state, if any, and the action. The arms are the C reference's
/// cases in their order; those for C1 controls are never reached, since
/// [`any_state`] takes them first, as in the C reference.
fn transition(state: State, c: u32) -> (Option<State>, Action) {
    use Action as A;
    use State as S;

    match state {
        S::Ground => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x80..=0x8f | 0x91..=0x9a | 0x9c => {
                (None, A::Execute)
            }
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
            0x3c..=0x3f => (Some(S::CsiParam), A::Collect),
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
            0x3c..=0x3f => (Some(S::DcsParam), A::Collect),
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
            0x9c => (Some(S::Ground), A::None),
            _ => (None, A::DcsCollect),
        },
        S::DcsIgnore => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x20..=0x7f => (None, A::None),
            0x9c => (Some(S::Ground), A::None),
            _ => (None, A::None),
        },
        S::OscString => match c {
            0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1c..=0x1f => (None, A::None),
            // DEL is collected (tsm-vte.c:3598)
            0x20..=0x7f => (None, A::OscCollect),
            0x07 | 0x9c => (Some(S::Ground), A::None),
            _ => (None, A::OscCollect),
        },
        S::StIgnore => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x20..=0x7f => (None, A::None),
            0x9c => (Some(S::Ground), A::None),
            _ => (None, A::None),
        },
        S::ApcString => match c {
            0x00..=0x17 | 0x19 | 0x1c..=0x1f | 0x7f => (None, A::None),
            0x20..=0x7e => (None, A::ApcCollect),
            0x9c => (Some(S::Ground), A::None),
            _ => (None, A::ApcCollect),
        },
    }
}

/// The escape-sequence parser: bytes in, [`Consumer`] calls out.
///
/// It holds every buffer it uses inline (about 1.8 KiB) and allocates
/// nothing.
#[derive(Clone, Debug)]
pub struct Parser {
    state: State,
    utf8: Decoder,
    params: Params,
    intermediates: Intermediates,
    osc: [u8; OSC_MAX],
    osc_len: usize,
    /// A character failed to fit, so the OSC string takes no more.
    osc_full: bool,
    dcs: Dcs,
    tcap: [u8; XTGETTCAP_MAX],
    tcap_len: usize,
    /// The payload passed [`XTGETTCAP_MAX`] (`tcap_over`).
    tcap_over: bool,
    /// An ESC ended an XTGETTCAP request, which is ST only if `\` comes
    /// next (`dcs_esc`).
    tcap_esc: bool,
    /// The last character was an ESC that ended an APC string. A reset
    /// keeps it, so the `\` of that ST is dropped after the reset
    /// (aaronsb/libtsm#53).
    apc_esc: bool,
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
            osc: [0; OSC_MAX],
            osc_len: 0,
            osc_full: false,
            dcs: Dcs::None,
            tcap: [0; XTGETTCAP_MAX],
            tcap_len: 0,
            tcap_over: false,
            tcap_esc: false,
            apc_esc: false,
        }
    }

    /// Back to ground state, with the UTF-8 decoder between characters,
    /// as `tsm_vte_reset` leaves the parser (tsm-vte.c:1043-1047). A string
    /// in progress is dropped with no end call, and an XTGETTCAP request
    /// waiting for the `\` of its ST is dropped (libtsm's
    /// `test_vte_xtgettcap_esc_reset`). The consumer clears its own string
    /// state, such as a sixel picture or a kitty command in progress, when
    /// it resets the parser.
    ///
    /// One departure from the C reference: after a reset between the ESC
    /// that ended an APC string and the `\` of its ST, that `\` is dropped
    /// where the C reference printed it (aaronsb/libtsm#53). A host that
    /// cancels a kitty request it was handed at the ESC resets there.
    pub fn reset(&mut self) {
        self.utf8.reset();
        self.dcs = Dcs::None;
        self.tcap_esc = false;
        self.state = State::Ground;
    }

    /// Parses `bytes`, driving `consumer`, and returns how many it
    /// consumed: all of them unless [`Consumer::paused`] stopped it
    /// (`vte_input`, tsm-vte.c:3646-3680). Input may be cut anywhere,
    /// inside a UTF-8 character included: the actions are the same as for
    /// the input whole, but for where [`Chunk::Bytes`] runs are cut.
    pub fn advance<C: Consumer>(&mut self, consumer: &mut C, bytes: &[u8]) -> usize {
        let mut i = 0;
        while i < bytes.len() {
            let mode = consumer.input_mode();
            let between = mode != InputMode::Utf8 || self.utf8.idle();
            let rest = &bytes[i..];
            if between && self.state == State::DcsPass && dcs_run_byte(rest[0]) {
                let n = rest.iter().take_while(|&&b| dcs_run_byte(b)).count();
                self.dcs_collect_run(consumer, &rest[..n]);
                i += n;
            } else if between && self.state == State::ApcString && apc_run_byte(rest[0]) {
                let n = rest.iter().take_while(|&&b| apc_run_byte(b)).count();
                consumer.apc_put(Chunk::Bytes(&rest[..n]));
                i += n;
            } else {
                let b = rest[0];
                i += 1;
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
            if consumer.paused() {
                break;
            }
        }
        i
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
            Ok(b @ 0xa0..=0xff) if in_sequence => char::from(b - 0x80),
            _ => c,
        }
    }

    /// `parse_data` (tsm-vte.c:3280-3642): one character through the state
    /// machine.
    fn parse<C: Consumer>(&mut self, consumer: &mut C, c: char) {
        // aaronsb/libtsm#53: the `\` of an APC's ST, after a reset
        if self.apc_esc {
            self.apc_esc = false;
            if self.state == State::Ground && c == '\\' {
                return;
            }
        }

        // tsm-vte.c:3282-3287: an XTGETTCAP request that an ESC ended
        if self.tcap_esc {
            self.tcap_esc = false;
            if c == '\\' {
                consumer.xtgettcap(&self.tcap[..self.tcap_len], self.tcap_over);
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

    /// `do_trans` (tsm-vte.c:3258-3273): with a new state, even the
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
                self.act(consumer, c, exit_action(self.state));
                self.act(consumer, c, action);
                self.act(consumer, c, entry_action(next));
                self.state = next;
            }
            None => self.act(consumer, c, action),
        }
    }

    /// `do_action` (tsm-vte.c:3173-3236).
    ///
    /// The casts to `u8` hold by the arms that reach each action: controls
    /// executed are below 0xA0, final bytes are below 0x7F, and strings end
    /// only on a control below 0xA0.
    fn act<C: Consumer>(&mut self, consumer: &mut C, c: char, action: Action) {
        match action {
            Action::None => {}
            Action::Print => consumer.print(c),
            Action::Execute => consumer.execute(c as u8),
            Action::Clear => self.clear(),
            Action::Collect => self.intermediates |= Intermediates::of(u32::from(c)),
            Action::Param => self.params.put(u32::from(c)),
            Action::EscDispatch => consumer.esc_dispatch(self.intermediates, c as u8),
            Action::CsiDispatch => {
                consumer.csi_dispatch(&self.params, self.intermediates, c as u8);
            }
            Action::DcsStart => self.dcs_start(consumer, c),
            Action::DcsCollect => self.dcs_collect(consumer, c),
            Action::DcsEnd => self.dcs_end(consumer, c),
            // tsm-vte.c:3215-3217
            Action::OscStart => self.clear(),
            Action::OscCollect => self.osc_collect(c),
            Action::OscEnd => consumer.osc_dispatch(&self.osc[..self.osc_len], c as u8),
            Action::ApcStart => consumer.apc_start(),
            Action::ApcCollect => {
                if c.is_ascii() {
                    consumer.apc_put(Chunk::Bytes(&[c as u8]));
                } else {
                    consumer.apc_put(Chunk::Char(c));
                }
            }
            Action::ApcEnd => {
                self.apc_esc = c == '\x1b';
                consumer.apc_end(c as u8);
            }
        }
    }

    /// `do_clear` (tsm-vte.c:1216-1230).
    fn clear(&mut self) {
        self.params.clear();
        self.intermediates = Intermediates::NONE;
        self.osc_len = 0;
        self.osc_full = false;
    }

    /// `do_osc_collect` (tsm-vte.c:2912-2921): the character as UTF-8, or
    /// nothing when it has no encoding. Once a character would pass
    /// [`OSC_MAX`], the string takes no more, so what it keeps is a prefix
    /// (a departure: the C reference kept a shorter character after it).
    fn osc_collect(&mut self, c: char) {
        if self.osc_full {
            return;
        }
        let mut buf = [0; 4];
        let len = utf8::encode(u32::from(c), &mut buf);
        if self.osc_len + len > OSC_MAX {
            self.osc_full = true;
            return;
        }
        self.osc[self.osc_len..self.osc_len + len].copy_from_slice(&buf[..len]);
        self.osc_len += len;
    }

    /// `do_dcs_start` (tsm-vte.c:3128-3140). `DCS + q` with no parameter
    /// is XTGETTCAP (tsm-vte.c:3134-3135), collected here; every other DCS
    /// string goes to the consumer, which picks its handler as
    /// `do_dcs_start` does.
    fn dcs_start<C: Consumer>(&mut self, consumer: &mut C, c: char) {
        if c == 'q'
            && self.intermediates == Intermediates::PLUS
            && self.params.count() == 1
            && self.params.get(0) < 0
        {
            self.dcs = Dcs::Xtgettcap;
            self.tcap_len = 0;
            self.tcap_over = false;
        } else {
            self.dcs = Dcs::Hooked;
            consumer.dcs_hook(&self.params, self.intermediates, c);
        }
    }

    /// `do_dcs_collect` (tsm-vte.c:3142-3154), one character.
    fn dcs_collect<C: Consumer>(&mut self, consumer: &mut C, c: char) {
        match self.dcs {
            Dcs::Hooked if c.is_ascii() => consumer.dcs_put(Chunk::Bytes(&[c as u8])),
            Dcs::Hooked => consumer.dcs_put(Chunk::Char(c)),
            // the payload is ASCII; anything else is kept as a byte no
            // name holds, so its name is refused
            Dcs::Xtgettcap => self.tcap_push(if c.is_ascii() { c as u8 } else { 0 }),
            Dcs::None => {}
        }
    }

    /// `do_dcs_collect` for a run of [`dcs_run_byte`]s, as `vte_sixel_run`
    /// takes them (tsm-vte-image.c:376-397).
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
        if self.tcap_len < XTGETTCAP_MAX {
            self.tcap[self.tcap_len] = b;
            self.tcap_len += 1;
        } else {
            self.tcap_over = true;
        }
    }

    /// `do_dcs_end` (tsm-vte.c:3157-3170). ST answers an XTGETTCAP request,
    /// ESC answers it if `\` follows, and CAN, SUB or another C1 control
    /// drops it.
    fn dcs_end<C: Consumer>(&mut self, consumer: &mut C, c: char) {
        match self.dcs {
            Dcs::Hooked => consumer.dcs_unhook(c as u8),
            Dcs::Xtgettcap if c == '\u{9c}' => {
                consumer.xtgettcap(&self.tcap[..self.tcap_len], self.tcap_over);
            }
            Dcs::Xtgettcap if c == '\x1b' => self.tcap_esc = true,
            Dcs::Xtgettcap | Dcs::None => {}
        }
        self.dcs = Dcs::None;
    }
}
