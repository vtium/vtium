//! The image helper's host against stand-in helpers (ADR-1101 D3, D7,
//! D8): the child's setup, the start from the opened program, the
//! deadline, the crash limit, and each check of a reply.
//!
//! A stand-in is this binary run again, from the program file the host
//! opened, with `--stand-in ROLE` before the host's own `--tmpdir`
//! argument. It speaks the real wire through vtium-image's encoding and
//! refuses what the real helper refuses: it exits on any request that
//! `decode_request` refuses, as `vtium-image` does. It has its own
//! `main`, so a stand-in has the one thread a helper has.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::panic,
    clippy::cast_possible_truncation,
    reason = "a test fails by panicking"
)]

use std::io::{IoSlice, IoSliceMut};
use std::mem::MaybeUninit;
use std::os::fd::{AsFd, BorrowedFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};

use rustix::event::{PollFd, PollFlags, Timespec};
use rustix::fs::{MemfdFlags, SealFlags};
use rustix::net::{
    RecvAncillaryBuffer, RecvFlags, SendAncillaryBuffer, SendAncillaryMessage, SendFlags,
};
use vtium_image::decode::wire::{self, Hello, ReplyHeader, Status};
use vtium_image::decode::{Alpha, Description, Layout, Medium, Payload, Reply, Request};
use vtium_platform::image_helper::{ImageHost, Program, Session, memory_cap};

// ---- the stand-in's side ----

fn stand_in(role: &str, tmpdir: Option<&str>) -> ! {
    #[expect(unsafe_code, reason = "the socket the host put on descriptor 3")]
    // SAFETY: the host starts a stand-in with its socket on descriptor 3,
    // which nothing else in this process owns.
    let socket = unsafe { OwnedFd::from_raw_fd(3) };
    if role == "report" {
        report(tmpdir.map(Path::new).expect("a --tmpdir for the report"));
    }
    if role != "no-hello" {
        let hello = Hello {
            version: wire::VERSION,
            landlock: 0,
            seccomp: false,
        };
        send(&socket, &wire::encode_hello(&hello), None);
    }
    loop {
        let mut buf = vec![0u8; wire::PACKET_MAX + 1];
        let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(4))];
        let mut control = RecvAncillaryBuffer::new(&mut space);
        let got = rustix::net::recvmsg(
            &socket,
            &mut [IoSliceMut::new(&mut buf)],
            &mut control,
            RecvFlags::CMSG_CLOEXEC,
        );
        let Ok(got) = got else { std::process::exit(1) };
        if got.bytes == 0 {
            std::process::exit(0);
        }
        // the real helper exits on every request it refuses
        let Ok((id, request)) = wire::decode_request(&buf[..got.bytes]) else {
            std::process::exit(2);
        };
        match role {
            "hang" => {
                if let Some(dir) = tmpdir {
                    // a worker-like child in the helper's process group
                    let child = std::process::Command::new("/bin/sleep")
                        .arg("100")
                        .spawn()
                        .unwrap();
                    std::fs::write(Path::new(dir).join("worker"), child.id().to_string()).unwrap();
                    std::mem::forget(child);
                }
                loop {
                    let _ = rustix::event::poll(&mut [], None);
                }
            }
            "crash" => std::process::exit(0),
            "once" => {
                answer(&socket, role, id, &request);
                std::process::exit(0);
            }
            _ => answer(&socket, role, id, &request),
        }
    }
}

/// The reply a role gives `request`: a picture of its described size, or
/// one pixel for PNG, made wrong in the way the role names.
fn answer(socket: &OwnedFd, role: &str, id: u64, request: &Request) {
    let (w, h) = request.pixels.map_or((1, 1), |d| (d.width, d.height));
    let mut header = ReplyHeader {
        status: Status::Ok,
        id: if role == "lie-id" { id + 1 } else { id },
        width: if role == "lie-big" { 101 } else { w },
        height: h,
        ignored: 0,
        pooled: role.starts_with("pooled"),
        reason: Vec::new(),
    };
    if request.discard {
        send(socket, &wire::encode_reply(&header), None);
        return;
    }
    let fd = rustix::fs::memfd_create("stand-in", MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING)
        .unwrap();
    let mut len = u64::from(w * h * 4);
    if role == "lie-size" {
        len += 4;
    }
    rustix::fs::ftruncate(&fd, len).unwrap();
    rustix::io::pwrite(&fd, &vec![0x10; len as usize], 0).unwrap();
    let seals = match role {
        "lie-unsealed" | "pooled-unsealed" => SealFlags::empty(),
        "lie-future-write" => SealFlags::SHRINK | SealFlags::GROW | SealFlags::FUTURE_WRITE,
        r if r.starts_with("pooled") => SealFlags::SHRINK | SealFlags::GROW,
        _ => SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE,
    };
    if !seals.is_empty() {
        rustix::fs::fcntl_add_seals(&fd, seals).unwrap();
    }
    if role == "lie-fd" {
        header.status = Status::Corrupt;
        header.width = 0;
        header.height = 0;
    }
    if role == "lie-ignored" {
        header.ignored = 1;
        header.status = Status::Unreadable;
        header.width = 0;
        header.height = 0;
        send(socket, &wire::encode_reply(&header), None);
        return;
    }
    send(socket, &wire::encode_reply(&header), Some(fd.as_fd()));
}

fn send(socket: &OwnedFd, bytes: &[u8], fd: Option<BorrowedFd<'_>>) {
    let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    let fds: Vec<BorrowedFd<'_>> = fd.into_iter().collect();
    if !fds.is_empty() {
        assert!(control.push(SendAncillaryMessage::ScmRights(&fds)));
    }
    rustix::net::sendmsg(
        socket,
        &[IoSlice::new(bytes)],
        &mut control,
        SendFlags::NOSIGNAL,
    )
    .unwrap();
}

/// What the stand-in started as, written to `dir/report`, one fact a
/// line.
fn report(dir: &Path) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let field = |name: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let limits = std::fs::read_to_string("/proc/self/limits").unwrap();
    let limit = |name: &str| {
        limits
            .lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l[name.len()..].split_whitespace().next().map(str::to_owned))
            .unwrap_or_default()
    };
    let mut fds: Vec<i32> = std::fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
        .collect();
    fds.sort_unstable();
    // the listing's own descriptor is the highest
    fds.pop();
    let pid = rustix::process::getpid();
    let sid = rustix::process::getsid(None).unwrap();
    let tty = std::fs::File::open("/dev/tty").is_ok();
    let exe = std::fs::metadata("/proc/self/exe")
        .map(|m| std::os::unix::fs::MetadataExt::ino(&m))
        .unwrap_or(0);
    let lines = [
        format!("uid {}", field("Uid:")),
        format!("gid {}", field("Gid:")),
        format!("nnp {}", field("NoNewPrivs:")),
        format!("capamb {}", field("CapAmb:")),
        format!("sigblk {}", field("SigBlk:")),
        format!("leader {}", pid == sid),
        format!("tty {tty}"),
        format!("cwd {}", std::env::current_dir().unwrap().display()),
        format!("env {}", std::env::vars_os().count()),
        format!("fds {fds:?}"),
        format!("as {}", limit("Max address space")),
        format!("core {}", limit("Max core file size")),
        format!("argv0 {}", std::env::args().next().unwrap_or_default()),
        format!("exe {exe}"),
    ];
    std::fs::write(dir.join("report"), lines.join("\n")).unwrap();
}

// ---- the host's side ----

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vtium-image-host-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A copy of this test binary in `dir`, its mode 0755, which the host's
/// program checks take, opened to start `role`.
fn program(dir: &Path, role: &str) -> Program {
    let path = dir.join("vtium-image");
    if !path.exists() {
        std::fs::copy(std::env::current_exe().unwrap(), &path).unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
    }
    Program::open_with(&path, vec!["--stand-in".into(), role.into()]).unwrap()
}

fn me() -> u32 {
    rustix::process::geteuid().as_raw()
}

fn session() -> Session {
    Session {
        child: None,
        pty_owner: Ok(me()),
    }
}

fn host(dir: &Path, role: &str) -> ImageHost {
    ImageHost::new(Some(program(dir, role)), Some(dir.to_owned()))
}

fn raw(medium: Medium) -> Request {
    Request {
        medium,
        offset: 0,
        max: 0,
        min: 12,
        inflate: false,
        png: false,
        exact: false,
        discard: false,
        any_image: false,
        pixels: Some(Description {
            layout: Layout::Rgb,
            alpha: Alpha::Straight,
            width: 2,
            height: 2,
            compressed: false,
        }),
        largest: (100, 100),
        largest_bytes: 1 << 20,
    }
}

fn file() -> Request {
    raw(Medium::File(b"/x".to_vec()))
}

/// Waits for the reply at most ten seconds a packet; `None` when nothing
/// came.
fn wait(host: &mut ImageHost) -> Option<Reply> {
    for _ in 0..3 {
        let socket = host.socket()?;
        let mut fds = [PollFd::new(&socket, PollFlags::IN)];
        let ten = Timespec {
            tv_sec: 10,
            tv_nsec: 0,
        };
        if rustix::event::poll(&mut fds, Some(&ten)).unwrap() == 0 {
            return None;
        }
        if let Some(reply) = host.readable() {
            return Some(reply);
        }
    }
    None
}

fn ask(host: &mut ImageHost, request: &Request) -> Reply {
    match host.send(request, &session()) {
        Err(answer) => answer,
        Ok(()) => wait(host).expect("a reply in ten seconds"),
    }
}

fn gone(pid: u32) -> bool {
    !Path::new(&format!("/proc/{pid}")).exists()
}

fn the_child_starts_alone_as_the_account_with_its_cap_and_nothing_else() {
    let dir = scratch("setup");
    let mut host = host(&dir, "report");
    let picture = ask(&mut host, &file());
    assert!(
        matches!(
            picture,
            Reply::Ok(Payload::Rgba {
                width: 2,
                height: 2,
                ..
            })
        ),
        "{picture:?}"
    );
    let report = std::fs::read_to_string(dir.join("report")).unwrap();
    let line = |key: &str| {
        report
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{key} ")))
            .unwrap_or_default()
            .to_owned()
    };
    let uid = me().to_string();
    assert_eq!(
        line("uid"),
        [uid.as_str(); 4].join(" "),
        "real, effective, saved and fs ids"
    );
    let gid = rustix::process::getegid().as_raw().to_string();
    assert_eq!(line("gid"), [gid.as_str(); 4].join(" "));
    assert_eq!(line("nnp"), "1", "no new privileges");
    assert_eq!(
        line("capamb"),
        "0000000000000000",
        "no ambient capabilities"
    );
    assert_eq!(line("sigblk"), "0000000000000000", "no blocked signal");
    assert_eq!(line("leader"), "true", "a session of its own");
    assert_eq!(line("tty"), "false", "no controlling terminal");
    assert_eq!(line("cwd"), "/");
    assert_eq!(line("env"), "0", "an empty environment");
    assert_eq!(line("fds"), "[0, 1, 2, 3]", "stdio and its socket only");
    assert_eq!(
        line("as"),
        memory_cap(1 << 20).to_string(),
        "its memory cap"
    );
    assert_eq!(line("core"), "0");
    assert_eq!(line("argv0"), dir.join("vtium-image").display().to_string());
    println!("test the_child_starts_alone_as_the_account_with_its_cap_and_nothing_else ... ok");
}

fn every_helper_starts_from_the_program_opened_at_start() {
    let dir = scratch("opened");
    let mut host = host(&dir, "report");
    let path = dir.join("vtium-image");
    let old = std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(&path).unwrap());
    // an upgrade renames a new program into place
    let new = dir.join("vtium-image.new");
    std::fs::copy(&path, &new).unwrap();
    std::fs::rename(&new, &path).unwrap();
    ask(&mut host, &file());
    let report = std::fs::read_to_string(dir.join("report")).unwrap();
    assert!(
        report.contains(&format!("exe {old}")),
        "the old program ran: {report}"
    );
    println!("test every_helper_starts_from_the_program_opened_at_start ... ok");
}

fn a_request_unanswered_by_its_deadline_fails_and_kills_the_group_without_a_crash() {
    let dir = scratch("deadline");
    let mut host = host(&dir, "hang");
    host.send(&file(), &session()).unwrap();
    let helper = host.pid().unwrap();
    // the hello comes; the reply never does
    assert_eq!(host.readable(), None);
    let deadline = host.deadline().unwrap();
    let just_before = deadline - std::time::Duration::from_nanos(1);
    assert_eq!(host.expired_at(just_before), None);
    // the stand-in's worker-like child has started
    let worker_file = dir.join("worker");
    for _ in 0..1000 {
        if worker_file.exists() {
            break;
        }
        let _ = rustix::event::poll(
            &mut [],
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 10_000_000,
            }),
        );
    }
    let worker: u32 = std::fs::read_to_string(&worker_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(host.expired_at(deadline), Some(Reply::Failed));
    assert!(!host.busy());
    assert_eq!(host.crashes(), 0, "a timeout is no crash");
    assert!(host.available());
    // the group is killed, the helper reaped; the worker, which the
    // helper never reaps, is gone once init reaps it
    for _ in 0..1000 {
        if gone(helper)
            && std::fs::read_to_string(format!("/proc/{worker}/stat"))
                .map_or(true, |s| s.contains(") Z "))
        {
            break;
        }
        let _ = rustix::event::poll(
            &mut [],
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 10_000_000,
            }),
        );
    }
    assert!(gone(helper), "the helper is reaped");
    let worker_state = std::fs::read_to_string(format!("/proc/{worker}/stat")).unwrap_or_default();
    assert!(
        worker_state.is_empty() || worker_state.contains(") Z "),
        "the worker was killed: {worker_state}"
    );
    println!(
        "test a_request_unanswered_by_its_deadline_fails_and_kills_the_group_without_a_crash ... ok"
    );
}

fn three_crashes_stop_the_consoles_helpers_until_its_session_ends() {
    let dir = scratch("crashes");
    let mut host = host(&dir, "crash");
    for n in 1..=3 {
        assert_eq!(ask(&mut host, &file()), Reply::Failed, "crash {n}");
        assert_eq!(host.crashes(), n);
    }
    assert!(!host.available());
    // answered as without a helper, and none started
    assert_eq!(host.send(&file(), &session()), Err(Reply::Unreadable));
    let inline = Request {
        inflate: true,
        pixels: Some(Description {
            compressed: true,
            ..file().pixels.unwrap()
        }),
        ..raw(Medium::Direct(vec![1]))
    };
    assert_eq!(host.send(&inline, &session()), Err(Reply::Corrupt));
    assert_eq!(host.pid(), None);
    host.session_ended();
    assert!(host.available());
    assert_eq!(host.crashes(), 0);
    assert!(
        host.send(&file(), &session()).is_ok(),
        "the next session starts a helper"
    );
    println!("test three_crashes_stop_the_consoles_helpers_until_its_session_ends ... ok");
}

fn each_reply_that_fails_a_check_fails_the_request_as_a_crash() {
    let dir = scratch("checks");
    for role in [
        "lie-id",
        "lie-unsealed",
        "lie-future-write",
        "lie-size",
        "lie-big",
        "lie-fd",
        "lie-ignored",
        "pooled-unsealed",
        "no-hello",
    ] {
        let mut host = host(&dir, role);
        assert_eq!(ask(&mut host, &file()), Reply::Failed, "{role}");
        assert_eq!(host.crashes(), 1, "{role}");
        assert!(!host.busy(), "{role}");
    }
    // a pooled file sealed against shrinking and growing is read
    let mut host = host(&dir, "pooled");
    let reply = ask(&mut host, &file());
    assert_eq!(
        reply,
        Reply::Ok(Payload::Rgba {
            width: 2,
            height: 2,
            data: vec![0x10; 16]
        })
    );
    // and a discard is answered with nothing
    let discard = Request {
        discard: true,
        min: 0,
        pixels: None,
        ..raw(Medium::TempFile(b"/tmp/tty-graphics-protocol-x".to_vec()))
    };
    assert_eq!(ask(&mut host, &discard), Reply::Ok(Payload::Discarded));
    assert_eq!(host.crashes(), 0);
    println!("test each_reply_that_fails_a_check_fails_the_request_as_a_crash ... ok");
}

fn a_request_no_account_may_read_starts_no_helper() {
    let dir = scratch("refused");
    let mut host = host(&dir, "answer");
    let other = Session {
        child: None,
        pty_owner: Ok(me() + 1),
    };
    assert_eq!(host.send(&file(), &other), Err(Reply::Unreadable));
    assert_eq!(host.pid(), None);
    // more bytes needed than the budget is answered without the helper
    let poor = Request {
        largest_bytes: 11,
        ..file()
    };
    assert_eq!(host.send(&poor, &session()), Err(Reply::OverBudget));
    assert_eq!(host.pid(), None);
    // and with no program at all
    let mut none = ImageHost::new(None, None);
    assert!(!none.available());
    assert_eq!(none.send(&file(), &session()), Err(Reply::Unreadable));
    println!("test a_request_no_account_may_read_starts_no_helper ... ok");
}

fn a_session_that_ends_drops_its_request_and_its_helper() {
    let dir = scratch("ended");
    let mut host = host(&dir, "hang");
    host.send(&file(), &session()).unwrap();
    let helper = host.pid().unwrap();
    host.session_ended();
    assert!(!host.busy());
    assert_eq!(host.pid(), None);
    for _ in 0..1000 {
        if gone(helper) {
            break;
        }
        let _ = rustix::event::poll(
            &mut [],
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 10_000_000,
            }),
        );
    }
    assert!(gone(helper));
    println!("test a_session_that_ends_drops_its_request_and_its_helper ... ok");
}

/// The program is opened above descriptor 3 even while 3 is free, since
/// the child puts its socket there before it starts the program. This
/// binary runs again with descriptor 3 closed, opens a stand-in and asks
/// it for a picture.
fn the_program_never_lies_on_the_childs_socket_descriptor() {
    let mut again = std::process::Command::new(std::env::current_exe().unwrap());
    again.arg("--fd-three");
    #[expect(unsafe_code, reason = "closing descriptor 3 between fork and exec")]
    // SAFETY: close(2) touches no memory; the child's descriptor 3, if it
    // has one, is its own copy, which nothing in the child uses.
    unsafe {
        std::os::unix::process::CommandExt::pre_exec(&mut again, || {
            libc::close(3);
            Ok(())
        });
    }
    let status = again.status().unwrap();
    assert!(status.success(), "{status}");
    println!("test the_program_never_lies_on_the_childs_socket_descriptor ... ok");
}

/// The run with descriptor 3 free and stdin closed, so the program's
/// first descriptor takes 0 and its move above stdio could land on 3.
fn with_three_free() -> ! {
    let dir = scratch("three");
    let path = dir.join("vtium-image");
    std::fs::copy(std::env::current_exe().unwrap(), &path).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let probe = std::fs::File::open("/dev/null").unwrap();
    assert_eq!(
        std::os::fd::AsRawFd::as_raw_fd(&probe),
        3,
        "descriptor 3 is free"
    );
    drop(probe);
    #[expect(
        unsafe_code,
        reason = "freeing descriptor 0 so the program's open takes it"
    )]
    // SAFETY: nothing in this run reads stdin, and std's handle to it is
    // never used after this.
    unsafe {
        libc::close(0);
    }
    let program = Program::open_with(&path, vec!["--stand-in".into(), "answer".into()]).unwrap();
    let mut host = ImageHost::new(Some(program), Some(dir));
    let reply = ask(&mut host, &file());
    assert!(matches!(reply, Reply::Ok(_)), "{reply:?}");
    std::process::exit(0);
}

/// A helper that has exited on its own, as one does after its idle
/// interval, is replaced at the next request, and no crash is counted.
fn a_helper_that_exited_idle_is_replaced_at_the_next_request() {
    let dir = scratch("idle");
    let mut host = host(&dir, "once");
    assert!(matches!(ask(&mut host, &file()), Reply::Ok(_)));
    let first = host.pid().unwrap();
    for _ in 0..1000 {
        let state = std::fs::read_to_string(format!("/proc/{first}/stat")).unwrap_or_default();
        if state.contains(") Z ") {
            break;
        }
        let _ = rustix::event::poll(
            &mut [],
            Some(&Timespec {
                tv_sec: 0,
                tv_nsec: 10_000_000,
            }),
        );
    }
    assert!(matches!(ask(&mut host, &file()), Reply::Ok(_)));
    assert_ne!(host.pid().unwrap(), first);
    assert!(gone(first), "the exited helper is reaped");
    assert_eq!(host.crashes(), 0);
    println!("test a_helper_that_exited_idle_is_replaced_at_the_next_request ... ok");
}

fn a_program_others_may_change_is_refused() {
    let dir = scratch("program");
    let path = dir.join("vtium-image");
    std::fs::copy(std::env::current_exe().unwrap(), &path).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o757)).unwrap();
    let why = Program::open(&path).unwrap_err();
    assert!(
        why.contains("may be written by its group or by others"),
        "{why}"
    );
    let why = Program::open(Path::new("relative/vtium-image")).unwrap_err();
    assert!(why.contains("not absolute"), "{why}");
    println!("test a_program_others_may_change_is_refused ... ok");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--fd-three") {
        with_three_free();
    }
    if args.first().map(String::as_str) == Some("--stand-in") {
        let role = args.get(1).cloned().unwrap_or_default();
        let tmpdir = args.iter().find_map(|a| a.strip_prefix("--tmpdir="));
        stand_in(&role, tmpdir);
    }
    the_program_never_lies_on_the_childs_socket_descriptor();
    the_child_starts_alone_as_the_account_with_its_cap_and_nothing_else();
    every_helper_starts_from_the_program_opened_at_start();
    a_request_unanswered_by_its_deadline_fails_and_kills_the_group_without_a_crash();
    three_crashes_stop_the_consoles_helpers_until_its_session_ends();
    each_reply_that_fails_a_check_fails_the_request_as_a_crash();
    a_request_no_account_may_read_starts_no_helper();
    a_session_that_ends_drops_its_request_and_its_helper();
    a_helper_that_exited_idle_is_replaced_at_the_next_request();
    a_program_others_may_change_is_refused();
}
