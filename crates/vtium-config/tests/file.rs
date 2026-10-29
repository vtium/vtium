//! Reading the head file from disk: the ownership check and the checks on
//! its bytes (ADR-1401 D3, D10).
//!
//! No test runs as root, so each trusts its own user's files and root's
//! directories with [`Trust::owner`]; the files live under cargo's
//! temporary directory for this target, whose directories no one else can
//! write.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use vtium_config::{FILE_MAX, Problem, Trust, Untrusted, read, text};

/// A fresh directory of its own for one test, mode 0755.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("vtium-config")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the scratch directory");
    for d in [dir.as_path(), dir.parent().expect("a parent")] {
        fs::set_permissions(d, fs::Permissions::from_mode(0o755)).expect("mode 0755");
    }
    dir
}

fn me() -> Trust {
    let uid = fs::metadata(env!("CARGO_TARGET_TMPDIR"))
        .expect("the temporary directory")
        .uid();
    Trust::owner(uid)
}

fn write(path: &Path, text: &str, mode: u32) {
    fs::write(path, text).expect("the file");
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("its mode");
}

fn problem(result: Result<Option<String>, vtium_config::Rejected>) -> Problem {
    match result {
        Err(r) => r.errors[0].problem.clone(),
        Ok(t) => panic!("read: {t:?}"),
    }
}

#[test]
fn a_file_is_read_only_when_its_owner_is_trusted_and_no_one_else_can_write_it() {
    let dir = scratch("owner");
    let file = dir.join("heads.conf");
    write(&file, "version = 1\n", 0o644);
    assert_eq!(
        read(&file, &me()).expect("trusted"),
        Some("version = 1\n".to_owned())
    );

    // as vtium reads it, a file a user owns is not root's
    assert!(matches!(
        problem(read(&file, &Trust::ROOT)),
        Problem::Untrusted(Untrusted::Owner { .. })
    ));

    for mode in [0o664, 0o646] {
        fs::set_permissions(&file, fs::Permissions::from_mode(mode)).expect("mode");
        assert!(
            matches!(
                problem(read(&file, &me())),
                Problem::Untrusted(Untrusted::Writable { .. })
            ),
            "mode {mode:o}"
        );
    }
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).expect("mode");
    assert!(
        read(&file, &me()).is_ok(),
        "a file only its owner reads passes"
    );
}

#[test]
fn a_directory_above_the_file_passes_the_same_check() {
    let dir = scratch("directory");
    let file = dir.join("heads.conf");
    write(&file, "version = 1\n", 0o644);
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o775)).expect("mode");
    let p = problem(read(&file, &me()));
    assert!(
        matches!(&p, Problem::Untrusted(Untrusted::Writable { path, .. }) if path.raw() == dir.to_string_lossy()),
        "{p:?}"
    );
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("mode");
    assert!(read(&file, &me()).is_ok());
}

#[test]
fn a_symlink_is_followed_and_its_targets_directories_are_checked() {
    let dir = scratch("symlink");
    let open = dir.join("open");
    fs::create_dir(&open).expect("a directory");
    fs::set_permissions(&open, fs::Permissions::from_mode(0o777)).expect("mode");
    let target = open.join("real.conf");
    write(&target, "version = 1\n", 0o644);
    let link = dir.join("heads.conf");
    symlink(&target, &link).expect("the link");
    assert!(matches!(
        problem(read(&link, &me())),
        Problem::Untrusted(Untrusted::Writable { .. })
    ));
    fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).expect("mode");
    assert_eq!(
        read(&link, &me()).expect("trusted"),
        Some("version = 1\n".to_owned())
    );
}

#[test]
fn a_missing_file_is_none_and_anything_but_a_regular_file_is_refused() {
    let dir = scratch("kinds");
    assert_eq!(read(&dir.join("absent.conf"), &me()).expect("absent"), None);
    let dangling = dir.join("dangling.conf");
    symlink(dir.join("nowhere"), &dangling).expect("the link");
    assert_eq!(read(&dangling, &me()).expect("absent"), None);
    assert_eq!(problem(read(&dir, &me())), Problem::NotAFile);
    // a FIFO with no writer is refused at once, never waited on
    let fifo = dir.join("fifo.conf");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::from_raw_mode(0o644),
        0,
    )
    .expect("the FIFO");
    assert_eq!(problem(read(&fifo, &me())), Problem::NotAFile);
}

#[test]
fn a_file_past_its_byte_bound_is_refused() {
    let dir = scratch("large");
    let file = dir.join("heads.conf");
    write(&file, &"#".repeat(FILE_MAX), 0o644);
    assert!(read(&file, &me()).is_ok(), "FILE_MAX bytes pass");
    write(&file, &"#".repeat(FILE_MAX + 1), 0o644);
    assert_eq!(problem(read(&file, &me())), Problem::TooLarge);
}

#[test]
fn a_file_with_a_control_character_or_no_utf8_is_refused_at_its_line() {
    assert!(text(b"version = 1\n\t# a tab\n").is_ok());
    let r = text(b"version = 1\n[head a\x1b[2J]\n# \x00\nok\r\n").expect_err("controls");
    let found: Vec<_> = r
        .errors
        .iter()
        .map(|e| (e.line, e.problem.clone()))
        .collect();
    assert_eq!(
        found,
        [
            (Some(2), Problem::ControlCharacter('\u{1b}')),
            (Some(3), Problem::ControlCharacter('\0')),
            (Some(4), Problem::ControlCharacter('\r')),
        ]
    );
    let shown = r.errors[0].to_string();
    assert!(
        !shown.contains('\u{1b}'),
        "the error shows the character escaped: {shown}"
    );
    assert!(
        text("# \u{85}\n".as_bytes()).is_err(),
        "a C1 control is a control character"
    );

    let r = text(b"version = 1\n# ok\n# \xff\n").expect_err("not UTF-8");
    assert_eq!(
        (r.errors[0].line, &r.errors[0].problem),
        (Some(3), &Problem::NotUtf8)
    );
}

#[test]
fn trust_takes_the_owner_for_files_and_root_too_for_directories() {
    let path = Path::new("/etc/vtium");
    assert!(Trust::ROOT.check(path, 0, 0o100644, false).is_ok());
    assert!(Trust::ROOT.check(path, 1000, 0o100644, false).is_err());
    assert!(
        Trust::ROOT.check(path, 0, 0o041777, true).is_err(),
        "a sticky directory anyone writes"
    );
    let user = Trust::owner(1000);
    assert!(user.check(path, 0, 0o040755, true).is_ok());
    assert!(
        user.check(path, 0, 0o100644, false).is_err(),
        "a file is the trusted user's own"
    );
    assert!(user.check(path, 1001, 0o040755, true).is_err());
}
