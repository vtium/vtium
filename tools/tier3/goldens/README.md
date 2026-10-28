# vtium's tier 3 goldens

The goldens of the small displays (vtium/vtium#75), which the C reference never drew: vtium's captures from `make tier3-minimal` at 5d3bbb6. The operator reviewed and accepted them on 2026-10-28, and accepted in advance the same day the goldens vtium/vtium#119 moves or adds within its bounds, so they judge; a golden of vtium's that nobody has reviewed yet goes in `../provisional.tsv` first. Each variant draws `SMALL_SCREENS` with a test font from `tools/testfont` and a shell prompt and steady cursor after the screen; `vt-restore` is compared with `cjk.png`. The fonts hold no CJK, so every CJK character is drawn as `?` across its two cells, vtium's missing glyph in a font that lacks U+25A1. When vtium loads fonts as vitrified sets with fallbacks (vtium/vtium#126), those cells change to the fallback's glyphs and these goldens move in their own commit.

| Golden | What it shows |
|---|---|
| `2d-320x240-psf8/small-console.png` | 320x240, 8x8 cells, 40 by 30. The ruler ending `40` in the last column; `bin` and `etc` bold blue, `run.sh` bold green, `notes.txt`, `lib.so` bold cyan; the eight background colors and the eight bright ones, four cells each; the six color names in their colors; `bold`, `faint` in grey, `under` underlined, `reverse` reversed and `struck` struck; a three-column box of sprite lines around `cpu`, `mem`, `load` and `12%`, `48M`, `0.08`; Latin-1; `double`, `curly`, `dotted` and `dashed` under their lines in each cell's bottom row, the double and curl drawn as the single line (vtium/vtium#119) and each dash from its cell's left edge for five of its eight pixels; a bar of 16 green full blocks and 16 light shades, then `50%`. |
| `2d-320x240-psf8/sgr.png` | The `sgr` screen wrapped at 40 columns: the `fgNN` and `bgNN` labels in their colors over three rows each, the styled words over two, the 256-color cube as six bands of two rows, and the 24-bit ramp over two rows. |
| `2d-320x240-psf8/boxdraw.png` | The `boxdraw` screen: a light box with a divider around its two text rows, a double box of two cells, a rounded box, shades, quadrants and eighths, all sprites at 8x8. |
| `2d-320x240-psf8/underline-styles.png` | The `underline-styles` screen wrapped at 40 columns: each style's label and its underlined text, the lower rows wrapping, the colored, bold, reversed and blue-background lines, and `?` for each wide character. The double and curly lines draw as the single line in each cell's bottom row (vtium/vtium#119), and each dash runs from its cell's left edge for five of its eight pixels. |
| `2d-320x240-psf8/underline-colour.png` | The `underline-colour` screen wrapped at 40 columns: each row's white text over a line in its SGR 58 color, red, sky blue or yellow, the curly ones drawn as the single line; green text over a magenta line; a green line that turns red after SGR 59; and no line for 58 alone. (vtium/vtium#119) |
| `2d-320x240-psf8/strike.png` | The `strike` screen wrapped at 40 columns: a line through the middle of each struck run, in white, red and bold, over an underline and a red line, in reverse, across the `?` of the wide characters, and stopping at SGR 29. (vtium/vtium#119) |
| `2d-320x240-psf8/wide-margin.png` | The `wide-margin` screen at its real edge, column 40: rulers ending in the last column; case 1's `?` in the last two columns; cases 2, 3 and 5 wrapped whole to the next row; case 4's dashes ending one column short of the edge, its `?` in the last two. |
| `2d-320x240-psf8/cjk.png` | The `cjk` screen: `Latin then CJK:` and `end` with four `?` between them, the Japanese and Korean rows as `?`, and the bars of the last two rows aligned. |
| `2d-640x480-psf16/small-console.png` | 640x480, 8x16 cells, 80 by 30: the `small-console` screen in the left half, its ruler ending at column 40, the decorations below their letters. |
| `2d-640x480-psf16/sgr.png` | The `sgr` screen at 80 columns: the labels over two rows each, the cube as six whole rows, and the ramp on one. |
| `2d-640x480-psf16/boxdraw.png` | The `boxdraw` screen at 8x16. |
| `2d-640x480-psf16/underline-styles.png` | The `underline-styles` screen at 80 columns, every line on its own row. |
| `2d-640x480-psf16/underline-colour.png` | The `underline-colour` screen at 80 columns, every line on its own row, the curly lines as waves in their colors. (vtium/vtium#119) |
| `2d-640x480-psf16/strike.png` | The `strike` screen at 80 columns, every line on its own row. (vtium/vtium#119) |
| `2d-640x480-psf16/wide-margin.png` | The `wide-margin` screen at column 80, its cases as at 320x240. |
| `2d-640x480-psf16/cjk.png` | The `cjk` screen at 8x16. |

The goldens of the guest without systemd (vtium/vtium#104, `../README.md`, The guest without systemd) are vtium's captures from `make tier3-nosystemd` on the branch that added it, each pixel for pixel the reviewed `2d-640x480-psf16` golden of the same screen, as the same display, configuration and font draw it under either init. They stay in `../provisional.tsv` until the operator reviews them.

| Golden | What it shows |
|---|---|
| `sysvinit-640x480/sgr.png` | The `sgr` screen at 80 columns, drawn by the console the inittab started on tty2, with the udev daemon running. |
| `sysvinit-640x480/cjk.png` | The `cjk` screen at 8x16, the same console. |
| `sysvinit-no-udev/sgr.png` | The `sgr` screen, with no udev daemon: the card and the keyboard found in sysfs alone. |
| `sysvinit-no-udev/cjk.png` | The `cjk` screen, with no udev daemon. |
| `sysvinit-two-vts/cjk.png` | The `cjk` screen, drawn by tty2's console with a second console started on tty3 at the boot (vtium/vtium#139). |

## Recaptured with the sets (vtium/vtium#126)

vtium draws only vitrified sets, so the small displays draw sets the guest builds before the console starts: the 8x8 test font with the built-in Unscii-8 after it, and the 8x16 one with unifont after it (`../guest/frits`). The sprites win over a fill font's glyphs, so box drawing, blocks, shades and Braille draw as before. Eight goldens moved, where a character the test font lacked, `?` before, now draws as Unscii-8's U+25A1, the missing glyph, at 8x8, since Unscii-8 holds none of them, and as unifont's glyph at 8x16. They were recaptured from `make tier3-minimal` and are listed in `../provisional.tsv` until the operator reviews them; the table's descriptions of those eight are the goldens before. The rows and columns that moved, counted from 1, with the pixels that differ:

- `2d-320x240-psf8/cjk`: 560 px; rows 1: cols 17-24; 2: cols 1-16,20-25; 3: cols 2-3,5-6,8-9,11-12,14-15
- `2d-320x240-psf8/strike`: 115 px; rows 9: cols 20-23,29-34
- `2d-320x240-psf8/underline-styles`: 140 px; rows 18: cols 16-19,25-30
- `2d-320x240-psf8/wide-margin`: 140 px; rows 3: cols 39-40; 6: cols 1-2; 8: cols 1-2; 10: cols 39-40; 13: cols 1-2
- `2d-640x480-psf16/cjk`: 1034 px; rows 1: cols 17-24; 2: cols 1-16,20-25; 3: cols 2-3,5-6,8-9,11-12,14-15
- `2d-640x480-psf16/strike`: 292 px; rows 7: cols 20-23,29-34
- `2d-640x480-psf16/underline-styles`: 305 px; rows 14: cols 16-19,25-30
- `2d-640x480-psf16/wide-margin`: 295 px; rows 2: cols 79-80; 4: cols 1-2; 6: cols 1-2; 7: cols 79-80; 10: cols 1-2
