//! The TUI's desk (ADR-1403 D7, D8): the draft the Flow view shows and
//! changes, the check it runs after each change, the installed history,
//! and the devices present. The state machine asks for each of these with
//! an [`Op`], and the loop hands the op to a [`Desk`] and the answer back
//! as an event, so the state machine itself reads and writes no file.
//!
//! [`Store`] is the real desk: vtium-config's store, the check through the
//! console binary, and a source of the devices present, which is the seam
//! a test fills with devices of its own instead of the machine's.

use crate::apply::{Ask, Runner};
use crate::drafts::{HEADER, Session, State};
use crate::flowmodel::Present;
use crate::outcome::Fail;
use crate::tui::Paths;

/// One thing the desk does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// Opens the current draft, making one from the installed file when
    /// there is none, and reads the devices present.
    Open,
    /// Writes `text` as the current draft, whatever the check says, and
    /// checks it: Save (ADR-1403 D7).
    Write(String),
    /// Checks `text`, the current draft with the TUI's unsaved changes,
    /// and counts it against the installed file, and writes nothing.
    Check(String),
    /// Readies an install: refuses a draft made before the installed file
    /// changed, or one the check fails; otherwise gives the plan the
    /// privileged step installs, which [`Desk::install`] runs.
    Prepare,
    /// Copies the installed file back over the draft.
    Revert,
    /// Empties the draft to the top level at its defaults and one look.
    New,
    /// Deletes the draft and starts a new one from the installed file.
    Delete,
    /// Lists the installed history, newest first.
    History,
    /// Copies the history's file of this number, counted from 1, newest
    /// first, into the draft.
    Restore(usize),
    /// Reads the devices present again, and the connected displays'
    /// densities.
    Devices,
    /// Activates or deactivates the resource at `path` in the draft,
    /// which keeps the change beside it until the install.
    ///
    /// References:
    /// - ADR-1205 D4: a draft's changes to the active resources wait
    ///   beside it until the install.
    Resource {
        path: std::path::PathBuf,
        change: crate::batch::Change,
    },
    /// Readies the install of the font file at `path`, one in the
    /// person's own folder, as a system font: reads the file once into a
    /// draft of its own, has the font helper check that the draft's copy
    /// is a font, and gives the command [`Desk::install_font`] runs. A
    /// file already installed with the same bytes needs no install.
    SystemFont(std::path::PathBuf),
}

/// The current draft as the desk read or wrote it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opened {
    pub name: String,
    pub text: String,
    /// Lines added and removed against the installed file.
    pub added: usize,
    pub removed: usize,
    /// The installed file changed since the draft was made from it.
    pub stale: bool,
    /// What the check found, each line as `vtium check-config` prints it.
    pub problems: Vec<String>,
    /// The problems the status line counts, the commands' count.
    pub count: usize,
    /// The draft's changes to the active resources, which the status line
    /// counts among its changes.
    pub batch: usize,
    /// Each resource that is active or that the draft activates or
    /// deactivates, with its state, so the tabs' marks follow the draft
    /// after every answer that changes the draft or its batch.
    pub resources: Vec<(std::path::PathBuf, crate::batch::State)>,
}

/// One file of the installed history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    /// Its name, `DRAFT-N.conf`.
    pub name: String,
    pub text: String,
}

/// What the desk did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Done {
    /// The draft after the op, and the devices present when the op read
    /// them.
    Draft(Opened, Option<Present>),
    /// The draft after an install of the system's vtium.conf, and what the
    /// step after it did on the VTs (ADR-1201 D5).
    Installed(Opened, crate::vts::Outcome),
    /// The plan an install hands the privileged step.
    Plan(crate::draft::Plan),
    /// The install is refused: why, and what the check printed.
    Refused(String, Vec<String>),
    History(Vec<Kept>),
    /// The devices present after [`Op::Devices`], and the density of each
    /// connected display, read again with them, as
    /// [`crate::tui::model::Inputs::densities`] holds it.
    Present {
        present: Present,
        densities: Vec<u32>,
    },
    /// The draft's view of the active resources after [`Op::Resource`]:
    /// each resource that is active or that the draft changes, with its
    /// state; what the change says, its warnings included; and how many
    /// changes the draft makes to the active resources.
    Resources {
        states: Vec<(std::path::PathBuf, crate::batch::State)>,
        said: String,
        changes: usize,
    },
    /// The install of a font file as a system font, readied by
    /// [`Op::SystemFont`].
    SystemFont(FontReady),
    /// The font file installed as a system font at `target`; what the
    /// privileged step printed, its check against the trust rule apart;
    /// why the copy fails the trust rule, where it does; and the font
    /// files found after it, or why the scan failed.
    FontInstalled {
        target: std::path::PathBuf,
        said: String,
        untrusted: Option<String>,
        found: Result<Vec<crate::discover::Found>, String>,
    },
}

/// The install of a font file as a system font, as the question before it
/// shows it: the file, where it goes, and whether a file of its name is
/// there; for a different file of that name, both files' facts; and the
/// exact command the install runs. Each text has passed through
/// [`crate::onscreen`], so a file's name prints as text.
///
/// References:
/// - ADR-1205 D5, its note of 2026-11-18: both files on a clash, and
///   nothing when they are identical.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontReady {
    pub source: String,
    pub target: String,
    pub placement: crate::apply::Placement,
    /// The installed file's facts, then the person's file's, for a file
    /// that replaces another.
    pub facts: Option<(String, String)>,
    /// The command, as [`crate::vts::shown`] spells it; empty when the
    /// font is installed already.
    pub command: String,
}

/// What the loop hands each [`Op`] to.
pub trait Desk {
    fn run(&mut self, op: &Op) -> Result<Done, String>;
    /// Installs `plan`, the prepared draft's and the frits beside it,
    /// through the privileged step `run` runs, then records the install,
    /// then, for the system's vtium.conf, enables vtium on the VTs its
    /// heads name and asks through `ask` whether to start and restart
    /// vtium, writing what that step does to `out` (ADR-1201 D5); the draft
    /// afterwards.
    fn install(
        &mut self,
        plan: &crate::draft::Plan,
        run: Runner<'_>,
        out: &mut dyn std::io::Write,
        ask: Ask<'_>,
    ) -> Result<Done, String>;

    /// Installs the font file that [`Op::SystemFont`] readied, through the
    /// privileged step `run` runs, and gives [`Done::FontInstalled`]. A
    /// desk that readies no font says so.
    fn install_font(&mut self, _run: Runner<'_>) -> Result<Done, String> {
        Err(crate::tui::words::NO_FONT_READY.to_owned())
    }
}

/// Reads the devices present.
pub type Devices = Box<dyn FnMut() -> Present>;

/// The real desk: the store under the user's XDG directories, the
/// installed file and the check the paths name, and the devices `devices`
/// reads.
pub struct Store {
    store: crate::store::Store,
    paths: Paths,
    devices: Devices,
    /// The install that [`Op::Prepare`] readied, which [`Desk::install`]
    /// runs.
    ready: Option<Prepared>,
    /// The system font that [`Op::SystemFont`] readied, which
    /// [`Desk::install_font`] runs.
    font: Option<FontDraft>,
}

/// A font file readied as a system font: the draft that holds its copy,
/// the command that installs the draft, and where the font goes.
struct FontDraft {
    draft: std::path::PathBuf,
    argv: Vec<std::ffi::OsString>,
    target: std::path::PathBuf,
}

/// An install that [`Op::Prepare`] readied: the draft's name and text, and
/// what the install does with vtium.conf.
struct Prepared {
    name: String,
    text: String,
    conf: Conf,
}

/// What a prepared install does with vtium.conf.
enum Conf {
    /// The draft is the installed vtium.conf already, so the install writes
    /// and removes frits alone and keeps nothing in the installed history.
    Same,
    /// The install writes the draft over the installed vtium.conf, whose
    /// text goes into the installed history once the privileged step has
    /// run, `None` when no vtium.conf is installed.
    Replaces(Option<String>),
}

impl Store {
    pub fn new(store: crate::store::Store, paths: Paths, devices: Devices) -> Store {
        Store {
            store,
            paths,
            devices,
            ready: None,
            font: None,
        }
    }

    /// Removes the draft of a font readied earlier and not installed, so a
    /// question answered with Esc leaves no draft behind once another font
    /// is readied or the TUI ends.
    fn drop_font(&mut self) {
        if let Some(old) = self.font.take() {
            let _ = std::fs::remove_dir_all(&old.draft);
        }
    }

    /// [`Op::SystemFont`]: reads the font file at `source` once into a
    /// draft that holds this one font, then has the font helper describe
    /// the draft's copy, so the file the helper judges is the file that is
    /// installed. A copy the helper cannot read as a font is refused, and
    /// its draft removed. When the folder already holds a file of the
    /// same name with the same bytes, nothing needs doing and no draft is
    /// written. When it holds other bytes, both files are described for
    /// the question.
    ///
    /// References:
    /// - ADR-1205 D5: the install step copies one font file into
    ///   `/usr/local/share/fonts` through the privileged step.
    fn system_font(&mut self, source: &std::path::Path) -> Result<Done, Fail> {
        use crate::apply::{Placement, SystemFont};
        use crate::discover::{Described, FileFacts, describe};
        use crate::onscreen::{path as shown, text};
        self.drop_font();
        let targets = self.paths.targets();
        let font = SystemFont::read(source, &targets).map_err(|why| Fail::Usage(text(&why)))?;
        let placement = font.placement();
        let mut ready = FontReady {
            source: shown(&font.source),
            target: shown(&font.target),
            placement,
            facts: None,
            command: String::new(),
        };
        if placement == Placement::Identical {
            return Ok(Done::SystemFont(ready));
        }
        let draft = font.write_draft(&self.paths.drafts).map_err(|e| {
            Fail::File(text(&format!(
                "the draft under {}: {e}",
                self.paths.drafts.display()
            )))
        })?;
        let mut asked = vec![font.draft_copy(&draft)];
        if placement == Placement::Replaces {
            asked.push(font.target.clone());
        }
        let described = describe(&self.paths.helper, &asked);
        let unknown = Described::Unknown(crate::tui::words::HELPER_SILENT.to_owned());
        let yours = described.first().unwrap_or(&unknown);
        if let Described::NotAFont(why) | Described::Unknown(why) = yours {
            let _ = std::fs::remove_dir_all(&draft);
            return Err(Fail::Check(crate::tui::words::system_font_not_a_font(
                &ready.source,
                &text(why),
            )));
        }
        if placement == Placement::Replaces {
            let installed = FileFacts::of(&font.target, described.get(1).unwrap_or(&unknown));
            ready.facts = Some((installed.line(), FileFacts::of(&font.source, yours).line()));
        }
        let program =
            std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("vtium-config"));
        let route = crate::apply::route(&targets, &self.paths.sudo);
        let argv = crate::apply::save_argv(&route, &program, &targets, &draft);
        ready.command = text(&crate::vts::shown(&argv));
        self.font = Some(FontDraft {
            draft,
            argv,
            target: font.target,
        });
        Ok(Done::SystemFont(ready))
    }

    fn session(&self) -> Session<'_> {
        Session {
            store: self.store.clone(),
            paths: &self.paths,
        }
    }

    /// The draft `name`, `text`, with its state and its check.
    fn opened(&self, name: &str, text: &str) -> Result<Opened, Fail> {
        let session = self.session();
        let state = session.state(name, text)?;
        let checked = session.check(text, &self.store.draft_path(name)?)?;
        let (added, removed) = state.counts();
        Ok(Opened {
            name: name.to_owned(),
            text: text.to_owned(),
            added,
            removed,
            stale: matches!(state, State::Stale { .. }),
            count: checked.count(),
            problems: checked.problems,
            batch: self.store.batch_changes(name)?.len(),
            resources: crate::batch::Resources::open(&session)?.list(),
        })
    }

    /// Writes `text` as the current draft and opens it.
    fn write(&self, text: &str) -> Result<Done, Fail> {
        let (name, _) = self.session().current()?;
        self.store.write_draft(&name, text)?;
        Ok(Done::Draft(self.opened(&name, text)?, None))
    }

    fn step(&mut self, op: &Op) -> Result<Done, Fail> {
        match op {
            Op::Devices => {
                // a display connected since the TUI started counts toward
                // a look's point sizes, so its density is read again here
                let conf = crate::tui::read_conf(&self.paths.configdir.join("vtium.conf"))
                    .ok()
                    .flatten();
                let displays = crate::display::displays(&self.paths.sys);
                return Ok(Done::Present {
                    present: (self.devices)(),
                    densities: crate::tui::densities(conf.as_deref(), &displays),
                });
            }
            Op::SystemFont(source) => return self.system_font(source),
            Op::Open => {
                let present = (self.devices)();
                let (name, text) = self.session().current()?;
                let opened = self.opened(&name, &text)?;
                return Ok(Done::Draft(opened, Some(present)));
            }
            _ => {}
        }
        let session = self.session();
        match op {
            Op::Write(text) => self.write(text),
            Op::Check(text) => {
                let (name, _) = session.current()?;
                Ok(Done::Draft(self.opened(&name, text)?, None))
            }
            Op::Prepare => {
                let (name, text) = session.current()?;
                let mut said = Vec::new();
                // a draft that is the installed file still readies an
                // install, because the frits waiting beside it install
                // without it (ADR-1403 D8)
                let done = match session.prepare(&name, &text, &mut said) {
                    Ok(None) => {
                        self.ready = Some(Prepared {
                            name,
                            text,
                            conf: Conf::Same,
                        });
                        Done::Plan(crate::draft::Plan::default())
                    }
                    Ok(Some(ready)) => {
                        self.ready = Some(Prepared {
                            name,
                            text,
                            conf: Conf::Replaces(ready.replaced),
                        });
                        Done::Plan(ready.plan)
                    }
                    Err(fail @ (Fail::Changed(_) | Fail::Check(_))) => Done::Refused(
                        fail.text().to_owned(),
                        String::from_utf8_lossy(&said)
                            .lines()
                            .map(str::to_owned)
                            .collect(),
                    ),
                    Err(fail) => return Err(fail),
                };
                Ok(done)
            }
            Op::Revert => {
                let (name, _) = session.current()?;
                let installed = session.installed()?;
                let text = installed.clone().unwrap_or_else(|| HEADER.to_owned());
                self.store
                    .set_base(&name, installed.as_deref().map(str::as_bytes))?;
                // the draft starts over, so its changes to the active
                // resources go, as the command line's revert drops them
                session.drop_batch(&name)?;
                self.store.write_draft(&name, &text)?;
                Ok(Done::Draft(self.opened(&name, &text)?, None))
            }
            Op::New => {
                let (name, _) = session.current()?;
                session.drop_batch(&name)?;
                self.write(&crate::ops::new_text(HEADER))
            }
            Op::Delete => {
                let (name, _) = session.current()?;
                self.store.remove_draft(&name)?;
                self.store.set_current(None)?;
                let (name, text) = session.current()?;
                Ok(Done::Draft(self.opened(&name, &text)?, None))
            }
            Op::History => {
                let mut out = Vec::new();
                for kept in self.store.history()? {
                    let text = crate::tui::read_conf(&kept.path)
                        .map_err(Fail::File)?
                        .unwrap_or_default();
                    out.push(Kept {
                        name: kept.file_name(),
                        text,
                    });
                }
                Ok(Done::History(out))
            }
            Op::Restore(n) => {
                let history = self.store.history()?;
                let kept = n
                    .checked_sub(1)
                    .and_then(|i| history.get(i))
                    .ok_or_else(|| Fail::Usage(crate::tui::words::no_history_file(*n)))?;
                let text = crate::tui::read_conf(&kept.path)
                    .map_err(Fail::File)?
                    .unwrap_or_default();
                self.write(&text)
            }
            Op::Resource { path, change } => {
                let mut resources = crate::batch::Resources::open(&session)?;
                let said = match resources.set(path, *change).map_err(Fail::Check)? {
                    crate::batch::Done::Changed { warnings } => {
                        let kind = crate::batch::kind(path);
                        std::iter::once(crate::tui::words::changed(*change, kind))
                            .chain(warnings)
                            .collect::<Vec<_>>()
                            .join(" ")
                    }
                    crate::batch::Done::AlreadyActive => {
                        crate::tui::words::already_active(crate::batch::kind(path))
                    }
                    crate::batch::Done::AlreadyActivates => {
                        crate::tui::words::already_activates(crate::batch::kind(path))
                    }
                };
                resources.save()?;
                Ok(Done::Resources {
                    states: resources.list(),
                    said,
                    changes: self.store.batch_changes(&resources.name)?.len(),
                })
            }
            Op::Open | Op::Devices | Op::SystemFont(_) => Err(Fail::Usage(String::new())),
        }
    }
}

/// A font readied and never installed, because the person answered its
/// question with Esc, leaves no draft once the TUI ends.
impl Drop for Store {
    fn drop(&mut self) {
        self.drop_font();
    }
}

impl Desk for Store {
    fn run(&mut self, op: &Op) -> Result<Done, String> {
        if *op == Op::Prepare {
            self.ready = None;
        }
        self.step(op).map_err(|f| f.text().to_owned())
    }

    fn install(
        &mut self,
        plan: &crate::draft::Plan,
        run: Runner<'_>,
        out: &mut dyn std::io::Write,
        ask: Ask<'_>,
    ) -> Result<Done, String> {
        let Some(Prepared { name, text, conf }) = self.ready.take() else {
            return Err(crate::tui::words::NOT_PREPARED.to_owned());
        };
        let session = self.session();
        let mut said = Vec::new();
        let opened = session
            .put(plan, &mut *run, &mut said)
            .and_then(|()| match &conf {
                Conf::Same => Ok(()),
                Conf::Replaces(replaced) => {
                    session.finish(&name, &text, replaced.as_deref(), &mut said)
                }
            })
            .and_then(|()| self.opened(&name, &text))
            .map_err(|fail| {
                let said = String::from_utf8_lossy(&said);
                format!("{}\n{}", fail.text(), said.trim_end())
            })?;
        // the step after an install runs when it wrote vtium.conf, as the
        // command line's install does
        let after = match conf {
            Conf::Same => None,
            Conf::Replaces(_) => session.after(&text, run, Some(ask), out),
        };
        Ok(match after {
            Some(outcome) => Done::Installed(opened, outcome),
            None => Done::Draft(opened, None),
        })
    }

    /// Runs the command [`Op::SystemFont`] showed, through `run`, which
    /// asks for sudo's password on the terminal. `apply` prints `wrote
    /// PATH`, `failed PATH WHY`, or `wrote PATH` then `untrusted PATH WHY`,
    /// and each column passes through [`crate::onscreen::text`]. The font
    /// files are found again afterwards, so the new system font joins the
    /// list, where the person can activate it. The font's draft is removed
    /// once the command has run, whatever its outcome.
    fn install_font(&mut self, run: Runner<'_>) -> Result<Done, String> {
        use crate::onscreen::text;
        let Some(font) = self.font.take() else {
            return Err(crate::tui::words::NO_FONT_READY.to_owned());
        };
        let ran = run(&font.argv);
        // the command has read the draft, or failed for good, so the
        // draft goes; a new attempt readies a draft of its own
        let _ = std::fs::remove_dir_all(&font.draft);
        let (ok, said) = ran.map_err(|e| text(&e.to_string()))?;
        let mut untrusted = None;
        let mut lines = Vec::new();
        for line in said.lines() {
            let columns: Vec<String> = line.split('\t').map(text).collect();
            match columns.split_first() {
                Some((first, rest)) if first == "untrusted" => {
                    untrusted = Some(rest.get(1..).unwrap_or_default().join(" "));
                }
                _ => lines.push(columns.join(" ")),
            }
        }
        let said = lines.join(" ");
        if !ok {
            return Err(said);
        }
        let found = crate::tui::read_conf(&self.paths.configdir.join("vtium.conf"))
            .and_then(|conf| crate::tui::scan_fonts(&self.store, &self.paths, conf.as_deref()))
            .map(|(found, _)| found)
            .map_err(|why| text(&why));
        Ok(Done::FontInstalled {
            target: font.target,
            said,
            untrusted,
            found,
        })
    }
}

/// The devices present as the commands read them: sysfs and udev's
/// database under `roots`, and the udev daemon whose control socket is
/// `control`.
pub fn machine(devices: crate::flowcli::Devices) -> Devices {
    Box::new(move || crate::flowmodel::present(&devices.roots, &devices.control))
}
