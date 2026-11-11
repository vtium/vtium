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
  - evidence: "Measured by this record's agent on 2026-11-11 from vtium-font's data/unifont.hex at integration 44792f56, over the 94 printable ASCII characters and é, É, Å, ², α and β, each 8×16 glyph scaled to 5×11 as `scripted` sizes it (crates/vtium-font/src/script.rs:47, 110). A glyph's shape counts as kept when its number of separate strokes (8-connected ink) and of enclosed holes (4-connected background not touching the edge) are unchanged. Each small pixel the average of what it covers, as vtium draws it today, then cut at half, as a 1-bit display cuts it (crates/vtium-render/src/mono.rs:18-21): 58 of 100 changed. Each small pixel the source pixel under its centre: 60 of 100 changed, because columns 1 and 6, where unifont draws its vertical strokes, fall between centres and vanish. Each source pixel sent to the small pixel that holds its centre, a small pixel ink when any pixel sent to it is ink: 9 of 100 changed (!, %, &, ?, @, a, i, j and Å, where a gap one pixel high closes). The script and the figure's source are in the agent's scratchpad, uncommitted."
  - evidence: "Read by this record's agent on 2026-11-11 with fontTools 4.68.0 from the fonts installed on the maintainer's machine. OS/2 superscript size (x by y), superscript offset, subscript size and subscript offset, in em: Noto Sans Mono, Noto Sans, Noto Sans CJK, Fira Mono, Fira Code, Hack and JetBrains Mono all carry 0.65×0.6, 0.35, 0.6 and 0.075; DejaVu Sans Mono 0.65×0.7, 0.48, 0.7 and 0.14; Liberation Mono 0.7×0.65, 0.422, 0.65 and 0.139. DejaVu Sans Mono's ascent is 0.928 em and its descent 0.236 em."
  - evidence: "Read by this record's agent on 2026-11-11 with fontTools 4.68.0, the characters each font's `sups` and `subs` lookups map: JetBrains Mono 11 (+ and the digits) and 10 (the digits); Fira Mono 15 and 14 (the digits, parentheses, plus, minus, and = for `sups`); Hack 10 and 10 (the digits); Noto Sans Mono 16 (the digits, parentheses, +, =, − and n) and 23 (those and h, k, l, m, p, s and t); Liberation Mono 6 (4 to 9) and 10 (the digits). DejaVu Sans Mono and Noto Sans CJK have neither feature."
  - evidence: "Terminus as installed on the maintainer's machine: the kbd console fonts ter-112n to ter-132n and the bitmap OpenType files ter-u12 to ter-u32 come at heights 12, 14, 16, 18, 20, 22, 24, 28 and 32, one height to a file."
  - evidence: "vtium at integration 44792f56: the console scales the normal glyph it drew (crates/vtium-font/src/face.rs:353-360, `Font::drawn_variant`), so the helper never sees the script (crates/vtium/src/helper.rs:1003-1006, an ask carries a style alone); a 1-bit display gives a superscript or subscript mask no stand-in and cuts its gray at half (crates/vtium-render/src/mono.rs:222); the renderer's cache already keys the script (crates/vtium-render/src/cache.rs:298-305); a set's strike record has a reserved 16-bit field after its format (crates/vtium-font/src/set/format.rs:12); the helper scales a color glyph once, from its source's largest strike, by the color method the operator chose (crates/vtium-config/src/outline.rs:1175-1215, crates/vtium-config/src/resample.rs:1-14); a CPI font holds heights 8, 14 and 16, of which `cp-size=` takes one (crates/vtium-config/src/frit.rs:28, 375-390)."
  - standard: "mintty's control sequences, https://github.com/mintty/mintty/wiki/CtrlSeqs, Text attributes and rendering: SGR 73 superscript, SGR 74 subscript, SGR 75 neither. The page names the codes and leaves the size and place of the glyph to the terminal."
  - standard: "OpenType 1.9.1, the OS/2 table (ySuperscriptXSize, ySuperscriptYSize, ySuperscriptYOffset and the subscript equivalents), https://learn.microsoft.com/en-us/typography/opentype/spec/os2, and the feature tags `sups` and `subs`, https://learn.microsoft.com/en-us/typography/opentype/spec/features_pt"
  - precedent: ADR-1200
  - precedent: ADR-1300
  - precedent: ADR-1302
  - precedent: ADR-1306
agent:
  name: Claude
  model: claude-opus-5-5
observable:
  - "Tier 2's `script-unifont` screen shows every superscript and subscript pixel fully on or fully off, each small glyph two thirds of its cell's height, with the shapes the figure's last row shows"
  - "The same screen in 1-bit mode shows the same small glyphs, where today it shows broken strokes"
  - "A frit of Terminus at 16 pixels with `script-source=kbd:ter-112n` draws its superscripts and subscripts with Terminus's own 12-pixel glyphs"
  - "In DejaVu Sans Mono, a superscript letter's edges are as sharp as a normal letter's at the same size"
status: proposed
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

- **Decided:** vtium draws a superscript or subscript glyph (SGR 73 or 74) as a smaller version of the character's glyph, made by the source the glyph comes from, in the way that suits that kind of source. Every kind shares one rule for size and place: two thirds of the normal glyph's size, centred across the cells, against the top of the row's text for a superscript and against its bottom for a subscript. An outline font's glyph is rasterized at two thirds of its size. A bitmap font's glyph comes from a smaller size of the same font where the font has one: the same font at a smaller whole factor, another height of a CPI file, or a smaller font that the frit names with a new key, `script-source=`. Where it has none, the glyph is scaled to two thirds with each small pixel fully on or fully off, so it stays crisp. A color glyph is scaled smoothly, once, from its source. vtium's own drawn symbols are drawn at the box's size. The helper draws the glyphs of a frit's sources, asked for by script as it is asked for by style. The console draws its built-in fonts' and its symbols' small glyphs. Both take the box and the on-or-off scale from one module of vtium-font.
- **Trades away:** The single rule of today, which scales whatever glyph was drawn. A superscript in a frit's font now waits for the helper, as a bold glyph does, and draws in the built-in font for that moment. A designed smaller size can be up to three quarters of the normal height, so its superscripts are larger than another font's. In 9 of 100 unifont glyphs a gap one pixel high closes. Script glyphs keep no ink above or below their row. OpenType's designed superscript glyphs and the fonts' own superscript metrics go unused.
- **One-way?** No. The rules live in vtium-font's script module and in the helper, and the wire format gains the script on both ends in one release, as it gained the style.
- **Probes:**
  - *Confident:* a superscript or subscript in unifont, or in any bitmap font with no smaller size, has only fully lit or fully dark pixels, two thirds of the normal size, as the last row of the figure shows, on a color display and on a 1-bit one alike. Right?
  - *Not confident:* a smaller size of the same font is used when it is between half and three quarters of the normal height, so Terminus at 16 pixels takes its superscripts from Terminus at 12, which is three quarters, rather than a scaled glyph at two thirds. Is a script that much closer to the normal size acceptable, to keep the font's own design?
- **Inversion:** The decision sits between "scale every drawn glyph the same way", which vtium does today with one rule in the console, and "let each font draw its small glyphs however it likes", which would give each format its best glyph but no common size or place. vtium takes one box and one placement for every kind of source, and lets only the drawing inside the box depend on the kind.

## Context

SGR 73 starts superscript, SGR 74 starts subscript, and SGR 75 returns to the baseline. mintty's control sequences name the codes and leave the glyph's size and place to the terminal (`basis`). The sequences are used for mathematical and chemical text, as in x² or H₂O written with ordinary digits.

vtium draws them today with one rule (crates/vtium-font/src/script.rs). It draws the cell's normal glyph, then scales it to two thirds of its width and height. Each pixel of the small glyph is the average of the normal pixels it covers. A superscript sits against the top of the cell, and a subscript against the bottom. The console does this to whatever glyph it already holds (`Font::drawn_variant`), so the glyph helper (vtium-config, the sandboxed program that turns the frit's fonts into glyphs for the console) never hears of the script.

A frit (a font set file) names its fonts as sources, each a line of its own (ADR-1200 D1). A source is an outline font, or a bitmap font: a kbd console font, a PSF, CPI or raw file, a BDF file, the kernel's font, or a built-in font. Each bitmap format vtium reads holds one height, except CPI, which holds heights 8, 14 and 16, of which `cp-size=` takes one. A bitmap font is drawn at whole factors of its own size (ADR-1200 D17), so a source drawn at twice its size also has its own size available, exactly. Families such as Terminus ship each height as a file of its own, from 12 to 32 pixels (`basis`). Color glyphs come from outline files' color tables, and the helper scales each once from its source to its cells.

The averaging rule turns a bitmap font's 1-bit glyph gray and soft. The maintainer reviewed the result in unifont and answered "Crisp for bitmap fonts". On a 1-bit display the gray is cut at half, and the strokes break (the figure's third row). The maintainer also suggested using a smaller size of the same bitmap font where one exists, and asked for "some sort of coherent engine for super and subscript, for managing bitmap and outlined fonts" (`basis`).

An outline glyph scaled from its full-size raster is softer than the same glyph rasterized at the small size, because the rasterizer and the hinter place its edges on the small size's pixel grid only when they draw at that size.

![Six unifont characters, e, a, B, 8, 2 and x, in four rows of 8×16 cells, each pixel drawn large. The first row shows the normal glyphs. The second row shows today's superscripts, two thirds of the size at the top of the cell, in grays where the averaging blends stroke and background. The third row shows those superscripts on a 1-bit display, the gray cut at half: most strokes are gone, and only a few dashes and dots remain. The fourth row shows this record's superscripts, two thirds of the size, every pixel fully on or off, each character's strokes whole and readable.](figures/ADR-1202-bitmap-script.svg)

## Decision

The words used below. A **script glyph** is the glyph of a superscript or subscript cell. A **strike** is a font's glyphs at one size. The **text band** is the rows of a cell that the text takes: the whole cell, less the rows that line spacing adds above and below (ADR-1302 D13). A **sprite** is a symbol vtium draws itself to fill its cell, such as box drawing, block elements, shades and Braille.

### D1: one box for every kind of source

- A glyph's **own box** is the rows from its font's ascent to its descent at the size it is drawn: the whole height of a bitmap strike, at the factor it is drawn at, and the rows an outline font's ascent and descent span. A script glyph is two thirds of the size of the normal glyph, rounded to whole pixels, unless D3 finds a smaller size of the font. Each kind of source makes it in its own way (D2 to D6).
- The engine places a script glyph so that its own box lies against an edge of the text band: its top on the band's top for a superscript, and its bottom on the band's bottom for a subscript. Across, the glyph is centred in its cells, as every glyph's advance is centred (ADR-1302 D2). Every source on a line therefore puts its superscripts against one edge and its subscripts against the other.
- A sprite has no font, so its box is two thirds of the text band's height and two thirds of its cells' width, placed against the same edges (D6).
- **Two thirds** is the size vtium draws today, and the size the maintainer reviewed, asking only for crisp edges. In a 16-row cell it keeps 11 rows, in which the letters keep about 7 (script.rs:16-18). The fonts' own superscript metrics name 0.6 to 0.7 of the em, a little under two thirds of a cell that the font's ascent and descent fill.
- **Against the edge** keeps the script glyph inside its row and as far from the baseline as the row allows. The fonts' own offsets would pass the row in DejaVu Sans Mono, the font vtium's tests draw: its superscript, 0.7 em raised 0.48 em, reaches 0.48 + 0.7 × 0.928 = 1.13 em above the baseline, 0.2 em above the cell's top (`basis`). Its subscript passes the cell's bottom by 0.07 em the same way.
- vtium does not read the OS/2 table's superscript and subscript sizes or offsets. Seven of the nine fonts measured carry the same values, 0.65×0.6 em raised 0.35 em, which font editors fill in, so they describe no design of those fonts. Reading them would give each source on a line a different script size and place.

### D2: an outline source

- The helper rasterizes the glyph at two thirds of the pixels to the em it draws the normal glyph at, with the frit's hinting and the display's antialiasing, as it draws any outline glyph (ADR-1200 D1, ADR-1301 D4). It never scales a full-size raster.
- Every outline source of the frit draws its script glyphs at two thirds of the size that ADR-1200 D17 gives it, so later sources keep their size against the first source's. Each stands on the script baseline that D1 gives the first source's own box. A later source's script glyph whose ink would pass its cells is drawn smaller to fit them, as its normal glyph would be (ADR-1200 D17).

### D3: a bitmap source with a smaller size

This is the maintainer's suggestion, made a rule. A **smaller size** of a bitmap source is any of these strikes:

- the source's own glyphs at a smaller whole factor than the one the cell draws it at, so unifont drawn at twice its size in a 16×32 cell has unifont at its own 8×16;
- another height of a CPI source than the one `cp-size=` takes;
- a font that the frit names with `script-source=` under the source (D12), at a whole factor of its own.

The engine takes the largest smaller size whose height is from one half to three quarters of the normal glyph's height, and whose width fits the glyph's cells. The normal glyph's height is the source's height times the factor it is drawn at. It places that strike's glyph by D1, unscaled, so every pixel stays the font's own. A character the strike lacks is drawn by D4 instead.

The range is half to three quarters, rather than two thirds exactly, because a font's sizes rarely fall on two thirds. A whole factor always reaches the range: for a source drawn at twice its size or more, the largest smaller factor that fits is at least half. Terminus at 16 pixels takes Terminus at 12, three quarters; at 20 it takes 14; at 24, 18. Below half, a letter loses too much to read beside its neighbours. Above three quarters, a superscript hardly differs from the normal text.

### D4: a bitmap source with no smaller size

The engine scales the normal glyph's own box to two thirds of its width and height, and keeps it one bit: each pixel of the normal glyph goes to the small pixel that holds its centre, and a small pixel is ink when any pixel that goes to it is ink. It places the result by D1.

This rule keeps every stroke and keeps each one a pixel wide. Over unifont's printable ASCII and six more characters, it changed the shape of 9 of 100 glyphs, where a gap one pixel high closed, as in the dot of i and j (`basis`). Today's averaging, cut at half on a 1-bit display, changed 58 of 100. Taking the pixel under each small pixel's centre changed 60, because it drops the columns where unifont draws its vertical strokes. The figure shows the three side by side.

D4 serves the bitmap sources of a frit, the built-in fonts unifont and Unscii-8, and the glyph that shows a missing character's codepoint in hex.

### D5: a color glyph

The helper scales a color glyph, such as an emoji, from its source's largest strike straight to two thirds of the size it draws the normal glyph at, with the color method it uses for every color glyph (crates/vtium-config/src/resample.rs), once. It never scales the glyph it drew for the normal cell. A color glyph keeps smooth edges, since its own pixels are not one bit. Images that a program places with the kitty graphics protocol, sixel or iTerm2's protocol are not glyphs, and SGR 73 and 74 do not change them.

### D6: a sprite

The console draws a sprite to fill the box that D1 gives a sprite, with the light line's thickness for that height (`sprite::light_line`). Its edges stay on whole pixels, as a sprite's always are, where scaling the full-size sprite would blur them.

### D7: bold and italic

The engine draws a script glyph in its style at its own small size. An outline is emboldened and slanted at the small size (ADR-1302 D4). A bitmap glyph, from D3 or D4, is widened by a pixel and slanted by shifting its rows after it is made small, about its own baseline, so each step moves whole pixels. A frit's own bold and italic faces (`style=`, ADR-1200 D1) draw their script glyphs by D2 to D4 like the regular face.

### D8: what stays where a normal cell has it

- The underline, the double and curly underlines, the strike-through and the overline stay where a normal cell draws them. vtium-render draws them from the cell's metrics, apart from the glyph (ADR-1200 D20, crates/vtium-render/src/deco.rs). The cell's background, the cursor and the selection cover the whole cell, as they do for any glyph.
- A script glyph's ink stays inside its row. It takes no part in the reach into the rows above and below that ADR-1306 D1 gives other glyphs, because a superscript against the top of its row would otherwise draw over the descenders of the row above. To the side, its ink follows the rule for its kind of glyph (ADR-1302 D1).

### D9: wide characters

A wide character's script glyph is made and placed by the same rules, and centred across its two cells. In D3, a strike's width must fit the two cells. In D4, the glyph is two thirds of the wide glyph's width, and in D6 a sprite is two thirds of the two cells' width.

### D10: the 1-bit display

A bitmap script glyph from D3 or D4 is one bit, so a 1-bit display shows it as it is. An outline script glyph is cut at half like every outline glyph on that display (crates/vtium-render/src/mono.rs). A color script glyph takes the stand-in that a 1-bit display draws for color glyphs, unifont's bitmap, in its script form by D4.

### D11: where the engine runs

- **The helper draws the script glyphs of a frit's sources.** The console asks for a cluster in a style and a script, as it asks for a style today (ADR-1200 D21). The store keys a glyph by its text, its style, its script and its width in cells, at a cell (ADR-1200 D20), as the renderer's cache already does (ADR-1300 D1). Each strike of a set says its script (ADR-1200 D2). The fill draws normal glyphs alone, and script glyphs come only when the console asks for them.
- **The console draws the script glyphs of its own sources:** the built-in fonts, the hex glyph and the sprites. It never rasterizes, and it parses no font (ADR-1200 D6).
- **One module of vtium-font holds the rules both sides use:** the size, the placement against the edge, the range of D3 and the on-or-off scale of D4. The helper and the console call it, so they never compute two different boxes.
- **While the helper's glyph is on its way,** the console draws the cell with the built-in font's script glyph, as it draws a style's glyph meanwhile (crates/vtium/src/deferred.rs:9-11). The lookup chain (ADR-1200 D22) runs within the cell's script: a cluster that the console builds from a base and its marks takes the script glyph of each.

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
8. **D3, smaller sizes.** No code chooses a smaller whole factor, keeps a CPI file's other heights, or picks a strike by the half-to-three-quarters range.
9. **D12, the frit key.** The frit reader has no `script-source=` (crates/vtium-config/src/frit.rs, beside `cp-size` at 375-390), and `vtium config check` has no note for it.
10. **D8, ink.** `scripted` cuts all ink to the cells, sideways too. The helper's script glyphs need ink boxes cut to the row and not to the sides.
11. **D10, 1-bit.** The 1-bit path gives a script mask no stand-in and cuts its gray at half (crates/vtium-render/src/mono.rs:222). With D4 in place, bitmap script glyphs arrive one bit and need nothing more there.
12. **D14, vtium-config.** The specimen (crates/vtium-config/src/specimen.rs) and the preview (crates/vtium-config/src/preview.rs) show no superscript or subscript.
13. **Words to update once this is accepted.** ADR-1000's `sgr.script` row says "a bitmap font's glyph scales the same way, with gray edges"; vtium-font's README section "Superscript and subscript" and script.rs's module comment describe the averaging; tier 2's `script-unifont` caption says "their edges gray"; ADR-1200 D1, D2, D20 and D21 each take a dated note pointing here.

The work can land in slices that each show on screen. The first slice is the console's side: D1's box, D4, D6, D7 and D10 in vtium-font, which makes the built-in fonts' scripts crisp and fixes the 1-bit display. The second is the helper's side: the ask, the key and the wire format, with D2, D4 for a frit's bitmap sources, and D5. The third is D3 and D12. The last is D14.

## Verification

Each test that drives this behaviour through vtium-font, vtium-render, vtium-config or vtium carries `// covers: sgr.script`. Tier 2's goldens change only in a commit of their own, which the maintainer reviews.

- vtium-font: D4's scale of a one-pixel stroke at each of the three phases two thirds gives keeps it one pixel wide; a glyph scaled by D4 has only on and off pixels; D1's box against the text band with and without line spacing; D3's choice among a set of strikes, at each end of the range.
- Tier 2: the `script-unifont` screen redrawn, and in 1-bit mode; a screen of a bitmap test font built in memory at two heights with `script-source=`; a screen of the outline test font with superscripts and subscripts in regular, bold and italic, beside an underline and a strike-through.
- The helper: an ask with a script answers a strike that says its script; the console refuses an answer whose script it did not ask for, as it refuses a style it did not ask for.

## Consequences

### Positive

- Bitmap fonts draw crisp superscripts and subscripts, and a 1-bit display shows them whole.
- A bitmap font with smaller sizes draws its scripts in its own design.
- Outline scripts are hinted and antialiased at their own size, as sharp as any glyph of that size.
- Every kind of source places its scripts by one rule, so a line of mixed fonts keeps one superscript edge and one subscript edge.
- Sprites in script cells keep whole-pixel edges.

### Negative

- A script glyph from a frit's font costs an ask, and the built-in font draws it for that moment.
- The store holds a strike for each script in use, beside each style.
- A designed smaller size can be up to three quarters of the normal height, so its scripts are larger than a scaled font's.
- D4 closes a gap one pixel high in some glyphs, such as the dot of i.
- The wire format and the store's key change on both ends.

### Neutral

- The renderer's cache already keys the script, so it does not change.
- vtium uses neither OS/2's superscript metrics nor OpenType's `sups` and `subs`, and D13 names what would make the second worth using.

## Alternatives Considered

- **Keep the averaging scale for everything.** Rejected: the maintainer asked for crisp bitmap scripts, and on a 1-bit display the averaged glyph cut at half changes the shape of 58 of 100 unifont glyphs.
- **Take the pixel under each small pixel's centre.** Rejected: it changes 60 of 100, dropping unifont's vertical strokes in columns 1 and 6.
- **Cut the average at half.** Rejected: it is what a 1-bit display shows today, the figure's third row.
- **The console makes every script glyph from the glyphs it holds, choosing by their pixel format.** This needs no change to the wire format, and the console could tell a 1-bit strike's glyph from an outline's. Not chosen: outline scripts would stay scaled from a full-size raster, and the console cannot reach a font's smaller sizes, since it parses no font (ADR-1200 D6). Its first part, D4 for the console's own sources, is the first slice of the work.
- **Read OS/2's superscript and subscript sizes and offsets.** Not chosen: seven of the nine fonts measured carry the font editors' defaults, DejaVu Sans Mono's offsets pass the cell, and each source on a line would take its own size and place (D1).
- **Use OpenType's `sups` and `subs`.** Not chosen for now (D13).
- **Find a bitmap font's other sizes by its file name or family name.** Not chosen: the names follow no common pattern, so the result would depend on how a distribution names its files (D12).
- **Draw unifont's scripts from Unscii-8, the other built-in font,** which is exactly half of unifont's height. Not chosen: it is another font with another design, and the maintainer asked for a smaller size "from the same font".
- **Take two thirds exactly from a smaller size, or nothing.** Not chosen: Terminus's heights never give two thirds of another, so the maintainer's suggestion would never apply to it.
- **Let a superscript reach the row above, as other glyphs may (ADR-1306 D1).** Not chosen: it would draw over the descenders of that row, and the box already keeps the glyph in its row.
