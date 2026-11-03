//! The [`Ledger`]: one context's live handles, and the checks every [`Gl`]
//! implementation makes before a call reaches GL.
//!
//! The stand-in GL and the binary's adapter over vtium-platform's GL both
//! keep a ledger and route each call through it, so the two refuse the same
//! calls with the same errors (AGENTS.md: a stand-in checks what the real
//! part checks). Each method that makes, uses or frees a handle takes the
//! GL work as a closure, which the ledger runs only once the call's
//! arguments have passed, and hands it the implementation's own name for
//! each object, a `u32` such as a GL object name.
//!
//! [`Gl`]: super::Gl

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use super::gl::{
    ContextId, Draw, DrawTarget, Error, ErrorKind, Filter, Format, Handle, Input, Limits, Program,
    Size, Source, Texture, Value,
};
use crate::target::Rect;

/// The next context's id. Ids only tell contexts apart, and never reach a
/// pixel.
static NEXT_CONTEXT: AtomicU64 = AtomicU64::new(1);

/// A live texture, as the ledger knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureInfo {
    /// The implementation's name for it.
    pub name: u32,
    pub format: Format,
    pub size: Size,
}

/// A live target, as the ledger knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetInfo {
    /// The implementation's name for it.
    pub name: u32,
    pub size: Size,
    /// The serial of the texture it draws into, for the upright frame;
    /// `None` for an imported display buffer.
    texture: Option<u64>,
}

#[derive(Clone, Debug)]
struct ProgramInfo {
    name: u32,
    inputs: Vec<Input>,
}

/// A value of a draw's input with its texture resolved to the
/// implementation's name.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Resolved {
    Texture(u32, Filter),
    Float(f32),
    Vec2([f32; 2]),
    Vec4([f32; 4]),
    IVec4([i32; 4]),
    UVec4([u32; 4]),
}

/// A draw whose handles have all passed, with the implementation's names.
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    pub target: TargetInfo,
    pub program: u32,
    /// Each input's name and value, in the order the draw gave them.
    pub inputs: Vec<(&'static str, Resolved)>,
}

/// One context's live handles and limits.
#[derive(Debug)]
pub struct Ledger {
    context: ContextId,
    limits: Limits,
    next: u64,
    lost: bool,
    textures: BTreeMap<u64, TextureInfo>,
    targets: BTreeMap<u64, TargetInfo>,
    programs: BTreeMap<u64, ProgramInfo>,
}

fn misuse(detail: impl Into<String>) -> Error {
    Error::new(ErrorKind::Misuse, detail)
}

impl Ledger {
    /// A ledger for a new context with `limits`.
    pub fn new(limits: Limits) -> Ledger {
        Ledger {
            context: ContextId(NEXT_CONTEXT.fetch_add(1, Ordering::Relaxed)),
            limits,
            next: 1,
            lost: false,
            textures: BTreeMap::new(),
            targets: BTreeMap::new(),
            programs: BTreeMap::new(),
        }
    }

    pub fn context(&self) -> ContextId {
        self.context
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Marks the context lost: every later call fails with
    /// [`ErrorKind::ContextLost`]. The implementation calls it when GL
    /// reports the loss.
    pub fn lose(&mut self) {
        self.lost = true;
    }

    /// How many textures, targets and programs are live.
    pub fn live(&self) -> (usize, usize, usize) {
        (self.textures.len(), self.targets.len(), self.programs.len())
    }

    /// Fails once the context is lost.
    pub fn alive(&self) -> Result<(), Error> {
        if self.lost {
            return Err(Error::new(ErrorKind::ContextLost, "the context was lost"));
        }
        Ok(())
    }

    fn handle(&mut self) -> Handle {
        let serial = self.next;
        self.next = self.next.saturating_add(1);
        Handle {
            context: self.context,
            serial,
        }
    }

    /// The serial of `h`, when this context made it.
    fn own(&self, h: &Handle, what: &str) -> Result<u64, Error> {
        self.alive()?;
        if h.context != self.context {
            return Err(Error::new(
                ErrorKind::Handle,
                format!("the {what} belongs to another context"),
            ));
        }
        Ok(h.serial)
    }

    fn texture_info(&self, texture: &Texture) -> Result<TextureInfo, Error> {
        let serial = self.own(&texture.0, "texture")?;
        self.textures
            .get(&serial)
            .copied()
            .ok_or_else(|| Error::new(ErrorKind::Handle, "the texture was freed"))
    }

    /// Checks a texture of `format` and `size`, and on success runs `make`,
    /// which makes it and gives its name.
    pub fn create_texture(
        &mut self,
        format: Format,
        size: Size,
        make: impl FnOnce() -> Result<u32, Error>,
    ) -> Result<Texture, Error> {
        self.alive()?;
        if !self.limits.fits_texture(size) {
            return Err(Error::new(
                ErrorKind::Limit,
                format!(
                    "a {}x{} texture passes the largest texture, {}",
                    size.width, size.height, self.limits.texture
                ),
            ));
        }
        let name = make()?;
        let h = self.handle();
        self.textures
            .insert(h.serial, TextureInfo { name, format, size });
        Ok(Texture(h))
    }

    /// Checks an upload of `len` bytes, rows `stride` apart, into `rect` of
    /// `texture`, and on success runs `fill` with the texture.
    pub fn fill_texture<T>(
        &self,
        texture: &Texture,
        rect: Rect,
        len: usize,
        stride: usize,
        fill: impl FnOnce(TextureInfo) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let info = self.texture_info(texture)?;
        if !info.size.holds(rect) {
            return Err(misuse(format!(
                "an upload to {rect:?} passes the {}x{} texture",
                info.size.width, info.size.height
            )));
        }
        let bpp = info.format.bytes();
        let row = usize::try_from(rect.width)
            .ok()
            .and_then(|w| w.checked_mul(bpp))
            .ok_or_else(|| misuse("an upload's row is too long"))?;
        if stride < row || stride.checked_rem(bpp) != Some(0) {
            return Err(misuse(format!(
                "a stride of {stride} bytes does not hold whole rows of {row} bytes"
            )));
        }
        let need = match usize::try_from(rect.height).unwrap_or(usize::MAX) {
            0 => 0,
            h => h
                .saturating_sub(1)
                .checked_mul(stride)
                .and_then(|n| n.checked_add(row))
                .unwrap_or(usize::MAX),
        };
        if len < need {
            return Err(misuse(format!(
                "an upload of {len} bytes is short of the {need} its rectangle needs"
            )));
        }
        fill(info)
    }

    /// Checks that `texture` is live and this context's, and on success runs
    /// `free` with it and forgets it.
    pub fn free_texture(
        &mut self,
        texture: Texture,
        free: impl FnOnce(TextureInfo) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let info = self.texture_info(&texture)?;
        free(info)?;
        self.textures.remove(&texture.0.serial);
        Ok(())
    }

    /// Checks an upright frame of `size` against the largest texture and
    /// viewport, and on success runs `make`, which makes its texture and its
    /// target and gives their names.
    pub fn create_frame(
        &mut self,
        size: Size,
        make: impl FnOnce() -> Result<(u32, u32), Error>,
    ) -> Result<DrawTarget, Error> {
        self.alive()?;
        if !self.limits.fits_texture(size) || !self.limits.fits_viewport(size) {
            return Err(Error::new(
                ErrorKind::Limit,
                format!(
                    "a {}x{} frame passes the largest texture or viewport",
                    size.width, size.height
                ),
            ));
        }
        let (texture, target) = make()?;
        let t = self.handle();
        self.textures.insert(
            t.serial,
            TextureInfo {
                name: texture,
                format: Format::Rgba8,
                size,
            },
        );
        let h = self.handle();
        self.targets.insert(
            h.serial,
            TargetInfo {
                name: target,
                size,
                texture: Some(t.serial),
            },
        );
        Ok(DrawTarget {
            handle: h,
            texture: Some(Texture(t)),
        })
    }

    /// Checks an imported display buffer of `size` against the largest
    /// renderbuffer and viewport, and on success runs `make` and records it.
    pub fn import_buffer(
        &mut self,
        size: Size,
        make: impl FnOnce() -> Result<u32, Error>,
    ) -> Result<DrawTarget, Error> {
        self.alive()?;
        if !self.limits.fits_buffer(size) {
            return Err(Error::new(
                ErrorKind::Limit,
                format!(
                    "a {}x{} buffer passes the largest renderbuffer or viewport",
                    size.width, size.height
                ),
            ));
        }
        let name = make()?;
        let h = self.handle();
        self.targets.insert(
            h.serial,
            TargetInfo {
                name,
                size,
                texture: None,
            },
        );
        Ok(DrawTarget {
            handle: h,
            texture: None,
        })
    }

    /// The live target `target`.
    pub fn target(&self, target: &DrawTarget) -> Result<TargetInfo, Error> {
        let serial = self.own(&target.handle, "target")?;
        self.targets
            .get(&serial)
            .copied()
            .ok_or_else(|| Error::new(ErrorKind::Handle, "the target was freed"))
    }

    /// Checks `target` and on success runs `free` with it and the upright
    /// frame's texture, and forgets both.
    pub fn free_target(
        &mut self,
        target: DrawTarget,
        free: impl FnOnce(TargetInfo, Option<TextureInfo>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let info = self.target(&target)?;
        let texture = target
            .texture
            .as_ref()
            .map(|t| self.texture_info(t))
            .transpose()?;
        free(info, texture)?;
        self.targets.remove(&target.handle.serial);
        if let Some(t) = &target.texture {
            self.textures.remove(&t.0.serial);
        }
        Ok(())
    }

    /// Checks that each of `source`'s inputs is declared as a uniform once,
    /// in one of its shaders, and that the GPU has `highp` in fragment
    /// shaders, and on success runs `compile`.
    pub fn compile(
        &mut self,
        source: &Source<'_>,
        compile: impl FnOnce() -> Result<u32, Error>,
    ) -> Result<Program, Error> {
        self.alive()?;
        if !self.limits.fragment_highp {
            return Err(Error::new(
                ErrorKind::Limit,
                "fragment shaders lack highp, which every program needs",
            ));
        }
        let declared = uniforms(&source.vertex_text())
            .into_iter()
            .chain(uniforms(&source.fragment_text()))
            .collect::<Vec<_>>();
        for (i, input) in source.inputs.iter().enumerate() {
            if !declared.iter().any(|d| d == input.name) {
                return Err(misuse(format!(
                    "the program declares no uniform `{}`",
                    input.name
                )));
            }
            if source
                .inputs
                .get(..i)
                .is_some_and(|before| before.iter().any(|b| b.name == input.name))
            {
                return Err(misuse(format!(
                    "the input `{}` is listed twice",
                    input.name
                )));
            }
        }
        let name = compile()?;
        let h = self.handle();
        self.programs.insert(
            h.serial,
            ProgramInfo {
                name,
                inputs: source.inputs.to_vec(),
            },
        );
        Ok(Program(h))
    }

    fn program_info(&self, program: &Program) -> Result<&ProgramInfo, Error> {
        let serial = self.own(&program.0, "program")?;
        self.programs
            .get(&serial)
            .ok_or_else(|| Error::new(ErrorKind::Handle, "the program was freed"))
    }

    /// Checks `program` and on success runs `free` with its name and forgets
    /// it.
    pub fn free_program(
        &mut self,
        program: Program,
        free: impl FnOnce(u32) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let name = self.program_info(&program)?.name;
        free(name)?;
        self.programs.remove(&program.0.serial);
        Ok(())
    }

    /// Checks `draw`: its target and program live and this context's, a
    /// value of the right kind for each of the program's inputs and for
    /// nothing else, each texture live and this context's and none the
    /// target draws into. On success it gives the draw with each handle
    /// resolved.
    pub fn draw(&self, draw: &Draw<'_>) -> Result<Checked, Error> {
        let target = self.target(draw.target)?;
        let program = self.program_info(draw.program)?;
        for input in &program.inputs {
            if !draw.inputs.iter().any(|(n, _)| *n == input.name) {
                return Err(misuse(format!(
                    "the draw gives no value for `{}`",
                    input.name
                )));
            }
        }
        let mut inputs = Vec::with_capacity(draw.inputs.len());
        for (i, (name, value)) in draw.inputs.iter().enumerate() {
            let Some(input) = program.inputs.iter().find(|p| p.name == *name) else {
                return Err(misuse(format!("the program has no input `{name}`")));
            };
            if input.kind != value.kind() {
                return Err(misuse(format!(
                    "`{name}` takes a {:?}, not a {:?}",
                    input.kind,
                    value.kind()
                )));
            }
            if draw
                .inputs
                .get(..i)
                .is_some_and(|before| before.iter().any(|(b, _)| b == name))
            {
                return Err(misuse(format!("`{name}` is given twice")));
            }
            inputs.push((*name, self.resolve(value, &target)?));
        }
        Ok(Checked {
            target,
            program: program.name,
            inputs,
        })
    }

    fn resolve(&self, value: &Value<'_>, target: &TargetInfo) -> Result<Resolved, Error> {
        Ok(match *value {
            Value::Texture(t, filter) => {
                let info = self.texture_info(t)?;
                if target.texture == Some(t.0.serial) {
                    return Err(misuse("a draw reads the texture it draws into"));
                }
                Resolved::Texture(info.name, filter)
            }
            Value::Float(v) => Resolved::Float(v),
            Value::Vec2(v) => Resolved::Vec2(v),
            Value::Vec4(v) => Resolved::Vec4(v),
            Value::IVec4(v) => Resolved::IVec4(v),
            Value::UVec4(v) => Resolved::UVec4(v),
        })
    }

    /// Checks a read of `rect` of `target` into `len` bytes of RGBA8.
    pub fn read(&self, target: &DrawTarget, rect: Rect, len: usize) -> Result<TargetInfo, Error> {
        let info = self.target(target)?;
        if !info.size.holds(rect) {
            return Err(misuse(format!("a read of {rect:?} passes the target")));
        }
        let need = usize::try_from(rect.area().saturating_mul(4)).unwrap_or(usize::MAX);
        if len != need {
            return Err(misuse(format!(
                "a read of {need} bytes is given {len} bytes"
            )));
        }
        Ok(info)
    }
}

/// The names of the uniforms `glsl` declares: each line that starts with
/// `uniform`, its last word without the `;`.
fn uniforms(glsl: &str) -> Vec<String> {
    glsl.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("uniform "))
        .filter_map(|l| {
            l.trim_end_matches(';')
                .split_whitespace()
                .last()
                .map(str::to_owned)
        })
        .collect()
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;
    use crate::gpu::StandIn;

    /// The handle a caller would hold had it kept a copy: the types forbid
    /// one outside this crate, and the ledger refuses it all the same.
    fn copy(h: &Handle) -> Handle {
        Handle {
            context: h.context,
            serial: h.serial,
        }
    }

    #[test]
    fn a_freed_handle_is_refused_and_its_serial_never_comes_back() {
        let mut ledger = Ledger::new(StandIn::LIMITS);
        let size = Size::new(4, 4);
        let t = ledger.create_texture(Format::R8, size, || Ok(7)).unwrap();
        let kept = Texture(copy(&t.0));
        ledger.free_texture(t, |_| Ok(())).unwrap();
        // a new texture takes GL's freed name, never the freed serial
        let newer = ledger.create_texture(Format::R8, size, || Ok(7)).unwrap();
        assert_ne!(newer.0.serial, kept.0.serial);
        let rect = size.rect();
        let e = ledger
            .fill_texture(&kept, rect, 16, 4, |_| Ok(()))
            .unwrap_err();
        assert_eq!(e.kind, ErrorKind::Handle);
        assert_eq!(
            ledger.free_texture(kept, |_| Ok(())).unwrap_err().kind,
            ErrorKind::Handle
        );

        let target = ledger.import_buffer(size, || Ok(3)).unwrap();
        let kept = DrawTarget {
            handle: copy(&target.handle),
            texture: None,
        };
        ledger.free_target(target, |_, _| Ok(())).unwrap();
        assert_eq!(ledger.target(&kept).unwrap_err().kind, ErrorKind::Handle);
    }

    #[test]
    fn a_failed_call_leaves_the_ledger_as_it_was() {
        let mut ledger = Ledger::new(StandIn::LIMITS);
        let gl_error = || Error::new(ErrorKind::OutOfMemory, "planted");
        let size = Size::new(4, 4);
        assert!(
            ledger
                .create_texture(Format::R8, size, || Err(gl_error()))
                .is_err()
        );
        assert!(ledger.create_frame(size, || Err(gl_error())).is_err());
        let t = ledger.create_texture(Format::R8, size, || Ok(1)).unwrap();
        // GL refused the free: the texture stays live, and the ledger with it
        let kept = Texture(copy(&t.0));
        assert!(ledger.free_texture(t, |_| Err(gl_error())).is_err());
        assert!(ledger.texture_info(&kept).is_ok());
        assert_eq!(ledger.live(), (1, 0, 0));
    }
}
