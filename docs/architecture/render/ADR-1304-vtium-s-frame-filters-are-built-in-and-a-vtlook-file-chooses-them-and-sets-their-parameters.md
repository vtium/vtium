---
contract: adr/v1
kind: decision
verb: add
capability: renderer
basis:
  - operator: aaronsb
    level: directed
    said: "the frame shader fills a slot in the GPU path's pipeline, which [the GPU path issue]'s comment of today describes. The pipeline reserves two slots after composition: a color slot (gamma, color space mapping, display compensation, contrast, color-blind filters) and a spatial slot (a CRT look, scanlines, sharpening). Both hold a no-op unless the configuration fills them. The color slot also exists on the CPU path, as a lookup table, so color correction does not need a GPU. The slots are reserved first, and their steps come one at a time. This issue's ADR defines what fills each slot: how a shader is named, its inputs, its cost bound, and the fall-back to the no-op when it fails."
    via: "the GitHub issue for an optional frame shader on the GPU path, comment of 2026-11-03 headed \"Decided 2026-11-03 (maintainer)\"; posted from the maintainer's account; who wrote the wording is not recorded; the issue number replaced by a phrase in brackets"
  - operator: aaronsb
    level: directed
    said: "The maintainer decided on 2026-11-03 that vtium hard-codes no filter. Each step that fills the color or spatial slot, on the CPU path or the GPU path, is defined by a specification that vtium loads at run time in one standard way, much as a shader is loaded: the definition names the filter and its parameters, and the configuration chooses a filter and sets its parameters. The frame shader's record (drafted as ADR-1304 on branch `docs/frame-filter-adr`) defines that specification."
    via: "the same issue, second comment of 2026-11-03; posted from the maintainer's account; who wrote the wording is not recorded"
  - upstream: "aaronsb/kmscon b1f7a7c5: gltex compiled its two shader programs from GLSL embedded in the binary, a precision line put before each fragment shader (src/render/gltex.c:450-537; the GLSL in src/render/gltex_atlas.vert, gltex_atlas.frag, gltex_image.vert and gltex_image.frag), and drew nothing after its draws that changed a pixel's color or read its neighbours; the only colors a user set were the palette's, through the palette and palette-* keys (src/config.c:1427-1436)"
  - evidence: "the measurement of this record's agent, 2026-11-03, on an AMD Ryzen 9 9950X3D, a release build of a stand-alone Rust program: a table of N x N x N nodes of half-precision floats, blended trilinearly in 32-bit floats and rounded as floor(x * 255 + 0.5). At N = 17, 33 and 65 the identity table returned all 16,777,216 colors unchanged. A gamma of 2.0 (each channel to the power 0.5) was off the exact result by up to 16, 12 and 8 levels, at inputs 3, 2 and 1, with 16, 7 and 3 of the 256 levels off by more than 1; a gamma of 1.2 was within 1 level everywhere at N = 33 and 65. At N = 33 a lookup took 28.4 ns a pixel over random colors, 58.9 ms for a 1920x1080 frame, and 1.36 ns a pixel over runs of 8 to 80 equal pixels with the last lookup remembered, 2.81 ms a frame; the table held 421 KiB as 32-bit floats and 280 KiB as RGBA16F"
  - standard: "OpenGL ES 3.0: three-dimensional textures are part of the core API, and RGBA16F is among the formats it lists as texture-filterable, so a 3D texture of half floats can be sampled with GL_LINEAR, which blends the eight nearest texels"
  - standard: "OpenGL ES 3.0, conversion from floating point to normalized fixed point: a color written to an 8-bit buffer becomes one of the two nearest 8-bit values, rounding to nearest preferred"
  - standard: "Adobe Cube LUT Specification 1.0: a text file of keyword lines (TITLE, LUT_1D_SIZE or LUT_3D_SIZE, DOMAIN_MIN, DOMAIN_MAX) and data lines of three numbers, red changing fastest; a 3D table holds LUT_3D_SIZE cubed lines"
  - evidence: "Machado, Oliveira and Fernandes, A Physiologically-based Model for Simulation of Color Vision Deficiency, IEEE Transactions on Visualization and Computer Graphics 15(6), 2009: matrices, applied in linear light, that simulate protanopia, deuteranopia and tritanopia"
  - precedent: ADR-1303
  - precedent: ADR-1200
  - precedent: ADR-1201
  - precedent: ADR-1401
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "vtium hard-codes no filter's settings. A filter's parameters are declared in one standard form and set by configuration, much as a shader declares its inputs and a program sets them."
    via: the maintainer in the steering session, 2026-11-03, relayed by the team lead; the wording is the team lead's, not the maintainer's
    paraphrase: true
  - operator: aaronsb
    said: "The layers are a known, fixed set: the color slot and the spatial slot of ADR-1303. The filters inside each layer are a known set built into vtium; configuration chooses filters and sets their parameters. No filter code is installed from outside."
    via: the maintainer in the steering session, 2026-11-03, relayed by the team lead; the wording is the team lead's, not the maintainer's
    paraphrase: true
  - operator: aaronsb
    said: "Each layer has a minimum contract a filter must meet to run in it."
    via: the maintainer in the steering session, 2026-11-03, relayed by the team lead; the wording is the team lead's, not the maintainer's
    paraphrase: true
  - operator: aaronsb
    said: "At the simplest end there are no shaders and no CPU filters at all: with no configuration, vtium compiles no filter shader piece, builds no table and does no filter work on the CPU."
    via: the maintainer in the steering session, 2026-11-03, relayed by the team lead; the wording is the team lead's, not the maintainer's
    paraphrase: true
  - operator: aaronsb
    said: "The pipeline's configuration lives in a file called a vtlook, much as a frit holds a font set."
    via: the maintainer in the steering session, 2026-11-03, relayed by the team lead; the wording is the team lead's, not the maintainer's; the maintainer chose the term vtlook
    paraphrase: true
status: proposed
date: 2026-11-03
deciders:
  - aaronsb
related: []
---

# ADR-1304: vtium's frame filters are built in, and a vtlook file chooses them and sets their parameters

## Summary

- **Decided by the maintainer (2026-11-03):** vtium hard-codes no filter's settings. A filter is a step that fills ADR-1303's color slot or spatial slot, which this record calls the color layer and the spatial layer. The layers are that fixed pair, and the filters inside each layer are a known set built into vtium; no filter code is installed from outside. Each filter declares its parameters once, in one standard form, and configuration chooses filters and sets their parameters, much as a shader declares its inputs and a program sets them. Each layer has a minimum contract that a filter must meet to run in it. With no configuration, vtium compiles no filter shader piece, builds no table and does no filter work on the CPU. The configuration lives in a file called a vtlook (a look file), as a frit (a font set file) holds a font set.
- **Proposed by this record's agent, for the maintainer to confirm:** A filter's declaration gives each parameter's name, kind, range, default and a line of help, and that one declaration drives `vtium check-config`, vtium-config's display and editing, the GPU path's shader inputs and the CPU path's function arguments (D1). A color filter maps each pixel's color using that color alone, and vtium merges the whole color layer into one table of 33 by 33 by 33 colors, which the CPU path looks each pixel up in and the GPU path samples as a texture: one lookup a pixel however many color filters run (D2). A spatial filter is one piece of GLSL in vtium-render, on the GPU path only, which declares how far it reads and how many reads it makes, under caps, and never reads the time (D3). A vtlook is `/etc/vtium/looks/NAME.vtlook`, owned by root, with one section per filter, run in file order (D4). A head chooses one with `look =` in heads.conf, and `look=` in vtium.conf sets the default (D5). A filter that fails turns its whole layer off, with one log line (D6). Where a look is in force, vtium draws the pointer in the frame instead of on the display's cursor plane, and a 1-bit display ignores the look (D7). The first filters are gamma, contrast, tint, color-blind and cube, which reads a `.cube` table file, in the color layer, and scanlines and sharpen in the spatial layer (D8, D9). The filters sit behind a cargo feature, `filters`, which the minspec build leaves off (D10). Eight open questions remain.
- **Trades away:** An effect vtium does not build cannot be added by a file. A color filter is drawn through a table, which is straight between its nodes, so a curve that bends hard near black loses detail there: a gamma of 2.0 is off by up to 12 levels among the darkest 7 levels of 256. The CPU path pays a lookup for each pixel it redraws, 28 ns for a pixel of a new color on a fast desktop and 1.4 ns along a run of one color. Spatial filters need the GPU path, which `hwaccel` leaves off by default. A head with a look gives up the hardware pointer.
- **One-way?** No. Everything is behind a cargo feature, and the vtlook files are the only data it stores. The vtlook format becomes expensive to change once people write files, and its `version = 1` line leaves room for that.
- **Probes:**
  - *Not confident (the arrow):* when a head shows a look, vtium draws the mouse arrow into the picture itself instead of using the display's hardware pointer, so the look's colors and effects reach the arrow too: an amber look makes the arrow amber. The cost is that each move of the mouse redraws the arrow's old and new places, and with an effect such as sharpening, the whole screen. Is that what you want, or should the hardware arrow stay and simply not take the look?
  - *Confident (a failing filter):* if one filter in a look cannot be used, because of a bad value, a missing `.cube` file or a shader the graphics driver refuses, vtium switches off that filter's whole layer, all the color filters or all the effects, and writes one line to the log. You never see half a color correction. Is that the behaviour you meant?
- **Inversion:** Between "no filters", the C reference's gltex, whose shaders were compiled from source built into the program with no color or effect stage, and "user shaders", any GLSL file a user installs, run in the present pass, as video players and emulators allow. The answer is a fixed menu of built-in filters, chosen and set by a file of data, with a color table file as the one outside input, and never code.

## Context

ADR-1303 gives the GPU path a fixed pipeline of five slots and reserves two of them, color and spatial, holding no-ops until a later record defines what fills them; the CPU path reserves the color slot too. The skeleton built for that record (`crates/vtium-render/src/gpu/pipeline.rs`) already joins a color piece and a spatial piece of GLSL into present's fragment shader under `#ifdef`, between its head and its `main`, and asks the spatial piece for its reach, how many pixels from the pixel it writes it reads. Both pieces are empty today.

The maintainer decided on 2026-11-03 what fills the slots, in five points this record writes down as decided: no filter's settings are hard-coded; the layers are fixed and their filters are a known built-in set; each layer has a minimum contract; with no configuration there is no filter work at all; and the configuration lives in a vtlook file. An earlier comment on the frame shader's issue spoke of a specification "that vtium loads at run time in one standard way, much as a shader is loaded". The later points settle what that means: what vtium loads at run time is the vtlook, a file of data that names built-in filters and sets their parameters, and each filter's definition is its declaration, compiled into vtium.

The maintainer asked for care with complexity in the GPU frame. This record keeps each layer to one mechanism: the color layer is always one table, and the spatial layer is always one chain of pieces in the present pass that already exists.

A filter is a built-in step of the color or spatial layer. A filter's declaration is the constant in vtium's code that names the filter, its layer and its parameters. A look is the set of filters a vtlook chooses, with their parameter values. A look is in force on a display when at least one of its layers holds a filter there.

## C notes: the C reference's flow

gltex compiled its two shader programs, one for glyph atlases and one for pictures, from GLSL embedded in the binary, putting a precision line before each fragment shader (`kmscon/src/render/gltex.c:450-537`). Nothing after its draws changed a pixel's color or read its neighbours, and no user file reached a shader. bbulk, the CPU renderer, had no such stage either. The only colors a user set were the palette's, through the `palette` key and the `palette-*` keys (`kmscon/src/config.c:1427-1436`). The C reference had no filters to be a floor for, so every filter here goes past it.

## Decision

D1 to D3 define a filter and the contract of each layer. D4 and D5 say where a look lives and how a head chooses one. D6 and D7 say what happens when a filter fails, to the pointer, and on a 1-bit display. D8 and D9 name the first filters. D10 and D11 place the code, and D12 states the build with no look. D1, D4 and D12 each open with what the maintainer decided; the rest of the record is this record's agent's proposal.

```mermaid
flowchart LR
    L["amber.vtlook<br>[color tint]<br>[color contrast]<br>[spatial scanlines]"]:::cfg
    R["vtlook reader<br>checks each value against<br>the filters' declarations"]:::proc
    T["one color table<br>33 x 33 x 33 nodes"]:::data
    G["spatial pieces, in file order,<br>joined into present's shader"]:::proc
    C["CPU path: each pixel it writes<br>looked up in the table"]:::path
    P["GPU path, present: spatial pieces,<br>the turn, then one table sample"]:::path
    L --> R
    R -->|color filters| T
    R -->|"spatial filters (GPU only)"| G
    T --> C
    T --> P
    G --> P
    classDef cfg fill:#fbbf24,color:#1a1a1a,stroke:#4a5568
    classDef proc fill:#2d7d9a,color:#ffffff,stroke:#4a5568
    classDef data fill:#2d8e5e,color:#ffffff,stroke:#4a5568
    classDef path fill:#7c3aed,color:#ffffff,stroke:#4a5568
```

The figure follows one vtlook from left to right. The reader checks every value against the declarations. The color filters become one table, which both paths read. The spatial filters become pieces of present's shader, on the GPU path only, and run before the turn and the table, as ADR-1303 D1 orders the slots.

### D1: a filter and its declaration

**Decided by the maintainer (2026-11-03):** vtium hard-codes no filter's settings. Each filter declares its parameters once, in one standard form, and configuration sets them. The layers are the color layer and the spatial layer, and the filters in each are a known set built into vtium. No filter code is installed from outside. Each layer has a minimum contract (D2, D3).

**Proposed:** a filter's declaration is one constant in vtium-render (D10), which gives:

- the filter's name, lowercase letters, digits and `-`, unique within its layer;
- its layer, color or spatial;
- a line of help;
- each parameter's name, kind, range or list of choices, default and a line of help. A parameter may have no default, and a look must then set it.

A spatial filter's declaration also gives its reach and its reads (D3).

| Kind | Written in a vtlook as | Example | Range | On the GPU path |
|---|---|---|---|---|
| number | a decimal, with an optional sign and at most four digits after the point | `1.15` | a minimum and a maximum, inclusive | `float` |
| integer | a whole number | `3` | a minimum and a maximum, inclusive | `int` |
| color | `r,g,b`, each 0 to 255, as vtium.conf's palette and notice colors take it | `255,176,0` | none | `vec3`, each channel divided by 255 |
| choice | one word from the declared list | `deuteranopia` | the list | `int`, the word's place in the list |
| on/off | `yes` or `no`, as heads.conf's `isolated` takes it | `yes` | none | `bool` |
| file | a file's name in the looks directory, with its suffix and no path | `panel.cube` | the suffixes the filter reads | none: only the `cube` color filter takes one (D9) |

The one declaration drives four readers:

- **The vtlook reader** in vtium-settings checks each value against its kind and range. `vtium check-config` runs the same reader, so a file checked before it is installed gets the verdict it gets at start.
- **vtium-config** shows each parameter with its help, range and default, and edits it with a control fit for its kind: a number in its range, a color, one of the choices.
- **The GPU path** gives each parameter of a spatial filter to its piece as a uniform named `<filter>_<parameter>`, with `-` written as `_`, of the type the table above names. The pipeline lists the uniforms as the program's inputs, so the check every `Gl` implementation makes, that a program declares each of its inputs (`crates/vtium-render/README.md`, the GPU path), refuses a piece whose uniforms do not match its declaration.
- **The CPU work** of a color filter takes the parameters as typed Rust values when it builds the table (D2), on both paths.

### D2: the color layer, one table

**Proposed.** The color layer's contract: a color filter maps each pixel's color to a new color using only that color. It reads no other pixel, not the pixel's place, not the cell the pixel belongs to and not the time. In vtium's code a color filter is a function from one color to another, given its parameters, over colors whose channels run from 0 to 1. Its one job is to give the color it maps each color to, so that vtium can turn the layer into a table.

vtium merges the whole layer into one table, a lattice of 33 steps in each of red, green and blue: 35,937 nodes. The node at (r, g, b) holds the output of the layer's filters, applied in file order, for the color (r/32, g/32, b/32), each filter's output clamped to 0 to 1 before the next takes it. Each node holds three half-precision floats. The CPU path and the GPU path read the same table, so the number of color filters changes the time it takes to build the table and nothing else: each pixel costs one lookup.

**Building it.** The table is built on the CPU, once for each head, when the look is read and again when it changes. It is built from IEEE 754's correctly rounded operations, which give the same bits on every machine, and from power, exponential and logarithm functions written in Rust, never the platform's maths library, whose last bits differ between machines. A test pins the bytes of each filter's table at its defaults and at one setting that is not its default, and CI's `arm-native` job runs it, so the CPU path's goldens hold on ARM as on x86.

**On the CPU path,** the slot sits where ADR-1303 D4 puts it, on the renderer's last write before the depth reduction. The path keeps the table's values as 32-bit floats, 421 KiB for each head. For each pixel it finds the eight nodes around the pixel's color and blends them trilinearly in 32-bit floats, with no fused multiply-add, and rounds each channel to 8 bits as floor(x × 255 + 0.5). It remembers the last color it looked up and the result, so a run of one color costs one lookup. Measured on a fast desktop, a lookup costs 28 ns a pixel over random colors, 59 ms for a 1920x1080 frame of them, and 1.4 ns a pixel over runs of 8 to 80 equal pixels, 2.8 ms a frame (`basis`). A terminal's screen lies between the two, mostly runs of a background color broken by the edges of glyphs, and the CPU path looks up only the pixels it redraws. The slice that builds the layer measures a full redraw of tier 3's screens in its 1-vCPU guest and reports it.

**On the GPU path,** present uploads the same half floats as a 3D texture in RGBA16F, 280 KiB for each head, which OpenGL ES 3.0 lets the GPU filter linearly. Present's color step samples it once, with linear filtering, at (c × 32 + 0.5) / 33 in each channel, after the spatial step and the turn (ADR-1303 D6), and leaves alpha as it is. The GPU blends the eight nodes itself, with weights of fewer bits than the CPU path's floats, which the specification leaves to the GPU; for the starting filters that moves a channel by under a tenth of a level. GL then writes the result into the display's 8-bit buffer as one of the two nearest levels, preferring the nearer. So a pixel lands at most one level from the CPU path's, which is ADR-1303 D1's tolerance. If the slice that builds the layer measures a share of differing pixels past ADR-1303's, it reports that to the maintainer and does not raise the share itself, as ADR-1303 D1 asks.

**Why 33 a side.** A table of 33 a side is the size `.cube` files most often hold (D9), the identity table returns all 16,777,216 colors unchanged on the CPU path at 17, 33 and 65 a side alike, and 33 keeps the table under half a megabyte. Between its nodes the table is a straight line, so a filter that bends hard near black is approximated in the first step of 8 levels. Measured against the exact curve, a gamma of 1.2 stays within one level everywhere, and a gamma of 2.0 is off by up to 12 levels, at input 2, with 7 of the 256 levels off by more than one. At 65 a side the worst is 8 levels, for tables of 2.1 MiB on the GPU and 3.1 MiB on the CPU; at 17 it is 16 levels (`basis`). The size is a constant in vtium-render's bounds table.

A change of the look's color filters rebuilds the table. The CPU path then draws its frame whole, since it colors pixels as it writes them, and forgets the color it remembered; the GPU path presents whole.

### D3: the spatial layer, one chain of pieces in present

**Proposed.** A spatial filter is one piece of GLSL in vtium-render, joined into present's fragment shader between its head and its `main` (ADR-1303 D6). It runs on the GPU path only, as ADR-1303 D5 decides. The spatial layer's contract:

- **What it reads.** A spatial filter writes one pixel of the upright frame from the frame's pixels within its reach of that pixel, the pixel's place in the upright frame, and its own parameters. Its parameters are its only uniforms.
- **No time.** The pipeline gives a spatial filter no clock. A filter that needs one, such as an animated effect, needs frames drawn on a clock and a new place where time enters (docs/architecture.md, Determinism), so it is outside this contract and needs a record of its own (ADR-1303 D5).
- **A declared cost.** The declaration gives the filter's reach and its reads: how many pixels away from the pixel it writes it reads, and how many texels it reads for each pixel it writes. The reach may depend on the parameters and on the frame's size, and vtium works it out for each display when it sets that display's pipeline up.
- **Caps.** The layer's whole chain reads at most 25 texels for each pixel and reaches at most 256 pixels, two rows of vtium-render's bounds table. ADR-1303 D3 tiles a frame past the GPU's largest texture with an overlap equal to the reach, and asks that the reach stay under half a tile. OpenGL ES 3.0's smallest largest texture is 2048 pixels, and a reach of 256 leaves such a tile 1536 pixels of its own.

Within the layer, filters run in file order, inside present's one pass. vtium joins each piece after defining a macro, `SPATIAL_IN(at)`, through which the piece reads its input at a place. For the first filter `SPATIAL_IN` reads the upright frame's texel, a place past the tile's edge taking the nearest pixel at that edge; the tile's overlap holds the neighbours past an inner edge (ADR-1303 D3). For each later filter, `SPATIAL_IN` is the filter before it. A chain's reach is therefore the sum of its filters' reaches, and its reads the product of theirs: scanlines then sharpen reach 1 pixel and read 5 texels. A look names each filter at most once, so no two pieces declare the same uniform.

A spatial filter that reads only the pixel it writes, such as scanlines, leaves damage tracking as it is: present still draws only what changed. A filter that reads any other pixel makes every present a whole one (ADR-1303 D5). A change of the look's spatial filters changes the reach, so the upright frame is made anew and drawn whole (ADR-1303 D3), and present's shader is compiled again (ADR-1303 D6).

The CPU path has no spatial layer. A head on the CPU path whose look holds spatial filters draws the color layer and skips the spatial one, and logs that once for the head, naming the look. `vtium check-config` says the same when `hwaccel` is off or the build has no `gpu` feature.

### D4: the vtlook file

**Decided by the maintainer (2026-11-03):** the pipeline's configuration lives in a file called a vtlook, as a frit holds a font set.

**Proposed:** a vtlook is a file `NAME.vtlook` in `/etc/vtium/looks`, the directory vtium was built with. A name follows the rule for a head's name, 1 to 64 characters of `A-Z a-z 0-9 . _ + -`, and `none` is reserved (D5). vtium reads a vtlook, and a `.cube` file it names, only when root owns the file and neither its group nor others can write it or a directory above it, the rule vtium applies to heads.conf (ADR-1401 D7). vtium never writes either. vtium's console process reads the file itself, as it reads heads.conf.

The file uses heads.conf's syntax: a `#` starts a comment, `key = value` lines, and sections in brackets.

```ini
# /etc/vtium/looks/amber.vtlook: an amber phosphor with scanlines.
version = 1

[color tint]
color = 255,176,0
amount = 1.0

[color contrast]
contrast = 1.15
brightness = 0.0

[spatial scanlines]
period = 3
strength = 0.3
```

`version = 1` comes before the first section. Each section is `[LAYER FILTER]`: the layer, `color` or `spatial`, and the name of a filter in that layer. Its lines set that filter's parameters. A parameter the section leaves out takes its declared default, and vtium-config writes every parameter out, so a file it writes states each value it uses. Within a layer the filters run in the order of their sections. The layers run in the pipeline's order, the spatial filters and then the color filters (ADR-1303 D1), wherever their sections sit in the file. A file holds at most 64 KiB and 32 sections, rows of vtium-settings's bounds.

The reader sorts each error by what it spoils:

- **The whole look is refused** for a file vtium may not read, a missing or unknown `version`, a line that is neither a section nor `key = value`, a section whose layer is neither `color` nor `spatial`, or a file past its bounds. The head draws with no look, and vtium logs one line naming the file, the line and the reason.
- **One layer is refused** for a filter that layer does not have, a parameter the filter does not declare, a value outside its kind or range, a filter named twice, or a parameter with no default left unset. That layer holds its no-op, the other layer runs, and vtium logs one line (D6).

### D5: how a head chooses its look

**Proposed** (open question 1): a head chooses a look with `look = NAME` in its section of heads.conf, beside `profile =`. `look=NAME` in vtium.conf sets the look of every head that does not set its own. `look = none` in a head gives that head no look whatever vtium.conf says. With neither set, a head has no look. Every display of a head takes the head's look, but for a 1-bit display (D7). ADR-1401 D3's list of a head's keys gains `look`, and vtium.conf's key table gains `look`. Both keys are read in every build, so one configuration serves every build (D10).

vtium reads the look when it starts, and again when it rereads its configuration, which ADR-1401's reload does once it is built. A change to the color filters rebuilds the table (D2). A change to the spatial filters changes the reach, makes the upright frame anew and compiles present's shader again (D3).

### D6: when a filter fails

**Proposed.** A filter fails when its section does not pass the reader (D4), when a file it needs cannot be read or parsed (D9), when the reach or reads of the spatial chain pass the layer's caps at a display's size (D3), or when the driver refuses to compile its piece. A filter that fails leaves its whole layer at the no-op, and the other layer runs. vtium logs one line naming the look, the filter and the reason, once each time it reads the look or sets a display up, never once a frame. The whole layer goes, not just the filter, because the filters of a layer are meant together: a chain that corrects a panel's color and then raises its contrast, run without its first step, can look worse than no look at all.

A compile failure never costs the head the GPU path. vtium compiles present's shader with both layers' pieces. If the driver refuses it, vtium logs the failure, leaves the spatial layer out, and compiles again. The color step's piece is one fixed piece of vtium's own, a single texture sample that every filter's table shares, so a failure of that second compile is present's own, which ADR-1303 D9 sorts as a failed set-up. A chain past a cap fails for that display alone, since reach depends on the display's size.

`vtium check-config` reports every failure but a refused compile, which needs the GPU, before a file is installed: it reads the vtlook a head or vtium.conf names with the same reader, and works the chain's reach and reads out at each size the head's displays could take.

### D7: the pointer, and 1-bit displays

**Proposed.** On a display where a look is in force, vtium draws the pointer in the frame, as it does with `soft-cursor`, and sets no cursor plane up. So both layers reach the arrow: a tint colors it, sharpening sharpens it, and a filter that bends the picture bends the arrow with the cell it selects (ADR-1303 D5). When the look leaves the display, through a reread with no look or a look whose layers both fail, the cursor plane comes back. A pointer moved over a look then costs what `soft-cursor` costs: the composition draws the arrow's old and new areas. Under a spatial filter that reads its neighbours, every present is whole, so every move presents the whole display.

A display drawn at 1 bit, by `display-depth=1` or by `auto` on a 1-bit panel, ignores the look: both layers hold their no-ops there, and vtium logs that once, naming the display and the look (open question 6). On a 1-bit display vtium lights each cell by its colors and cuts each pixel's coverage at half (`crates/vtium-render/src/mono.rs`), and a filter between the composition and that cut could carry a whole screen across it: a tint that darkens white below the cut of 128 would blank every lit pixel. A gray panel, `gray4` or `gray16`, is drawn as color, so it takes the look. On every other display the order ADR-1303 D6 sets holds: the spatial step, the turn, the color step, then the depth reduction.

### D8: the first filters

**Proposed.** The first slice builds these filters. A filter's defaults are what someone who writes only its section header most likely wants; where that is no change, as for gamma, the default is no change.

| Filter | Layer | Parameters: kind, range, default | What it does |
|---|---|---|---|
| `gamma` | color | `gamma`: number, 0.5 to 2.0, 1.0 | Raises each channel to the power 1 / `gamma`. Above 1 it lightens the middle tones, and below 1 it darkens them. |
| `contrast` | color | `contrast`: number, 0 to 2, 1.0; `brightness`: number, -0.5 to 0.5, 0 | Multiplies each channel's distance from middle gray by `contrast`, then adds `brightness`. |
| `tint` | color | `color`: color, 255,176,0; `amount`: number, 0 to 1, 1.0 | Mixes each pixel, by `amount`, toward its gray times `color`, the gray being BT.601's luma as mono.rs takes it: at 1 the screen is a single-color phosphor, amber by default. |
| `color-blind` | color | `kind`: choice of `protanopia`, `deuteranopia` or `tritanopia`, `deuteranopia`; `mode`: choice of `simulate` or `correct`, `correct`; `amount`: number, 0 to 1, 1.0 | `simulate` shows how a viewer with that color vision deficiency sees the screen, through Machado, Oliveira and Fernandes's matrices applied in linear light (`basis`). `correct` takes the difference between the screen and that simulation, which such a viewer cannot see, and moves it into channels they can. `amount` mixes the result with the input. |
| `cube` | color | `file`: file, no default | Maps each color through a `.cube` table (D9). |
| `scanlines` | spatial | `period`: integer, 2 to 8, 3; `strength`: number, 0 to 1, 0.3 | Darkens the last row of every `period` rows of the upright frame by `strength`, so the lines run along the text whichever way the display is turned. Reach 0, reads 1. |
| `sharpen` | spatial | `amount`: number, 0 to 2, 0.5 | Adds `amount` times the difference between a pixel and the mean of its four nearest neighbours. Reach 1, reads 5. |

Curvature, which bends the picture as a CRT's glass did, is the next spatial filter (open question 7). It fits the contract without a change to this record: its reach grows with its strength and the display's size, and it reads 4 texels a pixel to blend between them.

### D9: the `cube` filter, a color table from a file

**Proposed** (open question 3). The `cube` filter maps each color through a table read from a file in the format Adobe's Cube LUT Specification 1.0 defines, which color grading and calibration tools write. The file is data: numbers in a fixed grammar, never code. vtium reads `LUT_3D_SIZE` from 2 to 65 and `LUT_1D_SIZE` from 2 to 4096, `TITLE`, and `DOMAIN_MIN` and `DOMAIN_MAX` only at their defaults of 0 and 1. It refuses another domain, a keyword it does not know, a value that is not a finite number and a count of data lines that does not match the size, each with the line and the reason, and it clamps values outside 0 to 1. A file holds at most 16 MiB, enough for 65 cubed lines.

The file is `NAME.cube` in the looks directory, named by the filter's `file` parameter with no path, under the same ownership rule as a vtlook (D4). Root installs it there, by hand or through vtium-config's `apply` (D11). vtium's console process parses it when it reads the look, as it parses heads.conf. The grammar is a few keywords and lines of three numbers, which vtium's own reader takes strictly and within the bounds above, so no decoder of outside formats enters the console, which ADR-1200 D6 keeps fonts' decoders out of.

The filter's function is the file's own table, blended trilinearly for a 3D table and linearly in each channel for a 1D one. The merged table samples it at its 33 nodes (D2), so a file of 33 a side passes into it unchanged, and a larger file is resampled to 33.

### D10: where the code lives, and the `filters` feature

**Proposed** (open question 4).

- **vtium-render**, behind a new feature `filters`, holds the filters: `src/filter/`, one module for each filter with its declaration and its work, a color filter's function or a spatial filter's piece, reach and reads. The spatial pieces' GLSL sits in `src/gpu/glsl/filter/` and builds only with `gpu` as well. `src/filter/mod.rs` holds the one list of the filters that exist, the merge into the table and the CPU path's lookup. A filter is one module, its declaration beside the code it declares.
- **vtium-settings**, behind its own `filters` feature, depends on vtium-render's `filters` for the declarations, constants only, as it takes constants from vtium-font and vtium-term. It holds the vtlook reader and the `.cube` reader. The `look` key of vtium.conf and of heads.conf is in every build.
- **The binary's** `filters` feature turns both on. It is not among the default features, so the minspec build carries no filter code. vtium-config builds with `filters` always, since it lists and edits looks for any console.
- **A build without `filters`** that meets `look` logs once for each head that it has no filters and that the look is not applied, and draws as with no look. `vtium check-config` warns the same, as it warns of `hwaccel` in a build without `gpu`.

The pipeline's skeleton fits this as it stands. Its `Color` gains one step, the table's sample, with the table's texture as its input. Its `Spatial` becomes the look's chain of pieces, whose `reach` is the sum of theirs and whose `presents_whole` is true when any piece reads beyond its own pixel. docs/architecture.md's tree gains the arrow from vtium-settings to vtium-render, marked constants only and behind `filters`.

### D11: vtium-config and `check-config`

**Proposed.** vtium-config lists, checks, previews, edits and writes vtlooks as it does frits (ADR-1201): a `looks` command lists each look with whether vtium reads it and, where it does not, why; `check FILE` checks a file named `*.vtlook` with the console's own reader; its TUI shows each look's filters with each parameter's help, range and default from the declaration, and edits each value by its kind; a save goes through `apply`, which checks each vtlook and `.cube` file with the same readers before it writes any; `remove-look NAME` refuses a look that a head or vtium.conf names; and a head's `look =` line is changed through the one head-file writer, as a head's profile is (ADR-1201). A preview draws a sample, the 16 colors, a gray ramp and a line of text, through the look's table with vtium-render's own code, and shows it as a picture, as the TUI shows a frit's sizes (ADR-1201 D4). Spatial filters are not previewed, since vtium-config draws without a GPU, and the TUI says so beside them. vtium-config does not switch the live console's look (open question 8).

### D12: with no look

**Decided by the maintainer (2026-11-03):** at the simplest end there are no shaders and no CPU filters at all. With no configuration, vtium compiles no filter shader piece, builds no table and does no filter work on the CPU.

**Proposed, how vtium meets it:** with no look for a head, no `look` set or `look = none`, vtium reads no vtlook for it, builds no table, joins no filter piece into present's shader and adds nothing to the CPU path's write chain; the chain decides whether it holds a color step once, when it is set up, never for each pixel. The frame is then the frame of a build without filters, which the stand-in GL's test of present's sources and calls checks (Verification). A build without `filters` is in that state whatever the files say.

## What ADR-1303 changes to fit

ADR-1303 is accepted, and names this record as "the frame shader's record" throughout. This record does not edit it. When this record is accepted, these passages of ADR-1303 change, each with a dated note:

- **Context, second paragraph:** "The frame shader's record, which waits on this one, defines what fills the color and spatial slots" names ADR-1304.
- **D1's table:** the color slot's inputs become one pixel's color and the color table (ADR-1304 D2); the spatial slot's "and the time for a step that reads it" says that no filter of ADR-1304's contract reads the time, and that a step that does needs a record of its own.
- **D3:** "the spatial step's reach ... which the frame shader's record states for each step" and "the frame shader's record bounds each step's reach below that" point at ADR-1304 D3: a chain's reach is the sum of its filters', at most 256 pixels.
- **D4:** "The frame shader's record defines what fills the color and spatial slots" names ADR-1304. The one-table proposal is decided in ADR-1304 D2, and "the frame shader's record decides whether a step that cannot be a table may exist" is answered there: none may, subject to its open question 2.
- **D5:** "the frame shader's record defines what fills it" names ADR-1304 D3, and "the frame shader's record decides whether a spatial step takes the pointer off the cursor plane" is answered by ADR-1304 D7: any look in force takes the pointer off the cursor plane, a color-only look included.
- **D6:** on a 1-bit display both slots hold their no-ops (ADR-1304 D7).
- **D9:** a spatial piece the driver refuses to compile is not a failed set-up: present compiles again without the spatial layer (ADR-1304 D6), and only a failure without it counts under D9.
- **D11** and **Consequences, Neutral:** "against the frame shader's record" and "The frame shader's record fills slots 3 and 4 against this one" name ADR-1304.

Outside ADR-1303, the slice that builds the filters changes: ADR-1401 D3's list of a head's keys, which gains `look`; the head file's template (`crates/vtium-settings/templates/heads.conf`), vtium.conf's key table and its example; docs/config.md; docs/architecture.md's crate tree (D10); and the bounds tables of vtium-render and vtium-settings.

## Where vtium departs from the C reference

The C reference had no filters, so each item goes past it rather than departing from a behaviour it had.

- **A color layer on both paths and a spatial layer on the GPU path,** chosen and set by a vtlook file; gltex and bbulk drew nothing after their draws.
- **The pointer drawn in the frame where a look is in force.** The C reference put the pointer on the cursor plane unless `soft-cursor` was on (`kmscon/src/terminal.c:1269-1270`); vtium does the same where no look is in force.

## Verification

- **The table is exact where it should be.** A table built from filters at settings that change nothing, gamma at 1.0, contrast at 1.0 with brightness 0, gives every one of the 16,777,216 colors back unchanged on the CPU path. For two filters that do not commute, tint then contrast, the table matches the chain applied in file order at every node, and a planted merge that applies them in reverse order fails.
- **The table's bytes are the same on every machine.** A test pins the digest of each filter's table at its defaults and at one setting that is not its default; `arm-native` runs it. A table built with the platform's `powf` is the defect it guards against.
- **The CPU path's remembered color is forgotten with its table.** A planted lookup that keeps the remembered color across a change of look draws the old color and fails.
- **The reader sorts each error by what it spoils.** vtium-settings's tests give each error of D4 and check that it refuses the whole look or one layer as D4 says, with the line and reason in its message; vtium-config's tests show and edit each parameter from the same declaration.
- **No look costs nothing.** On the stand-in GL, with no look, present's sources and its calls are those of a build without `filters`, and the CPU path's write chain holds no color step.
- **The GPU path is wired to the declarations.** On the stand-in GL, a color look joins one color piece and one 3D texture input; a spatial chain's pieces are joined in file order through `SPATIAL_IN`, and its uniforms are its declared parameters, so a planted piece missing one fails the ledger's check; a refused compile leaves the spatial layer out, compiles once more, logs one line and keeps the head on the GPU path; a chain past a cap at one display's size leaves that display's spatial layer at the no-op.
- **Tier 2 judges each filter.** A screen of the 16 colors, the 256-color cube, a gray ramp, antialiased text, a picture and the arrow is drawn with each filter at its defaults and at one setting that is not, and with the two-filter chain above. The CPU path is compared exactly with vtium's own goldens, which stay provisional until the maintainer reviews them. The GPU path under llvmpipe is compared with the CPU path's drawing within ADR-1303's tolerance for color filters. For spatial filters, which the CPU path does not draw, tier 2's own code applies each filter's formula, written again in Rust, to the CPU path's drawing, and the GPU path is compared with that within the tolerance; a planted sharpen that reads the wrong neighbour fails it.
- **The reach holds across tiles.** With the largest texture set small for the test, a frame of two tiles under sharpen matches the same frame in one tile within the tolerance, and a planted sharpen that declares a reach of 0 fails at the seam.
- **The pointer takes the look.** With a look in force, no cursor plane is set up and the arrow passes through the table: under the default tint the arrow's white is amber. With the look gone the cursor plane returns.
- **A 1-bit display ignores the look.** A 1-bit display with a look draws the same bytes as with none, and logs one line.
- **Tier 3** gains one screen with a look in each variant: the 2D variant with a color look, compared exactly with vtium's own golden, and the GL variant with a color and a spatial look, within the tolerance, once the GL variant passes (ADR-1303 D11, part 7).
- **The minspec build has no filters.** A minspec build with `look=` set draws as with no look and logs one line.

## Open questions

1. **Where a head's look is chosen.** *Recommendation:* `look = NAME` for each head in heads.conf, a default `look=` in vtium.conf, and `look = none` to give one head none. *Defence:* a head already carries its own choices beside `profile =` (ADR-1401 D3), and a head's displays are often different panels, which want different corrections. A look in its own file can serve several heads and is swapped by one line; filter keys written straight into vtium.conf and heads.conf would repeat a panel's correction in each head that wants it, and mix up to a dozen keys for each filter into the files that place devices.
2. **Every color filter is a table.** *Recommendation:* yes, so a filter that cannot be a table does not exist in the color layer. *Defence:* one table gives one lookup a pixel on both paths however many filters run, and both paths read the same bytes, which keeps the GPU path within ADR-1303's one level; the identity table returns all 16,777,216 colors unchanged (D2, `basis`). The cost is the table's approximation near black, up to 12 levels at a gamma of 2.0 among the darkest 7 levels, and that a step needing anything but the pixel's own color, such as dithering, is not a color filter.
3. **A `cube` filter that reads a `.cube` file.** *Recommendation:* yes, from `/etc/vtium/looks/NAME.cube`, owned by root and installed by root, by hand or through vtium-config's `apply`, sizes 2 to 65 (D9). *Defence:* it lets a correction measured by a calibration tool reach the panel without adding any way to run code, since the file is three numbers a line in a grammar Adobe published, which vtium parses strictly and within bounds. A file larger than 33 a side is resampled into the merged table, which loses detail between its nodes.
4. **Filters behind a cargo feature that the minspec build leaves off.** *Recommendation:* yes, `filters` (D10). *Defence:* the minspec build is judged by its size, and the filter code, the readers of vtlooks and `.cube` files, the table and its lookup, does nothing for a board with no look; the `look` key stays readable in every build, so one configuration serves every build. The cost is that a minspec board corrects its panel's color only in a larger build.
5. **A look named by a profile.** *Recommendation:* the head names its look now; once profiles have files, a profile may name a look too, and a head's own `look =` wins over it. *Defence:* ADR-1200 D16 describes a profile as a display's capabilities with the options that suit that display, which fits a panel's correction, but profiles have no file of their own yet (ADR-1401 D3), so the head is the only place for a look today.
6. **A 1-bit display ignores the look.** *Recommendation:* yes (D7). *Defence:* vtium lights a 1-bit display's pixels by each cell's colors and cuts coverage at half (`crates/vtium-render/src/mono.rs`), and a filter before the cut can carry the whole screen across it, as a tint that darkens white below 128 blanks every lit pixel. Gray panels, drawn as color, take the look.
7. **Curvature in the first set of filters.** *Recommendation:* later, after scanlines and sharpen. *Defence:* curvature is the one filter whose reach grows with the display, a bend of 5 per cent of a 3840-pixel width reading 192 pixels away, and the one that blends between pixels, so it tests the reach cap and the tolerance hardest. It fits D3's contract unchanged, so adding it needs no new decision.
8. **Trying a look on the live console from vtium-config.** *Recommendation:* not in the first slice: vtium-config previews the color layer as a picture, and a look takes effect when it is saved and vtium next reads its configuration. *Defence:* the live font switch rests on OSC 50, a sequence other terminals already define (ADR-1201), while a live look switch would need a sequence of vtium's own and a way back if the tool dies, which ADR-1201 D4 had to design for fonts. Until ADR-1401's reload is built, "next reads" means the next start of vtium.

## Consequences

### Positive

- A panel's color, a viewer's color vision and a taste for an amber phosphor are each a vtlook, with no build of vtium's own, and a look is shared between heads by name.
- The color layer works on the CPU path, so a board with no GPU corrects color too.
- One declaration feeds the reader, `check-config`, vtium-config and both paths, so a parameter cannot mean one thing in the file and another on the screen.
- However many color filters a look holds, each pixel costs one lookup.
- With no look, the frame and its cost are those of a build without filters.
- A failing filter costs its layer, with one log line, never the GPU path or the display.

### Negative

- An effect vtium does not build cannot be added by a file; each new filter is a change to vtium.
- The table approximates a curve that bends hard near black: a gamma of 2.0 is off by up to 12 levels among the darkest 7 levels of 256.
- The CPU path's color layer costs a lookup for each pixel it redraws, up to 59 ms for a 1920x1080 frame of all-different colors on a fast desktop; a terminal's screen of mostly runs costs a few milliseconds there, and by estimate several times that on an ARM board, which the slice that builds the layer measures.
- Spatial filters need the GPU path, which `hwaccel` leaves off by default, and the CPU path skips them.
- A head with a look draws its pointer in the frame, so pointer moves cost redraws, and whole presents under a spatial filter that reads neighbours.
- vtium-settings depends on vtium-render under `filters`, for the declarations.
- The GPU path keeps a 280 KiB table for each head with a color look, and the CPU path 421 KiB.

### Neutral

- A `.cube` file larger than 33 a side is resampled.
- ADR-1303's references to the frame shader's record point here once this record is accepted (What ADR-1303 changes to fit).
- ADR-1401 D3's head keys gain `look`.
- A look takes effect at vtium's next start until ADR-1401's reload is built.

## Alternatives Considered

- **Shaders installed as files,** GLSL a user writes and vtium compiles into present, as video players and emulators allow. Rejected by the maintainer (2026-11-03): no filter code is installed from outside. A file of code compiled by the console is a new way to run a user's code in a root process's GPU context, and its cost and its reading of the time could not be bounded by a declaration vtium checks.
- **Each color filter as its own GLSL function and its own Rust function,** run in turn for every pixel. Not taken (open question 2): two implementations of each filter to keep in step, a cost that grows with each filter, and the two paths' arithmetic parting by more than a table's.
- **A table of 65 a side.** Not taken: it lowers the worst error at a gamma of 2.0 from 12 levels to 8, for a table of 2.1 MiB on the GPU and 3.1 MiB on the CPU against 280 KiB and 421 KiB (`basis`).
- **A curve for each channel before the table,** 256 entries a channel, as color pipelines put a shaper before a 3D table. Not taken: it removes the error near black for filters that act on each channel alone, at the cost of a second table and a rule for which filters fold into it; the error it removes lies in the darkest 3 per cent of levels.
- **A pass for each filter.** Not taken, as ADR-1303 did not take it: a pass reads and writes the whole frame, and the chain in present's one pass costs only the reads it declares.
- **The hardware pointer kept for a color-only look, its picture run through the table.** Not taken: two rules where one does, and the arrow would still miss the spatial filters.
- **Turning off only the filter that fails.** Not taken: half of a correction chain can look worse than none, and the log already names the filter.
- **The filters' declarations in vtium-settings and their work in vtium-render, matched by name.** Not taken: each filter would live in two crates, held in step by a test; with the declaration beside its code, a filter is one module, and vtium-settings takes the declarations as it takes vtium-font's constants.
- **A look for each display,** set by the head file's rules. Not taken now: a head is the unit configuration chooses, and a mirrored head's displays show one screen. A later record can add a display's own look.
- **Filters on a 1-bit display, before the cut.** Not taken (open question 6).
- **Spatial filters on the CPU path.** Not taken: ADR-1303 D5 keeps the spatial slot on the GPU path, and a neighbour-reading filter over every redrawn pixel on the CPU would cost several times the color layer's lookup.
