//! vtium-config's command line, run as the invoking user: its usage
//! errors, the store's commands gone, and the parser's child. The source
//! forms and decoders are judged through the helper's engine
//! (`tests/engine.rs`).

use std::fs;
use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn status(o: &Output) -> i32 {
    o.status.code().expect("exited")
}

#[test]
fn usage_errors_exit_2() {
    for args in [
        &["frobnicate"][..],
        &[],
        &["--bogus", "set", "x"],
        &["--frits"],
        &["serve", "x"],
    ] {
        let o = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(status(&o), 2, "{args:?}: {}", stderr(&o));
        assert!(stderr(&o).contains("--help"), "{args:?}");
    }
    let o = Command::new(BIN).arg("--help").output().unwrap();
    assert_eq!(status(&o), 0);
    assert!(stdout(&o).starts_with("Usage: vtium-config"));
}

/// The parser, started as the tool starts it, sets no-new-privileges and
/// its limits before it reads anything.
#[test]
fn the_parser_runs_without_new_privileges_and_within_its_limits() {
    let mut child = Command::new(BIN)
        .arg("--parse")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The parser reads its request only once every limit is set, so the
    // limits are read when a byte of it has left the pipe, unless the
    // parser ended first.
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(&[0]).unwrap();
    let read = loop {
        if rustix::io::ioctl_fionread(&stdin).unwrap() == 0 {
            break true;
        }
        if child.try_wait().unwrap().is_some() {
            break false;
        }
        std::thread::yield_now();
    };
    let proc_status =
        fs::read_to_string(format!("/proc/{}/status", child.id())).unwrap_or_default();
    let limits = fs::read_to_string(format!("/proc/{}/limits", child.id())).unwrap_or_default();
    drop(stdin);
    let mut err = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    let _ = child.stdout.take().unwrap().read_to_end(&mut Vec::new());
    assert!(!child.wait().unwrap().success());
    assert!(read, "the parser never read its request");
    assert!(
        proc_status
            .lines()
            .any(|l| l.split_whitespace().eq(["NoNewPrivs:", "1"])),
        "NoNewPrivs not set"
    );
    let limit = |name: &str| {
        limits
            .lines()
            .find(|l| l.starts_with(name))
            .map(|l| {
                l[name.len()..]
                    .split_whitespace()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    };
    assert_eq!(limit("Max address space"), "536870912 536870912");
    assert_eq!(limit("Max cpu time"), "30 30");
    assert_eq!(limit("Max processes"), "0 0");
    assert_eq!(limit("Max file size"), "0 0");
    // an empty request is refused, not parsed
    assert!(err.contains("malformed parse request"), "{err}");
    let _ = std::io::stdout().flush();
}

// ADR-1200 (#145): the prerendered store is gone, and with it the commands
// that built, installed, listed, verified and removed its sets. Each is now
// an unknown command, a usage error.
#[test]
fn the_store_commands_are_gone() {
    for command in ["create", "install", "list", "verify", "remove"] {
        let o = Command::new(BIN).arg(command).output().unwrap();
        assert_eq!(status(&o), 2, "{command}: {}", stderr(&o));
        assert!(
            stderr(&o).contains(&format!("unknown command {command}")),
            "{command}: {}",
            stderr(&o)
        );
    }
}

/// What `vtium-config` run as root refuses: every option that names a path
/// or a program, and the step that releases signals and runs another
/// program, so `sudo vtium-config` reaches only the system's files.
#[test]
fn as_root_every_path_and_program_option_is_refused() {
    use std::ffi::OsString;
    use vtium_config::cli::root_refusal;
    let args = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
    let flagged: Vec<&str> = vtium_config::cli::VALUE_OPTIONS
        .iter()
        .filter(|o| o.path)
        .map(|o| o.name)
        .collect();
    assert!(flagged.len() >= 8, "{flagged:?}");
    for option in flagged {
        let why = root_refusal(0, &args(&[option, "/tmp/x", "apply", "/tmp/d"]))
            .unwrap_or_else(|| panic!("{option} is refused as root"));
        assert!(why.contains(option) && why.contains("/etc/vtium"), "{why}");
        let glued = format!("{option}=/tmp/x");
        assert!(
            root_refusal(0, &args(&[&glued, "apply", "/tmp/d"])).is_some(),
            "{glued}"
        );
        assert_eq!(
            root_refusal(1000, &args(&[option, "/tmp/x", "apply", "/tmp/d"])),
            None
        );
    }
    let unblock = vtium_config::tui::port::UNBLOCK_ARG;
    assert!(root_refusal(0, &args(&[unblock, "/bin/sh"])).is_some());
    assert_eq!(root_refusal(1000, &args(&[unblock, "/bin/sh"])), None);
    // the one command sudo runs, and the other commands, pass
    assert_eq!(root_refusal(0, &args(&["apply", "/home/me/d"])), None);
    assert_eq!(
        root_refusal(0, &args(&["--no-banner", "set", "proof"])),
        None
    );
}

/// ADR-1201 D5: the TUI runs as the user from start to end and never as
/// root; as root it does not open, and says how to run it.
#[test]
fn as_root_the_tui_does_not_open() {
    use std::ffi::OsString;
    use vtium_config::cli::root_refusal;
    let args = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
    // an empty command word is no command, so it opens the TUI too
    for words in [&[][..], &["--no-banner"], &[""], &["--", ""]] {
        let why = root_refusal(0, &args(words)).expect("refused as root");
        assert!(why.contains("as yourself") && why.contains("sudo"), "{why}");
        assert_eq!(root_refusal(1000, &args(words)), None);
    }
    assert_eq!(root_refusal(0, &args(&["--version"])), None);
}

/// As root, the commands that read a file the user names, or that start
/// the glyph helper, which refuses root, are refused: `sudo vtium-config`
/// reads and writes only the system's files, and never prints a file root
/// read for the user.
#[test]
fn as_root_the_commands_that_run_as_the_user_are_refused() {
    use std::ffi::OsString;
    use vtium_config::cli::root_refusal;
    let args = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
    for (words, says) in [
        (&["check", "/etc/shadow"][..], "check"),
        (&["fonts"], "fonts"),
        (&["preview", "unifont"], "preview"),
        (&["new", "--from", "/etc/shadow"], "new --from"),
        (&["new", "--name", "x", "--from=/etc/shadow"], "new --from"),
    ] {
        let why = root_refusal(0, &args(words)).unwrap_or_else(|| panic!("{words:?}"));
        assert!(why.contains(says) && why.contains("as yourself"), "{why}");
        assert_eq!(root_refusal(1000, &args(words)), None, "{words:?}");
    }
    // the commands that change the system's files from text they build
    // themselves still run as root
    for words in [
        &["config", "font-size", "12"][..],
        &["new", "kbd:lat0-16", "--name", "lat"],
        &["remove-frit", "old"],
    ] {
        assert_eq!(root_refusal(0, &args(words)), None, "{words:?}");
    }
}

/// Runs the binary as root in an unprivileged user namespace, where its
/// effective uid is 0, with `args`; `None` where the machine has user
/// namespaces off.
fn as_namespace_root(args: &[&str]) -> Option<Output> {
    as_namespace_root_with(args, &[])
}

/// As [`as_namespace_root`], with the variables `envs` set besides the
/// test's own environment.
fn as_namespace_root_with(args: &[&str], envs: &[(&str, &str)]) -> Option<Output> {
    let probe = Command::new("unshare").args(["-r", "true"]).output().ok()?;
    if !probe.status.success() {
        eprintln!(
            "SKIPPED: unshare -r cannot make a user namespace here: {}",
            stderr(&probe)
        );
        return None;
    }
    Some(
        Command::new("unshare")
            .arg("-r")
            .arg(BIN)
            .args(args)
            .envs(envs.iter().copied())
            .stdin(Stdio::null())
            .output()
            .expect("unshare runs"),
    )
}

#[test]
fn the_binary_run_as_root_refuses_paths_programs_and_the_tui() {
    for (args, says) in [
        (
            &["--configdir", "/tmp/x", "apply", "/tmp/d"][..],
            "--configdir",
        ),
        (&["--vtium", "/bin/true", "apply", "/tmp/d"], "--vtium"),
        (&["--run-with-signals", "/bin/sh"], "--run-with-signals"),
        (&[], "as yourself"),
        (&[""], "as yourself"),
        (&["new"], "as yourself"),
        (&["check", "/etc/shadow"], "as yourself"),
        (&["new", "--from", "/etc/shadow"], "as yourself"),
    ] {
        let Some(o) = as_namespace_root(args) else {
            return;
        };
        assert_eq!(status(&o), 2, "{args:?}: {}", stderr(&o));
        assert!(stderr(&o).contains(says), "{args:?}: {}", stderr(&o));
    }
    // and takes the one command sudo runs, failing on the draft alone
    let Some(o) = as_namespace_root(&["apply", "/nonexistent-draft"]) else {
        return;
    };
    assert_eq!(status(&o), 1, "{}", stderr(&o));
    assert!(stderr(&o).contains("/nonexistent-draft"), "{}", stderr(&o));
}

/// doas with `keepenv` keeps a `SUDO_UID` the user set beside the
/// `DOAS_USER` doas sets; apply run as root refuses the draft then, since
/// either could name a user who did not run it.
#[test]
fn the_binary_run_as_root_refuses_more_than_one_tools_user() {
    let draft = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli-two-users-draft");
    let _ = fs::remove_dir_all(&draft);
    fs::create_dir_all(&draft).unwrap();
    fs::write(draft.join("MANIFEST"), "").unwrap();
    let dir = draft.to_str().unwrap();
    let Some(o) = as_namespace_root_with(
        &["apply", dir],
        &[("SUDO_UID", "4242"), ("DOAS_USER", "root")],
    ) else {
        return;
    };
    assert_eq!(status(&o), 1, "{}", stderr(&o));
    assert!(
        stderr(&o).contains("SUDO_UID") && stderr(&o).contains("DOAS_USER"),
        "{}",
        stderr(&o)
    );
    assert!(stderr(&o).contains("Nothing was written"), "{}", stderr(&o));
    // with one tool's variable alone, the same draft applies, writing nothing
    let Some(o) = as_namespace_root_with(&["apply", dir], &[("DOAS_USER", "root")]) else {
        return;
    };
    assert_eq!(status(&o), 0, "{}", stderr(&o));
}
