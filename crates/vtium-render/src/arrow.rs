//! The pointer's image: an arrow a cell tall, white with a dark outline,
//! its hotspot at the tip, turned with the display; or, under
//! `pointer-char`, a glyph of the font in the same colors, its hotspot at
//! its top left, which the renderer puts on the corner of the cell under the
//! pointer.
//!
//! A program can ask for another shape with OSC 22, and vtium has a picture
//! for each shape of its fixed set ([`PointerShape`]): an I-beam over text,
//! a hand over a link, a crosshair, an hourglass, the arrow with a small
//! hourglass, a circle with a slash, four arrows for a move, and double
//! arrows for resizing sideways and up and down. Each one is drawn the same
//! way as the arrow: polygons, rectangles and circles on a grid of units
//! that is scaled to the cell's height, filled white and outlined, with its
//! hotspot where the shape points or at its centre.
//!
//! One image serves both ways of showing the pointer: the display's cursor
//! plane takes it as it is, and the renderer blends it over the cells it
//! draws on a display without one. It is a cell tall, kept between 8 and
//! [`POINTER_SIDE_MAX`] pixels, white with a black outline at 220 of 255,
//! so it shows over light and dark cells alike. Its outline is one pixel wide
//! and one more for each 32 of its height.

use vtium_font::{Format, Glyph};
use vtium_term::{PointerShape, Rgb};

use crate::blend::over_bg;
use crate::bounds::POINTER_SIDE_MAX;
use crate::compose::{Area, int};
use crate::target::{Orientation, Target};

/// The opaque white the arrow fills with.
const FILL: u32 = 0xffff_ffff;

/// The dark outline around it, premultiplied: black at 220 of 255.
const OUTLINE: u32 = 0xdc00_0000;

/// The arrow's outline as a polygon in 18ths of its height, tip first: the
/// left edge, the notch, the tail and the right wing.
const SHAPE: [(i64, i64); 7] = [
    (0, 0),
    (0, 16),
    (4, 12),
    (7, 18),
    (9, 17),
    (6, 11),
    (11, 11),
];

/// The units [`SHAPE`] is drawn in, to one height.
const UNITS: i64 = 18;

/// The arrow's widest point, in [`UNITS`].
const SHAPE_WIDTH: i64 = 11;

/// One part of a shape's picture, in the units of its [`Sprite`]. A pixel
/// is inside the picture when its centre lies inside any of its parts.
enum Part {
    /// A polygon, by the even-odd rule.
    Poly(&'static [(i64, i64)]),
    /// A rectangle from its left and top edges to its right and bottom
    /// edges.
    Rect(i64, i64, i64, i64),
    /// A disc by its centre and radius.
    Disc(i64, i64, i64),
    /// A ring by its centre and its outer and inner radius.
    Ring(i64, i64, i64, i64),
}

/// A shape's picture: its parts on a grid `w` units wide and `h` units
/// tall, which is scaled so that `h` units fill the cell's height inside
/// the outline, and the unit its hotspot lies at.
struct Sprite {
    w: i64,
    h: i64,
    hot: (i64, i64),
    parts: &'static [Part],
}

/// The units a sprite is drawn in, to one height. Strokes are 4 units
/// wide, so each one covers at least one pixel centre in a picture 12
/// pixels tall inside its outline.
const SPRITE_UNITS: i64 = 36;

/// The fewest pixels a sprite's picture is tall, its outline included. On
/// a font whose cells are shorter, such as an 8×8 font, the shape draws a
/// little taller than a cell, because its strokes would otherwise fall
/// between the pixels and vanish.
const SPRITE_HEIGHT_MIN: i64 = 14;

/// The I-beam over text: a stem with a bar across each end, its hotspot
/// at its centre.
const TEXT: Sprite = Sprite {
    w: 14,
    h: SPRITE_UNITS,
    hot: (7, 18),
    parts: &[
        Part::Rect(5, 0, 9, 36),
        Part::Rect(0, 0, 14, 4),
        Part::Rect(0, 32, 14, 36),
    ],
};

/// A hand with its index finger raised, over a link or a button: the
/// finger's tip is the hotspot.
const HAND: Sprite = Sprite {
    w: 26,
    h: SPRITE_UNITS,
    hot: (10, 0),
    parts: &[
        // the index finger, rounded at its tip
        Part::Disc(10, 3, 3),
        Part::Rect(7, 3, 13, 18),
        // the three folded fingers' knuckles
        Part::Disc(15, 14, 3),
        Part::Disc(20, 15, 3),
        Part::Disc(24, 17, 2),
        // the palm and the thumb along its left side
        Part::Rect(7, 15, 26, 30),
        Part::Poly(&[(7, 20), (2, 16), (0, 19), (7, 30)]),
        // the wrist
        Part::Rect(9, 29, 24, 36),
    ],
};

/// A thin cross for picking a point, its hotspot at its centre.
const CROSSHAIR: Sprite = Sprite {
    w: 34,
    h: SPRITE_UNITS,
    hot: (17, 18),
    parts: &[Part::Rect(0, 16, 34, 20), Part::Rect(15, 1, 19, 35)],
};

/// An hourglass for a program that is busy, its hotspot at its waist.
const WAIT: Sprite = Sprite {
    w: 24,
    h: SPRITE_UNITS,
    hot: (12, 18),
    parts: &[
        Part::Rect(0, 0, 24, 4),
        Part::Rect(0, 32, 24, 36),
        Part::Poly(&[(2, 4), (22, 4), (14, 18), (22, 32), (2, 32), (10, 18)]),
    ],
};

/// The arrow with a small hourglass at its lower right, for a program that
/// is busy but still takes input: the arrow's tip is the hotspot.
const PROGRESS: Sprite = Sprite {
    w: 34,
    h: SPRITE_UNITS,
    hot: (0, 0),
    parts: &[
        // the arrow, as SHAPE draws it, in this grid's units
        Part::Poly(&[
            (0, 0),
            (0, 32),
            (8, 24),
            (14, 36),
            (18, 34),
            (12, 22),
            (22, 22),
        ]),
        Part::Rect(20, 16, 34, 19),
        Part::Rect(20, 33, 34, 36),
        Part::Poly(&[(21, 19), (33, 19), (28, 26), (33, 33), (21, 33), (26, 26)]),
    ],
};

/// A circle with a slash from its upper left to its lower right: the
/// action is not allowed. Its hotspot is at its centre.
const NOT_ALLOWED: Sprite = Sprite {
    w: 34,
    h: SPRITE_UNITS,
    hot: (17, 18),
    parts: &[
        Part::Ring(17, 18, 17, 12),
        Part::Poly(&[(9, 7), (28, 26), (25, 29), (6, 10)]),
    ],
};

/// A cross of four arrows for something that can be moved, its hotspot at
/// its centre.
const MOVE: Sprite = Sprite {
    w: 36,
    h: SPRITE_UNITS,
    hot: (18, 18),
    parts: &[
        Part::Rect(6, 16, 30, 20),
        Part::Rect(16, 6, 20, 30),
        Part::Poly(&[(0, 18), (8, 10), (8, 26)]),
        Part::Poly(&[(36, 18), (28, 10), (28, 26)]),
        Part::Poly(&[(18, 0), (10, 8), (26, 8)]),
        Part::Poly(&[(18, 36), (10, 28), (26, 28)]),
    ],
};

/// A double arrow pointing left and right, for resizing sideways, its
/// hotspot at its centre.
const EW_RESIZE: Sprite = Sprite {
    w: 36,
    h: SPRITE_UNITS,
    hot: (18, 18),
    parts: &[
        Part::Rect(8, 16, 28, 20),
        Part::Poly(&[(0, 18), (9, 9), (9, 27)]),
        Part::Poly(&[(36, 18), (27, 9), (27, 27)]),
    ],
};

/// A double arrow pointing up and down, for resizing up or down, its
/// hotspot at its centre.
const NS_RESIZE: Sprite = Sprite {
    w: 18,
    h: SPRITE_UNITS,
    hot: (9, 18),
    parts: &[
        Part::Rect(7, 8, 11, 28),
        Part::Poly(&[(9, 0), (0, 9), (18, 9)]),
        Part::Poly(&[(9, 36), (0, 27), (18, 27)]),
    ],
};

/// One part of a shape's picture as the pointer file's generator writes
/// it, in the units of the shape's grid.
pub(crate) enum PartOf {
    Poly(&'static [(i64, i64)]),
    Rect(i64, i64, i64, i64),
    Disc(i64, i64, i64),
    Ring(i64, i64, i64, i64),
}

/// The grid of `shape`'s picture, `(width, height)` in its units, its
/// hotspot's unit, and its parts, as the compiled-in picture draws them:
/// what the pointer file's generator writes as the shape's symbol.
pub(crate) fn grid(shape: PointerShape) -> ((i64, i64), (i64, i64), Vec<PartOf>) {
    let Some(sprite) = sprite(shape) else {
        return ((SHAPE_WIDTH, UNITS), (0, 0), vec![PartOf::Poly(&SHAPE)]);
    };
    let parts = sprite
        .parts
        .iter()
        .map(|p| match *p {
            Part::Poly(poly) => PartOf::Poly(poly),
            Part::Rect(l, t, r, b) => PartOf::Rect(l, t, r, b),
            Part::Disc(x, y, r) => PartOf::Disc(x, y, r),
            Part::Ring(x, y, o, i) => PartOf::Ring(x, y, o, i),
        })
        .collect();
    ((sprite.w, sprite.h), sprite.hot, parts)
}

/// The picture of each shape but the default one, which is the arrow or
/// the glyph `pointer-char` names.
fn sprite(shape: PointerShape) -> Option<&'static Sprite> {
    match shape {
        PointerShape::Default => None,
        PointerShape::Text => Some(&TEXT),
        PointerShape::Pointer => Some(&HAND),
        PointerShape::Crosshair => Some(&CROSSHAIR),
        PointerShape::Wait => Some(&WAIT),
        PointerShape::Progress => Some(&PROGRESS),
        PointerShape::NotAllowed => Some(&NOT_ALLOWED),
        PointerShape::Move => Some(&MOVE),
        PointerShape::EwResize => Some(&EW_RESIZE),
        PointerShape::NsResize => Some(&NS_RESIZE),
    }
}

/// The pointer's image: premultiplied ARGB8888, `width` pixels a row,
/// with the pixel the pointer's position names at `hot`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Arrow {
    pixels: Vec<u32>,
    width: u32,
    height: u32,
    hot: (u32, u32),
}

impl Arrow {
    /// The arrow for cells `cell_height` pixels tall on a display turned
    /// as `orientation` says: pointing at the text's top left.
    pub fn new(cell_height: u32, orientation: Orientation) -> Arrow {
        Arrow::upright(cell_height).turned(orientation)
    }

    /// `glyph` as the pointer on a display turned as `orientation` says:
    /// each pixel it covers at least half filled, outlined as the arrow is
    /// for the glyph's height, with its top left at the hotspot.
    pub fn glyph(glyph: &Glyph, orientation: Orientation) -> Arrow {
        Arrow::upright_glyph(glyph).turned(orientation)
    }

    /// [`Arrow::glyph`] upright, as the GPU path draws it into the upright
    /// frame (ADR-1303 D3).
    pub fn upright_glyph(glyph: &Glyph) -> Arrow {
        let (gw, gh) = (i64::from(glyph.width()), i64::from(glyph.height()));
        let thk = (gh.clamp(8, i64::from(POINTER_SIDE_MAX)) / 32).saturating_add(1);
        let covered = |x: i64, y: i64| {
            let (x, y) = (x.saturating_sub(thk), y.saturating_sub(thk));
            if x < 0 || y < 0 || x >= gw || y >= gh {
                return false;
            }
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                return false;
            };
            let at = match glyph.format() {
                Format::Mask => 0,
                Format::Rgba => 3,
            };
            glyph
                .pixel(x, y)
                .and_then(|p| p.get(at))
                .is_some_and(|&c| c >= 128)
        };
        Arrow::filled(gw, gh, thk, covered, (thk, thk))
    }

    /// A picture the font helper drew from a pointer file, upright, as
    /// `width` × `height` premultiplied RGBA bytes with its hotspot at
    /// `hot`. The console never trusts the helper's answer unchecked, so
    /// the picture is refused unless each side is 1 to
    /// [`POINTER_SIDE_MAX`], the bytes hold exactly that many pixels, the
    /// hotspot lies inside the picture, and no color exceeds its alpha.
    /// The error says which check failed.
    ///
    /// References:
    /// - ADR-1309 D5: the console's checks of each picture the helper sends.
    pub fn from_rgba(
        width: u32,
        height: u32,
        hot: (u32, u32),
        rgba: &[u8],
    ) -> Result<Arrow, String> {
        let side = 1..=POINTER_SIDE_MAX;
        if !side.contains(&width) || !side.contains(&height) {
            return Err(format!(
                "a picture of {width} × {height} pixels, outside 1 to {POINTER_SIDE_MAX} a side"
            ));
        }
        let n = usize::try_from(width.saturating_mul(height)).unwrap_or(usize::MAX);
        if rgba.len() != n.saturating_mul(4) {
            return Err(format!(
                "a picture of {width} × {height} pixels whose rows hold {} bytes",
                rgba.len()
            ));
        }
        if hot.0 >= width || hot.1 >= height {
            return Err(format!(
                "a picture whose hotspot {},{} lies outside its {width} × {height} pixels",
                hot.0, hot.1
            ));
        }
        let mut pixels = Vec::with_capacity(n);
        for &[r, g, b, a] in rgba.as_chunks::<4>().0 {
            if r > a || g > a || b > a {
                return Err("a picture with a color above its alpha".to_owned());
            }
            pixels.push(u32::from_be_bytes([a, r, g, b]));
        }
        Ok(Arrow {
            pixels,
            width,
            height,
            hot,
        })
    }

    /// Whether it fills any pixel: a blank glyph's image fills none.
    pub(crate) fn covers(&self) -> bool {
        self.pixels.contains(&FILL)
    }

    /// Every pixel, row by row.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The pixel the pointer's position names: the tip.
    pub fn hot(&self) -> (u32, u32) {
        self.hot
    }

    /// The pixel at `x`, `y`, transparent outside.
    pub(crate) fn pixel(&self, x: i64, y: i64) -> u32 {
        if x < 0 || y < 0 || x >= i64::from(self.width) {
            return 0;
        }
        y.checked_mul(i64::from(self.width))
            .and_then(|i| i.checked_add(x))
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.pixels.get(i).copied())
            .unwrap_or(0)
    }

    /// The arrow for cells `cell_height` pixels tall, upright, pointing up
    /// and left: [`Arrow::new`] on an unturned display, and what the GPU
    /// path draws into the upright frame (ADR-1303 D3).
    pub fn upright(cell_height: u32) -> Arrow {
        let side = i64::from(POINTER_SIDE_MAX);
        let h = i64::from(cell_height).clamp(8, side);
        let thk = (h / 32).saturating_add(1);
        let shape_h = h.saturating_sub(thk.saturating_mul(2));
        let shape_w = SHAPE_WIDTH
            .saturating_mul(shape_h)
            .saturating_add(UNITS.saturating_sub(1))
            / UNITS;
        let w = shape_w.min(side.saturating_sub(thk.saturating_mul(2)));
        let inside = |x: i64, y: i64| {
            let (x, y) = (x.saturating_sub(thk), y.saturating_sub(thk));
            x >= 0 && y >= 0 && x < shape_w && y < shape_h && in_shape(x, y, shape_h)
        };
        Arrow::filled(w, shape_h, thk, inside, (thk, thk))
    }

    /// The picture of `shape` for cells `cell_height` pixels tall, upright,
    /// as the GPU path draws it into the upright frame. The default shape
    /// is the arrow, and every other shape is its own picture, drawn the
    /// same height as the arrow and outlined the same way, but never less
    /// than [`SPRITE_HEIGHT_MIN`] pixels tall.
    pub fn upright_shape(shape: PointerShape, cell_height: u32) -> Arrow {
        let Some(sprite) = sprite(shape) else {
            return Arrow::upright(cell_height);
        };
        let side = i64::from(POINTER_SIDE_MAX);
        let h = i64::from(cell_height).clamp(SPRITE_HEIGHT_MIN, side);
        let thk = (h / 32).saturating_add(1);
        let shape_h = h.saturating_sub(thk.saturating_mul(2));
        let shape_w = sprite
            .w
            .saturating_mul(shape_h)
            .saturating_add(sprite.h.saturating_sub(1))
            .checked_div(sprite.h)
            .unwrap_or(0);
        let w = shape_w.min(side.saturating_sub(thk.saturating_mul(2)));
        let inside = |x: i64, y: i64| {
            let (x, y) = (x.saturating_sub(thk), y.saturating_sub(thk));
            x >= 0 && y >= 0 && x < w && y < shape_h && in_sprite(sprite, x, y, shape_h)
        };
        // the hotspot's unit, scaled as the parts are, kept on the picture
        let at = |u: i64, n: i64| {
            u.saturating_mul(shape_h)
                .checked_div(sprite.h)
                .unwrap_or(0)
                .min(n.saturating_sub(1))
                .max(0)
                .saturating_add(thk)
        };
        let hot = (at(sprite.hot.0, w), at(sprite.hot.1, shape_h));
        Arrow::filled(w, shape_h, thk, inside, hot)
    }

    /// [`Arrow::upright_shape`] on a display turned as `orientation` says.
    pub fn shape(shape: PointerShape, cell_height: u32, orientation: Orientation) -> Arrow {
        Arrow::upright_shape(shape, cell_height).turned(orientation)
    }

    /// A shape `w` by `h` with an outline `thk` wide all round: white
    /// where `inside` holds, the outline where it holds within `thk`,
    /// clear elsewhere, with the hotspot at `hot`.
    fn filled(
        w: i64,
        h: i64,
        thk: i64,
        inside: impl Fn(i64, i64) -> bool,
        hot: (i64, i64),
    ) -> Arrow {
        let (w, h) = (
            w.saturating_add(thk.saturating_mul(2)),
            h.saturating_add(thk.saturating_mul(2)),
        );
        let near = |x: i64, y: i64| {
            (y.saturating_sub(thk)..=y.saturating_add(thk))
                .any(|ny| (x.saturating_sub(thk)..=x.saturating_add(thk)).any(|nx| inside(nx, ny)))
        };
        let mut pixels = Vec::new();
        for y in 0..h {
            for x in 0..w {
                pixels.push(if inside(x, y) {
                    FILL
                } else if near(x, y) {
                    OUTLINE
                } else {
                    0
                });
            }
        }
        let side = |v: i64| u32::try_from(v).unwrap_or(0);
        Arrow {
            pixels,
            width: side(w),
            height: side(h),
            hot: (side(hot.0), side(hot.1)),
        }
    }

    /// This arrow on a display turned as `o` says, which draws the grid's
    /// pixel `x`, `y` where [`Orientation`] puts it.
    pub(crate) fn turned(self, o: Orientation) -> Arrow {
        if o == Orientation::Normal {
            return self;
        }
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        let last = |n: i64, v: i64| n.saturating_sub(1).saturating_sub(v);
        let place = |x: i64, y: i64| match o {
            Orientation::Normal => (x, y),
            Orientation::UpsideDown => (last(w, x), last(h, y)),
            Orientation::Right => (last(h, y), x),
            Orientation::Left => (y, last(w, x)),
        };
        let (tw, th) = if o.turned() { (h, w) } else { (w, h) };
        let mut pixels = vec![0; self.pixels.len()];
        for y in 0..h {
            for x in 0..w {
                let (tx, ty) = place(x, y);
                let at = ty.saturating_mul(tw).saturating_add(tx);
                if let Some(p) = usize::try_from(at).ok().and_then(|i| pixels.get_mut(i)) {
                    *p = self.pixel(x, y);
                }
            }
        }
        let (hx, hy) = place(i64::from(self.hot.0), i64::from(self.hot.1));
        let side = |v: i64| u32::try_from(v).unwrap_or(0);
        Arrow {
            pixels,
            width: side(tw),
            height: side(th),
            hot: (side(hx), side(hy)),
        }
    }
}

/// A target that blends the arrow, placed at `at` on the display, over
/// every run of pixels written beneath it: the pointer drawn over the
/// cells and the border as they are drawn, never read back.
pub(crate) struct Over<'a, T: Target + ?Sized> {
    target: &'a mut T,
    arrow: Option<(&'a Arrow, Area)>,
    line: &'a mut Vec<u32>,
}

impl<'a, T: Target + ?Sized> Over<'a, T> {
    /// Writes through to `target`, with `arrow` at its area when set;
    /// `line` is the scratch a run under it is blended in.
    pub(crate) fn new(
        target: &'a mut T,
        arrow: Option<(&'a Arrow, Area)>,
        line: &'a mut Vec<u32>,
    ) -> Over<'a, T> {
        Over {
            target,
            arrow,
            line,
        }
    }
}

impl<T: Target + ?Sized> Target for Over<'_, T> {
    fn write(&mut self, x: u32, y: u32, pixels: &[u32]) {
        let Some((arrow, at)) = self.arrow else {
            return self.target.write(x, y, pixels);
        };
        let (x0, y0) = (int(x), int(y));
        let end = x0.saturating_add(int(pixels.len()));
        let (from, to) = (x0.max(at.x), end.min(at.x.saturating_add(at.w)));
        if y0 < at.y || y0 >= at.bottom() || from >= to {
            return self.target.write(x, y, pixels);
        }
        self.line.clear();
        self.line.extend_from_slice(pixels);
        let ay = y0.saturating_sub(at.y);
        for px in from..to {
            let src = arrow.pixel(px.saturating_sub(at.x), ay);
            let i = usize::try_from(px.saturating_sub(x0)).unwrap_or(usize::MAX);
            if let Some(p) = self.line.get_mut(i).filter(|_| src != 0) {
                let [_, r, g, b] = p.to_be_bytes();
                *p = over_bg(Rgb::new(r, g, b), src);
            }
        }
        self.target.write(x, y, self.line);
    }
}

/// Whether the centre of pixel `x`, `y` lies in [`SHAPE`] drawn `h` pixels
/// tall.
fn in_shape(x: i64, y: i64, h: i64) -> bool {
    in_polygon(&SHAPE, centre(x, y, UNITS), h.saturating_mul(2))
}

/// The centre of pixel `x`, `y` in a picture drawn in `units` to its
/// height, scaled by twice that height so that it is whole: a point at
/// unit `u` lies at `u` times the scale.
fn centre(x: i64, y: i64, units: i64) -> (i64, i64) {
    (
        x.saturating_mul(2).saturating_add(1).saturating_mul(units),
        y.saturating_mul(2).saturating_add(1).saturating_mul(units),
    )
}

/// Whether the centre of pixel `x`, `y` lies in any part of `sprite` drawn
/// `h` pixels tall.
fn in_sprite(sprite: &Sprite, x: i64, y: i64, h: i64) -> bool {
    let (px, py) = centre(x, y, sprite.h);
    let scale = h.saturating_mul(2);
    let at = |v: i64| v.saturating_mul(scale);
    // whether the centre lies within `r` units of the point `cx`, `cy`
    let within = |cx: i64, cy: i64, r: i64| {
        let (dx, dy) = (px.saturating_sub(at(cx)), py.saturating_sub(at(cy)));
        dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy)) <= at(r).saturating_mul(at(r))
    };
    sprite.parts.iter().any(|part| match *part {
        Part::Poly(poly) => in_polygon(poly, (px, py), scale),
        Part::Rect(l, t, r, b) => (at(l)..at(r)).contains(&px) && (at(t)..at(b)).contains(&py),
        Part::Disc(cx, cy, r) => within(cx, cy, r),
        Part::Ring(cx, cy, outer, inner) => within(cx, cy, outer) && !within(cx, cy, inner),
    })
}

/// Whether the point `px`, `py` lies in `poly`, whose corners are scaled by
/// `scale`: the even-odd rule along the point's row, in whole numbers.
fn in_polygon(poly: &[(i64, i64)], (px, py): (i64, i64), scale: i64) -> bool {
    let corner = |(cx, cy): (i64, i64)| (cx.saturating_mul(scale), cy.saturating_mul(scale));
    let mut inside = false;
    let mut prev = poly.last().copied().map(corner).unwrap_or_default();
    for &v in poly {
        let (ax, ay) = prev;
        let (bx, by) = corner(v);
        prev = (bx, by);
        if (ay > py) == (by > py) {
            continue;
        }
        // the edge crosses the row at x = ax + (py - ay) (bx - ax) / (by - ay);
        // the centre lies left of it when px (by - ay) < that, signs kept
        let dy = by.saturating_sub(ay);
        let lhs = px.saturating_sub(ax).saturating_mul(dy);
        let rhs = py.saturating_sub(ay).saturating_mul(bx.saturating_sub(ax));
        if (dy > 0 && lhs < rhs) || (dy < 0 && lhs > rhs) {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use vtium_font::Format;

    /// The arrow as text: `#` fill, `o` outline, `.` clear.
    fn picture(a: &Arrow) -> Vec<String> {
        a.pixels()
            .chunks(usize::try_from(a.width()).unwrap_or(1))
            .map(|row| {
                row.iter()
                    .map(|&p| match p {
                        FILL => '#',
                        OUTLINE => 'o',
                        0 => '.',
                        _ => '?',
                    })
                    .collect()
            })
            .collect()
    }

    // A picture from the font helper keeps the hotspot the helper sent,
    // on either axis and at either edge, so the plane and the renderer put
    // the pointer's tip where the pointer file says it is.
    #[test]
    fn a_picture_from_the_helper_keeps_its_hotspot() {
        for hot in [(0, 0), (1, 0), (0, 1), (4, 2), (2, 4), (4, 4)] {
            let a = Arrow::from_rgba(5, 5, hot, &[0; 5 * 5 * 4]).expect("a valid picture");
            assert_eq!(a.hot(), hot);
        }
        let a = Arrow::from_rgba(3, 7, (2, 6), &[0; 3 * 7 * 4]).expect("a valid picture");
        assert_eq!((a.width(), a.height(), a.hot()), (3, 7, (2, 6)));
    }

    #[test]
    fn the_arrow_is_a_cell_tall_with_its_tip_at_the_hotspot() {
        let a = Arrow::new(16, Orientation::Normal);
        assert_eq!((a.width(), a.height(), a.hot()), (11, 16, (1, 1)));
        let want = [
            "...........",
            "ooo........",
            "o#oo.......",
            "o##oo......",
            "o###oo.....",
            "o####oo....",
            "o#####oo...",
            "o######oo..",
            "o#######oo.",
            "o########o.",
            "o#####oooo.",
            "o##oo##o...",
            "o#ooo##oo..",
            "ooo.oo##o..",
            ".....o#oo..",
            ".....ooo...",
        ];
        assert_eq!(picture(&a), want);
        // the tip is outlined
        assert_eq!(a.pixel(1, 1), OUTLINE);
    }

    // A glyph as the pointer: its covered pixels filled, outlined as the
    // arrow is, its top left at the hotspot, so that it sits on its cell.
    #[test]
    fn a_glyph_is_filled_and_outlined_with_its_corner_at_the_hotspot() {
        let pixels = [
            [255, 0, 0, 0],
            [255, 0, 0, 0],
            [255, 0, 0, 0],
            [255, 255, 255, 0],
        ]
        .concat();
        let g = Glyph::from_pixels(4, 4, Format::Mask, pixels).expect("a glyph");
        let a = Arrow::glyph(&g, Orientation::Normal);
        assert_eq!((a.width(), a.height(), a.hot()), (6, 6, (1, 1)));
        let want = [
            "ooo...", //
            "o#o...", //
            "o#o...", //
            "o#ooo.", //
            "o###o.", //
            "ooooo.",
        ];
        assert_eq!(picture(&a), want);
        // turned with the display, as the arrow is
        let r = Arrow::glyph(&g, Orientation::Right);
        assert_eq!((r.width(), r.height()), (6, 6));
        assert_eq!(r.pixel(5 - 1, 1), FILL);
        assert_eq!(r.hot(), (4, 1));
    }

    #[test]
    fn the_arrow_keeps_between_8_and_64_pixels_tall() {
        for (cell, h) in [(1, 8), (8, 8), (48, 48), (64, 64), (200, 64)] {
            let a = Arrow::new(cell, Orientation::Normal);
            assert_eq!(a.height(), h, "cell {cell}");
            assert!(a.width() <= POINTER_SIDE_MAX && a.width() < a.height());
            let len = usize::try_from(a.width() * a.height()).unwrap_or(0);
            assert_eq!(a.pixels().len(), len);
        }
    }

    #[test]
    fn the_arrow_turns_with_the_display() {
        let a = Arrow::new(16, Orientation::Normal);
        let (w, h) = (i64::from(a.width()), i64::from(a.height()));
        for (o, turned) in [
            (Orientation::UpsideDown, false),
            (Orientation::Right, true),
            (Orientation::Left, true),
        ] {
            let t = Arrow::new(16, o);
            if turned {
                assert_eq!((t.width(), t.height()), (a.height(), a.width()));
            } else {
                assert_eq!((t.width(), t.height()), (a.width(), a.height()));
            }
            for y in 0..h {
                for x in 0..w {
                    let (tx, ty) = match o {
                        Orientation::UpsideDown => (w - 1 - x, h - 1 - y),
                        Orientation::Right => (h - 1 - y, x),
                        Orientation::Left => (y, w - 1 - x),
                        Orientation::Normal => (x, y),
                    };
                    assert_eq!(t.pixel(tx, ty), a.pixel(x, y), "{o:?} {x},{y}");
                }
            }
            let (hx, hy) = (i64::from(a.hot().0), i64::from(a.hot().1));
            let want = match o {
                Orientation::UpsideDown => (w - 1 - hx, h - 1 - hy),
                Orientation::Right => (h - 1 - hy, hx),
                Orientation::Left => (hy, w - 1 - hx),
                Orientation::Normal => (hx, hy),
            };
            let got = t.hot();
            assert_eq!((i64::from(got.0), i64::from(got.1)), want, "{o:?}");
        }
    }

    // Every shape of vtium's set has a picture of its own, a cell tall
    // and kept between 14 and 64 pixels, with its hotspot on a pixel the
    // picture covers. The default shape is the arrow, which keeps between
    // 8 and 64 pixels.
    // covers: window.pointer-shape
    #[test]
    fn each_shape_has_its_own_picture_a_cell_tall() {
        for cell in [1, 9, 16, 31, 48, 200] {
            let mut seen: Vec<Arrow> = Vec::new();
            for s in PointerShape::ALL {
                let a = Arrow::upright_shape(s, cell);
                let arrow = Arrow::upright(cell);
                let tall = if s == PointerShape::Default { 8 } else { 14 };
                let want = cell.clamp(tall, POINTER_SIDE_MAX);
                assert_eq!(a.height(), want, "{} at {cell}", s.name());
                assert!(a.width() <= POINTER_SIDE_MAX, "{} at {cell}", s.name());
                assert!(a.covers(), "{} at {cell}", s.name());
                let (hx, hy) = a.hot();
                assert!(hx < a.width() && hy < a.height(), "{} at {cell}", s.name());
                let hot = a.pixel(i64::from(hx), i64::from(hy));
                assert_ne!(hot, 0, "{} at {cell}: the hotspot is clear", s.name());
                assert_eq!(
                    s == PointerShape::Default,
                    a == arrow,
                    "{} at {cell}",
                    s.name()
                );
                assert!(
                    !seen.contains(&a),
                    "{} at {cell} repeats a picture",
                    s.name()
                );
                seen.push(a);
            }
        }
    }

    // The I-beam and the crosshair at a 16-pixel cell: each stroke is a
    // pixel wide inside its outline, and the hotspot (`@`) is at the
    // centre of the shape.
    // covers: window.pointer-shape
    #[test]
    fn the_text_and_crosshair_pictures_at_16_pixels() {
        let marked = |a: &Arrow| {
            let mut p = picture(a);
            let (hx, hy) = a.hot();
            if let Some(row) = p.get_mut(usize::try_from(hy).unwrap_or(0)) {
                let x = usize::try_from(hx).unwrap_or(0);
                row.replace_range(x..=x, "@");
            }
            p
        };
        let text = [
            "ooooooo.", //
            "o#####o.", //
            "o#####o.", //
            "ooo#ooo.", //
            "..o#o...", //
            "..o#o...", //
            "..o#o...", //
            "..o#o...", //
            "..o@o...", //
            "..o#o...", //
            "..o#o...", //
            "..o#o...", //
            "ooo#ooo.", //
            "o#####o.", //
            "o#####o.", //
            "ooooooo.",
        ];
        assert_eq!(marked(&Arrow::upright_shape(PointerShape::Text, 16)), text);
        let cross = [
            "......ooo.......",
            "......o#o.......",
            "......o#o.......",
            "......o#o.......",
            "......o#o.......",
            "......o#o.......",
            "ooooooo#ooooooo.",
            "o#############o.",
            "o######@######o.",
            "ooooooo#ooooooo.",
            "......o#o.......",
            "......o#o.......",
            "......o#o.......",
            "......o#o.......",
            "......o#o.......",
            "......ooo.......",
        ];
        assert_eq!(
            marked(&Arrow::upright_shape(PointerShape::Crosshair, 16)),
            cross
        );
    }

    // A shape's picture turns with the display, as the arrow does.
    #[test]
    fn a_shapes_picture_turns_with_the_display() {
        for s in PointerShape::ALL {
            let up = Arrow::upright_shape(s, 20);
            for o in [
                Orientation::UpsideDown,
                Orientation::Right,
                Orientation::Left,
            ] {
                assert_eq!(Arrow::shape(s, 20, o), up.clone().turned(o), "{}", s.name());
            }
            assert_eq!(Arrow::shape(s, 20, Orientation::Normal), up);
        }
    }
}
