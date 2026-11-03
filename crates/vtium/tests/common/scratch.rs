//! A scratch directory of a test's own, removed when it is dropped, as its
//! test ends, whether the test passes or fails. A test declares it before
//! the seat whose sessions write into it, so the seat, dropped first, ends
//! its sessions before the directory goes.

use std::ops::Deref;
use std::path::{Path, PathBuf};

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
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
