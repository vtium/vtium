//! [`StandIn`], a GL context for the renderer's tests: it records the calls
//! that reach it and draws no pixels (ADR-1303 D7).
//!
//! It checks each call through a [`Ledger`], as the binary's adapter does,
//! so it refuses every call the ledger refuses; the crate's README lists
//! those, and what only a real driver refuses. Its limits are whatever a
//! test gives it, so a test can set a small largest texture and drive a
//! frame past it. A test can raise a GL error, which the next
//! [`Gl::check`] reports, as GL's error flag does, and can make the next
//! call of one kind fail as a driver would.

use super::gl::{
    Batch, Blend, Draw, DrawTarget, Error, ErrorKind, Format, Gl, Limits, Program, Quad, Size,
    Source, Texture,
};
use super::ledger::Ledger;
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
    Submit,
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
}

/// A GL context that records its calls and draws nothing.
#[derive(Debug)]
pub struct StandIn {
    ledger: Ledger,
    calls: Vec<Call>,
    next_name: u32,
    raised: Option<ErrorKind>,
    failing: Vec<(Op, ErrorKind)>,
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
        }
    }

    /// The calls so far.
    pub fn calls(&self) -> &[Call] {
        &self.calls
    }

    /// Forgets the calls so far.
    pub fn clear_calls(&mut self) {
        self.calls.clear();
    }

    /// How many textures, targets and programs are live.
    pub fn live(&self) -> (usize, usize, usize) {
        self.ledger.live()
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
        self.failing.push((op, kind));
    }

    /// A new name for `op`, or the failure a test asked for.
    fn make(&mut self, op: Op) -> Result<u32, Error> {
        if let Some(i) = self.failing.iter().position(|(o, _)| *o == op) {
            let (_, kind) = self.failing.remove(i);
            return Err(Error::new(kind, format!("{op:?} failed for the test")));
        }
        Ok(self.name())
    }

    fn name(&mut self) -> u32 {
        self.next_name = self.next_name.saturating_add(1);
        self.next_name
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
        let t = self.ledger.create_texture(format, size, || name)?;
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
        self.ledger
            .fill_texture(texture, rect, pixels.len(), stride, |_| Ok(()))?;
        self.calls.push(Call::FillTexture(rect));
        Ok(())
    }

    fn free_texture(&mut self, texture: Texture) -> Result<(), Error> {
        self.ledger.free_texture(texture, |_| Ok(()))?;
        self.calls.push(Call::FreeTexture);
        Ok(())
    }

    fn create_frame(&mut self, size: Size) -> Result<DrawTarget, Error> {
        let names = self.make(Op::CreateFrame).map(|n| (n, n));
        let t = self.ledger.create_frame(size, || names)?;
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
        self.ledger.free_target(target, |_, _| Ok(()))?;
        self.calls.push(Call::FreeTarget);
        Ok(())
    }

    fn compile(&mut self, source: &Source<'_>) -> Result<Program, Error> {
        let name = self.make(Op::Compile);
        let p = self.ledger.compile(source, || name)?;
        self.calls.push(Call::Compile(source.fragment_text()));
        Ok(p)
    }

    fn free_program(&mut self, program: Program) -> Result<(), Error> {
        self.ledger.free_program(program, |_| Ok(()))?;
        self.calls.push(Call::FreeProgram);
        Ok(())
    }

    fn clear(&mut self, target: &DrawTarget, rect: Rect, color: u32) -> Result<(), Error> {
        self.ledger.target(target)?;
        self.calls.push(Call::Clear(rect, color));
        Ok(())
    }

    fn draw(&mut self, draw: &Draw<'_>) -> Result<(), Error> {
        let checked = self.ledger.draw(draw)?;
        self.calls.push(Call::Draw {
            quads: draw.quads.to_vec(),
            blend: draw.blend,
            inputs: checked.inputs.iter().map(|(n, _)| *n).collect(),
        });
        Ok(())
    }

    fn submit(&mut self, target: &DrawTarget) -> Result<(), Error> {
        self.ledger.target(target)?;
        self.calls.push(Call::Submit);
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
