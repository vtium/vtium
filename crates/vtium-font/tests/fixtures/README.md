# vtium-font test fixtures

Fonts the tests load, and `reference/refdigest.c` drew to take the C reference's digests. Only fonts whose licence lets vtium redistribute them are here; kbd's other console fonts, most under the GPL, are read from the machine by the kbd sweep in `psf.rs` and never copied.

| Path | What it is | Source | Licence |
|---|---|---|---|
| `psf-test.psf` | A PSF2 font of 256 8x16 glyphs, glyph G's row R holding (G + R) & 0xff, with no unicode table | The C reference's `tests/fonts/psf-test.psf`, written by its `make-psf-test.py` | MIT, written for the C fork |
| `psf-test.psf.gz` | The same font gzipped with no timestamp | The C reference's `tests/fonts/psf-test.psf.gz` | MIT, written for the C fork |
| `ter-v16n.psf.gz` | Terminus 8x16: PSF1, 512 glyphs, with a unicode table | Terminus Font 4.49.1 (Arch `terminus-font 4.49.1-8`), as kbd installs it | SIL OFL 1.1, `LICENSE-Terminus.txt` |
| `ter-v22b.psf.gz` | Terminus bold 11x22: PSF2, rows of two bytes, 512 glyphs, with a unicode table | the same | the same |
| `ter-v32n.psf.gz` | Terminus 16x32: PSF2, 512 glyphs, with a unicode table | the same | the same |
| `LICENSE-Terminus.txt` | Terminus Font's copyright notice and the SIL OFL 1.1 | Arch `terminus-font`'s `LICENSE` | |
