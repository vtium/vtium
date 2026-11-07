//! The GPU path at a sideways bound of two cells, and at a bound of two
//! rows up and down (ADR-1306 D9, D12): the cell shader takes the bound as
//! compile-time constants, the boxes hold a glyph's ink two cells and two
//! rows past its own, and every frame the stand-in composes through its
//! model of the cell shader equals the CPU path's whole draw.

use vtium_font::{Style, Width as FontWidth};
use vtium_term::{Age, Rgb};

use super::{CH, CW, Plain, REACHING, Screen, TALLER, WIDER, renderer, stored, whole};
use crate::gpu::{Call, Config as GpuConfig, DrawTarget, Gl, Pipeline, Size, StandIn};
use crate::reach::Bound;
use crate::renderer::{DrawState, Renderer};

/// A display of `cols` by `rows` cells on the stand-in GL, whose glyphs
/// hold the reaching glyphs as the CPU path's cache holds them.
struct Gpu {
    gl: StandIn,
    pipeline: Pipeline,
    buffer: DrawTarget,
    r: Renderer<Plain>,
    since: Age,
}

impl Gpu {
    fn new(cols: usize, rows: usize, bound: Bound) -> Gpu {
        let size = Size::new(cols as u32 * CW, rows as u32 * CH);
        let mut gl = StandIn::new();
        let mut pipeline = Pipeline::new(&mut gl, size, GpuConfig::default()).unwrap();
        let buffer = gl.import_buffer(&size).unwrap();
        let mut r = renderer(cols, rows, bound);
        // a first frame makes the pipeline's glyphs for this renderer,
        // which then take the reaching glyphs, stored as the font would
        // store them
        let blank = Screen::new(cols, rows);
        let st = DrawState::default();
        r.draw_gpu(
            &mut gl,
            &mut pipeline,
            &blank.frame(Age::NEVER),
            &buffer,
            0,
            &st,
            &[],
        )
        .unwrap();
        let glyphs = pipeline.glyphs().unwrap();
        for c in REACHING {
            let text = [c];
            let place = glyphs
                .put(
                    &mut gl,
                    (&text, FontWidth::Narrow, Style::default()),
                    stored(c),
                )
                .unwrap();
            assert!(place.is_some_and(|p| p.boxed.is_some()), "{c} takes a box");
        }
        Gpu {
            gl,
            pipeline,
            buffer,
            r,
            since: Age::NEVER,
        }
    }

    /// Draws `s` into the buffer one frame old, the upright frame keeping
    /// the last.
    fn frame(&mut self, s: &mut Screen) {
        self.gl.clear_calls();
        let frame = s.frame(self.since);
        self.r
            .draw_gpu(
                &mut self.gl,
                &mut self.pipeline,
                &frame,
                &self.buffer,
                1,
                &DrawState::default(),
                &[],
            )
            .unwrap();
        self.since = s.damage.checkpoint();
    }

    /// Whether the upright frame holds `s` as the CPU path draws it whole
    /// at `bound`; the first pixel that differs when not.
    fn differs(&self, s: &Screen, bound: Bound) -> Option<(u32, u32, u32, u32)> {
        let cpu = whole(s, bound);
        let name = *self.gl.frame_names().last().unwrap();
        let (size, gpu) = self.gl.frame_pixels(name).unwrap();
        (0..size.height)
            .flat_map(|y| (0..size.width).map(move |x| (x, y)))
            .find_map(|(x, y)| {
                let (c, p) = (cpu.pixel(x, y)?, gpu[(y * size.width + x) as usize]);
                (c != p).then_some((x, y, c, p))
            })
    }
}

// D12: the bound reaches the shader as compile-time constants, and the
// boxes are as large as a glyph's ink may reach at the bound.
#[test]
fn the_cell_shader_and_the_boxes_follow_the_bound() {
    for (bound, side, rows, slot) in [
        (Bound::DRAWN, "1", "1", Size::new(4 * CW, 3 * CH)),
        (WIDER, "2", "1", Size::new(6 * CW, 3 * CH)),
        (TALLER, "2", "2", Size::new(6 * CW, 5 * CH)),
    ] {
        let mut g = Gpu::new(4, 3, bound);
        let compiled: Vec<&String> =
            g.gl.calls()
                .iter()
                .filter_map(|c| match c {
                    Call::Compile(text) => Some(text),
                    _ => None,
                })
                .collect();
        let cell = compiled.iter().rfind(|t| t.contains("box_slot")).unwrap();
        assert!(
            cell.contains(&format!("#define REACH_SIDE {side}\n")),
            "{bound:?}"
        );
        assert!(
            cell.contains(&format!("#define REACH_ROWS {rows}\n")),
            "{bound:?}"
        );
        let (_, [w, h, _]) = g.pipeline.glyphs().unwrap().boxes();
        assert_eq!(Size::new(w, h), slot, "{bound:?}");
    }
}

// D4's step 5 at a bound of two: the ink of the cells beside a cell is
// drawn farthest first and, at one distance, the cell before it first.
#[test]
fn ink_from_beside_is_drawn_farthest_first_on_the_gpu_path() {
    let mut s = Screen::new(7, 1);
    let mut g = Gpu::new(7, 1, WIDER);
    s.put(1, 0, '>', Rgb::new(250, 0, 0));
    s.put(2, 0, '>', Rgb::new(0, 250, 0));
    s.put(4, 0, '<', Rgb::new(0, 0, 250));
    s.put(5, 0, '<', Rgb::new(250, 250, 0));
    g.frame(&mut s);
    assert_eq!(g.differs(&s, WIDER), None);
    // where only the two farthest meet
    s.put(2, 0, ' ', Rgb::new(0, 250, 0));
    s.put(4, 0, ' ', Rgb::new(0, 0, 250));
    g.frame(&mut s);
    assert_eq!(g.differs(&s, WIDER), None);
}

// A script typing and erasing the reaching glyphs at each wider bound,
// every frame of the GPU path checked against the CPU path's whole draw.
#[test]
fn every_gpu_frame_matches_a_whole_draw_at_a_bound_of_two() {
    for bound in [WIDER, TALLER] {
        let (cols, rows) = (10usize, 6usize);
        let mut s = Screen::new(cols, rows);
        let mut g = Gpu::new(cols, rows, bound);
        let mut seed = 0x9e37_79b9u32;
        let mut rand = move |n: u32| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed % n
        };
        for step in 0..60 {
            for _ in 0..rand(4) {
                let c = ['R', 'L', 'U', 'T', '>', '<', 'x', ' '][rand(8) as usize];
                let fg = Rgb::new(rand(256) as u8, 100, 200);
                s.put(
                    rand(cols as u32) as usize,
                    rand(rows as u32) as usize,
                    c,
                    fg,
                );
            }
            g.frame(&mut s);
            assert_eq!(g.differs(&s, bound), None, "{bound:?}, step {step}");
        }
    }
}
