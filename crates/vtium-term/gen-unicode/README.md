# vtium-term Unicode table generator

Generates vtium-term's `src/unicode/data.rs` (each codepoint's Grapheme_Cluster_Break, Extended_Pictographic, Indic_Conjunct_Break, cell width and VS16 base flag) and `src/unicode/grapheme_break_test.txt` (the GraphemeBreakTest cases the segmenter's test runs) from the Unicode Character Database 18.0.0. Both outputs are committed, so the crate builds without the UCD; rerun this only to move the pinned version, and commit the new pins and the regenerated files on their own. It is its own workspace with no dependencies, so the main workspace and CI never build it.

Fetch the seven UCD files into a directory, then run it from the repository root:

```sh
mkdir -p /tmp/ucd
curl -fsSL -o /tmp/ucd/GraphemeBreakProperty.txt https://www.unicode.org/Public/18.0.0/ucd/auxiliary/GraphemeBreakProperty.txt
curl -fsSL -o /tmp/ucd/GraphemeBreakTest.txt https://www.unicode.org/Public/18.0.0/ucd/auxiliary/GraphemeBreakTest.txt
curl -fsSL -o /tmp/ucd/emoji-data.txt https://www.unicode.org/Public/18.0.0/ucd/emoji/emoji-data.txt
curl -fsSL -o /tmp/ucd/emoji-variation-sequences.txt https://www.unicode.org/Public/18.0.0/ucd/emoji/emoji-variation-sequences.txt
curl -fsSL -o /tmp/ucd/DerivedCoreProperties.txt https://www.unicode.org/Public/18.0.0/ucd/DerivedCoreProperties.txt
curl -fsSL -o /tmp/ucd/EastAsianWidth.txt https://www.unicode.org/Public/18.0.0/ucd/EastAsianWidth.txt
curl -fsSL -o /tmp/ucd/DerivedGeneralCategory.txt https://www.unicode.org/Public/18.0.0/ucd/extracted/DerivedGeneralCategory.txt
cargo run --manifest-path crates/vtium-term/gen-unicode/Cargo.toml -- /tmp/ucd
```

The generator refuses a file whose SHA-256 differs from its pin in `src/main.rs`. Two runs on the same files write identical outputs.

| Path | What it is |
|---|---|
| `src/main.rs` | The generator: the pinned files and their SHA-256s, the property word's layout, the width rule vtium-term's `src/unicode.rs` states, and SHA-256 itself. |
| `src/unicode-license.txt` | The Unicode License v3, from https://www.unicode.org/license.txt, copied into both outputs. |
| `Cargo.toml`, `Cargo.lock` | The generator crate, its own workspace. |
| `.gitignore` | Leaves out `target`. |
