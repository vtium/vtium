# HarfBuzz's shaping tests

The files here are the goldens that judge the glyph helper's shaper on syllabic scripts. A syllabic script, such as Khmer, Malayalam or Javanese, writes each syllable as a cluster of letters and marks, and the font decides how the cluster is drawn. The test `harfbuzz_judges_the_clusters_the_console_asks_for` in `src/shaping/harfbuzz.rs` reads them.

| File | What it is |
|---|---|
| `cases.tsv` | 180 test cases: each one's name, its font's file, its text as Unicode code points, and the output of HarfBuzz's `hb-shape` for that text, unchanged. |
| `differs.tsv` | The 9 cases that the helper cannot draw as `hb-shape` does, each with its reason. |
| `fonts/` | The 29 fonts that the cases use, unchanged. |
| `import` | The script that copied `cases.tsv` and `fonts/` from HarfRust. |
| `HarfRust-LICENSE.txt`, `HarfBuzz-COPYING.txt` | The MIT licences of HarfRust and HarfBuzz, which cover the test cases. |
| `Noto-LICENSE.txt`, `Apache-2.0.txt` | The licences of the fonts. See [The fonts](#the-fonts). |

## How the test judges the shaper

Each case gives a text and a font. The test writes the text on the first row of a vtium terminal and reads the clusters back, one cell at a time. Where the clusters of one syllable link, the test joins them, as the console joins the cells that hold them (ADR-1203). For example, ka with its virama links to the ra after it. Those are the clusters and syllables that the console asks the helper for. The test shapes each of them with the helper's own code, `shaping::shape::shape_all`, and lays them end to end. The result must be `hb-shape`'s glyphs, at `hb-shape`'s places, with `hb-shape`'s advance. The test compares glyph ids and positions in font units. It does not compare `hb-shape`'s cluster numbers, because the helper numbers each glyph by the character it comes from.

106 of the cases are one cluster each, and 74 are several clusters. 171 cases draw exactly as `hb-shape` draws them. `differs.tsv` lists the other 9, each under one of three reasons:

- **left-out:** the font has no glyph for a character, so `hb-shape` draws the font's .notdef glyph. The helper leaves such a cluster out, so that a later font in the frit, or the console, draws the cluster.
- **never-asked:** the text is only marks. A vtium terminal stores no mark at the start of a row, so the console never asks for such a text.
- **cut:** the terminal cuts the text into several clusters by Unicode's grapheme cluster rules (UAX #29), and the font shapes or positions glyphs across them. The helper shapes one cluster or one syllable at a time, so it cannot do that where the clusters are not one syllable. For example, the font kerns Tamil ta and virama against the superscript three after them, which no syllable joins.

The test fails when a case that `differs.tsv` does not list draws differently from `hb-shape`. It also fails when a listed case no longer differs for its reason, so the list stays true.

## Where the cases come from

HarfBuzz keeps its shaping tests in `test/shape/data/` in its repository. Each test gives a font, `hb-shape`'s options, a text and the expected output. HarfRust, the HarfBuzz project's Rust port and the shaper the helper uses, turns those tests into Rust tests by running `hb-shape` on each one. HarfRust's README says that its version 0.12.0 matches HarfBuzz 13.0.0.

The cases were copied on 2026-11-13 from HarfRust's tag `0.12.0`, commit `60b28ea22b5261710018d69c168a762bcb28794c`, which is the version vtium-config builds. They come from two of HarfRust's files. `harfrust/tests/shaping/in_house.rs` holds HarfBuzz's own tests, and `harfrust/tests/shaping/text_rendering_tests.rs` holds the Unicode Consortium's text rendering tests. The `import` script keeps a case when all of these hold:

- The case comes from a test file of a syllabic script. Those are HarfBuzz's Indic, Khmer, Myanmar, Sinhala, Tibetan and Universal Shaping Engine tests, and the Unicode tests of Balinese, Kannada and Tai Tham.
- The case gives `hb-shape` no options, so `hb-shape` chooses the script and the direction from the text, as the helper does.
- The text holds no default ignorable character, such as U+200D, and no space or control character. The helper removes default ignorable characters, and the console never asks for a space as a cluster.
- The font is a Noto font, by the family name in its name table.

None of the Unicode tests passed those rules, so every case comes from HarfBuzz's own tests. The script ran with fontTools 4.68.0, and the SHA-256 of the `cases.tsv` it wrote is `f0237f95b8d85f6d465c79a7b2a3744d4b92573def3ca17d06f3602bc768effd`. To copy the cases again, clone HarfRust at the tag that vtium-config's `harfrust` version names, and run `tests/harfbuzz/import /path/to/harfrust` from `crates/vtium-config`. A second run with the same checkout writes the same files.

The cases are goldens, so the author of a change never rewrites them. A new import goes in a commit of its own. That commit names the HarfRust tag and lists each case that was added, removed or changed, and the maintainer reviews it.

## The fonts

The fonts are subsets of Noto fonts from HarfBuzz's own tests, in `test/shape/data/in-house/fonts/`, and each file is named by its SHA-1. HarfRust copies them into its repository. HarfBuzz's `in-house` directory has no notice of its own. Their name tables keep the family name, and most keep a version and a copyright line, but no licence.

The Noto project publishes its fonts under the SIL Open Font License 1.1, which `Noto-LICENSE.txt` holds. The Noto fonts repository is `notofonts/noto-fonts`. The history of its `LICENSE` file shows the Apache License 2.0 from 2013-04-11, and the Open Font License from 2015-09-25. So a subset of a build from before 2015-09-25 may come under the Apache License 2.0, which `Apache-2.0.txt` holds. The table below says which subsets may. Those are the builds whose copyright line names 2013 or 2015, and the subsets whose name table gives no copyright line, and so no date. The maintainer decided on 2026-11-13 to keep these subsets and to ship both licences' texts beside them. Both licences let vtium copy a subset of the font with its licence. The Noto project's licence names no Reserved Font Name, so the subsets keep their names. The tests read the fonts, and vtium installs none of them.

| Font file | Family | Version | Copyright line | Cases | Licence |
|---|---|---|---|---|---|
| `fbb6c84c9e1fe0c39e152fbe845e51fd81f6748e.ttf` | Noto Sans Balinese | none | none | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `1ed7e9064f008f62de6ff0207bb4dd29409597a5.ttf` | Noto Sans Brahmi | none | none | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `28f497629c04ceb15546c9a70e0730125ed6698d.ttf` | Noto Sans Brahmi | Version 2.000 | Copyright 2018 Google Inc. All Rights Reserved. | 3 | OFL 1.1 |
| `96490dd2ff81233b335a650e7eb660e0e7b2eeea.ttf` | Noto Sans Cham | none | none | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `e68a88939e0f06e34d2bc911f09b70890289c8fd.ttf` | Noto Sans Cham | none | none | 3 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `9d8c53cb64b8747abdd2b70755cce2ee0eb42ef7.ttf` | Noto Sans Devanagari | Version 2.000 | Copyright 2015 Google Inc. All Rights Reserved. | 2 | OFL 1.1, or Apache 2.0 if built before 2015-09-25 |
| `dcf774ca21062e7439f98658b18974ea8b956d0c.ttf` | Noto Sans Grantha | Version 2.000 | Copyright 2018 Google Inc. All Rights Reserved. | 1 | OFL 1.1 |
| `1735326da89f0818cd8c51a0600e9789812c0f94.ttf` | Noto Sans Gurmukhi | none | none | 2 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `85fe0be440c64ac77699e21c2f1bd933a919167e.ttf` | Noto Sans Gurmukhi | none | none | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `f75c4b05a0a4d67c1a808081ae3d74a9c66509e8.ttf` | Noto Sans Gurmukhi | Version 1.03 uh | Copyright 2013 Google Inc. All Rights Reserved. | 2 | OFL 1.1, or Apache 2.0 if built before 2015-09-25 |
| `f70f345188472b93f565d1d7fae8c668dd6a3244.ttf` | Noto Sans Javanese | Version 2.003 | Copyright 2018-2022 Google Inc. All Rights Reserved. | 19 | OFL 1.1 |
| `3cae6bfe5b57c07ba81ddbd54c02fe4f3a1e3bf6.ttf` | Noto Sans Kannada | none | none | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `3d0b77a2360aa6faa1385aaa510509ab70dfbeff.ttf` | Noto Sans Kannada | none | none | 2 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `3c96e7a303c58475a8c750bf4289bbe73784f37d.ttf` | Noto Sans Kannada UI | Version 1.03 uh | Copyright 2013 Google Inc. All Rights Reserved. | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25 |
| `3998336402905b8be8301ef7f47cf7e050cbb1bd.ttf` | Noto Sans Khmer | Version 2.000 | Copyright 2016 Google Inc. All Rights Reserved. | 88 | OFL 1.1 |
| `ad01ab2ea1cb1a4d3a2783e2675112ef11ae6404.ttf` | Noto Sans Khmer | Version 2.001 | Copyright 2016 Google Inc. All Rights Reserved. | 1 | OFL 1.1 |
| `b6031119874ae9ff1dd65383a335e361c0962220.ttf` | Noto Sans Khmer | Version 2.000 | Copyright 2016 Google Inc. All Rights Reserved. | 25 | OFL 1.1 |
| `226bc2deab3846f1a682085f70c67d0421014144.ttf` | Noto Sans Malayalam | none | none | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `55e2910dbc9ef5dd89f4e146e7e0152169545b6a.ttf` | Noto Sans Malayalam | Version 2.101 | Copyright 2018 Google Inc. All Rights Reserved. | 15 | OFL 1.1 |
| `c825900b8a5b6571f0eb6c8c25c6512880bc42e9.ttf` | Noto Sans Malayalam | Version 2.104 | Copyright 2022 The Noto Project Authors (https://github.com/notofonts/malayalam) | 1 | OFL 1.1 |
| `e207635780b42f898d58654b65098763e340f5c7.ttf` | Noto Sans Malayalam | none | none | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `065b01e54f35f0d849fd43bd5b936212739a50cb.ttf` | Noto Sans Myanmar | 14.0d4e1 | Copyright 2015 Google Inc. All Rights Reserved. | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25 |
| `a232bb734d4c6c898a44506547d19768f0eba6a6.ttf` | Noto Sans Myanmar | Version 2.001 | Copyright 2015 Google Inc. All Rights Reserved. | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25 |
| `f4ba5a767ef56a40133844507efb98fee5635e71.ttf` | Noto Sans Myanmar | Version 2.107 | Copyright 2022 The Noto Project Authors (https://github.com/notofonts/myanmar) | 1 | OFL 1.1 |
| `573d3a3177c9a8646e94c8a0d7b224334340946a.ttf` | Noto Sans Newa | none | none | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `7c24183f26d60df414578a0a9f5e79ab9d32a22b.ttf` | Noto Sans Newa | Version 2.000 | Copyright 2018 Google Inc. All Rights Reserved. | 1 | OFL 1.1 |
| `54674a3111d209fb6be0ed31745314b7a8d2c244.ttf` | Noto Sans Tamil | none | none | 1 | OFL 1.1, or Apache 2.0 if built before 2015-09-25; the font gives no date |
| `e2b17207c4b7ad78d843e1b0c4d00b09398a1137.ttf` | Noto Sans Tamil MN | Version 2.000 | Copyright 2017 Google Inc. All Rights Reserved. | 1 | OFL 1.1 |
| `e716f6bd00a108d186b7e9f47b4515565f784f36.ttf` | Noto Sans Telugu | Version 2.001 | Copyright 2019-2022 Google LLC. All Rights Reserved. | 1 | OFL 1.1 |
