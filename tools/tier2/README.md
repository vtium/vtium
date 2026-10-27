# tier2

Tier 2 for vtium: the renderer draws known screens into memory, and each is compared pixel for pixel with its golden (vtium/vtium#17). The bytes go through `vtium-term` and the frames through `vtium-render`, into a double-buffered display in memory, as the C reference's `tests/test_pixels.c` drove libtsm and bbulk. The goldens are the judge: a difference from a golden is a listed departure, or the case fails.

The crate is a workspace member, so `cargo test --workspace` builds the tool and runs its self-tests, none of which needs a checkout of the C repositories. Reading the C reference's goldens does, so the run stays out of the default test run, in a Makefile target that CI's `tier2` job runs on every pull request and whose report and images it keeps:

```
make tier2                       # every screen against its golden (KMSCON=../kmscon)
target/release/tier2 render      # draw every screen into out/render, judging nothing
target/release/tier2 compare GOLDEN ACTUAL [--tol FILE] [--diff OUT.ppm]
target/release/tier2 screens     # vtium's unifont screens, for reference/draw.sh
```

`run` takes `--kmscon DIR` (default `../kmscon` beside this repository) and `--out DIR` (default `out/` here, which git ignores). It reads the pinned commit's goldens through `git archive` only, into `out/kmscon-<commit>/`, afresh on every run, so the checkout's branch and working tree never reach the judge; a checkout that lacks the commit is refused with the command that fetches it. It writes each drawn screen to `out/actual/`, each difference to `out/diff/`, and the report to `out/report.json`, deleting any earlier report first. It exits 0 when every case passes, 1 when one does not, and 2 when it refuses to run. `compare` exits 0 on pass or near, 1 on fail and 2 on an input it refuses. Nothing here writes a golden.

| Path | What it is |
|---|---|
| `src/screens.rs` | The screens, with where each one's golden lives. |
| `src/draw.rs` | Drawing a screen: the terminal, the frames, the two buffers, and the configurations that must draw the same bytes. |
| `src/ppm.rs` | Reading and writing binary PPM, refusing what it cannot judge. |
| `src/tol.rs` | Tolerance sidecars and their masks. |
| `src/compare.rs` | The comparison, its verdict, its regions and the diff image. |
| `src/report.rs` | The report's JSON. |
| `src/runner.rs` | The lists, extracting the pinned goldens, and judging every case. |
| `src/bin/tier2.rs` | The commands. |
| `pin.tsv` | The kmscon commit whose goldens judge vtium. |
| `pending.tsv` | Each golden of the C reference's that no screen draws yet, with the issue that owes it. |
| `departures.tsv` | Each case whose difference from its golden a departure in the render crate's README explains, with where the difference may lie. |
| `provisional.tsv` | vtium's goldens the operator has not reviewed. |
| `goldens/` | The goldens of vtium's screens, drawn by the C reference: see its README. |
| `reference/` | `draw.sh` and `pixels.c`, which draw vtium's unifont screens through the C reference's own pipeline. |
| `tests/` | The self-tests: each malformed input refused, and each verdict of the run. |

## The screens

The C reference's tier 2 drew every screen with an outline font, DejaVu Sans Mono through FreeType, with Noto's CJK and colour emoji fonts. vtium draws PSF and unifont until vtium/vtium#23, so a C golden judges vtium only where no outline glyph shows: `blocks` and `blocks-spaced`, which hold only box-drawing and block sprites and spaces. They are drawn in a face that holds the space alone, in the cell DejaVu Sans Mono gave (9x16 at 16 pixels; 12x28 at 22 pixels with 6 rows of spacing), and a screen drawn that way is refused if any cell holds a character that is not a sprite or a space. Every other C golden is listed in `pending.tsv`: its screens are re-baselined when the Rust font layer lands (ADR-304 §6, vtium/vtium#24), and those with pictures wait for vtium/vtium#30 too. A golden the pin brings that is neither drawn nor listed is `missing`, and fails.

vtium's own screens carry the harness work the C reference had queued, drawn in unifont:

| Screen | What it covers |
|---|---|
| `edges`, `edges-right`, `edges-upside-down`, `edges-left` | A display 5 pixels wider and 3 taller than its whole cells, and a wide character whose second column the display lacks, in each orientation (aaronsb/kmscon#8, #118) |
| `erase-pen` | Erases, an insert and a scroll under a pen of bold, underline, strike and a red background, which leave plain red cells (aaronsb/kmscon#122, aaronsb/libtsm#13) |
| `vs16-unifont`, `vs16-wcwidth` | The C reference's `vs16` screen by the Unicode width method and by wcwidth (aaronsb/kmscon#122) |
| `wide-deco-unifont` | The C reference's `wide-deco` screen, its caption saying the last-column character wraps (aaronsb/kmscon#70) |
| `redraw-unifont` | Four frames into two buffers, which must end equal to each other and to a fresh draw (aaronsb/kmscon#8) |

Each screen sets its own size, display and orientation (aaronsb/kmscon#10). Each is drawn with the default configuration and again with a cache of no glyphs, with the performance profile and with glyphs stored at 8 bits and at their drawn size, which ADR-1300 says change speed and never pixels: a drawing that differs is an error. A screen with a second part is drawn as four frames into two buffers, whose ages follow the swaps; both must hold the same picture at the end, and the picture a fresh renderer draws.

## The judge

A case is compared with its golden exactly, outside any mask its sidecar gives. The comparison, its verdicts and its regions are tier 3's (`fork/tier3/t3img.py` in the C reference), so the two tiers judge images one way (aaronsb/kmscon#18). A sidecar `<name>.tol` beside a golden holds `max_delta=`, `max_pixels=` and `mask=x0,y0,x1,y1` lines, masks end-exclusive; no sidecar means exact.

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

## vtium's goldens

The C reference draws the goldens of vtium's screens, so their author is not their judge. `reference/draw.sh OUT` reads the kmscon commit `pin.tsv` names and the libtsm commit that commit's `subprojects/libtsm.wrap` pins through `git archive`, builds them in a temporary directory, and compiles `reference/pixels.c` against them: libtsm parses each screen's bytes, `kmscon_text_draw` reads the screen, bbulk draws it with the unifont backend and drm2d's blend writes the buffer, as `tests/test_pixels.c` drew with FreeType. The screens come from `tier2 screens`, so both sides draw the same bytes. A new golden lands in a commit of its own and stays in `provisional.tsv` until the operator has reviewed it.

Each list is checked before any case runs: a pending golden the pin lacks or a screen draws, a departure whose case no screen draws or whose title the README does not list, a provisional golden no vtium screen draws, a vtium screen named as a C golden, and two screens of one name are each refused.

Refused rather than judged (aaronsb/kmscon#10): a PPM that is not P6 with a maxval of 255, with a side of 0 or past 16384, or with a raster shorter or longer than its header says; a sidecar line that is not `key=value`, an unknown or repeated key, a value that is not a decimal number, a `max_delta` past 255, one limit at 0 with the other not, and a mask that is empty, inverted or reaches past the golden. A mask that hides no difference is noted in the report.

## Not yet

- **PNG.** Tier 3's goldens are PNG; reading them is the shared compare's next step, when tier 3 judges vtium (vtium/vtium#22).
- **fbdev.** vtium draws fbdev's XRGB8888 framebuffer with the same renderer and the same pixels as drm2d; other layouts are converted by `vtium-platform`, whose tests hold the conversion (vtium/vtium#18).
- **The GPU path.** ADR-1300 asks tier 2 to draw each screen on the GPU path under llvmpipe as well (vtium/vtium#33).
