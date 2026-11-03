//! `vtium config` (ADR-1201 D12): the console binary replaces itself with
//! vtium-config, the font and settings tool, and hands it the rest of its
//! command line. The test copies the console binary into a directory of
//! its own, beside a stand-in vtium-config that writes down what it was
//! given, so the binary finds the stand-in beside itself and never a
//! vtium-config installed on this machine. The file holds one test, so no
//! other test's child can inherit the stand-in open for writing while the
//! binary runs it.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

#[test]
fn vtium_config_runs_the_tool_beside_it_with_its_arguments() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("vtium-config-hand-off");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("make the test's directory");
    let vtium = dir.join("vtium");
    fs::copy(env!("CARGO_BIN_EXE_vtium"), &vtium).expect("copy the console binary");
    let out = dir.join("out");
    // The stand-in writes its process id, then each argument on a line of
    // its own, and exits 7, a status vtium itself never ends a run with.
    let tool = dir.join("vtium-config");
    fs::write(
        &tool,
        "#!/bin/sh\n{ echo \"$$\"; for a in \"$@\"; do echo \"$a\"; done; } > \"$OUT\"\nexit 7\n",
    )
    .expect("write the stand-in");
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).expect("make it executable");

    let mut child = Command::new(&vtium)
        .args(["config", "fonts", "two words", "--unset", "--help"])
        .env("OUT", &out)
        .spawn()
        .expect("run vtium config");
    let pid = child.id();
    let status = child.wait().expect("wait for vtium config");

    assert_eq!(status.code(), Some(7), "vtium ends with the tool's status");
    let lines = fs::read_to_string(&out).expect("the stand-in ran");
    let lines: Vec<&str> = lines.lines().collect();
    // The same process: vtium replaced itself with the tool, and started
    // no child of its own.
    assert_eq!(lines.first(), Some(&pid.to_string().as_str()), "{lines:?}");
    // Every argument after `config`, unchanged and in order, `--help`
    // included, which the console would otherwise take as its own.
    assert_eq!(
        lines.get(1..),
        Some(&["fonts", "two words", "--unset", "--help"][..]),
        "{lines:?}"
    );
}
