//! testfont: writes the test fonts into a directory.
//!
//! ```text
//! testfont DIR
//! ```
//!
//! Writes `vtium-test-8x8.psf` (PSF2) and `vtium-test-8x16.psf` (PSF1),
//! the same bytes on every run, and exits 0, or 2 on a usage or write error.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir] = args.as_slice() else {
        eprintln!("usage: testfont DIR");
        return ExitCode::from(2);
    };
    let dir = PathBuf::from(dir);
    let written = vtium_testfont::files().and_then(|files| {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        files.iter().try_for_each(|(name, bytes)| {
            let path = dir.join(name);
            std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
        })
    });
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("testfont: {e}");
            ExitCode::from(2)
        }
    }
}
