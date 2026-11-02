# A PNG that panicked `png` 0.18.1

The PNG tests in `src/decode.rs` decode this file and require a refusal. It is a crash input the `png` fuzz target (`fuzz/fuzz_targets/png.rs`) found on 2026-11-02 against `png` 0.18.1, which the helper links, minimized with `cargo +nightly fuzz tmin`. `png` skips its checksums when it is built for fuzzing, so vtium rewrote each chunk's CRC to match its bytes, which makes the file reach the same panic in a normal build. vtium wrote it; nothing here comes from another project.

| File | Bytes | The crash it caused | Found from |
|---|---|---|---|
| `palette-remainder.png` | 599 | A palette picture whose `PLTE` chunk holds 518 bytes, which is not a whole number of three-byte entries: `png` copies the palette three bytes at a time and slices past its end, `src/decoder/transform/palette.rs` line 74, a panic in every build | `crash-a9c6744b9a2a0cd684cb88a2764549b9186c201a`, 605 bytes, found with `-O` |

The helper refuses such a palette as `CORRUPT` before `png` expands it.
