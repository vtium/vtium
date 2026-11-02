//! The second test of ADR-1101 D9: the helper's filter installed in a
//! child, which then makes calls outside the list and sees each answered
//! ENOSYS, while a call on the list still works: a socket pair of any type
//! but the sequenced-packet one, a fork that makes a sibling or a new
//! namespace, and setting the descriptor limit are outside it; the
//! worker's second filter refusing a fork, a memory file and a socket
//! pair, and its third refusing an open; the worker's last filter, its
//! second on a kernel with landlock and its third on one without,
//! refusing `seccomp`, so it can add no filter; the worker's landlock
//! domain, after which it opens nothing but still reads what it holds;
//! and two attacks on a `sleep` process this
//! test starts, which the filter's conditions and the domain's scope
//! each stop: pointing a descriptor's signals at it, and changing its
//! limits.
//!
//! It has its own `main`, so the child, this binary run again with
//! `--child`, has one thread when it installs the filter.

use std::os::fd::{AsRawFd, OwnedFd};
use std::process::{Command, ExitCode};

use rustix::io::Errno;
use rustix::net::{AddressFamily, SocketFlags, SocketType};

/// What the child sees under the filter, as an exit status: 0 when every
/// call answers as it should, otherwise the number of the first that
/// does not.
fn child(worker: bool) -> u8 {
    if !vtium_image_helper::fork::one_thread() {
        return 90;
    }
    // the filters of a worker on a kernel without landlock, which adds a
    // third
    let Ok(second) = vtium_image_helper::sandbox::WorkerFilter::build(0) else {
        return 92;
    };
    match vtium_image_helper::sandbox::install_helper_filter() {
        Ok(true) => {}
        Ok(false) => return 0,
        Err(_) => return 91,
    }
    // an internet socket is outside the list
    let inet = rustix::net::socket(AddressFamily::INET, SocketType::STREAM, None);
    if inet.err() != Some(Errno::NOSYS) {
        return 1;
    }
    // a socket pair of any family but Unix is too
    let pair = rustix::net::socketpair(
        AddressFamily::INET,
        SocketType::STREAM,
        SocketFlags::empty(),
        None,
    );
    if pair.err() != Some(Errno::NOSYS) {
        return 2;
    }
    // a Unix socket pair of the sequenced-packet type, for a worker's
    // channel, is on it, with its flags
    if rustix::net::socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
        None,
    )
    .is_err()
    {
        return 3;
    }
    // a Unix pair of any other type is not: a datagram socket can send to
    // any datagram socket the account may write by path, such as /dev/log
    for (n, kind) in (11u8..).zip([SocketType::DGRAM, SocketType::STREAM, SocketType::RAW]) {
        let pair = rustix::net::socketpair(AddressFamily::UNIX, kind, SocketFlags::CLOEXEC, None);
        if pair.err() != Some(Errno::NOSYS) {
            return n;
        }
    }
    // a fork is on it, but not one that makes the child its parent's
    // sibling or enters a new namespace
    for (n, flag) in (20u8..).zip([
        libc::CLONE_PARENT,
        libc::CLONE_NEWNS,
        libc::CLONE_NEWCGROUP,
        libc::CLONE_NEWUTS,
        libc::CLONE_NEWIPC,
        libc::CLONE_NEWUSER,
        libc::CLONE_NEWPID,
        libc::CLONE_NEWNET,
    ]) {
        if clone(flag) != Err(libc::ENOSYS) {
            return n;
        }
    }
    if clone(0).is_err() {
        return 28;
    }
    // the descriptor limit is read but never set, while other limits are
    let nofile = rustix::process::getrlimit(rustix::process::Resource::Nofile);
    if nofile.maximum.is_none() && nofile.current.is_none() {
        return 29;
    }
    let lowered = rustix::process::Rlimit {
        current: Some(64),
        maximum: Some(64),
    };
    if rustix::process::setrlimit(rustix::process::Resource::Nofile, lowered).err()
        != Some(Errno::NOSYS)
    {
        return 30;
    }
    if setrlimit_raw(i64::from(libc::RLIMIT_NOFILE), 64) != libc::ENOSYS {
        return 31;
    }
    if setrlimit_raw(i64::from(libc::RLIMIT_CORE), 0) != 0 {
        return 32;
    }
    // every ioctl is outside it
    let Ok(file) = std::fs::File::open("/") else {
        return 40;
    };
    if rustix::fs::ioctl_getflags(&file).err() != Some(Errno::NOSYS) {
        return 4;
    }
    // prctl with any option but no-new-privileges is too
    if rustix::thread::name().err() != Some(Errno::NOSYS) {
        return 5;
    }
    if worker {
        if second.install().is_err() {
            return 6;
        }
        // a worker can start no process
        if Command::new("/bin/true").status().is_ok() {
            return 7;
        }
        // nor make a memory file
        let memfd = rustix::fs::memfd_create("x", rustix::fs::MemfdFlags::CLOEXEC);
        if memfd.err() != Some(Errno::PERM) {
            return 8;
        }
        // nor a socket pair, even of the type the helper makes
        let pair = rustix::net::socketpair(
            AddressFamily::UNIX,
            SocketType::SEQPACKET,
            SocketFlags::CLOEXEC,
            None,
        );
        if pair.err() != Some(Errno::PERM) {
            return 12;
        }
        // its second filter is not its last, so it may still add one
        if seccomp_null() != libc::EFAULT {
            return 13;
        }
        // and once it has opened its source on a kernel without landlock,
        // its third filter refuses every open
        if second.install_no_open().is_err() {
            return 9;
        }
        if std::fs::File::open("/etc/passwd").is_ok() {
            return 10;
        }
        // and every further filter, so the count stays
        if seccomp_null() != libc::EPERM {
            return 14;
        }
    }
    0
}

/// A worker on a kernel with landlock, whose second filter is its last:
/// before it the call that adds a filter reaches the kernel, and once it
/// is in, the call answers EPERM, so the worker can add no third filter
/// and the count of its filters the helper reads stays where it is.
fn last_filter() -> u8 {
    if !vtium_image_helper::fork::one_thread() {
        return 90;
    }
    let Ok(second) = vtium_image_helper::sandbox::WorkerFilter::build(1) else {
        return 92;
    };
    match vtium_image_helper::sandbox::install_helper_filter() {
        Ok(true) => {}
        Ok(false) => return 0,
        Err(_) => return 91,
    }
    if seccomp_null() != libc::EFAULT {
        return 100;
    }
    if second.install().is_err() {
        return 101;
    }
    if seccomp_null() != libc::EPERM {
        return 102;
    }
    if second.install_no_open().is_ok() {
        return 103;
    }
    0
}

/// `seccomp(SECCOMP_SET_MODE_FILTER)` with no program: the kernel answers
/// EFAULT on the null pointer, so any other errno is a filter's answer.
fn seccomp_null() -> i32 {
    let op = libc::c_long::from(libc::SECCOMP_SET_MODE_FILTER);
    // SAFETY: the kernel reads the program through the pointer, which is
    // null, and so faults before it reads anything of this process.
    let got = unsafe { libc::syscall(libc::SYS_seccomp, op, 0, std::ptr::null::<u8>()) };
    if got == -1 { errno() } else { 0 }
}

/// `clone(2)` as a fork with `flag` added: `Ok` in the parent once the
/// child, which exits at once, is reaped; the errno when the call fails.
fn clone(flag: i32) -> Result<(), i32> {
    let flags = libc::c_long::from(flag | libc::SIGCHLD);
    // SAFETY: clone with no new stack forks this process, which has one
    // thread; the child calls only _exit, which is safe after a fork.
    let pid = unsafe { libc::syscall(libc::SYS_clone, flags, 0, 0, 0, 0) };
    match pid {
        -1 => Err(errno()),
        // SAFETY: the child ends at once, touching nothing.
        0 => unsafe { libc::_exit(0) },
        pid => {
            let pid = i32::try_from(pid).unwrap_or(-1);
            // a child of a sibling fork belongs to this process's parent,
            // so only a plain fork's child is reaped here
            if flag == 0 {
                let mut status = 0;
                // SAFETY: waitpid writes the status through a live local.
                unsafe { libc::waitpid(pid, &mut status, 0) };
            }
            Ok(())
        }
    }
}

/// `setrlimit(2)` itself, which the C library may route through
/// `prlimit64`, setting `resource` to `limit`, soft and hard; the errno, 0
/// when the call succeeded.
fn setrlimit_raw(resource: i64, limit: libc::rlim_t) -> i32 {
    let new = libc::rlimit {
        rlim_cur: limit,
        rlim_max: limit,
    };
    // SAFETY: the kernel reads one rlimit through a pointer to a live
    // local.
    let got = unsafe { libc::syscall(libc::SYS_setrlimit, resource, &raw const new) };
    if got == -1 { errno() } else { 0 }
}

/// `F_SETSIG`, which the libc crate names on some targets only: the
/// signal sent when a descriptor's owner is told of input.
const F_SETSIG: i32 = 10;

/// The errno of the last call that failed.
fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// `fcntl(fd, cmd, arg)` with integer arguments; the errno, 0 when the
/// call succeeded.
fn fcntl(fd: &OwnedFd, cmd: i32, arg: i32) -> i32 {
    // SAFETY: fcntl on a descriptor this process holds, with an integer
    // argument, touches no memory of this process.
    let got = unsafe { libc::fcntl(fd.as_raw_fd(), cmd, arg) };
    if got == -1 { errno() } else { 0 }
}

/// Points the signals of a socket's descriptor at `victim`, as SIGKILL,
/// and writes to the other end so the kernel sends one: the attack a
/// taken-over worker could make on any process its account owns. The
/// errno of each step, 0 for one that succeeded.
fn signal_by_descriptor(victim: i32) -> [i32; 3] {
    let Ok((a, b)) = rustix::net::socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    ) else {
        return [-1; 3];
    };
    let steps = [
        fcntl(&a, libc::F_SETOWN, victim),
        fcntl(&a, F_SETSIG, libc::SIGKILL),
        fcntl(&a, libc::F_SETFL, libc::O_ASYNC | libc::O_NONBLOCK),
    ];
    let _ = rustix::io::write(&b, b"x");
    steps
}

/// The two attacks under the helper's filter: each refused with ENOSYS,
/// while the forms of the same calls the helper makes still work.
fn attack(victim: i32) -> u8 {
    if !vtium_image_helper::fork::one_thread() {
        return 90;
    }
    match vtium_image_helper::sandbox::install_helper_filter() {
        Ok(true) => {}
        Ok(false) => return 0,
        Err(_) => return 91,
    }
    let nosys = libc::ENOSYS;
    let steps = signal_by_descriptor(victim);
    for (n, errno) in (60u8..).zip(steps) {
        if errno != nosys {
            return n;
        }
    }
    let Some(pid) = rustix::process::Pid::from_raw(victim) else {
        return 63;
    };
    let one = rustix::process::Rlimit {
        current: Some(1),
        maximum: Some(1),
    };
    let changed = rustix::process::prlimit(Some(pid), rustix::process::Resource::Nofile, one);
    if changed.err() != Some(Errno::NOSYS) {
        return 64;
    }
    // it reads its own limits and sets any but its descriptor limit; its
    // descriptors' flags, duplicates and seals stay
    let own = rustix::process::getrlimit(rustix::process::Resource::Core);
    if rustix::process::prlimit(None, rustix::process::Resource::Core, own).is_err() {
        return 65;
    }
    let nofile = rustix::process::getrlimit(rustix::process::Resource::Nofile);
    let raised = rustix::process::Rlimit {
        current: nofile.maximum,
        maximum: nofile.maximum,
    };
    if rustix::process::prlimit(None, rustix::process::Resource::Nofile, raised).err()
        != Some(Errno::NOSYS)
    {
        return 72;
    }
    let Ok(file) = std::fs::File::open("/") else {
        return 66;
    };
    let file = OwnedFd::from(file);
    if rustix::io::fcntl_getfd(&file).is_err()
        || rustix::fs::fcntl_getfl(&file).is_err()
        || rustix::fs::fcntl_setfl(&file, rustix::fs::OFlags::NONBLOCK).is_err()
        || file.try_clone().is_err()
    {
        return 67;
    }
    // no memory is made executable, by a mapping or after it
    use rustix::mm::{MapFlags, MprotectFlags, ProtFlags};
    // SAFETY: the kernel chooses the address of a new anonymous mapping,
    // which overlaps nothing this process holds.
    let exec = unsafe {
        rustix::mm::mmap_anonymous(
            std::ptr::null_mut(),
            4096,
            ProtFlags::READ | ProtFlags::EXEC,
            MapFlags::PRIVATE,
        )
    };
    if exec.err() != Some(Errno::NOSYS) {
        return 68;
    }
    // SAFETY: as above.
    let Ok(page) = (unsafe {
        rustix::mm::mmap_anonymous(
            std::ptr::null_mut(),
            4096,
            ProtFlags::READ | ProtFlags::WRITE,
            MapFlags::PRIVATE,
        )
    }) else {
        return 69;
    };
    // SAFETY: `page` is the mapping of 4096 bytes made above, which no
    // Rust value refers to.
    let to_exec =
        unsafe { rustix::mm::mprotect(page, 4096, MprotectFlags::READ | MprotectFlags::EXEC) };
    if to_exec.err() != Some(Errno::NOSYS) {
        return 70;
    }
    // SAFETY: as above.
    if unsafe { rustix::mm::mprotect(page, 4096, MprotectFlags::READ) }.is_err() {
        return 71;
    }
    0
}

/// The same signals under the worker's landlock domain alone, as on
/// armv7: the domain's scope keeps them from a process outside it. A
/// kernel whose landlock has no scopes, before version 6, passes.
fn scoped(victim: i32) -> u8 {
    if vtium_image_helper::sandbox::landlock_version() < 6 {
        println!("the kernel's landlock has no scopes");
        return 0;
    }
    if vtium_image_helper::sandbox::lock_in() != Ok(true) {
        return 80;
    }
    let Some(pid) = rustix::process::Pid::from_raw(victim) else {
        return 81;
    };
    if rustix::process::kill_process(pid, rustix::process::Signal::KILL).err() != Some(Errno::PERM)
    {
        return 82;
    }
    let _ = signal_by_descriptor(victim);
    0
}

/// Runs this binary again as `role` against a `sleep` it starts, and
/// checks the `sleep` lives on with its limits unchanged.
fn against_a_victim(me: &std::path::Path, role: &str) -> Result<(), String> {
    let mut victim = Command::new("sleep")
        .arg("30")
        .spawn()
        .map_err(|e| format!("no sleep to attack: {e}"))?;
    let limits =
        |pid: u32| std::fs::read_to_string(format!("/proc/{pid}/limits")).unwrap_or_default();
    let before = limits(victim.id());
    let status = Command::new(me)
        .args([role, &victim.id().to_string()])
        .status()
        .map_err(|e| format!("the child does not start: {e}"))?;
    let alive = matches!(victim.try_wait(), Ok(None));
    let after = limits(victim.id());
    let _ = victim.kill();
    let _ = victim.wait();
    if !status.success() {
        return Err(format!("{status}"));
    }
    if !alive {
        return Err("the sleep was killed".to_owned());
    }
    if before != after {
        return Err("the sleep's limits changed".to_owned());
    }
    Ok(())
}

/// A worker locked in: the descriptor it opened before still reads, and
/// no file opens after. On a kernel without landlock it reports so, and
/// passes.
fn locked() -> u8 {
    let Ok(held) =
        std::fs::File::open("/etc/hostname").or_else(|_| std::fs::File::open("/etc/passwd"))
    else {
        return 50;
    };
    match vtium_image_helper::sandbox::lock_in() {
        Ok(true) => {}
        Ok(false) => {
            println!("the kernel offers no landlock");
            return 0;
        }
        Err(_) => return 51,
    }
    let mut byte = [0u8; 1];
    if rustix::io::pread(&held, &mut byte, 0).is_err() {
        return 52;
    }
    if std::fs::File::open("/etc/passwd").is_ok() {
        return 53;
    }
    let dir = std::env::temp_dir().join(format!("vtium-image-locked-{}", std::process::id()));
    if std::fs::write(&dir, b"x").is_ok() {
        let _ = std::fs::remove_file(&dir);
        return 54;
    }
    0
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--child") => return ExitCode::from(child(false)),
        Some("--worker") => return ExitCode::from(child(true)),
        Some("--last") => return ExitCode::from(last_filter()),
        Some("--locked") => return ExitCode::from(locked()),
        Some(role @ ("--attack" | "--scoped")) => {
            let victim = args.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
            let code = if role == "--attack" {
                attack(victim)
            } else {
                scoped(victim)
            };
            return ExitCode::from(code);
        }
        _ => {}
    }
    let me = std::env::current_exe().expect("the test's own path");
    let mut failed = false;
    for (role, name) in [
        ("--child", "a_call_outside_the_list_answers_enosys"),
        (
            "--worker",
            "a_worker_under_its_second_filter_starts_no_process",
        ),
        ("--last", "a_worker_past_its_last_filter_adds_no_filter"),
        (
            "--locked",
            "a_locked_in_worker_reads_what_it_holds_and_opens_nothing",
        ),
    ] {
        let status = Command::new(&me)
            .arg(role)
            .status()
            .expect("the child starts");
        let ok = status.success();
        let result = if ok {
            "ok".to_owned()
        } else {
            format!("FAILED ({status})")
        };
        println!("test {name} ... {result}");
        failed |= !ok;
    }
    for (role, name) in [
        (
            "--attack",
            "the_helpers_filter_keeps_a_worker_from_signalling_or_limiting_another_process",
        ),
        (
            "--scoped",
            "the_workers_landlock_scope_keeps_its_signals_from_another_process",
        ),
    ] {
        let result = against_a_victim(&me, role);
        let text = match &result {
            Ok(()) => "ok".to_owned(),
            Err(why) => format!("FAILED ({why})"),
        };
        println!("test {name} ... {text}");
        failed |= result.is_err();
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
