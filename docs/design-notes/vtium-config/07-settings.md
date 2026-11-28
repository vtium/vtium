# Settings

Where this chapter starts: the save of 2026-11-28 at 11:30 holds two terminals on eDP-1, console on VT 1 with My-Look and work on VT 2 with Evening: `padnoir · 4 problems`. History lists four versions.

Settings holds every key of vtium.conf's top level, the ones that apply to the whole console rather than to one look or one terminal. `/etc/vtium/vtium.conf.example` and `vtium --help` describe every one.

## A section's keys

The sections sit at the left and the keys of the highlighted section at the right, so moving between sections shows their keys without Enter. → or Enter moves into the keys, which stand in headed groups. The highlighted key's help is one line at the foot of the viewer, and F1 opens all of it.

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
  font-name: the console's font, a font set's name, a kbd console font, or a font file's path.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Change  [?] Search  [F1] About this key  [Ctrl+S] Save  [.] More      [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Settings tab, row 6, `font-name  cullet-mono  changed`.*

A key with a fixed set of values opens them in a list box under its line, as every line with choices does. The person sets `glyph-source` to auto, so that vtium draws box and block characters itself only where the font lacks them. Enter keeps the pick as one change.

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
                                 │   glyph-source-box   ┌────────────────────────────────────────┐
                                 │   glyph-source-block │ ● unset    vtium draws all six ranges  │
                                 │                      │ ○ font                                 │
                                 │                      │ ○ sprites                              │
                                 │                      │ ○ auto                                 │
                                 │                      └────────────────────────────────────────┘
                                 │
                                 │
                                 │
                                 │
                                 │
  glyph-source: who draws box-drawing, block and similar characters: the font, or vtium itself.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Pick  [Enter] Keep  [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Settings tab, row 16, `glyph-source  unset  default`, row 21, `○ auto`.*

A key whose value is typed takes the text on its own line, and an empty value means vtium's default. The person starts on `font-name`, then leaves it as it was with Esc.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Settings                        │ Fonts and text                                      24 settings
                                 │
   Fonts and text            24  │ Font
   Colors                    22  │   font-name            cullet-mono_
   Keyboard and mouse        18  │   font-frits           /etc/vtium/fonts   default
   Key bindings              18  │   font-paths           unset              default
   Session and terminal      10  │   font-engine          unset              default
   Consoles and the VT        4  │   font-ligatures       off                default
   On-screen display and no… 10  │ Font size and spacing
   Displays                  12  │   font-size            0                  default
   Power and blanking         5  │   font-hinting         unset              default
   Images                    12  │   font-line-spacing    unset              default
   Running vtium              4  │ Box drawing and block characters
                                 │   glyph-source         auto               changed
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
  Type the value and press [Enter]. Leave it empty to use the default.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Keep  [Esc] Cancel                                                           [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Settings tab, row 6, `font-name  cullet-mono_`.*

F1 on a key opens its whole help, as vtium.conf's own reference gives it, with its default and its value now.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Settings › Fonts and text › font-name

 The console's font. The value is the name of a font set (a frit file, NAME.vtfrit in
 /etc/vtium/fonts). It can also be the name of a kbd console font, or the absolute path of a
 font file, such as a PSF, BDF or TrueType font; vtium then uses that one font. When unset,
 vtium uses the font from vconsole.conf, or else the font set terminus. If vtium cannot find
 the font, it falls back to its built-in font and says so in its log.

   Default     unset
   Now         cullet-mono   changed
















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Esc] Back                                                                    [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Settings tab.*

## Edit as text

File › Edit as text… opens the configuration in the person's editor: `$VISUAL`, else `$EDITOR`, else vi. It is the path `vtium config edit` already takes. While the editor runs, vtium-config turns mouse reporting off and steps aside, and it gives the terminal its own mode back. The person's editor is vim, and they change My-Look's size to 13 by hand. The `glyph-source = auto` line from the list box is already in the text.

```text

# Everything after this line is in the order vtium-config writes it.

[settings]
hwaccel = on
font-name = cullet-mono
osd = minimal
palette = vga
allow-font-ops = on
glyph-source = auto

# My-Look: the laptop's own panel.
[look My-Look]
palette = vga
font-size = 13
font-name = cullet-mono

[look Evening]
palette = nord
font-size = 12
font-name = cullet-mono
vtlook = night

[head console]
look = My-Look
vt = 1


padnoir.conf [+]                                                                  16,13   Top
-- INSERT --
```

*Marks: none. vim draws the file, its status line and its mode.*

When the editor exits, vtium-config runs `vtium check-config` on the text and says what the edit changed. `k` keeps the edit as a change, `e` opens the editor again where the person left it, and Esc drops the edit.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 My-Look                                                             used by the terminal console

   Name           My-Look
   Font set       cullet-mono
   Size           13 pt                                                   ◂ ▸
   Color scheme   vga
   vtl┌ Edit as text ───────────────────────────────────────────────────────────────────── × ┐
   Poi│ Your edit changed 1 line: font-size = 12 became font-size = 13.                      │
      │ vtium check-config: no problems.                                                     │
 13 pt│                                                                                      │
      │ Keep puts the edit in the configuration as a change, not saved, like any other.      │
 Fonts└──────────────────────────────────────────────────────────────────────────────────────┘
   Text           Cullet Mono Regular and Bold                   ● active
   Box drawing    vtium's own lines, unless the text font has them all
   Scripts        Symbols Nerd Font Mono                         ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it









────────────────────────────────────────────────────────────────────────────────────────────────────
 [k] Keep  [e] Edit again  [Esc] Drop the edit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 7, `Size  13 pt  ◂ ▸`.*

The person keeps it, and the top row counts two changes not saved. Had they typed `13pt`, the check would have found an error. The sheet then shows the error with its line, and Edit again comes first. Keep would still keep the text, since a configuration may be unfinished, but Save refuses it until the error is fixed. So nothing broken reaches `/etc`.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 My-Look                                                             used by the terminal console

   Name           My-Look
   Font set       cullet-mono
   Size           12 pt                                                   ◂ ▸
   Color scheme   vga
   vtl┌ Edit as text ───────────────────────────────────────────────────────────────────── × ┐
   Poi│ Your edit changed 2 lines.                                                           │
      │ vtium check-config found 1 error:                                                    │
 10 pt│   line 16: font-size = 13pt, needs a number of points from 4 to 200, or 0            │
      │                                                                                      │
 Fonts│ Edit again opens the editor on line 16. Keep keeps the text with its error, and Save │
   Tex│ stays off until the error is fixed.                                                  │
   Box└──────────────────────────────────────────────────────────────────────────────────────┘
   Scripts        Symbols Nerd Font Mono                         ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it









────────────────────────────────────────────────────────────────────────────────────────────────────
 [e] Edit again  [k] Keep  [Esc] Drop the edit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 7, `Size  12 pt  ◂ ▸`.*

## Revert to saved

In the console's preview, 13 pt looks no better than 12 pt, and the person no longer wants either change. File › Revert to saved… drops every change since the last save. It asks first and counts what it drops.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 2 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 My-Look                                                             used by the terminal console

   Name           My-Look
   Font set       cullet-mono
   Size           13 pt                                                   ◂ ▸
   Color scheme   vga
   vtl┌ Revert to saved ────────────────────────────────────────────────────────────────── × ┐
   Poi│ Drop every change since the last save?                                               │
      │ 2 changes are lost: 1 line added and 1 line changed.                                 │
 10 pt│                                                                                      │
      │ The configuration goes back to /etc/vtium/vtium.conf as saved on 2026-11-28 11:30.   │
 Fonts└──────────────────────────────────────────────────────────────────────────────────────┘
   Text           Cullet Mono Regular and Bold                   ● active
   Box drawing    vtium's own lines, unless the text font has them all
   Scripts        Symbols Nerd Font Mono                         ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it









────────────────────────────────────────────────────────────────────────────────────────────────────
 [y] Revert  [Esc] Cancel
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 7, `Size  13 pt  ◂ ▸`.*

`y` reverts, and the top row loses its ●. Revert is one step of the undo history, so Ctrl+Z straight after it would bring both changes back.

Where it leaves you: padnoir is the file of 2026-11-28 11:30 again, with no change not saved: `padnoir · 4 problems`. My-Look draws at 12 pt, and History lists four versions.
