//! The flow's commands (ADR-1403 D8), run as the binary against a fixture:
//! a configuration directory standing in for `/etc/vtium`, an XDG home of
//! the fixture's own for the store, a machine with no devices, and a
//! stand-in `vtium` whose `check-config` refuses each line that holds
//! `BAD`, so the check's verdict is the fixture's to choose.

#[path = "common/home.rs"]
mod home;
use vtium_testscript as script;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// The stand-in console: `check-config --configdir DIR FILE` prints
/// `FILE:N: BAD is not a key vtium reads`, and the line that says what the
/// error drops, on standard error for each line N of FILE that holds `BAD`
/// and exits 1, else notes `FILE: checked` on
/// standard output and exits 0.
const CHECKER: &str = r#"#!/bin/sh
[ "$1" = check-config ] || exit 9
file="$4"
bad=$(grep -n BAD "$file" | cut -d: -f1)
if [ -n "$bad" ]; then
  for n in $bad; do
    echo "$file:$n: BAD is not a key vtium reads" >&2
    echo "$file:$n: Because of this error, vtium ignores this line." >&2
  done
  exit 1
fi
echo "$file: checked"
"#;

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
        .join("flow")
        .join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("a fresh fixture");
    }
    for dir in ["etc/fonts", "home", "sys", "udev/data"] {
        std::fs::create_dir_all(root.join(dir)).expect("the fixture");
    }
    std::fs::write(root.join("etc/vtium.conf"), INSTALLED).expect("vtium.conf");
    let checker = root.join("vtium");
    script::executable(&checker, CHECKER);
    Fixture { root }
}

impl Fixture {
    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p)
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut c = home::command(BIN);
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
            .env_remove("VISUAL")
            .env_remove("EDITOR");
        c
    }

    fn run(&self, args: &[&str]) -> Ran {
        self.run_with(self.command(args), "")
    }

    fn run_with(&self, mut c: Command, stdin: &str) -> Ran {
        use std::io::Write;
        let mut child = c
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary runs");
        if let Some(mut input) = child.stdin.take() {
            input.write_all(stdin.as_bytes()).expect("its input");
        }
        let o = child.wait_with_output().expect("it ends");
        Ran {
            status: o.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&o.stdout)
                .replace(self.root.to_str().expect("a path"), "ROOT"),
            err: String::from_utf8_lossy(&o.stderr)
                .replace(self.root.to_str().expect("a path"), "ROOT"),
        }
    }

    /// The draft `desk`, made from the installed file and made current.
    fn desk(&self) {
        let made = self.run(&["draft", "new", "desk"]);
        assert_eq!(made.status, 0, "{}{}", made.out, made.err);
    }

    fn draft(&self) -> String {
        std::fs::read_to_string(self.path("home/config/vtium/drafts/desk.conf")).expect("the draft")
    }

    fn installed(&self) -> String {
        std::fs::read_to_string(self.path("etc/vtium.conf")).expect("vtium.conf")
    }
}

#[test]
fn a_rule_without_before_goes_before_the_first_wider_rule() {
    let f = fixture("placement");
    f.desk();
    let ran = f.run(&[
        "rule",
        "add",
        "input path:pci-0000:15:00.0-usb-0:3.3 -> desk",
    ]);
    assert_eq!(
        (ran.status, ran.out.as_str(), ran.err.as_str()),
        (
            0,
            "wrote ROOT/home/config/vtium/drafts/desk.conf\nROOT/home/config/vtium/drafts/desk.conf: checked\ndraft desk: 1 line added · no problems\n",
            ""
        )
    );
    assert_eq!(
        f.draft(),
        "font-size=12\nversion = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput path:pci-0000:15:00.0-usb-0:3.3 -> desk\ninput class:keyboard -> roaming\n"
    );
    assert_eq!(f.installed(), INSTALLED, "nothing is installed");
}

#[test]
fn an_unfinished_draft_is_written_and_the_command_exits_0() {
    let f = fixture("unfinished");
    f.desk();
    let ran = f.run(&["set", "font-name", "BAD"]);
    assert_eq!(
        (ran.status, ran.out.as_str()),
        (
            0,
            "wrote ROOT/home/config/vtium/drafts/desk.conf\nROOT/home/config/vtium/drafts/desk.conf:3: BAD is not a key vtium reads\nROOT/home/config/vtium/drafts/desk.conf:3: Because of this error, vtium ignores this line.\ndraft desk: 1 line added · 1 problem\n"
        )
    );
    // the line editor adds a new key after the last top-level line
    assert_eq!(
        f.draft(),
        "font-size=12\nversion = 1\nfont-name=BAD\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n"
    );
    // flow --json counts the problem as the state line does
    let json = f.run(&["flow", "--json"]);
    assert!(
        json.out.starts_with(r#"{"version":1,"draft":{"name":"desk","state":"changed","added":1,"removed":0,"problems":1},"#),
        "{}",
        json.out
    );
    // installing it fails the check, exit 1, and the draft keeps the change
    let install = f.run(&["install"]);
    assert_eq!(install.status, 1, "{}{}", install.out, install.err);
    assert_eq!(
        install.err,
        "vtium-config: nothing installed: the check found problems in draft desk. The draft keeps your changes.\n"
    );
    assert_eq!(f.installed(), INSTALLED);
    assert!(f.draft().contains("font-name=BAD"));
}

/// ADR-1005 D10: `set` writes a key vtium takes for itself as the line a
/// person writes by hand, `key=value`, and an empty value as `key=`, which
/// empties the key; `get` reads both back. The vtium crate's test
/// `vtium_config_set_and_a_hand_edit_give_the_same_keys` reads this text and
/// a hand-edited one and finds the same keys.
#[test]
fn set_writes_vtiums_own_keys_as_a_person_writes_them() {
    let f = fixture("key-bindings");
    f.desk();
    assert_eq!(f.run(&["set", "key-scroll-up", "<Alt>k"]).status, 0);
    assert_eq!(f.run(&["set", "key-reset", ""]).status, 0);
    assert_eq!(
        f.draft(),
        "font-size=12\nversion = 1\nkey-scroll-up=<Alt>k\nkey-reset=\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n"
    );
    assert_eq!(
        f.run(&["get", "key-scroll-up"]).out,
        "key-scroll-up\t<Alt>k\tdraft\n"
    );
    assert_eq!(f.run(&["get", "key-reset"]).out, "key-reset\t\tdraft\n");
    assert_eq!(
        f.run(&["get", "key-zoom-in"]).out,
        "key-zoom-in\t<Ctrl>plus,<Ctrl>equal\tdefault\n"
    );
}

/// ADR-1005 D10: on a vtium.conf from an earlier release, which binds the
/// paste by its old name, `set key-paste` writes the new name and comments
/// out the old one, and `get` reads the old name's value until then.
#[test]
fn set_moves_a_key_binding_from_its_old_name() {
    let f = fixture("old-name");
    let alpha6 = "grab-paste=<Alt>p\nversion = 1\n[head desk]\n[rules]\ndisplay * -> desk\n";
    std::fs::write(f.path("etc/vtium.conf"), alpha6).expect("vtium.conf");
    f.desk();
    assert_eq!(
        f.run(&["get", "key-paste"]).out,
        "key-paste\t<Alt>p\tdraft\n"
    );
    assert_eq!(f.run(&["set", "key-paste", "<Alt>n"]).status, 0);
    assert_eq!(
        f.draft(),
        "#grab-paste=<Alt>p\nversion = 1\nkey-paste=<Alt>n\n[head desk]\n[rules]\ndisplay * -> desk\n"
    );
}

#[test]
fn install_refuses_a_draft_made_before_the_installed_file_changed() {
    let f = fixture("changed-since");
    f.desk();
    assert_eq!(f.run(&["set", "font-size", "14"]).status, 0);
    let by_hand = format!("{INSTALLED}# root edited this by hand\n");
    std::fs::write(f.path("etc/vtium.conf"), &by_hand).expect("an edit by hand");
    let flow = f.run(&["flow"]);
    assert_eq!(
        flow.out.lines().next(),
        Some("draft desk: made before the installed file changed · no problems")
    );
    let ran = f.run(&["install"]);
    assert_eq!(
        (ran.status, ran.err.as_str()),
        (
            3,
            "vtium-config: Nothing installed: ROOT/etc/vtium.conf changed after draft desk was made from it. Run vtium config diff to see the difference, or vtium config revert to start again from the installed file.\n"
        )
    );
    assert_eq!(f.installed(), by_hand);
}

#[test]
fn install_installs_the_draft_and_keeps_the_file_it_replaced() {
    let f = fixture("install");
    f.desk();
    assert_eq!(f.run(&["set", "font-size", "14"]).status, 0);
    let ran = f.run(&["install"]);
    assert_eq!(ran.status, 0, "{}{}", ran.out, ran.err);
    assert!(
        ran.out
            .ends_with("kept ROOT/home/config/vtium/installed/desk-1.conf\ninstalled draft desk\n"),
        "{}",
        ran.out
    );
    assert_eq!(f.installed(), f.draft());
    assert!(f.installed().starts_with("font-size=14\n"));
    assert_eq!(
        std::fs::read_to_string(f.path("home/config/vtium/installed/desk-1.conf"))
            .expect("the history"),
        INSTALLED
    );
    let flow = f.run(&["flow"]);
    assert_eq!(
        flow.out.lines().next(),
        Some("draft desk: same as installed · no problems")
    );
    let history = f.run(&["history"]);
    assert!(
        history.out.starts_with("1\tdesk-1.conf\t"),
        "{}",
        history.out
    );
    // restore brings the replaced file back into the draft
    let restore = f.run(&["restore"]);
    assert_eq!(restore.status, 0, "{}", restore.err);
    assert_eq!(f.draft(), INSTALLED);
}

#[test]
fn edit_installs_nothing_when_the_editor_adds_a_bad_line() {
    let f = fixture("edit");
    f.desk();
    let editor = f.path("editor");
    script::executable(&editor, "#!/bin/sh\necho 'BAD line' >> \"$1\"\n");
    let mut c = f.command(&["edit", "--install"]);
    c.env("EDITOR", &editor);
    let ran = f.run_with(c, "n\n");
    assert_eq!(
        (ran.status, ran.out.as_str(), ran.err.as_str()),
        (
            1,
            "wrote ROOT/home/config/vtium/drafts/desk.conf\nROOT/home/config/vtium/drafts/desk.conf:7: BAD is not a key vtium reads\nROOT/home/config/vtium/drafts/desk.conf:7: Because of this error, vtium ignores this line.\nThe check found problems. Edit again? [y/N] ",
            "vtium-config: nothing installed: the check found problems in draft desk. The draft keeps your changes.\n"
        )
    );
    assert_eq!(f.installed(), INSTALLED);
    assert_eq!(f.draft(), format!("{INSTALLED}BAD line\n"));
}

#[test]
fn flow_json_carries_its_version_and_the_draft_s_state() {
    // no udev daemon runs in the fixture, so the serial rule needs udev,
    // and the device it names stands in its column, not found
    let f = fixture("json");
    f.desk();
    assert_eq!(
        f.run(&["rule", "add", "input serial:046d:c31c:X -> desk"])
            .status,
        0
    );
    let ran = f.run(&["flow", "--json"]);
    assert_eq!(ran.status, 0, "{}", ran.err);
    assert_eq!(
        ran.out,
        concat!(
            r#"{"version":1,"draft":{"name":"desk","state":"changed","added":1,"removed":0,"problems":0},"policy":"open","#,
            r#""base":{"set":[{"key":"font-size","value":"12"}],"defaults":"#,
            "@DEFAULTS@",
            r#"},"displays":[],"#,
            r#""rules":[{"number":1,"line":5,"kind":"display","selector":"*","target":"desk","state":"matches-nothing","words":"matches no connected display"},"#,
            r#"{"number":2,"line":6,"kind":"input","selector":"serial:046d:c31c:X","target":"desk","state":"needs-udev","words":"needs udev"},"#,
            r#"{"number":3,"line":7,"kind":"input","selector":"class:keyboard","target":"roaming","state":"matches-nothing","words":"matches no connected device"}],"#,
            r#""heads":[{"name":"desk","consoles":1,"look":"default","keys":[]}],"looks":[],"#,
            r#""inputs":[{"name":"046d:c31c:X","detail":"","keys":["serial:046d:c31c:X"],"found":false,"rule":2}],"problems":[],"missing":[]}"#,
            "\n"
        )
        .replace("@DEFAULTS@", &defaults().to_string())
    );
}

/// How many keys a file's top level may set less the one the fixture
/// sets.
fn defaults() -> usize {
    vtium_settings::keys::KEYS
        .iter()
        .filter(|k| k.scope == vtium_settings::keys::Scope::Anywhere)
        .count()
        - 1
}

#[test]
fn a_new_draft_never_overwrites_one_and_the_list_marks_the_current() {
    let f = fixture("drafts");
    f.desk();
    let again = f.run(&["draft", "new", "desk"]);
    assert_eq!(again.status, 2);
    assert!(again.err.contains("already exists"), "{}", again.err);
    assert_eq!(
        f.run(&["draft", "new", "spare", "--from", "empty"]).status,
        0
    );
    assert_eq!(f.run(&["set", "font-size", "9"]).status, 0);
    let list = f.run(&["draft", "list"]);
    assert_eq!(
        list.out,
        format!("  desk\tsame as installed\n* spare\t{}\n", {
            let spare = std::fs::read_to_string(f.path("home/config/vtium/drafts/spare.conf"))
                .expect("spare");
            let (a, r) = vtium_config::drafts::counts(INSTALLED, &spare);
            vtium_config::drafts::lines(a, r)
        })
    );
    assert_eq!(f.run(&["draft", "switch", "desk"]).status, 0);
    assert_eq!(
        f.run(&["draft", "rm", "spare"]).out,
        "removed draft spare\n"
    );
    // with every draft gone, the next command makes one named for the host
    assert_eq!(f.run(&["draft", "rm"]).status, 0);
    let host = vtium_config::store::hostname();
    let get = f.run(&["get", "font-size"]);
    assert_eq!(get.out, "font-size\t12\tdraft\n");
    assert!(
        f.path(&format!("home/config/vtium/drafts/{host}.conf"))
            .is_file()
    );
}

#[test]
fn a_command_that_cannot_be_used_exits_2_and_changes_nothing() {
    let f = fixture("usage");
    f.desk();
    for (args, says) in [
        (&["rule", "rm", "9"][..], "no rule 9"),
        (&["rule", "add", "input * ->"], "not a valid rule"),
        (
            &["head", "set", "nobody", "--consoles", "2"],
            "no section named",
        ),
        (&["set", "colour", "red"], "there is no setting named"),
        (&["get", "colour"], "there is no setting named"),
        (
            &["set", "configdir", "/x"],
            "can be set only on vtium's command line",
        ),
        (&["set", "allow-font-ops", "maybe"], "on or off"),
        (&["set", "font-hinting", "heavy"], "light, full"),
        (&["set", "font-name", "a\nb"], "cannot be set to"),
        (&["set", "font-frits", "fonts"], "the path must be absolute"),
        (&["set", "font-frits", ""], "the path must be absolute"),
        (&["set"], "set takes KEY VALUE"),
        (&["rule", "frob"], "rule takes add"),
        (
            &["head", "active", "desk"],
            "has no way to receive the request",
        ),
    ] {
        let ran = f.run(args);
        assert_eq!(ran.status, 2, "{args:?}: {}", ran.err);
        assert!(ran.err.contains(says), "{args:?}: {}", ran.err);
    }
    assert_eq!(f.draft(), INSTALLED);
}

#[test]
fn dry_run_prints_the_change_and_writes_nothing() {
    let f = fixture("dry-run");
    f.desk();
    let ran = f.run(&["rule", "move", "2", "--before", "1", "--dry-run"]);
    assert_eq!(
        ran.out,
        "--- ROOT/home/config/vtium/drafts/desk.conf\n+++ ROOT/home/config/vtium/drafts/desk.conf (after)\n@@ line 5\n-display * -> desk\n@@ line 6\n+display * -> desk\nROOT/home/config/vtium/drafts/desk.conf: checked\nthe check passes; nothing was saved (--dry-run)\n"
    );
    assert_eq!(f.draft(), INSTALLED);
    let revert = f.run(&["revert", "--dry-run"]);
    assert_eq!(
        revert.out,
        "revert would lose no lines changed of draft desk\n"
    );
}

#[test]
fn a_removed_head_keeps_its_rules_and_a_rename_takes_them_along() {
    let f = fixture("head-rm");
    f.desk();
    assert_eq!(f.run(&["head", "add", "panel"]).status, 0);
    let rename = f.run(&["head", "set", "desk", "--name", "office"]);
    assert_eq!(rename.status, 0, "{}", rename.err);
    assert_eq!(
        f.draft(),
        "font-size=12\nversion = 1\n[head office]\n[head panel]\n\n[rules]\ndisplay * -> office\ninput class:keyboard -> roaming\n"
    );
    let ran = f.run(&["head", "rm", "office"]);
    assert_eq!(
        (ran.status, ran.out.as_str()),
        (
            0,
            "rules 1 still send to office; the check reports them until you change or remove each one\nwrote ROOT/home/config/vtium/drafts/desk.conf\nROOT/home/config/vtium/drafts/desk.conf: checked\ndraft desk: 3 lines added, 2 removed · no problems\n"
        )
    );
    assert_eq!(
        f.draft(),
        "font-size=12\nversion = 1\n[head panel]\n\n[rules]\ndisplay * -> office\ninput class:keyboard -> roaming\n"
    );
    // the broken rule is still the commands' to fix
    assert_eq!(f.run(&["rule", "set", "1", "--target", "panel"]).status, 0);
    assert!(f.draft().contains("display * -> panel\n"));
}

#[test]
fn a_look_in_use_is_kept_unless_forced_to_the_default() {
    let f = fixture("look-rm");
    f.desk();
    assert_eq!(
        f.run(&["look", "add", "big", "--frit", "monoid", "--size", "14"])
            .status,
        0
    );
    assert_eq!(f.run(&["head", "set", "desk", "--look", "big"]).status, 0);
    let kept = f.run(&["look", "rm", "big"]);
    assert_eq!(kept.status, 2);
    assert!(
        kept.err
            .contains("Head desk uses look big. Switch it to another look first"),
        "{}",
        kept.err
    );
    assert_eq!(f.run(&["look", "rm", "big", "--force"]).status, 0);
    assert_eq!(
        f.draft(),
        "font-size=12\nversion = 1\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n"
    );
}

// ADR-1403 D5, D8: `look add` and `look set` take a look's cell, and
// `set`, which writes the top level, refuses it and names `look set`
#[test]
fn look_add_and_look_set_take_a_look_s_cell() {
    let f = fixture("look-cell");
    f.desk();
    let added = f.run(&["look", "add", "big", "--frit", "monoid", "--cell", "12x24"]);
    assert_eq!(added.status, 0, "{}", added.err);
    assert!(
        f.draft()
            .contains("[look big]\nfont-name = monoid\ncell = 12x24\n"),
        "{}",
        f.draft()
    );
    let set = f.run(&["look", "set", "big", "--cell", "10x20"]);
    assert_eq!(set.status, 0, "{}", set.err);
    assert!(
        f.draft()
            .contains("[look big]\nfont-name = monoid\ncell = 10x20\n"),
        "{}",
        f.draft()
    );
    let before = f.draft();
    let top = f.run(&["set", "cell", "8x16"]);
    assert_eq!(top.status, 2);
    assert!(
        top.err.contains("only in a [look NAME] section")
            && top.err.contains("look set NAME --cell VALUE"),
        "{}",
        top.err
    );
    assert_eq!(f.draft(), before);
}

#[test]
fn the_library_keeps_looks_and_rules_and_lets_them_go() {
    let f = fixture("library");
    f.desk();
    assert_eq!(f.run(&["look", "add", "big", "--frit", "monoid"]).status, 0);
    assert_eq!(f.run(&["library", "add", "look", "big"]).status, 0);
    assert_eq!(f.run(&["library", "add", "rule", "2"]).status, 0);
    assert_eq!(
        f.run(&["library", "list"]).out,
        "look big\nrule 1\tinput class:keyboard -> roaming\n"
    );
    assert_eq!(f.run(&["library", "rm", "look", "big"]).status, 0);
    assert_eq!(f.run(&["library", "rm", "rule", "1"]).status, 0);
    assert_eq!(
        std::fs::read_to_string(f.path("home/config/vtium/library.conf")).expect("the library"),
        "version = 1\n\n[rules]\n"
    );
}

#[test]
fn a_draft_name_that_leaves_the_store_is_refused_and_touches_nothing() {
    let f = fixture("traversal");
    f.desk();
    std::fs::write(f.path("home/config/vtium/library.conf"), "version = 1\n").expect("a library");
    for args in [
        &["draft", "rm", "../library"][..],
        &["draft", "switch", "../library"],
        &["draft", "new", "../escape"],
        &["draft", "new", "spare", "--from", "../library"],
        &["draft", "rm", "a/b"],
        &["draft", "switch", ".."],
    ] {
        let ran = f.run(args);
        assert_eq!(ran.status, 2, "{args:?}: {}{}", ran.out, ran.err);
        assert!(
            ran.err.contains("cannot be a draft's name"),
            "{args:?}: {}",
            ran.err
        );
    }
    assert_eq!(
        std::fs::read_to_string(f.path("home/config/vtium/library.conf")).expect("the library"),
        "version = 1\n"
    );
    assert!(!f.path("home/config/escape.conf").exists());
    assert_eq!(f.run(&["draft", "list"]).out, "* desk\tsame as installed\n");
}

#[test]
fn a_draft_copied_in_takes_its_base_when_first_seen() {
    let f = fixture("first-sight");
    f.desk();
    // a draft carried in from another machine has no base
    std::fs::write(
        f.path("home/config/vtium/drafts/carried.conf"),
        format!("{INSTALLED}# carried\n"),
    )
    .expect("a carried draft");
    assert_eq!(f.run(&["draft", "switch", "carried"]).status, 0);
    std::fs::write(f.path("etc/vtium.conf"), format!("{INSTALLED}# by hand\n"))
        .expect("an edit by hand");
    let ran = f.run(&["install"]);
    assert_eq!(ran.status, 3, "{}{}", ran.out, ran.err);
    assert_eq!(f.installed(), format!("{INSTALLED}# by hand\n"));
}

#[test]
fn the_history_keeps_the_newest_files_its_setting_allows() {
    let f = fixture("history");
    f.desk();
    std::fs::write(
        f.path("home/config/vtium/vtium-config.conf"),
        "draft = desk\nhistory = 2\n",
    )
    .expect("the settings");
    for size in ["13", "14", "15"] {
        assert_eq!(f.run(&["set", "font-size", size]).status, 0);
        let ran = f.run(&["install"]);
        assert_eq!(ran.status, 0, "{}{}", ran.out, ran.err);
    }
    let mut kept: Vec<String> = std::fs::read_dir(f.path("home/config/vtium/installed"))
        .expect("the history")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    kept.sort();
    assert_eq!(kept, ["desk-2.conf", "desk-3.conf"]);
    assert!(
        std::fs::read_to_string(f.path("home/config/vtium/installed/desk-3.conf"))
            .expect("the newest")
            .starts_with("font-size=14\n")
    );
}

/// The stand-in console of [`CHECKER`], which also refuses a file that
/// holds `NOROUTE` with vtium's own message for a file whose rules wire no
/// complete route (ADR-1403 D3), on the file and no line of it.
fn no_route_checker(f: &Fixture) {
    let message = vtium_settings::Missing::NoDisplayRule
        .to_string()
        .replace('\'', r"'\''");
    let script = CHECKER.replace(
        "if [ -n \"$bad\" ]; then",
        &format!(
            "if grep -q NOROUTE \"$file\"; then\n  printf '%s\\n' \"$file: \"'{message}' >&2\n  [ -n \"$bad\" ] || exit 1\nfi\nif [ -n \"$bad\" ]; then"
        ),
    );
    script::executable(&f.path("vtium"), &script);
}

// ADR-1403 D8: a draft whose only problem is a missing complete route is
// refused without a terminal, names --force, and installs with --force;
// --force skips no other refusal
#[test]
fn install_force_installs_a_draft_with_no_complete_path_and_nothing_else() {
    let f = fixture("no-route");
    no_route_checker(&f);
    f.desk();
    assert_eq!(f.run(&["set", "font-name", "NOROUTE"]).status, 0);
    // no terminal and no --force: exit 1, and the way on
    let refused = f.run(&["install"]);
    assert_eq!(
        (refused.status, refused.err.as_str()),
        (
            1,
            "vtium-config: This draft has no complete path: no connected display is sent to a head that a connected keyboard reaches. To install it anyway, run vtium config install --force.\n"
        ),
        "{}",
        refused.out
    );
    assert_eq!(f.installed(), INSTALLED);
    // a command's --install installs only a draft the check passes
    let set = f.run(&["set", "font-size", "13", "--install"]);
    assert_eq!(set.status, 1, "{}{}", set.out, set.err);
    assert_eq!(f.installed(), INSTALLED);
    // --force installs it, and apply accepts that one problem because the
    // bundle records the --force
    let forced = f.run(&["install", "--force"]);
    assert_eq!(forced.status, 0, "{}{}", forced.out, forced.err);
    assert!(
        forced.out.contains(vtium_config::apply::ACCEPTED_NO_ROUTE),
        "{}",
        forced.out
    );
    assert_eq!(f.installed(), f.draft());
    assert!(f.installed().contains("font-name=NOROUTE"));
}

#[test]
fn install_force_skips_no_other_problem() {
    let f = fixture("no-route-and-bad");
    no_route_checker(&f);
    f.desk();
    assert_eq!(f.run(&["set", "font-name", "NOROUTE"]).status, 0);
    assert_eq!(f.run(&["set", "login", "BAD"]).status, 0);
    let ran = f.run(&["install", "--force"]);
    assert_eq!(
        (ran.status, ran.err.as_str()),
        (
            1,
            "vtium-config: nothing installed: the check found problems in draft desk. The draft keeps your changes.\n"
        ),
        "{}",
        ran.out
    );
    assert_eq!(f.installed(), INSTALLED);
}

// ADR-1403 D8: --force never skips the refusal of a draft made before the
// installed file changed
#[test]
fn install_force_still_refuses_a_draft_made_before_the_installed_file_changed() {
    let f = fixture("no-route-stale");
    no_route_checker(&f);
    f.desk();
    assert_eq!(f.run(&["set", "font-name", "NOROUTE"]).status, 0);
    let by_hand = format!("{INSTALLED}# root edited this by hand\n");
    std::fs::write(f.path("etc/vtium.conf"), &by_hand).expect("an edit by hand");
    let ran = f.run(&["install", "--force"]);
    assert_eq!(ran.status, 3, "{}{}", ran.out, ran.err);
    assert_eq!(f.installed(), by_hand);
}

/// The maintainer, 2026-11-11: with no vtium.conf installed, a new draft
/// is vtium-config's short header and nothing else, and each change adds
/// only the lines it sets.
#[test]
fn a_draft_made_with_nothing_installed_is_the_header_alone() {
    let f = fixture("lean");
    std::fs::remove_file(f.path("etc/vtium.conf")).expect("no vtium.conf");
    f.desk();
    let header = vtium_config::drafts::HEADER;
    assert_eq!(f.draft(), header);
    assert!(
        header.starts_with("# This is vtium's configuration file, /etc/vtium/vtium.conf."),
        "{header}"
    );
    assert!(
        header
            .lines()
            .all(|l| l.is_empty() || (l.starts_with('#') && l.chars().count() <= 78)),
        "the header is comments wrapped at 78 columns"
    );
    assert_eq!(header.lines().count(), 6);
    // a key the file does not have adds its own line and nothing else
    assert_eq!(f.run(&["set", "font-size", "14"]).status, 0);
    assert_eq!(f.draft(), format!("{header}font-size=14\n"));
    assert_eq!(f.run(&["set", "--unset", "font-size"]).status, 0);
    assert_eq!(f.draft(), format!("{header}#font-size=14\n"));
    // new and revert start from the header too
    assert_eq!(f.run(&["new"]).status, 0);
    assert_eq!(
        f.draft(),
        format!("{}\n\nversion = 1\n\n[look default]\n", header.trim_end())
    );
    assert_eq!(f.run(&["revert"]).status, 0);
    assert_eq!(f.draft(), header);
    // a draft from empty is new's text
    assert_eq!(
        f.run(&["draft", "new", "spare", "--from", "empty"]).status,
        0
    );
    let spare =
        std::fs::read_to_string(f.path("home/config/vtium/drafts/spare.conf")).expect("spare");
    assert_eq!(
        spare,
        format!("{}\n\nversion = 1\n\n[look default]\n", header.trim_end())
    );
}

/// The maintainer, 2026-11-11, "Leave it": an installed vtium.conf that
/// holds the whole shipped example keeps every line it had, comments
/// included, when a change sets a key.
#[test]
fn a_full_installed_file_keeps_every_line_but_the_one_a_change_sets() {
    let f = fixture("full");
    let example = vtium_config::drafts::EXAMPLE;
    std::fs::write(f.path("etc/vtium.conf"), example).expect("vtium.conf");
    f.desk();
    assert_eq!(f.draft(), example);
    assert_eq!(f.run(&["set", "font-size", "14"]).status, 0);
    let draft = f.draft();
    let mut kept: Vec<&str> = draft.lines().collect();
    let at = kept
        .iter()
        .position(|l| *l == "font-size=14")
        .expect("the new line");
    assert_eq!(kept.get(at - 1), Some(&"#font-size=0"), "below its comment");
    kept.remove(at);
    assert_eq!(kept, example.lines().collect::<Vec<_>>());
}
