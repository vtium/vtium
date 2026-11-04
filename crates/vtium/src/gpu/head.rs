//! A head's GPU path on its render thread (ADR-1303 D9, D11 part 6).
//!
//! A head whose `hwaccel` is on makes one GL context on its render thread,
//! on the render node of the card that drives its first display, and keeps
//! a [`Pipeline`] for each of its displays. Each frame is drawn through the
//! pipeline into a frame of the display's size kept in GL, and the part of
//! it the display's buffer lacks at its age is read back into that buffer,
//! which the display lent in memory, as the CPU path's buffer is. That read
//! is the staging copy ADR-1401 D2 names for a display whose buffer the
//! context does not import; every display takes it until the displays lend
//! their buffers as dma-bufs.
//!
//! A set-up that fails at any step leaves the head on the CPU path, and the
//! loop logs the step. A frame that fails is drawn on the CPU path whole;
//! an out-of-memory error or a lost context also ends the GPU path for the
//! head until vtium restarts, since GL's state is undefined after either
//! (ADR-1303 D9).

use std::collections::BTreeMap;
use std::path::PathBuf;

use vtium_platform::display::DisplayId;
use vtium_platform::gpu as platform;
use vtium_render::gpu::{Config, DrawTarget, Error, ErrorKind, Gl, Pipeline, Size};
use vtium_render::{Depth, DrawState, Mapped, Orientation, Rect, Renderer, Stats, Target};
use vtium_term::Frame;

use super::PlatformGl;
use crate::font::DisplayFace;
use crate::painter::GpuSource;

/// One display's frames on the GPU path: its pipeline, and the frame in GL
/// that each frame is presented into and read back from.
struct Display {
    pipeline: Pipeline,
    staging: DrawTarget,
}

/// A head's GL context and its displays' pipelines, on its render thread.
pub struct HeadGpu {
    gl: PlatformGl,
    displays: BTreeMap<(PathBuf, DisplayId), Display>,
    /// A rectangle read back, RGBA8.
    read: Vec<u8>,
    /// One row as display pixels, `0x00RRGGBB`.
    pixels: Vec<u32>,
    /// An out-of-memory error or a lost context ended the GPU path.
    lost: bool,
}

impl HeadGpu {
    /// The head's GL context from `source`, or what failed, for the log.
    pub fn new(source: &GpuSource) -> Result<HeadGpu, String> {
        let made = match source {
            GpuSource::RenderNode(node) => platform::Context::on_render_node(node),
            GpuSource::Software => platform::Context::software(),
        };
        let context = made.map_err(|e| e.to_string())?;
        Ok(HeadGpu {
            gl: PlatformGl::new(context),
            displays: BTreeMap::new(),
            read: Vec::new(),
            pixels: Vec::new(),
            lost: false,
        })
    }

    /// The driver's name for the GPU, for the log.
    pub fn renderer(&self) -> &str {
        self.gl.context().renderer()
    }

    /// Whether a failure ended the GPU path, so the head draws on the CPU
    /// path until vtium restarts.
    pub fn lost(&self) -> bool {
        self.lost
    }

    /// Draws `frame` for the display `key` through its pipeline, and reads
    /// what `target`, the display's buffer `age` frames old, lacks back into
    /// it. Gives the damage and the figures, or `None` for a display the
    /// GPU path does not draw yet: a 1-bit one, whose depth present does not
    /// reduce (ADR-1303 D6). A frame that fails leaves the buffer as it
    /// was, and the error says what failed; one that loses the context or
    /// runs out of memory ends the GPU path.
    #[expect(
        clippy::too_many_arguments,
        reason = "the display, its renderer and frame, its buffer and age, and the frame's state"
    )]
    pub fn draw(
        &mut self,
        key: &(PathBuf, DisplayId),
        renderer: &mut Renderer<DisplayFace>,
        frame: &Frame<'_>,
        (target, size): (&mut Mapped<'_>, (u32, u32)),
        orientation: Orientation,
        age: u32,
        state: &DrawState,
    ) -> Option<Result<(Vec<Rect>, Stats), Error>> {
        if renderer.depth() == Depth::One {
            return None;
        }
        let drawn = self.draw_through(
            key,
            renderer,
            frame,
            (target, size),
            orientation,
            age,
            state,
        );
        if let Err(e) = &drawn
            && matches!(e.kind, ErrorKind::OutOfMemory | ErrorKind::ContextLost)
        {
            self.lost = true;
        }
        Some(drawn)
    }

    #[expect(clippy::too_many_arguments, reason = "as draw's")]
    fn draw_through(
        &mut self,
        key: &(PathBuf, DisplayId),
        renderer: &mut Renderer<DisplayFace>,
        frame: &Frame<'_>,
        (target, size): (&mut Mapped<'_>, (u32, u32)),
        orientation: Orientation,
        age: u32,
        state: &DrawState,
    ) -> Result<(Vec<Rect>, Stats), Error> {
        let size = Size::new(size.0, size.1);
        let gl = &mut self.gl;
        if self
            .displays
            .get(key)
            .is_some_and(|d| d.pipeline.size() != size)
            && let Some(old) = self.displays.remove(key)
        {
            // a display whose mode changed: its frame goes, and the next is
            // made at the new size; an error freeing it is the driver's to
            // report at the next check
            let _ = old.pipeline.free(gl);
            let _ = gl.free_target(old.staging);
        }
        let display = match self.displays.entry(key.clone()) {
            std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::btree_map::Entry::Vacant(e) => {
                let pipeline = Pipeline::turned(gl, size, orientation, Config::default())?;
                let staging = match gl.create_frame(size) {
                    Ok(t) => t,
                    Err(err) => {
                        let _ = pipeline.free(gl);
                        return Err(err);
                    }
                };
                e.insert(Display { pipeline, staging })
            }
        };
        display.pipeline.turn(gl, orientation)?;
        // The staging frame holds every frame drawn, so it lacks only what
        // this frame composes; present still gives what a buffer `age`
        // frames old lacks, which is what the read copies.
        let drawn = renderer.draw_gpu(
            gl,
            &mut display.pipeline,
            frame,
            &display.staging,
            age,
            state,
            &[],
        )?;
        let stats = drawn.stats;
        let damage = drawn.damage.to_vec();
        for r in &damage {
            read_into(
                gl,
                &display.staging,
                *r,
                (&mut self.read, &mut self.pixels),
                target,
            )?;
        }
        Ok((damage, stats))
    }
}

/// Reads `r` of `staging` back, in one read, and writes it into `target`
/// a row at a time, as display pixels.
fn read_into(
    gl: &mut PlatformGl,
    staging: &DrawTarget,
    r: Rect,
    (read, pixels): (&mut Vec<u8>, &mut Vec<u32>),
    target: &mut Mapped<'_>,
) -> Result<(), Error> {
    let width = usize::try_from(r.width).unwrap_or(0);
    let height = usize::try_from(r.height).unwrap_or(0);
    read.resize(width.saturating_mul(height).saturating_mul(4), 0);
    gl.read(staging, r, read)?;
    for (y, line) in (r.y..).zip(read.chunks_exact(width.saturating_mul(4).max(1))) {
        pixels.clear();
        pixels.extend(
            line.as_chunks::<4>()
                .0
                .iter()
                .map(|&[red, green, blue, _]| {
                    u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue)
                }),
        );
        target.write(r.x, y, pixels);
    }
    Ok(())
}
