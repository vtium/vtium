# tier2

Tier 2 for vtium: the renderer draws known screens into memory, and each is compared pixel for pixel with its golden (vtium/vtium#17). The bytes go through `vtium-term` and the frames through `vtium-render`, into a double-buffered display in memory, as the C reference's `tests/test_pixels.c` drove libtsm and bbulk. The goldens are the judge: a difference from a golden is a listed departure, or the case fails.

The crate is a workspace member, so `cargo test --workspace` builds the tool and runs its self-tests. Every file the run reads is in this tree: vtium's goldens in `goldens/`, and the C reference's files in `reference/`, copied byte for byte from kmscon with each one's source and SHA-256 in `provenance.tsv`. The run stays out of the default test run, in a Makefile target that CI's `tier2` job runs on every pull request and whose report and images it keeps:

```
make tier2                       # every screen against its golden
target/release/tier2 render      # draw every screen into out/render, judging nothing
target/release/tier2 compare GOLDEN ACTUAL [--tol FILE] [--diff OUT.ppm]
make tier2-provenance            # the copies against their sources (KMSCON=../kmscon); not a gate
```

`run` takes `--out DIR` (default `out/` here, which git ignores). It first checks each file under `reference/` against its row of `provenance.tsv`, and refuses to run when one differs, is gone, or has no row. It writes each drawn screen to `out/actual/`, each difference to `out/diff/`, and the report to `out/report.json`, deleting any earlier report first. It exits 0 when every case passes, 1 when one does not, and 2 when it refuses to run. `compare` exits 0 on pass or near, 1 on fail and 2 on an input it refuses. Nothing here writes a golden.

`provenance` takes `--kmscon DIR` (default `../kmscon` beside this repository) and reads that checkout with `git show` and `git ls-tree` alone. It reports each copy that differs from its row, each row whose source at its commit does not hash to the row's SHA-256, each file of the C reference's `tests/goldens/tier2/` at that commit that `c-goldens.list` lacks, and each name the list gives that the commit lacks. It runs when the copies or the list change, as `make conformance-provenance` does for tier 1's copies; no gate runs it, since no gate reads a fork checkout. It exits 0 with no fault, 1 with one, and 2 when it refuses.

| Path | What it is |
|---|---|
| `src/screens.rs` | The screens, with where each one's golden lives. |
| `src/draw.rs` | Drawing a screen: the terminal, the frames, the two buffers, and the configurations that must draw the same bytes. |
| `src/ppm.rs` | Reading and writing binary PPM, refusing what it cannot judge. |
| `src/tol.rs` | Tolerance sidecars and their masks. |
| `src/compare.rs` | The comparison, its verdict, its regions and the diff image. |
| `src/report.rs` | The report's JSON. |
| `src/runner.rs` | The lists, the copies' checks, and judging every case. |
| `src/bin/tier2.rs` | The commands. |
| `provenance.tsv` | Each file under `reference/`: `PATH REPO COMMIT SOURCE SHA256`, the path from the repository root, as tier 1's `tools/conformance/provenance.tsv` gives them. |
| `c-goldens.list` | The file names of the C reference's `tests/goldens/tier2/` at the commit `provenance.tsv` names, so `run` knows every C golden without reading the fork. |
| `pending.tsv` | Each golden of the C reference's that no screen draws yet, with the issue that owes it. |
| `departures.tsv` | Each case whose difference from its golden a departure in the render crate's README explains, with where the difference may lie. |
| `provisional.tsv` | vtium's goldens the operator has not reviewed. |
| `goldens/` | The goldens of vtium's screens, reviewed vtium output: see its README. |
| `reference/` | The C reference's files tier 2 reads, copied from kmscon: the goldens of the screens ported from it in `goldens/`, and `test_pixels.c`, whose screens the ported ones must carry byte for byte. |
| `tests/` | The self-tests: each malformed input refused, and each verdict of the run. |

## The screens

The C reference's tier 2 drew every screen with an outline font, DejaVu Sans Mono through FreeType, with Noto's CJK and color emoji fonts. vtium draws PSF and unifont until vtium/vtium#23, so a C golden judges vtium only where no outline glyph shows: `blocks` and `blocks-spaced`, which hold only box-drawing and block sprites and spaces. They are drawn in a face that holds the space alone, in the cell DejaVu Sans Mono gave (9x16 at 16 pixels; 12x28 at 22 pixels with 6 rows of spacing), and a screen drawn that way is refused if any cell holds a character that is not a sprite or a space. Every other C golden is listed in `pending.tsv`: its screens are re-baselined when the Rust font layer lands (kmscon `ADR-304` §6, vtium/vtium#24), <!-- adr-cite-ignore: kmscon's record --> and those with pictures wait for vtium/vtium#30 too. A golden `c-goldens.list` gives that no screen draws and `pending.tsv` does not list is `missing`, and fails. A screen that takes up a pending golden copies it into `reference/goldens/` with its row in `provenance.tsv`, and its `pending.tsv` row goes.

vtium's own screens carry the harness work the C reference had queued, drawn in unifont:

| Screen | What it covers |
|---|---|
| `edges`, `edges-right`, `edges-upside-down`, `edges-left` | A display 5 pixels wider and 3 taller than its whole cells, and a wide character whose second column the display lacks, in each orientation (aaronsb/kmscon#8, #118) |
| `erase-pen` | Erases, an insert and a scroll under a pen of bold, underline, strike and a red background, which leave plain red cells (aaronsb/kmscon#122, aaronsb/libtsm#13) |
| `vs16-unifont`, `vs16-wcwidth` | The C reference's `vs16` screen by the Unicode width method and by wcwidth (aaronsb/kmscon#122) |
| `wide-deco-unifont` | The C reference's `wide-deco` screen, its caption saying the last-column character wraps (aaronsb/kmscon#70) |
| `redraw-unifont` | Four frames into two buffers, which must end equal to each other and to a fresh draw (aaronsb/kmscon#8) |
| `small-320x240` | minspec's display floor, a 320x240 display in an 8x8 PSF2 font as 40 by 30 cells: a shell session with a line that wraps, sprites, colors, decorations, characters the font lacks, and text in the last column and row (vtium/vtium#75) |
| `small-640x480` | A 640x480 display in an 8x16 PSF1 font as 80 by 30 cells, where everything vtium supports is usable: the same parts at 80 columns, with more styles and color ramps (vtium/vtium#75) |
| `ascii-unifont`, `bold-unifont`, `palette-unifont`, `inverse-unifont`, `cjk-unifont`, `box-unifont`, `underline-unifont`, `strike-unifont`, `deco-config-unifont`, `deco-force-unifont`, `combining-unifont`, `mosaic-unifont`, `emoji-unifont` | The C reference's text screens of the same names, byte for byte, in unifont, so tier 2 sees text rendering before outline fonts land; `deco-config` and `deco-force` set `underline-color=196`, `strike-color=#00c0ff` and, for `deco-force`, `force-decoration-colors` |

The two `small-` screens draw with the PSF fonts `tools/testfont` writes, read through vtium-font's PSF loader; no font file of the system's reaches tier 2. Each screen sets its own size, display and orientation (aaronsb/kmscon#10). Each is drawn with the default configuration and again with a cache of no glyphs, with the performance profile and with glyphs stored at 8 bits and at their drawn size, which ADR-1300 says change speed and never pixels: a drawing that differs is an error. A screen with a second part is drawn as four frames into two buffers, whose ages follow the swaps; both must hold the same picture at the end, and the picture a fresh renderer draws.

## The judge

A case is compared with its golden exactly, outside any mask its sidecar gives. The comparison, its verdicts and its regions are tier 3's (`tools/tier3/t3img.py`), so the two tiers judge images one way (aaronsb/kmscon#18). A sidecar `<name>.tol` beside a golden holds `max_delta=`, `max_pixels=` and `mask=x0,y0,x1,y1` lines, masks end-exclusive; no sidecar means exact.

| Status | When |
|---|---|
| `pass` | identical outside the masks |
| `near` | different within both of the sidecar's limits |
| `departure` | different only inside the rectangles `departures.tsv` gives for a departure the render crate's README lists |
| `pending` | a C golden `pending.tsv` lists |
| `provisional` | a golden in `provisional.tsv`, not yet reviewed: compared, and the status it would get noted, never failing |
| `fail` | different past the tolerance, outside a departure's rectangles, or listed as a departure and no longer different |
| `missing` | no golden, or a golden no screen draws |
| `error` | the screen did not draw, or its golden or sidecar was refused |

A departure's rectangles cover whole cells, the cells where its difference shows, so any difference inside them passes as the departure: a defect in a listed cell, such as an underline lost from the cut `中` in `edges-left`, is not seen until that cell's departure row goes or its rectangles shrink.

A screen ported from one of the C reference's carries its bytes exactly: `run` reads the copy of `tests/test_pixels.c` and refuses to run when a ported screen's bytes or second part differ from the C screen it names.

Each list is checked before any case runs: a copied or pending golden `c-goldens.list` lacks, a pending golden a screen draws or that is copied, a departure whose case no screen draws or whose title the README does not list, a provisional golden no vtium screen draws, a vtium screen named as a C golden, and two screens of one name are each refused.

Refused rather than judged (aaronsb/kmscon#10): a PPM that is not P6 with a maxval of 255, with a side of 0 or past 16384, or with a raster shorter or longer than its header says; a sidecar line that is not `key=value`, an unknown or repeated key, a value that is not a decimal number, a `max_delta` past 255, one limit at 0 with the other not, and a mask that is empty, inverted or reaches past the golden. A mask that hides no difference is noted in the report.

## vtium's goldens

The goldens in `goldens/` are reviewed vtium output, kept byte for byte as the operator accepted them, which judge vtium as the copied goldens do; `goldens/README.md` gives who drew each and what it shows. No tool here draws a golden. A new screen's golden is vtium's output, lands in a commit of its own, and stays in `provisional.tsv` until the operator has reviewed it.

Moving a golden is a change to the judge: it lands in a commit of its own, with each moved value listed. So does moving the copies to another kmscon commit, with the run's report before and after, and `make tier2-provenance` run on the new rows.

## Not yet

- **PNG.** Tier 3's goldens are PNG, which tier 3's own `t3img.py` reads; tier 2's compare reads PPM only, and reading PNG is its next step should it judge a tier 3 golden.
- **fbdev.** vtium draws fbdev's XRGB8888 framebuffer with the same renderer and the same pixels as drm2d; other layouts are converted by `vtium-platform`, whose tests hold the conversion (vtium/vtium#18).
- **The C reference's other text screens in unifont.** `emoji-mono` would draw as `emoji-unifont` does, since neither side has a color font to turn off. `mosaic-spaced` and `emoji-spaced` set a cell taller than the font's line height, which vtium has no setting for. `osd` draws the render timer's figures over the screen, which `vtium-render` keeps (`RenderTimer::osd`) but does not draw.
- **The picture screens** (`img-*`, `kitty-*`, `sixel-vte`) wait for pictures in the CPU compositor (vtium/vtium#30) and the sixel and kitty handlers (vtium/vtium#48, #49).
- **The GPU path.** ADR-1300 asks tier 2 to draw each screen on the GPU path under llvmpipe as well (vtium/vtium#33).
