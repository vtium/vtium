//! The GPU path's adapter, with the `gpu` feature (ADR-1303 D7):
//! vtium-render's `Gl` trait over vtium-platform's safe GL type.
//!
//! vtium-render draws through its `Gl` trait and vtium-platform holds the
//! GL context, and neither crate depends on the other, so the binary joins
//! them here. [`PlatformGl`] routes every call through a [`Ledger`], the
//! same checks vtium-render's stand-in GL makes, before the call reaches
//! the context, so a handle that was freed or that another context made,
//! a size past the GPU's limits or an input the program lacks is refused
//! here as it is in the renderer's tests. vtium-platform's context checks
//! on its own what keeps its unsafe calls sound.
//!
//! The adapter also keeps each program's uniforms where the driver put
//! them, so a draw sets its inputs without asking the driver each time. A
//! uniform the driver dropped, because nothing reads it, has no place, and
//! a draw sets it as a no-op.
//!
//! Every error the context reports passes through one place,
//! `PlatformGl::fail`. A lost context and an out-of-memory error both mark
//! the ledger's context lost there, so every later call is refused before
//! it reaches GL (ADR-1303 D9).

mod flap;
pub mod head;

use std::collections::BTreeMap;

use vtium_platform::display::{DmaBuf, Rect as PlatformRect};
use vtium_platform::gpu as platform;
use vtium_render::Rect;
use vtium_render::gpu::{
    Batch, Blend, Draw, DrawTarget, Error, ErrorKind, Fence, Filter, Format, Gl, Ledger, Limits,
    Program, Resolved, Size, Source, Texture,
};

/// vtium-render's `Gl` over a vtium-platform context.
#[derive(Debug)]
pub struct PlatformGl {
    gl: platform::Context,
    ledger: Ledger,
    /// Each program's inputs, by the program's GL name, with the place the
    /// driver gave each.
    uniforms: BTreeMap<u32, Vec<(&'static str, Option<platform::Location>)>>,
}

/// The kind of a platform error, as vtium-render sorts it.
fn kind(k: platform::ErrorKind) -> ErrorKind {
    match k {
        platform::ErrorKind::OutOfMemory => ErrorKind::OutOfMemory,
        platform::ErrorKind::ContextLost => ErrorKind::ContextLost,
        platform::ErrorKind::Gl(_) => ErrorKind::Gl,
        platform::ErrorKind::Compile => ErrorKind::Compile,
        platform::ErrorKind::Import => ErrorKind::Import,
        platform::ErrorKind::Misuse => ErrorKind::Misuse,
    }
}

/// A platform error met inside one of the ledger's calls. The ledger is
/// borrowed while its closure runs, so the closure keeps the error here and
/// gives the ledger a copy, and the adapter passes the kept error to
/// [`PlatformGl::fail`] once the ledger's call has returned.
#[derive(Default)]
struct Met(Option<platform::Error>);

impl Met {
    fn keep(&mut self, e: platform::Error) -> Error {
        let copy = Error::new(kind(e.kind), e.detail.clone());
        self.0 = Some(e);
        copy
    }
}

fn rect(r: Rect) -> PlatformRect {
    PlatformRect {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    }
}

fn format(f: Format) -> platform::TextureFormat {
    match f {
        Format::R8 => platform::TextureFormat::R8,
        Format::Rgba8 => platform::TextureFormat::Rgba8,
    }
}

fn filter(f: Filter) -> platform::Filter {
    match f {
        Filter::Nearest => platform::Filter::Nearest,
        Filter::Linear => platform::Filter::Linear,
    }
}

impl PlatformGl {
    /// The adapter over `gl`, with a new ledger of its limits.
    pub fn new(gl: platform::Context) -> PlatformGl {
        let l = gl.limits();
        let limits = Limits {
            texture: l.texture,
            renderbuffer: l.renderbuffer,
            viewport: Size::new(l.viewport.0, l.viewport.1),
            fragment_highp: l.fragment_highp,
        };
        PlatformGl {
            gl,
            ledger: Ledger::new(limits),
            uniforms: BTreeMap::new(),
        }
    }

    /// Makes the context fail its next call with `kind`, as a driver would:
    /// for the tests of what a head does with each kind of failure.
    pub(crate) fn raise(&mut self, kind: platform::ErrorKind) {
        self.gl.raise(kind);
    }

    /// How many textures, targets and programs the context holds, for the
    /// tests of what a head gives back at a VT release.
    pub(crate) fn live(&self) -> (usize, usize, usize) {
        self.ledger.live()
    }

    /// The context, for its renderer's name.
    pub fn context(&self) -> &platform::Context {
        &self.gl
    }

    /// Every error the context reports passes here, to become vtium-render's.
    /// A lost context and an out-of-memory error mark the ledger's context
    /// lost, so every later call fails with [`ErrorKind::ContextLost`] and
    /// reaches no GL: after `GL_OUT_OF_MEMORY` GL's state is undefined, and
    /// the head leaves the GPU path for either (ADR-1303 D9).
    fn fail(&mut self, e: platform::Error) -> Error {
        let error = Error::new(kind(e.kind), e.detail);
        if matches!(error.kind, ErrorKind::ContextLost | ErrorKind::OutOfMemory) {
            self.ledger.lose();
        }
        error
    }

    /// The result of one of the ledger's calls, whose closure kept in `met`
    /// any error the context reported.
    fn settle<T>(&mut self, result: Result<T, Error>, met: Met) -> Result<T, Error> {
        match met.0 {
            Some(e) => Err(self.fail(e)),
            None => result,
        }
    }
}

impl Gl for PlatformGl {
    type Buffer = DmaBuf;

    fn limits(&self) -> Limits {
        self.ledger.limits()
    }

    fn create_texture(&mut self, f: Format, size: Size) -> Result<Texture, Error> {
        let (gl, mut met) = (&mut self.gl, Met::default());
        let made = self.ledger.create_texture(f, size, || {
            gl.create_texture(format(f), size.width, size.height)
                .map_err(|e| met.keep(e))
        });
        self.settle(made, met)
    }

    fn fill_texture(
        &mut self,
        texture: &Texture,
        r: Rect,
        pixels: &[u8],
        stride: usize,
    ) -> Result<(), Error> {
        let (gl, mut met) = (&mut self.gl, Met::default());
        let filled = self
            .ledger
            .fill_texture(texture, r, pixels.len(), stride, |info| {
                gl.fill_texture(info.name, rect(r), pixels, stride)
                    .map_err(|e| met.keep(e))
            });
        self.settle(filled, met)
    }

    fn free_texture(&mut self, texture: Texture) -> Result<(), Error> {
        let (gl, mut met) = (&mut self.gl, Met::default());
        let freed = self.ledger.free_texture(texture, |info| {
            gl.free_texture(info.name).map_err(|e| met.keep(e))
        });
        self.settle(freed, met)
    }

    fn create_frame(&mut self, size: Size) -> Result<DrawTarget, Error> {
        let (gl, mut met) = (&mut self.gl, Met::default());
        let made = self.ledger.create_frame(size, || {
            gl.create_frame(size.width, size.height)
                .map_err(|e| met.keep(e))
        });
        self.settle(made, met)
    }

    fn import_buffer(&mut self, buffer: &DmaBuf) -> Result<DrawTarget, Error> {
        let (width, height) = buffer.size();
        let (gl, mut met) = (&mut self.gl, Met::default());
        let made = self.ledger.import_buffer(Size::new(width, height), || {
            gl.import(buffer).map_err(|e| met.keep(e))
        });
        self.settle(made, met)
    }

    fn free_target(&mut self, target: DrawTarget) -> Result<(), Error> {
        let (gl, mut met) = (&mut self.gl, Met::default());
        let freed = self.ledger.free_target(target, |info, texture| {
            match texture {
                Some(t) => gl.free_frame(info.name, t.name),
                None => gl.free_import(info.name),
            }
            .map_err(|e| met.keep(e))
        });
        self.settle(freed, met)
    }

    fn compile(&mut self, source: &Source<'_>) -> Result<Program, Error> {
        let (gl, mut met) = (&mut self.gl, Met::default());
        let uniforms = &mut self.uniforms;
        let made = self.ledger.compile(source, || {
            let program = gl
                .compile(&source.vertex_text(), &source.fragment_text())
                .map_err(|e| met.keep(e))?;
            let mut places = Vec::with_capacity(source.inputs.len());
            for input in source.inputs {
                match gl.uniform_location(program, input.name) {
                    Ok(at) => places.push((input.name, at)),
                    Err(e) => {
                        let _ = gl.free_program(program);
                        return Err(met.keep(e));
                    }
                }
            }
            uniforms.insert(program, places);
            Ok(program)
        });
        self.settle(made, met)
    }

    fn free_program(&mut self, program: Program) -> Result<(), Error> {
        let (gl, mut met) = (&mut self.gl, Met::default());
        let uniforms = &mut self.uniforms;
        let freed = self.ledger.free_program(program, |name| {
            gl.free_program(name).map_err(|e| met.keep(e))?;
            uniforms.remove(&name);
            Ok(())
        });
        self.settle(freed, met)
    }

    fn clear(&mut self, target: &DrawTarget, r: Rect, color: u32) -> Result<(), Error> {
        let info = self.ledger.target(target)?;
        self.gl
            .clear(
                info.name,
                (info.size.width, info.size.height),
                rect(r),
                color,
            )
            .map_err(|e| self.fail(e))
    }

    fn draw(&mut self, draw: &Draw<'_>) -> Result<(), Error> {
        let checked = self.ledger.draw(draw)?;
        let places = self.uniforms.get(&checked.program);
        let place = |name: &str| {
            places
                .and_then(|p| p.iter().find(|(n, _)| *n == name))
                .and_then(|(_, at)| *at)
        };
        let uniforms: Vec<(Option<platform::Location>, platform::Uniform)> = checked
            .inputs
            .iter()
            .map(|&(name, value)| {
                let value = match value {
                    Resolved::Texture(t, f) => platform::Uniform::Texture(t, filter(f)),
                    Resolved::Float(v) => platform::Uniform::Float(v),
                    Resolved::Vec2(v) => platform::Uniform::Vec2(v),
                    Resolved::Vec4(v) => platform::Uniform::Vec4(v),
                    Resolved::IVec4(v) => platform::Uniform::IVec4(v),
                    Resolved::UVec4(v) => platform::Uniform::UVec4(v),
                };
                (place(name), value)
            })
            .collect();
        let instances: Vec<platform::Instance> = draw
            .quads
            .iter()
            .map(|q| platform::Instance {
                dst: q.dst,
                src: q.src,
                params: q.params,
            })
            .collect();
        let size = checked.target.size;
        self.gl
            .draw(&platform::DrawCall {
                framebuffer: checked.target.name,
                size: (size.width, size.height),
                program: checked.program,
                uniforms: &uniforms,
                instances: &instances,
                blend: match draw.blend {
                    Blend::Replace => platform::Blend::Replace,
                    Blend::Over => platform::Blend::Over,
                },
                scissor: draw.scissor.map(rect),
            })
            .map_err(|e| self.fail(e))
    }

    /// An EGL fence sync after the frame's commands, or `glFinish` where
    /// the context makes none (vtium-platform's `Context::submit`).
    fn submit(&mut self, target: &DrawTarget) -> Result<Fence, Error> {
        let (gl, mut met) = (&mut self.gl, Met::default());
        let made = self
            .ledger
            .submit(target, |_| gl.submit().map_err(|e| met.keep(e)));
        self.settle(made, met)
    }

    /// `eglClientWaitSync` on the frame's fence, before the render thread
    /// gives the buffer back to the display.
    fn wait(&mut self, fence: Fence) -> Result<(), Error> {
        let (gl, mut met) = (&mut self.gl, Met::default());
        let waited = self
            .ledger
            .wait(fence, |name| gl.wait(name).map_err(|e| met.keep(e)));
        self.settle(waited, met)
    }

    fn check(&mut self, batch: Batch) -> Result<(), Error> {
        self.ledger.alive()?;
        self.gl.check().map_err(|e| Error {
            batch: Some(batch),
            ..self.fail(e)
        })
    }

    fn read(&mut self, target: &DrawTarget, r: Rect, out: &mut [u8]) -> Result<(), Error> {
        let info = self.ledger.read(target, r, out.len())?;
        self.gl
            .read(info.name, rect(r), out)
            .map_err(|e| self.fail(e))
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    //! The adapter on Mesa's llvmpipe, through EGL's software device. Where
    //! llvmpipe or Mesa's EGL is missing, each test fails and names the
    //! packages; none passes without having drawn.

    use vtium_render::gpu::{
        Color, Config, Input, InputKind, Pipeline, Quad, Spatial, Value, Work,
    };

    use super::*;

    fn software() -> PlatformGl {
        match platform::Context::software() {
            Ok(gl) => {
                assert!(gl.renderer().contains("llvmpipe"), "{}", gl.renderer());
                PlatformGl::new(gl)
            }
            Err(e) => panic!(
                "{e}. The GPU path's tests draw on Mesa's llvmpipe through EGL: install \
                 libegl1, libegl-mesa0 and libgl1-mesa-dri (Debian, Ubuntu) or mesa (Arch)"
            ),
        }
    }

    fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// Each configuration of the pipeline's slots, so that every program
    /// vtium-render builds is compiled: present's, with each color and
    /// spatial step joined in. A step added to `Color` or `Spatial` is
    /// added here.
    fn configs() -> Vec<Config> {
        let colors = [Color::None];
        let spatials = [Spatial::None];
        colors
            .iter()
            .flat_map(|&color| {
                spatials
                    .iter()
                    .map(move |&spatial| Config { color, spatial })
            })
            .collect()
    }

    #[test]
    fn every_program_vtium_render_builds_compiles_and_links_on_llvmpipe() {
        let mut gl = software();
        for config in configs() {
            let pipeline = Pipeline::new(&mut gl, Size::new(64, 40), config)
                .unwrap_or_else(|e| panic!("{config:?}: {e}"));
            pipeline.free(&mut gl).unwrap();
        }
        assert_eq!(gl.ledger.live(), (0, 0, 0));
        gl.check(Batch::Uploads).unwrap();
    }

    /// A frame through part 1's pipeline, presented into an off-screen
    /// target and read back, as tier 2 sets the GPU path up: no device, no
    /// GBM, no dma-buf.
    #[test]
    fn a_frame_through_the_pipeline_presents_the_border_on_llvmpipe() {
        let mut gl = software();
        let size = Size::new(64, 40);
        let mut pipeline = Pipeline::new(&mut gl, size, Config::default()).unwrap();
        let target = gl.create_frame(size).unwrap();
        let mut out = vec![0u8; 64 * 40 * 4];
        for border in [0x20_4080, 0xC0_FFEE] {
            let work = Work::new(border, &[], &[]);
            let damage = pipeline.frame(&mut gl, &work, &target, 0).unwrap();
            assert_eq!(damage, [size.rect()]);
            gl.read(&target, size.rect(), &mut out).unwrap();
            let [_, r, g, b] = border.to_be_bytes();
            assert!(
                out.chunks(4).all(|p| p == [r, g, b, 255]),
                "border {border:06x}: {:?}",
                &out[..8]
            );
        }
        // each frame waited on its fence before it returned, through the
        // context
        assert_eq!((gl.ledger.fences(), gl.gl.pending_fences()), (0, 0));
        // nothing composed into a buffer that lacks nothing: no draw
        let work = Work::new(0xC0_FFEE, &[], &[]);
        assert_eq!(pipeline.frame(&mut gl, &work, &target, 1).unwrap(), []);
        gl.free_target(target).unwrap();
        pipeline.free(&mut gl).unwrap();
        gl.check(Batch::Frame).unwrap();
    }

    /// Two textures of one draw, each read through the uniform the draw
    /// names, with a uniform the driver drops set as a no-op, and quads at
    /// the contract's attribute locations, in rows from the first in
    /// memory.
    #[test]
    fn a_draws_inputs_and_quads_reach_the_program_through_the_adapter() {
        const VERT: &str = "#version 300 es
precision highp float;
precision highp int;
layout(location = 0) in ivec4 quad_dst;
layout(location = 1) in uvec4 quad_src;
layout(location = 2) in uvec4 quad_params;
uniform vec2 target_size;
out vec2 src_at;
flat out uvec4 params;
void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 at = vec2(quad_dst.xy) + corner * vec2(quad_dst.zw);
    gl_Position = vec4(at / target_size * 2.0 - 1.0, 0.0, 1.0);
    src_at = vec2(quad_src.xy) + corner * vec2(quad_src.zw);
    params = quad_params;
}
";
        const FRAG: &str = "#version 300 es
precision highp float;
precision highp int;
uniform highp sampler2D reds;
uniform highp sampler2D greens;
uniform float dropped;
in vec2 src_at;
flat in uvec4 params;
out vec4 color;
void main() {
    ivec2 at = ivec2(src_at);
    float blue = float(params.x) / 255.0;
    color = vec4(texelFetch(reds, at, 0).r, texelFetch(greens, at, 0).r, blue, 1.0);
}
";
        let mut gl = software();
        let inputs = [
            Input {
                name: "target_size",
                kind: InputKind::Vec2,
            },
            Input {
                name: "reds",
                kind: InputKind::Texture,
            },
            Input {
                name: "greens",
                kind: InputKind::Texture,
            },
            Input {
                name: "dropped",
                kind: InputKind::Float,
            },
        ];
        let program = gl
            .compile(&Source {
                vertex: &[VERT],
                fragment: &[FRAG],
                inputs: &inputs,
            })
            .unwrap();
        let reds = gl.create_texture(Format::R8, Size::new(2, 2)).unwrap();
        gl.fill_texture(&reds, rect(0, 0, 2, 2), &[10, 20, 30, 40], 2)
            .unwrap();
        let greens = gl.create_texture(Format::R8, Size::new(2, 2)).unwrap();
        gl.fill_texture(&greens, rect(0, 0, 2, 2), &[50, 60, 70, 80], 2)
            .unwrap();
        let target = gl.create_frame(Size::new(2, 2)).unwrap();
        // the top row from one quad, the bottom from another, each with its
        // own blue
        let quads = [
            Quad {
                dst: [0, 0, 2, 1],
                src: [0, 0, 2, 1],
                params: [100, 0, 0, 0],
            },
            Quad {
                dst: [0, 1, 2, 1],
                src: [0, 1, 2, 1],
                params: [200, 0, 0, 0],
            },
        ];
        gl.draw(&Draw {
            target: &target,
            program: &program,
            inputs: &[
                ("greens", Value::Texture(&greens, Filter::Nearest)),
                ("target_size", Value::Vec2([2.0, 2.0])),
                ("reds", Value::Texture(&reds, Filter::Nearest)),
                ("dropped", Value::Float(1.0)),
            ],
            quads: &quads,
            blend: Blend::Replace,
            scissor: None,
        })
        .unwrap();
        gl.check(Batch::Draws).unwrap();
        let mut out = [0u8; 16];
        gl.read(&target, rect(0, 0, 2, 2), &mut out).unwrap();
        assert_eq!(
            out,
            [
                10, 50, 100, 255, 20, 60, 100, 255, 30, 70, 200, 255, 40, 80, 200, 255
            ]
        );
        gl.free_target(target).unwrap();
        gl.free_texture(reds).unwrap();
        gl.free_texture(greens).unwrap();
        gl.free_program(program).unwrap();
        assert!(gl.uniforms.is_empty());
        gl.check(Batch::Uploads).unwrap();
    }

    /// The pointer's arrow on llvmpipe is the CPU path's, byte for byte,
    /// over cells of many colors and over the border, drawn whole and then
    /// moved into a buffer one frame old (ADR-1303 D1, D3). Its outline is
    /// black at 220 of 255 over each color, where a blend rounds one way
    /// or the other; no tier 2 screen draws the arrow.
    #[test]
    fn the_pointers_arrow_on_llvmpipe_is_the_cpu_paths_byte_for_byte() {
        use vtium_font::{Font, Unifont};
        use vtium_render::{Canvas, Config as RenderConfig, DrawState, Renderer};
        use vtium_term::{Age, Terminal};

        // 10 by 3 cells of 8x16, the grid at 2, 1 inside a border
        let size = Size::new(84, 51);
        let mut term = Terminal::new(10, 3, 0);
        // each cell a background of its own
        let mut bytes = Vec::new();
        for i in 0u32..30 {
            let (r, g, b) = (i * 37 % 256, i * 91 % 256, i * 53 % 256);
            bytes.extend(format!("\x1b[48;2;{r};{g};{b}m ").bytes());
        }
        let mut rest = &bytes[..];
        while !rest.is_empty() {
            let taken = term.input(rest);
            assert!(taken > 0, "the terminal stopped taking input");
            rest = &rest[taken..];
        }
        let mut gl = software();
        let font = || Font::new(Unifont::new(16));
        let mut cpu = Renderer::new(font(), size.width, size.height, RenderConfig::default());
        let mut gpu = Renderer::new(font(), size.width, size.height, RenderConfig::default());
        let mut canvas = Canvas::new(size.width, size.height, 0);
        let mut pipeline = Pipeline::new(&mut gl, size, Config::default()).unwrap();
        let target = gl.create_frame(size).unwrap();
        let mut out = vec![0u8; (size.width * size.height * 4) as usize];
        // over the cells, then reaching past the grid into the border,
        // which draws only what the pointer left and entered
        let mut since = Age::NEVER;
        for (age, pointer) in [(0, (13, 7)), (1, (74, 40))] {
            let state = DrawState {
                background: term.palette().bg(),
                pointer: Some(pointer),
                ..DrawState::default()
            };
            let frame = term.screen().frame(term.palette(), since);
            cpu.draw(&frame, &mut canvas, age, &state);
            gpu.draw_gpu(&mut gl, &mut pipeline, &frame, &target, age, &state, &[])
                .unwrap();
            since = term.screen_mut().checkpoint();
            assert_eq!(gl.gl.pending_fences(), 0, "a frame's fence left unwaited");
            gl.read(&target, size.rect(), &mut out).unwrap();
            let differ: Vec<(u32, u32)> = (0..size.height)
                .flat_map(|y| (0..size.width).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let i = ((y * size.width + x) * 4) as usize;
                    let [r, g, b] = [out[i], out[i + 1], out[i + 2]];
                    let gpu = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
                    canvas.pixel(x, y) != Some(gpu)
                })
                .collect();
            assert!(
                differ.is_empty(),
                "pointer at {pointer:?}: {} pixels differ, the first at {:?}",
                differ.len(),
                differ.first()
            );
        }
        gl.free_target(target).unwrap();
        pipeline.free(&mut gl).unwrap();
        gl.check(Batch::Frame).unwrap();
    }

    #[test]
    fn a_shader_the_driver_refuses_is_a_compile_error_with_its_log() {
        let mut gl = software();
        let e = gl
            .compile(&Source {
                vertex: &["#version 300 es\nvoid main() { gl_Position = vec4(0.0); }\n"],
                fragment: &["#version 300 es\nvoid main() { nonsense; }\n"],
                inputs: &[],
            })
            .unwrap_err();
        assert_eq!(e.kind, ErrorKind::Compile);
        assert!(e.detail.contains("nonsense"), "{e}");
        assert_eq!(gl.ledger.live(), (0, 0, 0));
    }

    /// A lost context or out of memory that the context reports inside one
    /// of the ledger's calls leaves every later call refused, as one from a
    /// check does.
    #[test]
    fn a_lost_context_or_out_of_memory_inside_a_call_fails_every_later_call() {
        for raised in [
            platform::ErrorKind::ContextLost,
            platform::ErrorKind::OutOfMemory,
        ] {
            let mut gl = software();
            gl.gl.raise(raised);
            let e = gl.create_texture(Format::R8, Size::new(1, 1)).unwrap_err();
            assert_eq!(e.kind, kind(raised), "{e}");
            let e = gl.create_frame(Size::new(1, 1)).unwrap_err();
            assert_eq!(e.kind, ErrorKind::ContextLost, "after {raised:?}");
        }
    }

    #[test]
    fn a_gl_error_a_check_finds_names_its_batch() {
        let mut gl = software();
        // past the largest texture, made on the context beside the ledger:
        // GL_INVALID_VALUE, which only a check reads
        let past = gl.limits().texture + 1;
        gl.gl
            .create_texture(platform::TextureFormat::R8, past, 1)
            .unwrap();
        let e = gl.check(Batch::Uploads).unwrap_err();
        assert_eq!(
            (e.kind, e.batch),
            (ErrorKind::Gl, Some(Batch::Uploads)),
            "{e}"
        );
        gl.check(Batch::Frame).unwrap();
    }

    /// A lost context and an out-of-memory error from the context's check
    /// both leave every later call refused, reaching no GL; any other GL
    /// error does not.
    #[test]
    fn a_lost_context_or_out_of_memory_from_check_fails_every_later_call() {
        for raised in [
            platform::ErrorKind::ContextLost,
            platform::ErrorKind::OutOfMemory,
        ] {
            let mut gl = software();
            gl.gl.raise(raised);
            let e = gl.check(Batch::Frame).unwrap_err();
            assert_eq!((e.kind, e.batch), (kind(raised), Some(Batch::Frame)), "{e}");
            let e = gl.create_texture(Format::R8, Size::new(1, 1)).unwrap_err();
            assert_eq!(e.kind, ErrorKind::ContextLost, "after {raised:?}");
            assert_eq!(
                gl.check(Batch::Frame).unwrap_err().kind,
                ErrorKind::ContextLost
            );
        }
        let mut gl = software();
        gl.gl.raise(platform::ErrorKind::Gl(0x0501));
        assert_eq!(gl.check(Batch::Draws).unwrap_err().kind, ErrorKind::Gl);
        gl.create_texture(Format::R8, Size::new(1, 1)).unwrap();
        gl.check(Batch::Uploads).unwrap();
    }

    /// Glyph ink that reaches past its cell (ADR-1306), drawn by the cell
    /// shader on llvmpipe. Each test feeds bytes to a terminal, draws every
    /// frame on the GPU path into a buffer one frame old, reads the display
    /// back, and compares it pixel for pixel with the CPU path's whole
    /// drawing of the same screen. vtium-render's own tests run a model of
    /// the shader, so these are the tests that judge the shader itself.
    #[expect(
        clippy::arithmetic_side_effects,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a test fails by panicking, on small known sizes"
    )]
    mod ink {
        use std::ops::Range;

        use vtium_font::{Cell, Face, Font, Format as FontFormat, Glyph, Style, Width};
        use vtium_render::{
            Budgets, Canvas, Config as RenderConfig, DrawState, Orientation, Renderer,
        };
        use vtium_render::{TestInk, gpu::Pipeline};
        use vtium_term::{Age, Terminal};

        use super::*;

        /// Ink from `x0`, `y0` to `x1`, `y1` from the top left of the
        /// glyph's 8x16 cell, at coverage `a`.
        type Ink = (i32, i32, i32, i32, u8);

        /// The ink each test character draws, a body inside its cell and
        /// ink past it: `A` an accent 3 pixels into the row above, its edges
        /// partly covered; `G` a descender 14 pixels into the row below; `V`
        /// an accent 6 pixels into the row above, where `G`'s descender from
        /// two rows up meets it; `Q` ink in every direction; `x` ink inside
        /// its cell. At a bound of two cells, `>` reaches two cells after
        /// its own, `<` two before it, and `U` the row above and two cells
        /// to each side.
        fn shape(c: char) -> &'static [Ink] {
            const BODY: Ink = (1, 6, 7, 14, 255);
            match c {
                'A' => &[
                    BODY,
                    (2, 4, 6, 6, 100),
                    (3, -3, 5, 0, 255),
                    (5, -3, 6, -2, 128),
                ],
                'G' => &[BODY, (2, 10, 5, 30, 255)],
                'V' => &[BODY, (3, -6, 6, 0, 255)],
                'Q' => &[
                    BODY,
                    (-3, -3, 11, 0, 255),
                    (-3, 16, 11, 19, 255),
                    (-3, 0, 0, 16, 200),
                    (8, 0, 11, 16, 200),
                ],
                'x' => &[BODY, (0, 6, 1, 14, 90), (7, 6, 8, 14, 90)],
                '>' => &[BODY, (6, 10, 24, 13, 200)],
                '<' => &[BODY, (-16, 10, 2, 13, 160)],
                'U' => &[BODY, (-12, -3, 20, 0, 230)],
                _ => &[],
            }
        }

        /// The glyphs whose ink reaches two cells, which no font can draw
        /// while the font crate's bound is one cell: the renderers take
        /// them through vtium-render's test hooks.
        const TWO_CELLS: [char; 3] = ['>', '<', 'U'];

        /// `c`'s coverage at `x`, `y` from its cell's top left.
        fn cov(c: char, x: i32, y: i32) -> u8 {
            shape(c)
                .iter()
                .rev()
                .find(|&&(x0, y0, x1, y1, _)| x >= x0 && x < x1 && y >= y0 && y < y1)
                .map_or(0, |r| r.4)
        }

        /// `c`'s ink box, from its cell's top left, and its pixels.
        fn ink_box(c: char) -> ((i32, i32, i32, i32), Vec<u8>) {
            let (mut l, mut t, mut r, mut b) = (0, 0, 8, 16);
            for &(x0, y0, x1, y1, _) in shape(c) {
                (l, t, r, b) = (l.min(x0), t.min(y0), r.max(x1), b.max(y1));
            }
            let px = (t..b)
                .flat_map(|y| (l..r).map(move |x| cov(c, x, y)))
                .collect();
            ((l, t, r, b), px)
        }

        /// A face in 8x16 cells that draws [`shape`]'s glyphs, those of
        /// [`TWO_CELLS`] inside their cells only.
        struct Face8;

        impl Face for Face8 {
            fn cell(&self) -> Cell {
                Cell {
                    width: 8,
                    height: 16,
                }
            }
            fn size_step(&self) -> u32 {
                1
            }
            fn has_glyph(&self, c: char) -> bool {
                c.is_ascii_graphic() || c == ' '
            }
            fn draw(&self, cluster: &[char], _: Style) -> Option<Glyph> {
                let c = *cluster.first()?;
                let own = (0..16)
                    .flat_map(|y| (0..8).map(move |x| cov(c, x, y)))
                    .collect();
                let g = Glyph::from_pixels(8, 16, FontFormat::Mask, own)?.with_cells(Width::Narrow);
                let ((l, t, r, b), px) = ink_box(c);
                if TWO_CELLS.contains(&c) || (l, t, r, b) == (0, 0, 8, 16) {
                    return Some(g);
                }
                g.with_ink((l, t), ((r - l) as u32, (b - t) as u32), px)
            }
        }

        /// A renderer for a display of `size` turned as `o` says, drawing
        /// at a bound of `side` cells and one row, its cache holding the
        /// glyphs of [`TWO_CELLS`] at a bound of two. A glyph evicted from
        /// a cache is drawn again by the font, which cuts those glyphs at
        /// one cell, so at a bound of two the caches are large enough to
        /// evict none.
        fn renderer(size: Size, o: Orientation, side: u8) -> Renderer<Face8> {
            let budgets = Budgets {
                narrow: 256,
                wide: 64,
                mask_bytes: 1 << 20,
                color_bytes: 1 << 20,
            };
            let config = RenderConfig {
                orientation: o,
                budgets: (side > 1).then_some(budgets),
                ..RenderConfig::default()
            };
            let mut r = Renderer::new(Font::new(Face8), size.width, size.height, config);
            if side > 1 {
                r.set_reach_bound_for_test(side, 1);
                for c in TWO_CELLS {
                    r.keep_glyph_for_test(c, test_ink(c, &ink_box(c)));
                }
            }
            r
        }

        fn test_ink(_: char, ((l, t, r, b), px): &((i32, i32, i32, i32), Vec<u8>)) -> TestInk<'_> {
            TestInk {
                left: *l,
                top: *t,
                width: (r - l) as u32,
                height: (b - t) as u32,
                pixels: px,
            }
        }

        /// One frame: the bytes the terminal reads before it, the columns
        /// selected in each row, and the cursor's blink phase.
        struct Step {
            bytes: String,
            sel: Vec<Range<usize>>,
            cursor_on: bool,
        }

        fn step(bytes: &str) -> Step {
            Step {
                bytes: bytes.to_owned(),
                sel: Vec::new(),
                cursor_on: true,
            }
        }

        const TURNS: [Orientation; 4] = [
            Orientation::Normal,
            Orientation::Right,
            Orientation::UpsideDown,
            Orientation::Left,
        ];
        const HIDE: &str = "\x1b[?25l";

        /// Runs `steps` on a terminal of `cols` by `rows` in 8x16 cells
        /// inside a border, on a display turned as `o` says, at a bound of
        /// `side` cells; fails at the first frame whose display differs
        /// from the CPU path's whole drawing.
        fn run(name: &str, (cols, rows): (usize, usize), o: Orientation, side: u8, steps: &[Step]) {
            let (w, h) = (cols as u32 * 8 + 4, rows as u32 * 16 + 2);
            let size = match o {
                Orientation::Right | Orientation::Left => Size::new(h, w),
                Orientation::Normal | Orientation::UpsideDown => Size::new(w, h),
            };
            let mut term = Terminal::new(cols, rows, 0);
            #[cfg(feature = "images")]
            {
                use vtium_term::images::{Kitty, Switches};
                term.screen_mut().set_image_limit(1 << 20);
                term.screen_mut().set_image_cell(8, 16).unwrap();
                term.set_image_switches(Switches {
                    sixel: true,
                    kitty: true,
                    iterm2: false,
                });
                let kitty = Box::new(Kitty::default());
                term.set_image_handler(vtium_term::ImageString::Kitty, Some(kitty));
            }
            let mut gl = software();
            let mut gpu = renderer(size, o, side);
            let mut pipeline = Pipeline::new(&mut gl, size, Config::default()).unwrap();
            let target = gl.create_frame(size).unwrap();
            let mut since = Age::NEVER;
            let mut age = 0;
            if side > 1 {
                // a first frame makes the pipeline's glyphs for this
                // renderer, which then take the glyphs that reach two cells
                let state = DrawState::default();
                let frame = term.screen().frame(term.palette(), since);
                gpu.draw_gpu(&mut gl, &mut pipeline, &frame, &target, 0, &state, &[])
                    .unwrap();
                since = term.screen_mut().checkpoint();
                age = 1;
                for c in TWO_CELLS {
                    let boxed = ink_box(c);
                    let put =
                        gpu.put_glyph_for_test(&mut gl, &mut pipeline, c, test_ink(c, &boxed));
                    assert!(put.unwrap(), "{c} takes a box");
                }
            }
            let mut out = vec![0u8; (size.width * size.height * 4) as usize];
            let mut cpu = Canvas::new(size.width, size.height, 0);
            for (n, s) in steps.iter().enumerate() {
                let mut rest = s.bytes.as_bytes();
                while !rest.is_empty() {
                    rest = &rest[term.input(rest)..];
                }
                let state = DrawState {
                    cursor_on: s.cursor_on,
                    background: term.palette().bg(),
                    ..DrawState::default()
                };
                let frame = term
                    .screen()
                    .frame(term.palette(), since)
                    .with_selection(s.sel.clone());
                renderer(size, o, side).draw(&frame, &mut cpu, 0, &state);
                gpu.draw_gpu(&mut gl, &mut pipeline, &frame, &target, age, &state, &[])
                    .unwrap();
                since = term.screen_mut().checkpoint();
                age = 1;
                gl.read(&target, size.rect(), &mut out).unwrap();
                let differ = (0..size.height)
                    .flat_map(|y| (0..size.width).map(move |x| (x, y)))
                    .filter_map(|(x, y)| {
                        let i = ((y * size.width + x) * 4) as usize;
                        let [r, g, b] = [out[i], out[i + 1], out[i + 2]].map(u32::from);
                        let (c, g) = (cpu.pixel(x, y)?, r << 16 | g << 8 | b);
                        (c != g).then_some((x, y, c, g))
                    })
                    .collect::<Vec<_>>();
                assert!(
                    differ.is_empty(),
                    "{name}, {o:?}, frame {n}: {} pixels differ from the CPU path's, the first \
                     (x, y, cpu, gpu) {:x?}",
                    differ.len(),
                    differ.first()
                );
            }
            gl.free_target(target).unwrap();
            pipeline.free(&mut gl).unwrap();
        }

        /// A kitty picture of one opaque cell, blue, at the cursor, at `z`,
        /// that leaves the cursor where it was.
        #[cfg(feature = "images")]
        fn picture(id: u32, z: i64) -> String {
            // 8x16 pixels of 20 60 80 ff, 512 bytes, in base64
            let pixels = format!("{}IGCA/yBggP8=", "IGCA/yBggP8gYID/".repeat(42));
            format!("\x1b_Ga=T,f=32,s=8,v=16,i={id},C=1,q=2,z={z},m=0;{pixels}\x1b\\")
        }

        // ADR-1306 D4: where a descender from the row above meets an accent
        // from the row below, the row below's ink lies on top.
        // covers: text.overhang
        #[test]
        fn a_descender_meeting_an_accent_draws_as_on_the_cpu_path() {
            for o in TURNS {
                let bytes =
                    format!("{HIDE}\x1b[1;3H\x1b[38;2;224;64;64mG\x1b[3;3H\x1b[38;2;64;224;64mV");
                run("descender and accent", (5, 3), o, 1, &[step(&bytes)]);
            }
        }

        // ADR-1306 D6: the letter under a block cursor keeps its ink above
        // and below in each phase of the blink, and the ink its neighbour
        // sends into the cursor's cell takes the cursor's text color.
        // covers: text.overhang
        #[test]
        fn ink_around_a_block_cursor_draws_as_on_the_cpu_path_in_each_phase() {
            let base = "\x1b[2;1H\x1b[38;2;224;192;64m\x1b[48;2;48;32;32mxQQx\x1b[1;2H\x1b[0mxxx\x1b[3;2Hxxx\x1b[2;2H";
            for o in TURNS {
                let mut steps = vec![step(base)];
                for cursor_on in [false, true, false] {
                    steps.push(Step {
                        cursor_on,
                        ..step("")
                    });
                }
                run("block cursor", (5, 3), o, 1, &steps);
            }
        }

        // ADR-1306 D6: the selection takes the ink inside it, and a selected
        // letter's ink outside the selection keeps its own colors.
        // covers: text.overhang
        #[test]
        fn ink_in_and_around_the_selection_draws_as_on_the_cpu_path() {
            let base = format!(
                "{HIDE}\x1b[2;1H\x1b[38;2;64;192;64m\x1b[48;2;32;64;192mxxxxx\x1b[0m\
                 \x1b[3;2H\x1b[38;2;240;240;240mAxA\x1b[1;3H\x1b[38;2;224;160;32mQ"
            );
            let sels = [
                vec![0..0, 0..0, 0..5],
                vec![0..0, 0..5],
                vec![0..0, 1..2, 3..4],
                vec![0..0, 3..4],
            ];
            for o in TURNS {
                let mut steps = vec![step(&base)];
                for sel in &sels {
                    steps.push(Step {
                        sel: sel.clone(),
                        ..step(&format!("\x1b[2J{base}"))
                    });
                }
                run("selection", (5, 3), o, 1, &steps);
            }
        }

        // ADR-1306 D6: under reverse video, ink takes the reversed
        // foreground.
        // covers: text.overhang
        #[test]
        fn ink_of_reverse_video_draws_as_on_the_cpu_path() {
            let bytes =
                format!("{HIDE}\x1b[1;1H\x1b[48;2;32;64;160mxxxx\x1b[0m\x1b[2;1H\x1b[7mAQA\x1b[0m");
            for o in TURNS {
                run("reverse video", (4, 3), o, 1, &[step(&bytes)]);
            }
        }

        // ADR-1306 D4, D5: a cell the frame draws twice, once with its row
        // and again once the row below has sent it ink, shows the second
        // drawing.
        // covers: text.overhang
        #[test]
        fn a_cell_drawn_twice_in_a_frame_draws_as_on_the_cpu_path() {
            for o in TURNS {
                run(
                    "drawn twice",
                    (3, 4),
                    o,
                    1,
                    &[
                        step(&format!("{HIDE}\x1b[1;1Hx\x1b[2;1HA\x1b[3;1Hx")),
                        step(
                            "\x1b[1;1H\x1b[38;2;224;64;64mx\x1b[3;1H\x1b[38;2;64;224;64mx\x1b[0m\x1b[4;1HA",
                        ),
                    ],
                );
            }
        }

        // ADR-1306 D7: ink that reaches a picture's cell is part of that
        // cell's text layer, over a picture below the text and under one
        // above it, also when the frame draws the cell twice.
        // covers: text.overhang
        #[cfg(feature = "images")]
        #[test]
        fn ink_reaching_a_pictures_cell_draws_as_on_the_cpu_path() {
            for z in [-1_100_000_000, -5, 0, 3] {
                let into = [
                    step(&format!(
                        "{HIDE}\x1b[1;1H\x1b[44mxxx\x1b[0m\x1b[1;2H{}\x1b[2;1H\
                         \x1b[38;2;240;200;40mAAQ\x1b[1;2H\x1b[38;2;200;40;40mx",
                        picture(9, z)
                    )),
                    step("\x1b[2;2H\x1b[38;2;40;240;40mV"),
                ];
                let twice = [
                    step(&format!(
                        "{HIDE}\x1b[1;1Hx\x1b[2;1HA\x1b[3;1Hx{}",
                        picture(7, z)
                    )),
                    step(
                        "\x1b[1;1H\x1b[38;2;224;64;64mx\x1b[3;1H\x1b[38;2;64;224;64mx\x1b[0m\x1b[4;1HA",
                    ),
                ];
                for o in TURNS {
                    run(&format!("ink into a picture at z {z}"), (4, 3), o, 1, &into);
                    run(
                        &format!("drawn twice under a picture at z {z}"),
                        (3, 4),
                        o,
                        1,
                        &twice,
                    );
                }
            }
        }

        /// A script that types and erases reaching letters, in colors of
        /// their own, at random places on a terminal of `cols` by `rows`.
        fn typing(
            seed: u32,
            (cols, rows): (u32, u32),
            letters: &[char],
            frames: usize,
        ) -> Vec<Step> {
            let mut seed = seed;
            let mut rand = move |n: u32| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed % n
            };
            let mut steps = vec![step(HIDE)];
            for _ in 0..frames {
                let mut bytes = String::new();
                for _ in 0..=rand(3) {
                    let c = letters[rand(letters.len() as u32) as usize];
                    let (row, col, r, g) = (rand(rows) + 1, rand(cols) + 1, rand(256), rand(64));
                    bytes +=
                        &format!("\x1b[{row};{col}H\x1b[38;2;{r};120;220m\x1b[48;2;16;{g};16m{c}");
                }
                steps.push(step(&bytes));
            }
            steps
        }

        // ADR-1306 D5, D9: every frame of typing and erasing reaching
        // letters, at each turn of the display.
        // covers: text.overhang
        #[test]
        fn every_frame_of_typing_draws_as_on_the_cpu_path() {
            let steps = typing(0x2545_f491, (8, 4), &['A', 'G', 'V', 'Q', 'x', ' '], 60);
            for o in TURNS {
                run("typing", (8, 4), o, 1, &steps);
            }
        }

        // ADR-1306 D12: at a bound of two cells, the cell shader draws the
        // ink from beside a cell farthest first, and every frame of typing
        // draws as on the CPU path.
        // covers: text.overhang
        #[test]
        fn every_frame_at_a_bound_of_two_cells_draws_as_on_the_cpu_path() {
            let meet = format!(
                "{HIDE}\x1b[1;2H\x1b[38;2;250;0;0m>\x1b[1;3H\x1b[38;2;0;250;0m>\
                 \x1b[1;5H\x1b[38;2;0;0;250m<\x1b[1;6H\x1b[38;2;250;250;0m<"
            );
            let letters = ['>', '<', 'U', 'A', 'V', 'G', 'x', ' '];
            let mut steps = vec![step(&meet)];
            steps.extend(typing(0x1234_5679, (10, 3), &letters, 40));
            for o in TURNS {
                run("bound two", (10, 3), o, 2, &steps);
            }
        }
    }
}
