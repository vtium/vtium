//! `vtium check-config` run as the operator runs it: the binary, on a
//! configuration directory of its own and a head file of its own, given
//! as the candidate, so no test reads the machine's `/etc/vtium`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A head file that checks clean, but for the notes outside the status.
const CLEAN_HEADS: &str =
    "version = 1\n[head console]\n[rules]\ndisplay * -> console\ninput * -> focus\n";

fn dir(name: &str, conf: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vtium-check-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("a directory");
    std::fs::write(d.join("kmscon.conf"), conf).expect("a configuration");
    std::fs::write(d.join("heads.conf"), CLEAN_HEADS).expect("a head file");
    d
}

/// `vtium check-config -c DIR DIR/heads.conf`, then `extra`.
fn check(dir: &PathBuf, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vtium"))
        .arg("check-config")
        .arg("-c")
        .arg(dir)
        .arg(dir.join("heads.conf"))
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
    std::fs::remove_dir_all(d).ok();
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
        lines[1].ends_with("kmscon.conf:1: sb-size=\"200000\" is out of bounds, 100000 used"),
        "{err}"
    );
    assert!(
        lines[2].ends_with(
            "kmscon.conf:2: palette=\"frob\" is none of legacy, custom, nord, \
             solarized, solarized-black, solarized-white, soft-black, base16-dark, base16-light, \
             vga, ignored"
        ),
        "{err}"
    );
    std::fs::remove_dir_all(d).ok();
}

#[test]
fn help_says_it_only_checks() {
    let d = dir("help", "");
    let out = check(&d, &["--help"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("It only checks"));
    std::fs::remove_dir_all(d).ok();
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
        std::fs::remove_dir_all(d).ok();
        return;
    };
    let calls = calls(&trace);
    let opened = opened(&calls);
    assert!(opened.iter().any(|p| p.ends_with("kmscon.conf")), "{trace}");
    assert!(opened.iter().any(|p| p.ends_with("heads.conf")), "{trace}");
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
    std::fs::remove_dir_all(d).ok();
}

/// The check's help lists no device: it reads the configuration, and
/// nothing of sysfs or udev's database.
#[test]
fn help_lists_no_device() {
    let d = dir("strace-help", "font-size=16\n");
    let Some(trace) = traced(&d, &["--help"]) else {
        std::fs::remove_dir_all(d).ok();
        return;
    };
    let calls = calls(&trace);
    assert!(
        opened(&calls).iter().any(|p| p.ends_with("kmscon.conf")),
        "{trace}"
    );
    for (_, path, line) in &calls {
        assert!(
            !path.starts_with("/sys/") && !path.starts_with("/run/udev/data"),
            "read {path}: {line}"
        );
    }
    std::fs::remove_dir_all(d).ok();
}

/// The trace of `vtium check-config -c DIR DIR/heads.conf`, then `extra`,
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
        .arg(d)
        .arg(d.join("heads.conf"))
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
        .arg(&d)
        .args(["check-config", "--help"])
        .output()
        .expect("vtium runs");
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("It only checks"));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "");
    std::fs::remove_dir_all(d).ok();
}

/// A head file in a directory of its own, as a candidate is checked
/// before it is installed (ADR-1401 D7), in place of the clean one.
fn head_file(name: &str, text: &str) -> (PathBuf, PathBuf) {
    let d = dir(name, "");
    let file = d.join("heads.conf");
    std::fs::write(&file, text).expect("a head file");
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
         profile = desk\n\
         frob = 1\n\
         [head desk]\n\
         [rules]\n\
         display * -> focus\n\
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
    at(4, "unknown key \"frob\"");
    at(5, "a second head named \"desk\"");
    at(7, "`focus` is no target for a display rule");
    at(8, "rule names undefined head \"desc\"");
    at(9, "a second [rules] section");
    assert!(!err.contains("is no option"), "{err}");
    std::fs::remove_dir_all(d).ok();
}

// ADR-1401 D7, D9: a guard turned off is a note on standard output,
// outside the exit status
#[test]
fn a_candidate_with_only_notes_exits_0_and_prints_them_on_stdout() {
    let (d, file) = head_file(
        "notes",
        "version = 1\n[head desk]\n[rules]\ndisplay * -> desk\n[guards]\nnever-dark = off\n",
    );
    let out = check(&d, &[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
    let notes = String::from_utf8_lossy(&out.stdout);
    let guard = format!("{}:6: the never-dark guard is off", file.display());
    assert!(notes.lines().any(|l| l == guard), "{notes}");
    std::fs::remove_dir_all(d).ok();
}
