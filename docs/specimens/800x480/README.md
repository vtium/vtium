# 800x480

A 7-inch panel, as on the Raspberry Pi touch display: 800x480, 24-bit color, drawn in `unifont` at 8x16, 100x30 cells, the card on 2 screens, shown together in the collage.

![800x480](collage.png)

| Display | Class | Cell | Grid | Font | Screens |
|---|---|---|---|---|---|
| 800x480, 24-bit color | small color LCD | 8x16 | 100x30 | `unifont`, built in | [screen-1.png](screen-1.png), [screen-2.png](screen-2.png) |

## Why this font

16-pixel cells leave at least 80x25 here, so #118's rule keeps `font-size=0`'s 16 pixels, and ADR-1200 D7 draws the built-in unifont from 16 pixels: 8x16 cells for Latin, Greek and Cyrillic and 16x16 for CJK and Hangul, which it covers whole, with nothing built. The size the display wants is under 24 pixels, the smallest outline size D11 builds, so a default machine holds no outline set for it. Emoji draw as unifont's monochrome glyphs where it has one, and Nerd Font symbols as the missing glyph until #23 and #64 draw them from an outline font.

## Configuration

[vtium.conf](vtium.conf):

```ini
# vtium's defaults: font-size=0 picks 16 pixels here, which draws
# the built-in unifont (ADR-1200 D7).
font-size=0
```

The console chooses this set by itself once #126 and #118's rule land; the screens here draw vtium-font's built-in set directly.
