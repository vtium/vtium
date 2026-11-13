# The sections of vtium.conf

vtium's configuration file is `/etc/vtium/vtium.conf`. vtium's settings come first, at its top. Below them come sections that declare three things: vtium's heads, the look each head draws with, and the ordered rules that place each display and each input in a head. A head is a terminal for one or more displays, and [Terms](#terms) below defines it and the other words this page uses.

This page is the reference for those sections. It describes the terms they use, their syntax and their match keys. It also lists the routes by which one keyboard can reach another person's session, and what vtium does with sections it cannot fully use. [Configure vtium](configure.md) describes the whole file and how to change it. [heads.md](heads.md) gives complete sections for each common setup. [Heads](concepts/heads.md#two-policies-open-and-closed) explains the two policies, open and closed.

## Terms

- **Head**: a terminal for one or more displays. Each head has its own grid of character cells, its own look and its own consoles. A head with several displays shows one grid on all of them. The grid is sized for the smallest display and drawn larger on the bigger ones.
- **Console**: one session of a head, with its own login, screen and scrollback. A head shows one of its consoles at a time. A head's consoles live inside the one VT that vtium runs on. They are separate from the kernel's virtual consoles, the VTs.
- **Look**: what a head draws with, declared in one `[look NAME]` section. A look holds a font set, its size and its other font settings. The font set is a [frit](concepts/fonts.md#the-frit), vtium's font set file `/etc/vtium/fonts/NAME.vtfrit`, and the look's `font-name` names it. A head names its look with `look = NAME`. When a head names no look, it uses the look `default`. That is the `[look default]` section if the file has one. Otherwise it is the settings above the first section.
- **Display**: a monitor or panel, on one connector of a graphics card. A head is lit when at least one of its displays is on. The **boot display** is the display that is lit at start. If no display is lit, it is the first connected display on the boot graphics card. If there is none there, it is the first connected display on any card.
- **Builtin**: the configuration vtium runs when vtium.conf has no section, and in place of sections it cannot use. Builtin puts one head on every display, and makes every keyboard, pointer and key device roam. It is compiled into vtium, and [default.conf](heads/default.conf) writes it out. vtium itself never writes vtium.conf. vtium-config writes it only when you install a draft. See [Configure vtium](configure.md#change-it-with-vtium-config).
- **Policy**, `open` or `closed`: what the sections give to anything they do not mention. Under the open policy, an input that no rule places roams, and a head is isolated only where it says so. Under the closed policy, vtium ignores such an input, and every head is isolated unless it says otherwise. Under both policies, a display that no rule names is off.
- **Isolated**: a head with `isolated = yes`. On its keyboards, the VT keys reach its session as keys and never switch the VT. The rescue key never takes its keyboards away from it. An isolated head is never the active head, so no roaming keyboard reaches it.
- **A head's VTs**: the VTs that a head's `vt` key names, such as `vt = 12`. A head with no `vt` runs on every VT. One vtium runs on each VT that vtium is enabled on, and the vtium on a VT builds only the heads that run there. Take a display and a keyboard that the rules give to a head of other VTs. On any VT where that head does not run, they stay with that head. The display shows a notice that names the head and its VTs, such as `DP-4 is not configured for VT 2. It is configured for terminal terminal-1, on VT 12.` vtium ignores the keyboard there, so its keys reach no session.
- **Switch on Enter**: a head with `switch-on-enter = yes`. Suppose you are on a VT where the head does not run. When you press Enter on a keyboard that the rules give to the head, vtium switches to the first of the head's VTs on which vtium runs. While such a keyboard is connected, the notice on the head's displays adds a line that says so: `Press Enter on this keyboard to switch to VT 12.` When vtium runs on none of the head's VTs, Enter switches nothing. The line then says which VT to enable, and gives the command that enables it: `vtium is not running on VT 12. To enable it there, run sudo vtium console enable --direct 12.` The command names seat mode, `--seat`, when every VT that vtium is enabled on already runs it in seat mode. Otherwise it names direct mode. Any other key on that keyboard only wakes the notice. The exception is `Ctrl-Alt` and a function key, which switch the VT as they do on any keyboard of a head that is not isolated. The Enter switch works only under the open policy, and never on an isolated head. It switches only to VTs 1 to 12, the VTs that `Ctrl-Alt` and a function key reach. When `switch-on-enter` is unset, it is `no`, and the notice keeps its text.
- **Roaming keyboard**: a keyboard or mouse that no head owns. It types into the active head.
- **The active head**: the one head that roaming keyboards type into. It is never an isolated head. At start, the active head is the head that shows the boot display, if that head is not isolated. Otherwise it is the first head in the file that has a lit display and is not isolated. Only the roaming chord and the anti-lockout guard move it. While there is no active head, the first head that can be the active head becomes it as soon as it lights. The active head stays where it is when its display goes, so roaming keyboards can type into a head that is not lit. The on-screen display marks the active head for a few seconds after it moves.
- **On-screen display (OSD)**: vtium's own status at the top of each head. vtium draws it apart from the session's output. vtium.conf's `osd` chooses what it draws. With `minimal`, the default, it draws a dot in the top-right character cell, filled on the active head and hollow on the others. With `full`, it draws a line such as `● desk · console 2 of 3`, which gives the head's name and which of its consoles the head shows. With `off`, it draws nothing. The OSD shows on each head with a lit display when the active head moves. Under `full`, it also shows on a head that switches to another console, opens or closes one, or gets its display back. It stays for `osd-timeout` seconds, 3 by default, or for good with `never`. See [config.md](config.md). A notice of a problem, such as an invalid file, shows whatever `osd` says.
- **The head a keyboard asks**: the head under whose rights a keyboard's keys act. A keyboard that a rule gives to a head asks that head. A roaming keyboard asks the active head, so its VT keys switch the VT. When there is no active head, a roaming keyboard asks no head, and vtium delivers nothing from it, not even the VT keys.
- **The roaming chord**: the keys that make the next or the previous lit head the active head. Today these are `Ctrl-Super-Up` and `Ctrl-Super-Down`. Only roaming keyboards can use it.
- **Console chords**: the keys that act on a head's consoles. They act on the head that the keyboard asks. `Ctrl-Super-Right` shows the next console, and `Ctrl-Super-Left` shows the previous one. Under the open policy, `Ctrl-Super-Return` opens a new console just after the one shown, and shows it. `Ctrl-Super-Backspace` closes the console shown and shows the one after it. The programs in the closed console get a hang-up. On a head's last console, `Ctrl-Super-Backspace` does nothing. When a head is at its limit of consoles, nothing opens, and the top of the head says so for a few seconds, as in `No more consoles: desk has 4, its most.` The limits are given below. Under the closed policy, a head keeps the consoles it started with. vtium takes both keys and does nothing with them, so neither reaches the session as Return or Backspace.
- **The rescue key**: `Ctrl-Alt-F<n>` for the VT you're already on. vtium runs on the VT of its unit, `vtium@ttyN`, so on VT 2 the rescue key is `Ctrl-Alt-F2`. See [systemd.md](systemd.md) for the unit. Under the open policy, the rescue key makes the keyboard that pressed it roam for the rest of the boot. Under the closed policy, or on an isolated head, a press moves nothing. vtium then swallows the press, and it never reaches the session.
- **The anti-lockout guard**: under the open policy, while no keyboard reaches a lit head, vtium makes every keyboard roam and shows a notice. The keyboards roam until one of them reaches a lit head.
- **The display guard**: when the rules light no display at all, vtium lights the boot display anyway. Under the open policy, the boot display then shows the first head. Under the closed policy, it shows only a notice, and only for a file that declares heads.

## Reading the sections

The sections need two keys among vtium's settings above the first section: `version = 1` and, optionally, `policy`. The two can come in any order. Everything from the first section heading on is the sections. So a setting written below a heading belongs to that section.

There are three kinds of section, and no other:

- Each `[look NAME]` section declares a look. It takes the font keys of the top level, under the same names: `font-name`, which names the look's frit, `font-engine`, `font-size`, `font-hinting`, `font-line-spacing`, `display-depth`, `display-dpi`, `cell`, and `vtlook`. `cell` is the character cell's size in pixels, as `WIDTHxHEIGHT`. Within one look, `font-size` wins over `cell`.
- Each `[head NAME]` section declares a head. A head's keys are `look`, `consoles`, `login`, `when-display-lost`, `isolated`, `vt` and `switch-on-enter`.
- Under `[rules]`, each line is `display MATCH -> TARGET` or `input MATCH -> TARGET`. vtium checks each display and each input against the rules on its own, from the top. The first rule that matches places it in a head, makes it roam (`roaming`), ignores it (`ignore`) or turns it off (`off`).

A key set in a head's section wins over the same key in the head's look. A key set in a look wins over the same key at the top level.

A key in a section always takes the form `key = value`. The switches, written as a key alone or as `no-` and the key, belong to the top level.

`#` starts a comment at the start of a line or after whitespace. So the comments after values in the files below are comments. A `#` inside a word, as in `cmd --x#1`, is part of the value. `\#` gives a literal `#` after whitespace.

[heads/reference.conf](heads/reference.conf) is the annotated reference to every option of the sections. As it stands, it runs builtin. It explains each key, match key and target in a comment, and it gives the alternatives as examples that are ready to uncomment.

vtium delivers nothing from an input that a rule sends to `ignore`. Under the closed policy, vtium never opens such an input. Under the open policy, vtium opens an ignored keyboard only to watch for the rescue key, and it opens no other ignored input. For a display that a rule sends to `off`, vtium sets the connector off, so the display shows nothing.

A head's `when-display-lost` says what its sessions do when its last display goes:

- `keep`, the default, runs them on unseen. A display that the head's rules match brings them back.
- `hangup` hangs them up at once.
- `hangup DURATION`, such as `hangup 30s`, brings them back if a display returns within the time, and hangs them up after it. The duration takes an `s`, `m` or `h` suffix, or is plain seconds.

After a hang-up, a display that returns starts the head's consoles afresh, each at its login.

A match is `*` or one or more `key:value` tokens. `*` matches every display or every input. When a match has tokens, every one of them must hold. Keys that need udev's database match nothing on a system with no udev daemon, and `check-config` notes each rule that uses one.

| Key | Matches | Needs udev's database |
|---|---|---|
| `path:` | a USB port, by udev's `ID_PATH` up to the port | yes |
| `serial:` | one unit wherever it is plugged, `VENDOR:PRODUCT:SERIAL` | yes |
| `bluetooth:` | one Bluetooth device, by its address | no |
| `id:` | every unit of a model, `VENDOR:PRODUCT` | no |
| `name:` | a device by the name it reports, a glob | no |
| `class:` | a kind of input: `keyboard`, `key`, `mouse`, `touchpad`, `touchscreen`, `trackball`, `pointingstick`, `tablet`, `tablet-pad`, `joystick`, `accelerometer`, `switch` | no: vtium derives the class itself where no udev daemon runs |
| `edid:` | one monitor, `MAKER:PRODUCT:SERIAL` from its EDID | no |
| `connector:` | a port of a graphics card, `CARD/CONNECTOR` (`pci-0000:03:00.0/DP-1`), globs allowed | yes |
| `connector-path:` | one port of a DisplayPort multi-stream hub, by the connector's `PATH` (`mst:...`), globs allowed. The `PATH` stays the same across a replug or a resume, but its first number can change across a kernel or firmware update | no |
| `card:` | every output of a graphics card, by its `ID_PATH`, a glob | yes |
| `udev:` | any udev property of an input, `NAME=VALUE`, a glob on the value | yes |

A rule names a device most firmly by its USB port (`path:`), its unit (`serial:`) or its monitor (`edid:`). Naming a device by its model (`id:`) or its name (`name:`) is only a convenience.

A `path:` value matches exactly, or as a prefix that ends at a port. For example, `...usb-0:3.1` takes every interface of the device in port 3.1 (`...usb-0:3.1:1.0`). It also takes every device on a hub plugged into that port (`...usb-0:3.1.2:1.0`). It never takes port 3.10.

`serial:` binds the unit wherever it is plugged in.

A monitor's EDID is the identity data it reports. Two monitors with the same EDID both match one `edid:` rule, and they show the same head. Two monitors of one model that both report serial 0 are an example. Tell them apart with `connector:`, or with `card:` where they hang off different graphics cards.

`vtium config flow --json` lists the values for every device present. To find them by hand:

- `path:` is udev's `ID_PATH` up to the port. For example, `udevadm info -q property -n /dev/input/event5 | grep ID_PATH=` prints `ID_PATH=pci-0000:15:00.0-usb-0:3.3:1.0`, and the rule takes `path:pci-0000:15:00.0-usb-0:3.3`. `ls -l /dev/input/by-path/` maps the ports to event nodes.
- `edid:` is `MAKER:PRODUCT:SERIAL` from the monitor's EDID, which `/sys/class/drm/card*-*/edid` holds. A decoder such as `edid-decode /sys/class/drm/card1-DP-8/edid` prints the manufacturer, the product code and the serial.

A wrong value is not an error. For example, a `path:` of an empty port or an `edid:` of another monitor matches nothing. vtium then checks the device against the next rule, and when no rule matches, the file's policy decides. See [the two policies](concepts/heads.md#two-policies-open-and-closed). `check-config` notes that the rule matches no device present.

A misspelt key, such as `pth:` for `path:`, is an error, and vtium drops the rule. The file is then partly broken. The console shows `vtium.conf has errors: running the parts without errors.`, and `check-config` exits 1.

Either way, the device does not go where the rule meant to send it. In an open file, a keyboard roams. In a closed file, vtium ignores it. A partly broken open file also runs every head as not isolated. See [Invalid files](#invalid-files).

vtium reads vtium.conf only when root owns it and neither its group nor others can write it. The owner must be root. The group may be any group, but a file that its group can write fails the check, even when that group is root. `/etc/vtium` and each directory above it must pass the same check. vtium-config installs a file that passes. See [When vtium-config asks for sudo](configure.md#when-vtium-config-asks-for-sudo). To install a file by hand, run `install -o root -g root -m 0644 vtium.conf /etc/vtium/vtium.conf`. Where the directory is missing, first run `install -d -o root -g root -m 0755 /etc/vtium`.

`vtium check-config FILE` checks a candidate file before it is installed. It reports the file's owner and mode as a note, since installing the file makes it root's. With no FILE, it checks the installed file, its ownership included.

check-config exits 1 on an error or a warning about the file itself. These are:

- a line it cannot parse,
- a missing or unknown `version`,
- an unknown key, match key or value,
- a key, a head or a `[rules]` given twice,
- a target of the wrong kind,
- a rule naming no head,
- a rule that an earlier rule shadows,
- a look naming a missing frit,
- a head naming a look that the file does not declare,
- a `consoles` value past its cap, and
- an installed file that fails the ownership check.

Other findings are notes, and check-config exits 0 for them. A head that no rule names is a note. So is everything that depends on the devices present: a rule that matches none of them, a head that only one keyboard reaches, what the policy did with the devices that no rule names, and every right that a file takes away. A head whose `vt` names only VTs on which vtium is not enabled, at boot or when you switch there, is a note too. That note gives the command that enables vtium on one of them, such as `sudo vtium console enable --direct 12`. With `--no-devices`, check-config says that it did not check the heads' VTs.

Each head's consoles run the head's `login` command as root. When the head has no `login`, they run the `login` at vtium.conf's top level. A kiosk program drops its privileges itself, or is started through a tool that does. vtium never reads `kmscon`'s own file, `/etc/kmscon/kmscon.conf`.

A head starts with the consoles that its `consoles` key gives it, 1 when the key is unset. A head can have at most 4 consoles, and all heads together at most 8. vtium.conf's `head-consoles-max` and `consoles-max` raise these caps, to at most 50. The same caps bound the consoles that the open chord adds later. A value past the head's cap gives the cap, with a warning. When the heads ask for more than the total, each head keeps its first console, and vtium gives out the rest in the file's order. A file declares at most 16 heads, and no setting raises that limit.

The programs in a console can tell which console they run in. When a console's session starts, vtium sets two variables in its environment. `VTIUM_HEAD` holds the head's name, and `VTIUM_CONSOLE` holds the console's id.

A head gives its consoles ids from 1, in the order it opens them at its start. A console that the open chord adds takes the next id that the head has not given. No id is given twice before vtium restarts. A console keeps its id when its session starts again, after its program exits or after a hang-up.

Under the closed policy, a head's consoles never open or close while it runs. So the id is also the console's place among them, which the full line of the on-screen display shows. For example, the console whose id is 2 reads `console 2 of 3`. Under the open policy, the id and the place can differ once a console opens or closes. Take consoles 1, 2 and 3. Close console 2, and then open a console from console 1. The new console is console 4, and it reads `console 2 of 3`.

The variables are fixed when the session starts, so they cannot say which console the head shows now. A program that wants to know that turns on focus reporting (`printf '\033[?1004h'`). While a keyboard types into the head, vtium then sends the program a focus-in when the head shows its console, and a focus-out when the head switches to another.

The variables reach the shell only through a login command that keeps the environment it is given. vtium's default command, `/bin/login -p`, keeps them. So the shipped `vtium@.service` and a vtium.conf with no `login` give them to the shell. With vtium.conf's `reset-env` off, vtium runs `/bin/login` without `-p`, which clears the environment. A getty's login clears it too, and so does any head's `login` command that clears it. Such a command can still take the variables as words: vtium replaces a word of the command that is exactly `$VTIUM_HEAD` or `$VTIUM_CONSOLE` with its value.

## Which input can reach whose session

A keyboard can disturb another person's session by several routes. Each route has a switch of its own. Each route acts under the rights of the head that the keyboard asks. See [Terms](#terms). In an open file, a route that the file says nothing about is open, as on today's console. In a closed file, such a route is closed until the file opens it.

| Route | Disturbs | Governed by |
|---|---|---|
| The VT keys, `Ctrl-Alt-F<n>` and `XF86Switch_VT_<n>` | every display, which leaves for another VT | the `isolated` of the head the keyboard asks. A roaming keyboard asks no head when there is no active head |
| The console chords | the console the head shows | the head's `consoles`, the number of consoles the head starts with, which the chords switch among. Under the closed policy that number never changes, so with 1 there is nothing to switch. Under the open policy, `Ctrl-Super-Return` and `Ctrl-Super-Backspace` open and close consoles within the caps |
| The roaming chord | where the roaming keyboards type | taken only from roaming keyboards |
| Enter on a keyboard of a head on another VT | every display, which leaves for that head's VT | the head's `switch-on-enter`, off unless set, and its `isolated`. It works only under the open policy |
| The rescue key | takes a keyboard off its head, or makes an ignored one roam | the policy: on under open and off under closed. Under open, also the head's `isolated` |
| The anti-lockout guard | makes every keyboard roam | the policy: on under open, off under closed |
| Unclaimed keyboards | type into the active head | the policy. Under open they roam, unless a rule such as `input * -> ignore` closes them. Under closed, vtium ignores them |
| Arrival by roaming | the active head | the head's `isolated`: an isolated head is never the active head |

## Invalid files

vtium never fixes, renames or replaces the file. It resolves the file in three steps: trust, the policy and the rest. An error in a section never drops a setting above the first section. A mistake above the first section never drops a section. Above the first section, vtium reads its settings in the forgiving way that [config.md](config.md) describes: it reports an unknown key or a bad value, and goes on.

**1. Trust.** vtium never reads a file that fails the ownership check above, or that it cannot open. Builtin runs instead, with every setting at its default, and the console shows `vtium.conf not used: running the built-in configuration.` So a closed file that is left writable, or is deleted, opens the machine at the next boot or the next restart of vtium.

**2. The policy.** vtium reads `policy` from the settings above the first section. A line above the first section that begins with `policy`, in any mix of upper and lower case, is a policy line.

When vtium cannot read a policy line, the file is closed and broken. Nothing of it runs, no keyboard delivers anything, and the panic screen shows that line's error first. vtium cannot read a policy line that:

- holds a byte that is not UTF-8, or a control character,
- runs past 8192 bytes,
- is cut by the 65536-byte bound, or
- says anything but `policy = open` or `policy = closed`, such as `Policy = closed`, `policy = opne` or `policy` alone.

The 65536-byte bound is the most of the file that vtium reads. vtium judges a line that the bound cuts by the bytes before the bound. So that line is a policy line only when all six letters of `policy` come before the bound.

Two kinds of line are not policy lines:

- A line such as `policies = open` does not begin with `policy`, so it is not a policy line. vtium reads it as one of its own settings, warns that it is an unknown key, and ignores it.
- A line can show as a policy line but begin with a character that does not show on screen, such as a no-break space or a zero-width space. It is not a policy line either. vtium warns that it is an unknown key, and the warning shows that character escaped.

When every policy line is readable, a `policy = closed` line wins over any other. A second readable `policy` line is an error that drops that line alone. So is a `policy` line inside a section.

**3. The rest.** vtium drops only the part of the file that an error is in:

- An error inside a head's section drops that head.
- An error inside a look's section drops that look. A head that names the dropped look draws with the default look.
- A bad rule drops that rule alone.
- A missing or unknown `version` drops every section and keeps the policy. So does an error that leaves the structure of the sections untrustworthy.

If a head survives, vtium runs what survives and shows `vtium.conf has errors: running the parts without errors.` In an open file, vtium then adds builtin's input rules after the surviving rules. So a keyboard, pointer or key device that no surviving rule places roams, and vtium ignores every other input. vtium also runs every surviving head as not isolated, so that the rescue key and the active head always have somewhere to land. A closed file keeps its heads' isolation as written.

If nothing survives, an open file runs builtin and shows `vtium.conf not used: running the built-in configuration.` A closed file shows the panic screen.

The panic screen means that vtium has started correctly but can serve nothing. The boot display shows these lines:

- `vtium.conf has errors and counts as closed: all input is off.`
- a sentence saying that the file's errors leave no head that can run
- the first three errors, with their lines
- `More errors: N`, when there are more
- a last line that gives the next step

Where the screen has room, vtium also shows a drawing of Tux, the Linux penguin, panicked inside a computer monitor: to the left of the lines on a screen 80 columns wide, or a smaller drawing above them on a screen 40 columns wide. On a screen too small for the drawing, the lines show alone. The lines never make room for the drawing, and the drawing says nothing that the lines do not.

The screen stays for as long as vtium runs. `check-config` rebuilds the full report from the file itself. You can run it over ssh, or from a live USB with the disk mounted.

That failure is final by design. vtium keeps no copy of the file and no last-known-good version of it. So check a closed file before you rely on it, with `vtium check-config FILE` or `vtium config check FILE`. A trial that applies a candidate and reverts it unless you confirm it is not built yet. See [the issue that tracks it](https://github.com/vtium/vtium/issues/331).
