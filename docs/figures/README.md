# docs/figures

The figures of vtium's documentation pages outside the decision records, which keep theirs beside them.

| Path | What it is | How it is made |
|---|---|---|
| `concepts-pieces.svg` | [concepts/suite.md](../concepts/suite.md): vtium's suite, which pieces run as root, which as nobody and which as the user, the pty between vtium and the session, the font helper that vtium starts and talks to over two pipes, and the one sudo of each install of vtium.conf or a frit | Written by hand; `rsvg-convert concepts-pieces.svg -o OUT.png` renders it to check |
| `concepts-vt-handover.svg` | [concepts/start-up.md](../concepts/start-up.md): who holds the display and the input across a VT switch away and back | Written by hand; `rsvg-convert concepts-vt-handover.svg -o OUT.png` renders it to check |
| `why-vtium-boot.svg` | [why-vtium.md](../why-vtium.md): where vtium sits in a Linux machine's boot, beside the kernel's console alone, with the getty fallback | Written by hand; `rsvg-convert why-vtium-boot.svg -o OUT.png` renders it to check |
| `why-vtium-spectrum.svg` | [why-vtium.md](../why-vtium.md): the displays vtium's test card is drawn at, from 320x240 to 2560x1440, to scale, with each one's cells, grid and measured memory | Written by hand; `rsvg-convert why-vtium-spectrum.svg -o OUT.png` renders it to check |
| `why-vtium-glass.svg` | [why-vtium.md](../why-vtium.md): glassmaking's raw materials, frit, melting and glass beside vtium's font files, frit, vtium-config and glyphs | Written by hand; `rsvg-convert why-vtium-glass.svg -o OUT.png` renders it to check |
| `concepts-screen-palette.png` | [concepts/what-vtium-is.md](../concepts/what-vtium-is.md): tier 2's `palette-unifont` screen as vtium draws it | Drawn by vtium's own renderer, below |
| `concepts-screen-cjk.png` | [concepts/what-vtium-is.md](../concepts/what-vtium-is.md): tier 2's `cjk-unifont` screen as vtium draws it | Drawn by vtium's own renderer, below |
| `concepts-screen-underline.png` | [concepts/what-vtium-is.md](../concepts/what-vtium-is.md): tier 2's `underline-unifont` screen as vtium draws it | Drawn by vtium's own renderer, below |
| `concepts-screen-small-320x240.png` | [concepts/what-vtium-is.md](../concepts/what-vtium-is.md): tier 2's `small-320x240` screen, minspec's display floor | Converted from tier 2's golden, below |
| `concepts-screen-small-640x480.png` | [concepts/what-vtium-is.md](../concepts/what-vtium-is.md): tier 2's `small-640x480` screen | Converted from tier 2's golden, below |
| `concepts-judged-golden-crop.png` | [concepts/how-vtium-is-judged.md](../concepts/how-vtium-is-judged.md): the row of tier 2's `small-320x240` golden that holds `notes.txt`, cropped and enlarged 8× | Cropped from tier 2's golden, below |
| `concepts-judged-actual-crop.png` | [concepts/how-vtium-is-judged.md](../concepts/how-vtium-is-judged.md): the same row as vtium draws it with one letter planted, `notas.txt`, cropped and enlarged 8× | Cropped from a tier 2 run with the planted letter, below |
| `concepts-judged-diff-crop.png` | [concepts/how-vtium-is-judged.md](../concepts/how-vtium-is-judged.md): the same row of the diff image that tier 2 writes for that run, three pixels in red, cropped and enlarged 8× | Cropped from the same run, below |
| `concepts-judged-diff.png` | [concepts/how-vtium-is-judged.md](../concepts/how-vtium-is-judged.md): that whole diff image at its real size, 320×240 | Converted from the same run, below |
| `concepts-flow-default.png` | [concepts/flows.md](../concepts/flows.md): vtium-config's Flow view of the built-in configuration | Drawn from the TUI golden `flow-default-120x32`, below |
| `concepts-flow-kiosk-admin.png` | [concepts/flows.md](../concepts/flows.md): the Flow view of [heads/kiosk-admin.conf](../heads/kiosk-admin.conf) on a machine with both its monitors and two keyboards | Drawn from the TUI golden `flow-kiosk-admin-120x32`, provisional, below |
| `concepts-flow-stations.png` | [concepts/flows.md](../concepts/flows.md): the Flow view of [heads/stations.conf](../heads/stations.conf) on a machine with its three monitors and a keyboard and mouse on each station's port | Drawn from the TUI golden `flow-stations-120x32`, provisional, below |
| `concepts-flow-two-vts.png` | [concepts/flows.md](../concepts/flows.md): the Flow view of two heads on VT 3 and VT 4, both showing every display | Drawn from the TUI golden `flow-shared-vts-120x32`, below |

The screens are tier 2's screens ([tools/tier2](https://github.com/vtium/vtium/blob/main/tools/tier2/README.md)), drawn into memory by vtium-term and vtium-render and converted from PPM with ImageMagick, `-strip` keeping the PNG free of timestamps so a redraw gives the same bytes. From the repository's root:

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

The four pictures of how tier 2 flags a difference come from a real run of tier 2 with one letter planted in the `small-320x240` screen's bytes. On a copy of `tools/tier2/src/screens.rs`, change `notes.txt` to `notas.txt` in the `ls` line of `SMALL_40`, run tier 2, then put the file back. The run fails `small-320x240`, `small-320x240-1bit` and `vitrified-320x240`, and it exits 1. The crop is the fourth row of 8×8 character cells, from column 12 to column 22. From the repository's root:

```sh
cp tools/tier2/src/screens.rs screens.rs.bak
sed -i 's/  notes.txt  \\x1b\[36mlib/  notas.txt  \\x1b[36mlib/' tools/tier2/src/screens.rs
make tier2
mv screens.rs.bak tools/tier2/src/screens.rs && touch tools/tier2/src/screens.rs
O=tools/tier2/out
for p in golden:tools/tier2/goldens/small-320x240.ppm actual:$O/actual/small-320x240.ppm diff:$O/diff/small-320x240.ppm; do
  magick ${p#*:} -crop 88x8+96+24 +repage -filter point -scale 800% -strip docs/figures/concepts-judged-${p%%:*}-crop.png
done
magick $O/diff/small-320x240.ppm -strip docs/figures/concepts-judged-diff.png
```

The four pictures of the Flow view are vtium-config's own screen goldens in `crates/vtium-config/tests/screens/`, drawn through vtium's renderer in unifont by `make tui-shots` (crates/vtium-config's README, "Reviewing the screens"). The test `the_flow_view_s_states_at_80x25_and_120x32` in `crates/vtium-config/tests/tui.rs` reaches each screen. The kiosk and the stations draw the files under `docs/heads/` themselves, so a change to one of those files moves its golden. `crates/vtium-config/tests/screens/provisional.tsv` lists the goldens the maintainer has not yet reviewed. From the repository's root:

```sh
make tui-shots OUT=DIR
cp DIR/flow-default-120x32.png docs/figures/concepts-flow-default.png
cp DIR/flow-kiosk-admin-120x32.png docs/figures/concepts-flow-kiosk-admin.png
cp DIR/flow-stations-120x32.png docs/figures/concepts-flow-stations.png
cp DIR/flow-shared-vts-120x32.png docs/figures/concepts-flow-two-vts.png
```
