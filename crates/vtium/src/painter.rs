//! A head's render thread (ADR-1401 D2): it composes the head's frames,
//! each into its display's shadow buffer, so one head's redraw delays
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
//! Each head's frame times, the compose's and the copy's, are logged at
//! debug level once a second while it draws ([`Figures`]).
//!
//! The thread's stack is [`STACK`]; it ends, and is joined, when its head
//! is dropped, as at a new head file and at the exit. A panic on it ends
//! the process, as one on the loop does (D10): under `panic=abort` at
//! once, and in a test when the loop finds the thread gone.

use std::io;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::term::{Composed, Job};

/// The render thread's stack, in bytes: the vtium tests compose in 64 KiB
/// in a debug build and in 16 KiB optimised.
pub const STACK: usize = 128 * 1024;

/// The frames each of a render thread's two channels holds.
pub const QUEUE: usize = 16;

/// A head's render thread, and the channels to it and back.
pub struct Painter {
    jobs: Option<SyncSender<Job>>,
    done: Option<Receiver<Composed>>,
    thread: Option<JoinHandle<()>>,
}

impl Painter {
    /// Starts the render thread of the head `name`; `wake` wakes the loop
    /// after each frame composed.
    pub fn start(name: &str, wake: impl Fn() + Send + 'static) -> io::Result<Painter> {
        let (jobs, todo) = sync_channel::<Job>(QUEUE);
        let (composed, done) = sync_channel::<Composed>(QUEUE);
        let thread = std::thread::Builder::new()
            .name(format!("render {name}"))
            .stack_size(STACK)
            .spawn(move || {
                for job in todo {
                    if composed.send(job.compose()).is_err() {
                        break;
                    }
                    wake();
                }
            })?;
        Ok(Painter {
            jobs: Some(jobs),
            done: Some(done),
            thread: Some(thread),
        })
    }

    /// Sends `job` to be composed; a full queue gives it back.
    pub fn send(&self, job: Job) -> Option<Job> {
        let jobs = self.jobs.as_ref()?;
        match jobs.try_send(job) {
            Ok(()) => None,
            Err(TrySendError::Full(job) | TrySendError::Disconnected(job)) => Some(job),
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
/// the time each took to compose and to copy into its display's buffer,
/// logged at debug level at most once a second while the head draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Figures {
    frames: u32,
    compose: (Duration, Duration),
    copy: (Duration, Duration),
    /// When the next line is due, once a frame came since the last.
    pub due: Option<Instant>,
}

impl Figures {
    /// A frame presented at `now`, which took `compose` and `copy`.
    pub fn frame(&mut self, compose: Duration, copy: Duration, now: Instant) {
        self.frames = self.frames.saturating_add(1);
        for ((sum, max), took) in [(&mut self.compose, compose), (&mut self.copy, copy)] {
            *sum = sum.saturating_add(took);
            *max = (*max).max(took);
        }
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
            "head {name}: {} frames, compose mean {} max {} us, copy mean {} max {} us",
            f.frames,
            mean(f.compose.0).as_micros(),
            f.compose.1.as_micros(),
            mean(f.copy.0).as_micros(),
            f.copy.1.as_micros(),
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
    use vtium_platform::display::{Canvas, DisplayId, Dpms, Event, Rect, SwapError, Swapped};
    use vtium_render::{Config, DrawState};
    use vtium_term::Terminal;

    use super::*;
    use crate::blink::Blink;
    use crate::head::DrawHook;
    use crate::sync::Sync;
    use crate::term::{Prepared, Term, View};
    use crate::video::Video;

    /// A display of 32 by 16 pixels in memory.
    struct Small(Vec<u8>);

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
        fn canvas(&mut self, _: DisplayId) -> Option<Canvas<'_>> {
            Some(Canvas {
                pixels: &mut self.0,
                width: 32,
                height: 16,
                stride: 128,
                age: 0,
            })
        }
        fn swap(&mut self, _: DisplayId, _: &[Rect]) -> Result<Swapped, SwapError> {
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
        let mut video = Small(vec![0; 32 * 16 * 4]);
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
                    Prepared::Job(job) => job,
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
        let mut painter = Painter::start("test", || {}).unwrap();
        let mut all = jobs(u32::try_from(QUEUE).unwrap() + 2, &hook).into_iter();
        assert!(painter.send(all.next().unwrap()).is_none());
        wait_started.recv().unwrap();
        for _ in 0..QUEUE {
            assert!(painter.send(all.next().unwrap()).is_none(), "queued");
        }
        let back = painter.send(all.next().unwrap());
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
        let painter = Painter::start("test", || {}).unwrap();
        assert!(painter.send(jobs(1, &hook).remove(0)).is_none());
        wait_started.recv().unwrap();
        drop(painter);
        assert!(composed.load(Ordering::SeqCst), "joined after its frame");
    }

    // The frame figures: a line once due, with the frames, the compose's
    // and the copy's mean and largest time, and the figures start again.
    #[test]
    fn the_figures_line_comes_once_due_and_starts_again() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut f = Figures {
            due: Some(t0 + Duration::from_secs(1)),
            ..Figures::default()
        };
        f.frame(ms(2), ms(1), t0);
        f.frame(ms(4), ms(3), t0);
        assert_eq!(f.line("desk", t0), None, "not due");
        assert_eq!(
            f.line("desk", t0 + Duration::from_secs(1)).as_deref(),
            Some("head desk: 2 frames, compose mean 3000 max 4000 us, copy mean 2000 max 3000 us")
        );
        assert_eq!(f, Figures::default());
    }
}
