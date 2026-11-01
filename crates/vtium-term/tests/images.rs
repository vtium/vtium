//! The image store on every screen event, the router and the barrier
//! (ADR-1100, D5 to D9), driven through the terminal.

#![cfg(feature = "images")]

mod common;

use common::Rng;
use vtium_parse::Chunk;
use vtium_term::images::decode::{Medium, Reply, Request};
use vtium_term::images::{
    Context, FramePlacement, Handler, ImageKey, Intro, NewImage, Place, Protocol, Switches,
};
use vtium_term::{ImageString, Palette, Terminal};

const COLS: usize = 12;
const ROWS: usize = 4;

fn term() -> Terminal {
    let mut t = Terminal::new(COLS, ROWS, 100);
    t.screen_mut().set_image_limit(1 << 30);
    t
}

fn image(t: &mut Terminal, protocol: Protocol, w: u32, h: u32, id: u32) -> ImageKey {
    let pixels = vec![0xff12_3456; usize::try_from(w * h).unwrap_or(0)];
    let new = NewImage {
        protocol,
        width: w,
        height: h,
        pixels,
        id,
        number: 0,
        transient: false,
    };
    t.screen_mut().add_image(new).expect("stored")
}

fn place(t: &mut Terminal, key: ImageKey, col: u32, row: u32) {
    let at = Place {
        col,
        row,
        ..Place::default()
    };
    t.screen_mut().place_image(key, at).expect("placed");
}

/// A sixel and a kitty image of 3 by 2 cells, at columns 1 and 6 of rows
/// 1 and 2.
fn two(t: &mut Terminal) -> (ImageKey, ImageKey) {
    let six = image(t, Protocol::SIXEL, 24, 32, 0);
    let kit = image(t, Protocol::KITTY, 24, 32, 1);
    place(t, six, 1, 1);
    place(t, kit, 6, 1);
    (six, kit)
}

/// The placements as (row, col, cols, rows, kind) tuples, in draw order.
fn shown(t: &Terminal) -> Vec<(i64, u32, u32, u32, &'static str)> {
    t.screen()
        .placements()
        .iter()
        .map(|p| (p.row, p.col, p.cols, p.rows, p.kind))
        .collect()
}

fn sorted(t: &Terminal) -> Vec<(i64, u32, u32, u32, &'static str)> {
    let mut v = shown(t);
    v.sort();
    v
}

// covers: image.pipeline
#[test]
fn text_cuts_a_sixel_and_never_a_kitty_placement() {
    let mut t = term();
    two(&mut t);
    t.input(b"\x1b[2;3Hxy\x1b[2;8Hz");
    assert_eq!(
        sorted(&t),
        [
            (1, 1, 1, 1, "sixel"),
            (1, 6, 3, 2, "kitty"),
            (2, 1, 3, 1, "sixel"),
        ]
    );
}

#[test]
fn each_erase_cuts_a_sixel_and_only_ed_2_removes_a_kitty_placement() {
    for (erase, sixel) in [
        (&b"\x1b[K"[..], vec![(1, 1, 1, 1), (2, 1, 3, 1)]),
        (b"\x1b[1K", vec![(1, 3, 1, 1), (2, 1, 3, 1)]),
        (b"\x1b[2K", vec![(2, 1, 3, 1)]),
        (b"\x1b[X", vec![(1, 1, 1, 1), (1, 3, 1, 1), (2, 1, 3, 1)]),
        (b"\x1b[J", vec![(1, 1, 1, 1)]),
        (b"\x1b[1J", vec![(1, 3, 1, 1), (2, 1, 3, 1)]),
        (b"\x1b[?J", vec![(1, 1, 1, 1)]),
        (b"\x1b[@", vec![(1, 1, 1, 1), (2, 1, 3, 1)]),
        (b"\x1b[P", vec![(1, 1, 1, 1), (2, 1, 3, 1)]),
    ] {
        let mut t = term();
        two(&mut t);
        t.input(b"\x1b[2;3H");
        t.input(erase);
        let got: Vec<(i64, u32, u32, u32)> = sorted(&t)
            .into_iter()
            .filter(|p| p.4 == "sixel")
            .map(|p| (p.0, p.1, p.2, p.3))
            .collect();
        assert_eq!(got, sixel, "{:?}", String::from_utf8_lossy(erase));
        assert!(
            shown(&t).iter().any(|p| p.4 == "kitty"),
            "{erase:?} left the kitty placement"
        );
    }
    let mut t = term();
    two(&mut t);
    t.input(b"\x1b[2J");
    assert_eq!(shown(&t), []);
    assert_eq!(
        t.screen().images().image_count(),
        1,
        "the kitty image keeps its data"
    );
}

#[test]
fn a_rectangle_erase_fill_or_copy_cuts_a_sixel() {
    for op in [
        &b"\x1b[2;2;2;2$z"[..],
        b"\x1b[65;2;2;2;2$x",
        b"\x1b[1;1;1;12;1;2;2;1$v",
    ] {
        let mut t = term();
        two(&mut t);
        t.input(op);
        let got = shown(&t);
        assert!(
            !got.contains(&(1, 1, 3, 2, "sixel")),
            "{:?} cut nothing",
            String::from_utf8_lossy(op)
        );
        assert!(
            got.contains(&(1, 6, 3, 2, "kitty")),
            "{:?}: {got:?}",
            String::from_utf8_lossy(op)
        );
    }
}

#[test]
fn a_full_screen_scroll_carries_placements_into_the_scrollback_and_back() {
    let mut t = term();
    t.screen_mut().set_scrollback_max(10);
    two(&mut t);
    // four lines scroll: the pictures' rows are 3 and 2 above the screen
    t.input(b"\x1b[4;1H\n\x1bD\x1b[2S");
    assert_eq!(shown(&t), []);
    t.screen_mut().scroll_view_up(3);
    assert_eq!(sorted(&t), [(0, 1, 3, 2, "sixel"), (0, 6, 3, 2, "kitty")]);
}

#[test]
fn a_region_scroll_splits_a_sixel_and_moves_a_kitty_placement_only_whole() {
    let mut t = term();
    two(&mut t);
    // rows 2 and 3 scroll up one: the sixel's lower row leaves the band's
    // top, the kitty placement crosses the band's top edge and stays
    t.input(b"\x1b[3;4r\x1b[4;1H\n");
    assert_eq!(sorted(&t), [(1, 1, 3, 1, "sixel"), (1, 6, 3, 2, "kitty")]);
    // IL in the band moves a placement wholly inside it
    let mut t = term();
    let k = image(&mut t, Protocol::KITTY, 8, 16, 3);
    place(&mut t, k, 0, 2);
    t.input(b"\x1b[2;1H\x1b[L");
    assert_eq!(shown(&t), [(3, 0, 1, 1, "kitty")]);
    // DL at its row deletes it, clipped to nothing
    t.input(b"\x1b[4;1H\x1b[M");
    assert_eq!(shown(&t), []);
}

#[test]
fn dl_at_the_first_row_clips_what_it_deletes() {
    let mut t = term();
    t.screen_mut().set_scrollback_max(10);
    let short = image(&mut t, Protocol::KITTY, 8, 16, 1);
    let tall = image(&mut t, Protocol::KITTY, 8, 32, 2);
    place(&mut t, short, 0, 0);
    place(&mut t, tall, 2, 0);
    t.input(b"\x1b[H\x1b[M");
    // the short picture went with its line; the tall one lost its top row
    // and moved up with the rest of the region
    assert_eq!(shown(&t), [(0, 2, 1, 1, "kitty")]);
    assert_eq!(
        t.screen().scrollback().len(),
        0,
        "DL put nothing in the scrollback"
    );
}

#[test]
fn decsed_2_removes_no_kitty_placement() {
    let mut t = term();
    two(&mut t);
    t.input(b"\x1b[?2J");
    assert_eq!(
        shown(&t),
        [(1, 6, 3, 2, "kitty")],
        "the selective erase cuts the sixel only"
    );
}

#[test]
fn printing_in_insert_mode_cuts_a_sixel_to_the_rows_end() {
    let mut t = term();
    let six = image(&mut t, Protocol::SIXEL, 80, 16, 0);
    place(&mut t, six, 1, 1);
    t.input(b"\x1b[4h\x1b[2;3Hx");
    assert_eq!(shown(&t), [(1, 1, 1, 1, "sixel")]);
}

#[test]
fn decaln_and_a_vs16_widening_cut_a_sixel() {
    let mut t = term();
    two(&mut t);
    t.input(b"\x1b#8");
    assert_eq!(shown(&t), [(1, 6, 3, 2, "kitty")]);
    let mut t = term();
    let six = image(&mut t, Protocol::SIXEL, 24, 16, 0);
    place(&mut t, six, 1, 0);
    // a heart and a VS16 at column 0 widen into column 1
    t.input("\u{2764}\u{fe0f}".as_bytes());
    assert_eq!(shown(&t), [(0, 2, 2, 1, "sixel")]);
}

#[test]
fn a_smaller_scrollback_clips_the_placements_on_the_lines_it_drops() {
    let mut t = term();
    t.screen_mut().set_scrollback_max(10);
    let k = image(&mut t, Protocol::KITTY, 8, 64, 1);
    place(&mut t, k, 0, 0);
    t.input(b"\x1b[4;1H\n\n");
    t.screen_mut().set_scrollback_max(1);
    t.screen_mut().scroll_view_up(1);
    assert_eq!(
        shown(&t),
        [(0, 0, 1, 3, "kitty")],
        "it lost the top row that went"
    );
}

#[test]
fn a_taller_screen_brings_a_scrollback_placement_back() {
    let mut t = term();
    t.screen_mut().set_scrollback_max(10);
    let k = image(&mut t, Protocol::KITTY, 8, 16, 1);
    place(&mut t, k, 3, 0);
    t.input(b"\x1b[4;1H");
    t.resize(COLS, 2);
    assert_eq!(shown(&t), []);
    t.resize(COLS, 4);
    assert_eq!(shown(&t), [(0, 3, 1, 1, "kitty")]);
    assert_eq!(t.screen().images().placement_count(), 1);
}

#[test]
fn the_charges_of_cut_pieces_settle_per_event_however_the_input_is_cut() {
    // a sixel 7 cells wide, cut twice in one stream under a budget with no
    // room for a piece's charge: each cut's smallest piece goes when that
    // cut ends, so which piece stays follows the events and not the input
    // call they fell in
    let stream = b"\x1b[1;5Hx\x1b[1;2Hy";
    let fresh = || {
        let mut t = term();
        t.screen_mut().set_image_limit(56 * 16 * 4 + 512);
        let six = image(&mut t, Protocol::SIXEL, 56, 16, 0);
        place(&mut t, six, 0, 0);
        t
    };
    let mut whole = fresh();
    whole.input(stream);
    let mut cut = fresh();
    cut.input(&stream[..7]);
    cut.input(&stream[7..]);
    assert_eq!(cut.screen().images(), whole.screen().images());
    // the first cut's right piece went, then the second cut's left one
    assert_eq!(shown(&whole), [(0, 2, 2, 1, "sixel")]);
}

#[test]
fn a_line_dropped_from_the_scrollback_clips_the_placements_on_it() {
    let mut t = term();
    t.screen_mut().set_scrollback_max(2);
    let k = image(&mut t, Protocol::KITTY, 8, 64, 1);
    place(&mut t, k, 0, 0);
    // the placement's top line goes 3 lines into a scrollback of 2
    t.input(b"\x1b[4;1H\n\n\n");
    t.screen_mut().scroll_view_up(2);
    assert_eq!(shown(&t), [(0, 0, 1, 3, "kitty")], "it lost its top row");
}

#[test]
fn ed_3_drops_the_placements_anchored_in_the_scrollback() {
    let mut t = term();
    t.screen_mut().set_scrollback_max(10);
    two(&mut t);
    t.input(b"\x1b[4;1H\n\n\x1b[3J");
    assert_eq!(shown(&t), []);
}

#[test]
fn the_alternate_screen_empties_its_buffer_both_ways_and_keeps_the_main_ones() {
    let mut t = term();
    two(&mut t);
    t.input(b"\x1b[?1049h");
    assert_eq!(shown(&t), []);
    let k = image(&mut t, Protocol::KITTY, 8, 16, 1);
    place(&mut t, k, 0, 0);
    assert_eq!(shown(&t), [(0, 0, 1, 1, "kitty")]);
    t.input(b"\x1b[?1049l");
    assert_eq!(sorted(&t), [(1, 1, 3, 2, "sixel"), (1, 6, 3, 2, "kitty")]);
    assert_eq!(
        t.screen().images().image_count(),
        2,
        "the alternate buffer was emptied"
    );
    t.input(b"\x1bc");
    assert_eq!(t.screen().images().image_count(), 0, "RIS empties both");
}

// covers: image.pipeline
#[test]
fn a_rewrap_carries_a_placement_with_its_lines_text() {
    let mut t = Terminal::new(10, 4, 100);
    t.screen_mut().set_image_limit(1 << 30);
    // fifteen characters wrap onto row 1; the placement sits on the
    // thirteenth, at row 1, column 2
    t.input(b"abcdefghijklmno");
    let k = image(&mut t, Protocol::KITTY, 8, 16, 1);
    place(&mut t, k, 2, 1);
    t.resize(20, 4);
    assert_eq!(shown(&t), [(0, 12, 1, 1, "kitty")]);
    t.resize(10, 4);
    assert_eq!(shown(&t), [(1, 2, 1, 1, "kitty")]);
}

/// Every placement the view shows at each offset back through the
/// scrollback, as (offset, row, col, kind) tuples.
fn at_every_offset(t: &mut Terminal) -> Vec<(usize, i64, u32, &'static str)> {
    let depth = t.screen().scrollback().len();
    let mut out = Vec::new();
    for offset in 0..=depth {
        t.screen_mut().reset_view();
        t.screen_mut().scroll_view_up(offset);
        out.extend(
            t.screen()
                .placements()
                .iter()
                .map(|p| (offset, p.row, p.col, p.kind)),
        );
    }
    t.screen_mut().reset_view();
    out
}

// covers: image.pipeline
#[test]
fn a_rewrap_carries_the_anchors_in_the_scrollback_lazily_as_eagerly() {
    // a placement on a wrapped line's continuation row, and one whose
    // column a narrower width moves to the line's next row, both pushed
    // deep into the scrollback before the width changes
    for (to, text, col, row) in [
        (20, &b"abcdefghijklmno"[..], 2, 1),
        (5, b"abcdefghij", 8, 0),
    ] {
        // eagerly, the view at the scrollback's top takes every row into
        // the resize's rewrap
        let fresh = |eager: bool| {
            let mut t = Terminal::new(10, 4, 100);
            t.screen_mut().set_image_limit(1 << 30);
            t.screen_mut().set_scrollback_max(100);
            t.input(text);
            let k = image(&mut t, Protocol::SIXEL, 8, 16, 0);
            place(&mut t, k, col, row);
            t.input(&[b'\n'; 20]);
            if eager {
                t.screen_mut().scroll_view_up(100);
            }
            t.resize(to, 4);
            t.screen_mut().reset_view();
            t
        };
        let mut lazy = fresh(false);
        let mut eager = fresh(true);
        let lazily = at_every_offset(&mut lazy);
        let eagerly = at_every_offset(&mut eager);
        assert!(
            !eagerly.is_empty(),
            "to {to}: the placement shows somewhere"
        );
        assert_eq!(
            lazily, eagerly,
            "to {to}: lazy and eager rewraps place it alike"
        );
        assert_eq!(lazy.screen().images().image_count(), 1);
    }
}

#[test]
fn a_frame_shares_its_images_pixels() {
    let mut t = term();
    two(&mut t);
    let palette = Palette::default();
    let frame = t.screen().frame(&palette, vtium_term::Age::NEVER);
    let first: Vec<FramePlacement> = frame.placements().to_vec();
    let again = t.screen().placements();
    assert_eq!(first.len(), 2);
    for (a, b) in first.iter().zip(&again) {
        assert!(
            a.pixels.same(&b.pixels),
            "the pixels are shared, never copied"
        );
    }
}

/// A stand-in handler: each string it finishes places a 1x1 picture, a
/// sixel or with `kitty` a kitty picture, at the cursor, after a request to
/// the helper when `ask`.
#[derive(Clone, Debug, PartialEq)]
struct Stand {
    kitty: bool,
    ask: bool,
    id: u32,
}

impl Stand {
    fn show(&mut self, ctx: &mut dyn Context) {
        let (col, row) = ctx.cursor();
        let new = NewImage {
            protocol: if self.kitty {
                Protocol::KITTY
            } else {
                Protocol::SIXEL
            },
            width: 8,
            height: 16,
            pixels: vec![0xffff_ffff; 128],
            id: self.id,
            number: 0,
            transient: false,
        };
        if let Ok(key) = ctx.add_image(new, None) {
            let at = Place {
                col: u32::try_from(col).unwrap_or(0),
                row: u32::try_from(row).unwrap_or(0),
                ..Place::default()
            };
            let _ = ctx.place(key, at);
        }
        if self.id != 0 {
            self.id = self.id.saturating_add(1);
        }
    }
}

impl Handler for Stand {
    fn start(&mut self, _: &mut dyn Context, _: Intro<'_>) {}
    fn feed(&mut self, _: &mut dyn Context, _: Chunk<'_>) {}
    fn finish(&mut self, ctx: &mut dyn Context) {
        if self.ask {
            ctx.request(Request {
                medium: Medium::Direct(vec![1]),
                offset: 0,
                max: 0,
                min: 0,
                inflate: true,
                png: false,
                exact: false,
                discard: false,
                largest: (10_000, 10_000),
                largest_bytes: 1 << 30,
            });
        } else {
            self.show(ctx);
        }
    }
    fn cancel(&mut self, _: &mut dyn Context) {}
    fn decoded(&mut self, ctx: &mut dyn Context, _: Reply) {
        self.show(ctx);
    }
    fn clone_box(&self) -> Box<dyn Handler> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn same(&self, other: &dyn Handler) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
}

fn routed(ask: bool) -> Terminal {
    let mut t = term();
    t.set_image_switches(Switches {
        sixel: true,
        kitty: true,
        iterm2: false,
    });
    t.set_image_helper(true);
    let six = Stand {
        kitty: false,
        ask: false,
        id: 0,
    };
    let kit = Stand {
        kitty: true,
        ask,
        id: 1,
    };
    t.set_image_handler(ImageString::Sixel, Some(Box::new(six)));
    t.set_image_handler(ImageString::Kitty, Some(Box::new(kit)));
    t
}

/// Feeds `stream` in the pieces `cuts` makes, answering each request as
/// a host does, and returns the bytes each input call took.
fn feed(t: &mut Terminal, stream: &[u8], cuts: &[usize]) -> Vec<usize> {
    let mut taken = Vec::new();
    let mut from = 0;
    for &cut in cuts.iter().chain([&stream.len()]) {
        let mut piece = &stream[from..cut];
        while !piece.is_empty() {
            let n = t.input(piece);
            taken.push(n);
            if t.take_image_request().is_some() {
                t.image_reply(Reply::Ok(vtium_term::images::decode::Payload::Bytes(
                    Vec::new(),
                )));
            }
            piece = &piece[n..];
        }
        from = cut;
    }
    taken
}

// covers: image.pipeline
#[test]
fn strings_land_in_stream_order_across_a_pending_decode() {
    // a kitty picture whose decode goes to the helper, then a sixel, both
    // at the screen's first cell
    let stream = b"\x1b_Gf=100;x\x1b\\\x1bPq#0~\x1b\\";
    let mut t = routed(true);
    let n = t.input(stream);
    assert_eq!(n, 12, "the input call stops after the kitty string's ST");
    assert_eq!(
        t.input(&stream[n..]),
        0,
        "and takes nothing while the request is out"
    );
    assert!(t.take_image_request().is_some());
    assert_eq!(t.take_image_request(), None, "a request is taken once");
    t.image_reply(Reply::Corrupt);
    assert_eq!(t.input(&stream[n..]), stream.len() - n);
    // the kitty picture arrived first, so the sixel draws over it
    assert_eq!(shown(&t), [(0, 0, 1, 1, "kitty"), (0, 0, 1, 1, "sixel")]);
}

#[test]
fn an_off_protocol_or_a_cancelled_string_places_nothing() {
    let mut t = routed(false);
    t.set_image_switches(Switches {
        sixel: false,
        kitty: true,
        iterm2: false,
    });
    t.input(b"\x1bPq#0~\x1b\\");
    assert_eq!(shown(&t), []);
    t.input(b"\x1b_Gx\x18");
    assert_eq!(shown(&t), [], "CAN cancels the kitty string");
    t.input(b"\x1b_Hx\x1b\\");
    assert_eq!(
        shown(&t),
        [],
        "an APC string not starting with G is not kitty's"
    );
    t.input(b"\x1b_Gx\x1b\\");
    assert_eq!(shown(&t), [(0, 0, 1, 1, "kitty")]);
}

#[test]
fn a_protocol_switched_off_or_a_budget_of_0_mid_string_stores_nothing() {
    for off in [
        |t: &mut Terminal| {
            t.set_image_switches(Switches {
                sixel: false,
                kitty: true,
                iterm2: false,
            })
        },
        |t: &mut Terminal| t.screen_mut().set_image_limit(0),
    ] {
        let mut t = routed(false);
        t.input(b"\x1bPq#0~");
        off(&mut t);
        t.input(b"~\x1b\\");
        assert_eq!(shown(&t), []);
        assert_eq!(t.screen().images().image_count(), 0);
    }
    // a budget of 0 offers nothing from the string's start either
    let mut t = routed(false);
    t.screen_mut().set_image_limit(0);
    t.input(b"\x1b_Gx\x1b\\");
    assert_eq!(t.screen().images().image_count(), 0);
}

/// A handler that reserves a cell's bytes when a string starts and gives
/// them back when it is cancelled.
#[derive(Clone, Debug, Default, PartialEq)]
struct Reserving(Option<vtium_term::images::Reservation>);

impl Handler for Reserving {
    fn start(&mut self, ctx: &mut dyn Context, _: Intro<'_>) {
        self.0 = ctx.reserve(Protocol::SIXEL, 0, 4096).ok();
    }
    fn feed(&mut self, _: &mut dyn Context, _: Chunk<'_>) {}
    fn finish(&mut self, _: &mut dyn Context) {}
    fn cancel(&mut self, ctx: &mut dyn Context) {
        if let Some(r) = self.0.take() {
            ctx.unreserve(r);
        }
    }
    fn decoded(&mut self, _: &mut dyn Context, _: Reply) {}
    fn clone_box(&self) -> Box<dyn Handler> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn same(&self, other: &dyn Handler) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
}

#[test]
fn a_handler_replaced_mid_string_is_cancelled_and_gives_its_bytes_back() {
    let mut t = routed(false);
    t.set_image_handler(ImageString::Sixel, Some(Box::new(Reserving::default())));
    t.input(b"\x1bPq#0~");
    assert_eq!(t.screen().images().bytes(), 4096);
    t.set_image_handler(ImageString::Sixel, None);
    assert_eq!(t.screen().images().bytes(), 0, "the reservation went back");
    t.input(b"~\x1b\\");
    assert_eq!(shown(&t), []);
}

// covers: image.pipeline
#[test]
fn the_store_follows_the_bytes_whole_or_cut_and_whatever_was_drawn() {
    let mut rng = Rng(7);
    let mut stream = Vec::new();
    for i in 0..200 {
        match rng.below(6) {
            0 | 1 => stream.extend_from_slice(b"\x1b_Gx\x1b\\"),
            2 => stream.extend_from_slice(b"\x1bPq#0~\x1b\\"),
            3 => stream.extend_from_slice(b"\n\n"),
            4 => stream.extend_from_slice(b"\x1b[2J\x1b[H"),
            _ => {
                stream.extend_from_slice(format!("\x1b[{};{}Hab", i % 4 + 1, i % 9 + 1).as_bytes())
            }
        }
    }
    let fresh = || {
        let mut t = routed(false);
        // a budget for a few pictures, so eviction and refusal take part
        t.screen_mut().set_image_limit(6 * (8 * 16 * 4 + 512));
        t.screen_mut().set_scrollback_max(5);
        t
    };
    let mut whole = fresh();
    feed(&mut whole, &stream, &[]);
    for seed in 0..20 {
        let mut rng = Rng(seed);
        let mut cuts: Vec<usize> = (0..1 + rng.below(8))
            .map(|_| rng.below(stream.len() + 1))
            .collect();
        cuts.sort_unstable();
        let mut cut = fresh();
        let mut from = 0;
        let palette = Palette::default();
        for &c in &cuts {
            feed(&mut cut, &stream[from..c], &[]);
            // a frame drawn here moves nothing the store keeps
            let _ = cut.screen().frame(&palette, vtium_term::Age::NEVER);
            from = c;
        }
        feed(&mut cut, &stream[from..], &[]);
        assert_eq!(
            cut.screen().images(),
            whole.screen().images(),
            "seed {seed}: cut at {cuts:?} the store differs"
        );
        assert_eq!(cut.screen().placements(), whole.screen().placements());
    }
}

#[test]
fn scans_against_a_full_store_stop_the_input_call_early() {
    let mut t = routed(false);
    let k = image(&mut t, Protocol::KITTY, 1, 1, 9);
    // 4000 placements on the last row, below a scroll region of the rows
    // above it: every line feed's scroll walks the screen's rows and what
    // is anchored there
    for _ in 0..4000 {
        place(&mut t, k, 11, 3);
    }
    t.input(b"\x1b[1;3r");
    let lfs = vec![b'\n'; 10_000];
    let taken = t.input(&lfs);
    assert!(
        taken < lfs.len(),
        "the scrolls' scans passed the work bound and stopped the call"
    );
    assert!(taken > 0);
    let mut rest = &lfs[taken..];
    while !rest.is_empty() {
        let n = t.input(rest);
        assert!(n > 0, "the next call goes on");
        rest = &rest[n..];
    }
}
