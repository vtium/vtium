//! Draws fragment shaders over a terminal-like frame off-screen and times
//! each frame on the GPU.
//!
//! A frame's time comes from two clocks. Where the driver has
//! `GL_EXT_disjoint_timer_query`, a `GL_TIME_ELAPSED_EXT` query around the
//! frame's draws gives the GPU's own time. On every driver, the CPU's clock
//! is read around the draws with `glFinish` on both sides, so the time
//! covers the GPU finishing the work, not only the driver taking the
//! commands. Without the second `glFinish`, a GPU that queues work would
//! report only the time to submit it. The harness reports both clocks and
//! judges the rate by the CPU's, because llvmpipe's timer queries report
//! only part of a heavy frame.
//!
//! References:
//! - ADR-1304 D16: the measurement times each frame through timer queries
//!   where the driver has them, and otherwise through `glFinish` around each
//!   frame, and takes the median frame.

use std::time::Instant;

use glow::HasContext;

use crate::device::Device;
use crate::source::VERTEX;

/// `GL_TIME_ELAPSED_EXT`.
const TIME_ELAPSED_EXT: u32 = 0x88BF;
/// `GL_GPU_DISJOINT_EXT`.
const GPU_DISJOINT_EXT: u32 = 0x8FBB;
/// The terminal-like frame's character cell, in pixels.
const CELL: (u32, u32) = (10, 20);

/// One frame's time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    /// The GPU's time for the frame's draws, from the timer query, in
    /// nanoseconds; `None` without timer queries or when the GPU reported
    /// a disjoint operation during the frame.
    pub gpu_ns: Option<u64>,
    /// The CPU's clock around the draws, `glFinish` on both sides.
    pub wall_ns: u64,
}

/// Where a pass reads and writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// The terminal-like frame, or the final target.
    Frame,
    /// The texture between two passes.
    Between,
}

/// One draw of a frame.
pub struct Pass<'p> {
    pub program: &'p Program,
    pub input: Surface,
    pub output: Surface,
    /// Whether the input is sampled with GL_LINEAR.
    pub linear: bool,
}

/// A linked program and its uniforms' locations.
pub struct Program {
    raw: glow::NativeProgram,
    resolution: Option<glow::NativeUniformLocation>,
    channel_resolution: Option<glow::NativeUniformLocation>,
    time: Option<glow::NativeUniformLocation>,
    delta: Option<glow::NativeUniformLocation>,
    rate: Option<glow::NativeUniformLocation>,
    frame: Option<glow::NativeUniformLocation>,
    channel: Option<glow::NativeUniformLocation>,
    reach: Option<glow::NativeUniformLocation>,
}

/// The frame, the target and the texture between two passes, at one size.
pub struct Bench<'d> {
    device: &'d Device,
    pub width: u32,
    pub height: u32,
    vao: glow::NativeVertexArray,
    frame: glow::NativeTexture,
    target: (glow::NativeTexture, glow::NativeFramebuffer),
    between: (glow::NativeTexture, glow::NativeFramebuffer),
    query: Option<glow::NativeQuery>,
}

impl Drop for Bench<'_> {
    fn drop(&mut self) {
        let gl = &self.device.gl;
        // SAFETY: each name is this context's own and is deleted once.
        unsafe {
            gl.delete_vertex_array(self.vao);
            gl.delete_texture(self.frame);
            for (texture, framebuffer) in [self.target, self.between] {
                gl.delete_framebuffer(framebuffer);
                gl.delete_texture(texture);
            }
            if let Some(q) = self.query {
                gl.delete_query(q);
            }
        }
    }
}

/// A small hash, for the frame's made-up text.
fn hash(x: u32, y: u32, seed: u32) -> u32 {
    let mut h = x
        .wrapping_mul(0x8da6_b343)
        .wrapping_add(y.wrapping_mul(0xd816_3841))
        .wrapping_add(seed.wrapping_mul(0xcb1a_b31f));
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h
}

/// A frame that looks like a terminal: a dark background, and most cells
/// holding a made-up glyph of lit pixels in one of a few colors.
pub fn terminal_frame(width: u32, height: u32) -> Vec<u8> {
    const BACKGROUND: [u8; 3] = [16, 20, 24];
    const INKS: [[u8; 3]; 5] = [
        [220, 220, 210],
        [120, 200, 120],
        [230, 180, 80],
        [110, 160, 240],
        [230, 100, 100],
    ];
    let mut out = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let (cx, cy) = (x / CELL.0, y / CELL.1);
            let (px, py) = (x % CELL.0, y % CELL.1);
            let cell = hash(cx, cy, 1);
            let inked = cell % 10 < 6 && (1..9).contains(&px) && (3..17).contains(&py);
            let lit = inked && hash(px / 2 + cell % 7, py / 2, cell).is_multiple_of(3);
            let rgb = if lit {
                INKS[(cell >> 8) as usize % INKS.len()]
            } else {
                BACKGROUND
            };
            out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    out
}

fn gl_error(gl: &glow::Context, what: &str) -> Result<(), String> {
    // SAFETY: glGetError on the current context.
    let e = unsafe { gl.get_error() };
    if e == glow::NO_ERROR {
        Ok(())
    } else {
        Err(format!("{what}: GL error {e:#06x}"))
    }
}

impl<'d> Bench<'d> {
    /// Makes the frame, the target and the texture between two passes at
    /// `width` × `height`.
    pub fn new(device: &'d Device, width: u32, height: u32) -> Result<Bench<'d>, String> {
        let gl = &device.gl;
        let (w, h) = (
            i32::try_from(width).map_err(|e| e.to_string())?,
            i32::try_from(height).map_err(|e| e.to_string())?,
        );
        let pixels = terminal_frame(width, height);
        // SAFETY: every call is on the current context; the upload reads
        // exactly width × height × 4 bytes, which `pixels` holds, with the
        // unpack alignment at 1 and no buffer bound for unpacking.
        let bench = unsafe {
            let texture = |data: Option<&[u8]>| -> Result<glow::NativeTexture, String> {
                let t = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w,
                    h,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(data),
                );
                for (key, value) in [
                    (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                    (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                    (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                ] {
                    gl.tex_parameter_i32(glow::TEXTURE_2D, key, value as i32);
                }
                Ok(t)
            };
            let framebuffer = |t: glow::NativeTexture| -> Result<glow::NativeFramebuffer, String> {
                let f = gl.create_framebuffer()?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(f));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(t),
                    0,
                );
                let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
                if status != glow::FRAMEBUFFER_COMPLETE {
                    return Err(format!("the framebuffer is incomplete: {status:#06x}"));
                }
                Ok(f)
            };
            let frame = texture(Some(&pixels))?;
            let target_texture = texture(None)?;
            let target = (target_texture, framebuffer(target_texture)?);
            let between_texture = texture(None)?;
            let between = (between_texture, framebuffer(between_texture)?);
            let query = if gl
                .supported_extensions()
                .contains("GL_EXT_disjoint_timer_query")
            {
                Some(gl.create_query()?)
            } else {
                None
            };
            Bench {
                device,
                width,
                height,
                vao: gl.create_vertex_array()?,
                frame,
                target,
                between,
                query,
            }
        };
        gl_error(gl, "making the frame and its targets")?;
        Ok(bench)
    }

    /// Whether the GPU's own clock times each frame.
    pub fn timer_queries(&self) -> bool {
        self.query.is_some()
    }

    /// Compiles and links a fragment shader with the full-target vertex
    /// shader.
    pub fn program(&self, fragment: &str) -> Result<Program, String> {
        let gl = &self.device.gl;
        // SAFETY: shader and program calls on the current context, each
        // name made here and deleted here or kept in the program.
        unsafe {
            let compile = |kind: u32, text: &str| -> Result<glow::NativeShader, String> {
                let s = gl.create_shader(kind)?;
                gl.shader_source(s, text);
                gl.compile_shader(s);
                if gl.get_shader_compile_status(s) {
                    Ok(s)
                } else {
                    let log = gl.get_shader_info_log(s);
                    gl.delete_shader(s);
                    Err(format!("the shader does not compile: {}", log.trim()))
                }
            };
            let vertex = compile(glow::VERTEX_SHADER, VERTEX)?;
            let fragment = match compile(glow::FRAGMENT_SHADER, fragment) {
                Ok(f) => f,
                Err(e) => {
                    gl.delete_shader(vertex);
                    return Err(e);
                }
            };
            let raw = gl.create_program()?;
            gl.attach_shader(raw, vertex);
            gl.attach_shader(raw, fragment);
            gl.link_program(raw);
            gl.delete_shader(vertex);
            gl.delete_shader(fragment);
            if !gl.get_program_link_status(raw) {
                let log = gl.get_program_info_log(raw);
                gl.delete_program(raw);
                return Err(format!("the program does not link: {}", log.trim()));
            }
            let at = |name: &str| gl.get_uniform_location(raw, name);
            Ok(Program {
                raw,
                resolution: at("iResolution"),
                channel_resolution: at("iChannelResolution"),
                time: at("iTime"),
                delta: at("iTimeDelta"),
                rate: at("iFrameRate"),
                frame: at("iFrame"),
                channel: at("iChannel0"),
                reach: at("vt_reach"),
            })
        }
    }

    pub fn delete(&self, program: Program) {
        // SAFETY: the program is this context's own, deleted once.
        unsafe { self.device.gl.delete_program(program.raw) };
    }

    /// Draws `warmup` frames, then `frames` frames, and returns the times
    /// of the latter. Frame k sees the time drawing at `refresh` frames a
    /// second from time 0 would give it, never the clock's.
    #[expect(
        clippy::disallowed_methods,
        reason = "the harness measures how long the GPU takes, and the clock decides nothing it draws"
    )]
    pub fn run(
        &self,
        passes: &[Pass],
        warmup: u32,
        frames: u32,
        refresh: f32,
    ) -> Result<Vec<Sample>, String> {
        let gl = &self.device.gl;
        let (w, h) = (self.width as i32, self.height as i32);
        let mut samples = Vec::with_capacity(frames as usize);
        for k in 0..warmup + frames {
            // SAFETY: every call is on the current context, with names this
            // bench and its programs own; the query's results are read only
            // after glFinish, and as one 32-bit value.
            let sample = unsafe {
                for pass in passes {
                    let p = pass.program;
                    gl.use_program(Some(p.raw));
                    gl.uniform_3_f32(p.resolution.as_ref(), w as f32, h as f32, 1.0);
                    gl.uniform_3_f32(p.channel_resolution.as_ref(), w as f32, h as f32, 1.0);
                    gl.uniform_1_f32(p.time.as_ref(), k as f32 / refresh);
                    gl.uniform_1_f32(p.delta.as_ref(), 1.0 / refresh);
                    gl.uniform_1_f32(p.rate.as_ref(), refresh);
                    gl.uniform_1_i32(p.frame.as_ref(), k as i32);
                    gl.uniform_1_i32(p.channel.as_ref(), 0);
                    gl.uniform_1_f32(p.reach.as_ref(), 256.0);
                }
                gl.finish();
                let start = Instant::now();
                if let Some(q) = self.query {
                    // reading the flag clears it, so a disjoint operation
                    // before this frame does not discard the frame
                    gl.get_parameter_i32(GPU_DISJOINT_EXT);
                    gl.begin_query(TIME_ELAPSED_EXT, q);
                }
                for pass in passes {
                    let input = match pass.input {
                        Surface::Frame => self.frame,
                        Surface::Between => self.between.0,
                    };
                    let output = match pass.output {
                        Surface::Frame => self.target.1,
                        Surface::Between => self.between.1,
                    };
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(output));
                    gl.viewport(0, 0, w, h);
                    gl.active_texture(glow::TEXTURE0);
                    gl.bind_texture(glow::TEXTURE_2D, Some(input));
                    let filter = if pass.linear {
                        glow::LINEAR
                    } else {
                        glow::NEAREST
                    } as i32;
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter);
                    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter);
                    gl.use_program(Some(pass.program.raw));
                    gl.bind_vertex_array(Some(self.vao));
                    gl.draw_arrays(glow::TRIANGLES, 0, 3);
                }
                if self.query.is_some() {
                    gl.end_query(TIME_ELAPSED_EXT);
                }
                gl.finish();
                let wall_ns = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
                let gpu_ns = self.query.and_then(|q| {
                    let ns = gl.get_query_parameter_u32(q, glow::QUERY_RESULT);
                    let disjoint = gl.get_parameter_i32(GPU_DISJOINT_EXT) != 0;
                    (!disjoint).then_some(u64::from(ns))
                });
                Sample { gpu_ns, wall_ns }
            };
            gl_error(gl, "drawing a frame")?;
            if k >= warmup {
                samples.push(sample);
            }
        }
        Ok(samples)
    }

    /// The target's pixel at (x, y), counted from the bottom left as GL
    /// counts it.
    pub fn pixel(&self, x: u32, y: u32) -> Result<[u8; 4], String> {
        let gl = &self.device.gl;
        let mut out = [0u8; 4];
        // SAFETY: the read writes one RGBA8 pixel, four bytes, into `out`,
        // with no buffer bound for packing.
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.target.1));
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.read_pixels(
                x as i32,
                y as i32,
                1,
                1,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut out)),
            );
        }
        gl_error(gl, "reading a pixel")?;
        Ok(out)
    }
}

/// The median and the largest of some times, in nanoseconds.
pub fn median_max(values: &mut [u64]) -> Option<(u64, u64)> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let n = values.len();
    let median = if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2
    };
    Some((median, values[n - 1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_of_odd_and_even_counts() {
        assert_eq!(median_max(&mut [5, 1, 3]), Some((3, 5)));
        assert_eq!(median_max(&mut [4, 1, 3, 2]), Some((2, 4)));
        assert_eq!(median_max(&mut []), None);
    }

    #[test]
    fn the_frame_holds_text_on_a_dark_background() {
        let f = terminal_frame(200, 100);
        assert_eq!(f.len(), 200 * 100 * 4);
        let lit = f.chunks(4).filter(|p| p[0] > 100).count();
        // some pixels are ink, and most are background
        assert!(lit > 200 * 100 / 50, "{lit}");
        assert!(lit < 200 * 100 / 2, "{lit}");
    }
}
