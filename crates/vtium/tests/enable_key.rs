//! `vtium.enable=0` under each policy of vtium.conf (ADR-1401 D9). The key
//! turns vtium off for the boot whatever vtium.conf holds. Before vtium
//! exits, it reads the policy of a trusted vtium.conf and no more of the
//! file. Under a closed policy it writes the marker that keeps the unit's
//! getty fallback off its VT, so no login prompt starts there. Under an
//! open file, a missing file and a file vtium cannot trust or read, it
//! writes no marker, and the getty fallback gives the VT a login prompt.
//!
//! The tests run as their own user, whose files a vtium that runs
//! unprivileged trusts, under the build's own temporary directory. Each
//! test gives vtium a stand-in for `/proc/cmdline` and for `/run/vtium`.
//! The VT is a real VT's device, which vtium only stats to read its
//! numbers: it never opens the device.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use vtium::config::File;
use vtium_settings::Policy;

#[path = "common/scratch.rs"]
mod scratch;
use scratch::Scratch;

/// The first `/dev/ttyN` that is a VT's device, with its number. A stat
/// reads the node's numbers and opens nothing.
fn a_vt() -> (PathBuf, u16) {
    (1..=63)
        .find_map(|n| {
            let tty = PathBuf::from(format!("/dev/tty{n}"));
            vtium::getty::vt_number(&tty).map(|vt| (tty, vt))
        })
        .expect("this machine has a VT's device, /dev/ttyN, to stat")
}

/// A boot of one test: its configuration directory, its stand-ins for
/// `/proc/cmdline` and `/run/vtium`, and the VT the command line names.
struct Boot {
    dir: Scratch,
    tty: PathBuf,
    vt: u16,
}

/// What vtium.conf is in a test.
enum Conf<'a> {
    Missing,
    /// The text, written with the mode.
    File(&'a str, u32),
    /// A directory where the file should be, which vtium cannot read as a
    /// regular file.
    Directory,
}

impl Boot {
    /// A boot whose kernel command line is `cmdline`, with vtium.conf as
    /// `conf` says.
    fn new(name: &str, cmdline: &str, conf: &Conf<'_>) -> Boot {
        let dir = Scratch::new(&format!("enable-key-{name}"));
        let etc = dir.join("etc");
        std::fs::create_dir(&etc).expect("the configuration directory");
        std::fs::set_permissions(&etc, std::fs::Permissions::from_mode(0o700)).expect("0700");
        let file = etc.join("vtium.conf");
        match conf {
            Conf::Missing => {}
            Conf::File(text, mode) => {
                std::fs::write(&file, text).expect("vtium.conf");
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(*mode))
                    .expect("its mode");
            }
            Conf::Directory => std::fs::create_dir(&file).expect("a directory"),
        }
        std::fs::write(dir.join("cmdline"), cmdline).expect("the kernel command line");
        let (tty, vt) = a_vt();
        Boot { dir, tty, vt }
    }

    fn args(&self, vt: bool) -> Vec<OsString> {
        let mut args = vec![OsString::from(format!(
            "--configdir={}",
            self.dir.join("etc").display()
        ))];
        if vt {
            args.push(OsString::from(format!("--vt={}", self.tty.display())));
        }
        args
    }

    fn run(&self) -> PathBuf {
        self.dir.join("run-vtium")
    }

    /// The start's first check, with the VT on the command line as the
    /// unit's `--vt=%I` gives it.
    fn start(&self) -> Option<u8> {
        self.start_with(true)
    }

    fn start_with(&self, vt: bool) -> Option<u8> {
        vtium::kernel::off_at_start(&self.args(vt), &self.dir.join("cmdline"), &self.run())
    }

    /// The marker of this boot's VT, at the path the getty fallback's
    /// condition names, with the unit's instance `%i` standing for `ttyN`.
    fn marker(&self) -> PathBuf {
        let fallback = include_str!("../install/vtium-getty@.service");
        let condition = fallback
            .lines()
            .find_map(|l| l.strip_prefix("ConditionPathExists=!"))
            .expect("the fallback's condition");
        let named = condition.replace("%i", &format!("tty{}", self.vt));
        let name = Path::new(&named).file_name().expect("a file name");
        assert_eq!(
            Path::new(&named).parent(),
            Some(Path::new(vtium::getty::DIR)),
            "the condition looks where vtium writes"
        );
        self.run().join(name)
    }

    /// The policy that a start without the key reads from the same
    /// vtium.conf, so the key and the start can be compared.
    fn start_policy(&self) -> Policy {
        let loaded = vtium::config::load(&self.args(true), &mut vtium::config::read_file);
        vtium::head::rules(&loaded.path, &loaded.file)
            .outcome
            .policy
    }
}

const OFF: &str = "BOOT_IMAGE=/vmlinuz root=/dev/sda1 vtium.enable=0 quiet\n";
const CLOSED: &str = "version = 1\npolicy = closed\n[head a]\n[rules]\ndisplay * -> a\n";
const OPEN: &str = "version = 1\npolicy = open\n[head a]\n[rules]\ndisplay * -> a\n";

// covers: platform.heads.policy
#[test]
fn under_a_trusted_closed_file_the_key_writes_the_marker_and_exits() {
    let boot = Boot::new("closed", OFF, &Conf::File(CLOSED, 0o644));
    assert_eq!(boot.start(), Some(1), "vtium exits with status 1");
    assert!(
        boot.marker().is_file(),
        "the getty fallback skips its getty"
    );
    assert_eq!(boot.start_policy(), Policy::Closed, "a start agrees");
}

// A policy line vtium cannot read makes the file closed and broken (ADR-1401
// D7), and the key honors that too.
// covers: platform.heads.policy
#[test]
fn under_a_policy_line_vtium_cannot_read_the_key_writes_the_marker() {
    let text = "version = 1\npolicy = opne\n[head a]\n[rules]\ndisplay * -> a\n";
    let boot = Boot::new("unreadable-policy", OFF, &Conf::File(text, 0o644));
    assert_eq!(boot.start(), Some(1));
    assert!(boot.marker().is_file());
    assert_eq!(boot.start_policy(), Policy::Closed);
}

// The key reads the policy alone: a closed file whose sections are broken
// still writes the marker.
// covers: platform.heads.policy
#[test]
fn under_a_closed_file_with_broken_sections_the_key_writes_the_marker() {
    let text = "policy = closed\n[head a]\nfocs = yes\n[rules]\ndisplay * -> nowhere\n";
    let boot = Boot::new("closed-broken", OFF, &Conf::File(text, 0o644));
    assert_eq!(boot.start(), Some(1));
    assert!(boot.marker().is_file());
    assert_eq!(boot.start_policy(), Policy::Closed);
}

// covers: platform.heads.policy
#[test]
fn under_an_open_file_the_key_exits_without_the_marker() {
    let boot = Boot::new("open", OFF, &Conf::File(OPEN, 0o644));
    assert_eq!(boot.start(), Some(1));
    assert!(
        !boot.marker().exists(),
        "the getty fallback starts its getty"
    );
    assert_eq!(boot.start_policy(), Policy::Open);
}

// An open policy removes a marker that an earlier closed run of the boot
// left, as a start under the open policy does.
#[test]
fn under_an_open_file_the_key_removes_a_stale_marker() {
    let boot = Boot::new("open-stale", OFF, &Conf::File(OPEN, 0o644));
    std::fs::create_dir_all(boot.run()).expect("the run directory");
    std::fs::write(boot.marker(), "").expect("a stale marker");
    assert_eq!(boot.start(), Some(1));
    assert!(!boot.marker().exists());
}

#[test]
fn under_a_missing_file_the_key_exits_without_the_marker() {
    let boot = Boot::new("missing", OFF, &Conf::Missing);
    assert_eq!(boot.start(), Some(1));
    assert!(!boot.marker().exists());
    assert_eq!(boot.start_policy(), Policy::Open);
}

// A closed file that others can write fails the trust check, so vtium
// reads none of it, its policy included (ADR-1401 D7, level 1).
// covers: platform.heads.policy
#[test]
fn under_an_untrusted_closed_file_the_key_exits_without_the_marker() {
    let boot = Boot::new("untrusted", OFF, &Conf::File(CLOSED, 0o664));
    assert_eq!(boot.start(), Some(1));
    assert!(!boot.marker().exists());
    let loaded = vtium::config::load(&boot.args(true), &mut vtium::config::read_file);
    assert!(
        matches!(loaded.file, File::Untrusted(_)),
        "{:?}",
        loaded.file
    );
    assert_eq!(boot.start_policy(), Policy::Open);
}

#[test]
fn under_a_file_vtium_cannot_read_the_key_exits_without_the_marker() {
    let boot = Boot::new("directory", OFF, &Conf::Directory);
    assert_eq!(boot.start(), Some(1));
    assert!(!boot.marker().exists());
    assert_eq!(boot.start_policy(), Policy::Open);
}

// Without the key, the check writes nothing and the start goes on, to
// read vtium.conf whole and mark the VT itself.
#[test]
fn without_the_key_the_check_writes_no_marker() {
    let boot = Boot::new("on", "root=/dev/sda1 quiet\n", &Conf::File(CLOSED, 0o644));
    assert_eq!(boot.start(), None);
    assert!(!boot.run().exists());
}

// The key marks the VT the command line names, as the unit's `--vt=%I`
// does; without one it marks nothing.
#[test]
fn without_a_vt_on_the_command_line_the_key_writes_no_marker() {
    let boot = Boot::new("no-vt", OFF, &Conf::File(CLOSED, 0o644));
    assert_eq!(boot.start_with(false), Some(1));
    assert!(!boot.run().exists());
}
