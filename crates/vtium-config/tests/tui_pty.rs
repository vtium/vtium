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

#[path = "common/home.rs"]
mod home;

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
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
    /// For each read that held an OSC 50 switch, how many it held.
    switches_per_read: Vec<usize>,
    /// The NULs the test wrote to the terminal that the console has read.
    marks: usize,
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
                // a NUL is the test's mark (`drained`), which the TUI never
                // writes, so the console counts it and drops it
                let marks = buf[..n].iter().filter(|&&b| b == 0).count();
                let kept: Vec<u8> = buf[..n].iter().copied().filter(|&b| b != 0).collect();
                let bytes = &kept[..];
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
                    s.marks += marks;
                    let sets = String::from_utf8_lossy(bytes)
                        .split("\x1b]50;")
                        .skip(1)
                        .filter(|r| !r.starts_with('?'))
                        .count();
                    if sets > 0 {
                        s.switches_per_read.push(sets);
                    }
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

    /// Waits until `done` holds of what the console has seen, and returns
    /// the screen on which it held.
    ///
    /// The TUI paints its screen row by row in one write, and the console
    /// may read that write in pieces, so a screen can hold the top of a new
    /// frame and the bottom of the old one, with one row cut partway. A
    /// test therefore waits until every text it checks is on the screen,
    /// and checks the screen this returns rather than reading it again.
    fn until(&self, what: &str, done: impl Fn(&Seen) -> bool) -> String {
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
        guard.screen.clone()
    }

    /// Waits until the console has read every byte written to the
    /// terminal before now. A NUL written to the terminal arrives after
    /// every byte written before it, so the test writes one and waits for
    /// the console to read it, or for the console to have closed, after
    /// which it reads nothing more.
    fn drained(&self, slave: &File) {
        let before = self.seen.0.lock().expect("the lock").marks;
        let mut out = slave;
        out.write_all(&[0]).expect("a mark");
        self.until("read everything written to the terminal", |s| {
            s.marks > before || s.closed
        });
    }

    fn key(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).expect("a key");
    }

    fn written(&self) -> Vec<u8> {
        self.seen.0.lock().expect("the lock").written.clone()
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
    home::command("setsid")
        .arg("--ctty")
        .arg(env!("CARGO_BIN_EXE_vtium-config"))
        .arg("--no-banner")
        .arg("--configdir")
        .arg(root)
        .arg("--frits")
        .arg(fonts)
        // the devices of the fixture's own, none, and no udev daemon
        .arg("--sys")
        .arg(root.join("sys"))
        .arg("--udev-data")
        .arg(root.join("udev/data"))
        .arg("tui")
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CONFIG_HOME", root.join("config"))
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

/// A click on the Fonts tab, as mode 1006 reports it: row 1, column 16,
/// counted from 1.
const FONTS_TAB: &[u8] = b"\x1b[<0;16;1M\x1b[<0;16;1m";

/// Clicks the Fonts tab of the Flow view, which the TUI opens on, and
/// waits for the Fonts view's keys.
fn to_fonts(c: &mut Console) {
    c.until("opened on the flow", |s| s.screen.contains("DISPLAYS"));
    c.key(FONTS_TAB);
    c.until("showed the fonts", |s| s.screen.contains("n new frit"));
}

/// Starts the TUI against a console of `kind`, waits for it to have
/// decided what the terminal is, and goes to the Fonts view.
fn started(name: &str, kind: Kind, cols: u16, rows: u16) -> (Console, Child, File) {
    let (master, slave) = pair(cols, rows);
    let (root, fonts) = fixture(name);
    let mut console = Console::start(master, cols.into(), rows.into(), kind, TAKES);
    let child = tui(&slave, &root, &fonts);
    console.until("drew its keys", |s| s.screen.contains("q leave"));
    console.until("decided", |s| !s.screen.contains("asking the terminal"));
    to_fonts(&mut console);
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
    // the four requests, then mouse reporting for the Flow view it opens on
    let start = b"\x1b[?1049h\x1b[?25l\x1b[>q\x1b]50;?\x1b\\\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c\x1b[?1000h\x1b[?1006h";
    assert!(
        written.starts_with(start),
        "{:?}",
        String::from_utf8_lossy(&written[..written.len().min(100)])
    );
    // the click on the Fonts tab turned it off again
    let text = String::from_utf8_lossy(&written).into_owned();
    assert!(
        text.contains("\x1b[?1006l\x1b[?1000l"),
        "mouse reporting off"
    );
    c.key(b"q");
    assert_eq!(status(child), 0);
    c.drained(&slave);
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
    c.drained(&slave);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    assert_eq!(c.font(), "unifont");
    assert!(cooked(&slave));
}

#[test]
fn ctrl_c_gives_the_start_font_back() {
    let (mut c, child, slave) = started("ctrl-c", Kind::Vtium { ops: true }, 80, 25);
    c.key(UP);
    c.until("drew tiny5", |s| s.font == "tiny5");
    c.key(b"\x03");
    assert_eq!(status(child), 130);
    c.drained(&slave);
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
    c.drained(&slave);
    assert_eq!(c.switches(), ["tiny5", "unifont"]);
    assert_eq!(c.font(), "unifont");
    assert!(cooked(&slave));
}

#[test]
fn a_terminal_that_answers_only_da1_gets_no_osc_50_switch() {
    let (mut c, child, slave) = started("da1", Kind::Da1Only, 80, 25);
    c.key(UP);
    c.key(UP);
    c.key(b"q");
    assert_eq!(status(child), 0);
    c.drained(&slave);
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
    let (mut c, child, slave) = started("ops-off", Kind::Vtium { ops: false }, 80, 25);
    c.until("said font operations are off", |s| {
        s.screen.contains("allow-font-ops is off")
    });
    c.key(UP);
    c.key(b"q");
    assert_eq!(status(child), 0);
    c.drained(&slave);
    assert_eq!(c.switches(), Vec::<String>::new());
}

#[test]
fn the_screen_fits_80x25_and_100x30() {
    for (cols, rows) in [(80u16, 25u16), (100, 30)] {
        let (mut c, child, slave) = started(
            &format!("grid-{cols}"),
            Kind::Vtium { ops: true },
            cols,
            rows,
        );
        // the tabs, the live preview, every frit and the keys, each on its
        // row of the grid
        c.until("drew the fonts view in the grid", |s| {
            let lines: Vec<&str> = s.screen.lines().collect();
            lines.len() == usize::from(rows)
                && lines[0].contains(" Fonts ")
                && lines[1].contains("live preview: on")
                && [
                    "minspec-16",
                    "minspec-8",
                    "proof",
                    "tiny5",
                    "unifont",
                    "unscii-8",
                ]
                .iter()
                .all(|name| s.screen.contains(name))
                && lines[usize::from(rows) - 1].contains("q leave, undo")
        });
        c.key(b"q");
        assert_eq!(status(child), 0);
        c.drained(&slave);
    }
}

/// `vtium-config use-frit NAME` on the pty, as its controlling terminal.
fn set(name: &str, kind: Kind) -> (i32, String, Vec<String>) {
    let (master, slave) = pair(80, 25);
    let label = match kind {
        Kind::Vtium { ops: true } => "vtium",
        Kind::Vtium { ops: false } => "ops-off",
        Kind::Da1Only => "da1",
    };
    let (_root, fonts) = fixture(&format!("set-{name}-{label}"));
    let c = Console::start(master, 80, 25, kind, TAKES);
    let out = home::command("setsid")
        .arg("--ctty")
        .arg(env!("CARGO_BIN_EXE_vtium-config"))
        .arg("--frits")
        .arg(&fonts)
        .args(["use-frit", name])
        .stdin(Stdio::from(
            slave.as_fd().try_clone_to_owned().expect("dup"),
        ))
        .output()
        .expect("setsid runs");
    c.drained(&slave);
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
    home::command("setsid")
        .arg("--ctty")
        .arg(env!("CARGO_BIN_EXE_vtium-config"))
        .arg("--no-banner")
        .args(options)
        .arg("tui")
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CONFIG_HOME", root.join("config"))
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
    to_fonts(&mut c);
    c.key(UP);
    c.until("drew tiny5", |s| s.font == "tiny5");
    c.key(UP);
    c.until("drew proof", |s| s.font == "proof");
    c.key(b"\r");
    c.key(b"i");
    c.until("asked", |s| {
        s.screen
            .contains("Install the draft? 1 line added, 1 removed.")
    });
    c.key(b"y");
    c.until("installed", |s| s.screen.contains("Installed draft"));
    c.key(b"q");
    assert_eq!(status(child), 0);
    c.drained(&slave);
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
    // the draft starts as the shipped example, which the check passes on
    // any machine with a display and a keyboard, whatever the system's own
    // vtium.conf holds
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vtium/install/vtium.conf.example");
    let drafts = root.join("config/vtium/drafts");
    std::fs::create_dir_all(&drafts).expect("the store");
    std::fs::copy(&example, drafts.join("pty.conf")).expect("the draft");
    std::fs::write(root.join("config/vtium/vtium-config.conf"), "draft = pty\n")
        .expect("the settings");
    let mut c = Console::start(master, 80, 25, Kind::Vtium { ops: true }, TAKES);
    // the check runs the console this build made, never the host's
    let vtium = console_bin::vtium();
    let child = tui_with(
        &slave,
        &root,
        &[
            "--sudo".as_ref(),
            sudo.as_os_str(),
            "--vtium".as_ref(),
            vtium.as_os_str(),
        ],
    );
    c.until("decided", |s| s.screen.contains("live preview: on"));
    to_fonts(&mut c);
    // a size, so the draft differs from the system's vtium.conf whatever it
    // holds
    c.key(b"\x1b[Z");
    c.key(b"\x1b[B");
    c.key(b"\r");
    c.key(b"i");
    c.until("asked", |s| s.screen.contains("Install the draft?"));
    let program = env!("CARGO_BIN_EXE_vtium-config");
    c.key(b"y");
    // the failure, with what the stand-in printed
    c.until("failed", |s| {
        s.screen.contains("Nothing is installed")
            && s.screen.contains("the stand-in installs nothing")
    });
    c.key(b"q");
    assert_eq!(status(child), 0);
    c.drained(&slave);
    let argv = std::fs::read_to_string(root.join("sudo.argv")).expect("the stand-in ran");
    let argv: Vec<&str> = argv.lines().collect();
    assert_eq!(argv.len(), 3, "{argv:?}");
    assert_eq!(argv[0], program);
    assert_eq!(argv[1], "apply");
    assert!(
        argv[2].starts_with(root.join("state/vtium").to_str().expect("a path")),
        "{argv:?}"
    );
    // the command ran with no signal held
    let blocked = std::fs::read_to_string(root.join("sudo.sigblk")).expect("the stand-in ran");
    assert_eq!(blocked.trim(), "SigBlk:\t0000000000000000", "{blocked}");
}

/// The command line's `--udev-data` reaches the TUI: beside a database
/// with no udev control socket, the status line says the view cannot
/// follow hotplug, where this machine's own udev runs.
#[test]
fn the_udev_database_the_command_line_names_reaches_the_tui() {
    let (master, slave) = pair(100, 30);
    let (root, fonts) = fixture("udev-data");
    std::fs::create_dir_all(root.join("udev/data")).expect("the database");
    let mut c = Console::start(master, 100, 30, Kind::Vtium { ops: true }, TAKES);
    let child = tui_with(
        &slave,
        &root,
        &[
            "--configdir".as_ref(),
            root.as_os_str(),
            "--frits".as_ref(),
            fonts.as_os_str(),
            "--vtium".as_ref(),
            "/bin/true".as_ref(),
            "--sys".as_ref(),
            root.join("sys").as_os_str(),
            "--udev-data".as_ref(),
            root.join("udev/data").as_os_str(),
        ],
    );
    c.until("followed no hotplug", |s| {
        s.screen.contains("cannot follow hotplug")
    });
    c.key(b"q");
    assert_eq!(status(child), 0);
    c.drained(&slave);
}

#[test]
fn two_keys_at_once_send_one_switch_and_the_next_after_its_reply() {
    let (mut c, child, slave) = started("in-flight", Kind::Vtium { ops: true }, 80, 25);
    // two Ups in one write: tiny5, then proof once tiny5's reply is in
    c.key(&[UP, UP].concat());
    c.until("drew proof", |s| s.font == "proof");
    c.key(b"q");
    assert_eq!(status(child), 0);
    c.drained(&slave);
    assert_eq!(c.switches(), ["tiny5", "proof", "unifont"]);
    let per_read = c.seen.0.lock().expect("the lock").switches_per_read.clone();
    assert!(
        per_read.iter().all(|&n| n == 1),
        "one switch at a time: {per_read:?}"
    );
}

#[test]
fn sighup_sigint_and_sigquit_give_the_start_font_back() {
    use rustix::process::Signal;
    for (signal, n) in [(Signal::HUP, 1), (Signal::INT, 2), (Signal::QUIT, 3)] {
        let (mut c, child, slave) =
            started(&format!("signal-{n}"), Kind::Vtium { ops: true }, 80, 25);
        c.key(UP);
        c.until("drew tiny5", |s| s.font == "tiny5");
        let pid = rustix::process::Pid::from_raw(child.id() as i32).expect("a pid");
        rustix::process::kill_process(pid, signal).expect("kill");
        assert_eq!(status(child), 128 + n, "signal {n}");
        c.drained(&slave);
        assert_eq!(c.switches(), ["tiny5", "unifont"], "signal {n}");
        assert_eq!(c.font(), "unifont", "signal {n}");
        assert!(cooked(&slave), "signal {n}");
    }
}

#[test]
fn a_terminal_that_closes_ends_the_tui() {
    // the test answers on the master itself, then closes it
    let (mut master, slave) = pair(80, 25);
    let (root, fonts) = fixture("closed");
    let mut child = tui(&slave, &root, &fonts);
    drop(slave);
    let mut got = Vec::new();
    let mut buf = [0u8; 4096];
    while !got.windows(3).any(|w| w == b"\x1b[c") {
        let n = master.read(&mut buf).expect("the requests");
        got.extend_from_slice(&buf[..n]);
    }
    master
        .write_all(b"\x1bP>|vtium(test)\x1b\\\x1b]50;unifont\x1b\\\x1b[?62c")
        .expect("the answers");
    // the TUI opens on the flow: the Fonts tab, then a font
    master.write_all(FONTS_TAB).expect("a click");
    master.write_all(UP).expect("a key");
    while !got.windows(9).any(|w| w == b"\x1b]50;tiny") {
        let n = master.read(&mut buf).expect("the switch");
        got.extend_from_slice(&buf[..n]);
    }
    drop(master);
    // bounded only so a hang fails instead of stopping the test run
    let (done, ended) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(child.wait());
    });
    let status = ended
        .recv_timeout(Duration::from_secs(10))
        .expect("the TUI ends when its terminal closes")
        .expect("wait");
    let code = status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0));
    assert!(
        matches!(code, 1 | 129),
        "the hangup's status, or a failed write's: {code}"
    );
}

/// The glyph helper the TUI starts holds only its three pipes, never a copy
/// of the user's terminal, which a font could write to, nor a descriptor the
/// TUI inherited; and while that
/// helper does not answer, the TUI goes on taking keys, and marks no frit
/// as drawing nothing, only its sources as not checked.
#[test]
fn the_helper_holds_none_of_the_terminal_and_a_silent_one_marks_no_frit_broken() {
    let (master, slave) = pair(80, 25);
    let (root, fonts) = fixture("silent-helper");
    // A stand-in for the helper, which never answers. The shell lists the
    // descriptors it inherited before it becomes sleep, because sleep holds
    // descriptors of its own while it starts (the dynamic loader holds each
    // library open while it maps it). `ls` takes the list inside a subshell,
    // a child of the shell, and the redirect to the list belongs to that
    // subshell, so the shell's own descriptors do not change while the list
    // is taken. A plain `ls ... > file` would let a shell such as dash, which
    // Ubuntu runs as /bin/sh, apply the redirect in the shell itself and keep
    // its real stdout on a spare descriptor, which the list then shows. The
    // stand-in runs under bash all the same, so every machine runs the same
    // shell. The shell renames the list into place, so the test reads either
    // no list or the whole of it, and the partial list's name carries the
    // shell's pid, so no other stand-in can rename it.
    let helper = root.join("helper");
    std::fs::write(
        &helper,
        "#!/bin/bash\n( ls -l /proc/$$/fd ) > \"$0.fds.$$\"\nmv \"$0.fds.$$\" \"$0.fds\"\nexec sleep 30\n",
    )
    .expect("helper");
    std::fs::set_permissions(&helper, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("its mode");
    let mut c = Console::start(master, 80, 25, Kind::Vtium { ops: true }, TAKES);
    // The TUI inherits a descriptor without close-on-exec, as a CI runner
    // leaves some open, and must not pass it on to the helper. dup's copy
    // lacks the flag. Other tests' children that start meanwhile inherit it
    // too, which does them no harm, since it is only /dev/null.
    let leaked =
        rustix::io::dup(File::open("/dev/null").expect("/dev/null")).expect("a leaked descriptor");
    let child = tui_with(
        &slave,
        &root,
        &[
            "--configdir".as_ref(),
            root.as_os_str(),
            "--frits".as_ref(),
            fonts.as_os_str(),
            "--helper".as_ref(),
            helper.as_os_str(),
        ],
    );
    drop(leaked);
    c.until("decided", |s| s.screen.contains("live preview: on"));
    to_fonts(&mut c);
    // the stand-in lists what the TUI gave it
    let fds_file = root.join("helper.fds");
    let deadline = now() + Duration::from_secs(10);
    let listed = loop {
        if let Ok(listed) = std::fs::read_to_string(&fds_file) {
            break listed;
        }
        assert!(now() < deadline, "the helper never started");
        std::thread::yield_now();
    };
    // After its "total" line, each line of `ls -l` reads "... <number> ->
    // <target>". A line without a target, which `ls` writes when it cannot
    // read the link, fails the test rather than hiding a descriptor.
    let all: Vec<(u32, String)> = listed
        .lines()
        .filter(|line| !line.starts_with("total "))
        .map(|line| {
            let parsed = line.split_once(" -> ").and_then(|(left, to)| {
                let n = left.split_whitespace().last()?.parse().ok()?;
                Some((n, to.to_owned()))
            });
            parsed.unwrap_or_else(|| panic!("a descriptor and its target: {line:?}"))
        })
        .collect();
    // The shell also holds the script it reads, on one descriptor of its own
    // (bash keeps it on 255) that it closes when it becomes sleep. That entry
    // is the shell's, not the TUI's, so the test leaves it out.
    let script = std::fs::canonicalize(&helper).expect("the stand-in's path");
    let script = script.to_string_lossy();
    let (on_script, mut fds): (Vec<_>, Vec<_>) = all.into_iter().partition(|(_, to)| *to == script);
    assert!(
        on_script.len() <= 1,
        "the shell holds its script once: {on_script:?}"
    );
    fds.sort();
    let numbers: Vec<u32> = fds.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        numbers,
        [0, 1, 2],
        "only its stdin and stdout pipes and stderr: {fds:?}"
    );
    assert!(
        fds.iter().all(|(_, to)| !to.starts_with("/dev/pts")),
        "no terminal: {fds:?}"
    );
    // a key reaches the TUI while the helper is silent
    c.key(UP);
    c.until("switched", |s| s.font == "tiny5");
    // once its wait is out, the TUI says it has not checked the sources
    let screen = c.until("gave up on the helper", |s| {
        s.screen.contains("sources not checked")
    });
    c.key(b"q");
    assert_eq!(status(child), 0);
    c.drained(&slave);
    // and in no frame it drew did it say a frit draws nothing; the test
    // reads every byte the TUI wrote, so no frame is caught half drawn
    assert!(
        !String::from_utf8_lossy(&c.written()).contains("no source on this machine"),
        "{screen}"
    );
}

/// Now, for a wait bounded only so the test fails instead of hanging.
#[expect(
    clippy::disallowed_methods,
    reason = "the wait for the stand-in helper is bounded so the test fails instead of hanging"
)]
fn now() -> std::time::Instant {
    std::time::Instant::now()
}
