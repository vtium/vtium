# Choose a font

This guide sets the font vtium draws in. vtium draws fonts from frits: a frit, or font set file, is a short text file, `/etc/vtium/fonts/NAME.vtfrit`, that lists a font's sources in order. [Your first frit](tutorial-first-frit.md) teaches the same thing step by step, and [concepts.md](concepts.md#fonts-generated-at-run-time) explains how a font reaches the screen.

The steps are, for now, in [managing-vtium.md](managing-vtium.md#fonts):

- [An outline font](managing-vtium.md#an-outline-font): writing a frit for a font such as Monoid with color emoji after it, and installing a font where vtium's font helper finds it.
- [A console font](managing-vtium.md#a-console-font) from kbd, which needs no frit.
- [The size](managing-vtium.md#size) of the font.
- [Choosing a font with vtium config](managing-vtium.md#choosing-a-font-with-vtium-config): trying each font on the live console and saving the one you keep.
- [Switching and zooming](managing-vtium.md#switching-and-zooming) for one session.

[crates/vtium-config/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium-config/README.md#frits) lists every source form and frit key.
