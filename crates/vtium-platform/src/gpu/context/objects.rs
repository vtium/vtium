//! A context's objects: textures, upright frames, imported buffers and
//! programs.
//!
//! OpenGL ES 3.0 has a row length (`GL_UNPACK_ROW_LENGTH`), so an upload
//! takes a rectangle of a larger image in one call.

use glow::HasContext;

use super::{
    Context, Import, Location, TextureFormat, TextureInfo, bound, int, ints, misuse, name, param,
};
use crate::display::{DmaBuf, Rect};
use crate::gpu::{Error, ErrorKind};

impl Context {
    /// A `width` by `height` texture of `format`, its contents undefined,
    /// sampled nearest and clamped at its edges. The context keeps its
    /// format and size, which every upload into it is checked against.
    pub fn create_texture(
        &mut self,
        format: TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<u32, Error> {
        self.current()?;
        let (w, h) = (int(width)?, int(height)?);
        // SAFETY: GL calls on the current context with no pointer.
        #[expect(unsafe_code, reason = "glTexStorage2D on a new texture")]
        unsafe {
            let texture = self
                .gl
                .create_texture()
                .map_err(|e| Error::new(ErrorKind::Gl(0), e))?;
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            self.gl
                .tex_storage_2d(glow::TEXTURE_2D, 1, format.internal(), w, h);
            // GL's default minifying filter reads mipmaps, which a texture of
            // one level lacks, so the texture would sample black with no
            // error. A draw samples through a sampler object, whose filter
            // overrides these; the texture's own keep it complete whatever
            // samples it.
            for (param, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::NEAREST),
                (glow::TEXTURE_MAG_FILTER, glow::NEAREST),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                self.gl
                    .tex_parameter_i32(glow::TEXTURE_2D, param, self::param(value));
            }
            // The texture is recorded whether or not GL gave it storage: a
            // texture whose storage GL refused, as out of memory, only makes
            // a later upload raise GL_INVALID_OPERATION, and a command that
            // raises an error does nothing else and reads no client memory
            // (OpenGL ES 3.0, section 2.5). The refusal itself reaches the
            // caller at the batch's check, with no call here that waits on
            // the GPU.
            let name = texture.0.get();
            self.textures.insert(
                name,
                TextureInfo {
                    format,
                    width,
                    height,
                },
            );
            Ok(name)
        }
    }

    /// Fills `rect` of `texture` from `pixels`, in the texture's format,
    /// rows `stride` bytes apart. Refused, reaching no GL, unless the
    /// texture is one this context made and keeps, `rect` lies inside it,
    /// `stride` holds whole pixels and a whole row, and `pixels` holds every
    /// row.
    pub fn fill_texture(
        &mut self,
        texture: u32,
        rect: Rect,
        pixels: &[u8],
        stride: usize,
    ) -> Result<(), Error> {
        let info = *self
            .textures
            .get(&texture)
            .ok_or_else(|| misuse(format!("texture {texture} is not this context's")))?;
        let inside = rect
            .x
            .checked_add(rect.width)
            .is_some_and(|r| r <= info.width)
            && rect
                .y
                .checked_add(rect.height)
                .is_some_and(|b| b <= info.height);
        if !inside {
            return Err(misuse(format!(
                "an upload to {rect:?} passes the {}x{} texture",
                info.width, info.height
            )));
        }
        let texture = glow::NativeTexture(name(texture)?);
        let format = info.format;
        let [x, y, w, h] = ints(rect)?;
        let bpp = format.bytes();
        let row = usize::try_from(rect.width)
            .ok()
            .and_then(|w| w.checked_mul(bpp))
            .ok_or_else(|| misuse("an upload's row is too long"))?;
        if stride < row || stride.checked_rem(bpp) != Some(0) {
            return Err(misuse(format!(
                "a stride of {stride} bytes holds no whole row of {row} bytes"
            )));
        }
        let rows = usize::try_from(rect.height).map_err(|_| misuse("too many rows"))?;
        let need = match rows.checked_sub(1) {
            None => 0,
            Some(before) => before
                .checked_mul(stride)
                .and_then(|n| n.checked_add(row))
                .ok_or_else(|| misuse("an upload past memory"))?,
        };
        let pixels = pixels.get(..need).ok_or_else(|| {
            misuse(format!(
                "an upload of {} bytes is short of the {need} its rectangle needs",
                pixels.len()
            ))
        })?;
        if need == 0 {
            return Ok(());
        }
        let row_length = stride
            .checked_div(bpp)
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| misuse("a stride past GL's integers"))?;
        self.current()?;
        // SAFETY: the texture is one this context made, of the format and
        // size it keeps, and `rect` lies inside it, so GL takes `bpp` bytes
        // a pixel from the format given here. With no buffer bound for
        // unpacking, an alignment of 1, no skipped pixels or rows and a row
        // length of `stride / bpp`, all set here, GL reads
        // `(h - 1) * stride + w * bpp` bytes, which is `need`, the length of
        // `pixels`.
        #[expect(unsafe_code, reason = "glTexSubImage2D from a slice checked above")]
        unsafe {
            self.gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
            self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            self.gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, row_length);
            self.gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, 0);
            self.gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, 0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                x,
                y,
                w,
                h,
                format.pixels(),
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(pixels)),
            );
        }
        Ok(())
    }

    /// Frees `texture`, one this context made and keeps.
    pub fn free_texture(&mut self, texture: u32) -> Result<(), Error> {
        if !self.textures.contains_key(&texture) {
            return Err(misuse(format!("texture {texture} is not this context's")));
        }
        let gl_texture = glow::NativeTexture(name(texture)?);
        self.current()?;
        self.textures.remove(&texture);
        // SAFETY: a GL call on the current context with no pointer.
        #[expect(unsafe_code, reason = "glDeleteTextures")]
        unsafe {
            self.gl.delete_texture(gl_texture);
        }
        Ok(())
    }

    /// Whether the framebuffer bound for drawing is complete; `what` names
    /// it in the error.
    fn complete(&mut self, what: &str) -> Result<(), Error> {
        // SAFETY: a GL query on the current context.
        #[expect(unsafe_code, reason = "glCheckFramebufferStatus")]
        let status = unsafe { self.gl.check_framebuffer_status(glow::FRAMEBUFFER) };
        if status == glow::FRAMEBUFFER_COMPLETE {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::Gl(status),
                format!("{what}'s framebuffer is incomplete ({status:#06x})"),
            ))
        }
    }

    /// An upright frame: a `width` by `height` RGBA8 texture and a
    /// framebuffer that draws into it. Gives the texture and the
    /// framebuffer.
    pub fn create_frame(&mut self, width: u32, height: u32) -> Result<(u32, u32), Error> {
        let texture = self.create_texture(TextureFormat::Rgba8, width, height)?;
        let tex = glow::NativeTexture(name(texture)?);
        // SAFETY: GL calls on the current context with no pointer.
        #[expect(unsafe_code, reason = "a framebuffer over the frame's texture")]
        let framebuffer = unsafe {
            match self.gl.create_framebuffer() {
                Ok(fb) => {
                    self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                    self.gl.framebuffer_texture_2d(
                        glow::FRAMEBUFFER,
                        glow::COLOR_ATTACHMENT0,
                        glow::TEXTURE_2D,
                        Some(tex),
                        0,
                    );
                    Ok(fb)
                }
                Err(e) => Err(Error::new(ErrorKind::Gl(0), e)),
            }
        };
        let fb = match framebuffer {
            Ok(fb) => fb.0.get(),
            Err(e) => {
                let _ = self.free_texture(texture);
                return Err(e);
            }
        };
        if let Err(e) = self.complete("the upright frame") {
            let _ = self.free_frame(fb, texture);
            return Err(e);
        }
        Ok((texture, fb))
    }

    /// Imports `buffer`, a display's buffer, as an EGL image bound as the
    /// storage of a renderbuffer, and gives a framebuffer that draws into
    /// it. A buffer the driver cannot import, or cannot draw into, is an
    /// error of kind [`ErrorKind::Import`]. The errors GL raises binding the
    /// image are read at once, as [`bound`] sorts them, so the next check
    /// does not report them against the frame that follows; the flags an
    /// earlier call left are set aside for that check first, so they fail
    /// no import.
    pub fn import(&mut self, buffer: &DmaBuf) -> Result<u32, Error> {
        self.current()?;
        let import = |detail: String| Error::new(ErrorKind::Import, detail);
        let Some(target) = self.image_target else {
            return Err(import("GL lacks GL_OES_EGL_image".to_owned()));
        };
        let image = self.egl.import(buffer).map_err(import)?;
        self.set_aside();
        // SAFETY: GL calls on the current context; the image is a live EGL
        // image of this display, which glEGLImageTargetRenderbufferStorageOES
        // takes as its pointer.
        #[expect(unsafe_code, reason = "an EGL image bound as a renderbuffer's storage")]
        let renderbuffer = unsafe {
            self.gl.create_renderbuffer().inspect(|&renderbuffer| {
                self.gl
                    .bind_renderbuffer(glow::RENDERBUFFER, Some(renderbuffer));
                target(glow::RENDERBUFFER, image.as_ptr());
            })
        };
        let renderbuffer = match renderbuffer {
            Ok(rb) => rb,
            Err(e) => {
                self.egl.destroy_image(image);
                return Err(import(e));
            }
        };
        let framebuffer = bound(&self.flags(), self.reset()).and_then(|()| {
            // SAFETY: GL calls on the current context with no pointer.
            #[expect(unsafe_code, reason = "a framebuffer over the renderbuffer")]
            unsafe {
                let fb = self.gl.create_framebuffer().map_err(import)?;
                self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fb));
                self.gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::RENDERBUFFER,
                    Some(renderbuffer),
                );
                Ok(fb)
            }
        });
        let fb = match framebuffer {
            Ok(fb) => fb,
            Err(e) => {
                // SAFETY: a GL call on the current context with no pointer.
                #[expect(unsafe_code, reason = "glDeleteRenderbuffers")]
                unsafe {
                    self.gl.delete_renderbuffer(renderbuffer);
                }
                self.egl.destroy_image(image);
                return Err(e);
            }
        };
        self.imports.insert(
            fb.0.get(),
            Import {
                renderbuffer,
                image,
            },
        );
        if let Err(e) = self.complete("an imported buffer") {
            let _ = self.free_import(fb.0.get());
            return Err(import(e.detail));
        }
        Ok(fb.0.get())
    }

    /// Frees an upright frame's framebuffer and texture.
    pub fn free_frame(&mut self, framebuffer: u32, texture: u32) -> Result<(), Error> {
        let fb = glow::NativeFramebuffer(name(framebuffer)?);
        self.free_texture(texture)?;
        // SAFETY: a GL call on the current context with no pointer.
        #[expect(unsafe_code, reason = "glDeleteFramebuffers")]
        unsafe {
            self.gl.delete_framebuffer(fb);
        }
        Ok(())
    }

    /// Frees an imported buffer's framebuffer, its renderbuffer and its
    /// image.
    pub fn free_import(&mut self, framebuffer: u32) -> Result<(), Error> {
        let fb = glow::NativeFramebuffer(name(framebuffer)?);
        self.current()?;
        let import = self.imports.remove(&framebuffer);
        // SAFETY: GL calls on the current context with no pointer.
        #[expect(unsafe_code, reason = "glDeleteFramebuffers and glDeleteRenderbuffers")]
        unsafe {
            self.gl.delete_framebuffer(fb);
            if let Some(i) = &import {
                self.gl.delete_renderbuffer(i.renderbuffer);
            }
        }
        if let Some(i) = import {
            self.egl.destroy_image(i.image);
        }
        Ok(())
    }

    /// Compiles `vertex` and `fragment` and links them. A shader that does
    /// not compile or a program that does not link is an error of kind
    /// [`ErrorKind::Compile`] with the driver's log.
    pub fn compile(&mut self, vertex: &str, fragment: &str) -> Result<u32, Error> {
        self.current()?;
        let compile = |detail: String| Error::new(ErrorKind::Compile, detail);
        // SAFETY: GL calls on the current context, each string passed with
        // its length.
        #[expect(unsafe_code, reason = "compiling and linking a program")]
        unsafe {
            let gl = &self.gl;
            let mut shaders = Vec::new();
            let mut logs = Vec::new();
            for (kind, label, text) in [
                (glow::VERTEX_SHADER, "vertex", vertex),
                (glow::FRAGMENT_SHADER, "fragment", fragment),
            ] {
                let shader = match gl.create_shader(kind) {
                    Ok(shader) => shader,
                    Err(e) => {
                        logs.push(format!("the {label} shader cannot be made: {e}"));
                        break;
                    }
                };
                gl.shader_source(shader, text);
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    logs.push(format!(
                        "the {label} shader does not compile: {}",
                        gl.get_shader_info_log(shader).trim_end()
                    ));
                }
                shaders.push(shader);
            }
            let program = if logs.is_empty() {
                gl.create_program().map_err(compile)
            } else {
                Err(compile(logs.join("; ")))
            };
            if let Ok(p) = program {
                for &s in &shaders {
                    gl.attach_shader(p, s);
                }
                gl.link_program(p);
                for &s in &shaders {
                    gl.detach_shader(p, s);
                }
            }
            for s in shaders {
                gl.delete_shader(s);
            }
            let p = program?;
            if !gl.get_program_link_status(p) {
                let log = gl.get_program_info_log(p);
                gl.delete_program(p);
                return Err(compile(format!(
                    "the program does not link: {}",
                    log.trim_end()
                )));
            }
            Ok(p.0.get())
        }
    }

    /// Where `program`'s uniform `uniform` is; `None` where the program
    /// has none of that name, as when the driver dropped a uniform nothing
    /// reads. A name with a NUL in it is refused, reaching no GL.
    pub fn uniform_location(
        &mut self,
        program: u32,
        uniform: &str,
    ) -> Result<Option<Location>, Error> {
        if uniform.contains('\0') {
            return Err(misuse(format!("the uniform name {uniform:?} holds a NUL")));
        }
        let p = glow::NativeProgram(name(program)?);
        self.current()?;
        // SAFETY: a GL query on the current context, the name passed with
        // its length.
        #[expect(unsafe_code, reason = "glGetUniformLocation")]
        let location = unsafe { self.gl.get_uniform_location(p, uniform) };
        Ok(location.map(|l| Location(l.0)))
    }

    /// Frees `program`.
    pub fn free_program(&mut self, program: u32) -> Result<(), Error> {
        let p = glow::NativeProgram(name(program)?);
        self.current()?;
        // SAFETY: a GL call on the current context with no pointer.
        #[expect(unsafe_code, reason = "glDeleteProgram")]
        unsafe {
            self.gl.delete_program(p);
        }
        Ok(())
    }
}
