# specimens

The demonstration set (vtium/vtium#142): vtium's test card, a byte stream written by Rust code here, drawn by vtium's renderer in memory for each display profile into `docs/specimens/`, and judged against what is committed there. It is a verification article as well as a gallery: each profile shows what vtium draws on that display in the font it chooses there.

```
make specimens-fonts            # fetch the outline profiles' pinned fonts into the cache, once
make specimens                  # draw every file in docs/specimens from the profile table
make specimens REVIEW=DIR       # and copy each profile's collage, or its one screen, to DIR/PROFILE.png
make specimens-check            # draw them again: an accepted profile that moved fails
target/release/specimens card PROFILE [PAGE]   # a page of the card's bytes, for `cat` on a VT of that size
```

`cargo test --workspace` runs the check too, through `tests/committed.rs`, so the gate line judges the specimens; it gives no vtium-vitrify, so it cannot draw the outline profiles, which may go undrawn only while they are provisional; `make specimens-check` draws them. A PNG is judged by its pixels and a text by its bytes; a file under `docs/specimens/` the table does not draw is a fault, as is a missing one, and the walk lists a symbolic link without following it. `provisional.tsv` there, which the operator keeps, lists the profiles not yet reviewed. A fault in an accepted profile fails; one in a provisional profile is drift, printed and failing nothing (the operator, 2026-10-28). `check` exits 0 when no accepted profile moved, 1 when one did, and 2 when it cannot run. `docs/specimens/` is drawn only by `make specimens`, but for its hand-written `provisional.tsv`: a branch that moves specimens regenerates them in a commit of their own, listing what moved, and a conflict there resolves by `make specimens-fonts && make specimens`.

| Path | What it is |
|---|---|
| `src/card.rs` | The card: five blocks of 40x30 cells, each written from its own top left with CUP. |
| `src/profiles.rs` | The profile table, the gray panels' conversion, and the layout of the card's blocks on a grid. |
| `src/fonts.rs` | The outline fonts, pinned by URL and SHA-256, their cache, and the desktop set vtium-vitrify builds from them. |
| `frits/desktop.vtfrit` | The desktop set's frit: the pinned fonts, then unifont, at 24 and 32 pixels. |
| `src/draw.rs` | A page through `vtium-term` and one frame of `vtium-render` into a canvas, and the collage. |
| `src/png.rs` | The PNG writer and reader: 8-bit RGB, deflated with `miniz_oxide`, which vtium-font already links. |
| `src/docs.rs` | What `docs/specimens/` holds: the screens, collages, READMEs and vtium.conf files, drawn from the table. |
| `src/check.rs` | Writing `docs/specimens/`, and judging it. |
| `tests/committed.rs` | The committed specimens against a fresh drawing. |

## The card

A block is one 320x240 screen at 8x8 cells (#118). A page holds as many blocks as its grid fits, across then down, centered; the card takes as many pages as it needs, and a profile of several pages shows them together in a collage. Every line ends with SGR 0, and the unit tests check that each block draws the same cells on every profile's page as on a 40x30 grid of its own.

1. **Text:** a header naming the profile (display, depth, cell, grid, set, display class), then ASCII, Latin-1 and Latin Extended, Greek, Cyrillic, combining marks beside their precomposed letters, CJK, Hangul and fullwidth forms in two cells, and punctuation, arrows, maths and currency.
2. **Attributes:** bold, dim, italic, reverse, conceal, strike and their combinations; the resets SGR 22, 23, 24, 25, 28 and 29, one a word; the five underline styles; each in an SGR 58 color, in the semicolon and colon forms; underline, strike and colors together, with SGR 59 putting an underline back in the text's color and SGR 21's double underline.
3. **Color:** the 16 colors as foreground, bold and background; the 256-color cube, the 16 and the grays; 24-bit hues as backgrounds, as foreground over background in half blocks, and per letter; reverse and dim over color, and SGR 39 and 49; then the grays a panel's levels and conversion show on: the 24 grays 232-255 and a 24-bit ramp from black to white in 40 steps as backgrounds, the 24 grays as text on black and on white, and a 24-bit ramp of 20 steps as text on black and on white.
4. **Sprites and symbols:** light, heavy, double and rounded boxes and their junctions, block elements, eighths, quadrants and shades, a Braille wave and a Braille sample, sextants, octants and the rest of Symbols for Legacy Computing's block shapes; and Nerd Font private-use symbols.
5. **Emoji and scripts:** color emoji single and with skin-tone modifiers, ZWJ sequences of a family, professions, a rainbow flag and a polar bear, regional-indicator flags and a tag-sequence flag, keycaps, and VS15 against VS16; then UTF-8 samples labelled with their bytes a character and their script: Latin with stacked combining marks, Armenian, Hebrew and Arabic in two bytes; polytonic Greek, Devanagari, Tamil, Thai, Georgian, Ethiopic and conjoining Hangul jamo in three; CJK Extension B, mathematical alphanumerics, Gothic, cuneiform and Legacy Computing's digits and symbols in four. Each is drawn as vtium draws it today: a set maps single codepoints and nothing is shaped, so a sequence draws as its first character and the scripts that need shaping draw unshaped, the baseline #144's shaping moves. Last, a wide character written where only the grid's last column is left, which wraps whole to the next row.

Every escape the card uses is one ADR-1000 lists as supported. What it leaves out:

| Left out | Why |
|---|---|
| Blink, SGR 5 and 6, but where the resets row sets SGR 5 for SGR 25 to clear | A still frame shows only the on phase. |
| Overline, SGR 53 | vtium-render draws nothing for it until #37. |
| Superscript and subscript, SGR 73-75 | vtium-render draws them as plain text until #37. |
| Double-width and double-height lines, DECDWL and DECDHL | vtium-term keeps a line's size and vtium-render draws every line single until #61. |
| RGBA color, SGR 38:6, 48:6, 58:6 | `ignored` in ADR-1000. |
| SGR 10-20, 26, 50-52, 54, 60-65, 221, 222 | `ignored` in ADR-1000. |
| Left and right margins, DECSLRM | Not built (#55); each block's lines are placed with CUP instead. |
| Bidirectional text, kitty text sizing, pictures | Built in vtium-term or not yet at all (#43, #60, #30, #48, #49), and not drawn by vtium-render. |

Conceal, SGR 8, is on the card though vtium-render draws its text until #37 hides it; the specimens move then. A bitmap set draws a cluster as its first character, so combining marks show their base letter beside the precomposed letter; and at 8x8 a double or curly underline draws as the single line (vtium-render's README). Those are what vtium draws today, and the card shows them.

## The profiles

Each profile draws with the set vtium chooses for its display at the height `font-size=0` picks with #118's rule: a built-in set (ADR-1200 D7) up to 1366x768 and on the 1-bit panel, whose cells vtium-render maps to lit or unlit (#135), and the desktop set at 1920x1080 and 2560x1440 (D11). Its README in `docs/specimens/` says why that is the best choice there.

vtium ships no third-party font bytes, so the desktop set's fonts are pinned in `src/fonts.rs` by URL and SHA-256, as tier 3 pins its guest images: JetBrains Mono 2.304, Symbols Nerd Font Mono 3.4.0 and Noto Color Emoji 2.051, each from its project's tagged release on GitHub. `make specimens-fonts` fetches them with curl into `$XDG_CACHE_HOME/vtium/specimens/fonts` and checks each hash; the frit repeats each hash as its `sha256=`, and a unit test holds the two lists equal. `make specimens` and `make specimens-check` give `specimens` the vtium-vitrify they build, which runs `create --out` on the frit with the cache as its only font directory, into a directory it creates beside the cache, refusing one that is there; a set built from only some of its sources is refused. Without the tool or a pinned font of the pinned bytes, an outline profile that is provisional is skipped, with why, and its committed screens and collage are neither judged nor strays; an accepted one fails, with why, since a skip never hides an accepted profile's regression. The gray panel profiles, `320x240-gray4`, `320x240-gray16` and `640x480-gray16`, are simulations: vtium draws a gray panel exactly as color and the panel's driver converts each pixel (#135), so their screens are the color drawing taken through that driver's arithmetic as Linux v7.3 makes it (`Panel` in `src/profiles.rs`, with each line cited), each level drawn as the gray it is on a panel of evenly spaced levels. They live in `docs/specimens/simulated/`, say in their header and README that they are simulations, and judge nothing of vtium's drawing: `check` reports a stale or stray one as its own kind of fault, apart from the specimens, failing once the simulation is accepted, as the three are, and as drift while it is provisional; `make specimens` redraws them with the rest. vtium ships no PSF font, so no profile draws one: the 8x8 and 8x16 PSF fonts tier 2 builds hold ASCII test glyphs, which no display's best configuration would choose.

## How it draws

A page is drawn as tier 2 draws a screen (`tools/tier2/src/draw.rs`): the cursor hidden, the bytes through `vtium-term` with the Unicode width method, and one frame through `vtium-render` with its default configuration, but the profile's `display-depth`, into a canvas of the display's size, at age 0. The cell the set draws must be the one the profile table gives, which the READMEs and the layout are written from. Tier 2 already checks that the cache, the profiles and the glyph storage never move a pixel, so the specimens draw once.
