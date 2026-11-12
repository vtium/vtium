# Heads, by scenario

This page shows the sections of vtium's configuration file, `/etc/vtium/vtium.conf`, in the setups they are made for. The sections come below vtium's settings in vtium.conf. They declare the heads, their looks and the rules. A head is a terminal for one or more displays. A look is the font and the font settings that a head draws with. The rules place each display and each input device in a head.

Each scenario gives its purpose and the sections it needs. It then says what to expect, such as what happens when you unplug something and what stays shared. Each scenario's text is also a whole vtium.conf on its own, which leaves every setting at its default. So you can check it with `vtium check-config FILE`, or add your settings above its `version` line. Each scenario's file is also a file of its own under [heads/](heads/). The `edid:` and `path:` values in the files are illustrative.

[Configure vtium](configure.md) explains how to put a file in place. [head-file.md](head-file.md) is the reference for the sections. It defines [terms](head-file.md#terms) such as the active head and the rescue key, and it gives the syntax and the match keys. [Heads](concepts/heads.md#two-policies-open-and-closed) explains the two policies, open and closed, which sort the scenarios below.

## Under the open policy

### The default

**Purpose.** A console you can type on with no configuration at all.

This is what vtium runs when vtium.conf has no section, or when there is no vtium.conf at all. The shipped example, `/etc/vtium/vtium.conf.example`, lists every setting at its default, commented out. After the settings, it lists these sections.

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
- The classes are udev's, read from udev's tags. Where no udev daemon runs, vtium derives the same classes itself, so this file means the same either way.
- When you unplug a display, the session keeps running. It comes back when a display returns. With every display gone, vtium keeps running and waits for one.
- `Ctrl-Alt-F<n>` on any keyboard switches the VT, and with it every display.
- When vtium.conf's `mouse` setting is off, vtium drops the pointer rules.

### Two heads with bound keyboards

**Purpose.** Two displays of different sizes, a large monitor and a small panel. Each display has its own grid at its own density, and its own keyboard. The spare devices roam.

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

- The large monitor shows head `desk`, and the small panel shows head `panel`. Each head has a grid sized for its display.
- On keyboard A, the console chords do nothing, since `desk` has one console. On keyboard B, they switch between `panel`'s two consoles. A bound keyboard is one that a rule gives to a head. When you press the roaming chord on a bound keyboard, it moves nothing, and it reaches that head's session as a key.
- `Ctrl-Alt-F<n>` on any keyboard switches the VT, and with it both displays.
- When you unplug the monitor, `desk`'s sessions keep running. They come back when the monitor returns. A head comes back only on a display that its rules match.
- When you unplug the panel, keyboard B goes on typing into `panel`'s console, unseen. After 30 s, vtium hangs up `panel`'s sessions. If the panel comes back within the 30 s, it brings the sessions back. If it comes back after 60 s, it shows `panel`'s consoles started afresh, at their login. The anti-lockout guard does not fire, because keyboard A still reaches `desk`, which is lit. If `panel` was the active head, roaming keyboards type into it while it is not lit, until the roaming chord moves them.
- While no keyboard reaches a lit head, the anti-lockout guard makes every keyboard roam and shows a notice. The keyboards roam until one of them reaches a lit head. The rescue key makes the keyboard that pressed it roam for the rest of the boot.

## Under the closed policy

Before you install a closed file, decide how the operator gets back in. See [the operator's way back in](concepts/heads.md#the-operators-way-back-in).

### A kiosk

**Purpose.** One display that runs one program. Only the kiosk's own keyboard reaches the program, and no keyboard can take the display away from it.

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

- The kiosk has one console. `Ctrl-Alt-F<n>` for another VT reaches the program as keys. The rescue key, pressed on the kiosk's keyboard, moves nothing, and vtium swallows it.
- `vtium check-config` exits 0. It notes that no keyboard can switch the VT, that no head can be the active head, and that only one keyboard reaches the kiosk. It also notes the keyboards present on other ports as ignored.
- As written, the operator gets in only from outside, since the machine has one display.
- When you unplug the display, the program keeps running. It comes back when the display returns. If the kiosk's monitor is absent at boot and another display is connected, that display shows `No head has a connected display.` If no display is connected at all, vtium keeps its VT and waits.

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

- The kiosk behaves as in the file above. `admin` is the active head, and it is the only head that can be.
- A keyboard on the spare port, 3.3, roams. `admin` is the active head, so that keyboard acts as `admin`'s keyboard. It types into `admin`'s login, and its `Ctrl-Alt-F<n>` switches the VT, which takes the kiosk's display away too. Switching the VT is the admin head's right, and the spare-port rule lends it to whatever is plugged into that port.
- `Ctrl-Alt-F<n>` pressed on the operator's keyboard, on port 3.2, switches the VT the same way.
- The rescue key is off under the closed policy, so vtium ignores a keyboard on any other port, and its keys reach no session.

### Stations: one person to each head

**Purpose.** Several people working at once on one machine, each at their own display, keyboard and mouse. This setup is a multi-terminal host, and it needs the closed policy. Without it, `Ctrl-Alt-F<n>` would switch every station's display away, and a roaming keyboard would type into someone's station.

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

- Each station's display shows its own head, with its own login. The station's keyboard and mouse work only there. vtium ignores a device that is on no station's port, and it turns off a display that no rule names.
- When you press `Ctrl-Alt-F<n>` or `XF86Switch_VT_<n>` for another VT on a station's keyboard, it reaches that station's session as keys, as a serial terminal sends them. The roaming chord reaches the session the same way. The rescue key moves nothing, and vtium swallows it.
- No station can be the active head, so vtium never moves a keyboard between stations.
- `vtium check-config` exits 0. It notes that no keyboard can switch the VT and that no head can be the active head. It lists the devices present on no station's port as ignored.
- As written, the operator gets in only from outside. An operator's head on a display of its own gives a way in at the machine. The VT keys pressed on that head's keyboard switch every station's display.
- When a station's display is unplugged, its sessions keep running, unseen, because `when-display-lost = keep` is the default. The station's keyboard still reaches them, and they come back when that display returns. For stations that people walk away from, `hangup` or `hangup 30s` ends the sessions when the display goes.
- When a station's keyboard is unplugged, its session keeps running. When the keyboard is plugged into the same port again, it returns to that session.
- When the stations' consoles add up to more than 8, raise vtium.conf's `consoles-max`.

**What stays shared.**

- One vtium process, running as root, serves every station, so a crash ends every station's sessions at once. Under the closed policy, no getty takes the VT after a crash. So every station's display shows no login prompt until vtium starts again. vtium starts again when the operator starts its unit, `vtium@ttyN`. Where logind starts vtium as `autovt@ttyN`, vtium also starts again when someone switches away from its VT and back. Sessions that survive a vtium restart are planned next after heads.
- One kernel VT is shown on every station's display, so anything that switches the VT switches every station.
- A station's login runs on a pty, a pseudo-terminal, and gets a logind session with no seat. So no device that is bound to a seat, such as audio, reaches it.
- A fully independent station would be a logind seat with its own graphics cards and inputs. It would need vtium to run on a seat other than the first, and vtium does not do that yet.

### Everything off

**Purpose.** A machine whose console must show nothing. vtium keeps its VT and draws nothing. It opens no keyboard, so no key can switch the VT.

```ini
version = 1
policy = closed
# No heads and no rules: vtium lights no display and opens no input.
# vtium check-config refuses this file, since no keyboard can reach a
# display, so vtium-config never installs it.
```

[heads/off.conf](heads/off.conf)

The boot display stays dark too. Under the closed policy, the display guard lights the boot display only for a file that declares heads.
