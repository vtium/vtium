//! The helper's loop (ADR-1101 D4): it takes one request at a time from
//! vtium on descriptor 3, forks a worker that carries it out, removes a
//! shared-memory object that held a valid picture, and replies.
//!
//! For each request the helper makes a Unix socket pair of the
//! sequenced-packet kind and the reply's memory file, forks a worker and
//! sends it the request, its inline memory file where it has one, and the
//! reply's memory file over the pair. The worker adds its second filter,
//! opens what the request names, removes a temporary file that must go,
//! enters its landlock domain, reads, inflates or decodes, writes the
//! reply's memory file and answers with its outcome. A worker killed by a
//! signal shares the helper's address layout, so the helper exits, and
//! vtium counts a crash and starts a new helper whose layout is chosen
//! afresh.
//!
//! The helper exits 0 when vtium closes its end or after
//! [`HELPER_IDLE_MS`] with no request; 1 when the socket or a worker
//! fails; 2 on a request vtium would not build.

use std::ffi::OsStr;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rustix::event::{PollFd, PollFlags, Timespec};
use rustix::net::{AddressFamily, SocketFlags, SocketType};
use rustix::process::WaitOptions;
use vtium_image::decode::wire::{self, PACKET_MAX, ReplyHeader, Status};
use vtium_image::decode::{Medium, Request};

use crate::bounds::{HELPER_IDLE_MS, SHM_DIR};
use crate::channel::{self, Outcome, Received};
use crate::fork::{Forked, fork};
use crate::job::{self, ReplyFile};
use crate::{files, sandbox};

/// The exit status for a request vtium would not build.
pub const EXIT_REQUEST: u8 = 2;
/// The exit status for a broken socket or a worker that failed.
pub const EXIT_FAILED: u8 = 1;

/// What the helper knows beside each request.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    /// vtium's own `TMPDIR`, resolved, where a temporary file may lie.
    pub tmpdir: Option<PathBuf>,
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
    loop {
        match readable(socket, HELPER_IDLE_MS) {
            Ok(true) => {}
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
        let reply = if request.discard {
            None
        } else {
            match job::memory_file("vtium-image-reply") {
                Ok(fd) => Some(fd),
                Err(_) => return EXIT_FAILED,
            }
        };
        let outcome = match run_worker(socket, &packet.bytes, packet.fds, reply.as_ref(), settings)
        {
            Ok(o) => o,
            Err(_) => return EXIT_FAILED,
        };
        let outcome = remove_shm(&request, outcome);
        if answer(socket, id, &request, &outcome, reply.as_ref()).is_err() {
            return EXIT_FAILED;
        }
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
/// file when the outcome is a picture.
fn answer(
    socket: impl AsFd,
    id: u64,
    request: &Request,
    outcome: &Outcome,
    reply: Option<&OwnedFd>,
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
        pooled: false,
        reason: outcome.reason.as_bytes().to_vec(),
    };
    let fds: Vec<BorrowedFd<'_>> = match (picture, reply) {
        (true, Some(fd)) => vec![fd.as_fd()],
        _ => Vec::new(),
    };
    channel::send(socket, &wire::encode_reply(&header), &fds)
}

/// Forks a one-off worker for one request and returns its outcome, or why
/// the helper must exit: the pair or the fork failed, the worker was
/// killed by a signal, or it ended without a well-formed outcome.
fn run_worker(
    socket: &OwnedFd,
    packet: &[u8],
    fds: Vec<OwnedFd>,
    reply: Option<&OwnedFd>,
    settings: &Settings,
) -> Result<Outcome, String> {
    let (ours, theirs) = rustix::net::socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .map_err(|e| format!("cannot make the worker's channel: {e}"))?;
    let close = [socket.as_raw_fd(), ours.as_raw_fd()];
    match fork(&close).map_err(|e| format!("cannot fork a worker: {e}"))? {
        Forked::Child => std::process::exit(worker(&theirs, settings, true)),
        Forked::Parent(pid) => {
            drop(theirs);
            let mut handed: Vec<BorrowedFd<'_>> = reply.iter().map(|fd| fd.as_fd()).collect();
            handed.extend(fds.iter().map(AsFd::as_fd));
            let sent = channel::send(&ours, packet, &handed);
            let got = channel::receive(&ours, channel::OUTCOME_MAX);
            drop(ours);
            let status = rustix::process::waitpid(Some(pid), WaitOptions::empty())
                .map_err(|e| format!("cannot wait for the worker: {e}"))?;
            if let Some(signal) = status.and_then(|(_, s)| s.terminating_signal()) {
                return Err(format!("the worker was killed by signal {signal}"));
            }
            sent.map_err(|e| format!("cannot send the worker its request: {e}"))?;
            let packet = got.map_err(|e| format!("the worker gave no outcome: {e:?}"))?;
            channel::decode_outcome(&packet.bytes).ok_or_else(|| "a malformed outcome".to_owned())
        }
    }
}

/// A worker's life on `channel`: its second filter, then each request it
/// is sent carried out and answered, one alone for a one-off worker. The
/// exit status: 0 once its channel closes or its one request is answered,
/// 1 on a failure.
pub fn worker(channel: &OwnedFd, settings: &Settings, one_off: bool) -> i32 {
    if sandbox::install_worker_filter().is_err() {
        return 1;
    }
    loop {
        let packet = match channel::receive(channel, PACKET_MAX) {
            Ok(p) => p,
            Err(Received::Closed) => return 0,
            Err(Received::Broken(_)) => return 1,
        };
        let Ok((_, request)) = wire::decode_request(&packet.bytes) else {
            return 1;
        };
        let mut fds = packet.fds.into_iter();
        let reply = if request.discard {
            None
        } else {
            fds.next().map(ReplyFile::Fresh)
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
        if one_off {
            return 0;
        }
    }
}
