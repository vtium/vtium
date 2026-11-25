//! The rows beyond the screen: the line ids that move with the rows in
//! every build, and, in a build with the `rows-beyond` feature, the
//! frame's capture of the rows above and below the screen with what each
//! row carries.
//!
//! References:
//! - ADR-1310 D2: the capture, its counts on each side, what a row beyond
//!   the screen carries, and the line id as each row's id.
//! - ADR-1310, "Verification": the ids move with the rows, and the capture
//!   reports what exists.

#![expect(clippy::unwrap_used, reason = "a test fails by panicking")]

use crate::Terminal;
use crate::line::LineId;

/// The ids of the scrollback's rows, oldest first, then the screen's.
fn ids(t: &Terminal) -> Vec<u64> {
    let screen = t.screen();
    screen
        .scrollback()
        .chain(screen.lines())
        .map(|l| l.id().get())
        .collect()
}

/// The ids of the screen's rows, top to bottom.
fn screen_ids(t: &Terminal) -> Vec<u64> {
    t.screen().lines().iter().map(|l| l.id().get()).collect()
}

/// A terminal of 4 columns by 3 rows that keeps `max` scrollback lines,
/// its rows reading a, b and c, the cursor on the bottom row.
fn abc(max: usize) -> Terminal {
    let mut t = Terminal::new(4, 3, 1000);
    t.screen_mut().set_scrollback_max(max);
    t.input(b"a\r\nb\r\nc");
    t
}

// A line feed on the bottom row moves every row up by one with its id, and
// the row that comes in at the bottom takes an id no row had.
#[test]
fn a_line_feed_keeps_each_rows_id_one_row_higher_and_gives_the_new_row_a_new_id() {
    let mut t = abc(10);
    let before = screen_ids(&t);
    t.input(b"\n");
    let after = screen_ids(&t);
    assert_eq!(after.get(..2), before.get(1..), "the rows that stay");
    let new = *after.last().unwrap();
    assert!(!before.contains(&new), "the new row's id {new} is new");
    let left: Vec<u64> = t.screen().scrollback().map(|l| l.id().get()).collect();
    assert_eq!(
        left,
        before.get(..1).unwrap(),
        "the top row went to the scrollback with its id"
    );
}

// A line that leaves the scrollback at its limit ends its id: no row holds
// it afterwards, through any number of scrolls.
#[test]
fn a_line_leaving_the_scrollback_ends_its_id() {
    let mut t = abc(2);
    let first = *screen_ids(&t).first().unwrap();
    t.input(b"\n\n");
    assert_eq!(ids(&t).first(), Some(&first), "still in the scrollback");
    t.input(b"\n");
    assert!(!ids(&t).contains(&first), "gone with its line");
    for _ in 0..100 {
        t.input(b"\n");
        assert!(!ids(&t).contains(&first), "never given again");
    }
}

// Over a million line feeds no id is given twice: every id in the
// scrollback and on the screen rises in reading order, each batch of line
// feeds brings exactly one new id for each of them, and every new id is
// above every id seen before.
#[test]
fn no_id_repeats_over_a_million_scrolls() {
    const BATCH: usize = 50;
    let mut t = abc(100);
    let mut seen_max = ids(&t).into_iter().max().unwrap();
    let mut before = ids(&t);
    let feeds = vec![b'\n'; BATCH];
    for batch in 0..1_000_000 / BATCH {
        t.input(&feeds);
        let now = ids(&t);
        assert!(now.is_sorted_by(|a, b| a < b), "batch {batch}: ids rise");
        let new: Vec<u64> = now.iter().copied().filter(|&id| id > seen_max).collect();
        assert_eq!(
            new.len(),
            BATCH,
            "batch {batch}: one new id for each line feed"
        );
        assert!(
            now.iter()
                .filter(|&&id| id <= seen_max)
                .all(|id| before.contains(id)),
            "batch {batch}: an id seen before belongs to a line still here"
        );
        seen_max = *now.last().unwrap();
        before = now;
    }
}

// The ids are vtium-term's line ids, which every build has: the frame's
// line at each row carries the id of the screen's line there.
#[test]
fn the_frame_carries_each_rows_line_id() {
    let t = abc(10);
    let palette = crate::Palette::default();
    let frame = t.screen().frame(&palette, crate::Age::NEVER);
    let shown: Vec<LineId> = (0..frame.rows())
        .filter_map(|r| frame.line(r))
        .map(|l| l.id())
        .collect();
    let lines: Vec<LineId> = t.screen().lines().iter().map(|l| l.id()).collect();
    assert_eq!(shown, lines);
}

// A build without the feature keeps the frame as it was: the rows beyond
// the screen add no field to it. The sizes are those of a 64-bit build
// before the feature existed, for each combination of images and bidi.
#[cfg(all(not(feature = "rows-beyond"), target_pointer_width = "64"))]
#[test]
fn a_build_without_the_feature_keeps_the_frames_size() {
    let expected = match (cfg!(feature = "images"), cfg!(feature = "bidi")) {
        (false, false) => 168,
        (true, false) => 192,
        (false, true) => 240,
        (true, true) => 264,
    };
    assert_eq!(size_of::<crate::Frame<'static>>(), expected);
}

#[cfg(feature = "rows-beyond")]
mod capture {
    use super::*;
    use crate::screen::Reach;
    use crate::{Age, Frame, Glyph, LineSize, Palette};

    /// The overscan's rows on each side.
    const K: usize = 4;

    /// A reach that asks for K rows above and K below, and no spare rows.
    const OVERSCAN: Reach = Reach {
        above: K,
        below: K,
        spare: 0,
        moved_since: Age::NEVER,
    };

    /// A terminal of 4 columns by 3 rows with a scrollback of 100 lines,
    /// after `lines` numbered lines: line 0 first, the cursor at the end of
    /// the last.
    fn numbered(lines: usize) -> Terminal {
        let mut t = Terminal::new(4, 3, 1000);
        t.screen_mut().set_scrollback_max(100);
        for i in 0..lines {
            if i > 0 {
                t.input(b"\r\n");
            }
            t.input(format!("{i}").as_bytes());
        }
        t
    }

    /// The text of each captured row from the top, as its first cell.
    fn texts(frame: &Frame<'_>) -> Vec<String> {
        frame
            .captured()
            .map(|r| {
                frame
                    .cells_at(r)
                    .take_while(|c| c.glyph != Glyph::Empty)
                    .map(|c| match c.glyph {
                        Glyph::Char(ch) => ch.to_string(),
                        Glyph::Cluster(s) => s.to_owned(),
                        Glyph::Empty => String::new(),
                    })
                    .collect()
            })
            .collect()
    }

    // Verification, "The capture reports what exists": a screen with no
    // scrollback captures no row above it.
    #[test]
    fn a_screen_with_no_scrollback_captures_no_row_above() {
        let t = numbered(3);
        let palette = Palette::default();
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        assert_eq!((frame.rows_above(), frame.rows_below()), (0, 0));
        assert_eq!(frame.captured(), 0..3);
        assert!(frame.line_at(-1).is_none());
    }

    // The alternate screen has no scrollback, so it captures no row above
    // it even while the main screen's scrollback holds lines.
    #[test]
    fn the_alternate_screen_captures_no_row_above() {
        let mut t = numbered(20);
        t.input(b"\x1b[?1049h");
        let palette = Palette::default();
        let reach = Reach {
            spare: 3,
            ..OVERSCAN
        };
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, reach);
        assert_eq!((frame.rows_above(), frame.rows_below()), (0, 0));
    }

    // Verification: a view at the newest output captures no row below it,
    // also when the main screen keeps lines below its last row after it
    // shrank.
    #[test]
    fn a_view_at_the_newest_output_captures_no_row_below() {
        let mut t = numbered(20);
        let palette = Palette::default();
        let reach = Reach {
            spare: 3,
            ..OVERSCAN
        };
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, reach);
        assert_eq!(frame.rows_below(), 0);
        assert_eq!(t.screen().rows_reached(reach).1, 0);
        // the cursor's line rises to the top row, and the two lines below
        // it stay below the shrunken screen
        t.input(b"\x1b[1;1H");
        t.screen_mut().resize(4, 1);
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, reach);
        assert_eq!(frame.rows_below(), 0);
        assert_eq!(t.screen().rows_reached(reach).1, 0);
    }

    // Verification: a view three rows into the scrollback captures the
    // three rows below it, which is all there is, and K rows above it and
    // one more for the ink of the rows next to them.
    #[test]
    fn a_view_three_rows_back_captures_three_rows_below_and_k_above() {
        let mut t = numbered(20);
        t.screen_mut().scroll_view_up(3);
        let palette = Palette::default();
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        assert_eq!((frame.rows_above(), frame.rows_below()), (K + 1, 3));
        assert_eq!(frame.captured(), -5..6);
        // lines 0 to 19: the screen shows 17 to 19, the view 14 to 16
        let want: Vec<String> = (9..20).map(|i| i.to_string()).collect();
        assert_eq!(texts(&frame), want);
        // each captured row is the line of the scrollback or the screen
        // there, with its id
        let all: Vec<LineId> = t
            .screen()
            .scrollback()
            .chain(t.screen().lines())
            .map(|l| l.id())
            .collect();
        let captured: Vec<LineId> = frame
            .captured()
            .map(|r| frame.line_at(r).unwrap().id())
            .collect();
        assert_eq!(captured, all.get(9..20).unwrap());
    }

    // A side asks for at most the rows that exist: two lines of scrollback
    // give two rows above, however many the reach asks for.
    #[test]
    fn a_side_holds_only_the_rows_that_exist() {
        let t = numbered(5);
        let palette = Palette::default();
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        assert_eq!((frame.rows_above(), frame.rows_below()), (2, 0));
        assert_eq!(texts(&frame), ["0", "1", "2", "3", "4"]);
    }

    // ADR-1310 D2, "What the capture holds under scroll-pan": the spare
    // rows drawn ahead and behind are captured only while the view is
    // scrolled back or the scroll record counted a move since the age the
    // reach names. At rest the capture holds the K rows above and one more.
    #[test]
    fn spare_rows_are_captured_only_while_a_scroll_can_show_them() {
        let mut t = numbered(30);
        let palette = Palette::default();
        let rest = t.screen_mut().checkpoint();
        let reach = Reach {
            above: K,
            below: 0,
            spare: 9,
            moved_since: rest,
        };
        assert_eq!(t.screen().rows_reached(reach), (K + 1, 0), "at rest");
        // a change in place counts no move
        t.input(b"x");
        assert_eq!(
            t.screen().rows_reached(reach),
            (K + 1, 0),
            "a change in place"
        );
        // a line feed on the bottom row is a counted move
        t.input(b"\n");
        assert_eq!(t.screen().rows_reached(reach), (10, 0), "after a line feed");
        // a display that drew after the move sees no move since
        let drawn = t.screen_mut().checkpoint();
        let reach = Reach {
            moved_since: drawn,
            ..reach
        };
        assert_eq!(t.screen().rows_reached(reach), (K + 1, 0), "drawn since");
        // while scrolled back the spare rows are held on both sides, below
        // only as far as the rows that exist
        t.screen_mut().scroll_view_up(4);
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, reach);
        assert_eq!((frame.rows_above(), frame.rows_below()), (10, 4));
    }

    // The cursor is reported only on the screen's rows: a row below the
    // view that holds the cursor's line carries no cursor.
    #[test]
    fn the_cursor_stays_on_the_screens_rows() {
        let mut t = numbered(20);
        t.screen_mut().scroll_view_up(3);
        let palette = Palette::default();
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        assert!(frame.rows_below() > 0);
        assert!(
            !frame.cursor().visible,
            "the cursor's row is below the view"
        );
        let at_rest = numbered(20);
        let frame = at_rest
            .screen()
            .frame_reaching(&palette, Age::NEVER, OVERSCAN);
        let cursor = frame.cursor();
        assert!(cursor.visible && cursor.row < frame.rows());
    }

    // ADR-1310 D2: a row beyond the screen reads as changed only in a
    // frame drawn whole. After a counted scroll it reads unchanged, while
    // the row that came in at the bottom of the screen reads changed.
    #[test]
    fn a_row_beyond_the_screen_reads_changed_only_in_a_frame_drawn_whole() {
        let mut t = numbered(20);
        let palette = Palette::default();
        let seen = t.screen_mut().checkpoint();
        t.input(b"\n");
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert_eq!(frame.scrolled(), Some(1));
        assert!(!frame.beyond_changed());
        assert!(!frame.row_changed_at(-1));
        assert!(frame.cells_at(-1).all(|c| !c.changed));
        assert!(frame.row_changed_at(2), "the new bottom row");
        // a frame for a display that never drew is drawn whole
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        assert!(frame.row_changed_at(-1));
        assert!(frame.cell_at(-1, 0).unwrap().changed);
        // so is a frame after DECSCNM, which marks everything
        let seen = t.screen_mut().checkpoint();
        t.input(b"\x1b[?5h");
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert!(frame.row_changed_at(-1));
    }

    // ADR-1310 D2, "What a row beyond the screen carries": the line size.
    // A pair of double-height lines split by the screen's top edge pairs
    // across it, while the screen's own rows see no partner.
    #[test]
    fn a_double_height_pair_split_at_the_top_pairs_across_the_edge() {
        let mut t = Terminal::new(8, 3, 1000);
        t.screen_mut().set_scrollback_max(10);
        t.input(b"\x1b#3AB\r\n\x1b#4AB\r\nc\r\nd");
        let palette = Palette::default();
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        assert_eq!(frame.size_at(-1), LineSize::DoubleHeightTop);
        assert_eq!(frame.size_at(0), LineSize::DoubleHeightBottom);
        assert_eq!(frame.pair_at(0), Some(-1));
        assert_eq!(frame.pair_at(-1), Some(0));
        assert_eq!(frame.pair_row(0), None, "the screen's rows alone");
        assert_eq!(frame.size_at(-2), LineSize::Single, "outside the capture");
    }

    // ADR-1310 D2: the selection. Lines selected while the view was
    // scrolled back keep their selected columns once they lie above the
    // screen.
    #[test]
    fn a_row_above_the_screen_carries_its_selection() {
        let mut t = numbered(20);
        t.screen_mut().scroll_view_up(3);
        // rows 1 and 2 of the view: lines 15 and 16
        t.screen_mut().select_start(1, 1);
        t.screen_mut().select_to(0, 2);
        t.screen_mut().reset_view();
        let palette = Palette::default();
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        // lines 15 and 16 lie at rows -2 and -1
        assert_eq!(frame.selection_at(-3), 0..0);
        assert_eq!(frame.selection_at(-2), 1..4);
        assert_eq!(frame.selection_at(-1), 0..1);
        assert!(frame.cell_at(-1, 0).unwrap().selected);
        assert!(!frame.cell_at(-1, 1).unwrap().selected);
        let selected: Vec<bool> = frame.cells_at(-2).map(|c| c.selected).collect();
        assert_eq!(selected, [false, true, true, true]);
    }

    // ADR-1310 D2: the pictures placed on a row beyond the screen. A
    // kitty picture two rows tall that scrolled above the screen is in the
    // capture's list, at its row above the screen, and in none of the
    // screen's.
    #[cfg(feature = "images")]
    #[test]
    fn a_row_above_the_screen_carries_its_pictures() {
        use crate::images::{NewImage, Place, Protocol};
        let mut t = Terminal::new(12, 4, 100);
        t.screen_mut().set_image_limit(1 << 30);
        t.screen_mut().set_scrollback_max(10);
        let new = NewImage {
            protocol: Protocol::KITTY,
            width: 24,
            height: 32,
            pixels: vec![0xff12_3456; 24 * 32],
            id: 1,
            number: 0,
            transient: false,
            opaque: None,
        };
        let key = t.screen_mut().add_image(new).unwrap();
        let at = Place {
            col: 2,
            row: 1,
            ..Place::default()
        };
        t.screen_mut().place_image(key, at).unwrap();
        // five lines scroll: the picture's rows are 4 and 3 above the screen
        t.input(b"\x1b[4;1H\n\n\n\n\n");
        let palette = Palette::default();
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        assert_eq!(frame.rows_above(), K + 1);
        assert!(frame.placements().is_empty(), "none on the screen");
        let beyond: Vec<(i64, u32, u32, u32)> = frame
            .placements_beyond()
            .iter()
            .map(|p| (p.row, p.col, p.cols, p.rows))
            .collect();
        assert_eq!(beyond, [(-4, 2, 3, 2)]);
        // a capture that reaches 3 rows above holds its lower row, and
        // lists it whole at its top row, for the renderer to cut, as the
        // screen's list does
        let reach = Reach {
            above: 2,
            ..OVERSCAN
        };
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, reach);
        let part: Vec<(i64, u32, u32)> = frame
            .placements_beyond()
            .iter()
            .map(|p| (p.row, p.y, p.height))
            .collect();
        assert_eq!(part, [(-4, 0, 32)]);
        // a capture that reaches 2 rows above holds none of it
        let reach = Reach {
            above: 1,
            ..OVERSCAN
        };
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, reach);
        assert_eq!(frame.rows_above(), 2);
        assert!(frame.placements_beyond().is_empty());
    }

    // ADR-1310 D2, under bidi: a paragraph that crosses the screen's top
    // edge draws its row above the screen in the order its whole paragraph
    // gives, as the screen draws its own rows.
    #[cfg(feature = "bidi")]
    #[test]
    fn a_paragraph_crossing_the_top_edge_keeps_its_order() {
        let mut t = Terminal::new(6, 2, 1000);
        t.screen_mut().set_scrollback_max(10);
        t.input("אבגדהוזח\n".as_bytes());
        let palette = Palette::default();
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        let drawn = |row: isize| -> String {
            frame
                .cells_at(row)
                .map(|c| match c.glyph {
                    Glyph::Char(ch) => ch,
                    _ => '.',
                })
                .collect()
        };
        assert_eq!(drawn(-1), "והדגבא");
        assert_eq!(drawn(0), "חז....");
        assert!(frame.cell_at(-1, 0).unwrap().rtl);
        assert_eq!(frame.cell_at(-1, 0).unwrap().col, 5);
    }
}
