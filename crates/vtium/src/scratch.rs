//! A directory of a unit test's own under the system's temporary
//! directory. The test holds a [`Scratch`] guard, and the guard removes
//! the directory when it is dropped, as the test ends, whether the test
//! passes or fails. The integration tests have a guard of their own,
//! `tests/common/scratch.rs`, under the build's own temporary directory.

use std::ops::Deref;
use std::path::{Path, PathBuf};

pub struct Scratch(PathBuf);

impl Scratch {
    /// The empty directory `vtium-<name>-<pid>` under the system's
    /// temporary directory. A directory left at that path by an earlier
    /// process with the same id is removed first.
    #[expect(clippy::panic, reason = "a test's set-up fails loudly")]
    pub fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("vtium-{name}-{}", std::process::id()));
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
    /// Removes the directory. A test can take its own permissions on the
    /// directory away, and a test that fails before it gives them back
    /// would leave a directory nobody can list, so the guard gives them
    /// back first.
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A machine's places for the batch under `dir`: the batch file
/// `etc/batch.conf`, root's lists in `etc/batch.d`, the packages' lists in
/// `share/batch.d` beside the package's font folder `share/fonts`, and no
/// console font. The folders it names do not exist until a test writes
/// into them with [`file`].
pub fn batch_places(dir: &Path) -> vtium_settings::batch::Places {
    vtium_settings::batch::Places {
        file: dir.join("etc/batch.conf"),
        vconsole_conf: PathBuf::from("/dev/null"),
        cmdline: PathBuf::from("/dev/null"),
        fonts: dir.join("share/fonts"),
        palettes: dir.join("share/palettes"),
        ..vtium_settings::batch::Places::system()
    }
}

/// Writes `text` to `path` with the mode `mode`, after making each folder
/// on the way with the mode 0755, so that the owner and mode check passes
/// the folders.
#[expect(clippy::panic, reason = "a test's set-up fails loudly")]
pub fn file(path: &Path, text: &[u8], mode: u32) {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let made = path.parent().map_or(Ok(()), |folder| {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o755)
            .create(folder)
    });
    let written = made
        .and_then(|()| std::fs::write(path, text))
        .and_then(|()| std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)));
    if let Err(e) = written {
        panic!("{}: {e}", path.display());
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::Scratch;

    // A test that panics after it made its directory still leaves no
    // directory behind: the guard's drop runs as the panic unwinds.
    #[test]
    fn the_directory_is_removed_when_the_test_panics() {
        let (tx, rx) = std::sync::mpsc::channel();
        let ended = std::thread::spawn(move || {
            let dir = Scratch::new("scratch-panics");
            std::fs::write(dir.join("file"), "x").unwrap();
            tx.send(dir.to_path_buf()).unwrap();
            panic!("the test fails after it made its directory");
        })
        .join();
        assert!(ended.is_err(), "the test panicked");
        let dir = rx.recv().unwrap();
        assert!(!dir.exists(), "{} is left behind", dir.display());
    }
}
