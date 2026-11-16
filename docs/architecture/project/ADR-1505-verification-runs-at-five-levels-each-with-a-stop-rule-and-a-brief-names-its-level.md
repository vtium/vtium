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
  - operator: aaronsb
    said: "Tier 1 is reviewed as pictures that vtium draws, through a tool that also adds to the coverage of review automation."
    via: the steering session's question on 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Every tier 1 picture keeps parity with a live interaction: each has a live play that draws the same screens, and the two paths are checked to agree."
    via: the steering session's question on 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "vtium's goldens fall into two tiers that are reviewed differently. The maintainer reviews every new or moved pixel golden of tier 2, tier 3 and the specimens by eye, at natural 1:1 resolution, because people judge the shape and flow of a drawing best. A behaviour golden of tier 1 is a readable case that cites the specification statement it tests in a # spec: line, with an expected result written independently of vtium before vtium's output is seen, from the specification or from an outside reference. Where that expectation agrees with vtium's output, a fresh reviewer agent checks that each cited statement supports it, and the steering session accepts the golden in a golden-only commit; the maintainer does not review these one by one. Where the two disagree, the case comes to the maintainer as a short question. An opaque .bin case converts to a readable case where its input stays byte-identical, and a .bin case that stays keeps the maintainer's review. At intervals the maintainer reviews the behaviour tier as a whole, to catch drift that case-by-case agreement cannot see. The author of a change never accepts its own golden, in either tier."
    via: the steering session's question on 2026-11-13, on reviewing tier 1 goldens
    paraphrase: true
  - operator: aaronsb
    said: "The behaviour tier's review is a ladder of three steps, and it applies to every failing comparison, not only to new goldens. First, the tool compares vtium's output with the golden, or a new case's output with its independent expectation; a match needs no one. Second, a fresh reviewer agent gets every mismatch, a regression where an accepted golden now fails or a new case whose expectation and output differ. It reads the cited specification statement and the maintainer's recorded decisions and settles the obvious cases: vtium is wrong, a defect to fix while the golden stays, or the golden or expectation is wrong, a golden-only change that names the statement that settles it. A case is obvious when a specification statement or a recorded decision addresses it directly and leaves one reading. Third, the maintainer gets what the reviewer cannot settle: more than one defensible reading, no statement that addresses the case, or a reading that would change a recorded decision, each as a short question with the quote, the readings and the one vtium follows. The author of a change never acts as the second step's reviewer of its own change. For pixel goldens an agent may triage a moved picture, such as which change moved it and by how many pixels, but acceptance stays with the maintainer at 1:1."
    via: the steering session, 2026-11-13, adding to the decision on reviewing tier 1 goldens
    paraphrase: true
  - operator: aaronsb
    said: "we don't run full verification runs independently (instead, assemble and verify just the capabilities added, then integrate and stage, and add them to manual review for me and scenario testing for automated testing) - I can be a path to judge the capability, and then with those reviews set, the agents can verify that code paths/way it's implemented/security/safety/performance are correct"
    via: the steering session, 2026-11-14, on how a wave's cards are verified
  - operator: aaronsb
    said: "V1 keeps all of tier 1 when a card changes the terminal. Every card gets a fresh code review after the maintainer's verdict. The wave's candidate merges into integration once its full gate line passes, before the maintainer's look and the reviews, and the stage PR to main waits until every card has had its review. The ADR-1503 D10 reviewer stays at V2, before the maintainer's look."
    via: the steering session's questions on 2026-11-14, on the amendment's open points
    paraphrase: true
  - operator: aaronsb
    said: "we previously have written in agents that a full review per worktree branch isn't always necessary - sometimes it can just be individual scoped reviews, then integrated in and then we run a single full review there, and fix findings"
    via: the steering session, 2026-11-16, on how a wave's code is reviewed
  - operator: aaronsb
    said: "scoped at merge, full after"
    via: the steering session's question on 2026-11-16, 'Where do the two reviews sit?'
  - operator: aaronsb
    said: "the reason I remind of that flow is that it consumes an unusual amount of time and tokens to perform full reviews otherwise."
    via: the steering session, 2026-11-16, after the amendment on how a wave's code is reviewed
  - operator: aaronsb
    said: "the cost has several dimensions. one is attention, from me: if a review flow iterates several times, it is difficult for me to continue to be focused on that item throughout a whole day, when I'm managing other concerns … the next is verification time: the same work is verified in slightly different ways that has significant statistical overlap and costs time. the last is token budget use: running and responding to nearly identical scenarios that get discarded on merge because they already verify correctly has a cost, but that cost is necessary because otherwise context doesn't happen. (the cost of getting it done)"
    via: the steering session, 2026-11-16, on the reason for one full review per wave
  - operator: aaronsb
    said: "1505 looks fine"
    via: the steering session, 2026-11-16, after the maintainer read the amendment's three commits with git log -p
---

# ADR-1505: Verification runs at five levels, each with a stop rule, and a brief names its level

## Summary

- **Decided:** Verification runs at five levels, V0 to V4, and each level has a stop rule that says what it never runs. A card proves only the capability it adds. V0 is the author's work on each change: fmt, clippy, the tests of the crates touched, each new test seen failing first, and planted defects. V1 is the author's end-of-card run, once: the card's new tests and scenarios, all of tier 1 when the card changes the terminal, and tier 2 or the specimens only when the card changes pictures. V1 runs tier 3 only when the card touches the install tree, VT or session handling, DRM/KMS, the GPU context or systemd. V2 is the steering session's run on the wave's candidate, the one branch that assembles the wave's cards. The full gate line runs once there, and not on each card. V2 also runs step 2 of ADR-1503 D10's ladder. A fresh reviewer settles each behaviour mismatch that has one reading, and checks the citations of new cases that match. V3 comes before the code review. In V3 the maintainer judges each new capability on a VT tour page or a picture page, once per batch, and the verdicts are the fix list. The VT tour runs on the GPU path and the CPU path. Each accepted capability keeps scenario tests that go on checking it. V2 also gives each card a scoped review as it merges into the candidate: the defects the card claims to catch are planted again, its diff is checked against the files its brief gave it, and a fresh reviewer reads the card's riskiest path. After the verdict, one full code review reads the wave's code as one change on `integration`: its code paths, the places where one card's code meets another's, its implementation, safety and security, and performance, with one fix round for the wave. The picture sheet shows every moved and new pixel golden, and the tier 1 cases that ADR-1503 D10 sends to the maintainer. A tier 1 case comes as a picture that `make tier1-shots` draws, with a live play, `make tier1-play`, that draws the same screens. At a release candidate, V3 also holds ADR-1503 D10's review of the behaviour tier against drift. V4 is the release candidate's run, once: tier 3 for every variant, every fuzz target, every CI job and the specimen check. Every brief names its level and the agent runs exactly that level. A card is sized to finish in well under half of an agent's context. A slice plan in an ADR names what each slice proves, and never which tiers it runs.
- **Trades away:** Some checks that ran on every branch now run once per batch, so a defect that only a heavy check shows surfaces at V2 or later, not at the branch that caused it. A defect outside the capability a card adds surfaces on the wave's candidate, not at the card. Before the verdict, a card gets only a scoped review, so a defect off the card's claimed defects and its riskiest path waits for the full review after the verdict. A second review round goes to the maintainer's eyes, not to another reviewer.
- **One-way?** No. AGENTS.md holds the table, and the maintainer can move a check between levels by editing it.
- **Probes:** *Confident:* the maintainer's look settles a picture faster than review rounds do, and an author that redraws goldens while another branch still changes the look redraws them twice. *Not confident:* the exact list of what triggers tier 3 at V1 (install tree, VT or session handling, DRM/KMS, the GPU context, systemd) is right, and a card that touches something else may still need it.
- **Inversion:** Between "every branch runs every check" (slow, and the agents' time goes to repeating checks that one run at the merge would do) and "the maintainer's eyes alone" (the alpha.6 defects the automated checks found would have reached a VT). The answer puts each check at the cheapest level that still catches what it catches, and gives the maintainer the one look that nothing else replaces.

> Edited 2026-11-13: the maintainer decided that tier 1 is reviewed as pictures that vtium draws, and that each picture keeps parity with a live play (`considered`, 2026-11-13). The Summary's sentence on V3 read: "V3 is the maintainer's look, once per batch: one picture sheet of every moved and new golden, then the VT tour on the GPU path and the CPU path."

> Edited 2026-11-13, later the same day: the maintainer decided that goldens are reviewed in two tiers (ADR-1503 D10, and the fourth and fifth `considered` entries). V2 gained step 2 of D10's ladder, and V3 covers pixel goldens, the tier 1 cases D10 sends to the maintainer, and the drift review. The Summary's sentence on V2 read: "V2 is the steering session's run at each merge: the full gate line once per push, and one fresh correctness review only for logic a picture cannot show." The sentence on V3 read: "V3 is the maintainer's look, once per batch: one picture sheet of every moved and new golden, tier 1's goldens among them as pictures that `make tier1-shots` draws, each with a live play, `make tier1-play`, that draws the same screens, then the VT tour on the GPU path and the CPU path."

> Edited 2026-11-14: the maintainer decided that a card proves only the capability it adds, that the full gate line runs once on the wave's candidate, and that the code review follows the maintainer's verdict (the sixth `considered` entry). The Summary's sentences on V1 read: "V1 is the author's end-of-card run, once: the gates the touched files feed, and a tier 3 run only when the card touches the install tree, VT or session handling, DRM/KMS, the GPU context or systemd." The sentence on V2 read: "V2 is the steering session's run at each merge: the full gate line once per push, and one fresh correctness review only for logic a picture cannot show." The sentence on V3 read: "V3 is the maintainer's look, once per batch: one picture sheet, then the VT tour on the GPU path and the CPU path." The next sentence began "The sheet shows". The last sentence of Trades away read: "A second review round on a logic change goes to the maintainer's eyes, not to another reviewer."

> Edited 2026-11-16: the maintainer decided that each card gets a scoped review as it merges into the candidate, and that one full code review of the wave follows the verdict (the eighth and ninth `considered` entries). The Summary's sentence on the code review read: "After the verdict, a fresh reviewer reads each card's code paths, implementation, safety and security, and performance, with one fix round per card." The last two sentences of Trades away read: "The code review of a card waits for the maintainer's verdict. A second review round on a card goes to the maintainer's eyes, not to another reviewer."

## Context

During the alpha.6 work, the automated checks found real defects: copied code, a GPU wrong-pixel bug and a color rule that had no test. They also cost a great deal of agent time. Agents re-ran the heavy checks on each branch, redrew pictures before the look had settled, so the pictures were redrawn again when the look moved, and ran review rounds that had no end. A single picture review and a single VT tour settled what the review rounds kept circling.

The maintainer said that the work does not need a long, thorough review of everything, and that the tour of the new items is enough. The maintainer also said that the agents' automated checks are valuable because they build the stage for the maintainer's own visual look, and that the look can end review cycles that otherwise have no end. AGENTS.md until now said "the checks of what it touched" for the author and left the rest of the choice to each brief, so each brief chose its own checks.

Terms used below. A level is a place in the work where a defined set of checks runs. A stop rule is a statement of what a level never runs, so that no level grows into the one above it. A card is one piece of work given to one delegated agent in its own worktree. A picture sheet is one page that the maintainer reads to judge goldens side by side. It shows every pixel golden that moved or is new, and the tier 1 cases that go to the maintainer (ADR-1503 D10). A capability is one thing a card adds that vtium can do, such as a key encoding or a way of drawing a script. A wave's candidate is one branch that merges every card of the wave, so the wave's checks run once on the merged code.

On 2026-11-14 the maintainer decided how a wave's cards are verified (the sixth `considered` entry). A card does not run a full verification of its own. Each card proves only the capability it adds. The steering session assembles the wave's cards, verifies them together, then integrates and stages them. The maintainer judges each new capability by eye, and the automated scenario tests keep checking it. With those verdicts set, fresh agents check each capability's code paths, its implementation, its safety and security, and its performance. The maintainer said "I can be a path to judge the capability": the maintainer's verdict comes first, and the code review follows it.

On 2026-11-16 the maintainer corrected how the record had read those words. The seventh `considered` entry, the steering session's paraphrase of the maintainer's answers, gave every card a full code review of its own after the verdict, and AGENTS.md followed it. The maintainer's own words of 2026-11-14 ask for something else: the steering session verifies the capabilities the cards add while it assembles them, and after the verdict the agents verify the code. The maintainer said that a full review of each branch is not always needed: each card gets a scoped review, the cards are integrated, and one full review then runs on the integrated code, and its findings are fixed. The scoped review runs at the merge into the candidate, and the full review after the verdict. The maintainer's reason is cost, which has three parts (the tenth and eleventh `considered` entries). The first is the maintainer's attention: a review that comes back for several rounds is hard to stay focused on across a day of other work, while each agent has only its one task. The second is verification time: checks that verify the same work in slightly different ways overlap heavily. The third is tokens: runs that repeat nearly the same scenarios are discarded at the merge once they verify correctly. Some of that token cost is the price of getting the work done, because an agent builds its context by doing the work.

## Decision

### D1: five levels, each with a stop rule

The table lives in AGENTS.md, in the section "How vtium is judged", and this record states what each row decides.

V0 runs while the author works. The author runs `cargo fmt --all --check`, clippy and the tests of the crates it touched. The author watches each new test fail before the change and pass after it, and plants the defects a change claims to catch. V0 never runs the full gate line, tier 2 in full, the specimens or tier 3.

V1 runs once, at the end of the card, and the author runs it. V1 proves the capability that the card adds, and nothing more. It runs the card's new tests and scenarios, and the author has seen each of them fail before the change. When the card changes the terminal, such as its parser, modes, replies or key encoding, V1 also runs all of tier 1 (`make conformance`) once. A regression in an older case then reaches the author while the card is still fresh in the author's context, and the run takes about a minute. It runs a fuzz target within reach of the change for 60 seconds. It runs `make tier2` once only when the card changes drawing, fonts or screens. When pictures move, V1 redraws goldens and specimens once, and only when no other branch still changes the look. V1 never runs the full gate line. V1 runs no tier 3, unless the card touches the install tree, VT or session handling, DRM/KMS, the GPU context or systemd. Then it runs `make tier3-minimal` once. When a tier 1 comparison fails, the author fixes vtium or reports the case for V2's reviewer (ADR-1503 D10). The author leaves the golden as it is.

> Edited 2026-11-13, later the same day: the maintainer decided that goldens are reviewed in two tiers (ADR-1503 D10, and the fifth `considered` entry). The paragraph on V1 gained its last two sentences.

> Edited 2026-11-14: the maintainer decided that a card proves only the capability it adds (the sixth `considered` entry). V1 runs the card's own tests and scenarios, and all of tier 1 when the card changes the terminal (the seventh `considered` entry). The paragraph gained the sentences that say V1 proves only that capability, that it runs all of tier 1 for a terminal card, and that it never runs the full gate line. The paragraph's second sentence read: "It runs the gates that the touched files feed: tier 1 when the card touched the terminal, `make tier2` once when it touched drawing, fonts or screens, and a fuzz target within reach of the change for 60 seconds."

V2 runs on the wave's candidate, and the steering session runs it. The steering session merges each card of the wave into the candidate. It runs the full gate line once on the candidate, and not on each card. The candidate then merges into `integration`, and the full gate line runs once per push of `integration`. V2 runs a scoped review of each card as the card merges, and no full code review, because the full review waits for the maintainer's verdict at V3. The scoped review checks what the card claims. The steering session plants again the defects that the card says its tests catch, and checks that the card's diff stays inside the files its brief gave it. A fresh reviewer reads the card's riskiest path, the one the brief or the author's report names. A finding goes back to the card's author before the candidate's full gate line runs. V2 runs step 2 of ADR-1503 D10's ladder. A fresh reviewer, which did not write the change, gets each mismatch of a behaviour case and settles the cases that have one reading. The reviewer also checks that each cited statement of a new case that matches supports its expectation. A case that fails that check goes back to its author in V2's one fix round. The steering session then makes the commits that change goldens and nothing else.

> Edited 2026-11-13, later the same day: the maintainer decided that goldens are reviewed in two tiers (ADR-1503 D10, and the fourth and fifth `considered` entries). The paragraph on V2 gained its last five sentences, on step 2 of D10's ladder.

> Edited 2026-11-14: the maintainer decided that the wave's cards are assembled and verified together, and that the code review follows the maintainer's verdict (the sixth `considered` entry). V2 runs on the wave's candidate, and its code review moved to V3. The paragraph's first six sentences read: "V2 runs at each merge into the candidate or into `integration`, and the steering session runs it. It runs the full gate line once per push. It adds one fresh correctness review, and only for logic that a picture cannot show: new algorithms, provenance, damage tracking, the GPU path and concurrency. V2 allows one review round. The steering session checks the fix round by planting the defects again. A second round goes to the maintainer's eyes." The next sentence began "V2 also runs step 2".

> Edited 2026-11-16: the maintainer decided that each card gets a scoped review as it merges into the candidate (the eighth and ninth `considered` entries). The sentence "V2 runs no code review, because the code review waits for the maintainer's verdict at V3." became the five sentences on the scoped review.

V3 is the maintainer's verdict on each new capability, once per batch, and the code review that follows the verdict. The maintainer's verdict comes before the code review. The maintainer judges each new capability of the batch on a VT tour page or on a picture page. A VT tour page is a step of the VT tour (`make vt-accept`), which the maintainer runs on the GPU path and on the CPU path. A picture page is a page of the picture sheet. The picture sheet shows every moved and new pixel golden: those of tier 2, tier 3 and the specimens. It also shows the tier 1 cases that ADR-1503 D10 sends to the maintainer. Those are the behaviour cases that V2's reviewer could not settle, each posed as a short question. They also include each `.bin` case whose golden is new or moved. A behaviour golden that passed V2's spec check does not come to the maintainer one case at a time. Tier 1's goldens are text: the screen state and replies vtium produces after a stream of input bytes. The maintainer sees each such case as a picture that `make tier1-shots` draws through vtium's own renderer. The tool draws one picture for each replay case and one for each screen a scenario dumps. Each picture comes with its live play: `make tier1-play` plays the same case or scenario in a terminal, and inside a vtium console vtium draws the same screens. The picture and the live play come from one replay of the same input, and `make tier1-shots` checks that the two draw the same picture. At a release candidate, V3 also holds the review of the behaviour tier against drift, at the cadence that ADR-1503 D10 sets. The maintainer's verdicts are the fix list. A fix comes back to the maintainer only if it moves a picture. Each accepted capability keeps scenario tests that go on checking it automatically. The scenarios that the card's author wrote at V1 become those tests. A fix adds a scenario for each case that the verdict found and the scenarios miss.

> Edited 2026-11-13: the maintainer decided that tier 1 is reviewed as pictures that vtium draws, through a tool that also adds to the coverage of review automation, and that each picture keeps parity with a live play (`considered`, 2026-11-13). The sentences on tier 1's goldens and their live play were added. The paragraph read: "V3 is the maintainer's look, once per batch. The maintainer reads one picture sheet of every moved and new golden, and then runs the VT tour (`make vt-accept`) on the GPU path and on the CPU path. The answers are the fix list. A fix comes back to the maintainer only if it moves a picture."

> Edited 2026-11-13, later the same day: the maintainer decided that goldens are reviewed in two tiers (ADR-1503 D10, and the fourth and fifth `considered` entries). The picture sheet now shows pixel goldens and the tier 1 cases D10 sends to the maintainer, and V3 holds the drift review at a release candidate. The paragraph read: "V3 is the maintainer's look, once per batch. The maintainer reads one picture sheet of every moved and new golden, and then runs the VT tour (`make vt-accept`) on the GPU path and on the CPU path. Tier 1's goldens are text: the screen state and replies vtium produces after a stream of input bytes. They join the picture sheet as pictures that `make tier1-shots` draws through vtium's own renderer, one for each replay case and one for each screen a scenario dumps. Each picture comes with its live play: `make tier1-play` plays the same case or scenario in a terminal, and inside a vtium console vtium draws the same screens. The picture and the live play come from one replay of the same input, and `make tier1-shots` checks that the two draw the same picture. The answers are the fix list. A fix comes back to the maintainer only if it moves a picture."

After the maintainer's verdict, the steering session starts one full code review of the wave, by fresh reviewers that did not write its cards. The review reads the diff that the wave's candidate brought to `integration` as one change: its code paths, the places where one card's code meets another's, its implementation, its safety and security, and its performance. A large wave's review may be split across several reviewers by crate, and it is still one round. The reviewers run the tests of what the wave touched and plant the defects that the cards claim to catch. Every finding is fixed in one fix round, and the fix lands on `integration`. The steering session checks the fix round by planting the defects again. A second round goes to the maintainer's eyes. The pull request from `stage` to main merges only after its batch has had the full review and its fix round.

> Edited 2026-11-14: the maintainer decided that the maintainer judges each new capability before the code review, and that fresh agents then review each capability's code (the sixth `considered` entry). The code review moved here from V2. It now covers every card, and no longer only logic that a picture cannot show. The paragraph's first two sentences read: "V3 is the maintainer's look, once per batch. The maintainer reads one picture sheet, and then runs the VT tour (`make vt-accept`) on the GPU path and on the CPU path." Its last sentences, on the verdicts, the scenario tests and the code review, were added or reworded; the sentence on the fix list read: "The answers are the fix list."

> Edited 2026-11-16: the maintainer decided that one full code review of the wave follows the verdict, in place of one review for each card (the eighth and ninth `considered` entries). The paragraph read: "After the maintainer's verdict, the steering session starts a fresh reviewer for each card, an agent that did not write the card. The reviewer reads the card's code paths, its implementation, its safety and security, and its performance. The reviewer runs the tests of what the card touched and plants the defects that the card claims to catch. Each card gets one fix round, and the fix lands on `integration`. The steering session checks the fix round by planting the defects again. A second round goes to the maintainer's eyes. The pull request from `stage` to main merges only after each card in its batch has had its code review and its fix round."

V4 runs once on the release candidate, and the steering session runs it. It runs `make tier3` for every variant, the default run of every fuzz target, every CI job including `arm-native`, and `make specimens-check`. A defect found at V4 is fixed on the candidate and rechecked at its own level, and the whole ladder does not run again.

### D2: a brief names its level

Every brief names the level the agent runs, and the agent runs exactly that level. A brief does not add checks beyond the table, and neither does a slice plan in an ADR. An ADR's slice list names what a slice proves, and does not say which tiers the slice runs, because the level decides that.

### D3: a card is sized to finish well within its context

The steering session sizes each card to finish in well under half of an agent's context, so that verification does not cost more than the build.

## Consequences

> Edited 2026-11-14: the maintainer decided that the code review follows the maintainer's verdict (the sixth `considered` entry). The third Positive item read: "Review has an end: one round at V2, and then the maintainer's eyes." The last two Positive items, the last two Negative items and the third alternative were added.

> Edited 2026-11-16: the maintainer decided that each card gets a scoped review at the merge and that one full review of the wave follows the verdict (the eighth and ninth `considered` entries). The third Positive item read: "Review has an end: one round per card at V3, and then the maintainer's eyes." The last Negative item read: "Every card gets a code review, not only a card whose logic a picture cannot show, so review costs one round for each card." The fourth Positive item and the last alternative were added.

### Positive

- Each check runs where it is cheapest and where it still catches what it catches. Heavy checks run once per batch, and not once per branch.
- Pictures are redrawn once, after the look has settled, so the maintainer reads one picture sheet in place of several drafts.
- Review has an end: a scoped review per card at V2, one full round for the wave at V3, and then the maintainer's eyes.
- The full review reads the places where the wave's cards meet, which a review of each card alone never reads.
- The full gate line runs once for a wave, on its candidate, and not once for each card.
- The code review reads code whose capability the maintainer has already accepted, so no review round is spent on code that a verdict then changes.
- A brief is shorter, because it names a level in place of listing checks.

### Negative

- A defect that only a V2 or V4 check shows is found later than it would be if every branch ran that check.
- An author that skips V1's tier 3 because its card seems not to touch the install tree can be wrong, and the defect then waits for V4.
- A card that breaks something outside the capability it adds is found on the wave's candidate, after the author has reported.
- One full review reads a whole wave, so it is large, and a large wave's review is split across reviewers by crate.

### Neutral

- AGENTS.md's working loop and its section on how vtium is judged point at the levels in place of listing checks of their own.
- The fuzz rule in AGENTS.md's testing rules stays as it is; V1 names when the author runs a target.

## Alternatives Considered

- **Keep "the checks of what it touched" and let each brief choose.** Rejected: that is how branches came to re-run heavy checks and review rounds had no end.
- **Run the full gate line and tier 3 on every branch.** Rejected: the cost grows with the number of branches, and one run on the merged tree catches the same defects.
- **Review each card's code before the maintainer's verdict.** Rejected (2026-11-14): a verdict that changes a capability sends its reviewed code through review again, and the maintainer chose to judge the capability first. The scoped review at the merge is not this review: it reads only what the card claims and its riskiest path.
- **A full code review of each card after the verdict.** Rejected (2026-11-16): it costs one round for each card and never reads the places where the cards meet. The maintainer's words of 2026-11-14 asked the agents to verify the code once the verdicts were set, not to review each card in full.
- **Leave review to the maintainer alone.** Rejected: the automated checks found defects the maintainer's look would have met on a VT, and they build the stage that the look needs.
