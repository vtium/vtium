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
  - evidence: "vtium-term cuts text into grapheme clusters by UAX #29. Rule GB9c keeps a consonant, a virama and the next consonant in one cluster only where the virama has Indic_Conjunct_Break=Linker, which DerivedCoreProperties.txt gives only to the viramas of Devanagari, Bengali, Gujarati, Oriya, Telugu and Malayalam (U+094D, U+09CD, U+0ACD, U+0B4D, U+0C4D, U+0D4D). IndicSyllabicCategory.txt gives Indic_Syllabic_Category=Virama to Kannada's U+0CCD, Tamil's U+0BCD, Sinhala's U+0DCA, Javanese's U+A9C0, Brahmi's U+11046 and Newa's U+11442, among others, and Invisible_Stacker to Myanmar's U+1039 and Khmer's U+17D2."
  - precedent: ADR-1004
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "as for indic choices, I lean b as well. I don't know much about ra and ka and other indic script but it looks cmore correct without me understanding how it joins"
    via: the maintainer, in the steering session, 2026-11-13, after the Indic Choices page showed Kannada conjuncts split and joined at 1:1
status: proposed
date: 2026-11-13
deciders:
  - aaronsb
related:
  - ADR-1004
  - ADR-1200
  - ADR-1302
  - ADR-1307
---

# ADR-1203: A syllable that the grapheme rules cut into several clusters is shaped once and drawn across their cells

## Summary

- **Decided:** Where Unicode's grapheme cluster rules cut one syllable into several clusters, as they cut every Kannada conjunct, the console asks the glyph helper for the whole syllable as one cluster. The helper shapes it once with HarfRust and draws it across the cells that its clusters take. The terminal keeps the clusters and the cells that UAX #29 gives, so the cursor, widths and what programs see do not change. Only the drawing joins.
- **Trades away:** some certainty about which cell holds which letter. A joined conjunct puts one letter's shape over another letter's cell, so vtium draws the syllable split again where the cursor stands on it or where its cells differ in color or style.
- **One-way?** No. The rule lives in the console's drawing and the helper's placement. No stored format or terminal state depends on it.
- **Probes:** *Not confident (cursor):* when the cursor stands on a joined syllable, vtium draws the syllable split, so that you can see which letter the cursor is on, as it already does for the lam-alef ligature. Is that what you want, or should the syllable stay joined under the cursor? *Not confident (scripts):* the rule joins every script whose virama Unicode marks, so Tamil, Sinhala, Javanese, Brahmi, Newa and others join too, not only Kannada. Is that what you want?
- **Inversion:** the choice sat between drawing each grapheme cluster on its own, which shows the virama and breaks the conjunct, and widening the terminal's clusters, which changes what programs count. Joining only the drawing keeps the terminal's clusters and still draws the conjunct.

## Context

A grapheme cluster takes the cells of its first character (ADR-1200 D21), and vtium-term finds clusters by UAX #29. Rule GB9c of UAX #29 keeps a consonant, a virama and the next consonant in one cluster only for the six scripts whose virama has Indic_Conjunct_Break=Linker: Devanagari, Bengali, Gujarati, Oriya, Telugu and Malayalam. In Kannada, and in Tamil, Sinhala, Javanese, Brahmi, Newa and other scripts, a conjunct is two or more clusters.

The console asks the helper for one cluster at a time, and the helper shapes each one on its own. So in those scripts a conjunct never forms. ಕ್ರ (ka, virama, ra) draws as ಕ್ in one cell and ರ in the next, with the virama showing, where a Kannada reader expects ra written under ka. HarfBuzz's own tests show the same: the cases that the terminal cuts are among the nine that the helper cannot draw as hb-shape draws them.

The maintainer compared the two drawings at 1:1 on the Indic choices page, split as vtium draws today and joined across the same cells, and chose the joined drawing.

ADR-1004 D6 already draws one shape across two cells: the lam-alef ligature, which the console asks for as one cluster and draws across the cells of lam and alef. This record draws a syllable the same way.

## Decision

### D1: the cells that join into one syllable

The console reads each row of the frame in the order its cells are stored, which is logical order (ADR-1004 D1). A cell joins the cell after it when all of these hold:

- the cell's cluster ends with a character whose Indic_Syllabic_Category is Virama or Invisible_Stacker, optionally followed by U+200D ZERO WIDTH JOINER;
- the next cell's cluster starts with a character whose Indic_Syllabic_Category is Consonant;
- both characters have the same Script property;
- both cells are on the same row of the screen.

A cluster that ends with U+200C ZERO WIDTH NON-JOINER does not end with a virama, so it joins nothing. That keeps the explicit virama that a writer asks for with U+200C, as the Unicode Standard's chapter 12 describes for Indic scripts. A run of joined cells is one syllable. ಸ್ತ್ರೀ is three clusters (ಸ್, ತ್ and ರೀ) and one syllable across three cells. A syllable spans at most four cells. A run that would span more is drawn split. Where UAX #29 already keeps the syllable in one cluster, as GB9c does in Devanagari, there is no boundary to join and nothing changes.

The tables come from Unicode's IndicSyllabicCategory.txt and Scripts.txt, at the Unicode version vtium's other tables use, generated by the generator that writes vtium-config's shaping tables.

### D2: the terminal keeps UAX #29's clusters

The terminal does not change. vtium-term stores each cluster in its own cells, gives each the width that ADR-1200 D21 states, and moves the cursor, erases, wraps, selects and copies by those clusters. A program that counts one cell per grapheme cluster keeps its columns. A mouse report names the cell under the pointer, as it does for any cell. The joining is found from the frame each time a row is drawn, so nothing about it is stored.

### D3: the console asks for the syllable, and the helper draws it across its cells

The console asks the helper for the syllable's whole text as one cluster, in the style and script of its first cell, beside each of its clusters on its own (ADR-1200 D21). The helper gives a key that vtium-term's segmentation cuts into several clusters the cells of all those clusters together, by the same rule it gives one cluster its cells. This amends ADR-1200 D21's rule for a glyph's cells.

The helper shapes the syllable once with HarfRust and places it in those cells as it places a cluster in its own. A syllable whose advance is wider than its cells is squeezed sideways to them at full height (ADR-1307 D1, with the syllable's cells as its span). A narrower one has its advance centred across them (ADR-1302 D2). Its ink reaches as any glyph's ink does (ADR-1302 D1, ADR-1306).

The renderer draws the syllable's glyph from its first cell across all its cells, and draws no glyph in the cells after the first. Each cell draws its own background, underline and other decorations, as it does today. The figure shows four Kannada words split as vtium draws them before this record, and joined as this record draws them, at 1:1 in 14×45 cells.

![Eight pictures in two columns, white text on black. The left column, split: ka with a visible virama beside ra; ra with a visible virama beside ka; sa, ta and ra each in its own cell with visible viramas; and the word kannada with a visible virama after the first na. The right column, joined: ka with ra written under it; ka with the reph above it; one stacked conjunct for stree; and kannada with the second na written under the first.](figures/ADR-1203-kannada-conjuncts.png)

### D4: when a syllable draws split

The syllable draws split, each cluster in its own cell with its own glyph as before this record, when any of these hold:

- the cursor stands on any of its cells, in either phase of its blink, as a lam-alef ligature draws as two letters under the cursor (ADR-1004 D6);
- its cells differ in foreground color, in style (bold or italic) or in script (superscript or subscript). A selection that covers part of the syllable gives its cells different colors, so a partly selected syllable draws split, and a wholly selected one draws joined in the selection's colors;
- a link's overlay covers any of its cells;
- the helper has not answered for the syllable yet, or no source in the frit shapes it as a whole. The console then draws each cluster by the lookup chain (ADR-1200 D22).

A change to any cell of a syllable changes how the syllable draws, so it counts as an ink change in every cell of the syllable (ADR-1302 D9), and the renderer draws all of them again.

### D5: the records this amends

- **ADR-1200 D21:** a key that vtium-term cuts into several clusters takes the cells of all of them. The rest of the rule for a glyph's cells stands.
- **ADR-1307 D1:** a joined syllable of an Indic script whose advance is wider than its cells is squeezed to all its cells, not to one.

## Verification

- vtium-render, or the crate that finds the joined spans: the syllable finder joins ಕ್ರ, ರ್ಕ and ಕನ್ನಡ's ನ್ನ across two cells, and ಸ್ತ್ರೀ across three. It joins nothing after U+200C, nothing across two scripts, nothing across a row's end, and no run of more than four cells. Under the cursor, across a color change, across a style change and under a link's overlay, it draws split.
- `crates/vtium-config/tests/serve.rs` or `tests/shaping.rs`: a key of several clusters takes the cells of all of them and is drawn across them, squeezed to them where it is wider.
- `crates/vtium-config/tests/harfbuzz/`: the golden test joins syllables as the console does. The cut cases whose boundary D1 joins, such as use_indic3_001, indic_special_cases_001 and use_010, then draw as hb-shape draws them. They leave `differs.tsv` in a commit of their own, which the maintainer reviews.
- Tier 2: a screen draws ಕ್ರ, ರ್ಕ, ಸ್ತ್ರೀ and ಕನ್ನಡ in the shipped `indic` font set, provisional until the maintainer reviews it at 1:1. The pictures in the figure are the reference the maintainer chose from.

## Consequences

### Positive

- Kannada conjuncts, and the conjuncts of the other scripts whose viramas UAX #29 does not join, draw as their fonts design them.
- Programs keep their columns, because the terminal's clusters and widths stay UAX #29's.

### Negative

- A joined syllable draws one letter's shape over another letter's cell. The cursor and a partial selection split it, so the drawing changes as the cursor moves through a word.
- The console finds syllables in every drawn row of these scripts, and asks the helper for one more key for each syllable.

### Neutral

- The scripts whose viramas UAX #29 already joins draw as before.
- UAX #29 gives Myanmar's visarga and tone marks clusters of their own, because it counts neither as a spacing mark. They are not syllables by D1, so each still draws on a dotted circle. A later record can decide them.

## Alternatives Considered

- **Draw each grapheme cluster on its own,** as vtium did before this record. Not taken by the maintainer: the virama shows and the conjunct never forms.
- **Widen the terminal's clusters** so that a syllable is one cluster, by a rule like GB9c for every virama. Not taken: it changes the terminal's width and cursor rules, and what programs that follow UAX #29 expect of them, as ADR-1307 also declined.
- **Shape a whole run of a script at once,** as a word processor does. Not taken: the terminal's cells fix each glyph's place, so a run's shaping can only be laid out syllable by syllable, and a syllable is the unit that D1 finds.
