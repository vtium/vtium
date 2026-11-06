# Cell digests

`tests/digests.rs` and the sprite test in `src/deco.rs` hold vtium-render to these digests. Each line is an FNV-1a 64 digest of what vtium draws: a whole screen, or one decoration sprite at one cell size. They are vtium's goldens. Tier 2's goldens (vtium/vtium#17) are the full judge; these are whole cells, drawn on an unturned display with no picture over them.

| Path | What it is |
|---|---|
| `screens.tsv` | The screens, one cell a line, which `digests.rs` reads. |
| `cells.tsv` | Tab-separated. `deco <kind> <w> <h> <digest>`: each decoration sprite (1 single, 2 double, 3 curly, 4 dotted, 5 dashed, 6 strike) at every cell size from 1x1 to 24x48, with the lines `Decorations::for_height` gives for the height; the digest covers the width and height as little-endian u32, then the coverage bytes. `screen <name> <digest>`: each screen, covering its width and height, then each `0x00RRGGBB` pixel as a little-endian u32, row by row. |

At some sizes a line holds the digest of a sprite's earlier shape, which vtium no longer draws, and the tests check those lines a different way:

- A double or curl that would take more than a quarter of the cell draws as the single line, or as a curl lowered into the cell's bottom quarter (the crate README, "Behaviour"). At those sizes, the double at every width at heights 2 to 11 and the curl at the sizes `past_a_quarter` in `src/deco.rs` gives, the test checks that the sprite differs from its digest and is that fallback.
- The dash runs from the cell's left edge for five eighths of it. Its digests are of a dash over each cell's middle two thirds, so in every cell 2 or more pixels wide the test checks that the sprite differs from its digest and is the dash.
- The `decorations` screen holds two dashed cells. `a_dashed_screen_matches_its_digest_but_for_its_dashes` draws it without the dashes, adds the earlier dash over columns 1 to 6 of each 8-pixel cell and matches the digest, then adds vtium's dash and matches what vtium draws.

## Changing a digest

A changed digest is a change to what vtium draws, so it is a golden change. When vtium draws other pixels, each test checks every line before it fails, writes every line of its kind as vtium draws it to a file, and fails with the number of lines that differ and that file's path:

- The sprite test in `src/deco.rs` (`cargo test -p vtium-render --all-features --lib every_sprite_matches`) writes every `deco` line to `vtium-render-digests-<pid>/deco.tsv` in the system's temporary directory, where `<pid>` is the test's process id. A line at a size where vtium draws a redrawn shape keeps its digest as it is.
- `tests/digests.rs` (`cargo test -p vtium-render --all-features --test digests`) writes every `screen` line to `digests/screens.tsv` in the test's scratch directory (`CARGO_TARGET_TMPDIR`, under the target directory). A dashed screen's line covers the screen with the earlier dash over columns 1 to 6 of each 8-pixel cell, which is the dash the stored digest covers.

A deliberate change copies those lines over the ones in `cells.tsv` in a golden commit of its own, which lists each line that moved and why, and the maintainer reviews it.
