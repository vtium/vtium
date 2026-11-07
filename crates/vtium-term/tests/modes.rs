//! The modes, driven through the byte stream: SM and RM (and DECSET and
//! DECRST) set and reset each row of the mode table, DECRQM reports it,
//! and DECSTR and RIS restore it. Each supported row's test sets and
//! resets its mode and checks the effect; the ignored and refused rows
//! stay at their fixed state.
//!
//! Each expected value is the one the mode's row in `docs/modes.md`
//! gives, from ctlseqs, the DEC manuals and ADR-1000.

mod common;

use common::{rows, term};
use vtium_term::{
    Age, Cursor, CursorShape, CursorStyle, Key, Mods, MouseButton, MouseEvent, Palette, Terminal,
    WidthMethod,
};

/// DECRQM's answer for a mode: 0 not recognised, 1 set, 2 reset, 3
/// permanently set, 4 permanently reset.
fn rqm(t: &mut Terminal, dec: bool, n: u32) -> u8 {
    t.take_replies();
    let q = if dec { "?" } else { "" };
    t.input(format!("\x1b[{q}{n}$p").as_bytes());
    let reply = String::from_utf8(t.take_replies()).unwrap();
    let want = format!("\x1b[{q}{n};");
    assert!(
        reply.starts_with(&want) && reply.ends_with("$y"),
        "DECRQM {q}{n} replied {reply:?}"
    );
    reply[want.len()..reply.len() - 2].parse().unwrap()
}

/// Sets and resets a mode, checking DECRQM after each.
fn toggles(t: &mut Terminal, dec: bool, n: u32) {
    let q = if dec { "?" } else { "" };
    t.input(format!("\x1b[{q}{n}h").as_bytes());
    assert_eq!(rqm(t, dec, n), 1, "{q}{n} set");
    t.input(format!("\x1b[{q}{n}l").as_bytes());
    assert_eq!(rqm(t, dec, n), 2, "{q}{n} reset");
}

/// The cursor as a frame draws it.
fn drawn(t: &Terminal) -> Cursor {
    t.screen().frame(&Palette::default(), Age::NEVER).cursor()
}

fn sent(t: &mut Terminal) -> Vec<u8> {
    t.take_replies()
}

// ECMA-48 7.2.10: IRM, printed text moves the rest of the line right.
// covers: ANSI-4
#[test]
fn irm_inserts_printed_text() {
    let mut t = term(8, 2, b"abc\r");
    assert_eq!(rqm(&mut t, false, 4), 2);
    t.input(b"\x1b[4hX");
    assert!(t.screen().insert_mode());
    assert_eq!(rows(t.screen())[0], "Xabc");
    toggles(&mut t, false, 4);
    t.input(b"\rY");
    assert!(!t.screen().insert_mode());
    assert_eq!(rows(t.screen())[0], "Yabc");
}

// ECMA-48 7.2.15: SRM. Reset, what the keyboard sends is also shown, as
// local echo; set, the default, it is not.
// covers: ANSI-12
#[test]
fn srm_reset_echoes_what_the_terminal_sends() {
    let mut t = term(8, 2, b"");
    assert_eq!(rqm(&mut t, false, 12), 1);
    t.paste(b"ab");
    assert_eq!(sent(&mut t), b"ab");
    assert_eq!(rows(t.screen())[0], "");
    t.input(b"\x1b[12l");
    assert_eq!(rqm(&mut t, false, 12), 2);
    t.paste(b"cd");
    assert_eq!(sent(&mut t), b"cd");
    assert_eq!(rows(t.screen())[0], "cd");
    t.input(b"\x1b[12h");
    t.paste(b"e");
    assert_eq!(rows(t.screen())[0], "cd");
    toggles(&mut t, false, 12);
}

// ECMA-48 7.2.2 (LNM, ctlseqs): a line feed also returns the carriage, and
// Return sends CR LF.
// covers: ANSI-20
#[test]
fn lnm_makes_a_line_feed_a_new_line() {
    let mut t = term(8, 3, b"ab\n");
    assert_eq!(t.screen().cursor(), (2, 1));
    t.input(b"\x1b[20hcd\n");
    assert_eq!(t.screen().cursor(), (0, 2));
    assert!(t.key(&Key::press(0xff0d, 0, Mods::empty(), None)));
    assert_eq!(sent(&mut t), b"\r\n");
    toggles(&mut t, false, 20);
    t.key(&Key::press(0xff0d, 0, Mods::empty(), None));
    assert_eq!(sent(&mut t), b"\r");
}

// ctlseqs, DECCKM: the cursor keys send SS3 forms while set.
// covers: DEC-1
#[test]
fn decckm_switches_the_cursor_keys() {
    let mut t = term(8, 2, b"");
    t.key(&Key::press(0xff52, 0, Mods::empty(), None));
    assert_eq!(sent(&mut t), b"\x1b[A");
    t.input(b"\x1b[?1h");
    assert_eq!(rqm(&mut t, true, 1), 1);
    t.key(&Key::press(0xff52, 0, Mods::empty(), None));
    t.key(&Key::press(0xff50, 0, Mods::empty(), None));
    assert_eq!(sent(&mut t), b"\x1bOA\x1bOH");
    toggles(&mut t, true, 1);
    t.key(&Key::press(0xff57, 0, Mods::empty(), None));
    assert_eq!(sent(&mut t), b"\x1b[F");
}

// ctlseqs, DECSCNM: the whole screen in reverse video.
// covers: DEC-5
#[test]
fn decscnm_reverses_the_screen() {
    let mut t = term(4, 2, b"\x1b[?5h");
    assert!(t.screen().reverse_video());
    toggles(&mut t, true, 5);
    assert!(!t.screen().reverse_video());
}

// ctlseqs, DECOM: addressing counts from the region's top. VT100 User
// Guide, DECOM: "The cursor is moved to the new home position when this
// mode is set or reset".
// covers: DEC-6
#[test]
fn decom_addresses_from_the_region() {
    let mut t = term(8, 6, b"\x1b[3;5r\x1b[?6h\x1b[2;4H");
    assert!(t.screen().origin_mode());
    assert_eq!(t.screen().cursor(), (3, 3));
    t.input(b"\x1b[3;3H\x1b[?6h");
    assert_eq!(t.screen().cursor(), (0, 2));
    t.input(b"\x1b[3;3H\x1b[?6l");
    assert_eq!(t.screen().cursor(), (0, 0));
    toggles(&mut t, true, 6);
    t.input(b"\x1b[1;1H");
    assert_eq!(t.screen().cursor(), (0, 0));
}

// ctlseqs, DECAWM: printing past the last column wraps.
// covers: DEC-7
#[test]
fn decawm_wraps_at_the_last_column() {
    let mut t = term(4, 2, b"\x1b[?7labcdef");
    assert!(!t.screen().auto_wrap());
    assert_eq!(rows(t.screen()), ["abcf", ""]);
    t.input(b"\x1b[?7h\r\x1b[Kabcdef");
    assert_eq!(rows(t.screen()), ["abcd", "ef"]);
    toggles(&mut t, true, 7);
}

// ctlseqs, DECARM: the host repeats a held key only while it is set.
// covers: DEC-8
#[test]
fn decarm_tells_the_host_to_repeat_keys() {
    let mut t = term(4, 2, b"");
    assert!(t.auto_repeat());
    t.input(b"\x1b[?8l");
    assert!(!t.auto_repeat());
    toggles(&mut t, true, 8);
    t.input(b"\x1b[?8h");
    assert!(t.auto_repeat());
}

// ctlseqs, cursor blink (mode 12): under a DECSCUSR style, set makes the
// cursor blink and reset steady, in the same shape; under the default
// style the host's blink holds, and DECRQM reports it.
// covers: DEC-12
#[test]
fn mode_12_blinks_a_chosen_cursor_style() {
    let mut t = term(4, 2, b"");
    t.set_cursor_blink(true);
    assert_eq!(rqm(&mut t, true, 12), 1);
    t.input(b"\x1b[?12l");
    assert_eq!(t.cursor_style(), CursorStyle::Default);
    assert_eq!(rqm(&mut t, true, 12), 1);
    t.input(b"\x1b[4 q");
    assert_eq!(rqm(&mut t, true, 12), 2);
    t.input(b"\x1b[?12h");
    assert_eq!(t.cursor_style(), CursorStyle::UnderlineBlink);
    assert_eq!(drawn(&t).shape, CursorShape::Underline);
    assert!(drawn(&t).blink);
    t.input(b"\x1b[?12l");
    assert_eq!(t.cursor_style(), CursorStyle::UnderlineSteady);
    assert!(!drawn(&t).blink);
    toggles(&mut t, true, 12);
    t.set_cursor_blink(false);
    t.input(b"\x1b[?12h");
    assert_eq!(rqm(&mut t, true, 12), 2);
}

// ctlseqs, DECTCEM: the cursor shows.
// covers: DEC-25
#[test]
fn dectcem_shows_and_hides_the_cursor() {
    let mut t = term(4, 2, b"\x1b[?25l");
    assert!(!t.screen().cursor_visible());
    toggles(&mut t, true, 25);
    t.input(b"\x1b[?25h");
    assert!(t.screen().cursor_visible());
}

// ctlseqs, mode 47: the alternate screen, neither cleared nor the cursor
// saved.
// covers: DEC-47
#[test]
fn mode_47_switches_screens_only() {
    let mut t = term(6, 2, b"main\x1b[?47h");
    assert!(t.screen().alternate());
    assert_eq!(t.screen().cursor(), (4, 0));
    t.input(b"\ralt");
    assert_eq!(rows(t.screen())[0], "alt");
    t.input(b"\x1b[?47l");
    assert_eq!(rows(t.screen())[0], "main");
    t.input(b"\x1b[?47h");
    assert_eq!(rows(t.screen())[0], "alt");
    toggles(&mut t, true, 47);
}

// ctlseqs, mode 1047: the alternate screen, cleared as it is left.
// covers: DEC-1047
#[test]
fn mode_1047_clears_the_alternate_screen_as_it_leaves() {
    let mut t = term(6, 2, b"main\x1b[?1047h\ralt\x1b[?1047l");
    assert!(!t.screen().alternate());
    assert_eq!(rows(t.screen())[0], "main");
    t.input(b"\x1b[?1047h");
    assert_eq!(rows(t.screen())[0], "");
    toggles(&mut t, true, 1047);
    // ctlseqs: the screen is cleared "if in the Alternate Screen Buffer",
    // so the main screen keeps its text.
    t.input(b"\x1b[?1047l");
    assert_eq!(rows(t.screen())[0], "main");
}

// ctlseqs, mode 1048: "Save cursor as in DECSC", and restore it as DECRC
// does; DECRQM reports it reset, as nothing stays set.
// covers: DEC-1048
#[test]
fn mode_1048_saves_and_restores_the_cursor_as_decsc_does() {
    let mut t = term(8, 4, b"\x1b[2;3H\x1b[1;31m\x1b[?1048h\x1b[H\x1b[m");
    assert_eq!(rqm(&mut t, true, 1048), 2);
    t.input(b"\x1b[?1048l");
    assert_eq!(t.screen().cursor(), (2, 1));
    assert!(t.screen().pen().attrs.contains(vtium_term::Attrs::BOLD));
    t.input(b"\x1b[?1048h");
    assert_eq!(rqm(&mut t, true, 1048), 2);
}

// ctlseqs, mode 1049: the cursor saved as DECSC saves it, the alternate
// screen cleared as it is entered; leaving restores the cursor and the
// pen, so the main screen erases with its own color.
// covers: DEC-1049
#[test]
fn mode_1049_saves_the_cursor_and_the_pen_around_the_alternate_screen() {
    let mut t = term(8, 3, b"main\x1b[?1049h");
    assert!(t.screen().alternate());
    assert_eq!(rows(t.screen()), ["", "", ""]);
    t.input(b"\x1b[45malt\x1b[?1049l\x1b[K");
    assert!(!t.screen().alternate());
    assert_eq!(rows(t.screen())[0], "main");
    assert_eq!(t.screen().cursor(), (4, 0));
    assert_eq!(t.screen().pen().bg, vtium_term::Color::Default);
    let line = t.screen().line(0).unwrap();
    assert_eq!(line.style(line.cell(5)).bg, vtium_term::Color::Default);
    toggles(&mut t, true, 1049);
}

// ctlseqs, mouse tracking: 9 reports presses in the X10 encoding, 1000
// releases too, 1002 drags, 1003 every motion; SM of one replaces the
// other, and RM of any clears tracking.
// covers: DEC-9, DEC-1000, DEC-1002, DEC-1003
#[test]
fn mouse_tracking_reports_by_its_mode() {
    let mut t = term(10, 5, b"");
    let left = MouseButton::Left;
    assert!(!t.mouse(1, 2, 0, 0, left, MouseEvent::Press, Mods::empty()));
    t.input(b"\x1b[?9h");
    assert_eq!(rqm(&mut t, true, 9), 1);
    t.mouse(1, 2, 0, 0, left, MouseEvent::Press, Mods::SHIFT);
    t.mouse(1, 2, 0, 0, left, MouseEvent::Release, Mods::empty());
    assert_eq!(sent(&mut t), b"\x1b[M\x20\x22\x23");
    t.input(b"\x1b[?1000h");
    assert_eq!(rqm(&mut t, true, 9), 2);
    assert_eq!(rqm(&mut t, true, 1000), 1);
    t.mouse(1, 2, 0, 0, left, MouseEvent::Press, Mods::CTRL);
    t.mouse(1, 2, 0, 0, left, MouseEvent::Release, Mods::empty());
    t.mouse(3, 2, 0, 0, left, MouseEvent::Move, Mods::empty());
    assert_eq!(sent(&mut t), b"\x1b[M\x30\x22\x23\x1b[M\x23\x22\x23");
    t.input(b"\x1b[?1002h");
    t.mouse(3, 2, 0, 0, left, MouseEvent::Move, Mods::empty());
    t.mouse(
        4,
        2,
        0,
        0,
        MouseButton::None,
        MouseEvent::Move,
        Mods::empty(),
    );
    assert_eq!(sent(&mut t), b"\x1b[M\x40\x24\x23");
    t.input(b"\x1b[?1003h");
    t.mouse(
        4,
        3,
        0,
        0,
        MouseButton::None,
        MouseEvent::Move,
        Mods::empty(),
    );
    t.mouse(
        4,
        3,
        0,
        0,
        MouseButton::None,
        MouseEvent::Move,
        Mods::empty(),
    );
    assert_eq!(sent(&mut t), b"\x1b[M\x43\x25\x24");
    toggles(&mut t, true, 9);
    toggles(&mut t, true, 1000);
    toggles(&mut t, true, 1002);
    toggles(&mut t, true, 1003);
    assert!(!t.mouse(1, 1, 0, 0, left, MouseEvent::Press, Mods::empty()));
}

// ctlseqs, SGR mouse (1006) and SGR-Pixels (1016), each independent of the
// order the modes were set in, pixels counted from 1; RM of an encoding
// clears only itself.
// covers: DEC-1006, DEC-1016
#[test]
fn sgr_mouse_encodings_report_cells_or_pixels() {
    let mut t = term(10, 5, b"\x1b[?1006h\x1b[?1000h");
    t.mouse(
        1,
        2,
        0,
        0,
        MouseButton::Right,
        MouseEvent::Press,
        Mods::empty(),
    );
    t.mouse(
        1,
        2,
        0,
        0,
        MouseButton::Right,
        MouseEvent::Release,
        Mods::ALT,
    );
    t.mouse(
        1,
        2,
        0,
        0,
        MouseButton::WheelUp,
        MouseEvent::Press,
        Mods::empty(),
    );
    assert_eq!(sent(&mut t), b"\x1b[<2;2;3M\x1b[<10;2;3m\x1b[<64;2;3M");
    t.input(b"\x1b[?1016h");
    assert_eq!(rqm(&mut t, true, 1006), 2);
    assert_eq!(rqm(&mut t, true, 1016), 1);
    t.mouse(
        1,
        2,
        0,
        7,
        MouseButton::Left,
        MouseEvent::Press,
        Mods::empty(),
    );
    assert_eq!(sent(&mut t), b"\x1b[<0;1;8M");
    t.input(b"\x1b[?1006l");
    assert_eq!(rqm(&mut t, true, 1016), 1);
    toggles(&mut t, true, 1006);
    toggles(&mut t, true, 1016);
    t.mouse(
        1,
        2,
        0,
        0,
        MouseButton::Left,
        MouseEvent::Press,
        Mods::empty(),
    );
    assert_eq!(sent(&mut t), b"\x1b[M\x20\x22\x23");
}

// ctlseqs, FocusIn/FocusOut (1004): reported while set; setting it sends
// nothing.
// covers: DEC-1004
#[test]
fn focus_events_are_reported_while_set() {
    let mut t = term(4, 2, b"");
    assert!(!t.focus(true));
    t.input(b"\x1b[?1004h");
    assert_eq!(sent(&mut t), b"");
    assert!(t.focus(false));
    assert!(t.focus(true));
    assert_eq!(sent(&mut t), b"\x1b[O\x1b[I");
    toggles(&mut t, true, 1004);
    assert!(!t.focus(true));
}

// ctlseqs, bracketed paste (2004): a paste is bracketed by CSI 200 ~ and
// CSI 201 ~ while set.
// covers: DEC-2004
#[test]
fn bracketed_paste_brackets_a_paste() {
    let mut t = term(4, 2, b"");
    t.paste(b"x");
    assert_eq!(sent(&mut t), b"x");
    t.input(b"\x1b[?2004h");
    t.paste(b"a\rb");
    assert_eq!(sent(&mut t), b"\x1b[200~a\rb\x1b[201~");
    toggles(&mut t, true, 2004);
    t.paste(b"y");
    assert_eq!(sent(&mut t), b"y");
}

// contour synchronized-output (2026): offered only while the host presents
// by it; a hold starts at SM and ends at RM, each end counted, and the
// input call returns after the byte that ended it.
// covers: DEC-2026
#[test]
fn mode_2026_holds_frames_while_the_host_offers_it() {
    let mut t = term(4, 2, b"\x1b[?2026h");
    assert!(!t.synchronized());
    assert_eq!(rqm(&mut t, true, 2026), 4);
    t.set_sync_enabled(true);
    assert_eq!(rqm(&mut t, true, 2026), 2);
    t.input(b"\x1b[?2026h");
    assert!(t.synchronized());
    let stream = b"ab\x1b[?2026lcd";
    let taken = t.input(stream);
    assert_eq!(taken, stream.len() - 2);
    assert!(!t.synchronized());
    assert_eq!(t.sync_frames(), 1);
    assert_eq!(t.input(&stream[taken..]), 2);
    toggles(&mut t, true, 2026);
    assert_eq!(t.sync_frames(), 2);
    t.input(b"\x1b[?2026h");
    t.set_sync_enabled(false);
    assert!(!t.synchronized());
    assert_eq!(t.sync_frames(), 3);
}

// foot(7): DCS = 1 s and DCS = 2 s begin and end the same hold as mode
// 2026.
// covers: ctl.sync-update
#[test]
fn a_dcs_synchronized_update_holds_as_mode_2026() {
    let mut t = term(4, 2, b"");
    t.set_sync_enabled(true);
    t.input(b"\x1bP=1s\x1b\\");
    assert!(t.synchronized());
    assert_eq!(rqm(&mut t, true, 2026), 1);
    t.input(b"\x1bP=2s\x18");
    assert!(t.synchronized());
    t.input(b"\x1bP=2s\x1b\\");
    assert!(!t.synchronized());
    assert_eq!(t.sync_frames(), 1);
}

// terminal-unicode-core (2027): reset cuts text by codepoint width, and
// RIS and DECSTR return it to the configured method.
// covers: DEC-2027
#[test]
fn mode_2027_reports_and_resets_to_the_configured_method() {
    let mut t = term(8, 2, b"");
    assert_eq!(rqm(&mut t, true, 2027), 1);
    toggles(&mut t, true, 2027);
    assert_eq!(t.screen().width_method(), WidthMethod::Wcwidth);
    t.set_default_width_method(WidthMethod::Wcwidth);
    t.input(b"\x1b[!p");
    assert_eq!(t.screen().width_method(), WidthMethod::Wcwidth);
    t.set_default_width_method(WidthMethod::Unicode);
    t.input(b"\x1bc");
    assert_eq!(rqm(&mut t, true, 2027), 1);
}

// ADR-1000, "The target set": an ignored or refused mode answers DECRQM
// with its fixed default, 3 for DECANM and 4 for the rest, and SM and RM
// change nothing.
// covers: ANSI-2, DEC-2, DEC-3, DEC-4, DEC-18, DEC-19, DEC-42, DEC-1001
#[test]
fn ignored_and_refused_modes_answer_their_fixed_state_and_change_nothing() {
    let rows: [(bool, u32, u8); 8] = [
        (false, 2, 4),
        (true, 2, 3),
        (true, 3, 4),
        (true, 4, 4),
        (true, 18, 4),
        (true, 19, 4),
        (true, 42, 4),
        (true, 1001, 4),
    ];
    let mut t = term(6, 3, b"ab\x1b[1;31m");
    let before = format!("{t:?}");
    for (dec, n, fixed) in rows {
        let q = if dec { "?" } else { "" };
        t.input(format!("\x1b[{q}{n}h").as_bytes());
        assert_eq!(rqm(&mut t, dec, n), fixed, "{q}{n} set");
        t.input(format!("\x1b[{q}{n}l").as_bytes());
        assert_eq!(rqm(&mut t, dec, n), fixed, "{q}{n} reset");
        assert_eq!(format!("{t:?}"), before, "{q}{n} changed the state");
    }
    // KAM set would lock the keyboard; it does not.
    t.input(b"\x1b[2h");
    assert!(t.key(&Key::press(0x61, 0x61, Mods::empty(), Some('a'))));
    assert_eq!(sent(&mut t), b"a");
}

// ctlseqs, DECSTR: a soft reset returns the modes, the pen, the cursor
// style and the saved cursor to their defaults, and leaves the text and the
// cursor. DECSTR also resets the mouse modes and 1004, which xterm resets
// only on RIS: the maintainer's decision, recorded in each mode's row.
// covers: ctl.decstr
#[test]
fn decstr_returns_the_modes_to_their_defaults() {
    let set = b"\x1b[4h\x1b[20h\x1b[?1h\x1b[?6h\x1b[?7l\x1b[?25l\x1b[?1000h\x1b[?1006h\
                \x1b[?1004h\x1b[?2004h\x1b[?8l\x1b[1;31m\x1b[3 q\x1b=\x1b[>4;2m";
    let mut t = term(8, 4, b"hello");
    t.input(set);
    t.input(b"\x1b[2;1H\x1b[!p");
    for (dec, n, v) in [
        (false, 4, 2),
        (false, 20, 2),
        (true, 1, 2),
        (true, 6, 2),
        (true, 7, 1),
        (true, 25, 1),
        (true, 1000, 2),
        (true, 1006, 2),
        (true, 1004, 2),
        (true, 2004, 2),
        (true, 8, 1),
    ] {
        assert_eq!(rqm(&mut t, dec, n), v, "mode {n}");
    }
    assert_eq!(t.screen().pen(), &vtium_term::Style::DEFAULT);
    assert_eq!(t.cursor_style(), CursorStyle::Default);
    assert_eq!(rows(t.screen())[0], "hello");
    assert_eq!(t.screen().cursor(), (0, 1));
    t.key(&Key::press(0xffb1, 0, Mods::empty(), None));
    t.input(b"\x1b[?4m");
    assert_eq!(sent(&mut t), b"1\x1b[>4;0m");
    // DECRC after DECSTR goes home with the default pen.
    t.input(b"\x1b8");
    assert_eq!(t.screen().cursor(), (0, 0));
}

// ctlseqs, RIS: DECSTR's reset, and the screen and scrollback erased and
// the cursor home.
// covers: ctl.decstr
#[test]
fn ris_erases_and_goes_home() {
    let mut t = term(8, 3, b"hello\x1b[?6h\x1b[2;3H");
    t.input(b"\x1bc");
    assert_eq!(rows(t.screen()), ["", "", ""]);
    assert_eq!(t.screen().cursor(), (0, 0));
    assert_eq!(rqm(&mut t, true, 6), 2);
}

// ctlseqs, XTREVWRAP: while DECAWM is set, BS and CUB go on from the first
// column to the last column of the row above when printing wrapped from
// that row; a pending wrap takes the first step without moving, as xterm's
// `CursorBack` counts it. A row the row above did not wrap into, and the
// screen's first row, stop the cursor at the first column.
// covers: DEC-45
#[test]
fn mode_45_wraps_backwards_into_the_row_that_wrapped() {
    let mut t = term(4, 3, b"abcdefg");
    assert_eq!(rqm(&mut t, true, 45), 2);
    t.input(b"\r\x08");
    assert_eq!(t.screen().cursor(), (0, 1), "reset, BS stops");
    t.input(b"\x1b[?45h");
    assert_eq!(rqm(&mut t, true, 45), 1);
    t.input(b"\x08");
    assert_eq!(t.screen().cursor(), (3, 0), "BS into the row that wrapped");
    t.input(b"\x08");
    assert_eq!(t.screen().cursor(), (2, 0));
    // CUB counts the wrap as one step: from column 1, three steps reach
    // the first column, the row above's last, and the one before it.
    t.input(b"\x1b[2;2H\x1b[3D");
    assert_eq!(t.screen().cursor(), (2, 0), "CUB through the wrap");
    // The top row has no row above.
    t.input(b"\x1b[1;1H\x08");
    assert_eq!(t.screen().cursor(), (0, 0));
    // A row the program reached by a newline was not wrapped into.
    t.input(b"\x1b[2J\x1b[Hab\r\ncd\r\x08");
    assert_eq!(t.screen().cursor(), (0, 1), "no wrap into this row");
    // A pending wrap takes the first step: BS lands on the last column.
    t.input(b"\x1b[2J\x1b[Habcd");
    assert_eq!(t.screen().cursor(), (4, 0));
    t.input(b"\x08");
    assert_eq!(t.screen().cursor(), (3, 0), "the pending wrap is the step");
    // CUB 2 from a pending wrap ends one column left of the last, where
    // xterm counts the pending wrap twice and ends on the last column.
    t.input(b"\x1b[2J\x1b[Habcd\x1b[2D");
    assert_eq!(t.screen().cursor(), (2, 0), "CUB n from a pending wrap");
    // With DECAWM reset the mode does nothing.
    t.input(b"\x1b[2J\x1b[Habcdefg\x1b[?7l\r\x08");
    assert_eq!(t.screen().cursor(), (0, 1), "DECAWM reset");
    t.input(b"\x1b[?7h");
    toggles(&mut t, true, 45);
    t.input(b"\x1b[2;1H\x08");
    assert_eq!(t.screen().cursor(), (0, 1), "reset again, BS stops");
}

// ctlseqs, DECNKM: the keypad's application mode, the state DECKPAM sets
// and DECKPNM resets.
// covers: DEC-66
#[test]
fn decnkm_switches_the_keypad_as_deckpam_and_deckpnm_do() {
    const KP_1: u32 = 0xffb1;
    let mut t = term(8, 2, b"");
    assert_eq!(rqm(&mut t, true, 66), 2);
    t.key(&Key::press(KP_1, 0, Mods::empty(), None));
    assert_eq!(sent(&mut t), b"1");
    t.input(b"\x1b[?66h");
    assert_eq!(rqm(&mut t, true, 66), 1);
    t.key(&Key::press(KP_1, 0, Mods::empty(), None));
    assert_eq!(sent(&mut t), b"\x1bOq");
    t.input(b"\x1b>");
    assert_eq!(rqm(&mut t, true, 66), 2, "DECKPNM resets it");
    t.input(b"\x1b=");
    assert_eq!(rqm(&mut t, true, 66), 1, "DECKPAM sets it");
    toggles(&mut t, true, 66);
    t.key(&Key::press(KP_1, 0, Mods::empty(), None));
    assert_eq!(sent(&mut t), b"1");
}

// ctlseqs, alternate scroll (1007), xterm's `alternateScroll`: on the
// alternate screen a wheel scroll the host does not report is sent as
// cursor keys, one a line, as DECCKM encodes them; on the main screen,
// while the program tracks the pointer, or reset, nothing is sent.
// covers: DEC-1007
#[test]
fn mode_1007_sends_wheel_steps_as_cursor_keys_on_the_alternate_screen() {
    let mut t = term(8, 3, b"");
    assert_eq!(rqm(&mut t, true, 1007), 1, "set at start");
    assert!(!t.alternate_scroll(true, 2), "the main screen scrolls");
    assert_eq!(sent(&mut t), b"");
    t.input(b"\x1b[?1049h");
    assert!(t.alternate_scroll(true, 2));
    assert!(t.alternate_scroll(false, 1));
    assert_eq!(sent(&mut t), b"\x1b[A\x1b[A\x1b[B");
    t.input(b"\x1b[?1h");
    assert!(t.alternate_scroll(true, 1));
    assert_eq!(sent(&mut t), b"\x1bOA");
    // At most the screen's rows.
    assert!(t.alternate_scroll(false, 1000));
    assert_eq!(sent(&mut t), b"\x1bOB\x1bOB\x1bOB");
    // While the program tracks the pointer, the wheel is its to report.
    t.input(b"\x1b[?1000h");
    assert!(!t.alternate_scroll(true, 1), "tracking on");
    assert_eq!(sent(&mut t), b"");
    t.input(b"\x1b[?1000l");
    toggles(&mut t, true, 1007);
    assert!(!t.alternate_scroll(true, 2), "reset, the host scrolls");
    assert_eq!(sent(&mut t), b"");
}

// ctlseqs, urxvt mouse (1015): `CSI Cb ; Cx ; Cy M` in decimal, Cb as the
// X10 encoding's byte, so a release is button 3, and the cell counted from
// 1 with no limit at 223. One encoding is set at a time.
// covers: DEC-1015
#[test]
fn urxvt_mouse_encoding_reports_in_decimal() {
    let mut t = term(400, 5, b"\x1b[?1000h");
    assert_eq!(rqm(&mut t, true, 1015), 2);
    t.input(b"\x1b[?1015h");
    assert_eq!(rqm(&mut t, true, 1015), 1);
    let left = MouseButton::Left;
    t.mouse(1, 2, 0, 0, left, MouseEvent::Press, Mods::SHIFT);
    t.mouse(1, 2, 0, 0, left, MouseEvent::Release, Mods::empty());
    t.mouse(
        299,
        4,
        0,
        0,
        MouseButton::WheelUp,
        MouseEvent::Press,
        Mods::empty(),
    );
    assert_eq!(sent(&mut t), b"\x1b[36;2;3M\x1b[35;2;3M\x1b[96;300;5M");
    // SGR replaces it, and RM of 1015 then leaves SGR set.
    t.input(b"\x1b[?1006h");
    assert_eq!(rqm(&mut t, true, 1015), 2);
    t.input(b"\x1b[?1015l");
    assert_eq!(rqm(&mut t, true, 1006), 1);
    t.input(b"\x1b[?1006l");
    toggles(&mut t, true, 1015);
    t.mouse(1, 2, 0, 0, left, MouseEvent::Press, Mods::empty());
    assert_eq!(sent(&mut t), b"\x1b[M\x20\x22\x23", "X10 once reset");
}

// ctlseqs, metaSendsEscape (1036) and eightBitInput (1034), as xterm's
// `Input` chooses between them: ESC before the key while 1036 is set; else
// the eighth bit of a one-byte key below 0x80, sent as a character in the
// input's encoding; else Alt sends nothing of its own.
// covers: DEC-1034, DEC-1036
#[test]
fn modes_1034_and_1036_choose_what_alt_sends() {
    let alt_a = |t: &mut Terminal| {
        t.key(&Key::press(0x61, 0x61, Mods::ALT, Some('a')));
        sent(t)
    };
    let mut t = term(8, 2, b"");
    assert_eq!(rqm(&mut t, true, 1036), 1, "1036 set at start");
    assert_eq!(rqm(&mut t, true, 1034), 1, "1034 set at start");
    assert_eq!(alt_a(&mut t), b"\x1ba");
    t.input(b"\x1b[?1036l");
    assert_eq!(rqm(&mut t, true, 1036), 2);
    assert_eq!(
        alt_a(&mut t),
        "\u{e1}".as_bytes(),
        "the eighth bit, as UTF-8"
    );
    t.key(&Key::press(0xff0d, 0, Mods::ALT, None));
    assert_eq!(sent(&mut t), "\u{8d}".as_bytes(), "Alt+Return is CR's");
    t.key(&Key::press(0xe9, 0xe9, Mods::ALT, Some('\u{e9}')));
    assert_eq!(
        sent(&mut t),
        "\u{e9}".as_bytes(),
        "two bytes: Alt does nothing"
    );
    t.input(b"\x1b[?1034l");
    assert_eq!(rqm(&mut t, true, 1034), 2);
    assert_eq!(alt_a(&mut t), b"a", "neither set");
    // 8-bit input sends the byte itself, and 7-bit input cannot carry it.
    t.input(b"\x1b[62;1\"p\x1b[?1036l");
    assert_eq!(alt_a(&mut t), b"\xe1");
    t.input(b"\x1b[61\"p\x1b[?1036l");
    assert_eq!(alt_a(&mut t), b"a");
    t.input(b"\x1b[!p");
    toggles(&mut t, true, 1034);
    toggles(&mut t, true, 1036);
    assert_eq!(alt_a(&mut t), b"a", "both reset");
}

// ctlseqs, numLock (1035), as xterm's `Input` reads it: while NumLock is
// on, a keypad key that makes a character sends it whatever DECKPAM says;
// keypad keys that make none keep their application sequences.
// covers: DEC-1035
#[test]
fn mode_1035_lets_numlock_override_the_application_keypad() {
    const KP_1: u32 = 0xffb1;
    const KP_ENTER: u32 = 0xff8d;
    const KP_INSERT: u32 = 0xff9e;
    const KP_F1: u32 = 0xff91;
    let num = Mods::NUM;
    let mut t = term(8, 2, b"\x1b=");
    assert_eq!(rqm(&mut t, true, 1035), 1, "set at start");
    t.key(&Key::press(KP_1, 0, Mods::empty(), None));
    assert_eq!(sent(&mut t), b"\x1bOq", "NumLock off");
    t.key(&Key::press(KP_1, 0, num, None));
    t.key(&Key::press(KP_ENTER, 0, num, None));
    assert_eq!(sent(&mut t), b"1\r", "NumLock on");
    t.key(&Key::press(KP_INSERT, 0, num, None));
    t.key(&Key::press(KP_F1, 0, num, None));
    assert_eq!(
        sent(&mut t),
        b"\x1b[2~\x1bOP",
        "keys that make no character"
    );
    t.input(b"\x1b[?1035l");
    assert_eq!(rqm(&mut t, true, 1035), 2);
    t.key(&Key::press(KP_1, 0, num, None));
    assert_eq!(sent(&mut t), b"\x1bOq", "reset, NumLock reads as nothing");
    toggles(&mut t, true, 1035);
    t.key(&Key::press(KP_1, 0, num, None));
    assert_eq!(sent(&mut t), b"\x1bOq");
}

// ctlseqs, bellIsUrgent (1042): vtium has no window to mark urgent, so
// while it is set a BEL reaches the host, which rings the VT bell by its
// bell setting, and while it is reset a BEL does nothing. A BEL that ends
// an OSC string is not one.
// covers: DEC-1042
#[test]
fn mode_1042_lets_bel_reach_the_host() {
    let mut t = term(8, 2, b"");
    assert_eq!(rqm(&mut t, true, 1042), 1, "set at start");
    assert!(!t.take_bell());
    t.input(b"a\x07b\x07");
    assert!(t.take_bell(), "two BELs are one bell");
    assert!(!t.take_bell(), "taken");
    t.input(b"\x1b]2;title\x07");
    assert!(!t.take_bell(), "the end of an OSC string");
    t.input(b"\x1b[?1042l\x07");
    assert_eq!(rqm(&mut t, true, 1042), 2);
    assert!(!t.take_bell(), "reset, BEL does nothing");
    toggles(&mut t, true, 1042);
    t.input(b"\x07");
    assert!(!t.take_bell());
}

/// The bidi modes, which only a build with the bidi feature has (ADR-1004
/// D13): BDSM, 1243, 2500 and 2501.
const BIDI_MODES: [(bool, u32); 4] = [(false, 8), (true, 1243), (true, 2500), (true, 2501)];

/// The supported rows of `docs/modes.tsv`, which the mode table generates:
/// whether each is a DEC private mode, and its number. A build without
/// images has no rows for sixel's modes, and a build without the bidi
/// feature none for the bidi modes, so it leaves them out.
fn supported_modes() -> Vec<(bool, u32)> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/modes.tsv");
    let tsv = std::fs::read_to_string(path).unwrap();
    let mut lines = tsv.lines();
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    let col = |name: &str| header.iter().position(|h| *h == name).unwrap();
    let (kind, number, status, host) = (col("kind"), col("number"), col("status"), col("host"));
    let modes: Vec<(bool, u32)> = lines
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|r| r[status] == "supported")
        .filter(|r| cfg!(feature = "images") || r[host] != "offers sixel")
        .map(|r| (r[kind] == "DEC", r[number].parse().unwrap()))
        .filter(|m| cfg!(feature = "bidi") || !BIDI_MODES.contains(m))
        .collect();
    assert!(modes.len() > 30, "the table's supported rows");
    modes
}

/// A terminal where every supported mode can change: the host offers mode
/// 2026, the cursor has a style mode 12 can make blink or not, and in a
/// build with images the host offers sixel, whose modes 80, 1070 and 8452
/// report their state only then.
fn every_mode_can_change() -> Terminal {
    let mut t = term(8, 4, b"\x1b[2 q");
    t.set_sync_enabled(true);
    #[cfg(feature = "images")]
    {
        use vtium_term::ImageString;
        use vtium_term::images::{Sixel, Switches};
        t.screen_mut().set_image_limit(1 << 20);
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Sixel, Some(Box::new(Sixel::default())));
    }
    t
}

/// SM of a mode DECRQM reports reset, and RM of one it reports set.
fn flip(t: &mut Terminal, dec: bool, n: u32) {
    let q = if dec { "?" } else { "" };
    let f = if rqm(t, dec, n) == 1 { 'l' } else { 'h' };
    t.input(format!("\x1b[{q}{n}{f}").as_bytes());
    t.take_replies();
}

// ctlseqs, XTSAVE and XTRESTORE: XTRESTORE of a mode restores the state
// XTSAVE of it saved, for every supported DEC private mode, from either
// state. Mode 1048 holds no state: XTSAVE of it saves the cursor and
// XTRESTORE restores it, as DECSC and DECRC do (xterm's `savemodes`).
// covers: ctl.xtsave
#[test]
fn xtrestore_restores_each_mode_xtsave_saved() {
    for (dec, n) in supported_modes().into_iter().filter(|m| m.0 && m.1 != 1048) {
        for start_flipped in [false, true] {
            let mut t = every_mode_can_change();
            if start_flipped {
                flip(&mut t, true, n);
            }
            let saved = rqm(&mut t, dec, n);
            t.input(format!("\x1b[?{n}s").as_bytes());
            flip(&mut t, true, n);
            assert_ne!(rqm(&mut t, true, n), saved, "mode {n} did not change");
            t.input(format!("\x1b[?{n}r").as_bytes());
            t.take_replies();
            assert_eq!(rqm(&mut t, true, n), saved, "XTRESTORE of {n}");
        }
    }
    let mut t = term(8, 4, b"\x1b[2;3H");
    t.input(b"\x1b[?1048s\x1b[4;5H\x1b[?1048r");
    assert_eq!(t.screen().cursor(), (2, 1), "1048 restores the cursor");
}

// ctlseqs, XTSAVE, as xterm's `savemodes` keeps the slots: the tracking
// modes 1000 to 1003 share one saved state, while mode 9 has its own, and
// the three encodings share one, as do the three alternate screen modes.
// covers: ctl.xtsave
#[test]
fn xtsave_keeps_one_state_for_modes_that_share_one() {
    let mut t = term(
        8,
        4,
        b"\x1b[?1000h\x1b[?1006h\x1b[?1000;1006s\x1b[?1003h\x1b[?1016h",
    );
    t.input(b"\x1b[?1002;1015r");
    assert_eq!(rqm(&mut t, true, 1000), 1, "1002's restore is 1000's slot");
    assert_eq!(rqm(&mut t, true, 1003), 2);
    assert_eq!(rqm(&mut t, true, 1006), 1, "1015's restore is 1006's slot");
    t.input(b"\x1b[?1003h\x1b[?1003s\x1b[?1000h\x1b[?1000r");
    assert_eq!(rqm(&mut t, true, 1003), 1, "1000's restore is 1003's slot");
    t.input(b"\x1b[?9h\x1b[?9s\x1b[?1000h\x1b[?1000s\x1b[?9r");
    assert_eq!(rqm(&mut t, true, 9), 1, "9 has a slot of its own");
    t.input(b"\x1b[?1002r");
    assert_eq!(rqm(&mut t, true, 1000), 1, "1000 to 1003 kept theirs");
    // The alternate screen comes back as mode 47 switches it, uncleared.
    t.input(b"\x1b[?1049h\x1b[?47s\x1b[?1049lmain\x1b[?1049h\x1b[Halt\x1b[?47s\x1b[?47l");
    assert_eq!(rows(t.screen())[0], "main");
    t.input(b"\x1b[?1049r");
    assert!(t.screen().alternate());
    assert_eq!(rows(t.screen())[0], "alt", "restored, not cleared");
}

// The maintainer's ruling, as Ghostty splits the resets: XTRESTORE of a
// mode nothing was saved for leaves it as it is; DECSTR keeps what XTSAVE
// saved, and RIS clears it, where xterm keeps it through both.
// covers: ctl.xtsave
#[test]
fn decstr_keeps_what_xtsave_saved_and_ris_clears_it() {
    // Mode 25 is set at start, and nothing was saved for it.
    let mut t = term(8, 4, b"\x1b[?25r");
    assert_eq!(rqm(&mut t, true, 25), 1, "nothing saved: the mode stays");
    t.input(b"\x1b[?25l\x1b[?25s\x1b[!p");
    assert_eq!(rqm(&mut t, true, 25), 1, "DECSTR sets 25");
    t.input(b"\x1b[?25r");
    assert_eq!(rqm(&mut t, true, 25), 2, "DECSTR kept what was saved");
    t.input(b"\x1b[?25s\x1bc");
    assert_eq!(rqm(&mut t, true, 25), 1, "RIS sets 25");
    t.input(b"\x1b[?25r");
    assert_eq!(rqm(&mut t, true, 25), 1, "RIS cleared what was saved");
}

// kitty's misc-protocol: XTSAVE and XTRESTORE with an empty list save and
// restore every mode free of side effects, the maintainer's ruling: every
// supported DEC private mode but DECOM (6), which moves the cursor, 47,
// 1047 and 1049, which switch screens, 1048, which holds no state, and
// 2026, which holds frames. The ANSI modes are not in it. A mode left out
// keeps what it is when the list is restored.
// covers: ctl.xtsave
#[test]
fn the_empty_list_saves_and_restores_every_mode() {
    const LEFT_OUT: [(bool, u32); 10] = [
        (false, 4),
        (false, 8),
        (false, 12),
        (false, 20),
        (true, 6),
        (true, 47),
        (true, 1047),
        (true, 1048),
        (true, 1049),
        (true, 2026),
    ];
    let modes = supported_modes();
    let mut t = every_mode_can_change();
    let report =
        |t: &mut Terminal| -> Vec<u8> { modes.iter().map(|&(dec, n)| rqm(t, dec, n)).collect() };
    let saved = report(&mut t);
    t.input(b"\x1b[?s");
    for &(dec, n) in modes.iter().filter(|m| m.1 != 1048) {
        flip(&mut t, dec, n);
    }
    let flipped = report(&mut t);
    // The last of each group that shares a state decides it.
    let unchanged: Vec<u32> = modes
        .iter()
        .zip(saved.iter().zip(&flipped))
        .filter(|(_, (a, b))| a == b)
        .map(|(m, _)| m.1)
        .collect();
    assert_eq!(unchanged, [9, 1000, 1002, 1006, 1015, 1048]);
    t.input(b"\x1b[?r");
    t.take_replies();
    let restored = report(&mut t);
    for (i, mode) in modes.iter().enumerate() {
        let want = if LEFT_OUT.contains(mode) {
            flipped[i]
        } else {
            saved[i]
        };
        assert_eq!(
            restored[i], want,
            "mode {mode:?} after the empty list's restore"
        );
    }
}

// The empty list leaves the cursor and the screen alone: its restore does
// not move the cursor home as DECOM does, does not switch back to a screen
// it was saved on, and does not save or restore the cursor as 1048 does.
// covers: ctl.xtsave
#[test]
fn the_empty_list_leaves_the_cursor_and_the_screen_alone() {
    let mut t = term(8, 4, b"\x1b[?s\x1b[?6h\x1b[3;4H\x1b[?r");
    assert_eq!(t.screen().cursor(), (3, 2), "DECOM's restore would go home");
    assert_eq!(rqm(&mut t, true, 6), 1);
    let t = term(8, 4, b"\x1b[?1049h\x1b[?s\x1b[?1049l\x1b[?r");
    assert!(!t.screen().alternate(), "saved on the alternate screen");
    let mut t = term(8, 4, b"\x1b[2;2H\x1b7\x1b[3;3H\x1b[?s\x1b[4;4H\x1b[?r");
    assert_eq!(t.screen().cursor(), (3, 3), "the restore leaves the cursor");
    t.input(b"\x1b8");
    assert_eq!(t.screen().cursor(), (1, 1), "the save left DECSC's cursor");
}

// ctlseqs, XTSHIFTESCAPE, as Ghostty's page reads it: `CSI > 1 s` asks
// that Shift reach the program's mouse reports, and `CSI > 0 s` or an
// omitted value lets the host take a Shift-click for its own selection;
// another value does nothing. DECSTR keeps it and RIS returns it to 0, the
// maintainer's ruling, as Ghostty splits the resets.
// covers: input.xtshiftescape
#[test]
fn xtshiftescape_says_whether_shift_reaches_the_mouse_reports() {
    let mut t = term(8, 4, b"\x1b[?1000h\x1b[?1006h");
    assert!(!t.mouse_captures_shift());
    t.input(b"\x1b[>2s");
    assert!(!t.mouse_captures_shift(), "2 from 0 does nothing");
    t.input(b"\x1b[>1s");
    assert!(t.mouse_captures_shift());
    t.mouse(
        1,
        2,
        0,
        0,
        MouseButton::Left,
        MouseEvent::Press,
        Mods::SHIFT,
    );
    assert_eq!(sent(&mut t), b"\x1b[<4;2;3M", "Shift in the report");
    t.input(b"\x1b[>2s");
    assert!(t.mouse_captures_shift(), "2 is not a value a program sets");
    t.input(b"\x1b[>s");
    assert!(!t.mouse_captures_shift(), "omitted is 0");
    t.input(b"\x1b[>1s\x1b[>0s");
    assert!(!t.mouse_captures_shift());
    t.input(b"\x1b[>1s\x1b[!p");
    assert!(t.mouse_captures_shift(), "DECSTR keeps it");
    t.input(b"\x1bc");
    assert!(!t.mouse_captures_shift(), "RIS returns it to 0");
    t.input(b"\x1b[>1s\x1b[?1s");
    assert!(t.mouse_captures_shift(), "a private marker makes XTSAVE");
}

// ctlseqs and the VT510 manual, DECALN, as xterm runs it: every cell an E
// in the default style, the margins the whole screen, origin mode off and
// the cursor home with no pending wrap, every row unwrapped and every cell
// marked changed. The pen keeps its colors and DECSCA's protection, as
// xterm's `resetRendition` keeps them, and loses its other attributes and
// its underline color, which xterm does not have and Ghostty drops.
// covers: ctl.decaln
#[test]
fn decaln_fills_the_screen_with_e_and_resets_the_margins_and_cursor() {
    let mut t = term(
        5,
        4,
        b"ab\x1b[1;4;31;44;58;5;3m\x1b[1\"q\x1b[2;3r\x1b[?6h\x1b[2;5Hxyz",
    );
    assert_eq!(t.screen().margins(), (1, 2));
    let wrapped = |t: &Terminal, row| {
        let line = t.screen().line(row).unwrap();
        line.flags().contains(vtium_term::LineFlags::WRAPPED)
    };
    assert!(wrapped(&t, 1), "row 1 wrapped into row 2");
    let seen = t.screen_mut().checkpoint();
    t.input(b"\x1b#8");
    assert_eq!(rows(t.screen()), ["EEEEE"; 4]);
    assert_eq!(t.screen().margins(), (0, 3));
    assert_eq!(t.screen().cursor(), (0, 0));
    assert_eq!(rqm(&mut t, true, 6), 2, "origin mode off");
    let palette = Palette::default();
    let frame = t.screen().frame(&palette, seen);
    for row in 0..4 {
        assert!(!wrapped(&t, row), "row {row} unwrapped");
        let line = t.screen().line(row).unwrap();
        for col in 0..5 {
            assert_eq!(line.style(line.cell(col)), vtium_term::Style::DEFAULT);
            assert!(frame.cell(row, col).unwrap().changed, "{row},{col} marked");
        }
    }
    let pen = t.screen().pen();
    assert_eq!(pen.fg, vtium_term::Color::Indexed(1));
    assert_eq!(pen.bg, vtium_term::Color::Indexed(4));
    assert_eq!(pen.attrs, vtium_term::Attrs::PROTECTED);
    assert_eq!(pen.underline_color, vtium_term::Color::Default);
    // With no wrap left, mode 45's BS stops at the first column.
    t.input(b"\x1b[?45h\x1b[3;1H\x08");
    assert_eq!(t.screen().cursor(), (0, 2));
    // The region is the whole screen: a line feed at the last row scrolls.
    t.input(b"\x1b[4;1H\n");
    assert_eq!(rows(t.screen()), ["EEEEE", "EEEEE", "EEEEE", ""]);
}

// ctlseqs, DECRQM: a mode vtium always or never has answers that state as
// permanent, 3 for one always set and 4 for one always reset, and SM and RM
// change nothing. vtium always scrolls the view to the bottom on a key
// press (1011) and always allows the alternate screen (1046); it never
// switches to 132 columns (40), never encodes the mouse in UTF-8 (1005),
// which SGR mouse replaces, and keeps the view where it is while output
// arrives (1010).
// covers: DEC-40, DEC-1005, DEC-1010, DEC-1011, DEC-1046
#[test]
fn modes_vtium_always_or_never_has_answer_their_fixed_state_and_change_nothing() {
    let rows: [(u32, u8); 5] = [(40, 4), (1005, 4), (1010, 4), (1011, 3), (1046, 3)];
    let mut t = term(6, 3, b"ab\x1b[1;31m");
    let before = format!("{t:?}");
    for (n, fixed) in rows {
        t.take_replies();
        t.input(format!("\x1b[?{n}$p").as_bytes());
        let reply = String::from_utf8(t.take_replies()).unwrap();
        assert_eq!(reply, format!("\x1b[?{n};{fixed}$y"), "DECRQM ?{n}");
        t.input(format!("\x1b[?{n}h").as_bytes());
        assert_eq!(rqm(&mut t, true, n), fixed, "?{n} set");
        t.input(format!("\x1b[?{n}l").as_bytes());
        assert_eq!(rqm(&mut t, true, n), fixed, "?{n} reset");
        assert_eq!(format!("{t:?}"), before, "?{n} changed the state");
    }
}
