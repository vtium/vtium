//! The step that saves, which the loop runs while the TUI steps aside, so
//! sudo asks for the password on the terminal. It runs the same steps as
//! `vtium config install`: the check, the install bundle built from the
//! draft and its base, `sudo vtium-config apply DIR`, the history of the
//! file it replaces, and then, for the system's vtium.conf, the step that
//! enables vtium on the VTs its heads name and asks whether to start and
//! restart it there. It names the command before sudo asks.
//!
//! [`installed`] reads what is installed now, for the review. [`step`] and
//! [`read`] take a [`Session`], so a test runs them over a machine in a
//! folder of its own, with a runner of its own in place of sudo.
//!
//! References:
//! - ADR-1403 D8, "Save: the review, then sudo once": Save saves through
//!   the steps of `vtium config install`, and sudo asks once.
//! - ADR-1201 D5: `vtium-config apply DIR` through sudo, the root side's
//!   check of the file again before it writes it, and the step after it
//!   on the VTs.
//! - ADR-1206 D2: the loop, not update, runs the privileged step.

use std::ffi::OsString;
use std::io::Write;

use vtium_config_core::apply::{Ask, Runner};
use vtium_config_core::document::{Installed, OnDisk};
use vtium_config_core::draft::Plan;
use vtium_config_core::drafts::{Over, Session, Unrouted, run_child};
use vtium_config_core::places::Paths;
use vtium_config_core::store::Store;
use vtium_config_core::vts;

use super::Saved;

/// Where Save writes, as the TUI names it.
pub const TARGET: &str = "/etc/vtium/vtium.conf";

/// What is installed now on this machine: vtium.conf and the font sets.
pub fn installed(paths: &Paths) -> Result<Installed, String> {
    let store = Store::system().map_err(|f| f.text().to_owned())?;
    read(&Session { store, paths })
}

/// What is installed now under `session`'s paths.
pub fn read(session: &Session<'_>) -> Result<Installed, String> {
    OnDisk::open(session)
        .map(|d| d.installed().clone())
        .map_err(|f| f.text().to_owned())
}

/// Saves the configuration `name` on this machine, on the terminal the
/// loop handed back: sudo asks for the password there, and so does the
/// question whether to restart vtium. What the step prints shows on the
/// terminal and comes back in [`Saved::said`]. Where something follows
/// the last question, the step waits for Enter, so the person reads it
/// before the TUI's screen comes back.
pub fn save(paths: &Paths, name: &str, plan: &Plan) -> Result<Saved, String> {
    let store = Store::system().map_err(|f| f.text().to_owned())?;
    let session = Session { store, paths };
    let mut out = Tee::default();
    let answered = std::cell::Cell::new(0usize);
    let mut run = |argv: &[OsString]| run_child(argv);
    let mut ask = |out: &mut dyn Write, question: &str| -> bool {
        let yes = vts::ask_line(out, question);
        answered.set(usize::MAX);
        yes
    };
    let done = step(&session, name, plan, &mut run, &mut ask, &mut out);
    if answered.get() == 0 || done.is_err() {
        let _ = writeln!(out, "{}", vts::PRESS_ENTER);
        let mut line = String::new();
        let _ = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
    }
    let said = String::from_utf8_lossy(&out.kept).into_owned();
    match done {
        Ok(installed) => Ok(Saved { installed, said }),
        Err(why) => Err(format!("{why}\n{}", said.trim_end())),
    }
}

/// Saves the draft `name` through `session`: names the save and its
/// first command, then installs the draft as `vtium config install`
/// does, through `run` and `ask`, writing what it does to `out`. A yes to
/// the question for a file with no complete route is `plan.no_route`.
/// Returns what is installed after the save.
pub fn step(
    session: &Session<'_>,
    name: &str,
    plan: &Plan,
    run: Runner<'_>,
    ask: Ask<'_>,
    out: &mut dyn Write,
) -> Result<Installed, String> {
    let text = session
        .store
        .read_draft(name)
        .map_err(|f| f.text().to_owned())?
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "vtium-config is saving {name} to {}. It runs:",
        session.installed_path().display()
    );
    // the first command is the privileged step, which the person sees
    // before sudo asks; the commands after it print themselves
    let mut first = true;
    let shown = std::cell::RefCell::new(Vec::new());
    let mut named = |argv: &[OsString]| {
        if first {
            first = false;
            let _ = writeln!(shown.borrow_mut(), "  {}", vts::shown(argv));
        }
        run(argv)
    };
    let unrouted = if plan.no_route {
        Unrouted::Install
    } else {
        Unrouted::Refuse
    };
    let mut buffered = Buffered { shown: &shown, out };
    session
        .install_over(
            name,
            &text,
            Over::Refuse,
            unrouted,
            &mut named,
            Some(ask),
            &mut buffered,
        )
        .map_err(|f| f.text().to_owned())?;
    read(session)
}

/// A writer that writes first the lines the runner kept, the command the
/// save runs, so it shows before what the command prints.
struct Buffered<'a> {
    shown: &'a std::cell::RefCell<Vec<u8>>,
    out: &'a mut dyn Write,
}

impl Write for Buffered<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let shown = std::mem::take(&mut *self.shown.borrow_mut());
        self.out.write_all(&shown)?;
        self.out.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

/// The terminal, with a copy of everything written to it.
#[derive(Default)]
struct Tee {
    kept: Vec<u8>,
}

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.kept.extend_from_slice(buf);
        let mut terminal = std::io::stdout().lock();
        terminal.write_all(buf)?;
        terminal.flush()?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::stdout().flush()
    }
}
