# vtium's additions to the replay interface

Interface 1.7 is `corpus/INTERFACE.md`, interface 1.0, with the additions below. Each is a command, a feature, a view, or a line that only a new command, feature or view prints, so it raises MINOR under 1.0's "Versioning and features", and every golden stays as it is. The runner and the probe runner read the fenced blocks here after 1.0's: this `describe` block's version line replaces 1.0's, and its view, feature and command lines follow 1.0's lines of the same kind; this `grammar` block's lines join 1.0's, using its fragments.

## The fixture host checks a picture's size

vtium's fixture host answers a `png` request as 1.0's table says, with one check before it: a picture whose width or height passes the request's largest picture, which the terminal takes from `max-graphic-size`, is answered `TOO_LARGE`, and one whose width times its height times 4 passes the most bytes the request allows, which the terminal takes from its image budget, is answered `OVER_BUDGET`. A picture exactly at a bound is answered as before. The image helper reads a picture's header and makes the same check before it decodes (ADR-1100, D7), so the fixture host, standing in for it, checks what it checks.

## iTerm2's pictures

The reference host also installs vtium-image's iTerm2 handler and turns it on, so a case's OSC `1337;File=` strings and its transfers in parts (`MultipartFile=`, `FilePart=`, `FileEnd`) show pictures (ADR-1103). No case of 1.0 holds such a string, so no golden of 1.0 changes. The handler hands each picture's file to the fixture host as a direct request with the any-image flag, and with the frames flag when the store's frame allowance is above 0. Its `host:` line has the flags `any`, or `any,frames`, and the fixture host answers it from the case's fixture files: `NAME.rgba` holds a still picture, as for `png`, and is checked against the request's bounds the same way; `NAME.frames` holds an animated GIF's frames, as the width, the height, the frame count and the loop count, each a little-endian `u32` with the loop count `4294967295` for none, then a little-endian `u16` delay in hundredths of a second for each frame, then each frame's straight RGBA. The fixture host answers frames the request did not ask for, more frames than its `max_frames`, or frames past the first over its `max_frame_bytes` with the first frame alone, as the image helper does. A request with neither file is answered `CORRUPT`, as a file the helper cannot decode is.

## The clipboard

A program sets the clipboard with OSC 52 (`osc.52.write` in ADR-1000), and a host takes what it set. The reference host takes it only when asked, with the `clipboard` command.

| Command | Keys | Feature | What it does |
|---|---|---|---|
| `clipboard` | none | `clipboard` | Takes the clipboard as a host does, and prints `test clipboard HEX`, HEX being the bytes the last OSC 52 write set since the previous `clipboard`, two lowercase hex digits each, or `test clipboard none` when no write set any. |

| Feature | What declaring it means |
|---|---|
| `clipboard` | OSC 52 writes set the clipboard; `clipboard`. |

## Kitty animation

A kitty animation is an image with frames past its own pixels: a program adds and edits frames with `a=f`, composes one frame onto another with `a=c`, sets the gaps, the run state and the loops with `a=a`, and deletes the frames with `d=f` or `d=F` (ADR-1102). The image's own pixels are its first frame, the *root frame*. The store records what these commands say and never which frame is on show, which an animation clock in the vtium binary picks, so a replay, which has no clock, prints the store's record.

After an image's `image` line, a block prints that image's `anim` lines when an animation command has named the image: one line for the animation, then one line for each frame, from 1, the root frame, to the last. An image that no animation command named prints none. A block prints `anim` lines only for the images it prints `image` lines for, the images its placements show. An animated image's placements show its current frame, so its `image` line hashes that frame's pixels, the same digest as the current frame's `anim` line. The interface check holds the order: an image's `anim M frames` line comes straight after its `image M` line and only there, its current frame is one of its frames, and its frame lines follow it numbered 1, 2, 3 and so on, exactly as many as its frames.

`anim M frames F current C run RUN loops L stamps S T` is image M's animation as the store records it. F is the number of frames, the root frame included. C is the frame a command last made current, which `a=a` with `c` sets and which becomes 1 when the frames are deleted or freed. RUN is `stopped`, `loading` or `looping`, as the last `a=a` with `s` set it (`s=1`, `s=2` and `s=3`), and `stopped` when no command set it or the frames were deleted or freed. L is the loops the last `a=a` with `v` asked for: 1 plays forever, a larger number plays that many less one, and it starts at 1. S is the frame stamp, a count that the store moves each time a command makes a frame current, and T is the run stamp, a count that it moves each time a command sets the run state or the loops; deleting or freeing the frames moves both. Both start at 0 and follow from the bytes alone.

`anim M frame K gap G digest HEX` is frame K of image M: its gap G in milliseconds, negative for a gapless frame and 0 for a root frame whose gap no command set, and the digest of its whole pixels, composed from its base and its changes as the store composes them for a display, hashed as the `image` line hashes an image's pixels.

| Feature | What declaring it means |
|---|---|
| `kitty-animation` | The kitty graphics protocol's `a=f`, `a=c`, `a=a`, `d=f` and `d=F` act on an image's frames, and blocks print `anim` lines. |

A worked example: a 2 by 1 image, then a frame on a clear canvas with its right pixel green and a gap of 80 ms, then `a=a` running it in a loop with frame 2 current, which its placement then shows.

```example kitty-animation
size 4 1
needs images kitty-graphics kitty-animation
cmd cell;w=2;h=1
send "\e_Ga=T,f=32,s=2,v=1,i=1,C=1,q=2;/wAA/wAA//8=\e\\"
send "\e_Ga=f,i=1,s=1,v=1,x=1,z=80,q=2;AP8A/w==\e\\"
send "\e_Ga=a,i=1,s=3,c=2,q=2\e\\"
```

```output kitty-animation
size 4x1
cursor 0,0
flags AUTO_WRAP
row 0 ||
placement 0 row 0 col 0 box 1x1 at 0,0 size 2x1 src 0,0 2x1 cell 2x1 z 0 above-text kitty image 0
image 0 2x1 digest f052c40d
anim 0 frames 2 current 2 run looping loops 1 stamps 1 1
anim 0 frame 1 gap 0 digest bc648309
anim 0 frame 2 gap 80 digest f052c40d
```

## Right-to-left text: the display view

vtium keeps every row in the order the program wrote it, and draws each paragraph (the rows a wrap joins) in the order Unicode's bidirectional algorithm gives (ADR-1004). The screen and frame views print each row in the order it was written, with each cell where the program wrote it and its damage as the screen keeps it, so their goldens are the same in a build with right-to-left text and one without. The display view prints the rows as vtium draws them, so a case can judge the drawn order without pixels.

`--display` prints the display view. A block of it starts with the head every view prints (`size`, `cursor`, `flags` and the replies), then a `display-cursor` line, then one `row` line for each row of the screen, and prints no runs, placements or images.

`display-cursor X,Y SIDE` is the cell the cursor is drawn on, counted from the left of the drawn row, and the side of that cell a bar cursor stands on: `right` when the cell's character resolves right to left, and `left` otherwise. The `cursor` line of the head still gives the cursor's logical cell.

`row N |TEXT|` is row N as drawn, from the left: a character drawn mirrored prints as its counterpart, as `(` prints `)` inside right-to-left text, with any marks on it after the counterpart, an empty cell prints a space, a cell prints all the codepoints it holds, and the second cell of a wide character prints nothing. The empty cells at the right end of the row are trimmed, so the empty cells a right-to-left paragraph draws at its left print as spaces.

| Feature | What declaring it means |
|---|---|
| `bidi` | vtium draws each paragraph in the order the bidirectional algorithm gives, and `--display` prints the display view. |

A worked example: `abc`, the Hebrew word shalom, ` (1) ` and `def`, written left to right, are drawn with the Hebrew word reversed and its brackets mirrored, and the cursor is drawn after `def`.

```example bidi
size 20 1
view display
needs bidi
send "abc \u{5E9}\u{5DC}\u{5D5}\u{5DD} (1) def"
```

```output bidi
size 20x1
cursor 16,0
flags AUTO_WRAP
display-cursor 16,0 left
row 0 |abc (1) םולש def|
```

## The key command's modifiers

The `key` command's `mods` takes 0 to 255, where 1.0 stops at 31. The bits 1.0 names keep their meaning: 1 Shift, 2 Lock, 4 Ctrl, 8 Alt and 16 Logo. The new bits are 32, NumLock is on, 64 Hyper and 128 Meta, which a host reports where the keymap gives Hyper or Meta a modifier of its own. The kitty keyboard protocol reports each of them in a key's modifiers (ADR-1005, D2), and xterm's encoding reads NumLock under mode 1035. A `key` line with a `mods` of 31 or less presses the key it pressed before, so every golden stays as it is. The `mouse` command's `mods` still stops at 31.

## Owed: a key's event, its unshifted key and its base layout key

The kitty keyboard protocol reports when a key repeats and when it is let go, and it reports a key's unshifted form and its key on the US PC-101 layout (ADR-1005, D2, D9 and D12). vtium-term's description of a key holds all three, and the `key` command does not take them yet. The two features below describe the keys the `key` command gains. vtium owes them to the kitty keyboard protocol's issue, #42: its second slice adds `key-events` to vtium-replay, and its third slice adds `key-forms`.

Until a slice adds its feature, vtium-replay does not declare that feature, and the runner does not count it among the features `--describe` must declare. A capability scenario that needs the feature fails as absent, which is what its `missing` line in `capabilities.list` expects. The runner keeps its own list of the owed features, and it refuses a `--describe` that declares one of them. So the change that makes vtium-replay declare a feature also takes it off that list, and it raises the interface's MINOR version.

| Key of `key` | Form | Feature | What it means |
|---|---|---|---|
| `event` | name, `press`, `repeat` or `release`, default `press` | `key-events` | Whether the key was pressed, repeated by the host's repeat timer, or let go. A release's `mods` are the modifiers after the key's own effect, as a press's are, so the release of Shift holds no Shift. |
| `unshifted` | hex, 0 to 1fffffff, default 0 | `key-forms` | The keysym the key makes at the first shift level of the active layout, which is the unshifted key, or 0 when the host reports none. A host reports none when a modifier the kitty keyboard protocol cannot encode, such as AltGr, chose the key's symbol. |
| `base` | hex, 0 to 10ffff, default none | `key-forms` | The character the key types on the US PC-101 layout, which is the base layout key, or none when that layout has no such key. |

| Feature | What declaring it means |
|---|---|
| `key-events` | The `key` command takes `event`. |
| `key-forms` | The `key` command takes `unshifted` and `base`. |

In a build that does not declare a feature, a `key` command that gives one of its keys exits 2, as any key a command does not take does.

## Overline, superscript and subscript

Interface 1.0 reserves three keys for features it lacks: `overline`, `superscript` and `subscript` (1.0's "Reserved keys"). vtium-term keeps the overline that SGR 53 sets and SGR 55 clears, and the superscript and subscript that SGR 73 and 74 set and SGR 75 clears, as mintty defines them. With the feature `attrs-renditions`, the screen view's `attr` lines and the frame view's `cells` and `frame-cursor` lines print these keys where 1.0 reserves them, after `strike`: `overline` when the overline is set, then `superscript` or `subscript` when one of them is set. A cell holds at most one of the two. No case of 1.0 sets them, so no golden of 1.0 changes.

| Feature | What declaring it means |
|---|---|
| `attrs-renditions` | The screen and frame views print `overline`, `superscript` and `subscript`. |

## The selection and copy

A user marks text with the pointer, and the host copies the marked text. The reference host selects through vtium-term's selection calls, the calls the vtium binary makes for a press of the left button and a drag, a double click, a triple click, and Ctrl with the right button. The `select` command names each of these by what it selects. It names a cell in the view: column `col` and row `row`, counted from 0 at the top left of the rows the view shows. While the view is scrolled back, a row of the view is a row of the scrollback. In a build with right-to-left text, `col` is the column as drawn, where the pointer is, and vtium-term finds the character written there (ADR-1004, D10).

| Command | Keys | Feature | What it does |
|---|---|---|---|
| `select` | `how`: name, `start`, `to`, `word`, `line`, `output` or `clear`; `col`: decimal, below the screen's width; `row`: decimal, below its height. Every `how` but `clear` requires `col` and `row`, and `clear` takes neither. | `selection` | Selects as the host does (below). |
| `copy` | none | `selection` | Copies the selection as a host does, and prints `test copy HEX`, HEX being the selected text as UTF-8, two lowercase hex digits a byte, or `test copy none` when nothing is selected or the selection holds no text. |

What each `how` selects:

- `start` starts a selection at the cell, as a press of the left button does. The selection holds that one cell.
- `to` extends the selection from the cell where it started to this cell, as moving the pointer with the button held does. Whichever of the two comes first in reading order is the selection's first cell. With no selection, `to` does nothing.
- `word` selects the word at the cell, as a double click does: the run of cells around it up to a space or, going right, an empty cell. A word that reaches the end of a row that wrapped goes on at the start of the next row. On a space, `word` selects nothing.
- `line` selects the whole line at the cell, every row of it when it wrapped, as a triple click does.
- `output` selects, as whole lines, the output of the command at the cell, as Ctrl with the right button does. OSC 133's marks set the output out (ADR-1002): it runs from the nearest line at or above the cell that `OSC 133 ; C` marked to the line before the next prompt, or to the last line written. On a prompt, on a command's input, on a row in no command's output, and on the alternate screen, `output` selects nothing, and the host clears the selection.
- `clear` clears the selection.

The copied text takes each cell's whole cluster, a space for an empty cell up to the line's last cell that holds something, and nothing for the second cell of a wide character. Each line ends with a line end (0a), except a row that wrapped, which goes on into the next, and the last line of a selection made with `start`, `to` or `word`. A selection made with `line` or `output` ends with a line end.

The selection holds its text, not its place on the screen. When the screen scrolls, the selection moves with its text, and a line the screen discards, such as a line that IL, DL or a scroll of the alternate screen removes, takes the selection with it. The frame view draws each selected cell with `inverse` toggled, as vtium draws the selection inverted, so a frame view's block shows where the selection lies.

| Feature | What declaring it means |
|---|---|
| `selection` | `select` and `copy`, and the frame view draws selected cells inverted. |

A worked example: a word, then a drag from `f` back to `c`, then nothing.

```example selection
size 8 2
needs selection
send "ab cd\r\nef"
cmd select;how=word;col=4;row=0
cmd copy
cmd select;how=start;col=1;row=1
cmd select;how=to;col=3;row=0
cmd copy
cmd select;how=clear
cmd copy
```

```output selection
test copy 6364
test copy 63640a6566
test copy none
size 8x2
cursor 2,1
flags AUTO_WRAP
row 0 |ab cd|
row 1 |ef|
```

The frame view of a selected word.

```example selection-frame
size 4 1
view frame
needs selection view-frame
send "ab c"
cmd select;how=word;col=1;row=0
```

```output selection-frame
size 4x1
cursor 4,0
flags AUTO_WRAP
palette legacy fg=#e5e5e5 bg=#000000
changed 4
frame-cursor 3,0 default visible w=1 fg=#000000 bg=#e5e5e5 ul=#000000
row 0 |ab c|
cells 0 0-1 w=1 fg=#000000 bg=#e5e5e5 ul=#000000 inverse
```

## Prompt jumps

A shell marks its prompts with OSC 133 (ADR-1002), and a user moves the view from one prompt to the next with the vtium binary's prompt keys. The reference host moves the view with the `prompt` command.

| Command | Keys | Feature | What it does |
|---|---|---|---|
| `prompt` | `dir`: name, `up` or `down` | `marks` | Moves the view to the nearest prompt that `OSC 133 ; A` or `OSC 133 ; N` marked above the view's top row (`up`) or below it (`down`). That prompt's line becomes the view's top row. Going down past the newest prompt in the scrollback returns the view to the screen. With no such prompt, and on the alternate screen, the view does not move. |

| Feature | What declaring it means |
|---|---|
| `marks` | vtium keeps OSC 133's marks with their lines; `prompt`, and `select` with `how=output` when `selection` is declared too. |

A worked example: two prompts, the view moved up to the first, the command's output selected and copied there, and the view moved back down.

```example marks
size 6 2
needs marks selection
cmd sb-max;n=10
send "\e]133;A\a$ ls\r\n\e]133;C\aa\r\nb\r\n\e]133;A\a$ "
cmd prompt;dir=up
cmd select;how=output;col=0;row=1
cmd copy
check
cmd prompt;dir=down
```

```output marks
test copy 610a620a
checkpoint 147
size 6x2
cursor 2,1
flags AUTO_WRAP
row 0 |$ ls|
row 1 |a|
checkpoint 175
size 6x2
cursor 2,1
flags AUTO_WRAP
row 0 |b|
row 1 |$|
```

## Links

A program attaches a URI to the text it prints with OSC 8, and vtium keeps the link in each cell the text takes (ADR-1002). The vtium binary shows a link when the pointer is over it: every cell of the link takes a dashed underline, and while Ctrl is held the URI shows on an overlay row. Ctrl with the left button copies the URI. The reference host reads the links through vtium-term's calls for these, the calls the vtium binary makes. The `link` and `hover` commands name a cell of the view: column `col` and row `row`, counted from 0 at the top left of the rows the view shows. While the view is scrolled back, a row of the view is a row of the scrollback.

| Command | Keys | Feature | What it does |
|---|---|---|---|
| `link` | `col`: decimal, below the screen's width; `row`: decimal, below its height | `links` | Prints `test link HEX`, HEX being the URI of the cell's link as UTF-8, two lowercase hex digits a byte, the URI that Ctrl with the left button copies; or `test link none` when the cell has no link. |
| `hover` | `col` and `row`, as `link` takes them, both or neither; `ctrl`: flag, 0 when absent | `links` | Puts the pointer over the cell, or takes it away when neither `col` nor `row` is given, with Ctrl held when `ctrl=1`. Then prints two lines: `test hover` and `test overlay`. |

- `test hover RUNS` gives the cells of the view that the link under the pointer takes, as runs `ROW:FIRST-LAST` of the cells in one row, top to bottom and left to right, joined by commas; or `test hover none` when the pointer is over no link. Cells written while one link was open take that link, and links opened with the same URI and the same `id` parameter are one link, so its runs can lie on several rows.
- `test overlay ROW HEX` gives the overlay row while Ctrl is held over a link: the view's bottom row, or its top row when the pointer is on the bottom row, and the link's URI as HEX; or `test overlay none`.

The pointer stays where `hover` put it until the next `hover`. A link the stream writes under the pointer is under the pointer from then on, but only the next `hover` prints it.

| Feature | What declaring it means |
|---|---|
| `links` | vtium keeps OSC 8's links in their cells; `link` and `hover`. |

A worked example: a link with an id whose two pieces lie on two rows, a link with no id, and the pointer over the first piece with Ctrl held, then taken away.

```example links
size 8 3
needs links
send "\e]8;id=a;x:a\e\\ab\e]8;;\e\\ \e]8;;x:b\e\\c\e]8;;\e\\\r\n\e]8;id=a;x:a\e\\de\e]8;;\e\\"
cmd link;col=3;row=0
cmd link;col=2;row=0
cmd hover;col=1;row=1;ctrl=1
cmd hover
```

```output links
test link 783a62
test link none
test hover 0:0-1,1:0-1
test overlay 2 783a61
test hover none
test overlay none
size 8x3
cursor 2,1
flags AUTO_WRAP
row 0 |ab c|
row 1 |de|
row 2 ||
```

## The BackSpace key

What the BackSpace key sends is the host's setting. The reference host sends 0x08 until the `backspace` command chooses otherwise.

| Command | Keys | Feature | What it does |
|---|---|---|---|
| `backspace` | `del`: flag | `backspace` | Chooses what BackSpace sends: 0x08 with `del=0`, and 0x7f with `del=1`. |

| Feature | What declaring it means |
|---|---|
| `backspace` | `backspace`. |

```example backspace
size 4 1
needs input backspace
cmd key;sym=ff08
cmd backspace;del=1
cmd key;sym=ff08
```

```output backspace
size 4x1
cursor 0,0
flags AUTO_WRAP
reply 087f
row 0 ||
```

## The bell

A program rings the bell with BEL (0x07). vtium passes each BEL to the host while mode 1042 is set, and it is set at start (ADR-1000's `DEC-1042`). A block prints `bell N` after its replies when N bells reached the host since the previous block, and prints no `bell` line when none did. Each BEL counts, wherever the stream is cut. A BEL that ends a string, as it ends OSC 0's, rings no bell. No case of 1.0 rings the bell, so no golden of 1.0 changes.

| Feature | What declaring it means |
|---|---|
| `bell` | Blocks print `bell` lines. |

A worked example: two bells, with a title string ended by BEL and a reply between them.

```example bell
size 4 1
needs bell
send "\a\e]0;title\a\e[5n\a"
```

```output bell
size 4x1
cursor 0,0
flags AUTO_WRAP
reply 1b5b306e
bell 2
row 0 ||
```

## The state

Some of a screen's state shows in no view: the margins, the tab stops, the modes, the scrollback, the pen and the colors. A program can ask for most of it with DECRQM, DECRQSS and OSC 10 to 12, but each query writes a reply, which then shows among the replies of the case that asked. The `state` command prints that state without a query.

| Command | Keys | Feature | What it does |
|---|---|---|---|
| `state` | none | `state` | Prints the screen's state as six `test state` lines, in the order below. |

- `test state margins T-B L-R` gives the scroll region's first and last rows (DECSTBM) and the left and right margins' columns (DECSLRM), each counted from 0. While mode 69 is reset, the left and right margins are the screen's first and last columns.
- `test state tabs LIST` gives the columns that hold a tab stop, counted from 0, in ascending order and joined by commas, or `none`.
- `test state modes ansi LIST dec LIST` gives the ANSI modes and then the DEC private modes in force, those DECRQM reports set (1), each by its number, in ascending order and joined by commas, or `none`. The mouse tracking modes (9, 1000, 1002 and 1003) and encodings (1006, 1015 and 1016) are among the DEC private modes. Which modes a build has follows ADR-1000: a build with images has the sixel modes, and a build with right-to-left text has BDSM (8) and mode 1243, so this line differs between builds.
- `test state scrollback lines L view V max M` gives the rows the scrollback holds, with the rows that wait to be rewrapped after a resize counted at the current width; how many rows the view is scrolled back, 0 when it shows the screen; and the most rows the scrollback keeps, as `sb-max` set it.
- `test state pen KEYS` gives the pen, the style that printed text takes, as the keys of the screen view's `attr` line in their order. With no key, the pen is the default.
- `test state colors fg=#rrggbb bg=#rrggbb cursor=C` gives the default foreground and background, from the palette or as OSC 10 and 11 set them, and the cursor's color as OSC 12 set it, or `default` when nothing set it.

| Feature | What declaring it means |
|---|---|
| `state` | `state`. |

A worked example. Its stream first resets the modes that only some builds have (BDSM, 1070 and 1243), so that its output is the same in every build.

```example state
size 10 4
needs state
send "\e[8l\e[?1070;1243l\e[2;3r\e[3g\e[5G\eH\e[1;4;38;5;196m\e]12;#00ff00\a"
cmd sb-max;n=100
cmd state
```

```output state
test state margins 1-2 0-9
test state tabs 4
test state modes ansi 12 dec 7,8,12,25,1007,1034,1035,1036,1042,2027,2028
test state scrollback lines 0 view 0 max 100
test state pen fg=#ff0000 bold underline
test state colors fg=#e5e5e5 bg=#000000 cursor=#00ff00
size 10x4
cursor 4,0
flags AUTO_WRAP
row 0 ||
row 1 ||
row 2 ||
row 3 ||
```

## Font and window operations

A program asks for a font set by name with OSC 50, and asks which font set is in use with `OSC 50 ; ? ST` (`text.font-change` in ADR-1000). A program asks for a text area of so many rows and columns with XTWINOPS 8, `CSI 8 ; height ; width t` (`window.resize`). The terminal takes each only while the host turns it on, as vtium's `allow-font-ops` and `allow-window-ops` keys do, and keeps the request until the host takes it. The reference host starts with both off and takes a request only when asked, with the `font-request` and `grid-request` commands. It acts on neither: a case that wants the size a host would make resizes with `resize`.

| Command | Keys | Feature | What it does |
|---|---|---|---|
| `font-ops` | `on`: flag, default 1 | `font-ops` | Turns OSC 50 on (1) or off (0), as the host's `allow-font-ops` does. Turning it off drops a request the host has not taken. |
| `font-name` | `name`: 1 to 64 bytes of `A-Z a-z 0-9 . _ + -`, not starting with `.` | `font-ops` | Names the font set in use, which OSC 50's query reports. Until it does, the query reports an empty name. |
| `font-request` | none | `font-ops` | Takes the font set request as a host does, and prints `test font-request NAME`, NAME being the name the last OSC 50 asked for since the previous `font-request`, or `test font-request none` when none asked. |
| `window-ops` | `on`: flag, default 1 | `window-ops` | Turns XTWINOPS 8 on (1) or off (0), as the host's `allow-window-ops` does. Turning it off drops a request the host has not taken. |
| `grid-request` | none | `window-ops` | Takes the text area request as a host does, and prints `test grid-request ROWS COLS`, from the last XTWINOPS 8 since the previous `grid-request`, or `test grid-request none` when none asked. Each of ROWS and COLS is the number of rows or columns asked for, `keep` for a parameter left out, which keeps that side, or `display` for 0, which takes the display's length on that side, as ctlseqs says. |

| Feature | What declaring it means |
|---|---|
| `font-ops` | `font-ops`, `font-name` and `font-request`. |
| `window-ops` | `window-ops` and `grid-request`. |

A worked example: a request for 25 rows of 80 columns while window operations are off, then one for 0 rows of 80 columns once they are on, and the size report, which answers the size the host made.

```example window-ops
size 10 4
needs window-ops
send "\e[8;25;80t"
cmd grid-request
cmd window-ops;on=1
send "\e[8;0;80t"
cmd grid-request
cmd resize;c=80;r=4
send "\e[18t"
```

```output window-ops
test grid-request none
test grid-request display 80
size 80x4
cursor 0,0
flags AUTO_WRAP
reply 1b5b383b343b383074
row 0 ||
row 1 ||
row 2 ||
row 3 ||
```

## The worked examples' strings

The worked examples, here and in 1.0, read each `send` string with the string syntax of tier 1's scenarios (ADR-1504, D2), so the examples and the scenarios have one syntax. It adds three escapes to 1.0's list: `\a` stands for the byte 07, `\b` for the byte 08, and `\u{HEX}` for the UTF-8 of the code point HEX. Every string 1.0's list accepts reads as it did, so no example's input changes.

## The describe block and the grammar

The grammar below adds the forms of the `attr`, `cells` and `frame-cursor` lines that print the keys `attrs-renditions` prints, the `test` lines of the clipboard, the copy, the links, the state and the font and text area requests, the `bell` line, the two forms of the `anim` line, and the display view's `display-cursor` line and structure. An `anim` line follows its image's `image` line, and a `bell` line follows a block's replies, so `HEAD`, `IMAGES` and the fragments and structures built on them are given again here. A fragment takes its value where it is defined, so the views' fragments and structures must be restated to use the new `HEAD` and `IMAGES`. They differ from 1.0's only in those two.

```describe
interface 1.7
view display
feature clipboard
feature attrs-renditions
feature kitty-animation
feature bidi
feature selection
feature marks
feature links
feature backspace
feature state
feature bell
feature key-events
feature key-forms
feature font-ops
feature window-ops
command clipboard clipboard
command select selection
command copy selection
command prompt marks
command link links
command hover links
command backspace backspace
command state state
command font-ops font-ops
command font-name font-ops
command font-request font-ops
command window-ops window-ops
command grid-request window-ops
```

```grammar
define RATTR (?: fg={CODE})?(?: bg={CODE})?(?: bold)?(?: italic)?(?: underline(?:=double|=curly|=dotted|=dashed)?)?(?: ul={CODE})?(?: inverse)?(?: protect)?(?: blink)?(?: dim)?(?: strike)?(?: overline)?(?: superscript| subscript)?
define RFCELL w=[0-2] fg={RGB} bg={RGB} ul={RGB}(?: ulset)?(?: underline=(?:single|double|curly|dotted|dashed))?(?: bold)?(?: italic)?(?: dim)?(?: blink)?(?: inverse)?(?: protect)?(?: strike)?(?: overline)?(?: superscript| subscript)?
line attr screen attr {N} {N}-{N}{RATTR}
line cells frame cells {N} {N}-{N} {RFCELL}
line frame-cursor frame frame-cursor {N},{N} (?:{STYLE}) (?:hidden|visible {RFCELL})
line test * test clipboard (?:none|(?:[0-9a-f]{2})+)
line host: * host: [1-9][0-9]* t=d flags=any(?:,frames)? O=0 max=0 min=0 name= len={N}
line anim * anim {N} frames [1-9][0-9]* current [1-9][0-9]* run (?:stopped|loading|looping) loops [1-9][0-9]* stamps {N} {N}
line anim * anim {N} frame [1-9][0-9]* gap {INT} digest [0-9a-f]{8}
line display-cursor display display-cursor {N},{N} (?:left|right)
line bell * bell [1-9][0-9]*
line test * test copy (?:none|(?:[0-9a-f]{2})+)
line test * test link (?:none|(?:[0-9a-f]{2})+)
line test * test hover (?:none|{N}:{N}-{N}(?:,{N}:{N}-{N})*)
line test * test overlay (?:none|{N} (?:[0-9a-f]{2})+)
define NUMS none|{N}(?:,{N})*
line test * test state margins {N}-{N} {N}-{N}
line test * test state tabs (?:{NUMS})
line test * test state modes ansi (?:{NUMS}) dec (?:{NUMS})
line test * test state scrollback lines {N} view {N} max {N}
line test * test state pen{RATTR}
line test * test state colors fg={RGB} bg={RGB} cursor=(?:{RGB}|default)
line test * test font-request (?:none|[A-Za-z0-9_+-][A-Za-z0-9._+-]{0,63})
line test * test grid-request (?:none|(?:[1-9][0-9]*|keep|display) (?:[1-9][0-9]*|keep|display))
define HEAD (?: size)(?: cursor)(?: flags)(?: reply| host:)*(?: bell)?
define IMAGES (?: placement)*(?: image(?: anim)*)*
define SCREEN {HEAD}(?: row(?: attr)*)+{IMAGES}
define DRAW2 {HEAD}(?: palette)(?: row(?: cells)*)+{IMAGES}
define FRAME {HEAD}(?: palette)(?: changed)(?: frame-cursor)(?: row(?: cells)*)+{IMAGES}
structure screen (?:(?: test)*(?: checkpoint){SCREEN})+|(?: test)*{SCREEN}
structure draw2 (?:(?: test)*(?: checkpoint){DRAW2})+|(?: test)*{DRAW2}
structure frame (?:(?: test)*(?: checkpoint){FRAME})+|(?: test)*{FRAME}
define DISPLAY {HEAD}(?: display-cursor)(?: row)+
structure display (?:(?: test)*(?: checkpoint){DISPLAY})+|(?: test)*{DISPLAY}
```
