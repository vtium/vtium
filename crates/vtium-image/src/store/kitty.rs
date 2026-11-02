//! The store operation only the kitty graphics protocol makes: kitty's
//! delete command, `a=d`, with each of its selectors (ADR-1100, D3, D6).
//!
//! A delete removes the kitty placements its selector chooses, in the
//! buffer in use, and never touches a sixel. The lowercase selectors keep
//! each image's data, so a later put can show it again; the uppercase ones
//! also free each image the delete touched that is left with no placement
//! anywhere, the scrollback included. An image named by id, number or id
//! range is touched; with a placement id given too, it is touched only
//! when it lost that placement.
//!
//! Where a selector chooses placements decides which it can reach. `a`,
//! `c`, `p`, `q` and `y` choose by the cells a placement covers on the
//! screen's own rows, or its rows reaching up into the scrollback from
//! them; `i`, `n`, `r`, `x` and `z` choose from the whole buffer, the
//! scrollback included. The C reference chose the same way (its
//! `screen_kitty_delete`, after Ghostty's reading of the spec).

use std::collections::BTreeSet;

use super::{Edit, Reservation, Screens, Store};
use crate::image::{ImageKey, PlacementKey, Refusal};
use crate::rules::Protocol;
use crate::rules::rules;

/// What a delete chooses, from kitty's `d` key and the keys it reads.
/// Columns and rows count from 0 at the screen's top left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Select {
    /// `d=a`: every placement with a row on the screen.
    All,
    /// `d=i`: the image of this id, or only its placement of this
    /// placement id when that is not 0.
    Id { id: u32, placement: u32 },
    /// `d=n`: the newest image of this number, or only its placement of
    /// this placement id when that is not 0.
    Number { number: u32, placement: u32 },
    /// `d=c`, `d=p`: every placement covering this cell.
    Cell { col: u32, row: u32 },
    /// `d=q`: every placement covering this cell at this z.
    CellZ { col: u32, row: u32, z: i32 },
    /// `d=r`: every image whose id lies from `from` to `to`, both included.
    Range { from: u32, to: u32 },
    /// `d=x`: every placement covering this column, on any row.
    Column(u32),
    /// `d=y`: every placement with a row on the screen covering this row.
    Row(u32),
    /// `d=z`: every placement at this z.
    Z(i32),
    /// A selector whose coordinate was not given: it chooses nothing.
    Nothing,
}

/// A kitty delete: what it chooses, and whether it frees the data of the
/// images it leaves with no placement (an uppercase `d`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Delete {
    pub(crate) select: Select,
    pub(crate) free: bool,
}

impl Store {
    /// The kitty id of image `key` of the buffer in use, 0 for an image
    /// sent with neither an id nor a number.
    pub(crate) fn kitty_id(&self, key: ImageKey) -> Option<u32> {
        let image = self.image(self.active(), key)?;
        Some(if rules(image.protocol.0).ids {
            image.id
        } else {
            0
        })
    }

    /// Runs a kitty delete on the buffer in use, whose screen shows
    /// `screens`'s rows.
    pub(crate) fn kitty_delete(&mut self, delete: Delete, screens: Screens<'_>) {
        let b = self.active();
        let rows = screens.of(b);
        let height = rows.height();
        // the images a selector by image names
        let chosen: BTreeSet<ImageKey> = match delete.select {
            Select::Id { id, .. } if id != 0 => self
                .buffers
                .get(b)
                .and_then(|buf| buf.find_id(id))
                .into_iter()
                .collect(),
            Select::Number { number, .. } => self.find_image(0, number).into_iter().collect(),
            Select::Range { from, to } => self
                .buffers
                .get(b)
                .map(|buf| {
                    buf.images
                        .iter()
                        .filter(|(_, i)| {
                            rules(i.protocol.0).ids && i.id != 0 && (from..=to).contains(&i.id)
                        })
                        .map(|(&k, _)| k)
                        .collect()
                })
                .unwrap_or_default(),
            _ => BTreeSet::new(),
        };
        // an image named without a placement id is touched whether or not
        // it loses a placement
        let by_placement = match delete.select {
            Select::Id { placement, .. } | Select::Number { placement, .. } => placement != 0,
            _ => false,
        };
        let mut touched: BTreeSet<ImageKey> = if by_placement {
            BTreeSet::new()
        } else {
            chosen.clone()
        };
        let tops = self.tops(b, rows);
        let Some(buf) = self.buffers.get(b) else {
            return;
        };
        let visited = buf
            .images
            .len()
            .saturating_mul(2)
            .saturating_add(buf.placements.len().saturating_mul(2));
        let mut hits: Vec<(PlacementKey, ImageKey)> = Vec::new();
        for (&k, p) in &buf.placements {
            if !rules(p.protocol.0).deleted_by_kitty {
                continue;
            }
            let top = tops.get(&k).copied();
            let (col, cols) = (i64::from(p.col), i64::from(p.cols));
            let on_screen =
                top.is_some_and(|t| t.saturating_add(i64::from(p.rows)) > 0 && t < height);
            let covers = |c: u32, r: u32| {
                let (c, r) = (i64::from(c), i64::from(r));
                top.is_some_and(|t| r >= t && r < t.saturating_add(i64::from(p.rows)))
                    && c >= col
                    && c < col.saturating_add(cols)
            };
            let hit = match delete.select {
                Select::All => on_screen,
                Select::Id { placement, .. } | Select::Number { placement, .. } => {
                    chosen.contains(&p.image) && (placement == 0 || p.id == placement)
                }
                Select::Range { .. } => chosen.contains(&p.image),
                Select::Cell { col: c, row: r } => covers(c, r),
                Select::CellZ { col: c, row: r, z } => covers(c, r) && p.z == z,
                Select::Column(c) => {
                    let c = i64::from(c);
                    c >= col && c < col.saturating_add(cols)
                }
                Select::Row(r) => on_screen && covers(p.col, r),
                Select::Z(z) => p.z == z,
                Select::Nothing => false,
            };
            if hit {
                hits.push((k, p.image));
            }
        }
        self.count(visited);
        for (k, image) in hits {
            touched.insert(image);
            self.free_placement(b, k, false);
        }
        if !delete.free {
            return;
        }
        for image in touched {
            let bare = self
                .image(b, image)
                .is_some_and(|i| i.placements == 0 && i.hold == 0);
            if bare {
                self.free_image(b, image);
            }
        }
    }
}

impl Edit<'_> {
    /// A buffer to decode a kitty picture of `len` pixels into, and the
    /// reservation of its whole charge, `bytes`, for an image of kitty id
    /// `id` (D5). When the store's spare buffer, the one the image it freed
    /// last left, holds `len` pixels and no frame holds it, the buffer comes
    /// still holding that image's pixels, which the caller overwrites, with
    /// its charge as the reservation, grown by the rest of `bytes`, so no reservation of the handler's own can evict the
    /// buffer it is about to fill; otherwise the buffer is new and the whole
    /// charge is reserved. The players that send video as kitty pictures
    /// replace each frame with one of its size: ncplayer deletes the
    /// previous frame's id after placing the new one, and timg's two ids
    /// alternate, so the frame freed last is the spare when the next frame
    /// begins, and its memory, already mapped, serves that frame where the
    /// budget has room for the spare. Which buffer is reused changes no
    /// pixel and nothing the store compares.
    pub(crate) fn kitty_buffer(
        &mut self,
        id: u32,
        len: usize,
        bytes: u64,
    ) -> Result<(Vec<u32>, Reservation), Refusal> {
        if let Some((buffer, mut reservation)) = self.take_spare(Protocol::KITTY, id, len) {
            let more = bytes.saturating_sub(reservation.bytes());
            if self.grow(&mut reservation, more).is_ok() {
                return Ok((buffer, reservation));
            }
            // the buffer goes with its charge, and a reservation of the
            // whole cost may still fit where the growth did not
            self.unreserve(reservation);
        }
        let reservation = self.reserve(Protocol::KITTY, id, bytes)?;
        Ok((Vec::new(), reservation))
    }

    /// Runs a kitty delete on the buffer in use.
    pub(crate) fn kitty_delete(&mut self, delete: Delete) {
        self.store.kitty_delete(delete, self.screens);
    }
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::image::{LineKey, NewImage, Place};
    use crate::rules::Protocol;
    use crate::store::Rows;

    const COLS: usize = 12;

    /// A screen of four rows, keys 10 up, and two scrollback rows above it,
    /// keys 1 and 0, newest first.
    struct Fake {
        screen: Vec<LineKey>,
        above: Vec<LineKey>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                screen: (10..14).map(LineKey::new).collect(),
                above: vec![LineKey::new(1), LineKey::new(0)],
            }
        }

        fn screens(&self) -> Screens<'_> {
            Screens {
                main: Rows {
                    screen: &self.screen,
                    above: &self.above,
                },
                alt: Rows::default(),
            }
        }
    }

    fn image(s: &mut Store, f: &Fake, protocol: Protocol, id: u32, number: u32) -> ImageKey {
        let new = NewImage {
            protocol,
            width: 16,
            height: 16,
            pixels: vec![0xff00_ff00; 256],
            id,
            number,
            transient: false,
            opaque: None,
        };
        s.add_image(new, None, f.screens()).expect("stored")
    }

    /// Places `img` as a 2 by 1 box at `col`, `row`, at `z`, with
    /// placement id `id`.
    fn put(s: &mut Store, f: &Fake, img: ImageKey, col: u32, row: u32, z: i32, id: u32) {
        let place = Place {
            col,
            row,
            z,
            id,
            ..Place::default()
        };
        s.place(img, place, f.screens(), COLS).expect("placed");
    }

    /// Three kitty images, 1, 2 and 3, each placed at row 0, columns 0, 4
    /// and 8, at z 1, 2 and 3, with placement id 1; image 2 also at row 2
    /// with placement id 2; and a sixel at row 1, column 0.
    fn three() -> (Store, Fake, [ImageKey; 4]) {
        let f = Fake::new();
        let mut s = Store::new();
        s.set_limit(1 << 20);
        let a = image(&mut s, &f, Protocol::KITTY, 1, 7);
        let b = image(&mut s, &f, Protocol::KITTY, 2, 7);
        let c = image(&mut s, &f, Protocol::KITTY, 3, 0);
        let six = image(&mut s, &f, Protocol::SIXEL, 0, 0);
        put(&mut s, &f, a, 0, 0, 1, 1);
        put(&mut s, &f, b, 4, 0, 2, 1);
        put(&mut s, &f, b, 4, 2, 2, 2);
        put(&mut s, &f, c, 8, 0, 3, 1);
        put(&mut s, &f, six, 0, 1, 0, 0);
        (s, f, [a, b, c, six])
    }

    /// Each placement as (image, col, row), and whether each image stays.
    fn left(s: &mut Store, f: &Fake, keys: &[ImageKey; 4]) -> (Vec<(u64, u32, i64)>, [bool; 4]) {
        let tops = s.tops(0, f.screens().main);
        let buf = s.buffers.first().expect("main");
        let placed = buf
            .placements
            .iter()
            .map(|(k, p)| {
                (
                    p.image.get(),
                    p.col,
                    tops.get(k).copied().unwrap_or(i64::MIN),
                )
            })
            .collect();
        let stays = keys.map(|k| buf.images.contains_key(&k));
        (placed, stays)
    }

    fn run(select: Select, free: bool) -> (Vec<(u64, u32, i64)>, [bool; 4]) {
        let (mut s, f, keys) = three();
        s.kitty_delete(Delete { select, free }, f.screens());
        left(&mut s, &f, &keys)
    }

    // covers: image.kitty
    #[test]
    fn d_a_removes_every_kitty_placement_on_the_screen_and_keeps_the_sixel() {
        let (placed, stays) = run(Select::All, false);
        assert_eq!(placed, vec![(3, 0, 1)]);
        assert_eq!(stays, [true; 4]);
        let (_, stays) = run(Select::All, true);
        assert_eq!(stays, [false, false, false, true]);
    }

    #[test]
    fn d_a_leaves_a_placement_that_has_scrolled_off_the_screen() {
        let (mut s, _, _) = three();
        // the screen scrolls four rows: every placement is in the scrollback
        let scrolled = Fake {
            screen: (14..18).map(LineKey::new).collect(),
            above: (10..14).rev().map(LineKey::new).collect(),
        };
        s.kitty_delete(
            Delete {
                select: Select::All,
                free: true,
            },
            scrolled.screens(),
        );
        assert_eq!(s.placement_count(), 5);
        // d=x reaches the scrollback, and takes image 2's two placements
        s.kitty_delete(
            Delete {
                select: Select::Column(4),
                free: false,
            },
            scrolled.screens(),
        );
        assert_eq!(s.placement_count(), 3);
    }

    #[test]
    fn d_i_with_a_placement_id_touches_only_the_image_that_lost_it() {
        let (placed, stays) = run(
            Select::Id {
                id: 2,
                placement: 2,
            },
            true,
        );
        assert_eq!(placed.len(), 4);
        assert!(!placed.contains(&(1, 4, 2)));
        assert_eq!(stays, [true; 4]);
        let (placed, stays) = run(
            Select::Id {
                id: 2,
                placement: 0,
            },
            true,
        );
        assert_eq!(placed.len(), 3);
        assert_eq!(stays, [true, false, true, true]);
    }

    #[test]
    fn d_n_takes_the_newest_image_of_its_number() {
        let (placed, stays) = run(
            Select::Number {
                number: 7,
                placement: 0,
            },
            true,
        );
        assert_eq!(placed, vec![(0, 0, 0), (2, 8, 0), (3, 0, 1)]);
        assert_eq!(stays, [true, false, true, true]);
    }

    // covers: image.kitty
    #[test]
    fn cell_selectors_choose_by_the_cells_a_placement_covers() {
        let (placed, _) = run(Select::Cell { col: 5, row: 0 }, false);
        assert_eq!(placed.len(), 4);
        assert!(!placed.contains(&(1, 4, 0)));
        // the sixel at 0, 1 is never touched
        let (placed, _) = run(Select::Cell { col: 0, row: 1 }, false);
        assert_eq!(placed.len(), 5);
        let (placed, _) = run(
            Select::CellZ {
                col: 8,
                row: 0,
                z: 2,
            },
            false,
        );
        assert_eq!(placed.len(), 5);
        let (placed, _) = run(
            Select::CellZ {
                col: 8,
                row: 0,
                z: 3,
            },
            false,
        );
        assert_eq!(placed.len(), 4);
    }

    #[test]
    fn rows_columns_z_and_id_ranges_each_choose_their_own() {
        assert_eq!(run(Select::Row(2), false).0.len(), 4);
        assert_eq!(run(Select::Column(5), false).0.len(), 3);
        assert_eq!(run(Select::Z(2), false).0.len(), 3);
        let (placed, stays) = run(Select::Range { from: 2, to: 3 }, true);
        assert_eq!(placed, vec![(0, 0, 0), (3, 0, 1)]);
        assert_eq!(stays, [true, false, false, true]);
        assert_eq!(run(Select::Nothing, true).0.len(), 5);
    }

    #[test]
    fn an_uppercase_delete_frees_an_unplaced_image_it_names() {
        let (mut s, f, keys) = three();
        let lone = image(&mut s, &f, Protocol::KITTY, 4, 0);
        s.kitty_delete(
            Delete {
                select: Select::Id {
                    id: 4,
                    placement: 0,
                },
                free: false,
            },
            f.screens(),
        );
        assert!(s.buffers.first().expect("main").images.contains_key(&lone));
        s.kitty_delete(
            Delete {
                select: Select::Id {
                    id: 4,
                    placement: 9,
                },
                free: true,
            },
            f.screens(),
        );
        assert!(s.buffers.first().expect("main").images.contains_key(&lone));
        s.kitty_delete(
            Delete {
                select: Select::Id {
                    id: 4,
                    placement: 0,
                },
                free: true,
            },
            f.screens(),
        );
        assert!(!s.buffers.first().expect("main").images.contains_key(&lone));
        assert_eq!(left(&mut s, &f, &keys).0.len(), 5);
    }

    #[test]
    fn a_placement_reaching_up_into_the_scrollback_is_chosen_by_its_screen_cells() {
        let f = Fake::new();
        let mut s = Store::new();
        s.set_limit(1 << 20);
        let tall = NewImage {
            protocol: Protocol::KITTY,
            width: 8,
            height: 48,
            pixels: vec![0xff00_00ff; 8 * 48],
            id: 5,
            number: 0,
            transient: false,
            opaque: None,
        };
        let key = s.add_image(tall, None, f.screens()).expect("stored");
        put(&mut s, &f, key, 0, 0, 0, 0);
        // the screen scrolls two rows: the placement's top is now row -2
        let f = Fake {
            screen: vec![
                LineKey::new(12),
                LineKey::new(13),
                LineKey::new(14),
                LineKey::new(15),
            ],
            above: vec![LineKey::new(11), LineKey::new(10)],
        };
        s.kitty_delete(
            Delete {
                select: Select::Cell { col: 0, row: 1 },
                free: false,
            },
            f.screens(),
        );
        assert_eq!(s.placement_count(), 1, "row 1 is below the box");
        s.kitty_delete(
            Delete {
                select: Select::Cell { col: 0, row: 0 },
                free: false,
            },
            f.screens(),
        );
        assert_eq!(s.placement_count(), 0);
    }
}
