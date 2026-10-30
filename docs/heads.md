# Heads, by scenario

This page shows the head file, `/etc/vtium/heads.conf`, in the setups it is made for: each scenario's purpose, a complete head file, and what to expect, on an unplug and of what stays shared. [ADR-1401](architecture/platform/ADR-1401-heads-a-terminal-per-physical-display-placed-by-a-first-match-ruleset.md) holds every rule; this page applies them. Heads are built, and some pieces this page uses are not yet: the `default` key, the head keys `vt-keys` and `focus`, the handling of a broken file described under [Invalid files](#invalid-files), the removal of the `[guards]` switches `anti-lockout` and `rescue-key` and of a head's own `font-name` and `font-size`, and `vtiumctl`. Each scenario says whether it works today.

## Terms

- **Display**: a monitor or panel, one connector of a graphics card.
- **Head**: a terminal for one or more displays, with its own grid, its own [profile](architecture/font/ADR-1200-glyphs-are-generated-at-run-time-in-a-sandboxed-helper-from-a-profile-s-frit.md) (how it looks, its font included) and its own virtual consoles. A head with several displays shows one grid on all of them, sized for the smallest and drawn larger on the bigger ones (the scaled rule).
- **A lit head**: a head with at least one of its displays on.
- **Console chords**: the keys that open, switch and close a head's virtual consoles, today `Ctrl-Super-Return`, `Ctrl-Super-Right`, `Ctrl-Super-Left` and `Ctrl-Super-Backspace`. They act on the head the keyboard types into.
- **The focus**: the one head that keyboards without a head of their own type into. A rule sends a device there with the target `focus`; a head's `focus` key says whether the focus may rest on that head; each head shows a mark when it holds it. At boot the focus starts on the head of the boot display, the display lit at start, else on the first head in the file with a lit display, taking only a head that allows it.
- **The focus chord**: the keys that move the focus to the next or previous lit head, today `Ctrl-Super-Up` and `Ctrl-Super-Down`. Only keyboards following the focus can use it.
- **The rescue key**: `Ctrl-Alt-F<n>` for the VT vtium is on, pressed while that VT is shown. In an allow file it moves the keyboard that pressed it to the focus until the next boot. A press that moves nothing, in a deny file or on a head with `rescue = off`, is swallowed and never reaches the session.
- **The guards**: the anti-lockout guard, which sends every keyboard to the focus while none reaches a lit head; the display guard, `never-dark`, which lights the boot display when the rules light none; and the rescue key.
- **kmscon.conf**: vtium's main configuration file, `/etc/kmscon/kmscon.conf`, listed in [config.md](config.md).

## Reading a head file

A head file starts with its top-level keys, in any order: `version = 1` and, optionally, `default`. Each `[head NAME]` section declares a head. Under `[rules]`, each line is `display MATCH -> TARGET` or `input MATCH -> TARGET`; each display and each input walks the rules from the top on its own, and the first that matches places it in a head, in the focus (`focus`), ignored (`ignore`) or dark (`dark`). `[guards]` holds one switch, `never-dark`, on unless the file turns it off.

A rule names a device most strongly by its USB port (`path:`), its unit (`serial:`) or its monitor (`edid:`), and only for convenience by its model (`id:`) or its name (`name:`). A `path:` match takes every interface behind that port, so a hub on the port joins with it; `serial:` binds the unit wherever it is plugged. The identity values in the files below are illustrative. Until `vtiumctl detect` lists them, find them by hand:

- `path:` is udev's `ID_PATH` up to the port: `udevadm info -q property -n /dev/input/event5 | grep ID_PATH=` prints `ID_PATH=pci-0000:15:00.0-usb-0:3.3:1.0`, and the rule takes `path:pci-0000:15:00.0-usb-0:3.3`. `ls -l /dev/input/by-path/` maps the ports to event nodes.
- `edid:` is `MAKER:PRODUCT:SERIAL` from the monitor's EDID, which `/sys/class/drm/card*-*/edid` holds; a decoder such as `edid-decode /sys/class/drm/card1-DP-8/edid` prints the manufacturer, the product code and the serial.

A wrong `path:` or `edid:` value is no error: the rule matches nothing, and the device walks on to the next rule, then to the file's posture ([below](#two-postures-allow-and-deny)). A misspelt keyboard rule therefore leaves that keyboard following the focus in an allow file, and ignored in a deny file.

vtium reads the file only when root owns it and no one else can write it. With no file it runs its compiled-in configuration and writes nothing. `vtium check-config FILE` checks a candidate before it is installed. It exits 1 on an error or a warning about the file itself: a line it cannot parse, an unknown key or value, a rule naming no head, a rule an earlier one shadows. Everything that depends on the devices present, a rule matching none of them, a head whose only keyboard is one device, what the posture did with the devices no rule names, and every right a file turns off, is a note, and it exits 0.

Each head's consoles run its `login` command, else kmscon.conf's, as root, as kmscon.conf's `login` does; a kiosk program drops its privileges itself, or is started through a tool that does. A head has 1 console unless its `consoles` says otherwise; by default each head may open 4 and all heads together 8, which kmscon.conf's `head-consoles-max` and `consoles-max` raise, up to 50 consoles and 16 heads.

## Two postures: allow and deny

A head file decides which keyboards reach which displays, as a firewall decides which traffic passes, and like a firewall it has a default policy, which vtium calls its posture: `default = allow` or `default = deny`. A file without it is an allow file. The rules read the same under either; the posture decides what happens to a device no rule matches, to a head key the file leaves unset, and to the rescue key and the anti-lockout guard. A rule `input * -> TARGET` matches every input, so in a file whose rules end with one, the posture's input default never applies.

**Allow.** A keyboard or mouse no rule matches follows the focus. A head key left unset is on: the head's keyboards switch the VT (`vt-keys`), the focus may rest on it (`focus`), and the rescue key may take its keyboards to the focus (`rescue`). The rescue key and the anti-lockout guard are on. Rules take rights away: "this keyboard belongs to the desk", "the panel's keyboard cannot switch VTs". The question while writing one is what you must stop from reaching everything. A forgotten rule leaks: a stranger's keyboard plugged into a spare port types into someone's session. Today's console and the shipped rules are allow files.

**Deny.** A keyboard or mouse no rule matches is ignored, and a head has only the rights its section grants: unset, `vt-keys`, `focus` and `rescue` are off. The rescue key and the anti-lockout guard are off, so no keyboard is ever moved anywhere the file did not send it; what remains is what a head grants, such as an operator's head with `focus = on`. Rules grant rights: "this keyboard belongs to the kiosk", "the operator's head may switch VTs". The question is what you must let in so you don't shut yourself out. A forgotten rule locks out: your own keyboard is dead. A kiosk and stations are deny files. Once vtium has read `default = deny` in a file it trusts, nothing opens anything for the rest of the boot: no other line of the file, broken or not, and nothing that happens while vtium runs.

A display no rule matches stays dark under either posture, and the display guard lights the boot display when the rules light none: in an allow file for the first head, and in a deny file only to show a notice. `vtium check-config` lists, as notes, the keyboards, mice and displays present that no rule matches and what the posture does with them; in a deny file it says prominently when no keyboard present can reach any head, before the file is installed.

## Which input can reach whose session

Each route by which one keyboard can disturb another person's session has a switch of its own. In an allow file a route the file says nothing about is open, as on today's console; in a deny file it is closed until the file opens it.

| Route | Disturbs | Governed by |
|---|---|---|
| The VT keys, `Ctrl-Alt-F<n>` and `XF86Switch_VT_<n>` | every display, which leaves for another VT | the `vt-keys` of the head the keyboard asks: its own, or the head holding the focus for a keyboard following it; with no focus, none; unset, on under allow and off under deny |
| The console chords | the console the head shows | the head's `consoles`: with 1 there is nothing to switch |
| The focus chord | where the keyboards following the focus type | taken only from keyboards following the focus |
| The rescue key | takes a keyboard off its head, or brings an ignored one into the focus | the posture, on under allow and off under deny, and under allow the head's `rescue` |
| The anti-lockout guard | sends every keyboard to the focus | the posture: on under allow, off under deny |
| Unclaimed keyboards | type into the head holding the focus | the posture: under allow they follow the focus unless a rule such as `input * -> ignore` closes them, and under deny they are ignored |
| Arrival through the focus | the head the focus rests on | the head's `focus`, unset on under allow and off under deny |

Every keyboard asks a head: a claimed keyboard asks its own head, a keyboard following the focus asks the head holding the focus, and with no head to ask the answer is no. A keyboard following the focus therefore switches the VT only where the focused head's `vt-keys` is on, and with no focus it delivers nothing, the VT keys included.

## The operator's way back in

### An operator's head

A deny file gives the operator a keyboard at the machine only through a head of their own, on a display of its own:

```ini
[head admin]
vt-keys = on
focus = on

[rules]
display edid:DDD:0x0004:ADMIN00001 -> admin
input path:pci-0000:00:14.0-usb-0:4 -> admin
```

The `input` rule gives the operator a port of their own: a keyboard plugged in there types into `admin`, and its VT keys reach a getty on another VT. Without it, `input class:keyboard -> focus` lets any keyboard no other rule claims follow the focus to `admin`, which also lets anyone at the machine reach that login prompt. A machine with one display has no display to spare for such a head: its operator gets in from outside, or gives the one head `vt-keys = on`, so `Ctrl-Alt-F2` reaches a getty on another VT, where a password stands between a passer-by and a shell.

### Recovery from outside

A machine its head file locks out is recovered as any misconfigured Linux system is: over ssh, from a serial getty, through out-of-band management, or by booting other media and editing the file. vtium has no rescue boot of its own.

### Invalid files

vtium never fixes, renames or replaces the file, and resolves it in three steps:

1. **Trust.** A file that root does not own, that others can write, or that vtium cannot open is never read: the shipped rules run, and the console shows `invalid config skipped, using builtin`. A deny file left writable, or deleted, therefore opens the machine at the next boot.
2. **The posture.** vtium reads `default` from the top-level keys. A readable `default = deny` wins over any other `default` line. A `default` line it cannot read, inside a section or with an unknown value, counts as no posture, so allow, and is an error.
3. **The rest.** An error inside a head's section drops that head; a bad rule drops that rule alone; an error that leaves the file's structure untrustworthy drops all but the posture. If a head survives, vtium runs what survives and shows `config partly invalid, running what's valid`; in an allow file the shipped rules' keyboard, pointer and key classes follow after the surviving rules, and the rescue key is forced on. If nothing survives, an allow file runs the shipped rules with `invalid config skipped, using builtin`, and a deny file runs nothing.

A deny file with nothing left is vtium's panic screen. vtium has started correctly and can serve nothing; the boot display shows, in words like these, `vtium: config invalid, deny: input off`, that no head in the file survives, the first errors with their lines, and `nothing on this console takes input; the full report: vtium check-config /etc/vtium/heads.conf`. `check-config` rebuilds that report from the file itself, run over ssh or from a live USB with the disk mounted.

> The panic screen's meaning, in the operator's words: "you done fucked up, Ay-Ay-Ron. Bring a USB stick or an ssh key."

That failure is final by design. vtium keeps no copy and no last-known-good of the file, so a deny file is checked before it is relied on: with `vtium check-config FILE` today, and with `vtiumctl`'s trial, which applies a candidate and reverts it unless confirmed, once it is built.

## Under allow

The two setups below are allow files: anything they don't name follows the focus or stays dark, and each rule takes something away.

### The default: no file

Works today.

**Purpose.** A console that can be typed on with no configuration at all.

**The file.** None. vtium runs the compiled-in configuration, an allow file that reads as this one; `vtiumctl template` writes it with every other setting as a commented line.

```ini
version = 1

[head console]
profile = default

[rules]
display * -> console
input class:keyboard -> focus
input class:mouse -> focus
input class:touchpad -> focus
input class:touchscreen -> focus
input class:trackball -> focus
input class:pointingstick -> focus
input class:key -> focus
input * -> ignore
```

**What to expect.**

- Every display lights as one head, and every keyboard, pointer and other key device, numpads, macro pads and media keys among them, types into it. Joysticks, tablets, switches and accelerometers stay closed.
- The classes are udev's, read from its tags, and vtium derives the same classes itself where no udev daemon runs, so this file means the same either way.
- A display unplugged leaves the session running (`on-loss = nohup`, the default); it comes back when a display returns. With every display gone, vtium keeps running and waits for one.
- `Ctrl-Alt-F<n>` on any keyboard switches the VT, and with it every display.
- kmscon.conf's `mouse` off drops the pointer rules.

### Two heads with bound keyboards

Works today.

**Purpose.** Two displays of different sizes, a large monitor and a small panel, each with its own grid at its own density and its own keyboard, and the spare devices following the focus.

**The file.**

```ini
# /etc/vtium/heads.conf; the EDID values are illustrative
version = 1

[head desk]
profile = desk

[head panel]
profile = small
consoles = 2
on-loss = hangup 30s

[rules]
display edid:AAA:0x0001:LARGE00001 -> desk
display edid:BBB:0x0002:SMALL00001 -> panel
input path:pci-0000:15:00.0-usb-0:3.3 -> desk
input path:pci-0000:15:00.0-usb-0:3.4.4 -> desk
input path:pci-0000:15:00.0-usb-0:3.2 -> panel
input path:pci-0000:15:00.0-usb-0:3.4.2 -> focus
display * -> desk
input class:keyboard -> focus
input class:mouse -> focus
# no class:key rule: media-key devices stay closed
input * -> ignore
```

**What to expect.**

- The large monitor shows head `desk` and the small panel head `panel`, each with a grid sized for it. Any other display joins `desk`, on the same grid.
- The keyboard and mouse at ports 3.3 and 3.4.4 work only on `desk`, and the keyboard at port 3.2 types only into `panel`. Their console chords switch their own head's consoles; `panel` has two.
- The OTP token at port 3.4.2, and every keyboard and mouse no rule names, follow the focus, which the focus chord moves between the heads. Pressed on a bound keyboard, the focus chord moves nothing and reaches that head's session as a key.
- `Ctrl-Alt-F<n>` on any keyboard switches the VT, and with it both displays.
- The monitor unplugged leaves `desk`'s sessions running; they come back when it returns. The panel unplugged hangs `panel`'s sessions up 30 seconds later, as a line losing its carrier, unless it returns within the time. A head comes back only on a display its rules match.
- With no keyboard reaching a lit head, every keyboard follows the focus, with a notice, until one does. The rescue key moves the keyboard that pressed it to the focus until the next boot.

## Under deny

The two setups below are deny files: anything they don't name is ignored or dark, and each rule grants something. Each needs the operator's way in decided before it is installed ([The operator's way back in](#the-operators-way-back-in)).

### A kiosk

Not built yet: it needs the `default` key and the head keys `vt-keys` and `focus`.

**Purpose.** One display running one program, which no keyboard but its own reaches and none takes away from it.

**The file.**

```ini
# /etc/vtium/heads.conf; the identity values are illustrative
version = 1
default = deny

[head kiosk]
# login runs as root: drop to the kiosk user (runuser's path is your distribution's)
login = /usr/sbin/runuser -u kiosk -- /usr/local/bin/kiosk

[rules]
display edid:CCC:0x0003:KIOSK00001 -> kiosk
input path:pci-0000:00:14.0-usb-0:2 -> kiosk
```

**What to expect.**

- The kiosk's display runs the program, as the user `kiosk`; any other display stays dark.
- The keyboard at port 2 types only into the kiosk. The kiosk has one console and nothing to switch; `Ctrl-Alt-F<n>` for another VT reaches the program as keys; the rescue key pressed on it moves nothing and is swallowed.
- The posture gives the kiosk `vt-keys`, `focus` and `rescue` off, and the file grants none of them back. A keyboard plugged into another port is ignored, and nothing moves it anywhere.
- `vtium check-config` notes that no keyboard can switch the VT, that no head takes the focus, that the kiosk's only keyboard is one device, and the keyboards present on other ports as ignored, and exits 0.
- As written, the operator gets in only from outside, since the machine has one display.
- The display unplugged leaves the program running, and it comes back when the display returns.

### Stations: one person to each head

Not built yet: it needs the `default` key and the head keys `vt-keys` and `focus`.

**Purpose.** Several people working at once on one machine, each at their own display, keyboard and mouse: a multi-terminal host. A deny file makes it work, since every station's head then has `vt-keys`, `focus` and `rescue` off without saying so: `Ctrl-Alt-F<n>` would otherwise switch every station's display away, and a keyboard following the focus would type into someone's station.

**The file.**

```ini
# /etc/vtium/heads.conf: three stations; the identity values are illustrative
version = 1
default = deny

[head station1]
[head station2]
[head station3]

[rules]
display edid:AAA:0x0001:STATION001 -> station1
display edid:AAA:0x0001:STATION002 -> station2
display edid:AAA:0x0001:STATION003 -> station3
input path:pci-0000:15:00.0-usb-0:1 -> station1
input path:pci-0000:15:00.0-usb-0:2 -> station2
input path:pci-0000:15:00.0-usb-0:3 -> station3
```

**What to expect.**

- A station is a USB port: a hub on it carries that station's keyboard and mouse, and anything plugged into the hub joins the station. A device bound by `serial:` in place of `path:` follows its unit to any port.
- Each station's display shows its own head, with its own login, and its keyboard and mouse work only there. A device on no station's port is ignored, and a display no rule names stays dark.
- `Ctrl-Alt-F<n>` and `XF86Switch_VT_<n>` for another VT, pressed on a station's keyboard, switch nothing and reach that station's session as keys, as a serial terminal sends them. The focus chord reaches the session the same way. The rescue key moves nothing and is swallowed.
- No station takes the focus, so there is no focus, and no keyboard is moved between stations.
- `vtium check-config` notes that no keyboard can switch the VT and that no head takes the focus, lists the devices present on no station's port as ignored, and exits 0.
- As written, the operator gets in only from outside. An operator's head on a display of its own gives a way in at the machine; the VT keys pressed there switch every station's display.
- A station's display unplugged leaves its sessions running unseen (`nohup`, the default), and its keyboard still reaches them; they come back when that display returns. For stations people walk away from, `on-loss = hangup` ends the sessions when the display goes, and `on-loss = hangup 30s` after a grace period.
- A station's keyboard unplugged leaves its session running, and the keyboard plugged into the same port returns to it.
- More than 8 stations need kmscon.conf's `consoles-max` raised.

**What stays shared.**

- One vtium process, running as root, serves every station, so a crash ends every station's sessions at once, and the unit restarts vtium or starts a getty. Sessions that survive a vtium restart are planned next after heads.
- One kernel VT is shown on every station's display, so anything that switches the VT switches every station.
- A station's login runs on a pty and gets a logind session with no seat, so no seat-bound device, such as audio, reaches it.
- A fully independent station, a logind seat with its own cards and inputs, is the seat extension of ADR-1401 D2, which is not built.
