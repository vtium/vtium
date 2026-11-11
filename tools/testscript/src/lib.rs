//! A stand-in program that a test writes or copies and then runs, put in
//! place so that the kernel never refuses to run it as busy.
//!
//! This crate serves the tests of every crate that runs a program it
//! made: vtium-config's, vtium's and vtium-platform's, each of which names
//! it as a dev-dependency (vtium/vtium#339).
//!
//! Linux refuses to run a file that any process holds open for writing,
//! and fails with ETXTBSY ("Text file busy"). If the test process wrote
//! the file itself, a child that another test thread forks during the
//! write would get a copy of the open descriptor and keep it until that
//! child runs its own program. Closing the file does not end the child's
//! hold, and neither does writing a temporary file and renaming it into
//! place, because the child still holds the same file under its new name.
//! So the test process never opens the file to write: a process of its
//! own writes it, and has exited before these functions return.

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

/// Writes `text` to `path` as a program with mode 0755, which the test may
/// run as soon as this returns. A `sh` of its own writes the file with
/// `cat` from a pipe.
pub fn executable(path: &Path, text: &str) {
    let mut writer = Command::new("sh")
        .arg("-c")
        .arg("cat > \"$1\"")
        .arg("sh")
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .expect("sh starts");
    // The pipe closes when its end is dropped here, and cat then sees the
    // end of the text.
    writer
        .stdin
        .take()
        .expect("the pipe to sh")
        .write_all(text.as_bytes())
        .expect("the program's text");
    let status = writer.wait().expect("sh ends");
    assert!(status.success(), "sh wrote {}: {status}", path.display());
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .expect("the program's mode");
}

/// Copies the program `from` to `to` with mode 0755, which the test may
/// run as soon as this returns. `cp` writes the copy.
pub fn executable_copy(from: &Path, to: &Path) {
    let status = Command::new("cp")
        .arg(from)
        .arg(to)
        .status()
        .expect("cp starts");
    assert!(
        status.success(),
        "cp {} {}: {status}",
        from.display(),
        to.display()
    );
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o755))
        .expect("the program's mode");
}

/// Why the helpers cannot run here, when the tests run under qemu-user:
/// `Some` with the reason when `VTIUM_QEMU_USER` names the qemu that
/// `make arm-test` runs them under (tools/arm/README.md) and a seccomp
/// filter cannot be installed, else `None`. qemu-user implements no
/// seccomp, since a filter would bind qemu itself, and the font helper
/// and the image helper each refuse to run without their filters. A test
/// that starts a real helper cannot mean anything there, and it skips and
/// says why. The probe installs a filter that allows every call, on a
/// thread of its own that ends straight after, and it runs only under
/// qemu, so the tests run in full everywhere else, CI's arm-native job
/// included.
pub fn qemu_without_seccomp() -> Option<String> {
    let qemu = std::env::var_os("VTIUM_QEMU_USER")?;
    let refused = seccomp_refused()?;
    Some(format!(
        "{} implements no seccomp ({refused}), so the helpers refuse to run",
        qemu.to_string_lossy()
    ))
}

/// The error a filter that allows every call meets, if any.
#[cfg(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
))]
fn seccomp_refused() -> Option<String> {
    // BPF_RET | BPF_K, SECCOMP_RET_ALLOW
    let allow = vec![seccompiler::sock_filter {
        code: 0x06,
        jt: 0,
        jf: 0,
        k: 0x7fff_0000,
    }];
    std::thread::spawn(move || {
        seccompiler::apply_filter(&allow)
            .err()
            .map(|e| e.to_string())
    })
    .join()
    .unwrap_or_else(|_| Some("the probe's thread panicked".to_owned()))
}

/// On an architecture seccompiler builds no filter for, the helpers run
/// without one, so nothing stops them.
#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
fn seccomp_refused() -> Option<String> {
    None
}
