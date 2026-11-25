use super::*;
use crate::pattern::BORDER;

/// A screen of 5 rows of 4×6 cells with no strip under the grid: 160
/// bytes a line, 960 a row.
const EVEN: Geometry = Geometry {
    width: 40,
    height: 30,
    cell_width: 4,
    cell_height: 6,
};

/// The same cells on a screen 2 lines taller, so a strip of 2 lines lies
/// under the grid.
const STRIP: Geometry = Geometry {
    width: 40,
    height: 32,
    cell_width: 4,
    cell_height: 6,
};

/// A stride wider than a line, as a dumb buffer's pitch can be.
const STRIDE: usize = 192;

fn buffer(lines: u32) -> Vec<u8> {
    vec![0xee; lines as usize * STRIDE]
}

/// The grid's lines of the view from pixel line `pos`, built a line at a
/// time from whole rows that the pattern draws, so that the check does not
/// lean on how a redraw splits rows at a pixel offset.
fn expected(g: Geometry, pos: i64) -> Vec<u8> {
    let pattern = Pattern::new(g);
    let mut row = vec![0u8; g.cell_height as usize * g.line_bytes()];
    let ch = i64::from(g.cell_height);
    (0..i64::from(g.rows() * g.cell_height))
        .flat_map(|y| {
            let line = pos + y;
            pattern.draw_row(&mut row, g.line_bytes(), line.div_euclid(ch));
            let at = line.rem_euclid(ch) as usize * g.line_bytes();
            row[at..at + g.line_bytes()].to_vec()
        })
        .collect()
}

/// The grid's lines in `buf` from pixel line `from`, without the padding
/// past each line's end.
fn grid_lines(g: Geometry, buf: &[u8], from: u32) -> Vec<u8> {
    (0..g.rows() * g.cell_height)
        .flat_map(|y| {
            let at = (from + y) as usize * STRIDE;
            buf[at..at + g.line_bytes()].to_vec()
        })
        .collect()
}

/// The strip's lines in `buf` from pixel line `from` all show the border.
fn strip_is_border(g: Geometry, buf: &[u8], from: u32) -> bool {
    (g.rows() * g.cell_height..g.height).all(|y| {
        let at = (from + y) as usize * STRIDE;
        buf[at..at + g.line_bytes()]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|px| *px == BORDER.to_le_bytes())
    })
}

/// The tops of `frames` frames of a scroll back at `speed` rows a frame,
/// from row 1,000.
fn tops(speed: i64, frames: usize) -> Vec<i64> {
    (0..frames as i64).map(|f| 1000 - f * speed).collect()
}

/// The same scroll as pixel lines: the top line of each frame's view.
fn rows_as_lines(g: Geometry, tops: &[i64]) -> Vec<i64> {
    tops.iter().map(|t| t * i64::from(g.cell_height)).collect()
}

/// The view's top lines over `frames` frames of a scroll back at `step`
/// pixel lines a frame, from row 1,000's top line.
fn steps(g: Geometry, step: i64, frames: usize) -> Vec<i64> {
    let from = 1000 * i64::from(g.cell_height);
    (0..frames as i64).map(|f| from - f * step).collect()
}

/// Runs `way` over the views from the pixel lines `places`, with `buffers`
/// display buffers taken in turn, as drm2d lends them, and the pan's ring
/// made for a scroll by `unit` with 3 spare rows. Checks every frame's
/// picture against the expected view, and returns each frame's counts.
fn run(way: Way, g: Geometry, buffers: usize, unit: Unit, places: &[i64]) -> Vec<Counts> {
    let pattern = Pattern::new(g);
    let mut pan = Pan::new(g, 3, unit, buffers);
    let lines = match way {
        Way::Pan => pan.ring.buffer_height(g),
        _ => g.height,
    };
    let mut bufs: Vec<Vec<u8>> = (0..buffers).map(|_| buffer(lines)).collect();
    let mut copy = Copy::new(g);
    let mut moves = Move::default();
    let mut counts = Vec::new();
    for (frame, &pos) in places.iter().enumerate() {
        let b = frame % buffers;
        // A buffer is `buffers` frames old once each has been drawn once.
        let age = if frame < buffers { 0 } else { buffers as u32 };
        let buf = &mut bufs[b];
        let (c, from) = match way {
            Way::Redraw => (redraw(&pattern, g, buf, STRIDE, pos), 0),
            Way::Copy => (copy.frame(&pattern, g, buf, STRIDE, pos), 0),
            Way::Move => {
                let ch = i64::from(g.cell_height);
                assert_eq!(pos % ch, 0, "the move scrolls by whole rows");
                (moves.frame(&pattern, g, buf, STRIDE, age, pos / ch), 0)
            }
            Way::Pan => pan.frame(&pattern, g, b, buf, STRIDE, pos),
        };
        assert!(
            grid_lines(g, buf, from) == expected(g, pos),
            "{} with {buffers} buffers shows the wrong lines at frame {frame}, line {pos}",
            way.name()
        );
        if way == Way::Pan {
            assert!(
                strip_is_border(g, buf, from),
                "the pan's strip at frame {frame}"
            );
        }
        counts.push(c);
    }
    counts
}

/// `run` over a scroll by whole rows, from the tops of its frames.
fn run_rows(way: Way, g: Geometry, buffers: usize, tops: &[i64]) -> Vec<Counts> {
    run(way, g, buffers, Unit::Rows, &rows_as_lines(g, tops))
}

#[test]
fn every_way_shows_what_a_redraw_shows() {
    for g in [EVEN, STRIP] {
        for way in Way::ALL {
            for buffers in 1..=3 {
                for speed in [1, 2, 3, 5, 7] {
                    run_rows(way, g, buffers, &tops(speed, 40));
                }
                // A scroll that turns and goes forward again.
                let turn: Vec<i64> = (0..40).map(|f: i64| 1000 - 12 + (f - 20).abs()).collect();
                run_rows(way, g, buffers, &turn);
            }
        }
    }
}

/// Every window of the ring, at every slot it can start at and across the
/// ring's end, shows the view's rows in order, and each frame of a scroll
/// back draws only the row that came in, at its slot and at its twin's.
#[test]
fn the_rings_twins_give_a_contiguous_window_at_every_offset() {
    let pattern = Pattern::new(EVEN);
    for spare in [0, 3] {
        let mut pan = Pan::new(EVEN, spare, Unit::Rows, 1);
        let ring = pan.ring;
        let mut buf = buffer(ring.buffer_height(EVEN));
        let mut starts = Vec::new();
        for (frame, top) in tops(1, 3 * ring.rows as usize).into_iter().enumerate() {
            let pos = top * i64::from(EVEN.cell_height);
            let (c, src_y) = pan.frame(&pattern, EVEN, 0, &mut buf, STRIDE, pos);
            let start = src_y / EVEN.cell_height;
            starts.push(start);
            for i in 0..EVEN.rows() {
                assert_eq!(
                    pan.held(0)[(start + i) as usize],
                    Some(top + i64::from(i)),
                    "spare {spare}, top {top}: slot {} of the window",
                    start + i
                );
            }
            if frame > 0 {
                let (_, twin) = ring.places(top);
                let copies = 1 + u32::from(twin.is_some());
                assert_eq!(c.rows_drawn, copies, "spare {spare}, top {top}: rows drawn");
            }
            // Each slot in the ring's first screen-height holds the same row
            // as its twin, once both are drawn.
            for slot in 0..ring.twins {
                let (a, b) = (
                    pan.held(0)[slot as usize],
                    pan.held(0)[(slot + ring.rows) as usize],
                );
                if a.is_some() && b.is_some() {
                    assert_eq!(a, b, "spare {spare}: slot {slot} and its twin");
                }
            }
        }
        starts.sort_unstable();
        starts.dedup();
        assert_eq!(
            starts,
            (0..ring.rows).collect::<Vec<_>>(),
            "spare {spare}: every start"
        );
    }
}

#[test]
fn the_ring_and_the_tall_buffer_have_the_sizes_the_record_gives() {
    // 1920 × 1080 in an 8×16 cell: 67 rows, and 68 twins to cover 1,080
    // lines. With no spare rows the buffer is 2,160 lines, twice the
    // screen's height.
    let hd = Geometry {
        width: 1920,
        height: 1080,
        cell_width: 8,
        cell_height: 16,
    };
    let ring = Ring::new(hd, 0, Unit::Rows);
    assert_eq!(
        ring,
        Ring {
            rows: 67,
            twins: 68
        }
    );
    assert_eq!(ring.buffer_height(hd), 2160);
    assert_eq!(ring.places(-1), (66, Some(133)));
    assert_eq!(ring.places(67), (0, Some(67)));
    assert_eq!(Ring::new(hd, 67, Unit::Rows).places(100), (100, None));
    // At pixel steps the ring holds a 68th row, the row the view shows in
    // part, and the buffer grows by its 16 lines.
    let sliding = Ring::new(hd, 0, Unit::Pixels);
    assert_eq!(
        sliding,
        Ring {
            rows: 68,
            twins: 68
        }
    );
    assert_eq!(sliding.buffer_height(hd), 2176);
}

#[test]
fn the_bytes_each_way_writes_and_reads() {
    let row = 960;
    let screen = 5 * row;
    let counts = |way, g, buffers| run_rows(way, g, buffers, &tops(1, 6));
    let c = |written, read, system_written, rows_drawn| Counts {
        written,
        read,
        system_written,
        rows_drawn,
    };

    // The redraw draws the grid's 5 rows at every frame.
    assert!(
        counts(Way::Redraw, EVEN, 1)
            .iter()
            .all(|&k| k == c(screen, 0, 0, 5))
    );

    // The copy draws the ring whole once, then a row a frame into the
    // ring, and copies the grid's 5 rows into the display at every frame.
    let copy = counts(Way::Copy, EVEN, 2);
    assert_eq!(copy[0], c(screen, 0, screen, 5));
    assert!(copy[1..].iter().all(|&k| k == c(screen, 0, row, 1)));

    // The move with one buffer moves 4 rows down, reading them back, and
    // draws the row that came in; with two buffers, each buffer is two
    // frames old, so it moves 3 rows and draws 2.
    let one = counts(Way::Move, EVEN, 1);
    assert_eq!(one[0], c(screen, 0, 0, 5));
    assert!(one[1..].iter().all(|&k| k == c(screen, 4 * row, 0, 1)));
    let two = counts(Way::Move, EVEN, 2);
    assert!(two[2..].iter().all(|&k| k == c(screen, 3 * row, 0, 2)));

    // The pan with 3 spare rows: a ring of 8 rows and 5 twins. Its first
    // frame draws the view's 5 rows, each where it falls: the rows of
    // slots 0 to 4 twice, the others once. Then each frame draws the row
    // that came in, twice where its slot has a twin.
    let pan = counts(Way::Pan, EVEN, 1);
    let ring = Ring::new(EVEN, 3, Unit::Rows);
    let twice = |top: i64| u64::from(ring.places(top).1.is_some()) + 1;
    let first: u64 = (1000..1005).map(twice).sum();
    assert_eq!(pan[0], c(first * row, 0, 0, first as u32));
    for (f, top) in tops(1, 6).into_iter().enumerate().skip(1) {
        let n = twice(top);
        assert_eq!(pan[f], c(n * row, 0, 0, n as u32), "pan frame {f}");
    }

    // Under a strip of 2 lines, each frame of the pan also draws the strip.
    let strip = 2 * 160;
    let pan = counts(Way::Pan, STRIP, 1);
    for (f, top) in tops(1, 6).into_iter().enumerate().skip(1) {
        assert!(
            pan[f].written >= twice(top) * row + strip,
            "pan frame {f} under a strip"
        );
        assert_eq!(
            pan[f].written % row,
            strip % row,
            "pan frame {f} writes the strip once"
        );
    }
}

/// The redraw, the copy and the pan show what a redraw would show at pixel
/// steps: a step of one line passes every offset inside a row, steps of 5
/// and 7 lines in 6-line cells cross a row's edge at most frames, 13 lines
/// cross two, and each scroll runs past the pan's ring end and the copy's
/// ring end more than once.
#[test]
fn every_way_shows_what_a_redraw_shows_at_pixel_steps() {
    for g in [EVEN, STRIP] {
        for way in [Way::Redraw, Way::Copy, Way::Pan] {
            for buffers in 1..=3 {
                for step in [1, 2, 4, 5, 7, 13] {
                    run(way, g, buffers, Unit::Pixels, &steps(g, step, 130));
                }
                // A scroll that turns, inside a row, and goes forward again.
                let turn: Vec<i64> = (0..60)
                    .map(|f: i64| 6000 - 100 + 3 * (f - 30).abs())
                    .collect();
                run(way, g, buffers, Unit::Pixels, &turn);
            }
        }
    }
}

/// The pan's window at every pixel offset inside a row and at every slot
/// of the ring, across its end, shows the view's rows in order, and a
/// frame draws a row only when a row comes into view.
#[test]
fn the_pans_window_at_every_pixel_offset_and_across_the_rings_end() {
    let ch = EVEN.cell_height;
    for g in [EVEN, STRIP] {
        let pattern = Pattern::new(g);
        for spare in [0, 3] {
            let mut pan = Pan::new(g, spare, Unit::Pixels, 1);
            let ring = pan.ring;
            let mut buf = buffer(ring.buffer_height(g));
            let mut seen = Vec::new();
            let mut last_top = None;
            let places = steps(g, 1, (3 * ring.rows * ch) as usize);
            for (frame, &pos) in places.iter().enumerate() {
                let before = pan.held(0).to_vec();
                let (c, src_y) = pan.frame(&pattern, g, 0, &mut buf, STRIDE, pos);
                let (top, offset) = (pos.div_euclid(ch.into()), pos.rem_euclid(ch.into()) as u32);
                let (start, off) = (src_y / ch, src_y % ch);
                assert_eq!(off, offset, "line {pos}: the window's offset");
                assert_eq!(start, ring.slot(top), "line {pos}: the window's slot");
                seen.push((start, off));
                let shown = g.rows() + u32::from(offset > 0);
                let in_view = top..top + i64::from(shown);
                for i in 0..shown {
                    let held = pan.held(0)[(start + i) as usize];
                    // The strip may cover the last row's slot below what the
                    // window shows of it; every other slot holds its row.
                    if i < g.rows() || g.strip() == 0 {
                        assert_eq!(
                            held,
                            Some(top + i64::from(i)),
                            "line {pos}: slot {}",
                            start + i
                        );
                    }
                }
                assert!(
                    grid_lines(g, &buf, src_y) == expected(g, pos),
                    "line {pos}: the picture"
                );
                // Every row the frame drew is a row the view shows.
                let drawn: Vec<i64> = before
                    .iter()
                    .zip(pan.held(0))
                    .filter_map(|(was, now)| now.filter(|_| was != now))
                    .collect();
                assert!(
                    drawn.iter().all(|r| in_view.contains(r)),
                    "{g:?} spare {spare}, line {pos}: drew {drawn:?} outside the view"
                );
                if frame > 0 && g.strip() == 0 {
                    // Scrolling back a line at a time, a row comes in only
                    // when the view crosses into a new top row, and the
                    // frame then draws it at its slot and its twin's.
                    let copies = 1 + u32::from(ring.places(top).1.is_some());
                    let want = if last_top != Some(top) { copies } else { 0 };
                    assert_eq!(c.rows_drawn, want, "spare {spare}, line {pos}: rows drawn");
                }
                last_top = Some(top);
            }
            seen.sort_unstable();
            seen.dedup();
            let every: Vec<(u32, u32)> = (0..ring.rows)
                .flat_map(|s| (0..ch).map(move |o| (s, o)))
                .collect();
            assert_eq!(seen, every, "spare {spare}: every slot at every offset");
        }
    }
}

#[test]
fn the_bytes_each_way_writes_at_pixel_steps() {
    // EVEN: lines of 160 bytes, rows of 6 lines, a grid of 30 lines.
    let line = 160;
    let grid = 30 * line;
    let c = |written, system_written, rows_drawn| Counts {
        written,
        read: 0,
        system_written,
        rows_drawn,
    };
    // A step of 4 lines from row 1,000's top: the view stands at offsets
    // 0, 2, 4, 0, 2, 4, …, so it crosses a row's edge at every other frame.
    let places = steps(EVEN, 4, 7);

    // The redraw writes the grid's 30 lines at every frame, from 6 rows,
    // two of them in part, when the view stands inside a row.
    let redraw = run(Way::Redraw, EVEN, 1, Unit::Pixels, &places);
    for (f, &pos) in places.iter().enumerate() {
        let rows = if pos % 6 == 0 { 5 } else { 6 };
        assert_eq!(redraw[f], c(grid, 0, rows), "redraw frame {f}");
    }

    // The copy draws the 4 lines that came in, into its ring, and copies
    // the grid's 30 lines into the display at every frame. The 4 lines lie
    // in one row when the view stays inside the top row, and in two rows
    // when it crosses an edge: from offset 2 to offset 4 they are lines 4
    // and 5 of the new top row and lines 0 and 1 of the row below it.
    let copy = run(Way::Copy, EVEN, 2, Unit::Pixels, &places);
    assert_eq!(copy[0], c(grid, grid, 5));
    let rows = [1, 2, 1, 1, 2, 1];
    for (f, &n) in rows.iter().enumerate() {
        assert_eq!(copy[f + 1], c(grid, 4 * line, n), "copy frame {}", f + 1);
    }

    // The pan draws a row, at its slot and its twin's, only when the view
    // crosses into a new top row: here at frames 1, 3 and 5 (offsets 2
    // after 0). Its ring for pixel steps holds 5 + 1 + 3 rows and 5 twins.
    let pan = run(Way::Pan, EVEN, 1, Unit::Pixels, &places);
    let ring = Ring::new(EVEN, 3, Unit::Pixels);
    assert_eq!(ring, Ring { rows: 9, twins: 5 });
    let twice = |top: i64| u64::from(ring.places(top).1.is_some()) + 1;
    for (f, &pos) in places.iter().enumerate().skip(1) {
        let top = pos.div_euclid(6);
        let new = top != places[f - 1].div_euclid(6);
        let n = if new { twice(top) } else { 0 };
        assert_eq!(pan[f], c(n * 960, 0, n as u32), "pan frame {f}");
    }
}
