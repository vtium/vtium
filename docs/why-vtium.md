# Why VTium

vtium is a terminal that runs on the Linux console. The Linux console is the set of full-screen text screens that a Linux machine shows before a desktop starts, or instead of a desktop. This page is for you if you are meeting vtium for the first time, as a packager or a curious Linux user, for example. It answers two questions. First, what does vtium give the console that a machine boots into? Second, why does vtium suit that console on every kind of machine, from a small board with a 320×240 panel to a desktop with a large monitor? [Concepts](concepts/index.md) explains how vtium's parts work. [What vtium does](features.md) lists what is built today.

See [the releases](https://github.com/vtium/vtium/releases) for the latest version.

## What vtium gives the console

Every Linux machine with a screen has a console. Its screens are VTs, virtual terminals, which you switch between with Ctrl+Alt and a function key. vtium runs on a VT in place of getty, the program that normally shows the login prompt there, and renders that VT itself. Programs in it get these:

- 24-bit color.
- Bold, faint and reversed text, and five underline styles.
- Unicode, with double-width Chinese, Japanese and Korean text, and color emoji.
- Mouse reporting.
- Scrollback that has a limit in bytes and rewraps when the screen's width changes.

In a build with the `images` feature, vtium also shows images sent with sixel, the kitty graphics protocol, or iTerm2's inline images.

vtium renders straight to the display through the kernel's display interface, KMS/DRM. It can render entirely on the CPU, so a GPU is optional. On a machine that has only the older fbdev, it renders through fbdev. So vtium serves the places where you work at the machine's own screen: a server, a small board, a kiosk, a recovery shell, or a machine before its desktop starts. On a machine that has a desktop, vtium runs on its own VT, and the desktop runs on another. The two share the graphics card, and vtium hands the display over at each switch between them.

vtium began as a Rust port of two programs written in C, a history that [Provenance](vtium-in-depth.md#provenance) tells in full.

## Built for the console

vtium has heads. A head is a terminal for one or more displays. vtium places each display and input device in a head by the first rule in its configuration file, vtium.conf, that matches the device. The line `policy = closed` in vtium.conf makes vtium ignore any keyboard or pointer that no rule names, as a kiosk needs. vtium also does these things:

- **Fonts are read by a separate, locked-down program.** vtium's main process never opens a font file. A helper program reads the fonts in a sandbox and never runs as root. vtium only checks and draws the glyphs that the helper sends.
- **It is built so that a program's output cannot crash it.** A panic is how a Rust program stops on an unexpected state. In the parts of vtium that read a program's output, a lint check refuses the operations that commonly panic, such as an unchecked index, unchecked arithmetic or an unwrap. Every change must pass that check. Only the part that talks to the kernel may use Rust's `unsafe`. [Architecture](architecture.md) lists the check behind each rule.
- **It is measured on a small machine.** Its tests run vtium in a virtual machine with one CPU and 512 MB, and record vtium's peak memory there. The tests fail if the minspec binary grows past 4 MiB and 128 KiB. They also draw on a 320×240 display.
- **It is tested without systemd.** vtium runs from systemd's units. It also runs from sysvinit's inittab, from a script, or by hand. Its tests boot it under systemd, and on Debian 13 with sysvinit.

[How vtium is judged](concepts/how-vtium-is-judged.md) describes how vtium is tested.

## Where vtium sits in a machine's boot

The figure follows a Linux machine from power on. The left column shows the kernel's console alone, and the right column shows the machine with vtium. First, the kernel starts, and its console shows the kernel's messages. Then init, such as systemd or sysvinit, starts the services. On each text VT where a getty would print a login prompt, vtium starts instead. vtium takes the VT and the display, and starts the system's login program. After you log in, your shell runs in vtium. A desktop, if the machine has one, runs on a VT of its own.

![Two columns, the kernel's console alone and with vtium, in five steps. 1, power on: the kernel starts, and its own console, fbcon, shows its messages. 2, init starts the services: systemd, sysvinit or another init. 3, on each text VT: on the left, getty prints a login prompt and fbcon draws it; on the right, vtium takes the VT and the display and starts login, with a dashed arrow to the left box. 4, after login: a shell drawn by the kernel's console, or a shell drawn by vtium in its own fonts and colors. 5, a desktop, if there is one, runs on a VT of its own, and Ctrl-Alt and a function key switch VTs. A note says the dashed arrow means that when vtium cannot start, for example with no display, a getty takes its VT.](figures/why-vtium-boot.svg)

Several things make vtium suit this early stage of the boot:

- **It shows text at once.** vtium carries three bitmap fonts inside its own program: Unscii-8, Unifont, and a small font of its own cut from Unscii-8 and Terminus. It draws its first screen with them. Meanwhile, its font helper reads the fonts that its frit names. A frit is a font set file, a short text file that lists fonts in order.
- **It never leaves a dead screen.** When vtium cannot start, for example because no display appears, it exits with a failure. With systemd, vtium's unit then starts a getty on that VT. It starts none when vtium.conf sets `policy = closed`, or when the getty would stop another program, such as a display manager. Under another init system, the line that starts vtium in that init's configuration decides what happens next.
- **It follows displays as they arrive.** vtium also starts drawing on a display that is plugged in after it starts.

[systemd, logind and PID 1](systemd.md) lists each place vtium touches systemd, logind or init, and what it does without them.

## From a small board to a large monitor

vtium is built for small machines first. Its default build, called minspec, is the smallest build that is still a complete terminal. It renders on the CPU into a plain memory buffer that the display shows, and it carries three bitmap fonts of its own. minspec is measured against a single-board computer with ARM cores and 512 MB of memory or less. Its smallest display is a 320×240 panel, where an 8×8 font gives 40 columns by 30 rows. The same build renders on large monitors too. There, a frit lets it draw desktop fonts with color emoji.

vtium's test card is a fixed set of sample screens. The figure shows, to scale, five of the displays that vtium draws the test card at. For each display, it shows the character cell size and the grid that the display gets. Where a test run measures it, the figure also shows vtium's peak memory. The grids and fonts are those of vtium's test card for each display ([specimens](specimens/README.md)). The memory figures come from a test run on 2026-11-01. See [Small machines](features.md#small-machines) for details.

![Five displays drawn to scale, one pixel for twelve. 320x240, a small panel: 8x8 cells, 40 by 30, 5.8 MiB. 640x480, an old monitor: 8x16 cells, 80 by 30, 7.8 MiB. 1366x768, a laptop: 8x16 cells, 170 by 48, 12.1 MiB. 1920x1080, a desktop monitor: 11x24 cells, 174 by 45. 2560x1440, a large monitor: 14x32 cells, 182 by 45. MiB is vtium's peak memory with minspec, measured in a virtual machine with 512 MB. Beneath, the test card uses the built-in fonts, Unscii-8 and Unifont, on the first three, and a frit of outline and color fonts on the last two. A bar under all five says that minspec, the default build, draws every size on the CPU through KMS/DRM, frits included. A dashed bar under all five says that at every size, a build with more features adds images, right-to-left text, and the GPU path.](figures/why-vtium-spectrum.svg)

A frit names the fonts vtium draws from, such as a code font, a font of symbols, and a color emoji font. It also names the cell size to draw them at. vtium-config, vtium's setup tool, reads those fonts while vtium runs. It turns them into the glyphs, the drawn letters, that vtium puts on the screen. vtium draws any character that a frit lacks from its built-in fonts. vtium draws box-drawing lines and blocks itself, so that they join at every size. [Fonts, generated at run time](concepts/fonts.md) follows a font from the frit to the screen.

## Why glass

vtium's name, and its words for fonts, come from glass. Glass links them to the history of terminals.

- **VT and TTY.** A teletype, TTY for short, was a terminal that printed on paper. Linux still names its terminal devices `tty`. DEC named its video terminals VT, as in the VT100 of 1978 ([Wikipedia, VT100](https://en.wikipedia.org/wiki/VT100)). The Linux console still understands "a large subset of the VT102" terminal's controls ([`console_codes(4)`](https://man7.org/linux/man-pages/man4/console_codes.4.html)).
- **Glass TTY.** Early terminals with a screen in place of paper, such as DEC's VT05, were often called glass TTYs. They were the video terminals that replaced paper teletypes ([Wikipedia, Computer terminal](https://en.wikipedia.org/wiki/Computer_terminal)).
- **The name vtium.** vtium echoes vitrum, Latin for glass. vtium is a glass terminal for the VT, and its name keeps the link to the glass TTYs. The maintainer also chose the name for its sound. It replaced the working title, kmscon-rust, which named kmscon, one of the two C programs that vtium's code began as a port of. The new name gives vtium a name of its own.
- **Vitrify.** To vitrify is to turn something into glass by heat. The word comes from the same Latin vitrum ([Wiktionary, vitrify](https://en.wiktionary.org/wiki/vitrify)). vtium's setup tool, vtium-config, also turns fonts into the glyphs on the screen. Its banner names it the VTium Vitrifier.
- **Frit.** A glassmaker's frit is a mix of raw materials that has been fused, cooled and ground. The glassmaker melts it into glass ([Wikipedia, Frit](https://en.wikipedia.org/wiki/Frit)). The maintainer named vtium's frit after it on 2026-10-28.

The figure sets the two side by side. The parallel has one gap. A glassmaker's frit is the material itself, while vtium's frit only names its fonts.

![Two rows side by side. Above, in glassmaking: raw materials (sand and fluxes), then frit (the mix, fused, cooled and ground), then melting (the frit vitrifies: it turns to glass), then glass (clear, and set in its shape). Below, in vtium, each joined to the step above it by a dotted line: font files (PSF, BDF, kbd and outline fonts), then NAME.vtfrit (fonts, characters and the cell), then vtium-config (reads the fonts and draws their glyphs), then glyphs (bitmaps at the cell, on the screen). A note says a glassmaker's frit is the material itself, and a vtium frit is a short text file that names the fonts to melt down and holds no font data.](figures/why-vtium-glass.svg)
