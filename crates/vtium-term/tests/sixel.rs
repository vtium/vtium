//! Sixel through the terminal (ADR-1100, D3, D6 and D10): where a picture
//! goes and where the cursor ends, the three sixel modes, DA1's attribute
//! 4, XTSMGRAPHICS, the color registers, the budget, a cancelled string
//! and the work bound. Built without images, the modes have no row, DA1
//! has no attribute 4 and XTSMGRAPHICS answers status 1.

mod common;

use vtium_term::Terminal;

/// A sixel string: DCS with parameters `params`, the data, ST.
fn sixel(params: &str, data: &str) -> Vec<u8> {
    format!("\x1bP{params}q{data}\x1b\\").into_bytes()
}

/// A solid picture of `w` by `h` dots in register 1, which the string
/// defines as pure blue, sized by raster attributes.
fn solid(w: u32, h: u32) -> Vec<u8> {
    let mut data = format!("\"1;1;{w};{h}#1;2;0;0;100#1");
    for band in 0..h.div_ceil(6) {
        let bits = (h - band * 6).min(6);
        let sixel = char::from(0x3f + ((1u8 << bits) - 1));
        data += &format!("!{w}{sixel}-");
    }
    sixel("0;0;0", &data)
}

fn reply(t: &mut Terminal, query: &[u8]) -> String {
    t.take_replies();
    t.input(query);
    String::from_utf8_lossy(&t.take_replies()).into_owned()
}

#[cfg(not(feature = "images"))]
mod minspec {
    use super::*;

    // ADR-1100 D10: a build without images answers XTSMGRAPHICS status 1,
    // "error in the item", for every item and action, with the item it
    // asked about, 0 when left out, and no value.
    // covers: query.xtsmgraphics
    #[test]
    fn xtsmgraphics_answers_status_1_in_a_build_without_images() {
        let mut t = Terminal::new(12, 4, 0);
        assert_eq!(reply(&mut t, b"\x1b[?1;1S"), "\x1b[?1;1S");
        assert_eq!(reply(&mut t, b"\x1b[?1;4S"), "\x1b[?1;1S");
        assert_eq!(reply(&mut t, b"\x1b[?2;1S"), "\x1b[?2;1S");
        assert_eq!(reply(&mut t, b"\x1b[?1;3;256S"), "\x1b[?1;1S");
        assert_eq!(reply(&mut t, b"\x1b[?S"), "\x1b[?0;1S");
    }

    // ADR-1100 D10: modes 80, 1070 and 8452 have no row without images, so
    // DECRQM answers 0, and DA1 carries no attribute 4.
    #[test]
    fn the_sixel_modes_have_no_row_and_da1_no_sixel_without_images() {
        let mut t = Terminal::new(12, 4, 0);
        for n in [80, 1070, 8452] {
            let want = format!("\x1b[?{n};0$y");
            assert_eq!(reply(&mut t, format!("\x1b[?{n}$p").as_bytes()), want);
        }
        assert_eq!(reply(&mut t, b"\x1b[c"), "\x1b[?62;22c");
        // a sixel string is ignored as every string vtium does not take
        let before = t.screen().clone();
        t.input(&solid(8, 6));
        assert!(t.screen() == &before);
    }
}

#[cfg(feature = "images")]
mod images {
    use super::common::rows;
    use super::*;
    use vtium_term::ImageString;
    use vtium_term::images::{Limits, Sixel, Switches};

    const BLUE: u32 = 0xff00_00ff;

    /// A terminal of `cols` by `rows` cells of 8 by 16 pixels that offers
    /// sixel: the handler installed, sixel on, a budget of 1 GiB.
    fn term(cols: usize, rows: usize) -> Terminal {
        let mut t = Terminal::new(cols, rows, 100);
        t.screen_mut().set_image_limit(1 << 30);
        t.screen_mut().set_image_cell(8, 16).expect("a cell");
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Sixel, Some(Box::new(Sixel::default())));
        t
    }

    /// The placements as (row, col, cols, rows, src_y, src_height).
    fn shown(t: &Terminal) -> Vec<(i64, u32, u32, u32, u32, u32)> {
        t.screen()
            .placements()
            .iter()
            .map(|p| (p.row, p.col, p.cols, p.rows, p.src_y, p.src_height))
            .collect()
    }

    fn first_pixel(t: &Terminal) -> u32 {
        let p = t.screen().placements();
        p.first().expect("a placement").pixels.as_slice()[0]
    }

    fn sixel_off(t: &mut Terminal) {
        t.set_image_switches(Switches {
            sixel: false,
            kitty: false,
            iterm2: false,
        });
    }

    // covers: image.sixel
    #[test]
    fn a_picture_goes_at_the_cursor_and_the_cursor_ends_on_its_last_row() {
        let mut t = term(12, 6);
        t.input(b"\x1b[2;3H");
        // 20 by 40 dots: 3 columns and 3 rows of 8 by 16
        t.input(&solid(20, 40));
        assert_eq!(shown(&t), [(1, 2, 3, 3, 0, 40)]);
        assert_eq!(first_pixel(&t), BLUE);
        // on the row of its last pixel row, at its left column
        assert_eq!(t.screen().cursor(), (2, 3));
        // a pending wrap counts as the last column
        let mut t = term(12, 6);
        t.input(b"\x1b[1;12Hx");
        assert_eq!(t.screen().cursor(), (12, 0));
        t.input(&solid(8, 16));
        assert_eq!(shown(&t), [(0, 11, 1, 1, 0, 16)]);
        assert_eq!(t.screen().cursor(), (11, 0));
    }

    // C reference: tsm-screen.c's screen_sixel_place scrolled the region
    // first, as far as it allowed, so a picture that fits the region moves
    // whole, and indexed for the rest.
    // covers: image.sixel
    #[test]
    fn a_picture_past_the_bottom_margin_scrolls_the_region_first_and_moves_whole() {
        let mut t = term(12, 6);
        // the region is rows 1 to 4; text on each of its rows
        t.input(b"\x1b[2;5r\x1b[2;1Ha\x1b[3;1Hb\x1b[4;1Hc\x1b[5;1Hd\x1b[4;1H");
        // three rows tall at row 3: one row past the margin, so the region
        // scrolls one row and the picture sits whole at row 2
        t.input(&solid(8, 48));
        assert_eq!(shown(&t), [(2, 0, 1, 3, 0, 48)]);
        assert_eq!(rows(t.screen())[1..5], ["b", "c", "d", ""]);
        assert_eq!(t.screen().cursor(), (0, 4));
        // six rows tall at row 2: the region scrolls one row, as far as it
        // can, and the picture sits at row 1, rows 1 to 6; the index then
        // scrolls the region two rows more, which pushes the picture's top
        // two rows out of it and leaves the two below the region in place
        let mut t = term(12, 6);
        t.input(b"\x1b[2;5r\x1b[3;1H");
        t.input(&solid(8, 16 * 6));
        assert_eq!(t.screen().cursor(), (0, 4));
        assert_eq!(shown(&t), [(1, 0, 1, 2, 32, 32), (5, 0, 1, 2, 64, 32)]);
    }

    // covers: DEC-80
    #[test]
    fn decsdm_places_a_picture_at_the_top_left_and_leaves_the_cursor() {
        let mut t = term(12, 4);
        t.input(b"\x1b[?80h\x1b[3;4H");
        // taller than the screen's 64 pixels: cut to the screen
        t.input(&solid(16, 100));
        assert_eq!(shown(&t), [(0, 0, 2, 4, 0, 64)]);
        assert_eq!(t.screen().cursor(), (3, 2));
        assert_eq!(reply(&mut t, b"\x1b[?80$p"), "\x1b[?80;1$y");
        // reset, the next picture goes at the cursor again
        t.input(b"\x1b[?80l");
        t.input(&solid(8, 16));
        assert!(shown(&t).contains(&(2, 3, 1, 1, 0, 16)));
        assert_eq!(reply(&mut t, b"\x1b[?80$p"), "\x1b[?80;2$y");
    }

    // covers: DEC-8452
    #[test]
    fn mode_8452_ends_the_cursor_right_of_the_picture() {
        let mut t = term(12, 4);
        t.input(b"\x1b[?8452h\x1b[1;3H");
        t.input(&solid(16, 32));
        assert_eq!(t.screen().cursor(), (4, 1));
        // at most the last column
        t.input(b"\x1b[1;11H");
        t.input(&solid(24, 16));
        assert_eq!(t.screen().cursor(), (11, 0));
        assert_eq!(reply(&mut t, b"\x1b[?8452$p"), "\x1b[?8452;1$y");
    }

    /// A picture of one dot in register 5, which `define` defines first.
    fn dot(define: bool) -> Vec<u8> {
        let data = if define { "#5;2;100;0;0#5@" } else { "#5@" };
        sixel("0;0;0", data)
    }

    // covers: DEC-1070
    #[test]
    fn mode_1070_gives_each_picture_its_own_registers() {
        // VT340's register 5, cyan: 20% 80% 80%
        const CYAN: u32 = 0xff33_cccc;
        const RED: u32 = 0xffff_0000;
        let last = |t: &Terminal| {
            let p = t.screen().placements();
            p.last().expect("a placement").pixels.as_slice()[0]
        };
        // set, as private-color-registers starts it: each picture starts
        // from the defaults
        let mut t = term(12, 4);
        assert_eq!(reply(&mut t, b"\x1b[?1070$p"), "\x1b[?1070;1$y");
        t.input(&dot(true));
        assert_eq!(last(&t), RED);
        t.input(b"\x1b[1;5H");
        t.input(&dot(false));
        assert_eq!(last(&t), CYAN);
        // reset: the pictures share their registers
        t.input(b"\x1b[?1070l\x1b[2;1H");
        t.input(&dot(true));
        t.input(b"\x1b[2;5H");
        t.input(&dot(false));
        assert_eq!(last(&t), RED);
        // DECSTR keeps the mode and the registers; RIS resets both
        t.input(b"\x1b[!p\x1b[3;5H");
        t.input(&dot(false));
        assert_eq!(last(&t), RED);
        t.input(b"\x1bc\x1b[?1070l");
        t.input(&dot(false));
        assert_eq!(last(&t), CYAN);
    }

    // C reference: tsm-modes.def marked 80, 1070 and 8452 HOST_SIXEL, and
    // mode_offered reported their fixed state while sixel was off; SM and
    // RM still stored them.
    // covers: DEC-80, DEC-1070, DEC-8452
    #[test]
    fn the_sixel_modes_are_offered_while_sixel_is() {
        let mut t = term(12, 4);
        t.input(b"\x1b[?80h\x1b[?1070l\x1b[?8452h");
        sixel_off(&mut t);
        // their fixed state: 80 and 8452 reset, 1070 as
        // private-color-registers says
        assert_eq!(reply(&mut t, b"\x1b[?80$p"), "\x1b[?80;4$y");
        assert_eq!(reply(&mut t, b"\x1b[?1070$p"), "\x1b[?1070;3$y");
        assert_eq!(reply(&mut t, b"\x1b[?8452$p"), "\x1b[?8452;4$y");
        // SM and RM still store them for when sixel comes on
        t.input(b"\x1b[?80l");
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        assert_eq!(reply(&mut t, b"\x1b[?80$p"), "\x1b[?80;2$y");
        assert_eq!(reply(&mut t, b"\x1b[?1070$p"), "\x1b[?1070;2$y");
        assert_eq!(reply(&mut t, b"\x1b[?8452$p"), "\x1b[?8452;1$y");
        // a budget of 0 is sixel off too
        t.screen_mut().set_image_limit(0);
        assert_eq!(reply(&mut t, b"\x1b[?80$p"), "\x1b[?80;4$y");
        t.screen_mut().set_image_limit(1 << 30);
        // RIS resets them, DECSTR does not
        t.input(b"\x1b[!p");
        assert_eq!(reply(&mut t, b"\x1b[?8452$p"), "\x1b[?8452;1$y");
        t.input(b"\x1bc");
        assert_eq!(reply(&mut t, b"\x1b[?8452$p"), "\x1b[?8452;2$y");
        assert_eq!(reply(&mut t, b"\x1b[?1070$p"), "\x1b[?1070;1$y");
        // private-color-registers sets 1070's state at reset
        t.set_image_limits(Limits {
            private_registers: false,
            ..Limits::default()
        });
        assert_eq!(reply(&mut t, b"\x1b[?1070$p"), "\x1b[?1070;2$y");
        sixel_off(&mut t);
        assert_eq!(reply(&mut t, b"\x1b[?1070$p"), "\x1b[?1070;4$y");
    }

    // covers: query.da1
    #[test]
    fn da1_reports_sixel_while_sixel_is_offered() {
        let mut t = term(12, 4);
        assert_eq!(reply(&mut t, b"\x1b[c"), "\x1b[?62;4;22c");
        sixel_off(&mut t);
        assert_eq!(reply(&mut t, b"\x1b[c"), "\x1b[?62;22c");
        let mut t = Terminal::new(12, 4, 0);
        assert_eq!(reply(&mut t, b"\x1b[c"), "\x1b[?62;22c", "no handler");
    }

    // C reference: tsm-vte-image.c's vte_xtsmgraphics.
    // covers: query.xtsmgraphics
    #[test]
    fn xtsmgraphics_reads_sets_and_resets_the_register_count() {
        let mut t = term(12, 4);
        assert_eq!(reply(&mut t, b"\x1b[?1;1S"), "\x1b[?1;0;1024S");
        assert_eq!(reply(&mut t, b"\x1b[?1;3;256S"), "\x1b[?1;0;256S");
        assert_eq!(reply(&mut t, b"\x1b[?1;1S"), "\x1b[?1;0;256S");
        // from 2 up to num-color-registers
        assert_eq!(reply(&mut t, b"\x1b[?1;3;1S"), "\x1b[?1;3S");
        assert_eq!(reply(&mut t, b"\x1b[?1;3;1025S"), "\x1b[?1;3S");
        assert_eq!(reply(&mut t, b"\x1b[?1;3S"), "\x1b[?1;3S");
        assert_eq!(reply(&mut t, b"\x1b[?1;1S"), "\x1b[?1;0;256S");
        assert_eq!(reply(&mut t, b"\x1b[?1;4S"), "\x1b[?1;0;1024S");
        assert_eq!(reply(&mut t, b"\x1b[?1;2S"), "\x1b[?1;0;1024S");
        assert_eq!(reply(&mut t, b"\x1b[?1;1S"), "\x1b[?1;0;1024S");
        // the geometry is max-graphic-size, and cannot be changed
        assert_eq!(reply(&mut t, b"\x1b[?2;1S"), "\x1b[?2;0;10000;10000S");
        assert_eq!(reply(&mut t, b"\x1b[?2;4S"), "\x1b[?2;0;10000;10000S");
        assert_eq!(reply(&mut t, b"\x1b[?2;3;100;100S"), "\x1b[?2;3S");
        assert_eq!(reply(&mut t, b"\x1b[?2;2S"), "\x1b[?2;3S");
        // an action it does not know, an item it does not know
        assert_eq!(reply(&mut t, b"\x1b[?1;9S"), "\x1b[?1;2S");
        assert_eq!(reply(&mut t, b"\x1b[?2S"), "\x1b[?2;2S");
        assert_eq!(reply(&mut t, b"\x1b[?3;1S"), "\x1b[?3;1S");
        assert_eq!(reply(&mut t, b"\x1b[?S"), "\x1b[?0;1S");
        // RIS returns the count to num-color-registers
        t.input(b"\x1b[?1;3;16S\x1bc");
        assert_eq!(reply(&mut t, b"\x1b[?1;1S"), "\x1b[?1;0;1024S");
        // while sixel is off, every item is status 1
        sixel_off(&mut t);
        assert_eq!(reply(&mut t, b"\x1b[?1;1S"), "\x1b[?1;1S");
        assert_eq!(reply(&mut t, b"\x1b[?2;1S"), "\x1b[?2;1S");
    }

    // covers: query.xtsmgraphics, image.sixel
    #[test]
    fn a_register_count_xtsmgraphics_sets_takes_effect_at_the_next_picture() {
        // VT340's register 15, gray 80%
        const GRAY: u32 = 0xffcc_cccc;
        let mut t = term(12, 4);
        t.input(b"\x1b[?1;3;16S");
        // register 20 of 16 is register 15
        t.input(&sixel("0;0;0", "#20@"));
        assert_eq!(first_pixel(&t), GRAY);
    }

    // covers: image.sixel
    #[test]
    fn a_cancelled_string_places_nothing_and_gives_its_bytes_back() {
        for end in [&b"\x18"[..], b"\x1a", b"\x1b7"] {
            let mut t = term(12, 4);
            let mut s = solid(16, 32);
            s.truncate(s.len() - 2);
            s.extend_from_slice(end);
            t.input(&s);
            assert!(shown(&t).is_empty(), "{end:?}");
            assert_eq!(t.screen().images().bytes(), 0, "{end:?}");
            // SUB shows its error character, as it does after any sequence
            let col = usize::from(end == b"\x1a");
            assert_eq!(t.screen().cursor(), (col, 0), "{end:?}");
        }
        // sixel switched off while the string streams in
        let mut t = term(12, 4);
        let s = solid(16, 32);
        let (a, b) = s.split_at(s.len() / 2);
        t.input(a);
        sixel_off(&mut t);
        t.input(b);
        assert!(shown(&t).is_empty());
        assert_eq!(t.screen().images().bytes(), 0);
    }

    // covers: image.sixel
    #[test]
    fn a_picture_past_the_budget_is_dropped_and_raster_attributes_refuse_it_first() {
        // 64 by 64 pixels cost 16,384 bytes and 512 beside them
        let mut t = term(12, 4);
        t.screen_mut().set_image_limit(16_000);
        t.input(&solid(64, 64));
        assert!(shown(&t).is_empty());
        assert_eq!(t.screen().images().bytes(), 0);
        // without raster attributes the picture is refused as it grows
        let mut t = term(12, 4);
        t.screen_mut().set_image_limit(16_000);
        t.input(&sixel("0;0;0", &"#1!64~-".repeat(11)));
        assert!(shown(&t).is_empty());
        assert_eq!(t.screen().images().bytes(), 0);
        // one that fits is stored with exactly its cost
        let mut t = term(12, 4);
        t.screen_mut().set_image_limit(16_896);
        t.input(&solid(64, 64));
        assert_eq!(shown(&t).len(), 1);
        assert_eq!(t.screen().images().bytes(), 64 * 64 * 4 + 512);
    }

    // ADR-1100 D5: the decoder's work counts against the input call's
    // bound, so a short string that writes many pixels stops the input
    // call after the run of data that passed the bound, and the rest is
    // taken by the next call, ending in the same state.
    // covers: image.sixel
    #[test]
    fn a_picture_that_writes_past_the_work_bound_stops_the_input_call() {
        // 300 bands of 10,000 columns: 18 million pixels from 2 KiB
        let mut s = sixel("0;0;0", &"#1!10000~-".repeat(300));
        s.extend_from_slice(b"after");
        let mut t = term(12, 4);
        let taken = t.input(&s);
        assert!(taken < s.len(), "took all {taken} bytes");
        assert!(shown(&t).is_empty(), "the string has not ended");
        let mut rest = s.get(taken..).expect("in the stream");
        while !rest.is_empty() {
            let n = t.input(rest);
            rest = rest.get(n..).expect("in the stream");
        }
        // the text is written over the picture, which it cuts
        assert_eq!(rows(t.screen())[3], "after");
        assert!(!shown(&t).is_empty());
    }

    // docs/architecture.md, "Determinism": a sixel stream fed whole, and
    // fed a byte at a time, ends in the same screen and the same store.
    // covers: image.sixel
    #[test]
    fn a_sixel_stream_fed_a_byte_at_a_time_ends_as_fed_whole() {
        let mut s = b"\x1b[2;2H".to_vec();
        s.extend(solid(20, 40));
        s.extend(b"\x1b[?1070l");
        s.extend(sixel("0;1", "#7;1;120;50;100#7??~~$#3NN!5w-#7@"));
        s.extend(b"\x1b[?8452h");
        s.extend(sixel("", "\"1;1;30;20#2~~-~~"));
        s.extend(b"\x1b[?1;1S\x1b[c");
        let mut whole = term(12, 6);
        whole.input(&s);
        let mut bytes = term(12, 6);
        for b in &s {
            bytes.input(core::slice::from_ref(b));
        }
        assert!(whole.screen() == bytes.screen());
        assert_eq!(whole.replies(), bytes.replies());
        let pixels = |t: &Terminal| -> Vec<Vec<u32>> {
            let p = t.screen().placements();
            p.iter().map(|p| p.pixels.as_slice().to_vec()).collect()
        };
        assert_eq!(pixels(&whole), pixels(&bytes));
        assert_eq!(shown(&whole), shown(&bytes));
        // the third picture is opaque and covers the second whole, and the
        // first picture's last row
        assert_eq!(shown(&whole).len(), 2);
    }

    // The C reference's flow (tsm-vte-image.c's vte_sixel_run) took sixel
    // bytes from the stream as they came; vtium never holds the string,
    // only the picture's pixels, so a picture costs its pixels and nothing
    // more, however its string is cut.
    // covers: image.sixel
    #[test]
    fn a_picture_holds_its_pixels_of_the_budget_while_its_string_streams() {
        let mut t = term(12, 4);
        let s = solid(64, 64);
        let (a, b) = s.split_at(s.len() - 4);
        t.input(a);
        // raster attributes reserved the picture's pixels, and no more
        assert_eq!(t.screen().images().bytes(), 64 * 64 * 4);
        t.input(b);
        assert_eq!(t.screen().images().bytes(), 64 * 64 * 4 + 512);
    }
}

/// The decode measurement #48 asks for: each sixel frame named in
/// `VTIUM_SIXEL_FRAMES` (paths, `:` between), a whole DCS `q` string such
/// as `mpv --vo=sixel` writes, fed through a terminal whose screen holds
/// it, with the cursor sent home before each, as mpv sends it. It prints
/// the bytes a second and the time a frame; it asserts nothing. Run it in
/// a release build: `VTIUM_SIXEL_FRAMES=a.six:b.six cargo test --release
/// -p vtium-term --features images --test sixel -- --ignored --nocapture`.
#[cfg(feature = "images")]
#[test]
#[ignore = "a measurement, run by hand"]
#[expect(
    clippy::disallowed_methods,
    reason = "a measurement of time, which never reaches state"
)]
fn sixel_decode_throughput() {
    use vtium_term::ImageString;
    use vtium_term::images::{Sixel, Switches};
    let Some(paths) = std::env::var_os("VTIUM_SIXEL_FRAMES") else {
        eprintln!("VTIUM_SIXEL_FRAMES is not set");
        return;
    };
    for path in std::env::split_paths(&paths) {
        let frame = std::fs::read(&path).expect("a frame");
        // the raster attributes give the picture's size: "1;1;W;H
        let head = String::from_utf8_lossy(&frame[..frame.len().min(64)]).into_owned();
        let size: Vec<usize> = head
            .split('"')
            .nth(1)
            .and_then(|r| r.split('#').next())
            .map(|r| r.split(';').filter_map(|n| n.parse().ok()).collect())
            .unwrap_or_default();
        let (w, h) = (
            size.get(2).copied().unwrap_or(1366),
            size.get(3).copied().unwrap_or(768),
        );
        let mut t = Terminal::new(w.div_ceil(8) + 1, h.div_ceil(16) + 1, 0);
        t.screen_mut().set_image_limit(1 << 31);
        t.screen_mut().set_image_cell(8, 16).expect("a cell");
        t.set_image_switches(Switches {
            sixel: true,
            kitty: false,
            iterm2: false,
        });
        t.set_image_handler(ImageString::Sixel, Some(Box::new(Sixel::default())));
        let mut s = b"\x1b[H".to_vec();
        s.extend_from_slice(&frame);
        // the same frame cancelled by CAN at its end: the decode alone,
        // with nothing stored or placed
        let mut cancelled = s.clone();
        cancelled.truncate(cancelled.len() - 2);
        cancelled.push(0x18);
        // as a host feeds a pty's reads, 64 KiB at a time
        let feed = |t: &mut Terminal, s: &[u8]| {
            for chunk in s.chunks(1 << 16) {
                let mut rest = chunk;
                while !rest.is_empty() {
                    let n = t.input(rest);
                    rest = &rest[n..];
                }
            }
        };
        let frames = 10_u32;
        for (what, s) in [("placed", &s), ("decoded only", &cancelled)] {
            feed(&mut t, s);
            let start = std::time::Instant::now();
            for _ in 0..frames {
                feed(&mut t, s);
            }
            let took = start.elapsed();
            let per = took / frames;
            let mbps = s.len() as f64 * f64::from(frames) / took.as_secs_f64() / 1e6;
            println!(
                "{}: {w}x{h}, {} bytes a frame, {what}: {:.2} ms a frame, {mbps:.0} MB/s, {:.0} frames/s",
                path.display(),
                s.len(),
                per.as_secs_f64() * 1e3,
                1.0 / per.as_secs_f64()
            );
        }
        assert_eq!(t.screen().placements().len(), 1);
    }
}
