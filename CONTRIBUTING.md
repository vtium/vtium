# Contributing to vtium

Contributions are welcome, written by hand, by a large language model, or both. Read [docs/architecture.md](docs/architecture.md) for the crates and what each one owns.

## Say how the code was written

Every pull request states how its code was written: by an LLM, naming the model and tool if you know them; by hand; or mixed, saying which parts. The pull request template has a section for it. The maintainer's own code is written by LLMs under review, and says so the same way.

Nothing is rejected for how it was written. Every change is judged by the same tests and the same review.

## Issues

Bug reports and capability requests have their own forms. A capability request is a work item:

- **Goal:** one sentence on what a user sees or can do once it lands.
- **Spec:** the document the behaviour follows, such as xterm ctlseqs, the kitty protocol docs or ECMA-48, with a link.
- **Crates:** the crates it touches.
- **Acceptance:** the test or probe that shows it works.

A change that needs a design decision first gets the `design` label. The decision is recorded before the code is written.

## Pull requests

- One change per pull request.
- Tests first. A fix starts with a test that fails without it, and a capability starts with the test that judges it.
- The C fork, [aaronsb/kmscon](https://github.com/aaronsb/kmscon), is the reference for behaviour. Where vtium and the fork differ, say which is right and why.

CI runs these gates on every pull request. Run them locally before you push:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --profile minspec
```

A change to what the fork's harness judges, such as parsing, the screen or rendering, also names the harness tiers it was run against.

## Rules for the code

- **`unsafe` only in `vtium-platform`.** It is there for ioctls and FFI. Every other crate carries `#![forbid(unsafe_code)]`.
- **Bounded parsing.** Anything read from the pty comes from an untrusted program. Every buffer, string, parameter list and image that input can grow has a fixed limit.
- **Write from specs and permissive sources, never from kitty's code.** Implement from published protocol specifications and from permissively licensed implementations such as Ghostty, WezTerm, foot and libsixel. kitty is GPL-3.0, and its source is not read while working on vtium, by a person or by an agent. kitty as a running program may be used for black-box comparison. This is the C fork's ADR-300 rule. A file adapted from a permissive source names the source and its licence in its header.

## Licence

Contributions are made under the MIT licence in [LICENSE](LICENSE). A file ported from kmscon or libtsm keeps the copyright notice it carried, beside vtium's; [COPYING](COPYING) describes the notices.

## Conduct

Participation is covered by the [Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Report vulnerabilities privately, as [SECURITY.md](SECURITY.md) describes, not in a public issue.
