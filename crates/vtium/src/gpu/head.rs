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
//! loop logs the step. During a run the failures are sorted by their kind
//! (ADR-1303 D9): a frame a GL error fails is dropped from the GPU path and
//! drawn on the CPU path whole, and the next frame on the GPU path draws
//! whole too; a GL error in a second frame running, an out-of-memory
//! error or a lost context ends the GPU path for the head until vtium
//! restarts, since GL's state cannot be trusted after them.

use std::collections::BTreeMap;
use std::path::PathBuf;

use vtium_platform::display::DisplayId;
use vtium_platform::gpu as platform;
use vtium_render::gpu::{Config, DrawTarget, Error, ErrorKind, Gl, Pipeline, Size};
use vtium_render::{Depth, DrawState, Mapped, Orientation, Rect, Renderer, Stats, Target};
use vtium_term::Frame;

use super::PlatformGl;
use crate::font::DisplayFace;
use crate::painter::{Fault, GpuSource, TestGpu};

/// One display's frames on the GPU path: its pipeline, and the frame in GL
/// that each frame is presented into and read back from.
struct Display {
    pipeline: Pipeline,
    staging: DrawTarget,
}

impl Display {
    /// Frees the pipeline, with its upright frame and glyph pools, and the
    /// staging frame. An error freeing them is the driver's to report at
    /// the next check.
    fn free(self, gl: &mut PlatformGl) {
        let _ = self.pipeline.free(gl);
        let _ = gl.free_target(self.staging);
    }
}

/// A head's GL context and its displays' pipelines, on its render thread.
pub struct HeadGpu {
    gl: PlatformGl,
    displays: BTreeMap<(PathBuf, DisplayId), Display>,
    /// A rectangle read back, RGBA8.
    read: Vec<u8>,
    /// One row as display pixels, `0x00RRGGBB`.
    pixels: Vec<u32>,
    /// The frames running that a GL error failed.
    failed_in_a_row: u32,
    /// Why the GPU path ended, once a failure ended it.
    lost: Option<String>,
    /// The test watching the GPU path, which queues its faults and reads
    /// how many displays it keeps.
    test: Option<std::sync::Arc<TestGpu>>,
}

impl HeadGpu {
    /// The head's GL context from `source`, or what failed, for the log.
    pub fn new(source: &GpuSource) -> Result<HeadGpu, String> {
        let made = match source {
            GpuSource::RenderNode(node) => platform::Context::on_render_node(node),
            GpuSource::Software | GpuSource::Test(_) => platform::Context::software(),
        };
        let test = match source {
            GpuSource::Test(t) => Some(std::sync::Arc::clone(t)),
            _ => None,
        };
        let context = made.map_err(|e| e.to_string())?;
        Ok(HeadGpu {
            gl: PlatformGl::new(context),
            displays: BTreeMap::new(),
            read: Vec::new(),
            pixels: Vec::new(),
            failed_in_a_row: 0,
            lost: None,
            test,
        })
    }

    /// The driver's name for the GPU, for the log.
    pub fn renderer(&self) -> &str {
        self.gl.context().renderer()
    }

    /// Why a failure ended the GPU path, once one did: the head draws on
    /// the CPU path until vtium restarts.
    pub fn lost(&self) -> Option<&str> {
        self.lost.as_deref()
    }

    /// The display `key` left the head: what the GPU path kept for it is
    /// freed, and a display that comes back with the same key starts anew.
    pub fn forget(&mut self, key: &(PathBuf, DisplayId)) {
        if let Some(display) = self.displays.remove(key) {
            display.free(&mut self.gl);
        }
        self.count();
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
        if renderer.depth() == Depth::One || self.lost.is_some() {
            return None;
        }
        self.raise_a_queued_fault();
        let drawn = self.draw_through(
            key,
            renderer,
            frame,
            (target, size),
            orientation,
            age,
            state,
        );
        self.count();
        match &drawn {
            Ok(_) => self.failed_in_a_row = 0,
            Err(e) => {
                self.failed_in_a_row = self.failed_in_a_row.saturating_add(1);
                if matches!(e.kind, ErrorKind::OutOfMemory | ErrorKind::ContextLost) {
                    self.lost = Some(e.to_string());
                } else if self.failed_in_a_row >= 2 {
                    self.lost = Some(format!("a GL error in two frames running, the last {e}"));
                }
            }
        }
        Some(drawn)
    }

    /// Tells a test watching how many displays the GPU path keeps frames
    /// for.
    fn count(&self) {
        if let Some(test) = &self.test {
            test.kept
                .store(self.displays.len(), std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Makes the context fail the frame's next call with the fault a test
    /// queued first, if any.
    fn raise_a_queued_fault(&mut self) {
        let Some(fault) = self
            .test
            .as_ref()
            .and_then(|t| t.faults.lock().ok().and_then(|mut q| q.pop_front()))
        else {
            return;
        };
        self.gl.raise(match fault {
            Fault::Gl => platform::ErrorKind::Gl(0x0502),
            Fault::OutOfMemory => platform::ErrorKind::OutOfMemory,
            Fault::ContextLost => platform::ErrorKind::ContextLost,
        });
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
            // made at the new size
            old.free(gl);
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
        // A pipeline is made at the display's orientation, and draw_gpu
        // turns it to the renderer's as each frame starts, which is the
        // view's: the view takes both from one configuration.
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
