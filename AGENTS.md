# AGENTS.md

This file tells coding agents, and the people who run them, how to work on vtium. [CONTRIBUTING.md](CONTRIBUTING.md) sets what a contribution must show, and [docs/architecture.md](docs/architecture.md) describes the crates, the tiers, the determinism rules and the reply rule. This file covers the working loop: where things are, which commands prove that a change works, and how a change travels from a worktree to main.

## Who does what

| Role | Who it is and what it does |
|---|---|
| The maintainer | The human who owns the project and decides its direction. |
| The steering session | The main Claude session the maintainer works with. It plans the work, briefs delegated agents and merges their branches. |
| A delegated agent | A subagent the steering session starts for one piece of work, in its own worktree. |
| The author | The delegated agent that writes a change. |
| A fresh reviewer | A different delegated agent, which did not write the change, that reads and tests it. |

The steering session runs work in waves. A wave is a batch of delegated agents working at the same time, each on its own crate, tool or document.

## What defines vtium's behaviour

vtium's behaviour is defined by the specifications it cites, ADR-1000's tables and vtium's own goldens. A golden is a saved expected result that vtium's output is compared against. [ADR-1503](docs/architecture/project/ADR-1503-vtium-judges-itself-by-its-own-goldens-and-its-committed-tree-names-no-c-fork.md) records this decision. vtium is tested in tiers: unit tests, tiers 1 to 3, and a final review by the maintainer on a real console. [How vtium is judged](#how-vtium-is-judged) lists them.

Other terminals are external references. ADR-1000's tables compare vtium with the Linux console, xterm, foot, Ghostty, WezTerm and kitty, as each one's published documentation describes it.

Work on vtium happens only in vtium's own repositories, under the vtium organization on GitHub. Issues, pull requests and comments about vtium go there.

## Writing

- Write about vtium in the present tense everywhere you write: docs, comments, commits, PR bodies and issues. History can be told in the past tense.
- A decision record (an ADR, under `docs/architecture/`) never names a GitHub issue or PR by its number. It says in a short phrase what the issue is about: what the work builds, what was found, or what was decided, as in "the work that draws right-to-left text". The record then carries its meaning on its own, the way a statically linked program carries its libraries, where a number only points somewhere else. The issue column of ADR-1000's TSVs is the one exception, because it exists to name each row's issue. Issues, PRs, commits, code and the other docs keep using numbers.
- Say each fact once. Don't hard-wrap prose: one paragraph is one line in the file.
- The configuration files vtium ships, `crates/vtium/install/vtium.conf.example` (generated from the key table) and the head file template `crates/vtium-settings/templates/heads.conf`, wrap the prose of their comments at 78 columns and break each description into paragraphs, because people read them in a terminal editor. vtium-config keeps those comments as they are when it edits a vtium.conf. Source, docs and records keep one paragraph per line.
- Write the way a person explains something to a colleague out loud: in whole sentences with a subject and a verb, in plain words. Don't be clever. Write so that someone who has never seen this code understands it the first time they read it. If that takes more words, use more words.
- Put one idea in each sentence. Split a sentence that chains clauses. Write "vtium runs on a Linux virtual terminal (VT)—one of the text screens you can switch to with Ctrl+Alt and a function key. It takes the place of getty, the program that normally displays the login prompt.", not "vtium runs on a Linux virtual terminal (VT), one of the text screens that Ctrl+Alt and a function key switch to, in place of getty, the program that prints the login prompt there."
- In a guide, a tutorial, the README or an introduction, talk to the reader as "you" and say what they can do: "You can switch between VTs with Ctrl+Alt and a function key." Decision records, specs and code comments stay in the third person.
- Use the words readers already use for a thing, in pages people read: "renders", "images", "display server", "on supported hardware", not the project's own "draws", "pictures" or "where the machine has one". Where a standard technical term needs explaining, link a standard explanation and add a short gloss, as in "[KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the modern graphics interface".
- Say what a thing does, and how. Mention something it lacks only where a reader would expect it, and then in a sentence of its own: "It can render entirely on the CPU, so a GPU is optional.", not "It draws with no display server and no GPU."
- In pages for people who use vtium (the README, the guides, the reference pages and the concepts pages), link the GitHub issue that tracks a feature or a gap, not the decision record behind it: "See [the libseat issue](https://github.com/vtium/vtium/issues/34)." Decision records stay the links of pages for contributors: architecture.md, the VT review pages, the crates' READMEs and the records themselves.
- Point to more detail at the end of a sentence, as "See [Provenance](#provenance) for details.", not in parentheses in the middle of one.
- Start an explanation from what the reader knows and move to what they don't, as VTium in depth starts from input and output on computers the reader has used before it reaches the VT. Give an example where a rule is abstract: "For example, one monitor could run a kiosk while another provides an operator's shell."
- Write a size with a multiplication sign (320×240, an 8×8 font), and put a comma before the last item of a list ("color emoji, 24-bit color, mouse reporting, and images").
- Unpack a compressed phrase into what it means. For example, "A frit of bitmap sources that states no cell takes its fonts' own cell" becomes "A frit made only of bitmap fonts, with no `cell=` line, draws at the fonts' own glyph size."
- A second example of unpacking: "Under the closed policy, a keyboard no rule mentions types nowhere, the guards are off, and no failure of vtium hands its VT to a getty's login prompt." becomes "Under the closed policy, vtium ignores any keyboard that no rule names, so its keys reach no session. The two guards that otherwise keep you from locking yourself out are off. If vtium fails, its VT is left with no login prompt." The first version packs three facts into one sentence, and "types nowhere" hides who acts. The second gives each fact its own sentence and names vtium as the one that acts.
- A sentence in a page people read stays under about 25 words. `docs/scripts/prose` lists each sentence over 30 words in those pages, and each page whose sentences average over 20 words. Run `docs/scripts/prose --table` with the pages you changed, before and after you edit them. A sentence that must stay longer, for example one that quotes a long string, goes in `docs/scripts/prose-allow.tsv` with the reason it stays.
- Define a project term the first time a section uses it, in the words a newcomer would use, as in "a frit (a font set file)". Prefer the everyday word where one exists.
- Name who does what. Write "vtium ignores the keyboard", not "the keyboard falls to the policy's input default".
- If you had to read a sentence twice, rewrite the sentence before you commit.
- In an instruction, name the thing again instead of writing "it". Someone following steps reads one step at a time and acts on it, so a pronoun makes them guess which thing the step means. Write "Copy the file before you plant the defect, and afterwards restore the file from the copy.", not "Copy the file before you plant the defect, and afterwards restore it."
- When you brief a subagent that will write prose for people to read, put these rules in the brief so that it follows them.
- Write the name as "vtium" in running text, code, paths and commands. Write "VTium" in a title or heading that names the project, with VT capitalized the way the hardware terminals printed it. Spell it "color", not "colour".

## Strings on screen

A string on screen is any text vtium, vtium-config or `vtium check-config` shows a person: a notice on the console, an error about vtium.conf, a help page, a log line read after a failed start. Whoever writes or rewords one follows these rules.

1. **The topic comes first.** Say what the message is about, then the details and what to do. Write "This rule has no `->`. A rule reads `display MATCH -> TARGET` or `input MATCH -> TARGET`.", not "A rule reads `display MATCH -> TARGET` or `input MATCH -> TARGET`, and this one has no `->`."
2. **Use the reader's words, not the project's.** Write "font set" for a frit and "connected" for a device that is present: "Look big uses the font set monoid", not "The look big names the frit monoid". A project term that stays, such as head or look, is explained the first time a screen uses it.
3. **Name things in full.** Write "the font helper", not "the helper", and "character cell", not "cell": "cannot start the font helper /usr/bin/vtium-config", not "cannot start /usr/bin/vtium-config".
4. **Say what something can do, from the reader's side.** Write "The font can display 40 of the 51 characters in the preview.", not "It draws 40 of the specimen's 51 characters itself."
5. **Keep "this terminal" apart from "the console".** "This terminal" is the one vtium-config runs in; "the console" is the vtium console being set up. Write "Live trial is off: this terminal is not a vtium console, so t cannot try a font on it."
6. **Follow the Writing section above.** Whole sentences, plain words, and who does what: "Because of this error, vtium ignores this rule.", not "For this error vtium drops this rule alone."

## The gates

A gate is a check that must pass before a change goes to review. The gate line below lists every gate in order. `make gates` is a Makefile target that runs the same commands, each to the end, and reports each exit code in the gate line's order. It runs up to 8 gates at once: `make gates JOBS=N` sets the number, and `JOBS=1` runs them one after another. Each gate builds in a cargo target directory of its own under `target/gates/`, and writes its output to a log of its own under `target/gates/logs/`, which the report names when the gate fails. When you set `CARGO_TARGET_DIR` yourself, those directories go under it. Never share one `CARGO_TARGET_DIR` between worktrees whose sources differ: cargo can give both worktrees' builds the same artifact names, and a test run in one worktree then runs a test binary built from the other's sources. The line below is the full set of gates. The steering session runs all of it on the merged tree before every push, and the author of a change runs only the checks of what it touched (see [The working loop](#the-working-loop), step 4):

```sh
cargo fmt --all --check \
  && cargo clippy --workspace --all-features --all-targets -- -D warnings \
  && cargo test --workspace --all-features \
  && cargo test --workspace \
  && cargo build --profile minspec \
  && make msrv \
  && make deny \
  && docs/scripts/adr lint --check \
  && docs/scripts/adr cite --check \
  && docs/scripts/issue-refs \
  && docs/scripts/fork-refs \
  && docs/scripts/claims \
  && docs/scripts/coverage \
  && make test-tools \
  && make conformance \
  && make conformance-probes \
  && make conformance-issues \
  && make tier2 \
  && make arm
```

`make conformance` and `make conformance-probes` read the goldens and probes in `tools/conformance/`. `make tier2` reads tier 2's goldens from `tools/tier2/goldens/`. `make conformance-issues` uses `gh` to check with GitHub that every issue named in the conformance lists is still open. Without network access it cannot run, so report it as not run, with that reason. `make deny` checks the license and the source of every crate that a lock file of vtium's names, against `deny.toml`. It needs cargo-deny at the version that the Makefile's `DENY_VERSION` names, and it tells you the `cargo install` command when that version is missing. The check of the workspace's crates against the RustSec advisory database needs the network, so it is not a gate: CI runs it as the job `advisories`, and `make advisories` runs it here. `docs/scripts/claims` checks the claim files beside the docs' figures and pages: each claim's file must exist, and the symbol it names must still be defined there. [docs/AGENTS.md](docs/AGENTS.md) says what to do when it fails.

vtium's code, tests and tools are written in Rust. Where vtium must talk to a C library, such as libseat (#34), C stays at that boundary as a binding or the thinnest possible shim, and everything around it is Rust.

When you report a gate, give its command and its result, or say why it did not run. Watch each new test fail before the change and pass after it. Watch each new check fail on a defect you planted on purpose, then pass once the defect is gone. Plant the defect on a copy of the file: `cp f f.bak`, plant, test, `mv f.bak f`, `touch f`. Never undo a planted defect with `git checkout`, because it discards work. A delegated agent may use `git stash` in its own worktree, for example to set its changes aside for a moment and measure the code before them, as long as it pops the stash straight after and leaves the stash list as it found it. Nobody uses `git stash` in the maintainer's checkout or in the `integration` worktree.

## The target set and its coverage

An ADR, or Agent Decision Record, is a document under `docs/architecture/` that records a decision. ADR-1000, in `docs/architecture/term/`, declares which capabilities vtium supports, ignores or refuses. It does so in two tables: `ADR-1000-modes.tsv` and `ADR-1000-features.tsv`. The target set is the list of what vtium aims to support: the capabilities terminals generally support, plus the maintainer's own list.

Each row has an `id` (such as `DEC-2026`, `ANSI-4` or `sgr.dim`), a `status`, a layer, and the issue that carries the work. The `status` is one of supported, partial (part of it is built), planned (vtium will build it, and its issue says how), weighed (another issue decides whether vtium builds it), ignored (accepted with no effect) or refused (deliberately not built). A mode whose note says it is not in vtium's mode table is one vtium does not recognize, and DECRQM answers it 0. The layer is the stage of the plan the row lands in, from the tracker, vtium/vtium#47, the issue that lists the plan and its order: minspec, text, images, gpu, seat or top. Rows set the scope of work: the rows an issue lists are its acceptance list, and the issue's Scope section names the crate that implements them. To find a row's id, run `grep -i <name> docs/architecture/term/ADR-1000-*.tsv`. For example, DECAWM is `DEC-7`.

When a test drives a row's behaviour through the crate that implements it and checks the result, put the tag `// covers: <id>` above its test attribute, or `# covers: <id>` in a replay file. A test of a lower layer that the row depends on gets no tag. `docs/scripts/coverage` reads the tags and checks them against the tables:

| Command | What it does |
|---|---|
| `docs/scripts/coverage` | Fails if it finds an unknown, duplicate or malformed id, a tag above a function that is not a test, a tag that is close to the right form but that it cannot read, or a `tests` column that does not match the tags. CI runs it. |
| `docs/scripts/coverage --update` | Rewrites each row's `tests` column from the tags. Run it after you add or move a tag. It is idempotent: running it a second time changes nothing. To resolve a merge conflict in the TSVs' `tests` column, run it again. |
| `docs/scripts/coverage --layer minspec --require` | Fails while any minspec row that needs a test has none. Every row needs one, apart from a mode whose note says it is not in vtium's mode table. Minspec is the default, smallest build, and the minspec slice is done when this passes. It joins CI when the vtium binary lands (#21). |

The docstring of `docs/scripts/coverage` explains how feature ids are formed. Once an id is committed it never changes, apart from the one rename that ADR-1000 records (#114). A row's `status` records the decision, and it changes only through an edit to ADR-1000. A row's coverage comes from the tags, and nobody edits it by hand.

## How vtium is judged

vtium is judged in tiers, from unit tests up to the maintainer's own review on a VT, a Linux virtual console.

| Tier | What it compares | Where it runs | Command |
|---|---|---|---|
| Unit | each crate's behaviour | here | `cargo test --workspace --all-features` |
| 1, conformance | the screen state and replies vtium produces after a stream of input bytes, written as text and compared against vtium's goldens | the replay corpus in `tools/conformance/corpus/`, through the language-neutral interface that its `INTERFACE.md` describes | `make conformance` (`tools/conformance/`, #14), which also runs the interface check |
| Capability probes | each capability vtium claims, tested through a replay, against `tools/conformance/capabilities.list` | `tools/conformance/scenarios/` | `make conformance-probes` |
| 2, pixels | what the renderer draws in memory, compared pixel for pixel | `tools/tier2/`: its goldens in `tools/tier2/goldens/` | `make tier2` |
| 3, the VM | screenshots of vtium's install tree running on a VT in a QEMU/KVM guest with a real KMS device, compared against its goldens in `tools/tier3/goldens/`; it also records size and memory figures | `tools/tier3/` | `make tier3-minimal` (the minspec variants, 1 vCPU and 512 MB), `make tier3` (every variant) |
| VT review | the maintainer looks at vtium on a real VT, guided step by step | this machine | `make vt-check GUIDE=start` ([docs/vt-review.md](docs/vt-review.md)); the vtium binary arrives with #21 |

The goldens decide whether vtium's output is right. Every golden is vtium's own (ADR-1503 D1). Some of them began as copies of another harness's results, and [tools/tier3/goldens/README.md](tools/tier3/goldens/README.md) tells that history for tier 3. A golden stays provisional until the maintainer reviews it, and a provisional golden judges nothing. The tiers list their provisional goldens in files named `provisional.tsv`.

The author of a change never rewrites, widens or regenerates the golden that judges that change. A golden changes only in a commit of its own, which lists each value that moved, and the maintainer reviews it.

Tier 3 needs `qemu-system-x86_64` with KVM, `qemu-img`, `xorriso`, `gpg`, `curl` and `tic`, and it runs as the user who starts it. Its guest images live in a cache that every vtium checkout shares, `$XDG_CACHE_HOME/vtium/tier3/images`. vtium's own scripts download the pristine (unmodified) image into the cache's `pristine/` directory and build the base image into its `base/` directory ([tools/tier3/README.md](tools/tier3/README.md)).

### The verification levels

Verification runs at five levels, which ADR-1505 decides. Each level has a stop rule, which says what the level never runs, so that no level grows into the one above it.

| Level | Who and when | What runs | Stop rule |
|---|---|---|---|
| V0, while working | the author, on each change | `cargo fmt --all --check`, clippy and the tests of the crates touched; each new test seen failing first; planted defects | never the full gate line, tier 2 in full, specimens or tier 3 |
| V1, end of card | the author, once | the gates the touched files feed: tier 1 when the card touched the terminal; `make tier2` once when it touched drawing, fonts or screens; a fuzz target in reach of the change for 60 s; one golden and specimen redraw if pictures move, only when no other branch still changes the look | no tier 3, unless the card touches the install tree, VT or session handling, DRM/KMS, the GPU context or systemd; then `make tier3-minimal` once |
| V2, integration | the steering session, at each merge into the candidate or into `integration` | the full gate line once per push; one fresh correctness review, only for logic a picture cannot show (new algorithms, provenance, damage tracking, the GPU path, concurrency) | one review round; the steering session checks the fix round by planting the defects again; a second round goes to the maintainer's eyes |
| V3, the maintainer's look | the maintainer, once per batch | one picture sheet of every moved and new golden, then the VT tour (`make vt-accept`) on the GPU path and on the CPU path | the answers are the fix list; a fix comes back to the maintainer only if it moves a picture |
| V4, release candidate | the steering session, once | `make tier3` for every variant, each fuzz target's default run, every CI job including `arm-native`, `make specimens-check` | a defect found here is fixed on the candidate and rechecked at its own level, and the whole ladder does not run again |

Two rules go with the table. The steering session sizes each card to finish in well under half of an agent's context, so that verification does not cost more than the build. Every brief names its level, the agent runs exactly that level, and a slice plan in an ADR may not add checks beyond the table: an ADR's slice list names what a slice proves, and never which tiers it runs.

### The VT review

In the VT review, the maintainer checks a build by eye on a real VT. The review runs one build at a time and starts it from one place, so that no other vtium build is left running on a VT. [docs/vt-review.md](docs/vt-review.md#this-machines-vts) describes this machine's VTs, the development VT that each run borrows, and how the development VT is refreshed after a merge.

- The steering session starts the run with `make vt-check GUIDE=...` without asking first, and tells the maintainer which VT to switch to. A delegated agent never starts a run.
- The maintainer switches to that VT (Ctrl-Alt-F<n>), follows each step of the guide, and answers y, n or skip, with a note. Sudo asks for its password in the terminal where the run started.
- When the run ends, the steering session reads the answers from the log.
- The steering session refreshes the development VT after each merge to `integration`, and a delegated agent never does. The steering session cannot type a password, so sudo's credentials must already be cached in its terminal; when they are not, it asks the maintainer to run `sudo -v` there first.

A delegated agent runs the level its brief names in its own worktree, and stops there. This machine is the maintainer's live desktop, so an agent on it never takes DRM master, never sets a display mode, never switches VT, never opens a `/dev/tty<N>` console and never runs sudo. An agent may create GL contexts on a render node, use its own ptys and run the tier 3 VM. A render node refuses dumb buffers, the plain memory buffers that the CPU draws into, with `EACCES`, so only tier 3 can prove the dumb-buffer path.

## The working loop

Work happens on this machine first. GitHub's runners are slow, so the steering session waits on them only at merge points, such as the end of a wave. `integration` is a branch in the maintainer's checkout that collects reviewed work. `stage` is a branch on GitHub that holds one batch of `integration`, frozen at a commit that passed the gates, while that batch goes to main.

The loop runs at the five verification levels that [The verification levels](#the-verification-levels) defines. The author works at V0 and V1 (step 4), the steering session runs V2 on the merged tree (steps 5 and 6), the maintainer's look is V3, and the release candidate gets V4 (step 8).

1. **The record comes first.** When a decision governs a change, the steering session writes or amends the decision's ADR before the build starts, often through a delegated agent. The maintainer reads it before it merges, as with every decision that sets direction. Then a gap-check reads the code against the ADR, statement by statement, and lists where they differ. The implementation work starts from the gaps it finds. For a brand-new feature the ADR still comes first, and the gap-check then finds everything missing.
2. **A worktree per branch,** placed beside the vtium checkout. From the vtium checkout, run `git worktree add -b feat/<topic> ../vtium-<topic> integration`. Each worktree has its own `target/` directory, so builds running in parallel never wait on each other's lock. No two agents edit the same file, and each crate has one owner at a time.
3. **The branch starts from `integration`,** so later work builds on earlier work before main sees it. `integration` reaches GitHub at each merge point. In a fresh clone that has no `integration` branch, branch from main.
4. **The author commits locally and runs the level its brief names,** V0 while it works and V1 once at the end of the card. The author does not run the full gate line; the steering session runs it once, on the merged tree. The author does not push or open a PR. The author's report gives the branch, the head commit, the gate results, and each place where the change chose between two readings of a specification, the way a PR body would.
5. **A fresh reviewer reads the diff** (`git diff integration...<branch>`) when V2 calls for a review, which is only for logic that a picture cannot show. The reviewer runs the tests of what the change touched and plants the defects that the change claims to catch. Every finding goes back to the author in one fix round, wording included, and the steering session checks that round by planting the defects again. A second round goes to the maintainer's eyes. A finding is fixed in the change that found it when the change caused it or when it sits in code the change touches or depends on, under the area's ADRs and conventions. Filing an issue does not fix a finding. Declining a finding needs evidence that it is wrong, and deferring one needs the maintainer's explicit OK for that finding.
6. **The reviewed branch merges into `integration`** with `git merge --no-ff`. The steering session runs the full gates once on the merged tree before it pushes, after one branch or after a batch of them.
7. **At a merge point,** the steering session picks a commit of `integration` that passed the full gates on the merged tree. It runs `make stage-check COMMIT=<commit>` first, which fetches main from GitHub and fails unless the commit descends from main. When the previous stage PR merged, main gained a merge commit that `integration` does not have. The check then fails until the steering session merges `origin/main` into `integration` and runs the full gates on the merged tree. Once the check passes, the steering session moves `stage` to the commit with `git push origin <commit>:stage`. `stage` only ever moves forward to a later commit of `integration`, and nobody commits on it. The steering session then opens one PR from `stage` to main, so the maintainer reads a diff that does not move, with the batch's ADR notes and goldens in it, while `integration` keeps taking new work. A fix that the maintainer's review asks for lands on `integration` and reaches main with the next batch, or the steering session moves `stage` forward to it, which starts the PR's CI again. The steering session waits for that PR's CI. A PR merges only once every CI job has passed, `arm-native` included. vtium has to keep working on platforms other than the x86 machines it is written on, and `arm-native` runs on ARM hardware like the hardware vtium's smallest build is most likely to run on. A PR merges with a regular merge commit, never a squash or a rebase. A release is tagged on main.
8. **Before a release, V4 runs on the release candidate**, the `stage` commit whose PR goes to main and is then tagged, and the maintainer's V3 look covers the build on this machine. A defect either finds is fixed on `integration` and moves `stage` forward, and it is rechecked at its own level. The tag and the published packages wait for V4, for V3 and for the maintainer's word.

### Testing rules

- Before you delete a test because the golden or other reference it compared against is going away, check which defects only that test would catch, and keep a test that still catches each of them.
- A stand-in (a stub or fake) that a test uses in place of a real part must check what the real part checks. A test's stand-in for the glyph helper once accepted any frit (font set file) name, so the real helper's refusal of a bad name surfaced only in tier 3.
- Fix a bug by fixing the state that is wrong, never with timing: no delays, retries or "redraw a moment later". A test checks that state right after the event that should change it, and counts turns or frames instead of measuring wall time. When a test has to wait for another thread, bound the wait only so the test fails instead of hanging. A console switch that kept drawing the old console was fixed by resetting what each display remembered drawing, not by redrawing later.
- A fuzz target is a cargo-fuzz program in a crate's `fuzz/` directory that feeds generated inputs to the crate's code, and the crate's fuzz README says what each target reaches. The author of a change to code that a fuzz target reaches runs that target at V1 for the default 60 s with `make fuzz FUZZ_TARGETS=<crate>:<target>`, and commits with the change the rows the run appends to `crates/<crate>/fuzz/runs.tsv`. The reviewer checks that those rows exist, that they read ok, or that the crash a row records is fixed or filed, and that their commit column names the change's commit, or its parent with -dirty when the author ran the target before committing. Fuzzing is not a gate, because it needs a nightly toolchain and minutes of wall time.

### Cooperating across worktrees

Several agents work at once, each in its own worktree, and the steering session plans the work so that they never collide:

- **One owner per file set.** In each wave, the steering session gives each agent a crate, a tool or a document of its own, and names it in the agent's brief. An agent that needs a change in a file it does not own reports the change to the steering session, which routes it to the owner or schedules it. Agents never edit each other's worktrees.
- **Shared files merge by their tools.** Add a new crate under `crates/`. The workspace's `members = ["crates/*"]` picks it up, so a new crate never needs an edit to the root manifest's member list. Only `docs/scripts/coverage --update` writes ADR-1000's `tests` column, so when two branches tag different rows, rerunning it resolves the conflict. `docs/specimens/` holds sample renderings, and only `make specimens` writes its files, apart from the hand-written `provisional.tsv`. A branch that changes specimens regenerates them in a commit of their own, and a conflict there is resolved by running `make specimens-fonts && make specimens`.
- **Picking up others' work.** When a branch lands on `integration` and a running agent needs it, the steering session sends that agent a message. The message reaches the agent when its current turn ends, and the agent then runs `git merge integration` in its own worktree. For example, every agent needed #67's ids. Agents never rebase a branch that has been shared.
- **Order follows the dependency graph.** A wave includes only items whose dependencies have already landed on `integration`. Work that others are waiting on, such as a contract that several branches cite, merges first.
- **Scratch is per agent.** An agent keeps its scratch files in a directory named for it, never at a shared path that another agent might also write to.
- **Reports, not chatter.** A delegated agent messages the steering session only when it finishes, when it is blocked, or when it needs a file it does not own. The steering session passes on what one agent needs from another.
- **The brief carries everything.** A message to a delegated agent reaches it only when it ends a turn, so a follow-up sent while the agent is working may arrive after it has already reported. The steering session therefore puts every instruction in the brief.
- **Gates run in the foreground.** A delegated agent runs every gate in the foreground. It never ends its turn to wait on a Monitor (Claude Code's tool for watching a background job) or on a background task of its own, because the notices from those go to the steering session.

Create and change ADRs with `docs/scripts/adr`. It follows contract adr/v1 and numbers ADRs by domain: term 1000, image 1100, font 1200, render 1300, platform 1400, project 1500.

The steering session releases each delegated agent explicitly once its work has merged. A message still queued for an agent can revive it after its context resets, and two revived agents in one worktree collide.

### Signed commits

Every commit to vtium is signed with the maintainer's SSH key, from 2026-11-10 on. The signature shows that the commit was made on the maintainer's machine, under the maintainer's control. It is part of the project's provenance, beside the decision records that quote the maintainer's words. The maintainer's global git config signs each commit and tag by itself, so an agent commits as usual and does nothing extra.

- When a commit fails because the signing key is not loaded, the agent stops and reports it. The maintainer loads the key with `ssh-add`.
- An agent never turns signing off with `-c commit.gpgsign=false` or `--no-gpg-sign`, and never skips the hooks with `--no-verify`.
- The pre-push hook in `tools/git-hooks/` refuses a push that carries an unsigned commit made after signing began. `git config core.hooksPath tools/git-hooks` turns it on. A commit made before that date passes unchecked, and its history stays as it was.

## Where to look

| For | Read |
|---|---|
| The crates, the tiers, determinism, the reply rule, tables and maps | [docs/architecture.md](docs/architecture.md) |
| Work under docs/: pages for users and contributors, generated pages, figures and their claim files | [docs/AGENTS.md](docs/AGENTS.md) |
| What a contribution must show, the code rules, the kitty source rule | [CONTRIBUTING.md](CONTRIBUTING.md) |
| The target set and its coverage | `docs/architecture/term/ADR-1000-*` |
| A crate's bounds and the choices it makes | `crates/<crate>/README.md` |
| Where vtium touches systemd, logind or PID 1, and what it does without them | [docs/systemd.md](docs/systemd.md) |
| The plan and its order | the tracker, vtium/vtium#47 |
| The replay interface | `tools/conformance/corpus/INTERFACE.md` |
| Tier 3 | [tools/tier3/README.md](tools/tier3/README.md) |
