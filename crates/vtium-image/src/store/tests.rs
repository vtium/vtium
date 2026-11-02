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
        for p in buf.placements.values() {
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
    assert_eq!(s.bytes(), 8 * 32 * 4 + IMAGE_OVERHEAD);
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
    assert_eq!(s.bytes(), 8 * 48 * 4 + IMAGE_OVERHEAD);
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
}
