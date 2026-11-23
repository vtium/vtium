<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# kitty keyboard protocol: CSI u, CSI ? u, CSI > u, CSI < u, CSI = u

| Sequence | What it does |
|---|---|
| `CSI > flags u` | push a set of flags |
| `CSI < Ps u` | pop sets of flags |
| `CSI = flags ; Pm u` | change the flags in force |
| `CSI ? u` | ask for the flags in force |
| `CSI Pk ; Pm u` | a key, as vtium reports it |

## Parameters

- `flags`: The sum of the flags a program asks for: 1 disambiguates escape codes, 2 reports repeats and releases, 4 reports the shifted key and the base layout key, 8 reports every key as an escape code, and 16 reports the text a key makes. vtium keeps these five bits and drops any other. The default is 0.
- `Ps`: How many sets to pop. 0 pops nothing. The default is 1.
- `Pm`: In `CSI = flags ; Pm u`, how to apply the flags: 1 sets exactly these flags, 2 sets their bits, and 3 clears their bits. Any other value changes nothing. In a key report, `Pm` is 1 plus the modifiers held. The default is 1.
- `Pk`: In a key report, the key's code: the Unicode code point of the unshifted key, or a number from the specification's table of functional keys.

## What vtium does

vtium keeps one stack of flags for the main screen and one for the alternate screen, and switching screens leaves each stack as it is. A stack holds the set in force and seven saved sets. A push onto a full stack drops the oldest saved set, and a pop of more sets than are saved puts 0 in force.

While flag 1 or flag 8 is in force, the protocol decides the bytes of every key. Without them, each key follows the specification's legacy encoding. `CSI ? u` is answered `CSI ? flags u` with the flags in force on the screen shown. RIS and DECSTR empty both stacks.

## Status

- **Status:** supported. vtium implements this as the specifications below define it.
- **Why:** editors and shells built for kitty tell every key and its release apart with it.
- **Note:** vtium keeps the flag stacks, answers the query and encodes keys under all five flags: 1, disambiguate escape codes; 2, report event types, with a release for every press a program got; 4, report alternate keys, from the unshifted key and the base layout key vtium-platform reports; 8, report all keys as escape codes; and 16, report associated text.
- **Specifications:** kitty keyboard-protocol

## Tests

These tests check what vtium does with the sequences on this page.

- `a_key_held_across_a_layout_change_reports_its_release` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `a_key_reports_its_shifted_key` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `a_key_reports_its_unshifted_key_and_base_layout_key` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `a_key_that_went_into_a_compose_sequence_reports_no_release` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `a_keymap_that_gives_hyper_a_modifier_of_its_own_reports_it` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `a_keymap_that_gives_meta_a_modifier_of_its_own_reports_it` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `a_non_latin_layout_reports_the_base_layout_key` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `a_release_is_reported_for_every_reported_press` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `a_symbol_altgr_chose_reports_no_unshifted_key` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `the_base_layout_table_follows_the_kernels_key_names` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `the_default_keymap_reports_neither_hyper_nor_meta` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `the_french_azerty_layout_reports_each_keys_forms` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `the_german_layout_reports_each_keys_forms` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `the_russian_layout_reports_each_keys_forms` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `the_us_layout_reports_each_keys_forms` in [crates/vtium-platform/src/input/tests.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-platform/src/input/tests.rs)
- `no_reply_carries_the_requests_bytes` in [crates/vtium-term/src/terminal/reply.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/src/terminal/reply.rs)
- `a_host_reset_leaves_the_terminal_as_ris_does` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_ninth_push_drops_the_oldest_level` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_push_of_11_reports_the_release_of_every_key` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_push_of_1_keeps_enter_tab_and_backspace_unmodified` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_push_of_1_sends_escape_alt_and_ctrl_keys_as_codes` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_push_of_1_sends_the_functional_keys_in_the_specifications_forms` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_push_of_1_sends_the_keypad_as_keys_of_its_own` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_push_of_25_reports_the_text_inside_the_code` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_push_of_3_reports_repeats_and_releases` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_push_of_5_reports_the_shifted_and_base_layout_keys` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_push_of_9_reports_every_key_as_an_escape_code` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_release_follows_from_its_press` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_text_event_with_no_keysym_is_sent_with_the_key_number_0` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_text_event_with_no_keysym_keeps_its_code_where_no_text_field_carries_it` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `each_screen_keeps_its_own_flags_across_a_switch` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_gives_enter_tab_and_backspace_no_release` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_leaves_a_key_outside_the_ctrl_table_untouched` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_reports_events_for_the_legacy_escape_codes` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_reports_f13_and_the_keypad_keys_events` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_reports_keypad_keys_as_their_equivalent_keys` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_reports_the_release_of_a_text_key` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_sends_a_modified_f3_in_its_tilde_form` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_sends_alt_shift_tab_as_esc_csi_z` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_sends_ctrl_shift_with_a_text_key_as_csi_u` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_sends_pf1_to_pf4_as_f1_to_f4` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_sends_super_and_ctrl_alt_shift_with_a_text_key_as_csi_u` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_sends_super_with_enter_and_escape_as_csi_u` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_alone_swaps_backspace_and_ctrl_backspace` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flag_2_sends_no_release_for_enter_tab_backspace_or_text` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flags_at_0_keep_xterms_bytes_where_the_legacy_section_differs` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `flags_at_0_send_xterms_bytes` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `modify_other_keys_waits_while_the_flags_are_not_0` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `ris_and_decstr_clear_both_stacks` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `the_keypads_begin_is_csi_e_under_flags_1_and_8` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `the_query_reports_only_the_flags_vtium_implements` in [crates/vtium-term/tests/kitty_keyboard.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/kitty_keyboard.rs)
- `a_console_chord_lets_go_of_the_keys_held` in [crates/vtium/src/app/held.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/held.rs)
- `a_console_that_stops_receiving_a_keyboard_gets_its_keys_released` in [crates/vtium/src/app/held.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/held.rs)
- `a_key_vtium_took_never_reaches_the_program_pressed_or_released` in [crates/vtium/src/app/held.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/held.rs)
- `a_release_goes_to_the_console_its_press_reached_once` in [crates/vtium/src/app/held.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/held.rs)
- `a_vt_switch_lets_go_of_the_keys_held` in [crates/vtium/src/app/held.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/held.rs)
- `an_unplugged_keyboard_lets_go_of_its_keys_held` in [crates/vtium/src/app/held.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/app/held.rs)
- `caps_lock_leaves_a_binding_unless_the_binding_names_it` in [crates/vtium/src/grab.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/src/grab.rs)
- `a_switch_the_vt_raises_lets_go_of_the_keys_held` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `the_russian_layout_reaches_the_program` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `the_unshifted_and_base_layout_keys_reach_the_program` in [crates/vtium/tests/boot.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/boot.rs)
- `the_focus_chord_lets_go_of_the_keys_held_on_the_head_it_leaves` in [crates/vtium/tests/heads.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/heads.rs)
- `the_french_azerty_layout_sends_its_keys_forms` in [crates/vtium/tests/keyboard_layouts.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/keyboard_layouts.rs)
- `the_german_layout_sends_its_keys_forms` in [crates/vtium/tests/keyboard_layouts.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/keyboard_layouts.rs)
- `the_russian_layout_sends_its_keys_forms` in [crates/vtium/tests/keyboard_layouts.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/keyboard_layouts.rs)
- `the_us_layout_sends_its_keys_forms` in [crates/vtium/tests/keyboard_layouts.rs](https://github.com/vtium/vtium/blob/main/crates/vtium/tests/keyboard_layouts.rs)
- [tools/conformance/scenarios/key-kitty-disambiguate.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/key-kitty-disambiguate.scenario)
- [tools/conformance/scenarios/key-kitty-event-types.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/key-kitty-event-types.scenario)
- [tools/conformance/scenarios/key-kitty-flags.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/key-kitty-flags.scenario)
- [tools/conformance/scenarios/key-kitty-legacy-functional.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/key-kitty-legacy-functional.scenario)
- [tools/conformance/scenarios/key-kitty-modify-other-keys.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/key-kitty-modify-other-keys.scenario)
- [tools/conformance/scenarios/key-kitty-release-rule.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/key-kitty-release-rule.scenario)
- [tools/conformance/scenarios/keypad-kitty.scenario](https://github.com/vtium/vtium/blob/main/tools/conformance/scenarios/keypad-kitty.scenario)

## Where to read more

To follow the work on this row, see issue [#42](https://github.com/vtium/vtium/issues/42).

To read why vtium does what it does here, see [ADR-1005 D3](../architecture/term/ADR-1005-the-kitty-keyboard-protocol-vtium-term-encodes-keys-by-a-stack-of-flags-for-each-screen-and-vtium-platform-reports-key-releases-and-each-key-s-other-forms.md#d3-the-flag-stacks).
