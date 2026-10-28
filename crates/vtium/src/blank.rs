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
//! kernel VT keeps open while it runs ([`find`]). Reading a process's
//! descriptors and signalling it take the same right, root's or the user
//! vtium runs as, so the subcommand works for those two and no one else.

use std::ffi::OsString;
use std::io;
use std::path::Path;

use vtium_platform::calloop::LoopHandle;
use vtium_platform::rt_signal::{self, RtSignals};

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

/// The vtium on VT `vt` among the processes under `proc` (`/proc`), but
/// `me`: a process named `vtium` with `/dev/tty<vt>` open. A forked child
/// that has not yet run its command holds the VT too, under the same
/// name; a process whose parent is also found is that child, and left
/// out.
pub fn find(proc: &Path, vt: u16, me: u32) -> Found {
    let device = format!("/dev/tty{vt}");
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
        let named = std::fs::read_to_string(dir.join("comm"))
            .is_ok_and(|comm| comm.trim_end_matches('\n') == "vtium");
        if pid == me || !named {
            continue;
        }
        let fds = match std::fs::read_dir(dir.join("fd")) {
            Ok(fds) => fds,
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                found.unreadable = found.unreadable.saturating_add(1);
                continue;
            }
            // Gone since the listing.
            Err(_) => continue,
        };
        let holds = fds
            .flatten()
            .any(|fd| std::fs::read_link(fd.path()).is_ok_and(|to| to == Path::new(&device)));
        if holds {
            holders.push((pid, parent(&dir)));
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

/// A process's parent, from its `stat`: the field after the state, which
/// follows the name's closing parenthesis.
fn parent(dir: &Path) -> Option<u32> {
    let stat = std::fs::read_to_string(dir.join("stat")).ok()?;
    let (_, rest) = stat.rsplit_once(')')?;
    rest.split_whitespace().nth(1)?.parse().ok()
}

/// Runs a subcommand's request against the running system: finds the vtium
/// on the VT and signals it. Returns the exit status, 0 once every
/// instance found was signalled.
pub fn run(command: Command) -> i32 {
    let Command { order, vt } = command;
    let word = order.word();
    let found = find(Path::new("/proc"), vt, std::process::id());
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
        if let Err(e) = rt_signal::send(pid, order.offset()) {
            eprintln!("vtium {word}: cannot signal vtium {pid} on VT {vt}: {e}");
            status = 1;
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
        /// `fds` open.
        fn process(&self, pid: u32, ppid: u32, comm: &str, fds: &[&str]) -> &Proc {
            let dir = self.0.join(pid.to_string());
            std::fs::create_dir_all(dir.join("fd")).unwrap();
            std::fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
            std::fs::write(
                dir.join("stat"),
                format!("{pid} ({comm}) S {ppid} {pid} {pid} 0 -1 4194560"),
            )
            .unwrap();
            for (n, to) in fds.iter().enumerate() {
                std::os::unix::fs::symlink(to, dir.join("fd").join(n.to_string())).unwrap();
            }
            self
        }

        fn find(&self, vt: u16, me: u32) -> Found {
            find(&self.0, vt, me)
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
