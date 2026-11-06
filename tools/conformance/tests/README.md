# vtium-conformance tests

`cargo test -p vtium-conformance` runs them. The unit tests of the test commands, the cuts, the palettes, the provisional list, the grammar, the probe files, the chunking check and the COVERAGE map sit in their modules under `src/`.

| Path | What it is |
|---|---|
| `runner.rs` | The runner's verdicts on a corpus of one case written in the test: a match, a difference, a replay that exits, and the masking; and `vtium-replay`'s exit codes. |
| `generate.rs` | The generator against the copies: every synthetic case byte for byte, its cuts and its fixtures, and its inputs refused when one is missing. |
| `interface.rs` | The interface check on the tree, and each kind of fault it catches on inputs written in the test. |
| `stand_in.rs` | Built with images: the image store against the `img-` cases' goldens, each solid sixel string replaced by the `image` and `place` commands that make the same picture, each compared with its goldens exactly, and the one case whose sixels are not solid left to the sixel handler. |
| `termcap.rs` | The xtgettcap probe's fixture against the XTGETTCAP table vtium answers from. |
| `probes.rs` | The probe files read and checked against the copied interface; and the probe runner on probes written in the test: a pass and each kind of failure, the order of `test` lines, an absent feature, the malformed probes refused before anything runs, a vtium probe taking a placeholder's place and refused another probe's name, a fixture served from `fixtures/`, and each verdict against the list. |
