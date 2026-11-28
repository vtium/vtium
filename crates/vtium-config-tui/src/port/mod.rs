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
//! References:
//! - ADR-1206 D2: the loop turns every wake into a message, and performs
//!   the effects.
//! - ADR-1206 D5: ratatui draws what changed, and the frame after a step
//!   aside is drawn whole.
//! - ADR-1403 D11, "The mouse": reporting on, modes 1000 and 1006, on
//!   every screen, off before the person's editor or sudo and on leaving.

pub mod tty;
pub mod wake;

use std::io;
use std::time::Duration;

use ratatui_core::terminal::Terminal;
use vtium_config_core::ui::Effect;
use vtium_config_widgets::backend::Vt;
use vtium_config_widgets::keys::{self, Decoder};

use crate::app::{App, Event, Size};
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

/// Runs `app` on `port` until it leaves.
pub fn drive(port: &mut dyn Port, mut app: App) -> io::Result<Left> {
    let mut painter = Painter::new(app.color())?;
    let mut decoder = Decoder::new();
    let mut queue = app.event(Event::Resize(port.size()));
    loop {
        for effect in std::mem::take(&mut queue) {
            match effect {
                Effect::Write(bytes) => port.write(&bytes)?,
                Effect::Wait(after) => port.arm(after),
                Effect::Exit(status) => {
                    return Ok(Left {
                        status,
                        line: app.farewell(),
                    });
                }
                // Save's privileged step (rebuild-save), the previews and
                // the font helper's answers (the tab cards and
                // rebuild-shell-live) and the desk's ops (rebuild-save and
                // rebuild-shell-live) are performed by the cards that
                // build them
                Effect::Install(_)
                | Effect::InstallFont
                | Effect::Specimen { .. }
                | Effect::Cell { .. }
                | Effect::Picture(_)
                | Effect::ScreenPicture(_)
                | Effect::Summary { .. }
                | Effect::Sources(_)
                | Effect::Describe(_)
                | Effect::Desk(_) => {}
            }
        }
        port.write(&painter.frame(&app)?)?;
        for wake in port.wait()? {
            match wake {
                Wake::Bytes(bytes) => decoder.push(&bytes),
                Wake::Resize => queue.extend(app.event(Event::Resize(port.size()))),
                Wake::Signal(n) => queue.extend(app.event(Event::Signal(n))),
                Wake::Timeout => queue.extend(app.event(Event::Timeout)),
            }
            // each input is decoded while the App says whether a reply is
            // due, so a lone ESC after the last reply is the Esc key
            while let Some(input) = decoder.next(app.awaiting()) {
                let event = match input {
                    keys::Event::Key(key) => Event::Key(key),
                    keys::Event::Mouse(mouse) => Event::Mouse(mouse),
                    keys::Event::Reply(reply) => Event::Reply(reply),
                };
                queue.extend(app.event(event));
            }
        }
    }
}
