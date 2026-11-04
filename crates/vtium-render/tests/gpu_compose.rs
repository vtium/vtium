//! The GPU path's composition on the stand-in GL (ADR-1303 D2, D3, D9;
//! ADR-1300 D3, D6): the calls and quads each frame makes for its cells,
//! glyphs, border, arrow and notices, the upright frame's tiles past the
//! largest texture, the glyph pools' budgets, and failures. The stand-in
//! draws no pixels; tier 2 judges the pixels on a real GL.
#![cfg(feature = "gpu")]

mod common;

use common::{Screen, rgb, style, unifont};
use vtium_font::{Cell, Face, Font, Style, Unifont};
use vtium_render::gpu::{
    Batch, Blend, Call, Config as GpuConfig, DrawTarget, Error, ErrorKind, Format, Gl, Limits, Op,
    Pipeline, Quad, Size, StandIn, Work,
};
use vtium_render::{
    Budgets, Colors, Config, Depth, DrawState, Notice, Orientation, Placed, Rect, Renderer, Stats,
    Surface,
};
use vtium_term::{Age, Attrs, UnderlineStyle};

const FG: &str = "c0c0c0";
const BG: &str = "101010";
/// 10 by 3 cells of 8x16, the grid at 2,1.
const DISPLAY: Size = Size::new(84, 51);

fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn cell(col: u32, row: u32) -> Rect {
    rect(2 + col * 8, 1 + row * 16, 8, 16)
}

/// A display on the stand-in GL: its pipeline, one buffer and a renderer.
struct Gpu<F: Face> {
    gl: StandIn,
    pipeline: Pipeline,
    buffer: DrawTarget,
    renderer: Renderer<F>,
    since: Age,
    state: DrawState,
}

impl Gpu<Unifont> {
    fn new(limits: Limits, config: Config) -> Gpu<Unifont> {
        Gpu::with_font(unifont(), limits, config)
    }
}

impl<F: Face> Gpu<F> {
    fn with_font(font: Font<F>, limits: Limits, config: Config) -> Gpu<F> {
        let mut gl = StandIn::with_limits(limits);
        let pipeline = Pipeline::new(&mut gl, DISPLAY, GpuConfig::default()).unwrap();
        let buffer = gl.import_buffer(&DISPLAY).unwrap();
        gl.clear_calls();
        Gpu {
            gl,
            pipeline,
            buffer,
            renderer: Renderer::new(font, DISPLAY.width, DISPLAY.height, config),
            since: Age::NEVER,
            state: DrawState {
                cursor_on: true,
                background: rgb(BG),
                ..DrawState::default()
            },
        }
    }

    /// Draws `s` into the buffer at `age`, with `overlays` over it, and
    /// gives the damage and the figures, the calls since the last frame
    /// left in the stand-in.
    fn frame(
        &mut self,
        s: &mut Screen,
        age: u32,
        overlays: &[Placed],
    ) -> Result<(Vec<Rect>, Stats), Error> {
        self.gl.clear_calls();
        let drawn = self.renderer.draw_gpu(
            &mut self.gl,
            &mut self.pipeline,
            &s.frame(self.since),
            &self.buffer,
            age,
            &self.state,
            overlays,
        )?;
        let out = (drawn.damage.to_vec(), drawn.stats);
        self.since = s.damage.checkpoint();
        Ok(out)
    }

    fn calls(&self) -> &[Call] {
        self.gl.calls()
    }

    /// The quads of the frame's draws through `program`, which takes `inputs`.
    fn quads(&self, inputs: &[&str]) -> Vec<Quad> {
        self.calls()
            .iter()
            .filter_map(|c| match c {
                Call::Draw {
                    quads, inputs: i, ..
                } if i == inputs => Some(quads.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn cells(&self) -> Vec<Quad> {
        self.quads(&["target_size", "masks", "colors", "sprites", "cell"])
    }

    fn fills(&self) -> usize {
        self.calls()
            .iter()
            .filter(|c| matches!(c, Call::FillTexture(_)))
            .count()
    }
}

fn screen() -> Screen {
    Screen::new(10, 3, rgb(FG), rgb(BG))
}

fn plain() -> vtium_term::Style {
    style(rgb(FG), rgb(BG))
}

/// What a cell quad's `params` hold: the foreground and the glyph's kind,
/// the background, the underline's and the strike's colors and sprites.
fn params(fg: &str, kind: u32, bg: &str, ul: u32, st: u32) -> [u32; 4] {
    let c = |h: &str| u32::from_str_radix(h, 16).unwrap();
    [c(fg) | kind << 24, c(bg), ul, st]
}

fn dst(r: Rect) -> [i32; 4] {
    [r.x as i32, r.y as i32, r.width as i32, r.height as i32]
}

#[test]
fn a_frame_draws_each_cell_as_one_quad_through_the_cell_shader() {
    let mut g = Gpu::new(StandIn::LIMITS, Config::default());
    let mut s = screen();
    s.text(0, 0, "AB", plain());
    let (damage, stats) = g.frame(&mut s, 0, &[]).unwrap();
    assert_eq!(damage, [DISPLAY.rect()]);
    assert_eq!((stats.cells, stats.misses, stats.uncached), (30, 2, 0));
    // the sprites, a cell of each kind side by side, and the narrow mask
    // pool's first texture, 64 slots of the cell
    let kinds: Vec<&Call> = g
        .calls()
        .iter()
        .filter(|c| !matches!(c, Call::Draw { .. }))
        .collect();
    assert_eq!(
        kinds,
        [
            &Call::CreateTexture(Format::R8, Size::new(48, 16)),
            &Call::FillTexture(rect(0, 0, 48, 16)),
            &Call::CreateTexture(Format::R8, Size::new(512, 16)),
            &Call::FillTexture(rect(0, 0, 8, 16)),
            &Call::FillTexture(rect(8, 0, 8, 16)),
            &Call::Check(Batch::Uploads),
            &Call::Clear(DISPLAY.rect(), 0x10_1010),
            &Call::Check(Batch::Draws),
            &Call::Check(Batch::Frame),
            &Call::Submit,
            &Call::Wait,
        ]
    );
    let quads = g.cells();
    assert_eq!(quads.len(), 30);
    // A samples its slot from the top left; a blank cell samples nothing
    let a = quads.iter().find(|q| q.dst == dst(cell(0, 0))).unwrap();
    assert_eq!(a.src[..2], [0, 0]);
    assert_eq!(a.params, params(FG, 1, BG, 0, 0));
    let b = quads.iter().find(|q| q.dst == dst(cell(1, 0))).unwrap();
    assert_eq!(b.src[..2], [8, 0]);
    let blank = quads.iter().find(|q| q.dst == dst(cell(9, 2))).unwrap();
    assert_eq!(blank.params, params(FG, 0, BG, 0, 0));
    // the cells draw in two batches: those with a glyph, those without
    let draws = g
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::Draw { .. }))
        .count();
    assert_eq!(draws, 2 + 1, "two batches of cells, then present");
    // the pipeline frees every texture, target and program it made
    let Gpu {
        mut gl,
        pipeline,
        buffer,
        ..
    } = g;
    pipeline.free(&mut gl).unwrap();
    gl.free_target(buffer).unwrap();
    assert_eq!(gl.live(), (0, 0, 0));
}

#[test]
fn a_frame_that_changes_nothing_makes_no_call() {
    let mut g = Gpu::new(StandIn::LIMITS, Config::default());
    let mut s = screen();
    s.text(0, 0, "AB", plain());
    g.frame(&mut s, 0, &[]).unwrap();
    let (damage, _) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!(damage, []);
    assert_eq!(g.calls(), &[]);
}

#[test]
fn a_keystroke_draws_its_cell_and_a_cached_glyph_uploads_nothing() {
    let mut g = Gpu::new(StandIn::LIMITS, Config::default());
    let mut s = screen();
    s.text(0, 0, "AB", plain());
    g.frame(&mut s, 0, &[]).unwrap();
    s.text(4, 1, "A", plain());
    let (damage, stats) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!(damage, [cell(4, 1)]);
    assert_eq!((stats.misses, g.fills()), (0, 0));
    // no clear: a cell's quad covers its pixels
    assert!(!g.calls().iter().any(|c| matches!(c, Call::Clear(..))));
    let quads = g.cells();
    assert_eq!(quads.len(), 1);
    assert_eq!(quads[0].dst, dst(cell(4, 1)));
    assert_eq!(quads[0].src[..2], [0, 0], "A's slot");
}

#[test]
fn a_cell_of_several_layers_names_its_sprites() {
    let mut g = Gpu::new(StandIn::LIMITS, Config::default());
    let mut s = screen();
    let mut st = plain();
    st.attrs.set_underline(UnderlineStyle::Curly);
    st.attrs.insert(Attrs::STRIKE);
    s.text(0, 0, "x", st);
    g.frame(&mut s, 0, &[]).unwrap();
    let q = g
        .cells()
        .into_iter()
        .find(|q| q.dst == dst(cell(0, 0)))
        .unwrap();
    let fg = u32::from_str_radix(FG, 16).unwrap();
    // the curl is the third sprite and the strike the sixth, each plus one
    assert_eq!(q.params, params(FG, 1, BG, fg | 3 << 24, fg | 6 << 24));
}

#[test]
fn a_pool_its_budget_refuses_draws_from_a_held_slot_and_takes_the_glyph_once_the_budget_allows() {
    // ADR-1300 D3: a full pool draws the glyph uncached, from a slot held
    // for the frame; the next frame asks the budget again
    let config = Config {
        budgets: Some(Budgets::glyphs(1)),
        ..Config::default()
    };
    let mut g = Gpu::new(StandIn::LIMITS, config);
    let mut s = screen();
    s.text(0, 0, "AB", plain());
    let (_, stats) = g.frame(&mut s, 0, &[]).unwrap();
    assert_eq!((stats.misses, stats.uncached), (2, 1));
    // B again in a frame that does not draw A: A goes and B stays
    s.text(5, 0, "B", plain());
    let (_, stats) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!((stats.misses, stats.uncached), (1, 0));
    assert!(
        !g.calls()
            .iter()
            .any(|c| matches!(c, Call::CreateTexture(..))),
        "the held slot came back: {:?}",
        g.calls()
    );
    s.text(6, 0, "B", plain());
    let (_, stats) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!((stats.misses, stats.uncached, g.fills()), (0, 0, 0));
    // frames that each draw two glyphs past the budget give their held
    // slots back, so the pool's first texture serves every one of them
    for _ in 0..40 {
        s.text(0, 1, "CDE", plain());
        let (_, stats) = g.frame(&mut s, 1, &[]).unwrap();
        assert_eq!(stats.uncached, 2);
        assert!(
            !g.calls()
                .iter()
                .any(|c| matches!(c, Call::CreateTexture(..))),
            "{:?}",
            g.calls()
        );
    }
}

#[test]
fn a_pool_past_its_first_texture_takes_another_twice_its_size() {
    // a budget past the screen's 60 glyphs, so the pool keeps all 70
    let config = Config {
        budgets: Some(Budgets::glyphs(1000)),
        ..Config::default()
    };
    let mut g = Gpu::new(StandIn::LIMITS, config);
    let mut s = Screen::new(10, 3, rgb(FG), rgb(BG));
    let mut drawn = 0;
    let mut textures = Vec::new();
    // 70 distinct glyphs over a few frames
    for (i, c) in ('!'..).take(70).enumerate() {
        s.text(i % 10, (i / 10) % 3, &c.to_string(), plain());
        if i % 10 == 9 || i == 69 {
            g.frame(&mut s, if drawn == 0 { 0 } else { 1 }, &[])
                .unwrap();
            drawn += 1;
            textures.extend(
                g.calls()
                    .iter()
                    .filter(|c| matches!(c, Call::CreateTexture(..)))
                    .cloned(),
            );
        }
    }
    assert_eq!(
        textures,
        [
            Call::CreateTexture(Format::R8, Size::new(48, 16)),
            Call::CreateTexture(Format::R8, Size::new(512, 16)),
            Call::CreateTexture(Format::R8, Size::new(1024, 16)),
        ]
    );
}

/// A face that draws nothing at all.
struct Blank;

impl Face for Blank {
    fn cell(&self) -> Cell {
        Cell {
            width: 8,
            height: 16,
        }
    }
    fn size_step(&self) -> u32 {
        16
    }
    fn has_glyph(&self, _: char) -> bool {
        false
    }
    fn draw(&self, _: &[char], _: Style) -> Option<vtium_font::Glyph> {
        None
    }
}

#[test]
fn a_glyph_the_font_draws_nothing_for_is_not_asked_for_every_frame() {
    // the C reference's GPU renderer remembered such glyphs (the C fork's
    // issue on a glyph asked for every frame)
    let mut g = Gpu::with_font(Font::new(Blank), StandIn::LIMITS, Config::default());
    let mut s = screen();
    s.text(0, 0, "A", plain());
    let (_, stats) = g.frame(&mut s, 0, &[]).unwrap();
    assert_eq!(stats.misses, 1);
    s.text(1, 0, "A", plain());
    let (_, stats) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!((stats.cells, stats.misses), (1, 0));
    // until its character arrives
    g.renderer.glyphs_changed(&['A']);
    let (_, stats) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!((stats.cells, stats.misses), (2, 1));
}

/// The arrow's program: its texture, and the copy of the pixels beneath it
/// with the pixel of the target the copy's first texel holds.
const ARROW_INPUTS: [&str; 4] = ["target_size", "arrow", "under", "under_at"];
/// The under shader, which copies the pixels beneath a draw out of a tile.
const COPY_INPUTS: [&str; 2] = ["target_size", "under"];

// ADR-1303 D1, D3: the GPU path is judged exactly, so the arrow leaves no
// blend to the GPU's own rounding. Its draw follows a copy of exactly the
// pixels beneath its quads, into a target of its own, and goes over them
// with the CPU path's over_bg in its shader, with Replace.
#[test]
fn the_arrow_is_one_texture_drawn_over_a_copy_of_what_the_frame_drew_beneath_it() {
    let mut g = Gpu::new(StandIn::LIMITS, Config::default());
    let mut s = screen();
    g.state.pointer = Some((10, 3));
    g.frame(&mut s, 0, &[]).unwrap();
    let arrow = g.renderer.upright_arrow().clone();
    let (w, h) = (arrow.width(), arrow.height());
    assert!(
        g.calls()
            .contains(&Call::CreateTexture(Format::Rgba8, Size::new(w, h)))
    );
    let draws: Vec<(&Call, &vtium_render::gpu::Seen)> = g
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::Draw { .. }))
        .zip(g.gl.seen())
        .collect();
    // the cells, the copy of what lies beneath the arrow, the arrow over
    // the copy, then present
    let (
        Call::Draw {
            quads: copied,
            blend: copy_blend,
            inputs: copy_inputs,
        },
        copy_seen,
    ) = draws[1]
    else {
        unreachable!()
    };
    let (
        Call::Draw {
            quads,
            blend,
            inputs,
        },
        seen,
    ) = draws[2]
    else {
        unreachable!()
    };
    assert_eq!(
        (*copy_blend, copy_inputs.as_slice()),
        (Blend::Replace, &COPY_INPUTS[..])
    );
    assert_eq!(
        (*blend, inputs.as_slice()),
        (Blend::Replace, &ARROW_INPUTS[..])
    );
    assert_ne!(
        copy_seen.target, seen.target,
        "the copy draws into the tile"
    );
    // each arrow quad reads the copy where the copy put its pixels
    let under_at = seen.ivec4.iter().find(|(n, _)| *n == "under_at").unwrap().1;
    assert_eq!(copied.len(), quads.len());
    for (c, q) in copied.iter().zip(quads) {
        assert_eq!((c.src[0], c.src[1]), (q.dst[0] as u32, q.dst[1] as u32));
        assert_eq!(
            c.dst,
            [
                q.dst[0] - under_at[0],
                q.dst[1] - under_at[1],
                q.dst[2],
                q.dst[3]
            ]
        );
    }
    // the whole frame drew, so the arrow covers its whole area once
    let (hx, hy) = arrow.hot();
    let at = (2 + 10 - hx as i32, 1 + 3 - hy as i32);
    let covered: u32 = g
        .quads(&ARROW_INPUTS)
        .iter()
        .map(|q| {
            assert_eq!(q.src[0] as i32, q.dst[0] - at.0);
            (q.dst[2] * q.dst[3]) as u32
        })
        .sum();
    assert_eq!(covered, w * h);
    // moved, it is drawn over the cells it left and enters, and its
    // texture is not uploaded again
    g.state.pointer = Some((30, 20));
    g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!(g.fills(), 0);
    let quads = g.quads(&ARROW_INPUTS);
    assert!(!quads.is_empty());
    let cells: Vec<Rect> = g
        .cells()
        .iter()
        .map(|q| {
            rect(
                q.dst[0] as u32,
                q.dst[1] as u32,
                q.dst[2] as u32,
                q.dst[3] as u32,
            )
        })
        .collect();
    for q in &quads {
        for y in q.dst[1]..q.dst[1] + q.dst[3] {
            for x in q.dst[0]..q.dst[0] + q.dst[2] {
                let (x, y) = (x as u32, y as u32);
                assert!(
                    cells
                        .iter()
                        .any(|c| x >= c.x && x < c.x + c.width && y >= c.y && y < c.y + c.height),
                    "the arrow at {x},{y} over a cell the frame did not draw"
                );
            }
        }
    }
}

// the issue on the arrow's blend and the copy frame: the copy of the
// pixels beneath a draw lasts from frame to frame, so a frame that draws
// over what it drew makes no target of its own; it is given back once no
// frame has read it for GPU_IDLE_FRAMES frames, counted in frames, and at
// a release
#[test]
fn the_copy_of_the_pixels_beneath_lasts_until_no_frame_reads_it_for_the_idle_frames() {
    let mut g = Gpu::new(StandIn::LIMITS, Config::default());
    let mut s = screen();
    let count =
        |g: &Gpu<Unifont>, want: fn(&Call) -> bool| g.calls().iter().filter(|c| want(c)).count();
    let made = |c: &Call| matches!(c, Call::CreateFrame(_));
    let freed = |c: &Call| matches!(c, Call::FreeTarget);
    g.state.pointer = Some((10, 3));
    g.frame(&mut s, 0, &[]).unwrap();
    assert_eq!((count(&g, made), count(&g, freed)), (1, 0));
    let (_, targets, _) = g.gl.live();
    // moved, the arrow reads the copy again, which makes and frees nothing
    g.state.pointer = Some((30, 20));
    g.frame(&mut s, 1, &[]).unwrap();
    assert!(!g.quads(&ARROW_INPUTS).is_empty());
    assert_eq!((count(&g, made), count(&g, freed)), (0, 0));
    assert_eq!(g.gl.live().1, targets);
    // gone, nothing reads the copy, and it goes after the idle frames
    g.state.pointer = None;
    for _ in 1..vtium_render::GPU_IDLE_FRAMES {
        g.frame(&mut s, 1, &[]).unwrap();
        assert_eq!(count(&g, freed), 0);
    }
    g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!(count(&g, freed), 1);
    assert_eq!(g.gl.live().1, targets - 1);
    // made again when a frame reads it, and freed at a release with the
    // upright frame, leaving the display's buffer
    g.state.pointer = Some((10, 3));
    g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!(count(&g, made), 1);
    g.pipeline.release(&mut g.gl).unwrap();
    assert_eq!(g.gl.live().1, 1);
}

fn notice() -> Placed {
    let font = unifont();
    let surface = Surface {
        width: DISPLAY.width,
        height: DISPLAY.height,
        orientation: Orientation::Normal,
        depth: Depth::Color,
    };
    Notice::new("hi")
        .upright(&font, surface, Colors::NOTICE, None)
        .unwrap()
}

#[test]
fn a_notice_is_uploaded_whole_once_then_over_what_the_frame_redraws_beneath_it() {
    let mut g = Gpu::new(StandIn::LIMITS, Config::default());
    let mut s = screen();
    let n = notice();
    let area = n.rect();
    assert_eq!((area.x, area.y, area.width), (0, 0, DISPLAY.width));
    g.frame(&mut s, 0, &[]).unwrap();
    // a new notice is uploaded whole over a frame that changes nothing
    // else, and presented
    let (damage, _) = g.frame(&mut s, 1, std::slice::from_ref(&n)).unwrap();
    assert_eq!(damage, [area]);
    assert_eq!(g.calls()[0], Call::FillTexture(area));
    assert_eq!(g.quads(&["target_size", "frame"]).len(), 1);
    // a frame that draws nothing beneath it uploads nothing
    s.text(0, 2, "z", plain());
    g.frame(&mut s, 1, std::slice::from_ref(&n)).unwrap();
    assert_eq!(g.fills(), 1, "the glyph's slot alone: {:?}", g.calls());
    // a cell redrawn beneath it takes the notice's part over the cell
    s.text(1, 0, "y", plain());
    g.frame(&mut s, 1, std::slice::from_ref(&n)).unwrap();
    let last = g
        .calls()
        .iter()
        .rposition(|c| matches!(c, Call::FillTexture(_)));
    let draws = g
        .calls()
        .iter()
        .rposition(|c| matches!(c, Call::Draw { .. }));
    assert_eq!(g.calls()[last.unwrap()], Call::FillTexture(cell(1, 0)));
    assert!(last < draws, "uploaded before present");
    // gone, the cells beneath it draw again and nothing is uploaded
    let (damage, stats) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!(stats.cells, 10 * area.height.div_ceil(16).min(3));
    assert!(!damage.is_empty());
    assert_eq!(g.fills(), 0);
}

#[test]
fn a_display_past_the_largest_texture_draws_each_cell_into_each_tile_it_meets() {
    let limits = Limits {
        texture: 48,
        ..StandIn::LIMITS
    };
    let mut g = Gpu::new(limits, Config::default());
    let mut s = screen();
    s.text(0, 0, "AB", plain());
    g.frame(&mut s, 0, &[]).unwrap();
    // tiles of 48x48, 36x48, 48x3 and 36x3: column 5 crosses the seam at
    // 48, from 42 to 50
    s.text(5, 0, "A", plain());
    g.frame(&mut s, 1, &[]).unwrap();
    let quads = g.cells();
    assert_eq!(
        quads.iter().map(|q| q.dst).collect::<Vec<_>>(),
        [[42, 1, 8, 16], [42 - 48, 1, 8, 16]]
    );
    // present draws each tile's own part
    let present = g.quads(&["target_size", "frame"]);
    assert_eq!(
        present.iter().map(|q| (q.dst, q.src)).collect::<Vec<_>>(),
        [
            ([42, 1, 6, 16], [42, 1, 6, 16]),
            ([48, 1, 2, 16], [0, 1, 2, 16])
        ]
    );
}

// the issue on GPU texture garbage collection: the first frame after a
// release makes the upright frame's tiles again; on a display of four
// tiles, one whose third tile fails frees the two made before it, and the
// next frame makes all four and draws whole
#[test]
fn a_restore_that_fails_partway_frees_the_tiles_it_made_before_the_failure() {
    let limits = Limits {
        texture: 48,
        ..StandIn::LIMITS
    };
    let mut g = Gpu::new(limits, Config::default());
    let mut s = screen();
    s.text(0, 0, "AB", plain());
    g.frame(&mut s, 0, &[]).unwrap();
    let (_, drawing, _) = g.gl.live();
    g.pipeline.release(&mut g.gl).unwrap();
    let (_, released, _) = g.gl.live();
    // the buffer alone
    assert_eq!((drawing, released), (5, 1));
    g.gl.fail_after(Op::CreateFrame, 2, ErrorKind::OutOfMemory);
    let e = g.frame(&mut s, 1, &[]).unwrap_err();
    assert_eq!(e.kind, ErrorKind::OutOfMemory);
    let made = |c: &Call| matches!(c, Call::CreateFrame(_));
    assert_eq!(g.calls().iter().filter(|c| made(c)).count(), 2);
    assert_eq!(
        g.gl.live().1,
        released,
        "a tile made before the failure leaked"
    );
    let (damage, _) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!(damage, [DISPLAY.rect()]);
    assert_eq!(g.calls().iter().filter(|c| made(c)).count(), 4);
    assert_eq!(g.gl.live().1, drawing);
}

#[test]
fn a_failed_frame_is_reported_and_the_next_one_draws_whole() {
    let mut g = Gpu::new(StandIn::LIMITS, Config::default());
    let mut s = screen();
    s.text(0, 0, "AB", plain());
    // the pool's first texture fails, as a driver out of memory would
    g.gl.fail_next(Op::CreateTexture, ErrorKind::OutOfMemory);
    let e = g.frame(&mut s, 0, &[]).unwrap_err();
    assert_eq!(e.kind, ErrorKind::OutOfMemory);
    let (damage, stats) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!((damage, stats.cells), (vec![DISPLAY.rect()], 30));
    // a GL error in the composition's draws drops that frame
    s.text(3, 0, "C", plain());
    g.gl.raise(ErrorKind::Gl);
    let e = g.frame(&mut s, 1, &[]).unwrap_err();
    assert_eq!((e.kind, e.batch), (ErrorKind::Gl, Some(Batch::Uploads)));
    let (damage, stats) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!((damage, stats.cells), (vec![DISPLAY.rect()], 30));
    assert!(g.calls().contains(&Call::Clear(DISPLAY.rect(), 0x10_1010)));
}

#[test]
fn a_new_renderer_on_the_pipeline_replaces_the_glyphs_and_draws_whole() {
    // a new font or cell size makes a new renderer
    let mut g = Gpu::new(StandIn::LIMITS, Config::default());
    let mut s = screen();
    s.text(0, 0, "AB", plain());
    g.frame(&mut s, 0, &[]).unwrap();
    let live = g.gl.live();
    g.renderer = Renderer::new(unifont(), DISPLAY.width, DISPLAY.height, Config::default());
    g.since = Age::NEVER;
    let (damage, stats) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!((damage, stats.misses), (vec![DISPLAY.rect()], 2));
    let frees = g
        .calls()
        .iter()
        .filter(|c| matches!(c, Call::FreeTexture))
        .count();
    assert_eq!(frees, 2, "the old sprites and pool");
    assert_eq!(g.gl.live(), live);
}

#[test]
fn a_1_bit_display_is_refused_until_present_reduces_the_depth() {
    let config = Config {
        depth: Depth::One,
        ..Config::default()
    };
    let mut g = Gpu::new(StandIn::LIMITS, config);
    let e = g.frame(&mut screen(), 0, &[]).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Misuse);
    assert_eq!(g.calls(), &[]);
}

#[test]
fn a_frame_the_cpu_path_draws_makes_the_next_gpu_frame_whole() {
    // the CPU path's frame moves the renderer's history on while the
    // upright frame keeps the last GPU frame, and the glyphs forget nothing
    // the CPU path's arrivals named
    let mut g = Gpu::new(StandIn::LIMITS, Config::default());
    let mut s = screen();
    s.text(0, 0, "AB", plain());
    g.frame(&mut s, 0, &[]).unwrap();
    s.text(3, 0, "C", plain());
    g.renderer.glyphs_changed(&['A']);
    let mut canvas = vtium_render::Canvas::new(DISPLAY.width, DISPLAY.height, 0);
    g.renderer.draw(&s.frame(g.since), &mut canvas, 1, &g.state);
    g.since = s.damage.checkpoint();
    let (damage, stats) = g.frame(&mut s, 1, &[]).unwrap();
    assert_eq!((damage, stats.cells), (vec![DISPLAY.rect()], 30));
    assert!(
        g.calls().contains(&Call::FreeTexture),
        "the glyphs are made anew"
    );
}

#[test]
fn present_draws_one_whole_rectangle_where_a_buffer_lacks_the_whole_frame() {
    // a buffer two frames old lacks the whole first frame and the cell
    // after it: the whole buffer covers both
    let mut gl = StandIn::new();
    let mut p = Pipeline::new(&mut gl, DISPLAY, GpuConfig::default()).unwrap();
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    p.frame(&mut gl, &Work::new(0, &[], &[]), &buffer, 0)
        .unwrap();
    let c = [cell(0, 0)];
    let damage = p.frame(&mut gl, &Work::new(0, &c, &c), &buffer, 2).unwrap();
    assert_eq!(damage, [DISPLAY.rect()]);
}
