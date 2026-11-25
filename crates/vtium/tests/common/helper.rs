//! The glyph helper's stand-in for the integration tests that start one:
//! vtium-config's real serve mode, run by the test binary itself in a
//! process of its own, so each answer is the one the real helper gives.
//! Each test binary that includes this module has its own
//! [`the_helper_stand_in`], which its [`real_helper`] starts.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The environment variable that makes [`the_helper_stand_in`] serve, as
/// the stand-in for the glyph helper, instead of passing as a test.
const STAND_IN: &str = "VTIUM_TEST_HELPER";

/// The shell the tests start in place of the font helper: a copy of
/// `/bin/sh` that the test's own account owns, in the build's temporary
/// directory. vtium starts a helper only when root or its own account owns
/// the program, and `/bin/sh` belongs to root only where the tests see
/// root's files as root's. In a user namespace that does not map root, such
/// as the one `make arm-test` runs the tests in (tools/arm/README.md), every
/// file of root's belongs to the overflow uid, 65534. Each test process
/// makes its copy once and renames it into place, so a test process that
/// runs at the same time never starts a copy that is still being written.
/// `cp` writes the copy (`vtium_testscript::executable_copy`), so this
/// process never holds the copy open for writing. Had it done so, a child
/// that another test thread forked during the copy would keep the copy
/// busy until that child ran its own program, and the kernel would refuse
/// to start the shell with ETXTBSY.
pub fn stand_in_shell() -> &'static Path {
    static SHELL: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    SHELL.get_or_init(|| {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
        let shell = dir.join("vtium-stand-in-sh");
        let part = dir.join(format!("vtium-stand-in-sh.{}", std::process::id()));
        vtium_testscript::executable_copy(Path::new("/bin/sh"), &part);
        std::fs::rename(&part, &shell).expect("the copy in place");
        shell
    })
}

/// The environment variable that names the signal by which
/// [`the_stand_in_dies_without_a_core_dump`] ends its process.
const DIE_BY: &str = "VTIUM_TEST_DIE_BY";

/// `script` for the stand-in shell, with traps first that end the shell
/// without a core dump when SIGSEGV or SIGXCPU reaches it. The tests end
/// the stand-in with those signals on purpose, and the kernel would write a
/// core dump of the shell for each one. Where `core_pattern` pipes to
/// systemd-coredump, the kernel ignores the core limit, so each dump lands
/// in the journal and the coredump store of the machine that runs the
/// tests. A process can turn its own dumps off with
/// `prctl(PR_SET_DUMPABLE, 0)`, but each exec turns them on again, and the
/// shell cannot make that call. So each trap runs this test binary in the
/// shell's place, its [`the_stand_in_dies_without_a_core_dump`] alone,
/// which turns its dumps off and then sends itself the signal. The process
/// that vtium started still ends by the signal the script sent.
pub fn undumpable(script: &str) -> String {
    let exe = std::env::current_exe().expect("the test binary");
    let exe = exe.to_str().expect("the test binary's path in UTF-8");
    assert!(
        !exe.contains(['\'', '"', '$', '`', '\\']),
        "the test binary's path needs no quoting: {exe}"
    );
    let trap = |signal: &str| {
        format!(
            "trap '{DIE_BY}={signal} exec \"{exe}\" --exact stand_in::the_stand_in_dies_without_a_core_dump -q >/dev/null 2>&1' {signal}; "
        )
    };
    format!("{}{}{script}", trap("SEGV"), trap("XCPU"))
}

/// Not a test of the console: run by a trap of [`undumpable`] with
/// [`DIE_BY`] set, it turns off this process's core dumps and ends the
/// process by the signal [`DIE_BY`] names; run by the test harness, it
/// does nothing. The standard library catches SIGSEGV to report a stack
/// overflow, and its handler puts back the default action and returns. So
/// the first SIGSEGV can end in that handler, and the signal is sent until
/// one ends the process.
#[test]
fn the_stand_in_dies_without_a_core_dump() {
    let Ok(name) = std::env::var(DIE_BY) else {
        return;
    };
    let signal = match name.as_str() {
        "SEGV" => rustix::process::Signal::SEGV,
        "XCPU" => rustix::process::Signal::XCPU,
        _ => panic!("no signal named {name}"),
    };
    rustix::process::set_dumpable_behavior(rustix::process::DumpableBehavior::NotDumpable)
        .expect("core dumps off");
    loop {
        let _ = rustix::process::kill_process(rustix::process::getpid(), signal);
        std::thread::yield_now();
    }
}

// vtium/vtium#451: a stand-in shell that a script ends by SIGSEGV ends by
// that signal and leaves no core dump. The kernel says in the wait status
// whether it wrote one, a pipe to systemd-coredump included.
#[test]
fn a_stand_in_ended_by_a_signal_leaves_no_core_dump() {
    use std::os::unix::process::ExitStatusExt;
    let status = std::process::Command::new(stand_in_shell())
        .arg("-c")
        .arg(undumpable("kill -SEGV $$; sleep 5"))
        .status()
        .expect("the stand-in runs");
    assert_eq!(status.signal(), Some(11), "{status:?}");
    assert!(!status.core_dumped(), "a core dump was written");
}

/// The glyph helper the outline tests run: this test binary again, its
/// [`the_helper_stand_in`] alone, which serves with vtium-config's real
/// serve mode, so each answer is the one the real helper gives. So the
/// stand-in reads each source with the real helper's reader of its kind,
/// and refuses an outline font under `file:` and a console font under
/// `outline:` as the real helper does. With
/// `go`, a FIFO, its first answer waits until the test opens `go` to
/// write, and the kind of each frame it sends is appended to `kinds`, one
/// line each. The module is included as `stand_in`, the name the test
/// harness is asked to run. The test harness writes
/// to standard output, so the frames go out through descriptor 3, which
/// the shell points at the console's pipe.
pub fn real_helper(kinds: &Path, go: Option<&Path>) -> vtium::helper::Command {
    let mode = format!(
        "{}:{}",
        kinds.display(),
        go.map(|g| g.display().to_string()).unwrap_or_default()
    );
    let exe = std::env::current_exe().expect("the test binary");
    vtium::helper::Command {
        program: stand_in_shell().to_path_buf(),
        args: [
            "-c".to_owned(),
            undumpable(
                "VTIUM_TEST_HELPER=\"$1\" exec \"$0\" --exact stand_in::the_helper_stand_in -q 3>&1 >/dev/null",
            ),
        ]
        .into_iter()
        .map(OsString::from)
        .chain([exe.into_os_string(), OsString::from(mode)])
        .collect(),
        account: None,
    }
}

/// What the stand-in writes to the console: each frame's kind logged, and
/// with `go`, a FIFO, nothing sent until the test opens it to write.
struct Logged {
    out: std::fs::File,
    kinds: PathBuf,
    go: Option<PathBuf>,
    pending: Vec<u8>,
}

impl std::io::Write for Logged {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if let Some(go) = self.go.take() {
            // the stand-in's only wait: opening a FIFO to read blocks until
            // the test opens it to write, once the console has stopped
            // waiting for the answer
            std::fs::File::open(&go)?;
        }
        self.pending.extend_from_slice(bytes);
        while let [k0, k1, k2, k3, l0, l1, l2, l3, ..] = *self.pending.as_slice() {
            let len = u32::from_le_bytes([l0, l1, l2, l3]) as usize;
            if self.pending.len() < 8 + len {
                break;
            }
            let kind = u32::from_le_bytes([k0, k1, k2, k3]);
            let mut log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.kinds)?;
            writeln!(log, "{kind}")?;
            self.pending.drain(..8 + len);
        }
        self.out.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

/// Not a test of the console: run by [`real_helper`] with [`STAND_IN`]
/// set, it serves as the glyph helper and exits with serve's status; run
/// by the test harness, it does nothing.
#[test]
fn the_helper_stand_in() {
    let Ok(mode) = std::env::var(STAND_IN) else {
        return;
    };
    let (kinds, go) = mode.split_once(':').expect("the stand-in's mode");
    let out = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/fd/3")
        .expect("the console's pipe");
    let mut out = Logged {
        out,
        kinds: PathBuf::from(kinds),
        go: (!go.is_empty()).then(|| PathBuf::from(go)),
        pending: Vec::new(),
    };
    let status = vtium_config::serve::serve(
        std::io::stdin(),
        &mut out,
        &vtium_config::serve::Paths::default(),
        Duration::from_secs(5),
    );
    std::process::exit(i32::from(status));
}
