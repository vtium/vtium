# conformance

Tier 1 for vtium. `vtium-replay` meets the replay interface, interface 1.8: `corpus/INTERFACE.md`, interface 1.0, with vtium's additions in `INTERFACE-vtium.md`, over `vtium-parse` and `vtium-term`. `conformance` replays every case of the corpus through it and judges each against its golden, plays every behaviour scenario through it and judges each against its transcript, and plays every capability scenario through it, each judged by vtium's capability list. The goldens are the judge: every difference from a golden fails, unless the provisional list names the case.

The corpus, its goldens, the interface and the scenarios are kept here (The copies). The tools that make and check them are written here in Rust: the generator of the synthetic cases, the recorder of the programs' cases, and the interface check. Nothing here reads another repository or runs code from outside the tree.

The crate is a workspace member, so `cargo test --workspace` builds both programs and runs the tests below. The whole corpus and the capability scenarios run in two Makefile targets, which CI's `conformance` job runs on every pull request:

```
make conformance          # every case, both views and the chunking check
make conformance-probes   # the capability list, through the capability scenarios
make conformance-issues   # the capability list and ADR-1000 agree with GitHub (gh)
make conformance-record OUT=DIR [NAMES='prog-shell ...']   # records the programs' cases; not a gate
target/release/conformance generate DIR   # writes every synthetic case and prints its cases.list lines
target/release/conformance interface      # the interface check alone
target/release/conformance scenarios [NAME...]               # the behaviour scenarios alone
target/release/conformance scenarios --write NAME...         # writes a new scenario's transcript
target/release/conformance scenarios --play NAME...          # plays a scenario in this terminal for review
target/release/conformance scenarios --bin NAME DIR          # writes a scenario's replay input for vtium-replay
```

`conformance` exits 0 when every verdict holds, 1 on a failure, and 2 when it refuses to run.

| Path | What it is |
|---|---|
| `src/bin/vtium-replay.rs` | The replay interface's command line: `vtium-replay [--frame] COLS ROWS FILE [CUTS]` and `vtium-replay --describe`. |
| `src/lib.rs` | The command line's parsing, the exit codes and `--describe`. |
| `src/replay.rs` | The replay: the reference host's settings, the feed between cuts, the test commands and the head of each block. |
| `src/replay/links.rs` | The `link` and `hover` commands: the URI of a cell's OSC 8 link, and the cells and overlay row of the link under the pointer. |
| `src/command.rs` | The test commands of interface 1, each value checked against its form and range. |
| `src/view.rs` | The screen and frame views, from vtium-term's lines and frame, and with images the `placement` and `image` lines. |
| `src/replay/images.rs` | With images: the reference host's image settings, the image commands, and the fixture host answering each request. |
| `src/host.rs` | With images: the fixture host, its `host:` line and its answer from the case's fixture files. |
| `src/spec.rs` | The interface's `describe` and `grammar` blocks, over 1.0 and vtium's additions, and the check of a line against the grammar. |
| `src/runner.rs` | The corpus, the masking and the verdict on each case. |
| `src/runner/chunking.rs` | The chunking check: each case with extra cuts at random offsets. |
| `src/scenario.rs`, `src/scenario/` | The scenarios: the reader and its commands (`commands.rs`), the key names (`keys.rs`), the string syntax that the scenarios and the worked examples share (`strings.rs`), the transcript printer and its check (`transcript.rs`), the play through vtium-replay's code (`run.rs`), the capability rules (`capability.rs`) and the player for review (`play.rs`). |
| `src/bin/conformance/scenarios.rs` | `conformance scenarios`, and the scenarios' part of `conformance run` and `conformance interface`. |
| `scenarios/` | The scenarios, each `NAME.scenario` with a behaviour scenario's transcript `NAME.transcript` beside it, and `provisional.tsv`, the behaviour scenarios whose transcripts wait for the maintainer's review. |
| `src/probes.rs` | A scenario's input and assertions as the judge reads them, and the checks made before a capability scenario runs. |
| `src/probes/judge.rs` | A capability scenario replayed and judged, and each capability's verdict against the list. |
| `src/coverage.rs` | `COVERAGE.md`, generated from `capabilities.list` and ADR-1000's tables, and its test. |
| `src/interface.rs`, `src/interface/` | The interface check: the goldens under the grammar, the worked examples, the palettes, the refusals and the grammar's controls. |
| `src/generate.rs`, `src/generate/` | The synthetic cases' recipes, and their writer. |
| `generator/` | The generator's copied inputs: `recorded/`, streams a client wrote, and `sixel/`, libsixel's `img2sixel` output. |
| `src/record.rs`, `src/bin/conformance-record.rs` | The recorder of the programs' cases, built with the `record` feature. |
| `fixtures/kmscon.termcap` | The xtgettcap capability scenario's termcap fixture: vtium's XTGETTCAP table. |
| `src/bin/conformance.rs` | The runner's commands and its report. |
| `corpus/` | The copied corpus: `INTERFACE.md`, `cases.list`, `cases/` with each case's stream, cuts and fixtures, and the goldens, `golden/` for the screen view and `golden-frame/` for the frame view. |
| `vtium/` | vtium's own cases, laid out as `corpus/` is: `cases.list`, `cases/`, `golden/` and `golden-frame/`, `golden-display/` for the display view of the cases of right-to-left text, and `provisional.tsv`, the provisional list: the cases, of the corpus or of vtium's own, whose goldens wait for the maintainer's review. |
| `src/runner/own.rs` | Reading `vtium/provisional.tsv`, the provisional list. |
| `INTERFACE-vtium.md` | vtium's additions to the interface: the `clipboard` command and feature and the line it prints, the `attrs-renditions` feature, which prints the keys 1.0 reserves for the overline, superscript and subscript, the `kitty-animation` feature and its `anim` lines, the display view and the `bidi` feature, the `select`, `copy`, `prompt`, `link`, `hover`, `backspace` and `state` commands with their features and lines, the `bell` line, the `font-ops` and `window-ops` features with their commands and lines, the `key-events` feature, whose `key` command takes a key's event, and the `key-forms` feature that vtium owes and vtium-replay does not offer yet. |
| `capabilities.list` | vtium's capability list: each capability `works` or `missing`, with the issue for each gap and the ADR-1000 rows it judges. |
| `COVERAGE.md` | Each capability, the tests that guard its rows, and its gap. |
| `tests/` | The verdicts, the command line, the scenarios and the capability judge on inputs written in the tests: see its README. |

## The copies

The corpus, the generator's inputs and the probes were first copied from the screen-and-parser library libtsm and the terminal kmscon, the released programs vtium began from. The draw2 goldens, which vtium does not owe, stayed behind. One copy is an excerpt: vtium-term's `crates/vtium-term/tests/palettes.c.txt` holds libtsm's palette tables, which vtium-term's palette test reads.

A golden changes only in a commit of its own that lists each moved value, as CONTRIBUTING.md asks.

## The cases' generator

`src/generate.rs` writes every synthetic case of the corpus from its recipe, each a byte stream of hand-written escape sequences that exercises one area of terminal behaviour, a case's text is written as UTF-8 with its cuts taken out into `<name>.cuts`, and the fixtures the replay's host serves it go in `<name>.files/`. Each `kit-host-` case is written again as `-nohook`, the host removed first and no fixtures given. `tests/generate.rs` holds the generator to the copies: each of its 160 cases equals the copy under `corpus/cases/` byte for byte, its stream, its cuts and its fixtures, and the only cases of `cases.list` it does not write are the four recordings.

The generator reads six inputs under `generator/`, copied in: the streams kitten icat and mpv wrote in the probes, and `img2sixel`'s sixel strings for three pictures. Each is read a byte to a character (Latin-1) and written as UTF-8, so a byte past 0x7f lands in the case as two.

A new synthetic case is a recipe in `src/generate/` and a line in `cases.list`; `conformance generate DIR` writes every case under DIR, which must be empty or new, and the new case's files are copied into `corpus/cases/`. Its goldens are vtium's own: they land in a commit of their own, reviewed as any golden change is (CONTRIBUTING.md).

## Recording programs

`conformance-record OUT [NAME...]` records the corpus's `prog-*` cases. It runs bash, less, vim and tmux in a pty with a fixed size and environment, with demo files dated 2026-01-01 UTC as the working directory, in a directory of its own made fresh under the system's temporary directory and removed after. It types each case's keys, keeps every byte the program writes, and cuts where each screen settled: after 0.4 s of quiet, or 5 s at most. It writes `OUT/<name>.bin` and `OUT/<name>.cuts`, and prints the case's line for `cases.list`. It answers no query the program sends. It spawns through vtium-platform's pty, so it is built only with the crate's `record` feature, and it is not a gate. A recording carries this machine's program versions: `prog-less` and `prog-vim` record byte for byte as the copied cases; `prog-shell` differs only in `ls -l`'s owner and directory size; `prog-tmux` differs with the tmux version. A new or changed recording lands in a commit of its own, its goldens reviewed as any golden change is.

## The interface check

`conformance interface`, and every `conformance run` before its cases, checks vtium-replay, the copied goldens and vtium's own goldens against the interface's own blocks: every line of each golden in `corpus/golden/`, `corpus/golden-frame/`, `vtium/golden/`, `vtium/golden-frame/` and `vtium/golden-display/` is a form of its kind in the grammar, the kinds follow the view's `structure`, and each block keeps the numbering rules of INTERFACE.md's "Blocks", at its size in `cases.list`; every worked example replays to its output block, but one that needs a feature or a view vtium-replay does not declare, which is counted; the frame view resolves each row of the `palettes` block; each input of the refusals table exits as the interface gives; and the grammar rejects seventeen outputs known to be bad and accepts one known to be good, a reserved key only with the reserved keys allowed, and one with good `copy`, `state` and `bell` lines. It also rejects ten outputs whose `anim` lines each break one of the rules in INTERFACE-vtium.md, each with a fault that names the rule broken, and accepts one with good `anim` lines. Each fault prints as `FAIL replay-interface: ...` and fails the run.

## The replay

`vtium-replay` offers the screen and frame views, and the commands and features interface 1.8 makes mandatory: `attrs-extended`, `view-frame`, `sync`, `identify`, `input`, `focus`, `palette`, `query`, and vtium's `clipboard`, `attrs-renditions`, `selection`, `marks`, `links`, `backspace`, `state`, `bell`, `key-events`, `font-ops` and `window-ops`. With the crate's `images` feature, which `make conformance` and `make conformance-probes` turn on, it carries the image pipeline (ADR-1100) and offers `images`, `sixel`, `kitty-graphics` and `image-host` with their commands, and vtium's `kitty-animation`; built without it, as `cargo test --workspace` builds the crate, those commands exit 2 as the interface asks, and `conformance run` refuses to run, since the goldens hold what the build with images prints. The feature is opt-in so that a workspace build without features keeps vtium-term without images. With the crate's `bidi` feature, which `make conformance` and `make conformance-probes` turn on as well, vtium-term draws right-to-left text in the order the bidirectional algorithm gives (ADR-1004), and vtium-replay offers the display view, `--display`, and the feature `bidi`; built without it, `--display` exits 2, and `conformance run` refuses to run. The draw2 view is optional (the operator's decision of 2026-10-26) and absent, and vtium-replay declines `damage-exact`, so the runner masks the changed counts on both sides.

With images, the reference host gives the store a budget of 320,000,000 bytes and a cell of 8 by 16 pixels, installs vtium-image's sixel, kitty graphics and iTerm2 handlers, turns the three protocols on, and installs the fixture host (`src/host.rs`), which answers each request a handler makes from the case's fixture files at once and prints its `host:` line among the replies at the point the request was made. A sixel string draws its picture through the sixel handler, and the `image` and `place` commands make and place pictures as a handler would. A placement's `changed` flag in the frame view says whether the placement or its image changed after the view's previous block, and is masked as the counts are. `tests/stand_in.rs` judges the store alone against the image goldens, without the sixel handler: it replaces each sixel string of one solid color in an `img-` case with the `image` and `place` commands that make the same picture, and requires the case's goldens, the placements compared as a set where D4's draw order departs.

The reference host is a scrollback of 0 lines (`sb-max` raises it, up to `SB_SIZE_MAX`), the cell size 8 by 16, the cursor blinking, mode 2026 presented, BackSpace sending 0x08, XTVERSION's text `tsm-replay`, an empty XTGETTCAP table, and the `legacy` palette, which is vtium-term's default. Each `termcap` fixture is checked against the interface's bounds before vtium-term's `Termcap` takes it.

The frame view prints the colors vtium-term's frame resolves, so a difference in that resolution shows as a difference from the golden. Each block's `changed` count is the cells the frame marks changed since the view's previous block, every cell for the first; each `frame-since` caller keeps its own age.

## vtium's own cases

`vtium/` holds cases for behaviour the copied corpus does not hold, such as `sixel-over`, which plays chafa's padded frames and mpv's opaque frames, whose edge ends inside a cell, over each other, and a picture with a clear pixel in each cell over the last, the cases that tell vtium's per-cell sixel coverage apart from cutting a whole box. Its files are vtium's own, and the generator does not write them. `conformance run` replays each in both views after the corpus, and in the display view too when `golden-display/` holds its golden, and compares it with its golden, masked as the corpus is, and puts it through the chunking check. A golden is vtium's own output, and it lands in a commit of its own. Until the maintainer reviews it, its case is listed in `provisional.tsv`, `CASE REASON` a line, and the golden judges nothing: a difference from it is printed as `provisional` and fails nothing. The list covers the corpus's cases too: a case of the corpus whose golden waits for work an open issue owes is listed there, with that issue in its reason, and the capability the case needs is a `missing` line of `capabilities.list` that names the issue. A case not listed there fails on any difference. A listed case that neither `cases.list` holds stops the run.

## The judge

A case in a view is judged against its golden, after INTERFACE.md's masking: a nonzero `changed` or `frame-since` count becomes `N`, and a placement's `changed` flag goes. The case passes when vtium's output equals the golden, and any other output fails, unless the provisional list names the case (vtium's own cases). A replay that exits before its end fails.

Each case also replays in each view with 1 to 8 extra cuts at random offsets outside test commands, two rounds (`--rounds`), and must end in the same last block, with the same replies over the whole replay and the same `test` lines, as the case with its own cuts (docs/architecture.md, "Determinism"). The last block's `changed` count is left out, since an extra block moves it. The seed is `GITHUB_RUN_ID` when it is set, so each CI run tries other cuts, else 1, and `--seed` sets it; the report prints it, and a failure names the seed and the cuts.

Before any case runs, the runner checks `--describe` against INTERFACE.md's `describe` block (the same major version, the block's order, and every mandatory feature), and each case of the provisional list against the two `cases.list` files.

## Scenarios

A scenario is a short text file under `scenarios/` that sets up one case, sends the program's bytes and the user's keys, mouse, paste and focus, and asks for dumps of the screen or makes assertions about it. ADR-1504 decides the language (D2), the transcript (D3) and how the runner plays a scenario (D4); this section says what the tool does beyond the record. A behaviour scenario checks one behaviour, and its golden is its transcript, `NAME.transcript` beside it: each command echoed after `> `, what vtium wrote back, copied or refused for that command after `< `, and each dump in the views' own lines. A capability scenario names a capability of `capabilities.list` in a `# capability: NAME` header line and is judged by its assertions alone, which `conformance probes` does (The capability scenarios); `conformance run` reads a capability scenario and leaves it to `conformance probes`.

`conformance run`, which `make conformance` runs, plays every behaviour scenario after the corpus, in each view its dumps name, and fails on a scenario whose transcript differs, whose assertions fail, or that cannot be played, such as one that needs a feature vtium-replay does not declare. A difference from a transcript that `scenarios/provisional.tsv` lists is printed as `provisional` and fails nothing, while the scenario's assertions still judge. A failure names the scenario and the command after which the transcript first differs, with each line it expected beside the line it got. Each scenario is then played again with extra cuts, as the corpus's chunking check plays a case, and its transcript must not change. The interface check reads each behaviour scenario's assertions and each transcript's dumps against the grammar.

`conformance scenarios --write NAME` plays a new behaviour scenario, writes its transcript and adds the scenario to `scenarios/provisional.tsv`; it refuses when the transcript exists, and `--write --replace NAME` rewrites one in a golden commit without touching the list. `conformance scenarios --play NAME` plays a scenario in this terminal for the maintainer's review: it shows each command and its side effects, draws each dumped screen in its colors and attributes from the frame view, says whether the transcript on file matches, and waits for a key after each command (q stops). `conformance scenarios --bin NAME DIR` writes the scenario's replay input as `NAME.bin` and `NAME.cuts`, with its fixtures in `NAME.files/`, and prints the `vtium-replay` command line that plays it. No scenario's `.bin` is committed.

The reader holds these rules beyond the record:

- A behaviour scenario's name starts with its area and a hyphen, one of `key`, `keypad`, `mouse`, `paste`, `focus`, `select`, `view`, `palette`, `reset`, `reply` and `bidi`; a capability scenario's file is named for its capability.
- The `# spec:`, `# covers:` and `# capability:` lines belong to the header, before `size`, and `size` is the first command.
- `key` fills `ascii` with the key's own keysym for a printable ASCII key and leaves it out for any other, and fills the character from the keysym: printable ASCII and Latin-1 keysyms and the keysyms of `scenario/keys.rs`'s table make their character, and the function, keypad and modifier keys make none. `type` presses one key for each character with no modifier held, and refuses a control character, which `key` presses by name.
- `mouse` and `wheel` leave out `px` and `py`, which the interface then takes as 0, unless the line gives `px=X,Y`.
- A command that adds nothing to the stream, such as `size` or `dump`, still has a cut of its own, at the same offset as the cut before it.
- A `dump state` adds the interface's `state` command, so its `test state` lines are its only `test` lines; the transcript prints them after the views the dump names.

The transcript masks the frame view's counts whatever a scenario's `needs` line says, which holds while vtium-replay declares no `damage-exact`. A scenario that needs exact counts will need them unmasked once an implementation declares it.

Alternate scroll (mode 1007), one of the first scenarios the gap-check named, cannot be written over interface 1.8, because it is the host's call, which no test command makes. The keypad under NumLock (mode 1035), which the gap-check also named, can be written since interface 1.4, whose `key` command takes NumLock as `mods=num`.

`docs/scripts/coverage` reads a scenario's `# covers:` line and names the scenario's file in each row's `tests` column, but not while `scenarios/provisional.tsv` lists the scenario, since a provisional transcript judges nothing. It refuses the line in a capability scenario, whose rows `capabilities.list` names.

## The capability list

`conformance issues` holds `capabilities.list` and ADR-1000's issue column to GitHub. It asks `gh` once for the state of every issue on vtium/vtium, and once more for the pull requests when a number is not an issue. It fails when an issue that a missing capability names is closed, since the issue's work should have made the capability work. It also fails when an ADR-1000 row and the issues it names disagree (`src/issues.rs`): a planned, partial or weighed row names no issue, or only closed ones; an open issue is named only by supported rows, so its work is done and the issue is still open, or the work it still holds has no row; or a number in the issue column is a pull request or no issue at all. Each message names the row or the issue and says what to change. It needs the network, so it runs in CI and through `make conformance-issues`, never in `cargo test`; `src/issues.rs`'s unit tests check the rules on fixture rows.

`capabilities.list` is `NAME STATUS ISSUE ROWS NOTE` a line: STATUS `works` or `missing`; ISSUE `-` for a working capability, the vtium issue (`#N`) that owes a missing one, or the build feature a working one needs (`-Dimages`); ROWS the ADR-1000 rows its scenario judges. `coverage_map_matches_the_capability_list_and_adr_1000` (`src/coverage.rs`) fails when `COVERAGE.md` differs from the list and ADR-1000's `tests` columns. `docs/scripts/coverage --update` rewrites both: it rewrites the `tests` columns from the `covers:` tags, and then it runs that test with `VTIUM_CONFORMANCE_BLESS=1`, which writes `COVERAGE.md`. Run `docs/scripts/coverage --update` after you add or move a tag, or after you change `capabilities.list`. The plain `docs/scripts/coverage` never runs cargo, so it does not check `COVERAGE.md`.

## The capability scenarios

`conformance probes` reads every scenario under `scenarios/`, plays each capability scenario through vtium-replay's code with a cut at each `check`, and judges `capabilities.list` through them (ADR-1504 D4):

- a capability listed `works` fails the run when its scenario fails, and one listed `missing` when it passes, so the change that adds a feature also sets its line to `works` and closes its issue;
- a capability listed with a build feature passes in a build with it and fails, as absent, in one without; the list's `-Dimages` names the image pipeline with the sixel and kitty handlers, and `-Dbidi` right-to-left text (ADR-1004); the runner takes both from the build of vtium-replay it runs with, and `make conformance-probes` builds it with both, which holds the two image handlers (#48, #49), so the image capabilities and `bidi` must pass there; a feature the build does not name fails the run;
- a scenario whose capability the list does not name, a name in the list no scenario declares, and two scenarios that declare one capability each fail the run;
- a feature that INTERFACE-vtium.md says vtium owes, which the runner lists in `OWED` (`src/bin/conformance.rs`), is one vtium-replay does not declare yet: `--describe` need not declare it, and a capability scenario that needs it fails as absent, which its `missing` line expects; the runner refuses a `--describe` that declares an owed feature, so the change that adds the feature takes it off `OWED`; the kitty keyboard protocol's `key-forms` is owed (#42);
- a scenario that names a fixture reads it from `fixtures/`, and one whose fixture is missing is not judged, which fails the run: `xtgettcap` names `kmscon.termcap`, vtium's XTGETTCAP table (`tests/termcap.rs`).

A capability scenario uses the assertions of ADR-1504 D2: `check`, `want`, `want-re`, `reject`, `reject-re`, `reply`, `reply-re`, `example` and `placeholder`. A `test` line belongs to the block whose `checkpoint` follows it, and the `want` and `want-re` assertions on `test` lines take them in order: each matches a line after the one the assertion before it matched. A regular expression uses no backreference or lookaround.

Every line the replay prints must parse under the interface's grammar for the view, or the scenario fails. Unless the scenario needs `damage-exact` and the replay declares it, which vtium-replay does not, the output and the `want` and `reject` lines are masked as the interface's "Comparing with the goldens" says, and an expectation writes such a count as `N`.

Before anything runs, each capability scenario is checked: each test command it sends is a command of the interface and each feature its `needs` line names is a feature of it; each `want` and `reject` line, and each `want-re` example, parses under the grammar for the scenario's view, with the reserved keys allowed only for a missing capability; and each `want-re` and `reply-re` matches its example, so a missing capability's expectations are lines and replies the feature could print and a typo reads as malformed instead of as still missing. A nonzero count or a placement's `changed` flag in a scenario that does not need `damage-exact` is refused with the advice to write `N`. Each assertion follows a `check` with no command between, and each capability scenario has a `want`, `want-re`, `reply` or `reply-re`, or is a placeholder.
