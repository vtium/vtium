# AGENTS.md for tools/

This file covers work under `tools/`: the tiers that judge vtium and their goldens, the VT review, and fuzz targets. The root [AGENTS.md](../AGENTS.md) still governs, and its [verification levels](../AGENTS.md#the-verification-levels) say which tiers each level runs. [docs/AGENTS.md](../docs/AGENTS.md#writing) sets how every page and message reads. A replay file tags the rows it covers by the rules in [crates/AGENTS.md](../crates/AGENTS.md#the-target-set-and-its-coverage).

## How vtium is judged

vtium is judged in tiers, from unit tests up to the maintainer's own review on a VT, a Linux virtual console.

| Tier | What it compares | Where it runs | Command |
|---|---|---|---|
| Unit | each crate's behaviour | here | `make test-all-features` |
| 1, conformance | the screen state and replies vtium produces after a stream of input bytes, written as text and compared against vtium's goldens | the replay corpus in `tools/conformance/corpus/`, through the language-neutral interface that its `INTERFACE.md` describes | `make conformance` (`tools/conformance/`, #14), which also runs the interface check |
| Capability probes | each capability vtium claims, tested through a replay, against `tools/conformance/capabilities.list` | `tools/conformance/scenarios/` | `make conformance-probes` |
| 2, pixels | what the renderer draws in memory, compared pixel for pixel | `tools/tier2/`: its goldens in `tools/tier2/goldens/` | `make tier2` |
| 3, the VM | screenshots of vtium's install tree running on a VT in a QEMU/KVM guest with a real KMS device, compared against its goldens in `tools/tier3/goldens/`; it also records size and memory figures | `tools/tier3/` | `make tier3-minimal` (the minspec variants, 1 vCPU and 512 MB), `make tier3` (the five default variants, or the variants `VARIANTS` names) |
| VT review | the maintainer looks at vtium on a real VT, guided step by step | this machine | `make vt-check GUIDE=start` ([docs/vt-review.md](../docs/vt-review.md)); the vtium binary arrives with #21 |

The goldens decide whether vtium's output is right. Every golden is vtium's own (ADR-1503 D1). Some of them began as copies of another harness's results, and [tools/tier3/goldens/README.md](tier3/goldens/README.md) tells that history for tier 3. A golden stays provisional until it is accepted, and a provisional golden judges nothing. Goldens are accepted in two tiers, which [ADR-1503 D10](../docs/architecture/project/ADR-1503-vtium-judges-itself-by-its-own-goldens-and-its-committed-tree-names-no-c-fork.md) decides. The maintainer reviews each pixel golden by eye: those of tier 2, tier 3 and the specimens. A behaviour golden of tier 1 is a readable case with a `# spec:` line and an expected result written independently of vtium. Each failing comparison of such a case, a regression or a new case's mismatch, goes to a fresh reviewer that did not write the change. The reviewer settles the cases that a specification statement or a recorded decision settles with one reading. The maintainer answers the rest as short questions. A new case that matches is accepted by the steering session once a fresh reviewer has checked its citation. An agent may triage a moved pixel golden, but only the maintainer accepts it. The tiers list their provisional goldens in files named `provisional.tsv`.

The author of a change never rewrites, widens or regenerates the golden that judges that change, and never accepts its own golden. A golden changes only in a commit of its own, which lists each value that moved. The maintainer reviews that commit for a pixel golden, a `.bin` case or a question the maintainer answered. For a behaviour golden that a fresh reviewer settled or checked, the steering session makes that commit and names the reviewer's check in it. When your change makes a tier 1 comparison fail, fix vtium or report the case for that reviewer, and leave the golden as it is.

Tier 3 needs `qemu-system-x86_64` with KVM, `qemu-img`, `xorriso`, `gpg`, `curl` and `tic`, and it runs as the user who starts it. Its guest images live in a cache that every vtium checkout shares, `$XDG_CACHE_HOME/vtium/tier3/images`. vtium's own scripts download the pristine (unmodified) image into the cache's `pristine/` directory and build the base image into its `base/` directory ([tools/tier3/README.md](tier3/README.md)).

## The VT review

In the VT review, the maintainer checks a build by eye on a real VT. The review runs one build at a time and starts it from one place, so that no other vtium build is left running on a VT. [docs/vt-review.md](../docs/vt-review.md#this-machines-vts) describes this machine's VTs, the development VT that each run borrows, and how the development VT is refreshed after a merge. A guide asks the maintainer to judge what a person sees through a program, such as the test editor `tools/vt/checks/vim-vt510.py`. Some terminal behaviour looks broken by design, such as letters lost at the right edge in insert mode. Tier 1 proves that behaviour, and a guide shows it only as an Explore step.

A delegated agent never starts a run. The steering session runs each review, and [the steering page](../docs/contributing/steering.md#the-vt-review) says how.

## Fuzz targets

A fuzz target is a cargo-fuzz program in a crate's `fuzz/` directory that feeds generated inputs to the crate's code, and the crate's fuzz README says what each target reaches. The author of a change to code that a fuzz target reaches runs that target at V1 for the default 60 s with `make fuzz FUZZ_TARGETS=<crate>:<target>`, and commits with the change the rows the run appends to `crates/<crate>/fuzz/runs.tsv`. The reviewer checks that those rows exist, that they read ok, or that the crash a row records is fixed or filed, and that their commit column names the change's commit, or its parent with -dirty when the author ran the target before committing. Fuzzing is not a gate, because it needs a nightly toolchain and minutes of wall time.
