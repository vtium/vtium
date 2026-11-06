# Heads, by scenario

This page shows the sections of vtium's configuration file, `/etc/vtium/vtium.conf`, in the setups they are made for: each scenario's purpose, the sections it needs, and what to expect, on an unplug and of what stays shared. The sections are the heads, their looks and the rules, which come below vtium's settings in vtium.conf. Each scenario's text is also a whole vtium.conf on its own, which leaves every setting at its default, so you can check it with `vtium check-config FILE`, or add your settings above its `version` line. [Configure vtium](configure.md) explains how to put it in place, and [head-file.md](head-file.md) is the reference for the sections, with its [terms](head-file.md#terms) such as the active head and the rescue key, its syntax and its match keys, and [Heads](concepts/heads.md#two-policies-open-and-closed) explains the two policies the scenarios are sorted by. [ADR-1401](architecture/platform/ADR-1401-heads-a-terminal-per-physical-display-placed-by-a-first-match-ruleset.md) holds every rule; this page applies them. Each scenario's file is also a file of its own under [heads/](heads/), and the `edid:` and `path:` values in them are illustrative.

## Under the open policy

### The default

**Purpose.** A console that can be typed on with no configuration at all.

With no section in vtium.conf, or no vtium.conf at all, this is what vtium runs. The shipped example, `/etc/vtium/vtium.conf.example`, lists every setting at its default, commented out, and then these sections.

```ini
# The sections of /etc/vtium/vtium.conf that vtium runs when the file has
# none: the compiled-in configuration, written out. Leaving them out gives
# exactly the same behaviour.
version = 1
policy = open              # open: heads are not isolated, and an input no rule places roams

[head console]             # one head, named "console"
                           # every head key is unset, so:
                           #   consoles = 1, look = default,
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
- vtium.conf's `mouse` off drops the pointer rules.

### Two heads with bound keyboards

**Purpose.** Two displays of different sizes, a large monitor and a small panel, each with its own grid at its own density and its own keyboard, and the spare devices roaming.

```ini
version = 1
policy = open

[look desk]
font-name = desk           # the frit desk draws with

[look small]
font-name = small

[head desk]
look = desk                # its font, from the look named desk

[head panel]
look = small
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

Each closed file needs the operator's way in decided before it is installed ([the operator's way back in](concepts/heads.md#the-operators-way-back-in)).

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
- The display unplugged leaves the program running, and it comes back when the display returns. With the kiosk's monitor absent at boot and another display connected, that display shows `No head has a connected display.`; with no display connected at all, vtium holds its VT and waits.

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
- More than 8 stations' consoles need vtium.conf's `consoles-max` raised.

**What stays shared.**

- One vtium process, running as root, serves every station, so a crash ends every station's sessions at once. Under the closed policy no getty takes the VT then, so every station's display shows no login until vtium starts again: when the operator starts its unit, `vtium@ttyN`, or, where logind starts vtium as `autovt@ttyN`, when someone switches away from its VT and back. Sessions that survive a vtium restart are planned next after heads.
- One kernel VT is shown on every station's display, so anything that switches the VT switches every station.
- A station's login runs on a pty and gets a logind session with no seat, so no seat-bound device, such as audio, reaches it.
- A fully independent station, a logind seat with its own cards and inputs, is the seat extension of ADR-1401 D2, which is not built.

### Everything off

**Purpose.** vtium holds its VT and draws nothing, for a machine whose console must show nothing. It opens no keyboard, so no key can switch the VT.

```ini
version = 1
policy = closed
# No heads and no rules: vtium lights no display and opens no input.
# vtium check-config refuses this file, since no keyboard can reach a
# display, so vtium-config never installs it.
```

[heads/off.conf](heads/off.conf)

The boot display stays dark too: the display guard lights it under the closed policy only for a file with heads.
