//! The head file's one writer (ADR-1201 D6): a head's `profile =` line set
//! in its section with every other byte kept, a file vtium refuses to read
//! refused, and a file replaced in one rename with the old one kept
//! beside it.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use vtium_settings::{Of, SectionEdit, install_file, parse, set_key};

/// Sets the look of the head `head` in `text` to `look`, with the section
/// writer.
fn set_look(text: &str, head: &str, look: &str) -> Result<String, SectionEdit> {
    set_key(text, Of::Head(head), "look", Some(look))
}

/// Every head file the tree ships or tests read, by its path from the
/// workspace's root.
const SHIPPED: [&str; 14] = [
    "crates/vtium-settings/templates/heads.conf",
    "docs/heads/default.conf",
    "docs/heads/kiosk.conf",
    "docs/heads/kiosk-admin.conf",
    "docs/heads/reference.conf",
    "docs/heads/stations.conf",
    "docs/heads/two-heads.conf",
    "tools/tier3/guest/heads/broken-closed.conf",
    "tools/tier3/guest/heads/closed-unmatched.conf",
    "tools/tier3/guest/heads/closed-valid.conf",
    "tools/tier3/guest/heads/kiosk.conf",
    "tools/tier3/guest/heads/partly-open.conf",
    "tools/tier3/guest/heads/policy-typo.conf",
    "tools/tier3/guest/heads/two-heads.conf",
];

fn shipped(path: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The lines of `a` and `b` that differ, as (line number, a's, b's).
fn changed(a: &str, b: &str) -> Vec<(usize, String, String)> {
    let a: Vec<&str> = a.split_inclusive('\n').collect();
    let b: Vec<&str> = b.split_inclusive('\n').collect();
    (0..a.len().max(b.len()))
        .filter_map(|i| {
            let (x, y) = (a.get(i).copied(), b.get(i).copied());
            (x != y).then(|| {
                (
                    i + 1,
                    x.unwrap_or_default().to_owned(),
                    y.unwrap_or_default().to_owned(),
                )
            })
        })
        .collect()
}

#[test]
fn every_shipped_file_with_its_own_looks_comes_back_byte_for_byte() {
    let mut heads = 0;
    for path in SHIPPED {
        let text = shipped(path);
        let Ok(parsed) = parse(&text) else { continue };
        for head in &parsed.config.heads {
            let out = set_look(&text, &head.name, &head.look)
                .unwrap_or_else(|e| panic!("{path} [head {}]: {e}", head.name));
            if head.look_line != 0 {
                assert_eq!(out, text, "{path} [head {}]", head.name);
            }
            heads += 1;
        }
    }
    assert!(heads >= 8, "the shipped files hold heads to edit: {heads}");
}

#[test]
fn every_shipped_file_changes_only_the_look_and_reads_the_same_otherwise() {
    for path in SHIPPED {
        let text = shipped(path);
        let Ok(before) = parse(&text) else { continue };
        for (i, head) in before.config.heads.iter().enumerate() {
            let out = set_look(&text, &head.name, "fresh.one").expect("the edit");
            let after = parse(&out).unwrap_or_else(|e| panic!("{path}: {e:?}"));
            let mut want = before.config.clone();
            want.heads[i].look = "fresh.one".into();
            let got = &after.config;
            assert_eq!(got.heads[i].look, "fresh.one", "{path}");
            if head.look_line == 0 {
                // one line inserted below the heading, every line after it
                // moved down by one
                assert_eq!(out.lines().count(), text.lines().count() + 1, "{path}");
                let lines: Vec<&str> = out.lines().collect();
                assert_eq!(lines[head.line], "look = fresh.one", "{path}");
                let mut kept = lines.clone();
                kept.remove(head.line);
                assert_eq!(kept, text.lines().collect::<Vec<_>>(), "{path}");
            } else {
                let diff = changed(&text, &out);
                assert_eq!(diff.len(), 1, "{path}: {diff:?}");
                assert_eq!(diff[0].0, head.look_line, "{path}");
                // the rest of the file reads as it did
                let mut same = got.clone();
                same.heads[i].look_line = want.heads[i].look_line;
                assert_eq!(same, want, "{path}");
            }
        }
    }
}

#[test]
fn a_value_is_replaced_in_place_and_its_comment_kept() {
    let text = "version = 1\n[head desk]\n  look\t=  desk    # the desk's font\n[rules]\ndisplay * -> desk\n";
    let out = set_look(text, "desk", "big").expect("the edit");
    assert_eq!(
        out,
        "version = 1\n[head desk]\n  look\t=  big    # the desk's font\n[rules]\ndisplay * -> desk\n"
    );
}

#[test]
fn a_missing_line_goes_below_the_heading_with_the_heading_s_line_ending() {
    let text =
        "version = 1\r\n[head  desk ]   # mine\r\nconsoles = 2\r\n[rules]\r\ndisplay * -> desk\r\n";
    let out = set_look(text, "desk", "big").expect("the edit");
    assert_eq!(
        out,
        "version = 1\r\n[head  desk ]   # mine\r\nlook = big\r\nconsoles = 2\r\n[rules]\r\ndisplay * -> desk\r\n"
    );
}

#[test]
fn a_heading_on_the_last_line_without_a_newline_gains_the_line_after_it() {
    let text = "version = 1\n[head desk]";
    assert_eq!(
        set_look(text, "desk", "big").expect("the edit"),
        "version = 1\n[head desk]\nlook = big\n"
    );
}

#[test]
fn a_byte_order_mark_and_a_crlf_look_line_stay() {
    let text = "\u{feff}version = 1\r\n[head desk]\r\nlook = a\r\n";
    assert_eq!(
        set_look(text, "desk", "b").expect("the edit"),
        "\u{feff}version = 1\r\n[head desk]\r\nlook = b\r\n"
    );
}

#[test]
fn the_head_named_is_the_one_edited() {
    let text = "version = 1\n[head a]\nlook = x\n[head b]\nlook = x\n";
    assert_eq!(
        set_look(text, "b", "y").expect("the edit"),
        "version = 1\n[head a]\nlook = x\n[head b]\nlook = y\n"
    );
}

#[test]
fn a_file_vtium_refuses_to_read_is_refused() {
    for path in SHIPPED {
        let text = shipped(path);
        if parse(&text).is_ok() {
            continue;
        }
        match set_look(&text, "desk", "big") {
            Err(SectionEdit::Unreadable(rejected)) => assert!(!rejected.errors.is_empty()),
            other => panic!("{path}: {other:?}"),
        }
    }
    // a section with no version line
    assert!(matches!(
        set_look("[head desk]\n", "desk", "big"),
        Err(SectionEdit::Unreadable(_))
    ));
}

#[test]
fn a_head_the_file_does_not_declare_is_refused() {
    let text = "version = 1\n[head desk]\n";
    match set_look(text, "panel", "big") {
        Err(SectionEdit::NoSection(name)) => assert_eq!(name, "panel"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_look_name_vtium_would_refuse_is_refused() {
    let text = "version = 1\n[head desk]\n";
    for bad in ["", "two words", "é", &"x".repeat(65), "a=b", "a\nb"] {
        assert!(set_look(text, "desk", bad).is_err(), "{bad:?}");
    }
    assert!(set_look(text, "desk", &"x".repeat(64)).is_ok());
}

#[test]
fn each_refusal_says_what_to_do() {
    let text = "version = 1\n[head desk]\n";
    let no_head = set_look(text, "panel", "big").unwrap_err().to_string();
    assert!(no_head.contains("panel"), "{no_head}");
    let unreadable = set_look("[head desk]\n", "desk", "b")
        .unwrap_err()
        .to_string();
    assert!(unreadable.contains("vtium check-config"), "{unreadable}");
}

/// A fresh directory of its own for one test.
fn fixture(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("vtium-settings-write")
        .join(name);
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o755));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the fixture directory");
    dir
}

#[test]
fn a_file_is_replaced_whole_with_its_mode_and_the_old_one_kept_as_bak() {
    let dir = fixture("replace");
    let file = dir.join("heads.conf");
    fs::write(&file, "old\n").expect("the old file");
    fs::write(dir.join("heads.conf.bak"), "older\n").expect("an older backup");
    install_file(&file, b"new\n", 0o644).expect("the install");
    assert_eq!(fs::read(&file).expect("the new file"), b"new\n");
    assert_eq!(
        fs::metadata(&file).expect("its mode").permissions().mode() & 0o7777,
        0o644
    );
    assert_eq!(
        fs::read(dir.join("heads.conf.bak")).expect("one generation"),
        b"old\n"
    );
    let left: Vec<_> = fs::read_dir(&dir)
        .expect("the directory")
        .map(|e| e.expect("an entry").file_name())
        .collect();
    assert_eq!(left.len(), 2, "no temporary file is left: {left:?}");
}

#[test]
fn a_new_file_is_written_with_no_backup() {
    let dir = fixture("new");
    let file = dir.join("tiny.vtfrit");
    install_file(&file, b"source=builtin:unifont\n", 0o644).expect("the install");
    assert_eq!(
        fs::read(&file).expect("the file"),
        b"source=builtin:unifont\n"
    );
    assert!(!dir.join("tiny.vtfrit.bak").exists());
}

#[test]
fn a_write_that_fails_before_its_rename_leaves_the_old_file_whole() {
    let dir = fixture("readonly");
    let file = dir.join("heads.conf");
    fs::write(&file, "old\n").expect("the old file");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("read-only");
    let result = install_file(&file, b"new\n", 0o644);
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("writable again");
    assert!(result.is_err());
    assert_eq!(fs::read(&file).expect("the old file"), b"old\n");
    let left: Vec<_> = fs::read_dir(&dir).expect("the directory").collect();
    assert_eq!(left.len(), 1, "nothing else is left");
}

#[test]
fn a_link_in_the_file_s_place_is_refused_and_both_left() {
    let dir = fixture("link");
    let target = dir.join("elsewhere");
    fs::write(&target, "theirs\n").expect("the link's target");
    let file = dir.join("heads.conf");
    std::os::unix::fs::symlink(&target, &file).expect("the link");
    assert!(install_file(&file, b"new\n", 0o644).is_err());
    assert_eq!(fs::read(&target).expect("the target"), b"theirs\n");
    assert!(
        fs::symlink_metadata(&file)
            .expect("the link")
            .file_type()
            .is_symlink(),
        "the link stays"
    );
    let left: Vec<_> = fs::read_dir(&dir).expect("the directory").collect();
    assert_eq!(left.len(), 2, "no backup or new file is left");
}

#[test]
fn the_backup_keeps_the_old_file_s_mode() {
    let dir = fixture("mode");
    let file = dir.join("vtium.conf");
    fs::write(&file, "old\n").expect("the old file");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).expect("mode 0600");
    install_file(&file, b"new\n", 0o644).expect("the install");
    let bak = fs::metadata(dir.join("vtium.conf.bak")).expect("the backup");
    assert_eq!(bak.permissions().mode() & 0o7777, 0o600);
}
