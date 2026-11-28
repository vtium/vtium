//! The draft and the installed file (ADR-1403 D8): the current draft,
//! made from the installed file when there is none, how it stands against
//! the installed file, the check every change runs, and `install`, which
//! hands the draft and its changes to the active resources to `apply DIR`
//! and keeps the file it replaced in the installed history.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::apply::{Ask, Runner};
use crate::draft::{FileEdit, Plan, changes};
use crate::outcome::Fail;
use crate::places::Paths;
use crate::store::{Store, hash};

/// The shipped example vtium.conf: the key table's commented lines and the
/// compiled-in flow's sections. vtium installs it as
/// `/etc/vtium/vtium.conf.example`, and vtium-config copies none of it into
/// a draft.
pub const EXAMPLE: &str = include_str!("../../vtium/install/vtium.conf.example");

/// The comment that opens a vtium.conf vtium-config writes from nothing:
/// what the file is, and where every setting is described. A draft made
/// when no vtium.conf is installed is this comment alone, and every change
/// then adds only the lines it sets. The comment is wrapped at 78 columns,
/// as the shipped example's comments are.
pub const HEADER: &str = "\
# This is vtium's configuration file, /etc/vtium/vtium.conf. It holds only
# the settings you change, and vtium uses its default for every other setting.
#
# /etc/vtium/vtium.conf.example describes every setting and shows its default.
# vtium --help lists every setting with a one-line description.

";

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

/// What `vtium check-config`'s exit status says of a draft.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Verdict {
    /// Exit status 0: the check found no problem.
    Passed,
    /// Exit status [`vtium_settings::CHECK_MISSING_FONTS`]: the only
    /// problems are fonts that a font set needs and this machine lacks.
    MissingFonts,
    /// Any other status, or none, as when the check was killed: the check
    /// found a problem, or could not run.
    #[default]
    Failed,
}

impl Verdict {
    /// The verdict that the exit `status` of `vtium check-config` gives.
    pub fn of(status: std::process::ExitStatus) -> Verdict {
        match status.code() {
            Some(0) => Verdict::Passed,
            Some(code) if code == i32::from(vtium_settings::CHECK_MISSING_FONTS) => {
                Verdict::MissingFonts
            }
            _ => Verdict::Failed,
        }
    }
}

/// What `vtium check-config` found in a draft.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Checked {
    pub verdict: Verdict,
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
        self.verdict == Verdict::Failed && only_no_route(&self.problems, shown)
    }

    /// Whether the check passed, or found only fonts that a font set needs
    /// and this machine lacks, which check-config says by its exit status.
    /// check-config's status is not 0 then, for scripts and CI, but
    /// vtium-config installs and saves such a draft, and shows the
    /// warnings, because vtium draws the set without the font.
    ///
    /// References:
    /// - ADR-1204 D7: check-config names each missing font.
    pub fn clears(&self) -> bool {
        matches!(self.verdict, Verdict::Passed | Verdict::MissingFonts)
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
        crate::places::read_conf(&self.installed_path()).map_err(Fail::File)
    }

    /// The current draft's name and text. When the settings name no draft,
    /// or one that is gone, vtium-config makes one from the installed file,
    /// or from [`HEADER`] alone when none is installed, named for the
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
        let from_draft = match &from {
            From::Draft(other) => Some(other.clone()),
            _ => None,
        };
        // a copy of a draft takes its changes to the active resources
        // along, and any other new draft has none
        let (text, changes) = match from {
            From::Installed => (
                installed.clone().unwrap_or_else(|| HEADER.to_owned()),
                crate::batch::Changes::default(),
            ),
            From::Empty => (
                crate::ops::new_text(HEADER),
                crate::batch::Changes::default(),
            ),
            From::Draft(other) => (
                self.store
                    .read_draft(&other)?
                    .ok_or_else(|| Fail::Usage(format!("there is no draft named {other}")))?,
                self.store.batch_changes(&other)?,
            ),
        };
        self.store.write_draft(name, &text)?;
        self.store.set_batch_changes(name, &changes)?;
        // a copy of a draft takes its font set changes along too
        match &from_draft {
            Some(other) => self.store.copy_frits(other, name)?,
            None => self.store.drop_frits(name)?,
        }
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

    /// The batch file an install writes, which lists the active resources
    /// ([`crate::batch::batch_file`]).
    pub fn batch_file(&self) -> PathBuf {
        crate::batch::batch_file(&self.paths.targets())
    }

    /// The installed batch file, as vtium reads it
    /// ([`crate::batch::Installed::read`]): under the trust that the
    /// console that reads it runs with, root for the system's directories.
    pub fn installed_batch(&self) -> crate::batch::Installed {
        let euid = crate::batch::rule_user(&self.paths.targets(), &self.paths.sudo);
        crate::batch::Installed::read(&self.batch_file(), euid)
    }

    /// The edit an install of the draft `name` makes to the batch file:
    /// the draft's changes to the active resources applied to the
    /// installed batch file, or to the list of a fresh install when vtium
    /// reads no batch file ([`crate::batch::applied`]). A batch file that
    /// vtium refuses is replaced, and none of its lines is kept, since
    /// someone other than root could have written them. `None` when the
    /// draft changes nothing there, so the install leaves the batch file
    /// alone.
    pub fn batch_edit(&self, name: &str) -> Result<Option<FileEdit>, Fail> {
        use crate::batch::Installed;
        let changes = self.store.batch_changes(name)?;
        if changes.is_empty() {
            return Ok(None);
        }
        let path = self.batch_file();
        let installed = self.installed_batch();
        let text = installed.text(&path).map_err(Fail::Check)?;
        let Some(new) = crate::batch::applied(text, &crate::batch::fresh(), &changes) else {
            return Ok(None);
        };
        // the file the install replaces, as apply finds it, for the check
        // that it has not changed since
        let old = match &installed {
            Installed::Missing => None,
            Installed::Read(_) => text.map(str::to_owned),
            Installed::Refused(why) => crate::places::read_conf(&path).map_err(|e| {
                Fail::File(format!(
                    "{why} vtium-config cannot replace the file: {e}. Remove it as root, and the install then writes a new one."
                ))
            })?,
        };
        Ok(Some(FileEdit { path, old, new }))
    }

    /// Takes out of the draft `name` each change to the active resources
    /// that the system now matches, as after an install: what is left is
    /// what an install still has to do.
    pub fn settle_batch(&self, name: &str) -> Result<(), Fail> {
        let mut changes = self.store.batch_changes(name)?;
        if changes.is_empty() {
            return Ok(());
        }
        changes.settle(&self.installed_batch().base(&crate::batch::fresh()));
        self.store.set_batch_changes(name, &changes)
    }

    /// Drops the draft `name`'s changes to the active resources, as
    /// `revert` and `new` do: the draft then installs none.
    pub fn drop_batch(&self, name: &str) -> Result<(), Fail> {
        self.store
            .set_batch_changes(name, &crate::batch::Changes::default())
    }

    /// The draft `name`'s font set changes against the frits installed in
    /// the frits' directory, as [`frit_changes`] counts them.
    pub fn frit_changes(&self, name: &str) -> Result<(Vec<FritChange>, Vec<String>), Fail> {
        let folder = self.store.frit_changes(name)?;
        if folder.kept.is_empty() && folder.removed.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let frits = self
            .paths
            .frits
            .as_ref()
            .map_err(|e| Fail::File(e.clone()))?;
        Ok(frit_changes(name, &folder, frits, |path| {
            std::fs::read(path).ok()
        }))
    }

    /// The problems of the draft `name`'s font set folder: each entry that
    /// is no font set change, and each frit there that the frit reader
    /// refuses, by its path, as the glyph helper reads it. Install refuses
    /// the draft while one is there.
    pub fn frit_problems(&self, name: &str) -> Result<Vec<String>, Fail> {
        let folder = self.store.frit_changes(name)?;
        let dir = self.store.frits_path(name)?;
        let mut problems = folder.problems;
        for (frit, text) in &folder.kept {
            let path = dir.join(format!("{frit}.vtfrit"));
            if let Err(e) = crate::frit::Frit::parse(text.as_bytes(), &path, frit) {
                problems.push(match e.line {
                    0 => format!("{}: {}", path.display(), e.why),
                    n => format!("{}:{n}: {}", path.display(), e.why),
                });
            }
        }
        Ok(problems)
    }

    /// Takes out of the draft `name`'s folder each font set change that the
    /// system now matches, as after an install: what is left is what an
    /// install still has to do.
    pub fn settle_frits(&self, name: &str) -> Result<(), Fail> {
        let folder = self.store.frit_changes(name)?;
        if folder.kept.is_empty() && folder.removed.is_empty() {
            return Ok(());
        }
        let Ok(frits) = &self.paths.frits else {
            return Ok(());
        };
        for (frit, text) in &folder.kept {
            if std::fs::read(frits.join(format!("{frit}.vtfrit")))
                .ok()
                .as_deref()
                == Some(text.as_bytes())
            {
                self.store.forget_frit(name, frit)?;
            }
        }
        for frit in &folder.removed {
            if !frits.join(format!("{frit}.vtfrit")).exists() {
                self.store.forget_frit(name, frit)?;
            }
        }
        Ok(())
    }

    /// How many changes wait beside the draft `name`: to the active
    /// resources, and to its font sets.
    pub fn beside(&self, name: &str) -> Result<(usize, usize), Fail> {
        Ok((
            self.store.batch_changes(name)?.len(),
            self.frit_changes(name)?.0.len(),
        ))
    }

    /// The lines a command prints after it changes the draft `name`, whose
    /// text is now `text` and which `checked` judged: what the check finds,
    /// then the draft's state line.
    pub fn report(&self, name: &str, text: &str, checked: &Checked) -> Result<Vec<String>, Fail> {
        let state = self.state(name, text)?;
        let mut lines: Vec<String> = checked
            .problems
            .iter()
            .chain(&checked.notes)
            .cloned()
            .collect();
        lines.push(crate::flowmodel::state_line(
            name,
            &state,
            self.beside(name)?,
            checked.count(),
        ));
        Ok(lines)
    }

    /// Runs `vtium check-config` on `text`, a copy of the draft `name` at
    /// `shown`, against the configuration directory, with the font set
    /// changes the install makes ([`Session::frit_changes`]); each line it
    /// prints names `shown`. A frit the folder adds or replaces is passed
    /// as its file in the folder, so the check reads the draft's text. A
    /// removal that removes nothing, and a frit whose text is the installed
    /// one's, are no change and pass nothing, so the check's verdict on
    /// them is the verdict on the installed frits. A console that cannot be
    /// started is exit 5's failure.
    ///
    /// References:
    /// - ADR-1403 D8, "A draft's font set changes": one `--with-frit` for each frit the folder adds or replaces, and one `--without-frit` for each frit it removes.
    pub fn check(&self, name: &str, text: &str, shown: &Path) -> Result<Checked, Fail> {
        let copy = crate::apply::Copy::new("vtium.conf", text.as_bytes()).map_err(Fail::File)?;
        let (changes, _) = self.frit_changes(name)?;
        let dir = self.store.frits_path(name)?;
        let mut command = std::process::Command::new(&self.paths.vtium);
        command
            .arg("check-config")
            .arg("--configdir")
            .arg(&self.paths.configdir);
        for change in &changes {
            if change.new.is_some() {
                command
                    .arg("--with-frit")
                    .arg(dir.join(format!("{}.vtfrit", change.name)));
            } else {
                command.arg("--without-frit").arg(&change.name);
            }
        }
        let ran = command
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
            verdict: Verdict::of(ran.status),
            problems: lines(&ran.stderr),
            notes: lines(&ran.stdout),
        })
    }

    /// Readies the draft `name`, `text`, for an install: refuses it when
    /// the installed file changed since its base (exit 3) or the check
    /// fails (exit 1), printing what the check found. A draft whose only
    /// problem is that it wires no complete route is readied with the
    /// plan's `no_route` set, and the caller asks the person, or takes
    /// `--force`, before it installs it. The plan writes vtium.conf when
    /// the draft differs from it, and the batch file when the draft changes
    /// the active resources ([`Session::batch_edit`]). `None` when the
    /// draft is the installed file already and changes no resource, and
    /// the installed file then becomes its base.
    ///
    /// References:
    /// - ADR-1403 D8: Install asks before it installs a draft with no complete route, and `--force` skips that one question.
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
        let checked = self.check(name, text, &shown)?;
        for line in checked.problems.iter().chain(&checked.notes) {
            let _ = writeln!(out, "{line}");
        }
        let folder = self.frit_problems(name)?;
        for line in &folder {
            let _ = writeln!(out, "{line}");
        }
        if !folder.is_empty() {
            return Err(Fail::Check(format!(
                "nothing installed: the font set folder of draft {name} holds files that are no font set change, or font sets that cannot be read. The draft keeps your changes."
            )));
        }
        let no_route = checked.no_route_only(&shown.to_string_lossy());
        if !checked.clears() && !no_route {
            return Err(Fail::Check(format!(
                "nothing installed: the check found problems in draft {name}. The draft keeps your changes."
            )));
        }
        let batch = self.batch_edit(name)?;
        if batch.is_some()
            && let crate::batch::Installed::Refused(why) = self.installed_batch()
        {
            let _ = writeln!(
                out,
                "{why} The install replaces it with the list of a fresh install and the changes of draft {name}."
            );
        }
        let (frits, notes) = self.frit_changes(name)?;
        for line in &notes {
            let _ = writeln!(out, "{line}");
        }
        // the frits' paths come from the installed frits' directory, so a
        // draft that also moves that directory installs the move first
        let key = vtium_settings::keys::FONT_FRITS.name;
        if !frits.is_empty()
            && vtium_settings::setting(text, key)
                != installed
                    .as_deref()
                    .and_then(|i| vtium_settings::setting(i, key))
        {
            return Err(Fail::Check(format!(
                "nothing installed: {} The draft keeps your changes.",
                crate::apply::FRITS_MOVE_FIRST
            )));
        }
        // the privileged step refuses to remove a frit that the vtium.conf
        // it installs still names. The check above catches a look's
        // font-name, but not the top level's, which the console reads as a
        // font family once no frit has that name, so the install refuses
        // that removal here, in the privileged step's own words, before
        // anything runs as root
        for removal in frits.iter().filter(|c| c.new.is_none()) {
            if let Some(by) = crate::apply::naming(&removal.name, Some(text)) {
                return Err(Fail::Check(format!(
                    "nothing installed: {} is not removed: {by} uses it. The draft keeps your changes.",
                    removal.path.display()
                )));
            }
        }
        let same = installed.as_deref() == Some(text);
        if same && batch.is_none() && frits.is_empty() {
            let _ = writeln!(
                out,
                "Nothing to install: draft {name} is the same as the installed vtium.conf."
            );
            self.store.set_base(name, Some(text.as_bytes()))?;
            self.settle_batch(name)?;
            self.settle_frits(name)?;
            return Ok(None);
        }
        // the frits and the batch file go first, so the vtium.conf that
        // follows them never names a frit or a resource that is not there
        // yet, and the removals go last, once vtium.conf no longer needs
        // them
        let (kept, removed): (Vec<FritChange>, Vec<FritChange>) =
            frits.into_iter().partition(|c| c.new.is_some());
        let kept = kept.into_iter().map(|c| FileEdit {
            path: c.path,
            old: c.old.map(|b| String::from_utf8_lossy(&b).into_owned()),
            new: c.new.unwrap_or_default(),
        });
        let removals = removed
            .into_iter()
            .map(|c| crate::draft::Removal {
                path: c.path,
                old: c.old.unwrap_or_default(),
            })
            .collect();
        let conf = (!same).then(|| FileEdit {
            path: self.installed_path(),
            old: installed.clone(),
            new: text.to_owned(),
        });
        Ok(Some(Ready {
            plan: Plan {
                files: kept.chain(batch).chain(conf).collect(),
                removals,
                no_route,
            },
            replaced: if same { None } else { installed },
            conf: !same,
        }))
    }

    /// Hands `plan` to the privileged step, `apply DIR`: at once as root,
    /// else through `run`, which runs sudo for the system's directories.
    /// sudo's own refusal is exit 4; `apply`'s is [`classify`]'s.
    pub fn put(&self, plan: &Plan, run: Runner<'_>, out: &mut dyn Write) -> Result<(), Fail> {
        let targets = self.paths.targets();
        if rustix::process::geteuid().is_root() {
            let wrote = crate::apply::install_now(plan, &targets, &self.paths.helper, out)
                .map_err(Fail::Check)?;
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
    /// makes the draft's base the new installed file, and takes out of
    /// the draft the changes to the active resources that are now
    /// installed.
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
        self.settle_batch(name)?;
        self.settle_frits(name)?;
        let _ = writeln!(out, "installed draft {name}");
        Ok(())
    }

    /// The route of the step after an install (ADR-1201 D5), the one that
    /// ran `apply DIR`: sudo for a user who installs the system's files,
    /// nothing for root, and `None` for other directories, whose vtium.conf
    /// no vtium on a VT reads.
    pub fn via(&self) -> Option<Vec<OsString>> {
        let targets = self.paths.targets();
        if targets.configdir != crate::apply::Targets::system().configdir {
            return None;
        }
        if rustix::process::geteuid().is_root() {
            return Some(Vec::new());
        }
        match crate::apply::route(&targets, &self.paths.sudo) {
            crate::apply::Route::Sudo(sudo) => Some(vec![sudo]),
            crate::apply::Route::User => None,
        }
    }

    /// After an install wrote `text` as vtium.conf: for the system's
    /// vtium.conf, enables vtium on the VTs its heads name and offers the
    /// start and restart that apply it, through `run` and `ask`
    /// ([`crate::vts::reach`]); `None` for other directories.
    pub fn after(
        &self,
        text: &str,
        run: Runner<'_>,
        ask: Option<Ask<'_>>,
        out: &mut dyn Write,
    ) -> Option<crate::vts::Outcome> {
        let via = self.via()?;
        Some(crate::vts::reach(
            text,
            &self.paths.vtium,
            &via,
            run,
            ask,
            out,
        ))
    }

    /// Installs the draft `name`, `text`, as `vtium config install` does:
    /// [`Session::prepare`], then for a draft that wires no complete route
    /// what `unrouted` says, then [`Session::put`] through `run`, then
    /// [`Session::finish`], then [`Session::after`], which asks through
    /// `ask` whether to start and restart vtium, and with no `ask` prints
    /// the commands that would.
    pub fn install(
        &self,
        name: &str,
        text: &str,
        unrouted: Unrouted<'_>,
        run: Runner<'_>,
        ask: Option<Ask<'_>>,
        out: &mut dyn Write,
    ) -> Result<(), Fail> {
        let Some(ready) = self.prepare(name, text, out)? else {
            return Ok(());
        };
        if ready.plan.no_route {
            match unrouted {
                Unrouted::Install => {}
                Unrouted::Refuse => {
                    return Err(Fail::Check(crate::words::no_route_force()));
                }
                Unrouted::Ask(ask) => {
                    let installed = self.installed()?;
                    let (added, removed) = counts(installed.as_deref().unwrap_or_default(), text);
                    let batch = if ready.plan.writes_batch() {
                        self.store.batch_changes(name)?.len()
                    } else {
                        0
                    };
                    let question = crate::words::no_route_question(
                        added,
                        removed,
                        batch,
                        ready.plan.font_sets(),
                    );
                    if !ask(out, &question) {
                        return Err(Fail::Check(crate::words::NO_ROUTE_NOT_INSTALLED.to_owned()));
                    }
                }
            }
        }
        self.put(&ready.plan, &mut *run, out)?;
        self.finish(name, text, ready.replaced.as_deref(), out)?;
        // vtium reads the batch file again at each login, so an install
        // that writes only the batch file needs no step that starts or
        // restarts vtium
        if ready.restarts() {
            self.after(text, run, ask, out);
        }
        Ok(())
    }
}

/// One font set change of a draft against the installed frits
/// ([`Session::frit_changes`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FritChange {
    /// The frit's name.
    pub name: String,
    /// Its file in the frits' directory, which the install writes or
    /// removes.
    pub path: PathBuf,
    /// The installed frit's bytes, `None` for a frit the draft adds.
    pub old: Option<Vec<u8>>,
    /// The frit's text after the install, `None` for a removal.
    pub new: Option<String>,
}

impl FritChange {
    /// What the change does to the installed frits.
    pub const fn edit(&self) -> FritEdit {
        match (&self.old, &self.new) {
            (_, None) => FritEdit::Remove,
            (None, Some(_)) => FritEdit::Add,
            (Some(_), Some(_)) => FritEdit::Replace,
        }
    }
}

/// What a font set change does to the installed frits: adds a frit,
/// replaces one, or removes one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FritEdit {
    Add,
    Replace,
    Remove,
}

impl FritEdit {
    /// The change's word, as `flow --json` lists it under `"frits"`.
    pub const fn word(self) -> &'static str {
        match self {
            FritEdit::Add => "add",
            FritEdit::Replace => "replace",
            FritEdit::Remove => "remove",
        }
    }
}

/// The font set changes that the folder `folder` of the draft `name` makes
/// against the frits installed in the frits' directory `frits`, in the
/// order of their names. This one function decides what counts as a
/// change for the check's options, `flow`, `diff`, install and the TUI's
/// status line. `read` gives the bytes of the installed file at a path,
/// `None` when there is none. A frit the folder adds or replaces counts
/// when its text differs from the installed frit's. A removal counts when
/// the frits' directory itself holds the frit, since an install removes
/// nothing from the catalog's category folders. Also the notes on what the
/// folder holds that is no change: each removal that removes nothing.
///
/// References:
/// - ADR-1403 D8, "A draft's font set changes": a frit with the installed text is no change, and a removal of a frit the machine lacks removes nothing.
pub fn frit_changes(
    name: &str,
    folder: &crate::store::FritChanges,
    frits: &Path,
    read: impl Fn(&Path) -> Option<Vec<u8>>,
) -> (Vec<FritChange>, Vec<String>) {
    let installed = |frit: &str| {
        let path = frits.join(format!("{frit}.vtfrit"));
        let old = read(&path);
        (path, old)
    };
    let mut changes = Vec::new();
    let mut notes = Vec::new();
    for (frit, text) in &folder.kept {
        let (path, old) = installed(frit);
        if old.as_deref() == Some(text.as_bytes()) {
            continue;
        }
        changes.push(FritChange {
            name: frit.clone(),
            path,
            old,
            new: Some(text.clone()),
        });
    }
    for frit in &folder.removed {
        match installed(frit) {
            (path, Some(old)) => changes.push(FritChange {
                name: frit.clone(),
                path,
                old: Some(old),
                new: None,
            }),
            (_, None) => notes.push(format!(
                "Draft {name} removes the font set {frit}, but no installed font set is named {frit}, so the removal removes nothing."
            )),
        }
    }
    changes.sort_by(|a, b| a.name.cmp(&b.name));
    (changes, notes)
}

/// `2 font set changes`, as the state line counts them.
pub fn frit_count(n: usize) -> String {
    match n {
        1 => "1 font set change".to_owned(),
        n => format!("{n} font set changes"),
    }
}

/// What an install does with a draft whose only problem is that it wires
/// no complete route (ADR-1403 D8).
pub enum Unrouted<'a> {
    /// Installs it: `vtium config install --force`.
    Install,
    /// Asks the question, and installs it only when the answer is yes:
    /// `vtium config install` at a terminal.
    Ask(Ask<'a>),
    /// Refuses it, exit 1, and says how to install it anyway: `install`
    /// with no terminal, and every command's `--install`, which installs
    /// only a draft the check passes.
    Refuse,
}

/// A draft the check passed, or found only missing fonts in, or whose only
/// problem is that it wires no complete route, ready for the privileged step: the plan it installs and
/// the installed file it replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ready {
    pub plan: Plan,
    pub replaced: Option<String>,
    /// Whether the plan writes vtium.conf. An install that writes only
    /// font sets and the batch file leaves vtium.conf as it is.
    pub conf: bool,
}

impl Ready {
    /// Whether the running vtium reads what the install writes only when
    /// it starts again: vtium.conf or a font set. The batch file alone it
    /// reads again at the next login.
    pub fn restarts(&self) -> bool {
        self.conf || self.plan.font_sets() > 0
    }
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

    // vtium reads the batch file again at each login, so an install of
    // the batch file alone offers no restart; vtium.conf and a font set
    // are read when vtium starts
    #[test]
    fn only_an_install_of_vtium_conf_or_a_font_set_restarts_vtium() {
        let edit = |path: &str| FileEdit {
            path: PathBuf::from(path),
            old: None,
            new: String::new(),
        };
        let ready = |files: Vec<FileEdit>, conf: bool| Ready {
            plan: Plan {
                files,
                removals: Vec::new(),
                no_route: false,
            },
            replaced: None,
            conf,
        };
        let batch = ready(vec![edit("/etc/vtium/batch.conf")], false);
        assert!(batch.plan.writes_batch());
        assert!(!batch.restarts());
        let frit = ready(vec![edit("/etc/vtium/fonts/mono.vtfrit")], false);
        assert_eq!(frit.plan.font_sets(), 1);
        assert!(frit.restarts());
        let conf = ready(
            vec![edit("/etc/vtium/batch.conf"), edit("/etc/vtium/vtium.conf")],
            true,
        );
        assert!(conf.restarts());
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
