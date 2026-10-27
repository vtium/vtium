---
contract: adr/v1
kind: decision
verb: add
capability: renderer
basis:
  - operator: aaronsb
    level: guided
    said: "remember we need to store color and transparency (key) in glyphs. in the c implementation we kept four vector caches per layer (I think) we just need to find the blend of performance and memory consumption. on non minspec machines we might consider a more resource consuming model at the benefit of performance (and a build or execution option that still allows minspec profile pick)"
    via: operator, in session, relayed verbatim by the team lead in this record's brief
  - operator: aaronsb
    level: guided
    said: "we need to consider the gpu accel and cpu version of this too"
    via: operator, in session, relayed verbatim by the team lead in this record's brief
  - operator: aaronsb
    level: directed
    said: "Let's be sure of kmscon architecture first before we confidently build something novel"
    via: operator, in session, relayed verbatim by the team lead in this record's brief
  - operator: aaronsb
    level: directed
    said: "we are not recreating c in rust but instead the logic flow"
    via: operator, in session, relayed verbatim by the team lead in this record's brief
  - operator: aaronsb
    level: directed
    paraphrase: true
    said: "Agreed: a full cache draws the glyph uncached; two profiles chosen by a configuration key; a use clock per image store; the change of cache key needs explaining, saying what it differs in."
    via: operator, in session 2026-10-26, answering the flags of docs/c-reference.md; the team lead's summary, not the operator's words
  - operator: aaronsb
    level: guided
    said: "Need adr - how does this differ from reference?"
    via: operator, in session 2026-10-26, on the 1-bit and scale-1 storage and the row-scratch composition, relayed verbatim by the team lead
  - upstream: "aaronsb/kmscon b1f7a7c5: the glyph key (src/font/font.h:64-102), bbulk's one LRU, rotation on insert and per-cell drawing (src/render/bbulk.c:433, 529-572, 603-672, 1413-1475), gltex's four pools, bounds and blank draws (src/render/gltex.c:207-241, 860-930) and its float blend (src/render/gltex_atlas.frag:81-94), and the CPU blend arithmetic (src/video/drm2d_render.c:46-87)"
  - evidence: "the glyph storage research on vtium/vtium#16 (2026-10-26): draw kernels, dumb-buffer reads and writes, GL memory, miss cost, lookup cost and LRU hit rates, measured on a Ryzen 9 9950X3D on one pinned core, with radeonsi (integrated and discrete) and llvmpipe; the scripts and outputs sit in the steering session's scratchpad, uncommitted"
  - standard: "OpenGL ES 3.0.6 §2.1.6.2: converting a float to an unsigned normalized value returns one of the two nearest integers, rounding to nearest preferred"
  - precedent: ADR-1001
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Vtium approach for 2 and 5 sound superior to me"
    via: operator, in session 2026-10-26, after the explanation of how each differs from the C reference
    covers: [small storage, row scratch]
  - operator: aaronsb
    said: "3. Agree 4. Agree"
    via: operator, in session 2026-10-26, after the explanation of how each differs from the C reference
  - operator: aaronsb
    said: "Codepoint cache key as proposed sounds superior to the c implementation"
    via: operator, in session 2026-10-26, after the explanation of how it differs from the C reference
  - operator: aaronsb
    said: "Sorry agree on adding to the sub pixel thoughts and the adr I agree with too"
    via: operator, in session 2026-10-26, after reading the record and the subpixel proposal
observable:
  - "Tier 2 draws each screen with a cache of 0 glyphs and with the performance budgets, on the CPU path and on the GPU path under llvmpipe, and the two agree byte for byte"
  - "`cargo test -p vtium-render --test memory -- --nocapture` prints each profile's figures and fails when they change"
status: accepted
date: 2026-10-26
deciders:
  - aaronsb
related: []
---

# ADR-1300: The drawn-glyph cache and frame composition

## Summary

- **Decided:** vtium-render keeps the C reference's flow: the font draws a glyph once, the renderer caches it as a mask or a premultiplied colour glyph, and each cell is drawn without reading screen memory. Six differences: the cache keys on the glyph's text, not on a frame's cluster number; bitmap glyphs are stored at 1 bit and at their font's own size; a full cache draws the glyph uncached, never blank; two cache profiles, minspec and performance, chosen by a configuration key; cells with more than one layer are composed a row at a time in main memory and copied out; the GPU path keeps gltex's four pools and blends with the CPU path's integer arithmetic.
- **Trades away:** Two more drawing loops (1-bit, and scaled while drawing). A copy for each composed row. A frame drawn past a full cache costs a font call for each uncached glyph. The GPU shader does integer arithmetic in floats.
- **One-way?** No. Everything here is inside vtium-render, with no stored data and no protocol, and changes in a session.
- **Probes:**
  - *Not confident (small storage):* bitmap glyphs stored at 1 bit and at the font's own size hold a full minspec cache at 0.5 MiB instead of 4 MiB, and 32 times smaller at double size, for two more drawing loops; on ordinary screens both stay under 40 KB. Worth it, yes or no?
  - *Confident (row scratch):* cells with an underline, a picture or an overhanging glyph are built in a row buffer in main memory and copied to the screen, about 0.01 ms more for five such rows than drawing straight, so glyphs can cross cell borders without reading screen memory. Yes or no?
- **Inversion:** Between "port bbulk and gltex as they were" (one mixed LRU, 8-bit glyphs at their drawn size, a per-cell scratch) and "design a new renderer" (cached pre-composed cells, a GPU by default). The answer is the C reference's flow with six measured differences; the CPU path's pixels do not change, and the GPU path's move to match them.

## Context

vtium/vtium#16 builds vtium-render's CPU compositor, and vtium/vtium#33 its GPU path. The minspec machine (1366x768, 1 vCPU, 512 MB, docs/architecture.md) sets the budget: 8,160 cells of 8x16. The operator asked for glyphs that carry colour and transparency, a balance of speed and memory set by profile, a minspec profile any build can pick, both paths considered, and the C reference understood before anything new is built.

ADR-1001 changes what a cache can key on: clusters live in each line's arena, at byte offsets that change when a line compacts, and a frame carries a cluster's codepoints, not a screen-wide number.

The figures below are from the glyph storage research on vtium/vtium#16. Each is marked measured or estimate. Speed figures are from the x86-64 baseline build (SSE2), since `forbid(unsafe_code)` rules out choosing SIMD at run time; an x86-64-v3 build halves the 1-bit loop's time at 8x16 (0.158 ms against 0.334 ms a frame, measured).

## The C reference's flow

The font answered whether it had a glyph for a cluster in a style, and drew it 1 or 2 cells wide, as an 8-bit coverage mask or as premultiplied ARGB for colour (`kmscon/src/font/font.h:53-63`). Backends kept no cache; the renderer owned each glyph it was given.

A glyph was cached under `kmscon_glyph_key`: the frame's 64-bit cluster key, which carried a generation so it never named two clusters, plus bold, italic, the cell width and one byte of the renderer's own (`kmscon/src/font/font.h:64-102`).

bbulk, the CPU renderer, kept one LRU of glyphs, twice the cell count, masks and colour glyphs mixed, each rotated once when inserted (`kmscon/src/render/bbulk.c:433`, `kmscon/src/render/bbulk.c:529-572`).

bbulk drew each cell as one blend request into the mapped back buffer, the dumb buffer, computed from the request's colours, so device memory was never read. An undecorated cell was its mask lerped from background to foreground. A decorated cell, or one under a picture, was first composed in a scratch of at most two cells, its text layer premultiplied, and then blended over its background (`kmscon/src/render/bbulk.c:603-672`, `kmscon/src/render/bbulk.c:1413-1475`, `kmscon/src/video/drm2d_render.c:46-87`).

gltex, the GPU renderer, kept four pools: masks and colour glyphs, each 1 or 2 cells wide. Each pool was strips one cell high with an LRU of slots, full at 8192 glyphs or 64 MiB, and grew past full for a frame that needed more; past a hard stop of 512 MiB a display, glyphs drew blank (`kmscon/src/render/gltex.c:207-241`, `kmscon/src/render/gltex.c:860-930`). Its shader composed underline, glyph and strike over the background in floating point (`kmscon/src/render/gltex_atlas.frag:81-94`).

## Carried forward

- Glyphs are 8-bit coverage masks, tinted when drawn, or premultiplied RGBA colour glyphs, exactly 1 or 2 cells wide: the colour and transparency the operator asked for.
- Decorations are one-cell sprites, tinted when drawn and tiled across cells, never part of a glyph.
- The font keeps no cache, and the renderer owns what it draws.
- A glyph is rotated once, when cached. A change of font, cell size or orientation empties the cache.
- Each cache is an LRU whose recency is the drawn frame.
- A cell draws its background, underline, glyph and strike, in that order, with the C reference's per-pixel arithmetic: `div255(t) = (t + 128 + ((t + 128) >> 8)) >> 8`; a mask cell is `div255(fg * a + bg * (255 - a))`; a cell of several layers composes its text layer premultiplied, then goes over its background as `src + div255(bg * (255 - a))`. These are the arithmetic tier 2's goldens were drawn with.
- Device memory is never read.

## Decision

Each difference says what the C reference did, what vtium does, why, the cost or saving, and whether any pixel changes.

### D1: the cache key is the glyph's text

- **The C reference:** the frame's 64-bit cluster key, a number naming one cluster as created, plus bold, italic, width and a renderer byte.
- **vtium:** the cluster's codepoints, at most `CLUSTER_MAX` (32), plus bold, italic and the width in cells. A single codepoint is held in the entry; a longer cluster's codepoints are stored with it. No renderer byte, since nothing is baked into a glyph.
- **Why:** ADR-1001 has no screen-wide cluster number to key on, and its arena offsets are renumbered when a line compacts. The C reference's key also changed each time a cluster was re-created, so the same text was drawn and cached again under a new key while the old entry aged out (`kmscon/src/render/gltex.c:911-914`).
- **Cost or saving:** a cluster's lookup compares up to 32 codepoints. The table is open addressing with a fixed hash, splitmix64 as `kmscon_glyph_key_hash` used: 0.8-2.0 ns a lookup, against 4.6-6.2 ns for a `BTreeMap` (measured). A re-created cluster costs no miss, where the C reference's cost a font call, 212 ns at 8x16 (measured with unifont).
- **Pixels:** none change.

### D2: storage by format and width, at 1 bit when lossless, and at the font's own size

A third format is reserved (added 2026-10-26, the operator's agreement): a per-channel subpixel mask, three coverage values a pixel, for outline fonts (#23, #24). It takes its own pools beside the mask and colour pools, under the same budgets and the same lossless rule. Its order comes from the DRM connector's `subpixel` property, after rotation, with grayscale when the property reports none or unknown and a configuration key to override it. The GPU path blends it per channel in the shader. Bitmap fonts and colour glyphs never use it.


- **The C reference:** bbulk held every glyph 8-bit at its drawn size, in one LRU of both formats. gltex had pools by format and width.
- **vtium:** both paths keep gltex's four pools, masks and colour glyphs, 1 or 2 cells wide, each with an LRU, a glyph count and a byte ceiling (D4). The colour pools exist only while a colour face is loaded. A mask whose every pixel is 0 or 255 is stored at 1 bit a pixel; any other mask stays 8-bit. A bitmap font's glyph (PSF, unifont) is stored at the font's own size, bold included, and scaled by its whole factor while drawn, each source pixel a square.
- **Why:** bitmap glyphs have no partial pixels, and anti-aliased glyphs have 7-24% (measured), so the 1-bit test keeps exactly what is lossless. A pool's freed slot always fits the next glyph of its pool, and the same layout serves both paths.
- **Cost or saving:** an 8x16 mask is 16 bytes at 1 bit against 128 at 8; at scale 2 a stored glyph stays 16 bytes where the C reference held 512. A full minspec cache of a bitmap font holds 0.50 MiB of pixels against the C reference's 3.98 MiB, and 32 times less at scale 2 (Memory). Drawing a full frame costs about the same: at 8x16, 0.334 ms at 1 bit against 0.329 ms at 8 bits; an 8x16 font at scale 2, 0.264 ms scaled while drawing against 0.266 ms stored at 16x32 (all measured). A font drawn 16x32 at its own size is the slow case, 0.329 ms at 1 bit against 0.266 ms at 8 bits (measured). The costs are a 1-bit loop, a scaling loop, one scan of each new mask, and vtium-font handing the renderer a bitmap glyph at scale 1 with its scale.
- **Pixels:** none change. vtium-font scales a bitmap glyph by repeating each pixel into a square after bold, which is what drawing does; a mask stored at 1 bit reads back as the same 0 and 255.

### D3: a full cache draws the glyph uncached

- **The C reference:** bbulk drew a cell blank when its glyph could not be cached and tried again on the next frame (`kmscon/src/render/bbulk.c:1426-1432`). gltex drew blank past its hard stop.
- **vtium:** a pool is full for a glyph when taking it would evict a glyph the frame draws, or when the glyph alone passes the pool's byte ceiling. The glyph is then drawn without being cached: on the CPU path from one reused glyph buffer; on the GPU path from a slot held for that frame and given back after it, which is gltex's growth past full with the frame's cells as its bound.
- **Why:** a blank cell makes the cache change pixels, which the verification forbids. A glyph past the ceiling is real once vtium/vtium#60 lets a stream set the cell size: at 2048 pixels a side a wide mask is 8 MiB and a colour glyph 32 MiB.
- **Cost or saving:** a font call each frame for each uncached glyph: 212 ns at 8x16, 666 ns at 16x32, 2,435 ns at 32x64 (measured with unifont). A whole 8x16 screen of them is about 1.7 ms a frame (estimate).
- **Pixels:** the C reference's blank cells are drawn with their glyphs. No other pixel changes.

### D4: two profiles, chosen by a configuration key

- **The C reference:** one budget per renderer, fixed in the source: bbulk's twice the cells, gltex's 8192 glyphs and 64 MiB a pool, changed only by rebuilding with `-DGLTEX_CACHE_MAX_BYTES`.
- **vtium:** two profiles of budgets, with the same code:

| Budget | minspec | performance |
|---|---|---|
| Narrow pools, glyphs | twice the cells | four times the cells |
| Wide pools, glyphs | the cells | twice the cells |
| Byte ceiling, each pool | two screens of the pool's format at the display's size | 64 MiB, gltex's |

- The `glyph-cache` key in `kmscon.conf`, and `--glyph-cache`, pick `minspec` or `performance` on any build; vtium/vtium#21 wires them. The default build defaults to `minspec`, and a build with the Cargo feature `performance-cache` defaults to `performance`.
- **Why:** the operator asked for a profile that spends memory on a larger machine and a way for any build to pick minspec. At minspec's counts a pool holds two screens of glyphs, so its byte ceiling only binds when the cell size grows past the font's.
- **Cost or saving:** on five recorded workloads (bat, git log, grep, ls, markdown) an LRU of 128 glyphs hit 99.76-99.98% (measured), so ordinary screens never fill either profile. The performance profile pays off on screens of many distinct glyphs, CJK or emoji, and costs twice minspec's memory (Memory).
- **Pixels:** none change; tier 2 draws both profiles.

### D5: cells of several layers are composed a row at a time

- **The C reference:** every cell went to the dumb buffer as one write-only blend; a cell of several layers was composed in a two-cell scratch first.
- **vtium:** a cell drawn from its background and at most one glyph is written straight into the dumb buffer, as the C reference wrote it. Where a row's damaged cells include any with more layers (a decoration, a picture, a cursor drawn over text, or a neighbour's overhang, vtium/vtium#26), the span of those cells is composed in a row scratch in cached memory and copied out, one copy per pixel line. The scratch holds one row of cells at the display's width, 87,424 bytes at 1366 pixels and 16-pixel rows, and at most 1 MiB; a taller row is composed in bands of pixel lines. On a rotated display the row is the strip of pixels its cells cover.
- **Why:** an overhanging glyph and a picture not aligned to cells draw across cell borders, and per-cell requests cannot give one cell's layers the pixels of its neighbour without reading the dumb buffer. Reading it is the slow path: 0.16 GB/s on an integrated GPU and 0.02 GB/s on a discrete one, against 87 GB/s from cache (measured).
- **Cost or saving:** five changed rows composed in the scratch and copied out take 0.039-0.044 ms, against 0.029-0.030 ms written straight (measured). A full frame blended by reading the dumb buffer took 8.5 ms on the integrated GPU and 36.6 ms on the discrete one, against 0.32-0.33 ms in cached memory (measured). A full frame written straight takes 0.26-0.27 ms (measured).
- **Pixels:** none change. The scratch runs the per-pixel arithmetic carried forward, cell by cell, and copies the result.

### D6: the GPU path keeps gltex's pools and draws the CPU path's pixels

- **The C reference:** gltex's four pools of one-cell strips, blended in floating point, so its pixels could differ from bbulk's by rounding (not measured).
- **vtium:** the four pools of D2 under D4's budgets, each pool's slots laid out as a grid in its textures, 8-bit for masks and RGBA8 for colour, uploaded a slot at a time. The fragment shader composes the cell as the CPU path does, with its integer arithmetic: values held as whole numbers in `highp` floats, which hold 255 × 255 exactly; `div255` with its shifts written as floors of divisions by 256; nearest sampling; no dithering; each channel `c` written as `(c + 0.25) / 255`. A GPU without `highp` in fragment shaders draws on the CPU path.
- **Why:** OpenGL ES lets the conversion to an 8-bit channel return either of the two nearest integers; at `c + 0.25` both rounding to nearest and truncating give `c`. A grid uses a texture's height, where a one-cell strip left texture unused at its end and, at large cells, most of its height (`kmscon/src/render/gltex.c:885-896`).
- **Cost or saving:** the GPU path is a feature for larger machines: its driver, context and a 1366x768 render target took about 72 MiB on a GPU sharing system memory, and four atlases of 10 MiB added about 34 MiB more (measured, radeonsi); a slot upload took 0.4-2.7 µs (measured). The integer shader's cost against gltex's is not measured.
- **Pixels:** the GPU path's pixels move from gltex's to the CPU path's. Parity is derived from the specification and not yet seen on hardware. A picture scaled with filtering cannot match and is outside it.

### Memory

Computed for 1366x768 in 8x16 cells, the pixels each cache can hold at its budget, the C reference's at its worst, every glyph wide:

| | C reference, bbulk | vtium minspec | vtium performance |
|---|---|---|---|
| Glyphs held | 16,320 | 16,320 narrow, 8,160 wide | 32,640 narrow, 16,320 wide |
| A bitmap font | 3.98 MiB | 0.50 MiB | 1.00 MiB |
| A bitmap font at scale 2, in 16x32 cells, a quarter the glyphs | 3.98 MiB | 0.12 MiB | 0.25 MiB |
| An anti-aliased font | 3.98 MiB | 3.98 MiB | 7.97 MiB |

Each entry's key, links and slot add about 24 bytes (estimate). The row scratch is 87,424 bytes. `crates/vtium-render/tests/memory.rs` computes these from the structures and pins them, as ADR-1001's test does; tier 3 measures the real figures from the first working build.

## Verification

- **The cache changes speed, never pixels.** Tier 2 (vtium/vtium#17) draws each screen with a cache of 0 glyphs and with the performance budgets, and compares them byte for byte, on the CPU path and on the GPU path under llvmpipe, which runs headless. It compares 1-bit storage with 8-bit, and scale-1 storage with storage at the drawn size, the same way. A flood of distinct glyphs past every budget draws every glyph, against the same screen drawn uncached. A key that drops bold, planted, fails on a screen holding a letter in bold and regular.
- **The GPU path draws the CPU path's pixels:** the same screens on both, byte for byte, pictures scaled with filtering excepted.
- **The bitmap-font screens** match the C reference's goldens pixel for pixel (vtium/vtium#17).
- **No order from a hash.** The cache's table uses a fixed hash, and nothing iterates it to draw (docs/architecture.md, "Determinism").
- **Eviction follows the bytes.** The glyph cache's recency follows drawn frames, which the invariance test keeps from reaching a pixel. The image store's use clock is the store's own and moves only on stream events, so drawing a picture never renews its use.

## Consequences

### Positive

- A bitmap font's cache costs an eighth of the C reference's at the font's size, and a thirty-second at double size.
- No cell draws blank, whatever the cache holds.
- Glyphs and pictures can cross cell borders without reading device memory.
- One layout and one set of budgets serve both paths, and the GPU path is judged against the CPU path.
- A larger machine spends more memory on glyphs with a key, and any build can run the minspec profile.

### Negative

- Three mask loops where the C reference had one: 8-bit, 1-bit, and 1-bit scaled.
- A row that holds a decorated cell pays a copy, 0.002-0.003 ms a row more than drawing straight.
- A bitmap font of 16x32 pixels at its own size draws 0.06 ms a full frame slower at 1 bit than at 8 bits on the baseline build.
- A screen of glyphs past a full cache calls the font for each one, every frame.
- The integer shader's cost on slow GPUs is unknown until #33 measures it.

### Neutral

- vtium-font hands the renderer a bitmap glyph at scale 1 with its scale (vtium/vtium#15's crate).
- vtium/vtium#21 wires `glyph-cache`, and vtium/vtium#33 builds the GPU path on D6.
- Tier 2 runs every screen twice per path, at a cache of 0 and at the performance budgets.

## Alternatives Considered

- **bbulk's one LRU of both formats, 8-bit at the drawn size.** Rejected: at scale 2 it holds 32 times the bytes of D2 for the same glyphs, and a freed entry need not fit the next glyph.
- **Pre-composed cells in the performance profile,** decorated and rotated glyphs held ready to copy, as the operator's first direction on #16 listed. Not taken: copying a pre-composed 8x16 cell saves at most 0.14 ms a full frame against the 1-bit loop (0.196 ms against 0.334 ms, measured), for 32 times the memory a cell. The profiles differ in budgets only.
- **Blending by reading the dumb buffer.** Rejected: 8.5-36.6 ms a frame (measured).
- **A whole-frame shadow buffer in cached memory,** copied out where damaged. Rejected for minspec: 4.2 MB where the row scratch is 87 KB, and every damaged pixel copied, 0.14-0.19 ms for a full frame (measured), where write-only cells need no copy.
- **gltex's blank draw past a hard stop.** Rejected: it makes the cache change pixels (D3).
- **A `BTreeMap` for lookup.** Rejected: 4.6-6.2 ns against 0.8-2.0 ns (measured). std's `HashMap` is denied (docs/architecture.md, "Determinism").
- **A profile chosen by the machine's memory,** as ADR-1001 caps the scrollback. Not taken: the operator chose a key, and the default build stays minspec.
