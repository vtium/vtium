# Configure vtium

This guide shows how to change the way vtium behaves. vtium reads one configuration file, `/etc/vtium/vtium.conf`. The file holds both vtium's settings and its heads. A head is a terminal that vtium shows on your displays. You change the file in one of two ways:

- with `vtium config`, which keeps your changes in a draft of your own and installs the draft when you are ready, or
- as root, with any editor.

[config.md](config.md) lists every setting with its default and its bounds. [The sections of vtium.conf](head-file.md) is the reference for the heads, looks and rules. [Set up heads](heads.md) gives a complete file for each common setup.

In the commands below, `#` marks a command that you run as root, for example with sudo. `$` marks a command that you run as your own user.

## The file

vtium reads `/etc/vtium/vtium.conf`. The package installs no such file, so vtium runs with its defaults until one is installed. The package does install `/etc/vtium/vtium.conf.example`. The example lists every setting at its default, commented out. After the settings, it holds the sections of the configuration that vtium runs when there is no file.

The file has two parts. The top part is a list of settings, one on each line. vtium takes these settings from `kmscon`'s configuration file, `kmscon.conf`, and adds settings of its own. A setting with a value is written `key=value`. A switch is written as the key alone to turn it on, or as the key with `no-` before it to turn it off. `#` starts a comment, except in a `#rrggbb` color. vtium reads a `kmscon.conf` copied to `/etc/vtium/vtium.conf` as it is. For example:

```
font-name=monoid
xkb-layout=de
dpms-timeout=300
bell
no-mouse
```

The sections come below the settings. Each section starts with a heading in brackets:

- A `[look NAME]` section says what a head renders with: its frit (a font set file), its size, and its other font settings.
- A `[head NAME]` section declares a head and names its look.
- One `[rules]` section lists the rules that send each display and each input device to a head.

A file with sections needs the line `version = 1` among the settings above them. A file with no section runs one head on every display. [The sections of vtium.conf](head-file.md) describes each section and its keys.

Every setting is also an option on vtium's command line, such as `--font-name=monoid` or `--no-mouse`. The command line wins over the settings at the top of the file. A look's or a head's own section wins over both. When you run vtium by hand, `--configdir=DIR` tells it to use a directory other than `/etc/vtium`.

vtium reads the file only when root owns it. Neither the file's group nor other users may be able to write to the file, or to any directory above it. When a file fails this check, vtium refuses it:

- vtium reads none of the file.
- vtium runs its built-in configuration.
- Every head shows `vtium.conf not used: running the built-in configuration.` with the next step, `To see why, run vtium check-config /etc/vtium/vtium.conf.`

`vtium check-config` names the file or directory that fails the check. It also gives the `chown` or `chmod` command that fixes it.

vtium reads the file once, when it starts. It reads the frits that the file names at the same time. So a change to the file reaches a VT only when vtium next starts on that VT. To apply a change, log out of the VT and restart its vtium, as [Upgrading and removing](install.md#upgrading-and-removing) shows. `vtium config install` offers to do that restart for you. See [How an install reaches the VTs](#how-an-install-reaches-the-vts) for details.

vtium reads no other configuration file. An earlier version of vtium may have left a `heads.conf` or a `seat0.seat.conf` in `/etc/vtium`. Such a file does nothing, and vtium says nothing about it. Move what it held into vtium.conf by hand.

## Check a file

`vtium check-config` reads the configuration the way vtium does when it starts. It applies nothing, and prints each problem it finds. When you give it a file, it checks that file in place of the installed one. So you can check a file before you install it:

```
$ vtium check-config
$ vtium check-config ./vtium.conf
```

`vtium check-config` exits 1 when it finds an error or a warning, and 0 when it finds none.

It also refuses a file that leaves nobody at the machine able to type. Such a file has no complete route. A complete route is two things together:

- a display that is connected, which the rules send to a head, and
- a keyboard that is connected and reaches that head, through an input rule or by roaming.

A keyboard that roams types into whichever head is on screen. When the check refuses a file, the message says what is missing, such as `No display present is sent to a head`.

Sometimes no display or no keyboard is connected, as in a container. `check-config` then judges the rules alone, and says so. `vtium check-config --no-devices FILE` asks for that reading on any machine. Use it for a file written for another machine, whose monitors and keyboards are not plugged in here. The configuration that vtium ships always passes the check.

## Change it with vtium config

`vtium config` runs `vtium-config`, vtium's configuration tool. It never edits `/etc/vtium/vtium.conf` while you work. Instead, it edits your draft, which is a whole vtium.conf of your own. It writes `/etc/vtium/vtium.conf` only when you install the draft. vtium reads only the installed file, so nothing you change in the draft reaches the console until you install it.

The tool and this guide use six words, each for one thing:

| Word | Meaning |
|---|---|
| draft | your working copy of vtium.conf, saved after every change you make |
| install | the one action that writes `/etc/vtium/vtium.conf`: `vtium config install`, or Install in the TUI |
| restore | copying a file that an install replaced back into your draft |
| tag | the mark Space puts on an item in the TUI's Flow view |
| path | a complete chain of tagged items, such as a keyboard, a head and a look |
| connect | what `c` does in the Flow view: it turns a path into changes in the draft |

In this guide, "the configuration" means the installed file and your draft together.

### The draft

The first command that needs a draft makes one, as a copy of the installed file. It names the draft after the machine's hostname. When that name is taken, it adds `-2`, `-3`, and so on, so that no draft is overwritten.

When no vtium.conf is installed, the draft starts as a short comment and nothing else. The comment says what the file is. It also says that `/etc/vtium/vtium.conf.example` and `vtium --help` describe every setting. Each change you make then adds only the lines it needs. A setting you change adds its own line, with no description above it. The first change to the looks, heads or rules adds three things: `version = 1`, the head `console`, and the rules that vtium runs when the file has no section. It adds them without the comments of the example file that vtium ships. A draft copied from an installed file keeps every line of that file, your own comments included.

Your drafts and everything else the tool keeps live under your home directory:

| Path | What it holds |
|---|---|
| `~/.config/vtium/drafts/NAME.conf` | each draft, a whole vtium.conf |
| `~/.config/vtium/vtium-config.conf` | the tool's own settings. `draft = NAME` names the draft you work on. `history = N` sets how many replaced files the installed history keeps, 10 unless you set it. |
| `~/.config/vtium/library.conf` | the library: looks, heads and rules you keep apart from any draft, in vtium.conf's section syntax |
| `~/.config/vtium/installed/DRAFT-N.conf` | the installed history: each file an install replaced, numbered for each draft |
| `~/.local/state/vtium/bases/NAME.base` | each draft's base: the SHA-256 of the installed file the draft was made from or last installed as |
| `~/.local/state/vtium/drafts/` | the install bundles that an install hands to its root step ([below](#when-vtium-config-asks-for-sudo)) |

`XDG_CONFIG_HOME` moves these files from `~/.config`, and `XDG_STATE_HOME` moves them from `~/.local/state`. The tool never keeps drafts in `/etc`. `/etc` holds only the installed file.

A draft may be unfinished. The tool writes every change you make to the draft, whatever the check says. So you can build a configuration in any order. After each change, the tool runs `vtium check-config` on the draft and prints what the check finds. It ends with a state line, such as:

```
draft north: 3 lines added, 1 removed · 1 problem
```

The state line says one of three things:

- `same as installed`,
- the lines that the draft adds and removes, compared with the installed file, or
- `made before the installed file changed`, when someone has changed the installed file since the draft was made from it.

Only an install needs the check to pass.

### The commands

These commands change the draft. `vtium config --help` lists every command, and `vtium config --help COMMAND` explains one.

| Command | What it does |
|---|---|
| `vtium config flow [--json]` | prints the draft's flow, from the displays to the input devices. It places the devices plugged in now on the flow, and numbers each rule. `--json` prints the flow for scripts. |
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

A rule's number N is its place in `[rules]`, counted from 1, as `flow` prints it.

Without `--before`, `rule add` puts the new rule where a wider rule does not hide it. Each rule is of one kind, `display` or `input`, which is its first word. A rule names either one device or a group of devices. It names one device by `edid`, `connector`, `connector-path`, `serial`, `path`, `bluetooth` or `name`. It names a group of devices by `class`, `id`, `card`, `udev` or `*`. `rule add` places the new rule this way:

- A rule that names one device goes before the first rule of the same kind that names a group of devices.
- A rule that names a group goes before the first `*` rule of the same kind.
- Any other rule goes last.

`head rm` removes a head but keeps the rules that send to it, and names those rules. The check lists them until you point each one at another head or remove it. `look rm` refuses to remove a look that a head uses, and names those heads. With `--force`, it moves those heads to the default look instead. `--name` on `head set` and `look set` renames the section, and every rule or head that names it.

Each command that changes the draft also takes two options:

- `--dry-run` prints the change as a difference and what the check finds, and writes nothing.
- `--install` installs the draft after the change.

A command asks for no confirmation, because running it is the confirmation. For example, these commands give a head named `console` a larger look:

```
$ vtium config look add big --frit monoid --size 14
$ vtium config head set console --look big
$ vtium config diff
$ vtium config install
```

`vtium config edit` copies the draft to a file under `~/.local/state/vtium/`, and opens the copy in your editor. It uses the editor that `$VISUAL` names. When `$VISUAL` is not set, it uses `$EDITOR`, and when neither is set, it uses `vi`. When the editor exits, your edited copy becomes the draft. If the check then finds an error or a warning, `vtium config edit` prints each one with its line, and asks whether to edit again. With `--install`, it installs the draft afterwards, but only when the check passes.

A draft is a complete vtium.conf. So you can build a configuration on one machine and use it on another. For example, the other machine may be a small board whose console is too narrow for the TUI, vtium-config's full-screen view. To use the draft there:

1. Copy the draft into `~/.config/vtium/drafts/` on the other machine.
2. Make it the current draft with `vtium config draft switch NAME`.
3. Install it there.

On the machine that built the draft, `vtium check-config --no-devices` checks the draft by its rules alone.

The commands exit with a fixed status, so a script can tell the outcomes apart:

| Exit status | Meaning |
|---|---|
| 0 | done: the draft is written, whatever the check finds; or the flow or the difference is printed; or the draft is installed |
| 1 | the check failed at an install, and nothing is installed; the draft keeps every change |
| 2 | the command could not be used: an unknown option, a rule, head or look that does not exist, or a value its key cannot take |
| 3 | the installed vtium.conf changed since the draft was made from it, and nothing is installed |
| 4 | sudo refused or is missing, and nothing is installed |
| 5 | a file could not be read or written, or the console that checks the draft could not be started; the message names the path |

An install stops with status 3 when someone changed `/etc/vtium/vtium.conf` after your draft was made from it. For example, someone may have edited the file as root with an editor. `vtium config diff` shows what differs. Then you have two choices. `vtium config revert` starts the draft again from the installed file. Or you can copy your changes across by hand.

### After an install

An install keeps the file it replaced in the installed history, in `~/.config/vtium/installed/`. The history keeps as many files as the `history` setting says. See [The draft](#the-draft) for that setting. `vtium config history` lists the files, newest first. `vtium config restore N` copies file N into the draft. You can then look at it and install it as usual. Only an install writes `/etc/vtium/vtium.conf`. A restore changes only the draft. The installed file takes effect when vtium next starts on each VT, as [The file](#the-file) says.

## When vtium-config asks for sudo

Everything `vtium config` does runs as you, apart from one step of an install. It reads the installed file, the frits, and the devices plugged in, without any special privilege. It keeps your drafts in your home directory. Only an install asks for sudo, because only root may write to `/etc/vtium`.

An install takes three steps:

1. **As you,** vtium-config first checks that the installed file has not changed since the draft was made from it. If it has changed, vtium-config refuses the draft with exit status 3, and asks nothing. It then checks the draft with `vtium check-config`. It refuses a draft that fails the check, with exit status 1. Last, it builds an install bundle in a new directory under `~/.local/state/vtium/drafts/`, named for the time. The bundle holds the new vtium.conf, and any frits that the TUI's Fonts view has waiting to install. It also holds a list of each file that the bundle replaces. The list gives the SHA-256 of each of those files as it is when the install starts.
2. **As root,** one command runs: `sudo /usr/bin/vtium-config apply DIR`, where DIR is the bundle's directory. sudo asks for your password here. The command takes no option that names a path or a program, so a sudo rule can allow exactly that command. The command reads the bundle without following links. It refuses the bundle, and writes nothing, when a file it would replace has changed since the install started. It checks every file before it writes any. It checks each frit with vtium's frit reader, and the new vtium.conf with the console's own `vtium check-config`. It then writes the frits first, and vtium.conf after them, so that vtium.conf never names a frit that is not there yet. It writes each new file beside the old one, with mode 0644, and renames it into place. It keeps the old file as `NAME.bak`, such as `vtium.conf.bak`. This step is one short process. It takes about 20 ms on the maintainer's machine, check included. A large vtium.conf or many frits take longer.
3. **As you again,** vtium-config saves the file that the install replaced in your installed history. It records the new installed file as the draft's base. The draft and the installed file are then the same.

When sudo refuses, or is not installed, nothing is installed and the command exits with status 4. Run `vtium config install` again. The bundle stays on disk. You could run `sudo vtium-config apply DIR` on it by hand, but that skips the third step. Nothing is then saved in the installed history. The draft's base stays the old installed file, so the next `vtium config install` stops with exit status 3.

Root needs no sudo. When root runs `vtium config`, it keeps root's own drafts under root's home directory, which is `/root/.config/vtium/drafts/` by default. vtium-config finds root's home directory from the password database. It never takes it from `$HOME`, or from the XDG variables that sudo can carry over from your account. An install as root runs the same checks, and writes the files the same way at once. The TUI does not run as root. Instead, it tells you to run `vtium config` as yourself, since it asks for sudo when it installs.

## How an install reaches the VTs

vtium reads vtium.conf only when it starts on a VT. So an install changes nothing on your screens until vtium starts again. A head with a `vt` line also appears only on the VTs that the line names, and only where vtium runs. For example, a head with `vt = 12` needs vtium running on VT 12. But logind, the systemd service that manages logins, starts vtium by itself only on VTs 1 to 6.

So after `vtium config install` or the TUI's Install writes `/etc/vtium/vtium.conf`, vtium-config takes two more steps:

1. **It enables vtium on the VTs your heads name.** vtium-config runs `vtium console status` to see where vtium is enabled. Some VTs that a head's `vt` line names may not have vtium enabled. For each of those VTs, vtium-config runs `sudo vtium console enable --direct N`. It uses seat mode, `--seat N`, instead, when the VTs where vtium is already enabled use seat mode. When they use both modes, or vtium is not enabled on any VT yet, it uses direct mode and says why. See [Direct mode and seat mode](install.md#direct-mode-and-seat-mode) for what the two modes mean. `vtium console` decides whether vtium can have the VT, and vtium-config shows what `vtium console` prints. For example, `vtium console` refuses a VT that a display manager or a graphical session holds. The install stays in place after a refusal. But that head does not appear until the VT is free and you run the `vtium console enable` command that vtium-config shows.
2. **It offers to start and restart vtium.** vtium-config asks, for example, `Start vtium on VT 12, and restart it on VTs 2 and 3? [y/N]`. When you answer y, it runs `sudo systemctl start vtium@tty12` and `sudo systemctl restart vtium@tty2 vtium@tty3`. The new file then applies at once. A restart keeps each session on those VTs running, because systemd holds the session's terminal open while vtium starts again. vtium clears the screen and draws it again, and the scrollback is lost. Starting vtium on a VT where a login prompt runs stops that login prompt, and the question says so. When you answer anything else, vtium-config starts and restarts nothing, and prints the commands to run later. The new file then applies on each VT when vtium next starts there, for example at the next boot.

`vtium config install` asks this question only when it runs in a terminal. Without a terminal, it asks nothing and prints the commands instead. The `--install` option of the other commands also asks nothing and prints the commands. The TUI asks on the terminal that it hands over to sudo. Its message line then says where vtium runs with the new file. An install into another directory, named with `--configdir`, takes neither step, because the vtium on each VT reads only `/etc/vtium`.

## The TUI

`vtium config tui` opens the TUI, vtium-config's full-screen view of the configuration, in the terminal you run it in. It needs a terminal of at least 80×25 character cells. In a smaller terminal, it shows only the size it needs and the commands. The commands work at any size. The TUI has five views: Flow, Looks, Fonts, Settings, and Help. It opens on Flow. Tab moves to the next view and Shift-Tab to the one before, wherever the selection is. The arrow keys move inside a view. While a card, a list, or a value you are typing is open, Tab does nothing. Press Esc to close it first. While a question waits for your answer, Tab answers no.

### The Flow view

The Flow view shows the draft as paths. A path is how vtium joins things when it runs. One or more displays show a terminal. The terminal renders with a font set. Inputs, such as keyboards and mice, type into the terminal. In vtium.conf, a path is one head with its rules.

The view is one document with three sections. From the top, they are CONFIGURATION, NEW PATH and PARTS. You build a path from the bottom up. You pick parts in PARTS into NEW PATH, and then you add the finished path to CONFIGURATION. The document scrolls as a whole under the tab bar, the status line and the key line. It always keeps the highlighted box in sight.

- **CONFIGURATION** shows the paths the draft holds, one row each, under the column titles DISPLAYS, TERMINALS and INPUTS. Each row reads displays → terminal ← inputs. A terminal's box shows the terminal's name. Under the name, it shows the font set and the VTs the terminal runs on, such as `font set proof · VT 3` or `default font set · every VT`.
- **NEW PATH** shows the path you are building, with dashed borders. An empty place in it reads `Choose… [Enter]`. The line beside the section's title says what the path still needs.
- **PARTS** holds the parts you can pick, in four groups: DISPLAYS, TERMINALS, INPUTS and FONT SETS. vtium-config makes a part for each display, keyboard and pointer that is connected. It also makes a part for each device that the draft's rules name, and for each special part the draft uses.

Two special terminals stand for something other than a head: `Active terminal` and `Nowhere`.

Inputs that roam type into whichever terminal is on screen. CONFIGURATION shows them once, in the row ROAMING. That row's terminal is the dotted box `Active terminal`, next to the sentence `These inputs type into the active terminal.`

A path to `Nowhere` holds the displays that vtium leaves off and the inputs that it ignores.

The view follows the devices while it is open. A device you plug in adds a part. When you unplug a device, its part turns grey and reads `Not connected`. The device stays in the configuration until you take its part out of its path. Press `d` to read every device again.

The box's title color shows the kind of part: displays are cyan, terminals yellow, inputs green and font sets magenta. The border shows the part's state:

- An amber border marks a connected device that is in no path yet.
- A grey border marks a part you cannot pick.
- A check mark `✓` marks a part you picked into NEW PATH.

With `NO_COLOR` set, or in a terminal without color, the view draws the same text and borders without color. You can choose the colors in `$XDG_CONFIG_HOME/vtium/vtium-config.conf`.

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

The input maker lists the choices from `USB port` on under the heading `OR CHOOSE BY`. Each of these choices opens a picker. So does each display choice after `All displays`. The picker lists each value that the connected devices give, with the devices that each value matches now. Its last line is `Other…`, where you type a value yourself. vtium-config checks a typed value the way vtium reads a rule. When it cannot use the value, it says so in one sentence, such as `This USB port is not a valid port path. A port path looks like pci-0000:00:14.0-usb-0:3.4.`

A choice makes a part, which joins its group in PARTS. The status line then says, for example, `Added DP-9. Press Space to put it in the new path.` When you opened the part maker from NEW PATH, the new part also goes into the new path.

#### Building and adding a path

Space on a part picks it into NEW PATH, and Space on it again takes it out. While you pick, the line beside NEW PATH says what the path still needs, such as `Pick an input and a display to make a path.` or `Add a display to finish this path.` What a complete path needs depends on its terminal:

- A path to a terminal of its own is complete once it has its terminal and at least one display.
- A path to `Active terminal` needs at least one input and no display.
- A path to `Nowhere` needs at least one display or input.

A complete path reads `This path is complete. Press a to add it.`

Press `a` to add the path to CONFIGURATION. vtium-config checks the path first. When something conflicts, the path stays in NEW PATH and one sentence says what conflicts. For example, vtium-config refuses a name that another terminal has.

A display or an input can be in one path for each VT. So two paths can share a display when their terminals run on different VTs. For example, the terminal desk on VT 3 and the terminal panel on VT 4 can both show `All displays`. Each VT then shows its own terminal. Sometimes you pick a display that a path on the same VT already holds. vtium-config then refuses it and names that VT: `The display All displays is already in the terminal desk on VT 3. A display can be in one path for each VT.` An input that you put in two paths on the same VT roams instead. Adding the path moves that input to the ROAMING row. The status line says so when you pick the input.

#### A terminal's card

Enter on a terminal's box opens its card. The card lists the terminal's settings: `Name`, `Font set`, `Size`, `Consoles`, `Login`, `When its screen is unplugged` and `Switch on Enter`. Up and Down move between the lines. Left and Right move between a line's choices. Enter sets the choice. On `Name`, Enter lets you type a new name.

The `Login` line sets the program that each console of the terminal starts. It has two choices:

- `Default: /bin/login -p` leaves the terminal's own `login` key unset. The choice shows the command that runs then: the `login` key at the top of vtium.conf, or vtium's default, `/bin/login -p`.
- `Other…` lets you type a command. vtium-config keeps the command only when its first word is one of two things. It can be the full path of a program that exists and can run. Or it can be a variable such as `$SHELL`. vtium replaces such a variable with its value when the console starts. Otherwise vtium-config says in one sentence what is wrong, such as `The login program login is not a full path. Type a path that starts with /, such as /bin/login.`

The `Switch on Enter` line sets the terminal's `switch-on-enter` key. With `on`, you can press Enter on one of the terminal's own keyboards while another VT shows, and vtium switches to the terminal's VT. With `off`, the default, vtium-config takes the key's line out of the draft. The switch has no effect in four cases:

- under `policy = closed`, the strict policy, which ignores any keyboard that no rule names,
- on an isolated terminal, whose keyboards never switch the VT,
- on a terminal that runs on every VT, and
- on a terminal whose VTs are all above 12.

While the cursor is on the line, the card says when one of these cases applies, such as `It has no effect now, because this terminal runs on every VT. Press e and choose a VT on the line VTs.` See [The sections of vtium.conf](head-file.md) for details.

Under its settings, the card shows WHAT THIS TERMINAL DRAWS: the terminal's font set, then its look, then its displays, joined left to right.

A path's card in CONFIGURATION changes only the settings above. The card says how to change the rest: `Which displays and inputs this path joins change only when you revise it: press e.` In NEW PATH, the terminal's card has two more kinds of line. The line `VTs` is where you choose `every VT`, or one VT from `VT 1` to `VT 12`. Each part of the path also has a line, where Enter disconnects that part.

#### Revising a path

Press `e` on a path in CONFIGURATION to revise it. Revising takes the path out of CONFIGURATION and puts it into NEW PATH, with its parts picked. You can then change it and add it back with `a`. A path you add back unchanged leaves the draft exactly as it was. To remove a path, revise it and press Esc. The status line then says `Took the path you were revising out of the configuration.`

#### Labels

Enter on a display or an input in PARTS opens its detail, whose first line is `Label`. A label is a name of your own for the part. A labelled part shows the label as its box's title. Under the label, it shows the name that the system gives the device. Clear the field to remove the label. vtium-config writes each label into the `[labels]` section of the draft.

#### Saving and installing

Each change you make goes into a set of unsaved changes. The status line counts them, such as `draft north · 3 changes · 1 not saved · no problems`. The line under it says what the policy does with a keyboard that no rule names, such as `policy open: a keyboard that no rule names types into the active terminal`.

The tab bar shows the Flow view's actions beside the views' names: `v View file`, `s Save`, `i Install`, `r Revert` and `n New`. Each one acts on the whole draft.

- **View file**, on `v`, shows the draft's vtium.conf with the unsaved changes applied. This is what Install would write. It opens at the line of the highlighted box, and marks each line that the check complains about. Its keys are `s save  i install  j k ↑↓ lines  h l ←→ sideways  g G top bottom  ^D ^U half page  q back`.
- **Save**, on `s`, writes the unsaved changes into the draft. It asks nothing, because a draft can be unfinished.
- **Install**, on `i`, saves first and runs the check. When the check fails, Install refuses and lists what fails. Otherwise it asks `Install the draft? 3 lines added, 1 removed.` and installs as [above](#when-vtium-config-asks-for-sudo). When the only problem is that no complete route exists, it asks before it installs anyway. The TUI leaves its screen while sudo asks for your password. It also leaves its screen while it enables vtium on the VTs your heads name and asks whether to restart vtium. [How an install reaches the VTs](#how-an-install-reaches-the-vts) describes those steps.
- **Revert**, on `r`, asks `Throw away the draft's changes? 3 lines added and 1 removed will be lost.` and then copies the installed file back over the draft.
- **Restore**, on `o`, lists the installed history. It copies the file you choose into the draft after asking, for example, `Replace the draft with earlier file 1? 2 lines added and 5 removed will be lost.`
- **New**, on `n`, asks `Start a new configuration? The draft's heads, rules and looks will be lost.` It then empties the draft to the short comment, `version = 1` and one look, `[look default]`, so that it sets no setting.

When you press `q` with changes that are not saved, the view asks whether to save them first, for example `2 changes are not saved. Save them before you quit? y saves them and quits, n quits without them, and Esc goes back.`

#### The keys

The key line shows the keys that fit what you are doing. With nothing open, it reads `↑↓←→ move  Space pick  Enter open  a add path  e revise  v view file  Esc clear  i install  o restore  n new  q quit`. Where there is room, it adds `p preview`, `t trial` and `PgUp PgDn scroll`.

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

The Looks view lists the console's sizes until vtium-config edits looks of their own.

The Fonts view lists the frits and the built-in fonts. It shows each one as a picture of the console rendered in that font. With `t`, it tries one on the live console. It also makes and removes frits. A frit that you make or remove there waits, and goes in with the next Install. See [Choose a font](fonts.md#choosing-a-font-with-vtium-config) for details.

The Settings view lists every setting with its value. Enter changes the highlighted setting in the draft. The Help view lists the keys of each view, and the commands.

## Edit the file as root

Root can still edit `/etc/vtium/vtium.conf` with any editor. Start from the example, check your file, then install it owned by root:

```
# cp /etc/vtium/vtium.conf.example /etc/vtium/vtium.conf
# vtium check-config /etc/vtium/vtium.conf
```

A draft that was made before such an edit shows `made before the installed file changed`. Its install stops with exit status 3 until you revert the draft, or carry the change across by hand.

## Blanking from a script

`vtium blank --vt N` turns off the displays of the vtium on VT N at once. `vtium wake --vt N` turns them on again, as a key press does. Root and the user that vtium runs as may send these commands:

```
# vtium blank --vt 3
```
