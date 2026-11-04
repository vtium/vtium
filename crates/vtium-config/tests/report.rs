//! `vtium-config report` against stand-in trees: each part of the
//! document, a journal the user cannot read, a machine with no card, a
//! run as root against one as a user, `--save`, and the privacy rules
//! (ADR-1200 D24).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

use vtium_config::report::{Machine, file_name, marker, report};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// The hostname the stand-in machine has, planted in its programs' output.
const HOST: &str = "planthost";

/// The monitor's serial string and serial number, planted in its EDID and
/// in a log line.
const SERIAL: &str = "SER1AL7731";
const NUMBER: u32 = 0x0102_0304;

/// A stand-in root: `/proc`, `/etc`, `/run`, `/sys` and the programs the
/// report runs, each a shell script.
struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Tree {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("report-{name}"));
        let _ = fs::remove_dir_all(&root);
        let t = Tree { root };
        t.file("proc/sys/kernel/osrelease", "6.9.1-test\n");
        t.file("proc/sys/kernel/arch", "aarch64\n");
        t.file("proc/sys/kernel/hostname", &format!("{HOST}\n"));
        t.file("etc/hostname", &format!("{HOST}.example.org\n"));
        t.file(
            "etc/os-release",
            "NAME=\"Test Linux\"\nPRETTY_NAME=\"Test Linux 42 (Plant)\"\nVERSION_ID=42\nID=test\n",
        );
        t.dir("run/systemd/system");
        t.dir("run/systemd/seats");
        t.file(
            "etc/vtium/vtium.conf",
            "# vtium.conf\nfont-name=monoid\n\n   # an indented comment\nno-mouse\n",
        );
        t.file("etc/vtium/heads.conf", "[head desk]\n");
        t.file("etc/vtium/fonts/monoid.vtfrit", "name=monoid\n");
        t.card();
        t.program("usr/bin/vtium", &vtium_script("vtium 9.9.9", ""));
        t.program(
            "usr/local/bin/vtium-dev",
            &vtium_script(
                "vtium 9.9.10\nrevision v9.9.10-3-gabc1234\nfeatures gpu images",
                "",
            ),
        );
        t.program("usr/bin/systemctl", SYSTEMCTL);
        t.program("usr/bin/journalctl", JOURNALCTL);
        t
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn dir(&self, rel: &str) {
        fs::create_dir_all(self.path(rel)).expect("a directory");
    }

    fn file(&self, rel: &str, text: &str) {
        self.bytes(rel, text.as_bytes());
    }

    fn bytes(&self, rel: &str, bytes: &[u8]) {
        let path = self.path(rel);
        fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        fs::write(path, bytes).expect("a file");
    }

    fn program(&self, rel: &str, script: &str) {
        self.file(rel, script);
        fs::set_permissions(self.path(rel), fs::Permissions::from_mode(0o755)).expect("mode");
    }

    /// card0, driven by `testdrm`, with its render node, a connected
    /// DP-1 whose EDID names the monitor and its serial, a disconnected
    /// HDMI-A-1, and its display state in debugfs.
    fn card(&self) {
        self.dir("sys/bus/pci/drivers/testdrm");
        self.dir("sys/class/drm/card0/device/drm/card0");
        self.dir("sys/class/drm/card0/device/drm/renderD128");
        std::os::unix::fs::symlink(
            "../../../../bus/pci/drivers/testdrm",
            self.path("sys/class/drm/card0/device/driver"),
        )
        .expect("the driver's link");
        self.file("sys/class/drm/card0-DP-1/status", "connected\n");
        self.file("sys/class/drm/card0-DP-1/enabled", "enabled\n");
        self.file("sys/class/drm/card0-DP-1/modes", "2560x1440\n1920x1080\n");
        self.bytes("sys/class/drm/card0-DP-1/edid", &edid());
        self.file("sys/class/drm/card0-HDMI-A-1/status", "disconnected\n");
        self.file("sys/class/drm/card0-HDMI-A-1/enabled", "disabled\n");
        self.file("sys/class/drm/card0-HDMI-A-1/modes", "");
        self.file("sys/class/drm/card0-HDMI-A-1/edid", "");
        self.file(
            "sys/kernel/debug/dri/0/state",
            "plane[31]: plane-0\n\tcrtc=crtc-0\ncrtc[80]: crtc-0\n\tactive=1\n",
        );
    }

    fn machine(&self, euid: u32) -> Machine {
        Machine {
            root: self.root.clone(),
            euid,
            lines: 300,
            // 2026-11-03 20:30:44 UTC
            now: SystemTime::UNIX_EPOCH + Duration::from_secs(1_793_737_844),
        }
    }
}

/// A stand-in vtium: `--version` prints `version`, `--help` names its
/// subcommands, and `console status` and `check-config` answer as vtium
/// does. `extra` is more of the script, such as a marker line.
fn vtium_script(version: &str, extra: &str) -> String {
    format!(
        r#"#!/bin/sh
{extra}
case "$1" in
--version) printf '%s\n' "{version}" ;;
--help) printf 'Usage: vtium [OPTION]...\n       vtium check-config [OPTION]... [FILE]\n       vtium console status                    show what each VT runs\n' ;;
console) printf 'VT  at boot  when switched to  running\n2   getty recorded  vtium@tty2  vtium@tty2\n7   -  -  graphical session of alice on {HOST}\n' ;;
check-config) printf '/etc/vtium/vtium.conf:3: unknown key frob\n' ; exit 1 ;;
*) echo "STARTED AS A CONSOLE" ; exit 9 ;;
esac
"#
    )
}

/// A stand-in systemctl with vtium@tty2 running and vtium-getty@tty3
/// failed.
const SYSTEMCTL: &str = r#"#!/bin/sh
case "$1" in
list-units)
  printf 'vtium@tty2.service loaded active running vtium on tty2\n'
  printf 'vtium-getty@tty3.service loaded failed failed a getty where vtium failed\n' ;;
show)
  for unit in "$@"; do :; done
  case "$unit" in
  vtium@tty2.service) printf 'ActiveState=active\nSubState=running\nActiveEnterTimestamp=Tue 2026-11-03 20:27:03 UTC\nNRestarts=0\n' ;;
  vtium-getty@tty3.service) printf 'ActiveState=failed\nSubState=failed\nActiveEnterTimestamp=\nNRestarts=2\n' ;;
  *) printf 'ActiveState=inactive\nSubState=dead\nActiveEnterTimestamp=\nNRestarts=0\n' ;;
  esac ;;
*) echo "systemctl: no such stand-in verb $1" >&2 ; exit 1 ;;
esac
"#;

/// A stand-in journalctl. `-F` names the units the journal holds,
/// vtium-dev@tty5 among them, which no unit list names; `-g` finds the
/// revision line; otherwise it prints a unit's log, with the hostname,
/// the monitor's serial and a user's name planted in it, and the
/// arguments it was given.
const JOURNALCTL: &str = r#"#!/bin/sh
unit=""
prev=""
for a in "$@"; do
  if [ "$prev" = "-u" ]; then unit="$a"; fi
  prev="$a"
done
case " $* " in
*" -F "*) printf 'vtium@tty2.service\nvtium-dev@tty5.service\nvtium@tty9.service\nsshd.service\n' ; exit 0 ;;
*" -g "*)
  case "$unit" in
  vtium@tty2.service) printf 'log: vtium Revision v1.2.3\n' ;;
  vtium-dev@tty5.service) printf 'log: vtium Revision v9.9.10-3-gabc1234\n' ;;
  esac
  exit 0 ;;
esac
case "$unit" in
vtium@tty9.service) printf -- '-- No entries --\n' ;;
*)
  printf 'args: %s\n' "$*"
  printf '2026-11-03T20:27:03+00:00 vtium[100]: log: vtium Revision v1.2.3 on %s\n' "$unit"
  printf '2026-11-03T20:27:04+00:00 vtium[100]: display DP-1 is DEL:0x4321:SER1AL7731 (0x01020304, 16909060)\n'
  printf '2026-11-03T20:27:05+00:00 login[200]: pam_unix(login:session): session opened for user alice\n'
  printf '2026-11-03T20:27:06+00:00 vtium[100]: PLANTHOST.example.org said hello to planthost\n' ;;
esac
"#;

/// A journalctl run by a user who cannot read the system's journal, as
/// journalctl answers that user.
const JOURNALCTL_UNREADABLE: &str = r#"#!/bin/sh
echo "Hint: You are currently not seeing messages from other users and the system." >&2
echo "      Users in groups 'adm', 'systemd-journal', 'wheel' can see all messages." >&2
echo "-- No entries --"
"#;

/// An EDID base block from maker DEL, product 0x4321, serial number
/// [`NUMBER`], 60 by 34 cm, naming the monitor TESTMON and the serial
/// string [`SERIAL`], its checksum made good.
fn edid() -> Vec<u8> {
    let mut b = vec![0u8; 128];
    b.splice(0..8, [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
    // DEL: D=4, E=5, L=12 in five bits each
    let maker: u16 = (4 << 10) | (5 << 5) | 12;
    b.splice(8..10, maker.to_be_bytes());
    b.splice(10..12, 0x4321u16.to_le_bytes());
    b.splice(12..16, NUMBER.to_le_bytes());
    b.splice(21..23, [60, 34]);
    let descriptor = |tag: u8, text: &[u8]| {
        let mut d = vec![0, 0, 0, tag, 0];
        let mut t = text.to_vec();
        t.push(b'\n');
        t.resize(13, b' ');
        d.extend(t);
        d
    };
    b.splice(72..90, descriptor(0xfc, b"TESTMON"));
    b.splice(90..108, descriptor(0xff, SERIAL.as_bytes()));
    b.truncate(128);
    let sum = b.iter().fold(0u8, |s, &x| s.wrapping_add(x));
    if let Some(last) = b.last_mut() {
        *last = 0u8.wrapping_sub(sum);
    }
    b
}

/// The text under the heading `name`, up to the next heading.
fn part<'a>(doc: &'a str, name: &str) -> &'a str {
    let rule = "-".repeat(name.chars().count());
    let head = format!("\n{name}\n{rule}\n");
    let start = doc
        .find(&head)
        .unwrap_or_else(|| panic!("no part {name:?} in:\n{doc}"))
        + head.len();
    let rest = &doc[start..];
    // the next heading is a line followed by a rule of dashes
    let lines: Vec<&str> = rest.split_inclusive('\n').collect();
    let mut end = rest.len();
    let mut at = 0;
    for pair in lines.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let b = b.trim_end();
        if !b.is_empty() && b.chars().all(|c| c == '-') && b.len() == a.trim_end().chars().count() {
            end = at;
            break;
        }
        at += a.len();
    }
    &rest[..end]
}

const PARTS: [&str; 8] = [
    "What this report holds",
    "vtium",
    "System",
    "Consoles",
    "Displays",
    "Configuration",
    "Logs",
    "Kernel display state",
];

#[test]
fn the_parts_come_in_order_under_plain_headings() {
    let doc = report(&Tree::new("order").machine(0));
    let mut at = 0;
    for name in PARTS {
        let head = format!("\n{name}\n{}\n", "-".repeat(name.len()));
        let found = doc[at..]
            .find(&head)
            .unwrap_or_else(|| panic!("{name:?} after byte {at} in:\n{doc}"));
        at += found + head.len();
    }
    assert!(doc.starts_with("vtium report\n"), "{doc}");
    assert!(
        doc.contains("2026-11-03 20:30:44 UTC"),
        "the time it was made: {doc}"
    );
}

#[test]
fn the_vtium_part_names_each_binary_its_version_and_its_features() {
    let doc = report(&Tree::new("vtium").machine(1000));
    let p = part(&doc, "vtium");
    assert!(p.contains("/usr/bin/vtium\n"), "{p}");
    assert!(p.contains("version: vtium 9.9.9\n"), "{p}");
    // no features line in --version and no marker: each is read from the
    // binary, and gpu has no marker
    assert!(p.contains("images no, logind no, gpu unknown"), "{p}");
    assert!(p.contains("/usr/local/bin/vtium-dev\n"), "{p}");
    assert!(p.contains("version: vtium 9.9.10\n"), "{p}");
    assert!(p.contains("revision: v9.9.10-3-gabc1234\n"), "{p}");
    assert!(
        p.contains("features: gpu images, as its --version lists them\n"),
        "{p}"
    );
    assert!(
        p.contains(concat!("vtium-config: ", env!("CARGO_PKG_VERSION"))),
        "{p}"
    );
}

#[test]
fn a_binary_s_features_are_read_from_its_marker_text() {
    let t = Tree::new("markers");
    // each marker as a comment of the script, joined here so that this
    // source never holds the joined line
    let comments: String = vtium_config::report::MARKERS
        .iter()
        .map(|(_, cut)| format!("# {}%s\n", marker(cut)))
        .collect();
    t.program("usr/bin/vtium", &vtium_script("vtium 9.9.9", &comments));
    let p = report(&t.machine(1000));
    let p = part(&p, "vtium");
    assert!(p.contains("images yes, logind yes, gpu unknown"), "{p}");
}

/// The markers are lines the console's code carries only in a build with
/// the feature; each test here holds a marker to the file it comes from,
/// so a change to that line fails here first.
#[test]
fn each_marker_is_text_its_feature_s_code_carries() {
    let image = include_str!("../../vtium-platform/src/image_helper/start.rs");
    let logind = include_str!("../../vtium-platform/src/logind.rs");
    for (feature, cut) in vtium_config::report::MARKERS {
        let marker = marker(cut);
        let source = match feature {
            "images" => image,
            "logind" => logind,
            other => panic!("no source named for the marker of {other}"),
        };
        assert!(source.contains(&marker), "{feature}: {marker:?}");
    }
}

#[test]
fn a_missing_binary_is_named_and_the_dev_install_left_out_when_absent() {
    let t = Tree::new("no-binary");
    fs::remove_file(t.path("usr/bin/vtium")).expect("removed");
    fs::remove_file(t.path("usr/local/bin/vtium-dev")).expect("removed");
    let doc = report(&t.machine(1000));
    let p = part(&doc, "vtium");
    assert!(p.contains("/usr/bin/vtium: not installed"), "{p}");
    assert!(!p.contains("vtium-dev"), "{p}");
    let c = part(&doc, "Consoles");
    assert!(c.contains("/usr/bin/vtium is not installed"), "{c}");
    let conf = part(&doc, "Configuration");
    assert!(conf.contains("/usr/bin/vtium is not installed"), "{conf}");
}

#[test]
fn the_system_part_reads_the_kernel_the_distribution_and_systemd() {
    let t = Tree::new("system");
    let doc = report(&t.machine(1000));
    let p = part(&doc, "System");
    assert!(p.contains("kernel: 6.9.1-test\n"), "{p}");
    assert!(p.contains("architecture: aarch64\n"), "{p}");
    assert!(p.contains("distribution: Test Linux, version 42\n"), "{p}");
    assert!(p.contains("systemd: runs as the init system\n"), "{p}");
    assert!(p.contains("logind: runs\n"), "{p}");

    fs::remove_dir_all(t.path("run/systemd")).expect("removed");
    let doc = report(&t.machine(1000));
    let p = part(&doc, "System");
    assert!(
        p.contains("systemd: does not run as the init system\n"),
        "{p}"
    );
    assert!(p.contains("logind: does not run\n"), "{p}");
}

#[test]
fn the_consoles_part_holds_console_status_and_each_unit_s_state() {
    let doc = report(&Tree::new("consoles").machine(1000));
    let p = part(&doc, "Consoles");
    assert!(
        p.contains("    2   getty recorded  vtium@tty2  vtium@tty2\n"),
        "{p}"
    );
    assert!(
        p.contains(
            "vtium@tty2.service: active (running) since Tue 2026-11-03 20:27:03 UTC, restarted 0 times, revision v1.2.3\n"
        ),
        "{p}"
    );
    assert!(
        p.contains("vtium-getty@tty3.service: failed (failed), restarted 2 times\n"),
        "{p}"
    );
    // a unit only the journal names, with lines this boot, is listed; one
    // with none this boot is not, and neither is another program's
    assert!(p.contains("vtium-dev@tty5.service: inactive (dead)"), "{p}");
    assert!(!p.contains("tty9"), "{p}");
    assert!(!p.contains("sshd"), "{p}");
}

#[test]
fn an_older_vtium_is_never_given_a_subcommand_its_help_does_not_name() {
    let t = Tree::new("old-vtium");
    t.program(
        "usr/bin/vtium",
        "#!/bin/sh\ncase \"$1\" in\n--version) echo 'vtium 0.0.1' ;;\n--help) echo 'Usage: vtium [OPTION]...' ;;\n*) echo 'STARTED AS A CONSOLE' ; exit 9 ;;\nesac\n",
    );
    let doc = report(&t.machine(1000));
    assert!(!doc.contains("STARTED AS A CONSOLE"), "{doc}");
    assert!(
        part(&doc, "Consoles").contains("this vtium has no console status"),
        "{doc}"
    );
    assert!(
        part(&doc, "Configuration").contains("this vtium has no check-config"),
        "{doc}"
    );
}

#[test]
fn the_displays_part_names_each_card_and_connector() {
    let doc = report(&Tree::new("displays").machine(1000));
    let p = part(&doc, "Displays");
    assert!(
        p.contains("card0: driver testdrm, render node renderD128\n"),
        "{p}"
    );
    assert!(
        p.contains(
            "  DP-1: connected, enabled, preferred mode 2560x1440, monitor TESTMON, 600x340 mm\n"
        ),
        "{p}"
    );
    assert!(p.contains("  HDMI-A-1: disconnected, disabled\n"), "{p}");
}

#[test]
fn a_machine_with_no_card_says_so() {
    let t = Tree::new("no-card");
    fs::remove_dir_all(t.path("sys/class/drm")).expect("removed");
    fs::create_dir_all(t.path("sys/class/drm")).expect("an empty class");
    let doc = report(&t.machine(0));
    assert!(
        part(&doc, "Displays").contains("No DRM card is in /sys/class/drm."),
        "{doc}"
    );
    assert!(
        part(&doc, "Kernel display state").contains("No DRM card is in /sys/class/drm."),
        "{doc}"
    );
}

#[test]
fn a_card_with_no_driver_or_render_node_or_state_says_what_it_lacks() {
    let t = Tree::new("bare-card");
    fs::remove_file(t.path("sys/class/drm/card0/device/driver")).expect("removed");
    fs::remove_dir_all(t.path("sys/class/drm/card0/device/drm/renderD128")).expect("removed");
    fs::remove_dir_all(t.path("sys/kernel/debug")).expect("removed");
    let doc = report(&t.machine(0));
    let p = part(&doc, "Displays");
    assert!(p.contains("card0: no driver, no render node\n"), "{p}");
    let k = part(&doc, "Kernel display state");
    assert!(
        k.contains("card0: cannot read /sys/kernel/debug/dri/0/state"),
        "{k}"
    );
}

#[test]
fn the_configuration_part_leaves_out_comments_and_names_the_files() {
    let doc = report(&Tree::new("configuration").machine(1000));
    let p = part(&doc, "Configuration");
    assert!(p.contains("    font-name=monoid\n    no-mouse\n"), "{p}");
    assert!(!p.contains("# vtium.conf"), "{p}");
    assert!(!p.contains("an indented comment"), "{p}");
    assert!(
        p.contains("Files in /etc/vtium: fonts/, heads.conf, vtium.conf\n"),
        "{p}"
    );
    assert!(
        p.contains("Files in /etc/vtium/fonts: monoid.vtfrit\n"),
        "{p}"
    );
    assert!(p.contains("vtium check-config exits 1:\n"), "{p}");
    assert!(
        p.contains("    /etc/vtium/vtium.conf:3: unknown key frob\n"),
        "{p}"
    );
}

#[test]
fn no_vtium_conf_says_vtium_runs_with_its_defaults() {
    let t = Tree::new("no-conf");
    fs::remove_file(t.path("etc/vtium/vtium.conf")).expect("removed");
    let doc = report(&t.machine(1000));
    let p = part(&doc, "Configuration");
    assert!(
        p.contains("There is no /etc/vtium/vtium.conf, so vtium runs with its defaults."),
        "{p}"
    );
}

#[test]
fn the_logs_part_holds_each_unit_s_latest_lines() {
    let t = Tree::new("logs");
    let mut m = t.machine(1000);
    m.lines = 7;
    let doc = report(&m);
    let p = part(&doc, "Logs");
    assert!(
        p.contains("vtium@tty2.service, its last 7 lines this boot:\n"),
        "{p}"
    );
    assert!(
        p.contains("-b -u vtium@tty2.service -n 7 -o short-iso --no-hostname --no-pager"),
        "{p}"
    );
    assert!(
        p.contains("vtium-dev@tty5.service, its last 7 lines"),
        "{p}"
    );
    assert!(
        p.contains("vtium-getty@tty3.service, its last 7 lines"),
        "{p}"
    );
    assert!(p.contains("session opened for user alice"), "{p}");
}

#[test]
fn an_unreadable_journal_says_how_to_read_it() {
    let t = Tree::new("unreadable-journal");
    t.program("usr/bin/journalctl", JOURNALCTL_UNREADABLE);
    let doc = report(&t.machine(1000));
    let p = part(&doc, "Logs");
    assert!(p.contains("The journal is not readable to you."), "{p}");
    assert!(p.contains("sudo vtium-config report"), "{p}");
    assert!(p.contains("systemd-journal"), "{p}");
    assert!(!p.contains("No entries"), "{p}");
    // the units systemctl lists are still named in the consoles part
    assert!(
        part(&doc, "Consoles").contains("vtium@tty2.service: active (running)"),
        "{doc}"
    );
}

#[test]
fn as_root_the_kernel_display_state_is_read_and_as_a_user_it_names_sudo() {
    let t = Tree::new("root-user");
    let root = report(&t.machine(0));
    let k = part(&root, "Kernel display state");
    assert!(
        k.contains("card0, from /sys/kernel/debug/dri/0/state:\n    plane[31]: plane-0\n"),
        "{k}"
    );
    assert!(root.contains("run as root"), "{root}");

    let user = report(&t.machine(1000));
    let k = part(&user, "Kernel display state");
    assert!(k.contains("needs root: sudo vtium-config report"), "{k}");
    assert!(!k.contains("plane[31]"), "{k}");
    assert!(user.contains("run as a user"), "{user}");
}

#[test]
fn neither_the_hostname_nor_a_serial_appears() {
    let doc = report(&Tree::new("privacy").machine(0));
    for planted in [
        HOST,
        "PLANTHOST",
        "example.org",
        SERIAL,
        "0x01020304",
        "01020304",
        "16909060",
    ] {
        assert!(!doc.contains(planted), "{planted:?} in:\n{doc}");
    }
    assert!(doc.contains("[hostname]"), "{doc}");
    assert!(doc.contains("[serial]"), "{doc}");
    // the raw EDID never appears, in bytes or in hex
    assert!(!doc.contains("00ffffffffffff00"), "{doc}");
    let what = part(&doc, "What this report holds");
    assert!(what.contains("user"), "{what}");
    assert!(what.contains("before"), "{what}");
}

#[test]
fn a_saved_report_is_named_for_the_time_in_utc() {
    let at = |s: u64| file_name(SystemTime::UNIX_EPOCH + Duration::from_secs(s));
    assert_eq!(at(1_793_737_844), "vtium-report-20261103-203044.txt");
    assert_eq!(at(0), "vtium-report-19700101-000000.txt");
    // 2024-02-29 23:59:59, a leap day
    assert_eq!(at(1_709_251_199), "vtium-report-20240229-235959.txt");
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("vtium-config runs")
}

#[test]
fn the_command_prints_to_standard_output_by_default() {
    let t = Tree::new("cli-stdout");
    let root = t.root.to_str().expect("UTF-8");
    let o = run(&t.root, &["--root", root, "report", "--lines", "5"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(out.starts_with("vtium report\n"), "{out}");
    assert!(out.contains("its last 5 lines"), "{out}");
}

#[test]
fn save_with_no_path_writes_a_new_file_here_and_prints_its_path() {
    let t = Tree::new("cli-save");
    let here = t.path("cwd");
    t.dir("cwd");
    let root = t.root.to_str().expect("UTF-8");
    let o = run(&here, &["--root", root, "report", "--save"]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let printed = String::from_utf8_lossy(&o.stdout).trim_end().to_owned();
    let path = PathBuf::from(&printed);
    assert!(path.is_absolute(), "{printed}");
    assert_eq!(
        path.parent(),
        Some(here.canonicalize().expect("cwd").as_path())
    );
    let name = path.file_name().and_then(|n| n.to_str()).expect("a name");
    assert!(
        name.len() == "vtium-report-YYYYMMDD-HHMMSS.txt".len()
            && name.starts_with("vtium-report-")
            && name.ends_with(".txt"),
        "{name}"
    );
    let text = fs::read_to_string(&path).expect("the report");
    assert!(text.starts_with("vtium report\n"), "{text}");
    let mode = fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "{mode:o}");
}

#[test]
fn save_with_a_path_writes_there_and_never_replaces_a_file() {
    let t = Tree::new("cli-save-path");
    let root = t.root.to_str().expect("UTF-8");
    let target = t.path("out/mine.txt");
    t.dir("out");
    let arg = format!("--save={}", target.display());
    let o = run(&t.root, &["--root", root, "report", &arg]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&o.stdout).trim_end(),
        target.display().to_string()
    );
    assert!(
        fs::read_to_string(&target)
            .expect("saved")
            .starts_with("vtium report\n")
    );

    fs::write(&target, "kept\n").expect("a file of the user's");
    let o = run(&t.root, &["--root", root, "report", &arg]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("mine.txt"));
    assert_eq!(fs::read_to_string(&target).expect("kept"), "kept\n");
}

#[test]
fn report_usage_errors_exit_2() {
    let t = Tree::new("cli-usage");
    let root = t.root.to_str().expect("UTF-8");
    for args in [
        &["report", "extra"][..],
        &["report", "--lines"],
        &["report", "--lines", "x"],
        &["report", "--save="],
        &["report", "--frob"],
    ] {
        let mut all = vec!["--root", root];
        all.extend_from_slice(args);
        let o = run(&t.root, &all);
        assert_eq!(
            o.status.code(),
            Some(2),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
}
