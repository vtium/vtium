# Contributing to vtium

Contributions are welcome, written by hand, by a large language model, or both. Read [docs/architecture.md](docs/architecture.md) for the crates and what each one owns.

## What every contribution must show

### It passes the evaluation gauntlet, with proofs

Every contribution passes the evaluation gauntlet: the CI gates, and the harness tiers that cover what it touches. [docs/architecture.md](docs/architecture.md#how-it-is-judged) lists the tiers.

The pull request carries the proofs: the commands run, their results, and a test seen failing before the change and passing after it.

CI runs these gates on every pull request. Run them locally before you push:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo test --workspace --all-features
cargo build --profile minspec
docs/scripts/adr lint --check
docs/scripts/coverage
```

`docs/scripts/adr cite --check` also passes before a change is merged. [AGENTS.md](AGENTS.md) describes the working loop: the worktrees, the tiers and their commands, the VT review, and how work reaches main.

### Anything new extends the harness

A contribution that adds something new extends the harness in the same pull request. It proves the extension works: the new check is shown red without the feature, or with the defect planted, and green with it. A new capability without a harness case that would catch its absence is not accepted.

### Its tests cite the rows they cover

ADR-1000's tables list every mode and feature vtium supports, ignores or refuses. A test that drives a row's behaviour through the crate that implements it, and checks the result, names the row in a `// covers: <id>` comment above it, or a `# covers: <id>` comment in a replay file. A test of a lower layer the row depends on carries no tag. `docs/scripts/coverage` checks the tags and states how ids are formed, and `docs/scripts/coverage --update` writes the tables' `tests` column from them.

### The submitter defends it

The person who submits a pull request answers for it. They can explain its purpose, what caused the need for it (the defect or the capability gap), its effects (what it changes and what could break), and its value. Review will challenge its design, its trade-offs and its proofs, and the submitter is expected to defend them in their own words.

However impressive a change is, it is not merged without a defence: why it is needed, and what happens when it is merged. Effects the submitter has not examined, or that the proofs show to be harmful, block it. A change its submitter cannot explain or defend is closed, whoever or whatever wrote the code.

### It says how the code was written

Every pull request states how its code was written: by an LLM, naming the model and tool if you know them; by hand; or mixed, saying which parts. LLM-written contributions are welcome. The maintainer's own code is written by LLMs under review, and says so the same way.

Nothing is rejected for how it was written. Every change runs the same gauntlet and gets the same review.

## Issues

Bug reports and capability requests have their own forms. A capability request is a work item:

- **Goal:** one sentence on what a user sees or can do once it lands.
- **Spec:** the document the behaviour follows, such as xterm ctlseqs, the kitty protocol docs or ECMA-48, with a link.
- **Crates:** the crates it touches.
- **Acceptance:** the harness case that will prove it works.

A change that needs a design decision first gets the `design` label. The decision is recorded before the code is written.

Decisions are recorded as ADRs in `docs/architecture/`, created and checked with `docs/scripts/adr`. The docs conventions are in [docs/architecture.md](docs/architecture.md#tables-references-and-maps).

## Pull requests

- One change per pull request.
- Tests first. A fix starts with a test that fails without it, and a capability starts with the harness case that judges it.
- Behaviour follows the protocol specifications. Where a specification leaves room, vtium's conformance goldens decide, and a change to a golden is its own reviewed change, listing what moved and why.

## Rules for the code

- **`unsafe` only in `vtium-platform`.** It is there for ioctls and FFI. Every other crate carries `#![forbid(unsafe_code)]`.
- **Bounded parsing.** Anything read from the pty comes from an untrusted program. Every buffer, string, parameter list and image that input can grow has a fixed limit.
- **Write from specs and permissive sources, never from kitty's code.** Implement from published protocol specifications and from permissively licensed implementations such as Ghostty, WezTerm, foot and libsixel. kitty is GPL-3.0, and its source is not read while working on vtium, by a person or by an agent. kitty as a running program may be used for black-box comparison. This is the C fork's rule, kmscon ADR-300. <!-- adr-cite-ignore: kmscon's record, not one of vtium's --> A file adapted from a permissive source names the source and its licence in its header.

## Licence

Contributions are made under the MIT licence in [LICENSE](LICENSE). A file ported from kmscon or libtsm keeps the copyright notice it carried, beside vtium's; [COPYING](COPYING) describes the notices.

## Conduct

Participation is covered by the [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Report vulnerabilities privately, as [SECURITY.md](SECURITY.md) describes, not in a public issue.
