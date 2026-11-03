//! `apply DIR` and the draft it installs (ADR-1201 D5, D8), in a temporary
//! directory standing in for `/etc/vtium` through the tool's path options,
//! never as root. The edited vtium.conf and head file are judged by the
//! console's own `vtium check-config`, the binary this workspace builds.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use vtium_config::apply::{Route, Targets, route, save_argv, trusted_owners, write_draft};

#[path = "common/console.rs"]
mod console;
use console::vtium;
use vtium_config::draft::{FileEdit, Plan};

/// A fixture: a configuration directory with `conf` as vtium.conf, a
/// frits directory, and a drafts directory, all of the test's own.
struct Fixture {
    root: PathBuf,
    targets: Targets,
}

fn fixture(name: &str, conf: Option<&str>) -> Fixture {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("apply")
        .join(name);
    if root.exists() {
        let _ = std::fs::set_permissions(root.join("etc"), std::fs::Permissions::from_mode(0o755));
        std::fs::remove_dir_all(&root).expect("a fresh fixture");
    }
    let etc = root.join("etc");
    let fonts = etc.join("fonts");
    std::fs::create_dir_all(&fonts).expect("the fixture");
    std::fs::write(fonts.join("tiny.vtfrit"), "source=builtin:unscii-8\n").expect("a frit");
    if let Some(conf) = conf {
        std::fs::write(etc.join("vtium.conf"), conf).expect("vtium.conf");
    }
    Fixture {
        targets: Targets {
            configdir: etc,
            frits: fonts,
            vtium: vtium().into(),
        },
        root,
    }
}

impl Fixture {
    fn conf_path(&self) -> PathBuf {
        self.targets.configdir.join("vtium.conf")
    }

    fn conf(&self) -> String {
        std::fs::read_to_string(self.conf_path()).expect("vtium.conf")
    }

    /// A draft of `plan` under the fixture's drafts directory.
    fn draft(&self, plan: &Plan) -> PathBuf {
        write_draft(&self.root.join("drafts"), plan, &self.targets).expect("the draft")
    }

    /// `vtium-config apply DIR` with the fixture's path options.
    fn apply(&self, draft: &Path) -> (i32, String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_vtium-config"))
            .args(self.targets.options())
            .arg("apply")
            .arg(draft)
            .output()
            .expect("the binary runs");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    /// The plan that replaces vtium.conf with `new`.
    fn conf_plan(&self, new: &str) -> Plan {
        Plan {
            files: vec![FileEdit {
                path: self.conf_path(),
                old: std::fs::read_to_string(self.conf_path()).ok(),
                new: new.to_owned(),
            }],
        }
    }
}

/// The shipped example, with the fixture's frits directory set so the
/// check finds its frits.
fn example(fonts: &Path) -> String {
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vtium/install/vtium.conf.example");
    let text = std::fs::read_to_string(shipped).expect("the shipped example");
    vtium_settings::edit_conf(
        &text,
        &[vtium_settings::Edit::Set(
            "font-frits",
            fonts.to_str().expect("a path"),
        )],
    )
    .expect("the edit")
}

#[test]
fn a_new_font_size_installs_with_one_line_changed_and_the_old_file_kept() {
    let f = fixture("one-line", None);
    let before = example(&f.targets.frits);
    std::fs::write(f.conf_path(), &before).expect("vtium.conf");
    let after = vtium_settings::edit_conf(&before, &[vtium_settings::Edit::Set("font-size", "12")])
        .expect("the edit");
    let draft = f.draft(&f.conf_plan(&after));
    let (status, out, err) = f.apply(&draft);
    assert_eq!(status, 0, "{out}{err}");
    assert_eq!(out, format!("wrote\t{}\n", f.conf_path().display()));
    assert_eq!(f.conf(), after);
    let lines = vtium_config::draft::changes(&before, &f.conf());
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].old, None);
    assert_eq!(lines[0].new.as_deref(), Some("font-size=12"));
    assert_eq!(
        std::fs::read_to_string(f.targets.configdir.join("vtium.conf.bak")).expect("the old file"),
        before
    );
    assert_eq!(
        std::fs::metadata(f.conf_path())
            .expect("its mode")
            .permissions()
            .mode()
            & 0o777,
        0o644
    );
    // the draft stays, so it can be applied again
    assert!(draft.join("vtium.conf").is_file());
}

#[test]
fn a_draft_whose_target_changed_since_is_refused_with_nothing_written() {
    let f = fixture("changed", Some("font-size=10\n"));
    let draft = f.draft(&f.conf_plan("font-size=12\n"));
    std::fs::write(f.conf_path(), "font-size=11\n").expect("an edit by hand meanwhile");
    let (status, out, err) = f.apply(&draft);
    assert_eq!(status, 1, "{out}{err}");
    assert!(err.contains("changed since the draft was made"), "{err}");
    assert!(err.contains(&f.conf_path().display().to_string()), "{err}");
    assert_eq!(f.conf(), "font-size=11\n");
    assert!(!f.targets.configdir.join("vtium.conf.bak").exists());
}

#[test]
fn a_file_that_fails_its_check_stops_the_apply_before_anything_is_written() {
    let f = fixture("check", Some("font-size=10\n"));
    let heads = f.targets.configdir.join("heads.conf");
    let plan = Plan {
        files: vec![
            FileEdit {
                path: heads.clone(),
                old: None,
                new: "version = 1
[head desk]
profile = tiny
[rules]
display * -> desk
"
                .to_owned(),
            },
            FileEdit {
                path: f.conf_path(),
                old: Some("font-size=10\n".into()),
                new: "font-size=lots\n".into(),
            },
        ],
    };
    let draft = f.draft(&plan);
    let (status, out, err) = f.apply(&draft);
    assert_eq!(status, 1, "{out}{err}");
    assert!(
        err.contains("vtium.conf line 1") && err.contains("font-size"),
        "{err}"
    );
    assert!(
        !err.contains("lots"),
        "root quotes nothing it read for the user: {err}"
    );
    assert_eq!(f.conf(), "font-size=10\n");
    assert!(!heads.exists(), "nothing is written when any file fails");
}

#[test]
fn a_head_file_with_errors_is_refused() {
    let f = fixture(
        "heads",
        Some(&format!("font-frits={}\n", fixture_fonts("heads"))),
    );
    let heads = f.targets.configdir.join("heads.conf");
    let draft = f.draft(&Plan {
        files: vec![FileEdit {
            path: heads.clone(),
            old: None,
            new: "[head desk]\n".into(),
        }],
    });
    let (status, out, err) = f.apply(&draft);
    assert_eq!(status, 1, "{out}{err}");
    assert!(!heads.exists());
}

fn fixture_fonts(name: &str) -> String {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("apply")
        .join(name)
        .join("etc/fonts")
        .display()
        .to_string()
}

#[test]
fn a_head_file_and_vtium_conf_install_head_file_first() {
    let f = fixture(
        "both",
        Some(&format!("font-frits={}\n", fixture_fonts("both"))),
    );
    let heads = f.targets.configdir.join("heads.conf");
    let conf = format!("font-frits={}\nfont-name=tiny\n", fixture_fonts("both"));
    let plan = Plan {
        files: vec![
            FileEdit {
                path: f.conf_path(),
                old: Some(f.conf()),
                new: conf.clone(),
            },
            FileEdit {
                path: heads.clone(),
                old: None,
                new: "version = 1\n[head desk]\nprofile = tiny\n[rules]\ndisplay * -> desk\ninput * -> desk\n".into(),
            },
        ],
    };
    let draft = f.draft(&plan);
    let (status, out, err) = f.apply(&draft);
    assert_eq!(status, 0, "{out}{err}");
    assert_eq!(
        out,
        format!(
            "wrote\t{}\nwrote\t{}\n",
            heads.display(),
            f.conf_path().display()
        )
    );
    assert_eq!(f.conf(), conf);
}

#[test]
fn a_write_that_fails_before_its_rename_leaves_the_old_file_whole() {
    let f = fixture("readonly", Some("font-size=10\n"));
    let draft = f.draft(&f.conf_plan("font-size=12\n"));
    std::fs::set_permissions(&f.targets.configdir, std::fs::Permissions::from_mode(0o555))
        .expect("read-only");
    let (status, out, err) = f.apply(&draft);
    std::fs::set_permissions(&f.targets.configdir, std::fs::Permissions::from_mode(0o755))
        .expect("writable");
    assert_eq!(status, 1, "{out}{err}");
    assert!(
        out.starts_with(&format!("failed\t{}\t", f.conf_path().display())),
        "{out}"
    );
    assert_eq!(f.conf(), "font-size=10\n");
    let left: Vec<_> = std::fs::read_dir(&f.targets.configdir)
        .expect("the directory")
        .map(|e| e.expect("an entry").file_name())
        .collect();
    assert_eq!(
        left.len(),
        2,
        "vtium.conf and fonts, nothing half written: {left:?}"
    );
}

#[test]
fn a_draft_holding_a_link_or_a_file_it_does_not_know_is_refused() {
    let f = fixture("symbolic", Some("font-size=10\n"));
    let draft = f.draft(&f.conf_plan("font-size=12\n"));
    // a valid vtium.conf of the user's own outside the draft, so only the
    // refusal to follow a link stops it
    let elsewhere = f.root.join("elsewhere.conf");
    std::fs::write(&elsewhere, "font-size=12\n").expect("the file");
    let file = draft.join("vtium.conf");
    std::fs::remove_file(&file).expect("the draft's file");
    std::os::unix::fs::symlink(&elsewhere, &file).expect("a link");
    let (status, out, err) = f.apply(&draft);
    assert_eq!(status, 1, "{out}{err}");
    assert!(err.contains("vtium.conf") && err.contains("link"), "{err}");
    assert!(out.is_empty(), "nothing is written: {out}");
    assert_eq!(f.conf(), "font-size=10\n");

    let draft = f.draft(&f.conf_plan("font-size=12\n"));
    let manifest = draft.join("MANIFEST");
    let text = std::fs::read_to_string(&manifest).expect("the manifest");
    std::fs::write(&manifest, text.replace("vtium.conf", "../../evil")).expect("a bad name");
    let (status, out, err) = f.apply(&draft);
    assert_eq!(status, 1, "{out}{err}");
    assert!(err.contains("MANIFEST line 1"), "{err}");
    assert!(
        !err.contains("evil"),
        "root quotes nothing it read for the user: {err}"
    );
    assert_eq!(f.conf(), "font-size=10\n");
}

#[test]
fn a_draft_file_past_its_bound_is_refused() {
    let f = fixture("large", Some("font-size=10\n"));
    let big = format!("font-size=12\n{}", "#\n".repeat(600_000));
    let draft = f.draft(&f.conf_plan(&big));
    let (status, out, err) = f.apply(&draft);
    assert_eq!(status, 1, "{out}{err}");
    assert!(err.contains("larger than"), "{err}");
    assert_eq!(f.conf(), "font-size=10\n");
}

#[test]
fn apply_takes_one_draft() {
    let f = fixture("usage", None);
    let out = Command::new(env!("CARGO_BIN_EXE_vtium-config"))
        .args(f.targets.options())
        .arg("apply")
        .output()
        .expect("the binary runs");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn the_privileged_step_is_sudo_vtium_config_apply_dir_and_nothing_else() {
    let draft = PathBuf::from("/home/me/.local/state/vtium/drafts/1 2;rm -rf");
    let program = Path::new("/usr/bin/vtium-config");
    let f = fixture("argv", None);
    let sudo = save_argv(&Route::Sudo("sudo".into()), program, &f.targets, &draft);
    let want: Vec<OsString> = vec![
        "sudo".into(),
        program.into(),
        "apply".into(),
        draft.clone().into(),
    ];
    assert_eq!(sudo, want);
    assert!(
        sudo.iter().all(|a| !a.to_string_lossy().starts_with('-')),
        "no option reaches root: {sudo:?}"
    );
    // directories not the system's install as the user, with their options
    let user = save_argv(&Route::User, program, &f.targets, &draft);
    let mut want: Vec<OsString> = vec![program.into()];
    want.extend(f.targets.options());
    want.extend(["apply".into(), draft.clone().into()]);
    assert_eq!(user, want);
}

#[test]
fn only_the_system_s_directories_are_installed_through_sudo() {
    let f = fixture("route", None);
    let system = Targets {
        configdir: PathBuf::from("/etc/vtium"),
        frits: vtium_config::switch::default_frits(),
        vtium: vtium_config::apply::VTIUM.into(),
    };
    let sudo: OsString = "sudo".into();
    assert_eq!(route(&system, &sudo), Route::Sudo(sudo.clone()));
    assert_eq!(route(&f.targets, &sudo), Route::User);
}

#[test]
fn a_draft_file_hard_linked_elsewhere_is_refused_and_nothing_is_quoted() {
    let f = fixture("second-name", Some("font-size=10\n"));
    let draft = f.draft(&f.conf_plan("font-size=12\n"));
    // a file of the user's own outside the draft, linked in by a second name
    let elsewhere = f.root.join("elsewhere.conf");
    // a valid file, so only the second name refuses it
    std::fs::write(&elsewhere, "font-size=12 # SECRET-TOKEN-7731\n").expect("the file");
    let file = draft.join("vtium.conf");
    std::fs::remove_file(&file).expect("the draft's file");
    std::fs::hard_link(&elsewhere, &file).expect("a hard link");
    let (status, out, err) = f.apply(&draft);
    assert_eq!(status, 1, "{out}{err}");
    assert!(
        err.contains("vtium.conf") && err.contains("hard link"),
        "{err}"
    );
    assert!(!format!("{out}{err}").contains("SECRET"), "{out}{err}");
    assert_eq!(f.conf(), "font-size=10\n");
}

#[test]
fn the_owners_a_draft_may_have_are_root_and_the_user_sudo_ran_for() {
    assert_eq!(trusted_owners(0, Some(1000)), [0, 1000]);
    assert_eq!(trusted_owners(0, None), [0]);
    // run as a user, the user's own files, whatever SUDO_UID says
    assert_eq!(trusted_owners(1000, Some(5)), [0, 1000]);
    assert_eq!(trusted_owners(1000, None), [0, 1000]);
}

#[test]
fn the_save_view_shows_the_command_the_save_runs() {
    use vtium_config::tui::model::Saving;
    use vtium_config::tui::{Paths, saving};
    let exe = std::env::current_exe().expect("this test's path");
    let system = Targets::system();
    let paths = Paths {
        configdir: system.configdir.clone(),
        frits: system.frits.clone(),
        drafts: PathBuf::from("/home/me/drafts"),
        sudo: "sudo".into(),
        vtium: system.vtium.clone(),
    };
    assert_eq!(
        saving(&paths),
        Saving::Sudo(format!("sudo {} apply DRAFT", exe.display()))
    );
    let f = fixture("shown", None);
    let mine = Paths {
        configdir: f.targets.configdir.clone(),
        frits: f.targets.frits.clone(),
        drafts: PathBuf::from("/home/me/drafts"),
        sudo: "sudo".into(),
        vtium: f.targets.vtium.clone(),
    };
    let want = save_argv(&Route::User, &exe, &f.targets, Path::new("DRAFT"))
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(saving(&mine), Saving::User(want));
}
