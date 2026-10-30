//! The parent's side of the parser's child (ADR-1200 D14), as the tests
//! run it: the binary started as `vtium-vitrify --parse` with an empty
//! environment, `/` as its directory and no descriptor but its three
//! pipes, a request written to it and its answer checked field by field.
//! Nothing in the shipped binary starts the child (`serve` parses in its
//! own sandbox), so the parent's side lives with the tests.

#![allow(dead_code)]

use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use rustix::process::{Pid, PidfdFlags, Signal, geteuid, pidfd_open, pidfd_send_signal};
use vtium_vitrify::child::{PARSE_ARG, PARSER_WALL_SECONDS, Request};
use vtium_vitrify::glyphs::{GLYPHS_BYTES_MAX, Glyphs};
use vtium_vitrify::outline::{OUTLINE_ANSWER_BYTES_MAX, Outline, OutlineRequest};

/// The most bytes of the parser's message on failure the parent keeps.
const MESSAGE_MAX: u64 = 4096;

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
        let out = self.exchange(request.encode(), GLYPHS_BYTES_MAX)?;
        Glyphs::decode(&out).map_err(|why| format!("the parser's answer was refused: {why}"))
    }

    /// Runs the parser on an outline font and checks what it gives back
    /// against `request`.
    pub fn rasterise(&self, request: &OutlineRequest) -> Result<Outline, String> {
        let out = self.exchange(request.encode(), OUTLINE_ANSWER_BYTES_MAX)?;
        Outline::decode(&out, request)
            .map_err(|why| format!("the parser's answer was refused: {why}"))
    }

    /// Starts the parser, hands it `request`, piece by piece, and reads
    /// its answer, at most `answer_max` bytes; any failure of the parser's
    /// is the error.
    fn exchange(&self, request: Vec<u8>, answer_max: usize) -> Result<Vec<u8>, String> {
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
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let wrote = stdin.map(|mut stdin| stdin.write_all(&request));
            let mut out = Vec::new();
            let read = stdout.map(|stdout| {
                stdout
                    .take(
                        u64::try_from(answer_max)
                            .unwrap_or(u64::MAX)
                            .saturating_add(1),
                    )
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
        Ok(out)
    }
}
