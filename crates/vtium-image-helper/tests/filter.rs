//! The second test of ADR-1101 D9: the helper's filter installed in a
//! child, which then makes calls outside the list and sees each answered
//! ENOSYS, while a call on the list still works; and the worker's second
//! filter refusing a fork.
//!
//! It has its own `main`, so the child, this binary run again with
//! `--child`, has one thread when it installs the filter.

use std::process::{Command, ExitCode};

use rustix::io::Errno;
use rustix::net::{AddressFamily, SocketFlags, SocketType};

/// What the child sees under the filter, as an exit status: 0 when every
/// call answers as it should, otherwise the number of the first that
/// does not.
fn child(worker: bool) -> u8 {
    if !vtium_image_helper::fork::one_thread() {
        return 90;
    }
    match vtium_image_helper::sandbox::install_helper_filter() {
        Ok(true) => {}
        Ok(false) => return 0,
        Err(_) => return 91,
    }
    // an internet socket is outside the list
    let inet = rustix::net::socket(AddressFamily::INET, SocketType::STREAM, None);
    if inet.err() != Some(Errno::NOSYS) {
        return 1;
    }
    // a socket pair of any family but Unix is too
    let pair = rustix::net::socketpair(
        AddressFamily::INET,
        SocketType::STREAM,
        SocketFlags::empty(),
        None,
    );
    if pair.err() != Some(Errno::NOSYS) {
        return 2;
    }
    // a Unix socket pair, for a worker's channel, is on it
    if rustix::net::socketpair(
        AddressFamily::UNIX,
        SocketType::SEQPACKET,
        SocketFlags::CLOEXEC,
        None,
    )
    .is_err()
    {
        return 3;
    }
    // every ioctl is outside it
    let Ok(file) = std::fs::File::open("/") else {
        return 40;
    };
    if rustix::fs::ioctl_getflags(&file).err() != Some(Errno::NOSYS) {
        return 4;
    }
    // prctl with any option but no-new-privileges is too
    if rustix::thread::name().err() != Some(Errno::NOSYS) {
        return 5;
    }
    if worker {
        if vtium_image_helper::sandbox::install_worker_filter().is_err() {
            return 6;
        }
        // a worker can start no process
        if Command::new("/bin/true").status().is_ok() {
            return 7;
        }
    }
    0
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--child") => return ExitCode::from(child(false)),
        Some("--worker") => return ExitCode::from(child(true)),
        _ => {}
    }
    let me = std::env::current_exe().expect("the test's own path");
    let mut failed = false;
    for role in ["--child", "--worker"] {
        let status = Command::new(&me)
            .arg(role)
            .status()
            .expect("the child starts");
        let ok = status.success();
        println!(
            "test {} ... {}",
            if role == "--child" {
                "a_call_outside_the_list_answers_enosys"
            } else {
                "a_worker_under_its_second_filter_starts_no_process"
            },
            if ok {
                "ok".to_owned()
            } else {
                format!("FAILED ({status})")
            }
        );
        failed |= !ok;
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
