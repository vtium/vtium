<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# OSC 133 prompt marks

| Sequence | What it does |
|---|---|
| `OSC 133 ; A ST` | a prompt starts |
| `OSC 133 ; B ST` | the user's input starts |
| `OSC 133 ; C ST` | the command's output starts |
| `OSC 133 ; D ; Ps ST` | the command ends, with its exit status |
| `OSC 133 ; I ST` | the user's input starts, as B |
| `OSC 133 ; L ST` | move to a fresh line |
| `OSC 133 ; N ST` | a prompt starts, as A |
| `OSC 133 ; P ST` | a part of the prompt, which vtium ignores |

## What vtium does

vtium marks the line where each part starts, so the user can jump from one prompt to the next. A and N first move to a fresh line when the cursor is not in the first column, and L only does that.

P, D and the options change nothing, and vtium does not keep D's exit status. ED 2, ED 3 and RIS clear the marks of the lines they erase.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** shells mark their prompts, so the user can jump between commands.
- **Specifications:** FinalTerm (foot(7)); kitty shell-integration

## Tests

These tests check what vtium does with the sequences on this page.

- `a_and_n_and_l_start_a_fresh_line` in [crates/vtium-term/tests/marks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/marks.rs)
- `a_commands_output_is_selected_from_its_mark_to_the_next_prompt` in [crates/vtium-term/tests/marks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/marks.rs)
- `a_malformed_mark_does_nothing` in [crates/vtium-term/tests/marks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/marks.rs)
- `a_prompt_jump_does_nothing_on_the_alternate_screen` in [crates/vtium-term/tests/marks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/marks.rs)
- `a_prompt_jump_lands_each_marked_prompt_in_the_scrollback` in [crates/vtium-term/tests/marks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/marks.rs)
- `a_prompt_jump_reaches_rows_waiting_to_be_rewrapped` in [crates/vtium-term/tests/marks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/marks.rs)
- `each_mark_sets_its_flag_on_the_cursors_line` in [crates/vtium-term/tests/marks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/marks.rs)
- `ed_2_and_ris_clear_marks_and_partial_erases_keep_them` in [crates/vtium-term/tests/marks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/marks.rs)
- `marks_travel_with_their_lines_and_through_a_rewrap` in [crates/vtium-term/tests/marks.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/marks.rs)
- `ctrl_right_click_copies_the_output_of_the_command_under_the_pointer` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `ctrl_shift_right_click_copies_a_commands_output_while_the_program_tracks` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `ctrl_shift_up_and_down_jump_between_the_marked_prompts` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- [tools/conformance/scenarios/select-output.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/select-output.scenario)
- [tools/conformance/scenarios/view-prompt-jump.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/view-prompt-jump.scenario)

## Where to read more

To follow the work on this row, see issue [#40](https://github.com/vtium/vtium/issues/40).

To read why vtium does what it does here, see [ADR-1002, "5. Prompt marks"](../architecture/term/ADR-1002-links-are-copied-never-opened-and-prompt-marks-are-line-flags-the-view-jumps-between.md#5-prompt-marks).
