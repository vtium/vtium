//! The console binary this workspace builds, which `apply` checks
//! configuration files with.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The target directory the tests build the console into, their own:
/// `vtium-config-console` beside the profile directory this test binary
/// runs from. The running cargo holds the lock of its own build directory,
/// so the tests cannot build the console there.
pub fn own_target() -> PathBuf {
    let exe = std::env::current_exe().expect("the test knows its own path");
    // The test binary is <target>/<profile>/deps/<name>.
    exe.ancestors()
        .nth(3)
        .expect("the test runs from a cargo target directory")
        .join("vtium-config-console")
}

/// The console binary, built from the current sources once per test
/// binary. Every call builds it with cargo into [`own_target`], and cargo
/// rebuilds whatever changed since the last build, so the tests never run
/// a console built from older sources, such as a `target/debug/vtium` left
/// from an earlier build (vtium/vtium#357). Cargo's lock on the directory
/// makes test binaries that run at once build it one after another.
pub fn vtium() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let own = own_target();
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
