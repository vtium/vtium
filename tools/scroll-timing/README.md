# The scroll timing tool

The tool times three ways to scroll the CPU path's display, beside the redraw that vtium does today. ADR-1310 D6 asks for this measurement before smooth scrolling's CPU half is built, and the maintainer picks a way from its numbers. The tool also shows what the faster-scroll work would cost on a slow CPU, where a scroll at the bottom line redraws every row.

The tool runs on a KMS device and takes DRM master of the card. It drives the card through vtium-platform's drm2d where drm2d can do the work: its dumb buffers, its atomic commits and its flip events. drm2d has no pan yet, so the pan runs through a small stand-in of the tool's own, in `src/kms.rs`.

## Running it

```sh
make scroll-timing                                   # in tier 3's 1-vCPU guest, at 1920x1080 and 1366x768
make scroll-timing ARGS='--frames 60 --cells 8x16'   # the same with fewer frames and one cell size
make scroll-timing DEVICE=/dev/dri/card1             # on that card, at its preferred mode
target/scroll-timing-tool/release/scroll-timing memory --sizes 1920x1080,1366x768
```

`DEVICE=tier3`, the default, boots tier 3's guest twice on the minspec machine, with 1 vCPU and 512 MB. The first boot runs at 1920×1080 and the second at 1366×768, the minspec machine's size. Each boot runs the tool on the guest's virtio-gpu card and writes its rows to `target/tier3/scroll-timing/WxH.tsv`. `tools/tier3/t3_scrolltiming.py` drives the guest, and `tools/tier3/guest/scrolltiming` runs the tool there.

A card's path runs the tool on that card and writes `target/scroll-timing/cardN.tsv`. The tool takes DRM master, so run it as root from a text console that nothing else draws on. A display server on another VT lets go of the card while its VT is not shown. Never run it on a desktop that is showing: the tool cannot take the card there, and it fails at once. `ls -l /dev/dri/by-path` shows which card belongs to which device.

`memory` prints what each way needs and opens no device. `ARGS` passes these options to `run`:

| Option | What it sets |
|---|---|
| `--sizes preferred,1920x1080` | The display modes. `preferred` is each display's preferred mode, the default. |
| `--cells 8x16,16x32` | The character cell sizes, these two by default. |
| `--speeds 1,4,16` | The text rows the view scrolls back each frame: a slow, a medium and a fast scroll. `none` runs no speed in text rows. |
| `--pixel-speeds 1,2,4,8` | The pixel lines the view scrolls back each frame, as a smooth scroll moves. `none` runs no speed in pixel lines. |
| `--buffers 1,2,3` | The display buffers each way keeps, all three counts by default. |
| `--ways redraw,copy,move,pan,pan-ahead` | The ways to time, all five by default. |
| `--spare 0,screen` | The pan's spare rows beyond the grid's: none, and a screen of them, as ADR-1310 D6 proposes. |
| `--look random` | What the rows show: `random` shapes, the default, or `text`, which a person can follow by eye. |
| `--frames 120 --warmup 10` | The frames timed for each row, after frames drawn and not timed. |

## The ways

Each way draws the same view at each frame. The view scrolls back through a scrollback with no end, so new rows come in at the top. Each way's tests check every frame's picture against a redraw of the same view.

A speed in text rows moves the view by whole character cells, as vtium scrolls today. A speed in pixel lines moves it as a smooth scroll does, so the view often stands inside a row. The grid then shows the bottom part of its top row, the full rows below it, and the top part of one more row at the bottom. The redraw, the copy and both pans run at both kinds of speed. The move runs only at whole rows, because padnoir's figures at whole rows already rule it out. On padnoir's i915 the move reads rows back from write-combined memory, which takes 13 to 23 ms a frame.

- **redraw** draws every row of the grid at every frame, as vtium does today when the whole screen moves.
- **copy** keeps the grid's pixel lines in a ring in system memory, as wide as the screen. It draws each line into the ring once, and copies the visible window into the display's buffer at every frame. At a pixel step it draws only the lines that came into view, which can lie in two rows when the step crosses a row's edge.
- **move** moves the rows already in the display's buffer by the rows the view moved since that buffer was drawn, then draws the rows that came in. A buffer drm2d lends n frames old has missed n frames of scrolling. The move reads the buffer back, which is slow where the driver maps it write-combined.
- **pan** keeps the ring inside a display buffer taller than the screen, and the primary plane shows a window of it. A scroll sets the plane's `SRC_Y` at the next flip, so no pixel moves. Each frame draws only the rows that came in, into each buffer that lacks them. At a pixel step, `SRC_Y` points as many lines into the top row's slot as the view stands inside that row. A row is drawn whole when any of its lines comes into view.
- **pan-ahead** is the pan with rows drawn ahead of the view, as ADR-1310 D6 proposes. It draws as many rows as the view passes before the buffer is drawn again, plus the 4 rows of overscan, in the scroll's direction. A frame then writes rows only outside the window it shows and the window the flip before it showed. Where the ring has no room for the rows ahead, as with no spare rows, the frame draws the rows it lacks inside its window, as the pan does.

The pan's buffer holds the ring and, below it, a second drawing of the ring's first screen-height. A row whose slot lies in that first screen-height is drawn twice, so a window that starts anywhere in the ring shows its rows in order. With no spare rows, every row is drawn twice. At pixel steps the ring holds one row more than at whole rows, the row shown in part at the bottom. Without that row, the first and last rows on the screen would need the same slot. The pixel rows under the grid's last full row are the border's strip. The pan draws the strip at each window's bottom, over whatever slots lie there, and those slots are drawn again when a window shows them. At a pixel step the strip covers the lower lines of the row shown in part at the bottom. The pan keeps track of the lines above the strip, so it draws that row again only when a window shows more of it than survived.

With one buffer, the display scans out the buffer the pan draws into, so a write inside the window on screen shows up as a flash. On padnoir, the pan with one buffer showed two dark bands near the bottom, which moved up and apart as the speed rose. The border's strip makes the bands. Each frame draws the strip at the bottom of the window it is about to show. In the window on screen, that place lies one scroll step above the bottom. The tool used to draw a frame before the previous flip had landed, so the screen could show either of two windows. The strip then showed one step and two steps above the bottom: two bands. With no spare rows, the incoming rows' second drawings landed in the same bottom rows. With one buffer the pan now draws a frame only once the flip before it has landed. With drawing ahead and spare rows, no row is drawn inside the window on screen. The strip still is, wherever the screen's height is not a whole number of rows, as at 1920×1080. Only a plane that stops at the grid's bottom, or a second plane for the strip, would keep the strip out of the buffer.

The tool's rows stand in for text. Each character cell gets two colors and one of eight bitmap shapes, chosen from the row's id and the cell's column. A row then costs about what a row of text from a glyph store costs. The tool never runs the font helper, so the figures measure scrolling and not glyphs. The text look draws each row's number in digits, then the letters VTIUM repeated along the row. The run shifts by a cell from each row to the next, and the rows' backgrounds alternate. Its glyphs are bitmasks of the tool's own, drawn the same way as the random shapes, so the look changes what a row shows and not what drawing it costs.

## How it times a frame

A frame's CPU time is the thread's own CPU clock, from the moment the frame's buffer is taken to the moment its commit returns. A thread that waits for a flip uses no CPU, so the wait does not count. The draw time is the part inside the way's own drawing.

With one or three buffers, drm2d lends a buffer while a flip is in flight, and the swap keeps the frame until the flip completes. The tool then sends the kept frame with `commit`, as vtium does when the display reports the flip. The `busy` column counts those swaps. `refused` counts commits the driver refused, which should stay 0.

The frame rate is the frames shown a second, measured on the wall clock over the timed frames. It reaches the display's refresh rate where every frame fits in a refresh period. The figures in a guest count a vCPU that the host preempts as the thread's CPU time, so compare runs on a host doing nothing else.

## The results

Each run prints a header, then one row for each way, size, cell, speed, buffer count and spare rows. The header names the machine, the kernel, the driver, the card and the command. It also says whether the driver accepts the pan, through two test commits. One shows a tall buffer from its last screen, and the other shows the tallest buffer the driver reports (`max_height`).

| Column | What it holds |
|---|---|
| `rows_a_frame` | The speed, in the unit that `speed_unit` names. |
| `cpu_median_ms`, `cpu_p95_ms`, `cpu_max_ms` | A frame's CPU time: its median, its 95th percentile and the largest. |
| `draw_median_ms` | The median of the way's own drawing. |
| `fps`, `refresh_hz` | The frames shown a second, and the display's refresh rate. |
| `written_bytes`, `read_bytes` | The mean bytes a frame writes into the display's buffer and reads back from it. |
| `system_bytes` | The mean bytes a frame writes into memory of the way's own, the copy's ring. |
| `rows_drawn` | The mean text rows a frame draws from the pattern. |
| `display_memory`, `system_memory` | The display's buffers together, as the way makes them, and the memory the way keeps outside them, the copy's ring. |
| `added_memory` | What the way adds to today's buffers at the screen's size. |
| `commits`, `busy`, `refused` | The swaps or flips asked for, those held back behind a flip, and those the driver refused. |
| `speed_unit` | `rows` for a speed in text rows a frame, `pixels` for one in pixel lines a frame. Results files written before this column existed have no `speed_unit`, and their speeds are all in text rows. |

Two rows a cell, `row-display` and `row-system`, give the cost of drawing one text row into a display buffer and into system memory. The pan pays that cost for each row that comes in, twice where the row's slot has a second drawing.

`memory` gives the same column. Only the pan's memory differs between the units: at pixel steps each of its buffers is one text row taller.

The dated files in `results/` hold the runs reported to the maintainer. `2026-11-24-padnoir-look.tsv` holds what the maintainer saw on padnoir's display in each of the 24 whole-row runs, one way, buffer count and cell size each, in the maintainer's own words. `2026-11-24-padnoir-look-pixels.tsv` holds the same for the 12 runs at 1, 2, 4 and 8 pixel lines a frame with the text look, which add `pan-ahead` and the pan's spare rows.

## What it leaves out

- A display turned on its side, where the pan would move `SRC_X` over a buffer wider than the screen.
- fbdev's pan through `FBIOPAN_DISPLAY`.
- The pass through the color table and the depth reduction, which a look with color filters adds to every pixel the copy writes.
- The rows drawn ahead of a scroll through the scrollback. Drawing ahead changes when a row is drawn, not how many rows a frame draws on average.

## Its tests

`cargo test --manifest-path tools/scroll-timing/Cargo.toml` runs the tests, which need no device. They check that every way shows what a redraw shows, with one, two and three buffers, at several speeds, and across a scroll that turns. They check that the pan's window shows its rows in order at every start in the ring, across the ring's end too. Each frame of the pan draws only the row that came in. They also check the bytes each way writes and reads, and the memory figures at 1920×1080.

At pixel steps, the tests check that the redraw, the copy and the pan show what a redraw shows. The steps pass every offset inside a row, cross a row's edge, cross two edges at once, and run past each ring's end. The expected picture is built a line at a time from whole rows, so it does not depend on how the redraw splits a row. The pan's window is checked at every slot and every offset. Each pan frame draws only rows that the view shows, and a frame without a strip draws a row only when the view crosses into a new top row. The tests also check the bytes each way writes at a pixel step, and that each row of the results file ends with its speed's unit.

For drawing ahead, the tests record every write a pan frame makes. Once each buffer has been drawn, no frame of pan-ahead writes a row inside the window its flip shows or the window the flip before showed. That holds with one, two and three buffers, at row and pixel steps, scrolling back, and scrolling forward on a screen without a strip. Over a long scroll, pan-ahead draws no more rows than the pan, apart from the rows it holds ahead at the end. That holds while a buffer's view moves less than a screen between two of its frames. Every way also shows what a redraw shows with the text look.

The tool is not a gate: its numbers belong to the machine that runs it. It is a workspace of its own, with its own `Cargo.lock`, so the main workspace and CI do not build it.
