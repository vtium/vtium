//! `vtium-image [--tmpdir=DIR]`: the image helper vtium starts with its
//! socket on descriptor 3, an empty environment and its sandbox already
//! set (ADR-1101 D3). It checks that it has one thread and its socket,
//! sets no-new-privileges, installs its seccomp filter, sends its hello
//! and serves requests until vtium closes the socket or it idles.
//!
//! `--tmpdir` is vtium's own `TMPDIR`, where a temporary file may also
//! lie; it reaches the helper as an argument so its environment stays
//! empty. An unknown argument, a missing socket, or a sandbox it cannot
//! set exits with status 2, 3 or 4 before any request is read.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vtium_image::decode::wire::{self, Hello};
use vtium_image_helper::serve::{self, Settings};
use vtium_image_helper::{channel, fork, sandbox};

/// The `TMPDIR` the arguments name, resolved; `Err` for an argument the
/// helper does not know. A `TMPDIR` that is not absolute, does not
/// resolve, or resolves to `/` is no temporary place.
fn settings(args: &[std::ffi::OsString]) -> Result<Settings, ()> {
    let mut tmpdir = None;
    for arg in args {
        let text = arg.to_str().ok_or(())?;
        let dir = text.strip_prefix("--tmpdir=").ok_or(())?;
        tmpdir = Some(PathBuf::from(dir));
    }
    let tmpdir = tmpdir
        .filter(|d| d.is_absolute())
        .and_then(|d| std::fs::canonicalize(d).ok())
        .filter(|d| d != Path::new("/"));
    let filter = sandbox::WorkerFilter::build().map_err(|_| ())?;
    Ok(Settings {
        tmpdir,
        filter,
        landlock: 0,
        filters: None,
    })
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let Ok(mut settings) = settings(&args) else {
        return ExitCode::from(2);
    };
    let Some(socket) = fork::socket() else {
        return ExitCode::from(3);
    };
    if !fork::one_thread() || rustix::thread::set_no_new_privs(true).is_err() {
        return ExitCode::from(4);
    }
    let landlock = sandbox::landlock_version();
    let before = sandbox::seccomp_filters();
    let Ok(seccomp) = sandbox::install_helper_filter() else {
        return ExitCode::from(4);
    };
    // the helper's own filter is in force where the kernel counts filters,
    // and each worker must then run under one more
    let filters = sandbox::seccomp_filters().filter(|_| seccomp);
    if filters.is_some() && filters != before.and_then(|n| n.checked_add(1)) {
        return ExitCode::from(4);
    }
    settings.landlock = landlock;
    settings.filters = filters;
    let hello = Hello {
        version: wire::VERSION,
        landlock,
        seccomp,
    };
    if channel::send(&socket, &wire::encode_hello(&hello), &[]).is_err() {
        return ExitCode::from(serve::EXIT_FAILED);
    }
    ExitCode::from(serve::serve(&socket, &settings))
}
