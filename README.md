# vtium

A terminal for the Linux virtual console, drawn straight to the display through KMS/DRM with no display server: modern text styling, sixel and kitty graphics on a bare VT.

vtium is a Rust port derived from [kmscon](https://github.com/kmscon/kmscon) and libtsm, by David Herrmann and their contributors (see [COPYING](COPYING)). It carries forward the work of the [aaronsb/kmscon](https://github.com/aaronsb/kmscon) fork, which added underline styles and colours, grapheme clusters, colour emoji, sixel and the kitty graphics protocol, a GPU path with a CPU fallback, and a minimal build for small machines. vtium is a separate project: it is not upstream kmscon and not its Rust conversion.

## Status

Not started. The plan is recorded as a decision record in the C fork, and the C fork stays the reference implementation until vtium passes the tests that cover each part:

- **Minspec first.** The first vtium is the minimal console as a vertical slice: drm2d or fbdev, a CPU renderer, bitmap fonts, the parser and screen, the pty and keyboard input. Text rendering through FreeType, images, the GPU path and the rest come after it, one layer at a time.
- **Judged by the fork's existing tests.** The fork's VM harness compares screenshots pixel for pixel and doesn't care what language it is testing. Its conformance and capability suites move behind a language-neutral interface first.

## Provenance

All code in vtium is written by large language models, under the review of its maintainer, and judged by the test harness. Each decision is recorded with the maintainer's words in the decision records.

## Licence

MIT, as kmscon and libtsm: see [LICENSE](LICENSE). [COPYING](COPYING) keeps the author lists of kmscon and libtsm and their original copyright notices.
