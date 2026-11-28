# Test fixtures

| File | What it is |
|---|---|
| `DejaVuSansMono-W-M-0.ttf`, `DejaVuSansMono-lam-shadda-fatha.ttf`, `DejaVuSansMono-ascii.ttf` | Copies of three subsets of DejaVu Sans Mono 2.37 from vtium-font-helper's `tests/fixtures/`, whose README says what each subset holds and how it was made. `tests/outline.rs` measures the W rule on the W, M and 0 subset and checks light and full hinting on the printable ASCII subset. `tests/shaping.rs` and `tests/serve.rs` draw lam with shadda and fatha. The TUI's tests and the console's tests read the copies too. |
| `NotoNaskhArabic-beh-fatha.ttf` | A subset of Noto Naskh Arabic 2.021 holding only beh (U+0628), fatha (U+064E) and U+200D, with the font's hinting instructions and its `GSUB`, `GPOS` and `GDEF` tables kept, so HarfRust shapes beh's joined forms as it does in the whole font: each as a dotless body and a separate dot, both from the beh. `tests/shaping.rs` checks on it that the helper keeps such a joining form, bare and with a fatha. |
| `NotoNaskhArabic-LICENSE.txt` | The SIL Open Font License 1.1, which covers the Noto Naskh Arabic subset, with the font's copyright line. |
| `DejaVuSansMono-LICENSE.txt` | DejaVu's licence, which covers the three DejaVu Sans Mono subsets above: the Bitstream Vera licence for the glyphs, with DejaVu's changes in the public domain. |

The Noto Naskh Arabic subset was made on 2026-11-03 from Arch Linux's `noto-fonts 1:2026.11.01-1`, whose `/usr/share/fonts/noto/NotoNaskhArabic-Regular.ttf` has the SHA-256 `6f0a92031367b2f5a2078fe9d24f3433122b61a0bad57c423aad8f3c39aa2e6e`, with fontTools 4.68.0:

```sh
pyftsubset NotoNaskhArabic-Regular.ttf --unicodes=U+0628,U+064E,U+200D --hinting \
  --name-IDs='*' --name-legacy --name-languages='*' --layout-features='*' \
  --output-file=NotoNaskhArabic-beh-fatha.ttf
```

Its SHA-256 was `4746666a031856abe44d1f8c3fd7b83ed4859c050f21a991981d855813ae401d`. The font names no Reserved Font Name, so the subset keeps its name, and it is used only by the tests and never installed.
