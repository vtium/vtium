<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# ENQ answerback

| Sequence | What it does |
|---|---|
| `ENQ` | enquiry: ask for the answerback message |

## What vtium does

vtium's answerback message is empty, so it sends nothing in reply to ENQ.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs that send ENQ expect an answerback, and vtium answers with nothing a program could misuse.
- **Note:** an empty answerback.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `enq_answers_nothing` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
