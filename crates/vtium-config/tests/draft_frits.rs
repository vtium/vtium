//! A draft's font set changes: the folder beside the draft that keeps the
//! frits it adds, replaces and removes, run as the binary against a
//! fixture whose configuration directory stands in for `/etc/vtium`, so
//! the install runs `apply` as the user and writes the fixture's frits.
//! The stand-in `vtium` passes every vtium.conf. The tests put each change
//! in the folder as the Fonts tab keeps it there.
//!
//! References:
//! - ADR-1403 D8, "A draft's font set changes": the folder, what reads it, and what install does with it.

#[path = "common/home.rs"]
mod home;
use vtium_testscript as script;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// The stand-in console: `check-config` passes every file.
const CHECKER: &str = "#!/bin/sh\n[ \"$1\" = check-config ] || exit 9\nexit 0\n";

const OLD: &str = "source=builtin:unifont\n";

const KEPT: &str = "source=builtin:unscii-8\n";

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
        .join("draft-frits")
        .join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("a fresh fixture");
    }
    for dir in ["etc/fonts", "home", "sys", "udev/data", "res"] {
        std::fs::create_dir_all(root.join(dir)).expect("the fixture");
    }
    std::fs::write(root.join("etc/vtium.conf"), INSTALLED).expect("vtium.conf");
    std::fs::write(root.join("etc/fonts/old.vtfrit"), OLD).expect("a frit");
    std::fs::write(root.join("etc/fonts/kept.vtfrit"), KEPT).expect("a frit");
    script::executable(&root.join("vtium"), CHECKER);
    Fixture { root }
}

impl Fixture {
    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p)
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

    /// The draft `draft`'s font set folder.
    fn folder(&self, draft: &str) -> PathBuf {
        self.path(&format!("home/config/vtium/drafts/{draft}.frits"))
    }

    /// Puts `file` with `text` in the draft desk's font set folder, as the
    /// Fonts tab keeps a change there.
    fn keep(&self, file: &str, text: &str) {
        std::fs::create_dir_all(self.folder("desk")).expect("the folder");
        std::fs::write(self.folder("desk").join(file), text).expect("a change");
    }

    fn first_line(&self) -> String {
        self.ok(&["flow"])
            .out
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned()
    }
}

const NEW: &str = "name=new\nsource=builtin:unscii-8\n";

/// A draft keeps the frits it adds, replaces and removes in a folder
/// beside it, so they wait there between runs of vtium-config. `flow`
/// counts them, `diff` prints each under its path in the frits' directory,
/// and `install` writes the new frits, then vtium.conf, then removes the
/// removed frit, and leaves the draft with no folder.
///
/// References:
/// - ADR-1403 D8, "A draft's font set changes": the folder, diff and install.
#[test]
fn a_draft_keeps_its_font_set_changes_until_install() {
    let f = fixture("install");
    f.desk();
    f.keep("new.vtfrit", NEW);
    f.keep("kept.vtfrit", "source=builtin:unscii-8\ncell=8x16\n");
    f.keep("old.removed", "");
    // a frit whose text is the installed frit's is no change
    f.keep("same.vtfrit", "source=builtin:unifont\n");
    std::fs::write(f.path("etc/fonts/same.vtfrit"), "source=builtin:unifont\n").expect("a frit");
    assert!(
        f.first_line()
            .starts_with("draft desk: 3 font set changes · "),
        "{}",
        f.first_line()
    );
    let diff = f.ok(&["diff"]).out;
    assert!(
        diff.ends_with(concat!(
            "--- ROOT/etc/fonts/kept.vtfrit\n+++ ROOT/etc/fonts/kept.vtfrit (after install)\n",
            "@@ line 2\n+cell=8x16\n",
            "--- ROOT/etc/fonts/new.vtfrit\n+++ ROOT/etc/fonts/new.vtfrit (after install)\n",
            "@@ line 1\n+name=new\n@@ line 2\n+source=builtin:unscii-8\n",
            "--- ROOT/etc/fonts/old.vtfrit\n+++ ROOT/etc/fonts/old.vtfrit (after install)\n",
            "@@ line 1\n-source=builtin:unifont\n",
        )),
        "{diff}"
    );
    // a change to vtium.conf and the font set changes count together
    f.ok(&["set", "font-size", "14"]);
    assert!(
        f.first_line()
            .starts_with("draft desk: 1 line added, 1 removed, 3 font set changes · "),
        "{}",
        f.first_line()
    );
    let install = f.ok(&["install"]);
    assert!(
        install.out.contains("wrote\tROOT/etc/fonts/new.vtfrit"),
        "{}",
        install.out
    );
    let read = |p: &str| std::fs::read_to_string(f.path(p)).ok();
    assert_eq!(read("etc/fonts/new.vtfrit").as_deref(), Some(NEW));
    assert_eq!(
        read("etc/fonts/kept.vtfrit").as_deref(),
        Some("source=builtin:unscii-8\ncell=8x16\n")
    );
    assert_eq!(read("etc/fonts/old.vtfrit"), None);
    assert_eq!(read("etc/fonts/old.vtfrit.bak").as_deref(), Some(OLD));
    assert!(
        read("etc/vtium.conf")
            .unwrap_or_default()
            .contains("font-size=14")
    );
    assert!(!f.folder("desk").exists(), "the install leaves no folder");
    assert!(
        f.first_line()
            .starts_with("draft desk: same as installed · "),
        "{}",
        f.first_line()
    );
}

/// `revert` and `new` drop the folder, `draft new --from` copies it, and
/// `draft rm` removes it with the draft.
#[test]
fn revert_new_copy_and_rm_carry_the_folder_with_the_draft() {
    let f = fixture("carry");
    f.desk();
    f.keep("new.vtfrit", NEW);
    let lost = f.ok(&["revert", "--dry-run"]).out;
    assert!(lost.contains("1 font set change"), "{lost}");
    assert!(f.folder("desk").exists(), "a dry run keeps the folder");
    f.ok(&["draft", "new", "copy", "--from", "desk"]);
    assert_eq!(
        std::fs::read_to_string(f.folder("copy").join("new.vtfrit"))
            .ok()
            .as_deref(),
        Some(NEW)
    );
    f.ok(&["draft", "rm", "copy"]);
    assert!(!f.folder("copy").exists());
    f.ok(&["draft", "switch", "desk"]);
    f.ok(&["revert"]);
    assert!(!f.folder("desk").exists());
    f.keep("new.vtfrit", NEW);
    f.ok(&["new"]);
    assert!(!f.folder("desk").exists());
}

/// A file in the folder that is no font set change, and a frit there that
/// the frit reader refuses, each keep the draft from installing, and the
/// message names the file.
#[test]
fn install_refuses_a_folder_that_holds_anything_else() {
    let f = fixture("refuse");
    f.desk();
    f.keep("notes.txt", "mine\n");
    let ran = f.run(&["install"]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.out.contains(
            "ROOT/home/config/vtium/drafts/desk.frits/notes.txt: this file is not a font set change"
        ),
        "{}{}",
        ran.out,
        ran.err
    );
    std::fs::remove_file(f.folder("desk").join("notes.txt")).expect("the file");
    f.keep("bad.vtfrit", "source=nothing:at-all\n");
    let ran = f.run(&["install"]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.out
            .contains("ROOT/home/config/vtium/drafts/desk.frits/bad.vtfrit:1: "),
        "{}{}",
        ran.out,
        ran.err
    );
    assert!(!f.path("etc/fonts/bad.vtfrit").exists());
    // a removal of a frit that is not installed removes nothing, and says so
    std::fs::remove_file(f.folder("desk").join("bad.vtfrit")).expect("the file");
    f.keep("gone.removed", "");
    let ran = f.ok(&["install"]);
    assert!(
        ran.out.contains("no installed font set is named gone"),
        "{}",
        ran.out
    );
    assert!(!f.folder("desk").exists());
}

/// vtium-config's check of a draft passes the console's check one
/// `--with-frit` for each frit the folder adds or replaces, naming its
/// file in the folder, and one `--without-frit` for each frit it removes,
/// so the check counts the frits the install leaves.
///
/// References:
/// - ADR-1403 D8, "A draft's font set changes": the check sees the same frits.
#[test]
fn the_check_of_a_draft_counts_the_folder_s_font_sets() {
    let f = fixture("check-args");
    // the stand-in console keeps each word it is given, one to a line
    let args = f.path("args");
    script::executable(
        &f.path("vtium"),
        &format!(
            "#!/bin/sh\n[ \"$1\" = check-config ] || exit 9\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            args.display()
        ),
    );
    f.desk();
    f.keep("new.vtfrit", NEW);
    f.keep("old.removed", "");
    f.ok(&["flow"]);
    let words = std::fs::read_to_string(&args).expect("the words the check took");
    let root = f.root.to_str().expect("a path");
    let words: Vec<String> = words.lines().map(|w| w.replace(root, "ROOT")).collect();
    let frits: Vec<&str> = words
        .windows(2)
        .filter(|w| w[0].ends_with("-frit"))
        .flat_map(|w| [w[0].as_str(), w[1].as_str()])
        .collect();
    assert_eq!(
        frits,
        [
            "--with-frit",
            "ROOT/home/config/vtium/drafts/desk.frits/new.vtfrit",
            "--without-frit",
            "old",
        ]
    );
    // a draft with no font set change passes neither
    std::fs::remove_dir_all(f.folder("desk")).expect("the folder");
    f.ok(&["flow"]);
    let words = std::fs::read_to_string(&args).expect("the words the check took");
    assert!(
        !words
            .lines()
            .any(|w| w.starts_with("--with") && w.ends_with("-frit")),
        "{words}"
    );
}

/// The frits' paths come from the installed frits' directory, so a draft
/// that moves that directory with `font-frits` and also holds a font set
/// change is refused at install, and nothing is installed; the move
/// installs on its own first.
#[test]
fn install_refuses_a_draft_that_moves_the_frits_and_changes_a_frit() {
    let f = fixture("moves");
    f.desk();
    f.ok(&["set", "font-frits", "/srv/frits"]);
    f.keep("new.vtfrit", NEW);
    let before = std::fs::read_to_string(f.path("etc/vtium.conf")).expect("vtium.conf");
    let ran = f.run(&["install"]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.err
            .contains("changes font-frits and also adds or removes font sets"),
        "{}{}",
        ran.out,
        ran.err
    );
    assert!(!f.path("etc/fonts/new.vtfrit").exists());
    assert_eq!(
        std::fs::read_to_string(f.path("etc/vtium.conf")).expect("vtium.conf"),
        before
    );
    // without the font set change, the move installs
    std::fs::remove_dir_all(f.folder("desk")).expect("the folder");
    f.ok(&["install"]);
}

/// The console this workspace builds, which judges a draft the way the
/// install's own check judges it.
#[path = "common/console.rs"]
mod console;

/// A draft whose look names a font set that only the draft adds installs:
/// both vtium-config's check of the draft and the check that `apply DIR`
/// runs count the folder's font set as installed. The console here is the
/// one this workspace builds, not a stand-in, so the check reads the font
/// set the way the console does.
///
/// References:
/// - ADR-1403 D8, "What `install` does": the check of `apply DIR` passes `--with-frit` and `--without-frit` for the bundle's font sets.
#[test]
fn a_look_that_names_a_font_set_only_the_draft_adds_installs() {
    let f = fixture("look-new-frit");
    script::executable(
        &f.path("vtium"),
        &format!("#!/bin/sh\nexec '{}' \"$@\"\n", console::vtium().display()),
    );
    f.desk();
    f.keep("new.vtfrit", NEW);
    f.keep("old.removed", "");
    let draft = f.path("home/config/vtium/drafts/desk.conf");
    std::fs::write(
        &draft,
        "font-size=12\nversion = 1\n[look big]\nfont-name = new\n[head desk]\nlook = big\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n",
    )
    .expect("the draft");
    let checked = f.run(&["flow"]);
    assert!(
        !checked.out.contains("not installed") && !checked.err.contains("not installed"),
        "{}{}",
        checked.out,
        checked.err
    );
    let install = f.run(&["install", "--force"]);
    assert_eq!(install.status, 0, "{}{}", install.out, install.err);
    assert!(!install.out.contains("not installed"), "{}", install.out);
    let read = |p: &str| std::fs::read_to_string(f.path(p)).ok();
    assert_eq!(read("etc/fonts/new.vtfrit").as_deref(), Some(NEW));
    assert_eq!(read("etc/fonts/old.vtfrit"), None);
    assert!(
        read("etc/vtium.conf")
            .unwrap_or_default()
            .contains("font-name = new")
    );
}

/// The check that `apply DIR` runs gets one `--with-frit` for each frit
/// the bundle writes and one `--without-frit` for each frit it removes.
/// Each `--with-frit` names a copy that the install made in a directory
/// of its own inside the frits' directory, with mode 0700, and the copy
/// holds the text the install writes. The directory is gone after the
/// install.
///
/// References:
/// - ADR-1403 D8, "What `install` does": the check of `apply DIR` passes `--with-frit` and `--without-frit` for the bundle's font sets.
#[test]
fn the_install_s_own_check_counts_the_bundle_s_font_sets() {
    let f = fixture("apply-args");
    // the stand-in console appends each run's words, and for each
    // --with-frit the mode of the copy's directory and the copy's text
    let log = f.path("log");
    script::executable(
        &f.path("vtium"),
        &format!(
            concat!(
                "#!/bin/sh\n[ \"$1\" = check-config ] || exit 9\n",
                "{{ echo ---; prev=; for a in \"$@\"; do echo \"$a\"; ",
                "if [ \"$prev\" = --with-frit ]; then stat -c 'mode %a' \"$(dirname \"$a\")\"; cat \"$a\"; fi; ",
                "prev=$a; done; }} >> '{}'\nexit 0\n",
            ),
            log.display()
        ),
    );
    f.desk();
    f.ok(&["set", "font-size", "14"]);
    f.keep("new.vtfrit", NEW);
    f.keep("old.removed", "");
    std::fs::write(&log, "").expect("the log");
    f.ok(&["install"]);
    let said = std::fs::read_to_string(&log).expect("the log");
    let root = f.root.to_str().expect("a path");
    let last = said.rsplit("---\n").next().unwrap_or_default();
    let words: Vec<String> = last.lines().map(|w| w.replace(root, "ROOT")).collect();
    let at = words
        .iter()
        .position(|w| w == "--with-frit")
        .expect("a --with-frit");
    let copy = &words[at + 1];
    let dir = copy
        .strip_suffix("/new.vtfrit")
        .expect("the copy is named for its frit");
    assert!(
        dir.starts_with("ROOT/etc/fonts/vtium-config-apply-"),
        "{words:?}"
    );
    assert_eq!(words[at + 2], "mode 700", "{words:?}");
    assert_eq!(
        words[at + 3..at + 5],
        ["name=new", "source=builtin:unscii-8"],
        "{words:?}"
    );
    let gone = words
        .iter()
        .position(|w| w == "--without-frit")
        .expect("a --without-frit");
    assert_eq!(words[gone + 1], "old", "{words:?}");
    assert!(
        !Path::new(&dir.replace("ROOT", root)).exists(),
        "the copies go after the check"
    );
}
