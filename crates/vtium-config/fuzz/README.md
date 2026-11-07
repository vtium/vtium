# vtium-config fuzz targets

Four cargo-fuzz targets in their own workspace, so the main workspace and CI never build them or pull `libfuzzer-sys`. A cluster the helper shapes is text a program wrote to the terminal (ADR-1301 D1), so `shape` drives clusters through HarfRust and swash as the helper does. A BDF font is a file anyone can name in a frit, so `bdf` drives any bytes through vtium-config's own BDF reader (ADR-1200 D14). The glyph helper reads its requests from the other end of a pipe, so its request parser is its attack surface, and `serve` drives any bytes through it (ADR-1301 D2). A `font:` source makes the helper read the name table of each installed font file it considers, and a user can install any file there, so `names` drives any bytes through that reader (ADR-1500 D8). Run each on nightly from this directory: `cargo +nightly fuzz run shape`, `cargo +nightly fuzz run bdf`, `cargo +nightly fuzz run serve`, `cargo +nightly fuzz run names`.

From the repository's root, `make fuzz FUZZ_TARGETS=vtium-config:shape` runs a target from a fresh corpus outside the repository and appends a row to `runs.tsv`, the record of how long each run lasted, against which commit, and what it found.

| Path | What it is |
|---|---|
| `fuzz_targets/shape.rs` | Any input as clusters for the test font of `tools/testfont/src/ttf.rs`, or as the font itself asked for a fixed set of clusters, rasterised with each cluster shaped: no panic. Its first byte picks which, and the style the rasteriser synthesises over the glyphs, so a crafted outline is also emboldened and slanted (ADR-1302 D4). |
| `fuzz_targets/bdf.rs` | Any input as a BDF font, plain or compressed: no panic, and each glyph the reader keeps is exactly its cells' rows. A seed corpus of a few small BDF files, such as the head of a real one cut before its `ENDFONT`, finds the reader's paths faster than an empty one. |
| `fuzz_targets/serve.rs` | Any input as the requests the console writes to the glyph helper, read one after another by `ServeRequest::read` until it ends or refuses one: no panic, and each request it takes, its style and the rows line spacing adds above and below its cell among its fields, encodes back to bytes it reads as the same request. |
| `fuzz_targets/names.rs` | Any input as a font file through `names::faces`, the reader of a font's table directory and `name` table: no panic, at most `FACES_MAX` faces, each read once at a rising index, and no family an empty string. It reaches the reads of a collection's header, each face's table directory and its `name` table, and read-fonts' parse of that table. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
