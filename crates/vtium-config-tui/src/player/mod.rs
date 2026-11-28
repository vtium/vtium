//! The macro player: it plays a macro's steps against the TUI's state
//! machine, as the loop's second source of events beside the terminal.
//!
//! Before each step, the player waits until the TUI rests: every effect
//! of the step before has its answer, and no reply from the terminal is
//! due. A key reaches the TUI as a vtium console sends it and the TUI's
//! decoder reads it ([`encode`]), and a key the decoder cannot make is
//! refused when the macro is parsed. A `Click` finds its label ([`label`])
//! and presses the keys a person would press to reach it. A key
//! from the terminal stops the player, and the TUI does not act on it.
//!
//! A macro plays on a scratch copy unless its run is live
//! ([`scratch`]). A live run needs both `Set Live` and the binary's live
//! flag, refuses a document with changes not saved, says LIVE on the top
//! row, and logs each real command through the [`Log`] the binary hands
//! in, before the command runs. Debug mode writes the screens and the
//! failure into a folder ([`debug`]). [`headless`] plays a macro with no
//! terminal at all, and on the terminal for `--macro`.
//!
//! A failure names its file, its line, the step, what it expected, the
//! screen, and the keys from the start, which played as a macro reach the
//! same screen.
//!
//! References:
//! - ADR-1208 D3: the player, the rest before each step, the keys as a
//!   terminal sends them, and the click through keys.
//! - ADR-1208 D4: visible play, its pace, a key that stops it, headless
//!   play and the exit statuses.
//! - ADR-1208 D5: the scratch copy, the live run and its log.
//! - ADR-1208 D6: debug mode and the failure report.
//! - ADR-1208 D10: the update loop's second source of events.

pub mod debug;
pub mod encode;
pub mod headless;
pub mod label;
pub mod scratch;
mod step;

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use vtium_config_core::ui::Message;
use vtium_config_macro::{KeyPath, Macro, Parser, Step, failure_report};
use vtium_config_widgets::frame::Mode;

use crate::app::{App, Event};
use crate::port::{Next, Source, wake};
use debug::Debug;
use encode::Encoder;
use label::Screen;
use step::Work;

/// The pause between keys in visible play, so a person can follow.
pub const PACE: Duration = Duration::from_millis(100);

/// How often visible play looks again for the next line of a stream that
/// has not come yet.
const STREAM_POLL: Duration = Duration::from_millis(20);

/// How a run is paced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// Visible play: [`PACE`] between keys, and each `Sleep` on top.
    Visible,
    /// Tests and headless play: no pause, and `Sleep` ignored.
    Off,
}

/// Where a live run writes the line it logs before each real command:
/// the system journal, or standard error where the machine has none. The
/// binary hands it in.
pub trait Log {
    /// Writes `line`. `Err` says why it could not, and the command then
    /// does not run.
    fn line(&mut self, line: &str) -> Result<(), String>;
}

/// What the binary asks of a run.
pub struct Options {
    /// The macro's name as messages give it, such as `cullet-11.vtmacro`,
    /// or `-` for standard input.
    pub file: String,
    /// Whether the command line asks for a live run, `--live`.
    pub live: bool,
    /// The run's pace.
    pub pace: Pace,
    /// Debug mode's folder, `--macro-debug DIR`.
    pub debug: Option<Debug>,
    /// Where a live run logs each real command.
    pub log: Option<Box<dyn Log>>,
    /// Where headless play writes each step's line and its result.
    pub out: Option<Box<dyn Write>>,
}

impl Options {
    /// A run of the macro `file` with no live flag, no debug folder, no
    /// log and no output, at `pace`.
    pub fn new(file: impl Into<String>, pace: Pace) -> Options {
        Options {
            file: file.into(),
            live: false,
            pace,
            debug: None,
            log: None,
            out: None,
        }
    }
}

/// How a run went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The run plays on.
    Playing,
    /// Every step passed.
    Passed,
    /// A step failed: its line, and the report.
    Failed { line: usize, report: String },
    /// A key from the terminal stopped the run before the step on `line`
    /// was done.
    Stopped { line: usize },
    /// The run was refused, or a line of a stream does not parse: the
    /// line that says why, and the exit status.
    Refused { why: String, status: u8 },
}

impl Outcome {
    /// The exit status the run leaves: 0 when every step passed, 1 for a
    /// failed step, a stop or a refusal, and 2 for a macro that does not
    /// parse.
    pub fn status(&self) -> u8 {
        match self {
            Outcome::Playing | Outcome::Passed => 0,
            Outcome::Failed { .. } | Outcome::Stopped { .. } => 1,
            Outcome::Refused { status, .. } => *status,
        }
    }
}

/// A refusal before the first step: its line, and the exit status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub why: String,
    pub status: u8,
}

/// Where the steps come from.
enum Steps {
    /// A macro file, parsed and checked whole before the first step.
    File { steps: Vec<Step>, at: usize },
    /// A stream read on the player's own thread, as `--macro -` reads
    /// standard input, each line parsed as it arrives.
    Stream {
        lines: Receiver<Result<Step, (usize, String)>>,
        at: usize,
    },
    /// A stream read as the player asks for each line, which only headless
    /// play may wait on.
    Blocking {
        reader: Box<dyn BufRead>,
        parser: Parser,
        at: usize,
    },
}

/// The next line of the steps.
enum Line {
    Step(usize, Step),
    /// A stream's next line has not come yet.
    Pending,
    End,
    /// A line of a stream that does not parse: its number and why.
    Bad(usize, String),
}

impl Steps {
    fn next(&mut self) -> Line {
        match self {
            Steps::File { steps, at } => match steps.get(*at) {
                Some(step) => {
                    *at = at.saturating_add(1);
                    Line::Step(*at, step.clone())
                }
                None => Line::End,
            },
            Steps::Stream { lines, at } => match lines.try_recv() {
                Ok(Ok(step)) => {
                    *at = at.saturating_add(1);
                    Line::Step(*at, step)
                }
                Ok(Err((line, why))) => Line::Bad(line, why),
                Err(TryRecvError::Empty) => Line::Pending,
                Err(TryRecvError::Disconnected) => Line::End,
            },
            Steps::Blocking { reader, parser, at } => match parser.read(reader) {
                Some(Ok(step)) => {
                    *at = at.saturating_add(1);
                    Line::Step(*at, step)
                }
                Some(Err(e)) => Line::Bad(e.line(), e.message().to_owned()),
                None => Line::End,
            },
        }
    }
}

/// The player of one macro.
pub struct Player {
    options: Options,
    steps: Steps,
    encoder: Encoder,
    /// The step under way, with its line, and the work it has left.
    current: Option<(usize, Step)>,
    work: VecDeque<Work>,
    /// Every key pressed from the start.
    path: KeyPath,
    /// When the next piece of work may play, in visible play.
    due: Option<Instant>,
    /// Whether an acting step has played, after which `Set Live` may not
    /// come.
    begun: bool,
    /// A real command that could not be logged, which fails the step at
    /// the next rest.
    unlogged: Option<String>,
    outcome: Outcome,
    /// The screen when the run ended.
    last: Vec<String>,
}

impl Player {
    /// The player of the macro `m`. Every key step and every character of
    /// a `Type` is checked first: a key that the TUI's decoder does not
    /// make from a terminal's bytes refuses the macro with exit status 2,
    /// before the first step. `Set Live` without the live flag, or the
    /// flag without `Set Live`, refuses it with status 1.
    pub fn new(m: &Macro, options: Options) -> Result<Player, Refusal> {
        let mut encoder = Encoder::new();
        for (line, s) in m.numbered() {
            check(&mut encoder, s).map_err(|why| Refusal {
                why: format!("{}:{line}: {why}", options.file),
                status: 2,
            })?;
        }
        if let Some(why) = words::live_mismatch(m.is_live(), options.live) {
            return Err(Refusal { why, status: 1 });
        }
        Ok(Player::with(
            Steps::File {
                steps: m.steps().to_vec(),
                at: 0,
            },
            options,
            encoder,
        ))
    }

    /// The player of a stream of steps, such as standard input, read on a
    /// thread of its own so the loop never waits on it. Each line is
    /// parsed as it arrives, and a line that does not parse ends the run
    /// at that line with exit status 2.
    pub fn stream(reader: impl BufRead + Send + 'static, options: Options) -> Player {
        let (send, lines) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = reader;
            let mut parser = Parser::new();
            while let Some(read) = parser.read(&mut reader) {
                let stop = read.is_err();
                let sent = send.send(read.map_err(|e| (e.line(), e.message().to_owned())));
                if stop || sent.is_err() {
                    break;
                }
            }
        });
        Player::with(Steps::Stream { lines, at: 0 }, options, Encoder::new())
    }

    /// The player of a stream that headless play reads as it asks for each
    /// line, on its own thread.
    pub fn blocking(reader: impl BufRead + 'static, options: Options) -> Player {
        Player::with(
            Steps::Blocking {
                reader: Box::new(reader),
                parser: Parser::new(),
                at: 0,
            },
            options,
            Encoder::new(),
        )
    }

    fn with(steps: Steps, options: Options, encoder: Encoder) -> Player {
        Player {
            options,
            steps,
            encoder,
            current: None,
            work: VecDeque::new(),
            path: KeyPath::new(),
            due: None,
            begun: false,
            unlogged: None,
            outcome: Outcome::Playing,
            last: Vec::new(),
        }
    }

    /// Whether the run is live.
    pub fn live(&self) -> bool {
        self.options.live
    }

    /// Readies `app` for the run: a live run refuses a document with
    /// changes not saved, and has a log to write to. The top row then
    /// says LIVE, or "scratch copy".
    pub fn start(&mut self, app: &mut App) -> Result<(), Refusal> {
        if self.options.live {
            if app.shared().document().count() > 0 {
                return Err(self.refuse(words::LIVE_UNSAVED.to_owned(), 1));
            }
            if self.options.log.is_none() {
                return Err(self.refuse(words::NO_LOG.to_owned(), 1));
            }
            app.set_macro_mode(Some(Mode::Live));
        } else {
            app.set_macro_mode(Some(Mode::ScratchCopy));
        }
        Ok(())
    }

    fn refuse(&mut self, why: String, status: u8) -> Refusal {
        self.outcome = Outcome::Refused {
            why: why.clone(),
            status,
        };
        Refusal { why, status }
    }

    /// How the run went so far.
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    /// Every key pressed from the start.
    pub fn path(&self) -> &KeyPath {
        &self.path
    }

    /// The screen as the run ended, one line for each row.
    pub fn last_screen(&self) -> &[String] {
        &self.last
    }

    fn headless(&self) -> bool {
        self.options.out.is_some()
    }

    fn say(&mut self, text: &str) {
        if let Some(out) = &mut self.options.out {
            let _ = writeln!(out, "{text}");
            let _ = out.flush();
        }
    }

    /// The step on `line` failed for `reason`: the report goes to the
    /// debug folder, to standard output in headless play, and to the
    /// message row in visible play.
    fn fail(&mut self, app: &App, line: usize, step: &Step, reason: &str) -> Next {
        let screen = Screen::of(app).rows();
        let mut report = failure_report(&self.options.file, line, step, reason, &self.path);
        report.push_str("\nThe screen:\n");
        report.push_str(&screen.join("\n"));
        self.last = screen;
        if let Some(d) = &self.options.debug {
            let _ = d.failure(&report);
        }
        self.say(&report);
        self.work.clear();
        self.current = None;
        let first = report.lines().next().unwrap_or_default().to_owned();
        self.outcome = Outcome::Failed { line, report };
        if self.headless() {
            return Next::Leave(1);
        }
        Next::Event(Event::Said(Message {
            line: format!("{first} [F1] More"),
            more: match &self.outcome {
                Outcome::Failed { report, .. } => Some(report.clone()),
                _ => None,
            },
        }))
    }

    /// The step on `line` is done: its screen goes to the debug folder,
    /// and its line to standard output in headless play.
    fn finished(&mut self, app: &App, line: usize, step: &Step) {
        if let Some(d) = &self.options.debug {
            let _ = d.step(line, &Screen::of(app).rows());
        }
        let text = format!("{}:{line}: {step}: ok", self.options.file);
        self.say(&text);
    }

    /// The run is over: every step passed.
    fn ended(&mut self, app: &App, line: usize) -> Next {
        self.last = Screen::of(app).rows();
        self.outcome = Outcome::Passed;
        if self.headless() {
            return Next::Leave(0);
        }
        Next::Event(Event::Said(Message {
            line: words::ended(line),
            more: None,
        }))
    }

    /// Plays the next piece of work, or begins the next step.
    fn play(&mut self, app: &App) -> Next {
        loop {
            if let Some(due) = self.due {
                let now = wake::now();
                if now < due {
                    return Next::Pause(due.saturating_duration_since(now));
                }
                self.due = None;
            }
            let (line, current) = match &self.current {
                Some((line, step)) => (*line, step.clone()),
                None => (0, Step::Blank),
            };
            if let Some(why) = self.unlogged.take() {
                return self.fail(app, line, &current, &why);
            }
            match self.work.pop_front() {
                Some(Work::Key(press)) => match self.encoder.key(press) {
                    Ok(key) => {
                        self.path.push(press);
                        self.paced();
                        return Next::Event(Event::Key(key));
                    }
                    Err(why) => return self.fail(app, line, &current, &why),
                },
                Some(Work::Mouse(input)) => {
                    if let Some(event) = step::mouse(input) {
                        self.paced();
                        return Next::Event(event);
                    }
                    continue;
                }
                Some(Work::Seek(mut seek)) => match seek.next(app) {
                    Ok(Some(press)) => {
                        self.work.push_front(Work::Seek(seek));
                        self.work.push_front(Work::Key(press));
                        continue;
                    }
                    Ok(None) => continue,
                    Err(why) => return self.fail(app, line, &current, &why),
                },
                None => {}
            }
            if let Some((line, step)) = self.current.take() {
                self.finished(app, line, &step);
            }
            match self.steps.next() {
                Line::Step(line, step) => {
                    if !step.acts() {
                        continue;
                    }
                    if let Some(next) = self.begin(app, line, &step) {
                        return next;
                    }
                }
                Line::Pending => return Next::Pause(STREAM_POLL),
                Line::End => {
                    let line = match &self.steps {
                        Steps::File { at, .. }
                        | Steps::Stream { at, .. }
                        | Steps::Blocking { at, .. } => *at,
                    };
                    return self.ended(app, line);
                }
                Line::Bad(line, why) => return self.unparsed(line, &why),
            }
        }
    }

    /// The line `line` of a stream does not parse, or presses a key the
    /// TUI's decoder cannot make: the run ends there with status 2.
    fn unparsed(&mut self, line: usize, why: &str) -> Next {
        let why = format!("{}:{line}: {why}", self.options.file);
        self.ends(why, 2)
    }

    /// The run ends, refused with `why` and `status`: the line goes to
    /// standard output in headless play, and to the message row in
    /// visible play.
    fn ends(&mut self, why: String, status: u8) -> Next {
        self.say(&why);
        self.work.clear();
        self.current = None;
        self.outcome = Outcome::Refused {
            why: why.clone(),
            status,
        };
        if self.headless() {
            return Next::Leave(status);
        }
        Next::Event(Event::Said(Message {
            line: why,
            more: None,
        }))
    }

    /// Begins the step on `line`: its work, its check, its pause or its
    /// screenshot. Returns what the loop does when the step fails or
    /// refuses at once, and `None` when it goes on.
    fn begin(&mut self, app: &App, line: usize, step: &Step) -> Option<Next> {
        // a stream's lines are checked as they come: a key the decoder
        // cannot make ends the run as a line that does not parse, and a
        // live run asked for by one side alone is refused
        if matches!(self.steps, Steps::Stream { .. } | Steps::Blocking { .. }) {
            if let Err(why) = check(&mut self.encoder, step) {
                return Some(self.unparsed(line, &why));
            }
            let live = matches!(step, Step::SetLive);
            let mismatch = if live && !self.options.live {
                words::live_mismatch(true, false)
            } else if !live && !self.begun && self.options.live {
                words::live_mismatch(false, true)
            } else {
                None
            };
            if let Some(why) = mismatch {
                return Some(self.ends(why, 1));
            }
        }
        self.begun = true;
        self.current = Some((line, step.clone()));
        match step {
            Step::Sleep(pause) if self.options.pace == Pace::Visible => {
                let from = self.due.unwrap_or_else(wake::now);
                self.due = from.checked_add(pause.duration());
            }
            Step::Screenshot(name) => {
                if let Some(d) = &self.options.debug
                    && let Err(why) = d.screenshot(name, &Screen::of(app).rows())
                {
                    return Some(self.fail(app, line, step, &why));
                }
            }
            _ => {}
        }
        match step::begin(app, step, &mut self.encoder) {
            Ok(work) => {
                self.work.extend(work);
                None
            }
            Err(why) => Some(self.fail(app, line, step, &why)),
        }
    }

    /// In visible play, the next key waits [`PACE`].
    fn paced(&mut self) {
        if self.options.pace == Pace::Visible {
            self.due = wake::now().checked_add(PACE);
        }
    }

    /// The line of the step under way, and the step.
    fn here(&self) -> (usize, String) {
        match &self.current {
            Some((line, step)) => (*line, step.to_string()),
            None => (0, String::new()),
        }
    }
}

/// Checks that each key a step presses reaches the TUI as a key: a key
/// step's key, and each character a `Type` types.
fn check(encoder: &mut Encoder, step: &Step) -> Result<(), String> {
    match step {
        Step::Key(k) => encoder.key(k.press()).map(|_| ()),
        Step::Type(text) => {
            for c in text.as_str().chars() {
                encoder.key(step::typed(c)?)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

impl Source for Player {
    fn rested(&mut self, app: &App) -> Next {
        match &self.outcome {
            Outcome::Playing => self.play(app),
            done if self.headless() => Next::Leave(done.status()),
            _ => Next::Idle,
        }
    }

    fn stop(&mut self) -> Option<Message> {
        if self.outcome != Outcome::Playing {
            return None;
        }
        let (line, step) = self.here();
        self.work.clear();
        self.outcome = Outcome::Stopped { line };
        Some(Message {
            line: words::stopped(line, &step),
            more: None,
        })
    }

    fn before_real(&mut self, command: &str) -> Result<(), String> {
        if !self.options.live {
            return Ok(());
        }
        let (line, _) = self.here();
        let text = words::logged(&self.options.file, line, command);
        let logged = match &mut self.options.log {
            Some(log) => log.line(&text),
            None => Err(words::NO_LOG.to_owned()),
        };
        if let Err(why) = logged {
            let why = words::unlogged(&why);
            self.unlogged = Some(why.clone());
            return Err(why);
        }
        if let Some(d) = &self.options.debug {
            let _ = d.live(&text);
        }
        Ok(())
    }

    fn left(&mut self, app: &App, status: u8) -> u8 {
        if self.outcome == Outcome::Playing {
            if let Some((line, step)) = self.current.take() {
                self.finished(app, line, &step);
            }
            self.work.clear();
            loop {
                match self.steps.next() {
                    Line::Step(line, step) => {
                        if let Err(why) = step::after_exit(&step, status) {
                            let _ = self.fail(app, line, &step, &why);
                            break;
                        }
                        if step.acts() {
                            self.finished(app, line, &step);
                        }
                    }
                    Line::Bad(line, why) => {
                        let why = format!("{}:{line}: {why}", self.options.file);
                        self.say(&why);
                        self.outcome = Outcome::Refused { why, status: 2 };
                        break;
                    }
                    Line::End | Line::Pending => {
                        self.last = Screen::of(app).rows();
                        self.outcome = Outcome::Passed;
                        break;
                    }
                }
            }
        }
        match self.outcome.status() {
            0 => status,
            failed => failed,
        }
    }
}

/// The words the player puts on the screen and in its log.
pub mod words {
    /// A live run with changes not saved.
    pub const LIVE_UNSAVED: &str = "vtium-config: the configuration has changes that are not saved: save or revert them before a live run.";

    /// A live run with no log to write each real command to.
    pub const NO_LOG: &str =
        "vtium-config: a live run has no log to write its commands to, so it does not run.";

    /// Install as a system font on a scratch copy.
    pub const SCRATCH_INSTALL: &str = "On a scratch copy, nothing is installed.";

    /// The line that refuses a live run asked for by one side alone, or
    /// `None` where both or neither ask.
    pub fn live_mismatch(set_live: bool, flag: bool) -> Option<String> {
        match (set_live, flag) {
            (true, false) => Some("vtium-config: the macro asks for a live run with Set Live, and the command line has no --live.".to_owned()),
            (false, true) => Some("vtium-config: --live needs a macro whose first step is Set Live.".to_owned()),
            _ => None,
        }
    }

    /// The command Save runs, as a live run logs it.
    pub fn save_command(name: &str) -> String {
        format!(
            "sudo vtium-config apply, saving {name} to {}",
            crate::save::privileged::TARGET
        )
    }

    /// The line a live run logs before a real command.
    pub fn logged(file: &str, line: usize, command: &str) -> String {
        format!("live macro {file} line {line} runs: {command}")
    }

    /// A real command whose line could not be logged.
    pub fn unlogged(why: &str) -> String {
        format!("vtium-config cannot log the command, so it does not run it: {why}")
    }

    /// The message row after a key from the terminal stopped the macro.
    pub fn stopped(line: usize, step: &str) -> String {
        if line == 0 {
            return "A key from the terminal stopped the macro before its first step.".to_owned();
        }
        format!("A key from the terminal stopped the macro at line {line}: {step}.")
    }

    /// The message row once the macro ended.
    pub fn ended(line: usize) -> String {
        format!("The macro ended after line {line}. The keys are yours again.")
    }

    /// `Expect Saved` with changes not saved.
    pub fn not_saved(n: usize) -> String {
        let changes = if n == 1 { "change is" } else { "changes are" };
        format!("{n} {changes} not saved.")
    }

    /// A scratch copy that could not be made.
    pub fn cannot_copy(path: &std::path::Path, e: &std::io::Error) -> String {
        format!(
            "vtium-config cannot make the scratch copy in {}: {e}",
            path.display()
        )
    }

    /// Install as a system font… on a scratch copy.
    pub fn scratch_font(source: &std::path::Path) -> String {
        format!(
            "On a scratch copy, vtium-config would ready {} as a system font, and changes nothing.",
            source.display()
        )
    }

    /// A Save on a scratch copy whose draft is missing.
    pub fn scratch_no_draft(name: &str) -> String {
        format!("The scratch copy holds no draft {name}.")
    }

    /// What a Save on a scratch copy says it would do.
    pub fn scratch_saved(command: &str, files: &[String]) -> String {
        format!(
            "On a scratch copy, Save changes nothing. It would run {command}, which would change {}.",
            files.join(", ")
        )
    }
}
