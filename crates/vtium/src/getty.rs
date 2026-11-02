//! The marker that keeps the unit's getty fallback off vtium's VT under a
//! closed policy (ADR-1401 D9). A getty's login on vtium's VT would take
//! any keyboard, so once the start has read a closed policy vtium writes
//! `/run/vtium/ttyN.closed` for its VT, and under any other policy it
//! removes that file, which an earlier closed run of this boot may have
//! left. The fallback unit, `vtium-getty@.service`, skips the getty
//! while the file exists, so a hangup, a session that cannot spawn or a
//! panic under closed leave the VT with no login. vtium never reads the
//! file.
//!
//! vtium writes it before it opens its loop or its VT, so a failure at
//! either still leaves it, and a failure before vtium has read the head
//! file leaves an earlier closed run's marker of this boot where it is.
//!
//! The VT's run directory, `/run/vtium/ttyN/`, will be the unit's
//! `RuntimeDirectory=vtium/%i` (ADR-1401 D10, not built yet). systemd
//! removes only that innermost directory when vtium stops, before the
//! fallback unit checks its condition, so the marker sits one level up, in
//! `/run/vtium/`. `/run` is cleared at every boot, so a marker never
//! outlives the boot that wrote it.

use std::io;
use std::path::{Path, PathBuf};

use rustix::fs::{Mode, OFlags, major, minor};
use vtium_config::Policy;

/// The directory the marker lives in, the parent of every VT's run
/// directory (ADR-1401 D10).
pub const DIR: &str = "/run/vtium";

/// The marker of VT `vt` in `dir`: `ttyN.closed`, the name the unit's
/// instance gives the VT and the fallback unit's condition names.
pub fn path(dir: &Path, vt: u16) -> PathBuf {
    dir.join(format!("tty{vt}.closed"))
}

/// The kernel's major number of the VTs' devices, `/dev/ttyN`.
const VT_MAJOR: u32 = 4;

/// The highest VT number the kernel gives, `MAX_NR_CONSOLES`.
const VT_LAST: u32 = 63;

/// The number of the VT whose device is at `tty`, read from the device's
/// numbers, so a path of any spelling names its VT; `None` for a path that
/// is no VT, a pty or a serial line among them, or that cannot be read.
fn vt_number(tty: &Path) -> Option<u16> {
    let rdev = rustix::fs::stat(tty).ok()?.st_rdev;
    let vt = minor(rdev);
    (major(rdev) == VT_MAJOR && (1..=VT_LAST).contains(&vt))
        .then(|| u16::try_from(vt).ok())
        .flatten()
}

/// Marks the VT at `tty` in `dir` for `policy`, as `mark_vt` does. Without
/// a kernel VT, `tty` is `None` and no getty can take one, so it does
/// nothing. A `tty` that is no VT has no marker, and under the closed
/// policy vtium logs a warning that it wrote none.
pub fn mark(dir: &Path, tty: Option<&Path>, policy: Policy) {
    let Some(tty) = tty else {
        return;
    };
    match vt_number(tty) {
        Some(vt) => mark_vt(dir, vt, policy),
        None if matches!(policy, Policy::Closed) => log::warn!(
            "{} is no VT, so vtium writes no getty marker for it under the closed policy",
            tty.display()
        ),
        None => {}
    }
}

/// Writes the marker of VT `vt` in `dir` under the closed policy, and
/// removes it under the open one, each logged; a failure is logged and
/// never stops the start.
fn mark_vt(dir: &Path, vt: u16, policy: Policy) {
    let marker = path(dir, vt);
    match policy {
        Policy::Closed => match write(dir, &marker) {
            Ok(()) => log::info!(
                "{} keeps the unit's getty off this VT under the closed policy",
                marker.display()
            ),
            Err(e) => log::error!(
                "cannot write {}: {e}; if vtium fails, its unit starts a getty on this VT, though the policy is closed",
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
        mark_vt(&dir, 2, Policy::Closed);
        assert!(path(&dir, 2).is_file(), "the marker of tty2");
        assert_eq!(path(&dir, 2), dir.join("tty2.closed"));
        assert!(!path(&dir, 3).exists(), "another VT's is not written");
    }

    // covers: platform.heads.policy
    #[test]
    fn an_open_file_writes_no_marker() {
        let dir = scratch("open");
        mark_vt(&dir, 2, Policy::Open);
        assert!(!path(&dir, 2).exists());
    }

    // covers: platform.heads.policy
    #[test]
    fn a_later_open_run_removes_an_earlier_closed_runs_marker() {
        let dir = scratch("stale");
        mark_vt(&dir, 2, Policy::Closed);
        mark_vt(&dir, 3, Policy::Closed);
        assert!(path(&dir, 2).is_file());
        mark_vt(&dir, 2, Policy::Open);
        assert!(!path(&dir, 2).exists(), "the stale marker is removed");
        assert!(path(&dir, 3).is_file(), "another VT's stays");
    }

    #[test]
    fn a_closed_run_again_keeps_its_marker() {
        let dir = scratch("again");
        mark_vt(&dir, 2, Policy::Closed);
        mark_vt(&dir, 2, Policy::Closed);
        assert!(path(&dir, 2).is_file());
    }

    #[test]
    fn without_a_kernel_vt_nothing_is_written() {
        let dir = scratch("no-vt");
        mark(&dir, None, Policy::Closed);
        assert!(!dir.exists());
    }

    #[test]
    fn a_path_that_is_no_vt_is_warned_of_under_closed() {
        let dir = scratch("no-vt-path");
        logged();
        mark(&dir, Some(Path::new("/dev/null")), Policy::Closed);
        assert!(!dir.exists(), "no marker is written");
        assert_eq!(
            logged(),
            ["/dev/null is no VT, so vtium writes no getty marker for it under the closed policy"]
        );
        mark(&dir, Some(Path::new("/dev/null")), Policy::Open);
        assert_eq!(logged(), Vec::<String>::new(), "nothing to keep off");
    }

    /// The log lines this thread wrote since the last call.
    fn logged() -> Vec<String> {
        use std::cell::RefCell;
        use std::sync::Once;
        thread_local! {
            static LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        }
        struct Capture;
        impl log::Log for Capture {
            fn enabled(&self, _: &log::Metadata<'_>) -> bool {
                true
            }
            fn log(&self, record: &log::Record<'_>) {
                LINES.with(|l| l.borrow_mut().push(record.args().to_string()));
            }
            fn flush(&self) {}
        }
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            log::set_logger(&Capture).unwrap();
            log::set_max_level(log::LevelFilter::Info);
        });
        LINES.with(|l| std::mem::take(&mut *l.borrow_mut()))
    }

    #[test]
    fn a_path_that_is_no_vt_has_no_number() {
        assert_eq!(vt_number(Path::new("/dev/null")), None);
        assert_eq!(vt_number(Path::new("/nonexistent/tty3")), None);
        assert_eq!(
            vt_number(Path::new("/dev/tty0")),
            None,
            "the VT shown, no one VT"
        );
        // a stat reads the node's numbers and opens nothing
        if Path::new("/dev/tty3").exists() {
            assert_eq!(vt_number(Path::new("/dev/tty3")), Some(3));
        }
    }

    /// The value of the first line of `unit` that sets `key`.
    fn setting<'a>(unit: &'a str, key: &str) -> Option<&'a str> {
        unit.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
    }

    // The VT unit's failure starts the fallback, and the fallback's
    // condition names the very file `mark` writes, with the unit's
    // instance, `%i`, standing for `ttyN`.
    // covers: platform.heads.policy
    #[test]
    fn the_units_skip_the_getty_on_the_file_mark_writes() {
        let vt = include_str!("../install/vtium@.service");
        let fallback = include_str!("../install/vtium-getty@.service");
        assert_eq!(setting(vt, "OnFailure"), Some("vtium-getty@%i.service"));
        let marker = path(Path::new(DIR), 7);
        let marker = marker.to_str().unwrap().replace("tty7", "%i");
        assert_eq!(
            setting(fallback, "ConditionPathExists"),
            Some(format!("!{marker}").as_str())
        );
        assert_eq!(
            setting(fallback, "ExecStart"),
            Some("systemctl --no-block start getty@%i.service")
        );
    }

    // The start writes the marker from `main`, ahead of the loop and the VT
    // it opens in `run`, so a failure at either still leaves it. `main` is
    // the binary's wiring, which no test drives, so this reads its text.
    #[test]
    fn main_marks_before_it_opens_the_loop_or_the_vt() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("fn main() -> ExitCode {")
            .and_then(|(_, rest)| rest.split_once("\nfn "))
            .map(|(body, _)| body)
            .unwrap();
        assert_eq!(source.matches("getty::mark(").count(), 1, "one call");
        let mark = body.find("vtium::getty::mark(").unwrap();
        let call = &body[mark..];
        let call = &call[..call.find(");").unwrap()];
        assert!(
            call.contains("path.as_deref()"),
            "it marks the VT at the path vtium opens"
        );
        assert!(!body.contains("Loop::new"), "main opens no loop before it");
        // Each call of `run_kernel` or `run`, a name that ends in either
        // and is not part of a longer name.
        let calls: Vec<usize> = ["run_kernel(", "run("]
            .iter()
            .flat_map(|name| body.match_indices(name).map(|(at, _)| at))
            .filter(|&at| {
                !body[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':')
            })
            .collect();
        assert!(calls.len() >= 2, "main calls run_kernel and run");
        assert!(
            calls.iter().all(|&at| mark < at),
            "main marks before it calls run_kernel or run"
        );
    }

    #[test]
    fn a_symlink_at_the_markers_name_is_not_followed() {
        let dir = scratch("link");
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.with_file_name("target");
        std::os::unix::fs::symlink(&target, path(&dir, 2)).unwrap();
        mark_vt(&dir, 2, Policy::Closed);
        assert!(!target.exists(), "nothing is written through the link");
    }
}
