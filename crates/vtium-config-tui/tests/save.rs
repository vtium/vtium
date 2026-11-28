//! Save: the review, the screen for a file changed on disk, settling the
//! lines both sides changed, and the privileged step. Each test drives the
//! App with a key, or with what the loop hands back, over the screen map's
//! machine or the large machine, and checks the state, the screen and the
//! effects that come out. One test drives the whole loop over a machine in
//! a folder of its own, with the stand-in privileged step: the binary's
//! `apply DIR`, run as the user into that folder.
//!
//! The screens are kept as snapshots under `tests/snapshots/`, each with
//! four screens: the screen map's machine and the large machine, each at
//! 100×30 and at 80×25.

#![expect(clippy::expect_used, reason = "a test fails by panicking")]

mod common;

use std::ffi::OsString;

use vtium_config_core::document::{Base, Change, Documents, Edit, Installed, OnDisk};
use vtium_config_core::draft::Plan;
use vtium_config_core::drafts::Session;
use vtium_config_core::flowmodel::Present;
use vtium_config_core::places::Paths;
use vtium_config_core::store::Store;
use vtium_config_core::ui::{Effect, Op, Shared};
use vtium_config_tui::BoxTab;
use vtium_config_tui::app::{App, Event, Size};
use vtium_config_tui::live::machine::{Machine as Loop, Ready};
use vtium_config_tui::port::{self, Wake};
use vtium_config_tui::save::review::Review;
use vtium_config_tui::save::{Came, Save, Saved, Screen, privileged};
use vtium_config_tui::sheets::Sheet;
use vtium_config_tui::sheets::problems::{Goes, Problem};
use vtium_config_widgets::frame::{Options, Regions};
use vtium_config_widgets::keys::{Code, Key};

use common::{Log, Machine, SIZES, app, app_mono, draw, keys, plain, press, text};

/// Ctrl+S.
fn ctrl_s() -> Key {
    Key::ctrl('s')
}

/// What the open document counts against, as the loop reads it again.
fn installed(app: &App) -> Installed {
    app.shared().document().installed().clone()
}

/// The installed file as the loop reads it, with `text` as vtium.conf.
fn on_disk(app: &App, text: &str) -> Installed {
    Installed {
        text: Some(text.to_owned()),
        ..installed(app)
    }
}

/// Ctrl+S, then the loop's read of `read`: the review, or the screen for
/// a file changed on disk.
fn open(app: &mut App, read: Installed) {
    assert_eq!(press(app, ctrl_s()), vec![Effect::Desk(Op::Prepare)]);
    app.event(Event::Save(Came::Read(Ok(read))));
}

/// The installed file of `machine` with a change that touches none of the
/// person's lines: the top level's font size.
fn elsewhere(installed: &str) -> String {
    installed.replacen("font-size=12", "font-size=13", 1)
}

/// The installed file of `machine` with a change to the lines the person
/// changed: a size for the look the person sized.
fn same_lines(machine: Machine, installed: &str) -> String {
    match machine {
        Machine::Map => installed.replacen(
            "font-name = proof\n",
            "font-name = proof\nfont-size = 12\n",
            1,
        ),
        Machine::Large => installed.replacen("font-size = 10", "font-size = 12", 1),
    }
}

/// Each screen of `make` on both machines at both sizes, checked for its
/// width and its marks without color, kept as the snapshot `name`.
fn screens(name: &str, make: impl Fn(&mut App, Machine)) {
    let mut out = String::new();
    for machine in Machine::BOTH {
        for size in SIZES {
            let (mut color, _, _) = app(machine, size);
            make(&mut color, machine);
            let (mut mono, _, _) = app_mono(machine, size);
            make(&mut mono, machine);
            let what = format!("{name} on {} at {}x{}", machine.name(), size.0, size.1);
            let (a, b) = (draw(&color), draw(&mono));
            common::rows_fit(&a, &what);
            common::marks_survive_without_color(&a, &b, &what);
            out.push_str(&format!("== {what}\n{}", common::snapshot_text(&b)));
        }
    }
    common::snapshot(name, &out);
}

#[test]
fn ctrl_s_reads_the_installed_file_again_then_opens_the_review() {
    for machine in Machine::BOTH {
        for size in SIZES {
            let (mut app, _, _) = app(machine, size);
            let read = installed(&app);
            open(&mut app, read);
            assert_eq!(app.node(), "any.save");
            let screen = text(&app);
            assert!(screen.contains("Save to /etc/vtium/vtium.conf"), "{screen}");
            assert!(screen.contains("+font-size = 11"), "{screen}");
            assert!(screen.contains("1 change · fonts unchanged"), "{screen}");
            assert!(screen.contains("[Ctrl+S] Save"), "{screen}");
        }
    }
    screens("save-review", |app, _| {
        let read = installed(app);
        open(app, read);
    });
}

#[test]
fn w_shows_the_whole_file_and_esc_goes_back_without_saving() {
    let (mut app, _, _) = app(Machine::Map, (100, 30));
    let read = installed(&app);
    open(&mut app, read);
    keys(&mut app, &["w"]);
    let screen = text(&app);
    assert!(screen.contains("The whole file:"), "{screen}");
    assert!(screen.contains("[head desk]"), "{screen}");
    keys(&mut app, &["w"]);
    assert!(!text(&app).contains("The whole file:"));
    assert!(press(&mut app, plain(Code::Escape)).is_empty());
    assert_eq!(app.node(), "fonts");
    assert_eq!(app.shared().document().count(), 1);
}

#[test]
fn ctrl_s_or_enter_on_the_review_saves_and_ctrl_z_does_nothing_there() {
    let (mut app, fonts, _) = app(Machine::Map, (100, 30));
    // a change of the person's, so Ctrl+Z would have a step to undo
    keys(&mut app, &["Space"]);
    let steps = app.shared().documents.history().to_undo().cloned();
    assert!(steps.is_some());
    let read = installed(&app);
    open(&mut app, read);
    let text_before = app.shared().document().text().to_owned();
    for key in [Key::ctrl('z'), Key::ctrl('y')] {
        assert!(press(&mut app, key).is_empty());
        assert_eq!(app.node(), "any.save");
        assert_eq!(app.shared().document().text(), text_before);
        assert_eq!(app.shared().documents.history().to_undo().cloned(), steps);
    }
    let save = vec![Effect::Install(Plan::default())];
    assert_eq!(press(&mut app, ctrl_s()), save);
    assert!(app.shared().installs.running);
    // Enter saves too
    let (mut app, _, _) = common::app(Machine::Large, (80, 25));
    let read = installed(&app);
    open(&mut app, read);
    assert_eq!(press(&mut app, plain(Code::Enter)), save);
    // no key of the review reached the tab
    assert!(fonts.borrow().iter().all(|a| !a.row.contains("save")));
}

#[test]
fn a_save_that_went_through_moves_the_base_and_the_mark_goes() {
    for machine in Machine::BOTH {
        let (mut app, _, _) = app(machine, (100, 30));
        let read = installed(&app);
        open(&mut app, read);
        press(&mut app, ctrl_s());
        let now = app.shared().document().text().to_owned();
        let saved = Saved {
            installed: on_disk(&app, &now),
            said: "installed draft".to_owned(),
        };
        app.event(Event::Save(Came::Saved(Ok(saved))));
        let shared = app.shared();
        assert_eq!(shared.document().count(), 0);
        assert_eq!(shared.document().content().base, Base::of(Some(&now)));
        assert!(!shared.installs.running);
        assert_eq!(app.node(), "fonts");
        let message = shared.message.as_ref().expect("the done line");
        assert!(
            message.line.starts_with(
                "Saved /etc/vtium/vtium.conf: 1 change. vtium reads it when it starts on a VT"
            ),
            "{}",
            message.line
        );
        assert_eq!(message.more.as_deref(), Some("installed draft"));
        assert!(!text(&app).contains("not saved"));
        assert_eq!(app.farewell(), None);
    }
}

#[test]
fn a_save_that_failed_keeps_the_changes_and_says_why() {
    let (mut app, _, _) = app(Machine::Map, (100, 30));
    let read = installed(&app);
    open(&mut app, read);
    press(&mut app, ctrl_s());
    let why =
        "Nothing installed: sudo refused to run vtium-config apply.\n[sudo] password for user:";
    app.event(Event::Save(Came::Saved(Err(why.to_owned()))));
    assert_eq!(app.shared().document().count(), 1);
    assert_eq!(app.node(), "fonts");
    let message = app.shared().message.as_ref().expect("the line");
    assert_eq!(
        message.line,
        "Nothing saved: Nothing installed: sudo refused to run vtium-config apply. [F1] More"
    );
    assert_eq!(message.more.as_deref(), Some(why));
}

#[test]
fn nothing_to_save_greys_save_and_ctrl_s_says_so() {
    for machine in Machine::BOTH {
        let (mut app, _, _) = app(machine, (100, 30));
        // the installed file now holds the person's change
        let now = app.shared().document().text().to_owned();
        let read = on_disk(&app, &now);
        open(&mut app, read);
        assert_eq!(app.node(), "fonts");
        assert_eq!(app.shared().document().count(), 0);
        assert!(press(&mut app, ctrl_s()).is_empty());
        let message = app.shared().message.as_ref().expect("the line");
        assert_eq!(message.line, "Nothing to save.");
        keys(&mut app, &["Alt-F"]);
        assert!(text(&app).contains("nothing to save"), "{}", text(&app));
    }
    screens("save-nothing", |app, _| {
        let now = app.shared().document().text().to_owned();
        let read = on_disk(app, &now);
        open(app, read);
        keys(app, &["Alt-F"]);
    });
}

#[test]
fn a_file_changed_on_disk_opens_its_screen_and_update_merges_it() {
    for machine in Machine::BOTH {
        let (mut app, _, _) = app(machine, (100, 30));
        let mine = app.shared().document().text().to_owned();
        let disk = elsewhere(&installed(&app).text.expect("installed"));
        let read = on_disk(&app, &disk);
        open(&mut app, read);
        assert_eq!(app.node(), "any.save.changed");
        let screen = text(&app);
        assert!(
            screen.contains("The file changed on disk since you opened it"),
            "{screen}"
        );
        assert!(screen.contains("+font-size=13"), "{screen}");
        assert!(
            screen.contains("Your changes do not touch those lines."),
            "{screen}"
        );
        assert!(screen.contains("[u] Update"), "{screen}");
        keys(&mut app, &["u"]);
        assert_eq!(app.node(), "any.save");
        let merged = app.shared().document().text().to_owned();
        assert_eq!(merged, elsewhere(&mine));
        assert_eq!(
            app.shared().document().content().base,
            Base::of(Some(&disk))
        );
        let history = app.shared().documents.history();
        assert_eq!(
            history.to_undo().map(|s| s.name.as_str()),
            Some("Update from the installed file")
        );
        // Esc, then Ctrl+Z, takes the merge and its base back in one step
        keys(&mut app, &["Esc", "Ctrl-Z"]);
        assert_eq!(app.shared().document().text(), mine);
    }
    screens("save-changed", |app, _| {
        let disk = elsewhere(&installed(app).text.expect("installed"));
        let read = on_disk(app, &disk);
        open(app, read);
    });
}

#[test]
fn lines_both_sides_changed_are_settled_one_at_a_time() {
    for machine in Machine::BOTH {
        let (mut app, _, _) = app(machine, (100, 30));
        let mine = app.shared().document().text().to_owned();
        let disk = same_lines(machine, &installed(&app).text.expect("installed"));
        let read = on_disk(&app, &disk);
        open(&mut app, read);
        assert!(text(&app).contains("touch the same lines in 1 place"));
        keys(&mut app, &["u"]);
        assert_eq!(app.node(), "any.save.settle");
        let screen = text(&app);
        assert!(screen.contains("in 1 place: 0 of 1 settled."), "{screen}");
        assert!(screen.contains("installed"), "{screen}");
        // Esc cancels the update, and the document stays as it was
        keys(&mut app, &["Esc"]);
        assert_eq!(app.node(), "any.save.changed");
        assert_eq!(app.shared().document().text(), mine);
        keys(&mut app, &["u", "m"]);
        assert_eq!(app.node(), "any.save");
        assert!(text(&app).contains("[Ctrl+S] Save"));
        assert_eq!(
            app.shared().document().content().base,
            Base::of(Some(&disk))
        );
        // the one place both changed keeps mine, and nothing else changed
        assert_eq!(app.shared().document().text(), mine);
    }
    screens("save-settle", |app, machine| {
        let disk = same_lines(machine, &installed(app).text.expect("installed"));
        let read = on_disk(app, &disk);
        open(app, read);
        keys(app, &["u"]);
    });
}

#[test]
fn overwrite_records_the_file_on_disk_as_the_base_and_reviews() {
    for machine in Machine::BOTH {
        let (mut app, _, _) = app(machine, (80, 25));
        let mine = app.shared().document().text().to_owned();
        let disk = elsewhere(&installed(&app).text.expect("installed"));
        let read = on_disk(&app, &disk);
        open(&mut app, read);
        keys(&mut app, &["o"]);
        assert_eq!(app.node(), "any.save");
        assert_eq!(app.shared().document().text(), mine);
        assert_eq!(
            app.shared().document().content().base,
            Base::of(Some(&disk))
        );
        let history = app.shared().documents.history();
        assert_eq!(
            history.to_undo().map(|s| s.name.as_str()),
            Some("Overwrite")
        );
        // the review counts the person's text against the file on disk
        assert!(text(&app).contains("-font-size=13"), "{}", text(&app));
        assert_eq!(
            press(&mut app, ctrl_s()),
            vec![Effect::Install(Plan::default())]
        );
    }
}

#[test]
fn revert_asks_then_starts_again_from_the_file_on_disk() {
    let (mut app, _, _) = app(Machine::Map, (100, 30));
    let disk = elsewhere(&installed(&app).text.expect("installed"));
    let read = on_disk(&app, &disk);
    open(&mut app, read);
    keys(&mut app, &["r"]);
    assert_eq!(app.node(), "any.question.confirm");
    assert!(text(&app).contains("Reload the file from disk and drop your changes?"));
    keys(&mut app, &["y"]);
    assert_eq!(app.node(), "fonts");
    assert_eq!(app.shared().document().text(), disk);
    assert_eq!(app.shared().document().count(), 0);
    assert_eq!(
        app.shared().message.as_ref().map(|m| m.line.as_str()),
        Some("Reverted to the file on disk. [Ctrl+Z] brings your changes back.")
    );
}

/// A problem of the check's, for the review.
fn problem(stops: bool, line: &str) -> Problem {
    Problem {
        stops,
        place: "Flow".to_owned(),
        line: line.to_owned(),
        more: None,
        goes: Goes::Item {
            tab: "flow",
            item: None,
        },
    }
}

/// A review of `machine`'s document with `problems`, as the sheet Save
/// opens.
fn review_with(machine: Machine, problems: Vec<Problem>) -> Save {
    let shared = machine.shared();
    let review = Review::of(&shared, None).with_problems(problems);
    Save::new(
        Screen::Review(review),
        shared.document().installed().clone(),
    )
}

#[test]
fn a_problem_that_stops_save_takes_save_off_the_bar() {
    let stop = problem(true, "line 9: palette = solarised needs a built-in palette");
    let warn = problem(false, "1 keyboard and 2 pointers are in no path");
    let save = review_with(Machine::Map, vec![warn.clone(), stop]);
    let Screen::Review(review) = save.screen() else {
        panic!("the review");
    };
    assert!(review.stops());
    assert!(!review.no_route_only());
    assert!(save.bar().buttons.iter().all(|b| b.key != "Ctrl+S"));
    let ok = review_with(Machine::Map, vec![warn]);
    assert!(ok.bar().buttons.iter().any(|b| b.key == "Ctrl+S"));
    // the check's own words for a file with no complete route
    let missing = vtium_settings::Missing::NoDisplay {
        display: vtium_settings::Quoted::path(std::path::Path::new("DP-8")),
        rule: Some(3),
    };
    let no_route = problem(true, &missing.to_string());
    let Screen::Review(r) = review_with(Machine::Map, vec![no_route]).screen().clone() else {
        panic!("the review");
    };
    assert!(r.no_route_only());
    // the sheet alone, as it draws over the viewer
    let mut out = String::new();
    for machine in Machine::BOTH {
        for (cols, rows) in SIZES {
            let save = review_with(
                machine,
                vec![
                    problem(false, "1 keyboard and 2 pointers are in no path"),
                    problem(true, "line 9: palette = solarised needs a built-in palette"),
                ],
            );
            let sheet = Sheet::Save(Box::new(save));
            let area = ratatui_core::layout::Rect::new(0, 0, cols, rows);
            let regions = Regions::of(area, Options::default());
            let mut buf = ratatui_core::buffer::Buffer::empty(area);
            sheet.draw(&mut buf, &sheet.place(&regions), false);
            let what = format!("save-stopped on {} at {cols}x{rows}", machine.name());
            common::rows_fit(&buf, &what);
            let screen = common::rows(&buf).join("\n");
            assert!(screen.contains("1 problem stops Save"), "{screen}");
            out.push_str(&format!("== {what}\n{}", common::snapshot_text(&buf)));
        }
    }
    common::snapshot("save-stopped", &out);
}

/// The test machine's installed file, before anything changes.
const BASE: &str = "font-size=12\nversion = 1\n[look big]\nfont-name = proof\nfont-size = 10\n[head desk]\nlook = big\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n";

/// The person's change: both sizes.
const MINE: &str = "font-size=14\nversion = 1\n[look big]\nfont-name = proof\nfont-size = 11\n[head desk]\nlook = big\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n";

/// The change made on disk meanwhile: both sizes, otherwise.
const DISK: &str = "font-size=13\nversion = 1\n[look big]\nfont-name = proof\nfont-size = 12\n[head desk]\nlook = big\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n";

/// Mine at the first place, the installed file's at the second.
const SETTLED: &str = "font-size=14\nversion = 1\n[look big]\nfont-name = proof\nfont-size = 12\n[head desk]\nlook = big\n[rules]\ndisplay * -> desk\ninput class:keyboard -> roaming\n";

/// A machine in the test's own folder, with `BASE` installed, a stand-in
/// console whose check passes every file, and an empty store.
fn folder(test: &str) -> (Paths, Store) {
    let root = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("save-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let etc = root.join("etc");
    std::fs::create_dir_all(&etc).expect("the folder");
    std::fs::write(etc.join("vtium.conf"), BASE).expect("vtium.conf");
    let vtium = root.join("vtium");
    std::fs::write(&vtium, "#!/bin/sh\nexit 0\n").expect("the stand-in console");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&vtium, std::fs::Permissions::from_mode(0o755)).expect("its mode");
    let paths = Paths {
        configdir: etc,
        frits: Err("no font sets on this machine".to_owned()),
        drafts: root.join("bundles"),
        sudo: "sudo".into(),
        vtium: vtium.into(),
        helper: vtium_config_core::specimen::Helper::at(root.join("no-helper")),
        sys: root.join("sys"),
    };
    let store = Store {
        config: root.join("config"),
        state: root.join("state"),
    };
    (paths, store)
}

/// The machine of a folder: it reads the installed file there, and saves
/// through the stand-in privileged step, which runs `apply DIR` as the
/// binary runs it, as the user, into the folder.
struct Folder {
    paths: Paths,
    store: Store,
    /// Each command the step ran.
    ran: Vec<Vec<OsString>>,
}

impl Folder {
    fn session(&self) -> Session<'_> {
        Session {
            store: self.store.clone(),
            paths: &self.paths,
        }
    }
}

impl Loop for Folder {
    fn devices(&mut self) -> Option<Present> {
        None
    }

    fn ready_font(&mut self, _source: &std::path::Path) -> Result<Ready, String> {
        Err("no fonts here".to_owned())
    }

    fn install_font(&mut self) -> Result<(), String> {
        Err("no fonts here".to_owned())
    }

    fn installed(&mut self) -> Result<Installed, String> {
        privileged::read(&self.session())
    }

    fn save(&mut self, name: &str, plan: &Plan) -> Result<Saved, String> {
        let targets = self.paths.targets();
        let helper = self.paths.helper.clone();
        let mut ran = Vec::new();
        let mut run = |argv: &[OsString]| -> std::io::Result<(bool, String)> {
            ran.push(argv.to_vec());
            let dir = argv.last().map(|d| d.to_string_lossy().into_owned());
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let status = vtium_config_core::apply::command(
                &dir.into_iter().collect::<Vec<_>>(),
                &targets,
                &helper,
                &mut out,
                &mut err,
            );
            out.extend(err);
            Ok((status == 0, String::from_utf8_lossy(&out).into_owned()))
        };
        let mut ask = |_: &mut dyn std::io::Write, _: &str| false;
        let mut said = Vec::new();
        let session = Session {
            store: self.store.clone(),
            paths: &self.paths,
        };
        let done = privileged::step(&session, name, plan, &mut run, &mut ask, &mut said);
        self.ran.extend(ran);
        let said = String::from_utf8_lossy(&said).into_owned();
        done.map(|installed| Saved { installed, said })
    }
}

#[test]
fn save_runs_the_stand_in_step_after_update_settled_mine_then_installed() {
    let (paths, store) = folder("update");
    let session = Session {
        store: store.clone(),
        paths: &paths,
    };
    let document = OnDisk::open(&session).expect("opened");
    let name = document.name().to_owned();
    let mut documents = Documents::new(document, Box::new(OnDisk::new(store.clone())));
    documents
        .change(Change::new("both sizes", Edit::Text(MINE.to_owned())))
        .expect("changed");
    std::fs::write(session.installed_path(), DISK).expect("the change on disk");
    let shared = Shared::new(documents, Present::default());
    let tabs: Vec<BoxTab> = vec![Box::new(common::Looks {
        log: Log::default(),
    })];
    let app = App::new(tabs, shared, Size::grid(100, 30), false);
    let wake = |bytes: &[u8]| vec![Wake::Bytes(bytes.to_vec())];
    // Ctrl+S, Update, mine at the first place, installed at the second,
    // then the review's Ctrl+S saves
    let mut script = common::Script::new(
        Size::grid(100, 30),
        vec![
            wake(b"\x13"),
            wake(b"u"),
            wake(b"m"),
            wake(b"i"),
            wake(b"\x13"),
        ],
    );
    let mut machine = Folder {
        paths: paths.clone(),
        store: store.clone(),
        ran: Vec::new(),
    };
    let left = port::drive_on(&mut script, &mut machine, app).expect("the loop");
    assert_eq!(script.asides, 1, "sudo's step runs once, stepped aside");
    assert_eq!(machine.ran.len(), 1, "{:?}", machine.ran);
    let apply = machine.ran.first().expect("the step");
    assert!(apply.iter().any(|a| a == "apply"), "{apply:?}");
    let now = std::fs::read_to_string(session.installed_path()).expect("installed");
    assert_eq!(now, SETTLED);
    // the base moved to the file the save installed, and ● went
    assert_eq!(
        store.base(&name).expect("the base"),
        Some(Some(vtium_config_core::store::hash(SETTLED.as_bytes())))
    );
    assert_eq!(left.line, None);
    // History keeps the file the save replaced
    let history = store.history().expect("the history");
    assert_eq!(history.len(), 1);
    let kept = std::fs::read_to_string(&history.first().expect("one").path).expect("kept");
    assert_eq!(kept, DISK);
}

#[test]
fn the_step_refuses_a_file_that_changed_on_disk_after_the_review() {
    let (paths, store) = folder("refuse");
    let session = Session {
        store: store.clone(),
        paths: &paths,
    };
    let document = OnDisk::open(&session).expect("opened");
    let name = document.name().to_owned();
    let mut documents = Documents::new(document, Box::new(OnDisk::new(store.clone())));
    documents
        .change(Change::new("both sizes", Edit::Text(MINE.to_owned())))
        .expect("changed");
    // the file changes on disk after the review showed it
    std::fs::write(session.installed_path(), DISK).expect("the change on disk");
    let mut ran = 0;
    let mut run = |_: &[OsString]| -> std::io::Result<(bool, String)> {
        ran += 1;
        Ok((true, String::new()))
    };
    let mut ask = |_: &mut dyn std::io::Write, _: &str| false;
    let mut said = Vec::new();
    let done = privileged::step(
        &session,
        &name,
        &Plan::default(),
        &mut run,
        &mut ask,
        &mut said,
    );
    let why = done.expect_err("refused");
    assert!(why.contains("changed after draft"), "{why}");
    assert_eq!(ran, 0, "nothing reached sudo");
    let now = std::fs::read_to_string(session.installed_path()).expect("installed");
    assert_eq!(now, DISK);
    let said = String::from_utf8_lossy(&said);
    assert!(
        said.starts_with(&format!("vtium-config is saving {name} to ")),
        "{said}"
    );
}
