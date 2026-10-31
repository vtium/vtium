//! The words of the overlay screens (ADR-1401 D5, D7, D8, D9), each from
//! the console function that gives it, so a reworded notice moves its
//! golden. The names and the device are the screens' own: the head `desk`,
//! active and showing console 2 of 3, the head `side` beside it, and the
//! keyboard at `/dev/input/event3`.

use std::sync::atomic::{AtomicU64, Ordering};

use vtium::notice;

/// The keyboard the rescue key moves.
const KEYBOARD: &str = "/dev/input/event3";

/// A head file with one error: a rule naming a head the file does not
/// declare.
const ONE_ERROR: &str = "version = 1\n[head desk]\n[rules]\ndisplay * -> desc\n";

/// A head file with two errors: the same rule, and a `[head]` key vtium
/// does not know.
const TWO_ERRORS: &str =
    "version = 1\n[head desk]\nfont-weight = bold\n[rules]\ndisplay * -> desc\n";

/// `osd = full`'s line on the active head.
pub fn osd_active() -> Result<String, String> {
    Ok(notice::osd_line(true, "desk", 2, 3))
}

/// `osd = full`'s line on a head that is not the active one.
pub fn osd_other() -> Result<String, String> {
    Ok(notice::osd_line(false, "side", 1, 1))
}

pub fn no_keyboard() -> Result<String, String> {
    Ok(notice::NO_KEYBOARD.to_owned())
}

pub fn anti_lockout() -> Result<String, String> {
    Ok(notice::anti_lockout("desk"))
}

/// The rescue key's notice while `desk` is active.
pub fn rescue() -> Result<String, String> {
    Ok(notice::rescue(KEYBOARD, "desk"))
}

/// The rescue key's notice while no screen is active.
pub fn rescue_no_head() -> Result<String, String> {
    Ok(notice::rescue_no_head(KEYBOARD))
}

pub fn oneshot() -> Result<String, String> {
    Ok(notice::session_ended())
}

pub fn head_file_one_error() -> Result<String, String> {
    head_file(ONE_ERROR)
}

pub fn head_file_two_errors() -> Result<String, String> {
    head_file(TWO_ERRORS)
}

/// The notice the start shows for the head file `text`, as
/// [`vtium::head::rules`] reads it. The file is written to a scratch
/// directory of this process's own, which the notice names; the notice
/// names vtium's head file in its place, as the start shows it.
fn head_file(text: &str) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "vtium-tier2-heads-{}-{}",
        std::process::id(),
        CALLS.fetch_add(1, Ordering::Relaxed)
    ));
    let path = dir.join("heads.conf");
    let read = || {
        std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        let uid = std::fs::metadata(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .uid();
        let trust = vtium_config::Trust::owner(uid).below(&dir);
        let (_, shown) = vtium::head::rules(&path, &trust);
        shown.ok_or_else(|| format!("the head file {text:?} shows no notice"))
    };
    let shown = read();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(shown?.replace(&*path.to_string_lossy(), vtium_config::HEAD_FILE))
}
