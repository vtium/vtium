//! A stand-in program that a test writes and then runs, written so that
//! the kernel never refuses to run it as busy.

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Stdio};

/// Writes `text` to `path` as a program with mode 0755, which the test may
/// run as soon as this returns.
///
/// Linux refuses to run a file that any process holds open for writing,
/// and fails with ETXTBSY ("Text file busy"). If this process wrote the
/// file itself, a child that another test thread forks during the write
/// would get a copy of the open descriptor and keep it until that child
/// runs its own program. Closing the file here does not end the child's
/// hold, and neither does writing a temporary file and renaming it into
/// place, because the child still holds the same file under its new name.
/// So this process never opens the file: a `sh` of its own writes it with
/// `cat` from a pipe, and has exited before this function returns.
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
