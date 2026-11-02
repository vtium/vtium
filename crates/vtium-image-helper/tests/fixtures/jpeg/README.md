# JPEGs that panicked `zune-jpeg` 0.4.21

`tests/jpeg.rs` decodes each of these and requires a picture or a refusal. Each is a crash input the `jpeg` fuzz target (`fuzz/fuzz_targets/jpeg.rs`) found on 2026-11-02 against `zune-jpeg` 0.4.21, starting from the seeds in `fuzz/seeds/jpeg/`, and minimized with `cargo +nightly fuzz tmin`. vtium wrote them; nothing here comes from another project.

| File | Bytes | The crash it caused | Found from |
|---|---|---|---|
| `idct-overflow.jpg` | 197 | An addition overflowed in the scalar IDCT, `src/idct/scalar.rs` line 128, during progressive decoding; a panic where overflow is checked, as in tests and the default fuzz build | `crash-9b534abaedb0331b5fdd4ef16d54cfcc007e6fd5`, 553 bytes |
| `upsample-assert.jpg` | 221 | `assert_eq!` on a horizontally subsampled component's buffer sizes, `src/worker.rs` line 412; a panic in every build | `crash-948d8f54c4d71b9596f8c5610d0bb6db5c70548a`, 254 bytes, found with `-O` |

`zune-jpeg` 0.5.7 moved the scalar IDCT to wrapping arithmetic, and 0.5.15, which the helper links, no longer makes the assertion.
