//! The helper's filters (ADR-1101 D9): a seccomp allow-list the helper
//! installs before it reads its first request; a second filter each
//! worker adds, which refuses forking, starting a program and making a
//! memory file; a third, which refuses opening and removing a file, for a
//! worker on a kernel without landlock; and the landlock domain each
//! worker enters once it holds the descriptor it reads.
//!
//! The allow-list answers every call outside it with ENOSYS rather than
//! killing the helper, so a C library update that tries a newer call falls
//! back to an older one. A call made under another architecture's
//! numbering kills the process, since the filter cannot read its number.
//! `seccompiler` builds filters for `x86_64`, `aarch64` and `riscv64`
//! only, so on 32-bit ARM the helper installs none and runs with landlock
//! and the rest of its sandbox, which its hello tells vtium.

use landlock::{
    ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, Ruleset, RulesetAttr, RulesetStatus,
    Scope,
};

/// The newest landlock version this build knows; a worker's domain handles
/// every filesystem right of the kernel's version up to it.
const LANDLOCK_NEWEST: ABI = ABI::V9;

/// The landlock version the kernel offers, 0 for none, found by making
/// rulesets of each version, newest first, without entering any.
pub fn landlock_version() -> u32 {
    const VERSIONS: [(ABI, u32); 9] = [
        (ABI::V9, 9),
        (ABI::V8, 8),
        (ABI::V7, 7),
        (ABI::V6, 6),
        (ABI::V5, 5),
        (ABI::V4, 4),
        (ABI::V3, 3),
        (ABI::V2, 2),
        (ABI::V1, 1),
    ];
    VERSIONS
        .into_iter()
        .find(|&(abi, _)| {
            Ruleset::default()
                .set_compatibility(CompatLevel::HardRequirement)
                .handle_access(AccessFs::from_all(abi))
                .and_then(|r| r.create())
                .is_ok()
        })
        .map_or(0, |(_, n)| n)
}

/// Enters a landlock domain that handles every filesystem right the
/// kernel's landlock knows and grants none, so the worker opens and
/// removes nothing more and reads only the descriptors it holds. Where the
/// kernel's landlock has them, the domain also refuses binding and
/// connecting TCP sockets (version 4) and is scoped (version 6), so the
/// worker signals no process outside it and connects to no abstract Unix
/// socket made outside it. Returns whether the domain is in force; a
/// kernel without landlock leaves it out, and the worker goes on.
pub fn lock_in() -> Result<bool, String> {
    let status = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_all(LANDLOCK_NEWEST))
        .and_then(|r| r.handle_access(AccessNet::from_all(LANDLOCK_NEWEST)))
        .and_then(|r| r.scope(Scope::from_all(LANDLOCK_NEWEST)))
        .and_then(|r| r.create())
        .and_then(|r| r.restrict_self())
        .map_err(|e| format!("the worker cannot enter its landlock domain: {e}"))?;
    Ok(status.ruleset != RulesetStatus::NotEnforced)
}

/// Whether this build installs a seccomp filter.
pub const SECCOMP: bool = cfg!(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
));

#[cfg(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
))]
mod filter {
    use std::collections::BTreeMap;

    use seccompiler::{
        BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
        SeccompRule, TargetArch,
    };

    #[cfg(target_arch = "x86_64")]
    const ARCH: TargetArch = TargetArch::x86_64;
    #[cfg(target_arch = "aarch64")]
    const ARCH: TargetArch = TargetArch::aarch64;
    #[cfg(target_arch = "riscv64")]
    const ARCH: TargetArch = TargetArch::riscv64;

    /// The calls the helper and its workers make, allowed with any
    /// arguments: reading and writing descriptors, `pread` and `pwrite`,
    /// packets with descriptors, memory files and their seals, the memory
    /// calls of the Rust allocator, opening through `openat2` and reading
    /// through `/proc/self/fd`, the `stat` and `statfs` family, reading a
    /// link, removing a directory entry, closing, forking a worker and
    /// waiting for it, exiting, signal masks and handlers, limits, random
    /// bytes, the landlock calls, and the futex and scheduling calls of the
    /// standard library. `socketpair`, `prctl`, `seccomp`, `fcntl`,
    /// `prlimit64`, `mmap` and `mprotect` are allowed with the arguments
    /// [`conditional`] names. Creating any other
    /// socket or connecting one, starting a program, tracing a process,
    /// signalling another, mounting and every `ioctl` are left out.
    #[cfg(target_arch = "x86_64")]
    pub const ALLOWED: &[i64] = &[
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_readv,
        libc::SYS_writev,
        libc::SYS_pread64,
        libc::SYS_pwrite64,
        libc::SYS_lseek,
        libc::SYS_close,
        libc::SYS_ftruncate,
        libc::SYS_memfd_create,
        libc::SYS_sendmsg,
        libc::SYS_recvmsg,
        libc::SYS_poll,
        libc::SYS_ppoll,
        libc::SYS_munmap,
        libc::SYS_mremap,
        libc::SYS_madvise,
        libc::SYS_brk,
        libc::SYS_open,
        libc::SYS_openat,
        libc::SYS_openat2,
        libc::SYS_stat,
        libc::SYS_lstat,
        libc::SYS_fstat,
        libc::SYS_newfstatat,
        libc::SYS_statx,
        libc::SYS_statfs,
        libc::SYS_fstatfs,
        libc::SYS_readlink,
        libc::SYS_readlinkat,
        libc::SYS_getcwd,
        libc::SYS_unlinkat,
        libc::SYS_clone,
        libc::SYS_wait4,
        libc::SYS_waitid,
        libc::SYS_exit,
        libc::SYS_exit_group,
        libc::SYS_getpid,
        libc::SYS_gettid,
        libc::SYS_getuid,
        libc::SYS_geteuid,
        libc::SYS_getgid,
        libc::SYS_getegid,
        libc::SYS_rt_sigprocmask,
        libc::SYS_rt_sigaction,
        libc::SYS_rt_sigreturn,
        libc::SYS_sigaltstack,
        libc::SYS_restart_syscall,
        libc::SYS_getrlimit,
        libc::SYS_setrlimit,
        libc::SYS_getrandom,
        libc::SYS_landlock_create_ruleset,
        libc::SYS_landlock_add_rule,
        libc::SYS_landlock_restrict_self,
        libc::SYS_futex,
        libc::SYS_sched_yield,
        libc::SYS_clock_gettime,
    ];

    /// [`ALLOWED`] on `aarch64` and `riscv64`, whose numbering has no
    /// `open`, `stat`, `lstat`, `readlink` or `poll`.
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    pub const ALLOWED: &[i64] = &[
        libc::SYS_read,
        libc::SYS_write,
        libc::SYS_readv,
        libc::SYS_writev,
        libc::SYS_pread64,
        libc::SYS_pwrite64,
        libc::SYS_lseek,
        libc::SYS_close,
        libc::SYS_ftruncate,
        libc::SYS_memfd_create,
        libc::SYS_sendmsg,
        libc::SYS_recvmsg,
        libc::SYS_ppoll,
        libc::SYS_munmap,
        libc::SYS_mremap,
        libc::SYS_madvise,
        libc::SYS_brk,
        libc::SYS_openat,
        libc::SYS_openat2,
        libc::SYS_fstat,
        libc::SYS_newfstatat,
        libc::SYS_statx,
        libc::SYS_statfs,
        libc::SYS_fstatfs,
        libc::SYS_readlinkat,
        libc::SYS_getcwd,
        libc::SYS_unlinkat,
        libc::SYS_clone,
        libc::SYS_wait4,
        libc::SYS_waitid,
        libc::SYS_exit,
        libc::SYS_exit_group,
        libc::SYS_getpid,
        libc::SYS_gettid,
        libc::SYS_getuid,
        libc::SYS_geteuid,
        libc::SYS_getgid,
        libc::SYS_getegid,
        libc::SYS_rt_sigprocmask,
        libc::SYS_rt_sigaction,
        libc::SYS_rt_sigreturn,
        libc::SYS_sigaltstack,
        libc::SYS_restart_syscall,
        libc::SYS_getrlimit,
        libc::SYS_setrlimit,
        libc::SYS_getrandom,
        libc::SYS_landlock_create_ruleset,
        libc::SYS_landlock_add_rule,
        libc::SYS_landlock_restrict_self,
        libc::SYS_futex,
        libc::SYS_sched_yield,
        libc::SYS_clock_gettime,
    ];

    /// A condition on argument `arg`, compared as the 32-bit integer the
    /// kernel reads it as.
    fn on(arg: u8, op: SeccompCmpOp, value: i32) -> Result<SeccompCondition, String> {
        let value = u64::from(u32::from_ne_bytes(value.to_ne_bytes()));
        SeccompCondition::new(arg, SeccompCmpArgLen::Dword, op, value)
            .map_err(|e| format!("cannot build a seccomp rule: {e}"))
    }

    /// A rule that matches when argument `arg` is `value`.
    fn is(arg: u8, value: i32) -> Result<SeccompRule, String> {
        SeccompRule::new(vec![on(arg, SeccompCmpOp::Eq, value)?])
            .map_err(|e| format!("cannot build a seccomp rule: {e}"))
    }

    /// A rule that matches when argument `arg` has none of `bits` set.
    fn without(arg: u8, bits: i32) -> Result<SeccompCondition, String> {
        on(
            arg,
            SeccompCmpOp::MaskedEq(u64::from(u32::from_ne_bytes(bits.to_ne_bytes()))),
            0,
        )
    }

    /// The calls allowed with some arguments only, each rule one form the
    /// helper makes:
    ///
    /// - a Unix socket pair for each worker's channel;
    /// - `prctl` setting no-new-privileges, which landlock and seccompiler
    ///   both make;
    /// - `seccomp` installing a filter, which a worker needs for its own;
    /// - `fcntl` reading and setting a descriptor's flags and its status
    ///   flags, never with `O_ASYNC`, adding and reading seals, and
    ///   duplicating a descriptor close-on-exec, so no descriptor's
    ///   signals can be pointed at another process (`F_SETOWN`);
    /// - `prlimit64` on the calling process alone, pid 0, so no other
    ///   process's limits change;
    /// - `mmap` and `mprotect` without `PROT_EXEC`, so no memory becomes
    ///   executable.
    fn conditional() -> Result<Vec<(i64, SeccompRule)>, String> {
        let mut rules = vec![
            (libc::SYS_socketpair, is(0, libc::AF_UNIX)?),
            (libc::SYS_prctl, is(0, libc::PR_SET_NO_NEW_PRIVS)?),
            (
                libc::SYS_seccomp,
                is(0, i32::try_from(libc::SECCOMP_SET_MODE_FILTER).unwrap_or(1))?,
            ),
            (libc::SYS_prlimit64, is(0, 0)?),
        ];
        for cmd in [
            libc::F_GETFD,
            libc::F_SETFD,
            libc::F_GETFL,
            libc::F_ADD_SEALS,
            libc::F_GET_SEALS,
            libc::F_DUPFD_CLOEXEC,
        ] {
            rules.push((libc::SYS_fcntl, is(1, cmd)?));
        }
        let setfl = SeccompRule::new(vec![
            on(1, SeccompCmpOp::Eq, libc::F_SETFL)?,
            without(2, libc::O_ASYNC)?,
        ])
        .map_err(|e| format!("cannot build a seccomp rule: {e}"))?;
        rules.push((libc::SYS_fcntl, setfl));
        for call in [libc::SYS_mmap, libc::SYS_mprotect] {
            let rule = SeccompRule::new(vec![without(2, libc::PROT_EXEC)?])
                .map_err(|e| format!("cannot build a seccomp rule: {e}"))?;
            rules.push((call, rule));
        }
        Ok(rules)
    }

    fn compile(
        rules: BTreeMap<i64, Vec<SeccompRule>>,
        mismatch: SeccompAction,
        matched: SeccompAction,
    ) -> Result<BpfProgram, String> {
        SeccompFilter::new(rules, mismatch, matched, ARCH)
            .and_then(BpfProgram::try_from)
            .map_err(|e| format!("cannot build the seccomp filter: {e}"))
    }

    /// The helper's allow-list, compiled: any call outside it answers
    /// ENOSYS.
    pub fn helper() -> Result<BpfProgram, String> {
        let mut rules: BTreeMap<i64, Vec<SeccompRule>> =
            ALLOWED.iter().map(|&n| (n, Vec::new())).collect();
        for (n, rule) in conditional()? {
            rules.entry(n).or_default().push(rule);
        }
        let enosys = u32::try_from(libc::ENOSYS).unwrap_or(38);
        compile(rules, SeccompAction::Errno(enosys), SeccompAction::Allow)
    }

    /// The worker's second filter: forking and starting a program answer
    /// EPERM, and every other call is left to the first filter.
    pub fn worker() -> Result<BpfProgram, String> {
        #[cfg(target_arch = "x86_64")]
        let refused = [
            libc::SYS_clone,
            libc::SYS_clone3,
            libc::SYS_fork,
            libc::SYS_vfork,
            libc::SYS_execve,
            libc::SYS_execveat,
            libc::SYS_memfd_create,
        ];
        #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
        let refused = [
            libc::SYS_clone,
            libc::SYS_clone3,
            libc::SYS_execve,
            libc::SYS_execveat,
            libc::SYS_memfd_create,
        ];
        refuse(&refused)
    }

    /// A filter that answers each of `calls` EPERM and leaves every other
    /// call to the filters before it.
    fn refuse(calls: &[i64]) -> Result<BpfProgram, String> {
        let rules = calls.iter().map(|&n| (n, Vec::new())).collect();
        let eperm = u32::try_from(libc::EPERM).unwrap_or(1);
        compile(rules, SeccompAction::Allow, SeccompAction::Errno(eperm))
    }

    /// The worker's third filter, for a kernel without landlock: opening
    /// and removing a file answer EPERM.
    pub fn no_open() -> Result<BpfProgram, String> {
        #[cfg(target_arch = "x86_64")]
        let refused = [
            libc::SYS_open,
            libc::SYS_openat,
            libc::SYS_openat2,
            libc::SYS_unlinkat,
        ];
        #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
        let refused = [libc::SYS_openat, libc::SYS_openat2, libc::SYS_unlinkat];
        refuse(&refused)
    }

    pub fn install(program: &BpfProgram) -> Result<(), String> {
        seccompiler::apply_filter(program)
            .map_err(|e| format!("cannot install the seccomp filter: {e}"))
    }
}

/// Installs the helper's allow-list; `Ok(false)` on an architecture
/// `seccompiler` builds no filter for.
pub fn install_helper_filter() -> Result<bool, String> {
    #[cfg(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    ))]
    {
        filter::install(&filter::helper()?)?;
        Ok(true)
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )))]
    Ok(false)
}

/// The worker's second filter, which refuses forking and starting a
/// program, built once by the helper so each worker only installs it.
#[derive(Clone, Debug)]
pub struct WorkerFilter {
    #[cfg(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    ))]
    program: seccompiler::BpfProgram,
    #[cfg(any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    ))]
    no_open: seccompiler::BpfProgram,
}

impl WorkerFilter {
    pub fn build() -> Result<Self, String> {
        Ok(Self {
            #[cfg(any(
                target_arch = "x86_64",
                target_arch = "aarch64",
                target_arch = "riscv64"
            ))]
            program: filter::worker()?,
            #[cfg(any(
                target_arch = "x86_64",
                target_arch = "aarch64",
                target_arch = "riscv64"
            ))]
            no_open: filter::no_open()?,
        })
    }

    /// Adds the filter to the calling worker; `Ok(false)` where no filter
    /// is built.
    pub fn install(&self) -> Result<bool, String> {
        #[cfg(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        ))]
        {
            filter::install(&self.program)?;
            Ok(true)
        }
        #[cfg(not(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        )))]
        Ok(false)
    }

    /// Adds the third filter, which refuses opening and removing a file,
    /// for a worker on a kernel without landlock; `Ok(false)` where no
    /// filter is built.
    pub fn install_no_open(&self) -> Result<bool, String> {
        #[cfg(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        ))]
        {
            filter::install(&self.no_open)?;
            Ok(true)
        }
        #[cfg(not(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        )))]
        Ok(false)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    #[test]
    fn both_filters_compile_and_the_landlock_version_is_read() {
        #[cfg(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        ))]
        {
            assert!(!filter::helper().unwrap().is_empty());
            assert!(!filter::worker().unwrap().is_empty());
        }
        // the build machine's kernel has landlock; a kernel without it reads 0
        assert!(landlock_version() <= 9);
    }
}
