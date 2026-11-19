# Vtlook files

A vtlook (a look file) chooses the frame filters for a head. A frame filter changes the finished picture before the display shows it, such as a color correction or a CRT effect. This page describes the file: where it lives, its syntax, its display requirements, the built-in vtlook, and the errors vtium reports.

vtium has five color filters, which change the color of each pixel: `gamma`, `contrast`, `tint`, `color-blind`, and `cube`, which reads a color table from a file. Spatial filters, which read the pixels around each pixel, such as scanlines, are not built yet. See [the frame filters issue](https://github.com/vtium/vtium/issues/219) for the filters as they arrive.

## Where a vtlook lives

A vtlook is a file named `NAME.vtlook`. vtium looks for it first in `/etc/vtium/looks`, which holds your own vtlooks, and then in `/usr/share/vtium/looks`, which holds the [presets](#preset-vtlooks) that come with vtium. The name has 1 to 64 of the characters `A-Z a-z 0-9 . _ + -`. vtium reads the file only when root owns it, and when neither its group nor other users can write it or a folder above it. vtium applies the same rule to vtium.conf.

You choose a vtlook in vtium.conf. A `[look NAME]` section names its vtlook with `vtlook = NAME`. The top-level `vtlook` key sets the vtlook of every look that names none. With neither, a head draws the built-in vtlook, `default`.

```ini
# /etc/vtium/vtium.conf
vtlook = calibrated
version = 1

[look big]
font-size = 14
vtlook = amber
```

Here the heads that use the look `big` take the vtlook `amber`. Every other head takes `calibrated`.

## The syntax

A vtlook uses the syntax of vtium.conf's sections. A `#` starts a comment, each setting is a `key = value` line, and a section starts with a heading in brackets. The file starts with `version = 1`, before its first section.

```vtlook
# /etc/vtium/looks/desk.vtlook: for 24-bit displays of at least 1280x720.
version = 1

[requires]
min-depth = 24
min-size = 1280x720
```

A vtlook holds at most one `[requires]` section, and at most 32 sections in all. The file holds at most 64 KiB. Each other section names a filter as `[LAYER FILTER]`, where the layer is `color` or `spatial`. The lines of a filter's section set its parameters, and a parameter you leave out takes its default. A vtlook names each filter at most once.

## Color filters

A color filter changes each pixel's color using that color alone. vtium merges a vtlook's color filters into one color table, applying them in the order of their sections. Each pixel then costs one lookup in the table, however many filters the vtlook holds. The table reaches everything on the display: the text, the border, the mouse pointer and vtium's own notices. While a head's vtlook holds a filter, vtium draws the mouse pointer into the picture instead of on the display's cursor plane, so the filters change the pointer too.

| Filter | Parameters, with their defaults | What it does |
|---|---|---|
| `gamma` | `gamma`, 0.5 to 2, default 1 | Raises each channel to the power 1 / gamma. Above 1 it lightens the middle tones, and below 1 it darkens them. |
| `contrast` | `contrast`, 0 to 2, default 1; `brightness`, -0.5 to 0.5, default 0 | Multiplies each channel's distance from middle gray by `contrast`, then adds `brightness`. |
| `tint` | `color`, `r,g,b`, default 255,176,0; `amount`, 0 to 1, default 1; `floor`, 0 to 1, default 0 | Moves each pixel toward its gray times `color`. At an amount of 1 the screen shows only shades of the color, like a single-color phosphor. `floor` lifts every gray above black to at least the floor before the color applies, so dark colors stay visible. |
| `color-blind` | `kind`, `protanopia`, `deuteranopia` or `tritanopia`, default `deuteranopia`; `mode`, `simulate` or `correct`, default `correct`; `amount`, 0 to 1, default 1 | `simulate` shows the screen as a viewer with that color vision deficiency sees it. `correct` moves the differences such a viewer cannot see into colors they can. `amount` mixes the result with the original. |
| `cube` | `file`, the name of a `.cube` file in `/etc/vtium/looks`, with no path and no default | Maps each color through the table in the file. See [Color tables from .cube files](#color-tables-from-cube-files). |

This vtlook gives the screen the look of an amber monitor with a little more contrast:

```vtlook
# /etc/vtium/looks/amber.vtlook: an amber phosphor.
version = 1

[color tint]
color = 255,176,0
amount = 1.0

[color contrast]
contrast = 1.15
brightness = 0.0
```

Under a tint, each color shows only how light it is. Blue is the darkest of the three primaries, so blue text on a black background can be hard to see. The tint's `floor` lifts every color above black, while black stays black and white keeps the full color. Each gray rises to `floor + gray × (1 − floor)`, so the colors keep their order. With a floor of 0.3, the default palette's blue goes from about 27,19,0 to about 95,66,0 under amber:

```vtlook
# /etc/vtium/looks/amber-lifted.vtlook: an amber phosphor with dark colors lifted.
version = 1

[color tint]
color = 255,176,0
floor = 0.3
```

The floor lifts every color except exact black. Under a palette whose background is a dark gray, the background rises along with the dark text, so leave the floor at 0 there.

This one corrects the screen for a viewer with protanopia, and darkens the middle tones a little:

```vtlook
# /etc/vtium/looks/red-weak.vtlook
version = 1

[color color-blind]
kind = protanopia
mode = correct

[color gamma]
gamma = 0.9
```

The table holds 33 steps in each of red, green and blue, and vtium blends between them. A filter that bends hard near black, such as a gamma of 2, can be off by a few levels in the darkest tones.

## Color tables from .cube files

Color grading and calibration tools save their corrections as `.cube` files, in the format of Adobe's Cube LUT Specification 1.0. The `cube` filter applies such a file. Name the file in the filter's `file` parameter:

```vtlook
# /etc/vtium/looks/film.vtlook: a warm grade, then an amber phosphor.
version = 1

[color cube]
file = warm.cube

[color tint]
color = 255,176,0
```

The file lives in `/etc/vtium/looks`, beside your vtlooks, and holds at most 16 MiB. vtium uses a `.cube` file only while it is active, which means that `/etc/vtium/batch.conf` lists its full path. The file must also pass the rule that vtium applies to every file it uses as a resource. Root owns the file and each folder above it, and neither the group nor other users can write any of them. No symbolic link lies on the path, and the file sits on a filesystem that is mounted at boot and stays mounted. vtium treats a file that fails either check as missing, so the vtlook fails and its heads draw the built-in vtlook.

A `.cube` file is text. Keyword lines come first, and each keyword appears at most once:

- `TITLE "text"` names the table, and is optional.
- `LUT_3D_SIZE N` makes a 3D table of N entries a side, with N from 2 to 65.
- `LUT_1D_SIZE N` makes a 1D table of N entries, with N from 2 to 4096. A file holds exactly one of the two sizes.
- `DOMAIN_MIN 0 0 0` and `DOMAIN_MAX 1 1 1` are optional, and vtium reads them only at those values.

Data lines follow, each three numbers for red, green and blue, separated by spaces or tabs. A number is a decimal with an optional sign and an optional exponent, such as `0.5`, `-0.01` or `1e-3`. vtium treats a value below 0 as 0 and one above 1 as 1. A line that starts with `#` is a comment, and vtium skips blank lines. Lines end in LF or CRLF.

A 3D table of N a side has N × N × N data lines, one for each input color whose channels are whole steps of 1 / (N − 1). Red changes fastest, then green, then blue. The smallest identity table maps each corner of the color cube to itself:

```cube
# identity.cube: every color to itself.
TITLE "identity"
LUT_3D_SIZE 2
0 0 0
1 0 0
0 1 0
1 1 0
0 0 1
1 0 1
0 1 1
1 1 1
```

vtium blends the eight entries around each color. This warm table keeps red, lowers green by 5 per cent and blue by 15 per cent, so a mid gray of 0.5 becomes 0.5, 0.475 and 0.425:

```cube
# warm.cube: white becomes 1, 0.95, 0.85.
TITLE "warm"
LUT_3D_SIZE 2
0 0 0
1 0 0
0 0.95 0
1 0.95 0
0 0 0.85
1 0 0.85
0 0.95 0.85
1 0.95 0.85
```

A 1D table holds one curve for each channel, and data line i gives the outputs for an input of i / (N − 1). vtium blends between the two points around each channel's value. This table darkens the middle tones of every channel alike, much like a `gamma` of 0.8:

```cube
# darken.cube: each channel through the same curve.
TITLE "darken"
LUT_1D_SIZE 5
0 0 0
0.1768 0.1768 0.1768
0.4204 0.4204 0.4204
0.6986 0.6986 0.6986
1 1 1
```

vtium samples the file's table at the 33 steps of its own color table. A 3D table of 33 a side therefore passes into it unchanged, and a larger table is resampled to 33.

## Preset vtlooks

vtium comes with six vtlooks in `/usr/share/vtium/looks`. You can name one in a look section as it is, or copy one and change it:

| Preset | What it does |
|---|---|
| `amber` | Shows the screen in shades of amber, 255,176,0, with a contrast of 1.15, like an amber monochrome monitor. |
| `green` | Shows the screen in shades of green, 51,255,102, like a green phosphor monitor. |
| `white` | Shows the screen in shades of a slightly cool white, 230,235,255, like a paper-white monochrome monitor. |
| `protanopia` | Corrects the colors for a viewer with protanopia, who sees little red light. |
| `deuteranopia` | Corrects the colors for a viewer with deuteranopia, who sees little green light. |
| `tritanopia` | Corrects the colors for a viewer with tritanopia, who sees little blue light. |

Each phosphor preset leaves its `floor` at 0. To change a preset, copy it to `/etc/vtium/looks` under the same name and edit the copy. vtium then reads your copy instead of the preset. If your copy has an error, vtium reports it and the head draws the built-in vtlook, not the preset. Remove the copy and vtium reads the preset again.

## Display requirements

The `[requires]` section says which displays the vtlook suits. Every key takes `any`, which sets no requirement. A key you leave out is `any` as well.

| Key | Values | Met when |
|---|---|---|
| `min-depth`, `max-depth` | `any`, `1`, `8`, `16` or `24`; `32` reads as `24` | the display's bits a pixel lie within the pair |
| `min-size`, `max-size` | `any` or `WxH` | the display mode's own pixels lie within the pair on each side |
| `min-dpi`, `max-dpi` | `any` or a whole number from 1 to 1000 | the display's density lies within the pair |
| `min-fps` | `any`, `refresh` or a whole number from 1 to 1000 | the filters keep this many frames a second; `refresh` is the display's refresh rate |
| `gpu` | `any`, `yes` or `no` | `yes` needs the GPU path, and `no` needs the CPU path |

A minimum above its maximum is an error. A display that does not meet the requirements draws the built-in vtlook, and the head's other displays keep the vtlook. vtium logs one line that names the display and the first requirement it misses:

```text
vtlook amber: display DP-1 (1280x800, 1 bit) draws the built-in vtlook: requires min-depth 24
```

vtium checks each display when the display joins the head and again at each modeset that may change its size, refresh or depth. vtium checks again each time the head moves between the GPU path and the CPU path, and after you rotate the head with Super+= or Super+-. A display can report one density across and another down, and a turn changes which one `min-dpi` and `max-dpi` read. A display that meets the requirements again draws the vtlook again.

## The built-in vtlook

The built-in vtlook is named `default`. A head that names no vtlook draws it, and so does `vtlook = default`. The built-in vtlook changes no pixel, and no file can take its name. vtium never reads a file named `default.vtlook`, and `vtium check-config` reports one it finds in `/etc/vtium/looks` as an error.

vtium carries the built-in vtlook's text as a template. To start a vtlook of your own, copy the template under another name and set its requirements:

```vtlook
# default.vtlook: a pure passthrough vtlook, which changes no pixel.
# vtium's built-in vtlook: a look that names no vtlook draws it. To make a
# vtlook of your own, copy this file under another name, uncomment the
# sections you want and set their values. Within a layer, filters run in the
# order of their sections.
version = 1

# Display requirements. A display that misses one draws the default vtlook;
# the head's other displays keep this vtlook. any is no constraint.
[requires]
min-depth = any     # any, 1, 8, 16 or 24 (32 reads as 24): bits a pixel
max-depth = any
min-size = any      # any, or WxH: the mode's own pixels
max-size = any
min-dpi = any       # any, or 1 to 1000: the density fonts are sized at
max-dpi = any
min-fps = any       # any, refresh or 1 to 1000: the frame rate to keep
gpu = any           # any; yes for the GPU path only; no for the CPU path only

# Color filters run on both paths, merged into one table, after the turn.

# [color gamma]
# Raises each channel to the power 1 / gamma.
# gamma = 1.0       # number, 0.5 to 2: above 1 lightens the middle tones

# [color contrast]
# Scales each channel's distance from middle gray, then shifts it.
# contrast = 1.0    # number, 0 to 2: above 1 spreads the tones apart
# brightness = 0.0  # number, -0.5 to 0.5: added to each channel

# [color tint]
# Turns the screen toward one color, as a single-color phosphor.
# color = 255,176,0 # color, r,g,b: the phosphor's color
# amount = 1.0      # number, 0 to 1: how far toward it
# floor = 0.0       # number, 0 to 1: how far dark colors rise above black

# [color color-blind]
# Simulates a color vision deficiency, or corrects for it.
# kind = deuteranopia # protanopia, deuteranopia or tritanopia: the deficiency
# mode = correct      # simulate or correct: show it, or correct for it
# amount = 1.0        # number, 0 to 1: how much of the result to show

# [color cube]
# Maps each color through a table read from a .cube file.
# file =            # file, a .cube file, no path: the color table it reads
```

## Errors

Any error in a vtlook makes the whole vtlook fail. The heads that take it draw the built-in vtlook instead. vtium logs one line for each such head, naming the first error and counting the rest:

```text
vtlook old (/etc/vtium/looks/old.vtlook:2): reading: found [color sepia] before version = 1, needs version = 1 first; 1 more error; head console draws the built-in vtlook instead
```

vtium reads the vtlook again only when it reads its configuration again. `vtium check-config` lists the errors of every vtlook that vtium.conf names, one a line. Each line gives the file and line, the section, the stage and what vtium needs there. A vtlook can hold thousands of errors, so the check lists the first 64 and counts the rest. After a vtlook's errors, the check names each line of vtium.conf that chooses it. The examples below show a vtlook and the errors `vtium check-config` prints for it.

A missing version and a filter vtium does not have are two errors:

```vtlook-error
# /etc/vtium/looks/old.vtlook
[color sepia]
amount = 0.5
```

```text
/etc/vtium/looks/old.vtlook:2: reading: found [color sepia] before version = 1, needs version = 1 first
/etc/vtium/looks/old.vtlook:2: color filter 1 of 1, sepia: checking: found sepia, needs gamma, contrast, tint, color-blind or cube
```

A parameter's value outside its range:

```vtlook-error
# /etc/vtium/looks/soft.vtlook
version = 1

[color gamma]
gamma = 3
```

```text
/etc/vtium/looks/soft.vtlook:5: color filter 1 of 1, gamma: checking: gamma = 3, needs a number from 0.5 to 2
```

A requirement with a value the key does not take:

```vtlook-error
# /etc/vtium/looks/panel.vtlook
version = 1

[requires]
min-depth = 12
gpu = maybe
```

```text
/etc/vtium/looks/panel.vtlook:5: requires: checking: min-depth = 12, needs any, 1, 8, 16 or 24
/etc/vtium/looks/panel.vtlook:6: requires: checking: gpu = maybe, needs any, yes or no
```

A pair of requirements out of order:

```vtlook-error
# /etc/vtium/looks/big.vtlook
version = 1

[requires]
min-size = 1920x1080
max-size = 1280x800
```

```text
/etc/vtium/looks/big.vtlook:5: requires: checking: min-size = 1920x1080 exceeds max-size = 1280x800
```

vtium reads a `.cube` file when it builds the color table, and reports each fault in the file with the file's own line. The report names the vtlook's filter that reads the file. A table that does not match its size is one example:

```cube-error
# /etc/vtium/looks/panel.cube
TITLE "panel"
LUT_3D_SIZE 3
0 0 0
1 1 1
```

```text
5: found 2 data lines, needs 27, LUT_3D_SIZE 3 cubed
```

Under a vtlook `film` whose first color filter reads it, vtium logs that fault as:

```text
vtlook film (/etc/vtium/looks/panel.cube:5): color filter 1 of 2, cube: checking: found 2 data lines, needs 27, LUT_3D_SIZE 3 cubed; head console draws the built-in vtlook instead
```

A keyword vtium does not read, a size out of range, two sizes in one file, and a line that is not three numbers are faults too:

```cube-error
# /etc/vtium/looks/odd.cube
LUT_3D_SIZE 2
LUT_1D_SIZE 4
LUT_3D_INPUT_RANGE 0 1
0 0 0
0.5,0.2,0.1
```

```text
3: found LUT_1D_SIZE after LUT_3D_SIZE, needs one of the two
4: found LUT_3D_INPUT_RANGE, needs TITLE, LUT_3D_SIZE, LUT_1D_SIZE, DOMAIN_MIN or DOMAIN_MAX
6: found "0.5,0.2,0.1", needs three numbers separated by blanks
6: found 2 data lines, needs 8, LUT_3D_SIZE 2 cubed
```

A `.cube` file that is not active, fails the ownership rule or is larger than 16 MiB counts as missing. vtium reports it at the reading stage, with the check the file failed.

A build of vtium without frame filters, such as the smallest build, reads no vtlook. For each head whose look names a vtlook, vtium logs one line and draws the built-in vtlook. `vtium check-config` reports the same for each vtlook that vtium.conf names.
