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
  - evidence: "measured 2026-11-04 on integration at 4667b6c2: ten crate READMEs carry a `## Departures` section, 246 bold-titled items in all (vtium-term 67, vtium 65, vtium-platform 42, vtium-image 20, vtium-render 15, vtium-config 13, vtium-font 10, vtium-parse 9, vtium-image-helper 3, vtium-settings 2); three carry `## C behaviour kept`, three `## Beyond the C reference`, the vtium crate's README a `## C notes` section, and vtium-term's README a `## Golden differences` section; docs/architecture.md has a `## Beyond the C reference` section and docs/vt-review.md a `## Departures from the C reference's review`. Thirteen of the 22 decision records carry a `C notes` section, and ten a section headed exactly `## Where vtium departs from the C reference` (fifteen carry a heading about departures of some form). A search of the record bodies for the C reference named with a present-tense verb finds 14 lines; the lines that point at the forks' checkouts are all in frontmatter basis entries."
  - evidence: "measured 2026-11-04 on integration at 4667b6c2: the goldens copied from the C reference are tier 1's corpus goldens (tools/conformance/corpus/golden, 164 files, and golden-frame) with the capability probes, tier 2's two PPMs in tools/tier2/reference/goldens/, and tier 3's 223 files in tools/tier3/c-goldens/ across seven variants. vtium's own goldens are tools/conformance/expected/ (49 screen files and the frame view), tools/tier2/goldens/ (86 files) and tools/tier3/goldens/ (158 files). The lists that excuse a difference from a copied golden are tools/conformance/expected.tsv (57 rows, every one of kind departure), tools/tier2/departures.tsv (4 rows) and tools/tier3/departures.tsv (10 rows covering 31 variant and screen pairs). tools/tier2/c-goldens.list names the 36 goldens of the C reference's tier 2, and tools/tier2/pending.tsv the 34 that no screen draws yet. Three C programs sit in the tree as the record of how digests and screens were made: crates/vtium-font/tests/reference/refdigest.c, crates/vtium-render/tests/reference/refcells.c and tools/tier2/reference/test_pixels.c; tools/tier2/src/cref.rs reads test_pixels.c on every tier 2 run to check that each ported screen carries its bytes."
  - evidence: "measured 2026-11-04 on integration at 4667b6c2: three make targets read the fork checkouts, `conformance-provenance`, `tier2-provenance` and `tier3-provenance`, through the variables LIBTSM and KMSCON, which default to the fork checkouts beside the vtium checkout; they run the subcommands `conformance provenance`, `tier2 provenance` and `t3.py provenance`, and tools/conformance/src/bin/conformance.rs and tools/tier2/src/bin/tier2.rs carry the same defaults. No gate and no CI job runs them. `make conformance` and `make tier2` check the copies inside the tree against their rows of provenance.tsv before they judge anything. tier 3's guest has twelve configuration files named guest/kmscon*.conf; its provisioning (guest/user-data.in) installs the C reference's runtime dependencies and links autovt@.service to kmsconvt@.service; its benchmark figures are named kmscon_cpu_ms, kmscon_cpu_pct, kmscon_cpu_ms_per_mpv_frame and kmscon_cpu_ms_per_tmux_sixel; and adr.yaml excludes tools/tier3/guest/screens/ from `adr cite` because the screens cite the C reference's own records."
  - evidence: "measured 2026-11-04 on integration at 4667b6c2, the mentions that name the released `kmscon` program or that the licence needs: COPYING and LICENSE carry kmscon's and libtsm's copyright notices and author lists; crates/vtium-font/src/unifont.rs, crates/vtium-parse/src/parser.rs and crates/vtium-parse/src/utf8.rs carry the copyright notices of the files they were ported from; docs/install.md's sections `Moving from kmscon` and `Sharing VTs with kmscon` tell a user how to move VTs from an installed `kmscon` to vtium and how to keep both; `vtium console` refuses `enable --all` while the template link names kmsconvt@.service; when the `term` key names `kmscon`, vtium answers XTGETTCAP from crates/vtium/install/kmscon.ti, which `make install-tree` installs; docs/why-vtium.md says vtium reads `kmscon`'s configuration file, but vtium never reads /etc/kmscon (crates/vtium/src/config/mod.rs, ADR-1500 D4): it reads a `kmscon` configuration file only when a user copies it to /etc/vtium/vtium.conf."
  - upstream: "Claude Code's memory documentation (code.claude.com/docs/en/memory), read 2026-11-04: Claude Code loads CLAUDE.md and CLAUDE.local.md from the working directory and every directory above it; a gitignored CLAUDE.local.md exists only in the worktree where it was created; by default Claude Code reads AGENTS.md only when no CLAUDE.md, .claude/CLAUDE.md or CLAUDE.local.md is in the working directory or above it, and a CLAUDE.md that imports AGENTS.md brings AGENTS.md in through the import."
  - standard: "The MIT licence that vtium, kmscon and libtsm are under: the copyright notice and the permission notice shall be included in all copies or substantial portions of the software."
  - evidence: "measured 2026-11-04 on integration at 4667b6c2, what the three provenance records cover: tools/conformance/provenance.tsv lists the corpus' cases, its 164 goldens in each of golden/ and golden-frame/, corpus/INTERFACE.md, corpus/cases.list, probes/probes.replay, crates/vtium-term/tests/palettes.c.txt and six generator inputs; tools/tier2/provenance.tsv lists tools/tier2/reference/test_pixels.c and the two PPMs beside it; tools/tier3/provenance.tsv lists the 223 files of c-goldens/, guest/kmscon-minimal.conf and guest/kmscon-minimal-psf.conf, guest/meta-data, guest/user-data.in, guest/tier3-install and its service, 29 screen scripts, kitty-common.inc, sixel-common.inc and tmux-grid.inc, mpv-stats.lua, the photo directory and t3img.py. `make conformance`, `make tier2` and tier 3's preflight refuse to run when a listed copy differs from its row. The tracked screens mpv-sixel.sh and tmux-grid.inc read the benchmark names kmscon_pid and kmscon_ms that acceptance.inc, which no provenance record lists, defines."
  - evidence: "read 2026-11-04 on integration at 4667b6c2: vtium's default TERM is `xterm-vtium` (crates/vtium-settings/src/keys.rs, TERM_NAME), its own terminfo entry in crates/vtium/install/xterm-vtium.ti, named with the xterm- prefix so that a program which guesses a terminal's abilities from its name treats it as xterm-like; the `term` key sets TERM, and its comment in crates/vtium/install/vtium.conf.example says the default names vtium's own entry and XTGETTCAP answers from it, that `kmscon` names the C reference's entry and XTGETTCAP answers from that, and that under any other name XTGETTCAP knows no capability."
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
  - operator: aaronsb
    said: "delete now, because I think they will continue to affect agent reasoning. we'll keep a watch on how things go"
    via: the steering session, 2026-11-04, on the provenance records, the C test programs, c-goldens.list and cref.rs, the provenance make targets and docs/c-reference.md
  - operator: aaronsb
    said: "I'm inclined to keep, but I'm unsure of how terminfo works in linux. if we write our own terminfo and install it, is it just as effective as any other terminfo? or is there something special I'm missing? I have been operating under an understanding that programs and software may have hardcoded a terminfo selection of just popular terminals, and therefore have no idea about vtium. if the case is that terminfo can offer most programs proper terminfo, then maybe I am over thinking it. I believe we should offer compatiblity since kmscon is known, but that probably will interfere with using all of vtium's features. (because kmscon's main branch doesn't support many features vtium does)"
    via: the steering session, 2026-11-04, on the kmscon terminal name
  - operator: aaronsb
    said: "One history sentence"
    via: the steering session, 2026-11-04, on docs/why-vtium.md's Beside kmscon; the maintainer chose this option from the steering session's interview, and the option's label and its description (the section becomes one past-tense sentence saying vtium began from kmscon, and the comparison table keeps kmscon as one row among the others) are the steering session's wording
  - operator: aaronsb
    said: "Yes, Linux console column"
    via: the steering session, 2026-11-04, on ADR-1000's c_reference column; the maintainer chose this option from the steering session's interview, and the option's label and its description (drop c_reference and add a linux column for the kernel's console) are the steering session's wording
    covers: [ADR-1000's comparison column]
  - operator: aaronsb
    said: "make the smallest edit, your lean"
    via: the steering session, 2026-11-04, on AGENTS.md's references to deleted files and targets; the lean the maintainer accepted is the steering session's wording
observable:
  - "`docs/scripts/fork-refs` exits 0 on the tree after the sweep, and exits 1 naming the file and line when a planted line in a crate README says `the C reference`"
  - "Every case is judged by one golden in one directory per tier: tools/conformance/expected.tsv, tools/tier2/departures.tsv, tools/tier3/departures.tsv and tools/tier3/c-goldens/ are gone, and `make conformance`, `make tier2` and `make tier3` pass"
  - "`git ls-files` lists no provenance.tsv under tools/conformance, tools/tier2 or tools/tier3, no .c file, and no docs/c-reference.md, and `make help` lists no target with provenance in its name"
  - "No crate README has a section named Departures, C behaviour kept, Beyond the C reference or C notes"
status: accepted
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

- **Decided:** vtium's own goldens are the judge of every tier. Every golden copied from the C reference, the maintainer's forks of `kmscon` and `libtsm`, becomes vtium's golden under the same review rule as every other golden, and each case has one golden in one directory per tier: the cases that pass today only because a list excuses their difference, about 85, get vtium's own output as their golden in golden commits batched by tier for one review pass, and the excuse lists go. The behaviour vtium implements is defined by the specifications it cites, ADR-1000's tables and its goldens, so the crate READMEs' departure lists go and a choice a reader needs is stated in the present tense as what vtium does and why. ADR-1000 drops its `c_reference` column and gains a `linux` column for the Linux kernel's own console. The leftovers of the fork are deleted now, and git keeps them: the three provenance records and the checks that read them, the three C test programs, tier 2's list of copied goldens and the code that checks screens against a C file, the three provenance make targets, and docs/c-reference.md. The docs, the README, the crate READMEs, code comments and strings, CONTRIBUTING.md, the Makefile and the tools are swept now, and a new check, `docs/scripts/fork-refs`, keeps the committed tree from naming the fork again. AGENTS.md and CLAUDE.md stay as they are until a later revisit, once the open issues have come down and vtium is more stable, apart from the smallest edit that removes AGENTS.md's references to deleted files and targets. The README keeps a short history in the past tense, the licence files and ported files keep their copyright notices, and text about the released `kmscon` program stays, including `kmscon` as a terminal name a user can choose. Existing decision records stay as history, with a pass over sentences that read as if the forks can be opened.
- **Trades away:** an outside judge. The copied goldens were drawn by a program vtium did not write; once they are vtium's, a regression the maintainer approves in a golden commit is no longer caught by anything independent. The departure lists' record of why each behaviour differs shrinks to the statements a reader still needs, and the rest lives in git history. The maintainer reviews three golden commits, one per tier, for about 85 cases (50 in tier 1, 4 in tier 2, 31 variant and screen pairs in tier 3). A contributor, and an agent, loses the map of how the C reference worked and the pointer from a comment to the C code it was written against; the maintainer chose that on purpose and watches how agents work without it.
- **One-way?** Expensive now, and one-way once the maintainer deletes the forks. Every committed change, the deleted files included, is in git and can be put back. Once the forks are gone, nobody can check a copied golden against its source again.
- **Probes:** *Confident (the judge):* where vtium deliberately draws or answers something other than a copied golden shows, the golden is replaced by vtium's output in a commit the maintainer reviews, and no list of departures stays beside it. *Not confident (ADR-1000's comparison column):* ADR-1000's tables lose their `c_reference` column, which recorded what the C reference did for each mode and feature, even though ADR-1000 is otherwise a record the maintainer called historical; the five columns for other terminals stay.
- **Inversion:** Between "keep naming the C reference as the judge until the forks are deleted" (the tree stays tied to code nobody outside can read) and "erase every trace of where vtium came from" (history, licence notices and the released `kmscon` user's path go too). The answer keeps the judge's values as vtium's own goldens, deletes the files that exist only to tie vtium to the fork, keeps the history in the README, the records and git, and leaves AGENTS.md's working rules for the steering session to the revisit.

## Context

vtium was built against the C reference: the maintainer's forks of the C terminal `kmscon` and its screen and parser library `libtsm`, frozen as the measure vtium had to meet. Its tests compared vtium with goldens, saved expected results, copied from the C reference with a record of where each came from (its provenance). A difference from a copied golden had to be a departure, a listed difference with its reason, and the crate READMEs list 246 of them. AGENTS.md tells every agent to answer each design question from the C reference's code first, and the code comments, docs and tools cite the C reference's files and functions on 3,915 lines in 349 files.

vtium now does more than the C reference did, and in places it draws better, so a copied golden can hold vtium back. The maintainer plans to delete the forks. A reader of the public repository cannot open them, and a list of "differences from kmscon" describes vtium by what another program did, which goes stale and breaks the rule that vtium is written about in the present tense. The maintainer also expects that files about the fork, left in the tree, keep pulling agents' reasoning toward it.

Terms used below. A golden is a saved expected result that a tier compares vtium's output with. A provisional golden is one of vtium's goldens that the maintainer has not reviewed yet, and it judges nothing. A golden commit is a commit that changes goldens' values and nothing else, lists each value that moved and why, and that the maintainer reviews. An excuse list is a file that lets a case pass while vtium's output differs from its golden: tools/conformance/expected.tsv, tools/tier2/departures.tsv and tools/tier3/departures.tsv. The revisit is the later piece of work, once the open issues have come down and vtium is more stable, that decides what AGENTS.md and CLAUDE.md say about the forks and whether a private layer of uncommitted files holds that (D6).

## Decision

### D1: vtium's goldens are the judge

Every tier judges vtium by vtium's goldens. Every golden copied from the C reference becomes vtium's golden. AGENTS.md's rule holds for all of them alike: the author of a change never rewrites the golden that judges it, and a golden changes only in a golden commit the maintainer reviews.

Every case passes today, but not every case passes because vtium produces what its copied golden shows. About 85 cases pass only because an excuse list covers their difference: the 50 cases that the 57 rows of tools/conformance/expected.tsv cover, the 4 of tools/tier2/departures.tsv, and the 10 rows of tools/tier3/departures.tsv, which cover 31 variant and screen pairs. Calling those copied goldens vtium's as they stand would give vtium goldens that show output vtium does not produce. Each of these cases instead gets vtium's own output as its golden. The golden commits are batched one per tier, and each lists every case, what moved, and the specification or decision behind the difference, so the maintainer reviews all of them in one pass. Each tier's excuse list goes in the commit after that tier's golden commit.

Each tier keeps its goldens in one directory, one golden per case. tools/tier2/reference/goldens/*.ppm move into tools/tier2/goldens/ with no byte changed. tools/tier3/c-goldens/ merges into tools/tier3/goldens/: where goldens/ already holds vtium's golden for a case, the copy goes, and every other copy moves with no byte changed. A commit that only moves goldens, with no byte changed, is not a golden commit, and git shows it as a rename. tools/conformance/expected/ is different: its screen/ (49 files) and frame/ (50 files) hold vtium's output for tier 1's excused cases under the same names as the goldens in corpus/golden and corpus/golden-frame, so merging it into the corpus replaces those goldens' bytes. That merge is tier 1's golden commit, and it lists each case.

Tier 1's golden commit also covers the capability probes. The copied `xtgettcap` probe in tools/conformance/probes/probes.replay gets vtium's expected answer, and the `departure` line in tools/conformance/probes/vtium.replay that let vtium's answer stand in for it goes, together with the code in tools/conformance that checks a probe's departure against a crate README.

With tools/conformance/expected.tsv gone, `conformance issues`, which `make conformance-issues` runs, reads capabilities.list alone. No row of expected.tsv is of the pending kind today; all 57 are departures. A copied case that vtium does not pass yet because an open issue owes the work is listed in tier 1's provisional list, which today holds vtium's own cases (tools/conformance/vtium/provisional.tsv) and covers the copied corpus from then on, so its golden judges nothing until the work lands; the capability the case needs is a `missing` line in capabilities.list that names the issue, and `conformance issues` checks that the issue is still open.

The provenance records go (D3), and with them the checks that compare each copy with its record before a run, in `make conformance`, `make tier2` and tier 3's preflight. vtium's goldens are guarded by the golden commit rule and by git, as vtium's other goldens are today. tier 2 stops checking its screens against the C program test_pixels.c: each screen's bytes live in tools/tier2/src/screens.rs as vtium's own, and tools/tier2/src/cref.rs goes. tools/tier2/pending.tsv becomes the list of screens tier 2 still owes, each named for what it shows. The glyph and cell digests in vtium-font's and vtium-render's tests are vtium's goldens; a deliberate change to what they cover writes the digest the failing test prints, in a golden commit.

Names in the tree that say "C", "reference" or "kmscon" for vtium's own files are renamed to say what they are now:

| Now | Becomes |
|---|---|
| tools/tier3/c-goldens/ | merged into tools/tier3/goldens/ |
| tools/conformance/expected/ | merged into tools/conformance/corpus/golden and golden-frame |
| tools/tier2/reference/goldens/*.ppm | tools/tier2/goldens/ |
| `c_goldens`, `c-goldens` and `kmscon_goldens` names in tools/tier2 and tools/tier3, and `c_ref` names in crates/vtium-render/tests/memory.rs | names for what the values are, such as `goldens` or `budget` |
| crates/vtium-font/tests/reference/ and crates/vtium-render/tests/reference/, with tests/reference.rs in each crate | tests/digests/ and tests/digests.rs |
| tier 3's twelve guest configurations, guest/kmscon*.conf | guest/vtium*.conf, as vtium.conf, vtium-gl.conf, vtium-minimal.conf |
| tier 3's benchmark names `kmscon_cpu_ms`, `kmscon_cpu_pct`, `kmscon_cpu_ms_per_mpv_frame` and `kmscon_cpu_ms_per_tmux_sixel`, and `kmscon_pid` and `kmscon_ms` in guest/screens/acceptance.inc and the screens that read them | `console_cpu_ms`, `console_cpu_pct`, `console_cpu_ms_per_mpv_frame`, `console_cpu_ms_per_tmux_sixel`, `console_pid`, `console_ms` |
| ADR-1000's `c_reference` column | removed, and a `linux` column added (D2) |

tier 3's guest provisioning stops installing the C reference's runtime dependencies and stops linking autovt@.service to kmsconvt@.service. That changes the base image's inputs, so each checkout builds a new base image once, and the commit that does it runs every tier 3 variant to show that no golden moved.

### D2: what defines vtium's behaviour

The behaviour vtium implements is defined by the specifications vtium cites, ADR-1000's tables, and vtium's goldens. No committed text defines vtium by comparison with another program.

The crate READMEs' `Departures`, `Golden differences`, `C behaviour kept`, `Beyond the C reference` and `C notes` sections go, as do docs/architecture.md's `Beyond the C reference` and docs/vt-review.md's `Departures from the C reference's review`. The excuse lists cite README entries, so the README sections go after the golden commits and the excuse lists (D1). The author of the sweep reads each item and does one of three things with it. An item that a specification and a test already say is dropped. An item a reader needs, such as a bound, a refusal, or a choice between two readings of a specification, becomes a present-tense statement in the crate README of what vtium does and why, with the specification it cites. An item that is about a mode or feature row becomes a sentence in that row's note column in ADR-1000. A security fix that the C reference lacked is stated as what vtium does, with the specification it follows.

ADR-1000's tables drop the `c_reference` column and gain a column named `linux` for the Linux kernel's own console: the escape and control sequence handling of the kernel's VT layer, which every Linux machine has, which fbcon draws, and which programs meet under TERM=linux. Like the columns for xterm, foot, Ghostty, WezTerm and kitty, it records published documentation: its values come from console_codes(4), in the words those columns use. This record does not fill the column. One edit to ADR-1000, a branch of its own in the sweep, drops `c_reference`, adds `linux` and fills it, with a dated note in ADR-1000 saying what changed and why. A row whose behaviour console_codes(4) leaves unclear reads `unverified`, and those rows go into a guide for the maintainer's VT review, because an agent on this machine never opens a /dev/tty<N> console and so cannot try the kernel console itself.

### D3: committed text describes vtium and cites no C code

Code comments, strings in code (such as the bounds table's citations in vtium-font and test failure messages), crate READMEs, docs pages, CONTRIBUTING.md, the Makefile, configuration file comments, tier 3's screen scripts and the tools describe vtium in the present tense and do not cite the C reference, its files, its functions or its issues. A comment that explained vtium's code by the C function it followed explains what vtium's code does, or goes.

The files that exist only to tie vtium to the fork are deleted, and git keeps them: tools/conformance/provenance.tsv, tools/tier2/provenance.tsv and tools/tier3/provenance.tsv; the C test programs tools/tier2/reference/test_pixels.c, crates/vtium-font/tests/reference/refdigest.c and crates/vtium-render/tests/reference/refcells.c; tools/tier2/c-goldens.list and tools/tier2/src/cref.rs; the make targets `conformance-provenance`, `tier2-provenance` and `tier3-provenance`, the LIBTSM and KMSCON variables and the `provenance` subcommands of conformance, tier2 and t3.py; and docs/c-reference.md with its entry in zensical.toml's menu. The READMEs beside the digests say how vtium's digests are checked and changed, without the recipe for the deleted C programs.

The files that came from the C reference and stay are vtium's own now, and the sweep rewrites their text like any other file: tools/conformance/corpus/INTERFACE.md, the replay interface's description, and tools/conformance/probes/probes.replay, whose comments cite the C reference and its issues. docs/modes.tsv is generated from vtium-term's mode table and carries `libtsm#N` citations in its spec column, so the branch that sweeps strings in code changes them in the table and regenerates docs/modes.tsv.

These stay, because they serve a reader who has no fork:

- The README's `Provenance` section keeps a short history in the past tense: vtium began as a Rust program derived from `kmscon` and `libtsm`, whose authors COPYING lists.
- COPYING and LICENSE keep kmscon's and libtsm's copyright notices and author lists, and a file ported from them keeps its notice (vtium-font's unifont.rs and vtium-parse's parser.rs and utf8.rs today), as the MIT licence requires. Their prose follows the tense rule.
- Text about the released `kmscon` program that a user may have installed: docs/install.md's `Sharing VTs with kmscon`, `vtium console`'s handling of kmsconvt@.service and the code and comments behind it, and vtium reading a `kmscon` configuration file that a user copies to /etc/vtium/vtium.conf; vtium never reads /etc/kmscon. Each describes another program on the user's machine in the present tense and says nothing about how vtium was judged.
- docs/install.md's `Moving from kmscon` becomes `Moving from another DRM/KMS VT`, rewritten to cover moving to vtium from any other terminal that runs on a VT through DRM/KMS, with `kmscon` as one example among others.
- docs/why-vtium.md's `Beside kmscon` becomes one past-tense sentence saying vtium began from `kmscon`, and the page's comparison table keeps `kmscon` as one row among the others. The section's claim that vtium reads `kmscon`'s configuration file goes with it, since vtium never reads /etc/kmscon.
- Text that a golden shows changes only in a golden commit. tier 3's screens print "kmscon" where the goldens show it: guest/screens/boxdraw.sh's heading, the three notices in guest/screens/kitty-files.sh and the heading of guest/screens/sixel-bound.sh. That text changes with tier 3's golden commit, so the maintainer reviews tier 3 once.

### D4: the terminal names

vtium's default TERM is `xterm-vtium`, its own terminfo entry, which describes everything vtium does. A program that reads terminfo, as most programs on Linux do through ncurses, learns vtium's abilities from that entry exactly as it would from any other terminal's entry; there is nothing special about the entries that ship with ncurses beyond being installed everywhere. The entry's name starts with `xterm-` so that a program that guesses a terminal's abilities from its name, instead of reading terminfo, treats vtium as xterm-like. vtium also answers XTGETTCAP, the query by which a program asks the terminal itself for a capability, from the same entry.

`kmscon` stays as a name a user can choose with the `term` key, for a machine or a program that knows the released `kmscon` and not vtium. Under it, vtium answers XTGETTCAP from crates/vtium/install/kmscon.ti, the entry for the released `kmscon`'s name, and a program that trusts that entry sees fewer of vtium's abilities than vtium has. Comments and docs describe kmscon.ti as the entry for the released `kmscon`'s name, without the fork and without the C reference's probes.

### D5: the decision records

The existing decision records stay as the history of what was decided and why. The tense pass changes only the sentences that read as if a reader could open the forks today, such as an instruction to read the C reference's code at a path or a statement of what the C reference does, and turns each into a past-tense statement of what was found. The frontmatter's `basis`, `considered` and `concern` entries stay as written, as the record convention requires. Existing `C notes` and `Where vtium departs from the C reference` sections stay, since they record what was found when the decision was made. A record that cites a deleted file keeps the citation as history.

A new record has no `C notes` section and no departures section. It argues from specifications, measurements and vtium's own needs. It may have a `Prior art` section that names other programs, the C reference among them, in the past tense, as history of what others did.

adr.yaml's comments follow D3, and its `cite.exclude` entry for tools/tier3/guest/screens/ goes once the screen scripts no longer cite the C reference's own records.

### D6: AGENTS.md and CLAUDE.md wait for the revisit

> Edited 2026-11-04: the maintainer deferred the private layer (the first `considered` entry). The earlier text moved the rules for using the forks into a CLAUDE.md in the directory that holds the checkout, and the provenance records, the C programs, docs/c-reference.md and the provenance checks into a private directory beside it; git keeps that text. The maintainer then chose to delete those files instead (the sixth `considered` entry).

AGENTS.md and the committed CLAUDE.md keep their rules about the C reference until the revisit. The maintainer and the steering session keep using the forks as prior art as they do today, reading the forks themselves and git history for what this record deletes.

AGENTS.md names files, targets and a rule that this record removes, so as it stands it would point at things that no longer exist and tell agents to write departure lists again. At 4667b6c2 these are: the instruction to start from docs/c-reference.md (line 29) and the table row that links it (line 173); tools/conformance's copies and provenance.tsv (line 74); the three provenance checks (line 76); tools/tier2/reference/ (lines 107 and 113); tools/tier3/c-goldens/ (lines 108 and 113); and the rule that a difference from a golden must be a listed departure, in the crate's README or in the conformance list of departures (line 117). The sweep makes the smallest edit to AGENTS.md that removes these references, and changes nothing else (the tenth `considered` entry). Each step of the work that removes or renames one of these makes the edit for it in the same branch, so AGENTS.md never points at something gone.

The revisit decides what AGENTS.md and CLAUDE.md say about the forks, and whether a private layer of files that are never committed holds the steering session's rules for them. For that revisit, these facts about Claude Code stand (the upstream entry in the basis):

- Claude Code loads a CLAUDE.md from the working directory and from every directory above it, so a CLAUDE.md in the directory that holds the checkout and its worktrees reaches every worktree there.
- A gitignored file inside the repository, such as CLAUDE.local.md, exists only in the worktree where it was created.
- By default Claude Code reads AGENTS.md only when no CLAUDE.md, .claude/CLAUDE.md or CLAUDE.local.md is in the working directory or above it. A CLAUDE.md in a parent directory therefore stops AGENTS.md loading by itself, and the committed CLAUDE.md's import of AGENTS.md is what keeps it loading.
- The project's own ways, the guidance files under the checkout's uncommitted .claude/ directory, teach the rule of listing departures from the C reference and change at the revisit.

### D7: CONTRIBUTING.md defines the judge before the builder

CONTRIBUTING.md first says what judges a change: the tiers from unit tests to tier 3 and the maintainer's VT review, vtium's goldens and the golden commit rule, and ADR-1000's rows with the tests tagged to them. Only then does it say how to build a change. A contribution brings its judge before its code: a test that fails before the change, a new golden marked provisional, or the ADR-1000 row it serves. A contribution that changes what vtium draws or answers brings the golden commit that records it.

### D8: a check holds the break

`docs/scripts/fork-refs` fails when a committed file names the fork: the phrases "C reference", "C fork", "kmscon fork" and "libtsm fork", the repositories `aaronsb/kmscon` and `aaronsb/libtsm`, and the fork issue forms `kmscon#N` and `libtsm#N`. It leaves the plain names `kmscon` and `libtsm` to review, since D3 and D4 keep them where they name the released program, a terminal name or the licence. It also fails when a crate README has a heading `## Departures`, `## Golden differences`, `## C behaviour kept`, `## Beyond the C reference` or `## C notes`, so that nobody brings the departure lists back.

It skips docs/architecture/, which D5 governs, COPYING and LICENSE, and its own source and test fixtures, which hold the patterns it looks for. Until the revisit it also skips AGENTS.md and CLAUDE.md, with a note on each entry saying so. CI and `make gates` run it; it joins AGENTS.md's gate line at the revisit. It lands with the last branch of the sweep, seen to fail on the tree before the sweep and on a planted line, and to pass after.

## Consequences

### Positive

- A reader of the docs, the READMEs and the code meets one judge, vtium's goldens, and one description of each behaviour, in the present tense, with no program they cannot open.
- A case is judged by one golden in one place, so a reviewer reads one expected result instead of a copied golden, a list of rectangles where vtium may differ, and a README entry that explains it.
- The gate line and CI need nothing beside the vtium checkout, as before, and the Makefile loses the targets that would break when the forks are deleted.
- The check in D8 stops the break from wearing away as agents trained on the old text keep writing "the C reference".

### Negative

- No independent program judges vtium any more. A wrong golden commit that the maintainer approves goes unnoticed until a person sees the screen, which is why each golden commit lists what moved.
- The golden commits for the excused cases change values, not only labels: the maintainer reviews about 85 cases where vtium's output differs from what the copied golden shows.
- The copied goldens lose the check that compared them with their records before every run; an edit to one shows only in git, as an edit to any of vtium's goldens does.
- Agents lose docs/c-reference.md, the map of how the C reference worked part by part. An agent that needs it reads git history or asks the steering session, and the maintainer watches whether agents work better or worse without it.
- Until the revisit, AGENTS.md still tells agents to read the C reference first, and fork-refs carries a skip entry for it.
- A program on a remote host reached over ssh does not know `xterm-vtium` until the entry is installed on that host, or until it reaches ncurses' own terminfo database; until then such a program falls back to what it knows of the name, or the user sets TERM to a name the host has.
- The sweep touches about 350 files across every crate and tool, in several branches, and conflicts with any work open on the same files at the time.
- Most of the 246 departure items carried reasoning. Those that are dropped keep their reasoning only in git history, and a later reader who wants it has to find the commit.
- Each checkout builds a new tier 3 base image once, about 330 MiB of packages and a file of about 1.1 GiB.
- Once the forks are deleted, nobody can check where a copied golden came from.

### Neutral

- GitHub issues already filed stay as they are. New issues, PR bodies and commit messages follow the same rule as committed text.
- The SPDX and REUSE header sweep, which adds a licence header to every file, runs in the same quiet window as a separate commit series, so the two sweeps never edit the same lines in one commit.

### The order of the work

1. This record, read by the maintainer before the sweep starts.
2. The deletions: the provenance records and the checks that read them, the C test programs, c-goldens.list and cref.rs, the provenance make targets and subcommands, and docs/c-reference.md with its menu entry, with AGENTS.md's smallest edit for them (lines 29, 74, 76 and 173) in the same branch. They come first because the copied goldens can move and change only once the records that pin their bytes are gone.
3. The goldens, one tier at a time, for the maintainer's single review pass, each tier with AGENTS.md's smallest edit for what it moves (lines 107, 108 and 113), and the last with the edit that removes the departure rule (line 117):
   - tier 1: merging tools/conformance/expected/ into the corpus is the golden commit, with the `xtgettcap` probe's expected answer; then the commit that removes expected.tsv, the probe's `departure` line and the code that checks it, and has `conformance issues` read capabilities.list alone.
   - tier 2: the copied PPMs move into tools/tier2/goldens/ with no byte changed; the golden commit for its 4 excused cases; then the commit that removes departures.tsv.
   - tier 3: the copied goldens vtium has none of its own for move into goldens/ with no byte changed, and the rest go; the golden commit for the 31 excused pairs, together with the screens' visible "kmscon" text and the goldens it moves; then the commit that removes departures.tsv.
4. The sweep, by area, each its own branch with its own review: docs and the README, docs/why-vtium.md's section among them; CONTRIBUTING.md; the crate READMEs, after step 3; code comments and strings, with docs/modes.tsv regenerated; the tools, tools/conformance/corpus/INTERFACE.md and probes.replay, the screen scripts' comments, the Makefile, the renames and tier 3's provisioning change; the decision records' tense pass and adr.yaml; and the ADR-1000 edit that drops `c_reference` and adds `linux`. Each branch makes AGENTS.md's smallest edit for anything it removes or renames that AGENTS.md names. docs/scripts/fork-refs lands with the last branch.
5. Later, the revisit: AGENTS.md and CLAUDE.md (D6), and fork-refs' skip entries for them.

## Open questions

1. **A private directory under version control.** Deferred to the revisit. The lean for then: a local git repository with no remote, so its history survives, and the maintainer adds a private remote if one is wanted.

The maintainer answered the record's other questions on 2026-11-04 (the `considered` entries), and the decision above carries each answer.

## Alternatives Considered

- **Keep the C reference as the named judge until the forks are deleted.** It leaves the work for later, but the tree keeps pointing at code an outside reader cannot open, the departure lists keep describing vtium by another program, and the maintainer called that program an inferior renderer that holds vtium back. Rejected.
- **Call the copied goldens vtium's as they stand and keep the excuse lists.** It needs no golden review, but about 85 of vtium's goldens would show output vtium does not produce, and every one of those cases would still need a list beside it. Rejected: the maintainer chose to replace them.
- **Keep the provenance records, the C programs and docs/c-reference.md in the tree until the revisit.** It keeps the copies checkable against their sources from the tree, but the copied goldens could not change while the records pin their bytes, so each tier would keep two golden directories, and the files would keep pulling agents toward the fork. Rejected: the maintainer chose to delete them now, and git keeps them.
- **Move those files to a private directory beside the checkout.** It keeps them at hand without committing them, but the maintainer is unsure a private layer works as intended and deferred it. Rejected for now; the revisit may take it up.
- **Rename the C reference to "the reference" and keep the departure lists.** It keeps two judges per case and a list of where vtium may differ from the one it does not follow. Rejected for one golden per case.
- **Make `kmscon` vtium's default TERM, or drop it.** As the default it would hide much of what vtium does from every program that trusts terminfo; dropped, it would leave a user whose machine or program knows only the released `kmscon` with no fallback. Rejected for D4's opt-in name.
- **Keep the private rules in a gitignored file inside the repository, such as CLAUDE.local.md or a file under .claude/.** Each worktree is a fresh checkout, and Claude Code's documentation says a gitignored CLAUDE.local.md exists only in the worktree where it was created, so worktree agents would not see it. A CLAUDE.local.md also counts as a CLAUDE.md and stops Claude Code from reading AGENTS.md under its default setting. Rejected, and kept as background for the revisit.
- **Import the private rules from the home directory in the committed CLAUDE.md.** Claude Code's documentation suggests it for sharing private instructions across worktrees, but the committed file would then name a private path that every other contributor's agent tries to import and lacks. Rejected.
- **Rewrite git history to remove the C reference.** It would break every link to an earlier commit and remove the history the maintainer wants kept, and the licence requires the notices anyway. Rejected.
- **Leave the decision records entirely untouched.** It keeps a few sentences that tell a reader to open the forks, which the maintainer asked to change. Rejected for the narrow tense pass in D5.
