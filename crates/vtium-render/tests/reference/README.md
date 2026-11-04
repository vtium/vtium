# Cell digests

`tests/reference.rs` and the sprite test in `src/deco.rs` hold vtium-render to these: each line is an FNV-1a 64 digest of what the C reference's own code drew, and vtium must draw the same pixels, but for the doubles and curls the render crate's quarter-cell departure redraws and the dashes its dash departure redraws, which must differ, and the `decorations` screen, which must be the C reference's but for its dashes.

The digests were taken from the C reference's code at kmscon `b1f7a7c5` and libtsm of the same day, built with gcc -O2 on x86-64. They are vtium's goldens now. Tier 2's goldens (vtium/vtium#17) are the full judge; these are whole cells, drawn the way the C reference drew a cell on an unturned display with no picture over it.

| Path | What it is |
|---|---|
| `screens.tsv` | The screens, one cell a line, which `reference.rs` reads. |
| `cells.tsv` | Tab-separated. `deco <kind> <w> <h> <digest>`: each decoration sprite (1 single, 2 double, 3 curly, 4 dotted, 5 dashed, 6 strike) at every cell size from 1x1 to 24x48, with the lines `kmscon_font_get_deco` derives for the height; the digest covers the width and height as little-endian u32, then the coverage bytes. `screen <name> <digest>`: each screen, covering its width and height, then each `0x00RRGGBB` pixel as a little-endian u32, row by row. |

No gate, test or build reads a checkout of the C reference: the tests read `cells.tsv` and `screens.tsv` alone.

## Changing a digest

A changed digest is a change to what vtium draws, so it is a golden change. When vtium deliberately draws other pixels, the sprite test in `src/deco.rs` lists each `deco` line whose digest differs, and `tests/reference.rs` names each screen that differs, each with the digest vtium draws. The new digests go into `cells.tsv` in a commit of their own, which lists each line that moved and why, and the maintainer reviews it.
