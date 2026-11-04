//! The help text: the usage `--help` prints, and the help `--help TOPIC`
//! prints for one command. Each command's text is written once, in
//! [`TOPICS`], and both read it.

use crate::banner;

/// One line of a command's usage: how it is written and what it does.
struct Form {
    synopsis: &'static str,
    does: &'static str,
}

/// A command's help.
struct Topic {
    command: Command,
    forms: &'static [Form],
    /// What the command does, its arguments and its options, in whole
    /// sentences.
    detail: &'static str,
    example: &'static str,
}

/// The commands: the one list that the dispatch in `cli.rs`, the usage and
/// `--help COMMAND` all read, so that a command cannot exist without a
/// help topic (a test checks each has one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Fonts,
    Check,
    Config,
    New,
    Tui,
    RemoveFrit,
    Preview,
    Set,
    Apply,
    Serve,
    Report,
}

impl Command {
    /// Every command, in the order the usage lists them.
    pub const ALL: [Command; 11] = [
        Command::Fonts,
        Command::Check,
        Command::Config,
        Command::New,
        Command::Tui,
        Command::RemoveFrit,
        Command::Preview,
        Command::Set,
        Command::Apply,
        Command::Serve,
        Command::Report,
    ];

    /// The word that runs the command.
    pub const fn name(self) -> &'static str {
        match self {
            Command::Fonts => "fonts",
            Command::Check => "check",
            Command::Config => "config",
            Command::New => "new",
            Command::Tui => "tui",
            Command::RemoveFrit => "remove-frit",
            Command::Preview => "preview",
            Command::Set => "set",
            Command::Apply => "apply",
            Command::Serve => crate::serve::SERVE_ARG,
            Command::Report => "report",
        }
    }

    /// The command a command-line word names.
    pub fn from_name(word: &str) -> Option<Command> {
        Command::ALL.into_iter().find(|c| c.name() == word)
    }

    fn topic(self) -> Option<&'static Topic> {
        TOPICS.iter().find(|t| t.command == self)
    }
}

const fn form(synopsis: &'static str, does: &'static str) -> Form {
    Form { synopsis, does }
}

const TOPICS: &[Topic] = &[
    Topic {
        command: Command::Fonts,
        forms: &[form(
            "fonts",
            "list the frits and built-in fonts, and the sources each lacks",
        )],
        detail: "Lists every frit (a font set file) and built-in font, one line each, with the name, whether vtium reads it, and the sources this machine lacks. It writes nothing and needs no privilege.",
        example: "vtium-config fonts",
    },
    Topic {
        command: Command::Check,
        forms: &[form(
            "check FILE",
            "check a frit (NAME.vtfrit), a head file (heads.conf) or a vtium.conf, writing nothing",
        )],
        detail: "Checks FILE as the console would read it. A file named NAME.vtfrit is read as a frit, a file named heads.conf as a head file, and any other file as a vtium.conf. Each problem prints with the file's name and line, and a good frit prints \"ok FILE\". It writes nothing and needs no privilege.",
        example: "vtium-config check /etc/vtium/vtium.conf",
    },
    Topic {
        command: Command::Config,
        forms: &[
            form(
                "config KEY [VALUE]",
                "print a vtium.conf key's value, or set it",
            ),
            form(
                "config --unset KEY",
                "leave a vtium.conf key at its default",
            ),
        ],
        detail: "With only KEY, prints the key, its value and where the value comes from (vtium.conf or the default). With VALUE, sets the key; with --unset, comments out every line that sets it. Run as a user, a change writes a draft and prints the command that installs it; run as root, it installs at once. KEY must be a key vtium.conf takes.",
        example: "vtium-config config font-size 14",
    },
    Topic {
        command: Command::New,
        forms: &[
            form(
                "new FONT... [--name NAME] [--cell WxH] [--size POINTS]",
                "make a frit of fonts, each a family or a source",
            ),
            form("new --from FILE [--name NAME]", "install the frit in FILE"),
            form("new --template", "print a frit to edit for new --from"),
            form(
                "new",
                "at a terminal, open the interactive tool at its new frit",
            ),
        ],
        detail: "Makes a frit from the fonts, in order. A FONT is a source as a frit spells it (kbd:ter-v16n, builtin:unifont) or a family name. --name names the frit; without it the name comes from the first font. --cell and --size set the glyph size. --from installs a frit file you wrote, and --template prints one to start from. The frit is checked before anything is drafted. Run as a user, it writes a draft and prints the command that installs it.",
        example: "vtium-config new unifont --name my-font --size 14",
    },
    Topic {
        command: Command::Tui,
        forms: &[form("tui", "at a terminal, open the interactive tool")],
        detail: "Opens the full-screen tool that lists the frits and the built-in fonts and tries each on the live console. It runs as you, never as root, asks for sudo when it saves, and needs a terminal on both standard input and standard output.",
        example: "vtium-config tui",
    },
    Topic {
        command: Command::RemoveFrit,
        forms: &[form(
            "remove-frit NAME",
            "remove a frit that vtium.conf does not name",
        )],
        detail: "Removes the frit NAME.vtfrit. It refuses a built-in font, a name with no frit, the frit that vtium.conf's font-name names, and a frit that a head's profile line names. Installing the removal keeps the file as NAME.vtfrit.bak.",
        example: "vtium-config remove-frit my-font",
    },
    Topic {
        command: Command::Preview,
        forms: &[form(
            "preview FONT... [--size POINTS]",
            "draw each font's specimen, as a picture where the terminal draws them",
        )],
        detail: "Draws each font's specimen and prints one line for it: the name, the size, the cell and how many specimen characters the font draws. At a terminal that draws kitty graphics the line is followed by the specimen as a picture. A FONT is a frit's name, a built-in font's, or a source as new takes it. --size sets the points, else vtium.conf's font-size, else 12.",
        example: "vtium-config preview unifont --size 16",
    },
    Topic {
        command: Command::Set,
        forms: &[form(
            "set NAME",
            "switch the running console to a frit or a built-in font",
        )],
        detail: "Switches the vtium console this terminal is attached to to the frit or built-in font NAME, for this session only. It checks NAME first, then asks the terminal what it is, and sends nothing to a terminal that is not vtium. It fails and says why when NAME is neither a frit nor a built-in font.",
        example: "vtium-config set unifont",
    },
    Topic {
        command: Command::Apply,
        forms: &[form(
            "apply DIR",
            "check a saved draft and install it (as root)",
        )],
        detail: "Checks the draft in DIR, which a command that changes something wrote, and installs it. It runs as root; the command a draft prints runs it through sudo.",
        example: "sudo vtium-config apply /home/me/.local/state/vtium/drafts/1",
    },
    Topic {
        command: Command::Serve,
        forms: &[form(
            "serve",
            "generate glyphs for the console (the console starts it)",
        )],
        detail: "Generates glyphs for the console from a frit's fonts. The console starts it, and it takes no argument; running it by hand does nothing useful.",
        example: "vtium-config serve",
    },
    Topic {
        command: Command::Report,
        forms: &[form(
            "report [--save[=PATH]] [--lines N] [--previous-boot]",
            "gather what a bug report needs into one document, changing nothing",
        )],
        detail: "Gathers what a bug report needs into one plain-text document and prints it: the vtium builds installed and the features each was built with, the kernel and the distribution, the state of each vtium console, the displays the kernel sees, vtium's configuration, and the log of each console. It changes nothing on the machine. --save writes the document to a new file in the current directory, named vtium-report-YYYYMMDD-HHMMSS.txt after the time in UTC, and --save=PATH writes it to PATH; either prints the file's path and never replaces a file that exists. --lines sets how many of each console's latest log lines it holds, 300 by default. --previous-boot reads the logs of the boot before this one, as after a defect that forced a reboot. Run as a user, each part it cannot read says so and names the command that reads it. Run as root, for the kernel's display state and the whole journal, it writes no file and takes no --save, so save what it prints with sudo vtium-config report > vtium-report.txt. It leaves out the hostname and each monitor's serial number, but it can still name user accounts, in the logs, in vtium console status and in vtium.conf, and the logs' times show your time zone, so read the report before you share it.",
        example: "vtium-config report --save",
    },
];

/// The `about` topic's line in the usage.
const ABOUT_LINE: &str = "about                              vtium-config's name, version and logo";

const HEAD: &str = "\
Usage: vtium-config [OPTION]... [COMMAND [ARG]...]

Manages the vtium console's fonts and settings, trying each font on the live console.

With no command, prints this help. To open the interactive tool, run vtium-config tui.

Commands:
";

const TAIL: &str = "
Run as a user, a command that changes something writes a draft and prints
the command that installs it; run as root, it installs at once.

Options:
  --configdir DIR       the directory of vtium.conf and heads.conf, in place of /etc/vtium
  --frits DIR           the frits directory, in place of vtium.conf's font-frits
  --sudo PROGRAM        the program that runs apply as root, in place of sudo
  --vtium PROGRAM       the console binary apply checks files with, in place of /usr/bin/vtium
  --kbd-dir DIR         a kbd data directory, in place of the build's list (repeatable)
  --font-dir DIR        an outline font directory, in place of the build's list (repeatable)
  --vconsole-conf FILE  vconsole.conf, in place of /etc/vconsole.conf
  --kernel-cmdline FILE the kernel command line, in place of /proc/cmdline
  --sys DIR             sysfs, where displays are measured, in place of /sys
  --helper PROGRAM      the glyph helper the tool starts, in place of this binary's serve
  --root DIR            the root report reads the machine under, in place of /
  --help [TOPIC], --version
";

/// Writes one form as a usage line: the synopsis in a column, then what it
/// does, on the next line when the synopsis is too long for the column.
fn line(text: &mut String, f: &Form) {
    if f.synopsis.len() <= 33 {
        text.push_str(&format!("  {:<34} {}\n", f.synopsis, f.does));
    } else {
        text.push_str(&format!("  {}\n  {:<34} {}\n", f.synopsis, "", f.does));
    }
}

/// The usage `--help` and a bare `vtium-config` print.
pub fn usage() -> String {
    let mut text = String::from(HEAD);
    for topic in Command::ALL.into_iter().filter_map(Command::topic) {
        for f in topic.forms {
            line(&mut text, f);
        }
    }
    text.push_str("\nHelp topics, for vtium-config --help TOPIC:\n  every command above, and\n  ");
    text.push_str(ABOUT_LINE);
    text.push('\n');
    text.push_str(TAIL);
    text
}

/// The names `--help` takes, for the message about an unknown one.
fn names() -> String {
    let mut names: Vec<&str> = Command::ALL.into_iter().map(Command::name).collect();
    names.push(ABOUT);
    names.join(", ")
}

/// The `about` topic's name.
pub const ABOUT: &str = "about";

/// What `--help TOPIC` prints: the command's forms, what it does and an
/// example, or the error text for an unknown topic.
pub fn topic(name: &str, out: &mut dyn std::io::Write) -> Result<(), String> {
    if name == ABOUT {
        banner::about(out);
        return Ok(());
    }
    let Some(t) = Command::from_name(name).and_then(Command::topic) else {
        return Err(format!("no help on {name}\nTopics: {}", names()));
    };
    let mut text = String::from("Usage:\n");
    for f in t.forms {
        text.push_str(&format!("  vtium-config {}\n", f.synopsis));
    }
    text.push_str(&format!("\n{}\n\nExample:\n  {}\n", t.detail, t.example));
    let _ = out.write_all(text.as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_exactly_one_topic_and_a_usage_line() {
        let usage = usage();
        for c in Command::ALL {
            assert_eq!(
                TOPICS.iter().filter(|t| t.command == c).count(),
                1,
                "{c:?} needs one help topic"
            );
            for f in c.topic().map_or(&[][..], |t| t.forms) {
                assert!(usage.contains(&format!("  {}", f.synopsis)), "{c:?}");
            }
            assert_eq!(Command::from_name(c.name()), Some(c));
        }
        assert_eq!(TOPICS.len(), Command::ALL.len());
    }
}
