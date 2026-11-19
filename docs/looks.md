# Vtlook files

A vtlook (a look file) chooses the frame filters for a head. A frame filter changes the finished picture before the display shows it, such as a color correction or a CRT effect. This page describes the file: where it lives, its syntax, its display requirements, the built-in vtlook, and the errors vtium reports.

vtium has four color filters, which change the color of each pixel: `gamma`, `contrast`, `tint` and `color-blind`. Spatial filters, which read the pixels around each pixel, such as scanlines, are not built yet. See [the frame filters issue](https://github.com/vtium/vtium/issues/219) for the filters as they arrive.

## Where a vtlook lives

A vtlook is a file named `NAME.vtlook` in `/etc/vtium/looks`. The name has 1 to 64 of the characters `A-Z a-z 0-9 . _ + -`. vtium reads the file only when root owns it, and when neither its group nor other users can write it or a folder above it. vtium applies the same rule to vtium.conf.

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
| `tint` | `color`, `r,g,b`, default 255,176,0; `amount`, 0 to 1, default 1 | Moves each pixel toward its gray times `color`. At an amount of 1 the screen shows only shades of the color, like a single-color phosphor. |
| `color-blind` | `kind`, `protanopia`, `deuteranopia` or `tritanopia`, default `deuteranopia`; `mode`, `simulate` or `correct`, default `correct`; `amount`, 0 to 1, default 1 | `simulate` shows the screen as a viewer with that color vision deficiency sees it. `correct` moves the differences such a viewer cannot see into colors they can. `amount` mixes the result with the original. |

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

vtium checks each display when the display joins the head and again at each modeset that may change its size, refresh or depth. vtium checks again each time the head moves between the GPU path and the CPU path, and a display that meets the requirements again draws the vtlook again.

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

# [color color-blind]
# Simulates a color vision deficiency, or corrects for it.
# kind = deuteranopia # protanopia, deuteranopia or tritanopia: the deficiency
# mode = correct      # simulate or correct: show it, or correct for it
# amount = 1.0        # number, 0 to 1: how much of the result to show
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
/etc/vtium/looks/old.vtlook:2: color filter 1 of 1, sepia: checking: found sepia, needs gamma, contrast, tint or color-blind
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

A build of vtium without frame filters, such as the smallest build, reads no vtlook. For each head whose look names a vtlook, vtium logs one line and draws the built-in vtlook. `vtium check-config` reports the same for each vtlook that vtium.conf names.
