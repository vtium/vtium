//! The helpers that the TUI's headless tests share: the console's own
//! terminal that answers the TUI, the desk that keeps the draft in memory,
//! the fixture's inputs, the keys as bytes, and the screen goldens. Each
//! `tui*.rs` test file includes this module and reads its items by a glob
//! import, so a test reads as it did when every test was in one file.
//!
//! Each test file uses some of these items and not the others, so the
//! module allows the items and the imports that one file leaves unused.

#![allow(dead_code, unused_imports)]

#[path = "../common/home.rs"]
pub mod home;
pub use vtium_testscript as script;

pub use std::collections::{BTreeMap, VecDeque};
pub use std::io;
pub use std::path::{Path, PathBuf};
pub use std::time::Duration;

pub use vtium_config::kitty;
pub use vtium_config::made::Made;
pub use vtium_config::serve::LeftOut;
pub use vtium_config::specimen::{Picture, Shot, Sources, Summary};
pub use vtium_config::tui::desk::Done;
pub use vtium_config::tui::fixture::SECTIONS;
pub use vtium_config::tui::model::{self, DpiFrom, Font, FontState, Inputs};
pub use vtium_config::tui::port::Step;
pub use vtium_config::tui::port::{Draw, Drawn, Port, Wake, drive};
pub use vtium_font::Cell;
pub use vtium_term::Glyph;
pub use vtium_term::Terminal;

/// What answers the TUI.
pub enum Answerer {
    /// The console's terminal, as vtium answers: XTVERSION, the OSC 50
    /// query and switches under `allow-font-ops`, DA1.
    Vtium(Box<Terminal>),
    /// A terminal that answers DA1 alone.
    Da1Only { cols: usize, rows: usize },
}

/// A console standing in for the terminal the TUI runs in.
pub struct Console {
    pub answerer: Answerer,
    /// The fonts the console takes, each with the grid it gives.
    pub fonts: BTreeMap<&'static str, (usize, usize)>,
    /// Keys still to type, one read each.
    pub script: VecDeque<Vec<u8>>,
    pub pending: VecDeque<Wake>,
    pub armed: bool,
    /// Every byte the TUI wrote.
    pub written: Vec<u8>,
    /// What the stand-in font helper says of a font set in place of what
    /// [`Port::sources`] says of it by default, as the console's helper
    /// says it through the installed batch.
    pub sources_found: BTreeMap<String, Sources>,
    /// Each OSC 50 switch the console took ("set NAME") or refused
    /// ("refuse NAME") as the TUI wrote it, and each reply ("reply NAME")
    /// as the TUI read it, in order.
    pub log: Vec<String>,
    /// Each command the privileged step ran, as its runner was handed it.
    pub saves: Vec<Vec<std::ffi::OsString>>,
    /// The privileged step's command fails, saying this.
    pub save_result: Result<(), String>,
    pub panic_bytes: Vec<u8>,
    /// The screen when the script ran out, after which the console sends
    /// SIGTERM.
    pub seen: Option<String>,
    /// How many bytes the TUI had written when the script ran out: the
    /// bytes that drew [`Console::seen`].
    pub seen_at: usize,
    /// The console answers kitty's graphics query, as vtium built with
    /// `images` does.
    pub graphics: bool,
    /// Each specimen the TUI asked for: the font, the pixel height and the
    /// most pixels it may take.
    pub pictures: Vec<(String, u32, (u32, u32))>,
    /// Each picture of the console asked for, whole.
    pub shots: Vec<Shot>,
    /// Each font's cell asked for, at its height.
    pub cells: Vec<(String, u32)>,
    /// Each detailed specimen asked for: its font, height and room.
    pub specimens: Vec<(String, u32, (u32, u32))>,
    /// The terminal's cell in pixels, as the terminal reports it.
    pub cell_px: Option<(u32, u32)>,
    /// Each summary the TUI asked for: the font and the pixel height.
    pub summaries: Vec<(String, u32)>,
    /// Each sources query the TUI made.
    pub sources_asked: Vec<Vec<String>>,
    /// The helper's answer to a sources query never comes, as a slow one
    /// has not yet.
    pub hold_sources: bool,
    /// The font files each describe request named, in order.
    pub described_asked: Vec<Vec<PathBuf>>,
    /// The helper's descriptions never come, as a slow helper's have not
    /// yet.
    pub hold_descriptions: bool,
    /// A save writes each file of its plan, as the privileged step would.
    pub installs: bool,
    /// The replies to a font switch reach the TUI in two reads, the first
    /// ending in the ESC that begins DA1's reply.
    pub cut_replies: bool,
    /// How many times the console cut a switch's replies so.
    pub cuts: usize,
}

impl Console {
    pub fn vtium(cols: usize, rows: usize, ops: bool, start: &str) -> Console {
        let mut t = Terminal::new(cols, rows, 0);
        t.set_version("vtium(test)").expect("printable");
        t.set_font_ops(ops);
        assert!(t.set_font_name(start));
        Console::with(Answerer::Vtium(Box::new(t)))
    }

    pub fn da1_only(cols: usize, rows: usize) -> Console {
        Console::with(Answerer::Da1Only { cols, rows })
    }

    pub fn with(answerer: Answerer) -> Console {
        Console {
            answerer,
            fonts: BTreeMap::new(),
            script: VecDeque::new(),
            pending: VecDeque::new(),
            armed: false,
            written: Vec::new(),
            sources_found: BTreeMap::new(),
            log: Vec::new(),
            saves: Vec::new(),
            save_result: Ok(()),
            panic_bytes: Vec::new(),
            seen: None,
            seen_at: 0,
            graphics: false,
            pictures: Vec::new(),
            shots: Vec::new(),
            cells: Vec::new(),
            specimens: Vec::new(),
            cell_px: Some((8, 16)),
            summaries: Vec::new(),
            sources_asked: Vec::new(),
            hold_sources: false,
            described_asked: Vec::new(),
            hold_descriptions: false,
            installs: false,
            cut_replies: false,
            cuts: 0,
        }
    }

    pub fn takes(mut self, name: &'static str, grid: (usize, usize)) -> Console {
        self.fonts.insert(name, grid);
        self
    }

    pub fn keys(mut self, chunks: &[&[u8]]) -> Console {
        self.script.extend(chunks.iter().map(|c| c.to_vec()));
        self
    }

    /// The terminal's screen as text, one line a row.
    pub fn screen(&self) -> String {
        let Answerer::Vtium(t) = &self.answerer else {
            return String::new();
        };
        let s = t.screen();
        (0..s.rows())
            .map(|r| {
                let line = s.line(r).expect("a row");
                let text: String = line
                    .cells()
                    .iter()
                    .map(|&c| match line.glyph(c) {
                        Glyph::Char(ch) => ch.to_string(),
                        Glyph::Cluster(s) => s.to_owned(),
                        Glyph::Empty => " ".to_owned(),
                    })
                    .collect();
                format!("{}\n", text.trim_end())
            })
            .collect()
    }

    /// The OSC 50 sequences the TUI wrote, but the query at start.
    pub fn switches(&self) -> Vec<String> {
        let text = String::from_utf8_lossy(&self.written).into_owned();
        text.split("\x1b]50;")
            .skip(1)
            .map(|s| s.split('\x1b').next().unwrap_or("").to_owned())
            .filter(|s| s != "?")
            .collect()
    }
}

impl Port for Console {
    fn size(&mut self) -> (usize, usize) {
        match &self.answerer {
            Answerer::Vtium(t) => (t.screen().cols(), t.screen().rows()),
            Answerer::Da1Only { cols, rows } => (*cols, *rows),
        }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.written.extend_from_slice(bytes);
        match &mut self.answerer {
            Answerer::Vtium(t) => {
                let mut rest = bytes;
                while !rest.is_empty() {
                    let taken = t.input(rest);
                    if let Some(name) = t.take_font_request() {
                        if let Some(&(cols, rows)) = self.fonts.get(name.as_str()) {
                            assert!(t.set_font_name(&name));
                            self.log.push(format!("set {name}"));
                            if (cols, rows) != (t.screen().cols(), t.screen().rows()) {
                                t.resize(cols, rows);
                                self.pending.push_back(Wake::Resize);
                            }
                        } else {
                            self.log.push(format!("refuse {name}"));
                        }
                    }
                    rest = &rest[taken..];
                }
                let mut replies = t.take_replies();
                if self.graphics && bytes.windows(4).any(|w| w == b"a=q,") {
                    // the graphics query's answer comes before DA1's
                    let at = replies
                        .windows(3)
                        .position(|w| w == b"\x1b[?")
                        .unwrap_or(replies.len());
                    let ok = format!("\x1b_Gi={};OK\x1b\\", model::QUERY_ID);
                    replies.splice(at..at, ok.into_bytes());
                }
                let da1 = replies.windows(3).position(|w| w == b"\x1b[?");
                match da1 {
                    Some(at) if self.cut_replies && bytes.starts_with(b"\x1b]50;") => {
                        let rest = replies.split_off(at + 1);
                        self.pending.push_back(Wake::Bytes(replies));
                        self.pending.push_back(Wake::Bytes(rest));
                        self.cuts += 1;
                    }
                    _ if !replies.is_empty() => self.pending.push_back(Wake::Bytes(replies)),
                    _ => {}
                }
            }
            Answerer::Da1Only { .. } => {
                if bytes.windows(3).any(|w| w == b"\x1b[c") {
                    self.pending.push_back(Wake::Bytes(b"\x1b[?62c".to_vec()));
                }
            }
        }
        Ok(())
    }

    fn arm(&mut self, _after: Duration) {
        self.armed = true;
    }

    fn wait(&mut self) -> io::Result<Vec<Wake>> {
        if !self.pending.is_empty() {
            let wakes: Vec<Wake> = self.pending.drain(..).collect();
            // a reply counts once the TUI reads it
            for wake in &wakes {
                if let Wake::Bytes(bytes) = wake {
                    for name in String::from_utf8_lossy(bytes).split("\x1b]50;").skip(1) {
                        let name = name.split('\x1b').next().unwrap_or("");
                        self.log.push(format!("reply {name}"));
                    }
                }
            }
            return Ok(wakes);
        }
        if let Some(keys) = self.script.pop_front() {
            if keys == b"@" {
                // udev reports a device that came or went
                return Ok(vec![Wake::Hotplug]);
            }
            if keys.first() == Some(&b'!') {
                // a signal, by its number after the mark
                let n = String::from_utf8_lossy(&keys[1..])
                    .parse()
                    .expect("a number");
                return Ok(vec![Wake::Signal(n)]);
            }
            return Ok(vec![Wake::Bytes(keys)]);
        }
        if std::mem::take(&mut self.armed) {
            return Ok(vec![Wake::Timeout]);
        }
        assert!(self.seen.is_none(), "the TUI still waits after SIGTERM");
        self.seen = Some(self.screen());
        self.seen_at = self.written.len();
        Ok(vec![Wake::Signal(15)])
    }

    /// Runs the privileged step with a runner that records each command
    /// and, with `installs`, runs it: this crate's own binary in place of
    /// the test's, so `apply DIR` installs into the fixture's directories
    /// as it does for a user.
    fn privileged(&mut self, step: Step<'_>) -> Result<Done, String> {
        let installs = self.installs;
        let failing = self.save_result.clone();
        let saves = &mut self.saves;
        let mut run = |argv: &[std::ffi::OsString]| -> io::Result<(bool, String)> {
            saves.push(argv.to_vec());
            if let Err(said) = &failing {
                return Ok((false, said.clone()));
            }
            if !installs {
                return Ok((true, String::new()));
            }
            let (_, args) = argv.split_first().expect("a command");
            let out = home::command(env!("CARGO_BIN_EXE_vtium-config"))
                .args(args)
                .output()?;
            let mut said = String::from_utf8_lossy(&out.stdout).into_owned();
            said.push_str(&String::from_utf8_lossy(&out.stderr));
            Ok((out.status.success(), said))
        };
        // what the step writes for the person, and each question it asks,
        // which this console answers no
        let mut talk: Vec<u8> = Vec::new();
        let mut ask = |out: &mut dyn std::io::Write, question: &str| -> bool {
            let _ = writeln!(out, "{question} [y/N]");
            false
        };
        step(&mut run, &mut talk, &mut ask)
    }

    fn on_panic(&mut self, bytes: Vec<u8>) {
        self.panic_bytes = bytes;
    }

    fn cell_px(&mut self) -> Option<(u32, u32)> {
        self.cell_px
    }

    /// Draws what is asked as the stand-in helper does, and answers at
    /// the next wait: tiny5's cell is 4x6 and every other font's 8x16; a
    /// picture is two pixels, or the real painter's refusal where its
    /// preview area cannot show one line of text; a summary is unifont's.
    fn draw(&mut self, ask: Draw) {
        let answer = match ask {
            Draw::Cell { font, height } => {
                self.cells.push((font.clone(), height));
                let (width, cell_height) = if font == "tiny5" { (4, 6) } else { (8, 16) };
                Drawn::Celled {
                    font,
                    height,
                    cell: Ok((
                        Cell {
                            width,
                            height: cell_height,
                        },
                        cell_height,
                    )),
                }
            }
            Draw::Picture(shot) => {
                self.pictures
                    .push((shot.font.clone(), shot.height, shot.max));
                let shown = vtium_config::specimen::shown(&shot);
                self.shots.push(shot);
                Drawn::Pictured(shown.map(|_| Picture {
                    width: 2,
                    height: 1,
                    rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
                }))
            }
            Draw::Specimen {
                font,
                height,
                max,
                display,
            } => {
                self.specimens.push((font.clone(), height, max));
                let grid = Some((display.0 / 8, display.1 / 16));
                Drawn::Detailed {
                    font,
                    height,
                    result: Ok((
                        Picture {
                            width: 2,
                            height: 1,
                            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
                        },
                        Summary {
                            cell: Cell {
                                width: 8,
                                height: 16,
                            },
                            grid,
                            covered: 300,
                            of: 323,
                            script: Some(Made::Scaled),
                        },
                    )),
                }
            }
            Draw::Summary { font, height } => {
                self.summaries.push((font.clone(), height));
                Drawn::Summarized {
                    font,
                    height,
                    summary: Ok(Summary {
                        cell: Cell {
                            width: 8,
                            height: 16,
                        },
                        grid: Some((80, 30)),
                        covered: 40,
                        of: 51,
                        script: Some(Made::Scaled),
                    }),
                }
            }
        };
        self.pending.push_back(Wake::Drawn(answer));
    }

    /// Answers with what the fixture's stand-in font helper says of each
    /// file, unless the test holds the descriptions back.
    fn describe(&mut self, paths: Vec<std::path::PathBuf>) {
        self.described_asked.push(paths.clone());
        if self.hold_descriptions {
            return;
        }
        let answers = paths
            .into_iter()
            .map(|p| {
                let d = vtium_config::tui::fixture::described(&p);
                (p, d)
            })
            .collect();
        self.pending.push_back(Wake::Described(answers));
    }

    fn sources(&mut self, fonts: Vec<String>) {
        self.sources_asked.push(fonts.clone());
        if self.hold_sources {
            return;
        }
        let found = fonts
            .into_iter()
            .map(|font| {
                if let Some(found) = self.sources_found.get(&font) {
                    return (font, found.clone());
                }
                let left = match font.as_str() {
                    "proof" => Sources::LeftOut(vec![LeftOut {
                        missing: true,
                        line: 27,
                        spelling: "font:JetBrains Mono".into(),
                        why: "no such family".into(),
                    }]),
                    "tiny5" => Sources::DrawsNothing("no source left".into()),
                    _ => Sources::LeftOut(Vec::new()),
                };
                (font, left)
            })
            .collect();
        self.pending.push_back(Wake::Sourced(found));
    }
}

pub const DOWN: &[u8] = b"\x1b[B";

pub const UP: &[u8] = b"\x1b[A";

/// Shift-Tab: the tab before, in the tab bar's order.
pub const BACKTAB: &[u8] = b"\x1b[Z";

/// The digits that go straight to a tab with nothing open: `1` the Fonts
/// tab, `2` the Effects tab, `3` the Looks tab, which holds the sizes, `5`
/// the Settings tab and `6` the Help tab.
pub const TO_FONTS: &[u8] = b"1";
pub const TO_EFFECTS: &[u8] = b"2";
pub const TO_LOOKS: &[u8] = b"3";
pub const TO_SETTINGS: &[u8] = b"5";
pub const TO_HELP: &[u8] = b"6";

pub fn font(name: &str, state: FontState) -> Font {
    Font {
        name: name.to_owned(),
        state,
    }
}

pub fn inputs() -> Inputs {
    vtium_config::tui::fixture::inputs()
}

/// The fixture's inputs with three looks that draw unifont, at automatic,
/// 6 pt and 7 pt, and no other look. The Looks tab opens on the first, so
/// Down previews unifont at 6 pt and Down again at 7 pt, as the tests of
/// pictures at a size need.
pub fn sized() -> Inputs {
    let mut i = inputs();
    i.conf = Some(
        "# mine\nfont-name=unifont\nversion = 1\n[look auto]\nfont-name = unifont\nfont-size = 0\n[look six]\nfont-name = unifont\nfont-size = 6\n[look seven]\nfont-name = unifont\nfont-size = 7\n".to_owned(),
    );
    i
}

/// The console of most tests: vtium at 80x25 drawing unifont, taking the
/// fonts that read at the same grid.
pub fn console() -> Console {
    Console::vtium(80, 25, true, "unifont")
        .takes("minspec-16", (80, 25))
        .takes("proof", (80, 25))
        .takes("tiny5", (80, 25))
        .takes("unifont", (80, 25))
        .takes("unscii-8", (80, 25))
}

/// A vtium console like [`console`]'s with `allow-font-ops` off, so the
/// TUI cannot switch its font and previews each change as a picture, or
/// as a written description where it draws none, as it does in every
/// terminal but a vtium console it can switch (ADR-1201 D4).
pub fn outside() -> Console {
    Console::vtium(80, 25, false, "unifont")
        .takes("minspec-16", (80, 25))
        .takes("proof", (80, 25))
        .takes("tiny5", (80, 25))
        .takes("unifont", (80, 25))
        .takes("unscii-8", (80, 25))
}

pub fn run(console: &mut Console) -> u8 {
    drive(console, inputs()).expect("the loop")
}

/// A desk standing in for the store in the tests of the views beside the
/// flow: its draft starts as the fixture's vtium.conf, its check passes,
/// its folder keeps the draft's font sets as the store's does, and it
/// keeps each plan it installs. As the store does, it installs only
/// what a Prepare readied, and refuses an install that no Prepare readied.
/// It hands the console's runner the command an install runs, so the
/// console decides whether the privileged step succeeds.
pub struct Desk {
    pub installed: String,
    pub draft: String,
    pub plans: Vec<vtium_config::draft::Plan>,
    /// A Prepare readied an install that has not yet run.
    pub ready: bool,
    /// What the step after the install did on the VTs, as an install of
    /// the system's vtium.conf answers; `None` for other directories.
    pub after: Option<vtium_config::vts::Outcome>,
    /// The resources that are active or that the draft changes, as the
    /// batch and the draft's changes hold them.
    pub resources: Vec<(PathBuf, vtium_config::batch::State)>,
    /// An Op::SystemFont readied a font's install that has not yet run.
    pub font_ready: bool,
    /// The fonts it installed as system fonts, by their targets.
    pub fonts_installed: Vec<PathBuf>,
    /// What the system's fonts folder holds where the person's font
    /// would go: nothing, a different file, or the same file.
    pub placement: vtium_config::apply::Placement,
    /// The draft's folder: the font sets it adds, replaces and removes.
    pub frits: vtium_config::store::FritChanges,
    /// The installed font sets, by name, as the fixture's inputs list
    /// them, which an install writes and removes.
    pub installed_frits: std::collections::BTreeMap<String, Vec<u8>>,
}

pub fn desk() -> Desk {
    let conf = inputs().conf.expect("the fixture's vtium.conf");
    Desk {
        installed: conf.clone(),
        draft: conf,
        plans: Vec::new(),
        ready: false,
        after: None,
        resources: active(),
        font_ready: false,
        fonts_installed: Vec::new(),
        placement: vtium_config::apply::Placement::New,
        frits: Default::default(),
        installed_frits: inputs().installed,
    }
}

/// The fixture's resources that are active at the start.
pub fn active() -> Vec<(PathBuf, vtium_config::batch::State)> {
    vtium_config::tui::fixture::active()
}

impl Desk {
    pub fn opened(&self) -> vtium_config::tui::desk::Opened {
        self.checked(&self.draft)
    }

    /// The draft `text` as the store's check answers it: counted against
    /// the installed file, with the draft's changes to the active
    /// resources counted as the store counts the changes kept beside it.
    pub fn checked(&self, text: &str) -> vtium_config::tui::desk::Opened {
        use vtium_config::batch::State;
        let (added, removed) = vtium_config::drafts::counts(&self.installed, text);
        vtium_config::tui::desk::Opened {
            name: "mine".into(),
            text: text.to_owned(),
            added,
            removed,
            batch: self
                .resources
                .iter()
                .filter(|(_, s)| matches!(s, State::Activates | State::Deactivates))
                .count(),
            resources: self.resources.clone(),
            frits: self.frits.clone(),
            frit_changes: vtium_config::tui::fixture::frit_edits(
                &self.frits,
                &self.installed_frits,
                &vtium_config::tui::fixture::inputs().frits,
            ),
            ..Default::default()
        }
    }

    /// The plan an install hands the privileged step, as the store's
    /// prepare builds it: the font sets first, then vtium.conf where the
    /// draft differs from it.
    fn plan(&self) -> vtium_config::draft::Plan {
        let (mut files, removals) = vtium_config::tui::fixture::frit_plan(
            &self.frits,
            &self.installed_frits,
            &inputs().frits,
        );
        if self.draft != self.installed {
            files.push(vtium_config::draft::FileEdit {
                path: PathBuf::from("/etc/vtium/vtium.conf"),
                old: Some(self.installed.clone()),
                new: self.draft.clone(),
            });
        }
        vtium_config::draft::Plan {
            files,
            removals,
            no_route: false,
        }
    }
}

impl vtium_config::tui::desk::Desk for Desk {
    fn run(&mut self, op: &vtium_config::tui::desk::Op) -> Result<Done, String> {
        use vtium_config::tui::desk::Op;
        if *op == Op::Prepare {
            self.ready = true;
        }
        match op {
            Op::Open => Ok(Done::Draft(self.opened(), Some(Default::default()))),
            Op::Write(text) => {
                self.draft.clone_from(text);
                Ok(Done::Draft(self.opened(), None))
            }
            // the store's check reads the text it is given and writes nothing
            Op::Check(text) => Ok(Done::Draft(self.checked(text), None)),
            Op::Prepare => Ok(Done::Plan(self.plan())),
            Op::FontSet { frit, edit, text } => {
                vtium_config::tui::fixture::font_set_op(&mut self.frits, frit, edit)?;
                Ok(Done::Draft(self.checked(text), None))
            }
            Op::Devices => Ok(Done::Present {
                present: Default::default(),
                densities: inputs().densities,
            }),
            Op::Resource { path, change } => {
                vtium_config::tui::fixture::resource_op(&mut self.resources, path, *change)
            }
            Op::SystemFont(path) => {
                let done = vtium_config::tui::fixture::system_font_op(path, self.placement)?;
                // an identical file readies nothing, as the store readies
                // no draft for it
                self.font_ready = self.placement != vtium_config::apply::Placement::Identical;
                Ok(done)
            }
            other => Err(format!("{other:?} is not in these tests")),
        }
    }

    fn install(
        &mut self,
        plan: &vtium_config::draft::Plan,
        run: vtium_config::apply::Runner<'_>,
        _out: &mut dyn std::io::Write,
        _ask: vtium_config::apply::Ask<'_>,
    ) -> Result<Done, String> {
        if !std::mem::take(&mut self.ready) {
            return Err(vtium_config::tui::words::NOT_PREPARED.to_owned());
        }
        self.plans.push(plan.clone());
        let argv: Vec<std::ffi::OsString> = ["sudo", "vtium-config", "apply", "DRAFT"]
            .iter()
            .map(Into::into)
            .collect();
        let (ok, said) = run(&argv).map_err(|e| e.to_string())?;
        if !ok {
            return Err(said);
        }
        self.installed.clone_from(&self.draft);
        vtium_config::tui::fixture::frits_install(&mut self.frits, &mut self.installed_frits);
        Ok(match self.after.clone() {
            Some(outcome) => Done::Installed(self.opened(), outcome),
            None => Done::Draft(self.opened(), None),
        })
    }

    /// Installs the font an Op::SystemFont readied, through the console's
    /// runner, as the store does; refuses when none is ready.
    fn install_font(&mut self, run: vtium_config::apply::Runner<'_>) -> Result<Done, String> {
        if !std::mem::take(&mut self.font_ready) {
            return Err(vtium_config::tui::words::NO_FONT_READY.to_owned());
        }
        let argv: Vec<std::ffi::OsString> = ["sudo", "vtium-config", "apply", "FONT-DRAFT"]
            .iter()
            .map(Into::into)
            .collect();
        let (ok, said) = run(&argv).map_err(|e| e.to_string())?;
        if !ok {
            return Err(said);
        }
        self.fonts_installed.push(PathBuf::from(
            vtium_config::tui::fixture::OWN_FONT_INSTALLED,
        ));
        // the store's scan after the install marks each font as the draft
        // and its changes to the active resources hold it
        let mut done = vtium_config::tui::fixture::font_installed();
        if let Done::FontInstalled {
            found: Ok(found), ..
        } = &mut done
        {
            for f in found {
                f.state = self
                    .resources
                    .iter()
                    .find(|(p, _)| *p == f.path)
                    .map_or(vtium_config::batch::State::Inactive, |(_, s)| *s);
            }
        }
        Ok(done)
    }
}

/// Runs the TUI on `console` with the stand-in desk `d`.
pub fn run_desk(console: &mut Console, d: &mut Desk) -> u8 {
    vtium_config::tui::port::drive_with(console, inputs(), Some(d)).expect("the loop")
}

/// The provisional goldens' names, from `tests/screens/provisional.tsv`.
pub fn provisional() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/screens/provisional.tsv");
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.split('\t').next().map(str::to_owned))
        .collect()
}

/// Compares `screen` with the golden `name`: a settled golden fails on a
/// difference, a provisional one reports it. `VTIUM_CONFIG_BLESS=1`
/// writes the provisional ones, and the settled ones still judge.
///
/// With `VTIUM_CONFIG_SHOTS=DIR`, it also writes `bytes`, every byte the
/// TUI wrote to reach the screen, as `DIR/NAME.bytes`, and the screen as
/// `DIR/NAME.txt`, for `make tui-shots` to draw (crates/vtium-config's
/// README).
pub fn golden(name: &str, screen: &str, bytes: &[u8]) {
    if let Some(dir) = std::env::var_os("VTIUM_CONFIG_SHOTS") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("the shots' directory");
        std::fs::write(dir.join(format!("{name}.bytes")), bytes).expect("the bytes");
        std::fs::write(dir.join(format!("{name}.txt")), screen).expect("the screen");
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/screens")
        .join(format!("{name}.txt"));
    let is_provisional = provisional().iter().any(|n| n == name);
    if is_provisional && std::env::var_os("VTIUM_CONFIG_BLESS").is_some() {
        std::fs::write(&path, screen).expect("the golden");
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    if want != screen {
        assert!(
            is_provisional,
            "{name} differs from its settled golden:\n{screen}"
        );
        eprintln!("provisional golden {name} drifted; it judges nothing:\n{screen}");
    }
}

/// The screen the script `keys` reaches at `cols` by `rows`, from the
/// Fonts view.
pub fn reach(cols: usize, rows: usize, keys: &[&[u8]]) -> String {
    reach_from(model::View::Fonts, cols, rows, keys).0
}

/// The screen the script `keys` reaches at `cols` by `rows`, from `view`,
/// and the bytes the TUI wrote to draw it.
pub fn reach_from(
    view: model::View,
    cols: usize,
    rows: usize,
    keys: &[&[u8]],
) -> (String, Vec<u8>) {
    reach_on(true, view, cols, rows, keys)
}

/// The screen the script `keys` reaches at `cols` by `rows`, from `view`,
/// in a vtium console with `allow-font-ops` on or, with `ops` false, off,
/// and the bytes the TUI wrote to draw it.
pub fn reach_on(
    ops: bool,
    view: model::View,
    cols: usize,
    rows: usize,
    keys: &[&[u8]],
) -> (String, Vec<u8>) {
    let grid = (cols, rows);
    let mut c = Console::vtium(cols, rows, ops, "unifont")
        .takes("minspec-16", grid)
        .takes("proof", grid)
        .takes("tiny5", grid)
        .takes("unifont", grid)
        .takes("unscii-8", grid)
        .keys(keys);
    let mut inputs = inputs();
    inputs.view = view;
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    let bytes = c.written.get(..c.seen_at).unwrap_or_default().to_vec();
    (screen, bytes)
}

pub const RIGHT: &[u8] = b"\x1b[C";

pub const LEFT: &[u8] = b"\x1b[D";

pub const TAB: &[u8] = b"\t";

/// The screen the script `keys` leaves at 80x25, from `view`.
pub fn key_line(view: model::View, keys: &[&[u8]]) -> String {
    reach_from(view, 80, 25, keys).0
}

/// The first page of the key line that the contract gives the node `id`
/// of the interface tree, the page the TUI shows when it comes to the node.
pub fn keys_of(id: &str) -> &'static str {
    vtium_config::tui::contract::key_pages(id)
        .and_then(|pages| pages.first())
        .copied()
        .unwrap_or_else(|| panic!("no key line for {id}"))
}

/// The key line at the foot of `screen`: as many of its last rows as the
/// key line `line` takes at 80 columns, each key kept whole on a row,
/// joined as a key line. It equals `line` when the screen shows `line`.
pub fn foot(screen: &str, line: &str) -> String {
    let mut rows: Vec<String> = Vec::new();
    for item in line.split("  ") {
        match rows.last_mut() {
            Some(row) if row.chars().count() + 2 + item.chars().count() <= 78 => {
                row.push_str("  ");
                row.push_str(item);
            }
            _ => rows.push(item.to_owned()),
        }
    }
    let shown: Vec<&str> = screen.lines().map(str::trim).collect();
    shown[shown.len().saturating_sub(rows.len())..].join("  ")
}

pub use vtium_config::tui::words;

/// The last row of `screen`, without its margin.
pub fn last_row(screen: &str) -> &str {
    screen.lines().last().unwrap_or_default().trim()
}

/// The bytes that show the stand-in's picture at the Looks view's place,
/// below its two rows of sentence and the blank row after them.
pub fn shown() -> Vec<u8> {
    let mut b = b"\x1b[6;26H".to_vec();
    b.extend(kitty::show(
        kitty::IMAGE_ID,
        2,
        1,
        &[255, 0, 0, 255, 0, 0, 255, 255],
    ));
    b
}

pub fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

/// The text a run of the TUI left on the console's screen, from the row
/// below the caption of the preview: the preview's rows, trimmed.
pub fn preview_rows(screen: &str, first: &str) -> Vec<String> {
    screen
        .lines()
        .skip_while(|l| !l.contains(first))
        .map(|l| l.trim().to_owned())
        .collect()
}

/// The TUI on the fixture, driven by its events with no loop and no
/// terminal, as the tests of a view drive it: opened on the Flow view,
/// with a key pressed, the screen read as text, and the bytes it writes.
mod direct {
    use super::*;
    use vtium_config::tui::input::{Input, Key};
    use vtium_config::tui::model::{App, Effect, Event, View};
    use vtium_config::tui::view;

    pub fn app(cols: usize, rows: usize) -> App {
        let mut i = inputs();
        i.view = View::Flow;
        App::new(i, cols, rows).0
    }

    pub fn press(app: &mut App, key: Key) -> Vec<Effect> {
        app.event(Event::Input(Input::Key(key)))
    }

    pub fn text(app: &App) -> String {
        view::as_text(&view::screen(app))
    }

    pub fn writes(effects: &[Effect], bytes: &[u8]) -> bool {
        effects.iter().any(|e| *e == Effect::Write(bytes.to_vec()))
    }
}

pub use direct::*;
