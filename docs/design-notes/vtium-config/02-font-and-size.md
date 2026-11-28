# Choosing a font and its size

Where this chapter starts: padnoir is open on the Settings tab, the same as the installed file: `padnoir · 4 problems`. My-Look draws cullet-mono at 10 pt, and History is empty.

The person finds 10 pt small on padnoir's display and wants Cullet Mono one size larger. Before changing the size, they look at the fonts the machine has.

## Looking through the fonts

Alt+N opens Fonts, and Enter on the filter line lists the filters. Each one keeps the fonts of a kind, such as the bitmap fonts or the fonts that cover one script. The line at the top says which part of the long list shows.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show                                                                       filters 1 to 18 of 36

   ● All fonts
   ○ Bitmap fonts
   ○ Outline fonts
   ○ Monospaced fonts
   ○ Proportional fonts
   ○ Nerd Fonts
   ○ Powerline fonts
   ○ Color emoji fonts
   ○ Box-drawing fonts
   ○ Fonts for Latin
   ○ Fonts for Greek
   ○ Fonts for Cyrillic
   ○ Fonts for Armenian
   ○ Fonts for Hebrew
   ○ Fonts for Arabic
   ○ Fonts for Devanagari
   ○ Fonts for Bengali
   ○ Fonts for Gurmukhi

 A filter other than All fonts leaves out each font the font helper has not described.




────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Show these  [Esc] Back                                                [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab, row 8, `○ Monospaced fonts`.*

Enter on Monospaced fonts shows those fonts alone, and the count at the right counts what passes the filter. Moving through the list shows each font's facts beside it without Enter. cullet-mono.otf is one of vtium's own fonts, active and used by My-Look's text layer.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show: Monospaced fonts ▾                      7 fonts · 4 active │ Cullet Mono Regular
 Activate missing fonts (1) · padnoir: 1 font not active          │
 System fonts                                                     │ file    cullet-mono.otf
   DejaVuSansMono.ttf        outline  ● active                    │ folder  /usr/share/vtium/fonts
   JetBrainsMono-Regular.ttf outline  ● active                    │ kind    outline, monospaced
   Monoid-Regular.ttf        outline  ○ not active                │ covers  Latin, Greek, Cyrillic,
   mononoki-Regular.ttf      outline  ○ not active                │         box drawing
   ter-u16n.otb              bitmap   ○ not active · My-Look      │ sizes   any
 vtium's own fonts                                                │ state   ● active · trusted
   cullet-mono.otf           outline  ● active                    │ used by My-Look's text
   unifont                   bitmap   ● always active             │
 ┌ picture of the console ──────────────────────────────────────┐ │ 10 pt: 9×20 px cells,
 │ user@padnoir:~/notes$ ls -l                                  │ │ 213×54 characters
 │ drwxr-xr-x 2 user user 4096 Nov 27 drafts                    │ │
 │ -rw-r--r-- 1 user user 1823 Nov 27 plan.txt                  │ │
 │ user@padnoir:~/notes$ colors                                 │ │
 │ red green yellow blue magenta cyan white                     │ │
 │ Bold text and italic text beside plain.                      │ │
 │ ┌──────────┬─────────┐                                       │ │
 │ │ plan.txt │ 1.8 KiB │                                       │ │
 │ └──────────┴─────────┘                                       │ │
 │ café naïve Ångström Straße žluťoučký                         │ │
 │ → ← ↑ ↓ ● ○ ✓ ✗ … ─│┌┐└┘ ▀▄█▌▐░▒▓                            │ │
 │                                                              │ │
 └──────────────────────────────────────────────────────────────┘ │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Activate  [A] Activate missing (1)  [Ctrl+S] Save  [.] More           [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab, row 12, `cullet-mono.otf  outline  ● active`.*

Enter on a font opens its full detail. The person opens Noto Color Emoji, which My-Look's emoji layer names but which never draws. Its detail says why: the file belongs to user 1000, and the trust rule takes only a file that root owns in a folder nobody else can write. So the font counts as missing, and no key on this screen activates it.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 NotoColorEmoji.ttf  Noto Color Emoji Regular, version 2.37

   File        /usr/local/share/fonts/NotoColorEmoji.ttf   system folder
   Kind        OpenType, outline, proportional · belongs to color emoji
   Covers      3000 characters, and no script in full
   Sizes       any size, since it has outlines
   State       ✗ missing: fails the trust rule
   Trust       fails: the file belongs to user 1000, and only root may own it
   Used by     My-Look's emoji layer



 The trust rule: root owns the file and each folder above it, and nobody else can write them.













────────────────────────────────────────────────────────────────────────────────────────────────────
 [Esc] Back                                                                    [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab.*

The person also keeps a font of their own, MyMono, in `~/.local/share/fonts`. vtium reads only system fonts, so the detail says that MyMono is not usable yet, and Enter offers to install it as one.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 MyMono-Regular.ttf  MyMono Regular, version 2.37

   File        /home/user/.local/share/fonts/MyMono-Regular.ttf    your folder
   Kind        OpenType, outline, proportional
   Covers      3000 characters: Latin
   Sizes       any size, since it has outlines
   State       not usable yet: vtium uses only system fonts. Install it as one, then activate it.
   Used by     no look


















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Install as a system font…  [Esc] Back                                 [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab.*

Install as a system font is an action, so it acts at once and is no change to the document. The question shows the command it runs before it runs it. The person has no use for MyMono on the console today, so Esc cancels and nothing is copied.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 MyMono-Regular.ttf  MyMono Regular, version 2.37

   File        /home/user/.local/share/fonts/MyMono-Regular.ttf    your folder
   Kind        OpenType, outline, proportional
   Covers      3000 characters: Latin
   Sizes       any size, since it has outlines
   State       not usable yet: vtium uses only system fonts. Install it as one, then activate it.
   Used by     no look

  ┌ Install as a system font ───────────────────────────────────────────────────────────────── × ┐
  │ Copy MyMono-Regular.ttf to /usr/local/share/fonts/MyMono-Regular.ttf? vtium-config runs:     │
  │                                                                                              │
  │   sudo vtium-config apply /home/user/.config/vtium/drafts/padnoir-font-1                     │
  │                                                                                              │
  │ sudo asks for your password. This acts now: it is not a change that waits for Save.          │
  └──────────────────────────────────────────────────────────────────────────────────────────────┘










────────────────────────────────────────────────────────────────────────────────────────────────────
 [y] Install  [Esc] Cancel
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab.*

## Changing My-Look's size

Alt+L opens Looks, and Enter on My-Look opens its card. Each line of the card holds one choice. ◂ and ▸ step a line through its choices, and Enter opens them in a list box under the line. The marks under the card name each font of My-Look by its role, with its state. The Fonts line offers `[A] Activate` for the one font that is not active, and [chapter 10](10-problems-and-fonts.md) comes back to it.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 My-Look                                                             used by the terminal console

   Name           My-Look
   Font set       cullet-mono
   Size           10 pt                                                   ◂ ▸
   Color scheme   vga
   vtlook         No filter
   Pointer theme  vtium's own pictures

 10 pt on eDP-1: 9×20 px cells, 213×54 characters. A size applies when vtium starts again.

 Fonts by role                                                   1 font not active · [A] Activate
   Text           Cullet Mono Regular and Bold                   ● active
   Box drawing    vtium's own lines, unless the text font has them all
   Scripts        Symbols Nerd Font Mono                         ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it









────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Change  [Enter] Choose  [Esc] Back  [Ctrl+S] Save  [.] More              [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 7, `Size  10 pt  ◂ ▸`.*

Enter on Font set opens its list box. It lists the font sets that are active, with ● at the one My-Look uses now. The person wants to keep cullet-mono, so Esc closes the list box and changes nothing.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 My-Look                                                             used by the terminal console

   Name           My-Look
   Font set       cullet-mono
   Size          ┌──────────────────────────────────────────┐             ◂ ▸
   Color scheme  │ ○ the font set vtium.conf sets (unifont) │
   vtlook        │ ● cullet-mono                            │
   Pointer theme │ ○ jetbrains                              │
                 │ ○ terminus                               │
 10 pt on eDP-1: │ ○ unifont                                │lies when vtium starts again.
                 └──────────────────────────────────────────┘
 Fonts by role                                                   1 font not active · [A] Activate
   Text           Cullet Mono Regular and Bold                   ● active
   Box drawing    vtium's own lines, unless the text font has them all
   Scripts        Symbols Nerd Font Mono                         ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it









────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Pick  [Enter] Keep  [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 6, `Font set  cullet-mono`, row 11, `○ terminus`.*

Enter on Size opens the sizes that My-Look's fonts can draw. An outline font offers vtium-config's usual sizes, and the foot of the box counts the rows below. The person moves down to 11 pt.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 My-Look                                                             used by the terminal console

   Name           My-Look
   Font set       cullet-mono
   Size           10 pt                                                   ◂ ▸
   Color scheme  ┌──────────────────────────────┐
   vtlook        │ ○ automatic, from vtium.conf │
   Pointer theme │ ○ 8 pt                       │
                 │ ○ 9 pt                       │
 10 pt on eDP-1: │ ● 10 pt                      │. A size applies when vtium starts again.
                 │ ○ 11 pt                      │
 Fonts by role   │ ○ 12 pt                      │                1 font not active · [A] Activate
   Text          └ ↓ 3 more ────────────────────┘                ● active
   Box drawing    vtium's own lines, unless the text font has them all
   Scripts        Symbols Nerd Font Mono                         ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it









────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Pick  [Enter] Keep  [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 7, `Size  10 pt  ◂ ▸`, row 13, `○ 11 pt`.*

Enter keeps 11 pt and closes the list box. That is one change. The top row now marks padnoir with ● and counts one change not saved, and the line under the card gives the cell and the grid at the new size. A size applies when vtium starts again on a VT.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 My-Look                                                             used by the terminal console

   Name           My-Look
   Font set       cullet-mono
   Size           11 pt                                                   ◂ ▸
   Color scheme   vga
   vtlook         No filter
   Pointer theme  vtium's own pictures

 11 pt on eDP-1: 10×22 px cells, 192×49 characters. A size applies when vtium starts again.

 Fonts by role                                                   1 font not active · [A] Activate
   Text           Cullet Mono Regular and Bold                   ● active
   Box drawing    vtium's own lines, unless the text font has them all
   Scripts        Symbols Nerd Font Mono                         ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it









────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Change  [Enter] Choose  [Esc] Back  [Ctrl+S] Save  [.] More              [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 7, `Size  11 pt  ◂ ▸`.*

On a narrower screen the top row's facts get shorter, so that the menus and the tabs never move.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 The top row at three widths

 100 columns
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems

 90 columns
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help          ● 1 · 4 problems

 80 columns
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help ● 1 · 4 problems

 On a narrow screen the name goes first, then the words, down to “● 1 · 4 problems”.
 Each count opens what it counts: “1 not saved” opens Show changes, “4 problems” the Problems list.













────────────────────────────────────────────────────────────────────────────────────────────────────
                                                                               [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the Alt letters F, A, n, E, L, o, S, H in each of the four top rows. Highlighted: the Looks tab in each.*

## Undo and Redo

Alt+F opens File, and the menu names the change Undo would take back. Ctrl+Z works without the menu.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
┌────────────────────────────────────────────┐──────────────────────────────────────────────────────
│ New…                                       │                       used by the terminal console
│ Open                                     › │
├────────────────────────────────────────────┤
│ Undo font-size = 11 in My-Look    Ctrl+Z   │
│ Redo · nothing to redo            Ctrl+Y   │                            ◂ ▸
├────────────────────────────────────────────┤
│ Save…                             Ctrl+S   │
│ Show changes                               │
│ Edit as text…                              │
├────────────────────────────────────────────┤ers. A size applies when vtium starts again.
│ Revert to saved…                           │
│ History                                  › │                   1 font not active
├────────────────────────────────────────────┤                   ● active
│ Quit                                   q   │e text font has them all
└────────────────────────────────────────────┘                   ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it









────────────────────────────────────────────────────────────────────────────────────────────────────
 [Ctrl+Z] Undo  [◂▸] Change  [Enter] Choose  [Ctrl+S] Save      [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. In the menu, bold and underlined, N of New…, O of Open, U of Undo font-size = 11 in My-Look, S of Save…, c of Show changes, E of Edit as text…, R of Revert to saved…, H of History, Q of Quit. Highlighted: the File menu, the Looks tab, row 6, `Undo font-size = 11 in My-Look  Ctrl+Z  │`.*

Ctrl+Z takes the size back. The card shows 10 pt again, the top row loses its ●, and the message line says what Undo did. Redo now names the change it would put back, and the action bar offers `[Ctrl+Y] Redo`.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
┌────────────────────────────────────────────┐──────────────────────────────────────────────────────
│ New…                                       │                       used by the terminal console
│ Open                                     › │
├────────────────────────────────────────────┤
│ Undo · nothing to undo            Ctrl+Z   │
│ Redo font-size = 11 in My-Look    Ctrl+Y   │                            ◂ ▸
├────────────────────────────────────────────┤
│ Save… · nothing to save           Ctrl+S   │
│ Show changes · none                        │
│ Edit as text…                              │
├────────────────────────────────────────────┤ers. A size applies when vtium starts again.
│ Revert to saved… · nothing to drop         │
│ History                                  › │                   1 font not active
├────────────────────────────────────────────┤                   ● active
│ Quit                                   q   │e text font has them all
└────────────────────────────────────────────┘                   ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it








 Undid font-size = 11 in My-Look. [Ctrl+Y] redoes it.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Ctrl+Y] Redo  [◂▸] Change  [Enter] Choose  [Ctrl+S] Save      [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. In the menu, bold and underlined, N of New…, O of Open, d of Redo font-size = 11 in My-Look, E of Edit as text…, H of History, Q of Quit. Highlighted: the File menu, the Looks tab, row 7, `Redo font-size = 11 in My-Look  Ctrl+Y`, row 7, `◂ ▸`.*

Ctrl+Y puts 11 pt back, and the top row counts one change not saved again. Undo reaches back as far as the last save, and no further.

## Trying the look on the console

The person wants to see 11 pt before saving it. With My-Look highlighted in the Looks list, `t`, or Actions › Try live, draws the look on this console for 10 seconds. The countdown is a small sheet, so the preview stays in sight.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Looks                            │ My-Look                                    used by console
   My-Look · 1 font not active    │
   default                        │ Font set       cullet-mono
 Font sets                        │ Size           11 pt · 10×22 px cells, 192×49 characters
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
                        ┌ Try live ───────────────────────────────────────────────────────────── × ┐
                        │ Trying My-Look (cullet-mono at 11 pt) on this console: 7 s left.         │
                        │ [k] keeps it for this session; any other key, or the end, puts it back.  │
                        └──────────────────────────────────────────────────────────────────────────┘
                                  │ └─────────────────────────────────────────────────────────────┘
                                  │ A size applies when vtium starts again.
                                  │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [k] Keep                                                                  [any other key] Put back
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 4, `My-Look · 1 font not active`.*

`k` keeps the look on the console until vtium-config quits. A trial is a preview, never a change, so the document is as it was and the top row still counts one change.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Looks                            │ My-Look                                    used by console
   My-Look · 1 font not active    │
   default                        │ Font set       cullet-mono
 Font sets                        │ Size           11 pt · 10×22 px cells, 192×49 characters
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
 The console keeps My-Look at 11 pt until you quit. To keep it after, save the change.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Edit  [n] New  [r] Remove  [?] Search  [Ctrl+S] Save  [.] More        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 4, `My-Look · 1 font not active`.*

### When Try live cannot run

Try live needs a vtium console that takes the font, and padnoir's does. Other consoles say why they cannot, in one line, where the person looks for the action. On a font that the console will not take, the line names the font, and F1 opens the rest of the reason:

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
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
   unifont                   bitmap   ● always active             │ 11 pt: 10×22 px cells,
 Your own fonts                                                   │ 192×49 characters
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
 The console refused DejaVuSansMono.ttf: it takes only trusted fonts root installed. [F1] says more.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Activate  [A] Activate missing (1)  [Ctrl+S] Save  [.] More           [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Fonts tab, row 6, `DejaVuSansMono.ttf  outline  ● active`.*

In a terminal that is not a vtium console, such as an ssh session from another machine, `t` says so once:

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Looks                            │ My-Look                                    used by console
   My-Look · 1 font not active    │
   default                        │ Font set       cullet-mono
 Font sets                        │ Size           11 pt · 10×22 px cells, 192×49 characters
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
 Try live is off: this terminal is not a vtium console. You can still save fonts and looks.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Edit  [n] New  [r] Remove  [?] Search  [Ctrl+S] Save  [.] More        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 4, `My-Look · 1 font not active`.*

The Actions menu there greys Try live out with the same reason. On a font of the person's own, Install as a system font is live:

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
─────────┌──────────────────────────────────────────────┐───────────────────────────────────────────
 Show: Al│ Try live · not a vtium console           t   │4 active │ MyMono Regular
 ○ not al│ Read devices again                       d   │         │
 System f│ Install as a system font…                    │         │ file    MyMono-Regular.ttf
   DejaVu└──────────────────────────────────────────────┘         │ folder  ~/.local/share/fonts
   JetBrainsMono-Regular.ttf outline  ● active                    │ kind    outline, proportional
   Monoid-Regular.ttf        outline  ○ not active                │ covers  Latin
   mononoki-Regular.ttf      outline  ○ not active                │ state   not usable yet: vtium
   NotoColorEmoji.ttf        color    ✗ missing · fails trust     │         uses only system fonts
   ter-u16n.otb              bitmap   ○ not active · My-Look      │ used by no look
 vtium's own fonts                                                │
   cullet-mono.otf           outline  ● active                    │
   unifont                   bitmap   ● always active             │
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
 [Enter] Pick  [→] Submenu  [Esc] Close                                                 [F10] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. In the menu, bold and underlined, R of Read devices again, I of Install as a system font…. Highlighted: the Actions menu, the Fonts tab, row 5, `Install as a system font…`, row 16, `MyMono-Regular.ttf  outline  not usable yet`.*

In a vtium console whose `allow-font-ops` is off, the line names the setting that turns trials on, and what has to follow:

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Looks                            │ My-Look                                    used by console
   My-Look · 1 font not active    │
   default                        │ Font set       cullet-mono
 Font sets                        │ Size           11 pt · 10×22 px cells, 192×49 characters
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
 Try live is off: allow-font-ops is off. Settings › Running vtium turns it on; save, restart.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Edit  [n] New  [r] Remove  [?] Search  [Ctrl+S] Save  [.] More        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 4, `My-Look · 1 font not active`.*

## Quitting

`q` quits at once and asks nothing, because the draft already holds the change. One line on the shell says that the change waits. The look the trial kept leaves the console with vtium-config, so the console draws 10 pt again.

```text
user@padnoir:~$ vtium config tui
vtium-config: padnoir has 1 change, not saved. vtium config tui opens it again.
user@padnoir:~$ _



























```

*Marks: none. The shell after vtium-config has quit.*

Where it leaves you: vtium-config has quit, and padnoir holds one change not saved, `font-size = 11` in My-Look: `padnoir ● 1 not saved · 4 problems`. The console draws 10 pt, and History is empty.
