<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTPUSHSGR, XTPOPSGR (CSI # {, CSI # }, and CSI # p, CSI # q)

| Sequence | What it does |
|---|---|
| `CSI Pm # {` | XTPUSHSGR: push the pen |
| `CSI Pm # p` | XTPUSHSGR, the other form |
| `CSI # }` | XTPOPSGR: pop the pen |
| `CSI # q` | XTPOPSGR, the other form |

## Parameters

- `Pm`: The parts of the pen to save, each named by its SGR code: 1 bold, 2 dim, 3 italic, 4 underline, 5 blink, 7 inverse, 8 conceal, 9 strike, 21 double underline, 30 the foreground and 31 the background. The default is every part of the pen that SGR sets.

## What vtium does

XTPUSHSGR saves the pen on a stack, and XTPOPSGR restores the parts that the push named. The underline's style and color count as one part, which 4 and 21 both name. The overline and the superscript and subscript are not in the list, so a pop restores them only when the push saved every part.

The stack holds a fixed number of pens. A push past that number saves nothing, and the pop that pairs with that push restores nothing. A pop of an empty stack does nothing.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** a program saves the pen before it draws and restores it after; xterm and contour both document it.
- **Note:** it pairs with the color stack.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); contour save-and-restore-sgr-attributes

## Tests

These tests check what vtium does with the sequences on this page.

- `a_push_of_everything_and_its_pop_restore_the_pen` in [crates/vtium-term/tests/stacks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/stacks.rs)
- `a_push_past_the_sgr_bound_keeps_nothing` in [crates/vtium-term/tests/stacks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/stacks.rs)

## Where to read more

To follow the work on this row, see issue [#58](https://github.com/vtium/vtium/issues/58).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
