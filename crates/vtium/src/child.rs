//! The session's command and its environment, which [`vtium_platform::Pty`]
//! runs on the pty.
//!
//! The command is `login`, or `/bin/login -p` with `reset-env` on and
//! `/bin/login` with it off. With `reset-env` on the environment starts
//! empty, and off it starts as vtium's own. Over it vtium sets `TERM`,
//! `COLORTERM=truecolor`, `XDG_SEAT` and `XDG_VTNR` when it knows them,
//! and `TERM_SESSION_TYPE`: `kms` on a DRM display, or `fb` on fbdev. Then each word
//! of the command that is `$NAME` becomes that variable of the
//! environment, empty when it is unset, and a word starting `$$` loses one
//! `$` (`src/misc/pty.c:252-302` and `src/shl/misc.h:395-449` at kmscon
//! b1f7a7c5).

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::process::Command;

use crate::config::Config;
use crate::config::keys::{LOGIN, RESET_ENV, TERM};

/// The display vtium draws on, which `TERM_SESSION_TYPE` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    /// A DRM device.
    Kms,
    /// An fbdev device.
    Fbdev,
}

/// Where the session runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place<'a> {
    pub seat: Option<&'a str>,
    pub vtnr: Option<u32>,
    pub display: Display,
}

/// The command to run and its whole environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Child {
    pub argv: Vec<OsString>,
    pub env: BTreeMap<OsString, OsString>,
}

/// The session's command for `config` at `place`. `inherited` is vtium's
/// own environment, used with `reset-env` off.
pub fn child(
    config: &Config,
    place: &Place<'_>,
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
) -> Child {
    let reset = config.switch(&RESET_ENV);
    let mut env: BTreeMap<OsString, OsString> = if reset {
        BTreeMap::new()
    } else {
        inherited.into_iter().collect()
    };
    let mut set = |k: &str, v: &str| env.insert(k.into(), v.into());
    set("TERM", config.text(&TERM).unwrap_or_default());
    set("COLORTERM", "truecolor");
    if let Some(seat) = place.seat {
        set("XDG_SEAT", seat);
    }
    if let Some(vtnr) = place.vtnr {
        set("XDG_VTNR", &vtnr.to_string());
    }
    let session_type = match place.display {
        Display::Kms => "kms",
        Display::Fbdev => "fb",
    };
    set("TERM_SESSION_TYPE", session_type);

    let words: Vec<String> = match config.command(&LOGIN) {
        Some(words) => words.to_vec(),
        None if reset => vec!["/bin/login".into(), "-p".into()],
        None => vec!["/bin/login".into()],
    };
    let argv = words
        .iter()
        .map(|w| match w.strip_prefix('$') {
            Some(escaped) if escaped.starts_with('$') => OsString::from(escaped),
            Some(name) => env.get(OsStr::new(name)).cloned().unwrap_or_default(),
            None => OsString::from(w),
        })
        .collect();
    Child { argv, env }
}

impl Child {
    /// The command for [`vtium_platform::Pty::spawn`]; `None` with no
    /// word to run.
    pub fn command(&self) -> Option<Command> {
        let (program, args) = self.argv.split_first()?;
        let mut command = Command::new(program);
        command.args(args).env_clear().envs(&self.env);
        Some(command)
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use super::*;
    use crate::config::load;

    fn config(args: &[&str]) -> Config {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        load(&args, None, &mut |_: &Path| Ok(None)).config
    }

    fn os(pairs: &[(&str, &str)]) -> BTreeMap<OsString, OsString> {
        pairs
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v)))
            .collect()
    }

    fn argv(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    const TTY2: Place<'static> = Place {
        seat: Some("seat0"),
        vtnr: Some(2),
        display: Display::Kms,
    };

    fn vtiums_own() -> Vec<(OsString, OsString)> {
        os(&[("HOME", "/root"), ("TERM", "linux"), ("PATH", "/usr/bin")])
            .into_iter()
            .collect()
    }

    #[test]
    fn the_default_is_login_with_the_environment_reset() {
        let c = child(&config(&[]), &TTY2, vtiums_own());
        assert_eq!(c.argv, argv(&["/bin/login", "-p"]));
        assert_eq!(
            c.env,
            os(&[
                ("COLORTERM", "truecolor"),
                ("TERM", "kmscon"),
                ("TERM_SESSION_TYPE", "kms"),
                ("XDG_SEAT", "seat0"),
                ("XDG_VTNR", "2"),
            ])
        );
    }

    #[test]
    fn without_reset_env_login_keeps_vtiums_environment() {
        let c = child(
            &config(&["--no-reset-env", "-t", "xterm"]),
            &TTY2,
            vtiums_own(),
        );
        assert_eq!(c.argv, argv(&["/bin/login"]));
        assert_eq!(
            c.env,
            os(&[
                ("COLORTERM", "truecolor"),
                ("HOME", "/root"),
                ("PATH", "/usr/bin"),
                ("TERM", "xterm"),
                ("TERM_SESSION_TYPE", "kms"),
                ("XDG_SEAT", "seat0"),
                ("XDG_VTNR", "2"),
            ])
        );
    }

    #[test]
    fn fbdev_and_an_unknown_seat() {
        let place = Place {
            seat: None,
            vtnr: None,
            display: Display::Fbdev,
        };
        let c = child(&config(&[]), &place, vtiums_own());
        assert_eq!(
            c.env,
            os(&[
                ("COLORTERM", "truecolor"),
                ("TERM", "kmscon"),
                ("TERM_SESSION_TYPE", "fb"),
            ])
        );
    }

    #[test]
    fn a_configured_command_with_variables() {
        let args = [
            "--no-reset-env",
            "--login",
            "--",
            "/bin/sh",
            "$TERM",
            "$HOME",
            "$NOPE",
            "$$HOME",
            "a$HOME",
        ];
        let c = child(&config(&args), &TTY2, vtiums_own());
        assert_eq!(
            c.argv,
            argv(&["/bin/sh", "kmscon", "/root", "", "$HOME", "a$HOME"])
        );
        // With the environment reset, HOME is not in it.
        let c = child(&config(&args[1..]), &TTY2, vtiums_own());
        assert_eq!(
            c.argv,
            argv(&["/bin/sh", "kmscon", "", "", "$HOME", "a$HOME"])
        );
    }

    #[test]
    fn the_command_runs_with_exactly_that_environment() {
        let c = child(&config(&[]), &TTY2, vtiums_own());
        let command = c.command().unwrap_or_else(|| Command::new("none"));
        assert_eq!(command.get_program(), OsStr::new("/bin/login"));
        assert_eq!(command.get_args().collect::<Vec<_>>(), [OsStr::new("-p")]);
        let env: BTreeMap<_, _> = command
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(OsStr::to_owned)))
            .collect();
        assert_eq!(env.len(), 5);
        assert_eq!(
            env.get(OsStr::new("TERM")),
            Some(&Some(OsString::from("kmscon")))
        );
        let none = Child {
            argv: Vec::new(),
            env: BTreeMap::new(),
        };
        assert!(none.command().is_none());
    }
}
