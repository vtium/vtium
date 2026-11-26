//! The use cases: one file for each task a person does in vtium-config,
//! under `cases/`, run from its first key to its end. The README beside
//! this module says how a case file reads.
//!
//! Each case runs the TUI's state machine on the real desk, vtium-config's
//! own store, over a machine of the case's own: a temporary folder that
//! stands in for `/etc/vtium`, the frits' directory, the font files and
//! the person's home. Save and Install run the same code as they do for a
//! person. The install's privileged step runs this crate's binary as
//! `apply DIR`, as the user, into the temporary folder in place of
//! `/etc/vtium`, so no case needs sudo or touches the machine's own files.
//! The font helper is this crate's binary too, run as `serve`. On the
//! other side of the TUI is the stand-in console of `console.rs`, which
//! refuses what vtium refuses.
//!
//! A case whose checks fail today because a known defect is still in
//! vtium-config names the defect on a `waits-for` line. The runner then
//! expects the case to fail and reports it as expected-red, and the test
//! fails once the case passes, so the line comes out with the fix.

mod console;

use std::path::PathBuf;

use vtium_config::tui::desk::{Desk, Store};
use vtium_config::tui::input::{Input, Key};
use vtium_config::tui::model::{App, Effect, Event, View};

use console::{Console, Drawn, Places};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// The folder of the case files.
fn cases_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios/cases")
}

/// One case as its file reads.
#[derive(Debug, Default)]
struct Case {
    name: String,
    title: String,
    /// The defect the case waits for, which makes it fail today.
    waits: Option<String>,
    grid: (usize, usize),
    view: Option<View>,
    head: String,
    /// Files of the machine, by their path under the root, `ROOT` in
    /// their text being the root's path.
    files: Vec<(String, String)>,
    /// Bitmap fonts the runner writes, by path, with their cell.
    fonts: Vec<(String, (u32, u32))>,
    /// The draft's text, or `None` for a draft made from the installed
    /// file.
    draft: Option<String>,
    steps: Vec<(usize, Step)>,
}

#[derive(Debug, Clone)]
enum Step {
    Press(Vec<String>),
    Type(String),
    /// Ticks of the live trial's countdown.
    Wait(usize),
    /// The person restarts vtium on the console's head.
    Restart,
    /// The person runs vtium-config on the command line with these
    /// words after the machine's options, such as `install`.
    Command(Vec<String>),
    /// A name for the checks that follow, which their failures carry.
    At(String),
    Check(Check),
}

#[derive(Debug, Clone)]
enum Check {
    Screen {
        shows: bool,
        text: String,
    },
    /// What the install's privileged step and the command lines printed.
    Said {
        shows: bool,
        text: String,
    },
    /// A text in a file: the draft or the installed vtium.conf.
    Text {
        file: Which,
        has: bool,
        text: String,
    },
    /// The value vtium takes for a head's key in a file, or the value a
    /// look's section sets.
    Value {
        file: Which,
        section: Section,
        key: String,
        value: String,
    },
    Console {
        what: String,
        value: String,
    },
    Received {
        has: bool,
        text: String,
    },
    Exited(String),
}

#[derive(Debug, Clone)]
enum Section {
    Head(String),
    Look(String),
}

#[derive(Debug, Clone, Copy)]
enum Which {
    /// The draft's file, as Save last wrote it.
    Draft,
    /// The installed vtium.conf.
    Installed,
}

/// The text between the first and the last double quote of `s`, with
/// `\e` read as ESC.
fn quoted(s: &str) -> Result<String, String> {
    let a = s.find('"').ok_or("a quoted text")?;
    let b = s.rfind('"').filter(|b| *b > a).ok_or("a closing quote")?;
    Ok(s[a + 1..b].replace("\\e", "\x1b"))
}

fn parse(name: &str, text: &str) -> Result<Case, String> {
    let mut case = Case {
        name: name.to_owned(),
        grid: (100, 30),
        head: "console".to_owned(),
        ..Case::default()
    };
    // the block an indented line belongs to
    enum Block {
        None,
        File(usize),
        Draft,
    }
    let mut block = Block::None;
    for (n, line) in text.lines().enumerate() {
        let n = n + 1;
        let fail = |why: &str| format!("{name}:{n}: {why}");
        if let Some(body) = line.strip_prefix("    ") {
            match block {
                Block::File(i) => {
                    case.files[i].1.push_str(body);
                    case.files[i].1.push('\n');
                }
                Block::Draft => {
                    let d = case.draft.get_or_insert_with(String::new);
                    d.push_str(body);
                    d.push('\n');
                }
                Block::None => return Err(fail("an indented line outside a file")),
            }
            continue;
        }
        block = Block::None;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        match verb {
            "title" => case.title = rest.to_owned(),
            "waits-for" => case.waits = Some(rest.to_owned()),
            "terminal" => {
                let (c, r) = rest
                    .split_once('x')
                    .ok_or_else(|| fail("terminal COLSxROWS"))?;
                case.grid = (
                    c.parse().map_err(|_| fail("columns"))?,
                    r.parse().map_err(|_| fail("rows"))?,
                );
            }
            "head" => case.head = rest.to_owned(),
            "view" => {
                case.view = Some(match rest {
                    "flow" => View::Flow,
                    "effects" => View::Effects,
                    "looks" => View::Looks,
                    "fonts" => View::Fonts,
                    "settings" => View::Settings,
                    "help" => View::Help,
                    _ => return Err(fail("an unknown view")),
                })
            }
            "file" => {
                case.files.push((rest.to_owned(), String::new()));
                block = Block::File(case.files.len() - 1);
            }
            "frit" => {
                case.files
                    .push((format!("etc/fonts/{rest}.vtfrit"), String::new()));
                block = Block::File(case.files.len() - 1);
            }
            "font" => {
                let (path, cell) = rest.split_once(' ').ok_or_else(|| fail("font PATH WxH"))?;
                let (w, h) = cell.split_once('x').ok_or_else(|| fail("a cell WxH"))?;
                case.fonts.push((
                    path.to_owned(),
                    (
                        w.parse().map_err(|_| fail("width"))?,
                        h.parse().map_err(|_| fail("height"))?,
                    ),
                ));
            }
            "draft" => {
                if rest != "from-installed" {
                    case.draft = Some(String::new());
                    block = Block::Draft;
                }
            }
            "press" => case.steps.push((
                n,
                Step::Press(rest.split_whitespace().map(str::to_owned).collect()),
            )),
            "type" => case
                .steps
                .push((n, Step::Type(quoted(rest).map_err(|e| fail(&e))?))),
            "wait" => case
                .steps
                .push((n, Step::Wait(rest.parse().map_err(|_| fail("wait TICKS"))?))),
            "restart" => case.steps.push((n, Step::Restart)),
            "command" => case.steps.push((
                n,
                Step::Command(rest.split_whitespace().map(str::to_owned).collect()),
            )),
            "at" => case.steps.push((n, Step::At(rest.to_owned()))),
            "check" => {
                let check = parse_check(rest).map_err(|e| fail(&e))?;
                case.steps.push((n, Step::Check(check)));
            }
            _ => return Err(fail(&format!("an unknown line {verb}"))),
        }
    }
    if case.title.is_empty() {
        return Err(format!("{name}: a case needs a title line"));
    }
    Ok(case)
}

fn parse_check(rest: &str) -> Result<Check, String> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    let file = |w: &str| match w {
        "draft" => Ok(Which::Draft),
        "installed" => Ok(Which::Installed),
        _ => Err(format!("{w} is neither draft nor installed")),
    };
    Ok(match words.as_slice() {
        ["screen", "shows", ..] => Check::Screen {
            shows: true,
            text: quoted(rest)?,
        },
        ["screen", "lacks", ..] => Check::Screen {
            shows: false,
            text: quoted(rest)?,
        },
        ["said", "shows", ..] => Check::Said {
            shows: true,
            text: quoted(rest)?,
        },
        ["said", "lacks", ..] => Check::Said {
            shows: false,
            text: quoted(rest)?,
        },
        [f, "has", ..] => Check::Text {
            file: file(f)?,
            has: true,
            text: quoted(rest)?,
        },
        [f, "lacks", ..] => Check::Text {
            file: file(f)?,
            has: false,
            text: quoted(rest)?,
        },
        [f, kind @ ("head" | "look"), name, key, value] => Check::Value {
            file: file(f)?,
            section: if *kind == "head" {
                Section::Head((*name).to_owned())
            } else {
                Section::Look((*name).to_owned())
            },
            key: (*key).to_owned(),
            value: (*value).to_owned(),
        },
        ["console", "received", ..] => Check::Received {
            has: true,
            text: quoted(rest)?,
        },
        ["console", "never-received", ..] => Check::Received {
            has: false,
            text: quoted(rest)?,
        },
        [
            "console",
            what @ ("font" | "draws" | "fell-back" | "palette" | "cell" | "grid"),
            value,
        ] => Check::Console {
            what: (*what).to_owned(),
            value: (*value).to_owned(),
        },
        ["exited", status] => Check::Exited((*status).to_owned()),
        _ => return Err(format!("an unknown check {rest}")),
    })
}

/// A key of a `press` line by its name: a named key, or one character.
fn key(name: &str) -> Result<Key, String> {
    Ok(match name {
        "Up" => Key::Up,
        "Down" => Key::Down,
        "Left" => Key::Left,
        "Right" => Key::Right,
        "PgUp" => Key::PageUp,
        "PgDn" => Key::PageDown,
        "Home" => Key::Home,
        "End" => Key::End,
        "Enter" => Key::Enter,
        "Tab" => Key::Tab,
        "Shift-Tab" => Key::BackTab,
        "Backspace" => Key::Backspace,
        "Esc" => Key::Escape,
        "Ctrl-C" => Key::Interrupt,
        "Space" => Key::Char(' '),
        _ => {
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c),
                _ => return Err(format!("{name} is not a key")),
            }
        }
    })
}

/// A bitmap font in BDF of `w` by `h` pixels whose printable ASCII
/// characters are each a box, so every glyph has ink.
fn bdf((w, h): (u32, u32)) -> String {
    // each row is whole bytes, the glyph's pixels in its high bits
    let bits = w.div_ceil(8) * 8;
    let digits = (bits / 4) as usize;
    let pixel = |x: u32| 1u64 << (bits - 1 - x);
    let full = format!("{:0digits$X}", (0..w).map(pixel).sum::<u64>());
    let edge = format!("{:0digits$X}", pixel(0) | pixel(w - 1));
    let mut out = format!(
        "STARTFONT 2.1\nFONT -vtium-usecase-medium-r-normal--{h}-{h}0-75-75-c-{w}0-iso10646-1\nSIZE {h} 75 75\nFONTBOUNDINGBOX {w} {h} 0 -2\nSTARTPROPERTIES 2\nFONT_ASCENT {}\nFONT_DESCENT 2\nENDPROPERTIES\nCHARS 95\n",
        h - 2
    );
    for c in 32u32..127 {
        out.push_str(&format!(
            "STARTCHAR U+{c:04X}\nENCODING {c}\nSWIDTH 500 0\nDWIDTH {w} 0\nBBX {w} {h} 0 -2\nBITMAP\n"
        ));
        for row in 0..h {
            out.push_str(if row == 0 || row == h - 1 {
                &full
            } else {
                &edge
            });
            out.push('\n');
        }
        out.push_str("ENDCHAR\n");
    }
    out.push_str("ENDFONT\n");
    out
}

/// The machine a case runs on: the folder that stands in for the system's
/// and the person's files.
struct Machine {
    root: PathBuf,
}

impl Machine {
    fn new(case: &Case) -> Machine {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join("usecases")
            .join(&case.name);
        if root.exists() {
            std::fs::remove_dir_all(&root).expect("a fresh machine");
        }
        for dir in ["etc/fonts", "fonts", "home", "sys", "udev/data"] {
            std::fs::create_dir_all(root.join(dir)).expect("the machine");
        }
        let m = Machine { root };
        for (path, text) in &case.files {
            m.write(path, &text.replace("ROOT", &m.root.to_string_lossy()));
        }
        for (path, cell) in &case.fonts {
            m.write(path, &bdf(*cell));
        }
        vtium_testscript::executable(&m.root.join("vtium"), vtium_config::tui::fixture::CHECKER);
        m
    }

    /// Writes `text` at `path` under the root, readable by all and
    /// writable by its owner alone, as the trust rule asks of an active
    /// font.
    fn write(&self, path: &str, text: &str) {
        use std::os::unix::fs::PermissionsExt;
        let at = self.root.join(path);
        if let Some(dir) = at.parent() {
            std::fs::create_dir_all(dir).expect("the file's folder");
        }
        std::fs::write(&at, text).expect("the machine's file");
        std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o644)).expect("its mode");
    }

    fn var(&self, name: &str) -> Option<PathBuf> {
        match name {
            "HOME" => Some(self.root.join("home")),
            "XDG_CONFIG_HOME" => Some(self.root.join("home/config")),
            "XDG_STATE_HOME" => Some(self.root.join("home/state")),
            _ => None,
        }
    }

    fn store(&self) -> vtium_config::store::Store {
        let euid = rustix::process::geteuid().as_raw();
        vtium_config::store::Store::locate(euid, |n| self.var(n).map(Into::into), |_| None)
            .expect("the store")
    }

    fn helper(&self) -> vtium_config::specimen::Helper {
        vtium_config::specimen::Helper {
            program: BIN.into(),
            options: vec!["--font-dir".into(), self.root.join("fonts").into()],
        }
    }

    fn paths(&self) -> vtium_config::tui::Paths {
        vtium_config::tui::Paths {
            configdir: self.root.join("etc"),
            frits: Ok(self.root.join("etc/fonts")),
            drafts: self.root.join("home/bundles"),
            sudo: "sudo".into(),
            vtium: self.root.join("vtium").into(),
            helper: self.helper(),
            sys: self.root.join("sys"),
        }
    }

    /// `vtium-config` run on the command line, as the person runs it,
    /// with the machine's folders in place of the system's: what it
    /// printed.
    fn command(&self, words: &[String]) -> String {
        let mut command = std::process::Command::new(BIN);
        command
            .arg("--configdir")
            .arg(self.root.join("etc"))
            .arg("--frits")
            .arg(self.root.join("etc/fonts"))
            .arg("--vtium")
            .arg(self.root.join("vtium"))
            .arg("--sys")
            .arg(self.root.join("sys"))
            .arg("--udev-data")
            .arg(self.root.join("udev/data"))
            .args(
                words
                    .iter()
                    .map(|w| w.replace("ROOT", &self.root.to_string_lossy())),
            )
            .stdin(std::process::Stdio::null());
        for name in ["HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME"] {
            command.env(name, self.var(name).expect("the machine's home"));
        }
        let out = command.output().expect("vtium-config runs");
        let mut said = String::from_utf8_lossy(&out.stdout).into_owned();
        said.push_str(&String::from_utf8_lossy(&out.stderr));
        said
    }

    fn installed(&self) -> String {
        std::fs::read_to_string(self.root.join("etc/vtium.conf")).unwrap_or_default()
    }

    fn draft(&self) -> String {
        let store = self.store();
        store
            .settings()
            .ok()
            .and_then(|s| s.draft)
            .and_then(|name| store.read_draft(&name).ok().flatten())
            .unwrap_or_default()
    }
}

/// A case running: the TUI, its desk and the console.
struct Run<'a> {
    machine: &'a Machine,
    app: App,
    desk: Store,
    console: Console,
    exited: Option<u8>,
    /// What the privileged step and the step after it printed.
    said: Vec<u8>,
}

/// The most effects one key may cause before the runner calls it a loop.
const CASCADE_MAX: usize = 512;

impl Run<'_> {
    fn press(&mut self, key: Key) {
        if self.exited.is_some() {
            return;
        }
        let out = self.app.event(Event::Input(Input::Key(key)));
        self.answer(out);
        if self.exited.is_none() && self.app.shared.leaving.is_some() {
            let out = self.app.event(Event::Timeout);
            self.answer(out);
        }
    }

    fn tick(&mut self) {
        if self.exited.is_none() {
            let out = self.app.event(Event::Timeout);
            self.answer(out);
        }
    }

    /// Answers `effects`, and the effects the answers cause, in order, as
    /// the TUI's loop in `port.rs` answers them.
    fn answer(&mut self, effects: Vec<Effect>) {
        let mut queue: std::collections::VecDeque<Effect> = effects.into();
        let mut n = 0usize;
        let frits = self.machine.root.join("etc/fonts");
        let helper = self.machine.helper();
        while let Some(effect) = queue.pop_front() {
            n += 1;
            assert!(n < CASCADE_MAX, "one key caused {n} effects");
            if self.exited.is_some() {
                break;
            }
            let event = match effect {
                Effect::Write(bytes) => {
                    for reply in self.console.read(&bytes) {
                        queue.extend(self.app.event(Event::Input(Input::Reply(reply))));
                    }
                    continue;
                }
                Effect::Wait(_) => continue,
                Effect::Exit(status) => {
                    self.exited = Some(status);
                    continue;
                }
                Effect::Install(plan) => {
                    let result =
                        self.privileged(|desk, run, out, ask| desk.install(&plan, run, out, ask));
                    let size = self.app.size;
                    queue.extend(self.app.event(Event::Resize(size)));
                    Event::Installed(result)
                }
                Effect::InstallFont => {
                    let result = self.privileged(|desk, run, _, _| desk.install_font(run));
                    Event::Installed(result)
                }
                Effect::Desk(op) => {
                    let done = self.desk.run(&op);
                    Event::Desked(op, done)
                }
                Effect::Cell { font, height } => {
                    let cell =
                        vtium_config::tui::measurer(frits.clone(), helper.clone())(&font, height);
                    Event::Celled { font, height, cell }
                }
                Effect::Sources(names) => {
                    Event::Sourced(vtium_config::tui::find_sources(&frits, &helper, &names))
                }
                Effect::Describe(paths) => {
                    let described = vtium_config::discover::describe(&helper, &paths);
                    Event::Described(paths.into_iter().zip(described).collect())
                }
                Effect::Specimen { font, height, .. } => Event::Detailed {
                    font,
                    height,
                    result: Err("the use cases draw no pictures".into()),
                },
                Effect::Picture(_) | Effect::ScreenPicture(_) => {
                    Event::Pictured(Err("the use cases draw no pictures".into()))
                }
                Effect::Summary { font, height } => Event::Summarized {
                    font,
                    height,
                    summary: Err("the use cases draw no pictures".into()),
                },
            };
            queue.extend(self.app.event(event));
        }
    }

    /// Runs the privileged step: each command it hands the runner is run
    /// as this crate's own binary, as the user, with the machine's home,
    /// so `apply DIR` installs into the machine's folder. The step after
    /// the install asks whether to start or restart vtium, and the person
    /// answers no; a case restarts the console with a `restart` line.
    fn privileged(
        &mut self,
        step: impl FnOnce(
            &mut Store,
            vtium_config::apply::Runner<'_>,
            &mut dyn std::io::Write,
            vtium_config::apply::Ask<'_>,
        ) -> Result<vtium_config::tui::desk::Done, String>,
    ) -> Result<vtium_config::tui::desk::Done, String> {
        let machine = self.machine;
        let mut run = |argv: &[std::ffi::OsString]| -> std::io::Result<(bool, String)> {
            let (_, args) = argv.split_first().expect("a command");
            let mut command = std::process::Command::new(BIN);
            command.args(args);
            for name in ["HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME"] {
                command.env(name, machine.var(name).expect("the machine's home"));
            }
            let out = command.output()?;
            let mut said = String::from_utf8_lossy(&out.stdout).into_owned();
            said.push_str(&String::from_utf8_lossy(&out.stderr));
            Ok((out.status.success(), said))
        };
        let mut ask = |out: &mut dyn std::io::Write, question: &str| -> bool {
            let _ = writeln!(out, "{question} [y/N]");
            false
        };
        let mut said = Vec::new();
        let result = step(&mut self.desk, &mut run, &mut said, &mut ask);
        self.said.extend(said);
        result
    }

    fn screen(&self) -> String {
        vtium_config::tui::view::screen(&self.app)
            .iter()
            .map(vtium_config::tui::view::text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Whether `check` holds, or what was found instead.
    fn check(&self, check: &Check) -> Result<(), String> {
        let file = |which: Which| match which {
            Which::Draft => ("the saved draft", self.machine.draft()),
            Which::Installed => ("the installed vtium.conf", self.machine.installed()),
        };
        match check {
            Check::Screen { shows, text } => {
                let screen = self.screen();
                let flat = screen.split_whitespace().collect::<Vec<_>>().join(" ");
                // a tab with a column beside its list wraps the column's
                // text between the rows of the other, so the column's own
                // words are read on their own too
                let left = screen
                    .lines()
                    .filter_map(|row| row.split("   ").find(|s| !s.trim().is_empty()))
                    .flat_map(str::split_whitespace)
                    .collect::<Vec<_>>()
                    .join(" ");
                let found = flat.contains(text.as_str()) || left.contains(text.as_str());
                if found != *shows {
                    let verb = if *shows { "does not show" } else { "shows" };
                    return Err(format!("the screen {verb} {text:?}:\n{screen}"));
                }
            }
            Check::Said { shows, text } => {
                let said = String::from_utf8_lossy(&self.said);
                let flat = said.split_whitespace().collect::<Vec<_>>().join(" ");
                if flat.contains(text.as_str()) != *shows {
                    let verb = if *shows { "never said" } else { "said" };
                    return Err(format!("vtium-config {verb} {text:?}:\n{said}"));
                }
            }
            Check::Text {
                file: which,
                has,
                text,
            } => {
                let (what, body) = file(*which);
                if body.contains(text.as_str()) != *has {
                    let verb = if *has { "lacks" } else { "has" };
                    return Err(format!("{what} {verb} {text:?}:\n{body}"));
                }
            }
            Check::Value {
                file: which,
                section,
                key,
                value,
            } => {
                let (what, body) = file(*which);
                let (got, whose) = match section {
                    // a head names its look in its own section, and takes
                    // every other key through the levels vtium reads
                    Section::Head(head) if key == "look" => (
                        section_value(&body, &format!("[head {head}]"), key),
                        format!("the head {head}"),
                    ),
                    Section::Head(head) => (
                        vtium_settings::value_for(&body, head, key).map(|(v, _)| v),
                        format!("the head {head}"),
                    ),
                    Section::Look(look) => (
                        section_value(&body, &format!("[look {look}]"), key),
                        format!("the look {look}"),
                    ),
                };
                if got.as_deref() != Some(value.as_str()) {
                    return Err(format!(
                        "in {what}, {whose} has {key} = {got:?}, not {value}:\n{body}"
                    ));
                }
            }
            Check::Console { what, value } => {
                let c = &self.console;
                let got = match what.as_str() {
                    "font" => c.font.clone(),
                    "draws" => match &c.drawn {
                        Drawn::Set(name) => name.clone(),
                        Drawn::BuiltIn(_) => "built-in".to_owned(),
                    },
                    "fell-back" => if matches!(c.drawn, Drawn::BuiltIn(_)) {
                        "yes"
                    } else {
                        "no"
                    }
                    .to_owned(),
                    "palette" => c.palette(),
                    "cell" => format!("{}x{}", c.cell.width, c.cell.height),
                    _ => format!("{}x{}", c.grid.0, c.grid.1),
                };
                if got != *value {
                    return Err(format!(
                        "the console's {what} is {got}, not {value} (it draws {:?}; its log: {:?})",
                        c.drawn, c.log
                    ));
                }
            }
            Check::Received { has, text } => {
                let got = String::from_utf8_lossy(&self.console.received);
                if got.contains(text.as_str()) != *has {
                    let verb = if *has { "never received" } else { "received" };
                    let osc: Vec<&str> = got.split('\x1b').filter(|s| s.starts_with(']')).collect();
                    return Err(format!(
                        "the console {verb} {:?}; the OSC strings it received: {osc:?}",
                        text.replace('\x1b', "\\e")
                    ));
                }
            }
            Check::Exited(status) => {
                let got = self
                    .exited
                    .map_or_else(|| "no".to_owned(), |s| s.to_string());
                if got != *status {
                    return Err(format!("vtium-config exited {got}, not {status}"));
                }
            }
        }
        Ok(())
    }
}

/// The value that the section of `text` headed `heading` sets for `key`.
fn section_value(text: &str, heading: &str, key: &str) -> Option<String> {
    let body: String = text
        .lines()
        .skip_while(|l| l.trim() != heading)
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with('['))
        .map(|l| format!("{l}\n"))
        .collect();
    vtium_settings::setting(&body, key)
}

/// Runs `case` and gives the failures of its checks, each with its line
/// and the name of its point.
fn run(case: &Case) -> Vec<String> {
    let machine = Machine::new(case);
    let paths = machine.paths();
    let store = machine.store();
    let session = vtium_config::drafts::Session {
        store: store.clone(),
        paths: &paths,
    };
    let name = session
        .make("mine", vtium_config::drafts::From::Installed)
        .expect("the draft mine");
    if let Some(text) = &case.draft {
        store
            .write_draft(
                &name,
                &text.replace("ROOT", &machine.root.to_string_lossy()),
            )
            .expect("the draft's text");
    }
    let frits = machine.root.join("etc/fonts");
    let mut inputs = vtium_config::tui::fixture::inputs();
    inputs.conf_path = machine.root.join("etc/vtium.conf");
    inputs.conf = vtium_config::tui::read_conf(&inputs.conf_path).expect("vtium.conf");
    inputs.fonts = vtium_config::tui::fonts(&frits);
    inputs.installed = vtium_config::tui::installed(&frits);
    inputs.layers = vtium_config::tui::layers(&frits);
    inputs.frits = frits.clone();
    inputs.flow = vtium_config::tui::flow::Flow::default();
    inputs.desk = true;
    inputs.head = Some(case.head.clone());
    inputs.view = case.view.unwrap_or(View::Flow);
    inputs.effects = Default::default();
    inputs.font_files = Default::default();
    let (cols, rows) = case.grid;
    // a display whose grid in the built-in font's 8×16 cell is the case's
    inputs.display = Some((
        u32::try_from(cols * 8).unwrap_or(800),
        u32::try_from(rows * 16).unwrap_or(480),
    ));
    let console = Console::start(
        Places {
            frits,
            conf: machine.root.join("etc/vtium.conf"),
            batch: machine.root.join("etc/batch.conf"),
            font_dirs: vec![machine.root.join("fonts")],
        },
        &case.head,
        inputs.dpi.0,
        case.grid,
    );
    let (app, first) = App::new(inputs, cols, rows);
    let mut run = Run {
        machine: &machine,
        app,
        desk: Store::new(store, paths.clone(), Box::new(Default::default)),
        console,
        exited: None,
        said: Vec::new(),
    };
    run.answer(first);
    let show = std::env::var_os("VTIUM_USECASE_SHOW").is_some();
    let mut at = "the start".to_owned();
    let mut failures = Vec::new();
    for (line, step) in &case.steps {
        match step {
            Step::Press(keys) => {
                for k in keys {
                    match key(k) {
                        Ok(k) => run.press(k),
                        Err(why) => failures.push(format!("{}:{line}: {why}", case.name)),
                    }
                }
            }
            Step::Type(text) => {
                for c in text.chars() {
                    run.press(Key::Char(c));
                }
            }
            Step::Wait(n) => (0..*n).for_each(|_| run.tick()),
            Step::Restart => run.console.restart(),
            Step::Command(words) => {
                let said = machine.command(words);
                run.said.extend(said.into_bytes());
            }
            Step::At(name) => at.clone_from(name),
            Step::Check(check) => {
                if let Err(why) = run.check(check) {
                    failures.push(format!("{}:{line}, at {at}: {why}", case.name));
                }
            }
        }
        if show && !matches!(step, Step::Check(_) | Step::At(_)) {
            eprintln!(
                "--- {}:{line} {step:?}\n{}\nconsole: font {} drawn {:?} cell {}x{} palette {} grid {:?}",
                case.name,
                run.screen(),
                run.console.font,
                run.console.drawn,
                run.console.cell.width,
                run.console.cell.height,
                run.console.palette(),
                run.console.grid
            );
        }
    }
    failures
}

/// The cases under `cases/`, by name, or the one `VTIUM_USECASE` names.
fn cases() -> Vec<Case> {
    let only = std::env::var("VTIUM_USECASE").ok();
    let mut files: Vec<PathBuf> = std::fs::read_dir(cases_dir())
        .expect("the cases' folder")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "case"))
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|p| {
            let name = p.file_stem()?.to_string_lossy().into_owned();
            if only.as_ref().is_some_and(|o| *o != name) {
                return None;
            }
            let text = std::fs::read_to_string(p).expect("a case file");
            Some(parse(&name, &text).unwrap_or_else(|e| panic!("{e}")))
        })
        .collect()
}

/// Every use case: a case with no `waits-for` line passes all its checks,
/// and a case with one fails at least one, until the defect it names is
/// fixed. The test prints each case's outcome.
#[test]
fn every_use_case_does_what_the_person_asked() {
    let mut wrong = Vec::new();
    let mut table = Vec::new();
    for case in cases() {
        let failures = run(&case);
        let outcome = match (&case.waits, failures.is_empty()) {
            (None, true) => "green".to_owned(),
            (Some(w), false) => format!("expected-red, waits for {w}"),
            (None, false) => {
                wrong.push(format!(
                    "{} ({}) fails:\n{}",
                    case.name,
                    case.title,
                    failures.join("\n")
                ));
                "RED".to_owned()
            }
            (Some(w), true) => {
                wrong.push(format!(
                    "{} passes now, so the defect it waits for is fixed: take out its line waits-for {w}",
                    case.name
                ));
                "GREEN, but it waits for a fix".to_owned()
            }
        };
        table.push(format!("{}: {outcome}", case.name));
        // an expected-red case lists the first line of each failure, so
        // the report shows what still waits
        if case.waits.is_some() {
            for f in &failures {
                table.push(format!("    {}", f.lines().next().unwrap_or_default()));
            }
        }
    }
    println!("{}", table.join("\n"));
    assert!(wrong.is_empty(), "{}", wrong.join("\n\n"));
}

#[test]
fn the_case_files_read() {
    let cases = cases();
    assert!(!cases.is_empty());
}
