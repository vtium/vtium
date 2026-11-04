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
  - precedent: ADR-1501
  - precedent: ADR-1000
agent:
  name: Claude
  model: claude-opus-5-5
observable:
  - "`docs/scripts/fork-refs` exits 0 on the tree after the sweep, and exits 1 naming the file and line when a planted line in a crate README says `the C reference`"
  - "`make help` lists no target that reads a fork checkout, and `git grep -nE 'LIBTSM|KMSCON'` finds nothing in the Makefile or the tools"
  - "Each tier judges every case by one golden: tools/conformance/expected.tsv, tools/tier2/departures.tsv and tools/tier3/departures.tsv are gone, and `make conformance`, `make tier2` and `make tier3` pass"
  - "No crate README has a section named Departures, C behaviour kept, Beyond the C reference or C notes"
  - "A worktree cut from integration in the directory that holds the checkout loads both the private CLAUDE.md and AGENTS.md in a fresh Claude Code session"
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

- **Decided:** vtium's own goldens are the judge of every tier. The goldens that were copied from the C reference, the maintainer's forks of `kmscon` and `libtsm`, become vtium's goldens under the same review rule as every other golden, and each case has one golden: where vtium means to differ, vtium's output replaces the golden in a reviewed commit of its own, and the lists that excused differences go. The behaviour vtium implements is defined by the specifications it cites, ADR-1000's tables and its goldens, so the crate READMEs' departure lists go and a choice a reader needs is stated in the present tense as what vtium does and why. Committed code, comments, READMEs, docs, AGENTS.md, CONTRIBUTING.md, the Makefile and the tools stop citing the C reference; the README keeps a short history in the past tense, the licence files and ported files keep their copyright notices, and text about the released `kmscon` program on a user's machine stays. Existing decision records stay as history, with a pass over sentences that read as if the forks can be opened. The forks, the provenance records and the checks that read them move to a private layer that is not committed: a CLAUDE.md in the directory that holds the checkout and its worktrees, and a private directory beside it. A new check, `docs/scripts/fork-refs`, keeps the committed tree from naming the fork again.
- **Trades away:** an outside judge. The copied goldens were drawn by a program vtium did not write; once they are vtium's, a regression the maintainer approves in a golden commit is no longer caught by anything independent. The departure lists' record of why each behaviour differs shrinks to the statements a reader still needs, and the rest lives in git history. The maintainer reviews a series of golden commits, one per tier, for about 90 cases (57 rows in tier 1, 4 in tier 2, 31 pairs in tier 3). A contributor loses the pointer from a comment to the C code it was written against.
- **One-way?** Expensive now, and one-way once the maintainer deletes the forks. Every committed change is in git and can be put back. The provenance checks need the forks, so after the forks are gone nobody can check a copied golden against its source again, and the private provenance records become notes that cannot be verified.
- **Probes:** *Confident (the judge):* where vtium deliberately draws or answers something other than a copied golden shows, the golden is replaced by vtium's output in a commit the maintainer reviews, and no list of departures stays beside it. *Not confident (ADR-1000's comparison column):* ADR-1000's tables lose their `c_reference` column, which recorded what the C reference did for each mode and feature, even though ADR-1000 is otherwise a record the maintainer called historical; the five columns for other terminals stay.
- **Inversion:** Between "keep naming the C reference as the judge until the forks are deleted" (the tree stays tied to code nobody outside can read) and "erase every trace of where vtium came from" (history, licence notices and the released `kmscon` user's path go too). The answer keeps the judge's values as vtium's own goldens, drops the name, keeps the history in the README, the records and git, and keeps the provenance where the maintainer and the steering session can still read it.

## Context

vtium was built against the C reference: the maintainer's forks of the C terminal `kmscon` and its screen and parser library `libtsm`, frozen as the measure vtium had to meet. Its tests compared vtium with goldens, saved expected results, copied from the C reference with a record of where each came from (its provenance). A difference from a copied golden had to be a departure, a listed difference with its reason, and the crate READMEs list 246 of them. AGENTS.md tells every agent to answer each design question from the C reference's code first, and the code comments, docs and tools cite the C reference's files and functions on 3,915 lines in 349 files.

vtium now does more than the C reference did, and in places it draws better, so a copied golden can hold vtium back. The maintainer plans to delete the forks. A reader of the public repository cannot open them, and a list of "differences from kmscon" describes vtium by what another program did, which goes stale and breaks the rule that vtium is written about in the present tense. The maintainer and the steering session still use the forks while they exist, to check where a copy came from and as prior art.

Terms used below. A golden is a saved expected result that a tier compares vtium's output with. A provisional golden is one of vtium's goldens that the maintainer has not reviewed yet, and it judges nothing. A golden commit is a commit that changes goldens and nothing else, lists each value that moved and why, and that the maintainer reviews. The private layer is the set of files the maintainer and the steering session use that are never committed to vtium.

## Decision

### D1: vtium's goldens are the judge

Every tier judges vtium by vtium's goldens. The goldens copied from the C reference become vtium's goldens. AGENTS.md's rule holds for all of them alike: the author of a change never rewrites the golden that judges it, and a golden changes only in a golden commit the maintainer reviews.

Each case has one golden. Where vtium deliberately differs from a copied golden, a golden commit replaces that golden with vtium's output, says what moved, and cites the specification or decision behind the difference. The lists that excused a difference from a copied golden then go: tools/conformance/expected.tsv, tools/tier2/departures.tsv and tools/tier3/departures.tsv. vtium's goldens in tools/conformance/expected/ move into the corpus' golden directories in place of the copies they replaced, and in tier 3 the copied golden that a golden in goldens/ replaced leaves the tree. A tier 3 case that is still judged by a copied golden keeps it, now as vtium's own.

The checks inside the tree that compare the copies with their provenance rows before a run go with the provenance records (D6). vtium's goldens are guarded by the golden commit rule and by git, as vtium's other goldens are today. tier 2 stops reading the C program test_pixels.c: each screen's bytes live in tools/tier2/src/screens.rs as vtium's own, and tools/tier2/src/cref.rs goes. tools/tier2/pending.tsv becomes the list of screens tier 2 still owes, each named for what it shows, without the name of a C golden. The glyph and cell digests in vtium-font's and vtium-render's tests are vtium's goldens; a deliberate change to what they cover takes the digest the failing test prints, in a golden commit.

Names in the tree that say "C" or "reference" for these files are renamed to say what they are now:

| Now | Becomes |
|---|---|
| tools/tier3/c-goldens/ | merged into tools/tier3/goldens/, one golden per case |
| tools/tier2/reference/goldens/*.ppm | tools/tier2/goldens/ |
| tools/tier2/reference/test_pixels.c, tools/tier2/c-goldens.list, tools/tier2/src/cref.rs | the private layer; cref.rs's check goes with them |
| tools/conformance/expected/ | merged into tools/conformance/corpus/golden and golden-frame |
| tools/conformance/provenance.tsv, tools/tier2/provenance.tsv, tools/tier3/provenance.tsv | the private layer |
| crates/vtium-font/tests/reference/, tests/reference.rs | crates/vtium-font/tests/digests/, tests/digests.rs; refdigest.c to the private layer |
| crates/vtium-render/tests/reference/, tests/reference.rs | crates/vtium-render/tests/digests/, tests/digests.rs; refcells.c to the private layer |
| `c_ref` and `c_goldens` names in crates/vtium-render/tests/memory.rs, tools/tier2 and tools/tier3/test_t3.py | names for what the values are, such as `budget` or `golden` |
| tools/tier3/guest/kmscon*.conf (twelve files) | tools/tier3/guest/vtium*.conf, as `vtium.conf`, `vtium-gl.conf`, `vtium-minimal.conf` |
| tier 3's benchmark keys `kmscon_cpu_ms`, `kmscon_cpu_pct`, `kmscon_cpu_ms_per_mpv_frame`, `kmscon_cpu_ms_per_tmux_sixel`, and `kmscon_pid` and `kmscon_ms` in guest/screens/acceptance.inc | `console_cpu_ms`, `console_cpu_pct`, `console_cpu_ms_per_mpv_frame`, `console_cpu_ms_per_tmux_sixel`, `console_pid`, `console_ms` |
| make targets `conformance-provenance`, `tier2-provenance`, `tier3-provenance`, the LIBTSM and KMSCON variables, and the `provenance` subcommands of conformance, tier2 and t3.py | the private layer |
| ADR-1000's `c_reference` column in ADR-1000-modes.tsv and ADR-1000-features.tsv | removed (open question 1) |
| docs/c-reference.md and its entry in zensical.toml's nav | the private layer |

tier 3's guest provisioning stops installing the C reference's runtime dependencies and stops linking autovt@.service to kmsconvt@.service. That changes the base image's inputs, so each checkout builds a new base image once, and the commit that does it runs every tier 3 variant to show that no golden moved.

### D2: what defines vtium's behaviour

The behaviour vtium implements is defined by the specifications vtium cites, ADR-1000's tables, and vtium's goldens. No committed text defines vtium by comparison with another program.

The crate READMEs' `Departures`, `C behaviour kept`, `Beyond the C reference` and `C notes` sections go, as do docs/architecture.md's `Beyond the C reference` and docs/vt-review.md's `Departures from the C reference's review`. The author of the sweep reads each item and does one of three things with it. An item that a specification and a test already say is dropped. An item a reader needs, such as a bound, a refusal, or a choice between two readings of a specification, becomes a present-tense statement in the crate README of what vtium does and why, with the specification it cites. An item that is about a mode or feature row becomes a sentence in that row's note column in ADR-1000. A security fix that the C reference lacked is stated as what vtium does, with the specification it follows.

### D3: committed text describes vtium and cites no C code

Code comments, strings in code (such as the bounds table's citations in vtium-font and test failure messages), crate READMEs, docs pages, CONTRIBUTING.md, AGENTS.md, the Makefile, configuration file comments and the tools describe vtium in the present tense and do not cite the C reference, its files, its functions or its issues. A comment that explained vtium's code by the C function it followed explains what vtium's code does, or goes.

These stay, because they serve a reader who has no fork:

- The README's `Provenance` section keeps a short history in the past tense: vtium began as a Rust program derived from `kmscon` and `libtsm`, whose authors COPYING lists.
- COPYING and LICENSE keep kmscon's and libtsm's copyright notices and author lists, and a file ported from them keeps its notice (vtium-font's unifont.rs and vtium-parse's parser.rs and utf8.rs today), as the MIT licence requires. Their prose follows the tense rule.
- Text about the released `kmscon` program that a user may have installed: docs/install.md's `Moving from kmscon` and `Sharing VTs with kmscon`, `vtium console`'s handling of kmsconvt@.service and the code and comments behind it, vtium reading `kmscon`'s configuration file, and vtium's answer to TERM=kmscon from crates/vtium/install/kmscon.ti. Each describes another program on the user's machine in the present tense and says nothing about how vtium was judged. Their comments drop the wording that ties them to the fork, such as "the C reference's entry, whose probes judge it".
- Text that a golden shows, such as a tier 3 screen's caption when it is drawn on the screen, changes only in a golden commit. The comments in a screen script that no golden shows follow this decision like any other comment.

### D4: the decision records

The existing decision records stay as the history of what was decided and why. The tense pass changes only the sentences that read as if a reader could open the forks today, such as an instruction to read the C reference's code at a path or a statement of what the C reference does, and turns each into a past-tense statement of what was found. The frontmatter's `basis`, `considered` and `concern` entries stay as written, as the record convention requires. Existing `C notes` and `Where vtium departs from the C reference` sections stay, since they record what was found when the decision was made.

A new record has no `C notes` section and no departures section. It argues from specifications, measurements and vtium's own needs. It may have a `Prior art` section that names other programs, the C reference among them, in the past tense, as history of what others did.

adr.yaml's comments follow D3. Its `cite.exclude` entry for tools/tier3/guest/screens/ goes once the screen scripts no longer cite the C reference's own records.

### D5: CONTRIBUTING.md defines the judge before the builder

CONTRIBUTING.md first says what judges a change: the tiers from unit tests to tier 3 and the maintainer's VT review, vtium's goldens and the golden commit rule, and ADR-1000's rows with the tests tagged to them. Only then does it say how to build a change. A contribution brings its judge before its code: a test that fails before the change, a new golden marked provisional, or the ADR-1000 row it serves. A contribution that changes what vtium draws or answers brings the golden commit that records it.

### D6: the private layer

The maintainer and the steering session keep using the forks for provenance and as prior art while they exist. Everything that needs the forks lives in files that are not committed to vtium:

- A CLAUDE.md in the directory that holds the vtium checkout and its worktrees holds the private rules: how to read the forks, that the steering session answers a design question from the C reference's logic before it designs, and where the private documents are. Claude Code reads a CLAUDE.md in every directory above the one it starts in, so every worktree there sees it.
- A private directory beside the checkout holds the private documents: the C reference map that docs/c-reference.md holds today, the three provenance records, tier 2's list of the C reference's goldens, the C programs that made digests and screens, and the provenance checks, as a small tool of their own. It is a git repository of its own with no public remote (open question 4).
- The committed CLAUDE.md stays a one-line import of AGENTS.md. Under Claude Code's default, a CLAUDE.md in a parent directory stops Claude Code from reading AGENTS.md by itself, and the import keeps AGENTS.md loading for every contributor, with or without the private layer.
- No gate, CI job, committed tool or committed document reads the private layer or the forks. A contributor never needs it.
- The project's own ways (the guidance files under the checkout's uncommitted .claude/ directory), which teach the old rule of listing departures from the C reference, change with the private layer.

When the maintainer deletes the forks, the private provenance checks stop working, and the private records stay as notes.

### D7: a check holds the break

`docs/scripts/fork-refs` fails when a committed file names the fork: the phrases "C reference", "C fork", "kmscon fork" and "libtsm fork", the repositories `aaronsb/kmscon` and `aaronsb/libtsm`, and the fork issue forms `kmscon#N` and `libtsm#N`. It skips docs/architecture/, which D4 governs, and COPYING and LICENSE. It leaves the plain names `kmscon` and `libtsm` to review, since D3 keeps them where they name the released program or the licence. It joins AGENTS.md's gate line after `docs/scripts/issue-refs` and CI's checks, and it lands with the last branch of the sweep, seen to fail on the tree before the sweep and on a planted line, and to pass after.

## Consequences

### Positive

- A reader of the repository meets one judge, vtium's goldens, and one description of each behaviour, in the present tense, with no program they cannot open.
- A case has one golden, so a reviewer reads one expected result instead of a copied golden, a list of rectangles where vtium may differ, and a README entry that explains it.
- The gate line and CI need nothing beside the vtium checkout, as before, and the Makefile loses the targets that would break when the forks are deleted.
- The check in D7 stops the break from wearing away as agents trained on the old text keep writing "the C reference".

### Negative

- No independent program judges vtium any more. A wrong golden commit that the maintainer approves goes unnoticed until a person sees the screen, which is why each golden commit lists what moved.
- The sweep touches about 350 files across every crate and tool, in several branches, and conflicts with any work open on the same files at the time.
- Most of the 246 departure items carried reasoning. Those that are dropped keep their reasoning only in git history, and a later reader who wants it has to find the commit.
- Each checkout builds a new tier 3 base image once, about 330 MiB of packages and a file of about 1.1 GiB.
- Once the forks are deleted, nobody can check where a copied golden came from.

### Neutral

- GitHub issues already filed stay as they are. New issues, PR bodies and commit messages follow the same rule as committed text.
- The SPDX and REUSE header sweep, which adds a licence header to every file, runs in the same quiet window as a separate commit series, so the two sweeps never edit the same lines in one commit.

### The order of the work

1. This record, read by the maintainer before the sweep starts.
2. The private layer: the parent-directory CLAUDE.md, the private directory with the C reference map, the provenance records, the C programs and the provenance checks, and the project's ways updated. It comes before the sweep so that nothing the steering session uses is lost when the sweep removes it from the tree.
3. The sweep, by area, each its own branch with its own review: docs and the README; CONTRIBUTING.md and AGENTS.md; the crate READMEs; code comments and strings; the tools, the renames and the Makefile, with the golden commits for each tier; the decision records' tense pass and adr.yaml. The tools branch lands its golden commits before the commit that removes the departure lists, and docs/scripts/fork-refs lands with the last branch.

## Open questions

1. **ADR-1000's `c_reference` column.** Lean: remove it from both tables. The tables are kept current as the target set, the column's values cite probes and files of the fork, and git keeps it. ADR-1000's prose that explains the column gets a dated note saying it was removed and why. The five columns for other terminals stay, since they cite published documentation anyone can read.
2. **The TERM=kmscon identity.** Lean: it stays, as compatibility with the `kmscon` terminfo entry that systems with `kmscon` installed set, with its comments rewritten to describe vtium's behaviour under that name and to drop the wording about the C reference's probes.
3. **docs/why-vtium.md's `Beside kmscon`.** Lean: it stays as a comparison with the released `kmscon` a user might choose instead, in the present tense, and loses the sentences about the fork and the C reference's tests. This sits close to D2's rule against defining vtium by another program, so the maintainer may prefer to cut the comparison to one sentence of history.
4. **The private directory under version control.** Lean: a local git repository with no remote, so its history survives, and the maintainer adds a private remote if one is wanted.
5. **The checks of the copies before a run.** Lean: they go with the provenance records, and no golden gets a committed checksum list, since git already shows any change to a golden. A checksum list for every golden is the alternative, if the maintainer wants a tier to refuse to run on an edited golden.

## Alternatives Considered

- **Keep the C reference as the named judge until the forks are deleted.** It leaves the work for later, but the tree keeps pointing at code an outside reader cannot open, the departure lists keep describing vtium by another program, and the maintainer called that program an inferior renderer that holds vtium back. Rejected.
- **Rename the C reference to "the reference" and keep the departure lists.** It keeps two judges per case and a list of where vtium may differ from the one it does not follow. Rejected for one golden per case.
- **Keep the private rules in a gitignored file inside the repository, such as CLAUDE.local.md or a file under .claude/.** Each worktree is a fresh checkout, and Claude Code's documentation says a gitignored CLAUDE.local.md exists only in the worktree where it was created, so worktree agents would not see it. A CLAUDE.local.md also counts as a CLAUDE.md and stops Claude Code from reading AGENTS.md under its default setting. Rejected.
- **Import the private rules from the home directory in the committed CLAUDE.md.** Claude Code's documentation suggests it for sharing private instructions across worktrees, but the committed file would then name a private path that every other contributor's agent tries to import and lacks. Rejected.
- **Keep the provenance checks committed, as optional targets.** They read the forks at default paths beside the checkout, so they fail for every contributor and stop working for everyone once the forks are deleted. Rejected.
- **Rewrite git history to remove the C reference.** It would break every link to an earlier commit and remove the history the maintainer wants kept, and the licence requires the notices anyway. Rejected.
- **Leave the decision records entirely untouched.** It keeps a few sentences that tell a reader to open the forks, which the maintainer asked to change. Rejected for the narrow tense pass in D4.
