# AGENTS.md for crates/

This file covers work under `crates/`: code comments, the comments of the shipped configuration files, strings on screen, the coverage of ADR-1000's rows, and the tests. The root [AGENTS.md](../AGENTS.md) still governs. [docs/AGENTS.md](../docs/AGENTS.md#writing) sets how every comment and message reads, and [tools/AGENTS.md](../tools/AGENTS.md) says how vtium is judged and how fuzz targets run.

## Code comments

A code comment explains in its own plain words what the code does and why. A reader understands it without following any reference, so it never leans on a decision record to finish a sentence. The maintainer decided this on 2026-11-13, after spot checks found comments that assume the reader already knows each record. Decision-record references go in a block of their own at the end of the comment, headed `References:`, with one line for each record that says what the record decides for this code:

```rust
/// Tries to wake the displays again once a second while this VT is shown.
/// Another program may let go of the display at any moment, and nobody may
/// press a key, so vtium keeps trying on its own instead of waiting for input.
///
/// References:
/// - ADR-1000, "Replies and policy": the slow retry and its once-a-second pace.
/// - ADR-1400 D1: why vtium needs DRM master to draw.
```

Write "ADR-1401 D9 decides it" in the block, never "(ADR-1401 D9)" in the middle of a sentence. New and changed comments follow this now. The existing comments move to it crate by crate, and a check that fails on a citation outside a References block comes with that work. See #403.

## The shipped configuration files

The configuration files vtium ships, `crates/vtium/install/vtium.conf.example` (generated from the key table) and the head file template `crates/vtium-settings/templates/heads.conf`, wrap the prose of their comments at 78 columns and break each description into paragraphs, because people read them in a terminal editor. vtium-config keeps those comments as they are when it edits a vtium.conf. Source, docs and records keep one paragraph per line.

## Strings on screen

A string on screen is any text vtium, vtium-config or `vtium check-config` shows a person: a notice on the console, an error about vtium.conf, a help page, a log line read after a failed start. Whoever writes or rewords one follows these rules.

1. **The topic comes first.** Say what the message is about, then the details and what to do. Write "This rule has no `->`. A rule reads `display MATCH -> TARGET` or `input MATCH -> TARGET`.", not "A rule reads `display MATCH -> TARGET` or `input MATCH -> TARGET`, and this one has no `->`."
2. **Use the reader's words, not the project's.** Write "font set" for a frit and "connected" for a device that is present: "Look big uses the font set monoid", not "The look big names the frit monoid". A project term that stays, such as head or look, is explained the first time a screen uses the term.
3. **Name things in full.** Write "the font helper", not "the helper", and "character cell", not "cell": "cannot start the font helper /usr/bin/vtium-config", not "cannot start /usr/bin/vtium-config".
4. **Say what something can do, from the reader's side.** Write "The font can display 40 of the 51 characters in the preview.", not "It draws 40 of the specimen's 51 characters itself."
5. **Keep "this terminal" apart from "the console".** "This terminal" is the one vtium-config runs in; "the console" is the vtium console being set up. Write "Live trial is off: this terminal is not a vtium console, so t cannot try a font on it."
6. **Follow the Writing section of [docs/AGENTS.md](../docs/AGENTS.md#writing).** Whole sentences, plain and idiomatic words, and who does what: "Because of this error, vtium ignores this rule.", not "For this error vtium drops this rule alone."

## The target set and its coverage

An ADR, or Agent Decision Record, is a document under `docs/architecture/` that records a decision. ADR-1000, in `docs/architecture/term/`, declares which capabilities vtium supports, ignores or refuses. It does so in two tables: `ADR-1000-modes.tsv` and `ADR-1000-features.tsv`. The target set is the list of what vtium aims to support: the capabilities terminals generally support, plus the maintainer's own list.

Each row has an `id` (such as `DEC-2026`, `ANSI-4` or `sgr.dim`), a `status`, a layer, and the issue that carries the work. The `status` is one of supported, partial (part of it is built), planned (vtium will build it, and its issue says how), weighed (another issue decides whether vtium builds it), ignored (accepted with no effect) or refused (deliberately not built). A mode whose note says it is not in vtium's mode table is one vtium does not recognize, and DECRQM answers it 0. The layer is the stage of the plan the row lands in, from the tracker, vtium/vtium#47, the issue that lists the plan and its order: minspec, text, images, gpu, seat or top. Rows set the scope of work: the rows an issue lists are its acceptance list, and the issue's Scope section names the crate that implements them. To find a row's id, run `grep -i <name> docs/architecture/term/ADR-1000-*.tsv`. For example, DECAWM is `DEC-7`.

When a test drives a row's behaviour through the crate that implements it and checks the result, put the tag `// covers: <id>` above that test's attribute, or `# covers: <id>` in a replay file. A test of a lower layer that the row depends on gets no tag. `docs/scripts/coverage` reads the tags and checks them against the tables:

| Command | What it does |
|---|---|
| `docs/scripts/coverage` | Fails if it finds an unknown, duplicate or malformed id, a tag above a function that is not a test, a tag that is close to the right form but that it cannot read, or a `tests` column that does not match the tags. CI runs it. |
| `docs/scripts/coverage --update` | Rewrites each row's `tests` column from the tags, then regenerates `tools/conformance/COVERAGE.md` from those columns through cargo. Run `docs/scripts/coverage --update` after you add or move a tag, and commit both generated files. A second run changes nothing. To resolve a merge conflict in the TSVs' `tests` column or in `COVERAGE.md`, run `docs/scripts/coverage --update` again. |
| `docs/scripts/coverage --layer minspec --require` | Fails while any minspec row that needs a test has none. Every row needs one, apart from a mode whose note says it is not in vtium's mode table. Minspec is the default, smallest build, and the minspec slice is done when this passes. It joins CI when the vtium binary lands (#21). |

The docstring of `docs/scripts/coverage` explains how feature ids are formed. Once an id is committed it never changes, apart from the one rename that ADR-1000 records (#114). A row's `status` records the decision, and the status changes only through an edit to ADR-1000. A row's coverage comes from the tags, and nobody edits the coverage by hand.

## Testing rules

- Before you delete a test because the golden or other reference that the test compared against is going away, check which defects only that test would catch, and keep a test that still catches each of them.
- A stand-in (a stub or fake) that a test uses in place of a real part must check what the real part checks. A test's stand-in for the glyph helper once accepted any frit (font set file) name, so the real helper's refusal of a bad name surfaced only in tier 3.
- Fix a bug by fixing the state that is wrong, never with timing: no delays, retries or "redraw a moment later". A test checks that state right after the event that should change the state, and counts turns or frames instead of measuring wall time. When a test has to wait for another thread, bound the wait only so the test fails instead of hanging. A console switch that kept drawing the old console was fixed by resetting what each display remembered drawing, not by redrawing later.
