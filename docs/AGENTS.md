# AGENTS.md for docs/

This file covers work under `docs/`. The root [AGENTS.md](../AGENTS.md) still governs, and its Writing section sets how every page reads. This file does not repeat it.

## Pages for users and pages for contributors

The site's menu, in `zensical.toml`, sorts the pages. The pages under "For contributors" are for contributors: `architecture.md`, `vt-review.md` and `vt-review-linux.md`. So are the decision records under `architecture/` and the acceptance passes under `acceptance/`. Every other page is for users.

The two kinds differ in two ways:

- A user page links the GitHub issue that tracks a feature or a gap. A contributor page links the decision record behind it.
- A user page talks to the reader as "you". A contributor page, a record and a code comment stay in the third person.

## Generated pages

Some pages are generated from the code. Never edit them by hand. Change their source, then run the command that writes them again.

| Page | Source | Command |
|---|---|---|
| `config.md` | `crates/vtium-settings/src/keys.rs` and `crates/vtium/src/config/doc.rs` | `VTIUM_BLESS=1 cargo test -p vtium generated_files` |
| `modes.md` and `modes.tsv` | `crates/vtium-term/src/terminal/modes.rs` | `VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents` |
| `specimens/` | the test card and its display profiles | `make specimens` |
| `showcase/` | tier 3's showcase screens | `make showcase` |

The tests behind the first two commands fail when a committed page differs from its source. `make specimens-check` draws the specimens again and compares them.

## A new page

Every new page needs a place in `zensical.toml`. Give it an entry in the menu, `nav`, or a pattern in `not_in_nav` when another page links it instead. `cargo test -p vtium-site` fails on a page that has neither. `make docs` builds the site in strict mode, so it fails on a broken link or anchor.

Run `docs/scripts/prose --table` on each page you change, before and after you edit it. It lists each sentence over 30 words and each page whose sentences average over 20.

## Figures

A figure lives in `figures/`, and `figures/README.md` has a row for it that says what it shows and how it is made. A figure is made in one of two ways:

- A hand-written SVG uses the vocabulary of the figures already there: their fonts, colors, line styles and legend. It has a claim file beside it, `figures/NAME.claims.tsv`, which lists each box, arrow, state and transition with the file and the symbol it rests on. Render it with `rsvg-convert NAME.svg -o OUT.png` and look at it at its natural size, on a light and on a dark background.
- A picture of vtium's own output is copied from a golden or drawn by vtium's renderer, by the command its README row gives, such as `make tui-shots` or tier 2's render. Never edit such a picture by hand. Draw it again.

A page whose sentences rest on the code can have a claim file too, such as `concepts/security.claims.tsv`.

## When the code changes

Run `docs/scripts/claims`. It names each claim file and row whose file is gone, or whose symbol is no longer defined there. Update the page or the figure that row describes in the same change, and the row with it. When a figure changes, render it again and check it at its natural size.

A claim file names a symbol, never a line number: a `fn`, a type, a `const`, a `mod`, a key of the key table written as `key NAME`, or a unit file's directive. The script's own help, `docs/scripts/claims --help`, lists what each kind of symbol must match.
