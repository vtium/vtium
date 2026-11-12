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
    pub const ALL: [Command; 25] = [
        Command::Flow,
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
            "show how the draft connects displays and input devices",
        )],
        detail: "Shows how the current draft connects the connected displays and input devices: the displays, the display rules, the heads, the looks, the input rules and the input devices. Each device appears under the first rule that matches it. The first line names the draft, says how it differs from the installed vtium.conf, and counts the problems vtium check-config finds. With --json it prints one JSON object whose version field is 1. It exits 0 whenever it prints, whatever problems it lists.",
        example: "vtium-config flow --json",
    },
    Topic {
        command: Command::Diff,
        forms: &[form(
            "diff",
            "show how the draft differs from the installed vtium.conf",
        )],
        detail: "Shows each line the current draft changes from the installed /etc/vtium/vtium.conf, with its line number in the draft. It changes nothing.",
        example: "vtium-config diff",
    },
    Topic {
        command: Command::Install,
        forms: &[form(
            "install [--force]",
            "check the draft and install it as /etc/vtium/vtium.conf",
        )],
        detail: "Checks the current draft with vtium check-config, then installs it with sudo vtium-config apply DIR, which asks for your password (as root, it installs without sudo). It does not install a draft that fails the check (exit 1), or a draft made before the installed file last changed (exit 3). It exits 4 when sudo refuses. A draft whose only problem is that it has no complete path, so that no connected keyboard can type into a terminal on a connected display, is the one exception. At a terminal, install asks before it installs that draft. --force installs it without asking. Without a terminal and without --force, install refuses it (exit 1). vtium-config keeps the file it replaces, so restore can bring it back.",
        example: "vtium-config install",
    },
    Topic {
        command: Command::Get,
        forms: &[form("get KEY", "show a setting's value in the draft")],
        detail: "Shows KEY, its value at the top of the draft, and whether the value is set in the draft or is the default. It changes nothing.",
        example: "vtium-config get font-size",
    },
    Topic {
        command: Command::Set,
        forms: &[
            form("set KEY VALUE", "change a setting in the draft"),
            form("set --unset KEY", "put a setting back to its default"),
        ],
        detail: "Changes a setting at the top of the draft, where it applies to every head, and leaves the rest of the file exactly as it was. --unset comments out the line that sets the key. Like every command that changes the draft, it saves the change even when the check finds problems, and prints what the check finds. --dry-run shows the change without saving it, and --install installs the draft afterwards.",
        example: "vtium-config set font-size 14",
    },
    Topic {
        command: Command::Revert,
        forms: &[form("revert [--dry-run]", "throw away the draft's changes")],
        detail: "Replaces the current draft with a copy of the installed vtium.conf, so the draft's changes are lost. When no vtium.conf is installed, the draft becomes a short comment that says what the file is, and sets no setting. The installed file does not change. --dry-run says how many lines would be lost.",
        example: "vtium-config revert",
    },
    Topic {
        command: Command::New,
        forms: &[form(
            "new [--dry-run]",
            "start the draft over from the defaults",
        )],
        detail: "Starts the current draft over: a short comment that says what the file is, version = 1 and one empty look, [look default]. The draft sets no setting, so vtium uses every default. The draft's heads, rules and looks are lost. --dry-run says how many lines would be lost.",
        example: "vtium-config new",
    },
    Topic {
        command: Command::Rule,
        forms: &[
            form("rule add [--before N] 'KIND MATCH -> TARGET'", "add a rule"),
            form("rule set N 'KIND MATCH -> TARGET'", "replace rule N"),
            form(
                "rule set N [--selector S] [--target T]",
                "change what rule N matches or where it sends",
            ),
            form("rule move N --before M | --last", "move rule N"),
            form("rule rm N", "remove rule N"),
        ],
        detail: "Changes the [rules] section of the draft. A rule's number N is its place in [rules], counted from 1, as vtium config flow shows it. Without --before, rule add places the new rule so that a broader rule does not catch its devices first: a rule for one device goes before the first rule of its kind for a class of devices, and a rule for a class goes before the first * rule of its kind. Each takes --dry-run and --install.",
        example: "vtium-config rule add 'input path:pci-0000:15:00.0-usb-0:3.3 -> desk'",
    },
    Topic {
        command: Command::Head,
        forms: &[
            form("head add NAME [--look L]", "add a head"),
            form(
                "head set NAME [--name NEW] [--look L] [--consoles N] [--login CMD] [--when-display-lost V] [--isolated yes|no]",
                "change a head's settings",
            ),
            form("head rm NAME", "delete a head, keeping its rules"),
            form(
                "head active NAME",
                "make NAME the active head of the running vtium",
            ),
        ],
        detail: "Adds, changes or deletes a [head NAME] section in the draft. A head is a screen: one or more displays that show the same consoles. --name renames the head and every rule that names it. head rm keeps the rules that send to the head; the check reports them until you change or remove each one. head active does not work yet (vtium has no way to receive the request), and exits 2. Each change takes --dry-run and --install.",
        example: "vtium-config head set desk --look big",
    },
    Topic {
        command: Command::Look,
        forms: &[
            form(
                "look add NAME [--frit F] [--size PT] [--cell WxH]",
                "add a look",
            ),
            form(
                "look set NAME [--name NEW] [--frit F] [--size PT] [--cell WxH] [--engine E] [--hinting H] [--line-spacing PCT] [--display-depth D] [--display-dpi DPI]",
                "change a look's settings",
            ),
            form("look rm NAME [--force]", "delete a look"),
        ],
        detail: "Adds, changes or deletes a [look NAME] section in the draft. A look is a font and its size, which heads use. --frit sets its font-name, --size its font-size and --cell its character cell; --engine, --hinting and --line-spacing set font-engine, font-hinting and font-line-spacing; --display-depth and --display-dpi set the keys of the same name. look add takes the same options as look set. A setting a look leaves out comes from the top of vtium.conf, and in one look a font-size other than 0 overrides cell. look rm does not delete a look a head uses, and names those heads, unless --force switches them to the default look. Each takes --dry-run and --install.",
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
        detail: "Manages the drafts in ~/.config/vtium/drafts. A draft is your working copy of vtium.conf. A new draft starts as a copy of the installed file unless --from says otherwise. Without a name it is named after the machine, with -2, -3 and so on added so that no draft is overwritten. draft rm without a name deletes the current draft; the next change starts a new one from the installed file.",
        example: "vtium-config draft new laptop --from empty",
    },
    Topic {
        command: Command::History,
        forms: &[form(
            "history",
            "list the earlier vtium.conf files that installs replaced",
        )],
        detail: "Lists the vtium.conf files that installs replaced, newest first: the number to give restore, the file's name (which names the draft whose install replaced it), and when it was replaced, in UTC. vtium-config keeps the 10 newest; set history = N in ~/.config/vtium/vtium-config.conf to keep another number.",
        example: "vtium-config history",
    },
    Topic {
        command: Command::Restore,
        forms: &[form(
            "restore [N] [--dry-run]",
            "copy an earlier vtium.conf into the draft",
        )],
        detail: "Copies earlier file N (from history; the newest when no number is given) into the current draft. The check runs on it there, and you install it as usual.",
        example: "vtium-config restore 2",
    },
    Topic {
        command: Command::Library,
        forms: &[
            form("library list", "list the saved looks, heads and rules"),
            form(
                "library add look|head NAME",
                "save a look or head from the draft",
            ),
            form(
                "library take look|head NAME",
                "copy a saved look or head into the draft",
            ),
            form("library rm look|head|rule NAME", "delete a saved item"),
        ],
        detail: "Keeps looks, heads and rules outside any draft, in ~/.config/vtium/library.conf, written as vtium.conf sections. Adding and taking copy an item; the original stays. Rules in the library, and removing an item, do not work yet; vtium-config says so when you try.",
        example: "vtium-config library add look big",
    },
    Topic {
        command: Command::Edit,
        forms: &[form(
            "edit [--install]",
            "edit the draft in your editor, as visudo does",
        )],
        detail: "Opens a copy of the current draft in your editor ($VISUAL, else $EDITOR, else vi). When you save and quit, the copy becomes the draft and vtium check-config checks it. If the check finds errors or warnings, vtium-config prints them and asks whether to edit again. With --install, it then installs the draft, but only if the check passes.",
        example: "vtium-config edit --install",
    },
    Topic {
        command: Command::Fonts,
        forms: &[form(
            "fonts",
            "list the font sets and built-in fonts, and the font files each is missing",
        )],
        detail: "Lists every font set (frit) and built-in font, one per line: its name, whether vtium can read it, and the font files it needs that are not on this machine. It changes nothing and needs no root.",
        example: "vtium-config fonts",
    },
    Topic {
        command: Command::Check,
        forms: &[form(
            "check FILE",
            "check a font set (NAME.vtfrit) or a vtium.conf",
        )],
        detail: "Checks FILE as the console would read it. A file named NAME.vtfrit is checked as a font set, and any other file as a vtium.conf, settings and sections together. Each problem prints with the file's name and line number, and a good font set prints \"ok FILE\". It changes nothing and needs no root.",
        example: "vtium-config check /etc/vtium/vtium.conf",
    },
    Topic {
        command: Command::NewFrit,
        forms: &[
            form(
                "new-frit FONT... [--name NAME] [--cell WxH] [--size POINTS]",
                "make a font set from fonts",
            ),
            form(
                "new-frit --from FILE [--name NAME]",
                "install the font set in FILE",
            ),
            form(
                "new-frit --template",
                "print a font set to edit, for new-frit --from",
            ),
            form(
                "new-frit",
                "open the full-screen tool at its new font set page",
            ),
        ],
        detail: "Makes a font set (frit) from the fonts, in order: each later font supplies characters the earlier ones lack. A FONT is a family name, or a source as a frit file writes it (kbd:ter-v16n, builtin:unifont). --name names the font set; without it, the name comes from the first font. --cell and --size set the character size. --from installs a frit file you wrote, and --template prints one to start from. vtium-config checks the font set before it saves anything. Run as a user, it writes a draft and prints the command that installs it.",
        example: "vtium-config new-frit unifont --name my-font --size 14",
    },
    Topic {
        command: Command::Tui,
        forms: &[form("tui", "open the full-screen tool")],
        detail: "Opens the full-screen tool, where you set up the console's displays, keyboards, fonts and settings, and preview fonts live on a vtium console. It runs as you, never as root, asks for sudo when it installs, and needs a terminal on both standard input and standard output.",
        example: "vtium-config tui",
    },
    Topic {
        command: Command::RemoveFrit,
        forms: &[form(
            "remove-frit NAME",
            "remove a font set that vtium.conf does not use",
        )],
        detail: "Removes the font set NAME.vtfrit. It does not remove a built-in font, a name with no font set, or a font set that font-name names, at the top of vtium.conf or in a look. When the removal is installed, the file is kept as NAME.vtfrit.bak.",
        example: "vtium-config remove-frit my-font",
    },
    Topic {
        command: Command::Preview,
        forms: &[form(
            "preview FONT... [--size POINTS]",
            "show a preview of each font, as an image if this terminal supports images",
        )],
        detail: "Prints one line for each font: its name, size, character cell, and how many of the preview's characters it can display. In a terminal that supports kitty graphics, an image of the preview follows the line. A FONT is the name of a font set or a built-in font, or a source as new-frit takes it. --size sets the size in points; the default is vtium.conf's font-size, else 12.",
        example: "vtium-config preview unifont --size 16",
    },
    Topic {
        command: Command::UseFrit,
        forms: &[form(
            "use-frit NAME",
            "switch the running console to a font set or built-in font",
        )],
        detail: "Switches the vtium console you are typing in to the font set or built-in font NAME, until this session ends. vtium-config checks NAME first, then checks that this terminal is a vtium console, and sends nothing otherwise. It fails and says why when NAME is neither a font set nor a built-in font.",
        example: "vtium-config use-frit unifont",
    },
    Topic {
        command: Command::Apply,
        forms: &[form(
            "apply DIR",
            "check a saved draft and install it (as root)",
        )],
        detail: "Checks the draft in DIR and installs it. A command that changes something writes DIR and prints the sudo command that runs apply on it. apply runs as root.",
        example: "sudo vtium-config apply /home/me/.local/state/vtium/drafts/1",
    },
    Topic {
        command: Command::Serve,
        forms: &[form(
            "serve",
            "draw characters for the console (vtium starts it)",
        )],
        detail: "Draws a font set's characters for the console: vtium starts it as its font helper. It takes no argument; running it by hand does nothing useful.",
        example: "vtium-config serve",
    },
    Topic {
        command: Command::Report,
        forms: &[form(
            "report [--save[=PATH]] [--lines N] [--previous-boot]",
            "collect what a bug report needs into one document",
        )],
        detail: "Collects what a bug report needs into one plain-text document and prints it: the vtium builds installed and the features each was built with, the kernel and the distribution, the state of each vtium console, the displays the kernel sees, vtium's configuration, and the log of each console. It changes nothing on the machine. --save writes the document to a new file in the current directory, named vtium-report-YYYYMMDD-HHMMSS.txt (the time in UTC); --save=PATH writes it to PATH. Either prints the file's path and never replaces an existing file. --lines sets how many of each console's latest log lines to include (300 by default). --previous-boot reads the logs of the boot before this one, for example after a crash that forced a reboot. Run as a user, each part it cannot read says so and names the command that reads it. Run as root (for the kernel's display state and the whole journal), it writes no file and takes no --save, so save its output with sudo vtium-config report > vtium-report.txt. It leaves out the hostname and monitors' serial numbers, but it can still name user accounts (in the logs, in vtium console status and in vtium.conf), and the log times show your time zone. Read the report before you share it.",
        example: "vtium-config report --save",
    },
];

/// The `about` topic's line in the usage.
const ABOUT_LINE: &str = "about                              vtium-config's name, version and logo";

const HEAD: &str = "\
Usage: vtium-config [OPTION]... [COMMAND [ARG]...]

Sets up the vtium console's fonts and settings, and can preview each font live on a vtium console.

With no command, vtium-config prints this help. To open the full-screen tool, run vtium-config tui.

Commands:
";

const TAIL: &str = "
The commands from flow to edit change your draft, a copy of vtium.conf in
~/.config/vtium/drafts/NAME.conf; only install writes
/etc/vtium/vtium.conf. They exit 0 when done, 1 when the check fails at
install, 2 on a usage error, 3 when the installed file changed after the
draft was made, 4 when sudo refuses, and 5 when a file cannot be read or
written. The font set commands (new-frit, remove-frit) write a draft and
print the command that installs it.

Options:
  --configdir DIR       the directory of vtium.conf, instead of /etc/vtium
  --frits DIR           the font set directory, instead of the one font-frits names
  --sudo PROGRAM        the program that runs apply as root, instead of sudo
  --vtium PROGRAM       the vtium binary apply checks files with, instead of /usr/bin/vtium
  --kbd-dir DIR         a kbd data directory, instead of the built-in list (repeatable)
  --font-dir DIR        an outline font directory, instead of the built-in list (repeatable)
  --vconsole-conf FILE  vconsole.conf, instead of /etc/vconsole.conf
  --kernel-cmdline FILE the kernel command line, instead of /proc/cmdline
  --sys DIR             sysfs, where displays and input devices are read, instead of /sys
  --udev-data DIR       udev's database, instead of /run/udev/data
  --helper PROGRAM      the font helper to start, instead of this program's serve
  --root DIR            the root directory report reads the machine under, instead of /
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
    text.push_str("\nHelp on one command: vtium-config --help COMMAND. Also:\n  ");
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
