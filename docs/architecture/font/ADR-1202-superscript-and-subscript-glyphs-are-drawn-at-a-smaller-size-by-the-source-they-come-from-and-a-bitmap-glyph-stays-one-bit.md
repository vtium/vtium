---
contract: adr/v1
kind: decision
verb: change
capability: glyphs
amends:
  - "ADR-1200#D1: the frit"
  - "ADR-1200#D2: the glyph chunk format, on the helper's pipe"
  - "ADR-1200#D20: keys"
  - "ADR-1200#D21: cluster asks and style asks"
basis:
  - operator: aaronsb
    level: directed
    said: "Crisp for bitmap fonts"
    via: "the maintainer's answer on reviewing tier 2's provisional `script-unifont` picture (tools/tier2/goldens/script-unifont.ppm), relayed word for word in this record's brief, 2026-11-11"
  - operator: aaronsb
    level: guided
    said: "if bitmap fonts have different sizes in them, maybe we can just offset a smaller size bitmap from the same font for super and subscript?"
    via: "the maintainer in the steering session, 2026-11-11, relayed word for word in this record's brief"
  - operator: aaronsb
    level: guided
    said: "we need some sort of coherent engine for super and subscript, for managing bitmap and outlined fonts"
    via: "the maintainer in the steering session, 2026-11-11, relayed word for word in this record's brief"
  - operator: aaronsb
    level: guided
    said: "the same could be said for bitmap fonts where we have an integer multiplier for fonts - say a highdpi display (such as mine). Even a 2x multiplier on bitmap fonts would afford super and subscript for a font that has only one size. For instance, the ATASCII font is bitmapped, but if we doubled (or other full integer multiplier) the size of the font on the native display screen, we can offer placement when we have more pixels to use"
    via: "the maintainer in the steering session, 2026-11-11, relayed word for word by the team lead after the record's first draft"
  - evidence: "How vtium picks a bitmap font's whole multiple, at integration 44792f56: with no `font-size`, the automatic rule's height is the smaller of twice the display's width over 120 and its height over 40, and never under 16 (crates/vtium/src/font.rs:128-146, `auto_size`); a bitmap first source with no `cell=` takes the largest whole multiple of its own cell whose height is not over that height (ADR-1200 D17); the built-in fonts take the largest whole factor whose cell fits (crates/vtium/src/font.rs:413-418). So an 8×16 font draws at 1× on a 1920×1080 display (height 27), 2× on 2560×1600 (40) and 3× on 3840×2160 (54), and an 8×8 font at 3×, 5× and 6×."
  - evidence: "Measured by this record's agent on 2026-11-11 from vtium-font's data/unifont.hex at integration 44792f56, over the 94 printable ASCII characters and é, É, Å, ², α and β, each 8×16 glyph scaled to 5×11 as `scripted` sizes it (crates/vtium-font/src/script.rs:47, 110). A glyph's shape counts as kept when its number of separate strokes (8-connected ink) and of enclosed holes (4-connected background not touching the edge) are unchanged. Each small pixel the average of what it covers, as vtium draws it today, then cut at half, as a 1-bit display cuts it (crates/vtium-render/src/mono.rs:18-21): 58 of 100 changed. Each small pixel the source pixel under its centre: 60 of 100 changed, because columns 1 and 6, where unifont draws its vertical strokes, fall between centres and vanish. Each source pixel sent to the small pixel that holds its centre, a small pixel ink when any pixel sent to it is ink: 9 of 100 changed (!, %, &, ?, @, a, i, j and Å, where a gap one pixel high closes). The script and the figure's source are in the agent's scratchpad, uncommitted."
  - evidence: "Read by this record's agent on 2026-11-11 with fontTools 4.68.0 from the fonts installed on the maintainer's machine. OS/2 superscript size (x by y), superscript offset, subscript size and subscript offset, in em: Noto Sans Mono, Noto Sans, Noto Sans CJK, Fira Mono, Fira Code, Hack and JetBrains Mono all carry 0.65×0.6, 0.35, 0.6 and 0.075; DejaVu Sans Mono 0.65×0.7, 0.48, 0.7 and 0.14; Liberation Mono 0.7×0.65, 0.422, 0.65 and 0.139. DejaVu Sans Mono's ascent is 0.928 em and its descent 0.236 em."
  - evidence: "Read by this record's agent on 2026-11-11 with fontTools 4.68.0, the characters each font's `sups` and `subs` lookups map: JetBrains Mono 11 (+ and the digits) and 10 (the digits); Fira Mono 15 and 14 (the digits, parentheses, plus, minus, and = for `sups`); Hack 10 and 10 (the digits); Noto Sans Mono 16 (the digits, parentheses, +, =, − and n) and 23 (those and h, k, l, m, p, s and t); Liberation Mono 6 (4 to 9) and 10 (the digits). DejaVu Sans Mono and Noto Sans CJK have neither feature."
  - evidence: "Terminus as installed on the maintainer's machine: the kbd console fonts ter-112n to ter-132n and the bitmap OpenType files ter-u12 to ter-u32 come at heights 12, 14, 16, 18, 20, 22, 24, 28 and 32, one height to a file."
  - evidence: "vtium at integration 44792f56: the console scales the normal glyph it drew (crates/vtium-font/src/face.rs:353-360, `Font::drawn_variant`), so the helper never sees the script (crates/vtium/src/helper.rs:1003-1006, an ask carries a style alone); a 1-bit display gives a superscript or subscript mask no stand-in and cuts its gray at half (crates/vtium-render/src/mono.rs:222); the renderer's cache already keys the script (crates/vtium-render/src/cache.rs:298-305); a set's strike record has a reserved 16-bit field after its format (crates/vtium-font/src/set/format.rs:12); the helper scales a color glyph once, from its source's largest strike, by the color method the operator chose (crates/vtium-config/src/outline.rs:1175-1215, crates/vtium-config/src/resample.rs:1-14); a CPI font holds heights 8, 14 and 16, of which `cp-size=` takes one (crates/vtium-config/src/frit.rs:28, 375-390)."
  - standard: "mintty's documentation of SGR 73, 74 and 75, read on 2026-11-11 from its wiki at mintty ab79b31: the table of text attributes under Tips, Text attributes and rendering (https://github.com/mintty/mintty/wiki/Tips#text-attributes-and-rendering), lists 73 superscript and 74 subscript, each ended by 75; the Changelog's 3.0.1 (28 May 2019) adds \"SGR 73 and SGR 74 for superscript and subscript display (75 clears)\" (https://github.com/mintty/mintty/wiki/Changelog). The CtrlSeqs page does not mention them. Neither page sets a size or a place for the glyph."
  - operator: aaronsb
    level: directed
    said: "we need to be careful not to start mixing up fonts too much against builtins. Someone could have their own font arrangement (vtfrit) that defines what they want and if we stomp over that careful arrangement with unifont or unscii, that would be bad"
    via: question tool, 2026-11-11
  - evidence: "CSS Fonts: the CSS Working Group's resolution of October 2015 dropped the requirement that font-variant-position use a font's OS/2 superscript and subscript metrics, noting that the metrics are often wrong or vary widely between fonts (https://lists.w3.org/Archives/Public/www-style/2015Oct/0224.html). Mozilla's work on font-variant-position found the same, with Gecko using some OS/2 values and WebKit fixed ratios (https://bugzilla.mozilla.org/show_bug.cgi?id=1024804). Both found by the steering session's research of 2026-11-11 and relayed in this record's brief."
  - evidence: "The steering session's research of 2026-11-11, relayed in this record's brief: few terminals draw SGR 73 and 74 at all, so terminals have no common practice for their size or place to follow."
  - standard: "OpenType 1.9.1, the OS/2 table (ySuperscriptXSize, ySuperscriptYSize, ySuperscriptYOffset and the subscript equivalents), https://learn.microsoft.com/en-us/typography/opentype/spec/os2, and the feature tags `sups` and `subs`, https://learn.microsoft.com/en-us/typography/opentype/spec/features_pt"
  - precedent: ADR-1200
  - precedent: ADR-1300
  - precedent: ADR-1302
  - precedent: ADR-1306
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Half to 3/4 (Recommended)"
    via: question tool, 2026-11-11
    covers: [size range]
  - operator: aaronsb
    said: "I lean towards 'no, one rule' but first, how common is it to use an OS/2 table? what do other terminal emulators do?"
    via: question tool, 2026-11-11
  - operator: aaronsb
    said: "No, one rule (Recommended)"
    via: question tool, 2026-11-11
  - operator: aaronsb
    said: "Explicit key (Recommended)"
    via: question tool, 2026-11-11
  - operator: aaronsb
    said: "we need to be careful not to start mixing up fonts too much against builtins. Someone could have their own font arrangement (vtfrit) that defines what they want and if we stomp over that careful arrangement with unifont or unscii, that would be bad"
    via: question tool, 2026-11-11
  - operator: aaronsb
    said: "I think there's just going to be specific fonts and sizes and display dpi for bitmap fonts that work better than others and it's ok to not have perfection"
    via: steering session, 2026-11-11, after accepting script-unifont
  - operator: aaronsb
    said: "Having no aliasing and crisp text is the better half of the tradeoff"
    via: steering session, 2026-11-11, after accepting script-unifont
  - operator: aaronsb
    said: "the underline is not tracking the text position of an under item, and struck line tracking is not struck out, the line appears above it. (the underline and strike lines are likely following the font's conventional non-super or sub positioning. let's make sure we have a sane decision tree for the engine to plan where to draw the lines for under or struck. for reversed items, I think the text block is ok, because it fills the entire character box."
    via: question tool, 2026-11-11, reviewing slice 3's tier 2 pictures
  - operator: aaronsb
    said: "Follows the glyph (Recommended)"
    via: "question tool, 2026-11-11, D8's open choices: line thickness, double and curly under a subscript, overline, underline cursor"
  - operator: aaronsb
    said: "Move up, like normal cells (Recommended)"
    via: "question tool, 2026-11-11, D8's open choices: line thickness, double and curly under a subscript, overline, underline cursor"
  - operator: aaronsb
    said: "On the small glyph (Recommended)"
    via: "question tool, 2026-11-11, D8's open choices: line thickness, double and curly under a subscript, overline, underline cursor"
  - operator: aaronsb
    said: "Same rows as normal (Recommended)"
    via: "question tool, 2026-11-11, D8's open choices: line thickness, double and curly under a subscript, overline, underline cursor"
observable:
  - "Tier 2's `script-unifont` screen shows every superscript and subscript pixel fully on or fully off, each small glyph two thirds of its cell's height, with the shapes the figure's last row shows"
  - "The same screen in 1-bit mode shows the same small glyphs, where today it shows broken strokes"
  - "A frit of Terminus at 16 pixels with `script-source=kbd:ter-112n` draws its superscripts and subscripts with Terminus's own 12-pixel glyphs"
  - "unifont in a 16×32 cell, drawn at twice its size, draws its superscripts and subscripts as unifont at its own 8×16, every pixel of the font one pixel of the screen"
  - "In DejaVu Sans Mono, a superscript letter's edges are as sharp as a normal letter's at the same size"
  - Tier 2's `script-unifont` screen shows a superscript's underline under the raised glyph, and a subscript's strike-through across the lowered glyph
status: accepted
date: 2026-11-11
deciders:
  - aaronsb
related:
  - ADR-1200
  - ADR-1300
  - ADR-1302
  - ADR-1306
  - ADR-1201
---

# ADR-1202: Superscript and subscript glyphs are drawn at a smaller size by the source they come from, and a bitmap glyph stays one bit

## Summary

- **Decided:** vtium draws a superscript or subscript glyph (SGR 73 or 74) as a smaller version of the character's glyph, made by the source the glyph comes from, in the way that suits that kind of source. Every kind shares one rule for size and place: two thirds of the normal glyph's size, centred across the cells, against the top of the row's text for a superscript and against its bottom for a subscript. An outline font's glyph is rasterized at two thirds of its size. A bitmap font's glyph comes from a smaller size of the same font where the font has one. A font that vtium draws at a whole multiple of its own size, as on a high-DPI display, always has one: the same bitmap at a smaller multiple, each of its pixels a whole block. A smaller size can also be another height of a CPI file, or a smaller font that the frit names with a new key, `script-source=`. Only a bitmap font drawn at its own size with none of these falls through to a glyph scaled to two thirds, each small pixel fully on or fully off, so it stays crisp. A color glyph is scaled smoothly, once, from its source. vtium's own drawn symbols are drawn at the box's size. The helper draws the glyphs of a frit's sources, asked for by script as it is asked for by style. The console draws its built-in fonts' and its symbols' small glyphs. Both take the size, the place and the on-or-off scale from one module of vtium-font. A script glyph always comes from the source that draws the character's normal glyph, and vtium never borrows another font for it, built-in or not.
- **Trades away:** The single rule of today, which scales whatever glyph was drawn. A superscript in a frit's font now waits for the helper, as a bold glyph does, and draws in the built-in font for that moment. A designed smaller size can be up to three quarters of the normal height, so its superscripts are larger than another font's. In 9 of 100 unifont glyphs a gap one pixel high closes. Script glyphs keep no ink above or below their row. OpenType's designed superscript glyphs and the fonts' own superscript metrics go unused.
- **One-way?** No. The rules live in vtium-font's script module and in the helper, and the wire format gains the script on both ends in one release, as it gained the style.
- **Probes:**
  - *Confident (crisp bitmaps):* a superscript or subscript in unifont, or in any bitmap font with no smaller size, has only fully lit or fully dark pixels, two thirds of the normal size, as the last row of the figure shows, on a color display and on a 1-bit one alike. Right?
  - *Not confident (size range):* a smaller size of the same font is used whenever it is between half and three quarters of the normal height, rather than a scaled glyph at two thirds. So a bitmap font drawn at twice its size on a high-DPI display takes its superscripts from itself at its own size, which is half, and Terminus at 16 pixels takes Terminus at 12, which is three quarters. Are both ends acceptable, to keep the font's own pixels?
- **Inversion:** The decision sits between "scale every drawn glyph the same way", which vtium does today with one rule in the console, and "let each font draw its small glyphs however it likes", which would give each format its best glyph but no common size or place. vtium takes one box and one placement for every kind of source, and lets only the drawing inside the box depend on the kind.

## Context

SGR 73 starts superscript, SGR 74 starts subscript, and SGR 75 returns to the baseline. The codes come from mintty, whose documentation lists them among its text attributes and leaves the glyph's size and place to the terminal (`basis`). Few terminals draw them at all. The sequences are used for mathematical and chemical text, as in x² or H₂O written with ordinary digits.

vtium draws them today with one rule (crates/vtium-font/src/script.rs). It draws the cell's normal glyph, then scales it to two thirds of its width and height. Each pixel of the small glyph is the average of the normal pixels it covers. A superscript sits against the top of the cell, and a subscript against the bottom. The console does this to whatever glyph it already holds (`Font::drawn_variant`), so the glyph helper (vtium-config, the sandboxed program that turns the frit's fonts into glyphs for the console) never hears of the script.

A frit (a font set file) names its fonts as sources, each a line of its own (ADR-1200 D1). A source is an outline font, or a bitmap font: a kbd console font, a PSF, CPI or raw file, a BDF file, the kernel's font, or a built-in font. Each bitmap format vtium reads holds one height, except CPI, which holds heights 8, 14 and 16, of which `cp-size=` takes one. A bitmap font is drawn at whole factors of its own size (ADR-1200 D17), so a source drawn at twice its size also has its own size available, exactly. Families such as Terminus ship each height as a file of its own, from 12 to 32 pixels (`basis`). Color glyphs come from outline files' color tables, and the helper scales each once from its source to its cells.

vtium draws a bitmap font at a whole multiple of its size when the cell is larger than the font. With no `font-size` set, the cell's height comes from the display: the smaller of twice its width over 120 and its height over 40, never under 16 pixels. A bitmap font takes the largest whole multiple of its own cell that fits that height (ADR-1200 D17, `basis`). An 8×16 font therefore draws at 1× on a 1920×1080 display, 2× on a 2560×1600 one and 3× on a 3840×2160 one. An 8×8 font, such as the ATASCII font the maintainer named, draws at 3×, 5× and 6× on the same displays. vtium ships no ATASCII font set today, so the record uses unifont and Terminus as its examples.

The averaging rule turns a bitmap font's 1-bit glyph gray and soft. The maintainer reviewed the result in unifont and answered "Crisp for bitmap fonts". On a 1-bit display the gray is cut at half, and the strokes break (the figure's third row). The maintainer also suggested using a smaller size of the same bitmap font where one exists, and then the same bitmap at a smaller whole multiple where vtium draws it at 2× or more, and asked for "some sort of coherent engine for super and subscript, for managing bitmap and outlined fonts" (`basis`).

An outline glyph scaled from its full-size raster is softer than the same glyph rasterized at the small size, because the rasterizer and the hinter place its edges on the small size's pixel grid only when they draw at that size.

![Six unifont characters, e, a, B, 8, 2 and x, in four rows of 8×16 cells, each pixel drawn large. The first row shows the normal glyphs. The second row shows today's superscripts, two thirds of the size at the top of the cell, in grays where the averaging blends stroke and background. The third row shows those superscripts on a 1-bit display, the gray cut at half: most strokes are gone, and only a few dashes and dots remain. The fourth row shows this record's superscripts, two thirds of the size, every pixel fully on or off, each character's strokes whole and readable.](figures/ADR-1202-bitmap-script.svg)

## Decision

The words used below. A **script glyph** is the glyph of a superscript or subscript cell. A **strike** is a font's glyphs at one size. The **text band** is the rows of a cell that the text takes: the whole cell, less the rows that line spacing adds above and below (ADR-1302 D13). A **sprite** is a symbol vtium draws itself to fill its cell, such as box drawing, block elements, shades and Braille.

D15 follows D1 because it bounds every decision after it.

### D1: one box for every kind of source

- A glyph's **own box** is the rows from its font's ascent to its descent at the size it is drawn: the whole height of a bitmap strike, at the factor it is drawn at, and the rows an outline font's ascent and descent span. A script glyph is two thirds of the size of the normal glyph, rounded to whole pixels, unless D3 finds a smaller size of the font. Each kind of source makes it in its own way (D2 to D6).
- The engine places a script glyph so that its own box lies against an edge of the text band: its top on the band's top for a superscript, and its bottom on the band's bottom for a subscript. Across, the glyph is centred in its cells, as every glyph's advance is centred (ADR-1302 D2). Every source on a line therefore puts its superscripts against one edge and its subscripts against the other.
- A sprite has no font, so its box is two thirds of the text band's height and two thirds of its cells' width, placed against the same edges (D6).
- **Two thirds** is the size vtium draws today, and the size the maintainer reviewed, asking only for crisp edges. In a 16-row cell it keeps 11 rows, in which the letters keep about 7 (script.rs:16-18). The fonts' own superscript metrics name 0.6 to 0.7 of the em, a little under two thirds of a cell that the font's ascent and descent fill.
- **Against the edge** keeps the script glyph inside its row and as far from the baseline as the row allows. The fonts' own offsets would pass the row in DejaVu Sans Mono, the font vtium's tests draw: its superscript, 0.7 em raised 0.48 em, reaches 0.48 + 0.7 × 0.928 = 1.13 em above the baseline, 0.2 em above the cell's top (`basis`). Its subscript passes the cell's bottom by 0.07 em the same way.
- vtium does not read the OS/2 table's superscript and subscript sizes or offsets. Seven of the nine fonts measured carry the same values, 0.65×0.6 em raised 0.35 em, which font editors fill in, so they describe no design of those fonts. Reading them would give each source on a line a different script size and place.

### D15: a script glyph comes only from its own source

The maintainer: "we need to be careful not to start mixing up fonts too much against builtins. Someone could have their own font arrangement (vtfrit) that defines what they want and if we stomp over that careful arrangement with unifont or unscii, that would be bad".

- **A script glyph comes from the same source as the cell's normal glyph.** The source may give it in four ways only: itself at a smaller whole multiple, its own other heights, the font the frit names for it with `script-source=` (D3), or its own normal glyph made small (D2, D4, D5). vtium never draws a script glyph from a font the frit did not name for that source, built-in or not. unifont at its own size scales its own glyphs by D4 and never borrows Unscii-8's, and a frit's bitmap source never borrows unifont's.
- **Whatever draws the normal glyph draws the script glyph.** Where the frit lacks a character, the built-in font already draws its normal glyph (ADR-1200 D7), so the built-in font draws its script glyph too. That is the frit's own fallback order, not a substitution.

Each decision keeps to this rule:

- D1 sets only a size and a place, and draws no glyph.
- D2 rasterizes the outline source's own glyph; each later outline source draws the script glyphs of the characters it draws.
- D3 takes the source's own multiples, a CPI source's own heights, and only the `script-source=` fonts the frit names (D12).
- D4 scales the source's own glyph, unifont's included.
- D5 scales the color source's own strike.
- D6 draws a sprite where the frit's sprite policy (`glyph-source=`, ADR-1200 D1) draws the normal character as a sprite, and the font's glyph where the policy gives the character to the font.
- D7 draws a style from the frit's own face for that style where it names one, and otherwise from the regular source, as the normal glyph is drawn (ADR-1200 D21).
- D10's stand-in for a color glyph on a 1-bit display is the stand-in the normal glyph already takes there (crates/vtium-render/src/mono.rs), so the script glyph follows its normal glyph.
- D11's built-in placeholder while the helper draws is the placeholder every glyph already has, and this record leaves it unchanged; the frit's own glyph replaces it.
- D13 leaves the font's own designed glyphs unused; were they used, they would come from the same font.

### D2: an outline source

- The helper rasterizes the glyph at two thirds of the pixels to the em it draws the normal glyph at, with the frit's hinting and the display's antialiasing, as it draws any outline glyph (ADR-1200 D1, ADR-1301 D4). It never scales a full-size raster.
- Every outline source of the frit draws its script glyphs at two thirds of the size that ADR-1200 D17 gives it, so later sources keep their size against the first source's. Each stands on the script baseline that D1 gives the first source's own box. A later source's script glyph whose ink would pass its cells is drawn smaller to fit them, as its normal glyph would be (ADR-1200 D17).

### D3: a bitmap source with a smaller size

This is the maintainer's two suggestions, made one rule. A **smaller size** of a bitmap source is any of these strikes, in the order the engine names them:

- **The same bitmap at a smaller whole multiple.** Where vtium draws the source at k times its own size, as the automatic rule does on a large display (Context), the source also has every multiple from 1 to k − 1, exactly. A glyph at multiple j draws each of the font's pixels as a block of j×j screen pixels, so nothing is resampled. unifont drawn at 2× in a 16×32 cell has unifont at 1×, its own 8×16.
- **Another height of a CPI source** than the one `cp-size=` takes, at each whole multiple that fits.
- **A font that the frit names with `script-source=`** under the source (D12), at each whole multiple that fits.

The engine takes the largest smaller size whose height is from one half to three quarters of the normal glyph's height, and whose width fits the glyph's cells. The normal glyph's height is the source's height times the multiple it is drawn at. The engine places the chosen glyph by D1, unscaled: its own box, the strike's height times its multiple, against the top of the text band for a superscript and against the bottom for a subscript, and centred across the cells. A centred glyph whose spare columns are odd leaves the odd column on its right, as ADR-1200 D7 centres a built-in glyph. A character the chosen strike lacks is drawn by D4 instead.

For the source's own multiples, the rule gives these sizes:

| Drawn at | Superscript at | Share of the normal height |
|---|---|---|
| 1× | none; D4 scales it to two thirds | 2/3 |
| 2× | 1× | 1/2 |
| 3× | 2× | 2/3 |
| 4× | 3× | 3/4 |
| 5× | 3× | 3/5 |
| 6× | 4× | 2/3 |

So only a bitmap source drawn at its own size, with no CPI height or `script-source=` in the range, reaches D4. At 2× the script is half the normal height, smaller than the two thirds an outline font gets. In unifont at 2×, a normal lowercase e is 16 screen pixels high in a 32-pixel row, and its superscript is 8, where two thirds would make it about 11. The half keeps every row and column of the font's design, one screen pixel each. The record takes the half over D4 at 2×, which would scale 32 rows to 21 and draw some of the font's rows one screen pixel high and others two, so strokes of one width in the font would come out in two widths.

The range is half to three quarters, rather than two thirds exactly, because a font's sizes rarely fall on two thirds. A whole multiple always reaches the range: for a source drawn at 2× or more, the largest smaller multiple at most three quarters is at least half. Terminus at 16 pixels takes Terminus at 12, three quarters; at 20 it takes 14; at 24, 18. Below half, a letter loses too much to read beside its neighbours. Above three quarters, a superscript hardly differs from the normal text.

### D4: a bitmap source with no smaller size

The engine scales the normal glyph's own box to two thirds of its width and height, and keeps it one bit: each pixel of the normal glyph goes to the small pixel that holds its centre, and a small pixel is ink when any pixel that goes to it is ink. It places the result by D1.

This rule keeps every stroke and keeps each one a pixel wide. Over unifont's printable ASCII and six more characters, it changed the shape of 9 of 100 glyphs, where a gap one pixel high closed, as in the dot of i and j (`basis`). Today's averaging, cut at half on a 1-bit display, changed 58 of 100. Taking the pixel under each small pixel's centre changed 60, because it drops the columns where unifont draws its vertical strokes. The figure shows the three side by side.

D4 serves the bitmap sources of a frit, the built-in fonts unifont and Unscii-8, and the glyph that shows a missing character's codepoint in hex.

### D5: a color glyph

The helper scales a color glyph, such as an emoji, from its source's largest strike straight to two thirds of the size it draws the normal glyph at, with the color method it uses for every color glyph (crates/vtium-config/src/resample.rs), once. It never scales the glyph it drew for the normal cell. A color glyph keeps smooth edges, since its own pixels are not one bit. Images that a program places with the kitty graphics protocol, sixel or iTerm2's protocol are not glyphs, and SGR 73 and 74 do not change them.

### D6: a sprite

The console draws a sprite to fill the box that D1 gives a sprite, with the light line's thickness for that height (`sprite::light_line`). Its edges stay on whole pixels, as a sprite's always are, where scaling the full-size sprite would blur them.

### D7: bold and italic

The engine draws a script glyph in its style at its own small size. An outline is emboldened and slanted at the small size (ADR-1302 D4). A bitmap glyph, from D3 or D4, is widened and slanted after it is made small, about its own baseline. A glyph at a whole multiple j is widened by one of its own pixels, j screen pixels, and its rows shift by whole blocks of j, so it stays the font's blocks; a glyph from D4 is widened by a screen pixel and its rows shift by whole pixels. A frit's own bold and italic faces (`style=`, ADR-1200 D1) draw their script glyphs by D2 to D4 like the regular face.

### D8: where a script cell's lines go, and what covers the whole cell

The maintainer reviewed the third slice's pictures. In them the underline and the strike-through stayed where a normal cell draws them, so a subscript's strike-through lay above the glyph. The maintainer asked for "a sane decision tree for the engine to plan where to draw the lines for under or struck" (`considered`, 2026-11-11). This decision is that tree.

The words used here. The **lines** are the underline in each of its styles (single, double, curly, dotted and dashed, and the dashed underline of a link under the pointer), the strike-through and the overline. vtium-render draws each line as a sprite one cell in size, from the rows and thicknesses that the font gives (`Decorations`, ADR-1200 D2, ADR-1302 D3; crates/vtium-render/src/deco.rs). A **script cell** is a cell that SGR 73 or 74 marks. Rows are counted from the cell's top row, which is row 0. The **plain lines** are the lines of a font that states none: a sixteenth of the height thick and at least a row, the underline on the bottom rows and the strike-through from the middle row (`Decorations::for_height`).

The engine decides a script cell's lines in this order.

1. **What covers the whole cell.** The cell's background, reverse video (SGR 7, and DECSCNM for the whole screen), the selection and the cursor cover the whole cell, as they do for any glyph. The maintainer: "for reversed items, I think the text block is ok, because it fills the entire character box". The cursor keeps one shape and one height along a line. The underline cursor therefore lies on a normal cell's underline rows in a script cell too, and it hides the cell's own underline as it does on any cell.
2. **One set of lines for each script.** A script cell's lines are the lines of the script glyph's own size, placed where D1 places the script glyph's own box. The engine makes one set for superscript and one for subscript, from the frit's first source, as the normal lines come from the first source (ADR-1302 D3). Every superscript on a line therefore draws its lines on the same rows, whichever source drew its glyph, and every subscript does too. A sprite, a color glyph and the hex glyph have no lines of their own, and take the set of their cell's script.
3. **How each kind of first source gives its script lines.**
   - An outline source (D2): the helper finds the lines by ADR-1302 D3's rule at the script size. It takes the `post` table's underline and the `OS/2` table's strikeout, scaled to the script glyph's pixels to the em, and measures them from the script baseline that D2 gives. Each line is at least a row thick and lies inside the cell. A script strike reports these lines, as a normal strike reports its own (ADR-1200 D2).
   - A bitmap source with a smaller size (D3): the chosen strike's own lines, at the multiple it is drawn at, moved down by the row where D1 places the strike's top. A strike drawn at a whole multiple has its lines times that multiple, or the plain lines of its drawn height where its own are the plain lines (crates/vtium-font/src/script/lines.rs, `at_multiple`). The source at a smaller multiple gives its own lines at that multiple. A CPI height or a `script-source=` font gives its own lines, and the plain lines of its height where it states none.
   - A bitmap source with no smaller size (D4): the normal lines go through D4's own map. Each row of a line goes to the small row that holds its centre, so the line lands on the row where D4 puts the glyph's pixels from the same row. A line keeps at least one row.
   - The built-in fonts are bitmap sources. unifont takes its script lines by D4 when it is drawn at its own size, and by D3 when it is drawn at twice its size or more.
   - While a frit's script strike is on its way, the console takes the cell's normal lines through D4's map. The strike's own lines replace them when it arrives, as a frit's normal lines replace the built-in font's.
   - One module of vtium-font holds the placing and the map, and the helper and the console both call it (D11).
4. **Where each line lies.**
   - The underline lies on the script set's underline rows. Under a superscript it lies under the raised glyph, in the row's lower part, which the glyph leaves blank. Under a subscript it lies at or near the bottom of the row, under the lowered glyph.
   - The strike-through lies on the script set's strike rows. It crosses the script glyph at the height where the normal strike-through crosses the normal glyph: through the middle of the letters' x-height, or where the font's strikeout puts it.
   - The overline lies on the top row of the script glyph's own box, as thick as the script set's underline. Over a superscript, that is the text band's top row, which is the normal overline's row when the row has no line spacing. Over a subscript, it lies over the lowered glyph.
   - Each line is as thick as the script set makes it, so a line on a small glyph has the weight of the font's line at the small size. Every line fills whole rows with every pixel fully on, so it stays crisp at any thickness, as the maintainer asked of the glyphs: "Having no aliasing and crisp text is the better half of the tradeoff".
5. **Double and curly underlines follow the same rule as in a normal cell.** deco.rs draws them down from the top of the underline's rows, in at most a quarter of the cell's rows, and moves a shape that would leave the cell up into it. Where the quarter is too small, a curl lowers its wave to fit, down to 2 rows from peak to peak, and a double, or a curl that still does not fit, draws as the single line. In a script cell the shape starts from the script set's underline. Under a superscript it has the row's blank lower rows below it, so it draws whole. Under a subscript whose underline lies on the row's bottom rows, the shape moves up to end inside the row, onto the rows a normal cell's shape takes. On a 1-bit display the curl is the wave of whole pixels that deco.rs draws in any cell.
6. **A run of normal and script cells under one underline.** Each cell draws its own lines in its own rows, so the underline steps up or down at the edge between a normal cell and a script cell. vtium draws no stroke that joins the two. Each cell's lines are a sprite inside that cell (ADR-1306 D6), so a cell redrawn alone draws the same pixels whatever its neighbours hold. The line under each glyph also reads as that glyph's own, as each character's dashed underline does (the maintainer, 2026-10-28). A run of superscripts has one unbroken underline, because its cells share one set of lines, and a run of subscripts has another.
7. **Every line stays inside the row,** as the script glyph's ink does (the paragraph after the table). The helper keeps an outline font's script lines inside the cell, and deco.rs moves any line that would leave the cell up into it.

For unifont, the rule gives these rows. Each entry gives the underline's top row, the strike-through's top row, the overline's row, and the thickness of each line.

| unifont drawn at | Cell height | Normal cell | Superscript | Subscript |
|---|---|---|---|---|
| 1×, script glyphs by D4 | 16 | 15, 8, 0; 1 row | 10, 5, 0; 1 row | 15, 10, 5; 1 row |
| 2×, script glyphs by D3 at 1× | 32 | 30, 16, 0; 2 rows | 15, 8, 0; 1 row | 31, 24, 16; 1 row |
| 3×, script glyphs by D3 at 2× | 48 | 45, 24, 0; 3 rows | 30, 16, 0; 2 rows | 46, 32, 16; 2 rows |

In unifont at 16 rows, a superscript's double underline takes rows 10 and 12, and its curl rows 10 to 12, in the blank rows under the raised glyph. A subscript's double takes rows 13 and 15, and its curl rows 13 to 15, which are a normal cell's rows.

**The glyph's ink.** A script glyph's ink stays inside its row. It takes no part in the reach into the rows above and below that ADR-1306 D1 gives other glyphs, because a superscript against the top of its row would otherwise draw over the descenders of the row above. To the side, its ink follows the rule for its kind of glyph (ADR-1302 D1).

> Edited 2026-11-11: the maintainer's review of the third slice's pictures found that the lines did not follow the script glyph (`considered`), and the decision tree above replaces D8's first bullet, which read: "The underline, the double and curly underlines, the strike-through and the overline stay where a normal cell draws them. vtium-render draws them from the cell's metrics, apart from the glyph (ADR-1200 D20, crates/vtium-render/src/deco.rs). The cell's background, the cursor and the selection cover the whole cell, as they do for any glyph." D8 was titled "what stays where a normal cell has it". The earlier text is in git at 3ea2c09f.

### D9: wide characters

A wide character's script glyph is made and placed by the same rules, and centred across its two cells. In D3, a strike's width must fit the two cells. In D4, the glyph is two thirds of the wide glyph's width, and in D6 a sprite is two thirds of the two cells' width.

### D10: the 1-bit display

A bitmap script glyph from D3 or D4 is one bit, so a 1-bit display shows it as it is. An outline script glyph is cut at half like every outline glyph on that display (crates/vtium-render/src/mono.rs). A color script glyph takes the stand-in that a 1-bit display draws for color glyphs, unifont's bitmap, in its script form by D4.

### D11: where the engine runs

- **The helper draws the script glyphs of a frit's sources.** The console asks for a cluster in a style and a script, as it asks for a style today (ADR-1200 D21). The store keys a glyph by its text, its style, its script and its width in cells, at a cell (ADR-1200 D20), as the renderer's cache already does (ADR-1300 D1). Each strike of a set says its script (ADR-1200 D2). The fill draws normal glyphs alone, and script glyphs come only when the console asks for them.
- **The console draws the script glyphs of its own sources:** the built-in fonts, the hex glyph and the sprites. It never rasterizes, and it parses no font (ADR-1200 D6).
- **One module of vtium-font holds the rules both sides use:** the size, the placement against the edge, the range of D3 and the on-or-off scale of D4. The helper and the console call it, so they never compute two different boxes.
- **While the helper's glyph is on its way,** the console draws the cell with the built-in font's script glyph. This is the placeholder every glyph already has while the helper draws it, normal glyphs included (ADR-1200 D7, D22 step 4; crates/vtium/src/deferred.rs:9-11), and this record does not change it. The frit's own script glyph replaces it when it arrives, as the frit's normal glyph replaces the built-in one. The lookup chain (ADR-1200 D22) runs within the cell's script: a cluster that the console builds from a base and its marks takes the script glyph of each.

### D12: `script-source=`, a smaller size of a bitmap source

- `script-source=` is a frit key for the bitmap `source=` line above it. It is repeatable, and it names a smaller size of the same font in any bitmap source form: `kbd:`, `file:` or `bdf:`. The font it names takes that source line's `unimap=` where it has no Unicode table of its own.
- vtium refuses a frit with `script-source=` under an outline source, and names the file and the line, as it refuses `cp-size=` under a source that is not a CPI font.
- A `script-source=` font that is not on the machine is left out quietly, as any source is (ADR-1200 D1). `vtium config check` notes a `script-source=` whose height falls outside D3's range for the frit's own cell, because no glyph would come from it there.
- vtium does not look for a font's other sizes by its file name. kbd's names, BDF's family names and files on disk follow no common pattern, and a frit's author knows which fonts belong together. vtium-config's guided builder may offer `script-source=` when it finds another size of the same family.

### D13: OpenType's designed superscripts

vtium does not use the `sups` and `subs` features, which replace a character with a glyph the font designs for superscript or subscript. The fonts measured cover the digits and a few signs with them, from 6 to 23 characters, and DejaVu Sans Mono covers none (`basis`). A superscript such as 2n would draw its 2 as the font designed it and its n by D2, at another size and height. They become worth using when two things hold: a frit's author turns them on for a source whose features cover the characters in use, and the helper shapes a run of script cells together, so that one run takes one form. The work on the outline font stack, which brings shaping across clusters for ligatures, is where the second can come from. Until then each cluster is asked for alone (ADR-1200 D21).

### D14: what vtium-config shows

vtium-config's font preview and the TUI's font view add a line of superscript and subscript text, such as x2 + y2 and H2O with the digits in script, drawn by the helper through this engine. The line's caption says how the font makes its small glyphs, in the reader's words: from a smaller size of the same font, naming it; from the font itself at a smaller size; or scaled. The words follow AGENTS.md's rules for strings on screen.

## Gap check

The code at integration 44792f56, read statement by statement against this record. Each line is a gap the build starts from.

1. **D4, the on-or-off scale.** `Glyph::scripted` averages every glyph (crates/vtium-font/src/script.rs:110-157). Nothing tells it whether a mask came from a 1-bit strike: a `Glyph` is R8 or RGBA, and a 1-bit strike is expanded to R8 before the console sees it. The face needs to say the kind of its source.
2. **D1, size and place.** `scripted` scales the whole cell, line spacing's rows and any blank rows around a bitmap glyph included, and places the result against the cell's top and bottom, rather than scaling the glyph's own box and placing it against the text band (script.rs:110-118).
3. **D6, sprites.** A sprite in a script cell is drawn at the cell's size and then averaged (crates/vtium-font/src/face.rs:329-333, 353-360).
4. **D7, style.** The style is drawn first and the result scaled (face.rs:354), so a bitmap's widening and shear are averaged too.
5. **D2, D3, D5, D11, the helper.** An ask carries a style and no script (crates/vtium/src/helper.rs:1003-1006). The helper's serve loop and its outline drawer draw no script glyph (crates/vtium-config/src/serve.rs, crates/vtium-config/src/outline.rs:1060). The color path scales to the cells and has no script box (outline.rs:1175-1215).
6. **D11, the store and the wire format.** The store's key is text and style (crates/vtium/src/deferred.rs:98-110). A strike record carries a style and no script; its reserved 16-bit field after the format (crates/vtium-font/src/set/format.rs:12) can carry it in a new minor version.
7. **D11, meanwhile.** No stand-in path exists for a script glyph, since none is asked for today.
8. **D3, smaller sizes.** No code chooses a smaller whole multiple, keeps a CPI file's other heights, or picks a strike by the half-to-three-quarters range. The multiple a bitmap source is drawn at is known to the face (`Face::scale`, crates/vtium-font/src/face.rs:118; crates/vtium-font/src/set/face.rs:133), and the helper draws a bitmap source at the cell's multiple, so both sides can find the smaller multiples. A script glyph at multiple j could be stored at 1× with scale j, as ADR-1300 D2 stores a bitmap glyph, where its place in the cell falls on whole blocks of j; `Font::glyph_unscaled_variant` hands every script glyph over at its drawn size today (face.rs:389-399).
9. **D12, the frit key.** The frit reader has no `script-source=` (crates/vtium-config/src/frit.rs, beside `cp-size` at 375-390), and `vtium config check` has no note for it.
10. **D8, ink.** `scripted` cuts all ink to the cells, sideways too. The helper's script glyphs need ink boxes cut to the row and not to the sides.
11. **D10, 1-bit.** The 1-bit path gives a script mask no stand-in and cuts its gray at half (crates/vtium-render/src/mono.rs:222). With D4 in place, bitmap script glyphs arrive one bit and need nothing more there.
12. **D14, vtium-config.** The specimen (crates/vtium-config/src/specimen.rs) and the preview (crates/vtium-config/src/preview.rs) show no superscript or subscript.
13. **Words to update once this is accepted.** ADR-1000's `sgr.script` row says "a bitmap font's glyph scales the same way, with gray edges"; vtium-font's README section "Superscript and subscript" and script.rs's module comment describe the averaging; tier 2's `script-unifont` caption says "their edges gray"; ADR-1200 D1, D2, D20 and D21 each take a dated note pointing here. script.rs's module comment, vtium-font's README and ADR-1000's `sgr.script` row cite mintty's CtrlSeqs page for SGR 73 to 75, which does not mention them; each should cite the Tips page's table of text attributes instead.
14. **D8, the lines, read at integration 3ea2c09f.** A font gives one set of lines, for the normal cell: `Face::decorations` and `Font::decorations` take no script (crates/vtium-font/src/face.rs:122-124, 328-330), and `Decorations` has no row for the overline, which deco.rs draws on the cell's top row (face.rs:59-64, crates/vtium-render/src/deco.rs:274-277). The script module needs the rule that makes a script set from the normal lines and the script box: D3's placing and D4's map. A set gives the lines of its regular baseline strike (crates/vtium-font/src/set/read.rs:686-692), so a script strike's own lines are never read. The helper finds an outline font's lines at the normal cell alone (crates/vtium-config/src/outline.rs:1477), and its script strikes need lines found at the script size. The console's store and its face give one set (crates/vtium/src/deferred.rs:1026-1029, 1485-1500). The renderer makes one set of line sprites from `font.decorations()` and makes it again when the lines change (crates/vtium-render/src/renderer.rs:619-633, 719, 914-925). `Sprites` holds the eight kinds of one set (deco.rs:284-310). The CPU path draws every cell's lines from that one set (crates/vtium-render/src/compose.rs:500-531), and the GPU path lays one set of eight sprites in its texture and picks a sprite by its kind alone (crates/vtium-render/src/gpu/glyphs.rs:297-310, 401; crates/vtium-render/src/gpu/compose.rs:1952-1958). Both paths need a set for each script, picked by the cell's script, with the underline cursor's line taken from the normal set (renderer.rs:1830-1836).

The work can land in slices that each show on screen. The first slice is the console's side: D1's box, D4, D6, D7 and D10 in vtium-font, which makes the built-in fonts' scripts crisp and fixes the 1-bit display. The second is the helper's side: the ask, the key and the wire format, with D2, D4 for a frit's bitmap sources, and D5. The third is D3 and D12. The lines of D8 (gap 14) follow it as a slice of their own, since they draw on the script box that every kind of source now gives. The last is D14.

## Verification

Each test that drives this behaviour through vtium-font, vtium-render, vtium-config or vtium carries `// covers: sgr.script`. Tier 2's goldens change only in a commit of their own, which the maintainer reviews.

- vtium-font: D4's scale of a one-pixel stroke at each of the three phases two thirds gives keeps it one pixel wide; a glyph scaled by D4 has only on and off pixels; D1's box against the text band with and without line spacing; D3's choice among a set of strikes, at each end of the range, and the table of whole multiples from 1× to 6×; a script glyph at a smaller multiple is the 1× glyph with each pixel a j×j block, placed against the band's edge.
- Tier 2: the `script-unifont` screen redrawn, and in 1-bit mode; the same screen in unifont at 2× and at 3×; a screen of a bitmap test font built in memory at two heights with `script-source=`; a screen of the outline test font with superscripts and subscripts in regular, bold and italic, beside an underline and a strike-through.
- The helper: an ask with a script answers a strike that says its script; the console refuses an answer whose script it did not ask for, as it refuses a style it did not ask for.
- D8 in vtium-font: the script sets of unifont at 1×, 2× and 3× are the rows of D8's table; D4's map takes a font's stated lines to the rows where it takes the glyph's pixels from the same rows; line spacing moves a script set with the script box; every line of every script set lies inside the cell. The helper: an outline script strike reports lines found at the script size, measured from the script baseline, in the outline test font.
- D8 in vtium-render: for each line (single, double, curly, dotted and dashed underline, strike-through, overline, and strike-through with overline) and for each script, the sprite's lit rows are the script set's rows. In unifont at 16 rows, a superscript's double and curl lie on rows 10 to 12 and a subscript's on rows 13 to 15. A 1-bit curl in a script cell is joined whole pixels. Two underlined cells side by side, one normal and one a superscript, draw their underlines on their own rows. The background, reverse video, the selection, and the block, bar and underline cursors draw in a script cell as in a normal cell. The CPU path and the GPU path draw the same lines.
- D8 in tier 2: the screens `script-unifont`, `script-unifont-2x`, `script-unifont-3x` and `script-source` are redrawn with lines that follow the glyph, each screen with a line of superscripts and subscripts under each line style, and an underlined run of normal and script cells. Their captions say where each line lies. The goldens change in a commit of their own, which the maintainer reviews.

## Consequences

### Positive

- Bitmap fonts draw crisp superscripts and subscripts, and a 1-bit display shows them whole.
- A bitmap font with smaller sizes draws its scripts in its own design.
- Outline scripts are hinted and antialiased at their own size, as sharp as any glyph of that size.
- Every kind of source places its scripts by one rule, so a line of mixed fonts keeps one superscript edge and one subscript edge.
- Sprites in script cells keep whole-pixel edges.
- A script glyph's underline, strike-through and overline lie under, across and over the small glyph, for every kind of source and at every size.
- Every superscript on a line shares one underline, and every subscript shares another.

### Negative

- A script glyph from a frit's font costs an ask, and the built-in font draws it for that moment.
- The store holds a strike for each script in use, beside each style.
- A designed smaller size can be up to three quarters of the normal height, so its scripts are larger than a scaled font's.
- D4 closes a gap one pixel high in some glyphs, such as the dot of i.
- The wire format and the store's key change on both ends.
- An underline across normal and script cells steps up or down at each edge between them.
- The renderer keeps three sets of line sprites where it kept one, and the GPU path's texture of line sprites holds three times as many.
- The helper finds an outline font's lines at each script size as well as at the normal size.

### Neutral

- The renderer's cache already keys the script, so it does not change.
- The background, reverse video, the selection and the cursor draw in a script cell as in any cell.
- vtium uses neither OS/2's superscript metrics nor OpenType's `sups` and `subs`, and D13 names what would make the second worth using.

## Alternatives Considered

- **Keep the averaging scale for everything.** Rejected: the maintainer asked for crisp bitmap scripts, and on a 1-bit display the averaged glyph cut at half changes the shape of 58 of 100 unifont glyphs.
- **Take the pixel under each small pixel's centre.** Rejected: it changes 60 of 100, dropping unifont's vertical strokes in columns 1 and 6.
- **Cut the average at half.** Rejected: it is what a 1-bit display shows today, the figure's third row.
- **The console makes every script glyph from the glyphs it holds, choosing by their pixel format.** This needs no change to the wire format, and the console could tell a 1-bit strike's glyph from an outline's. Not chosen: outline scripts would stay scaled from a full-size raster, and the console cannot reach a font's smaller sizes, since it parses no font (ADR-1200 D6). Its first part, D4 for the console's own sources, is the first slice of the work.
- **Read OS/2's superscript and subscript sizes and offsets.** Not chosen (the maintainer, 2026-11-11): seven of the nine fonts measured carry the font editors' defaults, DejaVu Sans Mono's offsets pass the cell, and each source on a line would take its own size and place (D1). The web found the same. CSS Fonts dropped its requirement to use these metrics in 2015, because they are often wrong or vary widely between fonts, and Mozilla's work on font-variant-position found Gecko using some OS/2 values and WebKit using fixed ratios (`basis`). Few terminals draw SGR 73 and 74, so no terminal practice points the other way.
- **Use OpenType's `sups` and `subs`.** Not chosen for now (D13).
- **Find a bitmap font's other sizes by its file name or family name.** Not chosen: the names follow no common pattern, so the result would depend on how a distribution names its files (D12).
- **Draw unifont's scripts from Unscii-8, the other built-in font,** which is exactly half of unifont's height. Rejected by D15: it is another font with another design, and the maintainer asked that vtium not stomp over a font set's arrangement with unifont or Unscii-8. unifont at its own size scales its own glyphs by D4.
- **At 2×, scale the glyph to two thirds by D4 instead of taking the font at 1×.** Not chosen: the script would be about 11 rows of a 16-row font's 16 instead of 8, but the font's one-pixel strokes would come out one or two screen pixels wide in an uneven pattern, where the 1× glyph keeps the design exactly (D3).
- **Take two thirds exactly from a smaller size, or nothing.** Not chosen: Terminus's heights never give two thirds of another, so the maintainer's suggestion would never apply to it.
- **Let a superscript reach the row above, as other glyphs may (ADR-1306 D1).** Not chosen: it would draw over the descenders of that row, and the box already keeps the glyph in its row.
