//! A home of the tests' own: every test that runs vtium-config's binary
//! starts it through [`command`], which points `HOME`, `XDG_CONFIG_HOME`
//! and `XDG_STATE_HOME` at a directory under the test target's temporary
//! directory, so no test reads or writes the real user's drafts. A test
//! that needs a store of its own sets these again after.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

/// The variables [`command`] sets, each to a directory of the tests' own.
pub fn vars() -> [(&'static str, PathBuf); 3] {
    let home = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("home");
    [
        ("XDG_CONFIG_HOME", home.join(".config")),
        ("XDG_STATE_HOME", home.join(".local/state")),
        ("HOME", home),
    ]
}

/// `program`, to run with the tests' own home.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut c = Command::new(program);
    c.envs(vars());
    c
}
