# Heads, by scenario

This page shows the head file, `/etc/vtium/heads.conf`, in the setups it is made for: each scenario's purpose, a complete head file, and what to expect, on an unplug and of what stays shared. Heads are the design [ADR-1401](architecture/platform/ADR-1401-heads-a-terminal-per-physical-display-placed-by-a-first-match-ruleset.md) decided, and neither heads nor `vtiumctl` is built yet ([architecture.md](architecture.md)): vtium today reads no head file and draws one grid mirrored across every display. ADR-1401 holds every rule; this page applies them.

## Reading a head file

A head file starts with `version = 1`. Each `[head NAME]` section declares a head: a terminal for one or more displays, with its own grid, its own profile and its own virtual consoles. Under `[rules]`, each line is `display MATCH -> TARGET` or `input MATCH -> TARGET`; each display and each input walks the rules from the top on its own, and the first that matches places it in a head, in the focus, ignored (`ignore`) or dark (`dark`). `[guards]` holds the switches of the guards against lockout, each on unless the file turns it off.

vtium reads the file only when root owns it and no one else can write it. With no file it runs its compiled-in configuration and writes nothing; an invalid file is reported, left as it is, and replaced in memory by the shipped rules. `vtium check-config FILE` checks a candidate before it is installed. `vtiumctl detect` lists every display and input with a rule line ready to paste; the identity values in the files below are illustrative, and real ones come from it.

A rule names a device most strongly by its USB port (`path:`), its unit (`serial:`) or its monitor (`edid:`), and only for convenience by its model (`id:`) or its name (`name:`). A `path:` match takes every interface behind that port, so a hub on the port joins with it; `serial:` binds the unit wherever it is plugged.

## The default: no file

**Purpose.** A console that can be typed on with no configuration at all.

**The file.** None. vtium runs the compiled-in configuration, which reads as this file; `vtiumctl template` writes it with every other setting as a commented line.

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

## Two heads with bound keyboards

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

- The large monitor shows head `desk` and the small panel head `panel`, each with a grid sized for it. Any other display joins `desk`, mirrored by the scaled rule.
- The keyboard and mouse at ports 3.3 and 3.4.4 work only on `desk`, and the keyboard at port 3.2 types only into `panel`. Their console chords switch their own head's consoles; `panel` has two.
- The OTP token at port 3.4.2, and every keyboard and mouse no rule names, follow the focus, which the focus chord moves between the heads. The chord's name and default are vtium/vtium#82's. Pressed on a bound keyboard, the focus chord moves nothing and reaches that head's session as a key.
- Each head shows whether it holds the focus.
- `Ctrl-Alt-F<n>` on any keyboard switches the VT, and with it both displays.
- The monitor unplugged leaves `desk`'s sessions running; they come back when it returns. The panel unplugged hangs `panel`'s sessions up 30 seconds later, as a line losing its carrier, unless it returns within the time. A head comes back only on a display its rules match.
- With no keyboard reaching a lit head, every keyboard follows the focus, with a notice, until one does. `Ctrl-Alt-F<n>` for the VT already shown moves the keyboard that pressed it to the focus until the next boot.

## A kiosk

**Purpose.** One display running one program, whose keyboard the rescue key never takes away.

**The file.**

```ini
# /etc/vtium/heads.conf; the identity values are illustrative
version = 1

[head kiosk]
login = /usr/local/bin/kiosk
rescue = off

[rules]
display edid:CCC:0x0003:KIOSK00001 -> kiosk
input path:pci-0000:00:14.0-usb-0:2 -> kiosk
display * -> dark
input * -> ignore
```

**What to expect.**

- The kiosk's display runs the program `login` names; any other display is set off.
- The keyboard at port 2 types only into the kiosk, and the rescue key pressed on it moves nothing. Every other input is ignored.
- A keyboard plugged into another port is opened only to watch for the rescue key, so `Ctrl-Alt-F<n>` for the shown VT pressed on it makes it follow the focus, which rests on the kiosk. While the kiosk's own keyboard is unplugged, the anti-lockout guard sends every keyboard to the kiosk. A kiosk that must refuse both sets `rescue-key = off` and `anti-lockout = off` under `[guards]`, at the risk of a lockout; `vtium.heads=builtin` on the kernel command line runs the compiled-in rules with every guard on for that boot.
- `Ctrl-Alt-F<n>` on the kiosk's keyboard switches the VT; `vt-keys = off` on the head passes it to the program instead, as for a station.
- The display unplugged leaves the program running, and it comes back when the display returns.

## Stations: one person to each head

**Purpose.** Several people working at once on one machine, each at their own display, keyboard and mouse: a multi-terminal host. Off by default; the head key `vt-keys = off` makes it work, since `Ctrl-Alt-F<n>` would otherwise switch every station's display away.

**The file.**

```ini
# /etc/vtium/heads.conf: three stations; the identity values are illustrative
version = 1

[head station1]
rescue = off
vt-keys = off

[head station2]
rescue = off
vt-keys = off

[head station3]
rescue = off
vt-keys = off

[rules]
display edid:AAA:0x0001:STATION001 -> station1
display edid:AAA:0x0001:STATION002 -> station2
display edid:AAA:0x0001:STATION003 -> station3
input path:pci-0000:15:00.0-usb-0:1 -> station1
input path:pci-0000:15:00.0-usb-0:2 -> station2
input path:pci-0000:15:00.0-usb-0:3 -> station3
display * -> dark
input * -> ignore

[guards]
rescue-key = off
```

**What to expect.**

- A station is a USB port: a hub on it carries that station's keyboard and mouse, and anything plugged into the hub joins the station. A device bound by `serial:` in place of `path:` follows its unit to any port.
- Each station's display shows its own head, with its own login, and its keyboard and mouse work only there. A device on no station's port is ignored, and a display no rule names is set off.
- `Ctrl-Alt-F<n>` and `XF86Switch_VT_<n>` for another VT, pressed on a station's keyboard, switch nothing and reach that station's session as keys, as a serial terminal sends them. The focus chord reaches the session the same way. The rescue key moves nothing.
- `vtium check-config` warns of this file that no keyboard can switch the VT, and exits 1 as for any warning about the file. `vtium.heads=builtin` on the kernel command line is the way back to the compiled-in rules.
- A station's display unplugged leaves its sessions running unseen (`nohup`, the default), and its keyboard still reaches them; they come back when that display returns. For stations people walk away from, `on-loss = hangup` ends the sessions when the display goes, and `on-loss = hangup 30s` after a grace period.
- A station's keyboard unplugged leaves its session running, and the keyboard plugged into the same port returns to it. With every station's keyboard gone, the anti-lockout guard sends every keyboard to the focused station until one returns.
- The default caps allow 4 consoles a head and 8 in total, so more than 8 stations need the total raised, up to the hard maxima of 16 heads and 50 consoles.

**What stays shared.**

- One vtium process, running as root, serves every station, so a crash ends every station's sessions at once, and the unit restarts vtium or starts a getty. Sessions that survive a vtium restart come directly after heads in the build order.
- One kernel VT is shown on every station's display, so anything that switches the VT switches every station.
- A station's login runs on a pty and gets a logind session with no seat, so no seat-bound device, such as audio, reaches it.
- A fully independent station, a logind seat with its own cards and inputs, is the seat extension of ADR-1401 D2, which is not built.
