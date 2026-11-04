# How VTium is judged

vtium is judged in tiers, each with its harness in vtium's tree; [AGENTS.md](https://github.com/vtium/vtium/blob/main/AGENTS.md#how-vtium-is-judged) lists them with their commands.

- **Unit tests:** each crate's behaviour.
- **Tier 1, conformance:** byte streams replayed through vtium's terminal, the screen and the replies compared as text with goldens ([tools/conformance](https://github.com/vtium/vtium/blob/main/tools/conformance/README.md)).
- **Capability probes:** each capability vtium claims has a replay that fails without it, run by the same tool.
- **Tier 2, pixels:** the renderer draws known screens into memory, compared pixel for pixel with goldens ([tools/tier2](https://github.com/vtium/vtium/blob/main/tools/tier2/README.md)).
- **Tier 3, the VM:** vtium's install tree on a VT in a QEMU/KVM guest with a real display device, its screens captured and compared with goldens, with size and memory figures ([tools/tier3](https://github.com/vtium/vtium/blob/main/tools/tier3/README.md)).
- **The VT review:** the maintainer judges a build by eye on a real VT, guided step by step ([vt-review.md](../vt-review.md)).

A golden is a saved expected result: a screen as text, a picture, a reply. The goldens are the judge: vtium's output matches its golden, or the case is listed with its reason, or the case fails. The author of a change never rewrites the golden that judges it. A golden changes only in a commit of its own, which lists each value that moved and why, and the maintainer reviews that commit before it lands. A new golden is provisional, and judges nothing, until the maintainer has reviewed it. vtium builds tier 3's guest images from its own scripts, and no gate, CI job or build needs anything outside vtium's tree.

[ADR-1000](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md)'s tables list each mode and feature vtium supports, ignores or refuses. A test that drives a row's behaviour carries the row's id in a tag, and a script checks that the tags and the tables agree, so each row shows the tests that judge it, and a row with none shows that nothing does yet.

[How vtium is built](how-vtium-is-built.md) explains why the judge comes before the code.
