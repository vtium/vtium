# vtium

A terminal for the Linux virtual console, drawn straight to the display through KMS/DRM with no display server: modern text styling, sixel and kitty graphics on a bare VT.

vtium is a Rust terminal inspired by [kmscon](https://github.com/kmscon/kmscon) and libtsm, by David Herrmann and their contributors (see [COPYING](COPYING)). It carries forward the work of the [aaronsb/kmscon](https://github.com/aaronsb/kmscon) fork, which added underline styles and colours, grapheme clusters, colour emoji, sixel and the kitty graphics protocol, a GPU path with a CPU fallback, and a minimal build for small machines. vtium is a separate project: it is not upstream kmscon and not its Rust conversion.

## Status

vtium's minspec slice runs on real hardware. On the maintainer's machine it draws a login shell on a 7680x2160 DisplayPort display through drm2d, with nano, vim, btop and mouse reporting. The C fork stays the reference implementation, and each part is judged against it ([docs/architecture.md](docs/architecture.md#how-it-is-judged)):

- **Built:** drm2d and fbdev output with hotplug, DPMS blanking and VT switching; a CPU renderer; unifont and PSF bitmap fonts; the parser and screen, scrollback and Unicode; the pty, keyboard and pointer input, with selection, copy and paste; the C fork's configuration file and command line, its named palettes included.
- **Judged:** the parser and screen against the C reference's goldens (tier 1) and capability probes, the renderer pixel for pixel (tier 2), and the install tree booted on a VT in a 1 vCPU, 512 MB VM and compared screenshot by screenshot (tier 3): the minspec variants pass with their departures listed, in a 2.3 MiB binary that peaks at 6.7 MiB resident.
- **Not yet built:** outline fonts (#23), sixel and kitty graphics (#28, #48, #49) and the GPU path (#33). The plan and its order are in the tracker, #47.

`make vt-shell` runs your own shell in vtium on a spare VT, and stops when you exit it; [docs/vt-review.md](docs/vt-review.md) describes it and the other VT runs.

## Provenance

The maintainer's code in vtium is written by large language models, under the maintainer's review, and judged by the test harness. Each decision is recorded with the maintainer's words in the decision records.

Contributions are welcome, written by hand or by a large language model. Each pull request says how its code was written. Nothing is rejected for how it was written: every change is judged by the same tests and the same review. See [CONTRIBUTING](CONTRIBUTING.md).

## Licence

MIT, as kmscon and libtsm: see [LICENSE](LICENSE). [COPYING](COPYING) keeps the author lists of kmscon and libtsm and their original copyright notices.
