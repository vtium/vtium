//! The real desk readies the install of a font of the person's as a
//! system font, over a bench whose system fonts folder is a temporary
//! directory and whose font helper is vtium-config itself. Each test
//! drives one of the three rules the install follows: a different file of
//! the same name shows both files and the command; a file with the same
//! bytes needs nothing, so no draft is written; and a file the font helper
//! cannot read as a font is refused, and its draft removed.
//!
//! References:
//! - ADR-1205 D5: the install of a font as a system font, its three rules.

use std::path::{Path, PathBuf};

use vtium_config::apply::Placement;
use vtium_config::specimen::Helper;
use vtium_config::tui::desk::{Desk, Done, Op, Store};
use vtium_config::tui::words;

/// A bench under the test target's directory: the configuration
/// directory, its system fonts folder, the person's own font folder, and
/// the drafts' directory.
struct Bench {
    root: PathBuf,
}

impl Bench {
    fn new(name: &str) -> Bench {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!(
                "vtium-config-system-font-{name}-{}",
                std::process::id()
            ));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["etc/fonts", "etc/local-fonts", "home/fonts", "sys"] {
            std::fs::create_dir_all(root.join(dir)).expect("the bench");
        }
        Bench {
            root: root.canonicalize().expect("the bench"),
        }
    }

    fn drafts(&self) -> PathBuf {
        self.root.join("home/bundles")
    }

    /// The real desk, whose font helper is `helper`.
    fn desk(&self, helper: Helper) -> Store {
        let home = self.root.clone();
        let store = vtium_config::store::Store::locate(
            1000,
            move |name: &str| match name {
                "HOME" => Some(home.join("home").into()),
                "XDG_CONFIG_HOME" => Some(home.join("home/config").into()),
                "XDG_STATE_HOME" => Some(home.join("home/state").into()),
                _ => None,
            },
            |_| None,
        )
        .expect("the store");
        let paths = vtium_config::tui::Paths {
            configdir: self.root.join("etc"),
            frits: Ok(self.root.join("etc/fonts")),
            drafts: self.drafts(),
            sudo: "sudo".into(),
            vtium: self.root.join("vtium").into(),
            helper,
            sys: self.root.join("sys"),
        };
        Store::new(store, paths, Box::new(Default::default))
    }

    /// The drafts written under the drafts' directory.
    fn drafts_written(&self) -> Vec<PathBuf> {
        std::fs::read_dir(self.drafts())
            .map(|dir| dir.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    }

    /// Writes `bytes` as the person's font `name`, and gives its path.
    fn yours(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.root.join("home/fonts").join(name);
        std::fs::write(&path, bytes).expect("the person's font");
        path
    }

    /// Writes `bytes` as the installed system font `name`.
    fn installed(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.root.join("etc/local-fonts").join(name), bytes)
            .expect("the installed font");
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn font_bytes() -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DejaVuSansMono-ascii.ttf"),
    )
    .expect("the fixture font")
}

fn helper() -> Helper {
    Helper::at(env!("CARGO_BIN_EXE_vtium-config"))
}

/// A different file of the same name already in the system fonts folder:
/// the desk describes both files, gives both files' facts, and gives the
/// command that installs the draft. The bench's folder is not the
/// system's, so the command runs `apply` without sudo.
#[test]
fn a_different_installed_file_of_the_same_name_gives_both_files_and_the_command() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let bench = Bench::new("replaces");
    let font = font_bytes();
    let source = bench.yours("Mine.ttf", &font);
    let mut other = font.clone();
    other.extend_from_slice(b"\0\0\0\0");
    bench.installed("Mine.ttf", &other);
    let mut desk = bench.desk(helper());
    let Ok(Done::SystemFont(ready)) = desk.run(&Op::SystemFont(source.clone())) else {
        panic!("the install is not readied");
    };
    assert_eq!(ready.placement, Placement::Replaces);
    let (installed, yours) = ready.facts.expect("both files' facts");
    assert!(installed.contains("local-fonts/Mine.ttf"), "{installed}");
    assert!(
        installed.contains(&format!("{} bytes", other.len())),
        "{installed}"
    );
    assert!(yours.contains("home/fonts/Mine.ttf"), "{yours}");
    assert!(yours.contains(&format!("{} bytes", font.len())), "{yours}");
    assert!(yours.contains("DejaVu Sans Mono"), "{yours}");
    assert!(ready.command.contains("apply"), "{}", ready.command);
    let drafts = bench.drafts_written();
    assert_eq!(drafts.len(), 1, "{drafts:?}");
    assert!(
        ready.command.contains(&*drafts[0].to_string_lossy()),
        "{}",
        ready.command
    );
}

/// The same bytes already in the system fonts folder: the font is
/// installed already, so the desk gives no command and writes no draft.
#[test]
fn an_identical_installed_file_needs_nothing_and_writes_no_draft() {
    let bench = Bench::new("identical");
    let font = font_bytes();
    let source = bench.yours("Mine.ttf", &font);
    bench.installed("Mine.ttf", &font);
    let mut desk = bench.desk(helper());
    let Ok(Done::SystemFont(ready)) = desk.run(&Op::SystemFont(source)) else {
        panic!("the install is not readied");
    };
    assert_eq!(ready.placement, Placement::Identical);
    assert!(ready.command.is_empty(), "{}", ready.command);
    assert_eq!(ready.facts, None);
    assert!(
        bench.drafts_written().is_empty(),
        "{:?}",
        bench.drafts_written()
    );
}

/// A file the font helper cannot read as a font, and a helper that gives
/// no answer: the desk refuses the install and removes the draft it wrote
/// for the check.
#[test]
fn a_file_the_helper_cannot_read_as_a_font_is_refused_and_its_draft_removed() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    for (name, bytes, helper) in [
        ("notes.ttf", b"not a font at all".to_vec(), helper()),
        (
            "Mine.ttf",
            font_bytes(),
            Helper::at("/nonexistent/vtium-config"),
        ),
    ] {
        let bench = Bench::new(name.trim_end_matches(".ttf"));
        let source = bench.yours(name, &bytes);
        let mut desk = bench.desk(helper);
        let Err(why) = desk.run(&Op::SystemFont(source.clone())) else {
            panic!("{name}: the install is readied");
        };
        let refused = words::system_font_not_a_font(&source.to_string_lossy(), "");
        assert!(why.starts_with(&refused), "{name}: {why}");
        assert!(
            bench.drafts_written().is_empty(),
            "{name}: {:?}",
            bench.drafts_written()
        );
        // nothing is readied, so nothing installs
        let mut run = |_: &[std::ffi::OsString]| -> std::io::Result<(bool, String)> {
            panic!("{name}: the install runs");
        };
        assert!(desk.install_font(&mut run).is_err(), "{name}");
    }
}

/// A font readied and never installed, as after Esc on its question,
/// leaves no draft once the desk is gone. A copy that `apply` finds fails
/// the trust rule comes back as such, apart from what `apply` wrote.
#[test]
fn a_readied_font_s_draft_goes_with_the_desk_and_an_untrusted_copy_is_reported() {
    if let Some(why) = vtium_testscript::qemu_without_seccomp() {
        eprintln!("skipped: {why}");
        return;
    }
    let bench = Bench::new("dropped");
    let source = bench.yours("Mine.ttf", &font_bytes());
    let mut desk = bench.desk(helper());
    assert!(matches!(
        desk.run(&Op::SystemFont(source.clone())),
        Ok(Done::SystemFont(_))
    ));
    assert_eq!(bench.drafts_written().len(), 1);
    drop(desk);
    assert!(
        bench.drafts_written().is_empty(),
        "{:?}",
        bench.drafts_written()
    );
    let mut desk = bench.desk(helper());
    assert!(matches!(
        desk.run(&Op::SystemFont(source)),
        Ok(Done::SystemFont(_))
    ));
    let mut run = |_: &[std::ffi::OsString]| -> std::io::Result<(bool, String)> {
        Ok((
            true,
            "wrote\t/x/Mine.ttf\nuntrusted\t/x/Mine.ttf\tthe file is writable by others.\n".into(),
        ))
    };
    let Ok(Done::FontInstalled {
        said, untrusted, ..
    }) = desk.install_font(&mut run)
    else {
        panic!("the font is not installed");
    };
    assert_eq!(said, "wrote /x/Mine.ttf");
    assert_eq!(
        untrusted.as_deref(),
        Some("the file is writable by others.")
    );
}
