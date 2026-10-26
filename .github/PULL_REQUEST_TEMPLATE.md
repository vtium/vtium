## Summary

<!-- What changes and why, in a few sentences. -->

## Issue

<!-- Closes #N. -->

## How the code was written

<!-- Required. One of: by an LLM (name the model and tool, if known); by hand; or mixed (say which parts). -->

## Tests

<!-- The tests that fail before this change and pass after it. -->

## Gates run

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features`
- [ ] `cargo build --profile minspec`
- [ ] Harness tiers, if the change affects what they judge: <!-- which tiers and cases -->

## Not verified

<!-- What you could not check, such as hardware you do not have. Write "nothing" if so. -->
