# Test fixtures

| File | What it is |
|---|---|
| `DejaVuSansMono-W-M-0.ttf` | A subset of DejaVu Sans Mono 2.37 holding only `W`, `M` and `0`, with the font's TrueType hinting instructions kept (`fpgm`, `prep`, `cvt ` and each glyph's program). `tests/outline.rs` measures the W rule on it: full hinting snaps its `W` to 8 columns at 16 pixels, where the unhinted outline covers 9, so a W rule that measured hinted ink again would fail a test the gates run. |
| `DejaVuSansMono-lam-shadda-fatha.ttf` | A subset of DejaVu Sans Mono 2.37 holding only lam (U+0644), shadda (U+0651) and fatha (U+064E), with the font's hinting instructions and its `GSUB`, `GPOS` and `GDEF` tables kept, so HarfRust places the marks as it does in the whole font. `src/shaping/shape.rs` checks on it that lam with shadda and fatha is scaled to fit a 16-pixel row, though the lam alone has blank rows above it. |
| `DejaVuSansMono-LICENSE.txt` | DejaVu's licence, which covers the subset: the Bitstream Vera licence for the glyphs, with DejaVu's changes in the public domain. |

The subset was made on 2026-11-01 from Arch Linux's `ttf-dejavu 2.37+18+g9b5d1b2f-8`, whose `/usr/share/fonts/TTF/DejaVuSansMono.ttf` has the SHA-256 `b5babc084554ebdd142fe099d8b7c573ff161738bca913723d3e84f5c3df1c78`, with fontTools 4.67.0:

```sh
pyftsubset DejaVuSansMono.ttf --unicodes=U+0030,U+004D,U+0057 --hinting \
  --name-IDs='*' --name-legacy --name-languages='*' \
  --drop-tables+=GSUB,GPOS,GDEF,FFTM --layout-features='' \
  --output-file=DejaVuSansMono-W-M-0.ttf
```

The result's SHA-256 was `f3eb2bf012160c47f98ff69b7fa371cc187b0e79b77df2459b9dc5a7809a9976`.

The lam, shadda and fatha subset was made the same day from the same file with fontTools 4.68.0:

```sh
pyftsubset DejaVuSansMono.ttf --unicodes=U+0644,U+064E,U+0651 --hinting \
  --name-IDs='*' --name-legacy --name-languages='*' \
  --drop-tables+=FFTM --layout-features='*' \
  --output-file=DejaVuSansMono-lam-shadda-fatha.ttf
```

Its SHA-256 was `896cdb69d42afb449e68eb66932c201a76aa8618020996771fd59ae8d4960f1c`. The Bitstream Vera licence lets a modified font be distributed only under a name without "Bitstream" or "Vera"; the subset keeps DejaVu's own name, which the licence allows, and it is used only by the tests and never installed.
