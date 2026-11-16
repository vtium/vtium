<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# SGR 6: rapid blink

| Sequence | What it does |
|---|---|
| `CSI 6 m` | rapid blink |

## Parameters

- `Pm`: One or more SGR codes separated by `;`. vtium applies them in order, so `CSI 1 ; 4 m` sets bold and underline. The default is 0, which resets every attribute.

## What vtium does

vtium blinks the text that follows SGR 6 at 150 blinks a minute, faster than SGR 5's blink. SGR 25 stops it. A cell that carries both blinks blinks at the rapid rate.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** ECMA-48 defines it, and the maintainer wants each SGR blink drawn.
- **Note:** vtium blinks SGR 6 at a phase of its own, 200 ms shown and 200 ms hidden: 150 blinks a minute, the slowest rate ECMA-48 §8.3.117 gives SGR 6 ("150 per minute or more"), and 2.5 flashes a second, under WCAG 2.2's limit of three; the timer runs only while a shown cell carries SGR 6, and blink off, blink-text=off and a VT that is not vtium's stop it as they stop SGR 5's blink; a cell that carries SGR 5 and SGR 6 blinks at SGR 6's phase.
- **Specifications:** [ECMA-48](https://ecma-international.org/publications-and-standards/standards/ecma-48/) §8.3.117; WezTerm es

## Tests

These tests check what vtium does with the sequences on this page.

- `a_rapid_phase_change_redraws_its_cells` in [crates/vtium-render/tests/rapid_blink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/rapid_blink.rs)
- `rapid_blink_hides_in_its_own_phase` in [crates/vtium-render/tests/rapid_blink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-render/tests/rapid_blink.rs)
- `rapid_blink_is_set_and_cleared` in [crates/vtium-term/tests/sgr.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/sgr.rs)
- `rapid_blink_flips_every_200_ms_while_a_cell_carries_it` in [crates/vtium/src/app/rapid.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/rapid.rs)
- `the_rapid_timer_stops_when_no_cell_carries_sgr_6` in [crates/vtium/src/app/rapid.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/rapid.rs)
- `what_stops_the_text_blink_stops_the_rapid_blink` in [crates/vtium/src/app/rapid.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/rapid.rs)
- `the_rapid_phase_flips_every_200_ms_while_a_cell_carries_sgr_6` in [crates/vtium/src/blink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/blink.rs)

## Where to read more

To follow the work on this row, see issue [#37](https://github.com/vtium/vtium/issues/37).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
