//! vtium: a terminal for the Linux virtual console.
//!
//! Start-up first reads the kernel command line: with `vtium.enable=0` on
//! it, vtium exits with status 1 before it reads anything else (ADR-1401
//! D9). Then it reads the configuration, vtium.conf's top level and its
//! sections, the flow (ADR-1401 D3, ADR-1403 D3), writes or removes the closed policy's getty marker (`getty.rs`), and
//! then opens the loop and its signals, the VT, the seat's devices and the
//! sessions, and runs the loop until something asks it to end; at the end
//! the VT switches back, the loop is served while that is pending, and the
//! VT is put back before the loop goes.
//!
//! vtium reads the flow before it opens the loop, its signals and the VT,
//! because the marker needs the file's policy before the VT opens, so
//! that a failure opening the loop or the VT under a closed policy still
//! leaves the marker.

use std::ffi::OsString;
use std::io;
use std::os::fd::OwnedFd;
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
use vtium_platform::vt::font::{ConsoleFont, ConsoleTty};
use vtium_platform::vt::{FakeVt, KernelVt, Switch, Tty};

fn help() -> String {
    let mut out = String::from(
        "Usage: vtium [OPTION]... [--login -- COMMAND...]\n       vtium check-config [OPTION]... [FILE]   check the configuration without applying it\n       vtium config [COMMAND] [ARG]...         run vtium-config, the font and settings tool\n       vtium blank --vt N                      turn off the displays of the vtium on VT N now\n       vtium wake --vt N                       turn on the displays of the vtium on VT N\n       vtium console enable N|--all            run vtium on VT N, or on every VT, from the next start\n       vtium console disable N|--all           stop running vtium on VT N, or on every VT\n       vtium console status                    show what runs on each VT\n       vtium console fallback N                start a login prompt on VT N after vtium failed there\n\n",
    );
    for k in &KEYS {
        let arg = match k.kind {
            Kind::Switch if k.scope == Scope::CommandLine => String::new(),
            Kind::Switch => format!(", --no-{}", k.name),
            Kind::Command => " -- COMMAND...".to_owned(),
            _ => "=VALUE".to_owned(),
        };
        let short = k.short.map(|c| format!("-{c}, ")).unwrap_or_default();
        out.push_str(&format!(
            "  {short}--{}{arg}\n      {}\n",
            k.name,
            k.what_line()
        ));
    }
    out
}

/// What `--version` prints: the package's version, as it always has on
/// its first line, then the build's revision and the features it was built
/// with on top of minspec, `none` where it has none, so that
/// `vtium-config report` can tell a build apart (ADR-1200 D24).
fn version() -> String {
    let on: Vec<&str> = vtium::video::FEATURES_ON_TOP
        .iter()
        .filter(|(_, on)| *on)
        .map(|(name, _)| *name)
        .collect();
    let features = if on.is_empty() {
        "none".to_owned()
    } else {
        on.join(" ")
    };
    format!(
        "vtium {}\nrevision {}\nfeatures {features}\n",
        env!("CARGO_PKG_VERSION"),
        vtium::logger::REVISION
    )
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
    if let Some(status) = vtium::config_tool::subcommand(&args)
        .or_else(|| vtium::blank::subcommand(&args))
        .or_else(|| vtium::console::subcommand(&args))
        .or_else(|| vtium::survive::hold::subcommand(&args))
    {
        return ExitCode::from(u8::try_from(status).unwrap_or(1));
    }
    // Before vtium.conf, the VT and any card (ADR-1401 D9).
    if let Some(status) = vtium::kernel::off_at_start(&args, Path::new(vtium::kernel::CMDLINE)) {
        return ExitCode::from(status);
    }
    let loaded = config::load(&args, &mut config::read_file);
    // The same reader, with its ownership check, reads the font keys again
    // at a session's end (ADR-1201 D11).
    let reread: vtium::app::Reread = {
        let args = args.clone();
        Box::new(move || config::load(&args, &mut config::read_file))
    };
    if loaded.check {
        // check-config reads a candidate whoever owns it, and judges its
        // ownership apart (ADR-1403 D3)
        return ExitCode::from(check::main(&config::load(&args, &mut config::read_bytes)));
    }
    for w in check::warnings(&loaded) {
        vtium::warn::warn(w);
    }
    if loaded.refuses_start() {
        log::error!("vtium cannot start with these options. See vtium --help.");
        return ExitCode::FAILURE;
    }
    let config = loaded.config;
    if config.switch(&keys::HELP) {
        print!("{}", help());
        return ExitCode::SUCCESS;
    }
    if config.switch(&keys::VERSION) {
        print!("{}", version());
        return ExitCode::SUCCESS;
    }
    log::info!(target: "log", "{}", vtium::logger::revision());
    let start = Start::new(&config, locale());
    log::set_max_level(start.log.to_level_filter());
    let path = match start.vt.clone() {
        Some(vt) => Some(vt),
        None => match Tty::find() {
            Ok(path) => path,
            Err(e) => {
                log::error!("cannot start: no VT found: {e}");
                return ExitCode::FAILURE;
            }
        },
    };
    // Before the loop and the VT, so a failure opening either under a
    // closed policy still leaves the marker that keeps the getty off.
    // Each vtium builds the heads of its own VT (ADR-1401 D2). Without a
    // kernel VT every head runs.
    let vt = path.as_deref().and_then(vtium::getty::vt_number);
    let heads = vtium::head::rules(&loaded.path, &loaded.file).on_vt(vt);
    vtium::getty::mark(
        Path::new(vtium::getty::DIR),
        path.as_deref(),
        heads.outcome.policy,
    );
    // Before the loop blocks any signal, since a holder started here
    // inherits the mask (ADR-1401 D13).
    let env: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    // Outside a logind session of its own, as in direct mode, vtium pins
    // libseat to seatd, so that no open of the seat reaches logind's
    // backend (ADR-1400 D3). It runs while vtium has one thread, and after
    // `env` is read, so no session inherits the variable.
    #[cfg(feature = "libseat")]
    vtium_platform::seat::keep_from_logind();
    let (keeper, kept) = vtium::survive::start(
        &env,
        path.as_deref(),
        Path::new(vtium::getty::DIR),
        Path::new("/proc/self/exe"),
        vtium_platform::take_passed,
    );
    let survival = (keeper, kept, env);
    let status = match path {
        Some(path) => run_kernel(config, reread, start, heads, survival, &path),
        None => run(
            config,
            reread,
            start,
            heads,
            survival,
            None,
            Box::new(|lp| {
                #[cfg(feature = "libseat")]
                if let Ok(vt) = seat(lp, None)? {
                    return Ok(vt);
                }
                #[cfg(not(feature = "libseat"))]
                let _ = lp;
                log::info!(target: "vt", "no VTs: using a fake VT");
                Ok(Box::new(FakeVt::new()))
            }),
        ),
    };
    let code = match status {
        Ok(code) => u8::try_from(code).unwrap_or(1),
        Err(e) => {
            log::error!("{e}");
            1
        }
    };
    leave(code)
}

/// Ends vtium with `code` once the seat has dropped. A render thread that
/// vtium went on without at a head's drop, because its GPU driver had not
/// let go of the head's GL context in time (ADR-1303 D9), may still be
/// inside the driver. The process's exit handlers would then tear libEGL
/// and Mesa down under it, which corrupts the heap or aborts, or hangs on
/// the driver's lock. So while such a thread runs, vtium ends at once
/// without running its exit handlers, after flushing its output.
fn leave(code: u8) -> ExitCode {
    #[cfg(feature = "gpu")]
    {
        let running = vtium::painter::wait_for_left(std::time::Duration::ZERO, &*clock());
        // No test reaches this branch. It needs the vtium binary to leave a
        // render thread at its exit, which takes a GPU driver that hangs
        // destroying a head's GL context, and only the in-process test GPU
        // can stand in for one; the binary has no such source. Its parts
        // are tested apart: tests/gpu.rs leaves threads through the app and
        // counts them with `wait_for_left`, and vtium-platform's
        // `exit_now_runs_no_exit_handler` shows that `exit_now` ends a
        // process with its status and runs no exit handler.
        if running > 0 {
            log::warn!(
                "{running} drawing thread(s) are still waiting on the GPU driver, so vtium exits at once, without its usual clean-up"
            );
            use std::io::Write;
            let _ = io::stdout().flush();
            log::logger().flush();
            vtium_platform::gpu::exit_now(code);
        }
    }
    ExitCode::from(code)
}

fn run_kernel(
    config: config::Config,
    reread: vtium::app::Reread,
    start: Start,
    heads: vtium::head::Start,
    survival: Survival,
    path: &Path,
) -> io::Result<i32> {
    let console = kernel_font(path);
    let path = path.to_owned();
    run(
        config,
        reread,
        start,
        heads,
        survival,
        console,
        // The VT opens once the loop blocks its signals: `vtium blank`
        // finds a vtium by its open VT, and its signal at the default
        // disposition would end vtium.
        Box::new(move |lp| {
            let (tty, num) = Tty::open(&path)?;
            #[cfg(feature = "libseat")]
            let Some((tty, num)) = (match seat(lp, Some((tty, num)))? {
                Ok(vt) => return Ok(vt),
                Err(tty) => tty,
            }) else {
                return Err(io::Error::other("the seat kept the VT it did not choose"));
            };
            let vt = KernelVt::new(lp, tty, num, |app: &mut App| app.vt.get::<KernelVt<Tty>>())?;
            let restore = vt.restorer()?;
            vtium::hook::restore_first(move || restore.run());
            Ok(Box::new(vt))
        }),
    )
}

/// A VT and its number, or none on a system without VTs.
#[cfg(feature = "libseat")]
type VtTty = Option<(Tty, u16)>;

/// With the `libseat` feature, the seat through its seat manager when one
/// answers, on the VT `tty` when the seat has VTs (ADR-1400's L2 layer);
/// otherwise `tty`, handed back for the kernel's VT or a fake one.
#[cfg(feature = "libseat")]
fn seat(lp: &mut Loop<App>, tty: VtTty) -> io::Result<Result<Box<dyn SeatVt>, VtTty>> {
    use vtium_platform::seat::{Chosen, LibseatVt};
    let chosen = LibseatVt::start(lp, tty, |app: &mut App| app.vt.get::<LibseatVt>())?;
    Ok(match chosen {
        Chosen::Seat(vt) => Ok(Box::new(vt)),
        Chosen::Kernel(tty) => Err(tty),
    })
}

/// The kernel's font on the VT at `path`, read once at start while the
/// VT is still in text mode, for any frit's `console:` source to draw
/// from, the one OSC 50 switches to included (ADR-1200 D8). A VT already
/// in graphics mode, as when vtium starts again on it, gives none, logged
/// at debug.
fn kernel_font(path: &Path) -> Option<ConsoleFont> {
    let read = ConsoleTty::open(path)
        .map_err(|e| e.to_string())
        .and_then(|tty| vtium_platform::vt::font::read(&tty).map_err(|e| e.to_string()));
    match read {
        Ok(font) => Some(font),
        Err(e) => {
            log::debug!("the kernel's font is not read from {}: {e}", path.display());
            None
        }
    }
}

/// How the seat's VT is opened, in the loop it runs in.
type Open = Box<dyn FnOnce(&mut Loop<App>) -> io::Result<Box<dyn SeatVt>>>;

/// Where each session's pty is kept through a restart, the ptys an earlier
/// vtium left (ADR-1401 D13), and vtium's environment.
type Survival = (
    vtium::survive::Keeper,
    Vec<(String, OwnedFd)>,
    Vec<(OsString, OsString)>,
);

/// Runs the seat on the VT `open` gives, with the head file `heads`
/// resolved to, until it ends; returns the exit status. It is compiled
/// once, whichever VT it runs on.
fn run(
    config: config::Config,
    reread: vtium::app::Reread,
    start: Start,
    heads: vtium::head::Start,
    survival: Survival,
    console: Option<ConsoleFont>,
    open: Open,
) -> io::Result<i32> {
    let (keeper, kept, env) = survival;
    let mut lp: Loop<App> = Loop::new()?;
    for signal in [Signal::SIGTERM, Signal::SIGINT] {
        lp.on_signal(signal, |app: &mut App| app.request_exit(Exit::Signal))?;
    }
    // A write to a child gone returns an error in place of ending vtium.
    lp.on_signal(Signal::SIGPIPE, |_| {})?;
    vtium::blank::listen(&lp.handle())?;
    // systemd starts the display manager only once vtium holds its VT
    // (ADR-1500 D7, "Ready before the display manager").
    let vt = vtium::survive::notify::opened(&env, || open(&mut lp))?;
    let clock = clock();
    let mut app = App::new(config, vt, lp.handle(), clock.clone(), env);
    if let Some(font) = console {
        app.set_console_font(font);
    }
    app.set_config_reader(reread);
    app.set_heads(heads);
    app.set_survival(keeper, kept);
    kernel_messages(&lp, &mut app);

    match start.input() {
        Some(mut input) => {
            input.insert(&lp.handle(), |app: &mut App| app.input.as_mut());
            app.set_input(input);
        }
        None => log::error!("no keymap, so vtium cannot read the keyboard"),
    }
    match Udev::new(app.vt.seat()) {
        Ok(mut udev) => {
            if let Err(e) = udev.scan() {
                log::warn!("cannot list the devices: {e}");
            }
            app.scanned();
            udev.insert(&lp.handle(), |app: &mut App| app.udev.as_mut())?;
            app.udev = Some(udev);
        }
        Err(e) => log::warn!("cannot watch for new devices: {e}"),
    }

    let started = if start.switch {
        app.vt.activate()
    } else {
        app.vt.wait()
    };
    if let Err(e) = started.as_ref() {
        log::error!("cannot switch to vtium's VT: {e}");
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
    // The sessions a keeper holds run on for the next vtium (ADR-1401
    // D13); the VT is put back before the loop's signalfd goes.
    app.release_sessions();
    drop(app);
    drop(lp);
    Ok(exit.status())
}

/// Reads the kernel's messages from `/dev/kmsg` through the loop, for the
/// app to show (ADR-1401 D15). Where vtium cannot open the device, it
/// logs why and shows none.
fn kernel_messages(lp: &Loop<App>, app: &mut App) {
    use vtium_platform::calloop::generic::Generic;
    use vtium_platform::calloop::{Interest, Mode, PostAction};
    let opened = vtium::kernel::System::open().and_then(|system| {
        let fd = system.watched()?;
        lp.handle()
            .insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                |_, _, app: &mut App| {
                    Ok(if app.kernel_ready() {
                        PostAction::Continue
                    } else {
                        PostAction::Remove
                    })
                },
            )
            .map_err(|e| e.error)?;
        Ok(system)
    });
    match opened {
        Ok(system) => app.set_kernel(Box::new(system)),
        Err(e) => log::info!(
            "vtium cannot read the kernel's messages from {}: {e}. It does not show them on screen.",
            vtium::kernel::KMSG
        ),
    }
}

#[cfg(test)]
mod tests {
    /// `--help`'s synopsis names every command of `vtium console`, the
    /// fallback that vtium-getty@.service runs included.
    #[test]
    fn help_names_every_console_command() {
        let help = super::help();
        for command in [
            "vtium console enable N|--all",
            "vtium console disable N|--all",
            "vtium console status",
            "vtium console fallback N",
        ] {
            assert!(help.contains(command), "{command}");
        }
    }

    /// `run` opens the VT through `notify::opened`, which sends systemd
    /// `READY=1` once the VT is open, and builds the app, which opens the
    /// cards and devices, only after it (ADR-1500 D7, "Ready before the
    /// display manager"). `run` is the binary's wiring, which no test
    /// drives, so this reads its text; `survive`'s tests drive `opened`.
    #[test]
    fn run_tells_systemd_it_is_ready_once_the_vt_is_open() {
        let source = include_str!("main.rs");
        let body = source
            .split_once("\nfn run(")
            .and_then(|(_, rest)| rest.split_once("\nfn "))
            .map(|(body, _)| body)
            .unwrap();
        let opened = body
            .find("vtium::survive::notify::opened(&env, || open(&mut lp))?")
            .expect("run opens the VT through notify::opened");
        assert_eq!(
            body.matches("open(&mut lp)").count(),
            1,
            "the VT opens once"
        );
        let app = body.find("App::new(").unwrap();
        assert!(opened < app, "READY=1 comes before the app opens the cards");
    }

    /// `--version` keeps its first line as it was, so a reader of that line
    /// alone sees no change, and adds the revision and the features, each
    /// read here from the build itself.
    #[test]
    fn version_names_the_revision_and_the_features_built() {
        let text = super::version();
        let lines: Vec<&str> = text.lines().collect();
        let built: Vec<&str> = [
            ("bidi", cfg!(feature = "bidi")),
            ("gpu", cfg!(feature = "gpu")),
            ("images", cfg!(feature = "images")),
            ("libseat", cfg!(feature = "libseat")),
            ("logind", cfg!(feature = "logind")),
        ]
        .iter()
        .filter(|(_, on)| *on)
        .map(|(name, _)| *name)
        .collect();
        let features = if built.is_empty() {
            "features none".to_owned()
        } else {
            format!("features {}", built.join(" "))
        };
        assert_eq!(
            lines,
            [
                format!("vtium {}", env!("CARGO_PKG_VERSION")).as_str(),
                format!("revision {}", vtium::logger::REVISION).as_str(),
                features.as_str(),
            ],
            "{text}"
        );
        assert!(text.ends_with('\n'), "{text:?}");
    }
}
