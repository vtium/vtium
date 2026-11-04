//! A head's render thread (ADR-1401 D2): it composes the head's frames,
//! each straight into the buffer its display lent in a token, so one head's
//! redraw delays
//! neither another head's frames nor the loop, which keeps the input, the
//! ptys, the devices and their flips. A frame depends only on its head's
//! screen and drawing, so the frames are what one thread would draw.
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
//! A display that leaves the head is sent on the same channel as the
//! frames, after the frames sent before it, and the GPU path frees what it
//! kept for the display. A full queue takes it with the next frame sent.
//!
//! A head whose `hwaccel` is on, in a build with `gpu`, draws on the GPU
//! path (ADR-1303 D9): the thread makes the head's GL context before it
//! takes a frame, from the [`GpuSource`] the loop gives it, and tells the
//! loop which [`Path`] the head draws on, or why the GPU path could not
//! start, before [`Painter::start`] returns. A head whose context cannot
//! be made draws on the CPU path.
//!
//! The thread's stack is [`GPU_STACK`] when the loop gives it a
//! [`GpuSource`], whether or not the GPU path then starts, and [`STACK`]
//! otherwise. The thread ends, and is joined, when its head is dropped,
//! as at a new head file and at the exit. A panic on it ends the process,
//! as one on the loop does (D10): under `panic=abort` at once, and in a
//! test when the loop finds the thread gone.

use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel};
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
    /// The display whose frames alone raise the queued faults; every
    /// display's when `None`.
    pub faulty_display: std::sync::Mutex<Option<vtium_platform::display::DisplayId>>,
    /// The displays whose set-up the GPU path refuses.
    pub refuse_set_up:
        std::sync::Mutex<std::collections::BTreeSet<vtium_platform::display::DisplayId>>,
    /// Each set-up of a display the GPU path tried, in order.
    pub set_ups: std::sync::Mutex<Vec<vtium_platform::display::DisplayId>>,
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
}

/// A head's render thread, and the channels to it and back.
pub struct Painter {
    jobs: Option<SyncSender<Message>>,
    /// The displays that left the head and that a full queue has not taken
    /// yet, the oldest first.
    gone: VecDeque<(PathBuf, DisplayId)>,
    done: Option<Receiver<Composed>>,
    thread: Option<JoinHandle<()>>,
    path: Path,
    /// Why the GPU path asked for could not start.
    refused: Option<String>,
}

impl Painter {
    /// Starts the render thread of the head `name`; `wake` wakes the loop
    /// after each frame composed. With `gpu`, the thread first makes the
    /// head's GL context there, and the call returns once it has, or has
    /// failed to, so the painter knows its [`Path`].
    pub fn start(
        name: &str,
        gpu: Option<GpuSource>,
        wake: impl Fn() + Send + 'static,
    ) -> io::Result<Painter> {
        let (jobs, todo) = sync_channel::<Message>(QUEUE);
        let (composed, done) = sync_channel::<Composed>(QUEUE);
        let (ready, set_up) = sync_channel::<Ready>(1);
        let thread = std::thread::Builder::new()
            .name(format!("render {name}"))
            .stack_size(if gpu.is_some() { GPU_STACK } else { STACK })
            .spawn(move || draw_frames(gpu, &ready, &todo, &composed, &wake))?;
        // the thread sends its set-up before it takes a frame, and a thread
        // that ended without sending it draws nothing
        let (path, refused) = set_up.recv().unwrap_or_default();
        Ok(Painter {
            jobs: Some(jobs),
            gone: VecDeque::new(),
            done: Some(done),
            thread: Some(thread),
            path,
            refused,
        })
    }

    /// The path the head's frames are drawn on.
    pub fn path(&self) -> Path {
        self.path
    }

    /// Records that the head's GPU path ended during the run: it draws on
    /// the CPU path from now on.
    pub fn left_gpu(&mut self) {
        self.path = Path::Cpu;
        self.gone.clear();
    }

    /// The display `key` left the head. On the GPU path the thread frees
    /// what it kept for the display once it has composed the frames sent
    /// before; a full queue takes the message with the next frame sent.
    pub fn forget(&mut self, key: (PathBuf, DisplayId)) {
        if self.path == Path::Gpu {
            self.gone.push_back(key);
            self.send_gone();
        }
    }

    /// Sends the displays that left, the oldest first, while the queue
    /// takes them. Gives whether none is left to send.
    fn send_gone(&mut self) -> bool {
        let Some(jobs) = &self.jobs else {
            return true;
        };
        while let Some(key) = self.gone.pop_front() {
            match jobs.try_send(Message::Forget(key)) {
                Ok(()) => {}
                Err(TrySendError::Full(message)) => {
                    if let Message::Forget(key) = message {
                        self.gone.push_front(key);
                    }
                    return false;
                }
                // a thread that ended keeps nothing to free
                Err(TrySendError::Disconnected(_)) => {
                    self.gone.clear();
                    return true;
                }
            }
        }
        true
    }

    /// Why the GPU path asked for could not start, once: the loop logs it.
    pub fn take_refusal(&mut self) -> Option<String> {
        self.refused.take()
    }

    /// Sends `job` to be composed, after the displays that left; a full
    /// queue gives it back.
    pub fn send(&mut self, job: Box<Job>) -> Option<Box<Job>> {
        if !self.send_gone() {
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

    /// The frames composed since the last call, in the order composed.
    /// A thread that ended by a panic passes the panic on.
    pub fn take(&mut self) -> Vec<Composed> {
        let mut frames = Vec::new();
        let Some(done) = &self.done else {
            return frames;
        };
        loop {
            match done.try_recv() {
                Ok(frame) => frames.push(frame),
                Err(TryRecvError::Empty) => return frames,
                Err(TryRecvError::Disconnected) => {
                    if let Some(Err(panic)) = self.thread.take().map(JoinHandle::join) {
                        std::panic::resume_unwind(panic);
                    }
                    return frames;
                }
            }
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
    for message in todo {
        let job = match message {
            Message::Frame(job) => job,
            Message::Forget(key) => {
                if let Some(h) = &mut head {
                    h.forget(&key);
                }
                continue;
            }
        };
        let mut frame = job.compose(head.as_mut());
        // ADR-1303 D9: the head stays on the CPU path until vtium restarts,
        // and the loop says so once. The frame goes back before the GL
        // context is dropped, so a driver that hangs destroying it holds
        // no frame.
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

/// The render thread in a build without `gpu`: every head on the CPU path.
#[cfg(not(feature = "gpu"))]
fn draw_frames(
    _: Option<GpuSource>,
    ready: &SyncSender<Ready>,
    todo: &Receiver<Message>,
    composed: &SyncSender<Composed>,
    wake: &impl Fn(),
) {
    let _ = ready.send((Path::Cpu, None));
    for message in todo {
        let Message::Frame(job) = message else {
            continue;
        };
        if composed.send(job.compose(None)).is_err() {
            break;
        }
        wake();
    }
}

impl Drop for Painter {
    /// Ends the thread once its frame composing is done, and joins it.
    fn drop(&mut self) {
        self.jobs = None;
        self.done = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
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

    /// A display of 32 by 16 pixels in memory.
    struct Small(Arc<Mutex<Vec<u8>>>);

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
            Some((32, 16))
        }
        fn refresh_mhz(&self, _: DisplayId) -> Option<u32> {
            None
        }
        fn lend(&mut self, id: DisplayId) -> Option<Token> {
            let memory: Arc<Memory> = self.0.clone();
            Some(Token::new(id, (32, 16), 128, 0, memory))
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

    /// `n` jobs, a display each, every one running `hook` before it
    /// composes.
    fn jobs(n: u32, hook: &DrawHook) -> Vec<Job> {
        let mut term = Term::new(
            Terminal::new(4, 1, 0),
            Sync::new(Duration::ZERO, 0),
            Blink::new(false),
            Rc::new(Instant::now),
        );
        let mut video = Small(Arc::new(Mutex::new(vec![0; 32 * 16 * 4])));
        let mut snapshot = None;
        (0..n)
            .map(|i| {
                let face: crate::font::DisplayFace = Box::new(Unifont::new(16));
                let mut view = View::new(
                    DisplayId(i),
                    Font::new(face),
                    (32, 16),
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

    // A display that leaves while the queue is full is sent with the next
    // frame, before it: the GPU path frees what it kept for the display,
    // counted in the displays it keeps.
    #[cfg(feature = "gpu")]
    #[test]
    fn a_display_that_leaves_while_the_queue_is_full_goes_with_the_next_frame() {
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
        assert_eq!(painter.path(), Path::Gpu, "the head on the GPU path");
        let mut all = jobs(u32::try_from(QUEUE).unwrap() + 2, &hook).into_iter();
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
        let kept = |painter: &mut Painter, frames: usize| {
            let end = Instant::now() + Duration::from_secs(10);
            let mut done = 0;
            while done < frames && Instant::now() < end {
                done += painter.take().len();
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(done, frames, "the frames composed");
            gpu.kept.load(Ordering::SeqCst)
        };
        // the forget waits while nothing is sent
        assert_eq!(kept(&mut painter, QUEUE + 1), QUEUE + 1);
        assert!(painter.send(Box::new(all.next().unwrap())).is_none());
        assert_eq!(kept(&mut painter, 1), QUEUE + 1, "display 1 freed");
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
        let end = Instant::now() + Duration::from_secs(10);
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

    // ADR-1303 D9: a display the GPU path cannot set up is drawn by the CPU
    // path until its mode changes, with no try at each frame, and says so
    // once; the head's other display stays on the GPU path
    #[cfg(feature = "gpu")]
    #[test]
    fn a_display_the_gpu_cannot_set_up_is_drawn_on_the_cpu_path_and_tried_once() {
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
                .all(|c| c.left_gpu.is_none() && c.failed_gpu.is_none())
        );
        let tries = gpu.set_ups.lock().unwrap().clone();
        assert_eq!(tries, [DisplayId(0), DisplayId(1)], "each set up once");
        assert_eq!(
            gpu.kept.load(Ordering::SeqCst),
            1,
            "display 0 on the GPU path"
        );
    }

    // ADR-1303 D9: a GL error in two of one display's frames running ends
    // the GPU path, though another display's frame drew between them
    #[cfg(feature = "gpu")]
    #[test]
    fn a_gl_error_in_two_of_a_displays_frames_running_ends_the_gpu_path() {
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
