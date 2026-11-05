//! The flow's commands (ADR-1403 D8), run as the binary against a fixture:
//! a configuration directory standing in for `/etc/vtium`, an XDG home of
//! the fixture's own for the store, a machine with no devices, and a
//! stand-in `vtium` whose `check-config` refuses each line that holds
//! `BAD`, so the check's verdict is the fixture's to choose.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// The stand-in console: `check-config --configdir DIR FILE` prints
/// `FILE:N: BAD is not a key vtium reads` on standard error for each line
/// N of FILE that holds `BAD` and exits 1, else notes `FILE: checked` on
/// standard output and exits 0.
const CHECKER: &str = r#"#!/bin/sh
[ "$1" = check-config ] || exit 9
file="$4"
bad=$(grep -n BAD "$file" | cut -d: -f1)
if [ -n "$bad" ]; then
  for n in $bad; do echo "$file:$n: BAD is not a key vtium reads" >&2; done
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
    std::fs::write(&checker, CHECKER).expect("the checker");
    std::fs::set_permissions(&checker, std::fs::Permissions::from_mode(0o755)).expect("its mode");
    Fixture { root }
}

impl Fixture {
    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p)
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
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
            "wrote ROOT/home/config/vtium/drafts/desk.conf\nROOT/home/config/vtium/drafts/desk.conf:3: BAD is not a key vtium reads\ndraft desk: 1 line added · 1 problem\n"
        )
    );
    // the line editor adds a new key after the last top-level line
    assert_eq!(
        f.draft(),
        "font-size=12\nversion = 1\nfont-name=BAD\n[head desk]\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n"
    );
    // installing it fails the check, exit 1, and the draft keeps the change
    let install = f.run(&["install"]);
    assert_eq!(install.status, 1, "{}{}", install.out, install.err);
    assert_eq!(
        install.err,
        "vtium-config: the check found problems in draft desk, so nothing is installed; the draft keeps every change\n"
    );
    assert_eq!(f.installed(), INSTALLED);
    assert!(f.draft().contains("font-name=BAD"));
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
            "vtium-config: ROOT/etc/vtium.conf changed since draft desk was made from it, so nothing is installed. Run vtium config diff to see the difference, or vtium config revert to start again from the installed file.\n"
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
    std::fs::write(&editor, "#!/bin/sh\necho 'BAD line' >> \"$1\"\n").expect("the editor");
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).expect("its mode");
    let mut c = f.command(&["edit", "--install"]);
    c.env("EDITOR", &editor);
    let ran = f.run_with(c, "n\n");
    assert_eq!(
        (ran.status, ran.out.as_str(), ran.err.as_str()),
        (
            1,
            "wrote ROOT/home/config/vtium/drafts/desk.conf\nROOT/home/config/vtium/drafts/desk.conf:7: BAD is not a key vtium reads\nThe check finds problems. Edit again? [y/N] ",
            "vtium-config: the check found problems in draft desk, so nothing is installed; the draft keeps every change\n"
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
            r#""rules":[{"number":1,"line":5,"kind":"display","selector":"*","target":"desk","state":"matches-nothing","words":"matches no display present"},"#,
            r#"{"number":2,"line":6,"kind":"input","selector":"serial:046d:c31c:X","target":"desk","state":"needs-udev","words":"needs udev"},"#,
            r#"{"number":3,"line":7,"kind":"input","selector":"class:keyboard","target":"roaming","state":"matches-nothing","words":"matches no device present"}],"#,
            r#""heads":[{"name":"desk","consoles":1,"look":"default","keys":[]}],"looks":[],"#,
            r#""inputs":[{"name":"046d:c31c:X","detail":"","keys":["serial:046d:c31c:X"],"found":false,"rule":2}],"problems":[],"missing":[]}"#,
            "\n"
        )
        .replace("@DEFAULTS@", &defaults().to_string())
    );
}

/// How many keys a file may set less the one the fixture sets.
fn defaults() -> usize {
    vtium_settings::keys::KEYS
        .iter()
        .filter(|k| k.scope != vtium_settings::keys::Scope::CommandLine)
        .count()
        - 1
}

#[test]
fn a_new_draft_never_overwrites_one_and_the_list_marks_the_current() {
    let f = fixture("drafts");
    f.desk();
    let again = f.run(&["draft", "new", "desk"]);
    assert_eq!(again.status, 2);
    assert!(again.err.contains("exists already"), "{}", again.err);
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
        (&["rule", "add", "input * ->"], "not a rule"),
        (
            &["head", "set", "nobody", "--consoles", "2"],
            "no section named",
        ),
        (&["set", "colour", "red"], "no key named"),
        (&["get", "colour"], "no key named"),
        (&["set", "configdir", "/x"], "command line only"),
        (&["set", "allow-font-ops", "maybe"], "on or off"),
        (&["set", "font-hinting", "heavy"], "light, full"),
        (&["set", "font-name", "a\nb"], "cannot be written"),
        (
            &["set", "font-frits", "fonts"],
            "an absolute path is needed",
        ),
        (&["set", "font-frits", ""], "an absolute path is needed"),
        (&["set"], "set takes KEY VALUE"),
        (&["rule", "frob"], "rule takes add"),
        (&["head", "active", "desk"], "control socket"),
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
        "--- ROOT/home/config/vtium/drafts/desk.conf\n+++ ROOT/home/config/vtium/drafts/desk.conf (after)\n@@ line 5\n-display * -> desk\n@@ line 6\n+display * -> desk\nROOT/home/config/vtium/drafts/desk.conf: checked\nthe check passes; nothing is written\n"
    );
    assert_eq!(f.draft(), INSTALLED);
    let revert = f.run(&["revert", "--dry-run"]);
    assert_eq!(
        revert.out,
        "revert would lose no lines changed of draft desk\n"
    );
}

#[test]
fn route_says_why_no_route_passes_through_an_item() {
    let f = fixture("route");
    f.desk();
    let ran = f.run(&["route", "rule", "1"]);
    assert_eq!(
        (ran.status, ran.out.as_str()),
        (
            0,
            "route card: rule 1\nnot used: matches no display present\n"
        )
    );
    let json = f.run(&["route", "head", "desk", "--json"]);
    assert_eq!(
        json.out,
        "{\"version\":1,\"item\":\"head desk\",\"says\":\"not used: no complete route passes through it\",\"routes\":[]}\n"
    );
    assert_eq!(f.run(&["route", "head", "nobody"]).status, 2);
}
