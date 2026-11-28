//! How the console starts the font helper, and how it checks the helper's
//! program and the account the helper runs as.

use std::ffi::OsString;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::sync::Arc;

use rustix::fs::{FileType, Mode, OFlags};

use vtium_settings::batch::{Batch, Entry};

use crate::config::Config;
use crate::config::keys::FONT_HELPER;

use super::sandbox::sandbox_args;
use super::wire::{ACTIVE, ACTIVE_BYTES_MAX, ACTIVE_PATH_MAX, REQUEST_MAGIC, len32, put32};

/// The helper's program vtium is built with, `font-helper`'s default, set
/// at build time with `VTIUM_FONT_HELPER`: the key table's.
pub const PROGRAM: &str = vtium_settings::keys::HELPER_PROGRAM;

/// The account the helper runs as when the console is root: `nobody`,
/// its uid and gid read from `passwd`.
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

/// How to start the helper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// The uid and gid to run as; `None` keeps the console's.
    pub account: Option<(u32, u32)>,
}

impl Command {
    /// `program serve`, as `nobody` when the console is root; an error as
    /// root with no `nobody` in `/etc/passwd`, since the helper never runs
    /// as root.
    pub fn system(program: &Path) -> Result<Command, String> {
        let account = if rustix::process::geteuid().is_root() {
            let passwd = std::fs::read_to_string("/etc/passwd").map_err(|e| {
                format!("cannot read /etc/passwd to find the font helper's account: {e}")
            })?;
            Some(
                nobody(&passwd)
                    .ok_or("/etc/passwd has no user nobody to run the font helper as")?,
            )
        } else {
            None
        };
        Ok(Command {
            program: program.to_owned(),
            args: vec!["serve".into()],
            account,
        })
    }
}

/// The helper's program, opened once, and the command each start runs it
/// with. Every helper starts from the opened file, so a package upgrade
/// that renames a new program into place reaches a console only when the
/// console starts again, and an old console never talks to a new helper.
#[derive(Clone, Debug)]
pub struct Launcher {
    pub(super) command: Command,
    program: Arc<OwnedFd>,
    /// The active fonts' request, which each helper is sent first; `None`
    /// sends none, for a test's stand-in helper.
    pub(super) active: Option<Arc<[u8]>>,
}

/// The request that names the active fonts to the helper: `owner`, whose
/// files pass the trust rule beside root's, and each path `batch` lists,
/// in the helper's layout (vtium-config's `serve/wire.rs`). The helper refuses
/// the whole request over one path that is not absolute UTF-8, holds a NUL
/// byte or is too long, and then exits, so such a path is left out here
/// and the other active fonts still reach the helper. The paths stop
/// before they would pass the helper's bound, which a merged batch within
/// its own bound never reaches.
pub(super) fn active_request(owner: u32, batch: &Batch) -> Vec<u8> {
    let mut paths = Vec::new();
    let mut total = 0usize;
    for Entry::Path(path) in batch.entries() {
        let Some(text) = path
            .to_str()
            .filter(|t| t.starts_with('/') && !t.contains('\0') && t.len() <= ACTIVE_PATH_MAX)
        else {
            continue;
        };
        total = total.saturating_add(text.len());
        if total > ACTIVE_BYTES_MAX {
            break;
        }
        paths.push(text);
    }
    let mut out = REQUEST_MAGIC.to_vec();
    put32(&mut out, ACTIVE);
    put32(&mut out, owner);
    put32(&mut out, len32(paths.len()));
    for path in paths {
        put32(&mut out, len32(path.len()));
        out.extend_from_slice(path.as_bytes());
    }
    out
}

impl Launcher {
    /// Opens `command`'s program ([`open_program`]), for the account vtium
    /// runs as.
    pub fn open(command: Command) -> Result<Launcher, String> {
        let me = rustix::process::geteuid().as_raw();
        let program = open_program(&command.program, me)?;
        Ok(Launcher {
            command,
            program: Arc::new(program),
            active: None,
        })
    }

    /// The helper `config` names, `font-helper`, opened, with the font
    /// folders `font-paths` lists in place of the build's, and told the
    /// active fonts: the batch of the console's gate
    /// ([`crate::font::gate`]), under the account vtium runs as.
    ///
    /// References:
    /// - ADR-1200 D25: `font-paths` names the font folders.
    /// - ADR-1205 D4: the helper draws only from the active fonts.
    pub fn configured(config: &Config) -> Result<Launcher, String> {
        let mut command = Command::system(&program(config))?;
        command.args.extend(font_dirs(config));
        command.args.extend(sandbox_args(config));
        let me = rustix::process::geteuid().as_raw();
        Launcher::open(command).map(|l| l.with_active(me, &crate::font::gate().batch))
    }

    /// This launcher, telling each helper it starts that the fonts `batch`
    /// lists are the active ones, and that `owner`'s files pass the trust
    /// rule beside root's. `owner` is the account vtium runs as: root, or
    /// the account of a test or a run by hand, which the helper runs as
    /// too. The helper draws from no other font file.
    ///
    /// References:
    /// - ADR-1205 D4: vtium draws only from the active resources, and an unprivileged vtium accepts its own account's files.
    pub fn with_active(mut self, owner: u32, batch: &Batch) -> Launcher {
        self.active = Some(active_request(owner, batch).into());
        self
    }

    /// The request naming the active fonts that each helper this launcher
    /// starts is sent first; `None` when it sends none.
    pub fn active(&self) -> Option<&[u8]> {
        self.active.as_deref()
    }

    /// This launcher, starting each helper with the console's `locale`
    /// (`--locale`), so that the helper picks the built-in font the console
    /// draws, which in a zh, ja or ko locale is Unifont at every height.
    ///
    /// References:
    /// - ADR-1200 D7: the built-in font for a cell height and locale.
    /// - ADR-1204 D5: the built-in font draws the text of a set whose text
    ///   font is missing.
    pub fn with_locale(mut self, locale: &str) -> Launcher {
        self.command
            .args
            .extend([OsString::from("--locale"), OsString::from(locale)]);
        self
    }

    /// The arguments each helper starts with, for a test.
    #[cfg(test)]
    pub(crate) fn args(&self) -> &[OsString] {
        &self.command.args
    }

    /// The process that starts a helper: the opened program, through
    /// `/proc/self/fd`, which the kernel lets a process read as any
    /// account, with the program's path as its name.
    pub fn process(&self) -> Process {
        let c = &self.command;
        let mut process = Process::new(format!("/proc/self/fd/{}", self.program.as_raw_fd()));
        process
            .arg0(&c.program)
            .args(&c.args)
            .env_clear()
            .current_dir("/")
            .process_group(0);
        if let Some((uid, gid)) = c.account {
            // std drops the supplementary groups before setuid when root
            process.uid(uid).gid(gid);
        }
        process
    }

    /// Checks that the helper started as `pid` holds the account the
    /// console dropped it to, read back from its `/proc/<pid>/status` once
    /// it runs. std drops the supplementary
    /// groups before it sets the uid, but goes on when the kernel refuses
    /// it that, so a console without the capability to change its groups
    /// would start a helper holding root's groups. A helper whose four
    /// uids are not the account's uid, whose four gids are not its gid, or
    /// that holds any supplementary group is refused. A console that keeps
    /// its own account checks nothing, since the helper holds what the
    /// console holds.
    pub fn dropped(&self, pid: u32) -> Result<(), String> {
        let Some(account) = self.command.account else {
            return Ok(());
        };
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).map_err(|e| {
            format!("vtium cannot read the font helper's ids from /proc/{pid}/status: {e}")
        })?;
        holds_account(&status, account)
    }

    /// Why a start failed with `e`. vtium starts the helper from the file it
    /// opened, through `/proc/self/fd`, so the message says `/proc` must be
    /// mounted: with `/proc` unmounted the start fails there, not at the
    /// program's path.
    pub fn failed(&self, e: &std::io::Error) -> String {
        format!(
            "vtium cannot start the font helper {}: {e}. vtium starts it through /proc/self/fd, so /proc must be mounted.",
            self.command.program.display(),
        )
    }
}

/// The ids a `/proc/<pid>/status` line names after its `key`, such as
/// `Uid:`'s real, effective, saved and filesystem uids, or `Groups:`'s
/// supplementary groups; `None` when the line is missing or a field is not
/// a number.
pub(super) fn status_ids(status: &str, key: &str) -> Option<Vec<u32>> {
    let line = status.lines().find_map(|l| l.strip_prefix(key))?;
    line.split_whitespace().map(|id| id.parse().ok()).collect()
}

/// Whether `status`, a helper's `/proc/<pid>/status`, shows it holding
/// `(uid, gid)` alone: each of its four uids `uid`, each of its four gids
/// `gid`, and no supplementary group. Says why not.
pub(super) fn holds_account(status: &str, (uid, gid): (u32, u32)) -> Result<(), String> {
    let ids = |key: &str| {
        status_ids(status, key).ok_or_else(|| {
            format!("the helper's ids do not read: its status has no {key} line vtium reads")
        })
    };
    let (uids, gids, groups) = (ids("Uid:")?, ids("Gid:")?, ids("Groups:")?);
    if uids.len() != 4 || uids.iter().any(|&u| u != uid) {
        return Err(format!(
            "the helper runs as uids {uids:?}, not as {uid} alone, the account vtium started it as"
        ));
    }
    if gids.len() != 4 || gids.iter().any(|&g| g != gid) {
        return Err(format!(
            "the helper runs with gids {gids:?}, not with {gid} alone, the account vtium started it as"
        ));
    }
    if !groups.is_empty() {
        return Err(format!(
            "the helper holds the supplementary groups {groups:?}, which vtium could not drop as it started it as uid {uid}"
        ));
    }
    Ok(())
}

/// The options that hand the helper the folders `font-paths` lists, a
/// `--font-dir` before each, in their order; none when it is unset, so
/// the helper searches the build's list (ADR-1200 D25).
pub fn font_dirs(config: &Config) -> Vec<OsString> {
    config
        .text(&crate::config::keys::FONT_PATHS)
        .map(|paths| {
            paths
                .split(':')
                .flat_map(|dir| [OsString::from("--font-dir"), OsString::from(dir)])
                .collect()
        })
        .unwrap_or_default()
}

/// The helper's program `config` names, `font-helper`.
pub fn program(config: &Config) -> PathBuf {
    PathBuf::from(config.text(&FONT_HELPER).unwrap_or(PROGRAM))
}

/// Opens the helper's program at `path` to start it from, close-on-exec
/// so no child keeps it and above stdio so no child's stdio replaces it,
/// and checks the opened file, so the file checked is the file that runs. It refuses a path that is not absolute, a file
/// that is not a regular file, and a file an account other than root or
/// `me`, the account vtium runs as, could change: one another account
/// owns, or that its group or others may write; and a setuid or setgid
/// file, which would run with its owner's or group's rights.
pub fn open_program(path: &Path, me: u32) -> Result<OwnedFd, String> {
    let name = path.display();
    if !path.is_absolute() {
        return Err(format!(
            "vtium does not start the font helper {name}: the path is not absolute"
        ));
    }
    let opened = rustix::fs::open(path, OFlags::PATH | OFlags::CLOEXEC, Mode::empty())
        .map_err(|e| format!("cannot open the font helper {name}: {e}"))?;
    // A descriptor from 0 to 2 would be replaced by a child's stdio before
    // its exec. Rust's runtime opens /dev/null on each of them that vtium
    // starts with closed, so this keeps that from mattering to the helper
    // if it ever stops.
    let file = rustix::io::fcntl_dupfd_cloexec(&opened, 3)
        .map_err(|e| format!("cannot prepare the font helper {name}: {e}"))?;
    drop(opened);
    let stat = rustix::fs::fstat(&file).map_err(|e| {
        format!("cannot read the owner and permissions of the font helper {name}: {e}")
    })?;
    match refusal(
        FileType::from_raw_mode(stat.st_mode),
        stat.st_uid,
        stat.st_mode,
        me,
    ) {
        None => Ok(file),
        Some(why) => Err(format!(
            "vtium does not start the font helper {name}: it {why}"
        )),
    }
}

/// Why vtium refuses a helper program of `kind`, owned by `owner`, with
/// `mode`, when it runs as `me`; `None` when it does not.
pub(super) fn refusal(kind: FileType, owner: u32, mode: u32, me: u32) -> Option<String> {
    if kind != FileType::RegularFile {
        return Some("is not a regular file".to_owned());
    }
    if mode & 0o6000 != 0 {
        // vtium drops to nobody before the exec, and the helper sets
        // no_new_privs only after it, so a setuid root helper runs as root
        return Some(format!("is setuid or setgid (mode {:o})", mode & 0o7777));
    }
    if owner != 0 && owner != me {
        return Some(format!(
            "belongs to uid {owner}, which is neither root nor vtium's own account"
        ));
    }
    if mode & 0o022 != 0 {
        return Some(format!(
            "can be changed by its group or by other users (mode {:o})",
            mode & 0o7777
        ));
    }
    None
}
