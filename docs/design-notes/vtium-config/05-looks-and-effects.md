# Looks and effects

Where this chapter starts: the save of 2026-11-28 at 10:20 holds `osd = minimal` and My-Look at 12 pt, and the console draws 12 pt: `padnoir · 4 problems`. History lists two versions, of 2026-11-28 10:05 and of 2026-11-12 08:53.

A look puts together what the console draws with: a font set and its size, a color scheme, a vtlook that holds the frame's filters, and a pointer theme. The pieces are resources. A look can use a resource only while it is active: root trusts it, and the batch, the list of active resources, holds it. [ADR-1205](../../architecture/font/ADR-1205-a-look-is-assembled-from-the-active-resources-and-a-resource-can-be-activated-only-from-a-root-owned-folder-on-a-filesystem-that-stays-mounted.md) decides how a look is assembled. The person wants a second look for the evening, with darker colors.

## The Looks tab's other lists

Below the looks, the Looks list holds the font sets, the color schemes and the pointer themes. With a color scheme highlighted, the place of the card shows the scheme's facts and its colors, and Space activates or deactivates it. vga is built in and always active.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Looks                            │ vga  color scheme · built in, always active
   My-Look · 1 font not active    │
   default                        │ used by  My-Look
 Font sets                        │
   cullet-mono                    │
   jetbrains                      │
   terminus                       │
   unifont                        │
 Color schemes                    │
   legacy                         │ ┌ vga's colors ───────────────────────────────────────────────┐
   vga                            │ │ user@padnoir:~/notes$ ls -l                                 │
   nord                           │ │ drwxr-xr-x 2 user user 4096 Nov 27 drafts                   │
   solarized                      │ │ -rw-r--r-- 1 user user 1823 Nov 27 plan.txt                 │
   dusk                           │ │ user@padnoir:~/notes$ colors                                │
   paper                          │ │ red green yellow blue magenta cyan white                    │
 Pointer themes                   │ │ Bold text and italic text beside plain.                     │
   vtium's own                    │ │ ┌──────────┬─────────┐                                      │
   Adwaita                        │ │ │ plan.txt │ 1.8 KiB │                                      │
                                  │ │ └──────────┴─────────┘                                      │
                                  │ │ café naïve Ångström Straße žluťoučký                        │
                                  │ │ → ← ↑ ↓ ● ○ ✓ ✗ … ─│┌┐└┘ ▀▄█▌▐░▒▓                           │
                                  │ │                                                             │
                                  │ └─────────────────────────────────────────────────────────────┘
                                  │
                                  │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Open  [Space] Activate  [?] Search  [Ctrl+S] Save                     [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 13, `vga`.*

Enter on a color scheme opens its detail: its file, its state, the looks that use it, and its 16 colors beside text in its own foreground and background.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Color scheme paper

   File      /usr/share/vtium/palettes/paper.palette
   State     active · trusted
   Used by   no look

 Its 16 colors, then its foreground and background

    0 ████ #101820        1 ████ #b42828        2 ████ #2c343c        3 ████ #3a424a
    4 ████ #485058        5 ████ #565e66        6 ████ #646c74        7 ████ #727a82
    8 ████ #808890        9 ████ #8e969e       10 ████ #9ca4ac       11 ████ #aab2ba
   12 ████ #b8c0c8       13 ████ #c6ced6       14 ████ #d4dce4       15 ████ #e2eaf2

   foreground #f0f0f0, background #101010

    ~/notes $ ls -l todo.txt error done










────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Deactivate  [Esc] Back                                                [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab.*

The pointer themes are the installed cursor themes, and each is activated by its folder, with every file in the folder. Adwaita is installed and trusted, and not active.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Looks                            │ Adwaita  pointer theme
   My-Look · 1 font not active    │
   default                        │ state    ○ not active · trusted
 Font sets                        │ used by  no look
   cullet-mono                    │
   jetbrains                      │
   terminus                       │
   unifont                        │
 Color schemes                    │
   legacy                         │ ┌ pointer ────────────────────────────────────────────────────┐
   vga                            │ │ user@padnoir:~/notes$ ls -l                                 │
   nord                           │ │ drwxr-xr-x 2 user user 4096 Nov 27 drafts                   │
   solarized                      │ │ -rw-r--r-- 1 user user 1823 Nov 27 plan.txt                 │
   dusk                           │ │ user@padnoir:~/notes$ colors                                │
   paper                          │ │ red green yellow blue magenta cyan white                    │
 Pointer themes                   │ │ Bold text and italic text beside plain.                     │
   vtium's own                    │ │ ┌──────────┬─────────┐                                      │
   Adwaita                        │ │ │ plan.txt │ 1.8 KiB │                                      │
                                  │ │ └──────────┴─────────┘                                      │
                                  │ │ café naïve Ångström Straße žluťoučký                        │
                                  │ │ → ← ↑ ↓ ● ○ ✓ ✗ … ─│┌┐└┘ ▀▄█▌▐░▒▓                           │
                                  │ │                                                             │
                                  │ └─────────────────────────────────────────────────────────────┘
                                  │
                                  │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Open  [Space] Activate  [?] Search  [Ctrl+S] Save                     [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 20, `Adwaita`.*

Further down the list is Bibata-Modern-Ice, whose detail says why it can never be active: one of its files is a symbolic link that leaves the theme's folder. A resource that fails the trust rule counts as missing.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Pointer theme Bibata-Modern-Ice

   Folder    /usr/share/icons/Bibata-Modern-Ice   every file in it is active while the folder is
   State     ✗ missing: fails the trust rule
   Trust     fails: cursors/left_ptr is a symbolic link that leaves the theme's folder
   Used by   no look

 vtium treats a theme that fails the trust rule as missing.


















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Esc] Back                                                                    [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab.*

## Two questions on the way

`w` in Looks lets programs resize the grid, which changes a setting, so it asks first. The person answers `n`, and nothing changes.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Looks                            │ My-Look                                    used by console
   My-Look · 1 font not active    │
   default                        │ Font set       cullet-mono
 Font sets                        │ Size           12 pt · 11×24 px cells, 174×45 characters
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
   vtium's own         ┌ Resize the grid ─────────────────────────────────────────────────────── × ┐
   Adwaita             │ Let programs resize the grid? This sets allow-window-ops = on: a program  │
                       │ can then set the console's rows and columns, and the font size follows.   │
                       └───────────────────────────────────────────────────────────────────────────┘
                                  │ │ café naïve Ångström Straße žluťoučký                        │
                                  │ │ → ← ↑ ↓ ● ○ ✓ ✗ … ─│┌┐└┘ ▀▄█▌▐░▒▓                           │
                                  │ └─────────────────────────────────────────────────────────────┘
                                  │ A size applies when vtium starts again.
                                  │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [y] Let them  [n] No
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 4, `My-Look · 1 font not active`.*

`r` on a look deletes it, so it asks first too, and names the terminals that would move to the default look. The person presses Esc.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help                padnoir · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Looks                            │ My-Look                                    used by console
   My-Look · 1 font not active    │
   default                        │ Font set       cullet-mono
 Font sets                        │ Size           12 pt · 11×24 px cells, 174×45 characters
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
   vtium's own            ┌ Delete look ──────────────────────────────────────────────────────── × ┐
   Adwaita                │ Delete the look My-Look? The terminal console then takes the default   │
                          │ look.                                                                  │
                          │ It is one change: [Ctrl+Z] takes it back, and Save makes it real.      │
                          └────────────────────────────────────────────────────────────────────────┘
                                  │ │ → ← ↑ ↓ ● ○ ✓ ✗ … ─│┌┐└┘ ▀▄█▌▐░▒▓                           │
                                  │ └─────────────────────────────────────────────────────────────┘
                                  │ A size applies when vtium starts again.
                                  │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [y] Delete  [Esc] Cancel
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 4, `My-Look · 1 font not active`.*

## Removing a font set

The font set terminus is not used by any look. `r` on it marks it for removal at the next save, and `r` again takes the mark back. The mark is one change.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 1 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Looks                            │ Font set terminus  removed at the next save
   My-Look · 1 font not active    │ [r] again takes the mark back
   default                        │
 Font sets                        │ Text     Terminus  ○ not active
   cullet-mono                    │ used by  no look
   jetbrains                      │
   terminus     removed           │
   unifont                        │
 Color schemes                    │
   legacy                         │ ┌ terminus, as the console draws it ──────────────────────────┐
   vga                            │ │ user@padnoir:~/notes$ ls -l                                 │
   nord                           │ │ drwxr-xr-x 2 user user 4096 Nov 27 drafts                   │
   solarized                      │ │ -rw-r--r-- 1 user user 1823 Nov 27 plan.txt                 │
   dusk                           │ │ user@padnoir:~/notes$ colors                                │
   paper                          │ │ red green yellow blue magenta cyan white                    │
 Pointer themes                   │ │ Bold text and italic text beside plain.                     │
   vtium's own                    │ │ ┌──────────┬─────────┐                                      │
   Adwaita                        │ │ │ plan.txt │ 1.8 KiB │                                      │
                                  │ │ └──────────┴─────────┘                                      │
                                  │ │ café naïve Ångström Straße žluťoučký                        │
                                  │ │ → ← ↑ ↓ ● ○ ✓ ✗ … ─│┌┐└┘ ▀▄█▌▐░▒▓                           │
                                  │ │                                                             │
                                  │ └─────────────────────────────────────────────────────────────┘
                                  │ A size applies when vtium starts again.
                                  │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Edit  [n] New  [r] Remove  [?] Search  [Ctrl+S] Save  [.] More        [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 9, `terminus`.*

## A new look

`n` among the looks makes a look and opens its card with the name being typed. The person types Evening, and Enter keeps it. Making the look is one change. The card starts with the choices shown, and the person changes two of them.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 2 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Evening_                                                             used by no terminal

   Name           Evening_
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








 Made the look new-look. Type its name, then press [Enter].
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Keep  [Esc] Cancel                                                           [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 5, `Name  Evening_`.*

Enter on Color scheme opens its list box, and the person keeps nord, a second change.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 2 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Evening                                                             used by no terminal

   Name           Evening
   Font set       cullet-mono
   Size           12 pt                                                   ◂ ▸
   Color scheme   vga
   vtlook        ┌───────────────────────────┐
   Pointer theme │ ○ legacy, from vtium.conf │
                 │ ● vga                     │
 12 pt on eDP-1: │ ○ nord                    │ers. A size applies when vtium starts again.
                 │ ○ solarized               │
 Fonts by role   │ ○ solarized-black         │                   1 font not active · [A] Activate
   Text          │ ○ dusk                    │                   ● active
   Box drawing   │ ○ paper                   │e text font has them all
   Scripts       └───────────────────────────┘                   ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it









────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Pick  [Enter] Keep  [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 8, `Color scheme  vga`, row 12, `○ nord`.*

The Pointer theme line lists only the active themes. vtium's own pictures are always there, and the person keeps them with Esc.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 3 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Evening                                                             used by no terminal

   Name           Evening
   Font set       cullet-mono
   Size           12 pt                                                   ◂ ▸
   Color scheme   nord
   vtlook         No filter
   Pointer theme  vtium's own pictures
                 ┌─────────────────────────────────────────┐
 12 pt on eDP-1: │ ● vtium's own pictures, from vtium.conf │plies when vtium starts again.
                 │ ○ Adwaita                               │
 Fonts by role   └─────────────────────────────────────────┘     1 font not active · [A] Activate
   Text           Cullet Mono Regular and Bold                   ● active
   Box drawing    vtium's own lines, unless the text font has them all
   Scripts        Symbols Nerd Font Mono                         ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it









────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Pick  [Enter] Keep  [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 10, `Pointer theme  vtium's own pictures`, row 13, `○ Adwaita`.*

The vtlook line lists the vtlooks, and its list is long enough that the box offers the `?` search in its foot. The person picks night, the administrator's vtlook, for the third change. The Flow tab gives a look to a terminal, and Evening has none yet. [Chapter 6](06-flow.md) wires the terminals.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 3 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Evening                                                             used by no terminal

   Name           Evening
   Font set       cullet-mono
   Size           12 pt                                                   ◂ ▸
   Color scheme   nord
   vtlook         No filter
   Pointer theme ┌──────────────────────────────────┐
                 │ ● No filter, the built-in vtlook │
 12 pt on eDP-1: │ ○ amber                          │size applies when vtium starts again.
                 │ ○ deuteranopia                   │
 Fonts by role   │ ○ green                          │            1 font not active · [A] Activate
   Text          │ ○ night                          │            ● active
   Box drawing   │ ○ protanopia                     │font has them all
   Scripts       └ ↓ 2 more ─[?] Search ────────────┘            ✗ missing  [Enter] says why
   Emoji          Noto Color Emoji                               ✗ missing · fails trust
   Fallback       Terminus                                       ○ not active  [A] activates it









────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Pick  [Enter] Keep  [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 9, `vtlook  No filter`, row 15, `○ night`.*

## A new font set

The Looks tab also makes font sets. A font set names a font for each role, and vtium tries the roles in row order, so each later font supplies what the ones before it lack. F1 explains font sets and each role.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 New font set

   Text font          none
   Box drawing        vtium's own lines, unless the text font has them all
   Scripts            none
   Symbols and icons  none
   Emoji              none
   Fallback           none
   Name               made from the text font
   Add this font set

 vtium tries the fonts in row order; each later font supplies what the ones before lack.














────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Choose  [F1] About font sets  [Esc] Back                              [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 5, `Text font  none`.*

Enter on a role opens the active fonts that can fill it, then Other…. The list is long, so its foot offers the search.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 New font set

   Text font          none
   Box drawing        ┌──────────────────────────────────────────────────────────────────────┐
   Scripts            │ ○ DejaVu Sans Mono    /usr/share/fonts/TTF/DejaVuSansMono.ttf        │
   Symbols and icons  │ ○ Cullet Mono         /usr/share/vtium/fonts/cullet-mono.otf         │
   Emoji              │ ○ JetBrains Mono      /usr/share/fonts/TTF/JetBrainsMono-Regular.ttf │
   Fallback           │ ○ unifont             built in                                       │
   Name               │ ○ Other…                                                             │
   Add this font set  └ [?] Search ──────────────────────────────────────────────────────────┘

 vtium tries the fonts in row order; each later font supplies what the ones before lack.














────────────────────────────────────────────────────────────────────────────────────────────────────
 [↑↓] Pick  [Enter] Keep  [Esc] Close
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 5, `Text font  none`, row 8, `○ Cullet Mono  /usr/share/vtium/fonts/cullet-mono.otf`.*

The name is made from the text font unless the person types one. The person only wanted to see the form, so Esc leaves it, and no font set is made.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 New font set

   Text font          none
   Box drawing        vtium's own lines, unless the text font has them all
   Scripts            none
   Symbols and icons  none
   Emoji              none
   Fallback           none
   Name               mono-work_
   Add this font set

 vtium tries the fonts in row order; each later font supplies what the ones before lack.













 Type a family, such as JetBrains Mono, or a font set file's name for it, such as kbd:ter-v16n.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Keep  [Esc] Cancel                                                           [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Looks tab, row 11, `Name  mono-work_`.*

## Effects

Alt+E opens Effects. A vtlook chooses filters in two layers. The color layer is built into vtium: gamma, contrast, tint, color-blind and cube, which reads a `.cube` color table. The spatial layer is shader files, which run on the GPU path only. [ADR-1304](../../architecture/render/ADR-1304-vtium-s-frame-filters-are-built-in-and-a-vtlook-file-chooses-them-and-sets-their-parameters.md) decides both layers. Enter on the filter line shows one group of the list or all three.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show

   ● All effects
   ○ vtlooks (look files)
   ○ Color maps
   ○ Shaders




















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Show these  [Esc] Back                                                [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 6, `○ vtlooks (look files)`.*

Enter on a vtlook opens its detail: its file, whether it reads, and its filters in order with their settings. A value left at its default says so.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook amber   preset                                              used by no look

   File      /usr/share/vtium/looks/amber.vtlook
   Reads     without errors

 Color layer, in order
   1  tint       color 255,176,0 · amount 1.0 · floor 0.0 (default)
   2  contrast   contrast 1.15 · brightness 0.0 (default)
 Spatial layer   none

 Requires   any depth · any size · any density · min-fps refresh · either path
 Displays   eDP-1 (1920x1080, 24 bit, 157 dpi): applies

 To change a preset, copy it to /etc/vtium/looks under the same name: vtium reads the copy.












────────────────────────────────────────────────────────────────────────────────────────────────────
 [p] Preview  [Esc] Back                                                       [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab. Planned, not built yet (ADR-1304): the Requires and Displays lines: D7 and D11's Show, slice 7.*

The administrator's green vtlook hides the preset of the same name, and it does not read. The detail gives the error whole: the line, the layer, the filter, the value and the range it needed. A terminal whose look names this vtlook draws the built-in one instead.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook green   administrator's                                   used by no look

   File      /etc/vtium/looks/green.vtlook   hides the preset of this name
   Reads     does not read: 1 error

   line 9: color filter 2 of 2, contrast: checking: contrast = 2.5, needs a number from 0 to 2

 A head that names this vtlook draws the built-in vtlook instead, and its log says where.


















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Esc] Back                                                                    [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab.*

The night vtlook that Evening uses reads the color map panel.cube. The color map's detail names the look that uses it, and through which vtlook.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Color map panel.cube                                  used by Evening through vtlook night

   File      /etc/vtium/looks/panel.cube
   State     active · trusted
   Table     LUT_3D_SIZE 33, TITLE "padnoir panel, measured 2026-11-02"





















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Deactivate  [Esc] Back                                                [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab.*

Space on a resource that a look uses asks first and names that look. The person presses Esc.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show: All effects ▾            │ Color map panel.cube
                                │
 vtlooks                        │ file     /etc/vtium/looks/panel.cube
   No filter     built in       │ state    ● active · trusted
   amber         preset         │ used by  Evening, through the vtlook night
   deuteranopia  preset         │ table    LUT_3D_SIZE 33
   green         preset, hidden │
   green         admin's, error │
   night         admin's        │
   protanopia    preset         │
   tritanopia    preset         │
   white         preset         │
 Color maps                     │
   panel.cube    ● active       │
   warm.cube     ○ not active   │
 Shaders  none installed        │
    ┌ Deactivate ───────────────────────────────────────────────────────────────────────────── × ┐
    │ Deactivate the color map panel.cube? The look Evening uses it through the vtlook night;    │
    │ once saved, Evening draws with no filter from the next login.                              │
    └────────────────────────────────────────────────────────────────────────────────────────────┘
                                │
                                │
                                │
                                │
                                │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [y] Deactivate  [Esc] Cancel
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 16, `panel.cube  ● active`.*

## Planned: the shader filters

The screens in this section are not built yet. They show the Effects tab once vtium ships its shader filters, and each one names the decision of [ADR-1304](../../architecture/render/ADR-1304-vtium-s-frame-filters-are-built-in-and-a-vtlook-file-chooses-them-and-sets-their-parameters.md) that plans it. The person's state carries through them unchanged.

Once shaders ship, the Shaders group lists vtium's own and the administrator's, where one of a name hides the other. A shader written in Ghostty's dialect is marked so, and so is an animated one. Beside the list, the highlighted shader's facts and its place in the chain show.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show: All effects ▾            │ scanlines  vtium's own · ● active · trusted · MIT
                                │
 vtlooks                        │ frame  →  scanlines  →  turn  →  screen
   No filter     built in       │
   amber         preset         │ reads   2 a pixel · reach 1 px
   deuteranopia  preset         │ help    Darkens the last row of every period rows.
   green         preset, hidden │ used by vtlook film
   green         admin's, error │
   night         admin's        │ In the list, Ghostty marks a shader in Ghostty's dialect,
   protanopia    preset         │ animated one that draws over time, and hidden vtium's own
   tritanopia    preset         │ mask, which the administrator's mask of the same name hides.
   white         preset         │
 Color maps                     │
   panel.cube    ● active       │
   warm.cube     ○ not active   │
 Shaders                        │
   scanlines     vtium's        │
   sharpen       vtium's        │
   curvature     vtium's        │
   crt           Ghostty        │
   vhs           animated       │
   mask          hidden         │
                                │
                                │
                                │

────────────────────────────────────────────────────────────────────────────────────────────────────
 [Space] Activate  [Enter] Detail  [?] Search  [Ctrl+S] Save  [Ctrl+Z] Undo    [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 19, `scanlines  vtium's`.*

A shader's detail comes from the `#pragma vtium` header of its file. It gives the help line, each parameter's kind, range and default, what the shader reads and requires, and its license. Its measured cost against the frame-rate floor follows.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Shader scanlines   vtium's own · active · trusted · MIT              used by vtlook film

   File      /usr/share/vtium/shaders/scanlines.vtshader
   Help      Darkens the last row of every period rows of the picture.

 Parameters
   period     integer  2 to 8   default 3     rows from one line to the next
   strength   number   0 to 1   default 0.3   how much each line darkens

   Reach      1 px       Reads   2 a pixel    Time  no    Feedback  no
   Requires   gpu yes
   Cost       412 fps at 1920x1080 on eDP-1 (measured over 8 frames; the floor is 60, refresh)

 Root installs a shader; vtium-config shader-check tests one as you before that.












────────────────────────────────────────────────────────────────────────────────────────────────────
 [v] View source  [Esc] Back                                                   [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab. Planned, not built yet (ADR-1304): D15 (the header), D16 (the cost against the floor), slice 4 and slice 7.*

A shader in Ghostty's dialect loads as its author wrote it. What vtium does not support is one line of a translation report, never an error, and the frame-rate floor decides whether it runs.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Shader crt   administrator's · Ghostty's dialect · active · trusted

   File      /etc/vtium/shaders/crt.glsl   a .glsl file with no header
   Reach     the whole frame       Reads   not counted: the frame-rate floor decides

 Translation report   vtium puts its prelude and a main() around the author's code
   uses   iResolution, iTime, iChannel0
   ignores   iMouse: vtium gives no mouse to a shader     iDate: no date
   compiled as desktop GLSL in a second OpenGL context

   Cost       58 fps at 1920x1080 on eDP-1, below the floor of 60: eDP-1 draws the built-in vtlook















────────────────────────────────────────────────────────────────────────────────────────────────────
 [v] View source  [Esc] Back                                                   [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab. Planned, not built yet (ADR-1304): D20 and slice 11; D16 for the floor.*

`n` among the vtlooks names a new one, which starts from the default template or as a copy of another.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 New vtlook

   Name     film_

   It starts from the default look's template: no filter, every requirement at any.
   Or from a copy of: ○ amber  ○ green  ○ white  ○ deuteranopia  ○ night




















────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Make  [Esc] Cancel                                                           [Ctrl+C] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 5, `film_`. Planned, not built yet (ADR-1304): D11 (Create) and slice 7.*

Editing a vtlook shows its two layers, each filter in order with its settings, and its requirements. Each change is checked as it is made.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook film   /etc/vtium/looks/film.vtlook                         used by look My-Look

 Spatial layer   GPU path only, before the turn
   1  scanlines   period 3 · strength 0.25
   2  sharpen     amount 0.4
 Color layer     both paths, after the turn
   1  cube        file panel.cube
   2  gamma       gamma 1.1
   3  contrast    contrast 1.1 · brightness 0.0
 Requires        min-depth 16 · min-size 1280x720 · max-size 3840x2160 · min-fps refresh

   + Add a filter














────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Settings  [a] Add  [Del] Remove  [Ctrl+S] Save  [.] More              [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 9, `1  cube  file panel.cube`. Planned, not built yet (ADR-1304): D11 (Edit and the TUI's looks view) and slice 7.*

Adding a filter lists the filters from their declarations, each with its line of help. A spatial filter lists the active shaders.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook film   /etc/vtium/looks/film.vtlook                         used by look My-Look

 Spatial layer   GPU path only, before the turn
   1  scanlines   period 3 · strength 0.25
   2  sharpe┌ Add a filter ───────────────────────────────────────────────────────────────── × ┐
 Color layer│ Color layer  built into vtium                                                    │
   1  cube  │   gamma         Raises each channel to the power 1 / gamma.                      │
   2  gamma │   contrast      Scales each channel's distance from middle gray, then shifts it. │
   3  contra│   tint          Turns the screen toward one color, as a single-color phosphor.   │
 Requires   │   color-blind   Simulates a color vision deficiency, or corrects for it.         │
            │   cube          Maps each color through a .cube table.                           │
   + Add a f│ Spatial layer  shader files, GPU path only                                       │
            │   scanlines     Darkens the last row of every period rows of the picture.        │
            │   sharpen       Sharpens edges against the four nearest pixels.                  │
            │   curvature     Bends the picture as a curved tube's glass does.                 │
            │   crt           Ghostty's dialect, from /etc/vtium/shaders.                      │
            └──────────────────────────────────────────────────────────────────────────────────┘









────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Add  [Esc] Cancel
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 9, `gamma  Raises each channel to the power 1 / gamma.`. Planned, not built yet (ADR-1304): D1, D8, D11 and slice 7; curvature is slice 5, crt slice 11.*

A number shows its value with ◂ ▸, its kind, range and default, and its help below. ◂ and ▸ step within the range.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook film › spatial 1, scanlines

   period      3                        ◂ ▸   integer, 2 to 8 · default 3
   strength    0.25                     ◂ ▸   number, 0 to 1 · default 0.3

 period: rows from one line to the next.




















────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Step  [Enter] Type  [Del] Default  [Esc] Back                            [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 5, `period  3  ◂ ▸  integer, 2 to 8 · default 3`. Planned, not built yet (ADR-1304): D1 (the control fit for each kind) and slice 7.*

A color shows its swatch and is typed as `r,g,b`. The tint's floor keeps dark colors visible.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook amber-floor › color 1, tint

   color       ████ 255,176,0           [Enter] color, r,g,b
   amount      1.0                      ◂ ▸   number, 0 to 1 · default 1
   floor       0.15                     ◂ ▸   number, 0 to 0.5 · default 0

 amount: how far toward the color.



















────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Step  [Enter] Type  [Del] Default  [Esc] Back                            [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 6, `amount  1.0  ◂ ▸  number, 0 to 1 · default 1`. Planned, not built yet (ADR-1304): D1, D21 (the floor) and slice 7.*

A choice shows its words in a row, and ◂ ▸ step through them.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook deuteranopia › color 1, color-blind

   kind        ○ protanopia   ● deuteranopia   ○ tritanopia
   mode        ○ simulate     ● correct
   amount      1.0                      ◂ ▸   number, 0 to 1 · default 1

 mode: simulate shows what a person with the deficiency sees; correct shifts colors apart.



















────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Choose  [Esc] Back                                                       [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 6, `mode  ○ simulate  ● correct`. Planned, not built yet (ADR-1304): D1 and slice 7.*

Alt+↑ and Alt+↓ move a filter within its layer, since the order changes the result. A filter never moves to the other layer.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook film   /etc/vtium/looks/film.vtlook                         used by look My-Look

 Spatial layer   GPU path only, before the turn
   1  scanlines   period 3 · strength 0.25
   2  sharpen     amount 0.4
 Color layer     both paths, after the turn
    1  gamma       gamma 1.1.cube
    2  cube        file panel.cube
   3  contrast    contrast 1.1 · brightness 0.0
 Requires        min-depth 16 · min-size 1280x720 · max-size 3840x2160 · min-fps refresh

   + Add a filter













 Moved gamma up: the color layer now runs gamma, then cube, then contrast.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Settings  [a] Add  [Del] Remove  [Ctrl+S] Save  [.] More              [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab. Planned, not built yet (ADR-1304): D11 (vtlook-move) and slice 7.*

Every requirement key shows with the values it accepts, and whether each display of the machine meets it.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook film › requires

   min-depth   16          ◂ ▸   any, 1, 8, 16 or 24
   max-depth   any         ◂ ▸
   min-size    1280x720    ◂ ▸   any or WxH
   max-size    3840x2160   ◂ ▸
   min-dpi     any         ◂ ▸   any or 1 to 1000
   max-dpi     any         ◂ ▸
   min-fps     refresh     ◂ ▸   any, refresh, or 1 to 1000; look-min-fps applies too
   gpu         any         ◂ ▸   any, yes (GPU path only) or no (CPU path only)

 A display that misses one draws the built-in vtlook; the head's other displays keep this one.
 eDP-1 (1920x1080, 24 bit, 157 dpi): meets every requirement.













────────────────────────────────────────────────────────────────────────────────────────────────────
 [◂▸] Step  [Enter] Type  [Esc] Back                                           [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 7, `min-size  1280x720  ◂ ▸  any or WxH`. Planned, not built yet (ADR-1304): D7 and D11 (vtlook-require), slice 7.*

An edit that the reader refuses shows the report in place. It stays out of the document and out of the preview.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook film   /etc/vtium/looks/film.vtlook                         used by look My-Look

 Spatial layer   GPU path only, before the turn
    1  scanlines   period 12 · strength 0.25
   2  sharpen     amount 0.4
 Color layer     both paths, after the turn
   1  cube        file panel.cube
   2  gamma       gamma 1.1
   3  contrast    contrast 1.1 · brightness 0.0
 Requires        min-depth 16 · min-size 1280x720 · max-size 3840x2160 · min-fps refresh

   + Add a filter













 scanlines: period = 12 needs an integer from 2 to 8. Not kept, not previewed.
────────────────────────────────────────────────────────────────────────────────────────────────────
 [Enter] Settings  [a] Add  [Del] Remove  [Ctrl+S] Save  [.] More              [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab. Planned, not built yet (ADR-1304): D6 and D11, slice 7.*

At a vtium console, `p` sends the vtlook to the console, which answers ok or the first error, with each display's measured rate against its floor. Like Try live, the preview is never a change, and `allow-look-ops` turns it off.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook film   /etc/vtium/looks/film.vtlook                         used by look My-Look

 Spatial layer   GPU path only, before the turn
   1  scanlines   period 3 · strength 0.25
   2  sharpen     amount 0.4
 Color layer     both paths, after the turn
   1  cube        file panel.cube
   2  gamma       gamma 1.1
   3  contrast    contrast 1.1 · brightness 0.0
 Requires        min-depth 16 · min-size 1280x720 · max-size 3840x2160 · min-fps refresh

   + Add a filter

          ┌ Preview on this console ────────────────────────────────────────────────── × ┐
          │ The console shows film now, through vtium's preview sequence.                │
          │ eDP-1: ok · measured 186 fps at 1920x1080, floor 60 (refresh)                │
          │ [Esc], quitting, the session's end or a terminal reset puts the look back.   │
          └──────────────────────────────────────────────────────────────────────────────┘








────────────────────────────────────────────────────────────────────────────────────────────────────
 [Esc] End preview
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab. Planned, not built yet (ADR-1304): D11 (the preview sequence, allow-look-ops), D16, slice 7.*

Each display stands where its frame rate puts it. One that missed the floor or a requirement draws the built-in vtlook, with the reason in the log's words.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 vtlook film   /etc/vtium/looks/film.vtlook                         used by look My-Look

   Reads      without errors
 Displays
   eDP-1      1920x1080, 24 bit: applies
   HDMI-A-1   3840x2160, 24 bit: draws the built-in vtlook: measuring: 41 fps, needs 60 (refresh)
   DP-2       1280x800, 1 bit: draws the built-in vtlook: requires min-depth 16

 A display that misses a requirement draws the built-in vtlook alone; the others keep the look.
 The fallback lasts until vtium reads its configuration again or the head's look changes.
















────────────────────────────────────────────────────────────────────────────────────────────────────
 [p] Preview  [Esc] Back                                                       [F10] Menu  [q] Quit
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab. Planned, not built yet (ADR-1304): D6, D7, D16 and D11's Show, slices 6 and 7.*

Removing a vtlook keeps a `.bak` copy. It refuses default, a preset and a vtlook that a look names.

```text
 File ▾  Actions ▾ │ Fonts  Effects  Looks  Flow  Settings  Help  padnoir ● 4 not saved · 4 problems
────────────────────────────────────────────────────────────────────────────────────────────────────
 Show: All effects ▾            │ night  administrator's · used by no look
                                │
 vtlooks                        │ frame  →  spatial: none  →  turn  →  gamma  →  cube  →  screen
   No filter     built in       │
   amber         preset         │ gamma      gamma 0.9
   deuteranopia  preset         │ cube       file panel.cube
   green         preset, hidden │
   green         admin's, error │
   night┌ Remove vtlook ──────────────────────────────────────────────────────────────── × ┐
   prota│ Remove night? vtium keeps it as /etc/vtium/looks/night.vtlook.bak.               │
   trita│ A vtlook that a look names cannot be removed: change the look first.             │
   white└──────────────────────────────────────────────────────────────────────────────────┘
 Color maps                     │
   panel.cube    ● active       │
   warm.cube     ○ not active   │
 Shaders  none installed        │
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
 [y] Remove  [Esc] Cancel
```

*Marks: Bold and underlined, the top row's Alt letters F, A, n, E, L, o, S, H. Highlighted: the Effects tab, row 11, `night`. Planned, not built yet (ADR-1304): D11 (Remove), slice 7.*

## Saving the evening look

The person saves the four changes with Ctrl+S, through the review and sudo, as in [chapter 3](03-saving.md). The save replaces the file of 10:20, which History now keeps.

Where it leaves you: the save of 2026-11-28 at 11:00 holds the look Evening, with nord and the vtlook night, and no longer holds the font set terminus: `padnoir · 4 problems`. No terminal uses Evening yet. History lists three versions, of 2026-11-28 10:20, 2026-11-28 10:05 and 2026-11-12 08:53.
