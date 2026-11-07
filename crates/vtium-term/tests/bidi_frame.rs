//! The frame of right-to-left text (ADR-1004 D3 to D11), driven through
//! the byte stream: each paragraph drawn in the order the bidirectional
//! algorithm gives, mirrored characters drawn as their counterparts, the
//! joining sides of Arabic letters, the cursor drawn where its logical
//! cell is, the selection and copy in logical order, the pointer mapped
//! back to the logical cell, the arrow keys under mode 1243, and the rows
//! a change redraws.
//!
//! A build without the `bidi` feature draws every row in the order it was
//! written and reports every cell where it was written.

mod common;

use common::term;
use vtium_term::{Age, Frame, Glyph, Palette, Terminal};

/// Each row of the frame as drawn, a character a column from the left,
/// an empty cell as `.` and a wide character's second cell as nothing.
fn drawn(t: &Terminal) -> Vec<String> {
    let palette = Palette::default();
    let frame = t.screen().frame(&palette, Age::NEVER);
    rows_of(&frame)
}

fn rows_of(frame: &Frame<'_>) -> Vec<String> {
    (0..frame.rows())
        .map(|r| {
            frame
                .cells(r)
                .filter_map(|c| match c.glyph {
                    Glyph::Char(ch) => Some(ch),
                    #[cfg(feature = "bidi")]
                    Glyph::Cluster(s) => c.mirror.or_else(|| s.chars().next()),
                    #[cfg(not(feature = "bidi"))]
                    Glyph::Cluster(s) => s.chars().next(),
                    Glyph::Empty if c.width == vtium_term::Width::WideTail => None,
                    Glyph::Empty => Some('.'),
                })
                .collect()
        })
        .collect()
}

// ADR-1004 D13: without the feature every row draws as written.
#[cfg(not(feature = "bidi"))]
#[test]
fn a_build_without_bidi_draws_in_written_order() {
    let mut t = term(10, 2, "abc שלום".as_bytes());
    assert_eq!(drawn(&t)[0], "abc שלום..");
    // the pointer names the cell it is over, and an arrow key is itself
    assert_eq!(t.pointer_cell(4, 0, 40), (4, 40));
    t.input(b"\x1b[2 k");
    assert_eq!(t.arrow_keysym(0xff51), 0xff51);
}

// ADR-1004 D13: a build without the feature has no Bidi_Class table.
#[cfg(not(feature = "bidi"))]
#[test]
fn a_build_without_bidi_calls_no_character_right_to_left() {
    assert!(!vtium_term::is_rtl_class('\u{05D0}'));
    assert!(!vtium_term::is_rtl_class('\u{0627}'));
}

#[cfg(feature = "bidi")]
mod with_the_feature {
    use super::*;
    use vtium_term::Joins;

    // A character's class alone: R and AL are right to left; Arabic digits,
    // marks, brackets and Latin letters are not.
    #[test]
    fn hebrew_and_arabic_letters_are_right_to_left_by_class() {
        use vtium_term::is_rtl_class;
        assert!(is_rtl_class('\u{05D0}')); // alef, R
        assert!(is_rtl_class('\u{0627}')); // alef, AL
        assert!(is_rtl_class('\u{07C0}')); // N'Ko digit zero, R
        assert!(!is_rtl_class('\u{0660}')); // Arabic-Indic zero, AN
        assert!(!is_rtl_class('\u{064E}')); // fatha, NSM
        assert!(!is_rtl_class('('));
        assert!(!is_rtl_class('a'));
        assert!(!is_rtl_class('1'));
    }

    // The figure of ADR-1004: abc, shalom, (1), def, the cursor after def.
    const MIXED: &str = "abc שלום (1) def";

    // ADR-1004 D3, D7: a left-to-right paragraph draws its Hebrew word
    // reversed in place, with the bracketed digit beside it, its brackets
    // mirrored, and the erased cells at the right.
    // covers: text.bidi
    #[test]
    fn a_mixed_line_draws_in_visual_order_in_a_left_to_right_paragraph() {
        let t = term(20, 1, MIXED.as_bytes());
        assert_eq!(drawn(&t)[0], "abc (1) םולש def....");
    }

    // ADR-1004 D3, D5: SCP 2 runs the paragraph right to left, so it starts
    // at the right edge and its erased cells are at the left.
    // covers: text.bidi, ctl.scp
    #[test]
    fn a_mixed_line_draws_from_the_right_edge_in_a_right_to_left_paragraph() {
        let t = term(20, 1, format!("\x1b[2 k{MIXED}").as_bytes());
        assert_eq!(drawn(&t)[0], "....def (1) םולש abc");
    }

    // ADR-1004 D3: European digits keep their order inside Hebrew, and a
    // bracket pair around Hebrew text resolves right to left and mirrors.
    // covers: text.bidi
    #[test]
    fn digits_and_brackets_follow_the_algorithm() {
        let t = term(16, 1, "אב 123 (גד)".as_bytes());
        assert_eq!(drawn(&t)[0], "(דג) 123 בא.....");
    }

    // ADR-1004 D3: a paragraph that wraps resolves as one paragraph, and
    // each row reorders on its own, so text never moves between rows.
    // covers: text.bidi
    #[test]
    fn a_paragraph_that_wraps_reorders_each_row_on_its_own() {
        let t = term(6, 2, "אבגדהוזח".as_bytes());
        assert_eq!(drawn(&t), ["והדגבא", "חז...."]);
    }

    // ADR-1004 D3: an erased cell inside a row reads as a space, and the
    // erased cells after the last written cell go to the paragraph's end.
    // covers: text.bidi
    #[test]
    fn erased_cells_inside_read_as_spaces_and_trailing_ones_go_to_the_end() {
        // א, an erased cell (CUF moves over it), then ב
        let t = term(6, 1, "א\x1b[Cב".as_bytes());
        assert_eq!(drawn(&t)[0], "ב.א...");
        let t = term(6, 1, "\x1b[2 kא\x1b[Cב".as_bytes());
        assert_eq!(drawn(&t)[0], "...ב.א");
    }

    // ADR-1004 D5: under mode 2501 a paragraph whose first strong
    // character is Hebrew runs right to left; one with none keeps its own
    // direction.
    // covers: DEC-2501
    #[test]
    fn detection_takes_the_first_strong_character() {
        let t = term(8, 3, "\x1b[?2501hשלום ab\r\n12 ab\r\n(1) שלום".as_bytes());
        assert_eq!(drawn(&t), [".ab םולש", "12 ab...", "םולש (1)"]);
    }

    // ADR-1004 D4: in explicit mode a left-to-right paragraph draws as
    // written, and a right-to-left one reversed, every character of it
    // mirrored.
    // covers: ANSI-8
    #[test]
    fn explicit_mode_draws_as_written_in_its_direction() {
        let t = term(10, 1, "\x1b[8lשלום (1)".as_bytes());
        assert_eq!(drawn(&t)[0], "שלום (1)..");
        let t = term(10, 1, "\x1b[8l\x1b[2 kab (1)".as_bytes());
        assert_eq!(drawn(&t)[0], "....(1) ba");
    }

    /// The joining sides of each cell of row `row`, from the left.
    fn joins(t: &Terminal, row: usize) -> Vec<(bool, bool)> {
        let palette = Palette::default();
        let frame = t.screen().frame(&palette, Age::NEVER);
        frame
            .cells(row)
            .map(|c| {
                let Joins { before, after } = c.joins;
                (before, after)
            })
            .collect()
    }

    // ADR-1004 D6: an Arabic word that wraps joins across the wrap, in the
    // order it was written; the frame carries each cell's sides.
    // covers: text.bidi
    #[test]
    fn joining_sides_come_from_logical_order_across_a_wrap() {
        // beh beh | beh alef: the third letter starts the second row
        let t = term(2, 2, "\u{0628}\u{0628}\u{0628}\u{0627}".as_bytes());
        // each row is reversed, so the cell read first is at the right
        assert_eq!(joins(&t, 0), [(true, true), (false, true)]);
        assert_eq!(joins(&t, 1), [(true, false), (true, true)]);
        // explicit mode joins the cells as drawn, read from right to left:
        // the program wrote alef then beh in visual order, so beh, at the
        // right, is read first and joins the alef at its left
        let t = term(2, 1, "\x1b[8l\u{0627}\u{0628}".as_bytes());
        assert_eq!(joins(&t, 0), [(true, false), (false, true)]);
    }

    // ADR-1004 D7: a mirrored character draws its counterpart only where it
    // resolves right to left, and box drawing mirrors under 2500 in a
    // right-to-left paragraph; the cell keeps what the program wrote.
    // covers: DEC-2500
    #[test]
    fn mirroring_and_box_drawing_under_2500() {
        let t = term(8, 1, "(a) (א)".as_bytes());
        assert_eq!(drawn(&t)[0], "(a) (א).");
        let t = term(8, 1, "\x1b[2 k\x1b[?2500h┌─┐ (a)".as_bytes());
        assert_eq!(drawn(&t)[0], ".(a) ┌─┐");
        let t = term(8, 1, "\x1b[2 k┌─┐ (a)".as_bytes());
        assert_eq!(drawn(&t)[0], ".(a) ┐─┌");
        assert_eq!(common::rows(t.screen())[0], "┌─┐ (a)");
    }

    // ADR-1004 D7: a bracket with a mark on it mirrors too; the frame
    // carries the counterpart, and the cell keeps the program's cluster.
    // covers: text.bidi
    #[test]
    fn a_mirrored_character_keeps_its_marks() {
        let t = term(10, 1, "\u{5D0}(\u{301}\u{5D1})".as_bytes());
        assert_eq!(drawn(&t)[0], "(\u{5D1})\u{5D0}......");
        let palette = Palette::default();
        let frame = t.screen().frame(&palette, Age::NEVER);
        let cell = frame.cell(0, 2).unwrap();
        assert_eq!(
            (cell.glyph, cell.mirror),
            (Glyph::Cluster("(\u{301}"), Some(')'))
        );
    }

    // ADR-1004 D9: the cursor stays on its logical cell and is drawn where
    // that cell is; a bar stands at the right of a right-to-left cell.
    // covers: text.bidi
    #[test]
    fn the_cursor_is_drawn_on_its_logical_cell() {
        let palette = Palette::default();
        // after abc and shalom, the cursor is on logical column 8, an
        // erased cell at the end, drawn at the right of the text
        let t = term(12, 1, "abc שלום".as_bytes());
        let c = t.screen().frame(&palette, Age::NEVER).cursor();
        assert_eq!((c.col, c.rtl), (8, false));
        // back on the shin, logical column 4, drawn at column 7
        let t = term(12, 1, "abc שלום\x1b[5G".as_bytes());
        let c = t.screen().frame(&palette, Age::NEVER).cursor();
        assert_eq!((c.col, c.rtl), (7, true));
        // in a right-to-left paragraph the cursor after the text is on
        // the left of it
        let t = term(12, 1, "\x1b[2 kשלום".as_bytes());
        let c = t.screen().frame(&palette, Age::NEVER).cursor();
        assert_eq!((c.col, c.rtl), (7, true));
    }

    // ADR-1004 D10: a pointer over a reordered cell names the logical
    // cell, its pixel the logical cell's left edge plus the offset inside
    // the cell it is over.
    // covers: text.bidi
    #[test]
    fn a_pointer_over_a_reordered_cell_names_its_logical_cell() {
        let mut t = term(12, 1, "abc שלום".as_bytes());
        t.set_cell_size(10, 20);
        // visual column 4 shows the final mem, logical column 7
        assert_eq!(t.pointer_cell(4, 0, 43), (7, 73));
        // a cell drawn in place names itself
        assert_eq!(t.pointer_cell(1, 0, 15), (1, 15));
    }

    // ADR-1004 D11: under 1243 Left and Right swap while the cursor's
    // paragraph resolves right to left; Home is itself.
    // covers: DEC-1243
    #[test]
    fn the_arrow_keys_swap_in_a_right_to_left_paragraph() {
        const LEFT: u32 = 0xff51;
        const RIGHT: u32 = 0xff53;
        const HOME: u32 = 0xff50;
        let mut t = term(8, 1, b"ab");
        assert_eq!(t.arrow_keysym(LEFT), LEFT);
        t.input(b"\r\x1b[2 k");
        assert_eq!(t.arrow_keysym(LEFT), RIGHT);
        assert_eq!(t.arrow_keysym(RIGHT), LEFT);
        assert_eq!(t.arrow_keysym(0xff96), 0xff98);
        assert_eq!(t.arrow_keysym(HOME), HOME);
        t.input(b"\x1b[?1243l");
        assert_eq!(t.arrow_keysym(LEFT), LEFT);
        // detection resolves a Hebrew paragraph right to left
        let t = term(8, 1, "\x1b[?2501hשלום".as_bytes());
        assert_eq!(t.arrow_keysym(LEFT), RIGHT);
    }

    // ADR-1004 D11 through the key encoder: under 1243 the Left arrow
    // key sends Right's sequence in a right-to-left paragraph, and its own
    // once 1243 is reset or in a left-to-right paragraph.
    // covers: DEC-1243
    #[test]
    fn terminal_key_swaps_the_arrows_in_a_right_to_left_paragraph() {
        use vtium_term::Mods;
        const LEFT: u32 = 0xff51;
        let key = |t: &mut Terminal, sym: u32| {
            t.take_replies();
            t.key(sym, 0, Mods::empty(), None);
            t.take_replies()
        };
        let mut t = term(8, 1, b"ab");
        assert_eq!(key(&mut t, LEFT), b"\x1b[D");
        t.input(b"\r\x1b[2 k");
        assert_eq!(key(&mut t, LEFT), b"\x1b[C");
        t.input(b"\x1b[?1243l");
        assert_eq!(key(&mut t, LEFT), b"\x1b[D");
    }

    // ADR-1004 D10 through the mouse encoder: a press over a reordered cell
    // reports the logical cell, in cells and, under mode 1016, in pixels.
    // covers: text.bidi
    #[test]
    fn terminal_mouse_reports_the_logical_cell() {
        use vtium_term::{Mods, MouseButton, MouseEvent};
        let press = |t: &mut Terminal, col: usize, px: u32| {
            t.take_replies();
            t.mouse(
                col,
                0,
                px,
                5,
                MouseButton::Left,
                MouseEvent::Press,
                Mods::empty(),
            );
            t.take_replies()
        };
        // visual column 4 shows the final mem, logical column 7
        let mut t = term(12, 1, "\x1b[?1000h\x1b[?1006habc שלום".as_bytes());
        t.set_cell_size(10, 20);
        assert_eq!(press(&mut t, 4, 43), b"\x1b[<0;8;1M");
        assert_eq!(press(&mut t, 1, 15), b"\x1b[<0;2;1M");
        t.input(b"\x1b[?1016h");
        assert_eq!(press(&mut t, 4, 43), b"\x1b[<0;74;6M");
    }

    // ADR-1004 D10: a drag from one drawn cell to another selects the
    // logical range between the cells it names, and copies it in the
    // order it was written; a word takes the logical word.
    // covers: text.bidi
    #[test]
    fn a_selection_is_a_logical_range_copied_in_written_order() {
        let mut t = term(20, 1, MIXED.as_bytes());
        // drawn "abc (1) םולש def": from the drawn 1 (column 5, logical 10)
        // to the drawn shin (column 11, logical 4), whichever is first in
        // the order written
        t.screen_mut().select_start(5, 0);
        t.screen_mut().select_to(11, 0);
        assert_eq!(t.screen().selection_text().as_deref(), Some("שלום (1"));
        // the frame shows the selected logical cells wherever they are drawn
        let palette = Palette::default();
        let frame = t.screen().frame(&palette, Age::NEVER);
        let shown: String = frame
            .cells(0)
            .map(|c| if c.selected { '#' } else { '.' })
            .collect();
        assert_eq!(shown, ".....#######........");
        assert_eq!(frame.selection().first(), Some(&(4..11)));
        assert_eq!(frame.selection().len(), 1);
        // a word under the drawn lamed is the Hebrew word
        t.screen_mut().select_word(10, 0);
        assert_eq!(t.screen().selection_text().as_deref(), Some("שלום"));
    }

    // ADR-1004 D2, D8: a rewrap keeps the paragraph's properties, so the
    // paragraph draws in its direction at the new width.
    // covers: text.bidi
    #[test]
    fn a_reflow_carries_the_properties() {
        let mut t = term(6, 3, "\x1b[2 kאבגדהו12".as_bytes());
        assert_eq!(drawn(&t)[..2], ["והדגבא", "....12"]);
        t.resize(10, 3);
        assert_eq!(drawn(&t)[0], "..12והדגבא");
    }

    // ADR-1004 D8: a rewrap makes new rows from the old ones' cells, and
    // each row it makes that holds Hebrew still draws through its
    // paragraph's order.
    // covers: text.bidi
    #[test]
    fn a_rewrap_keeps_reordering_the_rows_it_makes() {
        let mut t = term(
            6,
            3,
            "\u{5D0}\u{5D1}\u{5D2}\u{5D3}\u{5D4}\u{5D5}\u{5D6}\u{5D7}".as_bytes(),
        );
        t.resize(4, 3);
        assert_eq!(
            drawn(&t)[..2],
            [
                "\u{5D3}\u{5D2}\u{5D1}\u{5D0}",
                "\u{5D7}\u{5D6}\u{5D5}\u{5D4}"
            ]
        );
    }

    // ADR-1004, open question 4: implicit mode holds on the alternate
    // screen too.
    // covers: text.bidi
    #[test]
    fn the_alternate_screen_reorders() {
        let t = term(8, 1, "\x1b[?1049hab שלום".as_bytes());
        assert_eq!(drawn(&t)[0], "ab םולש.");
    }

    /// The rows of the frame a display that drew at `since` sees changed.
    fn changed_rows(t: &Terminal, since: vtium_term::Age) -> Vec<bool> {
        let palette = Palette::default();
        let frame = t.screen().frame(&palette, since);
        (0..frame.rows()).map(|r| frame.row_changed(r)).collect()
    }

    // ADR-1004 D8: a write in a paragraph drawn in another order marks
    // every row of it changed, and so does erasing its last Hebrew letter,
    // after which it draws in written order again.
    // covers: text.bidi
    #[test]
    fn a_change_in_a_reordered_paragraph_redraws_all_its_rows() {
        let mut t = term(4, 4, "אבגדהו\r\n\r\nxy".as_bytes());
        let since = t.screen_mut().checkpoint();
        t.input(b"\x1b[2;2Hz");
        assert_eq!(changed_rows(&t, since), [true, true, false, false]);
        // every cell of the first row reads changed, though none was written
        let palette = Palette::default();
        let frame = t.screen().frame(&palette, since);
        assert!(frame.cells(0).all(|c| c.changed));
        // erasing the Hebrew leaves Latin text that still redraws whole
        let mut t = term(6, 1, "a שלום".as_bytes());
        let since = t.screen_mut().checkpoint();
        t.input(b"\x1b[3G\x1b[K");
        assert_eq!(drawn(&t)[0], "a ....");
        let frame = t.screen().frame(&palette, since);
        assert!(frame.cells(0).all(|c| c.changed));
    }

    // ADR-1004 D8: an edit that ends a row's wrap splits its paragraph,
    // and the rows below it, which now resolve on their own, are marked
    // changed: digits that followed Hebrew, and Latin text whose detected
    // direction came from Hebrew above it.
    // covers: text.bidi
    #[test]
    fn a_split_paragraph_marks_the_rows_whose_order_changes() {
        for (input, before, after) in [
            ("\u{5D0}\u{5D1}\u{5D2}\u{5D3}1 2", "2 1.", "1 2."),
            ("\x1b[?2501h\u{5D0}\u{5D1}\u{5D2}\u{5D3}ab", "..ab", "ab.."),
        ] {
            let mut t = term(4, 2, input.as_bytes());
            assert_eq!(drawn(&t)[1], before, "{input:?}");
            let since = t.screen_mut().checkpoint();
            t.input(b"\x1b[1;1H\x1b[X");
            assert_eq!(drawn(&t)[1], after, "{input:?}");
            assert_eq!(changed_rows(&t, since), [true, true], "{input:?}");
        }
    }

    // ADR-1004 D8: a change of a paragraph property marks the rows of the
    // paragraph whose order it changes; a left-to-right row of Latin text
    // keeps its per-cell damage.
    // covers: text.bidi
    #[test]
    fn a_property_change_marks_the_paragraphs_rows() {
        let mut t = term(4, 2, b"abcdef\r");
        t.input(b"\x1b[H");
        let since = t.screen_mut().checkpoint();
        t.input(b"\x1b[2 k");
        assert_eq!(changed_rows(&t, since), [true, true]);
        assert_eq!(drawn(&t), ["abcd", "..ef"]);
        let mut t = term(4, 2, b"ab\r\ncd");
        let since = t.screen_mut().checkpoint();
        t.input(b"\x1b[1;3Hx");
        assert_eq!(changed_rows(&t, since), [true, false]);
        let palette = Palette::default();
        let frame = t.screen().frame(&palette, since);
        let changed: Vec<bool> = frame.cells(0).map(|c| c.changed).collect();
        assert_eq!(changed, [false, false, true, false]);
    }

    // ADR-1004 D8: a frame rebuilt from its parts, as a snapshot a render
    // thread reads, draws the same order.
    // covers: text.bidi
    #[test]
    fn a_frame_rebuilt_from_its_parts_keeps_its_order() {
        let t = term(20, 1, MIXED.as_bytes());
        let palette = Palette::default();
        let frame = t.screen().frame(&palette, Age::NEVER);
        let lines: Vec<&vtium_term::Line> =
            (0..frame.rows()).filter_map(|r| frame.line(r)).collect();
        let rebuilt = Frame::new(
            lines,
            t.screen().damage(),
            &palette,
            Age::NEVER,
            frame.cursor(),
            false,
        )
        .with_selection(frame.selection().to_vec())
        .with_order(frame.order().clone());
        assert_eq!(rows_of(&rebuilt), rows_of(&frame));
        assert_eq!(rebuilt.cursor(), frame.cursor());
    }
}
