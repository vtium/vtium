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

/// The GPU path on the stand-in GL, which records each texture's pixels.
#[cfg(feature = "gpu")]
mod gpu {
    use super::*;
    use vtium_render::gpu::{Call, Config as GpuConfig, Gl, Pipeline, Size, StandIn};

    /// The arrow program's inputs, which find its draw among the frame's.
    const ARROW_INPUTS: [&str; 4] = ["target_size", "arrow", "under", "under_at"];

    /// The size and pixels of the texture the last frame's arrow draw read.
    fn arrow_texture(gl: &StandIn) -> (Size, Vec<u8>) {
        let i = gl
            .calls()
            .iter()
            .filter(|c| matches!(c, Call::Draw { .. }))
            .position(|c| matches!(c, Call::Draw { inputs, .. } if inputs == &ARROW_INPUTS))
            .expect("the frame draws the pointer");
        let name = gl.seen()[i]
            .textures
            .iter()
            .find(|(input, _)| *input == "arrow")
            .map(|(_, name)| *name)
            .expect("the arrow's texture");
        let (_, size, bytes) = gl.texture_pixels(name).expect("its pixels");
        (size, bytes.to_vec())
    }

    /// Draws `s` with the pointer at `tip` in `shape` through a renderer
    /// that starts with that shape, and gives its arrow texture.
    fn fresh(s: &Screen, shape: Shape, tip: (u32, u32)) -> (Size, Vec<u8>) {
        let mut gl = StandIn::with_limits(StandIn::LIMITS);
        let size = Size::new(48, 32);
        let mut pipeline = Pipeline::new(&mut gl, size, GpuConfig::default()).unwrap();
        let buffer = gl.import_buffer(&size).unwrap();
        let mut r = Renderer::new(unifont(), 48, 32, Config::default());
        r.set_pointer_shape(shape);
        gl.clear_calls();
        let st = state(s, Some(tip));
        r.draw_gpu(
            &mut gl,
            &mut pipeline,
            &s.frame(Age::NEVER),
            &buffer,
            0,
            &st,
            &[],
        )
        .unwrap();
        arrow_texture(&gl)
    }

    // The GPU path draws the pointer from the shape's picture: after a
    // change of shape, the next frame's arrow draw reads a texture of the
    // new picture, the one a renderer that started with that shape draws.
    // covers: window.pointer-shape
    #[test]
    fn the_gpu_path_draws_the_new_shape() {
        let mut s = screen();
        let tip = (21, 14);
        let mut gl = StandIn::with_limits(StandIn::LIMITS);
        let size = Size::new(48, 32);
        let mut pipeline = Pipeline::new(&mut gl, size, GpuConfig::default()).unwrap();
        let buffer = gl.import_buffer(&size).unwrap();
        let mut r = Renderer::new(unifont(), 48, 32, Config::default());
        let st = state(&s, Some(tip));
        r.draw_gpu(
            &mut gl,
            &mut pipeline,
            &s.frame(Age::NEVER),
            &buffer,
            0,
            &st,
            &[],
        )
        .unwrap();
        let arrow = arrow_texture(&gl);
        assert_eq!(arrow, fresh(&s, Shape::Default, tip));
        let since = s.damage.checkpoint();
        for shape in [Shape::Crosshair, Shape::Pointer] {
            r.set_pointer_shape(shape);
            gl.clear_calls();
            r.draw_gpu(
                &mut gl,
                &mut pipeline,
                &s.frame(since),
                &buffer,
                1,
                &st,
                &[],
            )
            .unwrap();
            let image = Arrow::upright_shape(shape, 16);
            let got = arrow_texture(&gl);
            assert_eq!(
                got.0,
                Size::new(image.width(), image.height()),
                "{}",
                shape.name()
            );
            assert_ne!(got, arrow, "{}", shape.name());
            assert_eq!(got, fresh(&s, shape, tip), "{}", shape.name());
        }
    }
}

// A pointer file's picture serves each shape it holds in place of vtium's
// own. `pointer-char` keeps the default shape where the font draws the
// character, and a shape the file lacks keeps vtium's own picture; going
// back to no file draws vtium's own pictures again.
// covers: window.pointer-shape
#[test]
fn a_pointer_files_picture_comes_after_pointer_char_and_before_vtiums_own() {
    use std::sync::Arc;
    use vtium_render::{PictureSizes, PointerPictures};
    let dot = Arrow::from_rgba(3, 3, (1, 1), &[255; 3 * 3 * 4]).unwrap();
    let mut pictures = PointerPictures::new(PictureSizes::Theme { size: None });
    pictures.insert(Shape::Default, 24, dot.clone());
    pictures.insert(Shape::Text, 24, dot.clone());
    let pictures = Arc::new(pictures);
    let mut r = Renderer::new(unifont(), 48, 32, Config::default());
    r.set_pointer_pictures(Some(Arc::clone(&pictures)));
    assert_eq!(r.upright_arrow(), &dot);
    r.set_pointer_shape(Shape::Text);
    assert_eq!(r.upright_arrow(), &dot);
    r.set_pointer_shape(Shape::Wait);
    assert_eq!(r.upright_arrow(), &Arrow::upright_shape(Shape::Wait, 16));
    r.set_pointer_pictures(None);
    r.set_pointer_shape(Shape::Text);
    assert_eq!(r.upright_arrow(), &Arrow::upright_shape(Shape::Text, 16));
    // pointer-char keeps the default shape; the file still draws the rest
    let glyph = Config {
        pointer: PointerShape::Glyph('x'),
        ..Config::default()
    };
    let mut g = Renderer::new(unifont(), 48, 32, glyph);
    g.set_pointer_pictures(Some(Arc::clone(&pictures)));
    assert_ne!(g.upright_arrow(), &dot);
    assert_ne!(g.upright_arrow(), &Arrow::upright(16));
    g.set_pointer_shape(Shape::Text);
    assert_eq!(g.upright_arrow(), &dot);
    // a character the font cannot draw gives the default shape the file's
    let missing = Config {
        pointer: PointerShape::Glyph('\u{10fffd}'),
        ..Config::default()
    };
    let mut m = Renderer::new(unifont(), 48, 32, missing);
    m.set_pointer_pictures(Some(pictures));
    assert_eq!(m.upright_arrow(), &dot);
}
