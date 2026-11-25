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

    /// A terminal of 4 columns by 3 rows whose link ABCDEFG wraps from row
    /// -1, which shows ABCD, onto row 0, which shows EFG, at rest.
    fn link_across_the_top_edge() -> Terminal {
        let mut t = Terminal::new(4, 3, 1000);
        t.screen_mut().set_scrollback_max(100);
        t.input(b"a\r\nb\r\nc\r\n\x1b]8;;http://x/\x1b\\ABCDEFG\x1b]8;;\x1b\\\r\n\r\n");
        t
    }

    // A row above the screen whose look changes reads as changed. Hovering a
    // link that wraps from row -1 onto row 0 underlines row -1 too, and
    // moving the pointer off the link takes the underline away again.
    #[test]
    fn a_hover_that_reaches_a_row_above_the_screen_reads_as_changed() {
        let mut t = link_across_the_top_edge();
        let palette = Palette::default();
        let seen = t.screen_mut().checkpoint();
        t.screen_mut().set_pointer(Some((0, 0)), false);
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert!(
            frame.cell_at(-1, 0).unwrap().hovered,
            "row -1 is part of it"
        );
        assert!(frame.row_changed_at(-1), "hovered");
        assert!(frame.cells_at(-1).all(|c| c.changed));
        let seen = t.screen_mut().checkpoint();
        t.screen_mut().set_pointer(Some((3, 2)), false);
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert!(!frame.cell_at(-1, 0).unwrap().hovered);
        assert!(frame.row_changed_at(-1), "no longer hovered");
        // a hover of a link wholly on the screen leaves the rows above alone
        t.input(b"\x1b]8;;http://y/\x1b\\Z\x1b]8;;\x1b\\");
        let seen = t.screen_mut().checkpoint();
        t.screen_mut().set_pointer(Some((0, 2)), false);
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert!(frame.hovered().is_some());
        assert_eq!(frame.scrolled(), Some(0));
        assert!(!frame.row_changed_at(-1), "the hover is on the screen only");
    }

    // When the link store is full, a sweep strips the links of the
    // scrollback's oldest lines. A stripped line that held the hovered link
    // loses the hover's underline, so the frame reads as drawn whole.
    #[test]
    fn a_strip_of_the_hovered_link_from_a_row_above_reads_as_changed() {
        let osc8 = |uri: &str| format!("\x1b]8;;{uri}\x1b\\");
        let hovered = "https://hovered.example/";
        let mut t = Terminal::new(4, 3, 1000);
        t.screen_mut().set_scrollback_max(1000);
        // the oldest line holds the link, then 240 lines of 1000-byte
        // links fill the store to within a few links of its bound
        t.input(format!("{}L{}\r\n", osc8(hovered), osc8("")).as_bytes());
        for i in 0..240 {
            let uri = format!("https://{i:08}.example/{}", "p".repeat(975));
            t.input(format!("{}x{}\r\n", osc8(&uri), osc8("")).as_bytes());
        }
        // the link again on the bottom row, under the pointer
        t.input(format!("{}L{}", osc8(hovered), osc8("")).as_bytes());
        t.screen_mut().set_pointer(Some((0, 2)), false);
        let oldest = |t: &Terminal| {
            let line = t.screen().scrollback().next().unwrap();
            line.link(line.cell(0)).is_some()
        };
        assert!(oldest(&t), "the oldest line holds the link");
        let seen = t.screen_mut().checkpoint();
        // new links in one cell of the bottom row, until a sweep strips the
        // oldest line
        for i in 0..20 {
            let uri = format!("https://new{i:05}.example/{}", "q".repeat(975));
            t.input(format!("\x1b[3;2H{}y{}", osc8(&uri), osc8("")).as_bytes());
            if !oldest(&t) {
                break;
            }
        }
        assert!(!oldest(&t), "a sweep stripped the oldest line");
        assert!(t.screen().hovered_link().is_some(), "the hover stays");
        assert_eq!(t.screen().damage().scrolled(seen), None, "drawn whole");
    }

    // A label of link hints gives every cell of its link the hover's dashed
    // underline, so a link that reaches a row above the screen changes that
    // row's look as its label comes and goes.
    #[test]
    fn a_link_hint_that_reaches_a_row_above_the_screen_reads_as_changed() {
        let mut t = link_across_the_top_edge();
        let link = t.screen().link_at(0, 0).unwrap();
        let palette = Palette::default();
        let seen = t.screen_mut().checkpoint();
        t.screen_mut().set_link_hints(vec![(link, "a".to_owned())]);
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert_eq!(frame.labelled_links(), [link]);
        assert!(frame.row_changed_at(-1), "labelled");
        let seen = t.screen_mut().checkpoint();
        t.screen_mut().set_link_hints(Vec::new());
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert!(frame.row_changed_at(-1), "no longer labelled");
        // the label goes when its link leaves the view
        t.screen_mut().set_link_hints(vec![(link, "a".to_owned())]);
        let seen = t.screen_mut().checkpoint();
        t.input(b"\n");
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert!(frame.labelled_links().is_empty(), "the link left the view");
        assert!(frame.row_changed_at(-1), "the label went");
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

    // A kitty delete of a picture whose box reaches a row above the screen
    // changes that row's look, so the frame reads as drawn whole. Deleting
    // a picture that lies on the screen alone keeps the scroll record.
    #[cfg(feature = "images")]
    #[test]
    fn a_kitty_delete_that_reaches_a_row_above_the_screen_reads_as_changed() {
        use crate::images::{Kitty, Switches};
        use crate::terminal::ImageString;
        let mut t = Terminal::new(12, 4, 100);
        t.screen_mut().set_scrollback_max(10);
        t.screen_mut().set_image_limit(1 << 30);
        t.screen_mut().set_image_cell(8, 16).unwrap();
        t.set_image_switches(Switches {
            sixel: false,
            kitty: true,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
        // pictures three rows tall: id 7 at row 1, then id 8 at row 1 after
        // two lines scrolled, so 7's rows are -1 to 1 and 8's 1 to 3
        let picture = |id: u32| {
            let mut s = format!("\x1b[2;1H\x1b_Ga=T,f=24,s=8,v=48,i={id},C=1,q=2;").into_bytes();
            s.extend(b"////".repeat(384));
            s.extend(b"\x1b\\");
            s
        };
        t.input(&picture(7));
        t.input(b"\x1b[4;1H\n\n");
        t.input(&picture(8));
        let palette = Palette::default();
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        let above = |frame: &Frame<'_>| {
            frame
                .placements_beyond()
                .iter()
                .filter(|p| p.row < 0)
                .count()
        };
        assert_eq!(above(&frame), 1, "7 reaches row -1");
        let seen = t.screen_mut().checkpoint();
        t.input(b"\x1b_Ga=d,d=i,i=8,q=2\x1b\\");
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert_eq!(frame.scrolled(), Some(0), "8 lies on the screen alone");
        assert!(!frame.row_changed_at(-1));
        let seen = t.screen_mut().checkpoint();
        t.input(b"\x1b_Ga=d,d=i,i=7,q=2\x1b\\");
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert_eq!(above(&frame), 0, "deleted");
        assert!(frame.row_changed_at(-1), "row -1 lost its picture");
    }

    // ADR-1310 D2 and D6: while the view is scrolled back, the bottom side
    // holds the rows asked for and one more for the ink of the rows next to
    // them, each the line that follows the view in reading order.
    #[test]
    fn a_view_scrolled_back_captures_the_rows_asked_below_and_one_more() {
        let mut t = numbered(30);
        t.screen_mut().scroll_view_up(10);
        let palette = Palette::default();
        let reach = Reach {
            above: 1,
            below: 2,
            ..OVERSCAN
        };
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, reach);
        assert_eq!((frame.rows_above(), frame.rows_below()), (2, 3));
        // lines 0 to 29: the view shows 17 to 19
        let want: Vec<String> = (15..23).map(|i| i.to_string()).collect();
        assert_eq!(texts(&frame), want);
    }

    // ADR-1310 D2: the selection of a row below the screen. Lines selected
    // while the view showed them keep their selected columns once the view
    // scrolls further back and they lie below it.
    #[test]
    fn a_row_below_the_screen_carries_its_selection() {
        let mut t = numbered(20);
        t.screen_mut().scroll_view_up(6);
        // rows 1 and 2 of the view: lines 12 and 13
        t.screen_mut().select_start(1, 1);
        t.screen_mut().select_to(0, 2);
        t.screen_mut().scroll_view_up(2);
        let palette = Palette::default();
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        // the view shows lines 9 to 11, so 12 and 13 lie at rows 3 and 4
        assert_eq!(frame.selection_at(2), 0..0);
        assert_eq!(frame.selection_at(3), 1..4);
        assert_eq!(frame.selection_at(4), 0..1);
        assert_eq!(frame.selection_at(5), 0..0);
        assert!(frame.cell_at(4, 0).unwrap().selected);
        assert!(!frame.cell_at(4, 1).unwrap().selected);
        let selected: Vec<bool> = frame.cells_at(3).map(|c| c.selected).collect();
        assert_eq!(selected, [false, true, true, true]);
    }

    // A kitty picture shown through Unicode placeholder cells draws on the
    // rows that hold the cells. When the picture changes, a row above the
    // screen that holds its placeholder cells changes its look, so the
    // frame reads as drawn whole.
    #[cfg(feature = "images")]
    #[test]
    fn a_changed_picture_under_placeholder_cells_above_the_screen_reads_as_changed() {
        use crate::images::{Kitty, Switches};
        use crate::terminal::ImageString;
        let mut t = Terminal::new(12, 4, 100);
        t.screen_mut().set_scrollback_max(10);
        t.screen_mut().set_image_limit(1 << 30);
        t.screen_mut().set_image_cell(8, 16).unwrap();
        t.set_image_switches(Switches {
            sixel: false,
            kitty: true,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Kitty, Some(Box::new(Kitty::default())));
        let picture = |fill: &[u8]| {
            let mut s = b"\x1b_Ga=t,f=24,s=8,v=16,i=42,q=2;".to_vec();
            s.extend(fill.repeat(128));
            s.extend(b"\x1b\\");
            s
        };
        t.input(&picture(b"////"));
        t.input(b"\x1b_Ga=p,U=1,i=42,p=2,c=1,r=1,q=2\x1b\\");
        // the placeholder cell on row 0, then four lines scroll it to row -1
        t.input("\x1b[1;1H\x1b[38;5;42m\u{10EEEE}\u{0305}\u{0305}\x1b[m".as_bytes());
        t.input(b"\x1b[4;1H\n");
        let palette = Palette::default();
        let above = |frame: &Frame<'_>| {
            frame
                .placements_beyond()
                .iter()
                .filter(|p| p.row < 0)
                .count()
        };
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        assert_eq!(above(&frame), 1, "the placeholder cell draws on row -1");
        let seen = t.screen_mut().checkpoint();
        t.input(&picture(b"AAAA"));
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert!(frame.row_changed_at(-1), "the picture under row -1 changed");
        // sending the picture again replaced it with its placements; a
        // picture placed relative to a new virtual placement draws at the
        // origin its placeholder cells give, on row -1, and its delete
        // changes that row too
        t.input(b"\x1b_Ga=p,U=1,i=42,p=2,c=1,r=1,q=2\x1b\\");
        t.input(b"\x1b_Ga=t,f=24,s=8,v=16,i=3,q=2;");
        t.input(&b"////".repeat(128));
        t.input(b"\x1b\\\x1b_Ga=p,i=3,p=1,P=42,Q=2,H=1,V=0,q=2\x1b\\");
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        assert_eq!(above(&frame), 2, "the child draws on row -1");
        let seen = t.screen_mut().checkpoint();
        t.input(b"\x1b_Ga=d,d=i,i=3,q=2\x1b\\");
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert_eq!(above(&frame), 1, "deleted");
        assert!(frame.row_changed_at(-1), "the child left row -1");
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

    // Under bidi, a row above the screen that continues into row 0's
    // paragraph is ordered with the whole paragraph, so an edit of row 0
    // can move its characters. Here row -1 draws "!12 בא" before the edit
    // and "12 בא!" after it, so it reads as changed, while a row above that
    // paragraph keeps reading unchanged.
    #[cfg(feature = "bidi")]
    #[test]
    fn an_edit_of_row_0_marks_the_rows_above_of_its_paragraph() {
        let mut t = Terminal::new(6, 2, 1000);
        t.screen_mut().set_scrollback_max(10);
        t.input("x\r\nאב 12!גד\r\n".as_bytes());
        let palette = Palette::default();
        let drawn = |frame: &Frame<'_>, row: isize| -> String {
            frame
                .cells_at(row)
                .map(|c| match c.glyph {
                    Glyph::Char(ch) => ch,
                    _ => '.',
                })
                .collect()
        };
        let frame = t.screen().frame_reaching(&palette, Age::NEVER, OVERSCAN);
        let before = drawn(&frame, -1);
        assert_eq!(drawn(&frame, -2), "x.....");
        let seen = t.screen_mut().checkpoint();
        t.input(b"\x1b[1;1Hab");
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert_eq!(frame.scrolled(), Some(0));
        assert_ne!(drawn(&frame, -1), before, "row -1 is ordered again");
        assert!(frame.row_changed_at(-1));
        assert!(frame.cells_at(-1).all(|c| c.changed));
        assert!(!frame.row_changed_at(-2), "another paragraph");
        // with no change on the screen, nothing above changes either
        let seen = t.screen_mut().checkpoint();
        let frame = t.screen().frame_reaching(&palette, seen, OVERSCAN);
        assert!(!frame.row_changed_at(-1));
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

    // ADR-1310 D2, under bidi: a paragraph that crosses the bottom edge of
    // a view scrolled back draws its row below the view in the order its
    // whole paragraph gives.
    #[cfg(feature = "bidi")]
    #[test]
    fn a_paragraph_crossing_the_bottom_edge_keeps_its_order() {
        let mut t = Terminal::new(6, 2, 1000);
        t.screen_mut().set_scrollback_max(10);
        t.input("x\r\nאבגדהוזח\r\n1\r\n2\r\n3".as_bytes());
        // lines x, the paragraph's two rows, 1, 2 and 3: a view four rows
        // back shows x and the paragraph's first row
        t.screen_mut().scroll_view_up(4);
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
        assert_eq!(drawn(0), "x.....");
        assert_eq!(drawn(1), "והדגבא");
        assert_eq!(drawn(2), "חז....");
        assert!(frame.cell_at(2, 0).unwrap().rtl);
        assert_eq!(frame.cell_at(2, 0).unwrap().col, 1);
    }
}
