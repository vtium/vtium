//! A font set made in the TUI waits in the draft's folder, so leaving
//! vtium-config before Install loses nothing, and `q` asks no question
//! about it: the TUI runs on the real desk, a store under a home of the
//! test's own, and the stand-in console `vtium`, whose check refuses each
//! line that holds `BAD`.
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

/// The words of the columns left of the preview, so the sentences under
/// the list read whole where their rows run beside the preview's text.
fn left_words(screen: &str) -> String {
    let cols = vtium_config::tui::layout::PICTURE_COL - 2;
    let left: Vec<String> = screen
        .lines()
        .map(|l| l.chars().take(cols).collect())
        .collect();
    words(&left.join("\n"))
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

/// A font set that only the draft adds shows in the TUI before Install:
/// the Looks list lists it, marked as in the draft and not yet installed,
/// with its detail, and `t` says it can be tried once installed. A look's
/// card offers it among the font sets, so the person can pick it, and the
/// card then shows it without the note that this machine has no font set
/// of that name. `r` on it takes it back out of the draft, and the list
/// no longer holds it.
///
/// References:
/// - ADR-1403 D8, "The frits a draft sees": the views list the folder's frits, marked, and a look card offers them.
/// - ADR-1403 D8, "Every font set change is kept at once": `r` on a frit only the draft adds deletes its file.
/// - ADR-1403 D8, "The live trial": `t` on a frit only the draft holds.
#[test]
fn a_font_set_only_the_draft_adds_shows_in_the_list_and_the_card() {
    let m = machine("only-drafted");
    // the new-frit form makes quiet, as in the test above
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
    ];
    let screen = m.run(made);
    // the check judges the draft with quiet in it, and finds nothing
    let status = screen.lines().nth(1).unwrap_or_default();
    assert!(status.contains("no problems"), "{screen}");
    // a font set's row in the list, indented under its group's heading
    let row = |screen: &str| screen.lines().any(|l| l.starts_with("   quiet "));
    assert!(row(&screen), "{screen}");
    // the look big, the six font sets the machine has, then quiet
    let to_quiet: Vec<&[u8]> = std::iter::repeat_n(DOWN, 7).collect();
    // the sentence under the list wraps beside the preview's text
    let screen = m.run(&to_quiet);
    let said = left_words(&screen);
    assert!(
        said.contains("quiet: reads, in the") && said.contains("draft, not yet installed."),
        "{said}"
    );
    // the caption and the status line say that t can try quiet once the
    // draft is installed, and do not offer t now
    assert!(
        said.contains(
            "The console draws this in its current font. The font set quiet is in the draft, so t can try it once you install the draft with i. A size takes effect when vtium starts again."
        ),
        "{said}"
    );
    assert!(!said.contains("Press t"), "{said}");
    let status = screen.lines().nth(1).unwrap_or_default();
    assert!(
        status.contains("t: after install") && !status.contains("try live"),
        "{screen}"
    );
    // and so does p's picture of the console
    let said = words(&m.run(&[&to_quiet[..], &[b"p".as_slice()]].concat()));
    assert!(
        said.contains(
            "The font set quiet is in the draft, so t can try it once you install the draft with i."
        ),
        "{said}"
    );
    assert!(!said.contains("Press t"), "{said}");
    let said = words(&m.run(&[&to_quiet[..], &[ENTER]].concat()));
    assert!(said.contains("Font set quiet"), "{said}");
    assert!(
        said.contains("The draft adds this font set, and this machine does not have it yet."),
        "{said}"
    );
    let said = words(&m.run(&[&to_quiet[..], &[b"t".as_slice()]].concat()));
    assert!(
        said.contains(
            "The font set quiet is in the draft and not yet installed, so t cannot try it."
        ),
        "{said}"
    );
    // the card of big offers quiet on its font set line, and Enter picks
    // it; the signal that ends the run saves the change
    let card: &[&[u8]] = &[ENTER, DOWN, ENTER];
    let said = words(&m.run(card));
    assert!(
        said.contains("○ quiet in the draft, not yet installed"),
        "{said}"
    );
    m.run(&[card, &[END, ENTER]].concat());
    let draft = std::fs::read_to_string(m.root.join("home/config/vtium/drafts/mine.conf"))
        .expect("the draft");
    assert!(draft.contains("font-name = quiet"), "{draft}");
    let said = words(&m.run(card));
    assert!(
        said.contains("● quiet in the draft, not yet installed"),
        "{said}"
    );
    assert!(!said.contains("no font set of that name"), "{said}");
    // r takes it back out of the draft, and the list no longer holds it
    // Up draws the screen once more after the desk's answer
    let screen = m.run(&[&to_quiet[..], &[b"r".as_slice(), UP]].concat());
    assert!(
        words(&screen).contains("The draft no longer adds quiet."),
        "{screen}"
    );
    let folder = m.store().frit_changes("mine").expect("the folder");
    assert!(folder.kept.is_empty(), "{folder:?}");
    let screen = m.run(&[]);
    assert!(!row(&screen), "{screen}");
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

/// The check judges a draft that brings a font set change the way the
/// console's `vtium check-config` does: a line it refuses, in the draft's
/// vtium.conf or in the font set's file the draft adds, is one problem
/// on the status line, and the options that pass the font set change
/// count as none.
///
/// References:
/// - ADR-1403 D8, "A draft's font set changes": the check passes one `--with-frit` for each frit the folder adds, so it reads the draft's file.
#[test]
fn the_check_reports_a_bad_line_in_a_draft_that_adds_a_font_set() {
    let m = machine("bad-with-frit");
    m.run(&[]);
    let store = m.store();
    store
        .keep_frit("mine", "quiet", "source=builtin:unscii-8\n")
        .expect("a new frit");
    let status = |m: &Machine| m.run(&[]).lines().nth(1).unwrap_or_default().to_owned();
    let shown = status(&m);
    assert!(shown.contains("no problems"), "{shown}");
    // a line the check refuses in the draft's vtium.conf
    let path = store.draft_path("mine").expect("the draft's path");
    let draft = std::fs::read_to_string(&path).expect("the draft");
    std::fs::write(&path, format!("{draft}BAD = 1\n")).expect("a bad line");
    let shown = status(&m);
    assert!(shown.contains(" 1 problem "), "{shown}");
    // and one in the font set's file the draft adds
    std::fs::write(&path, &draft).expect("the draft as it was");
    store
        .keep_frit("mine", "quiet", "source=builtin:unscii-8\nBAD\n")
        .expect("a bad frit");
    let shown = status(&m);
    assert!(shown.contains(" 1 problem "), "{shown}");
}

/// The stand-in console reads its arguments as `vtium check-config` does:
/// options in any order and in either form, the file to check as the one
/// word that is no option, and a `--with-frit` that names no regular file
/// called `NAME.vtfrit` refused, as the real check refuses it.
#[test]
fn the_stand_in_console_reads_its_arguments_as_the_console_does() {
    let m = machine("stand-in");
    let etc = m.root.join("etc");
    let conf = etc.join("vtium.conf");
    std::fs::write(etc.join("fonts/quiet.vtfrit"), "source=builtin:unscii-8\n").expect("a frit");
    let check = |args: &[&std::ffi::OsStr]| {
        let out = Command::new(m.root.join("vtium"))
            .arg("check-config")
            .args(args)
            .output()
            .expect("the stand-in runs");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let frit = etc.join("fonts/quiet.vtfrit");
    let (status, out, err) = check(&[
        "--without-frit".as_ref(),
        "gone".as_ref(),
        "--configdir".as_ref(),
        etc.as_os_str(),
        "--with-frit".as_ref(),
        frit.as_os_str(),
        "--no-devices".as_ref(),
        conf.as_os_str(),
        "--without-frit=old".as_ref(),
    ]);
    assert_eq!(status, Some(0), "{err}");
    assert_eq!(out, format!("{}: checked\n", conf.display()));
    // without a file, the configuration directory's vtium.conf
    let (status, out, _) = check(&["--configdir".as_ref(), etc.as_os_str()]);
    assert_eq!(
        (status, out),
        (Some(0), format!("{}: checked\n", conf.display()))
    );
    // a missing font set file, and one not named NAME.vtfrit
    let missing = etc.join("fonts/missing.vtfrit");
    let (status, _, err) = check(&[
        "--configdir".as_ref(),
        etc.as_os_str(),
        "--with-frit".as_ref(),
        missing.as_os_str(),
        format!("--with-frit={}", conf.display()).as_ref(),
        conf.as_os_str(),
    ]);
    assert_eq!(status, Some(1), "{err}");
    assert_eq!(err.matches("is not a font set file").count(), 2, "{err}");
}
