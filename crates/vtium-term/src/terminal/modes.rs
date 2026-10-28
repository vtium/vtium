//! The mode table: one row per ANSI mode (SM, RM: `CSI Pm h`, `CSI Pm l`)
//! and DEC private mode (DECSET, DECRST: `CSI ? Pm h`, `CSI ? Pm l`) vtium
//! recognises, as the C reference's `tsm-modes.def` held them.
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
    /// It reports pointer or focus events, or stops repeating keys.
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
}

/// The accessors of a supported row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    AltScreen,
    NormalMouse,
    ButtonMouse,
    AnyMouse,
    Focus,
    SgrMouse,
    PixelMouse,
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
const MOUSE: &[&str] = &["modes.rs::mouse_tracking_reports_by_its_mode"];
const SGR_MOUSE: &[&str] = &["modes.rs::sgr_mouse_encodings_report_cells_or_pixels"];

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

/// Every mode vtium recognises, ANSI modes first, each kind by number.
#[rustfmt::skip]
pub(crate) static MODES: &[Row] = &[
    row!(ANSI, 2, "KAM", Refused, RESET, Never, Nothing, "", "ECMA-48 §7.2.11; refused: a program could lock the keyboard of the only terminal on the console (libtsm#57)", FIXED, None),
    row!(ANSI, 4, "IRM", Supported, RESET, Soft, Nothing, "", "ECMA-48 §7.2.10", &["modes.rs::irm_inserts_printed_text"], Some(Mode::Irm)),
    row!(ANSI, 12, "SRM", Supported, SET, Soft, Nothing, "", "ECMA-48 §7.2.15; reset, what the terminal sends the program is also shown (local echo)", &["modes.rs::srm_reset_echoes_what_the_terminal_sends"], Some(Mode::Srm)),
    row!(ANSI, 20, "LNM", Supported, RESET, Soft, Nothing, "", "ctlseqs; VT510; kept though ECMA-48's 5th edition withdraws it (ADR-1000)", &["modes.rs::lnm_makes_a_line_feed_a_new_line"], Some(Mode::Lnm)),
    row!(DEC, 1, "DECCKM", Supported, RESET, Soft, Nothing, "", "ctlseqs; VT510", &["modes.rs::decckm_switches_the_cursor_keys"], Some(Mode::Decckm)),
    row!(DEC, 2, "DECANM", Ignored, SET, Never, Nothing, "", "ctlseqs; VT510; VT52 is declined for now (vtium/vtium#65), so DECRQM reports ANSI mode as permanent", FIXED, None),
    row!(DEC, 3, "DECCOLM", Refused, RESET, Never, Nothing, "", "ctlseqs; VT510; refused: the console's size comes from the display and the font (libtsm#57)", FIXED, None),
    row!(DEC, 4, "DECSCLM", Ignored, RESET, Never, Nothing, "", "ctlseqs; VT510; a console has no smooth scroll", FIXED, None),
    row!(DEC, 5, "DECSCNM", Supported, RESET, Soft, Nothing, "flash", "ctlseqs; VT510", &["modes.rs::decscnm_reverses_the_screen"], Some(Mode::Decscnm)),
    row!(DEC, 6, "DECOM", Supported, RESET, Soft, Nothing, "", "ctlseqs; VT510; the cursor goes home as it is set or reset (VT100 User Guide)", &["modes.rs::decom_addresses_from_the_region"], Some(Mode::Decom)),
    row!(DEC, 7, "DECAWM", Supported, SET, Soft, Nothing, "", "ctlseqs; VT510", &["modes.rs::decawm_wraps_at_the_last_column"], Some(Mode::Decawm)),
    row!(DEC, 8, "DECARM", Supported, SET, Soft, Input, "", "ctlseqs; VT510; the host repeats a held key, and stops while it is reset", &["modes.rs::decarm_tells_the_host_to_repeat_keys"], Some(Mode::Decarm)),
    row!(DEC, 9, "X10MOUSE", Supported, RESET, Soft, Input, "", "ctlseqs, X10 mouse; DECSTR resets it, where xterm resets it on RIS only (libtsm#76)", MOUSE, Some(Mode::X10Mouse)),
    row!(DEC, 12, "CURBLINK", Supported, Initial::Host("the host's cursor blink"), Soft, Nothing, "", "ctlseqs, AT&T 610 cursor blink; held in a DECSCUSR style, while the default style blinks as the host's setting says", &["modes.rs::mode_12_blinks_a_chosen_cursor_style"], Some(Mode::CursorBlink)),
    row!(DEC, 18, "DECPFF", Ignored, RESET, Never, Nothing, "", "ctlseqs; VT510; a console has no printer", FIXED, None),
    row!(DEC, 19, "DECPEX", Ignored, RESET, Never, Nothing, "", "ctlseqs; VT510; a console has no printer", FIXED, None),
    row!(DEC, 25, "DECTCEM", Supported, SET, Soft, Nothing, "civis cnorm", "ctlseqs; VT510", &["modes.rs::dectcem_shows_and_hides_the_cursor"], Some(Mode::Dectcem)),
    row!(DEC, 42, "DECNRCM", Refused, RESET, Never, Nothing, "", "ctlseqs; VT510; refused: national replacement sets are obsolete next to UTF-8 (libtsm#57)", FIXED, None),
    row!(DEC, 47, "ALTBUF", Supported, RESET, Soft, Nothing, "", "ctlseqs, the alternate screen, neither cleared nor the cursor saved", &["modes.rs::mode_47_switches_screens_only"], Some(Mode::AltScreen)),
    row!(DEC, 1000, "VT200MOUSE", Supported, RESET, Soft, Input, "", "ctlseqs, normal tracking; DECSTR resets it, where xterm resets it on RIS only (libtsm#76)", MOUSE, Some(Mode::NormalMouse)),
    row!(DEC, 1001, "HILITEMOUSE", Refused, RESET, Never, Nothing, "", "ctlseqs; refused: the terminal waits on the program's XTHIMOUSE answer before it can show a selection (ADR-1000)", FIXED, None),
    row!(DEC, 1002, "BTNMOUSE", Supported, RESET, Soft, Input, "", "ctlseqs, button-event tracking; DECSTR resets it, where xterm resets it on RIS only (libtsm#76)", MOUSE, Some(Mode::ButtonMouse)),
    row!(DEC, 1003, "ANYMOUSE", Supported, RESET, Soft, Input, "", "ctlseqs, any-event tracking; DECSTR resets it, where xterm resets it on RIS only (libtsm#76)", MOUSE, Some(Mode::AnyMouse)),
    row!(DEC, 1004, "FOCUS", Supported, RESET, Soft, Input, "fe fd", "ctlseqs, FocusIn and FocusOut; DECSTR resets it, where xterm resets it on RIS only (libtsm#76)", &["modes.rs::focus_events_are_reported_while_set"], Some(Mode::Focus)),
    row!(DEC, 1006, "SGRMOUSE", Supported, RESET, Soft, Input, "XM", "ctlseqs, SGR encoding, whatever order the modes were set in; DECSTR resets it, where xterm resets it on RIS only (libtsm#76)", SGR_MOUSE, Some(Mode::SgrMouse)),
    row!(DEC, 1016, "PIXMOUSE", Supported, RESET, Soft, Input, "", "ctlseqs, SGR-Pixels encoding, pixels counted from 1; DECSTR resets it, where xterm resets it on RIS only (libtsm#76)", SGR_MOUSE, Some(Mode::PixelMouse)),
    row!(DEC, 1047, "ALTBUF1047", Supported, RESET, Soft, Nothing, "", "ctlseqs, the alternate screen, cleared as it is left", &["modes.rs::mode_1047_clears_the_alternate_screen_as_it_leaves"], Some(Mode::AltScreenClear)),
    row!(DEC, 1048, "SAVECUR", Supported, RESET, Never, Nothing, "", "ctlseqs, \"Save cursor as in DECSC\": set saves, reset restores as DECRC does, and nothing stays set", &["modes.rs::mode_1048_saves_and_restores_the_cursor_as_decsc_does"], Some(Mode::SaveCursor)),
    row!(DEC, 1049, "ALTBUF1049", Supported, RESET, Soft, Nothing, "smcup rmcup", "ctlseqs, the cursor saved as in DECSC and the alternate screen cleared as it is entered; leaving restores the cursor and the pen as DECRC does (libtsm#20)", &["modes.rs::mode_1049_saves_the_cursor_and_the_pen_around_the_alternate_screen"], Some(Mode::AltScreenSaveCursor)),
    row!(DEC, 2004, "BRPASTE", Supported, RESET, Soft, Paste, "BD BE PS PE", "ctlseqs, bracketed paste; a paste never carries its own end marker (ADR-1000)", &["modes.rs::bracketed_paste_brackets_a_paste"], Some(Mode::BracketedPaste)),
    row!(DEC, 2026, "SYNC", Supported, RESET, Soft, Draw, "Sync", "contour synchronized-output; offered while the host presents by it", &["modes.rs::mode_2026_holds_frames_while_the_host_offers_it"], Some(Mode::Sync)),
    row!(DEC, 2027, "GRAPHEME", Supported, Initial::Host("the host's width method"), Soft, Nothing, "", "contour unicode-core; RIS and DECSTR return it to the configured method", &["modes.rs::mode_2027_reports_and_resets_to_the_configured_method"], Some(Mode::GraphemeWidth)),
    row!(DEC, 2028, "REFLOW", Supported, SET, Hard, Nothing, "", "contour line-reflow-mode; each print, and SM and RM on the cursor's row, mark a row as rewrapping at a width change or, reset, cut at a narrower margin (ADR-1003); RIS sets it", &["reflow.rs::mode_2028_marks_the_rows_a_resize_cuts"], Some(Mode::Reflow)),
    row!(DEC, 2031, "SCHEMEREPORT", Supported, RESET, Soft, Nothing, "", "contour color-palette-update-notifications; CSI ? 997 ; Ps n when the default background turns dark or light by its luma (ITU-R BT.601), as CSI ? 996 n answers", &["inband.rs::mode_2031_reports_a_change_of_scheme", "inband.rs::decstr_and_ris_reset_the_report_modes"], Some(Mode::SchemeReport)),
    row!(DEC, 2033, "VISREPORT", Supported, RESET, Soft, Notify, "", "foot(7), rockorager's visibility reports; CSI ? 999 ; Ps n as it is set and on each change, a VT switch or a blank", &["inband.rs::mode_2033_reports_visibility_on_set_and_each_change", "inband.rs::decstr_and_ris_reset_the_report_modes"], Some(Mode::VisibilityReport)),
    row!(DEC, 2048, "RESIZEREPORT", Supported, RESET, Soft, Notify, "", "rockorager's in-band resize notifications; CSI 48 ; rows ; cols ; height ; width t as it is set and after each resize, the text area in pixels 0 while the cell size is unknown", &["inband.rs::mode_2048_reports_the_text_area_on_set_and_each_resize", "inband.rs::decstr_and_ris_reset_the_report_modes"], Some(Mode::ResizeReport)),
];

fn find(dec: bool, number: u32) -> Option<&'static Row> {
    MODES.iter().find(|r| r.dec == dec && r.number == number)
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

/// How a pointer event is encoded (modes 1006 and 1016, one at a time);
/// with neither, the X10 encoding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum MouseEncoding {
    #[default]
    X10,
    Sgr,
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
    tracking: MouseTracking,
    encoding: MouseEncoding,
    focus: bool,
    bracketed_paste: bool,
    sync: bool,
    scheme_report: bool,
    visibility_report: bool,
    resize_report: bool,
}

impl Modes {
    /// Each mode at its row's initial state.
    pub(super) const INITIAL: Self = Self {
        srm: true,
        lnm: false,
        decckm: false,
        decarm: true,
        tracking: MouseTracking::Off,
        encoding: MouseEncoding::X10,
        focus: false,
        bracketed_paste: false,
        sync: false,
        scheme_report: false,
        visibility_report: false,
        resize_report: false,
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
            Mode::AltScreen | Mode::AltScreenClear | Mode::AltScreenSaveCursor => s.alternate(),
            Mode::SaveCursor => false,
            Mode::Focus => m.focus,
            Mode::SgrMouse => m.encoding == MouseEncoding::Sgr,
            Mode::PixelMouse => m.encoding == MouseEncoding::Pixels,
            Mode::BracketedPaste => m.bracketed_paste,
            Mode::Sync => m.sync,
            Mode::GraphemeWidth => s.width_method() == WidthMethod::Unicode,
            Mode::Reflow => s.reflow(),
            Mode::SchemeReport => m.scheme_report,
            Mode::VisibilityReport => m.visibility_report,
            Mode::ResizeReport => m.resize_report,
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
            Mode::PixelMouse => self.encode(MouseEncoding::Pixels, on),
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
        }
    }

    /// SM of a tracking mode replaces the one set; RM of any clears
    /// tracking, as the C reference did. Motion is reported afresh.
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
            _ => false,
        }
    }

    /// Whether the host offers a row's mode: a mode the host presents
    /// frames by only while it says it does.
    fn offered(&self, row: &Row) -> bool {
        row.host != Host::Draw || self.host.sync_enabled
    }
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

/// Returns each row a reset covers to its initial state, through its
/// setter, where its state differs: DECSTR the soft rows, RIS the hard
/// rows too.
pub(super) fn reset(state: &mut State, hard: bool) {
    for row in MODES {
        let covered = row.reset == Reset::Soft || (hard && row.reset == Reset::Hard);
        if let (true, Some(mode)) = (covered, row.mode) {
            let initial = state.initial(row);
            if state.mode(mode) != initial {
                state.set_mode(mode, initial);
            }
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
        for row in MODES {
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
             vtium-term generated_documents` rewrites it and `docs/modes.tsv`. ADR-1000 decides \
             each row's status. A number with no row is not recognised: DECRQM reports 0, and \
             SM and RM change nothing. An ignored or refused row reports its default as \
             permanent, 3 when set and 4 when reset.\n\n",
        );
        out += &format!("| {} |\n", COLUMNS.join(" | "));
        out += &format!("|{}\n", "---|".repeat(COLUMNS.len()));
        for row in MODES {
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

    // ADR-1000, "The tables as data": each row's status is the ADR's, and
    // every mode the ADR gives #12 with a status other than "no row" has a
    // row.
    #[test]
    fn the_table_agrees_with_adr_1000() {
        let path = format!("{CRATE}/../../docs/architecture/term/ADR-1000-modes.tsv");
        let adr = std::fs::read_to_string(path).expect("ADR-1000-modes.tsv");
        let mut lines = adr.lines();
        let header: Vec<&str> = lines.next().expect("a header").split('\t').collect();
        let col = |name: &str| header.iter().position(|h| *h == name).expect(name);
        let (id, status_col, issue) = (col("id"), col("status"), col("issue"));
        let rows: Vec<Vec<&str>> = lines.map(|l| l.split('\t').collect()).collect();
        for row in MODES {
            let adr_row = rows
                .iter()
                .find(|r| r[id] == row.id())
                .unwrap_or_else(|| panic!("{} has no row in ADR-1000", row.id()));
            assert_eq!(adr_row[status_col], status(row), "{}'s status", row.id());
        }
        for r in &rows {
            let ours = r[issue].split(", ").any(|i| i == "#12");
            if ours && r[status_col] != "no row" {
                assert!(
                    MODES.iter().any(|m| m.id() == r[id]),
                    "ADR-1000's {} has no row in the mode table",
                    r[id]
                );
            }
        }
    }

    // docs/architecture.md, "Tables, references and maps": a supported row
    // names a test that sets and resets it, and every test named exists.
    #[test]
    fn every_row_names_tests_that_exist() {
        for row in MODES {
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
        const SETTERS: [&str; 7] = [
            ".set_insert_mode(",
            ".set_auto_wrap(",
            ".set_origin_mode(",
            ".set_reverse_video(",
            ".set_cursor_visible(",
            ".set_alternate(",
            ".set_width_method(",
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
    // without a row replies 0 to DECRQM, and SM and RM of it change none of
    // the terminal's state, compared whole; a number with a row replies
    // something else.
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
                assert!(
                    t.state == before,
                    "SM or RM {q}{n} changed the state:\n{:?}\n{:?}",
                    t.state,
                    before
                );
                checked += 1;
            }
        }
        assert_eq!(
            checked,
            256 + 65536 - u32::try_from(MODES.len()).expect("rows")
        );
    }
}
