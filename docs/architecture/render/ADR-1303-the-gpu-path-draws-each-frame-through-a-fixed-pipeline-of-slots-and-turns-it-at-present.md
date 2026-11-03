---
contract: adr/v1
kind: decision
verb: add
capability: renderer
amends:
  - 'ADR-1401#D2: one process, one VT, every card'
  - ADR-1300#Carried forward
  - ADR-1300#Alternatives Considered
basis:
  - operator: aaronsb
    level: directed
    said: "Each stage of the pipeline is a slot. A slot holds a working step or a no-op, and the pipeline is fixed, so later work fills a slot without changing the frame's structure. With every optional slot empty, the frame passes straight through at almost no cost."
    via: "the GitHub issue for the GPU path, comment of 2026-11-03 headed \"Decided 2026-11-03 (maintainer): the GPU path draws each frame through a pipeline of slots, and the frame is turned in the present stage.\"; posted from the maintainer's account; who wrote the wording is not recorded"
  - operator: aaronsb
    level: directed
    said: "Slice 0 reserves the slots with their no-ops and builds the two picture steps. Each other step that fills a slot comes later, in its own issue."
    via: the same comment
  - operator: aaronsb
    level: directed
    said: "**Present.** The frame is turned for a turned display and copied to the screen. Every stage before it works on the upright frame, so the frame is turned in one place only, which removes the kind of offset [the C fork's issue on a turned grid one pixel off] was. On the GPU path, the color and spatial steps that are switched on are folded into this one last pass, so empty slots add no pass."
    via: the same comment, item 5; the issue number replaced by a phrase in brackets, as AGENTS.md asks of a record
  - operator: aaronsb
    level: directed
    said: "*GPU scaling, the default.* The GPU uploads each picture's original pixels once and scales them itself. Filtering a picture shrunk past 2:1 ([the C fork's issue on pictures shrunk past 2:1]) belongs to this step, and tests compare its output with the CPU path's within a stated tolerance."
    via: the same comment, item 1; the issue number replaced by a phrase in brackets
  - operator: aaronsb
    level: directed
    said: "*The CPU copy.* The GPU uploads the CPU path's scaled copy and draws it pixel for pixel, so it matches the CPU path byte for byte. Tests use this step to check the rest of the frame exactly."
    via: the same comment, item 1
  - operator: aaronsb
    level: directed
    said: "**Color.** Gamma, color space mapping, display compensation, contrast and color-blind filters. Each pixel's new color depends only on its old color, so redrawing only what changed still works. The CPU path reserves the same slot and fills it with a lookup table."
    via: the same comment, item 3
  - operator: aaronsb
    level: directed
    said: "**Spatial effects.** A CRT look, scanlines and sharpening ([the issue for an optional frame shader]), on the GPU path only. A step here that moves pixels or reads the time makes vtium redraw the whole frame."
    via: the same comment, item 4; the issue number replaced by a phrase in brackets
  - operator: aaronsb
    level: directed
    said: "the frame shader fills a slot in the GPU path's pipeline, which [the GPU path issue]'s comment of today describes. The pipeline reserves two slots after composition: a color slot (gamma, color space mapping, display compensation, contrast, color-blind filters) and a spatial slot (a CRT look, scanlines, sharpening). Both hold a no-op unless the configuration fills them. The color slot also exists on the CPU path, as a lookup table, so color correction does not need a GPU. The slots are reserved first, and their steps come one at a time. This issue's ADR defines what fills each slot: how a shader is named, its inputs, its cost bound, and the fall-back to the no-op when it fails."
    via: "the GitHub issue for an optional frame shader on the GPU path, comment of 2026-11-03 headed \"Decided 2026-11-03 (maintainer)\"; posted from the maintainer's account; who wrote the wording is not recorded; the issue number replaced by a phrase in brackets"
  - operator: aaronsb
    level: guided
    said: "looking forward to the hardware renderer. Build it from the kmscon fork's logic (its gltex renderer at b1f7a7c5) as the reference, never its C structure; the libraries differ (glow over EGL and GBM here). Tier 3's GL variant fails until this lands, by decision."
    via: "the GitHub issue for the GPU path, comment headed \"Operator (2026-10-27)\"; posted from the maintainer's account; who wrote the wording is not recorded"
  - upstream: "aaronsb/kmscon b1f7a7c5: drm3d's set-up, a GBM device on the card, an EGL display on it, EGL_KHR_surfaceless_context required, an OpenGL ES 2 context made current with no surface, GL_EXT_unpack_subimage looked for (src/video/drm3d_video.c:480-646); a GBM surface of XRGB8888 and an EGL window surface for each display (src/video/drm3d_video.c:150-233); a swap through eglSwapBuffers and the surface's front buffer object (src/video/drm3d_video.c:382-424); hwaccel off by default (src/config.c:1413); drm3d tried when hwaccel was on and drm2d on the same descriptor when it failed (src/seat.c:960-1008); gltex on a display with GL and bbulk on any other, and no text renderer at all when one could not be made (src/terminal.c:1252-1260); the log line naming the text renderer (src/terminal.c:1272-1275); gltex's shaders compiled at set, its atlases capped at 2048 pixels a side and an image's texture at GL_MAX_TEXTURE_SIZE (src/render/gltex.c:503-608); highp in the fragment shader where the GPU had it and mediump where it did not (src/render/gltex.c:457-474); a turn drawn by rotating every quad in the vertex shader (src/render/gltex.c:419-425, src/render/gltex_atlas.vert), with the turned grid's margins computed in the display's axes and swapped into the rotated axes as fractions of the screen (src/render/gltex.c:427-448); a turn that dropped and rebuilt all GL state (src/render/gltex.c:1316-1322), and a set that failed retried each frame while nothing drew (src/render/gltex.c:1324-1350); an image's texture scaled down to fit GL_MAX_TEXTURE_SIZE, or made at the drawn size when the whole image did not fit the textures' budget (src/render/gltex.c:1679-1700, 1880-1890); pictures sampled nearest where a texel met a pixel and bilinear where scaled (src/render/gltex.c:2262-2302); the whole screen cleared every frame (src/render/gltex.c:1372-1373) and redrawn, and a GL error logged once per frame, after its draws, and the frame reported failed (src/render/gltex.c:2337-2396)"
  - evidence: "the C fork's issue on pictures shrunk past 2:1, closed: gltex sampled a scaled picture bilinear with no mipmaps where bbulk shrank it with a box filter; at 1.6:1 on tier 3's kitty screens the two differed by up to 41 levels in a channel, and past 2:1 gltex aliased; the issue listed mipmaps, a box filter in the shader, or a box-filtered upload from the CPU as the ways out"
  - evidence: "the C fork's issue on a turned grid one pixel off, closed: at 1280x800 turned right, gltex drew the whole grid one pixel right of bbulk's, and the two captures agreed exactly once shifted; it said the offset likely came from the two renderers computing a turned display's margin differently, not investigated further. A comment of 2026-10-25, posted from the maintainer's account, said the GL golden `sixel-evict-rotated` carries the offset, 11 440 pixels differing from the 2D golden and 270 at 1 level once shifted, and that a fixed GL golden \"should then match within 1 level\" of the 2D golden"
  - evidence: "the gap check of this record's agent against integration at 6d706694, 2026-11-03, listed under The starting state, each line checked against the file it names; and the CPU path as built there: the grid placed in the display's own axes with an odd margin's spare pixel on the right or bottom (crates/vtium-render/src/renderer.rs:205-225, `Layout::grid_for`), the border filled straight into the target (renderer.rs:737-763), the pointer blended on the way out by `Over` and the depth reduced last by `Threshold` (renderer.rs:664-665), the console's notices drawn over the buffer afterwards through their own `Threshold` (crates/vtium-render/src/notice.rs), and the pointer's arrow reaching past the grid into the border (crates/vtium-render/README.md, Departures)"
  - evidence: "crates.io, read 2026-11-03: glow 0.18.0 (2026-07-09, MIT OR Apache-2.0 OR Zlib, no native dependency on Linux); khronos-egl 6.0.0 (2023-10-05, libEGL linked through pkg-config or loaded at run time through libloading, its `static` and `dynamic` features); gbm 0.18.0 over gbm-sys 0.4.0 (libgbm linked, an optional `drm` feature at drm ^0.14); glutin 0.32.3; wgpu 30.0.1"
  - standard: "The OpenGL ES Shading Language 1.00 makes highp in fragment shaders optional, signalled by GL_FRAGMENT_PRECISION_HIGH; version 3.00 requires it. OpenGL ES 3.0 adds single-channel 8-bit textures (R8) and texelFetch, which reads one texel by integer coordinates with no filtering"
  - standard: "EGL_EXT_image_dma_buf_import and GL_OES_EGL_image: a buffer shared as a dma-buf is made an EGL image and bound as the storage of a GL renderbuffer, which a framebuffer object draws into"
  - precedent: ADR-1300
  - precedent: ADR-1401
  - precedent: ADR-1100
  - precedent: ADR-1102
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-03
deciders:
  - aaronsb
related: []
---

# ADR-1303: The GPU path draws each frame through a fixed pipeline of slots and turns it at present

## Summary

- **Decided by the maintainer (2026-11-03):** vtium's GPU path draws every frame through the same five stages, called slots: pictures at another size, composition into an upright frame off screen, color, spatial effects, and present, which turns the frame for a turned display and copies it to the screen. A slot that has nothing to do holds a no-op. The color and spatial steps that are switched on fold into the present pass, so an empty slot adds no pass. Pictures drawn at another size are scaled on the GPU by default, judged against the CPU path within a stated tolerance, and can instead be drawn from the CPU path's scaled copy, byte for byte. The CPU path reserves the color slot too, as a lookup table. The first work, slice 0, reserves the slots with their no-ops and builds both picture steps; each later step comes in its own issue. GL is reached through glow over EGL and GBM.
- **Proposed by this record's agent, for the maintainer to confirm:** the upright frame holds the grid alone, and present places it at the CPU path's offsets, so the turned grid lands on the CPU path's pixels (D3, D6); the pointer and the console's notices are drawn at present, after the spatial step and before color, as the CPU path draws them (D6); the CPU path's color slot sits on its last write before the depth reduction (D4); GL calls go through a safe `Gl` trait, with all unsafe code in vtium-platform (D7); khronos-egl and gbm beside glow (D8); a head draws on the GPU path only in a `gpu` build with `hwaccel` on, falls back to the CPU path at start, and sorts failures during a run into kinds, from a staging copy or an uncached draw up to the CPU path (D9); the log names the path that draws (D10). Nine open questions are left to the maintainer, each with a recommendation.
- **Trades away:** The GPU path keeps the grid's pixels for each display in GPU memory, at most the display's size: 7.9 MiB at 1920x1080 and 63.3 MiB at 7680x2160. ADR-1300 and ADR-1401 D2 had it keep no whole frame. One more pass a frame, the present copy, which the C reference's gltex did not have.
- **One-way?** No. Everything is behind the `gpu` feature, with no stored data and no protocol. It is reversible now, and costs days of rework once the composition and picture slices are built on the upright frame.
- **Probes:**
  - *Confident (the off-screen frame):* the GPU path keeps one picture of the screen's text in the graphics card's memory, about 8 MB at 1920x1080 and 66 MB at 7680x2160, so turning, color and effects happen in one place and the turned screen sits on exactly the pixels the software path draws. Earlier records said neither path keeps such a copy; this changes that for the GPU path only, and the software path still keeps none. Is that what you meant by the off-screen frame?
  - *Not confident (giving up on the GPU):* when the GPU path fails badly partway through a session, its context lost or its errors repeating, vtium moves that head to software drawing for the rest of the run and says so once in the log. A smaller failure, such as the card running out of room for a picture, only slows that picture down. Is giving up for the run what you want, or should vtium try the GPU again, for example after the next VT switch?
- **Inversion:** Between "port gltex as it was" (every quad turned in the vertex shader and drawn straight into the window surface, the screen cleared and redrawn each frame, pictures sampled bilinear at any scale) and "a general GPU compositor" (a render graph, any number of passes, a portable API such as wgpu). The answer is a fixed pipeline of five slots on OpenGL ES, with the turn in one place and the CPU path as the judge of every pixel.

## Context

vtium's GPU path takes the place of the C reference's GPU renderer, gltex, which drew through GL on the C reference's drm3d display backend, and goes past it. gltex is the record of how the C reference set up EGL and GBM, what failed and how it fell back; it is not a limit on the pipeline, and no slot is narrowed to fit what gltex did. ADR-1300 D6 already decides how the GPU path stores glyphs and blends a cell: gltex's four pools, under the CPU path's budgets, and a shader that does the CPU path's integer arithmetic so both paths draw the same bytes. ADR-1401 D2 decides how a frame reaches a display: the display lends its buffer to the head's render thread as a token, and on the GPU path the thread imports that buffer as an EGL image and draws into it.

The maintainer decided on 2026-11-03 how a frame flows through the GPU path, as a pipeline of slots, so that later work, such as color correction or a CRT look, fills a slot without changing the frame's structure. The frame shader's record, which waits on this one, defines what fills the color and spatial slots. This record writes the pipeline down with each slot's inputs and its no-op, and proposes what the pipeline needs around it: where unsafe GL code lives, which crates bind EGL and GBM, how vtium chooses between the CPU and GPU paths, and what vtium calls each path in its log. Nothing of the GPU path is built yet, and the section on the starting state lists what the code holds today.

A slot is one stage of the pipeline. A step is the work that fills a slot. A no-op is the step a slot holds when it has nothing to do: the frame leaves the slot as it came in. The upright frame is the grid as the terminal lays it out, its rows running across, before any turn for a display mounted on its side or upside down. The border is the strip of display pixels around the grid that no cell covers.

## C notes: the C reference's flow

drm3d, the C reference's GPU display backend, set up GL once for each card. It made a GBM device on the card's descriptor, an EGL display on that device, and required `EGL_KHR_surfaceless_context`. It bound OpenGL ES, chose a config whose native format was XRGB8888 or ARGB8888, made an OpenGL ES 2 context and made it current with no surface, and noted whether `GL_EXT_unpack_subimage` was there for faster uploads (`kmscon/src/video/drm3d_video.c:480-646`). For each display it made a GBM surface of XRGB8888 at the mode's size and an EGL window surface on it (`kmscon/src/video/drm3d_video.c:150-233`). A swap called `eglSwapBuffers`, locked the surface's front buffer object, made a framebuffer of it the first time it saw it, and presented it through the DRM code all backends shared (`kmscon/src/video/drm3d_video.c:382-424`).

The C reference chose its path in two places. The seat tried drm3d for a card when `hwaccel` was on, which it was not by default, and fell back to drm2d on the same descriptor when drm3d failed (`kmscon/src/config.c:1413`, `kmscon/src/seat.c:960-1008`). The terminal then chose gltex for a display with GL and bbulk for any other; when the text renderer could not be made, the display got no screen at all (`kmscon/src/terminal.c:1252-1260`). It logged the choice as `Display ... with backend [...] text renderer [...]` (`kmscon/src/terminal.c:1272-1275`).

gltex compiled its shaders when it was set up, capped its glyph atlases at 2048 pixels a side and an image's texture at `GL_MAX_TEXTURE_SIZE` (`kmscon/src/render/gltex.c:503-608`). It asked for `highp` floats in the fragment shader where the GPU had them and took `mediump` where it did not (`kmscon/src/render/gltex.c:457-474`). It drew a turned display by rotating every quad in its vertex shader (`kmscon/src/render/gltex.c:419-425`, `kmscon/src/render/gltex_atlas.vert`). It computed a turned grid's margins in the display's axes and swapped them into the rotated axes as fractions of the screen (`kmscon/src/render/gltex.c:427-448`), where bbulk placed the grid in the display's own pixels. At 1280x800 turned right, gltex's whole grid sat one pixel right of bbulk's. The C fork's issue on a turned grid one pixel off recorded the offset and named the two margins as its likely cause; the code above confirms that the two placed a turned grid by different arithmetic. A turn dropped all of gltex's GL state and built it again (`kmscon/src/render/gltex.c:1316-1322`); a set-up that failed was tried again every frame, and nothing drew meanwhile (`kmscon/src/render/gltex.c:1324-1350`). A set-up that failed during a turn left a quiet screen frozen until some other redraw came, as the C fork's issue on a failed set during a rotation recorded.

gltex uploaded each picture as a texture of its own: the whole image, scaled down to fit `GL_MAX_TEXTURE_SIZE`, or, when the whole image did not fit the textures' budget, the image at the size the placement drew it (`kmscon/src/render/gltex.c:1679-1700`, `kmscon/src/render/gltex.c:1880-1890`). It sampled a picture nearest where one texel met one pixel and bilinear where the picture was scaled (`kmscon/src/render/gltex.c:2262-2302`). bbulk shrank a picture with a box filter, which averages every source pixel a drawn pixel covers. Bilinear sampling reads only the four texels nearest a pixel, so gltex's picture shrunk past 2:1 skipped texels and aliased, and even at 1.6:1 it differed from bbulk's by up to 41 levels in a channel, as the C fork's issue on pictures shrunk past 2:1 recorded.

Each frame gltex cleared the screen (`kmscon/src/render/gltex.c:1372-1373`) and drew everything in passes: pictures below the backgrounds, the backgrounds, pictures below the text, the text, pictures above the text, and the pointer. It checked for GL errors once, after the frame's draws; an error was logged and the frame reported as failed, and the next frame drew as usual (`kmscon/src/render/gltex.c:2337-2396`).

## Decision

D1 to D6 write down the maintainer's pipeline; where they go past what the maintainer said, the text says the detail is proposed. D7 to D10 are this record's agent's proposals. D11 sets out the work.

### D1: a fixed pipeline of five slots

Every frame the GPU path draws passes through five slots, in this order. The order and the number never change; a configuration or a later step changes only what a slot holds.

```mermaid
flowchart LR
    P["1. Pictures at another size<br>GPU scaling (default)<br>or the CPU copy"]:::pic
    C["2. Composition<br>cells, glyphs, pictures, cursor<br>into the upright frame"]:::comp
    S["4. Spatial<br>no-op until a step fills it"]:::opt
    O["Present: place and turn the frame,<br>fill the border, draw the pointer<br>and the notices"]:::pres
    K["3. Color<br>no-op until a step fills it"]:::opt
    W["the display's buffer"]:::buf
    P --> C --> S --> O --> K --> W
    subgraph fold ["5. Present: one pass on the GPU"]
        S
        O
        K
    end
    classDef pic fill:#2d7d9a,color:#ffffff,stroke:#4a5568
    classDef comp fill:#7c3aed,color:#ffffff,stroke:#4a5568
    classDef opt fill:#475569,color:#ffffff,stroke:#94a3b8,stroke-dasharray: 4 3
    classDef pres fill:#2d8e5e,color:#ffffff,stroke:#4a5568
    classDef buf fill:#fbbf24,color:#1a1a1a,stroke:#4a5568
    style fold stroke:#d97706,fill:#f6821f1a
```

The figure shows the slots left to right, from the picture textures to the display's buffer. Slot 5, present, is the box: one pass on the GPU that runs the spatial step on the upright frame, places and turns the frame, fills the border, draws the pointer and the notices, and then runs the color step on every pixel it writes. Slots 3 and 4, dashed, hold no-ops until a later step fills them. Inside the pass the color step comes last, so it reaches the border, the pointer and the notices as the CPU path's does (D4); the slots keep the maintainer's numbers.

| Slot | Its inputs | What it gives the next slot | Its no-op | Paths | Damage |
|---|---|---|---|---|---|
| 1. Pictures at another size | A placement's picture, named by its image key, frame and pixel revision (ADR-1102 D3); the part of it the placement shows; the size it is drawn at | A texture the composition samples at the drawn size | A picture drawn at its own size is sampled as it is, one texel a pixel. In a build without `images` the slot is empty | GPU (the CPU path scales in its compositor, ADR-1100 D5) | None of its own: the cells a picture covers are already damaged when it changes |
| 2. Composition | The frame (cells, styles, the frame's placements in their order, the cursor), the glyph pools and decoration sprites of ADR-1300 D6, slot 1's textures | The upright frame: a texture the size of the grid in RGBA8, at most the display's size | None: composition always runs, and draws only the cells changed since it last ran | GPU | Draws the cells changed since the upright frame was last drawn |
| 3. Color | One pixel's color and the step's parameters | The same pixel, recolored | The identity, which the present pass leaves out | GPU, folded into present; CPU, a lookup table on the renderer's last write before the depth reduction (D4) | None: each pixel's new color depends only on its old one |
| 4. Spatial | The upright frame, its size, and the time for a step that reads it | One pixel for each pixel of the upright frame | The identity, which the present pass leaves out | GPU only, folded into present | A step whose output pixel reads any other pixel, or reads the time, makes every present a whole one |
| 5. Present | The upright frame, the display's size, orientation and depth, the border's color, the pointer, the notices, the enabled color and spatial steps, the buffer the token names, its age, and the rectangles changed over that age | The display's buffer | None: present always runs; with slots 3 and 4 empty and the display upright it is a copy | GPU | Draws the rectangles changed in the last `age` frames, and the whole buffer at age 0 |

### D2: slot 1, pictures at another size

Two steps can fill slot 1, and they give the composition the same thing: a texture it samples at the drawn size.

- **GPU scaling, the default.** The GPU path uploads each picture's own pixels once, as a texture keyed by the image key, the frame and the pixel revision, and scales it as it draws. The step owns the filtering of a picture shrunk past 2:1, so the picture does not alias the way gltex's did, and tests compare its output with the CPU path's within a stated tolerance (open question 3 sets the figure). The maintainer fixed the result and left the method open. This record proposes that the step compute the CPU path's own filter weights in the shader: the box filter where a picture shrinks and the bilinear blend where it grows (`crates/vtium-render/src/picture/scale.rs`).
- **The CPU copy.** The GPU path asks vtium-render's scaled-copy cache for the CPU path's copy of the picture, upright, at the drawn size (`crates/vtium-render/src/picture/copies.rs`, with `turn` 0), uploads it, and the composition samples it one texel a pixel. The frame then matches the CPU path byte for byte. Tests select this step to check every other part of the frame exactly.

The renderer's configuration chooses the step, GPU scaling unless a test asks for the CPU copy, and nothing else in the frame changes with it; no key in vtium.conf sets it. Each step keeps its textures in a cache of the renderer's own, outside the image store's budget, so no texture ever evicts a stored picture, as ADR-1100 D5 decides for the CPU path's copies. Open question 4 decides how large the picture textures may grow and what a picture too large for a texture does; the recommendation is ADR-1100 D5's bound for each display, with the CPU copy for a picture past it.

### D3: slot 2, composition into the upright frame

The composition draws the grid into an off-screen texture, the upright frame, in the order and with the arithmetic ADR-1300 decides: each cell's background, underline, glyph and strike, the frame's placements in the bands and order of ADR-1100 D4, and the cursor. Each display the GPU path draws keeps its own upright frame, since its age, its damage and its size are its own.

This record proposes that the upright frame hold the grid alone: its columns times the cell's width, by its rows times the cell's height, at most the display's size, 7.9 MiB at 1920x1080 and 63.3 MiB at 7680x2160. The border, the pointer and the notices are drawn at present, in the display's own pixels (D6). Open question 9 asks the maintainer to confirm this against a frame the size of the whole display.

The upright frame outlives a frame, so the composition draws only the cells that changed since it last drew, whatever the age of the display's buffer. A new upright frame, after a display is set up, a font or size change, or a wake-up, is drawn whole.

Glyphs on the GPU path are stored upright and never turned, because the composition works on the upright frame. A turn of the display changes the present pass alone and keeps every glyph and picture texture. ADR-1300's "Carried forward" says a glyph is rotated once when cached, and a change of orientation empties the cache; on the GPU path the orientation half of that no longer holds, and the CPU path keeps it.

### D4: slot 3, color

The color slot holds a step that maps each pixel's color to a new one: gamma, color space mapping, display compensation, contrast or a color-blind filter. The frame shader's record defines what fills the color and spatial slots: how a step is named in the configuration, its inputs, its cost bound and how it falls back to the no-op when it fails, as the maintainer's decision on the frame shader asks. Each step then comes in an issue of its own. Because a color step reads only the pixel it writes, damage tracking holds on both paths.

On the GPU path the step folds into the present pass and runs on every pixel present writes. On the CPU path the slot sits on the renderer's last write to the display's buffer before the depth reduction: between `Over`, which blends the pointer on the way out, and `Threshold`, which reduces a pixel to 1 bit for a 1-bit panel (`crates/vtium-render/src/renderer.rs:664-665`). So the border, which the renderer fills straight into the target, the cells and the pointer all pass through it. The console's notices, which reach the buffer through their own `Threshold` (`crates/vtium-render/src/notice.rs`), pass through the same slot there. This record proposes that a color step both paths run use one table, which the GPU samples as a texture, so the paths stay byte for byte equal with a color step on; the frame shader's record decides whether a step that cannot be a table may exist, and how it is judged.

### D5: slot 4, spatial effects

The spatial slot holds a step that reads the upright frame around a pixel or reads the time: a CRT look, scanlines or sharpening. It exists on the GPU path only, and the frame shader's record defines what fills it, as for the color slot. A step that reads any pixel other than the one it writes, or reads the time, makes every present a whole one; the composition still draws only what changed. A step that reads the time also needs frames drawn on a clock, which vtium does not draw today, and a new place where time enters (docs/architecture.md, "Determinism"); that step's record names both, and never lets the time reach the screen's state.

The spatial step runs on the upright frame, the grid alone (D3), before present draws the border, the pointer and the notices. A step that bends the picture, such as a CRT's curvature, therefore leaves the pointer where it is while the cell under it moves, and a pointer on the display's cursor plane (`crates/vtium-platform/src/display/drm2d.rs`) skips the color and spatial steps altogether. The frame shader's record decides how a spatial step treats the pointer, the border and the notices.

### D6: slot 5, present

Present places the upright frame on the display, turns it, and draws it into the buffer the display lent. It is the only place on the GPU path that knows the display's orientation. On the GPU it is one draw over the rectangles to be drawn, and for each pixel of the display's buffer, in order:

1. The pixel takes the border's color, or, where the grid covers it, its texel of the upright frame, read by integer coordinates with no filtering, after the spatial step. The grid sits where the CPU path puts it: centred in the display's own pixels, with an odd margin's spare pixel on the right or the bottom (`Layout::grid_for`, `crates/vtium-render/src/renderer.rs:205-225`). Centring the grid in the upright frame and turning that would put the spare pixel on the other side, which is the C fork's one-pixel offset again: at 1280x800 turned right with a 10x19 cell, 67 rows take 1273 pixels across and leave 7, and the CPU path puts 3 on the left where a turned centred frame puts 4.
2. The pointer's arrow is blended over it, at the pixel the CPU path's `Over` puts it, reaching past the grid into the border as it does there.
3. The console's notices and the focus mark are drawn over that, as the console draws them over the CPU path's buffer.
4. The color step runs.
5. The display's depth reduction runs (ADR-1301), as the CPU path's `Threshold` does, and each channel `c` is written as `(c + 0.25) / 255`, as ADR-1300 D6 writes a cell, so a pixel present only copies arrives exact.

The enabled steps are compiled into the present shader when the configuration changes, never per frame, so an empty slot adds no pass and no instruction. Present draws the rectangles that changed in the last `age` frames of the buffer the token names, and the whole buffer at age 0 or when a spatial step asks for it. The rectangles it drew are the frame's damage, which the loop hands to the display at the swap (ADR-1401 D2).

The CPU path keeps its own shape: it composes each pixel already turned, through the row scratch, into the display's buffer (ADR-1300 D5), and has no separate present step. The two paths agree because present places, turns and draws each pixel as the CPU path does, which tier 2 checks by comparing the GPU path's buffer with the CPU path's at each orientation (Verification).

### D7: the safe `Gl` trait, and where unsafe code lives (proposed)

Only vtium-platform may use `unsafe` (docs/architecture.md). vtium-render keeps `#![forbid(unsafe_code)]` and draws through a safe trait, `Gl`, which it defines behind its `gpu` feature. vtium-platform wraps GL, EGL and GBM in a safe type of its own behind its `gpu` feature, holding every `unsafe` site. Open question 10 decides what joins the two: vtium-platform implementing `Gl` with vtium-render as a dependency, or an adapter in the binary that implements `Gl` over vtium-platform's safe type, which keeps docs/architecture.md's arrows pointing one way; the recommendation is the adapter. The minspec build links neither and no GL library.

The trait speaks vtium-render's terms, not GL's: create, fill and free a texture of a stated format and size; the upright frame and an imported display buffer as targets to draw into; a program compiled from vtium-render's own shader source, with named inputs; a draw of a batch of quads; the GPU's limits, such as the largest texture and whether fragment shaders have `highp`; and a read of a rectangle back to memory, for tests. Every handle the trait gives out is an owned value tied to the context that made it, and a call with a handle that is freed or belongs to another context returns an error, so nothing the renderer does through the trait is undefined behaviour. GL errors are checked once for each batch of draws and at the end of each frame, as gltex checked them once a frame, or reported as they happen through `KHR_debug` where the driver has it; a check after every call would make a threaded driver wait on the GPU. An error is reported for the frame, which D9 then sorts. The GLSL source lives in vtium-render beside the CPU path's arithmetic it mirrors.

vtium-platform's GL type holds the EGL display, the context and the GL function table on the head's render thread, which owns the context for as long as it runs. Each `unsafe` site carries an `#[expect]` with its reason, as the crate's rule asks, and its README lists them.

A head has one GL context (ADR-1401 D2), made on the render node of the card that drives the head's first display, the first in the order the head file's rules light them, so it needs no DRM master. Each display allocates its buffers as GBM buffer objects on its own card (ADR-1401 D2), and the token carries each buffer as a dma-buf: its descriptor, format, modifier, stride and offset. The render thread imports it as an EGL image and binds it as the storage of a renderbuffer, attached to a framebuffer object that present draws into. A display on another card is imported the same way where the driver allows it, and otherwise reached through a staging copy, frame by frame, as ADR-1401 D2 decides.

A stand-in GL in vtium-render's tests implements the trait and checks what vtium-platform's GL type checks: each handle is live and its own, a texture's format and size are within the limits it reports, an upload lies inside its texture, a program's inputs exist. It records the calls and draws no pixels. Pixels are judged on a real GL, Mesa's llvmpipe, which runs headless on EGL's surfaceless platform with no device. That platform has no GBM device and no dma-buf, so tier 2 sets up the GPU path without GBM, presents into an off-screen texture and reads it back; only tier 3 proves the dma-buf import. Every machine that runs tier 2, CI's runners included, `arm-native` among them, needs Mesa's llvmpipe and its EGL.

### D8: the binding crates (proposed)

vtium binds GL with glow, as the maintainer named, and EGL with khronos-egl, inside vtium-platform under `gpu`. glow is maintained (0.18.0, 2026-07), has no native dependency on Linux and loads its function table through `eglGetProcAddress`. khronos-egl (6.0.0) either links libEGL or loads it at run time. Open question 5 decides how vtium reaches GBM; the recommendation is to load libEGL and libgbm when the GPU path starts, with a small loader of libgbm's few calls in place of the gbm crate. If the libraries are linked instead, the gbm crate, which Smithay maintains, binds GBM without its `drm` feature, so it adds no second copy of the DRM bindings beside vtium's `drm-ffi`.

The GPU path targets OpenGL ES, as gltex did. Open question 1 decides the version; the recommendation is 3.0, which D6's integer texel reads and ADR-1300 D6's single-channel glyph masks assume.

### D9: choosing the path, at start and on failure (proposed)

A head draws on the GPU path when the build has the `gpu` feature and `hwaccel` is on. Otherwise it draws on the CPU path, and `vtium check-config` says so when `hwaccel` is set in a build without `gpu`. Open question 2 decides `hwaccel`'s default in a `gpu` build; the recommendation is off, as the C reference's was, until tier 3's GL variant passes on vtium.

At start, the head's render thread sets up the GPU path in order: the render node, the GBM device, the EGL display and its extensions, the context and its version, `highp` in fragment shaders (ADR-1300 D6), and the shaders. The first step that fails ends the attempt. vtium logs the step that failed, and the head draws on the CPU path, its displays taking dumb buffers as they do today. Where the C reference left a display with no screen when gltex could not be made, vtium always draws.

During a run, vtium sorts a GPU failure by its kind, and only the last kind leaves the GPU path:

- **A buffer the render thread cannot import.** The frame reaches that display through a staging copy (ADR-1401 D2).
- **A refused allocation:** a texture, an upload or a pool's growth the driver refuses for want of memory. The glyph is drawn uncached from a slot held for that frame, as ADR-1300 D3 draws a glyph past a full cache, and the picture is drawn from the CPU copy, as ADR-1100 D5's cache draws a copy that does not fit. The cache changes speed and never pixels, and a pool refused growth grows once its budget allows.
- **A GL error in a frame.** The frame is dropped, as a token that came back too late, and the next frame draws whole, as gltex treated an error as one failed frame.
- **A lost context, or a GL error in two frames running.** The head moves to the CPU path: its displays take dumb buffers through a modeset, and the next frame draws whole. vtium logs it once. Open question 6 decides whether vtium tries the GPU again later; the recommendation is not within the run.

A turn of the display runs no set-up on the GPU path: it changes the present pass and keeps every texture (D3). That meets the lesson of the C fork's issue on a failed set during a rotation, where a set-up run by a turn failed and left the screen frozen.

### D10: the renderer's name in the log (proposed)

The line vtium logs for each display it draws names the path that draws it, which the head knows once it has chosen. vtium logs the line again when a head moves to the CPU path. Today the line names `bbulk` whatever happens (`crates/vtium/src/app.rs:1195`). Open question 7 decides the names; the recommendation is vtium's own, `cpu` and `gpu`. Any later report of a display's state, such as a status command, uses the same name.

### D11: slice 0, and its parts

The maintainer named the GPU path's first work slice 0: it builds the pipeline with every optional slot holding its no-op, and both picture steps. Each step that later fills the color or spatial slot comes in an issue of its own, against the frame shader's record (D4, D5).

Slice 0 is built in seven parts. Parts 1 to 6 build and test on their own, and part 7 waits for the work that builds outline-font text and re-baselines its goldens. The content of parts 1 to 6 is this record's agent's proposal; the maintainer and the steering session set the final order.

1. **The trait and the skeleton.** vtium-render's `gpu` feature, the `Gl` trait, the stand-in GL, and the five slots wired with their no-ops.
2. **EGL and GBM.** vtium-platform's `gpu` feature and its safe GL type: the render node, GBM, EGL, the context, glow, the GPU kind of buffer token and the dma-buf import; and what joins the type to the trait (open question 10).
3. **Composition and present.** The glyph pools as textures and ADR-1300 D6's shader drawing into the upright frame; present at the upright orientation with the border, the pointer and the notices; a glyph the font cannot draw not asked for again every frame; a pool refused growth growing once its budget does. Tier 2 draws its screens on both paths under llvmpipe and compares them byte for byte.
4. **The turn.** Present at all four orientations, the grid at the CPU path's offsets, an odd margin among the screens. A turn keeps every texture and runs no set-up.
5. **Pictures.** Both picture steps, the CPU copy matched byte for byte, GPU scaling within the tolerance at every scale, a picture shrunk past 2:1 among them.
6. **Choosing the path.** `hwaccel` acted on, the fall-back at start, the kinds of failure during a run, the staging copy, and the renderer's name in the log.
7. **Tier 3's GL variant.** The `gl-1280x800` variant passes on vtium under virgl. It waits for the outline-font work because its goldens are outline-font text: the C reference drew them with gltex and the pango engine in DejaVu Sans Mono at 16 (`tools/tier3/guest/kmscon-gl.conf:4-6`, the goldens copied from the C fork's `fork/tier3/golden/gl-1280x800/` at b1f7a7c5 in `tools/tier3/provenance.tsv`), and vtium's text screens fail the C reference's outline-font goldens on the font until that work re-baselines them (`tools/tier3/README.md`, the paragraph on `make tier3`). Open question 8 decides which goldens then judge the GL variant.

## The starting state

This record's agent checked each line against integration at 6d706694 on 2026-11-03.

- No crate has a `gpu` feature. vtium-render's features are `performance-cache` and `images`; vtium-platform's are `logind` and `images`; the binary's are `images` and `logind`.
- No manifest and no line of `Cargo.lock` names glow, an EGL binding or a GBM binding.
- vtium-render forbids unsafe code (`crates/vtium-render/src/lib.rs:1`), and keeps doing so under D7.
- The scaled-copy cache's key names a picture's pixels by its image, its frame and its pixel revision (`crates/vtium-render/src/picture/copies.rs:21-31`), as ADR-1102 D3 decides. The gap check before this record found the key at an older path without the frame and the pixel revision; commit 51a4d284 closed that gap, and the GPU path's textures key on the same three values.
- The log line for each display hard-codes `text renderer [bbulk]` (`crates/vtium/src/app.rs:1195`).
- The `hwaccel` key exists, off by default, among the keys not built yet (`crates/vtium/src/config/keys.rs:565-574`, `crates/vtium/src/config/keys.rs:1510`), and `vtium check-config` warns "hwaccel is not built yet; vtium draws in software" (`crates/vtium/src/notice.rs:180`).
- The display's buffer token has its CPU kind only; its comment says the GPU kind, carrying the GBM buffer object, is not built (`crates/vtium-platform/src/display/token.rs:17-19`). D7 proposes a dma-buf in its place.
- Tier 3's `gl-1280x800` variant expects the log to name `gltex` and fails on vtium today (`tools/tier3/t3_scenario.py:171-180`).

## Where vtium departs from the C reference

The pipeline goes past gltex in each item below. The first four have no counterpart in gltex at all.

- **A color slot and a spatial slot,** reserved on the GPU path and folded into its one present pass, and a color slot on the CPU path too. gltex had nowhere to put a color correction or an effect.
- **Two picture steps,** one of them drawing the CPU path's scaled copy so the whole GPU frame can be checked byte for byte; gltex had one way to draw a picture, and its pixels were never compared with bbulk's.
- **The CPU path's pixels.** The GPU path blends with the CPU path's integer arithmetic (ADR-1300 D6); gltex's float blend could differ from bbulk's by rounding.
- **Composition draws only what changed,** into an upright frame kept between frames; gltex cleared and redrew the whole screen every frame.
- **The turn happens once, at present,** and puts the grid at the CPU path's offsets, where gltex rotated every quad and computed the turned grid's margins its own way, a pixel away from bbulk's.
- **A turn keeps the GPU's glyphs and pictures, and runs no set-up.** gltex dropped and rebuilt all its GL state on a turn, and a turn whose set-up failed left the screen frozen.
- **A scaled picture is filtered as the CPU path filters it,** within a tolerance, a picture shrunk past 2:1 included; gltex sampled every scaled picture bilinear and skipped texels past 2:1.
- **A display always draws.** A failed GPU set-up puts the head on the CPU path, and a failure during a run costs speed, one frame, or the GPU path, by its kind (D9); the C reference gave a display no screen when gltex could not be made, and a failed gltex set-up drew nothing until it succeeded.
- **The render thread draws through a render node** into buffers the display lends, imported as dma-bufs, where drm3d drew through EGL window surfaces on the card itself.
- **No `mediump` fall-back.** A GPU without `highp` in fragment shaders draws on the CPU path (ADR-1300 D6); gltex drew with `mediump`.

## Verification

- **The GPU path draws the CPU path's bytes.** Tier 2 draws every screen on both paths, the GPU path under llvmpipe with the CPU copy step, and compares the GPU path's buffer with the CPU path's turned drawing byte for byte at all four orientations, the comparison ADR-1300's Verification asks for, extended here to the turns. The screens include a display with an odd margin, 1280x800 turned right with a 10x19 cell, and the pointer reaching into the border. A planted placement that centres the grid in the upright frame and turns it fails there.
- **GPU scaling stays within the tolerance.** Tier 2's picture screens, at shrink factors from 1.5 to 8 and growth factors of 2 and 3, compared with the CPU path's within the tolerance open question 3 sets. A planted bilinear sampler in place of the filter fails it.
- **Empty slots cost nothing.** With slots 3 and 4 empty, the present shader holds no instruction of theirs, and a frame's GL calls are the same as with the slots left out of the source, which a test on the stand-in GL compares.
- **Damage holds.** A frame that changes one cell composes that cell alone and presents the rectangles changed over the buffer's age, counted by the stand-in GL; the same frame with a spatial step that reads its neighbours presents the whole buffer.
- **Each kind of failure lands where D9 puts it.** The stand-in GL, made to fail each set-up step in turn, leaves the head on the CPU path with one log line naming the step. Made to refuse an allocation, it draws the frame's pixels unchanged; made to raise one GL error, it drops one frame and draws the next whole; made to lose its context or raise errors in two frames running, it leaves the head on the CPU path.
- **A turn runs no set-up.** On the stand-in GL, a turn creates and frees no texture and compiles no shader.
- **Tier 3** runs `gl-1280x800` on vtium under virgl, the only tier that proves the dma-buf import.

## Open questions

Each item is left open for the maintainer, with this record's recommendation.

1. **Which OpenGL ES version the GPU path needs.** Version 3.0 has `highp` in every fragment shader, single-channel textures for the glyph masks, and `texelFetch` for present's exact reads. Version 2.0 has them only through extensions, and is what older single-board GPUs offer, such as the Raspberry Pi 3's and the Mali-400. *Recommendation:* 3.0. Those older GPUs then always draw on the CPU path, which minspec targets anyway.
2. **`hwaccel`'s default in a build with `gpu`.** The C reference kept it off. ADR-1300 D4 and ADR-1401 D2 let the build set a default for `glyph-cache` and `display-buffers`. *Recommendation:* off in every build until tier 3's GL variant passes on vtium, then a fresh decision on whether a `gpu` build turns it on.
3. **The tolerance for GPU-scaled pictures.** *Recommendation:* at most 1 level in any channel of any pixel against the CPU path, which a shader that uses the CPU path's own filter weights in `highp` can meet. The slice that builds the step reports the largest difference it sees on tier 2's picture screens, and the maintainer confirms the figure there.
4. **How large the picture textures may grow, and what a picture too large for a texture does.** *Recommendation:* the textures of both picture steps count against ADR-1100 D5's bound for each display, twice the display's screen in RGBA, and a picture whose own pixels pass that bound or the GPU's largest texture is drawn from the CPU copy, at its drawn size, as gltex fell back to a texture at the drawn size. That one picture then matches the CPU path byte for byte.
5. **Linking the GPU libraries or loading them at start.** Linked, a `gpu` build does not start on a machine without libEGL and libgbm. Loaded when the GPU path starts, the same build runs there on the CPU path, at the cost of a small loader for libgbm's few calls in place of the gbm crate, and more unsafe code. *Recommendation:* load them at start, so a packager ships one `gpu` build to every machine.
6. **Trying the GPU again after a head has left it.** *Recommendation:* not within the run. A lost context or errors in two frames running point to a driver fault that a retry is likely to meet again, and flipping between paths would show the user two renderers. The second probe asks the maintainer.
7. **The renderer's names in the log.** The C reference's names, `bbulk` for the CPU path and `gltex` for the GPU path, are what tier 3's variants look for and what the C reference's logs show. vtium's own names, such as `cpu` and `gpu`, say plainly what draws, and need the `renderer` field of tier 3's variants changed in one place (`tools/tier3/t3_scenario.py`). *Recommendation:* vtium's own names. vtium's GPU path draws the CPU path's pixels through a pipeline gltex never had, so a log that says `gltex` tells its reader something false.
8. **Which goldens judge tier 3's GL variant.** The C reference's GL goldens carry two defects that vtium fixes by decision. `sixel-evict-rotated` draws the turned grid one pixel right of the 2D golden, 11 440 pixels differing and 270 at 1 level once shifted; a comment of 2026-10-25 on the C fork's issue on a turned grid one pixel off, posted from the maintainer's account, said the GL golden, once fixed and read beside the 2D golden, "should then match within 1 level". And tier 3's kitty screens show a 320x240 picture gltex shrank 1.6:1 bilinear, up to 41 levels from bbulk's box filter (the C fork's issue on pictures shrunk past 2:1). gltex's float blend could also round away from bbulk's (ADR-1300 D6), where vtium's GPU path draws the CPU path's pixels. The GL variant's configuration differs from `2d-1280x800`'s only by `hwaccel` (`tools/tier3/guest/kmscon.conf`, `kmscon-gl.conf`), at the same size. The outline-font work's scope names the goldens of tier 3's 2D variants and not the GL one's. *Recommendation:* judge `gl-1280x800` against `2d-1280x800`'s re-baselined goldens, which the maintainer's comment already expects within 1 level, and keep the C reference's GL goldens as the record of what gltex drew; the maintainer decides it with the outline-font work's re-baseline.
9. **How large the upright frame is.** Holding the grid alone, present draws the border, the pointer and the notices in the display's own pixels, and a spatial step reaches only the grid. Holding the whole display, with the grid placed at the CPU path's turned offsets mapped back into upright coordinates, a spatial step reaches the border too, and the pointer and notices could be composed into the frame; the frame costs the border's pixels more, and the pointer's arrow, which turns with the display, must be drawn unturned. *Recommendation:* the grid alone. It keeps the turn and the placement in one place, present, and leaves the border to the frame shader's record.
10. **What joins vtium-render's `Gl` trait to vtium-platform's GL.** vtium-platform can implement `Gl` itself, taking vtium-render as a dependency under `gpu`, which reverses the one-way arrows docs/architecture.md describes and builds vtium-term and vtium-font with vtium-platform. Or vtium-platform exposes a safe GL type of its own, and the binary implements `Gl` over it with a newtype adapter, which keeps the arrows, at the cost of a safe surface in vtium-platform that mirrors the trait's dozen or so calls. *Recommendation:* the adapter in the binary; the mirrored surface is small, and docs/architecture.md stays true.

## Consequences

### Positive

- Color correction, display compensation, accessibility filters and a CRT look each fill a slot without touching the rest of the frame, and the color slot works without a GPU.
- A turned display draws on the CPU path's pixels, and a turn keeps every texture and runs no set-up.
- With every optional slot empty, the GPU path pays one extra copy a frame, inside the one present pass.
- Tests can check the whole GPU frame byte for byte against the CPU path by choosing the CPU copy step.
- No GL call is reachable from safe code without the trait's checks, and vtium-render stays free of unsafe code.
- No display goes dark because the GPU path failed, and a small failure costs speed or one frame, never the GPU path.

### Negative

- Each display on the GPU path keeps an upright frame in GPU memory, at most the display's size: 4.0 MiB at 1366x768, 7.9 MiB at 1920x1080, 63.3 MiB at 7680x2160. ADR-1300's alternatives and ADR-1401 D2 said the GPU path keeps no whole frame; dated notes in both point here, and they take effect when this record is accepted.
- A head that leaves the GPU path during a run stays on the CPU path until vtium restarts, if open question 6 goes as recommended.
- Two picture steps mean two texture caches to bound and test.
- A spatial step that reads the time brings a frame clock and a new place where time enters, which its own record must justify.
- Tier 2's GPU screens need Mesa's llvmpipe on every machine that runs them, CI's runners included.

### Neutral

- The CPU path changes only by reserving the color slot as a no-op on its last write before the depth reduction.
- Adding `gpu` to the binary's features makes ADR-1401 D2's test of the build's buffer count fail until `gpu` is named in its list.
- ADR-1300 D6's shader, pools and budgets are unchanged; they now draw into the upright frame.
- The frame shader's record fills slots 3 and 4 against this one.

## Alternatives Considered

- **gltex's flow: turn every quad in the vertex shader and draw straight into the display's buffer.** Rejected by the maintainer's decision to turn at present. Each renderer placed a turned grid by its own arithmetic, which put gltex's a pixel away from bbulk's; a turn rebuilt all GL state; and each color or spatial step would need to know the orientation.
- **The grid centred in the upright frame, and the frame turned whole.** Rejected: with an odd margin the spare pixel lands on the other side from the CPU path's, which is the one-pixel offset the C fork recorded (D6).
- **No upright frame when every optional slot is empty and the display is upright,** composing straight into the display's buffer. Not taken: it gives the pipeline two shapes that tests must cover separately, for one copy a frame that costs the GPU little, and a change of configuration would switch shapes at run time.
- **A pass for each step,** each reading the last one's output. Not taken: a pass reads and writes the whole frame, and folding the color and spatial steps into present keeps an empty slot free.
- **unsafe GL calls inside vtium-render,** allowed module by module. Rejected: docs/architecture.md confines unsafe code to vtium-platform, and a trait lets the stand-in GL run the renderer's tests with no GPU.
- **vtium-platform implementing `Gl` with vtium-render as its dependency.** Open question 10 weighs it against the adapter in the binary.
- **The `Gl` trait in a crate of its own,** which vtium-render and vtium-platform both depend on. Not taken: the trait's types, a frame's quads and the shader source, belong to vtium-render, and the adapter in the binary keeps the arrows without a third crate.
- **A GL error check after every call.** Rejected: on a threaded driver each check makes the CPU wait for the GPU; a check for each batch, or `KHR_debug`, reports the same errors for the frame (D7).
- **wgpu.** Rejected: a large dependency for one pipeline, built for Vulkan and Metal first, with no hold on how its GL backend rounds or imports a dma-buf.
- **glutin.** Not taken: it provides contexts for windowing systems, and vtium needs only EGL on GBM, which khronos-egl gives directly.
- **Raw bindings to libEGL, libGLESv2 and libgbm.** Not taken for GL and EGL: glow and khronos-egl are those bindings already, maintained. A small loader of libgbm's calls remains the way to load GBM at start (open question 5).
- **Importing the display's dumb buffers** through PRIME, with no GBM. Not taken: drivers differ in whether they render into an imported dumb buffer, a dumb buffer is linear only, and the maintainer named GBM.
- **`mediump` where a GPU lacks `highp`, as gltex did.** Rejected by ADR-1300 D6: `mediump` cannot hold its integer arithmetic exactly, so the paths could not agree byte for byte. Whether OpenGL ES 2.0 with `highp` is enough is open question 1.
