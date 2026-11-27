//! The drawer's start: the broker forks, the child drops root, and the
//! child runs vtium's own binary again as the drawer.
//!
//! The broker resolves the account `vtium` and opens `/dev/null` before
//! the fork, so the child allocates nothing and takes no lock. Between the
//! fork and the exec the child makes these calls, in this order:
//!
//! 1. It starts a session of its own with `setsid`, so it has no
//!    controlling terminal. A root run by hand, from a root login or
//!    through sudo, would otherwise leave the drawer in the root shell's
//!    session. The drawer could then open `/dev/tty` and push keystrokes
//!    into the root shell with `TIOCSTI`. When that terminal is a VT, the
//!    kernel would also let the drawer make the VT calls that only the
//!    broker makes.
//! 2. It puts `/dev/null` on its standard input and its end of the socket
//!    on descriptor 3, and marks every descriptor from 4 up close-on-exec.
//!    Its standard output and standard error stay the broker's, so that
//!    its log reaches the journal.
//! 3. It empties the capability bounding set, while it still holds the
//!    capability that this needs.
//! 4. It sets its supplementary groups, then its real, effective and saved
//!    group ids, then its user ids, to the account's.
//! 5. It reads the ids back, and stops the start when any differs.
//! 6. It asks the kernel to send it `SIGKILL` when the broker dies. The
//!    kernel clears that request at a change of credentials, so the child
//!    makes it after step 4. When the broker is already gone, the child
//!    stops the start.
//! 7. It clears its ambient, effective, permitted and inheritable
//!    capabilities.
//! 8. It sets no-new-privileges, so no program it runs gains privilege.
//!
//! After the exec, the drawer makes itself not dumpable ([`begin`]),
//! because the kernel makes a process dumpable again at an exec that
//! leaves its ids equal, and a dumpable drawer could be traced by another
//! process of the account `vtium`.
//!
//! References:
//! - ADR-1400 D4, "The drawer's setup": the steps, the account, the death
//!   signal after `setresuid` and not dumpable after the exec.
//! - ADR-1400 D2: the helper's setup, whose order this follows, `setsid`
//!   and `/dev/null` first among them.

use std::ffi::{CString, OsString};
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use rustix::process::{Gid, Pid, PidfdFlags, Uid};

use super::protocol::{Link, Request};

/// The system account the drawer runs as.
pub const ACCOUNT: &str = "vtium";

/// The word on the drawer's command line, before vtium's own arguments,
/// that tells vtium it runs as the drawer.
pub const DRAWER: &str = "drawer";

/// An account, resolved before the fork: its user id, its group id, and
/// its supplementary groups, which only a broker running as root sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub uid: u32,
    pub gid: u32,
    pub groups: Option<Vec<u32>>,
}

/// The account `name` through the name service, with its supplementary
/// groups, or `None` when it does not exist.
pub fn lookup(name: &str) -> Option<Account> {
    let cname = CString::new(name).ok()?;
    let (uid, gid) = passwd(&cname)?;
    let groups = group_list(&cname, gid)?;
    Some(Account {
        uid,
        gid,
        groups: Some(groups),
    })
}

/// The home folder that the passwd entry of the account `name` names, or
/// `None` when the account does not exist.
pub fn home(name: &str) -> Option<PathBuf> {
    let cname = CString::new(name).ok()?;
    entry(&cname).map(|(_, _, home)| home)
}

/// The user and group id of the account `name`, through `getpwnam_r(3)`,
/// its buffer grown until the entry fits.
fn passwd(name: &CString) -> Option<(u32, u32)> {
    entry(name).map(|(uid, gid, _)| (uid, gid))
}

/// The user id, the group id and the home folder of the account `name`,
/// through `getpwnam_r(3)`, its buffer grown until the entry fits.
fn entry(name: &CString) -> Option<(u32, u32, PathBuf)> {
    let mut size = 16_384usize;
    loop {
        let mut buf: Vec<libc::c_char> = vec![0; size];
        #[expect(unsafe_code, reason = "a zeroed passwd, which getpwnam_r fills")]
        // SAFETY: `passwd` is plain old data, for which all zeroes is a
        // valid value of every field.
        let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
        let mut res: *mut libc::passwd = std::ptr::null_mut();
        #[expect(
            unsafe_code,
            reason = "getpwnam_r(3), the name service's account lookup"
        )]
        // SAFETY: every pointer is a live local or the buffer of the
        // length given; the answer points into that buffer, which this
        // function reads only through `pw`'s two integers and its home
        // folder's string, while the buffer lives.
        let r = unsafe {
            libc::getpwnam_r(
                name.as_ptr(),
                &mut pw,
                buf.as_mut_ptr(),
                buf.len(),
                &mut res,
            )
        };
        if r == libc::ERANGE && size < 1 << 22 {
            size = size.saturating_mul(2);
            continue;
        }
        if r != 0 || res.is_null() {
            return None;
        }
        let home = if pw.pw_dir.is_null() {
            PathBuf::from("/")
        } else {
            #[expect(unsafe_code, reason = "the home folder's C string in the buffer")]
            // SAFETY: getpwnam_r succeeded, so `pw_dir` points at a string
            // that ends in a NUL inside `buf`, which lives until the end of
            // this function.
            let dir = unsafe { std::ffi::CStr::from_ptr(pw.pw_dir) };
            PathBuf::from(std::ffi::OsStr::from_bytes(dir.to_bytes()))
        };
        return Some((pw.pw_uid, pw.pw_gid, home));
    }
}

/// The groups of the account `name`, its own group `gid` among them,
/// through `getgrouplist(3)`.
fn group_list(name: &CString, gid: u32) -> Option<Vec<u32>> {
    let mut n: libc::c_int = 32;
    loop {
        let mut list = vec![0 as libc::gid_t; usize::try_from(n).ok()?];
        let mut got = n;
        #[expect(
            unsafe_code,
            reason = "getgrouplist(3), the name service's group lookup"
        )]
        // SAFETY: `list` holds `got` gids, which getgrouplist writes at
        // most, and it writes the count it needs through `got`.
        let r = unsafe { libc::getgrouplist(name.as_ptr(), gid, list.as_mut_ptr(), &mut got) };
        if r >= 0 {
            list.truncate(usize::try_from(got).ok()?);
            list.sort_unstable();
            list.dedup();
            return Some(list);
        }
        if got <= n || got > 65_536 {
            return None;
        }
        n = got;
    }
}

/// The program the broker starts as the drawer, its arguments, and its
/// environment. With `clear_env`, the drawer's environment is `env` alone.
/// Without it, the drawer inherits the broker's environment, less the
/// variables through which systemd talks to the broker, with `env` set on
/// top, as a test's stand-in for the drawer does.
#[derive(Clone, Debug)]
pub struct Program {
    pub path: PathBuf,
    pub arg0: OsString,
    pub args: Vec<OsString>,
    pub clear_env: bool,
    pub env: Vec<(OsString, OsString)>,
}

/// vtium's cache folder, where systemd's `CACHE_DIRECTORY` names none.
pub const CACHE: &str = "/var/cache/vtium";

/// The folder inside vtium's cache folder that holds the GPU driver's
/// shader cache, which only the account that draws can read and write.
const GPU_CACHE: &str = "gpu";

/// The variables of the broker's environment that the drawer keeps, by
/// their full names. Every other variable of the broker's stays out of
/// the drawer's environment, so that what a root shell or sudo leaves
/// there, such as the shell's desktop session bus, its SSH agent, its
/// display, sudo's record of who ran it or a preloaded library, never
/// reaches the drawer, which runs as another account. Each entry stays
/// for this reason:
///
/// - `PATH`, for any program that a library the drawer loads looks up.
/// - `TERM`, which names the terminal vtium started from.
/// - `LANG` and `LANGUAGE`, with the `LC_` variables below: the locale,
///   which vtium reads for compose and which the C library reads.
/// - `TMPDIR`, which vtium reads for the image helper's rule on temporary
///   files.
/// - `RUST_BACKTRACE`, so that a panic of the drawer writes its backtrace
///   to the log when the administrator asked for one.
/// - `XCOMPOSEFILE` and `XLOCALEDIR`, which libxkbcommon reads for the
///   compose table, with the `XKB_` variables below.
/// - `DRI_PRIME`, with which Mesa chooses the GPU on a machine that has
///   two.
const DRAWER_KEEPS: [&str; 9] = [
    "PATH",
    "TERM",
    "LANG",
    "LANGUAGE",
    "TMPDIR",
    "RUST_BACKTRACE",
    "XCOMPOSEFILE",
    "XLOCALEDIR",
    "DRI_PRIME",
];

/// The starts of the names of the variables of the broker's environment
/// that the drawer keeps:
///
/// - `LC_`, the locale's categories.
/// - `XKB_`, the folders where libxkbcommon finds its keyboard layouts.
/// - `MESA_`, `GALLIUM_`, `LIBGL_` and `LP_`: Mesa's settings, the
///   choice of its driver, and llvmpipe's, its driver that renders on the
///   CPU.
/// - `__GL_`, NVIDIA's driver's settings.
/// - `EGL_` and `__EGL_`, the EGL loader's and libglvnd's choice of
///   vendor library.
/// - `GBM_`, the choice of the GBM backend.
/// - `VK_`, the Vulkan loader's settings.
///
/// Some of these name a library that the drawer then loads. Only the
/// administrator who starts vtium sets them, and the drawer loads the
/// library as the account that draws, which gains nothing by it.
const DRAWER_KEEPS_PREFIX: [&str; 11] = [
    "LC_", "XKB_", "MESA_", "GALLIUM_", "LIBGL_", "LP_", "__GL_", "EGL_", "__EGL_", "GBM_", "VK_",
];

/// Whether the drawer keeps the variable `name` of the broker's
/// environment: [`DRAWER_KEEPS`] names it, or it starts with one of
/// [`DRAWER_KEEPS_PREFIX`].
fn kept(name: &std::ffi::OsStr) -> bool {
    let name = name.as_bytes();
    DRAWER_KEEPS.iter().any(|k| name == k.as_bytes())
        || DRAWER_KEEPS_PREFIX
            .iter()
            .any(|p| name.starts_with(p.as_bytes()))
}

/// The cache folder that systemd made for vtium's unit, the first that
/// `CACHE_DIRECTORY` names in `env`, the broker's environment, or else
/// [`CACHE`].
pub fn cache_root(env: &[(OsString, OsString)]) -> PathBuf {
    env.iter()
        .find(|(k, _)| k == "CACHE_DIRECTORY")
        .and_then(|(_, v)| v.as_bytes().split(|&b| b == b':').next())
        .filter(|first| first.starts_with(b"/"))
        .map_or_else(
            || PathBuf::from(CACHE),
            |first| PathBuf::from(std::ffi::OsStr::from_bytes(first)),
        )
}

/// Makes the folder for the GPU driver's shader cache inside `root`,
/// vtium's cache folder, for `account`, the account that draws, and gives
/// its path. `by` is the user id of the process that calls it, root's in
/// the broker.
///
/// vtium makes `root` first where it is missing, owned by `by` and
/// readable by everyone, and sets its mode after making it, because the
/// umask would otherwise narrow it. vtium then holds `root` open and works
/// inside it only through that descriptor. It refuses a `root` that is a
/// symbolic link, that `by` does not own, or that its group or other
/// accounts can write, since an account that can write to `root` could
/// put a `gpu` of its own choosing there between two of these steps. It
/// also refuses a `root` that other accounts cannot enter, since the
/// account that draws could not reach `gpu` through it.
///
/// The folder `gpu` inside `root` belongs to `account`, and only that
/// account can read or write it, so no other account reads what the
/// driver compiled. vtium opens `gpu` without following a symbolic link,
/// so a `gpu` that is a symbolic link or not a folder is refused, and the
/// owner and the mode that vtium sets go to no other folder. A `gpu` that
/// another account owns holds entries that account wrote, so vtium removes
/// it and makes it again, empty. Before it gives the folder to `account`,
/// vtium checks that the folder it holds open belongs to `by`, who just
/// made it, or to `account`.
///
/// References:
/// - ADR-1304 D20: the folder, its owner and mode, the symbolic link, and
///   the emptying when the account that draws changes.
/// - ADR-1400 D4, its note on the GPU driver's shader cache: the broker
///   makes the folder at each start.
pub fn gpu_cache(root: &Path, by: u32, account: &Account) -> io::Result<PathBuf> {
    use rustix::fs::{Mode, OFlags};
    use rustix::io::Errno;
    let refused = |what: String| io::Error::other(what);
    let made = match rustix::fs::mkdir(root, Mode::from_raw_mode(0o755)) {
        Ok(()) => true,
        Err(Errno::EXIST) => false,
        Err(e) => return Err(e.into()),
    };
    let folder_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let dir = match rustix::fs::open(root, folder_flags, Mode::empty()) {
        Ok(dir) => dir,
        Err(Errno::LOOP | Errno::NOTDIR) => {
            return Err(refused(format!(
                "{} is a symbolic link or not a folder",
                root.display()
            )));
        }
        Err(e) => return Err(e.into()),
    };
    // The umask has masked the mode that mkdir asked for, and a root shell
    // with umask 077 would make a folder that the account cannot enter.
    if made {
        rustix::fs::fchmod(&dir, Mode::from_raw_mode(0o755))?;
    }
    let st = rustix::fs::fstat(&dir)?;
    if st.st_uid != by {
        return Err(refused(format!(
            "{} belongs to user id {}, and vtium uses it only when user id {by} owns it",
            root.display(),
            st.st_uid
        )));
    }
    if st.st_mode & 0o022 != 0 {
        return Err(refused(format!(
            "other accounts can write to {}, so one of them could replace the folder {GPU_CACHE} inside it",
            root.display()
        )));
    }
    if st.st_mode & 0o001 == 0 {
        return Err(refused(format!(
            "other accounts cannot enter {}, so the account that draws cannot reach the folder {GPU_CACHE} inside it",
            root.display()
        )));
    }
    let gpu = root.join(GPU_CACHE);
    let make = || match rustix::fs::mkdirat(&dir, GPU_CACHE, Mode::from_raw_mode(0o700)) {
        Ok(()) | Err(Errno::EXIST) => Ok(()),
        Err(e) => Err(io::Error::from(e)),
    };
    // Opened without following a symbolic link, so the owner and the mode
    // that vtium sets below go to this folder and to no other.
    let open = || match rustix::fs::openat(&dir, GPU_CACHE, folder_flags, Mode::empty()) {
        Ok(folder) => Ok(folder),
        Err(Errno::LOOP | Errno::NOTDIR) => Err(refused(format!(
            "{} is a symbolic link or not a folder",
            gpu.display()
        ))),
        Err(e) => Err(io::Error::from(e)),
    };
    let folder = match open() {
        Err(e) if e.raw_os_error() == Some(Errno::NOENT.raw_os_error()) => {
            make()?;
            open()?
        }
        Err(e) => return Err(e),
        Ok(folder) if rustix::fs::fstat(&folder)?.st_uid != account.uid => {
            drop(folder);
            // `root` belongs to `by` and nobody else can write to it, so
            // the `gpu` that this path names is the one opened above.
            std::fs::remove_dir_all(&gpu)?;
            make()?;
            open()?
        }
        Ok(folder) => folder,
    };
    taken(&folder, by, account).map_err(|owner| {
        refused(format!(
            "{} belongs to user id {owner}, which is neither vtium's root process nor the account that draws",
            gpu.display()
        ))
    })?;
    rustix::fs::fchown(
        &folder,
        Some(Uid::from_raw(account.uid)),
        Some(Gid::from_raw(account.gid)),
    )?;
    rustix::fs::fchmod(&folder, Mode::from_raw_mode(0o700))?;
    Ok(gpu)
}

/// Checks that `folder`, the open `gpu` folder, belongs to `by`, the
/// process that made it, or to `account`, before vtium gives it to
/// `account`. Otherwise it gives the user id that owns the folder.
fn taken(folder: &OwnedFd, by: u32, account: &Account) -> Result<(), u32> {
    let owner = rustix::fs::fstat(folder).map_or(u32::MAX, |st| st.st_uid);
    if owner == by || owner == account.uid {
        Ok(())
    } else {
        Err(owner)
    }
}

impl Program {
    /// vtium's own binary, through `/proc/self/exe`, so the drawer is the
    /// same build as the broker even when a newer vtium has replaced the
    /// file since. It runs with [`DRAWER`], the number of the VT the
    /// broker holds, and then the broker's own arguments. The drawer
    /// takes the VT's number from there and finds no VT of its own, so
    /// both processes always work on the same VT.
    pub fn vtium(arg0: OsString, vt: u16, args: &[OsString]) -> Program {
        let mut all = vec![OsString::from(DRAWER), OsString::from(vt.to_string())];
        all.extend(args.iter().cloned());
        Program {
            path: PathBuf::from("/proc/self/exe"),
            arg0,
            args: all,
            clear_env: false,
            env: Vec::new(),
        }
    }

    /// Gives the drawer the environment of the account it runs as, named
    /// `name`, whose passwd entry names the home folder `home`, in place
    /// of `broker`, the broker's own environment. `HOME`, `USER` and
    /// `LOGNAME` name that account. Of `broker`, the drawer keeps only
    /// what [`kept`] lets through, so the folders of the user who started
    /// vtium and that user's session stay out. `gpu` is
    /// the folder for the GPU driver's shader cache, which each driver
    /// learns from its own variable: Mesa from `MESA_SHADER_CACHE_DIR` and
    /// NVIDIA's driver from `__GL_SHADER_DISK_CACHE_PATH`. Without the
    /// folder, both drivers' disk caches are off, so no driver tries to
    /// write into a folder the account cannot write. vtium sets these only
    /// for the drawer, and never in a session's environment.
    ///
    /// References:
    /// - ADR-1304 D20: the drivers' variables, and the cache turned off
    ///   where the folder cannot be made.
    pub fn as_account(
        &mut self,
        name: &str,
        home: &Path,
        gpu: Option<&Path>,
        broker: &[(OsString, OsString)],
    ) {
        let mut ours: Vec<(OsString, OsString)> = Vec::new();
        let mut set = |k: &str, v: &std::ffi::OsStr| ours.push((k.into(), v.to_owned()));
        set("HOME", home.as_os_str());
        set("USER", name.as_ref());
        set("LOGNAME", name.as_ref());
        match gpu {
            Some(folder) => {
                set("MESA_SHADER_CACHE_DIR", folder.as_os_str());
                set("__GL_SHADER_DISK_CACHE_PATH", folder.as_os_str());
            }
            None => {
                set("MESA_SHADER_CACHE_DISABLE", "true".as_ref());
                set("__GL_SHADER_DISK_CACHE", "0".as_ref());
            }
        }
        self.clear_env = true;
        let mut env: Vec<(OsString, OsString)> = broker
            .iter()
            .filter(|(k, _)| kept(k) && !ours.iter().any(|(o, _)| o == k))
            .cloned()
            .collect();
        env.extend(ours);
        self.env = env;
    }
}

/// A started drawer: its pid and its pidfd. The broker holds the pidfd
/// from the start, so it never signals or waits on a process it did not
/// start.
#[derive(Debug)]
pub struct Started {
    pub pid: Pid,
    pub pidfd: OwnedFd,
}

/// The environment variables through which systemd talks to the unit's
/// main process. The broker is that process, so the drawer never sees
/// them: it never tells systemd it is ready, and it takes no descriptor
/// from systemd's store.
const SYSTEMD_ONLY: [&str; 5] = [
    "NOTIFY_SOCKET",
    "LISTEN_FDS",
    "LISTEN_PID",
    "LISTEN_FDNAMES",
    "WATCHDOG_USEC",
];

/// Starts `program` as the drawer, as `account`, with `theirs`, the
/// drawer's end of the socket, on its descriptor 3. `root` says the broker
/// runs as root, so the bounding set must empty; a broker without root,
/// as in a test, cannot empty it and has no capability to keep from the
/// drawer.
pub fn spawn(
    program: &Program,
    account: &Account,
    theirs: OwnedFd,
    root: bool,
) -> io::Result<Started> {
    // Above 3, so the child's moves onto 0 and 3 are real moves that clear
    // the close-on-exec flag, and close-on-exec until then.
    let theirs = rustix::io::fcntl_dupfd_cloexec(&theirs, 4)?;
    let null = rustix::io::fcntl_dupfd_cloexec(
        rustix::fs::open(
            "/dev/null",
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?,
        4,
    )?;
    let mut command = Command::new(&program.path);
    command.arg0(&program.arg0).args(&program.args);
    if program.clear_env {
        command.env_clear();
    }
    for name in SYSTEMD_ONLY {
        command.env_remove(name);
    }
    command.envs(program.env.iter().map(|(k, v)| (k, v)));
    // built here, before the fork, so the child allocates nothing
    let groups: Option<Vec<Gid>> = account
        .groups
        .as_ref()
        .map(|g| g.iter().map(|&gid| Gid::from_raw(gid)).collect());
    let ids = (account.uid, account.gid);
    let parent = rustix::process::getpid();
    let mut system = System {
        socket: theirs.as_raw_fd(),
        null: null.as_raw_fd(),
    };
    #[expect(
        unsafe_code,
        reason = "the drawer's descriptors and credentials can only be set between fork and exec"
    )]
    // SAFETY: the hook runs in the forked child before exec, which has one
    // thread, and makes only raw system calls through rustix and libc,
    // which allocate nothing and take no lock. The account and its group
    // list were resolved and `/dev/null` opened before the fork.
    unsafe {
        command.pre_exec(move || setup(&mut system, ids, groups.as_deref(), parent, root));
    }
    vtium_platform::cloexec_above_stdio()?;
    let child = command.spawn()?;
    drop(theirs);
    drop(null);
    let pid = Pid::from_child(&child);
    // the child is not reaped before this, since nothing else waits on it
    let pidfd = rustix::process::pidfd_open(pid, PidfdFlags::empty())?;
    drop(child);
    Ok(Started { pid, pidfd })
}

/// Moves `null` onto descriptor 0 and the socket onto descriptor 3, which
/// clears their close-on-exec flags, and marks every descriptor from 4 up
/// close-on-exec, so the drawer inherits only `/dev/null` as its standard
/// input, the broker's standard output and standard error, and the socket.
fn descriptors(null: i32, socket: i32) -> io::Result<()> {
    for (from, onto) in [(null, 0), (socket, 3)] {
        #[expect(
            unsafe_code,
            reason = "dup2(2) onto the drawer's standard input and descriptor 3"
        )]
        // SAFETY: dup2 duplicates a descriptor this child holds onto
        // `onto`, replacing whatever `onto` held, and touches no memory.
        let moved = unsafe { libc::dup2(from, onto) };
        if moved != onto {
            return Err(io::Error::last_os_error());
        }
    }
    #[expect(unsafe_code, reason = "close_range(2), which rustix does not wrap")]
    // SAFETY: with CLOSE_RANGE_CLOEXEC, close_range closes nothing and
    // touches no memory; it sets the close-on-exec flag on each open
    // descriptor in the range. A kernel without the flag leaves it to the
    // marking the broker made before the fork.
    let _ = unsafe {
        libc::syscall(
            libc::SYS_close_range,
            4 as libc::c_uint,
            libc::c_uint::MAX,
            libc::CLOSE_RANGE_CLOEXEC,
        )
    };
    Ok(())
}

/// The calls the drawer's setup makes, so a test can follow their order
/// and answer for the kernel. The real calls are [`System`]'s.
pub(super) trait Calls {
    /// Starts a session of its own, which has no controlling terminal.
    fn new_session(&mut self) -> io::Result<()>;
    /// Puts `/dev/null` on standard input and the socket on descriptor 3,
    /// and marks the rest close-on-exec.
    fn descriptors(&mut self) -> io::Result<()>;
    /// Empties the capability bounding set. Without the capability to,
    /// that fails only when `required`.
    fn drop_bounding_set(&mut self, required: bool) -> io::Result<()>;
    fn set_groups(&mut self, groups: &[Gid]) -> io::Result<()>;
    fn set_gids(&mut self, gid: u32) -> io::Result<()>;
    fn set_uids(&mut self, uid: u32) -> io::Result<()>;
    /// The real, effective and saved user ids, then the group ids.
    fn read_ids(&mut self) -> io::Result<([u32; 3], [u32; 3])>;
    fn kill_at_parent_death(&mut self) -> io::Result<()>;
    fn parent(&mut self) -> Option<Pid>;
    fn clear_capabilities(&mut self) -> io::Result<()>;
    fn no_new_privs(&mut self) -> io::Result<()>;
}

/// The drawer's setup between fork and exec, in the order the module's
/// comment gives. `parent` is the broker's pid, read before the fork.
pub(super) fn setup(
    calls: &mut impl Calls,
    (uid, gid): (u32, u32),
    groups: Option<&[Gid]>,
    parent: Pid,
    root: bool,
) -> io::Result<()> {
    calls.new_session()?;
    calls.descriptors()?;
    calls.drop_bounding_set(root)?;
    if let Some(groups) = groups {
        calls.set_groups(groups)?;
    }
    calls.set_gids(gid)?;
    calls.set_uids(uid)?;
    let (uids, gids) = calls.read_ids()?;
    if uids != [uid; 3] || gids != [gid; 3] {
        // an error that allocates nothing, after the fork
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    calls.kill_at_parent_death()?;
    // A broker that died before the call above sent no signal, and the
    // child now has another parent.
    if calls.parent() != Some(parent) {
        return Err(io::Error::from_raw_os_error(libc::ESRCH));
    }
    calls.clear_capabilities()?;
    calls.no_new_privs()?;
    Ok(())
}

/// The real calls, with the descriptors of `/dev/null` and of the
/// drawer's end of the socket. Each is a raw system call, so it may run
/// between fork and exec.
struct System {
    socket: i32,
    null: i32,
}

/// The highest capability number the bounding set is emptied up to. A
/// kernel that knows fewer answers EINVAL past its last, which ends the
/// loop.
const CAPABILITY_LAST: libc::c_ulong = 63;

impl Calls for System {
    fn new_session(&mut self) -> io::Result<()> {
        rustix::process::setsid()?;
        Ok(())
    }

    fn descriptors(&mut self) -> io::Result<()> {
        descriptors(self.null, self.socket)
    }

    fn drop_bounding_set(&mut self, required: bool) -> io::Result<()> {
        for capability in 0..=CAPABILITY_LAST {
            #[expect(unsafe_code, reason = "prctl(2) with PR_CAPBSET_DROP")]
            // SAFETY: PR_CAPBSET_DROP reads its integer argument and
            // touches no memory of this process.
            let dropped = unsafe { libc::prctl(libc::PR_CAPBSET_DROP, capability, 0, 0, 0) };
            if dropped == 0 {
                continue;
            }
            let errno = io::Error::last_os_error();
            return match errno.raw_os_error() {
                // past the kernel's last capability
                Some(libc::EINVAL) if capability > 0 => Ok(()),
                Some(libc::EPERM) if !required => Ok(()),
                _ => Err(errno),
            };
        }
        Ok(())
    }

    fn set_groups(&mut self, groups: &[Gid]) -> io::Result<()> {
        Ok(rustix::thread::set_thread_groups(groups)?)
    }

    fn set_gids(&mut self, gid: u32) -> io::Result<()> {
        let g = Gid::from_raw(gid);
        Ok(rustix::thread::set_thread_res_gid(g, g, g)?)
    }

    fn set_uids(&mut self, uid: u32) -> io::Result<()> {
        let u = Uid::from_raw(uid);
        Ok(rustix::thread::set_thread_res_uid(u, u, u)?)
    }

    fn read_ids(&mut self) -> io::Result<([u32; 3], [u32; 3])> {
        let (mut r, mut e, mut s) = (0, 0, 0);
        let (mut rg, mut eg, mut sg) = (0, 0, 0);
        #[expect(
            unsafe_code,
            reason = "getresuid(2) and getresgid(2), which rustix does not wrap"
        )]
        // SAFETY: each call writes one id through each pointer, each a
        // live local of the right type.
        let read = unsafe {
            libc::getresuid(&mut r, &mut e, &mut s) == 0
                && libc::getresgid(&mut rg, &mut eg, &mut sg) == 0
        };
        if !read {
            return Err(io::Error::last_os_error());
        }
        Ok(([r, e, s], [rg, eg, sg]))
    }

    fn kill_at_parent_death(&mut self) -> io::Result<()> {
        Ok(rustix::process::set_parent_process_death_signal(Some(
            rustix::process::Signal::KILL,
        ))?)
    }

    fn parent(&mut self) -> Option<Pid> {
        rustix::process::getppid()
    }

    fn clear_capabilities(&mut self) -> io::Result<()> {
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

    fn no_new_privs(&mut self) -> io::Result<()> {
        Ok(rustix::thread::set_no_new_privs(true)?)
    }
}

/// The drawer's first steps after the exec: it makes itself not dumpable,
/// takes the broker's socket from descriptor 3, and tells the broker it
/// runs. vtium calls it once, at the start of a process that runs as the
/// drawer.
pub fn begin() -> io::Result<Link> {
    rustix::process::set_dumpable_behavior(rustix::process::DumpableBehavior::NotDumpable)?;
    let mut passed = vtium_platform::take_passed(1)?;
    let fd = passed
        .pop()
        .ok_or_else(|| io::Error::other("vtium's root process passed no socket on descriptor 3"))?;
    let link = Link::passed(fd)?;
    link.request(&Request::Ready)?;
    Ok(link)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    /// A stand-in for the kernel that records each call by name, and
    /// answers the id read-back and the parent as told.
    struct Record {
        steps: Vec<&'static str>,
        ids: ([u32; 3], [u32; 3]),
        parent: Option<Pid>,
    }

    impl Calls for Record {
        fn new_session(&mut self) -> io::Result<()> {
            self.steps.push("setsid");
            Ok(())
        }
        fn descriptors(&mut self) -> io::Result<()> {
            self.steps.push("descriptors");
            Ok(())
        }
        fn drop_bounding_set(&mut self, required: bool) -> io::Result<()> {
            self.steps.push(if required {
                "bounding set, required"
            } else {
                "bounding set"
            });
            Ok(())
        }
        fn set_groups(&mut self, _: &[Gid]) -> io::Result<()> {
            self.steps.push("setgroups");
            Ok(())
        }
        fn set_gids(&mut self, _: u32) -> io::Result<()> {
            self.steps.push("setresgid");
            Ok(())
        }
        fn set_uids(&mut self, _: u32) -> io::Result<()> {
            self.steps.push("setresuid");
            Ok(())
        }
        fn read_ids(&mut self) -> io::Result<([u32; 3], [u32; 3])> {
            self.steps.push("read back");
            Ok(self.ids)
        }
        fn kill_at_parent_death(&mut self) -> io::Result<()> {
            self.steps.push("PDEATHSIG");
            Ok(())
        }
        fn parent(&mut self) -> Option<Pid> {
            self.steps.push("getppid");
            self.parent
        }
        fn clear_capabilities(&mut self) -> io::Result<()> {
            self.steps.push("capabilities");
            Ok(())
        }
        fn no_new_privs(&mut self) -> io::Result<()> {
            self.steps.push("NO_NEW_PRIVS");
            Ok(())
        }
    }

    fn broker() -> Pid {
        Pid::from_raw(4242).unwrap()
    }

    fn record(ids: ([u32; 3], [u32; 3])) -> Record {
        Record {
            steps: Vec::new(),
            ids,
            parent: Some(broker()),
        }
    }

    fn groups() -> [Gid; 2] {
        [Gid::from_raw(970), Gid::from_raw(971)]
    }

    /// The setup makes its calls in the module's order: a session of its
    /// own first, then `/dev/null` and the socket on their descriptors, the
    /// bounding set while root still can, the groups and ids, the
    /// read-back, the death signal after `setresuid`, the check that the
    /// broker still lives, then the capabilities and no-new-privileges
    /// last.
    #[test]
    fn the_drawers_setup_runs_in_order() {
        let mut calls = record(([970; 3], [970; 3]));
        setup(&mut calls, (970, 970), Some(&groups()), broker(), true).unwrap();
        assert_eq!(
            calls.steps,
            [
                "setsid",
                "descriptors",
                "bounding set, required",
                "setgroups",
                "setresgid",
                "setresuid",
                "read back",
                "PDEATHSIG",
                "getppid",
                "capabilities",
                "NO_NEW_PRIVS",
            ]
        );
        // a broker without root sets no groups, and the bounding set may
        // stay as it is
        let mut calls = record(([1000; 3], [1000; 3]));
        setup(&mut calls, (1000, 1000), None, broker(), false).unwrap();
        assert_eq!(
            calls.steps[..4],
            ["setsid", "descriptors", "bounding set", "setresgid"]
        );
    }

    /// An id that reads back other than the account's stops the setup at
    /// once, before the drawer can run with it: any of the three user ids,
    /// or any of the three group ids.
    #[test]
    fn a_failed_read_back_stops_the_start() {
        for (uids, gids) in [
            ([0, 970, 970], [970; 3]),
            ([970, 0, 970], [970; 3]),
            ([970, 970, 0], [970; 3]),
            ([970; 3], [0, 970, 970]),
            ([970; 3], [970, 0, 970]),
            ([970; 3], [970, 970, 0]),
        ] {
            let mut calls = record((uids, gids));
            let e = setup(&mut calls, (970, 970), Some(&groups()), broker(), true).unwrap_err();
            assert_eq!(e.raw_os_error(), Some(libc::EPERM));
            assert_eq!(calls.steps.last(), Some(&"read back"), "{uids:?} {gids:?}");
        }
    }

    /// A broker that died before the child asked for the death signal
    /// stops the start, since the kernel will never send that signal.
    #[test]
    fn a_broker_gone_before_the_death_signal_stops_the_start() {
        let mut calls = record(([970; 3], [970; 3]));
        calls.parent = Some(Pid::from_raw(1).unwrap());
        let e = setup(&mut calls, (970, 970), None, broker(), true).unwrap_err();
        assert_eq!(e.raw_os_error(), Some(libc::ESRCH));
        assert_eq!(calls.steps.last(), Some(&"getppid"));
    }

    /// The drawer's command line is [`DRAWER`], the VT's number and then
    /// the broker's own arguments, run from `/proc/self/exe`.
    #[test]
    fn the_drawer_runs_vtiums_own_binary_with_the_brokers_arguments() {
        let program = Program::vtium("vtium".into(), 3, &["--vt=tty3".into()]);
        assert_eq!(program.path, PathBuf::from("/proc/self/exe"));
        assert_eq!(
            program.args,
            [OsString::from("drawer"), "3".into(), "--vt=tty3".into()]
        );
    }

    /// The user running the tests, as the drawer's account.
    fn me() -> Account {
        Account {
            uid: rustix::process::getuid().as_raw(),
            gid: rustix::process::getgid().as_raw(),
            groups: None,
        }
    }

    /// An empty folder of the test's own under the system's temporary
    /// folder, removed when the guard drops.
    struct Folder(PathBuf);

    impl Folder {
        fn new(name: &str) -> Folder {
            let dir =
                std::env::temp_dir().join(format!("vtium-broker-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Folder(dir)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Sets the process's umask to `mask` until the guard drops, then
    /// puts back the umask it found.
    struct Umask(rustix::fs::Mode);

    impl Umask {
        fn set(mask: u32) -> Umask {
            Umask(rustix::process::umask(rustix::fs::Mode::from_raw_mode(
                mask,
            )))
        }
    }

    impl Drop for Umask {
        fn drop(&mut self) {
            rustix::process::umask(self.0);
        }
    }

    /// `list` as the environment of a process, as `std::env::vars_os`
    /// gives it.
    fn os_env(list: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        list.iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect()
    }

    /// The environment `program` gives the drawer over a broker whose own
    /// environment is `broker`: what the drawer would see.
    fn seen(program: &Program, broker: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut env: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        if !program.clear_env {
            env.extend(
                broker
                    .iter()
                    .filter(|(k, _)| !SYSTEMD_ONLY.contains(k))
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
            );
        }
        for (k, v) in &program.env {
            env.insert(
                k.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            );
        }
        env.into_iter().collect()
    }

    /// The environment of a broker started from a root shell: root's own
    /// folders, the shell's desktop session, sudo's record of who ran it,
    /// systemd's socket, and what the drawer needs.
    const ROOT_SHELL: [(&str, &str); 25] = [
        ("HOME", "/root"),
        ("USER", "root"),
        ("LOGNAME", "root"),
        ("MAIL", "/var/mail/root"),
        ("XDG_RUNTIME_DIR", "/run/user/0"),
        ("XDG_CONFIG_HOME", "/root/.config"),
        ("XDG_CACHE_HOME", "/root/.cache"),
        ("XDG_DATA_HOME", "/root/.local/share"),
        ("XDG_STATE_HOME", "/root/.local/state"),
        ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
        ("SSH_AUTH_SOCK", "/run/user/1000/ssh-agent.socket"),
        ("DISPLAY", ":0"),
        ("WAYLAND_DISPLAY", "wayland-0"),
        ("SUDO_USER", "someone"),
        ("SUDO_UID", "1000"),
        ("SUDO_COMMAND", "/usr/bin/vtium"),
        ("NOTIFY_SOCKET", "/run/systemd/notify"),
        ("LD_PRELOAD", "/root/preload.so"),
        ("PATH", "/usr/bin:/bin"),
        ("TERM", "linux"),
        ("LANG", "en_US.UTF-8"),
        ("LC_TIME", "C"),
        ("MESA_LOADER_DRIVER_OVERRIDE", "iris"),
        ("MESA_SHADER_CACHE_DIR", "/root/.cache/mesa_shader_cache"),
        ("VK_ICD_FILENAMES", "/usr/share/vulkan/icd.d/intel_icd.json"),
    ];

    /// The drawer runs with the environment of its own account: its home
    /// folder and its name, and only the variables of the broker's
    /// environment that it needs, such as the locale and the GPU driver's
    /// settings. Root's folders, the root shell's desktop session and
    /// sudo's record do not reach it. Each GPU driver finds its shader
    /// cache in the folder the broker made, and where the broker could
    /// not make one, each driver's disk cache is off.
    ///
    /// References:
    /// - ADR-1304 D20: the drivers' variables, and the cache turned off
    ///   where the folder cannot be made.
    #[test]
    fn the_drawer_runs_with_its_accounts_home_and_the_drivers_cache_folder() {
        let pairs = |list: &[(&str, &str)]| -> Vec<(String, String)> {
            list.iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        };
        let kept = [
            ("LANG", "en_US.UTF-8"),
            ("LC_TIME", "C"),
            ("MESA_LOADER_DRIVER_OVERRIDE", "iris"),
            ("PATH", "/usr/bin:/bin"),
            ("TERM", "linux"),
            ("VK_ICD_FILENAMES", "/usr/share/vulkan/icd.d/intel_icd.json"),
        ];
        let mut program = Program::vtium("vtium".into(), 3, &[]);
        program.as_account(
            "vtium",
            Path::new("/"),
            Some(Path::new("/var/cache/vtium/gpu")),
            &os_env(&ROOT_SHELL),
        );
        let mut want = pairs(&kept);
        want.extend(pairs(&[
            ("HOME", "/"),
            ("LOGNAME", "vtium"),
            ("MESA_SHADER_CACHE_DIR", "/var/cache/vtium/gpu"),
            ("USER", "vtium"),
            ("__GL_SHADER_DISK_CACHE_PATH", "/var/cache/vtium/gpu"),
        ]));
        want.sort();
        assert_eq!(seen(&program, &ROOT_SHELL), want);
        let mut program = Program::vtium("vtium".into(), 3, &[]);
        program.as_account("vtium", Path::new("/"), None, &os_env(&ROOT_SHELL));
        let mut want = pairs(&kept);
        want.extend(pairs(&[
            ("HOME", "/"),
            ("LOGNAME", "vtium"),
            // the root shell's own choice of folder, which the account
            // could not write anyway, and which the driver ignores while
            // its cache is off
            ("MESA_SHADER_CACHE_DIR", "/root/.cache/mesa_shader_cache"),
            ("MESA_SHADER_CACHE_DISABLE", "true"),
            ("USER", "vtium"),
            ("__GL_SHADER_DISK_CACHE", "0"),
        ]));
        want.sort();
        assert_eq!(seen(&program, &ROOT_SHELL), want);
    }

    /// The drawer that the broker starts sees the environment that
    /// [`Program::as_account`] gives it, and nothing of the broker's own
    /// environment besides: a shell run as the drawer writes its
    /// environment to a file. The broker's environment is the test's
    /// [`ROOT_SHELL`], so the test does not depend on the environment it
    /// runs in, and none of the test runner's own variables may reach the
    /// shell either.
    #[test]
    fn a_started_drawer_sees_its_accounts_environment() {
        use rustix::net::{AddressFamily, SocketFlags, SocketType, socketpair};
        let folder = Folder::new("drawer-env");
        let out = folder.0.join("env");
        let mut program = Program {
            path: PathBuf::from("/bin/sh"),
            arg0: "sh".into(),
            args: vec![
                "-c".into(),
                "env > \"$1\"".into(),
                "sh".into(),
                out.clone().into(),
            ],
            clear_env: false,
            env: Vec::new(),
        };
        program.as_account(
            "vtium-test",
            Path::new("/home-of-the-drawer"),
            Some(Path::new("/cache-of-the-drawer")),
            &os_env(&ROOT_SHELL),
        );
        let (_ours, theirs) = socketpair(
            AddressFamily::UNIX,
            SocketType::STREAM,
            SocketFlags::CLOEXEC,
            None,
        )
        .unwrap();
        let started = spawn(&program, &me(), theirs, false).unwrap();
        use std::os::fd::AsFd;
        rustix::process::waitid(
            rustix::process::WaitId::PidFd(started.pidfd.as_fd()),
            rustix::process::WaitIdOptions::EXITED,
        )
        .unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for want in [
            "HOME=/home-of-the-drawer",
            "USER=vtium-test",
            "LOGNAME=vtium-test",
            "MESA_SHADER_CACHE_DIR=/cache-of-the-drawer",
            "__GL_SHADER_DISK_CACHE_PATH=/cache-of-the-drawer",
            "PATH=/usr/bin:/bin",
            "LANG=en_US.UTF-8",
            "LC_TIME=C",
            "MESA_LOADER_DRIVER_OVERRIDE=iris",
            "VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/intel_icd.json",
        ] {
            assert!(lines.contains(&want), "{want} in {lines:#?}");
        }
        // the variables that the shell itself sets
        let shells = ["PWD", "SHLVL", "_", "OLDPWD"];
        let set = [
            "HOME",
            "USER",
            "LOGNAME",
            "MESA_SHADER_CACHE_DIR",
            "__GL_SHADER_DISK_CACHE_PATH",
        ];
        for line in &lines {
            let name = line.split('=').next().unwrap_or(line);
            assert!(
                set.contains(&name) || shells.contains(&name) || kept(name.as_ref()),
                "{name} reached the drawer: {lines:#?}"
            );
        }
        for (name, _) in ROOT_SHELL {
            if !set.contains(&name) && !kept(name.as_ref()) {
                assert!(
                    !lines.iter().any(|l| l.starts_with(&format!("{name}="))),
                    "{name} in {lines:#?}"
                );
            }
        }
    }

    /// The broker makes the folder for the GPU driver's shader cache inside
    /// vtium's cache folder, which it makes first where it is missing. The
    /// folder belongs to the account that draws, and only that account
    /// can read or write it. A second start keeps what the folder holds.
    /// A folder that is a symbolic link is not used.
    ///
    /// References:
    /// - ADR-1304 D20: the folder, its owner and mode, and the symbolic link.
    #[test]
    fn the_shader_cache_folder_belongs_to_the_account_that_draws() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        // a umask as strict as an administrator's root shell may set, so
        // the modes come from vtium and not from the runner's umask
        let _umask = Umask::set(0o077);
        let folder = Folder::new("gpu-cache");
        let root = folder.0.join("vtium");
        let gpu = gpu_cache(&root, me().uid, &me()).unwrap();
        assert_eq!(gpu, root.join("gpu"));
        let made = std::fs::symlink_metadata(&gpu).unwrap();
        assert!(made.is_dir());
        assert_eq!(made.permissions().mode() & 0o7777, 0o700);
        assert_eq!((made.uid(), made.gid()), (me().uid, me().gid));
        assert_eq!(
            std::fs::metadata(&root).unwrap().permissions().mode() & 0o7777,
            0o755
        );
        std::fs::write(gpu.join("entry"), "compiled").unwrap();
        std::fs::set_permissions(&gpu, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(gpu_cache(&root, me().uid, &me()).unwrap(), gpu);
        assert!(gpu.join("entry").exists(), "the cache kept");
        assert_eq!(
            std::fs::metadata(&gpu).unwrap().permissions().mode() & 0o7777,
            0o700
        );
    }

    /// A `gpu` that is a symbolic link could point the owner and the mode
    /// that the broker sets at any folder, so the broker refuses it and
    /// leaves the folder it points at as it was: its mode, its owner and
    /// what it holds.
    ///
    /// References:
    /// - ADR-1304 D20: the symbolic link.
    #[test]
    fn a_shader_cache_folder_that_is_a_symbolic_link_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let folder = Folder::new("gpu-cache-link");
        let target = folder.0.join("elsewhere");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("entry"), "kept").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o750)).unwrap();
        let root = folder.0.join("vtium");
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&target, root.join("gpu")).unwrap();
        let e = gpu_cache(&root, me().uid, &me()).unwrap_err();
        assert!(e.to_string().contains("symbolic link"), "{e}");
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o7777,
            0o750
        );
        assert_eq!(
            std::fs::read_to_string(target.join("entry")).unwrap(),
            "kept"
        );
        assert!(
            std::fs::symlink_metadata(root.join("gpu"))
                .unwrap()
                .is_symlink()
        );
    }

    /// Only root may own vtium's cache folder or write to it. Another
    /// account that could write there could put a `gpu` of its own choosing
    /// there between the broker's checks, which the broker would then give to
    /// the account that draws. So the broker refuses a cache folder that
    /// another account owns, or that its group or other accounts can
    /// write, and makes nothing inside it. A test runs without root, so
    /// the test's own account stands for root, and another user id for
    /// the account that is not root.
    #[test]
    fn a_cache_folder_that_another_account_owns_or_can_write_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let folder = Folder::new("gpu-cache-open");
        let root = folder.0.join("vtium");
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        let e = gpu_cache(&root, me().uid.wrapping_add(1), &me()).unwrap_err();
        assert!(e.to_string().contains("belongs to"), "{e}");
        for mode in [0o775, 0o757, 0o777] {
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(mode)).unwrap();
            let e = gpu_cache(&root, me().uid, &me()).unwrap_err();
            assert!(e.to_string().contains("can write"), "{mode:o}: {e}");
        }
        assert!(!root.join("gpu").exists());
    }

    /// vtium's cache folder must let other accounts enter it, since the
    /// account that draws reaches `gpu` through it. The broker refuses an
    /// existing cache folder that other accounts cannot enter, and makes
    /// nothing inside it.
    #[test]
    fn a_cache_folder_that_others_cannot_enter_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let folder = Folder::new("gpu-cache-closed");
        let root = folder.0.join("vtium");
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let e = gpu_cache(&root, me().uid, &me()).unwrap_err();
        assert!(e.to_string().contains("cannot enter"), "{e}");
        assert!(!root.join("gpu").exists());
    }

    /// The broker gives the open `gpu` folder to the account that draws
    /// only when the folder belongs to the broker, which just made it, or
    /// already to that account. A folder that a third account owns is
    /// one the broker did not make.
    #[test]
    fn only_a_gpu_folder_of_the_broker_or_the_account_is_given_away() {
        let folder = Folder::new("gpu-cache-taken");
        let fd = rustix::fs::open(
            &folder.0,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY,
            rustix::fs::Mode::empty(),
        )
        .unwrap();
        let mine = me().uid;
        let other = |uid: u32| Account { uid, ..me() };
        assert_eq!(taken(&fd, mine, &other(mine.wrapping_add(1))), Ok(()));
        assert_eq!(taken(&fd, mine.wrapping_add(1), &me()), Ok(()));
        assert_eq!(
            taken(&fd, mine.wrapping_add(1), &other(mine.wrapping_add(2))),
            Err(mine)
        );
    }

    /// A `gpu` folder that another account owns holds what that account's
    /// driver wrote, so the broker empties it before the account that
    /// draws takes it. A test runs without root, so it stands for the
    /// change of account with an account it cannot give the folder to:
    /// the old entry is gone, and giving the folder away then fails.
    #[test]
    fn a_shader_cache_folder_of_another_account_is_emptied() {
        let folder = Folder::new("gpu-cache-other");
        let root = folder.0.join("vtium");
        let gpu = gpu_cache(&root, me().uid, &me()).unwrap();
        std::fs::create_dir_all(gpu.join("ab")).unwrap();
        std::fs::write(gpu.join("ab").join("entry"), "compiled").unwrap();
        let other = Account {
            uid: me().uid.wrapping_add(1),
            ..me()
        };
        let given = gpu_cache(&root, me().uid, &other);
        assert!(!gpu.join("ab").exists(), "the other account's entries");
        if rustix::process::geteuid().is_root() {
            assert!(given.is_ok(), "{given:?}");
        } else {
            assert!(given.is_err(), "{given:?}");
        }
    }

    /// The lookup finds an account every system has, with its own group
    /// among its groups, and finds none for a name nobody uses.
    #[test]
    fn the_lookup_finds_an_account_and_its_groups() {
        let root = lookup("root").unwrap();
        assert_eq!((root.uid, root.gid), (0, 0));
        assert!(root.groups.unwrap().contains(&0));
        assert_eq!(lookup("vtium-no-such-account"), None);
        assert_eq!(lookup("a\0b"), None);
    }
}
