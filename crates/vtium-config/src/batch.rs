//! The active resources beside a draft, and the commands `activate`,
//! `deactivate` and `active`.
//!
//! vtium draws only from the active resources: the fonts, color schemes,
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

use std::io::Write;
use std::path::{Path, PathBuf};

use vtium_settings::batch::{self as active, Batch, Entry};

use crate::apply::{Route, Targets};
use crate::drafts::{Session, Unrouted, run_child};
use crate::outcome::Fail;
use crate::store::Store;
use crate::tui::Paths;

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
/// at most one for each path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    list: Vec<(PathBuf, Change)>,
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
            "# The changes that draft {draft} makes to the resources vtium may draw\n# from. vtium-config applies them when it installs the draft.\n"
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
        self.list
            .iter()
            .find_map(|(p, c)| (p == path).then_some(*c))
    }

    /// Makes `change` the draft's change for `path`, or with `None` takes
    /// the draft's change for it back.
    pub fn set(&mut self, path: PathBuf, change: Option<Change>) {
        match (self.list.iter_mut().find(|(p, _)| *p == path), change) {
            (Some(slot), Some(change)) => slot.1 = change,
            (None, Some(change)) => self.list.push((path, change)),
            (_, None) => self.list.retain(|(p, _)| *p != path),
        }
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
    }
}

/// `word` as a path the batch file can list: an absolute path without
/// `..`, with `.` and doubled slashes dropped, read as vtium reads a line
/// of the batch file.
pub fn plain(word: &str) -> Option<PathBuf> {
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
    if targets.configdir == crate::cli::default_configdir() {
        PathBuf::from(active::BATCH_FILE)
    } else {
        targets.configdir.join("batch.conf")
    }
}

/// The list of a fresh install on this machine: the fonts and color
/// schemes the package ships, and the console font that vconsole.conf's
/// `FONT=` names. vtium uses it while no batch file is installed.
pub fn fresh() -> Batch {
    let system = crate::serve::Paths::default();
    let conf = std::fs::read_to_string(&system.vconsole_conf).unwrap_or_default();
    let cmdline = std::fs::read_to_string(&system.kernel_cmdline).unwrap_or_default();
    let font = active::vconsole(&conf, &cmdline).font;
    let folders: Vec<&Path> = active::CONSOLE_FONT_FOLDERS.iter().map(Path::new).collect();
    Batch::fresh_install(
        Path::new(active::PACKAGE_FONTS),
        Path::new(vtium_settings::palette::PACKAGE_PALETTES),
        font.as_deref(),
        &folders,
    )
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

/// The comment that opens a batch file vtium-config writes when none is
/// installed. It is wrapped at 78 columns, as vtium.conf's comments are.
pub const HEADER: &str = "\
# vtium draws only from the resources this file lists: fonts, color
# schemes, color maps, shaders and pointer themes, one absolute path on each
# line. The built-in font is always active and is not listed here.
#
# vtium-config wrote this file from the list of a fresh install, the first
# time a draft changed the active resources. vtium-config activate and
# vtium-config deactivate change it through a draft, and root can edit it.

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
    let mut lines: Vec<String> = match installed {
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
    for (path, change) in changes.iter() {
        let names = |line: &String| line_path(line).as_deref() == Some(path);
        match change {
            Change::Deactivate => lines.retain(|l| !names(l)),
            Change::Activate if !lines.iter().any(names) => lines.push(path.display().to_string()),
            Change::Activate => {}
        }
    }
    let mut text = lines.join("\n");
    text.push('\n');
    let after = Batch::parse(text.as_bytes(), Path::new("")).batch;
    (after != base(installed, fresh)).then_some(text)
}

/// What kind of resource `path` is, in the words a person uses: a folder
/// or a `.svg` file is a pointer theme, a `.cube` file a color map, a
/// `.glsl` file a shader, a file in a color scheme folder a color scheme,
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
    } else if extension == "glsl" {
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
    use vtium_settings::cullet::{Rule, open, open_in_theme};
    let rule = Rule::running_as(euid);
    let mounts = vtium_settings::mounts::Mounts::system().map_err(|e| {
        format!(
            "{}: vtium-config cannot check this file, because it cannot read the mount table: {e}.",
            path.display()
        )
    })?;
    let is_folder = std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir());
    if !is_folder {
        return open(path, &rule, &mounts)
            .map(drop)
            .map_err(|refused| refused.to_string());
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
    open_in_theme(path, picture, &rule, &mounts)
        .map(drop)
        .map_err(|refused| refused.to_string())
}

/// Where [`users`] finds what the looks name: the font sets' folder, and
/// the folders of the color schemes.
pub struct Places<'a> {
    pub frits: Option<&'a Path>,
    pub palettes: &'a [&'a Path],
}

/// One look of vtium.conf's text, or its top level, with the font set and
/// the color scheme it names.
struct User {
    /// `None` for the top level.
    look: Option<String>,
    frit: Option<String>,
    palette: Option<String>,
}

/// The top level of vtium.conf's text `conf` and each of its looks.
fn looks_of(conf: &str) -> Vec<User> {
    use vtium_settings::keys::{FONT_NAME, PALETTE};
    let mut users = vec![User {
        look: None,
        frit: vtium_settings::setting(conf, FONT_NAME.name),
        palette: vtium_settings::setting(conf, PALETTE.name),
    }];
    let Some(reading) = vtium_settings::sections(conf.as_bytes()) else {
        return users;
    };
    for look in reading.parsed.config.looks {
        let value = |key: &str| {
            look.settings
                .iter()
                .rev()
                .find(|s| s.key.name == key)
                .map(|s| s.value.clone())
        };
        users.push(User {
            frit: value(FONT_NAME.name),
            palette: value(PALETTE.name),
            look: Some(look.name),
        });
    }
    users
}

/// The sentences that warn, before `path` is deactivated, of each look in
/// vtium.conf's text `conf` that uses it, where vtium-config can tell
/// without reading a font or a vtlook: a font set that names the font
/// file by its path, and a color scheme a look names. A font a set names
/// by its family is found only by the font helper, and the color maps and
/// shaders a vtlook names only by the vtlook reader, which this build of
/// vtium-config leaves out, so no warning names them.
pub fn users(path: &Path, conf: &str, places: &Places<'_>) -> Vec<String> {
    let mut out = Vec::new();
    for user in looks_of(conf) {
        let who = user.look.as_ref().map_or_else(
            || "The top level of vtium.conf".to_owned(),
            |name| format!("Look {name}"),
        );
        let after = user.look.as_ref().map_or_else(
            || "the looks that take their settings from the top level draw without it".to_owned(),
            |name| format!("look {name} draws without it"),
        );
        let mut uses = None;
        if let (Some(frit), Some(frits)) = (&user.frit, places.frits)
            && frit_names(frits, frit, path)
        {
            uses = Some(format!("this font through the font set {frit}"));
        }
        if let Some(palette) = &user.palette
            && palette_file(places.palettes, palette).as_deref() == Some(path)
        {
            uses = Some("this color scheme".to_owned());
        }
        if let Some(uses) = uses {
            out.push(format!(
                "{who} uses {uses}. Once this change is installed, {after}."
            ));
        }
    }
    out
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
    /// memory. `Err` says, file first, why it is refused: a file that fails
    /// the trust rule, and a deactivation of a resource that is not
    /// active. A change that brings the resource back to what the system
    /// holds takes the draft's change back.
    pub fn set(&mut self, path: &Path, change: Change) -> Result<Done, String> {
        let active = matches!(self.state(path), State::Active | State::Activates);
        match change {
            Change::Activate if active => return Ok(Done::AlreadyActive),
            Change::Activate => trusted(path, self.euid)?,
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
                let places = Places {
                    frits: self.session.paths.frits.as_ref().ok().map(PathBuf::as_path),
                    palettes: &palettes,
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
    let mut said = Vec::new();
    let mut refused = Vec::new();
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
                "{word} is not the path of a resource. Give its absolute path without .., such as /usr/share/fonts/TTF/Hack-Regular.ttf."
            )));
        };
        let line = format!("{}	{}", kind(&path), path.display());
        match resources.set(&path, change) {
            Ok(Done::AlreadyActive) => said.push(format!("already active	{line}")),
            Ok(Done::Changed { warnings }) => {
                said.push(format!("{command}	{line}"));
                said.extend(warnings);
            }
            Err(why) => refused.push(why),
        }
    }
    let name = &resources.name;
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
        return Ok(());
    }
    resources.save()?;
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
        assert_eq!(Changes::parse(&text, Path::new("d.batch")), Ok(changes));
        for bad in [
            "activate x.ttf",
            "activate /a/../b",
            "install /a",
            "activate",
        ] {
            let fail = Changes::parse(bad, Path::new("d.batch")).unwrap_err();
            assert!(fail.text().starts_with("d.batch line 1: "), "{bad}");
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
        let places = Places {
            frits: None,
            palettes: &[&etc, &pkg],
        };
        let big = [
            "Look big uses this color scheme. Once this change is installed, look big draws without it.",
        ];
        assert_eq!(users(&pkg.join("dracula.palette"), conf, &places), big);
        assert_eq!(
            users(&pkg.join("top.palette"), conf, &places),
            [
                "The top level of vtium.conf uses this color scheme. Once this change is installed, the looks that take their settings from the top level draw without it."
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
