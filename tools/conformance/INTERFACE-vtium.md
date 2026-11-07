# vtium's additions to the replay interface

Interface 1.4 is `corpus/INTERFACE.md`, interface 1.0, with the additions below. Each is a command, a feature, a view, or a line that only a new command, feature or view prints, so it raises MINOR under 1.0's "Versioning and features", and every golden stays as it is. The runner and the probe runner read the fenced blocks here after 1.0's: this `describe` block's version line replaces 1.0's, and its view, feature and command lines follow 1.0's lines of the same kind; this `grammar` block's lines join 1.0's, using its fragments.

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

## Overline, superscript and subscript

Interface 1.0 reserves three keys for features it lacks: `overline`, `superscript` and `subscript` (1.0's "Reserved keys"). vtium-term keeps the overline that SGR 53 sets and SGR 55 clears, and the superscript and subscript that SGR 73 and 74 set and SGR 75 clears, as mintty defines them. With the feature `attrs-renditions`, the screen view's `attr` lines and the frame view's `cells` and `frame-cursor` lines print these keys where 1.0 reserves them, after `strike`: `overline` when the overline is set, then `superscript` or `subscript` when one of them is set. A cell holds at most one of the two. No case of 1.0 sets them, so no golden of 1.0 changes.

| Feature | What declaring it means |
|---|---|
| `attrs-renditions` | The screen and frame views print `overline`, `superscript` and `subscript`. |

## The worked examples' strings

The worked examples, here and in 1.0, read each `send` string with the string syntax of tier 1's scenarios (ADR-1504, D2), so the examples and the scenarios have one syntax. It adds three escapes to 1.0's list: `\a` stands for the byte 07, `\b` for the byte 08, and `\u{HEX}` for the UTF-8 of the code point HEX. Every string 1.0's list accepts reads as it did, so no example's input changes.

## The describe block and the grammar

The grammar below adds the forms of the `attr`, `cells` and `frame-cursor` lines that print the keys `attrs-renditions` prints, the clipboard's `test` line, the two forms of the `anim` line, and the display view's `display-cursor` line and structure. An `anim` line follows its image's `image` line, so `IMAGES` and the fragments and structures built on it are given again here: a fragment takes its value where it is defined, so the views' fragments and structures must be restated to use the new `IMAGES`. They differ from 1.0's only in that `IMAGES`.

```describe
interface 1.4
view display
feature clipboard
feature attrs-renditions
feature kitty-animation
feature bidi
command clipboard clipboard
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
