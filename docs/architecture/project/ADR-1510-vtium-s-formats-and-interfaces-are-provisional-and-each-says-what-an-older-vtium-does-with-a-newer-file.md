---
contract: adr/v1
kind: decision
verb: constrain
capability: process
basis:
  - operator: aaronsb
    level: directed
    said: "A provisional record states what vtium does now with each format and interface. vtium tries to stay consistent with that record. Since nobody depends on vtium yet, a format may still change when there is a practical reason."
    via: "the maintainer's interview in the steering session, 2026-11-20, round 1, as the steering session's notes and its comment on the GitHub issue for vtium's formats record it"
    paraphrase: true
  - operator: aaronsb
    level: guided
    said: "What an older vtium does with a file written for a newer one, for example a key, a frit line or a line of a `[head NAME]` section it does not know: ignore it with a notice, refuse the file, or refuse that line. The answer should give the person a clear message instead of a silent wrong result."
    via: the body of the GitHub issue for vtium's formats, filed by the steering session from the maintainer's list
  - evidence: "vtium.conf's top level at integration 5872cd2e2: crates/vtium/src/config/mod.rs reads it and reports a key it does not know once, however often it appears, clamps a value out of its bound, and ignores a value it cannot read, keeping what the sources below it set; nothing in the top level stops the rest being read. The test `an_unknown_key_is_reported_once` (crates/vtium/src/config/tests.rs) holds it. An unknown option on vtium's command line is reported the same way (crates/vtium/src/config/cmdline.rs)."
  - evidence: "vtium.conf's sections at integration 5872cd2e2: crates/vtium-settings/src/parse.rs reads every line and names every error with its line. A missing or unknown `version` drops everything but the `policy` lines; an unknown section drops that section; a key a section cannot take, an unknown one included, drops the section, and a head drops with its rules (crates/vtium-settings/README.md). The renamed head keys `on-loss`, `rescue` and `profile` get a message that names the new key (`Problem::Renamed`)."
  - evidence: "Frits and layers at integration 5872cd2e2: vtium-config's reader refuses the whole frit at an unknown key, a malformed line, a value out of range or a key the build does not build yet, and names the file and the line (crates/vtium-config/src/frit.rs, its module comment; ADR-1200 D1). A layer's lines are read with the frit's (ADR-1204 D5). A frit carries no version line."
  - evidence: "The replay interface, tools/conformance/corpus/INTERFACE.md at integration 5872cd2e2: `--describe` prints `interface 1.0` and the views, features and commands the implementation offers; a view or test command the implementation lacks, or an unknown command, exits 2 with a usage error. `make conformance` runs the interface check that holds an implementation's `--describe` to the describe block."
  - evidence: "vtium-config's commands, ADR-1403 D8: their exit codes 0 to 5 are fixed, and 2 means the command could not be used, an unknown option among the reasons. The control socket's answer codes are fixed once a release ships them, and a new code is added beside the others (ADR-1401 D12). The man pages are generated from the code, and `make man-check` fails when a committed page is stale (ADR-1507)."
  - evidence: "What vtium answers to a query, at integration 5872cd2e2: replies follow the reply rule in docs/architecture.md; DECRQM answers from vtium-term's mode table and 0 for a mode the table lacks (ADR-1000); XTGETTCAP answers from the terminfo entry, and a check fails when the committed table differs from the entry (docs/architecture.md); tier 1's goldens and the capability probes (`make conformance`, `make conformance-probes`) hold each reply."
  - precedent: ADR-1000
  - precedent: ADR-1401
  - precedent: ADR-1403
  - precedent: ADR-1504
  - precedent: ADR-1507
  - precedent: ADR-1508
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "At the first beta"
    via: the steering session's interview on 2026-11-22 (question tool, brief/for/against/lean per probe); the maintainer chose the lean over 0.1.0 and 'when someone relies on one'
    covers: [when it binds]
  - operator: aaronsb
    said: "Ignore just the unknown key"
    via: "the steering session's interview on 2026-11-22 (question tool, brief/for/against/lean per probe); the maintainer chose against the lean (drop the section): an older vtium keeps the rest of the section and skips only the key it does not know, with the same message naming the file and line"
    covers: [an older vtium and a newer file]
status: proposed
date: 2026-11-20
deciders:
  - aaronsb
related:
  - ADR-1200
  - ADR-1204
  - ADR-1505
---

# ADR-1510: vtium's formats and interfaces are provisional, and each says what an older vtium does with a newer file

## Summary

- **Decided:** The maintainer decided on 2026-11-20 that vtium's formats and interfaces are provisional. This record writes down what vtium does now with each of them: vtium.conf's keys and its sections, frits and their layers, the replay interface, the command lines of `vtium`, `vtium check-config` and `vtium-config`, and what vtium answers to a program's queries. vtium tries to stay consistent with what the record says. Because nobody depends on vtium's formats yet, vtium changes a format when there is a practical reason, and the release notes and this record say what changed. For each format the record also says what an older vtium does with a file written for a newer one. The rule is the same everywhere: the person gets a clear message that names the file and the line, and never a silent wrong result. The record names, for each format, the test that would show a break.
- **Trades away:** No promise of compatibility yet. A person who keeps a vtium.conf or a frit may have to edit it after an upgrade, and a tool that reads vtium's output may have to change. Writing the rules down also binds vtium to say each change in the record and the release notes.
- **One-way?** No. The record is provisional by the maintainer's decision, and each format can still change. A format becomes expensive to change once people keep files in it, which is what a later, binding version of this record would settle.
- **Probes:**
  - *Not confident (when it binds):* the record stays provisional until the maintainer says otherwise. Should it become binding at the first beta, at 0.1.0, or only when someone outside the project relies on a format?
  - *Confident (an older vtium and a newer file):* an older vtium never guesses at a key or a line it does not know. It reports the key and ignores it at vtium.conf's top level, drops the section that holds it, or refuses the frit, and each time it names the file and the line. Is that the behaviour you want?
- **Inversion:** Between "promise nothing until 0.1.0", where any release may change any format without a word, and "freeze every format now", where every change needs a migration before anyone depends on vtium. vtium writes down what it does, keeps to it while there is no reason to change, and says each change.

## Context

vtium is alpha. People are starting to keep files that vtium reads, and to write tools that read what vtium writes. Each of those is a format that vtium has to keep working once people rely on it, and no record said what vtium promises about any of them. The maintainer listed the formats and asked, for each one, what an older vtium does with a newer file, what a newer vtium does with an old one, and how a test would show a break (`basis`). The maintainer then decided that the record is provisional: it states what vtium does now, vtium tries to stay consistent with it, and a format still changes for a practical reason, because nobody depends on vtium yet.

Three words recur below. A **format** is a file layout or an interface that a person or another program relies on. **An older vtium with a newer file** is a vtium that meets a key, a line or a section that a later release added. **A break** is a change that makes a file or a program that worked with one release fail or behave differently with the next.

## Decision

### D1: the formats are provisional

- **vtium keeps to what this record says** while there is no practical reason to change. A release that changes a format says so in its release notes, as ADR-1508 has a release's notes written once, and edits this record with a dated note at the format's section.
- **vtium changes a format when there is a practical reason,** such as a key whose name misleads, or a layout that blocks a feature. Nobody depends on the formats yet, so vtium carries no migration and no period of deprecation for such a change while the record is provisional.
- **A rename keeps a message for the old name where that costs little.** vtium.conf's renamed head keys show how: `on-loss`, `rescue` and `profile` each get a message that names the new key. A key binding given by its old name on vtium's command line still works (ADR-1005 D10).

### D2: one rule for an older vtium and a newer file

- **vtium never applies a key, a line or a section it does not know.** It reports what it does not know and names the file and the line, so the person sees why the setting has no effect.
- **What vtium drops follows how much the unknown part could change.** A top-level key of vtium.conf changes one setting, so vtium ignores that key alone. A key inside a section changes what the section means, so vtium drops the section. A frit line changes which glyphs the helper draws, so vtium refuses the whole frit, since a half-read frit draws the wrong glyphs.
- **A file that states a version vtium does not know** is read only as far as is safe: vtium.conf with an unknown `version` keeps its `policy` lines and drops every section (ADR-1401 D7).

### D3: each format, what vtium does now, and the test that shows a break

| Format | What vtium does now | An older vtium with a newer file | A newer vtium with an older file | The test that shows a break |
|---|---|---|---|---|
| vtium.conf's top-level keys and their values | The key table in vtium-settings defines each key, its values and its bounds. A value out of bounds is clamped, and a value vtium cannot read is ignored. | vtium reports an unknown key once, ignores it, and reads the rest of the file. `vtium check-config` names the key and its line. | A key that a release removes or renames gets an unknown-key report, or the renamed key's message where one is kept (D1). | `an_unknown_key_is_reported_once` in crates/vtium/src/config/tests.rs holds the report. The test that keeps the shipped `vtium.conf.example` equal to what the key table generates shows each changed key. A test that loads the previous release's example and expects no report would show a removed key; the gap-check writes it. |
| vtium.conf's sections: `version`, `policy`, `[head NAME]`, `[look NAME]`, `[rules]` and `[labels]` | The flow reads every line and names every error with its line (ADR-1403 D3 to D5). The file states `version = 1`. | An unknown section drops that section. An unknown key in a section drops the section, and a head drops with its rules. An unknown `version` drops everything but the `policy` lines. | A renamed head key gets a message that names its new name. | vtium-settings's tests of the flow's reader, for each kind of error and what it drops. A test that reads the previous release's head file template, `crates/vtium-settings/templates/heads.conf`, and expects no error would show a break; the gap-check writes it. |
| Frits and their layers | vtium-config reads a frit strictly. A layer's lines are read with the frit's own. A frit carries no version line. | vtium refuses the whole frit at an unknown key or a malformed line, and names the file and the line (ADR-1200 D1). | A key that a release removes is an unknown key, and the frit is refused with its line. | The frit reader's tests in crates/vtium-config/src/frit.rs, among them the one for an unknown key. A test that reads every frit and layer of the previous release's catalog and expects each to read would show a break; the gap-check writes it. |
| The replay interface | `tools/conformance/corpus/INTERFACE.md`, interface 1.0. `--describe` lists the views, features and commands an implementation offers. | An implementation that lacks a view or a test command that a newer case uses exits 2 with a usage error. A newer case never reads as a pass. | A case written for an older interface still plays, as long as its commands keep their names. | The interface check that `make conformance` runs, which holds `--describe` to the describe block, and the corpus itself. |
| The command lines of `vtium`, `vtium check-config` and `vtium-config` | `vtium` reports an unknown option and starts with the rest. `vtium-config`'s exit codes 0 to 5 are fixed, and 2 means the command could not be used (ADR-1403 D8). `vtium check-config`'s exit status judges a file. | A script that uses a newer option gets the unknown-option report from `vtium`, or exit 2 from `vtium-config`. | A removed option gets the same report or exit code. | `make man-check`, which fails when a man page generated from the code differs from the committed page (ADR-1507), and the tests of each command's exit codes. |
| What vtium answers to a program's queries: the terminfo entry, DA, XTGETTCAP and DECRQM | Each reply follows the reply rule in docs/architecture.md and ADR-1000's tables. XTGETTCAP answers from the terminfo entry, and DECRQM answers 0 for a mode vtium does not know. | A program that asks an older vtium for something it lacks gets no reply, or DECRQM's 0, as the specifications allow, so the program treats the feature as missing. | A reply that changes is a change to ADR-1000's tables, made through that record. | Tier 1's goldens and the capability probes (`make conformance`, `make conformance-probes`), and the check that fails when the XTGETTCAP table differs from the terminfo entry. |

### D4: the gap-check

A gap-check reads the code against D3, row by row, and lists each place where vtium does something other than the table says, and each test the table names that does not exist yet. Each gap becomes a card of its own, with its own verification level (ADR-1505).

## Consequences

### Positive

- A person who upgrades vtium sees a message that names each key, section or frit line the new release does not take, instead of a setting that silently stops working.
- A change to a format is said in one place, the release notes, and recorded here.
- Each format has a test that a break would turn red, so a change is a deliberate edit rather than an accident.

### Negative

- vtium promises no compatibility yet, so a person may have to edit a vtium.conf or a frit after an upgrade.
- Each change of a format costs an edit of this record and a line in the release notes.
- Three of the tests that D3 names do not exist yet, and the gap-check has to write them.

### Neutral

- Other files vtium reads or writes keep the rules of their own records: the state file and `vtium config flow --json`, which carry `version` 1, and the control socket's codes (ADR-1401 D12, ADR-1403 D8), vtlook files (ADR-1304), palette files (ADR-1205 D10), pointer files (ADR-1309) and the batch of active resources (ADR-1205). A later edit of this record can add them to D3.
- The internal formats between programs of one release, such as the glyph chunks on the helper's pipe (ADR-1200 D2), are not formats under this record, because both ends always come from the same release.

## Alternatives Considered

- **Promise nothing until 0.1.0.** Not taken: the maintainer wants vtium to stay consistent where it can, and a person deserves a clear message when a file stops working.
- **Freeze every format now, with migrations and periods of deprecation.** Not taken: nobody depends on the formats yet, and the maintainer decided that a practical reason is enough to change one.
- **Ignore every unknown key and line, everywhere, with a notice.** Not taken: an unknown key in a head's section or a frit line can change what a whole section or set means, so reading the rest would draw or route something the file did not intend.
- **Refuse the whole vtium.conf at any unknown key.** Not taken: one unknown top-level setting would cost the person every other setting, and the top level has always been read the forgiving way.
