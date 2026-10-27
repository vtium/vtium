//! The modes, driven through the byte stream: SM and RM (and DECSET and
//! DECRST) set and reset each row of the mode table, DECRQM reports it,
//! and DECSTR and RIS restore it. Each supported row's test sets and
//! resets its mode and checks the effect; the ignored and refused rows
//! stay at their fixed state.
//!
//! Each expected value is the C reference's behaviour, or the
//! specification's where vtium departs from it (the crate's README lists
//! each departure).

mod common;

use common::{rows, term};
use vtium_term::{
    Age, Cursor, CursorShape, CursorStyle, Mods, MouseButton, MouseEvent, Palette, Terminal,
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
    assert!(t.key(0xff0d, 0, Mods::empty(), None));
    assert_eq!(sent(&mut t), b"\r\n");
    toggles(&mut t, false, 20);
    t.key(0xff0d, 0, Mods::empty(), None);
    assert_eq!(sent(&mut t), b"\r");
}

// ctlseqs, DECCKM: the cursor keys send SS3 forms while set.
// covers: DEC-1
#[test]
fn decckm_switches_the_cursor_keys() {
    let mut t = term(8, 2, b"");
    t.key(0xff52, 0, Mods::empty(), None);
    assert_eq!(sent(&mut t), b"\x1b[A");
    t.input(b"\x1b[?1h");
    assert_eq!(rqm(&mut t, true, 1), 1);
    t.key(0xff52, 0, Mods::empty(), None);
    t.key(0xff50, 0, Mods::empty(), None);
    assert_eq!(sent(&mut t), b"\x1bOA\x1bOH");
    toggles(&mut t, true, 1);
    t.key(0xff57, 0, Mods::empty(), None);
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
// mode is set or reset"; the C reference left it where it was.
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
    // ctlseqs: the screen is cleared "if in the Alternate Screen Buffer";
    // the C reference cleared the main screen too.
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
// pen, so the main screen erases with its own colour (aaronsb/libtsm#20).
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
// order the modes were set in (aaronsb/libtsm#30), pixels counted from 1
// (aaronsb/libtsm#67); RM of an encoding clears only itself.
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
    assert!(t.key(0x61, 0x61, Mods::empty(), Some('a')));
    assert_eq!(sent(&mut t), b"a");
}

// ctlseqs, DECSTR: a soft reset returns the modes, the pen, the cursor
// style and the saved cursor to their defaults, and leaves the text and the
// cursor. DECSTR also resets the mouse modes and 1004, as the C reference
// did and xterm does only on RIS (aaronsb/libtsm#76, the operator's
// decision).
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
    t.key(0xffb1, 0, Mods::empty(), None);
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
