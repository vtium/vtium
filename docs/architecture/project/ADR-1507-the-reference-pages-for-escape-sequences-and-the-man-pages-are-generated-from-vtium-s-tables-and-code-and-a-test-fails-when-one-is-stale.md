---
contract: adr/v1
kind: decision
verb: add
capability: process
basis:
  - operator: aaronsb
    level: directed
    said: "The maintainer decided on 2026-11-13: generate the pages from code, as vtium already generates docs/config.md and vtium.conf.example from its key table. A small Rust generator writes the pages in roff. … A check fails when a committed page is stale, and the check joins the gate line."
    via: "the issue on man pages for vtium, vtium-config and vtium.conf, its body as filed; the wording is the steering session's record of the maintainer's decision"
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "The man page staleness check joins the gate line in AGENTS.md and make gates."
    via: "the steering session's questions on 2026-11-14, on the wave plan; the wording is the steering session's"
    paraphrase: true
  - precedent: ADR-1502
  - precedent: ADR-1000
  - evidence: "the tree on integration at b3955f37, read 2026-11-14: crates/vtium-term/src/terminal/modes.rs generates docs/modes.md and docs/modes.tsv from the mode table, and its test generated_documents_match_the_table fails when either differs; VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites them. crates/vtium/src/config/doc.rs generates docs/config.md and crates/vtium/install/vtium.conf.example from the key table in crates/vtium-settings/src/keys.rs; VTIUM_BLESS=1 cargo test -p vtium generated_files rewrites them."
  - evidence: "ADR-1000's tables at b3955f37: the features table has 187 rows in 11 groups, 166 of them outside the group 'Display, platform and configuration'; their ids start ctl (31), sgr (23), platform (21), text (19), query (18), color (14), image (14), osc (14), input (13), window (13), cursor (3), clipboard (2), dcs (2) and id (1). The feature column names the sequence in words and notation, as 'SGR 4 and 24: underline' or 'DECID (ESC Z)', and names no parameter defaults. The modes table has 131 rows. Both tables carry status, reason, note, layer, issue and tests columns, and docs/scripts/coverage is the only writer of the tests column."
  - evidence: "the commands at b3955f37: vtium's usage is a string in crates/vtium/src/main.rs; vtium console's is the string USAGE in crates/vtium/src/console/text.rs; vtium blank's and vtium wake's are strings in crates/vtium/src/notice.rs; vtium's own options are the configuration keys given as --key=value, which crates/vtium/src/config/cmdline.rs reads; crates/vtium/src/kernel.rs reads the kernel command line key vtium.enable. vtium-config keeps the options that take a value in a table, VALUE_OPTIONS, in crates/vtium-config/src/cli.rs, and its help in crates/vtium-config/src/help.rs."
  - evidence: "the Makefile at b3955f37: make install stages vtium and vtium-config under $(PREFIX)/bin, and, in a build with images, vtium-image, the image helper vtium starts, and vtium-gallery, the picture gallery tool; it stages the units, vtium.conf.example, the frits, the examples and the licenses, and no man page. The packaging recipes live in their own repositories, each a git submodule under packaging/ (ADR-1500)."
  - standard: "man-pages(7) and man(7), the Linux man-pages project's conventions for a manual page: section 1 for user commands, 5 for file formats, 8 for system administration commands, and the sections NAME, SYNOPSIS, DESCRIPTION, OPTIONS, EXIT STATUS, ENVIRONMENT, FILES and SEE ALSO, in that order. mandoc(1)'s -T lint reports the problems it finds at the levels style, warning, error and unsupported."
agent:
  name: Claude
  model: claude-opus-5-5
status: accepted
date: 2026-11-14
deciders:
  - aaronsb
related:
  - ADR-1000
  - ADR-1502
  - ADR-1508
extends: ADR-1502
considered:
  - operator: aaronsb
    said: "The sequence pages live at /sequences/ROW-ID/. As written."
    via: the steering session's questions on 2026-11-14, on the site records
    paraphrase: true
  - operator: aaronsb
    said: "Each sequence page links both the GitHub issue and the decision record that governs the row. The issue is the link for following the work, and the decision record is the link for readers who want the reasoning. The page says which link is which. AGENTS.md's rule that user pages link the issue, not the record, gains a line naming sequence pages as the exception."
    via: the steering session's questions on 2026-11-14, on the site records
    paraphrase: true
    covers: [links]
  - operator: aaronsb
    said: "The man pages are vtium(8), vtium-config(1), vtium.conf(5) and vtium-gallery(1), with the image helper as a section of vtium(8). As written."
    via: the steering session's questions on 2026-11-14, on the site records
    paraphrase: true
  - operator: aaronsb
    said: "we've already gone through this so yes"
    via: the steering session's chat on 2026-11-14, answering whether to accept wave 1's proposed records in one batch
---

# ADR-1507: The reference pages for escape sequences and the man pages are generated from vtium's tables and code, and a test fails when one is stale

## Summary

- **Decided:** vtium generates two new kinds of reference from the tables and code that tests already keep true. The first is a page on the site, under `docs/sequences/`, for each row of ADR-1000's features table that names an escape sequence. An index lists every sequence by the way it is put together: control characters, ESC, CSI, OSC, DCS and APC. Each page says what vtium does with the sequence, its parameters and their defaults, its status and why, and the tests behind it. It links the GitHub issue that tracks the row's work and the decision record that governs the row, and says which link is which (edited 2026-11-14, D1; it read "and the issue and tests behind it"). A hand-written concept page explains escape sequences to a reader who has never read ECMA-48. The second is a set of man pages in roff: `vtium(8)`, `vtium-config(1)`, `vtium.conf(5)` and `vtium-gallery(1)`. They come from the key table and from each command's own table of options, are committed under `man/`, and `make install` stages them, so every package carries them. For each kind, a test fails when a committed page differs from what its source generates now, and a variable rewrites the pages. The man pages' check also runs `mandoc -T lint`, and it joins the gate line and `make gates`.
- **Trades away:** every command's options move from a usage string into a table that the help text and the man page both come from. vtium-term gains a table of every sequence's form and parameters, which a person keeps beside the code that handles the sequence. Each change to the tests column through `docs/scripts/coverage --update` also rewrites the sequence pages. The gate machine needs `mandoc`.
- **One-way?** No. The pages are generated, so removing a generator removes its pages. The addresses `/sequences/ROW-ID/` are the part that people keep, and ADR-1000 never changes a committed id, so those addresses can stay.
- **Probes:** *Confident (one page per row):* a program's author who looks up one sequence, say OSC 52, lands on one page. That page says what vtium does with it, its parameters, whether vtium supports it and why, and which issue tracks what is missing, and it shows no other terminal's behaviour. *Not confident (links):* those pages link the GitHub issue behind each row, as AGENTS.md asks of every page for people who use vtium, and not the decision record behind it, although the work that asked for the pages named the decision.
- **Answered 2026-11-14** (`considered`): the maintainer kept one page per row and the addresses `/sequences/ROW-ID/`. Each page links both the issue and the decision record that governs the row, which D1 now decides. The man pages are the four this record names, with the image helper as a section of `vtium(8)`.
- **Inversion:** Between "write the reference by hand" (pages that read well and drift from the code within a release) and "publish the tables as they are" (pages that never drift and that nobody can read). The answer keeps the facts in the tables and the code, keeps a few hand-written sentences for each entry beside the code they describe, and lets a test say when a page is behind.

## Context

A program running inside vtium talks to it through escape sequences and control characters. It moves the cursor, sets colors, asks for vtium's size, and sends pictures. To a program's author, those sequences are vtium's interface. ADR-1000's two tables already record every sequence and mode vtium supports, ignores or refuses, with the reason, the issue and the tests. The site shows the modes, through `modes.md`, which a test generates from the mode table. Nothing shows one sequence on a page of its own, so a reader cannot look up `CSI Ps J` and read what vtium does with it.

The features table names each sequence in its feature column, in words and notation, such as "SGR 4 and 24: underline". It holds no parameters and no defaults. Some of its rows name no sequence at all: the rows of the group "Display, platform and configuration", and rows such as bidirectional text, outline fonts and input methods.

vtium also installs no man pages. On an installed system `man vtium` and `man vtium.conf` find nothing. vtium already generates `docs/config.md` and the example configuration from its key table. The maintainer decided that the man pages are generated from code in the same way. A check that fails when a committed page is stale joins the gate line. A man page is a file in roff, the typesetting language that `man` reads, and is numbered by section: 1 for commands a user runs, 5 for file formats and 8 for system administration.

## Decision

### D1: a page for each sequence

vtium's site gains a reference section of pages for escape sequences, generated and never edited by hand:

- **One page for each row.** Each row of ADR-1000's features table that names a sequence gets a page, `docs/sequences/ROW-ID.md`, such as `docs/sequences/sgr.underline.md`. A row already groups the sequences that belong together, such as SGR 4 and 24, and a row's id never changes once committed, so the page's address stays the same.
- **The modes stay on one page.** `modes.md` keeps listing every mode, and the index links it. The modes table needs no page per mode, since a mode's whole story is its row.
- **What a page holds.** The page's title is the row's feature. Then come each sequence's form in the notation of xterm's control sequences document, such as `CSI Ps J`, and each parameter with its meaning and its default. A few plain sentences say what vtium does with the sequence. The page then gives the row's status and its reason, and its note when there is one. It names the specifications the row cites. When vtium departs from what a specification says, the page says how in a sentence and links the section of the crate's README that records the departure. The page lists the tests in the row's tests column, each linked by its address on GitHub.

> Edited 2026-11-14: the maintainer asked for each page to link both the issue and the decision record (`considered`). The last sentence of "What a page holds" read: "The page links the GitHub issue in the row's issue column, and lists the tests in the row's tests column, each linked by its address on GitHub." The bullet "Two links, each named" is new. "What a page leaves out" ended: "The page links no decision record, because AGENTS.md has pages for people who use vtium link the issue that tracks a feature or a gap."

- **Two links, each named.** Each page ends with two links, and a short line before each says what it is for. The first is the GitHub issue in the row's issue column, the link for a reader who wants to follow the work on the row. The second is the decision record, and the section of it, that governs the row, the link for a reader who wants the reasoning. The page links the record by a relative path into `docs/architecture/`, so the strict site build checks the link. A row with an empty issue column gets the second link alone. AGENTS.md has pages for people who use vtium link the issue and not the record. The sequence pages are the one exception, and AGENTS.md names them as such.
- **What a page leaves out.** The page shows none of the table's columns for other terminals, because vtium is described by what it does and by the specifications it follows.
- **The index.** `docs/sequences/index.md` lists every sequence, grouped by the way it is put together, in this order: C0 and C1 control characters, ESC, CSI, OSC, DCS and APC. Within a group the sequences are sorted by their final byte or their number. Each line gives the form, the name, the status and a link to the row's page. A row whose sequences fall in two groups appears in each. The index opens with a link to the concept page (D4) and a link to `modes.md`.
- **The menu.** The Reference section gains "Escape sequences", which is the index, after "Modes". The row pages are reached from the index and stay out of the menu, through a pattern in `not_in_nav`. The strict site build checks every link the pages hold.
- **The words.** Every sentence on a page follows AGENTS.md's Writing section. No sentence is copied from another terminal's documentation, and the forms and parameter names follow the specifications the row cites.

### D2: the table the pages come from

The pages come from two sources. ADR-1000's features table gives each row's feature, status, reason, note, specifications, issue and tests. A table of sequences in vtium-term, `crates/vtium-term/src/terminal/sequences.rs`, gives what the features table lacks. For each row that names a sequence, an entry gives the row's id, each sequence's form, its parameters with their meanings and defaults, the sentences that say what vtium does, a departure when there is one, and the decision record and section that govern the row, such as ADR-1000's "Replies and policy" or ADR-1004 D2 (edited 2026-11-14: the record and section are new, for the second link of D1). The same file lists each row of the features table that names no sequence, with a few words on what the row is instead, such as "a font feature". The table lives in vtium-term, beside the code that handles each sequence, so that a change to a sequence's handling and a change to its entry go in one commit.

### D3: the test that keeps the pages current

A test in vtium-term, `generated_documents_of_the_sequences_match_the_tables`, generates every page and the index and compares each with the committed file. Its name starts like that of the test that keeps `modes.md`, so `VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents` rewrites both. The test fails in each of these cases:

- a committed page or the index differs from what the tables generate now;
- a row of the features table is neither in the table of sequences nor in its list of rows that name no sequence;
- an entry names an id that ADR-1000's tables lack;
- an entry names a decision record that does not exist under `docs/architecture/`;
- a file under `docs/sequences/` has no entry, so a row that leaves the table takes its page with it.

The test runs in `cargo test --workspace`, so it is already part of the gate line. Since a page lists its row's tests, `docs/scripts/coverage --update` rewrites the pages after it rewrites the tests column, in the same way it already regenerates `tools/conformance/COVERAGE.md`. A second run of `coverage --update` then changes nothing.

### D4: the concept page

One page is written by hand: `docs/concepts/escape-sequences.md`, "Escape sequences", in the Concepts section of Explanation. It is for someone who has never read ECMA-48. In plain words it explains the screen and its character cells, the cursor, and colors. Then it explains how a sequence is put together: the C0 controls, ESC, CSI with its parameters, intermediate and final bytes, and the strings OSC, DCS and APC. It gives an example of each. The index of sequences links it first, and the page links back to the index.

### D5: the man pages

vtium ships four man pages, written in roff with the `man(7)` macros:

- **`vtium(8)`**, the console. It covers each form of vtium's command line and its subcommands: `check-config`, `config`, `blank`, `wake`, and `console enable`, `disable`, `status` and `fallback`. It covers the systemd units vtium installs and the kernel command line key `vtium.enable`. vtium's own options are its configuration keys given as `--key=value`, so the page says that once and sends the reader to `vtium.conf(5)` for each key. Its FILES section names `/etc/vtium/vtium.conf`, `/etc/vtium/fonts` and the units, and its EXIT STATUS section names each status vtium exits with. A section on helpers covers the two programs that vtium starts and a person does not run: the font helper, which is `vtium-config` started by vtium, and the image helper `vtium-image`.
- **`vtium-config(1)`**, the font and settings tool: each of its commands and each of its options.
- **`vtium.conf(5)`**, the configuration file: every key with its type, its default and its range, then the file's sections for heads and looks, its rules and its two policies. It holds the same facts as `config.md` and `head-file.md`, from the same sources.
- **`vtium-gallery(1)`**, the picture gallery tool that a build with pictures installs: its command line and what it shows.

Each page follows man-pages(7): its sections in their usual order, and SEE ALSO naming the other three pages. The page's header names vtium's version and no date, so that the page changes only when its source does.

### D6: where the man pages come from

- **The key table.** `vtium.conf(5)`'s keys come from the key table in `crates/vtium-settings/src/keys.rs`, which `config.md` and `vtium.conf.example` already come from.
- **A table of options for each command.** Each command keeps its options in a table of its own: each option's name, its argument and a line on what it does. Both the command's `--help` text and its man page are generated from that table, so the two cannot differ. vtium-config already keeps the options that take a value in such a table. Where a command keeps its options only in a usage string today, as vtium, `vtium console`, `vtium blank` and `vtium wake` do, the man page work turns the string into a table. The `--help` text that the table generates either matches the string or changes in the same commit.
- **Templates for the prose.** The sections that no table holds, such as DESCRIPTION and the explanation of the policies, live in the generator's templates and follow AGENTS.md's Writing section.
- **The generator.** A small workspace crate, `tools/man`, generates the pages. It reads the key table and the commands' tables through the crates that own them, fills the templates, and writes `man/vtium.8`, `man/vtium-config.1`, `man/vtium.conf.5` and `man/vtium-gallery.1`, which are committed.

### D7: the check that keeps the man pages current

- **The test.** A test in `tools/man`, `man_pages_match_the_code`, generates each page and fails when a committed page differs from it. `VTIUM_BLESS=1 cargo test -p vtium-man` rewrites them, as the same variable rewrites `config.md`. The test runs in `cargo test --workspace`.
- **The gate.** `make man-check` runs that test and then `mandoc -T lint -W warning` on each page, which fails on any problem worse than a style note. `make man-check` joins the gate line in AGENTS.md and `make gates`. The gate machine needs `mandoc`, and the gate says so by name when `mandoc` is missing.

### D8: where the man pages install

`make install` stages each page under `$(PREFIX)/share/man`, in the folder for its section: `man8/vtium.8`, `man1/vtium-config.1` and `man5/vtium.conf.5`, and `man1/vtium-gallery.1` in a build with pictures. It stages them uncompressed, because each distribution's package tools compress man pages in their own way. Every recipe that stages vtium through `make install` therefore carries the pages. A recipe that installs files by hand adds the pages itself. The guide for installing vtium says where the pages come from and that `man vtium` opens the first one.

## Consequences

### Positive

- A program's author can look up one sequence and read on one page what vtium does with it, and the page is never behind the code, because a test fails first.
- `man vtium` and `man vtium.conf` work on every machine that installs a package of vtium.
- A command's help text and its man page come from one table, so they never disagree.
- The pages add no new source of facts: the features table, the key table and the commands' tables were already the truth, and the pages only show them.

### Negative

- A person writes the entry for each sequence by hand once: its form, its parameters, their defaults and what vtium does. With about 166 rows to cover, that is the largest piece of the work.
- A change to how vtium handles a sequence needs a change to its entry, and a change to a command needs a change to its table of options. The tests catch a missing entry or a stale page, but not an entry whose words are wrong.
- Each tag added to a test changes the pages of the rows it covers, so a branch that tags tests carries those pages too, and two such branches conflict there. Rerunning `docs/scripts/coverage --update` resolves the conflict, as it already does for the tests column.
- The gate line gains a gate, and the gate machine needs `mandoc`.

### Neutral

- The site gains about 166 pages that the menu does not list.
- The man pages are not published on the site, because `config.md`, `head-file.md` and the guides already show the same facts there.
- `tools/man` joins the workspace beside the other tools crates.

## Alternatives Considered

- **One page per group of the features table, such as one page for all SGR attributes.** Fewer pages. Not chosen: a page about 23 rows is long to read for one sequence, and an anchor inside a page breaks when a heading changes, where a row's id never changes.
- **One page per sequence, such as a page for SGR 4 and another for SGR 24.** Not chosen: the sequences of one row share one status, one reason and one issue, so their pages would repeat one another.
- **Keep the forms and parameters in a new column of ADR-1000's features table.** One file would hold everything. Not chosen: parameters and defaults are prose that grows long, the table is already wide, and a person who changes how vtium handles a sequence works in vtium-term, not in the decision record's table.
- **Write the sequence pages by hand.** They would read best. Rejected: they would drift from the code, which is what the maintainer's rule for generated pages exists to stop.
- **Write the man pages in markdown and convert them with a tool such as pandoc or scdoc.** Not chosen: the maintainer decided on a small Rust generator, the pages' facts already live in Rust tables, and a converter would add a tool to the build that the gate machine must also carry.
- **Generate the man pages at build time and commit none.** Nothing to keep current. Rejected: the maintainer asked for a check on committed pages, a reviewer then sees every change to a page in its diff, and a package can install the pages without running the generator.
- **Cover `vtium-image` with a page of its own.** Not chosen: vtium starts the image helper itself and a person has no reason to run it, so a section in `vtium(8)` serves the reader who wonders what the process is.
