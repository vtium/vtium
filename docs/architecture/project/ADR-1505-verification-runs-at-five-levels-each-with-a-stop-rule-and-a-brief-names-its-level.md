---
contract: adr/v1
kind: decision
verb: change
capability: process
basis:
  - operator: aaronsb
    level: directed
    said: "we don't have to go through a massive, thourough review of everything on this, just let me run the tour with the new items to look at"
    via: "the steering session, 2026-11-07, on how much review the alpha.6 candidate needs"
  - operator: aaronsb
    level: directed
    said: "while all the automated checks for agents are extremely valuable and build the stage for me to pick over things visually with the 'fine toothed eyeballs' ... we can use that at times to get through where it seems like there's endless cycles of reviews ahead of us"
    via: "the steering session, 2026-11-07, on the automated checks and the maintainer's own look"
  - evidence: "read 2026-11-07 from the alpha.6 work: the automated checks found real defects (copied code, a GPU wrong-pixel bug, a color rule with no test). Agents also re-ran heavy checks on each branch, redrew pictures before the look had settled, and ran review rounds that had no end. One picture review and one VT tour settled what the review rounds kept circling."
agent:
  name: Claude
  model: claude-sonnet-5-5
status: accepted
date: 2026-11-06
deciders:
  - aaronsb
related:
  - ADR-1501
  - ADR-1503
considered:
  - operator: aaronsb
    said: "ADR + AGENTS.md (Recommended)"
    via: "the steering session, 2026-11-07, answering the question 'Write it down?' about the five verification levels"
---

# ADR-1505: Verification runs at five levels, each with a stop rule, and a brief names its level

## Summary

- **Decided:** Verification runs at five levels, V0 to V4, and each level has a stop rule that says what it never runs. V0 is the author's work on each change: fmt, clippy, the tests of the crates touched, each new test seen failing first, and planted defects. V1 is the author's end-of-card run, once: the gates the touched files feed, and a tier 3 run only when the card touches the install tree, VT or session handling, DRM/KMS, the GPU context or systemd. V2 is the steering session's run at each merge: the full gate line once per push, and one fresh correctness review only for logic a picture cannot show. V3 is the maintainer's look, once per batch: one picture sheet of every moved and new golden, then the VT tour on the GPU path and the CPU path. V4 is the release candidate's run, once: tier 3 for every variant, every fuzz target, every CI job and the specimen check. Every brief names its level and the agent runs exactly that level. A card is sized to finish in well under half of an agent's context. A slice plan in an ADR names what each slice proves, and never which tiers it runs.
- **Trades away:** Some checks that ran on every branch now run once per batch, so a defect that only a heavy check shows surfaces at V2 or later, not at the branch that caused it. A second review round on a logic change goes to the maintainer's eyes, not to another reviewer.
- **One-way?** No. AGENTS.md holds the table, and the maintainer can move a check between levels by editing it.
- **Probes:** *Confident:* the maintainer's look settles a picture faster than review rounds do, and an author that redraws goldens while another branch still changes the look redraws them twice. *Not confident:* the exact list of what triggers tier 3 at V1 (install tree, VT or session handling, DRM/KMS, the GPU context, systemd) is right, and a card that touches something else may still need it.
- **Inversion:** Between "every branch runs every check" (slow, and the agents' time goes to repeating checks that one run at the merge would do) and "the maintainer's eyes alone" (the alpha.6 defects the automated checks found would have reached a VT). The answer puts each check at the cheapest level that still catches what it catches, and gives the maintainer the one look that nothing else replaces.

## Context

During the alpha.6 work, the automated checks found real defects: copied code, a GPU wrong-pixel bug and a color rule that had no test. They also cost a great deal of agent time. Agents re-ran the heavy checks on each branch, redrew pictures before the look had settled, so the pictures were redrawn again when the look moved, and ran review rounds that had no end. A single picture review and a single VT tour settled what the review rounds kept circling.

The maintainer said that the work does not need a long, thorough review of everything, and that the tour of the new items is enough. The maintainer also said that the agents' automated checks are valuable because they build the stage for the maintainer's own visual look, and that the look can end review cycles that otherwise have no end. AGENTS.md until now said "the checks of what it touched" for the author and left the rest of the choice to each brief, so each brief chose its own checks.

Terms used below. A level is a place in the work where a defined set of checks runs. A stop rule is a statement of what a level never runs, so that no level grows into the one above it. A card is one piece of work given to one delegated agent in its own worktree. A picture sheet is one page that shows every golden that moved and every new golden, so the maintainer judges them side by side.

## Decision

### D1: five levels, each with a stop rule

The table lives in AGENTS.md, in the section "How vtium is judged", and this record states what each row decides.

V0 runs while the author works. The author runs `cargo fmt --all --check`, clippy and the tests of the crates it touched. The author watches each new test fail before the change and pass after it, and plants the defects a change claims to catch. V0 never runs the full gate line, tier 2 in full, the specimens or tier 3.

V1 runs once, at the end of the card, and the author runs it. It runs the gates that the touched files feed: tier 1 when the card touched the terminal, `make tier2` once when it touched drawing, fonts or screens, and a fuzz target within reach of the change for 60 seconds. When pictures move, V1 redraws goldens and specimens once, and only when no other branch still changes the look. V1 runs no tier 3, unless the card touches the install tree, VT or session handling, DRM/KMS, the GPU context or systemd. Then it runs `make tier3-minimal` once.

V2 runs at each merge into the candidate or into `integration`, and the steering session runs it. It runs the full gate line once per push. It adds one fresh correctness review, and only for logic that a picture cannot show: new algorithms, provenance, damage tracking, the GPU path and concurrency. V2 allows one review round. The steering session checks the fix round by planting the defects again. A second round goes to the maintainer's eyes.

V3 is the maintainer's look, once per batch. The maintainer reads one picture sheet of every moved and new golden, and then runs the VT tour (`make vt-accept`) on the GPU path and on the CPU path. The answers are the fix list. A fix comes back to the maintainer only if it moves a picture.

V4 runs once on the release candidate, and the steering session runs it. It runs `make tier3` for every variant, the default run of every fuzz target, every CI job including `arm-native`, and `make specimens-check`. A defect found at V4 is fixed on the candidate and rechecked at its own level, and the whole ladder does not run again.

### D2: a brief names its level

Every brief names the level the agent runs, and the agent runs exactly that level. A brief does not add checks beyond the table, and neither does a slice plan in an ADR. An ADR's slice list names what a slice proves, and does not say which tiers the slice runs, because the level decides that.

### D3: a card is sized to finish well within its context

The steering session sizes each card to finish in well under half of an agent's context, so that verification does not cost more than the build.

## Consequences

### Positive

- Each check runs where it is cheapest and where it still catches what it catches. Heavy checks run once per batch, and not once per branch.
- Pictures are redrawn once, after the look has settled, so the maintainer reads one picture sheet in place of several drafts.
- Review has an end: one round at V2, and then the maintainer's eyes.
- A brief is shorter, because it names a level in place of listing checks.

### Negative

- A defect that only a V2 or V4 check shows is found later than it would be if every branch ran that check.
- An author that skips V1's tier 3 because its card seems not to touch the install tree can be wrong, and the defect then waits for V4.

### Neutral

- AGENTS.md's working loop and its section on how vtium is judged point at the levels in place of listing checks of their own.
- The fuzz rule in AGENTS.md's testing rules stays as it is; V1 names when the author runs a target.

## Alternatives Considered

- **Keep "the checks of what it touched" and let each brief choose.** Rejected: that is how branches came to re-run heavy checks and review rounds had no end.
- **Run the full gate line and tier 3 on every branch.** Rejected: the cost grows with the number of branches, and one run on the merged tree catches the same defects.
- **Leave review to the maintainer alone.** Rejected: the automated checks found defects the maintainer's look would have met on a VT, and they build the stage that the look needs.
