//! Check 8: whether processes of one account are kept apart. A target
//! process holds a descriptor and listens on an abstract unix socket, and
//! another process of the same account tries to trace it, to list its
//! descriptors, to take one with pidfd_getfd, to signal it and to connect
//! to its socket. The target is dumpable or not, and in a landlock domain
//! of its own or not, and the other process is its parent or its sibling,
//! since Yama's ptrace scope 1 lets a process trace only its descendants.

use std::ffi::c_int;

use crate::sys::{answer, await_signal, fork_run, last_error, pipe, result, signal, wait};

const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;
const LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET: u64 = 1;
const LANDLOCK_SCOPE_SIGNAL: u64 = 2;

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
    scoped: u64,
}

/// The landlock ABI the kernel offers, or the error.
pub fn landlock_abi() -> String {
    // SAFETY: the version query takes no attribute.
    let r = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    if r < 0 {
        format!("error: {}", last_error())
    } else {
        r.to_string()
    }
}

/// Puts this process into a new landlock domain that scopes signals and
/// abstract unix sockets to the domain, and handles no file or network
/// access.
fn restrict() -> Result<(), String> {
    let attr = RulesetAttr {
        handled_access_fs: 0,
        handled_access_net: 0,
        scoped: LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET | LANDLOCK_SCOPE_SIGNAL,
    };
    // SAFETY: the attribute is valid for the call, and the calls that
    // follow take plain integers.
    unsafe {
        let fd = libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &attr as *const RulesetAttr,
            std::mem::size_of::<RulesetAttr>(),
            0u32,
        );
        if fd < 0 {
            return Err(format!("landlock_create_ruleset: {}", last_error()));
        }
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            return Err(format!("PR_SET_NO_NEW_PRIVS: {}", last_error()));
        }
        if libc::syscall(libc::SYS_landlock_restrict_self, fd as c_int, 0u32) != 0 {
            return Err(format!("landlock_restrict_self: {}", last_error()));
        }
        libc::close(fd as c_int);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Target {
    dumpable: bool,
    landlock: bool,
}

/// The target: ignores SIGUSR1, so a signal that reaches it does no harm,
/// sets itself not dumpable and enters its domain as `t` says, opens
/// /dev/null and binds an abstract socket, writes the descriptor's number
/// to `ready`, and waits on `stop`.
fn target(t: Target, ready: c_int, stop: c_int) -> i32 {
    // SAFETY: plain system calls on this process.
    unsafe {
        libc::signal(libc::SIGUSR1, libc::SIG_IGN);
        if !t.dumpable {
            libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
        }
    }
    if t.landlock
        && let Err(e) = restrict()
    {
        eprintln!("target: {e}");
        return 1;
    }
    let null = crate::sys::cstring("/dev/null");
    // SAFETY: opens /dev/null and binds a socket with a valid address.
    let fd = unsafe { libc::open(null.as_ptr(), libc::O_RDONLY) };
    let sock = listen_abstract();
    let msg = fd.to_ne_bytes();
    // SAFETY: writes four bytes.
    unsafe { libc::write(ready, msg.as_ptr().cast(), 4) };
    await_signal(stop);
    let _ = sock;
    0
}

fn abstract_addr(pid: libc::pid_t) -> (libc::sockaddr_un, libc::socklen_t) {
    // SAFETY: sockaddr_un is plain data.
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let name = format!("t3-probe-{pid}");
    for (i, b) in name.bytes().enumerate() {
        addr.sun_path[i + 1] = b as libc::c_char;
    }
    let len = std::mem::size_of::<libc::sa_family_t>() + 1 + name.len();
    (addr, len as libc::socklen_t)
}

fn listen_abstract() -> c_int {
    // SAFETY: a socket bound to an address built above.
    unsafe {
        let s = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
        let (addr, len) = abstract_addr(libc::getpid());
        libc::bind(s, (&raw const addr).cast(), len);
        libc::listen(s, 1);
        s
    }
}

/// Each way into `pid` that check 8 asks about, as the calling process.
fn try_reach(check: &str, case: &str, pid: libc::pid_t, fd: c_int) {
    // SAFETY: each call is a plain system call on another process, whose
    // result is only reported.
    unsafe {
        let r = libc::ptrace(libc::PTRACE_SEIZE, pid, 0, 0);
        answer(check, &format!("{case} ptrace"), result(r as i64));
        if r == 0 {
            libc::ptrace(libc::PTRACE_DETACH, pid, 0, 0);
        }
    }
    let fds = match std::fs::read_dir(format!("/proc/{pid}/fd")) {
        Ok(d) => format!("ok ({} entries)", d.count()),
        Err(e) => format!("error: {e}"),
    };
    answer(check, &format!("{case} /proc/pid/fd"), fds);
    let environ = match std::fs::read(format!("/proc/{pid}/environ")) {
        Ok(b) => format!("ok ({} bytes)", b.len()),
        Err(e) => format!("error: {e}"),
    };
    answer(check, &format!("{case} /proc/pid/environ"), environ);
    // SAFETY: pidfd_open and pidfd_getfd take integers; any descriptor they
    // return is closed.
    unsafe {
        let pidfd = libc::syscall(libc::SYS_pidfd_open, pid, 0u32);
        if pidfd < 0 {
            answer(
                check,
                &format!("{case} pidfd_getfd"),
                format!("pidfd_open: {}", last_error()),
            );
        } else {
            let got = libc::syscall(libc::SYS_pidfd_getfd, pidfd as c_int, fd, 0u32);
            answer(check, &format!("{case} pidfd_getfd"), result(got as i64));
            if got >= 0 {
                libc::close(got as c_int);
            }
            libc::close(pidfd as c_int);
        }
        answer(
            check,
            &format!("{case} kill SIGUSR1"),
            result(i64::from(libc::kill(pid, libc::SIGUSR1))),
        );
        let s = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
        let (addr, len) = abstract_addr(pid);
        let r = libc::connect(s, (&raw const addr).cast(), len);
        answer(
            check,
            &format!("{case} connect to its abstract socket"),
            result(i64::from(r)),
        );
        libc::close(s);
    }
}

/// Starts a target and returns its pid, the number of its descriptor and
/// the write end of its stop pipe.
fn start_target(t: Target) -> Result<(libc::pid_t, c_int, c_int), String> {
    let (ready_r, ready_w) = pipe();
    let (stop_r, stop_w) = pipe();
    let pid = fork_run(|| target(t, ready_w, stop_r))?;
    // SAFETY: closes the ends this process does not use, and reads the
    // descriptor's number.
    unsafe {
        libc::close(ready_w);
        libc::close(stop_r);
        let mut b = [0u8; 4];
        let n = libc::read(ready_r, b.as_mut_ptr().cast(), 4);
        libc::close(ready_r);
        if n != 4 {
            return Err("the target did not start".into());
        }
        Ok((pid, c_int::from_ne_bytes(b), stop_w))
    }
}

fn stop_target(pid: libc::pid_t, stop_w: c_int) {
    signal(stop_w);
    // SAFETY: closes our end of the pipe.
    unsafe { libc::close(stop_w) };
    wait(pid);
}

/// Runs every case of check 8, labelled with `yama`, the ptrace scope the
/// guest set before the run.
pub fn run(check: &str) {
    answer(check, "identity", crate::sys::identity());
    answer(check, "landlock ABI", landlock_abi());
    let yama = std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope").unwrap_or_default();
    answer(check, "yama ptrace_scope", yama.trim());
    let cases = [
        (
            "dumpable",
            Target {
                dumpable: true,
                landlock: false,
            },
        ),
        (
            "not dumpable",
            Target {
                dumpable: false,
                landlock: false,
            },
        ),
        (
            "dumpable, own landlock domain",
            Target {
                dumpable: true,
                landlock: true,
            },
        ),
        (
            "not dumpable, own landlock domain",
            Target {
                dumpable: false,
                landlock: true,
            },
        ),
    ];
    for (name, t) in cases {
        // The parent reaches into its child.
        match start_target(t) {
            Ok((pid, fd, stop)) => {
                try_reach(check, &format!("parent to child, {name}:"), pid, fd);
                stop_target(pid, stop);
            }
            Err(e) => answer(check, &format!("parent to child, {name}"), e),
        }
        // A sibling reaches into the target, from a landlock domain of
        // its own when the target has one, as two session processes would.
        match start_target(t) {
            Ok((pid, fd, stop)) => {
                let sibling = fork_run(|| {
                    if t.landlock
                        && let Err(e) = restrict()
                    {
                        answer(check, &format!("sibling to sibling, {name}"), e);
                        return 1;
                    }
                    try_reach(check, &format!("sibling to sibling, {name}:"), pid, fd);
                    if t.landlock {
                        // SAFETY: getppid and kill on our own parent, which
                        // ignores nothing, so signal 0 only checks.
                        let r = unsafe { libc::kill(libc::getppid(), 0) };
                        answer(
                            check,
                            &format!(
                                "sibling to sibling, {name}: kill 0 to the parent outside its domain"
                            ),
                            result(i64::from(r)),
                        );
                    }
                    0
                });
                match sibling {
                    Ok(s) => {
                        wait(s);
                    }
                    Err(e) => answer(check, &format!("sibling to sibling, {name}"), e),
                }
                if t.landlock {
                    // A process outside the domain signals into it.
                    // SAFETY: the target ignores SIGUSR1.
                    let r = unsafe { libc::kill(pid, libc::SIGUSR1) };
                    answer(
                        check,
                        &format!("outside the domain into it, {name}: kill SIGUSR1"),
                        result(i64::from(r)),
                    );
                }
                stop_target(pid, stop);
            }
            Err(e) => answer(check, &format!("sibling to sibling, {name}"), e),
        }
    }
}
