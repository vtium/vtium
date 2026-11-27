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

use crate::term::{Composed, EchoFigures, Job};

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
    /// The GPU's largest texture, which a test sets below the context's:
    /// the head's context then refuses a larger texture, as a GPU of that
    /// limit would, and the upright frame is held in tiles (ADR-1303 D3).
    /// The context's when `None`.
    pub largest_texture: std::sync::Mutex<Option<u32>>,
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

/// A note for the log from the GPU path about one of the head's displays,
/// such as glyph textures at their ceiling.
pub struct Note {
    /// The display the note is about, by its device and its id there.
    pub display: (PathBuf, DisplayId),
    pub text: String,
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
    /// The GPU path's notes for the log, which the loop takes with
    /// [`Painter::take_notes`] and logs under each display's connector.
    notes: Option<Receiver<Note>>,
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
        let (note, notes) = sync_channel::<Note>(QUEUE);
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
    /// path, held on the CPU path for now or not, or its set-up is not
    /// known yet.
    pub fn may_hold_gpu(&self) -> bool {
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
                "head {}: waiting for the GPU driver to get ready; the head shows nothing until then",
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
    /// The log gets why the GPU path could not start, once. A thread that
    /// ended by a panic passes the panic on.
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
        frames
    }

    /// The GPU path's notes for the log since the last call.
    pub fn take_notes(&mut self) -> Vec<Note> {
        self.notes
            .as_ref()
            .map(|notes| notes.try_iter().collect())
            .unwrap_or_default()
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
            log::warn!("head {}: {why}, so it draws without the GPU", self.name);
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
    notes: &SyncSender<Note>,
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
fn send_notes(head: &mut crate::gpu::head::HeadGpu, notes: &SyncSender<Note>) {
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
    _: &SyncSender<Note>,
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
    /// thread is left to end alone, which the log says. Each thread left
    /// before that has ended since is joined too, so its stack is freed at
    /// the next drop of a head, as at a reload of vtium.conf, and not only
    /// at the exit.
    fn wait_to_end(&mut self, within: Duration) {
        let Some(thread) = self.thread.take() else {
            join_ended();
            return;
        };
        let let_go = self.ended.as_ref().map(|e| e.recv_timeout(within));
        match let_go {
            Some(Err(RecvTimeoutError::Timeout)) => {
                log::warn!(
                    "head {}: the GPU driver did not release the head within {LET_GO_MS} ms; vtium goes on without waiting",
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
        join_ended();
    }
}

/// A render thread vtium went on without, as [`Painter::wait_to_end`]
/// left it, and the flag the thread sets as it ends. The thread owns its
/// head's GL context; nothing here holds a GL object.
struct Left {
    thread: JoinHandle<()>,
    gone: Arc<AtomicBool>,
}

impl Left {
    /// Whether the thread has ended: it set its flag, or it ended without
    /// setting it, as a thread that panicked does.
    fn ended(&self) -> bool {
        self.gone.load(Ordering::SeqCst) || self.thread.is_finished()
    }
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
        let running = threads.iter().any(|l| !l.ended());
        let rest = deadline.map_or(within, |d| d.saturating_duration_since(clock()));
        if !running || rest.is_zero() {
            break;
        }
        // a wait that times out ends the loop, so a clock that does not
        // move cannot keep it going
        let (guard, result) = ENDED
            .wait_timeout(threads, rest)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        threads = guard;
        if result.timed_out() {
            break;
        }
    }
    join_ended_in(threads)
}

/// Joins each render thread vtium went on without that has ended, and
/// gives how many still run, without waiting.
fn join_ended() -> usize {
    join_ended_in(left())
}

/// Joins each thread of the locked list `threads` that has ended, with the
/// lock released, and gives how many still run.
fn join_ended_in(mut threads: std::sync::MutexGuard<'static, Vec<Left>>) -> usize {
    let (ended, running): (Vec<Left>, Vec<Left>) = std::mem::take(&mut *threads)
        .into_iter()
        .partition(Left::ended);
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
    join_ended();
}

/// A head's frame times since its last line: the frames presented, and
/// the time each took to compose into its display's buffer, logged at
/// debug level at most once a second while the head draws. The line also
/// gives the key-to-present times its displays measured since the last
/// line, when a key's echo was presented: the time from a key vtium sent
/// to the program until a display presented the frame that carries the
/// program's answer.
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

    /// The line for the head `name` once it is due at `now`, with the
    /// key-to-present times in `echoes`, the figures and the times
    /// starting again.
    pub fn line(&mut self, name: &str, echoes: &mut EchoFigures, now: Instant) -> Option<String> {
        self.due.filter(|&d| d <= now)?;
        let f = std::mem::take(self);
        let e = std::mem::take(echoes);
        let mean = |sum: Duration, n: u32| sum.checked_div(n.max(1)).unwrap_or_default();
        let mut line = format!(
            "head {name}: {} frames, compose mean {} max {} us",
            f.frames,
            mean(f.compose.0, f.frames).as_micros(),
            f.compose.1.as_micros(),
        );
        if e.count > 0 {
            line.push_str(&format!(
                ", {} key echoes, key to present mean {} max {} us",
                e.count,
                mean(e.sum, e.count).as_micros(),
                e.max.as_micros(),
            ));
        }
        Some(line)
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
mod tests;
