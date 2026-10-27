//! One image comparison for tiers 2 and 3 (aaronsb/kmscon#18): exact
//! outside the masks, the verdict under a tolerance, the clustered regions
//! of what differs, and a diff image. It is tier 3's `t3img.py` compare,
//! carried over: its verdicts, its end-exclusive masks and its regions.

use crate::ppm::Image;
use crate::tol::{Rect, Tolerance};

/// The most regions a comparison reports.
pub const REGIONS_MAX: usize = 16;
/// The side of the tiles differing pixels are grouped by, at first.
pub const TILE: u32 = 16;

/// The verdict on a comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing differs outside the masks.
    Pass,
    /// Something differs, within the tolerance's both limits.
    Near,
    /// Something differs past the tolerance, or the sizes differ.
    Fail,
}

impl Verdict {
    pub fn name(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Near => "near",
            Verdict::Fail => "fail",
        }
    }
}

/// What a comparison found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub verdict: Verdict,
    /// Pixels that differ outside the masks; every pixel of the larger
    /// image when the sizes differ.
    pub pixels: u64,
    /// The largest per-channel difference among them; 255 when the sizes
    /// differ.
    pub max_delta: u8,
    /// At most [`REGIONS_MAX`] rectangles, end-exclusive, holding them.
    pub regions: Vec<Rect>,
    /// The golden's and the actual image's sizes, when they differ.
    pub size_mismatch: Option<((u32, u32), (u32, u32))>,
    /// For each mask, the differing pixels it hid: a mask that hides
    /// nothing is stale.
    pub masked: Vec<u64>,
    /// The points that differ outside the masks.
    pub points: Vec<(u32, u32)>,
}

/// Compares `actual` with `golden` under `tol`. A mask reaching past the
/// golden is an error, never a mask that silently covers nothing.
pub fn compare(golden: &Image, actual: &Image, tol: &Tolerance) -> Result<Comparison, String> {
    let (gw, gh) = (golden.width(), golden.height());
    if let Some(m) = tol.masks.iter().find(|m| m.x1 > gw || m.y1 > gh) {
        return Err(format!("mask {m} reaches past the {gw}x{gh} golden"));
    }
    let mut masked = vec![0u64; tol.masks.len()];
    if (actual.width(), actual.height()) != (gw, gh) {
        let (w, h) = (gw.max(actual.width()), gh.max(actual.height()));
        return Ok(Comparison {
            verdict: Verdict::Fail,
            pixels: u64::from(w) * u64::from(h),
            max_delta: 255,
            regions: vec![Rect {
                x0: 0,
                y0: 0,
                x1: w,
                y1: h,
            }],
            size_mismatch: Some(((gw, gh), (actual.width(), actual.height()))),
            masked,
            points: Vec::new(),
        });
    }
    let mut points = Vec::new();
    let mut max_delta = 0u8;
    for y in 0..gh {
        let (rg, ra) = (golden.row(y), actual.row(y));
        if rg == ra {
            continue;
        }
        for (x, (pg, pa)) in (0..gw).zip(rg.chunks_exact(3).zip(ra.chunks_exact(3))) {
            if pg == pa {
                continue;
            }
            if let Some(i) = tol.masks.iter().position(|m| m.contains(x, y)) {
                masked[i] += 1;
                continue;
            }
            points.push((x, y));
            let d = pg.iter().zip(pa).map(|(a, b)| a.abs_diff(*b)).max();
            max_delta = max_delta.max(d.unwrap_or(0));
        }
    }
    let pixels = points.len() as u64;
    let verdict = if pixels == 0 {
        Verdict::Pass
    } else if pixels <= tol.max_pixels && max_delta <= tol.max_delta {
        Verdict::Near
    } else {
        Verdict::Fail
    };
    Ok(Comparison {
        verdict,
        pixels,
        max_delta,
        regions: regions(&points),
        size_mismatch: None,
        masked,
        points,
    })
}

/// The diff image: `actual` dimmed to a third, the masks tinted blue and
/// each differing pixel red.
pub fn diff_image(actual: &Image, tol: &Tolerance, points: &[(u32, u32)]) -> Image {
    let w = actual.width();
    let mut rgb: Vec<u8> = actual.rgb().iter().map(|b| b / 3).collect();
    for m in &tol.masks {
        for y in m.y0..m.y1.min(actual.height()) {
            for x in m.x0..m.x1.min(w) {
                let i = (y as usize * w as usize + x as usize) * 3 + 2;
                rgb[i] = rgb[i].saturating_add(96);
            }
        }
    }
    for &(x, y) in points {
        let i = (y as usize * w as usize + x as usize) * 3;
        rgb[i..i + 3].copy_from_slice(&[255, 0, 0]);
    }
    Image::new(w, actual.height(), rgb).unwrap_or_else(|| actual.clone())
}

fn union(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

/// The Chebyshev distance between two rectangles; 0 when they touch or
/// overlap.
fn gap(a: Rect, b: Rect) -> u32 {
    let dx = a.x0.max(b.x0).saturating_sub(a.x1.min(b.x1));
    let dy = a.y0.max(b.y0).saturating_sub(a.y1.min(b.y1));
    dx.max(dy)
}

/// The boxes of the 8-connected groups of `tile`-sided tiles holding
/// points.
fn cluster(points: &[(u32, u32)], tile: u32) -> Vec<Rect> {
    use std::collections::BTreeMap;
    let mut tiles: BTreeMap<(u32, u32), Rect> = BTreeMap::new();
    for &(x, y) in points {
        let p = Rect {
            x0: x,
            y0: y,
            x1: x + 1,
            y1: y + 1,
        };
        tiles
            .entry((x / tile, y / tile))
            .and_modify(|b| *b = union(*b, p))
            .or_insert(p);
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut boxes = Vec::new();
    for &start in tiles.keys() {
        if !seen.insert(start) {
            continue;
        }
        let mut stack = vec![start];
        let mut b = tiles[&start];
        while let Some((tx, ty)) = stack.pop() {
            b = union(b, tiles[&(tx, ty)]);
            for nx in tx.saturating_sub(1)..=tx.saturating_add(1) {
                for ny in ty.saturating_sub(1)..=ty.saturating_add(1) {
                    if tiles.contains_key(&(nx, ny)) && seen.insert((nx, ny)) {
                        stack.push((nx, ny));
                    }
                }
            }
        }
        boxes.push(b);
    }
    boxes
}

/// At most [`REGIONS_MAX`] boxes around the points: points in touching
/// tiles share a box, the tiles double while there are more than four
/// times as many groups, then the closest boxes merge. Sorted top to
/// bottom, then left to right.
pub fn regions(points: &[(u32, u32)]) -> Vec<Rect> {
    let mut tile = TILE;
    let mut boxes = cluster(points, tile);
    while boxes.len() > 4 * REGIONS_MAX {
        tile *= 2;
        boxes = cluster(points, tile);
    }
    while boxes.len() > REGIONS_MAX {
        let mut best = (u32::MAX, 0, 1);
        for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                let g = gap(boxes[i], boxes[j]);
                if g < best.0 {
                    best = (g, i, j);
                }
            }
        }
        let (_, i, j) = best;
        boxes[i] = union(boxes[i], boxes[j]);
        boxes.remove(j);
    }
    boxes.sort_by_key(|b| (b.y0, b.x0));
    boxes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Image {
        let f = &f;
        let rgb = (0..h)
            .flat_map(|y| (0..w).flat_map(move |x| f(x, y)))
            .collect();
        Image::new(w, h, rgb).unwrap()
    }

    #[test]
    fn identical_images_pass() {
        let a = img(4, 3, |x, y| [x as u8, y as u8, 7]);
        let c = compare(&a, &a, &Tolerance::default()).unwrap();
        assert_eq!(
            (c.verdict, c.pixels, c.regions.len()),
            (Verdict::Pass, 0, 0)
        );
    }

    #[test]
    fn a_difference_fails_exact_and_is_near_within_both_limits() {
        let a = img(40, 40, |_, _| [0, 0, 0]);
        let b = img(40, 40, |x, y| {
            if (x, y) == (3, 5) {
                [0, 2, 0]
            } else {
                [0, 0, 0]
            }
        });
        let c = compare(&a, &b, &Tolerance::default()).unwrap();
        assert_eq!((c.verdict, c.pixels, c.max_delta), (Verdict::Fail, 1, 2));
        assert_eq!(
            c.regions,
            [Rect {
                x0: 3,
                y0: 5,
                x1: 4,
                y1: 6
            }]
        );
        let near = Tolerance {
            max_delta: 2,
            max_pixels: 1,
            masks: vec![],
        };
        assert_eq!(compare(&a, &b, &near).unwrap().verdict, Verdict::Near);
        let small = Tolerance {
            max_delta: 1,
            max_pixels: 1,
            masks: vec![],
        };
        assert_eq!(compare(&a, &b, &small).unwrap().verdict, Verdict::Fail);
    }

    #[test]
    fn a_mask_hides_what_it_covers_and_counts_it() {
        let a = img(8, 8, |_, _| [0, 0, 0]);
        let b = img(8, 8, |x, _| if x < 2 { [9, 9, 9] } else { [0, 0, 0] });
        let tol = Tolerance {
            masks: vec![
                Rect {
                    x0: 0,
                    y0: 0,
                    x1: 2,
                    y1: 8,
                },
                Rect {
                    x0: 4,
                    y0: 4,
                    x1: 5,
                    y1: 5,
                },
            ],
            ..Tolerance::default()
        };
        let c = compare(&a, &b, &tol).unwrap();
        assert_eq!(
            (c.verdict, c.masked.as_slice()),
            (Verdict::Pass, &[16, 0][..])
        );
    }

    #[test]
    fn sizes_that_differ_fail_whole() {
        let c = compare(
            &img(4, 4, |_, _| [0; 3]),
            &img(5, 4, |_, _| [0; 3]),
            &Tolerance::default(),
        )
        .unwrap();
        assert_eq!((c.verdict, c.pixels, c.max_delta), (Verdict::Fail, 20, 255));
        assert_eq!(c.size_mismatch, Some(((4, 4), (5, 4))));
    }

    #[test]
    fn regions_group_touching_tiles_and_merge_down_to_the_most() {
        // two pixels in neighbouring tiles share a box; one far away does not
        let r = regions(&[(15, 0), (16, 0), (100, 100)]);
        assert_eq!(
            r,
            [
                Rect {
                    x0: 15,
                    y0: 0,
                    x1: 17,
                    y1: 1
                },
                Rect {
                    x0: 100,
                    y0: 100,
                    x1: 101,
                    y1: 101
                }
            ]
        );
        // a hundred scattered pixels come to at most REGIONS_MAX boxes
        // that hold them all
        let pts: Vec<(u32, u32)> = (0..100).map(|i| (i * 37 % 1000, i * 53 % 700)).collect();
        let r = regions(&pts);
        assert!(r.len() <= REGIONS_MAX);
        assert!(pts.iter().all(|&(x, y)| r.iter().any(|b| b.contains(x, y))));
    }
}
