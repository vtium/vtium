# vtium-term Unicode table generator

Generates vtium-term's `src/unicode/data.rs` (each codepoint's Grapheme_Cluster_Break, Extended_Pictographic, Indic_Conjunct_Break, cell width and VS16 base flag), `src/unicode/grapheme_break_test.txt` (the GraphemeBreakTest cases the segmenter's test runs) and `src/unicode/above_marks.txt` (every nonspacing mark with canonical combining class 230 and no decomposition mapping, which the test of the kitty placeholder diacritics checks against) from the Unicode Character Database 18.0.0. For the `bidi` feature (ADR-1004) it also generates `src/unicode/bidi/data.rs` (each codepoint's Bidi_Class and Joining_Type, from their `@missing` lines and then their data lines; the mirroring pairs of BidiMirroring.txt; the bracket pairs of BidiBrackets.txt, each closing bracket keyed to its opening bracket after canonical decomposition; and each box-drawing character's left-right counterpart, the character whose name in UnicodeData.txt swaps LEFT and RIGHT, or else gives the swapped name's halves around AND in the other order) and `src/unicode/bidi/bidi_test.txt` and `src/unicode/bidi/bidi_character_test.txt` (Unicode's two conformance files for the bidirectional algorithm, each line checked and the comments dropped). All the outputs are committed, so the crate builds without the UCD; rerun this only to move the pinned version, and commit the new pins and the regenerated files on their own. It is its own workspace with no dependencies, so the main workspace and CI never build it.

Fetch the fourteen UCD files into a directory, then run it from the repository root:

```sh
mkdir -p /tmp/ucd
curl -fsSL -o /tmp/ucd/GraphemeBreakProperty.txt https://www.unicode.org/Public/18.0.0/ucd/auxiliary/GraphemeBreakProperty.txt
curl -fsSL -o /tmp/ucd/GraphemeBreakTest.txt https://www.unicode.org/Public/18.0.0/ucd/auxiliary/GraphemeBreakTest.txt
curl -fsSL -o /tmp/ucd/emoji-data.txt https://www.unicode.org/Public/18.0.0/ucd/emoji/emoji-data.txt
curl -fsSL -o /tmp/ucd/emoji-variation-sequences.txt https://www.unicode.org/Public/18.0.0/ucd/emoji/emoji-variation-sequences.txt
curl -fsSL -o /tmp/ucd/DerivedCoreProperties.txt https://www.unicode.org/Public/18.0.0/ucd/DerivedCoreProperties.txt
curl -fsSL -o /tmp/ucd/EastAsianWidth.txt https://www.unicode.org/Public/18.0.0/ucd/EastAsianWidth.txt
curl -fsSL -o /tmp/ucd/DerivedGeneralCategory.txt https://www.unicode.org/Public/18.0.0/ucd/extracted/DerivedGeneralCategory.txt
curl -fsSL -o /tmp/ucd/UnicodeData.txt https://www.unicode.org/Public/18.0.0/ucd/UnicodeData.txt
curl -fsSL -o /tmp/ucd/DerivedBidiClass.txt https://www.unicode.org/Public/18.0.0/ucd/extracted/DerivedBidiClass.txt
curl -fsSL -o /tmp/ucd/DerivedJoiningType.txt https://www.unicode.org/Public/18.0.0/ucd/extracted/DerivedJoiningType.txt
curl -fsSL -o /tmp/ucd/BidiMirroring.txt https://www.unicode.org/Public/18.0.0/ucd/BidiMirroring.txt
curl -fsSL -o /tmp/ucd/BidiBrackets.txt https://www.unicode.org/Public/18.0.0/ucd/BidiBrackets.txt
curl -fsSL -o /tmp/ucd/BidiTest.txt https://www.unicode.org/Public/18.0.0/ucd/BidiTest.txt
curl -fsSL -o /tmp/ucd/BidiCharacterTest.txt https://www.unicode.org/Public/18.0.0/ucd/BidiCharacterTest.txt
cargo run --manifest-path crates/vtium-term/gen-unicode/Cargo.toml -- /tmp/ucd
```

The generator refuses a file whose SHA-256 differs from its pin in `src/main.rs`. Two runs on the same files write identical outputs.

| Path | What it is |
|---|---|
| `src/main.rs` | The generator: the pinned files and their SHA-256s, the property word's layout, the width rule vtium-term's `src/unicode.rs` states, the marks the placeholder diacritics are drawn from, and SHA-256 itself. |
| `src/bidi.rs` | The tables of the `bidi` feature and the two conformance files: the Bidi_Class and Joining_Type codes, the run word's layout, the bracket and box-drawing pairing rules, and the checks each conformance line passes. |
| `src/unicode-license.txt` | The Unicode License v3, from https://www.unicode.org/license.txt, copied into each output. |
| `Cargo.toml`, `Cargo.lock` | The generator crate, its own workspace. |
| `.gitignore` | Leaves out `target`. |
