//! A run of printable ASCII written into the screen in one step
//! (`Screen::print_ascii`, which the terminal calls for each run the
//! parser hands it) leaves the screen exactly as printing each byte alone
//! leaves it: the cells and their line tables, the damage, the cursor and
//! its pending wrap, and the cluster the next print may join.
//!
//! The property tests compare the two paths over streams that mix long
//! runs of ASCII with every sequence that turns the one-step write on or
//! off: insert mode, autowrap, the margins, the character sets and their
//! shifts, double-size lines, sized runs, wide characters, combining marks,
//! a prepended mark, styles, links, erases, scrolls and, in a build with
//! images, a sixel picture that text cuts. Screen equality covers every
//! field, damage ages included. A failure prints its seed.

mod common;

use common::Rng;
use vtium_term::{Screen, Terminal};

/// Sequences that turn the one-step write on or off: insert mode,
/// autowrap, the margins, the character sets and their shifts, the line
/// sizes, a sized run, grapheme clustering, and a conformance level that
/// sets 8-bit input and back.
const SWITCHES: &[&str] = &[
    "\x1b[4h",
    "\x1b[4l",
    "\x1b[?7l",
    "\x1b[?7h",
    "\x1b[?69h\x1b[3;9s",
    "\x1b[?69h\x1b[2;5s",
    "\x1b[?69l",
    "\x1b(0",
    "\x1b(B",
    "\x1b(A",
    "\x1b)0",
    "\x0e",
    "\x0f",
    "\x1bN",
    "\x1bO",
    "\x1b*0",
    "\x1b#6",
    "\x1b#3",
    "\x1b#5",
    "\x1b]66;s=2;AB\x07",
    "\x1b]66;w=2;x\x07",
    "\x1b[?2027h",
    "\x1b[?2027l",
    "\x1b[62;2\"p",
    "\x1b[65;1\"p",
];

/// Everything else a run meets: line ends, wide characters, marks,
/// clusters, a prepended mark, right-to-left text, REP, reflow, the pen, a
/// link, cursor movement, erases, edits and scrolls.
const OTHERS: &[&str] = &[
    "\r\n",
    "\r",
    "\n",
    "\x08",
    "\t",
    "\x1b[2;4r",
    "\x1b[r",
    "\u{4E2D}",
    "\u{1F600}",
    "\u{301}",
    "\u{FE0F}",
    "\u{200D}",
    "\u{600}",
    "\u{5D0}\u{5D1}",
    "\x1b[3b",
    "\x1b[?2028h",
    "\x1b[?2028l",
    "\x1b[1;31m",
    "\x1b[44m",
    "\x1b[0m",
    "\x1b]8;;https://example.com\x07",
    "\x1b]8;;\x07",
    "\x1b[H",
    "\x1b[2;3H",
    "\x1b[9G",
    "\x1b[K",
    "\x1b[2J",
    "\x1b[@",
    "\x1b[2P",
    "\x1b[L",
    "\x1b[M",
    "\x1b[S",
    "\x1b[T",
    "\x1b7",
    "\x1b8",
];

/// A small sixel picture, two cells wide, that text cuts.
#[cfg(feature = "images")]
const SIXEL: &str = "\x1bPq#1;2;100;0;0#1~~~~~~~~~~~~-~~~~~~~~~~~~\x1b\\";

/// A run of printable ASCII from 1 to 70 bytes, spaces included.
fn ascii(rng: &mut Rng) -> String {
    let len = 1 + rng.below(70);
    (0..len)
        .map(|_| char::from(0x20 + rng.below(0x5f) as u8))
        .collect()
}

/// A stream of about `len` bytes, mostly runs of ASCII.
fn stream(rng: &mut Rng, len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    while out.len() < len {
        match rng.below(10) {
            0..=4 => out.extend(ascii(rng).bytes()),
            5 | 6 => out.extend(rng.pick(SWITCHES).bytes()),
            #[cfg(feature = "images")]
            7 if rng.below(8) == 0 => out.extend(SIXEL.bytes()),
            _ => out.extend(rng.pick(OTHERS).bytes()),
        }
    }
    out
}

fn fresh(cols: usize, rows: usize) -> Terminal {
    let mut t = Terminal::new(cols, rows, 20);
    t.screen_mut().set_scrollback_max(20);
    #[cfg(feature = "images")]
    {
        use vtium_term::ImageString;
        use vtium_term::images::{Sixel, Switches};
        t.screen_mut().set_image_limit(1 << 20);
        t.screen_mut().set_image_cell(4, 6).expect("a cell");
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Sixel, Some(Box::new(Sixel::default())));
    }
    t
}

/// Feeds `piece` until the terminal has taken all of it, whatever pauses
/// it, `step` bytes at most a call.
fn feed(t: &mut Terminal, piece: &[u8], step: usize) {
    let mut rest = piece;
    let mut calls = 0;
    while !rest.is_empty() {
        let n = rest.len().min(step);
        let taken = t.input(&rest[..n]);
        rest = &rest[taken..];
        calls += 1;
        assert!(calls < 1 << 20, "the terminal stopped taking input");
    }
}

/// Feeds `stream` in the pieces `cuts` makes, each fed whole or a byte at
/// a time, and draws a frame after each piece, so damage ages differ
/// between pieces. Between some pieces the view is scrolled back, so
/// damage marks everything, and then reset.
fn run(cols: usize, rows: usize, stream: &[u8], cuts: &[usize], bytewise: bool) -> Terminal {
    let mut t = fresh(cols, rows);
    let step = if bytewise { 1 } else { usize::MAX };
    let mut from = 0;
    for (i, &cut) in cuts.iter().chain([&stream.len()]).enumerate() {
        feed(&mut t, &stream[from..cut], step);
        t.screen_mut().checkpoint();
        match i % 5 {
            1 => t.screen_mut().scroll_view_up(1),
            3 => t.screen_mut().reset_view(),
            _ => {}
        }
        from = cut;
    }
    t
}

fn same(a: &Terminal, b: &Terminal) -> bool {
    a.screen() == b.screen() && a.palette() == b.palette() && a.replies() == b.replies()
}

// The terminal fed each run whole writes it in one step where it can; fed
// a byte at a time, every byte goes through `print`. The two must agree on
// everything the screen holds, the damage between frames included.
#[test]
fn a_run_fed_whole_leaves_the_screen_a_byte_at_a_time_leaves() {
    #[cfg(feature = "images")]
    let mut pictures = 0usize;
    for seed in 0..common::seeds(600) {
        let mut rng = Rng(seed);
        let cols = 1 + rng.below(24);
        let rows = 1 + rng.below(7);
        let len = 1 + rng.below(1200);
        let s = stream(&mut rng, len);
        let mut cuts: Vec<usize> = (0..rng.below(10)).map(|_| rng.below(s.len() + 1)).collect();
        cuts.sort_unstable();
        let whole = run(cols, rows, &s, &cuts, false);
        let each = run(cols, rows, &s, &cuts, true);
        assert!(
            same(&whole, &each),
            "seed {seed}, {cols}x{rows}, cuts {cuts:?}: fed whole\n{:?}\nfed a byte at a time\n{:?}",
            common::rows(whole.screen()),
            common::rows(each.screen()),
        );
        #[cfg(feature = "images")]
        {
            pictures += usize::from(whole.screen().images().placement_count() > 0);
        }
    }
    // Text over a picture must meet one in enough streams to be tested.
    #[cfg(feature = "images")]
    assert!(pictures > 50, "only {pictures} streams kept a picture");
}

// `Screen::print_ascii` from any state the stream reaches, followed by
// `print` for the bytes it left, leaves the screen as `print` of each byte
// does. The one-step write runs in enough of the cases that the comparison
// tests it.
#[test]
fn print_ascii_then_print_for_the_rest_is_print_of_each_byte() {
    let (mut tried, mut wrote) = (0usize, 0usize);
    for seed in 0..common::seeds(1500) {
        let mut rng = Rng(seed);
        let cols = 1 + rng.below(24);
        let rows = 1 + rng.below(7);
        let len = rng.below(400);
        let prefix = stream(&mut rng, len);
        let text = ascii(&mut rng);
        let mut t = fresh(cols, rows);
        feed(&mut t, &prefix, usize::MAX);
        let mut fast: Screen = t.screen().clone();
        let mut slow: Screen = t.screen().clone();
        let n = fast.print_ascii(text.as_bytes());
        for c in text[n..].chars() {
            fast.print(c);
        }
        for c in text.chars() {
            slow.print(c);
        }
        tried += 1;
        wrote += usize::from(n > 0);
        assert!(
            fast == slow,
            "seed {seed}, {cols}x{rows}: {n} of {:?} in one step\n{:?}\nprinted a byte at a time\n{:?}",
            text,
            common::rows(&fast),
            common::rows(&slow),
        );
    }
    assert!(
        wrote * 3 > tried,
        "the one-step write ran in only {wrote} of {tried} cases"
    );
}

/// A screen with the cursor where `bytes` leave it.
fn screen(cols: usize, rows: usize, bytes: &[u8]) -> Screen {
    let mut t = fresh(cols, rows);
    feed(&mut t, bytes, usize::MAX);
    t.screen().clone()
}

// The one-step write takes the bytes that fit up to the right margin and
// leaves the cursor on the margin's column with the wrap pending, as
// printing each would (VT510 manual, DECAWM; xterm's `dotext`).
#[test]
fn a_run_stops_at_the_print_edge_with_the_wrap_pending() {
    let mut s = screen(10, 3, b"");
    assert_eq!(s.print_ascii(b"abcdefghijklmn"), 10);
    assert_eq!(s.cursor(), (10, 0));
    assert_eq!(common::rows(&s)[0], "abcdefghij");
    // Nothing more fits before the wrap.
    assert_eq!(s.print_ascii(b"klmn"), 0);
    s.print('k');
    assert_eq!(s.cursor(), (1, 1));
    assert_eq!(common::rows(&s)[1], "k");

    // A right margin inside the screen: the cursor stays on the margin's
    // column, and the next character wraps to the left margin.
    let mut s = screen(10, 3, b"\x1b[?69h\x1b[2;6s\x1b[1;2H");
    assert_eq!(s.print_ascii(b"abcdefgh"), 5);
    assert_eq!(s.cursor(), (5, 0));
    assert_eq!(common::rows(&s)[0], " abcde");
    assert_eq!(s.print_ascii(b"fgh"), 0);
    s.print('f');
    assert_eq!(s.cursor(), (2, 1));
    assert_eq!(common::rows(&s)[1], " f");
}

// The one-step write leaves each state that needs `print`'s own work to
// `print`: insert mode, a pending wrap, a double-width line, a row with a
// sized run, and a first byte that joins the cluster before it.
#[test]
fn a_run_is_left_to_print_where_one_step_would_differ() {
    let cases: &[(&str, &[u8])] = &[
        ("insert mode", b"\x1b[4h"),
        ("a pending wrap", b"abcdefghij"),
        ("a double-width line", b"\x1b#6"),
        ("a row with a sized run", b"\x1b]66;s=2;A\x07"),
        // GB9b: a Prepend character joins whatever follows it.
        ("a prepended mark before it", "a\u{600}".as_bytes()),
    ];
    for (what, bytes) in cases {
        let mut s = screen(10, 3, bytes);
        assert_eq!(s.print_ascii(b"xyz"), 0, "{what}");
    }
    let mut s = screen(10, 3, b"abc");
    assert_eq!(s.print_ascii(b"xyz"), 3, "the control case");
    // A byte that is not printable ASCII ends the step.
    assert_eq!(s.print_ascii(b"ab\x7fcd"), 2);
    assert_eq!(s.print_ascii("\u{e9}".as_bytes()), 0);
}

// The cluster a run's last byte starts stays open, so a combining mark
// that follows joins that cell, as it does after a print.
#[test]
fn a_mark_after_a_run_joins_its_last_cell() {
    let mut s = screen(10, 3, b"");
    assert_eq!(s.print_ascii(b"abc"), 3);
    s.print('\u{301}');
    assert_eq!(common::rows(&s)[0], "abc\u{301}");
    assert_eq!(s.cursor(), (3, 0));
}

// A run written over a wide character cuts it whole, at the run's first
// cell and at the cell after its last, as each print does.
#[test]
fn a_run_over_a_wide_character_cuts_it_whole() {
    let mut s = screen(10, 3, "\u{4E2D}\u{6587}\u{5B57}\x1b[1;2H".as_bytes());
    assert_eq!(s.print_ascii(b"xyz"), 3);
    assert_eq!(common::rows(&s)[0], " xyz\u{5B57}");
}
