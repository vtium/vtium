//! The helper's sandbox: the limits it sets on itself, the descriptors it
//! sheds at start, its refusal to run as root, its landlock domain, and its
//! socket and fallback filters. [`command`] starts the helper inside them.

use std::io;
use std::time::Duration;

use landlock::{
    ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, LandlockStatus, Ruleset,
    RulesetAttr, RulesetStatus, Scope,
};
use rustix::process::{Resource, Rlimit, setrlimit};

use crate::child::PARSER_MEMORY;
use crate::seccomp::Missing;
use crate::sources::Gate;

use super::engine::Paths;
#[cfg(doc)]
use super::wire::{DescribeAsk, FRAME_SANDBOX};
use super::wire::{SANDBOX_DOMAIN, SANDBOX_FALLBACK, SANDBOX_SOCKETS, Sandboxed, failed};
use super::{HELPER_IDLE_MS, HELPER_SECONDS, answer_requests, read_requests};

/// The variable the helper sets when it starts itself again without the
/// descriptors it inherited, so it does so once.
pub const SHED_VAR: &str = "VTIUM_CONFIG_SERVE_SHED";

/// The descriptors above standard error this process holds: each number
/// `/proc/self/fd` lists that still names a file once the listing's own
/// descriptor is closed.
fn inherited() -> Vec<u32> {
    let listed: Vec<u32> = std::fs::read_dir("/proc/self/fd")
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
                .filter(|&fd| fd > 2)
                .collect()
        })
        .unwrap_or_default();
    listed
        .into_iter()
        .filter(|fd| std::fs::read_link(format!("/proc/self/fd/{fd}")).is_ok())
        .collect()
}

/// Leaves the helper holding only its three pipes, whatever its starter
/// left open, as a terminal a font could write to: when it holds any
/// descriptor above standard error, it marks every one close-on-exec and
/// starts itself again, once, with the same arguments. Returns why it could
/// not; `None` when it holds only its pipes, or after the one restart.
fn shed_inherited() -> Option<String> {
    use std::os::unix::process::CommandExt;
    if std::env::var_os(SHED_VAR).is_some() || inherited().is_empty() {
        return None;
    }
    if let Err(e) = vtium_platform::cloexec_above_stdio() {
        return Some(format!("the helper cannot close what it inherited: {e}"));
    }
    let why = std::process::Command::new("/proc/self/exe")
        .args(std::env::args_os().skip(1))
        .env(SHED_VAR, "1")
        .exec();
    Some(format!(
        "the helper cannot start again without what it inherited: {why}"
    ))
}

/// The limits the helper sets on itself before it reads anything: the
/// parser's, with a CPU limit over the helper's life. With no process
/// left to start, it starts no thread either, so its reader starts first.
fn limits() -> Result<(), String> {
    for (resource, limit) in [
        (Resource::As, PARSER_MEMORY),
        (Resource::Cpu, HELPER_SECONDS),
        (Resource::Fsize, 0),
        (Resource::Nproc, 0),
        (Resource::Core, 0),
    ] {
        let limit = Rlimit {
            current: Some(limit),
            maximum: Some(limit),
        };
        setrlimit(resource, limit).map_err(|e| format!("the helper cannot set its limits: {e}"))?;
    }
    let status = std::fs::read_to_string("/proc/self/status")
        .map_err(|e| format!("the helper cannot read its ids from /proc/self/status: {e}"))?;
    refuse_root(&status)
}

/// The ids a `/proc/<pid>/status` line names after its `key`, such as
/// `Uid:`'s real, effective, saved and filesystem uids, or `Groups:`'s
/// supplementary groups; `None` when the line is missing or a field is not
/// a number.
pub(super) fn status_ids(status: &str, key: &str) -> Option<Vec<u32>> {
    let line = status.lines().find_map(|l| l.strip_prefix(key))?;
    line.split_whitespace().map(|id| id.parse().ok()).collect()
}

/// The helper never runs as root, nor with root's group (ADR-1200 D15).
/// `status` is its `/proc/self/status`, and the helper refuses to run when
/// any of its real, effective, saved and filesystem uids is 0, when any of
/// those gids is 0, or when its supplementary groups include 0. A saved id
/// of 0 would let it take root's id back, and group 0 reads what root's
/// group may read. A status it cannot read is refused too.
pub(super) fn refuse_root(status: &str) -> Result<(), String> {
    let ids = |key: &str| {
        status_ids(status, key)
            .filter(|ids| key == "Groups:" || ids.len() == 4)
            .ok_or_else(|| {
                format!(
                    "the helper cannot read its ids: /proc/self/status has no {key} line it reads"
                )
            })
    };
    if ids("Uid:")?.contains(&0) {
        return Err("the helper refuses to run as root".to_owned());
    }
    if ids("Gid:")?.contains(&0) || ids("Groups:")?.contains(&0) {
        return Err("the helper refuses to run with group 0, root's group".to_owned());
    }
    Ok(())
}

/// The newest landlock version this build knows; the helper's domain
/// handles every write right and every network right of the kernel's
/// version up to it.
const LANDLOCK_NEWEST: ABI = ABI::V9;

/// Why the helper's landlock domain is not in force, as bits 16 to 23
/// of its [`FRAME_SANDBOX`] number it; 0 there means the domain is in
/// force.
///
/// References:
/// - ADR-1200 D15, "Nothing on screen, and the log says why": the four
///   reasons a landlock domain is not in force.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum NoDomain {
    /// `helper-sandbox=off` turns the domain off.
    Off = 1,
    /// The kernel is older than landlock, Linux 5.13.
    TooOld = 2,
    /// The kernel is 5.13 or later and was built without landlock, so
    /// landlock's version query answers `ENOSYS`.
    NotBuilt = 3,
    /// The kernel was built with landlock but did not turn it on at boot,
    /// as when the boot's `lsm=` list leaves it out, so the version query
    /// answers `EOPNOTSUPP`.
    NotEnabled = 4,
}

/// Why a kernel whose landlock query answered `status` gives the helper
/// no domain, by its release. `ENOSYS` means either a kernel older than
/// landlock or one built without it, and the release tells the two
/// apart. A kernel that answers with a version enforces the domain, so
/// the helper meets that status here only if a later kernel refuses the
/// domain all the same; it is reported as one without landlock.
pub fn no_domain(status: LandlockStatus, release: Option<(u32, u32)>) -> NoDomain {
    match status {
        LandlockStatus::NotEnabled => NoDomain::NotEnabled,
        _ if release.is_some_and(|r| r < (5, 13)) => NoDomain::TooOld,
        _ => NoDomain::NotBuilt,
    }
}

/// Enters a landlock domain for the calling thread and the threads it
/// starts later. The domain handles every right to write, create, remove,
/// rename or truncate a file and grants none, refuses binding and
/// connecting TCP sockets, and is scoped, so the helper connects to no
/// abstract Unix socket made outside it and signals no process outside
/// it, each as far as the kernel's landlock version knows it. The domain
/// leaves reads open, because a helper that vtium-config starts reads a
/// font in a user's folder to preview it ([`DescribeAsk`]). The console's
/// helper opens a font only through its gate, which takes only the active
/// fonts. Returns `None` when the domain is in force, and otherwise why
/// the kernel gives none; the helper then runs as before, without it.
///
/// References:
/// - ADR-1200 D15: the domain and what it refuses (the maintainer,
///   2026-11-04), and the reasons the log gives for a missing one.
pub(super) fn lock_out() -> Result<Option<NoDomain>, String> {
    let status = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_write(LANDLOCK_NEWEST))
        .and_then(|r| r.handle_access(AccessNet::from_all(LANDLOCK_NEWEST)))
        .and_then(|r| r.scope(Scope::from_all(LANDLOCK_NEWEST)))
        .and_then(|r| r.create())
        .and_then(|r| r.restrict_self())
        .map_err(|e| format!("the helper cannot enter its landlock domain: {e}"))?;
    Ok((status.ruleset == RulesetStatus::NotEnforced)
        .then(|| no_domain(status.landlock, crate::seccomp::kernel_release())))
}

/// Installs the helper's socket filter on the calling thread and the
/// threads it starts later: a seccomp filter under which `socket`,
/// `socketpair` and the three io_uring calls answer EPERM and every other
/// call goes on, so the helper makes no socket of any kind on any kernel,
/// the UDP and Unix sockets landlock leaves open included
/// ([`crate::seccomp::socket_filter`]). Returns `None` when the filter is
/// in force, and otherwise why it is not: [`Missing::NoTable`] on an
/// architecture that [`crate::seccomp`] has no table of call numbers for,
/// or the kernel's lack of seccomp filters when the filter fails to
/// install there. On a kernel that offers the filters, a filter that
/// fails to install fails the helper.
///
/// References:
/// - ADR-1200 D15: the socket filter (the maintainer, 2026-11-04), and a
///   kernel without seccomp filters, which runs the helper without them.
pub(super) fn refuse_sockets() -> Result<Option<Missing>, String> {
    #[cfg(all(
        target_endian = "little",
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64",
            target_arch = "arm"
        )
    ))]
    {
        let filter = crate::seccomp::socket_filter()
            .map_err(|e| format!("the helper cannot build its socket filter: {e}"))?;
        match crate::seccomp::install(&filter) {
            Ok(()) => Ok(None),
            Err(e) => crate::seccomp::running_kernel_lacks_filters()
                .map(Some)
                .ok_or_else(|| format!("the helper cannot install its socket filter: {e}")),
        }
    }
    #[cfg(not(all(
        target_endian = "little",
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64",
            target_arch = "arm"
        )
    )))]
    Ok(Some(Missing::NoTable))
}

/// Installs the helper's fallback filter on the calling thread and the
/// threads it starts later, in place of the landlock domain on a kernel
/// without landlock: a seccomp filter that refuses the file, signal and
/// tracing calls the domain refuses ([`crate::seccomp::fallback_filter`]).
/// The helper installs it only once its socket filter is in force, so
/// the kernel offers the filters and this build has a table of call
/// numbers, and a filter that fails to install here fails the helper.
///
/// References:
/// - ADR-1200 D15: the fallback filter (the maintainer, 2026-11-13).
pub(super) fn fall_back() -> Result<(), String> {
    #[cfg(all(
        target_endian = "little",
        any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64",
            target_arch = "arm"
        )
    ))]
    {
        let pid = rustix::process::getpid().as_raw_nonzero().get();
        let pid = u32::try_from(pid).map_err(|_| "the helper's process id is negative")?;
        let filter = crate::seccomp::fallback_filter(pid)
            .map_err(|e| format!("the helper cannot build its fallback filter: {e}"))?;
        crate::seccomp::install(&filter)
            .map_err(|e| format!("the helper cannot install its fallback filter: {e}"))?;
    }
    Ok(())
}

/// The option that tells the helper whether to lock itself in, with the
/// value `on` or `off`, which the console passes from vtium.conf's
/// `helper-sandbox`. A helper started without it locks itself in.
pub const SANDBOX_OPTION: &str = "--sandbox";

/// Sandboxes the calling thread and the threads it starts later, when
/// `on`: the socket filter, then the landlock domain, or the fallback
/// filter where the kernel offers no landlock. With `on` false, which
/// `helper-sandbox=off` asks for, it installs no filter and enters no
/// domain. Returns the parts in force and why the others are not, as
/// [`FRAME_SANDBOX`] reports them, or why a part failed to install.
///
/// References:
/// - ADR-1200 D15, "A setting turns the sandbox off": what the helper
///   leaves out under `off`, and what stays.
pub(super) fn sandbox(on: bool) -> Result<Sandboxed, String> {
    if !on {
        return Ok(Sandboxed {
            parts: 0,
            seccomp: Some(Missing::Off),
            domain: Some(NoDomain::Off),
        });
    }
    let seccomp = refuse_sockets()?;
    let domain = lock_out()?;
    let mut parts = 0;
    if seccomp.is_none() {
        parts |= SANDBOX_SOCKETS;
    }
    if domain.is_none() {
        parts |= SANDBOX_DOMAIN;
    } else if seccomp.is_none() {
        fall_back()?;
        parts |= SANDBOX_FALLBACK;
    }
    Ok(Sandboxed {
        parts,
        seccomp,
        domain,
    })
}

/// `vtium-config serve`: no-new-privileges, the sandbox, the requests'
/// reader, the limits, then the requests on stdin answered on stdout until
/// [`HELPER_IDLE_MS`] passes with none, the first answer led by the
/// sandbox's report. With `sandboxed` false, which `--sandbox off` asks
/// for, the helper installs no seccomp filter and enters no landlock
/// domain. A failure to sandbox is written as a failed
/// frame, so the console logs it.
pub fn command(paths: &Paths, sandboxed: bool) -> u8 {
    let mut out = io::stdout().lock();
    if let Some(why) = shed_inherited() {
        let _ = failed(&mut out, &why);
        return 1;
    }
    if let Err(e) = rustix::thread::set_no_new_privs(true) {
        let _ = failed(
            &mut out,
            &format!("the helper cannot set no-new-privileges: {e}"),
        );
        return 1;
    }
    // before the reader's thread starts, which inherits the filters and
    // the domain, and before any font is read
    let report = match sandbox(sandboxed) {
        Ok(report) => report,
        Err(why) => {
            let _ = failed(&mut out, &why);
            return 1;
        }
    };
    let requests = read_requests(io::stdin());
    if let Err(why) = limits() {
        let _ = failed(&mut out, &why);
        return 1;
    }
    answer_requests(
        &requests,
        &mut out,
        paths,
        Duration::from_millis(HELPER_IDLE_MS),
        Some(report),
        Gate::Open,
    )
}
