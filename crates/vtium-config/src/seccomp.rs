//! The font helper's two seccomp filters (ADR-1200 D15): the socket
//! filter, which it installs on every kernel, and the fallback filter,
//! which it installs in place of its landlock domain on a kernel without
//! landlock (the maintainer, 2026-11-13).
//!
//! The helper builds both as BPF programs, the kernel's filter language,
//! from a table of call numbers for each architecture vtium builds for:
//! x86_64, aarch64, riscv64 and 32-bit ARM. `seccompiler` builds no
//! program for 32-bit ARM, so this module writes the programs itself and
//! uses `seccompiler` only to install one. A program has the layout that
//! the kernel's `Documentation/userspace-api/seccomp_filter.rst` gives:
//! it reads `struct seccomp_data` (the call's number at offset 0, the
//! architecture at 4, and each argument's 64 bits from 16), and answers
//! with one of the `SECCOMP_RET_*` actions of `linux/seccomp.h`.
//!
//! Each program first checks the architecture and ends the process when a
//! call comes under another architecture's numbering, since it cannot
//! read that call's number. On x86_64 it then refuses every call of the
//! x32 numbering, whose numbers carry bit 30 and so match no rule. Each
//! rule then refuses one call, whatever its arguments or by the low 32
//! bits of one argument, which is all the kernel reads of an `int` or
//! `unsigned int` argument. Every call no rule refuses goes on.

/// Whether this build has a table of call numbers, and so builds the
/// helper's filters.
pub const BUILT: bool = cfg!(all(
    target_endian = "little",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "arm"
    )
));

#[cfg(all(
    target_endian = "little",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "arm"
    )
))]
pub use program::{Program, Rule, compile, fallback_filter, install, socket_filter};

#[cfg(all(
    target_endian = "little",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "arm"
    )
))]
mod program {
    use seccompiler::sock_filter;

    /// A compiled filter, ready to install.
    pub type Program = seccompiler::BpfProgram;

    /// `AUDIT_ARCH_*` of `linux/audit.h` for this build's architecture:
    /// its ELF machine, with bit 31 for a 64-bit one and bit 30 for a
    /// little-endian one.
    #[cfg(target_arch = "x86_64")]
    const ARCH: u32 = 62 | 0x8000_0000 | 0x4000_0000;
    #[cfg(target_arch = "aarch64")]
    const ARCH: u32 = 183 | 0x8000_0000 | 0x4000_0000;
    #[cfg(target_arch = "riscv64")]
    const ARCH: u32 = 243 | 0x8000_0000 | 0x4000_0000;
    #[cfg(target_arch = "arm")]
    const ARCH: u32 = 40 | 0x4000_0000;

    /// `__X32_SYSCALL_BIT`, which every call of the x32 numbering carries.
    #[cfg(target_arch = "x86_64")]
    const X32_BIT: u32 = 0x4000_0000;

    /// The offsets in `struct seccomp_data` of the call's number, its
    /// architecture and its first argument.
    const NR_AT: u32 = 0;
    const ARCH_AT: u32 = 4;
    const ARGS_AT: u32 = 16;

    /// The BPF instructions of `linux/filter.h` the programs use: load a
    /// word of the data, jump on equal, on greater or equal and on any bit
    /// set, and return.
    const LOAD: u16 = 0x20;
    const JEQ: u16 = 0x15;
    #[cfg(target_arch = "x86_64")]
    const JGE: u16 = 0x35;
    const JSET: u16 = 0x45;
    const RET: u16 = 0x06;

    /// The actions of `linux/seccomp.h`.
    const ALLOW: u32 = 0x7fff_0000;
    const KILL_PROCESS: u32 = 0x8000_0000;
    const ERRNO: u32 = 0x0005_0000;

    /// `F_SETOWN_EX` of `asm-generic/fcntl.h`, which points a descriptor's
    /// signals at a process or a thread; the libc crate does not name it.
    const F_SETOWN_EX: i32 = 15;

    /// The old ARM ABI's `socketcall`, which the libc crate does not name.
    #[cfg(target_arch = "arm")]
    const SOCKETCALL: i64 = 102;

    /// `BPF_MAXINSNS`, the most instructions a program may hold.
    const INSTRUCTIONS_MAX: usize = 4096;

    /// One call a filter refuses, and when.
    pub enum Rule {
        /// The call answers EPERM, whatever its arguments.
        Always(i64),
        /// The call answers ENOSYS, as on a kernel that lacks it, so a
        /// program that tries it falls back to an older call.
        Absent(i64),
        /// The call answers EPERM when its argument `arg` has any of
        /// `bits` set.
        AnyBit(i64, u8, u32),
        /// The call answers EPERM when its argument `arg` is one of
        /// `values`.
        OneOf(i64, u8, Vec<u32>),
        /// The call answers EPERM unless its argument `arg` is one of
        /// `values`.
        NoneOf(i64, u8, Vec<u32>),
    }

    fn op(code: u16, k: u32) -> sock_filter {
        sock_filter {
            code,
            jt: 0,
            jf: 0,
            k,
        }
    }

    fn jump(code: u16, k: u32, jt: usize, jf: usize) -> Result<sock_filter, String> {
        let far = |n: usize| {
            u8::try_from(n).map_err(|_| "a seccomp rule names too many values".to_owned())
        };
        Ok(sock_filter {
            code,
            jt: far(jt)?,
            jf: far(jf)?,
            k,
        })
    }

    fn refuse(errno: i32) -> sock_filter {
        op(RET, ERRNO | u32::from(errno.unsigned_abs() as u16))
    }

    fn number(call: i64) -> Result<u32, String> {
        u32::try_from(call).map_err(|_| format!("the call number {call} does not fit a filter"))
    }

    /// Loads the low 32 bits of argument `arg`.
    fn argument(arg: u8) -> Result<sock_filter, String> {
        if arg > 5 {
            return Err(format!("a call has no argument {arg}"));
        }
        Ok(op(LOAD, ARGS_AT + 8 * u32::from(arg)))
    }

    /// The program that refuses each of `rules`' calls and lets every
    /// other call go on.
    pub fn compile(rules: &[Rule]) -> Result<Program, String> {
        let eperm = refuse(libc::EPERM);
        let mut p = vec![
            op(LOAD, ARCH_AT),
            jump(JEQ, ARCH, 1, 0)?,
            op(RET, KILL_PROCESS),
        ];
        #[cfg(target_arch = "x86_64")]
        p.extend([op(LOAD, NR_AT), jump(JGE, X32_BIT, 0, 1)?, eperm.clone()]);
        // each rule's block starts by loading the call's number, since the
        // block before it may have loaded an argument; a call the block
        // does not refuse jumps past its return
        for rule in rules {
            p.push(op(LOAD, NR_AT));
            match rule {
                Rule::Always(call) => {
                    p.push(jump(JEQ, number(*call)?, 0, 1)?);
                    p.push(eperm.clone());
                }
                Rule::Absent(call) => {
                    p.push(jump(JEQ, number(*call)?, 0, 1)?);
                    p.push(refuse(libc::ENOSYS));
                }
                Rule::AnyBit(call, arg, bits) => {
                    p.push(jump(JEQ, number(*call)?, 0, 3)?);
                    p.push(argument(*arg)?);
                    p.push(jump(JSET, *bits, 0, 1)?);
                    p.push(eperm.clone());
                }
                Rule::OneOf(_, _, values) if values.is_empty() => {
                    return Err("a seccomp rule names no values".to_owned());
                }
                Rule::OneOf(call, arg, values) => {
                    let n = values.len();
                    p.push(jump(JEQ, number(*call)?, 0, n + 2)?);
                    p.push(argument(*arg)?);
                    // a match jumps to the return after the last check;
                    // the last check's miss jumps past it
                    for (i, &v) in values.iter().enumerate() {
                        let last = i + 1 == n;
                        p.push(jump(JEQ, v, n - 1 - i, usize::from(last))?);
                    }
                    p.push(eperm.clone());
                }
                Rule::NoneOf(call, arg, values) => {
                    let n = values.len();
                    p.push(jump(JEQ, number(*call)?, 0, n + 2)?);
                    p.push(argument(*arg)?);
                    // a match jumps past the return
                    for (i, &v) in values.iter().enumerate() {
                        p.push(jump(JEQ, v, n - i, 0)?);
                    }
                    p.push(eperm.clone());
                }
            }
        }
        p.push(op(RET, ALLOW));
        if p.len() > INSTRUCTIONS_MAX {
            return Err("a seccomp filter of more instructions than the kernel takes".to_owned());
        }
        Ok(p)
    }

    /// `int` and `unsigned int` constants as the 32 bits the filter
    /// compares.
    fn bits(v: impl TryInto<i64>) -> u32 {
        v.try_into().map_or(0, |v: i64| v as u32)
    }

    /// The socket filter: `socket`, `socketpair` and the three io_uring
    /// calls answer EPERM. io_uring is refused whole, because its
    /// `IORING_OP_SOCKET`, from Linux 5.19, makes a socket without calling
    /// `socket`, and its open and rename operations would pass by the
    /// fallback filter. On 32-bit ARM, `socketcall`, call 102, answers
    /// EPERM too: an EABI process can still make the old ABI's calls on a
    /// kernel built with `OABI_COMPAT`, and the old ABI makes every socket
    /// through `socketcall`.
    pub fn socket_filter() -> Result<Program, String> {
        let rules = [
            libc::SYS_socket,
            libc::SYS_socketpair,
            libc::SYS_io_uring_setup,
            libc::SYS_io_uring_enter,
            libc::SYS_io_uring_register,
            #[cfg(target_arch = "arm")]
            SOCKETCALL,
        ];
        compile(&rules.map(Rule::Always))
    }

    /// The fallback filter for the helper whose process id is `pid`,
    /// which refuses the calls that its landlock domain refuses, as far as
    /// a filter can tell them apart by their numbers and arguments:
    ///
    /// - opening a file to write it, create it or truncate it, by its
    ///   access mode, `O_CREAT` or `O_TRUNC` (`O_TMPFILE` needs a write
    ///   mode); `openat2`, whose flags lie in memory the filter cannot
    ///   read, answers ENOSYS, so a program falls back to `openat`;
    /// - making, removing, renaming and linking a file, a folder, a device
    ///   node or a FIFO, and truncating a file by its path;
    /// - signalling a process other than the helper, by `kill`, `tgkill`,
    ///   `rt_sigqueueinfo` and `rt_tgsigqueueinfo` to another process id
    ///   and by `tkill` and `pidfd_send_signal` at all, and pointing a
    ///   descriptor's signals at another process with `F_SETOWN` or
    ///   `F_SETOWN_EX`;
    /// - tracing another process and reading or writing its memory:
    ///   `ptrace`, `process_vm_readv`, `process_vm_writev`,
    ///   `process_madvise`, `pidfd_getfd` and `kcmp`, which the kernel
    ///   checks as it checks tracing;
    /// - `ioctl`, but for `FIOCLEX`, `FIONCLEX`, `FIONBIO` and `FIOASYNC`,
    ///   the four commands that act on the descriptor itself and that the
    ///   domain allows on a device too.
    ///
    /// The helper starts no process, so its own id is its every thread's
    /// process id.
    pub fn fallback_filter(pid: u32) -> Result<Program, String> {
        let writes = bits(libc::O_ACCMODE | libc::O_CREAT | libc::O_TRUNC);
        let own = || vec![pid];
        let mut rules = Vec::new();
        #[cfg(any(target_arch = "x86_64", target_arch = "arm"))]
        {
            rules.push(Rule::AnyBit(libc::SYS_open, 1, writes));
            for call in [
                libc::SYS_creat,
                libc::SYS_mkdir,
                libc::SYS_mknod,
                libc::SYS_rmdir,
                libc::SYS_unlink,
                libc::SYS_rename,
                libc::SYS_link,
                libc::SYS_symlink,
            ] {
                rules.push(Rule::Always(call));
            }
        }
        rules.push(Rule::AnyBit(libc::SYS_openat, 2, writes));
        rules.push(Rule::Absent(libc::SYS_openat2));
        #[cfg(not(target_arch = "riscv64"))]
        rules.push(Rule::Always(libc::SYS_renameat));
        #[cfg(target_arch = "arm")]
        rules.push(Rule::Always(libc::SYS_truncate64));
        for call in [
            libc::SYS_mkdirat,
            libc::SYS_mknodat,
            libc::SYS_unlinkat,
            libc::SYS_renameat2,
            libc::SYS_linkat,
            libc::SYS_symlinkat,
            libc::SYS_truncate,
            libc::SYS_tkill,
            libc::SYS_pidfd_send_signal,
            libc::SYS_ptrace,
            libc::SYS_process_vm_readv,
            libc::SYS_process_vm_writev,
            libc::SYS_process_madvise,
            libc::SYS_pidfd_getfd,
            libc::SYS_kcmp,
        ] {
            rules.push(Rule::Always(call));
        }
        for call in [
            libc::SYS_kill,
            libc::SYS_tgkill,
            libc::SYS_rt_sigqueueinfo,
            libc::SYS_rt_tgsigqueueinfo,
        ] {
            rules.push(Rule::NoneOf(call, 0, own()));
        }
        let owners = vec![bits(libc::F_SETOWN), bits(F_SETOWN_EX)];
        #[cfg(target_arch = "arm")]
        rules.push(Rule::OneOf(libc::SYS_fcntl64, 1, owners.clone()));
        rules.push(Rule::OneOf(libc::SYS_fcntl, 1, owners));
        let own_descriptor = [libc::FIOCLEX, libc::FIONCLEX, libc::FIONBIO, libc::FIOASYNC];
        rules.push(Rule::NoneOf(
            libc::SYS_ioctl,
            1,
            own_descriptor.into_iter().map(bits).collect(),
        ));
        compile(&rules)
    }

    /// Installs `program` on the calling thread and the threads and
    /// processes it starts later.
    pub fn install(program: &Program) -> Result<(), String> {
        seccompiler::apply_filter(program).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[cfg(all(
    target_endian = "little",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64",
        target_arch = "arm"
    )
))]
#[expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::program::*;
    use crate::scratch::Scratch;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::process::{Command, Stdio};

    /// The python3 that makes each call for the filter tests: it prints
    /// its process id, reads a filter's program in hex on its standard
    /// input, installs it through ctypes, as seccompiler does, unless it
    /// is empty, and makes each call its arguments name, in the folder
    /// its first argument names. Each call is its number and its
    /// arguments, joined by commas: a number, `@` and a path, or a name
    /// the script fills in (its own process id, its parent's, a
    /// descriptor of `existing` read-only, a pidfd of itself or of its
    /// parent, a siginfo of `SI_QUEUE`, an `f_owner_ex` naming its parent,
    /// an `open_how` of zeros, or a buffer for an int). It prints each
    /// call's errno, 0 for a call that succeeds.
    const CALLER: &str = "import ctypes, os, struct, sys\n\
        libc = ctypes.CDLL(None, use_errno=True)\n\
        libc.syscall.restype = ctypes.c_long\n\
        os.chdir(sys.argv[1])\n\
        pid, ppid = os.getpid(), os.getppid()\n\
        print(pid, flush=True)\n\
        code = bytes.fromhex(sys.stdin.readline().strip())\n\
        fd = os.open('existing', os.O_RDONLY)\n\
        named = {'pid': pid, 'ppid': ppid, 'fd': fd, 'pidfd': os.pidfd_open(pid), 'ppidfd': os.pidfd_open(ppid),\n\
        \x20   'siginfo': ctypes.create_string_buffer(struct.pack('iii', 0, 0, -1) + bytes(116)),\n\
        \x20   'owner': ctypes.create_string_buffer(struct.pack('ii', 1, ppid)),\n\
        \x20   'how': ctypes.create_string_buffer(24), 'int': ctypes.create_string_buffer(8)}\n\
        def arg(a):\n\
        \x20   if a.startswith('@'): return ctypes.c_char_p(a[1:].encode())\n\
        \x20   v = named.get(a, None)\n\
        \x20   if v is None: return ctypes.c_long(int(a))\n\
        \x20   return v if isinstance(v, ctypes.Array) else ctypes.c_long(v)\n\
        if code:\n\
        \x20   class Prog(ctypes.Structure):\n\
        \x20       _fields_ = [('len', ctypes.c_ushort), ('filter', ctypes.c_char_p)]\n\
        \x20   prog = Prog(len(code) // 8, code)\n\
        \x20   assert libc.prctl(38, 1, 0, 0, 0) == 0\n\
        \x20   assert libc.prctl(22, 2, ctypes.byref(prog), 0, 0) == 0\n\
        out = []\n\
        for spec in sys.argv[2:]:\n\
        \x20   nr, *args = spec.split(',')\n\
        \x20   ctypes.set_errno(0)\n\
        \x20   r = libc.syscall(ctypes.c_long(int(nr)), *[arg(a) for a in args])\n\
        \x20   out.append(str(ctypes.get_errno() if r == -1 else 0))\n\
        print(' '.join(out))\n";

    /// The program `filter` in the bytes of `struct sock_filter`, in hex.
    fn hex(filter: &[seccompiler::sock_filter]) -> String {
        filter
            .iter()
            .flat_map(|f| {
                let mut b = f.code.to_ne_bytes().to_vec();
                b.extend([f.jt, f.jf]);
                b.extend(f.k.to_ne_bytes());
                b
            })
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Why no python3 of this build's architecture can make the calls, if
    /// none can. Under qemu-user, as `make arm-test` runs the tests,
    /// python3 is the host's, of another architecture than the filter.
    fn no_caller() -> Option<String> {
        let machine = Command::new("python3")
            .args(["-I", "-c", "import platform; print(platform.machine())"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|m| m.trim().to_owned());
        match machine {
            None => Some("no python3 to make the calls with".to_owned()),
            Some(m) if m != std::env::consts::ARCH => Some(format!(
                "python3 runs on {m}, and the filter is for {}",
                std::env::consts::ARCH
            )),
            Some(_) => None,
        }
    }

    /// Makes `calls` in a python3 in `dir`, under the program that
    /// `filter` builds for the python3's process id, or under none: each
    /// call's errno, 0 for a call that succeeds.
    fn errnos(
        dir: &std::path::Path,
        filter: Option<&dyn Fn(u32) -> Program>,
        calls: &[String],
    ) -> Vec<i32> {
        let mut python = Command::new("python3")
            .args(["-I", "-c", CALLER])
            .arg(dir)
            .args(calls)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut out = BufReader::new(python.stdout.take().unwrap());
        let mut pid = String::new();
        out.read_line(&mut pid).unwrap();
        let code = filter.map_or_else(String::new, |f| hex(&f(pid.trim().parse().unwrap())));
        let mut stdin = python.stdin.take().unwrap();
        writeln!(stdin, "{code}").unwrap();
        drop(stdin);
        let mut line = String::new();
        out.read_to_string(&mut line).unwrap();
        assert!(python.wait().unwrap().success(), "{line}");
        line.split_whitespace()
            .map(|e| e.parse().unwrap())
            .collect()
    }

    /// A folder for one run of the calls, holding the files they change.
    fn files(name: &str) -> Scratch {
        let dir = Scratch::new(&format!("seccomp-{name}"));
        for file in ["existing", "victim", "victim2", "from", "from2", "from3"] {
            std::fs::write(dir.join(file), b"font").unwrap();
        }
        for folder in ["empty", "empty2"] {
            std::fs::create_dir(dir.join(folder)).unwrap();
        }
        dir
    }

    // each call the fallback filter refuses answers EPERM under it, or
    // ENOSYS for openat2, where without it the same call does something
    // else, and each call it lets through still succeeds (ADR-1200 D15,
    // the maintainer, 2026-11-13). The landlock domain is never entered,
    // as on a kernel without landlock, where the helper installs this
    // filter in its place. A call that answers the same without the
    // filter, such as one this kernel lacks, is named and not judged.
    // covers: none
    #[test]
    fn the_fallback_filter_refuses_what_the_landlock_domain_refuses() {
        if let Some(why) = no_caller() {
            eprintln!("skipped: {why}");
            return;
        }
        let at = libc::AT_FDCWD.to_string();
        let call = |nr: i64, args: &[&str]| {
            let mut spec = nr.to_string();
            for a in args {
                spec.push(',');
                spec.push_str(if *a == "AT" { &at } else { a });
            }
            spec
        };
        let flag = |f: i32| f.to_string();
        let (wronly, rdwr) = (flag(libc::O_WRONLY), flag(libc::O_RDWR));
        let create = flag(libc::O_CREAT);
        let trunc = flag(libc::O_TRUNC);
        let fifo = flag(i32::try_from(libc::S_IFIFO | 0o600).unwrap());
        let removedir = flag(libc::AT_REMOVEDIR);
        let fionread = libc::FIONREAD.to_string();
        let fioclex = libc::FIOCLEX.to_string();
        let (setown, setown_ex) = (flag(libc::F_SETOWN), "15".to_owned());
        let getfl = flag(libc::F_GETFL);
        let eperm = libc::EPERM;
        // each call, and what it answers under the filter
        let mut refused: Vec<(&str, String, i32)> = vec![
            (
                "openat to write",
                call(libc::SYS_openat, &["AT", "@existing", &wronly]),
                eperm,
            ),
            (
                "openat to read and write",
                call(libc::SYS_openat, &["AT", "@existing", &rdwr]),
                eperm,
            ),
            (
                "openat to create",
                call(libc::SYS_openat, &["AT", "@created", &create, "384"]),
                eperm,
            ),
            (
                "openat to truncate",
                call(libc::SYS_openat, &["AT", "@existing", &trunc]),
                eperm,
            ),
            (
                "openat2",
                call(libc::SYS_openat2, &["AT", "@existing", "how", "24"]),
                libc::ENOSYS,
            ),
            (
                "mkdirat",
                call(libc::SYS_mkdirat, &["AT", "@made", "448"]),
                eperm,
            ),
            (
                "mknodat",
                call(libc::SYS_mknodat, &["AT", "@fifo", &fifo, "0"]),
                eperm,
            ),
            (
                "unlinkat a file",
                call(libc::SYS_unlinkat, &["AT", "@victim", "0"]),
                eperm,
            ),
            (
                "unlinkat a folder",
                call(libc::SYS_unlinkat, &["AT", "@empty", &removedir]),
                eperm,
            ),
            (
                "renameat2",
                call(libc::SYS_renameat2, &["AT", "@from", "AT", "@to", "0"]),
                eperm,
            ),
            (
                "linkat",
                call(libc::SYS_linkat, &["AT", "@existing", "AT", "@hard", "0"]),
                eperm,
            ),
            (
                "symlinkat",
                call(libc::SYS_symlinkat, &["@existing", "AT", "@soft"]),
                eperm,
            ),
            (
                "truncate",
                call(libc::SYS_truncate, &["@existing", "0"]),
                eperm,
            ),
            (
                "kill another process",
                call(libc::SYS_kill, &["ppid", "0"]),
                eperm,
            ),
            (
                "tgkill another process",
                call(libc::SYS_tgkill, &["ppid", "ppid", "0"]),
                eperm,
            ),
            ("tkill", call(libc::SYS_tkill, &["pid", "0"]), eperm),
            (
                "rt_sigqueueinfo to another process",
                call(libc::SYS_rt_sigqueueinfo, &["ppid", "0", "siginfo"]),
                eperm,
            ),
            (
                "rt_tgsigqueueinfo to another process",
                call(
                    libc::SYS_rt_tgsigqueueinfo,
                    &["ppid", "ppid", "0", "siginfo"],
                ),
                eperm,
            ),
            (
                "pidfd_send_signal",
                call(libc::SYS_pidfd_send_signal, &["ppidfd", "0", "0", "0"]),
                eperm,
            ),
            (
                "F_SETOWN",
                call(libc::SYS_fcntl, &["fd", &setown, "ppid"]),
                eperm,
            ),
            (
                "F_SETOWN_EX",
                call(libc::SYS_fcntl, &["fd", &setown_ex, "owner"]),
                eperm,
            ),
            (
                "ptrace",
                call(libc::SYS_ptrace, &["2", "ppid", "0", "0"]),
                eperm,
            ),
            (
                "process_vm_readv",
                call(
                    libc::SYS_process_vm_readv,
                    &["pid", "0", "0", "0", "0", "0"],
                ),
                eperm,
            ),
            (
                "process_vm_writev",
                call(
                    libc::SYS_process_vm_writev,
                    &["pid", "0", "0", "0", "0", "0"],
                ),
                eperm,
            ),
            (
                "process_madvise",
                call(libc::SYS_process_madvise, &["pidfd", "0", "0", "20", "0"]),
                eperm,
            ),
            (
                "pidfd_getfd",
                call(libc::SYS_pidfd_getfd, &["pidfd", "0", "0"]),
                eperm,
            ),
            (
                "kcmp",
                call(libc::SYS_kcmp, &["pid", "pid", "0", "0", "0"]),
                eperm,
            ),
            (
                "ioctl FIONREAD",
                call(libc::SYS_ioctl, &["fd", &fionread, "int"]),
                eperm,
            ),
        ];
        #[cfg(not(target_arch = "riscv64"))]
        refused.push((
            "renameat",
            call(libc::SYS_renameat, &["AT", "@from2", "AT", "@to2"]),
            eperm,
        ));
        #[cfg(any(target_arch = "x86_64", target_arch = "arm"))]
        refused.extend([
            (
                "open to write",
                call(libc::SYS_open, &["@existing", &wronly]),
                eperm,
            ),
            ("creat", call(libc::SYS_creat, &["@created2", "384"]), eperm),
            ("mkdir", call(libc::SYS_mkdir, &["@made2", "448"]), eperm),
            (
                "mknod",
                call(libc::SYS_mknod, &["@fifo2", &fifo, "0"]),
                eperm,
            ),
            ("rmdir", call(libc::SYS_rmdir, &["@empty2"]), eperm),
            ("unlink", call(libc::SYS_unlink, &["@victim2"]), eperm),
            ("rename", call(libc::SYS_rename, &["@from3", "@to3"]), eperm),
            (
                "link",
                call(libc::SYS_link, &["@existing", "@hard2"]),
                eperm,
            ),
            (
                "symlink",
                call(libc::SYS_symlink, &["@existing", "@soft2"]),
                eperm,
            ),
        ]);
        #[cfg(target_arch = "arm")]
        refused.extend([
            (
                "truncate64",
                call(libc::SYS_truncate64, &["@existing", "0", "0", "0"]),
                eperm,
            ),
            (
                "fcntl64 F_SETOWN",
                call(libc::SYS_fcntl64, &["fd", &setown, "ppid"]),
                eperm,
            ),
        ]);
        // a call of the x32 numbering: getpid
        #[cfg(target_arch = "x86_64")]
        refused.push((
            "x32 getpid",
            call(0x4000_0000 | libc::SYS_getpid, &[]),
            eperm,
        ));
        let allowed = [
            (
                "openat to read",
                call(libc::SYS_openat, &["AT", "@existing", "0"]),
            ),
            ("kill itself", call(libc::SYS_kill, &["pid", "0"])),
            (
                "tgkill itself",
                call(libc::SYS_tgkill, &["pid", "pid", "0"]),
            ),
            ("F_GETFL", call(libc::SYS_fcntl, &["fd", &getfl])),
            ("ioctl FIOCLEX", call(libc::SYS_ioctl, &["fd", &fioclex])),
            ("getpid", call(libc::SYS_getpid, &[])),
        ];
        let calls: Vec<String> = refused
            .iter()
            .map(|r| r.1.clone())
            .chain(allowed.iter().map(|a| a.1.clone()))
            .collect();
        let outside = errnos(&files("outside"), None, &calls);
        let fallback = |pid| fallback_filter(pid).unwrap();
        let inside = errnos(&files("inside"), Some(&fallback), &calls);
        let mut unjudged = Vec::new();
        for (i, (name, _, answer)) in refused.iter().enumerate() {
            if outside[i] == *answer {
                unjudged.push(*name);
                continue;
            }
            assert_eq!(inside[i], *answer, "{name}, {} outside", outside[i]);
        }
        for (i, (name, _)) in allowed.iter().enumerate() {
            let at = refused.len() + i;
            assert_eq!((outside[at], inside[at]), (0, 0), "{name}");
        }
        if !unjudged.is_empty() {
            eprintln!("answered the same outside the filter, so not judged: {unjudged:?}");
        }
    }

    // the socket filter refuses making a socket and io_uring, and on
    // x86_64 every call of the x32 numbering, and lets a read through
    // covers: none
    #[test]
    fn the_socket_filter_refuses_sockets_and_x32_calls() {
        if let Some(why) = no_caller() {
            eprintln!("skipped: {why}");
            return;
        }
        let mut refused = vec![
            format!(
                "{},{},{},0",
                libc::SYS_socket,
                libc::AF_INET,
                libc::SOCK_DGRAM
            ),
            format!(
                "{},{},{},0,int",
                libc::SYS_socketpair,
                libc::AF_UNIX,
                libc::SOCK_STREAM
            ),
            format!("{},1,0", libc::SYS_io_uring_setup),
        ];
        #[cfg(target_arch = "x86_64")]
        refused.push(format!(
            "{},{},{},0",
            0x4000_0000 | libc::SYS_socket,
            libc::AF_INET,
            libc::SOCK_DGRAM
        ));
        refused.push(format!("{}", libc::SYS_getpid));
        let outside = errnos(&files("socket-outside"), None, &refused);
        let sockets = |_| socket_filter().unwrap();
        let inside = errnos(&files("socket-inside"), Some(&sockets), &refused);
        let n = refused.len() - 1;
        for i in 0..n {
            if outside[i] != libc::EPERM {
                assert_eq!(inside[i], libc::EPERM, "{}", refused[i]);
            }
        }
        assert_eq!((outside[n], inside[n]), (0, 0), "getpid");
    }

    // each rule's jumps land on its own return or on the next rule's
    // first instruction, and the program ends by letting the call go on
    #[test]
    fn every_jump_lands_inside_the_program() {
        for p in [socket_filter().unwrap(), fallback_filter(4242).unwrap()] {
            for (i, f) in p.iter().enumerate() {
                if f.code & 0x07 == 0x05 {
                    assert!(i + 1 + usize::from(f.jt.max(f.jf)) < p.len(), "{i}: {f:?}");
                }
            }
            assert_eq!(p.last().map(|f| (f.code, f.k)), Some((0x06, 0x7fff_0000)));
        }
    }

    // a rule that names more values than a jump can pass, or a rule that
    // refuses one of no values, is refused, not built wrong
    #[test]
    fn a_rule_past_a_jumps_reach_is_refused() {
        let values: Vec<u32> = (0..300).collect();
        assert!(compile(&[Rule::NoneOf(libc::SYS_ioctl, 1, values)]).is_err());
        assert!(compile(&[Rule::OneOf(libc::SYS_fcntl, 1, Vec::new())]).is_err());
        assert!(compile(&[Rule::Always(libc::SYS_kill)]).is_ok());
    }
}
