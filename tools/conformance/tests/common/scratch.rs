//! A directory of an integration test's own, removed when the test drops
//! its guard, as the test ends, whether the test passes or fails. The
//! directory sits under the build's own temporary directory,
//! `CARGO_TARGET_TMPDIR`, so a test leaves nothing in `/tmp`.

use std::ops::Deref;
use std::path::{Path, PathBuf};

pub struct Scratch(PathBuf);

impl Scratch {
    /// The empty directory `vtium-<name>-<pid>` under the build's own
    /// temporary directory. A directory left at that path by an earlier
    /// process with the same id is removed first.
    pub fn new(name: &str) -> Scratch {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("vtium-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            panic!("{}: {e}", dir.display());
        }
        Scratch(dir)
    }
}

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
