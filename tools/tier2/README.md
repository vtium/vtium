# tier2

Tier 2 for vtium: the renderer draws known screens into memory, and each is compared pixel for pixel with its golden (vtium/vtium#17). The bytes go through `vtium-term` and the frames through `vtium-render`, into a double-buffered display in memory, as the C reference's `tests/test_pixels.c` drove libtsm and bbulk. The goldens are the judge: a difference from a golden past its sidecar's tolerance fails the case.

The crate is a workspace member, so `cargo test --workspace` builds the tool and runs its self-tests. Every file the run reads is in this tree, and the goldens are in `goldens/`. The run stays out of the default test run, in a Makefile target that CI's `tier2` job runs on every pull request and whose report and images it keeps:

```
make tier2                       # every screen against its golden
target/release/tier2 render      # draw every screen into out/render, judging nothing
target/release/tier2 compare GOLDEN ACTUAL [--tol FILE] [--diff OUT.ppm]
```

`run` takes `--out DIR` (default `out/` here, which git ignores). It writes each drawn screen to `out/actual/`, each difference to `out/diff/`, and the report to `out/report.json`, deleting any earlier report first. The report's `tools` names vtium's commit as `vtium`, `+dirty` with uncommitted changes, which `make tier2` passes in `T2_VTIUM_REV` as tier 3's report records it; a run started by hand records none. It exits 0 when every case passes, 1 when one does not, and 2 when it refuses to run. `compare` exits 0 on pass or near, 1 on fail and 2 on an input it refuses. Nothing here writes a golden.

| Path | What it is |
|---|---|
| `src/screens.rs` | The screens, with where each one's golden lives. |
| `src/draw.rs` | Drawing a screen: the terminal, the frames, the two buffers, and the configurations that must draw the same bytes. |
| `src/notices.rs` | The words of the overlay screens, each from the console function that gives it. |
| `src/colorset.rs` | The vitrified set `emoji-rgba` draws with, built in memory. |
| `src/pictures.rs` | The picture screens' kitty and sixel strings, generated from pixel rules. |
| `src/ppm.rs` | Reading and writing binary PPM, refusing what it cannot judge. |
| `src/tol.rs` | Tolerance sidecars and their masks. |
| `src/compare.rs` | The comparison, its verdict, its regions and the diff image. |
| `src/report.rs` | The report's JSON. |
| `src/runner.rs` | The lists, their checks, and judging every case. |
| `src/gpu.rs` | The GPU path's screens: each drawn through `Renderer::draw_gpu` on a GL and judged against the CPU path's drawing within its own sidecar. |
| `gpu/` | The GPU path's sidecars, `<screen>.tol`, one for each screen the GPU path draws: exact for now, `max_delta=0` and `max_pixels=0`, the maintainer's decision of 2026-11-03, and provisional. |
| `src/bin/tier2.rs` | The commands. |
| `pending.tsv` | Each screen tier 2 owes and no screen draws yet, with the issue that owes it: a case the run reports as pending. |
| `provisional.tsv` | vtium's goldens the operator has not reviewed. |
| `goldens/` | The goldens of tier 2's screens: see its README. |
| `tests/` | The self-tests: each malformed input refused, and each verdict of the run. |

## The screens

The C reference's tier 2 drew every screen with an outline font, DejaVu Sans Mono through FreeType, with Noto's CJK and color emoji fonts. Tier 2 draws PSF fonts, unifont and sets built in memory, never an outline font of the system's, so a C golden judges vtium only where no outline glyph shows: `blocks` and `blocks-spaced`, which hold only box-drawing and block sprites and spaces. They are drawn in a face that holds the space alone, in the cell DejaVu Sans Mono gave (9x16 at 16 pixels; 12x28 at 22 pixels with 6 rows of spacing), and a screen drawn that way is refused if any cell holds a character that is not a sprite or a space. Every other C golden is listed in `pending.tsv`: its screens are re-baselined when the Rust font layer lands (kmscon `ADR-304` §6, vtium/vtium#24), <!-- adr-cite-ignore: kmscon's record --> those with pictures among them, since their pictures stand over outline-font text. A golden that no screen draws is `missing`, and fails. A screen that takes up a pending row adds its golden to `goldens/`, provisional until the maintainer reviews it, and the row goes.

vtium's own screens carry the harness work the C reference had queued, drawn in unifont:

| Screen | What it covers |
|---|---|
| `edges`, `edges-right`, `edges-upside-down`, `edges-left` | A display 5 pixels wider and 3 taller than its whole cells, and a wide character whose second column the display lacks, in each orientation (aaronsb/kmscon#8, #118) |
| `erase-pen` | Erases, an insert and a scroll under a pen of bold, underline, strike and a red background, which leave plain red cells (aaronsb/kmscon#122, aaronsb/libtsm#13) |
| `vs16-unifont`, `vs16-wcwidth` | The C reference's `vs16` screen by the Unicode width method and by wcwidth (aaronsb/kmscon#122) |
| `wide-deco-unifont` | The C reference's `wide-deco` screen, its caption saying the last-column character wraps (aaronsb/kmscon#70) |
| `redraw-unifont` | Four frames into two buffers, which must end equal to each other and to a fresh draw (aaronsb/kmscon#8) |
| `small-320x240` | minspec's display floor, a 320x240 display in an 8x8 PSF2 font as 40 by 30 cells: a shell session with a line that wraps, sprites, colors, decorations plain and in color, characters the font lacks, and text in the last column and row (vtium/vtium#75, #119) |
| `small-640x480` | A 640x480 display in an 8x16 PSF1 font as 80 by 30 cells, where everything vtium supports is usable: the same parts at 80 columns, with more styles and color ramps (vtium/vtium#75, #119) |
| `small-320x240-1bit`, `small-640x480-1bit`, `palette-unifont-1bit`, `underline-unifont-1bit` | The same screens on a 1-bit display, each cell lit or unlit by its colors and every pixel black or white (vtium/vtium#135) |
| `light-theme-1bit` | A light theme set by OSC 10 and 11 on a 1-bit display: the screen lit and the default text unlit, colored cells flipping from it (vtium/vtium#135) |
| `osd-full`, `osd-minimal` | vtium's on-screen display over a shell session in unifont: the full line in its status box across the top, and the minimal dot in the top-right cell, each drawn as the console draws it over a frame (ADR-1401 D5, vtium/vtium#169) |
| `notice-no-keyboard`, `notice-anti-lockout`, `notice-rescue`, `notice-rescue-no-head`, `notice-oneshot`, `notice-no-console-opened`, `notice-head-file`, `notice-head-file-two`, `osd-full`, `osd-full-other`, `osd-minimal`, `osd-minimal-other`, `osd-full-notice`, `osd-minimal-notice` and `notices-four`, each ending `-800x480` and `-3840x540` | Every message bar vtium draws, over a shell session, at two display sizes: each notice alone, the on-screen display's line on the active head and on another and its dot on each, the line and the dot each with a notice, and the four notices a head shows at most (ADR-1401 D5, D7, D8, D9, vtium/vtium#156, vtium/vtium#169). A session that keeps ending as soon as it starts makes vtium exit with no notice under the open policy; under the closed policy vtium holds its VT and draws `notice::stopped()` on the boot display, which `notice-alone-stopped` draws. The small display is 800x480 in unifont's 8x16 cells. The large one has the 24x54 cell of a 7680x2160 display and is that display's top band, 3840x540 as 160 by 10 cells, where the bars are, since a whole 7680x2160 golden would be 50 MB; unifont is drawn in it as vtium-config places a bitmap font in a larger cell, tripled on each cell's bottom rows. The words come from the console's own functions, and the console's own overlay draws them, so a reworded notice moves its golden |
| `panic-800x480`, `panic-3840x540`, `panic-320x240`, and `notice-alone-no-display`, `notice-alone-partly` and `notice-alone-stopped`, each ending `-800x480` and `-3840x540` | The screen the display guard lights for a notice alone under the closed policy, with no session behind it and no cursor, so the box stands on an empty screen (ADR-1401 D1, D7, D9). The panic screen is the report on a closed vtium.conf whose sections are broken: its console line, that no head can run, the file's first three errors with their lines, the count of the rest, and what to do next; on minspec's 320x240 floor it lists the errors that fit and counts the rest, as the notice screen draws a report. The others are `No head has a connected display.`, a partly broken closed file's console line and the crash loop's notice, each alone. The panic screen comes from the start's own reading of a vtium.conf's bytes, and the partly broken file's line from its outcome's console line |
| `scheme-light-notice`, `scheme-light-osd`, `scheme-dark-notice`, `scheme-dark-osd`, `scheme-color-blind-safe-notice`, `scheme-color-blind-safe-osd` | The three other color schemes the example configuration carries for the notices and the on-screen display, `light`, `dark` and `color-blind-safe`, so they can be compared side by side with each other and with the default colors of `osd-minimal-notice-800x480` and `osd-full-800x480` (ADR-1401 D5). Each scheme draws twice over a shell session, 100 by 8 cells in unifont, `light` on a light terminal set by OSC 10 and 11: the notice that no keyboard was found with the minimal dot, which shows the notice's three colors and the on-screen display's text and box, and the full line alone, which shows the on-screen display's three. The colors come from the key table's schemes, which the example's lines are tested to set |
| `vitrified-320x240`, `vitrified-640x480` | The two `small-` screens drawn from the glyphs vtium-config's engine, the one the helper runs, generates from a frit naming each test font, with the built-in Unscii-8 or unifont after it filling what the font lacks (ADR-1301 D2, vtium/vtium#126, vtium/vtium#145) |
| `ascii-unifont`, `bold-unifont`, `palette-unifont`, `inverse-unifont`, `cjk-unifont`, `box-unifont`, `underline-unifont`, `strike-unifont`, `deco-config-unifont`, `deco-force-unifont`, `combining-unifont`, `mosaic-unifont`, `emoji-unifont` | The C reference's text screens of the same names, byte for byte, in unifont, so tier 2 sees text rendering before outline fonts land; `deco-config` and `deco-force` set `underline-color=196`, `strike-color=#00c0ff` and, for `deco-force`, `force-decoration-colors` |
| `link-hover-unifont`, `link-hover-unifont-1bit` | Links (OSC 8) with the pointer over one whose pieces on several rows share an id, under each style a cell can carry and wrapped from the last column, in color and on a 1-bit display: the hovered link's dashed underline (ADR-1002, "Drawing") |
| `emoji-rgba` | Color glyphs from a premultiplied RGBA strike beside a 1-bit text strike at one cell: between letters and in bold, on colored backgrounds, reversed, under each underline and strike, the heart in its narrow presentation, an emoji the set lacks, and one wrapped from the last column (vtium/vtium#23) |
| `pictures-kitty`, `pictures-kitty-right`, `pictures-selection`, `pictures-sixel`, `pictures-budget`, `pictures-tiled` | Pictures fed as kitty and sixel strings through the terminal's own handlers (ADR-1100, ADR-1303 D2): kitty pictures at their own size, shrunk by 1.5, 2.5, 4 and 8, grown by 2 and 3, and shrunk across while grown down, below the backgrounds, below the text and above it, opaque and half transparent, unturned and on a display turned right with odd margins, and under a selection; sixels made in a 10x20 cell and drawn in 8x16; a 160x64 display whose bound holds neither one picture's own pixels nor another's drawn copy; and pictures wider and taller than the 64-pixel textures the GPU path is held to for that screen |

The two `small-` screens draw with the PSF fonts `tools/testfont` writes, read through vtium-font's PSF loader; the two `vitrified-` screens with the same fonts' glyphs generated by vtium-config's engine at each font's own cell; and `emoji-rgba` with a vitrified set `src/colorset.rs` builds in memory and reads as the store's sets are read: unifont's printable ASCII as a 1-bit strike, and a grin, a rocket and a heart as a premultiplied RGBA strike at the same 8x16 cell, each two cells wide with edges of partial alpha; no font file of the system's reaches tier 2. A self-test draws both `small-` screens from their fonts' generated glyphs with nothing after them and requires the PSF loader's pixels, so those goldens judge the engine too. Each screen sets its own size, display and orientation (aaronsb/kmscon#10). Each is drawn with the default configuration and again with a cache of no glyphs, with the performance profile and with glyphs stored at 8 bits and at their drawn size, which ADR-1300 says change speed and never pixels: a drawing that differs is an error. A screen with a second part is drawn as four frames into two buffers, whose ages follow the swaps; both must hold the same picture at the end, and the picture a fresh renderer draws.

## The judge

A case is compared with its golden exactly, outside any mask its sidecar gives. The comparison, its verdicts and its regions are tier 3's (`tools/tier3/t3img.py`), so the two tiers judge images one way (aaronsb/kmscon#18). A sidecar `<name>.tol` beside a golden holds `max_delta=`, `max_pixels=` and `mask=x0,y0,x1,y1` lines, masks end-exclusive; no sidecar means exact.

| Status | When |
|---|---|
| `pass` | identical outside the masks |
| `near` | different within both of the sidecar's limits |
| `pending` | a screen `pending.tsv` lists |
| `provisional` | a golden in `provisional.tsv`, not yet reviewed: compared, and the status it would get noted, never failing |
| `fail` | different past the tolerance |
| `missing` | no golden, or a golden no screen draws |
| `error` | the screen did not draw, or its golden or sidecar was refused |

Each list is checked before any case runs: a pending row that a screen draws or whose golden `goldens/` holds, a provisional golden no screen draws, and two screens of one name are each refused.

Refused rather than judged (aaronsb/kmscon#10): a PPM that is not P6 with a maxval of 255, with a side of 0 or past 16384, or with a raster shorter or longer than its header says; a sidecar line that is not `key=value`, an unknown or repeated key, a value that is not a decimal number, a `max_delta` past 255, one limit at 0 with the other not, and a mask that is empty, inverted or reaches past the golden. A mask that hides no difference is noted in the report.

## The GPU path

`run` judges each screen on the GPU path too, as the case `<screen>@gpu`, when tier 2 is built with its `gpu` feature, which `make tier2` turns on. It draws on Mesa's llvmpipe, through vtium-platform's software context and the binary's adapter (`vtium::gpu::PlatformGl`), and a run that cannot set llvmpipe up reports one `error` case and fails; CI's tier2 job installs Mesa's EGL and llvmpipe for it. A GPU case passes when its drawing equals the CPU path's, is `near` within its sidecar, fails past it, and is `pending`, with the reason, for a screen the GPU path does not draw yet; its drawing and difference go to `out/actual/` and `out/diff/` under the case's name. The GPU path is judged against the CPU path's drawing of the same screen, which the run judges exactly against the screen's golden (ADR-1303 D1, "How the GPU path is judged"). `src/gpu.rs` draws each screen as `draw` does, in every configuration that must draw the same bytes, through `Renderer::draw_gpu`: into two buffers the GL makes in memory, whose ages follow the swaps, a second part fed after two frames, and the console's overlay in a frame of its own after the last, as the GPU path takes it from the console. It reads the buffer shown last back and compares it with the CPU path's drawing within the GPU path's own sidecar, `gpu/<screen>.tol`; the golden's sidecar never judges the GPU path, and no mask hides a GPU difference. Each GPU sidecar is exact for now, `max_delta=0` and `max_pixels=0`: the maintainer decided on 2026-11-03 to judge the GPU path exactly while every screen it draws measures no difference from the CPU path's drawing on llvmpipe, as all 81 do, the six picture screens among them. A picture screen is judged a second time as `<screen>@gpu-copy`, with each picture at another size drawn from the CPU path's copy (ADR-1303 D2): that case reads no sidecar and is always judged exactly, since the copy step's textures hold the CPU path's pixels. A self-test holds each sidecar to that, so a sidecar that widens fails until the decision and the test change with it. The sidecars stay provisional: ADR-1303 proposed a starting tolerance of 1 level and 3 per cent of a screen's pixels for GPUs that round differently, and a GPU that needs it brings that question back to the maintainer.

The GPU path draws every orientation in color; a 1-bit screen waits for present's depth reduction, and `gpu::waits` says so. The console's overlay reaches the GPU path upright, in the text's view, as it takes it from the console: each area the CPU path's overlay covers is mapped back into the view (`Surface::view_area`), and each of its pixels read from the display pixel the CPU path turned it to (`Surface::place`). A self-test draws a notice and the focus mark at the three turns on llvmpipe, on the CPU path's pixels. The self-tests draw through the whole GPU path on the tests' stand-in GL, which draws nothing and reads back zeros, and see the comparison fail: the judge sees a GPU path that draws nothing.

## vtium's goldens

The goldens in `goldens/` are reviewed vtium output, kept byte for byte as the operator accepted them, which judge vtium as the copied goldens do; `goldens/README.md` gives who drew each and what it shows. No tool here draws a golden. A new screen's golden is vtium's output, lands in a commit of its own, and stays in `provisional.tsv` until the operator has reviewed it.

Moving a golden is a change to the judge: it lands in a commit of its own, with each moved value listed.

## Not yet

- **PNG.** Tier 3's goldens are PNG, which tier 3's own `t3img.py` reads; tier 2's compare reads PPM only, and reading PNG is its next step should it judge a tier 3 golden.
- **fbdev.** vtium draws fbdev's XRGB8888 framebuffer with the same renderer and the same pixels as drm2d; other layouts are converted by `vtium-platform`, whose tests hold the conversion (vtium/vtium#18).
- **The C reference's other text screens in unifont.** `emoji-mono` is the C reference's emoji screen with its color font turned off: in unifont it would draw as `emoji-unifont` does, and vtium's color glyphs are drawn by `emoji-rgba`, so it waits with the outline screens for vtium/vtium#24. `mosaic-spaced` and `emoji-spaced` set a cell taller than the font's line height, which vtium has no setting for. `osd` draws the render timer's figures over the screen, which `vtium-render` keeps (`RenderTimer::osd`) but does not draw.
- **The C reference's picture screens** (`img-*`, `kitty-*`, `sixel-vte`) draw their pictures over outline-font text, so they wait with the other outline screens for vtium/vtium#24; vtium's own `pictures-*` screens draw pictures over unifont meanwhile.
- **The GPU path's 1-bit screens.** `gpu::waits` keeps them pending until present reduces the depth (ADR-1301).
