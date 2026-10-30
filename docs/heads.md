# Heads, by scenario

This page shows the head file, `/etc/vtium/heads.conf`, in the setups it is made for: each scenario's purpose, a complete head file, and what to expect, on an unplug and of what stays shared. Heads are in vtium, as [ADR-1401](architecture/platform/ADR-1401-heads-a-terminal-per-physical-display-placed-by-a-first-match-ruleset.md) decided them; four pieces this page uses are not built yet: the head file's `default` key and the head keys `vt-keys` and `focus`, which the kiosk and the stations need, and `vtiumctl`. ADR-1401 holds every rule; this page applies them.

## Two postures: allow and deny

A head file decides which keyboards reach which screens, as a firewall decides which traffic passes, and like a firewall it has a default policy. The rules read the same under either policy; the policy decides what happens to every keyboard, mouse and screen the rules don't mention. It is the line after `version = 1`: `default = allow` or `default = deny`. A file without it is an allow file.

**Allow.** Anything the file doesn't mention gets everything: a keyboard or mouse no rule names follows the focus and types into whichever screen holds it, every screen's keyboards can switch the VT, the focus can rest on every head, and the rescue key can take any keyboard back to the focus. Rules take rights away: "this keyboard belongs to the desk", "the panel's keyboard cannot switch VTs". The question to ask while writing one is what you must stop from reaching everything. A forgotten rule leaks: a stranger's keyboard plugged into a spare port types into someone's session. Today's console and the shipped rules are allow files. `vtium check-config` lists the keyboards and mice present that no rule names, which follow the focus, and the screens no rule names, which stay dark.

**Deny.** Anything the file doesn't mention gets nothing: a keyboard or mouse no rule names is ignored, and a head has only the rights its section grants. A head that doesn't say otherwise takes `vt-keys = off`, so its keyboards cannot switch the VT, `focus = off`, so the focus never rests on it, and `rescue = off`, so the rescue key leaves its keyboards where they are. Rules grant rights: "this keyboard belongs to the kiosk", "the operator's head may switch VTs". The question to ask is what you must let in so you don't shut yourself out. A forgotten rule locks out: your own keyboard is dead. A kiosk and stations are deny files. `vtium check-config` lists the keyboards and mice present that no rule names, which are ignored, and says prominently when no keyboard present can reach any head, before the file is installed.

A screen no rule names stays dark under either policy. Every `check-config` listing above is a note, and the check still exits 0.

## Reading a head file

A head file starts with `version = 1`, and the line after it may set `default`. Each `[head NAME]` section declares a head: a terminal for one or more displays, with its own grid, its own profile and its own virtual consoles. Under `[rules]`, each line is `display MATCH -> TARGET` or `input MATCH -> TARGET`; each display and each input walks the rules from the top on its own, and the first that matches places it in a head, in the focus, ignored (`ignore`) or dark (`dark`). `[guards]` holds the switches of the guards against lockout, each on unless the file turns it off.

vtium reads the file only when root owns it and no one else can write it. With no file it runs its compiled-in configuration and writes nothing. An invalid file is reported and left as it is, and what runs in its place follows its policy: vtium reads `version` and `default` first, so a broken deny file shows its error on the boot screen and lets no keyboard type, while a broken allow file, or one broken before its `default` line, is replaced in memory by the shipped rules. `vtium check-config FILE` checks a candidate before it is installed. `vtiumctl detect` lists every display and input with a rule line ready to paste; the identity values in the files below are illustrative, and real ones come from it.

A rule names a device most strongly by its USB port (`path:`), its unit (`serial:`) or its monitor (`edid:`), and only for convenience by its model (`id:`) or its name (`name:`). A `path:` match takes every interface behind that port, so a hub on the port joins with it; `serial:` binds the unit wherever it is plugged.

## Which input can reach whose session

Each route by which one keyboard can disturb another person's session has a switch of its own. In an allow file a route the file says nothing about is open, as on today's console; in a deny file a route the policy or a head key governs is closed until the file opens it. The switches under `[guards]` are the same under both.

| Route | Disturbs | Governed by |
|---|---|---|
| The VT keys, `Ctrl-Alt-F<n>` and `XF86Switch_VT_<n>` | every display, which leaves for another VT | the `vt-keys` of the head the keyboard asks: its own, or the focused head for a keyboard following the focus; with no focus, none; unset, on under allow and off under deny |
| The console chords | the console the head shows | the head's `consoles`: with 1 there is nothing to switch |
| The focus chord | where the keyboards following the focus type | taken only from keyboards following the focus |
| The rescue key | takes a keyboard off its head, or brings an ignored one into the focus | `[guards] rescue-key`, and the head's `rescue`, unset on under allow and off under deny |
| The anti-lockout guard | sends every keyboard to the focus | `[guards] anti-lockout` |
| Unclaimed keyboards | type into the focused head | the policy: under allow they follow the focus unless a rule such as `input * -> ignore` closes them, and under deny they are ignored |
| Arrival through the focus | the head the focus rests on | the head's `focus`, unset on under allow and off under deny |

Every keyboard asks a head: a claimed keyboard asks its own head, a keyboard following the focus asks the head holding the focus, and with no head to ask the answer is no. A keyboard following the focus therefore switches the VT only where the focused head's `vt-keys` is on, and with no focus it delivers nothing, the VT keys included.

Each right an allow file removes, and each a deny file leaves closed, is a note in `vtium check-config`, which still exits 0. The machine is the operator's, and so is a deliberate lockout: a machine its head file locks out is recovered as any misconfigured Linux system is, over ssh, from a serial getty, through out-of-band management, or by booting other media and editing the file. vtium has no rescue boot of its own.

## Under allow

The two setups below are allow files: anything they don't mention follows the focus or stays dark, and each rule takes something away.

### The default: no file

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

- Every display lights, mirrored as one grid, and every keyboard, pointer and other key device, numpads, macro pads and media keys among them, types into it. Joysticks, tablets, switches and accelerometers stay closed.
- The classes are udev's, read from its tags, and vtium derives the same classes itself where no udev daemon runs, so this file means the same either way.
- A display unplugged leaves the session running (`on-loss = nohup`, the default); it comes back when a display returns. With every display gone, vtium keeps running and waits for one.
- `Ctrl-Alt-F<n>` on any keyboard switches the VT, and with it every display.
- kmscon.conf's `mouse` off drops the pointer rules.

### Two heads with bound keyboards

**Purpose.** Two displays of different sizes, a large monitor and a small panel, each with its own grid at its own density and its own keyboard, and the spare devices following the focus.

**The file.**

```ini
# /etc/vtium/heads.conf; the EDID values are illustrative
version = 1
default = allow

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

- The large monitor shows head `desk` and the small panel head `panel`, each with a grid sized for it. Any other display joins `desk`, mirrored by the scaled rule.
- The keyboard and mouse at ports 3.3 and 3.4.4 work only on `desk`, and the keyboard at port 3.2 types only into `panel`. Their console chords switch their own head's consoles; `panel` has two.
- The OTP token at port 3.4.2, and every keyboard and mouse no rule names, follow the focus, which the focus chord moves between the heads. The chord's name and default are vtium/vtium#82's. Pressed on a bound keyboard, the focus chord moves nothing and reaches that head's session as a key.
- Each head shows whether it holds the focus.
- `Ctrl-Alt-F<n>` on any keyboard switches the VT, and with it both displays.
- The monitor unplugged leaves `desk`'s sessions running; they come back when it returns. The panel unplugged hangs `panel`'s sessions up 30 seconds later, as a line losing its carrier, unless it returns within the time. A head comes back only on a display its rules match.
- With no keyboard reaching a lit head, every keyboard follows the focus, with a notice, until one does. `Ctrl-Alt-F<n>` for the VT already shown moves the keyboard that pressed it to the focus until the next boot.

## Under deny

The two setups below are deny files: anything they don't mention is ignored or dark, and each rule grants something. Each needs a way in for the operator, decided before the file is installed: a head of the operator's own, on a screen of its own, that grants itself the VT switch and the focus, or else only the recovery outside vtium described above.

### A kiosk

**Purpose.** One display running one program, which no keyboard but its own reaches and none takes away from it.

**The file.**

```ini
# /etc/vtium/heads.conf; the identity values are illustrative
version = 1
default = deny

[head kiosk]
login = /usr/local/bin/kiosk

[rules]
display edid:CCC:0x0003:KIOSK00001 -> kiosk
input path:pci-0000:00:14.0-usb-0:2 -> kiosk
```

**What to expect.**

- The kiosk's display runs the program `login` names; any other display stays dark.
- The keyboard at port 2 types only into the kiosk. The kiosk has one console, the value when unset, and nothing to switch; `Ctrl-Alt-F<n>` for another VT reaches the program as keys; the rescue key pressed on it moves nothing.
- The policy gives the kiosk `vt-keys = off`, `focus = off` and `rescue = off`, and the file grants none of them back. The focus never rests on the kiosk, and no other head takes it, so there is no focus. A keyboard plugged into another port is ignored; one the rescue key moves, or every keyboard the anti-lockout guard moves while the kiosk's own is unplugged, follows the focus and delivers nothing, the VT keys included.
- `vtium check-config` notes that no keyboard can switch the VT and that no head takes the focus, lists the keyboards present on other ports as ignored, and exits 0.
- The machine-wide guards stay on and move no keyboard anywhere it can act.
- As written, the operator has no keyboard at the machine, only the routes outside vtium. A head of the operator's own gives one back:

  ```ini
  [head admin]
  vt-keys = on
  focus = on
  ```

  with a rule for its own screen (`display edid:... -> admin`). The focus then rests on `admin`, so the rescue key, `Ctrl-Alt-F<n>` for the VT already shown, brings any other keyboard that presses it to `admin`'s login, and with no keyboard reaching a lit head the anti-lockout guard sends every keyboard there; anyone at the machine can reach that login prompt. `rescue-key = off` under `[guards]` closes the first.
- The display unplugged leaves the program running, and it comes back when the display returns.

### Stations: one person to each head

**Purpose.** Several people working at once on one machine, each at their own display, keyboard and mouse: a multi-terminal host. Off by default; a deny file makes it work, since every station's head then has `vt-keys`, `focus` and `rescue` off without saying so: `Ctrl-Alt-F<n>` would otherwise switch every station's display away, and a keyboard following the focus would type into someone's station.

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
- `Ctrl-Alt-F<n>` and `XF86Switch_VT_<n>` for another VT, pressed on a station's keyboard, switch nothing and reach that station's session as keys, as a serial terminal sends them. The focus chord reaches the session the same way. The rescue key pressed on a station's keyboard moves nothing.
- No station takes the focus, so there is no focus: a keyboard the rescue key moves, and every keyboard the anti-lockout guard moves once every station's keyboard is gone, deliver nothing, the VT keys included.
- `vtium check-config` notes that no keyboard can switch the VT and that no head takes the focus, lists the devices present on no station's port as ignored, and exits 0.
- The machine-wide guards stay on and move no keyboard anywhere it can act, so no station's user can switch the others' displays away.
- As written, the operator has no keyboard at the machine, only the routes outside vtium. The `admin` head of the kiosk, on a screen of its own, gives one back. With it, a keyboard plugged in on no station's port reaches `admin`'s login through the rescue key, while the stations' own keyboards stay put, their heads keeping `rescue = off`; and the VT keys pressed in `admin` switch every station's display.
- A station's display unplugged leaves its sessions running unseen (`nohup`, the default), and its keyboard still reaches them; they come back when that display returns. For stations people walk away from, `on-loss = hangup` ends the sessions when the display goes, and `on-loss = hangup 30s` after a grace period.
- A station's keyboard unplugged leaves its session running, and the keyboard plugged into the same port returns to it.
- The default caps allow 4 consoles a head and 8 in total, so more than 8 stations need the total raised, up to the hard maxima of 16 heads and 50 consoles.

**What stays shared.**

- One vtium process, running as root, serves every station, so a crash ends every station's sessions at once, and the unit restarts vtium or starts a getty. Sessions that survive a vtium restart come directly after heads in the build order.
- One kernel VT is shown on every station's display, so anything that switches the VT switches every station.
- A station's login runs on a pty and gets a logind session with no seat, so no seat-bound device, such as audio, reaches it.
- A fully independent station, a logind seat with its own cards and inputs, is the seat extension of ADR-1401 D2, which is not built.
