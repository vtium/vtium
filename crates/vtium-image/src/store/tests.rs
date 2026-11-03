//! The store's own tests: the budget, the eviction order, the stack at one
//! depth, and the pieces a cut makes. vtium-term's tests drive the screen
//! events through a real screen.

#![expect(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    reason = "a test fails by panicking"
)]

use super::*;
use crate::rules::Protocol;

/// A screen of `rows` lines, keys 0 up, and no scrollback.
struct Fake {
    screen: Vec<LineKey>,
}

impl Fake {
    fn new(rows: u64) -> Self {
        Self {
            screen: (0..rows).map(LineKey::new).collect(),
        }
    }

    fn rows(&self) -> Rows<'_> {
        Rows {
            screen: &self.screen,
            above: &[],
        }
    }

    fn screens(&self) -> Screens<'_> {
        Screens {
            main: self.rows(),
            alt: Rows::default(),
        }
    }
}

const COLS: usize = 20;

fn store(limit: u64) -> Store {
    let mut s = Store::new();
    s.set_limit(limit);
    s
}

fn image(protocol: Protocol, w: u32, h: u32, id: u32) -> NewImage {
    NewImage {
        protocol,
        width: w,
        height: h,
        pixels: vec![0xff00_00ff; (w * h) as usize],
        id,
        number: 0,
        transient: false,
        opaque: None,
    }
}

fn add(s: &mut Store, f: &Fake, protocol: Protocol, w: u32, h: u32, id: u32) -> ImageKey {
    s.add_image(image(protocol, w, h, id), None, f.screens())
        .expect("stored")
}

fn put(s: &mut Store, f: &Fake, img: ImageKey, col: u32, row: u32) -> PlacementKey {
    let place = Place {
        col,
        row,
        ..Place::default()
    };
    s.place(img, place, f.screens(), COLS).expect("placed")
}

/// The frame's placements as (row, col, cols, rows, image) tuples.
/// The height counts each buffer keeps, recomputed from its placements:
/// a piece counted wrong fails here.
fn check_heights(s: &Store) {
    for buf in &s.buffers {
        let mut all: BTreeMap<u32, u32> = BTreeMap::new();
        let mut cut: BTreeMap<u32, u32> = BTreeMap::new();
        // only a placement on a line is counted
        for p in buf
            .placements
            .values()
            .filter(|p| matches!(p.spot, Spot::Line(_)))
        {
            *all.entry(p.rows).or_insert(0) += 1;
            if rules(p.protocol.0).cut_by_text {
                *cut.entry(p.rows).or_insert(0) += 1;
            }
        }
        assert_eq!(buf.heights, all, "the heights of every placement");
        assert_eq!(buf.cut_heights, cut, "the heights of those text cuts");
    }
}

fn shown(s: &Store, f: &Fake) -> Vec<(i64, u32, u32, u32, ImageKey)> {
    check_heights(s);
    s.frame(f.rows(), COLS)
        .iter()
        .map(|p| (p.row, p.col, p.cols, p.rows, p.image))
        .collect()
}

/// An 8x16 image's cost.
const ONE_CELL: u64 = 8 * 16 * 4 + IMAGE_OVERHEAD;

// covers: image.pipeline
#[test]
fn a_reservation_eviction_could_not_cover_evicts_nothing_and_is_refused() {
    let f = Fake::new(4);
    let mut s = store(3 * ONE_CELL);
    let a = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    // a sixel placed on the screen is never evicted
    let six = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    put(&mut s, &f, six, 0, 0);
    // four cells' worth more cannot fit even with the kitty image gone
    let big = image(Protocol::KITTY, 8, 64, 2);
    assert_eq!(s.add_image(big, None, f.screens()), Err(Refusal::NoSpace));
    assert_eq!(
        s.image_count(),
        2,
        "nothing was evicted for a request that could not fit"
    );
    assert!(s.image(MAIN, a).is_some());
    // one that fits once the unplaced kitty image goes evicts it
    let b = add(&mut s, &f, Protocol::KITTY, 8, 32, 3);
    assert!(s.image(MAIN, a).is_none());
    assert!(s.image(MAIN, b).is_some());
    assert_eq!(s.bytes(), ONE_CELL + 8 * 32 * 4 + IMAGE_OVERHEAD);
}

#[test]
fn a_refused_kitty_replacement_keeps_the_old_image() {
    let f = Fake::new(4);
    let mut s = store(2 * ONE_CELL);
    let old = add(&mut s, &f, Protocol::KITTY, 8, 16, 7);
    let p = put(&mut s, &f, old, 2, 1);
    let bigger = image(Protocol::KITTY, 8, 64, 7);
    assert_eq!(
        s.add_image(bigger, None, f.screens()),
        Err(Refusal::NoSpace)
    );
    assert!(s.image(MAIN, old).is_some());
    assert!(s.placement(MAIN, p).is_some());
    // a replacement that fits takes the old one's bytes and placements
    let new = add(&mut s, &f, Protocol::KITTY, 8, 32, 7);
    assert!(s.image(MAIN, old).is_none());
    assert_eq!(s.placement_count(), 0);
    // and the old one's pixels stay as the spare buffer, charged
    assert_eq!(s.bytes(), 8 * 32 * 4 + IMAGE_OVERHEAD + 8 * 16 * 4);
    assert_eq!(s.buffers[MAIN].find_id(7), Some(new));
}

#[test]
fn images_go_in_the_eviction_order_rank_by_rank() {
    let mut f = Fake::new(2);
    let mut s = store(1 << 30);
    let placed_kitty_old = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    let placed_kitty_new = add(&mut s, &f, Protocol::KITTY, 8, 16, 2);
    let in_scrollback = add(&mut s, &f, Protocol::KITTY, 8, 16, 3);
    let unplaced = add(&mut s, &f, Protocol::KITTY, 8, 16, 4);
    let transient = s
        .add_image(
            NewImage {
                transient: true,
                opaque: None,
                ..image(Protocol::KITTY, 8, 16, 5)
            },
            None,
            f.screens(),
        )
        .unwrap();
    let sixel = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    // placed on line 0, which then leaves the screen for the scrollback
    put(&mut s, &f, in_scrollback, 0, 0);
    put(&mut s, &f, placed_kitty_new, 1, 1);
    put(&mut s, &f, placed_kitty_old, 2, 1);
    put(&mut s, &f, sixel, 3, 1);
    let sb = vec![f.screen.remove(0)];
    f.screen.push(LineKey::new(100));
    let screens = Screens {
        main: Rows {
            screen: &f.screen,
            above: &[],
        },
        alt: Rows::default(),
    };
    let _ = &sb;
    let mut order = Vec::new();
    while let Some((_, k)) = s.victim(None, screens) {
        order.push(k);
        s.free_image(MAIN, k);
    }
    // unplaced (transient first), then placed only in the scrollback, then
    // kitty images on the screen oldest transmission first; the sixel on
    // the screen never
    assert_eq!(
        order,
        [
            transient,
            unplaced,
            in_scrollback,
            placed_kitty_old,
            placed_kitty_new
        ]
    );
    assert!(s.image(MAIN, sixel).is_some());
}

#[test]
fn a_count_bound_evicts_from_its_own_buffer_alone() {
    let f = Fake::new(2);
    let mut s = store(1 << 40);
    s.set_alternate(true);
    let alt_image = add(&mut s, &f, Protocol::KITTY, 1, 1, 9);
    s.alt = false;
    let first = add(&mut s, &f, Protocol::KITTY, 1, 1, 1);
    for id in 2..=IMAGES_PER_BUFFER as u32 {
        add(&mut s, &f, Protocol::KITTY, 1, 1, id);
    }
    assert!(s.image(MAIN, first).is_some());
    add(&mut s, &f, Protocol::KITTY, 1, 1, 5000);
    assert!(
        s.image(MAIN, first).is_none(),
        "the main buffer's oldest went"
    );
    assert!(
        s.image(ALT, alt_image).is_some(),
        "the alternate buffer's image stayed"
    );
    assert_eq!(s.buffers[MAIN].images.len() as u64, IMAGES_PER_BUFFER);
}

// covers: image.pipeline
#[test]
fn kitty_two_a_sixel_then_kitty_one_draw_one_two_sixel() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let two = add(&mut s, &f, Protocol::KITTY, 8, 16, 2);
    let chart = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    let one = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    put(&mut s, &f, two, 0, 0);
    put(&mut s, &f, chart, 0, 0);
    put(&mut s, &f, one, 0, 0);
    let images: Vec<ImageKey> = shown(&s, &f).iter().map(|p| p.4).collect();
    assert_eq!(
        images,
        [one, two, chart],
        "from the back: kitty 1, kitty 2, the sixel"
    );
}

#[test]
fn a_later_picture_goes_on_top_whatever_its_protocol() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let chart = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    let kit = add(&mut s, &f, Protocol::KITTY, 8, 16, 3);
    put(&mut s, &f, chart, 0, 0);
    put(&mut s, &f, kit, 0, 0);
    let images: Vec<ImageKey> = shown(&s, &f).iter().map(|p| p.4).collect();
    assert_eq!(images, [chart, kit]);
}

#[test]
fn a_kitty_picture_with_no_id_stacks_like_a_sixel() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let five = add(&mut s, &f, Protocol::KITTY, 8, 16, 5);
    let unnamed = add(&mut s, &f, Protocol::KITTY, 8, 16, 0);
    let three = add(&mut s, &f, Protocol::KITTY, 8, 16, 3);
    put(&mut s, &f, five, 0, 0);
    put(&mut s, &f, unnamed, 0, 0);
    put(&mut s, &f, three, 0, 0);
    let images: Vec<ImageKey> = shown(&s, &f).iter().map(|p| p.4).collect();
    assert_eq!(images, [three, five, unnamed]);
}

#[test]
fn a_replacement_is_a_new_arrival_in_its_stack() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let one = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    let chart = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    let place = Place {
        id: 4,
        ..Place::default()
    };
    let p = s.place(one, place, f.screens(), COLS).unwrap();
    put(&mut s, &f, chart, 0, 0);
    let again = s.place(
        one,
        Place {
            id: 4,
            col: 1,
            ..Place::default()
        },
        f.screens(),
        COLS,
    );
    assert_eq!(
        again,
        Ok(p),
        "the same image and placement id replace the placement"
    );
    assert_eq!(s.placement_count(), 2);
    let images: Vec<(u32, ImageKey)> = shown(&s, &f).iter().map(|p| (p.1, p.4)).collect();
    assert_eq!(
        images,
        [(0, chart), (1, one)],
        "the replacement draws on top"
    );
}

#[test]
fn layers_and_z_order_placements_before_their_stack() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let a = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    let b = add(&mut s, &f, Protocol::KITTY, 8, 16, 2);
    let c = add(&mut s, &f, Protocol::KITTY, 8, 16, 3);
    let at = |z| Place {
        z,
        ..Place::default()
    };
    s.place(a, at(5), f.screens(), COLS).unwrap();
    s.place(b, at(-1_073_741_825), f.screens(), COLS).unwrap();
    s.place(c, at(-3), f.screens(), COLS).unwrap();
    let frame = s.frame(f.rows(), COLS);
    let order: Vec<(ImageKey, Layer)> = frame.iter().map(|p| (p.image, p.layer)).collect();
    assert_eq!(
        order,
        [
            (b, Layer::BelowBackground),
            (c, Layer::BelowText),
            (a, Layer::AboveText)
        ]
    );
}

// covers: image.pipeline
#[test]
fn a_cut_through_a_sixel_leaves_pieces_above_left_right_below() {
    let f = Fake::new(6);
    let mut s = store(1 << 30);
    // 5 cells by 3 rows
    let six = add(&mut s, &f, Protocol::SIXEL, 40, 48, 0);
    let p = put(&mut s, &f, six, 2, 1);
    s.cut(2, 4, 4, f.rows(), COLS);
    let pieces: Vec<(PlacementKey, i64, u32, u32, u32)> = s
        .frame(f.rows(), COLS)
        .iter()
        .map(|q| (q.key, q.row, q.col, q.cols, q.rows))
        .collect();
    assert_eq!(pieces.len(), 4);
    assert_eq!(pieces[0], (p, 1, 2, 5, 1), "the piece above keeps the key");
    let rest: Vec<(i64, u32, u32, u32)> = pieces.iter().map(|q| (q.1, q.2, q.3, q.4)).collect();
    assert_eq!(
        rest,
        [(1, 2, 5, 1), (2, 2, 2, 1), (2, 5, 2, 1), (3, 2, 5, 1)]
    );
    let keys: Vec<PlacementKey> = pieces.iter().map(|q| q.0).collect();
    assert!(
        keys.windows(2).all(|w| w[0] < w[1]),
        "keys in the order the pieces were made"
    );
    // the pieces show the parts of the picture under them
    let srcs: Vec<(u32, u32, u32, u32)> = s
        .frame(f.rows(), COLS)
        .iter()
        .map(|q| (q.src_x, q.src_y, q.src_width, q.src_height))
        .collect();
    assert_eq!(
        srcs,
        [
            (0, 0, 40, 16),
            (0, 16, 16, 16),
            (24, 16, 16, 16),
            (0, 32, 40, 16)
        ]
    );
    assert_eq!(
        s.bytes(),
        40 * 48 * 4 + IMAGE_OVERHEAD + 3 * PLACEMENT_OVERHEAD
    );
}

#[test]
fn text_never_cuts_a_kitty_placement() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let k = add(&mut s, &f, Protocol::KITTY, 24, 32, 1);
    put(&mut s, &f, k, 0, 0);
    s.cut(0, 0, 10, f.rows(), COLS);
    assert_eq!(shown(&s, &f), [(0, 0, 3, 2, k)]);
}

#[test]
fn a_cut_past_the_placement_count_keeps_the_largest_pieces_largest_first() {
    let f = Fake::new(8);
    let mut s = store(1 << 40);
    // fill the buffer with 1x1 kitty placements to one short of its count
    let filler = add(&mut s, &f, Protocol::KITTY, 1, 1, 1);
    for _ in 0..PLACEMENTS_PER_BUFFER - 1 {
        put(&mut s, &f, filler, 19, 7);
    }
    s.settle_event(f.screens());
    // a sixel 5 by 5 cells: a cut at its middle cell makes four pieces,
    // room for two
    let six = add(&mut s, &f, Protocol::SIXEL, 40, 80, 0);
    let p = put(&mut s, &f, six, 0, 0);
    assert_eq!(s.placement_count() as u64, PLACEMENTS_PER_BUFFER);
    s.cut(1, 3, 3, f.rows(), COLS);
    let pieces: Vec<(PlacementKey, u32, u32, u32)> = s
        .frame(f.rows(), COLS)
        .iter()
        .filter(|q| q.image == six)
        .map(|q| (q.key, q.col, q.cols, q.rows))
        .collect();
    // below (5x3, 15 cells) and left (3x1) stay; above (5x1) and right
    // (1x1) go; the largest keeps the key and its place
    assert_eq!(pieces.len(), 1, "{pieces:?}");
    assert_eq!(pieces[0], (p, 0, 5, 3));
}

#[test]
fn pieces_the_budget_refuses_go_smallest_first() {
    let f = Fake::new(6);
    let six_cost = 40 * 48 * 4 + IMAGE_OVERHEAD;
    // room for the sixel and two pieces' charges, not three
    let mut s = store(six_cost + 2 * PLACEMENT_OVERHEAD);
    let six = add(&mut s, &f, Protocol::SIXEL, 40, 48, 0);
    let p = put(&mut s, &f, six, 2, 1);
    s.cut(2, 4, 4, f.rows(), COLS);
    assert_eq!(s.placement_count(), 4);
    s.settle_event(f.screens());
    let left: Vec<(PlacementKey, u32, u32)> = s
        .frame(f.rows(), COLS)
        .iter()
        .map(|q| (q.key, q.cols, q.rows))
        .collect();
    // the smallest piece, the 2x1 on the left, went
    assert_eq!(left.len(), 3, "{left:?}");
    assert_eq!(left[0], (p, 5, 1));
    assert_eq!(
        (left[1].1, left[2].1),
        (2, 5),
        "the right piece and the one below stay"
    );
    assert_eq!(s.bytes(), six_cost + 2 * PLACEMENT_OVERHEAD);
}

// covers: image.pipeline
#[test]
fn drawing_a_frame_moves_no_clock() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let a = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    put(&mut s, &f, a, 0, 0);
    let before = s.clone();
    for _ in 0..3 {
        let _ = s.frame(f.rows(), COLS);
    }
    assert_eq!(s, before);
    assert_eq!(s.clock, 2, "a transmission and a placement");
}

#[test]
fn a_region_scroll_splits_a_sixel_and_leaves_a_kitty_across_an_edge() {
    let mut f = Fake::new(6);
    let mut s = store(1 << 30);
    let six = add(&mut s, &f, Protocol::SIXEL, 8, 48, 0);
    let kit = add(&mut s, &f, Protocol::KITTY, 8, 48, 1);
    let inside = add(&mut s, &f, Protocol::KITTY, 8, 16, 2);
    put(&mut s, &f, six, 0, 1);
    put(&mut s, &f, kit, 1, 1);
    put(&mut s, &f, inside, 2, 3);
    // rows 2 to 4 scroll up one: the screen's lines move as a screen does
    s.scroll((2, 4), 1, true, false, f.rows());
    let line = f.screen.remove(2);
    f.screen.insert(4, LineKey::new(50));
    let _ = line;
    s.scroll_done(f.rows());
    let mut got = shown(&s, &f);
    got.sort();
    assert_eq!(
        got,
        [
            // the sixel's first row stays, its second leaves the band's
            // top, its third moves up into row 2
            (1, 0, 1, 1, six),
            (1, 1, 1, 3, kit),
            (2, 0, 1, 1, six),
            (2, 2, 1, 1, inside),
        ]
    );
}

#[test]
fn a_line_dropped_from_the_scrollback_clips_its_placements() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let tall = add(&mut s, &f, Protocol::KITTY, 8, 48, 1);
    let short = add(&mut s, &f, Protocol::KITTY, 8, 16, 2);
    put(&mut s, &f, tall, 0, 0);
    put(&mut s, &f, short, 1, 0);
    s.discard_line(LineKey::new(0), Some(LineKey::new(1)));
    assert_eq!(shown(&s, &f), [(1, 0, 1, 2, tall)]);
    assert!(
        s.image(MAIN, short).is_some(),
        "a kitty image outlives its placement"
    );
}

#[test]
fn a_line_dropped_for_several_rows_clips_as_many_as_one_drop_a_row() {
    let f = Fake::new(4);
    let mut once = store(1 << 30);
    let tall = add(&mut once, &f, Protocol::KITTY, 8, 48, 1);
    put(&mut once, &f, tall, 0, 0);
    let mut each = once.clone();
    once.discard_rows(LineKey::new(0), 2, Some(LineKey::new(2)));
    each.discard_line(LineKey::new(0), Some(LineKey::new(1)));
    each.discard_line(LineKey::new(1), Some(LineKey::new(2)));
    assert_eq!(shown(&once, &f), [(2, 0, 1, 1, tall)]);
    assert_eq!(shown(&once, &f), shown(&each, &f));
    once.discard_rows(LineKey::new(2), 1, Some(LineKey::new(3)));
    assert_eq!(shown(&once, &f), [], "no row is left to show");
}

#[test]
fn the_alternate_buffer_empties_both_ways_and_ris_empties_both() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let main = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    s.set_alternate(true);
    let screens = Screens {
        main: f.rows(),
        alt: f.rows(),
    };
    let alt = s
        .add_image(image(Protocol::KITTY, 8, 16, 1), None, screens)
        .unwrap();
    assert_ne!(main, alt, "kitty ids are per buffer");
    assert_eq!(s.image_count(), 2);
    s.set_alternate(false);
    assert_eq!(s.image_count(), 1);
    assert!(s.image(MAIN, main).is_some());
    s.reset();
    assert_eq!(s.image_count(), 0);
    assert_eq!(s.bytes(), 0);
}

#[test]
fn ed_2_removes_kitty_placements_and_keeps_their_images() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let k = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    let six = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    put(&mut s, &f, k, 0, 0);
    put(&mut s, &f, six, 1, 0);
    s.erase_screen(f.rows());
    assert_eq!(
        shown(&s, &f),
        [(0, 1, 1, 1, six)],
        "the erase's cuts take the sixel"
    );
    assert!(s.image(MAIN, k).is_some());
}

#[test]
fn place_refuses_what_its_bounds_refuse() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let k = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    let refused = |s: &mut Store, place: Place| s.place(k, place, f.screens(), COLS);
    assert_eq!(
        refused(
            &mut s,
            Place {
                col: 20,
                ..Place::default()
            }
        ),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        refused(
            &mut s,
            Place {
                row: 4,
                ..Place::default()
            }
        ),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        refused(
            &mut s,
            Place {
                x: 8,
                ..Place::default()
            }
        ),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        refused(
            &mut s,
            Place {
                src_x: 8,
                ..Place::default()
            }
        ),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        refused(
            &mut s,
            Place {
                cols: 10_001,
                ..Place::default()
            }
        ),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        s.place(ImageKey(99), Place::default(), f.screens(), COLS),
        Err(Refusal::NotFound)
    );
    let bad = NewImage {
        width: 0,
        ..image(Protocol::KITTY, 8, 16, 2)
    };
    assert_eq!(s.add_image(bad, None, f.screens()), Err(Refusal::Invalid));
}

#[test]
fn a_box_fits_and_centres_its_picture() {
    let f = Fake::new(8);
    let mut s = store(1 << 30);
    let k = add(&mut s, &f, Protocol::KITTY, 16, 16, 1);
    let place = Place {
        cols: 4,
        rows: 2,
        ..Place::default()
    };
    s.place(k, place, f.screens(), COLS).unwrap();
    let p = &s.frame(f.rows(), COLS)[0];
    // a 32x32 box: the square picture scales to 32x32, centred
    assert_eq!(
        (p.cols, p.rows, p.x, p.y, p.width, p.height),
        (4, 2, 0, 0, 32, 32)
    );
    let wide = Place {
        cols: 4,
        ..Place::default()
    };
    s.place(k, wide, f.screens(), COLS).unwrap();
    let p = &s.frame(f.rows(), COLS)[1];
    assert_eq!((p.cols, p.rows, p.width, p.height), (4, 2, 32, 32));
}

#[test]
fn an_opaque_sixel_placed_twice_over_itself_keeps_its_image() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let six = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    put(&mut s, &f, six, 0, 0);
    // the second placement's cover cut removes the first, its image's last
    put(&mut s, &f, six, 0, 0);
    assert_eq!(s.image_count(), 1, "the image being placed is never freed");
    assert_eq!(shown(&s, &f), [(0, 0, 1, 1, six)]);
    assert_eq!(s.bytes(), ONE_CELL);
}

#[test]
fn a_reservation_holds_its_bytes_and_the_image_it_replaces() {
    let f = Fake::new(4);
    let mut s = store(3 * ONE_CELL);
    let old = add(&mut s, &f, Protocol::KITTY, 8, 16, 7);
    let other = add(&mut s, &f, Protocol::KITTY, 8, 16, 8);
    // two cells for id 7, counting the one it holds: one more is reserved
    let mut r = s
        .reserve(Protocol::KITTY, 7, 2 * ONE_CELL, f.screens())
        .unwrap();
    assert_eq!(r.bytes(), ONE_CELL);
    assert_eq!(s.bytes(), 3 * ONE_CELL);
    // the budget is full, and what may go is the other image alone: the
    // held one is never evicted
    assert_eq!(s.grow(&mut r, ONE_CELL, f.screens()), Ok(()));
    assert!(s.image(MAIN, other).is_none(), "the unheld image made room");
    assert!(s.image(MAIN, old).is_some(), "the held image stays");
    assert_eq!(s.grow(&mut r, ONE_CELL, f.screens()), Err(Refusal::NoSpace));
    // stored, the new image takes the reservation and the old one's bytes
    let new = s
        .add_image(image(Protocol::KITTY, 8, 48, 7), Some(r), f.screens())
        .unwrap();
    assert!(s.image(MAIN, old).is_none());
    // and the old one's pixels stay as the spare buffer, charged
    assert_eq!(s.bytes(), 8 * 48 * 4 + IMAGE_OVERHEAD + 8 * 16 * 4);
    assert_eq!(s.find_image(7, 0), Some(new));
}

#[test]
fn a_reservation_given_back_frees_its_bytes_and_its_hold() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let old = add(&mut s, &f, Protocol::KITTY, 8, 16, 7);
    let r = s.reserve(Protocol::KITTY, 7, 4096, f.screens()).unwrap();
    assert_eq!(s.image(MAIN, old).map(|i| i.hold), Some(1));
    s.unreserve(r);
    assert_eq!(s.image(MAIN, old).map(|i| i.hold), Some(0));
    assert_eq!(s.bytes(), ONE_CELL);
    // a refused image gives its reservation back too
    let r = s.reserve(Protocol::SIXEL, 0, 4096, f.screens()).unwrap();
    let bad = NewImage {
        width: 0,
        ..image(Protocol::SIXEL, 8, 16, 0)
    };
    assert_eq!(
        s.add_image(bad, Some(r), f.screens()),
        Err(Refusal::Invalid)
    );
    assert_eq!(s.bytes(), ONE_CELL);
}

// A handler that gives one reservation back twice, through a copy, gets its
// bytes and its hold back once: the second copy finds the store's ledger
// without it and changes nothing.
// covers: image.pipeline
#[test]
fn a_reservation_given_back_twice_gives_its_bytes_and_its_hold_back_once() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    // a sixel of 1024 bytes, as the review of the handler contract stored
    let _ = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    assert_eq!(s.bytes(), 8 * 16 * 4 + IMAGE_OVERHEAD);
    let r = s.reserve(Protocol::SIXEL, 0, 4096, f.screens()).unwrap();
    s.unreserve(r.clone());
    s.unreserve(r);
    assert_eq!(
        s.bytes(),
        8 * 16 * 4 + IMAGE_OVERHEAD,
        "the sixel's bytes stay"
    );
    // two reservations hold the image of id 7; one given back twice
    // releases only its own hold
    let old = add(&mut s, &f, Protocol::KITTY, 8, 16, 7);
    let before = s.bytes();
    let first = s.reserve(Protocol::KITTY, 7, 4096, f.screens()).unwrap();
    let second = s.reserve(Protocol::KITTY, 7, 4096, f.screens()).unwrap();
    assert_eq!(s.image(MAIN, old).map(|i| i.hold), Some(2));
    s.unreserve(first.clone());
    s.unreserve(first);
    assert_eq!(
        s.image(MAIN, old).map(|i| i.hold),
        Some(1),
        "the second still holds it"
    );
    assert_eq!(s.bytes(), before + second.bytes());
    s.unreserve(second);
    assert_eq!(s.bytes(), before);
}

// A copy of a reservation an image already spent pays for no second image:
// that image reserves its whole cost, which a budget with room for one
// image refuses.
// covers: image.pipeline
#[test]
fn a_reservation_an_image_spent_pays_for_no_second_image() {
    let f = Fake::new(4);
    let mut s = store(ONE_CELL);
    let r = s
        .reserve(Protocol::SIXEL, 0, ONE_CELL, f.screens())
        .unwrap();
    let a = s
        .add_image(
            image(Protocol::SIXEL, 8, 16, 0),
            Some(r.clone()),
            f.screens(),
        )
        .expect("stored");
    // a sixel placed on the screen is never evicted
    put(&mut s, &f, a, 0, 0);
    assert_eq!(s.bytes(), ONE_CELL);
    assert_eq!(
        s.add_image(
            image(Protocol::SIXEL, 8, 16, 0),
            Some(r.clone()),
            f.screens()
        ),
        Err(Refusal::NoSpace),
        "the second image has nothing reserved"
    );
    assert_eq!(s.image_count(), 1);
    assert_eq!(s.bytes(), ONE_CELL);
    // and the copy given back gives back nothing
    s.unreserve(r);
    assert_eq!(s.bytes(), ONE_CELL);
}

// A copy of a reservation already spent does not grow: the bytes it would
// reserve would belong to no reservation the store could take them back
// from.
// covers: image.pipeline
#[test]
fn a_reservation_already_spent_is_refused_growth() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let r = s.reserve(Protocol::SIXEL, 0, 4096, f.screens()).unwrap();
    let mut copy = r.clone();
    s.unreserve(r);
    assert_eq!(s.bytes(), 0);
    assert_eq!(s.grow(&mut copy, 4096, f.screens()), Err(Refusal::Invalid));
    assert_eq!(s.bytes(), 0, "nothing was reserved");
    assert_eq!(copy.bytes(), 4096, "the copy is as it was");
    // a live reservation grows, and gives back what it grew by
    let mut live = s.reserve(Protocol::SIXEL, 0, 4096, f.screens()).unwrap();
    s.grow(&mut live, 4096, f.screens()).expect("room");
    assert_eq!(live.bytes(), 8192);
    assert_eq!(s.bytes(), 8192);
    s.unreserve(live);
    assert_eq!(s.bytes(), 0);
}

// The reservation `kitty_buffer` makes of the spare buffer's charge, grown
// by the rest of the picture's cost and holding the image of the id the
// new one replaces, is spent once like any other.
// covers: image.pipeline
#[test]
fn a_spare_buffer_reservation_given_back_twice_gives_back_once() {
    const LEN: usize = 16 * 32;
    let cost = 16 * 32 * 4 + IMAGE_OVERHEAD;
    let f = Fake::new(4);
    let mut s = store(1 << 24);
    let _ = add(&mut s, &f, Protocol::KITTY, 16, 32, 7);
    // the replacement frees the first image, whose pixels stay as the spare
    let old = add(&mut s, &f, Protocol::KITTY, 16, 32, 7);
    let images = s.bytes() - 16 * 32 * 4;
    let (buffer, r) = Edit::new(&mut s, f.screens(), COLS)
        .kitty_buffer(7, LEN, cost)
        .expect("room");
    assert_eq!(buffer.len(), LEN, "the buffer is the spare");
    let other = s.reserve(Protocol::KITTY, 7, cost, f.screens()).unwrap();
    assert_eq!(s.image(MAIN, old).map(|i| i.hold), Some(2));
    assert_eq!(s.bytes(), images + r.bytes() + other.bytes());
    s.unreserve(r.clone());
    s.unreserve(r);
    assert_eq!(
        s.image(MAIN, old).map(|i| i.hold),
        Some(1),
        "the other still holds it"
    );
    assert_eq!(s.bytes(), images + other.bytes());
    s.unreserve(other);
    assert_eq!(s.bytes(), images);
    // with no image of its id to count, the reservation grows by the 512
    // bytes beside the pixels, and gives those back too
    let _ = add(&mut s, &f, Protocol::KITTY, 16, 32, 7);
    let images = s.bytes() - 16 * 32 * 4;
    let (buffer, r) = Edit::new(&mut s, f.screens(), COLS)
        .kitty_buffer(0, LEN, cost)
        .expect("room");
    assert_eq!(buffer.len(), LEN, "the buffer is the spare");
    assert_eq!(r.bytes(), cost);
    assert_eq!(s.bytes(), images + cost);
    s.unreserve(r.clone());
    s.unreserve(r);
    assert_eq!(s.bytes(), images);
}

#[test]
fn a_command_finds_an_image_by_id_or_newest_number_and_stamps_it() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let numbered = |id, number| NewImage {
        number,
        ..image(Protocol::KITTY, 8, 16, id)
    };
    let a = s.add_image(numbered(1, 5), None, f.screens()).unwrap();
    let b = s.add_image(numbered(3, 5), None, f.screens()).unwrap();
    assert_eq!(s.find_image(1, 0), Some(a));
    assert_eq!(s.find_image(0, 5), Some(b), "the newest of the number");
    assert_eq!(s.find_image(0, 0), None);
    assert_eq!(s.free_id(), 2);
    let clock = s.clock;
    s.touch(a);
    assert_eq!(s.clock, clock + 1, "naming an image is a stream event");
    assert_eq!(s.image(MAIN, a).map(|i| i.used_at), Some(clock + 1));
    s.drop_image(b);
    assert_eq!(s.find_image(0, 5), Some(a), "then the older one");
}

#[test]
fn a_box_is_bounded_on_each_side_not_by_its_area() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let k = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    let at = |cols, rows| Place {
        cols,
        rows,
        ..Place::default()
    };
    assert!(s.place(k, at(200, 200), f.screens(), COLS).is_ok());
    assert_eq!(
        s.place(k, at(10_001, 1), f.screens(), COLS),
        Err(Refusal::Invalid)
    );
    assert!(s.place(k, at(10_000, 1), f.screens(), COLS).is_ok());
}

#[test]
fn a_cut_placements_pieces_stay_beneath_a_later_picture() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let a = add(&mut s, &f, Protocol::SIXEL, 24, 16, 0);
    // a picture with clear pixels covers nothing beneath it
    let clear = NewImage {
        pixels: vec![0x8000_0080; 24 * 16],
        ..image(Protocol::SIXEL, 24, 16, 0)
    };
    let b = s.add_image(clear, None, f.screens()).unwrap();
    put(&mut s, &f, a, 0, 0);
    put(&mut s, &f, b, 0, 0);
    s.cut(0, 1, 1, f.rows(), COLS);
    let order: Vec<(ImageKey, u32)> = s
        .frame(f.rows(), COLS)
        .iter()
        .map(|p| (p.image, p.col))
        .collect();
    assert_eq!(
        order,
        [(a, 0), (a, 2), (b, 0), (b, 2)],
        "a's pieces stay where a was, beneath b's"
    );
}

#[test]
fn a_frame_entry_carries_the_whole_placements_crop_and_drawn_picture() {
    let f = Fake::new(8);
    let mut s = store(1 << 30);
    let k = add(&mut s, &f, Protocol::KITTY, 100, 60, 1);
    // a 40x20 crop fitted to a 4 by 2 box of 8x16 cells, 32x32 pixels: it
    // scales to 32x16, centred 8 pixels down
    let place = Place {
        row: 1,
        cols: 4,
        rows: 2,
        src_x: 10,
        src_y: 5,
        src_width: 40,
        src_height: 20,
        ..Place::default()
    };
    s.place(k, place, f.screens(), COLS).unwrap();
    let p = &s.frame(f.rows(), COLS)[0];
    assert_eq!(
        (p.crop_x, p.crop_y, p.crop_width, p.crop_height),
        (10, 5, 40, 20)
    );
    assert_eq!((p.offset_x, p.offset_y), (0, 8));
    assert_eq!((p.drawn_width, p.drawn_height), (32, 16));
    assert_eq!((p.x, p.y, p.width, p.height), (0, 8, 32, 16));

    // a cut through a sixel at 1:1: the piece below the cut row shows only
    // the picture's lower part, and still names the whole picture
    let six = add(&mut s, &f, Protocol::SIXEL, 16, 48, 0);
    put(&mut s, &f, six, 10, 4);
    s.cut(4, 10, 11, f.rows(), COLS);
    let below = s
        .frame(f.rows(), COLS)
        .into_iter()
        .find(|p| p.image == six && p.row == 5)
        .expect("the piece below the cut");
    assert_eq!((below.cols, below.rows), (2, 2));
    assert_eq!(
        (below.src_y, below.src_height),
        (16, 32),
        "the visible part"
    );
    assert_eq!(
        (
            below.crop_x,
            below.crop_y,
            below.crop_width,
            below.crop_height
        ),
        (0, 0, 16, 48),
        "the whole crop"
    );
    assert_eq!((below.offset_x, below.offset_y), (0, -16));
    assert_eq!((below.drawn_width, below.drawn_height), (16, 48));

    // a cut through the middle column of a 24x16 sixel: the piece to its
    // right shows the picture's last 8 columns, drawn from 16 pixels left
    // of its box
    let wide = add(&mut s, &f, Protocol::SIXEL, 24, 16, 0);
    put(&mut s, &f, wide, 14, 7);
    s.cut(7, 15, 15, f.rows(), COLS);
    let right = s
        .frame(f.rows(), COLS)
        .into_iter()
        .find(|p| p.image == wide && p.col == 16)
        .expect("the piece right of the cut");
    assert_eq!((right.cols, right.rows), (1, 1));
    assert_eq!(
        (right.x, right.width, right.src_x, right.src_width),
        (0, 8, 16, 8),
        "the visible part"
    );
    assert_eq!(
        (
            right.crop_x,
            right.crop_y,
            right.crop_width,
            right.crop_height
        ),
        (0, 0, 24, 16),
        "the whole crop"
    );
    assert_eq!((right.offset_x, right.offset_y), (-16, 0));
    assert_eq!((right.drawn_width, right.drawn_height), (24, 16));
}

/// A sixel image whose pixels `f(x, y)` gives.
fn drawn(w: u32, h: u32, f: impl Fn(u32, u32) -> u32) -> NewImage {
    let pixels = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| f(x, y));
    NewImage {
        pixels: pixels.collect(),
        ..image(Protocol::SIXEL, w, h, 0)
    }
}

/// The placements as (row, col, cols, rows) of each, in draw order.
fn boxes(s: &Store, f: &Fake) -> Vec<(i64, u32, u32, u32)> {
    let view = Rows {
        screen: &f.screen,
        above: &[],
    };
    s.frame(view, COLS)
        .iter()
        .map(|p| (p.row, p.col, p.cols, p.rows))
        .collect()
}

// fork-perf item 15: the store keeps the buffer the handler filled, and
// the frame shares it.
// covers: image.pipeline
#[test]
fn an_image_keeps_the_handlers_buffer_without_a_copy() {
    let f = Fake::new(4);
    let mut s = store(1 << 20);
    let new = image(Protocol::SIXEL, 64, 32, 0);
    let buffer = new.pixels.as_ptr();
    let key = s.add_image(new, None, f.screens()).expect("stored");
    s.place(key, Place::default(), f.screens(), COLS)
        .expect("placed");
    let view = Rows {
        screen: &f.screen,
        above: &[],
    };
    let frame = s.frame(view, COLS);
    assert_eq!(frame[0].pixels.as_slice().as_ptr(), buffer);
}

// fork-perf item 14: a handler that knows its picture's opacity passes it,
// and the store reads no pixel for it; one that does not lets the store
// read them.
// covers: image.pipeline
#[test]
fn opacity_the_handler_passes_spares_the_store_a_read() {
    let f = Fake::new(4);
    let mut s = store(1 << 20);
    let before = s.work();
    s.add_image(
        NewImage {
            opaque: Some(true),
            ..image(Protocol::SIXEL, 64, 32, 0)
        },
        None,
        f.screens(),
    )
    .expect("stored");
    let given = s.work() - before;
    let before = s.work();
    s.add_image(image(Protocol::SIXEL, 64, 32, 0), None, f.screens())
        .expect("stored");
    assert_eq!(s.work() - before, given + 64 * 32);
}

// A build with debug assertions checks the opacity a handler passes.
#[cfg(debug_assertions)]
#[test]
#[should_panic = "the handler's opacity disagrees with the pixels"]
fn a_handler_that_passes_a_wrong_opacity_is_caught() {
    let f = Fake::new(4);
    let mut s = store(1 << 20);
    let clear = drawn(8, 16, |x, _| if x == 0 { 0 } else { 0xff00_00ff });
    let _ = s.add_image(
        NewImage {
            opaque: Some(true),
            ..clear
        },
        None,
        f.screens(),
    );
}

// ADR-1100 D3, a new picture of the same tag over it, cell by cell: a
// picture padded with clear cells, as chafa pads its frames, replaces the
// one under it, and the older picture's pieces in the padding, which hold
// no pixel set, go with it.
// covers: image.overlap
#[test]
fn a_padded_picture_over_the_last_replaces_it_cell_by_cell() {
    let f = Fake::new(4);
    let mut s = store(1 << 24);
    // 3 by 3 cells of 8 by 16, its content the top left 2 by 2
    let frame = || {
        drawn(
            24,
            48,
            |x, y| if x < 16 && y < 32 { 0xff00_00ff } else { 0 },
        )
    };
    for n in 0..10 {
        let key = s.add_image(frame(), None, f.screens()).expect("stored");
        put(&mut s, &f, key, 0, 0);
        assert_eq!(boxes(&s, &f), [(0, 0, 3, 3)], "after frame {n}");
        assert_eq!(s.image_count(), 1, "after frame {n}");
    }
}

// Under a cell the new picture covers only partly, the older one stays and
// shows through: a clear pixel in every cell covers none of them.
// covers: image.overlap
#[test]
fn a_picture_with_a_clear_pixel_in_each_cell_cuts_nothing() {
    let f = Fake::new(4);
    let mut s = store(1 << 24);
    let old = s
        .add_image(image(Protocol::SIXEL, 16, 32, 0), None, f.screens())
        .expect("stored");
    put(&mut s, &f, old, 0, 0);
    let holes = drawn(16, 32, |x, y| {
        if x % 8 == 3 && y % 16 == 5 {
            0
        } else {
            0xff00_ff00
        }
    });
    let new = s.add_image(holes, None, f.screens()).expect("stored");
    put(&mut s, &f, new, 0, 0);
    assert_eq!(boxes(&s, &f), [(0, 0, 2, 2), (0, 0, 2, 2)]);
}

// An opaque picture whose size is no whole number of cells covers fully
// only the cells it fills: the older picture keeps its parts under the new
// one's edge cells.
// covers: image.overlap
#[test]
fn an_opaque_picture_cuts_only_the_cells_it_fills() {
    let f = Fake::new(4);
    let mut s = store(1 << 24);
    let old = s
        .add_image(image(Protocol::SIXEL, 24, 48, 0), None, f.screens())
        .expect("stored");
    put(&mut s, &f, old, 0, 0);
    // 12 by 20 pixels fill the cell at 0,0 and part of three others
    let new = s
        .add_image(image(Protocol::SIXEL, 12, 20, 0), None, f.screens())
        .expect("stored");
    put(&mut s, &f, new, 0, 0);
    let mut shown = boxes(&s, &f);
    shown.sort_unstable();
    // the old picture loses cell 0,0: pieces right of it and below it
    assert_eq!(shown, [(0, 0, 2, 2), (0, 1, 2, 1), (1, 0, 3, 2)]);
}

// A kitty picture is never cut or dropped by a new sixel, even one that
// shows nothing, and a sixel never by a kitty one.
// covers: image.overlap
#[test]
fn a_new_picture_cuts_only_older_pictures_of_its_kind() {
    let f = Fake::new(4);
    let mut s = store(1 << 24);
    let clear = NewImage {
        pixels: vec![0; 16 * 32],
        ..image(Protocol::KITTY, 16, 32, 7)
    };
    let kitty = s.add_image(clear, None, f.screens()).expect("stored");
    put(&mut s, &f, kitty, 0, 0);
    let six = add(&mut s, &f, Protocol::SIXEL, 16, 32, 0);
    put(&mut s, &f, six, 0, 0);
    let kitty2 = add(&mut s, &f, Protocol::KITTY, 16, 32, 8);
    put(&mut s, &f, kitty2, 0, 0);
    assert_eq!(boxes(&s, &f).len(), 3);
}

// vtium's reading of "a piece that draws nothing": two opaque pictures of
// the same size that end inside a cell, as most of mpv's frames do. The
// older picture's pieces in the edge cells lie wholly under the newer
// one's opaque pixels, so they go, and one picture stays.
// covers: image.overlap
#[test]
fn opaque_frames_that_end_inside_a_cell_leave_one_picture() {
    let f = Fake::new(4);
    let mut s = store(1 << 24);
    for n in 0..10 {
        let key = add(&mut s, &f, Protocol::SIXEL, 20, 40, 0);
        put(&mut s, &f, key, 0, 0);
        assert_eq!(boxes(&s, &f), [(0, 0, 3, 3)], "after frame {n}");
        assert_eq!(s.image_count(), 1, "after frame {n}");
    }
    // a clear pixel of the newer one in an edge cell, over a set pixel of
    // the older one, lets the older one show there, so it stays
    let holed = drawn(
        20,
        40,
        |x, y| if (x, y) == (17, 37) { 0 } else { 0xff00_ff00 },
    );
    let key = s.add_image(holed, None, f.screens()).expect("stored");
    put(&mut s, &f, key, 0, 0);
    assert_eq!(s.image_count(), 2);
}

// fork-perf item 16's primitive: the buffer of the image freed last goes
// back to a handler that asks for one of its size, once no frame holds it.
// covers: image.pipeline
#[test]
fn a_freed_images_buffer_goes_back_to_the_next_picture_of_its_size() {
    let f = Fake::new(4);
    let mut s = store(1 << 24);
    let first = add(&mut s, &f, Protocol::KITTY, 16, 32, 7);
    let buffer = {
        let view = Rows {
            screen: &f.screen,
            above: &[],
        };
        put(&mut s, &f, first, 0, 0);
        s.frame(view, COLS)[0].pixels.as_slice().as_ptr()
    };
    // a kitty transmission of the same id replaces it and frees the old
    let _ = add(&mut s, &f, Protocol::KITTY, 16, 32, 7);
    let mut edit = Edit::new(&mut s, f.screens(), COLS);
    assert!(
        edit.take_spare(Protocol::KITTY, 0, 16 * 31).is_none(),
        "another size"
    );
    let (spare, r) = edit
        .take_spare(Protocol::KITTY, 0, 16 * 32)
        .expect("the freed buffer");
    assert_eq!(spare.as_ptr(), buffer);
    assert_eq!(r.bytes(), 16 * 32 * 4, "its charge comes with it");
    assert!(
        edit.take_spare(Protocol::KITTY, 0, 16 * 32).is_none(),
        "taken once"
    );
}

// A frame that still holds the freed image's pixels keeps the buffer from
// being filled again under it.
// covers: image.pipeline
#[test]
fn a_freed_buffer_a_frame_still_holds_is_not_given_back() {
    let f = Fake::new(4);
    let mut s = store(1 << 24);
    let first = add(&mut s, &f, Protocol::KITTY, 16, 32, 7);
    put(&mut s, &f, first, 0, 0);
    let view = Rows {
        screen: &f.screen,
        above: &[],
    };
    let frame = s.frame(view, COLS);
    let _ = add(&mut s, &f, Protocol::KITTY, 16, 32, 7);
    let mut edit = Edit::new(&mut s, f.screens(), COLS);
    assert!(edit.take_spare(Protocol::KITTY, 0, 16 * 32).is_none());
    drop(frame);
    assert!(edit.take_spare(Protocol::KITTY, 0, 16 * 32).is_some());
}

// A reservation still in the ledger at RIS is one a handler dropped
// without spending, since each handler's reset gives back what it holds
// before the store resets; a build with debug assertions stops there.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "a handler kept or dropped a reservation past RIS")]
fn a_reset_with_a_reservation_still_out_fails_a_debug_build() {
    let f = Fake::new(4);
    let mut s = store(1 << 24);
    // the handler drops the reservation without giving it back
    let _ = s.reserve(Protocol::KITTY, 0, 4096, f.screens()).unwrap();
    s.reset();
}

// The spare buffer is charged to the budget, and a reset frees it.
// covers: image.pipeline
#[test]
fn a_reset_frees_the_spare_buffer_and_its_charge() {
    let f = Fake::new(4);
    let mut s = store(1 << 24);
    let _ = add(&mut s, &f, Protocol::KITTY, 16, 32, 7);
    let one = s.bytes();
    let _ = add(&mut s, &f, Protocol::KITTY, 16, 32, 7);
    assert_eq!(s.bytes(), one + 16 * 32 * 4, "the spare is charged");
    s.reset();
    assert_eq!(s.bytes(), 0);
    let mut edit = Edit::new(&mut s, f.screens(), COLS);
    assert!(edit.take_spare(Protocol::KITTY, 0, 16 * 32).is_none());
}

// A reservation that needs room frees the spare buffer before it evicts
// any image.
// covers: image.pipeline
#[test]
fn a_reservation_that_needs_room_frees_the_spare_before_any_image() {
    let f = Fake::new(4);
    let image = 16 * 32 * 4 + IMAGE_OVERHEAD;
    // room for two images and the spare, and no more
    let mut s = store(2 * image + 16 * 32 * 4);
    let a = add(&mut s, &f, Protocol::KITTY, 16, 32, 7);
    put(&mut s, &f, a, 0, 0);
    let _ = add(&mut s, &f, Protocol::KITTY, 16, 32, 7);
    let _ = add(&mut s, &f, Protocol::KITTY, 16, 32, 8);
    assert_eq!(s.image_count(), 2);
    assert_eq!(s.bytes(), 2 * image + 16 * 32 * 4);
    // an unplaced image would be the first to go; the spare goes before it
    let r = s
        .reserve(Protocol::KITTY, 0, 16 * 32 * 4, f.screens())
        .expect("room");
    assert_eq!(s.image_count(), 2, "no image was evicted");
    s.unreserve(r);
    let mut edit = Edit::new(&mut s, f.screens(), COLS);
    assert!(edit.take_spare(Protocol::KITTY, 0, 16 * 32).is_none());
}

// kitty's case: under a budget that fits the frame shown and the one
// replacing it, and no more, a handler that takes the spare buffer with
// its charge, rather than reserving a frame afresh, reuses the buffer
// every frame: the reservation never needs room, so it never evicts the
// buffer it is about to fill.
// covers: image.pipeline
#[test]
fn frames_of_one_size_reuse_the_spare_under_a_tight_budget() {
    const LEN: usize = 16 * 32;
    let frame = 16 * 32 * 4;
    let f = Fake::new(4);
    let mut s = store(2 * (frame + IMAGE_OVERHEAD));
    let mut buffers = Vec::new();
    let mut shown: Option<ImageKey> = None;
    for n in 0..8 {
        let before = s.bytes();
        let taken = Edit::new(&mut s, f.screens(), COLS).take_spare(Protocol::KITTY, 0, LEN);
        // from the third frame on, the frame before the last has been freed
        // and nothing holds it, so the spare is there to take
        assert_eq!(taken.is_some(), n >= 2, "frame {n} takes the spare");
        let (pixels, r) = match taken {
            Some(spare) => {
                // the spare's charge moved into the reservation whole
                assert_eq!(s.bytes(), before, "frame {n}");
                assert_eq!(spare.1.bytes(), frame, "frame {n}");
                spare
            }
            None => {
                let r = s
                    .reserve(Protocol::KITTY, 0, frame, f.screens())
                    .expect("room for a frame");
                (vec![0xff00_00ff; LEN], r)
            }
        };
        buffers.push(pixels.as_ptr());
        let new = NewImage {
            pixels,
            ..image(Protocol::KITTY, 16, 32, 0)
        };
        let key = s.add_image(new, Some(r), f.screens()).expect("stored");
        if let Some(old) = shown.replace(key) {
            s.drop_image(old);
        }
        assert_eq!(s.image_count(), 1, "frame {n}");
        assert!(s.bytes() <= s.limit(), "frame {n}");
    }
    for n in 2..buffers.len() {
        assert_eq!(
            buffers[n],
            buffers[n - 2],
            "frame {n} reuses frame {}'s buffer",
            n - 2
        );
    }
}

/// Blends the pictures of `frame`'s entries over `out`, a screen of
/// `COLS` by `rows` cells of 8 by 16 pixels in premultiplied ARGB, each
/// entry at its pixel size: the compositor's "over" for pictures drawn 1:1.
fn blend_over(out: &mut [u32], frame: &[FramePlacement], rows: usize) {
    let (w, h) = (COLS * 8, rows * 16);
    for p in frame {
        assert_eq!((p.width, p.height), (p.src_width, p.src_height), "1:1");
        let px = p.pixels.as_slice();
        for j in 0..p.height {
            for i in 0..p.width {
                let sx = usize::try_from(p.col * 8 + p.x + i).expect("a column");
                let Ok(sy) = usize::try_from(p.row * 16 + i64::from(p.y + j)) else {
                    continue;
                };
                if sx >= w || sy >= h {
                    continue;
                }
                let src = px[((p.src_y + j) * p.image_width + p.src_x + i) as usize];
                let d = &mut out[sy * w + sx];
                let a = src >> 24;
                let blend = |shift: u32| {
                    let sc = (src >> shift) & 0xff;
                    let dc = (*d >> shift) & 0xff;
                    (sc + (dc * (255 - a) + 127) / 255).min(255) << shift
                };
                *d = blend(24) | blend(16) | blend(8) | blend(0);
            }
        }
    }
}

// D2's per-cell coverage and the hidden piece it drops: placing a sixel over
// older ones, whatever the store removes, draws exactly what the older
// pictures with the new one blended over them draw. 3,000 rounds of two to
// five random sixels, opaque, two-color, with clear or half-clear pixels,
// at random sizes and cells, from one fixed seed.
// covers: image.pipeline
#[test]
fn hidden_pieces_change_no_composite_pixel() {
    const ROWS: usize = 6;
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut rnd = move |n: u32| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % u64::from(n)) as u32
    };
    let colors = [0xff00_ff00u32, 0xffff_0000, 0x8040_0000, 0];
    for round in 0..3000 {
        let f = Fake::new(ROWS as u64);
        let mut s = store(1 << 30);
        for n in 0..2 + rnd(4) {
            let (w, h) = (1 + rnd(40), 1 + rnd(60));
            let kind = rnd(4);
            let pixels: Vec<u32> = (0..w * h)
                .map(|_| match kind {
                    0 => colors[0],
                    1 => colors[rnd(2) as usize],
                    2 => colors[rnd(4) as usize],
                    _ if rnd(8) == 0 => colors[2],
                    _ => colors[1],
                })
                .collect();
            let new = NewImage {
                pixels,
                ..image(Protocol::SIXEL, w, h, 0)
            };
            let (col, row) = (rnd(6), rnd(4));
            // what the screen showed, with the new picture blended over it
            let mut want = vec![0u32; COLS * 8 * ROWS * 16];
            blend_over(&mut want, &s.frame(f.rows(), COLS), ROWS);
            let mut alone = store(1 << 30);
            let key = alone
                .add_image(new.clone(), None, f.screens())
                .expect("stored");
            put(&mut alone, &f, key, col, row);
            blend_over(&mut want, &alone.frame(f.rows(), COLS), ROWS);
            // what the store shows once it places the new picture
            let key = s.add_image(new, None, f.screens()).expect("stored");
            put(&mut s, &f, key, col, row);
            let mut got = vec![0u32; COLS * 8 * ROWS * 16];
            blend_over(&mut got, &s.frame(f.rows(), COLS), ROWS);
            assert!(want == got, "round {round}, picture {n}");
        }
    }
}

/// Places `img` as `link` says, with the box `cols` by `rows` (0 for the
/// picture's own size) and kitty placement id `id`.
fn link(
    s: &mut Store,
    f: &Fake,
    img: ImageKey,
    link: Link,
    (cols, rows): (u32, u32),
    id: u32,
) -> PlacementKey {
    let place = Place {
        cols,
        rows,
        id,
        ..Place::default()
    };
    s.place_at(img, place, link, f.screens(), COLS)
        .expect("placed")
}

// covers: image.kitty-relative
#[test]
fn a_relative_placement_reports_its_box_from_its_roots_line() {
    let f = Fake::new(6);
    let mut s = store(1 << 30);
    let parent = add(&mut s, &f, Protocol::KITTY, 16, 16, 1);
    let child = add(&mut s, &f, Protocol::KITTY, 16, 16, 2);
    let place = Place {
        col: 1,
        row: 2,
        id: 1,
        ..Place::default()
    };
    s.place(parent, place, f.screens(), COLS).unwrap();
    let _ = s.take_changed();
    let at = |cols, rows| Link::Parent {
        image: 1,
        placement: 1,
        cols,
        rows,
    };
    link(&mut s, &f, child, at(3, -1), (0, 0), 1);
    let line = LineKey::new(2);
    assert_eq!(
        s.take_changed(),
        [Region {
            line,
            row: -1,
            col: 4,
            cols: 2,
            rows: 1
        }]
    );
    // a box that starts left of the screen reports only its part on it
    link(&mut s, &f, child, at(-2, 2), (0, 0), 2);
    assert_eq!(
        s.take_changed(),
        [Region {
            line,
            row: 2,
            col: 0,
            cols: 1,
            rows: 1
        }]
    );
    // moving the parent reports every child's old and new box
    s.move_anchor(PlacementKey(0), Some((LineKey::new(4), 1)));
    let changed = s.take_changed();
    assert!(
        changed.contains(&Region {
            line,
            row: -1,
            col: 4,
            cols: 2,
            rows: 1
        }),
        "{changed:?}"
    );
    let moved = LineKey::new(4);
    assert!(
        changed.contains(&Region {
            line: moved,
            row: -1,
            col: 4,
            cols: 2,
            rows: 1
        }),
        "{changed:?}"
    );
    assert_eq!(
        s.reach(),
        2,
        "a child two rows below its root's line reaches two rows"
    );
}

// covers: image.kitty-placeholders
#[test]
fn a_virtual_placement_has_its_size_in_cells_and_no_line() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    // 24 by 40 pixels: three cells wide and three high at 8 by 16
    let img = add(&mut s, &f, Protocol::KITTY, 24, 40, 1);
    let sized = link(&mut s, &f, img, Link::Virtual, (5, 2), 1);
    let own = link(&mut s, &f, img, Link::Virtual, (0, 0), 2);
    let buf = &s.buffers[MAIN];
    for (k, size) in [(sized, (5, 2)), (own, (3, 3))] {
        let p = &buf.placements[&k];
        assert_eq!(p.spot, Spot::Virtual);
        assert!(!p.attached);
        assert_eq!((p.cols, p.rows), size);
    }
    assert!(
        buf.by_line.is_empty(),
        "a virtual placement lies on no line"
    );
    assert!(buf.heights.is_empty());
    assert_eq!(s.reach(), 0);
    assert_eq!(s.placement_count(), 2);
    check_heights(&s);
    assert!(s.take_changed().is_empty(), "it covers no cell");
}

// covers: image.kitty-placeholders
#[test]
fn an_image_shown_only_by_a_virtual_placement_is_evicted_as_one_on_the_screen() {
    let mut f = Fake::new(2);
    let mut s = store(3 * ONE_CELL);
    // the virtual placement's image is used first, then one placed on a
    // line that leaves for the scrollback
    let shown = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    link(&mut s, &f, shown, Link::Virtual, (1, 1), 1);
    let scrolled = add(&mut s, &f, Protocol::KITTY, 8, 16, 2);
    put(&mut s, &f, scrolled, 0, 0);
    f.screen = vec![LineKey::new(7), LineKey::new(8)];
    let third = add(&mut s, &f, Protocol::KITTY, 8, 16, 3);
    put(&mut s, &f, third, 0, 0);
    // a fourth image needs room: the one placed only in the scrollback goes
    // before the one a virtual placement shows, however recently used
    add(&mut s, &f, Protocol::KITTY, 8, 16, 4);
    assert!(s.image(MAIN, shown).is_some());
    assert!(s.image(MAIN, scrolled).is_none());
}

// covers: image.kitty-relative
#[test]
fn a_child_above_a_parent_below_the_view_draws_from_the_lines_below() {
    // two scrollback lines, 20 and 21, above a screen of six, 0 to 5
    let f = Fake::new(6);
    let mut s = store(1 << 30);
    let parent = add(&mut s, &f, Protocol::KITTY, 8, 16, 1);
    let child = add(&mut s, &f, Protocol::KITTY, 8, 16, 2);
    let place = Place {
        row: 5,
        id: 1,
        ..Place::default()
    };
    s.place(parent, place, f.screens(), COLS).unwrap();
    let up = Link::Parent {
        image: 1,
        placement: 1,
        cols: 1,
        rows: -3,
    };
    link(&mut s, &f, child, up, (0, 0), 1);
    assert_eq!(s.reach_below(), 3);
    // the view scrolled back two rows shows lines 20, 21 and 0 to 3, and
    // the parent's line 5 lies below it
    let view: Vec<LineKey> = [20, 21, 0, 1, 2, 3].map(LineKey::new).to_vec();
    let below: Vec<LineKey> = [4, 5].map(LineKey::new).to_vec();
    let rows = Rows {
        screen: &view,
        above: &[],
    };
    let drawn: Vec<(i64, u32)> = s
        .frame_with_below(rows, &below, COLS)
        .iter()
        .map(|p| (p.row, p.col))
        .collect();
    assert_eq!(drawn, [(4, 1)], "the child on line 2, the view's row 4");
}

// covers: image.iterm2
#[test]
fn a_drawn_size_draws_the_picture_at_it_and_the_box_covers_the_cells_it_reaches() {
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    // 64 by 48 pixels drawn at 30 by 20: four columns and two rows at 8 by
    // 16, the picture stretched to the size given from the box's top left
    let img = add(&mut s, &f, Protocol::SIXEL, 64, 48, 0);
    let place = Place {
        col: 1,
        row: 1,
        ..Place::default()
    };
    let drawn = Link::Drawn {
        width: 30,
        height: 20,
    };
    s.place_at(img, place, drawn, f.screens(), COLS).unwrap();
    let frame = s.frame(f.rows(), COLS);
    let p = &frame[0];
    assert_eq!((p.col, p.row, p.cols, p.rows), (1, 1, 4, 2));
    assert_eq!((p.offset_x, p.offset_y), (0, 0));
    assert_eq!((p.drawn_width, p.drawn_height), (30, 20));
    assert_eq!((p.crop_width, p.crop_height), (64, 48));
    // a drawn size past the largest box is refused
    let wide = Link::Drawn {
        width: u32::try_from(BOX_CELLS_MAX * 8 + 1).unwrap(),
        height: 16,
    };
    assert_eq!(
        s.place_at(img, Place::default(), wide, f.screens(), COLS),
        Err(Placing::Refused(Refusal::Invalid))
    );
}

// covers: image.iterm2
#[test]
fn a_drawn_size_ignores_a_box_in_cells_the_place_names() {
    // the place's box of 9 by 7 cells is not the picture's: a drawn size
    // of 30 by 20 pixels still covers four columns and two rows at 8 by 16
    let f = Fake::new(4);
    let mut s = store(1 << 30);
    let img = add(&mut s, &f, Protocol::SIXEL, 64, 48, 0);
    let place = Place {
        col: 1,
        row: 1,
        cols: 9,
        rows: 7,
        ..Place::default()
    };
    let drawn = Link::Drawn {
        width: 30,
        height: 20,
    };
    s.place_at(img, place, drawn, f.screens(), COLS).unwrap();
    let frame = s.frame(f.rows(), COLS);
    let p = &frame[0];
    assert_eq!((p.col, p.row, p.cols, p.rows), (1, 1, 4, 2));
    assert_eq!((p.offset_x, p.offset_y), (0, 0));
    assert_eq!((p.drawn_width, p.drawn_height), (30, 20));
}

/// A picture under the sixel tag, 8 by 16 pixels of `root`, with a second
/// whole frame of `second`, as an iTerm2 picture's animated GIF is stored.
fn animated(s: &mut Store, f: &Fake, root: u32, second: Vec<u32>) -> ImageKey {
    let mut new = image(Protocol::SIXEL, 8, 16, 0);
    new.pixels = vec![root; 8 * 16];
    let key = s.add_image(new, None, f.screens()).expect("stored");
    let r = s
        .frame_reserve(key, anim::layer_cost(8, 16))
        .expect("reserved");
    let frame = NewFrame {
        x: 0,
        y: 0,
        width: 8,
        height: 16,
        opaque: all_opaque(&second),
        pixels: second,
        replace: true,
        base: 0,
        background: 0,
        edit: 0,
        gap: 0,
    };
    assert_eq!(s.add_frame(frame, r), Ok(2));
    key
}

// covers: image.iterm2
#[test]
fn an_animated_picture_covers_a_cell_only_where_every_frame_covers_it() {
    const RED: u32 = 0xffff_0000;
    let f = Fake::new(4);
    // a still opaque picture over an older one removes it
    let mut s = store(1 << 20);
    let old = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    put(&mut s, &f, old, 0, 0);
    let still = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    put(&mut s, &f, still, 0, 0);
    assert_eq!(
        shown(&s, &f).iter().map(|p| p.4).collect::<Vec<_>>(),
        [still]
    );
    // an animated one whose second frame has one clear pixel keeps it,
    // since that frame neither covers the cell nor hides the pixel beneath
    let mut s = store(1 << 20);
    let old = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    put(&mut s, &f, old, 0, 0);
    let mut second = vec![RED; 8 * 16];
    second[0] = 0;
    let gif = animated(&mut s, &f, RED, second);
    put(&mut s, &f, gif, 0, 0);
    assert_eq!(
        shown(&s, &f).iter().map(|p| p.4).collect::<Vec<_>>(),
        [old, gif]
    );
    // with both frames opaque it covers the cell and removes it
    let mut s = store(1 << 20);
    let old = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    put(&mut s, &f, old, 0, 0);
    let gif = animated(&mut s, &f, RED, vec![RED; 8 * 16]);
    put(&mut s, &f, gif, 0, 0);
    assert_eq!(shown(&s, &f).iter().map(|p| p.4).collect::<Vec<_>>(), [gif]);
}

// covers: image.iterm2
#[test]
fn an_animated_picture_draws_nothing_only_where_every_frame_draws_nothing() {
    const RED: u32 = 0xffff_0000;
    let f = Fake::new(4);
    // an older picture whose first frame is clear and whose second is not,
    // under a newer picture with clear pixels of its own: it still shows
    let mut s = store(1 << 20);
    let mut second = vec![0; 8 * 16];
    second[5] = RED;
    let gif = animated(&mut s, &f, 0, second);
    put(&mut s, &f, gif, 0, 0);
    // larger than the older picture, so the check of its two frames ends
    // before the bound of the pixels it may read
    let mut over = image(Protocol::SIXEL, 16, 32, 0);
    over.pixels = vec![0; 16 * 32];
    over.pixels[0] = RED;
    let top = s.add_image(over, None, f.screens()).expect("stored");
    put(&mut s, &f, top, 0, 0);
    assert_eq!(
        shown(&s, &f).iter().map(|p| p.4).collect::<Vec<_>>(),
        [gif, top]
    );
    // with every frame clear it draws nothing and goes
    let mut s = store(1 << 20);
    let gif = animated(&mut s, &f, 0, vec![0; 8 * 16]);
    put(&mut s, &f, gif, 0, 0);
    // larger than the older picture, so the check of its two frames ends
    // before the bound of the pixels it may read
    let mut over = image(Protocol::SIXEL, 16, 32, 0);
    over.pixels = vec![0; 16 * 32];
    over.pixels[0] = RED;
    let top = s.add_image(over, None, f.screens()).expect("stored");
    put(&mut s, &f, top, 0, 0);
    assert_eq!(shown(&s, &f).iter().map(|p| p.4).collect::<Vec<_>>(), [top]);
}

// covers: image.iterm2
#[test]
fn frames_reserved_ahead_of_their_image_free_other_animations_and_split_or_go_back() {
    let f = Fake::new(4);
    let mut s = store(1 << 20);
    let other = animated(&mut s, &f, 0xffff_0000, vec![0xffff_0000; 8 * 16]);
    assert!(s.frame_bytes() > 0);
    // the room counts what freeing the other animation would give back
    assert_eq!(s.frame_room(), s.frame_limit());
    let all = s.frames_ahead(s.frame_limit()).expect("reserved");
    assert_eq!(
        s.animation(other).unwrap().frames,
        1,
        "its frames were freed"
    );
    assert_eq!(s.frame_bytes(), s.frame_limit());
    assert_eq!(s.frames_ahead(1), Err(Refusal::NoSpace));
    // given back whole
    s.unreserve(all);
    assert_eq!(s.frame_bytes(), 0);
    // split onto a new image, the rest given back
    let key = add(&mut s, &f, Protocol::SIXEL, 8, 16, 0);
    let ahead = s.frames_ahead(1000).unwrap();
    let parts = s.split_frames(ahead, key, 300, 3).expect("split");
    assert_eq!(parts.len(), 3);
    assert_eq!(s.frame_bytes(), 900);
    for r in parts {
        s.unreserve(r);
    }
    assert_eq!(s.frame_bytes(), 0);
    // too little to split: everything goes back
    let ahead = s.frames_ahead(500).unwrap();
    assert!(s.split_frames(ahead, key, 300, 2).is_none());
    assert_eq!(s.frame_bytes(), 0);
}
