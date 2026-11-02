//! The glyph helper's stand-in for the integration tests that start one:
//! vtium-vitrify's real serve mode, run by the test binary itself in a
//! process of its own, so each answer is the one the real helper gives.
//! Each test binary that includes this module has its own
//! [`the_helper_stand_in`], which its [`real_helper`] starts.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The environment variable that makes [`the_helper_stand_in`] serve, as
/// the stand-in for the glyph helper, instead of passing as a test.
const STAND_IN: &str = "VTIUM_TEST_HELPER";

/// The glyph helper the outline tests run: this test binary again, its
/// [`the_helper_stand_in`] alone, which serves with vtium-vitrify's real
/// serve mode, so each answer is the one the real helper gives. With
/// `go`, a FIFO, its first answer waits until the test opens `go` to
/// write, and the kind of each frame it sends is appended to `kinds`, one
/// line each. The module is included as `stand_in`, the name the test
/// harness is asked to run. The test harness writes
/// to standard output, so the frames go out through descriptor 3, which
/// the shell points at the console's pipe.
pub fn real_helper(kinds: &Path, go: Option<&Path>) -> vtium::helper::Command {
    let mode = format!(
        "{}:{}",
        kinds.display(),
        go.map(|g| g.display().to_string()).unwrap_or_default()
    );
    let exe = std::env::current_exe().expect("the test binary");
    vtium::helper::Command {
        program: PathBuf::from("/bin/sh"),
        args: [
            "-c",
            "VTIUM_TEST_HELPER=\"$1\" exec \"$0\" --exact stand_in::the_helper_stand_in -q 3>&1 >/dev/null",
        ]
        .into_iter()
        .map(OsString::from)
        .chain([exe.into_os_string(), OsString::from(mode)])
        .collect(),
        account: None,
    }
}

/// What the stand-in writes to the console: each frame's kind logged, and
/// with `go`, a FIFO, nothing sent until the test opens it to write.
struct Logged {
    out: std::fs::File,
    kinds: PathBuf,
    go: Option<PathBuf>,
    pending: Vec<u8>,
}

impl std::io::Write for Logged {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if let Some(go) = self.go.take() {
            // the stand-in's only wait: opening a FIFO to read blocks until
            // the test opens it to write, once the console has stopped
            // waiting for the answer
            std::fs::File::open(&go)?;
        }
        self.pending.extend_from_slice(bytes);
        while let [k0, k1, k2, k3, l0, l1, l2, l3, ..] = *self.pending.as_slice() {
            let len = u32::from_le_bytes([l0, l1, l2, l3]) as usize;
            if self.pending.len() < 8 + len {
                break;
            }
            let kind = u32::from_le_bytes([k0, k1, k2, k3]);
            let mut log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.kinds)?;
            writeln!(log, "{kind}")?;
            self.pending.drain(..8 + len);
        }
        self.out.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

/// Not a test of the console: run by [`real_helper`] with [`STAND_IN`]
/// set, it serves as the glyph helper and exits with serve's status; run
/// by the test harness, it does nothing.
#[test]
fn the_helper_stand_in() {
    let Ok(mode) = std::env::var(STAND_IN) else {
        return;
    };
    let (kinds, go) = mode.split_once(':').expect("the stand-in's mode");
    let out = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/fd/3")
        .expect("the console's pipe");
    let mut out = Logged {
        out,
        kinds: PathBuf::from(kinds),
        go: (!go.is_empty()).then(|| PathBuf::from(go)),
        pending: Vec::new(),
    };
    let status = vtium_vitrify::serve::serve(
        std::io::stdin(),
        &mut out,
        &vtium_vitrify::serve::Paths::default(),
        Duration::from_secs(5),
    );
    std::process::exit(i32::from(status));
}
