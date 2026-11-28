//! Playing a macro from the command line: on the screen, or headless.
//!
//! [`visible`] plays a macro on a terminal, at the pace of [`super::PACE`],
//! and leaves the TUI open where the macro stopped or ended, for the
//! person to carry on by hand. [`headless`] plays it with no terminal: a
//! screen of 100×30 cells, a terminal that answers no query and so is no
//! vtium console, no pause and no `Sleep`, and each step's line and its
//! result on standard output. Both play on a scratch copy unless the run
//! is live.
//!
//! References:
//! - ADR-1208 D4: visible play, `--macro -` on the controlling terminal,
//!   playing with no terminal, and the exit status.
//! - ADR-1208 D5: the scratch copy unless the run is live.

use std::io;
use std::path::Path;
use std::time::Duration;

use vtium_config_core::ui::Shared;

use super::{Player, Refusal, scratch};
use crate::BoxTab;
use crate::app::{App, Size};
use crate::live::machine::Machine;
use crate::port::{self, Port, Wake};

/// The screen of headless play: the use cases' default.
pub const SIZE: Size = Size::grid(100, 30);

/// The port of headless play: a screen of [`SIZE`] that shows nothing,
/// answers no query, and whose waits end at once.
#[derive(Debug)]
pub struct Headless {
    size: Size,
    armed: bool,
}

impl Default for Headless {
    fn default() -> Headless {
        Headless::at(SIZE)
    }
}

impl Headless {
    /// The same port with a screen of `size`, as a test plays a macro at
    /// the minimum, 80×25.
    pub fn at(size: Size) -> Headless {
        Headless { size, armed: false }
    }
}

impl Port for Headless {
    fn size(&mut self) -> Size {
        self.size
    }

    fn write(&mut self, _bytes: &[u8]) -> io::Result<()> {
        Ok(())
    }

    fn arm(&mut self, _after: Duration) {
        self.armed = true;
    }

    /// A wait the update asked for ends at once. With none, the terminal
    /// is gone, which no headless run waits on, since the player leaves
    /// when its steps end.
    fn wait(&mut self) -> io::Result<Vec<Wake>> {
        if std::mem::take(&mut self.armed) {
            Ok(vec![Wake::Timeout])
        } else {
            Ok(vec![Wake::Signal(1)])
        }
    }

    fn step_aside(&mut self, aside: &mut dyn FnMut() -> io::Result<()>) -> io::Result<()> {
        aside()
    }
}

/// How a run ended: its exit status, and the line the TUI prints on the
/// shell after it leaves, where it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Played {
    pub status: u8,
    pub line: Option<String>,
}

/// Plays `player`'s macro with no terminal, on `tabs` and `shared`, with
/// `machine` for what the loop reads and does. A run that is not live
/// plays on a scratch copy in a folder under `scratch_in`, which goes
/// when the run ends.
pub fn headless(
    tabs: Vec<BoxTab>,
    shared: Shared,
    machine: &mut dyn Machine,
    player: &mut Player,
    scratch_in: &Path,
) -> io::Result<Played> {
    let mut port = Headless::default();
    play(&mut port, tabs, shared, false, machine, player, scratch_in)
}

/// Plays `player`'s macro on `port`, a terminal, as [`headless`] plays it
/// with none. The TUI stays open where the macro stopped or ended.
pub fn visible(
    port: &mut dyn Port,
    tabs: Vec<BoxTab>,
    shared: Shared,
    color: bool,
    machine: &mut dyn Machine,
    player: &mut Player,
    scratch_in: &Path,
) -> io::Result<Played> {
    play(port, tabs, shared, color, machine, player, scratch_in)
}

fn play(
    port: &mut dyn Port,
    tabs: Vec<BoxTab>,
    mut shared: Shared,
    color: bool,
    machine: &mut dyn Machine,
    player: &mut Player,
    scratch_in: &Path,
) -> io::Result<Played> {
    let size = port.size();
    if player.live() {
        let mut app = App::new(tabs, shared, size, color);
        if let Err(r) = player.start(&mut app) {
            return Ok(refused(player, r));
        }
        let left = port::drive_with(port, machine, app, Some(player))?;
        return Ok(Played {
            status: left.status,
            line: left.line,
        });
    }
    let folder = match scratch::copy(&mut shared, scratch_in) {
        Ok(f) => f,
        Err(why) => {
            let r = player.refuse(why, 1);
            return Ok(refused(player, r));
        }
    };
    let installed = shared.document().installed().clone();
    let mut on_copy = scratch::Scratch::new(machine, &folder, installed);
    let mut app = App::new(tabs, shared, size, color);
    if let Err(r) = player.start(&mut app) {
        return Ok(refused(player, r));
    }
    let left = port::drive_with(port, &mut on_copy, app, Some(player))?;
    Ok(Played {
        status: left.status,
        line: left.line,
    })
}

/// A run refused before its first step: its line goes where the run
/// writes each step's result, and the run ends with its status. The
/// refusal stays in [`Player::outcome`] for the binary to print.
fn refused(player: &mut Player, r: Refusal) -> Played {
    player.say(&r.why);
    Played {
        status: r.status,
        line: None,
    }
}
