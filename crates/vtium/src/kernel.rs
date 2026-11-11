//! vtium and the kernel: the switch on the kernel command line that turns
//! vtium off for one boot (ADR-1401 D9), and the kernel's messages, which
//! vtium shows on screen itself while its VT is in graphics mode
//! (ADR-1401 D15).
//!
//! vtium reads the messages from `/dev/kmsg`, the kernel's log as a
//! device. Each read of it returns one record, which the kernel's
//! documentation `Documentation/ABI/testing/dev-kmsg` describes: a
//! prefix of comma-separated fields, a `;`, the message, and a newline,
//! then lines of dictionary entries that each start with a space. The
//! prefix's first field is the record's priority, the facility times 8
//! plus the level, as syslog(3) numbers them. vtium opens the device at
//! its end, so it reads only the messages that arrive after it starts.
//!
//! vtium shows a message from the kernel itself, the facility 0, whose
//! level is warning (4) or more severe, and only when the kernel's
//! console log level lets the level through, as the kernel's own console
//! does: the kernel prints a message on its console when the message's
//! level is lower than the console log level (syslog(2), "/proc/sys/
//! kernel/printk"). That level is the first field of
//! `/proc/sys/kernel/printk`, and `quiet` or `loglevel=` on the kernel
//! command line set it.
//!
//! Plymouth is the boot splash program. vtium tells that it runs by its
//! pid file, `/run/plymouth/pid` ([`PLYMOUTH_PID`]). A pid file that
//! outlives plymouthd does not count: vtium checks that the process it
//! names is still running and is called `plymouthd`. This needs no
//! plymouth binary, so it works in the minspec build.

use std::ffi::OsString;
use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use crate::config::{Config, keys};

/// The kernel command line.
pub const CMDLINE: &str = "/proc/cmdline";

/// The key on the kernel command line that turns vtium off for one boot.
pub const ENABLE: &str = "vtium.enable";

/// The kernel's log, as a device.
pub const KMSG: &str = "/dev/kmsg";

/// The file whose first field is the kernel's console log level.
pub const PRINTK: &str = "/proc/sys/kernel/printk";

/// The file where plymouthd writes its process id. Arch's plymouth
/// package starts plymouthd from
/// `/usr/lib/systemd/system/plymouth-start.service`, whose line
/// `ExecStart=/usr/bin/plymouthd --mode=boot --pid-file=/run/plymouth/pid --attach-to-session`
/// names this file.
pub const PLYMOUTH_PID: &str = "/run/plymouth/pid";

/// The kernel's facility, as syslog(3) numbers it.
const KERNEL_FACILITY: u8 = 0;

/// The least severe level vtium shows: warning, as syslog(3) numbers it.
const WARNING: u8 = 4;

/// The console log level vtium uses when it cannot read the kernel's,
/// the kernel's own default, `CONFIG_CONSOLE_LOGLEVEL_DEFAULT`'s 7.
const CONSOLE_LEVEL_DEFAULT: u8 = 7;

/// The most reads vtium makes of `/dev/kmsg` in one turn. The loop
/// watches the device level-triggered, so the records left over are read
/// in the next turn.
const RECORDS_PER_TURN: usize = 256;

/// The longest message vtium shows, in characters. A longer one ends in
/// an ellipsis.
pub const MESSAGE_CHARS_MAX: usize = 240;

/// Whether the kernel command line `cmdline` turns vtium off for this
/// boot: its last `vtium.enable=` says `0`. Any other value, and no
/// `vtium.enable=` at all, leave vtium on.
pub fn turned_off(cmdline: &str) -> bool {
    params(cmdline)
        .into_iter()
        .rev()
        .find_map(|(name, value)| (name == ENABLE).then_some(value))
        .flatten()
        == Some("0")
}

/// The parameters of the kernel command line `cmdline`, each its name and
/// its value, if it has one, split as the kernel splits them: `next_arg()`
/// in the kernel's `kernel/params.c`, which `parse_args()` calls. That is
/// a reading of the kernel source from memory, since this machine has no
/// kernel source. Whitespace ends a parameter outside double quotes, so a
/// quoted value with spaces stays one parameter. Quotes around the whole
/// parameter, or around its value after the `=`, are stripped.
fn params(cmdline: &str) -> Vec<(&str, Option<&str>)> {
    let mut words = Vec::new();
    let mut start = None;
    let mut quoted = false;
    for (i, c) in cmdline.char_indices() {
        if c == '"' {
            quoted = !quoted;
        }
        match (start, c.is_whitespace() && !quoted) {
            (None, false) => start = Some(i),
            (Some(from), true) => {
                words.push(&cmdline[from..i]);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        words.push(&cmdline[from..]);
    }
    words
        .into_iter()
        .map(|word| {
            let word = unquote(word);
            match word.split_once('=') {
                Some((name, value)) => (name, Some(unquote(value))),
                None => (word, None),
            }
        })
        .collect()
}

/// `v` without the double quotes around it; a lone opening quote goes too,
/// as the kernel drops it.
fn unquote(v: &str) -> &str {
    v.strip_prefix('"')
        .map(|v| v.strip_suffix('"').unwrap_or(v))
        .unwrap_or(v)
}

/// Whether `args` start vtium as a console. The commands that only read
/// or report, `check-config`, `--help` and `--version`, run whatever the
/// kernel command line says, so that a person can still check vtium.conf
/// on a boot that turns vtium off. The words after `--` are the login
/// command's, so they never count.
pub fn runs_console(args: &[OsString]) -> bool {
    !args.iter().take_while(|a| *a != "--").any(|a| {
        ["check-config", "--help", "-h", "--version", "-V"].contains(&a.to_str().unwrap_or(""))
    })
}

/// The start's first check (ADR-1401 D9): when the kernel command line at
/// `cmdline` turns vtium off for this boot, vtium logs one line that names
/// the key and returns the exit status 1. It runs before vtium reads
/// vtium.conf and before it opens its VT or a card. The binary reads
/// [`CMDLINE`]; a test reads a stand-in through the same function.
pub fn off_at_start(args: &[OsString], cmdline: &Path) -> Option<u8> {
    if !runs_console(args) || !turned_off(&crate::font::read_text(cmdline)) {
        return None;
    }
    log::warn!(
        "vtium is turned off for this boot: the kernel command line sets {ENABLE}=0. Remove {ENABLE}=0 from the kernel command line to start vtium again."
    );
    Some(1)
}

/// One record of `/dev/kmsg`: who logged it, how severe it is, and its
/// message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub facility: u8,
    pub level: u8,
    pub text: String,
}

/// The record that the bytes of one read of `/dev/kmsg` hold, or `None`
/// when they are not a record. The message ends at its first newline, and
/// the dictionary lines after it are left out.
///
/// The kernel writes each byte of a message below 0x20, from 0x7f up, and
/// `\` itself as `\xNN` (the kernel's `Documentation/ABI/testing/dev-kmsg`).
/// vtium decodes those escapes back to bytes and reads the bytes as UTF-8,
/// with U+FFFD for a byte that is not UTF-8. A newline or a tab in the
/// message shows as a space, so each message stays one line of the box,
/// and vtium leaves out every other control character.
pub fn record(raw: &[u8]) -> Option<Record> {
    let semicolon = raw.iter().position(|&b| b == b';')?;
    let (prefix, rest) = raw.split_at(semicolon);
    let prefix = std::str::from_utf8(prefix).ok()?;
    let priority: u16 = prefix.split(',').next()?.trim().parse().ok()?;
    let message = rest.get(1..).unwrap_or_default();
    let message = message.split(|&b| b == b'\n').next().unwrap_or_default();
    let text: String = String::from_utf8_lossy(&unescaped(message))
        .chars()
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .filter(|c| !c.is_control())
        .collect();
    Some(Record {
        facility: u8::try_from(priority >> 3).ok()?,
        level: u8::try_from(priority & 7).ok()?,
        text,
    })
}

/// `message` with each `\xNN` escape the kernel wrote replaced by its
/// byte. A `\x` that two hex digits do not follow stays as it is.
fn unescaped(message: &[u8]) -> Vec<u8> {
    let hex = |b: u8| {
        char::from(b)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    };
    let mut out = Vec::with_capacity(message.len());
    let mut i = 0;
    while let Some(&b) = message.get(i) {
        let escape = match message.get(i..i.saturating_add(4)) {
            Some([b'\\', b'x', h, l]) => hex(*h).zip(hex(*l)),
            _ => None,
        };
        match escape {
            Some((h, l)) => {
                out.push(h.wrapping_mul(16).wrapping_add(l));
                i = i.saturating_add(4);
            }
            None => {
                out.push(b);
                i = i.saturating_add(1);
            }
        }
    }
    out
}

/// Whether vtium shows `record` while the kernel's console log level is
/// `console_level` (ADR-1401 D15): a message from the kernel itself, of
/// level warning or more severe, that the console log level lets through.
pub fn shown(record: &Record, console_level: u8) -> bool {
    record.facility == KERNEL_FACILITY && record.level <= WARNING && record.level < console_level
}

/// The kernel's console log level, the first field of the text of
/// `/proc/sys/kernel/printk`; the kernel's default where the text holds
/// no number.
pub fn console_level(printk: &str) -> u8 {
    printk
        .split_whitespace()
        .next()
        .and_then(|f| f.parse().ok())
        .unwrap_or(CONSOLE_LEVEL_DEFAULT)
}

/// Whether plymouthd runs, as its pid file `pid_file` and the process
/// directory `proc` say: the file names a process that is still running
/// and is called `plymouthd`.
pub fn plymouth_runs(pid_file: &Path, proc: &Path) -> bool {
    let Ok(pid) = std::fs::read_to_string(pid_file) else {
        return false;
    };
    let Ok(pid) = pid.trim().parse::<u32>() else {
        return false;
    };
    std::fs::read_to_string(proc.join(pid.to_string()).join("comm"))
        .is_ok_and(|comm| comm.trim_end() == "plymouthd")
}

/// Who shows the kernel's messages while Plymouth runs: vtium.conf's
/// `kernel-messages` (ADR-1401 D15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deferral {
    /// `after-plymouth`, the default: Plymouth shows them while it runs,
    /// and vtium once it has quit.
    AfterPlymouth,
    /// `always`: vtium shows them whether Plymouth runs or not.
    Always,
}

impl Deferral {
    /// The deferral `config` sets.
    pub fn configured(config: &Config) -> Deferral {
        match config.text(&keys::KERNEL_MESSAGES) {
            Some("always") => Deferral::Always,
            _ => Deferral::AfterPlymouth,
        }
    }
}

/// Where the kernel's messages come from, and what vtium reads beside
/// them: the device in the binary, a stand-in in a test.
pub trait Kernel {
    /// The records that arrived since the last call, each as one read of
    /// `/dev/kmsg` returned it. An error means that vtium cannot read the
    /// kernel's messages any more.
    fn drain(&mut self) -> io::Result<Vec<Vec<u8>>>;
    /// The kernel's console log level now.
    fn console_level(&self) -> u8;
    /// Whether Plymouth runs now.
    fn plymouth_runs(&self) -> bool;
}

/// The kernel's messages as vtium shows them: read from the kernel,
/// filtered by level, and held back while Plymouth shows them.
pub struct Messages {
    kernel: Box<dyn Kernel>,
    deferral: Deferral,
}

impl Messages {
    pub fn new(kernel: Box<dyn Kernel>, deferral: Deferral) -> Messages {
        Messages { kernel, deferral }
    }

    /// The messages that arrived since the last call and that vtium
    /// shows now, oldest first. Under `after-plymouth`, while Plymouth
    /// runs, vtium reads the messages and shows none of them, so a message
    /// that came while Plymouth ran is never shown later. The error is
    /// why vtium cannot read the kernel's messages any more.
    pub fn take(&mut self) -> io::Result<Vec<String>> {
        let raw = self.kernel.drain()?;
        if raw.is_empty() || self.deferral == Deferral::AfterPlymouth && self.kernel.plymouth_runs()
        {
            return Ok(Vec::new());
        }
        let level = self.kernel.console_level();
        Ok(raw
            .iter()
            .filter_map(|r| record(r))
            .filter(|r| shown(r, level))
            .map(|r| clipped(&r.text))
            .filter(|t| !t.is_empty())
            .collect())
    }
}

/// `text` at most [`MESSAGE_CHARS_MAX`] characters long.
fn clipped(text: &str) -> String {
    if text.chars().count() <= MESSAGE_CHARS_MAX {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(MESSAGE_CHARS_MAX - 1).collect();
    out.push('…');
    out
}

/// The records that `read` gives, as `/dev/kmsg` gives them: one record
/// a read, at most [`RECORDS_PER_TURN`] reads. `EPIPE` says the kernel
/// overwrote records vtium had not read yet, and the next read returns the
/// oldest record it still keeps, so reading goes on, as after `EINTR`.
/// `EAGAIN` says no record waits, which ends the batch. Another error ends
/// the batch with the records read so far, or, with none, is the error.
fn drain_with(
    mut read: impl FnMut(&mut [u8]) -> rustix::io::Result<usize>,
) -> io::Result<Vec<Vec<u8>>> {
    use rustix::io::Errno;
    let mut out = Vec::new();
    // The kernel refuses a read into a buffer smaller than the record,
    // and a record is at most 8 KiB with its dictionary.
    let mut buf = vec![0u8; 8192];
    for _ in 0..RECORDS_PER_TURN {
        match read(&mut buf) {
            Ok(0) => break,
            Ok(n) => out.push(buf.get(..n).unwrap_or_default().to_vec()),
            Err(Errno::PIPE | Errno::INTR) => {}
            Err(Errno::AGAIN) => break,
            // the next call meets the error again
            Err(_) if !out.is_empty() => break,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(out)
}

/// The kernel as the binary reads it: `/dev/kmsg`, the console log level
/// in `/proc/sys/kernel/printk`, and plymouthd's pid file.
pub struct System {
    kmsg: OwnedFd,
    printk: PathBuf,
    plymouth: PathBuf,
    proc: PathBuf,
}

impl System {
    /// Opens `/dev/kmsg` at its end, so that vtium reads only the messages
    /// that arrive from now on.
    pub fn open() -> io::Result<System> {
        use rustix::fs::{Mode, OFlags, SeekFrom};
        let flags = OFlags::RDONLY | OFlags::NOCTTY | OFlags::NONBLOCK | OFlags::CLOEXEC;
        let kmsg = rustix::fs::open(KMSG, flags, Mode::empty())?;
        rustix::fs::seek(&kmsg, SeekFrom::End(0))?;
        Ok(System {
            kmsg,
            printk: PathBuf::from(PRINTK),
            plymouth: PathBuf::from(PLYMOUTH_PID),
            proc: PathBuf::from("/proc"),
        })
    }

    /// The descriptor the loop watches: readable while a record waits.
    pub fn watched(&self) -> io::Result<OwnedFd> {
        self.kmsg.try_clone()
    }
}

impl Kernel for System {
    fn drain(&mut self) -> io::Result<Vec<Vec<u8>>> {
        let kmsg = &self.kmsg;
        drain_with(|buf| rustix::io::read(kmsg, buf))
    }

    fn console_level(&self) -> u8 {
        console_level(&std::fs::read_to_string(&self.printk).unwrap_or_default())
    }

    fn plymouth_runs(&self) -> bool {
        plymouth_runs(&self.plymouth, &self.proc)
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test's set-up fails loudly")]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    /// A directory of the test's own.
    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vtium-kernel-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ADR-1401 D9: `vtium.enable=0` turns vtium off; the last one on the
    // line wins, and any other value or none leaves vtium on.
    #[test]
    fn vtium_enable_0_alone_turns_vtium_off() {
        for (line, off) in [
            (
                "BOOT_IMAGE=/vmlinuz root=/dev/sda1 vtium.enable=0 quiet\n",
                true,
            ),
            ("vtium.enable=0", true),
            ("vtium.enable=1 vtium.enable=0", true),
            ("vtium.enable=0 vtium.enable=1", false),
            ("vtium.enable=1", false),
            ("vtium.enable=no", false),
            ("vtium.enable=00", false),
            ("vtium.enable=", false),
            ("vtium.enable", false),
            ("xvtium.enable=0", false),
            ("vtium.enable=\"0\"", true),
            ("\"vtium.enable=0\"", true),
            ("root=/dev/sda1 foo=\"a vtium.enable=0\" quiet", false),
            ("foo=\"a b\" vtium.enable=0", true),
            ("root=/dev/sda1 quiet", false),
            ("", false),
        ] {
            assert_eq!(turned_off(line), off, "{line:?}");
        }
    }

    // ADR-1401 D9: with `vtium.enable=0` on the command line that the start
    // reads, the start ends with status 1 before anything else; the commands
    // that only check or report still run, and without the key the start
    // goes on.
    #[test]
    fn the_start_exits_1_on_a_command_line_that_turns_vtium_off() {
        let dir = scratch("off_at_start");
        let off = dir.join("cmdline-off");
        std::fs::write(&off, "root=/dev/sda1 vtium.enable=0 quiet\n").unwrap();
        let on = dir.join("cmdline-on");
        std::fs::write(&on, "root=/dev/sda1 quiet\n").unwrap();
        assert_eq!(off_at_start(&args(&[]), &off), Some(1));
        assert_eq!(off_at_start(&args(&["--vt=/dev/tty3"]), &off), Some(1));
        assert_eq!(
            off_at_start(&args(&["--login", "--", "/bin/sh", "--help"]), &off),
            Some(1),
            "the login command's words do not count"
        );
        for command in [&["check-config"][..], &["--help"], &["--version"]] {
            assert_eq!(off_at_start(&args(command), &off), None, "{command:?}");
        }
        assert_eq!(off_at_start(&args(&[]), &on), None);
        assert_eq!(off_at_start(&args(&[]), &dir.join("none")), None);
    }

    #[test]
    fn a_record_is_its_priority_split_and_its_message_alone() {
        assert_eq!(
            record(b"4,1203,9135481,-;usb 1-2: device descriptor read/64, error -71\n SUBSYSTEM=usb\n DEVICE=c189:1\n"),
            Some(Record {
                facility: 0,
                level: 4,
                text: "usb 1-2: device descriptor read/64, error -71".to_owned(),
            })
        );
        assert_eq!(
            record(b"30,1204,9135490,-;systemd[1]: Started x.\n"),
            Some(Record {
                facility: 3,
                level: 6,
                text: "systemd[1]: Started x.".to_owned(),
            })
        );
        assert_eq!(
            record(b"3,1,2,-;a\x1bb\tc\n").map(|r| r.text),
            Some("ab c".to_owned()),
            "a tab shows as a space, and an escape character goes"
        );
        // the kernel's escapes are decoded: a newline shows as a space, and
        // UTF-8 reads as its characters
        assert_eq!(
            record(b"4,1,2,-;line one\\x0aline two\n").map(|r| r.text),
            Some("line one line two".to_owned())
        );
        assert_eq!(
            record(b"4,1,2,-;usb: Aaron\\xe2\\x80\\x99s keyboard\n").map(|r| r.text),
            Some("usb: Aaron\u{2019}s keyboard".to_owned())
        );
        assert_eq!(
            record(b"4,1,2,-;a \\x5c b \\x1b c \\xff d \\xZZ\n").map(|r| r.text),
            Some("a \\ b  c \u{fffd} d \\xZZ".to_owned()),
            "a backslash, a control character gone, a byte that is no UTF-8, and no escape"
        );
        assert_eq!(record(b"no prefix\n"), None);
        assert_eq!(record(b"x,1,2,-;text\n"), None);
    }

    // /dev/kmsg's errors (Documentation/ABI/testing/dev-kmsg): after
    // EPIPE, records were overwritten and reading goes on; EAGAIN ends the
    // batch, and the record after it waits for the next one; another error
    // with nothing read is the error.
    #[test]
    fn draining_reads_on_after_epipe_and_stops_at_eagain() {
        use rustix::io::Errno;
        let script = std::cell::RefCell::new(std::collections::VecDeque::from([
            Ok(&b"4,1,0,-;one\n"[..]),
            Err(Errno::PIPE),
            Ok(&b"4,9,0,-;two\n"[..]),
            Err(Errno::AGAIN),
            Ok(&b"4,10,0,-;three\n"[..]),
            Err(Errno::AGAIN),
            Err(Errno::IO),
        ]));
        let mut read = |buf: &mut [u8]| match script.borrow_mut().pop_front() {
            Some(Ok(record)) => {
                buf[..record.len()].copy_from_slice(record);
                Ok(record.len())
            }
            Some(Err(e)) => Err(e),
            None => Err(Errno::AGAIN),
        };
        let texts = |batch: Vec<Vec<u8>>| -> Vec<String> {
            batch
                .iter()
                .filter_map(|r| record(r))
                .map(|r| r.text)
                .collect()
        };
        assert_eq!(texts(drain_with(&mut read).unwrap()), ["one", "two"]);
        assert_eq!(texts(drain_with(&mut read).unwrap()), ["three"]);
        assert!(drain_with(&mut read).is_err(), "EIO with nothing read");
    }

    #[test]
    fn the_console_level_is_printks_first_field() {
        assert_eq!(console_level("3\t4\t1\t4\n"), 3);
        assert_eq!(console_level("7 4 1 7"), 7);
        assert_eq!(console_level(""), CONSOLE_LEVEL_DEFAULT);
    }

    /// A stand-in for the kernel: the records queued, the console log level
    /// and whether Plymouth runs, each shared with the test.
    #[derive(Clone, Default)]
    struct StandIn {
        queued: Rc<RefCell<Vec<Vec<u8>>>>,
        level: Rc<RefCell<u8>>,
        plymouth: Rc<RefCell<bool>>,
    }

    impl StandIn {
        fn log(&self, priority: u8, text: &str) {
            self.queued
                .borrow_mut()
                .push(format!("{priority},1,2,-;{text}\n").into_bytes());
        }
    }

    impl Kernel for StandIn {
        fn drain(&mut self) -> io::Result<Vec<Vec<u8>>> {
            Ok(std::mem::take(&mut *self.queued.borrow_mut()))
        }
        fn console_level(&self) -> u8 {
            *self.level.borrow()
        }
        fn plymouth_runs(&self) -> bool {
            *self.plymouth.borrow()
        }
    }

    fn messages(deferral: Deferral) -> (StandIn, Messages) {
        let kernel = StandIn::default();
        *kernel.level.borrow_mut() = 7;
        (kernel.clone(), Messages::new(Box::new(kernel), deferral))
    }

    // ADR-1401 D15: vtium shows the kernel's own warnings and worse, never
    // its info, its notices or another facility's warnings.
    #[test]
    fn warnings_from_the_kernel_show_and_info_does_not() {
        let (kernel, mut m) = messages(Deferral::Always);
        kernel.log(0, "Kernel panic - not syncing");
        kernel.log(2, "critical");
        kernel.log(3, "error");
        kernel.log(4, "warning");
        kernel.log(5, "notice");
        kernel.log(6, "info");
        kernel.log(7, "debug");
        // facility 3, daemon, at warning
        kernel.log(3 * 8 + 4, "a daemon's warning");
        assert_eq!(
            m.take().unwrap(),
            ["Kernel panic - not syncing", "critical", "error", "warning"]
        );
        assert_eq!(m.take().unwrap(), Vec::<String>::new(), "each message once");
    }

    // ADR-1401 D15: the console log level hides what it hides from the
    // kernel's console: at 3, as `loglevel=3` sets it, warnings and errors
    // stay off the screen.
    #[test]
    fn a_console_log_level_of_3_hides_warnings() {
        let (kernel, mut m) = messages(Deferral::Always);
        *kernel.level.borrow_mut() = 3;
        kernel.log(2, "critical");
        kernel.log(3, "error");
        kernel.log(4, "warning");
        assert_eq!(m.take().unwrap(), ["critical"]);
        *kernel.level.borrow_mut() = 5;
        kernel.log(4, "warning");
        assert_eq!(
            m.take().unwrap(),
            ["warning"],
            "the level is read again for each batch"
        );
    }

    // ADR-1401 D15: under `after-plymouth`, vtium shows nothing while
    // Plymouth runs, and once Plymouth has quit it shows the messages that
    // come after, never the ones that came while it ran. Under `always` it
    // shows them while Plymouth runs.
    #[test]
    fn after_plymouth_holds_the_messages_back_while_plymouth_runs_with_no_replay() {
        let (kernel, mut m) = messages(Deferral::AfterPlymouth);
        kernel.log(4, "before Plymouth");
        assert_eq!(m.take().unwrap(), ["before Plymouth"]);
        *kernel.plymouth.borrow_mut() = true;
        kernel.log(3, "while Plymouth runs");
        assert_eq!(m.take().unwrap(), Vec::<String>::new());
        *kernel.plymouth.borrow_mut() = false;
        assert_eq!(m.take().unwrap(), Vec::<String>::new(), "no replay");
        kernel.log(3, "after Plymouth quit");
        assert_eq!(m.take().unwrap(), ["after Plymouth quit"]);

        let (kernel, mut m) = messages(Deferral::Always);
        *kernel.plymouth.borrow_mut() = true;
        kernel.log(3, "while Plymouth runs");
        assert_eq!(m.take().unwrap(), ["while Plymouth runs"]);
    }

    // ADR-1401 D15: `kernel-messages` is `after-plymouth` unset.
    #[test]
    fn kernel_messages_sets_the_deferral() {
        let deferral = |words: &[&str]| {
            let loaded = crate::config::load(&args(words), &mut |_: &Path| Ok(None));
            assert_eq!(loaded.reports, [], "{words:?}");
            Deferral::configured(&loaded.config)
        };
        assert_eq!(deferral(&[]), Deferral::AfterPlymouth);
        assert_eq!(
            deferral(&["--kernel-messages=after-plymouth"]),
            Deferral::AfterPlymouth
        );
        assert_eq!(deferral(&["--kernel-messages=always"]), Deferral::Always);
    }

    // Plymouth runs while its pid file names a running process called
    // plymouthd; a stale file, another program's process or no file at
    // all is no Plymouth.
    #[test]
    fn plymouth_runs_while_its_pid_file_names_a_running_plymouthd() {
        let dir = scratch("plymouth");
        let pid = dir.join("pid");
        let proc = dir.join("proc");
        std::fs::create_dir_all(proc.join("412")).unwrap();
        std::fs::write(proc.join("412/comm"), "plymouthd\n").unwrap();
        std::fs::create_dir_all(proc.join("77")).unwrap();
        std::fs::write(proc.join("77/comm"), "bash\n").unwrap();
        assert!(!plymouth_runs(&pid, &proc), "no pid file");
        std::fs::write(&pid, "412\n").unwrap();
        assert!(plymouth_runs(&pid, &proc));
        std::fs::write(&pid, "77\n").unwrap();
        assert!(!plymouth_runs(&pid, &proc), "another program");
        std::fs::write(&pid, "9999\n").unwrap();
        assert!(!plymouth_runs(&pid, &proc), "a stale file");
        std::fs::write(&pid, "plymouth\n").unwrap();
        assert!(!plymouth_runs(&pid, &proc), "no number");
    }

    #[test]
    fn a_long_message_ends_in_an_ellipsis() {
        let long = "x".repeat(MESSAGE_CHARS_MAX + 10);
        let c = clipped(&long);
        assert_eq!(c.chars().count(), MESSAGE_CHARS_MAX);
        assert!(c.ends_with('…'));
        assert_eq!(clipped("short"), "short");
        // the cut counts the decoded characters, not the kernel's escapes
        let escaped = "\\xe2\\x80\\x99".repeat(MESSAGE_CHARS_MAX);
        let decoded = record(format!("4,1,2,-;{escaped}\n").as_bytes())
            .unwrap()
            .text;
        assert_eq!(decoded.chars().count(), MESSAGE_CHARS_MAX);
        assert_eq!(clipped(&decoded), decoded, "exactly the most, so no cut");
    }
}
