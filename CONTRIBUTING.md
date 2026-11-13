# Contributing to vtium

Contributions are welcome. You may write them by hand, work with AI coding agents, or both; vtium itself is built by one maintainer steering agents, and [How vtium is built](docs/concepts/how-vtium-is-built.md) explains how. Read [docs/architecture.md](docs/architecture.md) for the crates and what each one owns.

This page first says what judges a change, and then how to build one.

## What judges a change

### The tiers

vtium is judged in tiers: unit tests, tier 1's replays of byte streams compared as text, the capability probes, tier 2's pictures compared pixel for pixel, tier 3's screens captured from vtium running on a VT in a virtual machine, and last the maintainer's review by eye on a real VT. [How vtium is judged](docs/concepts/how-vtium-is-judged.md) explains each tier, and [AGENTS.md](AGENTS.md#how-vtium-is-judged) gives their commands.

### The goldens

A golden is a saved expected result that a tier compares vtium's output with. Inside a change, its author never rewrites, widens or regenerates the golden that judges it. A golden moves only in a commit of its own, which lists each value that moved and why; a contributor may propose such a commit, and the maintainer reviews it before it lands. A new golden is provisional, and judges nothing, until the maintainer has reviewed it.

Behaviour follows the protocol specifications. Where a specification leaves room, vtium's goldens decide.

### ADR-1000's rows

ADR-1000's tables list every mode and feature vtium supports, ignores or refuses. A test that drives a row's behaviour through the crate that implements it, and checks the result, names the row in a `// covers: <id>` comment above it, or a `# covers: <id>` comment in a replay file. A test of a lower layer the row depends on carries no tag. `docs/scripts/coverage` checks the tags and states how ids are formed, and `docs/scripts/coverage --update` writes the tables' `tests` column from them.

## A contribution brings its judge before its code

A contribution starts with what will judge it: a test, a new golden marked provisional, or the ADR-1000 row it serves. A fix starts with a test that the defect makes fail. A capability starts with the harness case that would catch its absence, in the same pull request, and a capability without one is not accepted. A contribution that changes what vtium draws or answers brings the golden commit that records it, as a commit of its own.

Each new test or check is seen to fail before it is trusted: it fails without the change, or with the defect it catches planted on a copy of the file, and it passes with the change.

## Building a change

### The gates

The gates are the checks every change passes before review. [AGENTS.md](AGENTS.md#the-gates) lists them in order as one command line, and `make gates` runs the same commands and reports each one's result. CI runs them on every pull request. A gate that cannot run where you work, such as one that needs the network, is reported as not run, with the reason.

`rust-toolchain.toml` pins the Rust toolchain that the gates and CI run on, and rustup installs it the first time you run cargo in the checkout. `rust-version` in `Cargo.toml` is the oldest Rust that vtium builds on, and `make msrv`, one of the gates, builds the workspace, its tests included, and the minspec profile with it. To move the pin, change `channel` in `rust-toolchain.toml`, fix what the gates then report, and put both in one commit.

The keyboard tests read real keyboard layouts that `crates/vtium-platform/tests/keymaps/` holds as committed files, so `cargo test` needs nothing more. To compile those files again, `make keymaps` needs `xkbcli`, libxkbcommon's command line tool, and xkeyboard-config with its pkg-config file. The package libxkbcommon provides `xkbcli` on Arch, libxkbcommon-tools on Debian and libxkbcommon-utils on Fedora. The fixtures' README says when to run it.

[AGENTS.md](AGENTS.md) also describes the rest of the working loop: the worktrees, the VT review, and how work reaches main.

### The pull request

- One change per pull request.
- The pull request carries the proofs: each gate's command and result, and the runs that show each new test or check failing and then passing.
- It states how its code was written: with AI agents, naming the model and tool if you know them; by hand; or mixed, saying which parts. The maintainer's own code is written by agents under review, and says so the same way. Nothing is rejected for how it was written: every change runs the same gates and gets the same review.

### The submitter answers for it

Before you open the pull request, you have run the gates and read the whole diff yourself.

The person who submits a pull request can explain its purpose, what caused the need for it (the defect or the capability gap), its effects (what it changes and what could break), and its value. Review will challenge its design, its trade-offs and its proofs, and the submitter is expected to defend them in their own words.

However impressive a change is, it is not merged without a defence: why it is needed, and what happens when it is merged. Effects the submitter has not examined, or that the proofs show to be harmful, block it. A change its submitter cannot explain or defend is closed, whoever or whatever wrote the code.

## Issues

Bug reports and capability requests have their own forms. A capability request is a work item:

- **Goal:** one sentence on what a user sees or can do once it lands.
- **Spec:** the document the behaviour follows, such as xterm ctlseqs, the kitty protocol docs or ECMA-48, with a link.
- **Crates:** the crates it touches.
- **Acceptance:** the harness case that will prove it works.

A change that needs a design decision first gets the `design` label. The decision is recorded before the code is written.

Decisions are recorded as ADRs in `docs/architecture/`, created and checked with `docs/scripts/adr`. The docs conventions are in [docs/architecture.md](docs/architecture.md#tables-references-and-maps).

## Rules for the code

- **`unsafe` only in `vtium-platform` and `vtium-image-helper`.** `vtium-platform` holds it for ioctls and FFI, and `vtium-image-helper` in its one module `src/fork.rs`, which forks the helper's worker (ADR-1101 D1). Both crates carry `#![deny(unsafe_code)]` and allow it at each site, with a SAFETY comment. Every other crate's library carries `#![forbid(unsafe_code)]`, and the programs' own entry points, the `main.rs` of `vtium`, `vtium-config` and `vtium-image-helper`, use no `unsafe`.
- **Shown words live in a catalog.** Every string vtium shows a person, on its screen or on the command line, is written in its crate's catalog module, `crates/vtium/src/notice.rs` or `crates/vtium-settings/src/messages.rs`, and nowhere else. Each entry is a whole message: a choice between wordings is two entries, never a sentence built from pieces. Anything that is not words to translate enters a message as a placeholder's value: a name, a path, a number, a key's name, the file's own syntax, or the system's error text. A message about a problem says what happened, then what the person can do next. Tier 2 and the specimens draw from the same catalog the console uses. Log lines stay where they are. The known exceptions, still to move: the configuration reader's, the frit reader's and the memory budget's lines that check-config prints (`crates/vtium/src/config/`, `font.rs`, `config/budget.rs`), the head file outcome's lines in `crates/vtium-settings/src/outcome.rs`, and the usage lines of `vtium --help` in `main.rs`.
- **Bounded parsing.** Anything read from the pty comes from an untrusted program. Every buffer, string, parameter list and image that input can grow has a fixed limit.
- **Write from specs and permissive sources, never from kitty's code.** Implement from published protocol specifications and from permissively licensed implementations such as Ghostty, WezTerm, foot and libsixel. kitty is GPL-3.0, and its source is not read while working on vtium, by a person or by an agent. kitty as a running program may be used for black-box comparison. The rule keeps vtium's code under the MIT licence. A file adapted from a permissive source names the source and its licence in its header.

## Licence

Contributions are made under the MIT licence in [LICENSE](LICENSE). A file ported from kmscon or libtsm keeps the copyright notice it carried, beside vtium's; [COPYING](COPYING) describes the notices.

Every commit to vtium is signed with an SSH key, from 2026-11-10 on. Set `gpg.format = ssh`, `user.signingkey` and `commit.gpgsign = true` in your git config, and add the key to GitHub as a signing key. The pre-push hook in `tools/git-hooks/` refuses a push that carries an unsigned commit made after that date. Turn it on with `git config core.hooksPath tools/git-hooks`.

## Conduct

Participation is covered by the [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Report vulnerabilities privately, as [SECURITY.md](SECURITY.md) describes, not in a public issue.
