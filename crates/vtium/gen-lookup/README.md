# vtium's cluster lookup table generator

Generates vtium's `src/deferred/data.rs` from the Unicode Character Database 18.0.0: the combining marks, the spacing marks among them and the side of their letter each stands on, the default ignorables, the canonical combining classes, the primary composites and the canonical decompositions the console's lookup for a cluster reads (ADR-1200 D22, steps 2 and 3), and the Arabic presentation forms of each letter and of lam with alef, which a build with the bidi feature draws where no font holds a letter's joined form (ADR-1004 D6). The output is committed, so the crate builds without the UCD; rerun this to move the pinned version or to change what the tables hold, and commit the generator, any new pins and the regenerated file in a commit of their own. It is its own workspace with no dependencies, so the main workspace and CI never build it.

Fetch the four UCD files into a directory, then run it from the repository root:

```sh
mkdir -p /tmp/ucd
curl -fsSL -o /tmp/ucd/UnicodeData.txt https://www.unicode.org/Public/18.0.0/ucd/UnicodeData.txt
curl -fsSL -o /tmp/ucd/DerivedCoreProperties.txt https://www.unicode.org/Public/18.0.0/ucd/DerivedCoreProperties.txt
curl -fsSL -o /tmp/ucd/DerivedNormalizationProps.txt https://www.unicode.org/Public/18.0.0/ucd/DerivedNormalizationProps.txt
curl -fsSL -o /tmp/ucd/IndicPositionalCategory.txt https://www.unicode.org/Public/18.0.0/ucd/IndicPositionalCategory.txt
cargo run --manifest-path crates/vtium/gen-lookup/Cargo.toml -- /tmp/ucd
cargo fmt --all
```

The generator writes each table's elements packed several to a line, and `cargo fmt` then lays them out one to a line, as the committed file has them, so run it before comparing or committing.

The generator refuses a file whose SHA-256 differs from its pin in `src/main.rs`. Two runs on the same files, each followed by `cargo fmt`, give identical files.

| Path | What it is |
|---|---|
| `src/main.rs` | The generator: the pinned files and their SHA-256s, the tables' rules, and SHA-256 itself. |
| `src/unicode-license.txt` | The Unicode License v3, from https://www.unicode.org/license.txt, copied into the output. |
| `Cargo.toml`, `Cargo.lock` | The generator crate, its own workspace. |
| `.gitignore` | Leaves out `target`. |
