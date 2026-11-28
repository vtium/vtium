# Saving, and what Save shows

Where this chapter starts: vtium-config has quit, and padnoir holds one change not saved, `font-size = 11` in My-Look: `padnoir ● 1 not saved · 4 problems`. The console draws 10 pt, and History is empty.

## Coming back to the change

The person runs `vtium config tui` again. vtium-config opens padnoir where the draft left it, so the top row still counts one change not saved. Nothing was lost when vtium-config quit, and nothing asked to be saved.

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
 ↓ 2 more groups: POINTERS, OTHER INPUTS
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save  [.] More             [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

Alt+F opens File. Undo has nothing to undo, although padnoir holds a change. vtium-config keeps the undo history only while it runs, and each start begins with an empty one. Between two runs, the command line can edit the same draft with `vtium config set` or `vtium config edit`, and an older history would then undo onto text it never saw. Revert to saved still drops every change since the last save. [ADR-1403](../../architecture/platform/ADR-1403-vtium-conf-holds-the-configuration-as-one-flow-from-the-displays-to-the-input-devices-and-vtium-config-draws-it.md) D11 sets both rules.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
┌────────────────────────────────────────────┐──────────────────────────────────────────────────────
│ New…                                       │le names types into the active terminal
│ Open                                     › │                           INPUTS
├────────────────────────────────────────────┤───────────────┐            ┌────────────────────────┐
│ Undo · nothing to undo            Ctrl+Z   │               │◄───────────│ AT Translated Set 2 k… │
│ Redo · nothing to redo            Ctrl+Y   │ every VT      │            │ keyboard · by name     │
├────────────────────────────────────────────┤───────────────┘            └────────────────────────┘
│ Save…                             Ctrl+S   │
│ Show changes                               │─────────────────────────────────────────────────────
│ Edit as text…                              │n [a] adds the path.
├────────────────────────────────────────────┤═══════════════╗            ╔════════════════════════╗
│ Revert to saved…                           │               ║            ║ Input                  ║
│ History                                  › │1              ║            ║ Choose… [Enter]        ║
├────────────────────────────────────────────┤═══════════════╝            ╚════════════════════════╝
│ Quit                                   q   │─────────────────────────────────────────────────────
└────────────────────────────────────────────┘
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
 [Enter] Pick  [→] Submenu  [Esc] Close                                                 [F10] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. In the menu, bold and underlined, N of New…, O of Open, S of Save…, c of Show changes, E of Edit as text…, R of Revert to saved…, H of History, Q of Quit. Highlighted: the File menu, the Flow tab, row 3, `New…`.*

## Show changes

File › Show changes shows the document's changes against `/etc/vtium/vtium.conf`, as git's diff shows them. Each hunk names the part of the file it is in and gives its line numbers with a little context. A removed line starts with `-` and is red, and an added line starts with `+` and is green. The font sets and active fonts the document changes follow the hunks, and padnoir changes none. A click on "1 not saved" in the top row opens the same screen.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Changes not saved  against /etc/vtium/vtium.conf                     ✓ Changes only    Whole file

 @@ [look My-Look]
     13  [look My-Look]
     14  palette = vga
   15 -font-size = 10
   15 +font-size = 11
     16  font-name = cullet-mono

 Fonts unchanged
















────────────────────────────────────────────────────────────────────────────────────────────────────
 [w] Whole file  [e] Edit as text  [Esc] Back  [Ctrl+S] Save                   [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab. In color, line 15's `-` row is red and its `+` row green.*

`w`, or a click on the pair at the top right, switches to the whole file, with each changed line still marked. The arrows, PgUp and PgDn scroll it.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 padnoir's vtium.conf  as it will be saved                              Changes only  ✓ Whole file

      1  # padnoir
      2  version = 1
      3  # Everything after this line is in the order vtium-config writes it.
      4
      5  [settings]
      6  hwaccel = on
      7  font-name = cullet-mono
      8  osd = full
      9  palette = vga
     10  allow-font-ops = on
     11
     12  # My-Look: the laptop's own panel.
     13  [look My-Look]
     14  palette = vga
   15 +font-size = 11
     16  font-name = cullet-mono
     17
     18  [head console]
     19  look = My-Look
     20
     21  [rules]
     22  display connector:eDP-1 -> console
     23  keyboard name:"AT Translated Set 2 keyboard" -> console

────────────────────────────────────────────────────────────────────────────────────────────────────
 [w] Changes only  [e] Edit as text  [Esc] Back  [Ctrl+S] Save                 [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab. In color, line 15 is green.*

## Save's review

Ctrl+S opens Save's review, and so do File › Save… and a click on `[Ctrl+S] Save`. The review shows exactly what saving changes: the lines of vtium.conf with their context, the fonts the save activates or deactivates, and the problems the check reports. The count at the top right is the count in the top row. The four problems are warnings, so saving still works with them, and `[Alt+P]` lists them. The review also says when the change takes effect: each terminal uses it at its next login.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Save to /etc/vtium/vtium.conf                                           1 change · fonts unchanged

 @@ [look My-Look]
     13  [look My-Look]
     14  palette = vga
   15 -font-size = 10
   15 +font-size = 11
     16  font-name = cullet-mono

 4 warnings (saving still works): 1 font My-Look uses is not active, Noto Color Emoji fails
 the trust rule, the layer nerd-symbols names a font that is not installed, and 1 keyboard and
 2 pointers are in no path. [Alt+P] lists them.

 Each terminal uses the change at its next login. sudo asks for your password once.












────────────────────────────────────────────────────────────────────────────────────────────────────
 [Ctrl+S] Save  [w] Whole file  [Alt+P] Problems  [Esc] Cancel                 [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab. In color, the warnings' heading is amber.*

Esc goes back and changes nothing. Ctrl+S again, Enter or a click on Save saves through the same steps as `vtium config install`: the check, the install bundle built from the draft, and `sudo vtium-config apply`. vtium-config steps aside while sudo asks for the password. It names the command first, and this is the one password prompt of a save. The root side checks the file again before it writes it.

```text
user@padnoir:~$ vtium config tui

vtium-config is saving padnoir to /etc/vtium/vtium.conf. It runs:
  sudo vtium-config apply /home/user/.config/vtium/drafts/padnoir

[sudo] password for user: _
























```

*Marks: none. vtium-config has stepped aside to the shell while sudo asks.*

After the write, a sheet says what the save did and that the file it replaced is in History. vtium reads vtium.conf only when it starts on a VT, so the sheet offers to restart vtium on VT 1. A session keeps running through the restart, but its screen is cleared. The top row has lost its ●.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
 ┏━━━━━━━━━━━━━━━━━━┓         ┌──────────────────────────────┐            ┌────────────────────────┐
 ┃ eDP-1            ┃━━━━━━━━▶│ console                      │◄───────────│ AT Translated Set 2 k… │
 ┌ Saved ────────────────────────────────────────────────────────────────────────────────────── × ┐│
 │ Saved /etc/vtium/vtium.conf: 1 change. The file it replaced is in History.                     │┘
 │                                                                                                │
 │ vtium reads vtium.conf only when it starts on a VT.                                            │
 │ vtium runs on VT 1, where the terminal console is.                                             │
 │                                                                                                │╗
 │ Restart vtium on VT 1, where it runs, so the file applies now?                                 │║
 │ Each session keeps running through a restart, but its screen is cleared.                       │║
 └────────────────────────────────────────────────────────────────────────────────────────────────┘╝
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
 [y] Restart now  [n] Later
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

The person answers `y`. vtium starts again on VT 1, and the console draws Cullet Mono at 11 pt, with 10×22 cells and a grid of 192×49 characters.

## Nothing to save

With nothing changed since the save, File's items that act on changes are greyed out, each with its reason. Ctrl+S says "Nothing to save" in one line.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
┌────────────────────────────────────────────┐──────────────────────────────────────────────────────
│ New…                                       │ 9 fonts · 4 active │ DejaVu Sans Mono Regular
│ Open                                     › │issing (1)          │
├────────────────────────────────────────────┤                    │ file    DejaVuSansMono.ttf
│ Undo · nothing to undo            Ctrl+Z   │                    │ folder  /usr/share/fonts/TTF
│ Redo · nothing to redo            Ctrl+Y   │                    │ kind    outline, monospaced
├────────────────────────────────────────────┤tive                │ covers  Latin, Greek, Cyrillic,
│ Save… · nothing to save           Ctrl+S   │tive                │         box drawing
│ Show changes · none                        │tive · fails trust  │ sizes   any
│ Edit as text…                              │tive · My-Look      │ state   ● active · trusted
├────────────────────────────────────────────┤                    │ used by no look
│ Revert to saved… · nothing to drop         │                   │
│ History                                  › │ active             │ 11 pt: 10×22 px cells,
├────────────────────────────────────────────┤                    │ 192×49 characters
│ Quit                                   q   │le yet              │
└────────────────────────────────────────────┘                    │
   Symbols Nerd Font Mono             ✗ missing · My-Look         │
 ┌ picture of the console ──────────────────────────────────────┐ │
 │ user@padnoir:~/notes$ ls -l                                  │ │
 │ drwxr-xr-x 2 user user 4096 Nov 27 drafts                    │ │
 │ -rw-r--r-- 1 user user 1823 Nov 27 plan.txt                  │ │
 │ user@padnoir:~/notes$ colors                                 │ │
 │ red green yellow blue magenta cyan white                     │ │
 │ Bold text and italic text beside plain.                      │ │
 │ ┌──────────┬─────────┐                                       │ │
 └──────────────────────────────────────────────────────────────┘ │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Pick  [→] Submenu  [Esc] Close                                                 [F10] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. In the menu, bold and underlined, N of New…, O of Open, E of Edit as text…, H of History, Q of Quit. Highlighted: the File menu, the Fonts tab, row 3, `New…`.*

Where it leaves you: the save of 2026-11-28 at 09:40 wrote `font-size = 11` into `/etc/vtium/vtium.conf`, and the console draws 11 pt: `padnoir · 4 problems`. History holds one version, the file of 2026-11-12 08:53 that the save replaced.
