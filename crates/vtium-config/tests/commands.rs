//! The commands that list, check and change the console's fonts and
//! settings (ADR-1201 D2), run as the binary in a temporary directory
//! standing in for `/etc/vtium` through the tool's path options, never as
//! root: each change a user makes is a draft, which `apply` installs.

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "common/console.rs"]
mod console;
use console::vtium;
use vtium_config::draft::changes;

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// A configuration directory, a frits directory holding `tiny` and
/// `spare`, and a state directory for drafts, all the test's own.
struct Fixture {
    root: PathBuf,
}

fn fixture(name: &str, conf: Option<&str>) -> Fixture {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("commands")
        .join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("a fresh fixture");
    }
    let fonts = root.join("etc/fonts");
    std::fs::create_dir_all(&fonts).expect("the fixture");
    std::fs::create_dir_all(root.join("state")).expect("the state");
    std::fs::write(fonts.join("tiny.vtfrit"), "source=builtin:unscii-8\n").expect("a frit");
    std::fs::write(fonts.join("spare.vtfrit"), "source=builtin:unifont\n").expect("a frit");
    if let Some(conf) = conf {
        std::fs::write(root.join("etc/vtium.conf"), conf).expect("vtium.conf");
    }
    Fixture { root }
}

/// What a command printed: its status, its standard output and its
/// standard error.
struct Ran {
    status: i32,
    out: String,
    err: String,
}

impl Fixture {
    fn etc(&self) -> PathBuf {
        self.root.join("etc")
    }

    fn conf(&self) -> String {
        std::fs::read_to_string(self.etc().join("vtium.conf")).expect("vtium.conf")
    }

    /// The tool's path options for the fixture.
    fn options(&self) -> Vec<std::ffi::OsString> {
        vec![
            "--configdir".into(),
            self.etc().into(),
            "--frits".into(),
            self.etc().join("fonts").into(),
            "--vtium".into(),
            vtium().into(),
        ]
    }

    /// `vtium-config` with the fixture's options and `args`, its drafts
    /// under the fixture's state directory.
    fn run(&self, args: &[&str]) -> Ran {
        let o = Command::new(BIN)
            .args(self.options())
            .args(args)
            .env("XDG_STATE_HOME", self.root.join("state"))
            .output()
            .expect("the binary runs");
        Ran {
            status: o.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&o.stdout).into_owned(),
            err: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }

    /// The draft a change printed, after checking that nothing is
    /// installed yet, then `apply` run on it as the printed command runs it.
    fn apply_printed(&self, ran: &Ran) -> Ran {
        assert_eq!(ran.status, 0, "{}{}", ran.out, ran.err);
        let draft = ran
            .out
            .lines()
            .find_map(|l| l.strip_prefix("draft\t"))
            .expect("the draft's line");
        let run = ran
            .out
            .lines()
            .find_map(|l| l.strip_prefix("run\t"))
            .expect("the command's line");
        assert!(run.ends_with(&format!("apply {draft}")), "{run}");
        assert!(
            draft.starts_with(
                self.root
                    .join("state/vtium/drafts")
                    .to_str()
                    .expect("a path")
            ),
            "{draft}"
        );
        self.run(&["apply", draft])
    }
}

fn example() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vtium/install/vtium.conf.example");
    std::fs::read_to_string(path).expect("the shipped example")
}

#[test]
fn config_prints_a_key_s_value_and_where_it_comes_from() {
    let f = fixture("config-print", Some("font-size=12\nno-allow-font-ops\n"));
    for (key, line) in [
        ("font-size", "font-size\t12\tvtium.conf"),
        ("allow-font-ops", "allow-font-ops\toff\tvtium.conf"),
        ("font-hinting", "font-hinting\tunset\tdefault"),
        ("sb-size", "sb-size\t1000\tdefault"),
        ("font-name", "font-name\tunset\tdefault"),
    ] {
        let ran = f.run(&["config", key]);
        assert_eq!(ran.status, 0, "{key}: {}", ran.err);
        assert_eq!(ran.out, format!("{line}\n"), "{key}");
    }
}

#[test]
fn config_refuses_a_key_or_value_vtium_conf_does_not_take() {
    let f = fixture("config-refuse", None);
    for (args, says) in [
        (&["config", "colour"][..], "no key named"),
        (&["config", "configdir", "/x"], "command line only"),
        (&["config", "allow-font-ops", "maybe"], "on or off"),
        (&["config", "font-hinting", "heavy"], "light, full"),
        (&["config", "font-name", "a\nb"], "cannot be written"),
    ] {
        let ran = f.run(args);
        assert_eq!(ran.status, 1, "{args:?}: {}", ran.out);
        assert!(ran.err.contains(says), "{args:?}: {}", ran.err);
    }
    let ran = f.run(&["config"]);
    assert_eq!(ran.status, 2, "{}", ran.err);
    assert!(!f.root.join("state/vtium").exists(), "no draft was made");
}

#[test]
fn config_sets_a_key_through_a_draft_that_changes_one_line() {
    let shipped = example();
    let f = fixture("config-set", Some(&shipped));
    let ran = f.run(&["config", "font-size", "12"]);
    assert!(
        ran.out.starts_with(&format!(
            "write\t{}\n",
            f.etc().join("vtium.conf").display()
        )),
        "{}",
        ran.out
    );
    assert_eq!(f.conf(), shipped, "a user's change only drafts");
    let applied = f.apply_printed(&ran);
    assert_eq!(applied.status, 0, "{}{}", applied.out, applied.err);
    let diff = changes(&shipped, &f.conf());
    assert_eq!(diff.len(), 1, "{diff:?}");
    assert_eq!(diff[0].old, None);
    assert_eq!(diff[0].new.as_deref(), Some("font-size=12"));
    // the same value again changes nothing and makes no draft
    let again = f.run(&["config", "font-size", "12"]);
    assert_eq!((again.status, again.out.as_str()), (0, "unchanged\n"));
    // and --unset comments the line out
    let unset = f.run(&["config", "--unset", "font-size"]);
    assert_eq!(f.apply_printed(&unset).status, 0);
    assert!(f.conf().contains("\n#font-size=12\n"), "{}", f.conf());
    assert_eq!(
        f.run(&["config", "font-size"]).out,
        "font-size\t0\tdefault\n"
    );
    // a switch is written as vtium.conf writes switches
    let off = f.run(&["config", "allow-font-ops", "off"]);
    assert_eq!(f.apply_printed(&off).status, 0);
    assert!(f.conf().contains("\nno-allow-font-ops\n"), "{}", f.conf());
}

#[test]
fn remove_frit_removes_a_frit_but_not_the_one_font_name_names() {
    let f = fixture("remove", Some("font-name=tiny\n"));
    let fonts = f.etc().join("fonts");
    for (name, says) in [
        ("tiny", "font-name names"),
        ("unifont", "built-in font"),
        ("nosuch", "nosuch.vtfrit"),
        ("../x", "no frit's name"),
    ] {
        let ran = f.run(&["remove-frit", name]);
        assert_eq!(ran.status, 1, "{name}: {}", ran.out);
        assert!(ran.err.contains(says), "{name}: {}", ran.err);
    }
    // a head's profile draws the frit of its name
    std::fs::write(
        f.etc().join("heads.conf"),
        "version = 1\n[head spare]\nprofile = spare\n[rules]\ndisplay * -> spare\n",
    )
    .expect("heads.conf");
    let ran = f.run(&["remove-frit", "spare"]);
    assert_eq!(ran.status, 1, "{}", ran.out);
    assert!(ran.err.contains("head spare's profile"), "{}", ran.err);
    std::fs::remove_file(f.etc().join("heads.conf")).expect("no head file");
    let ran = f.run(&["remove-frit", "spare"]);
    assert!(
        ran.out.starts_with(&format!(
            "remove\t{}\n",
            fonts.join("spare.vtfrit").display()
        )),
        "{}",
        ran.out
    );
    assert!(
        fonts.join("spare.vtfrit").exists(),
        "a user's removal only drafts"
    );
    let applied = f.apply_printed(&ran);
    assert_eq!(applied.status, 0, "{}{}", applied.out, applied.err);
    assert!(!fonts.join("spare.vtfrit").exists());
    assert_eq!(
        std::fs::read_to_string(fonts.join("spare.vtfrit.bak")).expect("kept as .bak"),
        "source=builtin:unifont\n"
    );
    assert!(fonts.join("tiny.vtfrit").exists());
}

#[test]
fn new_makes_a_frit_of_fonts_and_installs_one_from_a_file() {
    let f = fixture("new", None);
    let fonts = f.etc().join("fonts");
    let ran = f.run(&[
        "new",
        "builtin:unscii-8",
        "Some Family",
        "--name",
        "mine",
        "--cell",
        "8x16",
    ]);
    let applied = f.apply_printed(&ran);
    assert_eq!(applied.status, 0, "{}{}", applied.out, applied.err);
    let made = std::fs::read_to_string(fonts.join("mine.vtfrit")).expect("the new frit");
    assert!(made.contains("\nname=mine\n"), "{made}");
    assert!(
        made.contains("\nsource=builtin:unscii-8\nsource=font:Some Family\ncell=8x16\n"),
        "{made}"
    );
    // the template prints a frit that --from installs, named by its file
    let template = f.run(&["new", "--template"]);
    assert_eq!(template.status, 0, "{}", template.err);
    let file = f.root.join("edited.vtfrit");
    std::fs::write(&file, &template.out).expect("the template");
    let from = f.run(&["new", "--from", file.to_str().expect("a path")]);
    assert_eq!(f.apply_printed(&from).status, 0);
    assert_eq!(
        std::fs::read_to_string(fonts.join("edited.vtfrit")).expect("installed"),
        template.out
    );
    // a name the console keeps for a built-in font, a frit the reader
    // refuses and words new does not take are refused, and draft nothing
    for (args, status, says) in [
        (
            &["new", "x", "--name", "unifont"][..],
            1,
            "cannot name a frit",
        ),
        (&["new", "x", "--cell", "200x200"], 1, "cell="),
        (&["new", "--from", "/nonexistent.vtfrit"], 1, "nonexistent"),
        (&["new", "--frobnicate"], 2, "--frobnicate"),
        (&["new", "x", "--from", "y"], 2, "takes"),
    ] {
        let ran = f.run(args);
        assert_eq!(ran.status, status, "{args:?}: {}", ran.out);
        assert!(ran.err.contains(says), "{args:?}: {}", ran.err);
        assert!(ran.out.is_empty(), "{args:?}: {}", ran.out);
    }
}

/// vtium.conf's `font-frits` names the frits' directory the console
/// reads, so with no `--frits` every command lists, writes and removes
/// frits there, and never in the build's default directory.
#[test]
fn font_frits_names_the_directory_every_command_acts_in() {
    let f = fixture("font-frits", None);
    let elsewhere = f.root.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("the frits' directory");
    std::fs::write(elsewhere.join("gone.vtfrit"), "source=builtin:unifont\n").expect("a frit");
    std::fs::write(
        f.etc().join("vtium.conf"),
        format!("font-frits={}\n", elsewhere.display()),
    )
    .expect("vtium.conf");
    // the fixture's options less --frits
    let run = |args: &[&str]| {
        let o = Command::new(BIN)
            .arg("--configdir")
            .arg(f.etc())
            .arg("--vtium")
            .arg(vtium())
            .args(args)
            .env("XDG_STATE_HOME", f.root.join("state"))
            .output()
            .expect("the binary runs");
        Ran {
            status: o.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&o.stdout).into_owned(),
            err: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    };
    // apply, run as the drafting command printed it would be run, with no
    // --frits either
    let apply = |ran: &Ran| {
        assert_eq!(ran.status, 0, "{}{}", ran.out, ran.err);
        let line = |tag: &str| {
            ran.out
                .lines()
                .find_map(|l| l.strip_prefix(tag))
                .expect("the line")
                .to_owned()
        };
        assert!(
            line("run\t").contains(&format!("--frits {} ", elsewhere.display())),
            "{}",
            ran.out
        );
        run(&["apply", &line("draft\t")])
    };
    let listed = run(&["fonts"]);
    assert_eq!(listed.status, 0, "{}", listed.err);
    let names: Vec<&str> = listed
        .out
        .lines()
        .filter_map(|l| l.split('\t').next())
        .collect();
    assert_eq!(names, ["gone", "unifont", "unscii-8"], "{}", listed.out);
    let made = run(&["new", "builtin:unscii-8", "--name", "made"]);
    assert!(
        made.out.starts_with(&format!(
            "write\t{}\n",
            elsewhere.join("made.vtfrit").display()
        )),
        "{}{}",
        made.out,
        made.err
    );
    assert_eq!(apply(&made).status, 0);
    assert!(elsewhere.join("made.vtfrit").is_file());
    let removed = run(&["remove-frit", "gone"]);
    assert!(
        removed.out.starts_with(&format!(
            "remove\t{}\n",
            elsewhere.join("gone.vtfrit").display()
        )),
        "{}{}",
        removed.out,
        removed.err
    );
    assert_eq!(apply(&removed).status, 0);
    assert!(!elsewhere.join("gone.vtfrit").exists());
    // the fixture's own frits, in the directory --frits would name, are
    // untouched
    assert!(f.etc().join("fonts/tiny.vtfrit").is_file());
    assert!(!f.etc().join("fonts/made.vtfrit").exists());
}

#[test]
fn check_judges_a_frit_and_a_vtium_conf_and_writes_nothing() {
    let f = fixture("check", None);
    let good = f.root.join("good.vtfrit");
    std::fs::write(&good, "source=builtin:unifont\ncompress=yes\n").expect("a frit");
    let ran = f.run(&["check", good.to_str().expect("a path")]);
    assert_eq!(ran.status, 0, "{}", ran.err);
    assert!(
        ran.out.contains("warning\t"),
        "compress= warns: {}",
        ran.out
    );
    assert!(
        ran.out.ends_with(&format!("ok\t{}\n", good.display())),
        "{}",
        ran.out
    );
    let bad = f.root.join("bad.vtfrit");
    std::fs::write(&bad, "source=builtin:unifont\ncolour=red\n").expect("a frit");
    let ran = f.run(&["check", bad.to_str().expect("a path")]);
    assert_eq!(ran.status, 1, "{}", ran.out);
    assert!(ran.err.contains("bad.vtfrit:2"), "{}", ran.err);
    // vtium.conf is judged by the console's own check, under its own name
    let conf = f.root.join("mine.conf");
    std::fs::write(&conf, "font-size=12\nfont-size=abc\n").expect("a conf");
    let ran = f.run(&["check", conf.to_str().expect("a path")]);
    assert_eq!(ran.status, 1, "{}{}", ran.out, ran.err);
    assert!(
        ran.out
            .contains(&format!("{}:2: font-size", conf.display())),
        "{}",
        ran.out
    );
    std::fs::write(&conf, "font-size=12\n").expect("a conf");
    let ran = f.run(&["check", conf.to_str().expect("a path")]);
    assert_eq!(ran.status, 0, "{}{}", ran.out, ran.err);
    assert_eq!(ran.out, format!("ok\t{}\n", conf.display()));
    assert!(!f.root.join("state/vtium").exists(), "check writes nothing");
}

#[test]
fn fonts_lists_each_font_its_state_and_the_sources_this_machine_lacks() {
    let f = fixture("fonts", None);
    let fonts = f.etc().join("fonts");
    std::fs::write(
        fonts.join("half.vtfrit"),
        "source=font:No Such Family Anywhere\nsource=builtin:unifont\n",
    )
    .expect("a frit");
    std::fs::write(
        fonts.join("none.vtfrit"),
        "source=font:No Such Family Anywhere\n",
    )
    .expect("a frit");
    std::fs::write(fonts.join("broken.vtfrit"), "colour=red\n").expect("a frit");
    let ran = f.run(&["--font-dir", "/nonexistent", "fonts"]);
    assert_eq!(ran.status, 0, "{}", ran.err);
    let lines: Vec<&str> = ran.out.lines().collect();
    assert_eq!(lines.len(), 7, "{}", ran.out);
    assert!(
        lines[0].starts_with("broken\tdoes not read: "),
        "{}",
        lines[0]
    );
    assert_eq!(lines[1], "half\treads\tfont:No Such Family Anywhere");
    assert!(
        lines[2].starts_with("none\tdraws nothing: "),
        "{}",
        lines[2]
    );
    assert_eq!(lines[3], "spare\treads\t");
    assert_eq!(lines[4], "tiny\treads\t");
    assert_eq!(lines[5], "unifont\tbuilt in\t");
    assert_eq!(lines[6], "unscii-8\tbuilt in\t");
}

/// A helper that fails as it starts, as when it cannot set
/// no-new-privileges, sends one failed frame and exits before it reads a
/// request. That failure is the helper's, never the first frit's: every
/// frit is unknown, with the helper's reason, and none draws nothing.
#[test]
fn a_helper_that_fails_at_start_marks_every_frit_unknown() {
    let f = fixture("fonts-start-fails", None);
    let helper = f.root.join("fails");
    // kind 3, a failure, of 4 bytes, then the end
    std::fs::write(
        &helper,
        "#!/bin/sh\nprintf '\\003\\000\\000\\000\\004\\000\\000\\000boom'\n",
    )
    .expect("the stand-in");
    std::fs::set_permissions(&helper, std::os::unix::fs::PermissionsExt::from_mode(0o755))
        .expect("its mode");
    let ran = f.run(&["--helper", helper.to_str().expect("a path"), "fonts"]);
    assert_eq!(ran.status, 0, "{}", ran.err);
    assert!(!ran.out.contains("draws nothing"), "{}", ran.out);
    let lines: Vec<&str> = ran.out.lines().collect();
    for (at, name) in [(0, "spare"), (1, "tiny")] {
        assert!(
            lines[at].starts_with(&format!("{name}\treads\tunknown: "))
                && lines[at].contains("boom"),
            "{}",
            ran.out
        );
    }
}

/// A helper that never answers, or answers one frit and then stalls,
/// marks no frit as drawing nothing: the answers that came are kept, and
/// each frit still unanswered after the helper's wait is unknown.
#[test]
fn fonts_keeps_the_answers_that_came_and_marks_the_rest_unknown() {
    let f = fixture("fonts-stalled", None);
    for (name, script, first) in [
        ("silent", "exec sleep 30", "spare\treads\tunknown: "),
        (
            // one sources answer, kind 11 with no payload, then nothing
            "one-then-stalls",
            "printf '\\013\\000\\000\\000\\000\\000\\000\\000'\nexec sleep 30",
            "spare\treads\t\n",
        ),
    ] {
        let helper = f.root.join(name);
        std::fs::write(&helper, format!("#!/bin/sh\n{script}\n")).expect("the stand-in");
        std::fs::set_permissions(&helper, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("its mode");
        let ran = f.run(&["--helper", helper.to_str().expect("a path"), "fonts"]);
        assert_eq!(ran.status, 0, "{name}: {}", ran.err);
        assert!(!ran.out.contains("draws nothing"), "{name}: {}", ran.out);
        let lines: Vec<&str> = ran.out.lines().collect();
        assert!(
            format!("{}\n", lines[0]).starts_with(first),
            "{name}: {}",
            ran.out
        );
        assert!(
            lines[1].starts_with("tiny\treads\tunknown: the helper did not answer in time"),
            "{name}: {}",
            ran.out
        );
    }
}
