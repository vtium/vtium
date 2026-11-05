//! The draft and the installed file (ADR-1403 D8): the current draft,
//! made from the installed file when there is none, how it stands against
//! the installed file, the check every change runs, and `install`, which
//! hands the draft to `apply DIR` and keeps the file it replaced in the
//! installed history.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::draft::{FileEdit, Plan, changes};
use crate::outcome::Fail;
use crate::store::{Store, hash};
use crate::tui::Paths;

/// The shipped example vtium.conf: the key table's commented lines and the
/// compiled-in flow's sections. A draft starts from it when no vtium.conf
/// is installed.
pub const EXAMPLE: &str = include_str!("../../vtium/install/vtium.conf.example");

/// How a draft stands against the installed file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// The draft is the installed file.
    Same,
    /// The draft differs by these lines.
    Changed { added: usize, removed: usize },
    /// The installed file is no longer the one the draft was made from.
    Stale { added: usize, removed: usize },
}

impl State {
    /// The words the status line and `flow`'s first line say.
    pub fn words(&self) -> String {
        match *self {
            State::Same => "same as installed".to_owned(),
            State::Changed { added, removed } => lines(added, removed),
            State::Stale { .. } => "made before the installed file changed".to_owned(),
        }
    }

    /// The state's name in `flow --json`.
    pub const fn kind(&self) -> &'static str {
        match self {
            State::Same => "same",
            State::Changed { .. } => "changed",
            State::Stale { .. } => "stale",
        }
    }

    /// The lines added and removed against the installed file.
    pub const fn counts(&self) -> (usize, usize) {
        match *self {
            State::Same => (0, 0),
            State::Changed { added, removed } | State::Stale { added, removed } => (added, removed),
        }
    }
}

/// `3 lines added, 1 removed`, as the confirmations and the state say it.
pub fn lines(added: usize, removed: usize) -> String {
    let n = |n: usize| {
        if n == 1 {
            "1 line".to_owned()
        } else {
            format!("{n} lines")
        }
    };
    match (added, removed) {
        (0, 0) => "no lines changed".to_owned(),
        (a, 0) => format!("{} added", n(a)),
        (0, r) => format!("{} removed", n(r)),
        (a, r) => format!("{} added, {r} removed", n(a)),
    }
}

/// The lines added to and removed from `old` to make `new`.
pub fn counts(old: &str, new: &str) -> (usize, usize) {
    changes(old, new).iter().fold((0, 0), |(a, r), c| {
        (
            a + usize::from(c.new.is_some()),
            r + usize::from(c.old.is_some()),
        )
    })
}

/// The difference from `old` to `new` as `diff` and `--dry-run` print it:
/// a heading naming each file, then each changed line under the number of
/// its line in `new`, `-` before the line taken out and `+` before the
/// line put in.
pub fn diff(old_name: &str, old: &str, new_name: &str, new: &str) -> String {
    let mut out = format!("--- {old_name}\n+++ {new_name}\n");
    for c in changes(old, new) {
        out.push_str(&format!("@@ line {}\n", c.line));
        if let Some(o) = &c.old {
            out.push_str(&format!("-{o}\n"));
        }
        if let Some(n) = &c.new {
            out.push_str(&format!("+{n}\n"));
        }
    }
    out
}

/// What a command works on: the store, where the installed file is, and
/// the programs the check and the install run.
pub struct Session<'a> {
    pub store: Store,
    pub paths: &'a Paths,
}

/// What `vtium check-config` found in a draft.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Checked {
    pub passed: bool,
    /// What it counts against the file, its standard error, each line with
    /// the draft's path.
    pub problems: Vec<String>,
    /// What it notes outside the status, its standard output.
    pub notes: Vec<String>,
}

impl Checked {
    /// How many problems the status line counts: each problem line, not
    /// the lines that say what an error drops.
    pub fn count(&self) -> usize {
        self.problems
            .iter()
            .filter(|l| !l.contains(": For this error"))
            .count()
    }
}

impl Session<'_> {
    /// The installed vtium.conf's path.
    pub fn installed_path(&self) -> PathBuf {
        self.paths.configdir.join("vtium.conf")
    }

    /// The installed vtium.conf's text, `None` when none is installed.
    pub fn installed(&self) -> Result<Option<String>, Fail> {
        crate::tui::read_conf(&self.installed_path()).map_err(Fail::File)
    }

    /// The current draft's name and text. When the settings name no draft,
    /// or one that is gone, vtium-config makes one from the installed file,
    /// or from the shipped example when none is installed, named for the
    /// machine's hostname with an increment so nothing is overwritten, and
    /// records its base.
    pub fn current(&self) -> Result<(String, String), Fail> {
        let settings = self.store.settings()?;
        if let Some(name) = settings.draft
            && let Some(text) = self.store.read_draft(&name)?
        {
            self.seen(&name)?;
            return Ok((name, text));
        }
        let name = self.store.free_name(&crate::store::hostname())?;
        let text = self.make(&name, From::Installed)?;
        Ok((name, text))
    }

    /// Makes the draft `name` from `from`, records its base and makes it
    /// current; returns its text.
    pub fn make(&self, name: &str, from: From) -> Result<String, Fail> {
        let installed = self.installed()?;
        let text = match from {
            From::Installed => installed.clone().unwrap_or_else(|| EXAMPLE.to_owned()),
            From::Empty => crate::ops::new_text(EXAMPLE),
            From::Draft(other) => self
                .store
                .read_draft(&other)?
                .ok_or_else(|| Fail::Usage(format!("there is no draft named {other}")))?,
        };
        self.store.write_draft(name, &text)?;
        self.store
            .set_base(name, installed.as_deref().map(str::as_bytes))?;
        self.store.set_current(Some(name))?;
        Ok(text)
    }

    /// Records the installed file as the base of the draft `name` when it
    /// has none, the first time vtium-config sees it, as a draft copied in
    /// from another machine: an edit made to the installed file after that
    /// makes the draft stale, and install refuses it.
    pub fn seen(&self, name: &str) -> Result<(), Fail> {
        if self.store.base(name)?.is_none() {
            let installed = self.installed()?;
            self.store
                .set_base(name, installed.as_deref().map(str::as_bytes))?;
        }
        Ok(())
    }

    /// How the draft `name`, `text`, stands against the installed file.
    pub fn state(&self, name: &str, text: &str) -> Result<State, Fail> {
        let installed = self.installed()?;
        let (added, removed) = counts(installed.as_deref().unwrap_or_default(), text);
        if self.stale(name, installed.as_deref())? {
            return Ok(State::Stale { added, removed });
        }
        Ok(if added == 0 && removed == 0 {
            State::Same
        } else {
            State::Changed { added, removed }
        })
    }

    /// Whether the installed file `installed` is no longer the draft
    /// `name`'s base. A draft with no recorded base, such as one copied in
    /// from another machine, takes the installed file as its base.
    fn stale(&self, name: &str, installed: Option<&str>) -> Result<bool, Fail> {
        let now = installed.map(|t| hash(t.as_bytes()));
        Ok(match self.store.base(name)? {
            Some(base) => base != now,
            None => false,
        })
    }

    /// Runs `vtium check-config` on `text`, a copy of the draft at `shown`,
    /// against the configuration directory; each line it prints names
    /// `shown`. A console that cannot be started is exit 5's failure.
    pub fn check(&self, text: &str, shown: &Path) -> Result<Checked, Fail> {
        let copy = crate::apply::Copy::new("vtium.conf", text.as_bytes()).map_err(Fail::File)?;
        let ran = std::process::Command::new(&self.paths.vtium)
            .arg("check-config")
            .arg("--configdir")
            .arg(&self.paths.configdir)
            .arg(&copy.path)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| {
                Fail::File(format!(
                    "{}: {e}; the check could not run",
                    self.paths.vtium.to_string_lossy()
                ))
            })?;
        let copied = copy.path.to_string_lossy().into_owned();
        let shown = shown.to_string_lossy();
        let lines = |bytes: &[u8]| -> Vec<String> {
            String::from_utf8_lossy(bytes)
                .lines()
                .map(|l| l.replace(&copied, &shown))
                .collect()
        };
        Ok(Checked {
            passed: ran.status.success(),
            problems: lines(&ran.stderr),
            notes: lines(&ran.stdout),
        })
    }

    /// Installs the draft `name`, `text`: refuses it when the installed
    /// file changed since its base (exit 3) or the check fails (exit 1),
    /// then hands it to `apply DIR`, through sudo for the system's
    /// directories, or at once as root. After a good install it saves the
    /// file it replaced in the installed history and makes the draft's
    /// base the new installed file.
    pub fn install(&self, name: &str, text: &str, out: &mut dyn Write) -> Result<(), Fail> {
        let installed = self.installed()?;
        if self.stale(name, installed.as_deref())? {
            return Err(Fail::Changed(format!(
                "{} changed since draft {name} was made from it, so nothing is installed. Run vtium config diff to see the difference, or vtium config revert to start again from the installed file.",
                self.installed_path().display()
            )));
        }
        let checked = self.check(text, &self.store.draft_path(name)?)?;
        for line in checked.problems.iter().chain(&checked.notes) {
            let _ = writeln!(out, "{line}");
        }
        if !checked.passed {
            return Err(Fail::Check(format!(
                "the check found problems in draft {name}, so nothing is installed; the draft keeps every change"
            )));
        }
        if installed.as_deref() == Some(text) {
            let _ = writeln!(
                out,
                "draft {name} is the installed file; nothing to install"
            );
            return self.store.set_base(name, Some(text.as_bytes()));
        }
        let plan = Plan {
            files: vec![FileEdit {
                path: self.installed_path(),
                old: installed.clone(),
                new: text.to_owned(),
            }],
            removals: Vec::new(),
        };
        let targets = self.paths.targets();
        if rustix::process::geteuid().is_root() {
            let wrote = crate::apply::install_now(&plan, &targets, out).map_err(Fail::Check)?;
            if !wrote {
                return Err(Fail::File(format!(
                    "{} could not be written",
                    self.installed_path().display()
                )));
            }
        } else {
            let bundle = crate::apply::write_draft(&self.store.bundles(), &plan, &targets)
                .map_err(|e| Fail::File(format!("{}: {e}", self.store.bundles().display())))?;
            let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("vtium-config"));
            let route = crate::apply::route(&targets, &self.paths.sudo);
            let argv = crate::apply::save_argv(&route, &program, &targets, &bundle);
            run_apply(&argv, matches!(route, crate::apply::Route::Sudo(_)), out)?;
        }
        if let Some(replaced) = &installed {
            let keep = self.store.settings()?.history;
            let saved = self.store.save_history(name, replaced, keep)?;
            let _ = writeln!(out, "kept {}", saved.display());
        }
        self.store.set_base(name, Some(text.as_bytes()))?;
        let _ = writeln!(out, "installed draft {name}");
        Ok(())
    }
}

/// What a new draft is made from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum From {
    /// The installed file, or the shipped example when none is installed.
    Installed,
    /// The top level at its defaults and one empty look, as `new` makes.
    Empty,
    /// Another draft.
    Draft(String),
}

/// Runs `argv`, the `apply DIR` an install takes, printing what it
/// printed; `sudo` says whether it runs through sudo, whose own refusal is
/// exit 4.
fn run_apply(argv: &[OsString], sudo: bool, out: &mut dyn Write) -> Result<(), Fail> {
    let Some((program, args)) = argv.split_first() else {
        return Err(Fail::Privilege("no command to install with".to_owned()));
    };
    let ran = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .output();
    let ran = match ran {
        Ok(ran) => ran,
        Err(e) if sudo => {
            return Err(Fail::Privilege(format!(
                "{}: {e}. Install the draft as root with vtium config install, or install sudo.",
                program.to_string_lossy()
            )));
        }
        Err(e) => {
            return Err(Fail::File(format!("{}: {e}", program.to_string_lossy())));
        }
    };
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    let _ = write!(out, "{said}");
    if ran.status.success() {
        return Ok(());
    }
    Err(classify(&said, sudo))
}

/// What a failed `apply DIR` means, from what it and sudo printed: sudo's
/// own refusal, before `apply` ran, is exit 4; an installed file that
/// changed meanwhile is exit 3; a file `apply` could not write is exit 5;
/// anything else `apply` refused is the check's, exit 1.
pub fn classify(said: &str, sudo: bool) -> Fail {
    let apply_ran = said.lines().any(|l| {
        l.starts_with("vtium-config:") || l.starts_with("wrote") || l.starts_with("failed")
    });
    if sudo && !apply_ran {
        return Fail::Privilege(
            "sudo refused to run vtium-config apply, so nothing is installed; the draft keeps every change. Run vtium config install again, or install it as root.".to_owned(),
        );
    }
    if said.contains("changed since") {
        return Fail::Changed(
            "the installed vtium.conf changed while the draft was being installed, so nothing is installed".to_owned(),
        );
    }
    if said.lines().any(|l| l.starts_with("failed")) {
        return Fail::File("apply could not write every file; it names each above".to_owned());
    }
    Fail::Check("apply refused the draft; the draft keeps every change".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_says_lines_added_and_removed() {
        assert_eq!(
            State::Changed {
                added: 3,
                removed: 1
            }
            .words(),
            "3 lines added, 1 removed"
        );
        assert_eq!(
            State::Changed {
                added: 1,
                removed: 0
            }
            .words(),
            "1 line added"
        );
        assert_eq!(
            State::Changed {
                added: 0,
                removed: 2
            }
            .words(),
            "2 lines removed"
        );
        assert_eq!(counts("a\nb\n", "a\nB\nc\n"), (2, 1));
    }

    #[test]
    fn sudo_s_refusal_is_exit_4_and_apply_s_is_the_check_s() {
        assert_eq!(
            classify("sudo: 3 incorrect password attempts\n", true).code(),
            4
        );
        assert_eq!(classify("vtium-config: vtium.conf line 3: the console's check refused this line\nNothing was written.\n", true).code(), 1);
        assert_eq!(
            classify(
                "vtium-config: /etc/vtium/vtium.conf changed since the draft was made\n",
                false
            )
            .code(),
            3
        );
        assert_eq!(
            classify("failed /etc/vtium/vtium.conf: No space left\n", true).code(),
            5
        );
    }

    #[test]
    fn a_diff_names_each_changed_line() {
        assert_eq!(
            diff("a", "x\ny\n", "b", "x\nz\n"),
            "--- a\n+++ b\n@@ line 2\n-y\n+z\n"
        );
    }
}
