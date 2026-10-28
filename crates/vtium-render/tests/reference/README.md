# The C reference's cell digests

`tests/reference.rs` and the sprite test in `src/deco.rs` hold vtium-render to these: each line is an FNV-1a 64 digest of what the C reference's own code drew, and vtium must draw the same pixels, but for the doubles and curls the render crate's quarter-cell departure redraws, which must differ.

They were taken from aaronsb/kmscon at `b1f7a7c5df563f3378584db7298753848a7a9a13` and aaronsb/libtsm at its `origin/main` of the same day, built with gcc -O2 on x86-64. Tier 2's goldens (vtium/vtium#17) are the full judge; these are whole cells, drawn the way the C reference drew a cell on an unturned display with no picture over it.

| Path | What it is |
|---|---|
| `refcells.c` | The program that prints the digests: unifont's glyphs through `kmscon_font_render`, the sprites through `kmscon_deco_render`, decorated cells composed with `bbulk.c`'s `compose`, and every cell blended with `drm2d_render.c`'s `blend_mask` and `blend_argb`. Its own part is the order a cell is drawn in, transcribed from `bbulk_draw_cell`. |
| `screens.tsv` | The screens, one cell a line, which `refcells.c` and `reference.rs` both read. |
| `cells.tsv` | Tab-separated. `deco <kind> <w> <h> <digest>`: each decoration sprite (1 single, 2 double, 3 curly, 4 dotted, 5 dashed, 6 strike) at every cell size from 1x1 to 24x48, with the lines `kmscon_font_get_deco` derives for the height; the digest covers the width and height as little-endian u32, then the coverage bytes. `screen <name> <digest>`: each screen, covering its width and height, then each `0x00RRGGBB` pixel as a little-endian u32, row by row. |

No gate, test or build compiles `refcells.c` or reads a checkout of the C reference: the tests read `cells.tsv` and `screens.tsv` alone. `refcells.c` stays as the record of how the digests were taken, from a checkout of the C reference at `$K`, aaronsb/libtsm at `$K/../libtsm`, and this directory as `$R`, with zlib:

```sh
mkdir build && cd build
printf '#include <stddef.h>\nextern const char *_binary_font_unifont_data_start;\nextern size_t _binary_font_unifont_data_size;\n' > font_unifont_data.bin.h
{ sed -n '/^static inline uint_fast32_t div255/,/^int drm2d_display_blend/p' $K/src/video/drm2d_render.c | sed '$d'
  sed -n '/^static inline uint32_t over_div255/,/^\/\* paint sprite @kind/p' $K/src/render/bbulk.c | sed '$d'; } > blend.inc
gcc -O2 -std=gnu11 -I$K/src -I$K/../libtsm/src -I. -w -o refcells $R/refcells.c -lz -lm
gcc -O2 -o genunifont $K/src/font/genunifont.c -lz
./genunifont unifont.bin $K/src/font/font_unifont_data.hex
./refcells unifont.bin $R/screens.tsv > $R/cells.tsv
```

A changed digest is a change to what vtium draws: review it as a golden change, with what moved and why.
