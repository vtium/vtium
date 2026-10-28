//! The cluster arena's bound (ADR-1001, "The line") on every path that
//! writes, moves or erases cells: a flood of long clusters through each
//! path, and after every step, for every line of the screen and the
//! scrollback, the bytes its cells' clusters take, counted from the cells,
//! equal what the line accounts for, stay inside the budget of
//! `CLUSTER_BYTES_PER_CELL` a cell, and its heap stays inside the bound its
//! tables compact at. The budget is the width the line was written at: a
//! line trimmed for the scrollback keeps the clusters its cells name, which
//! may pass the budget of its fewer cells but never that of the widest
//! screen.
//!
//! A rewrap (ADR-1003) moves clusters with their cells, so a row may name
//! more than its own cells' budget, and the bounds are the logical line's
//! and the scrollback's:
//!
//! - A row written `w` columns wide names at most `CLUSTER_BYTES_PER_CELL *
//!   w` bytes. Every cell of a wrapped row is text but a pad at its end,
//!   so it names at most twice the budget a cell of text (at `w` = 2, one
//!   cell beside a pad); a logical line's last row may hold its budget on
//!   one cell. A logical line of `n` rows at most `W` cells wide holds at
//!   most `nW` cells of text, so it names at most `12nW + 6W`: the budget
//!   of `2n + 1` rows of the widest screen.
//! - Its heap is that budget's `heap_bound` and 256 bytes a row: each row
//!   holds its tables' 80-byte struct and up to two style entries of 14
//!   bytes past its cells' own, its fill's and a pad's.
//! - Across the scrollback and the screen the rows name at most
//!   `row_bound(W)` bytes a row on average. Every byte was written within a
//!   row's budget: a wrapped row's beyond 6 bytes a cell of its text only
//!   for a pad, and a pad needs a cell of text in its row, so pads add at
//!   most 6 bytes a cell (more than the half the wide character after a
//!   pad would allow, since that character may be erased); a logical
//!   line's last row holds at most `min(6w, CLUSTER_BYTES_MAX * t)` on its
//!   `t` cells, and there is at most one last row a row. So a row names at
//!   most `12W` and the most a last row's few cells hold beyond 6 bytes
//!   each: 3,012 bytes at the 170-column minspec grid, 2.95 times its
//!   budget, where ADR-1001 took 1,020. The shapes below reach twice the
//!   budget in a row and about 1.4 times it on average.

mod common;

use common::Rng;
use vtium_term::{CLUSTER_BYTES_PER_CELL, CLUSTER_MAX, Glyph, Line, LineFlags, Screen, Terminal};

/// The bytes the clusters a line's cells name take, length bytes included,
/// counted from the cells.
fn named_bytes(line: &Line) -> usize {
    line.cells()
        .iter()
        .map(|&c| match line.glyph(c) {
            Glyph::Cluster(s) => s.len() + 1,
            _ => 0,
        })
        .sum()
}

/// The heap a line of `len` cells may hold: its cells, and tables that
/// compact at twice what the line can name, each in a vector that may have
/// doubled its capacity.
fn heap_bound(len: usize) -> usize {
    let cells = len.max(1);
    let styles = 2 * cells * 14;
    let links = 2 * cells * 4;
    let arena = 2 * cells * CLUSTER_BYTES_PER_CELL + 1 + CLUSTER_MAX * 4;
    2 * (cells * 8 + styles + links + arena) + 256
}

/// Checks every line of `screen`, written on screens at most `widest`
/// columns wide: each row's accounting, and each logical line's bytes and
/// heap against the budget of `2n + 1` rows of the widest screen for its
/// `n` rows, as the module's notes derive.
fn check(screen: &Screen, widest: usize, path: &str, step: usize) {
    let mut lines: Vec<&Line> = screen.scrollback().collect();
    lines.extend(screen.lines());
    let (mut named, mut heap, mut rows) = (0, 0, 0);
    for (i, line) in lines.iter().enumerate() {
        let bytes = named_bytes(line);
        assert_eq!(
            bytes,
            line.cluster_bytes(),
            "{path}, step {step}, line {i}: the cells name {bytes} bytes, the line accounts {}",
            line.cluster_bytes()
        );
        named += bytes;
        heap += line.heap_bytes();
        rows += 1;
        if line.flags().contains(LineFlags::WRAPPED) && i + 1 != lines.len() {
            continue;
        }
        let cells = (2 * rows + 1) * line.len().max(widest);
        assert!(
            named <= cells * CLUSTER_BYTES_PER_CELL,
            "{path}, step {step}, line {i}: {named} bytes in {rows} rows over a budget of {cells} cells"
        );
        assert!(
            heap <= heap_bound(cells) + rows * 256,
            "{path}, step {step}, line {i}: {heap} heap bytes in {rows} rows"
        );
        (named, heap, rows) = (0, 0, 0);
    }
}

/// A cluster of `marks` combining marks on a base, at most `CLUSTER_MAX`
/// codepoints of three UTF-8 bytes each after the first.
fn cluster(marks: usize) -> String {
    std::iter::once('e')
        .chain(std::iter::repeat_n('\u{20D0}', marks))
        .collect()
}

/// Floods `path`'s sequences between prints of long clusters, wide ones
/// widened by a VS16, at random places.
fn flood(path: &str, sequences: &[&str], setup: impl Fn(&mut Screen)) {
    let mut rng = Rng(0x5eed ^ path.len() as u64);
    let mut t = Terminal::new(9, 4, 50);
    t.screen_mut().set_scrollback_max(50);
    setup(t.screen_mut());
    for step in 0..3000 {
        let row = 1 + rng.below(4);
        let col = 1 + rng.below(9);
        t.input(format!("\x1b[{row};{col}H").as_bytes());
        let marks = rng.below(CLUSTER_MAX + 4);
        let text = if rng.below(4) == 0 {
            format!("\u{2764}\u{FE0F}{}", cluster(marks))
        } else {
            cluster(marks)
        };
        t.input(text.as_bytes());
        t.input(rng.pick(sequences).as_bytes());
        check(t.screen(), 9, path, step);
    }
}

#[test]
fn printing_appends_and_widens_clusters_inside_the_budget() {
    flood(
        "print",
        &["", "\u{20D0}\u{20D0}\u{20D0}", "\u{FE0F}", "\r\n"],
        |_| {},
    );
}

#[test]
fn inserting_and_deleting_characters_give_clusters_back() {
    flood(
        "ICH, DCH",
        &["\x1b[@", "\x1b[3@", "\x1b[P", "\x1b[4P"],
        |_| {},
    );
}

#[test]
fn insert_mode_writes_and_widening_give_clusters_back() {
    flood("IRM", &["", "x", "\u{4E2D}", "\u{2764}\u{FE0F}"], |s| {
        s.set_insert_mode(true)
    });
}

#[test]
fn erasing_gives_clusters_back() {
    flood(
        "ED, EL, ECH",
        &[
            "\x1b[K", "\x1b[1K", "\x1b[2K", "\x1b[J", "\x1b[1J", "\x1b[3X",
        ],
        |_| {},
    );
}

#[test]
fn scrolling_lines_and_regions_keeps_clusters_bounded() {
    flood(
        "SU, SD, IL, DL, IND, RI",
        &[
            "\x1b[S",
            "\x1b[2T",
            "\x1b[L",
            "\x1b[2M",
            "\x1bD",
            "\x1bM",
            "\x1b[2;3r",
            "\x1b[r",
        ],
        |_| {},
    );
}

#[test]
fn rectangles_copy_restyle_fill_and_erase_clusters_inside_the_budget() {
    flood(
        "DECCRA, DECCARA, DECRARA, DECFRA, DECERA, DECSERA",
        &[
            "\x1b[1;1;2;9;1;3;1;1$v",
            "\x1b[3;2;4;9;1;1;1;1$v",
            "\x1b[1;1;4;9;1;2;3$v",
            "\x1b[2*x\x1b[1;1;4;9;1;44$r",
            "\x1b[1;1;4;9;0;4:3$r",
            "\x1b[*x\x1b[1;1;4;9;7$t",
            "\x1b[120;2;2;3;4$x",
            "\x1b[2;2;3;4$z",
            "\x1b[1;3;4;5${",
        ],
        |_| {},
    );
}

#[test]
fn rep_and_wrapping_keep_clusters_bounded() {
    flood("REP", &["\x1b[5b", "\x1b[30b"], |_| {});
}

#[test]
fn resizing_and_the_alternate_screen_keep_clusters_bounded() {
    let mut rng = Rng(7);
    let mut t = Terminal::new(9, 4, 50);
    t.screen_mut().set_scrollback_max(50);
    for step in 0..2000 {
        t.input(cluster(rng.below(CLUSTER_MAX + 4)).as_bytes());
        match rng.below(6) {
            0 => t.screen_mut().resize(1 + rng.below(12), 1 + rng.below(6)),
            1 => {
                let on = rng.below(2) == 0;
                t.screen_mut().set_alternate(on);
            }
            2 => {
                t.input(b"\r\n");
            }
            _ => {}
        }
        check(t.screen(), 12, "resize, alternate", step);
        if !t.screen().alternate() {
            t.screen_mut().rewrap_scrollback();
            check_total(t.screen(), 12, step);
        }
    }
}

/// The most a row of the scrollback and the screen names on average, for
/// a widest screen of `widest` columns: see the module's notes.
fn row_bound(widest: usize) -> usize {
    let budget = CLUSTER_BYTES_PER_CELL * widest;
    let cluster_max = 1 + CLUSTER_MAX * 4;
    let last = (1..=widest)
        .map(|t| budget.min(cluster_max * t) - CLUSTER_BYTES_PER_CELL * t)
        .max()
        .unwrap_or(0);
    2 * budget + last
}

/// The scrollback's and the screen's rows name at most `row_bound` bytes
/// a row; returns what they name a row on average, and the most one row
/// names.
fn check_total(screen: &Screen, widest: usize, step: usize) -> (usize, usize) {
    let rows: Vec<&Line> = screen.scrollback().chain(screen.lines()).collect();
    let named: usize = rows.iter().map(|l| named_bytes(l)).sum();
    let bound = row_bound(widest) * rows.len();
    assert!(
        named <= bound,
        "step {step}: {named} bytes in {} rows, over {bound}",
        rows.len()
    );
    let most = rows.iter().map(|l| named_bytes(l)).max().unwrap_or(0);
    (named / rows.len().max(1), most)
}

/// A terminal `narrow` columns wide fed `text`, widened to `widest` and
/// the rows the widening freed filled with rows at the full budget; the
/// bytes it names a row on average, and the most one row names.
fn widened(narrow: usize, widest: usize, text: &str) -> (usize, usize) {
    let mut t = Terminal::new(narrow, 4, 60);
    t.screen_mut().set_scrollback_max(60);
    t.input(text.as_bytes());
    t.screen_mut().resize(widest, 4);
    t.screen_mut().rewrap_scrollback();
    let merged = t.screen().scrollback().len();
    let full = "e\u{301}\u{301}".repeat(widest);
    for _ in merged..60 {
        t.input(format!("{full}\r\n").as_bytes());
    }
    check_total(t.screen(), widest, 0)
}

// ADR-1003, "The scrollback's bound": shapes that pass the widest screen's
// budget a row once widened, each inside the bound the module's notes
// derive. The pad a wide character leaves gives a row's budget to fewer
// cells of text, more so where the wide character is erased after it; a
// last row may hold its row's budget on a few cells.
#[test]
fn a_rewrap_names_at_most_the_derived_bound_a_row() {
    let budget = |w: usize| CLUSTER_BYTES_PER_CELL * w;
    for (narrow, widest) in [(3, 12), (2, 12), (5, 12), (3, 9), (11, 12), (7, 12)] {
        // A wide cluster that fills a row of `narrow` columns' budget.
        let marks = (CLUSTER_BYTES_PER_CELL * narrow - 4) / 2;
        let wide = format!("\u{4E2D}{}", "\u{301}".repeat(marks));
        let (named, _) = widened(narrow, widest, &format!("{}\r\n", wide.repeat(200)));
        if (narrow, widest) == (3, 12) {
            assert!(named > budget(widest));
        }
    }

    // Logical lines of two rows printed 11 columns wide: a heavy wide
    // character, four plain ones and a pad, then a last row of one heavy
    // wide character. Widened to 12, each is one row of two rows' budget.
    let heavy_wide = format!("\u{4E2D}{}", "\u{301}".repeat(30));
    let line = format!("{heavy_wide}{}{heavy_wide}\r\n", "\u{4E2D}".repeat(4));
    let (named, most) = widened(11, 12, &line.repeat(40));
    assert!(named > budget(13), "{named} a row");
    assert!(most > 2 * budget(11) - 10, "{most} in a row");

    // Rows printed 2 columns wide of a heavy cell and a pad, the wide
    // character after each erased: every row's budget on one cell of text,
    // so a row widened to 12 names twice its budget.
    let heavy = format!("e{}", "\u{301}".repeat(5));
    let line = format!("{}\r\n", format!("{heavy}\u{4E2D}\r\x1b[2X").repeat(30));
    let (_, most) = widened(2, 12, &line.repeat(4));
    assert_eq!(most, 2 * budget(12));
}
