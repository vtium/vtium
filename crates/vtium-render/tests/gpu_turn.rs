//! The GPU path's turn on the stand-in GL (ADR-1303 D3, D6, D9; D11 part 4):
//! present turns the upright frame into the display's buffer, and the grid,
//! the pointer and the notices land on the pixels where the CPU path draws
//! them, at every turn, with an odd margin on every side. A turn keeps every
//! texture and compiles nothing.
//!
//! The stand-in draws no pixels, so each test reads present's quads through
//! [`texel`], which does what present's fragment shader does
//! (`src/gpu/glsl/present_head.frag`), and compares where each upright pixel
//! lands with the CPU path's own drawing of the same screen. Tier 2 judges
//! the real shader's pixels.
#![cfg(feature = "gpu")]
#![expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "the stand-in's quads hold small coordinates"
)]

mod common;

use common::{Screen, arrow_over, rgb, style, unifont};
use vtium_render::gpu::{
    Call, Config as GpuConfig, DrawTarget, ErrorKind, Gl, Limits, Op, Pipeline, Quad, Size,
    StandIn, Work,
};
use vtium_render::{
    Canvas, Colors, Config, Depth, DrawState, Notice, Orientation, Rect, Renderer, Surface,
};
use vtium_term::Age;

const FG: &str = "c0c0c0";
const BG: &str = "101010";

/// With 8x16 cells, 85x53 leaves 5 pixels to spare on each axis at every
/// turn: 10x3 cells upright, and 6x5 turned a quarter, 80 pixels across
/// the display's width and 48 down its height.
const DISPLAY: Size = Size::new(85, 53);

const ALL: [Orientation; 4] = [
    Orientation::Normal,
    Orientation::Right,
    Orientation::UpsideDown,
    Orientation::Left,
];

fn grid(o: Orientation, display: Size) -> (usize, usize) {
    let (across, down) = match o {
        Orientation::Right | Orientation::Left => (display.height, display.width),
        _ => (display.width, display.height),
    };
    ((across / 8) as usize, (down / 16) as usize)
}

fn config(o: Orientation) -> Config {
    Config {
        orientation: o,
        ..Config::default()
    }
}

fn pixel(hex: &str) -> u32 {
    u32::from_str_radix(hex, 16).unwrap()
}

/// The display pixel that draws the upright pixel `x`, `y`, as the CPU
/// path's grid and notice.rs's `Surface` turn it.
fn place(o: Orientation, display: Size, x: u32, y: u32) -> (u32, u32) {
    let (w, h) = (display.width, display.height);
    match o {
        Orientation::Normal => (x, y),
        Orientation::UpsideDown => (w - 1 - x, h - 1 - y),
        Orientation::Right => (w - 1 - y, x),
        Orientation::Left => (y, h - 1 - x),
    }
}

/// present_head.frag's `texel`, which [`texel`] copies: a change to one
/// fails `the_shaders_texel_is_the_one_the_tests_read` until the other
/// follows it.
const TEXEL_GLSL: &str = "\
ivec2 texel(ivec2 p) {
    ivec2 d = p - dst.xy;
    ivec2 last = dst.zw - 1;
    ivec2 u;
    if (params.x == 1u) {
        u = ivec2(d.y, last.x - d.x);
    } else if (params.x == 2u) {
        u = last - d;
    } else if (params.x == 3u) {
        u = ivec2(last.y - d.y, d.x);
    } else {
        u = d;
    }
    return ivec2(src.xy) + u;
}
";

/// The texel of the upright frame's tile that present's shader reads for
/// the display pixel `x`, `y` of `q`: [`TEXEL_GLSL`] in Rust.
fn texel(q: &Quad, x: i32, y: i32) -> (u32, u32) {
    let (dx, dy) = (x - q.dst[0], y - q.dst[1]);
    let (lx, ly) = (q.dst[2] - 1, q.dst[3] - 1);
    let (ux, uy) = match q.params[0] {
        1 => (dy, lx - dx),
        2 => (lx - dx, ly - dy),
        3 => (ly - dy, dx),
        _ => (dx, dy),
    };
    (q.src[0] + ux as u32, q.src[1] + uy as u32)
}

fn present_quads(calls: &[Call]) -> Vec<Quad> {
    quads(calls, &["target_size", "frame"])
}

fn quads(calls: &[Call], inputs: &[&str]) -> Vec<Quad> {
    calls
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

/// For each pixel of a display of `size`, the upright frame's pixel present
/// draws there, from a frame of one tile. Damage rectangles may overlap,
/// and a pixel present draws twice reads the same texel each time.
fn presented(calls: &[Call], size: Size) -> Vec<Option<(u32, u32)>> {
    let mut out = vec![None; (size.width * size.height) as usize];
    for q in present_quads(calls) {
        for y in q.dst[1]..q.dst[1] + q.dst[3] {
            for x in q.dst[0]..q.dst[0] + q.dst[2] {
                let i = (y as u32 * size.width + x as u32) as usize;
                let t = texel(&q, x, y);
                assert!(
                    out[i].is_none_or(|o| o == t),
                    "{x},{y} presented from two texels"
                );
                out[i] = Some(t);
            }
        }
    }
    out
}

fn inside(r: [i32; 4], (x, y): (u32, u32)) -> bool {
    let (x, y) = (x as i32, y as i32);
    x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]
}

/// A display drawn by both paths: the CPU path into a canvas, the GPU path
/// on the stand-in GL into one buffer.
struct Both {
    orientation: Orientation,
    gl: StandIn,
    pipeline: Pipeline,
    buffer: DrawTarget,
    gpu: Renderer<vtium_font::Unifont>,
    cpu: Renderer<vtium_font::Unifont>,
    canvas: Canvas,
    since: (Age, Age),
    state: DrawState,
}

impl Both {
    fn new(o: Orientation, display: Size, limits: Limits) -> Both {
        let mut gl = StandIn::with_limits(limits);
        let pipeline = Pipeline::new(&mut gl, display, GpuConfig::default()).unwrap();
        let buffer = gl.import_buffer(&display).unwrap();
        Both {
            orientation: o,
            gl,
            pipeline,
            buffer,
            gpu: Renderer::new(unifont(), display.width, display.height, config(o)),
            cpu: Renderer::new(unifont(), display.width, display.height, config(o)),
            canvas: Canvas::new(display.width, display.height, 0),
            since: (Age::NEVER, Age::NEVER),
            state: DrawState {
                background: rgb(BG),
                ..DrawState::default()
            },
        }
    }

    /// Draws `s` on both paths, the buffer and the canvas `age` frames old,
    /// with `notice` over it, and gives each path's damage; the stand-in
    /// keeps the GPU frame's calls.
    fn frame(
        &mut self,
        s: &mut Screen,
        age: u32,
        notice: Option<&Notice>,
    ) -> (Vec<Rect>, Vec<Rect>) {
        let surface = self.surface();
        let font = unifont();
        let overlays: Vec<_> = notice
            .and_then(|n| n.upright(&font, surface, Colors::NOTICE, None))
            .into_iter()
            .collect();
        let cpu = self
            .cpu
            .draw(&s.frame(self.since.0), &mut self.canvas, age, &self.state)
            .damage
            .to_vec();
        if let Some(n) = notice {
            n.draw(&font, surface, Colors::NOTICE, None, &mut self.canvas);
        }
        self.since.0 = s.damage.checkpoint();
        self.gl.clear_calls();
        let gpu = self
            .gpu
            .draw_gpu(
                &mut self.gl,
                &mut self.pipeline,
                &s.frame(self.since.1),
                &self.buffer,
                age,
                &self.state,
                &overlays,
            )
            .unwrap()
            .damage
            .to_vec();
        self.since.1 = s.damage.checkpoint();
        assert_eq!(self.pipeline.orientation(), self.orientation);
        (cpu, gpu)
    }

    /// Turns the display on both paths.
    fn turn(&mut self, o: Orientation) {
        self.orientation = o;
        self.gpu = core::mem::replace(&mut self.gpu, renderer(o)).turned(o);
        self.cpu = core::mem::replace(&mut self.cpu, renderer(o)).turned(o);
    }

    fn surface(&self) -> Surface {
        let size = self.buffer.size();
        Surface {
            width: size.width,
            height: size.height,
            orientation: self.orientation,
            depth: Depth::Color,
        }
    }
}

/// A stand-in renderer, for a moment while another is turned.
fn renderer(o: Orientation) -> Renderer<vtium_font::Unifont> {
    Renderer::new(unifont(), 8, 16, config(o))
}

#[test]
fn present_reads_each_display_pixel_from_the_upright_pixel_the_cpu_path_turns_there() {
    for o in ALL {
        let mut gl = StandIn::new();
        let mut p = Pipeline::turned(&mut gl, DISPLAY, o, GpuConfig::default()).unwrap();
        let buffer = gl.import_buffer(&DISPLAY).unwrap();
        gl.clear_calls();
        let damage = p
            .frame(&mut gl, &Work::new(0, &[], &[]), &buffer, 0)
            .unwrap();
        assert_eq!(damage, [DISPLAY.rect()], "{o:?}");
        let quads = present_quads(gl.calls());
        assert_eq!(quads.len(), 1, "{o:?}");
        for (i, u) in presented(gl.calls(), DISPLAY).into_iter().enumerate() {
            let (x, y) = (i as u32 % DISPLAY.width, i as u32 / DISPLAY.width);
            let (ux, uy) = u.unwrap_or_else(|| panic!("{o:?}: {x},{y} not presented"));
            assert_eq!(place(o, DISPLAY, ux, uy), (x, y), "{o:?}");
        }
    }
}

#[test]
fn a_turned_pipeline_holds_its_upright_frame_with_the_sides_swapped() {
    let mut gl = StandIn::new();
    let p = Pipeline::turned(&mut gl, DISPLAY, Orientation::Right, GpuConfig::default()).unwrap();
    assert_eq!(gl.calls()[0], Call::CreateFrame(Size::new(53, 85)));
    assert_eq!((p.size(), p.orientation()), (DISPLAY, Orientation::Right));
}

#[test]
fn the_grid_lands_on_the_cpu_paths_pixels_at_every_turn_with_an_odd_margin() {
    // ADR-1303 D3: the GPU path puts a turned grid where the CPU path puts
    // it, mapped back into the upright frame, so the two paths draw it at
    // the same pixels
    let (first, ink, last) = ("ff0000", "ffff00", "00ff00");
    for o in ALL {
        let (cols, rows) = grid(o, DISPLAY);
        let mut b = Both::new(o, DISPLAY, StandIn::LIMITS);
        let mut s = Screen::new(cols, rows, rgb(FG), rgb(BG));
        s.text(0, 0, "A", style(rgb(ink), rgb(first)));
        s.text(cols - 1, rows - 1, " ", style(rgb(FG), rgb(last)));
        b.frame(&mut s, 0, None);
        let cells = quads(
            b.gl.calls(),
            &["target_size", "masks", "colors", "sprites", "cell"],
        );
        let shown = presented(b.gl.calls(), DISPLAY);
        for (bg, colors) in [(first, [first, ink]), (last, [last, last])] {
            let quad = cells
                .iter()
                .find(|q| q.params[1] == pixel(bg))
                .unwrap_or_else(|| panic!("{o:?}: no cell of {bg}"));
            // the display pixels present draws from the cell's quad are the
            // pixels the CPU path draws the cell on, its background and its
            // glyph, which no other cell shares
            let at = |i: usize| (i as u32 % DISPLAY.width, i as u32 / DISPLAY.width);
            let gpu: Vec<usize> = (0..shown.len())
                .filter(|&i| shown[i].is_some_and(|u| inside(quad.dst, u)))
                .collect();
            let cpu: Vec<usize> = (0..shown.len())
                .filter(|&i| {
                    let (x, y) = at(i);
                    colors
                        .iter()
                        .any(|&c| b.canvas.pixel(x, y) == Some(pixel(c)))
                })
                .collect();
            assert_eq!(gpu.len(), 8 * 16, "{o:?} {bg}");
            assert_eq!(
                (gpu.first().map(|&i| at(i)), gpu.len()),
                (cpu.first().map(|&i| at(i)), cpu.len()),
                "{o:?} {bg}: the cell's first pixel and its pixels"
            );
            assert_eq!(gpu, cpu, "{o:?} {bg}");
        }
    }
}

#[test]
fn the_pointer_and_the_border_beneath_it_land_where_the_cpu_path_draws_them_at_every_turn() {
    // the pointer appears with its tip near the grid's last pixel, so it
    // reaches past the grid into the border: the frame draws the cells and
    // the border beneath it, and the arrow over them
    let cells_bg = "202020";
    for o in ALL {
        let (cols, rows) = grid(o, DISPLAY);
        let mut b = Both::new(o, DISPLAY, StandIn::LIMITS);
        let mut s = Screen::new(cols, rows, rgb(FG), rgb(cells_bg));
        b.frame(&mut s, 0, None);
        b.state.pointer = Some((cols as u32 * 8 - 6, rows as u32 * 16 - 12));
        b.frame(&mut s, 1, None);
        let arrow = b.gpu.upright_arrow().clone();
        let arrows = quads(b.gl.calls(), &["target_size", "arrow", "under", "under_at"]);
        let cells = quads(
            b.gl.calls(),
            &["target_size", "masks", "colors", "sprites", "cell"],
        );
        let clears: Vec<(Rect, u32)> =
            b.gl.calls()
                .iter()
                .filter_map(|c| match c {
                    Call::Clear(r, color) => Some((*r, *color)),
                    _ => None,
                })
                .collect();
        assert!(!clears.is_empty(), "{o:?}: the border beneath the arrow");
        let (mut drawn, mut border) = (0, 0);
        for (i, u) in presented(b.gl.calls(), DISPLAY).into_iter().enumerate() {
            let Some(u) = u else { continue };
            let (x, y) = (i as u32 % DISPLAY.width, i as u32 / DISPLAY.width);
            // what the upright frame holds there: a cell's background, or
            // the border's color a clear filled
            let under = match cells.iter().find(|q| inside(q.dst, u)) {
                Some(q) => q.params[1],
                None => {
                    let (_, color) = clears
                        .iter()
                        .find(|(r, _)| {
                            inside([r.x as i32, r.y as i32, r.width as i32, r.height as i32], u)
                        })
                        .unwrap_or_else(|| panic!("{o:?}: {x},{y} presented, but nothing drew it"));
                    border += 1;
                    *color
                }
            };
            let src = arrows.iter().find(|q| inside(q.dst, u)).map_or(0, |q| {
                let ax = q.src[0] + u.0 - q.dst[0] as u32;
                let ay = q.src[1] + u.1 - q.dst[1] as u32;
                arrow.pixels()[(ay * arrow.width() + ax) as usize]
            });
            drawn += usize::from(src != 0);
            assert_eq!(
                b.canvas.pixel(x, y),
                Some(arrow_over(under, src)),
                "{o:?} at {x},{y}"
            );
        }
        assert!(drawn > 20, "{o:?}: the arrow drew {drawn} pixels");
        assert!(border > 0, "{o:?}: no border presented");
    }
}

#[test]
fn a_notice_lands_where_the_cpu_path_draws_it_at_every_turn() {
    let font = unifont();
    for o in ALL {
        let (cols, rows) = grid(o, DISPLAY);
        let mut b = Both::new(o, DISPLAY, StandIn::LIMITS);
        let mut s = Screen::new(cols, rows, rgb(FG), rgb(BG));
        b.frame(&mut s, 0, None);
        let notice = Notice::new("hi");
        b.frame(&mut s, 1, Some(&notice));
        let placed = notice
            .upright(&font, b.surface(), Colors::NOTICE, None)
            .unwrap();
        let area = placed.rect();
        assert!(b.gl.calls().contains(&Call::FillTexture(area)), "{o:?}");
        let shown = presented(b.gl.calls(), DISPLAY);
        let mut covered = 0;
        for (i, u) in shown.into_iter().enumerate() {
            let Some((ux, uy)) = u else { continue };
            if ux < area.x || uy < area.y || ux >= area.x + area.width || uy >= area.y + area.height
            {
                continue;
            }
            covered += 1;
            let (x, y) = (i as u32 % DISPLAY.width, i as u32 / DISPLAY.width);
            let want = placed.picture.pixels()[((uy - area.y) * area.width + ux - area.x) as usize];
            assert_eq!(b.canvas.pixel(x, y), Some(want), "{o:?} at {x},{y}");
        }
        assert_eq!(covered, area.area(), "{o:?}");
        // gone, it leaves the cells and the border beneath it drawn again
        // where the CPU path draws them, which the console hands the CPU
        // path's renderer as the area it covered
        let surface = b.surface();
        let shown = notice
            .draw(
                &font,
                surface,
                Colors::NOTICE,
                None,
                &mut Canvas::new(DISPLAY.width, DISPLAY.height, 0),
            )
            .unwrap();
        b.cpu.redraw_area(shown);
        let (cpu, gpu) = b.frame(&mut s, 1, None);
        assert!(!cpu.is_empty(), "{o:?}");
        assert_eq!(gpu, cpu, "{o:?}");
    }
}

#[test]
fn the_damage_is_turned_into_the_displays_pixels() {
    for o in ALL {
        let (cols, rows) = grid(o, DISPLAY);
        let mut b = Both::new(o, DISPLAY, StandIn::LIMITS);
        let mut s = Screen::new(cols, rows, rgb(FG), rgb(BG));
        b.frame(&mut s, 0, None);
        s.text(1, 1, "x", style(rgb(FG), rgb(BG)));
        s.text(cols - 1, rows - 1, "y", style(rgb(FG), rgb(BG)));
        let (cpu, gpu) = b.frame(&mut s, 1, None);
        assert_eq!(cpu.len(), 2, "{o:?}");
        assert_eq!(gpu, cpu, "{o:?}");
        // present draws exactly the damage
        let drawn: Vec<Rect> = present_quads(b.gl.calls())
            .iter()
            .map(|q| Rect {
                x: q.dst[0] as u32,
                y: q.dst[1] as u32,
                width: q.dst[2] as u32,
                height: q.dst[3] as u32,
            })
            .collect();
        assert_eq!(drawn, gpu, "{o:?}");
    }
}

#[test]
fn a_frame_past_the_largest_texture_presents_each_pixel_once_at_every_turn() {
    // tiles of at most 48 pixels a side, the sprites' texture the widest
    let limits = Limits {
        texture: 48,
        ..StandIn::LIMITS
    };
    for o in ALL {
        let (cols, rows) = grid(o, DISPLAY);
        let mut b = Both::new(o, DISPLAY, limits);
        let mut s = Screen::new(cols, rows, rgb(FG), rgb(BG));
        b.frame(&mut s, 0, None);
        let shown = presented(b.gl.calls(), DISPLAY);
        assert!(shown.iter().all(Option::is_some), "{o:?}");
        assert!(present_quads(b.gl.calls()).len() > 1, "{o:?}");
    }
}

/// The calls that make, fill or free a texture, a target or a program.
fn set_up(calls: &[Call]) -> Vec<&Call> {
    calls
        .iter()
        .filter(|c| {
            matches!(
                c,
                Call::CreateTexture(..)
                    | Call::FillTexture(_)
                    | Call::FreeTexture
                    | Call::CreateFrame(_)
                    | Call::FreeTarget
                    | Call::Compile(_)
                    | Call::FreeProgram
            )
        })
        .collect()
}

#[test]
fn a_turn_keeps_every_texture_and_compiles_nothing() {
    // ADR-1303 D9: a turn keeps the GL state it can, so a turn makes nothing
    // that could fail and leave the screen frozen
    let square = Size::new(64, 64);
    let mut b = Both::new(Orientation::Normal, square, StandIn::LIMITS);
    let mut s = Screen::new(8, 4, rgb(FG), rgb(BG));
    s.text(0, 0, "AB", style(rgb(FG), rgb(BG)));
    b.frame(&mut s, 0, None);
    let live = b.gl.live();
    for o in [
        Orientation::Right,
        Orientation::UpsideDown,
        Orientation::Left,
    ] {
        b.turn(o);
        let (cpu, gpu) = b.frame(&mut s, 1, None);
        assert_eq!((&gpu, &cpu), (&vec![square.rect()], &vec![square.rect()]));
        assert_eq!(set_up(b.gl.calls()), Vec::<&Call>::new(), "{o:?}");
        assert_eq!(b.gl.live(), live, "{o:?}");
        assert_eq!(b.pipeline.orientation(), o);
    }
}

#[test]
fn a_quarter_turn_of_a_display_that_is_not_square_makes_only_the_upright_frame_anew() {
    let mut b = Both::new(Orientation::Normal, DISPLAY, StandIn::LIMITS);
    let mut s = Screen::new(10, 3, rgb(FG), rgb(BG));
    s.text(0, 0, "AB", style(rgb(FG), rgb(BG)));
    b.frame(&mut s, 0, None);
    let live = b.gl.live();
    b.turn(Orientation::Right);
    let mut s = Screen::new(6, 5, rgb(FG), rgb(BG));
    s.text(0, 0, "AB", style(rgb(FG), rgb(BG)));
    b.since = (Age::NEVER, Age::NEVER);
    b.frame(&mut s, 1, None);
    assert_eq!(
        set_up(b.gl.calls()),
        [&Call::CreateFrame(Size::new(53, 85)), &Call::FreeTarget]
    );
    assert_eq!(b.gl.live(), live);
    // a half turn from there keeps the frame too
    b.turn(Orientation::Left);
    b.frame(&mut s, 1, None);
    assert_eq!(set_up(b.gl.calls()), Vec::<&Call>::new());
}

#[test]
fn the_shaders_texel_is_the_one_the_tests_read() {
    let shader = include_str!("../src/gpu/glsl/present_head.frag");
    assert!(
        shader.contains(TEXEL_GLSL),
        "present_head.frag's texel() is not the one this file copies in Rust"
    );
}

/// Present's quads of the last frame, each inside a display of `size`,
/// covering it exactly once.
fn covers_exactly(calls: &[Call], size: Size) {
    let quads = present_quads(calls);
    let mut n = vec![0u32; (size.width * size.height) as usize];
    for q in &quads {
        assert!(
            q.dst[0] >= 0
                && q.dst[1] >= 0
                && q.dst[0] + q.dst[2] <= size.width as i32
                && q.dst[1] + q.dst[3] <= size.height as i32,
            "a quad past the display: {q:?}"
        );
        for y in q.dst[1]..q.dst[1] + q.dst[3] {
            for x in q.dst[0]..q.dst[0] + q.dst[2] {
                n[(y as u32 * size.width + x as u32) as usize] += 1;
            }
        }
    }
    assert!(n.iter().all(|&c| c == 1), "{quads:?}");
}

#[test]
fn a_turn_whose_check_fails_leaves_the_pipeline_as_it_was() {
    // a GL error raised before a quarter turn fails the turn's check: the
    // frame and the orientation stay, so turning back finds them in step
    let mut b = Both::new(Orientation::Normal, DISPLAY, StandIn::LIMITS);
    let mut s = Screen::new(10, 3, rgb(FG), rgb(BG));
    b.frame(&mut s, 0, None);
    let live = b.gl.live();
    b.turn(Orientation::Right);
    b.gl.raise(ErrorKind::Gl);
    let e = b
        .gpu
        .draw_gpu(
            &mut b.gl,
            &mut b.pipeline,
            &s.frame(Age::NEVER),
            &b.buffer,
            0,
            &b.state,
            &[],
        )
        .map(|_| ())
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Gl);
    assert_eq!(b.pipeline.orientation(), Orientation::Normal);
    assert_eq!(b.gl.live(), live);
    b.turn(Orientation::Normal);
    b.since = (Age::NEVER, Age::NEVER);
    b.frame(&mut s, 0, None);
    covers_exactly(b.gl.calls(), DISPLAY);
}

#[test]
fn a_turn_whose_allocation_fails_leaves_the_pipeline_as_it_was() {
    let mut gl = StandIn::new();
    let mut p = Pipeline::new(&mut gl, DISPLAY, GpuConfig::default()).unwrap();
    let buffer = gl.import_buffer(&DISPLAY).unwrap();
    let live = gl.live();
    gl.fail_next(Op::CreateFrame, ErrorKind::OutOfMemory);
    let e = p.turn(&mut gl, Orientation::Right).unwrap_err();
    assert_eq!(e.kind, ErrorKind::OutOfMemory);
    assert_eq!((p.orientation(), gl.live()), (Orientation::Normal, live));
    // the frame is still the upright display's
    gl.clear_calls();
    p.frame(&mut gl, &Work::new(0, &[], &[]), &buffer, 0)
        .unwrap();
    assert!(!gl.calls().iter().any(|c| matches!(c, Call::CreateFrame(_))));
    covers_exactly(gl.calls(), DISPLAY);
    for (i, u) in presented(gl.calls(), DISPLAY).into_iter().enumerate() {
        let (x, y) = (i as u32 % DISPLAY.width, i as u32 / DISPLAY.width);
        assert_eq!(u, Some((x, y)));
    }
    // and the turn goes through once the allocation does
    p.turn(&mut gl, Orientation::Right).unwrap();
    assert_eq!(p.orientation(), Orientation::Right);
}
