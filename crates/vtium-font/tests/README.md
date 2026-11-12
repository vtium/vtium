# vtium-font integration tests

Each top-level `.rs` file is its own test crate, run by `cargo test -p vtium-font`. Every test says where its expectation comes from: a specification, the operator's decision, a model written apart from the code, or vtium's glyph digests in `reference/`.

| Path | What it is |
|---|---|
| `common/mod.rs` | Builders for crafted PSF1 and PSF2 fonts and gzip files, a gunzip for the kbd sweep, and a set's sections taken apart and put back together. |
| `psf.rs` | The PSF loader against crafted fonts, plain and gzipped: each malformed header, truncation and table bound refused, the unicode table's sequences and UTF-8 read as "The PSF font format" and RFC 3629 describe them, raw fonts as setfont(8) reads them, the gzip framing, the fixtures, and every font in `/usr/share/kbd/consolefonts` when the machine has it. |
| `sprite.rs` | The sprites against a box model written apart from `src/sprite.rs`: every cell size from 1x1 to 32x64 and 1000x2000, mosaic complements, and Braille layouts worked out by hand. |
| `legacy.rs` | The Symbols for Legacy Computing sprites against a model built from their Unicode names: every codepoint of the ranges drawn or left to the face by name, every cell size from 1x1 to 32x64 and three larger cells, and literal masks. |
| `policy.rs` | Each `Source` of a `SpritePolicy` against stand-in faces that cover a range fully, partly and not at all, the ranges against the sprites, the coverage count and its partial ranges, the default for a named font and for unifont, and the missing glyph. |
| `missing.rs` | `Missing::Hex` against literal masks of the operator's design, narrow and wide, up to U+FFFF and past it, and its scaling, centring and fallback. |
| `set.rs` | Vitrified sets: a set written and read back field by field, the same bytes from the same inputs and a pinned digest, deflated and raw chunks drawing alike, only drawn chunks held, every glyph of the four PSF fixtures drawn through its set equal to the `Psf` draw and to the page digests of `digests/psf.tsv`, the strike a height picks, decorations at a whole factor, and each refusal: a deflated chunk's length, an unknown major version, each bound and each inconsistent table. |
| `script_set.rs` | A set's superscript and subscript strikes (ADR-1202 D11): each strike's script in its record, a script strike's glyph drawn only for a cell in that script, a style it lacks taken from its script's regular strike, a set of format 2.0 or 2.1 read as normal strikes, the field a 2.0 or 2.1 reader refuses when it is not 0, and an unknown script refused. |
| `font.rs` | `Font`'s drawing of a cluster into its cells, the decoration lines, the two glyph formats and their bounds, and the built-in unifont. |
| `digests.rs` | Every sprite, PSF fixture glyph and unifont glyph against vtium's digests of them, so a change to what vtium draws fails here, and, since the digests were taken on x86-64, the check that a glyph is the same bytes on every host. The page digests leave out the Symbols for Legacy Computing characters that `legacy.rs` holds. |
| `digests/` | vtium's glyph digests and how they are changed: see its README. |
| `fixtures/` | Fonts the tests load, with their licences: see its README. |
