<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 9, OSC 777 notify, OSC 99: desktop notifications

| Sequence | What it does |
|---|---|
| `OSC 9 ; Pt ST` | show a notification |
| `OSC 777 ; notify ; Pt ST` | show a notification with a title |
| `OSC 99 ; Pt ST` | kitty's notifications |

## Parameters

- `Pt`: For OSC 9, the notification's text. For OSC 777, the title and the body, separated by `;`. For OSC 99, kitty's metadata, then `;` and the payload.

## What vtium does

vtium reads these strings and does nothing with them. OSC 99's queries get no reply.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** a VT has no notification service to show them.
- **Note:** OSC 99's queries get no reply.
- **Specifications:** [foot-ctlseqs(7)](https://codeberg.org/dnkl/foot/src/branch/master/doc/foot-ctlseqs.7.scd); kitty desktop-notifications

## Tests

These tests check what vtium does with the sequences on this page.

- `the_osc_strings_vtium_does_not_take_change_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#39](https://github.com/vtium/vtium/issues/39).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
