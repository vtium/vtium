use super::bounds::GIB;
use super::*;
use crate::bounds::{COLUMNS_MAX, LINKS_BYTES_MAX, ROWS_MAX, SB_MEMORY_MAX, SB_MEMORY_MIN};
use crate::cell::Width;
use crate::line::Glyph;
use crate::style::Style;

const MIB: u64 = 1 << 20;

/// A line of `len` cells of text, its id `id`.
fn text(id: u64, len: usize) -> Line {
    let mut line = Line::new(LineId::new(id), len);
    for x in 0..len {
        line.write(x, Glyph::Char('x'), Width::Narrow, &Style::DEFAULT, None);
    }
    line
}

fn ids(sb: &Scrollback) -> Vec<u64> {
    sb.lines().map(|l| l.id().get()).collect()
}

/// A row of `len` cells of text, its id `id`, marked wrapped.
#[cfg(feature = "images")]
fn wrapped(id: u64, len: usize) -> Line {
    let mut line = text(id, len);
    line.flags_mut().insert(LineFlags::WRAPPED);
    line
}

/// Every row's id, waiting and rewrapped, oldest first.
#[cfg(feature = "images")]
fn all_ids(sb: &Scrollback) -> Vec<u64> {
    sb.all_rows().map(|l| l.id().get()).collect()
}

#[cfg(feature = "images")]
fn dropped(line: u64, rows: usize, next: Option<u64>) -> Change {
    Change::Dropped {
        line: LineId::new(line),
        rows,
        next: next.map(LineId::new),
    }
}

/// A scrollback of 100 rows holding `lines`, then left to wait at
/// `width` with nothing taken, the anchors `anchors` given; its log is
/// taken first.
#[cfg(feature = "images")]
fn waiting_at(
    lines: Vec<Line>,
    width: usize,
    anchors: &[(u64, u32)],
    next: &mut u64,
) -> Scrollback {
    let mut sb = Scrollback::new(100);
    sb.set_max(100);
    for line in lines {
        sb.push(line);
    }
    let mut given: BTreeMap<LineId, Vec<u32>> = BTreeMap::new();
    for &(id, col) in anchors {
        given.entry(LineId::new(id)).or_default().push(col);
    }
    sb.set_anchors(given);
    let taken = sb.take_for_reflow(width, 0, None, next);
    assert!(taken.rows.is_empty());
    sb
}

// Rows already rewrapped drop one by one, each going on to the next;
// the newest dropped goes on to the oldest row left.
#[cfg(feature = "images")]
#[test]
fn rewrapped_rows_drop_one_by_one_onto_the_next() {
    let mut sb = Scrollback::new(100);
    sb.set_max(100);
    for id in 1..=5 {
        sb.push(text(id, 3));
    }
    let _ = sb.take_changes();
    sb.set_max(2);
    let want = [
        dropped(1, 1, Some(2)),
        dropped(2, 1, Some(3)),
        dropped(3, 1, Some(4)),
    ];
    assert_eq!(sb.take_changes(), want);
    sb.set_max(0);
    assert_eq!(
        sb.take_changes(),
        [dropped(4, 1, Some(5)), dropped(5, 1, None)]
    );
}

// A push into a full scrollback drops the oldest row, which goes on to
// the line pushed when nothing else is left.
#[cfg(feature = "images")]
#[test]
fn a_row_dropped_by_a_push_goes_on_to_the_line_pushed() {
    let mut sb = Scrollback::new(100);
    sb.set_max(1);
    sb.push(text(1, 3));
    sb.push(text(2, 3));
    assert_eq!(sb.take_changes(), [dropped(1, 1, Some(2))]);
}

// A waiting line dropped whole stands for the rows it takes at the
// width, 3 here, and each row of it goes on to the line after it.
#[cfg(feature = "images")]
#[test]
fn a_waiting_line_dropped_whole_stands_for_its_rows_at_the_width() {
    let mut next = 100;
    let lines = vec![wrapped(1, 6), text(2, 6), text(3, 2)];
    let mut sb = waiting_at(lines, 4, &[], &mut next);
    assert_eq!(sb.total(), 4);
    let _ = sb.take_changes();
    sb.set_max(1);
    assert_eq!(
        sb.take_changes(),
        [dropped(1, 3, Some(3)), dropped(2, 1, Some(3))]
    );
    assert_eq!(all_ids(&sb), [3]);
}

// A resize rewraps a waiting line an anchor is on at once, carrying the
// anchor, and later takes its rows as they are: their ids stay.
#[cfg(feature = "images")]
#[test]
fn a_waiting_anchored_line_rewraps_at_once_and_is_then_taken_as_it_is() {
    let mut next = 100;
    // one logical line of 8 cells, its anchor at the 6th
    let lines = vec![wrapped(1, 4), text(2, 4), text(3, 2)];
    let mut sb = waiting_at(lines, 3, &[(2, 1)], &mut next);
    assert_eq!(
        all_ids(&sb),
        [1, 100, 101, 3],
        "8 cells at 3: rows of 3, 3 and 2"
    );
    let carried = Move {
        from: LineId::new(2),
        col: 1,
        to: LineId::new(100),
        to_col: 2,
    };
    assert_eq!(sb.take_changes(), [Change::Carried(vec![carried])]);
    sb.rewrap_all();
    assert_eq!(ids(&sb), [1, 100, 101, 3], "taken as they are");
    assert_eq!(sb.bytes(), charge(sb.lines()));
}

// Two waiting lines with anchors, a line without one between them: the
// resize's search finds and rewraps both, and only then stops.
#[cfg(feature = "images")]
#[test]
fn a_resize_rewraps_every_anchored_waiting_line_before_its_search_stops() {
    let mut next = 100;
    // 8 cells held as 2 rows, its anchor at the 6th; 2 cells; 6 cells,
    // its anchor at the 3rd; 2 cells
    let lines = vec![
        wrapped(1, 4),
        text(2, 4),
        text(3, 2),
        text(4, 6),
        text(5, 2),
    ];
    let mut sb = waiting_at(lines, 3, &[(2, 1), (4, 2)], &mut next);
    assert_eq!(all_ids(&sb), [1, 100, 101, 3, 4, 102, 5]);
    let first = Move {
        from: LineId::new(2),
        col: 1,
        to: LineId::new(100),
        to_col: 2,
    };
    let second = Move {
        from: LineId::new(4),
        col: 2,
        to: LineId::new(4),
        to_col: 2,
    };
    let want = [Change::Carried(vec![first]), Change::Carried(vec![second])];
    assert_eq!(sb.take_changes(), want);
}

// A placement a drop moves onto a waiting line has that line rewrapped
// at the width then, and the byte count follows the rows it now holds.
#[cfg(feature = "images")]
#[test]
fn a_waiting_line_a_placement_moves_onto_rewraps_and_is_counted_exactly() {
    let mut next = 100;
    let lines = vec![text(1, 2), text(2, 12), text(3, 2)];
    let mut sb = waiting_at(lines, 4, &[], &mut next);
    let moved = sb.note_anchor(LineId::new(2), &[5], &mut next);
    assert_eq!(moved, [], "a placement on the line's first row stays there");
    // the resize reserved 100 and 101 for the line's rows to come, so
    // this rewrap takes new ids, and the reserved ones go unused
    assert_eq!(all_ids(&sb), [1, 2, 102, 103, 3]);
    assert_eq!(sb.bytes(), charge(sb.all_rows()), "the byte count is exact");
    assert_eq!(sb.total(), 5);
    // held rewrapped, a second placement there rewraps nothing
    assert_eq!(sb.note_anchor(LineId::new(2), &[0], &mut next), []);
    assert_eq!(all_ids(&sb), [1, 2, 102, 103, 3]);
}

// The operator's fence (vtium/vtium#131): from 1 MiB to the lesser of
// 1 GiB and a quarter of the machine's memory, and never below 1 MiB.
#[test]
fn sb_memory_is_fenced_by_its_bounds_and_a_quarter_of_the_memory() {
    assert_eq!(sb_memory_cap(512 * MIB), 128 << 20);
    assert_eq!(sb_memory_cap(4 << 30), 1 << 30);
    assert_eq!(sb_memory_cap(u64::MAX), SB_MEMORY_MAX);
    assert_eq!(sb_memory_cap(0), SB_MEMORY_MIN);
    assert_eq!(sb_memory_cap(4 * MIB - 1), SB_MEMORY_MIN);
    assert_eq!(sb_memory(0, 512 * MIB), SB_MEMORY_MIN);
    assert_eq!(sb_memory(SB_MEMORY_MIN - 1, 512 * MIB), SB_MEMORY_MIN);
    assert_eq!(sb_memory(SB_MEMORY_MIN, 512 * MIB), SB_MEMORY_MIN);
    assert_eq!(sb_memory(SB_MEMORY_DEFAULT, 512 * MIB), SB_MEMORY_DEFAULT);
    assert_eq!(sb_memory(128 << 20, 512 * MIB), 128 << 20);
    assert_eq!(sb_memory((128 << 20) + 1, 512 * MIB), 128 << 20);
    assert_eq!(sb_memory(SB_MEMORY_MAX, 8 << 30), SB_MEMORY_MAX);
    assert_eq!(sb_memory(SB_MEMORY_MAX + 1, 8 << 30), SB_MEMORY_MAX);
    assert_eq!(sb_memory(usize::MAX, 8 << 30), SB_MEMORY_MAX);
}

// What 16 MiB leaves after the rest of the process, the minspec grid's
// two screens at their worst and a full link store: 5,373,952 bytes
// (5.13 MiB) without images, and 4,915,200 bytes (4.69 MiB) with them,
// whose screen rows may hold placeholder cells; a smaller grid leaves
// more.
//
// References:
// - ADR-1009 D6: the figures.
#[test]
fn the_default_budget_is_what_the_minspec_peak_leaves() {
    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(SB_MEMORY_DEFAULT_MINSPEC, 5_373_952);
        assert_eq!(SB_MEMORY_DEFAULT_IMAGES, 4_915_200);
        #[cfg(feature = "images")]
        assert_eq!(SB_MEMORY_DEFAULT, 4_915_200);
        #[cfg(not(feature = "images"))]
        assert_eq!(SB_MEMORY_DEFAULT, 5_373_952);
    }
    let screens = screens_bytes_max(MINSPEC_GRID.0, MINSPEC_GRID.1);
    let rest = MINSPEC_PEAK - MINSPEC_REST - screens - LINKS_BYTES_MAX;
    assert!((SB_MEMORY_DEFAULT..SB_MEMORY_DEFAULT + (64 << 10)).contains(&rest));
    assert!(sb_memory_default(80, 30) > SB_MEMORY_DEFAULT);
    assert!(sb_memory_default(40, 30) > sb_memory_default(80, 30));
}

// Above minspec's grids the default stays at the minspec grid's value:
// a 3840x2160 display in 8x16 cells keeps that value, where the peak alone
// would leave it the fence's 1 MiB floor.
#[test]
fn a_grid_past_minspec_keeps_the_minspec_default() {
    assert_eq!(sb_memory_default(480, 135), SB_MEMORY_DEFAULT);
    assert_eq!(sb_memory_default(171, 48), SB_MEMORY_DEFAULT);
    assert_eq!(sb_memory_default(COLUMNS_MAX, ROWS_MAX), SB_MEMORY_DEFAULT);
}

#[test]
fn the_oldest_rows_go_past_the_byte_budget() {
    let row = cost(&{
        let mut l = text(0, 10);
        l.trim();
        l
    });
    let mut sb = Scrollback::new(100);
    sb.set_max(100);
    sb.set_bytes_max(5 * row);
    for id in 0..10 {
        assert!(sb.push(text(id, 10)).is_none());
        assert!(sb.bytes() <= 5 * row);
        assert_eq!(sb.bytes(), charge(sb.all_rows()));
    }
    assert_eq!(ids(&sb), [5, 6, 7, 8, 9]);
    // A lower budget drops the oldest now, and a view on them moves to
    // the oldest row left; the line cap still holds.
    sb.set_view(Some(1));
    sb.set_bytes_max(2 * row);
    assert_eq!(ids(&sb), [8, 9]);
    assert_eq!(sb.view_index(), Some(0));
    sb.set_bytes_max(5 * row);
    sb.set_max(1);
    assert_eq!(ids(&sb), [9]);
    assert_eq!(sb.bytes(), row);
    sb.clear();
    assert_eq!(sb.bytes(), 0);
}

#[test]
fn a_line_that_alone_passes_the_budget_is_given_back() {
    let mut sb = Scrollback::new(100);
    sb.set_max(100);
    assert!(sb.push(text(0, 4)).is_none());
    let wide = text(1, 400);
    sb.set_bytes_max(cost(&wide) - 1);
    assert!(sb.push(wide).is_some());
    assert_eq!(ids(&sb), [0]);
}

#[test]
fn taking_back_and_unwrapping_keep_the_count() {
    let mut sb = Scrollback::new(100);
    sb.set_max(100);
    for id in 0..4 {
        sb.push(text(id, 7));
    }
    sb.pop_newest();
    assert_eq!(sb.bytes(), charge(sb.all_rows()));
    sb.unwrap_newest();
    sb.rows_mut_while(|l| {
        l.resize(3);
        true
    });
    assert_eq!(sb.bytes(), charge(sb.all_rows()));
}

#[test]
fn the_default_cap_steps_at_2_and_4_gib() {
    assert_eq!(sb_size_cap(512 << 20, None), 10_000);
    assert_eq!(sb_size_cap(2 * GIB - 1, None), 10_000);
    assert_eq!(sb_size_cap(2 * GIB, None), 30_000);
    assert_eq!(sb_size_cap(4 * GIB - 1, None), 30_000);
    assert_eq!(sb_size_cap(4 * GIB, None), 100_000);
    assert_eq!(sb_size_cap(u64::MAX, None), 100_000);
}

#[test]
fn a_request_past_the_cap_is_clamped() {
    assert_eq!(sb_size(SB_SIZE_DEFAULT, 512 << 20, None), 1000);
    assert_eq!(sb_size(50_000, 512 << 20, None), 10_000);
    assert_eq!(sb_size(50_000, 3 * GIB, None), 30_000);
    assert_eq!(sb_size(usize::MAX, 8 * GIB, None), 100_000);
    assert_eq!(sb_size(0, 8 * GIB, None), 0);
}

// The operator, 2026-10-26: "10000 lines is fine but it should be
// configurable". A configured cap wins over the memory's default in
// both directions, and never passes SB_SIZE_MAX.
#[test]
fn a_configured_cap_wins_both_ways_inside_the_hard_bound() {
    assert_eq!(sb_size_cap(512 << 20, Some(50_000)), 50_000);
    assert_eq!(sb_size(50_000, 512 << 20, Some(50_000)), 50_000);
    assert_eq!(sb_size_cap(8 * GIB, Some(2_000)), 2_000);
    assert_eq!(sb_size(50_000, 8 * GIB, Some(2_000)), 2_000);
    assert_eq!(sb_size_cap(512 << 20, Some(0)), 0);
    assert_eq!(sb_size_cap(512 << 20, Some(usize::MAX)), SB_SIZE_MAX);
    assert_eq!(sb_size(usize::MAX, 8 * GIB, Some(usize::MAX)), SB_SIZE_MAX);
}
