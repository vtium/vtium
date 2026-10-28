# conformance

Tier 1 for vtium. `vtium-replay` meets the replay interface, interface 1.1: `corpus/INTERFACE.md`, interface 1.0, with vtium's additions in `INTERFACE-vtium.md`, over `vtium-parse` and `vtium-term`. `conformance` replays every case of the corpus through it and judges each against its golden, and runs the capability probes through it, each judged by vtium's capability list. The goldens are the judge: every difference from a golden is a listed departure, a listed pending case, or a failure.

The corpus, its goldens, the interface and the probe file were made by the C reference and copied here at the commits `provenance.tsv` names (The copies). The C reference's tools that made and checked them are ported here in Rust: the generator of the synthetic cases, the recorder of the programs' cases, and the interface check. Nothing here reads a checkout of the C repositories or runs their code, but `make conformance-provenance`.

The crate is a workspace member, so `cargo test --workspace` builds both programs and runs the tests below. The whole corpus and the probes run in two Makefile targets, which CI's `conformance` job runs on every pull request:

```
make conformance          # the copies, every case, both views and the chunking check
make conformance-probes   # the copies and the capability probes
make conformance-issues   # every issue the lists name is open (gh)
make conformance-provenance   # each row of provenance.tsv against its source (LIBTSM=DIR KMSCON=DIR); a review step, not a gate
make conformance-record OUT=DIR [NAMES='prog-shell ...']   # records the programs' cases; not a gate
target/release/conformance generate DIR   # writes every synthetic case and prints its cases.list lines
target/release/conformance interface      # the interface check alone
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
| `src/interface.rs`, `src/interface/` | The interface check: the goldens under the grammar, the worked examples, the palettes, the refusals and the grammar's controls. |
| `src/generate.rs`, `src/generate/` | The synthetic cases' recipes, and their writer. |
| `generator/` | The generator's copied inputs: `recorded/`, streams a client wrote, and `sixel/`, libsixel's `img2sixel` output. |
| `src/record.rs`, `src/bin/conformance-record.rs` | The recorder of the programs' cases, built with the `record` feature. |
| `fixtures/kmscon.termcap` | The xtgettcap probe's termcap fixture: vtium's XTGETTCAP table. |
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

`provenance.tsv` holds one row per copied file, `PATH REPO COMMIT SOURCE SHA256`: PATH from the root of this repository, the repository (`libtsm` or `kmscon`, aaronsb's forks), the commit, the path there, and the copy's SHA-256. The corpus came from libtsm's `test/conformance/`, the generator's inputs from its `test/conformance/recorded/` and `test/sixel/`, and the probes from kmscon's `tests/capabilities/probes.replay`. The draw2 goldens, which vtium does not owe, stayed behind. One row is an excerpt: vtium-term's `crates/vtium-term/tests/palettes.c.txt` is lines 254 to 451 of libtsm's `src/tsm/tsm-vte.c`, the C reference's palette tables, which vtium-term's palette test reads.

Every run, and `the_copies_match_their_provenance` in `cargo test`, checks each row's file against its SHA-256, and every file under `corpus/` and `generator/` for a row. Every file under `probes/` has a row too, but for vtium's own `vtium.replay`. A golden changes only in a commit of its own that lists each moved value, as CONTRIBUTING.md asks, and that commit changes its row too.

Nothing in a run ties a row to its source, so any change to `provenance.tsv` is verified at review with `make conformance-provenance` (LIBTSM=DIR KMSCON=DIR, default `../libtsm` and `../kmscon`): it reads each row's source from the checkouts with `git show COMMIT:PATH`, read-only, and fails on a source whose SHA-256 differs from the row's or a commit a checkout lacks. It is not a gate.

## The cases' generator

`src/generate.rs` writes every synthetic case of the corpus from its recipe, each a byte stream of hand-written escape sequences that exercises one area of terminal behaviour, as the C reference's `test/conformance/gen-synthetic.py` did: a case's text is written as UTF-8 with its cuts taken out into `<name>.cuts`, and the fixtures the replay's host serves it go in `<name>.files/`. Each `kit-host-` case is written again as `-nohook`, the host removed first and no fixtures given. `tests/generate.rs` holds the generator to the copies: each of its 160 cases equals the copy under `corpus/cases/` byte for byte, its stream, its cuts and its fixtures, and the only cases of `cases.list` it does not write are the four recordings.

The generator reads six inputs under `generator/`, copied with their rows in `provenance.tsv`: the streams kitten icat and mpv wrote in the C reference's probes, and `img2sixel`'s sixel strings for three pictures. Each is read a byte to a character and written as UTF-8, as the C reference's generator read them (Latin-1), so a byte past 0x7f lands in the case as two.

A new synthetic case is a recipe in `src/generate/` and a line in `cases.list`; `conformance generate DIR` writes every case under DIR, which must be empty or new, and the new case's files are copied into `corpus/cases/`. Its goldens are vtium's own: they land in a commit of their own, reviewed as any golden change is (CONTRIBUTING.md).

## Recording programs

`conformance-record OUT [NAME...]` records the corpus's `prog-*` cases, as the C reference's `test/conformance/record-programs.py` did. It runs bash, less, vim and tmux in a pty with a fixed size and environment, with demo files dated 2026-01-01 UTC as the working directory, in a directory of its own made fresh under the system's temporary directory and removed after. It types each case's keys, keeps every byte the program writes, and cuts where each screen settled: after 0.4 s of quiet, or 5 s at most. It writes `OUT/<name>.bin` and `OUT/<name>.cuts`, and prints the case's line for `cases.list`. It answers no query the program sends. It spawns through vtium-platform's pty, so it is built only with the crate's `record` feature, and it is not a gate. A recording carries this machine's program versions: `prog-less` and `prog-vim` record byte for byte as the copied cases; `prog-shell` differs only in `ls -l`'s owner and directory size; `prog-tmux` differs with the tmux version. A new or changed recording lands in a commit of its own, its goldens reviewed as any golden change is.

## The interface check

`conformance interface`, and every `conformance run` before its cases, checks vtium-replay and the copied goldens against the interface's own blocks, as the C reference's `test/conformance/interface-check.py` did: every line of each golden in `corpus/golden/` and `corpus/golden-frame/` is a form of its kind in the grammar, the kinds follow the view's `structure`, and each block keeps the numbering rules of INTERFACE.md's "Blocks", at its size in `cases.list`; every worked example replays to its output block, but one that needs a feature or a view vtium-replay does not declare, which is counted; the frame view resolves each row of the `palettes` block; each input of the refusals table exits as the interface gives; and the grammar rejects twelve outputs known to be bad and accepts one known to be good, a reserved key only with the reserved keys allowed. Each fault prints as `FAIL replay-interface: ...` and fails the run.

## The replay

`vtium-replay` offers the screen and frame views, and the commands and features interface 1.1 makes mandatory: `attrs-extended`, `view-frame`, `sync`, `identify`, `input`, `focus`, `palette`, `query` and vtium's `clipboard`. vtium is built without images, so it lacks `images`, `sixel`, `kitty-graphics` and `image-host`, and the commands that need them exit 2 as the interface asks; the draw2 view is optional (the operator's decision of 2026-10-26) and absent; and it declines `damage-exact`, so the runner masks the changed counts on both sides.

The reference host is a scrollback of 0 lines (`sb-max` raises it, up to `SB_SIZE_MAX`), the cell size 8 by 16, the cursor blinking, mode 2026 presented, BackSpace sending 0x08, XTVERSION's text `tsm-replay`, an empty XTGETTCAP table, and the `legacy` palette, which is vtium-term's default. Each `termcap` fixture is checked against the interface's bounds before vtium-term's `Termcap` takes it.

The frame view prints the colors vtium-term's frame resolves, so a difference in that resolution shows as a difference from the golden. Each block's `changed` count is the cells the frame marks changed since the view's previous block, every cell for the first; each `frame-since` caller keeps its own age.

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

`expected.tsv` holds one row per reason a case differs: `VIEW CASE KIND SOURCE ENTRY`, VIEW being `screen`, `frame` or `*` for both. A departure's SOURCE is the file that lists it: a `README.md` inside this repository, by a relative path, where ENTRY is the headline of an item under its `## Departures` or `## Golden differences` heading, the whole bold opening `- **ENTRY**` or, in an item with no bold opening, its whole opening sentences; or `INTERFACE.md` for the pinned interface, where ENTRY is a sentence that lets an implementation list the case as a departure. A pending row's SOURCE is the vtium issue that owes the case, and ENTRY says what vtium lacks; a case with any pending row is pending in that view. `conformance expect VIEW CASE...` writes each listed case's expected output or snapshot, printing what moved in each file it rewrites, and refuses an unlisted case. A departure's expected output is read against the golden before it is committed. A snapshot rewrite lands with the change that moved it, its commit says what moved and why, and review reads `git diff tools/conformance/expected/pending`, as CONTRIBUTING.md asks of a change to a golden.

`--without CASE` takes a case's rows out of the expectations for one run, so a listed difference can be seen failing. `conformance issues` asks `gh` for the state of every issue a pending row or a missing capability names, and fails on a closed one: its work should have taken the case up. It needs the network, so it runs in CI and through `make conformance-issues`, never in `cargo test`.

`capabilities.list` is `NAME STATUS ISSUE ROWS NOTE` a line: STATUS `works` or `missing`; ISSUE `-` for a working probe, the vtium issue (`#N`) that owes a missing one, or the build feature a working one needs (`-Dimages`); ROWS the ADR-1000 rows the probe judges. `coverage_map_matches_the_capability_list_and_adr_1000` fails when `COVERAGE.md` differs from the list and ADR-1000's `tests` columns, and `VTIUM_CONFORMANCE_BLESS=1 cargo test -p vtium-conformance coverage` rewrites it.

## The probes

`conformance probes` reads `probes/probes.replay` and vtium's own `probes/vtium.replay`, replays each probe through vtium-replay's code and judges it against the list:

- a probe listed `works` fails the run when it fails, and one listed `missing` when it passes, so the change that adds a feature also sets its line to `works` and closes its issue;
- a probe listed with a build feature passes in a build with it and fails, as absent, in one without; vtium is built without images until vtium-image lands (#48, #49), so the image probes must fail, and a feature the build does not name fails the run;
- a probe the list does not name, or a name in the list no probe defines, fails the run;
- a probe that names a fixture reads it from `fixtures/`, and one whose fixture is missing is not judged, which fails the run: `xtgettcap` names `kmscon.termcap`, vtium's XTGETTCAP table (`tests/termcap.rs`), as the C reference's build wrote its own from its terminfo entry with `tic`.

A probe of `vtium.replay` takes the place of a placeholder of the same name in the copied file: `osc-52-clipboard` replaces the C reference's placeholder, which failed whatever the terminal did, since the C reference dropped OSC 52 and interface 1.0 had no way to see a clipboard (#110). It takes the place of a copied probe that is not a placeholder only when it names a listed departure, and nothing else lets vtium override a copied probe: its `departure SOURCE ENTRY` line names a `README.md` inside this repository by a relative path with no `..`, and ENTRY is the headline of an item under that README's `## Departures` or `## Golden differences` heading: the whole bold opening of a line `- **ENTRY**`, or, in an item with no bold opening, its whole opening sentences, `- ENTRY` up to a full stop. `expected.tsv`'s departure rows follow the same rule. The run refuses any other source, a departure the README does not list whole, one on a probe that replaces a placeholder or nothing, and a vtium probe that replaces a copied one without a departure. `xtgettcap` is the one: vtium-term prints each name in uppercase hex where the copied probe expected the request's spelling. The C reference's runner, kmscon's `tests/capabilities/run.py`, also ran its terminfo and font probes; vtium's join when it has a terminfo entry (#21) and outline fonts (#23).

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
| `departure SOURCE ENTRY` | In `vtium.replay` only: the listed departure that lets the probe take the place of the copied probe of its name. |

A `test` line belongs to the block whose `checkpoint` follows it, and the `want` and `want-re` assertions on `test` lines take them in order: each matches a line after the one the assertion before it matched. A regular expression uses no backreference or lookaround.

Every line the replay prints must parse under the interface's grammar for the view, or the probe fails. Unless the probe needs `damage-exact` and the replay declares it, which vtium-replay does not, the output and the `want` and `reject` lines are masked as the interface's "Comparing with the goldens" says, and an expectation writes such a count as `N`.

Before anything runs, each stanza is checked: each `want` and `reject` line, and each `want-re` example, parses under the grammar for the probe's view, with the reserved keys allowed only in a missing probe, and each `want-re` and `reply-re` matches its example, so a missing probe's expectations are lines and replies the feature could print and a typo reads as malformed instead of as still missing. A nonzero count or a placement's `changed` flag in a probe that does not need `damage-exact` is refused with the advice to write `N`. Each assertion follows a `check` with no input between, and each probe has a `want`, `want-re`, `reply` or `reply-re`.
