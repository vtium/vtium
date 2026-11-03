//! The store's side of Unicode placeholders: frame entries made from runs,
//! a virtual parent's children placed by runs, and the changes vtium-term
//! marks (ADR-1100, D13).

#![expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]

use super::*;
use crate::image::{ImageKey, NewImage, Place};
use crate::rules::Protocol;
use crate::store::kitty::{Delete, Select};
use crate::store::{Control, FramePlacement, Link, NewFrame, Screens, layer_cost};

const COLS: usize = 12;

/// A screen of four rows, lines 10 to 13, with no scrollback.
struct Fake {
    screen: Vec<LineKey>,
}

impl Fake {
    fn new() -> Self {
        Self {
            screen: (10..14).map(LineKey::new).collect(),
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

fn store() -> Store {
    let mut s = Store::new();
    s.set_limit(1 << 20);
    s
}

/// A kitty image of id `id`, `w` by `h` pixels, each pixel its own value:
/// its column in the low 16 bits and its row above them, opaque.
fn image(s: &mut Store, f: &Fake, id: u32, w: u32, h: u32) -> ImageKey {
    let pixels = (0..h)
        .flat_map(|y| (0..w).map(move |x| 0xff00_0000 | (y << 8) | x))
        .collect();
    let new = NewImage {
        protocol: Protocol::KITTY,
        width: w,
        height: h,
        pixels,
        opaque: None,
        id,
        number: 0,
        transient: false,
    };
    s.add_image(new, None, f.screens()).expect("stored")
}

/// A virtual placement of `image`, `cols` by `rows` cells, placement id
/// `id`.
fn virtual_placement(s: &mut Store, f: &Fake, image: ImageKey, cols: u32, rows: u32, id: u32) {
    let place = Place {
        cols,
        rows,
        id,
        ..Place::default()
    };
    s.place_at(image, place, Link::Virtual, f.screens(), COLS)
        .expect("placed");
}

fn run(line: u64, col: u32, cells: u32, image: u32, row: u32, column: u32) -> PlaceholderRun {
    PlaceholderRun {
        line: LineKey::new(line),
        col,
        cells,
        image,
        placement: None,
        row,
        column,
    }
}

/// Each entry as (view row, column, box columns, box rows), the drawn part
/// (x, y, width, height) and the source rectangle (x, y, width, height).
type Seen = (
    (i64, u32, u32, u32),
    (u32, u32, u32, u32),
    (u32, u32, u32, u32),
);

fn seen(entries: &[FramePlacement]) -> Vec<Seen> {
    entries
        .iter()
        .map(|p| {
            (
                (p.row, p.col, p.cols, p.rows),
                (p.x, p.y, p.width, p.height),
                (p.src_x, p.src_y, p.src_width, p.src_height),
            )
        })
        .collect()
}

fn frame(s: &Store, f: &Fake, runs: &[PlaceholderRun]) -> Vec<FramePlacement> {
    s.frame_with_runs(f.rows(), &[], runs, COLS)
}

// covers: image.kitty-placeholders
#[test]
fn a_run_draws_its_slice_of_the_picture() {
    let (f, mut s) = (Fake::new(), store());
    // 16 by 32 pixels fill a box of 2 by 2 cells of 8 by 16 exactly
    let key = image(&mut s, &f, 7, 16, 32);
    virtual_placement(&mut s, &f, key, 2, 2, 0);
    let runs = [
        run(11, 3, 2, 7, 0, 0),
        run(12, 3, 2, 7, 1, 0),
        run(13, 9, 1, 7, 1, 1),
    ];
    let entries = frame(&s, &f, &runs);
    assert_eq!(
        seen(&entries),
        vec![
            ((1, 3, 2, 1), (0, 0, 16, 16), (0, 0, 16, 16)),
            ((2, 3, 2, 1), (0, 0, 16, 16), (0, 16, 16, 16)),
            ((3, 9, 1, 1), (0, 0, 8, 16), (8, 16, 8, 16)),
        ]
    );
    assert!(entries.iter().all(|p| p.image == key && p.kind == "kitty"));
    // every entry has a key of its own within the frame
    let mut keys: Vec<_> = entries.iter().map(|p| p.key).collect();
    keys.dedup();
    assert_eq!(keys.len(), 3);
}

// covers: image.kitty-placeholders
#[test]
fn a_run_shows_the_part_of_the_fitted_picture_under_its_cells() {
    let (f, mut s) = (Fake::new(), store());
    // 8 by 8 pixels fitted to 1 by 3 cells: 8 by 8, centred, rows 20 to 27
    // of the 48 the box has
    let key = image(&mut s, &f, 7, 8, 8);
    virtual_placement(&mut s, &f, key, 1, 3, 0);
    let runs = [
        run(10, 0, 1, 7, 0, 0),
        run(11, 0, 1, 7, 1, 0),
        run(12, 0, 1, 7, 2, 0),
    ];
    // the first and last rows hold none of the picture and draw nothing
    assert_eq!(
        seen(&frame(&s, &f, &runs)),
        vec![((1, 0, 1, 1), (0, 4, 8, 8), (0, 0, 8, 8))]
    );
}

// covers: image.kitty-placeholders
#[test]
fn a_run_whose_image_or_placement_does_not_exist_draws_nothing() {
    let (f, mut s) = (Fake::new(), store());
    let key = image(&mut s, &f, 7, 16, 32);
    virtual_placement(&mut s, &f, key, 2, 2, 5);
    // an image of id 8 with a placement on a line, and no virtual one
    let other = image(&mut s, &f, 8, 8, 16);
    s.place(other, Place::default(), f.screens(), COLS).unwrap();
    let named = |placement| PlaceholderRun {
        placement,
        ..run(11, 0, 2, 7, 0, 0)
    };
    assert_eq!(
        frame(&s, &f, &[run(11, 0, 2, 9, 0, 0)]).len(),
        1,
        "no image 9"
    );
    assert_eq!(frame(&s, &f, &[named(Some(6))]).len(), 1, "no placement 6");
    assert_eq!(
        frame(&s, &f, &[run(11, 0, 2, 8, 0, 0)]).len(),
        1,
        "8 is not virtual"
    );
    assert_eq!(
        frame(&s, &f, &[run(99, 0, 2, 7, 0, 0)]).len(),
        1,
        "line 99 is not shown"
    );
    // the one entry each time is image 8's own placement
    assert_eq!(frame(&s, &f, &[named(Some(5))]).len(), 2);
    assert_eq!(frame(&s, &f, &[named(None)]).len(), 2);
}

// covers: image.kitty-placeholders
#[test]
fn the_cells_of_a_run_outside_the_grid_draw_nothing() {
    let (f, mut s) = (Fake::new(), store());
    let key = image(&mut s, &f, 7, 16, 32);
    virtual_placement(&mut s, &f, key, 2, 2, 0);
    // picture row 2 is past the grid's two rows
    assert!(frame(&s, &f, &[run(10, 0, 2, 7, 2, 0)]).is_empty());
    // columns 1 to 3: only column 1 lies in the grid, at the run's first cell
    assert_eq!(
        seen(&frame(&s, &f, &[run(10, 4, 3, 7, 0, 1)])),
        vec![((0, 4, 1, 1), (0, 0, 8, 16), (8, 0, 8, 16))]
    );
    // a run that runs off the screen's right edge is cut there
    assert_eq!(
        seen(&frame(&s, &f, &[run(10, 11, 2, 7, 0, 0)])),
        vec![((0, 11, 1, 1), (0, 0, 8, 16), (0, 0, 8, 16))]
    );
}

// covers: image.kitty-placeholders
#[test]
fn a_run_with_no_placement_id_takes_the_virtual_placement_made_last() {
    let (f, mut s) = (Fake::new(), store());
    let key = image(&mut s, &f, 7, 16, 32);
    virtual_placement(&mut s, &f, key, 2, 2, 1);
    virtual_placement(&mut s, &f, key, 1, 1, 2);
    // placement 2's grid is one cell: the picture fitted to it is 8 by 16
    assert_eq!(
        seen(&frame(&s, &f, &[run(10, 0, 1, 7, 0, 0)])),
        vec![((0, 0, 1, 1), (0, 0, 8, 16), (0, 0, 16, 32))]
    );
    let one = PlaceholderRun {
        placement: Some(1),
        ..run(10, 0, 1, 7, 0, 0)
    };
    assert_eq!(
        seen(&frame(&s, &f, &[one])),
        vec![((0, 0, 1, 1), (0, 0, 8, 16), (0, 0, 8, 16))]
    );
}

// covers: image.kitty-placeholders
#[test]
fn run_entries_take_their_placements_place_in_the_draw_order() {
    let (f, mut s) = (Fake::new(), store());
    let a = image(&mut s, &f, 1, 8, 16);
    let v = image(&mut s, &f, 2, 16, 32);
    let c = image(&mut s, &f, 3, 8, 16);
    s.place(a, Place::default(), f.screens(), COLS).unwrap();
    virtual_placement(&mut s, &f, v, 2, 2, 0);
    s.place(c, Place::default(), f.screens(), COLS).unwrap();
    // runs handed bottom row first: the entries still draw row by row
    let runs = [run(12, 0, 2, 2, 1, 0), run(11, 0, 2, 2, 0, 0)];
    let order: Vec<(ImageKey, i64)> = frame(&s, &f, &runs)
        .iter()
        .map(|p| (p.image, p.row))
        .collect();
    assert_eq!(order, vec![(a, 0), (v, 1), (v, 2), (c, 0)]);
}

// covers: image.kitty-placeholders
#[test]
fn deleting_a_virtual_placement_empties_its_runs_and_reports_a_change() {
    let (f, mut s) = (Fake::new(), store());
    let key = image(&mut s, &f, 7, 16, 32);
    virtual_placement(&mut s, &f, key, 2, 2, 0);
    assert_eq!(s.take_changed_virtual(), vec![(false, 7)], "made");
    assert_eq!(s.take_changed_virtual(), vec![]);
    let runs = [run(10, 0, 2, 7, 0, 0)];
    assert_eq!(frame(&s, &f, &runs).len(), 1);
    let delete = Delete {
        select: Select::Id {
            id: 7,
            placement: 0,
        },
        free: false,
    };
    s.kitty_delete(delete, f.screens());
    assert!(frame(&s, &f, &runs).is_empty());
    assert_eq!(s.take_changed_virtual(), vec![(false, 7)], "deleted");
    // a replacement by the same placement id, and the image sent again
    virtual_placement(&mut s, &f, key, 2, 2, 4);
    virtual_placement(&mut s, &f, key, 1, 1, 4);
    let _ = image(&mut s, &f, 7, 8, 8);
    assert_eq!(s.take_changed_virtual(), vec![(false, 7)]);
    assert!(
        frame(&s, &f, &runs).is_empty(),
        "the new image has no placement"
    );
    // the alternate buffer's emptying reports its virtual placements
    s.set_alternate(true);
    let alt = image(&mut s, &f, 9, 8, 16);
    virtual_placement(&mut s, &f, alt, 1, 1, 0);
    assert_eq!(s.take_changed_virtual(), vec![(true, 9)], "made");
    s.set_alternate(false);
    assert_eq!(s.take_changed_virtual(), vec![(true, 9)], "emptied");
}

// covers: image.kitty-placeholders
#[test]
fn an_animated_images_run_shows_its_current_frame() {
    let (f, mut s) = (Fake::new(), store());
    let key = image(&mut s, &f, 7, 16, 32);
    virtual_placement(&mut s, &f, key, 2, 2, 0);
    let data = NewFrame {
        x: 0,
        y: 0,
        width: 16,
        height: 32,
        pixels: vec![0xff00_00ff; 16 * 32],
        opaque: true,
        replace: true,
        base: 0,
        background: 0,
        edit: 0,
        gap: 0,
    };
    let r = s.frame_reserve(key, layer_cost(16, 32)).unwrap();
    assert_eq!(s.add_frame(data, r), Ok(2));
    let runs = [run(10, 0, 2, 7, 0, 0)];
    assert_eq!(frame(&s, &f, &runs)[0].frame, 1);
    s.control_animation(
        key,
        Control {
            current: 2,
            ..Control::default()
        },
    )
    .unwrap();
    let entry = frame(&s, &f, &runs).remove(0);
    let two = s.frame_pixels(key, 2).unwrap();
    assert_eq!((entry.frame, entry.pixel_revision), (2, two.revision));
    assert!(entry.pixels.same(&two.pixels));
    assert_eq!(entry.animation, s.animation(key));
}

// covers: image.kitty-placeholders
#[test]
fn a_frame_takes_at_most_a_run_for_each_cell_it_shows() {
    let (f, mut s) = (Fake::new(), store());
    let key = image(&mut s, &f, 7, 8, 16);
    virtual_placement(&mut s, &f, key, 1, 1, 0);
    // the view's 48 cells take 48 runs; the runs past them are left
    let mut runs = vec![run(10, 0, 1, 9, 0, 0); 48];
    runs.push(run(10, 0, 1, 7, 0, 0));
    assert!(frame(&s, &f, &runs).is_empty());
    runs.swap(0, 48);
    assert_eq!(frame(&s, &f, &runs).len(), 1);
    // a decision takes as many, and counts each as input work
    let many = vec![run(10, 0, 1, 9, 0, 0); 10_000];
    let _ = s.take_work();
    s.with_runs([&many, &[]], f.screens(), COLS, |s| s.add_work(0));
    let work = s.take_work();
    assert!((48..=96).contains(&work), "work {work}");
}

/// Image 7, 16 by 32 pixels, as a virtual placement of 2 by 2 cells with
/// placement id 1, and image 8, 8 by 16, a child of it made with `H=1` and
/// `V=1`.
fn family(s: &mut Store, f: &Fake) -> (ImageKey, ImageKey) {
    let parent = image(s, f, 7, 16, 32);
    virtual_placement(s, f, parent, 2, 2, 1);
    let child = image(s, f, 8, 8, 16);
    let link = Link::Parent {
        image: 7,
        placement: 1,
        cols: 1,
        rows: 1,
    };
    s.place_at(child, Place::default(), link, f.screens(), COLS)
        .expect("placed");
    (parent, child)
}

// covers: image.kitty-placeholders
#[test]
fn a_virtual_parents_children_draw_from_the_origin_its_runs_give() {
    let (f, mut s) = (Fake::new(), store());
    assert!(!s.has_virtual_children(false));
    let (_, child) = family(&mut s, &f);
    assert!(s.has_virtual_children(false));
    assert!(!s.has_virtual_children(true));
    let child_at = |runs: &[PlaceholderRun]| -> Vec<(i64, u32)> {
        frame(&s, &f, runs)
            .iter()
            .filter(|p| p.image == child)
            .map(|p| (p.row, p.col))
            .collect()
    };
    // the parent's cell at row 2, column 4 shows picture row 1, picture
    // column 1: the origin is row 1, column 3, and the child one cell right
    // and down of it
    assert_eq!(child_at(&[run(12, 4, 1, 7, 1, 1)]), vec![(2, 4)]);
    // over several runs the origin is the least row and the least column
    let runs = [run(12, 4, 1, 7, 1, 1), run(13, 9, 1, 7, 1, 1)];
    assert_eq!(child_at(&runs), vec![(2, 4)]);
    // with no runs of the parent, the child draws nothing
    assert!(frame(&s, &f, &[]).is_empty());
}

// covers: image.kitty-placeholders
#[test]
fn a_delete_by_cell_takes_a_virtual_parents_child_by_the_runs_handed_with_it() {
    let (f, mut s) = (Fake::new(), store());
    family(&mut s, &f);
    let runs = [run(11, 3, 2, 7, 0, 0)];
    let delete = |col, row| Delete {
        select: Select::Cell { col, row },
        free: false,
    };
    // the child's cell is row 2, column 4; a delete of another cell, or
    // one with no runs handed, takes nothing
    s.with_runs([&runs, &[]], f.screens(), COLS, |s| {
        s.kitty_delete(delete(5, 2), f.screens())
    });
    s.kitty_delete(delete(4, 2), f.screens());
    assert_eq!(s.placement_count(), 2);
    s.with_runs([&runs, &[]], f.screens(), COLS, |s| {
        s.kitty_delete(delete(4, 2), f.screens())
    });
    assert_eq!(s.placement_count(), 1);
    // the runs are not kept past the call
    assert!(s.runs.iter().all(Vec::is_empty));
}

// covers: image.kitty-placeholders
#[test]
fn the_changes_nobody_takes_stay_within_the_images_the_store_can_hold() {
    let (f, mut s) = (Fake::new(), store());
    let bound = usize::try_from(crate::IMAGES_PER_BUFFER).unwrap() * 2;
    // a program that sends image k, places it virtually and deletes it,
    // for one id after another, with nobody taking the changes
    for id in 1..=u32::try_from(bound).unwrap() + 100 {
        let key = image(&mut s, &f, id, 1, 1);
        virtual_placement(&mut s, &f, key, 1, 1, 0);
        let delete = Delete {
            select: Select::Id { id, placement: 0 },
            free: true,
        };
        s.kitty_delete(delete, f.screens());
    }
    assert_eq!(s.image_count(), 0);
    assert_eq!(s.take_changed_virtual().len(), bound);
}

// covers: image.kitty-placeholders
#[test]
fn ed_2_and_d_a_take_a_virtual_parents_child_by_the_runs_handed_with_them() {
    let runs = [run(11, 3, 2, 7, 0, 0)];
    // ED 2: the child, at row 2, column 4, has a row on the screen's own
    // lines only by the runs
    let (f, mut s) = (Fake::new(), store());
    family(&mut s, &f);
    s.erase_screen(f.rows());
    assert_eq!(s.placement_count(), 2, "no runs: the child has no cells");
    s.with_runs([&runs, &[]], f.screens(), COLS, |s| s.erase_screen(f.rows()));
    assert_eq!(s.placement_count(), 1, "the virtual parent stays");
    // d=a: every placement on the screen
    let (f, mut s) = (Fake::new(), store());
    family(&mut s, &f);
    let all = Delete {
        select: Select::All,
        free: false,
    };
    s.kitty_delete(all, f.screens());
    assert_eq!(s.placement_count(), 2);
    s.with_runs([&runs, &[]], f.screens(), COLS, |s| s.kitty_delete(all, f.screens()));
    assert_eq!(s.placement_count(), 1);
}
