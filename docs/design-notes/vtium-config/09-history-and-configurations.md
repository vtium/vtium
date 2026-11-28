# History and several configurations

Where this chapter starts: nothing has changed since the save of 2026-11-28 11:30: `padnoir · 4 problems`. History lists four versions: 11:00, 10:20 and 10:05 on 2026-11-28, and 2026-11-12 08:53.

## History

File › History › lists the versions that saves replaced, newest first. The version saved now is not in the list, since Revert to saved goes back to it. Each entry says who wrote it: the person's saves through vtium-config, root's hand edit of 10:05, and the file that was installed on 2026-11-12.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
┌────────────────────────────────────────────┐──────────────────────────────────────────────────────
│ New…                                       │le names types into the active terminal
│ Open                                     › │                           INPUTS
├────────────────────────────────────────────┤───────────────┐            ┌────────────────────────┐
│ Undo · nothing to undo            Ctrl+Z   │               │◄───────────│ AT Translated Set 2 k… │
│ Redo · nothing to redo            Ctrl+Y   │ VT 1          │            │ keyboard · by name     │
├────────────────────────────────────────────┤───────────────┘            └────────────────────────┘
│ Save… · nothing to save           Ctrl+S   │───────────────┐            ┌────────────────────────┐
│ Show changes · none                        │               │◄───────────│ AT Translated Set 2 k… │
│ Edit as text…                              │ VT 2          │            │ keyboard · by name     │
├────────────────────────────────────────────┤───────────────┘            └────────────────────────┘
│ Revert to saved… · nothing to drop         │┌──────────────────────────────────────────┐
│ History                                  › ││ 2026-11-28 11:00   saved by user         │─────────
├────────────────────────────────────────────┤│ 2026-11-28 10:20   saved by user         │
│ Quit                                   q   ││ 2026-11-28 10:05   edited by root        │═════════╗
└────────────────────────────────────────────┘│ 2026-11-12 08:53   saved by root         │         ║
 ║ Choose… [Enter]  ║         ║ new: terminal-├──────────────────────────────────────────┤]        ║
 ╚══════════════════╝         ╚═══════════════│ Picking one shows it first.              │═════════╝
 ─────────────────────────────────────────────└──────────────────────────────────────────┘─────────
 PARTS
 DISPLAYS
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Display                 ║  │ eDP-1                   │  │ HDMI-A-1                │
 ║ Add display… [Enter]    ║  │ in console and work     │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘

 ↓ 3 more groups: KEYBOARDS, POINTERS, OTHER INPUTS
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Pick  [→] Submenu  [Esc] Close                                                 [F10] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. In the menu, bold and underlined, N of New…, O of Open, E of Edit as text…, H of History, Q of Quit. Highlighted: the File menu, the Flow tab, row 14, `History  ›`, row 15, `2026-11-28 10:20  saved by user`.*

The person wants to see how far padnoir has come since 10:20, before Evening and work existed. Picking that version opens a sheet that counts nothing and asks nothing. `[v]` shows what the version would change, and `[a]` uses it. Esc closes the sheet.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ eDP-1            ┃━━━━━━━━▶│ console                      │◄───────────│ AT Translated Set 2 k… │
 ┃ by its connector ┃         │ look My-Look · VT 1          │            │ keyboard · by name     │
 ┗━━━━━━━━━━━━━━━━━━┛         └──────────────────────────────┘            └────────────────────────┘
 ┌────┌ The version of 2026-11-28 10:20 ────────────────────────────────────────────────── × ┐─────┐
 │ eDP│ Saved by user on 2026-11-28 10:20.                                                   │2 k… │
 │ by │                                                                                      │     │
 └────│ [v] View changes       what it would change in the open document, read-only          │─────┘
 1 key│ [a] Use this version   loads it as one change; [Ctrl+Z] takes it back                │
 ─────└──────────────────────────────────────────────────────────────────────────────────────┘─────
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
 ║ Add display… [Enter]    ║  │ in console and work     │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘

 ↓ 3 more groups: KEYBOARDS, POINTERS, OTHER INPUTS
────────────────────────────────────────────────────────────────────────────────────────────────────
 [v] View changes  [a] Use this version  [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

View changes shows the version against the open document in Show changes' form, read-only. A red line would leave the document and a green one would come in, so the person sees that using the version drops Evening and work and brings terminus back. `w` toggles the whole file, and nothing here edits.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Version 2026-11-28 10:20  against the open document, read-only         ✓ Changes only  Whole file

 @@ [look Evening]
   18 -[look Evening]
   19 -palette = nord
   20 -font-size = 12
   21 -font-name = cullet-mono
   22 -vtlook = night

 @@ [head console]
     24  [head console]
     25  look = My-Look
   26 -vt = 1

 @@ [head work]
   28 -[head work]
   29 -look = Evening
   30 -vt = 2

                                                          … 1 more hunk: [PgDn], or [w] Whole file
 Font sets
     +terminus, which the open document removes




────────────────────────────────────────────────────────────────────────────────────────────────────
 [w] Whole file  [a] Use this version  [Esc] Back                              [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab. In color, the `-` rows are red and the `+` row green.*

`[a]` loads the version into the document as one change. No question comes first, because Ctrl+Z takes the change back and nothing is lost. The Flow page now shows the one path of 10:20, and the top row counts the change.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
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
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Keyboard                ║  │ AT Translated Set 2…    │  │ ThinkPad Extra Buttons  │
 ║ Add keyboard… [Enter]   ║  │ in the terminal console │  │ in no path              │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘
 Using the version of 2026-11-28 10:20: one change, not saved. [Ctrl+Z] takes it back.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Ctrl+Z] Undo  [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

The person leaves the version in padnoir for now, to decide later.

## A second and a third configuration

The person also keeps configurations that padnoir does not install. kiosk-test is for a kiosk machine at work, and travel is a copy of padnoir to try changes on. A configuration is a whole vtium.conf, so one built on padnoir can be copied to another machine and installed there.

File › New… asks for a name and where the new configuration starts: from what is saved, or from vtium's defaults. The person names it kiosk-test and starts from vtium's defaults. The configuration that was open keeps its change, so New never asks whether to throw it away.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ ┌ New configuration ────────────────────────────────────────────────────────────────────── × ┐… │
 ┃ │ Name     kiosk-test_                                                                       │  │
 ┗━│                                                                                            │──┘
 ┌─│ Start   ○ from what is saved, /etc/vtium/vtium.conf                                        │──┐
 │ │         ● from vtium's defaults: every setting at its default and one look, named default  │… │
 │ │                                                                                            │  │
 └─│ padnoir keeps its 1 change, not saved; File › Open brings it back.                         │──┘
 1 └────────────────────────────────────────────────────────────────────────────────────────────┘
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
 ║ Add display… [Enter]    ║  │ in console and work     │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘

 ↓ 3 more groups: KEYBOARDS, POINTERS, OTHER INPUTS
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Make  [↑↓] Field  [Esc] Cancel                                               [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 7, `kiosk-test_`.*

From vtium's defaults, kiosk-test holds one look named default and every setting at its default. Its Flow page draws vtium's built-in default: every display shows the console, and every input types into it. Its changes count against the installed file, as every configuration's do, so the top row counts two.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help            kiosk-test ● 2 not saved
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 vtium's built-in default: all displays show the console, and every input types into it.
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐
 ┃ All displays     ┃━━━━━━━━▶│ console                      │◄┄┄┄┄┄┄┄┄┄┄┄roaming inputs
 ┃ every display, … ┃         │ default font set · every VT  │
 ┗━━━━━━━━━━━━━━━━━━┛         └──────────────────────────────┘
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
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Keyboard                ║  │ AT Translated Set 2…    │  │ ThinkPad Extra Buttons  │
 ║ Add keyboard… [Enter]   ║  │ in the terminal console │  │ in no path              │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘

────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save  [.] More             [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

The person wires the kiosk as in [chapter 6](06-flow.md). It gets a closed policy, the kiosk's display with the keyboards on one USB port, and an admin station on a second display and port. padnoir has none of those devices, so the check finds one problem, no complete path among the devices connected here. On the kiosk machine, or with `vtium check-config --no-devices`, which checks the rules alone, kiosk-test has none.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help kiosk-test ● 6 not saved · 1 proble
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy closed: vtium ignores a keyboard no rule names
 DISPLAYS                     TERMINALS                                  INPUTS
 ┌──────────────────┐         ┌──────────────────────────────┐            ┌────────────────────────┐
 │ DP-1             │────────►│ kiosk                        │◄───────────│ keyboards on USB 3.1   │
 │ KIOSK00001 · by… │         │ default look · every VT      │            │ every input on that p… │
 └──────────────────┘         └──────────────────────────────┘            └────────────────────────┘
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ HDMI-A-1         ┃━━━━━━━━▶│ admin                        │◄───────────│ keyboards on USB 3.2   │
 ┃ ADMIN00001 · by… ┃         │ default look · every VT      │            │ every input on that p… │
 ┗━━━━━━━━━━━━━━━━━━┛         └──────────────────────────────┘            └────────────────────────┘
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
 ║ Display                 ║  │ DP-1                    │  │ HDMI-A-1                │
 ║ Add display… [Enter]    ║  │ in the terminal kiosk   │  │ in the terminal admin   │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘



────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save  [.] More             [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

Then the person makes travel from what is saved. A copy of the installed file holds no change, so its top row shows its name and its problems with no ●. One line says where kiosk-test's changes wait.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                 travel · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ eDP-1            ┃━━━━━━━━▶│ console                      │◄───────────│ AT Translated Set 2 k… │
 ┃ by its connector ┃         │ look My-Look · VT 1          │            │ keyboard · by name     │
 ┗━━━━━━━━━━━━━━━━━━┛         └──────────────────────────────┘            └────────────────────────┘
 ┌──────────────────┐         ┌──────────────────────────────┐            ┌────────────────────────┐
 │ eDP-1            │────────►│ work                         │◄───────────│ AT Translated Set 2 k… │
 │ by its connector │         │ look Evening · VT 2          │            │ keyboard · by name     │
 └──────────────────┘         └──────────────────────────────┘            └────────────────────────┘
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
 ║ Add display… [Enter]    ║  │ in console and work     │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘

 Opened travel. kiosk-test keeps its 6 changes; File › Open brings it back.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save  [.] More             [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

## Opening and managing configurations

File › Open › lists the configurations, the open one checked, each with its state. Opening another needs no question, because the one left keeps its changes. The person opens padnoir again, where the version of 10:20 waits.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                 travel · 4 problems
┌────────────────────────────────────────────┐──────────────────────────────────────────────────────
│ New…                                       │┌────────────────────────────────────┐l
│ Open                                     › ││   padnoir      ● 1 not saved       │
├────────────────────────────────────────────┤│ ✓ travel       saved               │───────────────┐
│ Undo · nothing to undo            Ctrl+Z   ││   kiosk-test   ● 6 not saved       │lated Set 2 k… │
│ Redo · nothing to redo            Ctrl+Y   │├────────────────────────────────────┤ · by name     │
├────────────────────────────────────────────┤│   New…                             │───────────────┘
│ Save… · nothing to save           Ctrl+S   ││   Manage configurations…           │───────────────┐
│ Show changes · none                        │└────────────────────────────────────┘lated Set 2 k… │
│ Edit as text…                              │ VT 2          │            │ keyboard · by name     │
├────────────────────────────────────────────┤───────────────┘            └────────────────────────┘
│ Revert to saved… · nothing to drop         │
│ History                                  › │─────────────────────────────────────────────────────
├────────────────────────────────────────────┤n [a] adds the path.
│ Quit                                   q   │═══════════════╗            ╔════════════════════════╗
└────────────────────────────────────────────┘               ║            ║ Input                  ║
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
 [Enter] Pick  [→] Submenu  [Esc] Close                                                 [F10] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. In the menu, bold and underlined, N of New…, O of Open, N of New…, M of Manage configurations…, E of Edit as text…, H of History, Q of Quit. Highlighted: the File menu, the Flow tab, row 4, `Open  ›`, row 5, `✓ travel  saved`.*

The submenu's last item, Manage configurations…, opens a sheet that renames and deletes them. Each configuration shows its state against the installed file, as `vtium config draft list` prints it. They live under `~/.config/vtium/drafts/`, and `/etc/vtium/vtium.conf` holds what is saved.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ eDP-1            ┃━━━━━━━━▶│ console                      │◄───────────│ AT Translated Set 2 k… │
 ┃ by it┌ Manage configurations ──────────────────────────────────────────────────────── × ┐me     │
 ┗━━━━━━│ padnoir       ● 1 not saved     open now                                         │───────┘
 ┌──────│ travel        saved                                                              │───────┐
 │ eDP-1│ kiosk-test    ● 6 not saved                                                      │t 2 k… │
 │ by it│                                                                                  │me     │
 └──────│ They live in ~/.config/vtium/drafts. /etc/vtium/vtium.conf holds what is saved.  │───────┘
 1 keybo└──────────────────────────────────────────────────────────────────────────────────┘
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
 ║ Add display… [Enter]    ║  │ in console and work     │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘

 ↓ 3 more groups: KEYBOARDS, POINTERS, OTHER INPUTS
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Open  [r] Rename…  [Del] Delete…  [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 9, `travel  saved`.*

padnoir is open, so Delete is greyed out on it, and one line says why. vtium-config always has a configuration open.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ eDP-1            ┃━━━━━━━━▶│ console                      │◄───────────│ AT Translated Set 2 k… │
 ┃ by it┌ Manage configurations ──────────────────────────────────────────────────────── × ┐me     │
 ┗━━━━━━│ padnoir       ● 1 not saved     open now                                         │───────┘
 ┌──────│ travel        saved                                                              │───────┐
 │ eDP-1│ kiosk-test    ● 6 not saved                                                      │t 2 k… │
 │ by it│                                                                                  │me     │
 └──────│ They live in ~/.config/vtium/drafts. /etc/vtium/vtium.conf holds what is saved.  │───────┘
 1 keybo│ padnoir is open, so it cannot be deleted; open another first.                    │
 ───────└──────────────────────────────────────────────────────────────────────────────────┘───────
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
 ║ Add display… [Enter]    ║  │ in console and work     │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘

 ↓ 3 more groups: KEYBOARDS, POINTERS, OTHER INPUTS
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Open  [r] Rename…  [Del] Delete…  [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 8, `padnoir  ● 1 not saved  open now`.*

Rename types the new name in place. The person starts on travel-2026, then keeps travel with Esc.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ eDP-1            ┃━━━━━━━━▶│ console                      │◄───────────│ AT Translated Set 2 k… │
 ┃ by it┌ Manage configurations ──────────────────────────────────────────────────────── × ┐me     │
 ┗━━━━━━│ padnoir       ● 1 not saved     open now                                         │───────┘
 ┌──────│ travel-2026_                                                                     │───────┐
 │ eDP-1│ kiosk-test    ● 6 not saved                                                      │t 2 k… │
 │ by it│                                                                                  │me     │
 └──────│ They live in ~/.config/vtium/drafts. /etc/vtium/vtium.conf holds what is saved.  │───────┘
 1 keybo└──────────────────────────────────────────────────────────────────────────────────┘
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
 ║ Add display… [Enter]    ║  │ in console and work     │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘

 ↓ 3 more groups: KEYBOARDS, POINTERS, OTHER INPUTS
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Keep  [Esc] Cancel                                                           [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 9, `travel-2026_`.*

The kiosk work is done for today, and the person deletes kiosk-test. Delete asks once and names what is lost: the configuration's text and its changes not saved. It never touches `/etc`.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ eDP-1            ┃━━━━━━━━▶│ console                      │◄───────────│ AT Translated Set 2 k… │
 ┃ by it┌ Manage configurations ──────────────────────────────────────────────────────── × ┐me     │
 ┗━━━━━━│ padnoir       ● 1 not saved     open now                                         │───────┘
 ┌──────│ travel        saved                                                              │───────┐
 │ eDP-1│ kiosk-test    ● 6 not saved                                                      │t 2 k… │
 │ by it│                                                                                  │me     │
 └──────│ They live in ~/.config/vtium/drafts. /etc/vtium/vtium.conf holds what is saved.  │───────┘
 1 keybo└──────────────────────────────────────────────────────────────────────────────────┘
 ───────────┌ Delete kiosk-test ──────────────────────────────────────────────────── × ┐───────────
 NEW PATH   │ Delete the configuration kiosk-test? Its text and its 6 changes not      │
 ╔══════════│ saved are lost, with ~/.config/vtium/drafts/kiosk-test.conf.             │═══════════╗
 ║ Display  │ /etc/vtium/vtium.conf stays as it is, and so do the other two.           │           ║
 ║ Choose… [└──────────────────────────────────────────────────────────────────────────┘er]        ║
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
 [y] Delete  [Esc] Cancel
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 10, `kiosk-test  ● 6 not saved`.*

## Deciding about the version

Back in padnoir, Ctrl+S shows what saving the version of 10:20 would do: drop Evening and work, and bring terminus back. Saving it would keep the file it replaces in History, so even that could be brought back.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Save to /etc/vtium/vtium.conf  the version of 2026-11-28 10:20, against what is saved

 @@ [look Evening]
   18 -[look Evening]
   19 -palette = nord
   20 -font-size = 12
   21 -font-name = cullet-mono
   22 -vtlook = night

 @@ [head console]
     24  [head console]
     25  look = My-Look
   26 -vt = 1

 @@ [head work]
   28 -[head work]
   29 -look = Evening
   30 -vt = 2

                                                          … 1 more hunk: [PgDn], or [w] Whole file
 Font sets
     +terminus, which the open document removes

 Each terminal uses the change at its next login. sudo asks for your password once.


────────────────────────────────────────────────────────────────────────────────────────────────────
 [Ctrl+S] Save  [w] Whole file  [Esc] Cancel                                   [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab. In color, the `-` rows are red and the `+` row green.*

The person keeps padnoir as it is saved. Esc closes the review, and Ctrl+Z takes the version back out. Each configuration keeps its own undo history while vtium-config runs, so switching to travel and kiosk-test and back left padnoir's history whole.

Where it leaves you: padnoir is the file of 2026-11-28 11:30 again, with two terminals: `padnoir · 4 problems`. The person keeps two configurations, padnoir and travel, and History lists four versions.
