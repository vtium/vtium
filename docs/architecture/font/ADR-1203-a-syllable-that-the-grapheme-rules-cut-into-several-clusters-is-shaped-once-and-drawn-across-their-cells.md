---
contract: adr/v1
kind: decision
verb: add
capability: renderer
amends:
  - "ADR-1200#D21: cluster asks and style asks"
extends:
  - ADR-1307
basis:
  - evidence: "Drawn by this record's agent on 2026-11-13 with the real Noto Sans Kannada from tier 2's font cache, at 32 pixels of text in Noto Sans Mono's 14-pixel cell with 40 per cent of line spacing, through the glyph helper's own rasteriser (`crates/vtium-config/tests/indic_sizes.rs`, `draw_pictures`): ಕ್ರ, ರ್ಕ, ಸ್ತ್ರೀ and ಕನ್ನಡ, each split into the clusters vtium-term makes and each joined, the syllable shaped once across the same cells. Split, the virama shows and no reph or subjoined form appears; joined, ra stands under ka, the reph sits on ka, ಸ್ತ್ರೀ is one stacked conjunct and ನ್ನ shows its subjoined na. The figure is these pictures at 1:1."
  - evidence: "HarfBuzz's shaping tests, copied into `crates/vtium-config/tests/harfbuzz/` from HarfRust 0.12.0, judge the helper's shaper. 168 of 180 cases draw as hb-shape draws them. Nine differ because the terminal cuts the text into several clusters and the font shapes or positions glyphs across them (`differs.tsv`, reason `cut`): among them Kannada ka, virama, ra (use_indic3_001) and ra, virama, ka (indic_special_cases_001), and Newa's conjuncts (use_010)."
  - evidence: "vtium-term cuts text into grapheme clusters by UAX #29, with tables of Unicode 18.0.0 (`crates/vtium-term/src/unicode/data.rs`, `VERSION`). Rule GB9c keeps a consonant, a linker and the next consonant in one cluster only where the linker has Indic_Conjunct_Break=Linker. UAX #44 revision 38, section 5.3.1, derives that value from Indic_Syllabic_Category Virama, Invisible_Stacker or Consonant_With_Stacker, but only in the scripts it lists: Devanagari, Bengali, Gujarati, Oriya, Telugu, Malayalam, Balinese and Javanese, and the twelve scripts that join through an invisible stacker, among them Myanmar, Khmer, Tai Tham, Sundanese and Chakma. DerivedCoreProperties-18.0.0.txt lists 22 such code points. IndicSyllabicCategory-18.0.0.txt gives Virama, outside those scripts, to Gurmukhi U+0A4D, Tamil U+0BCD, Kannada U+0CCD, Sinhala U+0DCA, Syloti Nagri U+A806, Saurashtra U+A8C4, Brahmi U+11046, Kaithi U+110B9, Sharada U+111C0, Khojki U+11235, Grantha U+1134D, Newa U+11442, Tirhuta U+114C2, Siddham U+115BF, Modi U+1163F, Takri U+116B6, Dogra U+11839, Nandinagari U+119E0 and Bhaiksuki U+11C3F; Invisible_Stacker to Masaram Gondi U+11D45 and Gunjala Gondi U+11D97; and Consonant_With_Stacker to Kannada U+0CF1 and U+0CF2, Brahmi U+11003 and U+11004, and Newa U+11460 and U+11461."
  - evidence: "Research relayed by the steering session, 2026-11-13. Every shaper forms syllables across grapheme clusters: HarfBuzz shapes a run of text, Microsoft's Indic shaper works syllable by syllable, Chromium and Firefox shape through HarfBuzz, and Android's Minikin does too. Editors never put a caret inside a syllable: Microsoft's script specification for Kannada says so, Uniscribe marks only syllable starts as caret stops (SCRIPT_LOGATTR.fCharStop), and Minikin's GraphemeBreak.cpp, lines 113 to 119, does the same. Browsers keep a ligature joined when part of it changes color or is selected, and clip it. Gecko clips it into equal horizontal pieces, which fails for stacked forms, and Blink painted the whole ligature in its first component's color. CSS Text Level 3, section 7.3, asks that a change of style that changes no glyph, such as color, not break shaping, and the CSS Working Group's open discussion of partial styling inside a ligature leaves the outcome undefined. Inferred, not proven: HarfBuzz's Indic syllable grammar and GB9c set no limit on a syllable's consonants, and Sanskrit written in Kannada has syllables of five consonants, as in ಕಾರ್ತ್ಸ್ನ್ಯ (kārtsnya)."
  - precedent: ADR-1004
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "as for indic choices, I lean b as well. I don't know much about ra and ka and other indic script but it looks cmore correct without me understanding how it joins"
    via: the maintainer, in the steering session, 2026-11-13, after the Indic Choices page showed Kannada conjuncts split and joined at 1:1
  - operator: aaronsb
    said: "Split under the cursor"
    via: the maintainer, through the steering session's question tool, 2026-11-13, answering ADR-1203's cursor probe
    covers: [cursor]
  - operator: aaronsb
    said: "All virama scripts"
    via: the maintainer, through the steering session's question tool, 2026-11-13, answering ADR-1203's scripts probe
    covers: [scripts]
  - operator: aaronsb
    said: "1203 seems fine and what we discussed"
    via: the maintainer, in the steering session, 2026-11-13, after reading the revised record on the pushed branch docs/indic-records
status: accepted
date: 2026-11-13
deciders:
  - aaronsb
related:
  - ADR-1004
  - ADR-1006
  - ADR-1200
  - ADR-1302
  - ADR-1307
---

# ADR-1203: A syllable that the grapheme rules cut into several clusters is shaped once and drawn across their cells

## Summary

- **Decided:** Where Unicode's grapheme cluster rules cut one syllable into several clusters, as they cut every Kannada conjunct, the console asks the glyph helper for the whole syllable as one cluster. The helper shapes it once with HarfRust and draws it across the cells that its clusters take. The terminal keeps UAX #29's clusters, and each keeps the width ADR-1006 gives it, so the cursor and what programs see do not change. Only the drawing joins.
- **Trades away:** some certainty about which cell holds which letter. A joined conjunct puts one letter's shape over another letter's cell, so vtium draws the syllable split again where the cursor stands on it or where its cells differ in color or style.
- **One-way?** No. The rule lives in the console's drawing and the helper's placement. No stored format or terminal state depends on it.
- **Probes:** *Not confident (cursor):* when the cursor stands on a joined syllable, vtium draws the syllable split, so that you can see which letter the cursor is on, as it already does for the lam-alef ligature. Is that what you want, or should the syllable stay joined under the cursor? *Not confident (scripts):* the rule joins every script whose virama Unicode marks, so Tamil, Sinhala, Javanese, Brahmi, Newa and others join too, not only Kannada. Is that what you want?
- **Inversion:** the choice sat between drawing each grapheme cluster on its own, which shows the virama and breaks the conjunct, and widening the terminal's clusters, which changes what programs count. Joining only the drawing keeps the terminal's clusters and still draws the conjunct.

## Context

vtium-term finds grapheme clusters by UAX #29, and gives each cluster its cells by ADR-1200 D21, as ADR-1006 widens them. Rule GB9c of UAX #29 keeps a consonant, a virama and the next consonant in one cluster only where the virama has Indic_Conjunct_Break=Linker. In Unicode 18.0, vtium-term's version, that holds for the viramas of Devanagari, Bengali, Gujarati, Oriya, Telugu, Malayalam, Balinese and Javanese. It also holds for the invisible stackers of twelve scripts, such as Myanmar and Khmer, which join consonants by stacking them. In Kannada, Tamil, Sinhala, Gurmukhi, Brahmi, Newa and many other scripts, a conjunct is two or more clusters.

> Edited 2026-11-13: research found that Unicode 17.0 gave Indic_Conjunct_Break=Linker to Balinese, Javanese and the twelve scripts of invisible stackers, so GB9c already joins Javanese, Myanmar and Khmer. The paragraph said GB9c joined only six scripts, and named Javanese among those it cuts; the third basis entry said the same and is corrected.

The console asks the helper for one cluster at a time, and the helper shapes each one on its own. So in those scripts a conjunct never forms. ಕ್ರ (ka, virama, ra) draws as ಕ್ in one cell and ರ in the next, with the virama showing, where a Kannada reader expects ra written under ka. HarfBuzz's own tests show the same: the cases that the terminal cuts are among the nine that the helper cannot draw as hb-shape draws them.

The maintainer compared the two drawings at 1:1 on the Indic choices page, split as vtium draws today and joined across the same cells, and chose the joined drawing.

ADR-1004 D6 already draws one shape across two cells: the lam-alef ligature, which the console asks for as one cluster and draws across the cells of lam and alef. This record draws a syllable the same way.

## Decision

### D1: the cells that join into one syllable

The console reads each row of the frame in the order its cells are stored, which is logical order (ADR-1004 D1). A cell joins the cell after it when all of these hold:

- the cell's cluster ends with a linker, optionally followed by U+200D ZERO WIDTH JOINER. A linker is a character whose Indic_Syllabic_Category is Virama, Invisible_Stacker or Consonant_With_Stacker;
- a consonant comes before the linker in the cell's cluster, as GB9c asks. A Consonant_With_Stacker is its own consonant;
- the next cell's cluster starts with a character whose Indic_Syllabic_Category is Consonant. After an Invisible_Stacker, it may also be Vowel_Independent;
- the two characters share a script: a value of the linker's Script_Extensions is also one of the follower's;
- both cells are on the same row of the screen.

This is the rule by which UAX #44, revision 38, section 5.3.1, derives the Indic_Conjunct_Break values that GB9c joins by, with its list of scripts left out. So D1 joins every script whose Unicode data marks a virama or a stacker, as the maintainer chose. For example, it joins Kannada's ೱ (U+0CF1, a consonant with its own stacker) to the ka after it, and does the same for Brahmi's U+11003 and Newa's U+11460.

A cluster that ends with U+200C ZERO WIDTH NON-JOINER does not end with a linker, so it joins nothing. That keeps the explicit virama that a writer asks for with U+200C, as the Unicode Standard's chapter 12 describes for Indic scripts. A run of joined cells is one syllable. ಸ್ತ್ರೀ is three clusters (ಸ್, ತ್ and ರೀ) and one syllable across their cells. Where UAX #29 already keeps the syllable in one cluster, as GB9c does in Devanagari, Myanmar or Khmer, there is no boundary to join and nothing changes.

A syllable's text holds at most 15 scalar values: one fewer than `CLUSTER_SCALARS_MAX`, 16 today, the most the glyph helper takes in one key (`crates/vtium-font/src/bounds.rs`). Each piece's key carries the syllable's text and one more scalar value, the piece's mark (D3). A run whose text would pass 15 draws split. The scripts set no limit on a syllable's consonants, and Sanskrit written in Kannada has syllables of five, such as the rtsny of ಕಾರ್ತ್ಸ್ನ್ಯ. Those five consonants and their four viramas are nine scalar values, so they fit, with room for marks. The bound comes from the key's own bound, so the finder's limit cannot disagree with the helper's.

The tables come from Unicode's IndicSyllabicCategory.txt and ScriptExtensions.txt, with Scripts.txt for the characters that ScriptExtensions.txt does not list, at the Unicode version vtium's other tables use. The generator that writes vtium-config's shaping tables writes them.

> Edited 2026-11-14: the build of this record (the work that draws joined syllables). The bound read: "A syllable's text holds at most `CLUSTER_SCALARS_MAX` scalar values, 16 today". A piece's key spends one scalar value on the piece's mark, so a syllable's text holds 15. D1 had no condition on what comes before the linker, so the x in x, virama, ra joined ra as if x were a consonant.

> Edited 2026-11-13: research on Unicode 18.0 and on HarfBuzz's grammar (the basis). D1 read: the cluster ends with a character whose Indic_Syllabic_Category is Virama or Invisible_Stacker; the next starts with a Consonant; "both characters have the same Script property"; "A syllable spans at most four cells. A run that would span more is drawn split." Consonant_With_Stacker was missing, so ೱಕ, Brahmi's and Newa's never joined; Script left out characters shared between scripts; and the limit of four cells split a syllable of five consonants.

### D2: the terminal keeps UAX #29's clusters

The terminal does not change for this record. vtium-term stores each cluster in its own cells and gives each the width that ADR-1200 D21 states, as ADR-1006 widens it. It moves the cursor, erases, wraps, selects and copies by those clusters. A joined syllable takes the sum of its clusters' cells. Under ADR-1006, ಸ್ತ್ರೀ takes 1, 1 and 2 cells, four in all, because the vowel sign of ರೀ is a spacing mark. A program that measures each cluster as ADR-1006 does keeps its columns. If ADR-1006 is not accepted, or when its setting `cluster-width` is `single`, each cluster keeps ADR-1200 D21's width, and ಸ್ತ್ರೀ takes three cells. The joining works the same under both values of that setting. A mouse report names the cell under the pointer, as it does for any cell. The joining is found from the frame each time a row is drawn, so nothing about it is stored.

> Edited 2026-11-13: ADR-1006, proposed the same day, gives a cluster with a spacing mark or a virama conjunct two cells. The paragraph said each cluster took ADR-1200 D21's width, and that a program counting one cell per grapheme cluster kept its columns.

### D3: the console asks for the syllable, and the helper draws it across its cells

The console asks the helper for each piece of the syllable, in the style and script of its first cell, beside each of its clusters on its own (ADR-1200 D21). A piece is the part of the syllable's drawing that lies in one cluster's cells. Its key is the syllable's whole text followed by the piece's mark. The mark is a character of Supplementary Private Use Area-B that names where the cluster's cells lie among the syllable's cells, and how many cells the syllable takes. The console counts those cells from its own frame, so they follow the terminal's `cluster-width` (ADR-1006 D8). vtium.conf sets `cluster-width` at its top level alone, so the console's store of the helper's glyphs assumes one rule for all heads. This amends ADR-1200 D21's rule for a glyph's cells.

The helper shapes the syllable's whole text once with HarfRust and places the drawing across all the syllable's cells, as it places a cluster in its own. A syllable whose advance is wider than its cells is squeezed sideways to them at full height, in any script (ADR-1307 D1, with the syllable's cells as its span). A narrower one has its advance centred across them (ADR-1302 D2). Its ink reaches as any glyph's ink does (ADR-1302 D1, ADR-1306). The helper then cuts the drawing at the edges of each cluster's cells, and sends each piece under its key. The ink past the syllable's left side goes with the first piece, and the ink past its right side with the last.

Each cell of the syllable draws its own piece. The pieces side by side are the syllable's drawing, so the pixels equal one glyph drawn across the cells. Each cell draws its own background, underline and other decorations, as it does today. The figure shows four Kannada words split as vtium draws them before this record, and joined as this record draws them, at 1:1 in 14×45 cells. It gives each cluster one cell, as ADR-1200 D21 did before ADR-1006, so ಸ್ತ್ರೀ spans three cells there.

![Eight pictures in two columns, white text on black. The left column, split: ka with a visible virama beside ra; ra with a visible virama beside ka; sa, ta and ra each in its own cell with visible viramas; and the word kannada with a visible virama after the first na. The right column, joined: ka with ra written under it; ka with the reph above it; one stacked conjunct for stree; and kannada with the second na written under the first.](figures/ADR-1203-kannada-conjuncts.png)

### D4: when a syllable draws split

The syllable draws split, each cluster in its own cell with its own glyph as before this record, when any of these hold:

- the cursor stands on any of its cells, in either phase of its blink, as a lam-alef ligature draws as two letters under the cursor (ADR-1004 D6);
- its cells differ in foreground color, in style (bold or italic) or in script (superscript or subscript). A selection that covers part of the syllable gives its cells different colors, so a partly selected syllable draws split, and a wholly selected one draws joined in the selection's colors;
- a link's overlay covers any of its cells;
- the helper has not answered for the syllable yet, or no source in the frit shapes it as a whole. The console then draws each cluster by the lookup chain (ADR-1200 D22).

A change to any cell of a syllable changes how the syllable draws, so it counts as an ink change in every cell of the syllable (ADR-1302 D9), and the renderer draws all of them again.

Blink and conceal act on each cell alone: a cell that blinks or is concealed hides its own piece. A selection splits a syllable through the colors it gives its cells, as the second item says, and in no other way. Only a build with the `bidi` feature joins syllables. The minspec build, which leaves that feature out, draws every syllable split, though it loads outline font sets through the helper.

A bitmap font draws Indic scripts on a best-effort basis: vtium orders and shapes them where its existing shaping and the font's own glyphs allow, and conjuncts the font has no glyph for show as their pieces. A cluster the helper drew nothing for is drawn from its characters' glyphs (ADR-1302 D14). Each letter after a virama or a stacker is drawn after the letter before it, with its own marks. In a cluster of two cells, each piece takes a whole cell of its own. A spacing vowel sign whose body stands before its letter, such as ि, comes first, then the letter with its nonspacing marks over it, then each other spacing vowel sign and each letter after a linker. vtium builds no shaping of its own for glyphs a bitmap font lacks.

> Edited 2026-11-14: the build of this record (the work that draws joined syllables). D3 read: "The console asks the helper for the syllable's whole text as one cluster" and "The renderer draws the syllable's glyph from its first cell across all its cells, and draws no glyph in the cells after the first." vtium-font's glyphs, the glyph sets and the GPU's glyph slots hold at most two cells, and a syllable takes up to 30, so each cell draws its piece instead, and the pixels are the same. The squeeze named no script; the code first squeezed only the blocks from Devanagari to Sinhala, and now squeezes a syllable of any script, as D5 says. The paragraphs on blink, conceal, selection, the bidi feature and bitmap fonts, and the sentence on the store's one rule for all heads, were added: the record was silent on them. The maintainer set the scope for bitmap fonts on 2026-11-14.

### D5: the records this amends

- **ADR-1200 D21:** a key that vtium-term cuts into several clusters takes the cells of all of them. The rest of the rule for a glyph's cells stands.
- **ADR-1307 D1:** a joined syllable of any script whose advance is wider than its cells is squeezed to all its cells, not to one.

> Edited 2026-11-14: the item read "a joined syllable of an Indic script". The maintainer chose to squeeze every syllable the helper shapes, in any script, so that no syllable's ink spills past its cells.

## Verification

- vtium-render, or the crate that finds the joined spans: the syllable finder joins ಕ್ರ, ರ್ಕ and ಕನ್ನಡ's ನ್ನ, ಸ್ತ್ರೀ's three clusters, and the five consonants of ಕಾರ್ತ್ಸ್ನ್ಯ. It joins ೱ (U+0CF1) to ka, Brahmi's U+11003 and Newa's U+11460 to the consonant after each, and a Masaram Gondi consonant after its stacker U+11D45. It joins nothing after U+200C, nothing across two scripts that share no Script_Extensions value, nothing across a row's end, and no run whose text passes 16 scalar values. Under the cursor, across a color change, across a style change and under a link's overlay, it draws split.
- `crates/vtium-config/tests/serve.rs` or `tests/shaping.rs`: a key of several clusters takes the cells of all of them and is drawn across them, squeezed to them where it is wider.
- `crates/vtium-config/tests/harfbuzz/`: the golden test joins syllables as the console does. The cut cases whose boundary D1 joins, such as use_indic3_001, indic_special_cases_001 and use_010, then draw as hb-shape draws them. They leave `differs.tsv` in a commit of their own, which the maintainer reviews.
- Tier 2: a screen draws ಕ್ರ, ರ್ಕ, ಸ್ತ್ರೀ and ಕನ್ನಡ in the shipped `indic` font set, provisional until the maintainer reviews it at 1:1. The pictures in the figure are the reference the maintainer chose from.

## Consequences

### Positive

- Kannada conjuncts, and the conjuncts of the other scripts whose viramas UAX #29 does not join, draw as their fonts design them.
- Programs keep their columns, because the terminal's clusters stay UAX #29's and their widths ADR-1006's.
- One rule joins every script whose Unicode data marks a virama or a stacker. It is the rule Unicode itself uses for GB9c, without GB9c's list of scripts.

### Negative

- A joined syllable draws one letter's shape over another letter's cell. The cursor and a partial selection split it, so the drawing changes as the cursor moves through a word.
- The console finds syllables in every drawn row of these scripts, and asks the helper for one more key for each syllable.

### Neutral

- The scripts whose viramas UAX #29 already joins draw as before.
- UAX #29 gives Myanmar's visarga and tone marks clusters of their own, because it counts neither as a spacing mark. They are not syllables by D1, so each still draws on a dotted circle. A later record can decide them.

## Alternatives Considered

- **Draw each grapheme cluster on its own,** as vtium did before this record. Not taken by the maintainer: the virama shows and the conjunct never forms.
- **Widen the terminal's clusters** so that a syllable is one cluster, by a rule like GB9c for every virama. Not taken: it changes the terminal's width and cursor rules, and what programs that follow UAX #29 expect of them, as ADR-1307 also declined.
- **Keep the syllable joined under the cursor and across color changes, and clip it at each cell's edge,** as browsers keep a ligature joined under part of a selection. Not taken: an editor never puts its caret inside a syllable, so it never has to show which letter the caret is on. A terminal must show the cursor in whichever cell the program puts it. Clipping fails for stacked forms: Gecko clips a ligature into equal pieces side by side, but a subjoined letter stands under the letter before it, not beside it. Blink painted the whole ligature in the color of its first part, which loses the color the program set for the rest. CSS Text Level 3 asks that a color change not break shaping, but the CSS Working Group leaves what then happens undefined. D4's split shows each letter in its own cell with its own color.
- **Shape a whole run of a script at once,** as a word processor does. Not taken: the terminal's cells fix each glyph's place, so a run's shaping can only be laid out syllable by syllable, and a syllable is the unit that D1 finds.
