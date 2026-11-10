//! The draft and the installed file (ADR-1403 D8): the current draft,
//! made from the installed file when there is none, how it stands against
//! the installed file, the check every change runs, and `install`, which
//! hands the draft to `apply DIR` and keeps the file it replaced in the
//! installed history.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::apply::Runner;
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
    /// the lines that say what an error drops. Those end with one of
    /// vtium's own words for what an error drops.
    pub fn count(&self) -> usize {
        counted(&self.problems).count()
    }

    /// Whether the check failed only because the draft at `shown` wires no
    /// complete route (ADR-1403 D3). That is the one problem Install lets
    /// the person accept (D8).
    pub fn no_route_only(&self, shown: &str) -> bool {
        !self.passed && only_no_route(&self.problems, shown)
    }
}

/// The problem lines the status line counts: every line but those that say
/// what an error drops.
fn counted<S: AsRef<str>>(lines: &[S]) -> impl Iterator<Item = &str> {
    lines.iter().map(AsRef::as_ref).filter(|l| {
        !vtium_settings::Drops::ALL
            .iter()
            .any(|d| l.ends_with(&d.to_string()))
    })
}

/// Whether the one problem among `lines`, which `vtium check-config` wrote
/// of the file at `shown`, is that the file wires no complete route. That
/// line names the file and no line of it (ADR-1403 D3). Any other problem,
/// or a second one, makes this false.
pub fn only_no_route<S: AsRef<str>>(lines: &[S], shown: &str) -> bool {
    let mut problems = counted(lines);
    let (Some(line), None) = (problems.next(), problems.next()) else {
        return false;
    };
    line.strip_prefix(shown)
        .and_then(|rest| rest.strip_prefix(": "))
        .is_some_and(says_no_route)
}

/// Whether `message` is one of vtium's own messages for a configuration
/// that wires no complete route, whatever display, head or rule number it
/// names. Each message's shape is read from vtium-settings, so the two
/// never drift apart.
pub fn says_no_route(message: &str) -> bool {
    use vtium_settings::{Missing, Quoted};
    const NUMBER: usize = 918_273_645;
    let hole = Quoted::path(Path::new("\u{1}"));
    let shown_hole = hole.to_string();
    let shapes = [
        Missing::NoDisplay {
            display: hole.clone(),
            rule: Some(NUMBER),
        },
        Missing::NoDisplay {
            display: hole.clone(),
            rule: None,
        },
        Missing::NoKeyboard {
            head: hole.clone(),
            display: hole.clone(),
        },
        Missing::Isolated { head: hole.clone() },
        Missing::NoDisplayRule,
        Missing::NoKeyboardRule {
            head: hole,
            rule: NUMBER,
        },
    ];
    shapes.iter().any(|shape| {
        let shape = shape.to_string().replace(&NUMBER.to_string(), &shown_hole);
        fits(
            message,
            &shape.split(shown_hole.as_str()).collect::<Vec<_>>(),
        )
    })
}

/// Whether `message` is `words` with some text, at least one character,
/// in each gap between them.
fn fits(message: &str, words: &[&str]) -> bool {
    if let [whole] = words {
        return message == *whole;
    }
    let [first, middle @ .., last] = words else {
        return false;
    };
    let Some(mut rest) = message.strip_prefix(first) else {
        return false;
    };
    for word in middle {
        let found = rest
            .char_indices()
            .skip(1)
            .find_map(|(i, _)| rest.get(i..)?.strip_prefix(word));
        let Some(after) = found else {
            return false;
        };
        rest = after;
    }
    rest.len() > last.len() && rest.ends_with(last)
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
                    "the check could not run: {}: {e}",
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

    /// Readies the draft `name`, `text`, for an install: refuses it when
    /// the installed file changed since its base (exit 3) or the check
    /// fails (exit 1), printing what the check found. A draft whose only
    /// problem is that it wires no complete route is readied with the
    /// plan's `no_route` set, and the caller asks the person, or takes
    /// `--force`, before it installs it (ADR-1403 D8). `None` when the
    /// draft is the installed file already, which then becomes its base.
    pub fn prepare(
        &self,
        name: &str,
        text: &str,
        out: &mut dyn Write,
    ) -> Result<Option<Ready>, Fail> {
        let installed = self.installed()?;
        if self.stale(name, installed.as_deref())? {
            return Err(Fail::Changed(format!(
                "Nothing installed: {} changed after draft {name} was made from it. Run vtium config diff to see the difference, or vtium config revert to start again from the installed file.",
                self.installed_path().display()
            )));
        }
        let shown = self.store.draft_path(name)?;
        let checked = self.check(text, &shown)?;
        for line in checked.problems.iter().chain(&checked.notes) {
            let _ = writeln!(out, "{line}");
        }
        let no_route = checked.no_route_only(&shown.to_string_lossy());
        if !checked.passed && !no_route {
            return Err(Fail::Check(format!(
                "nothing installed: the check found problems in draft {name}. The draft keeps your changes."
            )));
        }
        if installed.as_deref() == Some(text) {
            let _ = writeln!(
                out,
                "Nothing to install: draft {name} is the same as the installed vtium.conf."
            );
            self.store.set_base(name, Some(text.as_bytes()))?;
            return Ok(None);
        }
        Ok(Some(Ready {
            plan: Plan {
                files: vec![FileEdit {
                    path: self.installed_path(),
                    old: installed.clone(),
                    new: text.to_owned(),
                }],
                removals: Vec::new(),
                no_route,
            },
            replaced: installed,
        }))
    }

    /// Hands `plan` to the privileged step, `apply DIR`: at once as root,
    /// else through `run`, which runs sudo for the system's directories.
    /// sudo's own refusal is exit 4; `apply`'s is [`classify`]'s.
    pub fn put(&self, plan: &Plan, run: Runner<'_>, out: &mut dyn Write) -> Result<(), Fail> {
        let targets = self.paths.targets();
        if rustix::process::geteuid().is_root() {
            let wrote = crate::apply::install_now(plan, &targets, out).map_err(Fail::Check)?;
            if !wrote {
                return Err(Fail::File(format!(
                    "{} could not be written",
                    self.installed_path().display()
                )));
            }
            return Ok(());
        }
        let bundle = crate::apply::write_draft(&self.store.bundles(), plan, &targets)
            .map_err(|e| Fail::File(format!("{}: {e}", self.store.bundles().display())))?;
        let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("vtium-config"));
        let route = crate::apply::route(&targets, &self.paths.sudo);
        let sudo = matches!(route, crate::apply::Route::Sudo(_));
        let argv = crate::apply::save_argv(&route, &program, &targets, &bundle);
        let shown = argv
            .first()
            .map(|a| a.to_string_lossy().into_owned())
            .unwrap_or_default();
        match run(&argv) {
            Ok((ok, said)) => {
                let _ = write!(out, "{said}");
                if ok {
                    Ok(())
                } else {
                    Err(classify(&said, sudo))
                }
            }
            Err(e) if sudo => Err(Fail::Privilege(format!(
                "cannot run {shown}: {e}. Run vtium config install as root, or install sudo."
            ))),
            Err(e) => Err(Fail::File(format!("{shown}: {e}"))),
        }
    }

    /// After the privileged step installed the draft `name`, `text`:
    /// keeps `replaced`, the file it replaced, in the installed history,
    /// and makes the draft's base the new installed file.
    pub fn finish(
        &self,
        name: &str,
        text: &str,
        replaced: Option<&str>,
        out: &mut dyn Write,
    ) -> Result<(), Fail> {
        if let Some(replaced) = replaced {
            let keep = self.store.settings()?.history;
            let saved = self.store.save_history(name, replaced, keep)?;
            let _ = writeln!(out, "kept {}", saved.display());
        }
        self.store.set_base(name, Some(text.as_bytes()))?;
        let _ = writeln!(out, "installed draft {name}");
        Ok(())
    }

    /// Installs the draft `name`, `text`, as `vtium config install` does:
    /// [`Session::prepare`], then for a draft that wires no complete route
    /// what `unrouted` says, then [`Session::put`] with sudo run as a plain
    /// child, then [`Session::finish`].
    pub fn install(
        &self,
        name: &str,
        text: &str,
        unrouted: Unrouted<'_>,
        out: &mut dyn Write,
    ) -> Result<(), Fail> {
        let Some(ready) = self.prepare(name, text, out)? else {
            return Ok(());
        };
        if ready.plan.no_route {
            match unrouted {
                Unrouted::Install => {}
                Unrouted::Refuse => {
                    return Err(Fail::Check(crate::tui::words::no_route_force()));
                }
                Unrouted::Ask(ask) => {
                    let (added, removed) =
                        counts(ready.replaced.as_deref().unwrap_or_default(), text);
                    if !ask(&crate::tui::words::no_route_question(added, removed, 0)) {
                        return Err(Fail::Check(
                            crate::tui::words::NO_ROUTE_NOT_INSTALLED.to_owned(),
                        ));
                    }
                }
            }
        }
        self.put(&ready.plan, &mut run_child, out)?;
        self.finish(name, text, ready.replaced.as_deref(), out)
    }
}

/// What an install does with a draft whose only problem is that it wires
/// no complete route (ADR-1403 D8).
pub enum Unrouted<'a> {
    /// Installs it: `vtium config install --force`.
    Install,
    /// Asks the question, and installs it only when the answer is yes:
    /// `vtium config install` at a terminal.
    Ask(&'a mut dyn FnMut(&str) -> bool),
    /// Refuses it, exit 1, and says how to install it anyway: `install`
    /// with no terminal, and every command's `--install`, which installs
    /// only a draft the check passes.
    Refuse,
}

/// A draft the check passed, or whose only problem is that it wires no
/// complete route, ready for the privileged step: the plan it installs and
/// the installed file it replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ready {
    pub plan: Plan,
    pub replaced: Option<String>,
}

/// Runs `argv` as a plain child, its standard input the terminal's, so
/// sudo can ask for the password: whether it succeeded, and what it
/// printed.
pub fn run_child(argv: &[OsString]) -> std::io::Result<(bool, String)> {
    let Some((program, args)) = argv.split_first() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "no command to install with",
        ));
    };
    let ran = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .output()?;
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    Ok((ran.status.success(), said))
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
            "Nothing installed: sudo refused to run vtium-config apply. The draft keeps your changes. Run vtium config install again, or run it as root.".to_owned(),
        );
    }
    if said.contains(crate::apply::CHANGED) {
        return Fail::Changed(
            "nothing installed: the installed vtium.conf changed during the install".to_owned(),
        );
    }
    if said.lines().any(|l| l.starts_with("failed")) {
        return Fail::File(
            "apply could not write every file; the lines above name each one".to_owned(),
        );
    }
    Fail::Check("apply refused the draft; the draft keeps your changes".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR-1403 D8: Install lets the person accept one problem, a missing
    // complete route, and only when it is the check's one problem
    #[test]
    fn only_a_missing_complete_route_is_the_problem_install_may_accept() {
        use vtium_settings::{Missing, Quoted};
        let q = |s: &str| Quoted::path(Path::new(s));
        let each = [
            Missing::NoDisplay {
                display: q("DP-8"),
                rule: Some(12),
            },
            Missing::NoDisplay {
                display: q("HDMI-A-1"),
                rule: None,
            },
            Missing::NoKeyboard {
                head: q("desk"),
                display: q("DP-8"),
            },
            Missing::Isolated { head: q("kiosk") },
            Missing::NoDisplayRule,
            Missing::NoKeyboardRule {
                head: q("desk"),
                rule: 3,
            },
        ];
        for missing in &each {
            let line = format!("/d/desk.conf: {missing}");
            assert!(only_no_route(&[&line], "/d/desk.conf"), "{line}");
            // a second problem, a line of the file, or another file
            let bad = "/d/desk.conf:4: BAD is not a key vtium reads".to_owned();
            assert!(!only_no_route(&[&line, &bad], "/d/desk.conf"), "{line}");
            let numbered = format!("/d/desk.conf:4: {missing}");
            assert!(!only_no_route(&[&numbered], "/d/desk.conf"), "{line}");
            assert!(!only_no_route(&[&line], "/d/other.conf"), "{line}");
        }
        assert!(!only_no_route::<&str>(&[], "/d/desk.conf"));
        assert!(!says_no_route("No display rule sends a display to a head."));
        assert!(!says_no_route("Head is isolated"));
        assert!(!says_no_route("BAD is not a key vtium reads"));
    }

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
        assert_eq!(classify("vtium-config: vtium.conf line 3: vtium check-config refused this line\nNothing was installed.\n", true).code(), 1);
        assert_eq!(
            classify(
                &format!(
                    "vtium-config: /etc/vtium/vtium.conf {}\n",
                    crate::apply::CHANGED
                ),
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
