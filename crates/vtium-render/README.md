# vtium-render

vtium's renderer: frames from `vtium-term` in, display pixels out. This crate holds the CPU compositor (vtium/vtium#16), which ADR-1300 decides: a drawn-glyph cache, decorations as sprites, the cursor's shapes, the pointer, four orientations, damage handed to the display as clips, and the render timer. It forbids unsafe code and denies the determinism lints. It draws into anything that takes a run of pixels (`Target`), never reads it, and opens no device: the display backends are vtium/vtium#18, and the GPU path, behind a feature, is vtium/vtium#33.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation, its determinism lints and its public names. |
| `src/renderer.rs` | `Renderer`: what each frame redraws, the cell jobs, the row scratch, the cursor, the pointer and the damage rectangles. `Config`, `DrawState`, `Drawn` and `Stats`. |
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

A frame says which cells changed for this display. A cell that changed is drawn into each of the display's buffers once, as the next frames reach them (`Config::buffers`, two by default); a cell whose cursor, blink phase or wide pairing changed, or that the pointer covered, counts as changed. A frame that changes nothing writes nothing and returns no damage, and the display need not flip. A change of grid size, of the default background or a call to `redraw` draws everything into each buffer once, and fills the border around the centred grid.

## Departures

- **A full cache draws the glyph uncached.** A glyph that would evict one the frame draws, or that alone passes its pool's byte ceiling, is drawn from the font without being cached. The C reference drew such a cell blank and drew it again on the next frame (bbulk.c:1426-1432), and its GPU renderer drew blank past its hard stop (gltex.c:860-930). No specification covers a cache. The reason: a blank cell makes the cache change pixels (ADR-1300 D3). Test: `tests/cache.rs`, every frame at a cache of 1 and 7 glyphs against a cache of none.
- **A wide glyph cut at the margin shows its left half in every orientation.** On a display turned left or upside down, the C reference showed the glyph's right half in a wide cell's first column when the display held no second (aaronsb/kmscon#44, not fixed in the C fork). No specification covers turned displays. The reason: a turned display draws the unturned picture turned. Tests: `every_orientation_draws_the_same_picture_turned` and `a_wide_glyph_cut_at_the_margin_shows_its_left_half`.
- **A full redraw reaches each buffer once.** After a full redraw the C reference drew every cell a third time, and reported it as damage (aaronsb/kmscon#81, not fixed in the C fork). The reason: the second buffer already has the cells once the second frame is drawn. Test: `a_full_redraw_reaches_each_buffer_once`.
- **A keystroke damages only the cells it changes.** At 7680x2160 the C reference redrew about 70% of the screen at 20-28 flips a second while a prompt waited for typing, from a cause never found (aaronsb/kmscon#156, not fixed in the C fork). vtium's damage is the cells it draws, and a keystroke draws the cell it wrote and the cells the cursor left and entered. Test: `a_keystroke_damages_only_the_cells_it_changes`.

## Beyond the C reference

- **The cursor takes OSC 12's colour.** A block cursor's background, and an underline or bar cursor's line, take the colour OSC 12 set, and the character under a block keeps the cell's background as its colour; with none set, the cell is drawn inverted as the C reference drew it, which did not parse OSC 12 (aaronsb/libtsm#22). Test: `a_coloured_block_takes_its_colour`.
- **A wide half whose partner is missing draws as one cell.** The C reference's frame always paired them; vtium's frame does not promise it, so a leading half without its trailing half, or the reverse, draws in its own cell and no frame leaves a cell undrawn.
- **The two profiles, 1-bit and scale-1 storage, and the row scratch** are ADR-1300's D2, D4 and D5: memory and composition, with no pixel changed.

## Bounds

The frame comes from a program, the font from a file and the cell size from the configuration. Every glyph is bounded by the font's `CELL_SIDE_MAX`, a cluster by `vtium-term`'s `CLUSTER_MAX`, the cache by its budgets and the row scratch by its size.

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | Citation |
|---|---|---|---|---|
| `POOL_BYTES_PERFORMANCE` | 67108864 | Bytes of glyph pixels each of the four glyph pools holds in the performance profile; minspec's pools hold two screens of their format | Older glyphs are evicted; a glyph that would evict one the frame draws, or that alone passes it, is drawn uncached | gltex's per-pool budget, `GLTEX_CACHE_MAX_BYTES` (gltex.c:207-241); ADR-1300 D3 and D4 |
| `ROW_SCRATCH_MAX` | 1048576 | Bytes of the row scratch, where a row's span of cells of several layers is composed | The span is composed in bands of pixel lines that fit | ADR-1300 D5: one 16-pixel row across 1366 pixels is 87,424 bytes |
| `RENDER_TIMER_SAMPLES` | 1024 | Frames whose time the render timer keeps in an interval, for the median and the 95th percentile | Later frames count in the mean and the maximum only | `RENDER_TIMER_SAMPLES`, render_timer.h |
<!-- end of bounds table -->
