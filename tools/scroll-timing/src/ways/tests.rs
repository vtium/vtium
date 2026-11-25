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

/// The grid's lines of the view from `top`, as a redraw draws them.
fn expected(g: Geometry, top: i64) -> Vec<u8> {
    let pattern = Pattern::new(g);
    let mut buf = buffer(g.height);
    redraw(&pattern, g, &mut buf, STRIDE, top);
    grid_lines(g, &buf, 0)
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

/// Runs `way` over `tops` with `buffers` display buffers taken in turn, as
/// drm2d lends them, checks every frame's picture against a redraw of its
/// view, and returns each frame's counts.
fn run(way: Way, g: Geometry, buffers: usize, tops: &[i64]) -> Vec<Counts> {
    let pattern = Pattern::new(g);
    let mut pan = Pan::new(g, 3, buffers);
    let lines = match way {
        Way::Pan => pan.ring.buffer_height(g),
        _ => g.height,
    };
    let mut bufs: Vec<Vec<u8>> = (0..buffers).map(|_| buffer(lines)).collect();
    let mut copy = Copy::new(g);
    let mut moves = Move::default();
    let mut counts = Vec::new();
    for (frame, &top) in tops.iter().enumerate() {
        let b = frame % buffers;
        // A buffer is `buffers` frames old once each has been drawn once.
        let age = if frame < buffers { 0 } else { buffers as u32 };
        let buf = &mut bufs[b];
        let (c, from) = match way {
            Way::Redraw => (redraw(&pattern, g, buf, STRIDE, top), 0),
            Way::Copy => (copy.frame(&pattern, g, buf, STRIDE, top), 0),
            Way::Move => (moves.frame(&pattern, g, buf, STRIDE, age, top), 0),
            Way::Pan => pan.frame(&pattern, g, b, buf, STRIDE, top),
        };
        assert!(
            grid_lines(g, buf, from) == expected(g, top),
            "{} with {buffers} buffers shows the wrong rows at frame {frame}, top {top}",
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

#[test]
fn every_way_shows_what_a_redraw_shows() {
    for g in [EVEN, STRIP] {
        for way in Way::ALL {
            for buffers in 1..=3 {
                for speed in [1, 2, 3, 5, 7] {
                    run(way, g, buffers, &tops(speed, 40));
                }
                // A scroll that turns and goes forward again.
                let turn: Vec<i64> = (0..40).map(|f: i64| 1000 - 12 + (f - 20).abs()).collect();
                run(way, g, buffers, &turn);
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
        let mut pan = Pan::new(EVEN, spare, 1);
        let ring = pan.ring;
        let mut buf = buffer(ring.buffer_height(EVEN));
        let mut starts = Vec::new();
        for (frame, top) in tops(1, 3 * ring.rows as usize).into_iter().enumerate() {
            let (c, src_y) = pan.frame(&pattern, EVEN, 0, &mut buf, STRIDE, top);
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
    let ring = Ring::new(hd, 0);
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
    assert_eq!(Ring::new(hd, 67).places(100), (100, None));
}

#[test]
fn the_bytes_each_way_writes_and_reads() {
    let row = 960;
    let screen = 5 * row;
    let counts = |way, g, buffers| run(way, g, buffers, &tops(1, 6));
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
    let ring = Ring::new(EVEN, 3);
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
