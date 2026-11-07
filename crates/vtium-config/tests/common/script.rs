//! A stand-in program that a test writes or copies and then runs, put in
//! place so that the kernel never refuses to run it as busy.
//!
//! This one file serves the tests of every crate that runs a program it
//! made: vtium-config's tests, and vtium's and vtium-platform's, which
//! include it with `#[path]`. Each uses the functions it needs, so the
//! others are dead code there.
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

#![allow(dead_code)]

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
