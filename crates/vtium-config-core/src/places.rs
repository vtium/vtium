//! Where vtium-config reads and saves: the directories and programs the
//! command line names, the defaults of the directory of vtium.conf and the
//! drafts' directory, and the bounded read of a configuration file. The
//! commands and the TUI share them.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The most bytes the TUI reads of vtium.conf.
pub const CONF_MAX: u64 = 1 << 20;

/// Where the TUI reads from and saves to, as the command line names them.
#[derive(Clone, Debug)]
pub struct Paths {
    /// The directory of vtium.conf.
    pub configdir: PathBuf,
    /// The frits' directory, or why there is none.
    pub frits: crate::switch::Frits,
    /// The directory drafts go under.
    pub drafts: PathBuf,
    /// The program that runs `apply` as root: `sudo`, or a test's stand-in.
    pub sudo: OsString,
    /// The console binary `apply` checks configuration files with.
    pub vtium: OsString,
    /// The glyph helper that draws specimens and finds a frit's sources.
    pub helper: crate::specimen::Helper,
    /// sysfs, where the displays' modes and EDIDs are read: `/sys`.
    pub sys: PathBuf,
}

impl Paths {
    /// Where a save installs, as `apply` takes it.
    pub fn targets(&self) -> crate::apply::Targets {
        crate::apply::Targets {
            configdir: self.configdir.clone(),
            frits: self.frits.clone(),
            vtium: self.vtium.clone(),
        }
    }
}

/// The directory drafts go under: `$XDG_STATE_HOME/vtium/drafts`, else
/// `~/.local/state/vtium/drafts`.
pub fn default_drafts() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from(".local/state"));
    state.join("vtium/drafts")
}

/// The directory of vtium.conf, as the console's.
pub fn default_configdir() -> PathBuf {
    PathBuf::from("/etc/vtium")
}

/// A configuration file's text, `None` when it does not exist.
pub fn read_conf(path: &Path) -> Result<Option<String>, String> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let mut bytes = Vec::new();
    file.take(CONF_MAX.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > CONF_MAX {
        return Err(format!(
            "{} is too large: the limit is {CONF_MAX} bytes",
            path.display()
        ));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{} is not UTF-8 text", path.display()))
}
