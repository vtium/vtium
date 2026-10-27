# vtium-render

vtium's renderer: frames from `vtium-term` in, display pixels out. This crate holds the CPU compositor (vtium/vtium#16), which ADR-1300 decides: a drawn-glyph cache, decorations as sprites, the cursor's shapes, the pointer, four orientations, damage handed to the display as clips, and the render timer. It forbids unsafe code and denies the determinism lints. It draws into anything that takes a run of pixels (`Target`), never reads it, and opens no device: the display backends are vtium/vtium#18, and the GPU path, behind a feature, is vtium/vtium#33.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation, its determinism lints and its public names. |
| `src/renderer.rs` | `Renderer`: the grid's place on the display, the cell jobs, the cursor and the pointer. `Config`, `DrawState`, `Drawn` and `Stats`. |
| `src/damage.rs` | What each buffer lacks, cell by cell, and the damage rectangles. |
| `src/compose.rs` | The row's jobs composed into display lines through the row scratch. |
| `src/cache.rs` | The drawn-glyph cache: four pools by format and width, keyed by the glyph's text, an LRU on the drawn frame, and the `Profile` budgets. |
| `src/stored.rs` | A glyph as the cache keeps it: turned once, at 1 bit when lossless, at the bitmap font's own size, and drawn a line at a time. |
| `src/deco.rs` | The underline and strike sprites. |
| `src/trig.rs` | `sin` and `cos` from basic float operations, for the curly underline. |
| `src/blend.rs` | The per-pixel arithmetic the C reference drew with. |
| `src/target.rs` | `Target`, the write-only display buffer; `Canvas` in memory; `Mapped`, a mapped dumb buffer; `Orientation` and `Rect`. |
| `src/timer.rs` | `RenderTimer` and its `Report`. |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its citation: the one source for the constants and for the table below. |
| `tests/` | The integration tests and the C reference's digests: see its README. |

`cargo test -p vtium-render` runs every test. The bounds table below is generated from `src/bounds.rs`, and `readme_table_matches_bounds` fails when it drifts; `VTIUM_RENDER_BLESS=1 cargo test -p vtium-render readme_table` rewrites it. `cargo test -p vtium-render --test memory -- --nocapture` prints ADR-1300's memory table from filled caches, and `cargo test --release -p vtium-render --test figures -- --nocapture` the render timer's figures at 1366x768.

## Behaviour

The C reference is the floor: its `bbulk.c`, `deco.c`, `drm2d_render.c` and `render_timer.c`. vtium keeps their flow and their per-pixel arithmetic, so that tier 2's bitmap-font goldens (vtium/vtium#17) judge it unchanged; `tests/reference.rs` and `src/deco.rs` hold it to digests of whole cells and of every sprite at 1,152 cell sizes, taken from the C reference's own code (`tests/reference/README.md`). ADR-1300 records the six ways the cache and composition differ from the C reference's, none of which moves a pixel but the blank cells of the first departure below.

`draw` follows the frame contract with the displays (vtium/vtium#18): it takes the buffer the display lends and the buffer's age, 1 when it holds the last frame drawn with damage, n when it holds the frame n back, 0 when its contents are unknown. Each cell carries the serial of the frame that last changed what it shows, its cursor (place, shape and colour), blink phase and wide pairing included, and a buffer at age n gets the cells that changed in the last n frames. At age 0, or at an age past the renderer's history, the buffer gets the whole picture and the border, and every other buffer's history stays as it was. Nothing assumes the buffers take turns or counts them, so a swap that failed, a third buffer and a buffer the display lost each get exactly what they lack. The frame is read since the checkpoint taken after the renderer's previous `draw`, whatever buffer that drew into.

Dim text (SGR 2) draws its foreground halfway to its background, each channel the sum of the two halved, as the C reference drew it. Dim mixes the colours reverse video left, and the cursor draws over the result: a block swaps the mix with the background. An underline or strike in the foreground's colour takes the mix; SGR 58's underline colour is exact. Test: `tests/dim.rs`.

The renderer draws the pointer only when `DrawState::pointer` is set, which the owner does on a display without a cursor plane; where the display drives a cursor plane, moving the pointer involves no draw. A drawn pointer changes the cells under it only when it appears, moves, hides or changes colour, and is drawn again over any of them the frame draws. A frame that changes nothing, pointer shown or not, writes nothing and returns no damage; the owner does not swap it, and it does not count as a frame. A change of grid size or of the default background changes every cell and the border.

## Departures

- **A full cache draws the glyph uncached.** A glyph that would evict one the frame draws, or that alone passes its pool's byte ceiling, is drawn from the font without being cached. The C reference drew such a cell blank and drew it again on the next frame (bbulk.c:1426-1432), and its GPU renderer drew blank past its hard stop (gltex.c:860-930). No specification covers a cache. The reason: a blank cell makes the cache change pixels (ADR-1300 D3). Test: `tests/cache.rs`, every frame at a cache of 1 and 7 glyphs against a cache of none.
- **A wide glyph cut at the margin shows its left half in every orientation.** On a display turned left or upside down, the C reference showed the glyph's right half in a wide cell's first column when the display held no second (aaronsb/kmscon#44, not fixed in the C fork). No specification covers turned displays. The reason: a turned display draws the unturned picture turned. That holds for the grid, not for the border around it: an odd remainder is split rounding down in the display's own coordinates in every orientation, as the C reference's goldens do, so with 5 pixels to spare a display turned upside down leaves 3 at the text's left where an unturned one leaves 2. Tests: `every_orientation_draws_the_same_picture_turned` and `a_wide_glyph_cut_at_the_margin_shows_its_left_half`.
- **A full redraw reaches each buffer once.** After a full redraw the C reference drew every cell a third time, and reported it as damage (aaronsb/kmscon#81, not fixed in the C fork). The reason: each buffer gets what its age says it lacks, and a buffer drawn whole lacks nothing. Tests: `a_full_redraw_reaches_each_buffer_once` and `buffers_at_age_0_are_each_drawn_whole_once`.
- **The pointer's damage covers every pixel it draws.** The C reference marked the two by two cells its centre fell among (bbulk.c:1583-1613), but its box, half a cell rounded up each way from a centre kept half a cell rounded down inside the grid, reaches a pixel past them at an odd cell size, and a pixel into the border; where no decorated cell widened the row, its moves left pixels behind. No specification covers a software pointer. vtium marks those cells and every cell the box covers, and redraws the border when the box reaches into it. Test: `the_pointer_leaves_nothing_behind_at_odd_cell_sizes`.
- **A keystroke damages only the cells it changes.** At 7680x2160 the C reference redrew about 70% of the screen at 20-28 flips a second while a prompt waited for typing, from a cause never found (aaronsb/kmscon#156, not fixed in the C fork). vtium's damage is the cells it draws, and a keystroke draws the cell it wrote and the cells the cursor left and entered. Test: `a_keystroke_damages_only_the_cells_it_changes`.

## Beyond the C reference

- **The cursor takes OSC 12's colour.** A block cursor's background, and an underline or bar cursor's line, take the colour OSC 12 set, and the character under a block keeps the cell's background as its colour; with none set, the cell is drawn inverted as the C reference drew it, which did not parse OSC 12 (aaronsb/libtsm#22). Test: `a_coloured_block_takes_its_colour`.
- **A wide half whose partner is missing draws as one cell.** The C reference's frame always paired them; vtium's frame does not promise it, so a leading half without its trailing half, or the reverse, draws in its own cell and no frame leaves a cell undrawn.
- **The two profiles, 1-bit and scale-1 storage, and the row scratch** are ADR-1300's D2, D4 and D5: memory and composition, with no pixel changed. Every run of a row's drawn cells is composed in the scratch and reaches the display a pixel line at a time, where D5 writes a cell of one layer straight, a call for each line of each cell: at 1366x768 a full frame's writes take 0.05 ms where the cell's took 0.48 ms (vtium/vtium#78).

## Not yet

- **Conceal and overline are not drawn.** A concealed cell shows its text, and an overline draws nothing. Both are vtium/vtium#37's: a concealed cell draws as its background, and an overline as a decoration beside the underline.
- **Rapid blink (SGR 6) blinks at SGR 5's rate.** ADR-1000's `sgr.rapid-blink` asks for a faster phase, which is vtium/vtium#37's; `tests/blink.rs` pins the shared phase until then.
- **`Frame::palette()` is owed.** The frame does not expose its palette, so the owner passes the default foreground and background in `DrawState`. The frame types the renderers read are vtium/vtium#10's.
- **A full frame takes about 0.5 ms.** At 1366x768 a full redraw with a warm cache measures 0.49 ms in a release build and 0.91 ms in the minspec build, on one pinned core (`tests/figures.rs`), against the 0.33 ms ADR-1300 measured for its draw kernel alone over the same screen in cached memory. That kernel, the 1-bit glyph loop and its writes, takes 0.18 ms of the release figure. The rest lies outside it: reading the frame a row at a time about 0.035 ms (a cell at a time, 0.07 ms), each cell's history 0.04 ms, building the cells' jobs 0.12 ms, the glyph cache's lookups 0.04 ms, setting up each cell's composition 0.05 ms and collecting the damage 0.03 ms. The minspec build's `opt-level = "z"` doubles the composition and the writes; with this crate at `opt-level = 3` the minspec build measured 0.58 ms before the frame was read a row at a time. Reaching ADR-1300's figure, or the operator revising it, is vtium/vtium#78's.

## Bounds

The frame comes from a program, the font from a file and the cell size from the configuration. Every glyph is bounded by the font's `CELL_SIDE_MAX`, a cluster by `vtium-term`'s `CLUSTER_MAX`, the cache by its budgets and the row scratch by its size.

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | Citation |
|---|---|---|---|---|
| `POOL_BYTES_PERFORMANCE` | 67108864 | Bytes of glyph pixels each of the four glyph pools holds in the performance profile; minspec's pools hold two screens of their format | Older glyphs are evicted; a glyph that would evict one the frame draws, or that alone passes it, is drawn uncached | gltex's per-pool budget, `GLTEX_CACHE_MAX_BYTES` (gltex.c:207-241); ADR-1300 D3 and D4 |
| `ROW_SCRATCH_MAX` | 1048576 | Bytes of the row scratch, where each run of a row's drawn cells is composed | The run is composed in bands of pixel lines that fit | ADR-1300 D5: one 16-pixel row across 1366 pixels is 87,424 bytes |
| `RENDER_TIMER_SAMPLES` | 1024 | Frames whose time the render timer keeps in an interval, for the median and the 95th percentile | Later frames count in the mean and the maximum only | `RENDER_TIMER_SAMPLES`, render_timer.h |
<!-- end of bounds table -->
