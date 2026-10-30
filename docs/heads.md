# Heads, by scenario

This page shows the head file, `/etc/vtium/heads.conf`, in the setups it is made for: each scenario's purpose, a complete head file, and what to expect, on an unplug and of what stays shared. [ADR-1401](architecture/platform/ADR-1401-heads-a-terminal-per-physical-display-placed-by-a-first-match-ruleset.md) holds every rule; this page applies them. Each scenario's file is also a file of its own under [heads/](heads/), and the `edid:` and `path:` values in them are illustrative.

Heads are built, under older names: the first cut parses `focus` where this page writes `roaming`, `dark` for `off`, `on-loss` with `nohup` for `when-display-lost` with `keep`, a `[guards]` section, and the head keys `rescue`, `font-name` and `font-size`. Not built yet: `policy`, `isolated`, the names `roaming`, `off` and `when-display-lost`, the removal of `[guards]` and of a head's own font keys, the handling of a broken file under [Invalid files](#invalid-files), and `vtiumctl`. Nor is the check that each block on this page equals its file and that `vtium check-config` accepts each file; it comes with the change that parses the new names. Each scenario says what works today.

## Terms

- **Head**: a terminal for one or more displays, with its own grid, its own [profile](architecture/font/ADR-1200-glyphs-are-generated-at-run-time-in-a-sandboxed-helper-from-a-profile-s-frit.md) (how it looks, its font included) and its own virtual consoles. A head with several displays shows one grid on all of them, sized for the smallest and drawn larger on the bigger ones.
- **Display**: a monitor or panel, one connector of a graphics card. A head is lit when at least one of its displays is on.
- **Policy**, `open` or `closed`: what happens to anything the file doesn't mention. Open gives it everything; closed gives it nothing.
- **Isolated**: a head with `isolated = yes`. Its keyboards' VT keys reach its session as keys and never switch the VT, the rescue key never takes its keyboards away, and it is never the active head, so no roaming keyboard reaches it.
- **Roaming keyboard**: a keyboard, or mouse, that no head owns. It types into the active head.
- **The active head**: the one head roaming keyboards type into. Each head shows a mark when it is the active head. At boot it is the head of the boot display, the display lit at start, else the first head in the file with a lit display, never an isolated one.
- **The roaming chord**: the keys that make the next or previous lit head the active head, today `Ctrl-Super-Up` and `Ctrl-Super-Down`. Only roaming keyboards can use it.
- **Console chords**: the keys that open, switch and close a head's virtual consoles, today `Ctrl-Super-Return`, `Ctrl-Super-Right`, `Ctrl-Super-Left` and `Ctrl-Super-Backspace`. They act on the head the keyboard types into.
- **The rescue key**: `Ctrl-Alt-F<n>` for the VT vtium is on, pressed while that VT is shown. Under the open policy it makes the keyboard that pressed it roam until the next boot. A press that moves nothing, under the closed policy or on an isolated head, is swallowed and never reaches the session.
- **The anti-lockout guard**: under the open policy, while no keyboard reaches a lit head, every keyboard roams, with a notice, until one does.
- **The display guard**: when the rules light no display, it lights the boot display, under the open policy for the first head, and under the closed policy for a notice alone.

## Reading a head file

A head file starts with its top-level keys, in any order: `version = 1` and, optionally, `policy`. Each `[head NAME]` section declares a head. Under `[rules]`, each line is `display MATCH -> TARGET` or `input MATCH -> TARGET`; each display and each input walks the rules from the top on its own, and the first that matches places it in a head, roaming (`roaming`), ignored (`ignore`) or off (`off`). A head's keys are `profile`, `consoles`, `login`, `when-display-lost` and `isolated`; there is no other section.

A rule names a device most strongly by its USB port (`path:`), its unit (`serial:`) or its monitor (`edid:`), and only for convenience by its model (`id:`) or its name (`name:`). A `path:` match takes every interface behind that port, so a hub on the port joins with it; `serial:` binds the unit wherever it is plugged. Until `vtiumctl detect` lists the values, find them by hand:

- `path:` is udev's `ID_PATH` up to the port: `udevadm info -q property -n /dev/input/event5 | grep ID_PATH=` prints `ID_PATH=pci-0000:15:00.0-usb-0:3.3:1.0`, and the rule takes `path:pci-0000:15:00.0-usb-0:3.3`. `ls -l /dev/input/by-path/` maps the ports to event nodes.
- `edid:` is `MAKER:PRODUCT:SERIAL` from the monitor's EDID, which `/sys/class/drm/card*-*/edid` holds; a decoder such as `edid-decode /sys/class/drm/card1-DP-8/edid` prints the manufacturer, the product code and the serial.

A wrong `path:` or `edid:` value is no error: the rule matches nothing, and the device walks on to the next rule, then to the file's policy ([below](#two-policies-open-and-closed)). A misspelt keyboard rule therefore leaves that keyboard roaming in an open file, and ignored in a closed one.

vtium reads the file only when root owns it and no one else can write it. With no file it runs its compiled-in configuration, [default.conf](heads/default.conf), and writes nothing. `vtium check-config FILE` checks a candidate before it is installed. It exits 1 on an error or a warning about the file itself: a line it cannot parse, an unknown key or value, a rule naming no head, a rule an earlier one shadows. Everything that depends on the devices present, a rule matching none of them, a head whose only keyboard is one device, what the policy did with the devices no rule names, and every right a file takes away, is a note, and it exits 0.

Each head's consoles run its `login` command, else that of kmscon.conf, vtium's main configuration file (`/etc/kmscon/kmscon.conf`, listed in [config.md](config.md)), as root, as kmscon.conf's `login` does; a kiosk program drops its privileges itself, or is started through a tool that does. A head has 1 console unless its `consoles` says otherwise; by default each head may open 4 and all heads together 8, which kmscon.conf's `head-consoles-max` and `consoles-max` raise, up to 50 consoles and 16 heads.

## Two policies: open and closed

A head file decides which keyboards reach which displays, as a firewall decides which traffic passes, and like a firewall it has a default policy: `policy = open` or `policy = closed`. A file without it is open. The rules read the same under either; the policy decides what happens to a device no rule matches, to a head's `isolated` when the file leaves it unset, and to the rescue key and the anti-lockout guard. Under `policy = closed` only unset values change: every rule the file writes means what it says, so `input MATCH -> roaming` is still how a closed file gives a head that is not isolated a spare port. A rule `input * -> TARGET` matches every input, so in a file whose rules end with one, the policy's input default never applies.

**Open.** A keyboard or mouse no rule matches roams. A head is not isolated unless it says `isolated = yes`. The rescue key and the anti-lockout guard are on. Rules take rights away: "this keyboard belongs to the desk", "the panel is isolated". The question while writing one is what you must stop from reaching everything. A forgotten rule leaks: a stranger's keyboard plugged into a spare port types into someone's session. Today's console and the shipped rules are open.

**Closed.** A keyboard or mouse no rule matches is ignored, and every head is isolated unless it says `isolated = no`. The rescue key and the anti-lockout guard are off, so no keyboard is ever moved anywhere the file did not send it. Rules grant rights: "this keyboard belongs to the kiosk", "the operator's head is not isolated". The question is what you must let in so you don't shut yourself out. A forgotten rule locks out: your own keyboard is dead. A kiosk and stations are closed files. Once vtium has read `policy = closed` in a file it trusts, nothing opens anything for the rest of the boot: no other line of the file, broken or not, and nothing that happens while vtium runs.

A display no rule matches is off under either policy. When the rules light no display, the display guard lights the boot display: in an open file for the first head, and in a closed file only for a notice. A valid closed file whose heads have no display lit shows `vtium: no display for any head` there, the one notice a valid file shows; it says nothing about input, since each head's own keyboards still type into it unlit, as on the kernel's console. A closed file with no heads is dark and silent. `vtium check-config` lists, as notes, the keyboards, mice and displays present that no rule matches and what the policy does with them; for a closed file it says prominently when no keyboard present can reach any head, before the file is installed.

## Which input can reach whose session

Each route by which one keyboard can disturb another person's session has a switch of its own. In an open file a route the file says nothing about is open, as on today's console; in a closed file it is closed until the file opens it.

| Route | Disturbs | Governed by |
|---|---|---|
| The VT keys, `Ctrl-Alt-F<n>` and `XF86Switch_VT_<n>` | every display, which leaves for another VT | the `isolated` of the head the keyboard asks: its own, or the active head, never isolated, for a roaming keyboard; with no active head, none |
| The console chords | the console the head shows | the head's `consoles`: with 1 there is nothing to switch |
| The roaming chord | where the roaming keyboards type | taken only from roaming keyboards |
| The rescue key | takes a keyboard off its head, or makes an ignored one roam | the policy, on under open and off under closed, and under open the head's `isolated` |
| The anti-lockout guard | makes every keyboard roam | the policy: on under open, off under closed |
| Unclaimed keyboards | type into the active head | the policy: under open they roam unless a rule such as `input * -> ignore` closes them, and under closed they are ignored |
| Arrival by roaming | the active head | the head's `isolated`: an isolated head is never the active head |

Every keyboard asks a head: a claimed keyboard asks its own head, a roaming keyboard asks the active head, and with no head to ask the answer is no. A roaming keyboard therefore switches the VT whenever there is an active head, and with none it delivers nothing, the VT keys included.

## The operator's way back in

### An operator's head

A closed file gives the operator a keyboard at the machine only through a head of their own, on a display of its own, with `isolated = no` and a rule that sends their keyboard to it: [A kiosk with an operator's head](#a-kiosk-with-an-operators-head) shows one. A machine with one display has no display to spare for such a head: its operator gets in from outside, or gives the one head `isolated = no`, so `Ctrl-Alt-F2` reaches a getty on another VT, where a password stands between a passer-by and a shell, though that head then also takes roaming keyboards.

### Recovery from outside

A machine its head file locks out is recovered as any misconfigured Linux system is: over ssh, from a serial getty, through out-of-band management, or by booting other media and editing the file. vtium has no rescue boot of its own.

### Invalid files

vtium never fixes, renames or replaces the file, and resolves it in three steps:

1. **Trust.** A file that root does not own, that others can write, or that vtium cannot open is never read: the shipped rules run, and the console shows `invalid config skipped, using builtin`. A closed file left writable, or deleted, therefore opens the machine at the next boot.
2. **The policy.** vtium reads `policy` from the top-level keys. A readable `policy = closed` wins over any other `policy` line. A `policy` line it cannot read, inside a section or with an unknown value, counts as no policy, so open, and is an error.
3. **The rest.** An error inside a head's section drops that head; a bad rule drops that rule alone; an error that leaves the file's structure untrustworthy drops all but the policy. If a head survives, vtium runs what survives and shows `config partly invalid, running what's valid`; in an open file the shipped rules' keyboard, pointer and key classes follow after the surviving rules, and every surviving head acts as not isolated, so the rescue key and the active head always have somewhere to land; a closed file keeps its heads' isolation as written. If nothing survives, an open file runs the shipped rules with `invalid config skipped, using builtin`, and a closed file runs nothing.

A closed file with nothing left is vtium's panic screen. vtium has started correctly and can serve nothing; the boot display shows, in words like these, `vtium: config invalid, closed: input off`, that no head in the file survives, the first errors with their lines, and `nothing on this console takes input; the full report: vtium check-config /etc/vtium/heads.conf`. The screen stays for as long as vtium runs. `check-config` rebuilds that report from the file itself, run over ssh or from a live USB with the disk mounted.

> The panic screen's meaning, in the operator's words: "you done fucked up, Ay-Ay-Ron. Bring a USB stick or an ssh key."

That failure is final by design. vtium keeps no copy and no last-known-good of the file, so a closed file is checked before it is relied on: with `vtium check-config FILE` today, and with `vtiumctl`'s trial, which applies a candidate and reverts it unless confirmed, once it is built.

## Under the open policy

### The default

**Purpose.** A console that can be typed on with no configuration at all.

**Works today** with no file present. The file below, the compiled-in configuration written out, needs the new names; `vtiumctl template` will write it with every other setting as a commented line.

```ini
# /etc/vtium/heads.conf: the compiled-in configuration, written out.
# Deleting this file gives exactly the same behaviour.
version = 1
policy = open              # open: anything no rule names gets everything

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
input * -> ignore          # joysticks, tablets, switches: never opened
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

**Works today** under the first cut's names (`-> focus`, `on-loss`, no `policy` line); the file as written needs the new names.

```ini
version = 1
policy = open

[head desk]
profile = desk             # its font and look, from the profile named desk

[head panel]
profile = small
consoles = 2               # two virtual consoles on this head
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
- Keyboard A's console chords switch `desk`'s console, and keyboard B's switch `panel`'s two. Pressed on a bound keyboard, the roaming chord moves nothing and reaches that head's session as a key.
- `Ctrl-Alt-F<n>` on any keyboard switches the VT, and with it both displays.
- The monitor unplugged leaves `desk`'s sessions running; they come back when it returns. A head comes back only on a display its rules match.
- With no keyboard reaching a lit head, every keyboard roams, with a notice, until one does. The rescue key makes the keyboard that pressed it roam until the next boot.

## Under the closed policy

Each closed file needs the operator's way in decided before it is installed ([The operator's way back in](#the-operators-way-back-in)).

### A kiosk

**Purpose.** One display running one program, which no keyboard but its own reaches and none takes away from it.

**Not built yet:** it needs `policy` and `isolated`.

```ini
version = 1
policy = closed            # closed: anything no rule names gets nothing.
                           # Every head is isolated unless it says otherwise.

[head kiosk]
consoles = 1               # nothing to switch between
login = /usr/local/bin/kiosk   # runs as root, so it must drop privileges itself
                           # isolated = yes is implied by policy = closed:
                           #   its keyboard can't switch the VT or be rescued
                           #   away, and no other keyboard can reach it

[rules]
display edid:AAA:0x0001:KIOSK00001 -> kiosk
input path:pci-0000:15:00.0-usb-0:3.1 -> kiosk   # the kiosk's own USB port
# Nothing else is needed: under closed, any other display is off and any
# other input is ignored. A stranger's keyboard on another port does nothing.
```

[heads/kiosk.conf](heads/kiosk.conf)

**What to expect.**

- The kiosk has one console; `Ctrl-Alt-F<n>` for another VT reaches the program as keys, and the rescue key pressed on its keyboard moves nothing and is swallowed.
- `vtium check-config` notes that no keyboard can switch the VT, that no head can be the active head, that the kiosk's only keyboard is one device, and the keyboards present on other ports as ignored, and exits 0.
- As written, the operator gets in only from outside, since the machine has one display.
- The display unplugged leaves the program running, and it comes back when the display returns. With the kiosk's display absent at boot, the boot display shows `vtium: no display for any head`.

### A kiosk with an operator's head

**Purpose.** The kiosk above, with a second display where the operator can log in at the machine.

**Not built yet:** it needs `policy` and `isolated`.

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
- The keyboard on the spare port types into `admin`'s login, so anyone at the machine who plugs into that port can reach it. With the rescue key off under the closed policy, a keyboard on any other port goes nowhere.
- `Ctrl-Alt-F<n>` pressed on `admin`'s keyboards switches the VT, and with it the kiosk's display.

### Stations: one person to each head

**Purpose.** Several people working at once on one machine, each at their own display, keyboard and mouse: a multi-terminal host. The closed policy makes it work: `Ctrl-Alt-F<n>` would otherwise switch every station's display away, and a roaming keyboard would type into someone's station.

**Not built yet:** it needs `policy` and `isolated`.

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
# Use serial: instead of path: to bind a device wherever it's plugged in.
```

[heads/stations.conf](heads/stations.conf)

**What to expect.**

- Each station's display shows its own head, with its own login, and its keyboard and mouse work only there. A device on no station's port is ignored, and a display no rule names is off.
- `Ctrl-Alt-F<n>` and `XF86Switch_VT_<n>` for another VT, pressed on a station's keyboard, reach that station's session as keys, as a serial terminal sends them. The roaming chord reaches the session the same way. The rescue key moves nothing and is swallowed.
- No station can be the active head, so no keyboard is moved between stations.
- `vtium check-config` notes that no keyboard can switch the VT and that no head can be the active head, lists the devices present on no station's port as ignored, and exits 0.
- As written, the operator gets in only from outside. An operator's head on a display of its own gives a way in at the machine; the VT keys pressed there switch every station's display.
- A station's display unplugged leaves its sessions running unseen (`when-display-lost = keep`, the default), and its keyboard still reaches them; they come back when that display returns. For stations people walk away from, `when-display-lost = hangup` ends the sessions when the display goes, and `when-display-lost = hangup 30s` after a grace period.
- A station's keyboard unplugged leaves its session running, and the keyboard plugged into the same port returns to it.
- More than 8 stations' consoles need kmscon.conf's `consoles-max` raised.

**What stays shared.**

- One vtium process, running as root, serves every station, so a crash ends every station's sessions at once, and the unit restarts vtium or starts a getty. Sessions that survive a vtium restart are planned next after heads.
- One kernel VT is shown on every station's display, so anything that switches the VT switches every station.
- A station's login runs on a pty and gets a logind session with no seat, so no seat-bound device, such as audio, reaches it.
- A fully independent station, a logind seat with its own cards and inputs, is the seat extension of ADR-1401 D2, which is not built.

### Everything off

**Purpose.** A machine where vtium runs and shows nothing: no head, no display lit, no input read.

**Not built yet:** it needs `policy`.

```ini
version = 1
policy = closed
# No heads and no rules: every display is off, every input is ignored,
# and nothing appears on the screen. This is a valid file, not an error.
```

[heads/off.conf](heads/off.conf)

The boot display stays dark too: the display guard lights it under the closed policy only for a file with heads.
