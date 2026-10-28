# AGENTS.md

How to work on vtium, for coding agents and the people who run them. [CONTRIBUTING.md](CONTRIBUTING.md) sets what a contribution must show, and [docs/architecture.md](docs/architecture.md) holds the crates, the tiers, the determinism rules and the reply rule. This file holds the working loop: where things are, which commands prove a change, and how a change travels from a worktree to main.

## The repositories

| Repository | Path beside this checkout | Role |
|---|---|---|
| vtium/vtium | `.` | vtium itself |
| aaronsb/kmscon | `../kmscon` | the C reference's terminal, the origin of the capability probes vtium copied (#111), and the origin of the tier 2 and tier 3 goldens vtium copied (#111) |
| aaronsb/libtsm | `../libtsm` | the C reference's screen and parser, the origin of tier 1's replay interface and goldens, copied into `tools/conformance/` with their provenance, and the origin of the tools that made and checked them: the case generator, the recorder and the interface check, ported into `tools/conformance/` in Rust |

The two forks are the frozen C reference: the floor for behaviour, never the ceiling, and no guide to how vtium is built. vtium is idiomatic Rust. Read the forks and change nothing in them. No gate, CI job or build, and no tool but the three provenance checks under [The gates](#the-gates), runs their code or reads their checkouts: what vtium needs from them is copied into its own tree with its provenance (#111), and the provenance checks read a checkout only for review. Their local checkouts are often behind, so fetch and read `origin/main` (`git -C ../kmscon show origin/main:<path>`). Every `gh` command for a fork takes `-R aaronsb/kmscon` or `-R aaronsb/libtsm`, since plain `gh` here targets vtium/vtium.

A question about behaviour or structure is answered from the C reference first, through [docs/c-reference.md](docs/c-reference.md) and then the forks' code at `origin/main`, taking its logic flow and never its code structure, before anything new is designed.

A problem found in vtium's implementation is diagnosed the same way: first how the C reference's flow handled the same case and what it decided, cited, as a logical reference and never as C code to port. This holds for every change, a bug fix, a review finding or a failing test alike.

Upstream `kmscon/kmscon` and `kmscon/libtsm` are never written to: no issue, PR or comment, and no maintainer tagged. Upstream appears in vtium's text only in code spans, with no links. A defect found in the C reference, a security defect included, is fixed in vtium and listed as a departure; the fork is fixed only when the maintainer asks, and nothing is reported upstream.

## Writing

- vtium is the present tense in every authored surface: docs, comments, commits, PR bodies and issues. The C code is "the C reference", in the past tense ("the C reference replied 4"). History is fine in the past tense.
- A spec describes behaviour vtium implements. C function names belong in a marked C-notes section, not in a behaviour spec.
- Never scope vtium work as "what the C reference does". It does that and more, and follows the specification where the C reference departed from it, listing each departure.
- Say each fact once. Prose is one line per paragraph.
- The name is "vtium" in running text, code, paths and commands, and "VTium" in a title or heading that names the project, with VT capitalized as the hardware terminals printed it. Spell it "color", not "colour".

## The gates

Every change runs the full gate line before review, on the machine that wrote it:

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

`make conformance` and `make conformance-probes` read the goldens and probes in `tools/conformance/`, copied from the C reference with the provenance of each file in `tools/conformance/provenance.tsv`, and `make tier2` reads tier 2's in `tools/tier2/reference/` the same way. `make conformance-issues` asks GitHub through `gh` that every issue the conformance lists name is open. No gate needs a checkout of the C repositories. Three review steps, outside the gates, read one with git, read-only, when a change touches the copies: `make conformance-provenance`, `make tier2-provenance` and `make tier3-provenance` check each copied file against its source commit (`LIBTSM=DIR` and `KMSCON=DIR`, default `../libtsm` and `../kmscon`).

vtium's code, tests and tools are written in Rust. Where vtium must talk to a C library, such as libseat (#34), C stays at that boundary: a binding or the thinnest shim, with everything around it in Rust.

A gate reports executed, with its command and result, or names why it did not run. A test is seen failing before the change and passing after it. A new check is seen red on a planted defect, then green. Plant on a copy (`cp f f.bak`, plant, test, `mv f.bak f`, `touch f`), never with `git checkout` or `git stash`, which discard work.

## The target set and its coverage

ADR-1000 (`docs/architecture/term/`) declares what vtium supports, ignores or refuses, in two tables: `ADR-1000-modes.tsv` and `ADR-1000-features.tsv`. The target set is the capabilities terminals generally support plus the maintainer's own list; a mode with `no row` is a decision not to support it. Each row has an `id` (`DEC-2026`, `ANSI-4`, `sgr.dim`), a `status`, the layer it lands in and the issue that carries it. Work scopes itself from rows: an issue's rows are its acceptance list, and the issue's Scope names the crate that implements them. Find a row's id with `grep -i <name> docs/architecture/term/ADR-1000-*.tsv` (DECAWM is `DEC-7`).

A test that drives a row's behaviour through the crate that implements it, and checks the result, carries `// covers: <id>` above its test attribute (`# covers: <id>` in a replay file). A test of a lower layer the row depends on carries no tag. `docs/scripts/coverage` reads the tags and checks them against the tables:

| Command | What it does |
|---|---|
| `docs/scripts/coverage` | Fails on an unknown, duplicate or malformed id, a tag above a non-test function, a near-miss tag it cannot read, or a `tests` column that differs from the tags. Runs in CI. |
| `docs/scripts/coverage --update` | Rewrites each row's `tests` column from the tags. Run it after adding or moving a tag; it is idempotent. A merge conflict in the TSVs' `tests` column is resolved by running it again. |
| `docs/scripts/coverage --layer minspec --require` | Fails while any minspec row other than `no row` has no test. It is the minspec slice's done condition, and joins CI when the binary lands (#21). |

The lint's docstring states how feature ids are formed. An id never changes once committed, apart from the one rename ADR-1000 records (#114). A row's `status` records the decision and changes only through an edit to ADR-1000; its coverage is derived from the tags and is never edited by hand.

## How vtium is judged

| Tier | What it compares | Where it runs | Command |
|---|---|---|---|
| Unit | each crate's behaviour | here | `cargo test --workspace --all-features` |
| 1, conformance | screen state and replies after a byte stream, as text, against the C reference's goldens | the replay corpus in `tools/conformance/corpus/`, through the language-neutral interface in its `INTERFACE.md` | `make conformance` (`tools/conformance/`, #14), which also runs the interface check |
| Capability probes | each claimed capability, through a replay, against `tools/conformance/capabilities.list` | `tools/conformance/probes/` | `make conformance-probes` |
| 2, pixels | what the renderer draws in memory, pixel for pixel | `tools/tier2/`: the C reference's goldens, copied into `tools/tier2/reference/` with their provenance, and vtium's own reviewed goldens | `make tier2` |
| 3, the VM | screenshots of vtium's install tree on a VT in a QEMU/KVM guest with a real KMS device, against the C reference's goldens, copied into `tools/tier3/c-goldens/` with their provenance, with size and memory figures | `tools/tier3/` | `make tier3-minimal` (the minspec variants, 1 vCPU and 512 MB), `make tier3` (every variant) |
| VT review | the maintainer's eyes on a real VT, guided step by step | this machine | `make vt-check GUIDE=start` ([docs/vt-review.md](docs/vt-review.md)); the binary arrives with #21 |

Goldens are the judge. Tier 1's goldens and the capability probes live in `tools/conformance/`, tier 2's in `tools/tier2/reference/` and tier 3's in `tools/tier3/c-goldens/`, copied from the C reference with each file's provenance; goldens for behaviour only vtium has are vtium's own, in its tree, provisional until the maintainer reviews them. The author of a change never rewrites, widens or regenerates the golden that judges it; a golden changes only in its own commit, with each moved value listed. vtium's differences from a golden are its listed departures (each crate's README and #14's list); an unlisted difference fails.

Tier 3 needs `qemu-system-x86_64` with KVM, `qemu-img`, `xorriso`, `gpg`, `curl` and `tic`, and runs as the invoking user. Its guest images live in a cache every vtium checkout shares, `$XDG_CACHE_HOME/vtium/tier3/images`: vtium's own scripts fetch the pristine image into its `pristine/` and build the base image into its `base/` ([tools/tier3/README.md](tools/tier3/README.md)).

### The VT review

The VT review runs one build at a time, from one place, so no stray builds float around:

- The session that steers the work, never a delegated agent, starts the run on a free VT (`make vt-check GUIDE=...`) and names the VT to switch to. It does not ask first.
- The maintainer switches to that VT (Ctrl-Alt-F<n>), follows each step of the guide and answers y, n or skip, with a note. Sudo asks for its password in the terminal the run started from.
- When the run ends, the console comes back on that VT and the steering session reads the answers from the log.

A delegated agent proves its branch through the gates and tiers 1 to 3 in its worktree, and stops there. On this machine, which is the maintainer's live desktop, an agent takes no DRM master, sets no mode, switches no VT, opens no `/dev/tty<N>` console and runs no sudo. Creating GL contexts on a render node is fine, as are its own ptys and the tier 3 VM. A render node refuses dumb buffers (`EACCES`), so the dumb-buffer path is proven only in tier 3.

## The working loop

The work runs local-first: GitHub's runners are slow, so they are waited on only at merge points.

1. **A worktree per branch,** beside the repositories. From the vtium checkout: `git worktree add -b feat/<topic> ../vtium-<topic> integration`. The forks get no worktrees; they are read only. Each worktree has its own `target/`, so parallel builds never contend for a lock. No two agents edit the same file; a crate has one owner at a time.
2. **The branch starts from `integration`,** a branch in the maintainer's checkout that collects reviewed work, so later work builds on earlier work before main sees it. It reaches GitHub at each merge point; a fresh clone with no `integration` branches from main.
3. **The author commits locally and runs the gates.** It does not push or open a PR. Its report gives the branch, the head commit, the gate results and the departures, as a PR body would.
4. **A fresh reviewer reads the diff** (`git diff integration...<branch>`), runs the gates, and plants the defects the change claims to catch. Its findings are fixed in a round on the same branch. A fix round after the second goes to a freshly briefed agent.
5. **The reviewed branch merges into `integration`** with `git merge --no-ff`, and the full gates run again on the merged tree.
6. **At a merge point,** such as the end of a wave, `integration` is pushed, one PR goes to main, and its CI is waited on. A PR merges by a regular merge commit, never squash or rebase.

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
| How the C reference worked, part by part, and the vtium issue that takes each part | [docs/c-reference.md](docs/c-reference.md) |
| What a contribution must show, the code rules, the kitty source rule | [CONTRIBUTING.md](CONTRIBUTING.md) |
| The target set and its coverage | `docs/architecture/term/ADR-1000-*` |
| A crate's departures and bounds | `crates/<crate>/README.md` |
| Where vtium touches systemd, logind or PID 1, and what it does without them | [docs/systemd.md](docs/systemd.md) |
| The plan and its order | the tracker, vtium/vtium#47 |
| The replay interface | `tools/conformance/corpus/INTERFACE.md` |
| Tier 3 | [tools/tier3/README.md](tools/tier3/README.md) |
