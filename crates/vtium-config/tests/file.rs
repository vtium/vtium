//! Reading the head file from disk: the ownership check and the checks on
//! its bytes (ADR-1401 D3, D10).
//!
//! No test runs as root, so each trusts its own user's files and root's
//! directories with [`Trust::owner`], below a fixture directory of its own
//! with [`Trust::below`]: the fixture's modes are the test's, wherever the
//! target directory sits, `/tmp` included.

use std::fs;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use vtium_config::{FILE_MAX, Inspected, Problem, Trust, Untrusted, inspect, read, text};

/// A fresh fixture directory of its own for one test, mode 0755.
fn fixture(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("vtium-config-fixtures")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("the fixture directory");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("mode 0755");
    dir
}

/// The test's own user, trusted below `dir`.
fn me(dir: &Path) -> Trust {
    let uid = fs::metadata(dir).expect("the fixture").uid();
    Trust::owner(uid).below(dir)
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
    let dir = fixture("owner");
    let file = dir.join("heads.conf");
    write(&file, "version = 1\n", 0o644);
    assert_eq!(
        read(&file, &me(&dir)).expect("trusted"),
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
                problem(read(&file, &me(&dir))),
                Problem::Untrusted(Untrusted::Writable { .. })
            ),
            "mode {mode:o}"
        );
    }
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).expect("mode");
    assert!(
        read(&file, &me(&dir)).is_ok(),
        "a file only its owner reads passes"
    );
}

#[test]
fn a_directory_above_the_file_passes_the_same_check() {
    let dir = fixture("directory");
    let file = dir.join("heads.conf");
    write(&file, "version = 1\n", 0o644);
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o775)).expect("mode");
    let p = problem(read(&file, &me(&dir)));
    assert!(
        matches!(&p, Problem::Untrusted(Untrusted::Writable { path, .. }) if path.raw() == dir.to_string_lossy()),
        "{p:?}"
    );
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("mode");
    assert!(read(&file, &me(&dir)).is_ok());
}

#[test]
fn a_symlink_is_followed_and_its_targets_directories_are_checked() {
    let dir = fixture("symlink");
    let open = dir.join("open");
    fs::create_dir(&open).expect("a directory");
    fs::set_permissions(&open, fs::Permissions::from_mode(0o777)).expect("mode");
    let target = open.join("real.conf");
    write(&target, "version = 1\n", 0o644);
    let link = dir.join("heads.conf");
    symlink(&target, &link).expect("the link");
    assert!(matches!(
        problem(read(&link, &me(&dir))),
        Problem::Untrusted(Untrusted::Writable { .. })
    ));
    fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).expect("mode");
    assert_eq!(
        read(&link, &me(&dir)).expect("trusted"),
        Some("version = 1\n".to_owned())
    );
}

#[test]
fn a_missing_file_is_none_and_anything_but_a_regular_file_is_refused() {
    let dir = fixture("kinds");
    assert_eq!(
        read(&dir.join("absent.conf"), &me(&dir)).expect("absent"),
        None
    );
    let dangling = dir.join("dangling.conf");
    symlink(dir.join("nowhere"), &dangling).expect("the link");
    assert_eq!(read(&dangling, &me(&dir)).expect("absent"), None);
    assert_eq!(problem(read(&dir, &me(&dir))), Problem::NotAFile);
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
    assert_eq!(problem(read(&fifo, &me(&dir))), Problem::NotAFile);
}

// ADR-1401 D7: `vtium check-config` checks a candidate before it is
// installed, so the reader gives its text and the ownership verdict apart
#[test]
fn inspect_gives_the_text_and_the_ownership_verdict_together() {
    let dir = fixture("inspect");
    let file = dir.join("heads.conf");
    write(&file, "version = 1\n", 0o664);
    let Some(Inspected { text, trusted }) = inspect(&file, &me(&dir)).expect("read") else {
        panic!("the file is present");
    };
    assert_eq!(text, Ok("version = 1\n".to_owned()));
    assert!(
        matches!(trusted, Err(Untrusted::Writable { .. })),
        "{trusted:?}"
    );
    assert!(matches!(
        problem(read(&file, &me(&dir))),
        Problem::Untrusted(Untrusted::Writable { .. })
    ));
    write(&file, "version = 1\n", 0o644);
    let inspected = inspect(&file, &me(&dir)).expect("read").expect("present");
    assert_eq!(inspected.trusted, Ok(()));
}

/// Whether the inotify descriptor `fd` holds an event.
fn has_event(fd: &rustix::fd::OwnedFd) -> bool {
    let mut buf = [0_u8; 1024];
    rustix::io::read(fd, &mut buf).is_ok_and(|n| n > 0)
}

// Opening a FIFO, a watchdog or a serial port acts on it, so anything but
// a regular file is refused before it is opened
#[test]
fn a_fifo_is_refused_without_being_opened() {
    use rustix::fs::inotify;
    let dir = fixture("unopened");
    let fifo = dir.join("fifo.conf");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::from_raw_mode(0o644),
        0,
    )
    .expect("the FIFO");
    let link = dir.join("link.conf");
    symlink(&fifo, &link).expect("the link");
    let watch = inotify::init(inotify::CreateFlags::NONBLOCK | inotify::CreateFlags::CLOEXEC)
        .expect("inotify");
    inotify::add_watch(&watch, &fifo, inotify::WatchFlags::OPEN).expect("the watch");
    assert!(!has_event(&watch), "no event before the read");
    assert_eq!(problem(read(&fifo, &me(&dir))), Problem::NotAFile);
    assert_eq!(problem(read(&link, &me(&dir))), Problem::NotAFile);
    assert!(!has_event(&watch), "the FIFO was opened");
    // the watch sees an open: the test's own
    let _reader = fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(&fifo)
        .expect("the FIFO opens");
    assert!(has_event(&watch), "the watch sees an open");
}

// A file whose size is far past FILE_MAX is read no further than the
// bound: the test's 1 GiB of holes reads FILE_MAX bytes and one
#[test]
fn a_sparse_file_far_past_the_bound_is_refused_from_its_first_bytes() {
    let dir = fixture("sparse");
    let file = dir.join("heads.conf");
    write(&file, "version = 1\n", 0o644);
    fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .and_then(|f| f.set_len(1 << 30))
        .expect("the holes");
    assert_eq!(problem(read(&file, &me(&dir))), Problem::TooLarge);
}

#[test]
fn a_file_past_its_byte_bound_is_refused() {
    let dir = fixture("large");
    let file = dir.join("heads.conf");
    write(&file, &"#".repeat(FILE_MAX), 0o644);
    assert!(read(&file, &me(&dir)).is_ok(), "FILE_MAX bytes pass");
    write(&file, &"#".repeat(FILE_MAX + 1), 0o644);
    assert_eq!(problem(read(&file, &me(&dir))), Problem::TooLarge);
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

#[test]
fn the_check_stops_at_a_tests_top_and_reads_the_top_itself() {
    let dir = fixture("top");
    let open = dir.join("open");
    let inner = open.join("inner");
    fs::create_dir_all(&inner).expect("the directories");
    fs::set_permissions(&open, fs::Permissions::from_mode(0o777)).expect("mode");
    fs::set_permissions(&inner, fs::Permissions::from_mode(0o755)).expect("mode");
    let file = inner.join("heads.conf");
    write(&file, "version = 1\n", 0o644);
    assert!(
        read(&file, &me(&inner)).is_ok(),
        "above the top goes unchecked"
    );
    assert!(matches!(
        problem(read(&file, &me(&open))),
        Problem::Untrusted(Untrusted::Writable { .. })
    ));
    let uid = fs::metadata(&dir).expect("the fixture").uid();
    assert!(
        read(&file, &Trust::owner(uid)).is_err(),
        "with no top every directory up to / is read, the open one among them"
    );
}

#[test]
fn each_link_of_a_symlink_chain_sits_in_a_directory_that_passes() {
    let dir = fixture("chain");
    let (a, open, b) = (dir.join("a"), dir.join("open"), dir.join("b"));
    for d in [&a, &open, &b] {
        fs::create_dir(d).expect("a directory");
        fs::set_permissions(d, fs::Permissions::from_mode(0o755)).expect("mode");
    }
    let real = b.join("real.conf");
    write(&real, "version = 1\n", 0o644);
    // a/heads.conf -> open/middle -> ../b/real.conf, relative as it often is
    symlink("../b/real.conf", open.join("middle")).expect("the middle link");
    let first = a.join("heads.conf");
    symlink(open.join("middle"), &first).expect("the first link");
    assert_eq!(
        read(&first, &me(&dir)).expect("trusted"),
        Some("version = 1\n".to_owned())
    );

    // anyone could swap the middle link for one to a file of theirs
    fs::set_permissions(&open, fs::Permissions::from_mode(0o777)).expect("mode");
    let p = problem(read(&first, &me(&dir)));
    assert!(
        matches!(&p, Problem::Untrusted(Untrusted::Writable { path, .. }) if path.raw().ends_with("/open")),
        "{p:?}"
    );
}
