<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 13, 14, 113, 114: pointer colors

| Sequence | What it does |
|---|---|
| `OSC 13 ; spec ST` | set the pointer's foreground |
| `OSC 14 ; spec ST` | set the pointer's background |
| `OSC 113 ST` | reset the pointer's foreground |
| `OSC 114 ST` | reset the pointer's background |

## Parameters

- `spec`: A color as X11 writes it: `rgb:r/g/b` or `#rgb` with one to four hex digits for each channel, or an X11 color name such as `dark slate gray`. A `?` in its place asks for the color instead of setting it.

## What vtium does

vtium draws the mouse pointer in these two colors. A `?` in place of a spec asks for the color. While a color is unset, the query answers the default foreground for the pointer's foreground and the default background for its background.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** vtium draws a pointer, so programs may set its colors.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `osc_13_and_14_set_query_and_reset_the_pointer_colors` in [crates/vtium-term/tests/colors.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/colors.rs)

## Where to read more

To follow the work on this row, see issue [#38](https://github.com/vtium/vtium/issues/38).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
