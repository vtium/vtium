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
    Flow,
    Route,
    Diff,
    Install,
    Get,
    Set,
    Revert,
    New,
    Rule,
    Head,
    Look,
    Draft,
    History,
    Restore,
    Library,
    Edit,
    Fonts,
    Check,
    NewFrit,
    Tui,
    RemoveFrit,
    Preview,
    UseFrit,
    Apply,
    Serve,
    Report,
}

impl Command {
    /// Every command, in the order the usage lists them.
    pub const ALL: [Command; 26] = [
        Command::Flow,
        Command::Route,
        Command::Diff,
        Command::Install,
        Command::Get,
        Command::Set,
        Command::Revert,
        Command::New,
        Command::Rule,
        Command::Head,
        Command::Look,
        Command::Draft,
        Command::History,
        Command::Restore,
        Command::Library,
        Command::Edit,
        Command::Fonts,
        Command::Check,
        Command::NewFrit,
        Command::Tui,
        Command::RemoveFrit,
        Command::Preview,
        Command::UseFrit,
        Command::Apply,
        Command::Serve,
        Command::Report,
    ];

    /// The word that runs the command.
    pub const fn name(self) -> &'static str {
        match self {
            Command::Flow => "flow",
            Command::Route => "route",
            Command::Diff => "diff",
            Command::Install => "install",
            Command::Get => "get",
            Command::Set => "set",
            Command::Revert => "revert",
            Command::New => "new",
            Command::Rule => "rule",
            Command::Head => "head",
            Command::Look => "look",
            Command::Draft => "draft",
            Command::History => "history",
            Command::Restore => "restore",
            Command::Library => "library",
            Command::Edit => "edit",
            Command::Fonts => "fonts",
            Command::Check => "check",
            Command::NewFrit => "new-frit",
            Command::Tui => "tui",
            Command::RemoveFrit => "remove-frit",
            Command::Preview => "preview",
            Command::UseFrit => "use-frit",
            Command::Apply => "apply",
            Command::Serve => crate::serve::SERVE_ARG,
            Command::Report => "report",
        }
    }

    /// Whether the command is one of the flow's (ADR-1403 D8), which work
    /// on the draft.
    pub const fn flow(self) -> bool {
        matches!(
            self,
            Command::Flow
                | Command::Route
                | Command::Diff
                | Command::Install
                | Command::Get
                | Command::Set
                | Command::Revert
                | Command::New
                | Command::Rule
                | Command::Head
                | Command::Look
                | Command::Draft
                | Command::History
                | Command::Restore
                | Command::Library
                | Command::Edit
        )
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
        command: Command::Flow,
        forms: &[form(
            "flow [--json]",
            "print the draft's flow, with the devices present placed on it",
        )],
        detail: "Prints the flow of the current draft: the displays, the display rules, the heads, the looks, the input rules and the input devices, each device placed by the first rule that matches it. The first line names the draft and says how it stands against the installed file and how many problems vtium check-config finds. With --json it prints one JSON object whose version field is 1. It exits 0 whenever it prints, whatever problems it lists.",
        example: "vtium-config flow --json",
    },
    Topic {
        command: Command::Route,
        forms: &[form(
            "route ITEM [--json]",
            "print the complete routes an item is part of",
        )],
        detail: "Prints the route card of ITEM, which is display NAME, rule N, head NAME, look NAME or input NAME: each complete route from a display to a keyboard that the item is part of, with the item marked, or why none passes through it. With --json it prints one JSON object whose version field is 1.",
        example: "vtium-config route rule 5",
    },
    Topic {
        command: Command::Diff,
        forms: &[form(
            "diff",
            "print the difference between the installed file and the draft",
        )],
        detail: "Prints each line the current draft changes from the installed /etc/vtium/vtium.conf, under the number of its line in the draft. It changes nothing.",
        example: "vtium-config diff",
    },
    Topic {
        command: Command::Install,
        forms: &[form(
            "install",
            "check the draft and install it as /etc/vtium/vtium.conf",
        )],
        detail: "Checks the current draft with vtium check-config and installs it through sudo vtium-config apply DIR, which asks for your password; run as root it installs without sudo. It refuses a draft the check fails (exit 1) and a draft made before the installed file changed (exit 3), and exits 4 when sudo refuses. The file it replaces is kept in the installed history.",
        example: "vtium-config install",
    },
    Topic {
        command: Command::Get,
        forms: &[form(
            "get KEY",
            "print a top-level key's value in the draft",
        )],
        detail: "Prints KEY, its value in the draft's top level and where the value comes from, draft or default. It changes nothing.",
        example: "vtium-config get font-size",
    },
    Topic {
        command: Command::Set,
        forms: &[
            form("set KEY VALUE", "set a top-level key in the draft"),
            form("set --unset KEY", "leave a top-level key at its default"),
        ],
        detail: "Sets a key above the draft's first section, the defaults every head inherits, through the line editor, which keeps every other byte; --unset comments out the line vtium takes the key from. Like every command that changes the draft, it writes the draft whatever the check says and prints what the check finds. --dry-run prints the change and writes nothing, and --install installs the draft afterwards.",
        example: "vtium-config set font-size 14",
    },
    Topic {
        command: Command::Revert,
        forms: &[form("revert [--dry-run]", "throw away the draft's changes")],
        detail: "Copies the installed file back over the current draft, so the draft's changes are lost; the installed file is untouched. --dry-run says how many lines would be lost.",
        example: "vtium-config revert",
    },
    Topic {
        command: Command::New,
        forms: &[form(
            "new [--dry-run]",
            "empty the draft to the defaults and one look",
        )],
        detail: "Empties the current draft to the top level at its defaults, every key commented out as in the shipped example, and one look, [look default], with no keys. The draft's heads, rules and looks are lost. --dry-run says how many lines would be lost.",
        example: "vtium-config new",
    },
    Topic {
        command: Command::Rule,
        forms: &[
            form(
                "rule add [--before N] 'KIND SELECTOR -> TARGET'",
                "add a rule",
            ),
            form("rule set N 'KIND SELECTOR -> TARGET'", "replace rule N"),
            form(
                "rule set N [--selector S] [--target T]",
                "change rule N's selector or target",
            ),
            form("rule move N --before M | --last", "move rule N"),
            form("rule rm N", "remove rule N"),
        ],
        detail: "Changes [rules] in the draft. A rule's N is its place in [rules], counted from 1, as vtium config flow prints it. Without --before, rule add puts a rule that names one device before the first rule of its kind that names a kind of device, and a rule that names a kind of device before the first * rule of its kind, so a wider rule does not match its devices first. Each takes --dry-run and --install.",
        example: "vtium-config rule add 'input path:pci-0000:15:00.0-usb-0:3.3 -> desk'",
    },
    Topic {
        command: Command::Head,
        forms: &[
            form("head add NAME [--look L]", "add a head"),
            form(
                "head set NAME [--name NEW] [--look L] [--consoles N] [--login CMD] [--when-display-lost V] [--isolated yes|no]",
                "change a head's keys",
            ),
            form("head rm NAME", "delete a head, keeping its rules"),
            form(
                "head active NAME",
                "ask the running vtium to make NAME the active head",
            ),
        ],
        detail: "Adds, changes or deletes a [head NAME] section in the draft. --name renames the head and every rule that names it. head rm keeps the rules that send to the head, and the check lists them until each is pointed elsewhere or removed. head active needs the control socket the running vtium does not have yet, and says so with exit 2. Each change takes --dry-run and --install.",
        example: "vtium-config head set desk --look big",
    },
    Topic {
        command: Command::Look,
        forms: &[
            form("look add NAME [--frit F] [--size PT]", "add a look"),
            form(
                "look set NAME [--name NEW] [--frit F] [--size PT] [--engine E] [--hinting H] [--line-spacing PCT]",
                "change a look's keys",
            ),
            form("look rm NAME [--force]", "delete a look"),
        ],
        detail: "Adds, changes or deletes a [look NAME] section in the draft. --frit sets its font-name and --size its font-size; a key a look leaves out is inherited from the top level. look rm refuses a look a head uses, naming the heads, unless --force sets those heads to the default look. Each takes --dry-run and --install.",
        example: "vtium-config look add big --frit monoid --size 14",
    },
    Topic {
        command: Command::Draft,
        forms: &[
            form("draft list", "list the drafts, marking the current one"),
            form(
                "draft new [NAME] [--from installed|empty|DRAFT]",
                "make a draft and make it current",
            ),
            form("draft switch NAME", "make another draft current"),
            form("draft rm [NAME]", "delete a draft"),
        ],
        detail: "Manages the drafts under ~/.config/vtium/drafts. A new draft is made from the installed file unless --from says otherwise, and without a name takes the machine's hostname, with -2, -3 and so on appended so that no draft is overwritten. draft rm deletes the current draft when no name is given; the next change starts a new one from the installed file.",
        example: "vtium-config draft new laptop --from empty",
    },
    Topic {
        command: Command::History,
        forms: &[form(
            "history",
            "list the installed files that installs replaced",
        )],
        detail: "Lists the installed history, newest first: the number restore takes, the file's name, which names the draft whose install replaced it, and when it was replaced, in UTC. vtium-config keeps the 10 newest, a count history = N in ~/.config/vtium/vtium-config.conf changes.",
        example: "vtium-config history",
    },
    Topic {
        command: Command::Restore,
        forms: &[form(
            "restore [N] [--dry-run]",
            "copy an installed-history file into the draft",
        )],
        detail: "Copies file N of the installed history, the newest when no number is given, into the current draft, where the check runs and you install it as usual.",
        example: "vtium-config restore 2",
    },
    Topic {
        command: Command::Library,
        forms: &[
            form("library list", "list the library's looks, heads and rules"),
            form(
                "library add look|head NAME",
                "copy a node of the draft into the library",
            ),
            form(
                "library take look|head NAME",
                "copy a node of the library into the draft",
            ),
            form(
                "library rm look|head|rule NAME",
                "delete a node from the library",
            ),
        ],
        detail: "Keeps looks, heads and rules outside any draft, in ~/.config/vtium/library.conf, written as vtium.conf's sections. A node is copied when it moves, either way. The library's rules and removing a node wait on the section writer, and say so.",
        example: "vtium-config library add look big",
    },
    Topic {
        command: Command::Edit,
        forms: &[form(
            "edit [--install]",
            "edit the draft in your editor, as visudo does",
        )],
        detail: "Copies the current draft to a file and opens it in the editor $VISUAL names, else $EDITOR, else vi. When the editor exits with the file changed, the copy becomes the draft and vtium check-config checks it; when the check finds an error or a warning, it prints each and asks whether to edit again. With --install it then installs the draft, only when the check passes.",
        example: "vtium-config edit --install",
    },
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
            "check a frit (NAME.vtfrit) or a vtium.conf, writing nothing",
        )],
        detail: "Checks FILE as the console would read it. A file named NAME.vtfrit is read as a frit, and any other file as a vtium.conf, its keys and its sections in one run. Each problem prints with the file's name and line, and a good frit prints \"ok FILE\". It writes nothing and needs no privilege.",
        example: "vtium-config check /etc/vtium/vtium.conf",
    },
    Topic {
        command: Command::NewFrit,
        forms: &[
            form(
                "new-frit FONT... [--name NAME] [--cell WxH] [--size POINTS]",
                "make a frit of fonts, each a family or a source",
            ),
            form(
                "new-frit --from FILE [--name NAME]",
                "install the frit in FILE",
            ),
            form(
                "new-frit --template",
                "print a frit to edit for new-frit --from",
            ),
            form(
                "new-frit",
                "at a terminal, open the interactive tool at its new frit",
            ),
        ],
        detail: "Makes a frit from the fonts, in order. A FONT is a source as a frit spells it (kbd:ter-v16n, builtin:unifont) or a family name. --name names the frit; without it the name comes from the first font. --cell and --size set the glyph size. --from installs a frit file you wrote, and --template prints one to start from. The frit is checked before anything is drafted. Run as a user, it writes a draft and prints the command that installs it.",
        example: "vtium-config new-frit unifont --name my-font --size 14",
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
        detail: "Removes the frit NAME.vtfrit. It refuses a built-in font, a name with no frit, the frit that vtium.conf's font-name names, and a frit that a look's font-name names. Installing the removal keeps the file as NAME.vtfrit.bak.",
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
        command: Command::UseFrit,
        forms: &[form(
            "use-frit NAME",
            "switch the running console to a frit or a built-in font",
        )],
        detail: "Switches the vtium console this terminal is attached to to the frit or built-in font NAME, for this session only. It checks NAME first, then asks the terminal what it is, and sends nothing to a terminal that is not vtium. It fails and says why when NAME is neither a frit nor a built-in font.",
        example: "vtium-config use-frit unifont",
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
The flow's commands change the draft, ~/.config/vtium/drafts/NAME.conf,
and only install writes /etc/vtium/vtium.conf. They exit 0 when done, 1
when the check fails at an install, 2 on a usage error, 3 when the
installed file changed since the draft was made, 4 when sudo refuses, and 5
when a file cannot be read or written. The frit commands write an install
bundle and print the command that installs it.

Options:
  --configdir DIR       the directory of vtium.conf, in place of /etc/vtium
  --frits DIR           the frits directory, in place of the one font-frits names
  --sudo PROGRAM        the program that runs apply as root, in place of sudo
  --vtium PROGRAM       the console binary apply checks files with, in place of /usr/bin/vtium
  --kbd-dir DIR         a kbd data directory, in place of the build's list (repeatable)
  --font-dir DIR        an outline font directory, in place of the build's list (repeatable)
  --vconsole-conf FILE  vconsole.conf, in place of /etc/vconsole.conf
  --kernel-cmdline FILE the kernel command line, in place of /proc/cmdline
  --sys DIR             sysfs, where displays and inputs are read, in place of /sys
  --udev-data DIR       udev's database, in place of /run/udev/data
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
