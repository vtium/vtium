//! The drawer's start: the broker forks, the child drops root, and the
//! child runs vtium's own binary again as the drawer.
//!
//! The broker resolves the account `vtium` before the fork, so the child
//! allocates nothing and takes no lock. Between the fork and the exec the
//! child puts its end of the socket on descriptor 3 and then makes these
//! calls, in this order:
//!
//! 1. It empties the capability bounding set, while it still holds the
//!    capability that this needs.
//! 2. It sets its supplementary groups, then its real, effective and saved
//!    group ids, then its user ids, to the account's.
//! 3. It reads the ids back, and stops the start when any differs.
//! 4. It asks the kernel to send it `SIGKILL` when the broker dies. The
//!    kernel clears that request at a change of credentials, so the child
//!    makes it after step 2. When the broker is already gone, the child
//!    stops the start.
//! 5. It clears its ambient, effective, permitted and inheritable
//!    capabilities.
//! 6. It sets no-new-privileges, so no program it runs gains privilege.
//!
//! After the exec, the drawer makes itself not dumpable ([`begin`]),
//! because the kernel makes a process dumpable again at an exec that
//! leaves its ids equal, and a dumpable drawer could be traced by another
//! process of the account `vtium`.
//!
//! References:
//! - ADR-1400 D4, "The drawer's setup": the steps, the account, the death
//!   signal after `setresuid` and not dumpable after the exec.
//! - ADR-1400 D2: the helper's setup, whose order this follows.

use std::ffi::{CString, OsString};
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
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

/// The user and group id of the account `name`, through `getpwnam_r(3)`,
/// its buffer grown until the entry fits.
fn passwd(name: &CString) -> Option<(u32, u32)> {
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
        // function reads only through `pw`'s two integers.
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
        return (r == 0 && !res.is_null()).then_some((pw.pw_uid, pw.pw_gid));
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

/// The program the broker starts as the drawer, its arguments, and the
/// variables it sets in the drawer's environment on top of the broker's.
#[derive(Clone, Debug)]
pub struct Program {
    pub path: PathBuf,
    pub arg0: OsString,
    pub args: Vec<OsString>,
    pub env: Vec<(OsString, OsString)>,
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
            env: Vec::new(),
        }
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
    // above 3, so the child's move onto 3 is a real move that clears the
    // close-on-exec flag, and close-on-exec until then
    let theirs = rustix::io::fcntl_dupfd_cloexec(&theirs, 4)?;
    let mut command = Command::new(&program.path);
    command.arg0(&program.arg0).args(&program.args);
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
    let socket = theirs.as_raw_fd();
    #[expect(
        unsafe_code,
        reason = "the drawer's descriptors and credentials can only be set between fork and exec"
    )]
    // SAFETY: the hook runs in the forked child before exec, which has one
    // thread, and makes only raw system calls through rustix and libc,
    // which allocate nothing and take no lock. The account and its group
    // list were resolved before the fork.
    unsafe {
        command.pre_exec(move || {
            onto_three(socket)?;
            setup(&mut System, ids, groups.as_deref(), parent, root)
        });
    }
    vtium_platform::cloexec_above_stdio()?;
    let child = command.spawn()?;
    drop(theirs);
    let pid = Pid::from_child(&child);
    // the child is not reaped before this, since nothing else waits on it
    let pidfd = rustix::process::pidfd_open(pid, PidfdFlags::empty())?;
    drop(child);
    Ok(Started { pid, pidfd })
}

/// Moves the socket onto descriptor 3, which clears its close-on-exec
/// flag, and marks every descriptor from 4 up close-on-exec, so the
/// drawer inherits only the standard streams and the socket.
fn onto_three(socket: i32) -> io::Result<()> {
    #[expect(
        unsafe_code,
        reason = "dup2(2) onto descriptor 3, which the drawer reads"
    )]
    // SAFETY: dup2 duplicates a descriptor this child holds onto 3,
    // replacing whatever 3 held, and touches no memory.
    let moved = unsafe { libc::dup2(socket, 3) };
    if moved != 3 {
        return Err(io::Error::last_os_error());
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

/// The real calls. Each is a raw system call, so it may run between fork
/// and exec.
struct System;

/// The highest capability number the bounding set is emptied up to. A
/// kernel that knows fewer answers EINVAL past its last, which ends the
/// loop.
const CAPABILITY_LAST: libc::c_ulong = 63;

impl Calls for System {
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

    /// The setup makes its calls in the module's order: the bounding set
    /// while root still can, the groups and ids, the read-back, the death
    /// signal after `setresuid`, the check that the broker still lives,
    /// then the capabilities and no-new-privileges last.
    #[test]
    fn the_drawers_setup_runs_in_order() {
        let mut calls = record(([970; 3], [970; 3]));
        setup(&mut calls, (970, 970), Some(&groups()), broker(), true).unwrap();
        assert_eq!(
            calls.steps,
            [
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
        assert_eq!(calls.steps[..2], ["bounding set", "setresgid"]);
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
