//! The command line (ADR-1200 D4): non-interactive, one line a result,
//! and exit 0 on success, 1 on a failure and 2 on a usage error.

use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

const USAGE: &str = "\
Usage: vtium-config [OPTION]... [COMMAND [ARG]...]

Manages the vtium console's fonts and settings, trying each font on the live console.

With no command at a terminal, opens the interactive tool.

Commands:
  set NAME                           switch the running console to a frit or a built-in font
  apply DIR                          check a saved draft and install it (as root)
  serve                              generate glyphs for the console (the console starts it)

Options:
  --configdir DIR       the directory of vtium.conf and heads.conf, in place of /etc/vtium
  --frits DIR           the frits directory, in place of the system's
  --sudo PROGRAM        the program that runs apply as root, in place of sudo
  --vtium PROGRAM       the console binary apply checks files with, in place of /usr/bin/vtium
  --kbd-dir DIR         a kbd data directory, in place of the build's list (repeatable)
  --font-dir DIR        an outline font directory, in place of the build's list (repeatable)
  --vconsole-conf FILE  vconsole.conf, in place of /etc/vconsole.conf
  --kernel-cmdline FILE the kernel command line, in place of /proc/cmdline
  --no-banner           no banner at a terminal (NO_COLOR also turns it off)
  --help, --version
";

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
            // the table's test reads every name, so none lands here
            _ => {}
        }
    }

    /// Where the TUI and `apply` read and write, as the options name them.
    fn paths(&self) -> crate::tui::Paths {
        crate::tui::Paths {
            configdir: self.configdir.clone().unwrap_or_else(default_configdir),
            frits: self
                .frits
                .clone()
                .unwrap_or_else(crate::switch::default_frits),
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
        }
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
pub const VALUE_OPTIONS: [ValueOption; 8] = [
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
];

/// Why vtium-config, run with the effective uid `euid`, refuses
/// `args`: a path or program option of [`VALUE_OPTIONS`], the step that releases the
/// TUI's signals and runs another program, or no command or an empty
/// one, which opens the TUI; `None` when it takes them.
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
    // the TUI runs as the user from start to end (ADR-1201 D5); an empty
    // command word is no command, which `run` opens the TUI for
    let tui = parse_args(args)
        .is_ok_and(|o| o.words.first().is_none_or(String::is_empty))
        .then(|| "run vtium config as yourself; it asks for sudo when it saves".to_owned());
    named.or(tui)
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
            // The banner reads it before the command line runs.
            crate::banner::OFF => {}
            "--" => {
                for rest in it.by_ref() {
                    o.words.push(text(rest)?);
                }
            }
            a if a.starts_with('-') && a != "-" && !matches!(a, "--help" | "--version") => {
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
        "--help" => cli.say(USAGE.trim_end()),
        "--version" => cli.say(concat!("vtium-config ", env!("CARGO_PKG_VERSION"))),
        "set" => crate::switch::command(&rest, o.frits.as_deref(), cli.out, cli.err),
        "apply" => crate::apply::command(&rest, &o.paths().targets(), cli.out, cli.err),
        crate::serve::SERVE_ARG => cli.serve(&rest),
        "" if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() => {
            crate::tui::run(&o.paths(), cli.err)
        }
        "" => cli.usage("no command"),
        other => cli.usage(&format!("unknown command {other}")),
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
}
