//! The glyph helper's client (ADR-1301 D2): the console starts
//! `vtium-vitrify serve` on its first need for a frit's glyphs, and takes
//! each answer as it arrives without ever waiting on it.
//!
//! It asks under the operator's fill model (2026-10-28). An ask names the
//! characters and the clusters a cell shows that the store does not hold,
//! has not been told the frit lacks, and has not already asked for; the
//! helper answers it ahead of anything else, each cluster shaped and drawn
//! whole (ADR-1200 D21). The console's record of each cell keys a glyph as
//! the store does (ADR-1200 D20), a cluster by its whole text. A fill fills a cell's coverage while nothing is
//! asked: the screen's characters, then printable Latin and box drawing,
//! then the rest, a batch at a time, a waiting ask answered before the
//! next batch. The fill stops at the store's limit ([`crate::deferred`]),
//! so speculative glyphs never churn the store; only an ask past the limit
//! evicts, compacting the store by the overrun, the glyphs least recently
//! shown leaving, and an evicted glyph is asked for again when the screen
//! shows it.
//!
//! The helper runs with an empty environment in `/`, in its own process
//! group, holding only its pipes, as `nobody` when the console is root
//! (ADR-1400 D2's last account before login), and sandboxes itself. A
//! thread writes the requests and another reads the answers, so neither
//! a slow nor a hung helper stalls the loop; the reader wakes the loop
//! after each answer, and reaps the helper as it exits. Each frame is
//! checked before it is trusted: its length against [`FRAME_BYTES_MAX`]
//! before anything is allocated, a set through vtium-font's reader, which
//! bounds every field, every strike at a cell asked for and regular in
//! style, an ask's glyphs and clusters among those it asked for and not yet
//! sent, and a fill's among those the cell does not hold, with no cluster. The helper says when it takes
//! each stop, and a cell's fill frames before that belong to the fill
//! stopped and are dropped, so a cell filled again at once takes only its
//! new fill.
//!
//! A warning the frit's text gave as the helper read it, such as a key that
//! does nothing, is logged once a boot, at most [`WARNINGS_MAX`] of them.
//!
//! A helper that answers what is not a frame, what was not asked, or a
//! failure is given up for the boot, logged once, and the built-in font
//! goes on drawing what it has not sent, its clusters unshaped, a glyph a
//! codepoint (ADR-1301 D1). One that crashes, reaches its CPU
//! limit, gives no answer for [`ANSWER_MS`], leaves an ask unanswered for
//! [`REQUEST_MS`], or exits with work outstanding is started again, its
//! asks and fills sent again, at most [`RESTARTS_MAX`] times a boot. One
//! that exits 0 with nothing outstanding left at its idle interval, and
//! the next need starts another.
//!
//! A frit of bitmap sources that states no cell takes its fonts' own cell
//! (the operator, 2026-10-28), which the console asks the helper for
//! before its first frame ([`Helper::own_cell`]): the one wait on the
//! helper, at most [`OWN_CELL_MS`]. An answer that comes after it is
//! dropped.
//!
//! The wire format is `vtium-vitrify`'s serve mode's (`serve.rs`), whose
//! constants this module repeats.

use std::collections::{BTreeSet, VecDeque};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command as Process, ExitStatus, Stdio};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rustix::process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal};
use vtium_font::Cell;
use vtium_font::set::{Set, SetFace};
use vtium_platform::vt::font::ConsoleFont;

use crate::deferred::{Shared, Shown, Text};

/// How long the helper may take for each answer while work is
/// outstanding: a CJK frit's first batch on a Cortex-A53 takes about a
/// second.
pub const ANSWER_MS: u64 = 10_000;
/// How long one ask may take whole, however often frames come.
pub const REQUEST_MS: u64 = 60_000;
/// How many times a boot starts the helper again after it crashed,
/// reached its CPU limit, hung, or exited with work outstanding.
// Each start re-randomizes the helper's address layout, so an unbounded count would let an exploit retry until its guess lands (the operator, 2026-10-28).
pub const RESTARTS_MAX: u32 = 3;
/// The most bytes one frame's payload holds, the helper's
/// `FRAME_BYTES_MAX`.
pub const FRAME_BYTES_MAX: u32 = 16 << 20;
/// The most bytes of a failure's reason kept.
pub const REASON_BYTES_MAX: u32 = 4096;
/// The most codepoints one request names, the helper's `FIRST_MAX`, and the
/// most clusters.
pub const FIRST_MAX: usize = 32_000;
/// The most clusters a cell's record of those the frit lacks holds: past it
/// the record is emptied, and a cluster shown again is asked for again.
// A program can write clusters without end, each different; a screen shows at most FIRST_MAX, and 4,096 is a screen's worth of distinct ones on minspec's 80x25.
pub const CLUSTERS_LACKED_MAX: usize = 4096;
/// The most clusters a cell has asked for and not been answered: past it a
/// cluster shown draws unshaped, and is asked for on a later change.
pub const CLUSTERS_WAITING_MAX: usize = CLUSTERS_LACKED_MAX;
/// The most asks the helper has not answered whole: past it nothing more
/// is asked, and what the screen shows is asked for on a later change, so
/// a program that writes faster than the helper answers grows no queue.
// One ask a cell a turn, a few cells at most; 64 is many turns of a busy screen behind.
pub const ASKS_MAX: usize = 64;
/// The most warnings from the frit's text logged a boot; a helper that
/// sends more is not given up, and the rest are not logged.
// A frit's text gives a warning a line at most, and a frit worth drawing has few lines that do nothing; the bound keeps a helper that sends warnings without end out of the log.
pub const WARNINGS_MAX: usize = 16;
/// How long the console waits before its first frame for a frit's own
/// cell.
// The helper starts and reads a PSF well inside it on minspec, and a start that waits longer stalls visibly (the operator, 2026-10-28).
pub const OWN_CELL_MS: u64 = 100;

const REQUEST_MAGIC: &[u8; 8] = b"VTVSREQ3";
const ASK: u32 = 1;
const FILL: u32 = 2;
const STOP: u32 = 3;
const OWN_CELL: u32 = 4;
const FRAME_GLYPHS: u32 = 1;
const FRAME_DONE: u32 = 2;
const FRAME_FAILED: u32 = 3;
const FRAME_FILL: u32 = 4;
const FRAME_FILLED: u32 = 5;
const FRAME_OWN_CELL: u32 = 6;
const FRAME_NO_OWN_CELL: u32 = 7;
const FRAME_STOPPED: u32 = 8;
const FRAME_WARNING: u32 = 9;

/// `SIGXCPU`, which an exhausted `RLIMIT_CPU` sends.
const SIGXCPU: i32 = 24;

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
    /// `vtium-vitrify serve`, as `nobody` when the console is root; an
    /// error as root with no `nobody` in `/etc/passwd`, since the helper
    /// never runs as root.
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

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A frit as the console hands it to the helper: its name and its text,
/// read from its file or made on the fly (ADR-1200 D9), so the helper
/// needs no file for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FritText {
    pub name: String,
    pub text: String,
}

impl FritText {
    /// The frit at `path`, named for its file less `.vtfrit`.
    pub fn read(path: &Path) -> Result<FritText, String> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.strip_suffix(".vtfrit").unwrap_or(n))
            .filter(|n| !n.is_empty())
            .ok_or_else(|| format!("{} names no frit", path.display()))?;
        let text = crate::font::frit_text(path)
            .map_err(|why| format!("cannot read {}: {why}", path.display()))?;
        Ok(FritText {
            name: name.to_owned(),
            text,
        })
    }
}

/// One request's bytes: `kind` at `cell` for `frit`, naming `codepoints`
/// and `clusters` (each cut at [`FIRST_MAX`]) and `held`, with the
/// kernel's font. The display's capabilities are reserved, and sent empty.
fn encode(
    frit: &FritText,
    console: Option<&ConsoleFont>,
    kind: u32,
    cell: Cell,
    (codepoints, clusters): (&[u32], &[Text]),
    held: &[(u32, u32)],
) -> Vec<u8> {
    let mut out = REQUEST_MAGIC.to_vec();
    put32(&mut out, kind);
    for field in [&frit.name, &frit.text] {
        put32(&mut out, len32(field.len()));
        out.extend_from_slice(field.as_bytes());
    }
    put32(&mut out, cell.width);
    put32(&mut out, cell.height);
    put32(&mut out, 0);
    let codepoints = codepoints.get(..FIRST_MAX).unwrap_or(codepoints);
    put32(&mut out, len32(codepoints.len()));
    for &c in codepoints {
        put32(&mut out, c);
    }
    let clusters = clusters.get(..FIRST_MAX).unwrap_or(clusters);
    put32(&mut out, len32(clusters.len()));
    for cluster in clusters {
        put32(&mut out, len32(cluster.len()));
        for &c in cluster {
            put32(&mut out, u32::from(c));
        }
    }
    put32(&mut out, len32(held.len()));
    for &(a, b) in held {
        put32(&mut out, a);
        put32(&mut out, b);
    }
    match console {
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

/// What the reader thread saw.
#[derive(Debug)]
enum Event {
    /// A set, an ask's or a fill's, the codepoints it maps as `(first,
    /// count)` runs, and the bytes its frame took.
    Glyphs {
        fill: bool,
        set: Arc<Set>,
        runs: Vec<(u32, u32)>,
        bytes: u64,
    },
    /// The oldest ask is answered.
    Done,
    /// A cell's fill is done.
    Filled(Cell),
    /// A stop at a cell is taken: the frames of the fill it ended are all
    /// sent.
    Stopped(Cell),
    /// The frit's own cell, a cell query's answer.
    OwnCell(Cell),
    /// The frit has no own cell: why.
    NoOwnCell(String),
    /// A warning the frit's text gave as the helper read it.
    Warning(String),
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
        FRAME_GLYPHS | FRAME_FILL => FRAME_BYTES_MAX,
        FRAME_DONE => 0,
        FRAME_FILLED | FRAME_OWN_CELL | FRAME_STOPPED => 8,
        FRAME_FAILED | FRAME_NO_OWN_CELL | FRAME_WARNING => REASON_BYTES_MAX,
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
        FRAME_GLYPHS | FRAME_FILL => match mapped_runs(&payload) {
            Err(why) => Event::Malformed(why),
            Ok(runs) => match Set::from_bytes(payload) {
                Ok(set) => Event::Glyphs {
                    fill: kind == FRAME_FILL,
                    set: Arc::new(set),
                    runs,
                    bytes: u64::from(len),
                },
                Err(e) => Event::Malformed(format!("glyphs vtium cannot read: {e}")),
            },
        },
        FRAME_DONE => Event::Done,
        FRAME_FILLED | FRAME_STOPPED => match *payload.as_slice() {
            [w0, w1, w2, w3, h0, h1, h2, h3] => {
                let cell = Cell {
                    width: u32::from_le_bytes([w0, w1, w2, w3]),
                    height: u32::from_le_bytes([h0, h1, h2, h3]),
                };
                if kind == FRAME_FILLED {
                    Event::Filled(cell)
                } else {
                    Event::Stopped(cell)
                }
            }
            _ => Event::Malformed("a fill's end without its cell".to_owned()),
        },
        FRAME_OWN_CELL => match *payload.as_slice() {
            [w0, w1, w2, w3, h0, h1, h2, h3] => {
                let cell = Cell {
                    width: u32::from_le_bytes([w0, w1, w2, w3]),
                    height: u32::from_le_bytes([h0, h1, h2, h3]),
                };
                let side = 1..=vtium_font::CELL_SIDE_MAX;
                if side.contains(&cell.width) && side.contains(&cell.height) {
                    Event::OwnCell(cell)
                } else {
                    Event::Malformed("an own cell past CELL_SIDE_MAX".to_owned())
                }
            }
            _ => Event::Malformed("an own cell without its size".to_owned()),
        },
        FRAME_NO_OWN_CELL => Event::NoOwnCell(String::from_utf8_lossy(&payload).into_owned()),
        FRAME_WARNING => Event::Warning(String::from_utf8_lossy(&payload).into_owned()),
        _ => Event::Failed(String::from_utf8_lossy(&payload).into_owned()),
    })
}

/// An answer the console takes: glyphs now in the store at `cell`, drawn
/// through `face`, for `chars`, sorted: each codepoint it brought with
/// those it decomposes to, and each character of each cluster, so the
/// cells showing one, or a cluster that composes to one, redraw.
#[derive(Debug)]
pub struct Answer {
    pub cell: Cell,
    pub face: SetFace,
    pub chars: Vec<char>,
}

/// The helper's process: its pidfd, which ends it, the writer's queue and
/// the reader's events. The reader thread owns the child and reaps it.
struct Running {
    pidfd: Option<OwnedFd>,
    requests: mpsc::Sender<Vec<u8>>,
    events: mpsc::Receiver<Event>,
}

/// One bit for each Unicode codepoint.
struct Bits(Vec<u64>);

impl Bits {
    fn new() -> Bits {
        Bits(vec![0; 0x11_0000 / 64])
    }

    fn has(&self, c: u32) -> bool {
        self.0
            .get((c / 64) as usize)
            .is_some_and(|w| w & (1 << (c % 64)) != 0)
    }

    fn put(&mut self, c: u32, on: bool) {
        if let Some(w) = self.0.get_mut((c / 64) as usize) {
            if on {
                *w |= 1 << (c % 64);
            } else {
                *w &= !(1 << (c % 64));
            }
        }
    }

    /// The codepoints set, as ranges.
    fn ranges(&self) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = Vec::new();
        for (i, &w) in self.0.iter().enumerate() {
            if w == 0 {
                continue;
            }
            let base = u32::try_from(i).unwrap_or(0).saturating_mul(64);
            for b in (0..64).filter(|b| w & (1 << b) != 0) {
                let c = base.saturating_add(b);
                match out.last_mut() {
                    Some((_, last)) if last.checked_add(1) == Some(c) => *last = c,
                    _ => out.push((c, c)),
                }
            }
        }
        out
    }
}

/// Each codepoint of `runs`, `None` past `u32`.
fn codepoints(runs: &[(u32, u32)]) -> impl Iterator<Item = Option<u32>> + '_ {
    runs.iter()
        .flat_map(|&(first, n)| (0..n).map(move |i| first.checked_add(i)))
}

/// Where a cell's fill stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fill {
    /// Never started.
    None,
    Active,
    /// Ended at the store's limit or by a display gone.
    Stopped,
    Done,
}

/// What the console knows of one cell's clusters, as of its codepoints.
#[derive(Default)]
struct Clusters {
    held: BTreeSet<Text>,
    /// At most [`CLUSTERS_LACKED_MAX`].
    absent: BTreeSet<Text>,
    pending: BTreeSet<Text>,
}

/// What the console knows of one cell: the codepoints and clusters the
/// store holds, those the frit lacks, those asked for and not yet
/// answered, and its fill.
struct CellState {
    cell: Cell,
    held: Bits,
    absent: Bits,
    pending: Bits,
    clusters: Clusters,
    fill: Fill,
    /// The codepoints the fill asked for last has brought, held or not.
    brought: Bits,
    /// The fill's first codepoints, sent again with a restart.
    first: Vec<u32>,
    /// When the fill was asked for, and when its first glyphs came.
    started: Option<Instant>,
    first_glyphs: Option<Instant>,
}

impl CellState {
    /// The store holds `text`'s glyph at the cell.
    fn hold(&mut self, text: Text) {
        match *text.as_slice() {
            [c] => self.held.put(u32::from(c), true),
            _ => {
                self.clusters.held.insert(text);
            }
        }
    }

    /// The store no longer holds `text`'s glyph at the cell.
    fn forget(&mut self, text: Text) {
        match *text.as_slice() {
            [c] => self.held.put(u32::from(c), false),
            _ => {
                self.clusters.held.remove(&text);
            }
        }
    }

    fn new(cell: Cell) -> CellState {
        CellState {
            cell,
            held: Bits::new(),
            absent: Bits::new(),
            pending: Bits::new(),
            clusters: Clusters::default(),
            fill: Fill::None,
            brought: Bits::new(),
            first: Vec::new(),
            started: None,
            first_glyphs: None,
        }
    }
}

/// An ask the helper has not answered whole: its cell, the codepoints and
/// clusters it has yet to send, and when it was sent.
struct Ask {
    cell: Cell,
    want: BTreeSet<u32>,
    clusters: BTreeSet<Text>,
    sent: Instant,
}

/// The helper as the console sees it.
pub struct Helper {
    command: Command,
    /// Wakes the loop: called by the reader thread after each frame.
    wake: Arc<dyn Fn() + Send + Sync>,
    frit: FritText,
    console: Option<ConsoleFont>,
    store: Shared,
    limit: u64,
    running: Option<Running>,
    cells: Vec<CellState>,
    /// The cell of each stop sent that the helper has not yet taken: a
    /// cell's fill frames are dropped while it has one, as they belong to
    /// the fill stopped, however soon the cell is filled again.
    stopped: Vec<Cell>,
    asks: VecDeque<Ask>,
    /// A cell query is unanswered: its late answer is dropped.
    querying: bool,
    /// What the helper sent while the console waited for its own cell,
    /// for the next [`Helper::poll`].
    early: Option<Event>,
    /// When the next answer is due, while work is outstanding.
    due: Option<Instant>,
    restarts: u32,
    /// The store reached its limit: logged once.
    full: bool,
    /// The frit's warnings logged this boot, at most [`WARNINGS_MAX`].
    warned: BTreeSet<String>,
    failed: Option<String>,
    answer: Duration,
    request: Duration,
    /// When vtium started, for the line each fill logs.
    epoch: Instant,
}

impl Helper {
    /// The helper `command` starts for `frit`, handing it the kernel's
    /// `console` font, whose glyphs go into `store` up to `limit` bytes;
    /// its reader calls `wake` after each answer, in a vtium started at
    /// `epoch`.
    pub fn new(
        command: Command,
        wake: Arc<dyn Fn() + Send + Sync>,
        epoch: Instant,
        frit: FritText,
        console: Option<ConsoleFont>,
        store: Shared,
        limit: u64,
    ) -> Helper {
        Helper {
            command,
            wake,
            frit,
            console,
            store,
            limit,
            epoch,
            running: None,
            cells: Vec::new(),
            stopped: Vec::new(),
            asks: VecDeque::new(),
            querying: false,
            early: None,
            due: None,
            restarts: 0,
            full: false,
            warned: BTreeSet::new(),
            failed: None,
            answer: Duration::from_millis(ANSWER_MS),
            request: Duration::from_millis(REQUEST_MS),
        }
    }

    /// Why the helper was given up, once it has been.
    pub fn failed(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// When [`Helper::poll`] must next run: the next answer due, or the
    /// oldest ask's end, whichever comes first.
    pub fn deadline(&self) -> Option<Instant> {
        let ask = self.asks.front().map(|a| after(a.sent, self.request));
        match (self.due, ask) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Whether an ask or a fill is outstanding.
    pub fn busy(&self) -> bool {
        !self.asks.is_empty() || self.cells.iter().any(|c| c.fill == Fill::Active)
    }

    /// What the console knows of `cell`, made on first use.
    fn state(&mut self, cell: Cell) -> Option<&mut CellState> {
        if !self.cells.iter().any(|c| c.cell == cell) {
            self.cells.push(CellState::new(cell));
        }
        self.cells.iter_mut().find(|c| c.cell == cell)
    }

    /// Fills `cell`'s coverage, `first` first, while nothing is asked.
    pub fn fill(&mut self, cell: Cell, first: &[char], now: Instant) {
        if self.failed.is_some() {
            return;
        }
        let Some(state) = self.state(cell) else {
            return;
        };
        state.fill = Fill::Active;
        state.brought = Bits::new();
        state.first = first.iter().map(|&c| u32::from(c)).collect();
        state.started = Some(now);
        state.first_glyphs = None;
        let (codepoints, held) = (state.first.clone(), state.held.ranges());
        self.send(FILL, cell, (&codepoints, &[]), &held, now);
    }

    /// Asks for each character and each cluster `shown` at `cell` that the
    /// store does not hold, the frit is not known to lack and no ask has
    /// named: at most [`FIRST_MAX`] of each, at most
    /// [`CLUSTERS_WAITING_MAX`] clusters waiting at the cell, and nothing
    /// while [`ASKS_MAX`] asks wait; what is left is asked for on a later
    /// change. A cluster longer than [`vtium_font::CLUSTER_SCALARS_MAX`]
    /// is never asked for. A cluster not asked for draws unshaped.
    pub fn ask(&mut self, cell: Cell, shown: &Shown, now: Instant) {
        if self.failed.is_some() || self.asks.len() >= ASKS_MAX {
            return;
        }
        let Some(state) = self.state(cell) else {
            return;
        };
        let mut want = BTreeSet::new();
        for c in shown.chars.iter().map(|&c| u32::from(c)) {
            if want.len() >= FIRST_MAX {
                break;
            }
            if !state.held.has(c) && !state.absent.has(c) && !state.pending.has(c) {
                state.pending.put(c, true);
                want.insert(c);
            }
        }
        let longest = usize::try_from(vtium_font::CLUSTER_SCALARS_MAX).unwrap_or(usize::MAX);
        let known = &mut state.clusters;
        let room = CLUSTERS_WAITING_MAX
            .saturating_sub(known.pending.len())
            .min(FIRST_MAX);
        let mut clusters = BTreeSet::new();
        for t in shown
            .clusters
            .iter()
            .filter(|t| (2..=longest).contains(&t.len()))
        {
            if clusters.len() >= room {
                break;
            }
            if !known.held.contains(t) && !known.absent.contains(t) && !known.pending.contains(t) {
                known.pending.insert(t.clone());
                clusters.insert(t.clone());
            }
        }
        if want.is_empty() && clusters.is_empty() {
            return;
        }
        let codepoints: Vec<u32> = want.iter().copied().collect();
        let texts: Vec<Text> = clusters.iter().cloned().collect();
        self.asks.push_back(Ask {
            cell,
            want,
            clusters,
            sent: now,
        });
        self.send(ASK, cell, (&codepoints, &texts), &[], now);
    }

    /// The frit's own cell, asked of the helper and waited for at most
    /// `wait`, the one wait on the helper before a frame (the operator,
    /// 2026-10-28). The error is why there is none: the helper's refusal,
    /// a helper that cannot start or ends first, or no answer in time,
    /// whose late answer is dropped.
    pub fn own_cell(&mut self, wait: Duration, now: Instant) -> Result<Cell, String> {
        let none = Cell {
            width: 0,
            height: 0,
        };
        if self.failed.is_none() {
            self.send(OWN_CELL, none, (&[], &[]), &[], now);
        }
        if let Some(why) = &self.failed {
            return Err(why.clone());
        }
        let event = self
            .running
            .as_ref()
            .ok_or("the helper is not running")?
            .events
            .recv_timeout(wait);
        match event {
            Ok(Event::OwnCell(cell)) => Ok(cell),
            Ok(Event::NoOwnCell(why)) => Err(why.trim().to_owned()),
            Ok(other) => {
                self.early = Some(other);
                Err("the helper answered something else first".to_owned())
            }
            Err(_) => {
                self.querying = true;
                Err(format!(
                    "the helper gave no answer in {} ms",
                    wait.as_millis()
                ))
            }
        }
    }

    /// Forgets every cell but `cells`, as no display draws at them: their
    /// glyphs leave the store, and a running helper is told to stop each,
    /// whatever its fill.
    pub fn retain(&mut self, cells: &[Cell], now: Instant) {
        let gone: Vec<Cell> = self
            .cells
            .iter()
            .filter(|c| !cells.contains(&c.cell))
            .map(|c| c.cell)
            .collect();
        if self.running.is_some() {
            for cell in gone {
                self.stop_fill(cell, now);
            }
        }
        self.cells.retain(|c| cells.contains(&c.cell));
        self.store.borrow_mut().retain(cells);
    }

    /// Tells the helper to stop `cell`'s fill, and drops that cell's fill
    /// frames until the helper says it took the stop.
    fn stop_fill(&mut self, cell: Cell, now: Instant) {
        self.send(STOP, cell, (&[], &[]), &[], now);
        if self.running.is_some() {
            self.stopped.push(cell);
        }
    }

    /// Sends one request, starting the helper first when it is not
    /// running.
    fn send(
        &mut self,
        kind: u32,
        cell: Cell,
        named: (&[u32], &[Text]),
        held: &[(u32, u32)],
        now: Instant,
    ) {
        if self.running.is_none() {
            match self.start() {
                Ok(r) => self.running = Some(r),
                Err(why) => return self.fail(&why),
            }
        }
        let bytes = encode(&self.frit, self.console.as_ref(), kind, cell, named, held);
        if let Some(r) = &self.running {
            // a send fails only once the writer is gone, which the reader
            // reports as the helper closing
            let _ = r.requests.send(bytes);
        }
        if self.busy() {
            self.due.get_or_insert(after(now, self.answer));
        }
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

    /// Gives the helper up for the boot: ends it, drops what was asked
    /// and logs `why` once.
    fn fail(&mut self, why: &str) {
        self.stop();
        self.asks.clear();
        for c in &mut self.cells {
            if c.fill == Fill::Active {
                c.fill = Fill::Stopped;
            }
        }
        self.due = None;
        if self.failed.is_none() {
            log::warn!(
                "the font's glyphs are not generated ({why}): vtium draws its built-in font for them, and each cluster unshaped, a glyph a codepoint"
            );
            self.failed = Some(why.to_owned());
        }
    }

    /// Ends the helper's process, if it runs; its reader reaps it, and
    /// nothing it sent is taken.
    fn stop(&mut self) {
        self.stopped.clear();
        self.querying = false;
        if let Some(fd) = self.running.take().and_then(|r| r.pidfd) {
            let _ = pidfd_send_signal(&fd, Signal::KILL);
        }
    }

    /// Starts the helper again after `why`, its asks and fills sent again,
    /// or gives it up past [`RESTARTS_MAX`].
    fn restart(&mut self, why: &str, now: Instant) {
        self.stop();
        if self.restarts >= RESTARTS_MAX {
            return self.fail(&format!(
                "{why}, after {RESTARTS_MAX} starts again this boot"
            ));
        }
        self.restarts = self.restarts.saturating_add(1);
        log::warn!(
            "the glyph helper {why}: it is started again ({} of {RESTARTS_MAX} this boot)",
            self.restarts
        );
        self.due = None;
        let asks: Vec<(Cell, Vec<u32>, Vec<Text>)> = self
            .asks
            .iter_mut()
            .map(|a| {
                a.sent = now;
                let want = a.want.iter().copied().collect();
                (a.cell, want, a.clusters.iter().cloned().collect())
            })
            .collect();
        for (cell, want, clusters) in asks {
            self.send(ASK, cell, (&want, &clusters), &[], now);
        }
        let fills: Vec<Cell> = self
            .cells
            .iter()
            .filter(|c| c.fill == Fill::Active)
            .map(|c| c.cell)
            .collect();
        for cell in fills {
            let Some(state) = self.cells.iter_mut().find(|c| c.cell == cell) else {
                continue;
            };
            state.brought = Bits::new();
            let (first, held) = (state.first.clone(), state.held.ranges());
            self.send(FILL, cell, (&first, &[]), &held, now);
        }
    }

    /// Takes what the helper has sent since the last call; ends and starts
    /// again a helper whose answer or ask is past due.
    pub fn poll(&mut self, now: Instant) -> Vec<Answer> {
        let mut out = Vec::new();
        while self.failed.is_none() {
            let early = self.early.take();
            let Some(event) =
                early.or_else(|| self.running.as_ref().and_then(|r| r.events.try_recv().ok()))
            else {
                break;
            };
            let result = match event {
                Event::Glyphs {
                    fill,
                    set,
                    runs,
                    bytes,
                } => self
                    .glyphs(fill, &set, runs, bytes, now)
                    .map(|a| out.extend(a)),
                Event::Done => self.done(),
                Event::Filled(cell) => self.filled(cell, now),
                Event::Stopped(cell) => self.stop_taken(cell),
                // a cell query's answer after the console stopped waiting
                Event::OwnCell(_) | Event::NoOwnCell(_) if self.querying => {
                    self.querying = false;
                    Ok(())
                }
                Event::OwnCell(_) | Event::NoOwnCell(_) => {
                    Err("the helper answered a cell query not made".to_owned())
                }
                Event::Warning(why) => {
                    self.warn(why.trim());
                    Ok(())
                }
                Event::Failed(why) => Err(format!("the helper failed: {}", why.trim())),
                Event::Malformed(why) => Err(format!("the helper's answer was refused: {why}")),
                Event::Closed(status) => {
                    self.closed(status, now);
                    break;
                }
            };
            if let Err(why) = result {
                self.fail(&why);
                break;
            }
            self.due = self.busy().then(|| after(now, self.answer));
        }
        if self.failed.is_none() {
            if self
                .asks
                .front()
                .is_some_and(|a| after(a.sent, self.request) <= now)
            {
                self.restart(
                    &format!("left an ask unanswered for {} s", self.request.as_secs()),
                    now,
                );
            } else if self.due.is_some_and(|d| d <= now) {
                self.restart(
                    &format!("gave no answer for {} s", self.answer.as_secs()),
                    now,
                );
            }
        }
        out
    }

    /// Takes a frame of glyphs, or refuses it: an ask's must be at the
    /// oldest ask's cell and among the codepoints and clusters it has yet
    /// to send; a fill's at a cell filling, among those the fill has not
    /// brought, and with no cluster. The store takes the frame without the
    /// glyphs it holds, as a request may send again a glyph an earlier one
    /// brought (ADR-1200 D18), and a frame with nothing new adds nothing.
    /// An ask's past the store's limit compacts the store by the overrun,
    /// the glyphs least recently shown leaving
    /// ([`crate::deferred::Generated::make_room`]), and a frame that then
    /// carries a glyph that left adds it again; a fill's past the limit is
    /// dropped and the fill stopped.
    fn glyphs(
        &mut self,
        fill: bool,
        set: &Arc<Set>,
        runs: Vec<(u32, u32)>,
        bytes: u64,
        now: Instant,
    ) -> Result<Option<Answer>, String> {
        let not_asked = || "the helper answered what was not asked".to_owned();
        let cells = set.cells();
        let &[cell] = cells.as_slice() else {
            return Err(not_asked());
        };
        if !set
            .strikes()
            .all(|s| s.style == vtium_font::Style::default())
        {
            return Err(not_asked());
        }
        let clusters: Vec<Text> = set.clusters().map(|(_, _, key)| key).collect();
        let store = Rc::clone(&self.store);
        let without_held = || {
            store
                .borrow()
                .fresh(cell, Arc::clone(set), &runs, bytes)
                .ok_or_else(not_asked)
        };
        let fresh = if fill {
            if !clusters.is_empty() {
                return Err("the helper's fill carries a cluster".to_owned());
            }
            if self.stopped.contains(&cell) {
                return Ok(None);
            }
            let state = self
                .cells
                .iter_mut()
                .find(|c| c.cell == cell)
                .ok_or_else(not_asked)?;
            match state.fill {
                Fill::Active => {}
                Fill::Stopped | Fill::Done => return Ok(None),
                Fill::None => return Err(not_asked()),
            }
            for c in codepoints(&runs) {
                let Some(c) = c.filter(|&c| !state.brought.has(c)) else {
                    return Err("the helper's fill repeats a glyph".to_owned());
                };
                state.brought.put(c, true);
            }
            let fresh = without_held()?;
            if fresh.texts().next().is_none() {
                return Ok(None);
            }
            if self.store.borrow().bytes().saturating_add(fresh.bytes()) > self.limit {
                self.store_full(cell, now);
                return Ok(None);
            }
            fresh
        } else {
            let ask = self.asks.front_mut().ok_or_else(not_asked)?;
            if ask.cell != cell {
                return Err(not_asked());
            }
            for c in codepoints(&runs) {
                if !c.is_some_and(|c| ask.want.remove(&c)) {
                    return Err("the helper answered a glyph the ask did not name".to_owned());
                }
            }
            for t in &clusters {
                if !ask.clusters.remove(t) {
                    return Err("the helper answered a cluster the ask did not name".to_owned());
                }
            }
            let Some(state) = self.cells.iter_mut().find(|c| c.cell == cell) else {
                // no display draws at the cell any more
                return Ok(None);
            };
            // answered, held or not; what the store cannot take is asked
            // for again when the screen shows it
            for c in codepoints(&runs).flatten() {
                state.pending.put(c, false);
            }
            for t in &clusters {
                state.clusters.pending.remove(t);
            }
            let mut fresh = without_held()?;
            loop {
                if fresh.bytes() > self.limit {
                    return Ok(None);
                }
                let held = self.store.borrow().bytes();
                let overrun = held
                    .saturating_add(fresh.bytes())
                    .saturating_sub(self.limit);
                if overrun == 0 {
                    break fresh;
                }
                for (at, gone) in self.store.borrow_mut().make_room(overrun) {
                    if let Some(state) = self.cells.iter_mut().find(|c| c.cell == at) {
                        for t in gone {
                            state.forget(t);
                        }
                    }
                }
                if self.store.borrow().bytes() == held {
                    // nothing left to free
                    return Ok(None);
                }
                // a glyph the frame carries may have left the store, and
                // the frame then adds it again
                fresh = without_held()?;
            }
        };
        let added: Vec<Text> = fresh.texts().cloned().collect();
        let Some(face) = self.store.borrow_mut().add(fresh) else {
            return Ok(None);
        };
        // a cluster that decomposes from an arrived codepoint may draw it
        // now (D22 step 2), so the cells showing its base redraw too
        let mut chars: Vec<char> = added
            .iter()
            .flatten()
            .flat_map(|&c| crate::deferred::decomposition(c))
            .collect();
        chars.sort_unstable();
        chars.dedup();
        if let Some(state) = self.state(cell) {
            for t in added {
                state.hold(t);
            }
            if fill {
                state.first_glyphs.get_or_insert(now);
            }
        }
        Ok(Some(Answer { cell, face, chars }))
    }

    /// Logs the frit's warning `why`, once a boot and at most
    /// [`WARNINGS_MAX`] of them: a helper started again reads the frit
    /// again and sends its warnings again.
    fn warn(&mut self, why: &str) {
        if self.warned.len() < WARNINGS_MAX && self.warned.insert(why.to_owned()) {
            log::warn!("the frit {why}");
        }
    }

    /// The store is full while `cell` fills: the fill stops there, logged
    /// once a boot.
    fn store_full(&mut self, cell: Cell, now: Instant) {
        if let Some(state) = self.state(cell) {
            state.fill = Fill::Stopped;
        }
        self.stop_fill(cell, now);
        if !self.full {
            self.full = true;
            log::info!(
                "the glyph store is full at {} KiB: the font's other glyphs are generated as the screen shows them",
                self.limit / 1024
            );
        }
    }

    /// The oldest ask is answered whole: what it did not send, the frit
    /// lacks or cannot shape.
    fn done(&mut self) -> Result<(), String> {
        let ask = self
            .asks
            .pop_front()
            .ok_or_else(|| "the helper answered an ask not made".to_owned())?;
        if let Some(state) = self.cells.iter_mut().find(|c| c.cell == ask.cell) {
            for &c in &ask.want {
                state.pending.put(c, false);
                state.absent.put(c, true);
            }
            let known = &mut state.clusters;
            for t in ask.clusters {
                known.pending.remove(&t);
                if known.absent.len() >= CLUSTERS_LACKED_MAX {
                    known.absent.clear();
                }
                known.absent.insert(t);
            }
        }
        Ok(())
    }

    /// The helper took the oldest stop at `cell`: the frames after it are
    /// the next fill's. One with no stop sent was not asked.
    fn stop_taken(&mut self, cell: Cell) -> Result<(), String> {
        let i = self
            .stopped
            .iter()
            .position(|&c| c == cell)
            .ok_or_else(|| "the helper took a stop not sent".to_owned())?;
        self.stopped.remove(i);
        Ok(())
    }

    /// A cell's fill is done: logged with its timings, or refused when the
    /// cell holds no glyph. The end of a fill stopped is dropped.
    fn filled(&mut self, cell: Cell, now: Instant) -> Result<(), String> {
        if self.stopped.contains(&cell) {
            return Ok(());
        }
        let epoch = self.epoch;
        let state = self
            .cells
            .iter_mut()
            .find(|c| c.cell == cell)
            .ok_or_else(|| "the helper ended a fill not asked for".to_owned())?;
        if state.fill != Fill::Active {
            return Ok(());
        }
        if state.held.ranges().is_empty() {
            return Err(format!(
                "the helper answered no glyph at {}x{}",
                cell.width, cell.height
            ));
        }
        state.fill = Fill::Done;
        let ms = |d: Duration| d.as_millis();
        let started = state.started.unwrap_or(now);
        let since = |t: Instant| ms(t.saturating_duration_since(started));
        log::info!(
            "the font's glyphs at {}x{} are generated: asked {} ms after vtium started, the first in {} ms, the last in {} ms",
            cell.width,
            cell.height,
            ms(started.saturating_duration_since(epoch)),
            state.first_glyphs.map_or(0, since),
            since(now)
        );
        Ok(())
    }

    /// The helper closed its end and was reaped. With nothing outstanding,
    /// an exit 0 is its idle exit and the next need starts another; any
    /// other end, or an exit with work outstanding, counts as a start
    /// again, made now when work is outstanding.
    fn closed(&mut self, status: Option<ExitStatus>, now: Instant) {
        self.running = None;
        self.stopped.clear();
        self.querying = false;
        let clean = status.is_some_and(|s| s.success());
        let why = match status.and_then(|s| s.signal()) {
            Some(SIGXCPU | 9) => "reached its CPU limit".to_owned(),
            Some(sig) => format!("was ended by signal {sig}"),
            None => match status {
                Some(s) => format!("exited ({s})"),
                None => "exited".to_owned(),
            },
        };
        if self.busy() {
            return self.restart(&why, now);
        }
        self.due = None;
        if !clean {
            if self.restarts >= RESTARTS_MAX {
                return self.fail(&format!(
                    "the helper {why}, after {RESTARTS_MAX} starts again this boot"
                ));
            }
            self.restarts = self.restarts.saturating_add(1);
            log::warn!(
                "the glyph helper {why}: the next need starts it again ({} of {RESTARTS_MAX} this boot)",
                self.restarts
            );
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
    use vtium_font::set::{PixelFormat, SetBuilder, StrikeBuilder};
    use vtium_font::{Face, Style, Width};

    const CELL: Cell = Cell {
        width: 8,
        height: 16,
    };

    /// A set of `chars`, each solid, at `cell`, its chunks raw as the
    /// helper writes them.
    fn set(cell: Cell, chars: &[char]) -> Vec<u8> {
        let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default()).compress(false);
        let g = s.glyph(
            Width::Narrow,
            vec![255; (cell.width * cell.height) as usize],
        );
        for &c in chars {
            s.map(c, g);
        }
        SetBuilder::new("t").strike(s).write().unwrap()
    }

    fn frame_bytes(kind: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = kind.to_le_bytes().to_vec();
        out.extend_from_slice(&len32(payload.len()).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn filled(cell: Cell) -> Vec<u8> {
        let mut c = cell.width.to_le_bytes().to_vec();
        c.extend_from_slice(&cell.height.to_le_bytes());
        frame_bytes(FRAME_FILLED, &c)
    }

    /// The helper's word that it took a stop at `cell`, as it answers
    /// each.
    fn stopped(cell: Cell) -> Vec<u8> {
        let mut c = cell.width.to_le_bytes().to_vec();
        c.extend_from_slice(&cell.height.to_le_bytes());
        frame_bytes(FRAME_STOPPED, &c)
    }

    /// A helper whose answers are `script`, run by the shell after it
    /// writes `fixture` where the script finds it as `$1`.
    struct StandIn {
        helper: Helper,
        woke: mpsc::Receiver<()>,
        dir: PathBuf,
    }

    /// Reads the request, then answers with the fixture and waits.
    const ANSWERS: &str = "cat >/dev/null & cat \"$1\"; wait";

    impl StandIn {
        fn new(name: &str, script: &str, fixture: &[u8]) -> StandIn {
            StandIn::limited(name, script, fixture, 64 << 20)
        }

        fn limited(name: &str, script: &str, fixture: &[u8], limit: u64) -> StandIn {
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
                helper: Helper::new(
                    command,
                    wake,
                    t0(),
                    frit("source=builtin:unscii-8\n"),
                    None,
                    Shared::default(),
                    limit,
                ),
                woke,
                dir,
            }
        }

        /// Polls as the loop does, each time the reader wakes it, until the
        /// helper has nothing outstanding, has gone, or is given up; the
        /// answers taken.
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

        /// Polls until the helper's process is gone.
        fn until_gone(&mut self, now: Instant) {
            while self.helper.running.is_some()
                && self.woke.recv_timeout(Duration::from_secs(10)).is_ok()
            {
                let _ = self.helper.poll(now);
            }
        }

        fn held(&self, c: char) -> bool {
            self.helper
                .cells
                .iter()
                .any(|s| s.cell == CELL && s.held.has(u32::from(c)))
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

    /// The screen showing `chars`, no cluster.
    fn shown(chars: &[char]) -> Shown {
        Shown {
            chars: chars.to_vec(),
            clusters: Vec::new(),
        }
    }

    /// The frit `t` of `text`.
    fn frit(text: &str) -> FritText {
        FritText {
            name: "t".to_owned(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_request_is_the_helpers_format() {
        let bytes = encode(
            &frit("x=y\n"),
            None,
            ASK,
            CELL,
            (&[0x41], &[vec!['e', '\u{301}']]),
            &[(0x42, 0x43)],
        );
        let mut want = b"VTVSREQ3".to_vec();
        want.extend_from_slice(&ASK.to_le_bytes());
        want.extend_from_slice(&1u32.to_le_bytes());
        want.extend_from_slice(b"t");
        want.extend_from_slice(&4u32.to_le_bytes());
        want.extend_from_slice(b"x=y\n");
        // the cell, the reserved capabilities, the codepoint, the cluster
        // of two, the held range, no console font
        for v in [8u32, 16, 0, 1, 0x41, 1, 2, 0x65, 0x301, 1, 0x42, 0x43, 0] {
            want.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(bytes, want);
    }

    #[test]
    fn a_frit_is_read_with_the_name_of_its_file() {
        let dir = std::env::temp_dir().join(format!("vtium-helper-read-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ter-v16n.vtfrit");
        std::fs::write(&path, "source=kbd:ter-v16n\n").unwrap();
        let read = FritText::read(&path);
        let gone = FritText::read(&dir.join("gone.vtfrit"));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            read,
            Ok(FritText {
                name: "ter-v16n".to_owned(),
                text: "source=kbd:ter-v16n\n".to_owned(),
            })
        );
        assert!(gone.unwrap_err().contains("gone.vtfrit"));
    }

    #[test]
    fn an_asks_glyphs_are_taken_and_then_served_from_the_store() {
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new("ask", ANSWERS, &fixture);
        let now = t0();
        s.helper.ask(CELL, &shown(&['A', 'B']), now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].cell, CELL);
        assert!(answers[0].face.has_glyph('A'));
        assert!(s.held('A'));
        // A is held and B is known lacking: nothing more is asked
        s.helper.ask(CELL, &shown(&['A', 'B']), now);
        assert!(!s.helper.busy());
    }

    const ACUTE: [char; 2] = ['e', '\u{301}'];
    const GRAVE: [char; 2] = ['e', '\u{300}'];

    /// A set at `cell` mapping each of `clusters` to a solid glyph.
    fn cluster_set(cell: Cell, clusters: &[&[char]]) -> Vec<u8> {
        let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default()).compress(false);
        let g = s.glyph(
            Width::Narrow,
            vec![255; (cell.width * cell.height) as usize],
        );
        for &k in clusters {
            s.cluster(k, g);
        }
        SetBuilder::new("t").strike(s).write().unwrap()
    }

    /// The screen showing `clusters` whole, and each of their characters.
    fn clustered(clusters: &[&[char]]) -> Shown {
        let mut chars: Vec<char> = clusters.iter().flat_map(|k| k.iter().copied()).collect();
        chars.sort_unstable();
        chars.dedup();
        Shown {
            chars,
            clusters: clusters.iter().map(|k| k.to_vec()).collect(),
        }
    }

    // ADR-1200 D20, D21: a cluster is asked for by its whole text, its
    // glyph held by that text, and one the helper did not shape is known
    // lacking: neither is asked for again
    #[test]
    fn a_cluster_is_asked_for_whole_and_held_by_its_text() {
        let mut fixture = frame_bytes(FRAME_GLYPHS, &cluster_set(CELL, &[&ACUTE]));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new("cluster", ANSWERS, &fixture);
        let now = t0();
        let only_clusters = Shown {
            chars: Vec::new(),
            clusters: vec![ACUTE.to_vec(), GRAVE.to_vec()],
        };
        s.helper.ask(CELL, &only_clusters, now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 1);
        // the cells showing e or U+0301 redraw
        assert_eq!(answers[0].chars, ['e', '\u{301}']);
        let state = s.helper.cells.iter().find(|c| c.cell == CELL).unwrap();
        assert!(state.clusters.held.contains(ACUTE.as_slice()));
        assert!(state.clusters.absent.contains(GRAVE.as_slice()));
        assert!(!state.held.has(u32::from('e')));
        assert!(
            s.helper
                .store
                .borrow()
                .fresh(
                    CELL,
                    Arc::new(Set::from_bytes(cluster_set(CELL, &[&ACUTE])).unwrap()),
                    &[],
                    1
                )
                .unwrap()
                .texts()
                .next()
                .is_none(),
            "the store holds it by its text"
        );
        s.helper.ask(CELL, &only_clusters, now);
        assert!(!s.helper.busy());
    }

    // ADR-1200 D22 step 2: an arrived é redraws the cells whose cluster
    // holds e or U+0301, as e U+0301 now draws é's glyph
    #[test]
    fn an_arrived_codepoint_redraws_the_clusters_it_composes_from() {
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['\u{e9}', 'A']));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new("decomposed", ANSWERS, &fixture);
        let now = t0();
        s.helper.ask(CELL, &shown(&['\u{e9}', 'A']), now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].chars, ['A', 'e', '\u{e9}', '\u{301}']);
    }

    // ADR-1200 D15: a cluster the ask did not name, or one in a fill, is
    // what was not asked, and gives the helper up
    #[test]
    fn a_cluster_not_asked_for_gives_the_helper_up() {
        let mut s = StandIn::new(
            "cluster-unasked",
            ANSWERS,
            &frame_bytes(FRAME_GLYPHS, &cluster_set(CELL, &[&GRAVE])),
        );
        let now = t0();
        s.helper.ask(CELL, &clustered(&[&ACUTE]), now);
        let _ = s.run(now);
        let why = s.helper.failed().unwrap();
        assert!(why.contains("cluster the ask did not name"), "{why}");
        let (why, _) = given_up(
            "cluster-fill",
            ANSWERS,
            &frame_bytes(FRAME_FILL, &cluster_set(CELL, &[&ACUTE])),
            true,
        );
        assert!(why.contains("fill carries a cluster"), "{why}");
    }

    // the frit's warnings are logged once each, at most WARNINGS_MAX a
    // boot, and none gives the helper up
    #[test]
    fn a_frits_warnings_are_kept_to_their_bound() {
        let mut fixture = Vec::new();
        for i in 0..WARNINGS_MAX + 4 {
            fixture.extend(frame_bytes(
                FRAME_WARNING,
                format!("t:{i}: a key that does nothing").as_bytes(),
            ));
        }
        fixture.extend(frame_bytes(FRAME_WARNING, b"t:0: a key that does nothing"));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new("warnings", ANSWERS, &fixture);
        let now = t0();
        s.helper.ask(CELL, &shown(&['A']), now);
        let _ = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(s.helper.warned.len(), WARNINGS_MAX);
        assert!(s.helper.warned.contains("t:0: a key that does nothing"));
        assert!(
            !s.helper
                .warned
                .contains(&format!("t:{WARNINGS_MAX}: a key that does nothing"))
        );
    }

    // a cluster past CLUSTER_SCALARS_MAX is never asked for, as the helper
    // refuses it; nor is a lone codepoint taken for a cluster
    #[test]
    fn a_cluster_past_its_bound_is_not_asked_for() {
        let mut helper = Helper::new(
            Command {
                program: PathBuf::from("/nonexistent"),
                args: Vec::new(),
                account: None,
            },
            Arc::new(|| {}),
            t0(),
            frit("source=builtin:unscii-8\n"),
            None,
            Shared::default(),
            1 << 20,
        );
        let long = vec!['e'; vtium_font::CLUSTER_SCALARS_MAX as usize + 1];
        let shown = Shown {
            chars: Vec::new(),
            clusters: vec![long, vec!['e']],
        };
        helper.ask(CELL, &shown, t0());
        assert!(helper.asks.is_empty());
        assert_eq!(helper.failed(), None);
    }

    // a program that writes new clusters faster than the helper answers
    // grows nothing without bound: at most ASKS_MAX asks wait, and at most
    // CLUSTERS_WAITING_MAX clusters a cell; what is left is asked for on a
    // later change
    #[test]
    fn asks_and_the_clusters_they_wait_on_are_bounded() {
        let mut s = StandIn::new("flood-asks", "cat >/dev/null", b"");
        let now = t0();
        let cluster = |i: u32, j: u32| {
            vec![
                char::from_u32(0x4e00 + i).unwrap(),
                char::from_u32(0x300 + j).unwrap(),
            ]
        };
        for i in 0..1000 {
            let shown = Shown {
                chars: Vec::new(),
                clusters: (0..10).map(|j| cluster(i, j)).collect(),
            };
            s.helper.ask(CELL, &shown, now);
            assert!(s.helper.asks.len() <= ASKS_MAX);
        }
        assert_eq!(s.helper.asks.len(), ASKS_MAX);
        let waiting = |h: &Helper, cell: Cell| {
            h.cells
                .iter()
                .find(|c| c.cell == cell)
                .map_or(0, |c| c.clusters.pending.len())
        };
        assert_eq!(waiting(&s.helper, CELL), ASKS_MAX * 10);
        // one ask of more clusters than may wait at a cell
        let other = Cell {
            width: 16,
            height: 32,
        };
        let mut t = StandIn::new("flood-one", "cat >/dev/null", b"");
        let shown = Shown {
            chars: Vec::new(),
            clusters: (0..5000).map(|i| cluster(i / 10, i % 10)).collect(),
        };
        t.helper.ask(other, &shown, now);
        assert_eq!(waiting(&t.helper, other), CLUSTERS_WAITING_MAX);
        t.helper.ask(other, &shown, now);
        assert_eq!(t.helper.asks.len(), 1, "nothing more fits");
    }

    // a codepoint past an ask's FIRST_MAX is not asked for, so its answer
    // does not take it for one the frit lacks; a later change asks for it
    #[test]
    fn a_codepoint_past_an_asks_bound_is_not_taken_for_lacking() {
        let mut s = StandIn::new("first-max", ANSWERS, &frame_bytes(FRAME_DONE, &[]));
        let now = t0();
        let n = u32::try_from(FIRST_MAX).unwrap() + 5;
        let chars: Vec<char> = (0..n)
            .map(|i| char::from_u32(0x4e00 + i).unwrap())
            .collect();
        s.helper.ask(CELL, &shown(&chars), now);
        let _ = s.run(now);
        assert_eq!(s.helper.failed(), None);
        let state = s.helper.cells.iter().find(|c| c.cell == CELL).unwrap();
        let last = u32::from(chars[FIRST_MAX - 1]);
        assert!(state.absent.has(last));
        for &c in &chars[FIRST_MAX..] {
            let c = u32::from(c);
            assert!(!state.absent.has(c) && !state.pending.has(c), "{c:x}");
        }
    }

    // the record of clusters the frit lacks is bounded: past it, it is
    // emptied, and a cluster shown again is asked for again
    #[test]
    fn the_record_of_clusters_lacked_is_bounded() {
        let mut s = StandIn::new("lacked", ANSWERS, &frame_bytes(FRAME_DONE, &[]));
        let now = t0();
        {
            let state = s.helper.state(CELL).unwrap();
            for i in 0..u32::try_from(CLUSTERS_LACKED_MAX).unwrap() {
                let c = char::from_u32(0x300 + i % 0x70).unwrap();
                let d = char::from_u32(0x4e00 + i / 0x70).unwrap();
                state.clusters.absent.insert(vec![d, c]);
            }
            assert_eq!(state.clusters.absent.len(), CLUSTERS_LACKED_MAX);
        }
        s.helper.ask(CELL, &clustered(&[&ACUTE]), now);
        let _ = s.run(now);
        assert_eq!(s.helper.failed(), None);
        let state = s.helper.cells.iter().find(|c| c.cell == CELL).unwrap();
        assert_eq!(
            state.clusters.absent.iter().collect::<Vec<_>>(),
            [&ACUTE.to_vec()]
        );
    }

    #[test]
    fn a_fill_fills_until_its_end() {
        let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
        fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['B'])));
        fixture.extend(filled(CELL));
        let mut s = StandIn::new("fill", ANSWERS, &fixture);
        let now = t0();
        s.helper.fill(CELL, &[], now);
        assert_eq!(s.run(now).len(), 2);
        assert_eq!(s.helper.failed(), None);
        assert!(s.held('A') && s.held('B'));
        assert!(!s.helper.busy());
    }

    #[test]
    fn a_helper_that_exits_at_its_idle_interval_is_started_again() {
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new("idle", "head -c 1 >/dev/null; cat \"$1\"", &fixture);
        let now = t0();
        s.helper.ask(CELL, &shown(&['A']), now);
        assert_eq!(s.run(now).len(), 1);
        s.until_gone(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(s.helper.restarts, 0);
        // the cell asked for again, as after its glyphs were dropped; with
        // no helper running, the drop starts none
        s.helper.retain(&[], now);
        assert!(s.helper.running.is_none());
        s.helper.ask(CELL, &shown(&['A']), now);
        assert_eq!(s.run(now).len(), 1);
        assert_eq!(s.helper.failed(), None);
    }

    #[test]
    fn a_helper_that_closes_before_it_exits_is_no_failure() {
        // its output closes, then it lingers before exiting 0: reaped, not
        // ended
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new(
            "linger",
            "head -c 1 >/dev/null; cat \"$1\"; exec 1>&-; sleep 0.3; exit 0",
            &fixture,
        );
        let now = t0();
        s.helper.ask(CELL, &shown(&['A']), now);
        assert_eq!(s.run(now).len(), 1);
        s.until_gone(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(s.helper.restarts, 0);
    }

    fn given_up(name: &str, script: &str, fixture: &[u8], fill: bool) -> (String, u32) {
        let mut s = StandIn::new(name, script, fixture);
        let now = t0();
        if fill {
            s.helper.fill(CELL, &[], now);
        } else {
            s.helper.ask(CELL, &shown(&['A']), now);
        }
        let _ = s.run(now);
        let why = s.helper.failed().unwrap().to_owned();
        // given up: nothing more is asked
        s.helper.ask(CELL, &shown(&['Z']), now);
        assert!(!s.helper.busy());
        (why, s.helper.restarts)
    }

    #[test]
    fn a_helper_that_crashes_is_started_again_a_bounded_number_of_times() {
        let (why, restarts) = given_up("dies", "kill -SEGV $$", b"", false);
        assert!(why.contains("signal 11"), "{why}");
        assert_eq!(restarts, RESTARTS_MAX);
        let (why, restarts) = given_up("exits", "cat >/dev/null & exit 3", b"", false);
        assert!(why.contains("exit status: 3"), "{why}");
        assert_eq!(restarts, RESTARTS_MAX);
    }

    #[test]
    fn garbage_gives_the_helper_up_at_once() {
        for (name, fixture, fill, says) in [
            (
                "garbage",
                b"VTVOUTL1 not a frame".to_vec(),
                false,
                "refused",
            ),
            (
                "badset",
                frame_bytes(FRAME_GLYPHS, b"VTGLASS\0junk"),
                false,
                "cannot read",
            ),
            (
                "cell",
                frame_bytes(
                    FRAME_GLYPHS,
                    &set(
                        Cell {
                            width: 8,
                            height: 8,
                        },
                        &['A'],
                    ),
                ),
                false,
                "not asked",
            ),
            (
                "unasked",
                frame_bytes(FRAME_GLYPHS, &set(CELL, &['A', 'B'])),
                false,
                "did not name",
            ),
            (
                "nofill",
                frame_bytes(FRAME_FILL, &set(CELL, &['A'])),
                false,
                "not asked",
            ),
            (
                "fillcell",
                frame_bytes(
                    FRAME_FILL,
                    &set(
                        Cell {
                            width: 8,
                            height: 8,
                        },
                        &['A'],
                    ),
                ),
                true,
                "not asked",
            ),
            (
                "failed",
                frame_bytes(FRAME_FAILED, b"no source of the frit is here"),
                false,
                "no source of the frit is here",
            ),
        ] {
            let (why, restarts) = given_up(name, ANSWERS, &fixture, fill);
            assert!(why.contains(says), "{name}: {why}");
            assert_eq!(restarts, 0, "{name}");
        }
    }

    #[test]
    fn a_fill_that_repeats_a_glyph_is_refused() {
        // the same set over and over, as a stand-in that floods: given up
        // at the second, however long the flood
        let a = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
        let flood: Vec<u8> = a.iter().copied().cycle().take(a.len() * 20_000).collect();
        let (why, _) = given_up("flood", ANSWERS, &flood, true);
        assert!(why.contains("repeats"), "{why}");
        // an ask's glyph sent twice
        let mut twice = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
        twice.extend(frame_bytes(FRAME_GLYPHS, &set(CELL, &['A'])));
        let (why, _) = given_up("twice", ANSWERS, &twice, false);
        assert!(why.contains("did not name"), "{why}");
    }

    // D18: an ask during a fill and the fill are two requests, and each
    // sends the glyph the ask named; the console takes the ask's and drops
    // the fill's, which the store already holds
    #[test]
    fn a_fill_frame_of_glyphs_an_ask_brought_is_dropped() {
        let (a, b) = (set(CELL, &['A']), set(CELL, &['B']));
        let mut fixture = frame_bytes(FRAME_GLYPHS, &a);
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        fixture.extend(frame_bytes(FRAME_FILL, &a));
        fixture.extend(frame_bytes(FRAME_FILL, &b));
        fixture.extend(filled(CELL));
        let mut s = StandIn::new("fillafterask", ANSWERS, &fixture);
        let now = t0();
        s.helper.fill(CELL, &[], now);
        s.helper.ask(CELL, &shown(&['A']), now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 2);
        assert!(s.held('A') && s.held('B'));
        assert_eq!(s.helper.store.borrow().bytes(), (a.len() + b.len()) as u64);
        assert!(!s.helper.busy());
    }

    // D18, D19: the fill's later batch sends again the glyph an ask during
    // the fill brought; the store takes the batch without it, holds it
    // once and counts the batch as written again
    #[test]
    fn an_asked_glyph_a_later_fill_batch_sends_again_is_held_once() {
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['A', 'B'])));
        fixture.extend(filled(CELL));
        let mut s = StandIn::new("askinfill", ANSWERS, &fixture);
        let now = t0();
        s.helper.fill(CELL, &[], now);
        s.helper.ask(CELL, &shown(&['A']), now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 2);
        assert!(answers[0].face.has_glyph('A'));
        assert!(answers[1].face.has_glyph('B') && !answers[1].face.has_glyph('A'));
        assert!(s.held('A') && s.held('B'));
        let (a, b) = (set(CELL, &['A']).len(), set(CELL, &['B']).len());
        assert_eq!(s.helper.store.borrow().bytes(), (a + b) as u64);
        assert!(!s.helper.busy());
    }

    // D18: a fill batch on its way when an ask is sent brings the asked
    // glyph first; the ask's answer adds nothing, and the ask ends
    #[test]
    fn an_asks_glyph_a_fill_batch_brought_first_adds_nothing() {
        let ab = set(CELL, &['A', 'B']);
        let mut fixture = frame_bytes(FRAME_FILL, &ab);
        fixture.extend(frame_bytes(FRAME_GLYPHS, &set(CELL, &['A'])));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        fixture.extend(filled(CELL));
        let mut s = StandIn::new("fillbeforeask", ANSWERS, &fixture);
        let now = t0();
        s.helper.fill(CELL, &[], now);
        s.helper.ask(CELL, &shown(&['A']), now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 1);
        assert!(s.held('A') && s.held('B'));
        assert_eq!(s.helper.store.borrow().bytes(), ab.len() as u64);
        assert!(!s.helper.busy());
        // answered, so A is neither pending nor lacking
        let state = s.helper.cells.iter().find(|c| c.cell == CELL).unwrap();
        assert!(!state.pending.has(0x41) && !state.absent.has(0x41));
    }

    // D18, D19: the fill's limit weighs a batch as the store writes it
    // again, so a batch whose new glyphs fit is taken
    #[test]
    fn a_fill_batch_fits_the_store_by_its_new_glyphs() {
        let (a, c) = (set(CELL, &['A']), set(CELL, &['C']));
        let limit = (a.len() + c.len()) as u64;
        let mut fixture = frame_bytes(FRAME_GLYPHS, &a);
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['A', 'C'])));
        fixture.extend(filled(CELL));
        let mut s = StandIn::limited("fillfits", ANSWERS, &fixture, limit);
        let now = t0();
        s.helper.fill(CELL, &[], now);
        s.helper.ask(CELL, &shown(&['A']), now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 2);
        assert!(s.held('A') && s.held('C'));
        assert_eq!(s.helper.store.borrow().bytes(), limit);
        assert!(!s.helper.full);
    }

    // D18, D19: an ask's frame carries B, which the store holds and the
    // ask's compaction drops; the store keeps B from the frame
    #[test]
    fn a_held_glyph_whose_set_an_ask_evicts_is_kept_from_the_asks_frame() {
        let bc = set(CELL, &['B', 'C']);
        let limit = bc.len() as u64;
        let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A', 'B']));
        fixture.extend(frame_bytes(FRAME_GLYPHS, &bc));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        fixture.extend(filled(CELL));
        let mut s = StandIn::limited("evictheld", ANSWERS, &fixture, limit);
        let now = t0();
        s.helper.fill(CELL, &[], now);
        s.helper.ask(CELL, &shown(&['B', 'C']), now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 2);
        assert!(answers[1].face.has_glyph('B') && answers[1].face.has_glyph('C'));
        assert!(!s.held('A') && s.held('B') && s.held('C'));
        assert_eq!(s.helper.store.borrow().bytes(), limit);
    }

    /// A set of `chars` at `cell`, each glyph its own shade, its chunks
    /// raw as the helper writes them.
    fn shades(cell: Cell, chars: &[char]) -> Vec<u8> {
        let mut s = StrikeBuilder::new(PixelFormat::R8, cell, Style::default()).compress(false);
        for (seed, &c) in (1u32..).zip(chars) {
            let g = s.glyph(
                Width::Narrow,
                (0..cell.width * cell.height)
                    .map(|i| u8::try_from((i * 7 + seed * 13) % 254 + 1).unwrap())
                    .collect(),
            );
            s.map(c, g);
        }
        SetBuilder::new("t").strike(s).write().unwrap()
    }

    // #145, ADR-1200 D19: an ask at a full store compacts the set that
    // holds both glyphs the screen shows and the least recently shown:
    // the shown stay, only enough of the others leave to cover the
    // overrun, and the store counts the set as written again
    #[test]
    fn an_ask_at_a_full_store_compacts_the_set_it_takes_room_from() {
        let abcd = shades(CELL, &['A', 'B', 'C', 'D']);
        let e = shades(CELL, &['E']);
        let limit = (abcd.len() + e.len() - 1) as u64;
        let script = format!(
            "head -c {} >/dev/null; cat \"$1\"; head -c {} >/dev/null; cat \"$1.ask\"; cat >/dev/null",
            request_bytes(FILL),
            request_bytes(ASK) + 4
        );
        let mut fill = frame_bytes(FRAME_FILL, &abcd);
        fill.extend(filled(CELL));
        let mut s = StandIn::limited("askcompacts", &script, &fill, limit);
        let mut ask = frame_bytes(FRAME_GLYPHS, &e);
        ask.extend(frame_bytes(FRAME_DONE, &[]));
        std::fs::write(s.dir.join("answer.ask"), ask).unwrap();
        let now = t0();
        s.helper.fill(CELL, &[], now);
        assert_eq!(s.run(now).len(), 1);
        assert_eq!(s.helper.store.borrow().bytes(), abcd.len() as u64);
        // the screen shows A and B; C and D are the least recently shown
        s.helper.store.borrow_mut().touch(CELL, &shown(&['A', 'B']));
        s.helper.ask(CELL, &shown(&['E']), now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 1);
        assert!(answers[0].face.has_glyph('E'));
        // one byte over: C alone covers it
        assert!(s.held('A') && s.held('B') && s.held('D') && s.held('E'));
        assert!(!s.held('C'));
        let rebuilt = shades(CELL, &['A', 'B', 'D']).len();
        assert_eq!(s.helper.store.borrow().bytes(), (rebuilt + e.len()) as u64);
        assert!(s.helper.store.borrow().bytes() <= limit);
        // C, evicted, is asked for again when the screen shows it
        s.helper.ask(CELL, &shown(&['C']), now);
        assert!(s.helper.busy());
    }

    // a zoom or a hotplug during a fill: the frames already on their way
    // for the cell no display draws are dropped, and the helper stays
    #[test]
    fn a_fill_frame_for_a_cell_no_display_draws_is_dropped() {
        let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
        fixture.extend(filled(CELL));
        fixture.extend(stopped(CELL));
        let mut s = StandIn::new("late", ANSWERS, &fixture);
        let now = t0();
        s.helper.fill(CELL, &[], now);
        s.helper.retain(&[], now);
        // the three frames read, then taken
        for _ in 0..3 {
            s.woke.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        assert!(s.helper.poll(now).is_empty());
        assert_eq!(s.helper.failed(), None);
        assert!(!s.held('A'));
        assert!(s.helper.stopped.is_empty());
        assert!(!s.helper.busy());
    }

    // D15, D17: a zoom away and back before the old fill's frames are
    // taken; the frames before the helper took the stop are the old
    // fill's and dropped, the new fill's are taken, and the helper stays
    #[test]
    fn a_cell_stopped_and_filled_again_takes_only_the_new_fill() {
        let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
        fixture.extend(stopped(CELL));
        fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['A'])));
        fixture.extend(filled(CELL));
        let mut s = StandIn::new("zoomback", ANSWERS, &fixture);
        let now = t0();
        s.helper.fill(CELL, &[], now);
        s.helper.retain(&[], now);
        s.helper.fill(CELL, &[], now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 1);
        assert!(s.held('A'));
        assert!(s.helper.stopped.is_empty());
        assert!(!s.helper.busy());
    }

    // the old fill's end, sent before the helper took the stop, leaves the
    // new fill running, and the new fill's glyphs are taken
    #[test]
    fn a_stopped_fills_end_does_not_end_the_next_fill() {
        let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
        fixture.extend(filled(CELL));
        fixture.extend(stopped(CELL));
        fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['A'])));
        fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['B'])));
        fixture.extend(filled(CELL));
        let mut s = StandIn::new("zoombackend", ANSWERS, &fixture);
        let now = t0();
        s.helper.fill(CELL, &[], now);
        s.helper.retain(&[], now);
        s.helper.fill(CELL, &[], now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        assert_eq!(answers.len(), 2);
        assert!(s.held('A') && s.held('B'));
        assert!(!s.helper.busy());
    }

    // D15: the word that a stop was taken, with no stop sent, was not
    // asked
    #[test]
    fn a_stop_taken_that_was_not_sent_gives_the_helper_up() {
        let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
        fixture.extend(stopped(CELL));
        let (why, restarts) = given_up("stopunsent", ANSWERS, &fixture, true);
        assert!(why.contains("stop not sent"), "{why}");
        assert_eq!(restarts, 0);
    }

    /// The bytes of the stand-ins' request of `kind` at the cell, naming
    /// nothing.
    fn request_bytes(kind: u32) -> usize {
        encode(
            &frit("source=builtin:unscii-8\n"),
            None,
            kind,
            CELL,
            (&[], &[]),
            &[],
        )
        .len()
    }

    // a zoom away from a cell whose fill stopped at the store's limit: the
    // frames already on their way for it are dropped, and the helper stays
    #[test]
    fn a_late_frame_for_a_stopped_cell_no_display_draws_is_dropped() {
        let a = set(CELL, &['A']);
        let script = format!(
            "head -c {} >/dev/null; cat \"$1\"; head -c {stop} >/dev/null; cat \"$1.late\"; head -c {stop} >/dev/null; cat \"$1.stop\"; cat >/dev/null",
            request_bytes(FILL),
            stop = request_bytes(STOP)
        );
        let mut s = StandIn::limited("stoppedlate", &script, &frame_bytes(FRAME_FILL, &a), 1);
        // B, sent before the helper took the store's stop, then that stop
        // taken; then the second stop taken
        let mut late = frame_bytes(FRAME_FILL, &set(CELL, &['B']));
        late.extend(stopped(CELL));
        std::fs::write(s.dir.join("answer.late"), late).unwrap();
        std::fs::write(s.dir.join("answer.stop"), stopped(CELL)).unwrap();
        let now = t0();
        s.helper.fill(CELL, &[], now);
        // A past the store's limit: dropped, and the fill stopped
        s.woke.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(s.helper.poll(now).is_empty());
        assert_eq!(s.helper.failed(), None);
        s.helper.retain(&[], now);
        // B, then the two stops taken
        for _ in 0..3 {
            s.woke.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        assert!(s.helper.poll(now).is_empty());
        assert_eq!(s.helper.failed(), None);
        assert!(s.helper.stopped.is_empty());
    }

    // a cell dropped whose fill is done is stopped too
    #[test]
    fn a_cell_dropped_after_its_fill_is_stopped() {
        let mut fixture = frame_bytes(FRAME_FILL, &set(CELL, &['A']));
        fixture.extend(filled(CELL));
        let script = format!(
            "head -c {} >/dev/null; cat \"$1\"; head -c {} >\"$1.stop\"; cat \"$1.taken\"",
            request_bytes(FILL),
            request_bytes(STOP)
        );
        let mut s = StandIn::new("donestop", &script, &fixture);
        std::fs::write(s.dir.join("answer.taken"), stopped(CELL)).unwrap();
        let now = t0();
        s.helper.fill(CELL, &[], now);
        assert_eq!(s.run(now).len(), 1);
        assert!(!s.helper.busy());
        s.helper.retain(&[], now);
        s.until_gone(now);
        assert_eq!(s.helper.failed(), None);
        let stop = encode(&s.helper.frit, None, STOP, CELL, (&[], &[]), &[]);
        assert_eq!(std::fs::read(s.dir.join("answer.stop")).unwrap(), stop);
    }

    #[test]
    fn a_fill_with_no_glyph_is_a_failure() {
        let (why, _) = given_up("none", ANSWERS, &filled(CELL), true);
        assert!(why.contains("no glyph"), "{why}");
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
        let (why, _) = given_up("huge", ANSWERS, &huge, false);
        assert!(why.contains("past its kind's"), "{why}");
    }

    #[test]
    fn a_fill_stops_at_the_stores_limit_and_an_ask_past_it_evicts() {
        let a = set(CELL, &['A']);
        let limit = (a.len() as u64) * 3 / 2;
        let mut fixture = frame_bytes(FRAME_FILL, &a);
        fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['B'])));
        fixture.extend(frame_bytes(FRAME_FILL, &set(CELL, &['C'])));
        fixture.extend(stopped(CELL));
        fixture.extend(frame_bytes(FRAME_GLYPHS, &set(CELL, &['D'])));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::limited("limit", ANSWERS, &fixture, limit);
        let now = t0();
        s.helper.fill(CELL, &[], now);
        s.helper.ask(CELL, &shown(&['D']), now);
        let answers = s.run(now);
        assert_eq!(s.helper.failed(), None);
        // the fill took A and stopped; the ask's D evicted A, and the
        // eviction is no answer, so it damages nothing
        assert_eq!(answers.len(), 2);
        let holds = |a: &Answer, c| a.face.has_glyph(c);
        assert!(holds(&answers[0], 'A') && !holds(&answers[0], 'D'));
        assert!(holds(&answers[1], 'D') && !holds(&answers[1], 'A'));
        assert!(!s.held('A') && !s.held('B') && !s.held('C') && s.held('D'));
        assert!(s.helper.store.borrow().bytes() <= limit);
        assert!(s.helper.stopped.is_empty());
        assert!(!s.helper.busy());
        // A, evicted, is asked for again when the screen shows it
        s.helper.ask(CELL, &shown(&['A']), now);
        assert!(s.helper.busy());
    }

    #[test]
    fn an_asks_glyphs_the_store_cannot_take_are_asked_again() {
        // one frame past the whole limit: dropped, and its glyph not left
        // pending
        let a = set(CELL, &['A']);
        let limit = (a.len() as u64) / 2;
        let mut fixture = frame_bytes(FRAME_GLYPHS, &a);
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::limited("toolarge", ANSWERS, &fixture, limit);
        let now = t0();
        s.helper.ask(CELL, &shown(&['A']), now);
        assert!(s.run(now).is_empty());
        assert_eq!(s.helper.failed(), None);
        assert!(!s.held('A'));
        assert!(!s.helper.busy());
        s.helper.ask(CELL, &shown(&['A']), now);
        assert!(s.helper.busy());
    }

    #[test]
    fn a_helper_that_hangs_is_started_again_then_given_up() {
        let mut s = StandIn::new("hangs", "exec sleep 30", b"");
        let mut now = t0();
        s.helper.ask(CELL, &shown(&['A']), now);
        assert!(s.helper.poll(now).is_empty());
        assert_eq!(
            s.helper.deadline(),
            Some(after(now, Duration::from_millis(ANSWER_MS)))
        );
        for _ in 0..=RESTARTS_MAX {
            now = after(now, Duration::from_millis(ANSWER_MS));
            assert!(s.helper.poll(now).is_empty());
        }
        let why = s.helper.failed().unwrap();
        assert!(why.contains("gave no answer for 10 s"), "{why}");
        assert!(s.helper.running.is_none());
    }

    #[test]
    fn an_ask_has_one_deadline_however_often_frames_come() {
        // a fill's frame keeps the answer due, but not the ask's end
        let mut s = StandIn::new(
            "deadline",
            "cat >/dev/null & cat \"$1\"; exec sleep 30",
            &frame_bytes(FRAME_FILL, &set(CELL, &['B'])),
        );
        let now = t0();
        s.helper.fill(CELL, &[], now);
        s.helper.ask(CELL, &shown(&['A']), now);
        let _ = s.woke.recv_timeout(Duration::from_secs(10));
        let nearly = after(now, Duration::from_millis(REQUEST_MS - 1));
        let late = after(now, Duration::from_millis(REQUEST_MS));
        assert_eq!(s.helper.poll(nearly).len(), 1);
        assert_eq!(s.helper.deadline(), Some(late));
        assert!(s.helper.poll(late).is_empty());
        assert_eq!(s.helper.restarts, 1);
    }

    #[test]
    fn a_helper_ended_by_its_cpu_limit_is_replaced_a_bounded_number_of_times() {
        // each helper answers its ask, then the kernel ends it for its CPU
        // time: the next need starts another, up to RESTARTS_MAX
        let mut fixture = frame_bytes(FRAME_GLYPHS, &set(CELL, &['A']));
        fixture.extend(frame_bytes(FRAME_DONE, &[]));
        let mut s = StandIn::new(
            "cpu",
            "head -c 1 >/dev/null; cat \"$1\"; kill -XCPU $$",
            &fixture,
        );
        let now = t0();
        for round in 0..=RESTARTS_MAX {
            s.helper.retain(&[], now);
            s.helper.ask(CELL, &shown(&['A']), now);
            assert_eq!(s.run(now).len(), 1, "round {round}");
            s.until_gone(now);
            if round < RESTARTS_MAX {
                assert_eq!(s.helper.failed(), None, "round {round}");
            }
        }
        let why = s.helper.failed().unwrap();
        assert!(why.contains("CPU limit"), "{why}");
    }

    fn own_cell_frame(width: u32, height: u32) -> Vec<u8> {
        let mut c = width.to_le_bytes().to_vec();
        c.extend_from_slice(&height.to_le_bytes());
        frame_bytes(FRAME_OWN_CELL, &c)
    }

    // the operator's decision on #145, 2026-10-28: the console asks the
    // helper for a bitmap frit's own cell and waits a bounded time; a
    // refusal is no failure, and an answer after the wait is dropped
    #[test]
    fn a_frits_own_cell_is_asked_and_waited_for_a_bounded_time() {
        let wait = Duration::from_secs(10);
        let mut s = StandIn::new("own-cell", ANSWERS, &own_cell_frame(8, 16));
        assert_eq!(s.helper.own_cell(wait, t0()), Ok(CELL));
        let refusal = frame_bytes(
            FRAME_NO_OWN_CELL,
            b"source=outline:/f.ttf is an outline font",
        );
        let mut s = StandIn::new("own-cell-refused", ANSWERS, &refusal);
        let why = s.helper.own_cell(wait, t0()).unwrap_err();
        assert!(why.contains("is an outline font"), "{why}");
        s.until_gone(t0());
        assert_eq!(s.helper.failed(), None);
        // the answer comes after the wait, and is dropped
        let late = "cat >/dev/null & sleep 0.5; cat \"$1\"; wait";
        let mut s = StandIn::new("own-cell-late", late, &own_cell_frame(8, 16));
        let why = s
            .helper
            .own_cell(Duration::from_millis(50), t0())
            .unwrap_err();
        assert_eq!(why, "the helper gave no answer in 50 ms");
        s.until_gone(t0());
        assert_eq!(s.helper.failed(), None);
        // past the cell's bound, or never asked: given up
        let mut s = StandIn::new("own-cell-huge", ANSWERS, &own_cell_frame(8, 4096));
        assert!(s.helper.own_cell(wait, t0()).is_err());
        s.run(t0());
        assert!(s.helper.failed().unwrap().contains("past CELL_SIDE_MAX"));
        let mut s = StandIn::new("own-cell-unasked", ANSWERS, &own_cell_frame(8, 16));
        s.helper.ask(CELL, &shown(&['A']), t0());
        s.run(t0());
        assert!(s.helper.failed().unwrap().contains("a cell query not made"));
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
            frit(""),
            None,
            Shared::default(),
            1 << 20,
        );
        h.ask(CELL, &shown(&['A']), t0());
        assert!(h.failed().unwrap().contains("cannot start"));
    }

    #[test]
    fn held_codepoints_read_back_as_ranges() {
        let mut b = Bits::new();
        for c in [0x41, 0x42, 0x43, 0x45, 0x10_ffff] {
            b.put(c, true);
        }
        b.put(0x42, false);
        assert_eq!(
            b.ranges(),
            vec![
                (0x41, 0x41),
                (0x43, 0x43),
                (0x45, 0x45),
                (0x10_ffff, 0x10_ffff)
            ]
        );
    }

    #[test]
    fn nobody_is_read_from_passwd() {
        let passwd = "root:x:0:0::/root:/bin/sh\nnobody:x:65534:65534:Nobody:/:/usr/bin/nologin\n";
        assert_eq!(nobody(passwd), Some((65534, 65534)));
        assert_eq!(nobody("nobody:x:0:0::/:/bin/sh\n"), None);
    }
}
