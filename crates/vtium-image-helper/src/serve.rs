//! The helper's loop (ADR-1101 D4): it takes one request at a time from
//! vtium on descriptor 3, forks a worker that carries it out, removes a
//! shared-memory object that held a valid picture, and replies.
//!
//! For each request the helper makes a Unix socket pair of the
//! sequenced-packet kind and takes the reply's memory file, forks a worker
//! and sends it the request, the reply's memory file and, for inline
//! bytes, their memory file over the pair. The worker adds its second
//! filter, opens what the request names, removes a temporary file that
//! must go, enters its landlock domain, reads, inflates or decodes, writes
//! the reply's memory file and answers with its outcome. The helper
//! replies as soon as the outcome is in and reaps the worker after. A
//! worker killed by a signal shares the helper's address layout, so the
//! helper exits, and vtium counts a crash and starts a new helper whose
//! layout is chosen afresh.
//!
//! **Streams.** A program that plays video re-sends a picture for each
//! frame. Once [`STREAM_AFTER`] requests in a row carry the same medium,
//! flags and picture size, the helper calls it a stream and keeps
//! [`REPLY_POOL_FILES`] reply files of the stream's size, faulted in once
//! and sealed against shrinking and growing only; each frame's worker,
//! still a fresh one, writes into the next of them, and the reply says the
//! file is pooled. The measurement ADR-1101 D4 asks for chose this way
//! over one long-lived worker for the stream (the crate's README gives the
//! figures). Raw frames count from any medium, since their size is in the
//! request; a decoded frame counts only when it was sent inline, since a
//! frame that decodes to another size is run again, as a one-off with a
//! fresh reply file, and only an inline frame's sealed memory file reads
//! the same bytes twice. The stream ends on the first request that does
//! not match, after [`STREAM_IDLE_MS`] with no request, or on any outcome
//! but a picture. The helper counts requests, never time, to start one.
//!
//! The helper exits 0 when vtium closes its end or after
//! [`HELPER_IDLE_MS`] with no request; 1 when the socket or a worker
//! fails; 2 on a request vtium would not build.

use std::ffi::OsStr;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rustix::event::{PollFd, PollFlags, Timespec};
use rustix::fs::SealFlags;
use rustix::net::{AddressFamily, SocketFlags, SocketType};
use rustix::process::{Pid, WaitOptions};
use vtium_image::decode::wire::{self, PACKET_MAX, ReplyHeader, Status};
use vtium_image::decode::{Alpha, Layout, Medium, Request};

use crate::bounds::{HELPER_IDLE_MS, REPLY_POOL_FILES, SHM_DIR, STREAM_AFTER, STREAM_IDLE_MS};
use crate::channel::{self, Outcome, Received};
use crate::fork::{Forked, fork};
use crate::job::{self, ReplyFile};
use crate::{files, sandbox};

/// The exit status for a request vtium would not build.
pub const EXIT_REQUEST: u8 = 2;
/// The exit status for a broken socket or a worker that failed.
pub const EXIT_FAILED: u8 = 1;

/// The first byte of a job a worker is sent: a fresh reply file it sizes
/// and seals, or a pooled one it only writes.
const JOB_FRESH: u8 = 0;
const JOB_POOLED: u8 = 1;

/// What the helper knows beside each request.
#[derive(Clone, Debug)]
pub struct Settings {
    /// vtium's own `TMPDIR`, resolved, where a temporary file may lie.
    pub tmpdir: Option<PathBuf>,
    /// The worker's second filter, built once.
    pub filter: sandbox::WorkerFilter,
}

/// What requests of one stream share, their picture's size apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kind {
    medium: char,
    inflate: bool,
    png: bool,
    exact: bool,
    any_image: bool,
    raw: Option<(Layout, Alpha)>,
}

impl Kind {
    /// The kind of `request`, or `None` for one that never counts toward
    /// a stream: a discard, or a decoded picture from a named medium.
    fn of(request: &Request) -> Option<Self> {
        let direct = matches!(request.medium, Medium::Direct(_));
        if request.discard || (request.pixels.is_none() && !direct) {
            return None;
        }
        Some(Self {
            medium: request.medium.letter(),
            inflate: request.inflate,
            png: request.png,
            exact: request.exact,
            any_image: request.any_image,
            raw: request.pixels.map(|d| (d.layout, d.alpha)),
        })
    }
}

/// The requests in a row that match, and the stream's pool once there are
/// enough of them.
#[derive(Debug, Default)]
struct Run {
    /// The kind and size of the last request's picture.
    last: Option<(Kind, (u32, u32))>,
    count: u32,
    pool: Vec<OwnedFd>,
    next: usize,
}

impl Run {
    fn streaming(&self) -> bool {
        !self.pool.is_empty()
    }

    /// A copy of the descriptor of the pooled file `request` writes, when
    /// it continues the stream: the same kind, and for raw pixels the same
    /// size; a decoded picture's size is known only once it decodes.
    fn pooled(&mut self, request: &Request) -> Option<OwnedFd> {
        let (kind, size) = self.last?;
        let len = self.pool.len();
        if len == 0 || Kind::of(request) != Some(kind) {
            return None;
        }
        if request.pixels.is_some_and(|d| (d.width, d.height) != size) {
            return None;
        }
        let fd = self
            .pool
            .get(self.next.checked_rem(len)?)?
            .try_clone()
            .ok()?;
        self.next = self.next.wrapping_add(1);
        Some(fd)
    }

    /// Counts `request`'s outcome toward the run: a picture of the last
    /// one's kind and size continues it, anything else ends it. Once the
    /// run reaches [`STREAM_AFTER`], the pool is made; a pool that cannot
    /// be made leaves the frames to fresh reply files.
    fn after(&mut self, request: &Request, outcome: &Outcome) {
        let shape = Kind::of(request)
            .filter(|_| outcome.status == Status::Ok)
            .map(|k| (k, (outcome.width, outcome.height)));
        if shape.is_some() && shape == self.last {
            self.count = self.count.saturating_add(1);
        } else {
            self.end();
            self.last = shape;
            self.count = u32::from(shape.is_some());
        }
        if self.count >= STREAM_AFTER && self.pool.is_empty() {
            let (w, h) = (outcome.width, outcome.height);
            let len = u64::from(w).saturating_mul(u64::from(h)).saturating_mul(4);
            self.pool = (0..REPLY_POOL_FILES)
                .filter_map(|_| pooled_file(len).ok())
                .collect();
            if self.pool.len() != REPLY_POOL_FILES {
                self.pool.clear();
            }
        }
    }

    /// The stream ends: its pool is freed and the run counts again from
    /// nothing.
    fn end(&mut self) {
        self.pool.clear();
        self.next = 0;
        self.count = 0;
        self.last = None;
    }
}

/// A pooled reply file of `len` bytes, every page faulted in once, sealed
/// against shrinking and growing, which still allow `pwrite`.
fn pooled_file(len: u64) -> std::io::Result<OwnedFd> {
    let fd = job::memory_file("vtium-image-pool")?;
    rustix::fs::ftruncate(&fd, len)?;
    let zeros = vec![0u8; 1 << 16];
    let mut at = 0u64;
    while at < len {
        let n = usize::try_from(len.saturating_sub(at).min(1 << 16)).unwrap_or(0);
        let wrote = rustix::io::pwrite(&fd, zeros.get(..n).unwrap_or_default(), at)?;
        if wrote == 0 {
            return Err(std::io::Error::other("no space for the pool"));
        }
        at = at.saturating_add(u64::try_from(wrote).unwrap_or(0));
    }
    rustix::fs::fcntl_add_seals(&fd, SealFlags::SHRINK | SealFlags::GROW)?;
    Ok(fd)
}

/// Waits up to `ms` for `socket` to be readable; `false` on the timeout.
fn readable(socket: impl AsFd, ms: u64) -> std::io::Result<bool> {
    let timeout = Timespec {
        tv_sec: i64::try_from(ms / 1000).unwrap_or(i64::MAX),
        tv_nsec: i64::try_from(ms.wrapping_rem(1000).saturating_mul(1_000_000)).unwrap_or(0),
    };
    let mut fds = [PollFd::new(&socket, PollFlags::IN)];
    loop {
        match rustix::event::poll(&mut fds, Some(&timeout)) {
            Ok(n) => return Ok(n > 0),
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
    }
}

/// Serves requests on `socket` until vtium closes it, the helper idles,
/// or something fails; the exit status.
pub fn serve(socket: &OwnedFd, settings: &Settings) -> u8 {
    let mut run = Run::default();
    loop {
        let wait = if run.streaming() {
            STREAM_IDLE_MS
        } else {
            HELPER_IDLE_MS
        };
        match readable(socket, wait) {
            Ok(true) => {}
            Ok(false) if run.streaming() => {
                run.end();
                continue;
            }
            Ok(false) => return 0,
            Err(_) => return EXIT_FAILED,
        }
        let packet = match channel::receive(socket, PACKET_MAX) {
            Ok(p) => p,
            Err(Received::Closed) => return 0,
            Err(Received::Broken(_)) => return EXIT_FAILED,
        };
        let Ok((id, request)) = wire::decode_request(&packet.bytes) else {
            return EXIT_REQUEST;
        };
        let direct = matches!(request.medium, Medium::Direct(_));
        if packet.fds.len() != usize::from(direct) {
            return EXIT_REQUEST;
        }
        let inline: Vec<BorrowedFd<'_>> = packet.fds.iter().map(AsFd::as_fd).collect();
        let served = serve_one(socket, &packet.bytes, &inline, &request, &mut run, settings);
        let Ok((outcome, reply, pooled, worker)) = served else {
            return EXIT_FAILED;
        };
        run.after(&request, &outcome);
        let outcome = remove_shm(&request, outcome);
        if answer(socket, id, &request, &outcome, reply.as_ref(), pooled).is_err() {
            return EXIT_FAILED;
        }
        if !reaped(worker) {
            return EXIT_FAILED;
        }
    }
}

/// One request carried out: by a worker writing the stream's next pooled
/// file when it continues a stream, and otherwise, or when a pooled frame
/// decodes to another size, by one writing a fresh file. The outcome, the
/// reply file, whether it is pooled, and the worker still to reap.
type Served = (Outcome, Option<OwnedFd>, bool, Pid);

fn serve_one(
    socket: &OwnedFd,
    packet: &[u8],
    inline: &[BorrowedFd<'_>],
    request: &Request,
    run: &mut Run,
    settings: &Settings,
) -> Result<Served, String> {
    if !request.discard {
        if let Some(pooled) = run.pooled(request) {
            let (outcome, worker) =
                run_worker(socket, JOB_POOLED, packet, Some(&pooled), inline, settings)?;
            if !outcome.mismatch {
                return Ok((outcome, Some(pooled), true, worker));
            }
            if !reaped(worker) {
                return Err("the worker was killed by a signal".to_owned());
            }
            run.end();
        }
    }
    let reply = if request.discard {
        None
    } else {
        Some(job::memory_file("vtium-image-reply").map_err(|e| format!("no reply file: {e}"))?)
    };
    let (outcome, worker) =
        run_worker(socket, JOB_FRESH, packet, reply.as_ref(), inline, settings)?;
    Ok((outcome, reply, false, worker))
}

/// Reaps the worker; `false` when it was killed by a signal.
fn reaped(worker: Pid) -> bool {
    match rustix::process::waitpid(Some(worker), WaitOptions::empty()) {
        Ok(Some((_, status))) => status.terminating_signal().is_none(),
        _ => false,
    }
}

/// The outcome after the helper, which is not locked in, removes the
/// shared-memory object a valid picture was read from, when its own
/// account owns it. A failure to remove adds to the reason only.
fn remove_shm(request: &Request, mut outcome: Outcome) -> Outcome {
    let (Medium::SharedMemory(name), Some(id), Status::Ok) =
        (&request.medium, outcome.shm, outcome.status)
    else {
        return outcome;
    };
    let Some(bare) = files::shm_name(name) else {
        return outcome;
    };
    let path = Path::new(SHM_DIR).join(OsStr::from_bytes(bare));
    if let Err(why) = files::remove_checked(&path, id) {
        outcome.reason = format!("shared memory not removed: {why}");
    }
    outcome
}

/// Replies to request `id` with `outcome`, carrying the reply's memory
/// file when the outcome is a picture, marked pooled when it is one of a
/// stream's.
fn answer(
    socket: impl AsFd,
    id: u64,
    request: &Request,
    outcome: &Outcome,
    reply: Option<&OwnedFd>,
    pooled: bool,
) -> std::io::Result<()> {
    let picture = outcome.status == Status::Ok && !request.discard;
    let header = ReplyHeader {
        status: outcome.status,
        id,
        width: if picture { outcome.width } else { 0 },
        height: if picture { outcome.height } else { 0 },
        ignored: if picture {
            u32::try_from(outcome.ignored).unwrap_or(u32::MAX)
        } else {
            0
        },
        pooled: picture && pooled,
        reason: outcome.reason.as_bytes().to_vec(),
    };
    let fds: Vec<BorrowedFd<'_>> = match (picture, reply) {
        (true, Some(fd)) => vec![fd.as_fd()],
        _ => Vec::new(),
    };
    channel::send(socket, &wire::encode_reply(&header), &fds)
}

/// Forks a worker for one request and returns its outcome and its pid, to
/// reap once the reply is out, or why the helper must exit: the pair or
/// the fork failed, or the worker ended without a well-formed outcome, as
/// a worker killed by a signal does.
fn run_worker(
    socket: &OwnedFd,
    pooled: u8,
    packet: &[u8],
    reply: Option<&OwnedFd>,
    inline: &[BorrowedFd<'_>],
    settings: &Settings,
) -> Result<(Outcome, Pid), String> {
    let (ours, theirs) = rustix::net::socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .map_err(|e| format!("cannot make the worker's channel: {e}"))?;
    let close = [socket.as_raw_fd(), ours.as_raw_fd()];
    match fork(&close).map_err(|e| format!("cannot fork a worker: {e}"))? {
        Forked::Child => std::process::exit(worker(&theirs, settings)),
        Forked::Parent(pid) => {
            drop(theirs);
            let mut job = Vec::with_capacity(packet.len().saturating_add(1));
            job.push(pooled);
            job.extend_from_slice(packet);
            let mut handed: Vec<BorrowedFd<'_>> = reply.iter().map(|fd| fd.as_fd()).collect();
            handed.extend_from_slice(inline);
            let sent = channel::send(&ours, &job, &handed);
            let got = channel::receive(&ours, channel::OUTCOME_MAX);
            let outcome = sent
                .map_err(|e| format!("cannot send the worker its request: {e}"))
                .and_then(|()| got.map_err(|e| format!("the worker gave no outcome: {e:?}")))
                .and_then(|p| {
                    channel::decode_outcome(&p.bytes)
                        .ok_or_else(|| "a malformed outcome".to_owned())
                });
            match outcome {
                Ok(o) => Ok((o, pid)),
                Err(why) => {
                    let _ = reaped(pid);
                    Err(why)
                }
            }
        }
    }
}

/// A worker's life on `channel`: its second filter, then the one request
/// it is sent carried out and answered. The exit status: 0 once the
/// request is answered, 1 on a failure.
pub fn worker(channel: &OwnedFd, settings: &Settings) -> i32 {
    if settings.filter.install().is_err() {
        return 1;
    }
    let packet = match channel::receive(channel, PACKET_MAX.saturating_add(1)) {
        Ok(p) => p,
        Err(_) => return 1,
    };
    let Some((&kind, rest)) = packet.bytes.split_first() else {
        return 1;
    };
    let Ok((_, request)) = wire::decode_request(rest) else {
        return 1;
    };
    let mut fds = packet.fds.into_iter();
    let reply = if request.discard {
        None
    } else {
        match (kind, fds.next()) {
            (JOB_FRESH, Some(fd)) => Some(ReplyFile::Fresh(fd)),
            (JOB_POOLED, Some(fd)) => Some(ReplyFile::Pooled(fd)),
            _ => return 1,
        }
    };
    let inline = fds.next();
    let source = job::open(&request, inline, settings.tmpdir.as_deref());
    if sandbox::lock_in().is_err() {
        return 1;
    }
    let outcome = match source {
        Ok(source) => job::finish(&request, source, reply.as_ref()),
        Err(f) => Outcome::failed(f.status, f.reason),
    };
    if channel::send(channel, &channel::encode_outcome(&outcome), &[]).is_err() {
        return 1;
    }
    0
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use vtium_image::decode::Description;

    fn raw(medium: Medium, w: u32, h: u32) -> Request {
        Request {
            medium,
            offset: 0,
            max: 0,
            min: u64::from(w * h * 4),
            inflate: false,
            png: false,
            exact: false,
            discard: false,
            any_image: false,
            pixels: Some(Description {
                layout: Layout::Rgba,
                alpha: Alpha::Straight,
                width: w,
                height: h,
                compressed: false,
            }),
            largest: (100, 100),
            largest_bytes: 1 << 20,
        }
    }

    fn picture(w: u32, h: u32) -> Outcome {
        Outcome {
            width: w,
            height: h,
            ..Outcome::failed(Status::Ok, "")
        }
    }

    #[test]
    fn a_stream_starts_after_three_matching_requests_and_ends_on_another() {
        let shm = raw(Medium::SharedMemory(b"f".to_vec()), 2, 2);
        let mut run = Run::default();
        for n in 1..=STREAM_AFTER {
            assert!(run.pooled(&shm).is_none(), "request {n} is no stream yet");
            run.after(&shm, &picture(2, 2));
        }
        assert!(run.streaming());
        // the pooled files alternate, each the stream's size
        let a = run.pooled(&shm).unwrap();
        let b = run.pooled(&shm).unwrap();
        let ino = |fd: &OwnedFd| rustix::fs::fstat(fd).unwrap().st_ino;
        assert_ne!(ino(&a), ino(&b));
        assert_eq!(rustix::fs::fstat(&a).unwrap().st_size, 16);
        assert_eq!(ino(&run.pooled(&shm).unwrap()), ino(&a));
        // another size of raw pixels is no frame of it, and ends it
        let other = raw(Medium::SharedMemory(b"f".to_vec()), 3, 2);
        assert!(run.pooled(&other).is_none());
        run.after(&other, &picture(3, 2));
        assert!(!run.streaming());
        // an outcome other than a picture ends a run too
        for _ in 0..STREAM_AFTER {
            run.after(&shm, &picture(2, 2));
        }
        assert!(run.streaming());
        run.after(&shm, &Outcome::failed(Status::TooShort, ""));
        assert!(!run.streaming());
    }

    #[test]
    fn a_decoded_frame_counts_only_when_sent_inline() {
        let png = |medium| Request {
            png: true,
            pixels: None,
            min: 0,
            ..raw(medium, 1, 1)
        };
        let mut run = Run::default();
        for _ in 0..STREAM_AFTER {
            run.after(&png(Medium::File(b"/a.png".to_vec())), &picture(4, 4));
        }
        assert!(!run.streaming(), "a file read twice could differ");
        for _ in 0..STREAM_AFTER {
            run.after(&png(Medium::Direct(Vec::new())), &picture(4, 4));
        }
        assert!(run.streaming());
        let pooled = run.pooled(&png(Medium::Direct(Vec::new()))).unwrap();
        assert_eq!(rustix::fs::fstat(&pooled).unwrap().st_size, 64);
        let seals = rustix::fs::fcntl_get_seals(&pooled).unwrap();
        assert!(seals.contains(SealFlags::SHRINK | SealFlags::GROW));
        assert!(
            !seals.contains(SealFlags::WRITE),
            "a pooled file is written again"
        );
    }
}
