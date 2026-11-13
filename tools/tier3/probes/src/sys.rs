//! What every probe shares: the answer lines, errors as text, dropping to
//! an account, and loading a library at run time.

use std::ffi::{CStr, CString, c_int, c_void};
use std::io::Write;

/// Prints one answer line, `ROOTCHECK <check> <key>: <value>`, which the
/// driver collects from the guest's event port.
pub fn answer(check: &str, key: &str, value: impl std::fmt::Display) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "ROOTCHECK {check} {key}: {value}");
    let _ = out.flush();
}

/// The text of the last error of a system call.
pub fn last_error() -> String {
    std::io::Error::last_os_error().to_string()
}

/// "ok" for a call that returned 0 or more, else the error's text.
pub fn result(ret: i64) -> String {
    if ret >= 0 {
        "ok".into()
    } else {
        format!("error: {}", last_error())
    }
}

/// The text of an errno value.
pub fn errno_text(errno: c_int) -> String {
    std::io::Error::from_raw_os_error(errno).to_string()
}

pub fn cstring(s: &str) -> CString {
    CString::new(s).unwrap_or_default()
}

/// An account's user id, group id and home, from the password database.
pub struct Account {
    pub uid: u32,
    pub gid: u32,
}

pub fn account(name: &str) -> Result<Account, String> {
    let cname = cstring(name);
    // SAFETY: getpwnam returns a pointer into static storage or null, and
    // the probe is single-threaded where it calls it.
    let pw = unsafe { libc::getpwnam(cname.as_ptr()) };
    if pw.is_null() {
        return Err(format!("no account {name}"));
    }
    // SAFETY: pw is not null, so it points at a valid passwd record.
    let pw = unsafe { &*pw };
    Ok(Account {
        uid: pw.pw_uid,
        gid: pw.pw_gid,
    })
}

/// Drops from root to `name`: no supplementary groups, then the group and
/// the user, real, effective and saved. A change of user from root clears
/// every capability. Returns the capabilities left, as /proc/self/status
/// gives them.
pub fn drop_to(name: &str) -> Result<String, String> {
    let acct = account(name)?;
    // SAFETY: plain system calls with valid arguments.
    unsafe {
        if libc::setgroups(0, std::ptr::null()) != 0 {
            return Err(format!("setgroups: {}", last_error()));
        }
        if libc::setresgid(acct.gid, acct.gid, acct.gid) != 0 {
            return Err(format!("setresgid: {}", last_error()));
        }
        if libc::setresuid(acct.uid, acct.uid, acct.uid) != 0 {
            return Err(format!("setresuid: {}", last_error()));
        }
    }
    Ok(identity())
}

/// This process's user ids and effective capabilities.
pub fn identity() -> String {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| {
        status
            .lines()
            .find(|l| l.starts_with(name))
            .map(|l| {
                l[name.len()..]
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    };
    format!(
        "uid={} gid={} groups=[{}] CapEff={}",
        field("Uid:"),
        field("Gid:"),
        field("Groups:"),
        field("CapEff:")
    )
}

/// A library opened with dlopen, so that the probe builds and links on a
/// machine that lacks it.
pub struct Library(*mut c_void);

impl Library {
    pub fn open(name: &str) -> Result<Library, String> {
        let cname = cstring(name);
        // SAFETY: dlopen with a valid C string.
        let handle = unsafe { libc::dlopen(cname.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            return Err(format!("dlopen {name}: {}", dl_error()));
        }
        Ok(Library(handle))
    }

    /// The symbol `name` as a function pointer of type `T`.
    ///
    /// # Safety
    ///
    /// `T` must be a function pointer type that matches the symbol's C
    /// signature.
    pub unsafe fn get<T: Copy>(&self, name: &str) -> Result<T, String> {
        assert_eq!(std::mem::size_of::<T>(), std::mem::size_of::<*mut c_void>());
        let cname = cstring(name);
        // SAFETY: dlsym on a handle dlopen returned.
        let sym = unsafe { libc::dlsym(self.0, cname.as_ptr()) };
        if sym.is_null() {
            return Err(format!("dlsym {name}: {}", dl_error()));
        }
        // SAFETY: the caller promises T is the symbol's function type, and
        // the sizes match.
        Ok(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&sym) })
    }
}

fn dl_error() -> String {
    // SAFETY: dlerror returns null or a valid C string.
    let e = unsafe { libc::dlerror() };
    if e.is_null() {
        "unknown error".into()
    } else {
        // SAFETY: not null, so a valid C string.
        unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned()
    }
}

/// The text of a C string pointer, or "(null)".
pub fn text(p: *const libc::c_char) -> String {
    if p.is_null() {
        "(null)".into()
    } else {
        // SAFETY: not null, and the callers pass C strings.
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

/// Runs a command and returns its exit status and output on one line.
pub fn run(argv: &[&str]) -> String {
    match std::process::Command::new(argv[0])
        .args(&argv[1..])
        .output()
    {
        Ok(out) => {
            let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&out.stderr));
            format!(
                "exit={} {}",
                out.status.code().map_or("signal".into(), |c| c.to_string()),
                one_line(&s)
            )
        }
        Err(e) => format!("cannot run {}: {e}", argv[0]),
    }
}

/// Text on one line, its lines joined with " | ".
pub fn one_line(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
}

/// Forks. The child runs `f` and exits with its status.
pub fn fork_run(f: impl FnOnce() -> i32) -> Result<libc::pid_t, String> {
    // SAFETY: the probe is single-threaded when it forks.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(format!("fork: {}", last_error()));
    }
    if pid == 0 {
        let code = f();
        let _ = std::io::stdout().flush();
        // SAFETY: ends the child without running the parent's destructors.
        unsafe { libc::_exit(code) };
    }
    Ok(pid)
}

/// Waits for a child and returns how it ended.
pub fn wait(pid: libc::pid_t) -> String {
    let mut status: c_int = 0;
    // SAFETY: waits for our own child.
    if unsafe { libc::waitpid(pid, &mut status, 0) } < 0 {
        return format!("waitpid: {}", last_error());
    }
    if libc::WIFEXITED(status) {
        format!("exit {}", libc::WEXITSTATUS(status))
    } else if libc::WIFSIGNALED(status) {
        format!("signal {}", libc::WTERMSIG(status))
    } else {
        format!("status {status}")
    }
}

/// A pipe, as (read end, write end).
pub fn pipe() -> (c_int, c_int) {
    let mut fds = [0 as c_int; 2];
    // SAFETY: pipe2 fills two descriptors.
    unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    (fds[0], fds[1])
}

/// Writes one byte to a pipe.
pub fn signal(fd: c_int) {
    let b = [1u8];
    // SAFETY: writes one byte from a valid buffer.
    unsafe { libc::write(fd, b.as_ptr().cast(), 1) };
}

/// Waits for one byte on a pipe; false at the pipe's end.
pub fn await_signal(fd: c_int) -> bool {
    let mut b = [0u8];
    // SAFETY: reads one byte into a valid buffer.
    unsafe { libc::read(fd, b.as_mut_ptr().cast(), 1) == 1 }
}
