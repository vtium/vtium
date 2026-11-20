//! The help text: the usage `--help` prints, and the help `--help TOPIC`
//! prints for one command. Each command's text is written once, in
//! `TOPICS`, and both read it. The options come from the parser's own
//! table, [`crate::cli::VALUE_OPTIONS`], and the exit statuses from
//! [`EXIT_STATUS`]. vtium-config(1), the man page that tools/man
//! generates, reads the same lists.

use crate::banner;

/// One line of a command's usage: how it is written and what it does.
pub struct Form {
    pub synopsis: &'static str,
    pub does: &'static str,
}

/// A command's help.
pub struct Topic {
    pub command: Command,
    pub forms: &'static [Form],
    /// What the command does, its arguments and its options, in whole
    /// sentences.
    pub detail: &'static str,
    pub example: &'static str,
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
    Activate,
    Deactivate,
    Active,
    Preview,
    UseFrit,
    Apply,
    Serve,
    Report,
}

impl Command {
    /// Every command, in the order the usage lists them.
    pub const ALL: [Command; 28] = [
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
        Command::Activate,
        Command::Deactivate,
        Command::Active,
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
            Command::Activate => "activate",
            Command::Deactivate => "deactivate",
            Command::Active => "active",
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

    /// Whether the command is one of the flow's commands, which work on
    /// the draft. `activate`, `deactivate` and `active` work on the draft
    /// too, through [`crate::batch`].
    ///
    /// References:
    /// - ADR-1403 D8: the flow's commands, and the draft they work on.
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
        detail: "Shows each line the current draft changes from the installed /etc/vtium/vtium.conf, with its line number in the draft. When the draft changes the active resources, diff then shows each line that installing the draft changes in /etc/vtium/batch.conf. It changes nothing.",
        example: "vtium-config diff",
    },
    Topic {
        command: Command::Install,
        forms: &[form(
            "install [--force]",
            "check the draft and install it as /etc/vtium/vtium.conf",
        )],
        detail: "Checks the current draft with vtium check-config, then installs it with sudo vtium-config apply DIR, which asks for your password (as root, it installs without sudo). It does not install a draft that fails the check (exit 1), or a draft made before the installed file last changed (exit 3). When the check exits 6, because its only warnings are about fonts that a font set needs and this machine lacks, install shows the warnings and installs the draft anyway. It exits 4 when sudo refuses. A draft whose only problem is that it has no complete path, so that no connected keyboard can type into a terminal on a connected display, is the one exception. At a terminal, install asks before it installs that draft. --force installs it without asking. Without a terminal and without --force, install refuses it (exit 1). vtium-config keeps the file it replaces, so restore can bring it back. vtium reads vtium.conf only when it starts on a VT. So after it installs /etc/vtium/vtium.conf, install enables vtium with sudo vtium console enable on each VT a head names where vtium is not enabled, and shows what vtium console says, refusals included. At a terminal, it then asks whether to start vtium on those VTs and restart it where it runs, so the new file applies now. Each session keeps running through a restart, but its screen is cleared. Without a terminal, install prints those commands instead.",
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
                "head set NAME [--name NEW] [--look L] [--consoles N] [--login CMD] [--when-display-lost V] [--isolated yes|no] [--switch-on-enter yes|no]",
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
                "look set NAME [--name NEW] [--frit F] [--size PT] [--cell WxH] [--engine E] [--hinting H] [--line-spacing PCT] [--display-depth D] [--display-dpi DPI] [--login-image FILE] [--login-image-fit FIT] [--login-image-rows N] [--login-image-position P] [--login-image-scale PCT] [--palette NAME]",
                "change a look's settings",
            ),
            form("look rm NAME [--force]", "delete a look"),
        ],
        detail: "Adds, changes or deletes a [look NAME] section in the draft. A look is a font and its size, which heads use. --frit sets its font-name, --size its font-size and --cell its character cell; --engine, --hinting, --line-spacing and --ligatures set font-engine, font-hinting, font-line-spacing and font-ligatures; --display-depth, --display-dpi, --login-image, --login-image-fit, --login-image-rows, --login-image-position, --login-image-scale and --palette set the keys of the same name. look add takes the same options as look set. A setting a look leaves out comes from the top of vtium.conf, and in one look a font-size other than 0 overrides cell. look rm does not delete a look a head uses, and names those heads, unless --force switches them to the default look. Each takes --dry-run and --install.",
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
        detail: "Opens a copy of the current draft in your editor ($VISUAL, else $EDITOR, else vi). When you save and quit, the copy becomes the draft and vtium check-config checks it. If the check finds errors or warnings, vtium-config prints them and asks whether to edit again. When the check exits 6, because it warns only of missing fonts, vtium-config shows the warnings and does not ask. With --install, it then installs the draft, but only if the check exits 0 or 6.",
        example: "vtium-config edit --install",
    },
    Topic {
        command: Command::Fonts,
        forms: &[
            form(
                "fonts",
                "list the font sets and built-in fonts, and the font files each is missing",
            ),
            form("fonts --files", "list the font files found on this machine"),
            form(
                "fonts --install-system FILE [--replace]",
                "install a font file of yours as a system font",
            ),
        ],
        detail: "fonts lists every font set (frit) and built-in font, one per line: its name, whether vtium can read it, and the font files it needs that are not on this machine. It changes nothing and needs no root. \
                 fonts --files lists every font file that vtium-config finds. vtium-config looks in the system's font folders, in vtium's own, in kbd's console-font folders, in the folders that font-paths names, and in your own ~/.local/share/fonts and ~/.fonts. Each line has ten tab-separated columns. The first four are the file, the kind of folder, its state in your current draft as active prints it, and whether it passes the trust rule. The next five describe the font: its family and style, the sizes it supplies, monospaced or proportional, the scripts it covers, and its groups, such as Nerd Fonts, Powerline, color emoji or box drawing. The last column notes a file that a package installed too. The trust rule decides which files vtium may draw from. The file and each folder above it must belong to root, and no one else may write to them. None of them may be a symbolic link, and the file's filesystem must stay mounted. The font helper reads each file in its sandbox, and each line prints as soon as its file is read. When you have no draft yet, fonts --files makes one from the installed vtium.conf, as active does. A font in your own folder is not usable, because vtium draws only from fonts that root owns. Install it as a system font first. \
                 fonts --install-system FILE copies FILE into /usr/local/share/fonts, owned by root with mode 0644. vtium-config reads FILE once into a draft, and the font helper checks that the draft's copy is a font. A font that a package installed under /usr/share/fonts is refused, because it is a system font already. When /usr/local/share/fonts already holds the same file, nothing changes. When it holds a different file of the same name, the command shows both files. The command replaces the installed file only after you answer y, or with --replace. The draft is installed through sudo vtium-config apply DIR. At a terminal the command shows that command and asks before it runs it. Away from a terminal it prints the draft and the command, and runs nothing. After the copy, apply checks the font against the trust rule. Installing a font does not activate it: run activate afterwards.",
        example: "vtium-config fonts --install-system ~/.local/share/fonts/MyMono.ttf",
    },
    Topic {
        command: Command::Check,
        forms: &[form(
            "check FILE",
            "check a font set (NAME.vtfrit) or a vtium.conf",
        )],
        detail: "Checks FILE as the console would read it. A file named NAME.vtfrit is checked as a font set, and any other file as a vtium.conf, settings and sections together. Each problem prints with the file's name and line number, and a good file prints \"ok FILE\". It changes nothing and needs no root. It exits 0 when it finds no problem and 1 when it finds one. It exits 6, as vtium check-config does, when a vtium.conf's only problems are fonts that a font set needs and this machine lacks.",
        example: "vtium-config check /etc/vtium/vtium.conf",
    },
    Topic {
        command: Command::NewFrit,
        forms: &[
            form(
                "new-frit FONT... [--name NAME] [--cell WxH] [--size POINTS] [--layer NAME]...",
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
        detail: "Makes a font set (frit) from the fonts, in order: each later font supplies characters the earlier ones lack. A FONT is a family name, or a source as a frit file writes it (kbd:ter-v16n, builtin:unifont). --name names the font set; without it, the name comes from the first font. --cell and --size set the character size. Each --layer adds a layer from the layers folder of the font sets' directory, such as nerd-symbols or noto-emoji. The layers follow the fonts in the order of their roles: scripts, then symbols, then emoji, then the fallback, whatever order you give them in. --from installs a frit file you wrote, and --template prints one to start from. vtium-config checks the font set before it saves anything. Run as a user, it writes a draft and prints the command that installs it.",
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
        command: Command::Activate,
        forms: &[form(
            "activate PATH... [--dry-run] [--install]",
            "let vtium use a font, color scheme, color map, shader or pointer theme",
        )],
        detail: "Activates, in the draft, each resource that a PATH names. A resource is a font, color scheme, color map, shader or pointer theme, given by its absolute path. vtium uses only the active resources, which /etc/vtium/batch.conf lists. activate first checks that root owns the file and each folder above it, and that nobody else can write them. None of them may be a symbolic link. The file must be on a filesystem that stays mounted, so a file on removable media or a network filesystem fails. If any PATH fails, activate changes nothing. The change waits in the draft until you install it, and revert drops it. When no batch.conf is installed yet, the install writes one from the resources of a fresh install, with your change. --dry-run shows the change without saving it, and --install installs the draft afterwards.",
        example: "vtium-config activate /usr/share/fonts/TTF/Hack-Regular.ttf",
    },
    Topic {
        command: Command::Deactivate,
        forms: &[form(
            "deactivate PATH... [--dry-run] [--install]",
            "stop vtium using a resource",
        )],
        detail: "Deactivates, in the draft, each resource that a PATH names. From the next login after you install the change, vtium no longer uses the resource: it treats a font as a missing font, and any other resource as a file that does not exist. When a look uses the resource, deactivate names the look. It can tell this for a font that a font set names by its path, for a color scheme, and for a color map or shader that the look's vtlook names. It cannot tell this for a font that a font set names by its family. The built-in font is always active, and deactivate refuses it. The change waits in the draft until you install it, and revert drops it. --dry-run shows the change without saving it, and --install installs the draft afterwards.",
        example: "vtium-config deactivate /usr/share/vtium/fonts/CulletMono-Bold.ttf",
    },
    Topic {
        command: Command::Active,
        forms: &[form(
            "active",
            "list the resources vtium may use, with the draft's changes",
        )],
        detail: "Lists the active resources, one per line, tab-separated: the kind (font, color scheme, color map, shader or pointer theme), the path, and the state. The built-in font comes first and is always active. A resource the draft activates says activates on install, and one it deactivates says deactivates on install. When no /etc/vtium/batch.conf is installed, the list is the resources of a fresh install, and a last line says so. It changes nothing.",
        example: "vtium-config active",
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

/// How vtium-config's command line is written.
pub const SYNOPSIS: &str = "vtium-config [OPTION]... [COMMAND [ARG]...]";

/// What vtium-config is, in paragraphs.
pub const DESCRIPTION: [&str; 2] = [
    "Sets up the vtium console's fonts and settings, and can preview each font live on a vtium console.",
    "With no command, vtium-config prints its usage. To open the full-screen tool, run vtium-config tui.",
];

/// What the commands change, after the list of commands.
pub const DRAFTS: &str = "The commands from flow to edit change your draft, a copy of vtium.conf in ~/.config/vtium/drafts/NAME.conf; only install writes /etc/vtium/vtium.conf. activate and deactivate keep their changes beside the draft, in NAME.batch, and install writes them to /etc/vtium/batch.conf. The font set commands (new-frit, remove-frit) write a draft and print the command that installs it.";

/// The statuses vtium-config exits with, and what each means.
pub const EXIT_STATUS: [(u8, &str); 7] = [
    (0, "The command did what you asked."),
    (
        1,
        "The command failed. For install, the draft failed vtium check-config. For check, FILE has an error, or a warning about something other than missing fonts.",
    ),
    (2, "The command line is wrong."),
    (
        3,
        "install did not install the draft, because the installed vtium.conf changed after the draft was made.",
    ),
    (4, "sudo refused to run apply."),
    (5, "A file could not be read or written."),
    (
        vtium_settings::CHECK_MISSING_FONTS,
        "check found no problem in FILE but fonts that a font set needs and this machine lacks: a font that is not installed, or one that lacks a character the set checks for. vtium check-config exits 6 in the same case.",
    ),
];

/// The options `--help` and `--version`, which the parser reads apart
/// from [`crate::cli::VALUE_OPTIONS`].
pub const HELP_OPTIONS: &str = "--help [TOPIC], --version";

/// The widest line of the usage, in columns.
const WIDTH: usize = 78;

/// `text` wrapped into lines of at most [`WIDTH`] columns, the first
/// starting with `first` and the others with `rest`.
fn wrap(text: &str, first: &str, rest: &str) -> String {
    let mut out = String::new();
    let mut line = first.to_owned();
    let mut empty = true;
    for word in text.split_whitespace() {
        if !empty && line.len() + 1 + word.len() > WIDTH {
            out.push_str(&line);
            out.push('\n');
            line = rest.to_owned();
            empty = true;
        }
        if !empty {
            line.push(' ');
        }
        line.push_str(word);
        empty = false;
    }
    out.push_str(&line);
    out.push('\n');
    out
}

/// Each command's help, in the order the usage lists them.
pub fn topics() -> impl Iterator<Item = &'static Topic> {
    Command::ALL.into_iter().filter_map(Command::topic)
}

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
    let mut text = format!("Usage: {SYNOPSIS}\n\n");
    for paragraph in DESCRIPTION {
        text.push_str(paragraph);
        text.push_str("\n\n");
    }
    text.push_str("Commands:\n");
    for topic in topics() {
        for f in topic.forms {
            line(&mut text, f);
        }
    }
    text.push_str("\nHelp on one command: vtium-config --help COMMAND. Also:\n  ");
    text.push_str(ABOUT_LINE);
    text.push_str("\n\n");
    text.push_str(&wrap(DRAFTS, "", ""));
    text.push_str("\nExit status:\n");
    for (code, means) in EXIT_STATUS {
        text.push_str(&wrap(means, &format!("  {code}  "), "     "));
    }
    text.push_str("\nOptions:\n");
    for o in crate::cli::VALUE_OPTIONS {
        let option = format!("{} {}", o.name, o.value);
        text.push_str(&format!("  {option:<21} {}\n", o.does));
    }
    text.push_str(&format!("  {HELP_OPTIONS}\n"));
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
