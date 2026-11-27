//! What `vtium console` runs against: `systemctl` and `loginctl`, the root
//! it reads, and the links, unit files and records it finds there.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{GETTY, MASK, vt_digits};

/// What a `systemctl` run gave back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    /// Whether it exited with status 0.
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs systemd's `systemctl` and `loginctl` with the arguments it is
/// given.
pub trait Systemd {
    fn systemctl(&mut self, args: &[&str]) -> io::Result<Reply>;
    fn loginctl(&mut self, args: &[&str]) -> io::Result<Reply>;
}

/// The running system's `systemctl` and `loginctl`, found on `PATH`.
pub struct RunningSystemd;

impl RunningSystemd {
    fn run(program: &str, args: &[&str]) -> io::Result<Reply> {
        let out = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output()?;
        Ok(Reply {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

impl Systemd for RunningSystemd {
    fn systemctl(&mut self, args: &[&str]) -> io::Result<Reply> {
        RunningSystemd::run("systemctl", args)
    }

    fn loginctl(&mut self, args: &[&str]) -> io::Result<Reply> {
        RunningSystemd::run("loginctl", args)
    }
}

/// What the subcommand runs against.
pub struct Host<'a> {
    /// The system's root directory: `/`, or a scratch directory in a test.
    pub root: PathBuf,
    /// Whether the effective user is root.
    pub root_user: bool,
    /// Whether this vtium is built with `libseat` and `logind`, which seat
    /// mode needs.
    pub seat_build: bool,
    pub systemd: &'a mut dyn Systemd,
}

/// A symbolic link's state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum Link {
    /// Nothing is there.
    #[default]
    Absent,
    /// A link, to this path.
    To(String),
    /// Something that is not a link.
    Other,
}

impl Link {
    pub(super) fn read(path: &Path) -> Link {
        match std::fs::symlink_metadata(path) {
            Err(_) => Link::Absent,
            Ok(meta) if meta.file_type().is_symlink() => match std::fs::read_link(path) {
                Ok(to) => Link::To(to.to_string_lossy().into_owned()),
                Err(_) => Link::Other,
            },
            Ok(_) => Link::Other,
        }
    }

    /// The template prefix of the unit the link names, as systemd reads
    /// it from the name of the file it links to: `vtium` for
    /// `/usr/lib/systemd/system/vtium@.service`.
    pub(super) fn unit(&self) -> Option<&str> {
        match self {
            Link::To(to) => template_prefix(to),
            _ => None,
        }
    }

    pub(super) fn names(&self, prefix: &str) -> bool {
        self.unit() == Some(prefix)
    }

    pub(super) fn masks(&self) -> bool {
        *self == Link::To(MASK.to_owned())
    }

    /// The name of the file the link names: `sddm.service`.
    pub(super) fn file_name(&self) -> Option<&str> {
        match self {
            Link::To(to) => Path::new(to).file_name()?.to_str(),
            _ => None,
        }
    }
}

/// Where a template's unit file is, as systemd finds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum UnitFile {
    /// At this path on the running system.
    At(String),
    /// Masked: the first directory that has it links it to `/dev/null`.
    Masked,
    Missing,
}

/// What a VT's record says the VT had before `enable` took it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Record {
    /// No getty was enabled.
    NoGetty,
    /// `getty@ttyN` was enabled, its want linked to this path.
    Getty(String),
    /// The record holds neither, and is taken as no record.
    Unreadable,
}

impl Record {
    pub(super) fn parse(text: &str) -> Record {
        match text.strip_suffix('\n').unwrap_or(text) {
            "" => Record::NoGetty,
            path if !path.contains('\n') && template_prefix(path) == Some(GETTY) => {
                Record::Getty(path.to_owned())
            }
            _ => Record::Unreadable,
        }
    }
}

/// `PREFIX` of a path whose file name is `PREFIX@.service`.
pub(super) fn template_prefix(path: &str) -> Option<&str> {
    Path::new(path)
        .file_name()?
        .to_str()?
        .strip_suffix("@.service")
        .filter(|p| !p.is_empty())
}

/// The names whose `NAME.d` directories hold drop-ins for the unit `name`,
/// most specific first, as systemd expands them: `foo-bar-baz.service`,
/// then `foo-bar-.service` and `foo-.service`.
pub(super) fn drop_in_names(name: &str) -> Vec<String> {
    let mut out = vec![name.to_owned()];
    let mut prefix = name.strip_suffix(".service").unwrap_or_default();
    while let Some((head, _)) = prefix.rsplit_once('-') {
        if head.is_empty() {
            break;
        }
        out.push(format!("{head}-.service"));
        prefix = head;
    }
    out
}

/// `(PREFIX, N)` of a unit named `PREFIX@ttyN.service`.
pub(super) fn instance(name: &str) -> Option<(&str, u16)> {
    let (prefix, inst) = name.strip_suffix(".service")?.split_once('@')?;
    let n = vt_digits(inst.strip_prefix("tty")?)?;
    (!prefix.is_empty()).then_some((prefix, n))
}
