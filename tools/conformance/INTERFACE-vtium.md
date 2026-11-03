# vtium's additions to the replay interface

Interface 1.1 is `corpus/INTERFACE.md`, interface 1.0 as the C reference left it, with the additions below. Each is a command, a feature or a line only a new command prints, so it raises MINOR under 1.0's "Versioning and features", and every golden stays as it is. The runner and the probe runner read the fenced blocks here after 1.0's: this `describe` block's version line replaces 1.0's, and its view, feature and command lines follow 1.0's lines of the same kind; this `grammar` block's lines join 1.0's, using its fragments.

## The fixture host checks a picture's size

vtium's fixture host answers a `png` request as 1.0's table says, with one check before it: a picture whose width or height passes the request's largest picture, which the terminal takes from `max-graphic-size`, is answered `TOO_LARGE`, and one whose width times its height times 4 passes the most bytes the request allows, which the terminal takes from its image budget, is answered `OVER_BUDGET`. A picture exactly at a bound is answered as before. The image helper reads a picture's header and makes the same check before it decodes (ADR-1100, D7), so the fixture host, standing in for it, checks what it checks. The C reference's host made no such check, and its goldens show the same answers, which its terminal reached by checking the decoded picture itself.

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

```describe
interface 1.1
feature clipboard
command clipboard clipboard
```

```grammar
line test * test clipboard (?:none|(?:[0-9a-f]{2})+)
line host: * host: [1-9][0-9]* t=d flags=any(?:,frames)? O=0 max=0 min=0 name= len={N}
```
