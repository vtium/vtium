//! The Symbols for Legacy Computing sprites (vtium/vtium#27) against a model
//! written apart from `src/sprite/legacy.rs`, from the Unicode 17.0.0
//! character names and the Unicode 18.0 code charts for Symbols for Legacy
//! Computing (U+1FB00-U+1FBFF) and its supplement (U+1CC00-U+1CEBF):
//! - every codepoint of U+1CEA0-U+1CEAF and U+1FB3C-U+1FBFF is drawn as a
//!   sprite, or is a pictograph left to the face, or is unassigned;
//! - each sprite against the model built from its name, at every cell size
//!   from 1x1 to 32x64 with the light line thickening from 1 to 3 down the
//!   sizes, and in two large cells;
//! - literal masks of sprites from each family at a few sizes, odd widths
//!   among them.
//!
//! The model draws blocks, eighths, quarters, shades and fills pixel for
//! pixel, each edge at its exact fraction of the cell rounded as Ghostty's
//! common.zig Fraction.min() and max() round it. A slanted or curved shape
//! is held by the pixels it covers whole, which must be fully on, and the
//! pixels it misses, which must be off; the pixels its edge crosses may
//! take any coverage up to the shape's value.

use vtium_font::{Glyph, sprite};

/// The characters drawn as sprites, with their Unicode 17.0.0 names.
#[rustfmt::skip]
const NAMES: [(u32, &str); 168] = [
    (0x1cea0, "RIGHT HALF LOWER ONE QUARTER BLOCK"),
    (0x1cea1, "RIGHT THREE QUARTERS LOWER ONE QUARTER BLOCK"),
    (0x1cea2, "LEFT THREE QUARTERS LOWER ONE QUARTER BLOCK"),
    (0x1cea3, "LEFT HALF LOWER ONE QUARTER BLOCK"),
    (0x1cea4, "LOWER HALF LEFT ONE QUARTER BLOCK"),
    (0x1cea5, "LOWER THREE QUARTERS LEFT ONE QUARTER BLOCK"),
    (0x1cea6, "UPPER THREE QUARTERS LEFT ONE QUARTER BLOCK"),
    (0x1cea7, "UPPER HALF LEFT ONE QUARTER BLOCK"),
    (0x1cea8, "LEFT HALF UPPER ONE QUARTER BLOCK"),
    (0x1cea9, "LEFT THREE QUARTERS UPPER ONE QUARTER BLOCK"),
    (0x1ceaa, "RIGHT THREE QUARTERS UPPER ONE QUARTER BLOCK"),
    (0x1ceab, "RIGHT HALF UPPER ONE QUARTER BLOCK"),
    (0x1ceac, "UPPER HALF RIGHT ONE QUARTER BLOCK"),
    (0x1cead, "UPPER THREE QUARTERS RIGHT ONE QUARTER BLOCK"),
    (0x1ceae, "LOWER THREE QUARTERS RIGHT ONE QUARTER BLOCK"),
    (0x1ceaf, "LOWER HALF RIGHT ONE QUARTER BLOCK"),
    (0x1fb3c, "LOWER LEFT BLOCK DIAGONAL LOWER MIDDLE LEFT TO LOWER CENTRE"),
    (0x1fb3d, "LOWER LEFT BLOCK DIAGONAL LOWER MIDDLE LEFT TO LOWER RIGHT"),
    (0x1fb3e, "LOWER LEFT BLOCK DIAGONAL UPPER MIDDLE LEFT TO LOWER CENTRE"),
    (0x1fb3f, "LOWER LEFT BLOCK DIAGONAL UPPER MIDDLE LEFT TO LOWER RIGHT"),
    (0x1fb40, "LOWER LEFT BLOCK DIAGONAL UPPER LEFT TO LOWER CENTRE"),
    (0x1fb41, "LOWER RIGHT BLOCK DIAGONAL UPPER MIDDLE LEFT TO UPPER CENTRE"),
    (0x1fb42, "LOWER RIGHT BLOCK DIAGONAL UPPER MIDDLE LEFT TO UPPER RIGHT"),
    (0x1fb43, "LOWER RIGHT BLOCK DIAGONAL LOWER MIDDLE LEFT TO UPPER CENTRE"),
    (0x1fb44, "LOWER RIGHT BLOCK DIAGONAL LOWER MIDDLE LEFT TO UPPER RIGHT"),
    (0x1fb45, "LOWER RIGHT BLOCK DIAGONAL LOWER LEFT TO UPPER CENTRE"),
    (0x1fb46, "LOWER RIGHT BLOCK DIAGONAL LOWER MIDDLE LEFT TO UPPER MIDDLE RIGHT"),
    (0x1fb47, "LOWER RIGHT BLOCK DIAGONAL LOWER CENTRE TO LOWER MIDDLE RIGHT"),
    (0x1fb48, "LOWER RIGHT BLOCK DIAGONAL LOWER LEFT TO LOWER MIDDLE RIGHT"),
    (0x1fb49, "LOWER RIGHT BLOCK DIAGONAL LOWER CENTRE TO UPPER MIDDLE RIGHT"),
    (0x1fb4a, "LOWER RIGHT BLOCK DIAGONAL LOWER LEFT TO UPPER MIDDLE RIGHT"),
    (0x1fb4b, "LOWER RIGHT BLOCK DIAGONAL LOWER CENTRE TO UPPER RIGHT"),
    (0x1fb4c, "LOWER LEFT BLOCK DIAGONAL UPPER CENTRE TO UPPER MIDDLE RIGHT"),
    (0x1fb4d, "LOWER LEFT BLOCK DIAGONAL UPPER LEFT TO UPPER MIDDLE RIGHT"),
    (0x1fb4e, "LOWER LEFT BLOCK DIAGONAL UPPER CENTRE TO LOWER MIDDLE RIGHT"),
    (0x1fb4f, "LOWER LEFT BLOCK DIAGONAL UPPER LEFT TO LOWER MIDDLE RIGHT"),
    (0x1fb50, "LOWER LEFT BLOCK DIAGONAL UPPER CENTRE TO LOWER RIGHT"),
    (0x1fb51, "LOWER LEFT BLOCK DIAGONAL UPPER MIDDLE LEFT TO LOWER MIDDLE RIGHT"),
    (0x1fb52, "UPPER RIGHT BLOCK DIAGONAL LOWER MIDDLE LEFT TO LOWER CENTRE"),
    (0x1fb53, "UPPER RIGHT BLOCK DIAGONAL LOWER MIDDLE LEFT TO LOWER RIGHT"),
    (0x1fb54, "UPPER RIGHT BLOCK DIAGONAL UPPER MIDDLE LEFT TO LOWER CENTRE"),
    (0x1fb55, "UPPER RIGHT BLOCK DIAGONAL UPPER MIDDLE LEFT TO LOWER RIGHT"),
    (0x1fb56, "UPPER RIGHT BLOCK DIAGONAL UPPER LEFT TO LOWER CENTRE"),
    (0x1fb57, "UPPER LEFT BLOCK DIAGONAL UPPER MIDDLE LEFT TO UPPER CENTRE"),
    (0x1fb58, "UPPER LEFT BLOCK DIAGONAL UPPER MIDDLE LEFT TO UPPER RIGHT"),
    (0x1fb59, "UPPER LEFT BLOCK DIAGONAL LOWER MIDDLE LEFT TO UPPER CENTRE"),
    (0x1fb5a, "UPPER LEFT BLOCK DIAGONAL LOWER MIDDLE LEFT TO UPPER RIGHT"),
    (0x1fb5b, "UPPER LEFT BLOCK DIAGONAL LOWER LEFT TO UPPER CENTRE"),
    (0x1fb5c, "UPPER LEFT BLOCK DIAGONAL LOWER MIDDLE LEFT TO UPPER MIDDLE RIGHT"),
    (0x1fb5d, "UPPER LEFT BLOCK DIAGONAL LOWER CENTRE TO LOWER MIDDLE RIGHT"),
    (0x1fb5e, "UPPER LEFT BLOCK DIAGONAL LOWER LEFT TO LOWER MIDDLE RIGHT"),
    (0x1fb5f, "UPPER LEFT BLOCK DIAGONAL LOWER CENTRE TO UPPER MIDDLE RIGHT"),
    (0x1fb60, "UPPER LEFT BLOCK DIAGONAL LOWER LEFT TO UPPER MIDDLE RIGHT"),
    (0x1fb61, "UPPER LEFT BLOCK DIAGONAL LOWER CENTRE TO UPPER RIGHT"),
    (0x1fb62, "UPPER RIGHT BLOCK DIAGONAL UPPER CENTRE TO UPPER MIDDLE RIGHT"),
    (0x1fb63, "UPPER RIGHT BLOCK DIAGONAL UPPER LEFT TO UPPER MIDDLE RIGHT"),
    (0x1fb64, "UPPER RIGHT BLOCK DIAGONAL UPPER CENTRE TO LOWER MIDDLE RIGHT"),
    (0x1fb65, "UPPER RIGHT BLOCK DIAGONAL UPPER LEFT TO LOWER MIDDLE RIGHT"),
    (0x1fb66, "UPPER RIGHT BLOCK DIAGONAL UPPER CENTRE TO LOWER RIGHT"),
    (0x1fb67, "UPPER RIGHT BLOCK DIAGONAL UPPER MIDDLE LEFT TO LOWER MIDDLE RIGHT"),
    (0x1fb68, "UPPER AND RIGHT AND LOWER TRIANGULAR THREE QUARTERS BLOCK"),
    (0x1fb69, "LEFT AND LOWER AND RIGHT TRIANGULAR THREE QUARTERS BLOCK"),
    (0x1fb6a, "UPPER AND LEFT AND LOWER TRIANGULAR THREE QUARTERS BLOCK"),
    (0x1fb6b, "LEFT AND UPPER AND RIGHT TRIANGULAR THREE QUARTERS BLOCK"),
    (0x1fb6c, "LEFT TRIANGULAR ONE QUARTER BLOCK"),
    (0x1fb6d, "UPPER TRIANGULAR ONE QUARTER BLOCK"),
    (0x1fb6e, "RIGHT TRIANGULAR ONE QUARTER BLOCK"),
    (0x1fb6f, "LOWER TRIANGULAR ONE QUARTER BLOCK"),
    (0x1fb70, "VERTICAL ONE EIGHTH BLOCK-2"),
    (0x1fb71, "VERTICAL ONE EIGHTH BLOCK-3"),
    (0x1fb72, "VERTICAL ONE EIGHTH BLOCK-4"),
    (0x1fb73, "VERTICAL ONE EIGHTH BLOCK-5"),
    (0x1fb74, "VERTICAL ONE EIGHTH BLOCK-6"),
    (0x1fb75, "VERTICAL ONE EIGHTH BLOCK-7"),
    (0x1fb76, "HORIZONTAL ONE EIGHTH BLOCK-2"),
    (0x1fb77, "HORIZONTAL ONE EIGHTH BLOCK-3"),
    (0x1fb78, "HORIZONTAL ONE EIGHTH BLOCK-4"),
    (0x1fb79, "HORIZONTAL ONE EIGHTH BLOCK-5"),
    (0x1fb7a, "HORIZONTAL ONE EIGHTH BLOCK-6"),
    (0x1fb7b, "HORIZONTAL ONE EIGHTH BLOCK-7"),
    (0x1fb7c, "LEFT AND LOWER ONE EIGHTH BLOCK"),
    (0x1fb7d, "LEFT AND UPPER ONE EIGHTH BLOCK"),
    (0x1fb7e, "RIGHT AND UPPER ONE EIGHTH BLOCK"),
    (0x1fb7f, "RIGHT AND LOWER ONE EIGHTH BLOCK"),
    (0x1fb80, "UPPER AND LOWER ONE EIGHTH BLOCK"),
    (0x1fb81, "HORIZONTAL ONE EIGHTH BLOCK-1358"),
    (0x1fb82, "UPPER ONE QUARTER BLOCK"),
    (0x1fb83, "UPPER THREE EIGHTHS BLOCK"),
    (0x1fb84, "UPPER FIVE EIGHTHS BLOCK"),
    (0x1fb85, "UPPER THREE QUARTERS BLOCK"),
    (0x1fb86, "UPPER SEVEN EIGHTHS BLOCK"),
    (0x1fb87, "RIGHT ONE QUARTER BLOCK"),
    (0x1fb88, "RIGHT THREE EIGHTHS BLOCK"),
    (0x1fb89, "RIGHT FIVE EIGHTHS BLOCK"),
    (0x1fb8a, "RIGHT THREE QUARTERS BLOCK"),
    (0x1fb8b, "RIGHT SEVEN EIGHTHS BLOCK"),
    (0x1fb8c, "LEFT HALF MEDIUM SHADE"),
    (0x1fb8d, "RIGHT HALF MEDIUM SHADE"),
    (0x1fb8e, "UPPER HALF MEDIUM SHADE"),
    (0x1fb8f, "LOWER HALF MEDIUM SHADE"),
    (0x1fb90, "INVERSE MEDIUM SHADE"),
    (0x1fb91, "UPPER HALF BLOCK AND LOWER HALF INVERSE MEDIUM SHADE"),
    (0x1fb92, "UPPER HALF INVERSE MEDIUM SHADE AND LOWER HALF BLOCK"),
    (0x1fb94, "LEFT HALF INVERSE MEDIUM SHADE AND RIGHT HALF BLOCK"),
    (0x1fb95, "CHECKER BOARD FILL"),
    (0x1fb96, "INVERSE CHECKER BOARD FILL"),
    (0x1fb97, "HEAVY HORIZONTAL FILL"),
    (0x1fb98, "UPPER LEFT TO LOWER RIGHT FILL"),
    (0x1fb99, "UPPER RIGHT TO LOWER LEFT FILL"),
    (0x1fb9a, "UPPER AND LOWER TRIANGULAR HALF BLOCK"),
    (0x1fb9b, "LEFT AND RIGHT TRIANGULAR HALF BLOCK"),
    (0x1fb9c, "UPPER LEFT TRIANGULAR MEDIUM SHADE"),
    (0x1fb9d, "UPPER RIGHT TRIANGULAR MEDIUM SHADE"),
    (0x1fb9e, "LOWER RIGHT TRIANGULAR MEDIUM SHADE"),
    (0x1fb9f, "LOWER LEFT TRIANGULAR MEDIUM SHADE"),
    (0x1fba0, "BOX DRAWINGS LIGHT DIAGONAL UPPER CENTRE TO MIDDLE LEFT"),
    (0x1fba1, "BOX DRAWINGS LIGHT DIAGONAL UPPER CENTRE TO MIDDLE RIGHT"),
    (0x1fba2, "BOX DRAWINGS LIGHT DIAGONAL MIDDLE LEFT TO LOWER CENTRE"),
    (0x1fba3, "BOX DRAWINGS LIGHT DIAGONAL MIDDLE RIGHT TO LOWER CENTRE"),
    (0x1fba4, "BOX DRAWINGS LIGHT DIAGONAL UPPER CENTRE TO MIDDLE LEFT TO LOWER CENTRE"),
    (0x1fba5, "BOX DRAWINGS LIGHT DIAGONAL UPPER CENTRE TO MIDDLE RIGHT TO LOWER CENTRE"),
    (0x1fba6, "BOX DRAWINGS LIGHT DIAGONAL MIDDLE LEFT TO LOWER CENTRE TO MIDDLE RIGHT"),
    (0x1fba7, "BOX DRAWINGS LIGHT DIAGONAL MIDDLE LEFT TO UPPER CENTRE TO MIDDLE RIGHT"),
    (0x1fba8, "BOX DRAWINGS LIGHT DIAGONAL UPPER CENTRE TO MIDDLE LEFT AND MIDDLE RIGHT TO LOWER CENTRE"),
    (0x1fba9, "BOX DRAWINGS LIGHT DIAGONAL UPPER CENTRE TO MIDDLE RIGHT AND MIDDLE LEFT TO LOWER CENTRE"),
    (0x1fbaa, "BOX DRAWINGS LIGHT DIAGONAL UPPER CENTRE TO MIDDLE RIGHT TO LOWER CENTRE TO MIDDLE LEFT"),
    (0x1fbab, "BOX DRAWINGS LIGHT DIAGONAL UPPER CENTRE TO MIDDLE LEFT TO LOWER CENTRE TO MIDDLE RIGHT"),
    (0x1fbac, "BOX DRAWINGS LIGHT DIAGONAL MIDDLE LEFT TO UPPER CENTRE TO MIDDLE RIGHT TO LOWER CENTRE"),
    (0x1fbad, "BOX DRAWINGS LIGHT DIAGONAL MIDDLE RIGHT TO UPPER CENTRE TO MIDDLE LEFT TO LOWER CENTRE"),
    (0x1fbae, "BOX DRAWINGS LIGHT DIAGONAL DIAMOND"),
    (0x1fbaf, "BOX DRAWINGS LIGHT HORIZONTAL WITH VERTICAL STROKE"),
    (0x1fbbd, "NEGATIVE DIAGONAL CROSS"),
    (0x1fbbe, "NEGATIVE DIAGONAL MIDDLE RIGHT TO LOWER CENTRE"),
    (0x1fbbf, "NEGATIVE DIAGONAL DIAMOND"),
    (0x1fbce, "LEFT TWO THIRDS BLOCK"),
    (0x1fbcf, "LEFT ONE THIRD BLOCK"),
    (0x1fbd0, "BOX DRAWINGS LIGHT DIAGONAL MIDDLE RIGHT TO LOWER LEFT"),
    (0x1fbd1, "BOX DRAWINGS LIGHT DIAGONAL UPPER RIGHT TO MIDDLE LEFT"),
    (0x1fbd2, "BOX DRAWINGS LIGHT DIAGONAL UPPER LEFT TO MIDDLE RIGHT"),
    (0x1fbd3, "BOX DRAWINGS LIGHT DIAGONAL MIDDLE LEFT TO LOWER RIGHT"),
    (0x1fbd4, "BOX DRAWINGS LIGHT DIAGONAL UPPER LEFT TO LOWER CENTRE"),
    (0x1fbd5, "BOX DRAWINGS LIGHT DIAGONAL UPPER CENTRE TO LOWER RIGHT"),
    (0x1fbd6, "BOX DRAWINGS LIGHT DIAGONAL UPPER RIGHT TO LOWER CENTRE"),
    (0x1fbd7, "BOX DRAWINGS LIGHT DIAGONAL UPPER CENTRE TO LOWER LEFT"),
    (0x1fbd8, "BOX DRAWINGS LIGHT DIAGONAL UPPER LEFT TO MIDDLE CENTRE TO UPPER RIGHT"),
    (0x1fbd9, "BOX DRAWINGS LIGHT DIAGONAL UPPER RIGHT TO MIDDLE CENTRE TO LOWER RIGHT"),
    (0x1fbda, "BOX DRAWINGS LIGHT DIAGONAL LOWER LEFT TO MIDDLE CENTRE TO LOWER RIGHT"),
    (0x1fbdb, "BOX DRAWINGS LIGHT DIAGONAL UPPER LEFT TO MIDDLE CENTRE TO LOWER LEFT"),
    (0x1fbdc, "BOX DRAWINGS LIGHT DIAGONAL UPPER LEFT TO LOWER CENTRE TO UPPER RIGHT"),
    (0x1fbdd, "BOX DRAWINGS LIGHT DIAGONAL UPPER RIGHT TO MIDDLE LEFT TO LOWER RIGHT"),
    (0x1fbde, "BOX DRAWINGS LIGHT DIAGONAL LOWER LEFT TO UPPER CENTRE TO LOWER RIGHT"),
    (0x1fbdf, "BOX DRAWINGS LIGHT DIAGONAL UPPER LEFT TO MIDDLE RIGHT TO LOWER LEFT"),
    (0x1fbe0, "TOP JUSTIFIED LOWER HALF WHITE CIRCLE"),
    (0x1fbe1, "RIGHT JUSTIFIED LEFT HALF WHITE CIRCLE"),
    (0x1fbe2, "BOTTOM JUSTIFIED UPPER HALF WHITE CIRCLE"),
    (0x1fbe3, "LEFT JUSTIFIED RIGHT HALF WHITE CIRCLE"),
    (0x1fbe4, "UPPER CENTRE ONE QUARTER BLOCK"),
    (0x1fbe5, "LOWER CENTRE ONE QUARTER BLOCK"),
    (0x1fbe6, "MIDDLE LEFT ONE QUARTER BLOCK"),
    (0x1fbe7, "MIDDLE RIGHT ONE QUARTER BLOCK"),
    (0x1fbe8, "TOP JUSTIFIED LOWER HALF BLACK CIRCLE"),
    (0x1fbe9, "RIGHT JUSTIFIED LEFT HALF BLACK CIRCLE"),
    (0x1fbea, "BOTTOM JUSTIFIED UPPER HALF BLACK CIRCLE"),
    (0x1fbeb, "LEFT JUSTIFIED RIGHT HALF BLACK CIRCLE"),
    (0x1fbec, "TOP RIGHT JUSTIFIED LOWER LEFT QUARTER BLACK CIRCLE"),
    (0x1fbed, "BOTTOM LEFT JUSTIFIED UPPER RIGHT QUARTER BLACK CIRCLE"),
    (0x1fbee, "BOTTOM RIGHT JUSTIFIED UPPER LEFT QUARTER BLACK CIRCLE"),
    (0x1fbef, "TOP LEFT JUSTIFIED LOWER RIGHT QUARTER BLACK CIRCLE"),
];

/// The pictographs of U+1FB3C-U+1FBFF, left to the face: the pointer, check
/// mark, running man, arrows, folders, cross and squared dot; the saltire,
/// pointing index, squared question mark, stick figures, chevrons, cross
/// mark and bracket; the segmented digits and the alarm bell.
const LEFT_TO_THE_FACE: [(u32, u32); 3] =
    [(0x1fbb0, 0x1fbbc), (0x1fbc0, 0x1fbcd), (0x1fbf0, 0x1fbfa)];

/// Unassigned in Unicode 17.0.0.
const UNASSIGNED: [(u32, u32); 2] = [(0x1fb93, 0x1fb93), (0x1fbfb, 0x1fbff)];

/// The ranges vtium/vtium#27 names (aaronsb/kmscon#92).
const ISSUE_RANGES: [(u32, u32); 4] = [
    (0x1cea0, 0x1ceaf),
    (0x1fb82, 0x1fb85),
    (0x1fbe4, 0x1fbe7),
    (0x1fb3c, 0x1fbff),
];

fn named(cp: u32) -> Option<&'static str> {
    NAMES.iter().find(|(c, _)| *c == cp).map(|(_, n)| *n)
}

fn in_ranges(ranges: &[(u32, u32)], cp: u32) -> bool {
    ranges.iter().any(|&(a, b)| (a..=b).contains(&cp))
}

fn draw(cp: u32, w: u32, h: u32, t: u32) -> Glyph {
    let c = char::from_u32(cp).expect("a char");
    let g =
        sprite::draw(c, w, h, t).unwrap_or_else(|| panic!("U+{cp:04X} at {w}x{h} drew nothing"));
    assert_eq!((g.width(), g.height()), (w, h), "U+{cp:04X}");
    g
}

#[derive(Clone, Copy, Debug)]
struct Pt {
    x: f64,
    y: f64,
}

fn pt(x: f64, y: f64) -> Pt {
    Pt { x, y }
}

/// The cross product of b - a and p - a: its sign says which side of the
/// line a-b `p` is on.
fn side(a: Pt, b: Pt, p: Pt) -> f64 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

fn dist2(p: Pt, q: Pt) -> f64 {
    (p.x - q.x) * (p.x - q.x) + (p.y - q.y) * (p.y - q.y)
}

/// The squared distance from `p` to the segment a-b.
fn seg_dist2(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let u = if len2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    dist2(p, pt(a.x + u * dx, a.y + u * dy))
}

/// Samples a pixel side, as `src/sprite.rs` takes them.
const SS: u32 = 8;

/// Past half a pixel's diagonal: a pixel whose centre is this much farther
/// than a shape's reach lies wholly outside it.
const HALF_DIAGONAL: f64 = 0.7072;

enum Shape {
    /// The side of the line a-b that holds the third point.
    Half(Pt, Pt, Pt),
    Tri([Pt; 3]),
    /// Within the radius of the segment.
    Seg(Pt, Pt, f64),
    /// From the inner radius to the outer one round the centre.
    Ring(Pt, f64, f64),
}

/// The part of `poly` on `inside`'s side of the line a-b (Sutherland and
/// Hodgman).
fn clip(poly: &[Pt], a: Pt, b: Pt, inside: Pt) -> Vec<Pt> {
    let s = side(a, b, inside);
    let keep = |p: Pt| side(a, b, p) * s >= 0.0;
    let mut out = Vec::new();
    for (i, &p) in poly.iter().enumerate() {
        let q = poly[(i + 1) % poly.len()];
        if keep(p) {
            out.push(p);
        }
        if keep(p) != keep(q) {
            let (fp, fq) = (side(a, b, p), side(a, b, q));
            let u = fp / (fp - fq);
            out.push(pt(p.x + u * (q.x - p.x), p.y + u * (q.y - p.y)));
        }
    }
    out
}

fn area(poly: &[Pt]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (p, q) = (poly[i], poly[(i + 1) % n]);
            p.x * q.y - q.x * p.y
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

fn corners(x: u32, y: u32) -> [Pt; 4] {
    let (x, y) = (f64::from(x), f64::from(y));
    [
        pt(x, y),
        pt(x + 1.0, y),
        pt(x, y + 1.0),
        pt(x + 1.0, y + 1.0),
    ]
}

impl Shape {
    /// The exact share of the pixel at `x`, `y` a straight-edged shape
    /// covers; `None` for a curved one.
    fn share(&self, x: u32, y: u32) -> Option<f64> {
        let [p0, p1, p2, p3] = corners(x, y);
        let square = vec![p0, p1, p3, p2];
        match *self {
            Shape::Half(a, b, inside) => Some(area(&clip(&square, a, b, inside))),
            Shape::Tri(v) => {
                let mut poly = square;
                for i in 0..3 {
                    poly = clip(&poly, v[i], v[(i + 1) % 3], v[(i + 2) % 3]);
                }
                Some(area(&poly))
            }
            Shape::Seg(..) | Shape::Ring(..) => None,
        }
    }

    /// Whether the shape covers the pixel at `x`, `y` whole.
    fn covers(&self, x: u32, y: u32) -> bool {
        let c = corners(x, y);
        match *self {
            Shape::Half(a, b, inside) => {
                let s = side(a, b, inside);
                c.iter().all(|&p| side(a, b, p) * s >= 0.0)
            }
            Shape::Tri(v) => (0..3).all(|i| {
                let (a, b, o) = (v[i], v[(i + 1) % 3], v[(i + 2) % 3]);
                let s = side(a, b, o);
                c.iter().all(|&p| side(a, b, p) * s >= 0.0)
            }),
            Shape::Seg(a, b, r) => c.iter().all(|&p| seg_dist2(p, a, b) <= r * r),
            Shape::Ring(o, r0, r1) => {
                let far = c.iter().map(|&p| dist2(p, o)).fold(0.0, f64::max);
                far <= r1 * r1 && near2(o, x, y) >= r0 * r0
            }
        }
    }

    /// Whether the shape misses the pixel at `x`, `y` entirely.
    fn misses(&self, x: u32, y: u32) -> bool {
        let c = corners(x, y);
        let mid = pt(f64::from(x) + 0.5, f64::from(y) + 0.5);
        match *self {
            Shape::Half(a, b, inside) => {
                let s = side(a, b, inside);
                c.iter().all(|&p| side(a, b, p) * s < 0.0)
            }
            Shape::Tri(v) => (0..3).any(|i| {
                let (a, b, o) = (v[i], v[(i + 1) % 3], v[(i + 2) % 3]);
                let s = side(a, b, o);
                c.iter().all(|&p| side(a, b, p) * s < 0.0)
            }),
            Shape::Seg(a, b, r) => seg_dist2(mid, a, b) > (r + HALF_DIAGONAL) * (r + HALF_DIAGONAL),
            Shape::Ring(o, r0, r1) => {
                let far = c.iter().map(|&p| dist2(p, o)).fold(0.0, f64::max);
                near2(o, x, y) > r1 * r1 || far < r0 * r0
            }
        }
    }
}

/// The squared distance from `o` to the nearest point of the pixel.
fn near2(o: Pt, x: u32, y: u32) -> f64 {
    let (x, y) = (f64::from(x), f64::from(y));
    dist2(o, pt(o.x.clamp(x, x + 1.0), o.y.clamp(y, y + 1.0)))
}

/// A cell `w` by `h` with light lines `t` wide.
#[derive(Clone, Copy)]
struct Cell {
    w: u32,
    h: u32,
    t: u32,
}

impl Cell {
    fn wf(self) -> f64 {
        f64::from(self.w)
    }

    fn hf(self) -> f64 {
        f64::from(self.h)
    }

    /// The light lines' centre across `size`, as U+2500 and U+2502 draw
    /// them: (size - t) / 2 rounded down, plus half a line.
    fn mid(self, size: u32) -> f64 {
        f64::from(size.saturating_sub(self.t) / 2) + f64::from(self.t) / 2.0
    }
}

/// What a sprite should be.
enum Model {
    /// Every pixel.
    Exact(Vec<u8>),
    /// Pixels the shapes together cover whole are `on`, those they all miss
    /// are `off`, and the rest lie between.
    Cover { shapes: Vec<Shape>, on: u8, off: u8 },
}

/// The near and far edges of `f0` to `f1` of `size`: Fraction.min(),
/// size - round((1 - f0) * size), and Fraction.max(), round(f1 * size).
fn edges(size: u32, f0: f64, f1: f64) -> (u32, u32) {
    let s = f64::from(size);
    (
        size - ((1.0 - f0) * s).round() as u32,
        (f1 * s).round() as u32,
    )
}

/// Fractions across and down a cell.
type Rect = ((f64, f64), (f64, f64));

const WHOLE: (f64, f64) = (0.0, 1.0);

fn paint(px: &mut [u8], c: Cell, r: Rect, v: u8) {
    let (x0, x1) = edges(c.w, r.0.0, r.0.1);
    let (y0, y1) = edges(c.h, r.1.0, r.1.1);
    for y in y0..y1 {
        for x in x0..x1 {
            let p = &mut px[(y * c.w + x) as usize];
            *p = (*p).max(v);
        }
    }
}

/// The band `f` of the cell along `side`.
fn band(side: &str, f: f64) -> Rect {
    match side {
        "LEFT" => ((0.0, f), WHOLE),
        "RIGHT" => ((1.0 - f, 1.0), WHOLE),
        "UPPER" => (WHOLE, (0.0, f)),
        "LOWER" => (WHOLE, (1.0 - f, 1.0)),
        _ => panic!("no side {side}"),
    }
}

fn meet(a: Rect, b: Rect) -> Rect {
    (
        (a.0.0.max(b.0.0), a.0.1.min(b.0.1)),
        (a.1.0.max(b.1.0), a.1.1.min(b.1.1)),
    )
}

fn number(word: &str) -> f64 {
    match word {
        "ONE" => 1.0,
        "TWO" => 2.0,
        "THREE" => 3.0,
        "FIVE" => 5.0,
        "SEVEN" => 7.0,
        "QUARTER" | "QUARTERS" => 4.0,
        "EIGHTH" | "EIGHTHS" => 8.0,
        "THIRD" | "THIRDS" => 3.0,
        _ => panic!("no number {word}"),
    }
}

/// "HALF", or a count and a denominator: "THREE EIGHTHS".
fn fraction(words: &[&str]) -> f64 {
    match words {
        ["HALF"] => 0.5,
        [n, d] => number(n) / number(d),
        _ => panic!("no fraction {words:?}"),
    }
}

/// A point named on the smooth mosaics' grid: thirds down the sides,
/// halves along the top and bottom.
fn mosaic_point(c: Cell, name: &str) -> Pt {
    let (x, y) = match name {
        "UPPER LEFT" => (0.0, 0.0),
        "UPPER CENTRE" => (0.5, 0.0),
        "UPPER RIGHT" => (1.0, 0.0),
        "UPPER MIDDLE LEFT" => (0.0, 1.0 / 3.0),
        "UPPER MIDDLE RIGHT" => (1.0, 1.0 / 3.0),
        "LOWER MIDDLE LEFT" => (0.0, 2.0 / 3.0),
        "LOWER MIDDLE RIGHT" => (1.0, 2.0 / 3.0),
        "LOWER LEFT" => (0.0, 1.0),
        "LOWER CENTRE" => (0.5, 1.0),
        "LOWER RIGHT" => (1.0, 1.0),
        _ => panic!("no mosaic point {name}"),
    };
    pt(x * c.wf(), y * c.hf())
}

/// A point named on a diagonal line, and whether it lies on the cell's
/// edge. The centres are the light lines' centres, so the diagonals meet
/// box drawing's lines.
fn line_point(c: Cell, name: &str) -> (Pt, bool) {
    let (v, h) = name.split_once(' ').expect("two words");
    let x = match h {
        "LEFT" => 0.0,
        "CENTRE" => c.mid(c.w),
        "RIGHT" => c.wf(),
        _ => panic!("no point {name}"),
    };
    let y = match v {
        "UPPER" => 0.0,
        "MIDDLE" => c.mid(c.h),
        "LOWER" => c.hf(),
        _ => panic!("no point {name}"),
    };
    (pt(x, y), name != "MIDDLE CENTRE")
}

/// `p` moved far from `q` along the line through both: a line that ends on
/// the cell's edge runs on through it, cut off by the edge.
fn beyond(p: Pt, q: Pt, c: Cell) -> Pt {
    let (dx, dy) = (p.x - q.x, p.y - q.y);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return p;
    }
    // far past the cell in any direction
    let k = 4.0 * (c.wf() + c.hf()) / len2.sqrt();
    pt(p.x + dx * k, p.y + dy * k)
}

/// The segments of light diagonals named "P TO Q TO R AND S TO T".
fn diagonals(c: Cell, spec: &str, out: &mut Vec<Shape>) {
    let spec = match spec {
        "DIAMOND" => "UPPER CENTRE TO MIDDLE LEFT TO LOWER CENTRE TO MIDDLE RIGHT TO UPPER CENTRE",
        "CROSS" => "UPPER LEFT TO LOWER RIGHT AND UPPER RIGHT TO LOWER LEFT",
        s => s,
    };
    let r = f64::from(c.t) / 2.0;
    for path in spec.split(" AND ") {
        let pts: Vec<(Pt, bool)> = path.split(" TO ").map(|n| line_point(c, n)).collect();
        for pair in pts.windows(2) {
            let ((a, a_edge), (b, b_edge)) = (pair[0], pair[1]);
            let a2 = if a_edge { beyond(a, b, c) } else { a };
            let b2 = if b_edge { beyond(b, a, c) } else { b };
            out.push(Shape::Seg(a2, b2, r));
        }
    }
}

/// The checkerboard: four squares across, and down the even number of rows
/// nearest to square ones, at least two, so the board tiles across cells.
fn checkerboard(c: Cell, odd: u32) -> Vec<u8> {
    let mut px = vec![0; (c.w * c.h) as usize];
    let rows = 2 * ((2.0 * c.hf() / c.wf()).round() as u32).max(1);
    let edge =
        |size: u32, i: u32, n: u32| (f64::from(size) * f64::from(i) / f64::from(n)).round() as u32;
    for j in 0..rows {
        for i in 0..4 {
            if (i + j) % 2 != odd {
                continue;
            }
            for y in edge(c.h, j, rows)..edge(c.h, j + 1, rows) {
                for x in edge(c.w, i, 4)..edge(c.w, i + 1, 4) {
                    px[(y * c.w + x) as usize] = 0xff;
                }
            }
        }
    }
    px
}

/// Diagonal hatching: light lines parallel to the diagonal from the upper
/// left (or, `rising`, the upper right), evenly spaced so they run on into
/// the cells beside and below, as many across the cell as leaves at least a
/// line's width clear between them, and at least one.
fn hatch(c: Cell, rising: bool) -> Vec<Shape> {
    let apart = |n: u32| wf_hf(c) / (f64::from(n) * (c.wf() * c.wf() + c.hf() * c.hf()).sqrt());
    let n = (1..=c.w)
        .rev()
        .find(|&n| apart(n) >= 2.0 * f64::from(c.t))
        .unwrap_or(1);
    let s = c.wf() / f64::from(n);
    let r = f64::from(c.t) / 2.0;
    let n = i64::from(n);
    (-n - 1..=n + 1)
        .map(|k| {
            let o = k as f64 * s;
            let (a, b) = if rising {
                (pt(c.wf() + o, 0.0), pt(o, c.hf()))
            } else {
                (pt(o, 0.0), pt(c.wf() + o, c.hf()))
            };
            Shape::Seg(beyond(a, b, c), beyond(b, a, c), r)
        })
        .collect()
}

fn wf_hf(c: Cell) -> f64 {
    c.wf() * c.hf()
}

fn model(name: &str, c: Cell) -> Model {
    let mut px = vec![0u8; (c.w * c.h) as usize];
    let (wf, hf) = (c.wf(), c.hf());
    let centre = pt(wf / 2.0, hf / 2.0);
    let corner = |n: &str| match n {
        "UPPER LEFT" | "TOP LEFT" => pt(0.0, 0.0),
        "UPPER RIGHT" | "TOP RIGHT" => pt(wf, 0.0),
        "LOWER LEFT" | "BOTTOM LEFT" => pt(0.0, hf),
        "LOWER RIGHT" | "BOTTOM RIGHT" => pt(wf, hf),
        _ => panic!("no corner {n}"),
    };

    // smooth mosaics: "LOWER LEFT BLOCK DIAGONAL P TO Q", the cell on the
    // block's side of the line P-Q
    if let Some((block, line)) = name.split_once(" BLOCK DIAGONAL ") {
        let (p, q) = line.split_once(" TO ").expect("P TO Q");
        let shape = Shape::Half(mosaic_point(c, p), mosaic_point(c, q), corner(block));
        return Model::Cover {
            shapes: vec![shape],
            on: 0xff,
            off: 0,
        };
    }
    if let Some(spec) = name.strip_prefix("BOX DRAWINGS LIGHT DIAGONAL ") {
        let mut shapes = Vec::new();
        diagonals(c, spec, &mut shapes);
        return Model::Cover {
            shapes,
            on: 0xff,
            off: 0,
        };
    }
    if let Some(spec) = name.strip_prefix("NEGATIVE DIAGONAL ") {
        let mut shapes = Vec::new();
        diagonals(c, spec, &mut shapes);
        return Model::Cover {
            shapes,
            on: 0,
            off: 0xff,
        };
    }
    // "UPPER LEFT TRIANGULAR MEDIUM SHADE": the half of the cell on the
    // corner's side of the diagonal through the two corners beside it
    if let Some(at) = name.strip_suffix(" TRIANGULAR MEDIUM SHADE") {
        let o = corner(at);
        let shape = Shape::Tri([o, pt(wf - o.x, o.y), pt(o.x, hf - o.y)]);
        return Model::Cover {
            shapes: vec![shape],
            on: 0x80,
            off: 0,
        };
    }
    // "UPPER AND LOWER TRIANGULAR HALF BLOCK": a triangle on each edge named,
    // its apex at the cell's centre
    if let Some((sides, _)) = name.split_once(" TRIANGULAR ") {
        let shapes = sides
            .split(" AND ")
            .map(|s| {
                let (a, b) = match s {
                    "LEFT" => (pt(0.0, 0.0), pt(0.0, hf)),
                    "RIGHT" => (pt(wf, 0.0), pt(wf, hf)),
                    "UPPER" => (pt(0.0, 0.0), pt(wf, 0.0)),
                    "LOWER" => (pt(0.0, hf), pt(wf, hf)),
                    _ => panic!("no edge {s}"),
                };
                Shape::Tri([a, b, centre])
            })
            .collect();
        return Model::Cover {
            shapes,
            on: 0xff,
            off: 0,
        };
    }
    // "TOP RIGHT JUSTIFIED LOWER LEFT QUARTER BLACK CIRCLE": a circle as wide
    // as the cell's shorter side, centred on the edge or corner named;
    // white is its outline a light line wide
    if let Some((at, rest)) = name.split_once(" JUSTIFIED ") {
        let o = match at {
            "TOP" => pt(wf / 2.0, 0.0),
            "BOTTOM" => pt(wf / 2.0, hf),
            "LEFT" => pt(0.0, hf / 2.0),
            "RIGHT" => pt(wf, hf / 2.0),
            _ => corner(at),
        };
        let r = wf.min(hf) / 2.0;
        let r0 = if rest.ends_with("WHITE CIRCLE") {
            (r - f64::from(c.t)).max(0.0)
        } else {
            0.0
        };
        return Model::Cover {
            shapes: vec![Shape::Ring(o, r0, r)],
            on: 0xff,
            off: 0,
        };
    }
    if name.contains("MEDIUM SHADE") {
        // "UPPER HALF BLOCK AND LOWER HALF INVERSE MEDIUM SHADE"; the
        // inverse of half coverage is half coverage
        for part in name.split(" AND ") {
            let (what, v) = match part.strip_suffix(" BLOCK") {
                Some(w) => (w, 0xff),
                None => (part.strip_suffix(" MEDIUM SHADE").expect("a shade"), 0x80),
            };
            let what = what
                .trim_end_matches(" INVERSE")
                .trim_start_matches("INVERSE");
            let r = match what.split_once(' ') {
                Some((s, "HALF")) => band(s, 0.5),
                None if what.is_empty() => (WHOLE, WHOLE),
                _ => panic!("no shade {part}"),
            };
            paint(&mut px, c, r, v);
        }
        return Model::Exact(px);
    }
    match name {
        "CHECKER BOARD FILL" => return Model::Exact(checkerboard(c, 0)),
        "INVERSE CHECKER BOARD FILL" => return Model::Exact(checkerboard(c, 1)),
        // the second and fourth quarters down, so the bars tile down a column
        "HEAVY HORIZONTAL FILL" => {
            paint(&mut px, c, (WHOLE, (0.25, 0.5)), 0xff);
            paint(&mut px, c, (WHOLE, (0.75, 1.0)), 0xff);
            return Model::Exact(px);
        }
        "UPPER LEFT TO LOWER RIGHT FILL" => {
            return Model::Cover {
                shapes: hatch(c, false),
                on: 0xff,
                off: 0,
            };
        }
        "UPPER RIGHT TO LOWER LEFT FILL" => {
            return Model::Cover {
                shapes: hatch(c, true),
                on: 0xff,
                off: 0,
            };
        }
        // U+2500's light line across, and a heavy stroke down the middle half,
        // so it differs from U+2542, whose heavy line runs the cell's height
        "BOX DRAWINGS LIGHT HORIZONTAL WITH VERTICAL STROKE" => {
            let y0 = c.h.saturating_sub(c.t) / 2;
            let x0 = c.w.saturating_sub(2 * c.t) / 2;
            let (v0, v1) = edges(c.h, 0.25, 0.75);
            for y in 0..c.h {
                for x in 0..c.w {
                    let across = (y0..y0 + c.t).contains(&y);
                    let down = (x0..x0 + 2 * c.t).contains(&x) && (v0..v1).contains(&y);
                    if across || down {
                        px[(y * c.w + x) as usize] = 0xff;
                    }
                }
            }
            return Model::Exact(px);
        }
        _ => {}
    }
    let digits = |s: &str| s.bytes().map(|d| f64::from(d - b'0')).collect::<Vec<_>>();
    if let Some(n) = name.strip_prefix("VERTICAL ONE EIGHTH BLOCK-") {
        for d in digits(n) {
            paint(&mut px, c, (((d - 1.0) / 8.0, d / 8.0), WHOLE), 0xff);
        }
        return Model::Exact(px);
    }
    if let Some(n) = name.strip_prefix("HORIZONTAL ONE EIGHTH BLOCK-") {
        for d in digits(n) {
            paint(&mut px, c, (WHOLE, ((d - 1.0) / 8.0, d / 8.0)), 0xff);
        }
        return Model::Exact(px);
    }
    if let Some(sides) = name.strip_suffix(" ONE EIGHTH BLOCK") {
        for s in sides.split(" AND ") {
            paint(&mut px, c, band(s, 0.125), 0xff);
        }
        return Model::Exact(px);
    }
    // "UPPER CENTRE ONE QUARTER BLOCK": a block half the cell's width and
    // half its height, centred on the side
    let quarter = match name {
        "UPPER CENTRE ONE QUARTER BLOCK" => Some(((0.25, 0.75), (0.0, 0.5))),
        "LOWER CENTRE ONE QUARTER BLOCK" => Some(((0.25, 0.75), (0.5, 1.0))),
        "MIDDLE LEFT ONE QUARTER BLOCK" => Some(((0.0, 0.5), (0.25, 0.75))),
        "MIDDLE RIGHT ONE QUARTER BLOCK" => Some(((0.5, 1.0), (0.25, 0.75))),
        _ => None,
    };
    if let Some(r) = quarter {
        paint(&mut px, c, r, 0xff);
        return Model::Exact(px);
    }
    // "RIGHT HALF LOWER ONE QUARTER BLOCK": the lower quarter band, over the
    // right half; "UPPER ONE QUARTER BLOCK", "LEFT TWO THIRDS BLOCK": a band
    let words: Vec<&str> = name
        .strip_suffix(" BLOCK")
        .expect("a block")
        .split(' ')
        .collect();
    let r = match words.as_slice() {
        [side, amount @ .., across, "ONE", "QUARTER"] if !amount.is_empty() => {
            meet(band(side, fraction(amount)), band(across, 0.25))
        }
        [side, amount @ ..] => band(side, fraction(amount)),
        _ => panic!("no model for {name}"),
    };
    paint(&mut px, c, r, 0xff);
    Model::Exact(px)
}

/// Failures, reported together, the first 20 in full.
#[derive(Default)]
struct Failures(Vec<String>);

impl Failures {
    fn fail(&mut self, cp: u32, c: Cell, why: String) {
        self.0.push(format!(
            "U+{cp:04X} at {}x{} thickness {}: {why}",
            c.w, c.h, c.t
        ));
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

fn check(cp: u32, name: &str, c: Cell, f: &mut Failures) {
    let g = draw(cp, c.w, c.h, c.t);
    let px = g.pixels();
    match model(name, c) {
        Model::Exact(want) => {
            if px != want.as_slice() {
                f.fail(cp, c, "differs from the model".into());
            }
        }
        Model::Cover { shapes, on, off } => {
            if c.w >= 4 && c.h >= 8 && px.iter().all(|&p| p == off) {
                f.fail(cp, c, "blank".into());
            }
            for y in 0..c.h {
                for x in 0..c.w {
                    let v = px[(y * c.w + x) as usize];
                    // straight-edged shapes, which never overlap but along an
                    // edge: within a sample a sample row, and a step of the
                    // rounding down, of the exact area
                    let share: Option<f64> = shapes.iter().map(|s| s.share(x, y)).sum();
                    if let Some(share) = share {
                        let exact = share * f64::from(on);
                        let slack = f64::from(on) / f64::from(SS) + 1.0;
                        if (f64::from(v) - exact).abs() > slack {
                            f.fail(cp, c, format!("{v:#04x} at {x},{y}, area {exact:.1}"));
                            return;
                        }
                    }
                    let want = if shapes.iter().any(|s| s.covers(x, y)) {
                        Some(on)
                    } else if shapes.iter().all(|s| s.misses(x, y)) {
                        Some(off)
                    } else {
                        None
                    };
                    let bad = match want {
                        Some(w) => v != w,
                        None => v > on.max(off),
                    };
                    if bad {
                        f.fail(cp, c, format!("{v:#04x} at {x},{y}, want {want:?}"));
                        return;
                    }
                }
            }
        }
    }
}

/// The light line's thickness down the sizes, as a font's underline grows.
fn thickness(h: u32) -> u32 {
    1 + h / 24
}

// covers: text.sprites
#[test]
fn every_codepoint_in_the_ranges_is_a_sprite_or_left_to_the_face() {
    for (first, last) in ISSUE_RANGES {
        for cp in first..=last {
            let drawn = named(cp).is_some();
            let left = in_ranges(&LEFT_TO_THE_FACE, cp) || in_ranges(&UNASSIGNED, cp);
            assert!(
                drawn != left,
                "U+{cp:04X}: drawn {drawn}, left to the face {left}"
            );
            let c = char::from_u32(cp).expect("a char");
            assert_eq!(sprite::has(c), drawn, "U+{cp:04X}");
            assert_eq!(sprite::draw(c, 9, 17, 1).is_some(), drawn, "U+{cp:04X}");
        }
    }
    // the table holds only the ranges, in order, once each
    assert!(NAMES.windows(2).all(|p| p[0].0 < p[1].0));
    assert!(NAMES.iter().all(|&(cp, _)| in_ranges(&ISSUE_RANGES, cp)));
    // the sixteenth blocks below the range and the pictographs above it
    // stay with the face
    for cp in [0x1ce9f, 0x1ceb0, 0x1cebf] {
        assert!(
            !sprite::has(char::from_u32(cp).expect("a char")),
            "U+{cp:04X}"
        );
    }
}

/// Every sprite against the model in each cell from 1 to 32 wide and
/// `heights` high.
fn sweep(heights: std::ops::RangeInclusive<u32>) {
    let mut f = Failures::default();
    for h in heights {
        for w in 1..=32 {
            let c = Cell {
                w,
                h,
                t: thickness(h),
            };
            for (cp, name) in NAMES {
                check(cp, name, c, &mut f);
            }
        }
    }
    f.assert_none();
}

// The cells from 1x1 to 32x64, a band of heights a test so they run side
// by side.
// covers: text.sprites
#[test]
fn cells_1_to_24_high_match_the_model() {
    sweep(1..=24);
}

// covers: text.sprites
#[test]
fn cells_25_to_40_high_match_the_model() {
    sweep(25..=40);
}

// covers: text.sprites
#[test]
fn cells_41_to_52_high_match_the_model() {
    sweep(41..=52);
}

// covers: text.sprites
#[test]
fn cells_53_to_64_high_match_the_model() {
    sweep(53..=64);
}

// covers: text.sprites
#[test]
fn large_cells_match_the_model() {
    let mut f = Failures::default();
    for c in [
        Cell {
            w: 253,
            h: 509,
            t: 13,
        },
        Cell {
            w: 120,
            h: 80,
            t: 4,
        },
    ] {
        for (cp, name) in NAMES {
            check(cp, name, c, &mut f);
        }
    }
    // the blocks, eighths, shades and fills in the large cell the other
    // sprites are checked in
    let c = Cell {
        w: 1000,
        h: 2000,
        t: 1,
    };
    for (cp, name) in NAMES {
        if matches!(model(name, Cell { w: 2, h: 2, t: 1 }), Model::Exact(_)) {
            check(cp, name, c, &mut f);
        }
    }
    f.assert_none();
}

/// Sprites as text, a row a line: `#` fully on, `.` off, `:` the medium
/// shade (0x80), and a digit for any other coverage, in tenths rounded
/// down, at least 1.
fn mask(g: &Glyph) -> String {
    g.rows()
        .map(|row| {
            row.iter()
                .map(|&p| match p {
                    0xff => '#',
                    0 => '.',
                    0x80 => ':',
                    _ => char::from(b'0' + (u32::from(p) * 10 / 256).max(1) as u8),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A character, its cell and light line thickness, and its mask.
type Literal = (u32, u32, u32, u32, &'static str);

/// Masks of sprites from each family, odd widths among them. The blocks,
/// eighths, quarters, shades and checkerboards were worked out by hand from
/// the rounding rule; the slanted and curved shapes were drawn, then read
/// against the code charts' glyphs and the geometry the model gives.
#[rustfmt::skip]
const LITERALS: &[Literal] = &[
    (0x1cea0, 9, 17, 1, "
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
....#####
....#####
....#####
....#####
"),
    (0x1cea5, 8, 15, 1, "
........
........
........
........
##......
##......
##......
##......
##......
##......
##......
##......
##......
##......
##......
"),
    (0x1fb82, 9, 17, 1, "
#########
#########
#########
#########
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
"),
    (0x1fb85, 7, 13, 1, "
#######
#######
#######
#######
#######
#######
#######
#######
#######
#######
.......
.......
.......
"),
    (0x1fbe4, 9, 17, 1, "
..#####..
..#####..
..#####..
..#####..
..#####..
..#####..
..#####..
..#####..
..#####..
.........
.........
.........
.........
.........
.........
.........
.........
"),
    (0x1fbe6, 10, 15, 1, "
..........
..........
..........
..........
#####.....
#####.....
#####.....
#####.....
#####.....
#####.....
#####.....
..........
..........
..........
..........
"),
    (0x1fb3c, 9, 18, 1, "
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
3........
91.......
#81......
##6......
###3.....
###91....
"),
    (0x1fb41, 8, 16, 1, "
...3####
..19####
.18#####
.6######
3#######
9#######
########
########
########
########
########
########
########
########
########
########
"),
    (0x1fb4b, 9, 18, 1, "
........1
........3
........6
........8
.......1#
.......3#
.......6#
.......8#
......1##
......3##
......6##
......8##
.....1###
.....3###
.....6###
.....8###
....1####
....3####
"),
    (0x1fb67, 7, 15, 1, "
#######
#######
#######
#######
#######
39#####
.17####
..159##
....28#
.....16
.......
.......
.......
.......
.......
"),
    (0x1fb6c, 9, 18, 1, "
2........
7........
#2.......
#7.......
##2......
##7......
###2.....
###7.....
####2....
####2....
###7.....
###2.....
##7......
##2......
#7.......
#2.......
7........
2........
"),
    (0x1fb68, 8, 16, 1, "
7#######
2#######
.7######
.2######
..7#####
..2#####
...7####
...2####
...2####
...7####
..2#####
..7#####
.2######
.7######
2#######
7#######
"),
    (0x1fb9a, 9, 16, 1, "
7#######7
19#####91
.5#####5.
.19###91.
..4###4..
..18#81..
...3#3...
....5....
....5....
...3#3...
..18#81..
..4###4..
.19###91.
.5#####5.
19#####91
7#######7
"),
    (0x1fb70, 9, 6, 1, "
.#.......
.#.......
.#.......
.#.......
.#.......
.#.......
"),
    (0x1fb7c, 9, 17, 1, "
#........
#........
#........
#........
#........
#........
#........
#........
#........
#........
#........
#........
#........
#........
#........
#########
#########
"),
    (0x1fb81, 8, 16, 1, "
########
########
........
........
########
########
........
........
########
########
........
........
........
........
########
########
"),
    (0x1fb91, 7, 9, 1, "
#######
#######
#######
#######
#######
:::::::
:::::::
:::::::
:::::::
"),
    (0x1fb8c, 9, 8, 1, "
:::::....
:::::....
:::::....
:::::....
:::::....
:::::....
:::::....
:::::....
"),
    (0x1fb95, 8, 16, 1, "
##..##..
##..##..
..##..##
..##..##
##..##..
##..##..
..##..##
..##..##
##..##..
##..##..
..##..##
..##..##
##..##..
##..##..
..##..##
..##..##
"),
    (0x1fb96, 9, 13, 1, "
..###..##
..###..##
##...##..
##...##..
..###..##
..###..##
..###..##
##...##..
##...##..
..###..##
..###..##
##...##..
##...##..
"),
    (0x1fb97, 9, 17, 1, "
.........
.........
.........
.........
#########
#########
#########
#########
#########
.........
.........
.........
.........
#########
#########
#########
#########
"),
    (0x1fb98, 8, 16, 1, "
8.641913
8319146.
38.64191
.8319146
138.6419
6.831914
9138.641
46.83191
19138.64
146.8319
419138.6
9146.831
6419138.
19146.83
.6419138
319146.8
"),
    (0x1fb99, 9, 18, 2, "
8118#3.3#
3.3#8118#
118#3.3#8
.3#8118#3
18#3.3#81
3#8118#3.
8#3.3#811
#8118#3.3
#3.3#8118
8118#3.3#
3.3#8118#
118#3.3#8
.3#8118#3
18#3.3#81
3#8118#3.
8#3.3#811
#8118#3.3
#3.3#8118
"),
    (0x1fb9c, 9, 12, 1, "
::::::::3
:::::::41
::::::41.
::::::1..
:::::3...
::::41...
:::41....
:::1.....
::3......
:41......
41.......
1........
"),
    (0x1fba0, 9, 18, 1, "
...38....
..182....
..37.....
.181.....
.46......
191......
46.......
91.......
5........
1........
.........
.........
.........
.........
.........
.........
.........
.........
"),
    (0x1fbae, 11, 18, 2, "
...4##5....
..19##92...
..5#75#81..
.19#118#5..
.6#6..2992.
1#91...5#81
7#5....18#5
#91.....299
#4.......5#
#4.......5#
#91.....299
7#5....18#5
1#91...5#81
.6#6..2992.
.19#118#5..
..5#75#81..
..19##92...
...4##5....
"),
    (0x1fbaf, 9, 17, 1, "
.........
.........
.........
.........
...##....
...##....
...##....
...##....
#########
...##....
...##....
...##....
...##....
.........
.........
.........
.........
"),
    (0x1fbbd, 9, 14, 1, "
19#####91
34#####43
917###719
#619#916#
##24#42##
##81618##
###5.5###
###5.5###
##81618##
##24#42##
#619#916#
917###719
34#####43
19#####91
"),
    (0x1fbbf, 8, 16, 1, "
##6.6###
##2419##
#71944##
#35#917#
819##62#
34####26
19####71
3######2
19####81
26####34
62###819
918##35#
#54#719#
#91926##
##3319##
##7.6###
"),
    (0x1fbce, 9, 6, 1, "
######...
######...
######...
######...
######...
######...
"),
    (0x1fbd0, 9, 18, 1, "
.........
.........
.........
.........
.........
.........
.........
........1
.......16
.......67
......571
.....571.
....481..
...481...
..391....
.381.....
391......
92.......
"),
    (0x1fbd8, 9, 18, 1, "
8.......8
73.....37
281...182
.64...46.
.191.191.
..64.46..
..19191..
...585...
...191...
.........
.........
.........
.........
.........
.........
.........
.........
.........
"),
    (0x1fbe0, 9, 18, 1, "
91.....19
73.....37
281...182
.3964693.
..13431..
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
"),
    (0x1fbe8, 10, 18, 1, "
9########9
7########7
2########2
.5######5.
..279972..
..........
..........
..........
..........
..........
..........
..........
..........
..........
..........
..........
..........
..........
"),
    (0x1fbec, 9, 18, 1, "
....4####
....2####
.....7###
.....17##
.......24
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
.........
"),
];

// covers: text.sprites
#[test]
fn chosen_sprites_match_their_literal_masks() {
    let mut failures = Vec::new();
    for &(cp, w, h, t, want) in LITERALS {
        let got = mask(&draw(cp, w, h, t));
        if got != want.trim_matches('\n') {
            failures.push(format!("U+{cp:04X} at {w}x{h} thickness {t}:\n{got}\n"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
