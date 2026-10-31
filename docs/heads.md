# Heads, by scenario

This page shows the head file, `/etc/vtium/heads.conf`, in the setups it is made for: each scenario's purpose, a complete head file, and what to expect, on an unplug and of what stays shared. [ADR-1401](architecture/platform/ADR-1401-heads-a-terminal-per-physical-display-placed-by-a-first-match-ruleset.md) holds every rule; this page applies them. Each scenario's file is also a file of its own under [heads/](heads/), and the `edid:` and `path:` values in them are illustrative.

**What today's build does.** Today's build reads an earlier form of the head file and rejects every file on this page whole: `vtium check-config FILE` reports each new name as an error, and a boot with one installed shows the errors on the console and runs builtin, every display showing one head and every keyboard typing into it. Do not install kiosk.conf, kiosk-admin.conf, stations.conf or off.conf until vtium reads `policy`: installed today, they leave the machine open to every keyboard. What works today is a machine with no head file, which runs builtin, and, in the earlier form, heads, rules, the match keys, the guards and a head's `profile` and `login`. Not built yet: `policy`, `isolated`, `when-display-lost`, the targets `roaming` and `off`, `consoles` as a fixed count, `path:` matching a hub on its port, `connector-path:`, a closed file's wait for a display, the handling of a broken file under [Invalid files](#invalid-files), and `vtiumctl`. The check that each block on this page equals its file and that `vtium check-config` accepts each file comes with the change that reads the new names.

## Terms

- **Head**: a terminal for one or more displays, with its own grid, its own profile and its own consoles. A head with several displays shows one grid on all of them, sized for the smallest and drawn larger on the bigger ones.
- **Console**: one session of a head, with its own login, screen and scrollback; a head shows one of its consoles at a time. A head's consoles live inside the one VT vtium runs on, apart from the kernel's virtual consoles, the VTs.
- **Profile**: how a head looks, its font included ([ADR-1200](architecture/font/ADR-1200-glyphs-are-generated-at-run-time-in-a-sandboxed-helper-from-a-profile-s-frit.md)). Until a profile file format exists, a head's `profile` names a [frit](concepts.md#the-frit), vtium's font file, `/etc/vtium/fonts/NAME.vtfrit`, and `default`, the value when unset, is the compiled-in look.
- **Display**: a monitor or panel, one connector of a graphics card. A head is lit when at least one of its displays is on. The **boot display** is the display lit at start, else the first connected display on the boot graphics card, else the first connected display on any card.
- **Builtin**: the configuration vtium runs with no head file, and in place of a file it cannot use: one head on every display, with every keyboard, pointer and key device roaming. It is compiled in, ADR-1401 calls it the shipped rules, and [default.conf](heads/default.conf) writes it out. vtium never writes a head file.
- **Policy**, `open` or `closed`: what the file gives anything it does not mention. Open lets an input no rule places roam, and isolates a head only where it says so; closed ignores such an input, and isolates every head that does not say otherwise. Under both, a display no rule names is off.
- **Isolated**: a head with `isolated = yes`. Its keyboards' VT keys reach its session as keys and never switch the VT, the rescue key never takes its keyboards away, and it is never the active head, so no roaming keyboard reaches it.
- **Roaming keyboard**: a keyboard, or mouse, that no head owns. It types into the active head.
- **The active head**: the one head roaming keyboards type into, never an isolated head. At start it is the head showing the boot display, if that head is not isolated; otherwise it is the first head in the file that has a lit display and is not isolated. It moves only by the roaming chord or the anti-lockout guard; while there is none, the first head that can be the active head becomes it as soon as it lights. It stays where it is when its display goes, so roaming keyboards can type into an unlit head. The on-screen display marks it for a few seconds after it moves.
- **On-screen display (OSD)**: vtium's own status at the top of each head, drawn outside the session's output. kmscon.conf's `osd` chooses what it draws: with `minimal`, the default, a dot in the top-right cell, filled on the active head and hollow on the others; with `full`, a line such as `● desk · console 2 of 3`, the head's name and which of its consoles it shows; with `off`, nothing. It shows on each head with a lit display when the active head moves, and under `full` also on a head that switches to another console, gains or loses one, or gets its display back. It stays for `osd-timeout` seconds, 3 by default, or for good with `never` ([config.md](config.md)). A notice of a problem, such as an invalid file, shows whatever `osd` says.
- **The head a keyboard asks**: the head under whose rights a keyboard's keys act. A keyboard a rule gives a head asks that head; a roaming keyboard asks the active head, so its VT keys switch the VT; and with no active head a roaming keyboard asks none and delivers nothing, the VT keys included.
- **The roaming chord**: the keys that make the next or previous lit head the active head, today `Ctrl-Super-Up` and `Ctrl-Super-Down`. Only roaming keyboards can use it.
- **Console chords**: the keys that switch among a head's consoles, today `Ctrl-Super-Right` for the next and `Ctrl-Super-Left` for the previous. They act on the head the keyboard asks. A head has the consoles its `consoles` gives it, and the chords never open or close one; today's build also opens one with `Ctrl-Super-Return` and closes one with `Ctrl-Super-Backspace`, which go when `consoles` becomes a fixed count.
- **The rescue key**: `Ctrl-Alt-F<n>` for the VT you're already on. vtium runs on the VT of its unit, `kmsconvt@ttyN` ([systemd.md](systemd.md)), so on VT 2 the rescue key is `Ctrl-Alt-F2`. Under the open policy it makes the keyboard that pressed it roam for the rest of the boot. A press that moves nothing, under the closed policy or on an isolated head, is swallowed and never reaches the session.
- **The anti-lockout guard**: under the open policy, while no keyboard reaches a lit head, every keyboard roams, with a notice, until one does.
- **The display guard**: when the rules light no display at all, vtium lights the boot display anyway. Under open it shows the first head there. Under closed it shows only a notice there, and only for a file with heads.

## Reading a head file

A head file starts with its top-level keys, in any order: `version = 1` and, optionally, `policy`. Each `[head NAME]` section declares a head. Under `[rules]`, each line is `display MATCH -> TARGET` or `input MATCH -> TARGET`; each display and each input walks the rules from the top on its own, and the first that matches places it in a head, roaming (`roaming`), ignored (`ignore`) or off (`off`). A head's keys are `profile`, `consoles`, `login`, `when-display-lost` and `isolated`; there is no other section. `#` starts a comment at the start of a line or after whitespace, so the comments after values in the files below are comments; a `#` inside a word, as in `cmd --x#1`, is part of the value, and `\#` gives a literal `#` after whitespace.

[heads/reference.conf](heads/reference.conf) is the annotated reference to every option: it runs builtin as it stands, explains each key, match key and target in a comment, and gives the alternatives as examples ready to uncomment.

An input sent to `ignore` delivers nothing. Under closed vtium never opens it; under open vtium opens a keyboard only to watch for the rescue key, and opens no other ignored input. A display sent to `off` has its connector set off, so it shows nothing.

A head's `when-display-lost` says what its sessions do when its last display goes: `keep`, the default, runs them on unseen and brings them back with a display the head's rules match; `hangup` hangs them up at once; and `hangup DURATION`, such as `hangup 30s`, with an `s`, `m` or `h` suffix or plain seconds, brings them back if a display returns within the time and hangs them up after it. After a hang-up, a display returning starts the head's consoles afresh, each at its login.

A match is `*`, every display or every input, or one or more `key:value` tokens, every one of which must hold. Keys that need udev's database match nothing on a system with no udev daemon, and `check-config` notes each rule that uses one.

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
| `connector-path:` | one port of a DisplayPort multi-stream hub, by the connector's `PATH` (`mst:...`) | no |
| `card:` | every output of a graphics card, by its `ID_PATH`, a glob | yes |
| `udev:` | any udev property of an input, `NAME=VALUE`, a glob on the value | yes |

A rule names a device most strongly by its USB port (`path:`), its unit (`serial:`) or its monitor (`edid:`), and only for convenience by its model (`id:`) or its name (`name:`). A `path:` value matches exactly or as a prefix that ends at a port: `...usb-0:3.1` takes every interface of the device in port 3.1 (`...usb-0:3.1:1.0`) and every device on a hub plugged into it (`...usb-0:3.1.2:1.0`), and never port 3.10. `serial:` binds the unit wherever it is plugged. Two monitors with the same EDID, such as two of one model that report serial 0, both match one `edid:` rule and show the same head; tell them apart with `connector:`, or with `card:` where they hang off different graphics cards. Until `vtiumctl detect` lists the values, find them by hand:

- `path:` is udev's `ID_PATH` up to the port: `udevadm info -q property -n /dev/input/event5 | grep ID_PATH=` prints `ID_PATH=pci-0000:15:00.0-usb-0:3.3:1.0`, and the rule takes `path:pci-0000:15:00.0-usb-0:3.3`. `ls -l /dev/input/by-path/` maps the ports to event nodes.
- `edid:` is `MAKER:PRODUCT:SERIAL` from the monitor's EDID, which `/sys/class/drm/card*-*/edid` holds; a decoder such as `edid-decode /sys/class/drm/card1-DP-8/edid` prints the manufacturer, the product code and the serial.

A wrong value is no error: a `path:` of an empty port or an `edid:` of another monitor matches nothing, the device walks on to the next rule and then to the file's policy ([below](#two-policies-open-and-closed)), and `check-config` notes that the rule matches no device present. A misspelt key, such as `pth:` for `path:`, is an error that drops the rule: the file is partly broken, the console shows `config partly invalid, running what's valid`, and `check-config` exits 1. Either way the device goes where the rule did not send it: in an open file a keyboard roams, and in a closed one it is ignored. A partly broken open file also runs every head as not isolated ([Invalid files](#invalid-files)).

vtium reads the file only when root owns it and neither its group nor others can write it: the owner must be root, the group may be any group, and a group-writable file fails even when its group is root. `/etc/vtium` and each directory above it pass the same check. Install a file with `install -o root -g root -m 0644 kiosk.conf /etc/vtium/heads.conf`, after `install -d -o root -g root -m 0755 /etc/vtium` where the directory is missing.

`vtium check-config FILE` checks a candidate before it is installed, and reports its owner and mode as a note, since installing it makes it root's; with no FILE it checks the installed file, its ownership included. It exits 1 on an error or a warning about the file itself: a line it cannot parse, a missing or unknown `version`, an unknown key, match key or value, a key, a head or a `[rules]` given twice, a target of the wrong kind, a rule naming no head, a rule an earlier one shadows, a profile naming a missing frit, a `consoles` value past its cap, and an installed file that fails the ownership check. A head no rule names is a note, and so is everything that depends on the devices present, with exit status 0: a rule matching none of them, a head that only one keyboard reaches, what the policy did with the devices no rule names, and every right a file takes away.

Each head's consoles run its `login` command, else that of kmscon.conf, as root, as kmscon.conf's `login` does; a kiosk program drops its privileges itself, or is started through a tool that does. kmscon.conf, `/etc/kmscon/kmscon.conf`, is vtium's main configuration file, listed in [config.md](config.md), and keeps the C reference's name and place. A head has exactly the consoles its `consoles` gives it, 1 when unset, at most 4 a head and 8 for all heads together; kmscon.conf's `head-consoles-max` and `consoles-max` raise these, to at most 50. A value past the head's cap gives the cap, with a warning, and where the heads ask for more than the total, each keeps its first console and the rest are given in the file's order. A file declares at most 16 heads, a limit no setting raises.

The programs in a console can tell which console they run in. When a console's session starts, vtium sets two variables in its environment: `VTIUM_HEAD`, the head's name, and `VTIUM_CONSOLE`, the console's id. A head gives its consoles ids from 1 in the order it opens them and never reuses one, so a console keeps its id when another one closes. The id is not the console's place among the head's consoles, which the on-screen display shows: after console 1 closes, the console whose id is 2 reads `console 1 of 1` on the display and still has `VTIUM_CONSOLE=2`. The variables are fixed when the session starts, so they cannot say which console the head shows now. They reach the shell only through a login command that keeps the environment it is given. vtium's default command, `/bin/login -p`, keeps them, so the shipped `kmsconvt@.service` and a kmscon.conf with no `login` give them to the shell. With kmscon.conf's `reset-env` off, vtium runs `/bin/login` without `-p`, which clears the environment, and so do a getty's login and any `login` command of a head that clears it. Such a command can still take them as words: vtium replaces a word of the command that is exactly `$VTIUM_HEAD` or `$VTIUM_CONSOLE` with its value. A program that wants to know that turns on focus reporting (`printf '\033[?1004h'`): while a keyboard types into the head, vtium sends it a focus-in when the head shows its console and a focus-out when the head switches to another.

## Two policies: open and closed

A head file decides which keyboards reach which displays, as a firewall decides which traffic passes, and like a firewall it has a default policy: `policy = open` or `policy = closed`. A file without it is open. The rules read the same under either; the policy decides what happens to an input no rule matches, to a head's `isolated` when the file leaves it unset, to the rescue key and the anti-lockout guard, and to a start with no display. A rule the file writes means what it says under either policy: in a closed file, `input MATCH -> roaming` sends whatever is plugged into that port to the active head, a head that writes `isolated = no`, which is how [kiosk-admin.conf](#a-kiosk-with-an-operators-head) gives its admin head a spare port. A file whose rules end with `input * -> TARGET` places every input itself, so the policy's input default never applies.

**Open.** A keyboard or mouse no rule matches roams. A head is not isolated unless it says `isolated = yes`. The rescue key and the anti-lockout guard are on. Rules take rights away: "this keyboard belongs to the desk", "the panel is isolated". The question while writing one is what you must stop from reaching everything. A forgotten rule leaks: a stranger's keyboard plugged into a spare port types into someone's session. Today's console and builtin are open.

**Closed.** A keyboard or mouse no rule matches is ignored, and every head is isolated unless it says `isolated = no`. The rescue key and the anti-lockout guard are off, so no keyboard is ever moved anywhere the file did not send it. Rules grant rights: "this keyboard belongs to the kiosk", "the operator's head is not isolated". The question is what you must let in so you don't shut yourself out. A forgotten rule locks out: your own keyboard is dead. A kiosk and stations are closed files. Once vtium has read `policy = closed` in a file it trusts, nothing opens anything for as long as vtium runs: no other line of the file, broken or not, and nothing that happens while it runs. A reload, once it is built, applies only a valid closed file while a closed policy runs, and logs why it keeps what runs for anything else, a valid open file included; going from closed to open takes a restart of vtium, which reads the file afresh, as a boot does. While an open policy runs, a reload applies any valid file, a closed one included.

**With no display at start.** Under open, vtium waits `display-wait`, 10 s by default, then exits, and its unit, `kmsconvt@ttyN`, starts a getty on that VT, the kernel's own login, which any keyboard can type into. Under closed, vtium holds its VT and waits for a display however long it takes, so that getty never starts.

A display no rule matches is off under either policy. When the rules light no display, the display guard lights the boot display: in an open file it shows the first head there, and in a closed file only a notice. A valid closed file whose heads have no display lit shows `vtium: no display for any head` there, the one notice a valid file shows. Keyboards a rule gives a head keep typing into it while it is unlit, as on the kernel's console, so the notice says nothing about input. A closed file with no heads is dark and silent. `vtium check-config` lists, as notes, the keyboards, mice and displays present that no rule matches and what the policy does with them; for a closed file it says prominently when no keyboard present can reach any head, before the file is installed.

## Which input can reach whose session

Each route by which one keyboard can disturb another person's session has a switch of its own, and acts under the head the keyboard asks ([Terms](#terms)). In an open file a route the file says nothing about is open, as on today's console; in a closed file it is closed until the file opens it.

| Route | Disturbs | Governed by |
|---|---|---|
| The VT keys, `Ctrl-Alt-F<n>` and `XF86Switch_VT_<n>` | every display, which leaves for another VT | the `isolated` of the head the keyboard asks; a roaming keyboard with no active head asks none |
| The console chords | the console the head shows | the head's `consoles`, a fixed count the chords switch among: with 1 there is nothing to switch |
| The roaming chord | where the roaming keyboards type | taken only from roaming keyboards |
| The rescue key | takes a keyboard off its head, or makes an ignored one roam | the policy, on under open and off under closed, and under open the head's `isolated` |
| The anti-lockout guard | makes every keyboard roam | the policy: on under open, off under closed |
| Unclaimed keyboards | type into the active head | the policy: under open they roam unless a rule such as `input * -> ignore` closes them, and under closed they are ignored |
| Arrival by roaming | the active head | the head's `isolated`: an isolated head is never the active head |

## The operator's way back in

### An operator's head

A closed file gives the operator a keyboard at the machine only through a head of their own, on a display of its own, with `isolated = no` and a rule that sends their keyboard to it: [A kiosk with an operator's head](#a-kiosk-with-an-operators-head) shows one. A machine with one display has no display to spare for such a head: its operator gets in from outside, or gives the one head `isolated = no`. `Ctrl-Alt-F2` on that head's keyboard then switches to VT 2, where a getty's password stands between a passer-by and a shell, and the head becomes the active head, which any `-> roaming` rule in the file reaches.

### Recovery from outside

A machine its head file locks out is recovered as any misconfigured Linux system is: over ssh, from a serial getty, through out-of-band management, or by booting other media and editing the file. vtium has no rescue boot of its own.

### Invalid files

vtium never fixes, renames or replaces the file, and resolves it in three steps:

1. **Trust.** A file that fails the ownership check above, or that vtium cannot open, is never read: builtin runs, and the console shows `invalid config skipped, using builtin`. A closed file left writable, or deleted, therefore opens the machine at the next boot or restart of vtium.
2. **The policy.** vtium reads `policy` from the top-level keys. A readable `policy = closed` wins over any other `policy` line. A `policy` line it cannot read, inside a section or with an unknown value, counts as no policy, so open, and is an error.
3. **The rest.** An error inside a head's section drops that head; a bad rule drops that rule alone; a missing or unknown `version`, or an error that leaves the file's structure untrustworthy, drops all but the policy. If a head survives, vtium runs what survives and shows `config partly invalid, running what's valid`. In an open file vtium then adds builtin's input rules after the surviving rules, so a keyboard, pointer or key device that no surviving rule places roams and every other input is ignored, and it runs every surviving head as not isolated, so the rescue key and the active head always have somewhere to land. A closed file keeps its heads' isolation as written. If nothing survives, an open file runs builtin with `invalid config skipped, using builtin`, and a closed file shows the panic screen.

The panic screen means vtium has started correctly and can serve nothing. The boot display shows, in words like these, `vtium: config invalid, policy closed: input off`, that no head in the file survives, the first errors with their lines, and `nothing on this console takes input; the full report: vtium check-config /etc/vtium/heads.conf`. The screen stays for as long as vtium runs. `check-config` rebuilds that report from the file itself, run over ssh or from a live USB with the disk mounted.

That failure is final by design. vtium keeps no copy and no last-known-good of the file, so a closed file is checked before it is relied on: with `vtium check-config FILE` today, and with `vtiumctl`'s trial, which applies a candidate and reverts it unless confirmed, once it is built.

## Under the open policy

### The default

**Purpose.** A console that can be typed on with no configuration at all.

With no file present, this is what vtium runs today. `vtiumctl template` will write the file with every other setting as a commented line.

```ini
# /etc/vtium/heads.conf: the compiled-in configuration, written out.
# Deleting this file gives exactly the same behaviour.
version = 1
policy = open              # open: heads are not isolated, and an input no rule places roams

[head console]             # one head, named "console"
                           # every head key is unset, so:
                           #   consoles = 1, profile = default,
                           #   when-display-lost = keep, isolated = no

[rules]                    # first match wins, top to bottom
display * -> console       # every display shows the one head, mirrored
input class:keyboard -> roaming      # keyboards type into the active head
input class:mouse -> roaming         # (with one head, that's always "console")
input class:touchpad -> roaming
input class:touchscreen -> roaming
input class:trackball -> roaming
input class:pointingstick -> roaming
input class:key -> roaming # numpads, macro pads, media keys
input * -> ignore          # joysticks, tablets, switches: nothing they send is delivered
```

[heads/default.conf](heads/default.conf)

**What to expect.**

- Every display lights as one head, and every keyboard, pointer and other key device types into it.
- The classes are udev's, read from its tags, and vtium derives the same classes itself where no udev daemon runs, so this file means the same either way.
- A display unplugged leaves the session running; it comes back when a display returns. With every display gone, vtium keeps running and waits for one.
- `Ctrl-Alt-F<n>` on any keyboard switches the VT, and with it every display.
- kmscon.conf's `mouse` off drops the pointer rules.

### Two heads with bound keyboards

**Purpose.** Two displays of different sizes, a large monitor and a small panel, each with its own grid at its own density and its own keyboard, and the spare devices roaming.

```ini
version = 1
policy = open

[head desk]
profile = desk             # its font and look, from the profile named desk

[head panel]
profile = small
consoles = 2               # two consoles, which the console chords switch
when-display-lost = hangup 30s   # panel unplugged: sessions end after 30 s

[rules]
display edid:AAA:0x0001:LARGE00001 -> desk
display edid:BBB:0x0002:SMALL00001 -> panel
input path:pci-0000:15:00.0-usb-0:3.3 -> desk     # keyboard A: desk only
input path:pci-0000:15:00.0-usb-0:3.4.4 -> desk   # the mouse: desk only
input path:pci-0000:15:00.0-usb-0:3.2 -> panel    # keyboard B: panel only
display * -> desk          # any other monitor mirrors desk
input class:keyboard -> roaming   # a spare keyboard types where the roaming
input class:mouse -> roaming      # chord points it, desk or panel
input * -> ignore
```

[heads/two-heads.conf](heads/two-heads.conf)

**What to expect.**

- The large monitor shows head `desk` and the small panel head `panel`, each with a grid sized for it.
- Keyboard A's console chords do nothing, since `desk` has one console, and keyboard B's switch between `panel`'s two. Pressed on a bound keyboard, the roaming chord moves nothing and reaches that head's session as a key.
- `Ctrl-Alt-F<n>` on any keyboard switches the VT, and with it both displays.
- The monitor unplugged leaves `desk`'s sessions running; they come back when it returns. A head comes back only on a display its rules match.
- The panel unplugged: keyboard B goes on typing into `panel`'s console, unseen, and after 30 s its sessions are hung up. The panel back within the 30 s brings the sessions back; back after 60 s, it shows `panel`'s consoles started afresh at their login. The anti-lockout guard does not fire, since keyboard A still reaches `desk`, which is lit. If `panel` was the active head, roaming keyboards type into it unlit until the roaming chord moves them.
- With no keyboard reaching a lit head, every keyboard roams, with a notice, until one does. The rescue key makes the keyboard that pressed it roam for the rest of the boot.

## Under the closed policy

Each closed file needs the operator's way in decided before it is installed ([The operator's way back in](#the-operators-way-back-in)).

### A kiosk

**Purpose.** One display running one program, which no keyboard but its own reaches and none takes away from it.

```ini
version = 1
policy = closed            # closed: anything no rule names gets nothing.
                           # Every head is isolated unless it says otherwise.

[head kiosk]
consoles = 1               # nothing to switch between
login = /usr/local/bin/kiosk   # runs as root, so it must drop privileges itself
                           # isolated = yes, implied by policy = closed

[rules]
display edid:AAA:0x0001:KIOSK00001 -> kiosk
input path:pci-0000:15:00.0-usb-0:3.1 -> kiosk   # the kiosk's own USB port
# Nothing else is needed: any other display is off, under either policy, and
# under closed any other input is ignored, so a stranger's keyboard does nothing.
```

[heads/kiosk.conf](heads/kiosk.conf)

**What to expect.**

- The kiosk has one console; `Ctrl-Alt-F<n>` for another VT reaches the program as keys, and the rescue key pressed on its keyboard moves nothing and is swallowed.
- `vtium check-config` notes that no keyboard can switch the VT, that no head can be the active head, that only one keyboard reaches the kiosk, and the keyboards present on other ports as ignored, and exits 0.
- As written, the operator gets in only from outside, since the machine has one display.
- The display unplugged leaves the program running, and it comes back when the display returns. With the kiosk's monitor absent at boot and another display connected, that display shows `vtium: no display for any head`; with no display connected at all, vtium holds its VT and waits.

### A kiosk with an operator's head

**Purpose.** The kiosk above, with a second display where the operator can log in at the machine.

```ini
version = 1
policy = closed

[head kiosk]
consoles = 1
login = /usr/local/bin/kiosk

[head admin]
isolated = no              # the one head with rights: its keyboard can switch
                           # the VT, and it's the active head for roaming input
consoles = 4

[rules]
display edid:AAA:0x0001:KIOSK00001 -> kiosk
input path:pci-0000:15:00.0-usb-0:3.1 -> kiosk
display edid:BBB:0x0002:ADMIN00001 -> admin
input path:pci-0000:15:00.0-usb-0:3.2 -> admin   # the operator's keyboard
input path:pci-0000:15:00.0-usb-0:3.3 -> roaming # a spare port: whatever is
                                                 # plugged in here types at admin
# Without the spare-port line, only the keyboard on port 3.2 reaches admin.
```

[heads/kiosk-admin.conf](heads/kiosk-admin.conf)

**What to expect.**

- The kiosk behaves as in the file above. `admin` is the active head, the only one that can be.
- A keyboard on the spare port, 3.3, roams, and `admin` is the active head, so it acts as `admin`'s keyboard: it types into `admin`'s login, and its `Ctrl-Alt-F<n>` switches the VT, which takes the kiosk's display away too. That is the admin head's right, lent to whatever is plugged into that port.
- `Ctrl-Alt-F<n>` pressed on the operator's keyboard, on port 3.2, switches the VT the same way.
- With the rescue key off under the closed policy, a keyboard on any other port goes nowhere.

### Stations: one person to each head

**Purpose.** Several people working at once on one machine, each at their own display, keyboard and mouse: a multi-terminal host. The closed policy makes it work: `Ctrl-Alt-F<n>` would otherwise switch every station's display away, and a roaming keyboard would type into someone's station.

```ini
version = 1
policy = closed            # every station is isolated: nobody's keys can
                           # switch the VT away from everyone else

[head station-1]
consoles = 2
[head station-2]
consoles = 2
[head station-3]
consoles = 2

[rules]
display edid:AAA:0x0001:MON0000001 -> station-1
display edid:AAA:0x0001:MON0000002 -> station-2
display edid:AAA:0x0001:MON0000003 -> station-3
input path:pci-0000:15:00.0-usb-0:3.1 -> station-1  # a station is a USB port:
input path:pci-0000:15:00.0-usb-0:3.2 -> station-2  # a hub on it carries that
input path:pci-0000:15:00.0-usb-0:3.3 -> station-3  # station's keyboard and mouse
```

[heads/stations.conf](heads/stations.conf)

**What to expect.**

- Each station's display shows its own head, with its own login, and its keyboard and mouse work only there. A device on no station's port is ignored, and a display no rule names is off.
- `Ctrl-Alt-F<n>` and `XF86Switch_VT_<n>` for another VT, pressed on a station's keyboard, reach that station's session as keys, as a serial terminal sends them. The roaming chord reaches the session the same way. The rescue key moves nothing and is swallowed.
- No station can be the active head, so no keyboard is moved between stations.
- `vtium check-config` notes that no keyboard can switch the VT and that no head can be the active head, lists the devices present on no station's port as ignored, and exits 0.
- As written, the operator gets in only from outside. An operator's head on a display of its own gives a way in at the machine; the VT keys pressed there switch every station's display.
- A station's display unplugged leaves its sessions running unseen (`when-display-lost = keep`, the default), and its keyboard still reaches them; they come back when that display returns. For stations people walk away from, `hangup` or `hangup 30s` ends the sessions when the display goes.
- A station's keyboard unplugged leaves its session running, and the keyboard plugged into the same port returns to it.
- More than 8 stations' consoles need kmscon.conf's `consoles-max` raised.

**What stays shared.**

- One vtium process, running as root, serves every station, so a crash ends every station's sessions at once, and vtium's unit, `kmsconvt@ttyN`, then starts a getty on its VT (`OnFailure=`), which any keyboard can type into. Sessions that survive a vtium restart are planned next after heads.
- One kernel VT is shown on every station's display, so anything that switches the VT switches every station.
- A station's login runs on a pty and gets a logind session with no seat, so no seat-bound device, such as audio, reaches it.
- A fully independent station, a logind seat with its own cards and inputs, is the seat extension of ADR-1401 D2, which is not built.

### Everything off

**Purpose.** vtium holds its VT and draws nothing, for a machine whose console must show nothing. It opens no keyboard, so no key can switch the VT.

```ini
version = 1
policy = closed
# No heads and no rules: a valid file that lights no display and opens no input.
```

[heads/off.conf](heads/off.conf)

The boot display stays dark too: the display guard lights it under the closed policy only for a file with heads.
