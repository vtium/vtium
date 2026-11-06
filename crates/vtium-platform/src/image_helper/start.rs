//! Opening the helper's program once and starting each helper from it
//! (ADR-1101 D1, D3).
//!
//! vtium opens the program when it starts and checks the file it opened,
//! so the file checked is the file that runs, and starts every helper from
//! that open file through `/proc/self/fd`. A package upgrade that renames
//! a new program into place therefore reaches a console only when the
//! console starts again, and an old console never talks to a new helper.
//!
//! The account is resolved in vtium's own process before the fork, since
//! a child forked from a process with threads may call only what is safe
//! after a fork. The child then, in this order: starts a
//! session of its own with no controlling terminal; sets every signal to
//! its default and clears its mask; puts `/dev/null` on descriptors 0 to
//! 2 and its end of the socket on 3, every other descriptor marked
//! close-on-exec; sets its memory cap, the largest file it may write, a
//! core size of 0 and its descriptor limit; empties its capability bounding set; sets its
//! supplementary groups, its group ids and its user ids and reads them
//! back; clears every other capability set and sets no-new-privileges;
//! and starts the program in `/` with an empty environment. A logged-in
//! root's helper therefore runs as uid 0 with no capability.

use std::ffi::OsString;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use rustix::fs::{FileType, Mode, OFlags};
use rustix::net::{AddressFamily, SocketFlags, SocketType};
use rustix::process::{Gid, Pid, PidfdFlags, Resource, Rlimit, Uid};

use super::identity::Account;
use crate::bounds::IMAGE_HELPER_BASE;

/// The helper's program vtium is built with, `image-helper`'s default:
/// set at build time with `VTIUM_IMAGE`.
pub const PROGRAM: &str = match option_env!("VTIUM_IMAGE") {
    Some(path) => path,
    None => "/usr/bin/vtium-image",
};

/// The helper's program, opened once and checked.
#[derive(Clone, Debug)]
pub struct Program {
    path: PathBuf,
    file: Arc<OwnedFd>,
    /// Arguments before `--tmpdir`, for a stand-in a test starts.
    args: Vec<OsString>,
}

impl Program {
    /// Opens the program at `path` for the account vtium runs as.
    pub fn open(path: &Path) -> Result<Program, String> {
        Self::open_with(path, Vec::new())
    }

    /// [`Program::open`], started with `args` first: a test's stand-in.
    pub fn open_with(path: &Path, args: Vec<OsString>) -> Result<Program, String> {
        let me = rustix::process::geteuid().as_raw();
        Ok(Program {
            path: path.to_owned(),
            file: Arc::new(open_program(path, me)?),
            args,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Opens the program at `path` to start it from, close-on-exec and above
/// descriptor 3, where the child puts its socket, and checks the opened
/// file. It refuses a path that is not
/// absolute, a file that is not a regular file, one an account other than
/// root or `me` owns, one its group or others may write, and one that is
/// setuid or setgid: the font helper's rules.
pub fn open_program(path: &Path, me: u32) -> Result<OwnedFd, String> {
    let name = path.display();
    if !path.is_absolute() {
        return Err(format!(
            "the image helper's path {name} is not absolute, so vtium does not start it"
        ));
    }
    let opened = rustix::fs::open(path, OFlags::PATH | OFlags::CLOEXEC, Mode::empty())
        .map_err(|e| format!("cannot open the image helper {name}: {e}"))?;
    // above 3 as well as stdio: the child puts its socket on 3, which
    // would replace the program it is about to start from
    let file = rustix::io::fcntl_dupfd_cloexec(&opened, 4).map_err(|e| {
        format!("cannot move the image helper {name}'s descriptor above descriptor 3: {e}")
    })?;
    drop(opened);
    let st = rustix::fs::fstat(&file)
        .map_err(|e| format!("cannot read the image helper {name}'s owner and mode: {e}"))?;
    match refusal(
        FileType::from_raw_mode(st.st_mode),
        st.st_uid,
        st.st_mode,
        me,
    ) {
        None => Ok(file),
        Some(why) => Err(format!(
            "the image helper {name} {why}, so vtium does not start it"
        )),
    }
}

/// Why vtium refuses a program of `kind`, owned by `owner`, with `mode`,
/// when it runs as `me`.
pub(super) fn refusal(kind: FileType, owner: u32, mode: u32, me: u32) -> Option<String> {
    if kind != FileType::RegularFile {
        return Some("is not a regular file".to_owned());
    }
    if mode & 0o6000 != 0 {
        return Some(format!("is setuid or setgid (mode {:o})", mode & 0o7777));
    }
    if owner != 0 && owner != me {
        return Some(format!(
            "is owned by uid {owner}, which is neither root nor the account vtium runs as"
        ));
    }
    if mode & 0o022 != 0 {
        return Some(format!(
            "may be written by its group or by others (mode {:o})",
            mode & 0o7777
        ));
    }
    None
}

/// The most descriptors a helper and each of its workers may hold, soft
/// and hard: the helper holds about a dozen at its busiest, its four
/// first ones, a request's inline file, a reply file, a worker's channel
/// and a stream's channel and pool. A process without
/// `CAP_SYS_RESOURCE`, which no helper holds, cannot raise a hard limit,
/// so a worker taken over can hold no more sockets than this, each
/// buffering what the kernel's socket limits allow outside the memory
/// cap.
pub const HELPER_FILES: u64 = 64;

/// The memory cap of a helper started for requests of at most
/// `max_bytes`: three times them, for what it reads, an inflated copy and
/// the decoded picture, and [`IMAGE_HELPER_BASE`] for its code.
pub fn memory_cap(max_bytes: u64) -> u64 {
    max_bytes
        .saturating_mul(3)
        .saturating_add(u64::try_from(IMAGE_HELPER_BASE).unwrap_or(u64::MAX))
}

/// A running helper: its pidfd, held from the start so vtium never
/// signals a process it has reaped, its pid, which leads its process
/// group, and vtium's end of its socket.
#[derive(Debug)]
pub struct Started {
    pub pid: Pid,
    pub pidfd: OwnedFd,
    pub socket: OwnedFd,
}

/// Starts a helper from `program` as `account`, its memory cap set for
/// `max_bytes` and its largest file at `largest_file`, with vtium's
/// `TMPDIR` as its argument.
pub fn start(
    program: &Program,
    account: &Account,
    max_bytes: u64,
    largest_file: u64,
    tmpdir: Option<&Path>,
) -> Result<Started, String> {
    let (ours, theirs) = rustix::net::socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .map_err(|e| format!("cannot make the image helper's socket: {e}"))?;
    // above 3, so putting it on 3 in the child moves it, and close-on-exec
    // until then
    let theirs = rustix::io::fcntl_dupfd_cloexec(&theirs, 4)
        .map_err(|e| format!("cannot move the image helper's socket: {e}"))?;
    let mut command = Command::new(format!("/proc/self/fd/{}", program.file.as_raw_fd()));
    command
        .arg0(&program.path)
        .args(&program.args)
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = tmpdir {
        let mut arg = OsString::from("--tmpdir=");
        arg.push(dir);
        command.arg(arg);
    }
    // the largest memory file the helper fills is a reply of `max_bytes`
    // and an animated GIF's frames past the first (ADR-1101 D3), which the
    // caller adds, and the memory cap counts no memory file's pages
    let cap = memory_cap(max_bytes);
    // built here, before the fork, so the child allocates nothing
    let groups: Option<Vec<Gid>> = account
        .groups
        .as_ref()
        .map(|g| g.iter().map(|&gid| Gid::from_raw(gid)).collect());
    let ids = (account.uid, account.gid);
    let socket = theirs.as_raw_fd();
    #[expect(
        unsafe_code,
        reason = "the helper's session, descriptors, limits and ids can only be set between fork and exec"
    )]
    // SAFETY: the hook runs in the forked child before exec and makes
    // only async-signal-safe calls: raw syscalls through rustix and libc,
    // which allocate nothing and take no lock. The account was resolved,
    // and its group list built, before the fork, so the hook reads them
    // and allocates nothing. The child has one thread, as
    // `default_signals` requires.
    unsafe {
        command.pre_exec(move || child(socket, (cap, largest_file), ids, groups.as_deref()));
    }
    crate::fds::cloexec_before_spawn()
        .map_err(|e| format!("cannot start the image helper: {e}"))?;
    let child = command.spawn().map_err(|e| {
        format!(
            "cannot start the image helper {} through /proc/self/fd/{}: {e}; vtium starts the helper from the file it opened, so /proc must be mounted",
            program.path.display(),
            program.file.as_raw_fd()
        )
    })?;
    drop(theirs);
    let pid = Pid::from_child(&child);
    // the child is not reaped before this, since nothing waits on it
    let pidfd = rustix::process::pidfd_open(pid, PidfdFlags::empty())
        .map_err(|e| format!("cannot hold the image helper by its pidfd: {e}"))?;
    drop(child);
    Ok(Started {
        pid,
        pidfd,
        socket: ours,
    })
}

/// The child's setup between fork and exec, in ADR-1101 D3's order: `cap`
/// is its memory cap and `largest_file` the largest file it may write.
fn child(
    socket: i32,
    (cap, largest_file): (u64, u64),
    (uid, gid): (u32, u32),
    groups: Option<&[Gid]>,
) -> std::io::Result<()> {
    rustix::process::setsid()?;
    #[expect(unsafe_code, reason = "default dispositions, between fork and exec")]
    // SAFETY: the child has one thread and needs no handler or blocked
    // signal before exec, as `default_signals` requires.
    unsafe {
        crate::pty::default_signals()?;
    }
    #[expect(
        unsafe_code,
        reason = "dup2(2) onto descriptor 3, which the helper reads"
    )]
    // SAFETY: dup2 duplicates a descriptor this child holds onto 3,
    // replacing whatever 3 held, and touches no memory.
    let moved = unsafe { libc::dup2(socket, 3) };
    if moved != 3 {
        return Err(std::io::Error::last_os_error());
    }
    mark_cloexec_above(4);
    for (resource, limit) in [
        (Resource::As, cap),
        (Resource::Fsize, largest_file),
        (Resource::Core, 0),
        (Resource::Nofile, HELPER_FILES),
    ] {
        rustix::process::setrlimit(
            resource,
            Rlimit {
                current: Some(limit),
                maximum: Some(limit),
            },
        )?;
    }
    // while a vtium running as root still holds the capability to
    drop_bounding_set(uid == 0)?;
    if let Some(groups) = groups {
        rustix::thread::set_thread_groups(groups)?;
    }
    let (u, g) = (Uid::from_raw(uid), Gid::from_raw(gid));
    rustix::thread::set_thread_res_gid(g, g, g)?;
    rustix::thread::set_thread_res_uid(u, u, u)?;
    let (mut r, mut e, mut s) = (0, 0, 0);
    let (mut rg, mut eg, mut sg) = (0, 0, 0);
    #[expect(
        unsafe_code,
        reason = "getresuid(2) and getresgid(2), which rustix does not wrap"
    )]
    // SAFETY: each call writes one id through each pointer, each a live
    // local of the right type.
    let read = unsafe {
        libc::getresuid(&mut r, &mut e, &mut s) == 0
            && libc::getresgid(&mut rg, &mut eg, &mut sg) == 0
    };
    let ids = [r, e, s].iter().all(|&v| v == uid) && [rg, eg, sg].iter().all(|&v| v == gid);
    if !read || !ids {
        // an error that allocates nothing, after the fork
        return Err(std::io::Error::from_raw_os_error(libc::EPERM));
    }
    clear_capabilities()?;
    rustix::thread::set_no_new_privs(true)?;
    Ok(())
}

/// The highest capability number the bounding set is emptied up to; a
/// kernel that knows fewer answers EINVAL past its last, which ends the
/// loop.
const CAPABILITY_LAST: libc::c_ulong = 63;

/// Removes every capability from the calling thread's bounding set, so a
/// later start of a program as uid 0 grants it none. A process without
/// `CAP_SETPCAP`, a vtium not running as root, cannot and has no need to,
/// since it holds no capability to grant; for a helper that will run as
/// uid 0, `required`, that fails the start. Makes raw calls only, so it
/// may run between fork and exec.
fn drop_bounding_set(required: bool) -> std::io::Result<()> {
    for capability in 0..=CAPABILITY_LAST {
        #[expect(unsafe_code, reason = "prctl(2) with PR_CAPBSET_DROP")]
        // SAFETY: PR_CAPBSET_DROP reads its integer argument and touches no
        // memory of this process.
        let dropped = unsafe { libc::prctl(libc::PR_CAPBSET_DROP, capability, 0, 0, 0) };
        if dropped == 0 {
            continue;
        }
        let errno = std::io::Error::last_os_error();
        return match errno.raw_os_error() {
            // past the kernel's last capability
            Some(libc::EINVAL) if capability > 0 => Ok(()),
            Some(libc::EPERM) if !required => Ok(()),
            _ => Err(errno),
        };
    }
    Ok(())
}

/// Clears the calling thread's permitted, effective, inheritable and
/// ambient capability sets, which no process needs leave to do. Makes raw
/// calls only, so it may run between fork and exec.
fn clear_capabilities() -> std::io::Result<()> {
    use rustix::thread::{CapabilitySet, CapabilitySets};
    rustix::thread::clear_ambient_capability_set()?;
    rustix::thread::set_capabilities(
        None,
        CapabilitySets {
            effective: CapabilitySet::empty(),
            permitted: CapabilitySet::empty(),
            inheritable: CapabilitySet::empty(),
        },
    )?;
    Ok(())
}

/// Marks every descriptor from `first` up close-on-exec in the child,
/// where no other thread can open one meanwhile; a kernel without
/// `CLOSE_RANGE_CLOEXEC` leaves it to the marking vtium made before the
/// fork.
fn mark_cloexec_above(first: libc::c_uint) {
    #[expect(unsafe_code, reason = "close_range(2), which rustix does not wrap")]
    // SAFETY: with CLOSE_RANGE_CLOEXEC, close_range closes nothing and
    // touches no memory; it sets the close-on-exec flag on each open
    // descriptor in the range.
    let _ = unsafe {
        libc::syscall(
            libc::SYS_close_range,
            first,
            libc::c_uint::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    };
}

/// Kills the helper's whole process group, worker included, before it is
/// reaped: through the pidfd with the kernel's process-group flag where
/// the kernel has it, from Linux 6.9, and otherwise by signalling the
/// group's id, which cannot be reused while its leader is not yet reaped.
pub fn kill_group(started: &Started) {
    const PIDFD_SIGNAL_PROCESS_GROUP: libc::c_uint = 4;
    #[expect(
        unsafe_code,
        reason = "pidfd_send_signal(2) with a flag rustix does not offer"
    )]
    // SAFETY: the call reads only its descriptor and arguments, the
    // signal info is null, and SIGKILL to the group the pidfd leads is
    // the intent.
    let sent = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            started.pidfd.as_fd().as_raw_fd(),
            libc::SIGKILL,
            std::ptr::null::<libc::siginfo_t>(),
            PIDFD_SIGNAL_PROCESS_GROUP,
        )
    } == 0;
    if !sent {
        let _ = rustix::process::kill_process_group(started.pid, rustix::process::Signal::KILL);
    }
}

/// Whether the helper has exited, without reaping it, so its process
/// group's id stays its own until [`kill_group`] and [`reap`] are done.
pub fn exited(started: &Started) -> bool {
    use rustix::process::{WaitId, WaitIdOptions};
    let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
    !matches!(
        rustix::process::waitid(WaitId::PidFd(started.pidfd.as_fd()), options),
        Ok(None)
    )
}

/// Ends a helper that has exited or closed its socket with no request
/// out, and whether it exited with status 0, as a helper does when it
/// idles out or vtium closes its socket. Its process group is killed
/// first, which leaves the status of a helper already exiting as it was
/// and ends one that closed its socket and lives on, so the wait for it
/// that follows lasts only as long as its exit. It is then reaped through
/// its pidfd.
pub fn finish(started: &Started) -> bool {
    use rustix::process::{WaitId, WaitIdOptions};
    kill_group(started);
    loop {
        match rustix::process::waitid(WaitId::PidFd(started.pidfd.as_fd()), WaitIdOptions::EXITED) {
            Ok(Some(status)) => return status.exit_status() == Some(0),
            Err(rustix::io::Errno::INTR) => {}
            // nothing else reaps the helper, so its end is unknown
            Ok(None) | Err(_) => return false,
        }
    }
}

/// Reaps the helper through its pidfd; `false` while it has not exited.
pub fn reap(started: &Started, block: bool) -> bool {
    use rustix::process::{WaitId, WaitIdOptions};
    let mut options = WaitIdOptions::EXITED;
    if !block {
        options |= WaitIdOptions::NOHANG;
    }
    match rustix::process::waitid(WaitId::PidFd(started.pidfd.as_fd()), options) {
        Ok(Some(_)) => true,
        Ok(None) => false,
        // already reaped, or never ours
        Err(_) => true,
    }
}
