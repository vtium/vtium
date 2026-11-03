//! The TUI that `vtium config` opens at a terminal (ADR-1201 D3, D4, D7):
//! a full-screen program drawn in the terminal's text cells, which lists
//! the frits (font set files) and the built-in fonts, tries each font the
//! user highlights on the live console through OSC 50, and saves what the
//! user keeps through `apply`.
//!
//! [`model`] is the state machine, [`view`] its screen, [`input`] the
//! decoder of what the terminal sends, [`words`] the catalog of what the
//! TUI says, and [`port`] the loop and the terminal it runs on. [`gather`]
//! reads what the TUI starts from: the frits as text, never a font file
//! (D3), vtium.conf and the head file.

pub mod input;
pub mod model;
pub mod port;
pub mod view;
pub mod words;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use vtium_font::set::builtin;

use crate::frit::{Frit, RESERVED, valid_name};
use model::{DpiFrom, Font, FontState, Inputs};

/// The most bytes the TUI reads of vtium.conf or the head file.
pub const CONF_MAX: u64 = 1 << 20;

/// Where the TUI reads from and saves to, as the command line names them.
#[derive(Clone, Debug)]
pub struct Paths {
    /// The directory of vtium.conf and the head file.
    pub configdir: PathBuf,
    pub frits: PathBuf,
    /// The directory drafts go under.
    pub drafts: PathBuf,
    /// The program that runs `apply` as root: `sudo`, or a test's stand-in.
    pub sudo: OsString,
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

/// A configuration file's text, `None` when it does not exist.
fn read_conf(path: &Path) -> Result<Option<String>, String> {
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
        return Err(format!("{}: larger than {CONF_MAX} bytes", path.display()));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{}: not UTF-8 text", path.display()))
}

/// The frits in `dir` and the built-in fonts, sorted by name, each with
/// its state. A frit is read as text, as the console reads it.
pub fn fonts(dir: &Path) -> Vec<Font> {
    let mut out: Vec<Font> = RESERVED
        .iter()
        .filter(|n| builtin(n).is_some())
        .map(|n| Font {
            name: (*n).to_owned(),
            state: FontState::BuiltIn,
        })
        .collect();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".vtfrit"))
            else {
                continue;
            };
            if !valid_name(name) || RESERVED.contains(&name) {
                continue;
            }
            if !std::fs::metadata(&path).is_ok_and(|m| m.is_file()) {
                continue;
            }
            let state = match Frit::load(&path) {
                Ok(_) => FontState::Reads,
                Err(e) if e.line == 0 => FontState::Refused(e.why.clone()),
                Err(e) => FontState::Refused(format!("line {}: {}", e.line, e.why)),
            };
            out.push(Font {
                name: name.to_owned(),
                state,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// What the TUI starts from, read from `paths`.
pub fn gather(paths: &Paths) -> Result<Inputs, String> {
    let conf_path = paths.configdir.join("vtium.conf");
    let heads_path = paths.configdir.join("heads.conf");
    let conf = read_conf(&conf_path)?;
    let heads = read_conf(&heads_path)?;
    let dpi = conf
        .as_deref()
        .and_then(|c| vtium_settings::setting(c, "display-dpi"))
        .and_then(|d| d.parse::<u32>().ok())
        .filter(|d| *d > 0)
        .map_or((96, DpiFrom::Assumed), |d| (d, DpiFrom::Conf));
    Ok(Inputs {
        fonts: fonts(&paths.frits),
        conf_path,
        conf,
        heads_path,
        heads,
        program: "vtium-config".to_owned(),
        drafts: paths.drafts.clone(),
        root: rustix::process::geteuid().is_root(),
        dpi,
    })
}

/// Opens the TUI on the terminal and runs it until the user leaves;
/// returns the exit status.
pub fn run(paths: &Paths, err: &mut dyn std::io::Write) -> u8 {
    let inputs = match gather(paths) {
        Ok(inputs) => inputs,
        Err(why) => {
            let _ = writeln!(err, "vtium-config: {why}");
            return 1;
        }
    };
    let saver = crate::apply::saver(paths.clone());
    let mut tty = match port::Tty::open(saver) {
        Ok(tty) => tty,
        Err(e) => {
            let _ = writeln!(err, "vtium-config: the terminal: {e}");
            return 1;
        }
    };
    let status = port::drive(&mut tty, inputs);
    drop(tty);
    match status {
        Ok(status) => status,
        Err(e) => {
            let _ = writeln!(err, "vtium-config: the terminal: {e}");
            1
        }
    }
}
