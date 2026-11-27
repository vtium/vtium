# AGENTS.md

This file tells coding agents, and the people who run them, how to work on vtium. [CONTRIBUTING.md](CONTRIBUTING.md) sets what a contribution must show, and [docs/architecture.md](docs/architecture.md) describes the crates, the tiers, the determinism rules and the reply rule. This file covers the working loop: where things are, which commands prove that a change works, and how a change travels from a worktree to `integration`.

## The directory files

The rules for work in one directory live in that directory's AGENTS.md, and its CLAUDE.md loads the file with `@AGENTS.md`. Claude Code reads a directory's CLAUDE.md only when an agent works in that directory, so each agent carries the rules of the directories it works in. The steering session reads a directory's file before it briefs an agent for work there, so that the brief carries the file's rules.

| File | What it holds |
|---|---|
| [docs/AGENTS.md](docs/AGENTS.md) | The Writing rules and "What not to write", which apply to everything written for vtium, and the work under docs/: pages for users and contributors, generated pages, figures and their claim files |
| [crates/AGENTS.md](crates/AGENTS.md) | Code comments and their References block, the comments of the shipped configuration files, strings on screen, ADR-1000's target set and its coverage tags, and the testing rules |
| [tools/AGENTS.md](tools/AGENTS.md) | How vtium is judged: the tiers, the goldens and who accepts them, tier 3's needs, the VT review, and fuzz targets |
| [docs/contributing/steering.md](docs/contributing/steering.md) | What only the steering session does: steps 7 to 10 of the working loop, from `stage` to a release and its signed tag, changes to pages alone, the scope ledger, and running the VT review |

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

vtium's behaviour is defined by the specifications it cites, ADR-1000's tables and vtium's own goldens. A golden is a saved expected result that vtium's output is compared against. [ADR-1503](docs/architecture/project/ADR-1503-vtium-judges-itself-by-its-own-goldens-and-its-committed-tree-names-no-c-fork.md) records this decision. vtium is tested in tiers: unit tests, tiers 1 to 3, and a final review by the maintainer on a real console. [How vtium is judged](tools/AGENTS.md#how-vtium-is-judged) lists them.

Other terminals are external references. ADR-1000's tables compare vtium with the Linux console, xterm, foot, Ghostty, WezTerm and kitty, as each one's published documentation describes it.

Work on vtium happens only in vtium's own repositories, under the vtium organization on GitHub. Issues, pull requests and comments about vtium go there.

## The gates

A gate is a check that must pass before a change goes to review. The gate line below lists every gate in order. `make gates` is a Makefile target that runs the same commands, each to the end, and reports each exit code in the gate line's order. It runs up to 8 gates at once: `make gates JOBS=N` sets the number, and `JOBS=1` runs them one after another. Each gate builds in a directory of its own, which is both its cargo target directory and its build directory. The directories go under `VTIUM_GATES_DIR`, which is `/data/1/cargo/vtium/<checkout directory name>/gates` on a machine with that build volume, and `target/gates/` elsewhere or when you set `CARGO_TARGET_DIR` yourself. They stay between runs, and after each run `make gates` removes those that no current gate owns. Each gate writes its output to a log of its own under `target/gates/logs/`, which the report names when the gate fails. Never share one `CARGO_TARGET_DIR` or `VTIUM_GATES_DIR` between worktrees whose sources differ: cargo can give both worktrees' builds the same artifact names, and a test run in one worktree then runs a test binary built from the other's sources. After the gates, `make gates` builds the minspec binary the way tier 3 builds it and prints the binary's size against tier 3's budget. A binary over the budget gets a warning that `make tier3-minimal` fails on it, and the warning does not change the gates' result. The line below is the full set of gates. The steering session runs the whole gate line at the two points that [When the gate line runs](#when-the-gate-line-runs) names, and the author of a change runs only the checks of what the change touched (see [The working loop](#the-working-loop), step 4):

```sh
cargo fmt --all --check \
  && cargo clippy --workspace --all-features --all-targets -- -D warnings \
  && make test-all-features \
  && make test-default \
  && cargo build --profile minspec \
  && make msrv \
  && make deny \
  && docs/scripts/adr lint --check \
  && docs/scripts/adr cite --check \
  && docs/scripts/issue-refs \
  && docs/scripts/fork-refs \
  && docs/scripts/claims \
  && docs/scripts/prose --check \
  && docs/scripts/coverage \
  && make api \
  && make features-check \
  && make man-check \
  && make test-tools \
  && make conformance \
  && make conformance-probes \
  && make conformance-issues \
  && make tier2 \
  && make arm \
  && make arm-memory
```

`make conformance` and `make conformance-probes` read the goldens and probes in `tools/conformance/`. `make tier2` reads tier 2's goldens from `tools/tier2/goldens/`. `make arm-memory` runs vtium-term's memory tests for armv7 under qemu-user, so a change that moves a 32-bit memory figure fails there. `make api` builds the reference that rustdoc generates for each crate under `crates/`, and fails on any rustdoc warning, a broken intra-doc link included. `make features-check` runs clippy and rustdoc on each crate with cargo features, once with no feature and once with each feature alone. `make conformance-issues` uses `gh` to check the conformance lists and ADR-1000's issue column against GitHub's issue states. An open issue that only supported rows name is not a failure: the check lists it as one for the next stage to close. Without network access `make conformance-issues` cannot run, so report the gate as not run, with that reason. `make test-all-features` and `make test-default` run the workspace's tests with cargo-nextest, which runs the tests of every test binary in parallel across the cores. nextest does not run doctests, so both targets then run `cargo test --doc`. They need cargo-nextest at the version that the Makefile's `NEXTEST_VERSION` names, and they tell you the `cargo install` command when that version is missing. `make deny` checks the license and the source of every crate that a lock file of vtium's names, against `deny.toml`. It needs cargo-deny at the version that the Makefile's `DENY_VERSION` names, and it tells you the `cargo install` command when that version is missing. The check of the workspace's crates against the RustSec advisory database needs the network, so it is not a gate: CI runs it as the job `advisories`, and `make advisories` runs it here. `make ci-local` runs the workspace's tests in a container that matches CI's Ubuntu 24.04 runner, so a test that fails only on CI shows up before the push. `docs/scripts/claims` checks the claim files beside the docs' figures and pages: each claim's file must exist, and the symbol the claim names must still be defined there. [docs/AGENTS.md](docs/AGENTS.md) says what to do when `docs/scripts/claims` fails.

vtium's code, tests and tools are written in Rust. Where vtium must talk to a C library, such as libseat (#34), C stays at that boundary as a binding or the thinnest possible shim, and everything around the C code is Rust.

When you report a gate, give its command and its result, or say why the gate did not run. Watch each new test fail before the change and pass after the change. Watch each new check fail on a defect you planted on purpose, then pass once the defect is gone. Plant the defect on a copy of the file: `cp f f.bak`, plant, test, `mv f.bak f`, `touch f`. Never undo a planted defect with `git checkout`, because `git checkout` discards uncommitted work. A delegated agent may use `git stash` in its own worktree, for example to set its changes aside for a moment and measure the code before them, as long as the agent pops the stash straight after and leaves the stash list as it was. Nobody uses `git stash` in the maintainer's checkout or in the `integration` worktree.

### When the gate line runs

The full gate line takes a long time, so it runs at two points of each batch and nowhere else. It runs once on each wave's candidate, before the candidate merges into `integration`, and once on the `stage` commit, before the stage PR. A push of a tree that already passed the full line skips it: `git diff --quiet <commit that passed> <commit to push>` shows that the trees are the same. A small change on `integration` between those points, such as a doc fix, a test fix or a merge that resolves a conflict, runs only the gates that its files reach. Docs under `docs/` reach `docs/scripts/prose --check`, `docs/scripts/claims`, `docs/scripts/fork-refs`, `docs/scripts/issue-refs` and the two `docs/scripts/adr` checks. A crate reaches `cargo fmt --all --check`, clippy, its own tests and the tests of the crates that depend on it, and `make api`. A tool under `tools/` reaches `make test-tools` and that tool's own gate, such as `make tier2` for `tools/tier2`. `make gates-for BASE=<commit>` lists and runs those gates for the difference from BASE; until that target exists, the steering session picks them from this paragraph. A gate that the narrow run leaves out still runs at the next of the two points. The cards' own checks at V0 and V1 do not change.

## The verification levels

Verification runs at five levels, which ADR-1505 decides. Each level has a stop rule, which says what the level never runs, so that no level grows into the one above it.

| Level | Who and when | What runs | Stop rule |
|---|---|---|---|
| V0, while working | the author, on each change | `cargo fmt --all --check`, clippy and the tests of the crates touched; each new test seen failing first; planted defects | never the full gate line, tier 2 in full, specimens or tier 3 |
| V1, end of card | the author, once | only what proves the capability the card adds: the card's new tests and scenarios, each seen failing first; all of tier 1 (`make conformance`) when the card changes the terminal; `make tier2` once when the card changes drawing, fonts or screens; a fuzz target in reach of the change for 60 s; one golden and specimen redraw if pictures move, only when no other branch still changes the look | never the full gate line; no tier 3, unless the card touches the install tree, VT or session handling, DRM/KMS, the GPU context or systemd; then `make tier3-minimal` once |
| V2, the wave's candidate | the steering session, once on the candidate that merges the wave's cards | the full gate line once on the candidate; a fresh reviewer for each failing tier 1 comparison and each new tier 1 case's citation (ADR-1503 D10); a scoped review of each card as it merges: its claimed defects planted again, its diff checked against the files its brief gave it, and a fresh reviewer's read of its riskiest path | never the full gate line for a single card; no full code review, which waits for the maintainer's verdict at V3; one fix round for the tier 1 cases and the scoped reviews |
| V3, the maintainer's verdict, then the code review | the maintainer, once per batch; then one full code review of the wave by fresh reviewers | the maintainer judges each new capability on a VT tour page (`make vt-accept`, on the GPU path and on the CPU path) or on a picture page (the picture sheet of every moved and new pixel golden and of the tier 1 cases that ADR-1503 D10 sends to the maintainer); at a release candidate, the review of tier 1 against drift; each accepted capability keeps scenario tests that go on checking it; after the verdict, one full code review reads the wave's code on `integration` as one change: its code paths, the places where its cards meet, its implementation, safety and security, and performance | the verdicts are the fix list, and a fix comes back to the maintainer only if it moves a picture; one review round for the wave, split across reviewers by crate when the wave is large, which the steering session checks by planting the defects again; a second round goes to the maintainer's eyes |
| V4, release candidate | the steering session, once | `make tier3` with `VARIANTS` naming every variant, each fuzz target's default run, every CI job including `arm-native`, `make specimens-check` | a defect found here is fixed on the candidate and rechecked at its own level, and the whole ladder does not run again |

Two rules go with the table. The steering session sizes each card to finish in well under half of an agent's context, so that verification does not cost more than the build. Every brief names its level, the agent runs exactly that level, and a slice plan in an ADR may not add checks beyond the table: an ADR's slice list names what a slice proves, and never which tiers it runs.

A delegated agent runs the level its brief names in its own worktree, and stops there. This machine is the maintainer's live desktop, so an agent on this machine never takes DRM master, never sets a display mode, never switches VT, never opens a `/dev/tty<N>` console and never runs sudo. An agent may create GL contexts on a render node, use its own ptys and run the tier 3 VM. A render node refuses dumb buffers, the plain memory buffers that the CPU draws into, with `EACCES`, so only tier 3 can prove the dumb-buffer path.

## The working loop

Work happens on this machine first. GitHub's runners are slow, so the steering session waits on them only at merge points, such as the end of a wave. `integration` is a branch in the maintainer's checkout that collects each wave's work once the wave passes the full gate line. `stage` is a branch on GitHub that holds one batch of `integration`, frozen at a commit that passed the gates, while that batch goes to main.

The loop runs at the five verification levels that [The verification levels](#the-verification-levels) defines. The author works at V0 and V1 (step 4). The steering session runs V2 on the wave's candidate and on `integration` (steps 5 and 6). V3 is the maintainer's verdict and the code review that follows it (steps 8 and 9), and the release candidate gets V4 (step 10). Steps 7 to 10 belong to the steering session, and [the steering page](docs/contributing/steering.md) holds them.

1. **The record comes first.** When a decision governs a change, the steering session writes or amends the decision's ADR before the build starts, often through a delegated agent. The maintainer reads the ADR before the ADR merges, as with every decision that sets direction. Then a gap-check reads the code against the ADR, statement by statement, and lists where they differ. The implementation work starts from the gaps the gap-check finds. For a brand-new feature the ADR still comes first, and the gap-check then finds everything missing.
2. **A worktree per branch,** placed beside the vtium checkout. From the vtium checkout, run `git worktree add -b feat/<topic> ../vtium-<topic> integration`. Each worktree has its own `target/` directory, so builds running in parallel never wait on each other's lock. No two agents edit the same file, and each crate has one owner at a time.
3. **The branch starts from `integration`,** so later work builds on earlier work before main sees that work. `integration` reaches GitHub at each merge point. In a fresh clone that has no `integration` branch, branch from main.
4. **The author commits locally and runs the level its brief names,** V0 while the author works and V1 once at the end of the card. The card proves only the capability it adds. The author does not run the full gate line; the steering session runs the full gate line once, on the wave's candidate. The author does not push or open a PR. The author's report gives the branch, the head commit, the gate results, and each place where the change chose between two readings of a specification, the way a PR body would.
5. **The steering session assembles the wave's cards into one candidate** (V2). The candidate is a branch cut from `integration`. Each card's branch merges into the candidate with `git merge --no-ff`. As each card merges, the steering session gives it a scoped review: it plants again the defects the card claims its tests catch, checks that the card's diff stays inside the files its brief gave it, and has a fresh reviewer read the card's riskiest path. A finding goes back to the card's author before the gate line runs. The steering session runs the full gate line once on the candidate, and not on each card. A fresh reviewer settles the tier 1 cases that ADR-1503 D10 sends to V2.
6. **The candidate merges into `integration`** with `git merge --no-ff`. When the merged tree is the candidate's tree, which already passed, the steering session pushes without running the gates again. Otherwise it runs the gates that the difference reaches before it pushes.

### Cooperating across worktrees

Several agents work at once, each in its own worktree, and the steering session plans the work so that they never collide:

- **One owner per file set.** In each wave, the steering session gives each agent a crate, a tool or a document of its own, and names that crate, tool or document in the agent's brief. An agent that needs a change in a file it does not own reports the change to the steering session, which routes the change to the owner or schedules the change. Agents never edit each other's worktrees.
- **Shared files merge by their tools.** Add a new crate under `crates/`. The workspace's `members = ["crates/*"]` picks the new crate up, so a new crate never needs an edit to the root manifest's member list. Only `docs/scripts/coverage --update` writes ADR-1000's `tests` column and `tools/conformance/COVERAGE.md`, so when two branches tag different rows, rerunning `docs/scripts/coverage --update` resolves the conflict. `docs/specimens/` holds sample renderings, and only `make specimens` writes its files, apart from the hand-written `provisional.tsv`. A branch that changes specimens regenerates them in a commit of their own, and a conflict there is resolved by running `make specimens-fonts && make specimens`.
- **Picking up others' work.** When a branch lands on `integration` and a running agent needs that branch, the steering session sends that agent a message. The message reaches the agent when its current turn ends, and the agent then runs `git merge integration` in its own worktree. For example, every agent needed #67's ids. Agents never rebase a branch that has been shared.
- **Order follows the dependency graph.** A wave includes only items whose dependencies have already landed on `integration`. Work that others are waiting on, such as a contract that several branches cite, merges first.
- **Scratch is per agent.** An agent keeps its scratch files in a directory named for the agent, never at a shared path that another agent might also write to.
- **Reports, not chatter.** A delegated agent messages the steering session only when the agent finishes, when the agent is blocked, or when the agent needs a file it does not own. The steering session passes on what one agent needs from another.
- **The brief carries everything.** A message to a delegated agent reaches the agent only when the agent ends a turn, so a follow-up sent while the agent is working may arrive after the agent has already reported. The steering session therefore puts every instruction in the brief.
- **Gates run in the foreground.** A delegated agent runs every gate in the foreground. The agent never ends its turn to wait on a Monitor (Claude Code's tool for watching a background job) or on a background task of its own, because the notices from those go to the steering session.

Create and change ADRs with `docs/scripts/adr`. It follows contract adr/v1 and numbers ADRs by domain: term 1000, image 1100, font 1200, render 1300, platform 1400, project 1500.

The steering session releases each delegated agent explicitly once the agent's report is in and checked, not when its work merges. Later work on the same branch, such as a fix round, a review finding or a merge conflict, goes to a freshly briefed agent. Its brief names the findings, the branch and the worktree. Waking an idle agent sends its whole context again, so a fresh agent with a tight brief costs less than reviving the author. Releasing early also keeps a queued message from reviving an agent after its context resets, and two revived agents in one worktree collide.

### Signed commits

Every commit to vtium is signed with the maintainer's SSH key, from 2026-11-10 on. The signature shows that the commit was made on the maintainer's machine, under the maintainer's control. It is part of the project's provenance, beside the decision records that quote the maintainer's words. The maintainer's global git config signs each commit and tag by itself, so an agent commits as usual and does nothing extra.

- When a commit fails because the signing key is not loaded, the agent stops and reports the failure. The maintainer loads the key with `ssh-add`.
- An agent never turns signing off with `-c commit.gpgsign=false` or `--no-gpg-sign`, and never skips the hooks with `--no-verify`.
- The pre-push hook in `tools/git-hooks/` refuses a push that carries an unsigned commit made after signing began. `git config core.hooksPath tools/git-hooks` turns the hook on. A commit made before that date passes unchecked, and its history stays as it was.

## Where to look

| For | Read |
|---|---|
| The crates, the tiers, determinism, the reply rule, tables and maps | [docs/architecture.md](docs/architecture.md) |
| What a contribution must show, the code rules, the kitty source rule | [CONTRIBUTING.md](CONTRIBUTING.md) |
| The target set and its coverage | `docs/architecture/term/ADR-1000-*` |
| A crate's bounds and the choices it makes | `crates/<crate>/README.md` |
| Where vtium touches systemd, logind or PID 1, and what it does without them | [docs/systemd.md](docs/systemd.md) |
| The plan and its order | the tracker, vtium/vtium#47 |
| The replay interface | `tools/conformance/corpus/INTERFACE.md` |
| Tier 3 | [tools/tier3/README.md](tools/tier3/README.md) |
