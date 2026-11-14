---
contract: adr/v1
kind: decision
verb: add
capability: process
basis:
  - precedent: ADR-1502
  - evidence: "the docs workflow at b3955f37, .github/workflows/docs.yml: the docs job installs the pinned Zensical, checks that docs/architecture/INDEX.md is current, runs zensical build --strict --clean into target/site, and on a push to main uploads target/site with upload-pages-artifact; the deploy job publishes it. zensical.toml's site_dir is target/site. No step builds rustdoc."
  - evidence: "the workspace at b3955f37: the crates under crates/ are vtium, vtium-config, vtium-devices, vtium-font, vtium-image, vtium-image-helper, vtium-parse, vtium-platform, vtium-render, vtium-settings and vtium-term. Cargo.toml names each tools crate as a workspace member beside crates/*. rust-toolchain.toml pins the toolchain the gates and CI run on, 1.99.1. No crate sets the missing_docs lint."
  - evidence: "GitHub's releases of vtium/vtium, read 2026-11-14: eight pre-releases, v0.1.0-alpha.1 on 2026-11-02 to v0.1.0-alpha.8 on 2026-11-14, each named 'VTium VERSION'. The body of v0.1.0-alpha.8 is markdown: a heading, a paragraph on what vtium is, a line on what the release is about, a section 'What is new' with a subsection for each area, and a section on known departures. It links the site's pages by full address, such as https://vtium.github.io/vtium/fonts-for-scripts/, and each issue by its full address. The release carries the vendor archive and its SHA-256 as files. The repository holds no copy of any release's notes."
  - evidence: "measured 2026-11-04 with zensical 0.0.74 and recorded in ADR-1502: zensical build --strict fails on a link to a missing page or anchor and on a link that leaves docs_dir, and nav takes external addresses."
  - upstream: "rustdoc's book, the lints chapter: rustdoc::broken_intra_doc_links warns on an intra-doc link that resolves to nothing, and RUSTDOCFLAGS='-D warnings' turns every rustdoc warning into an error. cargo doc --no-deps documents the named packages without their dependencies and writes them under target/doc, one folder per crate, with shared files beside them."
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-14
deciders:
  - aaronsb
related:
  - ADR-1502
  - ADR-1507
extends: ADR-1502
considered:
  - operator: aaronsb
    said: "The site page docs/releases/VERSION.md is the only text of the release notes, and the GitHub release body is generated from it. As written."
    via: the steering session's questions on 2026-11-14, on the site records
    paraphrase: true
    covers: [one text]
  - operator: aaronsb
    said: "The releases list sits under Home, beside Showcase. As written."
    via: the steering session's questions on 2026-11-14, on the site records
    paraphrase: true
    covers: [where releases sit]
  - operator: aaronsb
    said: "make api joins the gate line and make gates, whatever it costs."
    via: the steering session's questions on 2026-11-14, on the site records
    paraphrase: true
---

# ADR-1508: The site publishes the crates' interfaces under /api/ and a page for each release, and each release's notes are written once

## Summary

- **Decided:** vtium's site gains two parts. The first is the crates' interfaces under `/api/`. The docs workflow builds rustdoc, the reference Rust generates from the doc comments, for every crate under `crates/`, and adds it to the site under `/api/` before the site is published. Three pages written by hand go with it: an index of the crates and their main types, a "Getting started" page whose code `cargo test` compiles, and a page on the rules every crate's interface follows. A build with a broken link inside rustdoc fails, and `make api` runs the same build on this machine, as a gate. The second is a page for each release, `docs/releases/VERSION.md`, with an index newest first, in the Home section beside the showcase. The page on the site is the one text of a release's notes. The GitHub release takes its body from that page through `make release-notes`, which turns each link to a page of the site into the page's full address.
- **Trades away:** the docs job takes longer, by a rustdoc build of the workspace, and the published site grows by rustdoc's files. A release's notes are written in the repository before the release is tagged, so a late fix to the notes is a commit, not an edit on GitHub. The pages for past releases are copies, made once, of what GitHub shows today.
- **One-way?** No. Both parts are files the site builds or copies, and removing them removes them. The addresses `/api/CRATE/` and `/releases/VERSION/` are the parts people keep.
- **Probes:** *Confident (one text):* you want each release's notes written once. The page on the site is that text, and the GitHub release shows the same words, with its links pointing at the site. *Not confident (where releases sit):* the releases sit in the Home section beside the showcase, so a reader who asks what is new finds them from the first page. You may prefer a tab of their own in the top bar, or a place beside the guide for installing vtium.
- **Answered 2026-11-14** (`considered`): the maintainer kept the site page as the only text of each release's notes, kept the releases under Home beside the showcase, and kept `make api` in the gate line and `make gates` whatever it costs.
- **Inversion:** Between "GitHub is the only place" and "the site is a second copy". In the first, release notes and the crates' interfaces stay on GitHub and in the source, and a reader on the site must leave it. In the second, the site holds its own text, which drifts from GitHub's. The answer makes the site's files the one source and generates what GitHub shows from them.

## Context

vtium's site, which ADR-1502 decides, shows the pages under `docs/`. Two things a reader may want are not there. The crates' interfaces are documented only in their doc comments, which a contributor reads by opening the source. The release notes live only in GitHub's releases, so a reader on the site cannot see what changed in the version they installed. vtium is a terminal program, not a library. Still, its engine is split into crates with interfaces of their own. vtium-parse reads the byte stream, vtium-term keeps the screen, vtium-render draws it, and vtium-font and vtium-image serve them.

Rustdoc is the tool that turns a crate's doc comments into HTML pages, with a page for each public item and links between them. `cargo doc` runs it. A doc test is a code block in a doc comment, which `cargo test` compiles and runs, so an example in the documentation cannot fall behind the code.

The notes of vtium's eight releases so far were written for each release and pasted into GitHub. They link the site's pages by full address, so they read the same on GitHub and anywhere else.

## Decision

### D1: the crates' interfaces under /api/

- **What is built.** `make api` runs `cargo doc --no-deps --all-features` for each crate under `crates/` that has a library, with `RUSTDOCFLAGS="-D warnings -D rustdoc::broken_intra_doc_links"`, on the toolchain `rust-toolchain.toml` pins. It writes `target/doc`, with one folder for each crate. The tools crates are not built, because they are vtium's own harnesses and not interfaces anyone builds on. Rustdoc documents only public items.
- **Where it goes.** The docs job runs `make api` after Zensical builds the site, and copies `target/doc` into `target/site/api/` before it uploads the site. Each crate's reference is then at `https://vtium.github.io/vtium/api/CRATE/`, such as `/api/vtium_term/`. A pull request's run builds the same and publishes nothing, as ADR-1502 D5 has it.
- **The pages written by hand.** Three pages live in `docs/api/`, and Zensical builds them into the same `/api/` folder:
  - `docs/api/index.md`, "The crates' interfaces": each crate under `crates/`, a line on what it does, its main types, and a link to its reference. A test in `tools/site` fails when a crate under `crates/` has no entry, or an entry names no such crate.
  - `docs/api/getting-started.md`, "Getting started": feed bytes to vtium-term and read the screen back. vtium-term includes the page as a doc comment on an item that exists only for doc tests, so `cargo test --workspace --all-features` compiles and runs its code. A change to vtium-term that breaks the page's code fails the tests.
  - `docs/api/conventions.md`, "Conventions": the rules every crate's interface follows. It covers bounded memory and bounded input, no panic on any input, how errors are returned, and which cargo feature turns on what. It also says what is stable. Until vtium decides which interfaces and file formats it promises to keep, the page says that any interface may change in any release.
- **Examples.** The doc tests and any `examples/` programs are the examples. `cargo test` already compiles both.
- **Links.** Zensical's strict build does not see rustdoc's pages, so the pages in `docs/api/` link a crate's reference by its full address on the site. The rustdoc flags are the check of every link inside rustdoc.
- **The menu.** "For contributors" gains "The crates' interfaces", which is `docs/api/index.md`, beside the link to the crates' folders on GitHub. The two other pages are reached from the index and listed in `not_in_nav`.
- **The gate.** `make api` joins the gate line and `make gates`, because it needs no network, and because a broken intra-doc link then fails on this machine before a push. It stays there however long the build takes, as the maintainer decided (`considered`).
- **Doc comments.** No crate sets the `missing_docs` lint yet. A count of the public items without a doc comment in each crate, taken once with the lint as a warning, starts the work that writes the missing comments. That work turns `missing_docs` on for each crate it finishes. Doc comments follow AGENTS.md's Writing section.

### D2: a page for each release

- **The pages.** Each release has a page, `docs/releases/VERSION.md`, where VERSION is the version without its leading `v`, such as `docs/releases/0.1.0-alpha.8.md` at `/releases/0.1.0-alpha.8/`. `docs/releases/index.md` lists every release, newest first, each with its date and a line on what it is about. A test in `tools/site` fails when a page under `docs/releases/` is missing from the index.
- **What a page holds.** The page says what the release adds, changes and fixes, in the reader's words, and anything a person must do when upgrading, such as a renamed configuration key, a changed default or a new package. It links the release on GitHub for its files. It follows AGENTS.md's Writing section, and it may tell what the release changed in the past tense, as history.
- **The menu.** The Home section lists "Releases", which is the index, after "Showcase". Each release's page is reached from the index and listed in `not_in_nav`.
- **Links.** A page links another page of the site by a relative path, as every page under `docs/` does (ADR-1502 D4), so the strict build checks each link and its anchor. An issue or a file outside `docs/` is linked by its full address on GitHub.

### D3: one text for the site and for GitHub

- **The source.** The page under `docs/releases/` is the one text of a release's notes. Nobody writes the notes anywhere else.
- **What GitHub shows.** `make release-notes VERSION=V` writes `target/release-notes/V.md` from the page. It turns each relative link to a page under `docs/` into that page's full address on the site, such as `https://vtium.github.io/vtium/fonts-for-scripts/`, and keeps every other line as it is. A command of `tools/site` does the conversion, and its tests check a link with an anchor, a link to an index page and a link that is already a full address.
- **When.** The release's steps write the page on `integration` before `stage` moves, so the page reaches `main` in the pull request that takes the release there, and the maintainer reads the notes in that diff. After the tag, the steering session creates the GitHub release with `gh release create --notes-file target/release-notes/V.md`. A later correction is a commit to the page, followed by `gh release edit --notes-file` with the file generated again.
- **Past releases.** The work that builds the section writes a page for each release from `v0.1.0-alpha.1` on, from its GitHub body as published. It turns each full address of a site page into a relative link and changes nothing else. Those pages keep the words GitHub shows, and the GitHub bodies stay as they are.

## Consequences

### Positive

- A contributor reads a crate's interface on the site without a checkout, and the "Getting started" page shows working code, because `cargo test` runs it.
- A reader on the site sees what changed in the version they installed and what an upgrade asks of them, without leaving the site.
- A release's notes are written once, and the strict site build checks their links before the release is tagged.
- A broken link inside rustdoc fails on this machine, in the gates, before CI sees it.

### Negative

- The docs job and the gates each gain a rustdoc build of every crate, which takes time on every pull request. That time is not measured yet, and it does not decide whether `make api` stays a gate.
- Rustdoc's output adds files to the published site. Their size is not measured yet.
- Rustdoc's output changes with the toolchain, so a change of the pinned toolchain changes every page under `/api/`. Nobody reviews those pages, since they are generated and not committed.
- The release's steps gain a page to write and a command to run, and the notes must be final when `stage` moves, before the tag.
- The three pages in `docs/api/` are written by hand and can fall behind the crates. The test on the index and the doc test on "Getting started" catch part of that, and "Conventions" stays a page a person keeps current.

### Neutral

- Rustdoc's pages carry rustdoc's own look, not the site's, and the site's search does not find them. Rustdoc has its own search.
- The release pages do not hold the release's files, which stay on GitHub.

## Alternatives Considered

- **Publish rustdoc on docs.rs.** docs.rs builds the documentation of every crate published to crates.io. Not chosen: vtium publishes no crate to crates.io, and the maintainer asked for the interfaces on vtium's own site.
- **Commit rustdoc's output under `docs/api/`.** The pages would show in a diff. Rejected: rustdoc's output is large and changes with each toolchain, and the docs job can build it from the source every time.
- **Write the release notes on GitHub and copy them into the site with a script.** The notes keep their present home. Not chosen: the strict build could then check their links only after the release, and the text would have two homes, one of which a later edit on GitHub would leave behind.
- **Put the releases under How-to guides, beside the guide for installing vtium.** A reader who upgrades finds them there. Not chosen: a list of releases is not a task, and the Home section is where a reader who asks what is new starts.
- **A tab of its own for releases.** Easy to find. Not chosen: the top bar already holds seven tabs, and the releases need only one entry.
