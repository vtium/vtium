//! `vtium config` (ADR-1201 D12): the console binary replaces itself with
//! vtium-config, the font and settings tool, by exec, and hands it every
//! word after `config`, as git runs its subcommands. The console binary
//! holds only this hand-off, so the tool's code never enters it or the
//! minspec size budget.
//!
//! vtium finds the tool as it finds the glyph helper, since the tool is
//! the helper: at the path the `font-helper` key names, else the path vtium
//! was built with, `/usr/bin/vtium-config` by default ([`helper::program`]).
//! It opens and checks that file as it checks the helper
//! ([`helper::open_program`]), refusing one that is not a regular file,
//! that is setuid or setgid, that an account other than root or the user
//! running it owns, or that its group or others may write, and it runs the
//! file it opened, through `/proc/self/fd`, so a file swapped in after the
//! check never runs. A user may type `sudo vtium config ...`, so the file
//! that runs as root is one root could trust.

use std::ffi::OsString;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::config::{self, Config};
use crate::helper;
use crate::notice::config_tool as text;

/// The word that names the subcommand.
pub const WORD: &str = "config";

/// Runs the tool `config` names for the font helper with `args`, as the
/// effective user: opens and checks it, then hands `exec` the command that
/// runs the opened file. `exec` returns only when it could not run it.
/// Returns the exit status vtium then ends with, 1, having said why on
/// `err`.
pub fn run(
    config: &Config,
    args: &[OsString],
    exec: impl FnOnce(&mut Command) -> io::Error,
    err: &mut dyn Write,
) -> i32 {
    let program = helper::program(config);
    let me = rustix::process::geteuid().as_raw();
    let file = match helper::open_program(&program, me) {
        Ok(file) => file,
        Err(why) => {
            let _ = writeln!(err, "{}", text::refused(&why));
            return 1;
        }
    };
    let fd = file.as_raw_fd();
    let mut command = Command::new(format!("/proc/self/fd/{fd}"));
    command.arg0(&program).args(args);
    let why = exec(&mut command);
    let _ = writeln!(err, "{}", text::cannot_run(program.display(), why));
    1
}

/// `vtium config ARGS...`, run: `None` when `args` names another command,
/// so the console goes on. When it runs the tool it never returns.
pub fn subcommand(args: &[OsString]) -> Option<i32> {
    let (first, rest) = args.split_first()?;
    (first == WORD).then_some(())?;
    // The configuration files alone: every word after `config` is the
    // tool's. A file that does not read leaves the key at its default, as
    // at a start.
    let loaded = config::load(&[], &mut config::read_file);
    Some(run(
        &loaded.config,
        rest,
        |command| command.exec(),
        &mut io::stderr(),
    ))
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test's set-up fails loudly"
)]
mod tests {
    use std::cell::Cell;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::*;

    fn words(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    /// The configuration with `font-helper` naming `program`, and no file.
    fn naming(program: &Path) -> Config {
        let arg = OsString::from(format!("--font-helper={}", program.display()));
        config::load(&[arg], &mut |_| Ok(None)).config
    }

    /// A directory of a test's own, emptied, and removed when the test
    /// drops it, whether the test passes or fails.
    struct Scratch(PathBuf);

    impl std::ops::Deref for Scratch {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("vtium-config-tool-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    /// `run` on the tool at `program`: its status, what it wrote, and the
    /// command it would have run, or `None` when it ran nothing.
    fn tried(program: &Path, args: &[&str]) -> (i32, String, Option<Command>) {
        let ran = Cell::new(None);
        let mut err = Vec::new();
        let status = run(
            &naming(program),
            &words(args),
            |command| {
                let mut copy = Command::new(command.get_program());
                copy.args(command.get_args());
                ran.set(Some(copy));
                io::Error::from(io::ErrorKind::PermissionDenied)
            },
            &mut err,
        );
        (status, String::from_utf8(err).unwrap(), ran.into_inner())
    }

    #[test]
    fn a_trusted_tool_runs_from_its_opened_file_with_every_argument() {
        let dir = scratch("trusted");
        let tool = dir.join("vtium-config");
        fs::write(&tool, "").unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        let (status, err, ran) = tried(&tool, &["set", "two words", "--help"]);
        let ran = ran.expect("it ran the tool");
        // The opened file, not the path, which could name another file by
        // the time of the exec.
        let program = ran.get_program().to_string_lossy().into_owned();
        assert!(program.starts_with("/proc/self/fd/"), "{program}");
        assert_eq!(
            ran.get_args().collect::<Vec<_>>(),
            ["set", "two words", "--help"]
        );
        // The exec failed, so vtium says why and ends with 1.
        assert_eq!(status, 1);
        assert!(
            err.starts_with(&format!("vtium config: cannot start {}: ", tool.display()))
                && err.contains("through /proc/self/fd, so /proc must be mounted"),
            "{err}"
        );
    }

    #[test]
    fn an_untrusted_tool_is_refused_and_nothing_runs() {
        let dir = scratch("untrusted");
        let tool = dir.join("vtium-config");
        fs::write(&tool, "").unwrap();
        for (mode, why) in [
            (
                0o775,
                "can be changed by its group or by other users (mode 775)",
            ),
            (
                0o757,
                "can be changed by its group or by other users (mode 757)",
            ),
            (0o4755, "is setuid or setgid (mode 4755)"),
            (0o2755, "is setuid or setgid (mode 2755)"),
        ] {
            fs::set_permissions(&tool, fs::Permissions::from_mode(mode)).unwrap();
            let (status, err, ran) = tried(&tool, &["fonts"]);
            assert!(ran.is_none(), "mode {mode:o} ran");
            assert_eq!(status, 1);
            assert_eq!(
                err,
                format!(
                    "vtium config: vtium does not start the font helper {}: it {why}. vtium config runs vtium-config from the path in the font-helper setting.\n",
                    tool.display()
                )
            );
        }
    }

    #[test]
    fn a_missing_tool_a_directory_or_a_relative_path_is_refused() {
        let dir = scratch("missing");
        fs::create_dir_all(dir.join("dir")).unwrap();
        for (program, why) in [
            (dir.join("absent"), "cannot open the font helper"),
            (dir.join("dir"), "is not a regular file"),
            (PathBuf::from("bin/vtium-config"), "is not absolute"),
        ] {
            let (status, err, ran) = tried(&program, &["fonts"]);
            assert!(ran.is_none(), "{program:?} ran");
            assert_eq!(status, 1);
            assert!(
                err.starts_with("vtium config: ") && err.contains(why),
                "{err}"
            );
        }
    }

    #[test]
    fn only_the_word_config_first_is_the_subcommand() {
        assert_eq!(subcommand(&words(&[])), None);
        assert_eq!(subcommand(&words(&["check-config"])), None);
        assert_eq!(subcommand(&words(&["--debug", "config"])), None);
    }
}
