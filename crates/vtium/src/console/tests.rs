//! `vtium console` against a scratch root and a stand-in systemctl.
//!
//! The stand-in, [`Fake`], does to the scratch root what systemd 262's
//! `systemctl --root` measured doing (ADR-1500's basis): it reads each
//! template's own `[Install]` section, found along the unit search path,
//! and links `Alias=` and `WantedBy=` for an instance. It refuses a masked
//! instance, a masked template and a missing unit file, still links the
//! wants when an alias is taken, and leaves a VT's alias at `disable`. Its
//! `systemctl show` answers from a table each test writes out literally, as
//! systemd 262 printed it (measured), and prints for a unit missing from the
//! table what systemd prints for a unit it does not know. It panics on any
//! call vtium console never makes, such as `mask` or an enable of a
//! template, and on a `start` outside `fallback`, so a test fails where
//! vtium strays.

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

/// The shipped unit, `vtium@.service`, whose `[Install]` section is the
/// C reference's `kmsconvt@.service`'s, written below under that name.
const KMSCON_UNIT: &str = include_str!("../../install/vtium@.service");

/// A stand-in for `systemctl` and `loginctl` on the running system whose
/// root is `root`.
struct Fake {
    root: PathBuf,
    /// Each call, its arguments joined by spaces.
    calls: Vec<String>,
    /// The units running, by name.
    running: Vec<String>,
    /// logind's sessions, as `loginctl show-session` prints each one's
    /// properties: Name, VTNr, Service, Type and Class.
    sessions: Vec<[&'static str; 5]>,
    /// Whether loginctl fails, as with no logind on the bus.
    logind_down: bool,
    /// A call that fails as if systemd refused it.
    fail: Option<String>,
    /// What `systemctl show` prints of each unit, by unit and property.
    shown: BTreeMap<String, BTreeMap<String, String>>,
    /// Whether `systemctl show`, asked about several units, leaves out the
    /// last one's block, as an answer cut short.
    short: bool,
    /// Whether the verb run is `fallback`, the one verb that starts a unit.
    starts: bool,
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
        // An instance masked, or its template masked (measured).
        let template = format!("{prefix}@.service");
        for masked in [name, template.as_str()] {
            if Link::read(&self.etc().join(masked)).masks() {
                return refused(format!(
                    "Failed to enable unit: Unit {} is masked\n",
                    self.etc().join(masked).display()
                ));
            }
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
        let Some((_, install)) = self.template(prefix) else {
            return refused(format!(
                "Failed to disable unit: Unit file {name} does not exist.\n"
            ));
        };
        // An instance's alias stays (measured); its wants go by their
        // name, whichever unit file they link to (measured).
        for target in install.get("WantedBy").into_iter().flatten() {
            let wants = self.etc().join(format!("{target}.wants")).join(name);
            if matches!(Link::read(&wants), Link::To(_)) {
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
        // A missing target is refused (measured).
        if !UNIT_DIRS
            .iter()
            .any(|d| self.root.join(d).join(target).is_file())
        {
            return refused(format!(
                "Failed to add dependency on unit: Unit {target} does not exist\n"
            ));
        }
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
        assert_eq!(
            patterns,
            ["*@tty*.service"],
            "vtium console asks list-units for every unit on a VT"
        );
        let mut stdout = String::new();
        for unit in &self.running {
            // The glob matches a non-VT instance too, such as ttyS0's.
            let on_tty = unit
                .split_once('@')
                .is_some_and(|(_, i)| i.starts_with("tty"));
            if on_tty {
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

impl Fake {
    /// `systemctl show -p PROP... -- UNIT...` from the test's table: a
    /// block of `PROP=value` lines a unit, in the order asked, the blocks
    /// parted by an empty line. A unit the table lacks reads as systemd
    /// prints a unit it cannot find: its Id, inactive, and the rest empty.
    fn show(&self, args: &[&str]) -> Reply {
        let dash = args
            .iter()
            .position(|a| *a == "--")
            .unwrap_or_else(|| panic!("systemctl show {args:?} puts no -- before its units"));
        let (opts, units) = args.split_at(dash);
        let props: Vec<&str> = opts
            .chunks(2)
            .map(|pair| match pair {
                ["-p", prop] => *prop,
                other => panic!("systemctl show takes -p PROP, not {other:?}"),
            })
            .collect();
        let mut blocks: Vec<String> = units
            .get(1..)
            .unwrap()
            .iter()
            .map(|unit| {
                let row = self.shown.get(*unit);
                props
                    .iter()
                    .map(|prop| {
                        let value = match (row.and_then(|r| r.get(*prop)), *prop) {
                            (Some(v), _) => v.clone(),
                            (None, "Id") => (*unit).to_owned(),
                            (None, "ActiveState") => "inactive".to_owned(),
                            (None, _) => String::new(),
                        };
                        format!("{prop}={value}\n")
                    })
                    .collect()
            })
            .collect();
        if self.short && blocks.len() > 1 {
            blocks.pop();
        }
        Reply {
            ok: true,
            stdout: blocks.join("\n"),
            stderr: String::new(),
        }
    }

    /// Adds `unit` to `systemctl show`'s table with these properties.
    fn shows(&mut self, unit: &str, props: &[(&str, &str)]) {
        let row = self.shown.entry(unit.to_owned()).or_default();
        for (k, v) in props {
            row.insert((*k).to_owned(), (*v).to_owned());
        }
    }
}

impl Systemd for Fake {
    fn loginctl(&mut self, args: &[&str]) -> io::Result<Reply> {
        self.calls.push(format!("loginctl {}", args.join(" ")));
        if self.logind_down {
            return Ok(refused(
                "Failed to connect to system scope bus via local transport: No such file or directory\n"
                    .to_owned(),
            ));
        }
        let id = |i: usize| format!("c{i}");
        Ok(match args {
            ["list-sessions", "--no-legend"] => Reply {
                ok: true,
                stdout: (0..self.sessions.len())
                    .map(|i| format!("{} 1000 someone seat0 100 user tty1 no -\n", id(i)))
                    .collect(),
                stderr: String::new(),
            },
            ["show-session", rest @ ..] => {
                let n = self.sessions.len();
                let ids: Vec<String> = (0..n).map(id).collect();
                let props = [
                    "-p", "Name", "-p", "VTNr", "-p", "Service", "-p", "Type", "-p", "Class",
                ];
                assert_eq!(
                    rest.get(..n).unwrap(),
                    ids,
                    "show-session asks for every session listed"
                );
                assert_eq!(
                    rest.get(n..).unwrap(),
                    props,
                    "show-session asks for the properties vtium reads"
                );
                // systemd prints the properties in its own order, not the
                // order asked.
                let blocks: Vec<String> = self
                    .sessions
                    .iter()
                    .enumerate()
                    .map(|(i, [name, vt, service, kind, class])| {
                        format!("Id={}\nName={name}\nVTNr={vt}\nService={service}\nType={kind}\nClass={class}\n", id(i))
                    })
                    .collect();
                Reply {
                    ok: true,
                    stdout: blocks.join("\n"),
                    stderr: String::new(),
                }
            }
            _ => panic!(
                "vtium console ran loginctl {}, which it never runs",
                args.join(" ")
            ),
        })
    }

    fn systemctl(&mut self, args: &[&str]) -> io::Result<Reply> {
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
            ["show", rest @ ..] => self.show(rest),
            ["--no-block", "--job-mode=fail", "start", name] if self.starts => {
                self.instance(name);
                ok()
            }
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
        std::fs::write(
            lib.join("getty.target"),
            "[Unit]\nDescription=Login Prompts\n",
        )
        .unwrap();
        Machine {
            fake: Fake {
                root,
                calls: Vec::new(),
                running: Vec::new(),
                sessions: Vec::new(),
                logind_down: false,
                fail: None,
                shown: BTreeMap::new(),
                short: false,
                starts: false,
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
        self.fake.starts = matches!(verb, Verb::Fallback(_));
        let mut host = Host {
            root: self.fake.root.clone(),
            root_user,
            systemd: &mut self.fake,
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
            .filter(|c| !c.starts_with("list-units") && !c.starts_with("loginctl"))
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
        assert!(err.contains("vtium@.service is in none of"), "{err}");
    }
    assert!(m.changes().is_empty());
    // status says so, and shows the VTs all the same.
    let (status, out, _) = m.console(&["status"]);
    assert_eq!(status, 0);
    assert!(out.starts_with("vtium@.service is in none of"), "{out}");
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
fn enable_n_where_no_getty_is_enabled_records_that_and_disable_adds_none() {
    let mut m = Machine::new("no-getty");
    let before = m.snapshot();
    assert_eq!(m.console(&["enable", "4"]).0, 0);
    let record = m.root().join(RECORDS).join("tty4");
    assert_eq!(std::fs::read_to_string(record).unwrap(), "");
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
    // It only reloads, in case the first run's reload failed.
    assert_eq!(m.changes().get(calls..).unwrap(), ["daemon-reload"]);
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
    // VT 1 had no getty, its record says so, and it gets none back.
    let (status, _, err) = m.console(&["disable", "1"]);
    assert_eq!(status, 1, "{err}");
    assert!(!m.wanted("getty@tty1.service"));
}

#[test]
fn disable_n_on_a_vt_vtium_does_not_hold_changes_nothing() {
    let mut m = Machine::gettys("not-given");
    let before = m.snapshot();
    let (status, out, _) = m.console(&["disable", "5"]);
    assert_eq!(status, 0);
    assert!(out.contains("not enabled on VT 5"), "{out}");
    // It only reloads, in case an earlier run's reload failed.
    assert_eq!(m.changes(), ["daemon-reload"]);
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
    assert!(m.changes().is_empty(), "{:?}", m.changes());
    assert_eq!(m.snapshot(), before);
    // enable --all leaves each of them to kmscon, getty and link included,
    // and takes the others.
    let (status, out, err) = m.console(&["enable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    for n in 2..=4 {
        assert!(
            out.contains(&format!("VT {n} keeps the C reference's console")),
            "{out}"
        );
        assert!(m.wanted(&format!("getty@tty{n}.service")), "{n}");
        assert!(
            !m.root().join(RECORDS).join(format!("tty{n}")).exists(),
            "{n}"
        );
    }
    assert_eq!(m.links("autovt@tty3.service"), to("kmsconvt"));
    assert!(!m.wanted("getty@tty5.service") && m.wanted("vtium@tty1.service"));
    assert_eq!(m.console(&["disable", "--all"]).0, 0);
    assert_eq!(m.snapshot(), before);
}

#[test]
fn kmscon_on_one_vt_beside_vtiums_template_keeps_it() {
    // The README's steps: vtium holds the template, VT 3's getty is
    // disabled, and a hand-made autovt@tty3.service links to kmscon, which
    // systemctl is-enabled reads as disabled (measured). vtium reads the
    // link itself.
    let mut m = Machine::new("kmscon-beside");
    assert_eq!(m.console(&["enable", "--all"]).0, 0);
    m.link("autovt@tty3.service", "kmsconvt");
    let (status, _, err) = m.console(&["enable", "3"]);
    assert_eq!(status, 1);
    assert!(err.contains("kmsconvt@tty3.service, holds VT 3"), "{err}");
    let (status, out, _) = m.console(&["enable", "--all"]);
    assert_eq!(status, 0, "{out}");
    assert!(
        out.contains("VT 3 keeps the C reference's console"),
        "{out}"
    );
    let (_, out, _) = m.console(&["status"]);
    assert!(
        out.lines()
            .any(|l| l.split_whitespace().collect::<Vec<_>>() == ["3", "-", "kmsconvt@tty3", "-"]),
        "{out}"
    );
    // disable --all leaves kmscon's VT as it is.
    assert_eq!(m.console(&["disable", "--all"]).0, 0);
    assert_eq!(m.links("autovt@tty3.service"), to("kmsconvt"));
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
    // add-wants refuses a target that does not exist (measured).
    assert!(m.fake.add_wants("getty.target", "getty@tty1.service").ok);
    std::fs::remove_file(m.root().join(WANTS).join("getty@tty1.service")).unwrap();
    std::fs::remove_file(m.root().join("usr/lib/systemd/system/getty.target")).unwrap();
    let reply = m.fake.add_wants("getty.target", "getty@tty1.service");
    assert!(!reply.ok);
    assert!(reply.stderr.contains("Unit getty.target does not exist"));
    assert!(!m.wanted("getty@tty1.service"));
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
        "On a VT with no link of its own, logind starts a getty: autovt@.service links to getty@.service."
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
    // logind starts no console by itself on a VT past its six.
    assert_eq!(row("9"), ["9", "getty@tty9", "-", "-"]);
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

#[test]
fn a_unit_name_with_tty_twice_names_no_vt() {
    assert_eq!(instance("getty@tty3.service"), Some(("getty", 3)));
    assert_eq!(instance("getty@ttytty3.service"), None);
    assert_eq!(instance("getty@3.service"), None);
    assert!(matches!(
        parse(&words(&["console", "enable", "ttytty3"])),
        Some(Err(_))
    ));
}

/// Links `/etc/systemd/system/display-manager.service` to `sddm.service`,
/// whose unit is installed, as an enabled display manager is.
fn display_manager(m: &Machine) {
    let lib = m.root().join("usr/lib/systemd/system");
    std::fs::write(
        lib.join("sddm.service"),
        "[Unit]\nConflicts=getty@tty1.service\nAfter=getty@tty1.service\n",
    )
    .unwrap();
    symlink(
        "/usr/lib/systemd/system/sddm.service",
        m.root().join(ETC).join(DISPLAY_MANAGER),
    )
    .unwrap();
}

#[test]
fn an_enabled_display_manager_keeps_vt_1() {
    let mut m = Machine::gettys("display-manager");
    display_manager(&m);
    let before = m.snapshot();
    let (status, _, err) = m.console(&["enable", "1"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("the display manager sddm.service holds VT 1"),
        "{err}"
    );
    assert!(m.changes().is_empty(), "{:?}", m.changes());
    let (status, out, err) = m.console(&["enable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    // VT 1 is the display manager's, and its getty and record are left be.
    assert!(!m.wanted("vtium@tty1.service"));
    assert!(m.wanted("getty@tty1.service"));
    assert!(!m.root().join(RECORDS).join("tty1").exists());
    assert!(
        out.contains("VT 1 is left as it is: the display manager sddm.service holds VT 1 at boot: a display manager is enabled, and by common convention VT 1 stays the display manager's and its getty's"),
        "{out}"
    );
    assert!(!out.contains("systemctl start vtium@tty1.service"), "{out}");
    // The other VTs are vtium's.
    assert_eq!(m.links(TEMPLATE), to("vtium"));
    assert!(!m.wanted("getty@tty2.service"));
    let (_, out, _) = m.console(&["status"]);
    assert!(out.contains("The display manager is sddm.service"), "{out}");
    assert!(
        out.lines()
            .any(|l| l.starts_with("1 ") && l.contains("display manager (sddm.service)")),
        "{out}"
    );
    assert!(
        out.contains("VT 1: the display manager sddm.service holds VT 1 at boot: a display manager is enabled, and by common convention"),
        "{out}"
    );
    assert_eq!(m.console(&["disable", "--all"]).0, 0);
    assert_eq!(m.snapshot(), before);
}

#[test]
fn a_display_managers_drop_in_adds_a_vt_it_claims() {
    // A drop-in in /etc claims VT 2 beside the vendor unit's VT 1. Its empty
    // Conflicts= clears nothing: systemd cannot reset a dependency
    // (measured).
    let mut m = Machine::gettys("dm-drop-in");
    display_manager(&m);
    let dir = m.root().join(ETC).join("sddm.service.d");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("vt.conf"),
        "[Unit]\nConflicts=\nConflicts=getty@tty2.service\n",
    )
    .unwrap();
    let (status, _, err) = m.console(&["enable", "2"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("the display manager sddm.service holds VT 2"),
        "{err}"
    );
    let (status, _, err) = m.console(&["enable", "1"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("the display manager sddm.service holds VT 1"),
        "{err}"
    );
    let (status, out, _) = m.console(&["enable", "--all"]);
    assert_eq!(status, 0, "{out}");
    assert!(m.wanted("getty@tty2.service"));
    assert!(m.wanted("getty@tty1.service"));
    assert!(!m.wanted("vtium@tty1.service"));
}

#[test]
fn a_graphical_session_keeps_its_vt() {
    // This machine: a Wayland session through sddm on VT 7, and one on VT
    // 3, inside logind's VTs, where getty@tty3 is enabled too.
    let mut m = Machine::gettys("session");
    m.fake.sessions = vec![
        ["aaron", "0", "systemd-user", "unspecified", "manager"],
        ["aaron", "7", "sddm", "wayland", "user"],
        ["bob", "3", "gdm-password", "x11", "user"],
        ["carol", "4", "login", "tty", "user"],
    ];
    for n in ["7", "3"] {
        let (status, _, err) = m.console(&["enable", n]);
        assert_eq!(status, 1, "{n}");
        assert!(err.contains("graphical session of"), "{err}");
    }
    let (_, _, err) = m.console(&["enable", "3"]);
    assert!(
        err.contains("the graphical session of bob (gdm-password, x11) holds VT 3"),
        "{err}"
    );
    // A text login is no graphical session.
    assert_eq!(m.console(&["enable", "4"]).0, 0);
    assert_eq!(m.console(&["disable", "4"]).0, 0);
    let before = m.snapshot();
    let (status, out, _) = m.console(&["enable", "--all"]);
    assert_eq!(status, 0, "{out}");
    assert!(m.wanted("getty@tty3.service"));
    assert!(!m.root().join(RECORDS).join("tty3").exists());
    assert!(
        out.contains("VT 3 is left as it is: the graphical session of bob"),
        "{out}"
    );
    assert!(
        out.contains("VT 7 is left as it is: the graphical session of aaron (sddm, wayland)"),
        "{out}"
    );
    assert!(!m.wanted("getty@tty5.service"));
    let (_, out, _) = m.console(&["status"]);
    assert!(
        out.lines()
            .any(|l| l.starts_with("7 ")
                && l.ends_with("graphical session of aaron (sddm, wayland)")),
        "{out}"
    );
    assert_eq!(m.console(&["disable", "--all"]).0, 0);
    assert_eq!(m.snapshot(), before);
}

#[test]
fn a_greeter_keeps_its_vt() {
    let mut m = Machine::gettys("greeter");
    m.fake.sessions = vec![["sddm", "1", "sddm-greeter", "x11", "greeter"]];
    let (status, _, err) = m.console(&["enable", "1"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("the greeter of sddm (sddm-greeter) holds VT 1"),
        "{err}"
    );
    assert_eq!(m.console(&["enable", "--all"]).0, 0);
    assert!(!m.wanted("vtium@tty1.service") && m.wanted("getty@tty1.service"));
}

#[test]
fn enable_all_refuses_graphics_on_loginds_reserved_vt() {
    // logind starts autovt@tty6 on every switch to VT 6, busy or not.
    let mut m = Machine::gettys("reserved");
    m.fake.sessions = vec![["aaron", "6", "sddm", "wayland", "user"]];
    let before = m.snapshot();
    let (status, _, err) = m.console(&["enable", "--all"]);
    assert_eq!(status, 1);
    assert!(err.contains("logind's reserved VT"), "{err}");
    assert!(m.changes().is_empty(), "{:?}", m.changes());
    assert_eq!(m.snapshot(), before);
}

#[test]
fn without_logind_enable_refuses_and_disable_works() {
    let mut m = Machine::gettys("no-logind");
    assert_eq!(m.console(&["enable", "3"]).0, 0);
    m.fake.logind_down = true;
    for verb in [&["enable", "4"][..], &["enable", "--all"]] {
        let (status, _, err) = m.console(verb);
        assert_eq!(status, 1, "{verb:?}");
        assert!(err.contains("cannot ask logind"), "{err}");
    }
    assert_eq!(m.console(&["disable", "3"]).0, 0);
    let (status, out, _) = m.console(&["status"]);
    assert_eq!(status, 0);
    assert!(out.contains("cannot ask logind"), "{out}");
}

#[test]
fn vt_1_with_no_getty_gets_none_back() {
    let mut m = Machine::new("vt1-no-getty");
    for n in 2..=6 {
        m.link(&format!("getty.target.wants/getty@tty{n}.service"), "getty");
    }
    m.link(TEMPLATE, "getty");
    let before = m.snapshot();
    assert_eq!(m.console(&["enable", "--all"]).0, 0);
    assert_eq!(
        std::fs::read_to_string(m.root().join(RECORDS).join("tty1")).unwrap(),
        ""
    );
    assert_eq!(m.console(&["disable", "--all"]).0, 0);
    assert!(!m.wanted("getty@tty1.service"));
    assert_eq!(m.snapshot(), before);
}

#[test]
fn enable_all_refuses_a_template_file_of_the_administrators() {
    let mut m = Machine::gettys("template-file");
    std::fs::remove_file(m.root().join(ETC).join(TEMPLATE)).unwrap();
    std::fs::write(
        m.root().join(ETC).join(TEMPLATE),
        "[Service]\nExecStart=/bin/mine\n",
    )
    .unwrap();
    let before = m.snapshot();
    let (status, _, err) = m.console(&["enable", "--all"]);
    assert_eq!(status, 1);
    assert!(err.contains("is a file, not a link"), "{err}");
    assert!(m.changes().is_empty(), "{:?}", m.changes());
    assert_eq!(m.snapshot(), before);
}

#[test]
fn enable_refuses_a_vts_own_unit_file() {
    let mut m = Machine::gettys("own-file");
    std::fs::write(
        m.root().join(ETC).join("autovt@tty4.service"),
        "[Service]\n",
    )
    .unwrap();
    let before = m.snapshot();
    let (status, _, err) = m.console(&["enable", "4"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("autovt@tty4.service is a file, not a link"),
        "{err}"
    );
    assert!(m.changes().is_empty(), "{:?}", m.changes());
    assert_eq!(m.snapshot(), before);
}

#[test]
fn a_masked_template_is_refused_before_anything_is_written() {
    let mut m = Machine::gettys("template-masked");
    symlink(MASK, m.root().join(ETC).join("vtium@.service")).unwrap();
    // The stand-in refuses as systemctl does (measured).
    assert!(!m.fake.enable("vtium@tty3.service").ok);
    assert!(!m.wanted("vtium@tty3.service"));
    let before = m.snapshot();
    for verb in [&["enable", "3"][..], &["enable", "--all"]] {
        let (status, _, err) = m.console(verb);
        assert_eq!(status, 1, "{verb:?}");
        assert!(err.contains("systemctl unmask vtium@.service"), "{err}");
    }
    assert!(m.changes().is_empty(), "{:?}", m.changes());
    assert_eq!(m.snapshot(), before);
    let (status, out, _) = m.console(&["status"]);
    assert_eq!(status, 0);
    assert!(out.starts_with("vtium@.service is masked"), "{out}");
}

#[test]
fn a_gettys_want_comes_back_linked_where_it_linked() {
    // An override in /etc, which systemctl add-wants would link to, while
    // the want links to the vendor file.
    let mut m = Machine::gettys("getty-target");
    std::fs::write(m.root().join(ETC).join("getty@.service"), GETTY_UNIT).unwrap();
    let before = m.snapshot();
    assert_eq!(m.console(&["enable", "3"]).0, 0);
    assert_eq!(
        std::fs::read_to_string(m.root().join(RECORDS).join("tty3")).unwrap(),
        "/usr/lib/systemd/system/getty@.service\n"
    );
    assert_eq!(m.console(&["disable", "3"]).0, 0);
    assert_eq!(
        m.links("getty.target.wants/getty@tty3.service"),
        to("getty")
    );
    assert_eq!(m.snapshot(), before);
}

/// A Debian host's vendor template link, `/lib/systemd/system/autovt@.service`.
fn vendor_template(m: &Machine) {
    let lib = m.root().join("lib/systemd/system");
    std::fs::create_dir_all(&lib).unwrap();
    symlink("getty@.service", lib.join(TEMPLATE)).unwrap();
}

#[test]
fn the_template_in_force_is_read_along_the_search_path() {
    let mut m = Machine::new("vendor-template");
    vendor_template(&m);
    let (status, out, _) = m.console(&["status"]);
    assert_eq!(status, 0);
    assert!(
        out.starts_with("On a VT with no link of its own, logind starts a getty"),
        "{out}"
    );
    assert!(
        out.lines()
            .any(|l| l.split_whitespace().collect::<Vec<_>>() == ["3", "-", "getty@tty3", "-"]),
        "{out}"
    );
}

#[test]
fn disable_all_after_the_hand_typed_steps_leaves_the_vendor_template_in_force() {
    let mut m = Machine::new("vendor-hand-typed");
    vendor_template(&m);
    m.link("getty.target.wants/getty@tty1.service", "getty");
    let before = m.snapshot();
    m.link(TEMPLATE, "vtium");
    let (status, out, err) = m.console(&["disable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert_eq!(m.links(TEMPLATE), Link::Absent);
    let mut after = m.snapshot();
    after.remove("var/lib/vtium/console");
    assert_eq!(after, before);
}

#[test]
fn a_display_manager_whose_unit_claims_another_vt_keeps_that_vt_and_vt_1() {
    let mut m = Machine::gettys("dm-vt2");
    let lib = m.root().join("usr/lib/systemd/system");
    std::fs::write(
        lib.join("gdm.service"),
        "[Unit]\nConflicts=getty@tty2.service plymouth-quit.service\nAfter=getty@tty2.service\n",
    )
    .unwrap();
    symlink(
        "/usr/lib/systemd/system/gdm.service",
        m.root().join(ETC).join(DISPLAY_MANAGER),
    )
    .unwrap();
    let (status, _, err) = m.console(&["enable", "2"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("the display manager gdm.service holds VT 2"),
        "{err}"
    );
    assert_eq!(m.console(&["enable", "--all"]).0, 0);
    assert!(m.wanted("getty@tty2.service"));
    // VT 1 stays the display manager's and its getty's by convention.
    assert!(!m.wanted("vtium@tty1.service") && m.wanted("getty@tty1.service"));
    let (_, out, _) = m.console(&["status"]);
    assert!(
        out.lines()
            .any(|l| l.starts_with("2 ") && l.contains("display manager (gdm.service)")),
        "{out}"
    );
}

#[test]
fn enable_all_passes_by_a_masked_vt_whose_getty_holds_it() {
    // A mask keeps vtium from a VT where logind starts no console through
    // the template: VT 1, and a VT whose enabled getty holds it from boot.
    let mut m = Machine::gettys("all-masked");
    for n in [1, 2] {
        symlink(
            MASK,
            m.root().join(ETC).join(format!("vtium@tty{n}.service")),
        )
        .unwrap();
    }
    let before = m.snapshot();
    let (status, out, err) = m.console(&["enable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    for n in [1, 2] {
        assert!(m.wanted(&format!("getty@tty{n}.service")), "{n}");
        assert!(!m.wanted(&format!("vtium@tty{n}.service")), "{n}");
        assert!(
            !m.root().join(RECORDS).join(format!("tty{n}")).exists(),
            "{n}"
        );
        assert!(
            out.contains(&format!(
                "VT {n} is left as it is, its getty included: vtium@tty{n}.service is masked"
            )),
            "{out}"
        );
    }
    assert!(!out.contains("systemctl start vtium@tty1.service"), "{out}");
    assert_eq!(m.links(TEMPLATE), to("vtium"));
    assert!(!m.wanted("getty@tty3.service"));
    assert_eq!(m.console(&["disable", "--all"]).0, 0);
    let mut after = m.snapshot();
    after.remove("var/lib/vtium/console");
    assert_eq!(after, before);
}

#[test]
fn enable_all_refuses_a_masked_vt_the_template_still_reaches() {
    // systemd loads autovt@ttyN.service through the template link even
    // while vtium@ttyN.service is masked (measured), so a mask keeps vtium
    // from neither VT 6, logind's reserved VT, nor a VT no getty holds.
    for (n, getty) in [(6, true), (3, false)] {
        let mut m = Machine::gettys(&format!("all-masked-{n}"));
        symlink(
            MASK,
            m.root().join(ETC).join(format!("vtium@tty{n}.service")),
        )
        .unwrap();
        if !getty {
            std::fs::remove_file(m.root().join(WANTS).join(format!("getty@tty{n}.service")))
                .unwrap();
        }
        let before = m.snapshot();
        let (status, _, err) = m.console(&["enable", "--all"]);
        assert_eq!(status, 1, "{n}");
        assert!(
            err.contains(&format!(
                "vtium@tty{n}.service is masked, but with autovt@.service linked to vtium"
            )),
            "{err}"
        );
        assert!(
            err.contains(&format!("systemctl unmask vtium@tty{n}.service")),
            "{err}"
        );
        assert!(m.changes().is_empty(), "{:?}", m.changes());
        assert_eq!(m.snapshot(), before);
        // VT 6's own link keeps the template from it.
        if n == 6 {
            m.link("autovt@tty6.service", "getty");
            let (status, out, err) = m.console(&["enable", "--all"]);
            assert_eq!((status, err.as_str()), (0, ""), "{out}");
            assert!(m.wanted("getty@tty6.service"));
        }
    }
}

#[test]
fn status_shows_no_console_logind_starts_on_vt_1() {
    // logind never starts autovt@tty1.service: asking whether VT 1 is busy,
    // it opens /dev/tty1, and that makes VT 1 busy.
    let mut m = Machine::gettys("status-vt1");
    let (_, out, _) = m.console(&["status"]);
    assert!(
        out.lines()
            .any(|l| l.split_whitespace().collect::<Vec<_>>() == ["1", "getty@tty1", "-", "-"]),
        "{out}"
    );
    let (status, out, _) = m.console(&["enable", "1"]);
    assert_eq!(status, 0);
    assert!(
        out.contains("systemd starts vtium@tty1 there at the next boot."),
        "{out}"
    );
    assert!(!out.contains("switch to VT 1"), "{out}");
    let (_, out, _) = m.console(&["status"]);
    assert!(
        out.lines().any(|l| l.split_whitespace().collect::<Vec<_>>()
            == ["1", "vtium@tty1,", "getty", "recorded", "-", "-"]),
        "{out}"
    );
}

#[test]
fn a_rerun_after_a_failed_reload_reloads() {
    // Each verb's last step is the reload. A run that finds the links done
    // reloads, so running the same command again finishes the change.
    for (words, setup) in [
        (&["enable", "--all"][..], &[][..]),
        (&["disable", "--all"], &["enable", "--all"][..]),
        (&["disable", "3"], &["enable", "3"]),
    ] {
        let mut m = Machine::gettys(&format!("reload-{}", words.join("-")));
        m.fake.running.push("vtium@tty3.service".into());
        if !setup.is_empty() {
            assert_eq!(m.console(setup).0, 0);
        }
        if words == ["disable", "3"] {
            // A hand-made link to take back, so disable N reloads.
            assert_eq!(m.links("autovt@tty3.service"), to("vtium"));
        }
        m.fake.fail = Some("daemon-reload".into());
        let (status, _, err) = m.console(words);
        assert_eq!(status, 1, "{words:?}");
        assert!(err.contains("systemctl daemon-reload failed"), "{err}");
        assert!(
            err.contains("Running the same command again finishes"),
            "{err}"
        );
        m.fake.fail = None;
        let snapshot = m.snapshot();
        let calls = m.changes().len();
        let (status, out, err) = m.console(words);
        assert_eq!((status, err.as_str()), (0, ""), "{words:?}: {out}");
        assert_eq!(
            m.changes().get(calls..).unwrap(),
            ["daemon-reload"],
            "{words:?}"
        );
        assert_eq!(m.snapshot(), snapshot, "{words:?}");
    }
}

/// sddm's unit, as [`display_manager`] installs it, with `files` written
/// under the root, each path relative to it.
fn display_manager_with(m: &Machine, files: &[(&str, &str)]) {
    display_manager(m);
    for (path, text) in files {
        let path = m.root().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

/// The VTs whose getty the display manager's unit names in `Conflicts=`,
/// as `vtium console status` gives them under its table. VT 1 is the
/// display manager's by convention whatever its unit names.
fn claimed(m: &mut Machine) -> Vec<u16> {
    let (_, out, _) = m.console(&["status"]);
    out.lines()
        .filter(|l| l.starts_with("VT ") && l.contains("in Conflicts="))
        .filter_map(|l| l.strip_prefix("VT ")?.split(':').next()?.parse().ok())
        .collect()
}

#[test]
fn a_display_managers_drop_ins_are_read_under_its_names_as_systemd_reads_them() {
    // Each claims one more VT: the alias's directory, its prefix directory,
    // and the directory for every service (measured).
    let mut m = Machine::gettys("dm-names");
    display_manager_with(
        &m,
        &[
            (
                "etc/systemd/system/display-manager.service.d/a.conf",
                "[Unit]\nConflicts=getty@tty2.service\n",
            ),
            (
                "etc/systemd/system/display-.service.d/b.conf",
                "[Unit]\nConflicts=getty@tty3.service\n",
            ),
            (
                "usr/lib/systemd/system/service.d/c.conf",
                "[Unit]\nConflicts=getty@tty4.service\n",
            ),
        ],
    );
    assert_eq!(claimed(&mut m), [2, 3, 4]);
    // A drop-in of one file name is read once: the unit's own name's
    // directories come before the alias's, so the vendor directory's copy
    // under sddm.service.d wins over /etc's under the alias (measured).
    let mut m = Machine::gettys("dm-same-name");
    display_manager_with(
        &m,
        &[
            (
                "etc/systemd/system/display-manager.service.d/vt.conf",
                "[Unit]\nConflicts=getty@tty2.service\n",
            ),
            (
                "usr/lib/systemd/system/sddm.service.d/vt.conf",
                "[Unit]\nConflicts=getty@tty3.service\n",
            ),
        ],
    );
    assert_eq!(claimed(&mut m), [3]);
}

#[test]
fn a_drop_in_in_an_earlier_directory_stands_for_one_of_its_name_in_a_later_one() {
    let mut m = Machine::gettys("dm-earlier");
    display_manager_with(
        &m,
        &[
            (
                "etc/systemd/system/sddm.service.d/vt.conf",
                "[Unit]\nConflicts=getty@tty2.service\n",
            ),
            (
                "usr/lib/systemd/system/sddm.service.d/vt.conf",
                "[Unit]\nConflicts=getty@tty3.service\n",
            ),
        ],
    );
    assert_eq!(claimed(&mut m), [2]);
}

#[test]
fn an_administrators_copy_of_the_display_managers_unit_is_the_one_read() {
    // systemd loads the unit by its name along the search path, so a copy
    // in /etc stands for the vendor file the link names (measured).
    let mut m = Machine::gettys("dm-copy");
    display_manager_with(
        &m,
        &[(
            "etc/systemd/system/sddm.service",
            "[Unit]\nConflicts=getty@tty3.service\n",
        )],
    );
    assert_eq!(claimed(&mut m), [3]);
}

#[test]
fn a_masked_display_manager_claims_nothing() {
    // display-manager.service masked, or the unit it names masked.
    for masked in [DISPLAY_MANAGER, "sddm.service"] {
        let mut m = Machine::gettys(&format!("dm-masked-{masked}"));
        display_manager(&m);
        let path = m.root().join(ETC).join(masked);
        let _ = std::fs::remove_file(&path);
        symlink(MASK, &path).unwrap();
        let (status, out, _) = m.console(&["status"]);
        assert_eq!(status, 0);
        assert!(!out.contains("display manager"), "{masked}: {out}");
        assert_eq!(m.console(&["enable", "1"]).0, 0, "{masked}");
    }
}

#[test]
fn a_login_on_loginds_reserved_vt_is_refused() {
    // logind starts autovt@tty6 on every switch to VT 6, so vtium there
    // would stop the getty a user is logged in at.
    let mut m = Machine::gettys("reserved-login");
    m.fake.running.push("getty@tty6.service".into());
    m.fake.sessions = vec![["carol", "6", "login", "tty", "user"]];
    let before = m.snapshot();
    for words in [&["enable", "--all"][..], &["enable", "6"]] {
        let (status, _, err) = m.console(words);
        assert_eq!(status, 1, "{words:?}");
        assert!(
            err.contains(
                "the login session of carol (login, tty) is on VT 6, logind's reserved VT"
            ),
            "{err}"
        );
        assert!(err.contains("Log out of VT 6 first"), "{err}");
    }
    assert!(m.changes().is_empty(), "{:?}", m.changes());
    assert_eq!(m.snapshot(), before);
    // A getty waiting at its login prompt on VT 6 holds no session, so both
    // verbs go ahead beside it, and say that the next switch stops it and
    // ends no session. A login on another VT is safe too: logind starts
    // nothing on a busy VT.
    m.fake.sessions = vec![["carol", "3", "login", "tty", "user"]];
    let (status, out, err) = m.console(&["enable", "6"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert!(
        out.contains(
            "which stops a getty there if one waits at its login prompt; that ends no session"
        ),
        "{out}"
    );
    assert!(out.contains("systemctl start vtium@tty6.service"), "{out}");
    assert_eq!(m.console(&["disable", "6"]).0, 0);
    let (status, out, err) = m.console(&["enable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert!(
        out.contains("A getty running now on VTs 2 to 5 keeps its VT"),
        "{out}"
    );
    assert!(
        out.contains(
            "On VT 6, logind's reserved VT, logind starts vtium at the next switch to it, which stops a getty there if one waits at its login prompt; that ends no session."
        ),
        "{out}"
    );
}

#[test]
fn kmscon_on_loginds_reserved_vt_keeps_it_only_by_its_own_link() {
    // Enabled or running on VT 6 without a link of its own, kmscon would be
    // stopped by vtium's Conflicts= at the first switch to VT 6.
    let mut m = Machine::gettys("reserved-kmscon");
    std::fs::remove_file(m.root().join(WANTS).join("getty@tty6.service")).unwrap();
    m.link("getty.target.wants/kmsconvt@tty6.service", "kmsconvt");
    let before = m.snapshot();
    let (status, _, err) = m.console(&["enable", "--all"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("kmsconvt@tty6.service, is enabled or running on VT 6, logind's reserved VT"),
        "{err}"
    );
    assert!(m.changes().is_empty(), "{:?}", m.changes());
    assert_eq!(m.snapshot(), before);
    // With VT 6's own link to kmscon, the template does not reach VT 6.
    m.link("autovt@tty6.service", "kmsconvt");
    let (status, out, err) = m.console(&["enable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert!(
        out.contains("VT 6 keeps the C reference's console"),
        "{out}"
    );
    assert!(!out.contains("logind's reserved VT"), "{out}");
}

#[test]
fn the_development_install_on_loginds_reserved_vt_keeps_it_only_by_its_own_link() {
    let mut m = Machine::gettys("reserved-dev");
    m.fake.running.push("vtium-dev@tty6.service".into());
    let before = m.snapshot();
    let (status, _, err) = m.console(&["enable", "--all"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("vtium-dev@tty6.service, is enabled or running on VT 6, logind's reserved VT"),
        "{err}"
    );
    assert!(err.contains("make vt-uninstall VT=6"), "{err}");
    assert!(m.changes().is_empty(), "{:?}", m.changes());
    assert_eq!(m.snapshot(), before);
    // make vt-install VT=6 links VT 6 to the development install.
    m.link("autovt@tty6.service", "vtium-dev");
    let (status, out, err) = m.console(&["enable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert!(out.contains("VT 6 keeps the development install"), "{out}");
}

#[test]
fn a_login_inside_vtium_on_loginds_reserved_vt_is_no_refusal() {
    // vtium sets XDG_VTNR for its login, so logind lists that login on VT
    // 6; the switch's start of vtium@tty6 finds vtium active and stops
    // nothing.
    let mut m = Machine::gettys("reserved-vtium-login");
    assert_eq!(m.console(&["enable", "--all"]).0, 0);
    m.fake.running.push("vtium@tty6.service".into());
    m.fake.sessions = vec![["carol", "6", "login", "tty", "user"]];
    let snapshot = m.snapshot();
    // A rerun, as after a failed reload, finds every VT done.
    let (status, out, err) = m.console(&["enable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert!(out.contains("already"), "{out}");
    assert_eq!(m.snapshot(), snapshot);
    // With vtium running on VT 6 already, there is no getty to stop and
    // nothing to start.
    let (status, out, err) = m.console(&["enable", "6"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert!(out.contains("vtium@tty6 runs there already."), "{out}");
    assert!(!out.contains("getty"), "{out}");
    assert!(!out.contains("To start it now"), "{out}");
    // A second enable 6 says that VT 6 is vtium's already, a login on VT 6
    // or not, and vtium running there or not.
    for running in [true, false] {
        if !running {
            m.fake.running.clear();
        }
        let (status, out, err) = m.console(&["enable", "6"]);
        assert_eq!((status, err.as_str()), (0, ""), "{running}: {out}");
        assert!(out.contains("VT 6 is vtium's already"), "{out}");
    }
}

#[test]
fn a_masked_vt_whose_own_link_names_vtium_still_has_vtium() {
    // enable 3 left VT 3's own link to vtium and no getty; a mask then does
    // not reach autovt@tty3.service, which loads vtium through the link
    // (measured).
    let mut m = Machine::gettys("masked-own-link");
    assert_eq!(m.console(&["enable", "3"]).0, 0);
    symlink(MASK, m.root().join(ETC).join("vtium@tty3.service")).unwrap();
    let (status, out, err) = m.console(&["enable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert!(
        out.contains("VT 3 is left as it is: vtium@tty3.service is masked, but VT 3's own link"),
        "{out}"
    );
    assert!(out.contains("vtium console disable 3"), "{out}");
    assert!(
        !out.contains("VT 3 is left as it is, its getty included"),
        "{out}"
    );
    // A masked VT above 6 with no getty keeps nothing it does not have.
    let mut m = Machine::new("masked-high");
    symlink(MASK, m.root().join(ETC).join("vtium@tty9.service")).unwrap();
    let (status, out, _) = m.console(&["enable", "--all"]);
    assert_eq!(status, 0, "{out}");
    assert!(
        out.contains("VT 9 is left as it is: vtium@tty9.service is masked"),
        "{out}"
    );
}

/// Enables the display manager `unit`, whose unit file is `text`, by
/// linking `display-manager.service` to it, and writes `files` under the
/// root, each path relative to it.
fn enable_dm(m: &Machine, unit: &str, text: &str, files: &[(&str, &str)]) {
    let lib = m.root().join("usr/lib/systemd/system");
    std::fs::write(lib.join(unit), text).unwrap();
    symlink(
        format!("/usr/lib/systemd/system/{unit}"),
        m.root().join(ETC).join(DISPLAY_MANAGER),
    )
    .unwrap();
    for (path, text) in files {
        let path = m.root().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

/// Arch's sddm.service, which names getty@tty1 whatever VT SDDM takes.
const SDDM: &str = "[Unit]\nDescription=Simple Desktop Display Manager\nConflicts=getty@tty1.service\nAfter=systemd-user-sessions.service getty@tty1.service plymouth-quit.service systemd-logind.service\n\n[Service]\nExecStart=/usr/bin/sddm\n\n[Install]\nAlias=display-manager.service\n";
/// SDDM's packaged default on Arch, an X11 greeter.
const SDDM_DEFAULT: (&str, &str) = (
    "usr/lib/sddm/sddm.conf.d/default.conf",
    "[General]\n# Which display server should be used.\nDisplayServer=x11\n\n[X11]\nServerArguments=-nolisten tcp\n",
);

#[test]
fn sddm_whose_greeter_runs_on_vt_2_still_keeps_vt_1() {
    // padnoir: SDDM's X11 greeter took VT 2, the first VT no program had
    // open. By convention VT 1 stays the display manager's and its getty's,
    // and the greeter's VT is held now as well.
    let mut m = Machine::gettys("sddm-vt2");
    enable_dm(&m, "sddm.service", SDDM, &[SDDM_DEFAULT]);
    m.fake.sessions = vec![["sddm", "2", "sddm-greeter", "x11", "greeter"]];
    let before = m.snapshot();
    let (status, _, err) = m.console(&["enable", "1"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("the display manager sddm.service holds VT 1 at boot: a display manager is enabled, and by common convention VT 1 stays the display manager's and its getty's, whatever VT the display manager uses now"),
        "{err}"
    );
    let (status, _, err) = m.console(&["enable", "2"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("the greeter of sddm (sddm-greeter) holds VT 2 now"),
        "{err}"
    );
    assert_eq!(m.snapshot(), before);
    let (status, out, _) = m.console(&["enable", "--all"]);
    assert_eq!(status, 0, "{out}");
    assert!(
        !m.wanted("vtium@tty1.service") && m.wanted("getty@tty1.service"),
        "{out}"
    );
    assert!(m.wanted("getty@tty2.service"), "{out}");
    assert!(!m.wanted("getty@tty3.service"), "{out}");
}

#[test]
fn a_display_manager_enabled_without_the_alias_keeps_vt_1() {
    // ly ships ly@.service with Alias=autovt@.service,
    // WantedBy=multi-user.target and DefaultInstance=tty2, and no
    // display-manager.service alias: enabled, it is ly@tty2 in
    // multi-user.target.wants.
    let mut m = Machine::gettys("ly");
    std::fs::write(
        m.root().join("usr/lib/systemd/system/ly@.service"),
        "[Unit]\nDescription=TUI display manager\nAfter=systemd-user-sessions.service getty@%i.service\nConflicts=getty@%i.service\n\n[Service]\nExecStart=/usr/bin/ly\nTTYPath=/dev/%I\n\n[Install]\nAlias=autovt@.service\nWantedBy=multi-user.target\nDefaultInstance=tty2\n",
    )
    .unwrap();
    m.link("multi-user.target.wants/ly@tty2.service", "ly");
    let before = m.snapshot();
    let (status, _, err) = m.console(&["enable", "1"]);
    assert_eq!(status, 1, "{err}");
    assert!(
        err.contains(
            "the display manager ly@tty2.service holds VT 1 at boot: a display manager is enabled"
        ),
        "{err}"
    );
    let (status, _, err) = m.console(&["enable", "2"]);
    assert_eq!(status, 1);
    assert!(err.contains("ly@tty2.service holds VT 2 at boot"), "{err}");
    assert_eq!(m.snapshot(), before);
}

#[test]
fn a_display_manager_whose_unit_names_no_getty_keeps_vt_1() {
    // Debian's lightdm.service has no Conflicts= at all.
    let mut m = Machine::gettys("dm-no-conflicts");
    enable_dm(
        &m,
        "lightdm.service",
        "[Unit]\nDescription=Light Display Manager\n",
        &[],
    );
    let (status, _, err) = m.console(&["enable", "1"]);
    assert_eq!(status, 1);
    assert!(
        err.contains(
            "the display manager lightdm.service holds VT 1 at boot: a display manager is enabled"
        ),
        "{err}"
    );
}

#[test]
fn an_sddm_minimum_vt_adds_its_vt() {
    let mut m = Machine::gettys("sddm-minimum-vt");
    enable_dm(
        &m,
        "sddm.service",
        SDDM,
        &[SDDM_DEFAULT, ("etc/sddm.conf", "[X11]\nMinimumVT=3\n")],
    );
    let (status, _, err) = m.console(&["enable", "3"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("the display manager sddm.service holds VT 3 at boot, by MinimumVT=3 in /etc/sddm.conf: SDDM before 0.20 starts its greeter on VT 3 or the first free VT above it"),
        "{err}"
    );
    assert_eq!(m.console(&["enable", "1"]).0, 1);
}

#[test]
fn gdm_on_vt_1_holds_it() {
    let gdm = "[Unit]\nDescription=GNOME Display Manager\nConflicts=getty@tty1.service plymouth-quit.service\nAfter=getty@tty1.service\n";
    let mut m = Machine::gettys("gdm");
    enable_dm(&m, "gdm.service", gdm, &[]);
    // A greeter of GDM's on VT 2, as a user switch starts, changes nothing.
    m.fake.sessions = vec![["gdm", "2", "gdm-launch-environment", "wayland", "greeter"]];
    let (status, _, err) = m.console(&["enable", "1"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("the display manager gdm.service holds VT 1 at boot"),
        "{err}"
    );
    let (_, out, _) = m.console(&["enable", "--all"]);
    assert!(
        !m.wanted("vtium@tty1.service") && m.wanted("getty@tty1.service"),
        "{out}"
    );
}

#[test]
fn lightdm_holds_vt_1_and_vt_7() {
    let lightdm = "[Unit]\nDescription=Light Display Manager\nConflicts=getty@tty1.service\nAfter=getty@tty1.service\n";
    let mut m = Machine::gettys("lightdm");
    enable_dm(&m, "lightdm.service", lightdm, &[]);
    let (status, _, err) = m.console(&["enable", "7"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("the display manager lightdm.service holds VT 7 at boot, by LightDM's built-in minimum-vt=7, which no file changes: LightDM starts its first greeter on VT 7"),
        "{err}"
    );
    let (status, out, err) = m.console(&["enable", "--all"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert!(!m.wanted("vtium@tty1.service"), "{out}");
    assert!(
        out.contains(
            "VT 1 is left as it is: the display manager lightdm.service holds VT 1 at boot"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "VT 7 is left as it is: the display manager lightdm.service holds VT 7 at boot"
        ),
        "{out}"
    );
}

#[test]
fn greetd_holds_vt_1_and_the_vt_its_configuration_names() {
    let greetd = "[Unit]\nDescription=Greeter daemon\nConflicts=getty@tty1.service\n[Service]\nExecStart=greetd\n";
    let mut m = Machine::gettys("greetd");
    enable_dm(
        &m,
        "greetd.service",
        greetd,
        &[("etc/greetd/config.toml", "[terminal]\nvt = 2\n")],
    );
    for n in ["1", "2"] {
        let (status, _, err) = m.console(&["enable", n]);
        assert_eq!(status, 1, "{n}");
        assert!(
            err.contains(&format!("greetd.service holds VT {n} at boot")),
            "{err}"
        );
    }
    let (_, _, err) = m.console(&["enable", "2"]);
    assert!(
        err.contains("by vt = 2 in /etc/greetd/config.toml: greetd runs its greeter on VT 2"),
        "{err}"
    );
}

#[test]
fn an_unknown_display_manager_holds_vt_1_and_the_vt_its_conflicts_name() {
    let mut m = Machine::gettys("unknown-dm");
    enable_dm(
        &m,
        "ly.service",
        "[Unit]\nConflicts=getty@tty2.service\n",
        &[],
    );
    let before = m.snapshot();
    let (status, _, err) = m.console(&["enable", "2"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("the display manager ly.service holds VT 2 at boot: its unit names getty@tty2.service in Conflicts=, so it takes the VT from its getty"),
        "{err}"
    );
    assert_eq!(m.console(&["enable", "1"]).0, 1);
    assert_eq!(m.snapshot(), before);
    let (status, out, _) = m.console(&["enable", "--all"]);
    assert_eq!(status, 0, "{out}");
    assert!(
        m.wanted("getty@tty1.service") && m.wanted("getty@tty2.service"),
        "{out}"
    );
}

#[test]
fn a_unit_enabled_on_a_vt_or_another_console_running_there_holds_it() {
    let mut m = Machine::gettys("kiosk");
    let lib = m.root().join("usr/lib/systemd/system");
    std::fs::write(
        lib.join("cage@.service"),
        "[Unit]\nConflicts=getty@%i.service\n[Service]\nTTYPath=/dev/%I\n",
    )
    .unwrap();
    std::fs::write(
        lib.join("debug-shell.service"),
        "[Service]\nTTYPath=/dev/tty9\n",
    )
    .unwrap();
    for (target, unit) in [
        ("graphical.target.wants", "cage@tty3.service"),
        ("sysinit.target.wants", "debug-shell.service"),
    ] {
        let dir = m.root().join(ETC).join(target);
        std::fs::create_dir_all(&dir).unwrap();
        let file = unit.replace("tty3", "");
        symlink(format!("/usr/lib/systemd/system/{file}"), dir.join(unit)).unwrap();
    }
    m.fake.running.push("fbterm@tty4.service".into());
    m.fake.running.push("serial-getty@ttyS0.service".into());
    for (n, says) in [
        (
            "3",
            "cage@tty3.service holds VT 3 at boot: it is enabled at boot on that VT",
        ),
        (
            "9",
            "debug-shell.service holds VT 9 at boot: its unit runs on /dev/tty9 (TTYPath=)",
        ),
        ("4", "another console, fbterm@tty4.service, holds VT 4 now"),
    ] {
        let (status, _, err) = m.console(&["enable", n]);
        assert_eq!(status, 1, "{n}");
        assert!(err.contains(says), "{n}: {err}");
    }
    let (status, out, _) = m.console(&["enable", "--all"]);
    assert_eq!(status, 0, "{out}");
    assert!(
        m.wanted("getty@tty3.service") && m.wanted("getty@tty4.service"),
        "{out}"
    );
    assert!(!m.wanted("getty@tty5.service"), "{out}");
    let (_, out, _) = m.console(&["status"]);
    assert!(
        out.lines()
            .any(|l| l.starts_with("3 ") && l.contains("cage@tty3")),
        "{out}"
    );
}

/// A machine whose vtium@ttyN failed. Its `systemctl show` table holds
/// what systemd 262 printed for `getty@ttyN.service`: its own `Conflicts=`,
/// and `ConflictedBy=` the failed vtium and `others` (measured).
fn failed_vtium(name: &str, n: u16, others: &[&str]) -> Machine {
    let mut m = Machine::gettys(name);
    m.link(&format!("getty.target.wants/vtium@tty{n}.service"), "vtium");
    let vtium = format!("vtium@tty{n}.service");
    let mut by = vec![vtium.as_str()];
    by.extend(others);
    m.fake.shows(
        &format!("getty@tty{n}.service"),
        &[
            ("Conflicts", "rescue.service shutdown.target"),
            ("ConflictedBy", &by.join(" ")),
        ],
    );
    m.fake.shows(
        &vtium,
        &[("ActiveState", "failed"), ("UnitFileState", "enabled")],
    );
    for unit in ["rescue.service", "shutdown.target"] {
        m.fake.shows(
            unit,
            &[("ActiveState", "inactive"), ("UnitFileState", "static")],
        );
    }
    m
}

/// Whether the fallback started any unit.
fn started(m: &Machine) -> bool {
    m.fake.calls.iter().any(|c| c.contains(" start "))
}

#[test]
fn the_fallback_starts_no_getty_whose_start_stops_the_display_manager() {
    for (active, file, job, says) in [
        ("active", "enabled", "", "sddm.service (active, enabled)"),
        // Enabled but stopped, it starts at the next boot.
        (
            "inactive",
            "enabled",
            "",
            "sddm.service (inactive, enabled)",
        ),
        (
            "activating",
            "static",
            "",
            "sddm.service (activating, static)",
        ),
        (
            "reloading",
            "disabled",
            "",
            "sddm.service (reloading, disabled)",
        ),
        (
            "inactive",
            "indirect",
            "",
            "sddm.service (inactive, indirect)",
        ),
        (
            "inactive",
            "generated",
            "",
            "sddm.service (inactive, generated)",
        ),
        // A job queued for a unit that is not enabled.
        (
            "inactive",
            "disabled",
            "42 start",
            "sddm.service (inactive, disabled, job 42 start)",
        ),
    ] {
        let mut m = failed_vtium(
            &format!("fallback-{active}-{file}-{}", job.len()),
            1,
            &["sddm.service"],
        );
        m.fake.shows(
            "sddm.service",
            &[
                ("ActiveState", active),
                ("UnitFileState", file),
                ("Job", job),
            ],
        );
        let (status, out, err) = m.console(&["fallback", "tty1"]);
        assert_eq!((status, err.as_str()), (0, ""), "{says}: {out}");
        assert!(!started(&m), "{says}: {:?}", m.fake.calls);
        assert!(
            out.contains(&format!("vtium on VT 1 failed, and VT 1 is left without a console: starting getty@tty1.service would stop {says}, which conflicts with it, so vtium starts nothing.")),
            "{says}: {out}"
        );
    }
}

#[test]
fn the_fallback_gives_an_ordinary_vt_its_getty_back() {
    // getty@tty3's own Conflicts=, rescue.service and shutdown.target,
    // are static and stopped, so they hold nothing back.
    let mut m = failed_vtium("fallback-plain", 3, &[]);
    let (status, out, err) = m.console(&["fallback", "3"]);
    assert_eq!((status, err.as_str()), (0, ""), "{out}");
    assert!(
        out.contains("vtium on VT 3 failed: starting getty@tty3.service in its place, which stops no other program."),
        "{out}"
    );
    assert_eq!(
        m.fake.calls,
        [
            "show -p Conflicts -p ConflictedBy -- getty@tty3.service",
            "show -p Id -p ActiveState -p UnitFileState -p Job -- rescue.service shutdown.target",
            "--no-block --job-mode=fail start getty@tty3.service",
        ]
    );
    // A unit that conflicts with the getty but is stopped and disabled.
    let mut m = failed_vtium("fallback-vt1", 1, &["sddm.service"]);
    m.fake.shows(
        "sddm.service",
        &[("ActiveState", "inactive"), ("UnitFileState", "disabled")],
    );
    assert_eq!(m.console(&["fallback", "1"]).0, 0);
    assert_eq!(
        m.fake.calls.last().map(String::as_str),
        Some("--no-block --job-mode=fail start getty@tty1.service")
    );
}

#[test]
fn the_fallback_starts_nothing_when_it_cannot_ask_the_manager() {
    let mut m = failed_vtium("fallback-down", 1, &[]);
    m.fake.fail = Some("show -p Conflicts -p ConflictedBy -- getty@tty1.service".into());
    let (status, _, err) = m.console(&["fallback", "1"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("VT 1 is left without a console: vtium cannot tell whether starting getty@tty1.service would stop another program"),
        "{err}"
    );
    assert!(!started(&m), "{:?}", m.fake.calls);
    // It needs root and systemd, as the other verbs do, and one VT.
    assert_eq!(m.console_as(false, &["fallback", "1"]).0, 1);
    assert!(matches!(
        parse(&words(&["console", "fallback", "--all"])),
        Some(Err(_))
    ));
}

#[test]
fn the_fallback_starts_nothing_when_the_manager_answers_for_fewer_units() {
    let mut m = failed_vtium("fallback-short", 1, &["sddm.service"]);
    m.fake.short = true;
    let (status, _, err) = m.console(&["fallback", "1"]);
    assert_eq!(status, 1);
    assert!(
        err.contains("systemctl show did not give the state of each of rescue.service, shutdown.target, sddm.service."),
        "{err}"
    );
    assert!(!started(&m), "{:?}", m.fake.calls);
}

#[test]
fn the_shipped_fallback_unit_runs_vtium_console_fallback() {
    // A getty started by the unit itself would stop a display manager
    // whose Conflicts= names it.
    let unit = include_str!("../../install/vtium-getty@.service");
    let exec: Vec<&str> = unit
        .lines()
        .filter(|l| l.starts_with("ExecStart="))
        .collect();
    assert_eq!(exec, ["ExecStart=/usr/bin/vtium console fallback %i"]);
    assert!(unit.contains("ConditionPathExists=!/run/vtium/%i.closed"));
}
