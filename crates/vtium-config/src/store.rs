//! vtium-config's store (ADR-1403 D8): the drafts, the library, the tool's
//! own settings, the installed history and each draft's base, under the
//! user's XDG directories.
//!
//! | Path | What it holds |
//! |---|---|
//! | `$XDG_CONFIG_HOME/vtium/drafts/NAME.conf` | each draft, a whole vtium.conf |
//! | `$XDG_CONFIG_HOME/vtium/library.conf` | the library, in vtium.conf's section syntax |
//! | `$XDG_CONFIG_HOME/vtium/vtium-config.conf` | `draft = NAME`, the current draft, and `history = N` |
//! | `$XDG_CONFIG_HOME/vtium/installed/DRAFT-N.conf` | each installed file an install replaced |
//! | `$XDG_STATE_HOME/vtium/bases/NAME.base` | each draft's base: the SHA-256 of the installed file it was made from or installed as, or `none` |
//!
//! Run as root, vtium-config finds root's home from the password database
//! and never from `$HOME` or the XDG variables, which `sudo` can carry
//! over from the user who ran it, so a root draft lands only in root's own
//! home ([`Store::locate`]).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use vtium_font::{Sha256, sha256_hex};

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
            "{name:?} cannot name a draft: 1 to 64 letters, digits, '.', '_', '+' and '-', not starting with '.' or '-'"
        )))
    }
}

/// `name` made into a draft's name: each character a name cannot hold
/// becomes `-`, and a name left empty is [`NAME_FALLBACK`].
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
    crate::tui::read_conf(path).map_err(Fail::File)
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
    let new = dir.join(format!(".{name}.new-{}", std::process::id()));
    std::fs::write(&new, text).map_err(fail)?;
    std::fs::rename(&new, path).map_err(|e| {
        let _ = std::fs::remove_file(&new);
        fail(e)
    })
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
                "the password database names no home for root, so vtium-config has nowhere to keep root's draft".to_owned()
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
                .ok_or_else(|| format!("neither {xdg} nor HOME is set, so vtium-config has nowhere to keep its drafts"))
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
    /// name is built here or in [`Store::base_path`],
    /// [`Store::edit_copy`] and [`Store::save_history`], each through
    /// [`named`], so no name can reach outside the store.
    pub fn draft_path(&self, name: &str) -> Result<PathBuf, Fail> {
        Ok(self.drafts_dir().join(format!("{}.conf", named(name)?)))
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

    /// Removes the draft `name` and its base.
    pub fn remove_draft(&self, name: &str) -> Result<(), Fail> {
        remove(&self.draft_path(name)?)?;
        remove(&self.base_path(name)?)
    }

    /// The draft `name`'s base, `None` when it has none recorded.
    pub fn base(&self, name: &str) -> Result<Option<Base>, Fail> {
        Ok(read(&self.base_path(name)?)?.map(|t| {
            let t = t.trim();
            (t != "none").then(|| t.to_owned())
        }))
    }

    /// Records `installed`, the installed file's bytes or `None`, as the
    /// draft `name`'s base.
    pub fn set_base(&self, name: &str, installed: Option<&[u8]>) -> Result<(), Fail> {
        let text = installed.map_or_else(|| "none".to_owned(), hash);
        write(&self.base_path(name)?, &format!("{text}\n"))
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
}
