# Glyph digests

`reference.rs` holds vtium-font to these: each line is an FNV-1a 64 digest of glyphs the C reference drew, and vtium must draw the same pixels. A digest covers each glyph's width and height as little-endian u32 and then its pixels, row by row, over the glyphs of the line in order.

The digests were taken from the C reference's code at kmscon `b1f7a7c5`, built with gcc -O2 on x86-64, which also drew the C reference's tier 2 goldens (vtium/vtium#17). They are vtium's goldens now. They were taken on one host and are checked on every other, so they also check that a glyph is the same bytes everywhere.

| Path | What it is |
|---|---|
| `box.tsv` | Tab-separated, `box <range> <w> <h> <thickness> <digest>`: `font_box.c`'s sprites, each range at every cell size from 1x1 to 20x40 with the thickness `kmscon_font_get_deco` gives, the box-drawing lines at 858 more sizes and thicknesses, and the other ranges at 1000x2000. |
| `psf.tsv` | `psf-glyphs <font> <height> <bold> <digest>`: every glyph of a fixture through `font_psf.c`; `render <font> <height> <page> <cells> <bold> <digest>`: a page of 256 codepoints through `kmscon_font_render`, less those `beyond_reference()` lists, which vtium draws as sprites where the C reference drew the font's glyph (vtium/vtium#27). Each fixture at 16, 32 and 48. |
| `unifont.tsv` | `unifont-glyphs <height> <bold> <page> <digest>`: every glyph unifont holds in a page, at its own width, through `font_unifont.c`; `render unifont ...`: as in `psf.tsv`, every page of plane 0 at 16 and a sample at 32. The surrogate pages D8-DF are skipped by the test, since a `char` cannot hold them. |

No gate, test or build reads a checkout of the C reference: the tests read the `.tsv` files alone.

## Changing a digest

A changed digest is a change to what vtium draws, so it is a golden change. When vtium deliberately draws other pixels, `cargo test -p vtium-font --test reference` fails and lists each line whose digest differs, with the digest vtium draws. The new digests go into the `.tsv` files in a commit of their own, which lists each line that moved and why, and the maintainer reviews it.
