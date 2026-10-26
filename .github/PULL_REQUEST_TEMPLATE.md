## Summary

<!-- What changes, in a few sentences. -->

## Issue

<!-- Closes #N. -->

## Proof: gates and tiers run, with results

<!-- Required. Each command run and its result. Name the test seen failing before this change and passing after it, and paste both results. -->

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features`
- [ ] `cargo build --profile minspec`
- [ ] Harness tiers that cover what this touches: <!-- which tiers and cases, and their results -->

## Harness extension (for anything new): the new case, shown red then green

<!-- Required if this adds something new. Name the new harness case. Show it failing without the feature, or with the defect planted, and passing with it. Write "nothing new" otherwise. -->

## Defence

<!-- Required, in your own words. Review will challenge this, and you are expected to defend it.
- Purpose: what the change is for.
- Cause: the defect or capability gap that made it necessary.
- Effects: what it changes, and what could break.
- Value: what it is worth to vtium's users.
- Why this approach over the alternatives, and what it costs.
- What the proofs above show. -->

## How the code was written

<!-- Required. One of: by an LLM (name the model and tool, if known); by hand; or mixed (say which parts). -->

## Not verified

<!-- What you could not check, such as hardware you do not have. Write "nothing" if so. -->
