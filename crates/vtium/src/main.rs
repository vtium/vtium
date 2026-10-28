//! vtium: a terminal for the Linux virtual console.
//!
//! Start-up runs in the C reference's order (`src/main.c:161-222` at
//! kmscon b1f7a7c5): the configuration, the loop and its signals, the VT,
//! the seat's devices, the session, then the loop until something asks it
//! to end; at the end the VT switches back, the loop is served while that
//! is pending, and the VT is put back before the loop goes.

use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Instant;

use vtium::app::{App, Exit, SeatVt};
use vtium::check;
use vtium::config::{self, KEYS, Kind, Scope, keys};
use vtium::start::Start;
use vtium::term::Clock;
use vtium_platform::event_loop::{Loop, Signal};
use vtium_platform::input::locale;
use vtium_platform::udev::Udev;
use vtium_platform::vt::{FakeVt, KernelVt, Switch, Tty};

fn help() -> String {
    let mut out = String::from(
        "Usage: vtium [OPTION]... [--login -- COMMAND...]\n       vtium check-config [OPTION]...   check the configuration, apply nothing\n\n",
    );
    for k in &KEYS {
        let arg = match k.kind {
            Kind::Switch if k.scope == Scope::CommandLine => String::new(),
            Kind::Switch => format!(", --no-{}", k.name),
            Kind::Command => " -- COMMAND...".to_owned(),
            _ => "=VALUE".to_owned(),
        };
        let short = k.short.map(|c| format!("-{c}, ")).unwrap_or_default();
        out.push_str(&format!("  {short}--{}{arg}\n      {}\n", k.name, k.what));
    }
    out
}

#[expect(
    clippy::disallowed_methods,
    reason = "the binary's loop reads the clock and passes it in (docs/architecture.md, Determinism)"
)]
fn clock() -> Clock {
    Rc::new(Instant::now)
}

fn main() -> ExitCode {
    vtium::logger::init(log::Level::Info);
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if let Some(status) = vtium::blank::subcommand(&args) {
        return ExitCode::from(u8::try_from(status).unwrap_or(1));
    }
    let loaded = config::load(&args, Some("seat0"), &mut config::read_file);
    if loaded.check {
        let (lines, status) = check::run(&loaded);
        for line in lines {
            eprintln!("{line}");
        }
        return ExitCode::from(u8::try_from(status).unwrap_or(1));
    }
    for w in check::warnings(&loaded) {
        vtium::warn::warn(w);
    }
    if loaded.refuses_start() {
        log::error!("the command line cannot start vtium; see --help");
        return ExitCode::FAILURE;
    }
    let config = loaded.config;
    if config.switch(&keys::HELP) {
        print!("{}", help());
        return ExitCode::SUCCESS;
    }
    if config.switch(&keys::VERSION) {
        println!("vtium {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    log::info!(target: "log", "{}", vtium::logger::revision());
    let start = Start::new(&config, locale());
    let path = match start.vt.clone() {
        Some(vt) => Some(vt),
        None => match Tty::find() {
            Ok(path) => path,
            Err(e) => {
                log::error!("cannot find a VT: {e}");
                return ExitCode::FAILURE;
            }
        },
    };
    let status = match path {
        Some(path) => run_kernel(config, start, &path),
        None => {
            log::info!(target: "vt", "no VTs: using a fake VT");
            run(config, start, Box::new(|_| Ok(Box::new(FakeVt::new()))))
        }
    };
    match status {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(e) => {
            log::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run_kernel(config: config::Config, start: Start, path: &Path) -> io::Result<i32> {
    let (tty, num) = Tty::open(path)?;
    run(
        config,
        start,
        Box::new(move |lp| {
            let vt = KernelVt::new(lp, tty, num, |app: &mut App| app.vt.get::<KernelVt<Tty>>())?;
            let restore = vt.restorer()?;
            vtium::hook::restore_first(move || restore.run());
            Ok(Box::new(vt))
        }),
    )
}

/// How the seat's VT is opened, in the loop it runs in.
type Open = Box<dyn FnOnce(&mut Loop<App>) -> io::Result<Box<dyn SeatVt>>>;

/// Runs the seat on the VT `open` gives, until it ends; returns the exit
/// status. It is compiled once, whichever VT it runs on.
fn run(config: config::Config, start: Start, open: Open) -> io::Result<i32> {
    let mut lp: Loop<App> = Loop::new()?;
    for signal in [Signal::SIGTERM, Signal::SIGINT] {
        lp.on_signal(signal, |app: &mut App| app.request_exit(Exit::Signal))?;
    }
    // A write to a child gone returns an error in place of ending vtium.
    lp.on_signal(Signal::SIGPIPE, |_| {})?;
    vtium::blank::listen(&lp.handle())?;
    let vt = open(&mut lp)?;
    let clock = clock();
    let mut app = App::new(
        config,
        vt,
        lp.handle(),
        clock.clone(),
        std::env::vars_os().collect(),
    );

    match start.input() {
        Some(mut input) => {
            input.insert(&lp.handle(), |app: &mut App| app.input.as_mut());
            app.input = Some(input);
        }
        None => log::error!("no keymap: keys are not read"),
    }
    match Udev::new(app.vt.seat()) {
        Ok(mut udev) => {
            if let Err(e) = udev.scan() {
                log::warn!("cannot scan the seat's devices: {e}");
            }
            udev.insert(&lp.handle(), |app: &mut App| app.udev.as_mut())?;
            app.udev = Some(udev);
        }
        Err(e) => log::warn!("cannot watch the seat's devices: {e}"),
    }

    let started = if start.switch {
        app.vt.activate()
    } else {
        app.vt.wait()
    };
    if let Err(e) = started.as_ref() {
        log::error!("cannot switch to the VT: {e}");
    }
    if matches!(started, Ok(Switch::Done)) && app.vt.is_active() {
        log::debug!("the VT is shown");
    }
    app.start();
    app.turn(clock());

    let exit = loop {
        let wait = app.next_wake(clock());
        lp.dispatch(wait, &mut app)?;
        app.turn(clock());
        if let Some(exit) = app.finished() {
            break exit;
        }
    };
    // The VT is put back before the loop's signalfd goes.
    drop(app);
    drop(lp);
    Ok(exit.status())
}
