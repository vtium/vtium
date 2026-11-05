# Configure vtium

This guide changes how vtium behaves. vtium reads one configuration file, `/etc/vtium/vtium.conf`, which holds both its settings and its heads, the terminals it shows on your displays. You change that file in one of two ways: with `vtium config`, which keeps your changes in a draft of your own and installs the draft when you are ready, or as root with any editor. [config.md](config.md) lists every setting with its default and its bounds, [The sections of vtium.conf](head-file.md) is the reference for the heads, looks and rules, and [Set up heads](heads.md) gives a complete file for each common setup.

In the commands below, `#` marks a command you run as root, for example with sudo, and `$` a command you run as your own user.

## The file

vtium reads `/etc/vtium/vtium.conf`. The package installs no such file, so vtium runs with its defaults until one is installed. The package does install `/etc/vtium/vtium.conf.example`, which lists every setting at its default, commented out, followed by the sections of the configuration vtium runs with no file.

The file has two parts. The top is a list of settings, one a line, which vtium takes from `kmscon`'s configuration file, `kmscon.conf`, and adds its own to. A setting with a value is `key=value`. A switch is the key alone to turn it on, or the key with `no-` before it to turn it off. `#` starts a comment, except in a `#rrggbb` color. A `kmscon.conf` copied to `/etc/vtium/vtium.conf` reads as it is. For example:

```
font-name=monoid
xkb-layout=de
dpms-timeout=300
bell
no-mouse
```

Below the settings come the sections, each starting with a heading in brackets. A `[look NAME]` section says what a head draws with: its frit (a font set file), its size and its other font settings. A `[head NAME]` section declares a head and names its look. One `[rules]` section lists the rules that send each display and each input device to a head. A file with sections needs `version = 1` among the settings above them. [The sections of vtium.conf](head-file.md) describes each section and its keys. A file with no section runs one head on every display.

Every setting is also an option on vtium's command line, such as `--font-name=monoid` or `--no-mouse`. The command line wins over the settings at the top of the file, and a look's or a head's own section wins over both. `--configdir=DIR` names another directory than `/etc/vtium`, for a vtium you run by hand.

vtium reads the file only when root owns it and neither its group nor others can write it, nor any directory above it. It refuses a file that fails this check: it reads none of it, runs its built-in configuration, and every head shows `invalid config skipped, using builtin` with the next step, `Run vtium check-config /etc/vtium/vtium.conf to see why.` `vtium check-config` names the file or directory that fails and the `chown` or `chmod` command that mends it.

vtium reads the file once, when it starts, together with the frits it names. A change reaches a VT at that VT's next start, so log out of the VT and restart its vtium, as in [Upgrading and removing](install.md#upgrading-and-removing).

vtium reads no other configuration file. A `heads.conf` or `seat0.seat.conf` left in `/etc/vtium` from an earlier version does nothing, and vtium says nothing about it; move what it held into vtium.conf by hand.

## Check a file

`vtium check-config` reads the configuration as a start would, applies nothing, and prints each problem it finds. Given a file, it checks that file in place of the installed one, so you can check a file before you install it:

```
$ vtium check-config
$ vtium check-config ./vtium.conf
```

It exits 1 when it finds an error or a warning, and 0 when it finds none. It also refuses a file that leaves nobody at the machine able to type: one with no complete route, which is a display that is present and that the rules send to a head, together with a keyboard that is present and reaches that head, through an input rule or by roaming. The message says what is missing, such as `No display present is sent to a head`. Where no display or no keyboard is present, as in a container, `check-config` judges the rules alone and says so. `vtium check-config --no-devices FILE` asks for that reading on any machine, for a file written for another machine whose monitors and keyboards are not plugged in here. The shipped configuration always passes.

## Change it with vtium config

`vtium config` runs `vtium-config`, vtium's configuration tool. It never edits `/etc/vtium/vtium.conf` while you work. It edits your draft, a whole vtium.conf of your own, and writes `/etc/vtium/vtium.conf` only when you install the draft. vtium reads only the installed file, so nothing you change in the draft reaches the console until you install it.

The tool and this guide use six words, each for one thing:

| Word | Meaning |
|---|---|
| draft | your working copy of vtium.conf, saved after every change you make |
| install | the one action that writes `/etc/vtium/vtium.conf`: `vtium config install`, or Install in the TUI |
| restore | copying a file that an install replaced back into your draft |
| tag | the mark Space puts on an item in the TUI's Flow view |
| path | a complete chain of tagged items, such as a keyboard, a head and a look |
| connect | what `c` does in the Flow view: it turns a path into changes in the draft |

The configuration means the installed file and your draft together.

### The draft

The first command that needs a draft makes one, as a copy of the installed file, or of the shipped example when none is installed. It names the draft after the machine's hostname, adding `-2`, `-3` and so on when that name is taken, so that no draft is overwritten. Your drafts and everything else the tool keeps live under your home directory:

| Path | What it holds |
|---|---|
| `~/.config/vtium/drafts/NAME.conf` | each draft, a whole vtium.conf |
| `~/.config/vtium/vtium-config.conf` | the tool's own settings: `draft = NAME`, the draft you work on, and `history = N`, how many replaced files the installed history keeps, 10 unless you set it |
| `~/.config/vtium/library.conf` | the library: looks, heads and rules you keep apart from any draft, in vtium.conf's section syntax |
| `~/.config/vtium/installed/DRAFT-N.conf` | the installed history: each file an install replaced, numbered for each draft |
| `~/.local/state/vtium/bases/NAME.base` | each draft's base: the SHA-256 of the installed file the draft was made from or last installed as |
| `~/.local/state/vtium/drafts/` | the install bundles that an install hands to its root step ([below](#when-vtium-config-asks-for-sudo)) |

`XDG_CONFIG_HOME` and `XDG_STATE_HOME` move these from `~/.config` and `~/.local/state`. Drafts are never kept in `/etc`, which holds only the installed file.

A draft may be unfinished. The tool writes every change you make to it, whatever the check says, so you can build a configuration in any order. After each change it runs `vtium check-config` on the draft, prints what the check finds, and ends with a state line such as:

```
draft north: 3 lines added, 1 removed · 1 problem
```

The state says `same as installed`, the lines the draft adds and removes against the installed file, or `made before the installed file changed` when someone has changed the installed file since the draft was made from it. Only an install needs the check to pass.

### The commands

These commands change the draft. `vtium config --help` lists every command, and `vtium config --help COMMAND` explains one.

| Command | What it does |
|---|---|
| `vtium config flow [--json]` | prints the draft's flow, from the displays to the input devices, with the devices plugged in now placed on it and each rule numbered; `--json` prints it for scripts |
| `vtium config route ITEM [--json]` | prints the routes an item is part of, as the TUI's route card shows them; ITEM is `display NAME`, `rule N`, `head NAME`, `look NAME` or `input NAME` |
| `vtium config diff` | prints the difference between the installed file and the draft |
| `vtium config get KEY` | prints a setting's value in the draft, or its default |
| `vtium config set KEY VALUE`, `set --unset KEY` | sets a setting at the top of the draft, or leaves it at its default |
| `vtium config rule add [--before N] 'KIND SELECTOR -> TARGET'` | adds a rule, such as `'input path:pci-0000:15:00.0-usb-0:3.3 -> desk'` |
| `vtium config rule set N ...`, `rule move N --before M`, `rule move N --last`, `rule rm N` | replaces rule N or changes its selector or target, moves it, or removes it |
| `vtium config head add NAME [--look L]`, `head set NAME ...`, `head rm NAME` | adds a head, changes its keys or its name, or deletes it |
| `vtium config look add NAME [--frit F] [--size PT]`, `look set NAME ...`, `look rm NAME [--force]` | adds a look, changes its keys or its name, or deletes it |
| `vtium config edit [--install]` | opens the draft in your editor, as `visudo` opens sudoers |
| `vtium config install` | checks the draft and installs it as `/etc/vtium/vtium.conf` |
| `vtium config revert` | throws away the draft's changes and copies the installed file back over it |
| `vtium config new` | empties the draft to the settings at their defaults and one look, `[look default]` |
| `vtium config history`, `restore [N]` | lists the files earlier installs replaced, newest first, and copies one into the draft |
| `vtium config draft list`, `draft new [NAME]`, `draft switch NAME`, `draft rm [NAME]` | lists the drafts, makes one, makes another the one you work on, or deletes one |
| `vtium config library ...` | copies looks, heads and rules between the draft and the library |

A rule's number N is its place in `[rules]`, counted from 1, as `flow` prints it. `rule add` without `--before` puts a rule where it does not hide behind a wider rule: a rule that names one device, by `edid`, `connector`, `connector-path`, `serial`, `path`, `bluetooth` or `name`, goes before the first rule of its kind that names a kind of device, by `class`, `id`, `card`, `udev` or `*`; a rule that names a kind goes before the first `*` rule of its kind; any other rule goes last. `head rm` keeps the rules that send to the head and names them, and the check lists them until you point each elsewhere or remove it. `look rm` refuses a look that a head uses, naming the heads, unless `--force` moves those heads to the default look. `--name` on `head set` and `look set` renames the section and every rule or head that names it.

Each command that changes the draft also takes `--dry-run`, which prints the change as a difference and what the check finds and writes nothing, and `--install`, which installs the draft after the change. A command asks for no confirmation: running it is the confirmation. For example, to give a head named `console` a larger look:

```
$ vtium config look add big --frit monoid --size 14
$ vtium config head set console --look big
$ vtium config diff
$ vtium config install
```

`vtium config edit` copies the draft to a file under `~/.local/state/vtium/`, opens it in the editor `$VISUAL` names, else `$EDITOR`, else `vi`, and makes your edited copy the draft when the editor exits. When the check then finds an error or a warning, it prints each one with its line and asks whether to edit again. With `--install` it installs the draft afterwards, and only when the check passes.

A draft is a complete vtium.conf, so you can build a configuration on one machine and use it on another, such as a small board whose console is too narrow for the TUI. Copy the draft into `~/.config/vtium/drafts/` on the other machine, make it the current draft with `vtium config draft switch NAME`, and install it there. On the machine that built it, `vtium check-config --no-devices` checks it by its rules alone.

The commands exit with a fixed status, so a script can tell the outcomes apart:

| Exit status | Meaning |
|---|---|
| 0 | done: the draft is written, whatever the check finds; or the flow or the difference is printed; or the draft is installed |
| 1 | the check failed at an install, and nothing is installed; the draft keeps every change |
| 2 | the command could not be used: an unknown option, a rule, head or look that does not exist, or a value its key cannot take |
| 3 | the installed vtium.conf changed since the draft was made from it, and nothing is installed |
| 4 | sudo refused or is missing, and nothing is installed |
| 5 | a file could not be read or written, or the console that checks the draft could not be started; the message names the path |

When an install stops with status 3, someone changed `/etc/vtium/vtium.conf` after your draft was made from it, for example as root with an editor. `vtium config diff` shows what differs. `vtium config revert` starts the draft again from the installed file, or you copy your changes across by hand.

### After an install

An install keeps the file it replaced in the installed history, `~/.config/vtium/installed/`, which keeps as many files as `history` sets ([The draft](#the-draft)). `vtium config history` lists them, newest first, and `vtium config restore N` copies file N into the draft, where you look at it and install it as usual. Only an install writes `/etc/vtium/vtium.conf`; a restore changes only the draft. The installed file takes effect as [The file](#the-file) says, at each vtium's next start.

## When vtium-config asks for sudo

Everything `vtium config` does runs as you, apart from one step of an install. It reads the installed file, the frits and the devices plugged in without privilege, and keeps your drafts in your home directory. Only an install asks for sudo, because only root may write `/etc/vtium`.

An install takes three steps:

1. **As you,** vtium-config refuses a draft made before the installed file changed, with exit status 3, and asks nothing. It then checks the draft with `vtium check-config` and refuses a draft that fails, with exit status 1. It builds an install bundle under `~/.local/state/vtium/drafts/`, in a new directory named for the time: the new vtium.conf, any frits the TUI's Fonts view has waiting to install, and a list of each file the bundle replaces with the SHA-256 of that file as it is when the install starts.
2. **As root,** one command runs: `sudo /usr/bin/vtium-config apply DIR`, where DIR is the bundle's directory. sudo asks for your password here. The command takes no option that names a path or a program, so a sudo rule can allow exactly that command. It reads the bundle without following links. It refuses the bundle, writing nothing, when a file it would replace has changed since the install started. It checks every file before it writes any: each frit with vtium's frit reader, and the new vtium.conf with the console's own `vtium check-config`. It then writes the frits, and vtium.conf after them, so that vtium.conf never names a frit that is not there yet. It writes each file beside the old one and renames it into place, mode 0644, and keeps the old file as `NAME.bak`, such as `vtium.conf.bak`. This step is one short process: about 20 ms on the maintainer's machine, check included; a large vtium.conf or many frits take longer.
3. **As you again,** vtium-config saves the file the install replaced in your installed history and records the new installed file as the draft's base, so the draft and the installed file are the same.

When sudo refuses, or is not installed, nothing is installed and the command exits with status 4; run `vtium config install` again. The bundle stays on disk, but running `sudo vtium-config apply DIR` on it by hand skips the third step: nothing is saved in the installed history, and the draft's base stays the old installed file, so the next `vtium config install` stops with exit status 3.

Root needs no sudo. Run as root, `vtium config` keeps root's own drafts under root's home directory, `/root/.config/vtium/drafts/` by default, which it finds from the password database and never from `$HOME` or the XDG variables that sudo can carry over from your account. An install as root runs the same checks and writes the files the same way at once. The TUI does not run as root: it says to run `vtium config` as yourself, since it asks for sudo when it installs.

## The TUI

`vtium config tui` opens vtium-config's full-screen view of the configuration, its TUI, in the terminal you run it in. It needs a terminal of at least 80 by 25 cells; below that it shows only the size it needs and the commands, which work at any size. Its views are Flow, Looks, Fonts, Settings and Help, and Tab steps through them. It opens on Flow.

The Flow view draws the draft as six columns, from left to right: the displays, the display rules, the heads, their looks, the input rules and the input devices. Each column has a color of its own, from the console's sixteen colors; displays, heads, looks and input devices are blocks of color, rules are colored text, and lines join each device to the rule that places it and each rule to its target. With `NO_COLOR` set, or in a terminal without color, the view draws the same text without color, each block's text in brackets.

The view places the devices plugged in now on the draft's rules, and follows them while it is open: a device you plug in appears at once, placed by the first rule that matches it. A rule that names one device, such as a monitor by its EDID or a keyboard by its USB port, keeps that device on the flow when it is unplugged, as a gray block marked `*` and `configured, not found`. The device leaves the configuration only when you delete its rule.

| Key | What it does |
|---|---|
| Tab, Left and Right | move between the columns; Tab past the last column moves to the next view |
| Up and Down | move within a column |
| Enter | opens the selected item's card below the columns, to change its properties |
| Space | tags the selected item, or untags it |
| `c` | connects the tagged items: it turns the path they make into changes in the draft |
| Esc | clears every tag |
| `f` | follows the selected item's line toward the heads |
| `?` | opens the route card of the selected item |
| `d` | reads the devices plugged in again |
| `v`, `i`, `r`, `o`, `n`, `x` | View, Install, Revert, Restore, New and Delete draft |
| `q` | leaves |

The mouse selects an item with a click and opens it with a second click.

Each change you make in the view goes into the draft at once, and the top row counts the draft against the installed file, such as `draft north · 3 changes · 1 problem`. A `new rule…`, `new head…` or `new look…` item at the foot of the rules, heads and looks columns adds one. A card lets you pick each value from the values it can take, or type one, and a delete asks first and names what still uses the item. The row above the columns, `base`, is a card of its own for the settings at the top of the file.

Tagging connects items without typing. Tag a keyboard, then a head, and press `c`: the keyboard's own rule now sends it to that head, or, when it has no rule of its own, a new rule naming it goes in. Tag a head and a look, and `c` sets the head's look. `c` shows the commands it runs, such as `vtium config head set panel --look small`, writes them into the draft and clears the tags. When the tags make no complete path, it says what is missing.

The route card, on `?`, says in words what the selected item does and lists every route it is part of, from the display through its rule, head, look and input rule to the input device. An item nothing uses says so, such as `not used: no rule sends anything here`, and a broken item says what is broken.

The actions in the top row work on the whole draft. Each but View asks before it acts:

- **View** shows the draft as text in a pager, with each line the check complains about marked; `j`, `k`, `g`, `G`, Ctrl-d and Ctrl-u move, and `q` or Esc goes back.
- **Install** runs the check, refuses when it fails, and otherwise asks `Install the draft? 3 lines added, 1 removed.`, then installs as [above](#when-vtium-config-asks-for-sudo). The TUI leaves its screen while sudo asks for your password.
- **Revert** throws away the draft's changes and copies the installed file back over it.
- **Restore** lists the installed history and copies the file you choose into the draft.
- **New** empties the draft to the settings at their defaults and one look, `[look default]`.
- **Delete draft** deletes the draft. The next change starts a new one from the installed file.

The Looks view lists the console's sizes until vtium-config edits looks of their own. The Fonts view lists the frits and the built-in fonts, tries each on the live console, and makes and removes frits ([Choose a font](fonts.md#choosing-a-font-with-vtium-config)); a frit you make or remove there waits and goes in with the next Install. The Settings view lists every setting with its value, and Enter changes the highlighted one in the draft. The Help view lists the keys of each view and the commands.

## Edit the file as root

Root can still edit `/etc/vtium/vtium.conf` with any editor. Start from the example, check your file, then install it owned by root:

```
# cp /etc/vtium/vtium.conf.example /etc/vtium/vtium.conf
# vtium check-config /etc/vtium/vtium.conf
```

A draft that was made before such an edit shows `made before the installed file changed`, and its install stops with exit status 3 until you revert the draft or carry the change across by hand.

## Blanking from a script

`vtium blank --vt N` turns off the displays of the vtium on VT N at once, and `vtium wake --vt N` turns them on again, as a key press does. Root and the user vtium runs as may send them:

```
# vtium blank --vt 3
```
