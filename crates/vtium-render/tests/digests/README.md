# Cell digests

`tests/digests.rs` and the sprite test in `src/deco.rs` hold vtium-render to these digests. Each line is an FNV-1a 64 digest of what vtium draws: a whole screen, or one decoration sprite at one cell size. They are vtium's goldens. Tier 2's goldens (vtium/vtium#17) are the full judge; these are whole cells, drawn on an unturned display with no picture over them.

| Path | What it is |
|---|---|
| `screens.tsv` | The screens, one cell a line, which `digests.rs` reads. |
| `cells.tsv` | Tab-separated. `deco <kind> <w> <h> <digest>`: each decoration sprite (1 single, 2 double, 3 curly, 4 dotted, 5 dashed, 6 strike) at every cell size from 1x1 to 24x48, with the lines `Decorations::for_height` gives for the height; the digest covers the width and height as little-endian u32, then the coverage bytes. `screen <name> <digest>`: each screen, covering its width and height, then each `0x00RRGGBB` pixel as a little-endian u32, row by row. |

At some sizes a line holds the digest of a sprite's earlier shape, which vtium no longer draws, and the tests check those lines a different way:

- A double or curl that would take more than a quarter of the cell draws as the single line, or as a curl lowered into the cell's bottom quarter (the crate README, "Behaviour"). At those sizes, the double at every width at heights 2 to 11 and the curl at the sizes `past_a_quarter` in `src/deco.rs` gives, the test checks that the sprite differs from its digest and is that fallback.
- A double underline thins its lines before it rises above the single line (the crate README, "Behaviour"). In a cell whose `for_height` lines are 2 or more rows thick, from 32 rows tall, the double is two lines one row thick. From 32 to 63 rows they lie in the cell's third row from the bottom and in its bottom row. From 64 to 79 rows the single line is 4 rows thick, so the gap between the lines widens to 2 rows. In a 64-row cell they lie in rows 60 and 63. The digests reach 48 rows, so at heights 32 to 48 the test checks that the sprite differs from its digest and is that shape.
- The dash runs from the cell's left edge for five eighths of it. Its digests are of a dash over each cell's middle two thirds, so in every cell 2 or more pixels wide the test checks that the sprite differs from its digest and is the dash.
- The `decorations` screen holds two dashed cells. `a_dashed_screen_matches_its_digest_but_for_its_dashes` draws it without the dashes, adds the earlier dash over columns 1 to 6 of each 8-pixel cell and matches the digest, then adds vtium's dash and matches what vtium draws.

## Changing a digest

A changed digest is a change to what vtium draws, so it is a golden change. When vtium draws other pixels, each test checks every line before it fails, and fails with the number of lines that differ. It writes every line of its kind as vtium draws it to a file, and the failure names that file:

- The sprite test in `src/deco.rs` (`VTIUM_DECO_DIGESTS=deco.tsv cargo test -p vtium-render --all-features --lib every_sprite_matches`) writes every `deco` line to the file that `VTIUM_DECO_DIGESTS` names, when a digest differs. Without the variable the test writes nothing. A line at a size where vtium draws a redrawn shape keeps its digest as it is.
- `tests/digests.rs` (`cargo test -p vtium-render --all-features --test digests`) writes every `screen` line to `digests/screens.tsv` in the test's scratch directory (`CARGO_TARGET_TMPDIR`, under the target directory). A dashed screen's line covers the screen with the earlier dash over columns 1 to 6 of each 8-pixel cell, which is the dash the stored digest covers.

A deliberate change copies those lines over the ones in `cells.tsv` in a golden commit of its own, which lists each line that moved and why, and the maintainer reviews it.
