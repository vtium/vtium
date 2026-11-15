//! Sized runs, the blocks of cells that OSC 66 writes, under the edits that
//! could leave part of a run on the screen: the rectangle edits, line size
//! changes, a resize that makes the screen shorter, and a line whose
//! cluster budget is nearly spent. Each test drives the terminal through
//! its input bytes. The last test mixes random runs with random edits and
//! checks after each step that every run is still whole.
mod common;

use common::Rng;
use std::collections::BTreeSet;

use vtium_term::{Attrs, Glyph, Line, LineId, LineSize, Terminal};

/// A terminal `cols` × `rows` with a scrollback of 50 lines, after `bytes`.
fn term(cols: usize, rows: usize, bytes: &str) -> Terminal {
    let mut t = Terminal::new(cols, rows, 50);
    t.screen_mut().set_scrollback_max(50);
    t.input(bytes.as_bytes());
    t
}

/// Each run share on row `y`: its first column, its columns, which row of
/// the run the line holds, and the run's rows.
fn runs(t: &Terminal, y: usize) -> Vec<(usize, usize, usize, usize)> {
    t.screen().line(y).map_or_else(Vec::new, |l| {
        l.runs()
            .iter()
            .map(|r| (r.col(), r.cols(), r.row(), r.rows()))
            .collect()
    })
}

/// Whether any cell of the screen is marked as part of a run, or any
/// line's run table holds an entry.
fn any_run(t: &Terminal) -> bool {
    t.screen()
        .lines()
        .iter()
        .any(|l| !l.runs().is_empty() || l.cells().iter().any(|c| c.in_run()))
}

/// The text of cell `x` of row `y`, or "" for an empty cell.
fn text(t: &Terminal, x: usize, y: usize) -> String {
    let Some(line) = t.screen().line(y) else {
        return String::new();
    };
    match line.glyph(line.cell(x)) {
        Glyph::Empty => String::new(),
        Glyph::Char(c) => c.to_string(),
        Glyph::Cluster(s) => s.to_owned(),
    }
}

// DECCRA whose target, clipped to the screen's last row, covers only the
// lower row of a two-row run erases the run on both rows.
// covers: text.kitty-sizing
#[test]
fn deccra_onto_a_runs_lower_row_erases_the_whole_run() {
    let t = term(10, 4, "\x1b[3;3H\x1b]66;s=2;A\x1b\\\x1b[1;1;3;4;1;6;1$v");
    assert!(
        !any_run(&t),
        "rows 2 and 3: {:?} {:?}",
        runs(&t, 2),
        runs(&t, 3)
    );
    assert_eq!(text(&t, 2, 2), "", "the run's top row is erased");
}

// DECFRA over the lower row of a two-row run erases the run on both rows.
// covers: text.kitty-sizing
#[test]
fn decfra_onto_a_runs_lower_row_erases_the_whole_run() {
    let t = term(10, 4, "\x1b[3;3H\x1b]66;s=2;A\x1b\\\x1b[120;4;1;4;10$x");
    assert!(
        !any_run(&t),
        "rows 2 and 3: {:?} {:?}",
        runs(&t, 2),
        runs(&t, 3)
    );
    assert_eq!(text(&t, 2, 2), "", "the run's top row is erased");
    assert_eq!(text(&t, 2, 3), "x", "the fill writes its row");
}

// DECCRA of a rectangle that holds a whole one-row run copies the run as a
// run of its own, and the source keeps its run.
// covers: text.kitty-sizing
#[test]
fn deccra_copies_a_whole_run_as_a_run() {
    let t = term(10, 4, "\x1b]66;w=2;AB\x1b\\\x1b[1;1;1;4;1;3;1;1$v");
    assert_eq!(runs(&t, 0), [(0, 2, 0, 1)]);
    assert_eq!(runs(&t, 2), [(0, 2, 0, 1)]);
    assert_eq!(text(&t, 0, 2), "AB");
}

// DECCRA of a rectangle that holds a whole two-row run copies both rows as
// one run of its own.
// covers: text.kitty-sizing
#[test]
fn deccra_copies_a_whole_tall_run_as_a_run() {
    let t = term(10, 4, "\x1b]66;s=2;A\x1b\\\x1b[1;1;2;2;1;3;5;1$v");
    assert_eq!(runs(&t, 2), [(4, 2, 0, 2)]);
    assert_eq!(runs(&t, 3), [(4, 2, 1, 2)]);
    assert_eq!((text(&t, 4, 2), text(&t, 4, 3)), ("A".into(), "A".into()));
    assert_eq!(runs(&t, 0), [(0, 2, 0, 2)], "the source keeps its run");
}

// DECCRA of a rectangle that holds only part of a run copies its cells as
// empty cells, as it copies half a wide character, and makes no run.
// covers: text.kitty-sizing
#[test]
fn deccra_of_part_of_a_run_copies_empty_cells() {
    let t = term(10, 4, "\x1b]66;s=2;A\x1b\\\x1b[1;1;1;2;1;4;1;1$v");
    assert!(runs(&t, 3).is_empty());
    assert_eq!(text(&t, 0, 3), "");
    assert_eq!(runs(&t, 0), [(0, 2, 0, 2)], "the source keeps its run");
}

// DECCRA whose target is clipped through a whole source run copies no
// run: the clipped copy would hold only part of it.
// covers: text.kitty-sizing
#[test]
fn deccra_whose_target_clips_a_run_copies_no_run() {
    let t = term(10, 4, "\x1b]66;s=2;A\x1b\\\x1b[1;1;2;2;1;4;1;1$v");
    assert!(runs(&t, 3).is_empty());
    assert_eq!(text(&t, 0, 3), "");
}

// DECCARA changes the style of a run's cells and keeps the run whole.
// covers: text.kitty-sizing
#[test]
fn deccara_over_part_of_a_run_keeps_the_run() {
    let t = term(10, 4, "\x1b]66;s=2;A\x1b\\\x1b[2*x\x1b[2;2;2;2;1$r");
    assert_eq!(runs(&t, 0), [(0, 2, 0, 2)]);
    assert_eq!(runs(&t, 1), [(0, 2, 1, 2)]);
    let line = t.screen().line(1).expect("row 1");
    assert!(
        line.style(line.cell(1)).attrs.contains(Attrs::BOLD),
        "the cell is bold"
    );
}

// A run whose top row is on a single-size line, and whose lower row would
// lie on a double-width line, is written as on a double-size line: one row
// high, at the width w gives. With w = 0 that is ordinary printed text.
// covers: text.kitty-sizing
#[test]
fn a_tall_run_over_a_double_width_line_is_one_row_high() {
    let t = term(10, 4, "\x1b[2;1H\x1b#6\x1b[1;7H\x1b]66;s=2;A\x1b\\");
    assert!(!any_run(&t));
    assert_eq!(text(&t, 6, 0), "A");
    assert_eq!(t.screen().cursor(), (7, 0));
    let t = term(10, 4, "\x1b[2;1H\x1b#6\x1b[1;1H\x1b]66;s=2:w=2;AB\x1b\\");
    assert_eq!(runs(&t, 0), [(0, 2, 0, 1)]);
    assert!(runs(&t, 1).is_empty());
    assert_eq!(t.screen().cursor(), (2, 0));
}

// DECLRMM returns every line to single size and keeps the runs on lines
// that were single size whole.
// covers: text.kitty-sizing
#[test]
fn declrmm_keeps_a_tall_run_beside_a_double_width_line_whole() {
    let t = term(
        10,
        4,
        "\x1b[3;1H\x1b#6\x1b[1;7H\x1b]66;s=2;A\x1b\\\x1b[?69h",
    );
    assert_eq!(runs(&t, 0), [(6, 2, 0, 2)]);
    assert_eq!(runs(&t, 1), [(6, 2, 1, 2)]);
}

// On the alternate screen, a resize that leaves a run's lower row below
// the screen erases the whole run.
// covers: text.kitty-sizing
#[test]
fn a_shorter_alternate_screen_erases_a_run_it_cuts() {
    let mut t = term(10, 4, "\x1b[?1049h\x1b[3;1H\x1b]66;s=2;A\x1b\\\x1b[1;1H");
    t.resize(10, 3);
    assert!(!any_run(&t), "row 2: {:?}", runs(&t, 2));
    assert_eq!(text(&t, 0, 2), "");
}

// A resize to fewer rows than a run has erases the run.
// covers: text.kitty-sizing
#[test]
fn a_screen_shorter_than_a_run_erases_it() {
    let mut t = term(10, 4, "\x1b[?1049h\x1b]66;s=3;A\x1b\\\x1b[1;1H");
    t.resize(10, 2);
    assert!(!any_run(&t));
}

// On the main screen, a resize that leaves a run's lower row below the
// screen erases the run, so the lines that come back when the screen grows
// again hold no part of it.
// covers: text.kitty-sizing
#[test]
fn a_shorter_main_screen_erases_a_run_it_cuts_and_growing_brings_back_none() {
    let mut t = term(17, 4, "\x1b[2;1H\x1b]66;s=2;A\x1b\\\x1b[1;1H");
    t.resize(17, 2);
    assert!(!any_run(&t), "row 1: {:?}", runs(&t, 1));
    t.input(b"\x1b[2;1Hx");
    t.resize(17, 4);
    assert!(
        !any_run(&t),
        "rows: {:?}",
        (0..4).map(|y| runs(&t, y)).collect::<Vec<_>>()
    );
}

// While the alternate screen is shown, a resize also erases each run of
// the main screen that the new height cuts.
// covers: text.kitty-sizing
#[test]
fn a_shorter_screen_erases_a_run_it_cuts_on_the_screen_not_shown() {
    let mut t = term(10, 4, "\x1b[3;1H\x1b]66;s=2;A\x1b\\\x1b[1;1H\x1b[?1049h");
    t.resize(10, 3);
    t.input(b"\x1b[?1049l");
    t.resize(10, 4);
    assert!(
        !any_run(&t),
        "rows: {:?}",
        (0..4).map(|y| runs(&t, y)).collect::<Vec<_>>()
    );
}

// A resize that keeps every row of a run on the screen keeps the run.
// covers: text.kitty-sizing
#[test]
fn a_shorter_screen_keeps_a_run_it_does_not_cut() {
    let mut t = term(10, 4, "\x1b]66;s=2;A\x1b\\\x1b[1;1H");
    t.resize(10, 3);
    assert_eq!(runs(&t, 0), [(0, 2, 0, 2)]);
    assert_eq!(runs(&t, 1), [(0, 2, 1, 2)]);
}

// A width change that rewraps the text above a run can push the run's
// lower row below the screen. When the screen then becomes shorter and
// scrolls, the run goes whole, its rows below the screen included, so a
// screen that grows again shows no part of it.
// covers: text.kitty-sizing
#[test]
fn a_run_that_a_rewrap_pushes_below_the_screen_goes_whole() {
    let mut t = term(10, 4, "abcdefghijklm\x1b[3;1H\x1b]66;s=2;A\x1b\\");
    t.resize(5, 3);
    check(&t, "at 5 × 3", None);
    t.resize(5, 6);
    check(&t, "at 5 × 6", None);
    assert!(
        !any_run(&t),
        "rows: {:?}",
        (0..6).map(|y| runs(&t, y)).collect::<Vec<_>>()
    );
}

// A screen that grows pulls lines back from the scrollback, which pushes
// the lines below the screen further down. A run whose rows that push
// would separate goes whole, so no part of it shows when the screen grows
// again.
// covers: text.kitty-sizing
#[test]
fn a_run_below_the_screen_that_growing_would_cut_goes_whole() {
    let mut t = term(10, 6, "\x1b[4;1H\x1b]66;s=3;A\x1b\\\x1b[1;1H");
    t.resize(10, 3);
    t.input(b"\x1b[3;1H\n");
    t.resize(10, 4);
    check(&t, "at 10 × 4", None);
    t.resize(10, 6);
    check(&t, "at 10 × 6", None);
    assert!(
        !any_run(&t),
        "rows: {:?}",
        (0..6).map(|y| runs(&t, y)).collect::<Vec<_>>()
    );
}

// A run on a line whose cluster budget is nearly spent keeps whole
// grapheme clusters: e with its accent, and not a with its accent cut off.
// covers: text.kitty-sizing
#[test]
fn a_run_on_a_full_line_keeps_whole_clusters() {
    // 4 cells give the line 24 bytes; e and eight U+0301 take 18 of them
    // with the length byte, which leaves 6.
    let heavy: String = core::iter::once('e')
        .chain(core::iter::repeat_n('\u{301}', 8))
        .collect();
    let t = term(
        4,
        2,
        &format!("{heavy}\x1b[1;3H\x1b]66;w=2;e\u{301}a\u{301}\x1b\\"),
    );
    assert_eq!(runs(&t, 0), [(2, 2, 0, 1)]);
    assert_eq!(text(&t, 2, 0), "e\u{301}");
}

// A line that a narrower screen holds to a smaller cluster budget keeps
// whole grapheme clusters of a run's text.
// covers: text.kitty-sizing
#[test]
fn a_narrower_screen_keeps_whole_clusters_of_a_runs_text() {
    // At 4 columns the line has 24 bytes; U+4E2D and seven U+0301 take 18
    // of them, which leaves 6 for the run's text.
    let heavy: String = core::iter::once('\u{4E2D}')
        .chain(core::iter::repeat_n('\u{301}', 7))
        .collect();
    let mut t = term(8, 2, &format!("{heavy}\x1b]66;w=2;e\u{301}a\u{301}\x1b\\"));
    assert_eq!(text(&t, 2, 0), "e\u{301}a\u{301}");
    t.resize(4, 2);
    assert_eq!(runs(&t, 0), [(2, 2, 0, 1)]);
    assert_eq!(text(&t, 2, 0), "e\u{301}");
}

// A run written over cells that hold clusters gives their bytes back
// before it stores its text, so the text keeps all the room those cells
// held.
// covers: text.kitty-sizing
#[test]
fn a_run_over_clusters_has_their_room_for_its_text() {
    // e and seven U+0301 take 16 of the 24 bytes, and the two clusters at
    // columns 2 and 3 take 4 each.
    let heavy: String = core::iter::once('e')
        .chain(core::iter::repeat_n('\u{301}', 7))
        .collect();
    let t = term(
        4,
        2,
        &format!("{heavy}\x1b[1;3He\u{301}e\u{301}\x1b[1;3H\x1b]66;w=2;a\u{301}\u{302}\x1b\\"),
    );
    assert_eq!(runs(&t, 0), [(2, 2, 0, 1)]);
    assert_eq!(text(&t, 2, 0), "a\u{301}\u{302}");
}

// A combining character that joins the text of a two-row run, when the
// top row's line cannot hold the longer text, leaves the text as it was
// on both rows, so the rows keep the same text.
// covers: text.kitty-sizing
#[test]
fn a_mark_that_one_row_cannot_hold_leaves_the_runs_text_on_every_row() {
    // e and nine U+0301 take 20 of the top line's 24 bytes, and the run's
    // text e with U+0301 takes the other 4.
    let heavy: String = core::iter::once('e')
        .chain(core::iter::repeat_n('\u{301}', 9))
        .collect();
    let t = term(
        4,
        3,
        &format!("{heavy}\x1b[1;3H\x1b]66;s=2:w=1;e\u{301}\x1b\\\u{302}"),
    );
    assert_eq!(runs(&t, 0), [(2, 2, 0, 2)]);
    assert_eq!(text(&t, 2, 0), "e\u{301}");
    assert_eq!(text(&t, 2, 1), "e\u{301}");
}

/// Edits that move, clear, overwrite or copy cells, and change the size of
/// lines or the margins. Each is input to the terminal as it is.
const EDITS: &[&str] = &[
    "\x1b[@",
    "\x1b[3@",
    "\x1b[P",
    "\x1b[2P",
    "\x1b[X",
    "\x1b[4X",
    "\x1b[K",
    "\x1b[1K",
    "\x1b[2K",
    "\x1b[J",
    "\x1b[1J",
    "\x1b[L",
    "\x1b[2L",
    "\x1b[M",
    "\x1b[2M",
    "\x1b[S",
    "\x1b[T",
    "\x1b[2T",
    "\x1b['}",
    "\x1b['~",
    "\x1bD",
    "\x1bM",
    "\n",
    "\r",
    "\x08",
    "\t",
    "\x1b#8",
    "\x1b[?69h\x1b[3;9s",
    "\x1b[?69l",
    "\x1b[2;5r",
    "\x1b[r",
    "\x1b[4h",
    "\x1b[4l",
    "\x1b[?7l",
    "\x1b[?7h",
    "\x1b[1;1;2;2$z",
    "\x1b[?1049h",
    "\x1b[?1049l",
    "\x1b[22J",
    "\x1b[3b",
    "\u{301}",
    "\u{FE0F}",
    "x",
    "\u{4E2D}",
    "e\u{301}",
    "\x1b[1\"q",
    "\x1b[0\"q",
    "\x1b[?J",
    "\x1b[?K",
    "\x1b#3",
    "\x1b#4",
    "\x1b#5",
    "\x1b#6",
];

/// A random rectangle edit: DECCRA, DECFRA, DECERA, DECSERA, DECCARA or
/// DECRARA, with corners anywhere on the screen and sometimes past it.
fn rect_edit(rng: &mut Rng, cols: usize, rows: usize) -> String {
    let mut corner = |n: usize| 1 + rng.below(n + 1);
    let (t, l, b, r) = (corner(rows), corner(cols), corner(rows), corner(cols));
    let (tt, ll) = (corner(rows), corner(cols));
    match rng.below(6) {
        0 => format!("\x1b[{t};{l};{b};{r};1;{tt};{ll};1$v"),
        1 => format!("\x1b[120;{t};{l};{b};{r}$x"),
        2 => format!("\x1b[{t};{l};{b};{r}$z"),
        3 => format!("\x1b[{t};{l};{b};{r}${{"),
        4 => format!("\x1b[{t};{l};{b};{r};1;7$r"),
        _ => format!("\x1b[{t};{l};{b};{r};1$t"),
    }
}

/// Checks that every run on the screen is whole: each line's run table
/// agrees with its cells' marks, no two runs on a line overlap, each row
/// stays within its memory bound, each run's rows below and above it on
/// the screen hold its other shares, no run reaches past the screen's last
/// row, and a run on a double-size line is one row high inside the line's
/// columns.
///
/// The scrollback keeps its rows as they are, so with a scrollback,
/// `kept` holds the lines that held a share of a run while they were in
/// the scrollback. A share on such a line may come back to the screen
/// without the run's other rows, and a run whose upper rows a scroll sent
/// into the scrollback keeps only its lower rows on the screen.
fn check(t: &Terminal, what: &str, kept: Option<&BTreeSet<LineId>>) {
    let screen = t.screen();
    let cols = screen.cols();
    let lines = screen.lines();
    for (y, line) in lines.iter().enumerate() {
        let runs = line.runs();
        let mut end = 0;
        for r in runs {
            assert!(r.col() >= end, "{what}: row {y}: runs overlap: {runs:?}");
            assert!(
                r.span().end <= line.len(),
                "{what}: row {y}: run past the line's end"
            );
            end = r.span().end;
        }
        for (x, c) in line.cells().iter().enumerate() {
            let covered = runs.iter().any(|r| r.span().contains(&x));
            assert_eq!(
                c.in_run(),
                covered,
                "{what}: row {y} column {x}: mark and table differ"
            );
        }
        let bytes = size_of::<Line>() + line.heap_bytes();
        assert!(
            bytes <= Line::bytes_max(cols),
            "{what}: row {y}: {bytes} bytes"
        );
        let size = line.flags().size();
        if kept.is_some_and(|k| k.contains(&line.id())) {
            continue;
        }
        for r in runs {
            if size.is_double() {
                assert_eq!(
                    r.rows(),
                    1,
                    "{what}: row {y}: a tall run on a double-size line"
                );
                assert!(
                    r.span().end <= size.columns(cols),
                    "{what}: row {y}: past the columns"
                );
            }
            let top = y.checked_sub(r.row());
            // With a scrollback, only the rows from this one down must be
            // on the screen.
            let first = if kept.is_some() { r.row() } else { 0 };
            for k in first..r.rows() {
                let Some(yy) = top.map(|top| top + k) else {
                    // The run's top row is in the scrollback.
                    continue;
                };
                assert!(
                    yy < lines.len(),
                    "{what}: run {r:?} on row {y} passes the last row"
                );
                let other = lines[yy].run_at(r.col());
                let same = other
                    .is_some_and(|o| o.span() == r.span() && o.row() == k && o.size() == r.size());
                assert!(
                    same,
                    "{what}: run {r:?} on row {y} lacks its row {k} on row {yy}: {other:?}"
                );
                if r.rows() > 1 {
                    assert_eq!(
                        lines[yy].flags().size(),
                        LineSize::Single,
                        "{what}: row {yy}"
                    );
                }
            }
        }
    }
}

// Random OSC 66 runs mixed with random edits, the rectangle edits, line
// size changes and resizes that change the width and the height: after
// every step each run on the screen is whole. The seeds are fixed, so a
// failure names its seed and its step; VTIUM_TERM_SEEDS runs more seeds.
// covers: text.kitty-sizing
#[test]
fn random_runs_and_edits_keep_every_run_whole() {
    for seed in 0..common::seeds(150) {
        let mut rng = Rng(seed);
        let cols = 4 + rng.below(20);
        let rows = 3 + rng.below(8);
        let mut t = Terminal::new(cols, rows, 50);
        // Half the seeds keep a scrollback, and half keep none, so a
        // shorter screen keeps its lowest lines below it. The scrollback
        // keeps a run's rows as they are, so a screen that grows can bring
        // back part of a run; with a scrollback, a resize keeps the height.
        let scrollback = seed % 2 == 0;
        if scrollback {
            t.screen_mut().set_scrollback_max(50);
        }
        let mut log: Vec<String> = Vec::new();
        let mut kept = BTreeSet::new();
        let note = |t: &Terminal, kept: &mut BTreeSet<LineId>| {
            let lines = t.screen().scrollback().filter(|l| !l.runs().is_empty());
            kept.extend(lines.map(Line::id));
        };
        for step in 0..200 {
            note(&t, &mut kept);
            let seq = match rng.below(12) {
                0..=3 => {
                    let s = 1 + rng.below(4);
                    let w = rng.below(4);
                    let text = *rng.pick(&[
                        "A",
                        "BC",
                        "\u{4E2D}",
                        "e\u{301}",
                        "xyz",
                        "\u{1F600}",
                        "ab\u{301}c",
                    ]);
                    format!("\x1b]66;s={s}:w={w};{text}\x1b\\")
                }
                4 => format!("\x1b[{};{}H", 1 + rng.below(rows), 1 + rng.below(cols)),
                5 => rect_edit(&mut rng, cols, rows),
                6 if rng.below(4) == 0 => {
                    let (c, r) = (2 + rng.below(24), 2 + rng.below(9));
                    let r = if scrollback { rows } else { r };
                    t.resize(c, r);
                    note(&t, &mut kept);
                    log.push(format!("resize {c}×{r}"));
                    check(
                        &t,
                        &format!("seed {seed} step {step} after {log:?}"),
                        scrollback.then_some(&kept),
                    );
                    t.resize(cols, rows);
                    log.push(format!("resize {cols}×{rows}"));
                    String::new()
                }
                _ => (*rng.pick(EDITS)).to_owned(),
            };
            t.input(seq.as_bytes());
            log.push(seq);
            if log.len() > 12 {
                log.remove(0);
            }
            check(
                &t,
                &format!("seed {seed} step {step} after {log:?}"),
                scrollback.then_some(&kept),
            );
        }
    }
}
