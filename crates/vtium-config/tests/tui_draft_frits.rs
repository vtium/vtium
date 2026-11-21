//! A font set made in the TUI waits in the draft's folder, so leaving
//! vtium-config before Install loses nothing, and `q` asks no question
//! about it: the TUI runs on the real desk, a store under a home of the
//! test's own, and the stand-in console `vtium`, whose check passes every
//! file.
//!
//! References:
//! - ADR-1403 D8, "A draft's font set changes": the folder, that quitting
//!   loses nothing, and that the TUI asks nothing about the folder.

mod tui_common;
use tui_common::*;

use vtium_testscript as script;

use std::path::PathBuf;
use std::process::{Command, Stdio};

use vtium_config::tui::desk;
use vtium_config::tui::fixture::{CHECKER, DRAFT as INSTALLED};
use vtium_config::tui::port::drive_with;

const BIN: &str = env!("CARGO_BIN_EXE_vtium-config");

/// The keys of Enter, End and Down.
const ENTER: &[u8] = b"\r";
const END: &[u8] = b"\x1b[F";

/// A change that waits for Save: `4` opens the Flow view, and the first
/// path's terminal, desk, is renamed deskx on its card.
const CHANGE: [&[u8]; 8] = [
    b"4", b"\x1b[H", b"\x1b[C", ENTER, ENTER, b"x", ENTER, b"\x1b",
];

/// A machine of the test's own: the installed vtium.conf under `etc`, an
/// empty frits' directory, the stand-in console, and a home whose store
/// holds the drafts.
struct Machine {
    root: PathBuf,
}

fn machine(name: &str) -> Machine {
    let home = home::vars()
        .into_iter()
        .find(|(k, _)| *k == "HOME")
        .map(|(_, v)| v)
        .expect("the tests' home");
    let root = home.join("tui-draft-frits").join(name);
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("a fresh fixture");
    }
    for dir in ["etc/fonts", "home", "sys", "udev/data"] {
        std::fs::create_dir_all(root.join(dir)).expect("the fixture");
    }
    std::fs::write(root.join("etc/vtium.conf"), INSTALLED).expect("vtium.conf");
    script::executable(&root.join("vtium"), CHECKER);
    Machine { root }
}

impl Machine {
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
        vtium_config::tui::Paths {
            configdir: self.root.join("etc"),
            frits: Ok(self.root.join("etc/fonts")),
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

    /// Starts the TUI in the Looks tab of a vtium console, on the real
    /// desk, typing `keys`, and gives back the last screen. The draft is
    /// `mine`, made from the installed file the first time.
    fn run(&self, keys: &[&[u8]]) -> String {
        let paths = self.paths();
        let store = self.store();
        if store.settings().expect("the settings").draft.is_none() {
            vtium_config::drafts::Session {
                store: store.clone(),
                paths: &paths,
            }
            .make("mine", vtium_config::drafts::From::Installed)
            .expect("the draft mine");
        }
        let mut desk = desk::Store::new(store, paths, Box::new(Default::default));
        let mut console = Console::vtium(100, 30, true, "unifont")
            .takes("unifont", (100, 30))
            .keys(keys);
        let mut i = inputs();
        i.view = vtium_config::tui::model::View::Looks;
        i.flow = vtium_config::tui::flow::Flow::default();
        drive_with(&mut console, i, Some(&mut desk)).expect("the loop");
        console.seen.clone().unwrap_or_default()
    }

    /// `vtium-config ARGS` on this machine, as a person runs it: its
    /// standard output.
    fn command(&self, args: &[&str]) -> String {
        let out = Command::new(BIN)
            .arg("--configdir")
            .arg(self.root.join("etc"))
            .arg("--frits")
            .arg(self.root.join("etc/fonts"))
            .arg("--vtium")
            .arg(self.root.join("vtium"))
            .arg("--sys")
            .arg(self.root.join("sys"))
            .arg("--udev-data")
            .arg(self.root.join("udev/data"))
            .args(args)
            .env("HOME", self.root.join("home"))
            .env("XDG_CONFIG_HOME", self.root.join("home/config"))
            .env("XDG_STATE_HOME", self.root.join("home/state"))
            .stdin(Stdio::null())
            .output()
            .expect("the binary runs");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

/// The screen's words with one space between each, so a message reads
/// whole wherever its rows wrap.
fn words(screen: &str) -> String {
    screen.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A font set made with the new-frit form survives leaving vtium-config
/// before Install: `q` leaves at once, the draft's folder holds the font
/// set, `vtium config diff` shows it, and the TUI started again finds it
/// waiting for the install. With a change that waits for Save, `q` asks
/// only the quit question, and its `n` leaves the folder as it was.
#[test]
fn a_font_set_made_in_the_tui_survives_leaving_before_install() {
    let m = machine("survives");
    // from the look big, Down to the first font set, n opens the form,
    // Other… on the text font types builtin:unscii-8, Down five times
    // reaches the name, which is typed and kept, Enter on the add row adds
    // the font set, and q leaves
    let made: &[&[u8]] = &[
        DOWN,
        b"n",
        ENTER,
        END,
        ENTER,
        b"builtin:unscii-8",
        ENTER,
        DOWN,
        DOWN,
        DOWN,
        DOWN,
        DOWN,
        ENTER,
        b"quiet",
        ENTER,
        ENTER,
        b"q",
    ];
    // the TUI leaves before the keys run out, so it draws no last screen
    // for the test to read; a question would have kept it open
    let screen = m.run(made);
    assert_eq!(screen, "", "q leaves at once");
    let text = "# quiet: made by vtium-config new-frit\nname=quiet\nsource=builtin:unscii-8\n";
    let kept = |m: &Machine| {
        let folder = m.store().frit_changes("mine").expect("the folder");
        assert_eq!(
            folder.kept.get("quiet").map(String::as_str),
            Some(text),
            "{folder:?}"
        );
        assert!(folder.removed.is_empty(), "{folder:?}");
        assert!(!m.root.join("etc/fonts/quiet.vtfrit").exists());
    };
    kept(&m);
    let diff = m.command(&["diff"]);
    assert!(diff.contains("quiet"), "{diff}");
    // started again, the TUI counts the font set that waits, and q with
    // nothing unsaved still leaves at once
    let screen = m.run(&[]);
    let status = screen.lines().nth(1).unwrap_or_default();
    assert!(status.contains("1 font set to install"), "{screen}");
    assert_eq!(m.run(&[b"q"]), "", "q leaves at once");
    // with a change that waits for Save, q then n quits without the
    // change, and the folder still holds the font set
    m.run(&[&CHANGE[..], &[b"q", b"n"]].concat());
    kept(&m);
    let diff = m.command(&["diff"]);
    assert!(diff.contains("quiet"), "{diff}");
    assert!(!diff.contains("deskx"), "{diff}");
    // and q asks the quit question alone; the keys then run out, and the
    // signal that ends the test saves the change, so this runs last
    let screen = m.run(&[&CHANGE[..], &[b"q"]].concat());
    let said = words(&screen);
    assert!(
        said.contains(
            "1 change is not saved. Save them before you quit? y saves them and quits, n quits without them, and Esc goes back."
        ),
        "{screen}"
    );
    assert!(!said.contains("Install before you quit?"), "{screen}");
    assert!(!said.contains("The draft keeps"), "{screen}");
}

/// The status line counts the font set changes an install makes, the
/// count `vtium config flow` gives: a font set in the draft's folder whose
/// text is the installed one's, and a removal of a font set that only a
/// category folder of the catalog holds, are no change.
///
/// References:
/// - ADR-1403 D8, "A draft's font set changes": a frit with the installed text is no change, and a removal of a frit the machine lacks removes nothing.
#[test]
fn the_status_line_counts_only_the_changes_an_install_makes() {
    let m = machine("status-count");
    let text = "source=builtin:unifont\n";
    std::fs::write(m.root.join("etc/fonts/same.vtfrit"), text).expect("a frit");
    std::fs::create_dir_all(m.root.join("etc/fonts/console")).expect("a category folder");
    std::fs::write(m.root.join("etc/fonts/console/cat.vtfrit"), text).expect("a set");
    m.run(&[]);
    let store = m.store();
    store
        .keep_frit("mine", "same", text)
        .expect("the same text");
    store
        .keep_removal("mine", "cat")
        .expect("a removal of the catalog's set");
    let status = |m: &Machine| m.run(&[]).lines().nth(1).unwrap_or_default().to_owned();
    let first = m.command(&["flow"]);
    assert!(first.contains("same as installed"), "{first}");
    let shown = status(&m);
    assert!(!shown.contains("to install"), "{shown}");
    // a real change counts
    store.keep_frit("mine", "new", text).expect("a new frit");
    let shown = status(&m);
    assert!(shown.contains("1 font set to install"), "{shown}");
}
