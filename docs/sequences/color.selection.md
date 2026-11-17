<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 17, 19, 117, 119: selection colors

| Sequence | What it does |
|---|---|
| `OSC 17 ; spec ST` | set the selection's background |
| `OSC 19 ; spec ST` | set the selection's foreground |
| `OSC 117 ST` | reset the selection's background |
| `OSC 119 ST` | reset the selection's foreground |

## Parameters

- `spec`: A color as X11 writes it: `rgb:r/g/b` or `#rgb` with one to four hex digits for each channel, or an X11 color name such as `dark slate gray`. A `?` in its place asks for the color instead of setting it.

## What vtium does

With no background set, vtium draws the selection inverted. With a background set, the selected text keeps its own color unless OSC 19 sets a foreground too.

A `?` in place of a spec asks for the color. While a color is unset, the query answers the color vtium draws in: the foreground for the background, and the background for the foreground.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** themes set the selection's colors.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `the_selection_draws_in_its_colors` in [crates/vtium-render/tests/colors.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/colors.rs)
- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `osc_17_and_19_color_the_selection` in [crates/vtium-term/tests/colors.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/colors.rs)

## Where to read more

To follow the work on this row, see issue [#38](https://github.com/vtium/vtium/issues/38).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
