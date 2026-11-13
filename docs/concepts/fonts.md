# Fonts, generated at run time

This page describes how vtium gets the glyphs it renders. A glyph is the image of one character in a font. These parts are all built: the font helper, the console's glyph store, the built-in fonts, switching, preview, and new-frit.

The console does not read font files itself. Two parts do that work for it:

- A **frit**, or font set file, names a font's sources.
- The **font helper** is vtium-config's serve command, which runs in a sandbox. It reads the frit's sources and rasterises them, which means it turns each character into a bitmap. It sends the console those glyphs at the size of the console's character cell, the box on the screen that holds one character. The glyphs come ready to draw, with their map from Unicode characters to glyphs and their metrics, the measurements that place each glyph in the cell.

The console checks each answer from the font helper. It keeps the glyphs in its own memory, and draws them. Nothing is rendered ahead of time. vtium writes no glyph to disk, so the glyphs are gone when vtium stops.

The font terms come from glassmaking. [why-vtium.md](../why-vtium.md) explains why, for a newcomer.

The figure below follows a font from the moment you choose it to the screen. Each command in the figure is a vtium config command, and the color of each box says which user that part runs as.

![How a font reaches the console. On the left, you work at a terminal as yourself. vtium config preview draws a sample of each font and writes nothing, and you pick fonts from what it shows. vtium config new-frit makes a font set of the fonts you name, or takes a file you wrote from its --template. You can also write the file by hand. Installing the file asks for sudo once. The file is the frit, a font set file at /etc/vtium/fonts/NAME.vtfrit. It names the font's sources in order, and root owns it. On the right, vtium, the console, runs as root. It reads the frit once the frit passes the same ownership check that vtium.conf passes. It holds the built-in fonts, Unifont and Unscii-8, which are compiled into vtium. They draw the first frame, and each character the glyph store lacks. The glyph store is in vtium's memory, within a limit set by the machine's memory, and nothing goes to disk. vtium draws the screen from the store and from the built-in fonts. Below vtium, the font helper, vtium-config serve, runs as nobody in a sandbox. vtium sends the font helper the frit's text and the characters to draw. The font helper reads the console fonts, font files and installed fonts that the frit names, and draws their glyphs. It sends back glyphs at the character cell's size, and vtium checks each one. At the bottom left, in your session at the console, vtium config use-frit NAME sends OSC 50 through the pty to vtium. That switches the console to another font set or to a built-in font, while allow-font-ops is on, until the session ends.](../figures/concepts-fonts-path.svg)

## Try many, keep one

Someone choosing a font tries many and keeps one. `vtium config preview` renders a sample of each font it names, the way the font helper would render it. It uses the size that its `--size` option gives. Without `--size`, it uses the font size in vtium.conf. In a terminal that shows kitty graphics images, it shows the sample as an image. In other terminals, it prints a line of text for each font. It writes nothing.

## The frit

A font is declared in a **frit**. The name comes from glassmaking, where frit is the ground glass that a glassmaker melts again. A frit is a small text file, `/etc/vtium/fonts/NAME.vtfrit`. It names the font's sources in order, and a later source fills in only the characters that the earlier sources do not have. The frit also names the character cell the font is drawn in.

A frit's sources can be any of these:

- console fonts
- font files
- the kernel's own font
- outline fonts, color emoji among them, which the font helper finds among the fonts installed on the machine by their family name

There are three ways to write a frit, and all three give the same file:

- You can write it by hand.
- `vtium config new-frit FONT...` makes a frit of the fonts you name, in the order you name them. `vtium config new-frit` with no fonts, run at a terminal, opens vtium config's full-screen tool at its new-frit view.
- `vtium config new-frit --template` prints a frit with each key explained. `vtium config new-frit --from FILE` installs the frit you wrote from that template. This way, a script or a package can make a frit without answering questions.

Installing a frit asks for sudo once. The tool shows you the command before it runs it.

vtium reads a frit only when it passes the same ownership check as vtium.conf. Root must own the frit, and no other user may be able to change it or any folder above it. When a frit fails that check, vtium does not read it. vtium logs the file and the reason, and draws with its built-in fonts. `vtium check-config` reports the same refusal.

`vtium-config --help about` draws the tool's name in glass. See [the glass banner issue](https://github.com/vtium/vtium/issues/112).

Later, vtium-config may find installed fonts that match the looks people choose, without a frit written for each. A look is a named set of font settings in vtium.conf, such as a font and its size. That direction is not decided yet. See [the issue about matching installed fonts](https://github.com/vtium/vtium/issues/117).

## Drawing at run time

When vtium starts, it draws the first frame at once in the built-in font, in the character cell that the frit states. The frit's glyphs then replace the built-in ones, character by character, as the font helper sends them. The font helper sends the characters on the screen first. While the console asks for nothing else, it goes on to printable Latin characters and box-drawing characters. After those, it sends the rest of the characters the frit covers.

The glyphs live in a store in vtium's memory. The store's limit depends on how much memory the machine has. When the store reaches its limit, the font helper stops filling it. Only a request for characters on the screen makes room in a full store. It evicts the glyphs least recently shown first.

The font helper starts the first time the console needs it, and exits when it has nothing to do. If it crashes, vtium starts it again, up to a limited number of times per boot.

## The fallback and switching

The console uses the built-in fonts in two cases, and it logs the reason. In the first case, the configured font names no frit, and no font that vtium can make a frit from on the fly. In the second case, the font helper is not available.

The built-in fonts are compiled into vtium. They are Unifont, and Unscii-8 for character cells under 16 pixels high. They also draw each character that a frit does not have.

vtium-config's use-frit command switches the running console to another frit or to a built-in font. It does this through the terminal, with xterm's font control sequence, OSC 50. vtium acts on that sequence while its allow-font-ops setting is on, as it is by default.

A switch lasts until the session it came from ends. The session ends when its program exits, when its console closes, or when vtium hangs it up. vtium then returns the console to the font the configuration gives.
