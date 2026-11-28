# Wiring displays and inputs in Flow

Where this chapter starts: the save of 2026-11-28 at 11:00 holds the look Evening, with nord and the vtlook night, and no longer holds the font set terminus: `padnoir · 4 problems`. No terminal uses Evening yet. History lists three versions.

vtium's configuration is one flow from the displays to the input devices. A path joins displays and inputs to a terminal, the group of consoles that vtium draws on those displays and that those inputs type into. The Flow tab draws the paths on one page in three sections. CONFIGURATION holds the paths of the file, NEW PATH builds one, and PARTS lists the machine's displays and inputs. [ADR-1403](../../architecture/platform/ADR-1403-vtium-conf-holds-the-configuration-as-one-flow-from-the-displays-to-the-input-devices-and-vtium-config-draws-it.md) D7 decides the page. The person wants a second terminal, work, on VT 2, which draws with Evening on the same display and keyboard.

## Reading the page

Enter on a part of a path in CONFIGURATION says what the part matches and where it goes. What a path joins changes only when the person revises the path with `e`.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › eDP-1

 DISPLAY  eDP-1   by its connector
   Matched by      connector:eDP-1
   Where it goes   in the terminal console, every VT

 What a path joins changes only when you revise it: [e].



















────────────────────────────────────────────────────────────────────────────────────────────────────
 [e] Revise  [Esc] Back                                                        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

`d`, or Actions › Read devices again, reads the connected displays and inputs again. The person has just plugged in a USB receiver for a mouse, and HDMI-A-1 has no monitor now. The new part is highlighted, and one line names what came and what went. Reading the devices changes nothing in the document. The person unplugs the receiver again.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ eDP-1            ┃━━━━━━━━▶│ console                      │◄───────────│ AT Translated Set 2 k… │
 ┃ by its connector ┃         │ look My-Look · every VT      │            │ keyboard · by name     │
 ┗━━━━━━━━━━━━━━━━━━┛         └──────────────────────────────┘            └────────────────────────┘
 1 keyboard and 2 pointers are in no path yet.
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 NEW PATH     Pick an input and a display, then [a] adds the path.
 ╔══════════════════╗         ╔══════════════════════════════╗            ╔════════════════════════╗
 ║ Display          ║         ║ Terminal                     ║            ║ Input                  ║
 ║ Choose… [Enter]  ║         ║ new: terminal-1              ║            ║ Choose… [Enter]        ║
 ╚══════════════════╝         ╚══════════════════════════════╝            ╚════════════════════════╝
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 PARTS
 DISPLAYS
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Display                 ║  │ eDP-1                   │  │ HDMI-A-1                │
 ║ Add display… [Enter]    ║  │ in the terminal console │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘
 KEYBOARDS
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┏━━━━━━━━━━━━━━━━━━━━━━━━━┓
 ║ Keyboard                ║  │ AT Translated Set 2…    │  ┃ Logitech USB Receiver   ┃
 ║ Add keyboard… [Enter]   ║  │ in the terminal console │  ┃ new · in no path        ┃
 ╚═════════════════════════╝  └─────────────────────────┘  ┗━━━━━━━━━━━━━━━━━━━━━━━━━┛
 Read devices again: connected Logitech USB Receiver; disconnected HDMI-A-1.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save  [.] More             [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

## Labels

Enter on a part in PARTS opens its detail, its label first, with the choices that narrow what it matches below. A label is a name of the person's own, which vtium's on-screen display shows.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › eDP-1

 DISPLAY PART  eDP-1
   Label     none, [Enter] adds one        shown on the console's on-screen display

 CHOOSE WHICH DISPLAYS THIS PART MATCHES  from the widest to the narrowest; [Enter] picks one
   ○ All displays        every display, connected now or later
   ○ eDP-1               by its connector
   ○ This monitor        BOE 0x0BCA, by its EDID, on any port

















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Label  [↓] Choices  [Esc] Back                                        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 6, `Label  none, [Enter] adds one  shown on the console's on-screen display`.*

The person starts to type Laptop panel for eDP-1, then changes their mind. Esc leaves the label as it was.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › eDP-1

 DISPLAY PART  eDP-1
   Label     Laptop panel_      [Enter] keeps it; clear it to remove the label

 CHOOSE WHICH DISPLAYS THIS PART MATCHES  from the widest to the narrowest; [Enter] picks one
   ○ All displays        every display, connected now or later
   ○ eDP-1               by its connector
   ○ This monitor        BOE 0x0BCA, by its EDID, on any port

















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Keep  [Esc] Cancel                                                           [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 6, `Label  Laptop panel_  [Enter] keeps it; clear it to remove the label`.*

On the keyboard's part they keep a label, Laptop keys. The label is a change like any other, and the top row counts it.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › AT Translated Set 2 keyboard

 INPUT PART  AT Translated Set 2 keyboard
   Label     Laptop keys             shown on the console's on-screen display

 CHOOSE WHICH INPUTS THIS PART MATCHES  from the widest to the narrowest; [Enter] picks one
   ○ All keyboards                   every keyboard, connected now or later
   ○ AT Translated Set 2 keyboard    by name

















 The label is now “Laptop keys”.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Label  [↓] Choices  [Esc] Back                                        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 6, `Label  Laptop keys  shown on the console's on-screen display`.*

## The terminal's card

Enter on a terminal of a path opens its card: its name, look, VTs, consoles and login, what happens when its screen is unplugged, and Switch on Enter. Under them, the card shows what the terminal draws. The Look line marks My-Look's font that is not active.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › console

 TERMINAL console · every VT
   Name                           console
   Look                           My-Look   1 font not active
   VTs                            every VT
   Consoles                       1
   Login                          Default: /bin/login -p
   When its screen is unplugged   keep the sessions running
   Switch on Enter                off
   What the path joins changes only when you revise it: [e].

 WHAT THIS TERMINAL DRAWS
   Font set cullet-mono  →  look My-Look  →  eDP-1
   Effects: No filter, the built-in vtlook, which changes no color.











────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Choose  [Enter] Set  [e] Revise  [Esc] Back  [Ctrl+S] Save  [.] More     [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 6, `Name  console`.*

Enter on Name types a new name. The person keeps console with Esc.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › console

 TERMINAL console_ · every VT
   Name                           console_
   Look                           My-Look   1 font not active
   VTs                            every VT
   Consoles                       1
   Login                          Default: /bin/login -p
   When its screen is unplugged   keep the sessions running
   Switch on Enter                off
   What the path joins changes only when you revise it: [e].

 WHAT THIS TERMINAL DRAWS
   Font set cullet-mono  →  look My-Look  →  eDP-1
   Effects: No filter, the built-in vtlook, which changes no color.











────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Keep  [Esc] Cancel                                                           [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 6, `Name  console_`.*

◂ and ▸ step a line through its choices, and the meaning of the highlighted line shows once below the card. Login starts the program each console of this terminal runs.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › console

 TERMINAL console · every VT
   Name                           console
   Look                           My-Look   1 font not active
   VTs                            every VT
   Consoles                       1
   Login                          ● Default: /bin/login -p   ○ Other…
   When its screen is unplugged   keep the sessions running
   Switch on Enter                off
   Login: the program each console of this terminal starts.
   What the path joins changes only when you revise it: [e].

 WHAT THIS TERMINAL DRAWS
   Font set cullet-mono  →  look My-Look  →  eDP-1
   Effects: No filter, the built-in vtlook, which changes no color.










────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Choose  [Enter] Set  [e] Revise  [Esc] Back  [Ctrl+S] Save  [.] More     [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 10, `Login  ● Default: /bin/login -p  ○ Other…`.*

Switch on Enter has no effect while the terminal runs on every VT, and the two lines below the card say so.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › console

 TERMINAL console · every VT
   Name                           console
   Look                           My-Look   1 font not active
   VTs                            every VT
   Consoles                       1
   Login                          Default: /bin/login -p
   When its screen is unplugged   keep the sessions running
   Switch on Enter                ● off   ○ on
   Switch on Enter: [Enter] on its own keyboard switches to its VT. No effect while it runs on
   every VT: press [e] and choose a VT.
   What the path joins changes only when you revise it: [e].

 WHAT THIS TERMINAL DRAWS
   Font set cullet-mono  →  look My-Look  →  eDP-1
   Effects: No filter, the built-in vtlook, which changes no color.









────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Choose  [Enter] Set  [e] Revise  [Esc] Back  [Ctrl+S] Save  [.] More     [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 12, `Switch on Enter  ● off  ○ on`.*

The console runs on every VT, so it must give VT 2 up to work. Enter on VTs opens the list box, and the person keeps VT 1, a second change.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › console

 TERMINAL console · every VT
   Name                           console
   Look                           My-Look   1 font not active
   VTs                            every VT
   Consoles                       ┌──────────────────────┐
   Login                          │ ● every VT           │
   When its screen is unplugged   │ ○ VT 1               │g
   Switch on Enter                │ ○ VT 2               │
   What the path joins changes onl│ ○ VT 3               │].
                                  │ ○ VT 4               │
 WHAT THIS TERMINAL DRAWS         └──────────────────────┘
   Font set cullet-mono  →  look My-Look  →  eDP-1
   Effects: No filter, the built-in vtlook, which changes no color.











────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Pick  [Enter] Keep  [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 8, `VTs  every VT`, row 11, `○ VT 1`.*

## A new path

NEW PATH builds a path in three places: a display, a terminal and an input. Enter on eDP-1's part in PARTS, then on its choice by connector, picks the display into NEW PATH.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 2 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › eDP-1

 DISPLAY PART  eDP-1
   Label     none, [Enter] adds one        shown on the console's on-screen display

 CHOOSE WHICH DISPLAYS THIS PART MATCHES  from the widest to the narrowest; [Enter] picks one
   ○ All displays        every display, connected now or later
   ● eDP-1               by its connector
   ○ This monitor        BOE 0x0BCA, by its EDID, on any port

















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Pick  [Esc] Back                                                      [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 10, `● eDP-1  by its connector`.*

Enter on NEW PATH's terminal opens the new terminal's card. The person names it work, gives it VT 2 and the look Evening. The card says what the terminal will draw, and how a part joins the path: Space on a part in PARTS joins it.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 2 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › new path

 THE NEW PATH'S TERMINAL · VT 2
   Name                           work
   Look                           Evening
   VTs                            VT 2
   Consoles                       1
   Login                          Default: /bin/login -p
   When its screen is unplugged   keep the sessions running
   Switch on Enter                off
   Display eDP-1                  connected
   [Space] on a part in PARTS joins it to this path; [Enter] on a part here takes it out.

 WHAT THIS TERMINAL DRAWS
   cullet-mono  →  Evening  →  eDP-1
   Effects: the vtlook night, which reads the color map panel.cube.










────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Choose  [Enter] Set  [e] Revise  [Esc] Back  [Ctrl+S] Save  [.] More     [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 6, `Name  work`.*

Space on the keyboard's part joins it, and `a` adds the path, the third change. CONFIGURATION now holds two paths on the same display and keyboard, console on VT 1 and work on VT 2. Ctrl+Alt+F1 and Ctrl+Alt+F2 switch between them.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 3 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┌──────────────────┐         ┌──────────────────────────────┐            ┌────────────────────────┐
 │ eDP-1            │────────►│ console                      │◄───────────│ AT Translated Set 2 k… │
 │ by its connector │         │ look My-Look · VT 1          │            │ keyboard · by name     │
 └──────────────────┘         └──────────────────────────────┘            └────────────────────────┘
 ┌──────────────────┐         ┌──────────────────────────────┐            ┌────────────────────────┐
 │ eDP-1            │────────►│ work                         │◄───────────│ AT Translated Set 2 k… │
 │ by its connector │         │ look Evening · VT 2          │            │ keyboard · by name     │
 └──────────────────┘         └──────────────────────────────┘            └────────────────────────┘
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 NEW PATH     Pick an input and a display, then [a] adds the path.
 ╔══════════════════╗         ╔══════════════════════════════╗            ╔════════════════════════╗
 ║ Display          ║         ║ Terminal                     ║            ║ Input                  ║
 ║ Choose… [Enter]  ║         ║ new: terminal-1              ║            ║ Choose… [Enter]        ║
 ╚══════════════════╝         ╚══════════════════════════════╝            ╚════════════════════════╝
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 PARTS
 DISPLAYS
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Display                 ║  │ eDP-1                   │  │ HDMI-A-1                │
 ║ Add display… [Enter]    ║  │ in the terminal console │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘



────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save  [.] More             [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

### Other ways to choose a part

A template in PARTS, or an empty place in NEW PATH, opens a maker, which offers the widest choices first and then the ways to choose by.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 3 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › Display

 ADD A DISPLAY   Which displays should this include?
   ○ All displays     every display, connected now or later
   OR CHOOSE BY
   ○ This monitor     one monitor by its EDID, on any port
   ○ Connector        the display on one connector, such as HDMI-A-1
   ○ Hub port         the display on one port of a DisplayPort hub
   ○ Graphics card    every display on one graphics card

















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Choose  [Esc] Back                                                    [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 8, `○ This monitor  one monitor by its EDID, on any port`.*

Each way lists the values that match a device now, then Other….

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 3 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › Display

 THIS MONITOR   each monitor lists what matches it now
   ○ edid:BOE:0x0bca:00000000     eDP-1
   ○ Other…                       type a value





















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Pick  [Esc] Back                                                      [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 6, `○ edid:BOE:0x0bca:00000000  eDP-1`.*

After Other… the person types the value. Enter adds it, and Esc goes back to the picker.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 3 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › Display

 THIS MONITOR   each monitor lists what matches it now
   ○ edid:BOE:0x0bca:00000000     eDP-1
   ○ Other…                       edid:BOE:_





















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Add  [Esc] Cancel                                                            [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 7, `○ Other…  edid:BOE:_`.*

The maker for an input offers no input, all keyboards, all pointers or all input devices, then the ways to choose by.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 3 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › Input

 ADD AN INPUT   Which inputs should this include?
   ○ No input                  display only, with no keyboard or mouse
   ○ All keyboards             every keyboard, connected now or later
   ○ All mice and touchpads    mice, touchpads and trackballs
   ○ All input devices         keyboards, pointers and every other kind
   OR CHOOSE BY
   ○ USB port                  devices plugged into one USB port
   ○ This one device           one device by its serial number, in any port
   ○ Model                     every device of one make and model
   ○ Bluetooth device          one paired Bluetooth device
   ○ Device name               devices that report a given name
   ○ Device type               touchscreens, tablets, joysticks and other kinds
   ○ udev property             devices with a given udev property (advanced)











────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Choose  [Esc] Back                                                    [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 11, `○ USB port  devices plugged into one USB port`.*

The USB port picker lists what is plugged into each port. Had the receiver stayed plugged in, its port would be the one to pick.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 3 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › Input

 USB PORT   each port lists what is plugged into it now
   ○ path:pci-0000:00:14.0-usb-0:1   Logitech USB Receiver
   ○ Other…                          type a value





















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Pick  [Esc] Back                                                      [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 6, `○ path:pci-0000:00:14.0-usb-0:1  Logitech USB Receiver`.*

## Revising a path

`e` on a path takes it out of CONFIGURATION into NEW PATH, to change it, and `a` puts it back. When `a` adds it back unchanged, the configuration is as it was.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 3 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
                              ┌──────────────────────────────┐
                              │ work                         │
                              │ look Evening · VT 2          │
                              └──────────────────────────────┘
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 NEW PATH     Pick an input and a display, then [a] adds the path.
 ╔══════════════════╗         ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓            ╔════════════════════════╗
 ║ eDP-1            ║         ┃ console                      ┃            ║ AT Translated Set 2 k… ║
 ║ by its connector ║         ┃ look My-Look · VT 1      ┃            ║ keyboard · by name     ║
 ╚══════════════════╝         ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛            ╚════════════════════════╝
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 PARTS
 DISPLAYS
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Display                 ║  │ eDP-1                   │  │ HDMI-A-1                │
 ║ Add display… [Enter]    ║  │ in the terminal console │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘
 KEYBOARDS
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Keyboard                ║  │ AT Translated Set 2…    │  │ ThinkPad Extra Buttons  │
 ║ Add keyboard… [Enter]   ║  │ in the terminal console │  │ in no path              │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘

 Revising console's path: change a part, then [a] adds it back.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save  [.] More             [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

A display can be in only one path for each VT. Picking eDP-1 for a third terminal on VT 1 says why it cannot join, in one line.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 3 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┌──────────────────┐         ┌──────────────────────────────┐            ┌────────────────────────┐
 │ eDP-1            │────────►│ console                      │◄───────────│ AT Translated Set 2 k… │
 │ by its connector │         │ look My-Look · VT 1          │            │ keyboard · by name     │
 └──────────────────┘         └──────────────────────────────┘            └────────────────────────┘
 ┌──────────────────┐         ┌──────────────────────────────┐            ┌────────────────────────┐
 │ eDP-1            │────────►│ work                         │◄───────────│ AT Translated Set 2 k… │
 │ by its connector │         │ look Evening · VT 2          │            │ keyboard · by name     │
 └──────────────────┘         └──────────────────────────────┘            └────────────────────────┘
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 NEW PATH     Pick an input and a display, then [a] adds the path.
 ╔══════════════════╗         ╔══════════════════════════════╗            ╔════════════════════════╗
 ║ Display          ║         ║ terminal-1                   ║            ║ Input                  ║
 ║ Choose… [Enter]  ║         ║ default look · VT 1          ║            ║ Choose… [Enter]        ║
 ╚══════════════════╝         ╚══════════════════════════════╝            ╚════════════════════════╝
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 PARTS
 DISPLAYS
 ╔═════════════════════════╗  ┏━━━━━━━━━━━━━━━━━━━━━━━━━┓  ┌─────────────────────────┐
 ║ Display                 ║  ┃ eDP-1                   ┃  │ HDMI-A-1                │
 ║ Add display… [Enter]    ║  ┃ in the terminal console ┃  │ not connected           │
 ╚═════════════════════════╝  ┗━━━━━━━━━━━━━━━━━━━━━━━━━┛  └─────────────────────────┘


 eDP-1 is already in the terminal console on VT 1. A display is in one path for each VT.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save  [.] More             [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

## The other inputs

The last group of PARTS folds the inputs that are neither keyboards nor pointers, each with the rule that catches it. None of them types into a terminal.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 3 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Flow › other inputs

 OTHER INPUTS   neither keyboards nor pointers, so none types into a terminal
   Power Button              no rule names it
   Lid Switch                no rule names it
   Sleep Button              no rule names it
   Video Bus                 no rule names it
   ThinkPad Extra Buttons    no rule names it


















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Esc] Back                                                                    [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

## Saving the two terminals

Ctrl+S saves the three changes through the review and sudo. vtium has not run on VT 2 before. So the sheet after the save says that it enabled vtium there, and offers to start it on VT 2 and restart it on VT 1. The person answers `y`.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ eDP-1            ┃━━━━━━━━▶│ console                      │◄───────────│ AT Translated Set 2 k… │
 ┌ Saved ────────────────────────────────────────────────────────────────────────────────────── × ┐│
 │ Saved /etc/vtium/vtium.conf: 3 changes. The file it replaced is in History.                    │┘
 │                                                                                                │┐
 │ vtium reads vtium.conf only when it starts on a VT.                                            ││
 │   vtium console enable 2     enabled vtium on VT 2, where the head work runs                   ││
 │                                                                                                │┘
 │ Start vtium on VT 2, and restart it on VT 1 where it runs, so the file applies now?            │
 │ Each session keeps running through a restart, but its screen is cleared.                       │
 └────────────────────────────────────────────────────────────────────────────────────────────────┘
 ╔══════════════════╗         ╔══════════════════════════════╗            ╔════════════════════════╗
 ║ Display          ║         ║ Terminal                     ║            ║ Input                  ║
 ║ Choose… [Enter]  ║         ║ new: terminal-1              ║            ║ Choose… [Enter]        ║
 ╚══════════════════╝         ╚══════════════════════════════╝            ╚════════════════════════╝
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 PARTS
 DISPLAYS
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Display                 ║  │ eDP-1                   │  │ HDMI-A-1                │
 ║ Add display… [Enter]    ║  │ in console and work     │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘

 ↓ 3 more groups: KEYBOARDS, POINTERS, OTHER INPUTS
────────────────────────────────────────────────────────────────────────────────────────────────────
 [y] Restart now  [n] Later
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

### When a save would leave nobody typing

A configuration needs at least one complete path, a display that a connected keyboard reaches. Had the person taken out every path, Save would ask this one extra question before it saves:

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ eDP-1            ┃━━━━━━━━▶│ console                      │◄───────────│ AT Translated Set 2 k… │
 ┃ by its connector ┃         │ look My-Look · every VT      │            │ keyboard · by name     │
 ┗━━━━┌ Save ───────────────────────────────────────────────────────────────────────────── × ┐─────┘
 1 key│ This configuration has no complete path: no connected display goes to a terminal     │
 ─────│ that a connected keyboard reaches. After saving, nobody at this machine may be able  │─────
 NEW P│ to type into vtium.                                                                  │
 ╔════│                                                                                      │═════╗
 ║ Dis│ Save anyway?                                                                         │     ║
 ║ Cho└──────────────────────────────────────────────────────────────────────────────────────┘     ║
 ╚══════════════════╝         ╚══════════════════════════════╝            ╚════════════════════════╝
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 PARTS
 DISPLAYS
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Display                 ║  │ eDP-1                   │  │ HDMI-A-1                │
 ║ Add display… [Enter]    ║  │ in the terminal console │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘
 KEYBOARDS
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Keyboard                ║  │ AT Translated Set 2…    │  │ ThinkPad Extra Buttons  │
 ║ Add keyboard… [Enter]   ║  │ in the terminal console │  │ in no path              │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘
 ↓ 2 more groups: POINTERS, OTHER INPUTS
────────────────────────────────────────────────────────────────────────────────────────────────────
 [y] Save anyway  [Esc] Cancel
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

### Two pages from other machines

The same page draws other kinds of machines. With roaming inputs, every mouse and touchpad types into whichever terminal is on the screen:

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┌──────────────────┐         ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓            ┌────────────────────────┐
 │ eDP-1            │────────►┃ console                      ┃◄───────────│ AT Translated Set 2 k… │
 │ by its connector │         ┃ look My-Look · every VT      ┃            │ keyboard · by name     │
 └──────────────────┘         ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛            └────────────────────────┘
                              ┌┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┐            ┌────────────────────────┐
 ROAMING  inputs that type    ┆ Active terminal              ┆◄┄┄┄┄┄┄┄┄┄┄┄│ All mice and touchpads │
 into the active terminal     ┆ the terminal on screen       ┆            │ mice, touchpads, trac… │
                              └┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┘            └────────────────────────┘
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 NEW PATH     Pick an input and a display, then [a] adds the path.
 ╔══════════════════╗         ╔══════════════════════════════╗            ╔════════════════════════╗
 ║ Display          ║         ║ Terminal                     ║            ║ Input                  ║
 ║ Choose… [Enter]  ║         ║ new: terminal-1              ║            ║ Choose… [Enter]        ║
 ╚══════════════════╝         ╚══════════════════════════════╝            ╚════════════════════════╝
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 PARTS
 DISPLAYS
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Display                 ║  │ eDP-1                   │  │ HDMI-A-1                │
 ║ Add display… [Enter]    ║  │ in the terminal console │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘



────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save  [.] More             [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

A lab machine can give each of three displays its own terminal, with the inputs of one USB port each. The screen below comes from such a machine's vtium-config, not from padnoir's.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                                 lab
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy closed: vtium ignores a keyboard no rule names
 DISPLAYS                     TERMINALS                                  INPUTS
 ┌──────────────────┐         ┌──────────────────────────────┐            ┌────────────────────────┐
 │ DP-1             │────────►│ station-1                    │◄───────────│ inputs on USB 3.1      │
 │ MON0000001 · by… │         │ default look · every VT      │            │ every input on that p… │
 └──────────────────┘         └──────────────────────────────┘            └────────────────────────┘
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ DP-2             ┃━━━━━━━━▶│ station-2                    │◄───────────│ inputs on USB 3.2      │
 ┃ MON0000002 · by… ┃         │ default look · every VT      │            │ every input on that p… │
 ┗━━━━━━━━━━━━━━━━━━┛         └──────────────────────────────┘            └────────────────────────┘
 ┌──────────────────┐         ┌──────────────────────────────┐            ┌────────────────────────┐
 │ HDMI-A-1         │────────►│ station-3                    │◄───────────│ inputs on USB 3.3      │
 │ MON0000003 · by… │         │ default look · every VT      │            │ every input on that p… │
 └──────────────────┘         └──────────────────────────────┘            └────────────────────────┘
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 NEW PATH     Pick an input and a display, then [a] adds the path.
 ╔══════════════════╗         ╔══════════════════════════════╗            ╔════════════════════════╗
 ║ Display          ║         ║ Terminal                     ║            ║ Input                  ║
 ║ Choose… [Enter]  ║         ║ new: terminal-1              ║            ║ Choose… [Enter]        ║
 ╚══════════════════╝         ╚══════════════════════════════╝            ╚════════════════════════╝
 ──────────────────────────────────────────────────────────────────────────────────────────────────
 PARTS




────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save  [.] More             [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

Where it leaves you: the save of 2026-11-28 at 11:30 holds two terminals on eDP-1 and the keyboard's label Laptop keys: `padnoir · 4 problems`. console runs on VT 1 with My-Look, and work on VT 2 with Evening. History lists four versions, of 11:00, 10:20 and 10:05 on 2026-11-28 and of 2026-11-12 08:53.
