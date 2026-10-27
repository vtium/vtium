//! The sprites against the C reference's box model (`tests/box_model.h`
//! and `tests/test_font_box.c` in aaronsb/kmscon), at every cell size from
//! 1x1 to 32x64 and in one large cell:
//! - the sprite ranges are claimed and nothing either side of them;
//! - each Braille, sextant and octant sprite is the cell's size and equals
//!   the model's, pixel for pixel;
//! - a mosaic and its complement cover the cell, so blocks of neighbouring
//!   patterns meet with no gap;
//! - Braille's eight dots against layouts worked out by hand from Ghostty's
//!   braille.zig for ten cell sizes, and, apart from any layout rule: equal
//!   squares inside the cell, the two columns and four rows aligned and in
//!   order, evenly spaced down the cell, not overlapping, at least a pixel
//!   wide once the cell is 4x8, and every pattern the union of its dots.
//!
//! The model is written apart from `src/sprite.rs`: the mosaics from the
//! Unicode 16.0.0 character names, BLOCK SEXTANT-n and BLOCK OCTANT-n, each
//! block's edges at its exact fraction of the cell rounded as Ghostty's
//! common.zig Fraction.min() and max() round them; the Braille dots
//! restating braille.zig's steps (Ghostty, MIT, src/font/sprite/draw/ at
//! f987028f, Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors).

use vtium_font::{Bitmap, sprite};

/// The digits of BLOCK SEXTANT-n, U+1FB00 on: 1 2 / 3 4 / 5 6 from the top.
#[rustfmt::skip]
const SEXTANTS: [&str; 60] = [
    "1", "2", "12", "3", "13", "23", "123", "4",
    "14", "24", "124", "34", "134", "234", "1234", "5",
    "15", "25", "125", "35", "235", "1235", "45", "145",
    "245", "1245", "345", "1345", "2345", "12345", "6", "16",
    "26", "126", "36", "136", "236", "1236", "46", "146",
    "1246", "346", "1346", "2346", "12346", "56", "156", "256",
    "1256", "356", "1356", "2356", "12356", "456", "1456", "2456",
    "12456", "3456", "13456", "23456",
];

/// The digits of BLOCK OCTANT-n, U+1CD00 on: 1 2 / 3 4 / 5 6 / 7 8.
#[rustfmt::skip]
const OCTANTS: [&str; 230] = [
    "3", "23", "123", "4", "14", "124", "34", "134",
    "234", "5", "15", "25", "125", "135", "235", "1235",
    "45", "145", "245", "1245", "345", "1345", "2345", "12345",
    "6", "16", "26", "126", "36", "136", "236", "1236",
    "146", "246", "1246", "346", "1346", "2346", "12346", "56",
    "156", "256", "1256", "356", "1356", "2356", "12356", "456",
    "1456", "2456", "12456", "3456", "13456", "23456", "17", "27",
    "127", "37", "137", "237", "1237", "47", "147", "247",
    "1247", "347", "1347", "2347", "12347", "157", "257", "1257",
    "357", "2357", "12357", "457", "1457", "12457", "3457", "13457",
    "23457", "67", "167", "267", "1267", "367", "1367", "2367",
    "12367", "467", "1467", "2467", "12467", "3467", "13467", "23467",
    "123467", "567", "1567", "2567", "12567", "3567", "13567", "23567",
    "123567", "4567", "14567", "24567", "124567", "34567", "134567", "234567",
    "1234567", "18", "28", "128", "38", "138", "238", "1238",
    "48", "148", "248", "1248", "348", "1348", "2348", "12348",
    "58", "158", "258", "1258", "358", "1358", "2358", "12358",
    "458", "1458", "2458", "12458", "3458", "13458", "23458", "123458",
    "168", "268", "1268", "368", "2368", "12368", "468", "1468",
    "12468", "3468", "13468", "23468", "568", "1568", "2568", "12568",
    "3568", "13568", "23568", "123568", "4568", "14568", "24568", "124568",
    "34568", "134568", "234568", "1234568", "178", "278", "1278", "378",
    "1378", "2378", "12378", "478", "1478", "2478", "12478", "3478",
    "13478", "23478", "123478", "578", "1578", "2578", "12578", "3578",
    "13578", "23578", "123578", "4578", "14578", "24578", "124578", "34578",
    "134578", "234578", "1234578", "678", "1678", "2678", "12678", "3678",
    "13678", "23678", "123678", "4678", "14678", "24678", "124678", "34678",
    "134678", "234678", "1234678", "15678", "25678", "125678", "35678", "235678",
    "1235678", "45678", "145678", "1245678", "1345678", "2345678",
];

const RANGES: [(u32, u32); 3] = [(0x2800, 0x28ff), (0x1fb00, 0x1fb3b), (0x1cd00, 0x1cde5)];

/// A model sprite: `w` by `h`, 0 or 0xff per pixel.
struct Model {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Model {
    fn new(w: u32, h: u32) -> Model {
        Model {
            w,
            h,
            px: vec![0; (w * h) as usize],
        }
    }

    fn fill(&mut self, x0: u32, y0: u32, x1: u32, y1: u32) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.px[(y * self.w + x) as usize] = 0xff;
            }
        }
    }
}

/// A block's edges along `size`, from `f0` to `f1` of it: the near edge
/// Fraction.min(), size - round((1 - f) * size), the far edge
/// Fraction.max(), round(f * size).
fn edges(size: u32, f0: f64, f1: f64) -> (u32, u32) {
    let size_f = f64::from(size);
    (
        size - ((1.0 - f0) * size_f).round() as u32,
        (f1 * size_f).round() as u32,
    )
}

/// The blocks named by `digits` on a grid two wide and `rows` high.
fn model_mosaic(digits: &str, rows: u32, m: &mut Model) {
    for d in digits.bytes() {
        let n = u32::from(d - b'1');
        let (c, r) = (n % 2, n / 2);
        let (x0, x1) = edges(m.w, f64::from(c) / 2.0, f64::from(c + 1) / 2.0);
        let (y0, y1) = edges(
            m.h,
            f64::from(r) / f64::from(rows),
            f64::from(r + 1) / f64::from(rows),
        );
        m.fill(x0, y0, x1, y1);
    }
}

/// braille.zig: a dot min(w/4, h/8) square, gaps of w/4 and h/8 between
/// the dots, margins of half a gap; then the pixels left over, as many as
/// each step takes and in this order, buy a one-pixel dot, a one-pixel
/// margin, a pixel more of gap (one across, three down), a pixel more of
/// margin each side, and a pixel more of dot.
fn model_braille(cp: u32, m: &mut Model) {
    let (w, h) = (m.w as i64, m.h as i64);
    let mut dot = (w / 4).min(h / 8);
    let (mut gx, mut gy) = (w / 4, h / 8);
    let (mut mx, mut my) = (gx / 2, gy / 2);
    let mut lx = w - 2 * mx - gx - 2 * dot;
    let mut ly = h - 2 * my - 3 * gy - 4 * dot;
    if dot == 0 && lx >= 2 && ly >= 4 {
        (dot, lx, ly) = (1, lx - 2, ly - 4);
    }
    if mx == 0 && lx >= 2 {
        (mx, lx) = (1, lx - 2);
    }
    if my == 0 && ly >= 2 {
        (my, ly) = (1, ly - 2);
    }
    if lx >= 1 {
        (gx, lx) = (gx + 1, lx - 1);
    }
    if ly >= 3 {
        (gy, ly) = (gy + 1, ly - 3);
    }
    if lx >= 2 {
        (mx, lx) = (mx + 1, lx - 2);
    }
    if ly >= 2 {
        (my, ly) = (my + 1, ly - 2);
    }
    if lx >= 2 && ly >= 4 {
        dot += 1;
    }
    // dots 1-3 and 7 down the left, 4-6 and 8 down the right
    for n in 0..8u32 {
        if cp >> n & 1 == 0 {
            continue;
        }
        let c = i64::from(matches!(n, 3 | 4 | 5 | 7));
        let r = if n < 6 { i64::from(n % 3) } else { 3 };
        let (x0, y0) = (mx + c * (dot + gx), my + r * (dot + gy));
        m.fill(x0 as u32, y0 as u32, (x0 + dot) as u32, (y0 + dot) as u32);
    }
}

/// `cp`'s model sprite.
fn model(cp: u32, w: u32, h: u32) -> Model {
    let mut m = Model::new(w, h);
    match cp {
        0x2800..=0x28ff => model_braille(cp, &mut m),
        0x1fb00..=0x1fb3b => model_mosaic(SEXTANTS[(cp - 0x1fb00) as usize], 3, &mut m),
        0x1cd00..=0x1cde5 => model_mosaic(OCTANTS[(cp - 0x1cd00) as usize], 4, &mut m),
        _ => panic!("U+{cp:04X} is not modelled"),
    }
    m
}

fn draw(cp: u32, w: u32, h: u32) -> Bitmap {
    let c = char::from_u32(cp).expect("a char");
    let g = sprite::draw(c, w, h, 1).unwrap_or_else(|| panic!("U+{cp:04X} at {w}x{h}"));
    assert_eq!((g.width(), g.height()), (w, h));
    g
}

/// Failures, reported together, the first 20 in full.
#[derive(Default)]
struct Failures(Vec<String>);

impl Failures {
    fn fail(&mut self, cp: u32, w: u32, h: u32, why: &str) {
        self.0.push(format!("U+{cp:04X} at {w}x{h}: {why}"));
    }

    fn assert_none(self) {
        assert!(
            self.0.is_empty(),
            "{} failures, first:\n{}",
            self.0.len(),
            self.0[..self.0.len().min(20)].join("\n")
        );
    }
}

/// Each sprite against the model.
fn check_model(w: u32, h: u32, f: &mut Failures) {
    for (first, last) in RANGES {
        for cp in first..=last {
            if draw(cp, w, h).pixels() != model(cp, w, h).px.as_slice() {
                f.fail(cp, w, h, "differs from the model");
            }
        }
    }
}

/// `mask`'s mosaic in `names`, found by its digits.
fn mosaic_cp(names: &[&str], base: u32, mask: u32) -> Option<u32> {
    let digits: String = (0..8)
        .filter(|b| mask >> b & 1 != 0)
        .map(|b| char::from(b'1' + b as u8))
        .collect();
    names
        .iter()
        .position(|n| *n == digits)
        .map(|i| base + i as u32)
}

/// Every mosaic whose complement is a mosaic too: together, each pixel.
fn check_complements(w: u32, h: u32, names: &[&str], base: u32, full: u32, f: &mut Failures) {
    for mask in 1..full {
        let (Some(a), Some(b)) = (
            mosaic_cp(names, base, mask),
            mosaic_cp(names, base, full & !mask),
        ) else {
            continue;
        };
        let (ga, gb) = (draw(a, w, h), draw(b, w, h));
        if ga
            .pixels()
            .iter()
            .zip(gb.pixels())
            .any(|(p, q)| *p == 0 && *q == 0)
        {
            f.fail(a, w, h, "a pixel neither it nor its complement covers");
        }
    }
}

/// A lit region's bounding box and pixel count.
#[derive(Clone, Copy, Default)]
struct Dot {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    n: u32,
}

fn find_dot(g: &Bitmap, f: &mut Failures) -> Dot {
    let mut d = Dot {
        x0: g.width(),
        y0: g.height(),
        ..Dot::default()
    };
    for y in 0..g.height() {
        for x in 0..g.width() {
            let v = g.get(x, y).expect("inside");
            if v != 0 && v != 0xff {
                f.fail(0x2800, g.width(), g.height(), "a pixel neither on nor off");
            }
            if v == 0 {
                continue;
            }
            d.n += 1;
            d.x0 = d.x0.min(x);
            d.y0 = d.y0.min(y);
            d.x1 = d.x1.max(x + 1);
            d.y1 = d.y1.max(y + 1);
        }
    }
    d
}

fn check_braille(w: u32, h: u32, f: &mut Failures) {
    const COL: [usize; 8] = [0, 0, 0, 1, 1, 1, 0, 1];
    const ROW: [usize; 8] = [0, 1, 2, 0, 1, 2, 3, 3];
    let d: Vec<Dot> = (0..8)
        .map(|n| find_dot(&draw(0x2800 + (1 << n), w, h), f))
        .collect();
    let side = if d[0].n > 0 { d[0].x1 - d[0].x0 } else { 0 };
    if side == 0 {
        // too small for a dot: every pattern blank
        for (n, dot) in d.iter().enumerate() {
            if dot.n > 0 {
                f.fail(0x2800 + (1 << n), w, h, "a dot where dot 1 has none");
            }
        }
        if w >= 4 && h >= 8 {
            f.fail(0x2801, w, h, "no dot in a cell of at least 4x8");
        }
        return;
    }
    for (n, dot) in d.iter().enumerate() {
        if dot.x1 - dot.x0 != side || dot.y1 - dot.y0 != side || dot.n != side * side {
            f.fail(0x2800 + (1 << n), w, h, "not a square the size of dot 1");
            return;
        }
    }
    let x = [d[0].x0, d[3].x0];
    let y = [d[0].y0, d[1].y0, d[2].y0, d[6].y0];
    for n in 0..8 {
        if d[n].x0 != x[COL[n]] || d[n].y0 != y[ROW[n]] {
            f.fail(0x2800 + (1 << n), w, h, "off its column or row");
        }
    }
    if x[1] < x[0] + side {
        f.fail(0x2808, w, h, "the right column overlaps the left");
    }
    for r in 0..3 {
        if y[r + 1] < y[r] + side || y[r + 1] - y[r] != y[1] - y[0] {
            f.fail(0x2800, w, h, "rows overlapping or unevenly spaced");
        }
    }
    if side < (w / 4).min(h / 8) {
        f.fail(0x2801, w, h, "a dot smaller than the cell allows");
    }
    // every pattern is the union of its dots
    for cp in 0x2800..=0x28ff {
        let mut want = Model::new(w, h);
        for (n, dot) in d.iter().enumerate() {
            if cp >> n & 1 != 0 {
                want.fill(dot.x0, dot.y0, dot.x1, dot.y1);
            }
        }
        if draw(cp, w, h).pixels() != want.px.as_slice() {
            f.fail(cp, w, h, "not the union of its dots");
        }
    }
}

fn check_size(w: u32, h: u32, f: &mut Failures) {
    check_model(w, h, f);
    check_complements(w, h, &SEXTANTS, 0x1fb00, 0x3f, f);
    check_complements(w, h, &OCTANTS, 0x1cd00, 0xff, f);
    check_braille(w, h, f);
}

#[test]
fn claims_its_ranges_and_nothing_beside_them() {
    for (first, last) in RANGES.into_iter().chain([(0x2500, 0x259f)]) {
        let has = |cp| char::from_u32(cp).is_some_and(sprite::has);
        assert!(!has(first - 1), "U+{:04X}", first - 1);
        assert!(!has(last + 1), "U+{:04X}", last + 1);
        assert!((first..=last).all(has), "U+{first:04X}-U+{last:04X}");
    }
    assert!(sprite::draw('A', 8, 16, 1).is_none());
    // a mosaic's complement is found by name, so the pairs exist
    assert_eq!(mosaic_cp(&SEXTANTS, 0x1fb00, 0x19), Some(0x1fb17));
    assert_eq!(mosaic_cp(&OCTANTS, 0x1cd00, 0x99), Some(0x1cd89));
}

#[test]
fn sizes_outside_the_bounds_draw_nothing() {
    use vtium_font::CELL_SIDE_MAX;
    assert!(sprite::draw('█', 0, 16, 1).is_none());
    assert!(sprite::draw('█', 8, 0, 1).is_none());
    assert!(sprite::draw('█', 8, CELL_SIDE_MAX + 1, 1).is_none());
    assert!(sprite::draw('█', 2 * CELL_SIDE_MAX + 1, 16, 1).is_none());
    assert!(sprite::draw('─', 8, 16, CELL_SIDE_MAX + 1).is_none());
    let full = sprite::draw('█', 2 * CELL_SIDE_MAX, CELL_SIDE_MAX, 1).expect("at the bound");
    assert!(full.pixels().iter().all(|&p| p == 0xff));
    // a thickness of 0 draws as 1
    assert_eq!(sprite::draw('─', 8, 16, 0), sprite::draw('─', 8, 16, 1));
    // the thickest line fills the cell
    let thick = sprite::draw('┼', 9, 17, CELL_SIDE_MAX).expect("drawn");
    assert!(thick.pixels().iter().all(|&p| p == 0xff));
}

/// A cell, a dot's side, the two columns' x and the four rows' y.
type Layout = (u32, u32, u32, [u32; 2], [u32; 4]);

/// Braille dot layouts worked out by hand from braille.zig draw2800_28FF()
/// (Ghostty, MIT, at f987028f) in the C reference's test_font_box.c, for
/// the cell sizes the tiers draw, the operator's 20x40, the smallest cells
/// and two lopsided ones: the cell, the dot's side, the two columns' x and
/// the four rows' y.
#[rustfmt::skip]
const BRAILLE_LAYOUTS: [Layout; 10] = [
    (1, 1, 0, [0, 0], [0, 0, 0, 0]),          // no room for a dot
    (2, 4, 1, [0, 1], [0, 1, 2, 3]),          // a one-pixel dot bought from what is left
    (4, 8, 1, [0, 3], [0, 2, 4, 6]),          // the left-over pixel widens the gap
    (9, 16, 2, [1, 6], [1, 5, 9, 13]),        // tier 2 mosaic
    (10, 19, 2, [1, 6], [1, 6, 11, 16]),      // tier 3
    (12, 28, 3, [1, 8], [2, 9, 16, 23]),      // tier 2 mosaic-spaced
    (20, 40, 5, [2, 13], [2, 12, 22, 32]),    // Monoid 30 on the operator's display
    (7, 30, 1, [1, 4], [2, 7, 12, 17]),       // narrow and tall
    (32, 8, 1, [5, 15], [0, 2, 4, 6]),        // wide and short
    (1000, 2000, 250, [125, 625], [125, 625, 1125, 1625]),
];

#[test]
fn braille_matches_the_hand_worked_layouts() {
    for (w, h, side, xs, ys) in BRAILLE_LAYOUTS {
        let mut want = Model::new(w, h);
        for x in xs {
            for y in ys {
                want.fill(x, y, x + side, y + side);
            }
        }
        assert_eq!(
            draw(0x28ff, w, h).pixels(),
            want.px.as_slice(),
            "sprite.rs at {w}x{h}"
        );
        assert_eq!(
            model(0x28ff, w, h).px,
            want.px,
            "the test's model at {w}x{h}"
        );
    }
}

#[test]
fn every_cell_size_matches_the_model() {
    let mut f = Failures::default();
    for h in 1..=64 {
        for w in 1..=32 {
            check_size(w, h, &mut f);
        }
    }
    f.assert_none();
}

#[test]
fn a_large_cell_matches_the_model() {
    let mut f = Failures::default();
    check_size(1000, 2000, &mut f);
    f.assert_none();
}
