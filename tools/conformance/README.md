# conformance

Tier 1 for vtium. `vtium-replay` meets the replay interface, interface 1.1: `corpus/INTERFACE.md`, interface 1.0, with vtium's additions in `INTERFACE-vtium.md`, over `vtium-parse` and `vtium-term`. `conformance` replays every case of the corpus through it and judges each against its golden, and runs the capability probes through it, each judged by vtium's capability list. The goldens are the judge: every difference from a golden is a listed departure, a listed pending case, or a failure.

The corpus, its goldens, the interface and the probe file were made by the C reference and copied here at the commits `provenance.tsv` names (The copies). Nothing here reads a checkout of the C repositories or runs their code.

The crate is a workspace member, so `cargo test --workspace` builds both programs and runs the tests below. The whole corpus and the probes run in two Makefile targets, which CI's `conformance` job runs on every pull request:

```
make conformance          # the copies, every case, both views and the chunking check
make conformance-probes   # the copies and the capability probes
make conformance-issues   # every issue the lists name is open (gh)
```

`conformance` exits 0 when every verdict holds, 1 on a failure, and 2 when it refuses to run.

| Path | What it is |
|---|---|
| `src/bin/vtium-replay.rs` | The replay interface's command line: `vtium-replay [--frame] COLS ROWS FILE [CUTS]` and `vtium-replay --describe`. |
| `src/lib.rs` | The command line's parsing, the exit codes and `--describe`. |
| `src/replay.rs` | The replay: the reference host's settings, the feed between cuts, the test commands and the head of each block. |
| `src/command.rs` | The test commands of interface 1, each value checked against its form and range. |
| `src/view.rs` | The screen and frame views, from vtium-term's lines and frame. |
| `src/spec.rs` | The interface's `describe` and `grammar` blocks, over 1.0 and vtium's additions, and the check of a line against the grammar. |
| `src/runner.rs` | The corpus, the masking and the verdict on each case. |
| `src/runner/lists.rs` | Reading `expected.tsv`, and its checks. |
| `src/runner/provenance.rs` | Reading `provenance.tsv`, and the check of each copy against it. |
| `src/runner/chunking.rs` | The chunking check: each case with extra cuts at random offsets. |
| `src/sha256.rs` | SHA-256, for the provenance check. |
| `src/probes.rs` | Reading the probe files, and the checks made before a probe runs. |
| `src/probes/judge.rs` | A probe replayed and judged, and each capability's verdict against the list. |
| `src/coverage.rs` | `COVERAGE.md`, generated from `capabilities.list` and ADR-1000's tables, and its test. |
| `src/bin/conformance.rs` | The runner's commands and its report. |
| `corpus/` | The copied corpus: `INTERFACE.md`, `cases.list`, `cases/` with each case's stream, cuts and fixtures, and the goldens, `golden/` for the screen view and `golden-frame/` for the frame view. |
| `INTERFACE-vtium.md` | vtium's additions to the interface: the `clipboard` command and feature, and the line it prints. |
| `probes/probes.replay` | The copied capability probes. |
| `probes/vtium.replay` | vtium's own probes. |
| `provenance.tsv` | Each copied file: its path here, the repository, commit and path it came from, and its SHA-256. |
| `expected.tsv` | The cases whose output differs from the golden, each with the departure that lists it or the issue that owes it. |
| `expected/screen/`, `expected/frame/` | vtium's expected output for each listed departure, by view. |
| `expected/pending/screen/`, `expected/pending/frame/` | Each pending case's snapshot: vtium's output, or what it printed before the replay stopped and its exit code and message. |
| `capabilities.list` | vtium's capability list: each probe `works` or `missing`, with the issue for each gap and the ADR-1000 rows it judges. |
| `COVERAGE.md` | Each capability, the tests that guard its rows, and its gap. |
| `tests/` | The verdicts, the command line and the probe runner on inputs written in the tests, and the copies against `provenance.tsv`: see its README. |

## The copies

`provenance.tsv` holds one row per copied file, `PATH REPO COMMIT SOURCE SHA256`: PATH from the root of this repository, the repository (`libtsm` or `kmscon`, aaronsb's forks), the commit, the path there, and the copy's SHA-256. The corpus came from libtsm's `test/conformance/` and the probes from kmscon's `tests/capabilities/probes.replay`. The draw2 goldens, which vtium does not owe, stayed behind. One row is an excerpt: vtium-term's `crates/vtium-term/tests/palettes.c.txt` is lines 254 to 451 of libtsm's `src/tsm/tsm-vte.c`, the C reference's palette tables, which vtium-term's palette test reads.

Every run, and `the_copies_match_their_provenance` in `cargo test`, checks each row's file against its SHA-256, and every file under `corpus/` for a row. A golden changes only in a commit of its own that lists each moved value, as CONTRIBUTING.md asks, and that commit changes its row too.

## The replay

`vtium-replay` offers the screen and frame views, and the commands and features interface 1.1 makes mandatory: `attrs-extended`, `view-frame`, `sync`, `identify`, `input`, `focus`, `palette`, `query` and vtium's `clipboard`. vtium is built without images, so it lacks `images`, `sixel`, `kitty-graphics` and `image-host`, and the commands that need them exit 2 as the interface asks; the draw2 view is optional (the operator's decision of 2026-10-26) and absent; and it declines `damage-exact`, so the runner masks the changed counts on both sides.

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

`capabilities.list` is `NAME STATUS ISSUE ROWS NOTE` a line: STATUS `works` or `missing`; ISSUE `-` for a working probe, the vtium issue (`#N`) that owes a missing one, or the build feature a working one needs (`-Dimages`); ROWS the ADR-1000 rows the probe judges. `coverage_map_matches_the_capability_list_and_adr_1000` fails when `COVERAGE.md` differs from the list and ADR-1000's `tests` columns, and `VTIUM_CONFORMANCE_BLESS=1 cargo test -p vtium-conformance coverage` rewrites it.

## The probes

`conformance probes` reads `probes/probes.replay` and vtium's own `probes/vtium.replay`, replays each probe through vtium-replay's code and judges it against the list:

- a probe listed `works` fails the run when it fails, and one listed `missing` when it passes, so the change that adds a feature also sets its line to `works` and closes its issue;
- a probe listed with a build feature passes in a build with it and fails, as absent, in one without; vtium is built without images until vtium-image lands (#48, #49), so the image probes must fail, and a feature the build does not name fails the run;
- a probe the list does not name, or a name in the list no probe defines, fails the run;
- a probe that names a fixture is skipped, printed as `skip`, since vtium has none yet: `xtgettcap` reads the termcap fixture the C reference's build compiled with `tic`, which joins with vtium's terminfo entry (#21).

A probe of `vtium.replay` takes the place of a placeholder of the same name in the copied file, and of nothing else: `osc-52-clipboard` replaces the C reference's placeholder, which failed whatever the terminal did, since the C reference dropped OSC 52 and interface 1.0 had no way to see a clipboard (#110). The C reference's runner, kmscon's `tests/capabilities/run.py`, also ran its terminfo and font probes; vtium's join when it has a terminfo entry (#21) and outline fonts (#23).

A stanza opens with `probe NAME`, builds a byte stream and the cuts of the interface's command line, and judges the blocks the replay prints at the cuts. A line starting with `#` is a comment.

| Line | Meaning |
|---|---|
| `size COLS ROWS` | The screen the replay starts with. Required. |
| `view NAME` | `screen` (when not given), `frame` or `draw2`: the view the replay prints. |
| `needs FEATURE...` | Features the probe needs beyond those its view and commands imply. |
| `fixture NAME` | A file the probe's fixture directory holds. |
| `send "..."` | Appends bytes. `\e`, `\r`, `\n`, `\t`, `\\`, `\"` and `\xHH` stand for one byte each, as in the interface's worked examples, and every other character for its UTF-8. |
| `cmd TEXT` | Appends the test command TEXT, `ESC _ tsm-test;TEXT ESC \`. |
| `check` | Puts a cut after the bytes so far. The assertions that follow judge the block printed there. |
| `want LINE` | The block holds LINE. |
| `want-re REGEX` | A line of the block matches REGEX whole. |
| `reject LINE` | The block does not hold LINE. |
| `reject-re REGEX` | No line of the block matches REGEX whole. |
| `reply "..."` | The block's replies, its `reply` lines decoded and joined, are these bytes. |
| `reply-re "..."` | The replies match these bytes, read as a regular expression over bytes, whole. A metacharacter meant literally takes a backslash, written `\\`. |
| `example "..."` | Follows a `want-re` or `reply-re`: a line or reply, written as for `send`, that the expression must match. Required in a missing probe. |
| `placeholder REASON` | The probe has nothing to run, and fails with REASON. |

A `test` line belongs to the block whose `checkpoint` follows it, and the `want` and `want-re` assertions on `test` lines take them in order: each matches a line after the one the assertion before it matched. A regular expression uses no backreference or lookaround.

Every line the replay prints must parse under the interface's grammar for the view, or the probe fails. Unless the probe needs `damage-exact` and the replay declares it, which vtium-replay does not, the output and the `want` and `reject` lines are masked as the interface's "Comparing with the goldens" says, and an expectation writes such a count as `N`.

Before anything runs, each stanza is checked: each `want` and `reject` line, and each `want-re` example, parses under the grammar for the probe's view, with the reserved keys allowed only in a missing probe, and each `want-re` and `reply-re` matches its example, so a missing probe's expectations are lines and replies the feature could print and a typo reads as malformed instead of as still missing. A nonzero count or a placement's `changed` flag in a probe that does not need `damage-exact` is refused with the advice to write `N`. Each assertion follows a `check` with no input between, and each probe has a `want`, `want-re`, `reply` or `reply-re`.
