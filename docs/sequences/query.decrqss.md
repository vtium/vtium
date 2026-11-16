<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# DECRQSS (DCS $ q); an unknown request gets the error form with no echo

| Sequence | What it does |
|---|---|
| `DCS $ q Pt ST` | DECRQSS: request a setting |

## Parameters

- `Pt`: The final bytes of the control function whose setting the program asks for, such as `m` for SGR or `r` for DECSTBM.

## What vtium does

vtium answers `DCS 1 $ r Pt ST`, where `Pt` is the sequence that sets the current value. It answers for SGR (`m`), DECSTBM (`r`), DECSLRM (`s`), DECSCUSR (`SP q`), DECSCL (`" p`), DECSCA (`" q`) and DECSACE (`* x`).

Any other request gets `DCS 0 $ r ST`, and the reply never repeats the request's bytes. For DECSCL, vtium reports 7-bit controls, because its replies stay 7-bit.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** programs read the terminal's settings, such as the cursor style or the scroll region.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [the VT510 reference manual](https://vt100.net/docs/vt510-rm/contents.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `decrqss_reports_the_left_and_right_margins` in [crates/vtium-term/tests/margins.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/margins.rs)
- `decrqss_reports_decsca_and_decsace` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- `decrqss_reports_every_underline_color_index_in_the_colon_form` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- `decrqss_reports_rapid_blink_conceal_overline_and_the_script` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- `decrqss_reports_settings_and_refuses_with_no_echo` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)
- `decrqss_reports_the_conformance_level` in [crates/vtium-term/tests/replies.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/replies.rs)

## Where to read more

To follow the work on this row, see issue [#12](https://github.com/vtium/vtium/issues/12).

To read why vtium does what it does here, see [ADR-1000, "Replies and policy"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#replies-and-policy).
