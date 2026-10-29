# 1920x1080

A full HD monitor: 1920x1080, 24-bit color, drawn in `desktop` at 11x24, 174x45 cells, the card on 2 screens, shown together in the collage.

![1920x1080](collage.png)

| Display | Class | Cell | Grid | Font | Screens |
|---|---|---|---|---|---|
| 1920x1080, 24-bit color | order unknown | 11x24 | 174x45 | `desktop`, the frit [`desktop.vtfrit`](../../../tools/specimens/frits/desktop.vtfrit) | [screen-1.png](screen-1.png), [screen-2.png](screen-2.png) |

## Why this font

ADR-1200 D11 builds one outline size for the display the package hook sees, snapped down to 24, 32 or 48 pixels, with unifont filling the outline font's gaps. The desktop set holds two of them, 24 and 32, from JetBrains Mono for text, Symbols Nerd Font Mono for the private-use symbols and Noto Color Emoji for emoji, with the built-in unifont last; `font-size=0` picks the strike, the largest whole multiple of a strike's height under the size it asks (ADR-1200 D9). Outline glyphs draw antialiased at this size. A set maps single codepoints, so ZWJ sequences, flags and keycaps draw as their parts until #144 shapes them, and the scripts that need shaping draw unshaped.

## Configuration

[vtium.conf](vtium.conf):

```ini
# The desktop frit: tools/specimens/frits/desktop.vtfrit in
# /etc/vtium/fonts, its glyphs generated at run time (ADR-1301 D2).
font-name=desktop
font-size=0
```

The console loads a set by name from #126. The screens here draw the glyphs vtium-vitrify's engine, the one the helper runs (ADR-1301 D2), generates from the frit and the pinned fonts `make specimens-fonts` fetches, in the profile's cell (`tools/specimens/README.md`); where those fonts are absent the check skips this profile while it is provisional, and fails it once accepted.
