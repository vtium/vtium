# vtium-font data

| Path | What it is |
|---|---|
| `unifont.hex` | GNU Unifont 17.0.03, the unifont-all build: every glyph as `codepoint:bits`, 32 hex digits for an 8x16 glyph and 64 for a 16x16 one, planes 0-3, 14 and 15. The same file as the C reference's `src/font/font_unifont_data.hex` at aaronsb/kmscon `b1f7a7c5`, so the built-in font draws the C reference's pixels. `build.rs` packs it into the binary. |
| `OFL-1.1.txt` | The SIL Open Font License 1.1, as unifoundry.com publishes it with Unifont. |

GNU Unifont is the work of Roman Czyborra, Paul Hardy and the Unifont contributors (unifoundry.com). Its fonts are dual-licensed under the SIL Open Font License 1.1 and the GNU GPL 2 or later with the GNU font embedding exception; vtium takes them under the OFL.
