<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# The Linux console's cursor blink interval, CSI 16 ; n ]

| Sequence | What it does |
|---|---|
| `CSI 16 ; Ps ]` | set the cursor's blink interval |

## Parameters

- `Ps`: How long each phase of the blink lasts, in milliseconds, from 50 to 65535. Any other value returns the interval to the default. The default is the default interval, 500 ms.

## What vtium does

vtium blinks the cursor of the console that receives the sequence with phases of Ps milliseconds. RIS returns the interval to the default, while DECSTR and the alternate screen keep it.

Blinking text keeps the `blink` key's 500 ms. The interval applies while the cursor blinks, so a steady cursor stays steady.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs written for the Linux console set how fast the cursor blinks with it.
- **Note:** vtium blinks the cursor of the console that receives the sequence with phases of n ms, n from 50 to 65535, as the Linux console takes it; any other n, n = 0 among them, or no n returns the interval to the default, the `blink` key's 500 ms, as the Linux console returns to its own default; RIS returns it to the default too, as the Linux console's reset does, and DECSTR, which the Linux console lacks, keeps it; the alternate screen keeps it; blinking text keeps the `blink` key's interval, and with `blink` off the cursor does not blink.
- **Specifications:** [console_codes(4)](https://man7.org/linux/man-pages/man4/console_codes.4.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `csi_16_sets_the_cursor_blink_interval_in_milliseconds` in [crates/vtium-term/tests/blank.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/blank.rs)
- `ris_returns_the_cursor_blink_interval_to_the_default` in [crates/vtium-term/tests/blank.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/blank.rs)
- `a_new_interval_never_stretches_the_running_phase` in [crates/vtium/src/app/cursor_blink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/cursor_blink.rs)
- `the_cursor_blinks_at_the_interval_a_program_set_without_a_key` in [crates/vtium/src/app/cursor_blink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/cursor_blink.rs)
- `the_cursor_takes_the_interval_again_when_the_vt_comes_back` in [crates/vtium/src/app/cursor_blink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/cursor_blink.rs)
- `with_blink_off_the_interval_blinks_nothing` in [crates/vtium/src/app/cursor_blink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/cursor_blink.rs)
- `the_cursor_timer_runs_at_the_interval_a_program_set` in [crates/vtium/src/blink.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/blink.rs)
- [tools/conformance/scenarios/reset-cursor-blink.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/reset-cursor-blink.scenario)

## Where to read more

To follow the work on this row, see issue [#404](https://github.com/vtium/vtium/issues/404).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
