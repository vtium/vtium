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

/// `bytes` as text, cut at [`OUTPUT_MAX`] with a line that says so.
fn bounded(bytes: &[u8]) -> String {
    match bytes.get(..OUTPUT_MAX) {
        Some(head) if bytes.len() > OUTPUT_MAX => format!(
            "{}\n[cut after {OUTPUT_MAX} bytes]\n",
            String::from_utf8_lossy(head)
        ),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// `text`'s lines, each indented four spaces, with a line feed after the
/// last.
pub fn indented(text: &str) -> String {
    text.lines().map(|l| format!("    {l}\n")).collect()
}

/// The words a part says after an error reading `what`: the error, and the
/// command that reads it where root could.
pub fn cannot(what: &str, e: &io::Error) -> String {
    let root = if e.kind() == io::ErrorKind::PermissionDenied {
        "; needs root: sudo vtium-config report"
    } else {
        ""
    };
    format!("cannot read {what}: {e}{root}")
}
