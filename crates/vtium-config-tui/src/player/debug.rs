//! Debug mode's folder. vtium-config creates the folder and refuses one
//! that exists, so one run's files never mix with another's. After each
//! step the player writes the screen as text to `step-NNNN.txt`, numbered
//! by the macro's line. Each `Screenshot NAME` writes `NAME.txt`, a live
//! run's log lines go to `live.log`, and a failure writes `failure.txt`.
//!
//! References:
//! - ADR-1208 D6: `--macro-debug DIR`, its files, and the folder that
//!   must not exist.

use std::io::Write;
use std::path::{Path, PathBuf};

use vtium_config_macro::Name;

/// The folder of one debug run.
#[derive(Debug)]
pub struct Debug {
    dir: PathBuf,
}

impl Debug {
    /// Creates `dir` for a run, and the folders above it where they are
    /// missing. `Err` says why: `dir` exists already, or cannot be made.
    pub fn create(dir: &Path) -> Result<Debug, String> {
        if dir.symlink_metadata().is_ok() {
            return Err(format!(
                "vtium-config: {} exists already: --macro-debug needs a folder that does not exist yet.",
                dir.display()
            ));
        }
        if let Some(parent) = dir.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| cannot(dir, &e))?;
        }
        // create_dir, not create_dir_all, so a folder made in between is
        // refused too
        std::fs::create_dir(dir).map_err(|e| cannot(dir, &e))?;
        Ok(Debug {
            dir: dir.to_path_buf(),
        })
    }

    /// The folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes the screen after the step on `line` to `step-NNNN.txt`.
    pub fn step(&self, line: usize, rows: &[String]) -> Result<(), String> {
        self.write(&format!("step-{line:04}.txt"), &screen_text(rows))
    }

    /// Writes a `Screenshot NAME` to `NAME.txt`.
    pub fn screenshot(&self, name: &Name, rows: &[String]) -> Result<(), String> {
        self.write(&format!("{}.txt", name.as_str()), &screen_text(rows))
    }

    /// Writes the failure report to `failure.txt`.
    pub fn failure(&self, report: &str) -> Result<(), String> {
        self.write("failure.txt", &format!("{report}\n"))
    }

    /// Adds a live run's log line to `live.log`.
    pub fn live(&self, line: &str) -> Result<(), String> {
        let path = self.dir.join("live.log");
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| writeln!(f, "{line}"))
            .map_err(|e| cannot(&path, &e))
    }

    fn write(&self, file: &str, text: &str) -> Result<(), String> {
        let path = self.dir.join(file);
        std::fs::write(&path, text).map_err(|e| cannot(&path, &e))
    }
}

/// The screen as text, one line for each row.
fn screen_text(rows: &[String]) -> String {
    let mut out = rows.join("\n");
    out.push('\n');
    out
}

fn cannot(path: &Path, e: &std::io::Error) -> String {
    format!("vtium-config cannot write {}: {e}", path.display())
}
