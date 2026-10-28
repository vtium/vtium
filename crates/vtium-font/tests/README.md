# vtium-font integration tests

Each top-level `.rs` file is its own test crate, run by `cargo test -p vtium-font`. Every test cites the C reference line, test or specification its expectation comes from; a test of a departure says so.

| Path | What it is |
|---|---|
| `common/mod.rs` | Builders for crafted PSF1 and PSF2 fonts and gzip files, a gunzip for the kbd sweep, and a set's sections taken apart and put back together. |
| `psf.rs` | The PSF loader against crafted fonts, plain and gzipped: the C reference's `test_font_psf.c` cases (aaronsb/kmscon#229), each malformed header, truncation and table bound refused, the sequence fix (aaronsb/kmscon#231) and the other departures, the gzip framing, the fixtures, and every font in `/usr/share/kbd/consolefonts` when the machine has it. |
| `sprite.rs` | The sprites against a box model written apart from `src/sprite.rs`, as the C reference's `test_font_box.c` and `box_model.h` hold `font_box.c`: every cell size from 1x1 to 32x64 and 1000x2000, mosaic complements, and Braille layouts worked out by hand. |
| `legacy.rs` | The Symbols for Legacy Computing sprites against a model built from their Unicode names: every codepoint of the ranges drawn or left to the face by name, every cell size from 1x1 to 32x64 and three larger cells, and literal masks. |
| `policy.rs` | Each `Source` of a `SpritePolicy` against stand-in faces that cover a range fully, partly and not at all, the ranges against the sprites, the coverage count and its partial ranges, the default for a named font and for unifont, and the missing glyph. |
| `missing.rs` | `Missing::Hex` against literal masks of the operator's design, narrow and wide, up to U+FFFF and past it, and its scaling, centring and fallback. |
| `set.rs` | Vitrified sets: a set written and read back field by field, the same bytes from the same inputs and a pinned digest, deflated and raw chunks drawing alike, only drawn chunks held, every glyph of the four PSF fixtures drawn through its set equal to the `Psf` draw and the C reference's page digests held through it, the strike a height picks, and each refusal: a chunk's hash, a deflated chunk's length, an unknown major version, each bound and each inconsistent table. |
| `store.rs` | The store's check as the invoking user: a set loaded and drawn, a whole-file hash mismatch, a chunk changed after the check, a wrong owner, a group- or other-writable store, parent, manifest or set, a symbolic link in place of each, a set that is a directory or a FIFO, a set past `SET_BYTES_MAX`, and the manifest read back and refused line by line within its bounds. |
| `font.rs` | `Font`'s drawing of a cluster into its cells, the decoration lines, the two glyph formats and their bounds, and the built-in unifont. |
| `reference.rs` | Every sprite, PSF fixture glyph and unifont glyph against digests of what the C reference drew: the pixel-parity and same-bytes-on-every-host check. The page digests leave out the characters vtium draws as sprites beyond the C reference. |
| `reference/` | The C reference's digests and the program that takes them: see its README. |
| `fixtures/` | Fonts the tests load, with their licences: see its README. |
