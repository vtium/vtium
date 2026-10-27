# vtium-font integration tests

Each top-level `.rs` file is its own test crate, run by `cargo test -p vtium-font`. Every test cites the C reference line, test or specification its expectation comes from; a test of a departure says so.

| Path | What it is |
|---|---|
| `common/mod.rs` | Builders for crafted PSF1 and PSF2 fonts and gzip files, and a gunzip for the kbd sweep. |
| `psf.rs` | The PSF loader against crafted fonts, plain and gzipped: the C reference's `test_font_psf.c` cases (aaronsb/kmscon#229), each malformed header, truncation and table bound refused, the sequence fix (aaronsb/kmscon#231) and the other departures, the gzip framing, the fixtures, and every font in `/usr/share/kbd/consolefonts` when the machine has it. |
| `sprite.rs` | The sprites against a box model written apart from `src/sprite.rs`, as the C reference's `test_font_box.c` and `box_model.h` hold `font_box.c`: every cell size from 1x1 to 32x64 and 1000x2000, mosaic complements, and Braille layouts worked out by hand. |
| `legacy.rs` | The Symbols for Legacy Computing sprites against a model built from their Unicode names: every codepoint of the ranges drawn or left to the face by name, every cell size from 1x1 to 32x64 and three larger cells, and literal masks. |
| `policy.rs` | Each `Source` of a `SpritePolicy` against stand-in faces that cover a range fully, partly and not at all, the ranges against the sprites, the coverage count and its partial ranges, the default for a named font and for unifont, and the missing glyph. |
| `font.rs` | `Font`'s drawing of a cluster into its cells, the decoration lines, the two glyph formats and their bounds, and the built-in unifont. |
| `reference.rs` | Every sprite, PSF fixture glyph and unifont glyph against digests of what the C reference drew: the pixel-parity and same-bytes-on-every-host check. The page digests leave out the characters vtium draws as sprites beyond the C reference. |
| `reference/` | The C reference's digests and the program that takes them: see its README. |
| `fixtures/` | Fonts the tests load, with their licences: see its README. |
