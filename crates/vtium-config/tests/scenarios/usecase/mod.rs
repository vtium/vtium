//! The use cases: one file for each task a person does in vtium-config,
//! under `cases/`, in one folder for each type of task. Each case is a
//! how-to in the person's words, run from its first step to its end, and
//! `steps.rs` follows each step through what the screen shows. The README
//! beside this module says how a case file reads.
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
mod steps;

use std::path::PathBuf;

use vtium_config::tui::desk::{Desk, Store};
use vtium_config::tui::input::{Input, Key};
use vtium_config::tui::model::{App, Effect, Event, View};

use console::{Console, Drawn, Places};
use steps::Step;

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// The folder of the case files.
fn cases_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios/cases")
}

/// One case as its file reads.
#[derive(Debug, Default)]
struct Case {
    /// The case's folder and file name, such as `looks/make-a-look`.
    name: String,
    /// The folder, the type of task the case is: `looks`, `fonts`.
    folder: String,
    kind: Kind,
    /// The task in the person's words: "How do I make a new look?".
    title: String,
    /// The defect the case waits for, which makes it fail today.
    waits: Option<String>,
    grid: (usize, usize),
    head: String,
    /// Files of the machine, by their path under the root, `ROOT` in
    /// their text being the root's path.
    files: Vec<(String, String)>,
    /// Bitmap fonts the runner writes, by path, with their cell.
    fonts: Vec<(String, (u32, u32))>,
    /// The displays and input devices connected to the machine.
    devices: Vec<Device>,
    /// The draft's text, or `None` for a draft made from the installed
    /// file.
    draft: Option<String>,
    steps: Vec<(usize, Line)>,
}

/// The three kinds of task a case can be.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// "How do I …?": the steps of a task.
    #[default]
    HowDoI,
    /// "I want to …": a goal, and the steps that reach it.
    IWantTo,
    /// "Check if …": a lookup that changes nothing, which the runner
    /// checks at the end.
    CheckIf,
}

/// A display or an input device a case connects.
#[derive(Debug, Clone)]
struct Device {
    /// A display's connector, or an input's name.
    name: String,
    identity: vtium_settings::Identity,
}

/// One line of a case's steps and checks, in order.
#[derive(Debug, Clone)]
enum Line {
    /// A numbered step, with its number and its sentence.
    Step(usize, String, Step),
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
    /// Where vtium puts a device under the file's rules: the head it
    /// reaches, or `roams`, `ignored` or `off`.
    Reaches {
        file: Which,
        device: String,
        place: String,
    },
    /// The draft, the installed files and the console are as they were
    /// at the start.
    Unchanged,
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
    /// The installed batch.conf, the list of active resources.
    Batch,
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
        folder: name.split('/').next().unwrap_or_default().to_owned(),
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
    let mut kind = None;
    let mut number = 0usize;
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
        // a numbered step: "3. Choose New look."
        if let Some((num, sentence)) = line.split_once(". ")
            && let Ok(num) = num.parse::<usize>()
        {
            number += 1;
            if num != number {
                return Err(fail(&format!("step {num} should be step {number}")));
            }
            let step = steps::parse(sentence.trim()).map_err(|e| fail(&e))?;
            case.steps
                .push((n, Line::Step(num, sentence.trim().to_owned(), step)));
            continue;
        }
        let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        match verb {
            "kind" => {
                kind = Some(match rest {
                    "how-do-i" => Kind::HowDoI,
                    "i-want-to" => Kind::IWantTo,
                    "check-if" => Kind::CheckIf,
                    _ => return Err(fail("kind how-do-i, i-want-to or check-if")),
                })
            }
            "title" => case.title = rest.to_owned(),
            // the help's prose, which the runner does not follow
            "intro" | "note" => {}
            "see" => see(rest).map_err(|e| fail(&e))?,
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
            "device" => case.devices.push(device(rest).map_err(|e| fail(&e))?),
            "draft" => {
                if rest != "from-installed" {
                    case.draft = Some(String::new());
                    block = Block::Draft;
                }
            }
            "at" => case.steps.push((n, Line::At(rest.to_owned()))),
            "check" => {
                let check = parse_check(rest).map_err(|e| fail(&e))?;
                case.steps.push((n, Line::Check(check)));
            }
            _ => return Err(fail(&format!("an unknown line {verb}"))),
        }
    }
    case.kind = kind.ok_or_else(|| format!("{name}: a case needs a kind line"))?;
    let opening = match case.kind {
        Kind::HowDoI => "How do I ",
        Kind::IWantTo => "I want to ",
        Kind::CheckIf => "Check if ",
    };
    if !case.title.starts_with(opening) {
        return Err(format!("{name}: the title of this kind starts {opening:?}"));
    }
    if number == 0 {
        return Err(format!("{name}: a case needs numbered steps"));
    }
    Ok(case)
}

/// Checks a `see` line's target: another case, by its path under
/// `cases/` without `.case`, or a page of the repository, by its path.
fn see(target: &str) -> Result<(), String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = if target.ends_with(".md") {
        manifest.join("../..").join(target)
    } else {
        cases_dir().join(format!("{target}.case"))
    };
    if path.exists() {
        Ok(())
    } else {
        Err(format!("see {target}: no such case or page"))
    }
}

/// A `device` line: `display CONNECTOR [MAKER:PRODUCT:SERIAL]`, or `keyboard VENDOR:PRODUCT NAME`
/// or `mouse VENDOR:PRODUCT NAME`.
fn device(rest: &str) -> Result<Device, String> {
    use vtium_settings::{Class, Identity};
    let (kind, rest) = rest.split_once(' ').ok_or("device KIND ...")?;
    if kind == "display" {
        let mut words = rest.split_whitespace();
        let connector = words.next().ok_or("device display CONNECTOR")?;
        let mut identity = Identity::display();
        identity.connector = Some(connector.to_owned());
        // the monitor's EDID: MAKER:PRODUCT:SERIAL, the product in hex
        if let Some(edid) = words.next() {
            let mut parts = edid.splitn(3, ':');
            let (Some(maker), Some(product), Some(serial)) =
                (parts.next(), parts.next(), parts.next())
            else {
                return Err("an EDID reads MAKER:PRODUCT:SERIAL".to_owned());
            };
            let product = product.trim_start_matches("0x");
            identity.edid = Some(vtium_settings::Edid {
                maker: maker.to_owned(),
                product: u16::from_str_radix(product, 16)
                    .map_err(|_| format!("{product} is not hex"))?,
                serial: serial.to_owned(),
            });
        }
        return Ok(Device {
            name: connector.to_owned(),
            identity,
        });
    }
    let class = match kind {
        "keyboard" => Class::Keyboard,
        "mouse" => Class::Mouse,
        _ => return Err(format!("{kind} is not display, keyboard or mouse")),
    };
    let (id, name) = rest
        .split_once(' ')
        .ok_or("device KIND VENDOR:PRODUCT NAME")?;
    let (v, p) = id.split_once(':').ok_or("VENDOR:PRODUCT")?;
    let hex = |s: &str| u16::from_str_radix(s, 16).map_err(|_| format!("{s} is not hex"));
    let mut identity = Identity::input().with_class(class);
    if class == Class::Keyboard {
        identity = identity.with_class(Class::Key);
    }
    identity.id = Some((hex(v)?, hex(p)?));
    identity.name = Some(name.to_owned());
    Ok(Device {
        name: name.to_owned(),
        identity,
    })
}

fn parse_check(rest: &str) -> Result<Check, String> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    let file = |w: &str| match w {
        "draft" => Ok(Which::Draft),
        "installed" => Ok(Which::Installed),
        "batch" => Ok(Which::Batch),
        _ => Err(format!("{w} is not draft, installed or batch")),
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
        [f, kind @ ("display" | "input"), ..] => {
            // the device's name may have spaces: the words up to "reaches"
            let at = words
                .iter()
                .position(|w| *w == "reaches")
                .ok_or("display|input NAME reaches PLACE")?;
            let _ = kind;
            Check::Reaches {
                file: file(f)?,
                device: words[2..at].join(" "),
                place: words[at + 1..].join(" "),
            }
        }
        ["nothing", "changed"] => Check::Unchanged,
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

    /// What the Effects tab lists from the machine's folders: the
    /// vtlooks under `etc/looks` and the person's own under the home's
    /// configuration, each color map and shader active when batch.conf
    /// lists it.
    fn effects(&self) -> vtium_config::tui::effects_found::Effects {
        use vtium_config::batch::State;
        let folders = vtium_config::tui::effects_found::Folders {
            presets: self.root.join("usr/share/vtium/looks"),
            looks: self.root.join("etc/looks"),
            shaders: vec![self.root.join("etc/shaders")],
            own_looks: Some(self.root.join("home/config/vtium/looks")),
            own_shaders: Some(self.root.join("home/config/vtium/shaders")),
        };
        let batch = std::fs::read_to_string(self.root.join("etc/batch.conf")).unwrap_or_default();
        let state = |p: &std::path::Path| {
            if batch.lines().any(|l| std::path::Path::new(l.trim()) == p) {
                State::Active
            } else {
                State::Inactive
            }
        };
        let euid = rustix::process::geteuid().as_raw();
        vtium_config::tui::effects_found::find(
            &folders,
            &vtium_settings::Trust::running_as(euid),
            &state,
            &|p| vtium_config::batch::trusted(p, euid),
        )
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
    /// The devices connected to the machine.
    devices: Vec<Device>,
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
                    // The Looks tab reads each font set through the active
                    // fonts the console reads, as the TUI's own finder does.
                    let active = console::active_fonts(&self.machine.root.join("etc/batch.conf"));
                    Event::Sourced(vtium_config::tui::find_sources(
                        &frits,
                        &helper,
                        Some(&active),
                        &names,
                    ))
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

    /// What a lookup must leave as it was: the saved draft, every file of
    /// the machine's `etc`, and what the console shows.
    fn state(&self) -> Vec<(String, String)> {
        let mut out = vec![("the saved draft".to_owned(), self.machine.draft())];
        let mut stack = vec![self.machine.root.join("etc")];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.filter_map(Result::ok) {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let rel = p.strip_prefix(&self.machine.root).unwrap_or(&p);
                    out.push((
                        rel.display().to_string(),
                        std::fs::read_to_string(&p).unwrap_or_default(),
                    ));
                }
            }
        }
        let c = &self.console;
        out.push((
            "the console".to_owned(),
            format!(
                "font {} drawn {:?} cell {}x{} palette {} grid {:?}",
                c.font,
                c.drawn,
                c.cell.width,
                c.cell.height,
                c.palette(),
                c.grid
            ),
        ));
        out.sort();
        out
    }

    /// Where vtium puts the device `name` under the rules of `text`.
    fn place(&self, text: &str, name: &str) -> Result<String, String> {
        use vtium_settings::Place;
        let config = vtium_settings::parse(text)
            .map_err(|e| format!("vtium cannot read the file: {e:?}"))?
            .config;
        let i = self
            .devices
            .iter()
            .position(|d| d.name == name)
            .ok_or_else(|| format!("the case connects no device {name:?}"))?;
        let ids: Vec<_> = self.devices.iter().map(|d| d.identity.clone()).collect();
        let placement = config.place(&ids, None);
        Ok(match placement.placed.get(i).map(|p| p.place) {
            Some(Place::Head(h)) => config
                .heads
                .get(h)
                .map_or_else(|| format!("head {h}"), |h| h.name.clone()),
            Some(Place::DefaultHead) => "console".to_owned(),
            Some(Place::Roaming) => "roams".to_owned(),
            Some(Place::Ignore) => "ignored".to_owned(),
            Some(Place::Off) => "off".to_owned(),
            other => format!("{other:?}"),
        })
    }

    /// Whether `check` holds, or what was found instead; `start` is the
    /// state at the case's start.
    fn check(&self, check: &Check, start: &[(String, String)]) -> Result<(), String> {
        let file = |which: Which| match which {
            Which::Draft => ("the saved draft", self.machine.draft()),
            Which::Installed => ("the installed vtium.conf", self.machine.installed()),
            Which::Batch => (
                "the installed batch.conf",
                std::fs::read_to_string(self.machine.root.join("etc/batch.conf"))
                    .unwrap_or_default(),
            ),
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
            Check::Reaches {
                file: which,
                device,
                place,
            } => {
                let (what, body) = file(*which);
                let got = self.place(&body, device)?;
                if got != *place {
                    return Err(format!(
                        "under {what}, vtium puts {device} on {got}, not {place}:\n{body}"
                    ));
                }
            }
            Check::Unchanged => {
                let now = self.state();
                let moved: Vec<String> = now
                    .iter()
                    .filter(|n| !start.contains(n))
                    .map(|(what, text)| format!("{what} is now:\n{text}"))
                    .chain(
                        start
                            .iter()
                            .filter(|(w, _)| !now.iter().any(|(n, _)| n == w))
                            .map(|(what, _)| format!("{what} is gone")),
                    )
                    .collect();
                if !moved.is_empty() {
                    return Err(format!("a lookup changed things: {}", moved.join("\n")));
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
    // make gives the new draft's text; the draft's name is mine
    session
        .make("mine", vtium_config::drafts::From::Installed)
        .expect("the draft mine");
    if let Some(text) = &case.draft {
        store
            .write_draft(
                "mine",
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
    inputs.view = View::Flow;
    inputs.effects = machine.effects();
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
    // the devices the desk reports, as sysfs and udev would on the case's
    // machine; no udev daemon runs there
    let present = vtium_config::flowmodel::Present {
        devices: case.devices.iter().map(|d| d.identity.clone()).collect(),
        udev: false,
    };
    let mut run = Run {
        machine: &machine,
        app,
        desk: Store::new(store, paths.clone(), Box::new(move || present.clone())),
        console,
        exited: None,
        said: Vec::new(),
        devices: case.devices.clone(),
    };
    run.answer(first);
    let start = run.state();
    let show = std::env::var_os("VTIUM_USECASE_SHOW").is_some();
    let mut at = "the start".to_owned();
    let mut failures = Vec::new();
    let mut lines = case.steps.clone();
    // a lookup changes nothing, whatever its own checks say
    if case.kind == Kind::CheckIf {
        lines.push((0, Line::At("the end, against the start".to_owned())));
        lines.push((0, Line::Check(Check::Unchanged)));
    }
    for (line, entry) in &lines {
        match entry {
            Line::Step(num, sentence, step) => {
                if let Err(why) = run.follow(step) {
                    failures.push(format!(
                        "{}:{line}, step {num} {sentence:?}: {why}",
                        case.name
                    ));
                    // the steps after one that cannot be followed mean
                    // nothing, so the case stops here
                    break;
                }
                if show {
                    eprintln!(
                        "--- {}:{line} {num}. {sentence}\n{}\nconsole: font {} drawn {:?} cell {}x{} palette {} grid {:?}",
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
            Line::At(name) => at.clone_from(name),
            Line::Check(check) => {
                if let Err(why) = run.check(check, &start) {
                    failures.push(format!("{}:{line}, at {at}: {why}", case.name));
                }
            }
        }
    }
    failures
}

/// The cases under `cases/`, one folder for each type of task, by name,
/// or the one `VTIUM_USECASE` names.
fn cases() -> Vec<Case> {
    let only = std::env::var("VTIUM_USECASE").ok();
    let mut files: Vec<PathBuf> = Vec::new();
    for folder in std::fs::read_dir(cases_dir()).expect("the cases' folder") {
        let folder = folder.expect("a folder of cases").path();
        assert!(
            folder.is_dir(),
            "{} is not in a folder: each case lives in the folder of its type of task",
            folder.display()
        );
        files.extend(
            std::fs::read_dir(&folder)
                .expect("a folder of cases")
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "case")),
        );
    }
    files.sort();
    files
        .iter()
        .filter_map(|p| {
            let stem = p.file_stem()?.to_string_lossy().into_owned();
            let folder = p.parent()?.file_name()?.to_string_lossy().into_owned();
            let name = format!("{folder}/{stem}");
            if only.as_ref().is_some_and(|o| *o != name && *o != stem) {
                return None;
            }
            let text = std::fs::read_to_string(p).expect("a case file");
            Some(parse(&name, &text).unwrap_or_else(|e| panic!("{e}")))
        })
        .collect()
}

/// Every use case: a case with no `waits-for` line passes all its checks,
/// and a case with one fails at least one, until the defect it names is
/// fixed. The test prints each case's outcome, grouped by its folder.
#[test]
fn every_use_case_does_what_the_person_asked() {
    let mut wrong = Vec::new();
    let mut table = Vec::new();
    let mut folder = String::new();
    for case in cases() {
        if case.folder != folder {
            folder.clone_from(&case.folder);
            table.push(format!("{folder}/"));
        }
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
        let file = case.name.rsplit('/').next().unwrap_or_default();
        table.push(format!("  {file}: {outcome}"));
        // an expected-red case lists the first line of each failure, so
        // the report shows what still waits
        if case.waits.is_some() {
            for f in &failures {
                table.push(format!("      {}", f.lines().next().unwrap_or_default()));
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
    for kind in [Kind::HowDoI, Kind::IWantTo, Kind::CheckIf] {
        assert!(
            cases.iter().any(|c| c.kind == kind),
            "no case of kind {kind:?}"
        );
    }
}
