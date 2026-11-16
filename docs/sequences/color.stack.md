<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Color stacks: XTPUSHCOLORS, XTPOPCOLORS, XTREPORTCOLORS (CSI # P, Q, R); kitty's OSC 30001 and 30101

| Sequence | What it does |
|---|---|
| `CSI Pm # P` | XTPUSHCOLORS: push the colors |
| `CSI Pm # Q` | XTPOPCOLORS: pop the colors |
| `CSI # R` | XTREPORTCOLORS: report the stack |
| `OSC 30001 ST` | push the colors |
| `OSC 30101 ST` | pop the colors |

## Parameters

- `Pm`: The entries of the stack to store into or restore from, numbered from 1 to 10. Several numbers act in turn, and 0 pushes or pops. The default is 0.

## What vtium does

A push stores every color in a stack entry: the 256 palette colors, the defaults, and the cursor's, pointer's and selection's colors. A pop restores them, and text on the screen changes with them. OSC 30001 and 30101 push and pop the same stack as XTPUSHCOLORS and XTPOPCOLORS.

The stack holds 10 entries. A push with the stack full, and a pop of an empty stack, do nothing. XTREPORTCOLORS answers `CSI ? Pc ; Ps # Q`, with the current entry and the number of entries stored.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** a program saves the palette before it changes it and restores it after.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [foot-ctlseqs(7)](https://codeberg.org/dnkl/foot/src/branch/master/doc/foot-ctlseqs.7.scd); kitty color-stack

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `a_push_past_the_color_bound_keeps_nothing` in [crates/vtium-term/tests/stacks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/stacks.rs)
- `osc_30001_and_30101_push_and_pop_the_colors` in [crates/vtium-term/tests/stacks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/stacks.rs)
- `xtpushcolors_and_xtpopcolors_restore_the_palette` in [crates/vtium-term/tests/stacks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/stacks.rs)
- `xtreportcolors_reports_the_entry_and_the_palettes_stored` in [crates/vtium-term/tests/stacks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/stacks.rs)

## Where to read more

To follow the work on this row, see issue [#58](https://github.com/vtium/vtium/issues/58).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
