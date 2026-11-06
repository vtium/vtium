# vtium's tier 3 goldens

The goldens of the small displays (vtium/vtium#75): vtium's captures from `make tier3-minimal` at 5d3bbb6. The operator reviewed and accepted them on 2026-10-28, and accepted in advance the same day the goldens vtium/vtium#119 moves or adds within its bounds, so they judge; a golden of vtium's that nobody has reviewed yet goes in `../provisional.tsv` first. Each variant draws `SMALL_SCREENS` with a test font from `tools/testfont` and a shell prompt and steady cursor after the screen; `vt-restore` is compared with `cjk.png`. The fonts hold no CJK, so every CJK character is drawn as `?` across its two cells, vtium's missing glyph in a font that lacks U+25A1. When vtium loads fonts as vitrified sets with fallbacks (vtium/vtium#126), those cells change to the fallback's glyphs and these goldens move in their own commit.

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

The goldens of the frit variants (vtium/vtium#145, `../README.md`, Variants) are vtium's captures from `make tier3-minimal`: 640x480, the font the frit `dejavu-mono` generated at run time by the helper (ADR-1301 D2). The frit states no cell, so `2d-640x480-frit` draws in DejaVu Sans Mono's own cell, 9x16, 71 by 30 (ADR-1200 D17), recaptured at e8560705; `2d-640x480-frit-held`, whose helper never answers, draws in the built-in font's 8x16 cell, 80 by 30, as captured on the branch that added them. They stay in `../provisional.tsv` until the operator reviews them.

| Golden | What it shows |
|---|---|
| `2d-640x480-frit/small-console.png` | The `small-console` screen once the helper's glyphs have arrived: its letters, digits and Latin-1 in DejaVu Sans Mono, antialiased, in 9x16 cells, so its 40-column ruler ends at x 360; the box lines, blocks and shades as the sprites. |
| `2d-640x480-frit/cjk.png` | The `cjk` screen in the same frit and cell: `Latin then CJK:`, `end`, `ASCII` and the bars in DejaVu Sans Mono, and the Chinese, Japanese and Korean characters, which the font lacks, in the built-in unifont, each centered in its two 9-pixel cells (vtium/vtium#141). |
| `2d-640x480-frit-held/small-console.png` | The `small-console` screen with a helper that never answers: the screen before the helper's first answer, all in the built-in unifont, pixel for pixel the reviewed `2d-640x480-psf16/small-console.png`. |
| `2d-640x480-frit-held/cjk.png` | The `cjk` screen with a helper that never answers, all in the built-in unifont, pixel for pixel `2d-640x480-psf16/cjk.png`. |

The goldens of the device variants (`../README.md`, Outputs and keyboards) are vtium's captures from `make tier3 VARIANTS="two-outputs-mirror two-cards-mirror usb-keyboards"` at 7da1d7a: 640x480, 8x16 cells, 80 by 30, in the 8x16 test font with unifont after it. Each is pixel for pixel the `2d-640x480-psf16` golden of the same screen, as vtium draws one grid on every output today. They stay in `../provisional.tsv` until the operator reviews them.

| Golden | What it shows |
|---|---|
| `two-outputs-mirror/sgr.png`, `two-outputs-mirror/sgr@1.png` | The `sgr` screen on the first and the second output of one card. |
| `two-outputs-mirror/cjk.png`, `two-outputs-mirror/cjk@1.png` | The `cjk` screen on each output; `vt-restore` and `vt-restore@1` are compared with them. |
| `two-cards-mirror/sgr.png`, `two-cards-mirror/sgr@card1.png` | The `sgr` screen on the first card and the second. |
| `two-cards-mirror/cjk.png`, `two-cards-mirror/cjk@card1.png` | The `cjk` screen on each card. |
| `usb-keyboards/cjk.png` | The `cjk` screen with a USB keyboard in the port the example rule names. |

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

The goldens of the head-file variants (`../README.md`, Head files) are vtium's captures from `make tier3` with those variants at a717d83: 640x480, the console's first screen of each head file and the `cjk` screen of the two that give a login, in the 8x16 test font with unifont after it. They stay in `../provisional.tsv` until the operator reviews them. Each first screen was the same, pixel for pixel, on three runs.

| Golden | What it shows |
|---|---|
| `heads-two-heads/cjk.png` | The `cjk` screen on the head desk, the first output; the second output is the head panel, with its own login prompt. Pixel for pixel `2d-640x480-psf16/cjk.png`. `vt-restore` is compared with it. |
| `heads-partly-open/partly-notice.png` | A red-bordered notice over the top two rows on a black screen: `config partly invalid, running what's valid`, then `Run vtium check-config /etc/vtium/vtium.conf to see why.` The notice covers the row of the login prompt. Recaptured from `make tier3` at 127b2ca3, where vtium reads its heads from vtium.conf. |
| `heads-partly-open/cjk.png` | The `cjk` screen, typed into the surviving head after the notice. Pixel for pixel `2d-640x480-psf16/cjk.png`. `vt-restore` is compared with it. |
| `heads-closed-broken/panic.png` | The panic screen in a red-bordered box over the top thirteen rows: `vtium: config invalid, policy closed: input off`; that the errors in `/etc/vtium/vtium.conf`, listed below, leave no head that can run; the errors of lines 13, 14 and 16, each wrapped at 80 columns, line 14's saying that a head's section cannot set `font-size` and to set it in the `[look NAME]` section the head names or at the top of vtium.conf; `More errors: 1`; and that nothing on this console takes input, with the `vtium check-config /etc/vtium/vtium.conf` command for the full report. The rest of the screen is black, with no login prompt. Recaptured from `make tier3` at 127b2ca3: the head file's lines sit below the variant's nine lines of keys in the one file. |
| `heads-policy-typo/policy-panic.png` | The panic screen of a vtium.conf whose policy line reads `policy = opne` below `version = 2`, captured from `make tier3` at d2fcad6a, in a red-bordered box over the top nine rows: `vtium: config invalid, policy closed: input off`; that the errors in `/etc/vtium/vtium.conf`, listed below, leave no head that can run; the error of line 18, that `policy` cannot be "opne", listed first, then the error of line 17, that vtium knows only `version = 1`, which comes before it in the file, each wrapped at 80 columns; and that nothing on this console takes input, with the `vtium check-config` command for the full report. The rest of the screen is black, with no login prompt. |
| `heads-kiosk/cjk.png` | The `cjk` screen, typed through the USB keyboard that claims the closed kiosk's head, captured from `make tier3` at 043fcc7. Pixel for pixel `2d-640x480-psf16/cjk.png`. |
| `heads-closed-unmatched/no-display-notice.png` | A red-bordered notice over the top two rows on a black screen: `vtium: no display for any head`, then `Connect a display that a rule in /etc/vtium/vtium.conf sends to a head.` Recaptured from `make tier3` at 127b2ca3. |

The goldens of the images variant (`../README.md`, Pictures) are vtium's captures from `make tier3-images` at 755b04a7, the commit before them: 640x480, 8x16 cells, 80 by 30, in the 8x16 test font with unifont after it, the console built with images. The pictures are `../t3_pictures.py`'s, and `PictureGoldenTest` in `../test_t3.py` finds each one in its golden, pixel for pixel, at the cell its screen places it. They stay in `../provisional.tsv` until the operator reviews them.

| Golden | What it shows |
|---|---|
| `images-640x480/img-sixel.png` | A sixel picture of red, green, blue and yellow quarters in a white frame at row 3, column 5, 8 cells by 3 rows; an orange ring around a blue disc, 6 cells by 3, at column 44, over three rows of dots that show at its clear corners; and the line `After the first picture the cursor was at row 5, column 5.` |
| `images-640x480/img-kitty.png` | The quarters as a kitty picture at row 3, columns 5 and 30, with `<cursor` right of the second's bottom row; its yellow quarter alone, 4 cells wide and a row and a half tall, at row 8, column 5; and the whole picture scaled to 16 cells by 6 rows at row 8, column 30, its edges blended. |
| `images-640x480/img-relative.png` | An orange and violet parent at row 4, column 3, its cyan and magenta child at row 5, column 11, and the child's child at row 7, column 9; the second chain at row 12, column 49, row 13, column 57, and row 15, column 55, and nothing at row 4 on the right, where its parent was first placed. |
| `images-640x480/img-overwrite.png` | At row 3: the sixel at column 3 with its middle row gone under the `#` row and 4 cells of its bottom row erased; the kitty picture at column 23 whole over the `#` row; the kitty picture at column 43 with the `#` row drawn over it. |
| `images-640x480/img-delete.png` | The labels `kept`, `d=i`, `d=I` and `d=p` over row 3, the quarters only under `kept`; image 32 placed again at row 9, column 13; and the replies `i=33;ENOENT:image not found` and `i=32;OK`. |
| `images-640x480/img-scroll.png` | The yellow, green, blue and violet bands of the sixel at column 3 and of the kitty picture at column 20, on rows 1 to 4 beside `row 4` to `row 7`; the line that scrolled the screen on row 27; the prompt on row 30. `img-scrollforward` is compared with it. |
| `images-640x480/img-scrollback.png` | The scrollback after Shift+PageUp: the caption on row 1, both pictures whole on rows 2 to 7 from red to violet beside `row 2` to `row 7`. |
| `images-640x480/img-tmux-pane.png` | The page after tmux exits: `tmux played the test pattern in the top left of 4 panes, 2 by 2,` and `through mpv --vo=sixel, and returned to this page when it ended.`, then the prompt, with no colored pixel of the video left. Added with `img-tmux-pane` at ef3f7765, captured from that commit's run. |
| `images-640x480/img-animation.png` | Captured at 3d293dd5. At row 3, column 5, the quarters' third frame of four, where the animation stopped: cyan top left, green top right, blue bottom left, magenta bottom right, in the white frame; at row 3, column 20, the child's last frame, where it stopped after playing once: yellow top left and bottom right, magenta top right and bottom left; the two lines of text on rows 8 and 9, then the prompt. |
| `images-640x480/img-iterm2.png` | Captured at 3d293dd5. From row 3: the quarters from the PNG at column 3, 8 cells by 3; the GIF's last frame at column 15, yellow top left, blue top right, green bottom left, red bottom right; the PNG sent in parts at twice its size at column 27, 16 cells by 6; and the PNG at half its size at column 47, 4 cells by a row and a half. Rows 11 to 15 give the cursor's place after each, rows 5, 5, 8 and 4, at columns 3, 15, 27 and 47, then the prompt. |
| `images-640x480/img-placeholder.png` | Captured at 3d293dd5. The quarters at row 3, column 35, 8 cells by 3, drawn through placeholder cells, with nothing at column 5, where the cells were written before they moved; the child at row 9, column 5, 4 cells by 2, under the image id 16909060; a line of text under each, then the prompt. |
| `images-640x480/img-png.png` | At row 3: the quarters from a PNG sent inline at column 3, the ring and disc from a PNG file at column 23 with dots at its corners, the quarters from a temporary file at column 43, with `removed` under its reply, and from zlib-compressed pixels at column 63, each over its reply `i=41;OK` to `i=44;OK`; on row 10, `a file the user cannot read: i=45;EBADF:Failed to read image file`; and on row 11, `a file on rpc_pipefs the user can read: i=46;EBADF:Failed to read image file`, the prompt under it on row 12. `vt-restore` is compared with it. Recaptured at d35826d6, when img-png added row 11. |

## The minimal variants' sgr (vtium/vtium#152)

The `sgr` goldens of `2d-potato-min` and `2d-potato-min-psf` are vtium's captures from `make tier3-minimal`. vtium shears a bitmap font's synthetic italic by 0.2 about its baseline, one pixel row at a time (ADR-1302 D4), so the word `italic` leans. In unifont it shears about the strike's baseline, row 14: rows 0 and 1 move 3 pixels right, rows 2 to 6 move 2, and rows 7 to 11 move 1. In kbd's default8x16 it shears about the row under its `H`, row 12: rows 0 to 4 move 2 pixels right, rows 5 to 9 move 1, and row 15 moves 1 left. The maintainer reviewed and approved these two goldens on 2026-11-04.

## The default 2D variants (vtium/vtium#24)

The goldens of `2d-1280x800`, `2d-1366x768-potato`, `2d-7680x2160` and `2d-7680x2160-font32` draw DejaVu Sans Mono through vtium's own glyph engine and helper. The configurations state `display-dpi=86`, so DejaVu Sans Mono at 16 points draws in a 10x19 cell; at 32 points the cell is 20x38. `photo-scrollforward` is compared with `photo-scroll.png` and `vt-restore` with `cjk.png`.

Four choices of vtium show in these goldens:

- **Built-in font**: crates/vtium/README.md, "A character the frit lacks draws in the built-in font." The frit vtium makes for `font-name=DejaVu Sans Mono` names that font alone, so the CJK characters and the emoji, which DejaVu Sans Mono lacks, draw in the built-in unifont. This shows in `wide-margin`, `cjk`, and the CJK on row 14 of `underline-styles` and row 7 of `strike`.
- **Cluster**: crates/vtium/README.md, "A cluster draws its own glyph, keyed by its whole text." In `combining`, `a` with U+0303 and U+0300 shows the grave above the tilde.
- **No eviction**: crates/vtium-render/README.md, "No scaled copy evicts a stored picture." In `sixel-evict-history`, picture 1 is still drawn in the scrollback.
- **W rule**: ADR-1200 D17. The cell is as wide as the wider of W's advance and W's ink at the cell's height: 10 pixels wide at a cell height of 19, 11 at 20, 20 at 38 and 20 at 39. In `sixel-evict-zoomed`, Ctrl+= zooms to an 11x20 cell, so the zoomed text and the picture's place move, and in the 32-point variant the grid is 20x38.

## The cases whose goldens predate vtium's glyph engine

The `sgr`, `colour-forms`, `underline-styles`, `underline-colour`, `strike`, `emoji` and `sixel-article` goldens of the four default 2D variants were drawn with pango and real bold and italic faces, and vtium's captures of them differ. They fail until the work each row names lands. They are not re-baselined, because a golden of vtium's would record a defect or a missing feature as right. In every case the letters' anti-aliasing also differs, by at most 25 levels in a channel where a letter has none of the differences below.

| Case | What differs | Owner |
|---|---|---|
| `sgr` | `italic` draws DejaVu Sans Mono's regular glyph slanted by 0.2, the helper's synthetic italic, where the golden shows DejaVu Sans Mono Oblique; `bold` draws the regular glyph synthetically emboldened, where the golden shows DejaVu Sans Mono Bold, since the frit vtium makes for `font-name=DejaVu Sans Mono` names no bold or italic face, so vtium synthesises both from the regular glyph (ADR-1200 D21, ADR-1302 D4). The underline and the strike sit where the font's `post` and `OS/2` tables put them, as in the golden. | none yet for the faces |
| `colour-forms` | `bold green on blue` draws the regular glyph synthetically emboldened, where the golden shows DejaVu Sans Mono Bold, for the same reason as `sgr`'s `bold` | none yet |
| `underline-styles` | The bold row, as in `colour-forms`. The two dashed lines draw each dash from the cell's left edge for five eighths of it (crates/vtium-render/README.md, "A dash runs from the cell's left edge for five eighths of it."), on the underline row only. The CJK characters of row 14 draw in the built-in unifont. The underlines sit on the same rows as in the golden. | the bold row as for `colour-forms` |
| `underline-colour` | Only the letters' anti-aliasing, at most 23 levels in a channel. The underlines sit on the same rows as in the golden, and the case names no departure of its own. | none; a golden commit can re-baseline it |
| `strike` | The bold row, as in `colour-forms`. The CJK characters of row 7 draw in the built-in unifont. The strike sits on the same row as in the golden. | the bold row as for `colour-forms` |
| `emoji` | Every emoji in the built-in unifont's black and white, where the golden shows Noto Color Emoji | the color emoji work of vtium/vtium#24 and the glyph cache in crates/vtium-render |
| `sixel-article` | Italic, bold and the emoji of its first paragraph, as above | as above |

`gl-1280x800/` holds the first GL goldens, drawn by a renderer vtium does not use. They are a record and judge no capture: `gl-1280x800`'s cases are judged by `2d-1280x800`'s goldens, since vtium's GPU path draws the CPU path's bytes (ADR-1303 D1, D11; `../README.md`, Variants).

## The screens' heading

The screens print `vtium tier 3` and no longer name another program: `boxdraw`'s box is headed `vtium tier 3`, and `sixel-bound`'s heading reads `One sixel with more work than vtium parses at once:`. Two goldens still show the old heading `kmscon tier 3`: `gl-1280x800/boxdraw` and `gl-1280x800/sixel-bound`, which are the record above and judge no capture.
