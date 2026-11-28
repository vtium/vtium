# Opening vtium-config on a first run

Where this chapter starts: vtium runs on padnoir, a laptop with one built-in display, eDP-1, and its own keyboard. `/etc/vtium/vtium.conf` was installed on 2026-11-12 at 08:53. It holds one look, My-Look, which draws the font set cullet-mono at 10 pt in the vga color scheme, and one terminal, console, which uses My-Look on every VT. The person who uses padnoir has never run vtium-config.

## The configuration vtium-config makes

The person types `vtium config tui` at the console. vtium-config keeps no configuration of the person's yet, so it makes one. It copies `/etc/vtium/vtium.conf` with every line and comment, and names the copy padnoir, after the machine. Nothing on the screen marks this step. The copy is the draft, a file under `~/.config/vtium/drafts/` that keeps the person's work across a quit, and the draft has no step of its own. [ADR-1403](../../architecture/platform/ADR-1403-vtium-conf-holds-the-configuration-as-one-flow-from-the-displays-to-the-input-devices-and-vtium-config-draws-it.md) D8 says where the draft comes from, and D11 why the screens never show it.

The configuration is the one open document, as a file is in a word processor. Every change edits it, ● marks it while it holds changes that are not saved, and one Save writes it to `/etc/vtium/vtium.conf`.

## The frame

vtium-config opens on the Flow tab. Every screen has the same three regions: the top row, the viewer under it, and the action bar along the bottom.

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
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Keyboard                ║  │ AT Translated Set 2…    │  │ ThinkPad Extra Buttons  │
 ║ Add keyboard… [Enter]   ║  │ in the terminal console │  │ in no path              │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘
 ↓ 2 more groups: POINTERS, OTHER INPUTS
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Pick  [Enter] Open  [a] Add path  [Ctrl+S] Save  [.] More             [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab.*

The top row holds the File and Actions menus, each marked ▾, then a bar and the six tabs. At its right are the open configuration's facts: its name, padnoir, and the count of problems that `vtium check-config` finds. No ● follows the name, because padnoir is still the installed file's text. Each count opens what it counts, so a click on "4 problems", or Alt+P, opens the Problems list. The four problems were already in the installed file, and [chapter 10](10-problems-and-fonts.md) deals with them.

The viewer shows the open tab. On the Flow tab it draws the configuration as paths. The display eDP-1 sends its picture to the terminal console, and the keyboard AT Translated Set 2 types into it. A grey line says that one keyboard and two pointers are in no path. NEW PATH below builds a path, and PARTS lists the displays and inputs the machine has. [Chapter 6](06-flow.md) wires them.

The bottom row is the action bar. Each button names its key in brackets and then its action, as `[Enter] Open`, and a click on the button does what the key does. `[F10] Menu` and `[q] Quit` sit at its right on every screen. Where the row has no room for every action, its last button is `[.] More`, which opens the rest as a small menu. So no action is out of reach of the bar.

A text block cannot show bold, underline or a highlight. So a line under each screen in this book names the letters drawn bold and underlined and the highlighted rows.

## The tabs

The letters drawn bold and underlined in the top row open the menus and tabs with Alt. Alt+F opens File, Alt+A Actions, Alt+N Fonts, Alt+E Effects, Alt+L Looks, Alt+O Flow, Alt+S Settings and Alt+H Help. The digits 1 to 6 go to the tabs as well, and Tab steps through them. The person looks at each tab once.

Alt+N opens Fonts. Each font is one row, with its kind and its state: active, not active or missing, in the same words wherever a font appears. The facts of the highlighted font sit beside the list, and a picture of the console sits under it. The first item belongs to the configuration itself: padnoir uses one font that is installed and trusted but not active, and Enter on the item activates it. [Chapter 10](10-problems-and-fonts.md) takes that up.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show: All fonts ▾                             9 fonts · 4 active │ DejaVu Sans Mono Regular
 Activate missing fonts (1) · padnoir: 1 font not active          │
 System fonts                                                     │ file    DejaVuSansMono.ttf
   DejaVuSansMono.ttf        outline  ● active                    │ folder  /usr/share/fonts/TTF
   JetBrainsMono-Regular.ttf outline  ● active                    │ kind    outline, monospaced
   Monoid-Regular.ttf        outline  ○ not active                │ covers  Latin, Greek, Cyrillic,
   mononoki-Regular.ttf      outline  ○ not active                │         box drawing
   NotoColorEmoji.ttf        color    ✗ missing · fails trust     │ sizes   any
   ter-u16n.otb              bitmap   ○ not active · My-Look      │ state   ● active · trusted
 vtium's own fonts                                                │ used by no look
   cullet-mono.otf           outline  ● active                    │
   unifont                   bitmap   ● always active             │ 10 pt: 9×20 px cells,
 Your own fonts                                                   │ 213×54 characters
   MyMono-Regular.ttf        outline  not usable yet              │
 Not installed  a font set names it                               │
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
 [Space] Activate  [A] Activate missing (1)  [Ctrl+S] Save  [.] More           [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab, row 6, `DejaVuSansMono.ttf  outline  ● active`.*

Alt+E opens Effects, the vtlooks (look files that choose filters), the color maps and the shaders. Beside the list, the highlighted vtlook's filters read left to right, in the order vtium runs them. [Chapter 5](05-looks-and-effects.md) goes through them.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show: All effects ▾            │ amber  preset · used by no look
                                │
 vtlooks                        │ frame  →  spatial: none  →  turn  →  tint  →  contrast  →  screen
   No filter     built in       │
   amber         preset         │ tint       color 255,176,0 · amount 1.0 · floor 0.0
   deuteranopia  preset         │ contrast   contrast 1.15 · brightness 0.0
   green         preset, hidden │
   green         admin's, error │ ┌ amber, on the sample screen ──────────────────────────────────┐
   night         admin's        │ │ user@padnoir:~/notes$ ls -l                                   │
   protanopia    preset         │ │ drwxr-xr-x 2 user user 4096 Nov 27 drafts                     │
   tritanopia    preset         │ │ -rw-r--r-- 1 user user 1823 Nov 27 plan.txt                   │
   white         preset         │ │ user@padnoir:~/notes$ colors                                  │
 Color maps                     │ │ red green yellow blue magenta cyan white                      │
   panel.cube    ● active       │ │ Bold text and italic text beside plain.                       │
   warm.cube     ○ not active   │ │ ┌──────────┬─────────┐                                        │
 Shaders  none installed        │ │ │ plan.txt │ 1.8 KiB │                                        │
                                │ │ └──────────┴─────────┘                                        │
                                │ │ café naïve Ångström Straße žluťoučký                          │
                                │ └───────────────────────────────────────────────────────────────┘
                                │
                                │
                                │
                                │
                                │
                                │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Activate  [Enter] Detail  [?] Search  [Ctrl+S] Save                   [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 7, `amber  preset`.*

Alt+L opens Looks. Its one list holds the looks, the font sets, the color schemes and the pointer themes, and the highlighted look's card sits beside the list above a preview. My-Look's row says that one of its fonts is not active.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Looks                            │ My-Look                                    used by console
   My-Look · 1 font not active    │
   default                        │ Font set       cullet-mono
 Font sets                        │ Size           10 pt · 9×20 px cells, 213×54 characters
   cullet-mono                    │ Color scheme   vga
   jetbrains                      │ vtlook         No filter
   terminus                       │ Pointer theme  vtium's own pictures
   unifont                        │ Fonts          1 font not active · [A] Activate
 Color schemes                    │                2 missing: Enter on the card says why
   legacy                         │
   vga                            │ ┌ as the console draws it ────────────────────────────────────┐
   nord                           │ │ user@padnoir:~/notes$ ls -l                                 │
   solarized                      │ │ drwxr-xr-x 2 user user 4096 Nov 27 drafts                   │
   dusk                           │ │ -rw-r--r-- 1 user user 1823 Nov 27 plan.txt                 │
   paper                          │ │ user@padnoir:~/notes$ colors                                │
 Pointer themes                   │ │ red green yellow blue magenta cyan white                    │
   vtium's own                    │ │ Bold text and italic text beside plain.                     │
   Adwaita                        │ │ ┌──────────┬─────────┐                                      │
                                  │ │ │ plan.txt │ 1.8 KiB │                                      │
                                  │ │ └──────────┴─────────┘                                      │
                                  │ │ café naïve Ångström Straße žluťoučký                        │
                                  │ │ → ← ↑ ↓ ● ○ ✓ ✗ … ─│┌┐└┘ ▀▄█▌▐░▒▓                           │
                                  │ └─────────────────────────────────────────────────────────────┘
                                  │ A size applies when vtium starts again.
                                  │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Edit  [n] New  [r] Remove  [?] Search  [Ctrl+S] Save  [.] More        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 4, `My-Look · 1 font not active`.*

Alt+S opens Settings, with the sections at the left and the keys of the highlighted section at the right. Each key shows its value, and whether the configuration changes it from vtium's default. The installed file sets `font-name`, so that key reads changed. The foot of the viewer describes the highlighted section in one line.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Settings                        │ Fonts and text                                      24 settings
                                 │
   Fonts and text            24  │ Font
   Colors                    22  │   font-name            cullet-mono        changed
   Keyboard and mouse        18  │   font-frits           /etc/vtium/fonts   default
   Key bindings              18  │   font-paths           unset              default
   Session and terminal      10  │   font-engine          unset              default
   Consoles and the VT        4  │   font-ligatures       off                default
   On-screen display and no… 10  │ Font size and spacing
   Displays                  12  │   font-size            0                  default
   Power and blanking         5  │   font-hinting         unset              default
   Images                    12  │   font-line-spacing    unset              default
   Running vtium              4  │ Box drawing and block characters
                                 │   glyph-source         unset              default
                                 │   glyph-source-box     unset              default
                                 │   glyph-source-block   unset              default
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
 The console's font and size, who draws box and block characters, blinking, text direction.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Keys  [?] Search  [Ctrl+S] Save                                       [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Settings tab, row 5, `Fonts and text  24`.*

Alt+H opens Help, which [chapter 8](08-finding-things.md) reads with the other ways of finding things.

## The menus

F10 or Alt+F opens File, which holds what a person does with the configuration as a document. Each item has a letter of its own, drawn bold and underlined, which picks the item while the menu is open, and its shortcut sits in the right-hand column. An item that cannot act is greyed out, with its reason after a dot. Nothing has changed yet, so Undo, Redo, Save, Show changes and Revert to saved have nothing to act on. History lists the versions that a save replaced, and no save has replaced one yet.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
┌────────────────────────────────────────────┐──────────────────────────────────────────────────────
│ New…                                       │le names types into the active terminal
│ Open                                     › │                           INPUTS
├────────────────────────────────────────────┤───────────────┐            ┌────────────────────────┐
│ Undo · nothing to undo            Ctrl+Z   │               │◄───────────│ AT Translated Set 2 k… │
│ Redo · nothing to redo            Ctrl+Y   │ every VT      │            │ keyboard · by name     │
├────────────────────────────────────────────┤───────────────┘            └────────────────────────┘
│ Save… · nothing to save           Ctrl+S   │
│ Show changes · none                        │─────────────────────────────────────────────────────
│ Edit as text…                              │n [a] adds the path.
├────────────────────────────────────────────┤═══════════════╗            ╔════════════════════════╗
│ Revert to saved… · nothing to drop         │┌──────────────────────────────────────────────┐     ║
│ History                                  › ││ No earlier version: no save replaced one yet.     │║
├────────────────────────────────────────────┤└──────────────────────────────────────────────┘═════╝
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

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. In the menu, bold and underlined, N of New…, O of Open, E of Edit as text…, H of History, Q of Quit. Highlighted: the File menu, the Flow tab, row 14, `History  ›`.*

Alt+A opens Actions, which act at once on the machine or on the console, outside the document. They never wait for Save, and they never count as a change. Install as a system font is greyed out here, because the highlighted font is already a system font.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
─────────┌──────────────────────────────────────────────┐───────────────────────────────────────────
 Show: Al│ Try live                                 t   │4 active │ DejaVu Sans Mono Regular
 ○ not al│ Read devices again                       d   │         │
 System f│ Install as a system font… · on a font of your own     ││ file    DejaVuSansMono.ttf
   DejaVu└──────────────────────────────────────────────┘         │ folder  /usr/share/fonts/TTF
   JetBrainsMono-Regular.ttf outline  ● active                    │ kind    outline, monospaced
   Monoid-Regular.ttf        outline  ○ not active                │ covers  Latin, Greek, Cyrillic,
   mononoki-Regular.ttf      outline  ○ not active                │         box drawing
   NotoColorEmoji.ttf        color    ✗ missing · fails trust     │ sizes   any
   ter-u16n.otb              bitmap   ○ not active · My-Look      │ state   ● active · trusted
 vtium's own fonts                                                │ used by no look
   cullet-mono.otf           outline  ● active                    │
   unifont                   bitmap   ● always active             │ 10 pt: 9×20 px cells,
 Your own fonts                                                   │ 213×54 characters
   MyMono-Regular.ttf        outline  not usable yet              │
 Not installed  a font set names it                               │
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

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. In the menu, bold and underlined, T of Try live, R of Read devices again. Highlighted: the Actions menu, the Fonts tab, row 3, `Try live  t`, row 6, `DejaVu`.*

## Sizes

vtium-config is designed for a terminal of 100×30, the size of every screen in this book. It runs at 80×25 too. There each screen is shorter, and the action bar moves more of its actions under `[.] More`.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 The action bar at 100 and 80 columns

 Fonts at 100 columns: [.] More holds Search, Detail, Try live
 [Space] Activate  [A] Activate missing (1)  [Ctrl+S] Save  [.] More           [F10] Menu  [q] Quit

 Fonts at 80 columns: [.] More holds Activate missing (1), Search, Detail, Try live
 [Space] Activate  [Ctrl+S] Save  [.] More                 [F10] Menu  [q] Quit

 Looks at 100 columns: [.] More holds Try live, Picture, Resize grid
 [Enter] Edit  [n] New  [r] Remove  [?] Search  [Ctrl+S] Save  [.] More        [F10] Menu  [q] Quit

 Looks at 80 columns: [.] More holds Remove, Search, Try live, Picture, Resize grid
 [Enter] Edit  [n] New  [Ctrl+S] Save  [.] More            [F10] Menu  [q] Quit

 Settings at 100 columns: [.] More holds Sections
 [Enter] Change  [?] Search  [F1] About this key  [Ctrl+S] Save  [.] More      [F10] Menu  [q] Quit

 Settings at 80 columns: [.] More holds About this key, Sections
 [Enter] Change  [?] Search  [Ctrl+S] Save  [.] More       [F10] Menu  [q] Quit

 Save, Menu and Quit stay at every width. The keys of the actions under [.] More still work,
 and F1 lists them.




────────────────────────────────────────────────────────────────────────────────────────────────────
                                                                               [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab.*

Below 80×25 no frame fits. The screen says what it needs and lists the commands, which work in a terminal of any size. A machine whose own console is smaller can still be configured over ssh, from a terminal of any size.

```text
vtium-config needs at least 80×25; this terminal is 79×24.
Make it larger, or use the commands, which work at any size:

  vtium-config flow      diff       install    get        set
  vtium-config revert    new        rule       head       look
  vtium-config draft     history    restore    library    edit
  vtium-config fonts     check      new-frit   remove-frit

[u] puts back a font or size a trial left; it has nothing to put back now.














 [u] Undo                                                             [q] Quit
```

*Marks: none. Drawn at 79×24, the size of a terminal too small for the frame.*

Where it leaves you: padnoir is open on the Settings tab, a copy of the installed file with no change: `padnoir · 4 problems`. My-Look draws cullet-mono at 10 pt, and History is empty.
