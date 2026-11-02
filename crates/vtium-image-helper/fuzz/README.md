# vtium-image-helper fuzz targets

cargo-fuzz targets in their own workspace, so the main workspace and CI never build them or pull `libfuzzer-sys`. ADR-1101 D6 gives each decoding path a fuzz target that covers every crate on that path, run in-process and outside the helper's memory cap. Run them on nightly from this directory, with a corpus directory of your own first and the seeds after it, for example `cargo +nightly fuzz run png corpus/png seeds/png`. A fuzzer rarely writes a PNG whose checksums hold, or a JPEG or zlib stream whose structure holds, so the seeds give it whole files to mutate from. The JPEGs that once crashed the decoder are in `../tests/fixtures/jpeg/`, which a run of the `jpeg` target can add as one more corpus directory. The decoders take the worker's proof that it has locked itself in; cargo-fuzz builds the crate with `cfg(fuzzing)`, under which the targets make one with `Locked::unsandboxed`.

| Path | What it is |
|---|---|
| `fuzz_targets/png.rs` | Any input as a PNG file, alone, as any image, and inside zlib (`png`, `fdeflate`, `miniz_oxide`): no panic, and a picture is exactly its size's premultiplied pixels within the bounds its header was checked against. |
| `fuzz_targets/zlib.rs` | Any input as zlib data for raw pixels, the first byte choosing their count and layout (`miniz_oxide` and the one conversion): no panic, an inflate never past its bound, and the converted pixels exactly the picture's. |
| `fuzz_targets/jpeg.rs` | Any input as a JPEG file (`zune-jpeg`): no panic, and a picture is exactly its size's opaque pixels. |
| `fuzz_targets/gif.rs` | Any input as a GIF file, its first frame on a transparent canvas (`gif`, `weezl`): no panic, and a picture is exactly its size's premultiplied pixels. |
| `fuzz_targets/request.rs` | Any input as a request packet, a reply packet and a hello (the wire both ends link): no panic, and every packet accepted encodes back to itself. |
| `seeds/png/`, `seeds/gif/`, `seeds/jpeg/`, `seeds/zlib/` | Small valid files to start from: PNGs of RGBA with partial alpha, grey, a palette with transparency and 16-bit RGB; a two-pixel GIF with a transparent pixel; JPEGs that are baseline, progressive, grey with an odd size, and subsampled 4:2:0; and zlib inputs, each a first byte choosing the pixels and then zlib data, for one RGB pixel, 38 RGBA pixels, one premultiplied pixel, stored blocks with junk after them, and more bytes than the pixels need. |
| `Cargo.toml`, `Cargo.lock` | The fuzz crate and its pinned dependencies. |
| `.gitignore` | Leaves out `target`, `corpus`, `artifacts` and `coverage`. |
