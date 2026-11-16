<!-- Generated from ADR-1000's features table and crates/vtium-term/src/terminal/sequences.rs by the test generated_documents_of_the_sequences_match_the_tables. VTIUM_TERM_BLESS=1 cargo test -p vtium-term generated_documents rewrites it. -->

# Charsets: SCS for G0-G3, DEC special graphics, LS2, LS3, SS2, SS3, UTF-8 printing whatever G1-G3 hold

| Sequence | What it does |
|---|---|
| `ESC ( C` | SCS: designate a set into G0 |
| `ESC ) C` | SCS: designate a set into G1 |
| `ESC * C` | SCS: designate a set into G2 |
| `ESC + C` | SCS: designate a set into G3 |
| `SI` | LS0: G0 into GL |
| `SO` | LS1: G1 into GL |
| `ESC n` | LS2: G2 into GL |
| `ESC o` | LS3: G3 into GL |
| `ESC ~` | LS1R: G1 into GR |
| `ESC }` | LS2R: G2 into GR |
| <code>ESC &#124;</code> | LS3R: G3 into GR |
| `ESC N` | SS2: G2 for the next character |
| `ESC O` | SS3: G3 for the next character |

## Parameters

- `C`: The set: `B` for ASCII, `0` for DEC special graphics, and `<` for DEC supplemental graphics, which vtium designates only in 8-bit input and takes as ASCII otherwise. The national replacement sets' finals `A`, `4`, `C`, `5`, `R`, `Q`, `K`, `Y`, `E`, `6`, `Z`, `H`, `7` and `=` designate ASCII. vtium ignores any other final, such as the national sets' `f`, `9`, `%2`, `%6`, `%=`, `"4` and `"?`.

## What vtium does

vtium keeps four character sets, G0 to G3. GL maps the ASCII graphic characters, and GR maps the bytes 0xA1 to 0xFE of 8-bit input. At the start G0 and G1 hold ASCII, and G2 and G3 hold ASCII, or DEC supplemental graphics in 8-bit input. GL holds G0 and GR holds G2.

DEC special graphics draws lines and boxes in place of letters, which programs such as ncurses use. A character decoded from UTF-8 prints as itself, whatever G1 to G3 hold. A single shift applies to the next printed character only. DECSC saves the sets and a waiting single shift.

The finals `A`, `4`, `C`, `5`, `R`, `Q`, `K`, `Y`, `E`, `6`, `Z`, `H`, `7` and `=` of the national replacement sets designate ASCII, since vtium does not implement national replacement sets. vtium ignores the other national sets' finals. See [Character sets in vtium-term's README](https://github.com/vtium/vtium/blob/main/crates/vtium-term/README.md#character-sets).

## Status

- **Status:** supported. vtium implements this as the specifications below define it, apart from the departure above.
- **Why:** ncurses draws lines and boxes with DEC special graphics, and old programs switch character sets.
- **Specifications:** [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html); [the VT510 reference manual](https://vt100.net/docs/vt510-rm/contents.html)

## Tests

These tests check what vtium does with the sequences on this page.

- `a_utf8_character_prints_as_itself_whatever_g1_to_g3_hold` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `dec_special_graphics_draws_lines` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `dec_supplemental_graphics_only_in_eight_bit_input` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `decsc_saves_the_character_sets` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `decstr_and_ris_reset_the_character_sets` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `gr_maps_the_upper_half_in_eight_bit_input` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `locking_shifts_invoke_g2_and_g3_into_gl` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `national_replacement_sets_designate_ascii` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `rep_repeats_the_character_a_single_shift_mapped` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `seven_bit_input_reaches_only_gl` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `shift_out_invokes_g1_into_gl` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- `single_shifts_take_the_next_character_only` in [crates/vtium-term/tests/charsets.rs](https://github.com/vtium/vtium/blob/main/crates/vtium-term/tests/charsets.rs)
- [tools/conformance/vtium/cases/charset-dec-special.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/charset-dec-special.case)
- [tools/conformance/vtium/cases/charset-gr.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/charset-gr.case)
- [tools/conformance/vtium/cases/charset-locking-shifts.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/charset-locking-shifts.case)
- [tools/conformance/vtium/cases/charset-nrcs.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/charset-nrcs.case)
- [tools/conformance/vtium/cases/charset-so-si.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/charset-so-si.case)
- [tools/conformance/vtium/cases/charset-ss2-ss3.case](https://github.com/vtium/vtium/blob/main/tools/conformance/vtium/cases/charset-ss2-ss3.case)

## Where to read more

To follow the work on this row, see issue [#13](https://github.com/vtium/vtium/issues/13).

To read why vtium does what it does here, see [ADR-1000, "The target set"](../architecture/term/ADR-1000-the-modes-and-features-vtium-supports.md#the-target-set).
