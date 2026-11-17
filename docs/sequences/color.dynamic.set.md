<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 10, 11, 12 set; OSC 12 query; OSC 110, 111, 112

| Sequence | What it does |
|---|---|
| `OSC 10 ; spec ST` | set the default foreground |
| `OSC 11 ; spec ST` | set the default background |
| `OSC 12 ; spec ST` | set the cursor's color |
| `OSC 12 ; ? ST` | ask for the cursor's color |
| `OSC 110 ST` | reset the default foreground |
| `OSC 111 ST` | reset the default background |
| `OSC 112 ST` | reset the cursor's color |

## Parameters

- `spec`: A color as X11 writes it: `rgb:r/g/b` or `#rgb` with one to four hex digits for each channel, or an X11 color name such as `dark slate gray`. A `?` in its place asks for the color instead of setting it.

## What vtium does

vtium sets the color and redraws every cell in it. A string can list several specs, and each goes to the next number, so `OSC 10 ; red ; blue ST` sets the foreground and the background. A reset returns the color to the configured one.

A cursor with no color of its own draws its cell inverted, and the query then answers the foreground.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** themes set the foreground, background and cursor colors.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `osc_10_11_and_12_set_and_query` in [crates/vtium-term/tests/colors.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/colors.rs)
- `osc_110_111_and_112_reset_to_the_configured_palette` in [crates/vtium-term/tests/colors.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/colors.rs)

## Where to read more

To follow the work on this row, see issue [#38](https://github.com/vtium/vtium/issues/38).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
