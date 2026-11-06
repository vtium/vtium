//! `vtium check-config` run as the operator runs it: the binary, on a
//! configuration directory of its own and a vtium.conf of its own, given
//! as the candidate, so no test reads the machine's `/etc/vtium`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "common/scratch.rs"]
mod scratch;
use scratch::Scratch;

/// The sections of a vtium.conf that check clean, but for the notes
/// outside the status, which follow a test's own keys.
const CLEAN_SECTIONS: &str =
    "version = 1\n[head console]\n[rules]\ndisplay * -> console\ninput * -> roaming\n";

/// A directory whose vtium.conf holds the keys `conf` and the clean
/// sections below them.
fn dir(name: &str, conf: &str) -> Scratch {
    let d = Scratch::new(&format!("check-{name}"));
    std::fs::write(d.join("vtium.conf"), format!("{conf}{CLEAN_SECTIONS}"))
        .expect("a configuration");
    d
}

/// `vtium check-config -c DIR/etc DIR/vtium.conf`, then `extra`: the
/// candidate DIR/vtium.conf, whose ownership the check notes and does not
/// count, beside a configuration directory with no vtium.conf installed.
fn check(dir: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vtium"))
        .arg("check-config")
        .arg("-c")
        .arg(dir.join("etc"))
        .arg(dir.join("vtium.conf"))
        .args(extra)
        .output()
        .expect("vtium runs")
}

#[test]
fn a_clean_configuration_exits_zero_and_says_nothing() {
    let d = dir("clean", "font-size=16\nno-blink\nrender-timer=log\n");
    let out = check(&d, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
}

#[test]
fn each_warning_is_printed_and_it_exits_one() {
    let d = dir("warn", "sb-size=200000\npalette=frob\n");
    let out = check(&d, &["--frob"]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 3, "{err}");
    assert!(
        lines[0].ends_with("unknown key \"--frob\", ignored"),
        "{err}"
    );
    assert!(
        lines[1].ends_with("vtium.conf:1: sb-size=\"200000\" is out of range, so 100000 is used"),
        "{err}"
    );
    assert!(
        lines[2].ends_with(
            "vtium.conf:2: palette=\"frob\" is not one of legacy, custom, nord, \
             solarized, solarized-black, solarized-white, soft-black, base16-dark, base16-light, \
             vga, ignored"
        ),
        "{err}"
    );
}

// vtium reads font-frits the same way whatever directory it starts in, so
// a relative or an empty one is reported with its file, line and key, and
// ignored, as the start reads it; a seat file beside vtium.conf is never
// read (ADR-1403 D2)
#[test]
fn a_relative_or_empty_font_frits_is_reported_and_it_exits_one() {
    let d = dir("frits", "font-frits=fonts\nfont-frits=\n");
    std::fs::write(d.join("seat0.seat.conf"), "font-frits=./fonts\n").expect("a seat file");
    let out = check(&d, &[]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 2, "{err}");
    let why = "the path must be absolute, starting with /, ignored";
    assert!(
        lines[0].ends_with(&format!("vtium.conf:1: font-frits=\"fonts\": {why}")),
        "{err}"
    );
    assert!(
        lines[1].ends_with(&format!("vtium.conf:2: font-frits=\"\": {why}")),
        "{err}"
    );
    // An absolute path checks clean.
    let d = dir("frits-absolute", "font-frits=/srv/frits\n");
    let out = check(&d, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
}

#[test]
fn help_says_it_only_checks() {
    let d = dir("help", "");
    let out = check(&d, &["--help"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("It only reads"));
}

/// The syscalls that change a file or its metadata by name or by a
/// descriptor, besides an open for writing.
const WRITES: [&str; 37] = [
    "creat",
    "rename",
    "renameat",
    "renameat2",
    "unlink",
    "unlinkat",
    "rmdir",
    "mkdir",
    "mkdirat",
    "mknod",
    "mknodat",
    "link",
    "linkat",
    "symlink",
    "symlinkat",
    "truncate",
    "ftruncate",
    "chmod",
    "fchmod",
    "fchmodat",
    "fchmodat2",
    "chown",
    "fchown",
    "lchown",
    "fchownat",
    "utime",
    "utimes",
    "utimensat",
    "futimesat",
    "setxattr",
    "lsetxattr",
    "fsetxattr",
    "removexattr",
    "lremovexattr",
    "fremovexattr",
    "name_to_handle_at",
    "open_by_handle_at",
];

/// What a line of `strace -f` names: the syscall, and its first quoted
/// argument, a path.
fn syscall(line: &str) -> Option<(&str, &str)> {
    let call = line.split('(').next()?.split_whitespace().last()?;
    let path = line.split('"').nth(1).unwrap_or("");
    Some((call, path))
}

/// With strace on the machine, the check opens nothing under `/dev` (no
/// VT, DRM node, input device or tty) and writes nothing: no open for
/// writing or creating, and no rename, unlink or other change to a file
/// (ADR-1401 D7, which lets it read sysfs and udev's database, governing
/// over vtium/vtium#21's "reads only the configuration files"). Under
/// qemu-user (tools/arm), strace traces qemu, whose opens of the target's
/// files carry the QEMU_LD_PREFIX it translates them to: such a trace says
/// so and is skipped.
#[test]
fn it_opens_nothing_under_dev_and_writes_nothing() {
    let d = dir("strace", "font-size=16\n");
    let Some(trace) = traced(&d, &[]) else {
        return;
    };
    let calls = calls(&trace);
    let opened = opened(&calls);
    assert!(opened.iter().any(|p| p.ends_with("vtium.conf")), "{trace}");
    assert!(
        calls.iter().any(|(_, path, _)| path.starts_with("/sys/")),
        "the devices are read from /sys: {trace}"
    );
    assert!(
        !opened.iter().any(|p| p.starts_with("/etc/vtium")),
        "{trace}"
    );
    for (call, path, line) in &calls {
        let open = matches!(*call, "open" | "openat" | "openat2");
        let flags = line.rsplit_once('"').map_or("", |(_, rest)| rest);
        let writing = ["O_WRONLY", "O_RDWR", "O_CREAT", "O_TRUNC"]
            .iter()
            .any(|f| flags.contains(f));
        assert!(
            !(open && path.starts_with("/dev/")),
            "opened {path}: {line}"
        );
        assert!(!(open && writing), "opened {path} to write: {line}");
        assert!(!WRITES.contains(call), "changed {path}: {line}");
    }
}

/// The check's help lists no device: it reads the configuration, and
/// nothing of sysfs or udev's database.
#[test]
fn help_lists_no_device() {
    let d = dir("strace-help", "font-size=16\n");
    let Some(trace) = traced(&d, &["--help"]) else {
        return;
    };
    let calls = calls(&trace);
    assert!(
        opened(&calls).iter().any(|p| p.ends_with("vtium.conf")),
        "{trace}"
    );
    for (_, path, line) in &calls {
        assert!(
            !path.starts_with("/sys/") && !path.starts_with("/run/udev/data"),
            "read {path}: {line}"
        );
    }
}

/// The trace of `vtium check-config -c DIR/etc DIR/vtium.conf`, then `extra`,
/// of the syscalls that name a file and of [`WRITES`]; `None`, saying
/// why, where strace is not installed, or where it traced qemu-user
/// (tools/arm), whose opens of the target's files carry the
/// QEMU_LD_PREFIX it translates them to.
fn traced(d: &Path, extra: &[&str]) -> Option<String> {
    if Command::new("strace").arg("-V").output().is_err() {
        eprintln!("strace is not installed: the trace check is skipped");
        return None;
    }
    let out = Command::new("strace")
        .args(["-f", "-o", "/dev/stderr", "-e"])
        .arg(format!("trace=%file,{}", WRITES.join(",")))
        .arg(env!("CARGO_BIN_EXE_vtium"))
        .args(["check-config", "-c"])
        .arg(d.join("etc"))
        .arg(d.join("vtium.conf"))
        .args(extra)
        .output()
        .expect("strace runs");
    let trace = String::from_utf8_lossy(&out.stderr).into_owned();
    if let (Some(qemu), Some(prefix)) = (
        std::env::var_os("VTIUM_QEMU_USER"),
        std::env::var("QEMU_LD_PREFIX")
            .ok()
            .filter(|p| !p.is_empty()),
    ) && opened(&calls(&trace))
        .iter()
        .any(|p| p.starts_with(&prefix))
    {
        eprintln!(
            "strace traced {}, which opens the target's files under {prefix}: the trace check is skipped",
            qemu.to_string_lossy()
        );
        return None;
    }
    Some(trace)
}

/// Each traced syscall: its name, its path and its line.
fn calls(trace: &str) -> Vec<(&str, &str, &str)> {
    trace
        .lines()
        .filter_map(|l| syscall(l).map(|(call, path)| (call, path, l)))
        .collect()
}

/// The paths `calls` opened.
fn opened<'a>(calls: &[(&str, &'a str, &str)]) -> Vec<&'a str> {
    calls
        .iter()
        .filter(|(call, _, l)| call.starts_with("open") && !l.contains("ENOENT"))
        .map(|(_, path, _)| *path)
        .collect()
}

/// The subcommand is found after the options too: this is the check's
/// help, not vtium's.
#[test]
fn check_config_after_the_options_still_checks() {
    let d = dir("after", "");
    let out = Command::new(env!("CARGO_BIN_EXE_vtium"))
        .arg("-c")
        .arg(&*d)
        .args(["check-config", "--help"])
        .output()
        .expect("vtium runs");
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("It only reads"));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
}

/// A vtium.conf of sections alone in a directory of its own, as a
/// candidate is checked before it is installed (ADR-1401 D7), in place of
/// the clean one.
fn head_file(name: &str, text: &str) -> (Scratch, PathBuf) {
    let d = dir(name, "");
    let file = d.join("vtium.conf");
    std::fs::write(&file, text).expect("a vtium.conf");
    (d, file)
}

// ADR-1401 D11: `vtium check-config` lists every planted error with its
// line
#[test]
fn check_config_lists_each_planted_head_file_error_with_its_line_and_exits_1() {
    let (d, file) = head_file(
        "planted",
        "version = 1\n\
         [head desk]\n\
         consoles = 2\n\
         frob = 1\n\
         [head desk]\n\
         [rules]\n\
         display * -> roaming\n\
         input path:pci-0000:15:00.0-usb-0:3.3 -> desc\n\
         [rules]\n",
    );
    let out = check(&d, &[]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    let at = |line: usize, what: &str| {
        let prefix = format!("{}:{line}: ", file.display());
        assert!(
            err.lines()
                .any(|l| l.starts_with(&prefix) && l.contains(what)),
            "no line {line} naming {what:?}: {err}"
        );
    };
    at(4, "This section has no key named \"frob\".");
    at(5, "The file already has a head named \"desk\".");
    at(7, "A display rule cannot send to `roaming`.");
    at(
        8,
        "This rule sends devices to head \"desc\", but the file has no head of that name.",
    );
    at(
        9,
        "The file has a second `[rules]` section, and only one is allowed.",
    );
    assert!(!err.contains("unknown option"), "{err}");
}

// ADR-1401 D7: a policy line vtium cannot read makes the file closed and
// broken; `vtium check-config` names the line, says the whole file drops
// under the closed policy, and exits 1
// covers: platform.heads.policy
#[test]
fn check_config_names_a_policy_line_it_cannot_read_and_exits_1() {
    let (d, file) = head_file(
        "unreadable-policy",
        "version = 1\npolicy = opne\n[head desk]\n[rules]\ndisplay * -> desk\n",
    );
    let out = check(&d, &[]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    let prefix = format!("{}:2: ", file.display());
    let named: Vec<&str> = err.lines().filter(|l| l.starts_with(&prefix)).collect();
    assert!(
        named
            .iter()
            .any(|l| l.contains("`policy` cannot be \"opne\"")),
        "{err}"
    );
    assert!(
        named
            .iter()
            .any(|l| l.contains("broken with the closed policy")),
        "{err}"
    );
}

// ADR-1401 D7: each error says what it drops, and standard output ends on
// what a start does with the file and the policy it states
// covers: platform.heads.rules
#[test]
fn each_error_says_what_it_drops_and_the_outcome_follows() {
    let (d, file) = head_file(
        "drops",
        "version = 1
         policy = closed
         [head desk]
         [head kiosk]
         frob = 1
         [rules]
         display * -> desk
         input * -> kiosk
         input name:Foo -> nosuch
",
    );
    // judged by the rules alone, so the route's message names no device
    // of the machine the test runs on
    let out = check(&d, &["--no-devices"]);
    assert_eq!(out.status.code(), Some(1));
    let f = file.display();
    let err = String::from_utf8_lossy(&out.stderr);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(
        lines,
        [
            format!(
                "{f}:5: This section has no key named \"frob\". Check the spelling, or remove the line."
            ),
            format!(
                "{f}:5: Because of this error, vtium ignores this head and every rule that names it."
            ),
            format!(
                "{f}:9: This rule sends devices to head \"nosuch\", but the file has no head of that name. Correct the name, or add a `[head NAME]` section for it."
            ),
            format!(
                "{f}:9: Because of this error, vtium ignores this rule, and each device goes on to the next rule."
            ),
            // ADR-1403 D3: what survives sends no keyboard to desk, which
            // the closed policy isolates
            format!(
                "{f}: No input rule sends a keyboard to head \"desk\", which gets its displays from rule 1. Add an input rule that sends a keyboard to \"desk\" or to `roaming`."
            ),
        ],
        "{err}"
    );
    let notes = String::from_utf8_lossy(&out.stdout);
    let tail: Vec<&str> = notes.lines().rev().take(2).collect();
    assert_eq!(
        tail,
        [
            format!(
                "{f}: The policy is closed: an input device no rule places is ignored, a head without an `isolated` line is isolated, and the keyboard safeguards (anti-lockout and the rescue key) are off."
            ),
            format!(
                "{f}: vtium starts with the heads and rules that have no errors, and the rescue key off. Each head keeps its `isolated` setting, and a head without one is isolated. It shows `vtium.conf has errors: running the parts without errors.`."
            ),
        ],
        "{notes}"
    );
}

// ADR-1401 D9, D10: a head's `consoles` past vtium.conf's
// `head-consoles-max` counts against the file, so the check exits 1
// covers: platform.heads.rules
#[test]
fn consoles_past_head_consoles_max_exit_1() {
    let (d, file) = head_file(
        "cap",
        "version = 1
[head desk]
consoles = 5
[rules]
display * -> desk
input * -> roaming
",
    );
    let out = check(&d, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        format!(
            "{}:3: Head \"desk\" asks for more consoles than `head-consoles-max` allows, so it gets 4. Lower its `consoles` to 4 or fewer, or raise `head-consoles-max` in vtium.conf.\n",
            file.display()
        )
    );
    let out = check(&d, &["--head-consoles-max=5"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
}

// ADR-1401 D4, D7: input sent to a head by `name` alone is a note on
// standard output, outside the exit status
#[test]
fn a_candidate_with_only_notes_exits_0_and_prints_them_on_stdout() {
    let (d, file) = head_file(
        "notes",
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput name:Foo* -> desk\n",
    );
    let out = check(&d, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
    let notes = String::from_utf8_lossy(&out.stdout);
    let weak = format!(
        "{}:5: weak match: this rule matches by `id` or `name` alone, which any device can claim",
        file.display()
    );
    assert!(notes.lines().any(|l| l == weak), "{notes}");
}
