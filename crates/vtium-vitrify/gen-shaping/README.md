# vtium-vitrify shaping table generator

Generates vtium-vitrify's `src/shaping/data.rs` from the Unicode Character Database 18.0.0: the RGI emoji sequences of more than one scalar value, the joining letters of the scripts HarfRust's Arabic shaper takes, the canonical decompositions keyed at each step, and the ranges of the scripts shaped on demand (vtium/vtium#144). The output is committed, so the crate builds without the UCD; rerun this only to move the pinned version, and commit the new pins and the regenerated file on their own. It is its own workspace with no dependencies, so the main workspace and CI never build it.

Fetch the five UCD files into a directory, then run it from the repository root:

```sh
mkdir -p /tmp/ucd
curl -fsSL -o /tmp/ucd/emoji-sequences.txt https://www.unicode.org/Public/18.0.0/emoji/emoji-sequences.txt
curl -fsSL -o /tmp/ucd/emoji-zwj-sequences.txt https://www.unicode.org/Public/18.0.0/emoji/emoji-zwj-sequences.txt
curl -fsSL -o /tmp/ucd/ArabicShaping.txt https://www.unicode.org/Public/18.0.0/ucd/ArabicShaping.txt
curl -fsSL -o /tmp/ucd/UnicodeData.txt https://www.unicode.org/Public/18.0.0/ucd/UnicodeData.txt
curl -fsSL -o /tmp/ucd/Scripts.txt https://www.unicode.org/Public/18.0.0/ucd/Scripts.txt
cargo run --manifest-path crates/vtium-vitrify/gen-shaping/Cargo.toml -- /tmp/ucd
```

The generator refuses a file whose SHA-256 differs from its pin in `src/main.rs`. Two runs on the same files write identical output.

| Path | What it is |
|---|---|
| `src/main.rs` | The generator: the pinned files and their SHA-256s, the scripts whose letters take joining forms and those shaped on demand, each as HarfRust 0.12 routes them to its shapers, and SHA-256 itself. |
| `src/unicode-license.txt` | The Unicode License v3, from https://www.unicode.org/license.txt, copied into the output. |
| `Cargo.toml`, `Cargo.lock` | The generator crate, its own workspace. |
| `.gitignore` | Leaves out `target`. |
