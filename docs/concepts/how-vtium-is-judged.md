# How VTium is judged

vtium is judged in tiers, each with its harness in vtium's tree; [AGENTS.md](https://github.com/vtium/vtium/blob/main/AGENTS.md#how-vtium-is-judged) lists them with their commands.

- **Unit tests:** each crate's behaviour.
- **Tier 1, conformance:** byte streams replayed through vtium's terminal, the screen and the replies compared as text with goldens ([tools/conformance](https://github.com/vtium/vtium/blob/main/tools/conformance/README.md)).
- **Capability probes:** each capability vtium claims has a replay that fails without it, run by the same tool.
- **Tier 2, pixels:** the renderer draws known screens into memory, compared pixel for pixel with goldens ([tools/tier2](https://github.com/vtium/vtium/blob/main/tools/tier2/README.md)).
- **Tier 3, the VM:** vtium's install tree on a VT in a QEMU/KVM guest with a real display device, its screens captured and compared with goldens, with size and memory figures ([tools/tier3](https://github.com/vtium/vtium/blob/main/tools/tier3/README.md)).
- **The VT review:** the maintainer judges a build by eye on a real VT, guided step by step ([vt-review.md](../vt-review.md)).

The goldens are the judge, and each tier keeps its own in vtium's tree. A difference from one is a failure, unless the tier's list of expected differences names the case, with vtium's expected output and the reason, or lists the case as pending. A new golden stays provisional, and judges nothing, until the maintainer reviews it, and a golden changes only in a commit of its own that the maintainer reviews. vtium builds tier 3's guest images from its own scripts.

[ADR-1000](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md)'s tables list each mode and feature vtium supports, ignores or refuses. A test that drives a row's behaviour carries the row's id in a tag, and a script checks that the tags and the tables agree, so each row shows the tests that judge it, and a row with none shows that nothing does yet.

[How vtium is built](how-vtium-is-built.md) explains why the judge comes before the code.
