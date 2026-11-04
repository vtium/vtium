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

use std::collections::BTreeMap;

use vtium_platform::display::{DmaBuf, Rect as PlatformRect};
use vtium_platform::gpu as platform;
use vtium_render::Rect;
use vtium_render::gpu::{
    Batch, Blend, Draw, DrawTarget, Error, ErrorKind, Filter, Format, Gl, Ledger, Limits, Program,
    Resolved, Size, Source, Texture,
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

    /// `glFlush`. The display shows the buffer through its dma-buf, and the
    /// kernel waits on the dma-buf's implicit fence before it scans the
    /// buffer out, as it waited for the C reference's `eglSwapBuffers`.
    fn submit(&mut self, target: &DrawTarget) -> Result<(), Error> {
        self.ledger.target(target)?;
        self.gl.submit().map_err(|e| self.fail(e))
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
}
