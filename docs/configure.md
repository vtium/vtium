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

vtium reads the file only when root owns it and neither its group nor others can write it, nor any directory above it. It refuses a file that fails this check: it reads none of it, runs its built-in configuration, and every head shows `vtium.conf not used: running the built-in configuration.` with the next step, `To see why, run vtium check-config /etc/vtium/vtium.conf.` `vtium check-config` names the file or directory that fails and the `chown` or `chmod` command that mends it.

vtium reads the file once, when it starts, together with the frits it names. A change reaches a VT at that VT's next start, so log out of the VT and restart its vtium, as in [Upgrading and removing](install.md#upgrading-and-removing). `vtium config install` offers that restart for you. See [How an install reaches the VTs](#how-an-install-reaches-the-vts).

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

The first command that needs a draft makes one, as a copy of the installed file. It names the draft after the machine's hostname, adding `-2`, `-3` and so on when that name is taken, so that no draft is overwritten.

When no vtium.conf is installed, the draft starts as a short comment and nothing else. The comment says what the file is, and that `/etc/vtium/vtium.conf.example` and `vtium --help` describe every setting. Each change you make then adds only the lines it needs. A setting you change adds its own line, with no description above it. The first change to the looks, heads or rules adds `version = 1`, the head `console` and the rules that vtium runs when the file has no section, without the comments of the shipped example. A draft copied from an installed file keeps every line of that file, your own comments included.

Your drafts and everything else the tool keeps live under your home directory:

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
| `vtium config new` | empties the draft to the short comment, `version = 1` and one look, `[look default]`, so that it sets no setting |
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

## How an install reaches the VTs

vtium reads vtium.conf only when it starts on a VT, so an install changes nothing on your screens until vtium starts again. A head with a `vt` line also appears only on the VTs that line names, and only where vtium runs. For example, a head with `vt = 12` needs vtium on VT 12, but logind starts vtium by itself only on VTs 1 to 6.

So after `vtium config install` or the TUI's Install writes `/etc/vtium/vtium.conf`, vtium-config takes two more steps:

1. **It enables vtium on the VTs your heads name.** vtium-config runs `vtium console status` to see where vtium is enabled. For each VT that a head's `vt` line names where vtium is not enabled, it runs `sudo vtium console enable --direct N`. It uses seat mode, `--seat N`, when the VTs vtium is enabled on already use seat mode. When they use both modes, or vtium is not enabled on any VT yet, it uses direct mode and says why. `vtium console` decides whether vtium can have the VT, and vtium-config shows what it prints. For example, `vtium console` refuses a VT that a display manager or a graphical session holds. The install stays in place after a refusal, but that head does not appear until the VT is free and you run the `vtium console enable` command that vtium-config shows.
2. **It offers to start and restart vtium.** vtium-config asks, for example, `Start vtium on VT 12, and restart it on VTs 2 and 3? [y/N]`. When you answer y, it runs `sudo systemctl start vtium@tty12` and `sudo systemctl restart vtium@tty2 vtium@tty3`, and the new file applies at once. A restart keeps each session on those VTs running, because systemd holds the session's terminal open while vtium starts again. The screen is cleared and drawn again, and its scrollback is lost. Starting vtium on a VT where a login prompt runs stops that login prompt, and the question says so. When you answer anything else, vtium-config starts and restarts nothing and prints the commands to run later. The new file then applies on each VT when vtium next starts there, for example at the next boot.

`vtium config install` asks only at a terminal. Without one, and for the `--install` option of the other commands, it asks nothing and prints the commands instead. The TUI asks on the terminal it gives back to sudo, and its message line then says where vtium runs with the new file. An install into another directory, named with `--configdir`, takes neither step, because the vtium on each VT reads only `/etc/vtium`.

## The TUI

`vtium config tui` opens vtium-config's full-screen view of the configuration, its TUI, in the terminal you run it in. It needs a terminal of at least 80 by 25 cells; below that it shows only the size it needs and the commands, which work at any size. Its views are Flow, Looks, Fonts, Settings and Help, and it opens on Flow. Tab moves to the next view and Shift-Tab to the one before, wherever the selection is. The arrow keys move inside a view. While a card, a list, or a value you are typing is open, Tab does nothing. Press Esc to close it first. While a question waits for your answer, Tab answers no.

### The Flow view

The Flow view shows the draft as paths. A path is how vtium joins things when it runs: one or more displays show a terminal, the terminal draws with a font set, and inputs such as keyboards and mice type into it. In vtium.conf, a path is one head with its rules.

The view is one document with three sections. From the top, they are CONFIGURATION, NEW PATH and PARTS. You build a path from the bottom up: you pick parts in PARTS into NEW PATH, and then you add the finished path to CONFIGURATION. The document scrolls as a whole under the tab bar, the status line and the key line, and it always keeps the highlighted box in sight.

- **CONFIGURATION** shows the paths the draft holds, one row each, under the column titles DISPLAYS, TERMINALS and INPUTS. Each row reads displays → terminal ← inputs. A terminal's box shows the terminal's name. Under the name, it shows the font set and the VTs the terminal runs on, such as `font set proof · VT 3` or `default font set · every VT`.
- **NEW PATH** shows the path you are building, with dashed borders. An empty place in it reads `Choose… [Enter]`, and the line beside the section's title says what the path still needs.
- **PARTS** holds the parts you can pick, in four groups: DISPLAYS, TERMINALS, INPUTS and FONT SETS. vtium-config makes a part for each display, keyboard and pointer that is connected, for each device that the draft's rules name, and for each special part the draft uses.

Two special terminals stand for something other than a head. Inputs that roam type into whichever terminal is on screen. CONFIGURATION draws them once, in the row ROAMING, whose terminal is the dotted box `Active terminal`, next to the sentence `These inputs type into the active terminal.` A path to `Nowhere` holds the displays that vtium leaves off and the inputs that it ignores.

The view follows the devices while it is open. A device you plug in adds a part. When you unplug a device, its part turns grey and reads `Not connected`. The device stays in the configuration until you take its part out of its path. Press `d` to read every device again.

The box's title color shows the kind of part: displays are cyan, terminals yellow, inputs green and font sets magenta. The border shows the part's state. An amber border marks a connected device that is in no path yet, a grey border marks a part you cannot pick, and a check mark `✓` marks a part you picked into NEW PATH. With `NO_COLOR` set, or in a terminal without color, the view draws the same text and borders without color. You can choose the colors in `$XDG_CONFIG_HOME/vtium/vtium-config.conf`.

#### Making a part

Each group in PARTS starts with its template. A template is a box with a double border that reads `Add display… [Enter]`, `Add terminal… [Enter]`, `Add input… [Enter]` or `Add font set… [Enter]`. You never put a template in a path. Instead, press Enter on a template, or on an empty place in NEW PATH, to open its part maker. A part maker asks one question and lists the choices that answer it, each with a description.

| Part maker and its question | Choice | Description |
|---|---|---|
| `ADD A DISPLAY`: `Which displays should this include?` | `All displays` | Every display, connected now or later |
| | `This monitor` | One monitor by its EDID, on any port |
| | `Connector` | Display on one connector, such as HDMI-A-1 |
| | `Hub port` | Display on one port of a DisplayPort hub |
| | `Graphics card` | Every display on one graphics card |
| `ADD A TERMINAL`: `Where should this path go?` | `New terminal…` | Separate terminal with its own consoles and look |
| | `Active terminal` | Terminal on screen, following you as you switch |
| | `Nowhere` | Displays off, inputs ignored |
| `ADD AN INPUT`: `Which inputs should this include?` | `No input` | Display only, with no keyboard or mouse |
| | `All keyboards` | Every keyboard, connected now or later |
| | `All mice and touchpads` | Mice, touchpads and trackballs |
| | `All input devices` | Keyboards, pointers and every other kind of input |
| | `USB port` | Devices plugged into one USB port |
| | `This one device` | One device by its serial number, in any port |
| | `Model` | Every device of one make and model |
| | `Bluetooth device` | One paired Bluetooth device |
| | `Device name` | Devices that report a given name |
| | `Device type` | Touchscreens, tablets, joysticks and other kinds |
| | `udev property` | Devices with a given udev property (advanced) |
| `ADD A FONT SET`: `Which font set should the terminal use?` | `default font set` | Font that vtium.conf sets |
| | each font set the draft defines | font set |

The input maker lists the choices from `USB port` on under the heading `OR CHOOSE BY`. Each of these choices, and each display choice after `All displays`, opens a picker. The picker lists each value that the connected devices give, with the devices that each value matches now. Its last line is `Other…`, where you type a value yourself. vtium-config checks a typed value the way vtium reads a rule, and when it cannot use the value, it says so in one sentence, such as `This USB port is not a valid port path. A port path looks like pci-0000:00:14.0-usb-0:3.4.`

A choice makes a part, which joins its group in PARTS, and the status line says, for example, `Added DP-9. Press Space to put it in the new path.` When you opened the part maker from NEW PATH, the new part also goes into the new path.

#### Building and adding a path

Space on a part picks it into NEW PATH, and Space on it again takes it out. While you pick, the line beside NEW PATH says what the path still needs, such as `Pick an input and a display to make a path.` or `Add a display to finish this path.` A path to a terminal of its own is complete once it has its terminal and at least one display. A path to `Active terminal` needs at least one input and no display, and a path to `Nowhere` needs at least one display or input. A complete path reads `This path is complete. Press a to add it.`

Press `a` to add the path to CONFIGURATION. vtium-config checks the path first. When something conflicts, the path stays in NEW PATH and one sentence says what conflicts. For example, a name that another terminal has is refused.

A display or an input can be in one path for each VT. Two paths can share a display when their terminals run on different VTs. For example, the terminal desk on VT 3 and the terminal panel on VT 4 can both show `All displays`, and each VT then shows its own terminal. When you pick a display that a path on the same VT already holds, vtium-config refuses it and names that VT: `The display All displays is already in the terminal desk on VT 3. A display can be in one path for each VT.` An input in two paths on the same VT roams instead. Adding the path moves that input to the ROAMING row, and the status line says so when you pick it.

#### A terminal's card

Enter on a terminal's box opens its card. The card lists the terminal's settings: `Name`, `Font set`, `Size`, `Consoles`, `Login` and `When its screen is unplugged`. Up and Down move between the lines, Left and Right move between a line's choices, and Enter sets the choice. On `Name`, Enter lets you type a new name.

The `Login` line sets the program each console of the terminal starts. It has two choices:

- `Default: /bin/login -p` leaves the terminal's own `login` key unset. The choice shows the command that runs then: the `login` key at the top of vtium.conf, or vtium's default, `/bin/login -p`.
- `Other…` lets you type a command. vtium-config keeps it only when its first word is the full path of a program that exists and can run, or a variable such as `$SHELL`, which vtium replaces with its value when the console starts. Otherwise it says in one sentence what is wrong, such as `The login program login is not a full path. Type a path that starts with /, such as /bin/login.`

Under its settings, the card shows WHAT THIS TERMINAL DRAWS: the terminal's font set, then its look, then its displays, joined left to right.

A path's card in CONFIGURATION changes only the settings above. The card says how to change the rest: `Which displays and inputs this path joins change only when you revise it: press e.` In NEW PATH, the terminal's card also has the line `VTs`, where you choose `every VT` or one VT from `VT 1` to `VT 12`, and a line for each part of the path, where Enter disconnects that part.

#### Revising a path

Press `e` on a path in CONFIGURATION to revise it. Revise takes the path out of CONFIGURATION and puts it into NEW PATH with its parts picked, so that you can change it and add it back with `a`. A path you add back unchanged leaves the draft exactly as it was. To remove a path, revise it and press Esc. The status line then says `Took the path you were revising out of the configuration.`

#### Labels

Enter on a display or an input in PARTS opens its detail, whose first line is `Label`. A label is a name of your own for the part. A labelled part shows the label as its box's title, and the name the system gives the device under it. Clear the field to remove the label. vtium-config writes each label into the `[labels]` section of the draft.

#### Saving and installing

Each change you make goes into a set of unsaved changes, and the status line counts them, such as `draft north · 3 changes · 1 not saved · no problems`. The line under it says what the policy does with a keyboard that no rule names, such as `policy open: a keyboard that no rule names types into the active terminal`.

The tab bar shows the Flow view's actions beside the views' names: `v View file`, `s Save`, `i Install`, `r Revert` and `n New`. Each one acts on the whole draft.

- **View file**, on `v`, shows the draft's vtium.conf with the unsaved changes applied, which is what Install would write. It opens at the line of the highlighted box and marks each line the check complains about. Its keys are `s save  i install  j k ↑↓ lines  h l ←→ sideways  g G top bottom  ^D ^U half page  q back`.
- **Save**, on `s`, writes the unsaved changes into the draft. It asks nothing, because a draft can be unfinished.
- **Install**, on `i`, saves first and runs the check. When the check fails, Install refuses and lists what fails. Otherwise it asks `Install the draft? 3 lines added, 1 removed.` and installs as [above](#when-vtium-config-asks-for-sudo). When the only problem is that no complete route exists, it asks before it installs anyway. The TUI leaves its screen while sudo asks for your password, and while it enables vtium on the VTs your heads name and asks whether to restart vtium, as [How an install reaches the VTs](#how-an-install-reaches-the-vts) describes.
- **Revert**, on `r`, asks `Throw away the draft's changes? 3 lines added and 1 removed will be lost.` and then copies the installed file back over the draft.
- **Restore**, on `o`, lists the installed history. It copies the file you choose into the draft after asking, for example, `Replace the draft with earlier file 1? 2 lines added and 5 removed will be lost.`
- **New**, on `n`, asks `Start a new configuration? The draft's heads, rules and looks will be lost.` and then empties the draft to the short comment, `version = 1` and one look, `[look default]`, so that it sets no setting.

When you press `q` with changes that are not saved, the view asks whether to save them first, for example `2 changes are not saved. Save them before you quit? y saves them and quits, n quits without them, and Esc goes back.`

#### The keys

The key line shows the keys that fit what you are doing. With nothing open, it reads `↑↓←→ move  Space pick  Enter open  a add path  e revise  v view file  Esc clear  i install  o restore  n new  q quit`, with `p preview`, `t trial` and `PgUp PgDn scroll` where there is room.

| Key | What it does |
|---|---|
| the arrow keys | move the highlight from box to box |
| Home and End | go to the first and the last box |
| PgUp and PgDn | scroll by a window |
| Tab and Shift-Tab | move to the next view and to the one before |
| Space | picks the highlighted part into NEW PATH, or takes it out |
| Enter | opens the highlighted box: a template's part maker, a part's detail, or a terminal's card |
| `a` | adds the path in NEW PATH to CONFIGURATION |
| `e` | revises the highlighted path |
| Esc | clears NEW PATH, or closes what is open |
| `v`, `s`, `i`, `r`, `o`, `n` | View file, Save, Install, Revert, Restore and New |
| `d` | reads every device again |
| `p` | previews a complete path on the console. vtium does not have the console's control socket for this yet, so `p` says in one sentence why it cannot. |
| `t` | says whether you can try a font on this console. The Fonts view tries one with `t`. |
| `q` | leaves |

The mouse selects a box with a click and opens it with a second click.

The Looks view lists the console's sizes until vtium-config edits looks of their own. The Fonts view lists the frits and the built-in fonts, shows each as a picture of the console drawn in it, tries one on the live console with `t`, and makes and removes frits ([Choose a font](fonts.md#choosing-a-font-with-vtium-config)); a frit you make or remove there waits and goes in with the next Install. The Settings view lists every setting with its value, and Enter changes the highlighted one in the draft. The Help view lists the keys of each view and the commands.

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
