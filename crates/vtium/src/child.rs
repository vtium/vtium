//! The session's command and its environment, which [`vtium_platform::Pty`]
//! runs on the pty.
//!
//! The command is `login`, or `/bin/login -p` with `reset-env` on and
//! `/bin/login` with it off. With `reset-env` on the environment starts
//! empty, and off it starts as vtium's own. Over it vtium sets `TERM`,
//! `TERM_PROGRAM=vtium` and `TERM_PROGRAM_VERSION`, vtium's version, which
//! name the terminal whatever `TERM` names, `COLORTERM=truecolor`,
//! `XDG_SEAT` and `XDG_VTNR` when it knows them, except on a seat that
//! libseat's logind backend opened,
//! `TERM_SESSION_TYPE`: `kms` on a DRM display, or `fb` on fbdev, and
//! `VTIUM_HEAD` and `VTIUM_CONSOLE`, the session's head and the console's
//! id (ADR-1401 D5), when it runs in a head's console. Then each word
//! of the command that is `$NAME` becomes that variable of the
//! environment, empty when it is unset, and a word starting `$$` loses one
//! `$`.
//!
//! On a seat that libseat's logind backend opened, vtium holds the seat
//! through its own logind session, and each session it starts runs in a
//! systemd scope of its own, where `pam_systemd` gives the login a logind
//! session of the user's own (ADR-1400 D3, its note on each login's own
//! session). That session must have no seat: one on seat0 with vtium's VT
//! would become the seat's active session, and logind would then pause
//! vtium's devices. So there vtium sets neither `XDG_SEAT` nor `XDG_VTNR`,
//! and removes them, and every `XDG_SESSION_*` variable of vtium's own
//! session, from the environment it inherited.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
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
    /// The head's name and the console's id, which [`crate::head`] never
    /// gives another console of the head.
    pub console: Option<(&'a str, u64)>,
    /// The seat came through libseat's logind backend: the session gets no
    /// seat, and none of vtium's own session's variables.
    pub logind_seat: bool,
}

/// Whether `name` is a variable of vtium's own logind session, which a
/// session on a seat that libseat's logind backend opened never inherits.
fn of_vtiums_session(name: &OsStr) -> bool {
    let name = name.as_bytes();
    name == b"XDG_SEAT" || name == b"XDG_VTNR" || name.starts_with(b"XDG_SESSION_")
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
        inherited
            .into_iter()
            .filter(|(k, _)| !(place.logind_seat && of_vtiums_session(k)))
            .collect()
    };
    let mut set = |k: &str, v: &str| env.insert(k.into(), v.into());
    set("TERM", config.text(&TERM).unwrap_or_default());
    set("TERM_PROGRAM", "vtium");
    set("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
    set("COLORTERM", "truecolor");
    if !place.logind_seat {
        if let Some(seat) = place.seat {
            set("XDG_SEAT", seat);
        }
        if let Some(vtnr) = place.vtnr {
            set("XDG_VTNR", &vtnr.to_string());
        }
    }
    let session_type = match place.display {
        Display::Kms => "kms",
        Display::Fbdev => "fb",
    };
    set("TERM_SESSION_TYPE", session_type);
    if let Some((head, id)) = place.console {
        set("VTIUM_HEAD", head);
        set("VTIUM_CONSOLE", &id.to_string());
    }

    let words: Vec<OsString> = match config.command(&LOGIN) {
        Some(words) => words.to_vec(),
        None if reset => vec!["/bin/login".into(), "-p".into()],
        None => vec!["/bin/login".into()],
    };
    let argv = words
        .iter()
        .map(|w| match w.as_bytes().strip_prefix(b"$") {
            Some(escaped) if escaped.starts_with(b"$") => OsString::from_vec(escaped.to_vec()),
            Some(name) => env
                .get(OsStr::from_bytes(name))
                .cloned()
                .unwrap_or_default(),
            None => w.clone(),
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

    /// vtium's version, which `TERM_PROGRAM_VERSION` gives.
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    use crate::config::load;

    fn config(args: &[&str]) -> Config {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        load(&args, &mut |_: &Path| Ok(None)).config
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
        console: None,
        logind_seat: false,
    };

    fn vtiums_own() -> Vec<(OsString, OsString)> {
        // started from another terminal, which named itself
        os(&[
            ("HOME", "/root"),
            ("TERM", "linux"),
            ("TERM_PROGRAM", "kitty"),
            ("PATH", "/usr/bin"),
        ])
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
                ("TERM", "xterm-vtium"),
                ("TERM_PROGRAM", "vtium"),
                ("TERM_PROGRAM_VERSION", VERSION),
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
                ("TERM_PROGRAM", "vtium"),
                ("TERM_PROGRAM_VERSION", VERSION),
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
            console: None,
            logind_seat: false,
        };
        let c = child(&config(&[]), &place, vtiums_own());
        assert_eq!(
            c.env,
            os(&[
                ("COLORTERM", "truecolor"),
                ("TERM", "xterm-vtium"),
                ("TERM_PROGRAM", "vtium"),
                ("TERM_PROGRAM_VERSION", VERSION),
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
            argv(&["/bin/sh", "xterm-vtium", "/root", "", "$HOME", "a$HOME"])
        );
        // With the environment reset, HOME is not in it.
        let c = child(&config(&args[1..]), &TTY2, vtiums_own());
        assert_eq!(
            c.argv,
            argv(&["/bin/sh", "xterm-vtium", "", "", "$HOME", "a$HOME"])
        );
    }

    // ADR-1401 D5: a head's console is told its head and its own id, which
    // a command's `$NAME` words read too.
    #[test]
    fn a_heads_console_is_told_its_head_and_its_id() {
        let place = Place {
            console: Some(("desk", 3)),
            ..TTY2
        };
        let args = ["--login", "--", "/bin/sh", "$VTIUM_HEAD", "$VTIUM_CONSOLE"];
        let c = child(&config(&args), &place, vtiums_own());
        assert_eq!(c.env.get(OsStr::new("VTIUM_HEAD")), Some(&"desk".into()));
        assert_eq!(c.env.get(OsStr::new("VTIUM_CONSOLE")), Some(&"3".into()));
        assert_eq!(c.argv, argv(&["/bin/sh", "desk", "3"]));
    }

    // ADR-1401 D5: the default command keeps the head's variables for the
    // shell only with the environment reset, where it is `login -p`; with
    // `reset-env` off it is `login`, which clears the environment it gets.
    #[test]
    fn the_default_login_keeps_the_heads_variables_with_the_environment_reset() {
        let place = Place {
            console: Some(("desk", 2)),
            ..TTY2
        };
        for (args, words) in [
            (&[][..], &["/bin/login", "-p"][..]),
            (&["--no-reset-env"][..], &["/bin/login"][..]),
        ] {
            let c = child(&config(args), &place, vtiums_own());
            assert_eq!(c.argv, argv(words), "{args:?}");
            assert_eq!(c.env.get(OsStr::new("VTIUM_HEAD")), Some(&"desk".into()));
            assert_eq!(c.env.get(OsStr::new("VTIUM_CONSOLE")), Some(&"2".into()));
        }
    }

    /// vtium's environment under its own unit on the libseat build: its
    /// PAM session's variables, which systemd sets for the unit.
    fn vtiums_own_in_its_session() -> Vec<(OsString, OsString)> {
        let mut env = vtiums_own();
        env.extend(os(&[
            ("XDG_SEAT", "seat0"),
            ("XDG_VTNR", "2"),
            ("XDG_SESSION_ID", "c1"),
            ("XDG_SESSION_CLASS", "greeter"),
            ("XDG_SESSION_TYPE", "tty"),
            ("XDG_RUNTIME_DIR", "/run/user/0"),
        ]));
        env
    }

    const LOGIND_TTY2: Place<'static> = Place {
        logind_seat: true,
        ..TTY2
    };

    /// The names of `env`'s variables that would tie a session to vtium's
    /// own logind session or to its seat.
    fn vtiums_session_names(env: &BTreeMap<OsString, OsString>) -> Vec<String> {
        env.keys()
            .map(|k| k.to_string_lossy().into_owned())
            .filter(|k| k == "XDG_SEAT" || k == "XDG_VTNR" || k.starts_with("XDG_SESSION_"))
            .collect()
    }

    // ADR-1400 D3, its note on each login's own session: on a seat that
    // libseat's logind backend opened, no kind of session vtium starts gets
    // a seat, a VT number or vtium's own session's variables: the default
    // login, login with the environment kept, and a `--login` shell.
    #[test]
    fn on_the_logind_seat_no_session_gets_vtiums_seat_or_session() {
        for args in [
            &[][..],
            &["--no-reset-env"][..],
            &["--no-reset-env", "--login", "--", "/bin/bash", "-l"][..],
            &["--login", "--", "/bin/bash", "-l"][..],
        ] {
            let c = child(&config(args), &LOGIND_TTY2, vtiums_own_in_its_session());
            assert_eq!(
                vtiums_session_names(&c.env),
                Vec::<String>::new(),
                "{args:?}"
            );
            assert_eq!(
                c.env.get(OsStr::new("TERM_PROGRAM")),
                Some(&"vtium".into()),
                "{args:?}"
            );
        }
        // what is not the session's stays, XDG_RUNTIME_DIR among it, which
        // pam_systemd sets afresh for the user
        let c = child(
            &config(&["--no-reset-env"]),
            &LOGIND_TTY2,
            vtiums_own_in_its_session(),
        );
        assert_eq!(c.env.get(OsStr::new("HOME")), Some(&"/root".into()));
        assert_eq!(
            c.env.get(OsStr::new("XDG_RUNTIME_DIR")),
            Some(&"/run/user/0".into())
        );
    }

    // Everywhere else, the session keeps what it had: the seat and VT
    // vtium sets, and with the environment kept, all that vtium inherited.
    #[test]
    fn off_the_logind_seat_sessions_keep_the_seat_and_what_vtium_inherited() {
        let c = child(&config(&[]), &TTY2, vtiums_own_in_its_session());
        assert_eq!(vtiums_session_names(&c.env), ["XDG_SEAT", "XDG_VTNR"]);
        let c = child(
            &config(&["--no-reset-env"]),
            &TTY2,
            vtiums_own_in_its_session(),
        );
        assert_eq!(
            vtiums_session_names(&c.env),
            [
                "XDG_SEAT",
                "XDG_SESSION_CLASS",
                "XDG_SESSION_ID",
                "XDG_SESSION_TYPE",
                "XDG_VTNR"
            ]
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
        assert_eq!(env.len(), 7);
        assert_eq!(
            env.get(OsStr::new("TERM")),
            Some(&Some(OsString::from("xterm-vtium")))
        );
        let none = Child {
            argv: Vec::new(),
            env: BTreeMap::new(),
        };
        assert!(none.command().is_none());
    }
}
