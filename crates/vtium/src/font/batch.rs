//! Where vtium finds frits, the font a name asks for, and the batch and
//! gate the console reads its resources through for each login.

use std::path::{Path, PathBuf};

use vtium_settings::cullet::{self, Rule};
use vtium_settings::fontpath::path_source;
use vtium_settings::mounts::Mounts;
use vtium_settings::{Trust, batch};

use crate::config::Config;
use crate::helper;

#[cfg(doc)]
use super::DEFAULT_SET;
use super::{FRITS, frits, trust};

/// Where vtium finds frits: the frits' directory, the helper that
/// generates a frit's glyphs, and the files that name the font when the
/// configuration does not.
#[derive(Clone, Debug)]
pub struct Sources {
    pub frits: PathBuf,
    /// The ownership check each frit passes before vtium reads it,
    /// [`trust`] but in a test whose frits sit under a directory others
    /// can write, such as `/tmp`.
    pub trust: Trust,
    /// The helper; `None` for the one `font-helper` names
    /// ([`helper::Launcher::configured`]).
    pub helper: Option<helper::Command>,
    pub vconsole_conf: PathBuf,
    pub kernel_cmdline: PathBuf,
}

impl Sources {
    /// The system's, as `config` names them: the frits' directory,
    /// `font-frits`; the helper `font-helper` names; `/etc/vconsole.conf`
    /// and `/proc/cmdline`.
    pub fn configured(config: &Config) -> Sources {
        Sources {
            frits: frits(config),
            ..Sources::default()
        }
    }
}

impl Default for Sources {
    /// The system's with every key at its default: the frits' directory
    /// vtium is built with, read under [`trust`], the helper `font-helper` names,
    /// `/etc/vconsole.conf` and `/proc/cmdline`.
    fn default() -> Sources {
        Sources {
            frits: PathBuf::from(FRITS),
            trust: trust(),
            helper: None,
            vconsole_conf: PathBuf::from("/etc/vconsole.conf"),
            kernel_cmdline: PathBuf::from("/proc/cmdline"),
        }
    }
}

/// Where the font's name came from, which picks the source of the frit
/// made on the fly for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Came {
    /// `font-name`: a path, or a kbd font's name.
    Config,
    /// `font-name` under `font-engine=freetype` or `pango`: an outline
    /// font's family, a `font:` source.
    Family,
    /// `vconsole.font=` on the kernel command line, or `FONT=` in
    /// vconsole.conf.
    Vconsole,
    /// No font is named, so the name is the default set's,
    /// [`DEFAULT_SET`]. vtium draws it only where a set of that name is
    /// installed, and never makes a set on the fly for it.
    Default,
}

/// The font a name asks for, and where the name came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    pub name: String,
    pub came: Came,
}

impl Wanted {
    /// The one source of the frit made on the fly for the name:
    /// `vconsole` for vconsole's font, and for a name that starts with `/`
    /// the source of the kind of font the file's extension names
    /// ([`path_source`]). A family's name, from `font-engine=freetype` or
    /// `pango`, takes `font:`, and any other name `kbd:`. A path takes the
    /// path's source under every `font-engine`, because `font:` names a
    /// family alone and the font helper refuses a `/` in it.
    /// The default set's name never reaches this, because [`frit_named`]
    /// refuses it when no set bears it; it reads as a kbd name.
    ///
    /// References:
    /// - ADR-1200 D9: the frit made on the fly for a name no frit bears.
    /// - ADR-1200 D1: the source forms.
    pub(super) fn source(&self) -> String {
        match self.came {
            Came::Vconsole => "vconsole".to_owned(),
            Came::Config | Came::Family if self.name.starts_with('/') => path_source(&self.name),
            Came::Family => format!("font:{}", self.name),
            Came::Config | Came::Default => format!("kbd:{}", self.name),
        }
    }
}

/// `path`'s text, read as a configuration file is, a regular file of at
/// most `FILE_MAX` bytes; empty when there is none.
pub fn read_text(path: &Path) -> String {
    crate::config::read_bytes(path)
        .ok()
        .flatten()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// What the console reads its resources through for this login: the gate
/// and the folder of the palettes the package ships.
struct Held {
    gate: std::sync::Arc<cullet::Gate>,
    palettes: PathBuf,
}

thread_local! {
    /// The console's gate for this login: the batch, the trust rule and
    /// the mount table that every resource the console reads passes. The
    /// console reads the batch and the mount table when it starts and again
    /// at each new login ([`load_batch`]), and every reader takes the gate from here: the
    /// font helpers, the color schemes, the color maps and the pointer
    /// files. The console reads its resources on its main loop's thread, so
    /// the gate is that thread's; each test, which runs on a thread of its
    /// own, holds a gate of its own.
    static HELD: std::cell::RefCell<Option<Held>> = const { std::cell::RefCell::new(None) };
}

/// What [`HELD`] holds, read the first time from this machine's places,
/// as a start reads them.
fn held<T>(f: impl Fn(&Held) -> T) -> T {
    if let Some(t) = HELD.with_borrow(|h| h.as_ref().map(&f)) {
        return t;
    }
    let _ = start_batch(&batch::Places::system(), &trust(), Rule::new(trust()));
    HELD.with_borrow(|h| h.as_ref().map(&f)).unwrap_or_else(|| {
        // `start_batch` always leaves something held; this stands in only
        // to keep the function free of a panic
        f(&Held {
            gate: std::sync::Arc::new(cullet::Gate::new(
                Rule::new(trust()),
                None,
                batch::Batch::default(),
            )),
            palettes: PathBuf::from(vtium_settings::palette::PACKAGE_PALETTES),
        })
    })
}

/// Reads the batch the way a start does, at `places` under `trust` with
/// `rule` for each resource, unless the console already holds one; what
/// vtium found reading the batch file, for the caller to log, or `None`
/// when the console already held a batch.
pub fn start_batch(places: &batch::Places, trust: &Trust, rule: Rule) -> Option<Found> {
    if HELD.with_borrow(Option::is_some) {
        return None;
    }
    load_batch(places, trust, rule)
}

/// The console's gate for this login. The first call before any
/// [`load_batch`] reads this machine's batch, as a start does.
///
/// References:
/// - ADR-1205 D4: one gate for every kind of resource, and one batch for each login.
pub fn gate() -> std::sync::Arc<cullet::Gate> {
    held(|h| std::sync::Arc::clone(&h.gate))
}

/// The folder of the palettes the package ships, as the places the batch
/// was last read from name it: [`vtium_settings::palette::PACKAGE_PALETTES`],
/// or a test's.
pub fn package_palettes() -> PathBuf {
    held(|h| h.palettes.clone())
}

/// Reads the batch at `places` under `trust`, the check vtium.conf passes,
/// and this machine's mount table, and makes them the console's gate, with
/// `rule`, the trust rule each resource passes. vtium calls this at the
/// start and at each new login. When the batch differs from the one the
/// console held, which it does at the first read, the result is what vtium
/// found reading the batch file, for the caller to log. The result is
/// `None` when the batch is the same, so a login that changes nothing logs
/// nothing.
///
/// References:
/// - ADR-1205 D4, its note of 2026-11-19: the console reads the batch again at each new login, so an activation takes effect at the next login.
pub fn load_batch(places: &batch::Places, trust: &Trust, rule: Rule) -> Option<Found> {
    load_batch_with(places, trust, rule, Mounts::system().unwrap_or_default())
}

/// [`load_batch`], judging filesystems by `mounts`. The gate takes the
/// mount table of every login, even one whose batch is the same, because
/// the kernel can give a mount ID to a new mount once the old mount is
/// gone. A gate that kept an older table would then judge the new
/// filesystem by the old mount's line.
pub(super) fn load_batch_with(
    places: &batch::Places,
    trust: &Trust,
    rule: Rule,
    mounts: Mounts,
) -> Option<Found> {
    let read = batch::Batch::system(places, trust);
    let (same, kept) = HELD.with_borrow(|held| {
        let Some(h) = held else {
            return (false, false);
        };
        let same = h.gate.batch == read.batch && h.palettes == places.palettes;
        (same, same && h.gate.mounts.as_ref() == Some(&mounts))
    });
    if kept {
        return None;
    }
    let more = read.more_line();
    HELD.set(Some(Held {
        gate: std::sync::Arc::new(cullet::Gate::new(rule, Some(mounts), read.batch)),
        palettes: places.palettes.clone(),
    }));
    (!same).then_some(Found {
        findings: read.findings,
        more,
    })
}

/// What vtium found reading the batch file, for the caller to log.
#[derive(Debug)]
pub struct Found {
    /// The findings, in the file's order, at most the first
    /// [`vtium_settings::ERRORS_MAX`] lines vtium ignores.
    pub findings: Vec<batch::Finding>,
    /// The line that counts the ignored lines past those listed, when
    /// there are more ([`batch::Read::more_line`]).
    pub more: Option<String>,
}

impl Found {
    /// Logs the findings: a missing file as information, since a machine
    /// upgraded from an earlier vtium has none, and a file vtium cannot
    /// use, or a line it ignores, as a warning. The count of the ignored
    /// lines past those listed comes last, as a warning too.
    pub fn log(&self) {
        for finding in &self.findings {
            match finding {
                batch::Finding::Missing { .. } => log::info!(target: "batch", "{finding}"),
                _ => log::warn!(target: "batch", "{finding}"),
            }
        }
        if let Some(more) = &self.more {
            log::warn!(target: "batch", "{more}");
        }
    }
}
