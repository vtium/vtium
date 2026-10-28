# vtium-font data

| Path | What it is |
|---|---|
| `unifont.hex` | GNU Unifont 17.0.03, the unifont-all build: every glyph as `codepoint:bits`, 32 hex digits for an 8x16 glyph and 64 for a 16x16 one, planes 0-3, 14 and 15. The same file as the C reference's `src/font/font_unifont_data.hex` at aaronsb/kmscon `b1f7a7c5`, so the built-in font draws the C reference's pixels. `build.rs` packs it into the binary. |
| `OFL-1.1.txt` | The SIL Open Font License 1.1, as unifoundry.com publishes it with Unifont. |
| `unscii-8.hex` | Unscii 2.1's 8x8 font, `unscii-8.hex` from <http://viznut.fi/unscii/unscii-8.hex>, fetched 2026-10-28, SHA-256 `03094f7fbab7085cf6a6b624cee61e47e71ce5d0c2f308c2f4436afdc17f776c`, the hash ADR-1200 D7 pins: 3,191 glyphs as `codepoint:bits`, 16 hex digits for an 8x8 glyph and 32 for the one 16x8 glyph, U+0000. The `.hex` is a build output the source tarball does not hold, so the published file is pinned by its hash. `build.rs` packs it into the built-in `unscii-8` set. In the public domain. |
| `unscii.txt` | Unscii's notes from `unscii-2.1-src.tar.gz` (<http://viznut.fi/unscii/unscii-2.1-src.tar.gz>, SHA-256 `4d1215d495e49d005ae70828ec1529e997049b9a7f152b9439d78c7e70c65b57`), unchanged: "Everything made by me (viznut) is in the Public Domain.", and only `unscii-16-full`, which vtium does not use, falls under the GPL. |

Unscii is the work of Viznut (viznut.fi). GNU Unifont is the work of Roman Czyborra, Paul Hardy and the Unifont contributors (unifoundry.com). Its fonts are dual-licensed under the SIL Open Font License 1.1 and the GNU GPL 2 or later with the GNU font embedding exception; vtium takes them under the OFL.
