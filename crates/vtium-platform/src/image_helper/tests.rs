//! The host's own rules, in process; the host against stand-in helpers and
//! the real one is in `tests/image_helper.rs` and the helper crate's tests.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]

use rustix::fs::{FileType, MemfdFlags, SealFlags};
use vtium_image::decode::wire::{ReplyHeader, Status};
use vtium_image::decode::{Medium, Reply, Request};

use super::identity::Account;
use super::start::{self, Program, refusal};
use super::{inline_file, memory_cap, pixels, without_helper};

fn request(medium: Medium) -> Request {
    Request {
        medium,
        offset: 0,
        max: 0,
        min: 0,
        inflate: false,
        png: true,
        exact: false,
        discard: false,
        any_image: false,
        pixels: None,
        largest: (100, 100),
        largest_bytes: 1 << 20,
    }
}

#[test]
fn a_program_root_or_vtium_owns_and_alone_writes_is_taken() {
    let me = 1000;
    assert_eq!(refusal(FileType::RegularFile, 0, 0o100_755, me), None);
    assert_eq!(refusal(FileType::RegularFile, me, 0o100_700, me), None);
    for mode in [0o104_755, 0o102_755] {
        assert!(
            refusal(FileType::RegularFile, 0, mode, 0)
                .unwrap()
                .contains("setuid or setgid")
        );
    }
    assert!(refusal(FileType::Directory, 0, 0o40_755, me).is_some());
    assert!(
        refusal(FileType::RegularFile, 1001, 0o100_755, me)
            .unwrap()
            .contains("uid 1001")
    );
    for mode in [0o100_775, 0o100_757] {
        assert!(
            refusal(FileType::RegularFile, me, mode, me)
                .unwrap()
                .contains("may be written")
        );
    }
}

#[test]
fn the_memory_cap_is_three_budgets_and_the_base() {
    assert_eq!(memory_cap(0), 64 << 20);
    assert_eq!(memory_cap(10 << 20), (30 << 20) + (64 << 20));
    assert_eq!(memory_cap(u64::MAX), u64::MAX);
}

#[test]
fn without_a_helper_files_are_unreadable_and_inline_bytes_corrupt() {
    assert_eq!(
        without_helper(&request(Medium::File(b"/a".to_vec()))),
        Reply::Unreadable
    );
    assert_eq!(
        without_helper(&request(Medium::SharedMemory(b"a".to_vec()))),
        Reply::Unreadable
    );
    assert_eq!(
        without_helper(&request(Medium::Direct(vec![1]))),
        Reply::Corrupt
    );
}

#[test]
fn inline_bytes_travel_in_a_file_sealed_against_every_change() {
    let fd = inline_file(b"abc").unwrap();
    let seals = rustix::fs::fcntl_get_seals(&fd).unwrap();
    assert!(
        seals.contains(SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE | SealFlags::SEAL)
    );
    let mut out = [0u8; 3];
    rustix::io::pread(&fd, &mut out, 0).unwrap();
    assert_eq!(&out, b"abc");
    assert!(rustix::io::pwrite(&fd, b"x", 0).is_err());
}

/// A stand-in reply's memory file of `len` bytes with `seals`.
fn reply_file(len: u64, seals: SealFlags) -> std::os::fd::OwnedFd {
    let fd =
        rustix::fs::memfd_create("reply", MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING).unwrap();
    rustix::fs::ftruncate(&fd, len).unwrap();
    if !seals.is_empty() {
        rustix::fs::fcntl_add_seals(&fd, seals).unwrap();
    }
    fd
}

fn header(pooled: bool) -> ReplyHeader {
    ReplyHeader {
        status: Status::Ok,
        id: 1,
        width: 1,
        height: 1,
        ignored: 0,
        pooled,
        reason: Vec::new(),
    }
}

#[test]
fn a_replys_seals_are_checked_before_its_size() {
    let r = request(Medium::Direct(Vec::new()));
    let sealed = SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE;
    // unsealed and of the wrong size: the seals fail it first, since until
    // the shrink seal holds the size could change under a mapping
    let both = reply_file(5, SealFlags::empty());
    assert_eq!(
        pixels(&both, &header(false), &r),
        Err("a reply's memory file is not sealed")
    );
    let pooled_both = reply_file(5, SealFlags::SHRINK);
    assert_eq!(
        pixels(&pooled_both, &header(true), &r),
        Err("a reply's memory file is not sealed")
    );
    // sealed, the size is read next
    let wrong = reply_file(5, sealed);
    assert_eq!(
        pixels(&wrong, &header(false), &r),
        Err("a reply's memory file is not exactly the size asked for")
    );
    assert_eq!(
        pixels(&reply_file(4, sealed), &header(false), &r),
        Ok(vec![0; 4])
    );
    let pooled = reply_file(4, SealFlags::SHRINK | SealFlags::GROW);
    assert_eq!(pixels(&pooled, &header(true), &r), Ok(vec![0; 4]));
}

/// The variable that tells this test binary, run again in a user
/// namespace, to take the namespace's side of the root case.
const AS_ROOT: &str = "VTIUM_TEST_IMAGE_HELPER_AS_ROOT";

/// The capability lines of `/proc/<process>/status`.
fn capability_lines(process: &str) -> Vec<String> {
    let status = std::fs::read_to_string(format!("/proc/{process}/status")).unwrap();
    ["CapInh:", "CapPrm:", "CapEff:", "CapBnd:", "CapAmb:"]
        .iter()
        .map(|name| {
            let value = status
                .lines()
                .find_map(|l| l.strip_prefix(name))
                .unwrap_or_default()
                .trim();
            format!("{name} {value}")
        })
        .collect()
}

/// A root helper's capabilities, as `start::start` drops them, tried in a
/// user namespace where this test binary runs again as uid 0 with every
/// capability: the helper it starts as uid 0 holds none, its bounding set
/// included. In the namespace the test first raises its own inheritable
/// set to every capability it holds, whatever this machine's inheritable
/// set is, since a start as uid 0 grants the inheritable set together
/// with the bounding set: so a start that skips clearing the sets hands
/// the helper every capability, and one that skips emptying the bounding
/// set leaves the helper's bounding set full, and either turns this test
/// red. A kernel or system that gives no such namespace, or a security
/// module that refuses the namespace's capabilities, as AppArmor does on
/// Ubuntu, skips the case and says so.
#[test]
fn a_root_helper_starts_with_no_capability() {
    if std::env::var_os(AS_ROOT).is_some() {
        as_root();
        return;
    }
    let run = std::process::Command::new("unshare")
        .args(["--user", "--map-root-user", "--"])
        .arg(std::env::current_exe().unwrap())
        .args([
            "image_helper::tests::a_root_helper_starts_with_no_capability",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(AS_ROOT, "1")
        .output();
    match run {
        Ok(out) if out.status.code() == Some(77) => println!(
            "skipped: {}",
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .find(|l| l.starts_with("skip:"))
                .unwrap_or_default()
        ),
        Ok(out) => assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(e) => println!("skipped: no unshare to run it: {e}"),
    }
}

/// The namespace's side: as uid 0 with every capability, the inheritable
/// set raised to them all, a stand-in started through `start::start` as
/// uid 0, its capabilities read from `/proc` once it runs. Exits 77 when
/// the namespace gives no capability to use.
fn as_root() {
    use rustix::thread::CapabilitySets;
    let none = "0000000000000000";
    let skip = |why: String| -> ! {
        println!("skip: {why}");
        std::process::exit(77);
    };
    // the sets are this thread's, which forks the child
    let before = capability_lines("thread-self");
    if rustix::process::geteuid().as_raw() != 0
        || before.iter().any(|l| l == &format!("CapEff: {none}"))
    {
        skip(format!(
            "the user namespace gives no capability: {before:?}"
        ));
    }
    let held = rustix::thread::capabilities(None).unwrap();
    rustix::thread::set_capabilities(
        None,
        CapabilitySets {
            inheritable: held.permitted,
            ..held
        },
    )
    .unwrap();
    assert!(
        !capability_lines("thread-self").contains(&format!("CapInh: {none}")),
        "the inheritable set is raised"
    );
    // the program checks want a file this account owns that only it may
    // write, so the test binary is copied beside itself with mode 0755
    let exe = std::env::current_exe().unwrap();
    let dir = exe
        .parent()
        .unwrap()
        .join(format!("vtium-image-root-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("vtium-image");
    std::fs::copy(&exe, &path).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let program = Program::open_with(
        &path,
        [
            "image_helper::tests::stand_in_that_waits_on_its_socket",
            "--exact",
            "--ignored",
            "--test-threads=1",
        ]
        .map(Into::into)
        .to_vec(),
    )
    .unwrap();
    let root = Account {
        uid: 0,
        gid: 0,
        groups: None,
    };
    let started = match start::start(&program, &root, 64 << 20, None) {
        Ok(started) => started,
        Err(why) if why.contains("(os error 1)") => {
            let _ = std::fs::remove_dir_all(&dir);
            skip(format!(
                "the user namespace's capabilities are refused: {why}"
            ))
        }
        Err(why) => panic!("{why}"),
    };
    // the stand-in writes once it runs, so its capabilities are the
    // started program's, not the child's before it started it
    let mut fds = [rustix::event::PollFd::new(
        &started.socket,
        rustix::event::PollFlags::IN,
    )];
    let ten = rustix::event::Timespec {
        tv_sec: 10,
        tv_nsec: 0,
    };
    let ready = rustix::event::poll(&mut fds, Some(&ten)).unwrap();
    let pid = started.pid.as_raw_nonzero().to_string();
    let helper = if ready > 0 {
        capability_lines(&pid)
    } else {
        Vec::new()
    };
    start::kill_group(&started);
    start::reap(&started, true);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ready > 0, "the stand-in did not run in ten seconds");
    for line in &helper {
        assert!(
            line.ends_with(none),
            "a capability is left: {line}\n{helper:?}"
        );
    }
}

/// The stand-in the root case starts, this test binary run again: it
/// sends one packet on the socket on descriptor 3 once it runs, and waits
/// there until it is killed. It acts only with the empty environment and
/// the socket on descriptor 3 a helper starts with, and otherwise does
/// nothing.
#[test]
#[ignore = "the stand-in helper of the root case, which starts it"]
fn stand_in_that_waits_on_its_socket() {
    use std::os::unix::fs::FileTypeExt;
    let socket = std::fs::metadata("/proc/self/fd/3").is_ok_and(|m| m.file_type().is_socket());
    if !socket || std::env::vars_os().next().is_some() {
        return;
    }
    #[expect(unsafe_code, reason = "the socket start::start put on descriptor 3")]
    // SAFETY: descriptor 3 is open, as just checked, and stays open for
    // this process's life, since nothing in it closes descriptor 3.
    let socket = unsafe { std::os::fd::BorrowedFd::borrow_raw(3) };
    rustix::net::send(socket, b"running", rustix::net::SendFlags::empty()).unwrap();
    let mut byte = [0u8; 1];
    while rustix::io::read(socket, &mut byte).is_ok_and(|n| n > 0) {}
}
