# Test fixtures

| File | What it is |
|---|---|
| `DejaVuSansMono-W-M-0.ttf` | A subset of DejaVu Sans Mono 2.37 holding only `W`, `M` and `0`, with the font's TrueType hinting instructions kept (`fpgm`, `prep`, `cvt ` and each glyph's program). `tests/outline.rs` measures the W rule on it: full hinting snaps its `W` to 8 columns at 16 pixels, where the unhinted outline covers 9, so a W rule that measured hinted ink again would fail a test the gates run. |
| `DejaVuSansMono-lam-shadda-fatha.ttf` | A subset of DejaVu Sans Mono 2.37 holding only lam (U+0644), shadda (U+0651) and fatha (U+064E), with the font's hinting instructions and its `GSUB`, `GPOS` and `GDEF` tables kept, so HarfRust places the marks as it does in the whole font. `src/shaping/shape.rs` checks on it that lam with shadda and fatha is scaled to fit a 16-pixel row, though the lam alone has blank rows above it, and `tests/shaping.rs` checks the same stack drawn through the rasteriser. |
| `DejaVuSansMono-ascii.ttf` | A subset of DejaVu Sans Mono 2.37 holding printable ASCII, U+0020 to U+007E, with the font's TrueType hinting instructions kept. `tests/outline.rs` and `src/serve.rs` check light and full hinting on it (ADR-1200 D1). Light hinting gives a TrueType font to the automatic hinter, which measures a font's Latin letters, such as `o`, `x` and `H`, to find the heights it fits rows to, so it hints this subset as it hints the whole font; it leaves the `W`, `M` and `0` subset unhinted, since that subset holds none of those letters. |
| `NotoNaskhArabic-beh-fatha.ttf` | A subset of Noto Naskh Arabic 2.021 holding only beh (U+0628), fatha (U+064E) and U+200D, with the font's hinting instructions and its `GSUB`, `GPOS` and `GDEF` tables kept, so HarfRust shapes beh's joined forms as it does in the whole font: each as a dotless body and a separate dot, both from the beh. `tests/shaping.rs` checks on it that the helper keeps such a joining form, bare and with a fatha. |
| `NotoNaskhArabic-LICENSE.txt` | The SIL Open Font License 1.1, which covers the Noto Naskh Arabic subset, with the font's copyright line. |
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

Its SHA-256 was `896cdb69d42afb449e68eb66932c201a76aa8618020996771fd59ae8d4960f1c`.

The printable ASCII subset was made the same day from the same file with fontTools 4.68.0:

```sh
pyftsubset DejaVuSansMono.ttf --unicodes=20-7E --hinting \
  --name-IDs='*' --name-legacy --name-languages='*' \
  --drop-tables+=GSUB,GPOS,GDEF,FFTM --layout-features='' \
  --output-file=DejaVuSansMono-ascii.ttf
```

Its SHA-256 was `e5ab9e38fd9d4ddf7c52501e199dceb301f2eda6e58773c6a072e55006e99876`. The Bitstream Vera licence lets a modified font be distributed only under a name without "Bitstream" or "Vera"; the subset keeps DejaVu's own name, which the licence allows, and it is used only by the tests and never installed.

The Noto Naskh Arabic subset was made on 2026-11-03 from Arch Linux's `noto-fonts 1:2026.11.01-1`, whose `/usr/share/fonts/noto/NotoNaskhArabic-Regular.ttf` has the SHA-256 `6f0a92031367b2f5a2078fe9d24f3433122b61a0bad57c423aad8f3c39aa2e6e`, with fontTools 4.68.0:

```sh
pyftsubset NotoNaskhArabic-Regular.ttf --unicodes=U+0628,U+064E,U+200D --hinting \
  --name-IDs='*' --name-legacy --name-languages='*' --layout-features='*' \
  --output-file=NotoNaskhArabic-beh-fatha.ttf
```

Its SHA-256 was `4746666a031856abe44d1f8c3fd7b83ed4859c050f21a991981d855813ae401d`. The font names no Reserved Font Name, so the subset keeps its name, and it is used only by the tests and never installed.
