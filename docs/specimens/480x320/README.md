# 480x320

A 480x320 SPI panel, 3.5 inches: 480x320, 24-bit color, drawn in `unscii-8` at 8x8, 60x40 cells, the card on 5 screens, shown together in the collage.

![480x320](collage.png)

| Display | Class | Cell | Grid | Font | Screens |
|---|---|---|---|---|---|
| 480x320, 24-bit color | small color LCD | 8x8 | 60x40 | `unscii-8`, built in | [screen-1.png](screen-1.png), [screen-2.png](screen-2.png), [screen-3.png](screen-3.png), [screen-4.png](screen-4.png), [screen-5.png](screen-5.png) |

## Why this font

ADR-1200 D7 draws the built-in Unscii-8 for cells under 16 pixels high, and #118's rule has `font-size=0` take a set's smallest height when 16-pixel cells would leave fewer than 80x25 of them, so vtium draws this display in 8x8 cells with no font keys at all. It is the most text the display holds at a cell a person can still read, and it costs no disk: D11 builds nothing beyond the built-ins for an English machine at this size. Unscii-8 holds no CJK or Hangul, nor the emoji and private-use symbols, so those draw as the missing glyph, U+25A1, until #141's per-character fallback draws them from unifont; in a zh, ja or ko locale D7 draws unifont at 16 pixels instead.

## Configuration

[vtium.conf](vtium.conf):

```ini
# vtium's defaults: font-size=0 picks 8 pixels here (#118), which
# draws the built-in Unscii-8 (ADR-1200 D7).
font-size=0
```

The console chooses this set by itself once #126 and #118's rule land; the screens here draw vtium-font's built-in set directly.
