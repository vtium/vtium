# conformance

Tier 1 for vtium. `vtium-replay` meets the replay interface, interface 1.9: `corpus/INTERFACE.md`, interface 1.0, with vtium's additions in `INTERFACE-vtium.md`, over `vtium-parse` and `vtium-term`. `conformance` replays every case of the corpus through it and judges each against its golden, plays every behaviour scenario through it and judges each against its transcript, and plays every capability scenario through it, each judged by vtium's capability list. The goldens are the judge: every difference from a golden fails, unless the provisional list names the case.

The corpus, its goldens, the interface and the scenarios are kept here (The copies). The tools that make and check them are written here in Rust: the generator of the synthetic cases, the recorder of the programs' cases, and the interface check. Nothing here reads another repository or runs code from outside the tree.

The crate is a workspace member, so `cargo test --workspace` builds both programs and runs the tests below. The whole corpus and the capability scenarios run in two Makefile targets, which CI's `conformance` job runs on every pull request:

```
make conformance          # every case, both views and the chunking check
make conformance-probes   # the capability list, through the capability scenarios
make conformance-issues   # the capability list and ADR-1000 agree with GitHub (gh)
make conformance-record OUT=DIR [NAMES='prog-shell ...']   # records the programs' cases; not a gate
make tier1-shots [OUT=DIR]  # draws every golden as a picture for review; not a gate
make tier1-play CASE=NAME   # plays a case in this terminal, one screen at a time; or SCENARIO=NAME
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
| `src/replay.rs` | The replay: the reference host's settings, the feed between cuts, the test commands and the head of each block. `run_with` hands the terminal at each block to a caller, which `make tier1-shots` draws. |
| `src/replay/links.rs` | The `link` and `hover` commands: the URI of a cell's OSC 8 link, and the cells and overlay row of the link under the pointer. |
| `src/command.rs` | The test commands of interface 1, each value checked against its form and range. |
| `src/live.rs` | The live path: a replay's terminal at one block written as the bytes that make another terminal show the same screen, and the reading of those bytes back through a terminal. |
| `src/view.rs` | The screen and frame views, from vtium-term's lines and frame, and with images the `placement` and `image` lines. |
| `src/replay/images.rs` | With images: the reference host's image settings, the image commands, and the fixture host answering each request. |
| `src/host.rs` | With images: the fixture host, its `host:` line and its answer from the case's fixture files. |
| `src/spec.rs` | The interface's `describe` and `grammar` blocks, over 1.0 and vtium's additions, and the check of a line against the grammar. |
| `src/runner.rs` | The corpus, the masking and the verdict on each case. |
| `src/runner/chunking.rs` | The chunking check: each case with extra cuts at random offsets. |
| `src/scenario.rs`, `src/scenario/` | The scenarios: the reader and its commands (`commands.rs`), the key names (`keys.rs`), the string syntax that the scenarios and the worked examples share (`strings.rs`), the transcript printer and its check (`transcript.rs`), the play through vtium-replay's code (`run.rs`), the capability rules (`capability.rs`) and the player for review and the key reader both players share (`play.rs`). |
| `src/bin/conformance/scenarios.rs` | `conformance scenarios`, and the scenarios' part of `conformance run` and `conformance interface`. |
| `scenarios/` | The scenarios, each `NAME.scenario` with a behaviour scenario's transcript `NAME.transcript` beside it, and `provisional.tsv`, the behaviour scenarios whose transcripts wait to be accepted. |
| `src/probes.rs` | A scenario's input and assertions as the judge reads them, and the checks made before a capability scenario runs. |
| `src/probes/judge.rs` | A capability scenario replayed and judged, and each capability's verdict against the list. |
| `src/coverage.rs` | `COVERAGE.md`, generated from `capabilities.list` and ADR-1000's tables, and its test. |
| `src/interface.rs`, `src/interface/` | The interface check: the goldens under the grammar, the worked examples, the palettes, the refusals and the grammar's controls. |
| `src/generate.rs`, `src/generate/` | The synthetic cases' recipes, and their writer. |
| `generator/` | The generator's copied inputs: `recorded/`, streams a client wrote, and `sixel/`, libsixel's `img2sixel` output. |
| `src/record.rs`, `src/bin/conformance-record.rs` | The recorder of the programs' cases, built with the `record` feature. |
| `fixtures/kmscon.termcap` | The xtgettcap capability scenario's termcap fixture: vtium's XTGETTCAP table. |
| `src/bin/conformance.rs` | The runner's commands and its report. |
| `corpus/` | The copied corpus: `INTERFACE.md`, `cases.list`, `cases/` with each case's stream and cuts, or its readable case (`kit-chunked.case`, `kit-chunk-abort.case`), and its fixtures, and the goldens, `golden/` for the screen view and `golden-frame/` for the frame view. |
| `vtium/` | vtium's own cases, laid out as `corpus/` is: `cases.list`, `cases/` (each case a stream, `NAME.bin`, or a readable case, `NAME.case`), `golden/` and `golden-frame/`, `golden-display/` for the display view of the cases of right-to-left text, and `provisional.tsv`, the provisional list: the cases, of the corpus or of vtium's own, whose goldens wait to be accepted. `expect/` holds the sealed expectations of the `kitty-upload-*` and `kitty-featq-*` cases: each `NAME.expect` lists the replies that ADR-1100 D16 and D14 give, written before vtium ran the case. The runner does not read them. Whoever reviews those goldens compares the replies with them. |
| `src/runner/own.rs` | Reading `vtium/provisional.tsv`, the provisional list. |
| `INTERFACE-vtium.md` | vtium's additions to the interface: the `clipboard` command and feature and the line it prints, the `attrs-renditions` feature, which prints the keys 1.0 reserves for the overline, superscript and subscript, the `kitty-animation` feature and its `anim` lines, the display view and the `bidi` feature, the `select`, `copy`, `prompt`, `link`, `hover`, `backspace` and `state` commands with their features and lines, the `bell` line, the `font-ops` and `window-ops` features with their commands and lines, the `key-events` feature, whose `key` command takes a key's event, and the `key-forms` feature, whose `key` command takes a key's unshifted key and base layout key. |
| `capabilities.list` | vtium's capability list: each capability `works` or `missing`, with the issue for each gap and the ADR-1000 rows it judges. |
| `COVERAGE.md` | Each capability, the tests that guard its rows, and its gap. |
| `tests/` | The verdicts, the command line, the scenarios and the capability judge on inputs written in the tests: see its README. |

## Tier 1's pictures

Tier 1's goldens are text. Where the maintainer reviews one (ADR-1503 D10, ADR-1505 V3), the maintainer sees it as a picture that vtium draws. `make tier1-shots` draws them. It builds `tier1-shots` from `tools/tier1-shots/`, with the feature `draw`, which turns on vtium-replay's `images` and `bidi` features. The pictures are review renderings, not goldens. Nothing in the output is committed, and the tool is not a gate.

The tool draws three groups of pictures:

- **corpus** and **vtium**: each case of `corpus/cases.list` and `vtium/cases.list`. The tool replays the case through vtium-replay's own replay, with its test commands, the fixture host and its fixtures, at the 8×16 reference cell. It replays the case once in each view that `conformance run` judges it in. Those are the screen and frame views, and the display view when `golden-display/` holds the case's golden. It then draws the terminal as the screen view's last block leaves it, after every test command.
- **scenarios**: each screen that a behaviour scenario dumps. The tool plays the scenario as `conformance run` plays it and checks the transcript on file. It replays the scenario once more and draws the terminal at each `dump`'s block, in order. A behaviour scenario that dumps no screen has no picture, and the tool prints its name. A capability scenario has no transcript and no picture.

Each picture is drawn through vtium's renderer at the size of the terminal's grid. Its text draws as the console draws it with no font set, through the console's built-in face: GNU Unifont, in cells of 8×16 pixels. A cluster that the font has no glyph for draws as its NFC form, or as its base with its combining marks over it (ADR-1200 D22). A screen that shows Arabic draws instead in tier 2's scripts font set, the fonts of tier 2's `scripts-arabic` screen. That set is Iosevka Term in its 16×40 cell at 40 pixels, with Noto Sans Arabic behind it, so each Arabic letter takes its joined form. `make tier2-fonts` fetches those fonts. When they are missing, the tool draws no picture for that screen, and it prints the missing font and the command that fetches it. Text and pictures, kitty, sixel and iTerm2, draw together. The cursor shows where the terminal shows it, and blinking text shows. Two runs write the same bytes.

The tool checks each output against its golden as `conformance run` does, after the same masking. A case or a scenario whose output differs from its golden is still drawn. The tool prints each such picture with the lines that differ, `-` for the golden's and `+` for the output's, each with its line number. A difference does not fail the run, since `conformance run` judges the goldens. The tool exits 1 when a replay or a drawing fails, and 2 when it refuses to run.

### The live path

Each picture has a live path. `make tier1-play CASE=NAME` plays a case in the terminal it runs in, one screen at a time, and `make tier1-play SCENARIO=NAME` plays a scenario. The player waits for a key after each screen, and q stops it. Inside a vtium console, vtium draws each screen itself, its kitty, sixel and iTerm2 pictures included.

The picture and the live play come from one replay of the same input, split at the same cuts. At each block, the replay hands its terminal to both paths. The picture path draws the terminal, and the live path (`src/live.rs`) writes bytes that make another terminal show the same screen. A case stops at every checkpoint, as `prog-vim` stops at each of its screens, and its last screen is the case's picture. A scenario plays through `conformance scenarios --play`'s player, which draws each dump through the live path.

The live path does not send the case's own stream. A test command, an answer of the fixture host or a picture made in another cell has no sequence that a terminal reads. So the live path describes the screen the replay left:

- Each cell is written at its own column, in its resolved colors as direct color, with its attributes, in the width method that gives it its width. The rows go in the order they draw, in explicit bidirectional mode.
- Each picture is sent through the kitty graphics protocol as the renderer draws it. vtium-render's `placed_picture` gives the part drawn and the pixels the renderer draws there, scaled from the picture's crop and cut to the screen. The live path sends those pixels at their own size, at their place by cell and pixel offset, with the picture's layer. The reading terminal then draws the renderer's pixels, whatever cell the picture was made in.
- The cursor goes to its cell in its shape, blink and color.

`make tier1-shots` checks each picture against its live path. It reads the live path's bytes for that block back through a terminal of the replay's size, and draws that terminal as the picture path draws. When the two drawings differ, the tool prints the picture's name and writes the live path's drawing beside the picture. Today all 250 pictures match their live play.

The output goes to `OUT`, `target/tier1-shots/` by default. `OUT` must be new, empty, or the output of an earlier run, which holds a `manifest.json`. The tool removes an earlier run's pictures and manifest first. It writes:

| Path | What it is |
|---|---|
| `corpus/NAME.png`, `vtium/NAME.png` | The picture of a replay case. |
| `scenarios/NAME-NN.png` | The picture of a scenario's dump `NN`, counted from `01`. |
| `NAME.live.png`, `NAME-NN.live.png` | The live path's drawing of a screen, beside its picture, only when it differs from the picture. |
| `manifest.json` | Every picture, in the order drawn: the corpus's cases, vtium's cases, then the scenarios by name. |

`manifest.json` holds `cell`, `[8, 16]`, and `font`, `"unifont"`, the cell and the font of a picture that shows no Arabic. It also holds `pictures`, a list with one object for each picture:

| Field | What it holds |
|---|---|
| `file` | The picture's path in the output. |
| `group` | `corpus`, `vtium` or `scenarios`. |
| `name` | The case's or the scenario's name. |
| `dump` | For a scenario's picture, `number` (from 1), `of` (the scenario's dumps), `command` (the `dump` as the transcript echoes it) and `line` (its line in the scenario's file). `null` for a case. |
| `width`, `height` | The picture's size in pixels. |
| `cell` | The size in pixels of the picture's character cell, or `[0, 0]` when there is no picture. |
| `font` | `unifont` for the console's built-in face, or `tier2-scripts` for tier 2's scripts font set, which draws a screen that shows Arabic. |
| `provisional` | The reason the provisional list gives, or `null` for a reviewed golden. |
| `matches` | Whether the output equals every golden that judges the picture. |
| `error` | Why a replay or a drawing stopped, or `null`. |
| `play` | The exact command that plays the picture's case or scenario live, as `make tier1-play CASE=NAME` or `make tier1-play SCENARIO=NAME`. |
| `live` | `matches`, whether the live path's bytes for this screen draw this picture, and `file`, the live path's drawing when it differs, or `null`. |
| `goldens` | One object for each golden: `view` (`screen`, `frame`, `display` or `transcript`), `path` (from the repository's root), `matches`, and `differs`, the lines that differ. Each line has `side` (`golden` or `output`), `line` (its number on its side) and `text` (the line as its file holds it). |

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

`vtium-replay` offers the screen and frame views, and the commands and features interface 1.9 makes mandatory: `attrs-extended`, `view-frame`, `sync`, `identify`, `input`, `focus`, `palette`, `query`, and vtium's `clipboard`, `attrs-renditions`, `selection`, `marks`, `links`, `backspace`, `state`, `bell`, `key-events`, `key-forms`, `font-ops` and `window-ops`. With the crate's `images` feature, which `make conformance` and `make conformance-probes` turn on, it carries the image pipeline (ADR-1100) and offers `images`, `sixel`, `kitty-graphics` and `image-host` with their commands, and vtium's `kitty-animation`; built without it, as `cargo test --workspace` builds the crate, those commands exit 2 as the interface asks, and `conformance run` refuses to run, since the goldens hold what the build with images prints. The feature is opt-in so that a workspace build without features keeps vtium-term without images. With the crate's `bidi` feature, which `make conformance` and `make conformance-probes` turn on as well, vtium-term draws right-to-left text in the order the bidirectional algorithm gives (ADR-1004), and vtium-replay offers the display view, `--display`, and the feature `bidi`; built without it, `--display` exits 2, and `conformance run` refuses to run. The draw2 view is optional (the operator's decision of 2026-10-26) and absent, and vtium-replay declines `damage-exact`, so the runner masks the changed counts on both sides.

With images, the reference host gives the store a budget of 320,000,000 bytes and a cell of 8 by 16 pixels, installs vtium-image's sixel, kitty graphics and iTerm2 handlers, turns the three protocols on, and installs the fixture host (`src/host.rs`), which answers each request a handler makes from the case's fixture files at once and prints its `host:` line among the replies at the point the request was made. A sixel string draws its picture through the sixel handler, and the `image` and `place` commands make and place pictures as a handler would. A placement's `changed` flag in the frame view says whether the placement or its image changed after the view's previous block, and is masked as the counts are. `tests/stand_in.rs` judges the store alone against the image goldens, without the sixel handler: it replaces each sixel string of one solid color in an `img-` case with the `image` and `place` commands that make the same picture, and requires the case's goldens, the placements compared as a set where D4's draw order departs.

The reference host is a scrollback of 0 lines (`sb-max` raises it, up to `SB_SIZE_MAX`), the cell size 8 by 16, the cursor blinking, mode 2026 presented, BackSpace sending 0x08, XTVERSION's text `tsm-replay`, an empty XTGETTCAP table, and the `legacy` palette, which is vtium-term's default. Each `termcap` fixture is checked against the interface's bounds before vtium-term's `Termcap` takes it.

The frame view prints the colors vtium-term's frame resolves, so a difference in that resolution shows as a difference from the golden. Each block's `changed` count is the cells the frame marks changed since the view's previous block, every cell for the first; each `frame-since` caller keeps its own age.

## vtium's own cases

`vtium/` holds cases for behaviour the copied corpus does not hold, such as `sixel-over`, which plays chafa's padded frames and mpv's opaque frames, whose edge ends inside a cell, over each other, and a picture with a clear pixel in each cell over the last, the cases that tell vtium's per-cell sixel coverage apart from cutting a whole box. Its files are vtium's own, and the generator does not write them. `conformance run` replays each in both views after the corpus, and in the display view too when `golden-display/` holds its golden, and compares it with its golden, masked as the corpus is, and puts it through the chunking check. A golden is vtium's own output, and it lands in a commit of its own. Until the golden is accepted, its case is listed in `provisional.tsv`, `CASE REASON` a line, and the golden judges nothing: a difference from it is printed as `provisional` and fails nothing. The list covers the corpus's cases too: a case of the corpus whose golden waits for work an open issue owes is listed there, with that issue in its reason, and the capability the case needs is a `missing` line of `capabilities.list` that names the issue. A case not listed there fails on any difference. A listed case that neither `cases.list` holds stops the run.

A case of `vtium/` is either a stream or a readable case. A stream is `cases/NAME.bin`, with its cuts in `cases/NAME.cuts`. A readable case is `cases/NAME.case`, written in the scenario language (Scenarios). Its header holds a `# spec:` line, a comment that says what the case checks, and a `# covers:` line when the case checks ADR-1000 rows. Its commands follow, from `size` on. The runner reads the file and compiles it with the scenario compiler, the code that `conformance scenarios --bin` uses. It then replays the result as it replays a stream. So no stream of a readable case is committed. The compiled input has a cut after each `dump`, except at the stream's end, where the replay prints a block anyway. `dump state` adds the interface's `state` command before its cut. A readable case has no `check` or assertion, since its goldens judge it, and it names no capability. Its `size` must agree with its line in `cases.list`. A case that has both a stream and a readable case stops the run. `docs/scripts/coverage` reads a readable case's `# covers:` line, and the line counts only once `provisional.tsv` no longer lists the case. The 54 cases that come from an outside collection of terminal test scenarios are readable cases.

## The judge

A case in a view is judged against its golden, after INTERFACE.md's masking: a nonzero `changed` or `frame-since` count becomes `N`, and a placement's `changed` flag goes. The case passes when vtium's output equals the golden, and any other output fails, unless the provisional list names the case (vtium's own cases). A replay that exits before its end fails.

Each case also replays in each view with 1 to 8 extra cuts at random offsets outside test commands, two rounds (`--rounds`), and must end in the same last block, with the same replies and the same count of bells over the whole replay and the same `test` lines, as the case with its own cuts (docs/architecture.md, "Determinism"). The last block's `changed` count and its `bell` line are left out, since an extra block moves them. The seed is `GITHUB_RUN_ID` when it is set, so each CI run tries other cuts, else 1, and `--seed` sets it; the report prints it, and a failure names the seed and the cuts.

Before any case runs, the runner checks `--describe` against INTERFACE.md's `describe` block (the same major version, the block's order, and every mandatory feature), and each case of the provisional list against the two `cases.list` files.

## Scenarios

A scenario is a short text file under `scenarios/` that sets up one case, sends the program's bytes and the user's keys, mouse, paste and focus, and asks for dumps of the screen or makes assertions about it. ADR-1504 decides the language (D2), the transcript (D3) and how the runner plays a scenario (D4); this section says what the tool does beyond the record. A behaviour scenario checks one behaviour, and its golden is its transcript, `NAME.transcript` beside it: each command echoed after `> `, what vtium wrote back, copied or refused for that command after `< `, and each dump in the views' own lines. A capability scenario names a capability of `capabilities.list` in a `# capability: NAME` header line and is judged by its assertions alone, which `conformance probes` does (The capability scenarios); `conformance run` reads a capability scenario and leaves it to `conformance probes`.

`conformance run`, which `make conformance` runs, plays every behaviour scenario after the corpus, in each view its dumps name, and fails on a scenario whose transcript differs, whose assertions fail, or that cannot be played, such as one that needs a feature vtium-replay does not declare. A difference from a transcript that `scenarios/provisional.tsv` lists is printed as `provisional` and fails nothing, while the scenario's assertions still judge. A failure names the scenario and the command after which the transcript first differs, with each line it expected beside the line it got. Each scenario is then played again with extra cuts, as the corpus's chunking check plays a case, and its transcript must not change. The interface check reads each behaviour scenario's assertions and each transcript's dumps against the grammar.

`conformance scenarios --write NAME` plays a new behaviour scenario, writes its transcript and adds the scenario to `scenarios/provisional.tsv`; it refuses when the transcript exists, and `--write --replace NAME` rewrites one in a golden commit without touching the list. `conformance scenarios --play NAME` plays a scenario in this terminal for the maintainer's review. It shows each command and its side effects, draws each dumped screen through the live path, says whether the transcript on file matches, and waits for a key after each command (q stops). The live path (`src/live.rs`, see Tier 1's pictures) makes this terminal draw the screen in its colors and attributes, with its pictures. `conformance scenarios --bin NAME DIR` writes the scenario's replay input as `NAME.bin` and `NAME.cuts`, with its fixtures in `NAME.files/`, and prints the `vtium-replay` command line that plays it. No scenario's `.bin` is committed.

The reader holds these rules beyond the record:

- A behaviour scenario's name starts with its area and a hyphen, one of `key`, `keypad`, `mouse`, `paste`, `focus`, `select`, `view`, `palette`, `reset`, `reply` and `bidi`; a capability scenario's file is named for its capability.
- The `# spec:`, `# covers:` and `# capability:` lines belong to the header, before `size`, and `size` is the first command.
- `key` fills `ascii` with the key's own keysym for a printable ASCII key and leaves it out for any other, and fills the character from the keysym: printable ASCII and Latin-1 keysyms and the keysyms of `scenario/keys.rs`'s table make their character, and the function, keypad and modifier keys make none. `type` presses one key for each character with no modifier held, and refuses a control character, which `key` presses by name.
- `mouse` and `wheel` leave out `px` and `py`, which the interface then takes as 0, unless the line gives `px=X,Y`.
- A command that adds nothing to the stream, such as `size` or `dump`, still has a cut of its own, at the same offset as the cut before it.
- A `dump state` adds the interface's `state` command, so its `test state` lines are its only `test` lines; the transcript prints them after the views the dump names.

The transcript masks the frame view's counts whatever a scenario's `needs` line says, which holds while vtium-replay declares no `damage-exact`. A scenario that needs exact counts will need them unmasked once an implementation declares it.

Alternate scroll (mode 1007), one of the first scenarios the gap-check named, cannot be written over interface 1.9, because it is the host's call, which no test command makes. The keypad under NumLock (mode 1035), which the gap-check also named, can be written since interface 1.4, whose `key` command takes NumLock as `mods=num`.

`docs/scripts/coverage` reads a scenario's `# covers:` line and names the scenario's file in each row's `tests` column, but not while `scenarios/provisional.tsv` lists the scenario, since a provisional transcript judges nothing. It refuses the line in a capability scenario, whose rows `capabilities.list` names.

## The capability list

`conformance issues` holds `capabilities.list` and ADR-1000's issue column to GitHub. It asks `gh` once for the state of every issue on vtium/vtium, and once more for the pull requests when a number is not an issue. It fails when an issue that a missing capability names is closed, since the issue's work should have made the capability work. It also fails when an ADR-1000 row and the issues it names disagree (`src/issues.rs`): a planned, partial or weighed row names no issue, or only closed ones; an open issue is named only by supported rows, so its work is done and the issue is still open, or the work it still holds has no row; or a number in the issue column is a pull request or no issue at all. Each message names the row or the issue and says what to change. It needs the network, so it runs in CI and through `make conformance-issues`, never in `cargo test`; `src/issues.rs`'s unit tests check the rules on fixture rows.

`capabilities.list` is `NAME STATUS ISSUE ROWS NOTE` a line: STATUS `works` or `missing`; ISSUE `-` for a working capability, the vtium issue (`#N`) that owes a missing one, or the build feature a working one needs (`-Dimages`); ROWS the ADR-1000 rows its scenario judges. `coverage_map_matches_the_capability_list_and_adr_1000` (`src/coverage.rs`) fails when `COVERAGE.md` differs from the list and ADR-1000's `tests` columns. `docs/scripts/coverage --update` rewrites both: it rewrites the `tests` columns from the `covers:` tags, and then it runs that test with `VTIUM_CONFORMANCE_BLESS=1`, which writes `COVERAGE.md`. Run `docs/scripts/coverage --update` after you add or move a tag, or after you change `capabilities.list`. The plain `docs/scripts/coverage` never runs cargo, so it does not check `COVERAGE.md`.

## The capability scenarios

`conformance probes` reads every scenario under `scenarios/`, plays each capability scenario through vtium-replay's code with a cut at each `check`, and judges `capabilities.list` through them (ADR-1504 D4):

- a capability listed `works` fails the run when its scenario fails, and one listed `missing` when it passes, so the change that adds a feature also sets its line to `works` and closes its issue;
- a capability listed with a build feature passes in a build with it and fails, as absent, in one without; the list's `-Dimages` names the image pipeline with the sixel and kitty handlers, and `-Dbidi` right-to-left text (ADR-1004); the runner takes both from the build of vtium-replay it runs with, and `make conformance-probes` builds it with both, which holds the two image handlers (#48, #49), so the image capabilities and `bidi` must pass there; a feature the build does not name fails the run;
- a scenario whose capability the list does not name, a name in the list no scenario declares, and two scenarios that declare one capability each fail the run;
- a feature that INTERFACE-vtium.md says vtium owes, which the runner lists in `OWED` (`src/bin/conformance.rs`), is one vtium-replay does not declare yet: `--describe` need not declare it, and a capability scenario that needs it fails as absent, which its `missing` line expects; the runner refuses a `--describe` that declares an owed feature, so the change that adds the feature takes it off `OWED`; no feature is owed today;
- a scenario that names a fixture reads it from `fixtures/`, and one whose fixture is missing is not judged, which fails the run: `xtgettcap` names `kmscon.termcap`, vtium's XTGETTCAP table (`tests/termcap.rs`).

A capability scenario uses the assertions of ADR-1504 D2: `check`, `want`, `want-re`, `reject`, `reject-re`, `reply`, `reply-re`, `example` and `placeholder`. A `test` line belongs to the block whose `checkpoint` follows it, and the `want` and `want-re` assertions on `test` lines take them in order: each matches a line after the one the assertion before it matched. A regular expression uses no backreference or lookaround.

Every line the replay prints must parse under the interface's grammar for the view, or the scenario fails. Unless the scenario needs `damage-exact` and the replay declares it, which vtium-replay does not, the output and the `want` and `reject` lines are masked as the interface's "Comparing with the goldens" says, and an expectation writes such a count as `N`.

Before anything runs, each capability scenario is checked: each test command it sends is a command of the interface and each feature its `needs` line names is a feature of it; each `want` and `reject` line, and each `want-re` example, parses under the grammar for the scenario's view, with the reserved keys allowed only for a missing capability; and each `want-re` and `reply-re` matches its example, so a missing capability's expectations are lines and replies the feature could print and a typo reads as malformed instead of as still missing. A nonzero count or a placement's `changed` flag in a scenario that does not need `damage-exact` is refused with the advice to write `N`. Each assertion follows a `check` with no command between, and each capability scenario has a `want`, `want-re`, `reply` or `reply-re`, or is a placeholder.
