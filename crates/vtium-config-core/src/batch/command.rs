//! The commands `activate`, `deactivate` and `active`, and the current
//! draft's view of the active resources they share with the tabs.

use std::io::Write;
use std::path::{Path, PathBuf};

use vtium_settings::batch::{Batch, Entry};

use crate::drafts::{Session, Unrouted, run_child};
use crate::outcome::Fail;
use crate::places::Paths;
use crate::store::Store;

use super::installed::{BATCH_KIB, too_long};
use super::{
    Change, Changes, Installed, NOT_INSTALLED, Places, Start, View, builtin, kind, list_lines,
    not_installed, origin_words, plain, rule_user, trusted, users, vtlook_files,
};

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
    /// What the batch starts from besides the batch file: the list of a
    /// fresh install and the folders of lists for this install.
    start: Start,
    /// The batch the system holds, with the lists.
    view: View,
    changes: Changes,
    /// The user whose files the trust rule accepts for this install.
    euid: u32,
}

impl<'a> Resources<'a> {
    /// The current draft's active resources.
    pub fn open(session: &'a Session<'a>) -> Result<Resources<'a>, Fail> {
        let (name, text) = session.current()?;
        let installed = session.installed_batch();
        let changes = session.store.batch_changes(&name)?;
        let euid = rule_user(&session.paths.targets(), &session.paths.sudo);
        let start = super::start(session.paths);
        let view = installed.view(&start);
        Ok(Resources {
            session,
            name,
            text,
            installed,
            start,
            view,
            changes,
            euid,
        })
    }

    /// The batch the system holds, with the packages' and root's lists,
    /// as vtium reads it.
    pub fn view(&self) -> &View {
        &self.view
    }

    /// The batch the system holds.
    fn base(&self) -> &Batch {
        self.view.batch()
    }

    /// Where the resource at `path` stands in the draft.
    pub fn state(&self, path: &Path) -> State {
        match (self.base().holds(path), self.changes.get(path)) {
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
            .base()
            .entries()
            .iter()
            .map(|Entry::Path(p)| (p.clone(), self.state(p)));
        let added = self
            .changes
            .iter()
            .filter(|(p, c)| *c == Change::Activate && !self.base().holds(p))
            .map(|(p, _)| (p.to_path_buf(), State::Activates));
        held.chain(added).collect()
    }

    /// The lines `vtium config active` prints, each of four columns
    /// separated by tabs: the kind, the path, the state, and where the
    /// state comes from. The built-in font comes first; then each active
    /// resource, the batch file's or the list of a fresh install's before
    /// the lists'; each path batch.conf turns off; each resource the draft
    /// activates; and last one line for each entry in the folders of
    /// lists, as [`list_lines`] gives it. A path that a list activates
    /// where no file is installed is [`NOT_INSTALLED`], and a state the
    /// draft makes comes from `the draft`.
    ///
    /// References:
    /// - ADR-1205 D4, as amended on 2026-11-27: what `vtium config active` prints.
    pub fn lines(&self) -> Vec<String> {
        use vtium_settings::batch::Origin;
        const DRAFT: &str = "the draft";
        let line = |path: &Path, state: &str, from: &str| {
            format!("{}\t{}\t{state}\t{from}", kind(path), path.display())
        };
        let base = self.base();
        let mut out = vec![format!(
            "font\tbuilt-in\t{}\tbuilt in",
            State::Always.words()
        )];
        for Entry::Path(path) in base.entries() {
            let state = self.state(path);
            out.push(match base.origin(path) {
                _ if state == State::Deactivates => line(path, state.words(), DRAFT),
                Some(origin @ Origin::List(_)) if not_installed(path) => {
                    line(path, NOT_INSTALLED, origin_words(origin))
                }
                Some(origin) => line(path, state.words(), origin_words(origin)),
                None => line(path, state.words(), "batch.conf"),
            });
        }
        for path in base.turned_off() {
            if self.changes.get(path).is_none() {
                out.push(line(path, State::Inactive.words(), "batch.conf"));
            }
        }
        for (path, change) in self.changes.iter() {
            if change == Change::Activate && !base.holds(path) {
                out.push(line(path, State::Activates.words(), DRAFT));
            }
        }
        out.extend(list_lines(&self.view));
        out
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
    /// vtium reads, [`BATCH_MAX`](vtium_settings::batch::BATCH_MAX) bytes, or at least is no longer
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
        let start = (&self.start.fresh, self.base());
        match too_long(installed, start, before, &self.changes) {
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
            Change::Activate if !self.base().holds(path) => trusted(path, self.euid)?,
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
            (self.base().holds(path) != wanted).then_some(change),
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

/// `active`: one line for each resource that is active, that the draft
/// changes or that batch.conf turns off, then one for each entry in the
/// folders of lists ([`Resources::lines`]), and last why vtium reads the
/// list of a fresh install, where it does.
fn list(session: &Session<'_>, words: &[String], out: &mut dyn Write) -> Result<(), Fail> {
    if !words.is_empty() {
        return Err(Fail::Usage("active takes no argument".to_owned()));
    }
    let resources = Resources::open(session)?;
    for line in resources.lines() {
        let _ = writeln!(out, "{line}");
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
