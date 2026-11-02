# Why VTium

vtium is a terminal for the Linux console: the full-screen text screens a Linux machine shows before, or instead of, a desktop. This page is for someone meeting vtium for the first time, such as a packager or a curious Linux user. It answers two questions: why use vtium rather than the kernel's own console, kmscon or a terminal on a desktop, and why vtium suits the console on every kind of machine, from a small board with a 320x240 panel to a desktop with a large monitor. [concepts.md](concepts.md) explains how vtium's parts work, and the [README](../README.md) lists what is built today.

vtium has no release yet. Everything this page describes is built in the repository, except what it marks as landing.

## Why another terminal

### Beside the kernel's own console

Every Linux machine with a screen has a console already: the kernel draws text on its VTs (virtual terminals, the screens Ctrl-Alt and a function key switch between) through its framebuffer console, fbcon. fbcon needs no program at all and is there from the first moment of boot, and vtium keeps it as the fallback. It is deliberately simple: it turns 256-color and 24-bit color requests into 16 foreground and 8 background colors, it holds at most 512 glyphs of a console font at once, and it shows no pictures.

vtium draws the VT itself instead. It gives programs 24-bit color, bold, faint and reversed text and five underline styles, Unicode with double-width Chinese, Japanese and Korean text and color emoji, mouse reporting, and scrollback that rewraps when the screen's width changes. Pictures, sent with sixel, the kitty graphics protocol or iTerm2's inline images, are landing.

### Beside kmscon

`kmscon` is the C terminal for the Linux console that vtium learned from. vtium is tested against what `kmscon` and its library `libtsm` did, and it reads `kmscon`'s configuration file with its settings, so a machine's `kmscon` configuration carries over. vtium is a new program in Rust rather than a version of `kmscon`, and it has a name of its own because `kmscon`'s developers plan a Rust `kmscon` of their own.

What vtium adds:

- **Fonts are read by a separate, locked-down program.** The console never opens a font file. A helper program reads the fonts in a sandbox and never runs as root, and the console only checks and draws the glyphs it sends. `kmscon` loaded and drew its fonts in its own process, which ran as root.
- **It is built so that a program's output cannot make it panic.** The parts that read a program's output deny every panic that output could reach, and only the part that talks to the kernel may use Rust's `unsafe`; [architecture.md](architecture.md) lists the check behind each rule.
- **It is measured on a small machine.** Its tests boot vtium in a virtual machine with one CPU and 512 MB, record its memory, and fail if the console's program grows past 3.5 MiB.
- **It does not need systemd.** vtium runs from systemd's units, and also from sysvinit's inittab, a script or by hand. Tests boot it both ways.
- **Pictures are landing,** where `kmscon` has an open request for sixel.

### Beside a terminal on a desktop

A terminal on a desktop, such as foot, kitty or GNOME Terminal, runs inside a display server and a desktop session. vtium needs neither: it draws straight to the display through the kernel's display interface (KMS/DRM, or the older fbdev where that is all a machine has). That makes it the terminal for the places a desktop is not: a server, a small board, a kiosk, a recovery shell, or a machine before its desktop starts. On a machine that has a desktop, vtium runs on its own VT and shares the graphics card with the desktop on another, handing the display over at each switch.

The [README](../README.md#vtium-beside-the-alternatives) compares vtium with the kernel's console, kmscon, fbterm, yaft and fbpad in a table, with a source for each row.

## Where vtium sits in a machine's boot

The figure follows a Linux machine from power on, with the kernel's console alone on the left and with vtium on the right. The kernel starts and its console shows the kernel's messages. Then init (systemd, sysvinit or another) starts the services. On each text VT where a getty would print a login prompt, vtium starts instead: it takes the VT and the display and starts the system's login program, and the shell after login runs in vtium. A desktop, if the machine has one, runs on a VT of its own.

![Two columns, the kernel's console alone and with vtium, in five steps. 1, power on: the kernel starts, and its own console, fbcon, shows its messages. 2, init starts the services: systemd, sysvinit or another init. 3, on each text VT: on the left, getty prints a login prompt and fbcon draws it; on the right, vtium takes the VT and the display and starts login, with a dashed arrow to the left box. 4, after login: a shell drawn by the kernel's console, or a shell drawn by vtium in its own fonts and colors. 5, a desktop, if there is one, runs on a VT of its own, and Ctrl-Alt and a function key switch VTs. A note says the dashed arrow means that when vtium cannot start, for example with no display, a getty takes its VT.](figures/why-vtium-boot.svg)

Several things make vtium fit this early place in the boot:

- **It shows text at once.** vtium carries two bitmap fonts inside its own program, Unscii-8 and Unifont, and draws its first screen with them while it reads any other font in the background.
- **It never leaves a dead screen.** When vtium cannot start, for example because no display appears, it exits with a failure and the system starts a getty on that VT instead, unless the machine is set up as a locked-down kiosk.
- **It needs little.** vtium needs no display server, no GPU and no systemd, and a display plugged in after it starts joins the console.

[systemd.md](systemd.md) lists each place vtium touches systemd, logind or init, and what it does without them.

## From a small board to a large monitor

vtium is built for small machines first. Its default build, called minspec, is the smallest complete console: it draws on the CPU into a plain memory buffer the display shows, and uses bitmap fonts. Its yardstick is a single-board computer with ARM cores and 512 MB of memory or less, down to a 320x240 panel, where an 8x8 font gives 40 columns by 30 rows. The same build draws on large monitors too, where a frit (below) lets it draw desktop fonts with color emoji.

The figure shows the displays vtium's test card is drawn at, to scale, with the cell size and the grid each one gets, and the console's peak memory where a test run measures it. The grids and fonts are those of vtium's test card for each display ([specimens](specimens/README.md)), and the memory figures come from a test run on 2026-11-01 ([README](../README.md#small-machines)).

![Five displays drawn to scale, one pixel for twelve. 320x240, a small panel: 8x8 cells, 40 by 30, 5.8 MiB. 640x480, an old monitor: 8x16 cells, 80 by 30, 7.8 MiB. 1366x768, a laptop: 8x16 cells, 170 by 48, 12.1 MiB. 1920x1080, a desktop monitor: 11x24 cells, 174 by 45. 2560x1440, a large monitor: 14x32 cells, 182 by 45. MiB is the console's peak memory, measured in a virtual machine with 512 MB. Beneath, the first three use the built-in fonts, Unscii-8 and Unifont, and the last two a frit of outline and color fonts; one bar under all five says the same build is used at every size, drawn on the CPU with no display server; a dashed bar under the last two says pictures and a GPU renderer are landing.](figures/why-vtium-spectrum.svg)

A frit is a small text file that names the fonts vtium draws from, such as a code font, a font of symbols and a color emoji font, and the cell size to draw them at. vtium-vitrify, the font tool, reads those fonts and turns them into the glyphs, the drawn letters, that vtium puts on the screen. Any character a frit lacks is drawn from the built-in fonts, and vtium draws box-drawing lines and blocks itself, so they join at every size. [concepts.md](concepts.md#fonts-generated-at-run-time) follows a font from the frit to the screen.

## Why glass

vtium's font words come from glassmaking, to keep a connection to the history of terminals, in which the early screen terminals were called glass TTYs.

- **VT and TTY.** A teletype, TTY for short, was a terminal that printed on paper, and Linux still names its terminal devices `tty`. DEC's video terminals were named VT, such as the VT100 of 1978 ([Wikipedia, VT100](https://en.wikipedia.org/wiki/VT100)), and the Linux console still speaks "a large subset of the VT102" terminal's controls ([`console_codes(4)`](https://man7.org/linux/man-pages/man4/console_codes.4.html)).
- **Glass TTY.** Early terminals with a screen in place of paper, DEC's VT05 among them, were often called glass TTYs ([Wikipedia, Computer terminal](https://en.wikipedia.org/wiki/Computer_terminal)). vtium is a glass terminal for the VT.
- **The name vtium.** The maintainer chose it for its sound, in place of the working title kmscon-rust ([the record of that choice, in the kmscon fork](https://github.com/aaronsb/kmscon/blob/main/docs/architecture/fork/ADR-304-vtium-a-full-rust-port-of-kmscon-and-libtsm-minspec-first-judged-by-the-existing-tiers.md)). <!-- adr-cite-ignore: the kmscon fork's record, not one of vtium's -->
- **Vitrify.** To vitrify is to turn something into glass by heat, from vitrum, Latin for glass ([Wiktionary, vitrify](https://en.wiktionary.org/wiki/vitrify)). vtium-vitrify turns fonts into the glyphs on the screen.
- **Frit.** A glassmaker's frit is the mix of raw materials, fused, cooled and ground, that is melted into glass ([Wikipedia, Frit](https://en.wikipedia.org/wiki/Frit)). vtium's frit is the list of fonts that vtium-vitrify melts down. The maintainer gave the frit its name on 2026-10-28, after the glassmaker's ground glass that is melted again.

The figure sets the two side by side. The parallel has one gap: a glassmaker's frit is the material itself, and vtium's frit only names its fonts.

![Two rows side by side. Above, in glassmaking: raw materials (sand and fluxes), then frit (the mix, fused, cooled and ground), then melting (the frit vitrifies: it turns to glass), then glass (clear, and set in its shape). Below, in vtium, each joined to the step above it by a dotted line: font files (PSF, BDF, kbd and outline fonts), then NAME.vtfrit (fonts, characters and the cell), then vtium-vitrify (reads the fonts and draws their glyphs), then glyphs (bitmaps at the cell, on the screen). A note says a glassmaker's frit is the material itself, and a vtium frit is a short text file that names the fonts to melt down and holds no font data.](figures/why-vtium-glass.svg)
