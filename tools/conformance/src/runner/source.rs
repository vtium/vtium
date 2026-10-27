//! The pinned commit's files, read from a checkout with git and written
//! under the runner's output directory, so the checkout's own branch and
//! working tree never reach the judge.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::lists::Pin;

/// Extracts `paths` of the pinned commit from the checkout at `repo` into
/// `out/<name>-<commit>/`, once, and returns that directory.
pub fn extract(repo: &Path, pin: &Pin, paths: &[&str], out: &Path) -> Result<PathBuf, String> {
    let dest = out.join(format!("{}-{}", pin.name, pin.commit));
    let done = dest.join(".extracted");
    if done.exists() {
        return Ok(dest);
    }
    let has = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["cat-file", "-e", &format!("{}^{{commit}}", pin.commit)])
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("git: {e}"))?;
    if !has.success() {
        return Err(format!(
            "{} has no commit {}: clone {} there, or fetch it (git -C {} fetch origin)",
            repo.display(),
            pin.commit,
            pin.url,
            repo.display()
        ));
    }
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    std::fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let mut git = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["archive", "--format=tar", &pin.commit, "--"])
        .args(paths)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("git archive: {e}"))?;
    let stdout = git.stdout.take().ok_or("git archive: no output")?;
    let tar = Command::new("tar")
        .arg("-x")
        .arg("-C")
        .arg(&dest)
        .stdin(stdout)
        .status()
        .map_err(|e| format!("tar: {e}"))?;
    let git = git.wait().map_err(|e| format!("git archive: {e}"))?;
    if !git.success() || !tar.success() {
        return Err(format!(
            "extracting {} at {} failed",
            paths.join(" "),
            pin.commit
        ));
    }
    std::fs::write(&done, &pin.commit).map_err(|e| format!("{}: {e}", done.display()))?;
    Ok(dest)
}
