//! The active resources beside a draft (ADR-1205 D4): `activate`,
//! `deactivate` and `active`, run as the binary against a fixture whose
//! configuration directory stands in for `/etc/vtium`, so the install runs
//! `apply` as the user and writes the fixture's `batch.conf`. The stand-in
//! `vtium` passes every vtium.conf.

#[path = "common/home.rs"]
mod home;
use vtium_testscript as script;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// The stand-in console: `check-config` passes every file.
const CHECKER: &str = "#!/bin/sh\n[ \"$1\" = check-config ] || exit 9\nexit 0\n";

const INSTALLED: &str = "font-size=12\nversion = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n";

struct Fixture {
    root: PathBuf,
}

struct Ran {
    status: i32,
    out: String,
    err: String,
}

fn fixture(name: &str) -> Fixture {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("batch")
        .join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("a fresh fixture");
    }
    for dir in ["etc/fonts", "home", "sys", "udev/data", "res"] {
        std::fs::create_dir_all(root.join(dir)).expect("the fixture");
    }
    std::fs::write(root.join("etc/vtium.conf"), INSTALLED).expect("vtium.conf");
    std::fs::write(root.join("res/mine.ttf"), "a font\n").expect("a font");
    std::fs::write(root.join("res/other.ttf"), "a font\n").expect("a font");
    script::executable(&root.join("vtium"), CHECKER);
    Fixture { root }
}

impl Fixture {
    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p)
    }

    /// `p` under the fixture, as a command's argument.
    fn arg(&self, p: &str) -> String {
        self.path(p).to_string_lossy().into_owned()
    }

    fn run(&self, args: &[&str]) -> Ran {
        let mut c: Command = home::command(BIN);
        c.arg("--configdir")
            .arg(self.path("etc"))
            .arg("--frits")
            .arg(self.path("etc/fonts"))
            .arg("--vtium")
            .arg(self.path("vtium"))
            .arg("--sys")
            .arg(self.path("sys"))
            .arg("--udev-data")
            .arg(self.path("udev/data"))
            .args(args)
            .env("HOME", self.path("home"))
            .env("XDG_CONFIG_HOME", self.path("home/config"))
            .env("XDG_STATE_HOME", self.path("home/state"))
            .stdin(Stdio::null());
        let o = c.output().expect("the binary runs");
        let root = self.root.to_str().expect("a path");
        Ran {
            status: o.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&o.stdout).replace(root, "ROOT"),
            err: String::from_utf8_lossy(&o.stderr).replace(root, "ROOT"),
        }
    }

    /// Runs `args` and fails the test unless the command exits 0.
    fn ok(&self, args: &[&str]) -> Ran {
        let ran = self.run(args);
        assert_eq!(ran.status, 0, "{args:?}\n{}{}", ran.out, ran.err);
        ran
    }

    /// The draft `desk`, made from the installed file and made current.
    fn desk(&self) {
        self.ok(&["draft", "new", "desk"]);
    }

    /// The file that keeps a draft's changes to the active resources.
    fn changes(&self, draft: &str) -> PathBuf {
        self.path(&format!("home/config/vtium/drafts/{draft}.batch"))
    }

    fn batch(&self) -> Option<String> {
        std::fs::read_to_string(self.path("etc/batch.conf")).ok()
    }

    /// The paths the installed batch file lists.
    fn listed(&self) -> Vec<String> {
        self.batch()
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned)
            .collect()
    }
}

/// Each font file of a fresh install, as the batch file lists it.
fn fresh_fonts() -> Vec<String> {
    vtium_settings::batch::FRESH_FONTS
        .iter()
        .map(|f| format!("{}/{f}", vtium_settings::batch::PACKAGE_FONTS))
        .collect()
}

#[test]
fn an_activation_waits_in_the_draft_and_installs_with_it() {
    let f = fixture("activate");
    f.desk();
    let mine = f.arg("res/mine.ttf");
    let ran = f.ok(&["activate", &mine]);
    assert_eq!(
        ran.out, "activate\tfont\tROOT/res/mine.ttf\n",
        "{}",
        ran.err
    );
    // the change waits beside the draft, and nothing is installed yet
    assert!(f.changes("desk").exists());
    assert_eq!(f.batch(), None);
    let active = f.ok(&["active"]);
    assert!(
        active
            .out
            .contains("font\tROOT/res/mine.ttf\tactivates on install\n"),
        "{}",
        active.out
    );
    assert!(
        active.out.starts_with("font\tbuilt-in\talways active\n"),
        "{}",
        active.out
    );
    // the install writes the batch file and leaves the draft with no change
    let install = f.ok(&["install"]);
    assert!(
        install.out.contains("wrote\tROOT/etc/batch.conf"),
        "{}",
        install.out
    );
    assert!(f.listed().contains(&mine), "{:?}", f.batch());
    assert!(!f.changes("desk").exists());
    let mode = std::fs::metadata(f.path("etc/batch.conf"))
        .expect("the batch file")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o644);
    let active = f.ok(&["active"]);
    assert!(
        active.out.contains("font\tROOT/res/mine.ttf\tactive\n"),
        "{}",
        active.out
    );
    // a deactivation installs the same way, and keeps every other line
    let before = f.listed();
    let ran = f.ok(&["deactivate", &mine]);
    assert_eq!(
        ran.out, "deactivate\tfont\tROOT/res/mine.ttf\n",
        "{}",
        ran.err
    );
    assert_eq!(f.listed(), before);
    f.ok(&["install"]);
    let mut after = before.clone();
    after.retain(|p| *p != mine);
    assert_eq!(f.listed(), after);
    assert!(!f.changes("desk").exists());
}

#[test]
fn a_missing_batch_file_starts_from_the_list_of_a_fresh_install() {
    let f = fixture("fresh");
    f.desk();
    assert_eq!(f.batch(), None);
    f.ok(&["activate", &f.arg("res/mine.ttf")]);
    f.ok(&["install"]);
    let listed = f.listed();
    for font in fresh_fonts() {
        assert!(listed.contains(&font), "{font} missing from {listed:?}");
    }
    assert_eq!(listed.last(), Some(&f.arg("res/mine.ttf")));
    // the file says what it is
    assert!(
        f.batch().expect("the batch file").starts_with("# "),
        "{:?}",
        f.batch()
    );

    // deactivating a font of a fresh install also writes the whole list
    // less that font
    let g = fixture("fresh-deactivate");
    g.desk();
    let terminus = fresh_fonts().into_iter().next().expect("a fresh font");
    g.ok(&["deactivate", &terminus]);
    g.ok(&["install"]);
    let listed = g.listed();
    assert!(!listed.contains(&terminus), "{listed:?}");
    for font in fresh_fonts().into_iter().skip(1) {
        assert!(listed.contains(&font), "{font} missing from {listed:?}");
    }

    // activating what a fresh install already holds changes nothing, so
    // the install writes no batch file
    let h = fixture("fresh-same");
    h.desk();
    let ran = h.ok(&["activate", &fresh_fonts()[1]]);
    assert!(ran.out.contains("already active"), "{}", ran.out);
    assert!(!h.changes("desk").exists());
}

#[test]
fn a_draft_that_never_touched_the_active_resources_leaves_the_batch_file_alone() {
    let f = fixture("untouched");
    let by_hand = "# root's own list\n/usr/share/vtium/fonts/ter-u16n.bdf.gz\n";
    std::fs::write(f.path("etc/batch.conf"), by_hand).expect("a batch file");
    f.desk();
    f.ok(&["set", "font-size", "14"]);
    let install = f.ok(&["install"]);
    assert!(!install.out.contains("batch.conf"), "{}", install.out);
    assert_eq!(f.batch().as_deref(), Some(by_hand));
}

#[test]
fn an_activation_keeps_the_hand_written_lines_of_the_batch_file() {
    let f = fixture("keep-lines");
    let by_hand = "# root's own list\n\n/usr/share/vtium/fonts/ter-u16n.bdf.gz\n";
    std::fs::write(f.path("etc/batch.conf"), by_hand).expect("a batch file");
    f.desk();
    f.ok(&["activate", &f.arg("res/mine.ttf")]);
    f.ok(&["install"]);
    assert_eq!(
        f.batch().expect("the batch file"),
        format!("{by_hand}{}\n", f.arg("res/mine.ttf"))
    );
}

#[test]
fn the_built_in_font_cannot_be_deactivated() {
    let f = fixture("builtin");
    f.desk();
    for word in ["unifont", "unscii-8", "builtin:unifont", "builtin"] {
        let ran = f.run(&["deactivate", word]);
        assert_eq!(ran.status, 1, "{word}: {}{}", ran.out, ran.err);
        assert!(
            ran.err
                .contains("The built-in font is always active, so it cannot be deactivated."),
            "{word}: {}",
            ran.err
        );
    }
    assert!(!f.changes("desk").exists());
}

#[test]
fn an_activation_that_fails_the_trust_rule_is_refused_and_nothing_is_kept() {
    let f = fixture("untrusted");
    f.desk();
    let open = f.path("res/open.ttf");
    std::fs::write(&open, "a font\n").expect("a font");
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o664)).expect("its mode");
    let ran = f.run(&["activate", &f.arg("res/mine.ttf"), &f.arg("res/open.ttf")]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.err
            .contains("ROOT/res/open.ttf: vtium treats this file as missing"),
        "{}",
        ran.err
    );
    assert!(!f.changes("desk").exists(), "nothing is kept");
    std::os::unix::fs::symlink(f.path("res/mine.ttf"), f.path("res/link.ttf")).expect("a link");
    let ran = f.run(&["activate", &f.arg("res/link.ttf")]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    for bad in ["relative.ttf", "/a/../b.ttf"] {
        let ran = f.run(&["activate", bad]);
        assert_eq!(ran.status, 2, "{bad}: {}{}", ran.out, ran.err);
    }
    assert!(!f.changes("desk").exists(), "nothing is kept");
}

#[test]
fn apply_checks_each_path_the_install_activates() {
    let f = fixture("apply-checks");
    f.desk();
    f.ok(&["activate", &f.arg("res/mine.ttf")]);
    // the file becomes one others can write after it was activated
    std::fs::set_permissions(
        f.path("res/mine.ttf"),
        std::fs::Permissions::from_mode(0o666),
    )
    .expect("its mode");
    let ran = f.run(&["install"]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.out
            .contains("ROOT/res/mine.ttf: vtium treats this file as missing")
            || ran
                .err
                .contains("ROOT/res/mine.ttf: vtium treats this file as missing"),
        "{}{}",
        ran.out,
        ran.err
    );
    assert_eq!(f.batch(), None);
    assert!(f.changes("desk").exists(), "the draft keeps its change");
}

#[test]
fn a_draft_s_changes_to_the_active_resources_go_with_the_draft() {
    let f = fixture("with-draft");
    f.desk();
    f.ok(&["activate", &f.arg("res/mine.ttf")]);
    // a copy of the draft takes its changes along
    f.ok(&["draft", "new", "copy", "--from", "desk"]);
    let lines = |draft: &str| -> Vec<String> {
        std::fs::read_to_string(f.changes(draft))
            .expect("the draft's changes")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(
        lines("copy"),
        [format!("activate {}", f.arg("res/mine.ttf"))]
    );
    assert_eq!(lines("copy"), lines("desk"));
    // a draft made from the installed file has none
    f.ok(&["draft", "new", "plain"]);
    assert!(!f.changes("plain").exists());
    // removing a draft removes its changes
    f.ok(&["draft", "rm", "copy"]);
    assert!(!f.changes("copy").exists());
    assert!(f.changes("desk").exists());
}

#[test]
fn deactivating_a_font_that_a_look_uses_warns_and_names_the_look() {
    let f = fixture("warn");
    let mine = f.arg("res/mine.ttf");
    std::fs::write(f.path("etc/batch.conf"), format!("{mine}\n")).expect("a batch file");
    std::fs::write(
        f.path("etc/fonts/monoid.vtfrit"),
        format!("source=outline:{mine}\n"),
    )
    .expect("a frit");
    std::fs::write(
        f.path("etc/vtium.conf"),
        format!("{INSTALLED}[look big]\nfont-name = monoid\n"),
    )
    .expect("vtium.conf");
    f.desk();
    let ran = f.ok(&["deactivate", &mine]);
    assert!(
        ran.out.contains(
            "Look big uses this font through the font set monoid. Once this change is installed, look big draws without it."
        ),
        "{}",
        ran.out
    );
    // a font no look uses deactivates without a warning
    let g = fixture("no-warn");
    let other = g.arg("res/other.ttf");
    std::fs::write(g.path("etc/batch.conf"), format!("{other}\n")).expect("a batch file");
    g.desk();
    let ran = g.ok(&["deactivate", &other]);
    assert_eq!(ran.out, "deactivate\tfont\tROOT/res/other.ttf\n");
    // a resource that is not active has nothing to deactivate
    let ran = g.run(&["deactivate", &g.arg("res/mine.ttf")]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
}
