//! `vtium config` (ADR-1201 D12): the console binary replaces itself with
//! vtium-config, the font and settings tool, by exec, and hands it every
//! word after `config`, as git runs its subcommands. The console binary
//! holds only this hand-off, so the tool's code never enters it or the
//! minspec size budget.
//!
//! vtium looks for the tool beside its own binary first, so a build runs
//! the tool built with it, then at the path it was built with for its glyph
//! helper, `/usr/bin/vtium-config` by default ([`helper::PROGRAM`]), since
//! the tool is also the helper.

use std::ffi::OsString;
use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::helper;
use crate::notice::config_tool as text;

/// The word that names the subcommand.
pub const WORD: &str = "config";

/// The tool's file name, beside the console binary.
pub const NAME: &str = "vtium-config";

/// Where `vtium config` looks for the tool, in order: beside the running
/// binary `exe`, then at [`helper::PROGRAM`], each place once.
pub fn candidates(exe: Option<&Path>) -> Vec<PathBuf> {
    let mut places: Vec<PathBuf> = exe
        .and_then(Path::parent)
        .map(|dir| dir.join(NAME))
        .into_iter()
        .collect();
    let built = PathBuf::from(helper::PROGRAM);
    if !places.contains(&built) {
        places.push(built);
    }
    places
}

/// Runs the first of `candidates` that is a file with `args`, through
/// `exec`, which returns only when it could not run it. Returns the exit
/// status vtium then ends with, 1, having said why on `err`.
pub fn hand_off(
    args: &[OsString],
    candidates: &[PathBuf],
    exec: impl FnOnce(&Path, &[OsString]) -> io::Error,
    err: &mut dyn Write,
) -> i32 {
    let Some(program) = candidates.iter().find(|p| p.is_file()) else {
        let places: Vec<String> = candidates.iter().map(|p| p.display().to_string()).collect();
        let _ = writeln!(err, "{}", text::missing(&places.join(", ")));
        return 1;
    };
    let why = exec(program, args);
    let _ = writeln!(err, "{}", text::cannot_run(program.display(), why));
    1
}

/// `vtium config ARGS...`, run: `None` when `args` names another command,
/// so the console goes on. When it runs the tool it never returns.
pub fn subcommand(args: &[OsString]) -> Option<i32> {
    let (first, rest) = args.split_first()?;
    (first == WORD).then_some(())?;
    let exe = std::env::current_exe().ok();
    Some(hand_off(
        rest,
        &candidates(exe.as_deref()),
        |program, args| Command::new(program).arg0(NAME).args(args).exec(),
        &mut io::stderr(),
    ))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a test's set-up fails loudly")]
mod tests {
    use std::cell::RefCell;
    use std::fs;

    use super::*;

    fn words(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    /// A directory of the test's own, emptied.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vtium-config-tool-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_tool_beside_the_binary_comes_before_the_built_path() {
        assert_eq!(
            candidates(Some(Path::new("/opt/vt/bin/vtium"))),
            [
                PathBuf::from("/opt/vt/bin/vtium-config"),
                PathBuf::from(helper::PROGRAM)
            ]
        );
        // A binary installed beside the built path looks there once.
        let beside = Path::new(helper::PROGRAM).with_file_name("vtium");
        assert_eq!(candidates(Some(&beside)), [PathBuf::from(helper::PROGRAM)]);
        // A binary whose path is unknown looks at the built path alone.
        assert_eq!(candidates(None), [PathBuf::from(helper::PROGRAM)]);
    }

    #[test]
    fn the_first_tool_found_runs_with_every_argument() {
        let dir = scratch("found");
        let missing = dir.join("absent").join(NAME);
        let tool = dir.join(NAME);
        fs::write(&tool, "").unwrap();
        let ran = RefCell::new(None);
        let mut err = Vec::new();
        let status = hand_off(
            &words(&["set", "two words", "--help"]),
            &[missing, tool.clone()],
            |program, args| {
                *ran.borrow_mut() = Some((program.to_owned(), args.to_vec()));
                io::Error::from(io::ErrorKind::PermissionDenied)
            },
            &mut err,
        );
        assert_eq!(
            ran.into_inner(),
            Some((tool.clone(), words(&["set", "two words", "--help"])))
        );
        // The exec failed, so vtium says why and ends with 1.
        assert_eq!(status, 1);
        let err = String::from_utf8(err).unwrap();
        assert!(
            err.starts_with(&format!("vtium config: cannot run {}: ", tool.display())),
            "{err}"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn no_tool_names_every_place_looked_and_runs_nothing() {
        let dir = scratch("missing");
        let places = [dir.join("a").join(NAME), dir.join("b").join(NAME)];
        // A directory by the tool's name is no tool.
        fs::create_dir_all(&places[1]).unwrap();
        let ran = RefCell::new(false);
        let mut err = Vec::new();
        let status = hand_off(
            &words(&["fonts"]),
            &places,
            |_, _| {
                *ran.borrow_mut() = true;
                io::Error::from(io::ErrorKind::NotFound)
            },
            &mut err,
        );
        assert!(!ran.into_inner(), "it ran nothing");
        assert_eq!(status, 1);
        assert_eq!(
            String::from_utf8(err).unwrap(),
            format!(
                "vtium config: no vtium-config at {}, {}. vtium config runs vtium-config, vtium's font and settings tool: install it beside vtium.\n",
                places[0].display(),
                places[1].display()
            )
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_the_word_config_first_is_the_subcommand() {
        assert_eq!(subcommand(&words(&[])), None);
        assert_eq!(subcommand(&words(&["check-config"])), None);
        assert_eq!(subcommand(&words(&["--debug", "config"])), None);
    }
}
