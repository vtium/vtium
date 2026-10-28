//! OSC 133 prompt marks (ADR-1002, sections 5 and 6): what each mark does
//! to the cursor's line, how marks travel with their lines, what erases
//! them, the prompt jumps and the copy of a command's output.

mod common;

use common::{rows, scrollback, term};
use vtium_term::{LineFlags, Screen, Terminal};

/// `OSC 133 ; body ST`.
fn mark(body: &str) -> String {
    format!("\x1b]133;{body}\x1b\\")
}

fn flags(s: &Screen, y: usize) -> LineFlags {
    s.line(y).unwrap().flags().intersection(LineFlags::MARKS)
}

/// A terminal `cols` by `rows` keeping `sb` scrollback rows, fed `bytes`.
fn with_scrollback(cols: usize, rows: usize, sb: usize, bytes: &[u8]) -> Terminal {
    let mut t = Terminal::new(cols, rows, sb);
    t.screen_mut().set_scrollback_max(sb);
    t.input(bytes);
    t
}

// "Prompt marks": A, N, B, I and C mark the cursor's line; A and N do a
// fresh line first; P and D do nothing.
// covers: osc.133
#[test]
fn each_mark_sets_its_flag_on_the_cursors_line() {
    let cases = [
        ("A", LineFlags::PROMPT),
        ("N", LineFlags::PROMPT),
        ("B", LineFlags::INPUT),
        ("I", LineFlags::INPUT),
        ("C", LineFlags::OUTPUT),
        ("P", LineFlags::empty()),
        ("D", LineFlags::empty()),
        ("D;0", LineFlags::empty()),
        ("L", LineFlags::empty()),
        ("A;aid=7;cl=m", LineFlags::PROMPT),
        ("C;k=s", LineFlags::OUTPUT),
    ];
    for (body, want) in cases {
        let t = term(8, 3, format!("{}x", mark(body)).as_bytes());
        assert_eq!(flags(t.screen(), 0), want, "{body:?}");
        assert_eq!(rows(t.screen())[0], "x", "{body:?} at the first column");
        assert!(t.replies().is_empty(), "{body:?}");
    }
}

// A malformed mark does nothing.
// covers: osc.133
#[test]
fn a_malformed_mark_does_nothing() {
    for body in ["", "AB", "Z", "a", ";A", "Ax"] {
        let t = term(8, 3, format!("ab{}x", mark(body)).as_bytes());
        assert_eq!(flags(t.screen(), 0), LineFlags::empty(), "{body:?}");
        assert_eq!(t.screen().cursor(), (3, 0), "{body:?}");
    }
    let t = term(8, 3, b"ab\x1b]133\x1b\\x");
    assert_eq!(t.screen().cursor(), (3, 0));
}

// "Prompt marks": a fresh line is a carriage return and a line feed when
// the cursor is not in the first column, a pending wrap included, and
// nothing when it is; L does only that.
// covers: osc.133
#[test]
fn a_and_n_and_l_start_a_fresh_line() {
    for (body, want) in [
        ("A", LineFlags::PROMPT),
        ("N", LineFlags::PROMPT),
        ("L", LineFlags::empty()),
    ] {
        let t = term(8, 3, format!("out{}$ ", mark(body)).as_bytes());
        let s = t.screen();
        assert_eq!(rows(s)[..2], ["out", "$ "], "{body:?}");
        assert_eq!(flags(s, 0), LineFlags::empty(), "{body:?}");
        assert_eq!(flags(s, 1), want, "{body:?}");

        // A pending wrap is not the first column: the line ends, unwrapped.
        let t = term(4, 3, format!("abcd{}$", mark(body)).as_bytes());
        assert_eq!(rows(t.screen())[..2], ["abcd", "$"], "{body:?}");
        let wrapped = t
            .screen()
            .line(0)
            .unwrap()
            .flags()
            .contains(LineFlags::WRAPPED);
        assert!(!wrapped, "{body:?}");
    }
    // B and C mark without a fresh line.
    let t = term(8, 3, format!("$ {}ls", mark("B")).as_bytes());
    assert_eq!(rows(t.screen())[0], "$ ls");
    assert_eq!(flags(t.screen(), 0), LineFlags::INPUT);
    // At the bottom the fresh line scrolls.
    let t = with_scrollback(8, 2, 10, format!("a\r\nb{}$", mark("A")).as_bytes());
    assert_eq!(scrollback(t.screen()), ["a"]);
    assert_eq!(rows(t.screen()), ["b", "$"]);
    assert_eq!(flags(t.screen(), 1), LineFlags::PROMPT);
}

/// A shell's session: `n` commands, each a prompt, its input and two lines
/// of output.
fn session(n: usize) -> String {
    let mut out = String::new();
    for i in 0..n {
        out += &format!(
            "{}$ {}cmd{i}\r\n{}out{i}a\r\nout{i}b\r\n{}",
            mark("A"),
            mark("B"),
            mark("C"),
            mark("D;0")
        );
    }
    out + &mark("A") + "$ "
}

// "Prompt marks": the marks travel with their lines into the scrollback,
// and a rewrap gives a rejoined line the marks of all its rows.
// covers: osc.133
#[test]
fn marks_travel_with_their_lines_and_through_a_rewrap() {
    let mut t = with_scrollback(10, 4, 100, session(3).as_bytes());
    t.screen_mut().rewrap_scrollback();
    let s = t.screen();
    let sb: Vec<(String, LineFlags)> = s
        .scrollback()
        .zip(scrollback(s))
        .map(|(l, text)| (text, l.flags().intersection(LineFlags::MARKS)))
        .collect();
    assert_eq!(
        sb[0],
        ("$ cmd0".into(), LineFlags::PROMPT | LineFlags::INPUT)
    );
    assert_eq!(sb[1], ("out0a".into(), LineFlags::OUTPUT));
    assert_eq!(sb[2], ("out0b".into(), LineFlags::empty()));

    // A prompt that wrapped at 4 columns rejoins at 10, its input's mark on
    // its second row going to its first.
    let bytes = format!("{}$ abc{}defg\r\n", mark("A"), mark("B"));
    let mut t = with_scrollback(4, 4, 100, bytes.as_bytes());
    assert_eq!(flags(t.screen(), 0), LineFlags::PROMPT);
    assert_eq!(flags(t.screen(), 1), LineFlags::INPUT);
    t.resize(10, 4);
    assert_eq!(rows(t.screen())[0], "$ abcdefg");
    assert_eq!(flags(t.screen(), 0), LineFlags::PROMPT | LineFlags::INPUT);
    assert_eq!(flags(t.screen(), 1), LineFlags::empty());
}

// "Prompt marks": ED 2, ED 3 and RIS clear the marks of the lines they
// erase; EL and the partial erases keep them.
// covers: osc.133
#[test]
fn ed_2_and_ris_clear_marks_and_partial_erases_keep_them() {
    let marked = format!("{}$ ls\r\n{}x", mark("A"), mark("C"));
    for (erase, kept) in [
        ("\x1b[K", true),
        ("\x1b[2K", true),
        ("\x1b[1K", true),
        ("\x1b[H\x1b[J", true),
        ("\x1b[1J", true),
        ("\x1b[2J", false),
        ("\x1bc", false),
    ] {
        let t = term(8, 3, format!("{marked}{erase}").as_bytes());
        let s = t.screen();
        let want = |f| if kept { f } else { LineFlags::empty() };
        assert_eq!(flags(s, 0), want(LineFlags::PROMPT), "{erase:?}");
        assert_eq!(flags(s, 1), want(LineFlags::OUTPUT), "{erase:?}");
    }
    let mut t = with_scrollback(8, 2, 10, session(2).as_bytes());
    assert!(t.screen().scrollback().len() > 0);
    t.input(b"\x1b[3J");
    assert_eq!(t.screen().scrollback().len(), 0);
}

/// The text of the view's top row.
fn top(s: &Screen) -> String {
    let palette = vtium_term::Palette::default();
    let frame = s.frame(&palette, vtium_term::Age::NEVER);
    frame
        .cells(0)
        .map(|c| match c.glyph {
            vtium_term::Glyph::Char(c) => c,
            _ => ' ',
        })
        .collect::<String>()
        .trim_end()
        .to_string()
}

// "What the marks are for": each jump up lands the nearest marked prompt
// above the view's top row at the view's top, one press each, and a jump
// down past the newest prompt returns the view to the live screen.
// covers: osc.133
#[test]
fn a_prompt_jump_lands_each_marked_prompt_in_the_scrollback() {
    let mut t = with_scrollback(10, 4, 100, session(5).as_bytes());
    let s = t.screen_mut();
    assert_eq!(s.scrolled_back(), 0);
    // The newest command's prompt is the view's top row already.
    assert_eq!(top(s), "$ cmd4");
    for i in (0..4).rev() {
        assert!(s.jump_to_prompt(true));
        assert_eq!(top(s), format!("$ cmd{i}"), "up to prompt {i}");
    }
    assert!(!s.jump_to_prompt(true), "no prompt above the oldest");
    assert_eq!(top(s), "$ cmd0");
    for i in 1..4 {
        assert!(s.jump_to_prompt(false));
        assert_eq!(top(s), format!("$ cmd{i}"), "down to prompt {i}");
    }
    // The next prompt is on the screen: down returns to the live screen.
    assert!(s.jump_to_prompt(false));
    assert_eq!(s.scrolled_back(), 0);
    assert_eq!(top(s), "$ cmd4");
    assert!(!s.jump_to_prompt(false), "nothing below the live screen");
}

// "What the marks are for": prompts still waiting to be rewrapped after a
// resize are reached too.
// covers: osc.133
#[test]
fn a_prompt_jump_reaches_rows_waiting_to_be_rewrapped() {
    let mut t = with_scrollback(10, 4, 100, session(5).as_bytes());
    t.resize(12, 4);
    let s = t.screen_mut();
    let mut seen = Vec::new();
    while s.jump_to_prompt(true) {
        seen.push(top(s));
    }
    assert_eq!(seen, ["$ cmd3", "$ cmd2", "$ cmd1", "$ cmd0"]);
}

// "What the marks are for": on the alternate screen a jump does nothing.
// covers: osc.133
#[test]
fn a_prompt_jump_does_nothing_on_the_alternate_screen() {
    let mut t = with_scrollback(10, 4, 100, session(5).as_bytes());
    t.input(b"\x1b[?1049h");
    assert!(!t.screen_mut().jump_to_prompt(true));
    assert_eq!(t.screen().scrolled_back(), 0);
}

// "What the marks are for": a command's output, from its output-flagged
// line to the line before the next prompt, is selected as lines.
// covers: osc.133
#[test]
fn a_commands_output_is_selected_from_its_mark_to_the_next_prompt() {
    let mut t = with_scrollback(10, 8, 100, session(2).as_bytes());
    let s = t.screen_mut();
    // Rows: $ cmd0, out0a, out0b, $ cmd1, out1a, out1b, $
    assert_eq!(
        rows(s)[..7],
        ["$ cmd0", "out0a", "out0b", "$ cmd1", "out1a", "out1b", "$ "]
    );
    assert!(s.select_output(3, 2));
    assert_eq!(s.selection_text().as_deref(), Some("out0a\nout0b\n"));
    assert!(s.select_output(0, 4));
    assert_eq!(s.selection_text().as_deref(), Some("out1a\nout1b\n"));
    // A prompt's line is no output.
    assert!(!s.select_output(0, 3));
    // Output still being written ends at the last written line.
    let bytes = format!("{}$ {}run\r\n{}one\r\ntwo", mark("A"), mark("B"), mark("C"));
    let mut t = term(10, 6, bytes.as_bytes());
    assert!(t.screen_mut().select_output(0, 2));
    assert_eq!(t.screen().selection_text().as_deref(), Some("one\ntwo\n"));
    // An empty line the cursor waits on is not part of it.
    t.input(b"\r\n");
    assert!(t.screen_mut().select_output(0, 1));
    assert_eq!(t.screen().selection_text().as_deref(), Some("one\ntwo\n"));
}
