---
contract: adr/v1
kind: decision
verb: change
capability: renderer
amends:
  - "ADR-1200#D17: the cell"
extends:
  - ADR-1302
basis:
  - operator: aaronsb
    level: directed
    said: "Squeeze each Indic cluster and letter sideways to its cell, at full height."
    via: "the maintainer in the steering session, 2026-11-12, choosing among three option pictures drawn at 1:1 by this record's agent (both rows of this record's figure, and a third row with clusters drawn smaller); relayed by the team lead; the wording is this record's agent's"
    paraphrase: true
  - evidence: "Measured by this record's agent on 2026-11-12 in tier 2 (`tools/tier2`, the screen `scripts-indic`) with Iosevka Term 34.9.1 and Noto Sans Devanagari and Noto Sans Bengali from the Noto project's release 24.9.1, in Iosevka Term's 16x40 cell at 40 pixels: Noto Sans Devanagari's letters are about 0.6 em wide and its conjuncts up to about 1 em, where the cell is 0.5 em. The glyph helper drew a cluster of several characters at its full width, centred on its one cell, so its ink covered both neighbours; it drew a single letter from the same font smaller until it fitted the cell (ADR-1200 D17), so one word mixed small letters with large, overlapping clusters. DejaVu Sans Mono's cell, 0.6 em, showed the same, less badly."
  - evidence: "vtium-term gives a grapheme cluster the cells of its first character (`crates/vtium-term/src/unicode.rs`, `width_of`; ADR-1200 D21), so a Devanagari conjunct such as स्व or न्त्र takes one cell."
  - precedent: ADR-1302
agent:
  name: Claude
  model: claude-opus-5-5
status: accepted
date: 2026-11-12
deciders:
  - aaronsb
related:
  - ADR-1301
  - ADR-1302
  - ADR-1200
---

# ADR-1307: An Indic cluster wider than its cell is squeezed sideways to it at full height

## Summary

- **Decided:** The glyph helper squeezes each cluster and each letter of an Indic script whose advance is wider than its cell sideways, just enough that its advance fills the cell, at the font's full height. It never draws one smaller. The cluster keeps the one cell the terminal gives it. This is the later record ADR-1302 D2 names for Devanagari and the other scripts whose letters join through a headline, and it answers how a shaped cluster in a syllabic script maps onto cells, which ADR-1301 leaves open.
- **Trades away:** letter shapes. A conjunct of three consonants is squeezed to about half its width in a cell half an em wide.
- **One-way?** No. The rule lives in the helper, and no stored format or terminal width depends on it.
- **Probes:** *Confident:* the words no longer collide, and every letter keeps the height of its neighbours. *Not confident:* whether a cell much narrower than 0.5 em squeezes conjuncts past reading; a later record can give such clusters two cells.
- **Inversion:** the choice sat between drawing a cluster smaller and letting it overflow its cell. Squeezing sideways keeps both the height and the cell.

## Context

A grapheme cluster takes the cells of its first character (ADR-1200 D21), so a Devanagari or Bengali cluster takes one cell whatever its length: स, स्व and न्त्र each take one. An Indic font's letters are wider than a monospace cell, and its conjuncts are wider still: Noto Sans Devanagari's letters are about 0.6 em, its conjuncts up to about 1 em, and Iosevka Term's cell is 0.5 em.

Before this record the helper drew such text two ways. A cluster of several characters, which the helper shapes whole, was placed by its advance and centred on its cell at its full size, and its ink past the cell reached both neighbours (ADR-1302 D1, D2). A single letter from a later source of the frit was drawn smaller until its ink fitted the cell (ADR-1200 D17). So one word mixed small letters with full-size clusters that ran over each other. The top row of the figure shows it.

ADR-1302 D2 says that Devanagari and the other scripts whose letters join through a headline need a rule of their own, which a later record decides. ADR-1301's open questions leave how a shaped cluster in a syllabic script maps onto cells to the work on shaping, which asks for a decision record first. This is that record.

## Decision

### D1: an Indic cluster or letter wider than its cell is squeezed sideways

- **Which glyphs.** A cluster or a single letter whose first character lies in the Unicode blocks from Devanagari to Sinhala, U+0900 to U+0DFF: Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam and Sinhala. The rule holds for an outline glyph from any source of a frit, the first included.
- **How.** When the glyph's advance, as the font gives it at the cell's size, is wider than its cells, the helper scales the glyph sideways before it is rasterised, by its cells' width over its advance, unrounded, and its pen sits on its cells' left edge. The whole shaped cluster is scaled together, every glyph and every offset HarfRust gave it, so each mark stays over its letter. The advance is what is scaled, not the ink, so ink a font draws past its advance still reaches the neighbour within ADR-1302 D1's reach. This is ADR-1302 D2's stretch for joining scripts, applied only to glyphs wider than their cells.
- **Full height.** The glyph keeps the size the frit draws its source at (ADR-1200 D17) and stands on the first source's baseline. ADR-1302 D3's mark rule, with ADR-1306 D2's edges, still moves or scales a stack of marks that passes the reach above or below.
- **Narrower glyphs.** A cluster or letter no wider than its cells keeps ADR-1302 D2's placement: its advance centred across its cells.
- **The cells.** The terminal's width does not change. A cluster takes the cells of its first character, as ADR-1200 D21 states.

The figure shows the first sentence of the Universal Declaration of Human Rights in Hindi and in Bengali, drawn by tier 2 in Iosevka Term's 16x40 cell with Noto Sans Devanagari and Noto Sans Bengali. The top row is the drawing before this record: clusters overflow their cells and single letters are drawn small. The bottom row is this decision: each cluster and letter is squeezed to its cell, at full height.

![Two drawings of the same two lines, Hindi above Bengali, each labelled. Before: the letters of each word overlap one another, conjuncts run over their neighbours, and single letters such as the first of each word stand smaller than the rest. Decided: every letter and conjunct stands at the same height in its own cell, narrower where it is wide, and no ink runs over a neighbour.](figures/ADR-1307-indic-clusters.png)

### D2: the records this amends

- **ADR-1200 D17:** a later source's glyph whose ink would pass its cells is drawn smaller to fit them, unless it is a glyph of a joining script. An Indic glyph wider than its cell is a second exception: it is squeezed sideways as D1 says, and not drawn smaller. Its ink above and below the row follows ADR-1306's reach, as a joining script's does.
- **ADR-1302 D2:** its bullet on Devanagari and the scripts that join through a headline names a later record; this is it. D1 does not make a headline meet across cells. A cluster narrower than its cell leaves its headline short, as it did before.

## Verification

- `crates/vtium-config/tests/shaping.rs`: a cluster of several characters wider than its cell, and a single letter wider than its cell from a later source, are each drawn at full height with their ink across the whole cell and no more than the font's own overhang past it.
- Tier 2's `scripts-indic` screen draws the figure's two lines, provisional until the maintainer reviews it at 1:1.

## Consequences

### Positive

- Indic words no longer collide in a narrow cell, and every letter keeps the height of its neighbours.
- A program that counts one cell per grapheme cluster keeps its columns.

### Negative

- A long conjunct in a narrow cell is squeezed hard, to about half its width at Iosevka Term's 0.5 em.
- Headlines still break between clusters narrower than their cells.

### Neutral

- No other golden moves: no other screen draws Indic text in an outline font.

## Alternatives Considered

- **Draw each cluster smaller until it fits its cell,** as ADR-1200 D17 draws single letters. Not taken by the maintainer: conjuncts become tiny and the height of letters jumps within a word.
- **Give a cluster more cells,** such as one for each consonant and spacing vowel sign, as a sum of character widths counts them. Not taken: it changes the terminal's width rule and what programs expect of it, and the glyph format holds glyphs of one or two cells only.
- **Leave clusters at their full size,** overflowing into their neighbours within ADR-1302 D1's reach. This was the drawing before this record, and it is the defect.
