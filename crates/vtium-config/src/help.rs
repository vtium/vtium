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
    name: &'static str,
    forms: &'static [Form],
    /// What the command does, its arguments and its options, in whole
    /// sentences.
    detail: &'static str,
    example: &'static str,
}

const fn form(synopsis: &'static str, does: &'static str) -> Form {
    Form { synopsis, does }
}

const TOPICS: &[Topic] = &[
    Topic {
        name: "fonts",
        forms: &[form(
            "fonts",
            "list the frits and built-in fonts, and the sources each lacks",
        )],
        detail: "Lists every frit (a font set file) and built-in font, one line each, with the name, whether vtium reads it, and the sources this machine lacks. It writes nothing and needs no privilege.",
        example: "vtium-config fonts",
    },
    Topic {
        name: "check",
        forms: &[form(
            "check FILE",
            "check a frit (NAME.vtfrit), a head file (heads.conf) or a vtium.conf, writing nothing",
        )],
        detail: "Checks FILE as the console would read it. A file named NAME.vtfrit is read as a frit, a file named heads.conf as a head file, and any other file as a vtium.conf. Each problem prints with the file's name and line, and a good frit prints \"ok FILE\". It writes nothing and needs no privilege.",
        example: "vtium-config check /etc/vtium/vtium.conf",
    },
    Topic {
        name: "config",
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
        name: "new",
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
        name: "tui",
        forms: &[form("tui", "at a terminal, open the interactive tool")],
        detail: "Opens the full-screen tool that lists the frits and the built-in fonts and tries each on the live console. It runs as you, never as root, asks for sudo when it saves, and needs a terminal on both standard input and standard output.",
        example: "vtium-config tui",
    },
    Topic {
        name: "remove-frit",
        forms: &[form(
            "remove-frit NAME",
            "remove a frit that vtium.conf does not name",
        )],
        detail: "Removes the frit NAME.vtfrit. It refuses a built-in font, a name with no frit, the frit that vtium.conf's font-name names, and a frit that a head's profile line names. Installing the removal keeps the file as NAME.vtfrit.bak.",
        example: "vtium-config remove-frit my-font",
    },
    Topic {
        name: "preview",
        forms: &[form(
            "preview FONT... [--size POINTS]",
            "draw each font's specimen, as a picture where the terminal draws them",
        )],
        detail: "Draws each font's specimen and prints one line for it: the name, the size, the cell and how many specimen characters the font draws. At a terminal that draws kitty graphics the line is followed by the specimen as a picture. A FONT is a frit's name, a built-in font's, or a source as new takes it. --size sets the points, else vtium.conf's font-size, else 12.",
        example: "vtium-config preview unifont --size 16",
    },
    Topic {
        name: "set",
        forms: &[form(
            "set NAME",
            "switch the running console to a frit or a built-in font",
        )],
        detail: "Switches the vtium console this terminal is attached to to the frit or built-in font NAME, for this session only. It checks NAME first, then asks the terminal what it is, and sends nothing to a terminal that is not vtium. It fails and says why when NAME is neither a frit nor a built-in font.",
        example: "vtium-config set unifont",
    },
    Topic {
        name: "apply",
        forms: &[form(
            "apply DIR",
            "check a saved draft and install it (as root)",
        )],
        detail: "Checks the draft in DIR, which a command that changes something wrote, and installs it. It runs as root; the command a draft prints runs it through sudo.",
        example: "sudo vtium-config apply /home/me/.local/state/vtium/drafts/1",
    },
    Topic {
        name: "serve",
        forms: &[form(
            "serve",
            "generate glyphs for the console (the console starts it)",
        )],
        detail: "Generates glyphs for the console from a frit's fonts. The console starts it, and it takes no argument; running it by hand does nothing useful.",
        example: "vtium-config serve",
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
    for topic in TOPICS {
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
    let mut names: Vec<&str> = TOPICS.iter().map(|t| t.name).collect();
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
    let Some(t) = TOPICS.iter().find(|t| t.name == name) else {
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
    fn every_command_the_usage_lists_has_a_topic() {
        let usage = usage();
        for t in TOPICS {
            for f in t.forms {
                assert!(usage.contains(&format!("  {}", f.synopsis)), "{}", t.name);
            }
        }
    }
}
