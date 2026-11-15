//! The pointer's shapes that a program asks for with OSC 22. The renderer
//! draws each shape of vtium's set as its own picture, blended over the
//! cells as the arrow is, with the picture's hotspot on the pointer's
//! pixel. A change of shape draws again the cells that the old picture and
//! the new one cover, and nothing else.

mod common;

use common::{Screen, arrow_over, rgb, style, unifont};
use vtium_render::{Arrow, Canvas, Config, DrawState, Orientation, PointerShape, Renderer};
use vtium_term::{Age, PointerShape as Shape};

/// A screen of 6×2 cells of text, 48×32 pixels.
fn screen() -> Screen {
    let st = style(rgb("e0e0e0"), rgb("203040"));
    let mut s = Screen::new(6, 2, rgb("c0c0c0"), rgb("101010"));
    s.text(0, 0, "abcdef", st);
    s.text(0, 1, "ghijkl", st);
    s
}

fn state(s: &Screen, pointer: Option<(u32, u32)>) -> DrawState {
    DrawState {
        background: s.palette.bg(),
        pointer,
        ..DrawState::default()
    }
}

/// `s` drawn whole by a new renderer that shows `shape`, with the pointer
/// at `tip`.
fn whole(s: &Screen, shape: Shape, tip: Option<(u32, u32)>, config: Config) -> Canvas {
    let mut r = Renderer::new(unifont(), 48, 32, config);
    r.set_pointer_shape(shape);
    let mut c = Canvas::new(48, 32, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(s, tip));
    c
}

/// What `image` blended over `under` with its hotspot at `tip` looks
/// like, pixel by pixel.
fn over(under: &Canvas, image: &Arrow, tip: (u32, u32)) -> Vec<u32> {
    let mut want = under.pixels().to_vec();
    let (hx, hy) = image.hot();
    for y in 0..32_u32 {
        for x in 0..48_u32 {
            let ix = i64::from(x) - i64::from(tip.0) + i64::from(hx);
            let iy = i64::from(y) - i64::from(tip.1) + i64::from(hy);
            let inside = (0..i64::from(image.width())).contains(&ix)
                && (0..i64::from(image.height())).contains(&iy);
            if !inside {
                continue;
            }
            let src = image.pixels()[(iy as u32 * image.width() + ix as u32) as usize];
            if src != 0 {
                want[(y * 48 + x) as usize] = arrow_over(under.pixel(x, y).unwrap(), src);
            }
        }
    }
    want
}

// Each shape draws as its own picture, its hotspot on the pointer's pixel,
// blended over the cells as the arrow is.
// covers: window.pointer-shape
#[test]
fn each_shape_draws_its_picture_at_the_pointer() {
    let s = screen();
    let without = whole(&s, Shape::Default, None, Config::default());
    let tip = (21, 14);
    for shape in Shape::ALL {
        let image = Arrow::upright_shape(shape, 16);
        let want = over(&without, &image, tip);
        let got = whole(&s, shape, Some(tip), Config::default());
        assert!(got.pixels() == want, "{}", shape.name());
        assert!(got != without, "{}: the picture shows", shape.name());
    }
}

// A change of shape between two frames draws the new picture, and the
// buffer it draws into at age 1 then holds what a whole draw holds.
// covers: window.pointer-shape
#[test]
fn a_new_shape_draws_at_the_next_frame() {
    let s = screen();
    let mut s = s;
    let tip = (21, 14);
    let mut r = Renderer::new(unifont(), 48, 32, Config::default());
    let mut c = Canvas::new(48, 32, 0);
    r.draw(&s.frame(Age::NEVER), &mut c, 0, &state(&s, Some(tip)));
    assert!(c == whole(&s, Shape::Default, Some(tip), Config::default()));
    for shape in [
        Shape::Pointer,
        Shape::Crosshair,
        Shape::NotAllowed,
        Shape::Default,
        Shape::Wait,
    ] {
        r.set_pointer_shape(shape);
        assert_eq!(r.upright_arrow(), &Arrow::upright_shape(shape, 16));
        assert_eq!(r.arrow(), &Arrow::shape(shape, 16, Orientation::Normal));
        // nothing on the screen changed since the last frame, so only the
        // cells under the pointer draw again
        let since = s.damage.checkpoint();
        r.draw(&s.frame(since), &mut c, 1, &state(&s, Some(tip)));
        let want = whole(&s, shape, Some(tip), Config::default());
        assert!(c == want, "{} at age 1", shape.name());
    }
}

// `pointer-char` names the default shape's glyph. A shape a program asks
// for draws its own picture, with its hotspot on the pointer's pixel
// rather than on the corner of the cell.
// covers: window.pointer-shape
#[test]
fn a_shape_draws_its_own_picture_under_pointer_char() {
    let s = screen();
    let config = Config {
        pointer: PointerShape::Glyph('+'),
        ..Config::default()
    };
    let without = whole(&s, Shape::Default, None, config);
    let tip = (21, 14);
    let image = Arrow::upright_shape(Shape::Crosshair, 16);
    let got = whole(&s, Shape::Crosshair, Some(tip), config);
    assert!(got.pixels() == over(&without, &image, tip));
    let mut r = Renderer::new(unifont(), 48, 32, config);
    r.set_pointer_shape(Shape::Crosshair);
    r.draw(
        &s.frame(Age::NEVER),
        &mut Canvas::new(48, 32, 0),
        0,
        &state(&s, None),
    );
    assert_eq!(r.pointer_at(tip), Some((21, 14)));
    // the default shape keeps the glyph, on the cell's corner
    r.set_pointer_shape(Shape::Default);
    assert_eq!(r.pointer_at(tip), Some((16, 0)));
    assert_ne!(r.arrow(), &Arrow::new(16, Orientation::Normal));
}
