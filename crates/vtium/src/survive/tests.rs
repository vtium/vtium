#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
#![expect(clippy::indexing_slicing, reason = "a test indexes what it built")]

use std::ffi::OsString;
use std::io::IoSliceMut;
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::process::Command;

use rustix::event::{PollFd, PollFlags, poll};
use rustix::net::{
    AddressFamily, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, SocketAddrUnix,
    SocketFlags, SocketType, bind, recvmsg, socket_with, sockopt,
};
use vtium_platform::pty::{Erase, Pty, Size};

use super::*;
use crate::scratch::Scratch;

const SIZE: Size = Size {
    cols: 80,
    rows: 25,
    cell_width: 8,
    cell_height: 16,
};

/// A session that runs until its pty hangs up.
fn session() -> Pty {
    let mut sleeper = Command::new("sleep");
    sleeper.arg("1000");
    Pty::spawn(sleeper, SIZE, Erase::Delete).unwrap()
}

/// The name of the pty whose master is `fd`, `/dev/pts/N`, which no other
/// open pty shares.
fn pts(fd: impl std::os::fd::AsFd) -> std::ffi::CString {
    rustix::pty::ptsname(fd, Vec::new()).unwrap()
}

/// One descriptor in the stand-in's store.
struct Entry {
    name: String,
    fd: OwnedFd,
    poll: bool,
}

/// A stand-in for systemd's notification socket and file-descriptor
/// store, which checks what systemd 262 checks (sd_notify(3),
/// systemd.service(5)): it takes a message only from the service's main
/// process (`NotifyAccess=main`, read from the sender's credentials);
/// stores descriptors only with `FDSTORE=1`, closing any sent without it;
/// names each by a valid `FDNAME=`, else `stored`, since systemd ignores a
/// name that is too long or holds a control character or a `:`; refuses a
/// descriptor past `FileDescriptorStoreMax=`; removes every descriptor of a
/// name on `FDSTOREREMOVE=1`, which needs `FDNAME=`; and closes a stored
/// descriptor that reports a hangup or an error unless it came with
/// `FDPOLL=0`. At a restart it passes copies of what it holds, in the
/// order stored, with `LISTEN_PID`, `LISTEN_FDS` and `LISTEN_FDNAMES`. It
/// counts each `READY=1`, which a `Type=notify` unit's start waits for.
struct Systemd {
    socket: OwnedFd,
    path: PathBuf,
    max: usize,
    main_pid: u32,
    store: Vec<Entry>,
    ready: usize,
}

impl Systemd {
    fn new(dir: &Path, max: usize) -> Systemd {
        let path = dir.join("notify");
        let socket = socket_with(
            AddressFamily::UNIX,
            SocketType::DGRAM,
            SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
            None,
        )
        .unwrap();
        bind(&socket, &SocketAddrUnix::new(&path).unwrap()).unwrap();
        sockopt::set_socket_passcred(&socket, true).unwrap();
        Systemd {
            socket,
            path,
            max,
            main_pid: std::process::id(),
            store: Vec::new(),
            ready: 0,
        }
    }

    /// The environment systemd gives the service.
    fn env(&self) -> Vec<(OsString, OsString)> {
        vec![
            ("NOTIFY_SOCKET".into(), self.path.clone().into_os_string()),
            ("FDSTORE".into(), self.max.to_string().into()),
        ]
    }

    /// Reads every message waiting.
    fn pump(&mut self) {
        loop {
            let mut bytes = [0_u8; 4096];
            let mut space =
                [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(8), ScmCredentials(1))];
            let mut control = RecvAncillaryBuffer::new(&mut space);
            let Ok(got) = recvmsg(
                &self.socket,
                &mut [IoSliceMut::new(&mut bytes)],
                &mut control,
                RecvFlags::CMSG_CLOEXEC,
            ) else {
                return;
            };
            let mut fds = Vec::new();
            let mut sender = None;
            for message in control.drain() {
                match message {
                    RecvAncillaryMessage::ScmRights(received) => fds.extend(received),
                    RecvAncillaryMessage::ScmCredentials(cred) => {
                        sender = u32::try_from(cred.pid.as_raw_nonzero().get()).ok();
                    }
                    _ => {}
                }
            }
            if sender != Some(self.main_pid) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes[..got.bytes]).into_owned();
            self.message(&text, fds);
        }
    }

    fn message(&mut self, text: &str, fds: Vec<OwnedFd>) {
        let field = |key: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        };
        if field("READY") == Some("1") {
            self.ready = self.ready.saturating_add(1);
        }
        let name = field("FDNAME").filter(|n| storable(n));
        if field("FDSTOREREMOVE") == Some("1")
            && let Some(name) = name
        {
            self.store.retain(|e| e.name != name);
        }
        if field("FDSTORE") != Some("1") {
            return;
        }
        let poll = field("FDPOLL") != Some("0");
        for fd in fds {
            if self.store.len() >= self.max {
                continue;
            }
            self.store.push(Entry {
                name: name.unwrap_or("stored").to_owned(),
                fd,
                poll,
            });
        }
    }

    /// The service starts again: closes what reported a hangup, then
    /// passes copies of the rest. Returns the environment and the
    /// descriptors as the service finds them from descriptor 3 up.
    fn restart(&mut self) -> (Vec<(OsString, OsString)>, Vec<OwnedFd>) {
        self.pump();
        self.store.retain(|e| {
            if !e.poll {
                return true;
            }
            let mut fds = [PollFd::new(&e.fd, PollFlags::empty())];
            poll(&mut fds, Some(&rustix::time::Timespec::default())).unwrap();
            !fds[0].revents().intersects(PollFlags::HUP | PollFlags::ERR)
        });
        let names: Vec<&str> = self.store.iter().map(|e| e.name.as_str()).collect();
        let env = vec![
            ("LISTEN_PID".into(), self.main_pid.to_string().into()),
            ("LISTEN_FDS".into(), self.store.len().to_string().into()),
            ("LISTEN_FDNAMES".into(), names.join(":").into()),
        ];
        let fds = self
            .store
            .iter()
            .map(|e| e.fd.try_clone().unwrap())
            .collect();
        (env, fds)
    }

    fn names(&mut self) -> Vec<String> {
        self.pump();
        self.store.iter().map(|e| e.name.clone()).collect()
    }
}

/// What a vtium that starts again takes back from `systemd`.
fn restarted(systemd: &mut Systemd) -> Kept {
    let (env, fds) = systemd.restart();
    let count = fds.len();
    Kept::new(notify::passed(&env, std::process::id(), |n| {
        assert_eq!(n, count, "the count passed is LISTEN_FDS");
        Ok(fds)
    }))
}

#[test]
fn a_name_holds_its_head_and_console() {
    let name = Name::new("left.panel", 12);
    assert_eq!(name.encode(), "pty.12.left.panel");
    assert_eq!(Name::parse("pty.12.left.panel"), Some(name));
    assert!(storable("pty.12.left.panel"));
    // systemd's name for a descriptor stored without one, and names vtium
    // never writes, place nothing
    for other in [
        "stored",
        "unknown",
        "pty.x.console",
        "pty.01.console",
        "pty.1.",
        "pty.1.a:b",
    ] {
        assert_eq!(Name::parse(other), None, "{other}");
    }
    assert!(!storable("a:b"));
    assert!(!storable("a\nb"));
    assert!(!storable(&"x".repeat(NAME_MAX + 1)));
}

#[test]
fn no_session_inherits_systemds_word_to_vtium() {
    let env: Vec<(OsString, OsString)> = [
        "LISTEN_FDS",
        "LISTEN_PID",
        "LISTEN_FDNAMES",
        "NOTIFY_SOCKET",
        "FDSTORE",
        "INVOCATION_ID",
        "JOURNAL_STREAM",
        "SYSTEMD_EXEC_PID",
        "MEMORY_PRESSURE_WATCH",
        "MEMORY_PRESSURE_WRITE",
        "HOME",
    ]
    .iter()
    .map(|k| ((*k).into(), "1".into()))
    .collect();
    let kept: Vec<OsString> = session_env(env).into_iter().map(|(k, _)| k).collect();
    assert_eq!(kept, [OsString::from("HOME")]);
}

#[test]
fn a_stored_master_comes_back_under_its_name_after_a_restart() {
    let dir = Scratch::new("survive-store");
    let mut systemd = Systemd::new(&dir, 50);
    let store = notify::Store::from_env(&systemd.env()).unwrap();
    let first = session();
    let second = session();
    store
        .keep(&Name::new("console", 1).encode(), first.master())
        .unwrap();
    store
        .keep(&Name::new("right", 3).encode(), second.master())
        .unwrap();
    assert_eq!(systemd.names(), ["pty.1.console", "pty.3.right"]);

    let mut kept = restarted(&mut systemd);
    let one = kept
        .take("console", 1)
        .expect("console 1's master comes back");
    let three = kept.take("right", 3).expect("right's console 3 comes back");
    assert_eq!(pts(&one), pts(first.master()));
    assert_eq!(pts(&three), pts(second.master()));
    assert!(kept.is_empty());
}

#[test]
fn a_forgotten_master_is_not_passed_back() {
    let dir = Scratch::new("survive-forget");
    let mut systemd = Systemd::new(&dir, 50);
    let store = notify::Store::from_env(&systemd.env()).unwrap();
    let pty = session();
    let name = Name::new("console", 1).encode();
    store.keep(&name, pty.master()).unwrap();
    store.forget(&name).unwrap();
    let kept = restarted(&mut systemd);
    assert!(kept.is_empty());
}

// systemd closes a stored descriptor that reports a hangup unless it came
// with FDPOLL=0; vtium decides when a session has ended, so a master whose
// pty reports one is kept until vtium drops it.
#[test]
fn the_store_keeps_a_master_that_reports_a_hangup() {
    let dir = Scratch::new("survive-fdpoll");
    let mut systemd = Systemd::new(&dir, 50);
    let store = notify::Store::from_env(&systemd.env()).unwrap();
    let mut done = Command::new("true");
    done.arg("");
    let pty = Pty::spawn(done, SIZE, Erase::Delete).unwrap();
    store
        .keep(&Name::new("console", 1).encode(), pty.master())
        .unwrap();
    // the child has exited once the master reports the slave closed
    let master = pty.master();
    let mut fds = [PollFd::new(&master, PollFlags::empty())];
    poll(
        &mut fds,
        Some(&rustix::time::Timespec {
            tv_sec: 5,
            tv_nsec: 0,
        }),
    )
    .unwrap();
    assert!(fds[0].revents().contains(PollFlags::HUP));
    let mut kept = restarted(&mut systemd);
    assert!(kept.take("console", 1).is_some());
}

#[test]
fn the_store_is_off_without_systemds_variables() {
    let dir = Scratch::new("survive-off");
    let systemd = Systemd::new(&dir, 50);
    let mut env = systemd.env();
    assert!(notify::Store::from_env(&env).is_some());
    env.retain(|(k, _)| k != "FDSTORE");
    assert!(notify::Store::from_env(&env).is_none(), "no FDSTORE");
    let mut zero = systemd.env();
    zero[1].1 = "0".into();
    assert!(notify::Store::from_env(&zero).is_none(), "FDSTORE=0");
}

#[test]
fn passed_descriptors_are_vtiums_only_when_systemd_names_its_pid() {
    let env = |pid: u32, names: &str| -> Vec<(OsString, OsString)> {
        vec![
            ("LISTEN_PID".into(), pid.to_string().into()),
            ("LISTEN_FDS".into(), "2".into()),
            ("LISTEN_FDNAMES".into(), names.into()),
        ]
    };
    let two = |_: usize| {
        let a = std::fs::File::open("/dev/null").unwrap();
        let b = std::fs::File::open("/dev/null").unwrap();
        Ok(vec![OwnedFd::from(a), OwnedFd::from(b)])
    };
    let names =
        |got: Vec<(String, OwnedFd)>| -> Vec<String> { got.into_iter().map(|(n, _)| n).collect() };
    assert_eq!(names(notify::passed(&env(7, "a:b"), 7, two)), ["a", "b"]);
    assert_eq!(
        names(notify::passed(&env(7, "a"), 7, two)),
        ["a", "unknown"]
    );
    assert_eq!(
        names(notify::passed(&env(7, ""), 7, two)),
        ["unknown", "unknown"]
    );
    let untouched = notify::passed(&env(8, "a:b"), 7, |_| -> std::io::Result<Vec<OwnedFd>> {
        panic!("another process's descriptors are not taken")
    });
    assert!(untouched.is_empty());
    assert!(notify::passed(&[], 7, |_| panic!("none passed")).is_empty());
}

#[test]
fn a_master_without_a_place_is_set_apart() {
    let null = || OwnedFd::from(std::fs::File::open("/dev/null").unwrap());
    let mut kept = Kept::new(vec![
        ("pty.1.console".into(), null()),
        ("pty.1.console".into(), null()),
        ("pty.2.gone".into(), null()),
        ("stored".into(), null()),
    ]);
    assert_eq!(kept.consoles("console"), [1]);
    let apart: Vec<String> = kept
        .without_place(|head| head == "console")
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(apart, ["pty.1.console", "stored", "pty.2.gone"]);
    assert!(kept.take("console", 1).is_some());
    assert!(kept.is_empty());
}

#[test]
fn the_ledger_tells_the_store_only_what_changed() {
    let dir = Scratch::new("survive-ledger");
    let mut systemd = Systemd::new(&dir, 50);
    let mut keeper = Keeper::Store(notify::Store::from_env(&systemd.env()).unwrap());
    let mut ledger = Ledger::default();
    let first = session();
    let name = Name::new("console", 1).encode();
    ledger.sync(&mut keeper, [(name.clone(), 1, first.master())]);
    ledger.sync(&mut keeper, [(name.clone(), 1, first.master())]);
    assert_eq!(systemd.names(), ["pty.1.console"], "kept once");
    // the console's program starts again on a new pty
    let second = session();
    ledger.sync(&mut keeper, [(name.clone(), 2, second.master())]);
    assert_eq!(systemd.names(), ["pty.1.console"]);
    let mut kept = restarted(&mut systemd);
    let back = kept.take("console", 1).unwrap();
    assert_eq!(
        pts(&back),
        pts(second.master()),
        "the new pty replaced the old"
    );
    ledger.sync(&mut keeper, []);
    assert_eq!(
        systemd.names(),
        Vec::<String>::new(),
        "a console gone is forgotten"
    );
    assert!(!ledger.holds(&name));
}

// The holder keeps what one vtium gave it for the next, and exits once a
// vtium leaves it nothing.
#[test]
fn the_holder_keeps_masters_from_one_vtium_for_the_next() {
    let dir = Scratch::new("survive-hold");
    let path = hold::socket_path(&dir);
    let listener = socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .unwrap();
    bind(&listener, &SocketAddrUnix::new(&path).unwrap()).unwrap();
    rustix::net::listen(&listener, 4).unwrap();
    let holder = std::thread::spawn(move || hold::serve(listener));

    let first = session();
    let second = session();
    {
        let vtium = hold::Client::connect(&path).unwrap();
        assert!(
            vtium.take().unwrap().is_empty(),
            "a new holder holds nothing"
        );
        vtium
            .keep(&Name::new("console", 1).encode(), first.master())
            .unwrap();
        vtium
            .keep(&Name::new("console", 2).encode(), second.master())
            .unwrap();
        vtium.forget(&Name::new("console", 2).encode()).unwrap();
        // a name vtium never writes is turned away
        vtium.keep("stored", second.master()).unwrap();
        // vtium crashes: its connection closes
    }
    let next = hold::Client::connect(&path).unwrap();
    let mut kept = Kept::new(next.take().unwrap());
    let back = kept.take("console", 1).expect("console 1 is held");
    assert_eq!(pts(&back), pts(first.master()));
    assert!(kept.is_empty(), "console 2 was forgotten");
    next.forget(&Name::new("console", 1).encode()).unwrap();
    drop(next);
    assert!(holder.join().unwrap().is_ok(), "the holder exits empty");
}

// A keeper that fails, here a store whose socket is gone, is given up
// after one failure: the ledger holds nothing and tells it nothing more.
#[test]
fn a_failing_keeper_is_given_up() {
    let dir = Scratch::new("survive-gone");
    let gone = dir.join("no-socket");
    let mut keeper =
        Keeper::Store(notify::Store::open(gone.as_os_str().as_encoded_bytes(), 50).unwrap());
    let mut ledger = Ledger::default();
    let pty = session();
    let name = Name::new("console", 1).encode();
    ledger.sync(&mut keeper, [(name.clone(), 1, pty.master())]);
    assert!(!keeper.keeps(), "the failed keeper is given up");
    assert!(!ledger.holds(&name));
    ledger.sync(&mut keeper, [(name.clone(), 1, pty.master())]);
    assert!(!keeper.keeps());
}

// A store with room for one: the master of a session that ended is
// forgotten before the next is kept, so the next fits; a session past the
// store's room is not recorded as held.
#[test]
fn the_ledger_makes_room_before_it_keeps_and_records_no_refused_keep() {
    let dir = Scratch::new("survive-room");
    let mut systemd = Systemd::new(&dir, 1);
    let mut keeper = Keeper::Store(notify::Store::from_env(&systemd.env()).unwrap());
    let mut ledger = Ledger::default();
    let (a, b) = (session(), session());
    let (one, two) = (
        Name::new("console", 1).encode(),
        Name::new("console", 2).encode(),
    );
    ledger.sync(&mut keeper, [(one.clone(), 1, a.master())]);
    ledger.sync(&mut keeper, [(two.clone(), 2, b.master())]);
    assert_eq!(
        systemd.names(),
        ["pty.2.console"],
        "console 1's master made room"
    );
    ledger.sync(
        &mut keeper,
        [(two.clone(), 2, b.master()), (one.clone(), 3, a.master())],
    );
    assert_eq!(systemd.names(), ["pty.2.console"]);
    assert!(
        !ledger.holds(&one),
        "a keep the store has no room for is not held"
    );
    assert!(keeper.keeps());
}

/// A listening holder socket in `dir`.
fn holder_socket(dir: &Path) -> (PathBuf, OwnedFd) {
    let path = hold::socket_path(dir);
    let listener = socket_with(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .unwrap();
    bind(&listener, &SocketAddrUnix::new(&path).unwrap()).unwrap();
    rustix::net::listen(&listener, 4).unwrap();
    (path, listener)
}

/// A user other than this process's.
fn other_user() -> rustix::process::Uid {
    rustix::process::Uid::from_raw(rustix::process::geteuid().as_raw().wrapping_add(1))
}

// The holder serves only its own user, and vtium trusts only a socket its
// own user set up.
#[test]
fn the_holder_and_vtium_each_refuse_another_user() {
    let dir = Scratch::new("survive-peer-holder");
    let (path, listener) = holder_socket(&dir);
    std::thread::spawn(move || hold::serve_for(listener, other_user()));
    let vtium = hold::Client::connect(&path).unwrap();
    assert!(vtium.take().is_err(), "the holder turns another user away");

    let dir = Scratch::new("survive-peer-client");
    let (path, listener) = holder_socket(&dir);
    let holder = std::thread::spawn(move || hold::serve(listener));
    let refused = hold::Client::connect_for(&path, other_user()).map(|_| ());
    assert_eq!(
        refused.map_err(|e| e.kind()),
        Err(std::io::ErrorKind::PermissionDenied)
    );
    assert!(holder.join().unwrap().is_ok(), "the holder exits empty");
}

#[test]
fn vtium_is_ready_once_its_vt_is_open_and_not_before() {
    // ADR-1500 D7, "Ready before the display manager": systemd starts the
    // display manager once vtium's unit is ready, so READY=1 comes only
    // after the VT is open.
    let dir = Scratch::new("survive-ready");
    let mut systemd = Systemd::new(&dir, 4);
    let env = systemd.env();
    let vt = notify::opened(&env, || {
        systemd.pump();
        assert_eq!(systemd.ready, 0, "READY=1 before the VT is open");
        Ok("the VT")
    })
    .unwrap();
    assert_eq!(vt, "the VT");
    systemd.pump();
    assert_eq!(systemd.ready, 1);
}

#[test]
fn a_vt_that_does_not_open_sends_no_ready() {
    let dir = Scratch::new("survive-ready-no-vt");
    let mut systemd = Systemd::new(&dir, 4);
    let env = systemd.env();
    let opened: std::io::Result<()> = notify::opened(&env, || Err(std::io::Error::other("no VT")));
    assert!(opened.is_err());
    systemd.pump();
    assert_eq!(systemd.ready, 0);
}

#[test]
fn without_systemds_socket_vtium_sends_no_ready() {
    // A host without systemd, or a run by hand, gives no NOTIFY_SOCKET.
    assert!(!notify::ready(&[]).unwrap());
    let env = [("FDSTORE".into(), "50".into())];
    assert!(!notify::ready(&env).unwrap());
    // With the socket and no store, as a unit with no
    // FileDescriptorStoreMax= gives it, vtium still says it is ready.
    let dir = Scratch::new("survive-ready-no-socket");
    let mut systemd = Systemd::new(&dir, 0);
    let env: Vec<(OsString, OsString)> = systemd
        .env()
        .into_iter()
        .filter(|(k, _)| k == "NOTIFY_SOCKET")
        .collect();
    assert!(notify::ready(&env).unwrap());
    systemd.pump();
    assert_eq!(systemd.ready, 1);
}
