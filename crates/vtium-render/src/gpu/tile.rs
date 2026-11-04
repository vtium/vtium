//! The tiler (ADR-1303 D2): an image larger than one texture holds, as a
//! grid of tiles.
//!
//! One function serves every image the GPU path holds in tiles: the upright
//! frame, whose overlap is twice the spatial step's reach, and a picture
//! past the GPU's largest texture, whose overlap is twice its filter's
//! reach. An image that fits one tile is one tile, and takes the same path
//! as many.

use super::gl::Size;
use crate::target::Rect;

/// One tile of an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    /// The part of the image its texture holds, the overlaps included.
    pub held: Rect,
    /// The part of the image it owns, which is drawn from it: up to the
    /// middle of each overlap it shares with a neighbour. The owned parts
    /// of an image's tiles cover it once.
    pub owned: Rect,
}

/// One axis of the tiles: where each tile starts and ends, and where the
/// part it owns starts and ends.
fn spans(length: u32, largest: u32, overlap: u32) -> Option<Vec<(u32, u32, u32, u32)>> {
    if length == 0 || largest == 0 {
        return None;
    }
    if length <= largest {
        return Some(vec![(0, length, 0, length)]);
    }
    // a tile no larger than its overlap never moves on
    let step = largest.checked_sub(overlap).filter(|&s| s > 0)?;
    let mut held = Vec::new();
    let mut start = 0u32;
    loop {
        let end = start.saturating_add(largest).min(length);
        held.push((start, end));
        if end == length {
            break;
        }
        start = start.saturating_add(step);
    }
    // a tile owns from the middle of the overlap before it to the middle
    // of the overlap after it
    let half = overlap / 2;
    let owned_start = |k: usize| match k {
        0 => 0,
        _ => held.get(k).map_or(length, |&(s, _)| s.saturating_add(half)),
    };
    Some(
        (0..held.len())
            .map(|k| {
                let (s, e) = held.get(k).copied().unwrap_or_default();
                let next = if k.saturating_add(1) == held.len() {
                    length
                } else {
                    owned_start(k.saturating_add(1))
                };
                (s, e, owned_start(k), next)
            })
            .collect(),
    )
}

/// The tiles of an image of `size`, each at most `largest`, neighbours
/// sharing `overlap` pixels, row by row from the top left. `None` when the
/// image or the largest tile is empty, or when a tile past one would be no
/// larger than its overlap.
pub fn tiles(size: Size, largest: Size, overlap: u32) -> Option<Vec<Tile>> {
    let across = spans(size.width, largest.width, overlap)?;
    let down = spans(size.height, largest.height, overlap)?;
    let mut out = Vec::with_capacity(across.len().saturating_mul(down.len()));
    for &(y0, y1, oy0, oy1) in &down {
        for &(x0, x1, ox0, ox1) in &across {
            out.push(Tile {
                held: Rect {
                    x: x0,
                    y: y0,
                    width: x1.saturating_sub(x0),
                    height: y1.saturating_sub(y0),
                },
                owned: Rect {
                    x: ox0,
                    y: oy0,
                    width: ox1.saturating_sub(ox0),
                    height: oy1.saturating_sub(oy0),
                },
            });
        }
    }
    Some(out)
}

/// Where `a` and `b` meet, when they do.
pub(crate) fn meet(a: Rect, b: Rect) -> Option<Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = a.x.saturating_add(a.width).min(b.x.saturating_add(b.width));
    let y1 =
        a.y.saturating_add(a.height)
            .min(b.y.saturating_add(b.height));
    (x0 < x1 && y0 < y1).then(|| Rect {
        x: x0,
        y: y0,
        width: x1.saturating_sub(x0),
        height: y1.saturating_sub(y0),
    })
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// How many of `tiles` own each pixel of an image of `size`.
    fn owners(size: Size, tiles: &[Tile]) -> Vec<u32> {
        let mut n = vec![0u32; (size.width * size.height) as usize];
        for t in tiles {
            let o = t.owned;
            for y in o.y..o.y + o.height {
                for x in o.x..o.x + o.width {
                    n[(y * size.width + x) as usize] += 1;
                }
            }
        }
        n
    }

    #[test]
    fn an_image_that_fits_is_one_tile() {
        let size = Size::new(1920, 1080);
        let t = tiles(size, Size::new(4096, 4096), 0).unwrap();
        assert_eq!(
            t,
            vec![Tile {
                held: size.rect(),
                owned: size.rect(),
            }]
        );
        // one that fits exactly too
        assert_eq!(tiles(size, size, 6).unwrap().len(), 1);
    }

    #[test]
    fn tiles_without_overlap_meet_edge_to_edge() {
        // a 7680x2160 display on a GPU whose largest texture is 4096
        let t = tiles(Size::new(7680, 2160), Size::new(4096, 4096), 0).unwrap();
        assert_eq!(
            t.iter().map(|t| t.held).collect::<Vec<_>>(),
            vec![rect(0, 0, 4096, 2160), rect(4096, 0, 3584, 2160)]
        );
        assert!(t.iter().all(|t| t.owned == t.held));
    }

    #[test]
    fn each_pixel_is_owned_once_and_held_by_its_owner() {
        for (size, largest, overlap) in [
            (Size::new(100, 37), Size::new(16, 16), 0),
            (Size::new(100, 37), Size::new(16, 10), 4),
            (Size::new(33, 33), Size::new(8, 8), 5),
            (Size::new(64, 1), Size::new(7, 7), 2),
        ] {
            let t = tiles(size, largest, overlap).unwrap();
            assert!(owners(size, &t).iter().all(|&n| n == 1), "{size:?}");
            for tile in &t {
                assert!(tile.held.width <= largest.width && tile.held.height <= largest.height);
                assert_eq!(meet(tile.held, tile.owned), Some(tile.owned), "{tile:?}");
            }
        }
    }

    #[test]
    fn an_overlap_of_twice_a_reach_holds_the_reach_around_every_owned_pixel() {
        // the upright frame's overlap for a spatial step that reads `reach`
        // pixels on each side of the pixel it writes
        for reach in 0..4u32 {
            let size = Size::new(50, 23);
            let t = tiles(size, Size::new(12, 9), reach * 2).unwrap();
            for tile in &t {
                let (h, o) = (tile.held, tile.owned);
                // inside the image, the reach around the owned part is held
                assert!(o.x - h.x >= reach || o.x == 0, "{tile:?}");
                assert!(o.y - h.y >= reach || o.y == 0, "{tile:?}");
                let right = |r: Rect| r.x + r.width;
                let bottom = |r: Rect| r.y + r.height;
                assert!(right(h) - right(o) >= reach || right(o) == 50, "{tile:?}");
                assert!(
                    bottom(h) - bottom(o) >= reach || bottom(o) == 23,
                    "{tile:?}"
                );
            }
        }
    }

    #[test]
    fn a_tile_no_larger_than_its_overlap_is_refused() {
        assert_eq!(tiles(Size::new(100, 10), Size::new(8, 8), 8), None);
        assert_eq!(tiles(Size::new(0, 10), Size::new(8, 8), 0), None);
        assert_eq!(tiles(Size::new(10, 10), Size::new(0, 8), 0), None);
        // an image that fits needs no step
        assert!(tiles(Size::new(8, 8), Size::new(8, 8), 8).is_some());
    }
}
