<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 4 set, OSC 104 reset

| Sequence | What it does |
|---|---|
| `OSC 4 ; c ; spec ST` | set palette color c |
| `OSC 104 ; c ST` | reset palette color c |

## Parameters

- `c`: The palette color's index, from 0 to 255. OSC 4 takes several `c ; spec` pairs, and OSC 104 several indexes separated by `;`. The default is every color, for OSC 104 with no index.
- `spec`: A color as X11 writes it: `rgb:r/g/b` or `#rgb` with one to four hex digits for each channel, or an X11 color name such as `dark slate gray`. A `?` in its place asks for the color instead of setting it.

## What vtium does

OSC 4 sets each palette color it names, and text already on the screen in that color changes with it. vtium skips an index past 255 and a spec it cannot read, such as `rgbi:`. OSC 104 returns each color it lists to the palette that vtium.conf sets.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** themes and programs set the 256 palette colors.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `osc_104_resets_palette_colors_to_the_configured_palette` in [crates/vtium-term/tests/colors.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/colors.rs)
- `osc_4_sets_palette_colors` in [crates/vtium-term/tests/colors.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/colors.rs)

## Where to read more

To follow the work on this row, see issue [#38](https://github.com/vtium/vtium/issues/38).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
