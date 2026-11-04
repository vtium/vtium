---
contract: adr/v1
kind: decision
verb: change
capability: [process, harness]
basis:
  - operator: aaronsb
    level: directed
    said: "we're nearing a transition phase where we should drop the references to the c fork reference as a first class comparison tool. this doesn't mean that we drop that internally in our work here, but outward committed content in the github repo that includes references to a c fork of kmscon will get convoluted and confusing for others."
    via: "the steering session, 2026-11-04"
  - operator: aaronsb
    level: directed
    said: "this conflicts with one of the core agent instructions which is to write in present tense. 'differences from kmscon' won't mean the same thing 6 months from now. we need a clean break in rhetoric too."
    via: "the steering session, 2026-11-04, on the crate READMEs' lists of differences from kmscon"
  - operator: aaronsb
    level: directed
    said: "the ADRs are fine. they're historical - the only thing I'd make changes to is if they are written in a tense that reads as if someone had the kmscon fork readily accessible. I plan on deleting the kmscon fork at some point."
    via: "the steering session, 2026-11-04, on the decision records"
  - operator: aaronsb
    level: directed
    said: "this is where we make the clean break. at this point, referencing old code libraries is just weight to drag along."
    via: "the steering session, 2026-11-04, on code comments"
  - operator: aaronsb
    level: directed
    said: "this is where I think we need to maintain an internal state that affords our c fork provenance while we continue to build privately, but outwardly what's committed doesn't reference it."
    via: "the steering session, 2026-11-04, on AGENTS.md"
  - operator: aaronsb
    level: directed
    said: "we're actually comparing against an inferior renderer at this point, so we should reference our own goldens now. think about the 'how to contribute' guidance which basically defines the judge before the builder."
    via: "the steering session, 2026-11-04, on the goldens and the tools"
  - evidence: "measured 2026-11-04 on integration at 4667b6c2: `git grep -nE 'C reference|kmscon|libtsm'` finds 3,915 lines in 349 files. By area, in lines and files: tools/tier3 728 in 53; Rust source under crates/ 707 in 167, of which 575 are comment lines; tools/conformance 666 in 30; the decision records under docs/architecture/ 654 in 26 (594 in the 22 records, 111 of them in frontmatter and 483 in the bodies, and 54 in ADR-1000's two tables); the ten crate READMEs at the top of each crate 418; the docs pages 365 in 18, of which docs/c-reference.md holds 247; tools/tier2 153 in 17; other files under crates/ (the test READMEs, Cargo.toml comments, install/ files) 90 in 14; the other tools 90 in 8; AGENTS.md 17; the Makefile 15; COPYING 6; README.md 3; CONTRIBUTING.md 2; zensical.toml 1; .github/ none."
  - evidence: "measured 2026-11-04 on integration at 4667b6c2: ten crate READMEs carry a `## Departures` section, 246 bold-titled items in all (vtium-term 67, vtium 65, vtium-platform 42, vtium-image 20, vtium-render 15, vtium-config 13, vtium-font 10, vtium-parse 9, vtium-image-helper 3, vtium-settings 2); three carry `## C behaviour kept`, three `## Beyond the C reference`, and the vtium crate's README a `## C notes` section; docs/architecture.md has a `## Beyond the C reference` section and docs/vt-review.md a `## Departures from the C reference's review`. Thirteen of the 22 decision records carry a `C notes` section and thirteen a `Where vtium departs from the C reference` section. A search of the record bodies for the C reference named with a present-tense verb finds 14 lines; the lines that point at the forks' checkouts are all in frontmatter basis entries."
  - evidence: "measured 2026-11-04 on integration at 4667b6c2: the goldens copied from the C reference are tier 1's corpus goldens (tools/conformance/corpus/golden, 164 files, and golden-frame) with the capability probes, tier 2's two PPMs in tools/tier2/reference/goldens/, and tier 3's 223 files in tools/tier3/c-goldens/ across seven variants. vtium's own goldens are tools/conformance/expected/ (49 screen files and the frame view), tools/tier2/goldens/ (86 files) and tools/tier3/goldens/ (158 files). The lists that excuse a difference from a copied golden are tools/conformance/expected.tsv (57 rows, every one of kind departure), tools/tier2/departures.tsv (4 rows) and tools/tier3/departures.tsv (10 rows covering 31 variant and screen pairs). tools/tier2/c-goldens.list names the 36 goldens of the C reference's tier 2, and tools/tier2/pending.tsv the 34 that no screen draws yet. Three C programs sit in the tree as the record of how digests and screens were made: crates/vtium-font/tests/reference/refdigest.c, crates/vtium-render/tests/reference/refcells.c and tools/tier2/reference/test_pixels.c; tools/tier2/src/cref.rs reads test_pixels.c on every tier 2 run to check that each ported screen carries its bytes."
  - evidence: "measured 2026-11-04 on integration at 4667b6c2: three make targets read the fork checkouts, `conformance-provenance`, `tier2-provenance` and `tier3-provenance`, through the variables LIBTSM and KMSCON, which default to the fork checkouts beside the vtium checkout; they run the subcommands `conformance provenance`, `tier2 provenance` and `t3.py provenance`, and tools/conformance/src/bin/conformance.rs and tools/tier2/src/bin/tier2.rs carry the same defaults. No gate and no CI job runs them. `make conformance` and `make tier2` check the copies inside the tree against their rows of provenance.tsv before they judge anything. tier 3's guest has twelve configuration files named guest/kmscon*.conf; its provisioning (guest/user-data.in) installs the C reference's runtime dependencies and links autovt@.service to kmsconvt@.service; its benchmark figures are named kmscon_cpu_ms, kmscon_cpu_pct, kmscon_cpu_ms_per_mpv_frame and kmscon_cpu_ms_per_tmux_sixel; and adr.yaml excludes tools/tier3/guest/screens/ from `adr cite` because the screens cite the C reference's own records."
  - evidence: "measured 2026-11-04 on integration at 4667b6c2, the mentions that name the released `kmscon` program or that the licence needs: COPYING and LICENSE carry kmscon's and libtsm's copyright notices and author lists; crates/vtium-font/src/unifont.rs, crates/vtium-parse/src/parser.rs and crates/vtium-parse/src/utf8.rs carry the copyright notices of the files they were ported from; docs/install.md's sections `Moving from kmscon` and `Sharing VTs with kmscon` tell a user how to move VTs from an installed `kmscon` to vtium and how to keep both; `vtium console` refuses `enable --all` while the template link names kmsconvt@.service; vtium answers to TERM=kmscon from crates/vtium/install/kmscon.ti, which `make install-tree` installs; docs/why-vtium.md says vtium reads `kmscon`'s configuration file."
  - upstream: "Claude Code's memory documentation (code.claude.com/docs/en/memory), read 2026-11-04: Claude Code loads CLAUDE.md and CLAUDE.local.md from the working directory and every directory above it; a gitignored CLAUDE.local.md exists only in the worktree where it was created; by default Claude Code reads AGENTS.md only when no CLAUDE.md, .claude/CLAUDE.md or CLAUDE.local.md is in the working directory or above it, and a CLAUDE.md that imports AGENTS.md brings AGENTS.md in through the import."
  - standard: "The MIT licence that vtium, kmscon and libtsm are under: the copyright notice and the permission notice shall be included in all copies or substantial portions of the software."
  - evidence: "measured 2026-11-04 on integration at 4667b6c2, what the three provenance records cover: tools/conformance/provenance.tsv lists the corpus' cases, its 164 goldens in each of golden/ and golden-frame/, corpus/INTERFACE.md, corpus/cases.list, probes/probes.replay, crates/vtium-term/tests/palettes.c.txt and six generator inputs; tools/tier2/provenance.tsv lists tools/tier2/reference/test_pixels.c and the two PPMs beside it; tools/tier3/provenance.tsv lists the 223 files of c-goldens/, guest/kmscon-minimal.conf and guest/kmscon-minimal-psf.conf, guest/meta-data, guest/user-data.in, guest/tier3-install and its service, 29 screen scripts, kitty-common.inc, sixel-common.inc and tmux-grid.inc, mpv-stats.lua, the photo directory and t3img.py. `make conformance`, `make tier2` and tier 3's preflight refuse to run when a listed copy differs from its row. The tracked screens mpv-sixel.sh and tmux-grid.inc read the benchmark names kmscon_pid and kmscon_ms that the untracked acceptance.inc defines."
  - standard: "console_codes(4), Linux man-pages 6.19 (man7.org, read 2026-11-04): the control characters, escape sequences and character-set mappings the Linux console supports, a subset of VT102 and ECMA-48 with private sequences of its own, compared there with the VT102 and xterm."
  - precedent: ADR-1501
  - precedent: ADR-1000
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "I'm unsure of the private layer concept working as intended. I think for now we can just leave agents.md and claude.md as they are, and later when we see a real reduction in open issues and vtium feels more stable, we'll revisit it then."
    via: the steering session, 2026-11-04
  - operator: aaronsb
    said: "let's adjust 'moving from kmscon' to 'moving from another DRM/KMS VT'."
    via: the steering session, 2026-11-04
  - operator: aaronsb
    said: "Outward sweep now"
    via: the steering session, 2026-11-04; the maintainer chose this option from the steering session's interview, and the option's label and its description (the docs, README, crate READMEs, code comments, renames and the fork-refs check now; AGENTS.md, CLAUDE.md, the provenance records, the C programs and docs/c-reference.md as they are until the revisit) are the steering session's wording
  - operator: aaronsb
    said: "replace, one review pass. (we're boot strapping our own goldens) - I assume they pass right now, so we're just calling them vtium's goldens now"
    via: the steering session, 2026-11-04
    covers: [the judge]
  - operator: aaronsb
    said: "do we drop it and add fbcon as a comparison target?"
    via: the steering session, 2026-11-04, on ADR-1000's c_reference column; a question, which the steering session answers with the lean this record drafts, pending the maintainer's confirmation
observable:
  - "`docs/scripts/fork-refs` exits 0 on the tree after the outward sweep, and exits 1 naming the file and line when a planted line in a crate README says `the C reference`"
  - "Every case is judged by one golden: tools/conformance/expected.tsv, tools/tier2/departures.tsv and tools/tier3/departures.tsv are gone, and `make conformance`, `make tier2` and `make tier3` pass"
  - "No crate README has a section named Departures, C behaviour kept, Beyond the C reference or C notes"
  - "The documentation site's menu has no entry for docs/c-reference.md, and `make docs` builds with no warning"
status: proposed
date: 2026-11-04
deciders:
  - aaronsb
related:
  - ADR-1000
  - ADR-1501
  - ADR-1502
---


# ADR-1503: vtium judges itself by its own goldens and its committed tree names no C fork

## Summary

- **Decided:** vtium's own goldens are the judge of every tier. Every golden copied from the C reference, the maintainer's forks of `kmscon` and `libtsm`, becomes vtium's golden under the same review rule as every other golden, and each case has one golden: the cases that pass today only because a list excuses their difference, about 90, get vtium's own output as their golden in golden commits batched by tier for one review pass, and the excuse lists go. The behaviour vtium implements is defined by the specifications it cites, ADR-1000's tables and its goldens, so the crate READMEs' departure lists go and a choice a reader needs is stated in the present tense as what vtium does and why. The outward sweep happens now: the docs, the README, the crate READMEs, code comments and strings, CONTRIBUTING.md, the Makefile, the tools, the renames that touch no provenance record, and a new check, `docs/scripts/fork-refs`, that keeps the committed tree from naming the fork again. AGENTS.md, CLAUDE.md, the three provenance records and every file they list, the C programs and docs/c-reference.md stay in the tree as they are until a later revisit, once the open issues have come down and vtium is more stable; docs/c-reference.md leaves the site's menu now. The README keeps a short history in the past tense, the licence files and ported files keep their copyright notices, and text about the released `kmscon` program on a user's machine stays. Existing decision records stay as history, with a pass over sentences that read as if the forks can be opened. ADR-1000 drops its `c_reference` column and gains one for the Linux kernel's own console, pending the maintainer's confirmation.
- **Trades away:** an outside judge. The copied goldens were drawn by a program vtium did not write; once they are vtium's, a regression the maintainer approves in a golden commit is no longer caught by anything independent. The departure lists' record of why each behaviour differs shrinks to the statements a reader still needs, and the rest lives in git history. The maintainer reviews three golden commits, one per tier, for about 90 cases (57 rows in tier 1, 4 in tier 2, 31 pairs in tier 3). A contributor loses the pointer from a comment to the C code it was written against. Until the revisit, the files that stay as they are still name the C reference, and an excused case keeps its copied golden beside vtium's, judging nothing.
- **One-way?** Expensive now, and one-way once the maintainer deletes the forks. Every committed change is in git and can be put back. The provenance checks need the forks, so after the forks are gone nobody can check a copied golden against its source again.
- **Probes:** *Confident (the judge):* where vtium deliberately draws or answers something other than a copied golden shows, the golden is replaced by vtium's output in a commit the maintainer reviews, and no list of departures stays beside it. *Not confident (ADR-1000's comparison column):* ADR-1000's tables lose their `c_reference` column, which recorded what the C reference did for each mode and feature, even though ADR-1000 is otherwise a record the maintainer called historical; the five columns for other terminals stay.
- **Inversion:** Between "keep naming the C reference as the judge until the forks are deleted" (the tree stays tied to code nobody outside can read) and "erase every trace of where vtium came from" (history, licence notices and the released `kmscon` user's path go too). The answer keeps the judge's values as vtium's own goldens, drops the name from everything an outside reader meets, keeps the history in the README, the records and git, and leaves the provenance records and the files they list in the tree, unchanged, until the revisit decides where they go.

## Context

vtium was built against the C reference: the maintainer's forks of the C terminal `kmscon` and its screen and parser library `libtsm`, frozen as the measure vtium had to meet. Its tests compared vtium with goldens, saved expected results, copied from the C reference with a record of where each came from (its provenance). A difference from a copied golden had to be a departure, a listed difference with its reason, and the crate READMEs list 246 of them. AGENTS.md tells every agent to answer each design question from the C reference's code first, and the code comments, docs and tools cite the C reference's files and functions on 3,915 lines in 349 files.

vtium now does more than the C reference did, and in places it draws better, so a copied golden can hold vtium back. The maintainer plans to delete the forks. A reader of the public repository cannot open them, and a list of "differences from kmscon" describes vtium by what another program did, which goes stale and breaks the rule that vtium is written about in the present tense. The maintainer and the steering session still use the forks while they exist, to check where a copy came from and as prior art.

Terms used below. A golden is a saved expected result that a tier compares vtium's output with. A provisional golden is one of vtium's goldens that the maintainer has not reviewed yet, and it judges nothing. A golden commit is a commit that changes goldens and nothing else, lists each value that moved and why, and that the maintainer reviews. An excuse list is a file that lets a case pass while vtium's output differs from its golden: tools/conformance/expected.tsv, tools/tier2/departures.tsv and tools/tier3/departures.tsv. The revisit is the later piece of work, once the open issues have come down and vtium is more stable, that decides what happens to the files that keep the forks' provenance (D6).

## Decision

### D1: vtium's goldens are the judge

Every tier judges vtium by vtium's goldens. Every golden copied from the C reference becomes vtium's golden. AGENTS.md's rule holds for all of them alike: the author of a change never rewrites the golden that judges it, and a golden changes only in a golden commit the maintainer reviews.

Every case passes today, but not every case passes because vtium produces what its copied golden shows. About 90 cases pass only because an excuse list covers their difference: the 57 rows of tools/conformance/expected.tsv, the 4 of tools/tier2/departures.tsv, and the 10 rows of tools/tier3/departures.tsv, which cover 31 variant and screen pairs. Calling those copied goldens vtium's as they stand would give vtium goldens that show output vtium does not produce. Each of these cases instead gets vtium's own output as its golden. The golden commits are batched one per tier, and each lists every case, what moved, and the specification or decision behind the difference, so the maintainer reviews all of them in one pass. Each tier's excuse list goes in the commit after that tier's golden commit.

The provenance records stay as they are until the revisit, and they and the checks that read them require every copied golden to stay byte for byte as it was copied. vtium's golden for an excused case therefore goes where vtium's own goldens already live: tools/conformance/expected/, which already holds vtium's output for these cases, tools/tier2/goldens/ and tools/tier3/goldens/. Each tier judges a case by vtium's golden where one exists and by the copied golden otherwise, as tier 3 already does for its re-baselined variants. The copied golden that vtium's golden replaces stays in place, judging nothing, until the revisit removes it. Until then, a later change that moves what vtium produces for a case still judged by a copied golden writes vtium's new golden into vtium's golden directory, in a golden commit, and leaves the copy alone.

The glyph and cell digests in vtium-font's and vtium-render's tests are vtium's goldens too. A deliberate change to what they cover writes the digest the failing test prints, in a golden commit.

Names in the tree that say "C" or "reference" for these files are renamed to say what they are now. The renames that would touch a provenance record or a file one lists wait for the revisit:

| Now | Becomes | When |
|---|---|---|
| docs/c-reference.md's entry in zensical.toml's nav | removed; the page stays in the repository | now |
| `c_ref` names in crates/vtium-render/tests/memory.rs | names for what the values are, such as `budget` | now |
| ten of tier 3's guest configurations: kmscon.conf, kmscon-font32.conf, kmscon-frit.conf, kmscon-gl.conf, kmscon-images.conf, kmscon-nodisplay.conf, kmscon-power.conf, kmscon-showcase.conf, kmscon-small-8x16.conf, kmscon-small-8x8.conf | vtium.conf, vtium-font32.conf and so on | now |
| ADR-1000's `c_reference` column | removed, with a `linux` column added (D2) | once the maintainer confirms |
| guest/kmscon-minimal.conf and guest/kmscon-minimal-psf.conf | vtium-minimal.conf and vtium-minimal-psf.conf | the revisit: tools/tier3/provenance.tsv lists them |
| tier 3's benchmark names `kmscon_cpu_ms`, `kmscon_cpu_pct`, `kmscon_cpu_ms_per_mpv_frame`, `kmscon_cpu_ms_per_tmux_sixel`, and `kmscon_pid` and `kmscon_ms` in guest/screens/acceptance.inc | `console_cpu_ms` and so on, `console_pid`, `console_ms` | the revisit: the listed screens mpv-sixel.sh and tmux-grid.inc read them |
| tools/tier3/c-goldens/ | merged into tools/tier3/goldens/, one golden per case | the revisit |
| tools/conformance/expected/ | merged into tools/conformance/corpus/golden and golden-frame | the revisit |
| tools/tier2/reference/goldens/*.ppm | tools/tier2/goldens/ | the revisit |
| `c_goldens` and `c-goldens` names in tools/tier2 and tools/tier3/test_t3.py | names for what the values are | the revisit, with the directories and lists they name |
| crates/vtium-font/tests/reference/ and crates/vtium-render/tests/reference/, with tests/reference.rs in each crate | tests/digests/ and tests/digests.rs | the revisit: each directory holds a C program |
| tools/tier2/reference/test_pixels.c, tools/tier2/c-goldens.list, tools/tier2/src/cref.rs, the three provenance.tsv files, the make targets `conformance-provenance`, `tier2-provenance` and `tier3-provenance` with their LIBTSM and KMSCON variables and `provenance` subcommands, and docs/c-reference.md | moved out of the repository or deleted, as the revisit decides | the revisit |

At the revisit, tier 3's guest provisioning also stops installing the C reference's runtime dependencies and stops linking autovt@.service to kmsconvt@.service, since guest/user-data.in is a listed copy. That changes the base image's inputs, so each checkout builds a new base image once, and the commit that does it runs every tier 3 variant to show that no golden moved.

### D2: what defines vtium's behaviour

The behaviour vtium implements is defined by the specifications vtium cites, ADR-1000's tables, and vtium's goldens. No committed text defines vtium by comparison with another program.

The crate READMEs' `Departures`, `C behaviour kept`, `Beyond the C reference` and `C notes` sections go, as do docs/architecture.md's `Beyond the C reference` and docs/vt-review.md's `Departures from the C reference's review`. The excuse lists cite README entries, so the README sections go after the golden commits and the excuse lists (D1). The author of the sweep reads each item and does one of three things with it. An item that a specification and a test already say is dropped. An item a reader needs, such as a bound, a refusal, or a choice between two readings of a specification, becomes a present-tense statement in the crate README of what vtium does and why, with the specification it cites. An item that is about a mode or feature row becomes a sentence in that row's note column in ADR-1000. A security fix that the C reference lacked is stated as what vtium does, with the specification it follows.

ADR-1000's tables drop the `c_reference` column and gain a column named `linux` for the Linux kernel's own console: the escape and control sequence handling of the kernel's VT layer, which every Linux machine has, which fbcon draws, and which programs meet under TERM=linux. Like the columns for xterm, foot, Ghostty, WezTerm and kitty, it records published documentation: its values come from console_codes(4), in the words those columns use. This record does not fill the column. One edit to ADR-1000, a branch of its own in the sweep, drops `c_reference`, adds `linux` and fills it, with a dated note in ADR-1000 saying what changed and why. A row whose behaviour console_codes(4) leaves unclear reads `unverified`, and those rows go into a guide for the maintainer's VT review, because an agent on this machine never opens a /dev/tty<N> console and so cannot try the kernel console itself. This change waits for the maintainer's confirmation (open question 1).

### D3: committed text describes vtium and cites no C code

Code comments, strings in code (such as the bounds table's citations in vtium-font and test failure messages), crate READMEs, docs pages, CONTRIBUTING.md, the Makefile, configuration file comments and the tools describe vtium in the present tense and do not cite the C reference, its files, its functions or its issues. A comment that explained vtium's code by the C function it followed explains what vtium's code does, or goes.

These files stay as they are until the revisit, and the sweep does not edit them: AGENTS.md and CLAUDE.md (D6); the three provenance records and every file they list, which includes tier 1's corpus, tier 3's listed screen scripts and guest files, and crates/vtium-term/tests/palettes.c.txt; tools/tier2/c-goldens.list and tools/tier2/src/cref.rs; the two digest directories with their C programs and READMEs; and docs/c-reference.md. A file the sweep does edit that has to describe something on this list, such as the Makefile's provenance targets or a tool's README describing its copied goldens, describes it in the present tense without naming the fork, for example "the goldens copied at the commits provenance.tsv names", and leaves the history to the provenance record.

These stay, because they serve a reader who has no fork:

- The README's `Provenance` section keeps a short history in the past tense: vtium began as a Rust program derived from `kmscon` and `libtsm`, whose authors COPYING lists.
- COPYING and LICENSE keep kmscon's and libtsm's copyright notices and author lists, and a file ported from them keeps its notice (vtium-font's unifont.rs and vtium-parse's parser.rs and utf8.rs today), as the MIT licence requires. Their prose follows the tense rule.
- Text about the released `kmscon` program that a user may have installed: docs/install.md's `Sharing VTs with kmscon`, `vtium console`'s handling of kmsconvt@.service and the code and comments behind it, vtium reading `kmscon`'s configuration file, and vtium's answer to TERM=kmscon from crates/vtium/install/kmscon.ti. Each describes another program on the user's machine in the present tense and says nothing about how vtium was judged. Their comments drop the wording that ties them to the fork, such as "the C reference's entry, whose probes judge it".
- docs/install.md's `Moving from kmscon` becomes `Moving from another DRM/KMS VT`, rewritten to cover moving to vtium from any other terminal that runs on a VT through DRM/KMS, with `kmscon` as one example among others. The docs sweep does the rewrite.
- Text that a golden shows, such as a tier 3 screen's caption when it is drawn on the screen, changes only in a golden commit.

### D4: the decision records

The existing decision records stay as the history of what was decided and why. The tense pass changes only the sentences that read as if a reader could open the forks today, such as an instruction to read the C reference's code at a path or a statement of what the C reference does, and turns each into a past-tense statement of what was found. The frontmatter's `basis`, `considered` and `concern` entries stay as written, as the record convention requires. Existing `C notes` and `Where vtium departs from the C reference` sections stay, since they record what was found when the decision was made.

A new record has no `C notes` section and no departures section. It argues from specifications, measurements and vtium's own needs. It may have a `Prior art` section that names other programs, the C reference among them, in the past tense, as history of what others did.

adr.yaml's comments follow D3. Its `cite.exclude` entry for tools/tier3/guest/screens/ stays until the revisit, since the listed screen scripts cite the C reference's own records and stay as they are.

### D5: CONTRIBUTING.md defines the judge before the builder

CONTRIBUTING.md first says what judges a change: the tiers from unit tests to tier 3 and the maintainer's VT review, vtium's goldens and the golden commit rule, and ADR-1000's rows with the tests tagged to them. Only then does it say how to build a change. A contribution brings its judge before its code: a test that fails before the change, a new golden marked provisional, or the ADR-1000 row it serves. A contribution that changes what vtium draws or answers brings the golden commit that records it.

### D6: AGENTS.md and CLAUDE.md stay as they are for now

> Edited 2026-11-04: the maintainer deferred the private layer (the first `considered` entry). The earlier text moved the rules for using the forks into a CLAUDE.md in the directory that holds the checkout, and the provenance records, the C programs, docs/c-reference.md and the provenance checks into a private directory beside it; git keeps that text.

AGENTS.md and the committed CLAUDE.md stay as they are, rules about the C reference included, and the sweep leaves them alone. The maintainer and the steering session keep using the forks for provenance and as prior art as they do today. The revisit, once the open issues have come down and vtium is more stable, decides whether a private layer keeps what needs the forks in files that are never committed, and where the files that D3 leaves as they are go: moved there, or deleted with git keeping them.

For that revisit, these facts about Claude Code stand (the upstream entry in the basis):

- Claude Code loads a CLAUDE.md from the working directory and from every directory above it, so a CLAUDE.md in the directory that holds the checkout and its worktrees reaches every worktree there.
- A gitignored file inside the repository, such as CLAUDE.local.md, exists only in the worktree where it was created.
- By default Claude Code reads AGENTS.md only when no CLAUDE.md, .claude/CLAUDE.md or CLAUDE.local.md is in the working directory or above it. A CLAUDE.md in a parent directory therefore stops AGENTS.md loading by itself, and the committed CLAUDE.md's import of AGENTS.md is what keeps it loading.
- The project's own ways, the guidance files under the checkout's uncommitted .claude/ directory, teach the rule of listing departures from the C reference and change when the private layer does.

### D7: a check holds the break

`docs/scripts/fork-refs` fails when a committed file names the fork: the phrases "C reference", "C fork", "kmscon fork" and "libtsm fork", the repositories `aaronsb/kmscon` and `aaronsb/libtsm`, and the fork issue forms `kmscon#N` and `libtsm#N`. It leaves the plain names `kmscon` and `libtsm` to review, since D3 keeps them where they name the released program or the licence.

It skips docs/architecture/, which D4 governs, and COPYING and LICENSE. Until the revisit it also skips the files D3 leaves as they are: AGENTS.md, CLAUDE.md, the three provenance records and every path they list, tools/tier2/c-goldens.list, tools/tier2/src/cref.rs, crates/vtium-font/tests/reference/, crates/vtium-render/tests/reference/ and docs/c-reference.md. The check keeps that skip list in one place, with a note on each entry saying it is there until the revisit, and it reads the listed paths from the provenance records themselves, so the list shrinks as the records do.

It joins AGENTS.md's gate line after `docs/scripts/issue-refs` and CI's checks when the revisit edits AGENTS.md; until then CI and `make gates` run it. It lands with the last branch of the sweep, seen to fail on the tree before the sweep and on a planted line, and to pass after.

## Consequences

### Positive

- A reader of the docs, the READMEs and the code meets one judge, vtium's goldens, and one description of each behaviour, in the present tense, with no program they cannot open.
- A case is judged by one golden, so a reviewer reads one expected result instead of a copied golden, a list of rectangles where vtium may differ, and a README entry that explains it.
- The gate line and CI need nothing beside the vtium checkout, as before.
- The check in D7 stops the break from wearing away as agents trained on the old text keep writing "the C reference".

### Negative

- No independent program judges vtium any more. A wrong golden commit that the maintainer approves goes unnoticed until a person sees the screen, which is why each golden commit lists what moved.
- The golden commits for the excused cases change values, not only labels: the maintainer reviews about 90 cases where vtium's output differs from what the copied golden shows.
- Until the revisit, the tree holds the files D3 leaves as they are, which still name the C reference, and an excused case's copied golden, which judges nothing. fork-refs carries a skip list for them.
- The sweep touches about 300 files across every crate and tool, in several branches, and conflicts with any work open on the same files at the time.
- Most of the 246 departure items carried reasoning. Those that are dropped keep their reasoning only in git history, and a later reader who wants it has to find the commit.
- At the revisit, each checkout builds a new tier 3 base image once, about 330 MiB of packages and a file of about 1.1 GiB.
- Once the forks are deleted, nobody can check where a copied golden came from.

### Neutral

- GitHub issues already filed stay as they are. New issues, PR bodies and commit messages follow the same rule as committed text.
- The SPDX and REUSE header sweep, which adds a licence header to every file, runs in the same quiet window as a separate commit series, so the two sweeps never edit the same lines in one commit. The files D3 leaves as they are include copies whose bytes the provenance checks compare, so the header sweep leaves those alone until the revisit too.

### The order of the work

1. This record, read by the maintainer before the sweep starts.
2. The golden commits, one per tier, for the maintainer's single review pass, each followed by the commit that removes that tier's excuse list and lets the tier judge a case by vtium's golden where one exists.
3. The outward sweep, by area, each its own branch with its own review: docs and the README, with docs/c-reference.md leaving the site's menu and install.md's section retitled; CONTRIBUTING.md; the crate READMEs, after step 2; code comments and strings; the tools, the Makefile and the renames marked "now"; the decision records' tense pass and adr.yaml's comments; and, once the maintainer confirms, the ADR-1000 edit that drops `c_reference` and adds `linux`. docs/scripts/fork-refs lands with the last branch.
4. Later, the revisit: D6's private layer, the renames marked "the revisit", tier 3's provisioning change, and the removal of fork-refs' skip list.

## Open questions

1. **ADR-1000's `c_reference` column, and the kernel console.** Drafted in D2 and waiting for the maintainer's confirmation: drop `c_reference` and add a `linux` column filled from console_codes(4), with unclear rows left `unverified` for the VT review. The tables are kept current as the target set, the old column's values cite probes and files of the fork, and git keeps it. The five columns for other terminals stay.
2. **The TERM=kmscon identity.** Lean: it stays, as compatibility with the `kmscon` terminfo entry that systems with `kmscon` installed set, with its comments rewritten to describe vtium's behaviour under that name and to drop the wording about the C reference's probes.
3. **docs/why-vtium.md's `Beside kmscon`.** Lean: it stays as a comparison with the released `kmscon` a user might choose instead, in the present tense, and loses the sentences about the fork and the C reference's tests. This sits close to D2's rule against defining vtium by another program, so the maintainer may prefer to cut the comparison to one sentence of history.
4. **The private directory under version control.** Deferred with the private layer (D6). The lean, for the revisit: a local git repository with no remote, so its history survives, and the maintainer adds a private remote if one is wanted.
5. **The checks of the copies before a run.** They stay until the revisit, since the copies stay. Lean for the revisit: they go with the provenance records, and no golden gets a committed checksum list, since git already shows any change to a golden.

## Alternatives Considered

- **Keep the C reference as the named judge until the forks are deleted.** It leaves the work for later, but the tree keeps pointing at code an outside reader cannot open, the departure lists keep describing vtium by another program, and the maintainer called that program an inferior renderer that holds vtium back. Rejected.
- **Call the copied goldens vtium's as they stand and keep the excuse lists.** It needs no golden review, but about 90 of vtium's goldens would show output vtium does not produce, and every one of those cases would still need a list beside it. Rejected: the maintainer chose to replace them.
- **Overwrite the copied goldens in place with vtium's output.** It gives one golden per file at once, but it breaks the provenance records' byte-for-byte checks, which stay until the revisit. Rejected for now; at the revisit the directories merge.
- **Rename the C reference to "the reference" and keep the departure lists.** It keeps two judges per case and a list of where vtium may differ from the one it does not follow. Rejected for one golden per case.
- **Keep the private rules in a gitignored file inside the repository, such as CLAUDE.local.md or a file under .claude/.** Each worktree is a fresh checkout, and Claude Code's documentation says a gitignored CLAUDE.local.md exists only in the worktree where it was created, so worktree agents would not see it. A CLAUDE.local.md also counts as a CLAUDE.md and stops Claude Code from reading AGENTS.md under its default setting. Rejected, and kept as background for the revisit.
- **Import the private rules from the home directory in the committed CLAUDE.md.** Claude Code's documentation suggests it for sharing private instructions across worktrees, but the committed file would then name a private path that every other contributor's agent tries to import and lacks. Rejected.
- **Rewrite git history to remove the C reference.** It would break every link to an earlier commit and remove the history the maintainer wants kept, and the licence requires the notices anyway. Rejected.
- **Leave the decision records entirely untouched.** It keeps a few sentences that tell a reader to open the forks, which the maintainer asked to change. Rejected for the narrow tense pass in D4.
