# 320x240-tiny5

minspec's display floor, a 320x240 SPI panel, in the example font set tiny5: 320x240, 24-bit color, drawn in `tiny5` at 6x9, 53x26 cells, the card on 10 screens, each block on 2, and vtium's message bars on two more, shown together in the collage.

![320x240-tiny5](collage.png)

| Display | Class | Cell | Grid | Font | Screens |
|---|---|---|---|---|---|
| 320x240, 24-bit color | small color LCD | 6x9 | 53x26 | `tiny5`, the font set [`tiny5.vtfrit`](https://github.com/vtium/vtium/blob/main/crates/vtium-config/frits/tiny5.vtfrit) | [screen-1.png](screen-1.png), [screen-2.png](screen-2.png), [screen-3.png](screen-3.png), [screen-4.png](screen-4.png), [screen-5.png](screen-5.png), [screen-6.png](screen-6.png), [screen-7.png](screen-7.png), [screen-8.png](screen-8.png), [screen-9.png](screen-9.png), [screen-10.png](screen-10.png), [screen-11.png](screen-11.png), [screen-12.png](screen-12.png) |

## Why this font

This profile shows the smallest font vtium ships a font set for: the example font set `tiny5`. vtium's own choice for this display is the built-in Unscii-8 at 8x8, which the profile `320x240` shows. If you want `tiny5`, you install the font set and its font by hand and name it in `font-name`. Tiny5 Mono is a pixel font whose capitals are 5 pixels tall. It draws at its own 6x9 character cell, which gives this display 53x26 cells, where the built-in Unscii-8's 8x8 gives 40x30: a third more columns, for four fewer rows. It holds ASCII, Latin-1, code page 437, Greek, Armenian, box drawing, blocks and Braille, 4,882 characters in all, so most of the card's text draws in it. vtium's sprites draw the box drawing, blocks and Braille. vtium ships the font set `tiny5` but not the font, so a machine needs Tiny5 Mono's BDF file installed by hand, as the font set's comment says. See [the Tiny5 Mono issue](https://github.com/vtium/vtium/issues/217). Its grid is shorter than one of the card's 40x30 blocks, so each block takes two screens here: its first 26 rows, then its title again and its last 4 rows. A character Tiny5 Mono lacks draws on these screens as the font's missing glyph, as CJK, Hangul and emoji do here. The console draws such a character in the built-in Unscii-8 where Unscii-8 holds it, centered in the 6x9 cell with a column cut off each side. These screens draw only the font set's own glyphs, so they do not show that. See [the issue about the fallback to the built-in font](https://github.com/vtium/vtium/issues/141).

## Configuration

[vtium.conf](vtium.conf):

```ini
# The font set tiny5: crates/vtium-config/frits/tiny5.vtfrit in
# /etc/vtium/fonts, with Tiny5 Mono's BDF file installed where its
# source= line says. It draws at its own 6x9 cell.
font-name=tiny5
font-size=0
```

The screens here draw the glyphs that the font helper, vtium-config, generates from the shipped font set and Tiny5 Mono's BDF file. `make specimens-fonts` fetches that file, pinned by commit and SHA-256, and the font set's `source=` line points at the fetched copy. `tools/specimens/README.md` describes how. Where that file is missing, the check skips this profile while it is provisional, and fails it once it is accepted.
