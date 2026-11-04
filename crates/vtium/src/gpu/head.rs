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
//! whole too; a GL error in two of one display's frames in a row, an
//! out-of-memory error or a lost context ends the GPU path for the head
//! until vtium restarts, since GL's state cannot be trusted after them. A
//! display's set-up counts the same way. A display the GPU path cannot set
//! up at its size for any other reason, such as a size past the GPU's
//! limits or a shader that does not compile, holds the whole head on the
//! CPU path while the display keeps that size: the head keeps its context
//! and each other display's pipeline, and draws on the GPU path again once
//! the display takes a size the GPU path sets up or leaves the head.

use std::collections::BTreeMap;
use std::path::PathBuf;

use vtium_platform::display::DisplayId;
use vtium_platform::gpu as platform;
use vtium_render::gpu::{Config, DrawTarget, Error, ErrorKind, Gl, Pipeline, Size};
use vtium_render::{Depth, DrawState, Mapped, Orientation, Rect, Renderer, Stats, Target};
use vtium_term::Frame;

use super::PlatformGl;
use crate::font::DisplayFace;
use crate::painter::{Fault, GpuSource, SetUpFault, TestGpu};
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
    /// Each display's frames in a row that a GL error failed, its set-ups
    /// included.
    failed_in_a_row: BTreeMap<(PathBuf, DisplayId), u32>,
    /// The displays whose set-up failed for a reason the same size meets
    /// again, at that size: while any is here, the whole head draws on the
    /// CPU path (ADR-1303 D9).
    refused: BTreeMap<(PathBuf, DisplayId), Size>,
    /// The head came back to the GPU path, as the last refused display
    /// took a new size or left, and the loop has not been told.
    back: bool,
    /// Why the GPU path ended, once a failure ended it.
    lost: Option<String>,
    /// The test watching the GPU path, which queues its faults and reads
    /// how many displays it keeps.
    test: Option<std::sync::Arc<TestGpu>>,
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
            back: false,
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
    /// A head the display held on the CPU path draws on the GPU path again
    /// once no other display holds it.
    pub fn forget(&mut self, key: &(PathBuf, DisplayId)) {
        if let Some(display) = self.displays.remove(key) {
            display.free(&mut self.gl);
        }
        self.failed_in_a_row.remove(key);
        self.let_go(key);
        self.count();
    }

    /// The display `key` no longer holds the head on the CPU path.
    fn let_go(&mut self, key: &(PathBuf, DisplayId)) {
        if self.refused.remove(key).is_some() && self.refused.is_empty() {
            self.back = true;
        }
    }

    /// Whether the head came back to the GPU path since the last call, so
    /// the loop logs its displays' lines again (ADR-1303 D10).
    pub fn take_back(&mut self) -> bool {
        std::mem::take(&mut self.back)
    }

    /// Draws `frame` for the display `key` through its pipeline, and reads
    /// what `target`, the display's buffer `age` frames old, lacks back into
    /// it, and says how that went. `None` is a frame the GPU path does not
    /// draw: a 1-bit display's, whose depth present does not reduce yet
    /// (ADR-1303 D6), any frame while a display of the head is at a size
    /// the GPU path could not set up (D9), and any frame once the GPU path
    /// ended. A display refused at one size is set up again at its next.
    /// A frame that fails may leave part of it in the buffer, when its read
    /// back fails partway. A lost context, an out-of-memory error or a GL
    /// error in two of the display's frames in a row ends the GPU path.
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
        if renderer.depth() == Depth::One || self.lost.is_some() {
            return None;
        }
        if self.refused.get(key).is_some_and(|&at| at != size) {
            self.let_go(key);
        }
        if !self.refused.is_empty() {
            return None;
        }
        if let Err(e) = self.set_up(key, size, orientation) {
            self.count();
            return Some(match e.kind {
                ErrorKind::Gl | ErrorKind::OutOfMemory | ErrorKind::ContextLost => {
                    self.failed(key, &e)
                }
                _ => {
                    self.refused.insert(key.clone(), size);
                    self.back = false;
                    OnGpuFrame::Refused(e.to_string())
                }
            });
        }
        let lose_at_read = self.raise_a_queued_fault(key);
        let drawn = self.draw_through(key, renderer, frame, target, age, state, lose_at_read);
        self.count();
        match drawn {
            Ok((damage, stats)) => {
                self.failed_in_a_row.remove(key);
                if let Some(test) = &self.test {
                    test.drawn.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                Some(OnGpuFrame::Drawn(damage, stats))
            }
            Err(e) => Some(self.failed(key, &e)),
        }
    }

    /// A frame of the display `key`, or its set-up, failed with `e`: a lost
    /// context, an out-of-memory error or a second GL error in a row of
    /// the display's frames ends the GPU path.
    fn failed(&mut self, key: &(PathBuf, DisplayId), e: &Error) -> OnGpuFrame {
        let failed = self.failed_in_a_row.entry(key.clone()).or_default();
        *failed = failed.saturating_add(1);
        if matches!(e.kind, ErrorKind::OutOfMemory | ErrorKind::ContextLost) {
            self.lost = Some(e.to_string());
        } else if *failed >= 2 {
            self.lost = Some(format!(
                "a GL error in two of a display's frames in a row, the last {e}"
            ));
        }
        OnGpuFrame::Failed(e.to_string())
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
    /// queued first, if any. Gives whether that fault is a lost context at
    /// the frame's read back, which this frame alone meets.
    fn raise_a_queued_fault(&mut self, key: &(PathBuf, DisplayId)) -> bool {
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
            return false;
        };
        self.gl.raise(match fault {
            Fault::Gl => platform::ErrorKind::Gl(0x0502),
            Fault::OutOfMemory => platform::ErrorKind::OutOfMemory,
            Fault::ContextLost => platform::ErrorKind::ContextLost,
            Fault::LostAtRead => return true,
        });
        false
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
            let fault = if test.refuse_set_up.lock().is_ok_and(|r| r.contains(&key.1)) {
                Some(SetUpFault::Limit)
            } else {
                test.set_up_faults
                    .lock()
                    .ok()
                    .and_then(|mut q| q.pop_front())
            };
            match fault {
                None => {}
                Some(SetUpFault::Limit) => {
                    return Err(Error::new(
                        ErrorKind::Limit,
                        "the test's GPU is too small for the display",
                    ));
                }
                // the set-up's first call to GL fails with it
                Some(SetUpFault::Compile) => gl.raise(platform::ErrorKind::Compile),
                Some(SetUpFault::Gl) => gl.raise(platform::ErrorKind::Gl(0x0502)),
                Some(SetUpFault::OutOfMemory) => gl.raise(platform::ErrorKind::OutOfMemory),
                Some(SetUpFault::ContextLost) => gl.raise(platform::ErrorKind::ContextLost),
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

    /// Draws the frame through the display's pipeline and reads it back;
    /// `lose_at_read` is a test's lost context at the read back.
    #[expect(clippy::too_many_arguments, reason = "as draw's, and the test's fault")]
    fn draw_through(
        &mut self,
        key: &(PathBuf, DisplayId),
        renderer: &mut Renderer<DisplayFace>,
        frame: &Frame<'_>,
        target: &mut Mapped<'_>,
        age: u32,
        state: &DrawState,
        lose_at_read: bool,
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
        if lose_at_read {
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
