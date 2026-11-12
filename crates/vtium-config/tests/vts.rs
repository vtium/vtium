//! The step after an install (ADR-1201 D5): vtium enabled on each VT a
//! head names where it is not, through `vtium console enable`, and the
//! start and restart that apply the new vtium.conf, run only after a yes.
//!
//! [`Machine`] stands in for sudo, `vtium console` and `systemctl`, and
//! checks what each real one checks: `vtium console enable` and
//! `systemctl start` and `restart` refuse a user who is not root, `vtium
//! console enable` refuses a VT another program holds and says when vtium
//! is enabled there already, and `vtium console status` prints its table
//! as the real one lays it out. The install itself runs through the
//! session and the TUI's desk with the system's directories, whose
//! privileged step, `sudo vtium-config apply DIR`, the stand-in sudo
//! answers without writing anything.

use vtium_testscript as script;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use vtium_config::drafts::{Session, Unrouted};
use vtium_config::tui::desk::{Desk, Done, Op};
use vtium_config::vts::{self, Applied, Mode, Status};

/// The machine the commands act on.
#[derive(Clone, Debug, Default)]
struct Machine {
    /// The VTs where vtium's unit starts at boot, by mode.
    enabled: BTreeMap<u16, Mode>,
    /// The VTs vtium runs on now, by mode.
    running: BTreeMap<u16, Mode>,
    /// The VTs a graphical session holds, with its words.
    held: BTreeMap<u16, String>,
    /// What else runs on a VT, such as a getty.
    others: BTreeMap<u16, String>,
    /// Every command run, with `root` before it when sudo ran it.
    ran: Vec<String>,
}

impl Machine {
    /// `vtium console status`'s table, laid out as the real one lays it
    /// out: each column but the last padded to its widest cell and two
    /// blanks.
    fn status(&self) -> String {
        let mut vts: Vec<u16> = (1..=6).collect();
        vts.extend(self.enabled.keys());
        vts.extend(self.running.keys());
        vts.extend(self.held.keys());
        vts.extend(self.others.keys());
        vts.sort_unstable();
        vts.dedup();
        let header = ["VT", "at boot", "when switched to", "running"].map(str::to_owned);
        let rows: Vec<[String; 4]> = vts
            .iter()
            .map(|&n| {
                let boot = match (n, self.enabled.get(&n)) {
                    (1, _) => "getty@tty1, display manager (sddm.service)".to_owned(),
                    (_, Some(m)) => format!("{}, getty to restore", m.unit(n)),
                    _ => String::new(),
                };
                let switched = match self.enabled.get(&n) {
                    Some(m) if (2..=6).contains(&n) => m.unit(n),
                    _ => String::new(),
                };
                let mut running: Vec<String> = Vec::new();
                if let Some(m) = self.running.get(&n) {
                    running.push(m.unit(n));
                }
                running.extend(self.others.get(&n).cloned());
                running.extend(self.held.get(&n).cloned());
                [n.to_string(), boot, switched, running.join(", ")]
                    .map(|c| if c.is_empty() { "-".to_owned() } else { c })
            })
            .collect();
        let mut widths = header.clone().map(|h| h.len());
        for row in &rows {
            for (w, cell) in widths.iter_mut().zip(row) {
                *w = (*w).max(cell.len());
            }
        }
        let mut out = String::from(
            "On a VT with no link of its own, logind starts vtium: autovt@.service links to vtium@.service.\n",
        );
        for row in std::iter::once(&header).chain(&rows) {
            let mut line = String::new();
            for (i, (cell, w)) in row.iter().zip(widths).enumerate() {
                if i == 3 {
                    line.push_str(cell);
                } else {
                    line.push_str(&format!("{cell:<w$}  "));
                }
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out.push_str("VT 1: the display manager sddm.service uses VT 1 at boot.\n");
        out
    }

    fn run(&mut self, argv: &[OsString]) -> io::Result<(bool, String)> {
        let words: Vec<String> = argv
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let (root, words) = match words.split_first() {
            Some((sudo, rest)) if sudo == "sudo" => (true, rest.to_vec()),
            _ => (false, words.clone()),
        };
        self.ran.push(format!(
            "{}{}",
            if root { "root " } else { "" },
            words.join(" ")
        ));
        let w: Vec<&str> = words.iter().map(String::as_str).collect();
        match w.as_slice() {
            [_, "apply", _] if root => Ok((true, "wrote /etc/vtium/vtium.conf\n".to_owned())),
            [VTIUM, "console", "status"] => Ok((true, self.status())),
            [VTIUM, "console", "enable", flag, n] => {
                let words = format!("enable {flag} {n}");
                if !root {
                    return Ok((
                        false,
                        format!(
                            "vtium console {words}: this needs root, because it changes systemd units. For example: sudo vtium console {words}\n"
                        ),
                    ));
                }
                let n: u16 = n.parse().expect("a VT");
                let mode = if *flag == "--seat" {
                    Mode::Seat
                } else {
                    Mode::Direct
                };
                if let Some(holder) = self.held.get(&n) {
                    return Ok((
                        false,
                        format!(
                            "vtium console {words}: VT {n} has a {holder}. vtium is not enabled on VT {n}, so that it never takes the VT or the display away. Give vtium another VT.\n"
                        ),
                    ));
                }
                if self.enabled.get(&n) == Some(&mode) {
                    return Ok((
                        true,
                        format!(
                            "vtium is already enabled in {} on VT {n}. Nothing changed.\n",
                            mode_words(mode)
                        ),
                    ));
                }
                self.enabled.insert(n, mode);
                Ok((
                    true,
                    format!(
                        "vtium is enabled in {} on VT {n}. systemd starts it there at the next boot. To start it now, run systemctl start {}.service; that ends every session on VT {n}.\n",
                        mode_words(mode),
                        mode.unit(n)
                    ),
                ))
            }
            ["systemctl", verb @ ("start" | "restart"), units @ ..] => {
                if !root {
                    return Ok((
                        false,
                        format!("Failed to {verb} {}: Access denied\n", units.join(" ")),
                    ));
                }
                for unit in units {
                    let (mode, n) =
                        unit_vt(unit).unwrap_or_else(|| panic!("{unit} is no unit of vtium's"));
                    self.running.insert(n, mode);
                    self.others.remove(&n);
                }
                Ok((true, String::new()))
            }
            _ => Err(io::Error::new(io::ErrorKind::NotFound, "no such command")),
        }
    }

    /// The commands that changed the machine: what root ran, but sudo's
    /// own `apply`.
    fn changes(&self) -> Vec<String> {
        self.ran
            .iter()
            .filter(|c| c.starts_with("root ") && !c.contains(" apply "))
            .cloned()
            .collect()
    }
}

/// The stand-in console's path, as the session names it.
const VTIUM: &str = "/usr/bin/vtium";

/// A mode as `vtium console` names it.
fn mode_words(mode: Mode) -> &'static str {
    match mode {
        Mode::Direct => "direct mode",
        Mode::Seat => "seat mode",
    }
}

/// The VT and mode of a unit of vtium's, `vtium@tty12`.
fn unit_vt(unit: &str) -> Option<(Mode, u16)> {
    let (mode, n) = if let Some(n) = unit.strip_prefix("vtium-seat@tty") {
        (Mode::Seat, n)
    } else {
        (Mode::Direct, unit.strip_prefix("vtium@tty")?)
    };
    Some((mode, n.parse().ok()?))
}

/// vtium on VTs 2 and 3 in direct mode, and a graphical session on VT 7.
fn laptop() -> Machine {
    let mut m = Machine::default();
    for n in [2, 3] {
        m.enabled.insert(n, Mode::Direct);
        m.running.insert(n, Mode::Direct);
    }
    m.held
        .insert(7, "graphical session of aaron (sddm, wayland)".into());
    m
}

const CONF: &str = "version = 1\n[look default]\n[head desk]\n[head terminal-1]\nvt = 12\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n";

/// Runs `reach` on `conf` against `machine`, answering `answer`, with no
/// question when `answer` is `None`: what it printed, the questions it
/// asked, and what it did.
fn reach(
    conf: &str,
    machine: &mut Machine,
    answer: Option<bool>,
) -> (String, Vec<String>, vts::Outcome) {
    let mut out = Vec::new();
    let mut asked = Vec::new();
    let mut ask = |out: &mut dyn io::Write, q: &str| -> bool {
        let _ = writeln!(out, "{q}");
        asked.push(q.to_owned());
        answer.unwrap_or(false)
    };
    let mut run = |argv: &[OsString]| machine.run(argv);
    let ask: Option<vtium_config::apply::Ask<'_>> = if answer.is_some() {
        Some(&mut ask)
    } else {
        None
    };
    let outcome = vts::reach(
        conf,
        &VTIUM.into(),
        &["sudo".into()],
        &mut run,
        ask,
        &mut out,
    );
    (String::from_utf8(out).expect("UTF-8"), asked, outcome)
}

#[test]
fn a_vt_a_head_names_where_vtium_is_not_enabled_is_enabled_and_started_after_a_yes() {
    let mut m = laptop();
    let (out, asked, outcome) = reach(CONF, &mut m, Some(true));
    assert_eq!(
        m.changes(),
        [
            "root /usr/bin/vtium console enable --direct 12",
            "root systemctl start vtium@tty12",
            "root systemctl restart vtium@tty2 vtium@tty3",
        ],
        "{out}"
    );
    // vtium console's own words reach the person
    assert!(
        out.contains(
            "vtium is enabled in direct mode on VT 12. systemd starts it there at the next boot."
        ),
        "{out}"
    );
    assert!(
        out.contains("Head terminal-1 runs on VT 12, but vtium is not enabled there."),
        "{out}"
    );
    assert!(out.contains("vtium-config enables vtium there in direct mode, the mode vtium uses on the VTs it is enabled on now:"), "{out}");
    assert!(
        out.contains("sudo /usr/bin/vtium console enable --direct 12\n"),
        "{out}"
    );
    assert_eq!(asked.len(), 1);
    assert!(
        asked[0].ends_with("Start vtium on VT 12, and restart it on VTs 2 and 3?"),
        "{asked:?}"
    );
    assert!(
        asked[0].contains("A restart keeps the sessions on a VT running"),
        "{asked:?}"
    );
    assert!(
        out.contains("vtium now runs with the new vtium.conf on VTs 2, 3, and 12."),
        "{out}"
    );
    assert_eq!(outcome.applied, Applied::Done(vec![2, 3, 12]));
    assert!(outcome.refused.is_empty());
}

#[test]
fn a_vt_vtium_is_enabled_on_already_is_not_enabled_again() {
    let mut m = laptop();
    let conf = CONF.replace("vt = 12", "vt = 3");
    let (out, asked, _) = reach(&conf, &mut m, Some(true));
    assert!(
        !m.ran.iter().any(|c| c.contains("console enable")),
        "{:?}\n{out}",
        m.ran
    );
    assert!(!out.contains("A head is a terminal"), "{out}");
    // the restart is still offered: the new file applies only then
    assert_eq!(
        asked,
        [
            "A restart keeps the sessions on a VT running, because systemd holds their terminals open while vtium starts again. Each screen is cleared and drawn again, and its scrollback is lost. Restart vtium on VTs 2 and 3?"
        ]
    );
}

#[test]
fn vtium_console_s_refusal_is_shown_and_the_head_s_vt_named() {
    let mut m = laptop();
    let conf = CONF.replace("vt = 12", "vt = 7");
    let (out, _, outcome) = reach(&conf, &mut m, Some(false));
    assert!(
        out.contains("vtium console enable --direct 7: VT 7 has a graphical session of aaron (sddm, wayland). vtium is not enabled on VT 7"),
        "{out}"
    );
    assert!(out.contains("vtium console did not enable vtium on VT 7, and the lines above say why. Head terminal-1 does not appear until VT 7 is free and vtium is enabled there. The new vtium.conf is installed all the same. Once VT 7 is free, run: sudo /usr/bin/vtium console enable --direct 7"), "{out}");
    assert_eq!(outcome.refused, [(7, vec!["terminal-1".to_owned()])]);
    // nothing starts on the refused VT
    assert!(
        !m.ran.iter().any(|c| c.contains("vtium@tty7")),
        "{:?}",
        m.ran
    );
    assert_eq!(
        vts::message("desk", &outcome),
        "Installed draft desk. The console uses it after vtium restarts. vtium is not enabled on VT 7, so head terminal-1 does not appear there."
    );
}

#[test]
fn nothing_starts_or_restarts_without_a_yes() {
    for answer in [Some(false), None] {
        let mut m = laptop();
        let (out, asked, outcome) = reach(CONF, &mut m, answer);
        assert_eq!(
            m.changes(),
            ["root /usr/bin/vtium console enable --direct 12"],
            "{answer:?}: {out}"
        );
        assert_eq!(asked.len(), usize::from(answer.is_some()), "{answer:?}");
        assert!(
            out.ends_with("To apply the new vtium.conf now, run:\n  sudo systemctl start vtium@tty12\n  sudo systemctl restart vtium@tty2 vtium@tty3\n"),
            "{answer:?}: {out}"
        );
        let applied = if answer.is_some() {
            Applied::Declined
        } else {
            Applied::Nothing
        };
        assert_eq!(outcome.applied, applied);
    }
}

#[test]
fn the_mode_is_the_one_vtium_s_vts_use_else_direct_mode() {
    let mut seat = Machine::default();
    seat.enabled.insert(2, Mode::Seat);
    seat.running.insert(2, Mode::Seat);
    let (out, _, _) = reach(CONF, &mut seat, Some(true));
    assert_eq!(
        seat.changes(),
        [
            "root /usr/bin/vtium console enable --seat 12",
            "root systemctl start vtium-seat@tty12",
            "root systemctl restart vtium-seat@tty2",
        ],
        "{out}"
    );
    let mut mixed = seat.clone();
    mixed.ran.clear();
    mixed.enabled.insert(3, Mode::Direct);
    mixed.enabled.remove(&12);
    mixed.running.remove(&12);
    let (out, _, _) = reach(CONF, &mut mixed, None);
    assert!(out.contains("vtium-config enables vtium there in direct mode, because vtium uses direct mode on some VTs and seat mode on others:"), "{out}");
    let mut none = Machine::default();
    let (out, asked, _) = reach(CONF, &mut none, Some(true));
    assert!(out.contains("vtium-config enables vtium there in direct mode, because vtium is not enabled on any VT yet:"), "{out}");
    assert_eq!(asked, ["Start vtium on VT 12?"]);
}

#[test]
fn starting_vtium_where_a_getty_runs_says_it_ends_its_sessions() {
    let mut m = laptop();
    m.others.insert(12, "getty@tty12".into());
    let (_, asked, _) = reach(CONF, &mut m, Some(false));
    assert!(
        asked[0].contains(
            "Starting vtium on VT 12 stops what runs there now, getty@tty12, and ends its sessions."
        ),
        "{asked:?}"
    );
}

#[test]
fn a_status_that_cannot_be_read_enables_nothing_and_says_why() {
    let mut out = Vec::new();
    let mut ran = Vec::new();
    let mut run = |argv: &[OsString]| -> io::Result<(bool, String)> {
        ran.push(vts::shown(argv));
        Ok((
            false,
            "vtium console status: this host was not booted with systemd.\n".to_owned(),
        ))
    };
    let outcome = vts::reach(
        CONF,
        &VTIUM.into(),
        &["sudo".into()],
        &mut run,
        None,
        &mut out,
    );
    let out = String::from_utf8(out).expect("UTF-8");
    assert_eq!(ran, ["/usr/bin/vtium console status"]);
    assert_eq!(
        out,
        format!(
            "{}\n/usr/bin/vtium console status\nvtium console status: this host was not booted with systemd.\n",
            vts::STATUS_UNREADABLE
        )
    );
    assert_eq!(outcome, vts::Outcome::default());
}

/// `vtium console status` as it ran on the maintainer's machine, read as
/// the real table lays it out: a masked unit is not enabled, and a VT
/// only logind starts vtium on when switched to is.
#[test]
fn the_real_status_table_reads_as_vtium_console_lays_it_out() {
    let text = "On a VT with no link of its own, logind starts vtium: autovt@.service links to vtium@.service.
The display manager is sddm.service (/etc/systemd/system/display-manager.service links to it).
VT  at boot                                     when switched to  running
1   getty@tty1, display manager (sddm.service)  -                 -
2   vtium@tty2, getty to restore                vtium@tty2        vtium@tty2
3   vtium@tty3 masked                           -                 -
4   -                                           vtium-seat@tty4   vtium-seat@tty4
5   vtium-dev@tty5                              vtium-dev@tty5    vtium-dev@tty5
6   vtium@tty6, getty to restore                vtium@tty6        vtium@tty6
7   -                                           -                 graphical session of aaron (sddm, wayland)
VT 1: the display manager sddm.service uses VT 1 at boot.
";
    let status = vts::read_status(text).expect("the table");
    assert_eq!(
        status,
        Status {
            enabled: BTreeMap::from([(2, Mode::Direct), (4, Mode::Seat), (6, Mode::Direct)]),
            running: BTreeMap::from([(2, Mode::Direct), (4, Mode::Seat), (6, Mode::Direct)]),
            others: BTreeMap::from([
                (5, "vtium-dev@tty5".to_owned()),
                (7, "graphical session of aaron (sddm, wayland)".to_owned()),
            ]),
        }
    );
    assert_eq!(vts::read_status("vtium console status: no systemd\n"), None);
    // the stand-in's table reads the same way
    let m = laptop();
    let status = vts::read_status(&m.status()).expect("the stand-in's table");
    assert_eq!(status.enabled, m.enabled);
    assert_eq!(status.running, m.running);
}

/// The console the session checks a draft with: `check-config` passes
/// every draft.
const CHECKER: &str = "#!/bin/sh\n[ \"$1\" = check-config ] || exit 9\necho \"$4: checked\"\n";

/// A store and paths of the test's own, with the system's configuration
/// directory, so the install takes sudo's route and the step after it runs.
struct Bench {
    root: PathBuf,
}

fn bench(name: &str) -> Bench {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("vts")
        .join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("a fresh bench");
    }
    std::fs::create_dir_all(root.join("home")).expect("the bench");
    script::executable(&root.join("vtium"), CHECKER);
    Bench { root }
}

impl Bench {
    fn store(&self) -> vtium_config::store::Store {
        let root = self.root.clone();
        vtium_config::store::Store::locate(
            1000,
            move |name: &str| match name {
                "HOME" => Some(root.join("home").into()),
                "XDG_CONFIG_HOME" => Some(root.join("home/config").into()),
                "XDG_STATE_HOME" => Some(root.join("home/state").into()),
                _ => None,
            },
            |_| None,
        )
        .expect("the store")
    }

    fn paths(&self) -> vtium_config::tui::Paths {
        let system = vtium_config::apply::Targets::system();
        vtium_config::tui::Paths {
            configdir: system.configdir,
            frits: system.frits,
            drafts: self.root.join("home/bundles"),
            sudo: "sudo".into(),
            vtium: self.root.join("vtium").into(),
            helper: vtium_config::specimen::Helper {
                program: "/bin/false".into(),
                options: Vec::new(),
            },
            sys: self.root.join("sys"),
        }
    }

    /// The draft `bench`, current, holding `text`, a vtium.conf no machine
    /// has installed.
    fn draft(&self, text: &str) {
        let store = self.store();
        store.write_draft("bench", text).expect("the draft");
        store.set_current(Some("bench")).expect("the settings");
    }
}

/// `machine`'s runner, with the stand-in console's path as the session
/// names it read as [`VTIUM`].
fn runner<'a>(
    machine: &'a mut Machine,
    vtium: &'a OsString,
) -> impl FnMut(&[OsString]) -> io::Result<(bool, String)> + 'a {
    move |argv: &[OsString]| {
        let argv: Vec<OsString> = argv
            .iter()
            .map(|a| if a == vtium { VTIUM.into() } else { a.clone() })
            .collect();
        machine.run(&argv)
    }
}

/// A draft that adds a comment line, so it differs from whatever the
/// machine running the test has installed.
fn conf(name: &str) -> String {
    format!("# the {name} test's draft\n{CONF}")
}

/// The command line's install and the TUI's install run the same step
/// after the install, with the same commands and the same words.
#[test]
fn the_tui_and_the_command_line_install_do_the_same_after_the_install() {
    let mut said: Vec<(Vec<String>, String, Vec<String>)> = Vec::new();
    // the command line's install
    {
        let b = bench("cli");
        let text = conf("cli-tui");
        b.draft(&text);
        let paths = b.paths();
        let session = Session {
            store: b.store(),
            paths: &paths,
        };
        let mut m = laptop();
        let mut out = Vec::new();
        let mut asked = Vec::new();
        let mut ask = |out: &mut dyn io::Write, q: &str| -> bool {
            let _ = writeln!(out, "{q}");
            asked.push(q.to_owned());
            true
        };
        let mut run = runner(&mut m, &paths.vtium);
        session
            .install(
                "bench",
                &text,
                Unrouted::Refuse,
                &mut run,
                Some(&mut ask),
                &mut out,
            )
            .expect("installed");
        drop(run);
        let out = String::from_utf8(out).expect("UTF-8");
        let after = out[out.find("A head is").expect("the step after the install")..]
            .replace(&*paths.vtium.to_string_lossy(), VTIUM);
        said.push((m.changes(), after, asked));
    }
    // the TUI's install, through the real desk
    {
        let b = bench("tui");
        let text = conf("cli-tui");
        b.draft(&text);
        let mut desk =
            vtium_config::tui::desk::Store::new(b.store(), b.paths(), Box::new(Default::default));
        let plan = match desk.run(&Op::Prepare).expect("prepared") {
            Done::Plan(plan) => plan,
            other => panic!("{other:?}"),
        };
        let mut m = laptop();
        let vtium = b.paths().vtium;
        let mut out = Vec::new();
        let mut asked = Vec::new();
        let mut ask = |out: &mut dyn io::Write, q: &str| -> bool {
            let _ = writeln!(out, "{q}");
            asked.push(q.to_owned());
            true
        };
        let mut run = runner(&mut m, &vtium);
        let done = desk
            .install(&plan, &mut run, &mut out, &mut ask)
            .expect("installed");
        drop(run);
        match done {
            Done::Installed(opened, outcome) => {
                assert_eq!(outcome.applied, Applied::Done(vec![2, 3, 12]));
                assert_eq!(
                    vts::message(&opened.name, &outcome),
                    "Installed draft bench. vtium now runs with it on VTs 2, 3, and 12."
                );
            }
            other => panic!("{other:?}"),
        }
        let out = String::from_utf8(out).expect("UTF-8");
        let after = out[out.find("A head is").expect("the step after the install")..]
            .replace(&*vtium.to_string_lossy(), VTIUM);
        said.push((m.changes(), after, asked));
    }
    let [cli, tui] = said.as_slice() else {
        panic!("two installs");
    };
    assert_eq!(
        cli.0,
        [
            "root /usr/bin/vtium console enable --direct 12",
            "root systemctl start vtium@tty12",
            "root systemctl restart vtium@tty2 vtium@tty3",
        ]
    );
    assert_eq!(cli, tui);
}

/// A refusal from `vtium console` leaves the install done: the command
/// line's install succeeds, the draft is recorded as installed, and the
/// refusal is on screen.
#[test]
fn a_refused_vt_leaves_the_install_in_place() {
    let b = bench("refused");
    let text = conf("refused").replace("vt = 12", "vt = 7");
    b.draft(&text);
    let paths = b.paths();
    let session = Session {
        store: b.store(),
        paths: &paths,
    };
    let mut m = laptop();
    let mut out = Vec::new();
    let mut run = runner(&mut m, &paths.vtium);
    let done = session.install("bench", &text, Unrouted::Refuse, &mut run, None, &mut out);
    drop(run);
    let out = String::from_utf8(out).expect("UTF-8");
    assert!(done.is_ok(), "{done:?}\n{out}");
    assert!(out.contains("installed draft bench\n"), "{out}");
    assert!(
        out.contains("VT 7 has a graphical session of aaron"),
        "{out}"
    );
    assert!(
        out.contains("Head terminal-1 does not appear until VT 7 is free"),
        "{out}"
    );
}

/// An install into directories that are not the system's, as a test or a
/// trial names them, enables nothing and offers nothing: no vtium on a VT
/// reads that vtium.conf.
#[test]
fn an_install_into_other_directories_leaves_the_vts_alone() {
    let b = bench("other");
    let mut paths = b.paths();
    paths.configdir = b.root.join("etc");
    let session = Session {
        store: b.store(),
        paths: &paths,
    };
    assert_eq!(session.via(), None);
    let mut m = laptop();
    let mut out = Vec::new();
    let mut run = runner(&mut m, &paths.vtium);
    assert_eq!(session.after(CONF, &mut run, None, &mut out), None);
    drop(run);
    assert!(m.ran.is_empty(), "{:?}", m.ran);
    assert!(out.is_empty());
    // the system's directories take sudo's route
    let paths = b.paths();
    let session = Session {
        store: b.store(),
        paths: &paths,
    };
    assert_eq!(session.via(), Some(vec![OsString::from("sudo")]));
}
