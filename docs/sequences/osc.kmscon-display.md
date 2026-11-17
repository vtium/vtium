<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# kmscon's OSC setBackground and setForeground

| Sequence | What it does |
|---|---|
| `OSC setBackground ST` | hand the display to a graphical program |
| `OSC setForeground ST` | take the display back |

## What vtium does

A program writes each string with no number after OSC, and BEL may end it as ST does. vtium takes both while the key `allow-display-ops` is on, which it is by default. Only a console that a head shows may send setBackground. Neither string gets a reply.

On the kernel's VT, setBackground gives up every display card the sending head draws on. vtium draws again after setForeground from the same console, when that console's session ends, or when vtium's VT is shown again. Under a seat manager, vtium ignores setBackground and logs that it did.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs written for the released kmscon hand the display to a graphical program and take it back.
- **Note:** vtium reads both behind the key allow-display-ops, on by default. Only a console that a head shows may send setBackground. On the kernel's VT it gives up every card the sending head draws on until setForeground from the same console, that console's session ending, or vtium's VT shown again; under a seat manager vtium ignores it and logs that it did (ADR-1401 D16). The shown console of each head the hand-over wholly pauses gets a focus-out, and a focus-in at its end (ADR-1401 D5). A hand-over of single ports through a DRM lease is tracked separately.
- **Specifications:** the released kmscon's private OSC strings; ADR-1000, "Replies and policy"

## Tests

These tests check what vtium does with the sequences on this page.

- `each_string_waits_for_the_host_ended_by_st_or_bel` in [crates/vtium-term/tests/display.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/display.rs)
- `only_the_two_exact_strings_are_requests` in [crates/vtium-term/tests/display.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/display.rs)
- `the_input_call_returns_after_each_request_so_the_host_takes_both_in_order` in [crates/vtium-term/tests/display.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/display.rs)
- `with_display_operations_off_both_strings_are_ignored` in [crates/vtium-term/tests/display.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/display.rs)
- `with_allow_display_ops_off_set_background_is_ignored` in [crates/vtium/tests/hand_over.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/hand_over.rs)

## Where to read more

To follow the work on this row, see issue [#36](https://github.com/vtium/vtium/issues/36).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
