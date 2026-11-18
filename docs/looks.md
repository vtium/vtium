# Vtlook files

A vtlook (a look file) chooses the frame filters for a head. A frame filter changes the finished picture before the display shows it, such as a color correction or a CRT effect. This page describes the file: where it lives, its syntax, its display requirements, the built-in vtlook, and the errors vtium reports.

vtium has no frame filters yet, so a vtlook does not change the picture yet. You can already write a vtlook, name it in vtium.conf and check it. See [the frame filters issue](https://github.com/vtium/vtium/issues/219) for the filters as they arrive.

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

A vtlook holds at most one `[requires]` section, and at most 32 sections in all. The file holds at most 64 KiB. Each other section names a filter as `[LAYER FILTER]`, where the layer is `color` or `spatial`. Since vtium has no filters yet, a filter's section is an error for now.

## Display requirements

The `[requires]` section says which displays the vtlook suits. Every key takes `any`, which sets no requirement. A key you leave out is `any` as well.

| Key | Values | Met when |
|---|---|---|
| `min-depth`, `max-depth` | `any`, `1`, `8`, `16` or `24`; `32` reads as `24` | the display's bits a pixel lie within the pair |
| `min-size`, `max-size` | `any` or `WxH` | the display mode's own pixels lie within the pair on each side |
| `min-dpi`, `max-dpi` | `any` or a whole number from 1 to 1000 | the display's density lies within the pair |
| `min-fps` | `any`, `refresh` or a whole number from 1 to 1000 | the filters keep this many frames a second; `refresh` is the display's refresh rate |
| `gpu` | `any`, `yes` or `no` | `yes` needs the GPU path, and `no` needs the CPU path |

A minimum above its maximum is an error. vtium reads and checks the requirements now. Until vtium has filters, every display draws the built-in vtlook whatever the requirements say.

## The built-in vtlook

The built-in vtlook is named `default`. A head that names no vtlook draws it, and so does `vtlook = default`. The built-in vtlook changes no pixel, and no file can take its name. vtium refuses a file named `default.vtlook`.

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
```

## Errors

Any error in a vtlook makes the whole vtlook fail. The heads that take it draw the built-in vtlook instead. vtium logs one line for each such head, naming the first error and counting the rest:

```text
vtlook old (/etc/vtium/looks/old.vtlook:2): reading: found [color sepia] before version = 1, needs version = 1 first; 1 more error; head console draws the vtlook default (built in)
```

vtium reads the vtlook again only when it reads its configuration again. `vtium check-config` lists every error of every vtlook that vtium.conf names, one a line. Each line gives the file and line, the section, the stage and what vtium needs there. The examples below show a vtlook and the lines `vtium check-config` prints for it.

A missing version and a filter vtium does not have are two errors:

```vtlook-error
# /etc/vtium/looks/old.vtlook
[color sepia]
amount = 0.5
```

```text
/etc/vtium/looks/old.vtlook:2: reading: found [color sepia] before version = 1, needs version = 1 first
/etc/vtium/looks/old.vtlook:2: color filter 1 of 1, sepia: checking: found sepia, but this build of vtium has no color filters
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
