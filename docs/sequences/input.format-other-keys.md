<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTFMTKEYS, XTQFMTKEYS: formatOtherKeys

| Sequence | What it does |
|---|---|
| `CSI > Pp ; Pv f` | XTFMTKEYS: choose the format of modified keys |
| `CSI ? Pp g` | XTQFMTKEYS: ask for that format |

## Parameters

- `Pp`: The resource: 0 formatKeyboard, 1 formatCursorKeys, 2 formatFunctionKeys, 3 formatKeypadKeys, 4 formatOtherKeys, 6 formatModifierKeys, and 7 formatSpecialKeys.
- `Pv`: The value XTFMTKEYS gives the resource. Without it, the resource returns to its initial value.

## What vtium does

vtium reads both sequences and does nothing with them. XTQFMTKEYS gets no reply.

## Status

- **Status:** ignored. vtium reads the sequence and does nothing with it.
- **Why:** only xterm documents it.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `key_formats_user_defined_keys_and_hilite_tracking_are_not_taken` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
