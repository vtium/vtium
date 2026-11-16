<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# XTGETTCAP (DCS + q), names printed in uppercase hex from the table

| Sequence | What it does |
|---|---|
| `DCS + q Pt ST` | XTGETTCAP: request terminfo capabilities |

## Parameters

- `Pt`: The names of the capabilities, each written in hex and separated by `;`.

## What vtium does

vtium answers from tables compiled from the terminfo entries it installs, `xterm-vtium` and `kmscon`, each under its own entry's name. It answers `DCS 1 + r Pt ST`. In the reply, `Pt` holds `name=value` for each capability, both in uppercase hex, or the name alone for a boolean capability.

vtium stops at the first name it does not know. If it knows none of the names, it answers `DCS 0 + r ST`. While `TERM` names any other entry, vtium knows no name, so it answers `DCS 0 + r ST` to every request. Each name in the reply comes from vtium's table, never from the request.

vtium answers a bounded number of names in one request, and keeps the reply under a bounded length.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs such as vim and tmux read terminfo capabilities from the terminal itself.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `a_table_past_its_bounds_is_refused` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- `xtgettcap_answers_from_the_table_in_uppercase` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- `xtgettcap_answers_sixteen_names` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- `xtgettcap_replies_up_to_its_bound` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- `each_entrys_name_gets_its_own_table` in [crates/vtium/src/termcap/terminfo.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/termcap/terminfo.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "The terminfo entry and TERM"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-terminfo-entry-and-term).
