//! The glyph helper's client (ADR-1301 D2): the console
//! starts `vtium-vitrify serve` on its first need for a frit's glyphs,
//! asks it for one cell's glyphs at a time, the screen's codepoints first,
//! and takes each answer as it arrives without ever waiting on it.
//!
//! The helper runs with an empty environment in `/`, in its own process
//! group, holding only its pipes, as `nobody` when the console is root
//! (ADR-1400 D2's last account before login), and sandboxes itself. A
//! thread writes the requests and another reads the answers, so neither
//! a slow nor a hung helper stalls the loop; the reader wakes the loop
//! after each answer; the reader also reaps the helper as it exits. Each
//! frame is checked before it is trusted: its length against
//! [`FRAME_BYTES_MAX`] before anything is allocated, a set through
//! vtium-font's reader, which bounds every field, every strike at the cell
//! asked for, regular in style, no codepoint the cell holds already, and
//! the cell's frames inside [`CELL_BYTES_MAX`].
//!
//! A helper that crashes, panics, fails a request, answers what is not a
//! frame, answers a request with no glyph, gives no answer for
//! [`ANSWER_MS`], or leaves a request unfinished for [`REQUEST_MS`] is
//! ended, and the console logs one line and asks it nothing more: the
//! built-in font goes on drawing what the helper has not sent. A helper
//! that exits 0 between requests left at its idle interval, and the next
//! request starts another; one that left as a request went out, before
//! answering it, is started again for it, once. One its CPU limit ended is
//! replaced at the next request, the request it was answering dropped, at
//! most [`RESTARTS_MAX`] times a boot.
//!
//! The wire format is `vtium-vitrify`'s serve mode's (`serve.rs`), whose
//! constants this module repeats.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Command as Process, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rustix::process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal};
use vtium_font::Cell;
use vtium_font::set::Set;
use vtium_platform::vt::font::ConsoleFont;

/// How long the helper may take for each answer, a frame or the request's
/// end, before it is ended: a CJK frit's first batch on a Cortex-A53 takes
/// about a second.
pub const ANSWER_MS: u64 = 10_000;
/// How long one request may take whole, however often its frames come.
pub const REQUEST_MS: u64 = 60_000;
/// The most bytes of glyphs one cell's frames hold.
pub const CELL_BYTES_MAX: u64 = 64 << 20;
/// How many times a boot replaces a helper its CPU limit ended.
pub const RESTARTS_MAX: u32 = 3;
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
    /// The kernel's font, read from the VT at start (vtium/vtium#145).
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
    /// A set and the codepoints it maps, as `(first, count)` runs, with
    /// the bytes its frame took.
    Glyphs {
        set: Arc<Set>,
        runs: Vec<(u32, u32)>,
        bytes: u64,
    },
    Done,
    Failed(String),
    /// Bytes that are not a frame, or a set its reader refused.
    Malformed(String),
    /// The helper closed its end and was reaped: how it exited.
    Closed(Option<ExitStatus>),
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

/// The codepoints a set maps, as `(first, count)` runs, read from its
/// `CMAP` section as vtium-font's format lays it out: every offset and
/// count is checked against the bytes, so a malformed set is an error,
/// and the set's reader checks the rest.
fn mapped_runs(bytes: &[u8]) -> Result<Vec<(u32, u32)>, String> {
    let bad = || "glyphs whose map vtium cannot read".to_owned();
    let field = |at: usize, n: usize| bytes.get(at..at.checked_add(n)?);
    let u32_at = |at: usize| {
        field(at, 4)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
    };
    let u64_at = |at: usize| {
        field(at, 8)
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
    };
    let usize_of = |v: u64| usize::try_from(v).ok();
    let sections = u32_at(12).ok_or_else(bad)?;
    let (mut off, mut len) = (None, None);
    for i in 0..sections {
        let entry = usize_of(u64::from(i))
            .and_then(|i| i.checked_mul(20))
            .and_then(|e| e.checked_add(16))
            .ok_or_else(bad)?;
        if field(entry, 4).ok_or_else(bad)? == b"CMAP" {
            off = entry.checked_add(4).and_then(u64_at).and_then(usize_of);
            len = entry.checked_add(12).and_then(u64_at).and_then(usize_of);
            break;
        }
    }
    let (off, len) = off.zip(len).ok_or_else(bad)?;
    let cmap = bytes
        .get(off..off.checked_add(len).ok_or_else(bad)?)
        .ok_or_else(bad)?;
    let at32 = |at: usize| {
        cmap.get(at..at.checked_add(4)?)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
    };
    let strikes = at32(0).ok_or_else(bad)?;
    let record = usize_of(u64::from(at32(4).ok_or_else(bad)?)).ok_or_else(bad)?;
    if record < 16 {
        return Err(bad());
    }
    let mut at = 8usize;
    let mut runs = Vec::new();
    for _ in 0..strikes {
        let count = at32(at).ok_or_else(bad)?;
        at = at.checked_add(4).ok_or_else(bad)?;
        for _ in 0..count {
            let first = at32(at).ok_or_else(bad)?;
            let n = at.checked_add(4).and_then(at32).ok_or_else(bad)?;
            runs.push((first, n));
            at = at.checked_add(record).ok_or_else(bad)?;
        }
    }
    Ok(runs)
}

/// Reads one frame into an event, or `None` at the end of the stream: its
/// length checked against its kind's bound before its payload is
/// allocated.
fn frame(r: &mut impl Read) -> Option<Event> {
    let (kind, len) = match head(r) {
        Ok(Some(h)) => h,
        Ok(None) => return None,
        Err(why) => return Some(Event::Malformed(why)),
    };
    let max = match kind {
        FRAME_GLYPHS => FRAME_BYTES_MAX,
        FRAME_DONE => 0,
        FRAME_FAILED => REASON_BYTES_MAX,
        other => return Some(Event::Malformed(format!("a frame of unknown kind {other}"))),
    };
    if len > max {
        return Some(Event::Malformed(format!(
            "a frame of {len} bytes, past its kind's {max}"
        )));
    }
    let mut payload = vec![0u8; usize::try_from(len).unwrap_or(0)];
    if r.read_exact(&mut payload).is_err() {
        return Some(Event::Malformed("a frame cut short".to_owned()));
    }
    Some(match kind {
        FRAME_GLYPHS => match mapped_runs(&payload) {
            Err(why) => Event::Malformed(why),
            Ok(runs) => match Set::from_bytes(payload) {
                Ok(set) => Event::Glyphs {
                    set: Arc::new(set),
                    runs,
                    bytes: u64::from(len),
                },
                Err(e) => Event::Malformed(format!("glyphs vtium cannot read: {e}")),
            },
        },
        FRAME_DONE => Event::Done,
        _ => Event::Failed(String::from_utf8_lossy(&payload).into_owned()),
    })
}

/// An answer the console takes.
#[derive(Debug)]
pub enum Answer {
    /// Glyphs at `cell`, checked.
    Glyphs { cell: Cell, set: Arc<Set> },
}

/// The helper's process: its pidfd, which ends it, the writer's queue and
/// the reader's events. The reader thread owns the child and reaps it.
struct Running {
    pidfd: Option<OwnedFd>,
    requests: mpsc::Sender<Vec<u8>>,
    events: mpsc::Receiver<Event>,
}

/// The request the helper is answering.
struct InFlight {
    request: Request,
    /// When it was asked, and when its first glyphs came.
    asked: Instant,
    first: Option<Instant>,
    /// When the next answer is due.
    due: Instant,
    /// When the request must be answered whole.
    end: Instant,
    /// Sent again to a helper started for it once already.
    resent: bool,
}

/// What the helper has sent at one cell this boot: a bit for each
/// codepoint, and the bytes of its frames.
struct Held {
    cell: Cell,
    bits: Vec<u64>,
    bytes: u64,
}

/// One bit for each Unicode codepoint.
const CODEPOINT_WORDS: usize = 0x11_0000 / 64;

impl Held {
    fn new(cell: Cell) -> Held {
        Held {
            cell,
            bits: vec![0; CODEPOINT_WORDS],
            bytes: 0,
        }
    }

    /// Marks each codepoint of `runs`; false when one is past Unicode or
    /// already held.
    fn mark(&mut self, runs: &[(u32, u32)]) -> bool {
        for &(first, n) in runs {
            for c in (0..n).map(|i| first.checked_add(i)) {
                let Some(c) = c else {
                    return false;
                };
                let (word, bit) = ((c / 64) as usize, c % 64);
                let Some(w) = self.bits.get_mut(word) else {
                    return false;
                };
                if *w & (1 << bit) != 0 {
                    return false;
                }
                *w |= 1 << bit;
            }
        }
        true
    }
}

/// The helper as the console sees it: started on the first request,
/// fed one request at a time, replaced at the next request when its CPU
/// limit ended it, at most [`RESTARTS_MAX`] times a boot, and otherwise
/// given up for this boot at its first failure.
pub struct Helper {
    command: Command,
    /// Wakes the loop: called by the reader thread after each frame.
    wake: Arc<dyn Fn() + Send + Sync>,
    running: Option<Running>,
    queue: VecDeque<Request>,
    in_flight: Option<InFlight>,
    held: Vec<Held>,
    restarts: u32,
    failed: Option<String>,
    answer: Duration,
    request: Duration,
    cell_bytes: u64,
    /// When vtium started, for the line each answered request logs.
    epoch: Instant,
}

impl Helper {
    /// The helper `command` starts, whose reader calls `wake` after each
    /// answer, in a vtium started at `epoch`.
    pub fn new(command: Command, wake: Arc<dyn Fn() + Send + Sync>, epoch: Instant) -> Helper {
        Helper {
            command,
            wake,
            epoch,
            running: None,
            queue: VecDeque::new(),
            in_flight: None,
            held: Vec::new(),
            restarts: 0,
            failed: None,
            answer: Duration::from_millis(ANSWER_MS),
            request: Duration::from_millis(REQUEST_MS),
            cell_bytes: CELL_BYTES_MAX,
        }
    }

    /// Why the helper was given up, once it has been.
    pub fn failed(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// When [`Helper::poll`] must next run: the next answer due, or the
    /// request's end, whichever comes first.
    pub fn deadline(&self) -> Option<Instant> {
        self.in_flight.as_ref().map(|f| f.due.min(f.end))
    }

    /// Whether a request is queued or answering.
    pub fn busy(&self) -> bool {
        self.in_flight.is_some() || !self.queue.is_empty()
    }

    /// Forgets what was sent at every cell but `cells`, as the console
    /// drops those glyphs, so a cell asked for again starts afresh.
    pub fn retain(&mut self, cells: &[Cell]) {
        self.held.retain(|h| cells.contains(&h.cell));
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
                asked: now,
                first: None,
                due: now,
                end: after(now, self.request),
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
        // a pidfd names this child alone, even once the reader has reaped it
        let pidfd = i32::try_from(child.id())
            .ok()
            .and_then(Pid::from_raw)
            .and_then(|pid| pidfd_open(pid, PidfdFlags::empty()).ok());
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
        std::thread::spawn(move || {
            if let Some(mut stdout) = stdout {
                while let Some(e) = frame(&mut stdout) {
                    let malformed = matches!(e, Event::Malformed(_));
                    if tx.send(e).is_err() {
                        break;
                    }
                    wake();
                    if malformed {
                        break;
                    }
                }
            }
            // reaped here, as it exits, so a helper that closes its output
            // a moment before it exits is waited for, never ended
            let status = child.wait().ok();
            let _ = tx.send(Event::Closed(status));
            wake();
        });
        Ok(Running {
            pidfd,
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

    /// Ends the helper's process, if it runs; its reader reaps it.
    fn stop(&mut self) {
        self.in_flight = None;
        if let Some(fd) = self.running.take().and_then(|r| r.pidfd) {
            let _ = pidfd_send_signal(&fd, Signal::KILL);
        }
    }

    /// Takes what the helper has sent since the last call, and gives the
    /// helper up when its answer or its request is past due.
    pub fn poll(&mut self, now: Instant) -> Vec<Answer> {
        let mut out = Vec::new();
        loop {
            let Some(event) = self.running.as_ref().and_then(|r| r.events.try_recv().ok()) else {
                break;
            };
            match event {
                Event::Glyphs { set, runs, bytes } => match self.glyphs(&set, &runs, bytes, now) {
                    Ok(cell) => out.push(Answer::Glyphs { cell, set }),
                    Err(why) => {
                        self.fail(why);
                        break;
                    }
                },
                Event::Done => {
                    let Some(flight) = self.in_flight.take() else {
                        self.fail("the helper answered what was not asked");
                        break;
                    };
                    if flight.first.is_none() {
                        let c = flight.request.cell;
                        self.fail(&format!(
                            "the helper answered no glyph at {}x{}",
                            c.width, c.height
                        ));
                        break;
                    }
                    self.done(&flight, now);
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
                Event::Closed(status) => {
                    self.closed(status, now);
                    break;
                }
            }
        }
        match &self.in_flight {
            Some(f) if f.end <= now => self.fail(&format!(
                "the helper did not finish a request within {} s and was ended",
                self.request.as_secs()
            )),
            Some(f) if f.due <= now => self.fail(&format!(
                "the helper gave no answer within {} s and was ended",
                self.answer.as_secs()
            )),
            _ => {}
        }
        out
    }

    /// Checks a frame of glyphs against the request answering: every
    /// strike at the cell asked for and regular, no codepoint the cell
    /// holds already, and the cell's bytes inside its budget.
    fn glyphs(
        &mut self,
        set: &Set,
        runs: &[(u32, u32)],
        bytes: u64,
        now: Instant,
    ) -> Result<Cell, &'static str> {
        let flight = self
            .in_flight
            .as_mut()
            .ok_or("the helper answered what was not asked")?;
        let cell = flight.request.cell;
        let fits = set.cells() == [cell]
            && set
                .strikes()
                .all(|s| s.style == vtium_font::Style::default());
        if !fits {
            return Err("the helper answered glyphs at a cell not asked for");
        }
        let held = match self.held.iter().position(|h| h.cell == cell) {
            Some(i) => self.held.get_mut(i),
            None => {
                self.held.push(Held::new(cell));
                self.held.last_mut()
            }
        }
        .ok_or("the helper answered what was not asked")?;
        held.bytes = held.bytes.saturating_add(bytes);
        if held.bytes > self.cell_bytes {
            return Err("the helper's glyphs at one cell passed their byte budget");
        }
        if !held.mark(runs) {
            return Err("the helper's glyphs repeat a codepoint it sent");
        }
        flight.due = after(now, self.answer);
        flight.first.get_or_insert(now);
        Ok(cell)
    }

    /// Logs a request answered whole: when it was asked after vtium
    /// started, and how long its first glyphs and the last took.
    fn done(&self, flight: &InFlight, now: Instant) {
        let ms = |d: Duration| d.as_millis();
        let cell = flight.request.cell;
        let since = |t: Instant| ms(t.saturating_duration_since(flight.asked));
        log::info!(
            "the font's glyphs at {}x{} are generated: asked {} ms after vtium started, the first in {} ms, the last in {} ms",
            cell.width,
            cell.height,
            ms(flight.asked.saturating_duration_since(self.epoch)),
            flight.first.map_or(0, since),
            since(now)
        );
    }

    /// The helper closed its end and was reaped. An exit 0 is its idle
    /// exit: the request it left unread, if any, goes to another, once. An
    /// end by its CPU limit (`SIGXCPU`, or `SIGKILL` past the hard limit)
    /// replaces it at the next request, the one it was answering dropped,
    /// at most [`RESTARTS_MAX`] times a boot. Anything else gives it up.
    fn closed(&mut self, status: Option<ExitStatus>, now: Instant) {
        self.running = None;
        let clean = status.is_some_and(|s| s.success());
        let signal = status.and_then(|s| s.signal());
        let cpu = matches!(signal, Some(SIGXCPU | SIGKILL));
        match self.in_flight.take() {
            None if clean => self.next(now),
            Some(flight) if clean && !flight.resent && flight.first.is_none() => self.send(
                InFlight {
                    resent: true,
                    ..flight
                },
                now,
            ),
            flight if cpu && self.restarts < RESTARTS_MAX => {
                self.restarts = self.restarts.saturating_add(1);
                if let Some(f) = flight {
                    let c = f.request.cell;
                    log::warn!(
                        "the helper reached its CPU limit at {}x{}: the characters it had not sent there draw in the built-in font",
                        c.width,
                        c.height
                    );
                }
                self.next(now);
            }
            _ => {
                let why = match signal {
                    Some(_) if cpu => format!(
                        "the helper reached its CPU limit {} times",
                        self.restarts.saturating_add(1)
                    ),
                    Some(sig) => format!("the helper was ended by signal {sig}"),
                    None => match status {
                        Some(s) => format!("the helper exited: {s}"),
                        None => "the helper exited".to_owned(),
                    },
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

/// `SIGXCPU` and `SIGKILL`, the signals an exhausted `RLIMIT_CPU` sends.
const SIGXCPU: i32 = 24;
const SIGKILL: i32 = 9;

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
                helper: Helper::new(command, wake, t0()),
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
        // the cell asked for again, as after its glyphs were dropped
        s.helper.retain(&[]);
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
            matches!(&e, Some(Event::Malformed(why)) if why.contains("past its kind's")),
            "{e:?}"
        );
        let mut done = FRAME_DONE.to_le_bytes().to_vec();
        done.extend_from_slice(&1u32.to_le_bytes());
        assert!(matches!(frame(&mut &done[..]), Some(Event::Malformed(_))));
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
    fn a_frame_that_repeats_a_codepoint_is_refused() {
        // the same set over and over, as a stand-in that floods: given up at
        // the second, however long the flood
        let a = frame_bytes(FRAME_GLYPHS, &set(CELL, 'A'));
        let flood: Vec<u8> = a.iter().copied().cycle().take(a.len() * 20_000).collect();
        let mut s = StandIn::new("flood", "cat >/dev/null & cat \"$1\"; wait", &flood);
        let now = t0();
        s.ask(now);
        assert_eq!(s.run(now).len(), 1);
        let why = s.helper.failed().unwrap();
        assert!(why.contains("repeat"), "{why}");
    }

    #[test]
    fn glyphs_past_a_cells_byte_budget_are_refused() {
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, 'A'));
        fixture.extend(frame_bytes(FRAME_GLYPHS, &set(CELL, 'B')));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new("budget", "cat >/dev/null & cat \"$1\"; wait", &fixture);
        s.helper.cell_bytes = (fixture.len() / 2) as u64;
        let now = t0();
        s.ask(now);
        assert_eq!(s.run(now).len(), 1);
        let why = s.helper.failed().unwrap();
        assert!(why.contains("budget"), "{why}");
    }

    #[test]
    fn a_request_has_one_deadline_however_often_frames_come() {
        // a frame now and then keeps the answer due, but not the request's
        // end
        let mut s = StandIn::new(
            "deadline",
            "cat >/dev/null & cat \"$1\"; exec sleep 30",
            &frame_bytes(FRAME_GLYPHS, &set(CELL, 'A')),
        );
        let now = t0();
        s.ask(now);
        let _ = s.woke.recv_timeout(Duration::from_secs(10));
        let late = after(now, Duration::from_millis(REQUEST_MS));
        let nearly = after(now, Duration::from_millis(REQUEST_MS - 1));
        // the frame came at `nearly`'s doorstep: the answer is due 10 s on,
        // the request at REQUEST_MS
        assert_eq!(s.helper.poll(nearly).len(), 1);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(s.helper.deadline(), Some(late));
        assert!(s.helper.poll(late).is_empty());
        let why = s.helper.failed().unwrap();
        assert!(why.contains("did not finish"), "{why}");
    }

    #[test]
    fn a_request_answered_with_no_glyph_is_a_failure() {
        let why = given_up(
            "none",
            "cat >/dev/null & cat \"$1\"; wait",
            &frame_bytes(FRAME_DONE, &[]),
        );
        assert!(why.contains("no glyph"), "{why}");
    }

    #[test]
    fn a_helper_that_closes_before_it_exits_is_no_failure() {
        // its output closes, then it lingers before exiting 0: reaped, not
        // ended, and the next request starts another
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, 'A'));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new(
            "linger",
            "head -c 1 >/dev/null; cat \"$1\"; exec 1>&-; sleep 0.3; exit 0",
            &fixture,
        );
        let now = t0();
        s.ask(now);
        assert_eq!(s.run(now).len(), 1);
        while s.helper.running.is_some() && s.woke.recv_timeout(Duration::from_secs(10)).is_ok() {
            assert!(s.helper.poll(now).is_empty());
        }
        assert_eq!(s.helper.failed(), None);
        s.helper.retain(&[]);
        s.ask(now);
        assert_eq!(s.run(now).len(), 1);
        assert_eq!(s.helper.failed(), None);
    }

    #[test]
    fn a_helper_ended_by_its_cpu_limit_is_replaced_a_bounded_number_of_times() {
        // each helper answers its request, then the kernel ends it for its
        // CPU time: the next request starts another, up to RESTARTS_MAX
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, 'A'));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new(
            "cpu",
            "head -c 1 >/dev/null; cat \"$1\"; kill -XCPU $$",
            &fixture,
        );
        let now = t0();
        for round in 0..=RESTARTS_MAX {
            s.helper.retain(&[]);
            s.ask(now);
            assert_eq!(s.run(now).len(), 1, "round {round}");
            while s.helper.running.is_some() && s.woke.recv_timeout(Duration::from_secs(10)).is_ok()
            {
                let _ = s.helper.poll(now);
            }
            if round < RESTARTS_MAX {
                assert_eq!(s.helper.failed(), None, "round {round}");
            }
        }
        let why = s.helper.failed().unwrap();
        assert!(why.contains("CPU limit"), "{why}");
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
            t0(),
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
