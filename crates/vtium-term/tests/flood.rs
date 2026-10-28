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
//! - Across the scrollback and the screen the rows name at most the budget
//!   of `W + 1` columns a row. A widening joins rows written narrower,
//!   each with at most one pad column of budget its text does not use,
//!   and frees rows that new text fills; a narrowing spreads text over more
//!   rows, the oldest past `sb-size` dropped. So the worst case ADR-1001
//!   computes for a full-width row grows by one column's budget, 6 bytes.

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

/// The scrollback's and the screen's rows name at most the budget of one
/// column more than the widest screen a row: see the module's notes.
fn check_total(screen: &Screen, widest: usize, step: usize) {
    let rows: Vec<&Line> = screen.scrollback().chain(screen.lines()).collect();
    let named: usize = rows.iter().map(|l| named_bytes(l)).sum();
    let bound = CLUSTER_BYTES_PER_CELL * (widest + 1) * rows.len();
    assert!(
        named <= bound,
        "step {step}: {named} bytes in {} rows, over {bound}",
        rows.len()
    );
}

// ADR-1003, "The scrollback's bound": rows printed narrow with a wide
// character and a pad, widened, and the rows freed filled with full rows,
// name more than the widest screen's budget a row, and no more than one
// column's more.
#[test]
fn a_rewrap_names_at_most_one_column_more_a_row() {
    let heavy = "e\u{301}\u{301}";
    for (narrow, widest) in [(3, 12), (2, 12), (5, 12), (3, 9), (11, 12), (7, 12)] {
        // A wide cluster that fills a row of `narrow` columns' budget.
        let marks = (CLUSTER_BYTES_PER_CELL * narrow - 4) / 2;
        let wide = format!("\u{4E2D}{}", "\u{301}".repeat(marks));
        let mut t = Terminal::new(narrow, 4, 60);
        t.screen_mut().set_scrollback_max(60);
        t.input(wide.repeat(200).as_bytes());
        t.input(b"\r\n");
        t.screen_mut().resize(widest, 4);
        t.screen_mut().rewrap_scrollback();
        let merged = t.screen().scrollback().len();
        for _ in merged..60 {
            t.input(format!("{}\r\n", heavy.repeat(widest)).as_bytes());
        }
        check_total(t.screen(), widest, 0);
        if (narrow, widest) == (3, 12) {
            let s = t.screen();
            let rows: Vec<&Line> = s.scrollback().chain(s.lines()).collect();
            let named: usize = rows.iter().map(|l| named_bytes(l)).sum();
            assert!(named > CLUSTER_BYTES_PER_CELL * widest * rows.len());
        }
    }
}
