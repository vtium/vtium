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

/// Each frit installed in `dir`, by its name: its text, or `None` for one
/// that is not UTF-8. A file past the frit's bound, or not a regular file,
/// is left out, as [`fonts`] leaves it.
pub fn installed(dir: &Path) -> std::collections::BTreeMap<String, Option<String>> {
    use std::io::Read;
    let mut out = std::collections::BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".vtfrit"))
            .filter(|n| valid_name(n) && !RESERVED.contains(n))
        else {
            continue;
        };
        if !std::fs::metadata(&path).is_ok_and(|m| m.is_file()) {
            continue;
        }
        let mut bytes = Vec::new();
        let max = u64::try_from(crate::frit::FRIT_BYTES_MAX).unwrap_or(0);
        let read = std::fs::File::open(&path)
            .and_then(|f| f.take(max.saturating_add(1)).read_to_end(&mut bytes));
        if read.is_ok() && bytes.len() <= crate::frit::FRIT_BYTES_MAX {
            out.insert(name.to_owned(), String::from_utf8(bytes).ok());
        }
    }
    out
}

/// How a save from `paths` is installed, with the command it runs, built
/// by the function that builds the command the save runs.
pub fn saving(paths: &Paths) -> model::Saving {
    let targets = paths.targets();
    let route = crate::apply::route(&targets, &paths.sudo);
    let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("vtium-config"));
    let shown = |argv: Vec<OsString>| {
        argv.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let argv = crate::apply::save_argv(&route, &program, &targets, Path::new(model::DRAFT));
    match route {
        crate::apply::Route::Sudo(_) => model::Saving::Sudo(shown(argv)),
        crate::apply::Route::User => model::Saving::User(shown(argv)),
    }
}

/// The density pictures are drawn at, in ADR-1200 D17's order:
/// vtium.conf's `display-dpi` where it is set within the bounds the
/// console believes; else the density of the first of `displays` whose
/// EDID states its size; else 96, as the console assumes.
pub fn density(conf: Option<&str>, displays: &[crate::display::Display]) -> (u32, DpiFrom) {
    use vtium_platform::display::{DPI_ASSUMED, DPI_MAX, DPI_MIN};
    let set = conf
        .and_then(|c| vtium_settings::setting(c, "display-dpi"))
        .and_then(|d| d.parse::<u32>().ok())
        .filter(|d| (DPI_MIN..=DPI_MAX).contains(&f64::from(*d)));
    if let Some(dpi) = set {
        return (dpi, DpiFrom::Conf);
    }
    if let Some(dpi) = displays.iter().find_map(crate::display::Display::dpi) {
        return (dpi, DpiFrom::Edid);
    }
    let assumed = format!("{DPI_ASSUMED}").parse::<u32>().unwrap_or(96);
    (assumed, DpiFrom::Assumed)
}

/// What the TUI starts from, read from `paths`, opening at `view`.
pub fn gather(paths: &Paths, view: model::View) -> Result<Inputs, String> {
    let conf_path = paths.configdir.join("vtium.conf");
    let heads_path = paths.configdir.join("heads.conf");
    let conf = read_conf(&conf_path)?;
    let heads = read_conf(&heads_path)?;
    let displays = crate::display::displays(&paths.sys);
    let dpi = density(conf.as_deref(), &displays);
    Ok(Inputs {
        fonts: fonts(&paths.frits),
        conf_path,
        conf,
        heads_path,
        heads,
        saving: saving(paths),
        drafts: paths.drafts.clone(),
        dpi,
        frits: paths.frits.clone(),
        view,
        display: displays.first().map(|d| d.pixels),
        installed: installed(&paths.frits),
    })
}

/// The name the specimen's frit takes: a frit's name, never a built-in
/// set's.
const SPECIMEN_FRIT: &str = "vtium-config-specimen";

/// The frit `font` names, as the helper takes it: the frit in `frits`, or
/// a frit of the one built-in font of that name. The helper refuses a frit
/// named for a built-in set, so a built-in font's frit takes a name of its
/// own.
pub fn frit_text(frits: &Path, font: &str) -> Result<crate::serve::FritText, String> {
    if RESERVED.contains(&font) {
        return Ok(crate::serve::FritText {
            name: SPECIMEN_FRIT.to_owned(),
            text: format!("source=builtin:{font}\n"),
        });
    }
    let path = frits.join(format!("{font}.vtfrit"));
    let text = Frit::load(&path).map_err(|e| e.to_string())?.text;
    Ok(crate::serve::FritText {
        name: font.to_owned(),
        text,
    })
}

/// The locale the specimen's line and the built-in font follow: `LC_ALL`,
/// else `LC_CTYPE`, else `LANG`, as the console reads it.
pub fn locale() -> String {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default()
}

/// The TUI's painter: a font's specimen in `locale`, drawn by `helper`,
/// from the frit in `frits` or the built-in font of that name.
pub fn painter(frits: PathBuf, helper: crate::specimen::Helper, locale: String) -> port::Painter {
    Box::new(move |font, height, max| {
        let frit = frit_text(&frits, font)?;
        crate::specimen::draw(&helper, &frit, height, max, &locale)
    })
}

/// The TUI's summarizer: what a font's specimen in `locale` gives at a
/// height, on a display of `display` pixels, for a terminal that draws no
/// pictures.
pub fn summarizer(
    frits: PathBuf,
    helper: crate::specimen::Helper,
    locale: String,
    display: Option<(u32, u32)>,
) -> port::Summarizer {
    Box::new(move |font, height| {
        let frit = frit_text(&frits, font)?;
        crate::specimen::sample(&helper, &frit, height, &locale).map(|s| s.summary(display))
    })
}

/// The sources each frit of `names` in `frits` lacks or refuses, or why it
/// draws nothing, asked of one helper under one wait.
pub fn find_sources(
    frits: &Path,
    helper: &crate::specimen::Helper,
    names: &[String],
) -> Vec<(String, model::SourcesFound)> {
    let texts: Vec<(String, Result<crate::serve::FritText, String>)> = names
        .iter()
        .map(|n| (n.clone(), frit_text(frits, n)))
        .collect();
    let readable: Vec<crate::serve::FritText> = texts
        .iter()
        .filter_map(|(_, t)| t.as_ref().ok().cloned())
        .collect();
    let mut answers = crate::specimen::sources_of(helper, readable).into_iter();
    texts
        .into_iter()
        .map(|(name, text)| {
            let found = match text {
                Ok(_) => answers.next().unwrap_or_else(|| {
                    crate::specimen::Sources::Unknown("the helper did not answer".to_owned())
                }),
                Err(why) => crate::specimen::Sources::DrawsNothing(why),
            };
            (name, found)
        })
        .collect()
}

/// The TUI's source finder, which runs on a thread of its own while the
/// TUI goes on reading keys (ADR-1201 D3).
pub fn finder(frits: PathBuf, helper: crate::specimen::Helper) -> port::Finder {
    Box::new(move |names| find_sources(&frits, &helper, &names))
}

/// Opens the TUI on the terminal at `view` and runs it until the user
/// leaves; returns the exit status.
pub fn run(paths: &Paths, view: model::View, err: &mut dyn std::io::Write) -> u8 {
    // the TUI runs as the user from start to end (ADR-1201 D5); main
    // refuses root before this, and so does this
    if rustix::process::geteuid().is_root() {
        let _ = writeln!(
            err,
            "vtium-config: run vtium config as yourself; it asks for sudo when it saves"
        );
        return 2;
    }
    let inputs = match gather(paths, view) {
        Ok(inputs) => inputs,
        Err(why) => {
            let _ = writeln!(err, "vtium-config: {why}");
            return 1;
        }
    };
    let saver = crate::apply::saver(paths.clone());
    let tools = port::Tools {
        painter: painter(paths.frits.clone(), paths.helper.clone(), locale()),
        summarizer: summarizer(
            paths.frits.clone(),
            paths.helper.clone(),
            locale(),
            inputs.display,
        ),
        finder: Some(finder(paths.frits.clone(), paths.helper.clone())),
    };
    let mut tty = match port::Tty::open(saver, tools) {
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
