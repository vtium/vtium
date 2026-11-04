# Cell digests

`tests/reference.rs` and the sprite test in `src/deco.rs` hold vtium-render to these: each line is an FNV-1a 64 digest of what the C reference's own code drew, and vtium must draw the same pixels, but for the doubles and curls the render crate's quarter-cell departure redraws and the dashes its dash departure redraws, which must differ, and the `decorations` screen, which must be the C reference's but for its dashes.

The digests were taken from the C reference's code at kmscon `b1f7a7c5` and libtsm of the same day, built with gcc -O2 on x86-64. They are vtium's goldens now. Tier 2's goldens (vtium/vtium#17) are the full judge; these are whole cells, drawn the way the C reference drew a cell on an unturned display with no picture over it.

| Path | What it is |
|---|---|
| `screens.tsv` | The screens, one cell a line, which `reference.rs` reads. |
| `cells.tsv` | Tab-separated. `deco <kind> <w> <h> <digest>`: each decoration sprite (1 single, 2 double, 3 curly, 4 dotted, 5 dashed, 6 strike) at every cell size from 1x1 to 24x48, with the lines `kmscon_font_get_deco` derives for the height; the digest covers the width and height as little-endian u32, then the coverage bytes. `screen <name> <digest>`: each screen, covering its width and height, then each `0x00RRGGBB` pixel as a little-endian u32, row by row. |

No gate, test or build reads a checkout of the C reference: the tests read `cells.tsv` and `screens.tsv` alone.

## Changing a digest

A changed digest is a change to what vtium draws, so it is a golden change. When vtium draws other pixels, each test checks every line before it fails, writes every line of its kind as vtium draws it to a file, and fails with the number of lines that differ and that file's path:

- The sprite test in `src/deco.rs` (`cargo test -p vtium-render --all-features --lib sprites_match`) writes every `deco` line to `vtium-render-digests/deco.tsv` in the system's temporary directory. A line the render crate redraws on purpose, a double or curl past a quarter of the cell or a dash, keeps its digest as it is.
- `tests/reference.rs` (`cargo test -p vtium-render --all-features --test reference`) writes every `screen` line to `digests/screens.tsv` in the test's scratch directory (`CARGO_TARGET_TMPDIR`, under the target directory). A dashed screen's line covers the screen with its dash over columns 1 to 6 of each 8-pixel cell, which is the dash the stored digest covers, as `a_dashed_screen_differs_from_the_c_reference_only_in_its_dashes` draws it.

A deliberate change copies those lines over the ones in `cells.tsv` in a commit of their own, which lists each line that moved and why, and the maintainer reviews it.
