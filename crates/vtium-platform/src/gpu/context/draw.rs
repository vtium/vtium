//! A context's draws, clears, submits, waits and reads.
//!
//! A frame ends with a submit, which gives a fence, and a wait on it before
//! the display shows the buffer. The display flips to the buffer through
//! its dma-buf, and the kernel waits on the dma-buf's implicit fence only
//! once the driver has sent it the frame's work; a driver that queues GL
//! calls to a worker thread, as Mesa's glthread does, may return from
//! `glFlush` before then. So the render thread waits for the GPU itself:
//! an EGL fence sync made after the frame's commands and waited on with
//! `eglClientWaitSync`, or `glFinish` where the display or GL makes no
//! fence sync. Passing a fence's descriptor with an atomic commit as the
//! plane's `IN_FENCE_FD` would let the kernel wait instead, but a card
//! without atomic commits flips through the legacy page flip, which takes
//! no fence, and the wait on the render thread serves both and keeps the
//! descriptor out of the buffer's token.
//!
//! A draw is one instanced draw of a strip of four vertices per quad: each
//! quad's `dst` at attribute location 0 as an `ivec4` and its `src` and
//! `params` at 1 and 2 as `uvec4`s, each advancing once an instance. Its
//! textures take units in the order the draw lists them, each sampled with
//! the draw's filter through a sampler object.

use glow::HasContext;

use khronos_egl as egl;

use super::{Context, INSTANCE_BYTES, int, ints, misuse, name};
use crate::display::Rect;
use crate::gpu::{Error, ErrorKind};

/// An EGL error as the context's: out of memory and a lost context keep
/// their kinds, which leave the GPU path (ADR-1303 D9), and any other is
/// an error of its EGL code.
fn egl_error(call: &str, e: egl::Error) -> Error {
    let kind = match e {
        egl::Error::BadAlloc => ErrorKind::OutOfMemory,
        egl::Error::ContextLost => ErrorKind::ContextLost,
        other => ErrorKind::Gl(egl::Int::from(other).unsigned_abs()),
    };
    Error::new(kind, format!("{call}: {e}"))
}

/// How a texture is sampled where a texel does not meet a pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    Linear,
}

/// How a draw's pixels meet what the target holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    /// Blending off: the draw's pixels replace the target's.
    Replace,
    /// Premultiplied over: `GL_ONE`, `GL_ONE_MINUS_SRC_ALPHA`.
    Over,
}

/// Where a program's uniform is, which [`Context::uniform_location`] gives
/// for a uniform the driver kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location(pub(super) u32);

/// A value set on a uniform for one draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Uniform {
    /// A texture, by name, and its filter.
    Texture(u32, Filter),
    Float(f32),
    Vec2([f32; 2]),
    Vec4([f32; 4]),
    IVec4([i32; 4]),
    UVec4([u32; 4]),
}

/// One quad's attributes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Instance {
    pub dst: [i32; 4],
    pub src: [u32; 4],
    pub params: [u32; 4],
}

/// One instanced draw.
#[derive(Clone, Copy, Debug)]
pub struct DrawCall<'a> {
    /// The framebuffer drawn into.
    pub framebuffer: u32,
    /// Its size, the viewport.
    pub size: (u32, u32),
    pub program: u32,
    /// Each uniform's value, at its location; a uniform the driver dropped,
    /// whose location is `None`, is not set.
    pub uniforms: &'a [(Option<Location>, Uniform)],
    pub instances: &'a [Instance],
    pub blend: Blend,
    /// The rectangle the draw is cut to.
    pub scissor: Option<Rect>,
}

impl Context {
    /// Binds `framebuffer` for drawing with a viewport of `size`. Its
    /// callers make the context current first.
    fn target(&self, framebuffer: u32, (width, height): (u32, u32)) -> Result<(), Error> {
        let fb = glow::NativeFramebuffer(name(framebuffer)?);
        let (w, h) = (int(width)?, int(height)?);
        // SAFETY: GL calls with no pointer.
        #[expect(unsafe_code, reason = "glBindFramebuffer and glViewport")]
        unsafe {
            self.gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fb));
            self.gl.viewport(0, 0, w, h);
        }
        Ok(())
    }

    /// Fills `rect` of `framebuffer`, of `size`, with `color`, `0x00RRGGBB`,
    /// opaque.
    pub fn clear(
        &mut self,
        framebuffer: u32,
        size: (u32, u32),
        rect: Rect,
        color: u32,
    ) -> Result<(), Error> {
        let [x, y, w, h] = ints(rect)?;
        self.current()?;
        let [_, r, g, b] = color.to_be_bytes();
        let channel = |c: u8| f32::from(c) / 255.0;
        // SAFETY: GL calls on the current context with no pointer.
        #[expect(unsafe_code, reason = "glClear within a scissor")]
        unsafe {
            self.target(framebuffer, size)?;
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(x, y, w, h);
            self.gl.color_mask(true, true, true, true);
            self.gl.clear_color(channel(r), channel(g), channel(b), 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        Ok(())
    }

    /// One instanced draw: four vertices a quad, in a strip, for each of
    /// `draw.instances`.
    pub fn draw(&mut self, draw: &DrawCall<'_>) -> Result<(), Error> {
        let count = i32::try_from(draw.instances.len()).map_err(|_| misuse("too many quads"))?;
        if count == 0 {
            return Ok(());
        }
        let textures = draw
            .uniforms
            .iter()
            .filter(|(_, u)| matches!(u, Uniform::Texture(..)))
            .count();
        if u32::try_from(textures).map_or(true, |n| n > self.limits.texture_units) {
            return Err(misuse(format!(
                "a draw of {textures} textures passes the GPU's {} units",
                self.limits.texture_units
            )));
        }
        let program = glow::NativeProgram(name(draw.program)?);
        let scissor = draw.scissor.map(ints).transpose()?;
        let mut bytes = Vec::with_capacity(draw.instances.len().saturating_mul(INSTANCE_BYTES));
        for q in draw.instances {
            for v in q.dst {
                bytes.extend(v.to_ne_bytes());
            }
            for v in q.src.iter().chain(&q.params) {
                bytes.extend(v.to_ne_bytes());
            }
        }
        self.current()?;
        // SAFETY: GL calls on the current context. The vertex array reads
        // three 16-byte vectors at offsets 0, 16 and 32 of each 48-byte
        // instance of the instance buffer, which holds exactly `count`
        // instances once `bytes` fills it. No other attribute is enabled.
        #[expect(
            unsafe_code,
            reason = "an instanced draw of the instances uploaded here"
        )]
        unsafe {
            let gl = &self.gl;
            self.target(draw.framebuffer, draw.size)?;
            match scissor {
                Some([x, y, w, h]) => {
                    gl.enable(glow::SCISSOR_TEST);
                    gl.scissor(x, y, w, h);
                }
                None => gl.disable(glow::SCISSOR_TEST),
            }
            gl.use_program(Some(program));
            let mut unit = 0u32;
            for &(location, value) in draw.uniforms {
                let at = location.map(|l| glow::NativeUniformLocation(l.0));
                let at = at.as_ref();
                match value {
                    Uniform::Texture(texture, filter) => {
                        let texture = glow::NativeTexture(name(texture)?);
                        let [nearest, linear] = self.samplers;
                        let sampler = match filter {
                            Filter::Nearest => nearest,
                            Filter::Linear => linear,
                        };
                        gl.active_texture(glow::TEXTURE0.saturating_add(unit));
                        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                        gl.bind_sampler(unit, Some(sampler));
                        gl.uniform_1_i32(at, int(unit)?);
                        unit = unit.saturating_add(1);
                    }
                    Uniform::Float(v) => gl.uniform_1_f32(at, v),
                    Uniform::Vec2([a, b]) => gl.uniform_2_f32(at, a, b),
                    Uniform::Vec4([a, b, c, d]) => gl.uniform_4_f32(at, a, b, c, d),
                    Uniform::IVec4([a, b, c, d]) => gl.uniform_4_i32(at, a, b, c, d),
                    Uniform::UVec4([a, b, c, d]) => gl.uniform_4_u32(at, a, b, c, d),
                }
            }
            match draw.blend {
                Blend::Replace => gl.disable(glow::BLEND),
                Blend::Over => {
                    gl.enable(glow::BLEND);
                    gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                }
            }
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes, glow::STREAM_DRAW);
            gl.draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, count);
            gl.bind_vertex_array(None);
        }
        Ok(())
    }

    /// Ends a frame: makes a fence after the queued commands and sends
    /// them to the GPU, and gives the fence's name for [`Context::wait`].
    /// Where the context makes no fence sync, `glFinish` waits for the GPU
    /// here, and the wait finds the frame finished.
    pub fn submit(&mut self) -> Result<u32, Error> {
        self.current()?;
        let sync = if self.fence_sync {
            Some(
                self.egl
                    .fence()
                    .map_err(|e| egl_error("eglCreateSync", e))?,
            )
        } else {
            // SAFETY: a GL call on the current context.
            #[expect(unsafe_code, reason = "glFinish")]
            unsafe {
                self.gl.finish();
            }
            None
        };
        self.last_fence = self.last_fence.wrapping_add(1).max(1);
        self.fences.insert(self.last_fence, sync);
        Ok(self.last_fence)
    }

    /// Waits until the GPU has done every command before the fence
    /// `fence` that [`Context::submit`] gave, and forgets it. Refused,
    /// reaching no EGL, for a name no submit gave or a wait already took.
    pub fn wait(&mut self, fence: u32) -> Result<(), Error> {
        let sync = self
            .fences
            .remove(&fence)
            .ok_or_else(|| misuse(format!("no fence {fence} waits")))?;
        let Some(sync) = sync else {
            return Ok(());
        };
        if let Err(e) = self.current() {
            self.egl.destroy_sync(sync);
            return Err(e);
        }
        self.egl
            .wait(sync)
            .map_err(|e| egl_error("eglClientWaitSync", e))
    }

    /// Reads `rect` of `framebuffer` into `out`, RGBA8 rows with no gap
    /// between them. Refused, reaching no GL, unless `out` is exactly the
    /// rectangle's size.
    pub fn read(&mut self, framebuffer: u32, rect: Rect, out: &mut [u8]) -> Result<(), Error> {
        let fb = glow::NativeFramebuffer(name(framebuffer)?);
        let [x, y, w, h] = ints(rect)?;
        let need = u64::from(rect.width)
            .checked_mul(u64::from(rect.height))
            .and_then(|n| n.checked_mul(4))
            .and_then(|n| usize::try_from(n).ok());
        if need != Some(out.len()) {
            return Err(misuse(format!(
                "a read of {}x{} pixels is given {} bytes",
                rect.width,
                rect.height,
                out.len()
            )));
        }
        if out.is_empty() {
            return Ok(());
        }
        self.current()?;
        // SAFETY: with no buffer bound for packing, an alignment of 1, no
        // row length and no skipped pixels or rows, GL writes `w * h * 4`
        // bytes of RGBA8, which is `out`'s length.
        #[expect(unsafe_code, reason = "glReadPixels into a slice checked above")]
        unsafe {
            self.gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            self.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            self.gl.pixel_store_i32(glow::PACK_ROW_LENGTH, 0);
            self.gl.pixel_store_i32(glow::PACK_SKIP_PIXELS, 0);
            self.gl.pixel_store_i32(glow::PACK_SKIP_ROWS, 0);
            self.gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fb));
            self.gl.read_pixels(
                x,
                y,
                w,
                h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(out)),
            );
        }
        Ok(())
    }
}
