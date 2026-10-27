# vtium-conformance tests

`cargo test -p vtium-conformance` runs them, with no checkout of the C repositories. The unit tests of the test commands, the cuts, the palettes, the lists, the chunking check and the COVERAGE map sit in their modules under `src/`.

| Path | What it is |
|---|---|
| `runner.rs` | The runner's verdicts on a corpus of one case written in the test: a match, an unlisted difference, a listed departure and a pending case, each listed case that comes to match its golden, and the masking; and `vtium-replay`'s exit codes. |
