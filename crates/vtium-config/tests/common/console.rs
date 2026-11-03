//! The console binary this workspace builds, which `apply` checks
//! configuration files with.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The console binary this workspace builds. A workspace test run has
/// built it for the console's own tests; a run of this crate alone builds
/// it once, in a target directory of its own, since the running cargo
/// holds the lock of the one it was started with.
pub fn vtium() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let target = std::env::var_os("CARGO_TARGET_DIR")
                .map_or_else(|| root.join("target"), PathBuf::from);
            let bin = target.join("debug/vtium");
            if bin.is_file() {
                return bin;
            }
            let own = target.join("vtium-config-console");
            let status = Command::new(env!("CARGO"))
                .args([
                    "build",
                    "-q",
                    "-p",
                    "vtium",
                    "--bin",
                    "vtium",
                    "--target-dir",
                ])
                .arg(&own)
                .current_dir(&root)
                .status()
                .expect("cargo runs");
            assert!(status.success(), "the console builds");
            own.join("debug/vtium")
        })
        .clone()
}
