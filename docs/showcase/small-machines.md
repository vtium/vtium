<!-- tools/tier3/showcase.sh writes this page from tier 3's showcase run. Change the scenes in tools/tier3/guest/screens/ or the figures in docs/figures/ and run it again; an edit here is lost. -->

# Small machines

The same console runs on a machine with 512 MB of memory and no GPU. These screens are tier 3 goldens of vtium's smallest build, minspec.

These screens are part of [vtium's showcase](README.md), which says how they are made.

![vtium on a 320x240 display: a ruler of 40 columns, a colored directory listing, the 16 background colors, six color names, bold, faint, underlined, reversed and struck words, a box of sprite lines around cpu, mem and load figures, a line of Latin-1, four underline styles, a progress bar at 50%, and a prompt](small/small-console.png)

The same build draws this in the same guest on a 320x240 display, the smallest vtium supports, in vtium's 8x8 PSF test font made from Unifont's glyphs folded to half height, 40 by 30 cells. It is also a tier 3 golden. The maintainer reviewed and accepted it.

![vtium on a 640x480 display: the labels fg30 to fg97 in their own colors and bg40 to bg107 on their own backgrounds, each set wrapped over two rows, with fg30 and bg40 drawn black on black; the words bold, underline, reverse, bold-underline-red, faint and italic in their styles, the 256-color cube as six bands, a 24-bit ramp from green to red, and a shell prompt with a block cursor](small/sgr.png)

vtium's minspec build draws this on a VT of tier 3's guest, which has 1 vCPU and 512 MB, on a virtio-vga display at 640x480, in vtium's 8x16 PSF test font made from Unifont's glyphs, 80 by 30 cells. It is a tier 3 golden. The font has no italic face, so vtium slants the word `italic` itself. It moves the upper rows of each letter to the right in steps of one pixel, and the lowest rows stay where they are.

