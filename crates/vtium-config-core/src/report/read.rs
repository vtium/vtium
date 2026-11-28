//! How the report reaches the machine: it reads files under the
//! machine's root and runs the programs that root holds, and does nothing
//! else. Every path the report names is an absolute one, such as
//! `/etc/os-release`, read under the root, so the document names the path
//! as it is on the machine.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::Machine;

/// The most bytes read of a file the document quotes.
pub const FILE_MAX: u64 = 256 * 1024;

/// The most bytes of a program's output the document quotes.
const OUTPUT_MAX: usize = 1024 * 1024;

/// The most bytes read of a vtium binary, looking for its markers.
pub const BINARY_MAX: u64 = 256 * 1024 * 1024;

/// What a program the report ran printed, and how it exited.
pub struct Ran {
    /// Its exit status; `None` when a signal ended it.
    pub code: Option<i32>,
    pub out: String,
    pub err: String,
}

impl Machine {
    /// The absolute path `abs` under the root.
    pub fn path(&self, abs: &str) -> PathBuf {
        self.root.join(abs.trim_start_matches('/'))
    }

    /// At most `max` bytes of the file `abs`.
    pub fn bytes(&self, abs: &str, max: u64) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        File::open(self.path(abs))?
            .take(max)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    /// At most `max` bytes of the file `abs`, as text.
    pub fn text(&self, abs: &str, max: u64) -> io::Result<String> {
        self.bytes(abs, max)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    /// The names in the directory `abs`, sorted, a directory's with `/`
    /// after it.
    pub fn names(&self, abs: &str) -> io::Result<Vec<String>> {
        let mut names: Vec<String> = std::fs::read_dir(self.path(abs))?
            .filter_map(Result::ok)
            .map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    format!("{name}/")
                } else {
                    name
                }
            })
            .collect();
        names.sort();
        Ok(names)
    }

    /// The program `name` the root holds in `/usr/bin` or `/bin`.
    pub fn program(&self, name: &str) -> Option<PathBuf> {
        ["/usr/bin/", "/bin/"]
            .iter()
            .map(|dir| self.path(&format!("{dir}{name}")))
            .find(|p| p.is_file())
    }

    /// Runs `program` with `args`, with no input and an environment of its
    /// own, and returns what it printed.
    pub fn run(&self, program: &Path, args: &[&str]) -> io::Result<Ran> {
        let output = Command::new(program)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LC_ALL", "C.UTF-8")
            .env("SYSTEMD_COLORS", "0")
            .stdin(Stdio::null())
            .output()?;
        Ok(Ran {
            code: output.status.code(),
            out: bounded(&output.stdout),
            err: bounded(&output.stderr),
        })
    }
}

/// `bytes` as text, its newest [`OUTPUT_MAX`] bytes where there are more,
/// after a line that says what was left out, so a failure a program
/// prints last survives.
fn bounded(bytes: &[u8]) -> String {
    match bytes.len().checked_sub(OUTPUT_MAX) {
        Some(cut) if cut > 0 => format!(
            "[the first {cut} bytes left out]\n{}",
            String::from_utf8_lossy(bytes.get(cut..).unwrap_or_default())
        ),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// `text`'s lines, each indented four spaces, with a line feed after the
/// last.
pub fn indented(text: &str) -> String {
    text.lines().map(|l| format!("    {l}\n")).collect()
}

impl Machine {
    /// The words a part says after an error reading `what`: the error, and
    /// what would read it. A user refused with `EACCES` is told the command
    /// that reads it as root. Root refused with `EPERM` is told that the
    /// kernel's lockdown refuses it, since lockdown refuses debugfs even to
    /// root.
    pub fn cannot(&self, what: &str, e: &io::Error) -> String {
        let why = match (e.raw_os_error(), self.euid) {
            (Some(EACCES), euid) if euid != 0 => {
                "; needs root: sudo vtium-config report > vtium-report.txt"
            }
            (Some(EPERM), 0) => {
                "; the kernel refuses it even to root, because its lockdown mode blocks debugfs"
            }
            _ => "",
        };
        format!("cannot read {what}: {e}{why}")
    }
}

/// The errors [`Machine::cannot`] tells apart.
const EPERM: i32 = 1;
const EACCES: i32 = 13;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_past_its_bound_keeps_its_newest_bytes() {
        let mut big = vec![b'a'; OUTPUT_MAX];
        big.extend_from_slice(b"\nthe failure, printed last\n");
        let text = bounded(&big);
        assert!(
            text.ends_with("the failure, printed last\n"),
            "{}",
            &text[text.len() - 40..]
        );
        assert!(
            text.starts_with("[the first 27 bytes left out]\n"),
            "{}",
            &text[..40]
        );
        assert_eq!(bounded(b"short\n"), "short\n");
        assert_eq!(bounded(&vec![b'b'; OUTPUT_MAX]).len(), OUTPUT_MAX);
    }

    #[test]
    fn cannot_names_root_for_a_user_and_lockdown_for_root() {
        let m = |euid| Machine {
            root: "/".into(),
            euid,
            lines: 1,
            previous_boot: false,
            now: std::time::SystemTime::UNIX_EPOCH,
        };
        let acces = io::Error::from_raw_os_error(EACCES);
        let perm = io::Error::from_raw_os_error(EPERM);
        assert!(
            m(1000)
                .cannot("/x", &acces)
                .ends_with("needs root: sudo vtium-config report > vtium-report.txt")
        );
        assert!(!m(0).cannot("/x", &acces).contains("needs root"));
        let root = m(0).cannot("/sys/kernel/debug/dri/0/state", &perm);
        assert!(root.contains("lockdown"), "{root}");
        assert!(!root.contains("needs root"), "{root}");
        assert!(!m(1000).cannot("/x", &perm).contains("lockdown"));
        let gone = io::Error::from_raw_os_error(2);
        assert_eq!(m(0).cannot("/x", &gone), format!("cannot read /x: {gone}"));
    }
}
