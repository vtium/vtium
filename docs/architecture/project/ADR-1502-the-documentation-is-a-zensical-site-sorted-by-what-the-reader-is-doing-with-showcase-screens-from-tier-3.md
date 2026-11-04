---
contract: adr/v1
kind: decision
verb: add
capability: process
basis:
  - operator: aaronsb
    level: directed
    said: "The maintainer wants vtium's user documentation organized by Diátaxis and published as a website with mkdocs to GitHub Pages, so people they circulate vtium to can read it. The README today is about 4,800 words and too dense for a newcomer."
    via: "the steering session, 2026-11-03, relayed by the team lead in this record's brief; the wording is the team lead's"
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "The maintainer also wants showcase screenshots in place of the current minspec hero image: smooth outline-font text, emoji, CJK and shaped scripts, colors, and pictures."
    via: "the steering session, 2026-11-03, relayed by the team lead in this record's brief; the wording is the team lead's"
    paraphrase: true
  - standard: "Diátaxis (diataxis.fr), Daniele Procida's framework for technical documentation: four kinds of page, each serving one need of the reader. A tutorial is a lesson a learner follows by doing; a how-to guide gets a competent reader through one task; reference describes the machinery for looking things up; explanation gives the reasons and the context. A page that mixes two kinds serves neither reader well."
  - evidence: "the tree on integration at 85989610, measured 2026-11-03: README.md holds 4,778 words, in sections for status, features (displays, sharing the display, heads, input, fonts, pictures, the terminal, scrollback, small machines), how vtium is built against other work, a comparison with the alternatives, how vtium is judged, trying it, provenance and licence. docs/ holds 11 pages of 1,879 to 9,075 words. managing-vtium.md (4,543 words) holds the steps to install, configure, choose a font and troubleshoot beside a reference table of the keys vtium keeps for itself. heads.md (6,126 words) holds a glossary, the head file's syntax and match keys, an explanation of its two policies and the table of input routes beside its scenarios. 23 links in docs/ leave docs/ by relative path (architecture.md 2, concepts.md 5, managing-vtium.md 8, why-vtium.md 4, figures/README.md 1, and one in each of three generated specimen READMEs), and no decision record has one. Only README.md links to managing-vtium.md. docs/architecture/INDEX.md, written by `adr index`, lists all 21 records and links adr.yaml. Mermaid diagrams appear in concepts.md and in six records."
  - evidence: "PyPI, read 2026-11-03: mkdocs 1.6.1, released 2024-08-30, is the newest MkDocs; mkdocs-material 9.8.0, released 2026-10-24, is the newest Material for MkDocs; zensical 0.0.74, released 2026-10-30, is the newest Zensical; properdocs 1.6.7, released 2026-03-20, is a continuation of MkDocs 1.6 that runs MkDocs' plugins and themes."
  - upstream: "Material for MkDocs' changelog for 9.7.0 and its maintenance notice: the project is in maintenance mode, with critical bug fixes and security updates promised through 2026-11-05, and its authors' new work goes to Zensical, a successor that reads an existing mkdocs.yml as it stands and builds the same Material look. Zensical's FAQ (zensical.org/docs/community/faqs/) gives no date for a 1.0 release; third-party reports in 2026 disagree on whether its strict build checks links and anchors yet."
  - evidence: "tools/tier3/README.md at 85989610: the harness is also the way to put any test content on a real VT, and a screen is a shell script in guest/screens/; a variant names its machine, its display size through the EDID QEMU gives the guest, and its configuration; no variant draws at 1920x1080; a case with no golden fails as missing. tools/tier3/t3_pictures.py makes the test pictures from a few lines of arithmetic, so nothing is downloaded. Tier 3 runs on the maintainer's machine under QEMU/KVM, and no job in .github/workflows/ci.yml runs it."
  - evidence: "tools/specimens/README.md at 85989610: vtium ships no third-party font bytes, so the specimens' desktop font set, JetBrains Mono 2.304, Symbols Nerd Font Mono 3.4.0 and Noto Color Emoji 2.051, is pinned by URL and SHA-256 in tools/specimens/src/fonts.rs and fetched into a cache by `make specimens-fonts`; `make specimens` writes the specimen pages and images, and a branch commits them in a commit of their own."
  - evidence: "ADR-1000's features table at 85989610: `text.bidi`, bidirectional text, is supported in the `top` layer and not built yet, so vtium draws right-to-left text in the order the program sends it; the README's status lists the bold and italic faces of outline fonts as not built yet, so bold is the regular glyph widened and italic stays upright."
  - evidence: "GitHub's API, read 2026-11-03: vtium/vtium is public and has no Pages site yet (the pages endpoint answers 404); .github/workflows/ci.yml runs on every push to main and on every pull request."
  - upstream: "aaronsb/kmscon origin/main, read 2026-11-03: the C reference documented itself in README.md and in two man pages written in DocBook, docs/man/kmscon.1.xml.in and docs/man/kmscon.conf.5.xml.in, which its meson build turned into man pages; the fork added markdown notes under docs/fork/ and READMEs beside its harnesses. It published no website."
  - evidence: "measured 2026-11-04 with zensical 0.0.74 from PyPI, in a scratch project under target/: `zensical build --strict` exits non-zero, reporting 'page does not exist' for a link to a missing page and for a link that leaves docs_dir, and 'anchor does not exist' for a link to a missing anchor; it builds a page that is in neither nav nor not_in_nav without a warning, and its configuration code says it supports only the validation of links for now; exclude_docs keeps scripts/ and *.tsv files out of the site; a heading anchor with a letter outside ASCII resolves when toc uses pymdownx.slugs.slugify in lower case; with font = false the built site names no Google Fonts address; the theme's script names https://unpkg.com/mermaid@12/dist/mermaid.min.js; nav in zensical.toml takes nested sections and external addresses; mkdocs.yml and zensical.toml with the same settings give the same warnings; `zensical serve --strict` says strict mode is currently unsupported. PyPI: zensical needs Python 3.11 or later, ships compiled wheels for Linux on x86_64 and aarch64, and it and zensical-ui are under the MIT license."
  - evidence: "GitHub and SourceForge, read 2026-11-04: Iosevka's newest release is v34.9.1, published 2026-10-24, with PkgTTF-IosevkaTerm-34.9.1.zip among its assets, under the SIL Open Font License 1.1; Terminus's newest release is 4.49.1, under the SIL Open Font License 1.1."
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Zensical from the start, not Material. Pin Zensical's version (0.0.74, or the latest at writing) with hashes. Use Zensical's own build and serve commands in docs.yml and make docs/make docs-serve. Keep the config in the form Zensical reads (mkdocs.yml or its own file; say which and why). Say what the record does if a pre-1.0 Zensical release breaks the build: pin back, and the strict build job catches it. Drop the Material-maintenance switch triggers, but keep the facts that led here in the context."
    via: the maintainer in the steering session's interview, 2026-11-04, relayed by the team lead
    paraphrase: true
    covers: [the builder]
  - operator: aaronsb
    said: "Decisions: publish the ADRs as a Decisions section (the proposal stands)."
    via: the maintainer in the steering session's interview, 2026-11-04, relayed by the team lead
    paraphrase: true
  - operator: aaronsb
    said: "Arabic: leave it off the scripts screen until bidi is built (the proposal stands)."
    via: the maintainer in the steering session's interview, 2026-11-04, relayed by the team lead
    paraphrase: true
  - operator: aaronsb
    said: "Readers: Linux users new to vtium first (the proposal stands)."
    via: the maintainer in the steering session's interview, 2026-11-04, relayed by the team lead
    paraphrase: true
    covers: [the readers]
  - operator: aaronsb
    said: "Address: vtium.github.io/vtium (the proposal stands)."
    via: the maintainer in the steering session's interview, 2026-11-04, relayed by the team lead
    paraphrase: true
  - operator: aaronsb
    said: "Showcase font: Iosevka, not JetBrains Mono. Also, the showcase must show plainly that a user can use whatever fonts they want, bitmap or outline. Add a fonts scene that shows several fonts side by side, each drawn by vtium with its name: Iosevka, a bitmap console font (a PSF), Unifont, and one more OFL outline font. Its caption says any bitmap or outline font works, through a frit. Keep the shell hero scene in Iosevka. Every font must be redistributable, pinned by URL and SHA-256."
    via: the maintainer in the steering session's interview, 2026-11-04, relayed by the team lead
    paraphrase: true
  - operator: aaronsb
    said: "Regenerate the showcase at each release, reviewed in the release PR (the proposal stands)."
    via: the maintainer in the steering session's interview, 2026-11-04, relayed by the team lead
    paraphrase: true
  - operator: aaronsb
    said: "Pages source: the maintainer sets it once. Leave that as stated."
    via: the maintainer in the steering session's interview, 2026-11-04, relayed by the team lead
    paraphrase: true
status: accepted
date: 2026-11-03
deciders:
  - aaronsb
related:
  - ADR-1500
  - ADR-1501
---

# ADR-1502: The documentation is a Zensical site sorted by what the reader is doing, with showcase screens from tier 3

## Summary

- **Decided:** vtium's documentation stays as markdown files under `docs/`, readable on GitHub, and is also published as a website at `vtium.github.io/vtium`. Zensical, a static site generator, builds the site from a `zensical.toml` at the repository's root. The site's menu sorts the pages by what the reader is doing, as Diátaxis does: tutorials, how-to guides, reference and explanation, then the decision records and a section for contributors. No existing file moves. `managing-vtium.md` and `heads.md`, which mix kinds of page, are split, and two tutorials are written: "Try vtium on one VT" and "Your first frit". The README shrinks to a short front door. A link that leaves `docs/` is a full GitHub address, so it works in both places. A GitHub Actions workflow publishes `main` to GitHub Pages, and a CI job on every pull request builds the site in strict mode, which fails on a broken link or anchor; a Rust test fails on a page missing from the menu. New showcase screens, vtium's own drawing on a VT of tier 3's QEMU guest at 1920x1080 in Iosevka, replace the small-machine screens at the top of the README, and one of them shows four fonts side by side, bitmap and outline, to say that any font works. They are committed, regenerated at each release, and judge nothing.
- **Trades away:** one more toolchain, Python with Zensical and its pinned dependencies, used only to build the site, on a generator that has not reached its 1.0 release, so a newer release may change or break the build. The README stops being the place that says everything. Each release gains a tier 3 run and a commit of new images, and the repository grows by those images.
- **One-way?** No. The pages stay plain markdown, and `zensical.toml`, one workflow and one Rust test are all the site adds; removing them removes the site. The site's address is the hardest thing to change once people have it, and GitHub Pages redirects the `github.io` address to a custom domain once one is set.
- **Probes:** *Confident (the readers):* the people you send vtium to run Linux but have never seen vtium, and the first thing they want is to try it on one VT and get back to their desktop safely; then to pick a font; then to look things up. The order of the menu and the two tutorials are built for them. *Not confident (the builder):* Material for MkDocs, which you named, stops getting fixes two days from now. This record builds with it anyway, pinned, and keeps `mkdocs.yml` to what Zensical, its authors' successor, also reads, so the switch later is a change of the build command. You may prefer to start on Zensical now, which is still before its 1.0 release.
- **Answered 2026-11-04** (`considered`): the readers are Linux users new to vtium, as proposed; the builder is Zensical from the start, which this record now decides.
- **Inversion:** Between "the README is the documentation" (one long page on GitHub, nothing to build or publish, and a newcomer lost in it) and "a documentation project of its own" (a separate site with its own copies of the text, which drifts from the code). The answer keeps one copy of each page in `docs/`, readable on GitHub as it is, and makes the site a second view of the same files.

## Context

vtium's maintainer wants to circulate vtium to people outside the project, and wants them to have documentation they can read. Today the README is that documentation's front door, and at about 4,800 words it is the whole story in one page: what vtium draws, how it shares the display, how it is judged, how it compares with other consoles, and how to try it. Beyond the README, `docs/` holds eleven pages. Some are written for a user, such as `why-vtium.md`, `concepts.md`, `managing-vtium.md`, `config.md` and `heads.md`, and some for a contributor, such as `architecture.md`, `c-reference.md` and `vt-review.md`. Nothing tells a newcomer which page to read first, and two of the user pages mix steps with reference tables and explanation.

Diátaxis is a way of sorting documentation by what the reader is doing at the moment they open a page. A reader who is learning wants a tutorial: a lesson that works if followed step by step. A reader with a task wants a how-to guide. A reader who needs a fact wants reference. A reader who wants to understand wants explanation. In this record a mode is one of those four kinds of page.

A static site generator turns a folder of markdown files into a website of plain HTML, CSS and JavaScript files that any web server can serve. GitHub Pages serves such a site from a repository at no cost, at `OWNER.github.io/REPOSITORY`. The maintainer first named mkdocs with the Material theme, the usual pair for this: a menu along the top, search that runs in the reader's browser, and a light and a dark look. Material's authors put it into maintenance mode on 2025-11-05, with critical fixes and security updates promised until 2026-11-05, and moved their work to Zensical, a new generator that builds the same look and reads an existing `mkdocs.yml`. MkDocs itself has had no release since 1.6.1 in August 2024. Zensical is at 0.0.74, released 2026-10-30, and has given no date for its 1.0. Given those facts, the maintainer chose Zensical from the start. Measured on 0.0.74, Zensical's strict build fails on a link to a missing page, on a link to a missing anchor and on a link that leaves the folder it builds, and it does not report a page that the menu leaves out.

The README's two images are tier 3 goldens: vtium's minspec build at 640x480 and 320x240, in a bitmap test font. They prove that vtium works on a small machine, and that is what they show a newcomer first. The maintainer wants the first image to show what vtium can do on an ordinary monitor instead: outline fonts drawn smoothly, color emoji, Chinese, Japanese and Korean (CJK) text and scripts that need shaping, color, and pictures. The maintainer also wants the showcase to show plainly that a user can draw vtium in whatever font they want, bitmap or outline. Tier 3 is vtium's harness that boots vtium's install tree in a QEMU/KVM guest, drives it on a real kernel VT with a KMS display device, and captures the screen; a golden is a saved capture that a later run is compared against.

## Decision

### D1: the pages, sorted by what the reader is doing

The site's menu has seven sections, in this order: Home, Tutorials, How-to guides, Reference, Explanation, Decisions and For contributors. Each user page belongs to one of the four modes. The figure shows the map, with each new page marked.

![A map of the documentation site. Across the top, the Home section: the home page and the showcase. In the middle, four boxes in a two by two grid: the left column holds pages that teach by doing, the right column pages that explain; the top row is for a reader who is learning, the bottom row for a reader at work. Tutorials, top left: Try vtium on one VT, and Your first frit. Explanation, top right: Why vtium, which takes the README's comparison with the alternatives, and Concepts, which takes heads.md's two policies and the way back in. How-to guides, bottom left: Running vtium day to day, the index, then Install vtium and give it VTs, Configure vtium, Choose a font, Set up heads, and Find out what went wrong. Reference, bottom right: What vtium does, Keys, Configuration keys, The head file, Modes, systemd, logind and PID 1, and Specimens. Across the bottom, two more sections: Decisions, the decision records reached through their index, and For contributors: Architecture, The C reference, The VT review, and links to AGENTS.md and CONTRIBUTING.md on GitHub.](figures/ADR-1502-site-map.svg)

Every existing page keeps its path. The menu, not the folder a file sits in, says which mode a page belongs to, so the links that issues, commits, crates' READMEs and other pages already hold keep working. New pages sit beside the existing ones, directly in `docs/`.

This table maps each existing page to its mode and says what changes:

| Page | Mode on the site | What changes |
|---|---|---|
| `README.md` | not on the site: the front door on GitHub | shortened (D2); each of its sections moves to the page this table names for it |
| `docs/why-vtium.md` | Explanation | takes the README's comparison with the other consoles, its table and its sources |
| `docs/concepts.md` | Explanation | takes `heads.md`'s explanation of the two policies and of the operator's way back in (an operator's head, and recovery from outside) |
| `docs/managing-vtium.md` | How-to guides | split: its sections become the guides below, and the file stays at its path as the index of the How-to section, a short page that lists each guide |
| `docs/heads.md` | How-to guides | split: it keeps its scenarios, each a complete head file for one setup, as the guide "Set up heads"; its terms, the head file's syntax and match keys, the table of input routes and the steps for an invalid file move to the new reference page `head-file.md` |
| `docs/config.md` | Reference | none; it is generated from the key table in `crates/vtium-settings/src/keys.rs` |
| `docs/modes.md` | Reference | none; it is generated from the mode table in `crates/vtium-term` |
| `docs/systemd.md` | Reference | none |
| `docs/specimens/` | Reference | its generator writes each link that leaves `docs/` as a full address (D4) |
| `docs/heads/*.conf` | published as files | linked from `heads.md` and `head-file.md` |
| `docs/architecture.md` | For contributors | takes the README's "How vtium is built against other work" |
| `docs/c-reference.md` | For contributors | none |
| `docs/vt-review.md` | For contributors | takes the development VT from `managing-vtium.md`'s tips, where it repeats what this page already says about `make vt-install` |
| `docs/architecture/` | Decisions | none; `INDEX.md` is the section's page, and each record is reached through it |
| `docs/figures/` | published as images | its `README.md`, a note on how each figure is made, is left off the site, as is each `README.md` under `docs/architecture/*/figures/` |
| `docs/scripts/`, `docs/modes.tsv`, `docs/architecture/term/ADR-1000-*.tsv` | not published | a page links a TSV by its GitHub address, where GitHub draws it as a table |

`managing-vtium.md` becomes these guides:

- `install.md`, "Install vtium and give it VTs": its Installation section (Arch Linux, from a checkout, giving vtium a VT, moving from `kmscon`, upgrading and removing) and the tip on sharing VTs with `kmscon`.
- `configure.md`, "Configure vtium": its Configuration section and the tip on blanking from a script.
- `fonts.md`, "Choose a font": its Fonts section.
- `troubleshooting.md`, "Find out what went wrong": its Troubleshooting section.
- `keys.md`, "Keys", a reference page: its Usage section, which is a table of the keys vtium keeps, the selection, and the terminal type sessions get.

The README's sections go to these pages:

- "What it does", its status and "Small machines" with the measured figures go to a new reference page, `features.md`, "What vtium does". A passage that `concepts.md` already explains, such as how vtium hands the display over at a VT switch, is said once, in `concepts.md`, and `features.md` links it.
- "vtium beside the alternatives" goes to `why-vtium.md`.
- "How it is judged" is already explained in `concepts.md`, and in detail in `architecture.md`; the README keeps one sentence and a link.
- "Trying it" goes to the tutorial "Try vtium on one VT" and to `install.md`.
- "How vtium is built against other work" goes to `architecture.md`.
- "Provenance" and "Licence" stay in the README, shortened.

Two tutorials are new:

- **"Try vtium on one VT"** (`tutorial-try-vtium.md`). The reader installs the package, or builds from a checkout where there is no package, gives vtium one VT with `vtium console enable N`, switches to it with Ctrl+Alt+F*N*, logs in, tries what vtium draws, switches back to the desktop, and then takes the VT back with `vtium console disable N` and removes the package. It is written so that a reader who follows it never loses their desktop, and it says at each step what the reader should see.
- **"Your first frit"** (`tutorial-first-frit.md`). A frit is a font set file, `/etc/vtium/fonts/NAME.vtfrit`, that lists a font's sources in order. The reader runs `vtium config tui` in a vtium console, moves the highlight through the fonts and watches the live console change, saves a choice through `sudo vtium-config apply`, restarts that VT's vtium, and sees the font at start.

> Edited 2026-11-04: the work that builds the site keeps `tutorial-first-frit.md` as a short page that says the tutorial is coming, and leaves it out of the menu, in `not_in_nav`, until it is written from the maintainer's own session with vtium config; it then joins the Tutorials section. The site map figure shows the menu as it stands once the tutorial is written. This entry read "The reader runs `vtium config`", and `vtium config` alone prints its usage; `vtium config tui` opens the tool.

The For contributors section lists `architecture.md`, `c-reference.md` and `vt-review.md`, and links `AGENTS.md`, `CONTRIBUTING.md` and the crates' folders by their GitHub addresses. Contributor pages keep their present shape; this record sorts only the user pages into modes.

The Decisions section publishes every decision record, reached through `docs/architecture/INDEX.md`. The records hold the maintainer's quoted words and their reasons, which are public in the repository already; on the site a reader who asks "why does vtium do this?" can follow a link from an explanation page into the record.

### D2: the README and the home page

The README stays the front door on GitHub, and becomes short: the name; one sentence saying what vtium is; one paragraph saying why it exists; the showcase's hero image (D6) with its caption; the release and a one-line status with a link to `features.md`; a "Try it" line that links the tutorial; a "Documentation" list that links the site and each of its sections; one sentence on how vtium is judged; and the provenance and licence, shortened. It links pages under `docs/` by relative path, so it reads the same in any clone, and links the site once, at the top.

The site has its own home page, `docs/index.md`: the same sentence and paragraph, the hero image, four short doors into the four modes with one line each on who each is for, and links to the GitHub repository and its releases. The one-paragraph pitch is the one passage the README and the home page both hold, since each is the first page of its own place.

The two small-machine screens, the 640x480 and 320x240 goldens, stay in the README below the hero, as the proof that the same console runs on a 512 MB board, and appear on the home page beside the "What vtium does" door.

### D3: Zensical and its configuration

> Edited 2026-11-04: the maintainer chose "Zensical from the start, not Material" (`considered`). It read: "`mkdocs.yml` sits at the repository's root", built by mkdocs 1.6.1 with mkdocs-material 9.8.0, kept to what Zensical also reads, with a move to Zensical or ProperDocs when an advisory, a failed install or Zensical's 1.0 called for it.

Zensical builds the site. Its configuration is `zensical.toml`, at the repository's root. Zensical reads `mkdocs.yml` as well, but its own TOML file is the form its documentation describes and recommends for a new site, and it names functions, such as the heading slug function and the Mermaid fence, as plain strings, where `mkdocs.yml` names them with YAML tags that only a Python YAML loader understands. A Rust test reads the same file (below). In `zensical.toml`, under `[project]`:

- `docs_dir` is `docs`, and `site_dir` is `target/site`, inside the `target/` directory that git already ignores.
- `site_url` is `https://vtium.github.io/vtium/`, `repo_url` is the GitHub repository, and `edit_uri` points each page's edit link at its file on `main`.
- The theme has its menu tabs, sections and section index pages, a button back to the top, and a copy button on code blocks, and a light and a dark palette with a switch between them. `font = false` stops the theme from loading its fonts from Google, so the page uses the reader's own fonts.
- The only plugin is the built-in search, which runs in the reader's browser. The site uses no other plugin.
- The markdown extensions are tables, `toc` with a permalink on each heading, `attr_list` and `md_in_html` (for the `<img width>` the specimen pages use), `admonition`, and pymdown-extensions' `highlight`, `superfences` with the custom fence that draws Mermaid diagrams, and `slugs`. The `toc` extension forms each heading's anchor with `pymdownx.slugs.slugify` in lower case, which keeps letters outside ASCII as GitHub does, so a link to `page.md#a-heading` reaches the same heading on GitHub and on the site.
- `exclude_docs` leaves out `scripts/`, every `*.tsv` file and each `README.md` under a `figures/` directory. `adr.yaml` stays published as a file, since `INDEX.md` links it.
- `not_in_nav` names the pages that are reached by a link and not from the menu: the decision records, which `INDEX.md` links, the specimen profile pages, which `specimens/README.md` links, and `tutorial-first-frit.md` until it is written (D1).
- `zensical build --strict` fails on every warning: a link to a missing page, a link to a missing anchor, and a link that leaves `docs/`.

Zensical's strict build does not report a page that the menu leaves out, so a Rust test does. A small workspace crate, `tools/site`, holds one test that reads `zensical.toml` with the `toml` crate and fails when a markdown page under `docs/`, apart from the files `exclude_docs` leaves out, is neither in `nav` nor matched by `not_in_nav`, and when `nav` names a file that does not exist. It runs in `cargo test --workspace`, so it is part of the local gates and needs no Python.

The menu, in `zensical.toml`'s `nav`, reads:

> Edited 2026-11-04: "Your first frit" leaves the Tutorials section until it is written (D1). The listing below matches `zensical.toml`.

```toml
nav = [
  { "Home" = ["index.md", { "Showcase" = "showcase/README.md" }] },
  { "Tutorials" = [
      { "Try vtium on one VT" = "tutorial-try-vtium.md" },
  ] },
  { "How-to guides" = [
      "managing-vtium.md",
      { "Install vtium and give it VTs" = "install.md" },
      { "Configure vtium" = "configure.md" },
      { "Choose a font" = "fonts.md" },
      { "Set up heads" = "heads.md" },
      { "Find out what went wrong" = "troubleshooting.md" },
  ] },
  { "Reference" = [
      { "What vtium does" = "features.md" },
      { "Keys" = "keys.md" },
      { "Configuration keys" = "config.md" },
      { "The head file" = "head-file.md" },
      { "Modes" = "modes.md" },
      { "systemd, logind and PID 1" = "systemd.md" },
      { "Specimens" = "specimens/README.md" },
  ] },
  { "Explanation" = [
      { "Why vtium" = "why-vtium.md" },
      { "Concepts" = "concepts.md" },
  ] },
  { "Decisions" = "architecture/INDEX.md" },
  { "For contributors" = [
      { "Architecture" = "architecture.md" },
      { "The C reference" = "c-reference.md" },
      { "The VT review" = "vt-review.md" },
      { "AGENTS.md" = "https://github.com/vtium/vtium/blob/main/AGENTS.md" },
      { "CONTRIBUTING.md" = "https://github.com/vtium/vtium/blob/main/CONTRIBUTING.md" },
      { "The crates" = "https://github.com/vtium/vtium/tree/main/crates" },
  ] },
]
```

Zensical has not reached 1.0, so a newer release may change its configuration, its warnings or the pages it builds. The pin (D5) keeps every build on the version it names. A newer Zensical enters only through a commit that changes the pin, and that commit's pull request runs the strict build and the menu test and shows the built site. When a release breaks the build, the pin stays at the last version that built cleanly, or goes back to it if the break shows only later, and the change of pin waits for a release that builds.

### D4: links that work on GitHub and on the site

- A link from one page under `docs/` to another is relative, such as `[concepts.md](concepts.md#the-frit)`. It works on GitHub and on the site, and the strict build checks it and its anchor.
- A link that leaves `docs/` is a full address on `main`: `https://github.com/vtium/vtium/blob/main/PATH` for a file and `https://github.com/vtium/vtium/tree/main/PATH` for a folder. This covers `README.md`, `AGENTS.md`, `CONTRIBUTING.md`, `COPYING`, `LICENSE`, each crate's README, everything under `tools/`, source files and the TSV tables. It works in both places. A relative link out of `docs/` fails the strict build, because Zensical cannot find the page.
- A page names a GitHub issue as a link, `[the libseat issue](https://github.com/vtium/vtium/issues/N)`, as the README already does. A reference written as `vtium/vtium#N` reads as plain text on the site.
- The programs that generate pages, `config.md`, `modes.md` and the specimen pages, write their links by the same rules.
- An image a page shows lives under `docs/`. The README, outside `docs/`, shows the showcase's images from `docs/showcase/`.

The 23 relative links that leave `docs/` today change to full addresses in the work that builds the site.

### D5: publishing

> Edited 2026-11-04: the maintainer chose Zensical (`considered`). It read: the packages "are pinned in `tools/site/requirements.txt`, compiled from `tools/site/requirements.in` (`mkdocs==1.6.1`, `mkdocs-material==9.8.0`)", and the jobs ran `mkdocs build --strict`.

- A workflow of its own, `.github/workflows/docs.yml`, has two jobs. The `docs` job runs on every pull request and on every push to `main`: it installs the pinned packages, runs `docs/scripts/adr index -y` and fails if `docs/architecture/INDEX.md` changed, so no record is left out of the Decisions section, and runs `zensical build --strict`. The `deploy` job runs only on a push to `main`, after `docs` passes, and publishes the built site with GitHub's `upload-pages-artifact` and `deploy-pages` actions. Only the deploy job holds the `pages: write` and `id-token: write` permissions. A second push while a deploy runs waits for it.
- The site publishes `main` only. A pull request's build is checked and not published, and `integration` and `stage` are never published.
- The `docs` job is one of the CI jobs every pull request to `main` must pass. It is not one of the 16 gates in AGENTS.md's gate line, because it installs Python packages from PyPI; the menu test in `tools/site` does run in the gates. `make docs` runs `zensical build --strict` on this machine, in a virtual environment under `target/site-venv`, and `make docs-serve` runs `zensical serve`, which shows the site at `http://localhost:8000` and rebuilds it as the author edits. `zensical serve` has no strict mode, so `make docs` is the check. The author of a change under `docs/` runs `make docs` before review; a machine without network access cannot, and the CI job runs it regardless.
- The packages are pinned in `tools/site/requirements.txt`, compiled from `tools/site/requirements.in` (`zensical==0.0.74`, the newest release on 2026-11-04) with the hash of every file of every package, the dependencies included, and installed with `pip install --require-hashes`. Zensical ships a compiled wheel for each platform, and the hashes cover the x86_64 and aarch64 Linux wheels alike. The CI job uses Python 3.12, which Zensical accepts (it needs 3.11 or later) and which is the oldest Python tier 3's harness accepts. A change of the pin is a commit of its own.
- The address is `https://vtium.github.io/vtium/`.
- The repository's Pages setting must name GitHub Actions as the site's source before the first deploy. Only an administrator of the repository changes it, so the maintainer does, once.
- The theme draws a Mermaid diagram with Mermaid's script, which it loads from `unpkg.com` on a page that holds a diagram. That is the one file the site loads from elsewhere; serving the script from the site itself is a later change, if the maintainer wants no outside load at all.

### D6: the showcase screens

> Edited 2026-11-04: the maintainer chose Iosevka in place of JetBrains Mono, and asked for a scene that shows several fonts side by side, bitmap and outline (`considered`). It read: "JetBrains Mono, Noto Sans CJK, Noto Sans Devanagari, Noto Naskh Arabic, Noto Color Emoji, and Symbols Nerd Font Mono for the prompt's symbols", and seven scenes, without the fonts scene.

The showcase is a set of screens that show a newcomer what vtium draws on an ordinary monitor. Each is vtium's own output, captured by tier 3 from a VT of its QEMU/KVM guest.

- **The variant.** A new tier 3 variant, `showcase-1920x1080`, runs the default machine (4 vCPUs, 4096 MB) with a `virtio-vga` display whose EDID gives 1920x1080, and an install tree built with the `images` feature. It draws on the CPU renderer, vtium's default. It runs only when named.
- **The screens.** Each scene is a shell script, `tools/tier3/guest/screens/showcase-NAME.sh`, that writes fixed bytes, so every run draws the same screen. Each draws in Iosevka, with the other fonts below as fallbacks for what Iosevka lacks, apart from the fonts scene:
    1. `showcase-text`: prose and a code listing, with color and the underline styles.
    2. `showcase-emoji`: color emoji beside text, with sequences joined by zero-width joiners, flags, keycaps and skin tones.
    3. `showcase-scripts`: Chinese, Japanese and Korean, and Devanagari, each line with its English beside it.
    4. `showcase-fonts`: four fonts, each drawn by vtium in a band of its own that starts with the font's name and whether it is a bitmap or an outline font, then the same sentence, digits, symbols and a line of box drawing: Iosevka, Terminus as a PSF console font, GNU Unifont, and JetBrains Mono. Its caption says that vtium draws any bitmap or outline font the user names in a frit, and links the tutorial "Your first frit".
    5. `showcase-color`: the 16 colors, the 256-color palette, 24-bit gradients and vtium's named palettes.
    6. `showcase-boxes`: a dashboard drawn by the script with box drawing, block elements, Braille and sextants.
    7. `showcase-pictures`: a sixel picture and a kitty picture, both computed by a script in vtium's tree, in the way `t3_pictures.py` computes tier 3's test pictures.
    8. `showcase-shell`: a shell session the script writes byte for byte, a prompt, commands and their output, in the way tier 3's `small-console` screen does. It shows several of the above at once and is the hero image of the README and the home page.
- **The fonts scene.** A console draws in one font at a time, so `showcase-fonts` is four captures of the same scene. Before each, the script switches the console to that font's frit with OSC 50 and waits for the switch, as the acceptance pass does (ADR-1501 D6). `make showcase` crops each capture to the band the scene draws and stacks the four bands, at their own pixel size and unscaled, into one image. This is the one image `make showcase` composes, and its caption says it is four captures.
- **Only what vtium draws right.** A scene shows only what vtium draws correctly for a reader of that script or feature at the time of capture, and its caption says plainly what it shows. Until the bold and italic faces of outline fonts are built, no scene uses them. Arabic stays off the scripts screen until vtium draws right-to-left text in the order its reader reads it, and joins that screen in the same change that builds bidirectional text.
- **The size of the text.** The font size gives a grid of about 110 to 130 columns, so the text stays readable when a page shows the image at half its width.
- **Fonts.** Every font may be redistributed. Each one vtium does not carry in its own tree is pinned by URL and SHA-256, as the specimens pin theirs, reusing `tools/specimens/src/fonts.rs`'s pins where the two sets share a font, and carried into the guest on the run's disc, so each image names the exact version of each font. None comes from the guest's packages.

| Font | Version | License | Used for | Where it comes from |
|---|---|---|---|---|
| Iosevka Term | 34.9.1 | SIL Open Font License 1.1 | every scene's text, the hero included, and a band of the fonts scene | the project's release on GitHub, `PkgTTF-IosevkaTerm-34.9.1.zip`. Iosevka Term is the build of Iosevka its authors make for terminals, with arrows and symbols one cell wide. |
| Terminus | 4.49.1 | SIL Open Font License 1.1 | the PSF band of the fonts scene, `ter-v32n` (16x32 cells) | Arch Linux's `terminus-font` package in the archive snapshot tier 3 pins, which builds the PSF files from the BDF sources in the release |
| GNU Unifont | the one built into vtium | SIL Open Font License 1.1 (COPYING) | a band of the fonts scene, drawn at twice its size | vtium's own tree; nothing to fetch |
| JetBrains Mono | 2.304 | SIL Open Font License 1.1 | the second outline band of the fonts scene | the specimens' pin |
| Noto Sans CJK, Noto Sans Devanagari | the newest release when the work is built | SIL Open Font License 1.1 | the scripts scene, and fallback in the others | the Noto project's releases on GitHub |
| Noto Color Emoji | 2.051 | SIL Open Font License 1.1 | emoji | the specimens' pin |
| Symbols Nerd Font Mono | 3.4.0 | MIT | the prompt's symbols | the specimens' pin |

- **Nothing from other programs.** No scene shows another program's output, logo, artwork or photograph. Every byte and every picture comes from vtium's tree.
- **Not goldens.** The variant has no goldens, and its cases carry the status `capture`, which passes and judges nothing. A showcase image never decides whether vtium's output is right; the tiers do.
- **`make showcase`** builds the tree with `images`, runs the variant, and writes `docs/showcase/NAME.png` for each scene and `docs/showcase/README.md`, the showcase page. The page shows each image with its alt text and its caption, taken from the header comment of the scene's script, and ends with the build's commit, the date, the machine, and each font with its version, SHA-256 and license. It also copies the two small-machine goldens the README shows today, `tools/tier3/goldens/2d-640x480-psf16/sgr.png` and `tools/tier3/goldens/2d-320x240-psf8/small-console.png`, into `docs/showcase/small/`, and records their source paths and SHA-256 on the page. It rewrites an image only when its pixels changed.
- **Committed, and regenerated at each release.** Tier 3 does not run in vtium's CI, so the site's build cannot make the images, and they are committed. Before a release is tagged, `make showcase` runs on `integration`, and the images that changed go in a commit of their own that names each one. The maintainer reviews them in the pull request that takes the release to `main`, since they show vtium in public.

### D7: a video of vtium, later

A video or screen recording of vtium is a later how-to guide, "Record a VT", and nothing in this record builds it. On a real VT, ffmpeg's `kmsgrab` input reads the frame the display shows straight from KMS; it needs the `CAP_SYS_ADMIN` capability, so it runs as root, and its frames are copied to memory with ffmpeg's `hwdownload` filter and encoded to WebM for the site. The maintainer records on their own machine, and an agent never does, since the recording reads the display as root. Whether `kmsgrab` works inside tier 3's guest, where vtium draws into dumb buffers on `virtio-gpu`, is untested; if it does, recordings can come from the guest as the screenshots do.

### The maintainer's answers

The maintainer answered the record's questions on 2026-11-04 (`considered`):

1. **The readers:** Linux users new to vtium come first, as proposed.
2. **The builder:** Zensical from the start, pinned with hashes (D3, D5).
3. **The decision records:** published as the Decisions section, as proposed.
4. **The address:** `vtium.github.io/vtium`, as proposed.
5. **Arabic:** off the scripts screen until bidirectional text is built, as proposed.
6. **The showcase's font:** Iosevka, and a fonts scene that shows bitmap and outline fonts side by side (D6).
7. **Regenerating the showcase:** at each release, reviewed in the release's pull request, as proposed.
8. **Turning on Pages:** the maintainer sets the repository's Pages source to GitHub Actions, once, before the first deploy.

## C notes: the C reference's documentation

The C reference documented itself in its README and in two man pages written in DocBook, `docs/man/kmscon.1.xml.in` for the program and `docs/man/kmscon.conf.5.xml.in` for its configuration file, which its meson build turned into man pages. The fork added markdown notes under `docs/fork/` and a README beside each of its harnesses. It published no website.

## Where vtium departs from the C reference

- vtium publishes its documentation as a website, sorted by what the reader is doing.
- vtium has no man pages yet. Writing them is a decision of its own; `config.md`, which vtium generates from its key table, is where a man page for `vtium.conf` would be generated from.

## Consequences

### Positive

- A newcomer lands on a short page with a picture of what vtium draws, and a menu that tells them where to start: the tutorial that tries vtium on one VT.
- The fonts scene shows a reader, before they install anything, that vtium draws a console font and an outline font alike.
- Each page has one job, so a reader looking up a key or a configuration setting finds a table, and a reader following steps finds steps.
- Every broken link and anchor between pages fails a pull request's CI, and every page left out of the menu fails the gates, which catches the drift that moved files and renamed headings cause today unseen.
- The same files read on GitHub and on the site, and no page is copied.

### Negative

- Python, Zensical and its dependencies join vtium's toolchain for one job. Zensical is before its 1.0, so each change of its pin needs its own check of the built site, and a release that breaks the build holds the pin back until one builds.
- Python-Markdown, which Zensical uses, reads some markdown differently from GitHub: a list needs a blank line before it, and a list inside a list needs four spaces of indent. The strict build does not catch a list that renders wrong, so the work that builds the site reads every page on the built site once.
- Splitting `managing-vtium.md` and `heads.md` and shortening the README touches pages that other work changes at the same time, such as the work that adds the report command and the troubleshooting steps to `managing-vtium.md`. The split starts after that work lands.
- Each release adds a tier 3 run, eleven captures (four of them for the fonts scene), a review of them, and git keeps every version of every image.
- A page added to `docs/` must be added to the menu, or to `not_in_nav`, in the same change, or the menu test fails.

### Neutral

- The site shows `main`, so it describes the newest merged vtium, which may be ahead of the newest release; the home page says which release is the newest.
- The decision records appear on the site with the YAML at their top read as page metadata and hidden.
- `make docs` and `make docs-serve` join the Makefile; `make showcase` joins it beside `make specimens`. The root `Cargo.toml` lists `tools/site` as a workspace member, beside the other tools crates.
- The specimens stay what they are, the renderer's own drawing of vtium's test card at each display profile, and the showcase is a separate set of screens drawn on a VT.

## Alternatives Considered

- **Shorten the README and publish no site.** GitHub draws markdown already, so this costs nothing to build. Rejected: GitHub's view of a folder has no menu, no search and no order, so a newcomer still has to know which file to open, and the maintainer asked for a website.
- **mkdocs with Material for MkDocs, pinned.** This record's first proposal: mature, and the pair the maintainer first named. Rejected by the maintainer: Material receives no fixes after 2026-11-05, and MkDocs has had no release since 2024, so the site would start on a frozen toolchain and move later anyway.
- **`mkdocs.yml` as Zensical's configuration.** Zensical reads it, and it would keep MkDocs and ProperDocs, a continuation of MkDocs 1.6, able to build the site. Not chosen: the maintainer chose Zensical from the start, its own file is the form it documents, and the YAML form names functions with Python-only tags.
- **mdBook.** It is written in Rust, a single binary, and fits vtium's rule that its own tools are Rust. Not chosen: the maintainer named a generator of the mkdocs family, and mdBook's menu is a single sidebar with no tabs for the four modes; its link checking needs a plugin of its own. Zensical is a third-party build tool here, as QEMU is for tier 3, and the one check vtium adds to it, the menu test, is Rust.
- **Move the files into `tutorials/`, `how-to/`, `reference/` and `explanation/` folders.** Rejected: issues, commits and crates' READMEs cite the pages by path, and the menu already sorts them.
- **Showcase screens from tier 2 or the specimens.** Both draw vtium's renderer into memory, in CI or on any machine. Rejected for the showcase: the maintainer asked for vtium's own output on a VT, and tier 3 shows the console process, the kernel VT and the KMS display together. The specimens stay as the renderer's view.
- **The fonts scene as one capture, with each font in its own head or console.** A head shows one console at a time and one font in it, and a 1920x1080 display holds one head, so one capture cannot hold four fonts. Four captures stacked, each named on the image by vtium itself, show each font as vtium draws it.
- **Build the showcase in CI.** Rejected: tier 3 does not run in CI.
- **Screenshots from the maintainer's own monitor.** Real hardware, but neither repeatable nor scripted, and taking them reads the display as root. Kept for video (D7).
