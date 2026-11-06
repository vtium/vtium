# Glyph digests

`reference.rs` holds vtium-font to these: each line is an FNV-1a 64 digest of glyphs vtium draws, and a change to what vtium draws fails the test. A digest covers each glyph's width and height as little-endian u32 and then its pixels, row by row, over the glyphs of the line in order.

The digests are vtium's goldens. They were taken on x86-64 and are checked on every host, so they also check that a glyph is the same bytes everywhere.

| Path | What it is |
|---|---|
| `box.tsv` | Tab-separated, `box <range> <w> <h> <thickness> <digest>`: the sprites, each range at every cell size from 1x1 to 20x40 with the line thickness `Decorations::for_height` gives a face with no line metrics, the box-drawing lines at 858 more sizes and thicknesses, and the other ranges at 1000x2000. |
| `psf.tsv` | `psf-glyphs <font> <height> <bold> <digest>`: every glyph of a fixture as `Psf` draws it; `render <font> <height> <page> <cells> <bold> <digest>`: a page of 256 codepoints drawn through `Font::glyph`, less the Symbols for Legacy Computing sprites that `outside_digests()` lists and `legacy.rs` holds (vtium/vtium#27). Each fixture at 16, 32 and 48. |
| `unifont.tsv` | `unifont-glyphs <height> <bold> <page> <digest>`: every glyph unifont holds in a page, at its own width, as `Unifont` draws it; `render unifont ...`: as in `psf.tsv`, every page of plane 0 at 16 and a sample at 32. The surrogate pages D8-DF are skipped by the test, since a `char` cannot hold them. |

## Changing a digest

A changed digest is a change to what vtium draws, so it is a golden change. When vtium draws other pixels, `cargo test -p vtium-font --all-features --test reference` checks every line before it fails. For each file with a line that differs, the test writes the whole file as vtium draws it, every checked line with vtium's digest and every other line as it is, to `digests/<file>` in the test's scratch directory (`CARGO_TARGET_TMPDIR`, under the target directory), and fails with the number of lines that differ and that file's path. A deliberate change copies that file over the one here in a commit of its own, which lists each line that moved and why, and the maintainer reviews it.
