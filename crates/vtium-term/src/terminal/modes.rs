//! The mode table: one row per ANSI mode (SM, RM: `CSI Pm h`, `CSI Pm l`)
//! and DEC private mode (DECSET, DECRST: `CSI ? Pm h`, `CSI ? Pm l`) vtium
//! recognises.
//!
//! The table is the single source for SM and RM, DECRQM's reports, what
//! DECSTR and RIS restore, and `docs/modes.md` and `docs/modes.tsv`, which
//! a test generates from it and compares with the committed files
//! (`VTIUM_TERM_BLESS=1` rewrites them). A number with no row is not
//! recognised: DECRQM reports 0 and SM and RM change nothing.
//!
//! A supported row's state is read and written only through the two
//! accessors here, [`State::mode`] and [`State::set_mode`]. The state the
//! terminal holds itself is in [`Modes`], whose fields only this module
//! reaches; the state the screen holds is behind the screen's setters,
//! which a source check keeps to this module.

use std::collections::BTreeMap;

use super::{CursorStyle, State};
use crate::screen::{Erase, WidthMethod};

/// A row's status, as ADR-1000 names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    /// Set and reset do what the specification says.
    Supported,
    /// Accepted with no effect; the spec column says why.
    Ignored,
    /// Deliberately not built; the spec column says why.
    Refused,
}

/// The state a row starts in, and the one an ignored or refused row
/// reports as permanent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Initial {
    Set,
    Reset,
    /// A host setting, named here.
    Host(&'static str),
}

/// Which resets return a row to its initial state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reset {
    /// DECSTR and RIS.
    Soft,
    /// RIS only.
    Hard,
    /// Neither.
    Never,
}

/// What the program that embeds vtium-term must do for the mode to work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Host {
    Nothing,
    /// It reports pointer or focus events, NumLock with each key, or wheel
    /// steps it does not report, or stops repeating keys.
    Input,
    /// It pastes through [`Terminal::paste`](crate::Terminal::paste).
    Paste,
    /// It presents frames by the mode; until it says it does
    /// ([`Terminal::set_sync_enabled`](crate::Terminal::set_sync_enabled)),
    /// SM does nothing and DECRQM reports the row's fixed state.
    Draw,
    /// It tells the terminal of a resize or a change of visibility
    /// ([`Terminal::resize`](crate::Terminal::resize),
    /// [`Terminal::set_visible`](crate::Terminal::set_visible)).
    Notify,
    /// It rings the bell by its setting when it takes one
    /// ([`Terminal::take_bell`](crate::Terminal::take_bell)).
    Bell,
    /// It offers sixel: the row exists only in a build with images, and
    /// until the host offers sixel
    /// ([`Terminal::set_image_switches`](crate::Terminal::set_image_switches),
    /// an installed handler and a budget above 0), DECRQM reports the
    /// row's fixed state while SM and RM still store it (ADR-1100, D10).
    Sixel,
}

/// The accessors of a supported row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Mode {
    Irm,
    Srm,
    Lnm,
    Decckm,
    Decscnm,
    Decom,
    Decawm,
    Decarm,
    X10Mouse,
    CursorBlink,
    Dectcem,
    ReverseWrap,
    AltScreen,
    Decnkm,
    /// DECLRMM: DECSLRM may set left and right margins.
    Lrmm,
    NormalMouse,
    ButtonMouse,
    AnyMouse,
    Focus,
    SgrMouse,
    AlternateScroll,
    UrxvtMouse,
    PixelMouse,
    EightBitMeta,
    NumLock,
    MetaEscape,
    Bell,
    AltScreenClear,
    SaveCursor,
    AltScreenSaveCursor,
    BracketedPaste,
    Sync,
    GraphemeWidth,
    Reflow,
    SchemeReport,
    VisibilityReport,
    ResizeReport,
    SixelDisplay,
    SixelPrivateRegisters,
    SixelCursorRight,
    /// BDSM: set, the cursor's paragraph is in implicit mode.
    #[cfg(feature = "bidi")]
    Bdsm,
    /// Mode 1243: the arrow keys follow the paragraph's direction.
    #[cfg(feature = "bidi")]
    ArrowDirection,
    /// Mode 2500: box drawing mirrored in a right-to-left paragraph.
    #[cfg(feature = "bidi")]
    BoxMirror,
    /// Mode 2501: the paragraph's direction detected.
    #[cfg(feature = "bidi")]
    DetectDirection,
}

/// One mode.
#[derive(Debug)]
#[cfg_attr(
    not(test),
    allow(dead_code, reason = "the documents' columns, read by their test")
)]
pub(crate) struct Row {
    /// A DEC private mode (`?`), or an ANSI mode.
    pub(crate) dec: bool,
    pub(crate) number: u32,
    pub(crate) name: &'static str,
    pub(crate) status: Status,
    pub(crate) initial: Initial,
    pub(crate) reset: Reset,
    pub(crate) host: Host,
    /// The terminfo capabilities whose strings set or reset the mode.
    pub(crate) terminfo: &'static str,
    pub(crate) spec: &'static str,
    /// The tests that set and reset the mode, `file::function` under the
    /// crate's `tests/`.
    pub(crate) tests: &'static [&'static str],
    /// The accessors, for a supported row.
    pub(crate) mode: Option<Mode>,
}

#[cfg_attr(not(test), allow(dead_code, reason = "read by the documents' test"))]
impl Row {
    /// ADR-1000's id for the row.
    pub(crate) fn id(&self) -> String {
        format!("{}-{}", if self.dec { "DEC" } else { "ANSI" }, self.number)
    }
}

const ANSI: bool = false;
const DEC: bool = true;
const SET: Initial = Initial::Set;
const RESET: Initial = Initial::Reset;
const FIXED: &[&str] =
    &["modes.rs::ignored_and_refused_modes_answer_their_fixed_state_and_change_nothing"];
const ALWAYS: &[&str] =
    &["modes.rs::modes_vtium_always_or_never_has_answer_their_fixed_state_and_change_nothing"];
const MOUSE: &[&str] = &["modes.rs::mouse_tracking_reports_by_its_mode"];
const SGR_MOUSE: &[&str] = &["modes.rs::sgr_mouse_encodings_report_cells_or_pixels"];
const SIXEL_MODES: &[&str] = &[
    "sixel.rs::the_sixel_modes_are_offered_while_sixel_is",
    "sixel.rs::decsdm_places_a_picture_at_the_top_left_and_leaves_the_cursor",
    "sixel.rs::mode_1070_gives_each_picture_its_own_registers",
    "sixel.rs::mode_8452_ends_the_cursor_right_of_the_picture",
];

macro_rules! row {
    ($dec:expr, $n:expr, $name:expr, $status:ident, $initial:expr, $reset:ident, $host:ident,
     $terminfo:expr, $spec:expr, $tests:expr, $mode:expr) => {
        Row {
            dec: $dec,
            number: $n,
            name: $name,
            status: Status::$status,
            initial: $initial,
            reset: Reset::$reset,
            host: Host::$host,
            terminfo: $terminfo,
            spec: $spec,
            tests: $tests,
            mode: $mode,
        }
    };
}

/// The test of DECSTR and RIS that each supported bidi row names.
#[cfg(any(test, feature = "bidi"))]
const BIDI_RESETS: &str = "bidi.rs::decstr_and_ris_return_the_paragraph_to_its_defaults";

/// The accessors of the bidi rows: none in a build without the bidi
/// feature, whose rows exist only for the documents.
#[cfg(feature = "bidi")]
mod bidi_modes {
    use super::Mode;
    pub(super) const BDSM: Option<Mode> = Some(Mode::Bdsm);
    pub(super) const ARROWS: Option<Mode> = Some(Mode::ArrowDirection);
    pub(super) const BOX_MIRROR: Option<Mode> = Some(Mode::BoxMirror);
    pub(super) const DETECT: Option<Mode> = Some(Mode::DetectDirection);
}
#[cfg(all(test, not(feature = "bidi")))]
mod bidi_modes {
    pub(super) const BDSM: Option<super::Mode> = None;
    pub(super) const ARROWS: Option<super::Mode> = None;
    pub(super) const BOX_MIRROR: Option<super::Mode> = None;
    pub(super) const DETECT: Option<super::Mode> = None;
}

/// The bidi modes (ADR-1004 D12, D13), in the table for its documents in
/// every build that tests them, and recognised only in a build with the
/// bidi feature, so a build without it carries none of them.
#[cfg(any(test, feature = "bidi"))]
#[rustfmt::skip]
static BIDI_MODES: &[Row] = &[
    row!(ANSI, 8, "BDSM", Supported, SET, Soft, Nothing, "", "ECMA-48 §7.2.1, the Terminal Working Group's BiDi recommendation: set, the cursor's paragraph (the rows a wrap joins) is in implicit mode, reset in explicit mode; SM and RM set the current value, which each new paragraph takes, and mark the cursor's paragraph when the cursor stands at its first position; in a build with the bidi feature, and a build without it has no such row, so DECRQM answers 0", &["bidi.rs::bdsm_marks_the_paragraph_at_its_first_position_and_only_new_ones_elsewhere", BIDI_RESETS], bidi_modes::BDSM),
    row!(ANSI, 9, "DCSM", Ignored, SET, Never, Nothing, "", "ECMA-48 §7.2.3; permanently set: editing always acts on the data, the order the program wrote; in a build with the bidi feature, and a build without it has no such row, so DECRQM answers 0", &["bidi.rs::dcsm_answers_permanently_set_and_changes_nothing"], None),
    row!(DEC, 1243, "BIDIARROWS", Supported, SET, Soft, Nothing, "", "the Terminal Working Group's BiDi recommendation, as mintty documents it: set, the Left and Right arrow keys swap while the cursor's paragraph resolves right to left; in a build with the bidi feature, and a build without it has no such row, so DECRQM answers 0", &["bidi.rs::mode_1243_is_set_by_default_and_toggles", BIDI_RESETS], bidi_modes::ARROWS),
    row!(DEC, 2500, "BIDIBOX", Supported, RESET, Soft, Nothing, "", "the Terminal Working Group's BiDi recommendation, as mintty documents it: set, box drawing in a right-to-left paragraph draws mirrored; SM and RM set the current value, which each new paragraph takes, and mark the cursor's paragraph when the cursor stands at its first position; in a build with the bidi feature, and a build without it has no such row, so DECRQM answers 0", &["bidi.rs::mode_2500_marks_box_drawing_mirrored_on_the_cursors_paragraph", BIDI_RESETS], bidi_modes::BOX_MIRROR),
    row!(DEC, 2501, "BIDIAUTO", Supported, Initial::Host("text-direction"), Soft, Nothing, "", "the Terminal Working Group's BiDi recommendation, as mintty documents it: set, each paragraph's direction is detected from its first strong character; SM and RM set the current value, which each new paragraph takes, and mark the cursor's paragraph when the cursor stands at its first position, and DECSTR and RIS return it to vtium.conf's `text-direction`, set under `auto`; in a build with the bidi feature, and a build without it has no such row, so DECRQM answers 0", &["bidi.rs::mode_2501_marks_detection_and_starts_as_text_direction_says", BIDI_RESETS], bidi_modes::DETECT),
];

/// Every mode vtium recognises, ANSI modes first, each kind by number,
/// but the bidi modes, which are in `BIDI_MODES`.
#[rustfmt::skip]
pub(crate) static MODES: &[Row] = &[
    row!(ANSI, 2, "KAM", Refused, RESET, Never, Nothing, "", "ECMA-48 §7.2.11; refused: a program could lock the keyboard of the only terminal on the console", FIXED, None),
    row!(ANSI, 4, "IRM", Supported, RESET, Soft, Nothing, "", "ECMA-48 §7.2.10", &["modes.rs::irm_inserts_printed_text"], Some(Mode::Irm)),
    row!(ANSI, 12, "SRM", Supported, SET, Soft, Nothing, "", "ECMA-48 §7.2.15; reset, what the terminal sends the program is also shown (local echo)", &["modes.rs::srm_reset_echoes_what_the_terminal_sends"], Some(Mode::Srm)),
    row!(ANSI, 20, "LNM", Supported, RESET, Soft, Nothing, "", "ctlseqs; VT510; kept though ECMA-48's 5th edition withdraws it", &["modes.rs::lnm_makes_a_line_feed_a_new_line"], Some(Mode::Lnm)),
    row!(DEC, 1, "DECCKM", Supported, RESET, Soft, Nothing, "", "ctlseqs; VT510", &["modes.rs::decckm_switches_the_cursor_keys"], Some(Mode::Decckm)),
    row!(DEC, 2, "DECANM", Ignored, SET, Never, Nothing, "", "ctlseqs; VT510; VT52 is declined for now ([the VT52 issue](https://github.com/vtium/vtium/issues/65)), so DECRQM reports ANSI mode as permanent", FIXED, None),
    row!(DEC, 3, "DECCOLM", Refused, RESET, Never, Nothing, "", "ctlseqs; VT510; refused: the console's size comes from the display and the font", FIXED, None),
    row!(DEC, 4, "DECSCLM", Ignored, RESET, Never, Nothing, "", "ctlseqs; VT510; a console has no smooth scroll", FIXED, None),
    row!(DEC, 5, "DECSCNM", Supported, RESET, Soft, Nothing, "flash", "ctlseqs; VT510", &["modes.rs::decscnm_reverses_the_screen"], Some(Mode::Decscnm)),
    row!(DEC, 6, "DECOM", Supported, RESET, Soft, Nothing, "", "ctlseqs; VT510; the cursor goes home as it is set or reset (VT100 User Guide)", &["modes.rs::decom_addresses_from_the_region"], Some(Mode::Decom)),
    row!(DEC, 7, "DECAWM", Supported, SET, Soft, Nothing, "", "ctlseqs; VT510", &["modes.rs::decawm_wraps_at_the_last_column"], Some(Mode::Decawm)),
    row!(DEC, 8, "DECARM", Supported, SET, Soft, Input, "", "ctlseqs; VT510; the host repeats a held key, and stops while it is reset", &["modes.rs::decarm_tells_the_host_to_repeat_keys"], Some(Mode::Decarm)),
    row!(DEC, 9, "X10MOUSE", Supported, RESET, Soft, Input, "", "ctlseqs, X10 mouse; DECSTR resets it, where xterm resets it on RIS only", MOUSE, Some(Mode::X10Mouse)),
    row!(DEC, 12, "CURBLINK", Supported, Initial::Host("the host's cursor blink"), Soft, Nothing, "", "ctlseqs, AT&T 610 cursor blink; held in a DECSCUSR style, while the default style blinks as the host's setting says", &["modes.rs::mode_12_blinks_a_chosen_cursor_style"], Some(Mode::CursorBlink)),
    row!(DEC, 18, "DECPFF", Ignored, RESET, Never, Nothing, "", "ctlseqs; VT510; a console has no printer", FIXED, None),
    row!(DEC, 19, "DECPEX", Ignored, RESET, Never, Nothing, "", "ctlseqs; VT510; a console has no printer", FIXED, None),
    row!(DEC, 25, "DECTCEM", Supported, SET, Soft, Nothing, "civis cnorm", "ctlseqs; VT510", &["modes.rs::dectcem_shows_and_hides_the_cursor"], Some(Mode::Dectcem)),
    row!(DEC, 42, "DECNRCM", Refused, RESET, Never, Nothing, "", "ctlseqs; VT510; refused: national replacement sets are obsolete next to UTF-8", FIXED, None),
    row!(DEC, 40, "ALLOW80TO132", Ignored, RESET, Never, Nothing, "", "ctlseqs; DECCOLM is refused, so 80 to 132 columns is never allowed", ALWAYS, None),
    row!(DEC, 45, "XTREVWRAP", Supported, RESET, Soft, Nothing, "", "ctlseqs, reverse wraparound: while DECAWM is set, BS and CUB go on from the first column to the last column of the row above when printing wrapped from that row, as xterm moves; a pending wrap takes one step, so CUB n from it ends n - 1 columns left of the last column, where xterm's CursorBack counts the pending wrap twice for n of 2 or more and ends n - 2 columns left", &["modes.rs::mode_45_wraps_backwards_into_the_row_that_wrapped"], Some(Mode::ReverseWrap)),
    row!(DEC, 47, "ALTBUF", Supported, RESET, Soft, Nothing, "", "ctlseqs, the alternate screen, neither cleared nor the cursor saved", &["modes.rs::mode_47_switches_screens_only"], Some(Mode::AltScreen)),
    row!(DEC, 66, "DECNKM", Supported, RESET, Soft, Nothing, "", "ctlseqs; VT510; the keypad's application mode, which DECKPAM sets and DECKPNM resets", &["modes.rs::decnkm_switches_the_keypad_as_deckpam_and_deckpnm_do"], Some(Mode::Decnkm)),
    row!(DEC, 69, "DECLRMM", Supported, RESET, Soft, Nothing, "", "ctlseqs; VT510; set, `CSI Pl ; Pr s` is DECSLRM and sets the left and right margins, and `CSI s` with no parameter sets them to the whole width; reset, the margins are the whole width and `CSI s` is SCOSC", &["margins.rs::mode_69_makes_csi_s_set_margins_and_resetting_it_clears_them"], Some(Mode::Lrmm)),
    row!(DEC, 80, "DECSDM", Supported, RESET, Hard, Sixel, "", "ctlseqs, DECSDM in xterm's sense: set, sixel scrolling is off and a picture sits at the screen's top left, cut to the screen, with the cursor left where it is; reset, a picture's top left is the cursor's cell", SIXEL_MODES, Some(Mode::SixelDisplay)),
    row!(DEC, 1000, "VT200MOUSE", Supported, RESET, Soft, Input, "", "ctlseqs, normal tracking; DECSTR resets it, where xterm resets it on RIS only", MOUSE, Some(Mode::NormalMouse)),
    row!(DEC, 1001, "HILITEMOUSE", Refused, RESET, Never, Nothing, "", "ctlseqs; refused: the terminal waits on the program's XTHIMOUSE answer before it can show a selection", FIXED, None),
    row!(DEC, 1002, "BTNMOUSE", Supported, RESET, Soft, Input, "", "ctlseqs, button-event tracking; DECSTR resets it, where xterm resets it on RIS only", MOUSE, Some(Mode::ButtonMouse)),
    row!(DEC, 1003, "ANYMOUSE", Supported, RESET, Soft, Input, "", "ctlseqs, any-event tracking; DECSTR resets it, where xterm resets it on RIS only", MOUSE, Some(Mode::AnyMouse)),
    row!(DEC, 1004, "FOCUS", Supported, RESET, Soft, Input, "fe fd", "ctlseqs, FocusIn and FocusOut; DECSTR resets it, where xterm resets it on RIS only", &["modes.rs::focus_events_are_reported_while_set"], Some(Mode::Focus)),
    row!(DEC, 1006, "SGRMOUSE", Supported, RESET, Soft, Input, "XM", "ctlseqs, SGR encoding, whatever order the modes were set in; DECSTR resets it, where xterm resets it on RIS only", SGR_MOUSE, Some(Mode::SgrMouse)),
    row!(DEC, 1005, "UTF8MOUSE", Ignored, RESET, Never, Nothing, "", "ctlseqs; never on: SGR mouse (1006) replaces the UTF-8 encoding", ALWAYS, None),
    row!(DEC, 1007, "ALTSCROLL", Supported, SET, Soft, Input, "", "ctlseqs, alternate scroll: on the alternate screen, while no tracking mode is set, a wheel step the host does not report is sent as cursor-up or cursor-down keys, one a line, as DECCKM encodes them; set at start, as foot and Ghostty start it", &["modes.rs::mode_1007_sends_wheel_steps_as_cursor_keys_on_the_alternate_screen"], Some(Mode::AlternateScroll)),
    row!(DEC, 1015, "URXVTMOUSE", Supported, RESET, Soft, Input, "", "ctlseqs, urxvt encoding: CSI Cb ; Cx ; Cy M in decimal, Cb as the X10 encoding's byte and the cell counted from 1, with no limit; one encoding at a time, as 1006; DECSTR resets it, where xterm resets it on RIS only", &["modes.rs::urxvt_mouse_encoding_reports_in_decimal"], Some(Mode::UrxvtMouse)),
    row!(DEC, 1016, "PIXMOUSE", Supported, RESET, Soft, Input, "", "ctlseqs, SGR-Pixels encoding, pixels counted from 1; DECSTR resets it, where xterm resets it on RIS only", SGR_MOUSE, Some(Mode::PixelMouse)),
    row!(DEC, 1010, "SCROLLTTYOUTPUT", Ignored, RESET, Never, Nothing, "", "ctlseqs, rxvt; never on: the view stays where it is while output arrives", ALWAYS, None),
    row!(DEC, 1011, "SCROLLKEY", Ignored, SET, Never, Nothing, "", "ctlseqs, rxvt; always on: a key press scrolls the view back to the bottom", ALWAYS, None),
    row!(DEC, 1034, "META8BIT", Supported, SET, Soft, Nothing, "", "ctlseqs, eightBitInput: while 1036 is reset, Alt sets the eighth bit of a key that sends one byte below 0x80, sent as U+0080 to U+00FF in the input's encoding; neither set, Alt sends nothing of its own", &["modes.rs::modes_1034_and_1036_choose_what_alt_sends"], Some(Mode::EightBitMeta)),
    row!(DEC, 1035, "NUMLOCK", Supported, SET, Soft, Input, "", "ctlseqs, numLock: while NumLock is on, a keypad key that makes a character sends it whatever DECKPAM says; set at start, as xterm's numLock resource; vtium has one Alt modifier, so the mode changes only how NumLock reads", &["modes.rs::mode_1035_lets_numlock_override_the_application_keypad"], Some(Mode::NumLock)),
    row!(DEC, 1036, "METAESC", Supported, SET, Soft, Nothing, "", "ctlseqs, metaSendsEscape: Alt puts ESC before a key's bytes; set at start, so Alt with a key works as Meta in shells and editors without the program asking", &["modes.rs::modes_1034_and_1036_choose_what_alt_sends"], Some(Mode::MetaEscape)),
    row!(DEC, 1042, "BELL", Supported, SET, Soft, Bell, "", "ctlseqs, bellIsUrgent; vtium has no window to mark urgent, so set lets BEL reach the host, which rings the VT bell by its bell setting, and reset BEL does nothing; set at start, so a program's BEL rings the VT bell without the program asking", &["modes.rs::mode_1042_lets_bel_reach_the_host"], Some(Mode::Bell)),
    row!(DEC, 1046, "TITEINHIBIT", Ignored, SET, Never, Nothing, "", "ctlseqs; always on: the alternate screen is always allowed", ALWAYS, None),
    row!(DEC, 1047, "ALTBUF1047", Supported, RESET, Soft, Nothing, "", "ctlseqs, the alternate screen, cleared as it is left", &["modes.rs::mode_1047_clears_the_alternate_screen_as_it_leaves"], Some(Mode::AltScreenClear)),
    row!(DEC, 1048, "SAVECUR", Supported, RESET, Never, Nothing, "", "ctlseqs, \"Save cursor as in DECSC\": set saves, reset restores as DECRC does, and nothing stays set", &["modes.rs::mode_1048_saves_and_restores_the_cursor_as_decsc_does"], Some(Mode::SaveCursor)),
    row!(DEC, 1049, "ALTBUF1049", Supported, RESET, Soft, Nothing, "smcup rmcup", "ctlseqs, the cursor saved as in DECSC and the alternate screen cleared as it is entered; leaving restores the cursor and the pen as DECRC does", &["modes.rs::mode_1049_saves_the_cursor_and_the_pen_around_the_alternate_screen"], Some(Mode::AltScreenSaveCursor)),
    row!(DEC, 1070, "SIXELPRIVATE", Supported, Initial::Host("private-color-registers"), Hard, Sixel, "", "ctlseqs, private sixel color registers: set, each picture starts from the default registers; reset, the pictures share one set of registers, kept until RIS or a change of the register count", SIXEL_MODES, Some(Mode::SixelPrivateRegisters)),
    row!(DEC, 2004, "BRPASTE", Supported, RESET, Soft, Paste, "BD BE PS PE", "ctlseqs, bracketed paste; a paste never carries its own end marker", &["modes.rs::bracketed_paste_brackets_a_paste"], Some(Mode::BracketedPaste)),
    row!(DEC, 2026, "SYNC", Supported, RESET, Soft, Draw, "Sync", "contour synchronized-output; offered while the host presents by it", &["modes.rs::mode_2026_holds_frames_while_the_host_offers_it"], Some(Mode::Sync)),
    row!(DEC, 2027, "GRAPHEME", Supported, Initial::Host("the host's width method"), Soft, Nothing, "", "contour unicode-core; RIS and DECSTR return it to the configured method", &["modes.rs::mode_2027_reports_and_resets_to_the_configured_method"], Some(Mode::GraphemeWidth)),
    row!(DEC, 2028, "REFLOW", Supported, SET, Hard, Nothing, "", "contour line-reflow-mode; each print, and SM and RM on the cursor's row, mark a row as rewrapping at a width change or, reset, cut at a narrower margin; RIS sets it", &["reflow.rs::mode_2028_marks_the_rows_a_resize_cuts"], Some(Mode::Reflow)),
    row!(DEC, 2031, "SCHEMEREPORT", Supported, RESET, Soft, Nothing, "", "contour color-palette-update-notifications; CSI ? 997 ; Ps n when the default background turns dark or light by its luma (ITU-R BT.601), as CSI ? 996 n answers", &["inband.rs::mode_2031_reports_a_change_of_scheme", "inband.rs::decstr_and_ris_reset_the_report_modes"], Some(Mode::SchemeReport)),
    row!(DEC, 2033, "VISREPORT", Supported, RESET, Soft, Notify, "", "foot(7), rockorager's visibility reports; CSI ? 999 ; Ps n as it is set and on each change, a VT switch or a blank", &["inband.rs::mode_2033_reports_visibility_on_set_and_each_change", "inband.rs::decstr_and_ris_reset_the_report_modes"], Some(Mode::VisibilityReport)),
    row!(DEC, 2048, "RESIZEREPORT", Supported, RESET, Soft, Notify, "", "rockorager's in-band resize notifications; CSI 48 ; rows ; cols ; height ; width t as it is set and after each resize, the text area in pixels 0 while the cell size is unknown", &["inband.rs::mode_2048_reports_the_text_area_on_set_and_each_resize", "inband.rs::decstr_and_ris_reset_the_report_modes"], Some(Mode::ResizeReport)),
    row!(DEC, 8452, "SIXELRIGHT", Supported, RESET, Hard, Sixel, "", "ctlseqs, sixel cursor to the right: set, the cursor ends a picture in the column after its right edge, at most the last column; reset, at its left column", SIXEL_MODES, Some(Mode::SixelCursorRight)),
];

/// The rows of the table this build compiles in: [`MODES`], and in a
/// build with the bidi feature the bidi rows (ADR-1004 D13).
fn rows() -> impl Iterator<Item = &'static Row> {
    let rows = MODES.iter();
    #[cfg(feature = "bidi")]
    let rows = rows.chain(BIDI_MODES);
    rows
}

/// The row of a mode number, in this build: the sixel rows are in the
/// table for its documents in every build, and only a build with images
/// has them (ADR-1100, D10).
fn find(dec: bool, number: u32) -> Option<&'static Row> {
    rows().find(|r| r.dec == dec && r.number == number && r.built())
}

impl Row {
    /// Whether this build has the row.
    fn built(&self) -> bool {
        self.host != Host::Sixel || cfg!(feature = "images")
    }
}

/// Mouse tracking: which pointer events are reported (modes 9, 1000,
/// 1002, 1003, one at a time).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseTracking {
    #[default]
    Off,
    /// Mode 9: presses, in the X10 encoding with no modifiers.
    X10,
    /// Mode 1000: presses and releases.
    Normal,
    /// Mode 1002: presses, releases and motion with a button held.
    Button,
    /// Mode 1003: every event, motion included.
    Any,
}

/// How a pointer event is encoded (modes 1006, 1015 and 1016, one at a
/// time); with none of them, the X10 encoding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum MouseEncoding {
    #[default]
    X10,
    Sgr,
    Urxvt,
    Pixels,
}

/// The modes whose state the terminal holds: only this module writes
/// them, through [`State::set_mode`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Modes {
    srm: bool,
    lnm: bool,
    decckm: bool,
    decarm: bool,
    reverse_wrap: bool,
    keypad: bool,
    tracking: MouseTracking,
    encoding: MouseEncoding,
    focus: bool,
    alternate_scroll: bool,
    eight_bit_meta: bool,
    num_lock: bool,
    meta_escape: bool,
    bell: bool,
    bracketed_paste: bool,
    sync: bool,
    scheme_report: bool,
    visibility_report: bool,
    resize_report: bool,
    sixel_display: bool,
    sixel_private_registers: bool,
    sixel_cursor_right: bool,
    /// Mode 1243.
    #[cfg(feature = "bidi")]
    arrow_direction: bool,
}

impl Modes {
    /// Each mode at its row's initial state.
    pub(super) const INITIAL: Self = Self {
        srm: true,
        lnm: false,
        decckm: false,
        decarm: true,
        reverse_wrap: false,
        keypad: false,
        tracking: MouseTracking::Off,
        encoding: MouseEncoding::X10,
        focus: false,
        alternate_scroll: true,
        eight_bit_meta: true,
        num_lock: true,
        meta_escape: true,
        bell: true,
        bracketed_paste: false,
        sync: false,
        scheme_report: false,
        visibility_report: false,
        resize_report: false,
        sixel_display: false,
        sixel_private_registers: true,
        sixel_cursor_right: false,
        #[cfg(feature = "bidi")]
        arrow_direction: true,
    };

    /// SRM: set, the default, sends with no local echo.
    pub(super) fn send_receive(&self) -> bool {
        self.srm
    }

    /// LNM.
    pub(super) fn newline(&self) -> bool {
        self.lnm
    }

    /// DECCKM.
    pub(super) fn cursor_keys(&self) -> bool {
        self.decckm
    }

    /// DECARM.
    pub(super) fn auto_repeat(&self) -> bool {
        self.decarm
    }

    /// Mode 45.
    pub(super) fn reverse_wrap(&self) -> bool {
        self.reverse_wrap
    }

    /// DECNKM, which DECKPAM sets: the keypad sends application sequences.
    pub(super) fn keypad_application(&self) -> bool {
        self.keypad
    }

    pub(super) fn tracking(&self) -> MouseTracking {
        self.tracking
    }

    pub(super) fn encoding(&self) -> MouseEncoding {
        self.encoding
    }

    /// Mode 1004.
    pub(super) fn focus(&self) -> bool {
        self.focus
    }

    /// Mode 1007.
    pub(super) fn alternate_scroll(&self) -> bool {
        self.alternate_scroll
    }

    /// Mode 1034.
    pub(super) fn eight_bit_meta(&self) -> bool {
        self.eight_bit_meta
    }

    /// Mode 1035.
    pub(super) fn num_lock(&self) -> bool {
        self.num_lock
    }

    /// Mode 1036.
    pub(super) fn meta_escape(&self) -> bool {
        self.meta_escape
    }

    /// Mode 1042.
    pub(super) fn bell(&self) -> bool {
        self.bell
    }

    /// Mode 2004.
    pub(super) fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    /// Mode 2026: a synchronized update is holding frames.
    pub(super) fn synchronized(&self) -> bool {
        self.sync
    }

    /// Mode 2031.
    pub(super) fn scheme_report(&self) -> bool {
        self.scheme_report
    }

    /// Mode 2033.
    pub(super) fn visibility_report(&self) -> bool {
        self.visibility_report
    }

    /// Mode 2048.
    pub(super) fn resize_report(&self) -> bool {
        self.resize_report
    }

    /// Mode 1243: the Left and Right arrow keys swap while the cursor's
    /// paragraph resolves right to left (ADR-1004 D11).
    #[cfg(feature = "bidi")]
    pub(super) fn arrow_direction(&self) -> bool {
        self.arrow_direction
    }
}

impl State {
    /// A supported mode's state, as DECRQM reports it.
    fn mode(&self, mode: Mode) -> bool {
        let s = &self.screen;
        let m = &self.modes;
        match mode {
            Mode::Irm => s.insert_mode(),
            Mode::Srm => m.srm,
            Mode::Lnm => m.lnm,
            Mode::Decckm => m.decckm,
            Mode::Decscnm => s.reverse_video(),
            Mode::Decom => s.origin_mode(),
            Mode::Decawm => s.auto_wrap(),
            Mode::Decarm => m.decarm,
            Mode::X10Mouse => m.tracking == MouseTracking::X10,
            Mode::NormalMouse => m.tracking == MouseTracking::Normal,
            Mode::ButtonMouse => m.tracking == MouseTracking::Button,
            Mode::AnyMouse => m.tracking == MouseTracking::Any,
            // The cursor blinks when the host blinks it and its style is
            // one that blinks.
            Mode::CursorBlink => self.host.cursor_blink && self.cursor_style.blinks(),
            Mode::Dectcem => s.cursor_visible(),
            Mode::ReverseWrap => m.reverse_wrap,
            Mode::Decnkm => m.keypad,
            Mode::Lrmm => s.lr_margin_mode(),
            Mode::AltScreen | Mode::AltScreenClear | Mode::AltScreenSaveCursor => s.alternate(),
            Mode::SaveCursor => false,
            Mode::Focus => m.focus,
            Mode::SgrMouse => m.encoding == MouseEncoding::Sgr,
            Mode::UrxvtMouse => m.encoding == MouseEncoding::Urxvt,
            Mode::PixelMouse => m.encoding == MouseEncoding::Pixels,
            Mode::AlternateScroll => m.alternate_scroll,
            Mode::EightBitMeta => m.eight_bit_meta,
            Mode::NumLock => m.num_lock,
            Mode::MetaEscape => m.meta_escape,
            Mode::Bell => m.bell,
            Mode::BracketedPaste => m.bracketed_paste,
            Mode::Sync => m.sync,
            Mode::GraphemeWidth => s.width_method() == WidthMethod::Unicode,
            Mode::Reflow => s.reflow(),
            Mode::SchemeReport => m.scheme_report,
            Mode::VisibilityReport => m.visibility_report,
            Mode::ResizeReport => m.resize_report,
            Mode::SixelDisplay => m.sixel_display,
            Mode::SixelPrivateRegisters => m.sixel_private_registers,
            Mode::SixelCursorRight => m.sixel_cursor_right,
            #[cfg(feature = "bidi")]
            Mode::Bdsm => !s.paragraph().contains(crate::LineFlags::EXPLICIT),
            #[cfg(feature = "bidi")]
            Mode::ArrowDirection => m.arrow_direction,
            #[cfg(feature = "bidi")]
            Mode::BoxMirror => s.paragraph().contains(crate::LineFlags::BOX_MIRROR),
            #[cfg(feature = "bidi")]
            Mode::DetectDirection => s.paragraph().contains(crate::LineFlags::DETECT),
        }
    }

    /// Sets or resets a supported mode, as SM, RM and the resets do.
    fn set_mode(&mut self, mode: Mode, on: bool) {
        match mode {
            Mode::Irm => self.screen.set_insert_mode(on),
            Mode::Srm => self.modes.srm = on,
            Mode::Lnm => self.modes.lnm = on,
            Mode::Decckm => self.modes.decckm = on,
            Mode::Decscnm => self.screen.set_reverse_video(on),
            // VT100 User Guide, DECOM: "The cursor is moved to the new
            // home position when this mode is set or reset."
            Mode::Decom => {
                self.screen.set_origin_mode(on);
                self.screen.move_to(0, 0);
            }
            Mode::Decawm => self.screen.set_auto_wrap(on),
            Mode::Decarm => self.modes.decarm = on,
            Mode::X10Mouse => self.track(MouseTracking::X10, on),
            Mode::NormalMouse => self.track(MouseTracking::Normal, on),
            Mode::ButtonMouse => self.track(MouseTracking::Button, on),
            Mode::AnyMouse => self.track(MouseTracking::Any, on),
            // Under the default style SM and RM leave the cursor as the
            // host blinks it.
            Mode::CursorBlink => {
                let style = self.cursor_style.with_blink(on);
                self.set_cursor_style(style);
            }
            Mode::Dectcem => self.screen.set_cursor_visible(on),
            Mode::ReverseWrap => self.modes.reverse_wrap = on,
            Mode::Decnkm => self.modes.keypad = on,
            Mode::Lrmm => self.screen.set_lr_margin_mode(on),
            Mode::AltScreen => self.screen.set_alternate(on),
            Mode::AltScreenClear => {
                if !on && self.screen.alternate() {
                    self.screen.erase_screen(Erase::All);
                }
                self.screen.set_alternate(on);
            }
            Mode::SaveCursor if on => self.save_cursor(),
            Mode::SaveCursor => self.restore_cursor(),
            Mode::AltScreenSaveCursor if on => {
                self.save_cursor();
                self.screen.set_alternate(true);
                self.screen.erase_screen(Erase::All);
            }
            Mode::AltScreenSaveCursor => {
                self.screen.set_alternate(false);
                self.restore_cursor();
            }
            Mode::Focus => self.modes.focus = on,
            Mode::SgrMouse => self.encode(MouseEncoding::Sgr, on),
            Mode::UrxvtMouse => self.encode(MouseEncoding::Urxvt, on),
            Mode::PixelMouse => self.encode(MouseEncoding::Pixels, on),
            Mode::AlternateScroll => self.modes.alternate_scroll = on,
            Mode::EightBitMeta => self.modes.eight_bit_meta = on,
            Mode::NumLock => self.modes.num_lock = on,
            Mode::MetaEscape => self.modes.meta_escape = on,
            Mode::Bell => self.modes.bell = on,
            Mode::BracketedPaste => self.modes.bracketed_paste = on,
            Mode::Sync if on => self.modes.sync = self.host.sync_enabled,
            Mode::Sync => self.end_sync(),
            Mode::GraphemeWidth => self.screen.set_width_method(if on {
                WidthMethod::Unicode
            } else {
                WidthMethod::Wcwidth
            }),
            Mode::Reflow => self.screen.set_reflow(on),
            Mode::SchemeReport => self.modes.scheme_report = on,
            // Set, modes 2033 and 2048 report at once, set already or not.
            Mode::VisibilityReport => {
                self.modes.visibility_report = on;
                if on {
                    self.reply_visibility();
                }
            }
            Mode::ResizeReport => {
                self.modes.resize_report = on;
                if on {
                    self.report_size();
                }
            }
            Mode::SixelDisplay => self.modes.sixel_display = on,
            Mode::SixelPrivateRegisters => self.modes.sixel_private_registers = on,
            Mode::SixelCursorRight => self.modes.sixel_cursor_right = on,
            // The paragraph modes mark every row of the cursor's paragraph
            // and each new row (ADR-1004 D2).
            #[cfg(feature = "bidi")]
            Mode::Bdsm => self
                .screen
                .set_paragraph_flag(crate::LineFlags::EXPLICIT, !on),
            #[cfg(feature = "bidi")]
            Mode::ArrowDirection => self.modes.arrow_direction = on,
            #[cfg(feature = "bidi")]
            Mode::BoxMirror => self
                .screen
                .set_paragraph_flag(crate::LineFlags::BOX_MIRROR, on),
            #[cfg(feature = "bidi")]
            Mode::DetectDirection => self.screen.set_paragraph_flag(crate::LineFlags::DETECT, on),
        }
    }

    /// SM of a tracking mode replaces the one set; RM of any clears
    /// tracking. Motion is reported afresh.
    fn track(&mut self, tracking: MouseTracking, on: bool) {
        let tracking = if on { tracking } else { MouseTracking::Off };
        if tracking != self.modes.tracking {
            self.modes.tracking = tracking;
            self.mouse_last = None;
        }
    }

    /// SM of an encoding replaces the one set; RM clears it only when it
    /// is the one set, as xterm does.
    fn encode(&mut self, encoding: MouseEncoding, on: bool) {
        if on {
            self.modes.encoding = encoding;
        } else if self.modes.encoding == encoding {
            self.modes.encoding = MouseEncoding::X10;
        }
    }

    /// Ends a synchronized update, by RM, a reset or the host: the end is
    /// counted, and the input call returns after the byte that ended it.
    pub(super) fn end_sync(&mut self) {
        if self.modes.sync {
            self.modes.sync = false;
            self.sync_frames = self.sync_frames.saturating_add(1);
            self.pause = true;
        }
    }

    /// A row's initial state, a host setting read where the row names one.
    fn initial(&self, row: &Row) -> bool {
        match (row.initial, row.mode) {
            (Initial::Set, _) => true,
            (Initial::Host(_), Some(Mode::CursorBlink)) => self.host.cursor_blink,
            (Initial::Host(_), Some(Mode::GraphemeWidth)) => {
                self.host.width_method == WidthMethod::Unicode
            }
            #[cfg(feature = "images")]
            (Initial::Host(_), Some(Mode::SixelPrivateRegisters)) => {
                self.image_limits().private_registers
            }
            #[cfg(feature = "bidi")]
            (Initial::Host(_), Some(Mode::DetectDirection)) => {
                self.host.text_direction == super::TextDirection::Auto
            }
            _ => false,
        }
    }

    /// Whether the host offers a row's mode: a mode the host presents
    /// frames by only while it says it does, and a mode of sixel's only
    /// while sixel is offered.
    fn offered(&self, row: &Row) -> bool {
        match row.host {
            Host::Draw => self.host.sync_enabled,
            #[cfg(feature = "images")]
            Host::Sixel => self.image_offered(super::images::ImageString::Sixel),
            #[cfg(not(feature = "images"))]
            Host::Sixel => false,
            _ => true,
        }
    }
}

/// The paragraph properties BDSM, 2500 and 2501 give at their rows'
/// initial states: explicit mode where BDSM starts reset, box drawing
/// mirrored where 2500 starts set, and detection where 2501 does.
#[cfg(feature = "bidi")]
pub(super) fn paragraph_modes_initial(state: &State) -> crate::LineFlags {
    use crate::LineFlags;
    let mut flags = LineFlags::empty();
    for row in BIDI_MODES {
        let initial = state.initial(row);
        match row.mode {
            Some(Mode::Bdsm) if !initial => flags.insert(LineFlags::EXPLICIT),
            Some(Mode::BoxMirror) if initial => flags.insert(LineFlags::BOX_MIRROR),
            Some(Mode::DetectDirection) if initial => flags.insert(LineFlags::DETECT),
            _ => {}
        }
    }
    flags
}

/// SM or RM of one mode number: the row's setter, or nothing for an
/// ignored or refused row or a number with no row.
pub(super) fn set(state: &mut State, dec: bool, number: u32, on: bool) {
    if let Some(Row {
        status: Status::Supported,
        mode: Some(mode),
        ..
    }) = find(dec, number)
    {
        state.set_mode(*mode, on);
    }
}

/// DECRQM's value for a mode number: 1 set, 2 reset, 3 permanently set, 4
/// permanently reset, 0 not recognised. An ignored or refused row, and a
/// row the host does not offer, reports its initial state as permanent.
pub(super) fn report(state: &State, dec: bool, number: u32) -> u8 {
    let Some(row) = find(dec, number) else {
        return 0;
    };
    match row.mode {
        Some(mode) if row.status == Status::Supported && state.offered(row) => {
            if state.mode(mode) {
                1
            } else {
                2
            }
        }
        _ if state.initial(row) => 3,
        _ => 4,
    }
}

/// The modes DECRQM reports set (1), each as whether it is a DEC private
/// mode and its number: the ANSI modes, then the DEC private modes, each
/// in ascending order.
pub(super) fn in_force(state: &State) -> Vec<(bool, u32)> {
    let mut set: Vec<(bool, u32)> = rows()
        .filter(|r| r.built() && report(state, r.dec, r.number) == 1)
        .map(|r| (r.dec, r.number))
        .collect();
    set.sort_unstable();
    set.dedup();
    set
}

impl super::Terminal {
    /// The ANSI and DEC private modes in force: those DECRQM reports set,
    /// each as whether it is a DEC private mode and its number, the ANSI
    /// modes first and each kind in ascending order. A host that shows the
    /// terminal's state reads them here without writing a query.
    pub fn modes_in_force(&self) -> Vec<(bool, u32)> {
        in_force(&self.state)
    }
}

/// Returns each row a reset covers to its initial state, through its
/// setter, where its state differs: DECSTR the soft rows, RIS the hard
/// rows too.
pub(super) fn reset(state: &mut State, hard: bool) {
    for row in rows() {
        let covered = row.reset == Reset::Soft || (hard && row.reset == Reset::Hard);
        if let (true, Some(mode)) = (covered, row.mode) {
            let initial = state.initial(row);
            if state.mode(mode) != initial {
                state.set_mode(mode, initial);
            }
        }
    }
}

/// Where XTSAVE keeps a mode's state, as xterm's `savemodes` keeps it:
/// tracking modes 1000 to 1003 share a slot and mode 9 has its own, though
/// each holds the tracking all five set; the three encodings share one, and
/// so do the three alternate screen modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Mode(Mode),
    X10Tracking,
    Tracking,
    Encoding,
    Screen,
}

/// A slot's saved state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    Mode(bool),
    Tracking(MouseTracking),
    Encoding(MouseEncoding),
    /// The alternate screen is shown.
    Screen(bool),
}

/// What XTSAVE saved, one state a slot, as DECSC keeps one cursor: until
/// a later XTSAVE of the slot replaces it, or RIS clears it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SavedModes(BTreeMap<Slot, Value>);

impl Mode {
    /// The slot XTSAVE keeps the mode's state in; none for mode 1048,
    /// which holds no state.
    fn slot(self) -> Option<Slot> {
        Some(match self {
            Self::X10Mouse => Slot::X10Tracking,
            Self::NormalMouse | Self::ButtonMouse | Self::AnyMouse => Slot::Tracking,
            Self::SgrMouse | Self::UrxvtMouse | Self::PixelMouse => Slot::Encoding,
            Self::AltScreen | Self::AltScreenClear | Self::AltScreenSaveCursor => Slot::Screen,
            Self::SaveCursor => return None,
            mode => Slot::Mode(mode),
        })
    }
}

impl State {
    fn saved_value(&self, slot: Slot) -> Value {
        match slot {
            Slot::Mode(mode) => Value::Mode(self.mode(mode)),
            Slot::X10Tracking | Slot::Tracking => Value::Tracking(self.modes.tracking),
            Slot::Encoding => Value::Encoding(self.modes.encoding),
            Slot::Screen => Value::Screen(self.screen.alternate()),
        }
    }

    /// Restores a slot's state: a mode through its setter where its state
    /// differs, and the alternate screen as mode 47 switches it, neither
    /// cleared nor the cursor moved, as xterm restores it.
    fn restore_value(&mut self, slot: Slot, value: Value) {
        match (slot, value) {
            (Slot::Mode(mode), Value::Mode(on)) if self.mode(mode) != on => {
                self.set_mode(mode, on);
            }
            (_, Value::Tracking(tracking)) => self.track(tracking, true),
            (_, Value::Encoding(encoding)) => self.modes.encoding = encoding,
            (_, Value::Screen(on)) => self.screen.set_alternate(on),
            _ => {}
        }
    }
}

/// The supported mode a DEC private mode number names.
fn dec_mode(number: u32) -> Option<Mode> {
    match find(true, number) {
        Some(Row {
            status: Status::Supported,
            mode,
            ..
        }) => *mode,
        _ => None,
    }
}

impl Row {
    /// Whether XTSAVE's empty list holds the row: kitty's rule, every mode
    /// free of side effects, as the maintainer ruled. That is every
    /// supported DEC private mode but DECOM, which moves the cursor home,
    /// 47, 1047 and 1049, which switch screens, 1048, which holds no state,
    /// and 2026, which holds frames. No ANSI mode is in it.
    fn in_empty_list(&self) -> bool {
        self.dec
            && self.status == Status::Supported
            && !matches!(
                self.mode,
                None | Some(
                    Mode::Decom
                        | Mode::AltScreen
                        | Mode::AltScreenClear
                        | Mode::SaveCursor
                        | Mode::AltScreenSaveCursor
                        | Mode::Sync
                )
            )
    }
}

/// The slots XTSAVE's empty list saves and restores, in the table's order,
/// each once.
fn empty_list_slots() -> Vec<Slot> {
    let mut slots: Vec<Slot> = Vec::new();
    for slot in rows()
        .filter(|r| r.in_empty_list())
        .filter_map(|r| r.mode.and_then(Mode::slot))
    {
        if !slots.contains(&slot) {
            slots.push(slot);
        }
    }
    slots
}

/// XTSAVE, `CSI ? Pm s`, of one DEC private mode, or with `None`, kitty's
/// empty list, of every mode [`Row::in_empty_list`] names. XTSAVE of 1048
/// saves the cursor as DECSC does, as xterm takes it. A number with no
/// supported row saves nothing.
pub(super) fn save(state: &mut State, number: Option<u32>) {
    let slots = match number {
        Some(n) => match dec_mode(n) {
            Some(Mode::SaveCursor) => return state.save_cursor(),
            mode => mode.and_then(Mode::slot).into_iter().collect(),
        },
        None => empty_list_slots(),
    };
    for slot in slots {
        let value = state.saved_value(slot);
        state.xtsave.0.insert(slot, value);
    }
}

/// XTRESTORE, `CSI ? Pm r`, of one DEC private mode, or with `None` of
/// every slot of the empty list that XTSAVE filled, in the table's order. A
/// slot nothing was saved in stays as it is; XTRESTORE of 1048 restores the
/// cursor as DECRC does, as xterm takes it.
pub(super) fn restore(state: &mut State, number: Option<u32>) {
    let slots = match number {
        Some(n) => match dec_mode(n) {
            Some(Mode::SaveCursor) => return state.restore_cursor(),
            mode => mode.and_then(Mode::slot).into_iter().collect(),
        },
        None => empty_list_slots(),
    };
    for slot in slots {
        if let Some(&value) = state.xtsave.0.get(&slot) {
            state.restore_value(slot, value);
        }
    }
}

impl CursorStyle {
    /// Whether the style blinks: the default one blinks as the host says.
    fn blinks(self) -> bool {
        matches!(
            self,
            Self::Default | Self::BlockBlink | Self::UnderlineBlink | Self::BarBlink
        )
    }

    /// The style in the same shape, blinking or steady; the default style
    /// stays as it is.
    fn with_blink(self, on: bool) -> Self {
        match (self, on) {
            (Self::BlockBlink | Self::BlockSteady, true) => Self::BlockBlink,
            (Self::BlockBlink | Self::BlockSteady, false) => Self::BlockSteady,
            (Self::UnderlineBlink | Self::UnderlineSteady, true) => Self::UnderlineBlink,
            (Self::UnderlineBlink | Self::UnderlineSteady, false) => Self::UnderlineSteady,
            (Self::BarBlink | Self::BarSteady, true) => Self::BarBlink,
            (Self::BarBlink | Self::BarSteady, false) => Self::BarSteady,
            (Self::Default, _) => Self::Default,
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::Terminal;

    const CRATE: &str = env!("CARGO_MANIFEST_DIR");

    /// Every row of the table, the bidi rows among the others, ANSI modes
    /// first, each kind in the order [`MODES`] gives it.
    fn every_row() -> Vec<&'static Row> {
        let mut out: Vec<&'static Row> = Vec::new();
        let mut bidi = BIDI_MODES.iter().peekable();
        for row in MODES {
            while let Some(b) = bidi.next_if(|b| (b.dec, b.number) < (row.dec, row.number)) {
                out.push(b);
            }
            out.push(row);
        }
        out.extend(bidi);
        out
    }

    fn kind(row: &Row) -> &'static str {
        if row.dec { "DEC" } else { "ANSI" }
    }

    fn initial(row: &Row) -> String {
        match row.initial {
            Initial::Set => "set".into(),
            Initial::Reset => "reset".into(),
            Initial::Host(what) => format!("set by {what}"),
        }
    }

    fn reset(row: &Row) -> &'static str {
        match row.reset {
            Reset::Soft => "DECSTR and RIS",
            Reset::Hard => "RIS",
            Reset::Never => "none",
        }
    }

    fn host(row: &Row) -> &'static str {
        match row.host {
            Host::Nothing => "nothing",
            Host::Input => "reports input",
            Host::Paste => "pastes",
            Host::Draw => "presents by it",
            Host::Notify => "tells of changes",
            Host::Bell => "rings the bell",
            Host::Sixel => "offers sixel",
        }
    }

    fn status(row: &Row) -> &'static str {
        match row.status {
            Status::Supported => "supported",
            Status::Ignored => "ignored",
            Status::Refused => "refused",
        }
    }

    fn tests(row: &Row) -> String {
        row.tests
            .iter()
            .map(|t| format!("crates/vtium-term/tests/{t}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    const COLUMNS: [&str; 11] = [
        "id", "kind", "number", "name", "status", "default", "reset", "host", "terminfo", "spec",
        "tests",
    ];

    fn cells(row: &Row) -> [String; 11] {
        [
            row.id(),
            kind(row).into(),
            row.number.to_string(),
            row.name.into(),
            status(row).into(),
            initial(row),
            reset(row).into(),
            host(row).into(),
            row.terminfo.into(),
            row.spec.into(),
            tests(row),
        ]
    }

    /// `docs/modes.tsv`, generated from the table.
    fn tsv() -> String {
        let mut out = COLUMNS.join("\t") + "\n";
        for row in every_row() {
            out += &(cells(row).join("\t") + "\n");
        }
        out
    }

    /// `docs/modes.md`, generated from the table.
    fn markdown() -> String {
        let mut out = String::from(
            "# The modes vtium recognises\n\n\
             Generated from the mode table in `crates/vtium-term/src/terminal/modes.rs` by the \
             test `generated_documents_match_the_table`; `VTIUM_TERM_BLESS=1 cargo test -p \
             vtium-term generated_documents` rewrites it and `docs/modes.tsv`. A row's status is supported, when set and reset do \
             what the specification says, ignored, when vtium accepts the mode and it has no \
             effect, or refused, when vtium deliberately does not implement it. The spec \
             column says why a mode is ignored or refused. A number with no row is not recognised: DECRQM reports 0, and \
             SM and RM change nothing. An ignored or refused row reports its default as \
             permanent, 3 when set and 4 when reset.\n\n\
             XTSAVE (`CSI ? Pm s`) saves the state of each DEC private mode it lists, and \
             XTRESTORE (`CSI ? Pm r`) sets each mode it lists back to the state saved, through \
             SM or RM; a mode nothing was saved for stays as it is. With an empty list they \
             save and restore every mode free of side effects, as kitty's misc-protocol has \
             it: every supported DEC private mode but DECOM (6), which moves the cursor home, \
             47, 1047 and 1049, which switch screens, 1048 and 2026; no ANSI mode is in it. \
             Modes that share one state share one saved state, as xterm keeps them: the \
             tracking modes 1000, 1002 and 1003, while 9 has a saved state of its own holding \
             the same tracking, the encodings 1006, 1015 and 1016, and the \
             alternate screen modes 47, 1047 and 1049, which XTRESTORE of one of them switches \
             as 47 does, neither cleared nor the cursor moved. Mode 1048 holds no state: XTSAVE \
             of it saves the cursor as DECSC does and XTRESTORE restores it as DECRC does. DECSTR keeps what XTSAVE saved, and RIS clears it.\n\n",
        );
        out += &format!("| {} |\n", COLUMNS.join(" | "));
        out += &format!("|{}\n", "---|".repeat(COLUMNS.len()));
        for row in every_row() {
            let cells = cells(row).map(|c| {
                if c.is_empty() {
                    c
                } else {
                    c.replace('|', "\\|")
                }
            });
            out += &format!("| {} |\n", cells.join(" | "));
        }
        out
    }

    // docs/architecture.md, "Tables, references and maps": the committed
    // documents are generated from the table.
    #[test]
    fn generated_documents_match_the_table() {
        let mut stale = Vec::new();
        for (name, want) in [("modes.tsv", tsv()), ("modes.md", markdown())] {
            let path = format!("{CRATE}/../../docs/{name}");
            if std::fs::read_to_string(&path).ok().as_deref() == Some(want.as_str()) {
                continue;
            }
            if std::env::var_os("VTIUM_TERM_BLESS").is_some() {
                std::fs::write(&path, want).expect("rewrite the document");
            } else {
                stale.push(name);
            }
        }
        assert!(
            stale.is_empty(),
            "docs/{stale:?} differ from the mode table; rerun with VTIUM_TERM_BLESS=1 to rewrite them"
        );
    }

    // ADR-1000, "The target set": each row's status is the ADR's. A mode the
    // ADR marks supported, partial, ignored or refused has a row, unless its
    // note says it is not in vtium's mode table; a mode whose note says so,
    // and a planned or weighed mode, has none, so DECRQM answers it 0.
    #[test]
    fn the_table_agrees_with_adr_1000() {
        const NOT_LISTED: &str = "not in vtium's mode table";
        let path = format!("{CRATE}/../../docs/architecture/term/ADR-1000-modes.tsv");
        let adr = std::fs::read_to_string(path).expect("ADR-1000-modes.tsv");
        let mut lines = adr.lines();
        let header: Vec<&str> = lines.next().expect("a header").split('\t').collect();
        let col = |name: &str| header.iter().position(|h| *h == name).expect(name);
        let (id, status_col, note) = (col("id"), col("status"), col("note"));
        let rows: Vec<Vec<&str>> = lines.map(|l| l.split('\t').collect()).collect();
        for row in every_row() {
            let adr_row = rows
                .iter()
                .find(|r| r[id] == row.id())
                .unwrap_or_else(|| panic!("{} has no row in ADR-1000", row.id()));
            assert_eq!(adr_row[status_col], status(row), "{}'s status", row.id());
        }
        for r in &rows {
            let answered = matches!(
                r[status_col],
                "supported" | "partial" | "ignored" | "refused"
            );
            let unlisted = r[note].contains(NOT_LISTED);
            assert!(
                !unlisted || matches!(r[status_col], "refused" | "weighed"),
                "ADR-1000's {} is {}, and only a refused or weighed mode may be {NOT_LISTED}",
                r[id],
                r[status_col]
            );
            let listed = answered && !unlisted;
            assert_eq!(
                every_row().iter().any(|m| m.id() == r[id]),
                listed,
                "ADR-1000's {} is {} with the note {:?}, so the mode table should {}list it",
                r[id],
                r[status_col],
                r[note],
                if listed { "" } else { "not " }
            );
        }
    }

    // docs/architecture.md, "Tables, references and maps": a supported row
    // names a test that sets and resets it, and every test named exists.
    #[test]
    fn every_row_names_tests_that_exist() {
        for row in every_row() {
            assert!(
                row.status != Status::Supported || !row.tests.is_empty(),
                "{} names no test",
                row.id()
            );
            for test in row.tests {
                let (file, func) = test.split_once("::").expect("file::function");
                let source = std::fs::read_to_string(format!("{CRATE}/tests/{file}"))
                    .unwrap_or_else(|_| panic!("{}'s test file {file} is missing", row.id()));
                assert!(
                    source.contains(&format!("fn {func}(")),
                    "{} names {test}, which does not exist",
                    row.id()
                );
            }
        }
    }

    /// The crate's sources outside the screen, which owns the mode
    /// setters, and this module, which alone calls them.
    fn other_sources() -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut dirs = vec![std::path::PathBuf::from(format!("{CRATE}/src"))];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).expect("src") {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                let name = path.to_string_lossy().into_owned();
                let owner = name.ends_with("src/screen.rs")
                    || name.contains("src/screen/")
                    || name.ends_with("src/terminal/modes.rs");
                if name.ends_with(".rs") && !owner {
                    out.push((name, std::fs::read_to_string(&path).expect("a source")));
                }
            }
        }
        out
    }

    // docs/architecture.md, "Tables, references and maps": SM, RM and
    // DECRQM go only through the table, and a mode's state is reached only
    // through its accessors. The accessors, `State::mode` and
    // `State::set_mode`, and the fields of `Modes` are private to this
    // module, which the compiler holds; the screen's setters are public, so
    // this check holds them to this module.
    #[test]
    fn modes_are_set_only_through_the_table() {
        const SETTERS: [&str; 8] = [
            ".set_insert_mode(",
            ".set_auto_wrap(",
            ".set_origin_mode(",
            ".set_reverse_video(",
            ".set_cursor_visible(",
            ".set_alternate(",
            ".set_width_method(",
            ".set_lr_margin_mode(",
        ];
        let sources = other_sources();
        assert!(sources.iter().any(|(n, _)| n.ends_with("src/terminal.rs")));
        for (name, source) in &sources {
            for setter in SETTERS {
                assert!(
                    !source.contains(setter),
                    "{name} calls {setter}, which only the mode table may"
                );
            }
        }
    }

    /// A terminal in a state far from its initial one: text, a pen, modes
    /// set, margins, a tab stop, a saved cursor, the cursor moved.
    fn busy() -> Terminal {
        let mut t = Terminal::new(6, 3, 10);
        t.set_sync_enabled(true);
        t.input(b"ab\x1b[1;31m\x1b[?1000h\x1b[?1006h\x1b[2;3r\x1b[2;2H\x1bHx\x1b7\x1b[4 q");
        t.input(b"\x1b[?2004h\x1b[?2026h\x1b=\x1b[>4;1m\x1b[20h");
        t.take_replies();
        t
    }

    // docs/architecture.md, "Tables, references and maps": every DEC mode
    // number from 0 to 65535 and every ANSI mode number from 0 to 255
    // without a row replies 0 to DECRQM, and SM and RM of it, and of a DEC
    // number XTSAVE and XTRESTORE, change none of the terminal's state,
    // compared whole; a number with a row replies something else.
    #[test]
    fn a_number_with_no_row_replies_0_and_changes_nothing() {
        let mut t = busy();
        let before = t.state.clone();
        let mut checked = 0_u32;
        for (dec, last) in [(false, 255_u32), (true, 65535)] {
            let q = if dec { "?" } else { "" };
            for n in 0..=last {
                t.input(format!("\x1b[{q}{n}$p").as_bytes());
                let reply = t.take_replies();
                // The reply rule test's record of queries is not state.
                t.state.answered.clear();
                let has_row = find(dec, n).is_some();
                let zero = format!("\x1b[{q}{n};0$y").into_bytes();
                assert_eq!(reply == zero, !has_row, "DECRQM {q}{n} replied {reply:?}");
                if has_row {
                    continue;
                }
                t.input(format!("\x1b[{q}{n}h\x1b[{q}{n}l").as_bytes());
                if dec {
                    // XTSAVE and XTRESTORE of it too.
                    t.input(format!("\x1b[?{n}s\x1b[?{n}r").as_bytes());
                }
                assert!(
                    t.state == before,
                    "SM, RM, XTSAVE or XTRESTORE {q}{n} changed the state:\n{:?}\n{:?}",
                    t.state,
                    before
                );
                checked += 1;
            }
        }
        let built = rows().filter(|r| r.built()).count();
        assert_eq!(checked, 256 + 65536 - u32::try_from(built).expect("rows"));
    }
}
