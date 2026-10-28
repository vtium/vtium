//! The glyph helper's client (ADR-1201 D1, ADR-1301 D2): the console
//! starts `vtium-vitrify serve` on its first need for a frit's glyphs,
//! asks it for one cell's glyphs at a time, the screen's codepoints first,
//! and takes each answer as it arrives without ever waiting on it.
//!
//! The helper runs with an empty environment in `/`, in its own process
//! group, holding only its pipes, as `nobody` when the console is root
//! (ADR-1400 D2's last account before login), and sandboxes itself. A
//! thread writes the requests and another reads the answers, so neither
//! a slow nor a hung helper stalls the loop; the reader wakes the loop
//! after each answer. Each frame is checked before it is trusted: its
//! length against [`FRAME_BYTES_MAX`] before anything is allocated, a set
//! through vtium-font's reader, which bounds every field, and every strike
//! at the cell asked for, regular in style.
//!
//! A helper that crashes, panics, fails a request, answers what is not a
//! frame, or gives no answer for [`ANSWER_MS`] is ended, and the console
//! logs one line and asks it nothing more: the built-in font goes on
//! drawing what the helper has not sent. A helper that exits 0 between
//! requests left at its idle interval, and the next request starts
//! another; one that left as a request went out is started again for it,
//! once.
//!
//! The wire format is `vtium-vitrify`'s serve mode's (`serve.rs`), whose
//! constants this module repeats.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Child, Command as Process, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use vtium_font::Cell;
use vtium_font::set::Set;
use vtium_platform::vt::font::ConsoleFont;

/// How long the helper may take for each answer, a frame or the request's
/// end, before it is ended: a CJK frit's first batch on a Cortex-A53 takes
/// about a second.
pub const ANSWER_MS: u64 = 10_000;
/// The most bytes one frame's payload holds, the helper's
/// `FRAME_BYTES_MAX`.
pub const FRAME_BYTES_MAX: u32 = 16 << 20;
/// The most bytes of a failure's reason kept.
pub const REASON_BYTES_MAX: u32 = 4096;
/// The most codepoints a request names first, the helper's `FIRST_MAX`.
pub const FIRST_MAX: usize = 32_000;

const REQUEST_MAGIC: &[u8; 8] = b"VTVSREQ1";
const FRAME_GLYPHS: u32 = 1;
const FRAME_DONE: u32 = 2;
const FRAME_FAILED: u32 = 3;

/// The helper's program, set at build time with `VTIUM_VITRIFY`.
pub fn default_program() -> PathBuf {
    PathBuf::from(option_env!("VTIUM_VITRIFY").unwrap_or("/usr/bin/vtium-vitrify"))
}

/// The account the helper runs as when the console is root: `nobody`,
/// its uid and gid read from `passwd`.
pub fn nobody(passwd: &str) -> Option<(u32, u32)> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next() == Some("nobody")).then_some(())?;
        let _password = fields.next()?;
        let uid = fields.next()?.parse().ok()?;
        let gid = fields.next()?.parse().ok()?;
        (uid != 0 && gid != 0).then_some((uid, gid))
    })
}

/// How to start the helper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// The uid and gid to run as; `None` keeps the console's.
    pub account: Option<(u32, u32)>,
}

impl Command {
    /// `vtium-vitrify serve`, as `nobody` when the console is root; `None`
    /// as root with no `nobody` in `/etc/passwd`, since the helper never
    /// runs as root.
    pub fn system() -> Result<Command, String> {
        let account = if rustix::process::geteuid().is_root() {
            let passwd = std::fs::read_to_string("/etc/passwd")
                .map_err(|e| format!("cannot read /etc/passwd for the helper's account: {e}"))?;
            Some(nobody(&passwd).ok_or("/etc/passwd holds no nobody to run the helper as")?)
        } else {
            None
        };
        Ok(Command {
            program: default_program(),
            args: vec!["serve".into()],
            account,
        })
    }
}

/// One cell's glyphs, asked of the helper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The frit, by its absolute path.
    pub frit: PathBuf,
    pub cell: Cell,
    /// The codepoints the helper draws first.
    pub first: Vec<char>,
    /// The kernel's font, read from the VT at start (ADR-1201 D7).
    pub console: Option<ConsoleFont>,
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl Request {
    /// The request's bytes, its first codepoints cut at [`FIRST_MAX`].
    pub fn encode(&self) -> Vec<u8> {
        let mut out = REQUEST_MAGIC.to_vec();
        let path = self.frit.to_string_lossy();
        put32(&mut out, len32(path.len()));
        out.extend_from_slice(path.as_bytes());
        put32(&mut out, self.cell.width);
        put32(&mut out, self.cell.height);
        let first = self.first.get(..FIRST_MAX).unwrap_or(&self.first);
        put32(&mut out, len32(first.len()));
        for &c in first {
            put32(&mut out, u32::from(c));
        }
        match &self.console {
            None => put32(&mut out, 0),
            Some(f) => {
                put32(&mut out, 1);
                for v in [f.width, f.height, f.charcount, len32(f.data.len())] {
                    put32(&mut out, v);
                }
                out.extend_from_slice(&f.data);
                put32(&mut out, len32(f.unimap.len()));
                for p in &f.unimap {
                    out.extend_from_slice(&p.unicode.to_le_bytes());
                    out.extend_from_slice(&p.fontpos.to_le_bytes());
                }
            }
        }
        out
    }
}

/// What the reader thread saw.
#[derive(Debug)]
enum Event {
    Glyphs(Arc<Set>),
    Done,
    Failed(String),
    /// Bytes that are not a frame, or a set its reader refused.
    Malformed(String),
    /// The helper closed its end.
    Closed,
}

/// Reads one frame's head: its kind and length, or `None` at the end of
/// the stream between frames.
fn head(r: &mut impl Read) -> Result<Option<(u32, u32)>, String> {
    let mut b = [0u8; 8];
    let mut got = 0;
    while got < b.len() {
        let rest = b.get_mut(got..).unwrap_or_default();
        match r.read(rest) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err("a frame cut short".to_owned()),
            Ok(n) => got = got.saturating_add(n),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(format!("cannot read the helper's answer: {e}")),
        }
    }
    let [k0, k1, k2, k3, n0, n1, n2, n3] = b;
    Ok(Some((
        u32::from_le_bytes([k0, k1, k2, k3]),
        u32::from_le_bytes([n0, n1, n2, n3]),
    )))
}

/// Reads one frame into an event: its length checked against its kind's
/// bound before its payload is allocated.
fn frame(r: &mut impl Read) -> Event {
    let (kind, len) = match head(r) {
        Ok(Some(h)) => h,
        Ok(None) => return Event::Closed,
        Err(why) => return Event::Malformed(why),
    };
    let max = match kind {
        FRAME_GLYPHS => FRAME_BYTES_MAX,
        FRAME_DONE => 0,
        FRAME_FAILED => REASON_BYTES_MAX,
        other => return Event::Malformed(format!("a frame of unknown kind {other}")),
    };
    if len > max {
        return Event::Malformed(format!("a frame of {len} bytes, past its kind's {max}"));
    }
    let mut payload = vec![0u8; usize::try_from(len).unwrap_or(0)];
    if r.read_exact(&mut payload).is_err() {
        return Event::Malformed("a frame cut short".to_owned());
    }
    match kind {
        FRAME_GLYPHS => match Set::from_bytes(payload) {
            Ok(set) => Event::Glyphs(Arc::new(set)),
            Err(e) => Event::Malformed(format!("glyphs vtium cannot read: {e}")),
        },
        FRAME_DONE => Event::Done,
        _ => Event::Failed(String::from_utf8_lossy(&payload).into_owned()),
    }
}

/// An answer the console takes.
#[derive(Debug)]
pub enum Answer {
    /// Glyphs at `cell`, checked.
    Glyphs { cell: Cell, set: Arc<Set> },
}

/// The helper's process and its two threads.
struct Running {
    child: Child,
    requests: mpsc::Sender<Vec<u8>>,
    events: mpsc::Receiver<Event>,
}

/// The request the helper is answering.
struct InFlight {
    request: Request,
    /// When the next answer is due.
    due: Instant,
    /// Sent again to a helper started for it once already.
    resent: bool,
}

/// The helper as the console sees it: started on the first request,
/// fed one request at a time, given up for this boot at its first failure.
pub struct Helper {
    command: Command,
    /// Wakes the loop: called by the reader thread after each frame.
    wake: Arc<dyn Fn() + Send + Sync>,
    running: Option<Running>,
    queue: VecDeque<Request>,
    in_flight: Option<InFlight>,
    failed: Option<String>,
    answer: Duration,
}

impl Helper {
    pub fn new(command: Command, wake: Arc<dyn Fn() + Send + Sync>) -> Helper {
        Helper {
            command,
            wake,
            running: None,
            queue: VecDeque::new(),
            in_flight: None,
            failed: None,
            answer: Duration::from_millis(ANSWER_MS),
        }
    }

    /// Why the helper was given up, once it has been.
    pub fn failed(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// When [`Helper::poll`] must next run: the answer due.
    pub fn deadline(&self) -> Option<Instant> {
        self.in_flight.as_ref().map(|f| f.due)
    }

    /// Whether a request is queued or answering.
    pub fn busy(&self) -> bool {
        self.in_flight.is_some() || !self.queue.is_empty()
    }

    /// Asks for `request`, after those already asked.
    pub fn request(&mut self, request: Request, now: Instant) {
        if self.failed.is_some() {
            return;
        }
        self.queue.push_back(request);
        self.next(now);
    }

    /// Sends the next queued request when none is answering, starting the
    /// helper first when it is not running.
    fn next(&mut self, now: Instant) {
        if self.in_flight.is_some() || self.failed.is_some() {
            return;
        }
        let Some(request) = self.queue.pop_front() else {
            return;
        };
        self.send(
            InFlight {
                request,
                due: now,
                resent: false,
            },
            now,
        );
    }

    fn send(&mut self, mut flight: InFlight, now: Instant) {
        if self.running.is_none() {
            match self.start() {
                Ok(r) => self.running = Some(r),
                Err(why) => return self.fail(&why),
            }
        }
        if let Some(r) = &self.running {
            // a send fails only once the writer is gone, which the reader
            // reports as the helper closing
            let _ = r.requests.send(flight.request.encode());
        }
        flight.due = after(now, self.answer);
        self.in_flight = Some(flight);
    }

    fn start(&self) -> Result<Running, String> {
        let c = &self.command;
        // a descriptor the console holds, the VT's and the display's among
        // them, would otherwise reach the helper
        vtium_platform::cloexec_above_stdio()
            .map_err(|e| format!("cannot keep the console's descriptors from it: {e}"))?;
        let mut process = Process::new(&c.program);
        process
            .args(&c.args)
            .env_clear()
            .current_dir("/")
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some((uid, gid)) = c.account {
            // std drops the supplementary groups before setuid when root
            process.uid(uid).gid(gid);
        }
        let mut child = process
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", c.program.display()))?;
        let (stdin, stdout) = (child.stdin.take(), child.stdout.take());
        let (requests, to_write) = mpsc::channel::<Vec<u8>>();
        if let Some(mut stdin) = stdin {
            std::thread::spawn(move || {
                for bytes in to_write {
                    if stdin
                        .write_all(&bytes)
                        .and_then(|()| stdin.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        let (tx, events) = mpsc::channel();
        let wake = Arc::clone(&self.wake);
        if let Some(mut stdout) = stdout {
            std::thread::spawn(move || {
                loop {
                    let e = frame(&mut stdout);
                    let end = matches!(e, Event::Closed | Event::Malformed(_));
                    if tx.send(e).is_err() {
                        break;
                    }
                    wake();
                    if end {
                        break;
                    }
                }
            });
        }
        Ok(Running {
            child,
            requests,
            events,
        })
    }

    /// Gives the helper up: ends it, drops what was asked and logs `why`
    /// once.
    fn fail(&mut self, why: &str) {
        self.stop();
        self.queue.clear();
        if self.failed.is_none() {
            log::warn!(
                "the font's glyphs are not generated ({why}): vtium draws its built-in font for them"
            );
            self.failed = Some(why.to_owned());
        }
    }

    /// Ends the helper's process, if it runs, and waits for it.
    fn stop(&mut self) {
        self.in_flight = None;
        if let Some(mut r) = self.running.take() {
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
    }

    /// Takes what the helper has sent since the last call, and gives the
    /// helper up when its answer is past due.
    pub fn poll(&mut self, now: Instant) -> Vec<Answer> {
        let mut out = Vec::new();
        loop {
            let Some(event) = self.running.as_ref().and_then(|r| r.events.try_recv().ok()) else {
                break;
            };
            match event {
                Event::Glyphs(set) => {
                    let Some(flight) = &mut self.in_flight else {
                        self.fail("the helper answered what was not asked");
                        break;
                    };
                    let cell = flight.request.cell;
                    let fits = set.cells() == [cell]
                        && set
                            .strikes()
                            .all(|s| s.style == vtium_font::Style::default());
                    if !fits {
                        self.fail("the helper answered glyphs at a cell not asked for");
                        break;
                    }
                    flight.due = after(now, self.answer);
                    out.push(Answer::Glyphs { cell, set });
                }
                Event::Done => {
                    if self.in_flight.take().is_none() {
                        self.fail("the helper answered what was not asked");
                        break;
                    }
                    self.next(now);
                }
                Event::Failed(why) => {
                    self.fail(&format!("the helper failed: {}", why.trim()));
                    break;
                }
                Event::Malformed(why) => {
                    self.fail(&format!("the helper's answer was refused: {why}"));
                    break;
                }
                Event::Closed => {
                    self.closed(now);
                    break;
                }
            }
        }
        if self.in_flight.as_ref().is_some_and(|f| f.due <= now) {
            self.fail(&format!(
                "the helper gave no answer within {} s and was ended",
                self.answer.as_secs()
            ));
        }
        out
    }

    /// The helper closed its end: an idle exit, or a failure.
    fn closed(&mut self, now: Instant) {
        let Some(mut r) = self.running.take() else {
            return;
        };
        let status = match r.child.try_wait() {
            Ok(Some(status)) => Some(status),
            // closed its output and still running: ended here
            _ => {
                let _ = r.child.kill();
                r.child.wait().ok()
            }
        };
        let clean = status.is_some_and(|s| s.success());
        match self.in_flight.take() {
            None if clean => self.next(now),
            Some(flight) if clean && !flight.resent => self.send(
                InFlight {
                    resent: true,
                    ..flight
                },
                now,
            ),
            _ => {
                let why = match status {
                    Some(s) if s.signal().is_some() => format!(
                        "the helper was ended by signal {}",
                        s.signal().unwrap_or_default()
                    ),
                    Some(s) => format!("the helper exited: {s}"),
                    None => "the helper exited".to_owned(),
                };
                self.fail(&why);
            }
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        self.stop();
    }
}

fn after(t: Instant, d: Duration) -> Instant {
    t.checked_add(d).unwrap_or(t)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use std::path::Path;
    use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder};
    use vtium_font::{Style, Width};

    const CELL: Cell = Cell {
        width: 8,
        height: 16,
    };

    /// A set of one glyph, `c` solid, at `cell`.
    fn set(cell: Cell, c: char) -> Vec<u8> {
        let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default());
        let g = s.glyph(
            Width::Narrow,
            vec![255; (cell.width * cell.height) as usize],
        );
        s.map(c, g);
        SetBuilder::new("t").strike(s).write().unwrap()
    }

    fn frame_bytes(kind: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = kind.to_le_bytes().to_vec();
        out.extend_from_slice(&len32(payload.len()).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    /// A helper whose answers are `script`, run by the shell after it
    /// writes `fixture` where the script finds it as `$1`.
    struct StandIn {
        helper: Helper,
        woke: mpsc::Receiver<()>,
        dir: PathBuf,
    }

    impl StandIn {
        fn new(name: &str, script: &str, fixture: &[u8]) -> StandIn {
            let dir =
                std::env::temp_dir().join(format!("vtium-helper-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let fixture_path = dir.join("answer");
            std::fs::write(&fixture_path, fixture).unwrap();
            let (tx, woke) = mpsc::channel();
            let tx = std::sync::Mutex::new(tx);
            let command = Command {
                program: PathBuf::from("/bin/sh"),
                args: vec![
                    "-c".into(),
                    script.into(),
                    "stand-in".into(),
                    fixture_path.into(),
                ],
                account: None,
            };
            let wake = Arc::new(move || {
                let _ = tx.lock().map(|t| t.send(()));
            });
            StandIn {
                helper: Helper::new(command, wake),
                woke,
                dir,
            }
        }

        fn ask(&mut self, now: Instant) {
            self.helper.request(
                Request {
                    frit: PathBuf::from("/etc/vtium/fonts/t.vtfrit"),
                    cell: CELL,
                    first: vec!['A'],
                    console: None,
                },
                now,
            );
        }

        /// Polls as the loop does, each time the reader wakes it, until the
        /// helper is idle or given up; the answers taken.
        fn run(&mut self, now: Instant) -> Vec<Answer> {
            let mut out = Vec::new();
            for _ in 0..50 {
                out.extend(self.helper.poll(now));
                if !self.helper.busy() || self.helper.failed().is_some() {
                    break;
                }
                let _ = self.woke.recv_timeout(Duration::from_secs(10));
            }
            out
        }
    }

    impl Drop for StandIn {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// The stand-ins' clock: a fixed instant, moved on by hand.
    #[expect(clippy::disallowed_methods, reason = "the tests' fixed instant")]
    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn a_request_is_the_helpers_format() {
        let bytes = Request {
            frit: PathBuf::from("/a"),
            cell: CELL,
            first: vec!['A'],
            console: None,
        }
        .encode();
        let mut want = b"VTVSREQ1".to_vec();
        want.extend_from_slice(&2u32.to_le_bytes());
        want.extend_from_slice(b"/a");
        for v in [8u32, 16, 1, 0x41, 0] {
            want.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(bytes, want);
    }

    #[test]
    fn glyphs_at_the_cell_asked_for_are_taken() {
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, 'A'));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new("good", "cat >/dev/null & cat \"$1\"; wait", &fixture);
        let now = t0();
        s.ask(now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 1);
        let Answer::Glyphs { cell, set } = &answers[0];
        assert_eq!(*cell, CELL);
        assert_eq!(set.cells(), vec![CELL]);
        assert!(!s.helper.busy());
    }

    #[test]
    fn a_helper_that_exits_at_its_idle_interval_is_started_again() {
        // it answers, then exits 0 as at its idle interval; the next
        // request starts it again
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, 'A'));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new("idle", "head -c 1 >/dev/null; cat \"$1\"", &fixture);
        let now = t0();
        s.ask(now);
        assert_eq!(s.run(now).len(), 1);
        // the exit is seen, and is no failure
        let _ = s.woke.recv_timeout(Duration::from_secs(10));
        assert!(s.helper.poll(now).is_empty());
        assert_eq!(s.helper.failed(), None);
        s.ask(now);
        assert_eq!(s.run(now).len(), 1);
        assert_eq!(s.helper.failed(), None);
    }

    fn given_up(name: &str, script: &str, fixture: &[u8]) -> String {
        let mut s = StandIn::new(name, script, fixture);
        let now = t0();
        s.ask(now);
        let answers = s.run(now);
        assert!(answers.is_empty(), "{answers:?}");
        let why = s.helper.failed().unwrap().to_owned();
        // given up: nothing more is asked
        s.ask(now);
        assert!(!s.helper.busy());
        why
    }

    #[test]
    fn a_helper_that_dies_is_given_up() {
        let why = given_up("dies", "kill -SEGV $$", b"");
        assert_eq!(why, "the helper was ended by signal 11");
        let why = given_up("exits", "cat >/dev/null & exit 3", b"");
        assert!(why.contains("exit status: 3"), "{why}");
    }

    #[test]
    fn garbage_is_refused() {
        let why = given_up(
            "garbage",
            "cat >/dev/null & cat \"$1\"; wait",
            b"VTVOUTL1 not a frame",
        );
        assert!(why.contains("refused"), "{why}");
        // a set its reader refuses
        let why = given_up(
            "badset",
            "cat >/dev/null & cat \"$1\"; wait",
            &frame_bytes(FRAME_GLYPHS, b"VTGLASS\0junk"),
        );
        assert!(why.contains("cannot read"), "{why}");
        // glyphs at another cell than asked for
        let why = given_up(
            "cell",
            "cat >/dev/null & cat \"$1\"; wait",
            &frame_bytes(
                FRAME_GLYPHS,
                &set(
                    Cell {
                        width: 8,
                        height: 8,
                    },
                    'A',
                ),
            ),
        );
        assert!(why.contains("cell not asked for"), "{why}");
        // a failure the helper reports
        let why = given_up(
            "failed",
            "cat >/dev/null & cat \"$1\"; wait",
            &frame_bytes(FRAME_FAILED, b"no source of the frit is here"),
        );
        assert!(why.contains("no source of the frit is here"), "{why}");
    }

    #[test]
    fn a_frame_past_its_bound_is_refused_before_allocation() {
        // a length of 4 GiB: refused on the head, with no allocation of it
        let mut huge = FRAME_GLYPHS.to_le_bytes().to_vec();
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        let e = frame(&mut &huge[..]);
        assert!(
            matches!(&e, Event::Malformed(why) if why.contains("past its kind's")),
            "{e:?}"
        );
        let mut done = FRAME_DONE.to_le_bytes().to_vec();
        done.extend_from_slice(&1u32.to_le_bytes());
        assert!(matches!(frame(&mut &done[..]), Event::Malformed(_)));
        let why = given_up("huge", "cat >/dev/null & cat \"$1\"; wait", &huge);
        assert!(why.contains("past its kind's"), "{why}");
    }

    #[test]
    fn a_helper_that_hangs_is_ended_at_its_deadline() {
        let mut s = StandIn::new("hangs", "exec sleep 30", b"");
        let now = t0();
        s.ask(now);
        assert!(s.helper.poll(now).is_empty());
        assert_eq!(s.helper.failed(), None);
        assert_eq!(
            s.helper.deadline(),
            Some(after(now, Duration::from_millis(ANSWER_MS)))
        );
        let late = after(now, Duration::from_millis(ANSWER_MS));
        assert!(s.helper.poll(late).is_empty());
        let why = s.helper.failed().unwrap();
        assert!(why.contains("no answer within 10 s"), "{why}");
        assert!(s.helper.running.is_none());
    }

    #[test]
    fn a_missing_helper_is_given_up() {
        let mut h = Helper::new(
            Command {
                program: Path::new("/nonexistent/vtium-vitrify").to_owned(),
                args: Vec::new(),
                account: None,
            },
            Arc::new(|| {}),
        );
        h.request(
            Request {
                frit: PathBuf::from("/a"),
                cell: CELL,
                first: Vec::new(),
                console: None,
            },
            t0(),
        );
        assert!(h.failed().unwrap().contains("cannot start"));
    }

    #[test]
    fn nobody_is_read_from_passwd() {
        let passwd = "root:x:0:0::/root:/bin/sh\nnobody:x:65534:65534:Nobody:/:/usr/bin/nologin\n";
        assert_eq!(nobody(passwd), Some((65534, 65534)));
        assert_eq!(nobody("nobody:x:0:0::/:/bin/sh\n"), None);
    }
}
