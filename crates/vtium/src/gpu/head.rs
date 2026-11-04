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
use crate::term::OnGpuFrame;

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
    /// Each display's frames running that a GL error failed.
    failed_in_a_row: BTreeMap<(PathBuf, DisplayId), u32>,
    /// The displays whose set-up failed, at the size it failed at: the CPU
    /// path draws each until its mode changes (ADR-1303 D9).
    refused: BTreeMap<(PathBuf, DisplayId), Size>,
    /// Why the GPU path ended, once a failure ended it.
    lost: Option<String>,
    /// The test watching the GPU path, which queues its faults and reads
    /// how many displays it keeps.
    test: Option<std::sync::Arc<TestGpu>>,
    /// A test's fault for the frame: the context is lost at its read back.
    lose_at_read: bool,
}

impl Drop for HeadGpu {
    /// Waits while a test holds the drop, standing in for a driver that
    /// hangs as its context is destroyed.
    fn drop(&mut self) {
        if let Some(test) = &self.test {
            let _held = test.hold_drop.lock();
        }
    }
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
            failed_in_a_row: BTreeMap::new(),
            refused: BTreeMap::new(),
            lost: None,
            test,
            lose_at_read: false,
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
        self.failed_in_a_row.remove(key);
        self.refused.remove(key);
        self.count();
    }

    /// Draws `frame` for the display `key` through its pipeline, and reads
    /// what `target`, the display's buffer `age` frames old, lacks back into
    /// it, and says how that went. `None` is a display the GPU path does
    /// not draw: a 1-bit one, whose depth present does not reduce yet
    /// (ADR-1303 D6), one whose set-up failed at this size, which the CPU
    /// path draws until its mode changes (D9), or any display once the GPU
    /// path ended. A frame that fails may leave part of it in the buffer,
    /// when its read back fails partway. A lost context, an out-of-memory
    /// error or a GL error in two of the display's frames running ends the
    /// GPU path.
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
    ) -> Option<OnGpuFrame> {
        let size = Size::new(size.0, size.1);
        if renderer.depth() == Depth::One
            || self.lost.is_some()
            || self.refused.get(key) == Some(&size)
        {
            return None;
        }
        if let Err(e) = self.set_up(key, size, orientation) {
            self.count();
            if !matches!(e.kind, ErrorKind::OutOfMemory | ErrorKind::ContextLost) {
                self.refused.insert(key.clone(), size);
                return Some(OnGpuFrame::Refused(e.to_string()));
            }
            self.lost = Some(e.to_string());
            return Some(OnGpuFrame::Failed(e.to_string()));
        }
        self.refused.remove(key);
        self.raise_a_queued_fault(key);
        let drawn = self.draw_through(key, renderer, frame, target, age, state);
        self.count();
        match drawn {
            Ok((damage, stats)) => {
                self.failed_in_a_row.remove(key);
                Some(OnGpuFrame::Drawn(damage, stats))
            }
            Err(e) => {
                let failed = self.failed_in_a_row.entry(key.clone()).or_default();
                *failed = failed.saturating_add(1);
                if matches!(e.kind, ErrorKind::OutOfMemory | ErrorKind::ContextLost) {
                    self.lost = Some(e.to_string());
                } else if *failed >= 2 {
                    self.lost = Some(format!(
                        "a GL error in two of a display's frames running, the last {e}"
                    ));
                }
                Some(OnGpuFrame::Failed(e.to_string()))
            }
        }
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
    fn raise_a_queued_fault(&mut self, key: &(PathBuf, DisplayId)) {
        let Some(fault) = self
            .test
            .as_ref()
            .filter(|t| {
                t.faulty_display
                    .lock()
                    .is_ok_and(|d| d.is_none_or(|id| id == key.1))
            })
            .and_then(|t| t.faults.lock().ok().and_then(|mut q| q.pop_front()))
        else {
            return;
        };
        self.gl.raise(match fault {
            Fault::Gl => platform::ErrorKind::Gl(0x0502),
            Fault::OutOfMemory => platform::ErrorKind::OutOfMemory,
            Fault::ContextLost => platform::ErrorKind::ContextLost,
            Fault::LostAtRead => {
                self.lose_at_read = true;
                return;
            }
        });
    }

    /// Makes the display `key`'s pipeline and staging frame at `size` and
    /// `orientation`, unless it has them at that size; a display whose mode
    /// changed gives up its old ones first.
    fn set_up(
        &mut self,
        key: &(PathBuf, DisplayId),
        size: Size,
        orientation: Orientation,
    ) -> Result<(), Error> {
        let gl = &mut self.gl;
        match self.displays.get(key) {
            Some(d) if d.pipeline.size() == size => return Ok(()),
            Some(_) => {
                // a display whose mode changed: its frame goes, and the next
                // is made at the new size
                if let Some(old) = self.displays.remove(key) {
                    old.free(gl);
                }
            }
            None => {}
        }
        if let Some(test) = &self.test {
            if let Ok(mut tries) = test.set_ups.lock() {
                tries.push(key.1);
            }
            if test.refuse_set_up.lock().is_ok_and(|r| r.contains(&key.1)) {
                return Err(Error::new(
                    ErrorKind::Limit,
                    "the test refuses the display's set-up",
                ));
            }
        }
        let pipeline = Pipeline::turned(gl, size, orientation, Config::default())?;
        let staging = match gl.create_frame(size) {
            Ok(t) => t,
            Err(err) => {
                let _ = pipeline.free(gl);
                return Err(err);
            }
        };
        self.displays
            .insert(key.clone(), Display { pipeline, staging });
        Ok(())
    }

    fn draw_through(
        &mut self,
        key: &(PathBuf, DisplayId),
        renderer: &mut Renderer<DisplayFace>,
        frame: &Frame<'_>,
        target: &mut Mapped<'_>,
        age: u32,
        state: &DrawState,
    ) -> Result<(Vec<Rect>, Stats), Error> {
        let gl = &mut self.gl;
        let Some(display) = self.displays.get_mut(key) else {
            return Err(Error::new(
                ErrorKind::Misuse,
                "a display drawn before its set-up",
            ));
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
        if std::mem::take(&mut self.lose_at_read) {
            gl.raise(platform::ErrorKind::ContextLost);
        }
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
