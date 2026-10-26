//! What the parser drives: the [`Consumer`] trait and the types its
//! methods take.

use crate::params::{Intermediates, Params};

/// How the parser turns bytes into characters, the C reference's
/// `TSM_VTE_FLAG_7BIT_MODE` and `TSM_VTE_FLAG_8BIT_MODE` (libtsm.h:881-882).
///
/// The terminal sets these modes (DECSCL sets one, RIS and DECSTR clear
/// both), so the parser asks [`Consumer::input_mode`] before each byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InputMode {
    /// Bytes are UTF-8. C1 controls arrive as U+0080 to U+009F.
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
/// fast path (`vte_sixel_run` and `vte_kitty_run`, tsm-vte-image.c:376,
/// 413). Here every character below U+0080 arrives in a [`Bytes`] run and
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

/// The actions the parser drives, one method for each kind.
///
/// Each method mirrors a handler in the C reference's tsm-vte.c, and is
/// called where that handler runs, with the same values. The arguments
/// borrow the parser's buffers or the input, and live only for the call.
///
/// A string (OSC, DCS or APC) ends when the parser leaves its state, by
/// whatever character moved it: ST, BEL for OSC, ESC, CAN, SUB or any C1
/// control. The end method gets that character as its `terminator`, and
/// for CAN, SUB and the C1 controls [`execute`](Self::execute) follows
/// with the same character. An ESC terminator is followed by whatever ESC
/// sequence comes next, `ESC \` included.
///
/// [`Parser::reset`](crate::Parser::reset) drops a string in progress
/// with no end call, as `tsm_vte_reset` does (tsm-vte.c:1032-1049): a
/// consumer that holds string state clears it alongside.
pub trait Consumer {
    /// A character to print, `ACTION_PRINT` (tsm-vte.c:3184-3187): 0x20 to
    /// 0x7F, DEL included (tsm-vte.c:3333), or a character from U+00A0 up,
    /// U+FFFD for ill-formed UTF-8. The G0 to G3 mapping is the consumer's.
    fn print(&mut self, c: char);

    /// A C0 or C1 control to execute, `do_execute` (tsm-vte.c:1108): 0x00
    /// to 0x1F other than ESC, or 0x80 to 0x9F. CAN and SUB also cancel the
    /// sequence in progress, and the parser is back in ground state.
    fn execute(&mut self, control: u8);

    /// An escape sequence, `do_esc` (tsm-vte.c:1344): `ESC`, its
    /// intermediates, then `final_byte` from 0x30 to 0x7E.
    fn esc_dispatch(&mut self, intermediates: Intermediates, final_byte: u8);

    /// A control sequence, `do_csi` (tsm-vte.c:2537): CSI, its parameters,
    /// markers and intermediates, then `final_byte` from 0x40 to 0x7E. The
    /// C reference runs no sequence but SGR when it carries sub-parameters
    /// (tsm-vte.c:2548-2551); that choice is the consumer's.
    fn csi_dispatch(&mut self, params: &Params, intermediates: Intermediates, final_byte: u8);

    /// An OSC string, `do_osc_end` (tsm-vte.c:3004-3015). `data` is its
    /// first [`OSC_MAX`](crate::OSC_MAX) bytes as UTF-8. A terminator of
    /// BEL asks for replies ended by BEL, anything else for replies ended
    /// by `ESC \` (tsm-vte.c:2996-3002).
    fn osc_dispatch(&mut self, data: &[u8], terminator: u8);

    /// A DCS string starts, `do_dcs_start` (tsm-vte.c:3128-3140): its
    /// parameters, markers and intermediates, and `final_char`, the
    /// character that ended them. That is 0x40 to 0x7E in a well-formed
    /// string, but any character from U+00A0 up ends them too
    /// (tsm-vte.c:3512). A string the parser takes as XTGETTCAP goes to
    /// [`xtgettcap`](Self::xtgettcap) instead, and a malformed one (a `:`,
    /// or a marker after a parameter) reaches no method.
    fn dcs_hook(&mut self, params: &Params, intermediates: Intermediates, final_char: char);

    /// A piece of the hooked DCS string's data, `do_dcs_collect`
    /// (tsm-vte.c:3142). The parser keeps none of it, so its bound is the
    /// consumer's.
    fn dcs_put(&mut self, data: Chunk<'_>);

    /// The hooked DCS string ends, `do_dcs_end` (tsm-vte.c:3157). The C
    /// reference shows a sixel picture for any terminator but CAN and SUB
    /// (tsm-vte-image.c:148).
    fn dcs_unhook(&mut self, terminator: u8);

    /// An APC string starts, `vte_apc_start` (tsm-vte.c:3245). The kitty
    /// graphics protocol rides on these.
    fn apc_start(&mut self);

    /// A piece of the APC string's data, `vte_apc_collect`
    /// (tsm-vte.c:3631). The parser keeps none of it, so its bound is the
    /// consumer's.
    fn apc_put(&mut self, data: Chunk<'_>);

    /// The APC string ends, `vte_apc_end` (tsm-vte.c:3253).
    fn apc_end(&mut self, terminator: u8);

    /// An XTGETTCAP request, `DCS + q names ST`, ended by ST (0x9C or
    /// `ESC \`), as `dcs_xtgettcap` receives it (tsm-vte.c:3078). `payload`
    /// is at most [`XTGETTCAP_MAX`](crate::XTGETTCAP_MAX) bytes, with each
    /// character from U+0080 up stored as 0 (tsm-vte.c:3149-3150), and
    /// `overflowed` says the payload was longer, which the C reference
    /// answers `DCS 0 + r ST` (tsm-vte.c:3087-3091). A request ended any
    /// other way is dropped with no call (tsm-vte.c:3162-3167).
    fn xtgettcap(&mut self, payload: &[u8], overflowed: bool);

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
    /// read after each byte or run of bytes it consumes, as `vte_input`
    /// stops for the image work bound, a kitty request awaiting the
    /// host's reply, or the end of a synchronized update
    /// (tsm-vte.c:3675-3677). The caller passes the rest of the input
    /// later.
    fn paused(&self) -> bool {
        false
    }
}
