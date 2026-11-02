//! `vtium console` against a scratch root and a stand-in systemctl.
//!
//! The stand-in, [`Fake`], does to the scratch root what systemd 262's
//! `systemctl --root` measured doing (ADR-1500's basis): it reads each
//! template's own `[Install]` section, links `Alias=` and `WantedBy=` for
//! an instance, refuses a masked unit, a missing unit file and an invalid
//! `DefaultInstance=` on a template, still links the wants when an alias is
//! taken, and leaves a VT's alias at `disable`. It panics on any call
//! vtium console never makes, such as `start` or `mask`, so a test fails
//! where vtium strays.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test's set-up fails loudly"
)]
#![expect(clippy::indexing_slicing, reason = "a test's set-up fails loudly")]

use std::collections::BTreeMap;
use std::os::unix::fs::symlink;
use std::path::Path;

use super::*;

/// systemd's `getty@.service` `[Install]` section, as Arch's systemd 262
/// ships it.
const GETTY_UNIT: &str = "[Unit]\nDescription=Getty on %I\n\n[Service]\nExecStart=-/sbin/agetty -o '-- \\\\u' --noreset --noclear - ${TERM}\n\n[Install]\nAlias=autovt@.service\n\nWantedBy=getty.target\nDefaultInstance=tty1\n";

/// The shipped unit, which the C reference's `kmsconvt@.service` is in
/// its `[Install]` section.
const KMSCON_UNIT: &str = include_str!("../../install/kmsconvt@.service");

/// A stand-in for `systemctl` on the running system whose root is `root`.
struct Fake {
    root: PathBuf,
    /// Each call, its arguments joined by spaces.
    calls: Vec<String>,
    /// The units running, by name.
    running: Vec<String>,
    /// A call that fails as if systemd refused it.
    fail: Option<String>,
}

fn ok() -> Reply {
    Reply {
        ok: true,
        ..Reply::default()
    }
}

fn refused(why: String) -> Reply {
    Reply {
        ok: false,
        stdout: String::new(),
        stderr: why,
    }
}

impl Fake {
    fn etc(&self) -> PathBuf {
        self.root.join(ETC)
    }

    /// The template file of `prefix@.service`, as a path on the system,
    /// and its `[Install]` lines by key.
    fn template(&self, prefix: &str) -> Option<(String, BTreeMap<String, Vec<String>>)> {
        let file = format!("{prefix}@.service");
        let dir = UNIT_DIRS
            .iter()
            .find(|d| self.root.join(d).join(&file).is_file())?;
        let text = std::fs::read_to_string(self.root.join(dir).join(&file)).unwrap();
        let mut install: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut inside = false;
        for line in text.lines() {
            if line.starts_with('[') {
                inside = line == "[Install]";
            } else if let Some((key, value)) = line.split_once('=').filter(|_| inside) {
                install
                    .entry(key.to_owned())
                    .or_default()
                    .push(value.to_owned());
            }
        }
        Some((format!("/{dir}/{file}"), install))
    }

    /// The unit `name`, which vtium console only ever names as an instance
    /// on a VT.
    fn instance<'n>(&self, name: &'n str) -> (&'n str, u16) {
        instance(name).unwrap_or_else(|| panic!("vtium console named {name}, no unit on a VT"))
    }

    fn link(&self, path: &Path, to: &str) -> Result<(), String> {
        match Link::read(path) {
            Link::Absent => {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                symlink(to, path).unwrap();
                Ok(())
            }
            Link::To(t) if t == to => Ok(()),
            Link::To(t) => Err(format!(
                "Failed to enable unit: File '{}' already exists and is a symlink to {t}\n",
                path.display()
            )),
            Link::Other => Err(format!(
                "Failed to enable unit: File '{}' already exists\n",
                path.display()
            )),
        }
    }

    fn enable(&mut self, name: &str) -> Reply {
        let (prefix, n) = self.instance(name);
        let Some((file, install)) = self.template(prefix) else {
            return refused(format!(
                "Failed to enable unit: Unit file {name} does not exist.\n"
            ));
        };
        if Link::read(&self.etc().join(name)) == Link::To("/dev/null".into()) {
            return refused(format!("Failed to enable unit: Unit {name} is masked\n"));
        }
        let mut errors = String::new();
        for alias in install.get("Alias").into_iter().flatten() {
            let alias = alias.replace("@.service", &format!("@tty{n}.service"));
            if let Err(e) = self.link(&self.etc().join(alias), &file) {
                errors.push_str(&e);
            }
        }
        for target in install.get("WantedBy").into_iter().flatten() {
            let wants = self.etc().join(format!("{target}.wants")).join(name);
            if let Err(e) = self.link(&wants, &file) {
                errors.push_str(&e);
            }
        }
        if errors.is_empty() {
            ok()
        } else {
            refused(errors)
        }
    }

    fn disable(&mut self, name: &str) -> Reply {
        let (prefix, _) = self.instance(name);
        let Some((file, install)) = self.template(prefix) else {
            return refused(format!(
                "Failed to disable unit: Unit file {name} does not exist.\n"
            ));
        };
        // An instance's alias stays (measured); its wants go.
        for target in install.get("WantedBy").into_iter().flatten() {
            let wants = self.etc().join(format!("{target}.wants")).join(name);
            if Link::read(&wants) == Link::To(file.clone()) {
                std::fs::remove_file(wants).unwrap();
            }
        }
        ok()
    }

    fn add_wants(&mut self, target: &str, name: &str) -> Reply {
        assert_eq!(
            target, "getty.target",
            "vtium console adds wants to getty.target alone"
        );
        let (prefix, _) = self.instance(name);
        let Some((file, _)) = self.template(prefix) else {
            return refused(format!(
                "Failed to add dependency: Unit file {name} does not exist.\n"
            ));
        };
        let wants = self.etc().join("getty.target.wants").join(name);
        match self.link(&wants, &file) {
            Ok(()) => ok(),
            Err(e) => refused(e),
        }
    }

    fn list_units(&self, patterns: &[&str]) -> Reply {
        let prefixes: Vec<&str> = patterns
            .iter()
            .map(|p| {
                p.strip_suffix("@*.service")
                    .unwrap_or_else(|| panic!("vtium console asked list-units for {p}"))
            })
            .collect();
        let mut stdout = String::new();
        for unit in &self.running {
            if instance(unit).is_some_and(|(p, _)| prefixes.contains(&p)) {
                stdout.push_str(&format!("{unit} loaded active running Some console\n"));
            }
        }
        Reply {
            ok: true,
            stdout,
            stderr: String::new(),
        }
    }
}

impl Systemctl for Fake {
    fn run(&mut self, args: &[&str]) -> io::Result<Reply> {
        let line = args.join(" ");
        self.calls.push(line.clone());
        if self.fail.as_deref() == Some(line.as_str()) {
            return Ok(refused("Failed: planted by the test\n".to_owned()));
        }
        Ok(match args {
            ["enable", name] => self.enable(name),
            ["disable", name] => self.disable(name),
            ["add-wants", target, name] => self.add_wants(target, name),
            ["daemon-reload"] => ok(),
            [
                "list-units",
                "--plain",
                "--no-legend",
                "--state=active",
                patterns @ ..,
            ] => self.list_units(patterns),
            _ => panic!("vtium console ran systemctl {line}, which it never runs"),
        })
    }
}

/// A scratch system: a root directory and the stand-in for its systemctl.
struct Machine {
    fake: Fake,
}

/// Makes `name@.service` in `dir` with the units' one generator,
/// `tools/vt/units.sh`, as `make install` does.
fn generate(name: &str, exec: &str, dir: &Path) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/vt/units.sh");
    let status = Command::new("bash")
        .arg(script)
        .args(["--name", name, "--exec", exec, "--by", "the console tests"])
        .args(["--conflicts", "kmsconvt@%i.service"])
        .arg(dir)
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "units.sh made no {name}@.service");
}

impl Machine {
    /// A host booted with systemd that has vtium's packaged unit, the
    /// development install's unit, systemd's getty and the C reference's
    /// console installed, and nothing enabled.
    fn new(name: &str) -> Machine {
        let root =
            std::env::temp_dir().join(format!("vtium-console-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lib = root.join("usr/lib/systemd/system");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::create_dir_all(root.join(BOOTED)).unwrap();
        std::fs::create_dir_all(root.join(WANTS)).unwrap();
        generate("vtium", "/usr/bin/vtium", &lib);
        generate("vtium-dev", "/usr/local/bin/vtium-dev", &lib);
        std::fs::write(lib.join("getty@.service"), GETTY_UNIT).unwrap();
        std::fs::write(lib.join("kmsconvt@.service"), KMSCON_UNIT).unwrap();
        Machine {
            fake: Fake {
                root,
                calls: Vec::new(),
                running: Vec::new(),
                fail: None,
            },
        }
    }

    /// As [`Machine::new`], with gettys enabled on VTs 1 to 6 and the
    /// template linked to the getty, as systemd's preset leaves a machine
    /// and as this machine has its gettys.
    fn gettys(name: &str) -> Machine {
        let m = Machine::new(name);
        for n in 1..=6 {
            m.link(&format!("getty.target.wants/getty@tty{n}.service"), "getty");
        }
        m.link(TEMPLATE, "getty");
        m
    }

    fn root(&self) -> &Path {
        &self.fake.root
    }

    /// Links `ETC/name` to `prefix@.service` in the packaged directory.
    fn link(&self, name: &str, prefix: &str) {
        let path = self.root().join(ETC).join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(format!("/usr/lib/systemd/system/{prefix}@.service"), path).unwrap();
    }

    /// Runs `vtium console WORDS` as root, or as another user.
    fn console_as(&mut self, root_user: bool, words: &[&str]) -> (i32, String, String) {
        let args: Vec<OsString> = std::iter::once("console")
            .chain(words.iter().copied())
            .map(OsString::from)
            .collect();
        let verb = match parse(&args) {
            Some(Ok(Parsed::Run(verb))) => verb,
            other => panic!("vtium console {words:?} parsed to {other:?}"),
        };
        let mut host = Host {
            root: self.fake.root.clone(),
            root_user,
            systemctl: &mut self.fake,
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = run(verb, &mut host, &mut out, &mut err);
        (
            status,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    fn console(&mut self, words: &[&str]) -> (i32, String, String) {
        self.console_as(true, words)
    }

    /// Every link and file under the root's `etc` and `var`, with what
    /// each links to or holds.
    fn snapshot(&self) -> BTreeMap<String, String> {
        fn walk(dir: &Path, base: &Path, into: &mut BTreeMap<String, String>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                let rel = path.strip_prefix(base).unwrap().display().to_string();
                match Link::read(&path) {
                    Link::To(to) => {
                        into.insert(rel, format!("-> {to}"));
                    }
                    _ if path.is_dir() => walk(&path, base, into),
                    _ => {
                        into.insert(rel, std::fs::read_to_string(&path).unwrap());
                    }
                }
            }
        }
        let mut into = BTreeMap::new();
        walk(&self.root().join("etc"), self.root(), &mut into);
        walk(&self.root().join("var"), self.root(), &mut into);
        into
    }

    fn links(&self, name: &str) -> Link {
        Link::read(&self.root().join(ETC).join(name))
    }

    fn wanted(&self, unit: &str) -> bool {
        self.root()
            .join(WANTS)
            .join(unit)
            .symlink_metadata()
            .is_ok()
    }

    /// The calls that change something, past the reads.
    fn changes(&self) -> Vec<&str> {
        self.fake
            .calls
            .iter()
            .map(String::as_str)
            .filter(|c| !c.starts_with("list-units"))
            .collect()
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.fake.root);
    }
}

fn to(prefix: &str) -> Link {
    Link::To(format!("/usr/lib/systemd/system/{prefix}@.service"))
}

fn words(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

#[test]
fn the_words_parse_to_a_verb_and_its_vts() {
    let parsed = |args: &[&str]| parse(&words(args));
    assert_eq!(parsed(&["check-config"]), None);
    assert_eq!(parsed(&[]), None);
    let run = |verb| Some(Ok(Parsed::Run(verb)));
    assert_eq!(parsed(&["console", "status"]), run(Verb::Status));
    assert_eq!(
        parsed(&["console", "enable", "3"]),
        run(Verb::Enable(Which::Vt(3)))
    );
    assert_eq!(
        parsed(&["console", "enable", "tty12"]),
        run(Verb::Enable(Which::Vt(12)))
    );
    assert_eq!(
        parsed(&["console", "enable", "--all"]),
        run(Verb::Enable(Which::All))
    );
    assert_eq!(
        parsed(&["console", "disable", "63"]),
        run(Verb::Disable(Which::Vt(63)))
    );
    assert_eq!(
        parsed(&["console", "disable", "--all"]),
        run(Verb::Disable(Which::All))
    );
    assert_eq!(parsed(&["console", "--help"]), Some(Ok(Parsed::Help)));
    assert_eq!(parsed(&["console", "enable", "-h"]), Some(Ok(Parsed::Help)));
}

#[test]
fn words_it_cannot_take_are_refused() {
    let refused = |args: &[&str]| matches!(parse(&words(args)), Some(Err(_)));
    // A bare disable never takes back every VT.
    assert!(refused(&["console", "disable"]));
    assert!(refused(&["console", "enable"]));
    assert!(refused(&["console"]));
    assert!(refused(&["console", "start", "3"]));
    for vt in ["0", "64", "03", "tty", "ttyx", "+3", "-3", "3a", "tty0"] {
        assert!(refused(&["console", "enable", vt]), "{vt}");
    }
    assert!(refused(&["console", "enable", "3", "4"]));
    assert!(refused(&["console", "enable", "--all", "3"]));
    assert!(refused(&["console", "status", "3"]));
    assert!(refused(&["console", "enable", "--now"]));
}

#[test]
fn a_host_without_systemd_changes_nothing_and_says_so() {
    let mut m = Machine::gettys("no-systemd");
    std::fs::remove_dir_all(m.root().join(BOOTED)).unwrap();
    let before = m.snapshot();
    for verb in [
        &["enable", "3"][..],
        &["enable", "--all"],
        &["disable", "3"],
        &["disable", "--all"],
        &["status"],
    ] {
        // Before the root check: no user is told to use sudo for nothing.
        let (status, out, err) = m.console_as(false, verb);
        assert_eq!(status, 1, "{verb:?}");
        assert!(out.is_empty(), "{verb:?}: {out}");
        assert!(err.contains("not booted with systemd"), "{verb:?}: {err}");
        assert!(err.contains("vtium --vt=ttyN --no-switchvt"), "{err}");
        assert!(err.contains("docs/systemd.md"), "{err}");
    }
    assert!(m.fake.calls.is_empty(), "{:?}", m.fake.calls);
    assert_eq!(m.snapshot(), before);
}

#[test]
fn enable_and_disable_need_root_and_say_how_without_sudo() {
    let mut m = Machine::gettys("not-root");
    let before = m.snapshot();
    for (verb, line) in [
        (&["enable", "3"][..], "sudo vtium console enable 3"),
        (&["enable", "--all"], "sudo vtium console enable --all"),
        (&["disable", "tty3"], "sudo vtium console disable 3"),
        (&["disable", "--all"], "sudo vtium console disable --all"),
    ] {
        let (status, out, err) = m.console_as(false, verb);
        assert_eq!(status, 1, "{verb:?}");
        assert!(out.is_empty(), "{out}");
        assert!(err.contains("needs root"), "{err}");
        assert!(err.contains(line), "{verb:?}: {err}");
    }
    assert!(m.fake.calls.is_empty(), "{:?}", m.fake.calls);
    assert_eq!(m.snapshot(), before);
    // status reads only what any user may.
    let (status, out, _) = m.console_as(false, &["status"]);
    assert_eq!(status, 0);
    assert!(out.contains("getty@tty3"), "{out}");
}

#[test]
fn without_the_packaged_unit_nothing_changes() {
    let mut m = Machine::gettys("not-installed");
    std::fs::remove_file(m.root().join("usr/lib/systemd/system/vtium@.service")).unwrap();
    for verb in [
        &["enable", "3"][..],
        &["enable", "--all"],
        &["disable", "3"],
        &["disable", "--all"],
    ] {
        let (status, _, err) = m.console(verb);
        assert_eq!(status, 1, "{verb:?}");
        assert!(err.contains("vtium@.service is not installed"), "{err}");
    }
    assert!(m.changes().is_empty());
    // status says so, and shows the VTs all the same.
    let (status, out, _) = m.console(&["status"]);
    assert_eq!(status, 0);
    assert!(out.starts_with("vtium@.service is not installed"), "{out}");
    assert!(out.contains("getty@tty3"), "{out}");
}

#[test]
fn enable_n_gives_the_vt_to_vtium_and_records_its_getty() {
    let mut m = Machine::gettys("enable-n");
    let (status, out, err) = m.console(&["enable", "3"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert!(m.wanted("vtium@tty3.service"));
    assert!(!m.wanted("getty@tty3.service"));
    assert_eq!(m.links("autovt@tty3.service"), to("vtium"));
    // The other VTs and the template are as they were.
    assert!(m.wanted("getty@tty2.service") && m.wanted("getty@tty4.service"));
    assert_eq!(m.links(TEMPLATE), to("getty"));
    assert!(m.root().join(RECORDS).join("tty3").is_file());
    // vtium is enabled before the getty goes, so VT 3 always has one.
    assert_eq!(
        m.changes(),
        ["enable vtium@tty3.service", "disable getty@tty3.service"]
    );
    // It starts nothing, and says how to start vtium now and what that ends.
    assert!(out.contains("systemctl start vtium@tty3.service"), "{out}");
    assert!(out.contains("ends every session on VT 3"), "{out}");
}

#[test]
fn disable_n_gives_back_exactly_what_enable_n_took() {
    let mut m = Machine::gettys("enable-disable-n");
    m.link("getty.target.wants/getty@tty9.service", "getty");
    let before = m.snapshot();
    for n in ["3", "9", "1"] {
        assert_eq!(m.console(&["enable", n]).0, 0);
    }
    assert_ne!(m.snapshot(), before);
    for n in ["3", "9", "1"] {
        let (status, out, err) = m.console(&["disable", n]);
        assert_eq!((status, err.as_str()), (0, ""), "{out}");
    }
    assert_eq!(m.snapshot(), before);
}

#[test]
fn enable_n_where_no_getty_is_enabled_records_nothing_and_disable_adds_none() {
    let mut m = Machine::new("no-getty");
    let before = m.snapshot();
    assert_eq!(m.console(&["enable", "4"]).0, 0);
    assert!(!m.root().join(RECORDS).join("tty4").exists());
    assert_eq!(m.changes(), ["enable vtium@tty4.service"]);
    assert_eq!(m.console(&["disable", "4"]).0, 0);
    let mut after = m.snapshot();
    after.remove("var/lib/vtium/console");
    assert_eq!(after, before);
}

#[test]
fn enable_n_twice_changes_nothing_the_second_time() {
    let mut m = Machine::gettys("twice");
    assert_eq!(m.console(&["enable", "3"]).0, 0);
    let calls = m.changes().len();
    let snapshot = m.snapshot();
    let (status, out, _) = m.console(&["enable", "3"]);
    assert_eq!(status, 0);
    assert!(out.contains("already"), "{out}");
    assert_eq!(m.changes().len(), calls);
    assert_eq!(m.snapshot(), snapshot);
}

#[test]
fn a_step_that_fails_leaves_a_console_enabled_and_the_same_verb_finishes() {
    let mut m = Machine::gettys("fails");
    let before = m.snapshot();
    m.fake.fail = Some("disable getty@tty3.service".into());
    let (status, _, err) = m.console(&["enable", "3"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("systemctl disable getty@tty3.service failed"),
        "{err}"
    );
    assert!(err.contains("planted by the test"), "{err}");
    // vtium is enabled and the getty still is: VT 3 is never left with
    // neither.
    assert!(m.wanted("vtium@tty3.service") && m.wanted("getty@tty3.service"));
    m.fake.fail = None;
    assert_eq!(m.console(&["enable", "3"]).0, 0);
    assert!(!m.wanted("getty@tty3.service"));
    // The record from the first run still says the getty was enabled.
    assert_eq!(m.console(&["disable", "3"]).0, 0);
    assert_eq!(m.snapshot(), before);
}

#[test]
fn enable_all_gives_every_vt_and_disable_all_gives_back_exactly_what_it_took() {
    // ADR-1500's observable: gettys on VTs 1 to 6 and the template linked
    // to the getty, there and back.
    let mut m = Machine::gettys("all");
    let before = m.snapshot();
    let (status, out, err) = m.console(&["enable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert_eq!(m.links(TEMPLATE), to("vtium"));
    // vtium at boot on VT 1 alone; logind starts it on the others.
    assert!(m.wanted("vtium@tty1.service"));
    for n in 1..=6 {
        assert!(!m.wanted(&format!("getty@tty{n}.service")), "{n}");
    }
    for n in 2..=6 {
        assert!(!m.wanted(&format!("vtium@tty{n}.service")), "{n}");
        assert!(m.root().join(RECORDS).join(format!("tty{n}")).is_file());
    }
    assert_eq!(
        m.fake.calls.last().map(String::as_str),
        Some("daemon-reload")
    );
    assert!(out.contains("systemctl start vtium@tty1.service"), "{out}");

    let (status, out, err) = m.console(&["disable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    let mut after = m.snapshot();
    assert_eq!(after.remove("var/lib/vtium/console"), None);
    assert_eq!(after, before);
}

#[test]
fn enable_all_where_no_template_linked_removes_it_again() {
    let mut m = Machine::new("all-no-template");
    m.link("getty.target.wants/getty@tty1.service", "getty");
    let before = m.snapshot();
    assert_eq!(m.console(&["enable", "--all"]).0, 0);
    assert_eq!(m.links(TEMPLATE), to("vtium"));
    assert_eq!(m.console(&["disable", "--all"]).0, 0);
    assert_eq!(m.links(TEMPLATE), Link::Absent);
    assert_eq!(m.snapshot(), before);
}

#[test]
fn enable_all_twice_changes_nothing_the_second_time() {
    let mut m = Machine::gettys("all-twice");
    assert_eq!(m.console(&["enable", "--all"]).0, 0);
    let snapshot = m.snapshot();
    let calls = m.changes().len();
    let (status, out, _) = m.console(&["enable", "--all"]);
    assert_eq!(status, 0);
    assert!(out.contains("already"), "{out}");
    assert_eq!(m.changes().len(), calls);
    assert_eq!(m.snapshot(), snapshot);
}

#[test]
fn enable_all_leaves_a_getty_above_loginds_vts_and_names_it() {
    let mut m = Machine::gettys("all-high");
    m.link("getty.target.wants/getty@tty8.service", "getty");
    let (status, out, _) = m.console(&["enable", "--all"]);
    assert_eq!(status, 0);
    assert!(m.wanted("getty@tty8.service"));
    assert!(out.contains("VT 8 keeps getty@tty8"), "{out}");
    assert!(out.contains("vtium console enable 8"), "{out}");
}

#[test]
fn disable_all_after_the_earlier_hand_typed_steps_links_the_template_to_the_getty() {
    // ADR-1500 D3's earlier steps: the template linked by hand, VT 1's
    // getty disabled and vtium enabled there, and no record.
    let mut m = Machine::new("hand-typed");
    m.link(TEMPLATE, "vtium");
    m.link("getty.target.wants/vtium@tty1.service", "vtium");
    m.link("autovt@tty1.service", "vtium");
    let (status, out, err) = m.console(&["disable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert_eq!(m.links(TEMPLATE), to("getty"));
    assert!(m.wanted("getty@tty1.service"));
    assert!(!m.wanted("vtium@tty1.service"));
    assert_eq!(m.links("autovt@tty1.service"), Link::Absent);
}

#[test]
fn a_template_record_that_is_no_unit_path_counts_as_none() {
    let mut m = Machine::new("bad-record");
    m.link(TEMPLATE, "vtium");
    std::fs::create_dir_all(m.root().join(RECORDS)).unwrap();
    std::fs::write(m.root().join(RECORDS).join(TEMPLATE), "/etc/shadow\n").unwrap();
    assert_eq!(m.console(&["disable", "--all"]).0, 0);
    assert_eq!(m.links(TEMPLATE), to("getty"));
}

#[test]
fn disable_n_under_the_vtium_template_says_logind_still_starts_vtium() {
    let mut m = Machine::new("template-still");
    assert_eq!(m.console(&["enable", "--all"]).0, 0);
    assert_eq!(m.console(&["enable", "3"]).0, 0);
    let (status, _, err) = m.console(&["disable", "3"]);
    assert_eq!(status, 1);
    assert!(err.contains("logind still starts vtium on VT 3"), "{err}");
    assert!(err.contains("vtium console disable --all"), "{err}");
    assert!(!m.wanted("vtium@tty3.service"));
    // VT 1 gets its getty back, record or none, and holds the VT from boot.
    let (status, _, err) = m.console(&["disable", "1"]);
    assert_eq!((status, err.as_str()), (0, ""));
    assert!(m.wanted("getty@tty1.service"));
}

#[test]
fn disable_n_on_a_vt_vtium_does_not_hold_changes_nothing() {
    let mut m = Machine::gettys("not-given");
    let before = m.snapshot();
    let (status, out, _) = m.console(&["disable", "5"]);
    assert_eq!(status, 0);
    assert!(out.contains("not enabled on VT 5"), "{out}");
    assert!(m.changes().is_empty());
    assert_eq!(m.snapshot(), before);
}

#[test]
fn enable_refuses_a_vt_the_c_references_console_holds() {
    // Enabled there, linked there, or running there.
    let mut m = Machine::gettys("kmscon-vt");
    m.link("getty.target.wants/kmsconvt@tty2.service", "kmsconvt");
    m.link("autovt@tty3.service", "kmsconvt");
    m.fake.running.push("kmsconvt@tty4.service".into());
    let before = m.snapshot();
    for (n, link) in [("2", false), ("3", true), ("4", false)] {
        let (status, _, err) = m.console(&["enable", n]);
        assert_eq!(status, 1, "{n}");
        assert!(
            err.contains(&format!("kmsconvt@tty{n}.service, holds VT {n}")),
            "{err}"
        );
        assert!(
            err.contains(&format!("systemctl disable --now kmsconvt@tty{n}.service")),
            "{err}"
        );
        assert_eq!(
            err.contains(&format!("rm /etc/systemd/system/autovt@tty{n}.service")),
            link
        );
    }
    let (status, _, err) = m.console(&["enable", "--all"]);
    assert_eq!(status, 1);
    assert!(err.contains("holds VT 2, 3, 4"), "{err}");
    assert!(m.changes().is_empty(), "{:?}", m.changes());
    assert_eq!(m.snapshot(), before);
}

#[test]
fn enable_all_refuses_the_c_references_template_and_enable_n_works_beside_it() {
    // This machine: autovt@.service links to kmsconvt@.service.
    let mut m = Machine::gettys("kmscon-template");
    std::fs::remove_file(m.root().join(ETC).join(TEMPLATE)).unwrap();
    m.link(TEMPLATE, "kmsconvt");
    let before = m.snapshot();
    let (status, _, err) = m.console(&["enable", "--all"]);
    assert_eq!(status, 1);
    assert!(err.contains("links to kmsconvt@.service"), "{err}");
    assert!(err.contains("systemctl disable kmsconvt@.service"), "{err}");
    assert!(m.changes().is_empty());
    assert_eq!(m.snapshot(), before);
    // VT 5's own link wins over the template, as D3's one-VT steps did.
    assert_eq!(m.console(&["enable", "5"]).0, 0);
    assert_eq!(m.links("autovt@tty5.service"), to("vtium"));
    assert_eq!(m.console(&["disable", "5"]).0, 0);
    assert_eq!(m.snapshot(), before);
}

#[test]
fn enable_refuses_a_vt_of_the_development_install_and_enable_all_passes_it_by() {
    let mut m = Machine::gettys("dev");
    // make vt-install VT=3: vtium-dev enabled, the getty disabled.
    std::fs::remove_file(m.root().join(WANTS).join("getty@tty3.service")).unwrap();
    m.link("getty.target.wants/vtium-dev@tty3.service", "vtium-dev");
    m.link("autovt@tty3.service", "vtium-dev");
    let (status, _, err) = m.console(&["enable", "3"]);
    assert_eq!(status, 1);
    assert!(err.contains("make vt-uninstall VT=3"), "{err}");
    assert!(m.changes().is_empty());
    let (status, out, _) = m.console(&["enable", "--all"]);
    assert_eq!(status, 0);
    assert!(out.contains("VT 3 keeps the development install"), "{out}");
    assert_eq!(m.links("autovt@tty3.service"), to("vtium-dev"));
    assert!(m.wanted("vtium-dev@tty3.service"));
}

#[test]
fn enable_refuses_a_masked_unit_and_a_vt_whose_own_link_names_another_unit() {
    let mut m = Machine::gettys("masked");
    symlink("/dev/null", m.root().join(ETC).join("vtium@tty2.service")).unwrap();
    // What systemctl enable getty@tty4.service leaves, which disable keeps.
    m.link("autovt@tty4.service", "getty");
    let before = m.snapshot();
    let (status, _, err) = m.console(&["enable", "2"]);
    assert_eq!(status, 1);
    assert!(err.contains("systemctl unmask vtium@tty2.service"), "{err}");
    let (status, _, err) = m.console(&["enable", "4"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("autovt@tty4.service links to /usr/lib/systemd/system/getty@.service"),
        "{err}"
    );
    assert!(m.changes().is_empty(), "{:?}", m.changes());
    assert_eq!(m.snapshot(), before);
}

#[test]
fn the_stand_in_refuses_what_systemctl_refuses() {
    // The stand-in fails where systemd 262 was measured failing, so the
    // refusals above stand for the real ones.
    let mut m = Machine::new("fake");
    symlink("/dev/null", m.root().join(ETC).join("vtium@tty2.service")).unwrap();
    assert!(!m.fake.enable("vtium@tty2.service").ok);
    assert!(!m.root().join(WANTS).join("vtium@tty2.service").exists());
    m.link("autovt@tty5.service", "kmsconvt");
    assert!(!m.fake.enable("vtium@tty5.service").ok);
    // The wants are linked all the same (measured).
    assert!(m.wanted("vtium@tty5.service"));
    assert!(!m.fake.enable("nothing@tty3.service").ok);
    assert!(m.fake.enable("vtium@tty3.service").ok);
    assert!(m.fake.disable("vtium@tty3.service").ok);
    // disable leaves the alias (measured).
    assert_eq!(m.links("autovt@tty3.service"), to("vtium"));
    assert!(!m.wanted("vtium@tty3.service"));
}

#[test]
fn status_shows_what_each_vt_starts_and_runs() {
    let mut m = Machine::gettys("status");
    assert_eq!(m.console(&["enable", "3"]).0, 0);
    m.link("getty.target.wants/getty@tty9.service", "getty");
    m.fake.running = vec!["getty@tty2.service".into(), "vtium@tty3.service".into()];
    let (status, out, _) = m.console_as(false, &["status"]);
    assert_eq!(status, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines[0],
        "On a VT with no link of its own, logind starts a getty: /etc/systemd/system/autovt@.service links to getty@.service."
    );
    let row = |n: &str| {
        lines
            .iter()
            .find(|l| l.split_whitespace().next() == Some(n))
            .map(|l| {
                l.split("  ")
                    .filter(|c| !c.is_empty())
                    .map(str::trim)
                    .collect::<Vec<_>>()
            })
            .unwrap()
    };
    assert_eq!(row("VT"), ["VT", "at boot", "when switched to", "running"]);
    assert_eq!(row("2"), ["2", "getty@tty2", "getty@tty2", "getty@tty2"]);
    assert_eq!(
        row("3"),
        [
            "3",
            "vtium@tty3, getty recorded",
            "vtium@tty3",
            "vtium@tty3"
        ]
    );
    assert_eq!(row("6"), ["6", "getty@tty6", "getty@tty6", "-"]);
    assert_eq!(row("9"), ["9", "getty@tty9", "getty@tty9", "-"]);
    // VTs 7 and 8 hold nothing and are past logind's six.
    assert!(
        !lines
            .iter()
            .any(|l| l.starts_with("7 ") || l.starts_with("8 "))
    );
}

#[test]
fn status_names_a_missing_template() {
    let mut m = Machine::new("status-none");
    let (status, out, _) = m.console(&["status"]);
    assert_eq!(status, 0);
    assert!(
        out.starts_with("On a VT with no link of its own, logind starts nothing"),
        "{out}"
    );
    assert!(
        out.lines()
            .any(|l| l.split_whitespace().collect::<Vec<_>>() == ["1", "-", "-", "-"]),
        "{out}"
    );
}
