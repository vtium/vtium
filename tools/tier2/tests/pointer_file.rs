//! vtium's shipped pointer file against the compiled-in pointer pictures:
//! each symbol of `crates/vtium/install/pointers.svg`, drawn through the
//! font helper's own code, against the picture vtium draws before the
//! helper answers, at every height from 8 to 64.
//!
//! The pointer then looks the same before and after the helper answers,
//! within the tolerance that the two ways of drawing allow. The compiled-in
//! outline is a ring of whole pixels, one pixel wide and one more for each
//! 32 of height, and the compiled-in shape shrinks inside it by whole
//! pixels. The file's stroke and shape scale smoothly with the picture
//! instead. So no static file can match the compiled-in pictures pixel for
//! pixel at every height. The test holds the file to what it can match:
//! the same height and a width within one pixel, the hotspot within one
//! pixel, each fill pixel within one pixel of the other picture's fill
//! (but for [`FILL_STRAYS`] pixels at a sharp tip), and each outline pixel
//! within two pixels of the other picture's outline. [`KNOWN`] lists the
//! pictures whose outline differs by more, each of which the test checks
//! still differs, so that a change that mends one fails until the list
//! drops it.
//!
//! References:
//! - ADR-1309 D7: the second test, each symbol against the compiled-in picture at every height from 8 to 64.
//! - ADR-1309 D7, the note of 2026-11-15: the tolerance the maintainer accepted, the height exact, the width, the hotspot and the fill within one pixel with two stray fill pixels at a sharp tip, the outline within two pixels, and the pictures that differ more listed in the test.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]

use vtium_config::pointer_svg::draw;
use vtium_render::Arrow;
use vtium_term::PointerShape;

/// The pictures whose outline lies more than two pixels from the
/// compiled-in outline somewhere: at the arrow's tip at the smallest
/// heights, where the file's stroke is thinner than a pixel, and where a
/// stroke's round joins and the compiled-in square ring part.
const KNOWN: [(&str, u32); 8] = [
    ("default", 8),
    ("default", 9),
    ("default", 10),
    ("default", 11),
    ("default", 12),
    ("text", 62),
    ("pointer", 15),
    ("crosshair", 15),
];

/// The fill pixels of one picture that may lie more than a pixel from the
/// other picture's fill: a sharp tip whose centre the file's smooth scale
/// puts inside the shape a pixel early.
const FILL_STRAYS: usize = 2;

const FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/vtium/install/pointers.svg"
);

/// What a pixel is: clear, outline or fill.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Clear,
    Outline,
    Fill,
}

/// A picture as classes of pixels.
struct Classes {
    w: i64,
    h: i64,
    px: Vec<Class>,
}

impl Classes {
    fn of_argb(a: &Arrow) -> Classes {
        let px = a
            .pixels()
            .iter()
            .map(|&p| match p {
                0 => Class::Clear,
                0xffff_ffff => Class::Fill,
                _ => Class::Outline,
            })
            .collect();
        Classes {
            w: i64::from(a.width()),
            h: i64::from(a.height()),
            px,
        }
    }

    fn of_rgba(w: u32, h: u32, rgba: &[u8]) -> Classes {
        let px = rgba
            .chunks(4)
            .map(|p| match p {
                [_, _, _, 0] => Class::Clear,
                [255, 255, 255, 255] => Class::Fill,
                _ => Class::Outline,
            })
            .collect();
        Classes {
            w: i64::from(w),
            h: i64::from(h),
            px,
        }
    }

    fn at(&self, x: i64, y: i64) -> Class {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return Class::Clear;
        }
        self.px[(y * self.w + x) as usize]
    }

    /// Whether a pixel of `class` lies within `r` pixels of `x`, `y`.
    fn near(&self, x: i64, y: i64, class: Class, r: i64) -> bool {
        (-r..=r).any(|dy| (-r..=r).any(|dx| self.at(x + dx, y + dy) == class))
    }
}

/// The pixels of `class` in `a` with no pixel of that class within `r`
/// pixels in `b`.
fn strays(a: &Classes, b: &Classes, class: Class, r: i64) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for y in 0..a.h {
        for x in 0..a.w {
            if a.at(x, y) == class && !b.near(x, y, class, r) {
                out.push((x, y));
            }
        }
    }
    out
}

// covers: window.pointer-shape
#[test]
fn each_symbol_of_the_shipped_file_draws_as_the_compiled_in_picture() {
    let file = std::fs::read(FILE).unwrap();
    let mut failures = Vec::new();
    for shape in PointerShape::ALL {
        for cell in 8..=64 {
            let compiled = Arrow::upright_shape(shape, cell);
            // the compiled-in sprites keep at least 14 pixels tall, so the
            // file is drawn at the compiled-in picture's own height
            let p = draw(&file, shape.name(), compiled.height()).unwrap();
            let (c, s) = (
                Classes::of_argb(&compiled),
                Classes::of_rgba(p.width, p.height, &p.rgba),
            );
            let name = shape.name();
            let known = KNOWN.contains(&(name, cell));
            let mut outline_differs = false;
            if p.height != compiled.height() || p.width.abs_diff(compiled.width()) > 1 {
                failures.push(format!(
                    "{name} at {cell}: {}×{} where the compiled-in picture is {}×{}",
                    p.width,
                    p.height,
                    compiled.width(),
                    compiled.height()
                ));
                continue;
            }
            let (hx, hy) = compiled.hot();
            if p.hot.0.abs_diff(hx) > 1 || p.hot.1.abs_diff(hy) > 1 {
                failures.push(format!(
                    "{name} at {cell}: the hotspot is {:?} where the compiled-in one is {:?}",
                    p.hot,
                    (hx, hy)
                ));
            }
            for (class, r, allowed) in [(Class::Fill, 1, FILL_STRAYS), (Class::Outline, 2, 0)] {
                for (a, b, which) in [(&s, &c, "the file's"), (&c, &s, "the compiled-in")] {
                    let stray = strays(a, b, class, r);
                    if class == Class::Outline && known {
                        outline_differs |= !stray.is_empty();
                        continue;
                    }
                    if stray.len() > allowed {
                        failures.push(format!(
                            "{name} at {cell}: {which} {class:?} pixels {stray:?} lie more than {r} pixels from the other's"
                        ));
                    }
                }
            }
            if known && !outline_differs {
                failures.push(format!(
                    "{name} at {cell}: its outline now matches; drop it from KNOWN"
                ));
            }
        }
    }
    if !failures.is_empty() {
        panic!("{} differences:\n{}", failures.len(), failures.join("\n"));
    }
}
