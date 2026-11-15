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
  - standard: "the freedesktop cursor conventions specification, a draft (www.freedesktop.org/wiki/Specifications/cursor-spec/, page last edited 7 May 2021), read 2026-11-15 through the Internet Archive because the site refused the agent's client: its cursor list names default, text, pointer, help, progress, wait, copy, alias, no-drop, not-allowed, all-scroll (\"Scroll/move cursor ... Often rendered as a combined vertical and horizontal twin-headed arrow\"), row-resize, col-resize and the eight edge resizes; its list of nice-to-have names adds vertical-text, crosshair and cell; its list up for discussion adds ew-resize, ns-resize, nesw-resize and nwse-resize. It names no move cursor"
  - standard: "Xcursor(3), libXcursor's man page, X Version 11, read 2026-11-15 as installed on the agent's machine: a file is a header (magic \"Xcur\", the header's length, a file version, the number of table of contents entries) and a table of contents whose entries give each chunk's type, subtype and absolute position; an image chunk has a 36-byte header with type 0xfffd0002, the nominal size as its subtype, version 1, width and height each at most 0x7fff, xhot at most the width and yhot at most the height, the delay between animation frames in milliseconds, then width × height packed ARGB pixels; a comment chunk has type 0xfffe0001; images of one nominal size are an animated sequence; the theme search follows index.theme's Inherits= and uses the first cursor file it finds. The XcursorImage structure's comment says the hotspot \"must be inside image\". The page states neither the byte order nor whether the pixels are premultiplied"
  - evidence: "the cursor files on the agent's machine, read 2026-11-15 with a scratch reader written from the man page: adwaita-cursors 51.0 (CC-BY-SA-3.0 OR LGPL-3.0-only) and Breeze. Every number is stored least significant byte first, and every pixel's red, green and blue are at most its alpha, so the pixels are premultiplied. Adwaita's still cursors are 78,208 bytes with nominal sizes 24, 30, 36, 48, 72 and 96, each image as many pixels a side as its nominal size. Its wait and progress files are 4,691,536 bytes, with 60 frames of each size at a 16 ms delay. Breeze's images are a third larger than their nominal size: nominal 12 to 72 in steps of 6, 16 to 96 pixels a side; its wait has 23 frames a size. Adwaita's move links to default, the arrow, and its hand1 to grab, an open hand; Breeze's move links to dnd-move. Oxygen and KDE Classic have no default, crosshair, ew-resize or ns-resize file, and do have left_ptr, cross, sb_h_double_arrow and sb_v_double_arrow. /usr/share/icons/default/index.theme reads Inherits=Adwaita"
  - evidence: "vtium as built, read on integration at 1f3849c8: `crates/vtium-render/src/arrow.rs` draws each of the ten OSC 22 shapes as polygons scaled to the cell's height, white with an outline of black at 220 of 255, as premultiplied ARGB8888, and one `Arrow` serves the cursor plane and the software pointer; `Arrow::turned` turns the picture and its hotspot with the display; `POINTER_SIDE_MAX` in `crates/vtium-render/src/bounds.rs` and `CURSOR_MAX` in `crates/vtium-platform/src/display/drm2d/output.rs` are both 64, and a test holds them equal; `setup_cursor` refuses an image larger than the plane, and the pointer is then drawn in software; `crates/vtium-term/src/terminal/pointer.rs` names the shapes and takes left_ptr, xterm, hand1, hand2, watch, fleur, sb_h_double_arrow and sb_v_double_arrow; `pointer-char` draws a glyph for the default shape only; vtium-term keeps and answers OSC 13 and 14, and no drawing reads them; `crates/vtium/src/font.rs`, `frit_text`, reads a frit through `vtium_settings::inspect`, whose walk checks every folder it crosses and follows at most a bounded number of symlinks, and which reads at most 64 KiB"
  - standard: "SVG 2 (W3C), as the agent knows it, not re-read for this record: the symbol element and its viewBox, drawn only through a use element that references it; use's width and height size a symbol's viewBox; the shape-rendering property's crispEdges value; every element may carry custom data- attributes, which renderers ignore"
  - evidence: "resvg 0.50.0, measured by the agent on 2026-11-15 in a scratch crate built with the workspace's release settings (lto, one codegen unit, panic abort): with its default features off, so with no text and no raster images, it adds 905,080 bytes to a stripped binary (296,992 bytes without it, 1,202,072 with it). Its dependency tree's licenses: resvg and usvg Apache-2.0 OR MIT; tiny-skia and tiny-skia-path 0.12.0 BSD-3-Clause; arrayref 0.3.9 BSD-2-Clause; the rest MIT, Apache-2.0, Zlib or a choice that includes one of them. deny.toml allows Apache-2.0, ISC, MIT, NCSA, Unicode-3.0, Zlib and bzip2-1.0.6. The same crate drew a symbol that fills with a gradient from the file's defs, through a use element appended to the file, at 7 × 16 and 12 × 16 pixels"
  - evidence: "the font helper as built, read on integration at 1f3849c8: `crates/vtium-config/src/serve.rs` reads requests of kinds 1 to 8 after the magic VTVSREQA and answers in frames of kinds 1 to 15, little-endian; the console checks every frame's length against FRAME_BYTES_MAX, 16 MiB (`crates/vtium/src/helper.rs`), before it allocates; serve sets the parser's sandbox on itself and parses in its own process (`crates/vtium-config/src/child.rs`), runs as nobody when the console is root, and installs a landlock domain and seccomp filters (`crates/vtium-config/src/seccomp.rs`); `make install-tree` installs vtium-config beside the minspec console, and `make install-build` fails unless the minspec console names the helper's path (Makefile); tier 3's size budget, 4 MiB, judges the minspec console binary alone (`tools/tier3/t3_impl.py`, ADR-1200 D19), so the helper has no size budget"
  - precedent: ADR-1503
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "First frame only [vtium shows the first frame of an animated wait or progress cursor]"
    via: the steering session, 2026-11-15, the question tool
    covers: [animation]
  - operator: aaronsb
    said: "pointer-char wins [when both are set, the character draws the default shape and the theme draws the other nine]"
    via: the steering session, 2026-11-15, the question tool
    covers: ['`pointer-char` and a theme together']
  - operator: aaronsb
    said: "I think it's also worthwhile to offer our vector style pointers as a file (can it be svg?)"
    via: the steering session, 2026-11-15, the question tool
  - operator: aaronsb
    said: "perhaps it can be rendered in a sandboxed helper just as glyphs are, and supplied as a bitmap always"
    via: the steering session, 2026-11-15, the question tool
  - operator: aaronsb
    said: "One key for both [the note:] it feels like adding a theme is ambiguous. (sort of like how fonts you just...name them and they show up?) I'd rather be able to point to a a specific pointer theme file too. or am I missing something?"
    via: the steering session, 2026-11-15, the question tool; the steering session answered that a desktop theme is a folder of files and a path removes the ambiguity
  - operator: aaronsb
    said: "I guess I feel like just naming a theme with no specific path is unsafe because you don't know exactly how that is selectiing it"
    via: the steering session, 2026-11-15
  - operator: aaronsb
    said: "Allow both BSDs [deny.toml allows BSD-2-Clause and BSD-3-Clause, and the package ships tiny-skia's notice]"
    via: the steering session, 2026-11-15, the question tool
    covers: [BSD licenses]
  - operator: aaronsb
    said: "data-hotspot, required"
    via: the steering session, 2026-11-15, the question tool
    covers: [hotspot]
status: proposed
date: 2026-11-15
deciders:
  - aaronsb
related:
  - ADR-1000
  - ADR-1200
  - ADR-1301
  - ADR-1303
  - ADR-1401
  - ADR-1403
  - ADR-1503
  - ADR-1505
---

# ADR-1309: An SVG file or a cursor theme can replace vtium's pointer pictures, and the font helper draws them

## Summary

- **Decided:** The vtium.conf key `pointer-theme = PATH` names a file of pointer pictures, an absolute path. PATH is either one SVG file or a desktop cursor theme's folder, and vtium tells them apart by what is at the path. An SVG file holds each shape as a `<symbol>` whose `id` is the OSC 22 shape's name, with its hotspot in a `data-hotspot` attribute. The console never parses a pointer file. It reads the file under the ownership check that font sets pass, and sends the bytes to the font helper. The helper draws each shape in its sandbox and answers with pixels and a hotspot, which the console checks as it checks glyphs. The helper draws SVG with resvg and reads cursor files with a parser of its own. vtium's own shapes ship as `/usr/share/vtium/pointers.svg`, generated from the shapes compiled into the console. The compiled-in shapes still draw before the helper answers and when no file is set. A shape the file lacks keeps vtium's own picture, and `pointer-char` keeps the default shape.
- **Trades away:** The helper grows by about 0.9 MB with resvg. resvg's rasterizer, tiny-skia, is under the BSD-3-Clause license, which deny.toml does not allow yet. A cursor theme's busy cursors stand still, and its pictures keep their own colors. vtium ignores a theme's `Inherits=` line, so a person names the folder that holds the cursors.
- **One-way?** No. The key is off by default, and nothing on disk changes. The key's name and the SVG file's layout become costly to change once a release ships them, since people write files to that layout.
- **Probes:** *Not confident (animation):* vtium shows only the first frame of a cursor theme's busy cursor, so Adwaita's spinning wait cursor stands still. Is a still frame what you want? *Not confident (`pointer-char` and a theme together):* when both are set, the character draws the default pointer and the file draws the other nine shapes. Should the file win instead? *Not confident (BSD licenses):* resvg draws through tiny-skia, under BSD-3-Clause, and needs arrayref, under BSD-2-Clause. vtium's license list allows neither. Shall vtium allow both, and ship tiny-skia's notice? *Not confident (hotspot):* an SVG shape marks its hotspot as `data-hotspot="x y"` in its viewBox's units, and a shape without one is skipped. Is that the form you want? *Confident (no scaling of a cursor theme):* vtium picks the nearest size a theme ships and never resamples it.
- **Inversion:** The choice sat between a console that reads pointer files itself and a console that reads none. The maintainer chose to have the sandboxed helper draw them, as it draws glyphs, so the console only ever takes pixels it can check. A path replaces a name search, which the maintainer found unsafe because no one can see which file it selects.

## Context

vtium draws its own pointer. `crates/vtium-render/src/arrow.rs` draws each shape that a program sets with OSC 22 as polygons scaled to the cell's height. The shapes are an arrow, an I-beam, a hand, a crosshair, an hourglass, the arrow with a small hourglass, and a circle with a slash. Four arrows mark a move, and two double arrows mark the resizes. Each picture is white with a dark outline, at most 64 pixels a side (`POINTER_SIDE_MAX`). One picture serves both ways of showing the pointer. A display with a cursor plane shows it there, and the renderer blends it over the cells on a display without one. `pointer-char` replaces the default arrow with a character of the console's font.

People who use a desktop have chosen a cursor theme, and they want the console's pointer to match. A cursor theme is a folder, such as `/usr/share/icons/Adwaita`, whose `cursors/` folder holds one Xcursor file for each cursor name. Each file holds the picture at several nominal sizes, each with its own hotspot, and a busy cursor holds several frames of each size. Others want to draw their own pointers, or adjust vtium's, in an SVG editor.

The maintainer first chose a cursor theme named by its name, over a PNG file for each shape. A PNG file needs the image helper, which the minspec builds lack, and asks for ten paths and ten hotspots. The maintainer then asked for three more things. vtium's own vector pointers should be offered as an SVG file. The pictures should be drawn in a sandboxed helper, as glyphs are, and reach the console as a bitmap. And one key should take a path, because a theme's name gives no way to know which file vtium selects, which the maintainer called unsafe.

vtium already has such a helper. The font helper, `vtium-config serve`, runs as an unprivileged account under a sandbox: no new privileges, memory and CPU limits, no children, a landlock domain and seccomp filters. It draws glyphs and answers in frames that the console checks before it trusts any of them. Every build starts it, the minspec builds included: `make install-tree` installs `vtium-config` beside the minspec console, and `make install-build` fails unless the console names the helper's path.

This record decides the key, what a path may name, and how each shape is found in a file. It decides how the console and the helper divide the work, how the helper draws, which size draws, and what vtium logs. It extends ADR-1303 only in that the GPU path draws a file's picture upright, as it draws vtium's own.

## Decision

### D1: the keys

Two keys join the key table's "Mouse pointer" group, both at vtium.conf's top level and on the command line, as `pointer-char` is:

| Key | Values | Default | What it does |
|---|---|---|---|
| `pointer-theme` | an absolute path | unset | The SVG file or the cursor theme's folder whose pictures the pointer takes. Unset, vtium draws its own pictures. |
| `pointer-size` | 8 to 64 pixels | unset | For a cursor theme, the nominal size of its pictures. Unset, vtium picks the size nearest the character cell's height (D6). An SVG file ignores it. |

- **A path, never a name.** vtium searches no icon folder and reads no `index.theme`. The maintainer's reason: "I guess I feel like just naming a theme with no specific path is unsafe because you don't know exactly how that is selectiing it". The person names the file or the folder they mean, so no other file can take its place. A relative path is refused, as every path key of vtium's is.
- **`pointer-size` names a nominal size,** the number a desktop's cursor size setting uses, so a person can copy it from there. It stops at 64 because vtium never shows a picture larger than 64 pixels a side.
- **An SVG file draws a cell tall,** so `pointer-size` does not apply to it. A vector picture can be drawn at exactly the size the cell needs.
- **`pointer-char` keeps the default shape.** When both keys are set, the character draws the default shape, and the file draws the other nine. `pointer-char` names one shape and the file names all ten, so the narrower key wins where they overlap. When the font cannot draw the character, the default shape takes the file's picture, then vtium's arrow.
- **vtium reads neither `XCURSOR_THEME` nor `XCURSOR_SIZE`.** The console starts as a system service before anyone logs in, so its environment is not a person's. vtium.conf is the one place its configuration lives.

### D2: what the path names

vtium looks at what is at the path when the console starts.

- **A regular file** is an SVG file.
- **A folder that holds a `cursors` folder** is a cursor theme. Its pictures are the files in `cursors/`, named as D3 says.
- **Anything else** draws vtium's own pictures, with one log line (D9). That includes a missing path and a folder with no `cursors` folder, such as `/usr/share/icons/default`, which only names another theme.
- **vtium follows no `Inherits=` line.** A theme whose cursors live in the theme it inherits from is named by that theme's folder. The Icon Theme Specification's search exists to find a theme by its name, and a path needs none of it.
- **The checks are the file system's, not a parser's.** The console reads only the path's type to tell a file from a folder. It never reads a pointer file's contents.

### D3: which picture serves each shape

In an SVG file, the shape is the `<symbol>` whose `id` is the shape's OSC 22 name: `default`, `text`, `pointer`, `crosshair`, `wait`, `progress`, `not-allowed`, `move`, `ew-resize` or `ns-resize`. The file is vtium's own format, so it takes only those names.

In a cursor theme, vtium tries the files in this order, and the first one the helper can draw serves the shape:

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
- **`move` takes `all-scroll`.** The cursor specification names no move cursor. Its `all-scroll` is the four-headed arrow that vtium's move picture draws. Adwaita's `move` file links to its plain arrow, and Breeze's to its drag-and-drop arrow.
- **`hand1` is left out,** because Adwaita links it to `grab`, an open hand.
- **The resize shapes end with the splitter cursors,** `col-resize` and `row-resize`, which mark a bar between two panes. They come last because their picture adds a bar across the arrow.
- **The file names are fixed.** No text from a program reaches a path, since OSC 22 only chooses among the ten shapes.

### D4: the SVG file

- **One file, ten symbols.** Each shape is a `<symbol>` element anywhere in the file, with an `id`, a `viewBox` and a `data-hotspot` attribute. A symbol may use the file's `<defs>`, such as a gradient, as any SVG may.
- **The hotspot is `data-hotspot="x y"`,** two numbers in the symbol's viewBox units, the point the pointer names. An attribute keeps the hotspot on the element it belongs to, with no element of its own that would draw. SVG 2 lets any element carry a `data-` attribute, and renderers ignore it. A point in viewBox units scales with the picture, so one file serves every cell height.
- **A symbol without a hotspot is skipped,** and so is one whose hotspot lies outside its viewBox. The shape then keeps vtium's picture, and vtium logs why (D9). A pointer whose aim is guessed is worse than vtium's own.
- **The size.** The viewBox's height draws at the pointer's height: a cell tall, kept between 8 pixels and `POINTER_SIDE_MAX`, as vtium's own pictures are. The width follows the viewBox's proportions, rounded up. When that width would pass `POINTER_SIDE_MAX`, the helper draws at the scale that makes the width `POINTER_SIDE_MAX` instead.
- **The hotspot's pixel** is each coordinate times the scale, rounded down, kept inside the picture.
- **How the helper draws a symbol.** The helper reads the file with usvg, adds one `<use>` of the symbol at the asked size, and draws the result with resvg. The agent checked that this draws a symbol with a gradient from `<defs>` at the asked size.
- **Antialiasing follows the file.** A file draws antialiased unless it says otherwise. SVG's `shape-rendering="crispEdges"` makes resvg fill each pixel whose centre lies inside the shape.
- **The file's size** is at most `POINTER_SVG_BYTES_MAX`, 1 MiB. vtium's own file is a few kilobytes.

### D5: the console reads, the helper draws

- **The console reads each file once,** at start, under the ownership check that a font set and vtium.conf pass. The file is root's, or the user's that vtium runs as, and so is every folder crossed on the way to it. Neither the group nor other users can write any of them. vtium-settings' walk follows a symlink, such as Adwaita's `xterm` to `text`, and checks each folder it crosses. vtium-settings gains a checked read with a size limit of the caller's choosing, since vtium.conf's own read stops at 64 KiB.
- **The console never parses a pointer file.** It sends the bytes to the font helper in a new request, the pointer ask, kind 9 of the request the helper's serve mode reads (`crates/vtium-config/src/serve.rs`). The ask carries the file's kind, SVG or Xcursor, its bytes, the shape, and the pointer heights the displays need. The console starts the helper for it if the helper is not running, as it does for glyphs.
- **The helper answers each shape with a picture frame,** a new frame kind: the shape, the size, the width, the height, the hotspot and premultiplied RGBA rows. Or it answers with a frame that says why it drew nothing. The ask ends with the done frame that every ask ends with.
- **The console checks each frame before it trusts it,** as it checks glyphs. The frame's length is within `FRAME_BYTES_MAX` before the console allocates, and the shape and the size are among those asked. The width and the height are each from 1 to `POINTER_SIDE_MAX`, and the rows hold exactly that many pixels. The hotspot lies inside the picture, and no color channel exceeds its alpha. A frame that fails leaves the shape with vtium's picture, with one log line.
- **The console keeps pictures, not files,** with one exception. For a cursor theme, the helper answers every size the file keeps (D6), and the console drops the bytes. For an SVG file, the console keeps the bytes and asks again at a zoom, because each new cell height needs a new drawing.
- **The compiled-in pictures draw first.** Until the helper answers, and when it cannot start, the pointer shows vtium's compiled-in picture. A missing helper costs the file's pictures, never the pointer.
- **Why the helper.** The helper already runs every file vtium does not trust into pixels, inside its sandbox. A flaw in an SVG or Xcursor parser then reaches only the helper, and the console reads only frames it checks field by field.

### D6: the cursor theme's files

The Xcursor parser lives in the helper, in `crates/vtium-config/src/xcursor.rs`, and runs in the serve process's sandbox. Its checks stay as the record first set them.

- **What it reads.** Every number is a 32-bit unsigned integer, least significant byte first, as every file on the agent's machine and the pinned test files are. It reads the header's magic, its length and its count of table entries. In each table entry it reads the type, the subtype and the position. It ignores the file's version and every chunk that is not an image.
- **The pixels pass through.** A theme's pixels are premultiplied ARGB. The helper lowers any channel that exceeds its alpha to the alpha, and sends them as premultiplied RGBA.
- **The checks.** A file fails when any of these is false:

  | Check | Limit |
  |---|---|
  | The file's size | at most `XCURSOR_FILE_MAX`, 16 MiB; Adwaita's busy cursors are 4.7 MB |
  | The magic | the bytes `Xcur` |
  | The header's length | at least 16, the four fields the man page lists; the table starts there |
  | The table's entries | at most `XCURSOR_TOC_MAX`, 4,096 (Adwaita's busy cursors have 360), and the whole table inside the file |
  | Each image kept | its header 36 bytes long, its type `0xfffd0002`, its subtype the table entry's, its version 1, and its pixels inside the file |
  | Each image's side | its width and height at least 1 |
  | The hotspot | inside the image: `xhot` below the width and `yhot` below the height |

- **A picture too large is no failure.** An image wider or taller than `POINTER_SIDE_MAX` does not fit the cursor plane, so the helper drops that size. Adwaita's sizes 72 and 96 go this way, and its other sizes still serve.
- **A failed file counts as missing,** so the console asks for the shape's next file name (D3). The console reads the next file only then.
- **First frame only.** The helper keeps the first image of each nominal size, in the table's order. Adwaita's wait and progress hold 60 frames at 16 ms, and Breeze's 23. A still frame still says the program is busy, and animating would mean up to 60 picture changes a second on a minspec machine. The maintainer chose the first frame.
- **The sizes kept.** The helper keeps sizes from the smallest up, while the shape's pixels stay within `POINTER_THEME_PIXELS_MAX`, 16,384, which is four pictures of 64 × 64. With `pointer-size` set, it keeps only the size nearest it. So the ten shapes hold at most 640 KiB.
- **Which size draws.** Each display's renderer picks the kept size whose nominal size is nearest `pointer-size`, or else nearest its own cell's height. Of two sizes equally near, it picks the smaller. A zoom picks again from the sizes kept, with no new ask.
- **No scaling.** A theme's picture has one-pixel outlines and a hotspot placed by hand for each size. Resampling would blur the outlines and move the hotspot off its pixel, and the theme's sizes sit close together.

### D7: vtium's own shapes as a file

- **vtium ships its shapes as `/usr/share/vtium/pointers.svg`,** installed from `crates/vtium/install/pointers.svg`. A person can copy it, change it in an SVG editor, and point `pointer-theme` at the copy.
- **The file is generated** from the definitions in `crates/vtium-render/src/arrow.rs`, the same polygons, rectangles, discs and rings the compiled-in pictures draw. Each symbol's viewBox is the shape's grid, and its hotspot is the shape's hotspot unit. Each part is drawn twice: first as a black outline stroke at 220 of 255, then filled white. The polygons keep the even-odd rule.
- **The file asks for crisp edges.** Its root carries `shape-rendering="crispEdges"`, so the helper fills each pixel whose centre lies inside a shape. That is the compiled-in pictures' own rule. The pointer then looks the same before and after the helper answers.
- **Two tests keep the file and the compiled-in shapes the same.** A test in vtium-render generates the file and fails when it differs from the committed copy, as the README's table of bounds does. `VTIUM_RENDER_BLESS=1` rewrites it. A test in tools/tier2 draws each symbol through the helper's code at every height from 8 to 64 and compares it with the compiled-in picture.
- **What the second test allows.** The white fill and the hotspot must match pixel for pixel. The outline may differ by one pixel at an edge: the compiled-in outline grows by one pixel for each 32 of height, while the file's stroke scales smoothly. Each outline pixel the helper draws lies within one pixel of the compiled-in outline, and the reverse.

### D8: turning, colors and the GPU path

- **A file's picture turns with the display** as vtium's own pictures do: `Arrow::turned` turns its pixels and its hotspot. The GPU path draws it upright into the upright frame, which ADR-1303 turns at present.
- **A file's picture keeps its own colors.** OSC 13 and 14, the pointer's foreground and background, never recolor it. vtium-term keeps and answers them as it does today.
- **A larger plane is not assumed.** When a display's cursor plane is smaller than a picture, `setup_cursor` refuses it, and that display draws the pointer in software, as it does today.

### D9: what vtium logs

Each line follows "Strings on screen" in AGENTS.md: the topic first, the reader's words, and what vtium does.

- **An unusable path,** one warning: "The pointer file /usr/share/icons/default is neither an SVG file nor a folder that holds a cursors folder, so vtium draws its own pointer pictures."
- **A file vtium does not read,** one warning for each: "vtium does not read the pointer file /usr/share/icons/Adwaita/cursors/wait, because it is longer than 16 MiB." An ownership refusal uses the words vtium.conf's refusal uses, with the path and the command that fixes it.
- **A picture the font helper cannot draw,** one warning for each: "The font helper cannot draw the wait shape from /home/kim/pointers.svg, because its symbol has no data-hotspot. vtium draws its own picture for that shape."
- **Shapes the file lacks,** one line at info level: "The pointer file /usr/share/icons/Oxygen_White has no picture for the ew-resize and ns-resize shapes, so vtium draws its own pictures for them."

`vtium check-config` checks that the path is absolute, as it checks every path key. It does not open the file, because the file is read where the console runs.

### D10: the card and the crates it touches

One agent builds this record as one card.

- **vtium-settings:** the two keys in the key table and its "Mouse pointer" group, with their help text, and the checked read with a caller's size limit (D5). `crates/vtium/install/vtium.conf.example` and `docs/config.md` regenerate from the table.
- **vtium-config:** the pointer ask and its answer frames in `src/serve.rs`, the Xcursor parser in `src/xcursor.rs`, and the SVG drawing in `src/pointer_svg.rs`. Its manifest gains `resvg`, at 0.50 with its default features off, so it builds no text or raster image support. Two fuzz targets join `crates/vtium-config/fuzz`: `xcursor` and `pointer_svg`.
- **vtium-render:** the bounds `XCURSOR_FILE_MAX`, `XCURSOR_TOC_MAX`, `POINTER_SVG_BYTES_MAX` and `POINTER_THEME_PIXELS_MAX` in `src/bounds.rs`, with their rows in the README's table. Also an `Arrow` built from a checked picture, a table of pictures each renderer picks its size from, and the generator of `pointers.svg` with its test.
- **vtium:** `src/pointer_file.rs`, which tells a file from a theme folder, reads each file through the checked read, sends the pointer asks, checks the answers and logs D9's lines. The precedence of `pointer-char` goes where `pointer-char` is applied today.
- **Makefile:** `make install-tree` and `make install` install `pointers.svg` to `share/vtium/` under the prefix, and tiny-skia's license notice beside vtium's.
- **deny.toml:** BSD-2-Clause and BSD-3-Clause in the allowed licenses, once the maintainer answers the probe on them.
- **tools/tier2:** the pinned Adwaita file, the screens and the comparison test of D7.
- **tools/vt/checks:** the guide `pointer-file.guide`.
- **ADR-1000:** the note of the `window.pointer-shape` row gains a sentence on the pointer file. The row's status does not change.

vtium-term does not change, because OSC 22, its names and its stacks stay as they are.

## Verification

The card runs at V1 (ADR-1505): its new tests and screens, each seen failing first, `make tier2` once, and each fuzz target for 60 s.

- **The Xcursor parser's unit tests** in vtium-config build files in memory. Each check of D6 has a file that passes at its limit and one that fails one past it. A picture past 64 a side drops that size alone. A hotspot on the picture's last pixel passes, and one past it fails. A file cut short inside the table, an image header or the pixels fails each time. A comment chunk is ignored, two images of one size keep the first, and the kept sizes stop at the pixel budget.
- **The SVG drawing's unit tests:** a symbol with a gradient from `<defs>`, a symbol missing its hotspot, and a hotspot outside the viewBox. Others cover an unknown `id`, a wide viewBox drawn `POINTER_SIDE_MAX` wide, and a file past 1 MiB. Each hotspot lands on the pixel D4's rule gives.
- **The pointer ask's tests** in vtium-config and vtium: a stand-in helper that answers each wrong frame the console must refuse. Those are a picture too large, rows of the wrong length, a hotspot outside, a channel over its alpha, and a shape not asked for. Following the testing rules, the stand-in checks what the real helper checks.
- **The console's tests** in vtium run under a temporary root that the tests own. They tell a file from a theme folder and refuse a folder with no `cursors`. They cover D3's order of names, a symlinked cursor file, and a file in a folder that others can write. Each D9 line fires. `pointer-char` keeps the default shape, and the compiled-in picture draws until the helper answers.
- **The size choice:** the nominal size nearest `pointer-size` and nearest a cell height, and the smaller of two equally near. A zoom picks again from the kept sizes with no new ask, and asks again for an SVG file.
- **D7's two tests:** the generated file against the committed copy, and each symbol against the compiled-in picture at every height from 8 to 64.
- **Tier 2:** ten screens, `pointer-file-default` to `pointer-file-ns-resize`, on the 12 × 3 cell screen that `pointer-default` uses. Each draws one shape from vtium's shipped `pointers.svg` through the helper. An eleventh, `pointer-file-adwaita`, draws Adwaita's default arrow at nominal size 24. A twelfth, `pointer-file-turned`, draws the SVG's default shape on a display turned right. The screens are provisional until the maintainer reviews them at 1:1.
- **The pinned theme file.** Tier 2 fetches Adwaita 51.0's `default` file from GNOME's repository at the tag `51.0`, as ADR-1503 D9 pins test inputs. It comes by its URL and SHA-256 into tier 2's cache, and is never committed. Adwaita's license, CC-BY-SA-3.0 or LGPL-3.0-only, lets vtium's tools fetch and use it. The URL is `https://gitlab.gnome.org/GNOME/adwaita-icon-theme/-/raw/51.0/Adwaita/cursors/default`, and its SHA-256 is `47f579498d72b12d828aa40e7e484c5ebd6b75ae04293cf5c96532c21c3e1594`. The agent fetched it on 2026-11-15, and it matched the file the adwaita-cursors 51.0 package installs.
- **The fuzz targets** in `crates/vtium-config/fuzz`: `xcursor` feeds any bytes to the parser, and `pointer_svg` feeds any bytes to the SVG drawing, each with a size taken from the input. Each asserts that nothing panics, and that each picture is at most 64 a side with its hotspot inside. The runs' rows go in `crates/vtium-config/fuzz/runs.tsv`.
- **The VT guide** `tools/vt/checks/pointer-file.guide` runs the console with its own configuration, installed as the restart guide installs one. It walks the ten shapes three times: with no file, with `pointer-theme = /usr/share/vtium/pointers.svg`, and with `pointer-theme = /usr/share/icons/Adwaita`. One more step names a path that does not exist. The maintainer judges that the first two look alike, that Adwaita's pictures show, and that each hotspot is where its picture points.
- The tests of the file's drawing carry `covers: window.pointer-shape`.

## Consequences

### Positive

- The console's pointer can match a desktop's theme, or take pictures a person draws in an SVG editor, with one line of vtium.conf, on every build.
- vtium's own pointers come as a file, a starting point for a person's own.
- No pointer file is parsed in the console. A bad file reaches only the sandboxed helper, and the console takes only pictures it has checked.
- An SVG picture draws at exactly the cell's size, at every zoom.

### Negative

- The helper grows by about 0.9 MB, and its dependencies gain a 2D rasterizer and an XML parser.
- vtium's license list gains two BSD licenses, and the package ships one more license notice.
- An SVG file costs a new ask at each zoom, and its pictures arrive a moment after the zoom draws, with the compiled-in picture in between.
- A cursor theme's busy cursors stand still, and its pictures ignore OSC 13 and 14.

### Neutral

- Without `pointer-theme`, nothing changes: vtium draws its compiled-in pictures, and `pointer-char` works as it does today.
- The ten shapes and the names programs use for them do not change.

## Alternatives Considered

- **A PNG file for each shape.** Not taken: the maintainer declined it. It needs the image helper, which the minspec builds lack, and a person writes ten paths and ten hotspots.
- **Name a theme and search the icon folders for it,** following its `Inherits=` line. Not taken. The maintainer's reason: "I guess I feel like just naming a theme with no specific path is unsafe because you don't know exactly how that is selectiing it". A path names one file.
- **Parse the cursor files in the console,** as the record first decided. Not taken: the maintainer asked for the pictures to be drawn in a sandboxed helper and supplied as a bitmap. A parser in the console runs with the console's privileges on a file a person supplies.
- **A small SVG reader of vtium's own,** taking only paths and fills. Not taken: the maintainer asked for SVG files from any editor, and editors write gradients, transforms, `<use>` and CSS. A partial reader would draw those files wrong, and quietly.
- **Scale a cursor theme's nearest size to the cell.** Not taken: resampling blurs its one-pixel outlines and moves its hotspot.
- **Animate busy cursors.** Not taken: the maintainer chose the first frame.
- **Let the file win over `pointer-char`.** Not taken: the maintainer chose `pointer-char`.
- **A hotspot as an element,** such as a `<circle>` named for it inside the symbol. Not taken: the element draws unless it is hidden, and an editor moves it apart from the shape.
- **Draw vtium's own pictures through the helper always,** from the shipped file. Not taken: the pointer must show before the helper answers and when it cannot start, so the compiled-in pictures stay.
