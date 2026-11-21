//! A draft's font set changes: the folder beside the draft that keeps the
//! frits it adds, replaces and removes, run as the binary against a
//! fixture whose configuration directory stands in for `/etc/vtium`, so
//! the install runs `apply` as the user and writes the fixture's frits.
//! The stand-in `vtium` passes every vtium.conf. The tests put each change
//! in the folder as the Fonts tab keeps it there. `XDG_RUNTIME_DIR` points
//! at the fixture's own `run` folder, mode 0700, where the install makes
//! the private folder for its check's copies of the draft's font sets.
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
    for dir in ["etc/fonts", "home", "sys", "udev/data", "res", "run"] {
        std::fs::create_dir_all(root.join(dir)).expect("the fixture");
    }
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.join("run"), std::fs::Permissions::from_mode(0o700))
            .expect("the runtime folder's mode");
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
        self.run_in(self.command(args))
    }

    /// The binary with `args`, in the fixture's own directories.
    fn command(&self, args: &[&str]) -> Command {
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
            .env("XDG_RUNTIME_DIR", self.path("run"))
            .stdin(Stdio::null());
        c
    }

    /// Runs `c`, with the fixture's root shown as `ROOT` in what it says.
    fn run_in(&self, mut c: Command) -> Ran {
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

    /// What the draft `draft`'s font set folder holds, by name, sorted;
    /// nothing when there is no folder.
    fn held(&self, draft: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.folder(draft))
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
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
/// counts them, `flow --json` lists them, `diff` prints each under its
/// path in the frits' directory, and `install` writes the new and
/// replaced frits, then vtium.conf, then removes the removed frit, in
/// that order, and leaves the draft's folder holding nothing.
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
    // flow --json lists each change by its name, in the order of the names
    let json = f.ok(&["flow", "--json"]).out;
    assert!(
        json.contains(concat!(
            r#""frits":[{"name":"kept","change":"replace"},"#,
            r#"{"name":"new","change":"add"},{"name":"old","change":"remove"}],"#
        )),
        "{json}"
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
    // the order of the install's lines is the order of its writes
    let at = |line: &str| {
        install
            .out
            .lines()
            .position(|l| l == line)
            .unwrap_or_else(|| panic!("no line {line:?} in\n{}", install.out))
    };
    let conf = at("wrote\tROOT/etc/vtium.conf");
    assert!(
        at("wrote\tROOT/etc/fonts/kept.vtfrit") < conf,
        "{}",
        install.out
    );
    assert!(
        at("wrote\tROOT/etc/fonts/new.vtfrit") < conf,
        "{}",
        install.out
    );
    assert!(
        conf < at("removed\tROOT/etc/fonts/old.vtfrit"),
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
    assert_eq!(
        f.held("desk"),
        Vec::<String>::new(),
        "the install leaves no change"
    );
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
    assert_eq!(f.held("desk"), Vec::<String>::new());
}

/// A temporary file that vtium-config's write of a change leaves in the
/// folder, when the write is cut short before its rename or is still under
/// way in another vtium-config, is no font set change: the draft counts
/// only its real changes and installs.
#[test]
fn a_write_s_temporary_file_in_the_folder_blocks_nothing() {
    let f = fixture("leftover");
    f.desk();
    f.keep("new.vtfrit", NEW);
    f.keep(".new.vtfrit.new-4242", "name=new\n");
    f.keep(".old.removed.new-17", "");
    assert!(
        f.first_line()
            .starts_with("draft desk: 1 font set change · "),
        "{}",
        f.first_line()
    );
    let install = f.ok(&["install"]);
    assert!(
        install.out.contains("wrote\tROOT/etc/fonts/new.vtfrit"),
        "{}",
        install.out
    );
    assert!(f.path("etc/fonts/old.vtfrit").exists());
}

/// Taking the last change back, as an install does once the system matches
/// the folder, leaves the folder in place and empty, so another
/// vtium-config that has just made the folder for a change of its own can
/// still write into it. An empty folder is no change: the draft is the
/// same as installed and installs nothing.
#[test]
fn the_last_change_taken_back_leaves_an_empty_folder_that_holds_no_change() {
    let f = fixture("empty");
    f.desk();
    f.keep("new.vtfrit", NEW);
    f.ok(&["install"]);
    assert!(f.folder("desk").is_dir(), "the folder stays");
    // the other vtium-config writes its change into the folder it made
    std::fs::write(f.folder("desk").join("late.removed"), "").expect("a write into the folder");
    std::fs::remove_file(f.folder("desk").join("late.removed")).expect("the file");
    assert!(
        f.first_line()
            .starts_with("draft desk: same as installed · "),
        "{}",
        f.first_line()
    );
    let diff = f.ok(&["diff"]).out;
    assert!(!diff.contains("@@") && !diff.contains("fonts/"), "{diff}");
    let again = f.ok(&["install"]);
    assert!(!again.out.contains("wrote"), "{}", again.out);
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

/// The check of a draft passes the console's check only the font set
/// changes that `flow`, `diff` and install count. A removal that removes
/// nothing, such as the removal of a set that only a category folder of
/// the catalog holds or of a frit this machine lacks, and a frit whose
/// text is the installed one's, pass nothing. So a look that names the
/// catalog's set is not warned about, and the install does not refuse a
/// draft for a removal that would do nothing.
///
/// References:
/// - ADR-1403 D8, "A draft's font set changes": a frit with the installed text is no change, and a removal of a frit the machine lacks removes nothing.
#[test]
fn the_check_passes_only_the_font_set_changes_install_makes() {
    let f = fixture("check-changes");
    let args = f.path("args");
    script::executable(
        &f.path("vtium"),
        &format!(
            "#!/bin/sh\n[ \"$1\" = check-config ] || exit 9\nprintf '%s\\n' \"$@\" > '{}'\nexit 0\n",
            args.display()
        ),
    );
    std::fs::create_dir_all(f.path("etc/fonts/console")).expect("a category folder");
    std::fs::write(f.path("etc/fonts/console/cat.vtfrit"), OLD).expect("a set of the catalog");
    f.desk();
    f.keep("cat.removed", "");
    f.keep("gone.removed", "");
    f.keep("kept.vtfrit", KEPT);
    f.keep("old.removed", "");
    let first = f.first_line();
    let words = std::fs::read_to_string(&args).expect("the words the check took");
    let frits: Vec<&str> = words
        .lines()
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|w| w[0].ends_with("-frit"))
        .flat_map(|w| [w[0], w[1]])
        .collect();
    assert_eq!(frits, ["--without-frit", "old"], "{first}");
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

/// A draft that removes a font set its top-level `font-name` names is
/// refused before the privileged step, with the words of `apply`'s own
/// refusal, and nothing is installed. The console's check does not flag
/// that line, because it reads a name that no font set has as a font
/// family, so the stand-in console here passes the draft as the real one
/// would.
///
/// References:
/// - ADR-1403 D8, "What `install` does": the privileged step refuses to remove a font set that the installed vtium.conf names.
#[test]
fn install_refuses_a_removal_that_the_top_level_font_name_names() {
    let f = fixture("top-names");
    f.desk();
    f.ok(&["set", "font-name", "old"]);
    f.keep("old.removed", "");
    let before = std::fs::read_to_string(f.path("etc/vtium.conf")).expect("vtium.conf");
    let ran = f.run(&["install", "--force"]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.err.contains(
            "nothing installed: ROOT/etc/fonts/old.vtfrit is not removed: font-name in vtium.conf uses it."
        ),
        "{}{}",
        ran.out,
        ran.err
    );
    assert!(!ran.out.contains("apply"), "{}", ran.out);
    assert!(f.path("etc/fonts/old.vtfrit").exists());
    assert_eq!(
        std::fs::read_to_string(f.path("etc/vtium.conf")).expect("vtium.conf"),
        before
    );
    assert_eq!(f.held("desk"), ["old.removed"]);
    // once the top level names another font, the removal installs
    f.ok(&["set", "font-name", "kept"]);
    f.ok(&["install", "--force"]);
    assert!(!f.path("etc/fonts/old.vtfrit").exists());
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
/// Each `--with-frit` names a copy, mode 0600, that the install made in a
/// private folder of its own under the user's runtime folder, with mode
/// 0700, and the copy holds the text the install writes. The folder is
/// gone after the install, and the install made nothing in the frits'
/// directory but the frit itself.
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
                "if [ \"$prev\" = --with-frit ]; then stat -c 'mode %a' \"$(dirname \"$a\")\" \"$a\"; cat \"$a\"; fi; ",
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
    assert!(dir.starts_with("ROOT/run/vtium-config-apply-"), "{words:?}");
    assert_eq!(words[at + 2..at + 4], ["mode 700", "mode 600"], "{words:?}");
    assert_eq!(
        words[at + 4..at + 6],
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
    let mut fonts: Vec<String> = std::fs::read_dir(f.path("etc/fonts"))
        .expect("the frits' directory")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    fonts.sort();
    assert_eq!(fonts, ["kept.vtfrit", "new.vtfrit", "old.vtfrit.bak"]);
}

/// The install checks the frits' directory, and every folder above it,
/// with the walk the console reads every font set through, before it
/// writes anything there or runs its check. A frits' directory that other
/// users can write is refused, and nothing is installed, since the console
/// would refuse every font set in it.
///
/// References:
/// - ADR-1403 D8, "What `install` does": apply checks the frits' directory with the console's own rule before it writes there.
/// - ADR-1401 D10: the ownership check that every installed frit passes.
#[test]
fn the_install_refuses_a_frits_directory_others_can_write() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture("apply-writable");
    // the stand-in console lists the frits' directory each time it runs
    let log = f.path("log");
    script::executable(
        &f.path("vtium"),
        &format!(
            "#!/bin/sh\n[ \"$1\" = check-config ] || exit 9\nls -a '{}' >> '{}'\nexit 0\n",
            f.path("etc/fonts").display(),
            log.display()
        ),
    );
    f.desk();
    f.ok(&["set", "font-size", "14"]);
    f.keep("new.vtfrit", NEW);
    let fonts = f.path("etc/fonts");
    std::fs::set_permissions(&fonts, std::fs::Permissions::from_mode(0o777)).expect("the mode");
    std::fs::write(&log, "").expect("the log");
    let ran = f.run(&["install"]);
    std::fs::set_permissions(&fonts, std::fs::Permissions::from_mode(0o755)).expect("the mode");
    assert_ne!(ran.status, 0, "{}{}", ran.out, ran.err);
    // the install passes on what apply says
    assert!(
        format!("{}{}", ran.out, ran.err).contains(
            "vtium reads no font set in ROOT/etc/fonts, so the install writes none there: other users can change \"ROOT/etc/fonts\" (mode 777)."
        ),
        "{}{}",
        ran.out,
        ran.err
    );
    let listed = std::fs::read_to_string(&log).expect("the log");
    assert!(
        !listed.contains("vtium-config-apply-"),
        "the check's folder was made: {listed}"
    );
    assert!(!fonts.join("new.vtfrit").exists(), "nothing is installed");
    assert_eq!(f.held("desk"), ["new.vtfrit"]);
}

/// A bundle that writes or removes a font set but holds no vtium.conf,
/// because the draft's vtium.conf is the installed one, runs no check of
/// vtium.conf. The install still checks the frits' directory with the
/// console's walk, and refuses a directory other users can write: it
/// writes no frit there and removes none.
///
/// References:
/// - ADR-1403 D8, "What `install` does": apply checks the frits' directory with the console's own rule before it writes there.
#[test]
fn the_install_refuses_a_frits_directory_others_can_write_without_vtium_conf() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture("apply-writable-frits-only");
    f.desk();
    f.keep("new.vtfrit", NEW);
    f.keep("old.removed", "");
    let fonts = f.path("etc/fonts");
    std::fs::set_permissions(&fonts, std::fs::Permissions::from_mode(0o777)).expect("the mode");
    let ran = f.run(&["install"]);
    std::fs::set_permissions(&fonts, std::fs::Permissions::from_mode(0o755)).expect("the mode");
    assert_ne!(ran.status, 0, "{}{}", ran.out, ran.err);
    assert!(
        format!("{}{}", ran.out, ran.err).contains(
            "vtium reads no font set in ROOT/etc/fonts, so the install writes none there: other users can change \"ROOT/etc/fonts\" (mode 777)."
        ),
        "{}{}",
        ran.out,
        ran.err
    );
    assert!(!fonts.join("new.vtfrit").exists(), "nothing is installed");
    assert_eq!(
        std::fs::read_to_string(fonts.join("old.vtfrit"))
            .ok()
            .as_deref(),
        Some(OLD),
        "nothing is removed"
    );
    assert_eq!(f.held("desk"), ["new.vtfrit", "old.removed"]);
}

/// The install writes and removes each font set in the frits' directory
/// it checked, through a descriptor it holds on that directory, never by
/// the path again. Here the frits' directory is a symbolic link, and the
/// stand-in console points the link at another folder while the install's
/// own check runs, after the install checked the directory. The new frit
/// still lands in the folder that was checked, the removal renames the
/// frit there, and the other folder stays as it was.
///
/// References:
/// - ADR-1403 D8, "What `install` does": apply checks the frits' directory with the console's own rule before it writes there.
#[test]
fn a_link_swapped_after_the_check_does_not_move_the_install() {
    let f = fixture("apply-swapped-link");
    let (checked, other) = (f.path("etc/checked"), f.path("etc/other"));
    std::fs::rename(f.path("etc/fonts"), &checked).expect("the checked folder");
    std::fs::create_dir(&other).expect("the other folder");
    std::fs::write(other.join("old.vtfrit"), OLD).expect("a frit");
    std::os::unix::fs::symlink("checked", f.path("etc/fonts")).expect("the link");
    // the stand-in console swaps the link only in the install's own check,
    // whose --with-frit names a copy the install made, and not in the
    // check vtium-config runs on the draft before the install
    script::executable(
        &f.path("vtium"),
        &format!(
            concat!(
                "#!/bin/sh\n[ \"$1\" = check-config ] || exit 9\n",
                "prev=; for a in \"$@\"; do ",
                "case \"$prev $a\" in '--with-frit '*/vtium-config-apply-*) ",
                "ln -s other '{etc}/swap' && mv -T '{etc}/swap' '{etc}/fonts' ;; esac; ",
                "prev=$a; done\nexit 0\n",
            ),
            etc = f.path("etc").display(),
        ),
    );
    f.desk();
    f.ok(&["set", "font-size", "14"]);
    f.keep("new.vtfrit", NEW);
    f.keep("old.removed", "");
    let ran = f.run(&["install"]);
    assert_eq!(ran.status, 0, "{}{}", ran.out, ran.err);
    assert_eq!(
        std::fs::read_link(f.path("etc/fonts")).expect("the link"),
        Path::new("other"),
        "the stand-in console swapped the link"
    );
    let read = |p: &Path| std::fs::read_to_string(p).ok();
    assert_eq!(read(&checked.join("new.vtfrit")).as_deref(), Some(NEW));
    assert_eq!(read(&checked.join("old.vtfrit")), None);
    assert_eq!(read(&checked.join("old.vtfrit.bak")).as_deref(), Some(OLD));
    assert_eq!(
        read(&other.join("new.vtfrit")),
        None,
        "nothing lands in the other folder"
    );
    assert_eq!(read(&other.join("old.vtfrit")).as_deref(), Some(OLD));
}

/// An install run as a user makes the private folder for its check's
/// copies under `XDG_RUNTIME_DIR`. With that variable unset, an install
/// that writes a font set and vtium.conf is refused with words that say
/// why, and writes nothing.
///
/// References:
/// - ADR-1403 D8, "What `install` does": the check of `apply DIR` reads root's copies from a private folder under /run, or the user's runtime folder when apply runs unprivileged.
#[test]
fn an_install_with_no_runtime_folder_is_refused() {
    let f = fixture("apply-no-runtime");
    f.desk();
    f.ok(&["set", "font-size", "14"]);
    f.keep("new.vtfrit", NEW);
    let mut c = f.command(&["install"]);
    c.env_remove("XDG_RUNTIME_DIR");
    let ran = f.run_in(c);
    assert_ne!(ran.status, 0, "{}{}", ran.out, ran.err);
    assert!(
        format!("{}{}", ran.out, ran.err).contains(
            "The install has no private folder for its check of the draft's font sets. As root it makes one under /run, and as a user under XDG_RUNTIME_DIR. XDG_RUNTIME_DIR is not set to an absolute path."
        ),
        "{}{}",
        ran.out,
        ran.err
    );
    assert!(
        !f.path("etc/fonts/new.vtfrit").exists(),
        "nothing is installed"
    );
    assert!(
        !std::fs::read_to_string(f.path("etc/vtium.conf"))
            .unwrap_or_default()
            .contains("font-size=14")
    );
}

/// An install run as a user whose `XDG_RUNTIME_DIR` is a folder other users
/// can write is refused before apply makes anything there. The refusal
/// says that the install has no private folder and why, instead of
/// leaving the console's check to refuse the line of vtium.conf that names
/// the new font set. Nothing is installed, and the folder stays empty.
///
/// References:
/// - ADR-1403 D8, "What `install` does": the check of `apply DIR` reads its copies from a private folder under /run, or the user's runtime folder when apply runs unprivileged.
#[test]
fn an_install_whose_runtime_folder_others_can_write_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture("apply-open-runtime");
    f.desk();
    f.ok(&["set", "font-size", "14"]);
    f.keep("new.vtfrit", NEW);
    let open = f.path("open-run");
    std::fs::create_dir(&open).unwrap();
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
    let mut c = f.command(&["install"]);
    c.env("XDG_RUNTIME_DIR", &open);
    let ran = f.run_in(c);
    let said = format!("{}{}", ran.out, ran.err);
    assert_ne!(ran.status, 0, "{said}");
    // the fixture's output names its root folder ROOT
    assert!(
        said.contains(
            "The install has no private folder for its check of the draft's font sets. As root it makes one under /run, and as a user under XDG_RUNTIME_DIR. vtium reads no font set in ROOT/open-run, so the install makes no copies there: other users can change \"ROOT/open-run\" (mode 777)."
        ),
        "{said}"
    );
    assert_eq!(
        std::fs::read_dir(&open).unwrap().count(),
        0,
        "nothing is made in the open folder"
    );
    assert!(
        !f.path("etc/fonts/new.vtfrit").exists(),
        "nothing is installed"
    );
    assert!(
        !std::fs::read_to_string(f.path("etc/vtium.conf"))
            .unwrap_or_default()
            .contains("font-size=14")
    );
}
