//! Pointer pictures from a pointer file, and vtium's own pointer file.
//!
//! `pointer-theme` names an SVG file or a cursor theme's folder, whose
//! pictures the font helper draws and the console checks
//! ([`Arrow::from_rgba`]). [`PointerPictures`] holds them, each shape's
//! pictures at the sizes the helper answered, and each renderer picks the
//! one its cells need. A picture from an SVG file serves only the cell
//! height it was drawn for, so a zoom to a new height draws vtium's own
//! picture until the helper answers for that height. A cursor theme's
//! pictures come at the theme's fixed sizes, and a renderer takes the size
//! nearest `pointer-size`, or else nearest its cell's height, the smaller
//! of two equally near. A theme's picture is never scaled.
//!
//! [`svg`] writes vtium's own pictures as a pointer file, the one installed
//! as `/usr/share/vtium/pointers.svg`, from the same parts the compiled-in
//! pictures draw. Each shape is a symbol whose viewBox is the shape's grid
//! with a margin for the outline, and whose `data-hotspot` is the shape's
//! hotspot unit. Each part is drawn twice: a black outline stroke at 220 of
//! 255, then the white fill on top. The root asks for crisp edges, so the
//! helper fills each pixel whose centre lies inside a shape, as the
//! compiled-in pictures do.
//!
//! References:
//! - ADR-1309 D6: which size of a cursor theme draws, and no scaling.
//! - ADR-1309 D4: an SVG file's picture is drawn a cell tall.
//! - ADR-1309 D7: vtium's own shapes as a file, generated from the compiled-in parts, and the comment that opens it.

use std::fmt::Write as _;

use vtium_term::PointerShape;

use crate::arrow::{Arrow, PartOf, grid};

/// Where a table's pictures come from, which decides how a renderer picks
/// among a shape's sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureSizes {
    /// An SVG file: each picture serves the cell height it was drawn for.
    Svg,
    /// A cursor theme: each picture's size is its nominal size, and
    /// `pointer-size`, when set, names the size wanted.
    Theme { size: Option<u32> },
}

/// The pictures of a pointer file, upright, each shape's at the sizes the
/// font helper answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointerPictures {
    sizes: PictureSizes,
    /// Each shape's pictures, in [`PointerShape::ALL`]'s order, by size.
    shapes: [Vec<(u32, Arrow)>; 10],
}

/// A shape's place in [`PointerShape::ALL`].
fn place(shape: PointerShape) -> usize {
    PointerShape::ALL
        .iter()
        .position(|&s| s == shape)
        .unwrap_or(0)
}

impl PointerPictures {
    /// A table with no picture yet, whose pictures come as `sizes` says.
    pub fn new(sizes: PictureSizes) -> PointerPictures {
        PointerPictures {
            sizes,
            shapes: Default::default(),
        }
    }

    pub fn sizes(&self) -> PictureSizes {
        self.sizes
    }

    /// Keeps `arrow`, upright, as `shape`'s picture at `size`, in place of
    /// one the table held at that size.
    pub fn insert(&mut self, shape: PointerShape, size: u32, arrow: Arrow) {
        if let Some(pictures) = self.shapes.get_mut(place(shape)) {
            pictures.retain(|(s, _)| *s != size);
            pictures.push((size, arrow));
            pictures.sort_by_key(|(s, _)| *s);
        }
    }

    /// Whether the table holds any picture of `shape`.
    pub fn has(&self, shape: PointerShape) -> bool {
        self.shapes.get(place(shape)).is_some_and(|p| !p.is_empty())
    }

    /// The sizes the table holds of `shape`, smallest first.
    pub fn sizes_of(&self, shape: PointerShape) -> Vec<u32> {
        self.shapes
            .get(place(shape))
            .map(|p| p.iter().map(|(s, _)| *s).collect())
            .unwrap_or_default()
    }

    /// `shape`'s picture for cells `cell_height` pixels tall, upright:
    /// from an SVG file, the one drawn for that height; from a cursor
    /// theme, the size nearest `pointer-size`, or else nearest the cell's
    /// height, the smaller of two equally near. `None` when the table has
    /// none to give, and vtium draws its own picture.
    pub fn pick(&self, shape: PointerShape, cell_height: u32) -> Option<&Arrow> {
        let pictures = self.shapes.get(place(shape))?;
        match self.sizes {
            PictureSizes::Svg => pictures
                .iter()
                .find(|(s, _)| *s == cell_height)
                .map(|(_, a)| a),
            PictureSizes::Theme { size } => {
                let want = size.unwrap_or(cell_height);
                pictures
                    .iter()
                    .min_by_key(|(s, _)| (s.abs_diff(want), *s))
                    .map(|(_, a)| a)
            }
        }
    }

    /// Whether a renderer at cells `cell_height` tall needs the font helper
    /// to draw again: an SVG file's table that has some shape, but not at
    /// this height. A cursor theme's table never does, since a zoom picks
    /// again from the sizes kept.
    pub fn lacks(&self, cell_height: u32) -> bool {
        self.sizes == PictureSizes::Svg
            && self
                .shapes
                .iter()
                .any(|p| !p.is_empty() && !p.iter().any(|(s, _)| *s == cell_height))
    }
}

/// The margin around each symbol's grid, as a fraction of the grid's
/// height, that holds the outline. The compiled-in outline is one pixel
/// wide, and one more for each 32 pixels of height, while a stroke in the
/// file scales with the picture. A margin of 4/87 of the grid makes the
/// file's outline 4/95 of the picture's height, the width that stays
/// within two thirds of a pixel of the compiled-in outline at every height
/// from 8 to 64.
const MARGIN_PER_HEIGHT: f64 = 1.0 / 20.0;

/// The outline's alpha as a fraction: 220 of 255.
const OUTLINE_OPACITY: &str = "0.8627";

/// `v` with at most four decimals and no trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

/// One part as an SVG element.
fn element(part: &PartOf) -> String {
    match *part {
        PartOf::Poly(points) => {
            let list: Vec<String> = points.iter().map(|&(x, y)| format!("{x},{y}")).collect();
            format!("<polygon points=\"{}\"/>", list.join(" "))
        }
        PartOf::Rect(l, t, r, b) => format!(
            "<rect x=\"{l}\" y=\"{t}\" width=\"{}\" height=\"{}\"/>",
            r.saturating_sub(l),
            b.saturating_sub(t)
        ),
        PartOf::Disc(x, y, r) => format!("<circle cx=\"{x}\" cy=\"{y}\" r=\"{r}\"/>"),
        PartOf::Ring(x, y, outer, inner) => {
            let circle = |r: i64| {
                format!(
                    "M{} {y}a{r} {r} 0 1 0 {} 0a{r} {r} 0 1 0 {} 0Z",
                    x.saturating_sub(r),
                    r.saturating_mul(2),
                    r.saturating_mul(-2)
                )
            };
            format!("<path d=\"{}{}\"/>", circle(outer), circle(inner))
        }
    }
}

/// The comment that opens vtium's pointer file, wrapped at 78 columns for
/// a terminal editor.
const HEADER: &str = "<!--
  vtium's own pointer pictures: one symbol for each pointer shape that a
  program can ask for. Each symbol's id is the shape's name, its viewBox
  sets its proportions, and data-hotspot is the point the pointer names, in
  the viewBox's units.

  The format is described in the pointer file reference:
  https://vtium.github.io/vtium/reference/pointer-file/

  To draw your own pointer, copy this file, change its symbols in an SVG
  editor, and point pointer-theme in vtium.conf at the copy. vtium writes
  this file from the pictures built into it, so a change to the installed
  file is lost at the next upgrade.
-->
";

/// vtium's own pointer pictures as a pointer file: the text of
/// `crates/vtium/install/pointers.svg`.
pub fn svg() -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(HEADER);
    out.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\" shape-rendering=\"crispEdges\">\n");
    for shape in PointerShape::ALL {
        let ((w, h), (hx, hy), parts) = grid(shape);
        let (w, h) = (int_f64(w), int_f64(h));
        let m = h * MARGIN_PER_HEIGHT;
        let shapes: String = parts.iter().map(element).collect();
        let _ = write!(
            out,
            "  <symbol id=\"{id}\" viewBox=\"{x} {x} {vw} {vh}\" data-hotspot=\"{hx} {hy}\">\n    <g opacity=\"{OUTLINE_OPACITY}\" fill=\"none\" stroke=\"#000\" stroke-width=\"{sw}\" stroke-linejoin=\"round\">{shapes}</g>\n    <g fill=\"#fff\" fill-rule=\"evenodd\">{shapes}</g>\n  </symbol>\n",
            id = shape.name(),
            x = num(-m),
            vw = num(w + 2.0 * m),
            vh = num(h + 2.0 * m),
            sw = num(2.0 * m * 1.25),
        );
    }
    out.push_str("</svg>\n");
    out
}

/// A grid's units, which are small, as a float.
fn int_f64(v: i64) -> f64 {
    i32::try_from(v).map(f64::from).unwrap_or(0.0)
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use crate::target::Orientation;

    fn arrow(side: u32) -> Arrow {
        Arrow::from_rgba(side, side, (0, 0), &vec![255; (side * side * 4) as usize]).unwrap()
    }

    // A cursor theme's table picks the size nearest pointer-size, or else
    // nearest the cell's height, and the smaller of two equally near; a
    // zoom picks again from the same sizes.
    #[test]
    fn a_theme_picks_the_nearest_size_and_the_smaller_on_a_tie() {
        let mut t = PointerPictures::new(PictureSizes::Theme { size: None });
        for s in [24, 32, 48] {
            t.insert(PointerShape::Default, s, arrow(s.min(64)));
        }
        let picked =
            |t: &PointerPictures, cell| t.pick(PointerShape::Default, cell).map(Arrow::width);
        assert_eq!(picked(&t, 16), Some(24));
        assert_eq!(picked(&t, 28), Some(24));
        assert_eq!(picked(&t, 29), Some(32));
        assert_eq!(picked(&t, 40), Some(32));
        assert_eq!(picked(&t, 41), Some(48));
        assert_eq!(picked(&t, 200), Some(48));
        assert_eq!(t.pick(PointerShape::Text, 16), None);
        assert!(!t.lacks(17));
        let sized = PointerPictures {
            sizes: PictureSizes::Theme { size: Some(46) },
            ..t
        };
        assert_eq!(picked(&sized, 16), Some(48));
    }

    // An SVG file's table gives a picture only at the height it was drawn
    // for, and says when a renderer's height needs a new drawing.
    #[test]
    fn an_svg_table_serves_only_the_height_drawn() {
        let mut t = PointerPictures::new(PictureSizes::Svg);
        t.insert(PointerShape::Text, 16, arrow(16));
        assert_eq!(t.pick(PointerShape::Text, 16).map(Arrow::width), Some(16));
        assert_eq!(t.pick(PointerShape::Text, 20), None);
        assert!(!t.lacks(16));
        assert!(t.lacks(20));
        t.insert(PointerShape::Text, 20, arrow(20));
        assert!(!t.lacks(20));
        assert_eq!(t.sizes_of(PointerShape::Text), [16, 20]);
    }

    // The console refuses each picture the helper may send wrong: a side
    // past 64 or of 0, rows of the wrong length, a hotspot outside, and a
    // color above its alpha.
    #[test]
    fn a_picture_from_the_helper_is_checked_before_it_is_trusted() {
        let ok = vec![0u8; 4 * 4 * 4];
        assert!(Arrow::from_rgba(4, 4, (3, 3), &ok).is_ok());
        assert!(Arrow::from_rgba(65, 1, (0, 0), &vec![0; 65 * 4]).is_err());
        assert!(Arrow::from_rgba(0, 4, (0, 0), &[]).is_err());
        assert!(Arrow::from_rgba(4, 4, (0, 0), &ok[..60]).is_err());
        assert!(Arrow::from_rgba(4, 4, (4, 0), &ok).is_err());
        assert!(Arrow::from_rgba(4, 4, (0, 4), &ok).is_err());
        let mut over = ok.clone();
        over[0] = 1;
        assert!(Arrow::from_rgba(4, 4, (0, 0), &over).is_err());
        // premultiplied RGBA becomes the arrow's ARGB, and turns as the
        // compiled-in pictures do
        let mut one = vec![0u8; 2 * 4];
        one[..4].copy_from_slice(&[0x10, 0x20, 0x30, 0x80]);
        let a = Arrow::from_rgba(2, 1, (1, 0), &one).unwrap();
        assert_eq!(a.pixels(), [0x8010_2030, 0]);
        let r = a.turned(Orientation::Right);
        assert_eq!((r.width(), r.height(), r.hot()), (1, 2, (0, 1)));
    }

    // The committed pointers.svg is the generator's output: the shipped
    // file and the compiled-in pictures never drift apart.
    // `VTIUM_RENDER_BLESS=1` rewrites it.
    #[test]
    fn the_shipped_pointer_file_matches_the_generator() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../vtium/install/pointers.svg");
        let want = svg();
        let have = std::fs::read_to_string(path).unwrap_or_default();
        if have == want {
            return;
        }
        if std::env::var_os("VTIUM_RENDER_BLESS").is_some() {
            std::fs::write(path, want).expect("rewrite pointers.svg");
            return;
        }
        panic!(
            "crates/vtium/install/pointers.svg differs from the compiled-in pointer pictures; rerun with VTIUM_RENDER_BLESS=1 to rewrite it. Generated:\n{want}"
        );
    }
}
