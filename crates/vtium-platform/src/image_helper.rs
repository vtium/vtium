//! The image helper's host (ADR-1101): it starts `vtium-image` as the
//! session's user, sends it one request at a time, checks each reply
//! before it reads a pixel, kills and reaps the helper, and bounds its
//! crashes.
//!
//! Each console has one [`ImageHost`], which lasts as long as the
//! console's session, so its crash count does too. The binary hands it
//! each request the terminal makes, with the console's [`Session`]:
//! [`ImageHost::send`] either answers at once, without a helper, or sends
//! the request and returns. While the request is out, the binary pauses
//! the console's pty, watches [`ImageHost::wake`] in its loop and holds the
//! reply timer; [`ImageHost::readable`] gives the reply, which the binary
//! hands back to the terminal before it resumes the pty. When the timer
//! fires first, the binary calls [`ImageHost::stop`], which kills the
//! helper and counts no crash.
//!
//! - **Whose account.** `identity` finds the session's user again
//!   before each request, and a running helper whose account differs is
//!   stopped.
//! - **The start.** [`Program`] is the program, opened once and checked;
//!   each helper starts from it as `start.rs` describes.
//! - **The reply.** Every packet is checked in this order:
//!   a request out, the exact length, the version, id and status, the
//!   bytes ignored, the one descriptor of a picture, a regular file whose
//!   seals hold before its size is read, the exact size. The reply then
//!   carries the file itself, unread, as a [`ReplyFile`]: the handler
//!   reserves the picture's bytes under the budget first, and only then
//!   copies the pixels once into that buffer, clamping them as it goes,
//!   out of a read-only mapping of a sealed file or with `pread` from a
//!   pooled one.
//! - **Failures.** The socket failing, or a reply that fails a check,
//!   ends the request with [`Reply::Failed`], kills the helper's process
//!   group and reaps it, and counts a crash. A helper found exited, or
//!   with its socket closed, counts a crash too unless it exited with
//!   status 0, which a helper that idles out or loses vtium's socket does:
//!   the real helper exits 1 when a worker is killed by a signal, after
//!   its outcome is in or before. When that happens while a request is
//!   out, the request fails either way. A helper that idles out just as
//!   vtium sends a request exits 0 with the request unread, and counts no
//!   crash, since an honest helper never exits 0 once it has read one.
//!   After
//!   [`IMAGE_CRASHES_MAX`] the console starts no more helpers until its
//!   session ends. The binary's timer stops the helper the same way but
//!   counts nothing, since a large picture on a slow board can take that
//!   long honestly.

mod identity;
#[cfg(feature = "logind")]
mod login;
mod start;
#[cfg(test)]
mod tests;

use std::io::IoSliceMut;
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rustix::fs::{FileType, SealFlags};
use rustix::net::{
    RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags, SendAncillaryBuffer,
    SendAncillaryMessage, SendFlags,
};
use vtium_image::decode::wire::{
    self, Hello, REASON_MAX, REPLY_FIXED, REPLY_FRAMES_MAX, ReplyHeader, Status,
};
use vtium_image::decode::{FrameData, Medium, Payload, Reply, ReplyFile, ReplySource, Request};

pub use identity::{
    Account, Facts, LOGIN_CLASSES, LoginSession, Me, PRELOGIN_ACCOUNTS, Prelogin, Refusal, Session,
    System, holds_group_zero, resolve,
};
pub use start::{PROGRAM, Program, Started, memory_cap, open_program, start};

use crate::bounds::IMAGE_CRASHES_MAX;

/// The most descriptors a reply is read with, so every one a helper sends
/// is received and closed, and one too many is seen.
const REPLY_FDS: usize = 8;

/// Whether this boot has logged an image helper's sandbox line ([`note`]).
static SANDBOX_NOTED: AtomicBool = AtomicBool::new(false);

/// What starts a host's helpers: this process, which also chooses each
/// helper's account, or under the privilege split vtium's root process,
/// which chooses the account itself.
///
/// References:
/// - ADR-1400 D4, "What the broker does": the broker starts each image
///   helper at the drawer's request and chooses its user.
pub trait Launch {
    /// The account a helper for one request of file media (`file`) or
    /// inline bytes runs as, as far as this process can tell, and whether
    /// a disagreement between the pty's owner and logind should be
    /// logged, as [`resolve`] gives them. A running helper whose account
    /// differs is stopped before the request.
    fn account(
        &mut self,
        session: &Session,
        file: bool,
    ) -> (Result<Account, Refusal>, Option<(u32, u32)>);
    /// Whether a helper can start at all, such as when a program is open.
    fn can_start(&self) -> bool;
    /// Starts a helper as `account` for a request of file media (`file`)
    /// or inline bytes, its memory cap set for `max_bytes` and its largest
    /// file at `largest_file`.
    fn start(
        &mut self,
        account: &Account,
        file: bool,
        max_bytes: u64,
        largest_file: u64,
    ) -> Result<Box<dyn Process>, String>;
}

/// A helper's process, as its host sees it.
pub trait Process: std::fmt::Debug {
    /// The host's end of the helper's socket.
    fn socket(&self) -> BorrowedFd<'_>;
    /// The helper's process id, when this process knows it.
    fn pid(&self) -> Option<u32>;
    /// Whether the helper has exited.
    fn exited(&self) -> bool;
    /// Kills the helper's process group, and reaps the helper.
    fn kill(self: Box<Self>);
    /// Ends a helper that closed its socket or has exited, and says how
    /// it ended, or hands the process back while that is not known yet.
    fn finish(self: Box<Self>) -> Finish;
}

/// How a helper that closed its socket or exited ended.
#[derive(Debug)]
pub enum Finish {
    /// With status 0.
    Clean,
    /// With another status, or killed by a signal.
    Failed,
    /// Not known yet: the process that reaps the helper has not reported
    /// its end. The host asks again before its next request.
    Unknown(Box<dyn Process>),
}

/// This process as the starter of a host's helpers: from `program`, with
/// vtium's own `TMPDIR`, as the account [`resolve`] finds.
struct Here {
    program: Option<Program>,
    tmpdir: Option<PathBuf>,
    facts: Box<dyn Facts>,
    prelogin: Prelogin,
}

impl Launch for Here {
    fn account(
        &mut self,
        session: &Session,
        file: bool,
    ) -> (Result<Account, Refusal>, Option<(u32, u32)>) {
        resolve(&*self.facts, &mut self.prelogin, session, file)
    }

    fn can_start(&self) -> bool {
        self.program.is_some()
    }

    fn start(
        &mut self,
        account: &Account,
        _: bool,
        max_bytes: u64,
        largest_file: u64,
    ) -> Result<Box<dyn Process>, String> {
        let program = self
            .program
            .as_ref()
            .ok_or_else(|| "no image helper to start".to_owned())?;
        let started = start::start(
            program,
            account,
            max_bytes,
            largest_file,
            self.tmpdir.as_deref(),
        )?;
        Ok(Box::new(started))
    }
}

impl Process for Started {
    fn socket(&self) -> BorrowedFd<'_> {
        self.socket.as_fd()
    }

    fn pid(&self) -> Option<u32> {
        u32::try_from(self.pid.as_raw_nonzero().get()).ok()
    }

    fn exited(&self) -> bool {
        start::exited(self)
    }

    /// Nothing reaps the helper before this, so its group's id cannot
    /// have passed to another process when the group is signalled.
    fn kill(self: Box<Self>) {
        start::kill_group(&self);
        if !start::reap(&self, false) {
            // a process killed is gone in a moment; this thread waits for
            // it so no zombie stays behind
            std::thread::spawn(move || start::reap(&self, true));
        }
    }

    fn finish(self: Box<Self>) -> Finish {
        if start::finish(&self) {
            Finish::Clean
        } else {
            Finish::Failed
        }
    }
}

/// A running helper and what it was started for.
#[derive(Debug)]
struct Running {
    process: Box<dyn Process>,
    account: Account,
    /// Its memory cap was set from this.
    max_bytes: u64,
    /// Its largest file was set to this: `max_bytes` and the frames past
    /// the first of an animated GIF (ADR-1101 D3).
    largest_file: u64,
    /// Its hello, once it has arrived.
    hello: Option<Hello>,
}

/// The request out.
#[derive(Debug)]
struct Out {
    id: u64,
    /// The request, its inline bytes dropped.
    request: Request,
}

/// One console's image helper host.
pub struct ImageHost {
    launch: Box<dyn Launch>,
    running: Option<Running>,
    /// The helpers that ended before their end was known, which the host
    /// asks again before its next request.
    ending: Vec<Box<dyn Process>>,
    out: Option<Out>,
    crashes: usize,
    next_id: u64,
    /// The last disagreement between the pty's owner and logind logged.
    disagreement: Option<(u32, u32)>,
}

impl std::fmt::Debug for ImageHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageHost")
            .field("running", &self.running)
            .field("out", &self.out)
            .field("crashes", &self.crashes)
            .finish_non_exhaustive()
    }
}

impl ImageHost {
    /// A host that starts helpers from `program`, or answers every request
    /// as ADR-1100 D7's "Without a helper" says when there is none; a
    /// temporary file may also lie in `tmpdir`, vtium's own `TMPDIR`.
    pub fn new(program: Option<Program>, tmpdir: Option<PathBuf>) -> Self {
        Self::with_facts(program, tmpdir, Box::new(System))
    }

    /// [`ImageHost::new`] reading the system through `facts`.
    pub fn with_facts(
        program: Option<Program>,
        tmpdir: Option<PathBuf>,
        facts: Box<dyn Facts>,
    ) -> Self {
        Self::launched(Box::new(Here {
            program,
            tmpdir,
            facts,
            prelogin: Prelogin::default(),
        }))
    }

    /// A host whose helpers `launch` starts, such as vtium's root process
    /// under the privilege split.
    pub fn launched(launch: Box<dyn Launch>) -> Self {
        Self {
            launch,
            running: None,
            ending: Vec::new(),
            out: None,
            crashes: 0,
            next_id: 1,
            disagreement: None,
        }
    }

    /// Whether this console can start a helper: it has a program and has
    /// not reached the crash limit. While it cannot, the binary tells the
    /// terminal there is no helper, so the kitty handler answers itself.
    pub fn available(&self) -> bool {
        self.launch.can_start() && self.crashes < IMAGE_CRASHES_MAX
    }

    /// Crashes this console's session has counted.
    pub fn crashes(&self) -> usize {
        self.crashes
    }

    /// Whether a request is out.
    pub fn busy(&self) -> bool {
        self.out.is_some()
    }

    /// The running helper's pid, while one runs.
    pub fn pid(&self) -> Option<u32> {
        self.running.as_ref().and_then(|r| r.process.pid())
    }

    /// The running helper's hello, once it has arrived: the landlock
    /// version its kernel offers and whether its seccomp filter is in
    /// force.
    pub fn hello(&self) -> Option<Hello> {
        self.running.as_ref().and_then(|r| r.hello)
    }

    /// While a request is out, a descriptor that becomes readable when the
    /// helper has written to its socket: a copy of the socket the loop may
    /// own, in a calloop source say, until the reply. Each helper has a
    /// socket of its own, so the loop takes a new one for each request.
    pub fn wake(&self) -> Option<OwnedFd> {
        self.out.as_ref()?;
        let socket = self.running.as_ref()?.process.socket();
        rustix::io::fcntl_dupfd_cloexec(socket, 3).ok()
    }

    /// Sends `request` for the console whose session is `session`; `Err`
    /// holds the reply when it is answered at once: with no helper, past
    /// the crash limit, with no account that may read it, or when the
    /// helper cannot be started or sent the request.
    pub fn send(&mut self, request: &Request, session: &Session) -> Result<(), Reply> {
        if self.out.is_some() {
            return Err(Reply::Failed);
        }
        self.settle_ending();
        if self.running.as_ref().is_some_and(|r| r.process.exited()) {
            self.retire();
        }
        if !self.available() {
            return Err(without_helper(request));
        }
        // more bytes needed than the budget is answered without the helper,
        // which would exit on it
        if !request.discard && request.min > request.largest_bytes {
            return Err(Reply::OverBudget);
        }
        let file = !matches!(request.medium, Medium::Direct(_));
        let (account, disagree) = self.launch.account(session, file);
        if let Some(pair) = disagree.filter(|&p| self.disagreement != Some(p)) {
            self.disagreement = Some(pair);
            log::warn!(
                "the pty belongs to uid {}, but logind's session is uid {}'s; the image helper runs as {}",
                pair.0,
                pair.1,
                pair.1
            );
        }
        let account = account.map_err(|why| {
            log::debug!("an image request refused: {why:?}");
            Reply::Unreadable
        })?;
        let frames = request.frames.map_or(0, |l| l.max_frame_bytes);
        self.ensure(
            &account,
            file,
            request.largest_bytes,
            request.largest_bytes.saturating_add(frames),
        )?;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let packet = wire::encode_request(id, request).map_err(|why| {
            log::warn!("an image request vtium would not send: {why}");
            Reply::Failed
        })?;
        let inline = match &request.medium {
            Medium::Direct(bytes) => Some(inline_file(bytes).map_err(|e| {
                log::warn!("cannot hand inline bytes to the image helper: {e}");
                Reply::Unreadable
            })?),
            _ => None,
        };
        let Some(running) = &self.running else {
            return Err(Reply::Unreadable);
        };
        if let Err(e) = send_packet(running.process.socket(), &packet, inline.as_ref()) {
            log::warn!("cannot send image request {id}: {e}");
            self.retire();
            return Err(Reply::Unreadable);
        }
        let mut kept = request.clone();
        if let Medium::Direct(bytes) = &mut kept.medium {
            *bytes = Vec::new();
        }
        self.out = Some(Out { id, request: kept });
        Ok(())
    }

    /// A helper running as `account` for requests of up to `max_bytes`
    /// whose reply files reach `largest_file`: the one running, unless its
    /// account differs or either of its caps was set from fewer bytes, else
    /// a new one. A helper started again for the same account takes the
    /// larger of each cap, the old one's and the request's, so requests
    /// that alternate a larger budget and more frame room restart it once.
    fn ensure(
        &mut self,
        account: &Account,
        file: bool,
        max_bytes: u64,
        largest_file: u64,
    ) -> Result<(), Reply> {
        let (mut max_bytes, mut largest_file) = (max_bytes, largest_file);
        let stale = self.running.as_ref().is_some_and(|r| {
            r.account != *account || max_bytes > r.max_bytes || largest_file > r.largest_file
        });
        if stale {
            if let Some(r) = self.running.as_ref().filter(|r| r.account == *account) {
                max_bytes = max_bytes.max(r.max_bytes);
                largest_file = largest_file.max(r.largest_file);
            }
            self.kill();
        }
        if self.running.is_some() {
            return Ok(());
        }
        if !self.launch.can_start() {
            return Err(Reply::Unreadable);
        }
        let process = self
            .launch
            .start(account, file, max_bytes, largest_file)
            .map_err(|why| {
                log::warn!("{why}");
                Reply::Unreadable
            })?;
        log::debug!(
            "image helper {:?} started as uid {}",
            process.pid(),
            account.uid
        );
        self.running = Some(Running {
            process,
            account: account.clone(),
            max_bytes,
            largest_file,
            hello: None,
        });
        Ok(())
    }

    /// Kills the helper's process group and reaps the helper.
    fn kill(&mut self) {
        if let Some(r) = self.running.take() {
            r.process.kill();
        }
    }

    /// The request out fails with `why`: the helper is stopped and the
    /// crash counted.
    fn crash(&mut self, why: &str) -> Reply {
        log::warn!("image helper: {why}; stopping it");
        self.kill();
        self.out = None;
        self.count_crash();
        Reply::Failed
    }

    /// A helper that has exited or closed its socket is reaped, and a
    /// crash counted unless it exited with status 0.
    fn retire(&mut self) {
        let Some(r) = self.running.take() else {
            return;
        };
        self.finished(r.process);
    }

    /// Counts a crash for `process`, which ended, unless it exited with
    /// status 0. While its end is not known it waits in `ending`.
    fn finished(&mut self, process: Box<dyn Process>) {
        let pid = process.pid();
        match process.finish() {
            Finish::Clean => {}
            Finish::Failed => {
                log::warn!("image helper {pid:?} ended with another status than 0");
                self.count_crash();
            }
            Finish::Unknown(process) => self.ending.push(process),
        }
    }

    /// Counts a crash for each helper in `ending` whose end is now known
    /// and was not status 0.
    fn settle_ending(&mut self) {
        for process in std::mem::take(&mut self.ending) {
            self.finished(process);
        }
    }

    fn count_crash(&mut self) {
        self.crashes = self.crashes.saturating_add(1);
        if self.crashes == IMAGE_CRASHES_MAX {
            log::warn!(
                "the image helper crashed {IMAGE_CRASHES_MAX} times on this console; it starts no more until the console's session ends"
            );
        }
    }

    /// The binary's reply timer fired: the request out is dropped and the
    /// helper killed, with no crash counted. The next request starts a new
    /// helper.
    pub fn stop(&mut self) {
        if let Some(out) = self.out.take() {
            log::warn!(
                "image request {} had no reply in time; stopping the helper",
                out.id
            );
        }
        self.kill();
    }

    /// The console's session has ended: the request out is dropped with no
    /// reply, the helper stopped, and the crash count cleared.
    pub fn session_ended(&mut self) {
        self.out = None;
        self.kill();
        self.crashes = 0;
        self.disagreement = None;
    }

    /// The socket has input: the reply to the request out once it has
    /// arrived and passed every check, or [`Reply::Failed`] when it has
    /// not; `None` while there is nothing more to read.
    pub fn readable(&mut self) -> Option<Reply> {
        loop {
            let running = self.running.as_ref()?;
            let packet = match receive(running.process.socket()) {
                Ok(Some(p)) => p,
                Ok(None) => return None,
                Err(Broken::Closed) => {
                    // a helper that idled out, perhaps just as the request
                    // went out, or one that failed: its status says which
                    let out = self.out.take();
                    self.retire();
                    return out.map(|out| {
                        log::debug!("image request {} went out as the helper ended", out.id);
                        Reply::Failed
                    });
                }
                Err(why) => return Some(self.crash(why.text())),
            };
            if running.hello.is_none() {
                match wire::decode_hello(&packet.bytes) {
                    Ok(hello) if packet.fds.is_empty() => {
                        note(&hello);
                        if let Some(r) = self.running.as_mut() {
                            r.hello = Some(hello);
                        }
                        continue;
                    }
                    _ => {
                        let why = match wire::hello_version(&packet.bytes) {
                            Some(v) if v != wire::VERSION => format!(
                                "its hello speaks wire version {v}, and vtium speaks version {}",
                                wire::VERSION
                            ),
                            _ => "its first packet is no hello".to_owned(),
                        };
                        return Some(self.crash(&why));
                    }
                }
            }
            return Some(match self.check(packet) {
                Ok(reply) => {
                    self.out = None;
                    reply
                }
                Err(why) => self.crash(why),
            });
        }
    }

    /// One reply packet checked against the request out, before any byte
    /// of a picture is read; its picture stays in its file, unread.
    fn check(&mut self, mut packet: Packet) -> Result<Reply, &'static str> {
        let out = self.out.as_ref().ok_or("a reply with no request out")?;
        let header = wire::decode_reply(&packet.bytes).map_err(|m| m.0)?;
        let request = &out.request;
        if header.ignored != 0
            && (header.status != Status::Ok
                || request.discard
                || (request.max != 0 && u64::from(header.ignored) > request.max))
        {
            return Err("a reply whose ignored count is out of bounds");
        }
        if header.id != out.id {
            return Err("a reply to another request");
        }
        let picture = header.status == Status::Ok && !request.discard;
        if packet.fds.len() != usize::from(picture) {
            return Err(if picture {
                "a reply without exactly one descriptor"
            } else {
                "a reply with a descriptor it should not carry"
            });
        }
        // a picture counts one frame or more, any other reply none; more
        // than one only for a request that asked for frames, within its
        // max_frames
        let count = header.delays.len();
        let frames_ok = if picture {
            count == 1
                || (count > 1
                    && request.frames.is_some_and(|l| {
                        u64::try_from(count).is_ok_and(|c| c <= u64::from(l.max_frames))
                    }))
        } else {
            count == 0 && header.loops.is_none()
        };
        if !frames_ok {
            return Err("a reply whose frame count does not fit its request");
        }
        log_reason(out.id, &header);
        if let Some(failure) = header.status.failure() {
            return Ok(failure);
        }
        if request.discard {
            return Ok(Reply::Ok(Payload::Discarded));
        }
        let fd = packet
            .fds
            .pop()
            .ok_or("a reply without exactly one descriptor")?;
        let file = checked_file(fd, &header, request)?;
        if header.ignored != 0 {
            log::debug!(
                "image request {}, t={}: {} bytes past what was used ignored",
                out.id,
                request.medium.letter(),
                header.ignored
            );
        }
        if count > 1 {
            return Ok(Reply::Ok(Payload::Frames {
                width: header.width,
                height: header.height,
                delays: header.delays,
                loops: header.loops,
                data: FrameData::File(file),
            }));
        }
        Ok(Reply::Ok(Payload::File {
            width: header.width,
            height: header.height,
            file,
        }))
    }
}

impl Drop for ImageHost {
    fn drop(&mut self) {
        self.kill();
    }
}

/// The answer to `request` with no helper (ADR-1100 D7): file media are
/// unreadable, which the kitty handler answers EBADF; PNG and zlib do not
/// decode, which it answers EINVAL.
pub fn without_helper(request: &Request) -> Reply {
    match request.medium {
        Medium::Direct(_) => Reply::Corrupt,
        _ => Reply::Unreadable,
    }
}

/// Logs once a boot what the helper's hello says it lacks, as
/// [`sandbox_line`] words it, and shows nothing on screen.
fn note(hello: &Hello) {
    if let Some(line) = sandbox_line(hello)
        && !SANDBOX_NOTED.swap(true, Ordering::Relaxed)
    {
        log::warn!("{line}");
    }
}

/// The line vtium logs when the helper's hello says a part of its sandbox
/// is not in force: each missing part and the reason the helper gives, and
/// whether the workers' third filter stands in for their landlock domains.
/// `None` when every part is in force.
///
/// References:
/// - ADR-1200 D15, "Nothing on screen, and the log says why": one line
///   once a boot, with the reasons it tells apart.
/// - ADR-1101 D9: the workers' third filter on a kernel without landlock.
fn sandbox_line(hello: &Hello) -> Option<String> {
    use vtium_image::decode::wire::{NoLandlock, NoSeccomp};
    if hello.no_seccomp == Some(NoSeccomp::Off) && hello.no_landlock == Some(NoLandlock::Off) {
        return Some(
            "The image helper runs without its seccomp filter, and its workers without their landlock domains, because the setting helper-sandbox=off turns them off. An image that took over the image helper could open network connections and change any file that the image helper's account may change.".to_owned(),
        );
    }
    // the decoder refuses a hello whose filter is missing without a
    // reason, or whose landlock version is 0 without one, so each reason
    // here is the one the helper gave
    let seccomp = hello.no_seccomp.map(|why| {
        let why = match why {
            NoSeccomp::Off => "the setting helper-sandbox=off turns it off",
            NoSeccomp::NoTable => "vtium has no seccomp filter for this machine's architecture",
            NoSeccomp::NotBuilt => {
                "this kernel is new enough to offer seccomp but was built without it"
            }
            NoSeccomp::NoFilters => {
                "this kernel was built with seccomp but without seccomp filters"
            }
            NoSeccomp::TooOld => {
                "this kernel is older than Linux 3.8, too old for vtium's seccomp filters"
            }
        };
        format!("The image helper runs without its seccomp filter, because {why}.")
    });
    let landlock = hello.no_landlock.map(|why| {
        let why = match why {
            NoLandlock::Off => "the setting helper-sandbox=off turns them off",
            NoLandlock::TooOld => "this kernel is older than Linux 5.13, which added landlock",
            NoLandlock::NotEnabled => {
                "this kernel did not turn landlock on at boot, as when the boot's lsm= list leaves it out"
            }
            NoLandlock::NotBuilt => "this kernel was built without landlock",
        };
        let stands_in = if hello.seccomp {
            "A third seccomp filter in each worker refuses opening and removing files in their place."
        } else {
            "No filter takes their place."
        };
        format!("The image helper's workers run without their landlock domains, because {why}. {stands_in}")
    });
    let lines: Vec<String> = seccomp.into_iter().chain(landlock).collect();
    (!lines.is_empty()).then(|| lines.join(" "))
}

/// The helper's reason, every byte outside printable ASCII as `?`, at
/// debug level: a program can make a failure on every command.
fn log_reason(id: u64, header: &ReplyHeader) {
    if header.reason.is_empty() {
        return;
    }
    let text: String = header
        .reason
        .iter()
        .map(|&b| {
            if (0x20..=0x7e).contains(&b) {
                char::from(b)
            } else {
                '?'
            }
        })
        .collect();
    log::debug!("image request {id}: {:?}: {text}", header.status);
}

/// The inline bytes in a memory file sealed against shrinking, growing,
/// writing and further seals, which the helper reads.
fn inline_file(bytes: &[u8]) -> std::io::Result<OwnedFd> {
    use rustix::fs::MemfdFlags;
    let flags = MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING;
    let fd = match rustix::fs::memfd_create("vtium-image-data", flags | MemfdFlags::NOEXEC_SEAL) {
        Err(rustix::io::Errno::INVAL) => rustix::fs::memfd_create("vtium-image-data", flags)?,
        other => other?,
    };
    let mut done = 0usize;
    while done < bytes.len() {
        match rustix::io::write(&fd, bytes.get(done..).unwrap_or_default()) {
            Ok(0) => return Err(std::io::Error::other("no space for the inline bytes")),
            Ok(n) => done = done.saturating_add(n),
            Err(rustix::io::Errno::INTR) => {}
            Err(e) => return Err(e.into()),
        }
    }
    rustix::fs::fcntl_add_seals(
        &fd,
        SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE | SealFlags::SEAL,
    )?;
    Ok(fd)
}

fn send_packet(socket: impl AsFd, packet: &[u8], fd: Option<&OwnedFd>) -> std::io::Result<()> {
    let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    let fds: Vec<BorrowedFd<'_>> = fd.iter().map(|f| f.as_fd()).collect();
    if !fds.is_empty() && !control.push(SendAncillaryMessage::ScmRights(&fds)) {
        return Err(std::io::Error::other("no room for the descriptor"));
    }
    let sent = rustix::net::sendmsg(
        socket,
        &[std::io::IoSlice::new(packet)],
        &mut control,
        SendFlags::NOSIGNAL | SendFlags::DONTWAIT,
    )?;
    if sent != packet.len() {
        return Err(std::io::Error::other("the request was sent in part"));
    }
    Ok(())
}

/// One packet from the helper, its descriptors already close-on-exec.
#[derive(Debug)]
struct Packet {
    bytes: Vec<u8>,
    fds: Vec<OwnedFd>,
}

/// Why no packet could be read.
#[derive(Debug, PartialEq, Eq)]
enum Broken {
    Closed,
    Failed,
    Truncated,
    Foreign,
}

impl Broken {
    fn text(&self) -> &'static str {
        match self {
            Self::Closed => "it closed its socket",
            Self::Failed => "its socket failed",
            Self::Truncated => "a truncated reply",
            Self::Foreign => "a reply with a control message other than descriptors",
        }
    }
}

/// The next packet, `None` while none is waiting. Every descriptor that
/// arrives is received, so any it should not carry is closed with it.
fn receive(socket: impl AsFd) -> Result<Option<Packet>, Broken> {
    let most = REPLY_FIXED
        .saturating_add(REPLY_FRAMES_MAX.saturating_mul(2))
        .saturating_add(REASON_MAX);
    let mut bytes = vec![0u8; most.saturating_add(1)];
    let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(REPLY_FDS))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    let got = loop {
        match rustix::net::recvmsg(
            &socket,
            &mut [IoSliceMut::new(&mut bytes)],
            &mut control,
            RecvFlags::DONTWAIT | RecvFlags::CMSG_CLOEXEC,
        ) {
            Ok(got) => break got,
            Err(rustix::io::Errno::INTR) => {}
            Err(rustix::io::Errno::AGAIN) => return Ok(None),
            // the helper closed its end with a request unread, as one that
            // idles out just as the request goes out does; its exit status
            // says whether that was honest
            Err(rustix::io::Errno::CONNRESET) => return Err(Broken::Closed),
            Err(_) => return Err(Broken::Failed),
        }
    };
    let mut fds = Vec::new();
    let mut foreign = false;
    for message in control.drain() {
        match message {
            RecvAncillaryMessage::ScmRights(received) => fds.extend(received),
            _ => foreign = true,
        }
    }
    if got
        .flags
        .intersects(ReturnFlags::TRUNC | ReturnFlags::CTRUNC)
    {
        return Err(Broken::Truncated);
    }
    if foreign {
        return Err(Broken::Foreign);
    }
    if got.bytes == 0 && fds.is_empty() {
        return Err(Broken::Closed);
    }
    bytes.truncate(got.bytes);
    Ok(Some(Packet { bytes, fds }))
}

/// The picture of an OK reply in `fd`, checked before any of it is read: a
/// size within `max-graphic-size` and the budget, and the size the request
/// described if it did; for an animated GIF's frames, those past the first
/// within the request's `max_frame_bytes`; a regular file; its seals,
/// before its size, since until the shrink seal holds the helper could cut
/// the file under a mapping and fault the root process; then exactly the
/// frame count times a picture's bytes. Nothing is
/// read here: the handler reads the file once, after it has reserved the
/// picture's bytes ([`MemoryFile`]).
fn checked_file(
    fd: OwnedFd,
    header: &ReplyHeader,
    request: &Request,
) -> Result<ReplyFile, &'static str> {
    let (w, h) = (header.width, header.height);
    if w == 0 || h == 0 || w > request.largest.0 || h > request.largest.1 {
        return Err("a reply's size is outside max-graphic-size");
    }
    if request
        .pixels
        .is_some_and(|d| (d.width, d.height) != (w, h))
    {
        return Err("a reply of another size than the pixels the request described");
    }
    if !(request.png || request.any_image || request.pixels.is_some()) {
        return Err("a picture the request did not ask for");
    }
    let frame = u64::from(w)
        .checked_mul(u64::from(h))
        .and_then(|n| n.checked_mul(4))
        .filter(|&n| n <= request.largest_bytes)
        .ok_or("a reply larger than the budget")?;
    let count = u64::try_from(header.delays.len().max(1)).unwrap_or(u64::MAX);
    let past = frame
        .checked_mul(count.saturating_sub(1))
        .ok_or("frames larger than the frame allowance")?;
    if past > request.frames.map_or(0, |l| l.max_frame_bytes) {
        return Err("frames larger than the frame allowance");
    }
    let bytes = frame
        .checked_mul(count)
        .ok_or("frames larger than the frame allowance")?;
    let len = usize::try_from(bytes).map_err(|_| "a reply larger than the budget")?;
    let st = rustix::fs::fstat(&fd).map_err(|_| "a reply's descriptor cannot be read")?;
    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile {
        return Err("a reply's descriptor is not a regular file");
    }
    let seals = rustix::fs::fcntl_get_seals(&fd).map_err(|_| "a reply's file is no memory file")?;
    let mut need = SealFlags::SHRINK | SealFlags::GROW;
    if !header.pooled {
        // F_SEAL_FUTURE_WRITE does not stand in for F_SEAL_WRITE
        need |= SealFlags::WRITE;
    }
    if !seals.contains(need) {
        return Err("a reply's memory file is not sealed");
    }
    let st = rustix::fs::fstat(&fd).map_err(|_| "a reply's descriptor cannot be read")?;
    if u64::try_from(st.st_size).ok() != Some(bytes) {
        return Err("a reply's memory file is not exactly the size asked for");
    }
    Ok(ReplyFile::new(MemoryFile {
        fd,
        len,
        pooled: header.pooled,
    }))
}

/// A reply's checked memory file, read once when the handler asks: a
/// sealed one out of a read-only mapping, a pooled one with `pread` in
/// runs of [`RUN`] bytes through a buffer on the stack, so neither way
/// allocates anything the size of the picture.
#[derive(Debug)]
struct MemoryFile {
    fd: OwnedFd,
    len: usize,
    pooled: bool,
}

/// The bytes of a pooled file each `pread` run takes: a whole number of
/// pixels.
const RUN: usize = 16 << 10;

impl ReplySource for MemoryFile {
    fn bytes(&self) -> u64 {
        u64::try_from(self.len).unwrap_or(u64::MAX)
    }

    fn read(&self, sink: &mut dyn FnMut(&[u8])) -> bool {
        let read = if self.pooled {
            read_runs(&self.fd, self.len, sink)
        } else {
            read_mapped(&self.fd, self.len, sink)
        };
        if let Err(why) = &read {
            log::debug!("an image reply's memory file cannot be read whole: {why}");
        }
        read.is_ok()
    }
}

/// `len` bytes of `fd` from its start, with `pread`, handed to `sink` a
/// full run at a time; fewer fails.
fn read_runs(fd: &OwnedFd, len: usize, sink: &mut dyn FnMut(&[u8])) -> std::io::Result<()> {
    let mut run = [0u8; RUN];
    let mut done = 0usize;
    while done < len {
        let want = RUN.min(len.saturating_sub(done));
        let mut filled = 0usize;
        while filled < want {
            let at = u64::try_from(done.saturating_add(filled)).unwrap_or(u64::MAX);
            let into = run.get_mut(filled..want).unwrap_or_default();
            match rustix::io::pread(fd, into, at) {
                Ok(0) => return Err(std::io::Error::other("a short read")),
                Ok(n) => filled = filled.saturating_add(n),
                Err(rustix::io::Errno::INTR) => {}
                Err(e) => return Err(e.into()),
            }
        }
        sink(run.get(..want).unwrap_or_default());
        done = done.saturating_add(want);
    }
    Ok(())
}

/// The `len` bytes of the sealed memory file `fd`, handed to `sink` out of
/// a private, read-only mapping of it.
fn read_mapped(fd: &OwnedFd, len: usize, sink: &mut dyn FnMut(&[u8])) -> std::io::Result<()> {
    use rustix::mm::{MapFlags, ProtFlags};
    #[expect(
        unsafe_code,
        reason = "a read-only mapping of a sealed memory file, to copy it"
    )]
    // SAFETY: the kernel chooses the address, so the mapping overlaps no
    // memory Rust owns; the file is sealed against shrinking, which was
    // checked before its size, so every byte of the `len` mapped stays
    // backed until the mapping is undone below.
    let ptr = unsafe {
        rustix::mm::mmap(
            std::ptr::null_mut(),
            len,
            ProtFlags::READ,
            MapFlags::PRIVATE,
            fd,
            0,
        )?
    };
    #[expect(unsafe_code, reason = "the mapped bytes, read once")]
    // SAFETY: `ptr` starts a readable mapping of `len` bytes, alive until
    // the munmap below, and nothing writes through it; the sealed file
    // cannot be written either.
    sink(unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) });
    #[expect(unsafe_code, reason = "undoing the mapping made above")]
    // SAFETY: `ptr` and `len` are the mapping made above, and `sink` has
    // returned, so nothing holds the slice of it.
    let _ = unsafe { rustix::mm::munmap(ptr, len) };
    Ok(())
}
