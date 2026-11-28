//! The loop around the App, and the terminal it reads and writes.
//!
//! [`drive`] is the one loop: it takes what the [`Port`] gives, decodes
//! the bytes into keys, clicks and replies, hands each to [`App::event`],
//! carries out the effects that come back, and draws the screen after each
//! turn, through ratatui, which writes only the cells that changed. The
//! decoder asks the App before each input whether a reply is due, which
//! decides what an ESC that ends a read is. A test drives the same loop
//! through a port of its own.
//!
//! [`tty::Tty`] is the real port: the terminal in raw mode on the
//! alternate screen, with mouse reporting on for every screen while the
//! TUI runs. Mouse reporting goes off when the TUI leaves, and while it
//! steps aside for an editor or sudo ([`Port::step_aside`]).
//!
//! The loop reads the machine for the Actions menu's items through
//! [`crate::live::machine::Machine`], and hands the App what it found as a
//! message. It runs the install of a system font while it steps aside, so
//! sudo can ask for the password on the terminal, and runs the person's
//! editor for Edit as text the same way.
//!
//! [`drive_with`] runs the same loop with a second source of events beside
//! the terminal, a [`Source`], which is the macro player. The loop hands
//! the source the TUI whenever it rests, and takes from it the next key.
//! A key or a click from the terminal stops the source, and the TUI does
//! not act on it. Before a real command, the loop asks the source, which
//! logs a live run's command or refuses to run it.
//!
//! References:
//! - ADR-1206 D2: the loop turns every wake into a message, and performs
//!   the effects.
//! - ADR-1208 D3: the player waits until the TUI rests before each step.
//! - ADR-1208 D10: the update loop takes the player's steps as a second
//!   source of events, and a key from the terminal stops the player.
//! - ADR-1208 D5: a live run logs each real command before it runs.
//! - ADR-1206 D5: ratatui draws what changed, and the frame after a step
//!   aside is drawn whole.
//! - ADR-1403 D11, "The mouse": reporting on, modes 1000 and 1006, on
//!   every screen, off before the person's editor or sudo and on leaving.

pub mod tty;
pub mod wake;

use std::io;
use std::time::Duration;

use ratatui_core::terminal::Terminal;
use vtium_config_core::ui::{Effect, Message, Op};
use vtium_config_widgets::backend::Vt;
use vtium_config_widgets::keys::{self, Decoder};

use crate::app::{App, Event, Size};
use crate::live::machine::{Machine, Nowhere};
use crate::save::Came;
use crate::view;
pub use wake::Wake;

/// Mouse reporting on: button presses (mode 1000) in SGR's form (1006).
pub const MOUSE_ON: &[u8] = b"\x1b[?1000h\x1b[?1006h";
/// Mouse reporting off.
pub const MOUSE_OFF: &[u8] = b"\x1b[?1006l\x1b[?1000l";
/// The alternate screen, the cursor hidden, and mouse reporting on.
pub const ENTER: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1006h";
/// Mouse reporting off, the cursor shown, and the normal screen back.
pub const LEAVE: &[u8] = b"\x1b[?1006l\x1b[?1000l\x1b[?25h\x1b[?1049l";

/// The terminal the loop runs against.
pub trait Port {
    /// The screen's size.
    fn size(&mut self) -> Size;
    /// Writes `bytes` whole.
    fn write(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Starts a wait of `after`, which ends in [`Wake::Timeout`].
    fn arm(&mut self, after: Duration);
    /// Waits for what comes next.
    fn wait(&mut self) -> io::Result<Vec<Wake>>;
    /// Gives the terminal back as it was before the TUI took it, with
    /// mouse reporting off and the normal screen shown, runs `aside`, such
    /// as the person's editor or sudo, then takes the terminal again. The
    /// next frame is drawn whole, since `aside` may have written anything.
    fn step_aside(&mut self, aside: &mut dyn FnMut() -> io::Result<()>) -> io::Result<()>;
    /// Starts the source's own wait of `after`, beside the update's, which
    /// ends in [`Wake::Step`]. Returns `false` for a port with no such
    /// wait, where the source plays its next step at once.
    fn arm_step(&mut self, after: Duration) -> bool {
        let _ = after;
        false
    }
}

/// A second source of events beside the terminal: the macro player.
pub trait Source {
    /// The TUI rests: every effect has its answer and no reply is due.
    /// Returns what the source does next.
    fn rested(&mut self, app: &App) -> Next;
    /// A key or a click came from the terminal. Returns the line for the
    /// message row where the source was playing and stops now, and the
    /// TUI then does not act on the key.
    fn stop(&mut self) -> Option<Message>;
    /// The loop is about to run the real command `command`. `Err` says
    /// why it must not run, and the loop does not run it.
    fn before_real(&mut self, command: &str) -> Result<(), String>;
    /// The TUI leaves with `status`. Returns the status the run exits
    /// with.
    fn left(&mut self, app: &App, status: u8) -> u8;
}

/// What a [`Source`] does when the TUI rests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    /// Hands the TUI this event.
    Event(Event),
    /// Waits this long, then is asked again.
    Pause(Duration),
    /// Has nothing to do: the loop waits for the terminal.
    Idle,
    /// The run is over, and the TUI leaves with this status.
    Leave(u8),
}

/// How the TUI left: its exit status, and the line it prints on the shell
/// once the terminal is back, where it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Left {
    pub status: u8,
    pub line: Option<String>,
}

/// Draws the App's frames through ratatui into bytes for the terminal.
pub struct Painter {
    terminal: Terminal<Vt>,
}

impl Painter {
    pub fn new(color: bool) -> io::Result<Painter> {
        Ok(Painter {
            terminal: Terminal::new(Vt::new(0, 0, color))?,
        })
    }

    /// The bytes that bring the terminal from the last frame to `app`'s.
    pub fn frame(&mut self, app: &App) -> io::Result<Vec<u8>> {
        let size = app.size();
        let backend = self.terminal.backend_mut();
        backend.set_size(size.cols, size.rows);
        backend.set_color(app.color());
        self.terminal.draw(|frame| view::draw(app, frame))?;
        Ok(self.terminal.backend_mut().take())
    }

    /// Forgets what the terminal shows, so the next frame is drawn whole.
    pub fn forget(&mut self) -> io::Result<Vec<u8>> {
        self.terminal.clear()?;
        Ok(self.terminal.backend_mut().take())
    }
}

/// Runs `app` on `port` until it leaves, on a loop with no machine to
/// read: Read devices again and Install as a system font… say that they
/// cannot act here.
pub fn drive(port: &mut dyn Port, app: App) -> io::Result<Left> {
    drive_on(port, &mut Nowhere, app)
}

/// Runs `app` on `port` until it leaves, reading and acting on `machine`
/// for the Actions menu's items.
pub fn drive_on(port: &mut dyn Port, machine: &mut dyn Machine, app: App) -> io::Result<Left> {
    drive_with(port, machine, app, None)
}

/// Runs `app` on `port` until it leaves, as [`drive_on`] does, with
/// `source` as a second source of events beside the terminal.
pub fn drive_with(
    port: &mut dyn Port,
    machine: &mut dyn Machine,
    mut app: App,
    mut source: Option<&mut dyn Source>,
) -> io::Result<Left> {
    let mut painter = Painter::new(app.color())?;
    // the command of the font readied last, which a live run logs
    let mut font_command = String::new();
    let mut decoder = Decoder::new();
    let mut queue = app.event(Event::Resize(port.size()));
    loop {
        // an effect the loop performs can hand the App an event whose
        // effects follow at once
        while !queue.is_empty() {
            for effect in std::mem::take(&mut queue) {
                match effect {
                    Effect::Write(bytes) => port.write(&bytes)?,
                    Effect::Wait(after) => port.arm(after),
                    Effect::Exit(status) => {
                        let status = match source.as_deref_mut() {
                            Some(s) => s.left(&app, status),
                            None => status,
                        };
                        return Ok(Left {
                            status,
                            line: app.farewell(),
                        });
                    }
                    Effect::Desk(Op::Devices) => {
                        let present = machine.devices();
                        queue.extend(app.event(Event::Devices(present)));
                    }
                    Effect::Desk(Op::SystemFont(path)) => {
                        let ready = machine.ready_font(&path);
                        if let Ok(r) = &ready {
                            font_command.clone_from(&r.command);
                        }
                        queue.extend(app.event(Event::FontReady(ready)));
                    }
                    // sudo asks for the password on the terminal, so the
                    // install runs while the TUI steps aside, and the next
                    // frame is drawn whole
                    Effect::InstallFont => {
                        if let Some(Err(why)) =
                            source.as_deref_mut().map(|s| s.before_real(&font_command))
                        {
                            queue.extend(app.event(Event::FontInstalled(Err(why))));
                            continue;
                        }
                        let mut done = Ok(());
                        port.step_aside(&mut || {
                            done = machine.install_font();
                            Ok(())
                        })?;
                        port.write(&painter.forget()?)?;
                        queue.extend(app.event(Event::FontInstalled(done)));
                    }
                    // Save reads the installed file again for its review
                    Effect::Desk(Op::Prepare) => {
                        let read = machine.installed();
                        queue.extend(app.event(Event::Save(Came::Read(read))));
                    }
                    // sudo asks for the password on the terminal, so Save's
                    // privileged step runs while the TUI steps aside, and
                    // the next frame is drawn whole
                    Effect::Install(plan) => {
                        let name = app.shared().document().name().to_owned();
                        let command = crate::player::words::save_command(&name);
                        if let Some(Err(why)) =
                            source.as_deref_mut().map(|s| s.before_real(&command))
                        {
                            queue.extend(app.event(Event::Save(Came::Saved(Err(why)))));
                            continue;
                        }
                        port.write(&painter.frame(&app)?)?;
                        let mut done = Err(String::new());
                        port.step_aside(&mut || {
                            done = machine.save(&name, &plan);
                            Ok(())
                        })?;
                        port.write(&painter.forget()?)?;
                        queue.extend(app.event(Event::Save(Came::Saved(done))));
                    }
                    // the person's editor takes the terminal, so it runs
                    // while the TUI steps aside, as the install does
                    Effect::Editor { name, text } => {
                        let mut done = Err(crate::editor::words::NO_EDITOR.to_owned());
                        port.step_aside(&mut || {
                            done = machine.edit(&name, &text);
                            Ok(())
                        })?;
                        port.write(&painter.forget()?)?;
                        queue.extend(app.event(Event::Edited(done)));
                    }
                    // the previews and the font helper's answers (the tab
                    // cards) and the desk's other ops are performed by the
                    // cards that build them
                    Effect::Specimen { .. }
                    | Effect::Cell { .. }
                    | Effect::Picture(_)
                    | Effect::ScreenPicture(_)
                    | Effect::Summary { .. }
                    | Effect::Sources(_)
                    | Effect::Describe(_)
                    | Effect::Desk(Op::Check | Op::Versions) => {}
                }
            }
        }
        port.write(&painter.frame(&app)?)?;
        // the source plays its next step once the TUI rests
        if let Some(s) = source.as_deref_mut()
            && !app.awaiting()
        {
            match s.rested(&app) {
                Next::Event(event) => {
                    queue.extend(app.event(event));
                    continue;
                }
                Next::Pause(after) => {
                    if !port.arm_step(after) {
                        continue;
                    }
                }
                Next::Idle => {}
                Next::Leave(status) => {
                    return Ok(Left {
                        status,
                        line: app.farewell(),
                    });
                }
            }
        }
        for wake in port.wait()? {
            match wake {
                Wake::Bytes(bytes) => decoder.push(&bytes),
                Wake::Resize => queue.extend(app.event(Event::Resize(port.size()))),
                Wake::Signal(n) => queue.extend(app.event(Event::Signal(n))),
                Wake::Timeout => queue.extend(app.event(Event::Timeout)),
                Wake::Step => {}
            }
            // each input is decoded while the App says whether a reply is
            // due, so a lone ESC after the last reply is the Esc key
            while let Some(input) = decoder.next(app.awaiting()) {
                let event = match input {
                    keys::Event::Key(key) => Event::Key(key),
                    keys::Event::Mouse(mouse) => Event::Mouse(mouse),
                    keys::Event::Reply(reply) => Event::Reply(reply),
                };
                // a key or a click from the terminal stops the source,
                // and the TUI does not act on it
                if matches!(event, Event::Key(_) | Event::Mouse(_))
                    && let Some(said) = source.as_deref_mut().and_then(|s| s.stop())
                {
                    queue.extend(app.event(Event::Said(said)));
                    continue;
                }
                queue.extend(app.event(event));
            }
        }
    }
}
