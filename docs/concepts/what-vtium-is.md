# What VTium is

vtium is a terminal for the Linux virtual console. The virtual console is the set of full-screen text terminals that the kernel provides, and each of them is called a VT. vtium renders the terminal itself, straight to the display. It uses the kernel's display interface, KMS/DRM. On a machine that has only the older fbdev framebuffer, it uses that instead. Because vtium renders the screen itself, it needs no display server.

A program running in vtium sees a modern terminal, with colors, text styles, Unicode, and mouse reporting. A vtium built with the `images` feature also shows images. A program can send them as sixel, through the kitty graphics protocol, or as iTerm2's inline images. See [the image pipeline issue](https://github.com/vtium/vtium/issues/28). [What vtium does](../features.md) lists its features, and [modes.md](../modes.md) lists the terminal modes it recognizes.

vtium's renderer drew these three screens in memory. They show the color palette, double-width Japanese, Chinese and Korean text, and the underline styles, each in a color of its own.

![vtium's drawing of the palette: the 16 background colors, the 16 foreground colors on letter pairs, a ramp through the 256-color palette, and a 24-bit gradient from red to green.](../figures/concepts-screen-palette.png) ![vtium's drawing of double-width text: Japanese, Chinese and Korean, full-width Latin, narrow letters between wide characters, and wide text in bold and in reverse.](../figures/concepts-screen-cjk.png) ![vtium's drawing of underlines: single, double, curly, dotted and dashed, each in the text color and in a color of its own, then underlines on a background, in reverse and in bold.](../figures/concepts-screen-underline.png)

vtium is written in Rust. Two things set what it does: the specifications it cites, and the project's table of the modes and features it supports. vtium's own tests check it against goldens. A golden is a saved expected result that vtium's output is compared with. See [How vtium is judged](how-vtium-is-judged.md) for details.

## Small machines first

vtium is built for small machines first, and it runs on large ones as well. Its default build is called minspec. Minspec is the smallest build that is still a complete console. It renders on the CPU into a plain memory buffer that the display shows, so it needs no GPU.

The fonts built into vtium are bitmap fonts. A separate program, the font helper, draws the glyphs of any other font while vtium runs. A glyph is the drawn shape of a character. The font helper also draws outline fonts. See [the issue that generates glyphs in the font helper](https://github.com/vtium/vtium/issues/145).

Images, right-to-left text, and rendering on the GPU are features built on top of minspec.

The machine that minspec is measured against is a single-board computer with ARM cores and 512 MB of memory or less. Its smallest display is a 320×240 panel. On that panel, an 8×8 font gives a grid of 40 columns by 30 rows. The same build renders on large displays too. [architecture.md](../architecture.md#the-minspec-build) describes that machine, the smallest display, and what vtium promises at each size.

The two screens below show the same shell session as vtium renders it at two sizes. The first is the smallest display, 320×240, with an 8×8 font, which gives 40 columns by 30 rows. The second is one step up, 640×480, with an 8×16 font, which gives 80 columns by 30 rows.

![A shell session on a 320×240 display in an 8×8 font: a reversed title row, a wrapped command line, a box around styled words, the 16 colors, underline styles and a reversed status line, in 40 columns by 30 rows](../figures/concepts-screen-small-320x240.png)

![The same session on a 640×480 display in an 8×16 font, 80 columns by 30 rows, with a directory listing, four box styles, and 256-color and 24-bit color ramps](../figures/concepts-screen-small-640x480.png)
