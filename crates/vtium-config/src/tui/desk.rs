//! The TUI's desk (ADR-1403 D7, D8): the draft the Flow view shows and
//! changes, the check it runs after each change, the installed history,
//! and the devices present. The state machine asks for each of these with
//! an [`Op`], and the loop hands the op to a [`Desk`] and the answer back
//! as an event, so the state machine itself reads and writes no file.
//!
//! [`Store`] is the real desk: vtium-config's store, the check through the
//! console binary, and a source of the devices present, which is the seam
//! a test fills with devices of its own instead of the machine's.

use crate::apply::Runner;
use crate::drafts::{EXAMPLE, Session, State};
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
    /// checks it.
    Write(String),
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
    /// Reads the devices present again.
    Devices,
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
    /// The plan an install hands the privileged step.
    Plan(crate::draft::Plan),
    /// The install is refused: why, and what the check printed.
    Refused(String, Vec<String>),
    History(Vec<Kept>),
    Present(Present),
}

/// What the loop hands each [`Op`] to.
pub trait Desk {
    fn run(&mut self, op: &Op) -> Result<Done, String>;
    /// Installs `plan`, the prepared draft's and the frits beside it,
    /// through the privileged step `run` runs, then records the install;
    /// the draft afterwards.
    fn install(&mut self, plan: &crate::draft::Plan, run: Runner<'_>) -> Result<Done, String>;
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
        }
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
            Op::Devices => return Ok(Done::Present((self.devices)())),
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
                let text = installed.clone().unwrap_or_else(|| EXAMPLE.to_owned());
                self.store
                    .set_base(&name, installed.as_deref().map(str::as_bytes))?;
                self.store.write_draft(&name, &text)?;
                Ok(Done::Draft(self.opened(&name, &text)?, None))
            }
            Op::New => self.write(&crate::ops::new_text(EXAMPLE)),
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
            Op::Open | Op::Devices => Err(Fail::Usage(String::new())),
        }
    }
}

impl Desk for Store {
    fn run(&mut self, op: &Op) -> Result<Done, String> {
        if *op == Op::Prepare {
            self.ready = None;
        }
        self.step(op).map_err(|f| f.text().to_owned())
    }

    fn install(&mut self, plan: &crate::draft::Plan, run: Runner<'_>) -> Result<Done, String> {
        let Some(Prepared { name, text, conf }) = self.ready.take() else {
            return Err(crate::tui::words::NOT_PREPARED.to_owned());
        };
        let session = self.session();
        let mut said = Vec::new();
        let done = session
            .put(plan, run, &mut said)
            .and_then(|()| match &conf {
                Conf::Same => Ok(()),
                Conf::Replaces(replaced) => {
                    session.finish(&name, &text, replaced.as_deref(), &mut said)
                }
            })
            .and_then(|()| self.opened(&name, &text));
        done.map(|opened| Done::Draft(opened, None))
            .map_err(|fail| {
                let said = String::from_utf8_lossy(&said);
                format!("{}\n{}", fail.text(), said.trim_end())
            })
    }
}

/// The devices present as the commands read them: sysfs and udev's
/// database under `roots`, and the udev daemon whose control socket is
/// `control`.
pub fn machine(devices: crate::flowcli::Devices) -> Devices {
    Box::new(move || crate::flowmodel::present(&devices.roots, &devices.control))
}
