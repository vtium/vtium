//! A head's render thread (ADR-1401 D2): it composes the head's frames,
//! each straight into the buffer its display lent in a token, so one head's
//! redraw delays neither another head's frames nor the loop, which keeps
//! the input, the ptys, the devices and their flips. A frame depends only
//! on its head's screen and drawing, so the frames are what one thread
//! would draw.
//!
//! The loop sends a [`Job`] and takes back a [`Composed`] at its next
//! turn, woken by the thread. Each display has at most one frame on its
//! way at a time ([`crate::term::View::prepare`]); a draw asked for
//! meanwhile waits on the loop's side and is taken from the screen as it
//! is when the frame comes back, so a newer frame always replaces the
//! one pending and none queues behind another. Both channels hold
//! [`QUEUE`] frames: a job that finds its queue full goes back to its
//! view, which draws at the next turn, and the thread waits while the
//! loop has not taken what it composed.
//!
//! Each head's compose times are logged at debug level once a second while
//! it draws ([`Figures`]).
//!
//! A display that leaves the head, and the head's VT being released, are
//! sent on the same channel as the frames, after the frames sent before
//! them: the GPU path frees what it kept for the display, or gives back its
//! textures while another session has the VT. A full queue takes them at
//! the loop's next turn that finds room, and always before a later frame.
//!
//! A head whose `hwaccel` is on, in a build with `gpu`, draws on the GPU
//! path (ADR-1303 D9): the thread makes the head's GL context before it
//! takes a frame, from the [`GpuSource`] the loop gives it, and then tells
//! the loop which [`Path`] the head draws on, or why the GPU path could not
//! start. [`Painter::start`] does not wait for it: the loop takes the
//! answer at the turn the thread wakes it, so a driver that hangs making
//! the context holds no input, no pty and no other head. Frames sent
//! before the answer wait in the queue and are composed on the path the
//! thread chose. A head whose context cannot be made draws on the CPU
//! path.
//!
//! The thread's stack is [`GPU_STACK`] when the loop gives it a
//! [`GpuSource`], whether or not the GPU path then starts, and [`STACK`]
//! otherwise. The thread ends when its head is dropped, as at a new head
//! file and at the exit, and the drop waits for it. A thread on the CPU
//! path is joined; a thread given a [`GpuSource`] is waited for at most
//! [`LET_GO_MS`], since a driver can hang destroying its context, and past
//! that it is left to end alone and the log says the GPU driver did not
//! let go. A panic on the thread ends the process, as one on the loop does
//! (D10): under `panic=abort` at once, and in a test when the loop finds
//! the thread gone.
//!
//! A thread left to end alone is still vtium's: [`wait_for_left`] joins it
//! once it ends, and says how many still run. A process must not run its
//! exit handlers while one does, because libEGL's and Mesa's handlers tear
//! EGL down under the thread, which corrupts the heap or aborts. So vtium
//! ends without its exit handlers while a left thread runs, and a test that
//! leaves one joins it before it returns.

use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{
    Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError, sync_channel,
};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use vtium_platform::display::DisplayId;

use crate::term::{Composed, Job};

/// The render thread's stack, in bytes: the vtium tests compose in 64 KiB
/// in a debug build and in 16 KiB optimised.
pub const STACK: usize = 128 * 1024;

/// The stack of a render thread that makes a GL context, in bytes: the
/// driver compiles the shaders and runs its draws on the calling thread,
/// and Mesa's llvmpipe compiles them through LLVM, which overflows
/// [`STACK`]. 8 MiB is a main thread's stack on Linux; the memory is
/// reserved, and only what the thread touches is used.
pub const GPU_STACK: usize = 8 * 1024 * 1024;

/// The frames each of a render thread's two channels holds.
pub const QUEUE: usize = 16;

/// Milliseconds the drop of a head waits for a render thread that was
/// given a [`GpuSource`] to end: the frame it composes waits at most
/// vtium-platform's `GPU_FENCE_WAIT_MS`, 2000 ms, on its fence, and the
/// rest is for the driver to destroy the head's GL context. Past it the
/// thread is left to end alone, and the log says the GPU driver did not let
/// go (the README's bounds table).
pub const LET_GO_MS: u64 = 3000;

/// Where a head's render thread makes its GL context.
#[derive(Clone, Debug)]
pub enum GpuSource {
    /// The render node of the card that drives the head's first display.
    RenderNode(PathBuf),
    /// Mesa's llvmpipe with no device: the tests' GPU.
    Software,
    /// Mesa's llvmpipe, which a test watches and makes fail.
    Test(std::sync::Arc<TestGpu>),
}

/// What a test sees of, and does to, a head's GPU path on
/// [`GpuSource::Test`].
#[derive(Debug, Default)]
pub struct TestGpu {
    /// The faults the GPU raises, one a frame, in order: the tests of what
    /// a head does with each kind of failure (ADR-1303 D9).
    pub faults: std::sync::Mutex<std::collections::VecDeque<Fault>>,
    /// How many displays the head's GPU path keeps frames for, as the
    /// render thread last counted them.
    pub kept: std::sync::atomic::AtomicUsize,
    /// Held by a test, it keeps the head's GPU path from being dropped, as
    /// a driver that hangs in eglDestroyContext would.
    pub hold_drop: std::sync::Mutex<()>,
    /// A lock the next head's context set-up waits on before it makes the
    /// context, as a driver that hangs in eglInitialize would, while the
    /// test holds it. The first set-up takes it, so a later head's set-up
    /// does not wait.
    pub set_up_gate: std::sync::Mutex<Option<std::sync::Arc<std::sync::Mutex<()>>>>,
    /// The textures, targets and programs the head's context holds, as the
    /// render thread last counted them.
    pub live: std::sync::Mutex<(usize, usize, usize)>,
    /// The VT releases the head's GPU path gave its textures back at.
    pub releases: std::sync::atomic::AtomicUsize,
    /// The display whose frames alone raise the queued faults; every
    /// display's when `None`.
    pub faulty_display: std::sync::Mutex<Option<vtium_platform::display::DisplayId>>,
    /// The displays whose every set-up the GPU path fails as past the
    /// GPU's limits.
    pub refuse_set_up:
        std::sync::Mutex<std::collections::BTreeSet<vtium_platform::display::DisplayId>>,
    /// The GPU's largest renderbuffer, which a test sets in place of the
    /// context's: a display wider or taller is past the GPU's limits, and
    /// the head's own check refuses its set-up (ADR-1303 D9). The
    /// context's when `None`.
    pub widest: std::sync::Mutex<Option<u32>>,
    /// The failures the GPU path's next set-ups meet, one a set-up, in
    /// order, whichever display it sets up.
    pub set_up_faults: std::sync::Mutex<std::collections::VecDeque<SetUpFault>>,
    /// Each set-up of a display the GPU path tried, by its key, in order.
    pub set_ups: std::sync::Mutex<Vec<(PathBuf, vtium_platform::display::DisplayId)>>,
    /// The frames the GPU path drew into a display's buffer.
    pub drawn: std::sync::atomic::AtomicUsize,
}

/// A failure a test makes the GPU raise in a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// A GL error other than out of memory.
    Gl,
    OutOfMemory,
    ContextLost,
    /// A lost context at the frame's read back into the display's buffer,
    /// after the GPU drew the frame.
    LostAtRead,
}

/// A failure a test makes a display's set-up on the GPU path meet
/// (ADR-1303 D9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetUpFault {
    /// The display is past the GPU's limits.
    Limit,
    /// A shader does not compile.
    Compile,
    /// A GL error other than out of memory.
    Gl,
    OutOfMemory,
    ContextLost,
}

/// The path that draws a head's frames, which the log names (ADR-1303
/// D10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Path {
    /// The CPU path: the renderer composes into the display's buffer.
    #[default]
    Cpu,
    /// The GPU path: the head's GL context draws the frames.
    Gpu,
}

impl Path {
    /// The name the display's log line gives the path.
    pub fn name(self) -> &'static str {
        match self {
            Path::Cpu => "cpu",
            Path::Gpu => "gpu",
        }
    }

    /// The path that draws a display of `depth` on a head on this path:
    /// the CPU path draws a 1-bit display, whose depth the GPU path's
    /// present does not reduce yet (ADR-1303 D6).
    pub fn for_depth(self, depth: vtium_render::Depth) -> Path {
        if depth == vtium_render::Depth::One {
            Path::Cpu
        } else {
            self
        }
    }
}

/// How the render thread's set-up went: the path the head draws on, and,
/// when the GPU path was asked for and could not start, why.
type Ready = (Path, Option<String>);

/// What the loop sends a head's render thread, taken in the order sent.
enum Message {
    /// A frame to compose.
    Frame(Box<Job>),
    /// A display that left the head: the GPU path frees what it kept for
    /// it.
    Forget((PathBuf, DisplayId)),
    /// The head's VT was released: the GPU path gives back its textures
    /// and keeps its context and programs.
    Release,
}

/// A head's render thread, and the channels to it and back.
pub struct Painter {
    /// The head's name, for the log.
    name: String,
    jobs: Option<SyncSender<Message>>,
    /// The messages other than frames that a full queue has not taken yet,
    /// the oldest first.
    pending: VecDeque<Message>,
    done: Option<Receiver<Composed>>,
    thread: Option<JoinHandle<()>>,
    /// The thread's set-up, until the loop has taken it.
    ready: Option<Receiver<Ready>>,
    /// The path the thread chose, once the loop has taken its set-up.
    path: Option<Path>,
    /// The loop took the set-up and has not logged its displays' lines.
    told: bool,
    /// A display of the head is at a size the GPU path could not set up,
    /// so the head draws on the CPU path for now (ADR-1303 D9).
    held_on_cpu: bool,
    /// The GPU path's notes for the log, such as glyph textures at their
    /// ceiling, which the loop logs.
    notes: Option<Receiver<String>>,
    /// Ends when the thread does, for a thread given a [`GpuSource`]: the
    /// drop waits on it for at most [`LET_GO_MS`].
    ended: Option<Receiver<()>>,
    /// Set as the thread ends, after everything it held has gone, under
    /// [`LEFT`]'s lock: what [`wait_for_left`] waits on once the thread is
    /// left to end alone.
    gone: Arc<AtomicBool>,
    /// The log said the head waits for its GPU set-up.
    told_waiting: bool,
}

impl Painter {
    /// Starts the render thread of the head `name`; `wake` wakes the loop
    /// after its set-up and after each frame composed. With `gpu`, the
    /// thread first makes the head's GL context there. The call does not
    /// wait for it: the head's [`Path`] is known once [`Painter::take`]
    /// has taken the thread's set-up.
    pub fn start(
        name: &str,
        gpu: Option<GpuSource>,
        wake: impl Fn() + Send + 'static,
    ) -> io::Result<Painter> {
        let (jobs, todo) = sync_channel::<Message>(QUEUE);
        let (composed, done) = sync_channel::<Composed>(QUEUE);
        let (ready, set_up) = sync_channel::<Ready>(1);
        let (note, notes) = sync_channel::<String>(QUEUE);
        let (end, ended) = sync_channel::<()>(0);
        let on_gpu = gpu.is_some();
        let gone = Arc::new(AtomicBool::new(false));
        let ends = Arc::clone(&gone);
        let thread = std::thread::Builder::new()
            .name(format!("render {name}"))
            .stack_size(if on_gpu { GPU_STACK } else { STACK })
            .spawn(move || {
                draw_frames(gpu, &ready, &todo, &composed, &note, &wake);
                // after everything the thread held, the GL context among
                // them, has gone
                drop(end);
                let _left = left();
                ends.store(true, Ordering::SeqCst);
                ENDED.notify_all();
            })?;
        Ok(Painter {
            name: name.to_owned(),
            jobs: Some(jobs),
            pending: VecDeque::new(),
            done: Some(done),
            thread: Some(thread),
            ready: Some(set_up),
            path: None,
            told: false,
            held_on_cpu: false,
            notes: Some(notes),
            ended: on_gpu.then_some(ended),
            gone,
            told_waiting: false,
        })
    }

    /// The path the head's frames are drawn on: the CPU path while the
    /// loop has not taken the thread's set-up.
    pub fn path(&self) -> Path {
        if self.held_on_cpu {
            Path::Cpu
        } else {
            self.path.unwrap_or_default()
        }
    }

    /// Whether the loop has taken the thread's set-up, so [`Painter::path`]
    /// names the path that draws the head.
    pub fn known(&self) -> bool {
        self.path.is_some()
    }

    /// Whether the loop took the thread's set-up since the last call, so it
    /// logs each of the head's display lines, naming the path (ADR-1303
    /// D10).
    pub fn take_known(&mut self) -> bool {
        std::mem::take(&mut self.told)
    }

    /// Records that the head's GPU path ended during the run: it draws on
    /// the CPU path from now on.
    pub fn left_gpu(&mut self) {
        self.path = Some(Path::Cpu);
        self.held_on_cpu = false;
        self.pending.clear();
    }

    /// Records whether a display of the head is at a size the GPU path
    /// could not set up, which holds the head on the CPU path while it
    /// lasts (ADR-1303 D9). The head keeps its GL context meanwhile, and
    /// the thread is still told of each display that leaves.
    pub fn hold_on_cpu(&mut self, held: bool) {
        self.held_on_cpu = held;
    }

    /// Whether the thread may hold a GL context: the head is on the GPU
    /// path, or its set-up is not known yet.
    fn may_hold_gpu(&self) -> bool {
        self.path != Some(Path::Cpu)
    }

    /// The display `key` left the head. On the GPU path the thread frees
    /// what it kept for the display once it has composed the frames sent
    /// before.
    pub fn forget(&mut self, key: (PathBuf, DisplayId)) {
        if self.may_hold_gpu() {
            self.pending.push_back(Message::Forget(key));
            self.send_pending();
        }
    }

    /// The head's VT was released. On the GPU path the thread gives back
    /// each display's textures and frames once it has composed the frames
    /// sent before, keeping the context and the compiled programs
    /// (ADR-1303 D9), so the head holds no textures while another session
    /// has the VT.
    pub fn release(&mut self) {
        if self.may_hold_gpu() {
            self.pending.push_back(Message::Release);
            self.send_pending();
        }
    }

    /// Sends the messages a full queue has not taken, the oldest first,
    /// while the queue takes them. Gives whether none is left to send.
    fn send_pending(&mut self) -> bool {
        let Some(jobs) = &self.jobs else {
            return true;
        };
        while let Some(message) = self.pending.pop_front() {
            match jobs.try_send(message) {
                Ok(()) => {}
                Err(TrySendError::Full(message)) => {
                    self.pending.push_front(message);
                    return false;
                }
                // a thread that ended keeps nothing to free
                Err(TrySendError::Disconnected(_)) => {
                    self.pending.clear();
                    return true;
                }
            }
        }
        true
    }

    /// Sends `job` to be composed, after the messages waiting; a full
    /// queue gives it back.
    ///
    /// The first frame sent to a head given a [`GpuSource`] while its
    /// set-up is not known yet makes the log say the head waits for the
    /// GPU driver, once: a driver that never finishes the set-up leaves the
    /// head drawing nothing, and that line is what says why.
    pub fn send(&mut self, job: Box<Job>) -> Option<Box<Job>> {
        self.take_set_up();
        if self.path.is_none() && self.ended.is_some() && !self.told_waiting {
            self.told_waiting = true;
            log::info!(
                "head {}: waiting for the GPU driver to set up the head's GL context: the head draws nothing until it has",
                self.name
            );
        }
        if !self.send_pending() {
            return Some(job);
        }
        let jobs = self.jobs.as_ref()?;
        match jobs.try_send(Message::Frame(job)) {
            Err(
                TrySendError::Full(Message::Frame(job))
                | TrySendError::Disconnected(Message::Frame(job)),
            ) => Some(job),
            Ok(()) | Err(_) => None,
        }
    }

    /// The frames composed since the last call, in the order composed,
    /// once the thread's set-up is taken, and the messages waiting sent.
    /// The log gets why the GPU path could not start, once, and the GPU
    /// path's notes. A thread that ended by a panic passes the panic on.
    pub fn take(&mut self) -> Vec<Composed> {
        self.take_set_up();
        self.send_pending();
        let mut frames = Vec::new();
        if let Some(done) = &self.done {
            loop {
                match done.try_recv() {
                    Ok(frame) => frames.push(frame),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if let Some(Err(panic)) = self.thread.take().map(JoinHandle::join) {
                            std::panic::resume_unwind(panic);
                        }
                        break;
                    }
                }
            }
        }
        // the thread sends its set-up before its first frame
        if !frames.is_empty() {
            self.take_set_up();
        }
        if let Some(notes) = &self.notes {
            for note in notes.try_iter() {
                log::warn!("head {}: {note}", self.name);
            }
        }
        frames
    }

    /// Takes the thread's set-up, if it came: the path the head draws on,
    /// and why the GPU path asked for could not start, which the log says.
    /// A thread that ended before its set-up draws nothing, on the CPU
    /// path.
    fn take_set_up(&mut self) {
        let Some(ready) = &self.ready else {
            return;
        };
        let (path, refused) = match ready.try_recv() {
            Ok(set_up) => set_up,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => (Path::Cpu, None),
        };
        self.ready = None;
        self.path = Some(path);
        self.told = true;
        if let Some(why) = refused {
            log::warn!("head {}: {why}: it draws on the CPU path", self.name);
        }
    }
}

/// The render thread: its GPU path's set-up, sent back as `ready`, then
/// each frame of `todo` composed, sent back on `composed`, and the loop
/// woken.
#[cfg(feature = "gpu")]
fn draw_frames(
    gpu: Option<GpuSource>,
    ready: &SyncSender<Ready>,
    todo: &Receiver<Message>,
    composed: &SyncSender<Composed>,
    notes: &SyncSender<String>,
    wake: &impl Fn(),
) {
    use crate::gpu::head::HeadGpu;
    let mut head = None;
    let mut refused = None;
    if let Some(source) = &gpu {
        match HeadGpu::new(source) {
            Ok(h) => head = Some(h),
            Err(why) => refused = Some(why),
        }
    }
    let path = if head.is_some() { Path::Gpu } else { Path::Cpu };
    let _ = ready.send((path, refused));
    wake();
    for message in todo {
        let job = match message {
            Message::Frame(job) => job,
            Message::Forget(key) => {
                if let Some(h) = &mut head {
                    h.forget(&key);
                }
                continue;
            }
            Message::Release => {
                if let Some(h) = &mut head {
                    h.release();
                    send_notes(h, notes);
                }
                continue;
            }
        };
        let mut frame = job.compose(head.as_mut());
        if let Some(h) = &mut head {
            send_notes(h, notes);
        }
        // ADR-1303 D9: the head stays on the CPU path until vtium restarts,
        // and the loop says so once. The frame goes back before the GL
        // context is dropped, so a driver that hangs destroying it holds
        // no frame.
        if let Some(h) = &mut head {
            frame.back_on_gpu = h.take_back();
        }
        let ended = head
            .as_ref()
            .and_then(HeadGpu::lost)
            .map(str::to_owned)
            .and_then(|why| {
                frame.left_gpu = Some(why);
                head.take()
            });
        if composed.send(frame).is_err() {
            break;
        }
        wake();
        drop(ended);
    }
}

/// Sends the GPU path's notes for the log to the loop; a note that finds
/// the channel full is dropped, as the loop has not taken the last ones.
#[cfg(feature = "gpu")]
fn send_notes(head: &mut crate::gpu::head::HeadGpu, notes: &SyncSender<String>) {
    for note in head.take_notes() {
        let _ = notes.try_send(note);
    }
}

/// The render thread in a build without `gpu`: every head on the CPU path.
#[cfg(not(feature = "gpu"))]
fn draw_frames(
    _: Option<GpuSource>,
    ready: &SyncSender<Ready>,
    todo: &Receiver<Message>,
    composed: &SyncSender<Composed>,
    _: &SyncSender<String>,
    wake: &impl Fn(),
) {
    let _ = ready.send((Path::Cpu, None));
    wake();
    for message in todo {
        let job = match message {
            Message::Frame(job) => job,
            // the CPU path keeps nothing for a display, and holds no GPU
            Message::Forget(_key) => continue,
            Message::Release => continue,
        };
        if composed.send(job.compose(None)).is_err() {
            break;
        }
        wake();
    }
}

impl Painter {
    /// Tells the thread to end once its frame composing is done.
    fn end(&mut self) {
        self.jobs = None;
        self.done = None;
    }

    /// Waits for the thread [`Painter::end`] told to end, and joins it. A
    /// thread given a [`GpuSource`] is waited for at most `within`: past it
    /// the GPU driver has not let go of the head's GL context, and the
    /// thread is left to end alone, which the log says.
    fn wait_to_end(&mut self, within: Duration) {
        let Some(thread) = self.thread.take() else {
            return;
        };
        let let_go = self.ended.as_ref().map(|e| e.recv_timeout(within));
        match let_go {
            Some(Err(RecvTimeoutError::Timeout)) => {
                log::warn!(
                    "head {}: the GPU driver did not let go of the head's GL context within {LET_GO_MS} ms: vtium goes on without it",
                    self.name
                );
                left().push(Left {
                    thread,
                    gone: Arc::clone(&self.gone),
                });
            }
            Some(Ok(()) | Err(RecvTimeoutError::Disconnected)) | None => {
                let _ = thread.join();
            }
        }
    }
}

/// A render thread vtium went on without, as [`Painter::wait_to_end`]
/// left it, and the flag the thread sets as it ends. The thread owns its
/// head's GL context; nothing here holds a GL object.
struct Left {
    thread: JoinHandle<()>,
    gone: Arc<AtomicBool>,
}

/// The render threads vtium went on without that [`wait_for_left`] has not
/// joined yet. The list is process-wide, because vtium's exit asks after
/// every head's thread, those of the heads a head file reload dropped
/// included.
static LEFT: Mutex<Vec<Left>> = Mutex::new(Vec::new());

/// Notified, under [`LEFT`]'s lock, as each render thread ends.
static ENDED: Condvar = Condvar::new();

/// The render threads left, locked. A thread that panicked holding the
/// lock left the list whole, since each change to it is one push or one
/// replacement of the whole list.
fn left() -> std::sync::MutexGuard<'static, Vec<Left>> {
    LEFT.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Waits at most `within`, from when `clock` says the wait starts, for the
/// render threads vtium has gone on without to end, joins each that has,
/// and gives how many still run. While one runs, it may be inside the GPU
/// driver, and the process must not run its exit handlers (the module's
/// doc). The wait releases the list's lock, so a head let go of meanwhile,
/// and another caller, do not wait on it.
pub fn wait_for_left(within: Duration, clock: &dyn Fn() -> Instant) -> usize {
    let deadline = clock().checked_add(within);
    let mut threads = left();
    loop {
        let running = threads.iter().any(|l| !l.gone.load(Ordering::SeqCst));
        let rest = deadline.map_or(within, |d| d.saturating_duration_since(clock()));
        if !running || rest.is_zero() {
            break;
        }
        threads = match ENDED.wait_timeout(threads, rest) {
            Ok((guard, _)) => guard,
            Err(poisoned) => poisoned.into_inner().0,
        };
    }
    let (ended, running): (Vec<Left>, Vec<Left>) = std::mem::take(&mut *threads)
        .into_iter()
        .partition(|l| l.gone.load(Ordering::SeqCst));
    *threads = running;
    let count = threads.len();
    drop(threads);
    for l in ended {
        let _ = l.thread.join();
    }
    count
}

impl Drop for Painter {
    /// Ends the thread once its frame composing is done, and waits for it,
    /// a thread given a [`GpuSource`] at most [`LET_GO_MS`].
    fn drop(&mut self) {
        self.end();
        self.wait_to_end(Duration::from_millis(LET_GO_MS));
    }
}

/// Lets go of every one of `painters` at once, as when a head file reload
/// or vtium's exit drops every head: each thread is told to end first, and
/// then all of them are waited for against one deadline, [`LET_GO_MS`]
/// after `clock` says the wait starts. Heads whose GPU driver hangs
/// destroying their contexts hold vtium [`LET_GO_MS`] in all, not that for
/// each head.
pub fn let_go(painters: impl IntoIterator<Item = Painter>, clock: &dyn Fn() -> Instant) {
    let mut painters: Vec<Painter> = painters.into_iter().collect();
    for painter in &mut painters {
        painter.end();
    }
    let bound = Duration::from_millis(LET_GO_MS);
    let deadline = clock().checked_add(bound);
    for mut painter in painters {
        let left = deadline.map_or(bound, |d| d.saturating_duration_since(clock()));
        painter.wait_to_end(left);
    }
}

/// A head's frame times since its last line: the frames presented, and
/// the time each took to compose into its display's buffer, logged at
/// debug level at most once a second while the head draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Figures {
    frames: u32,
    compose: (Duration, Duration),
    /// When the next line is due, once a frame came since the last.
    pub due: Option<Instant>,
}

impl Figures {
    /// A frame presented at `now`, which took `compose`.
    pub fn frame(&mut self, compose: Duration, now: Instant) {
        self.frames = self.frames.saturating_add(1);
        let (sum, max) = &mut self.compose;
        *sum = sum.saturating_add(compose);
        *max = (*max).max(compose);
        if self.due.is_none() && log::log_enabled!(target: "render", log::Level::Debug) {
            self.due = now.checked_add(Duration::from_secs(1));
        }
    }

    /// The line for the head `name` once it is due at `now`, the figures
    /// starting again.
    pub fn line(&mut self, name: &str, now: Instant) -> Option<String> {
        self.due.filter(|&d| d <= now)?;
        let f = std::mem::take(self);
        let mean = |sum: Duration| sum.checked_div(f.frames.max(1)).unwrap_or_default();
        Some(format!(
            "head {name}: {} frames, compose mean {} max {} us",
            f.frames,
            mean(f.compose.0).as_micros(),
            f.compose.1.as_micros(),
        ))
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "a test's clock is the host's, its stand-in draw sleeps, and a test fails by panicking"
)]
mod tests {
    use std::any::Any;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    use vtium_font::{Font, Unifont};
    use vtium_platform::display::{
        DisplayId, Dpms, Event, Memory, Rect, SwapError, Swapped, Token,
    };
    use vtium_render::{Config, DrawState};
    use vtium_term::Terminal;

    use super::*;
    use crate::blink::Blink;
    use crate::head::DrawHook;
    use crate::sync::Sync;
    use crate::term::{Prepared, Term, View};
    use crate::video::Video;

    /// A display in memory, of the size it holds.
    struct Small(Arc<Mutex<Vec<u8>>>, (u32, u32));

    impl Video for Small {
        fn backend(&self) -> &'static str {
            "small"
        }
        fn events(&mut self) -> Vec<Event> {
            Vec::new()
        }
        fn displays(&self) -> Vec<DisplayId> {
            Vec::new()
        }
        fn name(&self, _: DisplayId) -> String {
            String::new()
        }
        fn size(&self, _: DisplayId) -> Option<(u32, u32)> {
            Some(self.1)
        }
        fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
            None
        }
        fn lend(&mut self, id: DisplayId) -> Option<Token> {
            let memory: Arc<Memory> = self.0.clone();
            Some(Token::new(
                id,
                self.1,
                self.1.0.saturating_mul(4),
                0,
                memory,
            ))
        }
        fn give_back(&mut self, _: Token) {}
        fn swap(&mut self, _: Token, _: &[Rect]) -> Result<Swapped, SwapError> {
            Ok(Swapped::Frame)
        }
        fn is_flipping(&self, _: DisplayId) -> bool {
            false
        }
        fn ready_for_frame(&self, id: DisplayId) -> bool {
            !self.is_flipping(id)
        }
        fn set_dpms(&mut self, _: DisplayId, _: Dpms) -> io::Result<()> {
            Ok(())
        }
        fn sleep(&mut self) {}
        fn wake_up(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn unblank(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn input(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn hotplug(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn as_any(&mut self) -> &mut dyn Any {
            self
        }
    }

    /// `n` jobs, a display of 32 by 16 pixels each, every one running
    /// `hook` before it composes.
    fn jobs(n: u32, hook: &DrawHook) -> Vec<Job> {
        jobs_at(n, (32, 16), hook)
    }

    /// `n` jobs, a display of `size` each, every one running `hook` before
    /// it composes.
    fn jobs_at(n: u32, size: (u32, u32), hook: &DrawHook) -> Vec<Job> {
        let mut term = Term::new(
            Terminal::new(4, 1, 0),
            Sync::new(Duration::ZERO, 0),
            Blink::new(false),
            Rc::new(Instant::now),
        );
        let bytes = usize::try_from(size.0.saturating_mul(size.1).saturating_mul(4)).unwrap();
        let mut video = Small(Arc::new(Mutex::new(vec![0; bytes])), size);
        let mut snapshot = None;
        (0..n)
            .map(|i| {
                let face: crate::font::DisplayFace = Box::new(Unifont::new(16));
                let mut view = View::new(
                    DisplayId(i),
                    Font::new(face),
                    size,
                    None,
                    Config::default(),
                    false,
                );
                let key = (PathBuf::from("/dev/test"), DisplayId(i));
                let state = DrawState::default();
                match view.prepare(
                    &key,
                    &mut term,
                    &mut snapshot,
                    &mut video,
                    &state,
                    Some(hook),
                    &Instant::now,
                ) {
                    Prepared::Job(job) => *job,
                    Prepared::Later | Prepared::Nothing => panic!("a job"),
                }
            })
            .collect()
    }

    /// The path the head draws on, once the loop has taken the render
    /// thread's set-up.
    fn set_up(painter: &mut Painter) -> Path {
        let end = Instant::now().checked_add(Duration::from_secs(10)).unwrap();
        while !painter.known() && Instant::now() < end {
            assert!(painter.take().is_empty(), "a frame before any was sent");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(painter.take_known(), "the set-up came");
        painter.path()
    }

    // ADR-1401 D2's bounded channel: while the thread composes one frame,
    // the queue takes QUEUE more, and gives the next back to its view;
    // every frame taken comes back composed, in order.
    #[test]
    fn a_full_queue_gives_the_job_back() {
        let (started, wait_started) = channel();
        let (release, wait_release) = channel::<()>();
        let (started, wait_release) = (Mutex::new(started), Mutex::new(wait_release));
        let hook: DrawHook = Arc::new(move || {
            let _ = started.lock().unwrap().send(());
            let _ = wait_release.lock().unwrap().recv();
        });
        let mut painter = Painter::start("test", None, || {}).unwrap();
        let mut all = jobs(u32::try_from(QUEUE).unwrap() + 2, &hook).into_iter();
        assert!(painter.send(Box::new(all.next().unwrap())).is_none());
        wait_started.recv().unwrap();
        for _ in 0..QUEUE {
            assert!(
                painter.send(Box::new(all.next().unwrap())).is_none(),
                "queued"
            );
        }
        let back = painter.send(Box::new(all.next().unwrap()));
        // released before any assertion, so a failing one joins the thread
        drop(release);
        let back = back.expect("the queue is full");
        assert_eq!(back.key.1, DisplayId(u32::try_from(QUEUE).unwrap() + 1));
        let end = Instant::now() + Duration::from_secs(5);
        let mut done = Vec::new();
        while done.len() < QUEUE + 1 && Instant::now() < end {
            done.extend(painter.take().into_iter().map(|c| c.key.1));
            std::thread::sleep(Duration::from_millis(5));
        }
        let ids: Vec<DisplayId> = (0..=u32::try_from(QUEUE).unwrap()).map(DisplayId).collect();
        assert_eq!(done, ids);
    }

    // A display that leaves while the queue is full is sent once the queue
    // has room, at the loop's next take, after the frames sent before it:
    // the GPU path frees what it kept for the display, counted in the
    // displays it keeps, with no later frame sent
    #[cfg(feature = "gpu")]
    #[test]
    fn a_display_that_leaves_while_the_queue_is_full_goes_once_the_queue_has_room() {
        let (started, wait_started) = channel();
        let (release, wait_release) = channel::<()>();
        let (started, wait_release) = (Mutex::new(started), Mutex::new(wait_release));
        let hook: DrawHook = Arc::new(move || {
            let _ = started.lock().unwrap().send(());
            let _ = wait_release.lock().unwrap().recv();
        });
        let gpu = Arc::new(TestGpu::default());
        let mut painter =
            Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
        assert_eq!(set_up(&mut painter), Path::Gpu, "the head on the GPU path");
        let mut all = jobs(u32::try_from(QUEUE).unwrap() + 1, &hook).into_iter();
        assert!(painter.send(Box::new(all.next().unwrap())).is_none());
        wait_started.recv().unwrap();
        for _ in 0..QUEUE {
            assert!(
                painter.send(Box::new(all.next().unwrap())).is_none(),
                "queued"
            );
        }
        painter.forget((PathBuf::from("/dev/test"), DisplayId(1)));
        drop(release);
        let end = Instant::now().checked_add(Duration::from_secs(10)).unwrap();
        let mut done = 0;
        while (done < QUEUE + 1 || gpu.kept.load(Ordering::SeqCst) > QUEUE) && Instant::now() < end
        {
            done += painter.take().len();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(done, QUEUE + 1, "the frames composed");
        assert_eq!(gpu.kept.load(Ordering::SeqCst), QUEUE, "display 1 freed");
    }

    // ADR-1303 D9: the frame that ends the GPU path comes back to the
    // loop before the head's GL context is dropped, so a driver that hangs
    // in the drop holds no frame
    #[cfg(feature = "gpu")]
    #[test]
    fn the_frame_that_ends_the_gpu_path_comes_back_before_the_context_goes() {
        let gpu = Arc::new(TestGpu::default());
        let mut painter =
            Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
        let held = gpu.hold_drop.lock().unwrap();
        gpu.faults.lock().unwrap().push_back(Fault::ContextLost);
        let hook: DrawHook = Arc::new(|| {});
        assert!(painter.send(Box::new(jobs(1, &hook).remove(0))).is_none());
        let end = Instant::now().checked_add(Duration::from_secs(10)).unwrap();
        let mut back = Vec::new();
        while back.is_empty() && Instant::now() < end {
            back = painter.take();
            std::thread::sleep(Duration::from_millis(5));
        }
        // released before any assertion, so a failing one joins the thread
        drop(held);
        assert_eq!(back.len(), 1, "the frame came back while the drop hung");
        assert!(
            back.first().is_some_and(|c| c.left_gpu.is_some()),
            "it carries why the path ended"
        );
    }

    /// Sends a frame of each of `n` displays, `rounds` times, each round
    /// once the last came back, and gives every frame composed.
    #[cfg(feature = "gpu")]
    fn rounds(painter: &mut Painter, n: u32, rounds: usize) -> Vec<Composed> {
        let hook: DrawHook = Arc::new(|| {});
        let mut all = Vec::new();
        for _ in 0..rounds {
            for job in jobs(n, &hook) {
                assert!(painter.send(Box::new(job)).is_none());
            }
            let end = Instant::now().checked_add(Duration::from_secs(10)).unwrap();
            let mut back = Vec::new();
            while back.len() < usize::try_from(n).unwrap() && Instant::now() < end {
                back.extend(painter.take());
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(
                back.len(),
                usize::try_from(n).unwrap(),
                "every frame came back"
            );
            all.extend(back);
        }
        all
    }

    // ADR-1303 D9: a display the GPU path cannot set up at its size puts
    // its whole head on the CPU path, with no try at each frame, and says so
    // once; once the display leaves, the head draws on the GPU path again
    // and the frame that does so says it
    #[cfg(feature = "gpu")]
    #[test]
    fn a_display_the_gpu_cannot_set_up_holds_its_whole_head_on_the_cpu_path_until_it_leaves() {
        let gpu = Arc::new(TestGpu::default());
        gpu.refuse_set_up.lock().unwrap().insert(DisplayId(1));
        let mut painter =
            Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
        let frames = rounds(&mut painter, 2, 3);
        let refused: Vec<DisplayId> = frames
            .iter()
            .filter(|c| c.refused_gpu.is_some())
            .map(|c| c.key.1)
            .collect();
        assert_eq!(refused, [DisplayId(1)], "said once, for display 1");
        assert!(
            frames
                .iter()
                .all(|c| c.left_gpu.is_none() && c.failed_gpu.is_none() && !c.back_on_gpu)
        );
        let tries: Vec<DisplayId> = gpu.set_ups.lock().unwrap().iter().map(|k| k.1).collect();
        assert_eq!(tries, [DisplayId(0), DisplayId(1)], "each set up once");
        assert_eq!(
            gpu.drawn.load(Ordering::SeqCst),
            1,
            "only display 0's first frame drawn on the GPU path"
        );
        painter.forget((PathBuf::from("/dev/test"), DisplayId(1)));
        let back = rounds(&mut painter, 1, 2);
        let said: Vec<bool> = back.iter().map(|c| c.back_on_gpu).collect();
        assert_eq!(said, [true, false], "the first frame after says it");
        assert_eq!(
            gpu.drawn.load(Ordering::SeqCst),
            3,
            "display 0 on the GPU path"
        );
    }

    // ADR-1303 D9: a display's set-up on the GPU path that fails is sorted
    // by its kind: out of memory or a lost context ends the head's GPU path;
    // a GL error is one of the display's failed frames, and the next frame
    // sets it up again; past the GPU's limits or a shader that does not
    // compile holds the head on the CPU path at that size
    #[cfg(feature = "gpu")]
    #[test]
    fn a_failed_set_up_is_sorted_by_its_kind() {
        /// What each of a display's frames said: failed, left, refused.
        fn said(c: &Composed) -> (bool, bool, bool) {
            (
                c.failed_gpu.is_some(),
                c.left_gpu.is_some(),
                c.refused_gpu.is_some(),
            )
        }
        /// The set-ups' faults; what each of three frames said; how many
        /// set-ups were tried and frames drawn on the GPU path.
        type Case = (&'static [SetUpFault], [(bool, bool, bool); 3], usize, usize);
        let cases: [Case; 6] = [
            (
                &[SetUpFault::OutOfMemory],
                [
                    (true, true, false),
                    (false, false, false),
                    (false, false, false),
                ],
                1,
                0,
            ),
            (
                &[SetUpFault::ContextLost],
                [
                    (true, true, false),
                    (false, false, false),
                    (false, false, false),
                ],
                1,
                0,
            ),
            (
                &[SetUpFault::Gl],
                [
                    (true, false, false),
                    (false, false, false),
                    (false, false, false),
                ],
                2,
                2,
            ),
            (
                &[SetUpFault::Gl, SetUpFault::Gl],
                [
                    (true, false, false),
                    (true, true, false),
                    (false, false, false),
                ],
                2,
                0,
            ),
            (
                &[SetUpFault::Limit],
                [
                    (false, false, true),
                    (false, false, false),
                    (false, false, false),
                ],
                1,
                0,
            ),
            (
                &[SetUpFault::Compile],
                [
                    (false, false, true),
                    (false, false, false),
                    (false, false, false),
                ],
                1,
                0,
            ),
        ];
        for (faults, want, set_ups, drawn) in cases {
            let gpu = Arc::new(TestGpu::default());
            gpu.set_up_faults.lock().unwrap().extend(faults);
            let mut painter =
                Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
            let frames = rounds(&mut painter, 1, 3);
            let got: Vec<(bool, bool, bool)> = frames.iter().map(said).collect();
            assert_eq!(got, want, "{faults:?}");
            assert_eq!(
                gpu.set_ups.lock().unwrap().len(),
                set_ups,
                "{faults:?}: set-ups tried"
            );
            assert_eq!(
                gpu.drawn.load(Ordering::SeqCst),
                drawn,
                "{faults:?}: frames drawn on the GPU path"
            );
        }
    }

    // ADR-1303 D9: a hold on the CPU path breaks a display's row of GL
    // errors, since the CPU path drew its frames in between: an error
    // before the hold and one after it do not end the GPU path
    #[cfg(feature = "gpu")]
    #[test]
    fn a_hold_on_the_cpu_path_breaks_a_displays_row_of_gl_errors() {
        let gpu = Arc::new(TestGpu::default());
        *gpu.faulty_display.lock().unwrap() = Some(DisplayId(0));
        gpu.faults.lock().unwrap().extend([Fault::Gl, Fault::Gl]);
        gpu.refuse_set_up.lock().unwrap().insert(DisplayId(1));
        let mut painter =
            Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
        let held = rounds(&mut painter, 2, 2);
        painter.forget((PathBuf::from("/dev/test"), DisplayId(1)));
        let back = rounds(&mut painter, 1, 2);
        assert!(gpu.faults.lock().unwrap().is_empty(), "both errors raised");
        let failed = held
            .iter()
            .chain(&back)
            .filter(|c| c.failed_gpu.is_some())
            .count();
        assert_eq!(failed, 2, "display 0's frames failed");
        assert!(
            held.iter().chain(&back).all(|c| c.left_gpu.is_none()),
            "the second error ended the GPU path"
        );
        assert_eq!(gpu.drawn.load(Ordering::SeqCst), 1, "the last frame drawn");
    }

    // ADR-1303 D9: a GL error in two of one display's frames in a row ends
    // the GPU path, though another display's frame drew between them
    #[cfg(feature = "gpu")]
    #[test]
    fn a_gl_error_in_two_of_a_displays_frames_in_a_row_ends_the_gpu_path() {
        let gpu = Arc::new(TestGpu::default());
        *gpu.faulty_display.lock().unwrap() = Some(DisplayId(0));
        gpu.faults.lock().unwrap().extend([Fault::Gl, Fault::Gl]);
        let mut painter =
            Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
        let frames = rounds(&mut painter, 2, 2);
        let failed: Vec<DisplayId> = frames
            .iter()
            .filter(|c| c.failed_gpu.is_some())
            .map(|c| c.key.1)
            .collect();
        assert_eq!(
            failed,
            [DisplayId(0), DisplayId(0)],
            "display 0's frames failed"
        );
        assert!(
            frames.iter().any(|c| c.left_gpu.is_some()),
            "the second ended the GPU path"
        );
    }

    // ADR-1303 D9: a lone GL error every 100 frames ages out of the
    // display's window of 60 frames before the next, so the head stays on
    // the GPU path however long the run
    #[cfg(feature = "gpu")]
    #[test]
    fn a_lone_gl_error_every_100_frames_never_ends_the_gpu_path() {
        let gpu = Arc::new(TestGpu::default());
        let mut painter =
            Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
        let mut frames = Vec::new();
        for frame in 0..301 {
            if frame % 100 == 0 {
                gpu.faults.lock().unwrap().push_back(Fault::Gl);
            }
            frames.extend(rounds(&mut painter, 1, 1));
        }
        let failed = frames.iter().filter(|c| c.failed_gpu.is_some()).count();
        assert_eq!(failed, 4, "each error failed its frame");
        assert!(
            frames.iter().all(|c| c.left_gpu.is_none()),
            "the path ended"
        );
        assert_eq!(
            gpu.drawn.load(Ordering::SeqCst),
            297,
            "frames on the GPU path"
        );
    }

    /// Sends one frame of a display of `size`, and gives it composed.
    #[cfg(feature = "gpu")]
    fn one_at(painter: &mut Painter, size: (u32, u32)) -> Composed {
        let hook: DrawHook = Arc::new(|| {});
        assert!(
            painter
                .send(Box::new(jobs_at(1, size, &hook).remove(0)))
                .is_none()
        );
        let end = Instant::now().checked_add(Duration::from_secs(10)).unwrap();
        let mut back = Vec::new();
        while back.is_empty() && Instant::now() < end {
            back = painter.take();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(back.len(), 1, "the frame came back");
        back.remove(0)
    }

    // ADR-1303 D9: a display whose buffer passes the GPU's largest
    // renderbuffer holds its head on the CPU path, refused before anything
    // is made for it, with the reason naming the limit; at a size within
    // it the head draws on the GPU path again
    #[cfg(feature = "gpu")]
    #[test]
    fn a_display_past_the_gpus_largest_renderbuffer_is_refused_before_its_set_up() {
        let gpu = Arc::new(TestGpu::default());
        *gpu.widest.lock().unwrap() = Some(40);
        let mut painter =
            Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
        assert_eq!(set_up(&mut painter), Path::Gpu);
        let wide = one_at(&mut painter, (64, 16));
        let why = wide.refused_gpu.unwrap_or_default();
        assert!(
            why.contains("its 64x16 buffer passes the GPU's largest renderbuffer, 40,"),
            "{why}"
        );
        assert_eq!(*gpu.live.lock().unwrap(), (0, 0, 0), "nothing made");
        assert_eq!(gpu.drawn.load(Ordering::SeqCst), 0);
        let fits = one_at(&mut painter, (32, 16));
        assert!(fits.back_on_gpu && fits.refused_gpu.is_none());
        assert_eq!(gpu.drawn.load(Ordering::SeqCst), 1, "drawn on the GPU path");
    }

    // ADR-1303 D9, vtium/vtium#286: a display refused at one size and at
    // its next is let go and refused again in the same frame, which counts
    // no trip back to the GPU path. Refused at 64 pixels wide, then at 48
    // and 64 by turns four times, the head goes back to the GPU path once,
    // when the display takes a size the GPU sets up, and its GPU path does
    // not end
    #[cfg(feature = "gpu")]
    #[test]
    fn a_let_go_and_a_refusal_in_the_same_frame_count_no_trip_back() {
        let gpu = Arc::new(TestGpu::default());
        *gpu.widest.lock().unwrap() = Some(40);
        let mut painter =
            Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
        assert_eq!(set_up(&mut painter), Path::Gpu);
        let said: Vec<(u32, bool, bool, bool)> = [64, 48, 64, 48, 64, 32, 32]
            .into_iter()
            .map(|width| {
                let c = one_at(&mut painter, (width, 16));
                (
                    width,
                    c.refused_gpu.is_some(),
                    c.back_on_gpu,
                    c.left_gpu.is_some(),
                )
            })
            .collect();
        assert_eq!(
            said,
            [
                (64, true, false, false),
                (48, true, false, false),
                (64, true, false, false),
                (48, true, false, false),
                (64, true, false, false),
                (32, false, true, false),
                (32, false, false, false),
            ]
        );
        assert_eq!(
            gpu.drawn.load(Ordering::SeqCst),
            2,
            "the head on the GPU path"
        );
    }

    // ADR-1303 D9, vtium/vtium#244: at a VT release the GPU path gives back
    // every texture and frame each display holds, and keeps the context's
    // compiled programs and each display's pipeline; the next frames make
    // them again with no display set up anew, and draw on the GPU path
    #[cfg(feature = "gpu")]
    #[test]
    fn a_release_gives_back_every_texture_and_keeps_the_programs() {
        let gpu = Arc::new(TestGpu::default());
        let mut painter =
            Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
        assert_eq!(set_up(&mut painter), Path::Gpu);
        rounds(&mut painter, 2, 1);
        let (textures, targets, programs) = *gpu.live.lock().unwrap();
        assert!(
            textures > 0 && targets > 0 && programs > 0,
            "drawn on the GPU"
        );
        painter.release();
        let end = Instant::now().checked_add(Duration::from_secs(10)).unwrap();
        while gpu.releases.load(Ordering::SeqCst) == 0 && Instant::now() < end {
            assert!(painter.take().is_empty());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(gpu.releases.load(Ordering::SeqCst), 1, "released");
        assert_eq!(
            *gpu.live.lock().unwrap(),
            (0, 0, programs),
            "only the programs held"
        );
        assert_eq!(gpu.kept.load(Ordering::SeqCst), 2, "both pipelines kept");
        rounds(&mut painter, 2, 1);
        assert_eq!(
            *gpu.live.lock().unwrap(),
            (textures, targets, programs),
            "made again as the first frames made them"
        );
        assert_eq!(gpu.drawn.load(Ordering::SeqCst), 4, "drawn on the GPU");
        assert_eq!(
            gpu.set_ups.lock().unwrap().len(),
            2,
            "no display set up anew"
        );
    }

    // A head on the CPU path sends nothing at a release: it holds no GPU
    #[test]
    fn a_release_on_the_cpu_path_sends_nothing() {
        let mut painter = Painter::start("test", None, || {}).unwrap();
        assert_eq!(set_up(&mut painter), Path::Cpu);
        painter.release();
        assert!(painter.pending.is_empty());
        let hook: DrawHook = Arc::new(|| {});
        assert!(painter.send(Box::new(jobs(1, &hook).remove(0))).is_none());
    }

    // vtium/vtium#283: Painter::start returns while the render thread's GPU
    // set-up hangs, as a driver that hangs making its context would. The
    // head's path is not known meanwhile, its frame waits in the queue, and
    // the thread that started it, which stands for the loop, starts another
    // head on the CPU path and takes its frames. Once the set-up ends, the
    // frame waiting is composed on the GPU path and the loop learns the
    // path
    #[cfg(feature = "gpu")]
    #[test]
    fn start_returns_while_the_gpu_set_up_hangs_and_its_frames_wait_for_it() {
        let gpu = Arc::new(TestGpu::default());
        let gate = Arc::new(Mutex::new(()));
        *gpu.set_up_gate.lock().unwrap() = Some(Arc::clone(&gate));
        let held = gate.lock().unwrap();
        let (said, heard) = channel::<&'static str>();
        let (open, opened) = channel::<()>();
        let watched = Arc::clone(&gpu);
        let lp = std::thread::spawn(move || {
            let source = GpuSource::Test(Arc::clone(&watched));
            let mut painter = Painter::start("test", Some(source), || {}).unwrap();
            said.send("started").unwrap();
            let hook: DrawHook = Arc::new(|| {});
            assert!(painter.send(Box::new(jobs(1, &hook).remove(0))).is_none());
            let mut other = Painter::start("other", None, || {}).unwrap();
            assert_eq!(set_up(&mut other), Path::Cpu);
            assert_eq!(rounds(&mut other, 1, 3).len(), 3, "the other head drew");
            assert!(painter.take().is_empty(), "a frame before the set-up");
            assert!(!painter.known(), "the path known before the set-up");
            assert_eq!(painter.path(), Path::Cpu);
            said.send("waiting").unwrap();
            opened.recv().unwrap();
            let end = Instant::now().checked_add(Duration::from_secs(10)).unwrap();
            let mut back = Vec::new();
            while back.is_empty() && Instant::now() < end {
                back = painter.take();
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(back.len(), 1, "the frame waiting came back");
            assert!(painter.take_known(), "the loop told the path");
            assert_eq!(painter.path(), Path::Gpu);
            assert_eq!(watched.drawn.load(Ordering::SeqCst), 1, "on the GPU path");
        });
        let started = heard.recv_timeout(Duration::from_secs(10));
        let waiting = heard.recv_timeout(Duration::from_secs(30));
        drop(held);
        let _ = open.send(());
        let joined = lp.join();
        assert_eq!(started, Ok("started"), "start waited for the set-up");
        assert_eq!(waiting, Ok("waiting"));
        if let Err(panic) = joined {
            std::panic::resume_unwind(panic);
        }
    }

    // vtium/vtium#283: the drop of a head whose GPU driver hangs destroying
    // its context waits for it at most LET_GO_MS, and leaves the thread to
    // end alone
    #[cfg(feature = "gpu")]
    #[test]
    fn dropping_a_painter_whose_driver_hangs_letting_go_does_not_wait_for_it() {
        let gpu = Arc::new(TestGpu::default());
        let mut painter =
            Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
        assert_eq!(set_up(&mut painter), Path::Gpu);
        rounds(&mut painter, 1, 1);
        let (holding, wait_holding) = channel();
        let (let_go, wait_let_go) = channel::<()>();
        let held_by = Arc::clone(&gpu);
        // holds the drop until the test lets go, or until long past the
        // bound, so a drop that waits for the driver ends and fails the test
        let holder = std::thread::spawn(move || {
            let _held = held_by.hold_drop.lock().unwrap();
            holding.send(()).unwrap();
            wait_let_go.recv_timeout(Duration::from_secs(30)).is_ok()
        });
        wait_holding.recv().unwrap();
        drop(painter);
        let _ = let_go.send(());
        assert_eq!(
            wait_for_left(Duration::from_secs(10), &Instant::now),
            0,
            "the thread left ends once the driver lets go"
        );
        assert!(
            holder.join().unwrap(),
            "the drop waited for the driver to let go"
        );
    }

    // ADR-1303 D9: letting go of two heads whose GPU driver hangs destroying
    // their contexts tells both threads to end first and waits for them
    // against one deadline, so the wait in all stays within one LET_GO_MS
    #[cfg(feature = "gpu")]
    #[test]
    fn letting_go_of_two_heads_whose_driver_hangs_waits_one_bound_in_all() {
        let gpu = Arc::new(TestGpu::default());
        let painters: Vec<Painter> = (0..2)
            .map(|_| {
                let source = GpuSource::Test(Arc::clone(&gpu));
                let mut painter = Painter::start("test", Some(source), || {}).unwrap();
                assert_eq!(set_up(&mut painter), Path::Gpu);
                rounds(&mut painter, 1, 1);
                painter
            })
            .collect();
        let (holding, wait_holding) = channel();
        let (let_go_now, wait_let_go) = channel::<()>();
        let held_by = Arc::clone(&gpu);
        // holds both drops for one and a half bounds: two waits one after
        // the other outlast it, and the test fails
        let holder = std::thread::spawn(move || {
            let _held = held_by.hold_drop.lock().unwrap();
            holding.send(()).unwrap();
            wait_let_go
                .recv_timeout(Duration::from_millis(LET_GO_MS * 3 / 2))
                .is_ok()
        });
        wait_holding.recv().unwrap();
        let_go(painters, &Instant::now);
        let _ = let_go_now.send(());
        assert_eq!(
            wait_for_left(Duration::from_secs(10), &Instant::now),
            0,
            "the threads left end once the driver lets go"
        );
        assert!(
            holder.join().unwrap(),
            "the two heads were waited for one after the other"
        );
    }

    // A render thread is joined when its head is dropped: the drop waits
    // for the frame composing.
    #[test]
    fn dropping_the_painter_joins_its_thread() {
        let (started, wait_started) = channel();
        let started = Mutex::new(started);
        let composed = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&composed);
        let hook: DrawHook = Arc::new(move || {
            let _ = started.lock().unwrap().send(());
            std::thread::sleep(Duration::from_millis(200));
            flag.store(true, Ordering::SeqCst);
        });
        let mut painter = Painter::start("test", None, || {}).unwrap();
        assert!(painter.send(Box::new(jobs(1, &hook).remove(0))).is_none());
        wait_started.recv().unwrap();
        drop(painter);
        assert!(composed.load(Ordering::SeqCst), "joined after its frame");
    }

    // The frame figures: a line once due, with the frames and the
    // compose's mean and largest time, and the figures start again.
    #[test]
    fn the_figures_line_comes_once_due_and_starts_again() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut f = Figures {
            due: Some(t0 + Duration::from_secs(1)),
            ..Figures::default()
        };
        f.frame(ms(2), t0);
        f.frame(ms(4), t0);
        assert_eq!(f.line("desk", t0), None, "not due");
        assert_eq!(
            f.line("desk", t0 + Duration::from_secs(1)).as_deref(),
            Some("head desk: 2 frames, compose mean 3000 max 4000 us")
        );
        assert_eq!(f, Figures::default());
    }
}
