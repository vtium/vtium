# Finding things

Where this chapter starts: padnoir is the file of 2026-11-28 11:30, with no change not saved: `padnoir · 4 problems`. My-Look draws at 12 pt, and History lists four versions.

vtium-config has four ways to find something. `?` filters the list on screen, Ctrl+F finds an item on any tab, F1 explains what is highlighted, and the Help tab lists every key. [ADR-1403](../../architecture/platform/ADR-1403-vtium-conf-holds-the-configuration-as-one-flow-from-the-displays-to-the-input-devices-and-vtium-config-draws-it.md) D11 decides all four.

## `?` filters a list

`?` opens one row stacked on the action bar, so the viewer gives up a single row and nothing covers the list. The entry sits at the left after the `?` prompt, and the syntax help sits at the right, with the fields this list matches. The patterns ignore case: `^mono` keeps what starts with "mono", `$vga` what ends with "vga", and `*nerd` or plain `nerd` what holds "nerd" anywhere. Enter keeps the filter and returns to the list, and Esc, or Backspace on an empty entry, closes the row and clears it.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show: All fonts ▾ · matching ^mono                  2 of 9 fonts │ Monoid Regular
                                                                  │
 System fonts                                                     │ file    Monoid-Regular.ttf
   Monoid-Regular.ttf        outline  ○ not active                │ folder  /usr/share/fonts/TTF
   mononoki-Regular.ttf      outline  ○ not active                │ kind    outline, monospaced
 ┌ picture of the console ──────────────────────────────────────┐ │ covers  Latin, Greek, Cyrillic
 │ user@padnoir:~/notes$ ls -l                                  │ │ sizes   any
 │ drwxr-xr-x 2 user user 4096 Nov 27 drafts                    │ │ state   ○ not active
 │ -rw-r--r-- 1 user user 1823 Nov 27 plan.txt                  │ │         trusted
 │ user@padnoir:~/notes$ colors                                 │ │ used by no look
 │ red green yellow blue magenta cyan white                     │ │
 │ Bold text and italic text beside plain.                      │ │
 │ ┌──────────┬─────────┐                                       │ │
 │ │ plan.txt │ 1.8 KiB │                                       │ │
 │ └──────────┴─────────┘                                       │ │
 │ café naïve Ångström Straße žluťoučký                         │ │
 │ → ← ↑ ↓ ● ○ ✓ ✗ … ─│┌┐└┘ ▀▄█▌▐░▒▓                            │ │
 │                                                              │ │
 │                                                              │ │
 │                                                              │ │
 │                                                              │ │
 │                                                              │ │
 │                                                              │ │
 │                                                              │ │
 └──────────────────────────────────────────────────────────────┘ │
 ? ^mono▏                     matches family and file name · ^start  $end  *anywhere · case ignored
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Keep  [Esc] Clear                                                            [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab, row 6, `Monoid-Regular.ttf  outline  ○ not active`.*

When nothing matches, the list says so in one line and says how to go on.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show: All fonts ▾ · matching $vga                   0 of 9 fonts │
                                                                  │
 No font's family or file name ends with “vga”.                   │
 [Backspace] edits the pattern; [Esc] clears it.                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
                                                                  │
 ? $vga▏                      matches family and file name · ^start  $end  *anywhere · case ignored
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Keep  [Esc] Clear                                                            [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab.*

In Settings the search matches each key and its description, so a setting is found by what it does as well as by its name. `*bright` finds `display-depth`, whose description says that displays are told apart "never by their brightness". The sections without a match leave the left pane while the search is open.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Settings matching *bright       │ Displays                                       1 of 12 settings
                                 │
   Displays                   1  │ Display colors and density
                                 │   display-depth        unset              default
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
  display-depth: how many colors or shades each display can show · “…never by their brightness”
 ? *bright▏                    matches key and description · ^start  $end  *anywhere · case ignored
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Keep  [Esc] Clear                                                            [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Settings tab, row 6, `display-depth  unset  default`.*

Only the screens that declare the search answer `?`. They are the Fonts list, the Effects list, the Looks tab's lists, Settings, Help, and a list box with more choices than it shows. Elsewhere `?` does nothing, so the Flow page stays uncluttered.

## Ctrl+F finds anything

Ctrl+F opens Find, the same row with the prompt `Find` and the same patterns. Find searches every tab: the settings by key and description, the fonts, font sets, looks, color schemes and effects by name, and the Flow page's parts by name and label. Its matches take the rows above the search row, in the tabs' order, each with where it lives. The person types `*term`.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
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
────────────────────────────────────────────────────────────────────────────────────────────────────
  ter-u16n.otb    Fonts                            Terminus Regular · bitmap · ○ not active
  term            Settings › Session and terminal  The value of TERM that programs see.
  font-name       Settings › Fonts and text        … or else the font set terminus …
  program-blank   Settings › Power and blanking    … as setterm --blank does …

 4 found among the settings, fonts, font sets, looks, color schemes, effects and parts
 Find *term▏                                                 ^start  $end  *anywhere · case ignored
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Go  [↑↓] Match  [Esc] Close                                           [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 23, `term  Settings › Session and terminal  The value of TERM that programs see.`.*

Enter goes to the highlighted item, here the key `term` in Settings, as if the person had moved there. Esc closes Find and leaves the screen as it was.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Settings                        │ Session and terminal                                10 settings
                                 │
   Fonts and text            24  │ The console's command
   Colors                    22  │   login                unset              default
   Keyboard and mouse        18  │   oneshot              unset              default
   Key bindings              18  │ The console's terminal type
   Session and terminal      10  │   term                 unset              default
   Consoles and the VT        4  │   reset-env            unset              default
   On-screen display and no… 10  │   backspace-delete     unset              default
   Displays                  12  │ Scrollback
   Power and blanking         5  │   sb-size              unset              default
   Images                    12  │   sb-size-cap          unset              default
   Running vtium              4  │   sb-memory            unset              default
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
                                 │
  term: the value of TERM that programs on the console see.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Change  [?] Search  [F1] About this key  [Ctrl+S] Save  [.] More      [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Settings tab, row 9, `term  unset  default`.*

## F1 explains what is highlighted

A line that the screen cuts to fit opens in full on F1, as a setting's help does in [chapter 7](07-settings.md), and so does a message that ends with `[F1]`. Where nothing more specific is highlighted, F1 opens the Help tab on the topic of the screen the person is on. The topic lists every key of that screen, then the keys that work everywhere. Here F1 was pressed on the Looks tab.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Help                │ Looks  Each look, font set, color scheme and pointer theme, with a preview.
                     │
  Fonts              │   [↑] [↓] [PgUp] [PgDn] [Home] [End]  move
  Effects            │   [Enter]  edit a look, open a detail    [n]      new look or font set
  Looks              │   [r]      remove a look, mark a set     [t]      try live for 10 s
  Flow               │   [w]      let programs resize the grid  [p]      picture or sample
  Settings           │   [Space]  activate a scheme or theme    [A]      activate missing fonts
  Menus and Save     │   [?]      search the list
  Mouse              │
  Commands           │ Everywhere
  About              │   [F10]     menu bar          [Alt+F]   File       [Alt+A]   Actions
                     │   [Ctrl+S]  Save…             [Ctrl+Z]  Undo       [Ctrl+Y]  Redo
                     │   [Ctrl+F]  find anything     [F1]      help       [Alt+P]   Problems
                     │   [Esc]     back, close       [q]       quit
                     │   [Alt+N] Fonts  [Alt+E] Effects  [Alt+L] Looks  [Alt+O] Flow
                     │   [Alt+S] Settings  [Alt+H] Help  [1] to [6] a tab
                     │   [Tab] [Shift+Tab]  the next tab, the one before
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Topic  [?] Search                                                        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Help tab, row 7, `Looks`.*

## The Help tab

Alt+H opens Help. The topics sit at the left, the six tabs first, and the right shows the highlighted topic, so the page needs no long scroll. Each tab's topic says what the tab does and lists its keys. The Fonts topic also lists the marks a font carries everywhere.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Help                │ Fonts  The fonts on this system, each with its mark, and a picture.
                     │
  Fonts              │   [Space]   activates the highlighted font, or deactivates it; asks first
  Effects            │             when a look uses it
  Looks              │   [A]       activates every font the configuration uses that is not active
  Flow               │   [Enter]   opens a font's full detail, or the filters on the filter line
  Settings           │   [t]       tries the highlighted font on this console for 10 seconds
  Menus and Save     │   [?]       searches the list by family and file name
  Mouse              │
  Commands           │ Marks, the same wherever a font appears
  About              │   ● active           vtium draws with it
                     │   ○ not active       installed and trusted, waiting for [Space] or [A]
                     │   ✗ missing          no file of it is installed and passes the trust rule
                     │   N fonts not active in a look's or the configuration's own row
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Topic  [?] Search                                                        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Help tab, row 5, `Fonts`.*

Menus and Save lists the menus, Save, Undo and Redo, F1, Find, the search and the tabs' keys.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Help                │ Menus and Save
                     │
  Fonts              │   [F10]      opens the menu bar; [Alt+F] opens File and [Alt+A] Actions
  Effects            │   [Ctrl+S]   Save: shows the review of the changes, then writes /etc
  Looks              │              through sudo once
  Flow               │   [Ctrl+Z]   Undo takes back the last change; [Ctrl+Y] Redo puts it back
  Settings           │   [Alt+P]    opens the Problems list, as a click on the count at the top
  Menus and Save     │   [F1]       help on the highlighted line, or this screen's topic here
  Mouse              │   [Ctrl+F]   finds a setting, font, look, color scheme, effect or part
  Commands           │   [?]        searches the list, on the screens that offer it
  About              │
                     │ Tabs
                     │   [Alt+N] Fonts  [Alt+E] Effects  [Alt+L] Looks  [Alt+O] Flow
                     │   [Alt+S] Settings  [Alt+H] Help
                     │   [1] to [6] go straight to a tab; [Tab] and [Shift+Tab] go to the next
                     │   and the one before
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Topic  [?] Search                                                        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Help tab, row 10, `Menus and Save`.*

Mouse says what a click does, and how to select text to copy while vtium-config has the mouse.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Help                │ Mouse
                     │   A click picks a menu, a tab, an item or a row, presses a button, and
  Fonts              │   closes a menu or a sheet. A click on a count at the top right opens
  Effects            │   Show changes or the Problems list.
  Looks              │   To select text to copy while vtium-config has the mouse, hold Shift
  Flow               │   and drag.
  Settings           │
  Menus and Save     │
  Mouse              │
  Commands           │
  About              │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Topic  [?] Search                                                        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Help tab, row 12, `Commands`.*

Commands lists the commands, which work from a shell in a terminal of any size.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Help                │ Commands  Each works from a shell, in a terminal of any size:
                     │
  Fonts              │   vtium-config flow      diff       install    get        set
  Effects            │   vtium-config revert    new        rule       head       look
  Looks              │   vtium-config draft     history    restore    library    edit
  Flow               │   vtium-config fonts     check      new-frit   remove-frit
  Settings           │
  Menus and Save     │
  Mouse              │
  Commands           │
  About              │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Topic  [?] Search                                                        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Help tab, row 12, `Commands`.*

About shows vtium-config's name, version and logo, as `vtium-config --help about` prints them.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Help                │
                     │     ▄▀▀▄ ▀█▀ ▀█▀ █  █ █▄ ▄█
  Fonts              │     █  █  █   █  █  █ █ ▀ █
  Effects            │      ▀▀   ▀  ▀▀▀  ▀▀  ▀   ▀
  Looks              │
  Flow               │ vtium-config 0.1.0-alpha.13
  Settings           │ vtium-config sets up the vtium console: its fonts and frits, its
  Menus and Save     │ settings and heads, checked before they are saved.
  Mouse              │
  Commands           │
  About              │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Topic  [?] Search                                                        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Help tab, row 13, `About`.*

Help declares the search too. It matches each topic's name and lines, and keeps the lines that match under their topic.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Help                │ Fonts  2 lines match
                     │
  Fonts              │   [Space]   activates the highlighted font, or deactivates it; asks first
  Effects            │   [A]       activates every font the configuration uses that is not active
  Looks              │
  Flow               │
  Settings           │
  Menus and Save     │
  Mouse              │
  Commands           │
  About              │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
                     │
 ? *activ▏                          matches topic and line · ^start  $end  *anywhere · case ignored
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Keep  [Esc] Clear                                                            [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Help tab, row 5, `Fonts`.*

Where it leaves you: nothing has changed since the save of 2026-11-28 11:30: `padnoir · 4 problems`. History lists four versions.
