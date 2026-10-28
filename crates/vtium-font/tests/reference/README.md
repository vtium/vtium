# The C reference's glyph digests

`reference.rs` holds vtium-font to these: each line is an FNV-1a 64 digest of glyphs the C reference drew, and vtium must draw the same pixels. A digest covers each glyph's width and height as little-endian u32 and then its pixels, row by row, over the glyphs of the line in order.

They were taken from aaronsb/kmscon at `b1f7a7c5df563f3378584db7298753848a7a9a13`, built with gcc -O2 on x86-64, and the C reference's tier 2 goldens (vtium/vtium#17) were drawn by the same code.

| Path | What it is |
|---|---|
| `refdigest.c` | The program that prints the digests, built once per backend against the C reference's sources. |
| `box.tsv` | Tab-separated, `box <range> <w> <h> <thickness> <digest>`: `font_box.c`'s sprites, each range at every cell size from 1x1 to 20x40 with the thickness `kmscon_font_get_deco` gives, the box-drawing lines at 858 more sizes and thicknesses, and the other ranges at 1000x2000. |
| `psf.tsv` | `psf-glyphs <font> <height> <bold> <digest>`: every glyph of a fixture through `font_psf.c`; `render <font> <height> <page> <cells> <bold> <digest>`: a page of 256 codepoints through `kmscon_font_render`, less those `beyond_reference()` lists, which vtium draws as sprites where the C reference drew the font's glyph (vtium/vtium#27). Each fixture at 16, 32 and 48. |
| `unifont.tsv` | `unifont-glyphs <height> <bold> <page> <digest>`: every glyph unifont holds in a page, at its own width, through `font_unifont.c`; `render unifont ...`: as in `psf.tsv`, every page of plane 0 at 16 and a sample at 32. The surrogate pages D8-DF are skipped by the test, since a `char` cannot hold them. |

No gate, test or build compiles `refdigest.c` or reads a checkout of the C reference: the tests read the `.tsv` files alone. `refdigest.c` stays as the record of how the digests were taken, from a checkout of the C reference at `$K` and this directory as `$R`, with zlib:

```sh
mkdir build && cd build
printf '#include <stddef.h>\nextern const char *_binary_font_unifont_data_start;\nextern size_t _binary_font_unifont_data_size;\n' > font_unifont_data.bin.h
cc="gcc -O2 -std=gnu11 -I$K/src -I. -w"
$cc -DREF_BOX -o ref_box $R/refdigest.c -lm
$cc -DREF_PSF -DKMSCON_PSF_GZ -o ref_psf $R/refdigest.c -lz
$cc -I$K/../libtsm/src -DREF_UNIFONT -o ref_unifont $R/refdigest.c -lz
gcc -O2 -o genunifont $K/src/font/genunifont.c -lz
./genunifont unifont.bin $K/src/font/font_unifont_data.hex
./ref_box > $R/box.tsv
./ref_psf $R/../fixtures/psf-test.psf $R/../fixtures/ter-v16n.psf.gz $R/../fixtures/ter-v22b.psf.gz $R/../fixtures/ter-v32n.psf.gz > $R/psf.tsv
./ref_unifont unifont.bin > $R/unifont.tsv
```

A changed digest is a change to what vtium draws: review it as a golden change, with what moved and why.
