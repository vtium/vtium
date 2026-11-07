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
#[path = "common/script.rs"]
mod script;

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use vtium_config::kitty;
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
    /// picture is two pixels; a summary is unifont's.
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
                self.shots.push(shot);
                Drawn::Pictured(Ok(Picture {
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
            Op::Prepare if self.draft == self.installed => Ok(Done::Plan(Default::default())),
            Op::Prepare => Ok(Done::Plan(vtium_config::draft::Plan {
                files: vec![vtium_config::draft::FileEdit {
                    path: PathBuf::from("/etc/vtium/vtium.conf"),
                    old: Some(self.installed.clone()),
                    new: self.draft.clone(),
                }],
                removals: Vec::new(),
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

/// Moving the highlight previews each font as a picture and sends the
/// live console nothing (ADR-1201 D4): no switch goes, and leaving has no
/// font to give back.
#[test]
fn each_highlighted_font_leaves_the_live_console_alone() {
    let mut c = console().keys(&[UP, &[DOWN, DOWN].concat(), DOWN, b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    assert_eq!(c.switches(), Vec::<String>::new(), "{:?}", c.log);
    assert!(!c.log.iter().any(|l| l.starts_with("set ")), "{:?}", c.log);
    // each font highlighted got its specimen instead; of the two Downs of
    // one read, the first asks for unifont's, and the second waits for it
    // and asks for unscii-8's
    let fonts: Vec<&str> = c.specimens.iter().map(|(f, _, _)| f.as_str()).collect();
    assert_eq!(fonts, ["unifont", "tiny5", "unifont", "unscii-8"]);
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
        ("grab-scroll-up", "<Shift>Up"),
        ("grab-scroll-down", "<Shift>Down"),
        ("grab-page-up", "<Shift>Prior"),
        ("grab-page-down", "<Shift>Next"),
        ("grab-prompt-up", "<Ctrl><Shift>Up"),
        ("grab-prompt-down", "<Ctrl><Shift>Down"),
        ("grab-zoom-in", "<Ctrl>plus,<Ctrl>equal"),
        ("grab-zoom-out", "<Ctrl>minus"),
        ("grab-rotate-cw", "<Logo>plus,<Logo>equal"),
        ("grab-rotate-ccw", "<Logo>minus"),
        ("grab-reset", "<Ctrl><Shift>Delete"),
        ("grab-console-next", "<Ctrl><Logo>Right"),
        ("grab-console-previous", "<Ctrl><Logo>Left"),
        ("grab-console-open", "<Ctrl><Logo>Return"),
        ("grab-console-close", "<Ctrl><Logo>BackSpace"),
        ("grab-focus-next", "<Ctrl><Logo>Up"),
        ("grab-focus-previous", "<Ctrl><Logo>Down"),
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
        .position(|k| k.name == "grab-reset")
        .expect("listed");
    let mut keys: Vec<&[u8]> = vec![b"\t"];
    keys.extend(std::iter::repeat_n(DOWN, at));
    let mut c = console().keys(&keys);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("grab-reset"), "{screen}");
    assert!(screen.contains("<Ctrl><Shift>Delete (default)"), "{screen}");
    assert!(screen.contains("Reset the console on screen"), "{screen}");
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
        b"\r", b"q",
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
    let grid = (cols, rows);
    let mut c = Console::vtium(cols, rows, true, "unifont")
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

/// Down from the first display to the keyboard: the Flow view's input
/// devices are its sixth column.
const TO_KEYBOARD: [&[u8]; 6] = [b"\x1b[C", b"\x1b[C", b"\x1b[C", b"\x1b[C", b"\x1b[C", DOWN];

#[test]
fn each_view_at_80x25_and_100x30() {
    use model::View::{Flow, Fonts};
    // f follows the keyboard's line to rule 5, and Enter opens it
    let card: Vec<&[u8]> = [&TO_KEYBOARD[..], &[b"f", b"\r"]].concat();
    let route: Vec<&[u8]> = [&TO_KEYBOARD[..], &[b"?"]].concat();
    let scripts: [(&str, model::View, &[&[u8]]); 8] = [
        ("flow", Flow, &[]),
        ("card", Flow, &card),
        ("route", Flow, &route),
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
            // the last row holds the key that quits, or the route card's
            // that goes back
            let last = lines[rows - 1];
            assert!(
                last.contains(" quit") || last.contains("q back"),
                "{name}: {screen}"
            );
            golden(&format!("{name}-{cols}x{rows}"), &screen, &bytes);
        }
    }
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
        Fonts => words::KEYS_FONTS,
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
    assert_eq!(line, words::KEYS_FONTS, "{screen}");
}

#[test]
fn tab_does_nothing_while_a_value_is_typed_in_settings() {
    // Settings, Down to font-line-spacing, Enter starts typing its value;
    // Tab and Shift-Tab between the two digits change nothing
    let mut c = console().keys(&[
        TAB, DOWN, DOWN, DOWN, DOWN, b"\r", b"1", TAB, BACKTAB, b"0", b"\r", b"q",
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
    assert_eq!(line, words::KEYS_FONTS);
}

#[test]
fn tab_on_the_route_card_and_during_the_live_trial_stays_on_the_view() {
    for key in [TAB, BACKTAB] {
        let line = key_line(model::View::Flow, &[b"?", key]);
        assert_eq!(line, words::KEYS_ROUTE, "{key:?}");
    }
    // the live trial: Tab, as any key but k, puts the font back
    let mut c = console().keys(&[DOWN, b"t", TAB]);
    run(&mut c);
    let screen = c.seen.clone().expect("the screen");
    let line = screen.lines().last().unwrap_or_default().trim();
    assert_eq!(line, words::KEYS_FONTS, "{screen}");
    assert!(screen.contains(&words::trial_back("unifont")), "{screen}");
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
    // frit that reads lacks
    // and, as the terminal draws no pictures, for the line that describes
    // the highlighted font at 12 points
    let frits = ["minspec-16", "proof", "tiny5"].map(String::from).to_vec();
    assert_eq!(
        started,
        [
            Effect::Sources(frits),
            Effect::Summary {
                font: "unifont".into(),
                height: 16
            }
        ]
    );
    // unifont's line is still being drawn, so tiny5's waits for it
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
    for r in [
        Reply::Version("vtium(test)".into()),
        Reply::Font("unifont".into()),
        Reply::Attributes,
    ] {
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
        Reply::Font("unifont".into()),
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
    let mut c = console().keys(&[b"s", UP]);
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
    let mut c = console().keys(&[b"s"]);
    c.graphics = true;
    let mut small = inputs();
    small.display = Some((320, 240));
    drive(&mut c, small).expect("the loop");
    let shot = c.shots.first().expect("a picture");
    assert_eq!(shot.grid, (40, 15));
    let first = vtium_config::tui::view::text(&shot.screen[0]);
    assert!(first.starts_with("This terminal is too small"), "{first}");
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
    let mut c = console().keys(&[BACKTAB, DOWN, DOWN, b"\t", b"q"]);
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
    let mut c = console().keys(&[b"s", UP, b"s", b"q"]);
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
    let mut c = console().keys(&[]);
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
/// describes it, whether or not the console can be switched live.
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
    let fonts: Vec<&str> = live.specimens.iter().map(|(f, _, _)| f.as_str()).collect();
    assert_eq!(fonts, ["unifont", "tiny5"]);
    assert_eq!(live.switches(), Vec::<String>::new());
}

/// Where the terminal draws no pictures, a size shows what it gives: its
/// cell, its grid and how much of the specimen the font draws itself.
#[test]
fn without_pictures_a_size_shows_its_cell_grid_and_coverage() {
    let mut c = console().keys(&[BACKTAB, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN, DOWN]);
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

#[test]
fn leaving_with_a_picture_deletes_it_before_the_font_comes_back() {
    let mut c = console().keys(&[UP, b"t", b"k", BACKTAB, DOWN, b"q"]);
    c.graphics = true;
    assert_eq!(run(&mut c), 0);
    let text = String::from_utf8_lossy(&c.written).into_owned();
    let delete = text
        .rfind("\x1b_Ga=d,d=I,i=1201,q=2\x1b\\")
        .expect("deleted");
    let back = text.rfind("\x1b]50;unifont\x1b\\").expect("the font back");
    assert!(delete < back, "deleted, then the font given back");
}

#[test]
fn automatic_has_no_picture() {
    let mut c = console().keys(&[BACKTAB, DOWN, UP]);
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
    let mut c = console().keys(&[BACKTAB, DOWN, DOWN]);
    run(&mut c);
    assert!(c.pictures.is_empty());
    assert_eq!(count(&c.written, b"\x1b_Ga=T"), 0);
    let screen = c.seen.clone().expect("the screen");
    assert!(screen.contains("does not support images"), "{screen}");
}

#[test]
fn the_picture_never_reaches_the_footer_and_never_passes_the_cap() {
    // 80x25 at 8x16: rows 7 to 20 are the area, above the longest message
    let mut c = console().keys(&[BACKTAB, DOWN]);
    c.graphics = true;
    run(&mut c);
    let (_, _, (w, h)) = c.pictures.last().cloned().expect("6 pt's picture");
    assert!(h <= (25 - 8) * 16 && w <= (80 - 25) * 8, "{w}x{h}");
    // a terminal of very large cells is capped
    let mut c = console().keys(&[BACKTAB, DOWN]);
    c.graphics = true;
    c.cell_px = Some((200, 400));
    run(&mut c);
    let (_, _, max) = c.pictures.last().cloned().expect("6 pt's picture");
    assert_eq!(max, vtium_config::specimen::PICTURE_MAX);
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
        (&[], words::KEYS_FONTS, &[]),
        (&[BACKTAB], words::KEYS_LOOKS, &[words::NO_PICTURES]),
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
        for (_, short) in words::ACTIONS {
            assert!(rows[0].contains(short), "{short}: {:?}", rows[0]);
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
    assert!(joined(&top).contains(words::NO_PICTURES), "{top}");
    assert!(
        !top.contains("32 pt"),
        "the list is longer than its room: {top}"
    );
    // the last size scrolls into sight, and the sentence stays whole
    let bottom = reach(80, 25, &[BACKTAB, b"\x1b[4~"]);
    assert!(bottom.contains("> 32 pt"), "{bottom}");
    let first = words::NO_PICTURES.split(", ").next().unwrap_or("");
    assert_eq!(
        joined(&bottom).contains(first),
        joined(&bottom).contains(words::NO_PICTURES),
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
    use vtium_config::tui::flow::{Item, New, Special};
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

    // The Flow view starts from a flow nothing has read, and says so, until
    // the desk opens the draft; a desk that cannot open it leaves the view
    // so, which is when a person sees the words
    #[test]
    fn a_flow_nothing_has_read_says_so() {
        let mut i = inputs();
        i.view = View::Flow;
        i.flow = vtium_config::tui::flow::Flow::default();
        let a = App::new(i, 80, 25).0;
        let shown = text(&a).replace('\n', " ");
        let first = words::FLOW_NOT_READ
            .split(' ')
            .take(4)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(shown.contains(&first), "{shown}");
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

    #[test]
    fn the_arrows_move_by_column_to_the_nearest_row_and_within_a_column() {
        let mut a = app(80, 25);
        assert_eq!(a.flow_at, Item::Display(0));
        let mut path = Vec::new();
        for _ in 0..6 {
            press(&mut a, Key::Right);
            path.push(a.flow_at);
        }
        assert_eq!(
            path,
            [
                Item::Rule(0),
                Item::Head(0),
                Item::HeadLook(0),
                Item::Rule(3),
                Item::Input(0),
                // the arrows stop at the last column
                Item::Input(0),
            ]
        );
        assert_eq!(a.view, View::Flow);
        press(&mut a, Key::Down);
        assert_eq!(a.flow_at, Item::Input(1));
        // the keyboard's row is level with rule 6, below rules 4 and 5 that
        // each take two rows
        press(&mut a, Key::Left);
        assert_eq!(a.flow_at, Item::Rule(5));
        press(&mut a, Key::Up);
        assert_eq!(a.flow_at, Item::Rule(4));
        press(&mut a, Key::End);
        assert_eq!(a.flow_at, Item::New(New::InputRule));
        // the heads' column ends in the targets that are not heads
        let mut a = app(80, 25);
        press(&mut a, Key::Right);
        press(&mut a, Key::Right);
        press(&mut a, Key::End);
        assert_eq!(a.flow_at, Item::Policy);
        press(&mut a, Key::Up);
        assert_eq!(a.flow_at, Item::Special(Special::Off));
        // up from the first display is the base card
        let mut a = app(80, 25);
        press(&mut a, Key::Up);
        assert_eq!(a.flow_at, Item::Base);
        press(&mut a, Key::Down);
        assert_eq!(a.flow_at, Item::Display(0));
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
    fn tab_and_shift_tab_step_the_views_from_every_column_of_the_flow() {
        for column in 0..6 {
            for (key, next) in [(TAB, View::Looks), (BACKTAB, View::Help)] {
                let mut a = app(80, 25);
                for _ in 0..column {
                    type_in(&mut a, RIGHT);
                }
                let at = a.flow_at;
                assert_eq!(a.inputs.flow.column_of(at), column, "{at:?}");
                let out = type_in(&mut a, key);
                assert_eq!(a.view, next, "{key:?} from column {column}");
                assert!(writes(&out, MOUSE_OFF), "{out:?}");
                // and back: the flow keeps the item that was selected
                let back = if key == TAB { BACKTAB } else { TAB };
                let out = type_in(&mut a, back);
                assert_eq!((a.view, a.flow_at), (View::Flow, at), "column {column}");
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

    #[test]
    fn tab_closes_a_card_with_nothing_to_change_as_it_steps_the_view() {
        // without a draft a card only shows its item, and the flow's keys
        // pass through it
        let mut a = app(80, 25);
        type_in(&mut a, b"\r");
        assert!(a.opened && a.fields().is_empty());
        type_in(&mut a, TAB);
        assert_eq!((a.view, a.opened), (View::Looks, false));
        type_in(&mut a, BACKTAB);
        assert_eq!((a.view, a.opened), (View::Flow, false));
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
        // a card with fields, its field being typed, and the pager: Tab
        // and Shift-Tab do nothing while each is open
        let mut a = drafted(80, 25);
        a.flow_at = Item::Rule(4);
        type_in(&mut a, b"\r");
        assert!(a.opened && !a.fields().is_empty());
        let card = a.card.clone();
        for key in [TAB, BACKTAB] {
            type_in(&mut a, key);
            assert_eq!((a.view, a.opened, &a.card), (View::Flow, true, &card));
        }
        // Down to the selector, past its values to type…
        type_in(&mut a, b"\x1b[B");
        for _ in 0..20 {
            type_in(&mut a, RIGHT);
        }
        type_in(&mut a, b"\r");
        assert!(a.card.typed.is_some(), "{:?}", a.card);
        type_in(&mut a, b"ab");
        let typed = a.card.typed.clone();
        for key in [TAB, BACKTAB] {
            type_in(&mut a, key);
            assert_eq!((a.view, &a.card.typed), (View::Flow, &typed));
        }
        let mut a = drafted(80, 25);
        type_in(&mut a, b"v");
        assert!(a.pager.is_some());
        for key in [TAB, BACKTAB] {
            type_in(&mut a, key);
            assert_eq!((a.view, a.pager.is_some()), (View::Flow, true));
        }
        // the route card
        let mut a = app(80, 25);
        type_in(&mut a, b"?");
        let route = a.route;
        assert!(route.is_some());
        for key in [TAB, BACKTAB] {
            type_in(&mut a, key);
            assert_eq!((a.view, a.route), (View::Flow, route));
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
        a.flow_at = Item::Rule(4);
        type_in(&mut a, b"\r");
        click(&mut a, looks);
        assert_eq!((a.view, a.opened), (View::Flow, true));
        // a card with nothing to change closes as the click switches
        let mut a = app(80, 25);
        type_in(&mut a, b"\r");
        click(&mut a, looks);
        assert_eq!((a.view, a.opened), (View::Looks, false));
    }

    #[test]
    fn enter_opens_the_selected_item_s_card_and_esc_closes_it() {
        let mut a = app(80, 25);
        a.flow_at = Item::Rule(4);
        assert!(!text(&a).contains("rule 5, line 23"));
        press(&mut a, Key::Enter);
        assert!(a.opened);
        let screen = text(&a);
        assert!(
            screen.contains("rule 5, line 23   input class:keyboard -> roaming"),
            "{screen}"
        );
        assert!(
            screen.contains("sends to   desk  panel  roaming  ignore"),
            "{screen}"
        );
        // the card follows the selection
        press(&mut a, Key::Down);
        assert!(text(&a).contains("rule 6, line 24"), "{}", text(&a));
        press(&mut a, Key::Escape);
        assert!(!a.opened);
        assert!(!text(&a).contains("rule 6, line 24"));
    }

    #[test]
    fn space_tags_any_item_the_status_line_lists_the_tags_and_esc_clears_them() {
        let mut a = app(100, 30);
        a.flow_at = Item::Input(1);
        press(&mut a, Key::Char(' '));
        a.flow_at = Item::Head(0);
        press(&mut a, Key::Char(' '));
        a.flow_at = Item::HeadLook(0);
        press(&mut a, Key::Char(' '));
        assert_eq!(a.tags, [Item::Input(1), Item::Head(0), Item::HeadLook(0)]);
        let screen = text(&a);
        let status = screen.lines().nth(1).unwrap_or_default();
        assert!(status.contains("tagged: keyboard, desk, big"), "{status}");
        assert!(screen.contains("◆ keyboard"), "{screen}");
        // Space again untags; the base card and the new items take no tag
        press(&mut a, Key::Char(' '));
        assert_eq!(a.tags, [Item::Input(1), Item::Head(0)]);
        a.flow_at = Item::Base;
        press(&mut a, Key::Char(' '));
        a.flow_at = Item::New(New::Head);
        press(&mut a, Key::Char(' '));
        assert_eq!(a.tags.len(), 2);
        press(&mut a, Key::Escape);
        assert!(a.tags.is_empty());
        assert!(!text(&a).contains("tagged:"));
    }

    #[test]
    fn f_follows_a_line_toward_the_heads_and_steps_through_several() {
        let mut a = app(80, 25);
        a.flow_at = Item::Input(1);
        press(&mut a, Key::Char('f'));
        assert_eq!(a.flow_at, Item::Rule(4));
        press(&mut a, Key::Char('f'));
        assert_eq!(a.flow_at, Item::Special(Special::Roaming));
        // desk has two display rules' lines: f steps through them
        a.flow_at = Item::Head(0);
        let mut seen = Vec::new();
        for _ in 0..3 {
            press(&mut a, Key::Char('f'));
            seen.push(a.flow_at);
        }
        assert_eq!(seen, [Item::Rule(0), Item::Rule(2), Item::Rule(0)]);
        a.flow_at = Item::Base;
        press(&mut a, Key::Char('f'));
        assert_eq!(a.flow_at, Item::Base);
        assert_eq!(a.message.as_deref(), Some(words::NOTHING_TO_FOLLOW));
    }

    #[test]
    fn the_route_card_shows_every_route_and_enter_opens_an_item_on_it() {
        let mut a = app(80, 25);
        a.flow_at = Item::Input(1);
        press(&mut a, Key::Char('?'));
        // the cursor starts on the selected item's line in the first route
        assert_eq!(a.route, Some(6));
        let screen = text(&a);
        assert!(
            screen.contains("Route card: keyboard, usb-0:3.3"),
            "{screen}"
        );
        assert!(
            screen.contains("route 2 of 2, when panel is the active head"),
            "{screen}"
        );
        assert!(
            !screen.contains("DISPLAYS"),
            "the card takes the columns' place"
        );
        // q goes back to the flow and does not leave
        let out = press(&mut a, Key::Char('q'));
        assert!(out.is_empty(), "{out:?}");
        assert_eq!((a.route, a.leaving), (None, None));
        // Up to the input rule, and Enter opens it
        press(&mut a, Key::Char('?'));
        press(&mut a, Key::Up);
        assert_eq!(a.route, Some(5));
        press(&mut a, Key::Enter);
        assert_eq!((a.route, a.flow_at, a.opened), (None, Item::Rule(4), true));
        // a look no head uses is part of no route, and says so
        a.flow_at = Item::Look(2);
        press(&mut a, Key::Char('?'));
        assert!(text(&a).contains(words::LOOK_NOT_USED), "{}", text(&a));
        press(&mut a, Key::Escape);
        assert_eq!(a.route, None);
    }

    #[test]
    fn the_route_card_marks_the_selected_item_in_words_too() {
        let mut i = inputs();
        i.view = View::Flow;
        i.color = false;
        let (mut a, _) = App::new(i, 80, 25);
        a.flow_at = Item::Input(1);
        press(&mut a, Key::Char('?'));
        // the cursor leaves the selected item's line, and the mark stays
        press(&mut a, Key::Up);
        let marked: Vec<String> = text(&a)
            .lines()
            .filter(|l| l.starts_with(view::SELECTED))
            .map(str::to_owned)
            .collect();
        assert_eq!(marked.len(), 2, "one in each route: {}", text(&a));
        assert!(
            marked.iter().all(|l| l.contains("keyboard, usb-0:3.3")),
            "{marked:?}"
        );
        let bytes = view::draw(&a);
        assert!(
            sgr(&bytes)
                .iter()
                .all(|p| p.iter().all(|n| [0, 1, 7].contains(n)))
        );
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
    fn a_click_selects_a_second_click_opens_and_a_view_s_name_switches_to_it() {
        let mut a = app(80, 25);
        // the head's block, right of the arrow that reaches it
        let (col, row) = cell(&a, "► panel");
        let panel = (col + 2, row);
        click(&mut a, panel);
        assert_eq!((a.flow_at, a.opened), (Item::Head(1), false));
        click(&mut a, panel);
        assert!(a.opened);
        assert!(text(&a).contains("head panel · 2 consoles"), "{}", text(&a));
        // the base card's row selects the base card
        click(&mut a, (5, 2));
        assert_eq!(a.flow_at, Item::Base);
        // the wheel moves as Down does
        press(
            &mut a,
            Key::Mouse(Mouse {
                button: Button::WheelDown,
                col: 0,
                row: 0,
            }),
        );
        assert_eq!(a.flow_at, Item::Display(0));
        let looks = cell(&a, "Looks");
        let out = click(&mut a, looks);
        assert_eq!(a.view, View::Looks);
        assert!(writes(&out, MOUSE_OFF), "{out:?}");
        // the other views asked for no mouse, and a click there does nothing
        click(&mut a, (2, 0));
        assert_eq!(a.view, View::Looks);
    }

    #[test]
    fn a_click_reaches_the_tui_through_its_decoder() {
        // SGR mouse, mode 1006: a press on the last cell of desk's block,
        // row 5 column 39 counted from 1, the cell before the look's
        let mut c = console().keys(&[b"\x1b[<0;39;5M", b"\x1b[<0;39;5m", b"\r"]);
        let mut i = inputs();
        i.view = View::Flow;
        drive(&mut c, i).expect("the loop");
        let written = String::from_utf8_lossy(&c.written).into_owned();
        assert!(written.contains("\x1b[?1000h\x1b[?1006h"), "turned on");
        let screen = c.seen.clone().expect("the screen");
        assert!(
            screen.contains("head desk · 1 console · the active head"),
            "{screen}"
        );
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
        a.flow_at = Item::Input(1);
        press(&mut a, Key::Enter);
        screens.push(view::draw(&a));
        press(&mut a, Key::Char('?'));
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
        // a display is black on bright cyan, an input device black on
        // bright green, a head black on bright yellow, a look black on
        // bright magenta, the targets that are not heads black on grey
        let flow = sgr(&screens[0]);
        for block in [[30, 106], [30, 102], [30, 105], [30, 100]] {
            assert!(
                flow.iter().any(|p| p.ends_with(&block)),
                "{block:?}: {flow:?}"
            );
        }
        assert!(flow.iter().any(|p| p == &[1, 30, 103]), "a head's name");
        // the rules in their columns' colors: cyan and green
        assert!(flow.iter().any(|p| p == &[36]) && flow.iter().any(|p| p == &[32]));
    }

    #[test]
    fn without_color_the_view_draws_no_color_and_brackets_each_block() {
        let mut i = inputs();
        i.view = View::Flow;
        i.color = false;
        let (mut a, _) = App::new(i, 80, 25);
        let bytes = view::draw(&a);
        for params in sgr(&bytes) {
            assert!(params.iter().all(|p| [0, 1, 7].contains(p)), "{params:?}");
        }
        let screen = text(&a);
        assert!(screen.contains("[DP-8     ]"), "{screen}");
        assert!(screen.contains("[not       ]"), "{screen}");
        assert!(screen.contains("[connected ]"), "{screen}");
        assert!(screen.contains("[shared settings"), "{screen}");
        // the selection is still reverse video
        assert!(sgr(&bytes).iter().any(|p| p.contains(&7)));
        // and every state still has its words
        a.flow_at = Item::Rule(2);
        // the rule's short words, "none" above "connected" in its column
        let t = text(&a);
        let rows: Vec<Vec<char>> = t.lines().map(|r| r.chars().collect()).collect();
        let below = |i: usize, at: usize| -> String {
            rows.get(i + 1)
                .map(|r| r.iter().skip(at).take(9).collect())
                .unwrap_or_default()
        };
        let said = rows.iter().enumerate().any(|(i, r)| {
            (0..r.len()).any(|at| {
                r[at..].starts_with(&['n', 'o', 'n', 'e', ' ']) && below(i, at) == "connected"
            })
        });
        assert!(said, "{t}");
    }

    #[test]
    fn a_rule_to_a_head_that_does_not_exist_is_red_and_says_so() {
        let mut i = inputs();
        i.view = View::Flow;
        i.flow.rules[0].target = "tv".into();
        let (mut a, _) = App::new(i, 100, 30);
        let screen = text(&a);
        assert!(screen.contains("! 1 edid:"), "{screen}");
        assert!(screen.contains("! head not"), "{screen}");
        let bytes = String::from_utf8_lossy(&view::draw(&a)).into_owned();
        assert!(bytes.contains("\x1b[91m! 1 edid:"), "bright red");
        // no line runs from it, and its route card says what is broken
        a.flow_at = Item::Rule(0);
        press(&mut a, Key::Char('?'));
        assert!(
            text(&a).contains(
                "Rule 1 sends edid:AAA:0x0001:LARGE00001 to tv, but there is no head named tv."
            ),
            "{}",
            text(&a)
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
    fn a_flow_not_read_says_so_in_place_of_an_empty_flow() {
        let mut i = inputs();
        i.view = View::Flow;
        i.flow = vtium_config::tui::flow::Flow::default();
        let (a, _) = App::new(i, 80, 25);
        let screen = text(&a);
        let joined = screen.lines().map(str::trim).collect::<Vec<_>>().join(" ");
        assert!(joined.contains(words::FLOW_NOT_READ), "{screen}");
        assert!(
            joined.contains(
                "The Flow view is empty: no draft of vtium.conf is open. Quit and start vtium config to open one."
            ),
            "{screen}"
        );
        assert!(screen.contains("no draft"), "{screen}");
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
    const END: &[u8] = b"\x1b[F";
    const ESC: &[u8] = b"\x1b";

    /// A fixture under the tests' own home.
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

    /// The display `DP-1`.
    fn display() -> Identity {
        let mut d = Identity::display();
        d.connector = Some("DP-1".into());
        d.edid = Some(vtium_settings::Edid {
            maker: "AAA".into(),
            product: 1,
            serial: "S1".into(),
        });
        d
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

    #[test]
    fn the_view_opens_on_the_draft_and_its_state() {
        let b = bench("opens");
        let ran = run(&b, &[]);
        assert_eq!(b.draft(), INSTALLED);
        let status = ran.status();
        assert!(status.contains("· no changes · no problems"), "{status}");
        assert!(!status.contains(words::NO_HOTPLUG), "{status}");
        let screen = ran.screen();
        for want in ["desk", "panel", "big", "kbd"] {
            assert!(screen.contains(want), "{want}: {screen}");
        }
    }

    #[test]
    fn a_rule_s_card_picks_its_target_and_types_its_selector() {
        let b = bench("rule-card");
        // with no display present the view opens on rule 1: open it,
        // pick the next target, keep it
        let ran = run(&b, &[b"\r", RIGHT, b"\r"]);
        assert_eq!(
            b.draft(),
            INSTALLED.replace("display * -> desk", "display * -> panel")
        );
        assert!(
            ran.screen()
                .contains("vtium config rule set 1 --target panel"),
            "{}",
            ran.screen()
        );
        let status = ran.status();
        assert!(status.contains("1 change"), "{status}");
        // its selector, typed: Down to the selector, past its values to
        // type…, a value that replaces the current one
        let b = bench("rule-selector");
        run(
            &b,
            &[b"\r", DOWN, RIGHT, b"\r", b"\x7f", b"card:pci-0", b"\r"],
        );
        assert_eq!(
            b.draft(),
            INSTALLED.replace("display * -> desk", "display card:pci-0 -> desk")
        );
    }

    #[test]
    fn a_rule_moves_up_within_its_column() {
        let b = bench("rule-move");
        // the input rules column, its second rule, the actions, move up
        let keys: Vec<&[u8]> = vec![RIGHT, RIGHT, RIGHT, DOWN, b"\r", END, b"\r"];
        run(&b, &keys);
        assert_eq!(
            b.draft(),
            INSTALLED.replace(
                "input path:pci-0:3.3 -> panel\ninput class:keyboard -> roaming",
                "input class:keyboard -> roaming\ninput path:pci-0:3.3 -> panel"
            )
        );
    }

    #[test]
    fn a_head_s_card_picks_its_look_from_the_defined_looks() {
        let b = bench("head-look");
        // the heads column, desk, open: its look field offers big and
        // default; Right picks default
        run(&b, &[RIGHT, b"\r", RIGHT, b"\r"]);
        assert_eq!(
            b.draft(),
            INSTALLED.replace("[head desk]\nlook = big", "[head desk]\nlook = default")
        );
    }

    #[test]
    fn a_look_s_card_sets_its_size_and_tries_its_frit_on_t() {
        let b = bench("look-size");
        // desk's look, open, Down past its name and frit to its size,
        // Right twice from automatic to 7 points
        let ran = run(&b, &[RIGHT, RIGHT, b"\r", DOWN, DOWN, RIGHT, RIGHT, b"\r"]);
        assert_eq!(
            b.draft(),
            INSTALLED.replace("[look big]\n", "[look big]\nfont-size = 7\n")
        );
        assert!(ran.screen().contains("vtium config look set big --size 7"));
        // on the frit, moving the cursor sends the live console nothing,
        // and t tries the cursor's frit on it (ADR-1201 D4)
        let b = bench("look-frit");
        let ran = run(&b, &[RIGHT, RIGHT, b"\r", DOWN, LEFT]);
        assert!(
            !ran.console
                .log
                .iter()
                .any(|l| l.starts_with("set ") || l.starts_with("refuse ")),
            "{:?}",
            ran.console.log
        );
        let ran = run(&b, &[RIGHT, RIGHT, b"\r", DOWN, LEFT, b"t"]);
        assert!(
            ran.console
                .log
                .iter()
                .any(|l| l.starts_with("set ") || l.starts_with("refuse ")),
            "{:?}",
            ran.console.log
        );
        assert_eq!(b.draft(), INSTALLED);
    }

    /// On a look's card, the frit, the size and the cell under the cursor
    /// are each previewed as a picture of the console, in the columns'
    /// place, and the live console is sent nothing (ADR-1201 D4).
    #[test]
    fn a_look_s_card_previews_its_frit_size_and_cell_as_pictures() {
        let b = bench("look-picture");
        let mut console = Console::vtium(100, 30, true, "unifont")
            .takes("proof", (100, 30))
            .takes("unifont", (100, 30))
            .keys(&[
                RIGHT, RIGHT, b"\r", DOWN, RIGHT, DOWN, RIGHT, RIGHT, DOWN, b"\r", b"12x24",
            ]);
        console.graphics = true;
        let mut desk = b.desk(vec![present(vec![keyboard()], true)]);
        let mut i = inputs();
        i.view = model::View::Flow;
        i.flow = vtium_config::tui::flow::Flow::default();
        drive_with(&mut console, i, Some(&mut desk)).expect("the loop");
        assert!(
            !console.log.iter().any(|l| l.starts_with("set ")),
            "{:?}",
            console.log
        );
        let seen: Vec<(&str, u32)> = console
            .shots
            .iter()
            .map(|s| (s.font.as_str(), s.height))
            .collect();
        // the frit at the look's automatic size, drawn at 12 points, and
        // the next frit under the cursor; on the size, the look's frit at
        // automatic, 6 and 7 points; then automatic again on the cell field
        assert_eq!(
            &seen[..6],
            [
                ("proof", 16),
                ("tiny5", 16),
                ("proof", 16),
                ("proof", 8),
                ("proof", 9),
                ("proof", 16)
            ]
        );
        // the cell as typed sets the cell, with no question to the helper
        let last = console.shots.last().expect("a picture");
        assert_eq!(
            (last.cell.width, last.cell.height, last.grid, last.display),
            (12, 24, (53, 20), (640, 480))
        );
        assert!(!console.cells.contains(&("proof".to_owned(), 24)));
        // the picture takes the columns' place, below the base card
        let at = String::from_utf8_lossy(&console.written).into_owned();
        assert!(
            at.contains("\x1b[5;2H\x1b_Ga=T"),
            "placed below the base card"
        );
        // and the columns, the input device's among them, are not drawn
        // under it
        let screen = console.seen.clone().expect("the screen");
        assert!(!screen.contains("kbd"), "{screen}");
    }

    // ADR-1403 D5: the look card offers the look's cell, after its size,
    // and sets it through `look set --cell`
    #[test]
    fn a_look_s_card_sets_its_cell() {
        let b = bench("look-cell");
        // desk's look, open, Down past its name, frit and size to its
        // cell, typed
        let ran = run(
            &b,
            &[
                RIGHT, RIGHT, b"\r", DOWN, DOWN, DOWN, b"\r", b"12x24", b"\r",
            ],
        );
        assert_eq!(
            b.draft(),
            INSTALLED.replace("[look big]\n", "[look big]\ncell = 12x24\n"),
            "{}",
            ran.screen()
        );
        assert!(
            ran.screen()
                .contains("vtium config look set big --cell 12x24"),
            "{}",
            ran.screen()
        );
    }

    // the look card shows each key's value from the draft, the keys past
    // the frit and the size included
    #[test]
    fn a_look_s_card_shows_each_key_the_draft_sets() {
        let b = bench("look-values");
        b.desk(Vec::new());
        let text = INSTALLED.replace(
            "[look big]\n",
            "[look big]\ncell = 12x24\ndisplay-depth = gray16\ndisplay-dpi = 144\n",
        );
        b.store().write_draft("bench", &text).expect("the draft");
        let row = |ran: &Ran, key: &str| {
            ran.screen()
                .lines()
                .find(|l| l.contains(&format!(" {key} ")))
                .unwrap_or_default()
                .to_owned()
        };
        // desk's look, open on its name, shows its cell
        let ran = run(&b, &[RIGHT, RIGHT, b"\r"]);
        assert!(row(&ran, "cell").contains("12x24"), "{}", ran.screen());
        assert!(
            !row(&ran, "font-engine").contains("12x24"),
            "{}",
            ran.screen()
        );
        // Down to its last field, the card scrolled to its display keys
        let mut keys: Vec<&[u8]> = vec![RIGHT, RIGHT, b"\r"];
        keys.extend([DOWN; 8]);
        let ran = run(&b, &keys);
        for (key, value) in [("display-depth", "gray16"), ("display-dpi", "144")] {
            assert!(row(&ran, key).contains(value), "{key}: {}", ran.screen());
        }
    }

    #[test]
    fn new_head_asks_its_name_and_look_and_adds_it() {
        let b = bench("new-head");
        // new head… follows the heads: its name typed, its look picked,
        // then add
        let keys: Vec<&[u8]> = vec![
            RIGHT, DOWN, DOWN, b"\r", b"\r", b"side", b"\r", b"\r", b"\r",
        ];
        let ran = run(&b, &keys);
        assert!(
            b.draft().contains("[head side]\nlook = big\n"),
            "{}\n{}",
            b.draft(),
            ran.screen()
        );
    }

    #[test]
    fn deleting_a_head_names_its_rules_and_keeps_them_broken() {
        let b = bench("head-rm");
        let ran = run(&b, &[RIGHT, b"\r", END, b"\r"]);
        assert!(
            ran.screen()
                .contains("Rule 1 sends to it and will not work"),
            "{}",
            ran.screen()
        );
        // without a yes nothing changes
        assert_eq!(b.draft(), INSTALLED);
        run(&b, &[RIGHT, b"\r", END, b"\r", b"y"]);
        let draft = b.draft();
        assert!(!draft.contains("[head desk]"), "{draft}");
        assert!(draft.contains("display * -> desk"), "{draft}");
    }

    #[test]
    fn deleting_a_look_sets_its_heads_to_the_default_look() {
        let b = bench("look-rm");
        let ran = run(&b, &[RIGHT, RIGHT, b"\r", END, b"\r"]);
        assert!(
            ran.screen()
                .contains("Head desk uses it and will switch to the default look."),
            "{}",
            ran.screen()
        );
        run(&b, &[RIGHT, RIGHT, b"\r", END, b"\r", b"y"]);
        let draft = b.draft();
        assert!(!draft.contains("[look big]"), "{draft}");
        assert!(draft.contains("[head desk]\n#look = big\n"), "{draft}");
    }

    #[test]
    fn connecting_a_display_and_a_head_adds_its_rule_and_clears_the_tags() {
        let b = bench("connect");
        // the display, tagged; panel, tagged; c
        let seen = vec![present(vec![display(), keyboard()], true)];
        let ran = run_on(&b, seen, true, &[b" ", RIGHT, RIGHT, DOWN, b" ", b"c"]);
        assert_eq!(
            b.draft(),
            INSTALLED.replace(
                "display * -> desk",
                "display edid:AAA:0x0001:S1 -> panel\ndisplay * -> desk"
            )
        );
        assert!(
            ran.screen()
                .contains("vtium config rule add 'display edid:AAA:0x0001:S1 -> panel'"),
            "{}",
            ran.screen()
        );
        assert!(!ran.status().contains("tagged"), "{}", ran.status());
        // a display tagged with roaming describes no change and says why
        let b = bench("connect-refused");
        let seen = vec![present(vec![display(), keyboard()], true)];
        let ran = run_on(
            &b,
            seen,
            true,
            &[b" ", RIGHT, RIGHT, DOWN, DOWN, DOWN, b" ", b"c"],
        );
        assert_eq!(b.draft(), INSTALLED);
        assert!(
            ran.screen().contains("A display cannot be sent to roaming"),
            "{}",
            ran.screen()
        );
    }

    #[test]
    fn install_asks_with_its_counts_then_installs_and_keeps_the_old_file() {
        let b = bench("install");
        let change: Vec<&[u8]> = vec![b"\r", RIGHT, b"\r", ESC];
        let asked = run(&b, &[&change[..], &[b"i"]].concat());
        assert!(
            asked
                .screen()
                .contains("Install the draft? 1 line added, 1 removed."),
            "{}",
            asked.screen()
        );
        assert!(asked.console.saves.is_empty());
        assert_eq!(b.installed(), INSTALLED);
        let ran = run(&b, &[b"i", b"y"]);
        assert_eq!(ran.console.saves.len(), 1);
        assert_eq!(b.installed(), b.draft());
        assert!(b.installed().contains("display * -> panel"));
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

    /// Restore's list and a question stay open on Tab and Shift-Tab: the
    /// list does nothing with them, and the question takes them as no.
    #[test]
    fn tab_leaves_neither_restore_s_list_nor_a_question_open_behind() {
        let b = bench("tab-restore");
        run(&b, &[b"\r", RIGHT, b"\r", ESC, b"i", b"y"]);
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
        let ran = run(&b, &[b"\r", RIGHT, b"\r", ESC, b"r", TAB]);
        assert!(ran.said().contains(words::NOT_DONE), "{}", ran.screen());
        assert!(ran.status().contains("1 change"), "{}", ran.status());
        let line = ran.screen().lines().last().unwrap_or_default().trim();
        assert!(line.ends_with("q quit"), "{}", ran.screen());
    }

    #[test]
    fn install_refuses_a_draft_made_before_the_installed_file_changed() {
        let b = bench("install-stale");
        // the draft made, then root edits the installed file by hand
        let change: Vec<&[u8]> = vec![b"\r", RIGHT, b"\r", ESC];
        run(&b, &change);
        let edited = INSTALLED.replace("font-size=12", "font-size=14");
        std::fs::write(b.root.join("etc/vtium.conf"), &edited).expect("the hand edit");
        let ran = run(&b, &[b"i"]);
        assert!(ran.said().contains("changed after"), "{}", ran.screen());
        // and no question asked, so a y installs nothing
        let ran = run(&b, &[b"i", b"y"]);
        assert!(ran.console.saves.is_empty());
        assert_eq!(b.installed(), edited);
        assert!(b.draft().contains("display * -> panel"), "{}", b.draft());
    }

    #[test]
    fn install_refuses_a_draft_the_check_fails() {
        let b = bench("install-refused");
        // a base key typed as BAD: the base card's first key, font-name
        let keys: Vec<&[u8]> = vec![LEFT, b"\r", b"\r", b"BAD", b"\r", ESC, b"i"];
        let ran = run(&b, &keys);
        assert!(b.draft().contains("font-name=BAD"), "{}", b.draft());
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
    fn revert_new_and_delete_draft_each_ask_first() {
        let b = bench("revert");
        let change: Vec<&[u8]> = vec![b"\r", RIGHT, b"\r", ESC];
        run(&b, &change);
        assert_ne!(b.draft(), INSTALLED);
        let ran = run(&b, &[b"r", b"n"]);
        assert!(ran.screen().contains(words::NOT_DONE), "{}", ran.screen());
        assert_ne!(b.draft(), INSTALLED);
        run(&b, &[b"r", b"y"]);
        assert_eq!(b.draft(), INSTALLED);
        run(&b, &[b"n", b"y"]);
        assert_eq!(
            b.draft(),
            vtium_config::ops::new_text(vtium_config::drafts::EXAMPLE)
        );
        // the draft deleted, the next one starts from the installed file
        run(&b, &change);
        assert_ne!(b.draft(), INSTALLED);
        let ran = run(&b, &[b"x", b"y"]);
        assert!(ran.screen().contains("Draft deleted."), "{}", ran.screen());
        assert_eq!(b.draft(), INSTALLED);
    }

    #[test]
    fn the_pager_shows_the_draft_at_the_selected_card_s_line() {
        let b = bench("pager");
        // rule 2, the input path rule, on line 10
        let ran = run(&b, &[RIGHT, RIGHT, RIGHT, b"v"]);
        let screen = ran.screen();
        assert!(screen.contains(words::PAGER_TITLE), "{screen}");
        assert!(
            screen.contains("▸  10 input path:pci-0:3.3 -> panel"),
            "{screen}"
        );
        assert!(screen.contains("q back"), "{screen}");
        // q goes back to the flow, and leaving still works
        let ran = run(&b, &[b"v", b"j", b"G", b"\x04", b"q"]);
        assert!(!ran.screen().contains(words::PAGER_TITLE));
    }

    #[test]
    fn a_device_that_leaves_becomes_its_placeholder_and_one_that_comes_is_placed() {
        let b = bench("hotplug");
        // the keyboard rule 2 names, and a spare one, then the spare alone
        let seen = vec![
            present(vec![keyboard(), other_keyboard()], true),
            present(vec![other_keyboard()], true),
        ];
        // select the keyboard, then udev reports it went: rule 2 names
        // it, so it stays as its placeholder, after the spare, and the
        // selection moves to the placeholder, whose card deletes rule 2
        let keys: Vec<&[u8]> = vec![RIGHT, RIGHT, RIGHT, RIGHT, b"@"];
        let ran = run_on(&b, seen.clone(), true, &keys);
        assert!(
            ran.status().contains("disconnected: kbd"),
            "{}",
            ran.status()
        );
        assert!(ran.screen().contains("* pci-0:3.3"), "{}", ran.screen());
        assert_eq!(b.draft(), INSTALLED);
        let keys: Vec<&[u8]> = vec![RIGHT, RIGHT, RIGHT, RIGHT, b"@", b"\r", b"\r", b"y"];
        run_on(&b, seen, true, &keys);
        assert_eq!(
            b.draft(),
            INSTALLED.replace("input path:pci-0:3.3 -> panel\n", "")
        );
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
    fn two_devices_of_one_name_keep_the_selection_on_the_one_selected() {
        let b = bench("same-name");
        // a second keyboard named kbd, on another port, which rule 3 places
        let mut twin = keyboard();
        twin.path = Some("pci-0:7.7".into());
        let seen = vec![
            present(vec![keyboard(), twin.clone()], true),
            present(vec![twin, keyboard()], true),
        ];
        // the first kbd, rule 2's, selected; udev reorders them; the pager
        // opens at the selected device's rule, still rule 2 on line 10
        let ran = run_on(&b, seen.clone(), true, &[RIGHT, RIGHT, RIGHT, RIGHT, b"v"]);
        assert!(
            ran.screen().contains("▸  10 input path:pci-0:3.3 -> panel"),
            "the first kbd selected: {}",
            ran.screen()
        );
        let ran = run_on(&b, seen, true, &[RIGHT, RIGHT, RIGHT, RIGHT, b"@", b"v"]);
        assert!(
            ran.screen().contains("▸  10 input path:pci-0:3.3 -> panel"),
            "{}",
            ran.screen()
        );
    }

    #[test]
    fn deleting_a_placeholder_deletes_its_rule() {
        let b = bench("placeholder");
        // no keyboard present: rule 2's keyboard is a placeholder, the
        // first item of the input devices column
        let seen = vec![present(vec![], true)];
        run_on(
            &b,
            seen,
            true,
            &[RIGHT, RIGHT, RIGHT, RIGHT, b"\r", b"\r", b"y"],
        );
        assert_eq!(
            b.draft(),
            INSTALLED.replace("input path:pci-0:3.3 -> panel\n", "")
        );
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

    /// The screens of a draft's card, a question and the pager, at 80x25
    /// and 100x30: vtium's own goldens, provisional until the maintainer
    /// reviews the layout.
    #[test]
    fn a_draft_s_card_question_and_pager_at_80x25_and_100x30() {
        let scripts: [(&str, &[&[u8]]); 4] = [
            ("draft-rule-card", &[b"\r"]),
            ("draft-head-card", &[RIGHT, b"\r"]),
            ("draft-delete-head", &[RIGHT, b"\r", END, b"\r"]),
            ("draft-pager", &[RIGHT, RIGHT, RIGHT, b"v"]),
        ];
        for (cols, rows) in [(80, 25), (100, 30)] {
            for (name, keys) in scripts {
                let b = bench(&format!("{name}-{cols}"));
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
