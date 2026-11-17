<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# 8-bit C1 controls, S7C1T and S8C1T

| Sequence | What it does |
|---|---|
| `0x80 to 0x9F` | the C1 controls as single bytes |
| `ESC SP F` | S7C1T: send 7-bit controls |
| `ESC SP G` | S8C1T: send 8-bit controls |

## What vtium does

In 8-bit input vtium acts on the C1 controls sent as single bytes, such as 0x9B for CSI. In UTF-8 input it ignores U+0080 to U+009F, so a program that strips ESC from untrusted text stops every control. Each C1 control also works in its 7-bit form, ESC and a second byte.

vtium accepts S7C1T and S8C1T, and its replies stay 7-bit either way. A program that reads UTF-8 would take an 8-bit control for ill-formed text.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs that send 8-bit controls, and the switch between 7-bit and 8-bit replies, rely on it.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `c1_controls_and_their_announcers` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issues [#9](https://github.com/vtium/vtium/issues/9) and [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
