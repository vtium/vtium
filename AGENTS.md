# AGENTS.md

How to work on vtium, for coding agents and the people who run them. [CONTRIBUTING.md](CONTRIBUTING.md) sets what a contribution must show, and [docs/architecture.md](docs/architecture.md) holds the crates, the tiers, the determinism rules and the reply rule. This file holds the working loop: where things are, which commands prove a change, and how a change travels from a worktree to main.

## The repositories

| Repository | Path beside this checkout | Role |
|---|---|---|
| vtium/vtium | `.` | vtium itself |
| aaronsb/kmscon | `../kmscon` | the C reference's terminal, the capability probes, tier 2 and tier 3 (the VM) |
| aaronsb/libtsm | `../libtsm` | the C reference's screen and parser, tier 1's replay interface and goldens |

The two forks are the frozen C reference: the floor for behaviour, never the ceiling, and no guide to how vtium is built. vtium is idiomatic Rust. Read the forks; change them only for the test infrastructure vtium is judged by. Their local checkouts are often behind, so fetch and read `origin/main` (`git -C ../kmscon show origin/main:<path>`). Every `gh` command for a fork takes `-R aaronsb/kmscon` or `-R aaronsb/libtsm`, since plain `gh` here targets vtium/vtium.

Upstream `kmscon/kmscon` and `kmscon/libtsm` are never written to: no issue, PR or comment, and no maintainer tagged. Upstream appears in vtium's text only in code spans, with no links. A defect found in the C reference, a security defect included, is fixed in vtium and listed as a departure; the fork is fixed only when the maintainer asks, and nothing is reported upstream.

## Writing

- vtium is the present tense in every authored surface: docs, comments, commits, PR bodies and issues. The C code is "the C reference", in the past tense ("the C reference replied 4"). History is fine in the past tense.
- A spec describes behaviour vtium implements. C function names belong in a marked C-notes section, not in a behaviour spec.
- Never scope vtium work as "what the C reference does". It does that and more, and follows the specification where the C reference departed from it, listing each departure.
- Say each fact once. Prose is one line per paragraph.

## The gates

Every change runs the full gate line before review, on the machine that wrote it:

```sh
cargo fmt --all --check \
  && cargo clippy --workspace --all-features --all-targets -- -D warnings \
  && cargo test --workspace --all-features \
  && cargo build --profile minspec \
  && docs/scripts/adr lint --check \
  && docs/scripts/adr cite --check \
  && docs/scripts/coverage
```

vtium's code, tests and tools are written in Rust. Where vtium must talk to a C library, such as libseat (#34) or the C reference's parser that the parse oracle (`tools/parse-oracle/`, #52) compares against, C stays at that boundary: a binding or the thinnest shim, with everything around it in Rust. A tool that needs the C reference stays out of the default build, so `cargo test --workspace` needs no C toolchain or fork checkout. The C repositories keep their own harness, which judges vtium through language-neutral interfaces.

A gate reports executed, with its command and result, or names why it did not run. A test is seen failing before the change and passing after it. A new check is seen red on a planted defect, then green. Plant on a copy (`cp f f.bak`, plant, test, `mv f.bak f`, `touch f`), never with `git checkout` or `git stash`, which discard work.

## The target set and its coverage

ADR-1000 (`docs/architecture/term/`) declares what vtium supports, ignores or refuses, in two tables: `ADR-1000-modes.tsv` and `ADR-1000-features.tsv`. The target set is the capabilities terminals generally support plus the maintainer's own list; a mode with `no row` is a decision not to support it. Each row has an `id` (`DEC-2026`, `ANSI-4`, `sgr.dim`), a `status`, the layer it lands in and the issue that carries it. Work scopes itself from rows: an issue's rows are its acceptance list, and the issue's Scope names the crate that implements them. Find a row's id with `grep -i <name> docs/architecture/term/ADR-1000-*.tsv` (DECAWM is `DEC-7`).

A test that drives a row's behaviour through the crate that implements it, and checks the result, carries `// covers: <id>` above its test attribute (`# covers: <id>` in a replay file). A test of a lower layer the row depends on carries no tag. `docs/scripts/coverage` reads the tags and checks them against the tables:

| Command | What it does |
|---|---|
| `docs/scripts/coverage` | Fails on an unknown, duplicate or malformed id, a tag above a non-test function, a near-miss tag it cannot read, or a `tests` column that differs from the tags. Runs in CI. |
| `docs/scripts/coverage --update` | Rewrites each row's `tests` column from the tags. Run it after adding or moving a tag; it is idempotent. A merge conflict in the TSVs' `tests` column is resolved by running it again. |
| `docs/scripts/coverage --layer minspec --require` | Fails while any minspec row other than `no row` has no test. It is the minspec slice's done condition, and joins CI when the binary lands (#21). |

The lint's docstring states how feature ids are formed. An id never changes once committed. A row's `status` records the decision and changes only through an edit to ADR-1000; its coverage is derived from the tags and is never edited by hand.

## How vtium is judged

| Tier | What it compares | Where it runs | Command |
|---|---|---|---|
| Unit | each crate's behaviour | here | `cargo test --workspace --all-features` |
| 1, conformance | screen state and replies after a byte stream, as text, against the C reference's goldens | the replay corpus in `../libtsm/test/conformance/`, through the language-neutral interface in its `INTERFACE.md` | vtium's runner (#14); until it lands, `make test` in `../libtsm` runs the C reference against its own goldens |
| Capability probes | each claimed capability, through a replay | `../kmscon/tests/capabilities/` (`probes.replay`, `run.py`) | `make test` in `../kmscon` |
| 2, pixels | what the renderer draws in memory, pixel for pixel | ported from `../kmscon/tests/` (#17) | with vtium-render |
| 3, the VM | screenshots of the installed build on a VT in a QEMU/KVM guest with a real KMS device, against goldens, with size and memory figures | `../kmscon/fork/tier3/` | `make tier3-minimal` in `../kmscon` (the minspec variants, 1 vCPU and 512 MB); #22 adds the option that boots a vtium build |
| VT review | the maintainer's eyes on a real VT, guided step by step | this machine | `make vt-check GUIDE=start` ([docs/vt-review.md](docs/vt-review.md)); the binary arrives with #21 |

Goldens live in the C repositories and are the judge. The author of a change never rewrites, widens or regenerates the golden that judges it; a golden changes only in its own commit, with each moved value listed. vtium's differences from a golden are its listed departures (each crate's README and #14's list); an unlisted difference fails.

Tier 3 needs docker, `qemu-system-x86_64` with KVM, `qemu-img`, `xorriso`, `gpg` and `curl`, and runs as the invoking user. A new kmscon worktree links `fork/vm/images` from the main checkout, so every checkout shares the provisioned guest image ([`../kmscon/fork/tier3/README.md`](https://github.com/aaronsb/kmscon/blob/main/fork/tier3/README.md)).

### The VT review

The VT review runs one build at a time, from one place, so no stray builds float around:

- The session that steers the work, never a delegated agent, starts the run on a free VT (`make vt-check GUIDE=...`) and names the VT to switch to. It does not ask first.
- The maintainer switches to that VT (Ctrl-Alt-F<n>), follows each step of the guide and answers y, n or skip, with a note. Sudo asks for its password in the terminal the run started from.
- When the run ends, the console comes back on that VT and the steering session reads the answers from the log.

A delegated agent proves its branch through the gates and tiers 1 to 3 in its worktree, and stops there. On this machine, which is the maintainer's live desktop, an agent takes no DRM master, sets no mode, switches no VT, opens no `/dev/tty<N>` console and runs no sudo. Creating GL contexts on a render node and allocating and mapping dumb buffers without a modeset are fine, as are its own ptys and the tier 3 VM.

## The working loop

The work runs local-first: GitHub's runners are slow, so they are waited on only at merge points.

1. **A worktree per branch,** beside the repositories. From the vtium checkout: `git worktree add -b feat/<topic> ../vtium-<topic> integration`, and `../kmscon-<topic>`, `../libtsm-<topic>` for the forks. Each worktree has its own `target/`, so parallel builds never contend for a lock. No two agents edit the same file; a crate has one owner at a time.
2. **The branch starts from `integration`,** a branch in the maintainer's checkout that collects reviewed work, so later work builds on earlier work before main sees it. It reaches GitHub at each merge point; a fresh clone with no `integration` branches from main.
3. **The author commits locally and runs the gates.** It does not push or open a PR. Its report gives the branch, the head commit, the gate results and the departures, as a PR body would.
4. **A fresh reviewer reads the diff** (`git diff integration...<branch>`), runs the gates, and plants the defects the change claims to catch. Its findings are fixed in a round on the same branch. A fix round after the second goes to a freshly briefed agent.
5. **The reviewed branch merges into `integration`** with `git merge --no-ff`, and the full gates run again on the merged tree.
6. **At a merge point,** such as the end of a wave or a check of vtium against the forks, `integration` is pushed and one PR goes to main, as do the forks' PRs, and their CI is waited on. A PR merges by a regular merge commit, never squash or rebase.

### Cooperating across worktrees

Several agents work at once, each in its own worktree, and the steering session plans the work so they never collide:

- **One owner per file set.** A wave gives each agent a crate, a tool or a document of its own, named in its brief. An agent that needs a change in a file it does not own reports the change to the steering session, which routes it to the owner or schedules it. Agents never edit each other's worktrees.
- **Shared files merge by their tools.** A crate is added under `crates/`, which the workspace's `members = ["crates/*"]` picks up, so new crates never touch the root manifest's member list. ADR-1000's `tests` column is written only by `docs/scripts/coverage --update`, so two branches that tag different rows resolve by rerunning it.
- **Picking up others' work.** When a branch lands on `integration` and a running agent needs it, as every agent needed #67's ids, the steering session tells it, and the agent runs `git merge integration` in its own worktree. Agents never rebase a branch that has been shared.
- **Order follows the dependency graph.** A wave runs only items whose dependencies have landed on `integration`. Work that unblocks others, such as a contract several branches cite, merges first.
- **Scratch is per agent.** An agent keeps scratch files in a directory named for it, never at a shared path another agent might also write.
- **Reports, not chatter.** An agent messages the steering session when it finishes, when it is blocked, or when it needs a file it does not own. The steering session relays what one agent needs from another.

A decision that sets direction, such as a new ADR, is read by the maintainer before it merges. ADRs are created and changed with `docs/scripts/adr` (contract adr/v1; domains term 1000, image 1100, font 1200, render 1300, platform 1400, project 1500).

A delegated agent is released explicitly when its work has merged. Messages still queued for an agent can revive it after its context resets, and two revived agents in one worktree collide.

## Where to look

| For | Read |
|---|---|
| The crates, the tiers, determinism, the reply rule, tables and maps | [docs/architecture.md](docs/architecture.md) |
| What a contribution must show, the code rules, the kitty source rule | [CONTRIBUTING.md](CONTRIBUTING.md) |
| The target set and its coverage | `docs/architecture/term/ADR-1000-*` |
| A crate's departures and bounds | `crates/<crate>/README.md` |
| The plan and its order | the tracker, vtium/vtium#47 |
| The replay interface | `../libtsm/test/conformance/INTERFACE.md` |
| Tier 3 | `../kmscon/fork/tier3/README.md` |
