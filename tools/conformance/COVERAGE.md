# Coverage

<!-- generated from capabilities.list and ADR-1000's tables; VTIUM_CONFORMANCE_BLESS=1 cargo test -p vtium-conformance coverage rewrites it -->

Each capability the C reference's probes judge, the ADR-1000 rows its probe covers, the tests that guard those rows, and the gap: the issue that owes a missing capability, or a row no test guards yet. The probes themselves run through `conformance probes` (README.md).

| Capability | Status | ADR-1000 rows | Tests | Gap |
|---|---|---|---|---|
| `sgr-colon-subparams` | works | `sgr.extended-colour` | `crates/vtium-term/tests/sgr.rs::extended_colours_take_both_forms` |  |
| `sgr-underline-styles` | works | `sgr.underline-style` | `crates/vtium-term/tests/sgr.rs::underline_styles_take_a_sub_parameter` |  |
| `sgr-underline-colour` | works | `sgr.underline-colour` | `crates/vtium-term/tests/sgr.rs::sgr_58_and_59_set_the_underline_colour` |  |
| `sgr-strike` | works | `sgr.strike` | `crates/vtium-term/tests/sgr.rs::strike_is_set_and_cleared` |  |
| `sgr-blink` | works | `sgr.blink` | `crates/vtium-render/tests/blink.rs::blinking_text_hides_as_blank_cells_but_under_the_cursor`<br>`crates/vtium-term/tests/sgr.rs::blink_is_set_and_cleared` |  |
| `sgr-italic` | works | `sgr.italic` | `crates/vtium-term/tests/sgr.rs::italic_is_set_and_cleared` |  |
| `sgr-truecolor` | works | `sgr.extended-colour` | `crates/vtium-term/tests/sgr.rs::extended_colours_take_both_forms` |  |
| `sgr-overline` | missing | `sgr.overline` |  | missing, #37; no test for `sgr.overline` |
| `sgr-super-subscript` | missing | `sgr.script` |  | missing, #37; no test for `sgr.script` |
| `palette-at-draw` | works | `sgr.draw-time-colour` | `crates/vtium-term/src/frame.rs::a_palette_change_recolours_text_already_written` |  |
| `modkeys-not-sgr` | works | `input.modify-other-keys` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/input.rs::modify_other_keys_encodes_modified_keys`<br>`crates/vtium-term/tests/replies.rs::xtmodkeys_sets_and_reports_the_level` |  |
| `cluster` | works | `text.grapheme` | `crates/vtium-term/tests/screen.rs::a_grapheme_cluster_takes_one_cell` |  |
| `mode-2027` | works | `DEC-2027` | `crates/vtium-term/tests/modes.rs::mode_2027_reports_and_resets_to_the_configured_method`<br>`crates/vtium-term/tests/screen.rs::the_width_method_cuts_text_into_cells` |  |
| `vs16-width` | works | `text.vs-width` | `crates/vtium-term/tests/screen.rs::vs16_widens_and_a_wide_character_wraps_whole` |  |
| `wide-at-margin` | works | `text.vs-width` | `crates/vtium-term/tests/screen.rs::vs16_widens_and_a_wide_character_wraps_whole` |  |
| `rep` | works | `text.rep` | `crates/vtium-term/tests/screen.rs::rep_repeats_the_last_printed_base` |  |
| `erase-colours-only` | works | `ctl.bce` | `crates/vtium-term/tests/screen.rs::erasing_takes_only_the_pens_background` |  |
| `frame-age-per-caller` | works | `platform.multi-monitor` |  | no test for `platform.multi-monitor` |
| `reflow` | missing | `text.reflow` |  | missing, #41; no test for `text.reflow` |
| `bidi` | missing | `text.bidi` |  | missing, #43; no test for `text.bidi` |
| `mode-2026-sync` | works | `DEC-2026` | `crates/vtium-term/tests/modes.rs::mode_2026_holds_frames_while_the_host_offers_it` |  |
| `decrqm` | works | `query.decrqm` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes` |  |
| `da1` | works | `query.da1` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/replies.rs::da1_reports_a_vt220_with_colour` |  |
| `da1-level` | works | `query.da1` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/replies.rs::da1_reports_a_vt220_with_colour` |  |
| `da2` | works | `query.da2` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/replies.rs::da2_reports_a_vt220` |  |
| `da3` | works | `query.da3` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/replies.rs::da3_reports_a_unit_id` |  |
| `xtversion` | works | `query.xtversion` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/replies.rs::xtversion_reports_the_hosts_text` |  |
| `xtgettcap` | works | `query.xtgettcap` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/replies.rs::a_table_past_its_bounds_is_refused`<br>`crates/vtium-term/tests/replies.rs::xtgettcap_answers_from_the_table_in_uppercase`<br>`crates/vtium-term/tests/replies.rs::xtgettcap_answers_sixteen_names`<br>`crates/vtium-term/tests/replies.rs::xtgettcap_replies_up_to_its_bound` |  |
| `osc-4-query` | works | `colour.palette.query` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/replies.rs::osc_4_reports_palette_colours` |  |
| `osc-4-set` | works | `colour.palette.set` | `crates/vtium-term/tests/colours.rs::osc_104_resets_palette_colours_to_the_configured_palette`<br>`crates/vtium-term/tests/colours.rs::osc_4_sets_palette_colours` |  |
| `osc-10-11-query` | works | `colour.dynamic.query` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/replies.rs::osc_10_and_11_report_the_default_colours` |  |
| `osc-10-11-set` | works | `colour.dynamic.set` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/colours.rs::osc_10_11_and_12_set_and_query`<br>`crates/vtium-term/tests/colours.rs::osc_110_111_and_112_reset_to_the_configured_palette` |  |
| `osc-12-query` | works | `colour.dynamic.set` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/colours.rs::osc_10_11_and_12_set_and_query`<br>`crates/vtium-term/tests/colours.rs::osc_110_111_and_112_reset_to_the_configured_palette` |  |
| `osc-104-110-111-reset` | works | `colour.palette.set` | `crates/vtium-term/tests/colours.rs::osc_104_resets_palette_colours_to_the_configured_palette`<br>`crates/vtium-term/tests/colours.rs::osc_4_sets_palette_colours` |  |
| `kitty-keyboard` | missing | `input.kitty-keyboard` |  | missing, #42; no test for `input.kitty-keyboard` |
| `modify-other-keys` | works | `input.modify-other-keys` | `crates/vtium-term/src/terminal/reply.rs::no_reply_carries_the_requests_bytes`<br>`crates/vtium-term/tests/input.rs::modify_other_keys_encodes_modified_keys`<br>`crates/vtium-term/tests/replies.rs::xtmodkeys_sets_and_reports_the_level` |  |
| `mouse-1000` | works | `DEC-1000` | `crates/vtium-term/tests/modes.rs::mouse_tracking_reports_by_its_mode` |  |
| `mouse-1006` | works | `DEC-1006` | `crates/vtium-term/tests/modes.rs::sgr_mouse_encodings_report_cells_or_pixels` |  |
| `mouse-1006-first` | works | `DEC-1006` | `crates/vtium-term/tests/modes.rs::sgr_mouse_encodings_report_cells_or_pixels` |  |
| `mouse-1003-sgr` | works | `DEC-1003` | `crates/vtium-term/tests/modes.rs::mouse_tracking_reports_by_its_mode` |  |
| `mouse-1003-x10` | works | `DEC-1003` | `crates/vtium-term/tests/modes.rs::mouse_tracking_reports_by_its_mode` |  |
| `mouse-1016` | works | `DEC-1016` | `crates/vtium-term/tests/modes.rs::sgr_mouse_encodings_report_cells_or_pixels` |  |
| `bracketed-paste` | works | `DEC-2004` | `crates/vtium-term/tests/modes.rs::bracketed_paste_brackets_a_paste` |  |
| `focus-1004` | works | `DEC-1004` | `crates/vtium-term/tests/modes.rs::focus_events_are_reported_while_set` |  |
| `osc-8-hyperlink` | missing | `osc.8` |  | missing, #40; no test for `osc.8` |
| `osc-52-clipboard` | missing | `osc.52.write` | `crates/vtium-term/tests/clipboard.rs::osc_52_at_the_cap_sets_and_one_byte_over_is_dropped`<br>`crates/vtium-term/tests/clipboard.rs::osc_52_sets_the_selection`<br>`crates/vtium-term/tests/clipboard.rs::osc_52_sets_the_selection_for_c_p_and_s`<br>`crates/vtium-term/tests/clipboard.rs::osc_52_split_at_every_offset`<br>`crates/vtium-term/tests/clipboard.rs::osc_52_with_invalid_base64_is_dropped`<br>`crates/vtium/tests/boot.rs::a_hostile_selection_is_pasted_filtered_once_a_press` | missing, #21 |
| `osc-133-marks` | missing | `osc.133` |  | missing, #40; no test for `osc.133` |
| `sixel` | works | `image.sixel`<br>`query.xtsmgraphics` |  | needs the build feature images; no test for `image.sixel`, `query.xtsmgraphics` |
| `kitty-graphics` | works | `image.kitty` |  | needs the build feature images; no test for `image.kitty` |
| `mode-80` | works | `DEC-80` |  | needs the build feature images; no test for `DEC-80` |
| `mode-1070` | works | `DEC-1070` |  | needs the build feature images; no test for `DEC-1070` |
| `mode-8452` | works | `DEC-8452` |  | needs the build feature images; no test for `DEC-8452` |
