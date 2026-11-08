//! `vtium config` (ADR-1201 D12) replacing its process with the tool, for
//! real. The process that runs `vtium::config_tool::run` is this test
//! binary again, its [`child`] test alone, in a role named by
//! `VTIUM_CONFIG_TOOL_CHILD`, with the configuration's `font-helper`
//! naming the stand-in, as the glyph helper's tests name theirs. The
//! stand-in is a shell: `/bin/sh` itself, or a copy of it this test owns,
//! so the tool vtium runs is a program it trusts. The child's exec
//! replaces the whole test process, so what the stand-in writes to `OUT`,
//! and the status it ends with, are the child's.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

#[path = "common/scratch.rs"]
mod scratch;
use scratch::Scratch;
// The helper that writes the stand-in programs, shared by every crate
// whose tests run a program they made.
use vtium_testscript as script;

/// The environment variable that names the child's role.
const ROLE: &str = "VTIUM_CONFIG_TOOL_CHILD";

/// The words the child hands the tool: a script for `sh -c`, then the
/// words a user typed, `--help` among them, which the console would
/// otherwise take as its own. The script writes its process id and each
/// word after the script on a line of its own, and ends with 7, a status
/// vtium itself never ends with.
const ARGS: [&str; 5] = [
    "-c",
    "{ echo \"$$\"; echo \"$0\"; for a in \"$@\"; do echo \"$a\"; done; } > \"$OUT\"; exit 7",
    "fonts",
    "two words",
    "--help",
];

/// The process `role` names: runs the tool `TOOL` names with [`ARGS`].
/// In the role `swapped`, the file at `TOOL` is replaced by `SWAP` after
/// vtium opened it and before the exec. As a plain test it does nothing.
#[test]
fn child() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    let tool = PathBuf::from(std::env::var_os("TOOL").expect("TOOL names the tool"));
    let arg = OsString::from(format!("--font-helper={}", tool.display()));
    let config = vtium::config::load(&[arg], &mut |_| Ok(None)).config;
    let args: Vec<OsString> = ARGS.iter().map(OsString::from).collect();
    let status = vtium::config_tool::run(
        &config,
        &args,
        |command| {
            if role == "swapped" {
                let swap = std::env::var_os("SWAP").expect("SWAP names the file");
                if let Err(e) = fs::rename(&swap, &tool) {
                    return e;
                }
            }
            command.exec()
        },
        &mut io::stderr(),
    );
    std::process::exit(status);
}

/// Runs [`child`] in `role` with `env`; its status and what the stand-in
/// wrote to `out`, one line each, or `None` when it wrote nothing.
fn spawn(role: &str, out: &Path, env: &[(&str, &Path)]) -> (u32, Option<i32>, Option<Vec<String>>) {
    let _ = fs::remove_file(out);
    let mut command = Command::new(std::env::current_exe().expect("the test binary"));
    command
        .args(["--exact", "child", "-q", "--test-threads=1"])
        .env(ROLE, role)
        .env("OUT", out);
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("run the child");
    let pid = child.id();
    let status = child.wait().expect("wait for the child").code();
    let lines = fs::read_to_string(out)
        .ok()
        .map(|t| t.lines().map(str::to_owned).collect());
    (pid, status, lines)
}

/// Held by each test for its whole run. A test writes a program and then
/// runs it, and a child another test forks meanwhile would hold the file
/// open for writing until its own exec, so the kernel would refuse to run
/// it (ETXTBSY).
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn one_at_a_time() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

/// A directory of the test's own, empty, removed when the test ends,
/// whether it passes or fails.
fn scratch(name: &str) -> Scratch {
    Scratch::new(&format!("config-tool-{name}"))
}

#[test]
fn vtium_config_replaces_itself_with_the_tool_and_passes_every_word() {
    let _one = one_at_a_time();
    let dir = scratch("runs");
    let out = dir.join("out");
    // A copy of /bin/sh that the test's own account owns: vtium runs a
    // tool only when root or its own account owns it, and /bin/sh belongs
    // to uid 65534 in the user namespace make arm-test runs the tests in
    // (tools/arm/README.md).
    let tool = dir.join("sh");
    script::executable_copy(Path::new("/bin/sh"), &tool);
    let (pid, status, lines) = spawn("runs", &out, &[("TOOL", &tool)]);
    assert_eq!(status, Some(7), "vtium ends with the tool's status");
    let lines = lines.expect("the tool ran");
    // The same process: vtium replaced itself with the tool, and started
    // no child of its own.
    assert_eq!(lines.first(), Some(&pid.to_string()), "{lines:?}");
    // Every word after the script, unchanged and in order.
    let words: Vec<String> = ARGS[2..].iter().map(|w| (*w).to_owned()).collect();
    assert_eq!(lines.get(1..), Some(&words[..]), "{lines:?}");
}

#[test]
fn a_tool_swapped_after_vtium_opened_it_never_runs() {
    let _one = one_at_a_time();
    let dir = scratch("swapped");
    let out = dir.join("out");
    // The tool, a copy of /bin/sh this test owns and alone writes, which
    // vtium trusts.
    let tool = dir.join("vtium-config");
    script::executable_copy(Path::new("/bin/sh"), &tool);
    // What lands at the tool's path once vtium has checked it.
    let swap = dir.join("swap");
    script::executable(&swap, "#!/bin/sh\necho swapped > \"$OUT\"\nexit 3\n");

    let (pid, status, lines) = spawn("swapped", &out, &[("TOOL", &tool), ("SWAP", &swap)]);
    assert_eq!(
        fs::read_to_string(&tool).ok().as_deref(),
        Some("#!/bin/sh\necho swapped > \"$OUT\"\nexit 3\n"),
        "the swap happened"
    );
    // The file vtium opened and checked ran, not the one swapped in.
    assert_eq!(status, Some(7));
    let lines = lines.expect("the tool ran");
    assert_eq!(lines.first(), Some(&pid.to_string()), "{lines:?}");
}

#[test]
fn an_untrusted_tool_never_runs() {
    let _one = one_at_a_time();
    let dir = scratch("untrusted");
    let out = dir.join("out");
    let tool = dir.join("vtium-config");
    script::executable_copy(Path::new("/bin/sh"), &tool);
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o777)).expect("chmod the tool");
    let (_, status, lines) = spawn("runs", &out, &[("TOOL", &tool)]);
    assert_eq!(status, Some(1));
    assert_eq!(lines, None, "the tool ran");
}
