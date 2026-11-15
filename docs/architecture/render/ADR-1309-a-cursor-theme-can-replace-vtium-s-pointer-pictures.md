---
contract: adr/v1
kind: decision
verb: add
capability: renderer
extends:
  - ADR-1303
basis:
  - operator: aaronsb
    level: directed
    said: "A key such as `pointer-theme=Adwaita` reads the standard cursor files from `/usr/share/icons/<theme>/cursors/`. Those files use the names OSC 22 uses (text, pointer, wait, ns-resize and so on), and each file carries its own hotspot and several sizes. The format is plain pixels, so the minspec builds read it without the image helper. vtium gets a small file parser in the console, under the same ownership check as font sets. Themes come at fixed sizes, so vtium picks the size nearest the cell height unless an optional `pointer-size=` names one. A shape the theme lacks falls back to vtium's own picture."
    via: "the steering session's question on 2026-11-15, which offered three options: a cursor theme, a PNG file for each shape, or both; the maintainer chose the cursor theme. The wording is the steering session's"
    paraphrase: true
  - standard: "the freedesktop Icon Theme Specification, version 0.13 of 2 July 2013 (specifications.freedesktop.org/icon-theme/latest/), read 2026-11-15: \"By default, apps should look in $HOME/.icons (for backwards compatibility), in $XDG_DATA_DIRS/icons and in /usr/share/pixmaps (in that order).\" \"Each theme is stored as subdirectories of the base directories.\" \"The internal name of the theme is the name of the subdirectory\". \"theme names are case sensitive, and are limited to ASCII characters. Theme names may also not contain comma or space.\" index.theme's Inherits key: \"The name of the theme that this theme inherits from.\", not required, of type strings"
  - standard: "the XDG Base Directory Specification: when $XDG_DATA_DIRS is empty or unset, it defaults to /usr/local/share/:/usr/share/"
  - standard: "the freedesktop cursor conventions specification, a draft (www.freedesktop.org/wiki/Specifications/cursor-spec/, page last edited 7 May 2021), read 2026-11-15 through the Internet Archive because the site refused the agent's client: its cursor list names default, text, pointer, help, progress, wait, copy, alias, no-drop, not-allowed, all-scroll (\"Scroll/move cursor ... Often rendered as a combined vertical and horizontal twin-headed arrow\"), row-resize, col-resize and the eight edge resizes; its list of nice-to-have names adds vertical-text, crosshair and cell; its list up for discussion adds ew-resize, ns-resize, nesw-resize and nwse-resize. It names no move cursor"
  - standard: "Xcursor(3), libXcursor's man page, X Version 11, read 2026-11-15 as installed on the agent's machine: a file is a header (magic \"Xcur\", the header's length, a file version, the number of table of contents entries) and a table of contents whose entries give each chunk's type, subtype and absolute position; an image chunk has a 36-byte header with type 0xfffd0002, the nominal size as its subtype, version 1, width and height each at most 0x7fff, xhot at most the width and yhot at most the height, the delay between animation frames in milliseconds, then width × height packed ARGB pixels; a comment chunk has type 0xfffe0001; images of one nominal size are an animated sequence; the theme search follows index.theme's Inherits= and uses the first cursor file it finds. The XcursorImage structure's comment says the hotspot \"must be inside image\". The page states neither the byte order nor whether the pixels are premultiplied"
  - evidence: "the cursor files on the agent's machine, read 2026-11-15 with a scratch reader written from the man page: adwaita-cursors 51.0 (CC-BY-SA-3.0 OR LGPL-3.0-only) and Breeze. Every number is stored least significant byte first, and every pixel's red, green and blue are at most its alpha, so the pixels are premultiplied. Adwaita's still cursors are 78,208 bytes with nominal sizes 24, 30, 36, 48, 72 and 96, each image as many pixels a side as its nominal size. Its wait and progress files are 4,691,536 bytes, with 60 frames of each size at a 16 ms delay. Breeze's images are a third larger than their nominal size: nominal 12 to 72 in steps of 6, 16 to 96 pixels a side; its wait has 23 frames a size. Adwaita's move links to default, the arrow, and its hand1 to grab, an open hand; Breeze's move links to dnd-move. Oxygen and KDE Classic have no default, crosshair, ew-resize or ns-resize file, and do have left_ptr, cross, sb_h_double_arrow and sb_v_double_arrow. /usr/share/icons/default/index.theme reads Inherits=Adwaita"
  - evidence: "vtium as built, read on integration at 1f3849c8: `crates/vtium-render/src/arrow.rs` draws each of the ten OSC 22 shapes as polygons scaled to the cell's height, white with an outline of black at 220 of 255, as premultiplied ARGB8888, and one `Arrow` serves the cursor plane and the software pointer; `Arrow::turned` turns the picture and its hotspot with the display; `POINTER_SIDE_MAX` in `crates/vtium-render/src/bounds.rs` and `CURSOR_MAX` in `crates/vtium-platform/src/display/drm2d/output.rs` are both 64, and a test holds them equal; `setup_cursor` refuses an image larger than the plane, and the pointer is then drawn in software; `crates/vtium-term/src/terminal/pointer.rs` names the shapes and takes left_ptr, xterm, hand1, hand2, watch, fleur, sb_h_double_arrow and sb_v_double_arrow; `pointer-char` draws a glyph for the default shape only; vtium-term keeps and answers OSC 13 and 14, and no drawing reads them; `crates/vtium/src/font.rs`, `frit_text`, reads a frit through `vtium_settings::inspect`, whose walk checks every folder it crosses and follows at most a bounded number of symlinks, and which reads at most 64 KiB"
  - precedent: ADR-1503
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-15
deciders:
  - aaronsb
related:
  - ADR-1000
  - ADR-1303
  - ADR-1401
  - ADR-1403
  - ADR-1503
  - ADR-1505
---

# ADR-1309: A cursor theme can replace vtium's pointer pictures

## Summary

- **Decided:** A desktop cursor theme can replace vtium's pointer pictures. The vtium.conf key `pointer-theme = NAME` names the theme, and vtium looks for it in `/usr/local/share/icons` and `/usr/share/icons`, following the theme's `Inherits=` line. For each of the ten pointer shapes that OSC 22 sets, vtium tries a short list of cursor file names, the cursor specification's name first and the X names after it. vtium reads the files with a small parser of its own in every build, the minspec builds included, under the ownership check that font sets pass. It reads them once, when the console starts. It checks every number in a file against a limit, and a file that fails a check counts as missing. vtium does not scale a theme's picture. It picks the size nearest `pointer-size`, or else nearest the character cell's height. It shows the first frame of an animated cursor. A shape the theme lacks keeps vtium's own picture, and `pointer-char` still draws the default shape when it is set.
- **Trades away:** A theme's busy cursors stand still. A theme's picture keeps its own colors, so OSC 13 and 14 never recolor it. Because vtium does not scale, a theme with few sizes may draw a pointer a little larger or smaller than a cell. vtium ignores themes in home folders and the `XCURSOR_THEME` and `XCURSOR_SIZE` variables.
- **One-way?** No. The key is off by default, and nothing on disk changes. The names `pointer-theme` and `pointer-size` become costly to change once a release ships them, since people write them into vtium.conf.
- **Probes:** *Not confident (animation):* vtium shows only the first frame of a busy cursor, so Adwaita's spinning wait cursor stands still. Animating it would mean up to 60 picture changes a second while a program is busy. Is a still frame what you want? *Not confident (`pointer-char` and a theme together):* when both are set, the character draws the default pointer and the theme draws the other nine shapes. Should the theme win instead? *Confident (folders):* vtium looks only in the two system icon folders, never in a home folder. A theme installed for one user is copied to `/usr/local/share/icons` to reach the console. *Confident (no scaling):* vtium picks the nearest size a theme ships and never resamples it. *Confident (move):* the move shape takes the theme's `all-scroll` picture, because Adwaita's `move` file is the plain arrow.
- **Inversion:** The choice sat between pictures vtium draws itself, which fit any cell and any color, and pictures a person brings, which match their desktop. The maintainer chose a cursor theme over a PNG file for each shape. A theme already holds every shape, its hotspots and its sizes in one folder, in a format the minspec builds can read without the image helper.

## Context

vtium draws its own pointer. `crates/vtium-render/src/arrow.rs` draws each shape that a program sets with OSC 22 as polygons scaled to the cell's height. The shapes are an arrow, an I-beam, a hand, a crosshair, an hourglass, the arrow with a small hourglass, a circle with a slash, four arrows for a move, and two double arrows. Each picture is white with a dark outline and at most 64 pixels a side (`POINTER_SIDE_MAX`). One picture serves both ways of showing the pointer. A display with a cursor plane shows it there, and the renderer blends it over the cells on a display without one. `pointer-char` replaces the default arrow with a character of the console's font.

People who use a desktop have chosen a cursor theme for it, and they want the console's pointer to match. A cursor theme is a folder of files in the Xcursor format, one for each cursor name, under an icon folder such as `/usr/share/icons/Adwaita/cursors/`. Each file holds the picture at several nominal sizes, each with its own hotspot, and a busy cursor holds several frames of each size. The pixels are plain premultiplied ARGB, so the console can read them without the image helper, which the minspec builds lack.

The maintainer chose this on 2026-11-15 over two other options. A PNG file for each shape needs the image helper to decode, so the minspec builds could not use it, and it asks a person for ten paths and ten hotspots. Both together would be two features where one may be enough. Files for each shape can come later if someone asks.

This record decides the keys, where vtium looks for a theme, and which file serves each shape. It decides how vtium reads and checks a file, which size it picks, what it does with animated cursors and colors, and what it logs. It extends ADR-1303 only in that the GPU path draws a theme's picture upright, as it draws vtium's own.

## Decision

### D1: the keys

Two keys join the key table's "Mouse pointer" group, both at vtium.conf's top level and on the command line, as `pointer-char` is:

| Key | Values | Default | What it does |
|---|---|---|---|
| `pointer-theme` | a theme's name: 1 to 64 characters of `A-Z a-z 0-9 . _ + -`, not starting with `.` | unset | The cursor theme whose pictures the pointer takes. Unset, vtium draws its own pictures. |
| `pointer-size` | 8 to 64 pixels | unset | The nominal size of the theme's pictures. Unset, vtium picks the size nearest the character cell's height (D5). |

- **The name's characters** are those of a frit's name. The Icon Theme Specification limits a theme's name to ASCII with no comma or space, and every theme name seen on the agent's machine fits the narrower rule. The rule keeps a name from reaching outside the icon folders, since it holds no `/` and no leading `.`. `default` is a valid name. On most distributions `/usr/share/icons/default` inherits from the system's chosen theme, so `pointer-theme = default` gives the system's own cursors.
- **`pointer-size` names a nominal size,** the same number a desktop's cursor size setting uses, so a person can copy it from there. It stops at 64 because vtium never shows a picture larger than 64 pixels a side (D4). Without `pointer-theme`, `pointer-size` does nothing.
- **`pointer-char` keeps the default shape.** When both keys are set, the character draws the default shape, and the theme draws the other nine. `pointer-char` names one shape and the theme names all ten, so the narrower key wins where they overlap. Someone who sets both most likely wants their character as the everyday pointer and the theme's pictures for the hints programs set. When the font cannot draw the character, the default shape takes the theme's picture, then vtium's arrow.
- **vtium reads neither `XCURSOR_THEME` nor `XCURSOR_SIZE`.** The console starts as a system service before anyone logs in, and its environment is the service manager's, not a person's. vtium.conf is the one place its configuration lives.

### D2: where vtium looks for a theme

- **Two folders.** The Icon Theme Specification looks in `$HOME/.icons`, then in `icons` under each folder of `$XDG_DATA_DIRS`, then in `/usr/share/pixmaps`. vtium runs as root, for every user of the machine and before any of them logs in. So it looks only in the folders that serve the whole machine. These are `$XDG_DATA_DIRS`'s default, `/usr/local/share/icons` and then `/usr/share/icons`. vtium does not read `$XDG_DATA_DIRS` from its environment, for D1's reason. It does not read a home folder, because the console belongs to no single user, and the ownership check (D4) would refuse a user's files anyway. It does not read `/usr/share/pixmaps`, which holds no cursor themes.
- **A theme is a folder** of that name in either icon folder, as the specification says. For a cursor file, vtium tries `/usr/local/share/icons/NAME/cursors/FILE` and then `/usr/share/icons/NAME/cursors/FILE`. For `index.theme`, it reads the first of the two it finds.
- **Inheritance.** vtium reads the `Inherits=` key in the `[Icon Theme]` group of the theme's `index.theme`. The value is a comma-separated list of theme names, as the specification's type "strings" means. The themes vtium searches form a list: the named theme, then each inherited theme in the order its line names them, each followed by its own inherited themes, depth first. A theme already in the list is not added again, a name that fails D1's rule is skipped, and the list stops at 8 themes. `index.theme` is read through vtium-settings' checked read, the one vtium.conf passes, so it is at most 64 KiB of UTF-8 text owned as D4 says.
- **The theme comes first, then the names.** For each shape, vtium goes through the list of themes in order. In each theme it tries the shape's file names in D3's order, and the first file that passes D4's checks serves the shape. So a theme's own `left_ptr` wins over a `default` file in a theme it inherits from. The pictures a person chose stay theirs wherever the theme has one.
- **vtium adds no theme of its own to the list.** The specification has a program add `hicolor`, which holds icons and no cursors. vtium does not fall back to the `default` theme either: a shape that no theme in the list holds keeps vtium's own picture.
- **A theme that is missing** draws vtium's own pictures, with one log line (D8). A theme is missing when neither icon folder holds a folder of its name. A theme found with no usable cursor file for any shape draws vtium's pictures too, with the same line.

### D3: which file serves each shape

| OSC 22 shape | Files vtium tries, in order |
|---|---|
| `default` | `default`, `left_ptr` |
| `text` | `text`, `xterm` |
| `pointer` | `pointer`, `hand2` |
| `crosshair` | `crosshair`, `cross` |
| `wait` | `wait`, `watch` |
| `progress` | `progress`, `left_ptr_watch` |
| `not-allowed` | `not-allowed`, `crossed_circle` |
| `move` | `all-scroll`, `fleur` |
| `ew-resize` | `ew-resize`, `sb_h_double_arrow`, `col-resize` |
| `ns-resize` | `ns-resize`, `sb_v_double_arrow`, `row-resize` |

- **The cursor specification's name comes first,** and the X cursor font's name for the same shape after it. Themes written for X carry only the X names: Oxygen and KDE Classic have `left_ptr`, `cross` and `sb_h_double_arrow`, and no `default`, `crosshair` or `ew-resize`.
- **`move` takes `all-scroll`.** The cursor specification names no move cursor. Its `all-scroll` is the four-headed arrow that vtium's own move picture draws. A theme's `move` file serves dragging and is not that picture: Adwaita's `move` links to its plain arrow, and Breeze's to its drag-and-drop arrow.
- **`hand1` is left out.** vtium-term takes it as a name for the pointer shape, but Adwaita links `hand1` to `grab`, an open hand, which says something else.
- **The resize shapes end with the splitter cursors.** `col-resize` and `row-resize` are the specification's names for a bar between two panes, which is what the two resize shapes mark in a terminal. They come last because their picture adds a bar across the arrow.
- **The file names are fixed.** No text from a program reaches a path, since OSC 22 only chooses among the ten shapes. The only names in a path are the theme names, which D1's rule bounds, and this table's names.

### D4: reading a cursor file

- **Once, at start.** vtium reads the theme when the console starts and reads vtium.conf, before it draws its first frame, as it reads `pointer-char`. It never reads a cursor file while it draws. A change to `pointer-theme` takes effect when the console restarts.
- **The ownership check.** A cursor file passes the check that a font set and vtium.conf pass. The file is root's, or the user's that vtium runs as, and so is every folder crossed on the way to it. Neither the group nor other users can write any of them. Cursor files are often symlinks, such as Adwaita's `xterm` to `text`. vtium-settings' walk follows a symlink from the folder that holds it and checks every folder it crosses, as it does for a frit. vtium-settings gains a checked open that returns the opened file and the check's verdict without reading the file. A cursor file is too large for the 64 KiB read that vtium.conf takes, and vtium needs only a few parts of it.
- **Positioned reads.** vtium reads the file's header and table of contents, and then only the chunks of the images it keeps (D5), each at the position the table names. Adwaita's wait cursor is 4.7 MB, and vtium reads about 4 KB of its table and one image of each size from it.
- **What vtium reads.** Every number is a 32-bit unsigned integer, least significant byte first. The man page does not state the byte order, and every file on the agent's machine and the pinned test files use it. vtium reads the header's magic, its length and its count of table entries. In each table entry it reads the type, the subtype and the position. It ignores the file's version, which the man page gives no value for, and every chunk that is not an image, such as a comment. For each image it keeps, it reads the image header and `width × height` pixels. It ignores the delay (D6).
- **The pixels pass through.** vtium's pointer image is premultiplied ARGB8888, as a DRM cursor plane takes it and as the renderer blends it. A theme's pixels are premultiplied ARGB, so they become the picture as they are. A pixel whose red, green or blue is greater than its alpha is not premultiplied. vtium lowers each such channel to the alpha, so that blending never overflows.
- **The checks.** A file fails when any of these is false:

  | Check | Limit |
  |---|---|
  | The file's size | at most `XCURSOR_FILE_MAX`, 16 MiB, so Adwaita's 4.7 MB busy cursors pass with room to spare |
  | The magic | the bytes `Xcur` |
  | The header's length | at least 16, the four fields the man page lists. The table starts at that length |
  | The table's entries | at most `XCURSOR_TOC_MAX`, 4,096 (Adwaita's busy cursors have 360), and the whole table inside the file |
  | Each image kept | its header 36 bytes long, its type `0xfffd0002`, its subtype the table entry's, its version 1, and its pixels inside the file |
  | Each image's side | its width and height at least 1 |
  | The hotspot | inside the image: `xhot` below the width and `yhot` below the height. The man page allows `xhot` equal to the width, which is a pixel outside the picture |

- **A picture too large is no failure.** An image wider or taller than `POINTER_SIDE_MAX`, 64 pixels, does not fit the cursor plane (`CURSOR_MAX` is also 64), so vtium does not keep that size. Adwaita's sizes 72 and 96 are dropped this way, and the file still serves its other sizes. A file whose every size is too large counts as missing.
- **A failed file counts as missing.** vtium goes on to the shape's next file name and the next theme, as for a file that does not exist, and logs why it did not read the file (D8). A broken file never hides a good one further down the list.
- **The parser touches no files.** It reads through a small trait of positioned reads, so the tests and the fuzz target give it bytes in memory, and the console gives it the checked file.

### D5: the size

- **The sizes vtium keeps.** For each nominal size in a file, vtium keeps the first image of that size in the table's order, unless D4 dropped it as too large. It keeps sizes from the smallest up, while the shape's kept pixels stay within `POINTER_THEME_PIXELS_MAX`, 16,384 pixels, which is four pictures of 64 × 64. Larger sizes past that budget are not kept. Adwaita keeps 24, 30, 36 and 48, which is 5,076 pixels for each shape. Breeze keeps 12 to 48, which is 12,992 pixels. With `pointer-size` set, vtium keeps only the size it picks. So the ten shapes hold at most 640 KiB of pictures.
- **Which size draws.** Each display's renderer picks the kept size whose nominal size is nearest `pointer-size`, or else nearest its own cell's height in pixels. Of two sizes equally near, it picks the smaller. A zoom or a display with another cell size picks again from the sizes kept, with no new read. Adwaita draws its 24 pixel pictures over 16 or 19 pixel cells, and its 36 pixel pictures over 38 pixel cells.
- **The nominal size, not the picture's side.** The nominal size is the number a theme and a desktop call the cursor's size. Breeze draws its nominal 24 in 32 pixels, so its pictures come out a third larger than Adwaita's at the same setting, as they do on a desktop.
- **No scaling.** vtium draws the picture at the size the theme drew it. A theme's picture has one-pixel outlines and a hotspot placed by hand for each size, and resampling would blur the outlines and move the hotspot off its pixel. The theme's sizes sit close together, so the nearest is never far from the cell. vtium's own pictures still scale, because vtium draws them from shapes for every cell height.
- **Turning with the display.** A theme's picture turns with the display as vtium's own pictures do: `Arrow::turned` turns its pixels and its hotspot. The GPU path draws it upright into the upright frame, which ADR-1303 turns at present, as it draws vtium's own.
- **A larger plane is not assumed.** When a display's cursor plane is smaller than the picture, `setup_cursor` refuses it, and that display draws the pointer in software, as it does today.

### D6: animated cursors show their first frame

A busy cursor holds several frames of each size, with a delay between them: Adwaita's wait and progress hold 60 frames at 16 ms, and Breeze's 23. vtium shows the first frame of the chosen size and does not animate.

- **The pointer is a hint.** A still hourglass or spinner still says the program is busy, and vtium's own wait picture is still.
- **Animating costs steady work for a hint.** At Adwaita's pace, the pointer would change 60 times a second while a program shows it. On a display with a cursor plane, each change is a new plane buffer. Without one, each change redraws the cells under the pointer. On a minspec machine drawing on the CPU, that work runs for as long as the program stays busy.
- **Nothing needs a timer.** The pointer changes only when a program sets a shape or the pointer moves, as it does today, and damage tracking marks the pointer's old and new places as it does now.
- **If a later record animates.** The frame number would be state of the pointer, advanced at a deadline that the event loop keeps, as the text blink is. Each advance would mark only the pointer's area as damaged, or set a new plane buffer. This is a scheduled change of state, like the blink. It does not break vtium's rule that a bug is fixed in the state that is wrong and never by redrawing later. vtium would then read every frame of the chosen size, and D5's pixel budget would count them.

### D7: colors

A theme's picture keeps the colors its author drew. OSC 13 and 14, the pointer's foreground and background, never recolor it. A theme's picture has many colors and shades, so two colors cannot map onto it without changing what its author drew. vtium-term keeps and answers OSC 13 and 14 as it does today. This record changes nothing about how vtium's own pictures use them.

### D8: what vtium logs

Each line follows "Strings on screen" in AGENTS.md: the topic first, the reader's words, and what vtium does.

- **A missing theme,** at warning level, once: "The pointer theme Adwaita is not in /usr/local/share/icons or /usr/share/icons, so vtium draws its own pointer pictures."
- **A theme with no usable file,** at warning level, once: "The pointer theme Adwaita has no cursor file vtium can read, so vtium draws its own pointer pictures."
- **Shapes the theme lacks,** at info level, in one line: "The pointer theme Oxygen_White has no picture for the ew-resize and ns-resize shapes, so vtium draws its own pictures for them." A theme that lacks a shape is ordinary, so this line is not a warning.
- **A file vtium does not read,** at warning level, one line for each file: "vtium does not read the cursor file /usr/share/icons/Adwaita/cursors/wait for the wait shape, because the hotspot lies outside the picture." The reason names the check that failed. An ownership refusal uses the words vtium.conf's refusal uses, with the path that fails and the command that fixes it.
- **An `index.theme` vtium does not read** gets the same kind of line, and vtium searches that theme's own files without its inherited themes.

`vtium check-config` checks the two keys' values, as it checks every key from the table. It does not look the theme up, because the theme is read where the console runs.

### D9: the card and the crates it touches

One agent builds this record as one card.

- **vtium-settings:** the two keys in the key table and in its "Mouse pointer" group, with their help text. Also a checked open that returns the file and the ownership verdict without reading the file (D4). `crates/vtium/install/vtium.conf.example` and `docs/config.md` regenerate from the table.
- **vtium-render:** a module `src/xcursor.rs` that parses a file through a trait of positioned reads (D4) and keeps its sizes (D5). It adds the bounds `XCURSOR_FILE_MAX`, `XCURSOR_TOC_MAX` and `POINTER_THEME_PIXELS_MAX` to `src/bounds.rs`, with their rows in the README's table. It also adds a constructor for an `Arrow` from a theme's picture, and a theme table each renderer picks its size from. It adds a new fuzz directory, `crates/vtium-render/fuzz`, with the target `xcursor`.
- **vtium:** a module `src/pointer_theme.rs` that finds the theme and its inherited themes, reads `index.theme`, opens each cursor file through the checked open, and logs D8's lines. It hands the theme table to each renderer. The precedence of `pointer-char` (D1) goes where `pointer-char` is applied today.
- **tools/tier2:** the pinned Adwaita files and one screen for each shape (Verification).
- **tools/vt/checks:** the guide `pointer-theme.guide` (Verification).
- **ADR-1000:** the note of the `window.pointer-shape` row gains a sentence on the theme. The row's status does not change.

vtium-term does not change, because OSC 22, its names and its stacks stay as they are.

## Verification

The card runs at V1 (ADR-1505): its new tests and screens, each seen failing first, `make tier2` once, and the fuzz target for 60 s.

- **The parser's unit tests** in vtium-render build files in memory. Each check of D4 has a file that passes at its limit and one that fails one past it. The checks are the file size, the magic, the header's length, the table's entries, and the table past the file's end. They go on with an image header's length, type, subtype and version, and a width or height of 0. A picture past 64 a side drops that size alone. A hotspot on the picture's last pixel passes, and one past it fails. A file cut short inside the table, inside an image header and inside the pixels fails each time. A comment chunk is ignored. Two images of one size keep the first in the table's order. A pixel brighter than its alpha is lowered to it. The kept sizes stop at the pixel budget.
- **The size choice:** the nearest nominal size to `pointer-size` and to a cell height, the smaller of two equally near, and a new pick after a zoom with no new read.
- **The lookup's tests** in vtium build theme folders under a temporary root that the tests own. They cover D3's order for each shape and the theme-first order of D2, so a theme's `left_ptr` beats an inherited `default`. They cover `Inherits=` with two names, a loop of themes, a chain cut at 8 themes, a symlinked cursor file, and a theme in both icon folders. A file in a folder that group or other users can write fails the ownership check. A missing theme, a theme with no usable file, a lacking shape and a refused file each log D8's line. `pointer-char` draws the default shape with a theme set, and the theme draws the default shape when the font cannot draw the character.
- **Tier 2:** ten screens, `pointer-theme-default` to `pointer-theme-ns-resize`, on the same 12 × 3 cell screen that `pointer-default` uses. Each draws one shape from Adwaita 51.0 at nominal size 24. An eleventh, `pointer-theme-turned`, draws the default shape on a display turned right. The files are fetched from GNOME's repository at the tag `51.0`, under ADR-1503 D9's rule for pinned test inputs: each file by its URL and SHA-256, into tier 2's cache, never committed. Adwaita's license, CC-BY-SA-3.0 or LGPL-3.0-only, lets vtium's tools fetch and use them. The URLs are `https://gitlab.gnome.org/GNOME/adwaita-icon-theme/-/raw/51.0/Adwaita/cursors/FILE`:

  | File | SHA-256 |
  |---|---|
  | `default` | `47f579498d72b12d828aa40e7e484c5ebd6b75ae04293cf5c96532c21c3e1594` |
  | `text` | `7ec54ff5b92fdbe65ced307272e839f4ae9a32ab4ea0f139daac781aa44bf05d` |
  | `pointer` | `3f6fc5b501fd3a27435660805264e25ac39210008baa325003ad1ca9d73266dc` |
  | `crosshair` | `c73e992d8482def0d093c11a78a9074e62def2c3db822c84bf3c444c63761a46` |
  | `wait` | `16077ef503db098a1e8ea44173256367ad8d89138157b2cc1ac025dedc06ad1d` |
  | `progress` | `db648cdb0b79532708efb6869cfbee359bac36d959a6bb2b82a8604a3761704a` |
  | `not-allowed` | `9c5ede64eb64f2d0d1c8c73a9e1b98bc10bf95f183edb19ef00239675b918246` |
  | `all-scroll` | `da6d0c16cd35578876c98cf3633f8969a4436d5b821532813d4c19f30e291544` |
  | `ew-resize` | `c09fb1b77fec8700a28a9f0fa7eb471d5817002a30860c028df3345ac8e30098` |
  | `ns-resize` | `8371a4fb816b9e2109e83d84c250d5e07c0ef87e8293176dd8a7822a8bef83bd` |

  The agent fetched each URL on 2026-11-15, and each file's SHA-256 matched the file that the distribution's adwaita-cursors 51.0 package installs. The screens are provisional until the maintainer reviews them at 1:1.
- **The fuzz target** `xcursor` feeds any bytes to the parser as a file, with a size and a cell height taken from the input. It asserts that the parser never panics, and that each picture it returns is at most 64 a side with its hotspot inside. The run's rows go in `crates/vtium-render/fuzz/runs.tsv`.
- **The VT guide** `tools/vt/checks/pointer-theme.guide` runs the console with `pointer-theme = Adwaita` in a configuration of its own, installed the way the restart guide installs one. It walks the ten shapes as `pointer.guide` does, with one step at `pointer-size = 48` and one with a theme name that does not exist. The maintainer judges that each picture is Adwaita's, that its hotspot is where it points, and that the missing theme draws vtium's arrow.
- The tests of the theme's drawing carry `covers: window.pointer-shape`.

## Consequences

### Positive

- The console's pointer can match the person's desktop with one line of vtium.conf, on every build, the minspec builds included.
- A theme brings a picture for every shape at sizes its author drew by hand, with hotspots placed for each size.
- vtium reads a theme once at start, so drawing the pointer costs what it costs today.
- The parser reads only the parts of a file it keeps, so a large animated file costs a few kilobytes of reading.

### Negative

- vtium carries a parser for a binary format that it reads as root. The checks of D4, the fuzz target and the ownership check bound what a bad file can do, and a file vtium refuses only costs that shape its theme picture.
- A theme's busy cursors stand still.
- A theme's picture ignores OSC 13 and 14.
- A theme installed for one user does not reach the console until it is copied to a system icon folder.

### Neutral

- Without `pointer-theme`, nothing changes: vtium draws its own pictures, and `pointer-char` works as it does today.
- The ten shapes and the names programs use for them do not change.

## Alternatives Considered

- **A PNG file for each shape.** Not taken: the maintainer declined it. It needs the image helper, so the minspec builds cannot use it, and a person writes ten paths and ten hotspots.
- **Both a theme and a file for each shape.** Not taken: the maintainer declined it as two features where one may be enough. Files for each shape can come later if someone asks.
- **Scale the nearest size to the cell's height.** Not taken: resampling blurs a theme's one-pixel outlines and moves its hotspot, and the theme's own sizes are already close together.
- **Pick by the picture's side instead of its nominal size.** Not taken: `pointer-size` would then mean something other than a desktop's cursor size, and a theme drawn larger than its nominal size, as Breeze is, would look different on the console than on the desktop.
- **Animate busy cursors.** Not taken in this draft, for D6's reasons. It is the first probe.
- **Let the theme win over `pointer-char`.** Not taken in this draft: `pointer-char` names the default shape alone, so it is the more specific choice. It is the second probe.
- **Follow only the first inherited theme,** as the Xcursor library does. Not taken: the specification types `Inherits=` as a list of strings, and following each one in order costs nothing once the list is bounded.
- **Search home folders and the environment,** as a desktop program does. Not taken: the console runs as root for every user, before anyone logs in, and vtium.conf is the one place its configuration lives.
- **Take a theme's `move` file for the move shape.** Not taken: the cursor specification names no move cursor, and Adwaita's `move` file is the plain arrow, so programs that set move would show no change.
