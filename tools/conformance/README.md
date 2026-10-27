# conformance

Tier 1 for vtium. `vtium-replay` meets the C reference's replay interface, libtsm's `test/conformance/INTERFACE.md` (interface 1.0), over `vtium-parse` and `vtium-term`. `conformance` replays every case of the C reference's corpus through it and judges each against its golden, at the libtsm commit `pin.tsv` names; it also runs the C reference's capability probes through `vtium-replay`, judged by vtium's capability list. The goldens are the judge: every difference from a golden is a listed departure, a listed pending case, or a failure.

The crate is a workspace member, so `cargo test --workspace` builds both programs and runs the tests below, none of which needs a checkout of the C repositories. Reading the goldens and the probes does, so they stay out of the default test run, in two Makefile targets that CI's `conformance` job runs on every pull request:

```
make conformance          # every case, both views, and the chunking check (LIBTSM=../libtsm)
make conformance-probes   # the capability probes (LIBTSM=../libtsm KMSCON=../kmscon)
make conformance-issues   # every issue the lists name is open (gh)
```

`conformance` takes `--libtsm DIR` and `--kmscon DIR` (default `../libtsm` and `../kmscon` beside this repository) and `--out DIR` (default `out/` here, which git ignores). It reads the pinned commits through `git archive` only, into `out/<repo>-<commit>/`, afresh on every run, so the checkouts' branches and working trees never reach the judge; a checkout that lacks the commit is refused with the command that fetches it. It exits 0 when every verdict holds, 1 on a failure, and 2 when it refuses to run.

| Path | What it is |
|---|---|
| `src/bin/vtium-replay.rs` | The replay interface's command line: `vtium-replay [--frame] COLS ROWS FILE [CUTS]` and `vtium-replay --describe`. |
| `src/lib.rs` | The command line's parsing, the exit codes and `--describe`. |
| `src/replay.rs` | The replay: the reference host's settings, the feed between cuts, the test commands and the head of each block. |
| `src/command.rs` | The test commands of interface 1, each value checked against its form and range. |
| `src/view.rs` | The screen and frame views, from vtium-term's lines and frame. |
| `src/palettes.rs` | The nine palettes of INTERFACE.md's `palettes` block. |
| `src/runner.rs` | The corpus, the masking, and the verdict on each case. |
| `src/runner/lists.rs` | Reading `pin.tsv` and `expected.tsv`, and their checks. |
| `src/runner/source.rs` | Extracting the pinned commits' files. |
| `src/runner/chunking.rs` | The chunking check: each case with extra cuts at random offsets. |
| `src/coverage.rs` | `COVERAGE.md`, generated from `capabilities.list` and ADR-1000's tables, and its test. |
| `src/bin/conformance.rs` | The runner's commands and its report. |
| `pin.tsv` | The libtsm and kmscon commits the runner judges against. |
| `expected.tsv` | The cases whose output differs from the golden, each with the departure that lists it or the issue that owes it. |
| `expected/screen/`, `expected/frame/` | vtium's expected output for each listed departure, by view. |
| `expected/pending/screen/`, `expected/pending/frame/` | Each pending case's snapshot: vtium's output, or what it printed before the replay stopped and its exit code and message. |
| `capabilities.list` | vtium's capability list: each probe `works` or `missing`, with the issue for each gap and the ADR-1000 rows it judges. |
| `COVERAGE.md` | Each capability, the tests that guard its rows, and its gap. |
| `tests/` | The verdicts and the command line on a corpus written in the test: see its README. |

## The replay

`vtium-replay` offers the screen and frame views, and the commands and features interface 1 makes mandatory: `attrs-extended`, `view-frame`, `sync`, `identify`, `input`, `focus`, `palette` and `query`. vtium is built without images, so it lacks `images`, `sixel`, `kitty-graphics` and `image-host`, and the commands that need them exit 2 as the interface asks; the draw2 view is optional (the operator's decision of 2026-10-26) and absent; and it declines `damage-exact`, so the runner masks the changed counts on both sides.

The reference host is a scrollback of 0 lines (`sb-max` raises it, up to `SB_SIZE_MAX`), the cell size 8 by 16, the cursor blinking, mode 2026 presented, BackSpace sending 0x08, XTVERSION's text `tsm-replay`, an empty XTGETTCAP table, and the `legacy` palette, which is vtium-term's default. Each `termcap` fixture is checked against the interface's bounds before vtium-term's `Termcap` takes it.

The frame view prints the colours vtium-term's frame resolves, so a difference in that resolution shows as a difference from the golden. Each block's `changed` count is the cells the frame marks changed since the view's previous block, every cell for the first; each `frame-since` caller keeps its own age.

## The judge

A case in a view is judged against its golden, after INTERFACE.md's masking: a nonzero `changed` or `frame-since` count becomes `N`, and a placement's `changed` flag goes.

| Listed in `expected.tsv` | The case passes when |
|---|---|
| no | vtium's output equals the golden |
| as a departure | vtium's output equals `expected/<view>/<case>.txt`, and the golden differs from it |
| as pending | vtium's output differs from the golden and equals `expected/pending/<view>/<case>.txt`, or the replay exits as that snapshot says, after printing what it says |

So an unlisted difference fails, a listed case that comes to match its golden fails until its rows go, a departure prints exactly what it lists, and a pending case fails on any change from its snapshot: a regression in the part of it vtium runs shows while the rest waits. A replay that stops at a command vtium lacks still prints the blocks before it, as the C reference did, and the snapshot holds them.

Each case also replays in each view with 1 to 8 extra cuts at random offsets outside test commands, two rounds (`--rounds`), and must end in the same last block, with the same replies over the whole replay and the same `test` lines, as the case with its own cuts (docs/architecture.md, "Determinism"). The last block's `changed` count is left out, since an extra block moves it. The seed is `GITHUB_RUN_ID` when it is set, so each CI run tries other cuts, else 1, and `--seed` sets it; the report prints it, and a failure names the seed and the cuts.

Before any case runs, the runner checks `--describe` against INTERFACE.md's `describe` block (the same major version, the block's order, and every mandatory feature), each row of `expected.tsv` against `cases.list`, each departure's entry against the file it names, and each file under `expected/` against a departure row.

## The lists

`expected.tsv` holds one row per reason a case differs: `VIEW CASE KIND SOURCE ENTRY`, VIEW being `screen`, `frame` or `*` for both. A departure's SOURCE is the file that lists it: a README, where ENTRY appears word for word under its `## Departures` or `## Golden differences` heading; or `INTERFACE.md` for the pinned interface, where ENTRY is a sentence that lets an implementation list the case as a departure. A pending row's SOURCE is the vtium issue that owes the case, and ENTRY says what vtium lacks; a case with any pending row is pending in that view. `conformance expect VIEW CASE...` writes each listed case's expected output or snapshot, printing what moved in each file it rewrites, and refuses an unlisted case. A departure's expected output is read against the golden before it is committed. A snapshot rewrite lands with the change that moved it, its commit says what moved and why, and review reads `git diff tools/conformance/expected/pending`, as CONTRIBUTING.md asks of a change to a golden.

`--without CASE` takes a case's rows out of the expectations for one run, so a listed difference can be seen failing. `conformance issues` asks `gh` for the state of every issue a pending row or a missing capability names, and fails on a closed one: its work should have taken the case up. It needs the network, so it runs in CI and through `make conformance-issues`, never in `cargo test`.

`capabilities.list` is run.py's format, `NAME STATUS ISSUE NOTE`, with the note starting with the ADR-1000 rows the probe judges. The image probes are `works -Dimages`: they pass in a build with images and must fail in one without, and `conformance probes` passes `--build images=false` until vtium-image lands (#48, #49). run.py fails when a `works` probe fails, a `missing` probe passes, or the list and `probes.replay` name different probes; `coverage_map_matches_the_capability_list_and_adr_1000` fails when `COVERAGE.md` differs from the list and ADR-1000's `tests` columns, and `VTIUM_CONFORMANCE_BLESS=1 cargo test -p vtium-conformance coverage` rewrites it. The terminfo probes join when vtium has a terminfo entry (#21), and the font probes with outline fonts (#23); run.py skips `xtgettcap` without the termcap fixture kmscon's build compiles with `tic`.
