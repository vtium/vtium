//! A character no font holds draws the font's missing glyph: U+25A1, else
//! the face's last resort. When a font set arrives that brings its own
//! U+25A1, the cells that drew the missing glyph draw it again from the set
//! (vtium/vtium#314), whether a cell shows U+25A1 itself or not.

mod common;

use std::cell::Cell as Shared;
use std::rc::Rc;

use common::{Screen, rgb, style};
use vtium_font::{Cell, Face, Font, Format, Glyph, SpritePolicy, Style};
use vtium_render::{Canvas, Config, DrawState, Renderer};
use vtium_term::Age;

const CELL: Cell = Cell {
    width: 8,
    height: 16,
};

/// A face whose set has not arrived until a test says so. Before, it holds
/// nothing, and its last resort lights the cell's top half. Once the set
/// has arrived, it holds U+25A1, which lights the whole cell.
struct Arriving {
    arrived: Rc<Shared<bool>>,
}

fn mask(rows: u32) -> Option<Glyph> {
    let lit = usize::try_from(CELL.width * rows).ok()?;
    let all = usize::try_from(CELL.width * CELL.height).ok()?;
    let mut px = vec![0xff; lit];
    px.resize(all, 0);
    Glyph::from_pixels(CELL.width, CELL.height, Format::Mask, px)
}

impl Face for Arriving {
    fn cell(&self) -> Cell {
        CELL
    }
    fn size_step(&self) -> u32 {
        CELL.height
    }
    fn has_glyph(&self, c: char) -> bool {
        c == '\u{25a1}' && self.arrived.get()
    }
    fn sprite_policy(&self) -> SpritePolicy {
        SpritePolicy::SPRITES
    }
    fn draw(&self, cluster: &[char], _: Style) -> Option<Glyph> {
        (cluster == ['\u{25a1}'] && self.arrived.get())
            .then(|| mask(CELL.height))
            .flatten()
    }
    fn last_resort(&self, _: Style) -> Option<Glyph> {
        mask(CELL.height / 2)
    }
}

fn font(arrived: &Rc<Shared<bool>>) -> Font<Arriving> {
    Font::new(Arriving {
        arrived: Rc::clone(arrived),
    })
}

fn state() -> DrawState {
    DrawState {
        background: rgb("101010"),
        ..DrawState::default()
    }
}

/// Draws `X` before the set arrives, tells the renderer U+25A1 arrived,
/// as `deliver` does, and draws the next frame: it must match a renderer
/// made after the arrival, and differ from the frame before.
fn arrival_redraws_the_missing_glyph(deliver: impl Fn(&mut Renderer<Arriving>, &[char])) {
    let (w, h) = (CELL.width * 4, CELL.height);
    let arrived = Rc::new(Shared::new(false));
    let mut r = Renderer::new(font(&arrived), w, h, Config::default());
    let (fg, bg) = (rgb("c0c0c0"), rgb("101010"));
    let mut s = Screen::new(4, 1, fg, bg);
    s.text(0, 0, "X", style(fg, bg));
    let mut canvas = Canvas::new(w, h, 0);
    r.draw(&s.frame(Age::NEVER), &mut canvas, 0, &state());
    let before = canvas.clone();
    let since = s.damage.checkpoint();
    arrived.set(true);
    deliver(&mut r, &['\u{25a1}']);
    r.draw(&s.frame(since), &mut canvas, 1, &state());
    let mut fresh = Canvas::new(w, h, 0);
    Renderer::new(font(&arrived), w, h, Config::default()).draw(
        &s.frame(Age::NEVER),
        &mut fresh,
        0,
        &state(),
    );
    assert!(before != fresh, "the set's U+25A1 moves no pixel");
    assert!(
        canvas == fresh,
        "X still shows the missing glyph drawn before the set arrived"
    );
}

#[test]
fn a_set_that_brings_u25a1_redraws_the_cells_that_drew_the_missing_glyph() {
    arrival_redraws_the_missing_glyph(|r, chars| r.glyphs_changed(chars));
}

#[test]
fn an_arrival_no_cell_shows_still_redraws_the_missing_glyph_on_the_next_frame() {
    arrival_redraws_the_missing_glyph(|r, chars| r.forget_glyphs(chars));
}

// the GPU path's glyphs drop the missing glyph's drawing too, on the frame
// after the arrival
#[cfg(feature = "gpu")]
#[test]
fn on_the_gpu_path_an_arrival_fills_the_missing_glyph_again() {
    use vtium_render::gpu::{Call, Config as GpuConfig, Gl, Pipeline, Size, StandIn};
    let display = Size::new(CELL.width * 4, CELL.height);
    let mut gl = StandIn::new();
    let mut pipeline = Pipeline::new(&mut gl, display, GpuConfig::default()).unwrap();
    let buffer = gl.import_buffer(&display).unwrap();
    let arrived = Rc::new(Shared::new(false));
    let mut r = Renderer::new(
        font(&arrived),
        display.width,
        display.height,
        Config::default(),
    );
    let (fg, bg) = (rgb("c0c0c0"), rgb("101010"));
    let mut s = Screen::new(4, 1, fg, bg);
    s.text(0, 0, "X", style(fg, bg));
    let mut frame = |r: &mut Renderer<Arriving>, gl: &mut StandIn, s: &Screen, since, age| {
        gl.clear_calls();
        r.draw_gpu(
            gl,
            &mut pipeline,
            &s.frame(since),
            &buffer,
            age,
            &state(),
            &[],
        )
        .unwrap();
        gl.calls()
            .iter()
            .filter(|c| matches!(c, Call::FillTexture(_)))
            .count()
    };
    // X's first drawing, and the sprites' strip
    assert_eq!(frame(&mut r, &mut gl, &s, Age::NEVER, 0), 2);
    let since = s.damage.checkpoint();
    arrived.set(true);
    r.forget_glyphs(&['\u{25a1}']);
    assert_eq!(
        frame(&mut r, &mut gl, &s, since, 1),
        1,
        "X drawn from the set"
    );
}
