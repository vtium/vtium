//! [`StandIn`], a GL context for the renderer's tests: it records the calls
//! that reach it (ADR-1303 D7), keeps what each texture was filled with,
//! and draws the clears and the cell program's quads into the upright
//! frames it made, the cells through a model of the cell shader in
//! `model.rs`. It draws no other program's pixels, so a test reads from it
//! the cells the GPU path composes, and nothing that present, the
//! pictures, the arrow or a text layer over a picture draws.
//!
//! It checks each call through a [`Ledger`], as the binary's adapter does,
//! so it refuses every call the ledger refuses; the crate's README lists
//! those, and what only a real driver refuses. Its limits are whatever a
//! test gives it, so a test can set a small largest texture and drive a
//! frame past it. A test can raise a GL error, which the next
//! [`Gl::check`] reports, as GL's error flag does, and can make a call of
//! one kind fail as a driver would, the next one or one later.

use std::collections::BTreeMap;

use super::compose::{CELL_FRAG, INK, OVER};
use super::gl::{
    Batch, Blend, Draw, DrawTarget, Error, ErrorKind, Fence, Format, Gl, Limits, Program, Quad,
    Size, Source, Texture,
};
use super::ledger::{Ledger, Resolved};
use super::model::{self, Inputs, Texels};
use crate::target::Rect;

/// A call that reached the stand-in's GL, past the ledger's checks.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    CreateTexture(Format, Size),
    FillTexture(Rect),
    FreeTexture,
    CreateFrame(Size),
    ImportBuffer(Size),
    FreeTarget,
    /// A program compiled, with its fragment shader's text.
    Compile(String),
    FreeProgram,
    Clear(Rect, u32),
    /// A draw: its quads, its blend and the names of its inputs.
    Draw {
        quads: Vec<Quad>,
        blend: Blend,
        inputs: Vec<&'static str>,
    },
    Check(Batch),
    /// A frame's end, which gives a fence.
    Submit,
    /// A wait on a fence, which frees it.
    Wait,
    Read(Rect),
}

/// A call a test can make fail as a driver would, past the ledger's
/// checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    CreateTexture,
    CreateFrame,
    ImportBuffer,
    Compile,
    Wait,
}

/// A GL context that records its calls and draws nothing.
#[derive(Debug)]
pub struct StandIn {
    ledger: Ledger,
    calls: Vec<Call>,
    next_name: u32,
    raised: Option<ErrorKind>,
    /// The calls a test made fail: each kind of call, how many of that
    /// kind pass first, and the error.
    failing: Vec<(Op, usize, ErrorKind)>,
    /// The name of each upright frame made, in the order they were made.
    frames: Vec<u32>,
    /// What the stand-in saw of each draw since the calls were forgotten.
    seen: Vec<Seen>,
    /// Each live texture's format, size and pixels, by its name.
    texels: BTreeMap<u32, (Format, Size, Vec<u8>)>,
    /// Each upright frame's size and pixels, `0x00RRGGBB`, by its name.
    pixels: BTreeMap<u32, (Size, Vec<u32>)>,
    /// The fragment shader of each live program, by its name.
    programs: BTreeMap<u32, String>,
}

/// What the stand-in saw of one draw past what [`Call::Draw`] records: the
/// name of its target, and the values of its `ivec4` inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    /// The name of the framebuffer the draw went to.
    pub target: u32,
    /// Each `ivec4` input the draw set, by its name in the shader, with
    /// its value.
    pub ivec4: Vec<(&'static str, [i32; 4])>,
    /// Each texture input the draw set, by its name in the shader, with
    /// the texture's name.
    pub textures: Vec<(&'static str, u32)>,
}

impl StandIn {
    /// Limits large enough for every display the tests draw: 16384 pixels
    /// a side for a texture, a renderbuffer and the viewport, with `highp`.
    pub const LIMITS: Limits = Limits {
        texture: 16384,
        renderbuffer: 16384,
        viewport: Size::new(16384, 16384),
        fragment_highp: true,
    };

    /// A stand-in with [`StandIn::LIMITS`].
    pub fn new() -> StandIn {
        StandIn::with_limits(StandIn::LIMITS)
    }

    /// A stand-in that reports `limits`, such as a smaller largest texture.
    pub fn with_limits(limits: Limits) -> StandIn {
        StandIn {
            ledger: Ledger::new(limits),
            calls: Vec::new(),
            next_name: 0,
            raised: None,
            failing: Vec::new(),
            frames: Vec::new(),
            seen: Vec::new(),
            texels: BTreeMap::new(),
            pixels: BTreeMap::new(),
            programs: BTreeMap::new(),
        }
    }

    /// The pixels of the upright frame named `name`, `0x00RRGGBB`, as its
    /// clears and the cell program's quads drew them, and its size.
    pub fn frame_pixels(&self, name: u32) -> Option<(Size, &[u32])> {
        self.pixels.get(&name).map(|(s, p)| (*s, p.as_slice()))
    }

    /// The format, size and pixels of the texture named `name`, as the
    /// fills since it was made left them.
    pub fn texture_pixels(&self, name: u32) -> Option<(Format, Size, &[u8])> {
        self.texels
            .get(&name)
            .map(|(f, s, p)| (*f, *s, p.as_slice()))
    }

    /// The calls so far.
    pub fn calls(&self) -> &[Call] {
        &self.calls
    }

    /// Forgets the calls so far.
    pub fn clear_calls(&mut self) {
        self.calls.clear();
        self.seen.clear();
    }

    /// The name of each upright frame the stand-in made, in the order it
    /// made them, kept when the calls are forgotten: a test maps a draw's
    /// target to the tile it draws into.
    pub fn frame_names(&self) -> &[u32] {
        &self.frames
    }

    /// What the stand-in saw of each [`Call::Draw`] in [`StandIn::calls`],
    /// in the same order: its target, and its `ivec4` inputs' values.
    pub fn seen(&self) -> &[Seen] {
        &self.seen
    }

    /// How many textures, targets and programs are live.
    pub fn live(&self) -> (usize, usize, usize) {
        self.ledger.live()
    }

    /// How many fences are given out and not yet waited on.
    pub fn fences(&self) -> usize {
        self.ledger.fences()
    }

    /// Raises a GL error of `kind`, which the next check reports. A lost
    /// context also fails every call after that check.
    pub fn raise(&mut self, kind: ErrorKind) {
        self.raised = Some(kind);
    }

    /// Makes the next call of `op` fail with `kind`, as the driver would:
    /// an import a driver cannot do fails with [`ErrorKind::Import`], a
    /// shader it does not compile with [`ErrorKind::Compile`].
    pub fn fail_next(&mut self, op: Op, kind: ErrorKind) {
        self.fail_after(op, 0, kind);
    }

    /// Lets `passing` calls of `op` through, and makes the one after them
    /// fail with `kind`: an allocation that fails partway through a batch.
    pub fn fail_after(&mut self, op: Op, passing: usize, kind: ErrorKind) {
        self.failing.push((op, passing, kind));
    }

    /// A new name for `op`, or the failure a test asked for.
    fn make(&mut self, op: Op) -> Result<u32, Error> {
        if let Some(e) = take_failure(&mut self.failing, op) {
            return Err(e);
        }
        Ok(self.name())
    }

    fn name(&mut self) -> u32 {
        self.next_name = self.next_name.saturating_add(1);
        self.next_name
    }
}

impl StandIn {
    /// Draws `quads` into the upright frame named `target` through the
    /// model of the cell shader, when `program` is the cell program built
    /// without `OVER`, with `inputs` bound.
    fn draw_cells(
        &mut self,
        inputs: &[(&'static str, Resolved)],
        program: u32,
        target: u32,
        quads: &[Quad],
    ) {
        let Some(text) = self.programs.get(&program) else {
            return;
        };
        if !text.contains(CELL_FRAG) || text.contains(OVER) {
            return;
        }
        let define = |name: &str| {
            let at = text.find(name).map_or(0, |i| i.saturating_add(name.len()));
            text.get(at..)
                .and_then(|t| t.split_whitespace().next())
                .and_then(|n| n.parse::<i64>().ok())
                .unwrap_or(0)
        };
        let bound = (define("#define REACH_SIDE"), define("#define REACH_ROWS"));
        let empty = Texels {
            format: Format::R8,
            size: Size::new(0, 0),
            bytes: &[],
        };
        let texture = |name: &str| {
            inputs
                .iter()
                .find_map(|(n, v)| match v {
                    Resolved::Texture(t, _) if *n == name => self.texels.get(t),
                    _ => None,
                })
                .map_or(empty, |(format, size, bytes)| Texels {
                    format: *format,
                    size: *size,
                    bytes,
                })
        };
        let ivec4 = |name: &str| {
            inputs
                .iter()
                .find_map(|(n, v)| match v {
                    Resolved::IVec4(v) if *n == name => Some(v.map(i64::from)),
                    _ => None,
                })
                .unwrap_or_default()
        };
        let [cw, ch, _, _] = ivec4("cell");
        let [sw, sh, across, _] = ivec4("box_slot");
        let i = Inputs {
            masks: texture("masks"),
            colors: texture("colors"),
            sprites: texture("sprites"),
            boxes: texture("boxes"),
            ink: texture("ink"),
            cell: (cw, ch),
            box_slot: (sw, sh, across),
            bound,
            with_ink: text.contains(INK),
        };
        let mut drawn = Vec::new();
        let Some((size, _)) = self.pixels.get(&target) else {
            return;
        };
        for q in quads {
            let [x, y, w, h] = q.dst.map(i64::from);
            let (right, bottom) = (
                x.saturating_add(w).min(i64::from(size.width)),
                y.saturating_add(h).min(i64::from(size.height)),
            );
            for py in y.max(0)..bottom {
                for px in x.max(0)..right {
                    drawn.push((px, py, model::cell(q, &i, (px, py))));
                }
            }
        }
        if let Some((size, px)) = self.pixels.get_mut(&target) {
            let w = i64::from(size.width);
            for (x, y, c) in drawn {
                let at = y.saturating_mul(w).saturating_add(x);
                if let Some(p) = usize::try_from(at).ok().and_then(|i| px.get_mut(i)) {
                    *p = c;
                }
            }
        }
    }
}

/// The failure a test planned for the next call of `op`, once the calls it
/// lets pass have passed.
fn take_failure(failing: &mut Vec<(Op, usize, ErrorKind)>, op: Op) -> Option<Error> {
    let i = failing.iter().position(|(o, _, _)| *o == op)?;
    match failing.get_mut(i) {
        Some((_, passing, _)) if *passing > 0 => {
            *passing = passing.saturating_sub(1);
            None
        }
        _ => {
            let (_, _, kind) = failing.remove(i);
            Some(Error::new(kind, format!("{op:?} failed for the test")))
        }
    }
}

impl Default for StandIn {
    fn default() -> StandIn {
        StandIn::new()
    }
}

impl Gl for StandIn {
    /// The stand-in's display buffer is its size alone.
    type Buffer = Size;

    fn limits(&self) -> Limits {
        self.ledger.limits()
    }

    fn create_texture(&mut self, format: Format, size: Size) -> Result<Texture, Error> {
        let name = self.make(Op::CreateTexture);
        let named = name.as_ref().ok().copied();
        let t = self.ledger.create_texture(format, size, || name)?;
        let bytes = usize::try_from(size.rect().area())
            .unwrap_or(0)
            .saturating_mul(format.bytes());
        if let Some(name) = named {
            self.texels.insert(name, (format, size, vec![0; bytes]));
        }
        self.calls.push(Call::CreateTexture(format, size));
        Ok(t)
    }

    fn fill_texture(
        &mut self,
        texture: &Texture,
        rect: Rect,
        pixels: &[u8],
        stride: usize,
    ) -> Result<(), Error> {
        let info = self
            .ledger
            .fill_texture(texture, rect, pixels.len(), stride, Ok)?;
        if let Some((format, size, held)) = self.texels.get_mut(&info.name) {
            let bpp = format.bytes();
            let at = |v: u32| usize::try_from(v).unwrap_or(0);
            let (w, line) = (at(size.width), at(rect.width).saturating_mul(bpp));
            for (y, src) in (at(rect.y)..at(rect.y.saturating_add(rect.height)))
                .zip(pixels.chunks(stride.max(1)))
            {
                let start = y
                    .saturating_mul(w)
                    .saturating_add(at(rect.x))
                    .saturating_mul(bpp);
                if let (Some(dst), Some(src)) = (
                    held.get_mut(start..start.saturating_add(line)),
                    src.get(..line),
                ) {
                    dst.copy_from_slice(src);
                }
            }
        }
        self.calls.push(Call::FillTexture(rect));
        Ok(())
    }

    fn free_texture(&mut self, texture: Texture) -> Result<(), Error> {
        let texels = &mut self.texels;
        self.ledger.free_texture(texture, |info| {
            texels.remove(&info.name);
            Ok(())
        })?;
        self.calls.push(Call::FreeTexture);
        Ok(())
    }

    fn create_frame(&mut self, size: Size) -> Result<DrawTarget, Error> {
        let names = self.make(Op::CreateFrame).map(|n| (n, n));
        let t = self.ledger.create_frame(size, || names)?;
        let name = self.ledger.target(&t)?.name;
        self.frames.push(name);
        let px = usize::try_from(size.rect().area()).unwrap_or(0);
        self.pixels.insert(name, (size, vec![0; px]));
        self.calls.push(Call::CreateFrame(size));
        Ok(t)
    }

    fn import_buffer(&mut self, buffer: &Size) -> Result<DrawTarget, Error> {
        let name = self.make(Op::ImportBuffer);
        let t = self.ledger.import_buffer(*buffer, || name)?;
        self.calls.push(Call::ImportBuffer(*buffer));
        Ok(t)
    }

    fn free_target(&mut self, target: DrawTarget) -> Result<(), Error> {
        let name = self.ledger.target(&target)?.name;
        self.ledger.free_target(target, |_, _| Ok(()))?;
        self.pixels.remove(&name);
        self.calls.push(Call::FreeTarget);
        Ok(())
    }

    fn compile(&mut self, source: &Source<'_>) -> Result<Program, Error> {
        let name = self.make(Op::Compile);
        let named = name.as_ref().ok().copied();
        let p = self.ledger.compile(source, || name)?;
        if let Some(name) = named {
            self.programs.insert(name, source.fragment_text());
        }
        self.calls.push(Call::Compile(source.fragment_text()));
        Ok(p)
    }

    fn free_program(&mut self, program: Program) -> Result<(), Error> {
        let programs = &mut self.programs;
        self.ledger.free_program(program, |name| {
            programs.remove(&name);
            Ok(())
        })?;
        self.calls.push(Call::FreeProgram);
        Ok(())
    }

    fn clear(&mut self, target: &DrawTarget, rect: Rect, color: u32) -> Result<(), Error> {
        let name = self.ledger.target(target)?.name;
        if let Some((size, px)) = self.pixels.get_mut(&name) {
            for y in rect.y..rect.y.saturating_add(rect.height).min(size.height) {
                for x in rect.x..rect.x.saturating_add(rect.width).min(size.width) {
                    let at = u64::from(y)
                        .saturating_mul(u64::from(size.width))
                        .saturating_add(u64::from(x));
                    if let Some(p) = usize::try_from(at).ok().and_then(|i| px.get_mut(i)) {
                        *p = color & 0x00ff_ffff;
                    }
                }
            }
        }
        self.calls.push(Call::Clear(rect, color));
        Ok(())
    }

    fn draw(&mut self, draw: &Draw<'_>) -> Result<(), Error> {
        let checked = self.ledger.draw(draw)?;
        self.seen.push(Seen {
            target: checked.target.name,
            ivec4: checked
                .inputs
                .iter()
                .filter_map(|(n, v)| match v {
                    Resolved::IVec4(v) => Some((*n, *v)),
                    _ => None,
                })
                .collect(),
            textures: checked
                .inputs
                .iter()
                .filter_map(|(n, v)| match v {
                    Resolved::Texture(t, _) => Some((*n, *t)),
                    _ => None,
                })
                .collect(),
        });
        self.draw_cells(
            &checked.inputs,
            checked.program,
            checked.target.name,
            draw.quads,
        );
        self.calls.push(Call::Draw {
            quads: draw.quads.to_vec(),
            blend: draw.blend,
            inputs: checked.inputs.iter().map(|(n, _)| *n).collect(),
        });
        Ok(())
    }

    fn submit(&mut self, target: &DrawTarget) -> Result<Fence, Error> {
        let name = self.name();
        let fence = self.ledger.submit(target, |_| Ok(name))?;
        self.calls.push(Call::Submit);
        Ok(fence)
    }

    fn wait(&mut self, fence: Fence) -> Result<(), Error> {
        // the ledger checks the fence before a failure planned for a wait
        // is used up, as the adapter reaches its driver only then
        let failing = &mut self.failing;
        self.ledger.wait(fence, |_| {
            take_failure(failing, Op::Wait).map_or(Ok(()), Err)
        })?;
        self.calls.push(Call::Wait);
        Ok(())
    }

    fn check(&mut self, batch: Batch) -> Result<(), Error> {
        self.ledger.alive()?;
        self.calls.push(Call::Check(batch));
        match self.raised.take() {
            None => Ok(()),
            Some(kind) => {
                if kind == ErrorKind::ContextLost {
                    self.ledger.lose();
                }
                Err(Error {
                    kind,
                    batch: Some(batch),
                    detail: "raised by the test".to_owned(),
                })
            }
        }
    }

    /// Checks the read and fills `out` with zeros: the stand-in draws no
    /// pixels.
    fn read(&mut self, target: &DrawTarget, rect: Rect, out: &mut [u8]) -> Result<(), Error> {
        self.ledger.read(target, rect, out.len())?;
        out.fill(0);
        self.calls.push(Call::Read(rect));
        Ok(())
    }
}
