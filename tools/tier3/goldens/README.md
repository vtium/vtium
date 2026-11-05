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
| `heads-policy-typo/policy-panic.png` | The panic screen of a vtium.conf whose policy line reads `policy = opne`, captured from `make tier3` at 127b2ca3, in a red-bordered box over the top seven rows: `vtium: config invalid, policy closed: input off`; that the errors in `/etc/vtium/vtium.conf`, listed below, leave no head that can run; the error of line 17, that `policy` cannot be "opne", wrapped at 80 columns; and that nothing on this console takes input, with the `vtium check-config` command for the full report. The rest of the screen is black, with no login prompt. |
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

## The minimal variants' sgr, re-baselined (vtium/vtium#152)

The C reference drew `2d-potato-min` and `2d-potato-min-psf` in the built-in unifont and in kbd's default8x16 PSF font, with italic upright. vtium shears a bitmap font's synthetic italic by 0.2 about its baseline, one pixel row at a time (ADR-1302 D4), so these two `sgr` goldens are vtium's captures from `make tier3-minimal` at fbc3cd54, whose consoles logged `vtium v0.1.0-alpha.1-689-gfbc3cd54`. They differ from the C reference's goldens only in the italic word, which the maintainer reviewed and approved on 2026-11-04. Both variants are re-baselined (`"rebaselined"` in `../t3_scenario.py`), so every other case of theirs is still judged by the C reference's golden. In each row the first number counts the pixels that differ from the C reference's golden, and the second those that differ by more than 32 levels in a channel.

| Golden | Differing pixels | Over 32 levels | What moved and why |
|---|---|---|---|
| `2d-potato-min/sgr.png` | 108 | 108 | `italic`, in unifont, sheared about the strike's baseline, row 14: rows 0 and 1 move 3 pixels right, rows 2 to 6 2, rows 7 to 11 1 |
| `2d-potato-min-psf/sgr.png` | 104 | 104 | `italic`, in default8x16, sheared about the row under its `H`, row 12: rows 0 to 4 move 2 pixels right, rows 5 to 9 1, row 15 1 left |

## The default 2D variants, re-baselined (vtium/vtium#24)

The C reference drew the goldens of `2d-1280x800`, `2d-1366x768-potato`, `2d-7680x2160` and `2d-7680x2160-font32` in DejaVu Sans Mono through pango. vtium draws the font through its own glyph engine and helper, so its letters differ from pango's by design, and these goldens are vtium's captures of the same screens, from `make tier3 VARIANTS="2d-1280x800 2d-1366x768-potato 2d-7680x2160 2d-7680x2160-font32"` at ee1fff50, whose consoles logged `vtium v0.1.0-alpha.1-447-gee1fff50`. A capture of each case was set beside the C reference's golden and read before it became a golden: a case is re-baselined only where every difference is the font engine or a departure a crate's README lists. The C reference's goldens stay in `../c-goldens/` as the record of what it drew, and judge every case these tables do not list (`../README.md`, Variants). Each golden stays in `../provisional.tsv` until the operator reviews it. `photo-scrollforward` is compared with `photo-scroll.png` and `vt-restore` with `cjk.png`.

The configurations state `display-dpi=86`, so DejaVu Sans Mono at 16 points draws in the C reference's 10x19 cell and every screen keeps the C reference's grid; at 32 points the cell is 20x38. In each row, the first number counts the pixels that differ from the C reference's golden, and the second the pixels that differ by more than 32 levels in a channel, which the anti-aliasing of the same letter never reaches. The reasons name these departures and decisions:

- **built-in font**: crates/vtium/README.md, "A character the frit lacks draws in the built-in font." The frit vtium makes for `font-name=DejaVu Sans Mono` names that font alone, where pango asked fontconfig for another font for each character DejaVu Sans Mono lacks.
- **cluster**: crates/vtium/README.md, "A cluster draws its own glyph, keyed by its whole text."
- **no eviction**: crates/vtium-render/README.md, "No scaled copy evicts a stored picture."
- **W rule**: ADR-1200 D17. The cell is as wide as the wider of W's advance and W's ink at the cell's height. At 19 and 39 pixels that is the width pango gave, 10 and 20; at 20 and 38 pixels it is one pixel wider, 11 and 20 where pango gave 10 and 19.

| Golden | Differing pixels | Over 32 levels | What moved and why |
|---|---|---|---|
| `2d-1280x800/boxdraw.png` | 967 | 0 | the letters' anti-aliasing, at most 18 levels in a channel |
| `2d-1280x800/blocks.png` | 66 | 0 | the letters' anti-aliasing, at most 15 levels in a channel |
| `2d-1280x800/mosaic.png` | 921 | 0 | the letters' anti-aliasing, at most 17 levels in a channel |
| `2d-1280x800/wide-margin.png` | 10,478 | 625 | the letters; the CJK characters and the emoji, which DejaVu Sans Mono lacks, in the built-in unifont where pango fell back to WenQuanYi Micro Hei and Noto Color Emoji (built-in font) |
| `2d-1280x800/combining.png` | 10,241 | 12 | the letters; `a` with U+0303 and U+0300 shows the grave above the tilde, where the C reference drew the tilde alone (cluster) |
| `2d-1280x800/sync-hold.png` | 8,487 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1280x800/sixel.png` | 18,253 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1280x800/sixel-ramps.png` | 29,801 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1280x800/sixel-bound.png` | 3,484 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1280x800/sixel-evict.png` | 7,653 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1280x800/sixel-evict-rotated.png` | 7,653 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1280x800/sixel-evict-history.png` | 351,438 | 348,160 | the letters; picture 1 is still drawn in the scrollback, where the C reference had evicted it for a scaled copy (no eviction) |
| `2d-1280x800/sixel-evict-zoomed.png` | 19,310 | 16,124 | Ctrl+= zooms to an 11x20 cell where the C reference zoomed to 10x20, so the zoomed text and the picture's place move (W rule) |
| `2d-1280x800/photos.png` | 6,314 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-1280x800/photo-scroll.png` | 15,788 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-1280x800/photo-scrollback.png` | 15,364 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-1280x800/mpv-sixel.png` | 3,922 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-1280x800/tmux-2x2.png` | 4,022 | 0 | the letters' anti-aliasing, at most 18 levels in a channel |
| `2d-1280x800/tmux-3x3.png` | 4,043 | 0 | the letters' anti-aliasing, at most 18 levels in a channel |
| `2d-1280x800/kitty.png` | 8,339 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1280x800/kitty-photo.png` | 2,579 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1280x800/kitty-files.png` | 11,424 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1280x800/kitty-layers.png` | 12,509 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1280x800/s1-scroll.png` | 130,720 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-1280x800/flood.png` | 618 | 0 | the letters' anti-aliasing, at most 17 levels in a channel |
| `2d-1280x800/cjk.png` | 4,071 | 1,897 | the letters; the CJK characters in the built-in unifont where pango drew WenQuanYi Micro Hei (built-in font) |
| `2d-1366x768-potato/boxdraw.png` | 967 | 0 | the letters' anti-aliasing, at most 18 levels in a channel |
| `2d-1366x768-potato/blocks.png` | 66 | 0 | the letters' anti-aliasing, at most 15 levels in a channel |
| `2d-1366x768-potato/mosaic.png` | 921 | 0 | the letters' anti-aliasing, at most 17 levels in a channel |
| `2d-1366x768-potato/wide-margin.png` | 10,527 | 625 | the letters; the CJK characters and the emoji, which DejaVu Sans Mono lacks, in the built-in unifont where pango fell back to WenQuanYi Micro Hei and Noto Color Emoji (built-in font) |
| `2d-1366x768-potato/combining.png` | 10,241 | 12 | the letters; `a` with U+0303 and U+0300 shows the grave above the tilde, where the C reference drew the tilde alone (cluster) |
| `2d-1366x768-potato/sync-hold.png` | 8,487 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1366x768-potato/sixel.png` | 18,253 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1366x768-potato/sixel-ramps.png` | 29,801 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1366x768-potato/sixel-bound.png` | 3,484 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1366x768-potato/sixel-evict.png` | 7,679 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1366x768-potato/sixel-evict-rotated.png` | 7,679 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1366x768-potato/sixel-evict-history.png` | 351,516 | 348,160 | the letters; picture 1 is still drawn in the scrollback, where the C reference had evicted it for a scaled copy (no eviction) |
| `2d-1366x768-potato/sixel-evict-zoomed.png` | 18,813 | 15,759 | Ctrl+= zooms to an 11x20 cell where the C reference zoomed to 10x20, so the zoomed text and the picture's place move (W rule) |
| `2d-1366x768-potato/photos.png` | 6,314 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-1366x768-potato/photo-scroll.png` | 15,887 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-1366x768-potato/photo-scrollback.png` | 15,339 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-1366x768-potato/mpv-sixel.png` | 3,879 | 0 | the letters' anti-aliasing, at most 18 levels in a channel |
| `2d-1366x768-potato/tmux-2x2.png` | 4,022 | 0 | the letters' anti-aliasing, at most 18 levels in a channel |
| `2d-1366x768-potato/tmux-3x3.png` | 4,043 | 0 | the letters' anti-aliasing, at most 18 levels in a channel |
| `2d-1366x768-potato/kitty.png` | 8,339 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1366x768-potato/kitty-photo.png` | 2,579 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1366x768-potato/kitty-files.png` | 11,424 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1366x768-potato/kitty-layers.png` | 12,509 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-1366x768-potato/s1-scroll.png` | 124,329 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-1366x768-potato/flood.png` | 618 | 0 | the letters' anti-aliasing, at most 17 levels in a channel |
| `2d-1366x768-potato/cjk.png` | 4,071 | 1,897 | the letters; the CJK characters in the built-in unifont where pango drew WenQuanYi Micro Hei (built-in font) |
| `2d-7680x2160/boxdraw.png` | 967 | 0 | the letters' anti-aliasing, at most 18 levels in a channel |
| `2d-7680x2160/blocks.png` | 66 | 0 | the letters' anti-aliasing, at most 15 levels in a channel |
| `2d-7680x2160/mosaic.png` | 921 | 0 | the letters' anti-aliasing, at most 17 levels in a channel |
| `2d-7680x2160/wide-margin.png` | 15,628 | 625 | the letters; the CJK characters and the emoji, which DejaVu Sans Mono lacks, in the built-in unifont where pango fell back to WenQuanYi Micro Hei and Noto Color Emoji (built-in font) |
| `2d-7680x2160/combining.png` | 10,241 | 12 | the letters; `a` with U+0303 and U+0300 shows the grave above the tilde, where the C reference drew the tilde alone (cluster) |
| `2d-7680x2160/sync-hold.png` | 8,487 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-7680x2160/sixel.png` | 18,253 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-7680x2160/sixel-ramps.png` | 29,801 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-7680x2160/sixel-bound.png` | 3,484 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-7680x2160/sixel-evict.png` | 7,689 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-7680x2160/sixel-evict-rotated.png` | 7,689 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-7680x2160/sixel-evict-history.png` | 4,197,792 | 4,193,280 | the letters; picture 1 is still drawn in the scrollback, where the C reference had evicted it for a scaled copy (no eviction) |
| `2d-7680x2160/sixel-evict-zoomed.png` | 19,165 | 16,118 | Ctrl+= zooms to an 11x20 cell where the C reference zoomed to 10x20, so the zoomed text and the picture's place move (W rule) |
| `2d-7680x2160/photos.png` | 6,314 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-7680x2160/photo-scroll.png` | 47,820 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-7680x2160/photo-scrollback.png` | 47,013 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-7680x2160/mpv-sixel.png` | 4,007 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-7680x2160/tmux-2x2.png` | 4,022 | 0 | the letters' anti-aliasing, at most 18 levels in a channel |
| `2d-7680x2160/tmux-3x3.png` | 4,043 | 0 | the letters' anti-aliasing, at most 18 levels in a channel |
| `2d-7680x2160/kitty.png` | 8,339 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-7680x2160/kitty-photo.png` | 2,579 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-7680x2160/kitty-files.png` | 11,424 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-7680x2160/kitty-layers.png` | 12,509 | 0 | the letters' anti-aliasing, at most 25 levels in a channel |
| `2d-7680x2160/s1-scroll.png` | 356,427 | 0 | the letters' anti-aliasing, at most 23 levels in a channel |
| `2d-7680x2160/flood.png` | 618 | 0 | the letters' anti-aliasing, at most 17 levels in a channel |
| `2d-7680x2160/cjk.png` | 4,071 | 1,897 | the letters; the CJK characters in the built-in unifont where pango drew WenQuanYi Micro Hei (built-in font) |
| `2d-7680x2160-font32/boxdraw.png` | 14,313 | 13,476 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/blocks.png` | 37,041 | 36,809 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/mosaic.png` | 168,797 | 167,873 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/wide-margin.png` | 81,457 | 73,903 | the 20x38 grid where the C reference's was 19x38 (W rule); the letters; the CJK characters and the emoji, which DejaVu Sans Mono lacks, in the built-in unifont where pango fell back to WenQuanYi Micro Hei and Noto Color Emoji (built-in font) |
| `2d-7680x2160-font32/combining.png` | 60,115 | 51,930 | the 20x38 grid where the C reference's was 19x38 (W rule); the letters; `a` with U+0303 and U+0300 shows the grave above the tilde, where the C reference drew the tilde alone (cluster) |
| `2d-7680x2160-font32/sync-hold.png` | 70,642 | 64,387 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/sixel.png` | 233,199 | 219,148 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/sixel-ramps.png` | 967,893 | 355,154 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/sixel-bound.png` | 20,860 | 18,253 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/sixel-evict.png` | 49,350 | 43,887 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/sixel-evict-rotated.png` | 54,431 | 48,999 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/sixel-evict-history.png` | 4,224,347 | 4,220,873 | the 20x38 grid where the C reference's was 19x38 (W rule); the letters; picture 1 is still drawn in the scrollback, where the C reference had evicted it for a scaled copy (no eviction) |
| `2d-7680x2160-font32/sixel-evict-zoomed.png` | 25,458 | 15,994 | vtium draws a 20x38 grid where the C reference drew 19x38 (W rule), and Ctrl+= zooms both to a 20x39 cell, in which the letters' anti-aliasing differs. |
| `2d-7680x2160-font32/photos.png` | 222,434 | 92,737 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/photo-scroll.png` | 116,417 | 97,690 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/photo-scrollback.png` | 178,659 | 112,695 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/mpv-sixel.png` | 24,099 | 20,988 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/tmux-2x2.png` | 25,105 | 22,137 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/tmux-3x3.png` | 25,145 | 22,158 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/kitty.png` | 253,054 | 152,943 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/kitty-photo.png` | 133,874 | 43,337 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/kitty-files.png` | 449,028 | 213,824 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/kitty-layers.png` | 210,598 | 112,687 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/s1-scroll.png` | 1,079,582 | 952,474 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/flood.png` | 3,439 | 2,922 | the 20x38 grid where the C reference's was 19x38 (W rule) |
| `2d-7680x2160-font32/cjk.png` | 18,953 | 16,779 | the 20x38 grid where the C reference's was 19x38 (W rule); the letters; the CJK characters in the built-in unifont where pango drew WenQuanYi Micro Hei (built-in font) |

These cases of the four variants keep the C reference's goldens, and fail, until the work each row names lands; they are not re-baselined, because a golden of vtium's would record a defect or a missing feature as right.

| Case | What differs | Owner |
|---|---|---|
| `sgr` | `italic` draws DejaVu Sans Mono's regular glyph slanted by 0.2, the helper's synthetic italic, where pango drew DejaVu Sans Mono Oblique; `bold` draws the regular glyph synthetically emboldened, where pango drew DejaVu Sans Mono Bold, since the frit vtium makes for `font-name=DejaVu Sans Mono` names no bold or italic face, so vtium synthesises both from the regular glyph (ADR-1200 D21, ADR-1302 D4); the underline sits on the cell's bottom row and not where the font's `post` table puts it | none yet for the faces; the underline as for `underline-styles` |
| `colour-forms` | `bold green on blue` draws the regular glyph synthetically emboldened, where pango drew DejaVu Sans Mono Bold, for the same reason as `sgr`'s `bold` | none yet |
| `underline-styles` | Every underline sits on the cell's bottom row, row 18 of 19, where the C reference drew it on row 15 from the font's `post` table: the console draws a frit's lines for the cell's height (`Decorations::for_height`) and not the lines the helper measures from the font; the bold row as in `colour-forms`. The two dashed lines will also show the dash departure, crates/vtium-render/README.md, "A dash runs from the cell's left edge for five eighths of it.", on the underline row only | crates/vtium (the console's `DeferredFace::decorations`); the bold row as for `colour-forms` |
| `underline-colour` | The underlines on the bottom row, as in `underline-styles` | crates/vtium |
| `strike` | The strike on row 9 of 19, half the cell, where the C reference drew it on row 11 from the font's `OS/2` table, for the same reason; the bold row as in `colour-forms` | crates/vtium; the bold row as for `colour-forms` |
| `emoji` | Every emoji in the built-in unifont's black and white, where pango drew Noto Color Emoji; the underlined row as in `underline-styles` | the color emoji work of vtium/vtium#24 and the glyph cache in crates/vtium-render |
| `sixel-article` | Italic, bold, the underlines, the strike and the emoji of its first paragraph, as above | as above |
