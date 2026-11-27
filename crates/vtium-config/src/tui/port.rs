//! The loop around the state machine (ADR-1201 D7), and the terminal it
//! reads and writes.
//!
//! [`drive`] is the one loop: it takes what the [`Port`] gives, decodes the
//! bytes into keys and replies, hands each to the state machine, carries
//! out the effects, and draws the screen whole after each turn. The
//! decoder asks the state machine before each input whether a reply is
//! awaited, which decides what an ESC that ends a read is. A test
//! drives the same loop through a port of its own.
//!
//! [`Tty`] is the real port, the terminal on standard input and output: in
//! raw mode on the alternate screen (DEC mode 1049) with the cursor hidden
//! (DEC mode 25), with SIGTERM, SIGINT, SIGQUIT, SIGHUP and SIGWINCH held
//! and read in the loop, as `set` holds them. Dropping it puts the screen,
//! the cursor and the terminal's mode back; a panic puts them back from the
//! panic hook, which also gives the console its start font back, since a
//! release build aborts on a panic without dropping anything.

use std::ffi::OsString;
use std::io::{self, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rustix::termios::{OptionalActions, QueueSelector, Termios, tcflush, tcgetattr, tcsetattr};
use vtium_platform::calloop::channel::{Event as Sent, Sender, channel};
use vtium_platform::calloop::generic::Generic;
use vtium_platform::calloop::{Interest, Mode, PostAction};
use vtium_platform::event_loop::{Loop, Signal};

use vtium_platform::watch::Watch;

mod kept;

use super::desk::Desk;
use super::desk::Done;
use super::input::Decoder;
use super::model::{self, App, Effect, Event, Inputs, Live};
use super::view;
use crate::apply::{Ask, Runner};
use kept::{Kept, measured, painted};

/// What runs while the terminal is given back: the install, through the
/// runner it is handed, writing what the person reads to the terminal it
/// is handed and asking its questions there (ADR-1201 D5).
pub type Step<'a> = &'a mut dyn FnMut(Runner<'_>, &mut dyn Write, Ask<'_>) -> Result<Done, String>;

/// What woke the loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wake {
    Bytes(Vec<u8>),
    /// The sources each frit lacks, which [`Port::sources`] asked for.
    Sourced(Vec<(String, super::model::SourcesFound)>),
    /// What the font helper said of some of the files [`Port::describe`]
    /// named, as the answers came.
    Described(Vec<(PathBuf, crate::discover::Described)>),
    Resize,
    Signal(i32),
    Timeout,
    /// udev reported that a device came or went.
    Hotplug,
    /// A preview [`Port::draw`] was asked for has come.
    Drawn(Drawn),
}

/// A preview the loop asks the glyph helper for (ADR-1201 D4), which comes
/// back later as [`Wake::Drawn`] while the loop goes on reading keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Draw {
    /// The font's cell at a pixel height.
    Cell { font: String, height: u32 },
    /// A picture of the console.
    Picture(crate::specimen::Shot),
    /// The detailed specimen of a font at a pixel height, in at most so
    /// many pixels.
    Specimen {
        font: String,
        height: u32,
        max: (u32, u32),
        /// The display whose grid the specimen's summary counts.
        display: (u32, u32),
    },
    /// The line that describes the font at a height, for a terminal that
    /// draws no pictures.
    Summary { font: String, height: u32 },
}

/// What a [`Draw`] gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drawn {
    Celled {
        font: String,
        height: u32,
        cell: Result<(vtium_font::Cell, u32), String>,
    },
    Pictured(Result<crate::specimen::Picture, String>),
    /// A picture drawn through an effect, and what became of the effect.
    Filtered {
        picture: crate::specimen::Picture,
        outcome: crate::specimen::Outcome,
    },
    /// The detailed specimen, and what it gives: the cell, the grid on the
    /// display and how many of its characters the font draws itself.
    Detailed {
        font: String,
        height: u32,
        result: Result<(crate::specimen::Picture, crate::specimen::Summary), String>,
    },
    Summarized {
        font: String,
        height: u32,
        summary: Result<crate::specimen::Summary, String>,
    },
}

impl Drawn {
    /// The state machine's event for what came.
    fn event(self) -> Event {
        match self {
            Drawn::Celled { font, height, cell } => Event::Celled { font, height, cell },
            Drawn::Pictured(p) => Event::Pictured(p),
            Drawn::Filtered { picture, outcome } => Event::Filtered { picture, outcome },
            Drawn::Detailed {
                font,
                height,
                result,
            } => Event::Detailed {
                font,
                height,
                result,
            },
            Drawn::Summarized {
                font,
                height,
                summary,
            } => Event::Summarized {
                font,
                height,
                summary,
            },
        }
    }
}

/// The terminal the loop runs against.
pub trait Port {
    /// The grid, in columns and rows.
    fn size(&mut self) -> (usize, usize);
    /// Writes `bytes` whole.
    fn write(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Starts a wait of `after`, which ends in [`Wake::Timeout`].
    fn arm(&mut self, after: Duration);
    /// Waits for what comes next.
    fn wait(&mut self) -> io::Result<Vec<Wake>>;
    /// Runs `step` with the terminal given back for sudo's password,
    /// handing it the runner that runs the privileged step's command.
    fn privileged(&mut self, step: Step<'_>) -> Result<Done, String>;
    /// What to write if the TUI panics: the switch back to the start font.
    fn on_panic(&mut self, bytes: Vec<u8>);
    /// The terminal's cell in pixels, where the terminal reports its size
    /// in pixels.
    fn cell_px(&mut self) -> Option<(u32, u32)>;
    /// Asks the glyph helper for a preview without waiting: the answer
    /// comes later, as [`Wake::Drawn`], and the loop reads keys meanwhile.
    fn draw(&mut self, ask: Draw);
    /// Asks which sources each frit of `fonts` lacks or refuses, without
    /// waiting: the answers come later, as [`Wake::Sourced`], and the loop
    /// reads keys meanwhile.
    fn sources(&mut self, fonts: Vec<String>);
    /// Asks the font helper what each of `paths` is, without waiting: the
    /// answers come later, a few at a time, as [`Wake::Described`], and
    /// the loop reads keys meanwhile.
    fn describe(&mut self, paths: Vec<PathBuf>);
}

/// Alternate screen, cursor hidden; and back, with mouse reporting off in
/// case the Flow view left it on.
pub const ENTER: &[u8] = b"\x1b[?1049h\x1b[?25l";
pub const LEAVE: &[u8] = b"\x1b[?1006l\x1b[?1000l\x1b[?25h\x1b[?1049l";

/// The terminal's size as `port` reports it: its grid, and its cell in
/// pixels where it reports one. The loop hands the size to the model in a
/// message and never sets the model's size itself.
///
/// References:
/// - ADR-1206 D2: the size changes only through a message.
fn size_of(port: &mut dyn Port) -> super::model::Size {
    let (cols, rows) = port.size();
    super::model::Size {
        cols,
        rows,
        cell_px: port.cell_px(),
    }
}

/// Runs the TUI from `inputs` on `port` until it leaves, with no desk, so
/// the Flow view shows `inputs`'s flow and changes nothing; returns its
/// exit status.
pub fn drive(port: &mut dyn Port, inputs: Inputs) -> io::Result<u8> {
    drive_with(port, inputs, None)
}

/// Runs the TUI from `inputs` on `port` until it leaves, the Flow view's
/// draft on `desk`; returns its exit status.
pub fn drive_with(
    port: &mut dyn Port,
    mut inputs: Inputs,
    mut desk: Option<&mut dyn Desk>,
) -> io::Result<u8> {
    inputs.desk = desk.is_some();
    let (mut app, first) = App::start(inputs, size_of(port));
    let mut painter = view::Painter::new()?;
    let mut decoder = Decoder::keys();
    let mut queue: Vec<Effect> = first;
    loop {
        // effects may make events, a save's end among them
        while !queue.is_empty() {
            for effect in std::mem::take(&mut queue) {
                match effect {
                    Effect::Write(bytes) => {
                        port.write(&bytes)?;
                        // the picture's cells come back as the text under
                        // them, so the next frame draws whole
                        if bytes == crate::kitty::delete(crate::kitty::IMAGE_ID) {
                            port.write(&painter.forget()?)?;
                        }
                    }
                    Effect::Wait(d) => port.arm(d),
                    Effect::Exit(status) => {
                        port.on_panic(give_back(&app));
                        return Ok(status);
                    }
                    Effect::Cell { font, height } => port.draw(Draw::Cell { font, height }),
                    Effect::Picture(shot) => port.draw(Draw::Picture(shot)),
                    Effect::ScreenPicture(mut shot) => {
                        let (cols, rows) = shot.grid;
                        let size = model::Size {
                            cols,
                            rows,
                            ..app.size
                        };
                        shot.screen = view::screen_at(&app, size);
                        port.draw(Draw::Picture(shot));
                    }
                    Effect::Specimen {
                        font,
                        height,
                        max,
                        display,
                    } => port.draw(Draw::Specimen {
                        font,
                        height,
                        max,
                        display,
                    }),
                    Effect::Summary { font, height } => port.draw(Draw::Summary { font, height }),
                    Effect::Sources(fonts) => port.sources(fonts),
                    Effect::Describe(paths) => port.describe(paths),
                    Effect::Desk(op) => {
                        // readying a system font waits on the font helper,
                        // so the screen first shows that it is checking
                        if matches!(op, super::desk::Op::SystemFont(_)) {
                            port.write(&painter.frame(&app)?)?;
                        }
                        let done = match desk.as_deref_mut() {
                            Some(desk) => desk.run(&op),
                            None => Err(super::words::NO_DESK.to_owned()),
                        };
                        queue.extend(app.event(Event::Desked(op, done)));
                    }
                    Effect::Install(plan) => {
                        port.write(&painter.frame(&app)?)?;
                        let result = match desk.as_deref_mut() {
                            Some(desk) => port.privileged(&mut |run, out, ask| {
                                desk.install(&plan, run, out, ask)
                            }),
                            None => Err(super::words::NO_DESK.to_owned()),
                        };
                        // the privileged step wrote to the terminal, so
                        // the next frame draws whole
                        port.write(&painter.forget()?)?;
                        queue.extend(app.event(Event::Resize(size_of(port))));
                        queue.extend(app.event(Event::Installed(result)));
                    }
                    Effect::InstallFont => {
                        port.write(&painter.frame(&app)?)?;
                        let result = match desk.as_deref_mut() {
                            Some(desk) => port.privileged(&mut |run, _, _| desk.install_font(run)),
                            None => Err(super::words::NO_DESK.to_owned()),
                        };
                        port.write(&painter.forget()?)?;
                        queue.extend(app.event(Event::Resize(size_of(port))));
                        queue.extend(app.event(Event::Installed(result)));
                    }
                }
            }
        }
        port.on_panic(give_back(&app));
        port.write(&painter.frame(&app)?)?;
        for wake in port.wait()? {
            let events = match wake {
                Wake::Bytes(bytes) => {
                    decoder.push(&bytes);
                    Vec::new()
                }
                Wake::Resize => vec![Event::Resize(size_of(port))],
                Wake::Signal(n) => vec![Event::Signal(n)],
                Wake::Sourced(found) => vec![Event::Sourced(found)],
                Wake::Described(answers) => vec![Event::Described(answers)],
                Wake::Timeout => vec![Event::Timeout],
                Wake::Hotplug => vec![Event::Hotplug],
                Wake::Drawn(drawn) => vec![drawn.event()],
            };
            for event in events {
                queue.extend(app.event(event));
            }
            // each input is decoded while the model says whether it awaits
            // a reply, so a reply that settles the last query, or the
            // deadline at start, decides what a lone ESC after it is
            while let Some(input) = decoder.next(app.awaiting()) {
                queue.extend(app.event(Event::Input(input)));
            }
        }
    }
}

/// The bytes that give the console its colors back, when a look's trial
/// changed them, its start font, when a switch changed it, and then its
/// start grid, when the Looks tab changed it.
fn give_back(app: &App) -> Vec<u8> {
    let mut out = super::shot::colors_on_leaving(&app.shared);
    out.extend(font_back(app));
    if app.shared.grid_moved {
        out.extend(super::shot::grid_request(app.shared.start_grid));
    }
    out
}

/// The font switch that gives the console its start font back, where a
/// trial changed it.
fn font_back(app: &App) -> Vec<u8> {
    match (&app.shared.live, &app.shared.start_font) {
        (Live::On, Some(start))
            if app
                .shared
                .in_flight
                .as_ref()
                .or(app.shared.shown_font.as_ref())
                != Some(start) =>
        {
            format!("\x1b]50;{start}\x1b\\").into_bytes()
        }
        _ => Vec::new(),
    }
}

/// What the panic hook restores: the terminal's mode as it was, and what
/// to write first.
struct Restore {
    saved: Termios,
    tty: OwnedFd,
    bytes: Vec<u8>,
}

static RESTORE: Mutex<Option<Restore>> = Mutex::new(None);

/// Puts the terminal back from what [`RESTORE`] holds, once.
fn restore() {
    let taken = RESTORE.lock().ok().and_then(|mut r| r.take());
    if let Some(r) = taken {
        let _ = rustix::io::write(&r.tty, &r.bytes);
        let _ = rustix::io::write(&r.tty, LEAVE);
        let _ = tcflush(&r.tty, QueueSelector::IFlush);
        let _ = tcsetattr(&r.tty, OptionalActions::Now, &r.saved);
    }
}

/// What the loop's sources leave for [`Tty::wait`], and the udev monitor
/// they drain.
#[derive(Default)]
struct Woken {
    wakes: Vec<Wake>,
    udev: Option<Watch>,
}

/// Draws a picture of the console, on a thread of its own.
pub type Painter = std::sync::Arc<
    dyn Fn(&crate::specimen::Shot) -> Result<crate::specimen::Picture, String> + Send + Sync,
>;

/// Draws a picture through an effect, on the painter's thread, and says
/// what became of the effect.
pub type Filterer = std::sync::Arc<
    dyn Fn(&mut crate::specimen::Picture, &crate::specimen::Filtered) -> crate::specimen::Outcome
        + Send
        + Sync,
>;

/// Draws a font's detailed specimen at a pixel height in at most so many
/// pixels, on a thread of its own.
pub type Detailer = std::sync::Arc<
    dyn Fn(
            &str,
            u32,
            (u32, u32),
            (u32, u32),
        ) -> Result<(crate::specimen::Picture, crate::specimen::Summary), String>
        + Send
        + Sync,
>;

/// Measures a font's cell at a pixel height, on a thread of its own.
pub type Measurer =
    std::sync::Arc<dyn Fn(&str, u32) -> Result<(vtium_font::Cell, u32), String> + Send + Sync>;

/// Summarizes a font's specimen at a pixel height, on a thread of its own.
pub type Summarizer =
    std::sync::Arc<dyn Fn(&str, u32) -> Result<crate::specimen::Summary, String> + Send + Sync>;

/// Finds the sources each frit lacks, on a thread of its own.
pub type Finder = Box<dyn FnOnce(Vec<String>) -> Vec<(String, super::model::SourcesFound)> + Send>;

/// Describes font files through the font helper, on a thread of its own,
/// passing each answer on in order as it comes.
pub type Describer =
    std::sync::Arc<dyn Fn(&[PathBuf], &mut dyn FnMut(crate::discover::Described)) + Send + Sync>;

/// How many descriptions the describing thread sends the loop at once.
const DESCRIBED_AT_ONCE: usize = 32;

/// How long the describing thread holds descriptions before it sends
/// those it has.
const DESCRIBED_WAIT: Duration = Duration::from_millis(250);

/// What the real port asks the glyph helper through.
pub struct Tools {
    pub painter: Painter,
    pub filterer: Filterer,
    pub measurer: Measurer,
    pub detailer: Detailer,
    pub summarizer: Summarizer,
    /// Taken by the one sources query a run makes.
    pub finder: Option<Finder>,
    pub describer: Describer,
}

/// The terminal on standard input and output.
pub struct Tty {
    tools: Tools,
    /// Where the source finder's thread sends its answers, which the loop
    /// reads as [`Wake::Sourced`].
    sourced: Sender<Vec<(String, super::model::SourcesFound)>>,
    /// Where the describing thread sends its answers, which the loop reads
    /// as [`Wake::Described`].
    described: Sender<Vec<(PathBuf, crate::discover::Described)>>,
    /// Where each preview's thread sends what it drew, which the loop reads
    /// as [`Wake::Drawn`].
    drawn: Sender<Drawn>,
    input: OwnedFd,
    raw: Termios,
    saved: Termios,
    events: Loop<Woken>,
    deadline: Option<Instant>,
    /// vtium-platform's watch on udev's device events, which opens no
    /// device, between waits; `None` when there is none. At each change
    /// the desk reads the devices present again (ADR-1403 D7).
    udev: Option<Watch>,
    /// The last cell and the last unfiltered picture the preview's threads
    /// made, which the next preview reuses.
    kept: std::sync::Arc<Mutex<Kept>>,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the TUI's waits are bounded in time (ADR-1201 D4), and the terminal's port is where time enters"
)]
fn now() -> Instant {
    Instant::now()
}

impl Tty {
    /// Whether the loop watches udev's device events.
    pub fn watches_hotplug(&self) -> bool {
        self.udev.is_some()
    }

    /// Takes the terminal: raw mode, the alternate screen, the signals
    /// held.
    pub fn open(tools: Tools) -> io::Result<Tty> {
        let input = rustix::io::fcntl_dupfd_cloexec(std::io::stdin().as_fd(), 0)?;
        let saved = tcgetattr(&input)?;
        let mut raw = saved.clone();
        raw.make_raw();
        let mut events = Loop::new()?;
        for (signal, n) in [
            (Signal::SIGTERM, 15),
            (Signal::SIGINT, 2),
            (Signal::SIGQUIT, 3),
            (Signal::SIGHUP, 1),
        ] {
            events.on_signal(signal, move |w: &mut Woken| w.wakes.push(Wake::Signal(n)))?;
        }
        events.on_signal(Signal::SIGWINCH, |w: &mut Woken| w.wakes.push(Wake::Resize))?;
        // held so the TUI can take the terminal's foreground back after
        // the privileged step without being stopped
        events.on_signal(Signal::SIGTTOU, |_: &mut Woken| {})?;
        let source = Generic::new(
            rustix::io::fcntl_dupfd_cloexec(&input, 0)?,
            Interest::READ,
            Mode::Level,
        );
        events
            .handle()
            .insert_source(source, |_, fd, w: &mut Woken| {
                let mut buf = [0u8; 4096];
                match rustix::io::read(fd.as_fd(), &mut buf) {
                    Ok(0) => {
                        // the terminal is gone
                        w.wakes.push(Wake::Signal(1));
                        Ok(PostAction::Remove)
                    }
                    Ok(n) => {
                        w.wakes
                            .push(Wake::Bytes(buf.get(..n).unwrap_or_default().to_vec()));
                        Ok(PostAction::Continue)
                    }
                    Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => {
                        Ok(PostAction::Continue)
                    }
                    Err(e) => Err(e.into()),
                }
            })
            .map_err(|e| io::Error::other(e.to_string()))?;
        let (sourced, answers) = channel();
        events
            .handle()
            .insert_source(answers, |sent, _, w: &mut Woken| {
                if let Sent::Msg(found) = sent {
                    w.wakes.push(Wake::Sourced(found));
                }
            })
            .map_err(|e| io::Error::other(e.to_string()))?;
        let (described, descriptions) = channel();
        events
            .handle()
            .insert_source(descriptions, |sent, _, w: &mut Woken| {
                if let Sent::Msg(answers) = sent {
                    w.wakes.push(Wake::Described(answers));
                }
            })
            .map_err(|e| io::Error::other(e.to_string()))?;
        let (drawn, previews) = channel();
        events
            .handle()
            .insert_source(previews, |sent, _, w: &mut Woken| {
                if let Sent::Msg(d) = sent {
                    w.wakes.push(Wake::Drawn(d));
                }
            })
            .map_err(|e| io::Error::other(e.to_string()))?;
        let udev = Watch::new().ok();
        if let Some(watch) = &udev {
            let fd = watch.as_fd().try_clone_to_owned()?;
            events
                .handle()
                .insert_source(
                    Generic::new(fd, Interest::READ, Mode::Level),
                    |_, _, w: &mut Woken| {
                        if let Some(watch) = w.udev.as_mut()
                            && !watch.changes().is_empty()
                        {
                            w.wakes.push(Wake::Hotplug);
                        }
                        Ok(PostAction::Continue)
                    },
                )
                .map_err(|e| io::Error::other(e.to_string()))?;
        }
        tcsetattr(&input, OptionalActions::Now, &raw)?;
        if let Ok(mut r) = RESTORE.lock() {
            *r = Some(Restore {
                saved: saved.clone(),
                tty: rustix::io::fcntl_dupfd_cloexec(&input, 0)?,
                bytes: Vec::new(),
            });
        }
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));
        let mut tty = Tty {
            tools,
            sourced,
            described,
            drawn,
            input,
            raw,
            saved,
            events,
            deadline: None,
            udev,
            kept: std::sync::Arc::default(),
        };
        tty.write(ENTER)?;
        Ok(tty)
    }

    /// The terminal as it was, for the privileged step; and raw again.
    fn cooked(&mut self, on: bool) -> io::Result<()> {
        if on {
            self.write(LEAVE)?;
            tcsetattr(&self.input, OptionalActions::Now, &self.saved)?;
        } else {
            tcsetattr(&self.input, OptionalActions::Now, &self.raw)?;
            self.write(ENTER)?;
        }
        Ok(())
    }
}

/// The word that makes vtium-config release the signals the TUI holds and
/// run the command after it in its place.
pub const UNBLOCK_ARG: &str = "--run-with-signals";

/// The signals the TUI holds.
const HELD: [Signal; 6] = [
    Signal::SIGTERM,
    Signal::SIGINT,
    Signal::SIGQUIT,
    Signal::SIGHUP,
    Signal::SIGWINCH,
    Signal::SIGTTOU,
];

/// [`UNBLOCK_ARG`]'s side: releases the signals a process inherits held
/// from the TUI, then replaces itself with `argv`. A child inherits the
/// signals its parent holds, and the TUI holds them in its own process so
/// that none can end it between a release and a spawn; this step releases
/// them in the child alone. Returns only when `argv` could not run.
pub fn run_with_signals(argv: &[OsString]) -> u8 {
    use std::os::unix::process::CommandExt;
    // dropping a signal source releases the signals it holds
    drop(vtium_platform::calloop::signals::Signals::new(&HELD));
    let Some((program, args)) = argv.split_first() else {
        return 2;
    };
    let why = std::process::Command::new(program).args(args).exec();
    eprintln!(
        "vtium-config: cannot run {}: {why}",
        program.to_string_lossy()
    );
    127
}

/// Runs `argv` with the signals the TUI holds released, in a process
/// group of its own in the terminal's foreground, so the keys that
/// interrupt it reach it and not the TUI, and waits for it: whether it
/// succeeded, and what it printed, which the TUI shows once its screen is
/// back. sudo asks for the password on the terminal itself.
fn spawn_foreground(argv: &[OsString]) -> io::Result<(bool, String)> {
    use std::os::unix::process::CommandExt;
    if argv.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no command to run",
        ));
    }
    let stdin = std::io::stdin();
    let controlling =
        rustix::termios::tcgetpgrp(stdin.as_fd()).is_ok_and(|fg| fg == rustix::process::getpgrp());
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command.arg(UNBLOCK_ARG).args(argv);
    if controlling {
        command.process_group(0);
    }
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = command.spawn()?;
    if controlling && let Some(pid) = rustix::process::Pid::from_raw(child.id().cast_signed()) {
        let _ = rustix::termios::tcsetpgrp(stdin.as_fd(), pid);
        // a child that read the terminal before it held the foreground was
        // stopped; it goes on
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::CONT);
    }
    let out = child.wait_with_output()?;
    let mut said = String::from_utf8_lossy(&out.stdout).into_owned();
    said.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok((out.status.success(), said))
}

/// Standard output, the terminal while the TUI has given it back,
/// counting the bytes written to it.
struct Counted<'a> {
    written: &'a std::cell::Cell<usize>,
}

impl Write for Counted<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = std::io::stdout().lock().write(buf)?;
        self.written.set(self.written.get().saturating_add(n));
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        std::io::stdout().lock().flush()
    }
}

impl Port for Tty {
    fn size(&mut self) -> (usize, usize) {
        rustix::termios::tcgetwinsize(std::io::stdout().as_fd())
            .map(|w| (usize::from(w.ws_col), usize::from(w.ws_row)))
            .ok()
            .filter(|(c, r)| *c > 0 && *r > 0)
            .unwrap_or((80, 24))
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut out = std::io::stdout().lock();
        out.write_all(bytes)?;
        out.flush()
    }

    fn arm(&mut self, after: Duration) {
        self.deadline = now().checked_add(after);
    }

    fn wait(&mut self) -> io::Result<Vec<Wake>> {
        let mut woken = Woken {
            wakes: Vec::new(),
            udev: self.udev.take(),
        };
        let result = loop {
            if !woken.wakes.is_empty() {
                break Ok(());
            }
            let timeout = self.deadline.map(|d| d.saturating_duration_since(now()));
            if timeout == Some(Duration::ZERO) {
                self.deadline = None;
                woken.wakes.push(Wake::Timeout);
                break Ok(());
            }
            if let Err(e) = self.events.dispatch(timeout, &mut woken) {
                break Err(e);
            }
        };
        self.udev = woken.udev.take();
        result?;
        Ok(woken.wakes)
    }

    fn privileged(&mut self, step: Step<'_>) -> Result<Done, String> {
        // an install can write font sets, so a kept cell or picture of a
        // font set by its name may no longer be what the font set draws
        if let Ok(mut kept) = self.kept.lock() {
            *kept = Kept::default();
        }
        self.cooked(true).map_err(|e| e.to_string())?;
        let mut run = |argv: &[OsString]| -> io::Result<(bool, String)> {
            let result = spawn_foreground(argv);
            // the TUI takes the foreground back; SIGTTOU is held
            let stdin = std::io::stdin();
            let _ = rustix::termios::tcsetpgrp(stdin.as_fd(), rustix::process::getpgrp());
            result
        };
        // what the step writes after its last question waits for Enter,
        // so the person reads it before the TUI's screen comes back
        let written = std::cell::Cell::new(0usize);
        let answered = std::cell::Cell::new(0usize);
        let mut terminal = Counted { written: &written };
        let mut ask = |out: &mut dyn Write, question: &str| -> bool {
            let yes = crate::vts::ask_line(out, question);
            answered.set(written.get());
            yes
        };
        let result = step(&mut run, &mut terminal, &mut ask);
        if written.get() > answered.get() {
            let _ = writeln!(terminal, "{}", crate::vts::PRESS_ENTER);
            let mut line = String::new();
            let _ = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
        }
        self.cooked(false).map_err(|e| e.to_string())?;
        result
    }

    fn cell_px(&mut self) -> Option<(u32, u32)> {
        let w = rustix::termios::tcgetwinsize(std::io::stdout().as_fd()).ok()?;
        let cell = (
            u32::from(w.ws_xpixel).checked_div(u32::from(w.ws_col))?,
            u32::from(w.ws_ypixel).checked_div(u32::from(w.ws_row))?,
        );
        (cell.0 > 0 && cell.1 > 0).then_some(cell)
    }

    fn draw(&mut self, ask: Draw) {
        let tools = (
            std::sync::Arc::clone(&self.tools.measurer),
            std::sync::Arc::clone(&self.tools.painter),
            std::sync::Arc::clone(&self.tools.summarizer),
            std::sync::Arc::clone(&self.tools.detailer),
            std::sync::Arc::clone(&self.tools.filterer),
        );
        let drawn = self.drawn.clone();
        let kept = std::sync::Arc::clone(&self.kept);
        std::thread::spawn(move || {
            let (measure, paint, summarize, detail, filter) = tools;
            let answer = match ask {
                Draw::Cell { font, height } => {
                    let cell = measured(&font, height, &measure, &kept);
                    Drawn::Celled { font, height, cell }
                }
                Draw::Picture(shot) => painted(&shot, &paint, &filter, &kept),
                Draw::Specimen {
                    font,
                    height,
                    max,
                    display,
                } => {
                    let result = detail(&font, height, max, display);
                    Drawn::Detailed {
                        font,
                        height,
                        result,
                    }
                }
                Draw::Summary { font, height } => {
                    let summary = summarize(&font, height);
                    Drawn::Summarized {
                        font,
                        height,
                        summary,
                    }
                }
            };
            let _ = drawn.send(answer);
        });
    }

    fn sources(&mut self, fonts: Vec<String>) {
        if let Some(finder) = self.tools.finder.take() {
            let sourced = self.sourced.clone();
            std::thread::spawn(move || {
                let _ = sourced.send(finder(fonts));
            });
        }
    }

    /// Describes `paths` on a thread of its own. The answers go to the loop
    /// `DESCRIBED_AT_ONCE` at a time, or sooner once `DESCRIBED_WAIT` has
    /// passed since the last went, so the list fills in steps without a
    /// redraw for each file.
    fn describe(&mut self, paths: Vec<PathBuf>) {
        let describer = std::sync::Arc::clone(&self.tools.describer);
        let described = self.described.clone();
        std::thread::spawn(move || {
            let mut names = paths.iter();
            let mut batch = Vec::new();
            let mut sent = now();
            describer(&paths, &mut |answer| {
                if let Some(path) = names.next() {
                    batch.push((path.clone(), answer));
                }
                if batch.len() >= DESCRIBED_AT_ONCE || now().duration_since(sent) >= DESCRIBED_WAIT
                {
                    let _ = described.send(std::mem::take(&mut batch));
                    sent = now();
                }
            });
            if !batch.is_empty() {
                let _ = described.send(batch);
            }
        });
    }

    fn on_panic(&mut self, bytes: Vec<u8>) {
        if let Ok(mut r) = RESTORE.lock()
            && let Some(r) = r.as_mut()
        {
            r.bytes = bytes;
        }
    }
}

impl Drop for Tty {
    /// Puts the terminal back, giving the console its start font first
    /// where the loop ended before the font came back.
    fn drop(&mut self) {
        restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever ends the TUI, leaving the terminal turns mouse reporting
    /// off, which the Flow view may have left on: a panic's restore and a
    /// drop write these bytes, not the model's.
    #[test]
    fn leaving_the_terminal_turns_mouse_reporting_off_and_shows_the_screen() {
        let off = super::super::model::MOUSE_OFF;
        assert!(LEAVE.windows(off.len()).any(|w| w == off), "{LEAVE:?}");
        assert!(LEAVE.ends_with(b"\x1b[?25h\x1b[?1049l"), "{LEAVE:?}");
    }
}
