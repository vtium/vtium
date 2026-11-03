//! A scratch directory of a test's own, removed when it is dropped, as its
//! test ends, whether the test passes or fails. A session or helper the
//! test started can still be running then: a seat's pty, dropped, sends
//! its session SIGHUP and closes, and nothing waits for the session to
//! exit. So the guard first renames the directory to a fresh name, and
//! then removes the renamed one: a write the session makes by the old
//! path after the rename fails with ENOENT, and so cannot land in the
//! directory while it is being removed or bring it back afterwards.

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Scratch(PathBuf);

impl Scratch {
    /// The empty directory `vtium-<name>-<pid>` under the build's own
    /// temporary directory. It is on disk with the build, and not in
    /// `/tmp`, a RAM-backed tmpfs that every process of the machine shares.
    pub fn new(name: &str) -> Scratch {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("vtium-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test's scratch directory");
        Scratch(dir)
    }
}

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(retire(&self.0));
    }
}

/// Renames `dir` to a name no path a test handed out names, beside it,
/// and returns the new path, which the caller removes; when the rename
/// fails, as for a directory already gone, returns `dir` itself.
pub fn retire(dir: &Path) -> PathBuf {
    static RETIRED: AtomicUsize = AtomicUsize::new(0);
    let n = RETIRED.fetch_add(1, Ordering::Relaxed);
    let mut name = dir.file_name().unwrap_or_default().to_owned();
    name.push(format!(".gone-{n}"));
    let gone = dir.with_file_name(name);
    match std::fs::rename(dir, &gone) {
        Ok(()) => gone,
        Err(_) => dir.to_path_buf(),
    }
}
