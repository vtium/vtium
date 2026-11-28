# Problems and fonts not yet active

Where this chapter starts: padnoir is the file of 2026-11-28 11:30, with two terminals: `padnoir · 4 problems`. The person keeps padnoir and travel, and History lists four versions.

The four problems have been in the top row since [chapter 1](01-first-run.md). They are warnings, so Save has worked with them all along, but each one is something padnoir draws or wires without. This chapter goes through them.

## The Problems list

Alt+P, or a click on "4 problems", opens the Problems list. Each problem is one line with where it is: the tab, the item and, for a line of vtium.conf, its line number. Problems that stop Save come first, then the warnings, each group in the tabs' order. Enter or a click goes to the place, so the person can fix it there. [ADR-1403](../../architecture/platform/ADR-1403-vtium-conf-holds-the-configuration-as-one-flow-from-the-displays-to-the-input-devices-and-vtium-config-draws-it.md) D11 sets the list.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
┌ Problems ───────────────────────────────────────────────────────────────────────────────────── × ┐
│ 4 problems  each a warning: Save still works                                                     │
│                                                                                                  │
│  Stop Save   none                                                                                │
│  Warnings                                                                                        │
│    Fonts › padnoir's fonts            1 is not active; [Enter] activates ter-u16n.otb            │
│    Fonts › NotoColorEmoji.ttf         missing: user 1000 owns the file, so it fails trust        │
│    Looks › My-Look › Scripts          the layer nerd-symbols names Symbols Nerd Font Mono, whic… │
│    Flow › PARTS · vtium.conf line 32  1 keyboard and 2 pointers are in no path                   │
│ Problems that stop Save come first, then warnings, each group in tab order.                      │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
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
 [Enter] Go to  [F1] Whole line  [Esc] Close                                   [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 10, `Fonts › padnoir's fonts  1 is not active; [Enter] activates ter-u16n.otb`.*

A line too long for the list is cut, and F1 on it opens all of it. The Looks line says that the layer nerd-symbols names a font that is not installed, where vtium looked for it, and which package ships it.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 CONFIGURATION   policy open: a keyboard no rule names types into the active terminal
 DISPLAYS                     TERMINALS                                  INPUTS
┌ Problems ───────────────────────────────────────────────────────────────────────────────────── × ┐
│ 4 problems  each a warning: Save still works                                                     │
│                                                                                                  │
│  Stop Save   none                                                                                │
│  Warnings                                                                                        │
│    Fonts › padnoir's fonts            1 is not active; [Enter] activates ter-u16n.otb            │
│    Fonts › NotoColorEmoji.ttf         missing: user 1000 owns the file, so it fails trust        │
│    Looks › My-Look › Scripts          the layer nerd-symbols names Symbols Nerd Font Mono, whic… │
│    Flow › PARTS · vtium.conf line 32  1 keyboard and 2 pointers are in no path                   │
│ Prob┌ Looks › My-Look › Scripts ────────────────────────────────────────────────────────── × ┐   │
└─────│ The layer nerd-symbols names Symbols Nerd Font Mono, which is not installed. vtium     │───┘
 ╔════│ searched /usr/share/fonts, /usr/local/share/fonts and /usr/share/vtium/fonts. The      │═══╗
 ║ Dis│ package ttf-nerd-fonts-symbols-mono ships it.                                          │   ║
 ║ Cho│                                                                                        │   ║
 ╚════│ [Enter] on the problem goes to My-Look's card, at the Scripts line.                    │═══╝
 ─────└────────────────────────────────────────────────────────────────────────────────────────┘───
 PARTS
 DISPLAYS
 ╔═════════════════════════╗  ┌─────────────────────────┐  ┌─────────────────────────┐
 ║ Display                 ║  │ eDP-1                   │  │ HDMI-A-1                │
 ║ Add display… [Enter]    ║  │ in console and work     │  │ not connected           │
 ╚═════════════════════════╝  └─────────────────────────┘  └─────────────────────────┘

 ↓ 3 more groups: KEYBOARDS, POINTERS, OTHER INPUTS
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Flow tab, row 12, `Looks › My-Look › Scripts  the layer nerd-symbols names Symbols Nerd Font Mono, whic…`.*

Enter on that line opens My-Look's card with the line at fault highlighted and the problem in one line below. Alt+P goes back to the list.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 My-Look                                                             used by the terminal console

   Name           My-Look
   Font set       cullet-mono
   Size           12 pt                                                   ◂ ▸
   Color scheme   vga
   vtlook         No filter
   Pointer theme  vtium's own pictures

 12 pt on eDP-1: 11×24 px cells, 174×45 characters. A size applies when vtium starts again.

 Fonts by role                                                   1 font not active · [A] Activate
   Text           Cullet Mono Regular and Bold                   ● active
   Box drawing    vtium's own lines, unless the text font has them all
   Scripts        Symbols Nerd Font Mono                         ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it








 Problem 3 of 4: the layer nerd-symbols names Symbols Nerd Font Mono, which is not installed.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] What it misses  [Alt+P] Problems  [Esc] Back  [Ctrl+S] Save           [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 17, `Scripts  Symbols Nerd Font Mono  ✗ missing  [Enter] says why`.*

Enter on the line opens what the look still misses. Symbols Nerd Font Mono needs a package that root installs, and Noto Color Emoji needs root to own its file. Neither is something vtium-config can do, so the person leaves both. Terminus is installed and trusted, and only waits to be activated.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 My-Look › What it still misses

   Scripts   the layer nerd-symbols names Symbols Nerd Font Mono, which is not installed.
             Searched /usr/share/fonts, /usr/local/share/fonts and /usr/share/vtium/fonts.
             The package ttf-nerd-fonts-symbols-mono ships it.
   Emoji     Noto Color Emoji is ✗ missing: it fails the trust rule, since user 1000
             owns the file, so vtium never activates it.
   Fallback  Terminus is installed and ○ not active: [A] Activate missing (1) activates it.

   Text and box drawing miss nothing.
















────────────────────────────────────────────────────────────────────────────────────────────────────
 [A] Activate missing (1)  [Esc] Back                                          [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab.*

The font set's own detail shows the same marks by role, and offers the same fix.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Font set cullet-mono                                                   used by My-Look and Evening

   The console reads it without errors.

 Its fonts, by role                                                1 font not active
   Text           Cullet Mono Regular and Bold                   ● active
   Box drawing    vtium's own lines, unless the text font has them all
   Scripts        the layer nerd-symbols: Symbols Nerd Font Mono ✗ missing
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active

 [Enter] says which fonts it misses and why; [A] Activate missing (1) activates Terminus.














────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] What it misses  [A] Activate missing (1)  [t] Try live  [Esc] Back    [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab.*

## Activate missing

A font draws on the console only while the batch, the file that lists the active resources, holds it. So a look whose font set names a font that is installed but not active draws without it, and Terminus has been missing from My-Look's fallback layer. vtium-config lists, for the open configuration, every font that a font set it uses would draw and that is installed and trusted but not active. [ADR-1205](../../architecture/font/ADR-1205-a-look-is-assembled-from-the-active-resources-and-a-resource-can-be-activated-only-from-a-root-owned-folder-on-a-filesystem-that-stays-mounted.md) D4 says which fonts count. The Fonts tab's first item offers to activate them, with the count, and so does `[A] Activate missing (1)` on the action bar.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show: All fonts ▾                             9 fonts · 4 active │ Terminus Regular
 Activate missing fonts (1) · padnoir: 1 font not active          │
 System fonts                                                     │ file    ter-u16n.otb
   DejaVuSansMono.ttf        outline  ● active                    │ folder  /usr/share/fonts/misc
   JetBrainsMono-Regular.ttf outline  ● active                    │ kind    bitmap, 8×16
   Monoid-Regular.ttf        outline  ○ not active                │ covers  Latin, Greek, Cyrillic,
   mononoki-Regular.ttf      outline  ○ not active                │         box drawing
   NotoColorEmoji.ttf        color    ✗ missing · fails trust     │ sizes   12 pt
   ter-u16n.otb              bitmap   ○ not active · My-Look      │ state   ○ not active · trusted
 vtium's own fonts                                                │ used by My-Look's fallback
   cullet-mono.otf           outline  ● active                    │
   unifont                   bitmap   ● always active             │ [Space] or [A] activates it
 Your own fonts                                                   │
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

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab, row 11, `ter-u16n.otb  bitmap  ○ not active · My-Look`.*

`A` puts every such font into the configuration's batch as one change, through the same path as Space on one font. Undo takes the change back, and nothing is activated until Save. The message line says what the action did. Noto Color Emoji fails the trust rule, so the action never activates it, and F1 lists it with its reason. The item at the top of the list goes, because no font is left for it to activate.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 3 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show: All fonts ▾                             9 fonts · 5 active │ Terminus Regular
                                                                   │
 System fonts                                                     │ file    ter-u16n.otb
   DejaVuSansMono.ttf        outline  ● active                    │ folder  /usr/share/fonts/misc
   JetBrainsMono-Regular.ttf outline  ● active                    │ kind    bitmap, 8×16
   Monoid-Regular.ttf        outline  ○ not active                │ covers  Latin, Greek, Cyrillic,
   mononoki-Regular.ttf      outline  ○ not active                │         box drawing
   NotoColorEmoji.ttf        color    ✗ missing · fails trust     │ sizes   12 pt
   ter-u16n.otb              bitmap   ● active · not saved        │ state   ● active · trusted
 vtium's own fonts                                                │ used by My-Look's fallback
   cullet-mono.otf           outline  ● active                    │
   unifont                   bitmap   ● always active             │ activated, not saved
 Your own fonts                                                   │
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
 Activated ter-u16n.otb as one change. 1 more cannot be activated [F1]. [Ctrl+Z] undoes it.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Deactivate  [Ctrl+Z] Undo  [?] Search  [Ctrl+S] Save                  [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab, row 11, `ter-u16n.otb  bitmap  ● active · not saved`.*

Enter on the Fonts line of the Problems list does the same, and the line leaves the list. The top row now counts three problems.

### When a problem stops Save

Warnings never stop Save, but an error does. Had the person also typed `palette = solarised` in Settings, the check would refuse the file. Save's review would then list the error first, with its place and its line, and grey Save out until the error is fixed. Enter goes to the line.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 2 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Save to /etc/vtium/vtium.conf                           2 changes · 1 font activated

 1 error stops Save: vtium check-config refuses the file until it is fixed.

   Settings › Colors › palette    line 9: palette = solarised, needs a built-in palette,
                                  custom, or a NAME.palette file

 3 warnings (they alone would not stop Save), in tab order
   Fonts › NotoColorEmoji.ttf       missing: it fails the trust rule
   Looks › My-Look › Scripts        the layer nerd-symbols names a font that is not installed
   Flow › PARTS · line 32           1 keyboard and 2 pointers are in no path

 [Enter] goes to the highlighted line, where you fix it; then [Ctrl+S] shows this review again.













────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Go to  [Ctrl+S] Save  [Esc] Back                                      [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 7, `Settings › Colors › palette  line 9: palette = solarised, needs a built-in palette,`, row 8, `custom, or a NAME.palette file`.*

## Saving the font

The person saves the activation. The save's review lists Terminus under the fonts it activates, and the font draws in My-Look's fallback layer from the next login.

Where it leaves you: the save of 2026-11-28 at 12:10 activates Terminus: `padnoir · 3 problems`. Noto Color Emoji and Symbols Nerd Font Mono stay missing, and one keyboard and two pointers are in no path. History lists five versions, the newest the file of 11:30.
