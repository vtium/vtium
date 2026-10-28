//! The parser's child process (ADR-1200 D14). The parent reads each
//! source's bytes as the invoking user and hands them to a child: the
//! same binary, run as `vtium-vitrify --parse`, with an empty environment,
//! `/` as its directory and no descriptor but its three pipes. Run as root,
//! the parent starts the child as `nobody`, its supplementary groups
//! dropped; the child then sets no-new-privileges and its limits, refuses
//! to run as root, parses, and writes the glyph form ([`Glyphs`]), which
//! the parent checks field by field. A panic, a fault, a limit reached or
//! a malformed answer fails that source and nothing else.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use std::time::Duration;

use rustix::process::{
    Pid, PidfdFlags, Resource, Rlimit, Signal, geteuid, pidfd_open, pidfd_send_signal, setrlimit,
};

use crate::decode;
use crate::glyphs::{GLYPHS_BYTES_MAX, Glyphs};

/// The argument that makes the binary the parser.
pub const PARSE_ARG: &str = "--parse";

/// The parser's address space: the largest font, decompressed, its glyph
/// form and the map of every codepoint, with room to spare.
pub const PARSER_MEMORY: u64 = 512 << 20;
/// The parser's CPU time, in seconds: the kernel ends it past this.
pub const PARSER_SECONDS: u64 = 30;
/// How long the parser may take by the wall clock: past it, the parser is
/// killed and its source fails, so `create` never hangs holding the
/// store's lock. Twice its CPU time, for a loaded machine.
pub const PARSER_WALL_SECONDS: u64 = 2 * PARSER_SECONDS;
/// The most bytes of the parser's message on failure the parent keeps.
const MESSAGE_MAX: u64 = 4096;

const REQUEST_MAGIC: &[u8; 8] = b"VTVREQ1\0";
/// The most bytes a request takes: a font and two unimaps, each at most a
/// PSF's bound.
const REQUEST_BYTES_MAX: u64 = 8 + 3 * (4 + vtium_font::PSF_BYTES_MAX as u64);

/// What the parser is given: a font file's bytes and the unimaps it may
/// take.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub font: Vec<u8>,
    /// The frit's `unimap=`.
    pub unimap: Option<Vec<u8>>,
    /// kbd's `def.uni`, for a font with no table and no `unimap=`.
    pub def_uni: Option<Vec<u8>>,
}

impl Request {
    fn encode(&self) -> Vec<u8> {
        let mut out = REQUEST_MAGIC.to_vec();
        for part in [
            Some(&self.font),
            self.unimap.as_ref(),
            self.def_uni.as_ref(),
        ] {
            match part {
                Some(bytes) => {
                    out.extend_from_slice(
                        &u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_le_bytes(),
                    );
                    out.extend_from_slice(bytes);
                }
                None => out.extend_from_slice(&u32::MAX.to_le_bytes()),
            }
        }
        out
    }

    fn decode(mut bytes: &[u8]) -> Result<Request, &'static str> {
        bytes = bytes
            .strip_prefix(&REQUEST_MAGIC[..])
            .ok_or("not a parse request")?;
        let mut part = || -> Result<Option<Vec<u8>>, &'static str> {
            let (len, rest) = bytes.split_first_chunk::<4>().ok_or("cut short")?;
            let len = u32::from_le_bytes(*len);
            if len == u32::MAX {
                bytes = rest;
                return Ok(None);
            }
            let len = usize::try_from(len).map_err(|_| "cut short")?;
            if len > rest.len() {
                return Err("cut short");
            }
            let (head, tail) = rest.split_at(len);
            bytes = tail;
            Ok(Some(head.to_vec()))
        };
        let font = part()?.ok_or("no font")?;
        let (unimap, def_uni) = (part()?, part()?);
        if !bytes.is_empty() {
            return Err("bytes after the request");
        }
        Ok(Request {
            font,
            unimap,
            def_uni,
        })
    }
}

/// The account the parser runs as when the parent is root: `nobody`, as
/// ADR-1400's image helper falls back to it, its uid and gid read from
/// `/etc/passwd`.
pub fn nobody(passwd: &str) -> Option<(u32, u32)> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        (fields.next() == Some("nobody")).then_some(())?;
        let _password = fields.next()?;
        let uid = fields.next()?.parse().ok()?;
        let gid = fields.next()?.parse().ok()?;
        (uid != 0 && gid != 0).then_some((uid, gid))
    })
}

/// How to start the parser.
#[derive(Clone, Debug)]
pub struct Parser {
    program: PathBuf,
    args: Vec<OsString>,
    /// The account to run as: set when the parent is root.
    account: Option<(u32, u32)>,
    /// How long the parser may take, by the wall clock, before it is
    /// killed.
    deadline: Duration,
}

impl Parser {
    /// This binary as the parser, run as `nobody` when the parent is root.
    pub fn new() -> Result<Parser, String> {
        let program = std::env::current_exe()
            .map_err(|e| format!("cannot find this program to start the parser: {e}"))?;
        let account = if geteuid().is_root() {
            let passwd = std::fs::read_to_string("/etc/passwd")
                .map_err(|e| format!("cannot read /etc/passwd for the parser's account: {e}"))?;
            Some(
                nobody(&passwd).ok_or(
                    "no unprivileged account to parse fonts as: /etc/passwd holds no nobody",
                )?,
            )
        } else {
            None
        };
        Ok(Parser {
            program,
            args: vec![PARSE_ARG.into()],
            account,
            deadline: Duration::from_secs(PARSER_WALL_SECONDS),
        })
    }

    /// A stand-in program as the parser, for tests of the parent's side.
    pub fn with_program(program: &Path, args: &[&str]) -> Parser {
        Parser {
            program: program.to_owned(),
            args: args.iter().map(OsString::from).collect(),
            account: None,
            deadline: Duration::from_secs(PARSER_WALL_SECONDS),
        }
    }

    /// This parser with another wall-clock deadline.
    pub fn deadline(mut self, deadline: Duration) -> Parser {
        self.deadline = deadline;
        self
    }

    /// Runs the parser on `request` and checks what it gives back.
    pub fn parse(&self, request: &Request) -> Result<Glyphs, String> {
        // a descriptor this process inherited without close-on-exec, one
        // root opened among them, would otherwise reach the parser
        vtium_platform::cloexec_above_stdio()
            .map_err(|e| format!("cannot keep this process's descriptors from the parser: {e}"))?;
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .env_clear()
            .current_dir("/")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some((uid, gid)) = self.account {
            // std drops the supplementary groups before setuid when root
            command.uid(uid).gid(gid);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("cannot start the parser: {e}"))?;
        // a pidfd names this child alone, even after it is reaped
        let pidfd = i32::try_from(child.id())
            .ok()
            .and_then(Pid::from_raw)
            .map(|pid| pidfd_open(pid, PidfdFlags::empty()));
        let (stdin, stdout, stderr) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take());
        // stderr is read on its own thread, so a parser that fills it
        // cannot stall the read of stdout
        let message = std::thread::spawn(move || {
            let mut message = String::new();
            if let Some(stderr) = stderr {
                let _ = stderr.take(MESSAGE_MAX).read_to_string(&mut message);
            }
            message
        });
        // the input, the answer and the exit on another, so the deadline
        // is kept however the parser behaves; the parser reads all its
        // input before it writes, and one that dies early closes its end
        let request = request.encode();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let wrote = stdin.map(|mut stdin| stdin.write_all(&request));
            let mut out = Vec::new();
            let read = stdout.map(|stdout| {
                stdout
                    .take(GLYPHS_BYTES_MAX as u64 + 1)
                    .read_to_end(&mut out)
            });
            let _ = tx.send((wrote, read, out, child.wait()));
        });
        let (wrote, read, out, status) = match rx.recv_timeout(self.deadline) {
            Ok(done) => done,
            Err(_) => {
                if let Some(Ok(fd)) = &pidfd {
                    let _ = pidfd_send_signal(fd, Signal::KILL);
                }
                // the thread reaps it once its pipes close
                return Err(format!(
                    "the parser took longer than {} s and was ended",
                    self.deadline.as_secs_f32()
                ));
            }
        };
        let status = status.map_err(|e| format!("cannot wait for the parser: {e}"))?;
        let message = message.join().unwrap_or_default();
        let message = message.trim();
        if let Some(signal) = status.signal() {
            return Err(format!("the parser was ended by signal {signal}"));
        }
        if !status.success() {
            return Err(if message.is_empty() {
                format!("the parser failed: {status}")
            } else {
                message.to_owned()
            });
        }
        if let Some(Err(e)) = wrote {
            return Err(format!("cannot write to the parser: {e}"));
        }
        if let Some(Err(e)) = read {
            return Err(format!("cannot read the parser's answer: {e}"));
        }
        Glyphs::decode(&out).map_err(|why| format!("the parser's answer was refused: {why}"))
    }
}

/// The parser's side: limits set, privilege checked, the request read
/// from stdin and parsed, the glyph form written to stdout. Returns the
/// exit status.
pub fn run_parser() -> u8 {
    match parser_main() {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "{e}");
            1
        }
    }
}

fn parser_main() -> Result<(), String> {
    rustix::thread::set_no_new_privs(true)
        .map_err(|e| format!("the parser cannot set no-new-privileges: {e}"))?;
    for (resource, limit) in [
        (Resource::As, PARSER_MEMORY),
        (Resource::Cpu, PARSER_SECONDS),
        (Resource::Fsize, 0),
        (Resource::Nproc, 0),
        (Resource::Core, 0),
    ] {
        let limit = Rlimit {
            current: Some(limit),
            maximum: Some(limit),
        };
        setrlimit(resource, limit).map_err(|e| format!("the parser cannot set its limits: {e}"))?;
    }
    if geteuid().is_root() {
        return Err("the parser refuses to run as root".to_owned());
    }
    let mut input = Vec::new();
    std::io::stdin()
        .take(REQUEST_BYTES_MAX)
        .read_to_end(&mut input)
        .map_err(|e| format!("the parser cannot read its request: {e}"))?;
    let request =
        Request::decode(&input).map_err(|why| format!("a malformed parse request: {why}"))?;
    let glyphs = decode::font(
        &request.font,
        request.unimap.as_deref(),
        request.def_uni.as_deref(),
    )?;
    std::io::stdout()
        .write_all(&glyphs.encode())
        .map_err(|e| format!("the parser cannot answer: {e}"))
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

    #[test]
    fn a_request_reads_back() {
        for r in [
            Request {
                font: vec![1, 2, 3],
                unimap: None,
                def_uni: Some(b"0x00 U+0041\n".to_vec()),
            },
            Request {
                font: vec![],
                unimap: Some(vec![]),
                def_uni: None,
            },
        ] {
            assert_eq!(Request::decode(&r.encode()), Ok(r));
        }
        let mut long = Request::default().encode();
        long.push(0);
        assert!(Request::decode(&long).is_err());
        assert!(Request::decode(b"VTVREQ1\0\x05\0\0\0ab").is_err());
    }

    #[test]
    fn nobody_is_read_from_passwd() {
        let passwd = "root:x:0:0::/root:/bin/sh\nnobody:x:65534:65534:Nobody:/:/usr/bin/nologin\n";
        assert_eq!(nobody(passwd), Some((65534, 65534)));
        assert_eq!(nobody("root:x:0:0::/root:/bin/sh\n"), None);
        // a nobody with uid 0 is no unprivileged account
        assert_eq!(nobody("nobody:x:0:0::/:/bin/sh\n"), None);
    }

    fn stand_in(script: &str) -> Parser {
        Parser::with_program(Path::new("/bin/sh"), &["-c", script])
    }

    #[test]
    fn a_parser_that_dies_fails_the_source() {
        let e = stand_in("kill -ABRT $$")
            .parse(&Request::default())
            .expect_err("died");
        assert_eq!(e, "the parser was ended by signal 6");
        let e = stand_in("kill -SEGV $$")
            .parse(&Request::default())
            .expect_err("died");
        assert_eq!(e, "the parser was ended by signal 11");
    }

    #[test]
    fn a_parser_that_fails_gives_its_message() {
        let e = stand_in("cat >/dev/null; echo 'bad PSF header' >&2; exit 1")
            .parse(&Request::default())
            .expect_err("failed");
        assert_eq!(e, "bad PSF header");
        let e = stand_in("exit 101")
            .parse(&Request::default())
            .expect_err("failed");
        assert!(e.contains("101"), "{e}");
    }

    #[test]
    fn a_malformed_answer_is_refused() {
        let e = stand_in("cat >/dev/null; printf VTVGLYF1junk")
            .parse(&Request::default())
            .expect_err("refused");
        assert!(e.starts_with("the parser's answer was refused"), "{e}");
        // a parser that answers without reading its input is still read
        let e = stand_in("printf nonsense")
            .parse(&Request {
                font: vec![0; 1 << 20],
                ..Request::default()
            })
            .expect_err("refused");
        assert!(e.contains("refused") || e.contains("write"), "{e}");
    }

    #[test]
    fn a_parser_past_its_deadline_is_ended() {
        let e = stand_in("sleep 30")
            .deadline(std::time::Duration::from_millis(300))
            .parse(&Request::default())
            .expect_err("ended");
        assert!(e.contains("longer than"), "{e}");
    }

    #[test]
    fn a_parser_that_floods_stderr_does_not_hang_the_parent() {
        let e = stand_in("cat >/dev/null; head -c 1000000 /dev/zero | tr '\\0' x >&2; exit 1")
            .deadline(std::time::Duration::from_secs(10))
            .parse(&Request::default())
            .expect_err("failed");
        assert!(e.starts_with("xxxx") && e.len() <= 4096, "{}", e.len());
    }

    #[test]
    fn the_parsers_environment_is_empty() {
        // the shell exports PWD, SHLVL and _ of its own; nothing else may
        // reach the parser, and it runs in /
        let e = stand_in("cat >/dev/null; env | grep -v -E '^(PWD|SHLVL|_)=' >&2; pwd >&2; exit 1")
            .parse(&Request::default())
            .expect_err("reports");
        assert_eq!(e, "/");
    }

    #[test]
    fn the_parser_holds_only_its_pipes() {
        // dup's copy is not close-on-exec, as a descriptor the invoking
        // process passes down is not
        let null = std::fs::File::open("/dev/null").unwrap();
        let held = rustix::io::dup(&null).unwrap();
        let e = stand_in("cat >/dev/null; ls /proc/self/fd >&2; exit 1")
            .parse(&Request::default())
            .expect_err("reports");
        // ls opens one descriptor of its own to read the directory
        let fds: Vec<&str> = e.lines().collect();
        let held = held.as_raw_fd().to_string();
        assert!(
            fds.len() <= 4
                && ["0", "1", "2"].iter().all(|f| fds.contains(f))
                && !fds.contains(&held.as_str()),
            "{fds:?}"
        );
    }
}
