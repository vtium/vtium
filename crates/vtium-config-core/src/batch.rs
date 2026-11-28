//! The active resources beside a draft, and the commands `activate`,
//! `deactivate` and `active`.
//!
//! vtium uses only the active resources: the fonts, color schemes,
//! color maps, shaders and pointer themes that the batch file,
//! `/etc/vtium/batch.conf`, lists one path on each line. The built-in
//! font is always active and never appears there. A change to that list
//! waits beside the draft, in `drafts/NAME.batch`, until the draft is
//! installed, so it installs with the draft and goes when the draft is
//! reverted or removed.
//!
//! `NAME.batch` keeps changes, one `activate PATH` or `deactivate PATH`
//! on each line, and not a copy of the batch file. The install applies
//! them to the batch file as it is then, so a line root wrote by hand
//! stays. When no batch file is installed, or vtium refuses the one that
//! is, the install starts from the list of a fresh install, as vtium does,
//! so the first change never drops the default fonts. vtium refuses a
//! batch file that fails the owner and mode check, is too large, or cannot
//! be read, and vtium-config reads the file the same way
//! ([`Installed::read`]), so it never builds on lines vtium ignores.
//!
//! References:
//! - ADR-1205 D4: the batch, its file, the trust rule each activated file passes, the list of a fresh install, and a draft keeping its changes to the batch beside it.
//! - ADR-1205 D8: deactivating a resource warns and names the looks that use it.
//! - ADR-1403 D8: a draft keeps its font set changes beside it in the same way.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use vtium_settings::batch::{self as active, Batch, Entry};

use crate::apply::{Route, Targets};
use crate::drafts::{Session, Unrouted, run_child};
use crate::outcome::Fail;
use crate::places::Paths;
use crate::store::Store;

/// One change a draft makes to the active resources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Activate,
    Deactivate,
}

impl Change {
    /// The word for the change, as `NAME.batch` and the commands write it.
    pub const fn word(self) -> &'static str {
        match self {
            Change::Activate => "activate",
            Change::Deactivate => "deactivate",
        }
    }
}

/// A draft's changes to the active resources, in the order they were made,
/// at most one for each path. An index by path finds a path's change
/// without a scan of the list, so reading a large `NAME.batch` does not
/// take time in the square of its length.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    list: Vec<(PathBuf, Change)>,
    /// Each path's place in `list`.
    index: BTreeMap<PathBuf, usize>,
}

impl Changes {
    /// Reads the text of a `NAME.batch` file at `shown`. A line vtium-config
    /// cannot read refuses the whole file, naming the line.
    pub fn parse(text: &str, shown: &Path) -> Result<Changes, Fail> {
        let mut changes = Changes::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parsed = line.split_once(' ').and_then(|(word, rest)| {
                let change = match word {
                    "activate" => Change::Activate,
                    "deactivate" => Change::Deactivate,
                    _ => return None,
                };
                Some((plain(rest.trim())?, change))
            });
            let Some((path, change)) = parsed else {
                return Err(Fail::File(format!(
                    "{} line {}: vtium-config cannot read this line. Each line reads activate PATH or deactivate PATH, with an absolute path.",
                    shown.display(),
                    n.saturating_add(1)
                )));
            };
            changes.set(path, Some(change));
        }
        Ok(changes)
    }

    /// The text of the `NAME.batch` file of the draft `draft`.
    pub fn text(&self, draft: &str) -> String {
        let mut out = format!(
            "# The changes that draft {draft} makes to the resources vtium may use.\n# vtium-config applies them when it installs the draft.\n"
        );
        for (path, change) in &self.list {
            out.push_str(&format!("{} {}\n", change.word(), path.display()));
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// The change for `path`, if the draft makes one.
    pub fn get(&self, path: &Path) -> Option<Change> {
        let at = *self.index.get(path)?;
        self.list.get(at).map(|(_, c)| *c)
    }

    /// Makes `change` the draft's change for `path`, or with `None` takes
    /// the draft's change for it back.
    pub fn set(&mut self, path: PathBuf, change: Option<Change>) {
        let at = self.index.get(&path).copied();
        match (at.and_then(|at| self.list.get_mut(at)), change) {
            (Some(slot), Some(change)) => slot.1 = change,
            (None, Some(change)) => {
                self.index.insert(path.clone(), self.list.len());
                self.list.push((path, change));
            }
            (Some(_), None) => {
                self.list.retain(|(p, _)| *p != path);
                self.reindex();
            }
            (None, None) => {}
        }
    }

    /// Builds the index again after changes left `list`.
    fn reindex(&mut self) {
        #[cfg(test)]
        tests::LIST_PASSES.with(|n| n.set(n.get().saturating_add(1)));
        self.index = self
            .list
            .iter()
            .enumerate()
            .map(|(at, (p, _))| (p.clone(), at))
            .collect();
    }

    /// Each change, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&Path, Change)> {
        self.list.iter().map(|(p, c)| (p.as_path(), *c))
    }

    /// Keeps only the changes that `base`, the batch the system holds,
    /// does not already match: an activation of a path it lists, and a
    /// deactivation of a path it lacks, are no change.
    pub fn settle(&mut self, base: &Batch) {
        self.list.retain(|(path, change)| match change {
            Change::Activate => !base.holds(path),
            Change::Deactivate => base.holds(path),
        });
        self.reindex();
    }
}

/// `1 change to the active resources`, or `N changes ...`, as the state
/// line and `revert --dry-run` count a draft's changes.
pub fn resource_changes(n: usize) -> String {
    match n {
        1 => "1 change to the active resources".to_owned(),
        n => format!("{n} changes to the active resources"),
    }
}

/// `word` as a path the batch file can list: an absolute path without
/// `..`, with `.` and doubled slashes dropped, read as vtium reads a line
/// of the batch file. A path that holds a control character, such as a tab
/// or an escape, is refused: `active` separates its fields with tabs, and
/// an escape would reach the terminal raw when a command prints the path.
pub fn plain(word: &str) -> Option<PathBuf> {
    if word.chars().any(char::is_control) {
        return None;
    }
    let read = Batch::parse(word.as_bytes(), Path::new(""));
    match (read.findings.as_slice(), read.batch.entries()) {
        ([], [Entry::Path(path)]) => Some(path.clone()),
        _ => None,
    }
}

/// Whether `word` names the built-in font, which is always active: a
/// built-in font's name, `builtin`, or a frit's `builtin:NAME` source.
pub fn builtin(word: &str) -> bool {
    crate::frit::RESERVED.contains(&word) || word == "builtin" || word.starts_with("builtin:")
}

/// The batch file that an install to `targets` writes: the one vtium
/// reads, [`active::BATCH_FILE`], for the system's directories, and
/// `batch.conf` in the configuration directory for any other, such as a
/// test's.
pub fn batch_file(targets: &Targets) -> PathBuf {
    if targets.is_system() {
        PathBuf::from(active::BATCH_FILE)
    } else {
        targets.configdir.join("batch.conf")
    }
}

/// The list of a fresh install on this machine: the fonts, color schemes
/// and pointer file the package ships, and the console font that
/// vconsole.conf's `FONT=` names, with the kernel command line over it.
/// vtium uses it while no batch file is installed, and builds it the same
/// way ([`Batch::fresh`]).
pub fn fresh() -> Batch {
    Batch::fresh(&vtium_settings::batch::Places::system())
}

/// The batch the system holds: the one the installed batch file's text
/// `installed` lists, or `fresh` when vtium reads no batch file.
pub fn base(installed: Option<&str>, fresh: &Batch) -> Batch {
    match installed {
        Some(text) => Batch::parse(text.as_bytes(), Path::new("")).batch,
        None => fresh.clone(),
    }
}

/// The installed batch file, as vtium reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Installed {
    /// No batch file is installed, so vtium uses the list of a fresh
    /// install.
    Missing,
    /// vtium ignores the batch file and uses the list of a fresh install.
    /// The text says why, file first, in the words of `vtium check-config`.
    Refused(String),
    /// vtium reads the batch file: its bytes.
    Read(Vec<u8>),
}

impl Installed {
    /// Reads the batch file at `path` as vtium does when it runs as the
    /// user `euid`. The file and each folder above it must belong to root
    /// or to `euid`, and nobody else may write them. vtium reads at most
    /// [`active::BATCH_MAX`] bytes. A file that fails the check, is larger,
    /// is not a regular file or cannot be read counts as missing, because
    /// someone other than root could have written it.
    ///
    /// References:
    /// - ADR-1205 D4, its note of 2026-11-19: vtium uses the list of a fresh install in place of a batch file it cannot use.
    pub fn read(path: &Path, euid: u32) -> Installed {
        use vtium_settings::batch::{BATCH_MAX, Finding, Unread};
        let trust = vtium_settings::Trust::running_as(euid);
        match vtium_settings::read_limited(path, &trust, BATCH_MAX) {
            Ok(Some(bytes)) => Installed::Read(bytes),
            Ok(None) => Installed::Missing,
            Err(rejected) => {
                let why = rejected
                    .errors
                    .into_iter()
                    .next()
                    .map_or(Unread::NotAFile, |e| e.problem.into());
                let finding = Finding::Refused {
                    path: path.to_path_buf(),
                    why,
                };
                Installed::Refused(finding.to_string())
            }
        }
    }

    /// The batch vtium uses: the one the file lists, or `fresh`, the list
    /// of a fresh install, when vtium reads no file. A line that vtium
    /// ignores, such as one that is not UTF-8, lists nothing.
    pub fn base(&self, fresh: &Batch) -> Batch {
        match self {
            Installed::Read(bytes) => Batch::parse(bytes, Path::new("")).batch,
            Installed::Missing | Installed::Refused(_) => fresh.clone(),
        }
    }

    /// The text of the file vtium reads, the one that changes start from,
    /// or `None` when vtium reads no file and changes start from the list
    /// of a fresh install. `Err` names the first line of the file at
    /// `path` that is not UTF-8, which vtium-config cannot keep in a file
    /// it writes.
    pub fn text(&self, path: &Path) -> Result<Option<&str>, String> {
        let Installed::Read(bytes) = self else {
            return Ok(None);
        };
        std::str::from_utf8(bytes).map(Some).map_err(|e| {
            let before = bytes.get(..e.valid_up_to()).unwrap_or_default();
            let line = before
                .iter()
                .filter(|&&b| b == b'\n')
                .count()
                .saturating_add(1);
            format!(
                "{}:{line}: this line is not UTF-8 text. vtium ignores the line, and vtium-config cannot keep it in the file it writes. Fix the line or remove it as root, then install again.",
                path.display()
            )
        })
    }
}

/// The comment that opens a batch file vtium-config writes when vtium
/// reads none, because none is installed or vtium refuses the installed
/// one. It is wrapped at 78 columns, as vtium.conf's comments are.
pub const HEADER: &str = "\
# vtium uses only the resources this file lists: fonts, color schemes,
# color maps, shaders and pointer themes, one absolute path on each line.
# The built-in font is always active and is not listed here.
#
# vtium-config wrote this file from the list of a fresh install, when a
# draft changed the active resources while vtium used that list.
# vtium-config activate and vtium-config deactivate change it through a
# draft, and root can edit it.

";

/// The path a line of a batch file names, if it names one.
fn line_path(line: &str) -> Option<PathBuf> {
    let read = Batch::parse(line.as_bytes(), Path::new(""));
    match read.batch.entries() {
        [Entry::Path(path)] => Some(path.clone()),
        _ => None,
    }
}

/// The batch file's text after `changes`, starting from `installed`, the
/// installed file's text, or from [`HEADER`] and `fresh` when none is
/// installed: a deactivation takes out each line that names its path, and
/// an activation of a path no line names adds a line at the end. Every
/// other line, comments and blank lines included, stays as it was. `None`
/// when the result lists the same resources as the system holds now, so
/// the install leaves the batch file as it is.
pub fn applied(installed: Option<&str>, fresh: &Batch, changes: &Changes) -> Option<String> {
    let text = edited(installed, fresh, changes);
    let after = Batch::parse(text.as_bytes(), Path::new("")).batch;
    (after != base(installed, fresh)).then_some(text)
}

/// The text [`applied`] makes, whether or not it changes anything. Each
/// line is read once, and the paths it names are looked up in sets, so a
/// long list of changes takes time in proportion to its length.
fn edited(installed: Option<&str>, fresh: &Batch, changes: &Changes) -> String {
    use std::collections::BTreeSet;
    let lines: Vec<String> = match installed {
        Some(text) => text.lines().map(str::to_owned).collect(),
        None => HEADER
            .lines()
            .map(str::to_owned)
            .chain(
                fresh
                    .entries()
                    .iter()
                    .map(|Entry::Path(p)| p.display().to_string()),
            )
            .collect(),
    };
    let gone: BTreeSet<&Path> = changes
        .iter()
        .filter(|(_, c)| *c == Change::Deactivate)
        .map(|(p, _)| p)
        .collect();
    let mut named = BTreeSet::new();
    let mut text = String::new();
    for line in &lines {
        let path = line_path(line);
        if path.as_deref().is_some_and(|p| gone.contains(p)) {
            continue;
        }
        named.extend(path);
        text.push_str(line);
        text.push('\n');
    }
    // `changes` holds each path once, so no activation adds a line that
    // another activation added
    for (path, change) in changes.iter() {
        if change == Change::Activate && !named.contains(path) {
            text.push_str(&path.display().to_string());
            text.push('\n');
        }
    }
    if text.is_empty() {
        text.push('\n');
    }
    text
}

/// How many bytes vtium reads of the batch file at most, in KiB, as the
/// messages about the bound say it.
const BATCH_KIB: usize = active::BATCH_MAX / 1024;

/// Why the batch file that `changes` make, starting from `installed` and
/// `fresh` as [`applied`] does, is refused: it is longer than vtium reads,
/// and longer than the file `before` makes. `None` when the file fits, or
/// when it shrinks, so that a draft that holds too many activations can
/// always be brought back within the bound by deactivating resources.
fn too_long(
    installed: Option<&str>,
    fresh: &Batch,
    before: &Changes,
    changes: &Changes,
) -> Option<usize> {
    let after = edited(installed, fresh, changes).len();
    (after > active::BATCH_MAX && after > edited(installed, fresh, before).len()).then_some(after)
}

/// What kind of resource `path` is, in the words a person uses: a folder
/// or a `.svg` file is a pointer theme, a `.cube` file a color map, a
/// `.vtshader` file a shader, a file in a color scheme folder a color scheme,
/// and anything else a font.
pub fn kind(path: &Path) -> &'static str {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let in_palettes = path.parent().is_some_and(|dir| {
        vtium_settings::palette::FOLDERS
            .iter()
            .any(|f| dir == Path::new(f))
    });
    if path.is_dir() || extension == "svg" {
        "pointer theme"
    } else if extension == "cube" {
        "color map"
    } else if extension == "vtshader" {
        "shader"
    } else if in_palettes {
        "color scheme"
    } else {
        "font"
    }
}

/// The user whose files the trust rule accepts for an install to
/// `targets` through `sudo`: root for the system's directories, which
/// root installs, and this process's user for any other, as `apply`
/// runs as that user there.
pub fn rule_user(targets: &Targets, sudo: &std::ffi::OsString) -> u32 {
    let euid = rustix::process::geteuid().as_raw();
    match crate::apply::route(targets, sudo) {
        Route::Sudo(_) => 0,
        Route::User => euid,
    }
}

/// Whether the resource at `path` passes the trust rule for the user
/// `euid`'s files: it and every folder above it belong to root, or to
/// `euid`, and no one else can write them, none is a symbolic link, and
/// its filesystem stays mounted. A pointer theme is a folder, and passes
/// when a picture in its `cursors` folder passes the rule for a theme,
/// whose links may stay inside the theme. `Err` says why, file first.
pub fn trusted(path: &Path, euid: u32) -> Result<(), String> {
    use vtium_settings::cullet::{Rule, check, check_in_theme};
    let rule = Rule::running_as(euid);
    let mounts = vtium_settings::mounts::Mounts::system().map_err(|e| {
        format!(
            "{}: vtium-config cannot check this file, because it cannot read the mount table: {e}.",
            path.display()
        )
    })?;
    let is_folder = std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
    if !is_folder {
        return check(path, &rule, &mounts).map_err(|refused| refused.to_string());
    }
    let cursors = path.join("cursors");
    let mut pictures: Vec<PathBuf> = std::fs::read_dir(&cursors)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    pictures.sort();
    let Some(picture) = pictures.first() else {
        return Err(format!(
            "{}: this folder holds no cursors folder with pictures in it, so it is not a pointer theme.",
            path.display()
        ));
    };
    check_in_theme(path, picture, &rule, &mounts).map_err(|refused| refused.to_string())
}

/// Where [`uses`] finds what the looks name: the font sets' folder, the
/// folders of the color schemes, and the color maps and shaders each
/// vtlook names.
pub struct Places<'a> {
    pub frits: Option<&'a Path>,
    pub palettes: &'a [&'a Path],
    /// The color maps and shaders that the vtlook of this name names, as
    /// vtium reads the vtlook, or none when it cannot be read.
    pub vtlook: &'a dyn Fn(&str) -> Vec<PathBuf>,
}

/// How a look uses a resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Through {
    /// The look's font set, of this name, names the font by its path.
    FontSet(String),
    /// The look names the color scheme.
    Palette,
    /// The look's vtlook, of this name, names the color map or shader.
    Vtlook(String),
    /// The look's `pointer-theme` names the pointer theme, or for the top
    /// level, vtium.conf's top-level `pointer-theme`, which every look that
    /// names no theme of its own draws.
    PointerTheme,
}

/// A look that uses a resource, and how. `look` is `None` for the top
/// level of vtium.conf, which a head draws with when no section has the
/// name of its look.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Use {
    pub look: Option<String>,
    pub through: Through,
}

/// One look of vtium.conf's text, or its top level, with the font set,
/// the color scheme and the vtlook it draws with.
struct Drawn {
    /// `None` for the top level.
    look: Option<String>,
    frit: Option<String>,
    palette: Option<String>,
    vtlook: Option<String>,
    /// The look's own `pointer-theme`, `None` where it takes the top
    /// level's.
    pointer: Option<String>,
}

/// What each look of vtium.conf's text `conf` draws with. A look takes
/// the top level's font set, color scheme and vtlook where it names none
/// of its own. The top level is listed too when a head draws with it,
/// because no section has the name of the head's look, or because the
/// text has no heads.
///
/// References:
/// - ADR-1304 D5: a head's vtlook comes from its look, else the top level, else the built-in one.
fn looks_of(conf: &str) -> Vec<Drawn> {
    use vtium_settings::keys::{FONT_NAME, PALETTE, POINTER_THEME};
    let top = Drawn {
        look: None,
        frit: vtium_settings::setting(conf, FONT_NAME.name),
        palette: vtium_settings::setting(conf, PALETTE.name),
        vtlook: vtium_settings::setting(conf, "vtlook"),
        pointer: None,
    };
    let Some(reading) = vtium_settings::sections(conf.as_bytes()) else {
        return vec![top];
    };
    let config = reading.parsed.config;
    let mut out = Vec::new();
    // a head draws its look's section, or the top level when no section
    // has its look's name; an unset `look` names the look `default`
    let bare = config
        .heads
        .iter()
        .any(|h| !config.looks.iter().any(|l| l.name == h.look));
    if config.heads.is_empty() || bare {
        out.push(Drawn {
            look: None,
            frit: top.frit.clone(),
            palette: top.palette.clone(),
            vtlook: top.vtlook.clone(),
            pointer: None,
        });
    }
    for look in config.looks {
        let value = |key: &str| {
            look.settings
                .iter()
                .rev()
                .find(|s| s.key.name == key)
                .map(|s| s.value.clone())
        };
        out.push(Drawn {
            frit: value(FONT_NAME.name).or_else(|| top.frit.clone()),
            palette: value(PALETTE.name).or_else(|| top.palette.clone()),
            vtlook: look.vtlook.clone().or_else(|| top.vtlook.clone()),
            pointer: value(POINTER_THEME.name),
            look: Some(look.name),
        });
    }
    out
}

/// The looks of vtium.conf's text `conf` that use the resource at `path`,
/// the top level first, and how each uses it: a font that its font set
/// names by its path, its color scheme, or a color map or shader that its
/// vtlook names; and the top level when its `pointer-theme` names the
/// pointer theme at `path`. A font that a set names by its family is
/// found only by the font helper, so no look is named for it here. The
/// command line's warning and the TUI's question both come from this
/// list.
pub fn uses(path: &Path, conf: &str, places: &Places<'_>) -> Vec<Use> {
    let mut out = Vec::new();
    // paths compare by their components, so a last slash, a doubled
    // slash or a `.` in the key's value changes nothing, as in the batch
    let theme = vtium_settings::setting(conf, vtium_settings::keys::POINTER_THEME.name);
    if theme.is_some_and(|t| Path::new(&t) == path) {
        out.push(Use {
            look: None,
            through: Through::PointerTheme,
        });
    }
    for drawn in looks_of(conf) {
        let mut through = None;
        if let (Some(frit), Some(frits)) = (&drawn.frit, places.frits)
            && frit_names(frits, frit, path)
        {
            through = Some(Through::FontSet(frit.clone()));
        }
        if let Some(palette) = &drawn.palette
            && palette_file(places.palettes, palette).as_deref() == Some(path)
        {
            through = Some(Through::Palette);
        }
        if let Some(vtlook) = &drawn.vtlook
            && vtlook != vtium_settings::vtlook::DEFAULT
            && (places.vtlook)(vtlook).iter().any(|f| f == path)
        {
            through = Some(Through::Vtlook(vtlook.clone()));
        }
        if drawn
            .pointer
            .as_deref()
            .is_some_and(|t| Path::new(t) == path)
        {
            through = Some(Through::PointerTheme);
        }
        if let Some(through) = through {
            out.push(Use {
                look: drawn.look,
                through,
            });
        }
    }
    out
}

/// The looks of vtium.conf's text `conf` that draw with the vtlook
/// `name`, the top level first, as [`uses`] counts them.
pub fn drawing_vtlook(conf: &str, name: &str) -> Vec<Option<String>> {
    looks_of(conf)
        .into_iter()
        .filter(|d| d.vtlook.as_deref() == Some(name))
        .map(|d| d.look)
        .collect()
}

/// The looks of vtium.conf's text `conf` that draw with the color scheme
/// `name`, the top level first, as [`uses`] counts them. A look that
/// names no scheme takes the top level's, and with neither set it draws
/// `legacy`, the first built-in scheme. This names the users of a scheme
/// built into vtium, which has no file for [`uses`] to match.
///
/// References:
/// - ADR-1205 D10: a look that names no palette takes the top level's, and with neither set a head draws `legacy`.
pub fn drawing_palette(conf: &str, name: &str) -> Vec<Option<String>> {
    let fallback = vtium_term::PALETTE_NAMES
        .first()
        .copied()
        .unwrap_or_default();
    looks_of(conf)
        .into_iter()
        .filter(|d| d.palette.as_deref().unwrap_or(fallback) == name)
        .map(|d| d.look)
        .collect()
}

/// The color maps and shaders that `vtlook` names, each in `looks`, the
/// folder vtium reads them from.
pub fn named_files(vtlook: &vtium_settings::vtlook::Vtlook, looks: &Path) -> Vec<PathBuf> {
    use vtium_settings::vtlook::Value;
    vtlook
        .color
        .iter()
        .chain(&vtlook.spatial)
        .flat_map(|f| f.values.iter())
        .filter_map(|(_, v)| match v {
            Value::File(file) => Some(looks.join(file)),
            _ => None,
        })
        .collect()
}

/// The color maps and shaders that the vtlook `name` names, read as vtium
/// reads it under `trust`: from the administrator's folder, else from the
/// package's presets. None when the vtlook cannot be read.
pub fn vtlook_files(name: &str, trust: &vtium_settings::Trust) -> Vec<PathBuf> {
    use vtium_settings::vtlook::{LOOKS, SUFFIX, folders, read};
    let file = format!("{name}{SUFFIX}");
    let Some(dir) = folders()
        .into_iter()
        .find(|d| d.join(&file).symlink_metadata().is_ok())
    else {
        return Vec::new();
    };
    read(name, dir, trust).map_or_else(|_| Vec::new(), |v| named_files(&v, Path::new(LOOKS)))
}

/// The sentences that warn, before `path` is deactivated, of each look in
/// vtium.conf's text `conf` that uses it, as [`uses`] finds them. A
/// deactivation takes effect at the next login after the install, when
/// vtium reads the batch again.
///
/// References:
/// - ADR-1205 D4, its note of 2026-11-19: vtium reads the batch again at each new login.
/// - ADR-1205 D8: deactivating a resource warns and lists the looks that use it.
pub fn users(path: &Path, conf: &str, places: &Places<'_>) -> Vec<String> {
    let kind = kind(path);
    uses(path, conf, places)
        .into_iter()
        .map(|u| {
            match (&u.through, &u.look) {
                (Through::PointerTheme, None) => {
                    return "vtium.conf's pointer-theme names this pointer theme. From the next login after the install, each look that names no pointer theme of its own shows vtium's own pointer pictures in its place.".to_owned();
                }
                (Through::PointerTheme, Some(name)) => {
                    return format!("Look {name} uses this pointer theme. From the next login after the install, look {name} shows vtium's own pointer pictures in its place.");
                }
                _ => {}
            }
            let (who, after) = match &u.look {
                None => (
                    "The top level of vtium.conf".to_owned(),
                    "each head that uses the top level renders".to_owned(),
                ),
                Some(name) => (format!("Look {name}"), format!("look {name} renders")),
            };
            let (uses, without) = match &u.through {
                Through::FontSet(frit) => (
                    format!("this font through the font set {frit}"),
                    "without it",
                ),
                Through::Palette => ("this color scheme".to_owned(), "without it"),
                Through::Vtlook(vtlook) => (
                    format!("this {kind} through the vtlook {vtlook}"),
                    "with no filter",
                ),
                Through::PointerTheme => (String::new(), ""),
            };
            format!("{who} uses {uses}. From the next login after the install, {after} {without}.")
        })
        .collect()
}

/// The file vtium reads for the color scheme `name`: `NAME.palette` in
/// the first of `folders` that holds one, so the administrator's copy in
/// `/etc/vtium/palettes` shadows the package's. `None` for a scheme built
/// into vtium, which has no file, and for a name no file can take.
fn palette_file(folders: &[&Path], name: &str) -> Option<PathBuf> {
    use vtium_settings::palette::{SUFFIX, built_in, named};
    if built_in(name) || !named(name) {
        return None;
    }
    folders
        .iter()
        .map(|dir| dir.join(format!("{name}{SUFFIX}")))
        .find(|file| file.symlink_metadata().is_ok())
}

/// Whether the font set `name` in the folder `frits` names the font file
/// `path` by its path in one of its sources.
fn frit_names(frits: &Path, name: &str, path: &Path) -> bool {
    use crate::frit::{FRIT_BYTES_MAX, Frit, Origin};
    let Some(file) = crate::switch::frit_file(frits, name) else {
        return false;
    };
    let Ok(bytes) = std::fs::read(&file) else {
        return false;
    };
    if bytes.len() > FRIT_BYTES_MAX {
        return false;
    }
    let Ok(frit) = Frit::parse(&bytes, &file, name) else {
        return false;
    };
    frit.sources.iter().any(|s| match &s.origin {
        Origin::File(p) | Origin::Bdf(p) | Origin::Hex(p) | Origin::Outline(p) => p == path,
        _ => false,
    })
}

/// What `activate`, `deactivate` and `active` were asked.
struct Asked {
    paths: Vec<String>,
    dry_run: bool,
    install: bool,
}

fn asked(command: &str, words: &[String]) -> Result<Asked, Fail> {
    let mut asked = Asked {
        paths: Vec::new(),
        dry_run: false,
        install: false,
    };
    for word in words {
        match word.as_str() {
            "--dry-run" => asked.dry_run = true,
            "--install" => asked.install = true,
            w if w.starts_with("--") => {
                return Err(Fail::Usage(format!("{command} takes no option {w}")));
            }
            w => asked.paths.push(w.to_owned()),
        }
    }
    if asked.paths.is_empty() {
        return Err(Fail::Usage(format!(
            "{command} takes the path of one or more resources"
        )));
    }
    if asked.dry_run && asked.install {
        return Err(Fail::Usage(
            "--dry-run and --install cannot be used together".to_owned(),
        ));
    }
    Ok(asked)
}

/// `activate PATH...`, `deactivate PATH...` and `active`, run on the
/// current draft, writing to `out` and `err`; returns the exit status.
pub fn command(
    command: crate::help::Command,
    words: &[String],
    paths: &Paths,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let ran = Store::system().and_then(|store| {
        let session = Session { store, paths };
        match command {
            crate::help::Command::Activate => change(&session, Change::Activate, words, out),
            crate::help::Command::Deactivate => change(&session, Change::Deactivate, words, out),
            _ => list(&session, words, out),
        }
    });
    match ran {
        Ok(()) => 0,
        Err(fail) => {
            let _ = writeln!(err, "vtium-config: {}", fail.text());
            if fail.code() == 2 {
                let _ = writeln!(err, "Try vtium-config --help.");
            }
            fail.code()
        }
    }
}

/// Where a resource stands in a draft, as `active` and the tabs show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// The built-in font, which is always active.
    Always,
    /// The system holds it, and the draft leaves it so.
    Active,
    /// The system lacks it, and the draft leaves it so.
    Inactive,
    /// The draft activates it, and its install makes it active.
    Activates,
    /// The draft deactivates it, and its install makes it inactive.
    Deactivates,
}

impl State {
    /// The words `active` prints for the state.
    pub const fn words(self) -> &'static str {
        match self {
            State::Always => "always active",
            State::Active => "active",
            State::Inactive => "not active",
            State::Activates => "activates on install",
            State::Deactivates => "deactivates on install",
        }
    }
}

/// What [`Resources::set`] did with one resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Done {
    /// The draft now makes the change. For a deactivation, each sentence
    /// warns of a look that uses the resource ([`users`]).
    Changed { warnings: Vec<String> },
    /// The resource was already active, so nothing changed.
    AlreadyActive,
    /// The draft already activates the resource, so nothing changed.
    AlreadyActivates,
}

/// The current draft's view of the active resources: what the system
/// holds, the draft's changes, and what the commands and the tabs need to
/// judge a new change. [`Resources::set`] changes it in memory, and
/// [`Resources::save`] keeps it beside the draft, so several changes are
/// kept together or not at all.
pub struct Resources<'a> {
    session: &'a Session<'a>,
    pub name: String,
    /// The draft's vtium.conf, which [`users`] reads for the warnings.
    pub text: String,
    /// The installed batch file as vtium reads it. When it is missing or
    /// vtium refuses it, vtium uses the list of a fresh install.
    pub installed: Installed,
    base: Batch,
    changes: Changes,
    /// The user whose files the trust rule accepts for this install.
    euid: u32,
}

impl<'a> Resources<'a> {
    /// The current draft's active resources.
    pub fn open(session: &'a Session<'a>) -> Result<Resources<'a>, Fail> {
        let (name, text) = session.current()?;
        let installed = session.installed_batch();
        let base = installed.base(&fresh());
        let changes = session.store.batch_changes(&name)?;
        let euid = rule_user(&session.paths.targets(), &session.paths.sudo);
        Ok(Resources {
            session,
            name,
            text,
            installed,
            base,
            changes,
            euid,
        })
    }

    /// Where the resource at `path` stands in the draft.
    pub fn state(&self, path: &Path) -> State {
        match (self.base.holds(path), self.changes.get(path)) {
            (true, Some(Change::Deactivate)) => State::Deactivates,
            (true, _) => State::Active,
            (false, Some(Change::Activate)) => State::Activates,
            (false, _) => State::Inactive,
        }
    }

    /// Every resource that is active or that the draft activates or
    /// deactivates, with its state, the batch file's first and then the
    /// draft's activations, each in order. The built-in font is not
    /// listed, since it has no path.
    pub fn list(&self) -> Vec<(PathBuf, State)> {
        let held = self
            .base
            .entries()
            .iter()
            .map(|Entry::Path(p)| (p.clone(), self.state(p)));
        let added = self
            .changes
            .iter()
            .filter(|(p, c)| *c == Change::Activate && !self.base.holds(p))
            .map(|(p, _)| (p.to_path_buf(), State::Activates));
        held.chain(added).collect()
    }

    /// Activates or deactivates the resource at `path` in the draft, in
    /// memory, as the TUI's Space does. `Err` says why it is refused: a
    /// file that fails the trust rule, a deactivation of a resource that
    /// is not active, both file first, and an activation that would make
    /// the batch file longer than vtium reads ([`Resources::fits`]). A
    /// refused change leaves the draft's other changes as they were.
    pub fn set(&mut self, path: &Path, change: Change) -> Result<Done, String> {
        let before = self.changes.clone();
        let done = self.record(path, change)?;
        if self.fits(&before).is_err() {
            self.changes = before;
            return Err(format!(
                "The list of active resources is full. vtium reads at most {BATCH_KIB} KiB of the list, about a thousand paths, so vtium-config did not {} this {}. Deactivate resources you do not use to make room.",
                change.word(),
                kind(path)
            ));
        }
        Ok(done)
    }

    /// Whether the batch file that the draft's changes make fits in what
    /// vtium reads, [`active::BATCH_MAX`] bytes, or at least is no longer
    /// than the one the changes `before` make. `Err` gives its length in
    /// bytes. vtium ignores a longer file, so the install would refuse it;
    /// this check refuses the change at once instead, and keeps the file's
    /// edit, which takes time in proportion to its length, within the
    /// bound. A batch file that is not UTF-8 is not judged here, as the
    /// install refuses it with its own reason.
    pub fn fits(&self, before: &Changes) -> Result<(), usize> {
        let file = self.session.batch_file();
        let Ok(installed) = self.installed.text(&file) else {
            return Ok(());
        };
        match too_long(installed, &fresh(), before, &self.changes) {
            Some(len) => Err(len),
            None => Ok(()),
        }
    }

    /// [`Resources::set`] without the check of the batch file's length,
    /// which `activate` and `deactivate` make once after every path.
    fn record(&mut self, path: &Path, change: Change) -> Result<Done, String> {
        let state = self.state(path);
        let active = matches!(state, State::Active | State::Activates);
        match change {
            Change::Activate if state == State::Activates => return Ok(Done::AlreadyActivates),
            Change::Activate if active => return Ok(Done::AlreadyActive),
            Change::Activate if !self.base.holds(path) => trusted(path, self.euid)?,
            Change::Activate => {}
            Change::Deactivate if !active => {
                return Err(format!(
                    "{} is not active, so there is nothing to deactivate.",
                    path.display()
                ));
            }
            Change::Deactivate => {}
        }
        let warnings = match change {
            Change::Activate => Vec::new(),
            Change::Deactivate => {
                let palettes: Vec<&Path> = vtium_settings::palette::FOLDERS
                    .iter()
                    .map(Path::new)
                    .collect();
                let trust = vtium_settings::Trust::running_as(self.euid);
                let vtlook = |name: &str| vtlook_files(name, &trust);
                let places = Places {
                    frits: self.session.paths.frits.as_ref().ok().map(PathBuf::as_path),
                    palettes: &palettes,
                    vtlook: &vtlook,
                };
                users(path, &self.text, &places)
            }
        };
        let wanted = change == Change::Activate;
        self.changes.set(
            path.to_path_buf(),
            (self.base.holds(path) != wanted).then_some(change),
        );
        Ok(Done::Changed { warnings })
    }

    /// Keeps the draft's changes beside it, in `NAME.batch`.
    pub fn save(&self) -> Result<(), Fail> {
        self.session
            .store
            .set_batch_changes(&self.name, &self.changes)
    }
}

/// `activate` or `deactivate`: records `change` for each path in the
/// current draft, or refuses them all and keeps nothing.
fn change(
    session: &Session<'_>,
    change: Change,
    words: &[String],
    out: &mut dyn Write,
) -> Result<(), Fail> {
    let command = change.word();
    let asked = asked(command, words)?;
    let mut resources = Resources::open(session)?;
    let before = resources.changes.clone();
    let mut said = Vec::new();
    let mut refused = Vec::new();
    let mut changed = false;
    for word in &asked.paths {
        if builtin(word) {
            match change {
                Change::Activate => {
                    said.push("The built-in font is always active.".to_owned());
                    continue;
                }
                Change::Deactivate => {
                    return Err(Fail::Check(
                        "The built-in font is always active, so it cannot be deactivated."
                            .to_owned(),
                    ));
                }
            }
        }
        let Some(path) = plain(word) else {
            return Err(Fail::Usage(format!(
                "{} is not the path of a resource. Give its absolute path, with no .. in it, such as /usr/share/fonts/TTF/Hack-Regular.ttf.",
                word.escape_debug()
            )));
        };
        let line = format!("{}	{}", kind(&path), path.display());
        match resources.record(&path, change) {
            Ok(Done::AlreadyActive) => said.push(format!("already active	{line}")),
            Ok(Done::AlreadyActivates) => {
                said.push(format!("already activates on install	{line}"));
            }
            Ok(Done::Changed { warnings }) => {
                said.push(format!("{command}	{line}"));
                said.extend(warnings);
                changed = true;
            }
            Err(why) => refused.push(why),
        }
    }
    let name = &resources.name;
    if refused.is_empty()
        && let Err(len) = resources.fits(&before)
    {
        refused.push(format!(
            "These changes would make the list of active resources too long. vtium reads at most {BATCH_KIB} KiB of the list, about a thousand paths, and these changes make it {len} bytes. Activate fewer resources, or deactivate resources you do not use."
        ));
    }
    if !refused.is_empty() {
        return Err(Fail::Check(format!(
            "{}\nNothing was changed: draft {name} keeps the active resources it had.",
            refused.join("\n")
        )));
    }
    for line in &said {
        let _ = writeln!(out, "{line}");
    }
    if asked.dry_run {
        let _ = writeln!(out, "nothing was saved (--dry-run)");
        return Ok(());
    }
    resources.save()?;
    // a change waits beside the draft for the install, which the state
    // line counts; a check that cannot run is said and fails nothing,
    // since the change is kept whatever the check says
    if changed {
        let path = session.store.draft_path(name)?;
        let report = session
            .check(name, &resources.text, &path)
            .and_then(|checked| session.report(name, &resources.text, &checked));
        match report {
            Ok(lines) => {
                for line in lines {
                    let _ = writeln!(out, "{line}");
                }
            }
            Err(fail) if !asked.install => {
                let _ = writeln!(out, "{}", fail.text());
            }
            Err(fail) => return Err(fail),
        }
    }
    if asked.install {
        session.install(
            name,
            &resources.text,
            Unrouted::Refuse,
            &mut run_child,
            None,
            &mut *out,
        )?;
    }
    Ok(())
}

/// `active`: the built-in font, then each resource the batch file lists
/// and each the draft activates, one line each, tab-separated: its kind,
/// its path, and its [`State`].
fn list(session: &Session<'_>, words: &[String], out: &mut dyn Write) -> Result<(), Fail> {
    if !words.is_empty() {
        return Err(Fail::Usage("active takes no argument".to_owned()));
    }
    let resources = Resources::open(session)?;
    let _ = writeln!(out, "font\tbuilt-in\t{}", State::Always.words());
    for (path, state) in resources.list() {
        let _ = writeln!(
            out,
            "{}\t{}\t{}",
            kind(&path),
            path.display(),
            state.words()
        );
    }
    match &resources.installed {
        Installed::Read(_) => {}
        Installed::Missing => {
            let _ = writeln!(
                out,
                "{} does not exist, so vtium uses the resources of a fresh install.",
                session.batch_file().display()
            );
        }
        Installed::Refused(why) => {
            let _ = writeln!(out, "{why}");
        }
    }
    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking"
)]
mod tests {
    thread_local! {
        /// How many times this thread's `Changes` walked their whole list
        /// to build the index again.
        pub(super) static LIST_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    use super::*;

    fn batch(paths: &[&str]) -> Batch {
        Batch::new(
            paths
                .iter()
                .map(|p| Entry::Path(PathBuf::from(p)))
                .collect(),
        )
    }

    #[test]
    fn a_changes_file_reads_back_as_it_was_written() {
        let mut changes = Changes::default();
        changes.set("/a/x.ttf".into(), Some(Change::Activate));
        changes.set("/a/y.ttf".into(), Some(Change::Deactivate));
        changes.set("/a/x.ttf".into(), Some(Change::Deactivate));
        let text = changes.text("desk");
        assert!(
            text.ends_with("deactivate /a/x.ttf\ndeactivate /a/y.ttf\n"),
            "{text}"
        );
        assert_eq!(
            Changes::parse(&text, Path::new("d.batch")),
            Ok(changes.clone())
        );
        // taking a change back keeps the others found by their paths
        changes.set("/a/x.ttf".into(), None);
        assert_eq!(changes.get(Path::new("/a/x.ttf")), None);
        assert_eq!(changes.get(Path::new("/a/y.ttf")), Some(Change::Deactivate));
        changes.set("/a/z.ttf".into(), Some(Change::Activate));
        assert_eq!(changes.get(Path::new("/a/z.ttf")), Some(Change::Activate));
        assert_eq!(changes.len(), 2);
        for bad in [
            "activate x.ttf",
            "activate /a/../b",
            "install /a",
            "activate",
            "activate /a/\tb.ttf",
        ] {
            let fail = Changes::parse(bad, Path::new("d.batch")).unwrap_err();
            assert!(fail.text().starts_with("d.batch line 1: "), "{bad}");
        }
    }

    /// A large `NAME.batch` reads in one pass: each line finds its path
    /// through the index, so the read never walks the list again. The test
    /// counts the passes over the list instead of timing the read.
    #[test]
    fn a_large_changes_file_reads_without_a_scan_for_each_line() {
        let text: String = (0..26_000)
            .map(|n| {
                format!("activate /fonts/font-{n:05}.ttf\ndeactivate /fonts/font-{n:05}.ttf\n")
            })
            .collect();
        LIST_PASSES.with(|n| n.set(0));
        let changes = Changes::parse(&text, Path::new("d.batch")).expect("the file reads");
        assert_eq!(LIST_PASSES.with(std::cell::Cell::get), 0);
        assert_eq!(changes.len(), 26_000);
        assert_eq!(changes.index.len(), 26_000);
        for (at, (path, change)) in changes.list.iter().enumerate() {
            assert_eq!(changes.index.get(path), Some(&at));
            assert_eq!(*change, Change::Deactivate);
        }
    }

    #[test]
    fn the_changes_apply_to_the_installed_file_and_keep_its_other_lines() {
        let fresh = batch(&["/f/one", "/f/two"]);
        let mut changes = Changes::default();
        changes.set("/n/new".into(), Some(Change::Activate));
        changes.set("/k/kept".into(), Some(Change::Deactivate));
        let installed = "# mine\n/k/kept\n\n/k//other\n";
        assert_eq!(
            applied(Some(installed), &fresh, &changes).as_deref(),
            Some("# mine\n\n/k//other\n/n/new\n")
        );
        // none installed: the fresh list, then the change
        let text = applied(None, &fresh, &changes).expect("a new file");
        assert!(text.starts_with(HEADER), "{text}");
        assert!(text.ends_with("/f/one\n/f/two\n/n/new\n"), "{text}");
        // changes the system already matches change nothing
        let mut same = Changes::default();
        same.set("/f/one".into(), Some(Change::Activate));
        same.set("/gone".into(), Some(Change::Deactivate));
        assert_eq!(applied(None, &fresh, &same), None);
        assert_eq!(applied(Some("/f/one\n"), &fresh, &same), None);
        same.settle(&fresh);
        assert!(same.is_empty());
    }

    #[test]
    fn the_built_in_font_is_named_by_its_names() {
        for word in ["unifont", "unscii-8", "builtin", "builtin:unifont"] {
            assert!(builtin(word), "{word}");
        }
        assert!(!builtin("/usr/share/fonts/unifont.ttf"));
        assert_eq!(plain("/a//b/./c"), Some(PathBuf::from("/a/b/c")));
        assert_eq!(plain("a/b"), None);
        assert_eq!(plain("/a/../b"), None);
        assert_eq!(plain("/a\n/b"), None);
        // a control character would break `active`'s tab-separated fields
        // or reach the terminal raw
        for bad in [
            "/a\tb.ttf",
            "/a\u{1b}[31mb.ttf",
            "/a\rb",
            "/a\u{7f}b",
            "/a\u{9b}b",
        ] {
            assert_eq!(plain(bad), None, "{bad:?}");
        }
        assert_eq!(plain("/a b.ttf"), Some(PathBuf::from("/a b.ttf")));
    }

    // A look that names no vtlook draws the top level's, and one that
    // names `default` draws the built-in vtlook. The top level itself is
    // named only where a head draws it, because the head names no look or
    // the text has no heads.
    #[test]
    fn a_look_that_takes_the_top_level_s_vtlook_uses_its_files() {
        let sepia = Path::new("/etc/vtium/looks/sepia.cube");
        let files = |name: &str| match name {
            "sepia" => vec![sepia.to_path_buf()],
            _ => Vec::new(),
        };
        let places = Places {
            frits: None,
            palettes: &[],
            vtlook: &files,
        };
        let looks = "vtlook = sepia\nversion = 1\n[look desk]\n[look plain]\nvtlook = default\n[look own]\nvtlook = sepia\n";
        let named = |conf: &str| -> Vec<Option<String>> {
            uses(sepia, conf, &places)
                .into_iter()
                .map(|u| u.look)
                .collect()
        };
        let some = |n: &str| Some(n.to_owned());
        assert_eq!(named(looks), [None, some("desk"), some("own")]);
        let headed = format!("{looks}[head h]\nlook = desk\n");
        assert_eq!(named(&headed), [some("desk"), some("own")]);
        let bare = format!("{looks}[head h]\nlook = desk\n[head g]\n");
        assert_eq!(named(&bare), [None, some("desk"), some("own")]);
        assert_eq!(
            uses(sepia, &headed, &places)
                .first()
                .map(|u| u.through.clone()),
            Some(Through::Vtlook("sepia".to_owned()))
        );
        assert_eq!(
            drawing_vtlook(&headed, "sepia"),
            [some("desk"), some("own")]
        );
        assert_eq!(
            users(sepia, &headed, &places).first().map(String::as_str),
            Some(
                "Look desk uses this color map through the vtlook sepia. From the next login after the install, look desk renders with no filter."
            )
        );
    }

    #[test]
    fn a_look_that_names_a_color_scheme_is_warned_of() {
        // vtium reads a look's `palette = NAME` from NAME.palette in the
        // first folder that holds one, so the administrator's file
        // shadows the package's
        let root = std::env::temp_dir().join(format!("vtium-config-users-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (etc, pkg) = (root.join("etc"), root.join("pkg"));
        std::fs::create_dir_all(&etc).unwrap();
        std::fs::create_dir_all(&pkg).unwrap();
        for file in [pkg.join("dracula.palette"), pkg.join("top.palette")] {
            std::fs::write(file, "palette = 0=#000000\n").unwrap();
        }
        let conf = "palette = top\nversion = 1\n[look big]\npalette = dracula\n";
        let none = |_: &str| Vec::new();
        let places = Places {
            frits: None,
            palettes: &[&etc, &pkg],
            vtlook: &none,
        };
        let big = [
            "Look big uses this color scheme. From the next login after the install, look big renders without it.",
        ];
        assert_eq!(users(&pkg.join("dracula.palette"), conf, &places), big);
        // the text has no heads, so the console draws the top level
        assert_eq!(
            users(&pkg.join("top.palette"), conf, &places),
            [
                "The top level of vtium.conf uses this color scheme. From the next login after the install, each head that uses the top level renders without it."
            ]
        );
        // a path that is not the file vtium reads warns of nothing
        assert!(users(&pkg.join("dracula"), conf, &places).is_empty());
        assert!(users(&root.join("q/dracula.palette"), conf, &places).is_empty());
        // once the administrator's copy exists, vtium reads that one
        std::fs::write(etc.join("dracula.palette"), "palette = 0=#000000\n").unwrap();
        assert!(users(&pkg.join("dracula.palette"), conf, &places).is_empty());
        assert_eq!(users(&etc.join("dracula.palette"), conf, &places), big);
        let _ = std::fs::remove_dir_all(&root);
    }
}
