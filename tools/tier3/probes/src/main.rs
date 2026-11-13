//! t3-probe: the probes behind ADR-1400 D4's checks, "The checks the build
//! makes first". The tier 3 guest's script `guest/rootchecks` runs them as
//! root or as the probe's account, and each prints its answers as lines of
//! the form `ROOTCHECK <check> <key>: <value>`.
//!
//! The commands:
//!
//! - `session --check N --service S --user U --vt V [--env NAME=VALUE]... -- CMD...`:
//!   as root, shows VT V, opens a session of U through the PAM service S on
//!   it, and runs CMD in the session as U, with VT V's tty on descriptor 3.
//! - `as --check N --user U [--vt V] -- CMD...`: as root, runs CMD as U,
//!   with VT V's tty on descriptor 3 when V is given.
//! - `control --check N [--device PATH]... [--then release|exit]`: inside
//!   a logind session, takes control of the session, takes each device and
//!   asks logind's power questions, reading the VT's state through
//!   descriptor 3 after each step.
//! - `seat-client --check N [--device PATH]...`: opens the seat through
//!   libseat's backend LIBSEAT_BACKEND names and each device through it,
//!   reading the VT's state through descriptor 3 after each step.
//! - `builtin --check N --user U --vt V`: as root, opens the seat through
//!   libseat's builtin backend, then drops to U and opens a card.
//! - `kmsg --check N --user U`: as root, opens /dev/kmsg with
//!   dmesg_restrict at 1, and reads it after dropping to U.
//! - `isolate --check N`: as one account, reaches into another process of
//!   the same account in each way check 8 names.
//! - `vt-state --check N --vt V --label L`: as root, the state of VT V.
//! - `pty-run --check N --user U --label L -- CMD...`: as root, runs CMD
//!   as U on a new pty that is its controlling terminal, and answers what
//!   CMD wrote there within 5 s and how it ended.
//! - `drm --check N --user U --vt V`: as root, opens the card and holds
//!   master on it, and passes a copy of the descriptor to a process running
//!   as U, which draws with dumb buffers and page flips while root drops
//!   and sets master on its own copy.

mod bus;
mod drm;
mod isolate;
mod pam;
mod seat;
mod sys;
mod vt;

use std::ffi::c_int;
use std::os::unix::process::CommandExt;

use sys::{answer, drop_to, fork_run, result, wait};

/// The command line: the command, its `--name value` options and what
/// follows `--`.
struct Args {
    command: String,
    opts: Vec<(String, String)>,
    rest: Vec<String>,
}

impl Args {
    fn parse() -> Args {
        let mut it = std::env::args().skip(1);
        let command = it.next().unwrap_or_default();
        let mut opts = Vec::new();
        let mut rest = Vec::new();
        while let Some(a) = it.next() {
            if a == "--" {
                rest.extend(it.by_ref());
                break;
            }
            if let Some(name) = a.strip_prefix("--") {
                opts.push((name.to_string(), it.next().unwrap_or_default()));
            }
        }
        Args {
            command,
            opts,
            rest,
        }
    }

    fn get(&self, name: &str) -> String {
        self.opts
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }

    fn all(&self, name: &str) -> Vec<String> {
        self.opts
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
            .collect()
    }

    fn vt(&self) -> Option<u32> {
        self.get("vt").parse().ok()
    }
}

fn main() {
    let args = Args::parse();
    let code = match args.command.as_str() {
        "session" => session(&args),
        "as" => as_user(&args),
        "control" => control(&args),
        "seat-client" => seat_client(&args),
        "builtin" => builtin(&args),
        "kmsg" => kmsg(&args),
        "isolate" => {
            isolate::run(&args.get("check"));
            0
        }
        "drm" => draw(&args),
        "vt-state" => vt_state(&args),
        "pty-run" => pty_run(&args),
        _ => {
            eprintln!(
                "usage: t3-probe session|as|control|seat-client|builtin|kmsg|isolate|drm|vt-state|pty-run --check N ..."
            );
            2
        }
    };
    std::process::exit(code);
}

/// Waits 100 ms: one turn of a bounded wait for state another process
/// changes.
fn turn() {
    // SAFETY: poll with no descriptors only waits.
    unsafe { libc::poll(std::ptr::null_mut(), 0, 100) };
}

/// Puts `tty` on descriptor 3 for the command a probe starts, open across
/// exec. dup2 onto 3 clears close-on-exec, but leaves the flag as it was
/// when `tty` is already 3, so the flag is cleared explicitly.
fn tty_on_3(tty: c_int) {
    // SAFETY: dup2 and fcntl on descriptors this process owns.
    unsafe {
        libc::dup2(tty, 3);
        libc::fcntl(3, libc::F_SETFD, 0);
    }
}

/// Drops to `user` and runs `rest`, never returning on success.
fn drop_and_exec(check: &str, user: &str, rest: &[String], env: &[String]) -> i32 {
    for e in env {
        if let Some((k, v)) = e.split_once('=') {
            // SAFETY: the forked child is single-threaded.
            unsafe { std::env::set_var(k, v) };
        }
    }
    match drop_to(user) {
        Ok(id) => answer(check, "the command runs as", id),
        Err(e) => {
            answer(check, "drop to the account", e);
            return 1;
        }
    }
    let Some((cmd, cargs)) = rest.split_first() else {
        return 2;
    };
    let e = std::process::Command::new(cmd).args(cargs).exec();
    answer(check, "exec", e);
    1
}

fn session(args: &Args) -> i32 {
    let check = args.get("check");
    let (user, service) = (args.get("user"), args.get("service"));
    let Some(n) = args.vt() else { return 2 };
    answer(
        &check,
        &format!("show tty{n}"),
        vt::activate(n).map_or_else(|e| e, |()| "ok".into()),
    );
    let tty = match vt::open(n) {
        Ok(fd) => fd,
        Err(e) => {
            answer(&check, "open the VT", e);
            return 1;
        }
    };
    answer(&check, "VT before the session", vt::state(tty));
    let envs = args.all("env");
    let opened = pam::open(&service, &user, n, &envs);
    for (step, said) in &opened.steps {
        answer(&check, step, said);
    }
    let Some(session) = opened.session else {
        return 1;
    };
    let env = session.env();
    let id = env
        .iter()
        .find_map(|e| e.strip_prefix("XDG_SESSION_ID="))
        .unwrap_or("")
        .to_string();
    answer(&check, "PAM environment", env.join(" "));
    answer(
        &check,
        "logind's session",
        sys::run(&[
            "loginctl",
            "show-session",
            &id,
            "-p",
            "Name",
            "-p",
            "Class",
            "-p",
            "Type",
            "-p",
            "Seat",
            "-p",
            "VTNr",
            "-p",
            "Active",
            "-p",
            "State",
            "-p",
            "Leader",
        ]),
    );
    answer(&check, "VT after the session opened", vt::state(tty));
    let rest = args.rest.clone();
    let child = fork_run(|| {
        tty_on_3(tty);
        drop_and_exec(&check, &user, &rest, &env)
    });
    match child {
        Ok(pid) => answer(&check, "the command ended", wait(pid)),
        Err(e) => answer(&check, "fork", e),
    }
    // logind learns of a controller's exit when its bus connection closes,
    // after the process has ended, so the VT is read until it changes, for
    // at most 50 turns of 100 ms.
    let ended = vt::state(tty);
    let mut turns = 0;
    let mut now = ended.clone();
    while now == ended && turns < 50 {
        turn();
        turns += 1;
        now = vt::state(tty);
    }
    answer(&check, "VT as the command ended", &ended);
    if now == ended {
        answer(&check, "VT 5 s after the command ended", "unchanged");
    } else {
        answer(
            &check,
            &format!("VT {} ms after the command ended", turns * 100),
            now,
        );
    }
    answer(&check, "pam_close_session", session.close());
    answer(&check, "VT after the session closed", vt::state(tty));
    0
}

fn vt_state(args: &Args) -> i32 {
    let check = args.get("check");
    let Some(n) = args.vt() else { return 2 };
    match vt::open(n) {
        Ok(fd) => answer(&check, &args.get("label"), vt::state(fd)),
        Err(e) => answer(&check, &args.get("label"), e),
    }
    0
}

fn pty_run(args: &Args) -> i32 {
    let check = args.get("check");
    let label = args.get("label");
    // SAFETY: opens a pty master and unlocks its slave; ptsname's static
    // buffer is copied at once.
    let (master, slave) = unsafe {
        let m = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC);
        if m < 0 || libc::grantpt(m) != 0 || libc::unlockpt(m) != 0 {
            answer(&check, &label, format!("no pty: {}", sys::last_error()));
            return 1;
        }
        (m, sys::text(libc::ptsname(m)))
    };
    let (user, rest) = (args.get("user"), args.rest.clone());
    // The pty's slave belongs to the account, as a pty that a program of
    // the account opened would.
    let owner = match sys::account(&user) {
        Ok(a) => a,
        Err(e) => {
            answer(&check, &label, e);
            return 1;
        }
    };
    let path = sys::cstring(&slave);
    // SAFETY: chown and chmod on the slave's path.
    unsafe {
        libc::chown(path.as_ptr(), owner.uid, owner.gid);
        libc::chmod(path.as_ptr(), 0o620);
    }
    let child = fork_run(|| {
        // SAFETY: a new session whose controlling terminal is the pty's
        // slave, on descriptors 0 to 2.
        unsafe {
            libc::setsid();
            let s = libc::open(path.as_ptr(), libc::O_RDWR);
            libc::ioctl(s, libc::TIOCSCTTY, 0);
            for fd in 0..3 {
                libc::dup2(s, fd);
            }
            if s > 2 {
                libc::close(s);
            }
        }
        if let Err(e) = drop_to(&user) {
            eprintln!("{e}");
            return 1;
        }
        let Some((cmd, cargs)) = rest.split_first() else {
            return 2;
        };
        let e = std::process::Command::new(cmd).args(cargs).exec();
        eprintln!("exec: {e}");
        1
    });
    let pid = match child {
        Ok(p) => p,
        Err(e) => {
            answer(&check, &label, e);
            return 1;
        }
    };
    // At most 50 turns of 100 ms: the bound fails a command that hangs
    // instead of hanging the probe.
    let mut out = Vec::new();
    let mut ended = None;
    for _ in 0..50 {
        let mut pfd = libc::pollfd {
            fd: master,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: polls and reads the master into a valid buffer.
        let drained = unsafe {
            let mut drained = true;
            if libc::poll(&mut pfd, 1, 100) == 1 {
                let mut buf = [0u8; 512];
                let n = libc::read(master, buf.as_mut_ptr().cast(), buf.len());
                if n > 0 {
                    out.extend_from_slice(&buf[..n as usize]);
                    drained = false;
                }
            }
            let mut status: c_int = 0;
            if ended.is_none() && libc::waitpid(pid, &mut status, libc::WNOHANG) == pid {
                ended = Some(if libc::WIFEXITED(status) {
                    format!("exit {}", libc::WEXITSTATUS(status))
                } else {
                    format!("signal {}", libc::WTERMSIG(status))
                });
            }
            drained
        };
        if ended.is_some() && drained {
            break;
        }
    }
    let ended = ended.unwrap_or_else(|| {
        // SAFETY: ends our own child, which still runs after 5 s.
        unsafe { libc::kill(pid, libc::SIGKILL) };
        format!("still running after 5 s, then {}", wait(pid))
    });
    answer(
        &check,
        &label,
        format!(
            "{ended}; wrote: {}",
            sys::one_line(&String::from_utf8_lossy(&out))
        ),
    );
    0
}

fn as_user(args: &Args) -> i32 {
    let check = args.get("check");
    let tty = match args.vt() {
        Some(n) => match vt::open(n) {
            Ok(fd) => Some(fd),
            Err(e) => {
                answer(&check, "open the VT", e);
                return 1;
            }
        },
        None => None,
    };
    if let Some(fd) = tty {
        tty_on_3(fd);
    }
    drop_and_exec(&check, &args.get("user"), &args.rest, &[])
}

/// The device number of `path`, as logind's TakeDevice takes it.
fn device_number(path: &str) -> Result<(u32, u32), String> {
    let p = sys::cstring(path);
    // SAFETY: stat fills a structure of the right size.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        if libc::stat(p.as_ptr(), &mut st) != 0 {
            return Err(format!("stat {path}: {}", sys::last_error()));
        }
        Ok((libc::major(st.st_rdev), libc::minor(st.st_rdev)))
    }
}

fn control(args: &Args) -> i32 {
    use bus::{Arg, Reply, Want};
    let check = args.get("check");
    let tty: c_int = 3;
    answer(&check, "controller", sys::identity());
    answer(&check, "VT before TakeControl", vt::state(tty));
    let bus = match bus::Bus::system() {
        Ok(b) => b,
        Err(e) => {
            answer(&check, "the system bus", e);
            return 1;
        }
    };
    const SESSION: &str = "/org/freedesktop/login1/session/auto";
    const IFACE: &str = "org.freedesktop.login1.Session";
    let took = bus.login1(
        SESSION,
        IFACE,
        "TakeControl",
        &[Arg::Bool(false)],
        Want::Nothing,
    );
    answer(
        &check,
        "TakeControl",
        took.as_ref().map_or_else(Clone::clone, |_| "ok".into()),
    );
    answer(&check, "VT after TakeControl", vt::state(tty));
    for path in args.all("device") {
        let (major, minor) = match device_number(&path) {
            Ok(n) => n,
            Err(e) => {
                answer(&check, &format!("TakeDevice {path}"), e);
                continue;
            }
        };
        match bus.login1(
            SESSION,
            IFACE,
            "TakeDevice",
            &[Arg::U32(major), Arg::U32(minor)],
            Want::FdBool,
        ) {
            Ok(Reply::FdBool(fd, inactive)) => {
                answer(
                    &check,
                    &format!("TakeDevice {path}"),
                    format!("ok, fd {fd}, inactive={inactive}"),
                );
                if path.contains("/dri/card") {
                    answer(&check, &format!("{path} holds master"), drm::is_master(fd));
                }
            }
            Ok(_) => answer(&check, &format!("TakeDevice {path}"), "no descriptor"),
            Err(e) => answer(&check, &format!("TakeDevice {path}"), e),
        }
        answer(
            &check,
            &format!("VT after TakeDevice {path}"),
            vt::state(tty),
        );
    }
    for q in ["CanPowerOff", "CanReboot", "CanSuspend"] {
        let r = bus.login1(
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            q,
            &[],
            Want::Str,
        );
        answer(
            &check,
            &format!("{q} from the session"),
            match r {
                Ok(Reply::Str(s)) => s,
                Ok(_) => "no answer".into(),
                Err(e) => e,
            },
        );
    }
    let unit = format!("t3-probe-{}.service", std::process::id());
    answer(
        &check,
        "StartTransientUnit from the session",
        sys::run(&[
            "busctl",
            "call",
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            "StartTransientUnit",
            "ssa(sv)a(sa(sv))",
            &unit,
            "fail",
            "1",
            "ExecStart",
            "a(sasb)",
            "1",
            "/usr/bin/true",
            "1",
            "/usr/bin/true",
            "false",
            "0",
        ]),
    );
    if args.get("then") == "release" {
        let r = bus.login1(SESSION, IFACE, "ReleaseControl", &[], Want::Nothing);
        answer(
            &check,
            "ReleaseControl",
            r.map_or_else(|e| e, |_| "ok".into()),
        );
        answer(&check, "VT after ReleaseControl", vt::state(tty));
    } else {
        answer(&check, "the controller exits without ReleaseControl", "now");
    }
    0
}

fn seat_client(args: &Args) -> i32 {
    let check = args.get("check");
    let tty: c_int = 3;
    answer(&check, "client", sys::identity());
    answer(
        &check,
        "LIBSEAT_BACKEND",
        std::env::var("LIBSEAT_BACKEND").unwrap_or_default(),
    );
    answer(&check, "VT before the seat opened", vt::state(tty));
    let (mut seat, name) = match seat::Seat::open() {
        Ok(s) => s,
        Err(e) => {
            answer(&check, "libseat_open_seat", e);
            return 1;
        }
    };
    answer(&check, "libseat_open_seat", format!("ok, seat {name}"));
    answer(&check, "the seat enabled", seat.wait_enabled(5000));
    answer(&check, "VT after the seat enabled", vt::state(tty));
    for path in args.all("device") {
        match seat.open_device(&path) {
            Ok(fd) => {
                answer(
                    &check,
                    &format!("libseat_open_device {path}"),
                    format!("ok, fd {fd}"),
                );
                if path.contains("/dri/card") {
                    answer(&check, &format!("{path} holds master"), drm::is_master(fd));
                }
            }
            Err(e) => answer(
                &check,
                &format!("libseat_open_device {path}"),
                format!("error: {e}"),
            ),
        }
        answer(&check, &format!("VT after opening {path}"), vt::state(tty));
    }
    answer(&check, "enable events", seat.enabled());
    answer(&check, "libseat_close_seat", seat.close());
    answer(&check, "VT after the seat closed", vt::state(tty));
    0
}

/// Each child of this process, with its user ids and capabilities.
fn children() -> String {
    let me = std::process::id().to_string();
    let mut out = Vec::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Ok(status) = std::fs::read_to_string(e.path().join("status")) else {
            continue;
        };
        let field = |n: &str| {
            status
                .lines()
                .find(|l| l.starts_with(n))
                .map(|l| l[n.len()..].trim().to_string())
                .unwrap_or_default()
        };
        if field("PPid:") == me {
            out.push(format!(
                "pid {} {} Uid=[{}] CapEff={}",
                field("Pid:"),
                field("Name:"),
                field("Uid:"),
                field("CapEff:")
            ));
        }
    }
    if out.is_empty() {
        "none".into()
    } else {
        out.join("; ")
    }
}

fn builtin(args: &Args) -> i32 {
    let check = args.get("check");
    let Some(n) = args.vt() else { return 2 };
    answer(
        &check,
        &format!("show tty{n}"),
        vt::activate(n).map_or_else(|e| e, |()| "ok".into()),
    );
    let tty = match vt::open(n) {
        Ok(fd) => fd,
        Err(e) => {
            answer(&check, "open the VT", e);
            return 1;
        }
    };
    answer(&check, "VT before the seat opened", vt::state(tty));
    answer(&check, "client before", sys::identity());
    let (mut seat, name) = match seat::Seat::open() {
        Ok(s) => s,
        Err(e) => {
            answer(&check, "libseat_open_seat with the builtin backend", e);
            return 1;
        }
    };
    answer(
        &check,
        "libseat_open_seat with the builtin backend",
        format!("ok, seat {name}"),
    );
    answer(&check, "the seat enabled", seat.wait_enabled(5000));
    answer(&check, "VT after the seat enabled", vt::state(tty));
    answer(&check, "the client's children", children());
    match drop_to(&args.get("user")) {
        Ok(id) => answer(&check, "the client after dropping root", id),
        Err(e) => answer(&check, "the client drops root", e),
    }
    match seat.open_device("/dev/dri/card0") {
        Ok(fd) => {
            answer(
                &check,
                "the client without root opens /dev/dri/card0 through the seat",
                format!("ok, fd {fd}"),
            );
            answer(&check, "/dev/dri/card0 holds master", drm::is_master(fd));
        }
        Err(e) => answer(
            &check,
            "the client without root opens /dev/dri/card0 through the seat",
            format!("error: {e}"),
        ),
    }
    answer(
        &check,
        "the client's children after dropping root",
        children(),
    );
    answer(&check, "libseat_close_seat", seat.close());
    answer(&check, "VT after the seat closed", vt::state(tty));
    // The embedded server is this process's child, so the probe reaps it,
    // waiting at most 50 turns of 100 ms.
    let mut reaped = String::from("still running after 5 s");
    for _ in 0..50 {
        let mut status: c_int = 0;
        // SAFETY: reaps any child of this process without blocking.
        let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if pid > 0 {
            reaped = format!("pid {pid} ended with exit {}", libc::WEXITSTATUS(status));
            break;
        }
        turn();
    }
    answer(&check, "the embedded server after the seat closed", reaped);
    0
}

fn kmsg(args: &Args) -> i32 {
    let check = args.get("check");
    let w = std::fs::write("/proc/sys/kernel/dmesg_restrict", "1");
    answer(
        &check,
        "set kernel.dmesg_restrict to 1",
        w.map_or_else(|e| e.to_string(), |()| "ok".into()),
    );
    let restrict = std::fs::read_to_string("/proc/sys/kernel/dmesg_restrict").unwrap_or_default();
    answer(&check, "kernel.dmesg_restrict", restrict.trim());
    let path = sys::cstring("/dev/kmsg");
    // SAFETY: open with a valid path.
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    answer(&check, "root opens /dev/kmsg", result(i64::from(fd)));
    if fd < 0 {
        return 1;
    }
    let user = args.get("user");
    let child = fork_run(|| {
        match drop_to(&user) {
            Ok(id) => answer(&check, "the reader", id),
            Err(e) => {
                answer(&check, "drop to the account", e);
                return 1;
            }
        }
        let mut buf = [0u8; 1024];
        // SAFETY: reads one record into a valid buffer.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        let said = if n > 0 {
            let rec = String::from_utf8_lossy(&buf[..n as usize]);
            format!(
                "ok, {n} bytes: {}",
                sys::one_line(&rec).chars().take(80).collect::<String>()
            )
        } else {
            format!("error: {}", sys::last_error())
        };
        answer(&check, "the account reads root's descriptor", said);
        // SAFETY: open with a valid path, closed if it opened.
        let own = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
        answer(
            &check,
            "the account opens /dev/kmsg itself",
            result(i64::from(own)),
        );
        // SAFETY: SYSLOG_ACTION_SIZE_BUFFER takes no buffer.
        let size = unsafe { libc::klogctl(10, std::ptr::null_mut(), 0) };
        answer(
            &check,
            "the account calls syslog(2)",
            result(i64::from(size)),
        );
        0
    });
    match child {
        Ok(pid) => answer(&check, "the reader ended", wait(pid)),
        Err(e) => answer(&check, "fork", e),
    }
    0
}

/// Sends `fd` over `sock` with SCM_RIGHTS.
fn send_fd(sock: c_int, fd: c_int) -> String {
    // SAFETY: builds one control message with one descriptor, in a buffer
    // of the size CMSG_SPACE gives.
    unsafe {
        let mut byte = [0u8];
        let mut iov = libc::iovec {
            iov_base: byte.as_mut_ptr().cast(),
            iov_len: 1,
        };
        let space = libc::CMSG_SPACE(std::mem::size_of::<c_int>() as u32) as usize;
        let mut cbuf = vec![0u8; space];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cbuf.as_mut_ptr().cast();
        msg.msg_controllen = space as _;
        let cm = libc::CMSG_FIRSTHDR(&msg);
        (*cm).cmsg_level = libc::SOL_SOCKET;
        (*cm).cmsg_type = libc::SCM_RIGHTS;
        (*cm).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<c_int>() as u32) as _;
        std::ptr::write_unaligned(libc::CMSG_DATA(cm).cast::<c_int>(), fd);
        result(libc::sendmsg(sock, &msg, 0) as i64)
    }
}

/// Receives one descriptor sent with SCM_RIGHTS.
fn recv_fd(sock: c_int) -> Result<c_int, String> {
    // SAFETY: receives into buffers of the sizes the message needs.
    unsafe {
        let mut byte = [0u8];
        let mut iov = libc::iovec {
            iov_base: byte.as_mut_ptr().cast(),
            iov_len: 1,
        };
        let space = libc::CMSG_SPACE(std::mem::size_of::<c_int>() as u32) as usize;
        let mut cbuf = vec![0u8; space];
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cbuf.as_mut_ptr().cast();
        msg.msg_controllen = space as _;
        if libc::recvmsg(sock, &mut msg, libc::MSG_CMSG_CLOEXEC) < 1 {
            return Err(format!("recvmsg: {}", sys::last_error()));
        }
        let cm = libc::CMSG_FIRSTHDR(&msg);
        if cm.is_null() || (*cm).cmsg_type != libc::SCM_RIGHTS {
            return Err("no descriptor in the message".into());
        }
        Ok(std::ptr::read_unaligned(
            libc::CMSG_DATA(cm).cast::<c_int>(),
        ))
    }
}

/// Sends or waits for one step of the drawing's turn-taking.
fn step(sock: c_int) {
    sys::signal(sock);
}

fn draw(args: &Args) -> i32 {
    let check = args.get("check");
    let Some(n) = args.vt() else { return 2 };
    answer(
        &check,
        &format!("show tty{n}"),
        vt::activate(n).map_or_else(|e| e, |()| "ok".into()),
    );
    let tty = match vt::open(n) {
        Ok(fd) => fd,
        Err(e) => {
            answer(&check, "open the VT", e);
            return 1;
        }
    };
    answer(
        &check,
        "root sets KD_GRAPHICS",
        vt::set_mode(tty, vt::KD_GRAPHICS),
    );
    let card = sys::cstring("/dev/dri/card0");
    // SAFETY: open with a valid path.
    let fd = unsafe { libc::open(card.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
    answer(&check, "root opens /dev/dri/card0", result(i64::from(fd)));
    if fd < 0 {
        return 1;
    }
    answer(
        &check,
        "root's file holds master after the open",
        drm::is_master(fd),
    );
    answer(
        &check,
        "root calls SET_MASTER on its copy",
        drm::master_call(fd, drm::SET_MASTER),
    );
    let mut pair = [0 as c_int; 2];
    // SAFETY: socketpair fills two descriptors.
    unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
            0,
            pair.as_mut_ptr(),
        )
    };
    let (root_end, drawer_end) = (pair[0], pair[1]);
    let user = args.get("user");
    let drawer = fork_run(|| {
        // SAFETY: the drawer keeps only its end of the socket, and no copy
        // of the card but the one it receives.
        unsafe {
            libc::close(root_end);
            libc::close(fd);
        }
        match drop_to(&user) {
            Ok(id) => answer(&check, "the drawer", id),
            Err(e) => {
                answer(&check, "drop to the account", e);
                return 1;
            }
        }
        // SAFETY: open with a valid path.
        let own = unsafe { libc::open(card.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
        answer(
            &check,
            "the drawer opens /dev/dri/card0 itself",
            result(i64::from(own)),
        );
        let copy = match recv_fd(drawer_end) {
            Ok(c) => c,
            Err(e) => {
                answer(&check, "the drawer receives the card", e);
                return 1;
            }
        };
        answer(
            &check,
            "the drawer's copy holds master",
            drm::is_master(copy),
        );
        let mut drawing = match drm::Drawing::new(copy) {
            Ok((d, what)) => {
                answer(
                    &check,
                    "the drawer sets a mode with a dumb buffer (SETCRTC)",
                    format!("ok, {what}"),
                );
                d
            }
            Err(e) => {
                answer(
                    &check,
                    "the drawer sets a mode with a dumb buffer (SETCRTC)",
                    e,
                );
                return 1;
            }
        };
        let flips = (0..10).map(|_| drawing.flip(copy)).collect::<Vec<_>>();
        let done = flips.iter().filter(|r| r.is_ok()).count();
        let first_err = flips
            .iter()
            .find_map(|r| r.as_ref().err().cloned())
            .unwrap_or_default();
        answer(
            &check,
            "the drawer's page flips, each waited for by its event",
            format!("{done} of 10 {first_err}"),
        );
        answer(
            &check,
            "the CRTC shows the drawer's last buffer",
            drawing.shows_front(copy),
        );
        step(drawer_end);
        sys::await_signal(drawer_end);
        answer(
            &check,
            "after root's DROP_MASTER, the drawer's copy holds master",
            drm::is_master(copy),
        );
        answer(
            &check,
            "after root's DROP_MASTER, a page flip",
            drawing.flip(copy).map_or_else(|e| e, |()| "ok".into()),
        );
        answer(
            &check,
            "the drawer calls SET_MASTER on its copy",
            drm::master_call(copy, drm::SET_MASTER),
        );
        step(drawer_end);
        sys::await_signal(drawer_end);
        answer(
            &check,
            "after root's SET_MASTER again, the drawer's copy holds master",
            drm::is_master(copy),
        );
        answer(
            &check,
            "after root's SET_MASTER again, a page flip",
            drawing.flip(copy).map_or_else(|e| e, |()| "ok".into()),
        );
        answer(
            &check,
            "the CRTC shows the drawer's last buffer",
            drawing.shows_front(copy),
        );
        0
    });
    let pid = match drawer {
        Ok(p) => p,
        Err(e) => {
            answer(&check, "fork", e);
            return 1;
        }
    };
    // SAFETY: the root process keeps its own end only.
    unsafe { libc::close(drawer_end) };
    answer(
        &check,
        "root passes a copy of its descriptor (SCM_RIGHTS)",
        send_fd(root_end, fd),
    );
    if sys::await_signal(root_end) {
        answer(
            &check,
            "root calls DROP_MASTER on its copy",
            drm::master_call(fd, drm::DROP_MASTER),
        );
        step(root_end);
        if sys::await_signal(root_end) {
            answer(
                &check,
                "root calls SET_MASTER on its copy again",
                drm::master_call(fd, drm::SET_MASTER),
            );
            step(root_end);
        }
    }
    answer(&check, "the drawer ended", wait(pid));
    answer(&check, "root sets KD_TEXT", vt::set_mode(tty, vt::KD_TEXT));
    0
}
