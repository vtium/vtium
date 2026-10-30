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

## The repositories

The C reference is the C terminal kmscon and its screen and parser library libtsm, which vtium is measured against. The maintainer keeps a fork of each beside the vtium checkout, and the forks hold the C reference's code. vtium copied what it needs from the forks into its own tree (#111, the issue that ported the C reference's test machinery into vtium), and it records the provenance of each copy: the file and the commit it came from. Many of the copies are goldens: saved expected results that vtium's output is compared against. vtium is tested in tiers: unit tests, tiers 1 to 3, and a final review by the maintainer on a real console. [How vtium is judged](#how-vtium-is-judged) lists them.

| Repository | Path beside this checkout | Role |
|---|---|---|
| vtium/vtium | `.` | vtium itself |
| aaronsb/kmscon | `../kmscon` | The C reference's terminal. vtium copied its capability probes and its tier 2 and tier 3 goldens from here (#111). |
| aaronsb/libtsm | `../libtsm` | The C reference's screen and parser. vtium copied tier 1's replay interface and goldens from here into `tools/conformance/`, with their provenance. The replay interface is the command-line contract by which a program plays a byte stream into a terminal and prints the resulting screen as text. vtium also ported the tools that made and checked them into `tools/conformance/`, in Rust: the case generator, the recorder and the interface check. |

The C reference is frozen. vtium does at least everything the C reference did, and may do more. It does not copy how the C reference was built, because vtium is idiomatic Rust. Agents read the forks and never change them on their own. A fork changes only when the maintainer asks for a fix, and then as the maintainer directs. No gate, CI job, build or tool runs the C reference's code or reads the forks' checkouts, except the three provenance checks described under [The gates](#the-gates). The local checkouts of the forks are often out of date, so fetch first and read from `origin/main`, for example `git -C ../kmscon show origin/main:<path>`. Plain `gh` in this checkout talks to vtium/vtium, so every `gh` command for a fork needs `-R aaronsb/kmscon` or `-R aaronsb/libtsm`.

Before you design anything new, answer each question about behaviour or structure from the C reference. Start with [docs/c-reference.md](docs/c-reference.md), then read the C reference's code in the forks at `origin/main`. Take the C reference's logic flow, and never its code structure.

Diagnose a problem in vtium's implementation the same way. First find out how the C reference's flow handled the same case and what it decided, and cite that. Treat it as a reference for the logic, never as C code to port. This applies to every change, whether it is a bug fix, a review finding or a failing test.

Upstream means the original projects the forks came from, `kmscon/kmscon` and `kmscon/libtsm`. Nobody working on vtium writes to them: no issue, no PR, no comment, and no tag of an upstream maintainer. When vtium's text names upstream, it uses a code span and no link. When you find a defect in the C reference, a security defect included, fix it in vtium, list it as a departure (a recorded difference from the C reference), and report nothing upstream.

## Writing

- Write about vtium in the present tense everywhere you write: docs, comments, commits, PR bodies and issues. Call the C code "the C reference" and write about it in the past tense ("the C reference replied 4"). History can be told in the past tense.
- A spec describes behaviour that vtium implements. Put C function names in a section marked as C notes, not in the behaviour spec.
- Never scope vtium work as "what the C reference does". vtium does that and more. Where the C reference departed from the specification, vtium follows the specification and lists each such case as a departure.
- Say each fact once. Don't hard-wrap prose: one paragraph is one line in the file.
- Write the way a person explains something to a colleague out loud: in whole sentences with a subject and a verb, in plain words. The test is whether a reader new to the project understands each sentence on the first read.
- Unpack a compressed phrase into what it means. For example, "A frit of bitmap sources that states no cell takes its fonts' own cell" becomes "A frit made only of bitmap fonts, with no `cell=` line, draws at the fonts' own glyph size."
- Define a project term the first time a section uses it, in the words a newcomer would use, as in "a frit (a font set file)". Prefer the everyday word where one exists.
- Name who does what. Write "vtium ignores the keyboard", not "the keyboard falls to the policy's input default".
- If you had to read a sentence twice, rewrite it before you commit it.
- When you brief a subagent that will write prose for people to read, put these rules in the brief so that it follows them.
- Write the name as "vtium" in running text, code, paths and commands. Write "VTium" in a title or heading that names the project, with VT capitalized the way the hardware terminals printed it. Spell it "color", not "colour".

## The gates

A gate is a check that must pass before a change goes to review. The gate line below lists every gate in order. `make gates` is a Makefile target that runs the same commands, each to the end, and reports each exit code. The author of a change runs the full gate line before review, on the machine where the change was written:

```sh
cargo fmt --all --check \
  && cargo clippy --workspace --all-features --all-targets -- -D warnings \
  && cargo test --workspace --all-features \
  && cargo build --profile minspec \
  && docs/scripts/adr lint --check \
  && docs/scripts/adr cite --check \
  && docs/scripts/coverage \
  && make test-tools \
  && make conformance \
  && make conformance-probes \
  && make conformance-issues \
  && make tier2 \
  && make arm
```

`make conformance` and `make conformance-probes` read the goldens and probes in `tools/conformance/`. Those files were copied from the C reference, and `tools/conformance/provenance.tsv` records where each one came from. `make tier2` reads tier 2's goldens from `tools/tier2/reference/`, which were copied and recorded the same way. `make conformance-issues` uses `gh` to check with GitHub that every issue named in the conformance lists is still open. Without network access it cannot run, so report it as not run, with that reason. No gate needs a checkout of the C repositories.

Three provenance checks sit outside the gates: `make conformance-provenance`, `make tier2-provenance` and `make tier3-provenance`. The reviewer of a change that touches the copied files runs them. Each one checks every copied file against the commit it came from, in the fork checkout it reads with git only, changing nothing. They take `LIBTSM=DIR` and `KMSCON=DIR`, which default to `../libtsm` and `../kmscon`.

vtium's code, tests and tools are written in Rust. Where vtium must talk to a C library, such as libseat (#34), C stays at that boundary as a binding or the thinnest possible shim, and everything around it is Rust.

When you report a gate, give its command and its result, or say why it did not run. Watch each new test fail before the change and pass after it. Watch each new check fail on a defect you planted on purpose, then pass once the defect is gone. Plant the defect on a copy of the file: `cp f f.bak`, plant, test, `mv f.bak f`, `touch f`. Never undo a planted defect with `git checkout` or `git stash`, because they discard work.

## The target set and its coverage

An ADR, or Agent Decision Record, is a document under `docs/architecture/` that records a decision. ADR-1000, in `docs/architecture/term/`, declares which capabilities vtium supports, ignores or refuses. It does so in two tables: `ADR-1000-modes.tsv` and `ADR-1000-features.tsv`. The target set is the list of what vtium aims to support: the capabilities terminals generally support, plus the maintainer's own list.

Each row has an `id` (such as `DEC-2026`, `ANSI-4` or `sgr.dim`), a `status`, a layer, and the issue that carries the work. The `status` is one of supported, ignored (accepted with no effect), refused (deliberately not built), parked (its own issue designs it first), weighed (another issue decides whether vtium builds it), or `no row`. A mode whose status is `no row` is a decision not to support that mode. The layer is the stage of the plan the row lands in, from the tracker, vtium/vtium#47, the issue that lists the plan and its order: minspec, text, images, gpu, seat or top. Rows set the scope of work: the rows an issue lists are its acceptance list, and the issue's Scope section names the crate that implements them. To find a row's id, run `grep -i <name> docs/architecture/term/ADR-1000-*.tsv`. For example, DECAWM is `DEC-7`.

When a test drives a row's behaviour through the crate that implements it and checks the result, put the tag `// covers: <id>` above its test attribute, or `# covers: <id>` in a replay file. A test of a lower layer that the row depends on gets no tag. `docs/scripts/coverage` reads the tags and checks them against the tables:

| Command | What it does |
|---|---|
| `docs/scripts/coverage` | Fails if it finds an unknown, duplicate or malformed id, a tag above a function that is not a test, a tag that is close to the right form but that it cannot read, or a `tests` column that does not match the tags. CI runs it. |
| `docs/scripts/coverage --update` | Rewrites each row's `tests` column from the tags. Run it after you add or move a tag. It is idempotent: running it a second time changes nothing. To resolve a merge conflict in the TSVs' `tests` column, run it again. |
| `docs/scripts/coverage --layer minspec --require` | Fails while any minspec row, other than those marked `no row`, has no test. Minspec is the default, smallest build, and the minspec slice is done when this passes. It joins CI when the vtium binary lands (#21). |

The docstring of `docs/scripts/coverage` explains how feature ids are formed. Once an id is committed it never changes, apart from the one rename that ADR-1000 records (#114). A row's `status` records the decision, and it changes only through an edit to ADR-1000. A row's coverage comes from the tags, and nobody edits it by hand.

## How vtium is judged

vtium is judged in tiers, from unit tests up to the maintainer's own review on a VT, a Linux virtual console.

| Tier | What it compares | Where it runs | Command |
|---|---|---|---|
| Unit | each crate's behaviour | here | `cargo test --workspace --all-features` |
| 1, conformance | the screen state and replies vtium produces after a stream of input bytes, written as text and compared against the C reference's goldens | the replay corpus in `tools/conformance/corpus/`, through the language-neutral interface that its `INTERFACE.md` describes | `make conformance` (`tools/conformance/`, #14), which also runs the interface check |
| Capability probes | each capability vtium claims, tested through a replay, against `tools/conformance/capabilities.list` | `tools/conformance/probes/` | `make conformance-probes` |
| 2, pixels | what the renderer draws in memory, compared pixel for pixel | `tools/tier2/`: the C reference's goldens, copied into `tools/tier2/reference/` with their provenance, and vtium's own reviewed goldens | `make tier2` |
| 3, the VM | screenshots of vtium's install tree running on a VT in a QEMU/KVM guest with a real KMS device, compared against the C reference's goldens, which are copied into `tools/tier3/c-goldens/` with their provenance; it also records size and memory figures | `tools/tier3/` | `make tier3-minimal` (the minspec variants, 1 vCPU and 512 MB), `make tier3` (every variant) |
| VT review | the maintainer looks at vtium on a real VT, guided step by step | this machine | `make vt-check GUIDE=start` ([docs/vt-review.md](docs/vt-review.md)); the vtium binary arrives with #21 |

The goldens decide whether vtium's output is right, and there are three kinds:

- **The C reference's goldens.** Tier 1's goldens and the capability probes live in `tools/conformance/`, tier 2's goldens in `tools/tier2/reference/`, and tier 3's in `tools/tier3/c-goldens/`, all copied from the C reference with each file's provenance.
- **vtium's own goldens.** For behaviour that only vtium has, vtium keeps its own goldens in its tree.
- **Provisional goldens.** vtium's own goldens stay provisional until the maintainer reviews them, and a provisional golden judges nothing.

The author of a change never rewrites, widens or regenerates the golden that judges that change. A golden changes only in a commit of its own, which lists each value that moved, and the maintainer reviews it. Where vtium differs from a golden, the difference must be a listed departure, in the crate's README or in #14's list. A difference that is not listed fails.

Tier 3 needs `qemu-system-x86_64` with KVM, `qemu-img`, `xorriso`, `gpg`, `curl` and `tic`, and it runs as the user who starts it. Its guest images live in a cache that every vtium checkout shares, `$XDG_CACHE_HOME/vtium/tier3/images`. vtium's own scripts download the pristine (unmodified) image into the cache's `pristine/` directory and build the base image into its `base/` directory ([tools/tier3/README.md](tools/tier3/README.md)).

### The VT review

In the VT review, the maintainer checks a build by eye on a real VT. The review runs one build at a time and starts it from one place, so that no other vtium build is left running on a VT:

- The steering session starts the run on a free VT with `make vt-check GUIDE=...` without asking first, and tells the maintainer which VT to switch to. A delegated agent never starts a run.
- The maintainer switches to that VT (Ctrl-Alt-F<n>), follows each step of the guide, and answers y, n or skip, with a note. Sudo asks for its password in the terminal where the run started.
- When the run ends, the console comes back on that VT, and the steering session reads the answers from the log.

A delegated agent proves its branch with the gates and tiers 1 to 3 in its own worktree, and stops there. This machine is the maintainer's live desktop, so an agent on it never takes DRM master, never sets a display mode, never switches VT, never opens a `/dev/tty<N>` console and never runs sudo. An agent may create GL contexts on a render node, use its own ptys and run the tier 3 VM. A render node refuses dumb buffers, the plain memory buffers that the CPU draws into, with `EACCES`, so only tier 3 can prove the dumb-buffer path.

## The working loop

Work happens on this machine first. GitHub's runners are slow, so the steering session waits on them only at merge points, such as the end of a wave. `integration` is a branch in the maintainer's checkout that collects reviewed work before it goes to main.

1. **The record comes first.** When a decision governs a change, the steering session writes or amends the decision's ADR before the build starts, often through a delegated agent. The maintainer reads it before it merges, as with every decision that sets direction. Then a gap-check reads the code against the ADR, statement by statement, and lists where they differ. The implementation work starts from the gaps it finds. For a brand-new feature the ADR still comes first, and the gap-check then finds everything missing.
2. **A worktree per branch,** placed beside the repositories. From the vtium checkout, run `git worktree add -b feat/<topic> ../vtium-<topic> integration`. The forks get no worktrees, because they are read only. Each worktree has its own `target/` directory, so builds running in parallel never wait on each other's lock. No two agents edit the same file, and each crate has one owner at a time.
3. **The branch starts from `integration`,** so later work builds on earlier work before main sees it. `integration` reaches GitHub at each merge point. In a fresh clone that has no `integration` branch, branch from main.
4. **The author commits locally and runs the gates.** The author does not push or open a PR. The author's report gives the branch, the head commit, the gate results and the departures, the way a PR body would.
5. **A fresh reviewer reads the diff** (`git diff integration...<branch>`), runs the gates, and plants the defects that the change claims to catch. The author fixes the findings on the same branch, in up to two fix rounds, and a third round goes to a newly briefed agent.
6. **The reviewed branch merges into `integration`** with `git merge --no-ff`, and the steering session runs the full gates again on the merged tree.
7. **At a merge point,** the steering session pushes `integration`, opens one PR to main, and waits for that PR's CI. A PR merges with a regular merge commit, never a squash or a rebase.

### Testing rules

- Before you delete a test because the golden or other reference it compared against is going away, check which defects only that test would catch, and keep a test that still catches each of them.
- A stand-in (a stub or fake) that a test uses in place of a real part must check what the real part checks. A test's stand-in for the glyph helper once accepted any frit (font set file) name, so the real helper's refusal of a bad name surfaced only in tier 3.

### Cooperating across worktrees

Several agents work at once, each in its own worktree, and the steering session plans the work so that they never collide:

- **One owner per file set.** In each wave, the steering session gives each agent a crate, a tool or a document of its own, and names it in the agent's brief. An agent that needs a change in a file it does not own reports the change to the steering session, which routes it to the owner or schedules it. Agents never edit each other's worktrees.
- **Shared files merge by their tools.** Add a new crate under `crates/`. The workspace's `members = ["crates/*"]` picks it up, so a new crate never needs an edit to the root manifest's member list. Only `docs/scripts/coverage --update` writes ADR-1000's `tests` column, so when two branches tag different rows, rerunning it resolves the conflict. `docs/specimens/` holds sample renderings, and only `make specimens` writes its files, apart from the hand-written `provisional.tsv`. A branch that changes specimens regenerates them in a commit of their own, and a conflict there is resolved by running `make specimens-fonts && make specimens`.
- **Picking up others' work.** When a branch lands on `integration` and a running agent needs it, the steering session sends that agent a message. The message reaches the agent when its current turn ends, and the agent then runs `git merge integration` in its own worktree. For example, every agent needed #67's ids. Agents never rebase a branch that has been shared.
- **Order follows the dependency graph.** A wave includes only items whose dependencies have already landed on `integration`. Work that others are waiting on, such as a contract that several branches cite, merges first.
- **Scratch is per agent.** An agent keeps its scratch files in a directory named for it, never at a shared path that another agent might also write to.
- **Reports, not chatter.** A delegated agent can message the steering session at any time, and it does so when it finishes, when it is blocked, or when it needs a file it does not own. The steering session passes on what one agent needs from another.
- **The brief carries everything.** A message to a delegated agent reaches it only when it ends a turn, so a follow-up sent while the agent is working may arrive after it has already reported. The steering session therefore puts every instruction in the brief.
- **Gates run in the foreground.** A delegated agent runs every gate in the foreground. It never ends its turn to wait on a Monitor (Claude Code's tool for watching a background job) or on a background task of its own, because the notices from those go to the steering session.

Create and change ADRs with `docs/scripts/adr`. It follows contract adr/v1 and numbers ADRs by domain: term 1000, image 1100, font 1200, render 1300, platform 1400, project 1500.

The steering session releases each delegated agent explicitly once its work has merged. A message still queued for an agent can revive it after its context resets, and two revived agents in one worktree collide.

## Where to look

| For | Read |
|---|---|
| The crates, the tiers, determinism, the reply rule, tables and maps | [docs/architecture.md](docs/architecture.md) |
| How the C reference worked, part by part, and the vtium issue that takes each part | [docs/c-reference.md](docs/c-reference.md) |
| What a contribution must show, the code rules, the kitty source rule | [CONTRIBUTING.md](CONTRIBUTING.md) |
| The target set and its coverage | `docs/architecture/term/ADR-1000-*` |
| A crate's departures and bounds | `crates/<crate>/README.md` |
| Where vtium touches systemd, logind or PID 1, and what it does without them | [docs/systemd.md](docs/systemd.md) |
| The plan and its order | the tracker, vtium/vtium#47 |
| The replay interface | `tools/conformance/corpus/INTERFACE.md` |
| Tier 3 | [tools/tier3/README.md](tools/tier3/README.md) |
