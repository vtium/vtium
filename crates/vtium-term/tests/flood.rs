//! The cluster arena's bound (ADR-1001, "The line") on every path that
//! writes, moves or erases cells: a flood of long clusters through each
//! path, and after every step, for every line of the screen and the
//! scrollback, the bytes its cells' clusters take, counted from the cells,
//! equal what the line accounts for, stay inside the budget of
//! `CLUSTER_BYTES_PER_CELL` a cell, and its heap stays inside the bound its
//! tables compact at. The budget is the width the line was written at: a
//! line trimmed for the scrollback keeps the clusters its cells name, which
//! may pass the budget of its fewer cells but never that of the widest
//! screen. A rewrap (ADR-1003) moves clusters with their cells and holds
//! them to the logical line's budget as written: a written row of `w`
//! columns names at most `CLUSTER_BYTES_PER_CELL * w` bytes on at least
//! half its columns of text but its last, a pad taking at most one, so a
//! logical line of `n` rows at most the widest screen wide names at most
//! the budget of `2n + 1` rows of it.

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
/// `n` rows.
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
    }
}
