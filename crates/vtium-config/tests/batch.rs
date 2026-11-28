//! The active resources beside a draft: `activate`, `deactivate` and
//! `active`, run as the binary against a fixture whose configuration
//! directory stands in for `/etc/vtium`, so the install runs `apply` as
//! the user and writes the fixture's `batch.conf`. The stand-in `vtium`
//! passes every vtium.conf.
//!
//! References:
//! - ADR-1205 D4: the batch, its file, the trust rule, and a draft keeping its changes to the batch beside it.

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

/// `activate --dry-run` says what it would change and that it saved
/// nothing, and writes nothing. A real `activate` ends with the draft's
/// state line, which counts the change that waits for the install.
#[test]
fn a_dry_run_saves_nothing_and_says_so_and_a_real_run_prints_the_state() {
    let f = fixture("activate-dry");
    f.desk();
    let mine = f.arg("res/mine.ttf");
    let dry = f.ok(&["activate", &mine, "--dry-run"]);
    assert_eq!(
        dry.out,
        "activate\tfont\tROOT/res/mine.ttf\nnothing was saved (--dry-run)\n"
    );
    assert!(!f.changes("desk").exists());
    let ran = f.ok(&["activate", &mine]);
    assert_eq!(
        ran.out,
        "activate\tfont\tROOT/res/mine.ttf\ndraft desk: 1 change to the active resources · no problems\n"
    );
    assert!(f.changes("desk").exists());
}

#[test]
fn an_activation_waits_in_the_draft_and_installs_with_it() {
    let f = fixture("activate");
    f.desk();
    let mine = f.arg("res/mine.ttf");
    let ran = f.ok(&["activate", &mine]);
    assert_eq!(
        ran.out,
        "activate\tfont\tROOT/res/mine.ttf\ndraft desk: 1 change to the active resources · no problems\n",
        "{}",
        ran.err
    );
    // a second activation says the draft already makes it
    let again = f.ok(&["activate", &mine]);
    assert_eq!(
        again.out,
        "already activates on install\tfont\tROOT/res/mine.ttf\n"
    );
    // the change waits beside the draft, and nothing is installed yet
    assert!(f.changes("desk").exists());
    assert_eq!(f.batch(), None);
    let active = f.ok(&["active"]);
    assert!(
        active
            .out
            .contains("font\tROOT/res/mine.ttf\tactivates on install\tthe draft\n"),
        "{}",
        active.out
    );
    assert!(
        active
            .out
            .starts_with("font\tbuilt-in\talways active\tbuilt in\n"),
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
        active
            .out
            .contains("font\tROOT/res/mine.ttf\tactive\tbatch.conf\n"),
        "{}",
        active.out
    );
    let again = f.ok(&["activate", &mine]);
    assert_eq!(again.out, "already active\tfont\tROOT/res/mine.ttf\n");
    // a deactivation installs the same way, and keeps every other line
    let before = f.listed();
    let ran = f.ok(&["deactivate", &mine]);
    assert_eq!(
        ran.out,
        "deactivate\tfont\tROOT/res/mine.ttf\ndraft desk: 1 change to the active resources · no problems\n",
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
    for bad in [
        "relative.ttf",
        "/a/../b.ttf",
        "/a/\tb.ttf",
        "/a/\u{1b}b.ttf",
    ] {
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
fn revert_and_new_drop_the_draft_s_changes_to_the_active_resources() {
    for command in ["revert", "new"] {
        let f = fixture(&format!("reset-{command}"));
        f.desk();
        f.ok(&["activate", &f.arg("res/mine.ttf")]);
        let dry = f.ok(&[command, "--dry-run"]);
        assert!(
            dry.out.contains("1 change to the active resources"),
            "{command}: {}",
            dry.out
        );
        assert!(f.changes("desk").exists());
        f.ok(&[command]);
        assert!(!f.changes("desk").exists(), "{command}");
        f.ok(&["install", "--force"]);
        assert_eq!(f.batch(), None, "{command}");
    }
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
            "Look big uses this font through the font set monoid. From the next login after the install, look big renders without it."
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
    assert_eq!(
        ran.out,
        "deactivate\tfont\tROOT/res/other.ttf\ndraft desk: 1 change to the active resources · no problems\n"
    );
    // a resource that is not active has nothing to deactivate
    let ran = g.run(&["deactivate", &g.arg("res/mine.ttf")]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
}

/// Deactivating the pointer theme that vtium.conf's pointer-theme names
/// warns that each look that names no pointer theme of its own shows
/// vtium's own pointer pictures from the next login. The key's value
/// matches the theme's folder with or without its last slash.
///
/// References:
/// - ADR-1205 D8: deactivating a resource warns and names what uses it.
#[test]
fn deactivating_the_pointer_theme_vtium_conf_names_warns() {
    let f = fixture("warn-pointer");
    std::fs::create_dir_all(f.path("res/theme/cursors")).expect("a cursor theme");
    std::fs::write(f.path("res/theme/cursors/default"), "a cursor\n").expect("a cursor");
    let theme = f.arg("res/theme");
    std::fs::write(f.path("etc/batch.conf"), format!("{theme}\n")).expect("a batch file");
    std::fs::write(
        f.path("etc/vtium.conf"),
        format!("pointer-theme = {theme}/\n{INSTALLED}"),
    )
    .expect("vtium.conf");
    f.desk();
    let ran = f.ok(&["deactivate", &theme]);
    assert!(
        ran.out.contains(
            "vtium.conf's pointer-theme names this pointer theme. From the next login after the install, each look that names no pointer theme of its own shows vtium's own pointer pictures in its place."
        ),
        "{}",
        ran.out
    );
}

/// Deactivating a pointer theme that a look names in its own
/// `pointer-theme` line warns that the look shows vtium's own pointer
/// pictures from the next login.
///
/// References:
/// - ADR-1205 D6: each look picks its own pointer theme.
/// - ADR-1205 D8: deactivating a resource warns and names the looks that
///   use it.
#[test]
fn deactivating_a_pointer_theme_a_look_names_warns_of_that_look() {
    let f = fixture("warn-look-pointer");
    std::fs::create_dir_all(f.path("res/theme/cursors")).expect("a cursor theme");
    std::fs::write(f.path("res/theme/cursors/default"), "a cursor\n").expect("a cursor");
    let theme = f.arg("res/theme");
    std::fs::write(f.path("etc/batch.conf"), format!("{theme}\n")).expect("a batch file");
    std::fs::write(
        f.path("etc/vtium.conf"),
        format!("{INSTALLED}[look big]\npointer-theme = {theme}\n"),
    )
    .expect("vtium.conf");
    f.desk();
    let ran = f.ok(&["deactivate", &theme]);
    assert!(
        ran.out.contains(
            "Look big uses this pointer theme. From the next login after the install, look big shows vtium's own pointer pictures in its place."
        ),
        "{}",
        ran.out
    );
}

/// A batch file that vtium refuses counts as missing: vtium uses the list
/// of a fresh install, `active` shows that list and says why, and the
/// install writes the fresh list with the draft's change, carrying none
/// of the refused file's lines.
#[test]
fn a_batch_file_that_vtium_refuses_counts_as_missing() {
    // a file others can write, which lists a font others can write
    let f = fixture("refused-mode");
    let evil = f.arg("res/evil.ttf");
    std::fs::write(f.path("res/evil.ttf"), "a font\n").expect("a font");
    std::fs::set_permissions(
        f.path("res/evil.ttf"),
        std::fs::Permissions::from_mode(0o666),
    )
    .expect("its mode");
    std::fs::write(f.path("etc/batch.conf"), format!("{evil}\n")).expect("a batch file");
    std::fs::set_permissions(
        f.path("etc/batch.conf"),
        std::fs::Permissions::from_mode(0o666),
    )
    .expect("its mode");
    f.desk();
    let active = f.ok(&["active"]);
    assert!(!active.out.contains("evil.ttf"), "{}", active.out);
    for font in fresh_fonts() {
        assert!(
            active
                .out
                .contains(&format!("font\t{font}\tactive\tfresh install\n")),
            "{font} missing from {}",
            active.out
        );
    }
    assert!(
        active.out.contains(
            "ROOT/etc/batch.conf: vtium ignores this list of active fonts, color schemes, effects and pointer themes and uses the list of a fresh install, because"
        ),
        "{}",
        active.out
    );
    f.ok(&["activate", &f.arg("res/mine.ttf")]);
    let install = f.ok(&["install"]);
    assert!(
        install.out.contains("vtium ignores this list"),
        "the install says why it starts from the fresh list: {}",
        install.out
    );
    let listed = f.listed();
    assert!(!listed.contains(&evil), "{listed:?}");
    for font in fresh_fonts() {
        assert!(listed.contains(&font), "{font} missing from {listed:?}");
    }
    assert_eq!(listed.last(), Some(&f.arg("res/mine.ttf")));
    let mode = std::fs::metadata(f.path("etc/batch.conf"))
        .expect("the batch file")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o644);

    // a file larger than vtium reads
    let g = fixture("refused-large");
    let line = format!("{}\n", g.arg("res/other.ttf"));
    let lines = vtium_settings::batch::BATCH_MAX / line.len() + 1;
    std::fs::write(g.path("etc/batch.conf"), line.repeat(lines)).expect("a batch file");
    g.desk();
    let active = g.ok(&["active"]);
    assert!(!active.out.contains("other.ttf"), "{}", active.out);
    assert!(
        active.out.contains("vtium ignores this list"),
        "{}",
        active.out
    );
    g.ok(&["activate", &g.arg("res/mine.ttf")]);
    g.ok(&["install"]);
    let listed = g.listed();
    assert!(!listed.contains(&g.arg("res/other.ttf")), "{listed:?}");
    assert_eq!(listed.last(), Some(&g.arg("res/mine.ttf")));
}

/// A batch file with a line that is not UTF-8 is still read, as vtium
/// reads it: vtium ignores that line and uses the others.
#[test]
fn a_batch_file_with_a_line_that_is_not_utf_8_is_read_as_vtium_reads_it() {
    let f = fixture("not-utf8");
    let mine = f.arg("res/mine.ttf");
    let mut bytes = b"/res/\xff.ttf\n".to_vec();
    bytes.extend_from_slice(format!("{mine}\n").as_bytes());
    std::fs::write(f.path("etc/batch.conf"), bytes).expect("a batch file");
    f.desk();
    let active = f.ok(&["active"]);
    assert!(
        active
            .out
            .contains("font\tROOT/res/mine.ttf\tactive\tbatch.conf\n"),
        "{}",
        active.out
    );
    f.ok(&["deactivate", &mine]);
    // the install cannot keep that line in the file it writes, so it
    // stops and names the line for root to fix
    let ran = f.run(&["install"]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.err
            .contains("ROOT/etc/batch.conf:1: this line is not UTF-8 text."),
        "{}",
        ran.err
    );
}

/// vtium-config reads the batch file to the same verdict as vtium's own
/// reader: the same batch, and for a file vtium refuses, the same words.
#[test]
fn vtium_config_reads_the_batch_file_as_vtium_reads_it() {
    use vtium_config::batch::{Folders, Installed, Start};
    use vtium_settings::batch::{Batch, Entry, Finding};
    let f = fixture("same-reader");
    let file = f.path("etc/batch.conf");
    let euid = rustix::process::geteuid().as_raw();
    let trust = vtium_settings::Trust::running_as(euid);
    let fresh = || Batch::new(vec![Entry::Path(PathBuf::from("/fresh/one.ttf"))]);
    // a package's list beside the batch file, which both readers add
    let (package, admin) = (f.path("etc/package-lists"), f.path("etc/batch.d"));
    std::fs::create_dir_all(&package).expect("the packages' folder");
    std::fs::write(
        package.join("vtium-fonts.conf"),
        format!("{}\n", f.arg("res/other.ttf")),
    )
    .expect("a package's list");
    let start = Start {
        fresh: fresh(),
        folders: Folders {
            package: package.clone(),
            admin: admin.clone(),
        },
        trust: trust.clone(),
    };
    let compare = |case: &str| {
        let ours = Installed::read(&file, euid);
        let mut theirs = Batch::read(&file, &trust, fresh());
        theirs.add_lists(&package, &admin, &trust);
        assert!(theirs.batch.holds(&f.path("res/other.ttf")), "{case}");
        assert_eq!(ours.base(&start), theirs.batch, "{case}");
        assert_eq!(ours.view(&start).list_files(), theirs.list_files, "{case}");
        match (&ours, theirs.findings.first()) {
            (Installed::Refused(why), Some(finding @ Finding::Refused { .. })) => {
                assert_eq!(*why, finding.to_string(), "{case}");
            }
            (Installed::Missing, Some(Finding::Missing { .. })) => {}
            (Installed::Read(_), None | Some(Finding::Line { .. })) => {}
            (ours, theirs) => panic!("{case}: {ours:?} against {theirs:?}"),
        }
    };
    compare("missing");
    std::fs::write(&file, format!("{}\nrelative\n", f.arg("res/mine.ttf"))).expect("a batch file");
    compare("read");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o664)).expect("its mode");
    compare("its group can write it");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).expect("its mode");
    let line = format!("{}\n", f.arg("res/mine.ttf"));
    let lines = vtium_settings::batch::BATCH_MAX / line.len() + 1;
    std::fs::write(&file, line.repeat(lines)).expect("a batch file");
    compare("too large");
    std::fs::remove_file(&file).expect("the file goes");
    std::fs::create_dir(&file).expect("a folder in its place");
    compare("a folder");
}

/// A line of root's batch file that vtium ignores stays as root wrote it,
/// and does not stop an install that changes another line: vtium-config
/// adds no such line itself.
#[test]
fn a_line_vtium_ignores_in_root_s_batch_file_does_not_block_an_install() {
    let f = fixture("bad-line");
    let by_hand = "# root's own list\nfonts/x.ttf\n";
    std::fs::write(f.path("etc/batch.conf"), by_hand).expect("a batch file");
    f.desk();
    f.ok(&["activate", &f.arg("res/mine.ttf")]);
    f.ok(&["install"]);
    assert_eq!(
        f.batch().expect("the batch file"),
        format!("{by_hand}{}\n", f.arg("res/mine.ttf"))
    );
}

/// `diff` shows the edit an install makes to the batch file, and `flow`'s
/// state line counts the draft's changes to the active resources.
#[test]
fn diff_and_the_state_line_show_a_draft_s_changes_to_the_active_resources() {
    let f = fixture("diff");
    let by_hand = "# root's own list\n";
    std::fs::write(f.path("etc/batch.conf"), by_hand).expect("a batch file");
    f.desk();
    let mine = f.arg("res/mine.ttf");
    f.ok(&["activate", &mine]);
    let diff = f.ok(&["diff"]);
    assert!(
        diff.out.ends_with(
            "--- ROOT/etc/batch.conf\n+++ ROOT/etc/batch.conf (after install)\n@@ line 2\n+ROOT/res/mine.ttf\n"
        ),
        "{}",
        diff.out
    );
    let flow = f.ok(&["flow"]);
    let first = flow.out.lines().next().unwrap_or_default();
    assert!(
        first.starts_with("draft desk: 1 change to the active resources · "),
        "{first}"
    );
    // a change to vtium.conf and one to the active resources together
    f.ok(&["set", "font-size", "14"]);
    let flow = f.ok(&["flow"]);
    let first = flow.out.lines().next().unwrap_or_default();
    assert!(
        first.starts_with(
            "draft desk: 1 line added, 1 removed, 1 change to the active resources · "
        ),
        "{first}"
    );
}

/// Activating a resource the system already holds takes a pending
/// deactivation back without judging the file again, as `apply` judges
/// only the paths the system does not hold.
#[test]
fn activating_takes_a_pending_deactivation_back_without_the_trust_rule() {
    let f = fixture("take-back");
    let gone = f.arg("res/gone.ttf");
    std::fs::write(f.path("etc/batch.conf"), format!("{gone}\n")).expect("a batch file");
    f.desk();
    f.ok(&["deactivate", &gone]);
    assert!(f.changes("desk").exists());
    f.ok(&["activate", &gone]);
    assert!(!f.changes("desk").exists(), "the draft makes no change");
}

/// Font files of the fixture's own, `res/many/NNNN.ttf`, enough of them
/// that their paths together pass the most bytes vtium reads of the batch
/// file.
fn many_fonts(f: &Fixture) -> Vec<String> {
    std::fs::create_dir_all(f.path("res/many")).expect("the folder");
    let one = f.arg("res/many/0000.ttf").len().saturating_add(1);
    let n = vtium_settings::batch::BATCH_MAX / one + 2;
    (0..n)
        .map(|i| {
            let path = f.path(&format!("res/many/{i:04}.ttf"));
            std::fs::write(&path, "a font\n").expect("a font");
            path.to_string_lossy().into_owned()
        })
        .collect()
}

/// An activation that would make the batch file longer than vtium reads
/// is refused at once, with the bound in the reader's words, and nothing
/// is kept. Without the check, `activate` kept them all and only
/// `install` refused, naming the draft's file.
#[test]
fn activating_more_than_the_batch_file_holds_is_refused_and_nothing_is_kept() {
    let f = fixture("too-many");
    f.desk();
    let fonts = many_fonts(&f);
    let mut args = vec!["activate"];
    args.extend(fonts.iter().map(String::as_str));
    let ran = f.run(&args);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.err.starts_with(
            "vtium-config: These changes would make the list of active resources too long. vtium reads at most 64 KiB of the list, about a thousand paths,"
        ),
        "{}",
        ran.err
    );
    assert!(
        ran.err
            .contains("Nothing was changed: draft desk keeps the active resources it had."),
        "{}",
        ran.err
    );
    assert_eq!(ran.err.lines().count(), 2, "said once: {}", ran.err);
    assert!(!f.changes("desk").exists(), "nothing is kept");
    // half of them fit
    let mut args = vec!["activate"];
    args.extend(fonts.iter().take(fonts.len() / 2).map(String::as_str));
    f.ok(&args);
    let install = f.ok(&["install"]);
    assert!(
        install.out.contains("wrote\tROOT/etc/batch.conf"),
        "{}",
        install.out
    );
}

/// The TUI's Space changes one resource through `Resources::set`, which
/// refuses an activation the batch file has no room for and keeps the
/// draft's other changes. A deactivation always passes, so a draft that
/// holds too many activations can be brought back within the bound.
#[test]
fn one_activation_past_the_bound_is_refused_and_the_draft_keeps_the_rest() {
    use vtium_config::batch::{Change, Done, Resources, State};
    let f = fixture("one-too-many");
    f.desk();
    let fonts = many_fonts(&f);
    let root = f.root.clone();
    let store = vtium_config::store::Store::locate(
        rustix::process::geteuid().as_raw(),
        move |name: &str| match name {
            "HOME" => Some(root.join("home").into()),
            "XDG_CONFIG_HOME" => Some(root.join("home/config").into()),
            "XDG_STATE_HOME" => Some(root.join("home/state").into()),
            _ => None,
        },
        |_| None,
    )
    .expect("the store");
    let paths = vtium_config::tui::Paths {
        configdir: f.path("etc"),
        frits: Ok(f.path("etc/fonts")),
        drafts: f.path("home/drafts"),
        sudo: "sudo".into(),
        vtium: f.path("vtium").into(),
        helper: vtium_config::specimen::Helper::at(BIN),
        sys: f.path("sys"),
    };
    let session = vtium_config::drafts::Session {
        store,
        paths: &paths,
    };
    let mut resources = Resources::open(&session).expect("the draft's resources");
    let mut refused = None;
    for font in &fonts {
        match resources.set(Path::new(font), Change::Activate) {
            Ok(Done::Changed { .. }) => {}
            Ok(other) => panic!("{font}: {other:?}"),
            Err(why) => {
                refused = Some((font.clone(), why));
                break;
            }
        }
    }
    let (font, why) = refused.expect("an activation past the bound is refused");
    assert_eq!(
        why,
        "The list of active resources is full. vtium reads at most 64 KiB of the list, about a thousand paths, so vtium-config did not activate this font. Deactivate resources you do not use to make room."
    );
    assert_eq!(resources.state(Path::new(&font)), State::Inactive);
    let kept = resources.list().len();
    assert!(kept > fonts.len() / 2, "{kept}");
    // a deactivation always passes
    let first = Path::new(&fonts[0]);
    assert!(matches!(
        resources.set(first, Change::Deactivate),
        Ok(Done::Changed { .. })
    ));
    assert_eq!(resources.state(first), State::Inactive);
    resources.save().expect("the draft keeps its changes");
    let active = f.ok(&["active"]);
    assert!(!active.out.contains(&font), "{}", active.out);
}

/// `active` on the record's example machine: a package's list that
/// activates a font, names one that is not installed and names one that a
/// `-` line of batch.conf turns off; a second package's list that root
/// turns off with a link to /dev/null; and a font the draft activates.
/// Each line has four columns, the fourth saying where the state comes
/// from, and one line follows for each entry in the folders of lists.
///
/// References:
/// - ADR-1205 D4, as amended on 2026-11-27: what `vtium config active` prints, and its example.
#[test]
fn active_names_where_each_state_comes_from_and_lists_the_lists() {
    let f = fixture("active-lists");
    for dir in ["etc/package-lists", "etc/batch.d"] {
        std::fs::create_dir_all(f.path(dir)).expect("a folder of lists");
    }
    std::fs::write(f.path("res/tiny5.bdf"), "a font\n").expect("a font");
    std::fs::write(f.path("res/bold.ttf"), "a font\n").expect("a font");
    std::fs::write(
        f.path("etc/package-lists/vtium-fonts.conf"),
        format!(
            "# the fonts vtium-fonts activates\n{}\n{}\n{}\n",
            f.arg("res/tiny5.bdf"),
            f.arg("res/cjk.ttc"),
            f.arg("res/bold.ttf")
        ),
    )
    .expect("a package's list");
    std::fs::write(
        f.path("etc/package-lists/other-fonts.conf"),
        format!("{}\n", f.arg("res/other.ttf")),
    )
    .expect("a package's list");
    std::os::unix::fs::symlink("/dev/null", f.path("etc/batch.d/other-fonts.conf"))
        .expect("root's link to /dev/null");
    std::fs::write(
        f.path("etc/batch.conf"),
        format!("-{}\n", f.arg("res/bold.ttf")),
    )
    .expect("a batch file");
    f.desk();
    f.ok(&["activate", &f.arg("res/mine.ttf")]);
    let active = f.ok(&["active"]);
    assert_eq!(
        active.out,
        "\
font\tbuilt-in\talways active\tbuilt in
font\tROOT/res/tiny5.bdf\tactive\tvtium-fonts
font\tROOT/res/cjk.ttc\tnot installed\tvtium-fonts
font\tROOT/res/bold.ttf\tnot active\tbatch.conf
font\tROOT/res/mine.ttf\tactivates on install\tthe draft
list\tROOT/etc/package-lists/other-fonts.conf\tturned off\tROOT/etc/batch.d/other-fonts.conf
list\tROOT/etc/batch.d/other-fonts.conf\tturned off\tROOT/etc/batch.d/other-fonts.conf
list\tROOT/etc/package-lists/vtium-fonts.conf\tread\tROOT/etc/package-lists/vtium-fonts.conf
",
        "{}",
        active.err
    );
    // a font a list activates can be deactivated, and the draft says so
    f.ok(&["deactivate", &f.arg("res/tiny5.bdf")]);
    let active = f.ok(&["active"]);
    assert!(
        active
            .out
            .contains("font\tROOT/res/tiny5.bdf\tdeactivates on install\tthe draft\n"),
        "{}",
        active.out
    );
}
