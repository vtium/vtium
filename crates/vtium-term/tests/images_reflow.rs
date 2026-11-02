#![cfg(feature = "images")]
//! Placements through rewraps (ADR-1003, "Images"; ADR-1100, D8).
//!
//! Three terminals run each stream of text, resizes, view moves,
//! scrollback sizes and placements, and the tests compare them:
//!
//! - Lazy leaves the oldest rows to wait at each resize, as vtium does.
//! - Eager runs the same resize and then rewraps every row left waiting.
//!   It runs the same code as Lazy for the lines an anchor is on: the
//!   resize rewraps those at once in both (`take_for_reflow`), and a drop
//!   onto a waiting line rewraps it in both (`note_anchor`). So Eager
//!   checks the rows that wait without an anchor, the drops past
//!   `sb-size` and the view moves, and it is no oracle for how an anchor
//!   is carried.
//! - Whole stands the view on the oldest row as each resize happens, so
//!   the resize takes every row at once and nothing waits, as the rewrap
//!   did before rows waited. It never reaches `rewrap_at` or
//!   `note_anchor`, so it is the oracle for the carry. It shares
//!   `rewrap_line`, the drop log, `discard_rows` and the screen's handling
//!   of both with the other two.
//!
//! What all three share is pinned by tests with expected results written
//! out: `a_waiting_placement_past_the_text_keeps_the_column_each_width_left_it`
//! and `a_placement_dropped_through_a_whole_waiting_line_loses_its_rows_at_the_width`
//! here, the scrollback's and `rewrap_line`'s unit tests, the store's
//! `a_line_dropped_for_several_rows_clips_as_many_as_one_drop_a_row`, and
//! the drop tests in `tests/images.rs`.
//!
//! The property test also runs Lazy with no view moves and compares its
//! placements. The crafted sweeps compare the three on drops during a
//! rewrap. `recheck_cost` measures a resize with a placement kept alive at
//! the scrollback's oldest row through every resize (ignored: run it with
//! `--release -- --ignored --nocapture`). `RECHECK_ONLY=SEED` replays one seed of the
//! property test with its stream printed; `VTIUM_TERM_SEEDS` sets how
//! many seeds it runs.

mod common;

use common::Rng;
use vtium_term::Terminal;
use vtium_term::images::{NewImage, Place, Protocol};

/// What a test compares: the placements at every view offset, the
/// placement and image counts, and the screen's text.
type Snap = (Vec<(usize, i64, u32, u32, u32)>, usize, usize, Vec<String>);

#[derive(Clone, Copy, Debug)]
enum Ev {
    Resize(usize, usize),
    ViewUp(usize),
    ViewDown(usize),
    Place(bool, u32, u32),
    SbMax(usize),
}

fn ev(rng: &mut Rng) -> Ev {
    match rng.below(9) {
        0..=2 => Ev::Resize(2 + rng.below(28), 1 + rng.below(8)),
        3 => Ev::ViewUp(1 + rng.below(40)),
        4 => Ev::ViewDown(1 + rng.below(10)),
        5 => Ev::SbMax(rng.below(80)),
        _ => Ev::Place(
            rng.below(2) == 0,
            1 + rng.below(3) as u32,
            1 + rng.below(3) as u32,
        ),
    }
}

/// How a terminal under test rewraps its scrollback at a resize.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// The rows that wait rewrap when something reaches them.
    Lazy,
    /// Every row left waiting rewraps right after the resize.
    Eager,
    /// The view stands on the oldest row as the resize happens, so the
    /// resize takes every row at once and nothing waits, as the rewrap did
    /// before the scrollback waited.
    Whole,
}

fn apply(t: &mut Terminal, e: Ev, mode: Mode) {
    match e {
        Ev::Resize(c, r) => {
            if mode == Mode::Whole {
                t.screen_mut().scroll_view_up(usize::MAX);
                t.resize(c, r);
                t.screen_mut().reset_view();
            } else {
                t.resize(c, r);
            }
            if mode != Mode::Lazy {
                t.screen_mut().rewrap_scrollback();
            }
        }
        Ev::ViewUp(n) => t.screen_mut().scroll_view_up(n),
        Ev::ViewDown(n) => t.screen_mut().scroll_view_down(n),
        Ev::SbMax(n) => t.screen_mut().set_scrollback_max(n),
        Ev::Place(kitty, w, h) => {
            let (x, y) = t.screen().cursor();
            let new = NewImage {
                protocol: if kitty {
                    Protocol::KITTY
                } else {
                    Protocol::SIXEL
                },
                width: w * 8,
                height: h * 16,
                pixels: vec![0xff00_0000; (w * 8 * h * 16) as usize],
                id: 0,
                number: 0,
                transient: false,
            };
            if let Ok(k) = t.screen_mut().add_image(new) {
                let _ = t.screen_mut().place_image(
                    k,
                    Place {
                        col: x as u32,
                        row: y as u32,
                        ..Place::default()
                    },
                );
            }
        }
    }
}

fn feed(t: &mut Terminal, bytes: &[u8], events: &[(usize, Ev)], mode: Mode, views: bool) {
    let mut at = 0;
    let mut stops: Vec<usize> = events.iter().map(|e| e.0).collect();
    stops.push(bytes.len());
    stops.sort_unstable();
    stops.dedup();
    for stop in stops {
        if stop > at {
            t.input(&bytes[at..stop]);
            at = stop;
        }
        for &(_, e) in events.iter().filter(|r| r.0 == stop) {
            if !views && matches!(e, Ev::ViewUp(_) | Ev::ViewDown(_)) {
                continue;
            }
            if std::env::var("RECHECK_ONLY").is_ok() {
                eprintln!("EV {e:?} {mode:?}");
            }
            apply(t, e, mode);
        }
    }
}

fn snap(t: &mut Terminal) -> Snap {
    t.screen_mut().rewrap_scrollback();
    let depth = t.screen().scrollback().len();
    let mut out = Vec::new();
    for off in 0..=depth {
        t.screen_mut().reset_view();
        t.screen_mut().scroll_view_up(off);
        for p in t.screen().placements() {
            out.push((off, p.row, p.col, p.cols, p.rows));
        }
    }
    t.screen_mut().reset_view();
    let mut text = common::scrollback(t.screen());
    text.extend(common::rows(t.screen()));
    (
        out,
        t.screen().images().placement_count(),
        t.screen().images().image_count(),
        text,
    )
}

#[test]
fn recheck_lazy_eager_with_placements() {
    let mut bad = 0;
    let only: Option<u64> = std::env::var("RECHECK_ONLY")
        .ok()
        .and_then(|s| s.parse().ok());
    for seed in 0..common::seeds(400) {
        if only.is_some_and(|o| o != seed) {
            continue;
        }
        if std::env::var("RECHECK_SEED").is_ok() {
            eprintln!("SEED {seed}");
        }
        let mut rng = Rng(seed ^ 0x1100);
        let mut bytes = Vec::new();
        let mut events = Vec::new();
        for _ in 0..2 + rng.below(6) {
            let len = rng.below(120);
            let line: String = (0..len)
                .map(|i| char::from(b'a' + (i % 26) as u8))
                .collect();
            bytes.extend(format!("{line}\r\n").into_bytes());
            if rng.below(3) == 0 {
                bytes.extend(
                    "\u{4e00}\u{4e01}xx\u{4e02}"
                        .repeat(rng.below(8))
                        .into_bytes(),
                );
                bytes.extend(b"\r\n");
            }
            events.push((bytes.len().saturating_sub(rng.below(5)), ev(&mut rng)));
            events.push((bytes.len(), ev(&mut rng)));
        }
        for _ in 0..rng.below(8) {
            events.push((bytes.len(), ev(&mut rng)));
        }
        let cols = 2 + rng.below(28);
        let make = || {
            let mut t = Terminal::new(cols, 4, 100);
            t.screen_mut().set_scrollback_max(60);
            t.screen_mut().set_image_limit(1 << 30);
            t
        };
        if only.is_some() {
            eprintln!("cols {cols} bytes {} events {events:?}", bytes.len());
            eprintln!("BYTES {:?}", String::from_utf8_lossy(&bytes));
        }
        let mut lazy = make();
        feed(&mut lazy, &bytes, &events, Mode::Lazy, true);
        let mut eager = make();
        feed(&mut eager, &bytes, &events, Mode::Eager, true);
        let mut whole = make();
        feed(&mut whole, &bytes, &events, Mode::Whole, true);
        let mut noview = make();
        feed(&mut noview, &bytes, &events, Mode::Lazy, false);
        let (l, e, w, n) = (
            snap(&mut lazy),
            snap(&mut eager),
            snap(&mut whole),
            snap(&mut noview),
        );
        if l != e || l != w || l.0 != n.0 || l.1 != n.1 {
            bad += 1;
            if bad <= 3 {
                eprintln!(
                    "seed {seed} cols {cols}\nevents {events:?}\nlazy  {:?} {} {}\neager {:?} {} {}\nwhole {:?} {} {}\nnoview {:?} {} {}",
                    l.0, l.1, l.2, e.0, e.1, e.2, w.0, w.1, w.2, n.0, n.1, n.2
                );
                if l.3 != e.3 || l.3 != w.3 {
                    eprintln!("text differs");
                }
            }
        }
    }
    assert_eq!(bad, 0, "seeds where lazy and eager differ");
}

#[test]
#[ignore = "a measurement, run on its own in release"]
#[expect(clippy::disallowed_methods, reason = "a measurement reads the clock")]
fn recheck_cost() {
    for (deep, label) in [
        (false, "no placement"),
        (true, "placement at the oldest row"),
    ] {
        // Lines of 170 wide characters take 2 rows at 170 columns and 4 at
        // 97, so sb-size is 100,000: at 10,000 the first narrowing would
        // drop the oldest rows, and the placement with them.
        let mut t = Terminal::new(170, 50, 100_000);
        t.screen_mut().set_scrollback_max(100_000);
        t.screen_mut().set_scrollback_memory(1 << 30);
        t.screen_mut().set_image_limit(1 << 30);
        let line: Vec<u8> = (0..170)
            .flat_map(|i| format!("\x1b[38;5;{}m\u{4e00}", i % 256).into_bytes())
            .collect();
        if deep {
            apply(&mut t, Ev::Place(true, 2, 2), Mode::Lazy);
        }
        for _ in 0..4_950 {
            t.input(&line);
            t.input(b"\r\n");
        }
        let placements = usize::from(deep);
        eprintln!(
            "{label}: sb {} placements {}",
            t.screen().scrollback().len(),
            t.screen().images().placement_count()
        );
        let mut spent = std::time::Duration::ZERO;
        for i in 0..20 {
            let start = std::time::Instant::now();
            t.resize(if i % 2 == 0 { 97 } else { 170 }, 50);
            spent += start.elapsed();
            assert_eq!(
                t.screen().images().placement_count(),
                placements,
                "{label}: the placement lives through resize {i}"
            );
        }
        eprintln!("{label}: {:?} a resize", spent / 20);
    }
}

fn crafted(mode: Mode, c_width: usize, k: usize, sbm: usize, a: usize, b: usize) -> Snap {
    let mut t = Terminal::new(20, 4, 100);
    t.screen_mut().set_scrollback_max(sbm);
    t.screen_mut().set_image_limit(1 << 30);
    let step = |t: &mut Terminal, c: usize, r: usize| apply(t, Ev::Resize(c, r), mode);
    for i in 0..3 {
        t.input(format!("line {i:02}\r\n").as_bytes());
    }
    t.input(b"line 03       ");
    apply(&mut t, Ev::Place(true, 1, 3), Mode::Lazy);
    t.input(b"\r\n");
    for i in 4..4 + k {
        t.input(format!("line {i:02}\r\n").as_bytes());
    }
    t.input(b"x");
    if std::env::var("RECHECK_ONLY").is_ok() {
        eprintln!("before A: {:?}", t.screen().images().placement_count());
    }
    step(&mut t, a, 4); // A: narrowing doubles rows, drops oldest
    step(&mut t, b, 4); // B: a waiting stretch
    t.input(b"\r\n"); // a scroll processes the drop
    t.screen_mut().set_scrollback_max(100);
    for i in 0..20 {
        t.input(format!("more {i:02}\r\n").as_bytes());
    }
    step(&mut t, c_width, 4); // C
    snap(&mut t)
}

#[test]
fn recheck_crafted() {
    let mut found = 0;
    for sbm in [6, 8, 10, 14] {
        for k in 0..14 {
            for a in [3, 4, 5] {
                for b in [6, 8, 12] {
                    for c in 2..12 {
                        let (l, e, w) = (
                            crafted(Mode::Lazy, c, k, sbm, a, b),
                            crafted(Mode::Eager, c, k, sbm, a, b),
                            crafted(Mode::Whole, c, k, sbm, a, b),
                        );
                        if l != e || l != w {
                            found += 1;
                            if found < 4 {
                                eprintln!(
                                    "sbm={sbm} k={k} A={a} B={b} C={c}\n lazy  {:?} {} {}\n eager {:?} {} {}\n whole {:?} {} {}",
                                    l.0, l.1, l.2, e.0, e.1, e.2, w.0, w.1, w.2
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    eprintln!("found {found}");
    assert_eq!(found, 0);
}

/// A placement on a line printed at 5 columns, past the first row's width
/// once the scrollback widens to 20, goes on to the next line as the rows
/// above it drop, while that line still waits at 5 columns; then the
/// scrollback narrows to `last` columns.
fn dropped_onto_waiting(mode: Mode, sbm: usize, last: usize) -> Snap {
    let mut t = Terminal::new(5, 4, 100);
    t.screen_mut().set_scrollback_max(100);
    t.screen_mut().set_image_limit(1 << 30);
    // twelve cells: the cursor ends at column 2 of the line's third row,
    // which is column 12 of one row 20 wide
    t.input(b"pppppppppppp");
    apply(&mut t, Ev::Place(true, 1, 3), mode);
    t.input(b"\r\nbbbbbbbbbbbb\r\n");
    for i in 0..10 {
        t.input(format!("f{i}\r\n").as_bytes());
    }
    apply(&mut t, Ev::Resize(20, 4), mode);
    t.screen_mut().set_scrollback_max(sbm);
    apply(&mut t, Ev::Resize(last, 4), mode);
    snap(&mut t)
}

#[test]
fn a_placement_dropped_onto_a_waiting_line_lands_as_each_width_puts_it() {
    let mut found = 0;
    let mut shown = 0;
    for sbm in 1..16 {
        for last in [6, 8, 10, 13] {
            let (l, e, w) = (
                dropped_onto_waiting(Mode::Lazy, sbm, last),
                dropped_onto_waiting(Mode::Eager, sbm, last),
                dropped_onto_waiting(Mode::Whole, sbm, last),
            );
            shown += usize::from(!e.0.is_empty());
            if l != e || l != w {
                found += 1;
                if found < 4 {
                    eprintln!(
                        "sbm={sbm} last={last}\n lazy  {:?}\n eager {:?}\n whole {:?}",
                        l.0, e.0, w.0
                    );
                }
            }
        }
    }
    assert!(shown > 0, "some combination keeps the placement in view");
    assert_eq!(found, 0, "combinations where lazy and eager differ");
}

/// The placements the view shows from the scrollback's oldest row.
fn from_the_oldest(t: &mut Terminal) -> Vec<(i64, u32, u32, u32)> {
    t.screen_mut().scroll_view_up(usize::MAX);
    let shown = t
        .screen()
        .placements()
        .iter()
        .map(|p| (p.row, p.col, p.cols, p.rows))
        .collect();
    t.screen_mut().reset_view();
    shown
}

/// A 12 by 4 terminal holding `first` with a kitty placement of 1 by
/// `rows` cells at column `col` of its row, then `after`, then 20 short
/// lines, so that a resize leaves them all waiting.
fn deep(first: &str, col: usize, rows: u32, after: &str) -> Terminal {
    let mut t = Terminal::new(12, 4, 100);
    t.screen_mut().set_scrollback_max(100);
    t.screen_mut().set_image_limit(1 << 30);
    t.input(first.as_bytes());
    t.input(format!("\x1b[1;{}H", col + 1).as_bytes());
    apply(&mut t, Ev::Place(true, 1, rows), Mode::Lazy);
    t.input(format!("\r\n{after}").as_bytes());
    for i in 0..20 {
        t.input(format!("\r\nf{i}").as_bytes());
    }
    t
}

// "abc" with a placement at column 7, past its text, waiting through 5
// columns and then 10: at 5 the placement is kept inside the row, at
// column 4, and it stays there at 10, where a rewrap from 12 to 10 alone
// would leave it at 7.
#[test]
fn a_waiting_placement_past_the_text_keeps_the_column_each_width_left_it() {
    for mode in [Mode::Lazy, Mode::Eager, Mode::Whole] {
        let mut t = deep("abc", 7, 1, "");
        apply(&mut t, Ev::Resize(5, 4), mode);
        assert_eq!(from_the_oldest(&mut t), [(0, 4, 1, 1)], "{mode:?} at 5");
        apply(&mut t, Ev::Resize(10, 4), mode);
        assert_eq!(from_the_oldest(&mut t), [(0, 4, 1, 1)], "{mode:?} at 10");
    }
}

// A placement 5 rows tall on "a", above a line of 12 cells that waits at
// 12 columns after a resize to 4 and so stands for 3 rows: dropping the 4
// oldest rows takes "a" and that line whole, and the placement loses 4
// rows, its last one left on the line after them, "c".
#[test]
fn a_placement_dropped_through_a_whole_waiting_line_loses_its_rows_at_the_width() {
    for mode in [Mode::Lazy, Mode::Eager, Mode::Whole] {
        let mut t = deep("a", 0, 5, "bbbbbbbbbbbb\r\nc");
        apply(&mut t, Ev::Resize(4, 4), mode);
        let mut count = t.clone();
        count.screen_mut().rewrap_scrollback();
        let total = count.screen().scrollback().len();
        t.screen_mut().set_scrollback_max(total - 4);
        assert_eq!(from_the_oldest(&mut t), [(0, 0, 1, 1)], "{mode:?}");
        let oldest = common::scrollback(t.screen()).into_iter().next();
        assert_eq!(oldest.as_deref().map(str::trim_end), Some("c"), "{mode:?}");
    }
}
