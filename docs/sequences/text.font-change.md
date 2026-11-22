<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Runtime font change and zoom: OSC 50

| Sequence | What it does |
|---|---|
| `OSC 50 ; Pt ST` | switch to the font set Pt |
| `OSC 50 ; ? ST` | ask which font set is in use |

## Parameters

- `Pt`: The name of a font set or a built-in font: 1 to 64 characters of `A-Z a-z 0-9 . _ + -`, not starting with `.`.

## What vtium does

vtium switches to the font set that Pt names. When the new font's character cell has another size, vtium resizes the grid to fit it. The query gets `OSC 50 ; NAME`, ended as it was asked, where NAME is the set in use. While the `allow-font-ops` key is off, vtium ignores both forms.

vtium takes only the name of one of its font sets, and ignores anything else, such as a path or an X font name. It also ignores a Pt that starts with `#`, the form that picks from a font menu. See [OSC strings and the Linux console's sequences in vtium-term's README](https://github.com/vtium/vtium/blob/main/crates/vtium-term/README.md#osc-strings-and-the-linux-consoles-sequences).

## Status

- **Status:** supported. vtium implements this as the specifications below define it, apart from the departure above.
- **Why:** programs and users change the font and zoom while the console runs.
- **Note:** late; a program setting the grid's size is the XTWINOPS 8 row.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `a_console_hidden_during_the_wait_answers_once_shown_in_the_new_cell` in [crates/vtium/src/app.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app.rs)
- `a_console_shown_during_the_wait_holds_its_replies_until_the_new_cell` in [crates/vtium/src/app.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app.rs)
- `a_hidden_console_names_the_font_it_last_drew_in_until_shown` in [crates/vtium/src/app.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app.rs)
- `a_hidden_console_takes_the_late_cell_and_its_name_together_once_shown` in [crates/vtium/src/app.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app.rs)
- `a_switch_answers_in_the_kept_cell_once_the_helper_fails` in [crates/vtium/src/app.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app.rs)
- `a_switch_answers_once_its_late_cell_is_known` in [crates/vtium/src/app.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app.rs)
- `osc_50_switches_back_to_vconsoles_font_by_the_name_its_query_reported` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `osc_50_switches_to_a_frit_and_its_helper_is_asked` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `a_hidden_consoles_query_names_the_font_it_last_drew_in_until_shown` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `a_sessions_end_returns_its_switch_to_the_configured_font` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `a_sessions_end_undoes_its_zoom` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `an_osc_50_switch_back_asks_again_after_a_timeout_but_not_a_refusal` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `an_osc_50_switch_holds_the_programs_replies_until_the_late_cell` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)

## Where to read more

To follow the work on this row, see issue [#44](https://github.com/vtium/vtium/issues/44).

To read why vtium does what it does here, see [ADR-1200 D5](../architecture/font/ADR-1200-glyphs-are-generated-at-run-time-in-a-sandboxed-helper-from-a-profile-s-frit.md#d5-vtium-vitrify-set-through-osc-50).
