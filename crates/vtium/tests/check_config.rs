//! `vtium check-config` run as the operator runs it: the binary, on a
//! configuration directory of its own.

use std::path::PathBuf;
use std::process::{Command, Output};

fn dir(name: &str, conf: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vtium-check-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("a directory");
    std::fs::write(d.join("kmscon.conf"), conf).expect("a configuration");
    d
}

fn check(dir: &PathBuf, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vtium"))
        .arg("check-config")
        .arg("-c")
        .arg(dir)
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
    let d = dir("warn", "sb-size=200000\npalette=nord\n");
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
    assert_eq!(lines[2], "palette=nord is not built yet; legacy is used");
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

/// With strace on the machine, the files it opens are only the
/// configuration's and the loader's: no VT, DRM node, input device or tty.
/// Under qemu-user (tools/arm), strace traces qemu, whose opens of the
/// target's files carry the QEMU_LD_PREFIX it translates them to: such a
/// trace says so and is skipped.
#[test]
fn it_opens_nothing_but_the_configuration() {
    if Command::new("strace").arg("-V").output().is_err() {
        eprintln!("strace is not installed: the open check is skipped");
        return;
    }
    let d = dir("strace", "font-size=16\n");
    let out = Command::new("strace")
        .args(["-f", "-e", "trace=open,openat", "-o", "/dev/stderr"])
        .arg(env!("CARGO_BIN_EXE_vtium"))
        .args(["check-config", "-c"])
        .arg(&d)
        .output()
        .expect("strace runs");
    let trace = String::from_utf8_lossy(&out.stderr);
    let opened: Vec<&str> = trace
        .lines()
        .filter(|l| l.contains("open") && !l.contains("ENOENT"))
        .filter_map(|l| l.split('"').nth(1))
        .collect();
    if let (Some(qemu), Some(prefix)) = (
        std::env::var_os("VTIUM_QEMU_USER"),
        std::env::var("QEMU_LD_PREFIX")
            .ok()
            .filter(|p| !p.is_empty()),
    ) && opened.iter().any(|p| p.starts_with(&prefix))
    {
        eprintln!(
            "strace traced {}, which opens the target's files under {prefix}: the open check is skipped",
            qemu.display()
        );
        std::fs::remove_dir_all(d).ok();
        return;
    }
    assert!(opened.iter().any(|p| p.ends_with("kmscon.conf")), "{trace}");
    for p in &opened {
        assert!(
            !p.starts_with("/dev/") && !p.starts_with("/sys/") && !p.starts_with("/run/udev"),
            "opened {p}: {trace}"
        );
    }
    std::fs::remove_dir_all(d).ok();
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
