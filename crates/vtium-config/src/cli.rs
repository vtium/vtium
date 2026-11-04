//! The command line (ADR-1200 D4): non-interactive, one line a result,
//! and exit 0 on success, 1 on a failure and 2 on a usage error.

use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

use crate::help::Command;

/// What the command line asked for.
#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    configdir: Option<PathBuf>,
    sudo: Option<String>,
    vtium: Option<String>,
    frits: Option<PathBuf>,
    kbd_dirs: Vec<PathBuf>,
    font_dirs: Vec<PathBuf>,
    vconsole_conf: Option<PathBuf>,
    kernel_cmdline: Option<PathBuf>,
    sys: Option<PathBuf>,
    helper: Option<PathBuf>,
    root: Option<PathBuf>,
    words: Vec<String>,
}

impl Options {
    /// Takes `value` for the option `name` of [`VALUE_OPTIONS`].
    fn set(&mut self, name: &str, value: String) {
        match name {
            "--configdir" => self.configdir = Some(value.into()),
            "--sudo" => self.sudo = Some(value),
            "--vtium" => self.vtium = Some(value),
            "--frits" => self.frits = Some(value.into()),
            "--kbd-dir" => self.kbd_dirs.push(value.into()),
            "--font-dir" => self.font_dirs.push(value.into()),
            "--vconsole-conf" => self.vconsole_conf = Some(value.into()),
            "--kernel-cmdline" => self.kernel_cmdline = Some(value.into()),
            "--sys" => self.sys = Some(value.into()),
            "--helper" => self.helper = Some(value.into()),
            "--root" => self.root = Some(value.into()),
            // the table's test reads every name, so none lands here
            _ => {}
        }
    }

    /// Where the TUI and `apply` read and write, as the options name them.
    fn paths(&self) -> crate::tui::Paths {
        let configdir = self.configdir.clone().unwrap_or_else(default_configdir);
        crate::tui::Paths {
            frits: self
                .frits
                .clone()
                .map_or_else(|| crate::switch::frits_dir(&configdir), Ok),
            configdir,
            drafts: crate::tui::default_drafts(),
            sudo: self
                .sudo
                .clone()
                .unwrap_or_else(|| "sudo".to_owned())
                .into(),
            vtium: self
                .vtium
                .clone()
                .unwrap_or_else(|| crate::apply::VTIUM.to_owned())
                .into(),
            helper: self.helper(),
            sys: self.sys.clone().unwrap_or_else(|| PathBuf::from("/sys")),
        }
    }

    /// The glyph helper, this binary run as `serve`, finding sources where
    /// the options name them.
    fn helper(&self) -> crate::specimen::Helper {
        let program = self.helper.clone().unwrap_or_else(|| {
            std::env::current_exe().unwrap_or_else(|_| PathBuf::from("/usr/bin/vtium-config"))
        });
        let mut options: Vec<OsString> = Vec::new();
        for dir in &self.kbd_dirs {
            options.extend(["--kbd-dir".into(), dir.into()]);
        }
        for dir in &self.font_dirs {
            options.extend(["--font-dir".into(), dir.into()]);
        }
        if let Some(f) = &self.vconsole_conf {
            options.extend(["--vconsole-conf".into(), f.into()]);
        }
        if let Some(f) = &self.kernel_cmdline {
            options.extend(["--kernel-cmdline".into(), f.into()]);
        }
        crate::specimen::Helper { program, options }
    }
}

/// An option that takes a value, and whether that value names a path or
/// a program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueOption {
    pub name: &'static str,
    pub path: bool,
}

/// Every option that takes a value: the one table the parser reads its
/// options from and root's refusal reads the path and program options
/// from ([`root_refusal`]), so an option the parser takes cannot be missed
/// by the refusal. Run as root, vtium-config takes none of the flagged
/// ones, so `sudo vtium-config` reads and writes only the system's
/// directories and runs only the console it was built with.
pub const VALUE_OPTIONS: [ValueOption; 11] = [
    ValueOption {
        name: "--configdir",
        path: true,
    },
    ValueOption {
        name: "--frits",
        path: true,
    },
    ValueOption {
        name: "--sudo",
        path: true,
    },
    ValueOption {
        name: "--vtium",
        path: true,
    },
    ValueOption {
        name: "--kbd-dir",
        path: true,
    },
    ValueOption {
        name: "--font-dir",
        path: true,
    },
    ValueOption {
        name: "--vconsole-conf",
        path: true,
    },
    ValueOption {
        name: "--kernel-cmdline",
        path: true,
    },
    ValueOption {
        name: "--sys",
        path: true,
    },
    ValueOption {
        name: "--helper",
        path: true,
    },
    ValueOption {
        name: "--root",
        path: true,
    },
];

/// Why vtium-config, run with the effective uid `euid`, refuses
/// `args`: a path or program option of [`VALUE_OPTIONS`], the step that releases the
/// TUI's signals and runs another program, no command or an empty one,
/// which opens the TUI, or a command that runs as the user: `check`,
/// `fonts`, `preview` and `new --from`, which read files the user names or
/// start the glyph helper, which refuses root, or `report --save` in any
/// form, since root's report writes no file; `None` when it takes them.
/// It refuses only as root, an effective uid of 0; `main` runs it first.
pub fn root_refusal(euid: u32, args: &[OsString]) -> Option<String> {
    if euid != 0 {
        return None;
    }
    let named = args.iter().find_map(|a| {
        let a = a.to_string_lossy();
        let named = VALUE_OPTIONS
            .iter()
            .filter(|o| o.path)
            .map(|o| o.name)
            .find(|o| a == *o || a.strip_prefix(*o).is_some_and(|r| r.starts_with('=')));
        match named {
            Some(option) => Some(format!(
                "run as root, vtium-config takes no {option}: as root it reads and writes only /etc/vtium and its frits' directory, and checks files with the console it was built with"
            )),
            None if a == crate::tui::port::UNBLOCK_ARG => Some(format!(
                "run as root, vtium-config takes no {a}: that step runs another program and is the TUI's own, run as the user"
            )),
            None => None,
        }
    });
    // the TUI runs as the user from start to end (ADR-1201 D5); `tui` opens
    // it, and `new` alone opens it too
    let tui = parse_args(args)
        .is_ok_and(|o| match o.words.as_slice() {
            [w] => w == "tui" || w == "new",
            _ => false,
        })
        .then(|| "run vtium config as yourself; it asks for sudo when it saves".to_owned());
    // a command that reads a file the user names, or runs the glyph helper,
    // which refuses root, runs as the user: as root it would read any file
    // and could print what is in it
    let own = parse_args(args).ok().and_then(|o| {
        let user_only = match o.words.as_slice() {
            [c, ..] if matches!(c.as_str(), "check" | "fonts" | "preview") => Some(c.clone()),
            [c, rest @ ..] if c == "new" && rest.iter().any(|w| w.starts_with("--from")) => {
                Some("new --from".to_owned())
            }
            _ => None,
        };
        user_only.map(|c| {
            format!(
                "run vtium config {c} as yourself: it needs no privilege, and as root it would read files only the user should name"
            )
        })
    });
    // root's report writes no file: what it prints, the user's own shell
    // saves (ADR-1200 D24)
    let save = parse_args(args)
        .ok()
        .and_then(|o| match o.words.split_first() {
            Some((c, rest)) if c == "report" && crate::report::saves(rest) => {
                Some(crate::report::ROOT_SAVES_NOTHING.to_owned())
            }
            _ => None,
        });
    named.or(tui).or(own).or(save)
}

/// The directory of vtium.conf and the head file, as the console's.
pub fn default_configdir() -> PathBuf {
    PathBuf::from("/etc/vtium")
}

/// A usage error's message.
#[derive(Debug)]
struct Usage(String);

fn parse_args(args: &[OsString]) -> Result<Options, Usage> {
    let mut o = Options::default();
    let mut it = args.iter();
    let text = |a: &OsString| {
        a.to_str()
            .map(str::to_owned)
            .ok_or_else(|| Usage(format!("{} is not UTF-8", a.to_string_lossy())))
    };
    while let Some(arg) = it.next() {
        let arg = text(arg)?;
        let mut value = |name: &str| {
            it.next()
                .ok_or_else(|| Usage(format!("{name} takes a value")))
                .and_then(text)
        };
        if let Some(option) = VALUE_OPTIONS.iter().find(|v| v.name == arg) {
            let v = value(option.name)?;
            o.set(option.name, v);
            continue;
        }
        match arg.as_str() {
            // It once turned the banner off; it does nothing now, and scripts still pass it.
            crate::banner::OFF => {}
            "--" => {
                for rest in it.by_ref() {
                    o.words.push(text(rest)?);
                }
            }
            // after the command, an option this table does not name is the
            // command's own, as `new --name`
            a if a.starts_with('-')
                && a != "-"
                && o.words.is_empty()
                && !matches!(a, "--help" | "--version") =>
            {
                return Err(Usage(format!("unknown option {a}")));
            }
            _ => o.words.push(arg),
        }
    }
    Ok(o)
}

/// Runs the command line `args` (the program's name left out), writing to
/// `out` and `err`; returns the exit status.
pub fn run(args: &[OsString], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let o = match parse_args(args) {
        Ok(o) => o,
        Err(Usage(why)) => {
            let _ = writeln!(err, "vtium-config: {why}\nTry vtium-config --help.");
            return 2;
        }
    };
    let (command, rest) = match o.words.split_first() {
        Some((c, rest)) => (c.as_str(), rest.to_vec()),
        None => ("", Vec::new()),
    };
    let mut cli = Cli { o: &o, out, err };
    match command {
        "--help" => match rest.first() {
            None => cli.say(crate::help::usage().trim_end()),
            Some(topic) => match crate::help::topic(topic, cli.out) {
                Ok(()) => 0,
                Err(why) => {
                    let _ = writeln!(cli.err, "vtium-config: {why}");
                    2
                }
            },
        },
        "--version" => cli.say(concat!("vtium-config ", env!("CARGO_PKG_VERSION"))),
        "" => cli.say(crate::help::usage().trim_end()),
        other => match Command::from_name(other) {
            Some(c) => cli.command(c, &rest),
            None => cli.usage(&format!("unknown command {other}")),
        },
    }
}

impl Cli<'_> {
    /// Runs `command` with its arguments `rest`.
    fn command(mut self, command: Command, rest: &[String]) -> u8 {
        let o = self.o;
        match command {
            Command::Fonts => crate::commands::fonts(rest, &o.paths(), self.out, self.err),
            Command::Check => crate::commands::check(rest, &o.paths(), self.out, self.err),
            Command::Config => crate::commands::config(rest, &o.paths(), self.out, self.err),
            Command::RemoveFrit => {
                crate::commands::remove_frit(rest, &o.paths(), self.out, self.err)
            }
            Command::New if rest.is_empty() => {
                if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
                    crate::tui::run(&o.paths(), crate::tui::model::View::New, self.err)
                } else {
                    self.usage("new takes FONT..., --from FILE or --template away from a terminal")
                }
            }
            Command::New => crate::commands::new(rest, &o.paths(), self.out, self.err),
            Command::Preview => crate::preview::command(rest, &o.paths(), self.out, self.err),
            Command::Set => match o.paths().frits {
                Ok(frits) => crate::switch::command(rest, &frits, self.out, self.err),
                Err(why) => {
                    let _ = writeln!(self.err, "vtium-config: {why}");
                    1
                }
            },
            Command::Apply => crate::apply::command(rest, &o.paths().targets(), self.out, self.err),
            Command::Serve => self.serve(rest),
            Command::Report => crate::report::command(
                rest,
                o.root.as_deref(),
                rustix::process::geteuid().as_raw(),
                self.out,
                self.err,
            ),
            Command::Tui if !rest.is_empty() => self.usage("tui takes no argument"),
            Command::Tui if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() => {
                crate::tui::run(&o.paths(), crate::tui::model::View::Fonts, self.err)
            }
            Command::Tui => self.usage("tui opens the interactive tool at a terminal only"),
        }
    }
}

struct Cli<'a> {
    o: &'a Options,
    out: &'a mut dyn Write,
    err: &'a mut dyn Write,
}

impl Cli<'_> {
    fn say(self, text: &str) -> u8 {
        let _ = writeln!(self.out, "{text}");
        0
    }

    fn usage(&mut self, why: &str) -> u8 {
        let _ = writeln!(self.err, "vtium-config: {why}\nTry vtium-config --help.");
        2
    }

    /// `serve`: the helper the console starts (ADR-1301 D2), its kbd and
    /// font directories and vconsole files as the options name them.
    fn serve(&mut self, rest: &[String]) -> u8 {
        if !rest.is_empty() {
            return self.usage("serve takes no argument");
        }
        let o = self.o;
        let system = crate::serve::Paths::default();
        let paths = crate::serve::Paths {
            kbd_dirs: if o.kbd_dirs.is_empty() {
                system.kbd_dirs
            } else {
                o.kbd_dirs.clone()
            },
            font_dirs: if o.font_dirs.is_empty() {
                system.font_dirs
            } else {
                o.font_dirs.clone()
            },
            vconsole_conf: o.vconsole_conf.clone().unwrap_or(system.vconsole_conf),
            kernel_cmdline: o.kernel_cmdline.clone().unwrap_or(system.kernel_cmdline),
        };
        crate::serve::command(&paths)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails by panicking"
)]
mod tests {
    use super::*;

    /// Every option the table lists is read by the parser, so the table
    /// root's refusal is built from is the parser's own.
    #[test]
    fn every_option_in_the_table_is_read() {
        for option in VALUE_OPTIONS {
            let parsed = parse_args(&[option.name.into(), "/x".into()]);
            let o = parsed.unwrap_or_else(|Usage(why)| panic!("{}: {why}", option.name));
            assert_ne!(o, Options::default(), "{} sets nothing", option.name);
            assert!(o.words.is_empty(), "{}", option.name);
        }
        assert!(parse_args(&["--no-such".into(), "/x".into()]).is_err());
        let _ = parse_args(&[]).unwrap();
    }

    /// Root runs `report` and its parts that only root reads, but never
    /// under a stand-in root, whose programs it would run.
    #[test]
    fn root_runs_report_but_refuses_a_stand_in_root() {
        let args = |w: &[&str]| w.iter().map(OsString::from).collect::<Vec<_>>();
        assert_eq!(root_refusal(0, &args(&["report", "--lines", "9"])), None);
        for refused in [
            &["--root", "/tmp/x", "report"][..],
            &["report", "--root=/tmp/x"],
        ] {
            let why = root_refusal(0, &args(refused)).unwrap();
            assert!(why.contains("--root"), "{refused:?}: {why}");
        }
        assert_eq!(
            root_refusal(1000, &args(&["--root", "/tmp/x", "report"])),
            None
        );
    }

    /// Root's report writes no file, wherever `--save` names: a rule that
    /// lets a user run the report as root would otherwise let that user
    /// create a file anywhere root can, such as /etc/ld.so.preload.
    #[test]
    fn root_saves_no_report() {
        let args = |w: &[&str]| w.iter().map(OsString::from).collect::<Vec<_>>();
        for refused in [
            &["report", "--save"][..],
            &["report", "--save=/etc/ld.so.preload"],
            &["report", "--lines", "5", "--save=x.txt"],
        ] {
            let why =
                root_refusal(0, &args(refused)).unwrap_or_else(|| panic!("root takes {refused:?}"));
            assert!(
                why.contains("sudo vtium-config report > vtium-report.txt"),
                "{refused:?}: {why}"
            );
        }
        assert_eq!(root_refusal(1000, &args(&["report", "--save=x.txt"])), None);
        assert_eq!(root_refusal(0, &args(&["report"])), None);
    }
}
