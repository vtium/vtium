use super::tables::{ARENA_OFFSETS, ClusterArena, U24, cluster_at, reserve_within};
use super::*;
use crate::bounds::{CLUSTER_BYTES_MAX, CLUSTER_MAX, PLACEHOLDER_MARKS};
use crate::style::{Attrs, Color, Rgb};

const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";

fn red() -> Style {
    Style {
        fg: Color::Indexed(1),
        ..Style::DEFAULT
    }
}

fn rgb(n: u32) -> Style {
    let [_, r, g, b] = n.to_be_bytes();
    Style {
        fg: Color::Rgb(Rgb::new(r, g, b)),
        ..Style::DEFAULT
    }
}

/// `e` and `marks` combining acute accents.
fn accented(marks: usize) -> String {
    std::iter::once('e')
        .chain(std::iter::repeat_n('\u{301}', marks))
        .collect()
}

fn tables(line: &Line) -> &Tables {
    line.tables.as_ref().unwrap()
}

#[test]
fn a_new_line_is_blank_and_has_no_tables() {
    let line = Line::new(LineId::new(1), 80);
    assert_eq!(line.len(), 80);
    assert!(line.cells().iter().all(|c| c.is_blank()));
    assert_eq!(line.heap_bytes(), 80 * 8);
}

#[test]
fn a_written_cell_reads_back_through_its_tables() {
    let mut line = Line::new(LineId::new(1), 10);
    let mut style = red();
    style.attrs.insert(Attrs::OVERLINE);
    style.attrs.insert(Attrs::CONCEAL);
    let link = LinkId::new(42);
    assert!(line.write(3, Glyph::Char('x'), Width::Narrow, &style, Some(link)));
    let cell = line.cell(3);
    assert_eq!(line.glyph(cell), Glyph::Char('x'));
    assert_eq!(line.style(cell), style);
    assert_eq!(line.link(cell), Some(link));

    assert!(line.write(4, Glyph::Cluster(FAMILY), Width::Wide, &style, None));
    let cell = line.cell(4);
    assert_eq!(line.glyph(cell), Glyph::Cluster(FAMILY));
    assert_eq!(cell.width(), Width::Wide);
    assert_eq!(line.link(cell), None);
}

#[test]
fn a_cluster_of_one_codepoint_is_that_codepoint() {
    let mut line = Line::new(LineId::new(1), 4);
    line.write(0, Glyph::Cluster("e"), Width::Narrow, &Style::DEFAULT, None);
    assert_eq!(line.cell(0).content(), Content::Char('e'));
    line.write(1, Glyph::Cluster(""), Width::Narrow, &Style::DEFAULT, None);
    assert!(line.cell(1).is_blank());
    // Neither needed a table.
    assert!(line.tables.is_none());
    assert_eq!(line.heap_bytes(), 4 * 8);
}

// ADR-1001, "The line": extenders past 32 are dropped.
#[test]
fn a_cluster_keeps_its_first_cluster_max_codepoints() {
    let mut line = Line::new(LineId::new(1), 64);
    let long = accented(40);
    line.write(
        0,
        Glyph::Cluster(&long),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    assert_eq!(
        line.glyph(line.cell(0)),
        Glyph::Cluster(&accented(CLUSTER_MAX - 1))
    );
}

// ADR-1001, "The line": a cluster past the line's budget keeps the
// longest run of its first codepoints that fits, at least its first.
#[test]
fn a_cluster_past_the_budget_keeps_what_fits() {
    // Four cells: 24 bytes. The first cluster takes 1 + 1 + 2 * 11.
    let mut line = Line::new(LineId::new(1), 4);
    line.write(
        0,
        Glyph::Cluster(&accented(31)),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    assert_eq!(line.glyph(line.cell(0)), Glyph::Cluster(&accented(11)));
    line.write(
        1,
        Glyph::Cluster(&accented(31)),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    assert_eq!(line.cell(1).content(), Content::Char('e'));
    // Overwriting the first gives its bytes back.
    line.write(0, Glyph::Char('a'), Width::Narrow, &Style::DEFAULT, None);
    line.write(
        1,
        Glyph::Cluster(&accented(2)),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    assert_eq!(line.glyph(line.cell(1)), Glyph::Cluster(&accented(2)));
}

/// The arena bytes the line's cells name, counted from the cells.
fn named_bytes(line: &Line) -> usize {
    line.cells()
        .iter()
        .map(|&c| match line.glyph(c) {
            Glyph::Cluster(s) => s.len().saturating_add(1),
            _ => 0,
        })
        .sum()
}

// Clusters of mixed sizes, maximal ones among them, written at
// scattered columns through many compactions: the bytes the cells
// name stay inside the budget, and `live` agrees with them.
#[test]
fn a_flood_of_mixed_clusters_stays_inside_the_budget() {
    for cols in [4, 21, 170] {
        let mut line = Line::new(LineId::new(1), cols);
        let mut seed = 0x9e37_79b9_u32;
        for _ in 0..20_000 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let col = (seed >> 8) as usize % cols;
            let marks = (seed >> 20) as usize % CLUSTER_MAX;
            let cluster = accented(marks);
            line.write(
                col,
                Glyph::Cluster(&cluster),
                Width::Narrow,
                &Style::DEFAULT,
                None,
            );
            let named = named_bytes(&line);
            assert!(
                named <= cols * CLUSTER_BYTES_PER_CELL,
                "{named} bytes at {cols} cells"
            );
            assert_eq!(line.tables.as_ref().map_or(0, |t| t.live()), named);
            assert!(
                line.tables.as_ref().map_or(0, |t| t.clusters.len())
                    <= compaction_limit(cols * CLUSTER_BYTES_PER_CELL)
            );
            match line.glyph(line.cell(col)) {
                Glyph::Char('e') => {}
                Glyph::Cluster(s) => assert!(cluster.starts_with(s)),
                other => panic!("cell {col} holds {other:?}"),
            }
        }
        line.trim();
        assert!(named_bytes(&line) <= cols * CLUSTER_BYTES_PER_CELL);
    }
}

#[test]
fn a_write_past_the_end_changes_nothing() {
    let mut line = Line::new(LineId::new(1), 4);
    assert!(!line.write(4, Glyph::Char('x'), Width::Narrow, &red(), None));
    assert_eq!(line.len(), 4);
    assert!(line.cells().iter().all(|c| c.is_blank()));
}

// Cells past a stored line's end read as the blank cell in the default
// style, so an SGR under BCE does not recolor them.
#[test]
fn cells_past_the_end_read_blank_in_the_default_style() {
    let mut line = Line::new(LineId::new(1), 4);
    line.write(0, Glyph::Char('a'), Width::Narrow, &red(), None);
    line.trim();
    assert_eq!(line.len(), 1);
    let past = line.cell(10);
    assert!(past.is_blank());
    assert_eq!(line.style(past), Style::DEFAULT);
    assert_eq!(line.glyph(past), Glyph::Empty);
}

#[test]
fn trim_keeps_cells_erased_in_a_color() {
    let mut line = Line::new(LineId::new(1), 8);
    line.write(0, Glyph::Char('a'), Width::Narrow, &Style::DEFAULT, None);
    let bce = Style {
        bg: Color::Indexed(4),
        ..Style::DEFAULT
    };
    line.erase(2..5, &bce);
    line.trim();
    assert_eq!(line.len(), 5);
    assert_eq!(line.style(line.cell(4)).bg, Color::Indexed(4));
    assert!(line.cell(1).is_blank());
}

// ADR-1003, "Logical lines": a line's fill is the run of empty cells
// of one style that ends it; empty cells before it stay as text, and
// cells past its end read in its style.
#[test]
fn trim_keeps_the_fill_as_the_style_past_the_end() {
    let bce = Style {
        bg: Color::Indexed(4),
        ..Style::DEFAULT
    };
    let mut line = Line::new(LineId::new(1), 8);
    line.write(0, Glyph::Char('a'), Width::Narrow, &Style::DEFAULT, None);
    line.erase(3..8, &bce);
    line.trim();
    assert_eq!(line.len(), 3);
    assert_eq!(line.fill(), bce);
    assert_eq!(line.style(line.cell(6)), bce);
    assert!(line.cell(1).is_blank());
    // The fill survives compaction, and paints a widening.
    line.resize(10);
    assert_eq!(line.style(line.cell(9)), bce);
    assert_eq!(line.cell(9).content(), Content::Empty);
}

// A wrapped line's cells are all text: trimming keeps them.
#[test]
fn trim_keeps_every_cell_of_a_wrapped_line() {
    let mut line = Line::new(LineId::new(1), 6);
    line.write(0, Glyph::Char('a'), Width::Narrow, &Style::DEFAULT, None);
    line.flags_mut().insert(LineFlags::WRAPPED);
    line.trim();
    assert_eq!(line.len(), 6);
    line.unwrap();
    line.trim();
    assert_eq!(line.len(), 1);
}

// A pad belongs only in the last column of a wrapped line.
#[test]
fn a_pad_that_leaves_the_end_of_a_wrapped_line_is_an_empty_cell() {
    let pad = |line: &Line, col| line.cell(col).width() == Width::Pad;
    let mut line = Line::new(LineId::new(1), 4);
    line.write(3, Glyph::Empty, Width::Pad, &red(), None);
    line.flags_mut().insert(LineFlags::WRAPPED);
    let mut moved = line.clone();
    moved.delete_cells(1, 1, 4, &Style::DEFAULT);
    assert!(!pad(&moved, 2) && moved.cell(2).content() == Content::Empty);
    assert_eq!(moved.style(moved.cell(2)), red());
    line.unwrap();
    assert!(!pad(&line, 3));
    assert_eq!(line.style(line.cell(3)), red());
}

// A shrink cuts a wide character at the new end whole, and ends the
// wrap of a line whose text it cut.
#[test]
fn shrinking_cuts_a_wide_character_and_ends_the_wrap() {
    let mut line = Line::new(LineId::new(1), 4);
    line.write(
        1,
        Glyph::Cluster(FAMILY),
        Width::Wide,
        &Style::DEFAULT,
        None,
    );
    line.write(2, Glyph::Empty, Width::WideTail, &Style::DEFAULT, None);
    line.flags_mut().insert(LineFlags::WRAPPED);
    line.resize(2);
    assert_eq!(line.cell(1).content(), Content::Empty);
    assert_eq!(line.cell(1).width(), Width::Narrow);
    assert_eq!(line.cluster_bytes(), 0);
    assert!(!line.flags().contains(LineFlags::WRAPPED));
}

#[test]
fn erase_clips_to_the_line_and_gives_back_clusters() {
    let mut line = Line::new(LineId::new(1), 4);
    line.write(
        3,
        Glyph::Cluster(&accented(3)),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    assert_eq!(tables(&line).live(), 8);
    line.erase(2..100, &red());
    assert!(line.cell(1).is_blank());
    assert_eq!(line.style(line.cell(3)), red());
    assert_eq!(line.glyph(line.cell(3)), Glyph::Empty);
    assert_eq!(tables(&line).live(), 0);
    assert_eq!(line.len(), 4);
}

#[test]
fn rewriting_one_cell_in_new_styles_keeps_the_table_bounded() {
    let mut line = Line::new(LineId::new(1), 4);
    for n in 0..10_000 {
        line.write(0, Glyph::Char('a'), Width::Narrow, &rgb(n), None);
        let t = tables(&line).styles.len();
        assert!(t <= table_limit(4), "the style table grew to {t}");
        assert_eq!(line.style(line.cell(0)), rgb(n));
    }
}

#[test]
fn compaction_keeps_what_every_cell_names() {
    let cols = 16;
    let mut line = Line::new(LineId::new(1), cols);
    let cluster = accented(1);
    // Fill every cell with its own style, link and cluster, then
    // overwrite them round after round so every table compacts.
    for round in 0..20u32 {
        for (col, c) in (0..cols).zip(0u32..) {
            let n = round * 100 + c;
            let link = LinkId::new(n);
            line.write(
                col,
                Glyph::Cluster(&cluster),
                Width::Narrow,
                &rgb(n),
                Some(link),
            );
        }
        for (col, c) in (0..cols).zip(0u32..) {
            let n = round * 100 + c;
            let cell = line.cell(col);
            assert_eq!(line.style(cell), rgb(n));
            assert_eq!(line.link(cell), Some(LinkId::new(n)));
            assert_eq!(line.glyph(cell), Glyph::Cluster(&cluster));
        }
        assert!(tables(&line).styles.len() <= table_limit(cols));
    }
}

/// A cluster of exactly `bytes` UTF-8 bytes, 2 to 128: a base of 1 to
/// 4 bytes and marks of 4 bytes, or a mark of 1 to 3 bytes; a single
/// letter for 1 byte.
fn sized(bytes: usize) -> String {
    match bytes {
        1 => "x".into(),
        2 => "ea".into(),
        3 => "e\u{301}".into(),
        4 => "e\u{20D0}".into(),
        _ => {
            let rest = bytes.saturating_sub(1);
            let base = match rest % 4 {
                0 => 'e',
                1 => '\u{E9}',
                2 => '\u{4E2D}',
                _ => '\u{1F600}',
            };
            std::iter::once(base)
                .chain(std::iter::repeat_n('\u{E0100}', rest / 4))
                .collect()
        }
    }
}

// A table's capacity stops at the limit it compacts at, so a row
// `write` fills holds no room past what `Line::bytes_max` prices. At
// 17 columns no limit is a power of two, so a table that only doubled
// would pass it. Each write takes a style and a link no other write
// takes; sweeps of plain text and of small clusters, with each size of
// cluster in the first cell, bring each table to its limit and no
// further. `tests/memory.rs` drives rows to the whole bound.
// The arena compacts only when the cluster a write adds would take it
// past its limit: on a row of 10 cells, whose arena holds 90 bytes,
// 30 overwrites of a 2-byte cluster fill it with dead clusters, and
// the 31st compacts it.
#[test]
fn the_arena_compacts_on_the_bytes_a_write_adds() {
    let mut line = Line::new(LineId::new(1), 10);
    let cluster = sized(2);
    for _ in 0..30 {
        line.write(
            0,
            Glyph::Cluster(&cluster),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
    }
    assert_eq!(tables(&line).clusters.len(), 90);
    line.write(
        0,
        Glyph::Cluster(&cluster),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    assert_eq!(tables(&line).clusters.len(), 3);
    assert_eq!(line.glyph(line.cell(0)), Glyph::Cluster(&cluster));
}

// The line's cells bound what its run table allocates: a run in each of
// 170 cells leaves the table's capacity at 170, where a vector's own
// doubling would reach 256, and a line cut to 40 cells gives the room
// back.
//
// References:
// - ADR-1009 D1: the cell count bounds the run table's allocation.
#[test]
fn the_run_table_allocates_no_more_than_the_cells() {
    let mut line = Line::new(LineId::new(1), 170);
    let size = TextSize {
        width: 1,
        ..TextSize::NORMAL
    };
    let mut most = 0;
    for col in 0..170 {
        let run = SizedRun::new(col, 1, 0, size);
        assert!(line.put_run(run, Glyph::Char('x'), &rgb(1), None));
        most = most.max(tables(&line).runs.0.as_ref().map_or(0, |r| r.capacity()));
    }
    let runs = tables(&line)
        .runs
        .0
        .as_ref()
        .map(|r| (r.len(), r.capacity()));
    assert_eq!(runs, Some((170, 170)));
    assert!(most <= 170, "the run table allocated {most} entries");
    line.resize(40);
    let runs = tables(&line)
        .runs
        .0
        .as_ref()
        .map(|r| (r.len(), r.capacity()));
    assert_eq!(runs, Some((40, 40)));
}

// A run's entry keeps every key at the most each one takes, in 6 bytes.
#[test]
fn a_sized_run_keeps_its_keys_in_six_bytes() {
    let size = TextSize {
        scale: 7,
        width: 7,
        numerator: 15,
        denominator: 15,
        vertical: 2,
        horizontal: 2,
    };
    let run = SizedRun::new(8191, 49, 6, size);
    assert_eq!(size_of::<SizedRun>(), 6);
    assert_eq!(
        (run.col(), run.cols(), run.row(), run.rows()),
        (8191, 49, 6, 7)
    );
    assert_eq!(run.size(), size);
    let small = SizedRun::new(
        0,
        1,
        0,
        TextSize {
            width: 1,
            ..TextSize::NORMAL
        },
    );
    assert_eq!(
        (
            small.cols(),
            small.row(),
            small.size().width,
            small.size().scale
        ),
        (1, 0, 1, 1)
    );
    assert_eq!(small.at(3).span(), 3..4);
}

/// [`Line::bytes_max`] less the run table at its most, for rows that
/// hold no sized run.
fn bytes_max_without_runs(cols: usize) -> usize {
    Line::bytes_max(cols)
        .saturating_sub(size_of::<Vec<SizedRun>>())
        .saturating_sub(cols.saturating_mul(size_of::<SizedRun>()))
}

#[test]
fn the_tables_grow_no_further_than_they_compact_at() {
    let cols = 17;
    let limit = table_limit(cols);
    let (mut most, mut styles, mut links, mut arena) = (0, 0, 0, 0);
    let mut n = 0u32;
    for first in 2..=128 {
        for small in [1, 2, 5, 11] {
            let mut line = Line::new(LineId::new(1), cols);
            for col in (0..6 * cols).map(|i| i % cols) {
                n += 1;
                let cluster = sized(if col == 0 { first } else { small });
                let glyph = match small {
                    1 if col > 0 => Glyph::Char('x'),
                    _ => Glyph::Cluster(&cluster),
                };
                line.write(col, glyph, Width::Narrow, &rgb(n), Some(LinkId::new(n)));
                let t = tables(&line);
                styles = styles.max(t.styles.capacity());
                links = links.max(t.links.capacity());
                arena = arena.max(t.clusters.capacity());
                most = most.max(size_of::<Line>() + line.heap_bytes());
            }
        }
    }
    assert_eq!(styles, limit);
    assert_eq!(links, limit);
    assert_eq!(arena, compaction_limit(cols * CLUSTER_BYTES_PER_CELL));
    assert!(most <= bytes_max_without_runs(cols), "{most}");
}

/// A kitty placeholder, U+10EEEE, and `marks` 4-byte marks.
fn placeholder(marks: usize) -> String {
    std::iter::once(PLACEHOLDER)
        .chain(std::iter::repeat_n('\u{E0100}', marks))
        .collect()
}

// ADR-1001, "Placeholder cells": in a build with images, a placeholder
// cluster keeps its first three marks wherever it starts in its line,
// after other clusters have spent the budget, and drops the marks past
// them; the budget still cuts every other cluster.
#[cfg(feature = "images")]
#[test]
fn a_placeholder_keeps_three_marks_whatever_the_budget() {
    let mut line = Line::new(LineId::new(1), 4);
    let heavy = accented(31);
    line.write(
        0,
        Glyph::Cluster(&heavy),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    assert_eq!(line.cluster_bytes(), 24, "the budget of 4 cells is spent");
    let five = placeholder(5);
    line.write(
        1,
        Glyph::Cluster(&five),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    line.write(
        2,
        Glyph::Cluster(&heavy),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    let one = placeholder(1);
    line.write(
        3,
        Glyph::Cluster(&one),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    assert_eq!(
        line.glyph(line.cell(1)),
        Glyph::Cluster(&placeholder(3)),
        "the placeholder keeps three of its five marks"
    );
    assert_eq!(
        line.glyph(line.cell(2)),
        Glyph::Char('e'),
        "the budget cuts"
    );
    assert_eq!(line.glyph(line.cell(3)), Glyph::Cluster(&one));
    assert_eq!(line.cluster_bytes(), 24 + 17 + 9);
    assert_eq!(tables(&line).live(), 24);
    assert_eq!(tables(&line).placeholders(), 2);
    line.erase(1..2, &Style::DEFAULT);
    assert_eq!(tables(&line).placeholders(), 1);
    assert_eq!(line.cluster_bytes(), 24 + 9);
}

// Holding a line to its budget rebuilds its arena and counts its
// placeholder cells again, from the cells it has now: a line cut short
// names fewer than it did.
#[cfg(feature = "images")]
#[test]
fn holding_a_line_to_its_budget_counts_its_placeholders_again() {
    let mut line = Line::new(LineId::new(1), 8);
    let heavy = accented(31);
    let p = placeholder(3);
    for col in 0..8 {
        let glyph = if col % 2 == 0 { &heavy } else { &p };
        line.write(
            col,
            Glyph::Cluster(glyph),
            Width::Narrow,
            &Style::DEFAULT,
            None,
        );
    }
    assert_eq!(tables(&line).placeholders(), 4);
    line.cells.truncate(3);
    line.hold_to_budget();
    assert_eq!(
        tables(&line).placeholders(),
        1,
        "one placeholder cell is left"
    );
    assert_eq!(line.cluster_bytes(), 17 + tables(&line).live());
}

// In a build without images, U+10EEEE is a private-use character like
// any other: a cluster that starts with it keeps only what the line's
// budget leaves, and the line counts no placeholder.
#[cfg(not(feature = "images"))]
#[test]
fn without_images_a_placeholder_cluster_keeps_only_what_the_budget_allows() {
    let mut line = Line::new(LineId::new(1), 4);
    let heavy = accented(31);
    line.write(
        0,
        Glyph::Cluster(&heavy),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    let five = placeholder(5);
    line.write(
        1,
        Glyph::Cluster(&five),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    assert_eq!(
        line.glyph(line.cell(1)),
        Glyph::Char(PLACEHOLDER),
        "the budget is spent"
    );
    // five cells' budget, 30 bytes, holds the 25 the cluster takes
    let mut line = Line::new(LineId::new(1), 5);
    line.write(
        0,
        Glyph::Cluster(&five),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    assert_eq!(
        line.glyph(line.cell(0)),
        Glyph::Cluster(&five),
        "all five marks fit"
    );
    assert_eq!(tables(&line).placeholders(), 0);
    assert_eq!(tables(&line).live(), five.len() + 1);
    // ADR-1009 D6's figure on a 64-bit target.
    #[cfg(target_pointer_width = "64")]
    assert_eq!(Line::bytes_max(170), 8_142);
}

// The arena's limit takes 17 bytes for each cell that names a
// placeholder, so a row of placeholders, rewritten in new styles and
// links, compacts within it and its tables reach `Line::bytes_max`.
#[cfg(feature = "images")]
#[test]
fn a_row_of_placeholders_holds_to_its_raised_limit() {
    let cols = 17;
    let (mut arena, mut most, mut n) = (0, 0, 0u32);
    for first in 1..=3 {
        let mut line = Line::new(LineId::new(1), cols);
        for col in (0..6 * cols).map(|i| i % cols) {
            n += 1;
            let c = placeholder(if col == 0 { first } else { 3 });
            line.write(
                col,
                Glyph::Cluster(&c),
                Width::Narrow,
                &rgb(n),
                Some(LinkId::new(n)),
            );
            let t = tables(&line);
            assert!(
                t.clusters.len()
                    <= compaction_limit(cols * (CLUSTER_BYTES_PER_CELL + PLACEHOLDER_BYTES_MAX))
            );
            arena = arena.max(t.clusters.capacity());
            most = most.max(size_of::<Line>() + line.heap_bytes());
        }
        assert_eq!(tables(&line).placeholders(), 17);
    }
    assert_eq!(
        arena,
        compaction_limit(cols * (CLUSTER_BYTES_PER_CELL + PLACEHOLDER_BYTES_MAX))
    );
    assert_eq!(most, bytes_max_without_runs(cols));
}

// A row a rewrap built names past its arena's limit. Compacting it
// sizes the arena to what its cells name, without doubling past it.
#[test]
fn compacting_a_row_past_its_limit_sizes_its_arena_to_what_it_names() {
    let cols = 10;
    let mut src = Line::new(LineId::new(1), 4 * cols);
    let heavy = accented(31);
    src.write(
        0,
        Glyph::Cluster(&heavy),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    let mut row = Line::with_capacity(LineId::new(2), LineFlags::empty(), cols);
    let mut map = IndexMap::default();
    for _ in 0..cols {
        row.push_from(&src, src.cell(0), Width::Narrow, false, &mut map);
    }
    row.shrink();
    let named = row.cluster_bytes();
    assert!(named > compaction_limit(cols * CLUSTER_BYTES_PER_CELL));
    // A write compacts such a row once its style or link table fills.
    row.compact();
    let t = tables(&row);
    assert_eq!(t.clusters.len(), named);
    assert_eq!(t.clusters.capacity(), named);
}

/// A cluster of the most bytes a line keeps: 32 codepoints of four
/// bytes each, 129 bytes with its length byte.
fn heaviest() -> String {
    std::iter::repeat_n('\u{E0100}', CLUSTER_MAX).collect()
}

// The widest row a rewrap can build has every cell name a cluster of
// the most bytes, as a rewrap copies each cluster whole. Its arena and
// its count of named bytes pass 16 bits by far, and the last cluster
// still starts at an offset a cell can name. Held to its screen's
// limit, twice its budget, the row still counts more than 16 bits.
#[test]
fn a_row_full_of_the_largest_clusters_at_the_widest_width_counts_them_all() {
    let heavy = heaviest();
    assert_eq!(heavy.len() + 1, CLUSTER_BYTES_MAX);
    let mut src = Line::new(LineId::new(1), COLUMNS_MAX);
    src.write(
        0,
        Glyph::Cluster(&heavy),
        Width::Narrow,
        &Style::DEFAULT,
        None,
    );
    let mut row = Line::with_capacity(LineId::new(2), LineFlags::empty(), COLUMNS_MAX);
    let mut map = IndexMap::default();
    for _ in 0..COLUMNS_MAX {
        row.push_from(&src, src.cell(0), Width::Narrow, false, &mut map);
    }
    let named = COLUMNS_MAX * CLUSTER_BYTES_MAX;
    assert_eq!(named, 1_056_768);
    assert_eq!(row.len(), COLUMNS_MAX);
    assert_eq!(tables(&row).live(), named);
    assert_eq!(tables(&row).clusters.len(), named);
    assert_eq!(row.cluster_bytes(), named);
    assert!(
        row.cells()
            .iter()
            .all(|&c| row.glyph(c) == Glyph::Cluster(&heavy))
    );
    let last = row.cell(COLUMNS_MAX - 1).content();
    assert_eq!(
        last,
        Content::Cluster(ClusterOffset(
            u32::try_from(named - CLUSTER_BYTES_MAX).unwrap()
        ))
    );

    row.paint(COLUMNS_MAX);
    // One and a half times the budget holds 571 whole clusters, and the
    // 69 bytes left keep the first 17 codepoints of the next.
    let limit = compaction_limit(COLUMNS_MAX * CLUSTER_BYTES_PER_CELL);
    let kept = limit / CLUSTER_BYTES_MAX;
    assert_eq!((limit, kept), (73_728, 571));
    assert_eq!(tables(&row).live(), limit);
    assert!(tables(&row).live() > usize::from(u16::MAX));
    assert_eq!(row.glyph(row.cell(kept - 1)), Glyph::Cluster(&heavy));
    let part: String = std::iter::repeat_n('\u{E0100}', 17).collect();
    assert_eq!(row.glyph(row.cell(kept)), Glyph::Cluster(&part));
    assert_eq!(row.glyph(row.cell(kept + 1)), Glyph::Char('\u{E0100}'));
}

// In a build with images, every cell of the widest line may name a
// placeholder cluster, and the line counts each of them.
#[cfg(feature = "images")]
#[test]
fn a_row_of_placeholders_at_the_widest_width_counts_every_cell() {
    let mut line = Line::new(LineId::new(1), COLUMNS_MAX);
    let p = placeholder(PLACEHOLDER_MARKS);
    for col in 0..COLUMNS_MAX {
        assert!(line.write(
            col,
            Glyph::Cluster(&p),
            Width::Narrow,
            &Style::DEFAULT,
            None
        ));
    }
    assert_eq!(tables(&line).placeholders(), COLUMNS_MAX);
    assert_eq!(tables(&line).live(), 0);
    assert_eq!(line.cluster_bytes(), COLUMNS_MAX * PLACEHOLDER_BYTES_MAX);
    assert!(
        line.cells()
            .iter()
            .all(|&c| line.glyph(c) == Glyph::Cluster(&p))
    );
}

// A cell names its cluster by a 21-bit offset. The arena takes
// clusters up to the last offset a cell can name, and refuses the
// next one, leaving itself as it was.
#[test]
fn the_arena_refuses_a_cluster_past_the_offsets_a_cell_names() {
    let s = "a".repeat(255);
    let mut arena = ClusterArena::default();
    let mut last = None;
    while let Some(offset) = arena.append(&s) {
        last = Some(offset);
    }
    let last = usize::try_from(last.unwrap()).unwrap();
    assert_eq!(last, ARENA_OFFSETS - 256);
    assert_eq!(arena.len(), ARENA_OFFSETS);
    assert_eq!(arena.append("ab"), None);
    assert_eq!(arena.len(), ARENA_OFFSETS);
    let offset = u32::try_from(last).unwrap();
    assert_eq!(cluster_at(arena.as_slice(), offset), Some(s.as_str()));
    // A cluster longer than its length byte can say is refused too.
    assert_eq!(ClusterArena::default().append(&"a".repeat(256)), None);
}

// The arena's length and its count of named bytes are kept in 24 bits.
// The most the arena reaches fits; a larger count is stored as the
// largest, and the count of placeholder cells stops at the largest
// 16-bit value, as neither may wrap to a small number.
#[test]
fn the_arena_counts_hold_their_largest_values() {
    for n in [0, 1, 0xFFFF, 0x1_0000, ARENA_OFFSETS + 255, U24::MAX] {
        assert_eq!(U24::saturating(n).get(), n);
    }
    assert_eq!(U24::saturating(U24::MAX + 1).get(), U24::MAX);
    assert_eq!(U24::saturating(usize::MAX).get(), U24::MAX);

    let mut t = Tables::default();
    t.clusters.set_live(U24::MAX);
    t.count("ab");
    assert_eq!(t.live(), U24::MAX);
    if cfg!(feature = "images") {
        t.clusters.placeholders = u16::MAX;
        t.count(&placeholder(PLACEHOLDER_MARKS));
        assert_eq!(t.placeholders(), usize::from(u16::MAX));
    }
}

// The arena reaches the capacities a vector of bytes reaches through
// the same calls: exact reserves, appends that grow by doubling,
// shrinking, and clones.
#[test]
fn the_arena_grows_and_shrinks_as_a_vector_of_bytes() {
    let mut arena = ClusterArena::default();
    let mut vec: Vec<u8> = Vec::new();
    let check = |arena: &ClusterArena, vec: &Vec<u8>| {
        assert_eq!(arena.as_slice(), vec.as_slice());
        assert_eq!(arena.capacity(), vec.capacity());
        let copy = arena.clone();
        assert_eq!(copy.capacity(), vec.clone().capacity());
        assert_eq!(&copy, arena);
    };
    for (i, n) in [1usize, 2, 7, 30, 3, 129, 0, 5, 60, 255, 2, 40]
        .into_iter()
        .enumerate()
    {
        let s = "x".repeat(n);
        match i % 4 {
            0 => {
                arena.reserve_exact(n);
                vec.reserve_exact(n);
            }
            1 => {
                arena.reserve_within(n.saturating_add(1), 64);
                reserve_within(&mut vec, n.saturating_add(1), 64);
            }
            2 => {
                arena.shrink_to_fit();
                vec.shrink_to_fit();
            }
            _ => {}
        }
        check(&arena, &vec);
        let offset = arena.append(&s).unwrap();
        assert_eq!(usize::try_from(offset).unwrap(), vec.len());
        vec.push(u8::try_from(n).unwrap());
        vec.extend_from_slice(s.as_bytes());
        check(&arena, &vec);
    }
}

// A style index that passed 16 bits would collapse to another entry or
// the default. Distinct styles across the widest line, through several
// compactions, each read back as written.
#[test]
fn distinct_styles_across_the_widest_line_survive_compaction() {
    let mut line = Line::new(LineId::new(1), COLUMNS_MAX);
    let mut n = 0u32;
    for _ in 0..3 {
        for col in 0..COLUMNS_MAX {
            n += 1;
            line.write(col, Glyph::Char('a'), Width::Narrow, &rgb(n), None);
        }
        let first = n - u32::try_from(COLUMNS_MAX).unwrap() + 1;
        for (col, m) in (0..COLUMNS_MAX).zip(first..) {
            assert_eq!(line.style(line.cell(col)), rgb(m), "column {col}");
        }
    }
}

#[test]
fn trim_frees_tables_no_cell_names() {
    let mut line = Line::new(LineId::new(1), 4);
    line.write(
        0,
        Glyph::Char('a'),
        Width::Narrow,
        &red(),
        Some(LinkId::new(1)),
    );
    line.write(0, Glyph::Char('a'), Width::Narrow, &Style::DEFAULT, None);
    line.trim();
    assert!(line.tables.is_none());
    assert_eq!(line.heap_bytes(), 8);
}

#[test]
fn resize_pads_with_blank_cells_and_clamps() {
    let mut line = Line::new(LineId::new(1), 2);
    line.write(1, Glyph::Char('b'), Width::Narrow, &red(), None);
    line.resize(5);
    assert_eq!(line.len(), 5);
    assert!(line.cell(4).is_blank());
    assert_eq!(line.glyph(line.cell(1)), Glyph::Char('b'));
    line.resize(usize::MAX);
    assert_eq!(line.len(), COLUMNS_MAX);
    assert_eq!(Line::new(LineId::new(2), usize::MAX).len(), COLUMNS_MAX);
}

// Shrinking keeps no more tables than the shorter line can name.
#[test]
fn shrinking_compacts_the_tables() {
    let mut line = Line::new(LineId::new(1), COLUMNS_MAX);
    for (col, n) in (0..COLUMNS_MAX).zip(0u32..) {
        let link = Some(LinkId::new(n));
        line.write(
            col,
            Glyph::Cluster(&accented(2)),
            Width::Narrow,
            &rgb(n),
            link,
        );
    }
    line.resize(1);
    let t = tables(&line);
    assert_eq!((t.styles.len(), t.links.len(), t.clusters.len()), (1, 1, 6));
    assert_eq!(line.heap_bytes(), 8 + size_of::<Tables>() + 14 + 2 + 6);
    assert_eq!(line.style(line.cell(0)), rgb(0));
    assert_eq!(line.glyph(line.cell(0)), Glyph::Cluster(&accented(2)));
}

#[test]
fn flags_and_size_are_independent() {
    let mut f = LineFlags::empty();
    f.insert(LineFlags::WRAPPED);
    f.insert(LineFlags::PROMPT);
    f.insert(LineFlags::FIXED);
    f.set_size(LineSize::DoubleHeightBottom);
    assert!(f.contains(LineFlags::FIXED));
    assert!(f.contains(LineFlags::WRAPPED));
    assert!(f.contains(LineFlags::PROMPT));
    assert!(!f.contains(LineFlags::OUTPUT));
    assert_eq!(f.size(), LineSize::DoubleHeightBottom);
    f.remove(LineFlags::WRAPPED);
    f.set_size(LineSize::Single);
    assert!(!f.contains(LineFlags::WRAPPED));
    assert!(f.contains(LineFlags::PROMPT));
    assert_eq!(f.size(), LineSize::Single);
}

// A double-size line holds half the screen's columns, rounded down,
// and at least one; a single-size line holds them all (VT100 User
// Guide, DECDWL: "The use of double-width characters reduces the
// number of characters per line by half").
#[test]
fn a_double_size_line_holds_half_the_columns() {
    for size in [
        LineSize::DoubleWidth,
        LineSize::DoubleHeightTop,
        LineSize::DoubleHeightBottom,
    ] {
        assert!(size.is_double());
        assert_eq!(size.columns(80), 40);
        assert_eq!(size.columns(81), 40);
        assert_eq!(size.columns(1), 1);
    }
    assert!(!LineSize::Single.is_double());
    assert_eq!(LineSize::Single.columns(81), 81);
}
