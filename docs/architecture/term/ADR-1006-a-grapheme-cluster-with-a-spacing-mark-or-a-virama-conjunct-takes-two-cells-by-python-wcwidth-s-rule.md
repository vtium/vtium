---
contract: adr/v1
kind: decision
verb: change
capability: screen
amends:
  - "ADR-1200#D21: cluster asks and style asks"
  - "ADR-1307#D1: an Indic cluster or letter wider than its cell is squeezed sideways"
basis:
  - standard: "Python wcwidth's Specification (https://wcwidth.readthedocs.io/en/latest/specs.html, the source docs/specs.rst of github.com/jquast/wcwidth), for release 0.9.4 with tables of Unicode 18.0, read 2026-11-13. \"Width of 2\": \"Any character of non-zero width followed by an Mc (Spacing Combining Mark) character when measured in sequence by wcwidth.wcswidth or wcwidth.width. The Mc character caps the cluster width at 2 … Zero-width combining marks (Mn) between the base character and the Mc do not break the association\"; \"Any grapheme cluster width is limited to 2 cells since 0.8.0\". \"Virama Conjunct Formation\": a Virama contributes 0 width; six viramas have category Mc and \"are recognised as viramas, so they begin a conjunct\"; \"A Consonant immediately following a Virama adds its width to the current grapheme cluster\"; \"The cluster total is capped at 2 cells\"; Mn marks do not break conjunct context; ZWJ after a virama is consumed without breaking it. \"Scope\": \"Each string yielded by wcwidth.iter_graphemes may be mapped to wcwidth.wcswidth to accurately measure the width of a grapheme\", and \"Tamil, Kannada and Sinhala text is measured differently than it is segmented\". The specification's Virama Conjunct Formation counts Invisible_Stacker as a virama."
  - standard: "Unicode 18.0.0: DerivedGeneralCategory.txt gives General_Category Mc to 477 code points; IndicSyllabicCategory.txt gives Virama to 27 code points, of which six are Mc (Balinese U+1B44, Javanese U+A9C0, Sharada U+111C0, Khojki U+11235, Grantha U+1134D, Takri U+116B6), and Invisible_Stacker to 14."
  - standard: "The terminal-unicode-core specification for mode 2027 (github.com/contour-terminal/terminal-unicode-core, spec/terminal-unicode-core.tex, read 2026-11-13), section Grapheme Cluster: \"extending a grapheme cluster with consecutively added codepoints will not move the cursor except for variation selector 16 (VS16) that may have caused the width of the grapheme cluster to change to wide (2 grid cells)\"; section Feature and Mode State Detection: DECRQM of mode 2027 tells the mode's state."
  - standard: "kitty's text sizing protocol (https://sw.kovidgoyal.net/kitty/text-sizing-protocol/, read 2026-11-13): for the key w, \"When zero, the terminal should calculate the width as it would for normal text, splitting it up into scaled cells\"; its section The algorithm for splitting text into cells gives every code point whose category starts with M zero width, and a SpacingMark a width only for U+0E33 THAI CHARACTER SARA AM and U+0EB3 LAO VOWEL SIGN AM."
  - evidence: "Measured by this record's agent on 2026-11-13 with a copy of `crates/vtium-config/tests/indic_sizes.rs` that gives each cluster the cells D1 gives it, with tier 2's Noto fonts at 32 pixels of text in Noto Sans Mono's 14-pixel cell, through the helper's rasteriser. The squeeze, a cluster's advance over its cells, falls for Tamil கொ from 3.91 to 1.95 and கை from 3.28 to 1.64, for Malayalam ക്ഷ്മ from 4.00 to 2.00 and സ്ത്രീ from 4.46 to 2.23, and for Sinhala සිං from 2.14 to 1.07. Devanagari र्म goes from 0.99 to 0.49 and कि from 1.70 to 0.85. The lines of tier 2's frit-indic screen grow from 26 to 36 cells in Devanagari, 13 to 17 in Tamil, 15 to 17 in Kannada, 9 to 13 in Malayalam and 11 to 16 in Sinhala. The figure is the Tamil line before and after, at 1:1."
  - evidence: "vtium at 4645b726: vtium-term gives a cluster the width of its first codepoint, or two for an emoji-style base with U+FE0F (`crates/vtium-term/src/unicode.rs`, `width_of`); when U+FE0F arrives it widens the stored cluster in place (`crates/vtium-term/src/screen/print.rs`, `widen`); its generator gives General_Category Mn, Me, Cf and Cc no cell and every Mc one (`crates/vtium-term/gen-unicode/src/main.rs`), so with mode 2027 reset a spacing mark takes a cell of its own."
  - evidence: "Research relayed by the steering session, 2026-11-13: glibc 2.44's wcswidth sums its code points' widths, with an Mc taking 1, so it gives கொ 2 cells where vtium gives 1; for a long conjunct glibc gives 3 or 4 and Neovim 1; Ghostty and Windows Terminal follow Python wcwidth's specification for these clusters, and the ucs-detect suite uses it as its reference."
  - evidence: "Research relayed by the steering session, 2026-11-13, on how programs and terminals measure a cluster with a spacing mark or a virama conjunct. One cell per cluster: Neovim 0.12, foot 1.28, WezTerm, kitty 0.49, which offers its text sizing protocol for wider text instead, and mlterm 3.9.5 by default. ucs-detect's results for mlterm (https://ucs-detect.readthedocs.io/sw_results/mlterm.html) measure Tamil கா at 1 cell where wcwidth's rule gives 2; mlterm's option -J or --dyncomb, off by default, counts a base and its combining mark as two columns. Two cells by Python wcwidth's rule: glibc's wcswidth, and with it bash, readline and C programs, Vim 9.2, Python wcwidth, Ghostty and Windows Terminal. The sum of every code point's width, 3 or 4 cells for a long conjunct: glibc and Vim on conjuncts of several consonants, Konsole and Contour."
  - precedent: ADR-1000
  - precedent: ADR-1203
  - precedent: ADR-1307
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "can we adobe wcwidth's rule?"
    via: the maintainer, through the steering session's question tool, 2026-11-13, after research showed Python wcwidth's rule, Ghostty and Windows Terminal give these clusters two cells; the steering session took the answer as yes
  - operator: aaronsb
    said: "Yes, one rule for all"
    via: the maintainer, through the steering session's question tool, 2026-11-13, answering ADR-1006's Devanagari probe
    covers: [Devanagari]
  - operator: aaronsb
    said: "Per cluster, like glibc"
    via: the maintainer, through the steering session's question tool, 2026-11-13, answering ADR-1006's Kannada probe
    covers: [Kannada]
  - operator: aaronsb
    said: "could we offer a uniform behavior flag that is independent of the actual language, and instead offers packed (narrowed/scaled) or double (two cells) for any multi cell character?"
    via: the maintainer, in the steering session, 2026-11-13, proposing a uniform setting for these clusters
  - operator: aaronsb
    said: "right, from unicode not the font"
    via: the maintainer, in the steering session, 2026-11-13, confirming that the setting's trigger comes from Unicode's character properties and never from font metrics, because a program counts cells without knowing vtium's font
  - operator: aaronsb
    said: "is this really an issue? how many other terminals act this way?"
    via: the maintainer, in the steering session, 2026-11-13, asking about the record's sentence "A program that gives a cluster its first character's width disagrees with vtium on these clusters. Before, it agreed."; the steering session answered that the trade cuts both ways, with the programs and terminals the basis lists
  - operator: aaronsb
    said: "I think the default is double, but packed can be used for compatibility"
    via: the maintainer, in the steering session, 2026-11-13, answering whether cluster-width's default is double
status: proposed
date: 2026-11-13
deciders:
  - aaronsb
related:
  - ADR-1000
  - ADR-1001
  - ADR-1200
  - ADR-1203
  - ADR-1302
  - ADR-1307
---

# ADR-1006: A grapheme cluster with a spacing mark or a virama conjunct takes two cells, by Python wcwidth's rule

## Summary

- **Decided:** vtium-term measures each grapheme cluster by two rules of Python wcwidth's specification. A cluster in which a spacing mark follows its first character takes two cells, as கொ and कि do. A cluster that holds a virama conjunct takes the sum of its consonants' widths, at most two, as क्ष does. Every other cluster keeps the width it has today. The terminal still cuts text into clusters by UAX #29, so each cluster is measured on its own. A setting in vtium.conf, `cluster-width`, chooses between this rule, `double`, which is the default, and `packed`, the value for compatibility. Under `packed`, each cluster keeps the cells of its first character, and its glyph is squeezed to fit, as in programs and terminals that count one cell per cluster (D8).
- **Trades away:** room, and agreement with one group of programs or the other. Under `double`, Devanagari and the other scripts with spacing vowel signs take up to half as much room again, and a cluster narrower than its two cells leaves a gap. The cursor moves when a spacing mark arrives, which the mode 2027 specification allows only for VS16. Programs split on these clusters. Under `double`, vtium agrees with the shell and disagrees with Neovim, foot and WezTerm. Under `packed`, it is the other way round.
- **One-way?** No. The rule lives in vtium-term's width function and its tables, and no stored format depends on it.
- **Probes:** *Not confident (Devanagari):* Devanagari takes more room. The Hindi line of tier 2's font set screen grows from 26 to 36 cells. A cluster such as र्म fills only half its two cells, so the headline breaks around it. Is that acceptable? *Not confident (Kannada):* a Kannada, Tamil or Sinhala conjunct that the grapheme rules cut into several clusters takes the cells of each. ಸ್ತ್ರೀ takes 4 cells, as glibc counts it, where Python wcwidth's measure of the whole word, by its own rules, gives 2. Is that what you want? *Not confident (Setting):* the setting is named `cluster-width`, with the values `double` and `packed`, and sits beside `text-direction` under "Fonts and text". Is that name right? Another name could be `syllable-width`, which reads more plainly but fits less well outside the Brahmic scripts.
- **Inversion:** the choice sat between the first character's width, one cell whatever follows, and the sum of every character's width, as glibc counts. The rule gives two cells, the most a glyph holds, to the clusters whose shapes need room, and no more. The setting's value `packed` keeps the first end for compatibility, on a console whose programs count one cell per cluster.

> Edited 2026-11-13: the maintainer asked for "a uniform behavior flag that is independent of the actual language" with the values packed and double, and asked of the sentence on programs that disagree "is this really an issue? how many other terminals act this way?" (`considered`). The Decided item had no setting. The Trades away item read: "room and agreement with some programs. Devanagari and the other scripts with spacing vowel signs take up to half as much room again, and a cluster narrower than its two cells leaves a gap. A program that gives a cluster its first character's width disagrees with vtium on these clusters. The cursor moves when a spacing mark arrives, which the mode 2027 specification allows only for VS16." The Setting probe and the inversion's last sentence are new. The maintainer later confirmed the default, "I think the default is double, but packed can be used for compatibility", so the Setting probe asks only about the name.

## Context

vtium-term gives a grapheme cluster the cells of its first character (ADR-1200 D21). A Tamil syllable such as கொ, ka with the vowel sign o, takes one cell. Its glyph is about four cells wide, so ADR-1307 D1 squeezes it to a quarter of its width. The measurement in the basis found squeezes of 2 to 4.5 times in Tamil, Malayalam and Sinhala. ADR-1307 named a later record that could give such clusters two cells.

Programs disagree with that one cell. glibc's wcswidth gives கொ two cells, because it counts the vowel sign as a character of its own. Python wcwidth's specification gives it two cells by rule. Ghostty and Windows Terminal follow that specification, and the ucs-detect suite uses it as its reference.

The specification has two rules for these clusters. A spacing mark (General_Category Mc), such as most vowel signs that stand beside their consonant, widens the cluster to two cells. A virama conjunct, consonants joined by a virama, takes the sum of its consonants' widths, at most two. The maintainer asked whether vtium could adopt that rule.

## Decision

### D1: the width of a cluster

With mode 2027 set, which is vtium's default, vtium-term gives a cluster the width that ADR-1200 D21 states: its first character's width, or two for an emoji-style base with U+FE0F. Two rules from Python wcwidth's specification, release 0.9.4, then widen it:

- **A spacing mark.** When a character of General_Category Mc follows a first character of nonzero width, the cluster takes two cells. Nonspacing marks between them do not matter. The specification calls this rule part of "Width of 2".
- **A virama conjunct.** A character of Indic_Syllabic_Category Consonant that follows a character of Indic_Syllabic_Category Virama or Invisible_Stacker adds its width to the cluster's. A U+200D ZERO WIDTH JOINER or nonspacing marks may stand between them. The specification calls this rule "Virama Conjunct Formation".
- **The six spacing viramas.** Six viramas have General_Category Mc: Balinese U+1B44, Javanese U+A9C0, Sharada U+111C0, Khojki U+11235, Grantha U+1134D and Takri U+116B6. Each counts as a virama, not as a spacing mark, so it does not widen its cluster.
- **At most two.** No cluster takes more than two cells. The glyph format holds glyphs of one or two cells (ADR-1200 D21).

The rules hold for every script. In practice they reach the Brahmic scripts of South and Southeast Asia, such as Devanagari, Tamil, Myanmar and Khmer. vtium takes only these two rules from the specification. Every other width stays ADR-1200 D21's, even where the specification differs.

The tables come from Unicode's DerivedGeneralCategory.txt and IndicSyllabicCategory.txt, at the Unicode version vtium-term's other tables use. vtium-term's generator writes them.

### D2: each cluster is measured on its own

vtium-term still cuts text into clusters by UAX #29, and D1 measures each cluster on its own. That is how the specification's Scope section says a grapheme is measured: each grapheme from its `iter_graphemes`, measured by `wcswidth`.

Where UAX #29 keeps a conjunct in one cluster, the conjunct rule applies inside it. GB9c does this for the scripts whose virama has Indic_Conjunct_Break=Linker, such as Devanagari, Malayalam, Myanmar and Khmer. क्ष takes two cells, and स्त्री, one cluster, takes two.

Where UAX #29 cuts a conjunct into several clusters, each cluster keeps its own width. It does this in Kannada, Tamil, Sinhala, Gurmukhi and the other scripts whose virama has no Indic_Conjunct_Break=Linker. ಸ್ತ್ರೀ is ಸ್, ತ್ and ರೀ, which take 1, 1 and 2 cells. A syllable that ADR-1203 draws joined takes the sum of its clusters' cells.

Here vtium departs from the specification's measure of a whole string, which caps such a conjunct at two cells. The specification itself says that it measures Tamil, Kannada and Sinhala differently than it segments them. vtium cannot give a run of clusters fewer cells than it has clusters. The sum per cluster also agrees with glibc's wcswidth for such a conjunct, which counts ಸ್ತ್ರೀ as 4.

### D3: the cursor when a cluster widens

A cluster takes two cells as soon as the character that widens it arrives, a spacing mark or the consonant after a virama. vtium-term widens the stored cluster as it widens one for U+FE0F today:

- the cluster takes the cell after it, and the cursor moves one cell further;
- in the last column, with autowrap set, the cluster moves to the start of the next line, and the cell it left is cleared;
- without autowrap, or on a screen one column wide, the cluster stays one cell wide;
- in insert mode, the cells after it move right by one.

Erasing, wrapping, selecting, reflowing and mouse reports treat the cluster as any cluster of two cells. Cursor movement counts cells, as it does for any wide character.

This departs from the terminal-unicode-core specification of mode 2027. That specification says that adding a codepoint to a cluster moves the cursor only for VS16. The departure holds for the two rules of D1, with mode 2027 set. Its reason is that these clusters' shapes need two cells, and programs that follow Python wcwidth or glibc put the cursor two cells on. The departure ends when the Unicode Consortium's Text Terminal Working Group publishes a width rule for terminals. A later record then decides whether vtium follows that rule.

### D4: mode 2027 and DECRQM

DECRQM of mode 2027 answers as it does today: set by default, and reset after a program resets the mode (ADR-1000, row DEC-2027). With mode 2027 reset, vtium-term gives each codepoint its own width, and this record changes nothing there. A spacing mark already takes a cell of its own in that mode, as glibc counts it. The row's note names this record when it is built.

### D5: kitty's text sizing

kitty's text sizing protocol, OSC 66, is planned and not built (ADR-1000, row text.kitty-sizing). The protocol says that a width key `w` of 0 asks the terminal to measure the text as it measures normal text. vtium measures that text by D1, so a cluster with a spacing mark takes two scaled cells. The protocol's own section on splitting text into cells gives every spacing mark zero width, except Thai and Lao SARA AM. vtium follows its normal-text rule instead, and the design note that the row's issue writes first says so. With `w` set, the program's width stands.

### D6: the drawing

The glyph helper places each glyph in the cells the terminal gives its cluster (ADR-1200 D21), so it places a widened cluster in two cells. A cluster wider than its two cells is squeezed to both (ADR-1307 D1). A cluster narrower than its two cells has its advance centred across them (ADR-1302 D2).

The figure shows the Tamil line of tier 2's frit-indic screen at 1:1, in 14×45 cells. The upper row is each cluster in one cell, as vtium draws it today. The lower row is the cells this record gives: மி, கொ, கை and கூ take two cells each.

![Two rows of white Tamil text on black. Upper row: the word Tamil, a colon, the conjunct ksha, and the syllables ko, kai and koo, each squeezed into one cell, the last three small and cramped. Lower row: the same text wider, with mi, ko, kai and koo each in two cells and drawn at a readable width.](figures/ADR-1006-tamil-two-cells.png)

### D7: the records this amends

- **ADR-1200 D21:** a cluster takes the cells of its first character, widened by this record's D1. The rest of the rule for a glyph's cells stands.
- **ADR-1307 D1:** a cluster takes the cells this record gives it. ADR-1307 declined to give a cluster more cells. This record takes that alternative in part: two cells at most, by Python wcwidth's rules, not one cell for each consonant and vowel sign.
- **ADR-1203 D2:** the terminal keeps UAX #29's clusters, and each takes the width this record gives it.

### D8: a setting chooses double or packed

> Added 2026-11-13: the maintainer asked for "a uniform behavior flag that is independent of the actual language, and instead offers packed (narrowed/scaled) or double (two cells) for any multi cell character", and confirmed that its trigger comes from Unicode, "not the font". The maintainer then set the default: "I think the default is double, but packed can be used for compatibility" (`considered`).

A key of vtium.conf's top level, `cluster-width`, chooses what vtium-term does with the clusters that D1's rules name. It has two values:

- **`double`**, the default. D1 to D3 apply as written. A cluster with a spacing mark after its first character, or with a virama conjunct, takes two cells, and its glyph is drawn at full size (D6).
- **`packed`**, the value for compatibility. It suits a console whose programs count one cell per cluster, such as Neovim, foot, WezTerm, kitty and mlterm with its default options. D1's two rules do not apply. Every cluster keeps the cells that Unicode's grapheme and East_Asian_Width rules give it, as ADR-1200 D21 states: its first character's width, or two for an emoji-style base with U+FE0F. The glyph helper squeezes a glyph wider than those cells to fit them, as ADR-1307 D1 decides. A cluster never widens after it is printed, so D3's departure from the mode 2027 specification does not arise.

The setting applies to every script alike. It names no language and no script. It chooses only between two widths for the clusters that D1's rules name, and every other cluster takes the same width under both values.

What makes a cluster one of these is a Unicode property of its characters, never a measure of the font. The properties are General_Category Mc after a first character of nonzero width, and Indic_Syllabic_Category Virama or Invisible_Stacker before a Consonant. A program counts cells without knowing which font vtium draws with, so a width taken from the font would put the program's cursor and vtium's in different places. Tier 1's screen state is also computed without a font, and a width from the font would make it depend on one.

vtium reads the key when it starts, so a change takes effect when vtium restarts, as with `text-direction`. A reload of vtium.conf, once reload is built, leaves the running value as it is. Changing the value while vtium runs would change the width of text that is already on the screen, and the cursor positions of programs that are already running.

The key belongs in the key table's section "Fonts and text", in a group of its own beside "Right-to-left text". vtium-term reads it. It is set only at the top level, as `text-direction` is, so that every console measures text the same way. Its description in the key table says what each value does, with கொ as the example, and that a change takes effect when vtium restarts.

The setting applies with mode 2027 set. With mode 2027 reset, each codepoint takes its own width under both values, as D4 states. DECRQM of mode 2027 answers set or reset, as D4 states, under both values. No standard query tells a program which width rule a terminal uses, and vtium adds none. A program that needs to know prints a cluster such as கொ and asks for the cursor's position with DSR 6, as the ucs-detect suite does. Under `double`, the cursor is two cells on, and under `packed`, one.

Under `packed`, kitty's text sizing protocol with a width key `w` of 0 gives a cluster with a spacing mark one scaled cell, not the two that D5 states.

## What the build moves

The build changes this code. It changes no golden itself: each golden that moves changes in a commit of its own, which the maintainer reviews.

- `crates/vtium-term/gen-unicode/src/main.rs` and the table it writes, `crates/vtium-term/src/unicode/data.rs`: the property word gains General_Category Mc and the Indic_Syllabic_Category values Virama, Invisible_Stacker and Consonant.
- `crates/vtium-term/src/unicode.rs`: `width_of` and `cluster_width` follow D1, and their tests, `a_clusters_width_is_the_one_the_screen_stores` and `widths_of_named_codepoints`, gain the new cases.
- `crates/vtium-term/src/screen/print.rs`: `append_to_cluster`, `stored_width` and `widen` widen any cluster whose width grows, not only one with U+FE0F.
- `crates/vtium-term/README.md`: the paragraph on how vtium-term cuts text into cells.
- `crates/vtium-config/tests/shaping.rs`: the test of the key क्ष, which takes two cells.
- `crates/vtium-config/tests/indic_sizes.rs`: `line_cells`, which gives every cluster one cell.
- ADR-1000's row DEC-2027: its note names this record.

Tier 1's corpus and goldens hold no text of these scripts, so none of them moves. These pictures move:

- tier 2's `scripts-indic` golden, the Hindi and Bengali lines that the maintainer accepted under ADR-1307;
- tier 2's `frit-indic` screen, which has no golden yet and draws its first one under this record;
- the specimens' Devanagari line, नमस्ते दुनिया, at every size in `docs/specimens/`. Their Tamil line, வணக்கம் உலகம், holds no spacing mark and does not move;
- the showcase's `scripts.png` and its hero, `hero.png`, which hold Hindi, Bengali and नमस्ते;
- the VT review's Hindi section, `tools/vt/sections/88-hindi.sh`, and its text `udhr-article1.tsv`.

ADR-1307's figure and ADR-1203's figure stay as they were published, at the widths of their day.

## Verification

- `crates/vtium-term/src/unicode.rs`: கொ, कि, क्ष and स्त्री take two cells. कु, க், ಸ್ and a lone spacing mark take what they take today. Javanese ka with its pangkon U+A9C0 takes one, and no cluster takes three.
- `crates/vtium-term/tests/screen.rs`: printing க and then ொ leaves the cursor two cells on. In the last column with autowrap, கொ moves to the next line and leaves its cell cleared. Without autowrap it stays one cell. Insert mode moves the cells after it.
- `crates/vtium-term/tests/modes.rs`: with mode 2027 reset, கொ takes two cells, one for each codepoint, as before.
- Tier 2: `scripts-indic` and `frit-indic` drawn at the new widths, reviewed by the maintainer at 1:1.

## Consequences

### Positive

- Tamil, Malayalam and Sinhala syllables with vowel signs are squeezed about half as hard, and read at the size of the text around them.
- vtium agrees with glibc and with Python wcwidth on a consonant with a spacing vowel sign, so a shell's line editor puts the cursor where vtium draws it.

### Negative

- Devanagari, Bengali and the other headline scripts take more room. A cluster narrower than its two cells is centred with a gap on each side, so the headline breaks more often.
- Programs and terminals split three ways on these clusters, and either value of `cluster-width` disagrees with some of them. One group gives each cluster one cell: Neovim 0.12, foot 1.28, WezTerm, kitty 0.49 and, by default, mlterm 3.9.5. A second group gives two cells by Python wcwidth's rule: glibc's wcswidth, which bash, readline and C programs use, Vim 9.2, Python wcwidth, Ghostty and Windows Terminal. A third group sums every code point's width, three or four cells for a long conjunct: glibc and Vim on conjuncts of several consonants, Konsole and Contour. Under `double`, vtium agrees with the second group, the shell among it, and disagrees with the first. Under `packed`, it agrees with the first group and disagrees with the shell, as vtium did before this record. The basis gives the sources.

> Edited 2026-11-13: the maintainer asked of this item "is this really an issue? how many other terminals act this way?" (`considered`). It read: "A program that gives a cluster its first character's width disagrees with vtium on these clusters. Before, it agreed."

- A long conjunct in one cluster takes two cells, where glibc counts three or four.
- A single wide letter keeps one cell. Tamil ஷ stays squeezed about 2.2 times.

### Neutral

- Mode 2027 reset and its DECRQM answer do not change.
- The screens and pictures listed under "What the build moves" are drawn again.

## Alternatives Considered

- **Keep one cell for every cluster,** as ADR-1200 D21 states today. Not taken by the maintainer: Tamil, Malayalam and Sinhala syllables are squeezed past reading, and the shell's own count disagrees.
- **Make one cell the default.** Not taken: the maintainer chose Python wcwidth's rule for these clusters, and said that `double` is the default and that `packed` can be used for compatibility (D8).
- **Take the width from the font,** two cells when the glyph needs them. Not taken: a program cannot know vtium's font, and the maintainer confirmed that the trigger comes "from unicode not the font" (D8).
- **Choose the width per language or per script.** Not taken: the maintainer asked for one setting that names no language (D8).
- **Give each cluster the sum of its characters' widths,** as glibc's wcswidth counts. Not taken: a long conjunct would take three or four cells, which the glyph format cannot hold, and the squeeze would turn into wide gaps.
- **Give a cluster two cells when its glyph is wider than one and a half cells,** as the maintainer saw on the Indic choices page. Not taken: the width would depend on the font, so a program could not know it.
- **Measure a conjunct cut into several clusters as one, at most two cells,** as Python wcwidth's `wcswidth` measures a whole string. Not taken: each cluster needs at least one cell, so a conjunct of three clusters cannot take two.
