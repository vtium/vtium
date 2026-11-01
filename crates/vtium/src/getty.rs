//! The marker that keeps the unit's getty fallback off vtium's VT under a
//! closed policy (ADR-1401 D9). A getty's login on vtium's VT would take
//! any keyboard, so once the start has read a closed policy vtium writes
//! `/run/vtium/ttyN.closed` for its VT, and under any other policy it
//! removes that file, which an earlier closed run of this boot may have
//! left. The fallback unit, `kmsconvt-getty@.service`, skips the getty
//! while the file exists, so a hangup, a session that cannot spawn or a
//! panic under closed leave the VT with no login. vtium never reads the
//! file.
//!
//! The file sits beside the VT's run directory, `/run/vtium/ttyN/`, never
//! in it: systemd removes the unit's `RuntimeDirectory=` when vtium stops,
//! before the fallback unit checks its condition, and removes only the
//! innermost directory, so `/run/vtium/` stays. `/run` is cleared at every
//! boot, so a marker never outlives the boot that wrote it.

use std::io;
use std::path::{Path, PathBuf};

use rustix::fs::{Mode, OFlags};
use vtium_config::Policy;

/// The directory the marker lives in, the parent of every VT's run
/// directory (ADR-1401 D10).
pub const DIR: &str = "/run/vtium";

/// The marker of VT `vt` in `dir`: `ttyN.closed`, the name the unit's
/// instance gives the VT and the fallback unit's condition names.
pub fn path(dir: &Path, vt: u16) -> PathBuf {
    dir.join(format!("tty{vt}.closed"))
}

/// Writes the marker of VT `vt` in `dir` under the closed policy, and
/// removes it under the open one, each logged; a failure is logged and
/// never stops the start. Without a kernel VT, `vt` is `None` and no
/// getty can take one, so it does nothing.
pub fn mark(dir: &Path, vt: Option<u16>, policy: Policy) {
    let Some(vt) = vt else {
        return;
    };
    let marker = path(dir, vt);
    match policy {
        Policy::Closed => match write(dir, &marker) {
            Ok(()) => log::info!(
                "{} keeps the unit's getty off this VT under the closed policy",
                marker.display()
            ),
            Err(e) => log::error!(
                "cannot write {}: {e}; under the closed policy a failure of vtium starts a getty on this VT",
                marker.display()
            ),
        },
        Policy::Open => match std::fs::remove_file(&marker) {
            Ok(()) => log::info!(
                "{} removed: the head file's policy is open",
                marker.display()
            ),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => log::warn!(
                "cannot remove {}: {e}; a failure of vtium starts no getty on this VT",
                marker.display()
            ),
        },
    }
}

/// Creates `dir`, root's and writable by no one else, where it is missing,
/// as the unit's `RuntimeDirectory=` would, then the empty file `marker`,
/// following no symlink at its name.
fn write(dir: &Path, marker: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(dir)?;
    rustix::fs::open(
        marker,
        OFlags::WRONLY | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o644),
    )?;
    Ok(())
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test's set-up fails loudly")]
mod tests {
    use super::*;

    /// A stand-in for `/run/vtium`, of its own for each test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vtium-getty-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("run-vtium")
    }

    // covers: platform.heads.policy
    #[test]
    fn a_closed_file_writes_the_marker_of_its_vt() {
        let dir = scratch("closed");
        mark(&dir, Some(2), Policy::Closed);
        assert!(path(&dir, 2).is_file(), "the marker of tty2");
        assert_eq!(path(&dir, 2), dir.join("tty2.closed"));
        assert!(!path(&dir, 3).exists(), "another VT's is not written");
    }

    // covers: platform.heads.policy
    #[test]
    fn an_open_file_writes_no_marker() {
        let dir = scratch("open");
        mark(&dir, Some(2), Policy::Open);
        assert!(!path(&dir, 2).exists());
    }

    // covers: platform.heads.policy
    #[test]
    fn a_later_open_run_removes_an_earlier_closed_runs_marker() {
        let dir = scratch("stale");
        mark(&dir, Some(2), Policy::Closed);
        mark(&dir, Some(3), Policy::Closed);
        assert!(path(&dir, 2).is_file());
        mark(&dir, Some(2), Policy::Open);
        assert!(!path(&dir, 2).exists(), "the stale marker is removed");
        assert!(path(&dir, 3).is_file(), "another VT's stays");
    }

    #[test]
    fn a_closed_run_again_keeps_its_marker() {
        let dir = scratch("again");
        mark(&dir, Some(2), Policy::Closed);
        mark(&dir, Some(2), Policy::Closed);
        assert!(path(&dir, 2).is_file());
    }

    #[test]
    fn without_a_kernel_vt_nothing_is_written() {
        let dir = scratch("no-vt");
        mark(&dir, None, Policy::Closed);
        assert!(!dir.exists());
    }

    #[test]
    fn a_symlink_at_the_markers_name_is_not_followed() {
        let dir = scratch("link");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.with_file_name("target");
        std::os::unix::fs::symlink(&target, path(&dir, 2)).unwrap();
        mark(&dir, Some(2), Policy::Closed);
        assert!(!target.exists(), "nothing is written through the link");
    }
}
