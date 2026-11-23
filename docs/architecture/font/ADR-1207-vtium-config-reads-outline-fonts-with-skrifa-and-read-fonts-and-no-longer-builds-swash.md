---
contract: adr/v1
kind: decision
verb: change
capability: glyphs
basis:
  - operator: aaronsb
    level: directed
    said: "swash is basically not active anymore"
    via: the maintainer in the steering session, 2026-11-22, recorded in the steering session's decisions for that day, after relaying that swash's maintainers have moved to skrifa and harfrust
  - operator: aaronsb
    level: directed
    said: "any sort of library organizing would come after removal of swash"
    via: the maintainer in the steering session, 2026-11-22, recorded in the same decisions
  - evidence: "Read by this record's agent on 2026-11-22 at integration 56261ff70: every use of swash and zeno in vtium-config. swash 0.2.10 is a dependency of vtium-config alone, and vtium-config reaches zeno 0.3.3 only through swash's re-export `swash::zeno`. vtium-config already draws every outline through skrifa 0.44 and hints it there (src/outline.rs, `Drawer::path`), reads color bitmap strikes with read-fonts 0.41 and the png crate (src/outline/strike.rs), and shapes with harfrust 0.12. What still comes from swash is the face and collection reader (`names::swash_fonts`), the metrics, the advances, the character map, the names, raw table lookups, COLR version 0 layers (`ScaleContext::scale_color_outline`), the W rule's ink measure (`Render`), the image types the drawing code passes around (`Image`, `Content`, `Source`), and through zeno the path type, the bounds, the transform and the mask filler."
  - evidence: "Read by this record's agent on 2026-11-22 from the sources of swash 0.2.10, skrifa 0.44.0, read-fonts 0.41.0 and zeno 0.3.3. swash's metrics take OS/2's typographic metrics when its USE_TYPO_METRICS flag is set and the typographic ascent is not 0, else hhea's. skrifa's take OS/2's typographic metrics whenever the flag is set, else hhea's, and when hhea's ascent and descent are both 0 it takes the typographic metrics and then the Windows metrics, as FreeType does. skrifa's descent is negative where swash's is positive. skrifa scales an advance in 16.16 fixed point, where swash multiplies in f32. skrifa's character map picks a symbol subtable first and else the best Unicode one, and maps a codepoint to a 32-bit glyph number; swash adds a format 12 group's glyph number in 16 bits. skrifa's ColorGlyphCollection::get prefers a COLR version 1 glyph to a version 0 one. zeno sizes its mask from the bounds of the path's points, control points included, rounded out to whole pixels, snaps each point to 1/256 of a pixel by truncation, flattens a quadratic within a quarter of a pixel and a cubic within half a pixel, and gives coverage in 256ths of a pixel, truncated, with a full pixel held at 255."
  - evidence: "Measured by this record's agent on 2026-11-22 at integration 56261ff70, in a scratch copy of the tree that was never committed. The copy routes vtium-config's two mask fills, an outline's and a COLR layer's, through one function that an environment variable switches between three fillers. All three fill the box zeno sizes, so only coverage can differ. The fillers are zeno as vtium-config uses it; tiny-skia 0.12's anti-aliased path filler into its 8-bit Mask, nonzero winding, the version resvg already builds into vtium-config; and a coverage rasterizer of about 200 lines in the font-rs style, which adds each line's signed area to an accumulation buffer and sums each row. That rasterizer flattens curves to within 0.05 pixel, snaps points and maps coverage as zeno does. `tier2 render` drew all 296 of tier 2's screens with each filler; the zeno run matches every golden. tiny-skia moves 132 screens and 503,692 pixels, by at most 32 levels of 255 on DejaVu Sans Mono text, 53 on Noto Sans CJK, and 115 on the ink test font's straight edges. The font-rs rasterizer moves 125 screens and 181,967 pixels, by at most 12 levels on DejaVu Sans Mono text, 38 on Noto Sans CJK and 2 on the ink test font. On a 1-bit display or under `render=bitmap`, a moved pixel flips between 0 and 255: tiny-skia flips 7,179 pixels of overflow-accents-1bit, the font-rs rasterizer 83. The same rasterizer with zeno's own flattening rule still moves 118 screens and 102,133 pixels, so only zeno's fixed-point arithmetic reproduces zeno's pixels."
  - evidence: "Measured by this record's agent on 2026-11-22 in the same copy, with an ignored test built with `--release`. Each filler drew DejaVu Sans Mono's 94 printable ASCII glyphs and 15 ideographs of Noto Sans CJK SC, unhinted, at 16 and 32 pixels, 2,000 times. The reference is the font-rs rasterizer with curves flattened to within 0.0005 pixel, the coverage of the exact outline under zeno's edge rules. At 16 pixels on DejaVu Sans Mono, zeno is off the reference by 1.18 levels on average and at most 16, tiny-skia by 5.90 and at most 49, and the font-rs rasterizer by 0.92 and at most 12. On Noto Sans CJK at 32 pixels the figures are 0.77 and 35, 4.90 and 45, and 0.32 and 12. A glyph's fill takes 0.56 µs with zeno, 2.50 µs with tiny-skia and 0.94 µs with the font-rs rasterizer for DejaVu at 16 pixels, so the ascii screen's 94 glyphs fill in 53, 235 and 88 µs. For Noto Sans CJK at 32 pixels a glyph takes 1.85, 7.89 and 3.25 µs. Ryzen 9 9950X3D, one core."
  - evidence: "Measured by this record's agent on 2026-11-22 in the same copy: `cargo test -p vtium-config --all-features --no-fail-fast` with each filler. Under zeno every test that bears on drawing passes. Under the font-rs rasterizer one test fails, `only_the_separators_range_is_placed_as_separators`, which compares digests of drawn glyphs. Under tiny-skia ten fail, among them the tests of a glyph's ink box, of hinting, of a separator's edges and of a double-size ask, because tiny-skia's coverage at a glyph's edges differs from the rules those tests pin."
  - precedent: ADR-1200
  - precedent: ADR-1301
  - precedent: ADR-1503
amends:
  - "ADR-1200#D14: parsing and rasterizing safely"
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-22
deciders:
  - aaronsb
related:
  - ADR-1200
  - ADR-1301
  - ADR-1503
---

# ADR-1207: vtium-config reads outline fonts with skrifa and read-fonts and no longer builds swash

## Summary

- **Decided by the maintainer (2026-11-22):** swash is unmaintained, so vtium-config stops building it. The library work on vtium's renderer waits until swash is gone.
- **Proposed by this record's agent, for the maintainer to confirm:** vtium-config reads each face, its metrics, advances, character map and names with read-fonts and skrifa (D1). It already draws outlines with them. COLR color glyphs are read through skrifa's color painter (D2). The font check keeps only what read-fonts itself still needs (D4). The work runs as three cards (D5).
- **Left to the maintainer:** which code fills a glyph's outline into pixels (D3). The choices are zeno kept as a direct dependency, tiny-skia's path filler, or a small coverage rasterizer of vtium's own.
- **Trades away:** Under tiny-skia or vtium's own rasterizer, every outline-font picture moves once by a few levels of gray and goes to the maintainer's picture sheet. Under vtium's own, vtium maintains and fuzzes the rasterizer itself.
- **One-way?** No. The rasterizer sits behind one function, and the reader change keeps today's cells and baselines.
- **Probes:**
  - *Confident (metrics):* a font whose `hhea` table gives no ascent or descent now draws from its `OS/2` metrics, as FreeType draws it, where vtium-config refused it before. Is that what you want?
  - *Not confident (the rasterizer):* lean, vtium's own rasterizer. See [Open questions](#open-questions).

## Context

swash is a Rust crate that reads, scales and rasterizes fonts. Its maintainers moved their work to skrifa and read-fonts, which read fonts, and to harfrust, which shapes text. vtium-config already builds all three. It draws and hints every outline through skrifa, reads color bitmap strikes with read-fonts, and shapes with harfrust. swash still reads faces, metrics and the character map, and draws COLR version 0 layers. Through zeno, it also fills each path into a mask.

swash has also shaped vtium-config's font check. `names::sound` refuses a font in each case where swash 0.2.10 overflows or panics. Those are a table past the end of the file, a broken `name` table, and a format 12 character map that maps past U+10FFFF. They are also a face with no glyph widths, a descender of -32768, and four kinds of broken `HVAR` table. read-fonts checks each offset and count it reads and returns an error instead, so most of those refusals have no reason left. One refusal comes from read-fonts itself: a `glyf` glyph with no contours that still gives a point flag makes read-fonts 0.41 write past the end of its flags. The 4,096-pixel glyph bound comes from zeno, which sizes its mask with 32-bit arithmetic, and from the helper's memory limit, which any rasterizer's mask must stay under.

zeno comes from swash's author and is as inactive. The maintainer filed two zeno issues and five swash issues upstream on 2026-11-22.

## Decision

### D1: faces, metrics and the character map come from read-fonts and skrifa

- **Faces.** vtium-config opens a font file or collection with read-fonts and reads at most `FACES_MAX` faces of a collection, as it does now. The font check runs first, on every path that opens a face.
- **Metrics.** vtium-config takes a face's ascent, descent and units per em from skrifa's metrics in font units. skrifa follows FreeType's rule: `OS/2`'s typographic metrics when the face sets USE_TYPO_METRICS, `hhea`'s otherwise, and when `hhea` gives 0 for both, the typographic and then the Windows metrics. A face that sets the flag but gives a typographic ascent of 0 takes that ascent, where swash took `hhea`'s.
- **Advances.** vtium-config reads each advance in font units and scales it in f32, as it scales it now. skrifa's own scaled advance uses 16.16 fixed point, which can round a W's advance to another whole pixel near a half and change a cell's width.
- **Character map.** vtium-config maps characters through skrifa's character map. A character that maps to a glyph number past 65,535 counts as unmapped, because the glyph helper's glyph numbers are 16 bits. The list of mapped characters keeps its one-bit-per-codepoint set, so a character map that lists the same range many times still costs one entry for each codepoint.
- **Names and tables.** `names.rs` reads every name a face gives: its family, style, version and license. Its reader of the `name` table is the one the search for a family already uses, so one reader answers every name. A raw table such as `post` or `OS/2` is read through read-fonts.

### D2: COLR color glyphs are read through skrifa's color painter

vtium-config reads a COLR glyph with skrifa's `ColorGlyphCollection`, asking for the version 0 form. skrifa's color painter hands vtium-config each layer as a glyph to clip to and a solid color to fill it with. vtium-config fills each layer's outline, unhinted, in the face's first `CPAL` palette and lays the layers over each other premultiplied, as it does now. A layer in the text's color takes mid gray, as now. A glyph that has only a version 1 form keeps drawing from its outline, as now. Drawing version 1 glyphs in color, with their gradients, transforms and clip boxes, is a feature of its own for later.

### D3: one function fills an outline, and the maintainer picks it

vtium-config fills every outline and every COLR layer through one function that takes a path and returns the glyph's coverage and its box. The box is today's: the path's points, control points included, rounded out to whole pixels. A glyph whose box is wider or taller than 4,096 pixels draws as empty, as now. The bound keeps every mask within 4,096 × 4,096 pixels, under the helper's memory limit. The W rule's ink measure goes through the same function.

The function's filler is one of three. The maintainer picks it (Open questions, question 1).

- **zeno, as a direct dependency.** Its pixels stay byte for byte, and no picture moves.
- **tiny-skia's path filler.** resvg already builds tiny-skia into vtium-config to draw pointer files, so it adds no crate. tiny-skia samples four rows within each pixel, which moves every outline picture and puts its error at horizontal edges, up to 115 levels on straight edges. Ten of vtium-config's tests pin edge rules that it breaks.
- **A coverage rasterizer of vtium's own,** about 200 lines in vtium-config. It accumulates each line's signed area into a buffer and sums each row, so its coverage is the area each pixel covers. It flattens curves to within 0.05 pixel and keeps zeno's edge rules: points snapped to 1/256 of a pixel, and coverage in 256ths with a full pixel at 255. Every outline picture moves by a few levels, at most 12 on DejaVu Sans Mono and 38 on Noto Sans CJK. Its coverage lies closer to the exact coverage than zeno's.

### D4: the font check keeps what read-fonts needs

`names::sound` keeps the check of `glyf` glyphs that give point flags without points, until read-fonts no longer writes past their flags. It keeps the cap of `FACES_MAX` faces, which bounds the work a collection can ask for. Its other checks exist because swash overflows, so they go once swash is gone. Those are the table directory's bounds, the `name` table's records, the format 12 subtable, the glyph widths, the descender of -32768 and the `HVAR` table. read-fonts returns an error for a damaged table, and vtium-config treats that table as absent. The names fuzz target drives every read-fonts and skrifa call that vtium-config makes on a face, in place of the swash calls it drives now.

### D5: the work

The move runs as three cards, one after another, because vtium-config has one owner at a time.

1. **Reading.** D1, D4's reader changes, and the image and path types that the drawing code passes around become vtium-config's own. Afterwards, swash is used only to draw COLR layers and the W rule's ink, and zeno only to fill.
2. **Drawing.** D2 and D3. Afterwards, nothing in vtium-config's source names swash, and zeno only when the maintainer keeps it.
3. **Removal.** D4's checks retired, the fuzz targets retargeted, swash removed from both lock files, and the crate's README and the features page rewritten.

## Open questions

1. **Which filler draws outlines?** Lean, which stands as the lean and not as the maintainer's answer: vtium's own rasterizer. For: it removes the last crate of an inactive author, and vtium already pins the edge rules it follows in its tests. It is closer to the exact coverage than zeno on every font measured. It adds no crate, and it fills a glyph in under a microsecond at 16 pixels, a small part of a glyph's cost. Against: every outline picture in tier 2, tier 3 and the specimens moves once by a few levels of gray, 125 of tier 2's 296 screens. vtium must also fuzz and maintain the rasterizer itself. It fills a glyph about 1.7 times slower than zeno. If the pictures should not move yet, the reading card can land with zeno as a direct dependency. The drawing card then swaps the filler later, with one picture sheet.

## Consequences

### Positive

- vtium-config reads fonts through one maintained reader, the one its outlines, hinting, strikes and shaping already use, and swash leaves the build.
- Most of the font check's refusals go, so fonts that read-fonts can read are no longer refused for what swash could not read.
- The library work on vtium's renderer can start once the removal card lands.

### Negative

- Under tiny-skia or vtium's own rasterizer, every outline-font golden moves once and the maintainer reviews them.
- A font whose `hhea` table gives no metrics now draws, from its `OS/2` metrics, so the tests that expected it refused change.

### Neutral

- The glyph bound, the strike reader and harfrust's shaping are unchanged.
- COLR version 1 glyphs still draw from their outlines.

## Alternatives Considered

- **Keep swash.** Not taken: its maintainers have moved to the crates vtium-config already builds, and the maintainer treats it as unmaintained.
- **Port zeno's rasterizer into vtium-config.** Its pixels would stay, since only zeno's fixed-point arithmetic reproduces them. Not taken as a fourth choice: it keeps zeno's coarser curves, and the code is about four times the size of the font-rs rasterizer.
