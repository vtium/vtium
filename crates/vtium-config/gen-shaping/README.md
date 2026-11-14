# vtium-config shaping table generator

Generates vtium-config's `src/shaping/data.rs` from the Unicode Character Database 18.0.0: the RGI emoji sequences of more than one scalar value, the characters of Emoji_Presentation=Yes (`emoji-data.txt`, which UTS #51 shows as emoji when no variation selector follows them), the joining letters of the scripts HarfRust's Arabic shaper takes, the characters of every script that join to their neighbours (Joining_Type D, R, L or C in `ArabicShaping.txt`, which ADR-1302 D2 stretches to their cells), the canonical decompositions keyed at each step, and the ranges of the scripts shaped on demand (vtium/vtium#144). The output is committed, so the crate builds without the UCD; rerun this only to move the pinned version, and commit the new pins and the regenerated file on their own. It is its own workspace with no dependencies, so the main workspace and CI never build it.

Fetch the nine UCD files into a directory, then run it from the repository root:

```sh
mkdir -p /tmp/ucd
curl -fsSL -o /tmp/ucd/emoji-sequences.txt https://www.unicode.org/Public/18.0.0/emoji/emoji-sequences.txt
curl -fsSL -o /tmp/ucd/emoji-zwj-sequences.txt https://www.unicode.org/Public/18.0.0/emoji/emoji-zwj-sequences.txt
curl -fsSL -o /tmp/ucd/ArabicShaping.txt https://www.unicode.org/Public/18.0.0/ucd/ArabicShaping.txt
curl -fsSL -o /tmp/ucd/UnicodeData.txt https://www.unicode.org/Public/18.0.0/ucd/UnicodeData.txt
curl -fsSL -o /tmp/ucd/Scripts.txt https://www.unicode.org/Public/18.0.0/ucd/Scripts.txt
curl -fsSL -o /tmp/ucd/emoji-data.txt https://www.unicode.org/Public/18.0.0/ucd/emoji/emoji-data.txt
curl -fsSL -o /tmp/ucd/IndicSyllabicCategory.txt https://www.unicode.org/Public/18.0.0/ucd/IndicSyllabicCategory.txt
curl -fsSL -o /tmp/ucd/ScriptExtensions.txt https://www.unicode.org/Public/18.0.0/ucd/ScriptExtensions.txt
curl -fsSL -o /tmp/ucd/PropertyValueAliases.txt https://www.unicode.org/Public/18.0.0/ucd/PropertyValueAliases.txt
mkdir -p /tmp/cldr
curl -fsSL -o /tmp/cldr/cldr-common-49.zip https://unicode.org/Public/cldr/49/cldr-common-49.zip
unzip -q -d /tmp/cldr /tmp/cldr/cldr-common-49.zip 'common/main/*.xml'
cargo run --manifest-path crates/vtium-config/gen-shaping/Cargo.toml -- /tmp/ucd /tmp/cldr
```

It also writes vtium-font's `src/letters/data.rs`: the main exemplar characters of each language in vtium-font's table of locales, from the Unicode Common Locale Data Repository (CLDR) 49. These are the everyday letters `vtium check-config` checks a look's font set for (ADR-1401 D7). The generator reads each language's `common/main/<language>.xml` and refuses one whose SHA-256 differs from its pin in `src/main.rs`.

It also writes vtium-font's `src/syllable/data.rs`. That file lists the characters that link the clusters of a syllable, and the characters that follow a link, by their Indic_Syllabic_Category (`IndicSyllabicCategory.txt`). Each comes with the scripts it belongs to. Those scripts are its Script_Extensions (`ScriptExtensions.txt`), or else its one script (`Scripts.txt`), by the short name `PropertyValueAliases.txt` gives it. vtium joins a syllable's clusters by them (ADR-1203 D1).

The generator refuses a file whose SHA-256 differs from its pin in `src/main.rs`. Two runs on the same files write identical output.

| Path | What it is |
|---|---|
| `src/main.rs` | The generator: the pinned files and their SHA-256s, the scripts whose letters take joining forms and those shaped on demand, each as HarfRust 0.12 routes them to its shapers, and SHA-256 itself. |
| `src/unicode-license.txt` | The Unicode License v3, from https://www.unicode.org/license.txt, copied into the output. |
| `src/cldr-license.txt` | CLDR 49's `LICENSE`, which puts its data under the Unicode License v3, copied into vtium-font's letters file. |
| `Cargo.toml`, `Cargo.lock` | The generator crate, its own workspace. |
| `.gitignore` | Leaves out `target`. |
