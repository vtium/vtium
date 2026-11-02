//! The helper's loop (ADR-1101 D4): it takes one request at a time from
//! vtium on descriptor 3, has a worker carry it out, removes a
//! shared-memory object that held a valid picture, and replies.
//!
//! For a one-off request the helper makes a Unix socket pair of the
//! sequenced-packet kind and the reply's memory file, forks a worker and
//! sends it the request, the reply's memory file and, for inline bytes,
//! their memory file over the pair. The worker checks its sandbox, opens
//! what the request names, removes a temporary file that must go, locks
//! itself in, reads, inflates or decodes, writes the reply's memory file
//! and answers with its outcome. The helper replies as soon as the outcome
//! is in and reaps the worker after. A worker killed by a signal shares
//! the helper's address layout, so the helper exits, and vtium counts a
//! crash and starts a new helper whose layout is chosen afresh.
//!
//! **Streams.** A program that plays video re-sends a picture for each
//! frame. Once [`STREAM_AFTER`] inline requests in a row carry the same
//! flags and picture size, the helper calls it a stream: it forks one
//! long-lived stream worker, which locks itself in before it reads
//! anything, and keeps [`REPLY_POOL_FILES`] reply files of the stream's
//! size, faulted in once and sealed against shrinking and growing only.
//! The stream worker writes each later frame into the next of them, and
//! the reply says the file is pooled. Frames from files and shared memory
//! never make a stream, since the stream worker can open nothing. A frame
//! that decodes to another size than the stream's ends the stream and is
//! run again as a one-off with a fresh reply file, which reads the same
//! bytes, since an inline frame's memory file is sealed. The stream ends
//! on the first request that does not match, after [`STREAM_IDLE_MS`]
//! with no request, or on any outcome but a picture. The helper counts
//! requests, never time, to start one. The maintainer chose the stream
//! worker after the measurement in the crate's README.
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

use crate::bounds::{
    HELPER_IDLE_MS, REPLY_POOL_FILES, SHM_DIR, SOCKET_FD, STREAM_AFTER, STREAM_IDLE_MS,
};
use crate::channel::{self, Outcome, Received};
use crate::fork::{self, Forked};
use crate::job::{self, ReplyFile, Source};
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
    /// The worker's second and third filters, built once.
    pub filter: sandbox::WorkerFilter,
    /// The landlock version the kernel offers, 0 for none: above 0, every
    /// worker must be in a landlock domain.
    pub landlock: u32,
    /// The seccomp filters the helper runs under once its own is in, where
    /// it installs one and the kernel counts them: every worker must run
    /// under one more.
    pub filters: Option<u32>,
}

/// What inline requests of one stream share, their picture's size apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kind {
    inflate: bool,
    png: bool,
    exact: bool,
    any_image: bool,
    raw: Option<(Layout, Alpha)>,
}

impl Kind {
    /// The kind of `request`, or `None` for one that never counts toward
    /// a stream: a discard, or anything but inline bytes.
    fn of(request: &Request) -> Option<Self> {
        if request.discard || !matches!(request.medium, Medium::Direct(_)) {
            return None;
        }
        Some(Self {
            inflate: request.inflate,
            png: request.png,
            exact: request.exact,
            any_image: request.any_image,
            raw: request.pixels.map(|d| (d.layout, d.alpha)),
        })
    }
}

/// The long-lived stream worker, its channel and the stream's pool.
#[derive(Debug)]
struct Stream {
    channel: OwnedFd,
    pid: Pid,
    pool: Vec<OwnedFd>,
    next: usize,
}

/// The inline requests in a row that match, and the stream once there are
/// enough of them.
#[derive(Debug, Default)]
struct Run {
    /// The kind and size of the last request's picture.
    last: Option<(Kind, (u32, u32))>,
    count: u32,
    stream: Option<Stream>,
}

impl Run {
    fn streaming(&self) -> bool {
        self.stream.is_some()
    }

    /// Whether `request` is the stream's next frame: of its kind, and for
    /// raw pixels of its size; a decoded picture's size is known only once
    /// it decodes.
    fn continues(&self, request: &Request) -> bool {
        let Some((kind, size)) = self.last else {
            return false;
        };
        self.streaming()
            && Kind::of(request) == Some(kind)
            && !request.pixels.is_some_and(|d| (d.width, d.height) != size)
    }

    /// Counts `request`'s outcome toward the run: a picture of the last
    /// one's kind and size continues it, anything else ends it. `true`
    /// when the run has just reached [`STREAM_AFTER`] with no stream yet,
    /// so one starts; `Err` when the stream it ends had a worker killed by
    /// a signal.
    fn after(&mut self, request: &Request, outcome: &Outcome) -> Result<bool, String> {
        let shape = Kind::of(request)
            .filter(|_| outcome.status == Status::Ok)
            .map(|k| (k, (outcome.width, outcome.height)));
        if shape.is_some() && shape == self.last {
            self.count = self.count.saturating_add(1);
        } else {
            self.end()?;
            self.last = shape;
            self.count = u32::from(shape.is_some());
        }
        Ok(self.count >= STREAM_AFTER && !self.streaming())
    }

    /// The run ends: the stream worker's channel is closed, so it exits,
    /// and it is reaped; the pool is freed, and the run counts again from
    /// nothing. `Err` when the stream worker was killed by a signal.
    fn end(&mut self) -> Result<(), String> {
        self.last = None;
        self.count = 0;
        let Some(stream) = self.stream.take() else {
            return Ok(());
        };
        drop(stream.channel);
        drop(stream.pool);
        if reaped(stream.pid) {
            Ok(())
        } else {
            Err("the stream worker was killed by a signal".to_owned())
        }
    }
}

/// The bytes of each pooled file for a stream of pictures `size`, as the
/// worker reported it, or `None` when that size is outside what `request`
/// allows: its largest width and height, and its `max_bytes`.
fn pool_len(request: &Request, (w, h): (u32, u32)) -> Option<u64> {
    if w == 0 || h == 0 || w > request.largest.0 || h > request.largest.1 {
        return None;
    }
    u64::from(w)
        .checked_mul(u64::from(h))?
        .checked_mul(4)
        .filter(|&len| len <= request.largest_bytes)
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

/// A stream worker forked for a stream of pictures `size`, the request
/// that started it giving the bounds, and its pool made; `None` when the
/// size is outside the bounds or the pool or the fork fails, which leaves
/// the frames to one-off workers.
fn start_stream(
    socket: &OwnedFd,
    request: &Request,
    size: (u32, u32),
    settings: &Settings,
) -> Option<Stream> {
    let len = pool_len(request, size)?;
    let (ours, theirs) = pair().ok()?;
    let close = [socket.as_raw_fd(), ours.as_raw_fd()];
    let pid = match fork::fork(&close).ok()? {
        Forked::Child => std::process::exit(stream_worker(&theirs, settings)),
        Forked::Parent(pid) => pid,
    };
    drop(theirs);
    let pool: Vec<OwnedFd> = (0..REPLY_POOL_FILES)
        .map_while(|_| pooled_file(len).ok())
        .collect();
    if pool.len() != REPLY_POOL_FILES {
        drop(ours);
        let _ = reaped(pid);
        return None;
    }
    Some(Stream {
        channel: ours,
        pid,
        pool,
        next: 0,
    })
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
                if run.end().is_err() {
                    return EXIT_FAILED;
                }
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
        // a request that is no frame of the stream ends it before any
        // other worker is forked, so none holds the stream's descriptors
        if run.streaming() && !run.continues(&request) && run.end().is_err() {
            return EXIT_FAILED;
        }
        let inline: Vec<BorrowedFd<'_>> = packet.fds.iter().map(AsFd::as_fd).collect();
        let served = serve_one(socket, &packet.bytes, &inline, &request, &mut run, settings);
        let Ok((outcome, reply, pooled, worker)) = served else {
            return EXIT_FAILED;
        };
        if !sandboxed(&outcome, settings) {
            return EXIT_FAILED;
        }
        let outcome = remove_shm(&request, outcome);
        if answer(socket, id, &request, &outcome, reply.as_ref(), pooled).is_err() {
            return EXIT_FAILED;
        }
        if worker.is_some_and(|w| !reaped(w)) {
            return EXIT_FAILED;
        }
        match run.after(&request, &outcome) {
            Ok(true) => {
                run.stream =
                    start_stream(socket, &request, (outcome.width, outcome.height), settings);
            }
            Ok(false) => {}
            Err(_) => return EXIT_FAILED,
        }
    }
}

/// One request carried out: by the stream worker writing the stream's
/// next pooled file when it is the stream's next frame, and otherwise, or
/// when a frame decodes to another size than the stream's, by a one-off
/// worker writing a fresh file. The outcome, the reply file, whether it is
/// pooled, and the one-off worker still to reap.
type Served = (Outcome, Option<OwnedFd>, bool, Option<Pid>);

fn serve_one(
    socket: &OwnedFd,
    packet: &[u8],
    inline: &[BorrowedFd<'_>],
    request: &Request,
    run: &mut Run,
    settings: &Settings,
) -> Result<Served, String> {
    if run.continues(request) {
        let frame = run.stream.as_mut().map(|s| frame(s, packet, inline));
        match frame {
            Some(Ok((outcome, pooled))) if !outcome.mismatch => {
                return Ok((outcome, Some(pooled), true, None));
            }
            Some(Ok(_)) => run.end()?,
            Some(Err(why)) => {
                let _ = run.end();
                return Err(why);
            }
            None => {}
        }
    }
    let reply = if request.discard {
        None
    } else {
        Some(job::memory_file("vtium-image-reply").map_err(|e| format!("no reply file: {e}"))?)
    };
    let (outcome, worker) = one_off(socket, packet, reply.as_ref(), inline, settings)?;
    Ok((outcome, reply, false, Some(worker)))
}

/// The stream's next frame sent to its worker with the next pooled file;
/// the outcome and the pooled file it wrote, or why the stream worker
/// failed.
fn frame(
    stream: &mut Stream,
    packet: &[u8],
    inline: &[BorrowedFd<'_>],
) -> Result<(Outcome, OwnedFd), String> {
    let len = stream.pool.len();
    let pooled = stream
        .pool
        .get(stream.next.checked_rem(len).unwrap_or(0))
        .and_then(|fd| fd.try_clone().ok())
        .ok_or("no pooled file")?;
    stream.next = stream.next.wrapping_add(1);
    let outcome = exchange(&stream.channel, JOB_POOLED, packet, Some(&pooled), inline)?;
    Ok((outcome, pooled))
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
    if let Err(why) = files::remove_checked(&path, id, files::me()) {
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

/// A Unix socket pair of the sequenced-packet kind, for a worker's
/// channel.
fn pair() -> Result<(OwnedFd, OwnedFd), String> {
    rustix::net::socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .map_err(|e| format!("cannot make the worker's channel: {e}"))
}

/// Sends a worker on `channel` one job, the request `packet` behind its
/// `kind`, with the reply file and the inline bytes' file, and returns its
/// outcome.
fn exchange(
    channel: &OwnedFd,
    kind: u8,
    packet: &[u8],
    reply: Option<&OwnedFd>,
    inline: &[BorrowedFd<'_>],
) -> Result<Outcome, String> {
    let mut job = Vec::with_capacity(packet.len().saturating_add(1));
    job.push(kind);
    job.extend_from_slice(packet);
    let mut handed: Vec<BorrowedFd<'_>> = reply.iter().map(|fd| fd.as_fd()).collect();
    handed.extend_from_slice(inline);
    channel::send(channel, &job, &handed)
        .map_err(|e| format!("cannot send the worker its request: {e}"))?;
    let got = channel::receive(channel, channel::OUTCOME_MAX)
        .map_err(|e| format!("the worker gave no outcome: {e:?}"))?;
    channel::decode_outcome(&got.bytes).ok_or_else(|| "a malformed outcome".to_owned())
}

/// Forks a one-off worker for one request and returns its outcome and its
/// pid, to reap once the reply is out, or why the helper must exit: the
/// pair or the fork failed, or the worker ended without a well-formed
/// outcome, as a worker killed by a signal or one that found its sandbox
/// wrong does.
fn one_off(
    socket: &OwnedFd,
    packet: &[u8],
    reply: Option<&OwnedFd>,
    inline: &[BorrowedFd<'_>],
    settings: &Settings,
) -> Result<(Outcome, Pid), String> {
    let (ours, theirs) = pair()?;
    let close = [socket.as_raw_fd(), ours.as_raw_fd()];
    match fork::fork(&close).map_err(|e| format!("cannot fork a worker: {e}"))? {
        Forked::Child => std::process::exit(worker(&theirs, settings)),
        Forked::Parent(pid) => {
            drop(theirs);
            match exchange(&ours, JOB_FRESH, packet, reply, inline) {
                Ok(o) => Ok((o, pid)),
                Err(why) => {
                    let _ = reaped(pid);
                    Err(why)
                }
            }
        }
    }
}

/// A worker's first steps, before it reads anything: vtium's socket,
/// descriptor 3, is closed in it, and it adds its second filter and then
/// runs under one more seccomp filter than the helper. The seccomp filters
/// it runs under, 0 where none is built or the kernel does not count
/// them; `None` when any of these is wrong, and the worker exits without
/// an outcome.
fn confined(settings: &Settings) -> Option<u32> {
    if fork::is_open(SOCKET_FD) {
        return None;
    }
    match settings.filter.install() {
        Ok(true) => {
            let filters = sandbox::seccomp_filters();
            let expected = settings.filters.and_then(|n| n.checked_add(1));
            if settings.filters.is_some() && filters != expected {
                return None;
            }
            Some(filters.unwrap_or(0))
        }
        Ok(false) => Some(0),
        Err(_) => None,
    }
}

/// `outcome` with what the worker found of its sandbox as it answers: the
/// seccomp filters `confined` counted, and whether it can open a
/// directory, probed now, apart from the lock, so the helper sees a worker
/// whose lock was skipped.
fn attested(mut outcome: Outcome, filters: u32) -> Outcome {
    outcome.filters = filters;
    outcome.locked = !sandbox::can_open_root();
    outcome
}

/// Whether a worker's outcome shows the sandbox the helper expects: one
/// more seccomp filter than the helper's, where the kernel counts them,
/// and a worker that could open no directory, wherever the kernel offers
/// landlock or the third filter is built. A worker that shows otherwise
/// fails the request as a crash.
fn sandboxed(outcome: &Outcome, settings: &Settings) -> bool {
    let filters = settings
        .filters
        .is_none_or(|n| n.checked_add(1) == Some(outcome.filters));
    let locked = outcome.locked || (settings.landlock == 0 && !sandbox::SECCOMP);
    filters && locked
}

/// The worker locks itself in: its landlock domain, or on a kernel
/// without landlock its third filter. `false` when the kernel offers
/// landlock and the worker is in no domain, or when a domain or the third
/// filter is in force and the worker can still open a directory.
fn locked(settings: &Settings) -> bool {
    let Ok(landlocked) = sandbox::lock_in() else {
        return false;
    };
    let no_open = if landlocked {
        false
    } else {
        match settings.filter.install_no_open() {
            Ok(installed) => installed,
            Err(_) => return false,
        }
    };
    if settings.landlock > 0 && !landlocked {
        return false;
    }
    !((landlocked || no_open) && sandbox::can_open_root())
}

/// A one-off worker's life on `channel`: its sandbox checked, then the one
/// request it is sent carried out and answered. The exit status: 0 once
/// the request is answered, 1 on a failure.
pub fn worker(channel: &OwnedFd, settings: &Settings) -> i32 {
    let Some(filters) = confined(settings) else {
        return 1;
    };
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
            _ => return 1,
        }
    };
    let inline = fds.next();
    let source = job::open(&request, inline, settings.tmpdir.as_deref());
    if !locked(settings) {
        return 1;
    }
    let outcome = match source {
        Ok(source) => job::finish(&request, source, reply.as_ref()),
        Err(f) => Outcome::failed(f.status, f.reason),
    };
    let outcome = attested(outcome, filters);
    if channel::send(channel, &channel::encode_outcome(&outcome), &[]).is_err() {
        return 1;
    }
    0
}

/// The stream worker's life on `channel`: its sandbox checked and locked
/// in before it reads anything, then each frame it is sent written into
/// the pooled file that comes with it and answered, until the helper
/// closes the channel. The exit status: 0 when the channel closes, 1 on a
/// failure, such as a job that is no inline frame with a pooled file.
pub fn stream_worker(channel: &OwnedFd, settings: &Settings) -> i32 {
    let Some(filters) = confined(settings) else {
        return 1;
    };
    if !locked(settings) {
        return 1;
    }
    loop {
        let packet = match channel::receive(channel, PACKET_MAX.saturating_add(1)) {
            Ok(p) => p,
            Err(Received::Closed) => return 0,
            Err(Received::Broken(_)) => return 1,
        };
        let Some((&JOB_POOLED, rest)) = packet.bytes.split_first() else {
            return 1;
        };
        let Ok((_, request)) = wire::decode_request(rest) else {
            return 1;
        };
        if Kind::of(&request).is_none() {
            return 1;
        }
        let mut fds = packet.fds.into_iter();
        let (Some(reply), Some(inline), None) = (fds.next(), fds.next(), fds.next()) else {
            return 1;
        };
        let reply = ReplyFile::Pooled(reply);
        let outcome = job::finish(&request, Source::Inline(inline), Some(&reply));
        let outcome = attested(outcome, filters);
        if channel::send(channel, &channel::encode_outcome(&outcome), &[]).is_err() {
            return 1;
        }
    }
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

    fn zlib(w: u32, h: u32) -> Request {
        Request {
            medium: Medium::Direct(Vec::new()),
            offset: 0,
            max: 0,
            min: u64::from(w * h * 4),
            inflate: true,
            png: false,
            exact: false,
            discard: false,
            any_image: false,
            pixels: Some(Description {
                layout: Layout::Rgba,
                alpha: Alpha::Straight,
                width: w,
                height: h,
                compressed: true,
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
    fn a_stream_starts_after_three_matching_inline_requests_and_a_mismatch_restarts_the_count() {
        let frame = zlib(2, 2);
        let mut run = Run::default();
        for n in 1..STREAM_AFTER {
            assert!(!run.after(&frame, &picture(2, 2)).unwrap(), "request {n}");
        }
        assert!(
            run.after(&frame, &picture(2, 2)).unwrap(),
            "a stream starts"
        );
        // another size of raw pixels restarts the count, as does an outcome
        // other than a picture
        assert!(!run.after(&zlib(3, 2), &picture(3, 2)).unwrap());
        assert_eq!(run.count, 1);
        run.after(&frame, &Outcome::failed(Status::TooShort, ""))
            .unwrap();
        assert_eq!((run.count, run.last), (0, None));
    }

    #[test]
    fn frames_from_files_and_shared_memory_never_make_a_stream() {
        let mut run = Run::default();
        for medium in [
            Medium::SharedMemory(b"f".to_vec()),
            Medium::File(b"/a".to_vec()),
            Medium::TempFile(b"/tmp/tty-graphics-protocol".to_vec()),
        ] {
            let r = Request {
                medium,
                inflate: false,
                ..zlib(2, 2)
            };
            for _ in 0..STREAM_AFTER * 2 {
                assert!(!run.after(&r, &picture(2, 2)).unwrap());
            }
        }
        let discard = Request {
            discard: true,
            ..zlib(2, 2)
        };
        assert_eq!(Kind::of(&discard), None);
    }

    #[test]
    fn the_pool_is_bounded_by_the_request_that_started_the_stream() {
        let r = zlib(2, 2);
        assert_eq!(pool_len(&r, (2, 2)), Some(16));
        // the size a worker reports is untrusted
        assert_eq!(pool_len(&r, (101, 1)), None);
        assert_eq!(pool_len(&r, (1, 101)), None);
        assert_eq!(pool_len(&r, (0, 1)), None);
        let poor = Request {
            largest_bytes: 15,
            ..r
        };
        assert_eq!(pool_len(&poor, (2, 2)), None);
    }

    #[test]
    fn a_pooled_file_is_sealed_against_shrinking_and_growing_only() {
        let pooled = pooled_file(64).unwrap();
        assert_eq!(rustix::fs::fstat(&pooled).unwrap().st_size, 64);
        let seals = rustix::fs::fcntl_get_seals(&pooled).unwrap();
        assert!(seals.contains(SealFlags::SHRINK | SealFlags::GROW));
        assert!(
            !seals.contains(SealFlags::WRITE),
            "a pooled file is written again"
        );
    }
}
