# docs/figures

The figures of vtium's documentation pages outside the decision records, which keep theirs beside them.

| Path | What it is | How it is made |
|---|---|---|
| `concepts-pieces.svg` | [concepts.md](../concepts.md): vtium's suite, which pieces run as root and which as the user, the pty between them, and the font builder's one sudo | Written by hand; `rsvg-convert concepts-pieces.svg -o OUT.png` renders it to check |
| `concepts-vt-handover.svg` | [concepts.md](../concepts.md): who holds the display and the input across a VT switch away and back | Written by hand; `rsvg-convert concepts-vt-handover.svg -o OUT.png` renders it to check |
| `terminology-journey.svg` | [terminology.md](../terminology.md): a character's glyph from the font files and the frit, through the helper and a vitrified set, to the console and the cells, with the sprites and the built-in sets inside vtium | Written by hand; `rsvg-convert terminology-journey.svg -o OUT.png` renders it to check |
| `terminology-glass.svg` | [terminology.md](../terminology.md): glassmaking's raw materials, frit, melting and glass beside vtium's font files, frit, vtium-vitrify and glyphs | Written by hand; `rsvg-convert terminology-glass.svg -o OUT.png` renders it to check |
| `terminology-names.svg` | [terminology.md](../terminology.md): the family of names, from TTY, glass TTY and VT, and from vitrum, vitrify and frit, to vtium and the names in its tree | Written by hand; `rsvg-convert terminology-names.svg -o OUT.png` renders it to check |
| `concepts-screen-palette.png` | [concepts.md](../concepts.md): tier 2's `palette-unifont` screen as vtium draws it | Drawn by vtium's own renderer, below |
| `concepts-screen-cjk.png` | [concepts.md](../concepts.md): tier 2's `cjk-unifont` screen as vtium draws it | Drawn by vtium's own renderer, below |
| `concepts-screen-underline.png` | [concepts.md](../concepts.md): tier 2's `underline-unifont` screen as vtium draws it | Drawn by vtium's own renderer, below |
| `concepts-screen-small-320x240.png` | [concepts.md](../concepts.md): tier 2's `small-320x240` screen, minspec's display floor | Converted from tier 2's golden, below |
| `concepts-screen-small-640x480.png` | [concepts.md](../concepts.md): tier 2's `small-640x480` screen | Converted from tier 2's golden, below |

The screens are tier 2's screens ([tools/tier2](../../tools/tier2/README.md)), drawn into memory by vtium-term and vtium-render, never by the C reference, and converted from PPM with ImageMagick, `-strip` keeping the PNG free of timestamps so a redraw gives the same bytes. From the repository's root:

```sh
cargo build --release -p vtium-tier2 --bins
target/release/tier2 render --out DIR
for n in palette cjk underline; do
  magick DIR/$n-unifont.ppm -strip docs/figures/concepts-screen-$n.png
done
```

The two small-display screens are tier 2's own goldens for #75, reviewed by the operator, converted the same way:

```sh
for s in 320x240 640x480; do
  magick tools/tier2/goldens/small-$s.ppm -strip docs/figures/concepts-screen-small-$s.png
done
```
