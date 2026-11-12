//! The TUI driven headlessly through its real loop (ADR-1201 D8): scripted
//! keys go in as bytes, the console's own terminal, vtium-term, answers on
//! the other side as a vtium console does, and each test reads the state's
//! effects in the bytes the TUI wrote and in the screen the terminal holds.
//!
//! The screens at 80x25 and 100x30 are vtium's own goldens in
//! `tests/screens/`, provisional until the maintainer reviews the layout:
//! `tests/screens/provisional.tsv` lists them, and a provisional golden
//! that drifts is reported and judges nothing.

#[path = "common/home.rs"]
mod home;
use vtium_testscript as script;

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use vtium_config::kitty;
use vtium_config::made::Made;
use vtium_config::serve::LeftOut;
use vtium_config::specimen::{Picture, Shot, Sources, Summary};
use vtium_config::tui::desk::Done;
use vtium_config::tui::fixture::SECTIONS;
use vtium_config::tui::model::{self, DpiFrom, Font, FontState, Inputs};
use vtium_config::tui::port::Step;
use vtium_config::tui::port::{Draw, Drawn, Port, Wake, drive};
use vtium_font::Cell;
use vtium_term::Glyph;
use vtium_term::Terminal;

/// What answers the TUI.
enum Answerer {
    /// The console's terminal, as vtium answers: XTVERSION, the OSC 50
    /// query and switches under `allow-font-ops`, DA1.
    Vtium(Box<Terminal>),
    /// A terminal that answers DA1 alone.
    Da1Only { cols: usize, rows: usize },
}

/// A console standing in for the terminal the TUI runs in.
struct Console {
    answerer: Answerer,
    /// The fonts the console takes, each with the grid it gives.
    fonts: BTreeMap<&'static str, (usize, usize)>,
    /// Keys still to type, one read each.
    script: VecDeque<Vec<u8>>,
    pending: VecDeque<Wake>,
    armed: bool,
    /// Every byte the TUI wrote.
    written: Vec<u8>,
    /// Each OSC 50 switch the console took ("set NAME") or refused
    /// ("refuse NAME") as the TUI wrote it, and each reply ("reply NAME")
    /// as the TUI read it, in order.
    log: Vec<String>,
    /// Each command the privileged step ran, as its runner was handed it.
    saves: Vec<Vec<std::ffi::OsString>>,
    /// The privileged step's command fails, saying this.
    save_result: Result<(), String>,
    panic_bytes: Vec<u8>,
    /// The screen when the script ran out, after which the console sends
    /// SIGTERM.
    seen: Option<String>,
    /// How many bytes the TUI had written when the script ran out: the
    /// bytes that drew [`Console::seen`].
    seen_at: usize,
    /// The console answers kitty's graphics query, as vtium built with
    /// `images` does.
    graphics: bool,
    /// Each specimen the TUI asked for: the font, the pixel height and the
    /// most pixels it may take.
    pictures: Vec<(String, u32, (u32, u32))>,
    /// Each picture of the console asked for, whole.
    shots: Vec<Shot>,
    /// Each font's cell asked for, at its height.
    cells: Vec<(String, u32)>,
    /// Each detailed specimen asked for: its font, height and room.
    specimens: Vec<(String, u32, (u32, u32))>,
    /// The terminal's cell in pixels, as the terminal reports it.
    cell_px: Option<(u32, u32)>,
    /// Each summary the TUI asked for: the font and the pixel height.
    summaries: Vec<(String, u32)>,
    /// Each sources query the TUI made.
    sources_asked: Vec<Vec<String>>,
    /// The helper's answer to a sources query never comes, as a slow one
    /// has not yet.
    hold_sources: bool,
    /// A save writes each file of its plan, as the privileged step would.
    installs: bool,
}

impl Console {
    fn vtium(cols: usize, rows: usize, ops: bool, start: &str) -> Console {
        let mut t = Terminal::new(cols, rows, 0);
        t.set_version("vtium(test)").expect("printable");
        t.set_font_ops(ops);
        assert!(t.set_font_name(start));
        Console::with(Answerer::Vtium(Box::new(t)))
    }

    fn da1_only(cols: usize, rows: usize) -> Console {
        Console::with(Answerer::Da1Only { cols, rows })
    }

    fn with(answerer: Answerer) -> Console {
        Console {
            answerer,
            fonts: BTreeMap::new(),
            script: VecDeque::new(),
            pending: VecDeque::new(),
            armed: false,
            written: Vec::new(),
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
            installs: false,
        }
    }

    fn takes(mut self, name: &'static str, grid: (usize, usize)) -> Console {
        self.fonts.insert(name, grid);
        self
    }

    fn keys(mut self, chunks: &[&[u8]]) -> Console {
        self.script.extend(chunks.iter().map(|c| c.to_vec()));
        self
    }

    /// The terminal's screen as text, one line a row.
    fn screen(&self) -> String {
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
    fn switches(&self) -> Vec<String> {
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
                if !replies.is_empty() {
                    self.pending.push_back(Wake::Bytes(replies));
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
        step(&mut run)
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

    fn sources(&mut self, fonts: Vec<String>) {
        self.sources_asked.push(fonts.clone());
        if self.hold_sources {
            return;
        }
        let found = fonts
            .into_iter()
            .map(|font| {
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

const DOWN: &[u8] = b"\x1b[B";
const UP: &[u8] = b"\x1b[A";
/// Shift-Tab: from the Fonts view, the Looks view, which holds the sizes.
const BACKTAB: &[u8] = b"\x1b[Z";

fn font(name: &str, state: FontState) -> Font {
    Font {
        name: name.to_owned(),
        state,
    }
}

fn inputs() -> Inputs {
    vtium_config::tui::fixture::inputs()
}

/// The console of most tests: vtium at 80x25 drawing unifont, taking the
/// fonts that read at the same grid.
fn console() -> Console {
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
fn outside() -> Console {
    Console::vtium(80, 25, false, "unifont")
        .takes("minspec-16", (80, 25))
        .takes("proof", (80, 25))
        .takes("tiny5", (80, 25))
        .takes("unifont", (80, 25))
        .takes("unscii-8", (80, 25))
}

fn run(console: &mut Console) -> u8 {
    drive(console, inputs()).expect("the loop")
}

/// A desk standing in for the store in the tests of the views beside the
/// flow: its draft starts as the fixture's vtium.conf, its check passes,
/// and it keeps each plan it installs. As the store does, it installs only
/// what a Prepare readied, and refuses an install that no Prepare readied.
/// It hands the console's runner the command an install runs, so the
/// console decides whether the privileged step succeeds.
struct Desk {
    installed: String,
    draft: String,
    plans: Vec<vtium_config::draft::Plan>,
    /// A Prepare readied an install that has not yet run.
    ready: bool,
}

fn desk() -> Desk {
    let conf = inputs().conf.expect("the fixture's vtium.conf");
    Desk {
        installed: conf.clone(),
        draft: conf,
        plans: Vec::new(),
        ready: false,
    }
}

impl Desk {
    fn opened(&self) -> vtium_config::tui::desk::Opened {
        let (added, removed) = vtium_config::drafts::counts(&self.installed, &self.draft);
        vtium_config::tui::desk::Opened {
            name: "mine".into(),
            text: self.draft.clone(),
            added,
            removed,
            ..Default::default()
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
            Op::Check(text) => {
                let (added, removed) = vtium_config::drafts::counts(&self.installed, text);
                let opened = vtium_config::tui::desk::Opened {
                    name: "mine".into(),
                    text: text.clone(),
                    added,
                    removed,
                    ..Default::default()
                };
                Ok(Done::Draft(opened, None))
            }
            Op::Prepare if self.draft == self.installed => Ok(Done::Plan(Default::default())),
            Op::Prepare => Ok(Done::Plan(vtium_config::draft::Plan {
                files: vec![vtium_config::draft::FileEdit {
                    path: PathBuf::from("/etc/vtium/vtium.conf"),
                    old: Some(self.installed.clone()),
                    new: self.draft.clone(),
                }],
                removals: Vec::new(),
                no_route: false,
            })),
            Op::Devices => Ok(Done::Present(Default::default())),
            other => Err(format!("{other:?} is not in these tests")),
        }
    }

    fn install(
        &mut self,
        plan: &vtium_config::draft::Plan,
        run: vtium_config::apply::Runner<'_>,
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
        Ok(Done::Draft(self.opened(), None))
    }
}

/// Runs the TUI on `console` with the stand-in desk `d`.
fn run_desk(console: &mut Console, d: &mut Desk) -> u8 {
    vtium_config::tui::port::drive_with(console, inputs(), Some(d)).expect("the loop")
}

#[test]
fn the_four_requests_go_first_in_their_order() {
    let mut c = console().keys(&[b"q"]);
    run(&mut c);
    assert!(
        c.written.starts_with(&model::probe()),
        "{:?}",
        String::from_utf8_lossy(&c.written[..60])
    );
    assert_eq!(
        model::probe(),
        b"\x1b[>q\x1b]50;?\x1b\\\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c"
    );
}

/// Moving the highlight sends the live console nothing (ADR-1201 D4): no
/// switch goes, and leaving has no font to give back.
#[test]
fn each_highlighted_font_leaves_the_live_console_alone() {
    let mut c = console().keys(&[UP, &[DOWN, DOWN].concat(), DOWN, b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.switches(), Vec::<String>::new(), "{:?}", c.log);
    assert!(!c.log.iter().any(|l| l.starts_with("set ")), "{:?}", c.log);
    // and the console draws the preview's characters itself, so no font
    // highlighted is drawn as a picture, though the console draws them
    assert!(c.specimens.is_empty(), "{:?}", c.specimens);
    assert!(c.shots.is_empty() && c.summaries.is_empty());
    assert_eq!(count(&c.written, b"\x1b_Ga=T"), 0);
}

/// `t` switches the console to the highlighted font, one switch at a
/// time: a key that puts the font back while the trial's switch is still
/// in flight sends the switch back only once the console has replied.
#[test]
fn t_tries_the_font_live_and_one_switch_is_in_flight_at_a_time() {
    let mut c = console().keys(&[UP, &[b"t".as_slice(), b"x"].concat(), b"q"]);
    assert_eq!(run(&mut c), 0);
    let mut outstanding = false;
    for entry in &c.log {
        if entry.starts_with("set ") || entry.starts_with("refuse ") {
            assert!(!outstanding, "a second switch before a reply: {:?}", c.log);
            outstanding = true;
        } else if entry.starts_with("reply ") {
            outstanding = false;
        }
    }
    assert_eq!(c.switches(), ["tiny5", "unifont"], "{:?}", c.log);
}

#[test]
fn a_font_the_console_refuses_is_marked_and_the_tui_goes_on() {
    let mut c = Console::vtium(80, 25, true, "unifont")
        .takes("unifont", (80, 25))
        .takes("proof", (80, 25))
        .keys(&[UP, b"t", UP, b"t", b"k", b"q"]);
    assert_eq!(run(&mut c), 0);
    assert!(c.log.contains(&"refuse tiny5".to_owned()), "{:?}", c.log);
    assert!(c.log.contains(&"set proof".to_owned()), "{:?}", c.log);
}

#[test]
fn the_refusal_mark_is_on_the_screen() {
    let mut c = Console::vtium(80, 25, true, "unifont")
        .takes("unifont", (80, 25))
        .keys(&[UP, b"t"]);
    let mut inputs = inputs();
    inputs.conf = None;
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("tiny5")
            && screen.contains("not used: the console kept its font (see its log)"),
        "{screen}"
    );
    assert!(screen.contains("The console refused tiny5"), "{screen}");
}

#[test]
fn leaving_by_q_gives_the_start_font_back() {
    let mut c = console().keys(&[UP, b"t", b"k", b"q"]);
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    assert_eq!(c.log.last().map(String::as_str), Some("reply unifont"));
    assert!(
        c.panic_bytes.is_empty(),
        "the font is back, so a panic writes nothing"
    );
}

/// Ctrl-C while the trial counts down leaves, and leaving gives the start
/// font back.
#[test]
fn leaving_by_ctrl_c_gives_the_start_font_back() {
    let mut c = console().keys(&[UP, b"t", b"\x03"]);
    assert_eq!(run(&mut c), 130);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
}

#[test]
fn leaving_by_sigterm_gives_the_start_font_back() {
    let mut c = console().keys(&[UP, b"t", b"!15"]);
    assert_eq!(run(&mut c), 143);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
}

#[test]
fn leaving_with_a_switch_in_flight_waits_for_both_replies() {
    // the switch to tiny5 and the leave come in one read
    let mut c = console().keys(&[&[UP, b"t", b"\x03"].concat()]);
    assert_eq!(run(&mut c), 130);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    assert_eq!(c.log.last().map(String::as_str), Some("reply unifont"));
    assert!(
        c.panic_bytes.is_empty(),
        "the font is back, so nothing is written again"
    );
}

#[test]
fn another_terminal_gets_no_osc_50_but_the_start_query() {
    let mut c = Console::da1_only(80, 25).keys(&[DOWN, DOWN, UP, b"\r", b"q"]);
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.switches(), Vec::<String>::new());
    let text = String::from_utf8_lossy(&c.written);
    assert_eq!(
        text.matches("\x1b]50;").count(),
        1,
        "the query at start only"
    );
}

#[test]
fn with_font_operations_off_the_tui_says_so_and_offers_the_key() {
    let mut c = Console::vtium(80, 25, false, "unifont")
        .takes("tiny5", (80, 25))
        .keys(&[UP, UP]);
    run(&mut c);
    assert_eq!(c.switches(), Vec::<String>::new());
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("text preview · no live trial"), "{screen}");
    assert!(screen.contains("allow-font-ops turned off"), "{screen}");
    // t says why it does nothing, and sends nothing
    let mut c = Console::vtium(80, 25, false, "unifont")
        .takes("tiny5", (80, 25))
        .keys(&[UP, b"t"]);
    run(&mut c);
    assert_eq!(c.switches(), Vec::<String>::new());
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains(vtium_config::tui::words::TRIAL_OFF),
        "{screen}"
    );
}

#[test]
fn turning_font_operations_on_is_a_change_installed_like_any_other() {
    let mut d = desk();
    let mut c = Console::vtium(80, 25, false, "unifont").keys(&[b"o", b"i", b"y", b"q"]);
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert_eq!(plan.files.len(), 1);
    assert!(
        plan.files[0].new.contains("\nallow-font-ops\n"),
        "{:?}",
        plan.files[0].new
    );
}

#[test]
fn a_font_switch_that_changes_the_grid_redraws_at_the_new_grid() {
    let mut c = console().takes("tiny5", (100, 30)).keys(&[UP, b"t", b"k"]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows.len(), 30);
    assert!(
        rows[29].contains("Enter choose"),
        "the keys on the last row: {screen}"
    );
    assert!(
        rows[0].len() > 80,
        "the title reaches the new edge: {:?}",
        rows[0]
    );
}

/// A font kept from the live trial that leaves a grid too small for any
/// view draws the screen that says so, and keeps the keys to leave and to
/// undo the font on it however few its rows (ADR-1201 D3).
#[test]
fn a_grid_too_small_draws_the_way_out() {
    let mut c = console().takes("tiny5", (30, 8)).keys(&[UP, b"t", b"k"]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows.len(), 8);
    // the whole of what is wrong, then the keys, in the 8 rows
    let joined = rows.iter().map(|r| r.trim()).collect::<Vec<_>>().join(" ");
    assert!(
        joined.starts_with(&vtium_config::tui::words::too_small(30, 8)),
        "{screen}"
    );
    assert!(
        joined.ends_with(vtium_config::tui::words::TOO_SMALL_KEYS),
        "{screen}"
    );
    assert!(rows.iter().all(|r| r.chars().count() <= 30), "{screen}");
}

#[test]
fn keeping_a_font_and_a_size_writes_each_into_the_draft_and_install_counts_them() {
    let mut d = desk();
    let mut c = console().keys(&[
        UP, b"\r", BACKTAB, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, b"\r", b"i",
    ]);
    run_desk(&mut c, &mut d);
    // the top level's last line is `version = 1`, so the new key follows it
    assert_eq!(
        d.draft,
        format!("# mine\nfont-name=tiny5\n{SECTIONS}")
            .replace("version = 1\n", "version = 1\nfont-size=12\n")
    );
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Install the draft? 2 lines added, 1 removed."),
        "{screen}"
    );
    assert!(d.plans.is_empty());
}

#[test]
fn installing_runs_one_install_and_the_console_keeps_the_installed_font() {
    let mut d = desk();
    let mut c = console().keys(&[UP, b"\r", b"i", b"y", b"q"]);
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert_eq!(d.plans.len(), 1);
    assert_eq!(c.saves.len(), 1);
    let file = &d.plans[0].files[0];
    assert_eq!(file.new, format!("# mine\nfont-name=tiny5\n{SECTIONS}"));
    // after installing, leaving does not switch back
    assert_eq!(c.switches(), ["tiny5"]);
}

#[test]
fn a_failed_install_keeps_the_draft_and_says_why() {
    let mut d = desk();
    let mut c = console().keys(&[UP, b"\r", b"i", b"y"]);
    c.save_result = Err("sudo: a password is required".into());
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Not installed: sudo: a password is required"),
        "{screen}"
    );
    assert!(d.draft.contains("font-name=tiny5"), "{}", d.draft);
    assert_eq!(d.installed, inputs().conf.expect("the fixture"));
}

/// The settings view lists every key vtium.conf takes, from the console's
/// own key table, the font keys first, each once.
#[test]
fn the_settings_view_lists_every_key_vtium_conf_takes() {
    use vtium_settings::keys::{KEYS, Scope};
    let listed: Vec<&str> = model::setting_keys().iter().map(|k| k.name).collect();
    let takes: Vec<&str> = KEYS
        .iter()
        .filter(|k| k.scope == Scope::Anywhere)
        .map(|k| k.name)
        .collect();
    assert_eq!(listed.len(), takes.len(), "{listed:?}");
    for name in &takes {
        assert_eq!(listed.iter().filter(|n| *n == name).count(), 1, "{name}");
    }
    assert_eq!(&listed[..model::FONT_KEYS.len()], &model::FONT_KEYS[..]);
}

/// ADR-1005 D10: the settings view lists the seventeen keys vtium takes
/// for itself, each with its built-in default from the console's key
/// table, and shows a key's default on its row.
#[test]
fn the_settings_view_lists_vtiums_own_keys_with_their_built_in_defaults() {
    let want = [
        ("key-scroll-up", "<Shift>Up"),
        ("key-scroll-down", "<Shift>Down"),
        ("key-page-up", "<Shift>Prior"),
        ("key-page-down", "<Shift>Next"),
        ("key-prompt-up", "<Ctrl><Shift>Up"),
        ("key-prompt-down", "<Ctrl><Shift>Down"),
        ("key-zoom-in", "<Ctrl>plus,<Ctrl>equal"),
        ("key-zoom-out", "<Ctrl>minus"),
        ("key-rotate-cw", "<Logo>plus,<Logo>equal"),
        ("key-rotate-ccw", "<Logo>minus"),
        ("key-reset", "<Ctrl><Shift>Delete"),
        ("key-console-next", "<Ctrl><Logo>Right"),
        ("key-console-previous", "<Ctrl><Logo>Left"),
        ("key-console-open", "<Ctrl><Logo>Return"),
        ("key-console-close", "<Ctrl><Logo>BackSpace"),
        ("key-focus-next", "<Ctrl><Logo>Up"),
        ("key-focus-previous", "<Ctrl><Logo>Down"),
    ];
    let listed = model::setting_keys();
    for (name, default) in want {
        let key = listed
            .iter()
            .find(|k| k.name == name)
            .unwrap_or_else(|| panic!("{name} is not listed"));
        assert_eq!(
            vtium_config::commands::value_text(key, None),
            (default.to_owned(), false),
            "{name}"
        );
    }
    let at = listed
        .iter()
        .position(|k| k.name == "key-reset")
        .expect("listed");
    let mut keys: Vec<&[u8]> = vec![b"\t"];
    keys.extend(std::iter::repeat_n(DOWN, at));
    let mut c = console().keys(&keys);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("key-reset"), "{screen}");
    assert!(screen.contains("<Ctrl><Shift>Delete (default)"), "{screen}");
    assert!(screen.contains("Reset the console on screen"), "{screen}");
}

/// ADR-1005 D10: in the settings view an empty value of a key binding is
/// D10's empty value, no key, written `key-reset=`; the editor starts from
/// the key's combinations, so Enter alone keeps them; and the word
/// `default` returns the key to its default.
#[test]
fn the_settings_view_empties_a_key_binding_and_default_returns_it() {
    let at = model::setting_keys()
        .iter()
        .position(|k| k.name == "key-reset")
        .expect("listed");
    let erase = [b"\x7f".as_slice(); 19];
    let mut keys: Vec<&[u8]> = vec![b"\t"];
    keys.extend(std::iter::repeat_n(DOWN, at));
    keys.push(b"\r");
    keys.extend(erase);
    keys.push(b"\r");
    let mut d = desk();
    let mut c = console().keys(&[keys.as_slice(), &[b"q".as_slice(), b"y"]].concat());
    assert_eq!(run_desk(&mut c, &mut d), 0, "{:?}", c.seen);
    assert!(d.draft.contains("\nkey-reset=\n"), "{}", d.draft);
    // Enter again opens the empty value, and `default` returns the key to
    // its default: its line is commented out
    let mut c = console().keys(
        &[
            keys.as_slice(),
            &[b"\r".as_slice(), b"default", b"\r", b"q", b"y"],
        ]
        .concat(),
    );
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0, "{:?}", c.seen);
    assert!(d.draft.contains("\n#key-reset=\n"), "{}", d.draft);
    // Enter alone keeps the default's combinations as they are
    let mut c =
        console().keys(&[&keys[..keys.len() - 20], &[b"\r".as_slice(), b"q", b"y"]].concat());
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert!(!d.draft.contains("key-reset="), "{}", d.draft);
}

#[test]
fn a_setting_shows_its_default_and_what_it_does() {
    let mut c = console().keys(&[b"\t", DOWN]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("font-size"), "{screen}");
    assert!(screen.contains("0 (default)"), "{screen}");
    assert!(screen.contains("The font size in points"), "{screen}");
}

/// Enter types a value for a key that takes one, a second Enter keeps it,
/// a switch turns, a choice steps, and the save changes each line; typed
/// text takes `q` and `i` as letters.
/// Enter types a value for a key that takes one, a second Enter keeps it,
/// a switch turns and a choice steps, each written into the draft at
/// once; typed text takes `q` and `i` as letters.
#[test]
fn a_typed_value_a_switch_and_a_choice_go_into_the_draft() {
    let mut d = desk();
    let mut c = console().keys(&[
        b"\t", DOWN, DOWN, DOWN, b"\r", DOWN, b"\r", b"10", b"\r", DOWN, b"\r", DOWN, b"\r", b"qi",
        b"\r", b"q", b"y",
    ]);
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert_eq!(
        d.draft,
        // a new key goes after the last line of the top level, never into a
        // section (ADR-1403 D8)
        "# mine\nfont-name=unifont\nversion = 1\nfont-hinting=light\nfont-line-spacing=10\nno-allow-font-ops\ndisplay-dpi=qi\n[look proof]\nfont-name = proof\n[head desk]\nlook = proof\n[head panel]\n[rules]\ndisplay * -> desk\n"
    );
}

#[test]
fn a_new_frit_is_typed_in_its_view_and_installed_into_the_frits() {
    let mut c = console().keys(&[
        b"n",
        b"quiet",
        b"\r",
        b"builtin:unscii-8",
        b"\r",
        b"Some Family",
        b"\r",
        b"\r",
        b"\r",
        b"\r",
        b"\r",
        b"i",
        b"y",
        b"q",
    ]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert_eq!(plan.files.len(), 1);
    assert_eq!(
        plan.files[0].path,
        PathBuf::from("/etc/vtium/fonts/quiet.vtfrit")
    );
    assert_eq!(
        plan.files[0].new,
        "# quiet: made by vtium-config new-frit\nname=quiet\nsource=builtin:unscii-8\nsource=font:Some Family\n"
    );
}

/// A new frit named as an installed one replaces it, the save showing the
/// installed text it replaces, as the TUI gathered it at start; one whose
/// installed file is not UTF-8 text is not replaced, and says why.
#[test]
fn a_new_frit_replaces_the_installed_one_of_its_name() {
    let to_new = [b"n".as_slice()];
    let mut keys: Vec<&[u8]> = to_new.to_vec();
    keys.extend([b"proof".as_slice(), b"\r", b"builtin:unifont"]);
    // five fields to the add row, the add, and the install
    keys.extend([b"\r".as_slice(); 6]);
    keys.extend([b"i".as_slice(), b"y", b"q"]);
    let mut c = console().keys(&keys);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let plan = d.plans.first().expect("an install");
    assert_eq!(
        plan.files[0].path,
        PathBuf::from("/etc/vtium/fonts/proof.vtfrit")
    );
    assert_eq!(
        plan.files[0].old.as_deref(),
        Some("source=font:JetBrains Mono\n")
    );
    let mut keys: Vec<&[u8]> = to_new.to_vec();
    keys.extend([b"latin1".as_slice(), b"\r", b"builtin:unifont"]);
    keys.extend([b"\r".as_slice(); 6]);
    let mut c = console().keys(&keys);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("A new font set cannot replace latin1.vtfrit, because the installed file is not UTF-8 text."),
        "{screen}"
    );
    assert!(d.plans.is_empty());
}

/// `r` in the fonts view removes the highlighted frit when the user
/// saves: the font list marks it, the save view lists its file, and the
/// save's plan removes it with the bytes the TUI read at start, as
/// `remove-frit`'s draft does. A second `r` keeps it.
/// `r` in the fonts view removes the highlighted frit at the next
/// install: the font list marks it, and the install's plan removes it with
/// the bytes the TUI read at start, as `remove-frit`'s draft does. A
/// second `r` keeps it.
#[test]
fn r_removes_the_highlighted_frit_at_the_next_install() {
    // unifont is highlighted at start; Up reaches tiny5
    let mut c = console().keys(&[UP, b"r", b"i"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("and 1 font set"), "{screen}");
    let mut c = console().keys(&[UP, b"r", b"i", b"y", b"q"]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert!(plan.files.is_empty(), "{plan:?}");
    assert_eq!(
        plan.removals,
        [vtium_config::draft::Removal {
            path: PathBuf::from("/etc/vtium/fonts/tiny5.vtfrit"),
            old: b"source=bdf:/x/Tiny5Mono.bdf\n".to_vec(),
        }]
    );
    // marked in the list, and kept by a second r
    let mut c = console().keys(&[UP, b"r"]);
    run_desk(&mut c, &mut desk());
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("to remove"), "{screen}");
    let mut c = console().keys(&[UP, b"r", b"r", b"i", b"y", b"q"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    assert!(d.plans.is_empty());
}

/// The remove key refuses what `remove-frit` refuses, saying why, and
/// adds nothing to the save: a built-in font, the frit vtium.conf's
/// `font-name` names, as it is or as the save leaves it, and the frit a
/// look's `font-name` names.
#[test]
fn r_refuses_what_remove_frit_refuses() {
    for (keys, says) in [
        // unifont, highlighted at start
        (vec![b"r".as_slice()], "built-in font"),
        // proof is the frit look proof draws
        (vec![UP, UP, b"r"], "`[look proof]`"),
        // minspec-16 kept as font-name, then r
        (vec![UP, UP, UP, b"\r", b"r"], "font-name"),
    ] {
        let mut all = keys.clone();
        all.extend([b"i".as_slice(), b"y", b"q"]);
        let mut c = console().keys(&keys);
        run_desk(&mut c, &mut desk());
        let screen = c.seen.clone().expect("the screen");
        let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(text.contains("cannot be removed:"), "{says}: {screen}");
        assert!(text.contains(says), "{says}: {screen}");
        let mut c = console().keys(&all);
        let mut d = desk();
        run_desk(&mut c, &mut d);
        assert!(
            d.plans.iter().all(|p| p.removals.is_empty()),
            "{says}: {:?}",
            d.plans
        );
    }
    // a frit marked for removal and then kept as font-name is not removed:
    // the save says why and saves nothing
    let mut c = console().keys(&[UP, b"r", b"\r", b"i", b"y", b"q"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    assert!(d.plans.is_empty(), "{:?}", d.plans);
}

/// A save that changes font-frits and also writes or removes a frit is
/// refused: the frits would go to the directory font-frits named before
/// the save, so the user saves the new directory first. A save that
/// changes only one of the two is planned.
/// An install that moves font-frits and also writes or removes a frit is
/// refused: the frits would go to the directory font-frits named before,
/// so the user installs the new directory first. An install that changes
/// only one of the two is planned.
#[test]
fn an_install_that_moves_the_frits_and_changes_a_frit_is_refused() {
    use vtium_config::tui::desk::Opened;
    use vtium_config::tui::model::App;
    let conf = inputs().conf.expect("the fixture");
    let drafted = |text: &str| Opened {
        name: "mine".into(),
        text: text.to_owned(),
        ..Opened::default()
    };
    let moved = conf.replace(
        "font-name=unifont\n",
        "font-name=unifont\nfont-frits=/srv/frits\n",
    );
    let (mut app, _) = App::new(inputs(), 80, 25);
    app.draft = Some(drafted(&moved));
    assert!(app.frit_plan().is_ok());
    app.pending.removals.push("tiny5".to_owned());
    let why = app.frit_plan().expect_err("a removal and a new directory");
    assert!(why.contains("font-frits"), "{why}");
    app.pending.removals.clear();
    app.pending
        .frits
        .push(("made".to_owned(), "source=builtin:unifont\n".to_owned()));
    let why = app.frit_plan().expect_err("a new frit and a new directory");
    assert!(why.contains("font-frits"), "{why}");
    // a draft that leaves font-frits alone is no move
    let (mut app, _) = App::new(inputs(), 80, 25);
    app.draft = Some(drafted(
        &conf.replace("font-name=unifont", "font-name=proof"),
    ));
    app.pending.removals.push("tiny5".to_owned());
    assert!(app.frit_plan().is_ok());
    // nor is one that names the directory the installed file names
    let mut named = inputs();
    named.conf = Some(moved.clone());
    let (mut app, _) = App::new(named, 80, 25);
    app.draft = Some(drafted(&moved));
    app.pending.removals.push("tiny5".to_owned());
    assert!(app.frit_plan().is_ok(), "{:?}", app.frit_plan());
}

#[test]
fn a_new_frit_the_reader_refuses_stays_in_its_view_and_says_why() {
    let mut c = console().keys(&[b"n", b"\r", b"\r", b"\r", b"\r", b"\r", b"\r", b"\r"]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("This font set cannot be read"), "{screen}");
    assert!(screen.contains("Font 1"), "{screen}");
    assert!(c.saves.is_empty());
}

#[test]
fn new_opens_the_tui_at_its_new_frit_view() {
    let mut inputs = inputs();
    inputs.view = model::View::New;
    let mut c = console();
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Add this font set to the install"),
        "{screen}"
    );
}

/// Leaving while a frit's removal waits for an install asks first and
/// names it; `l` leaves without it, and the top row counts it.
#[test]
fn leaving_with_a_frit_waiting_asks_and_l_leaves_without_it() {
    let mut c = console().keys(&[UP, b"r", b"q"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("Quit without installing? You will lose the removal of tiny5."),
        "{screen}"
    );
    // the status line says the font set waiting; the next test checks
    // its words at each width
    let status = screen.lines().nth(1).unwrap_or_default();
    assert!(status.contains("· 1 to install"), "{screen}");
    let mut c = console().keys(&[UP, b"r", b"q", b"l"]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert!(d.plans.is_empty());
}

/// The status line with a font set waiting to be installed, on a screen
/// `cols` wide.
fn status_with_a_font_set_waiting(cols: u16) -> String {
    let grid = (usize::from(cols), 25);
    let mut c = Console::vtium(usize::from(cols), 25, true, "unifont")
        .takes("minspec-16", grid)
        .takes("proof", grid)
        .takes("tiny5", grid)
        .takes("unifont", grid)
        .takes("unscii-8", grid)
        .keys(&[UP, b"r", b"q"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    screen.lines().nth(1).unwrap_or_default().to_owned()
}

/// The status line keeps its long words where both sides fit; else the
/// font sets waiting take their short words; else the preview's words
/// shorten too, so the left side fits whole.
#[test]
fn the_status_line_shortens_its_left_side_then_its_right_side() {
    let draft = " draft mine · no changes · no problems · ";
    for (cols, left, right) in [
        (100, "1 font set to install", "text preview · t: try live"),
        (90, "1 to install", "text preview · t: try live"),
        (82, "1 to install", "text preview · t: try live"),
        (81, "1 to install", "text · t: try live"),
        (80, "1 to install", "text · t: try live"),
    ] {
        let status = status_with_a_font_set_waiting(cols);
        assert!(
            status.starts_with(&format!("{draft}{left} ")),
            "{cols}: {status:?}"
        );
        assert!(
            status.ends_with(&format!("  {right}")),
            "{cols}: {status:?}"
        );
    }
}

/// Any other key goes back with the frit still waiting, and `i` at the
/// question installs it.
#[test]
fn leaving_with_a_frit_waiting_goes_back_or_installs() {
    let mut c = console().keys(&[UP, b"r", b"q", b"x", b"q", b"i", b"y", b"q"]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    let plan = d.plans.first().expect("an install");
    assert_eq!(
        plan.removals,
        [vtium_config::draft::Removal {
            path: PathBuf::from("/etc/vtium/fonts/tiny5.vtfrit"),
            old: b"source=bdf:/x/Tiny5Mono.bdf\n".to_vec(),
        }]
    );
}

#[test]
fn nothing_changed_means_nothing_to_install() {
    let mut c = console().keys(&[b"i", b"y", b"q"]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert!(d.plans.is_empty());
    assert!(c.saves.is_empty());
}

/// The provisional goldens' names, from `tests/screens/provisional.tsv`.
fn provisional() -> Vec<String> {
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
fn golden(name: &str, screen: &str, bytes: &[u8]) {
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
fn reach(cols: usize, rows: usize, keys: &[&[u8]]) -> String {
    reach_from(model::View::Fonts, cols, rows, keys).0
}

/// The screen the script `keys` reaches at `cols` by `rows`, from `view`,
/// and the bytes the TUI wrote to draw it.
fn reach_from(view: model::View, cols: usize, rows: usize, keys: &[&[u8]]) -> (String, Vec<u8>) {
    reach_on(true, view, cols, rows, keys)
}

/// The screen the script `keys` reaches at `cols` by `rows`, from `view`,
/// in a vtium console with `allow-font-ops` on or, with `ops` false, off,
/// and the bytes the TUI wrote to draw it.
fn reach_on(
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

#[test]
fn each_view_at_80x25_and_100x30() {
    use model::View::Fonts;
    let scripts: [(&str, model::View, &[&[u8]]); 5] = [
        ("looks", Fonts, &[BACKTAB]),
        ("fonts", Fonts, &[]),
        ("settings", Fonts, &[b"\t"]),
        ("help", Fonts, &[b"\t", b"\t"]),
        ("new", Fonts, &[b"n"]),
    ];
    for (cols, rows) in [(80, 25), (100, 30)] {
        for (name, view, keys) in scripts {
            let (screen, bytes) = reach_from(view, cols, rows, keys);
            let lines: Vec<&str> = screen.lines().collect();
            assert_eq!(lines.len(), rows, "{name} at {cols}x{rows}");
            assert!(lines.iter().all(|l| l.chars().count() <= cols), "{name}");
            assert!(lines[0].contains(" Flow "), "{name}: {screen}");
            // the last row holds the key that quits
            let last = lines[rows - 1];
            assert!(last.contains(" quit"), "{name}: {screen}");
            golden(&format!("{name}-{cols}x{rows}"), &screen, &bytes);
        }
    }
}

/// The Fonts view at 80x25 in a vtium console with `allow-font-ops` off,
/// where the TUI cannot switch the font: the written description, as in
/// any terminal that shows no images, and the notice that the live trial
/// is off (ADR-1201 D4).
#[test]
fn the_fonts_view_without_font_operations_at_80x25() {
    let (screen, bytes) = reach_on(false, model::View::Fonts, 80, 25, &[]);
    assert!(screen.contains("does not support images"), "{screen}");
    assert!(screen.contains(words::KEYS_FONTS), "{screen}");
    golden("fonts-nolive-80x25", &screen, &bytes);
}

// Tab and Shift-Tab step the views wherever the selection is, and the
// arrows move inside a view (#355). Each script goes through the TUI's
// loop, and the key line, the screen's last row, says where it ended.

const RIGHT: &[u8] = b"\x1b[C";
const LEFT: &[u8] = b"\x1b[D";
const TAB: &[u8] = b"\t";

/// The key line the script `keys` leaves at 80x25, from `view`.
fn key_line(view: model::View, keys: &[&[u8]]) -> String {
    let (screen, _) = reach_from(view, 80, 25, keys);
    screen.lines().last().unwrap_or_default().trim().to_owned()
}

/// Whether `line` is the Flow view's key line, which shows the keys that
/// fit and starts with the first.
fn is_flow_keys(line: &str) -> bool {
    let first = words::KEYS_FLOW.split("  ").next().unwrap_or_default();
    line.starts_with(first) && line.ends_with("q quit")
}

use vtium_config::tui::words;

#[test]
fn tab_and_shift_tab_step_the_views_from_every_column_of_the_flow_through_the_loop() {
    for column in 0..6 {
        let mut keys: Vec<&[u8]> = vec![RIGHT; column];
        keys.push(TAB);
        let line = key_line(model::View::Flow, &keys);
        assert_eq!(line, words::KEYS_LOOKS, "Tab from column {column}");
        keys.pop();
        keys.push(BACKTAB);
        let line = key_line(model::View::Flow, &keys);
        assert_eq!(line, words::KEYS_HELP, "Shift-Tab from column {column}");
    }
}

#[test]
fn tab_and_shift_tab_step_every_other_view_and_the_arrows_stay_in_it() {
    use model::View::{Flow, Fonts, Help, Looks, Settings};
    let own = |view| match view {
        Looks => words::KEYS_LOOKS,
        Fonts => words::KEYS_FONTS_LIVE,
        Settings => words::KEYS_SETTINGS,
        _ => words::KEYS_HELP,
    };
    let shown = |line: &str, view| match view {
        Flow => is_flow_keys(line),
        _ => line == own(view),
    };
    for (view, next, before) in [
        (Looks, Fonts, Flow),
        (Fonts, Settings, Looks),
        (Settings, Help, Fonts),
        (Help, Flow, Settings),
    ] {
        let line = key_line(view, &[TAB]);
        assert!(shown(&line, next), "Tab from {view:?}: {line}");
        let line = key_line(view, &[BACKTAB]);
        assert!(shown(&line, before), "Shift-Tab from {view:?}: {line}");
        let arrows = [LEFT, LEFT, RIGHT, RIGHT, RIGHT, UP, DOWN, DOWN];
        let line = key_line(view, &arrows);
        assert_eq!(line, own(view), "the arrows on {view:?}");
    }
}

#[test]
fn tab_answers_a_question_no_on_any_view_and_stays() {
    // a size kept on Looks, then i asks to install it
    let ask: [&[u8]; 4] = [BACKTAB, DOWN, b"\r", b"i"];
    let mut c = console().keys(&ask);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("Install the draft?"), "{screen}");
    let line = screen.lines().last().unwrap_or_default().trim();
    assert_eq!(line, words::KEYS_CONFIRM, "{screen}");
    let mut c = console().keys(&[&ask[..], &[TAB]].concat());
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains(words::NOT_DONE), "{screen}");
    let line = screen.lines().last().unwrap_or_default().trim();
    assert_eq!(line, words::KEYS_LOOKS, "{screen}");
    assert!(d.plans.is_empty());
    // the question before leaving with a frit waiting: Tab goes back
    let mut c = console().keys(&[UP, b"r", b"q"]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    let line = screen.lines().last().unwrap_or_default().trim();
    assert_eq!(line, words::KEYS_LEAVE, "{screen}");
    let mut c = console().keys(&[UP, b"r", b"q", TAB]);
    let mut d = desk();
    run_desk(&mut c, &mut d);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains(words::NOT_LEFT), "{screen}");
    let line = screen.lines().last().unwrap_or_default().trim();
    assert_eq!(line, words::KEYS_FONTS_LIVE, "{screen}");
}

#[test]
fn tab_does_nothing_while_a_value_is_typed_in_settings() {
    // Settings, Down to font-line-spacing, Enter starts typing its value;
    // Tab and Shift-Tab between the two digits change nothing
    let mut c = console().keys(&[
        TAB, DOWN, DOWN, DOWN, DOWN, b"\r", b"1", TAB, BACKTAB, b"0", b"\r", b"q", b"y",
    ]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert!(d.draft.contains("\nfont-line-spacing=10\n"), "{}", d.draft);
}

#[test]
fn tab_does_nothing_in_the_new_frit_view_and_esc_goes_back_to_fonts() {
    for key in [TAB, BACKTAB] {
        let (screen, _) = reach_from(model::View::Fonts, 80, 25, &[b"n", b"abc", key]);
        let line = screen.lines().last().unwrap_or_default().trim();
        assert_eq!(line, words::KEYS_NEW, "{key:?}: {screen}");
        assert!(screen.contains("abc"), "{screen}");
    }
    let line = key_line(model::View::Fonts, &[b"n", b"abc", TAB, b"\x1b"]);
    assert_eq!(line, words::KEYS_FONTS_LIVE);
}

#[test]
fn a_console_that_never_answers_the_give_back_is_left_after_the_wait() {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event};
    let (mut app, _) = App::new(inputs(), 80, 25);
    let mut started = Vec::new();
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Font("unifont".into()),
        Reply::Attributes,
    ] {
        started.extend(app.event(Event::Input(Input::Reply(r))));
    }
    // once the terminal is known, the helper is asked which sources each
    // frit that reads lacks, and for no preview, which the console draws
    let frits = ["minspec-16", "proof", "tiny5"].map(String::from).to_vec();
    assert_eq!(started, [Effect::Sources(frits)]);
    assert_eq!(app.event(Event::Input(Input::Key(Key::Up))), []);
    let tried = app.event(Event::Input(Input::Key(Key::Char('t'))));
    assert_eq!(
        tried,
        [
            Effect::Write(model::switch("tiny5")),
            Effect::Wait(vtium_config::tui::shot::TICK)
        ]
    );
    assert_eq!(
        app.event(Event::Input(Input::Reply(Reply::Font("tiny5".into())))),
        []
    );
    assert_eq!(app.event(Event::Input(Input::Key(Key::Char('k')))), []);
    let leave = app.event(Event::Input(Input::Key(Key::Char('q'))));
    assert_eq!(
        leave,
        [
            Effect::Write(model::switch("unifont")),
            Effect::Wait(model::WAIT)
        ]
    );
    // keys while leaving do nothing; the wait's end leaves
    assert_eq!(app.event(Event::Input(Input::Key(Key::Down))), []);
    assert_eq!(app.event(Event::Timeout), [Effect::Exit(0)]);
}

/// The TUI on a vtium console drawing unifont that answered the start
/// requests, with tiny5 highlighted and tried by `t`, its switch answered.
fn trying_tiny5() -> vtium_config::tui::model::App {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event};
    let (mut app, _) = App::new(inputs(), 80, 25);
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Font("unifont".into()),
        Reply::Attributes,
    ] {
        app.event(Event::Input(Input::Reply(r)));
    }
    app.event(Event::Input(Input::Key(Key::Up)));
    let tried = app.event(Event::Input(Input::Key(Key::Char('t'))));
    assert_eq!(
        tried,
        [
            Effect::Write(model::switch("tiny5")),
            Effect::Wait(vtium_config::tui::shot::TICK)
        ]
    );
    assert_eq!(
        app.event(Event::Input(Input::Reply(Reply::Font("tiny5".into())))),
        []
    );
    app
}

/// One preview is drawn at a time: a highlight that moves while one is
/// being drawn asks for the next once it comes, for whatever is
/// highlighted then.
#[test]
fn one_preview_is_drawn_at_a_time() {
    use vtium_config::specimen::Summary;
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event};
    let (mut app, _) = App::new(inputs(), 80, 25);
    let mut out = Vec::new();
    for r in [Reply::Version("vtium(test)".into()), Reply::Attributes] {
        out.extend(app.event(Event::Input(Input::Reply(r))));
    }
    assert!(out.contains(&Effect::Summary {
        font: "unifont".into(),
        height: 16
    }));
    assert_eq!(app.event(Event::Input(Input::Key(Key::Up))), []);
    assert_eq!(app.event(Event::Input(Input::Key(Key::Up))), []);
    let summary = Summary {
        cell: Cell {
            width: 8,
            height: 16,
        },
        grid: Some((80, 30)),
        covered: 1,
        of: 1,
        script: None,
    };
    let next = app.event(Event::Summarized {
        font: "unifont".into(),
        height: 16,
        summary: Ok(summary),
    });
    assert_eq!(
        next,
        [Effect::Summary {
            font: "proof".into(),
            height: 16
        }]
    );
}

/// A picture that comes after the highlight moved is not shown: no
/// picture is sent, and the next is asked for, for the size highlighted
/// now (ADR-1201 D4).
#[test]
fn a_picture_that_comes_after_the_highlight_moved_is_not_shown() {
    use vtium_config::specimen::Picture;
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{App, Effect, Event};
    let (mut app, _) = App::new(inputs(), 80, 25);
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Graphics(true),
        Reply::Attributes,
    ] {
        app.event(Event::Input(Input::Reply(r)));
    }
    // the specimen of unifont comes, then the Looks view at 6 pt
    let shown = app.event(Event::Detailed {
        font: "unifont".into(),
        height: 16,
        result: Err("none".into()),
    });
    assert!(!shown.iter().any(|e| matches!(e, Effect::Write(_))));
    app.event(Event::Input(Input::Key(Key::BackTab)));
    let asked = app.event(Event::Input(Input::Key(Key::Down)));
    assert_eq!(
        asked,
        [Effect::Cell {
            font: "unifont".into(),
            height: 8
        }]
    );
    let cell = Ok((
        Cell {
            width: 4,
            height: 8,
        },
        8,
    ));
    let picture = app.event(Event::Celled {
        font: "unifont".into(),
        height: 8,
        cell,
    });
    assert!(
        matches!(picture.as_slice(), [Effect::Picture(_)]),
        "{picture:?}"
    );
    // the highlight moves to 7 pt while 6 pt's picture is drawn
    assert_eq!(app.event(Event::Input(Input::Key(Key::Down))), []);
    let came = app.event(Event::Pictured(Ok(Picture {
        width: 2,
        height: 1,
        rgba: vec![0; 8],
    })));
    let sent: Vec<u8> = came
        .iter()
        .filter_map(|e| match e {
            Effect::Write(b) => Some(b.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(!sent.windows(3).any(|w| w == b"a=T"), "{came:?}");
    assert_eq!(
        came.last(),
        Some(&Effect::Cell {
            font: "unifont".into(),
            height: 9
        })
    );
}

/// A picture that fails while the live trial counts down leaves the
/// countdown on the message line.
#[test]
fn a_failed_picture_leaves_the_live_trial_s_countdown() {
    use vtium_config::tui::model::Event;
    let mut app = trying_tiny5();
    let countdown = app.message.clone();
    app.event(Event::Pictured(Err("the helper failed".into())));
    app.event(Event::Detailed {
        font: "tiny5".into(),
        height: 16,
        result: Err("the helper failed".into()),
    });
    assert_eq!(app.message, countdown);
}

/// The live trial counts its ticks down on the message line and, with no
/// keep, puts the font back on the last one (ADR-1201 D4).
#[test]
fn the_live_trial_puts_the_font_back_after_its_ticks() {
    use vtium_config::tui::model::{Effect, Event};
    use vtium_config::tui::shot::{TICK, TRIAL_TICKS};
    let mut app = trying_tiny5();
    assert_eq!(TRIAL_TICKS, 10);
    for left in (1..TRIAL_TICKS).rev() {
        assert_eq!(
            app.event(Event::Timeout),
            [Effect::Wait(TICK)],
            "{left} left"
        );
        assert_eq!(
            app.message.as_deref(),
            Some(vtium_config::tui::words::trial_countdown("tiny5", left).as_str())
        );
    }
    assert_eq!(
        app.event(Event::Timeout),
        [Effect::Write(model::switch("unifont"))]
    );
    assert!(app.trial.is_none());
    // a tick after the trial does nothing
    assert_eq!(app.event(Event::Timeout), []);
}

/// `k` keeps the tried font: no tick puts it back.
#[test]
fn the_keep_key_stops_the_live_trial_s_revert() {
    use vtium_config::tui::input::{Input, Key};
    use vtium_config::tui::model::Event;
    let mut app = trying_tiny5();
    assert_eq!(app.event(Event::Input(Input::Key(Key::Char('k')))), []);
    for _ in 0..20 {
        assert_eq!(app.event(Event::Timeout), []);
    }
    assert_eq!(app.shown_font.as_deref(), Some("tiny5"));
}

/// Any other key during the countdown puts the font back and does nothing
/// else: the highlight stays, and `q` does not leave.
#[test]
fn any_other_key_during_the_live_trial_puts_the_font_back() {
    use vtium_config::tui::input::{Input, Key};
    use vtium_config::tui::model::{Effect, Event};
    for key in [Key::Down, Key::Char('q'), Key::Enter, Key::Char('t')] {
        let mut app = trying_tiny5();
        let at = app.font_at;
        assert_eq!(
            app.event(Event::Input(Input::Key(key))),
            [Effect::Write(model::switch("unifont"))],
            "{key:?}"
        );
        assert_eq!(app.font_at, at, "{key:?} moved the highlight");
        assert!(app.trial.is_none() && app.leaving.is_none(), "{key:?}");
        assert_eq!(app.drafted("font-name").as_deref(), Some("unifont"));
    }
}

/// Leaving while the switch that puts the start font back is still in
/// flight sends it no second time: the TUI waits for that switch's reply
/// and leaves, since the console ends in the start font either way.
#[test]
fn leaving_with_the_start_font_in_flight_sends_it_once() {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{Effect, Event};
    let mut app = trying_tiny5();
    // a key puts unifont back; its reply has not come when q arrives
    assert_eq!(
        app.event(Event::Input(Input::Key(Key::Down))),
        [Effect::Write(model::switch("unifont"))]
    );
    let leave = app.event(Event::Input(Input::Key(Key::Char('q'))));
    assert!(
        !leave
            .iter()
            .any(|e| matches!(e, Effect::Write(b) if b.windows(3).any(|w| w == b"]50"))),
        "no second switch: {leave:?}"
    );
    assert!(leave.contains(&Effect::Wait(model::WAIT)), "{leave:?}");
    assert_eq!(
        app.event(Event::Input(Input::Reply(Reply::Font("unifont".into())))),
        [Effect::Exit(0)]
    );
}

/// Leaving during the countdown, by Ctrl-C or a signal, gives the start
/// font back and leaves once the console has answered.
#[test]
fn leaving_during_the_live_trial_gives_the_start_font_back() {
    use vtium_config::tui::input::{Input, Key, Reply};
    use vtium_config::tui::model::{Effect, Event};
    for event in [Event::Input(Input::Key(Key::Interrupt)), Event::Signal(15)] {
        let mut app = trying_tiny5();
        let out = app.event(event.clone());
        assert_eq!(
            out.first(),
            Some(&Effect::Write(model::switch("unifont"))),
            "{event:?}"
        );
        assert!(app.trial.is_none());
        let done = app.event(Event::Input(Input::Reply(Reply::Font("unifont".into()))));
        assert!(matches!(done.as_slice(), [Effect::Exit(_)]), "{done:?}");
    }
}

/// While the trial counts down on a grid too small for any view, the small
/// screen says first how to keep the font or put it back.
#[test]
fn the_small_screen_shows_the_live_trial_s_countdown() {
    use vtium_config::tui::model::Event;
    use vtium_config::tui::view::{as_text, screen};
    let mut app = trying_tiny5();
    app.event(Event::Resize { cols: 40, rows: 12 });
    let text = as_text(&screen(&app));
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        joined.starts_with("Trying tiny5 on the console for 10 s. Press k to keep it"),
        "{text}"
    );
    assert!(joined.contains("vtium-config needs at least"), "{text}");
}

/// The picture shows the whole display at its resolution, laid out at the
/// grid the font's cell gives on it: the TUI's own screen at that grid,
/// the screen that says the grid is too small included (ADR-1201 D4).
#[test]
fn the_picture_shows_the_display_at_the_grid_the_font_gives() {
    let mut c = outside().keys(&[b"s", UP]);
    c.graphics = true;
    run(&mut c);
    // unifont's 8x16 and tiny5's 4x6 on the fixture's 640x480 display
    let fonts: Vec<&str> = c.shots.iter().map(|s| s.font.as_str()).collect();
    assert_eq!(fonts, ["unifont", "tiny5"]);
    let displays: Vec<(u32, u32)> = c.shots.iter().map(|s| s.display).collect();
    assert_eq!(displays, [(640, 480), (640, 480)]);
    let grids: Vec<(usize, usize)> = c.shots.iter().map(|s| s.grid).collect();
    assert_eq!(grids, [(80, 30), (160, 80)]);
    for shot in &c.shots {
        assert_eq!(shot.screen.len(), shot.grid.1);
        let first = vtium_config::tui::view::text(&shot.screen[0]);
        assert!(first.contains("Fonts"), "the TUI's own tab bar: {first}");
    }
    // the line beside it says what it shows
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("tiny5 at 12 pt on this 640x480 display: each character cell is 4x6 pixels, and the console will be 160 characters wide and 80 lines high."),
        "{screen}"
    );
    // on a display too small for the TUI at that cell, the picture is the
    // screen that says so
    let mut c = outside().keys(&[b"s"]);
    c.graphics = true;
    let mut small = inputs();
    small.display = Some((320, 240));
    drive(&mut c, small).expect("the loop");
    let shot = c.shots.first().expect("a picture");
    assert_eq!(shot.grid, (40, 15));
    let first = vtium_config::tui::view::text(&shot.screen[0]);
    assert!(first.starts_with("This terminal is too small"), "{first}");
}

/// The Looks view's preview draws a fixed screen of sample content,
/// whatever the display's size, at the cell the look gets on that display;
/// the line beside it still counts the real display's grid.
#[test]
fn the_looks_preview_draws_a_fixed_screen_of_sample_content() {
    let mut c = outside().keys(&[BACKTAB, DOWN]);
    c.graphics = true;
    let mut wide = inputs();
    wide.display = Some((7680, 2160));
    drive(&mut c, wide).expect("the loop");
    let shot = c.shots.last().expect("a picture");
    assert_eq!(shot.display, vtium_config::tui::shot::PREVIEW_SCREEN);
    let (cw, ch) = (shot.cell.width as usize, shot.cell.height as usize);
    assert_eq!(shot.grid, (1024 / cw, 768 / ch));
    let first = vtium_config::tui::view::text(&shot.screen[0]);
    assert!(first.starts_with("user@desk:~/notes$ ls -l"), "{first}");
    // the line beside it counts the real display's characters and lines
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    let wide_grid = format!(
        "the console will be {} characters wide and {} lines high",
        7680 / cw,
        2160 / ch
    );
    assert!(text.contains(&wide_grid), "{wide_grid}: {screen}");
    // with VTIUM_CONFIG_PREVIEW_PPM=FILE, the picture as the glyph helper
    // draws it, for the maintainer's review
    if let Some(out) = std::env::var_os("VTIUM_CONFIG_PREVIEW_PPM") {
        let helper = vtium_config::specimen::Helper::at(env!("CARGO_BIN_EXE_vtium-config"));
        let frit = vtium_config::tui::frit_text(Path::new("/nonexistent"), &shot.font)
            .expect("a built-in font's frit");
        // the whole screen, where the TUI shows the part of it that fits
        let mut full = shot.clone();
        full.max = vtium_config::tui::shot::PREVIEW_SCREEN;
        let picture =
            vtium_config::specimen::screen(&helper, &frit, &full, "C").expect("the picture");
        let mut ppm = format!("P6\n{} {}\n255\n", picture.width, picture.height).into_bytes();
        for px in picture.rgba.chunks(4) {
            ppm.extend_from_slice(px.get(..3).unwrap_or_default());
        }
        std::fs::write(out, ppm).expect("the picture's file");
    }
}

#[test]
fn a_terminal_that_answers_nothing_is_settled_by_the_wait() {
    use vtium_config::tui::model::{App, Event, Live};
    let (mut app, first) = App::new(inputs(), 80, 25);
    assert_eq!(first[1], model::Effect::Wait(model::WAIT));
    assert_eq!(app.live, Live::Asking);
    app.event(Event::Timeout);
    assert_eq!(app.live, Live::NotVtium);
}

#[test]
fn a_failed_command_s_words_show_on_the_message_line() {
    let mut c = console().keys(&[UP, b"\r", b"i", b"y"]);
    c.save_result = Err(
        "vtium-config: vtium check-config refused the draft:\n/x/vtium.conf:2: font-size=\"lots\" is not a whole number, ignored".into(),
    );
    run_desk(&mut c, &mut desk());
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("Not installed: vtium-config: vtium check-config refused the draft:"),
        "{screen}"
    );
    assert!(text.contains("is not a whole number"), "{screen}");
}

/// The bytes that show the stand-in's picture at the Looks view's place,
/// below its two rows of sentence and the blank row after them.
fn shown() -> Vec<u8> {
    let mut b = b"\x1b[6;26H".to_vec();
    b.extend(kitty::show(
        kitty::IMAGE_ID,
        2,
        1,
        &[255, 0, 0, 255, 0, 0, 255, 255],
    ));
    b
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

#[test]
fn each_highlighted_size_shows_its_specimen_and_deletes_the_last() {
    let mut c = outside().keys(&[BACKTAB, DOWN, DOWN, b"\t", b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    // 6 pt and 7 pt at 96 dpi, unifont highlighted in the font view, in
    // the area beside the list, 55 columns by 15 rows of 8x16 cells: down
    // to the room the keys and the longest message leave
    let area = (55 * 8, 15 * 16);
    assert_eq!(
        c.pictures,
        [
            ("unifont".to_owned(), 8, area),
            ("unifont".to_owned(), 9, area)
        ]
    );
    // the Fonts view's specimen of unifont at 12 points comes first and
    // last
    let fonts: Vec<(&str, u32)> = c
        .specimens
        .iter()
        .map(|(f, h, _)| (f.as_str(), *h))
        .collect();
    assert_eq!(fonts, [("unifont", 16), ("unifont", 16)]);
    assert_eq!(
        kitty::show(kitty::IMAGE_ID, 2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]),
        b"\x1b_Ga=T,f=32,t=d,i=1201,s=2,v=1,C=1,q=2,m=0;/wAA/wAA//8=\x1b\\"
    );
    assert_eq!(count(&c.written, &shown()), 2);
    // deleted at automatic, when the next size comes, when the Fonts view
    // draws its own, and when the TUI leaves
    assert_eq!(count(&c.written, &kitty::delete(kitty::IMAGE_ID)), 4);
    let last_show = c
        .written
        .windows(shown().len())
        .rposition(|w| w == shown())
        .expect("shown");
    let last_delete = c
        .written
        .windows(kitty::delete(kitty::IMAGE_ID).len())
        .rposition(|w| w == kitty::delete(kitty::IMAGE_ID))
        .expect("deleted");
    assert!(last_delete > last_show, "the last picture is deleted");
}

/// `s` in the Fonts view switches the picture area to the detailed
/// specimen of the highlighted font, at the draft's size and in the same
/// room, and `s` again back to the picture of the console; each highlight
/// asks for the specimen of the font it reaches (ADR-1201 D4). The Fonts
/// view starts on the specimen.
#[test]
fn s_switches_the_fonts_view_between_the_console_and_the_detailed_specimen() {
    let mut c = outside().keys(&[b"s", UP, b"s", b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    // below the list and the line beside the specimen: rows 14 to 20 at
    // 80x25
    let room = (79 * 8, 7 * 16);
    assert_eq!(
        c.specimens,
        [
            ("unifont".to_owned(), 16, room),
            ("tiny5".to_owned(), 16, room)
        ]
    );
    // the console's picture after the first s, until the second
    let fonts: Vec<&str> = c.pictures.iter().map(|(f, _, _)| f.as_str()).collect();
    assert_eq!(fonts, ["unifont", "tiny5"]);
    // the line beside the specimen gives the cell, the grid on the display
    // and how many characters the font draws itself, and its marks
    let mut c = outside().keys(&[]);
    c.graphics = true;
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("unifont at 12 pt: each character cell is 8x16 pixels, and the console will be 80x30 characters on this 640x480 display. The font can display 300 of the 323 characters in the preview."),
        "{screen}"
    );
    assert!(
        text.contains("a box marks a character no font can display"),
        "{screen}"
    );
    assert!(screen.contains("s preview"), "{screen}");
}

/// The font view shows each highlighted font as its detailed specimen, at
/// 12 points where nothing names a size, below the list and the line that
/// describes it, where the console cannot be switched live.
#[test]
fn each_highlighted_font_is_shown_as_its_specimen() {
    let mut c = Console::vtium(80, 25, false, "unifont").keys(&[UP, UP, UP, UP, b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    let fonts: Vec<(&str, u32)> = c
        .specimens
        .iter()
        .map(|(f, h, _)| (f.as_str(), *h))
        .collect();
    assert_eq!(
        fonts,
        [
            ("unifont", 16),
            ("tiny5", 16),
            ("proof", 16),
            ("minspec-16", 16)
        ]
    );
    // below the six fonts' rows and the line's rows, to the room above the
    // footer
    let (_, _, (w, h)) = c.specimens[0];
    assert_eq!((w, h), (79 * 8, 7 * 16));
    // the broken frit, which does not read, gets none: each picture is
    // deleted as the next font is highlighted
    assert_eq!(count(&c.written, &kitty::delete(kitty::IMAGE_ID)), 4);
    let mut live = console().keys(&[UP, b"q"]);
    live.graphics = true;
    run(&mut live);
    // a vtium console it can switch draws the preview as text instead
    assert!(live.specimens.is_empty(), "{:?}", live.specimens);
    assert_eq!(live.switches(), Vec::<String>::new());
}

/// Where the terminal draws no pictures, a size shows what it gives: its
/// cell, its grid and how much of the specimen the font draws itself.
#[test]
fn without_pictures_a_size_shows_its_cell_grid_and_coverage() {
    let mut c = outside().keys(&[BACKTAB, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains(
            "With this font and size, each character cell is 8x16 pixels, the console will be 80x30 characters, and the font can display 40 of the 51 characters in the preview."
        ),
        "{screen}"
    );
    // the Fonts view's line at 12 pt, then one summary a size, at 96 dpi:
    // 6 to 12 pt and the highlighted font
    let heights: Vec<u32> = c.summaries.iter().map(|(_, h)| *h).collect();
    assert_eq!(heights, [16, 8, 9, 11, 12, 13, 15, 16]);
    assert!(c.summaries.iter().all(|(f, _)| f == "unifont"));
    assert!(c.pictures.is_empty());
}

/// The sources query does not hold the keys up: while the helper has not
/// answered, a key switches the console and a signal leaves (ADR-1201 D3).
#[test]
fn keys_reach_the_tui_while_the_helper_finds_the_sources() {
    let mut c = console().keys(&[UP, b"t", b"k"]);
    c.hold_sources = true;
    // the script runs out with the answer still to come, and SIGTERM leaves
    assert_eq!(run(&mut c), 128 + 15);
    assert_eq!(
        c.sources_asked,
        [["minspec-16", "proof", "tiny5"].map(String::from).to_vec()]
    );
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("proof      reads\n"), "{screen}");
}

/// Each frit that reads is noted with the sources the helper says this
/// machine lacks, or that none of its sources is here.
#[test]
fn the_font_list_names_the_sources_a_frit_lacks() {
    let mut c = console().keys(&[]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("proof      reads, 1 font file missing"),
        "{screen}"
    );
    assert!(
        screen.contains("tiny5      no font file on this machine"),
        "{screen}"
    );
    assert!(screen.contains("minspec-16 reads\n"), "{screen}");
}

/// The text a run of the TUI left on the console's screen, from the row
/// below the caption of the preview: the preview's rows, trimmed.
fn preview_rows(screen: &str, first: &str) -> Vec<String> {
    screen
        .lines()
        .skip_while(|l| !l.contains(first))
        .map(|l| l.trim().to_owned())
        .collect()
}

/// In a vtium console the TUI can switch, the Fonts view prints the
/// detailed specimen's characters, its lines and the locale's line, as
/// the console's own text, in the font the console draws now, and asks
/// the helper for no picture; after `t` the console draws them in the font
/// tried, and the caption names it (ADR-1201 D4).
#[test]
fn a_vtium_console_prints_the_specimen_as_its_own_text_and_t_redraws_it() {
    let grid = (100, 50);
    let mut c = Console::vtium(grid.0, grid.1, true, "unifont")
        .takes("tiny5", grid)
        .takes("unifont", grid)
        .keys(&[UP, b"t", b"k"]);
    let mut i = inputs();
    i.locale = "ru_RU.UTF-8".into();
    drive(&mut c, i).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    // every line the picture of the specimen draws, in its order, with the
    // Russian locale's own line after the samples, then its line of
    // superscripts and subscripts
    let mut want: Vec<String> = vtium_config::specimen::detail_lines("ru_RU.UTF-8")
        .iter()
        .map(|l| l.trim().to_owned())
        .collect();
    assert!(
        want.iter().any(|l| l.starts_with("Съешь же ещё")),
        "{want:?}"
    );
    let script: String = vtium_config::specimen::SCRIPT_LINE
        .iter()
        .map(|(run, _)| *run)
        .collect();
    want.push(script.trim().to_owned());
    // the console's grid gives a wide character two cells, which the
    // screen's text shows as the character and an empty cell
    let shown: Vec<String> = preview_rows(&screen, "!\"#$%")
        .into_iter()
        .take(want.len())
        .map(|l| l.replace(' ', ""))
        .collect();
    let want: Vec<String> = want.iter().map(|l| l.replace(' ', "")).collect();
    assert_eq!(shown, want, "{screen}");
    // the superscripts and subscripts are SGR 73 and 74
    let text = String::from_utf8_lossy(&c.written).into_owned();
    assert!(text.contains("\x1b[73m2"), "a superscript");
    assert!(text.contains("\x1b[74m2"), "a subscript");
    // no picture, and no written description in its place
    assert!(c.specimens.is_empty() && c.summaries.is_empty() && c.cells.is_empty());
    assert!(!screen.contains("does not support images"), "{screen}");
    // the console draws tiny5 now, and the caption says so; leaving gives
    // unifont back
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    let flat = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flat.contains(&words::console_font_caption(Some("tiny5"))),
        "{screen}"
    );
    assert!(screen.contains(words::KEYS_FONTS_LIVE), "{screen}");
}

/// In a vtium console the Looks view prints the sample screen the picture
/// of a size draws, in its colors, at the console's current size, and its
/// caption says that a size takes effect when vtium starts again.
#[test]
fn a_vtium_console_prints_the_looks_sample_at_its_current_size() {
    let mut c = console().keys(&[BACKTAB, DOWN, DOWN]);
    c.graphics = true;
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let rows = preview_rows(&screen, "user@desk:~/notes$ ls -l");
    let sample: Vec<String> = vtium_config::tui::shot::sample_rows()
        .iter()
        .map(|line| line.iter().map(|(t, _)| *t).collect::<String>())
        .collect();
    for (i, line) in sample.iter().enumerate() {
        let row = rows.get(i).map(String::as_str).unwrap_or_default();
        assert!(
            row.ends_with(line.trim()),
            "row {i}: {row:?} for {line:?}\n{screen}"
        );
    }
    let flat = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains(words::CONSOLE_SIZE_CAPTION), "{screen}");
    // the colors are the console's own: the prompt in bright green
    let text = String::from_utf8_lossy(&c.written).into_owned();
    assert!(text.contains("\x1b[92muser@desk"), "the prompt's color");
    assert!(c.pictures.is_empty() && c.cells.is_empty());
}

/// Outside a vtium console the TUI can switch, the Fonts view keeps
/// today's preview: the written description where the terminal shows no
/// images, the helper's picture where it does, and `s` switches the
/// picture. It never prints the specimen as text.
#[test]
fn outside_a_vtium_console_the_preview_is_a_picture_or_a_description() {
    for graphics in [false, true] {
        let mut c = outside().keys(&[b"s"]);
        c.graphics = graphics;
        run(&mut c);
        let screen = c.seen.clone().expect("the screen");
        assert!(!screen.contains("@ABCDEFGHIJ"), "{screen}");
        assert!(screen.contains(words::KEYS_FONTS), "{screen}");
        if graphics {
            assert!(!c.specimens.is_empty() && !c.shots.is_empty());
        } else {
            assert!(screen.contains("does not support images"), "{screen}");
            assert!(!c.summaries.is_empty());
        }
    }
    // a terminal that is not vtium at all asks for the description too
    let mut c = Console::da1_only(80, 25).keys(&[]);
    run(&mut c);
    assert_eq!(c.summaries, [("unifont".to_owned(), 16)]);
}

/// A vtium console the TUI can switch gets no picture, even where it
/// draws kitty graphics: the preview is its own text (ADR-1201 D4), and a
/// font tried and kept still comes back when the TUI leaves.
#[test]
fn a_vtium_console_that_draws_pictures_gets_no_picture() {
    let mut c = console().keys(&[UP, b"t", b"k", BACKTAB, DOWN, b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    assert!(c.pictures.is_empty() && c.specimens.is_empty() && c.cells.is_empty());
    assert_eq!(count(&c.written, b"\x1b_Ga=T"), 0);
    let text = String::from_utf8_lossy(&c.written).into_owned();
    assert!(text.contains("\x1b]50;unifont\x1b\\"), "the font back");
}

#[test]
fn automatic_has_no_picture() {
    let mut c = outside().keys(&[BACKTAB, DOWN, UP]);
    c.graphics = true;
    run(&mut c);
    let heights: Vec<u32> = c.pictures.iter().map(|(_, h, _)| *h).collect();
    assert_eq!(heights, [8], "6 pt only");
    let screen = c.seen.clone().expect("the screen");
    assert!(
        screen.contains("Automatic lets vtium pick the size"),
        "{screen}"
    );
}

#[test]
fn without_kitty_graphics_no_picture_is_sent_and_the_view_says_so() {
    let mut c = outside().keys(&[BACKTAB, DOWN, DOWN]);
    run(&mut c);
    assert!(c.pictures.is_empty());
    assert_eq!(count(&c.written, b"\x1b_Ga=T"), 0);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("does not support images"), "{screen}");
}

#[test]
fn the_picture_never_reaches_the_footer_and_never_passes_the_cap() {
    // 80x25 at 8x16: rows 7 to 20 are the area, above the longest message
    let mut c = outside().keys(&[BACKTAB, DOWN]);
    c.graphics = true;
    run(&mut c);
    let (_, _, (w, h)) = c.pictures.last().cloned().expect("6 pt's picture");
    assert!(h <= (25 - 8) * 16 && w <= (80 - 25) * 8, "{w}x{h}");
    // a terminal of very large cells is capped
    let mut c = outside().keys(&[BACKTAB, DOWN]);
    c.graphics = true;
    c.cell_px = Some((200, 400));
    run(&mut c);
    let (_, _, max) = c.pictures.last().cloned().expect("6 pt's picture");
    assert_eq!(max, vtium_config::specimen::PICTURE_MAX);
}

/// Where the preview area cannot show one line of text at the size's
/// cell, the message line says so, and no sliver of a picture is sent.
#[test]
fn a_preview_area_less_than_one_line_high_says_so_on_the_message_line() {
    // cells of 2x1 pixels leave the Looks view's area 14 pixels high, and
    // the stand-in's 8x16 cell needs 16
    let mut c = outside().keys(&[BACKTAB, DOWN]);
    c.graphics = true;
    c.cell_px = Some((2, 1));
    run(&mut c);
    let (_, _, (_, h)) = c
        .pictures
        .last()
        .cloned()
        .expect("6 pt's picture asked for");
    assert!(h < 16, "{h}");
    // the Fonts view's detailed specimen, which the loop opens on, is the
    // only picture sent
    assert_eq!(count(&c.written, b"\x1b_Ga=T"), c.specimens.len());
    let screen = c.seen.clone().expect("the screen");
    let text = screen.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        text.contains("No preview image of the console: the preview area is too small to show one line of text at this size. Make this terminal larger, or choose a smaller size."),
        "{screen}"
    );
}

#[test]
fn display_dpi_outside_the_console_s_bounds_is_not_used() {
    use vtium_config::tui::density;
    let none = &[];
    assert_eq!(
        density(Some("display-dpi=150\n"), none),
        (150, DpiFrom::Conf)
    );
    assert_eq!(
        density(Some("display-dpi=1000\n"), none),
        (1000, DpiFrom::Conf)
    );
    assert_eq!(
        density(Some("display-dpi=1001\n"), none),
        (96, DpiFrom::Assumed)
    );
    assert_eq!(
        density(Some("display-dpi=19\n"), none),
        (96, DpiFrom::Assumed)
    );
    assert_eq!(
        density(Some("display-dpi=lots\n"), none),
        (96, DpiFrom::Assumed)
    );
    assert_eq!(density(None, none), (96, DpiFrom::Assumed));
    // a display's EDID comes after display-dpi and before the assumption:
    // the first display that states its size
    use vtium_config::display::Display;
    let displays = [
        Display {
            pixels: (1024, 768),
            mm: None,
        },
        Display {
            pixels: (2560, 1440),
            mm: Some((600, 340)),
        },
    ];
    assert_eq!(density(None, &displays), (108, DpiFrom::Edid));
    assert_eq!(
        density(Some("display-dpi=150\n"), &displays),
        (150, DpiFrom::Conf)
    );
}

#[test]
fn after_installing_a_font_leaving_puts_the_console_in_the_installed_font() {
    // try and keep tiny5 on the console, keep it in the draft, try proof
    // and keep it on the console, install, leave
    let mut c = console().keys(&[UP, b"t", b"k", b"\r", UP, b"t", b"k", b"i", b"y", b"q"]);
    let mut d = desk();
    assert_eq!(run_desk(&mut c, &mut d), 0);
    assert_eq!(
        d.plans[0].files[0].new,
        format!("# mine\nfont-name=tiny5\n{SECTIONS}")
    );
    assert_eq!(c.switches(), ["tiny5", "proof", "tiny5"]);
}

/// Below 80 by 25 cells no view is drawn: the screen names the size the
/// TUI needs and this one's, the commands, which work at any size, and the
/// keys; `u` puts back the font a preview changed, and the views come back
/// with the grid.
#[test]
fn below_the_minimum_the_screen_says_so_and_u_undoes_the_preview() {
    use vtium_config::tui::words;
    for grid in [(79, 30), (80, 24)] {
        let mut c = Console::vtium(80, 25, true, "unifont")
            .takes("unifont", (80, 25))
            .takes("tiny5", grid)
            .keys(&[UP, b"t", b"k"]);
        run(&mut c);
        let screen = c.seen.clone().expect("the screen");
        let joined = screen.lines().map(str::trim).collect::<Vec<_>>().join(" ");
        assert!(
            joined.contains(&words::too_small(grid.0, grid.1)),
            "{grid:?}: {screen}"
        );
        assert!(joined.contains("vtium-config fonts"), "{screen}");
        assert!(joined.contains("vtium-config set"), "{screen}");
        assert!(joined.contains(words::TOO_SMALL_KEYS), "{screen}");
        // keys that would move are ignored below the minimum; u gives the
        // console its start font back, and the views come back
        let mut c = Console::vtium(80, 25, true, "unifont")
            .takes("unifont", (80, 25))
            .takes("tiny5", grid)
            .takes("proof", (80, 25))
            .keys(&[UP, b"t", b"k", UP, b"u"]);
        run(&mut c);
        assert_eq!(c.switches(), ["tiny5", "unifont"], "{grid:?}");
        let screen = c.seen.clone().expect("the screen");
        assert!(screen.contains("Fonts"), "{screen}");
        assert!(!screen.contains("vtium-config needs"), "{screen}");
    }
}

/// The Fonts view's key line fits on one row at 80 columns, the key that
/// leaves included.
#[test]
fn the_fonts_key_line_is_one_row_at_80_columns() {
    let screen = reach(80, 25, &[]);
    let rows: Vec<&str> = screen.lines().collect();
    assert_eq!(rows.len(), 25);
    assert!(rows[24].starts_with(" Enter choose"), "{screen}");
    assert!(rows[24].ends_with("q quit"), "{screen}");
    assert!(rows[23].starts_with("---"), "the rule just above: {screen}");
}

#[test]
fn at_80x25_no_key_or_sentence_is_cut() {
    use vtium_config::tui::words;
    // the keys that reach a view, its key line, and its sentences
    type Case<'a> = (&'a [&'a [u8]], &'a str, &'a [&'a str]);
    let views: [Case; 4] = [
        (&[], words::KEYS_FONTS_LIVE, &[]),
        (
            &[BACKTAB],
            words::KEYS_LOOKS,
            &[words::CONSOLE_SIZE_CAPTION],
        ),
        (&[b"\t"], words::KEYS_SETTINGS, &[words::SETTINGS_HINT]),
        (&[b"\t", b"\t"], words::KEYS_HELP, &[]),
    ];
    for (keys, key_line, sentences) in views {
        let screen = reach(80, 25, keys);
        let rows: Vec<&str> = screen.lines().collect();
        assert_eq!(rows.len(), 25);
        for item in key_line.split("  ") {
            assert!(
                rows.iter().any(|r| r.contains(item)),
                "{item:?} whole:\n{screen}"
            );
        }
        // a sentence wrapped by words reads whole once its rows are joined
        let joined = rows.iter().map(|r| r.trim()).collect::<Vec<_>>().join(" ");
        for sentence in sentences {
            assert!(joined.contains(sentence), "{sentence:?}:\n{screen}");
        }
        // the views and the actions fit the tab bar
        for name in ["Flow", "Looks", "Fonts", "Settings", "Help"] {
            assert!(rows[0].contains(name), "{name}: {:?}", rows[0]);
        }
        // the Help view shows no actions (ADR-1403 D7)
        if key_line != words::KEYS_HELP {
            for (_, short) in words::ACTIONS {
                assert!(rows[0].contains(short), "{short}: {:?}", rows[0]);
            }
        }
    }
}

#[test]
fn an_edit_vtium_conf_cannot_take_is_said_in_words() {
    use vtium_config::tui::words::conf_edit;
    use vtium_settings::ConfEdit;
    let key = conf_edit(&ConfEdit::BadKey("Font".into()));
    assert!(key.contains("Font") && !key.contains("BadKey"), "{key}");
    let value = conf_edit(&ConfEdit::BadValue {
        key: "font-name".into(),
        value: "a#b".into(),
    });
    assert!(
        value.contains("font-name") && value.contains("a#b") && !value.contains("BadValue"),
        "{value}"
    );
}

#[test]
fn at_80x25_the_size_list_scrolls_and_its_sentence_shows_whole_or_not_at_all() {
    use vtium_config::tui::words;
    let joined = |screen: &str| screen.lines().map(str::trim).collect::<Vec<_>>().join(" ");
    let top = reach(80, 25, &[BACKTAB]);
    assert!(joined(&top).contains(words::CONSOLE_SIZE_CAPTION), "{top}");
    assert!(
        !top.contains("32 pt"),
        "the list is longer than its room: {top}"
    );
    // the last size scrolls into sight, and the sentence stays whole
    let bottom = reach(80, 25, &[BACKTAB, b"\x1b[4~"]);
    assert!(bottom.contains("> 32 pt"), "{bottom}");
    let first = words::CONSOLE_SIZE_CAPTION.split(". ").next().unwrap_or("");
    assert_eq!(
        joined(&bottom).contains(first),
        joined(&bottom).contains(words::CONSOLE_SIZE_CAPTION),
        "whole or not at all:\n{bottom}"
    );
}

#[test]
fn a_refused_frit_s_long_reason_wraps_by_words() {
    let reason = "line 2: unknown key colour, which a frit spells color, as vtium.conf spells it in every key";
    let mut inputs = inputs();
    inputs.fonts[0] = font("broken", FontState::Refused(reason.into()));
    let mut c = console();
    drive(&mut c, inputs).expect("the loop");
    let screen = c.seen.clone().expect("the screen");
    let joined = screen.lines().map(str::trim).collect::<Vec<_>>().join(" ");
    assert!(
        joined.contains(&format!("cannot be read: {reason}")),
        "{screen}"
    );
    assert!(screen.lines().all(|l| l.chars().count() <= 80));
    // the reason goes on below its name's row, under its column
    let continued: Vec<&str> = screen
        .lines()
        .filter(|l| l.starts_with("              ") && !l.trim().is_empty())
        .collect();
    assert_eq!(continued.len(), 1, "{screen}");
}

// The Flow view (ADR-1403 D7), on the flow of the record's figure.

mod flow_view {
    use super::*;
    use vtium_config::tui::flow::Item;
    use vtium_config::tui::input::{Button, Input, Key, Mouse};
    use vtium_config::tui::model::{App, Effect, Event, MOUSE_OFF, MOUSE_ON, View};
    use vtium_config::tui::view;
    use vtium_config::tui::words;

    fn app(cols: usize, rows: usize) -> App {
        let mut i = inputs();
        i.view = View::Flow;
        App::new(i, cols, rows).0
    }

    fn press(app: &mut App, key: Key) -> Vec<Effect> {
        app.event(Event::Input(Input::Key(key)))
    }

    fn text(app: &App) -> String {
        view::as_text(&view::screen(app))
    }

    fn writes(effects: &[Effect], bytes: &[u8]) -> bool {
        effects.iter().any(|e| *e == Effect::Write(bytes.to_vec()))
    }

    #[test]
    fn mouse_reporting_is_on_in_the_flow_view_alone() {
        let mut i = inputs();
        i.view = View::Flow;
        let (_, first) = App::new(i, 80, 25);
        assert_eq!(first[0], Effect::Write(model::probe()));
        assert!(writes(&first, MOUSE_ON), "{first:?}");
        let (_, first) = App::new(inputs(), 80, 25);
        assert!(!writes(&first, MOUSE_ON), "{first:?}");
        // leaving the TUI from the flow turns it off
        let mut a = app(80, 25);
        let out = press(&mut a, Key::Char('q'));
        assert!(
            out.iter()
                .any(|e| matches!(e, Effect::Write(b) if b.ends_with(MOUSE_OFF))),
            "{out:?}"
        );
        assert_eq!(out.last(), Some(&Effect::Exit(0)));
    }

    /// Types `bytes` into `app` through the TUI's own key decoder, as the
    /// loop does, and returns what the app asked the loop to do.
    fn type_in(app: &mut App, bytes: &[u8]) -> Vec<Effect> {
        let mut decoder = vtium_config::tui::input::Decoder::keys();
        decoder
            .feed(bytes)
            .into_iter()
            .flat_map(|input| app.event(Event::Input(input)))
            .collect()
    }

    const TAB: &[u8] = b"\t";
    const RIGHT: &[u8] = b"\x1b[C";

    // Tab and Shift-Tab step the views wherever the selection is, and the
    // arrows move inside a view (#355)
    #[test]
    fn tab_and_shift_tab_step_the_views_from_every_box_of_the_flow() {
        for boxes in 0..6 {
            for (key, next) in [(TAB, View::Looks), (BACKTAB, View::Help)] {
                let mut a = app(80, 25);
                for _ in 0..boxes {
                    type_in(&mut a, RIGHT);
                }
                let at = a.flow_view.at.clone();
                assert!(at.is_some());
                let out = type_in(&mut a, key);
                assert_eq!(a.view, next, "{key:?} from box {boxes}");
                assert!(writes(&out, MOUSE_OFF), "{out:?}");
                // and back: the flow keeps the box that was highlighted
                let back = if key == TAB { BACKTAB } else { TAB };
                let out = type_in(&mut a, back);
                assert_eq!((a.view, &a.flow_view.at), (View::Flow, &at), "box {boxes}");
                assert!(writes(&out, MOUSE_ON), "{out:?}");
            }
        }
    }

    #[test]
    fn the_arrows_and_the_keys_that_move_in_a_column_never_leave_the_flow() {
        let mut a = app(80, 25);
        let keys: [&[u8]; 8] = [
            b"\x1b[D", b"\x1b[C", b"\x1b[A", b"\x1b[B", b"\x1b[5~", b"\x1b[6~", b"\x1b[H",
            b"\x1b[F",
        ];
        for key in keys {
            for _ in 0..8 {
                type_in(&mut a, key);
                assert_eq!(a.view, View::Flow, "{key:?}");
            }
        }
    }

    /// The flow with a draft, so a card offers fields to change.
    fn drafted(cols: usize, rows: usize) -> App {
        let mut a = app(cols, rows);
        a.draft = Some(vtium_config::tui::desk::Opened {
            name: "mine".into(),
            text: inputs().conf.expect("the fixture's vtium.conf"),
            ..Default::default()
        });
        a
    }

    #[test]
    fn tab_and_shift_tab_leave_nothing_open_behind() {
        // a box's detail, then the file view: Tab and Shift-Tab do nothing
        // while each is open (ADR-1403 D7, The keys)
        let mut a = drafted(80, 25);
        type_in(&mut a, b"\r");
        let layer = a.flow_view.layer.clone();
        assert!(layer.is_some(), "{}", text(&a));
        for key in [TAB, BACKTAB] {
            type_in(&mut a, key);
            assert_eq!((a.view, &a.flow_view.layer), (View::Flow, &layer));
        }
        type_in(&mut a, b"\x1b");
        assert!(a.flow_view.layer.is_none());
        type_in(&mut a, b"v");
        assert!(a.pager.is_some());
        for key in [TAB, BACKTAB] {
            type_in(&mut a, key);
            assert_eq!((a.view, a.pager.is_some()), (View::Flow, true));
        }
    }

    #[test]
    fn a_click_on_a_view_s_name_leaves_nothing_open_behind() {
        let mut a = drafted(80, 25);
        let looks = cell(&a, "Looks");
        type_in(&mut a, b"v");
        click(&mut a, looks);
        assert_eq!((a.view, a.pager.is_some()), (View::Flow, true));
        let mut a = drafted(80, 25);
        type_in(&mut a, b"\r");
        assert!(a.flow_view.layer.is_some());
        click(&mut a, looks);
        assert_eq!((a.view, a.flow_view.layer.is_some()), (View::Flow, true));
        // with nothing open, the click switches
        let mut a = drafted(80, 25);
        click(&mut a, looks);
        assert_eq!(a.view, View::Looks);
    }

    /// The cell where `needle` starts on the screen of `app`.
    fn cell(app: &App, needle: &str) -> (usize, usize) {
        text(app)
            .lines()
            .enumerate()
            .find_map(|(y, l)| {
                l.find(needle)
                    .map(|b| (l.get(..b).unwrap_or_default().chars().count(), y))
            })
            .unwrap_or_else(|| panic!("{needle} on the screen:\n{}", text(app)))
    }

    fn click(app: &mut App, (col, row): (usize, usize)) -> Vec<Effect> {
        press(
            app,
            Key::Mouse(Mouse {
                button: Button::Press(0),
                col,
                row,
            }),
        )
    }

    #[test]
    fn a_click_reaches_the_tui_through_its_decoder() {
        // SGR mouse, mode 1006: a press on DP-8's box, row 5 column 39
        // counted from 1, then Enter opens it
        let mut c = console().keys(&[b"\x1b[<0;39;5M", b"\x1b[<0;39;5m", b"\r"]);
        let mut i = inputs();
        i.view = View::Flow;
        drive(&mut c, i).expect("the loop");
        let written = String::from_utf8_lossy(&c.written).into_owned();
        assert!(written.contains("\x1b[?1000h\x1b[?1006h"), "turned on");
        let screen = c.seen.clone().expect("the screen");
        // the second press on the highlighted box opens its detail
        assert!(screen.contains("DISPLAY DP-8"), "{screen}");
    }

    #[test]
    fn without_a_draft_the_action_keys_say_so_and_change_nothing() {
        let mut a = app(80, 25);
        for key in ['d', 'v', 'r', 'o', 'n', 'x', 'i'] {
            let out = press(&mut a, Key::Char(key));
            assert_eq!(a.message.as_deref(), Some(words::NO_DESK), "{key}");
            assert_eq!(a.view, View::Flow);
            assert!(out.is_empty(), "{key}: {out:?}");
        }
    }

    /// Every SGR parameter `bytes` sets.
    fn sgr(bytes: &[u8]) -> Vec<Vec<u16>> {
        let text = String::from_utf8_lossy(bytes);
        text.split("\x1b[")
            .skip(1)
            .filter_map(|s| {
                let end = s.find(|c: char| ('@'..='~').contains(&c))?;
                (s.get(end..=end) == Some("m")).then(|| {
                    s.get(..end)
                        .unwrap_or_default()
                        .split(';')
                        .filter_map(|p| p.parse().ok())
                        .collect()
                })
            })
            .collect()
    }

    #[test]
    fn the_views_draw_in_the_sixteen_colors_with_no_blue() {
        let mut a = app(100, 30);
        let mut screens = vec![view::draw(&a)];
        press(&mut a, Key::Enter);
        screens.push(view::draw(&a));
        press(&mut a, Key::Escape);
        for _ in 0..4 {
            press(&mut a, Key::BackTab);
            screens.push(view::draw(&a));
        }
        for bytes in &screens {
            for params in sgr(bytes) {
                for p in &params {
                    assert!(![34, 44, 94, 104].contains(p), "blue in {params:?}");
                }
                // 256-color and direct color are not used either
                assert!(!params.contains(&38) && !params.contains(&48), "{params:?}");
            }
        }
        // each kind's title in its color: displays cyan, terminals yellow,
        // inputs green (ADR-1403 D7, Colors, borders and lines)
        let flow = sgr(&screens[0]);
        for color in [36, 33, 32] {
            assert!(
                flow.iter().any(|p| p.last() == Some(&color)),
                "{color}: {flow:?}"
            );
        }
    }

    #[test]
    fn without_color_the_view_draws_no_color_and_keeps_its_words() {
        let mut i = inputs();
        i.view = View::Flow;
        i.color = false;
        let (a, _) = App::new(i, 80, 25);
        let bytes = view::draw(&a);
        for params in sgr(&bytes) {
            assert!(
                params
                    .iter()
                    .all(|p| !(30..=49).contains(p) && !(90..=107).contains(p)),
                "{params:?}"
            );
        }
        let screen = text(&a);
        // the boxes keep their borders and their words
        for want in ["CONFIGURATION", "DP-8", "┌", "┘"] {
            assert!(screen.contains(want), "{want}: {screen}");
        }
    }

    #[test]
    fn a_rule_to_a_head_that_does_not_exist_belongs_to_no_path_and_says_so() {
        let mut i = inputs();
        i.view = View::Flow;
        i.flow.rules[0].target = "tv".into();
        let (a, _) = App::new(i, 100, 30);
        let screen = text(&a);
        assert!(
            screen.contains("1 rule belongs to no path. v shows it in the file."),
            "{screen}"
        );
    }

    #[test]
    fn below_80x25_the_flow_view_draws_the_way_out() {
        for (cols, rows) in [(79, 25), (80, 24)] {
            let mut a = app(cols, rows);
            let screen = text(&a);
            assert!(screen.starts_with("This terminal is too small"), "{screen}");
            // the flow's keys do nothing there; q still leaves
            press(&mut a, Key::Right);
            assert_eq!(a.flow_at, Item::Display(0));
            let out = press(&mut a, Key::Char('q'));
            assert_eq!(out.last(), Some(&Effect::Exit(0)));
        }
    }

    #[test]
    fn the_help_view_draws_the_keys_from_the_catalog_and_about_from_its_source() {
        let mut a = app(80, 25);
        a.flow_at = Item::Display(0);
        press(&mut a, Key::BackTab);
        assert_eq!(a.view, View::Help);
        let joined = |a: &App| text(a).lines().map(str::trim).collect::<Vec<_>>().join(" ");
        assert!(
            joined(&a).contains("Flow shows how the console is set up"),
            "{}",
            text(&a)
        );
        press(&mut a, Key::End);
        let end = joined(&a);
        // a command's line wider than the screen goes on under it
        assert!(
            end.contains("report [--save[=PATH]] [--lines N] [--previous-boot]"),
            "{end}"
        );
        assert!(
            end.contains("collect what a bug report needs into one document"),
            "{end}"
        );
        assert!(text(&a).lines().all(|l| l.chars().count() <= 80));
        press(&mut a, Key::Right);
        assert!(a.about);
        assert!(
            joined(&a).contains("vtium-config sets up the vtium console"),
            "{}",
            text(&a)
        );
    }
}

/// The Flow view on a draft (ADR-1403 D7, D8), driven through the real
/// loop with keys: a configuration directory standing in for
/// `/etc/vtium`, the store under a directory of the tests' own home, a
/// stand-in `vtium` whose `check-config` refuses each line that holds
/// `BAD`, and devices the test gives the desk instead of the machine's.
/// Each test reads the draft's text after its keys.
mod draft_flow {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use vtium_config::flowmodel::Present;
    use vtium_config::tui::desk;
    use vtium_config::tui::port::drive_with;
    use vtium_config::tui::words;
    use vtium_settings::{Class, Identity};

    use vtium_config::tui::fixture::{CHECKER, DRAFT as INSTALLED};

    const RIGHT: &[u8] = b"\x1b[C";
    const LEFT: &[u8] = b"\x1b[D";
    const ESC: &[u8] = b"\x1b";

    /// A fixture under the tests' own home.
    /// The screen's words with each run of spaces and line ends as one
    /// space, so a sentence the message line wraps reads whole.
    fn flat(screen: &str) -> String {
        screen.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// A change on the Flow view: the first path's terminal, desk, renamed
    /// deskx on its card.
    const CHANGE: [&[u8]; 7] = [b"\x1b[H", RIGHT, b"\r", b"\r", b"x", b"\r", ESC];

    struct Bench {
        root: PathBuf,
    }

    fn bench(name: &str) -> Bench {
        let home = home::vars()
            .into_iter()
            .find(|(k, _)| *k == "HOME")
            .map(|(_, v)| v)
            .expect("the tests' home");
        let root = home.join("tui-draft").join(name);
        if root.exists() {
            std::fs::remove_dir_all(&root).expect("a fresh fixture");
        }
        for dir in ["etc/fonts", "home", "sys"] {
            std::fs::create_dir_all(root.join(dir)).expect("the fixture");
        }
        std::fs::write(root.join("etc/vtium.conf"), INSTALLED).expect("vtium.conf");
        let checker = root.join("vtium");
        script::executable(&checker, CHECKER);
        Bench { root }
    }

    /// The keyboard at `pci-0:3.3`, which rule 2 names.
    fn keyboard() -> Identity {
        let mut k = Identity::input().with_class(Class::Keyboard);
        k.name = Some("kbd".into());
        k.path = Some("pci-0:3.3".into());
        k
    }

    /// A second keyboard no rule names alone.
    fn other_keyboard() -> Identity {
        let mut k = Identity::input().with_class(Class::Keyboard);
        k.name = Some("spare".into());
        k.path = Some("pci-0:9.9".into());
        k
    }

    fn present(devices: Vec<Identity>, udev: bool) -> Present {
        Present { devices, udev }
    }

    impl Bench {
        fn store(&self) -> vtium_config::store::Store {
            let root = self.root.clone();
            vtium_config::store::Store::locate(
                1000,
                move |name: &str| match name {
                    "HOME" => Some(root.join("home").into()),
                    "XDG_CONFIG_HOME" => Some(root.join("home/config").into()),
                    "XDG_STATE_HOME" => Some(root.join("home/state").into()),
                    _ => None,
                },
                |_| None,
            )
            .expect("the store")
        }

        fn paths(&self) -> vtium_config::tui::Paths {
            vtium_config::tui::Paths {
                configdir: self.root.join("etc"),
                frits: Ok(self.root.join("etc/fonts")),
                drafts: self.root.join("home/bundles"),
                sudo: "sudo".into(),
                vtium: self.root.join("vtium").into(),
                helper: vtium_config::specimen::Helper {
                    program: "/bin/false".into(),
                    options: Vec::new(),
                },
                sys: self.root.join("sys"),
            }
        }

        /// The desk, whose devices are each of `seen` in turn, the last
        /// for every read after. Its draft is `bench`, made from the
        /// installed file the first time, so no screen shows the
        /// machine's hostname.
        fn desk(&self, seen: Vec<Present>) -> desk::Store {
            let paths = self.paths();
            let store = self.store();
            if store.settings().expect("the settings").draft.is_none() {
                vtium_config::drafts::Session {
                    store: store.clone(),
                    paths: &paths,
                }
                .make("bench", vtium_config::drafts::From::Installed)
                .expect("the draft bench");
            }
            let queue = Rc::new(RefCell::new(VecDeque::from(seen)));
            let last = Rc::new(RefCell::new(Present::default()));
            let devices: desk::Devices = Box::new(move || {
                let next = queue.borrow_mut().pop_front();
                if let Some(next) = next {
                    *last.borrow_mut() = next;
                }
                last.borrow().clone()
            });
            desk::Store::new(self.store(), paths, devices)
        }

        /// The current draft's text.
        fn draft(&self) -> String {
            let store = self.store();
            let name = store
                .settings()
                .expect("the settings")
                .draft
                .expect("a draft");
            store
                .read_draft(&name)
                .expect("the draft")
                .expect("the draft's file")
        }

        fn installed(&self) -> String {
            std::fs::read_to_string(self.root.join("etc/vtium.conf")).expect("vtium.conf")
        }
    }

    /// What a run on the bench left: the console and the app's last screen.
    struct Ran {
        console: Console,
    }

    impl Ran {
        fn screen(&self) -> &str {
            self.console.seen.as_deref().unwrap_or_default()
        }

        /// The screen's words with one space between each, so a message
        /// reads whole wherever its rows wrap: a message that names the
        /// bench's path wraps where the path's length puts it.
        fn said(&self) -> String {
            self.screen()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        }

        fn status(&self) -> String {
            self.screen().lines().nth(1).unwrap_or_default().to_owned()
        }
    }

    /// Runs the TUI on `b`'s desk at 100x30 with the devices `seen`,
    /// typing `keys`.
    fn run_on(b: &Bench, seen: Vec<Present>, hotplug: bool, keys: &[&[u8]]) -> Ran {
        run_at(b, (100, 30), seen, hotplug, keys)
    }

    fn run_at(
        b: &Bench,
        (cols, rows): (usize, usize),
        seen: Vec<Present>,
        hotplug: bool,
        keys: &[&[u8]],
    ) -> Ran {
        let mut console = Console::vtium(cols, rows, true, "unifont")
            .takes("proof", (cols, rows))
            .takes("unifont", (cols, rows))
            .keys(keys);
        console.installs = true;
        let mut desk = b.desk(seen);
        let mut i = inputs();
        i.view = model::View::Flow;
        i.hotplug = hotplug;
        i.flow = vtium_config::tui::flow::Flow::default();
        drive_with(&mut console, i, Some(&mut desk)).expect("the loop");
        Ran { console }
    }

    fn run(b: &Bench, keys: &[&[u8]]) -> Ran {
        run_on(b, vec![present(vec![keyboard()], true)], true, keys)
    }

    /// Runs the TUI on `b`'s desk at 100x30 in the Fonts view of a vtium
    /// console of the head `head`, which draws unifont and takes tiny5 and
    /// a font set named BADX, which the bench's check refuses, typing
    /// `keys`.
    fn run_fonts(b: &Bench, head: Option<&str>, keys: &[&[u8]]) -> Ran {
        let grid = (100, 30);
        let mut console = Console::vtium(grid.0, grid.1, true, "unifont")
            .takes("tiny5", grid)
            .takes("unifont", grid)
            .takes("BADX", grid)
            .keys(keys);
        let mut desk = b.desk(vec![present(vec![keyboard()], true)]);
        let mut i = inputs();
        i.view = model::View::Fonts;
        i.head = head.map(str::to_owned);
        i.fonts.push(font("BADX", FontState::Reads));
        i.flow = vtium_config::tui::flow::Flow::default();
        drive_with(&mut console, i, Some(&mut desk)).expect("the loop");
        Ran { console }
    }

    /// The keys that try tiny5 on the console and keep it: from unifont,
    /// one row up.
    const TRY_TINY5: [&[u8]; 3] = [UP, b"t", b"k"];

    /// `s` in a vtium console's Fonts view saves the font the console
    /// shows where the console's head takes its font from. The bench's
    /// head desk draws the look big, which names proof, so the save makes
    /// a look for tiny5, gives it to desk, and removes big, which no head
    /// uses then, through the Flow card's plan; the top level's font-name
    /// stays unset (ADR-1201 D4, ADR-1403 D8).
    #[test]
    fn s_saves_the_tried_font_into_the_look_of_the_consoles_head() {
        let b = bench("tried-look");
        let ran = run_fonts(&b, Some("desk"), &[&TRY_TINY5[..], &[b"s"]].concat());
        let draft = b.draft();
        assert!(
            draft.contains("[look tiny5]\nfont-name = tiny5\n"),
            "{draft}"
        );
        assert!(draft.contains("[head desk]\nlook = tiny5\n"), "{draft}");
        assert!(!draft.contains("[look big]"), "{draft}");
        assert_eq!(
            vtium_settings::setting(&draft, "font-name"),
            None,
            "{draft}"
        );
        assert!(
            flat(ran.screen()).contains(&flat(&words::tried_saved_look(
                "tiny5", "desk", "tiny5", "bench"
            ))),
            "{}",
            ran.screen()
        );
        assert!(ran.status().contains("no problems"), "{}", ran.status());
        // the draft is the installed file until the save, which waits for
        // nothing else
        assert_ne!(draft, INSTALLED);
    }

    /// A head whose look names no font takes the top level's: the save
    /// writes `font-name` there and leaves the looks alone.
    #[test]
    fn s_saves_the_tried_font_at_the_top_level_for_a_head_whose_look_names_none() {
        let b = bench("tried-top");
        let ran = run_fonts(&b, Some("panel"), &[&TRY_TINY5[..], &[b"s"]].concat());
        let draft = b.draft();
        assert_eq!(
            vtium_settings::setting(&draft, "font-name").as_deref(),
            Some("tiny5"),
            "{draft}"
        );
        assert!(draft.contains("[look big]\nfont-name = proof\n"), "{draft}");
        assert!(draft.contains("[head desk]\nlook = big\n"), "{draft}");
        assert!(
            flat(ran.screen()).contains(&flat(&words::tried_saved_top("tiny5", "bench"))),
            "{}",
            ran.screen()
        );
        // without VTIUM_HEAD, where a look names a font of its own, the
        // TUI cannot tell where the console takes its font from, and
        // writes nothing
        let b = bench("tried-no-head");
        let ran = run_fonts(&b, None, &[&TRY_TINY5[..], &[b"s"]].concat());
        assert_eq!(b.draft(), INSTALLED);
        assert!(
            flat(ran.screen()).contains(&flat(&words::tried_no_head("tiny5"))),
            "{}",
            ran.screen()
        );
    }

    /// A save the check fails writes nothing: the draft stays as it was,
    /// and the message line shows what `vtium check-config` printed.
    #[test]
    fn s_refuses_a_tried_font_the_check_fails_and_shows_the_checks_words() {
        let b = bench("tried-refused");
        let end: &[u8] = b"\x1b[4~";
        let ran = run_fonts(&b, Some("panel"), &[end, b"t", b"k", b"s"]);
        assert_eq!(ran.console.switches(), ["BADX", "unifont"]);
        assert_eq!(b.draft(), INSTALLED);
        let said = flat(ran.screen());
        assert!(
            said.contains(&flat(words::TRIED_REFUSED)),
            "{}",
            ran.screen()
        );
        assert!(
            said.contains("BAD is not a key vtium reads"),
            "{}",
            ran.screen()
        );
    }

    #[test]
    fn the_view_opens_on_the_draft_and_its_state() {
        let b = bench("opens");
        let ran = run(&b, &[]);
        assert_eq!(b.draft(), INSTALLED);
        let status = ran.status();
        assert!(status.contains("· no changes · no problems"), "{status}");
        assert!(!status.contains(words::NO_HOTPLUG), "{status}");
        let screen = ran.screen();
        for want in ["desk", "panel", "proof", "kbd"] {
            assert!(screen.contains(want), "{want}: {screen}");
        }
    }

    #[test]
    fn install_saves_first_then_asks_with_its_counts_and_installs() {
        let b = bench("install");
        let asked = run(&b, &[&CHANGE[..], &[b"i"]].concat());
        // Install saves first, then asks (ADR-1403 D7)
        assert!(b.draft().contains("[head deskx]"), "{}", b.draft());
        assert!(
            flat(asked.screen()).contains("Install the draft? 4 lines added, 3 removed."),
            "{}",
            asked.screen()
        );
        assert!(asked.console.saves.is_empty());
        assert_eq!(b.installed(), INSTALLED);
        let ran = run(&b, &[b"i", b"y"]);
        assert_eq!(ran.console.saves.len(), 1);
        assert_eq!(b.installed(), b.draft());
        assert!(b.installed().contains("display * -> deskx"));
        assert!(ran.status().contains("no changes"), "{}", ran.status());
        let kept = b.store().history().expect("the history");
        assert_eq!(kept.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&kept[0].path).expect("the kept file"),
            INSTALLED
        );
        // and o restores it into the draft, after asking
        let ran = run(&b, &[b"o", b"\r"]);
        assert!(
            ran.screen()
                .contains("Replace the draft with earlier file 1?"),
            "{}",
            ran.screen()
        );
        run(&b, &[b"o", b"\r", b"y"]);
        assert_eq!(b.draft(), INSTALLED);
    }

    // ADR-1403 D7: Enter on a terminal shows its subflow, font set, then
    // what its look draws, then its displays
    #[test]
    fn a_terminal_s_card_shows_its_subflow() {
        let b = bench("subflow");
        let ran = run_at(
            &b,
            (120, 32),
            vec![present(vec![keyboard()], true)],
            true,
            &[b"\x1b[H", RIGHT, b"\r"],
        );
        let screen = ran.screen();
        assert!(screen.contains(words::SUBFLOW_TITLE), "{screen}");
        assert!(
            screen.contains("Font set proof  →  look big  →  All displays"),
            "{screen}"
        );
        assert!(flat(screen).contains(words::SUBFLOW_REQUIRES), "{screen}");
    }

    #[test]
    fn a_change_waits_for_save_and_s_saves_it() {
        let b = bench("save");
        let ran = run(&b, &CHANGE);
        assert!(ran.status().contains("1 not saved"), "{}", ran.status());
        // q, then n: the TUI leaves without the change it never saved
        let b = bench("save-n");
        run(&b, &[&CHANGE[..], &[b"q", b"n"]].concat());
        assert_eq!(b.draft(), INSTALLED, "the change waits for Save");
        // the file view shows the change before it is saved
        let ran = run(&b, &[&CHANGE[..], &[b"v"]].concat());
        assert!(ran.screen().contains("[head deskx]"), "{}", ran.screen());
        let b = bench("save-s");
        let ran = run(&b, &[&CHANGE[..], &[b"s"]].concat());
        assert!(b.draft().contains("[head deskx]"), "{}", b.draft());
        assert!(
            ran.said().contains("Saved 1 change into draft bench."),
            "{}",
            ran.screen()
        );
        assert!(!ran.status().contains("not saved"), "{}", ran.status());
    }

    #[test]
    fn q_with_unsaved_changes_asks_and_ctrl_c_saves_them() {
        let b = bench("quit");
        let ran = run(&b, &[&CHANGE[..], &[b"q"]].concat());
        assert!(
            flat(ran.screen()).contains(
                "1 change is not saved. Save them before you quit? y saves them and quits, n quits without them, and Esc goes back."
            ),
            "{}",
            ran.screen()
        );
        assert!(ran.screen().contains(words::KEYS_QUIT), "{}", ran.screen());
        // n quits without the change, and y saves it and quits
        let b = bench("quit-n");
        run(&b, &[&CHANGE[..], &[b"q", b"n"]].concat());
        assert_eq!(b.draft(), INSTALLED);
        let b = bench("quit-y");
        run(&b, &[&CHANGE[..], &[b"q", b"y"]].concat());
        assert!(b.draft().contains("[head deskx]"), "{}", b.draft());
        // Esc goes back, and the TUI stays
        let b = bench("quit-esc");
        let ran = run(&b, &[&CHANGE[..], &[b"q", ESC]].concat());
        assert!(ran.status().contains("1 not saved"), "{}", ran.status());
        // Ctrl-C saves before it leaves, and so does SIGTERM
        let b = bench("quit-ctrl-c");
        run(&b, &[&CHANGE[..], &[b"\x03"]].concat());
        assert!(b.draft().contains("[head deskx]"), "{}", b.draft());
        let b = bench("quit-sigterm");
        run(&b, &CHANGE);
        assert!(b.draft().contains("[head deskx]"), "{}", b.draft());
    }

    /// Restore's list and a question stay open on Tab and Shift-Tab: the
    /// list does nothing with them, and the question takes them as no.
    #[test]
    fn tab_leaves_neither_restore_s_list_nor_a_question_open_behind() {
        let b = bench("tab-restore");
        run(&b, &[&CHANGE[..], &[b"i", b"y"]].concat());
        for key in [TAB, BACKTAB] {
            let ran = run(&b, &[b"o", key]);
            let line = ran.screen().lines().last().unwrap_or_default().trim();
            assert_eq!(line, words::KEYS_RESTORE, "{key:?}: {}", ran.screen());
        }
        let ran = run(&b, &[b"o", TAB, BACKTAB, b"\r"]);
        assert!(
            ran.said()
                .contains("Replace the draft with earlier file 1?"),
            "{}",
            ran.screen()
        );
        // revert asks, and Tab answers no on the flow
        let b = bench("tab-revert");
        let ran = run(&b, &[&CHANGE[..], &[b"s", b"r", TAB]].concat());
        assert!(ran.said().contains(words::NOT_DONE), "{}", ran.screen());
        assert!(ran.status().contains("4 changes"), "{}", ran.status());
        let line = ran.screen().lines().last().unwrap_or_default().trim();
        assert!(line.ends_with("q quit"), "{}", ran.screen());
    }

    #[test]
    fn install_refuses_a_draft_made_before_the_installed_file_changed() {
        let b = bench("install-stale");
        // the draft made, then root edits the installed file by hand
        run(&b, &[&CHANGE[..], &[b"s"]].concat());
        let edited = INSTALLED.replace("font-size=12", "font-size=14");
        std::fs::write(b.root.join("etc/vtium.conf"), &edited).expect("the hand edit");
        let ran = run(&b, &[b"i"]);
        assert!(ran.said().contains("changed after"), "{}", ran.screen());
        // and no question asked, so a y installs nothing
        let ran = run(&b, &[b"i", b"y"]);
        assert!(ran.console.saves.is_empty());
        assert_eq!(b.installed(), edited);
        assert!(b.draft().contains("display * -> deskx"), "{}", b.draft());
    }

    // ADR-1403 D7: a draft whose only problem is a missing complete route
    // installs only after Install's own question, and a no installs nothing
    #[test]
    fn install_asks_before_it_installs_a_draft_with_no_complete_path() {
        let b = bench("install-no-route");
        let message = vtium_settings::Missing::NoDisplayRule
            .to_string()
            .replace('\'', r"'\''");
        let checker = CHECKER.replace(
            "if [ -n \"$bad\" ]; then",
            &format!(
                "if grep -q NOROUTE \"$file\"; then\n  printf '%s\\n' \"$file: \"'{message}' >&2\n  [ -n \"$bad\" ] || exit 1\nfi\nif [ -n \"$bad\" ]; then"
            ),
        );
        script::executable(&b.root.join("vtium"), &checker);
        run(&b, &[&CHANGE[..], &[b"s"]].concat());
        let unrouted = format!("# NOROUTE\n{}", b.draft());
        let path = b.store().draft_path("bench").expect("the draft's path");
        std::fs::write(path, &unrouted).expect("the draft");
        let asked = run(&b, &[b"i"]);
        assert!(
            flat(asked.screen()).contains(&flat(&words::no_route_question(5, 3, 0))),
            "{}",
            asked.screen()
        );
        let no = run(&b, &[b"i", b"n"]);
        assert!(no.console.saves.is_empty());
        assert_eq!(b.installed(), INSTALLED);
        let ran = run(&b, &[b"i", b"y"]);
        assert_eq!(ran.console.saves.len(), 1);
        assert_eq!(b.installed(), unrouted);
    }

    #[test]
    fn install_refuses_a_draft_the_check_fails() {
        let b = bench("install-refused");
        // a draft the stand-in check refuses: it names BAD
        run(&b, &[&CHANGE[..], &[b"s"]].concat());
        let bad = b.draft().replace("font-size=12", "font-name=BAD");
        let path = b.store().draft_path("bench").expect("the draft's path");
        std::fs::write(path, &bad).expect("the draft");
        let ran = run(&b, &[b"i"]);
        assert!(ran.status().contains("1 problem"), "{}", ran.status());
        assert!(
            ran.screen().contains("the check found problems"),
            "{}",
            ran.screen()
        );
        assert!(ran.console.saves.is_empty());
        assert_eq!(b.installed(), INSTALLED);
    }

    #[test]
    fn revert_and_new_each_ask_first() {
        let b = bench("revert");
        run(&b, &[&CHANGE[..], &[b"s"]].concat());
        assert_ne!(b.draft(), INSTALLED);
        let ran = run(&b, &[b"r", b"n"]);
        assert!(ran.screen().contains(words::NOT_DONE), "{}", ran.screen());
        assert_ne!(b.draft(), INSTALLED);
        // the question counts an unsaved change among what is lost
        let unsaved = bench("revert-unsaved");
        let ran = run(&unsaved, &[&CHANGE[..], &[b"r"]].concat());
        assert!(
            flat(ran.screen()).contains("This includes 1 change that is not saved."),
            "{}",
            ran.screen()
        );
        run(&b, &[b"r", b"y"]);
        assert_eq!(b.draft(), INSTALLED);
        run(&b, &[b"n", b"y"]);
        assert_eq!(
            b.draft(),
            vtium_config::ops::new_text(vtium_config::drafts::HEADER)
        );
    }

    #[test]
    fn the_file_view_opens_at_the_highlighted_box_s_line() {
        let b = bench("pager");
        // the first path's display, All displays: its rule on line 9
        let ran = run(&b, &[b"\x1b[H", b"v"]);
        let screen = ran.screen();
        assert!(screen.contains(words::PAGER_TITLE), "{screen}");
        assert!(screen.contains("▸   9 display * -> desk"), "{screen}");
        assert!(screen.contains("q back"), "{screen}");
        // q goes back to the flow, and leaving still works
        let ran = run(&b, &[b"v", b"j", b"G", b"\x04", b"q"]);
        assert!(!ran.screen().contains(words::PAGER_TITLE));
    }

    /// Runs the Flow view on `b`'s desk at 100x30, typing `keys`.
    fn run_rows(b: &Bench, keys: &[&[u8]]) -> Ran {
        let mut console = Console::vtium(100, 30, true, "unifont")
            .takes("proof", (100, 30))
            .takes("unifont", (100, 30))
            .keys(keys);
        console.installs = true;
        let mut desk = b.desk(vec![present(vec![keyboard()], true)]);
        let mut i = inputs();
        i.view = model::View::Flow;
        i.hotplug = true;
        i.flow = vtium_config::tui::flow::Flow::default();
        drive_with(&mut console, i, Some(&mut desk)).expect("the loop");
        Ran { console }
    }

    #[test]
    fn the_file_view_saves_and_installs_and_stays_open() {
        let b = bench("file-view");
        let ran = run_rows(&b, &[b"v"]);
        let screen = ran.screen();
        assert!(screen.contains(words::PAGER_TITLE), "{screen}");
        assert!(screen.contains("s save  i install"), "{screen}");
        // s says what Save says, and the file stays open
        let ran = run_rows(&b, &[b"v", b"s"]);
        let screen = ran.screen();
        assert!(screen.contains(words::PAGER_TITLE), "{screen}");
        assert!(screen.contains(words::NOTHING_TO_SAVE), "{screen}");
        // i runs the install flow: here the draft is what is installed
        let ran = run_rows(&b, &[b"v", b"i"]);
        let screen = ran.screen();
        assert!(screen.contains(words::PAGER_TITLE), "{screen}");
        assert!(screen.contains(words::NOTHING_TO_INSTALL), "{screen}");
    }

    #[test]
    fn the_pager_reaches_the_draft_s_last_line() {
        let b = bench("pager-end");
        let long = format!(
            "{INSTALLED}{}# the last line\n",
            "# a line of comment\n".repeat(40)
        );
        std::fs::write(b.root.join("etc/vtium.conf"), long).expect("vtium.conf");
        let ran = run(&b, &[b"v", b"G"]);
        let screen = ran.screen();
        assert!(screen.contains("# the last line"), "{screen}");
    }

    #[test]
    fn a_device_that_leaves_becomes_its_placeholder_and_one_that_comes_is_placed() {
        let b = bench("hotplug");
        // the keyboard rule 2 names, and a spare one, then the spare alone
        let seen = vec![
            present(vec![keyboard(), other_keyboard()], true),
            present(vec![other_keyboard()], true),
        ];
        // udev reports the keyboard went: rule 2 names it, so its part
        // stays, grey, and says so (ADR-1403 D7)
        let ran = run_on(&b, seen, true, &[b"@"]);
        assert!(
            ran.status().contains("disconnected: kbd"),
            "{}",
            ran.status()
        );
        assert!(ran.screen().contains("Not connected"), "{}", ran.screen());
        assert_eq!(b.draft(), INSTALLED);
        // a keyboard that comes is placed by the rules, roaming here
        let b = bench("hotplug-came");
        let seen = vec![
            present(vec![keyboard()], true),
            present(vec![], true),
            present(vec![other_keyboard()], true),
        ];
        let ran = run_on(&b, seen, true, &[b"@", b"@"]);
        assert!(
            ran.status().contains("connected: spare"),
            "{}",
            ran.status()
        );
        assert!(ran.screen().contains("spare"), "{}", ran.screen());
        assert_eq!(b.draft(), INSTALLED);
    }

    #[test]
    fn without_a_udev_daemon_the_status_line_says_hotplug_is_not_followed() {
        let b = bench("no-udev");
        let ran = run_on(&b, vec![present(vec![keyboard()], false)], true, &[]);
        assert!(ran.status().contains(words::NO_HOTPLUG), "{}", ran.status());
        let ran = run_on(&b, vec![present(vec![keyboard()], true)], false, &[]);
        assert!(ran.status().contains(words::NO_HOTPLUG), "{}", ran.status());
    }

    /// An install whose only change is a new frit, with the draft the
    /// installed vtium.conf, installs the frit through the privileged step
    /// and leaves vtium.conf and the installed history as they were.
    #[test]
    fn an_install_of_a_new_frit_alone_installs_the_frit() {
        let b = bench("frit-alone");
        let mut console = Console::vtium(100, 30, true, "unifont")
            .takes("proof", (100, 30))
            .takes("unifont", (100, 30))
            .keys(&[
                b"n",
                b"quiet",
                b"\r",
                b"builtin:unscii-8",
                b"\r",
                b"\r",
                b"\r",
                b"\r",
                b"\r",
                b"\r",
                b"i",
                b"y",
            ]);
        console.installs = true;
        let mut desk = b.desk(vec![present(vec![keyboard()], true)]);
        let mut i = inputs();
        i.view = model::View::Fonts;
        i.conf = Some(INSTALLED.to_owned());
        i.frits = b.root.join("etc/fonts");
        i.installed = BTreeMap::new();
        drive_with(&mut console, i, Some(&mut desk)).expect("the loop");
        let ran = Ran { console };
        assert_eq!(ran.console.saves.len(), 1, "{}", ran.screen());
        assert_eq!(
            std::fs::read_to_string(b.root.join("etc/fonts/quiet.vtfrit"))
                .expect("the installed frit"),
            "# quiet: made by vtium-config new-frit\nname=quiet\nsource=builtin:unscii-8\n"
        );
        assert!(
            ran.screen().contains("Installed draft bench"),
            "{}",
            ran.screen()
        );
        assert_eq!(b.installed(), INSTALLED);
        assert!(b.store().history().expect("the history").is_empty());
    }

    /// The stand-in desk refuses what the store refuses: an install that
    /// no Prepare readied. After a Prepare of a draft that is the installed
    /// file, both run the install.
    #[test]
    fn the_stand_in_desk_installs_only_what_a_prepare_readied_as_the_store_does() {
        use vtium_config::tui::desk::Op;
        let b = bench("stand-in");
        let mut store = b.desk(vec![present(Vec::new(), true)]);
        let mut stand_in = super::desk();
        let desks: [&mut dyn vtium_config::tui::desk::Desk; 2] = [&mut store, &mut stand_in];
        for (n, d) in desks.into_iter().enumerate() {
            let mut ran = 0;
            let mut run = |_: &[std::ffi::OsString]| -> io::Result<(bool, String)> {
                ran += 1;
                Ok((true, String::new()))
            };
            let plan = vtium_config::draft::Plan::default();
            let why = d.install(&plan, &mut run).expect_err("no Prepare");
            assert_eq!(why, words::NOT_PREPARED, "desk {n}");
            assert!(matches!(d.run(&Op::Prepare), Ok(Done::Plan(_))), "desk {n}");
            assert!(d.install(&plan, &mut run).is_ok(), "desk {n}");
            let why = d
                .install(&plan, &mut run)
                .expect_err("one install a Prepare");
            assert_eq!(why, words::NOT_PREPARED, "desk {n}");
            assert_eq!(ran, 1, "desk {n}");
        }
    }

    /// The Flow view's states on a draft at 80x25 and 120x32 (ADR-1403
    /// D7), each reached by its keys: the shipped default, a configuration
    /// of three paths (desk, panel and the roaming inputs), a new path half
    /// built, the Input maker, the USB port picker, a terminal's card and
    /// a labelled part.
    #[test]
    fn the_flow_view_s_states_at_80x25_and_120x32() {
        const HOME: &[u8] = b"\x1b[H";
        // Down from the first box reaches PARTS' inputs at the sixth step
        let to_inputs: Vec<&[u8]> = [&[HOME][..], &[DOWN; 6][..]].concat();
        let maker: Vec<&[u8]> = [&to_inputs[..], &[LEFT, b"\r"][..]].concat();
        let enter: &[u8] = b"\r";
        let picker: Vec<&[u8]> = [&maker[..], &[DOWN; 4][..], &[enter][..]].concat();
        let typed: [&[u8]; 4] = [enter, enter, b"Left desk", enter];
        let label: Vec<&[u8]> = [&to_inputs[..], &typed[..]].concat();
        // the draft with desk on VT 3 and panel on VT 4, each showing
        // every display (ADR-1403 D7)
        let vts = "font-size=12\nversion = 1\n[look big]\nfont-name = proof\n[head desk]\nlook = big\nvt = 3\n[head panel]\nvt = 4\n[rules]\ndisplay * -> desk\ndisplay * -> panel\ninput path:pci-0:3.3 -> panel\ninput class:keyboard -> roaming\n";
        // no sections: the draft runs vtium's built-in default
        let shipped = "font-size=12\n";
        let space: &[u8] = b" ";
        let clash: Vec<&[u8]> = [&[HOME][..], &[DOWN; 4][..], &[space][..]].concat();
        // the terminal desk's card with the cursor on its Login line
        let login: Vec<&[u8]> = [&[HOME, RIGHT, enter][..], &[DOWN; 4][..]].concat();
        // each golden's name, the vtium.conf it starts from when not the
        // fixture's, and the keys that reach it
        type Script<'a> = (&'a str, Option<&'a str>, &'a [&'a [u8]]);
        let scripts: [Script<'_>; 10] = [
            ("flow-default", Some(shipped), &[]),
            ("flow-paths", None, &[]),
            ("flow-new-path", None, &[HOME, RIGHT, RIGHT, b" "]),
            ("flow-input-maker", None, &maker),
            ("flow-picker", None, &picker),
            ("flow-terminal-card", None, &[HOME, RIGHT, b"\r"]),
            ("flow-login", None, &login),
            ("flow-label", None, &label),
            ("flow-shared-vts", Some(vts), &[]),
            ("flow-vt-clash", Some(vts), &clash),
        ];
        for (cols, rows) in [(80, 25), (120, 32)] {
            for (name, conf, keys) in scripts {
                let b = bench(&format!("{name}-{cols}"));
                if let Some(conf) = conf {
                    std::fs::write(b.root.join("etc/vtium.conf"), conf).expect("vtium.conf");
                }
                let ran = run_at(
                    &b,
                    (cols, rows),
                    vec![present(vec![keyboard()], true)],
                    true,
                    keys,
                );
                let lines: Vec<&str> = ran.screen().lines().collect();
                assert_eq!(lines.len(), rows, "{name} at {cols}x{rows}");
                assert!(lines.iter().all(|l| l.chars().count() <= cols), "{name}");
                assert!(lines[1].contains("draft bench"), "{name}: {}", ran.screen());
                let c = &ran.console;
                let bytes = c.written.get(..c.seen_at).unwrap_or_default();
                golden(&format!("{name}-{cols}x{rows}"), ran.screen(), bytes);
            }
        }
    }
}
