use std::any::Any;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use vtium_font::{Font, Unifont};
use vtium_platform::display::{DisplayId, Dpms, Event, Memory, Rect, SwapError, Swapped, Token};
use vtium_render::{Config, DrawState};
use vtium_term::Terminal;

use super::*;
use crate::blink::Blink;
use crate::head::DrawHook;
use crate::sync::Sync;
use crate::term::{Prepared, Term, View};
use crate::video::Video;

/// A display in memory, of the size it holds, which lends its one
/// buffer as that many frames old.
struct Small(Arc<Mutex<Vec<u8>>>, (u32, u32), u32);

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
            self.2,
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
    let mut video = Small(Arc::new(Mutex::new(vec![0; bytes])), size, 0);
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
                crate::pointer_file::Store::default(),
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

/// One display's terminal, view and buffer, kept from frame to frame,
/// so each frame is drawn by the renderer the frame before gave back,
/// as the loop draws a display.
#[cfg(feature = "gpu")]
struct Shown {
    term: Term,
    view: View,
    video: Small,
    key: (PathBuf, DisplayId),
}

#[cfg(feature = "gpu")]
impl Shown {
    /// Display `id` of `size`, showing a terminal of `grid` columns and
    /// rows.
    fn new(id: DisplayId, size: (u32, u32), grid: (usize, usize)) -> Shown {
        let bytes = usize::try_from(size.0.saturating_mul(size.1).saturating_mul(4)).unwrap();
        let face: crate::font::DisplayFace = Box::new(Unifont::new(16));
        Shown {
            term: Term::new(
                Terminal::new(grid.0, grid.1, 0),
                Sync::new(Duration::ZERO, 0),
                Blink::new(false),
                Rc::new(Instant::now),
            ),
            view: View::new(
                id,
                Font::new(face),
                size,
                None,
                Config::default(),
                false,
                crate::pointer_file::Store::default(),
            ),
            video: Small(Arc::new(Mutex::new(vec![0; bytes])), size, 0),
            key: (PathBuf::from("/dev/test"), id),
        }
    }

    /// The terminal takes `output`, and the job draws the screen into
    /// the display's buffer, lent as `age` frames old.
    fn job(&mut self, output: &[u8], age: u32) -> Job {
        assert_eq!(self.term.parse(output, &mut |_| {}), output.len());
        self.video.2 = age;
        let hook: DrawHook = Arc::new(|| {});
        match self.view.prepare(
            &self.key,
            &mut self.term,
            &mut None,
            &mut self.video,
            &DrawState::default(),
            Some(&hook),
            &Instant::now,
        ) {
            Prepared::Job(job) => *job,
            Prepared::Later | Prepared::Nothing => panic!("a job"),
        }
    }

    /// The view takes back the drawing of the frame `composed`.
    fn back(&mut self, composed: Composed) {
        self.view.present(
            composed,
            &mut self.term,
            &mut self.video,
            true,
            &Instant::now,
        );
    }

    /// The display's buffer.
    fn pixels(&self) -> Vec<u8> {
        self.video.0.lock().unwrap().clone()
    }
}

/// What display 0 of `size` and `grid` shows on a head on the CPU path
/// after a frame of each of `frames`: the output the terminal takes,
/// and the age its buffer is lent at.
#[cfg(feature = "gpu")]
fn on_the_cpu_path(size: (u32, u32), grid: (usize, usize), frames: &[(&[u8], u32)]) -> Vec<u8> {
    let mut painter = Painter::start("test", None, || {}).unwrap();
    assert_eq!(set_up(&mut painter), Path::Cpu);
    let mut shown = Shown::new(DisplayId(0), size, grid);
    for &(output, age) in frames {
        let c = one(&mut painter, shown.job(output, age));
        shown.back(c);
    }
    shown.pixels()
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
    // The bound only turns a hang into a failure (AGENTS.md, testing rules).
    // It is generous because a cold shader cache under qemu-user can
    // take many seconds to compose the queued frames.
    let end = Instant::now()
        .checked_add(Duration::from_secs(120))
        .unwrap();
    let mut done = 0;
    while (done < QUEUE + 1 || gpu.kept.load(Ordering::SeqCst) > QUEUE) && Instant::now() < end {
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
    one(painter, jobs_at(1, size, &hook).remove(0))
}

/// Sends `job`, and gives it composed.
#[cfg(feature = "gpu")]
fn one(painter: &mut Painter, job: Job) -> Composed {
    assert!(painter.send(Box::new(job)).is_none());
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

// ADR-1303 D9: a head held on the CPU path gives back every texture and
// frame its displays hold, as at a VT release, and keeps its programs,
// so it holds no GPU memory but its programs while the CPU path draws.
// Once the display that held it leaves, the next frame makes them again
// with no display set up anew, and draws on the GPU path: display 0's
// buffer then holds byte for byte what the CPU path draws there over
// the same frames
#[cfg(feature = "gpu")]
#[test]
fn a_head_held_on_the_cpu_path_holds_no_textures() {
    let (size, grid) = ((96, 16), (12, 1));
    let frames: [(&[u8], u32); 3] = [
        (b"\x1b[31mthe se\x1b[42mams", 0),
        (b"\x1b[0m held", 1),
        (b"\x1b[7m back", 1),
    ];
    let gpu = Arc::new(TestGpu::default());
    let mut painter =
        Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
    assert_eq!(set_up(&mut painter), Path::Gpu);
    let mut zero = Shown::new(DisplayId(0), size, grid);
    let first = one(&mut painter, zero.job(frames[0].0, frames[0].1));
    assert!(first.failed_gpu.is_none() && first.refused_gpu.is_none());
    zero.back(first);
    assert_eq!(gpu.drawn.load(Ordering::SeqCst), 1, "drawn on the GPU path");
    let (textures, targets, programs) = *gpu.live.lock().unwrap();
    assert!(
        textures > 0 && targets > 0 && programs > 0,
        "drawn on the GPU"
    );
    gpu.refuse_set_up.lock().unwrap().insert(DisplayId(1));
    let hook: DrawHook = Arc::new(|| {});
    let refused = one(&mut painter, jobs_at(2, (32, 16), &hook).remove(1));
    assert!(refused.refused_gpu.is_some(), "held by display 1");
    let held = one(&mut painter, zero.job(frames[1].0, frames[1].1));
    assert!(held.refused_gpu.is_none() && held.failed_gpu.is_none());
    zero.back(held);
    assert_eq!(gpu.drawn.load(Ordering::SeqCst), 1, "held on the CPU path");
    assert_eq!(
        *gpu.live.lock().unwrap(),
        (0, 0, programs),
        "only the programs held"
    );
    assert_eq!(
        gpu.kept.load(Ordering::SeqCst),
        1,
        "display 0's pipeline kept"
    );
    painter.forget((PathBuf::from("/dev/test"), DisplayId(1)));
    let back = one(&mut painter, zero.job(frames[2].0, frames[2].1));
    assert!(
        back.back_on_gpu && back.failed_gpu.is_none() && back.refused_gpu.is_none(),
        "back on the GPU path: {:?} {:?}",
        back.failed_gpu,
        back.refused_gpu
    );
    zero.back(back);
    assert_eq!(gpu.drawn.load(Ordering::SeqCst), 2, "drawn on the GPU path");
    assert!(
        zero.pixels() == on_the_cpu_path(size, grid, &frames),
        "the first frame back is the CPU path's"
    );
    assert_eq!(
        *gpu.live.lock().unwrap(),
        (textures, targets, programs),
        "made again as the first frame made them"
    );
    let tries: Vec<DisplayId> = gpu.set_ups.lock().unwrap().iter().map(|k| k.1).collect();
    assert_eq!(tries, [DisplayId(0), DisplayId(1)], "each set up once");
}

// ADR-1303 D3, D9: a display wider than the GPU's largest texture, and
// within its largest renderbuffer, draws on the GPU path, its upright
// frame in tiles and its frame in a buffer in GL's memory, with no
// refusal; what it draws into the display's buffer is byte for byte
// what the CPU path draws there, in a whole frame and in a frame whose
// buffer is one frame old. Text crosses the seam at 64 pixels
#[cfg(feature = "gpu")]
#[test]
fn a_display_past_the_gpus_largest_texture_draws_on_the_gpu_path_as_the_cpu_path_does() {
    let (size, grid) = ((96, 16), (12, 1));
    let gpu = Arc::new(TestGpu::default());
    *gpu.largest_texture.lock().unwrap() = Some(64);
    let mut heads = [
        (None, Path::Cpu),
        (Some(GpuSource::Test(Arc::clone(&gpu))), Path::Gpu),
    ]
    .map(|(source, path)| {
        let mut painter = Painter::start("test", source, || {}).unwrap();
        assert_eq!(set_up(&mut painter), path);
        (painter, Shown::new(DisplayId(0), size, grid), path)
    });
    let frames: [(&[u8], u32); 2] = [
        (b"\x1b[31mthe se\x1b[42mams\x1b[7m ok", 0),
        (b"\r\x1b[27;34mand ag\x1b[43main", 1),
    ];
    for (n, &(output, age)) in frames.iter().enumerate() {
        for (painter, shown, path) in &mut heads {
            let c = one(painter, shown.job(output, age));
            assert!(
                c.refused_gpu.is_none() && c.failed_gpu.is_none() && c.left_gpu.is_none(),
                "{path:?}: {:?} {:?} {:?}",
                c.refused_gpu,
                c.failed_gpu,
                c.left_gpu
            );
            shown.back(c);
        }
        assert_eq!(
            gpu.drawn.load(Ordering::SeqCst),
            n + 1,
            "drawn on the GPU path"
        );
        let [(_, on_cpu, _), (_, on_gpu, _)] = &heads;
        let (on_cpu, on_gpu) = (on_cpu.pixels(), on_gpu.pixels());
        assert!(
            on_cpu.chunks(4).any(|p| on_cpu.get(..4) != Some(p)),
            "the frame shows more than one color"
        );
        assert!(
            on_cpu == on_gpu,
            "frame {n}, at age {age}: the GPU path's frame is the CPU path's"
        );
    }
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
    let _alone = alone();
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
    let _alone = alone();
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

/// Held by each test that leaves render threads, so no other test's
/// wait for the threads left joins this test's threads before it looks.
#[cfg(feature = "gpu")]
fn alone() -> std::sync::MutexGuard<'static, ()> {
    static LEFT_TESTS: Mutex<()> = Mutex::new(());
    LEFT_TESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// A thread left to end alone that has ended since is joined at the next
// let-go of a head, as at a reload of vtium.conf, so its stack is freed
// then and not only at the exit
#[cfg(feature = "gpu")]
#[test]
fn a_left_thread_that_ended_is_joined_at_the_next_let_go() {
    let _alone = alone();
    let gpu = Arc::new(TestGpu::default());
    let mut painter =
        Painter::start("test", Some(GpuSource::Test(Arc::clone(&gpu))), || {}).unwrap();
    assert_eq!(set_up(&mut painter), Path::Gpu);
    let id = painter.thread.as_ref().unwrap().thread().id();
    let gone = Arc::clone(&painter.gone);
    let listed = || left().iter().any(|l| l.thread.thread().id() == id);
    let held = gpu.hold_drop.lock().unwrap();
    drop(painter);
    assert!(listed(), "left while the driver holds the drop");
    // a wait whose clock does not move ends at its bound all the same
    let (waited, wait_done) = channel();
    let frozen = Instant::now();
    std::thread::spawn(move || {
        let _ = waited.send(wait_for_left(Duration::from_millis(50), &move || frozen));
    });
    assert!(
        wait_done
            .recv_timeout(Duration::from_secs(10))
            .is_ok_and(|running| running >= 1),
        "the wait on a clock that does not move ended, the thread still running"
    );
    drop(held);
    let (threads, _) = ENDED
        .wait_timeout_while(left(), Duration::from_secs(10), |_| {
            !gone.load(Ordering::SeqCst)
        })
        .unwrap();
    drop(threads);
    assert!(gone.load(Ordering::SeqCst), "the thread ended");
    let_go(Vec::new(), &Instant::now);
    assert!(!listed(), "joined at the let-go");
}

// A left thread that ended without setting its flag, as one that
// panicked does, counts as ended and is joined
#[cfg(feature = "gpu")]
#[test]
fn a_left_thread_that_panicked_counts_as_ended() {
    let _alone = alone();
    let thread = std::thread::spawn(|| panic!("a left thread that panics"));
    let end = Instant::now() + Duration::from_secs(10);
    while !thread.is_finished() && Instant::now() < end {
        std::thread::yield_now();
    }
    assert!(thread.is_finished(), "the thread panicked");
    let id = thread.thread().id();
    left().push(Left {
        thread,
        gone: Arc::new(AtomicBool::new(false)),
    });
    wait_for_left(Duration::ZERO, &Instant::now);
    assert!(
        !left().iter().any(|l| l.thread.thread().id() == id),
        "joined"
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
    let mut none = EchoFigures::default();
    assert_eq!(f.line("desk", &mut none, t0), None, "not due");
    assert_eq!(
        f.line("desk", &mut none, t0 + Duration::from_secs(1))
            .as_deref(),
        Some("head desk: 2 frames, compose mean 3000 max 4000 us")
    );
    assert_eq!(f, Figures::default());
}

// The frame figures' line gives the key-to-present times of the keys
// whose echo the head's displays presented since the last line, and
// takes them, so the next line starts again.
#[test]
fn the_figures_line_gives_the_key_to_present_times() {
    let t0 = Instant::now();
    let us = Duration::from_micros;
    let mut f = Figures {
        due: Some(t0),
        ..Figures::default()
    };
    f.frame(us(500), t0);
    let mut echoes = EchoFigures {
        count: 2,
        sum: us(3000),
        max: us(2000),
    };
    assert_eq!(
        f.line("desk", &mut echoes, t0).as_deref(),
        Some(
            "head desk: 1 frames, compose mean 500 max 500 us, \
             2 key echoes, key to present mean 1500 max 2000 us"
        )
    );
    assert_eq!(echoes, EchoFigures::default(), "the times start again");
    let mut f = Figures {
        due: Some(t0 + Duration::from_secs(1)),
        ..Figures::default()
    };
    let mut echoes = EchoFigures {
        count: 1,
        sum: us(700),
        max: us(700),
    };
    assert_eq!(f.line("desk", &mut echoes, t0), None, "not due");
    assert_eq!(echoes.count, 1, "kept for the line that is due");
}
