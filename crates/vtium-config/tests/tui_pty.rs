//! The real binary on a pty (ADR-1201 D8): `vtium-config` with no command
//! runs its TUI with the pty as its controlling terminal, and on the other
//! side the console's own terminal, vtium-term, answers as a vtium console
//! does: XTVERSION, DA1, and OSC 50 under `allow-font-ops`, switching to a
//! font it takes and naming the font it draws in each reply. The test types
//! keys into the pty, records every byte the TUI writes, and reads the
//! terminal's screen as text.
//!
//! Each wait is for a state the console reaches, bounded only so that a
//! test fails instead of hanging.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::termios::{LocalModes, Winsize, tcgetattr, tcsetwinsize};
use vtium_term::{Glyph, Terminal};

#[path = "common/console.rs"]
mod console_bin;

/// A pty's master and its slave, neither the test's controlling terminal.
fn pair(cols: u16, rows: u16) -> (File, File) {
    let master =
        openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC).expect("a pty");
    grantpt(&master).expect("grantpt");
    unlockpt(&master).expect("unlockpt");
    let name = ptsname(&master, Vec::new()).expect("ptsname");
    let slave = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(rustix::fs::OFlags::NOCTTY.bits() as i32)
        .open(name.to_str().expect("a path"))
        .expect("the slave");
    tcsetwinsize(
        &master,
        Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        },
    )
    .expect("the grid");
    (File::from(master), slave)
}

/// What the console has seen.
#[derive(Default)]
struct Seen {
    /// Every byte the TUI wrote.
    written: Vec<u8>,
    screen: String,
    font: String,
    /// The pty closed.
    closed: bool,
}

/// The console on a pty's master.
struct Console {
    seen: Arc<(Mutex<Seen>, Condvar)>,
    master: File,
    /// Ends when every end of the pty closes.
    _thread: JoinHandle<()>,
}

/// How the stand-in on the master answers.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// vtium, with font operations on or off.
    Vtium { ops: bool },
    /// A terminal that answers DA1 alone.
    Da1Only,
}

fn screen_text(t: &Terminal) -> String {
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

impl Console {
    fn start(
        master: File,
        cols: usize,
        rows: usize,
        kind: Kind,
        takes: &'static [&'static str],
    ) -> Console {
        let seen: Arc<(Mutex<Seen>, Condvar)> = Arc::default();
        let mut reader = master.try_clone().expect("dup");
        let shared = Arc::clone(&seen);
        let thread = std::thread::spawn(move || {
            let mut t = Terminal::new(cols, rows, 0);
            t.set_version("vtium(test)").expect("printable");
            t.set_font_ops(matches!(kind, Kind::Vtium { ops: true }));
            assert!(t.set_font_name("unifont"));
            let mut buf = [0u8; 4096];
            while let Ok(n @ 1..) = reader.read(&mut buf) {
                let bytes = &buf[..n];
                let mut replies = Vec::new();
                match kind {
                    Kind::Vtium { .. } => {
                        let mut rest = bytes;
                        while !rest.is_empty() {
                            let taken = t.input(rest);
                            if let Some(name) = t.take_font_request()
                                && takes.contains(&name.as_str())
                            {
                                assert!(t.set_font_name(&name));
                            }
                            rest = &rest[taken..];
                        }
                        replies = t.take_replies();
                    }
                    Kind::Da1Only => {
                        t.input(bytes);
                        let _ = t.take_replies();
                        if bytes.windows(3).any(|w| w == b"\x1b[c") {
                            replies = b"\x1b[?62c".to_vec();
                        }
                    }
                }
                {
                    let (lock, cv) = &*shared;
                    let mut s = lock.lock().expect("the lock");
                    s.written.extend_from_slice(bytes);
                    s.screen = screen_text(&t);
                    // the font the console draws: its query's answer
                    t.input(b"\x1b]50;?\x1b\\");
                    let probe = t.take_replies();
                    let text = String::from_utf8_lossy(&probe);
                    s.font = text
                        .strip_prefix("\x1b]50;")
                        .and_then(|r| r.split('\x1b').next())
                        .unwrap_or("")
                        .to_owned();
                    cv.notify_all();
                }
                if !replies.is_empty() && reader.write_all(&replies).is_err() {
                    break;
                }
            }
            let (lock, cv) = &*shared;
            lock.lock().expect("the lock").closed = true;
            cv.notify_all();
        });
        Console {
            seen,
            master,
            _thread: thread,
        }
    }

    /// Waits until `done` holds of what the console has seen.
    fn until(&self, what: &str, done: impl Fn(&Seen) -> bool) {
        let (lock, cv) = &*self.seen;
        let guard = lock.lock().expect("the lock");
        let (guard, timeout) = cv
            .wait_timeout_while(guard, Duration::from_secs(10), |s| !done(s))
            .expect("the lock");
        assert!(
            !timeout.timed_out(),
            "never {what}; the screen:\n{}\nwritten: {:?}",
            guard.screen,
            String::from_utf8_lossy(&guard.written)
        );
    }

    fn key(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).expect("a key");
    }

    fn written(&self) -> Vec<u8> {
        self.seen.0.lock().expect("the lock").written.clone()
    }

    fn screen(&self) -> String {
        self.seen.0.lock().expect("the lock").screen.clone()
    }

    fn font(&self) -> String {
        self.seen.0.lock().expect("the lock").font.clone()
    }

    /// The OSC 50 switches the TUI wrote, the query at start apart.
    fn switches(&self) -> Vec<String> {
        String::from_utf8_lossy(&self.written())
            .split("\x1b]50;")
            .skip(1)
            .map(|s| s.split('\x1b').next().unwrap_or("").to_owned())
            .filter(|s| s != "?")
            .collect()
    }
}

/// A configuration directory and a frits directory with the shipped frits.
fn fixture(name: &str) -> (PathBuf, PathBuf) {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("tui-pty")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    let fonts = root.join("fonts");
    std::fs::create_dir_all(&fonts).expect("the fixture");
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("frits");
    for entry in std::fs::read_dir(shipped).expect("the shipped frits") {
        let path = entry.expect("an entry").path();
        std::fs::copy(&path, fonts.join(path.file_name().expect("a name"))).expect("a copy");
    }
    std::fs::write(root.join("vtium.conf"), "font-name=unifont\n").expect("vtium.conf");
    (root, fonts)
}

/// The TUI on `slave`, with the slave as its controlling terminal.
fn tui(slave: &File, root: &Path, fonts: &Path) -> Child {
    let io = || Stdio::from(slave.as_fd().try_clone_to_owned().expect("dup"));
    Command::new("setsid")
        .arg("--ctty")
        .arg(env!("CARGO_BIN_EXE_vtium-config"))
        .arg("--no-banner")
        .arg("--configdir")
        .arg(root)
        .arg("--frits")
        .arg(fonts)
        .env("XDG_STATE_HOME", root.join("state"))
        .stdin(io())
        .stdout(io())
        .stderr(io())
        .spawn()
        .expect("setsid runs")
}

const TAKES: &[&str] = &[
    "minspec-16",
    "minspec-8",
    "proof",
    "tiny5",
    "unifont",
    "unscii-8",
];
const UP: &[u8] = b"\x1b[A";

/// Starts the TUI against a console of `kind` and waits for it to have
/// decided what the terminal is.
fn started(name: &str, kind: Kind, cols: u16, rows: u16) -> (Console, Child, File) {
    let (master, slave) = pair(cols, rows);
    let (root, fonts) = fixture(name);
    let console = Console::start(master, cols.into(), rows.into(), kind, TAKES);
    let child = tui(&slave, &root, &fonts);
    console.until("drew its keys", |s| s.screen.contains("q leave"));
    console.until("decided", |s| !s.screen.contains("asking the terminal"));
    (console, child, slave)
}

fn status(mut child: Child) -> i32 {
    let status = child.wait().expect("wait");
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

fn cooked(slave: &File) -> bool {
    tcgetattr(slave)
        .expect("tcgetattr")
        .local_modes
        .contains(LocalModes::ICANON | LocalModes::ECHO)
}

#[test]
fn the_tui_starts_on_the_alternate_screen_with_the_four_requests() {
    let (mut c, child, slave) = started("start", Kind::Vtium { ops: true }, 80, 25);
    let written = c.written();
    let start = b"\x1b[?1049h\x1b[?25l\x1b[>q\x1b]50;?\x1b\\\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c";
    assert!(
        written.starts_with(start),
        "{:?}",
        String::from_utf8_lossy(&written[..written.len().min(80)])
    );
    c.key(b"q");
    assert_eq!(status(child), 0);
    assert!(
        c.written().ends_with(b"\x1b[?25h\x1b[?1049l"),
        "the screen and cursor come back"
    );
    assert!(cooked(&slave), "the terminal's mode is back");
}

#[test]
fn a_highlighted_font_switches_the_console_and_q_gives_the_start_font_back() {
    let (mut c, child, slave) = started("browse", Kind::Vtium { ops: true }, 80, 25);
    assert_eq!(c.font(), "unifont");
    c.key(UP);
    c.until("switched to tiny5", |s| {
        String::from_utf8_lossy(&s.written).contains("\x1b]50;tiny5\x1b\\\x1b]50;?\x1b\\")
    });
    c.until("drew tiny5", |s| s.font == "tiny5");
    c.key(b"q");
    assert_eq!(status(child), 0);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    assert_eq!(c.font(), "unifont");
    assert!(cooked(&slave));
}

#[test]
fn ctrl_c_gives_the_start_font_back() {
    let (mut c, child, _slave) = started("ctrl-c", Kind::Vtium { ops: true }, 80, 25);
    c.key(UP);
    c.until("drew tiny5", |s| s.font == "tiny5");
    c.key(b"\x03");
    assert_eq!(status(child), 130);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    assert_eq!(c.font(), "unifont");
}

#[test]
fn sigterm_from_another_terminal_gives_the_start_font_back() {
    let (mut c, child, slave) = started("sigterm", Kind::Vtium { ops: true }, 80, 25);
    c.key(UP);
    c.until("drew tiny5", |s| s.font == "tiny5");
    let pid = rustix::process::Pid::from_raw(child.id() as i32).expect("a pid");
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).expect("kill");
    assert_eq!(status(child), 143);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    assert_eq!(c.font(), "unifont");
    assert!(cooked(&slave));
}

#[test]
fn a_terminal_that_answers_only_da1_gets_no_osc_50_switch() {
    let (mut c, child, _slave) = started("da1", Kind::Da1Only, 80, 25);
    c.key(UP);
    c.key(UP);
    c.key(b"q");
    assert_eq!(status(child), 0);
    assert_eq!(c.switches(), Vec::<String>::new());
    assert_eq!(
        String::from_utf8_lossy(&c.written())
            .matches("\x1b]50;")
            .count(),
        1
    );
}

#[test]
fn with_font_operations_off_no_osc_50_switch_goes() {
    let (mut c, child, _slave) = started("ops-off", Kind::Vtium { ops: false }, 80, 25);
    assert!(
        c.screen().contains("allow-font-ops is off"),
        "{}",
        c.screen()
    );
    c.key(UP);
    c.key(b"q");
    assert_eq!(status(child), 0);
    assert_eq!(c.switches(), Vec::<String>::new());
}

#[test]
fn the_screen_fits_80x25_and_100x30() {
    for (cols, rows) in [(80u16, 25u16), (100, 30)] {
        let (mut c, child, _slave) = started(
            &format!("grid-{cols}"),
            Kind::Vtium { ops: true },
            cols,
            rows,
        );
        let screen = c.screen();
        let lines: Vec<&str> = screen.lines().collect();
        assert_eq!(lines.len(), usize::from(rows));
        assert!(
            lines[0].contains("VTium Vitrifier") && lines[0].contains("live preview: on"),
            "{screen}"
        );
        for name in [
            "minspec-16",
            "minspec-8",
            "proof",
            "tiny5",
            "unifont",
            "unscii-8",
        ] {
            assert!(screen.contains(name), "{name}: {screen}");
        }
        assert!(
            lines[usize::from(rows) - 1].contains("q leave, undo"),
            "{screen}"
        );
        c.key(b"q");
        assert_eq!(status(child), 0);
    }
}

/// `vtium-config set NAME` on the pty, as its controlling terminal.
fn set(name: &str, kind: Kind) -> (i32, String, Vec<String>) {
    let (master, slave) = pair(80, 25);
    let label = match kind {
        Kind::Vtium { ops: true } => "vtium",
        Kind::Vtium { ops: false } => "ops-off",
        Kind::Da1Only => "da1",
    };
    let (_root, fonts) = fixture(&format!("set-{name}-{label}"));
    let c = Console::start(master, 80, 25, kind, TAKES);
    let out = Command::new("setsid")
        .arg("--ctty")
        .arg(env!("CARGO_BIN_EXE_vtium-config"))
        .arg("--frits")
        .arg(&fonts)
        .args(["set", name])
        .stdin(Stdio::from(
            slave.as_fd().try_clone_to_owned().expect("dup"),
        ))
        .output()
        .expect("setsid runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        c.switches(),
    )
}

#[test]
fn set_switches_a_vtium_console() {
    let (status, out, switches) = set("tiny5", Kind::Vtium { ops: true });
    assert_eq!((status, out.as_str()), (0, "set\ttiny5\n"));
    assert_eq!(switches, ["tiny5"]);
}

#[test]
fn set_sends_nothing_to_a_terminal_that_is_not_vtium() {
    let (status, out, switches) = set("tiny5", Kind::Da1Only);
    assert_eq!(status, 1, "{out}");
    assert!(out.contains("not vtium"), "{out}");
    assert_eq!(switches, Vec::<String>::new());
}

/// Spawns the TUI on `slave` with `options` before its own.
fn tui_with(slave: &File, root: &Path, options: &[&std::ffi::OsStr]) -> Child {
    let io = || Stdio::from(slave.as_fd().try_clone_to_owned().expect("dup"));
    Command::new("setsid")
        .arg("--ctty")
        .arg(env!("CARGO_BIN_EXE_vtium-config"))
        .arg("--no-banner")
        .args(options)
        .env("XDG_STATE_HOME", root.join("state"))
        .stdin(io())
        .stdout(io())
        .stderr(io())
        .spawn()
        .expect("setsid runs")
}

#[test]
fn saving_into_directories_not_the_system_s_installs_as_the_user() {
    let (master, slave) = pair(80, 25);
    let (root, fonts) = fixture("save");
    let conf = format!("font-frits={}\nfont-name=unifont\n", fonts.display());
    std::fs::write(root.join("vtium.conf"), &conf).expect("vtium.conf");
    let vtium = console_bin::vtium();
    let mut c = Console::start(master, 80, 25, Kind::Vtium { ops: true }, TAKES);
    let child = tui_with(
        &slave,
        &root,
        &[
            "--configdir".as_ref(),
            root.as_os_str(),
            "--frits".as_ref(),
            fonts.as_os_str(),
            "--vtium".as_ref(),
            vtium.as_os_str(),
        ],
    );
    c.until("decided", |s| s.screen.contains("live preview: on"));
    c.key(UP);
    c.until("drew tiny5", |s| s.font == "tiny5");
    c.key(UP);
    c.until("drew proof", |s| s.font == "proof");
    c.key(b"\r");
    c.key(b"s");
    c.until("showed the save", |s| {
        s.screen.contains("+ font-name=proof")
    });
    assert!(c.screen().contains("with no sudo"), "{}", c.screen());
    c.key(b"\r");
    c.until("saved", |s| s.screen.contains("Saved 1 file."));
    c.key(b"q");
    assert_eq!(status(child), 0);
    // one line changed, the old file kept, and the console left in the
    // saved font
    assert_eq!(
        std::fs::read_to_string(root.join("vtium.conf")).expect("vtium.conf"),
        format!("font-frits={}\nfont-name=proof\n", fonts.display())
    );
    assert_eq!(
        std::fs::read_to_string(root.join("vtium.conf.bak")).expect("the old"),
        conf
    );
    assert_eq!(c.font(), "proof");
    assert_eq!(c.switches(), ["tiny5", "proof"]);
    // the TUI left its screen for the command and came back
    let written = String::from_utf8_lossy(&c.written()).into_owned();
    assert!(
        written.matches("\x1b[?1049l").count() >= 2,
        "left for the command and at the end"
    );
}

#[test]
fn saving_the_system_s_files_runs_exactly_sudo_vtium_config_apply_dir() {
    let (master, slave) = pair(80, 25);
    let (root, _fonts) = fixture("save-system");
    // the stand-in for sudo records its argv and its blocked signals and
    // installs nothing
    let sudo = root.join("sudo");
    std::fs::write(
        &sudo,
        "#!/bin/sh\ngrep '^SigBlk' /proc/self/status > \"$0.sigblk\"\nprintf '%s\\n' \"$@\" > \"$0.argv\"\necho 'the stand-in installs nothing'\nexit 1\n",
    )
    .expect("the stand-in");
    std::fs::set_permissions(&sudo, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("its mode");
    let mut c = Console::start(master, 80, 25, Kind::Vtium { ops: true }, TAKES);
    let child = tui_with(&slave, &root, &["--sudo".as_ref(), sudo.as_os_str()]);
    c.until("decided", |s| s.screen.contains("live preview: on"));
    // a size, so the save changes the system's vtium.conf whatever it holds
    c.key(b"\t");
    c.key(b"\x1b[B");
    c.key(b"\r");
    c.key(b"s");
    c.until("showed the save", |s| s.screen.contains("+ font-size=6"));
    let program = env!("CARGO_BIN_EXE_vtium-config");
    assert!(
        c.screen().contains("apply DRAFT"),
        "the command shown: {}",
        c.screen()
    );
    c.key(b"\r");
    c.until("failed", |s| s.screen.contains("Nothing was saved"));
    assert!(
        c.screen().contains("the stand-in installs nothing"),
        "{}",
        c.screen()
    );
    c.key(b"q");
    assert_eq!(status(child), 0);
    let argv = std::fs::read_to_string(root.join("sudo.argv")).expect("the stand-in ran");
    let argv: Vec<&str> = argv.lines().collect();
    assert_eq!(argv.len(), 3, "{argv:?}");
    assert_eq!(argv[0], program);
    assert_eq!(argv[1], "apply");
    assert!(
        argv[2].starts_with(root.join("state/vtium/drafts").to_str().expect("a path")),
        "{argv:?}"
    );
    // the command ran with no signal held
    let blocked = std::fs::read_to_string(root.join("sudo.sigblk")).expect("the stand-in ran");
    assert_eq!(blocked.trim(), "SigBlk:\t0000000000000000", "{blocked}");
}
