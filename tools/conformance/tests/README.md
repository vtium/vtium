# vtium-conformance tests

`cargo test -p vtium-conformance` runs them. The unit tests of the test commands, the cuts, the palettes, the lists, the provenance check, SHA-256, the grammar, the probe files, the chunking check and the COVERAGE map sit in their modules under `src/`.

| Path | What it is |
|---|---|
| `runner.rs` | The runner's verdicts on a corpus of one case written in the test: a match, an unlisted difference, a listed departure and a pending case, each listed case that comes to match its golden, and the masking; and `vtium-replay`'s exit codes. |
| `probes.rs` | The copies against `provenance.tsv`; the probe files read and checked against the copied interface; and the probe runner on probes written in the test: a pass and each kind of failure, the order of `test` lines, an absent feature, the malformed probes refused before anything runs, a vtium probe taking a placeholder's place, and each verdict against the list. |
