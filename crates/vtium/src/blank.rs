//! Blanking and waking from outside the session (the operator's decisions
//! on vtium/vtium#79): `SIGRTMIN+1` blanks every display at once and
//! `SIGRTMIN+2` wakes them as input does, so a script or a power daemon
//! acts without writing to the session's terminal. The subcommands
//! `vtium blank --vt N` and `vtium wake --vt N` find the vtium on VT N and
//! send it the signal.
//!
//! A signal reaches the seat through its loop ([`listen`]), and the seat
//! takes it at the turn's end ([`crate::app::App::order`]) through the
//! blank's own paths: the blank the timer runs, and the wake a program's
//! `CSI 13 ]` gives. `program-blank` does not gate it; that key gates the
//! programs on the terminal.
//!
//! An instance is found by what it holds: the vtium on VT N is the
//! process named `vtium` with `/dev/ttyN` open, which every vtium on a
//! kernel VT keeps open while it runs ([`find`]). It is held by its pidfd
//! and looked at again before the signal, so a process that took its pid
//! in between is never signalled. Reading a process's
//! descriptors and signalling it take the same right, root's or the user
//! vtium runs as, so the subcommand works for those two and no one else.

use std::ffi::OsString;
use std::io;
use std::path::Path;

use rustix::fs::{FileType, major, minor};
use vtium_platform::calloop::LoopHandle;
use vtium_platform::rt_signal::{Process, RtSignals};

use crate::app::App;

/// What a signal from outside asks of the blank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Every display powers down now.
    Blank,
    /// The displays wake, as at input.
    Wake,
}

impl Order {
    /// The signal's offset from `SIGRTMIN`.
    pub const fn offset(self) -> u8 {
        match self {
            Order::Blank => 1,
            Order::Wake => 2,
        }
    }

    fn from_offset(offset: u8) -> Option<Order> {
        [Order::Blank, Order::Wake]
            .into_iter()
            .find(|o| o.offset() == offset)
    }

    /// The subcommand's name.
    pub const fn word(self) -> &'static str {
        match self {
            Order::Blank => "blank",
            Order::Wake => "wake",
        }
    }
}

/// Takes `SIGRTMIN+1` and `SIGRTMIN+2` through the seat's loop, each
/// handed to [`App::order`]. Called with the loop's other signals, before
/// any thread starts. With both pending at once the kernel gives
/// `SIGRTMIN+1` first, so the wake holds.
pub fn listen(handle: &LoopHandle<'static, App>) -> io::Result<()> {
    let offsets = [Order::Blank.offset(), Order::Wake.offset()];
    let source = RtSignals::new(&offsets)?;
    handle
        .insert_source(source, |offset, _, app: &mut App| {
            if let Some(order) = Order::from_offset(offset) {
                app.order(order);
            }
        })
        .map_err(|e| e.error)?;
    Ok(())
}

/// A subcommand's request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub order: Order,
    /// The VT's number.
    pub vt: u16,
}

/// What a subcommand's words ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parsed {
    Run(Command),
    Help(Order),
}

/// The highest VT number the kernel has (`MAX_NR_CONSOLES`).
const LAST_VT: u16 = 63;

/// The subcommand `args` names, its words parsed: `None` when the first
/// word is neither `blank` nor `wake`. `--vt N` or `--vt=N` names the VT,
/// N its number or `ttyN`.
pub fn parse(args: &[OsString]) -> Option<Result<Parsed, String>> {
    let order = match args.first()?.to_str()? {
        "blank" => Order::Blank,
        "wake" => Order::Wake,
        _ => return None,
    };
    Some(parse_options(order, args.get(1..).unwrap_or_default()))
}

fn parse_options(order: Order, args: &[OsString]) -> Result<Parsed, String> {
    let mut vt = None;
    let mut words = args.iter();
    while let Some(word) = words.next() {
        let word = word
            .to_str()
            .ok_or_else(|| format!("{:?} is not UTF-8", word))?;
        let value = match word.split_once('=') {
            Some(("--vt", value)) => value,
            None if word == "--vt" => words
                .next()
                .and_then(|w| w.to_str())
                .ok_or("--vt needs a VT's number")?,
            None if word == "--help" || word == "-h" => return Ok(Parsed::Help(order)),
            _ => return Err(format!("unknown option {word}")),
        };
        if vt.is_some() {
            return Err("--vt is given twice".to_owned());
        }
        vt = Some(vt_number(value).ok_or_else(|| format!("{value} is no VT 1 to {LAST_VT}"))?);
    }
    let vt = vt.ok_or("--vt N names the VT")?;
    Ok(Parsed::Run(Command { order, vt }))
}

/// `N` or `ttyN`, for N from 1 to [`LAST_VT`].
fn vt_number(value: &str) -> Option<u16> {
    let digits = value.strip_prefix("tty").unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|n| (1..=LAST_VT).contains(n))
}

/// The usage line for `order`'s subcommand.
pub fn usage(order: Order) -> String {
    let (word, what) = match order {
        Order::Blank => ("blank", "blanks its displays now (SIGRTMIN+1)"),
        Order::Wake => ("wake", "wakes its displays (SIGRTMIN+2)"),
    };
    format!(
        "Usage: vtium {word} --vt N\nFinds the vtium on VT N and {what}. Root or the user vtium runs as may send it.\n"
    )
}

/// The vtium instances a lookup found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// The process ids of the vtium on the VT, one but in a race.
    pub pids: Vec<u32>,
    /// Processes named `vtium` whose descriptors cannot be read: another
    /// user's, to all but root.
    pub unreadable: usize,
}

/// The kernel's major number for the VTs, `/dev/ttyN` being minor N.
const TTY_MAJOR: u32 = 4;

/// Reads the character device a descriptor's link names, as its major
/// and minor numbers.
type Device = dyn Fn(&Path) -> Option<(u32, u32)>;

/// The character device `fd` names, through the link: what stat finds.
fn char_device(fd: &Path) -> Option<(u32, u32)> {
    let stat = rustix::fs::stat(fd).ok()?;
    (FileType::from_raw_mode(stat.st_mode) == FileType::CharacterDevice)
        .then(|| (major(stat.st_rdev), minor(stat.st_rdev)))
}

/// The vtium on VT `vt` among the processes under `proc` (`/proc`), but
/// `me`: a process named `vtium` holding VT `vt`'s device open past its
/// stdin, stdout and stderr, which may be another VT's, the one it was
/// started from. A forked child that has not yet run its command holds
/// the VT too, under the same name; a process whose parent is also found
/// is that child, and left out.
pub fn find(proc: &Path, vt: u16, me: u32) -> Found {
    find_with(proc, vt, me, &char_device)
}

fn find_with(proc: &Path, vt: u16, me: u32, device: &Device) -> Found {
    let mut found = Found::default();
    let mut holders: Vec<(u32, Option<u32>)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(proc) else {
        return found;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let dir = entry.path();
        if pid == me || !named_vtium(&dir) {
            continue;
        }
        match holds(&dir, vt, device) {
            Ok(true) => holders.push((pid, parent(&dir))),
            Ok(false) => {}
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                found.unreadable = found.unreadable.saturating_add(1);
            }
            // Gone since the listing.
            Err(_) => {}
        }
    }
    found.pids = holders
        .iter()
        .filter(|(_, ppid)| !ppid.is_some_and(|p| holders.iter().any(|(h, _)| *h == p)))
        .map(|(pid, _)| *pid)
        .collect();
    found.pids.sort_unstable();
    found
}

/// Whether the process at `dir` is named `vtium`.
fn named_vtium(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("comm"))
        .is_ok_and(|comm| comm.trim_end_matches('\n') == "vtium")
}

/// Whether the process at `dir` holds VT `vt` open on a descriptor past
/// stderr.
fn holds(dir: &Path, vt: u16, device: &Device) -> io::Result<bool> {
    let tty = (TTY_MAJOR, u32::from(vt));
    Ok(std::fs::read_dir(dir.join("fd"))?.flatten().any(|fd| {
        let past_stdio = fd
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| n > 2);
        past_stdio && device(&fd.path()) == Some(tty)
    }))
}

/// A process's parent, from its `stat`: the field after the state, which
/// follows the name's closing parenthesis.
fn parent(dir: &Path) -> Option<u32> {
    let stat = std::fs::read_to_string(dir.join("stat")).ok()?;
    let (_, rest) = stat.rsplit_once(')')?;
    rest.split_whitespace().nth(1)?.parse().ok()
}

/// Signals the process at `dir`, which a lookup found as the vtium on VT
/// `vt`: `open` holds it, by its pidfd, and `send` signals it once it is
/// still that vtium, so a process that took its pid since is never
/// signalled. Returns whether it was.
fn deliver<T>(
    dir: &Path,
    vt: u16,
    device: &Device,
    open: impl FnOnce() -> io::Result<T>,
    send: impl FnOnce(&T) -> io::Result<()>,
) -> io::Result<bool> {
    let held = open()?;
    if !(named_vtium(dir) && holds(dir, vt, device)?) {
        return Ok(false);
    }
    send(&held)?;
    Ok(true)
}

/// Runs a subcommand's request against the running system: finds the vtium
/// on the VT and signals it. Returns the exit status, 0 once every
/// instance found was signalled.
pub fn run(command: Command) -> i32 {
    let Command { order, vt } = command;
    let word = order.word();
    let proc = Path::new("/proc");
    let found = find(proc, vt, std::process::id());
    if found.pids.is_empty() {
        if found.unreadable > 0 {
            eprintln!(
                "vtium {word}: no vtium found on VT {vt}; another user's vtium cannot be read: run as root or as vtium's user"
            );
        } else {
            eprintln!("vtium {word}: no vtium runs on VT {vt}");
        }
        return 1;
    }
    let mut status = 0;
    for pid in found.pids {
        let sent = deliver(
            &proc.join(pid.to_string()),
            vt,
            &char_device,
            || Process::open(pid),
            |p| p.signal(order.offset()),
        );
        match sent {
            Ok(true) => {}
            Ok(false) => {
                eprintln!("vtium {word}: vtium {pid} left VT {vt} before the signal");
                status = 1;
            }
            Err(e) => {
                eprintln!("vtium {word}: cannot signal vtium {pid} on VT {vt}: {e}");
                status = 1;
            }
        }
    }
    status
}

/// The subcommand `args` names, run: `None` when it names none, so the
/// console starts. Its exit status otherwise: 2 for words it cannot take.
pub fn subcommand(args: &[OsString]) -> Option<i32> {
    Some(match parse(args)? {
        Ok(Parsed::Run(command)) => run(command),
        Ok(Parsed::Help(order)) => {
            print!("{}", usage(order));
            0
        }
        Err(e) => {
            let word = args
                .first()
                .map(|w| w.to_string_lossy().into_owned())
                .unwrap_or_default();
            eprint!("vtium {word}: {e}\n{}", parse_usage(&word));
            2
        }
    })
}

fn parse_usage(word: &str) -> String {
    usage(if word == "wake" {
        Order::Wake
    } else {
        Order::Blank
    })
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test's set-up fails loudly")]
mod tests {
    use std::cell::Cell;
    use std::path::PathBuf;

    use super::*;

    fn words(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn run_on(order: Order, vt: u16) -> Option<Result<Parsed, String>> {
        Some(Ok(Parsed::Run(Command { order, vt })))
    }

    #[test]
    fn blank_and_wake_take_the_vt_in_either_form() {
        assert_eq!(
            parse(&words(&["blank", "--vt", "5"])),
            run_on(Order::Blank, 5)
        );
        assert_eq!(parse(&words(&["wake", "--vt=5"])), run_on(Order::Wake, 5));
        assert_eq!(
            parse(&words(&["wake", "--vt", "tty12"])),
            run_on(Order::Wake, 12)
        );
        assert_eq!(
            parse(&words(&["blank", "--vt=63"])),
            run_on(Order::Blank, 63)
        );
        assert_eq!(
            parse(&words(&["wake", "--help"])),
            Some(Ok(Parsed::Help(Order::Wake)))
        );
    }

    #[test]
    fn other_words_are_no_subcommand_of_these() {
        assert_eq!(parse(&words(&[])), None);
        assert_eq!(parse(&words(&["check-config"])), None);
        assert_eq!(parse(&words(&["--vt=5", "blank"])), None);
        assert_eq!(parse(&words(&["--login", "--", "blank"])), None);
    }

    #[test]
    fn a_vt_missing_out_of_range_or_twice_is_refused() {
        for args in [
            &["blank"][..],
            &["blank", "--vt"],
            &["blank", "--vt", "0"],
            &["blank", "--vt", "64"],
            &["blank", "--vt=+5"],
            &["blank", "--vt=tty"],
            &["blank", "--vt=/dev/tty5"],
            &["blank", "--vt=5", "--vt=6"],
            &["wake", "--vt=5", "--now"],
            &["wake", "5"],
        ] {
            assert!(
                matches!(parse(&words(args)), Some(Err(_))),
                "{args:?} is refused"
            );
        }
    }

    #[test]
    fn a_word_it_cannot_take_exits_2_and_help_0() {
        assert_eq!(subcommand(&words(&["wake", "--vt=99"])), Some(2));
        assert_eq!(subcommand(&words(&["blank", "--help"])), Some(0));
        assert_eq!(subcommand(&words(&["--help"])), None);
    }

    #[test]
    fn the_signals_are_rtmin_plus_1_and_2() {
        assert_eq!(Order::Blank.offset(), 1);
        assert_eq!(Order::Wake.offset(), 2);
        assert_eq!(Order::from_offset(1), Some(Order::Blank));
        assert_eq!(Order::from_offset(2), Some(Order::Wake));
        assert_eq!(Order::from_offset(4), None);
    }

    /// A planted descriptor's device: `/dev/ttyN` is VT N's, (4, N), and
    /// `/dev/vc-N` stands for another name of the same device.
    fn planted(fd: &Path) -> Option<(u32, u32)> {
        let to = std::fs::read_link(fd).ok()?;
        let to = to.to_str()?;
        let n = to
            .strip_prefix("/dev/tty")
            .or_else(|| to.strip_prefix("/dev/vc-"))?;
        Some((TTY_MAJOR, n.parse().ok()?))
    }

    /// A `/proc` planted in a scratch directory.
    struct Proc(PathBuf);

    impl Proc {
        fn new(name: &str) -> Proc {
            let dir =
                std::env::temp_dir().join(format!("vtium-blank-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Proc(dir)
        }

        /// A process `pid` named `comm`, child of `ppid`, with each of
        /// `fds` open from descriptor 3.
        fn process(&self, pid: u32, ppid: u32, comm: &str, fds: &[&str]) -> &Proc {
            let dir = self.0.join(pid.to_string());
            std::fs::create_dir_all(dir.join("fd")).unwrap();
            std::fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
            std::fs::write(
                dir.join("stat"),
                format!("{pid} ({comm}) S {ppid} {pid} {pid} 0 -1 4194560"),
            )
            .unwrap();
            for (n, to) in (3..).zip(fds) {
                std::os::unix::fs::symlink(to, dir.join("fd").join(n.to_string())).unwrap();
            }
            self
        }

        /// Process `pid`'s stdin, stdout and stderr on `to`.
        fn stdio(&self, pid: u32, to: &str) -> &Proc {
            for n in 0..3 {
                let fd = self.0.join(pid.to_string()).join("fd").join(n.to_string());
                std::os::unix::fs::symlink(to, fd).unwrap();
            }
            self
        }

        fn find(&self, vt: u16, me: u32) -> Found {
            find_with(&self.0, vt, me, &planted)
        }

        fn dir(&self, pid: u32) -> PathBuf {
            self.0.join(pid.to_string())
        }
    }

    impl Drop for Proc {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_vtium_holding_the_vt_is_found() {
        let p = Proc::new("found");
        p.process(
            100,
            1,
            "vtium",
            &["/dev/null", "/dev/tty5", "/dev/dri/card0"],
        )
        .process(200, 1, "vtium", &["/dev/tty6"])
        // Another program holding the VT is not vtium.
        .process(300, 1, "agetty", &["/dev/tty5"])
        // A name that only starts like it.
        .process(400, 1, "vtium-vitrify", &["/dev/tty5"])
        // The VT's number as a prefix of another's.
        .process(500, 1, "vtium", &["/dev/tty55"]);
        std::fs::write(p.0.join("self"), "").unwrap();
        assert_eq!(p.find(5, 1).pids, [100]);
        assert_eq!(p.find(6, 1).pids, [200]);
        assert_eq!(p.find(55, 1).pids, [500]);
        assert_eq!(p.find(7, 1), Found::default());
    }

    /// A vtium started from VT 5's login to run on VT 7 has VT 5 as its
    /// stdin, stdout and stderr: it is the vtium on VT 7 alone.
    #[test]
    fn a_vt_on_stdio_is_not_the_vt_held() {
        let p = Proc::new("stdio");
        p.process(100, 1, "vtium", &["/dev/tty7"])
            .stdio(100, "/dev/tty5");
        assert_eq!(p.find(5, 1), Found::default());
        assert_eq!(p.find(7, 1).pids, [100]);
    }

    /// The VT is matched by its device's number, under any name.
    #[test]
    fn the_vt_is_its_device_number() {
        let p = Proc::new("number");
        p.process(100, 1, "vtium", &["/dev/vc-5"]);
        assert_eq!(p.find(5, 1).pids, [100]);
    }

    /// The device is read through the link as stat reads it: a character
    /// device's numbers, and nothing for any other file.
    #[test]
    fn a_descriptors_device_is_its_character_devices_numbers() {
        let p = Proc::new("device");
        std::fs::create_dir_all(&p.0).unwrap();
        let null = p.0.join("null");
        std::os::unix::fs::symlink("/dev/null", &null).unwrap();
        assert_eq!(char_device(&null), Some((1, 3)));
        let file = p.0.join("file");
        std::fs::write(&file, "").unwrap();
        assert_eq!(char_device(&file), None);
        assert_eq!(char_device(&p.0.join("gone")), None);
    }

    /// A pid found, held by its pidfd, and no longer the vtium on the VT,
    /// as when another process took the pid in between, is not signalled.
    #[test]
    fn a_pid_no_longer_the_vtium_is_not_signalled() {
        let p = Proc::new("reuse");
        p.process(100, 1, "vtium", &["/dev/tty5"]);
        let sent = Cell::new(false);
        assert!(
            deliver_to(&p, 100, &|| {}, &sent),
            "still the vtium on VT 5"
        );
        assert!(sent.take());
        let renamed = || std::fs::write(p.dir(100).join("comm"), "bash\n").unwrap();
        assert!(!deliver_to(&p, 100, &renamed, &sent));
        assert!(!sent.get(), "another program took the pid");
        p.process(200, 1, "vtium", &["/dev/tty5"]);
        let moved = || std::fs::remove_file(p.dir(200).join("fd/3")).unwrap();
        assert!(!deliver_to(&p, 200, &moved, &sent));
        assert!(!sent.get(), "a vtium that no longer holds VT 5");
    }

    fn deliver_to(p: &Proc, pid: u32, between: &dyn Fn(), sent: &Cell<bool>) -> bool {
        deliver(
            &p.dir(pid),
            5,
            &planted,
            || {
                between();
                Ok(())
            },
            |()| {
                sent.set(true);
                Ok(())
            },
        )
        .unwrap()
    }

    #[test]
    fn the_subcommand_itself_and_a_forked_child_are_left_out() {
        let p = Proc::new("self");
        p.process(100, 1, "vtium", &["/dev/tty5"])
            // The login child between its fork and its exec.
            .process(101, 100, "vtium", &["/dev/tty5"])
            // `vtium blank --vt 5` run from VT 5's own getty.
            .process(900, 1, "vtium", &["/dev/tty5"]);
        assert_eq!(p.find(5, 900).pids, [100]);
    }

    #[test]
    fn a_vtium_whose_descriptors_cannot_be_read_is_counted() {
        let p = Proc::new("unreadable");
        p.process(100, 1, "vtium", &["/dev/tty5"]);
        std::fs::create_dir_all(p.0.join("200")).unwrap();
        std::fs::write(p.0.join("200/comm"), "vtium\n").unwrap();
        // Root reads every directory; the planted refusal needs a user.
        let fd = p.0.join("200/fd");
        std::fs::create_dir_all(&fd).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fd, std::fs::Permissions::from_mode(0o000)).unwrap();
        let readable = std::fs::read_dir(&fd).is_ok();
        let found = p.find(5, 1);
        std::fs::set_permissions(&fd, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(found.pids, [100]);
        if !readable {
            assert_eq!(found.unreadable, 1);
        }
    }
}
