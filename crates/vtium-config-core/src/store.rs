//! vtium-config's store (ADR-1403 D8): the drafts, the library, the tool's
//! own settings, the installed history and each draft's base, under the
//! user's XDG directories.
//!
//! | Path | What it holds |
//! |---|---|
//! | `$XDG_CONFIG_HOME/vtium/drafts/NAME.conf` | each draft, a whole vtium.conf |
//! | `$XDG_CONFIG_HOME/vtium/drafts/NAME.batch` | each draft's changes to the active resources, when it has any ([`crate::batch`]) |
//! | `$XDG_CONFIG_HOME/vtium/library.conf` | the library, in vtium.conf's section syntax |
//! | `$XDG_CONFIG_HOME/vtium/vtium-config.conf` | `draft = NAME`, the current draft, and `history = N` |
//! | `$XDG_CONFIG_HOME/vtium/installed/DRAFT-N.conf` | each installed file an install replaced |
//! | `$XDG_STATE_HOME/vtium/bases/NAME.base` | each draft's base: the SHA-256 of the installed file it was made from or installed as, or `none` |
//! | `$XDG_STATE_HOME/vtium/bases/NAME.conf` | the base's text beside its hash, which Update merges from; absent when no file was installed |
//!
//! Run as root, vtium-config finds root's home from the password database
//! and never from `$HOME` or the XDG variables, which `sudo` can carry
//! over from the user who ran it, so a root draft lands only in root's own
//! home ([`Store::locate`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use vtium_font::{Sha256, sha256_hex};

use crate::batch::Changes;
use crate::outcome::Fail;

/// The tool's settings file, in the store's configuration directory.
pub const SETTINGS: &str = "vtium-config.conf";

/// The library, in the store's configuration directory.
pub const LIBRARY: &str = "library.conf";

/// How many installed files the history keeps when `history =` is unset.
pub const HISTORY_DEFAULT: usize = 10;

/// The name a draft takes when the machine's hostname gives none.
const NAME_FALLBACK: &str = "draft";

/// The store's two directories.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Store {
    /// `$XDG_CONFIG_HOME/vtium`: the drafts, the library, the settings and
    /// the installed history.
    pub config: PathBuf,
    /// `$XDG_STATE_HOME/vtium`: the bases, the install bundles and the
    /// copies `edit` opens.
    pub state: PathBuf,
}

/// The base of a draft: the hash of the installed file it was made from
/// or installed as, `None` when no file was installed.
pub type Base = Option<String>;

/// The SHA-256 of `bytes`, as a base holds it.
pub fn hash(bytes: &[u8]) -> String {
    sha256_hex(&Sha256::digest(bytes))
}

/// Whether `name` can name a draft: 1 to 64 letters, digits, `.`, `_`,
/// `+` and `-`, not starting with `.` or `-`.
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && !name.starts_with(['.', '-'])
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

/// `name` when it can name a draft, else the usage error, exit 2, that
/// says what a name may hold. Every path built from a draft's name goes
/// through this, so `../library` or `a/b` never names a file.
pub fn named(name: &str) -> Result<&str, Fail> {
    if valid_name(name) {
        Ok(name)
    } else {
        Err(Fail::Usage(format!(
            "{name:?} cannot be a draft's name: a name has 1 to 64 letters, digits, '.', '_', '+' or '-', and cannot start with '.' or '-'"
        )))
    }
}

/// `name` made into a draft's name: each character a name cannot hold
/// becomes `-`, and a name left empty is `NAME_FALLBACK`.
pub fn name_from(name: &str) -> String {
    let made: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-') {
                c
            } else {
                '-'
            }
        })
        .take(56)
        .collect();
    let made = made.trim_start_matches(['.', '-']).to_owned();
    if valid_name(&made) {
        made
    } else {
        NAME_FALLBACK.to_owned()
    }
}

/// Whether the guard against the real user's store is on: in a debug
/// build, `debug`, run under cargo, which sets `CARGO_MANIFEST_DIR` for
/// the processes it runs, as `var` reads it. A release build, the one
/// installed for users, never refuses a user's own store whatever its
/// environment holds.
pub fn armed(debug: bool, var: impl Fn(&str) -> Option<OsString>) -> bool {
    debug && var("CARGO_MANIFEST_DIR").is_some()
}

/// The home the password database gives the user `uid`.
pub fn home_of(uid: u32) -> Option<PathBuf> {
    nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
        .ok()
        .flatten()
        .map(|u| u.dir)
}

/// The machine's hostname, as the kernel gives it, made into a draft's
/// name.
pub fn hostname() -> String {
    let name = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    name_from(&name)
}

/// The tool's settings: the current draft and how many installed files the
/// history keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub draft: Option<String>,
    pub history: usize,
}

/// A file of the installed history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    /// The draft whose install replaced it.
    pub draft: String,
    /// Its number among that draft's, from 1.
    pub number: u32,
    pub path: PathBuf,
    /// When it was replaced, in seconds since 1970.
    pub when: u64,
}

impl Installed {
    /// Its file's name, `DRAFT-N.conf`.
    pub fn file_name(&self) -> String {
        format!("{}-{}.conf", self.draft, self.number)
    }
}

/// A file's text, `None` when it does not exist; a file that cannot be read
/// is exit 5's failure, naming its path.
fn read(path: &Path) -> Result<Option<String>, Fail> {
    crate::places::read_conf(path).map_err(Fail::File)
}

/// The temporary file [`write()`] writes before it renames the file over
/// `name`: `.NAME.new-PID`, with this process's id.
fn temporary(name: &str) -> String {
    format!(".{name}.new-{}", std::process::id())
}

/// Whether `file` is a name that [`write()`] gives its temporary file, of
/// this process or of any other: `.NAME.new-PID`. A process killed between
/// the write and the rename leaves one behind, and another process sees
/// one while its write is under way, so a reader of the folder skips it.
fn is_temporary(file: &str) -> bool {
    file.strip_prefix('.')
        .and_then(|rest| rest.rsplit_once(".new-"))
        .is_some_and(|(name, pid)| {
            !name.is_empty() && !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit())
        })
}

/// Writes `text` to `path` through a new file renamed over it, making the
/// directory first, so a draft is never left half written.
pub fn write(path: &Path, text: &str) -> Result<(), Fail> {
    let fail = |e: std::io::Error| Fail::File(format!("{}: {e}", path.display()));
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir).map_err(fail)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let new = dir.join(temporary(&name));
    std::fs::write(&new, text).map_err(fail)?;
    std::fs::rename(&new, path).map_err(|e| {
        let _ = std::fs::remove_file(&new);
        fail(e)
    })
}

/// The font set changes a draft's folder holds ([`Store::frit_changes`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FritChanges {
    /// Each frit the draft adds or replaces, by name, with its whole text.
    pub kept: std::collections::BTreeMap<String, String>,
    /// Each installed frit the draft removes, by name.
    pub removed: std::collections::BTreeSet<String>,
    /// Each entry of the folder that is not a font set change, by its
    /// path and why. The check lists each one, and install refuses the
    /// draft while one is there.
    pub problems: Vec<String>,
}

impl FritChanges {
    /// How many font set changes the folder holds.
    pub fn len(&self) -> usize {
        self.kept.len() + self.removed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0 && self.problems.is_empty()
    }
}

/// `frit` when it is a name a frit can have and no built-in font's.
fn frit_name(frit: &str) -> Result<&str, Fail> {
    if crate::frit::RESERVED.contains(&frit) {
        return Err(Fail::Usage(format!(
            "{frit} is a built-in font, not a font set"
        )));
    }
    if !crate::frit::valid_name(frit) {
        return Err(Fail::Usage(format!(
            "{frit:?} is not a name a font set can have"
        )));
    }
    Ok(frit)
}

/// A frit's file in a draft's folder, read whole when it is at most
/// [`crate::frit::FRIT_BYTES_MAX`] bytes of UTF-8.
fn read_bounded(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| {
            f.take(
                u64::try_from(crate::frit::FRIT_BYTES_MAX)
                    .unwrap_or(0)
                    .saturating_add(1),
            )
            .read_to_end(&mut bytes)
        })
        .map_err(|e| e.to_string())?;
    if bytes.len() > crate::frit::FRIT_BYTES_MAX {
        return Err(format!(
            "this font set is larger than {} bytes",
            crate::frit::FRIT_BYTES_MAX
        ));
    }
    String::from_utf8(bytes).map_err(|_| "this font set is not UTF-8 text".to_owned())
}

/// Removes `path`; a file already gone is no failure.
fn remove(path: &Path) -> Result<(), Fail> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Fail::File(format!("{}: {e}", path.display()))),
    }
}

impl Store {
    /// The store of the process whose effective uid is `euid`: as root,
    /// under the home `home_of(0)` gives, the password database's, and
    /// never from `var`; as anyone else, under `$XDG_CONFIG_HOME` and
    /// `$XDG_STATE_HOME`, each `~/.config` and `~/.local/state` from
    /// `$HOME` when unset or empty. `var` reads the environment.
    pub fn locate(
        euid: u32,
        var: impl Fn(&str) -> Option<OsString>,
        home_of: impl Fn(u32) -> Option<PathBuf>,
    ) -> Result<Store, String> {
        if euid == 0 {
            let home = home_of(0).ok_or_else(|| {
                "vtium-config has nowhere to keep root's draft: the password database gives root no home directory".to_owned()
            })?;
            return Ok(Store {
                config: home.join(".config/vtium"),
                state: home.join(".local/state/vtium"),
            });
        }
        let set = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = set("HOME");
        let under = |xdg: &str, fallback: &str| {
            set(xdg)
                .or_else(|| home.as_ref().map(|h| h.join(fallback)))
                .map(|d| d.join("vtium"))
                .ok_or_else(|| {
                    format!(
                        "vtium-config has nowhere to keep its drafts: neither {xdg} nor HOME is set"
                    )
                })
        };
        Ok(Store {
            config: under("XDG_CONFIG_HOME", ".config")?,
            state: under("XDG_STATE_HOME", ".local/state")?,
        })
    }

    /// This process's store. A debug build run under cargo, as every test
    /// of this crate is, refuses the real user's own store
    /// ([`Store::guard`]); a release build never does ([`armed`]).
    pub fn system() -> Result<Store, Fail> {
        let euid = rustix::process::geteuid().as_raw();
        let store = Store::locate(euid, |name: &str| std::env::var_os(name), home_of)
            .map_err(Fail::File)?;
        let armed = armed(cfg!(debug_assertions), |name: &str| std::env::var_os(name));
        store
            .guard(armed, home_of(euid).as_deref())
            .map_err(Fail::File)?;
        Ok(store)
    }

    /// Refuses, under cargo, the store of the real user whose home the
    /// password database gives as `home`: `~/.config/vtium` or
    /// `~/.local/state/vtium`. A test that forgets to point `HOME`,
    /// `XDG_CONFIG_HOME` and `XDG_STATE_HOME` at a directory of its own
    /// then fails loudly, and the real user's drafts are never touched.
    pub fn guard(&self, under_cargo: bool, home: Option<&Path>) -> Result<(), String> {
        let Some(home) = home.filter(|_| under_cargo) else {
            return Ok(());
        };
        let real = [home.join(".config/vtium"), home.join(".local/state/vtium")];
        if real.contains(&self.config) || real.contains(&self.state) {
            return Err(format!(
                "run under cargo, vtium-config refuses the real user's store under {}: a test sets HOME, XDG_CONFIG_HOME and XDG_STATE_HOME to a directory of its own",
                home.display()
            ));
        }
        Ok(())
    }

    pub fn drafts_dir(&self) -> PathBuf {
        self.config.join("drafts")
    }

    /// The draft `name`'s file. Every path the store builds from a draft's
    /// name is built here or in `Store::base_path`, [`Store::batch_path`],
    /// [`Store::edit_copy`] and [`Store::save_history`], each through
    /// [`named`], so no name can reach outside the store.
    pub fn draft_path(&self, name: &str) -> Result<PathBuf, Fail> {
        Ok(self.drafts_dir().join(format!("{}.conf", named(name)?)))
    }

    /// The file beside the draft `name` that keeps its changes to the
    /// active resources, `NAME.batch`.
    pub fn batch_path(&self, name: &str) -> Result<PathBuf, Fail> {
        Ok(self.drafts_dir().join(format!("{}.batch", named(name)?)))
    }

    /// The draft `name`'s changes to the active resources; none when it
    /// has no file of them.
    pub fn batch_changes(&self, name: &str) -> Result<Changes, Fail> {
        let path = self.batch_path(name)?;
        match read(&path)? {
            Some(text) => Changes::parse(&text, &path),
            None => Ok(Changes::default()),
        }
    }

    /// Keeps `changes` as the draft `name`'s changes to the active
    /// resources. A draft with no change has no file, so this removes the
    /// file when `changes` is empty.
    pub fn set_batch_changes(&self, name: &str, changes: &Changes) -> Result<(), Fail> {
        let path = self.batch_path(name)?;
        if changes.is_empty() {
            remove(&path)
        } else {
            write(&path, &changes.text(name))
        }
    }

    /// The folder beside the draft `name` that keeps its font set changes,
    /// `NAME.frits/`. A draft that never held a font set change has no
    /// folder, and one whose changes were all taken back or installed may
    /// keep an empty folder, which holds no change.
    pub fn frits_path(&self, name: &str) -> Result<PathBuf, Fail> {
        Ok(self.drafts_dir().join(format!("{}.frits", named(name)?)))
    }

    /// The draft `name`'s font set changes as its folder holds them: each
    /// `FRIT.vtfrit`, a frit the draft adds or replaces, and each
    /// `FRIT.removed`, an installed frit it removes. vtium-config reads only
    /// regular files whose names are a frit's name with one of those two
    /// endings, each at most the largest frit the glyph helper reads, and
    /// lists every other entry of the folder as a problem. It skips the
    /// temporary files that [`write()`] makes on the way to one of those
    /// files, so a write cut short or under way in another process blocks
    /// no install. A missing folder and an empty one both hold no change.
    ///
    /// References:
    /// - ADR-1403 D8, "A draft's font set changes": the folder and what it may hold.
    pub fn frit_changes(&self, name: &str) -> Result<FritChanges, Fail> {
        let dir = self.frits_path(name)?;
        let mut changes = FritChanges::default();
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(changes),
            Err(e) => return Err(Fail::File(format!("{}: {e}", dir.display()))),
        };
        for entry in entries {
            let entry = entry.map_err(|e| Fail::File(format!("{}: {e}", dir.display())))?;
            let path = entry.path();
            let file = entry.file_name().to_string_lossy().into_owned();
            if is_temporary(&file) {
                continue;
            }
            let problem = |why: &str| format!("{}: {why}", path.display());
            let regular = entry.file_type().is_ok_and(|t| t.is_file());
            let frit = |ending: &str| {
                file.strip_suffix(ending)
                    .filter(|n| crate::frit::valid_name(n) && !crate::frit::RESERVED.contains(n))
                    .map(str::to_owned)
            };
            match (frit(".vtfrit"), frit(".removed")) {
                _ if !regular => changes
                    .problems
                    .push(problem("this is not a regular file, so it is not a font set change")),
                (Some(frit), _) => match read_bounded(&path) {
                    Ok(text) => {
                        changes.kept.insert(frit, text);
                    }
                    Err(why) => changes.problems.push(problem(&why)),
                },
                (_, Some(frit)) => {
                    changes.removed.insert(frit);
                }
                _ => changes.problems.push(problem(
                    "this file is not a font set change. A draft's font set folder holds only NAME.vtfrit and NAME.removed files.",
                )),
            }
        }
        // the folder never holds both files for one frit; where someone put
        // both there, the removal is the problem the check names
        for frit in changes.kept.keys() {
            if changes.removed.remove(frit) {
                changes.problems.push(format!(
                    "{}: the folder both adds and removes the font set {frit}",
                    dir.display()
                ));
            }
        }
        Ok(changes)
    }

    /// Keeps the frit `frit`, `text`, as a font set the draft `name` adds
    /// or replaces: writes `FRIT.vtfrit` and deletes `FRIT.removed`.
    pub fn keep_frit(&self, name: &str, frit: &str, text: &str) -> Result<(), Fail> {
        let dir = self.frits_path(name)?;
        let frit = frit_name(frit)?;
        write(&dir.join(format!("{frit}.vtfrit")), text)?;
        remove(&dir.join(format!("{frit}.removed")))
    }

    /// Keeps the removal of the installed frit `frit` in the draft `name`:
    /// writes an empty `FRIT.removed` and deletes `FRIT.vtfrit`.
    pub fn keep_removal(&self, name: &str, frit: &str) -> Result<(), Fail> {
        let dir = self.frits_path(name)?;
        let frit = frit_name(frit)?;
        write(&dir.join(format!("{frit}.removed")), "")?;
        remove(&dir.join(format!("{frit}.vtfrit")))
    }

    /// Takes the draft `name`'s change to the frit `frit` back, whichever
    /// it is. The folder stays, even when it is left empty: another
    /// vtium-config may have just made the folder to keep a change of its
    /// own and not yet written it, and removing the folder then would make
    /// that write fail. An empty folder holds no change wherever
    /// vtium-config reads it ([`Store::frit_changes`]), and `revert`, `new`
    /// and `draft rm` remove the folder with every change in it.
    pub fn forget_frit(&self, name: &str, frit: &str) -> Result<(), Fail> {
        let dir = self.frits_path(name)?;
        let frit = frit_name(frit)?;
        remove(&dir.join(format!("{frit}.vtfrit")))?;
        remove(&dir.join(format!("{frit}.removed")))
    }

    /// Drops every font set change of the draft `name`, as `revert`, `new`
    /// and `draft rm` do: deletes its folder.
    pub fn drop_frits(&self, name: &str) -> Result<(), Fail> {
        let dir = self.frits_path(name)?;
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Fail::File(format!("{}: {e}", dir.display()))),
        }
    }

    /// Gives the draft `to` the font set changes of the draft `from`, as
    /// `draft new --from DRAFT` does, in place of any it had.
    pub fn copy_frits(&self, from: &str, to: &str) -> Result<(), Fail> {
        let changes = self.frit_changes(from)?;
        self.drop_frits(to)?;
        for (frit, text) in &changes.kept {
            self.keep_frit(to, frit, text)?;
        }
        for frit in &changes.removed {
            self.keep_removal(to, frit)?;
        }
        Ok(())
    }

    /// Where `edit` copies the draft `name` for the editor.
    pub fn edit_copy(&self, name: &str) -> Result<PathBuf, Fail> {
        Ok(self
            .state
            .join(format!("edit-{}-{}.conf", named(name)?, std::process::id())))
    }

    pub fn library_path(&self) -> PathBuf {
        self.config.join(LIBRARY)
    }

    fn settings_path(&self) -> PathBuf {
        self.config.join(SETTINGS)
    }

    fn base_path(&self, name: &str) -> Result<PathBuf, Fail> {
        Ok(self
            .state
            .join("bases")
            .join(format!("{}.base", named(name)?)))
    }

    /// Where the draft `name`'s base keeps its text, beside its hash.
    fn base_text_path(&self, name: &str) -> Result<PathBuf, Fail> {
        Ok(self
            .state
            .join("bases")
            .join(format!("{}.conf", named(name)?)))
    }

    /// Where `install` writes the install bundles that `apply DIR` takes:
    /// `$XDG_STATE_HOME/vtium/drafts/<time>/` (ADR-1201 D2).
    pub fn bundles(&self) -> PathBuf {
        self.state.join("drafts")
    }

    pub fn installed_dir(&self) -> PathBuf {
        self.config.join("installed")
    }

    /// The tool's settings; a file with no line for a key leaves it at its
    /// default, and a line vtium-config cannot read is skipped.
    pub fn settings(&self) -> Result<Settings, Fail> {
        let text = read(&self.settings_path())?.unwrap_or_default();
        let mut settings = Settings {
            draft: None,
            history: HISTORY_DEFAULT,
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "draft" if valid_name(value) => settings.draft = Some(value.to_owned()),
                "history" => {
                    if let Ok(n) = value.parse::<usize>() {
                        settings.history = n;
                    }
                }
                _ => {}
            }
        }
        Ok(settings)
    }

    /// Sets the settings' `draft` line to `name`, or with `None` removes
    /// it, keeping every other line.
    pub fn set_current(&self, name: Option<&str>) -> Result<(), Fail> {
        let name = name.map(named).transpose()?;
        let text = read(&self.settings_path())?.unwrap_or_default();
        let mut lines: Vec<String> = text
            .lines()
            .filter(|l| l.split_once('=').is_none_or(|(k, _)| k.trim() != "draft"))
            .map(str::to_owned)
            .collect();
        if let Some(name) = name {
            lines.insert(0, format!("draft = {name}"));
        }
        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        write(&self.settings_path(), &out)
    }

    /// The drafts' names, in order.
    pub fn drafts(&self) -> Result<Vec<String>, Fail> {
        let dir = self.drafts_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(Fail::File(format!("{}: {e}", dir.display()))),
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_str()?.strip_suffix(".conf")?.to_owned();
                valid_name(&name).then_some(name)
            })
            .collect();
        names.sort();
        Ok(names)
    }

    /// `wanted` when no draft has that name, else `wanted-2`, `wanted-3`
    /// and so on, the first that is free, so no draft is overwritten.
    pub fn free_name(&self, wanted: &str) -> Result<String, Fail> {
        let taken = self.drafts()?;
        let mut name = wanted.to_owned();
        let mut n = 1u32;
        while taken.contains(&name) {
            n = n.saturating_add(1);
            name = format!("{wanted}-{n}");
        }
        Ok(name)
    }

    /// The draft `name`'s text, `None` when it does not exist.
    pub fn read_draft(&self, name: &str) -> Result<Option<String>, Fail> {
        read(&self.draft_path(name)?)
    }

    pub fn write_draft(&self, name: &str, text: &str) -> Result<(), Fail> {
        write(&self.draft_path(name)?, text)
    }

    /// Removes the draft `name`, its changes to the active resources, its
    /// font set folder and its base, the base's text with it.
    pub fn remove_draft(&self, name: &str) -> Result<(), Fail> {
        remove(&self.draft_path(name)?)?;
        remove(&self.batch_path(name)?)?;
        self.drop_frits(name)?;
        remove(&self.base_text_path(name)?)?;
        remove(&self.base_path(name)?)
    }

    /// Renames the draft `name` to `new`, with its changes to the active
    /// resources, its font set folder and its base, and keeps it current
    /// when it was. It refuses a name another draft has, since
    /// vtium-config never overwrites a draft, and a draft that does not
    /// exist. The files beside the draft move first and the draft's own
    /// file last, so a draft is never listed under the new name without
    /// them.
    ///
    /// References:
    /// - ADR-1403 D8, "vtium-config's store: drafts and the library": `draft rename` renames a draft with its font set folder and its base.
    pub fn rename_draft(&self, name: &str, new: &str) -> Result<(), Fail> {
        let (from, to) = (self.draft_path(name)?, self.draft_path(new)?);
        if read(&from)?.is_none() {
            return Err(Fail::Usage(format!("there is no draft named {name}")));
        }
        if read(&to)?.is_some() {
            return Err(Fail::Usage(format!(
                "a draft named {new} already exists, and vtium-config never overwrites a draft"
            )));
        }
        for (a, b) in [
            (self.batch_path(name)?, self.batch_path(new)?),
            (self.frits_path(name)?, self.frits_path(new)?),
            (self.base_text_path(name)?, self.base_text_path(new)?),
            (self.base_path(name)?, self.base_path(new)?),
            (from, to),
        ] {
            match std::fs::rename(&a, &b) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Fail::File(format!("{}: {e}", a.display()))),
            }
        }
        if self.settings()?.draft.as_deref() == Some(name) {
            self.set_current(Some(new))?;
        }
        Ok(())
    }

    /// The draft `name`'s base, `None` when it has none recorded.
    pub fn base(&self, name: &str) -> Result<Option<Base>, Fail> {
        Ok(read(&self.base_path(name)?)?.map(|t| {
            let t = t.trim();
            (t != "none").then(|| t.to_owned())
        }))
    }

    /// Records `installed`, the installed file's bytes or `None`, as the
    /// draft `name`'s base: its hash, and its text beside it, so Update can
    /// merge a change made on disk later. The text goes first, so a write
    /// cut short between the two leaves a text whose hash is not the
    /// base's, which [`Store::base_text`] never gives.
    ///
    /// References:
    /// - ADR-1403 D8, "When the installed file changed on disk": the draft keeps the base's text beside its hash, and vtium-config writes both whenever it writes the base.
    pub fn set_base(&self, name: &str, installed: Option<&[u8]>) -> Result<(), Fail> {
        let text_path = self.base_text_path(name)?;
        match installed.map(std::str::from_utf8) {
            Some(Ok(text)) => write(&text_path, text)?,
            // vtium reads only a vtium.conf of UTF-8 text, so a base of any
            // other bytes keeps its hash alone
            Some(Err(_)) | None => remove(&text_path)?,
        }
        let hash = installed.map_or_else(|| "none".to_owned(), hash);
        write(&self.base_path(name)?, &format!("{hash}\n"))
    }

    /// The text of the draft `name`'s base, when the store keeps it and its
    /// hash is the recorded base's: `None` for a base of no installed file,
    /// for a draft made before vtium-config kept the text, and for a text
    /// left by a write cut short.
    pub fn base_text(&self, name: &str) -> Result<Option<String>, Fail> {
        let Some(Some(base)) = self.base(name)? else {
            return Ok(None);
        };
        Ok(read(&self.base_text_path(name)?)?.filter(|t| hash(t.as_bytes()) == base))
    }

    /// The installed history, newest first: each file `DRAFT-N.conf` with
    /// the time it was replaced, its file's modification time.
    pub fn history(&self) -> Result<Vec<Installed>, Fail> {
        let dir = self.installed_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(Fail::File(format!("{}: {e}", dir.display()))),
        };
        let mut out: Vec<Installed> = entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                let file = e.file_name().to_str()?.strip_suffix(".conf")?.to_owned();
                let (draft, number) = file.rsplit_once('-')?;
                let number = number.parse::<u32>().ok().filter(|n| *n > 0)?;
                let when = e
                    .metadata()
                    .ok()?
                    .modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                valid_name(draft).then(|| Installed {
                    draft: draft.to_owned(),
                    number,
                    path: e.path(),
                    when,
                })
            })
            .collect();
        // newest first; one draft's later number is newer at the same time
        out.sort_by(|a, b| {
            b.when
                .cmp(&a.when)
                .then_with(|| a.draft.cmp(&b.draft))
                .then_with(|| b.number.cmp(&a.number))
        });
        Ok(out)
    }

    /// Saves `replaced`, the installed file an install of the draft `draft`
    /// replaced, as the draft's next `DRAFT-N.conf`, then removes the
    /// oldest files past `keep`. Returns the saved file's path.
    pub fn save_history(&self, draft: &str, replaced: &str, keep: usize) -> Result<PathBuf, Fail> {
        let history = self.history()?;
        let number = history
            .iter()
            .filter(|h| h.draft == draft)
            .map(|h| h.number)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let path = self
            .installed_dir()
            .join(format!("{}-{number}.conf", named(draft)?));
        write(&path, replaced)?;
        for old in self.history()?.iter().skip(keep) {
            remove(&old.path)?;
        }
        Ok(path)
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test fails by panicking")]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    /// Root's store is under the home the password database gives root,
    /// never under the `HOME` or the XDG directories `sudo` carried over
    /// from another user.
    #[test]
    fn root_s_store_comes_from_the_password_database() {
        let carried = [
            ("HOME", "/home/alice"),
            ("XDG_CONFIG_HOME", "/home/alice/.config"),
            ("XDG_STATE_HOME", "/home/alice/.local/state"),
        ];
        let store = Store::locate(0, env(&carried), |uid| {
            (uid == 0).then(|| PathBuf::from("/root"))
        })
        .unwrap();
        assert_eq!(
            store,
            Store {
                config: PathBuf::from("/root/.config/vtium"),
                state: PathBuf::from("/root/.local/state/vtium"),
            }
        );
        let none = Store::locate(0, env(&carried), |_| None).unwrap_err();
        assert!(none.contains("password database"), "{none}");
    }

    #[test]
    fn a_user_s_store_follows_the_xdg_directories() {
        let store = Store::locate(
            1000,
            env(&[("HOME", "/home/bo"), ("XDG_STATE_HOME", "/s")]),
            |_| Some(PathBuf::from("/root")),
        )
        .unwrap();
        assert_eq!(
            store,
            Store {
                config: PathBuf::from("/home/bo/.config/vtium"),
                state: PathBuf::from("/s/vtium"),
            }
        );
        assert!(Store::locate(1000, env(&[]), |_| None).is_err());
    }

    /// Under cargo, the real user's store is refused, whichever variable
    /// leads there; a store of a test's own, or a run outside cargo, is
    /// taken.
    #[test]
    fn under_cargo_the_real_user_s_store_is_refused() {
        let home = Path::new("/home/bo");
        let real = Store::locate(1000, env(&[("HOME", "/home/bo")]), |_| None).unwrap();
        let why = real.guard(true, Some(home)).unwrap_err();
        assert!(why.contains("XDG_CONFIG_HOME"), "{why}");
        let state_only = Store::locate(
            1000,
            env(&[
                ("HOME", "/tmp/t"),
                ("XDG_STATE_HOME", "/home/bo/.local/state"),
            ]),
            |_| None,
        )
        .unwrap();
        assert!(state_only.guard(true, Some(home)).is_err());
        assert_eq!(real.guard(false, Some(home)), Ok(()));
        let own = Store::locate(1000, env(&[("HOME", "/home/bo/target/tmp/t")]), |_| None).unwrap();
        assert_eq!(own.guard(true, Some(home)), Ok(()));
    }

    /// The release binary never refuses a user, whatever its environment
    /// holds; a debug build under cargo does.
    #[test]
    fn only_a_debug_build_under_cargo_arms_the_guard() {
        let cargo = env(&[("CARGO_MANIFEST_DIR", "/src/vtium")]);
        assert!(!armed(false, &cargo));
        assert!(armed(true, &cargo));
        assert!(!armed(true, env(&[])));
    }

    /// A name that would reach outside the store is refused wherever a
    /// path is built from it.
    #[test]
    fn a_draft_s_name_never_leaves_the_store() {
        let store = Store {
            config: PathBuf::from("/c"),
            state: PathBuf::from("/s"),
        };
        for bad in [
            "../library",
            "a/b",
            "..",
            ".",
            ".hidden",
            "-x",
            "a\0b",
            "",
            &"x".repeat(65),
        ] {
            assert_eq!(
                store.draft_path(bad).map_err(|f| f.code()),
                Err(2),
                "{bad:?}"
            );
            assert_eq!(
                store.base_path(bad).map_err(|f| f.code()),
                Err(2),
                "{bad:?}"
            );
            assert_eq!(
                store.batch_path(bad).map_err(|f| f.code()),
                Err(2),
                "{bad:?}"
            );
            assert_eq!(
                store.edit_copy(bad).map_err(|f| f.code()),
                Err(2),
                "{bad:?}"
            );
            assert_eq!(
                store.save_history(bad, "x", 10).map_err(|f| f.code()),
                Err(2),
                "{bad:?}"
            );
            assert_eq!(
                store.set_current(Some(bad)).map_err(|f| f.code()),
                Err(2),
                "{bad:?}"
            );
        }
        assert_eq!(
            store.draft_path("desk").unwrap(),
            PathBuf::from("/c/drafts/desk.conf")
        );
    }

    #[test]
    fn a_hostname_becomes_a_draft_s_name() {
        assert_eq!(name_from("workstation\n"), "workstation");
        assert_eq!(name_from("my box.lan"), "my-box.lan");
        assert_eq!(name_from(""), "draft");
        assert_eq!(name_from("..."), "draft");
        assert!(!valid_name("../x"));
        assert!(!valid_name("-x"));
    }

    /// A store in a scratch directory of the test's own.
    fn scratch(name: &str) -> (crate::scratch::Scratch, Store) {
        let dir = crate::scratch::Scratch::new(name);
        let store = Store {
            config: dir.join("config"),
            state: dir.join("state"),
        };
        (dir, store)
    }

    #[test]
    fn a_base_keeps_its_text_beside_its_hash() {
        let (_dir, store) = scratch("store-base-text");
        assert_eq!(store.base_text("desk").unwrap(), None);
        store.set_base("desk", Some(b"version = 1\n")).unwrap();
        assert_eq!(
            store.base("desk").unwrap(),
            Some(Some(hash(b"version = 1\n")))
        );
        assert_eq!(
            store.base_text("desk").unwrap().as_deref(),
            Some("version = 1\n")
        );
        // a text whose hash is not the base's is never given, as after a
        // write cut short between the two files
        write(&store.base_text_path("desk").unwrap(), "other\n").unwrap();
        assert_eq!(store.base_text("desk").unwrap(), None);
        // a base of no installed file keeps no text
        store.set_base("desk", Some(b"a\n")).unwrap();
        store.set_base("desk", None).unwrap();
        assert_eq!(store.base("desk").unwrap(), Some(None));
        assert!(!store.base_text_path("desk").unwrap().exists());
        // and removing the draft removes the text
        store.set_base("desk", Some(b"a\n")).unwrap();
        store.remove_draft("desk").unwrap();
        assert!(!store.base_text_path("desk").unwrap().exists());
    }

    #[test]
    fn a_draft_is_renamed_with_everything_beside_it() {
        let (_dir, store) = scratch("store-rename");
        store.write_draft("desk", "version = 1\n").unwrap();
        store
            .keep_frit("desk", "mine", "source=builtin:unifont\n")
            .unwrap();
        let mut changes = Changes::default();
        changes.set(
            PathBuf::from("/usr/share/fonts/Terminus.otb"),
            Some(crate::batch::Change::Activate),
        );
        store.set_batch_changes("desk", &changes).unwrap();
        store.set_base("desk", Some(b"version = 1\n")).unwrap();
        store.set_current(Some("desk")).unwrap();
        store.write_draft("spare", "").unwrap();
        // a name another draft has is refused, and nothing moves
        assert_eq!(
            store.rename_draft("desk", "spare").map_err(|f| f.code()),
            Err(2)
        );
        assert_eq!(
            store.rename_draft("nosuch", "other").map_err(|f| f.code()),
            Err(2)
        );
        assert_eq!(store.drafts().unwrap(), ["desk", "spare"]);
        store.rename_draft("desk", "laptop").unwrap();
        assert_eq!(store.drafts().unwrap(), ["laptop", "spare"]);
        assert_eq!(
            store.read_draft("laptop").unwrap().as_deref(),
            Some("version = 1\n")
        );
        assert!(
            store
                .frit_changes("laptop")
                .unwrap()
                .kept
                .contains_key("mine")
        );
        assert_eq!(store.batch_changes("laptop").unwrap(), changes);
        assert_eq!(
            store.base_text("laptop").unwrap().as_deref(),
            Some("version = 1\n")
        );
        assert_eq!(store.settings().unwrap().draft.as_deref(), Some("laptop"));
        for gone in [
            store.batch_path("desk").unwrap(),
            store.frits_path("desk").unwrap(),
            store.base_path("desk").unwrap(),
            store.base_text_path("desk").unwrap(),
        ] {
            assert!(!gone.exists(), "{}", gone.display());
        }
        // renaming a draft that is not current leaves the current one
        store.rename_draft("spare", "kiosk").unwrap();
        assert_eq!(store.settings().unwrap().draft.as_deref(), Some("laptop"));
    }
}
