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
  --kbd-dir DIR         a kbd data directory, in place of the build's list (repeatable)
  --font-dir DIR        an outline font directory, in place of the build's list (repeatable)
  --vconsole-conf FILE  vconsole.conf, in place of /etc/vconsole.conf
  --kernel-cmdline FILE the kernel command line, in place of /proc/cmdline
  --no-banner           no banner at a terminal (NO_COLOR also turns it off)
  --help, --version
";

/// What the command line asked for.
#[derive(Debug, Default)]
struct Options {
    configdir: Option<PathBuf>,
    sudo: Option<String>,
    frits: Option<PathBuf>,
    kbd_dirs: Vec<PathBuf>,
    font_dirs: Vec<PathBuf>,
    vconsole_conf: Option<PathBuf>,
    kernel_cmdline: Option<PathBuf>,
    words: Vec<String>,
}

impl Options {
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
        }
    }
}

/// The directory of vtium.conf and the head file, as the console's.
pub fn default_configdir() -> PathBuf {
    PathBuf::from("/etc/vtium")
}

/// A usage error's message.
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
        match arg.as_str() {
            "--configdir" => o.configdir = Some(value("--configdir")?.into()),
            "--sudo" => o.sudo = Some(value("--sudo")?),
            "--frits" => o.frits = Some(value("--frits")?.into()),
            "--kbd-dir" => o.kbd_dirs.push(value("--kbd-dir")?.into()),
            "--font-dir" => o.font_dirs.push(value("--font-dir")?.into()),
            "--vconsole-conf" => o.vconsole_conf = Some(value("--vconsole-conf")?.into()),
            "--kernel-cmdline" => o.kernel_cmdline = Some(value("--kernel-cmdline")?.into()),
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
