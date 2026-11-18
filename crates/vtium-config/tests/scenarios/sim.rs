//! The TUI's state machine with stand-ins around it, so a scenario drives
//! it with no terminal, no pty and no files. [`Sim`] holds an [`App`], a
//! stand-in for the terminal it runs in and a stand-in for its desk. Each
//! key goes to [`App::event`], and every effect the key causes is answered
//! at once, the way the loop in `port` answers it: a desk operation by the
//! desk, a font switch by the terminal, a picture by the helper's refusal.
//! So the state after [`Sim::key`] is the state the TUI rests in once the
//! key's work is done.

use vtium_config::draft::{FileEdit, Plan};
use vtium_config::flowmodel::Present;
use vtium_config::tui::desk::{Done, Kept, Op, Opened};
use vtium_config::tui::input::{Button, Input, Key, Mouse, Reply};
use vtium_config::tui::model::{App, Effect, Event};
use vtium_settings::{Class, Identity};

/// The kinds of terminal the TUI tells apart (ADR-1201 D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Term {
    /// A vtium console that switches fonts: OSC 50 works.
    Vtium,
    /// A vtium console with `allow-font-ops` off.
    OpsOff,
    /// Any other terminal.
    Other,
}

impl Term {
    pub const ALL: [Term; 3] = [Term::Vtium, Term::OpsOff, Term::Other];

    pub fn name(self) -> &'static str {
        match self {
            Term::Vtium => "vtium",
            Term::OpsOff => "ops-off",
            Term::Other => "other",
        }
    }
}

/// The comment lines the stand-in desk's draft ends with.
const LONG_FILE_COMMENTS: usize = 30;

/// The font the stand-in console draws when the TUI starts.
pub const START_FONT: &str = "unifont";

/// The stand-in desk: the installed vtium.conf, the draft and the
/// installed history, all in memory. Every check passes.
#[derive(Clone, Debug)]
pub struct Desk {
    pub installed: String,
    pub draft: String,
    pub history: Vec<String>,
    /// How many times the draft was written, by Save, by a signal or by
    /// an install's save.
    pub writes: usize,
    /// The check finds that the draft wires no complete route, so a
    /// prepared install needs the person's yes.
    pub no_route: bool,
}

impl Desk {
    fn opened(&self, text: &str) -> Opened {
        let (added, removed) = vtium_config::drafts::counts(&self.installed, text);
        Opened {
            name: "mine".into(),
            text: text.to_owned(),
            added,
            removed,
            ..Default::default()
        }
    }

    fn draft(&self) -> Done {
        Done::Draft(self.opened(&self.draft), None)
    }

    fn run(&mut self, op: &Op) -> Result<Done, String> {
        Ok(match op {
            Op::Open => Done::Draft(self.opened(&self.draft), Some(present())),
            Op::Write(text) | Op::WriteChecked(text) => {
                self.draft.clone_from(text);
                self.writes = self.writes.saturating_add(1);
                self.draft()
            }
            Op::Check(text) => Done::Draft(self.opened(text), None),
            Op::Prepare if self.draft == self.installed => Done::Plan(Plan {
                no_route: self.no_route,
                ..Plan::default()
            }),
            Op::Prepare => Done::Plan(Plan {
                files: vec![FileEdit {
                    path: "/etc/vtium/vtium.conf".into(),
                    old: Some(self.installed.clone()),
                    new: self.draft.clone(),
                }],
                removals: Vec::new(),
                no_route: self.no_route,
            }),
            Op::Revert | Op::Delete => {
                self.draft.clone_from(&self.installed);
                self.draft()
            }
            Op::New => {
                self.draft = "version = 1\n[look default]\n".to_owned();
                self.draft()
            }
            Op::History => Done::History(
                self.history
                    .iter()
                    .enumerate()
                    .map(|(i, text)| Kept {
                        name: format!("DRAFT-{}.conf", i.saturating_add(1)),
                        text: text.clone(),
                    })
                    .collect(),
            ),
            Op::Restore(n) => {
                let text = self
                    .history
                    .get(n.saturating_sub(1))
                    .cloned()
                    .ok_or_else(|| format!("no installed file {n}"))?;
                self.draft = text;
                self.draft()
            }
            Op::Devices => Done::Present(present()),
        })
    }
}

/// The devices the stand-in machine has: a monitor, a keyboard, a mouse,
/// and a power button, which is neither a keyboard nor a pointer.
pub fn present() -> Present {
    let mut monitor = Identity::display();
    monitor.edid = Some(vtium_settings::Edid {
        maker: "AAA".into(),
        product: 1,
        serial: "LARGE00001".into(),
    });
    monitor.connector = Some("DP-8".into());
    let input = |class: Class, name: &str, port: &str| {
        let mut d = Identity::input().with_class(class);
        d.name = Some(name.into());
        d.path = Some(format!("pci-0000:15:00.0-usb-0:{port}"));
        d
    };
    Present {
        devices: vec![
            monitor,
            input(Class::Keyboard, "keyboard", "3.3:1.0"),
            input(Class::Mouse, "mouse", "3.4:1.0"),
            input(Class::Key, "Power Button", "3.5:1.0"),
        ],
        udev: true,
    }
}

/// One thing the person does: a key, or the terminal resized.
#[derive(Clone, Copy, Debug)]
pub enum Press {
    Key(Key),
    /// A mouse button or the wheel at a cell, counted from 0 at the top
    /// left.
    Mouse(Button, usize, usize),
    Resize(usize, usize),
}

/// What one key did, beyond the state it left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Did {
    /// The effects the key caused, the answers' effects included, each as
    /// a short word: `save`, `check`, `install`, `switch`, `exit 0`.
    pub effects: Vec<String>,
    /// The font switches written.
    pub switches: usize,
    /// The TUI exited, with this status.
    pub exit: Option<u8>,
}

/// The TUI with its stand-ins.
#[derive(Clone, Debug)]
pub struct Sim {
    pub app: App,
    pub term: Term,
    pub desk: Desk,
    /// The font the stand-in console draws.
    pub console_font: String,
    /// Mouse reporting as the terminal has it from the bytes written.
    pub mouse: bool,
    /// The TUI exited, with this status.
    pub exited: Option<u8>,
}

/// The most effects one key may cause before the scenario calls it a
/// loop.
const CASCADE_MAX: usize = 256;

impl Sim {
    /// The TUI at its start in `term`, at `cols` by `rows`, on the
    /// fixture, with a desk whose draft is the fixture's vtium.conf.
    pub fn start(term: Term, cols: usize, rows: usize) -> Sim {
        let mut inputs = vtium_config::tui::fixture::inputs();
        inputs.desk = true;
        // comments after the fixture's settings make the file longer than
        // the file view's window, so its scrolling keys have lines to move
        let mut conf = inputs.conf.clone().unwrap_or_default();
        for n in 1..=LONG_FILE_COMMENTS {
            conf.push_str(&format!("# comment {n} of the stand-in desk's draft\n"));
        }
        inputs.conf = Some(conf.clone());
        let (app, first) = App::new(inputs, cols, rows);
        let mut sim = Sim {
            app,
            term,
            desk: Desk {
                installed: conf.clone(),
                draft: conf.clone(),
                // two installed files, so Restore's list has an item to
                // move to
                history: vec![
                    format!("{conf}# an older file\n"),
                    format!("{conf}# the oldest file\n"),
                ],
                writes: 0,
                no_route: false,
            },
            console_font: START_FONT.to_owned(),
            mouse: false,
            exited: None,
        };
        let mut did = Did::default();
        sim.answer(first, &mut did);
        let replies = match term {
            Term::Vtium => vec![
                Reply::Version("vtium(0.1.0)".into()),
                Reply::Font(START_FONT.into()),
            ],
            Term::OpsOff => vec![Reply::Version("vtium(0.1.0)".into())],
            Term::Other => vec![Reply::Version("XTerm(390)".into())],
        };
        for reply in replies.into_iter().chain([Reply::Attributes]) {
            sim.event(Event::Input(Input::Reply(reply)), &mut did);
        }
        sim
    }

    /// Does `press` and answers everything it causes.
    pub fn press(&mut self, press: Press) -> Did {
        let mut did = Did::default();
        if self.exited.is_some() {
            return did;
        }
        let event = match press {
            Press::Key(key) => Event::Input(Input::Key(key)),
            Press::Mouse(button, col, row) => {
                Event::Input(Input::Key(Key::Mouse(Mouse { button, col, row })))
            }
            Press::Resize(cols, rows) => Event::Resize { cols, rows },
        };
        self.event(event, &mut did);
        // the wait for the console's reply ends once the TUI is leaving
        if self.exited.is_none() && self.app.leaving.is_some() {
            self.event(Event::Timeout, &mut did);
        }
        did
    }

    fn event(&mut self, event: Event, did: &mut Did) {
        let out = self.app.event(event);
        self.answer(out, did);
    }

    /// Answers `effects`, and the effects the answers cause, in order.
    fn answer(&mut self, effects: Vec<Effect>, did: &mut Did) {
        let mut queue: std::collections::VecDeque<Effect> = effects.into();
        let mut n = 0usize;
        while let Some(effect) = queue.pop_front() {
            n = n.saturating_add(1);
            assert!(n < CASCADE_MAX, "one key caused {n} effects: {did:?}");
            if self.exited.is_some() {
                break;
            }
            let event = match effect {
                Effect::Write(bytes) => {
                    for reply in self.terminal(&bytes, did) {
                        queue.extend(self.app.event(Event::Input(Input::Reply(reply))));
                    }
                    continue;
                }
                Effect::Wait(_) => continue,
                Effect::Exit(status) => {
                    did.effects.push(format!("exit {status}"));
                    did.exit = Some(status);
                    self.exited = Some(status);
                    continue;
                }
                Effect::Install(plan) => {
                    did.effects.push("install".into());
                    let _ = plan;
                    self.desk.installed.clone_from(&self.desk.draft);
                    Event::Installed(Ok(self.desk.draft()))
                }
                Effect::Desk(op) => {
                    did.effects.push(op_word(&op).into());
                    let done = self.desk.run(&op);
                    Event::Desked(op, done)
                }
                Effect::Specimen { font, height, .. } => Event::Detailed {
                    font,
                    height,
                    result: Err("no helper in the scenarios".into()),
                },
                Effect::Cell { font, height } => Event::Celled {
                    font,
                    height,
                    cell: Err("no helper in the scenarios".into()),
                },
                Effect::Picture(_) => Event::Pictured(Err("no helper in the scenarios".into())),
                Effect::Summary { font, height } => Event::Summarized {
                    font,
                    height,
                    summary: Err("no helper in the scenarios".into()),
                },
                Effect::Sources(_) => Event::Sourced(Vec::new()),
            };
            queue.extend(self.app.event(event));
        }
    }

    /// The stand-in terminal reads `bytes`: it follows mouse reporting,
    /// and in a vtium console that switches fonts, it takes each OSC 50
    /// switch and answers each OSC 50 query with the font it draws.
    fn terminal(&mut self, bytes: &[u8], did: &mut Did) -> Vec<Reply> {
        let text = String::from_utf8_lossy(bytes);
        if text.contains("\x1b[?1000h") {
            self.mouse = true;
            did.effects.push("mouse on".into());
        }
        if text.contains("\x1b[?1000l") {
            self.mouse = false;
            did.effects.push("mouse off".into());
        }
        let mut replies = Vec::new();
        for osc in text.split("\x1b]50;").skip(1) {
            let Some((name, _)) = osc.split_once("\x1b\\") else {
                continue;
            };
            if name == "?" {
                if self.term == Term::Vtium {
                    replies.push(Reply::Font(self.console_font.clone()));
                }
                continue;
            }
            did.switches = did.switches.saturating_add(1);
            did.effects.push("switch".into());
            if self.term == Term::Vtium && name != "broken" {
                self.console_font = name.to_owned();
            }
        }
        replies
    }

    /// The screen as text, one line per row.
    pub fn screen(&self) -> Vec<String> {
        vtium_config::tui::view::screen(&self.app)
            .iter()
            .map(vtium_config::tui::view::text)
            .collect()
    }
}

fn op_word(op: &Op) -> &'static str {
    match op {
        Op::Open => "open",
        Op::Write(_) => "save",
        Op::WriteChecked(_) => "save checked",
        Op::Check(_) => "check",
        Op::Prepare => "prepare",
        Op::Revert => "revert",
        Op::New => "new",
        Op::Delete => "delete",
        Op::History => "history",
        Op::Restore(_) => "restore",
        Op::Devices => "devices",
    }
}
