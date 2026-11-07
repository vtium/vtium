# testfont

The PSF test fonts tier 2 and tier 3 draw the small displays with (vtium/vtium#75), written by vtium so no test depends on a console font a system happens to ship.

| File | Format | Cell | Draws |
|---|---|---|---|
| `vtium-test-8x8.psf` | PSF2 | 8x8 | a 320x240 display as 40 by 30 cells |
| `vtium-test-8x16.psf` | PSF1 | 8x16 | a 640x480 display as 80 by 30 cells |

Each holds 256 glyphs with a unicode table: printable ASCII, Latin-1 but the soft hyphen, quotes, dashes, the bullet, the ellipsis, the euro sign, four arrows and U+FFFD, then blank glyphs no codepoint maps to. The glyphs are vtium's unifont, under the SIL Open Font License 1.1 (`crates/vtium-font/data/OFL-1.1.txt`): as they are at 8x16, and folded to half height at 8x8, each 8x8 row the OR of the unifont rows `FOLD` in `src/lib.rs` names. Box drawing and block elements are left out, so vtium's sprites draw them.

```
cargo run -p vtium-testfont -- DIR   # write both fonts into DIR, the same bytes every run, with OFL-1.1.txt and NOTICE
```

Tier 2 builds the fonts in memory through the library; `make tier3` writes them into `target/tier3/fonts` for the guest. Nothing commits them: they follow from unifont and this crate. `cargo test -p vtium-testfont` reads each back through vtium-font's PSF loader, glyph for glyph.

## The TrueType test font writer

`src/ttf.rs` writes TrueType fonts in memory from a description of their glyphs: polygon outlines, a character map, color glyphs in COLR and CPAL or in CBDT, GSUB and GPOS lookups, and the `post` and `OS/2` tables' underline and strikeout. vtium-config's tests and its shaping fuzz target build their outline fonts with it, and tier 2 builds its ink test font with it (`tools/tier2/src/inkfont.rs`). Each of them depends on this crate, so none of them includes another crate's test file. A change to the writer changes the fonts that vtium-config's tests and tier 2's ink screens draw, so both judge it (vtium/vtium#339).
