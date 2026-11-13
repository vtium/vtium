# From keyboards to displays

vtium can drive several displays and many keyboards from one VT. You decide which keyboard types into which display in vtium's configuration file, `/etc/vtium/vtium.conf`. This page explains the idea behind those lines. Each picture draws one configuration as a flow, from the displays on the left to the keyboards and mice on the right. The pictures come from the Flow view of vtium-config, vtium's configuration tool. See [Configure vtium](../configure.md) for the tool itself.

## The parts of a path

On a desktop computer, the monitor shows a window, and the keyboard types into that window. vtium joins the same kinds of things on the console:

- A **display** is a monitor or a panel, on one connector of a graphics card.
- A **head** is a terminal for one or more displays. The Flow view calls a head a terminal. Each head has its own grid of character cells and its own consoles.
- A **console** is one login session of a head, with its own screen and scrollback. A head shows one of its consoles at a time.
- A **look** is the font set and the font size that a head renders with. A head that names no look uses the default look.
- An **input** is a keyboard, a mouse, or another input device.

A **path** joins these parts. One or more displays show a head. The head renders with its look. Inputs type into the head. In vtium.conf, a path is one `[head NAME]` section and the rules that name that head.

## Rules and the policy

The rules are the lines under `[rules]`. Each rule names some displays or some inputs and says where they go. For example, `input path:pci-0000:15:00.0-usb-0:3.2 -> admin` sends whatever is plugged into USB port 3.2 to the head `admin`.

vtium checks each display and each input against the rules on its own, from the top. The first rule that matches decides where the device goes. A rule can send a device to one of four places:

- a head, by its name,
- `roaming`, so that the input types into the active head,
- `ignore`, so that the input's keys reach no session, or
- `off`, so that the display shows nothing.

vtium places a device by the device's own identity and the rules alone. So a keyboard that you plug in later goes where it would have gone at start.

An input that **roams** belongs to no head of its own. It types into the **active head**, which is one head at a time, so it follows you from head to head. The roaming chord, `Ctrl-Super-Up` or `Ctrl-Super-Down` on a roaming keyboard, makes another head the active head. A keyboard that a rule gives to a head stays with that head when the active head moves.

A head can be **isolated**. An isolated head is never the active head, so no roaming input types into it. On an isolated head's keyboards, the keys that switch the VT reach the session as plain keys.

The **policy** decides what happens to a device that no rule names. It is `policy = open` or `policy = closed`, and a file that does not set it is open.

- Under both policies, vtium turns off a display that no rule names.
- Under the open policy, an input that no rule names roams. A head is not isolated unless its section says `isolated = yes`.
- Under the closed policy, vtium ignores an input that no rule names. Every head is isolated unless its section says `isolated = no`.

The Flow view states the policy on its third line. Under the open policy, two guards also keep you from locking yourself out. See [Two policies: open and closed](heads.md#two-policies-open-and-closed) for details.

## The default: one head on every display

When vtium.conf has no sections, vtium runs its built-in configuration. These lines write it out:

```ini
version = 1
policy = open

[head console]

[rules]
display * -> console
input class:keyboard -> roaming
input class:mouse -> roaming
input class:touchpad -> roaming
input class:touchscreen -> roaming
input class:trackball -> roaming
input class:pointingstick -> roaming
input class:key -> roaming
input * -> ignore
```

![vtium-config's Flow view of the built-in configuration. The first row joins the box All displays with a solid arrow to the head console, default font set, every VT, and a dotted line from the words roaming inputs reaches console. The row ROAMING joins All keyboards and All mice and touchpads with dotted arrows to a dotted box, Active terminal. The last row joins All input devices with a solid arrow to Nowhere: displays off, inputs ignored.](../figures/concepts-flow-default.png)

The picture shows three rows:

- The first row is the head `console`. The rule `display * -> console` puts every display in it, connected now or later. The head names no look, so it renders with the default font set. It has no `vt` key, so it runs on every VT.
- The row ROAMING holds the inputs that roam. The rules for the keyboard and key classes show as one box, All keyboards. The rules for the pointer classes show as All mice and touchpads. Their dotted box, Active terminal, stands for whichever head is active. The words roaming inputs beside `console` say that the roaming inputs reach it. Here, `console` is the only head, so it is always the active head.
- The last row is `input * -> ignore`, drawn as a path to Nowhere. vtium ignores every other input, such as a joystick or a tablet.

[heads.md](../heads.md#the-default) lists what to expect from this configuration.

## A kiosk with an operator's head

A kiosk is a display that runs one program. This file adds a second display, where an operator can log in at the machine. It uses the closed policy.

```ini
version = 1
policy = closed

[head kiosk]
consoles = 1
login = /usr/local/bin/kiosk

[head admin]
isolated = no
consoles = 4

[rules]
display edid:AAA:0x0001:KIOSK00001 -> kiosk
input path:pci-0000:15:00.0-usb-0:3.1 -> kiosk
display edid:BBB:0x0002:ADMIN00001 -> admin
input path:pci-0000:15:00.0-usb-0:3.2 -> admin
input path:pci-0000:15:00.0-usb-0:3.3 -> roaming
```

![vtium-config's Flow view of the kiosk file, under policy closed. The first row joins the monitor DP-1, KIOSK00001 by EDID, to the head kiosk, default font set, every VT, and the keyboards on USB 3.1 to kiosk, with no roaming inputs line. The second row joins the monitor HDMI-A-1, ADMIN00001 by EDID, to the head admin, and the keyboards on USB 3.2 to admin, with a dotted line from the words roaming inputs. The row ROAMING joins the inputs on USB 3.3 to the dotted box Active terminal.](../figures/concepts-flow-kiosk-admin.png)

The picture shows the file on a machine with both monitors and two keyboards connected. Port 3.3 is empty.

- The first row is the head `kiosk`. Its display is the monitor whose EDID serial is KIOSK00001, connected to DP-1. Its input is the keyboard on USB port 3.1.
- The second row is the head `admin`, with the monitor ADMIN00001 on HDMI-A-1 and the keyboard on port 3.2.
- The row ROAMING holds USB port 3.3, the spare port.

The head `kiosk` does not set `isolated`, so under the closed policy it is isolated. That is why the picture draws no roaming inputs line beside it. The head `admin` sets `isolated = no`, so it is the only head that can be the active head. The words roaming inputs appear beside `admin` alone. So a keyboard plugged into port 3.3 types into `admin`.

Under the closed policy, vtium ignores a keyboard on any other port, and its keys reach no session. So a keyboard plugged into a port other than 3.1, 3.2 or 3.3 types into nothing. [heads.md](../heads.md#a-kiosk-with-an-operators-head) lists what to expect from this file.

## Stations: one person at each head

Several people can work at once on one machine, each at their own display, keyboard and mouse. Each person's set is a station, and each station is a head. This file also uses the closed policy.

```ini
version = 1
policy = closed

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
input path:pci-0000:15:00.0-usb-0:3.1 -> station-1
input path:pci-0000:15:00.0-usb-0:3.2 -> station-2
input path:pci-0000:15:00.0-usb-0:3.3 -> station-3
```

![vtium-config's Flow view of the stations file, under policy closed. Three rows each join one monitor, DP-1, DP-2 and HDMI-A-1, with the serials MON0000001, MON0000002 and MON0000003 by EDID, to the heads station-1, station-2 and station-3, each with the default font set on every VT. Each head takes the inputs on its own USB port, 3.1, 3.2 or 3.3. There is no ROAMING row.](../figures/concepts-flow-stations.png)

The picture shows three paths, one for each station. Each station's port carries a USB hub, with the station's keyboard and mouse plugged into it. A `path:` rule that ends at a port also takes every device on a hub in that port, so the picture reads "inputs on USB 3.1".

The picture has no ROAMING row, because no rule sends an input to `roaming`. No head sets `isolated = no`, so under the closed policy every station is isolated. No station can be the active head, so vtium never moves a keyboard from one station to another. [heads.md](../heads.md#stations-one-person-to-each-head) lists what to expect, and what the stations still share.

## Two heads on two VTs

A head's `vt` key names the VTs it runs on. One vtium runs on each VT where vtium is enabled. Each vtium builds only the heads that run on its own VT. So two heads on different VTs can both show every display. This file puts the head `desk` on VT 3 and the head `panel` on VT 4:

```ini
font-size=12
version = 1
[look big]
font-name = proof
[head desk]
look = big
vt = 3
[head panel]
vt = 4
[rules]
display * -> desk
display * -> panel
input path:pci-0:3.3 -> panel
input class:keyboard -> roaming
```

![vtium-config's Flow view of two heads on two VTs, under policy open. The first row joins All displays to the head desk, font set proof, VT 3, and a dotted line from roaming inputs reaches desk. The second row joins All displays to the head panel, default font set, VT 4, and the keyboard kbd on pci-0:3.3 to panel, with a dotted line from roaming inputs. The row ROAMING joins every keyboard to the dotted box Active terminal.](../figures/concepts-flow-two-vts.png)

The picture shows the file with one keyboard connected, `kbd`, on port 3.3.

- The first row is the head `desk`. It renders with the look `big`, whose font set is proof. It runs on VT 3, where every display shows it.
- The second row is the head `panel`. It names no look, so it renders with the default font set. It runs on VT 4, where every display shows it. The keyboard on port 3.3 belongs to it.
- The row ROAMING holds every other keyboard. On VT 3 they type into `desk`, and on VT 4 they type into `panel`, since each is the only head on its VT.

On VT 3, the keyboard on port 3.3 types into nothing, because its head runs only on VT 4. Its `Ctrl-Alt-F4` still switches to VT 4, where it types into `panel`. With `switch-on-enter = yes` in `panel`'s section, Enter on that keyboard switches to VT 4 too. See [Terms](../head-file.md#terms) for details.

## When a keyboard reaches no screen

Under the open policy, two guards keep you from locking yourself out. The anti-lockout guard and the rescue key each move a keyboard to a head where you can use it. Each one shows a notice over the session that says where the keyboard went. See [Notices on the screen](../troubleshooting.md#notices-on-the-screen) for how long a notice stays. [Heads](heads.md) describes both guards.

### Every keyboard now types into NAME

![vtium's notice over a shell session, 800×480 pixels. A pale red box across the top of the screen holds the notice, wrapped onto two lines: "Every keyboard now types into desk, because no keyboard reached a screen that is on. To send a keyboard elsewhere, change its rule in /etc/vtium/vtium.conf, then restart vtium or reboot." Below the box, the shell session goes on.](../figures/concepts-notice-anti-lockout.png)

**Every keyboard now types into NAME, because no keyboard reached a screen that is on.**

**Why.** The anti-lockout guard shows this notice. A head is lit when at least one of its displays is on. The guard acts while keyboards are connected, but no keyboard reaches a lit head. For example, the rules send your only keyboard to a head whose monitor is unplugged, while another head's monitor is on. The guard then makes every keyboard roam, and it makes a lit head, NAME, the active head. So every keyboard types into NAME.

**What to do.** As soon as a keyboard reaches a lit head through its own rule, the guard stops. Each keyboard then goes back to where its rule sends it. For example, the guard stops when you plug the monitor back in.

To send a keyboard somewhere else for good, change its rule in vtium.conf. Then restart vtium or reboot.

**How to avoid it.** Before you install a file, run `vtium check-config` with your keyboards and displays connected. It notes when the rules ignore every keyboard, and when a head's only keyboard is one device. Keep a rule below your own rules that sends every keyboard somewhere you can use it, such as `input class:keyboard -> roaming`, as the built-in configuration does.

### Keyboard DEVICE now types into NAME, the active screen

![vtium's notice over a shell session, 800×480 pixels. A pale red box across the top of the screen holds the notice, wrapped onto two lines: "Keyboard /dev/input/event3 now types into desk, the active screen, until the next boot (the rescue key moved it). To keep it there, give it a rule in /etc/vtium/vtium.conf." Below the box, the shell session goes on.](../figures/concepts-notice-rescue.png)

**Keyboard DEVICE now types into NAME, the active screen, until the next boot (the rescue key moved it).**

**Why.** You pressed the rescue key, and the rescue key shows this notice. The rescue key is `Ctrl-Alt` and the function key of the VT that is already on screen, such as `Ctrl-Alt-F2` on VT 2. The rescue key makes the keyboard that pressed it roam, so that keyboard types into the active head, NAME. DEVICE is the keyboard's device file, such as `/dev/input/event3`.

**What to do.** Nothing, unless you want the keyboard somewhere else. To keep the keyboard there after the next boot, give the keyboard a rule in vtium.conf. See [Rules and the policy](#rules-and-the-policy) for details.

**How to avoid it.** Give each keyboard a rule that sends it where you want it. You then need the rescue key only when a rule goes wrong.

### Keyboard DEVICE now types into the active screen

![vtium's notice over a shell session, 800×480 pixels. A pale red box across the top of the screen holds the notice, wrapped onto two lines: "Keyboard /dev/input/event3 now types into the active screen until the next boot (the rescue key moved it), but no screen is active. Press Ctrl+Super+↑ to choose one." Below the box, the shell session goes on.](../figures/concepts-notice-rescue-no-head.png)

**Keyboard DEVICE now types into the active screen until the next boot (the rescue key moved it), but no screen is active.**

**Why.** The rescue key moved the keyboard, as above, but no head is the active head at that moment. A head can be the active head only when it is not isolated. So this happens when every head sets `isolated = yes`, or when no head that can be the active head is lit. The keyboard types into nothing until a head becomes the active head.

**What to do.** The notice names the roaming chord, `Ctrl-Super-Up` by default. Press the roaming chord on this keyboard to make a lit head the active head. While no head that can be the active head is lit, the first such head becomes the active head as soon as it lights.

**How to avoid it.** Leave at least one head without `isolated = yes`, and give that head a display that is on. There is then always an active head for the rescue key to send a keyboard to.

## Where the details are

- [Heads](heads.md) explains the policies, the guards, and what happens when a display goes away.
- [Heads, by scenario](../heads.md) gives complete files for common setups, with what to expect from each one.
- [The sections of vtium.conf](../head-file.md) is the reference for the heads, looks, rules and match keys.
