//! What the parser drives: the [`Consumer`] trait and the types its
//! methods take.

use crate::params::{Intermediates, Params};

/// How the parser turns bytes into characters, the C reference's
/// 7-bit and 8-bit modes (libtsm.h:881-882).
///
/// The terminal sets these modes (DECSCL sets one, RIS and DECSTR clear
/// both), so the parser asks [`Consumer::input_mode`] before each byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InputMode {
    /// Bytes are UTF-8. U+0080 to U+009F do nothing, so C1 controls
    /// arrive only in their 7-bit form, ESC Fe.
    #[default]
    Utf8,
    /// Each byte is the character of its value: 0x80 to 0x9F are C1
    /// controls and 0xA0 to 0xFF print as U+00A0 to U+00FF.
    EightBit,
    /// Each byte loses its top bit, so 0x9B is ESC and 0xC1 is `A`.
    SevenBit,
}

/// A piece of a DCS or APC string's data, as [`Consumer::dcs_put`] and
/// [`Consumer::apc_put`] receive it.
///
/// The C reference hands its string handlers one character at a time
/// (tsm-vte.c:3565, 3631), and hands runs of plain bytes over whole on a
/// fast path for sixel and kitty data (tsm-vte-image.c:376, 413). Here every character below U+0080 arrives in a [`Bytes`] run and
/// every other character on its own as [`Char`], so a consumer sees the
/// same characters in the same order whichever way they came. Runs are
/// cut wherever the input was cut, so one string can arrive in any number
/// of pieces.
///
/// [`Bytes`]: Chunk::Bytes
/// [`Char`]: Chunk::Char
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chunk<'a> {
    /// Characters below U+0080, one byte each. A DCS string's include
    /// the C0 controls other than CAN, SUB and ESC; an APC string's are
    /// 0x20 to 0x7E only.
    Bytes(&'a [u8]),
    /// One character from U+00A0 up, U+FFFD for ill-formed UTF-8.
    Char(char),
}

/// How an OSC, DCS or APC string ended. A consumer acts on a string only
/// when it ended [`St`](Self::St) or [`Bel`](Self::Bel); a
/// [`Cancelled`](Self::Cancelled) end lets it release what it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StringEnd {
    /// ST: 0x9C, or ESC followed by `\` (ECMA-48 5.6, 8.3.143).
    St,
    /// BEL, which ends an OSC string as xterm accepts; replies to it end in
    /// BEL too.
    Bel,
    /// CAN, SUB, another C1 control, or an ESC followed by anything but
    /// `\`, which cancel a control string (vt100.net; ECMA-48 8.3.6). The
    /// byte is the one that cancelled it.
    Cancelled(u8),
}

impl StringEnd {
    /// Whether the string ended with its terminator, so the consumer acts
    /// on it.
    pub fn is_complete(self) -> bool {
        !matches!(self, StringEnd::Cancelled(_))
    }
}

/// The actions the parser drives, one method for each kind.
///
/// Each method cites the C reference's tsm-vte.c for the behaviour it
/// carries over; the departures are listed in the crate's README. The arguments
/// borrow the parser's buffers or the input, and live only for the call.
///
/// A string (OSC, DCS or APC) ends when the parser leaves its state, and
/// its end method gets a [`StringEnd`]. ST and BEL complete it. CAN, SUB
/// and the other C1 controls cancel it, and [`execute`](Self::execute)
/// follows with the same control. An ESC waits for the next character: a
/// `\` makes it ST, and anything else cancels the string and goes on as
/// the ESC sequence it starts. Acting only on complete strings is a
/// departure: the C reference acted on a string however it ended.
///
/// [`Parser::reset`](crate::Parser::reset) drops a string in progress
/// with no end call, as the C reference's reset does (tsm-vte.c:1032-1049): a
/// consumer that holds string state clears it alongside.
pub trait Consumer {
    /// A character to print (tsm-vte.c:3184-3187): 0x20 to
    /// 0x7F, DEL included (tsm-vte.c:3333), or a character from U+00A0 up,
    /// U+FFFD for ill-formed UTF-8. The G0 to G3 mapping is the consumer's.
    fn print(&mut self, c: char);

    /// A C0 or C1 control to execute (tsm-vte.c:1108): 0x00
    /// to 0x1F other than ESC, or 0x80 to 0x9F. CAN and SUB also cancel the
    /// sequence in progress, and the parser is back in ground state.
    fn execute(&mut self, control: u8);

    /// An escape sequence (tsm-vte.c:1344): `ESC`, its
    /// intermediates, then `final_byte` from 0x30 to 0x7E.
    fn esc_dispatch(&mut self, intermediates: Intermediates, final_byte: u8);

    /// A control sequence (tsm-vte.c:2537): CSI, its parameters
    /// with their private marker, its intermediates, then `final_byte` from
    /// 0x40 to 0x7E. A sequence with more than
    /// [`INTERMEDIATES_MAX`](crate::INTERMEDIATES_MAX) intermediates does
    /// not dispatch. The
    /// C reference runs no sequence but SGR when it carries sub-parameters
    /// (tsm-vte.c:2548-2551); that choice is the consumer's.
    fn csi_dispatch(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8);

    /// An OSC string ends (tsm-vte.c:3004-3015). `data` is
    /// its first bytes as UTF-8, at most [`OSC_MAX`](crate::OSC_MAX). A
    /// string ended by BEL takes replies ended by BEL, and one ended by ST
    /// replies ended by `ESC \` (tsm-vte.c:2996-3002).
    fn osc_end(&mut self, data: &[u8], end: StringEnd);

    /// A DCS string starts (tsm-vte.c:3128-3140): its
    /// parameters with their private marker, its intermediates, and
    /// `final_byte` from 0x40 to 0x7E. A string the parser takes as
    /// XTGETTCAP goes to [`xtgettcap`](Self::xtgettcap) instead, and a
    /// malformed one (a `:`, a marker after a parameter, a third
    /// intermediate, or a character past ASCII) reaches no method.
    fn dcs_hook(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8);

    /// A piece of the hooked DCS string's data (tsm-vte.c:3142). The parser keeps none of it, so its bound is the
    /// consumer's.
    fn dcs_put(&mut self, data: Chunk<'_>);

    /// The hooked DCS string ends (tsm-vte.c:3157).
    fn dcs_unhook(&mut self, end: StringEnd);

    /// An APC string starts (tsm-vte.c:3245). The kitty
    /// graphics protocol rides on these.
    fn apc_start(&mut self);

    /// A piece of the APC string's data (tsm-vte.c:3631). The parser keeps none of it, so its bound is the
    /// consumer's.
    fn apc_put(&mut self, data: Chunk<'_>);

    /// The APC string ends (tsm-vte.c:3253).
    fn apc_end(&mut self, end: StringEnd);

    /// An XTGETTCAP request, `DCS + q names ST`, ended by ST (0x9C or
    /// `ESC \`) (tsm-vte.c:3078). `payload`
    /// is the names, each character from U+0080 up stored as 0
    /// (tsm-vte.c:3149-3150), or `None` when they passed
    /// [`XTGETTCAP_MAX`](crate::XTGETTCAP_MAX) bytes, which the C reference
    /// answers `DCS 0 + r ST` (tsm-vte.c:3087-3091). A request ended any
    /// other way is dropped with no call (tsm-vte.c:3162-3167).
    fn xtgettcap(&mut self, payload: Option<&[u8]>);

    /// The mode the next byte is decoded in. Read before every byte, so a
    /// mode the consumer changes in a dispatch applies from the next byte.
    ///
    /// It is a query because the mode changes inside a dispatch (DECSCL,
    /// DECSTR, RIS), while [`Parser::advance`](crate::Parser::advance)
    /// holds the parser borrowed and the consumer has no way to reach it.
    fn input_mode(&self) -> InputMode {
        InputMode::Utf8
    }

    /// Whether [`Parser::advance`](crate::Parser::advance) returns now,
    /// read after each byte or run of bytes it consumes, as the C reference's input loop
    /// stops for the image work bound, a kitty request awaiting the
    /// host's reply, or the end of a synchronized update
    /// (tsm-vte.c:3675-3677). The caller passes the rest of the input
    /// later.
    fn paused(&self) -> bool {
        false
    }
}
