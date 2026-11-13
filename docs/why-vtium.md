# Why VTium

vtium is a terminal for the Linux console. The console is the set of full-screen text screens that a Linux machine shows before a desktop starts, or instead of a desktop. This page is for you if you are meeting vtium for the first time, as a packager or a curious Linux user, for example. It answers two questions. First, why use vtium rather than the kernel's own console, `kmscon`, or a terminal on a desktop? Second, why does vtium suit the console on every kind of machine, from a small board with a 320×240 panel to a desktop with a large monitor? [Concepts](concepts/index.md) explains how vtium's parts work. [features.md](features.md) lists what is built today.

See [the releases](https://github.com/vtium/vtium/releases) for the latest version. Everything this page describes is built in the repository, except what it marks as landing. Landing means designed and being built, but not finished yet.

## Why another terminal

### Beside the kernel's own console

Every Linux machine with a screen already has a console. The kernel draws text on its VTs through its framebuffer console, fbcon. VTs are virtual terminals, the screens you switch between with Ctrl-Alt and a function key. fbcon needs no program at all, and it is there from the first moment of boot. vtium keeps fbcon as the fallback. fbcon is deliberately simple:

- It turns 256-color and 24-bit color requests into 16 foreground and 8 background colors.
- It holds at most 512 glyphs of a console font at once.
- It shows no images.

vtium renders the VT itself instead. It gives programs these:

- 24-bit color.
- Bold, faint and reversed text, and five underline styles.
- Unicode, with double-width Chinese, Japanese and Korean text, and color emoji.
- Mouse reporting.
- Scrollback that rewraps when the screen's width changes.

In a build with the `images` feature, vtium also shows images sent with sixel, the kitty graphics protocol, or iTerm2's inline images.

### Where vtium began

vtium began as a Rust program derived from `kmscon` and its library `libtsm`. `kmscon` is a terminal for the Linux console, written in C.

### Beside a terminal on a desktop

A terminal on a desktop, such as foot, kitty or GNOME Terminal, runs inside a display server and a desktop session. vtium needs neither. It renders straight to the display through the kernel's display interface, KMS/DRM. On a machine that has only the older fbdev, it uses fbdev. So vtium is the terminal for the places where there is no desktop: a server, a small board, a kiosk, a recovery shell, or a machine before its desktop starts. On a machine that has a desktop, vtium runs on its own VT, and the desktop runs on another. The two share the graphics card, and vtium hands the display over at each switch between them.

## vtium beside the alternatives

The table lists the other ways to get a terminal on a Linux console without a display server. Each row comes from the sources listed under the table. vtium's row includes what is landing ([features.md](features.md#status)). It also includes what vtium draws only in a build with a particular feature.

| | Draws through | Colors | Fonts | Pictures | Latest release |
|---|---|---|---|---|---|
| The kernel's VT console (fbcon) | the kernel's framebuffer device | 256-color and 24-bit requests are "shoehorned into" 16 foreground and 8 background colors | console bitmap fonts, at most 512 glyphs at once; with more than 256, only 8 of the 16 colors, as kbd's `setfont(8)` says | none in `console_codes(4)` | ships with the kernel |
| `kmscon/kmscon` 10.1.0, with `kmscon/libtsm` 4.8.0 | KMS/DRM, in software or through OpenGL ES 2, or fbdev | 24-bit | FreeType, Pango, PSF, and a built-in 8x16 font and Unifont | sixel is an open feature request | 10.1.0, 2026-10-26 |
| fbterm 1.7 | fbdev, or a VESA video card | 256, through its own private sequences under `TERM=fbterm` | fontconfig and FreeType | none listed | 1.7, 2010-10-06 |
| yaft 0.2.9 | fbdev at 8 to 32 bits per pixel, on Linux and FreeBSD, and on NetBSD and OpenBSD as experimental | 256 | BDF fonts converted into its source when it is built | sixel and DRCS, marked experimental | tag v0.2.9; last commit 2018-11-14 |
| fbpad | Linux fbdev | 256 | its own tinyfont format and PSF2 | none listed | last commit 2026-10-23 |
| vtium | KMS/DRM dumb buffers drawn on the CPU, or on the GPU in a build with the `gpu` feature and `hwaccel` on, or fbdev | 24-bit | PSF, kbd, BDF, outline and color fonts, drawn by a helper that never runs as root; built-in Unifont and Unscii-8 | sixel, kitty graphics with animation, and iTerm2 inline images, in a build with the `images` feature | 0.1.0-alpha.4, a pre-release |

Sources, one per row:

- The kernel's VT console: [`console_codes(4)`](https://man7.org/linux/man-pages/man4/console_codes.4.html), from man-pages 6.19, and kbd's [`setfont(8)`](https://man7.org/linux/man-pages/man8/setfont.8.html), from kbd's repository as of 2026-08-04. The row also rests on [the fbcon documentation](https://docs.kernel.org/fb/fbcon.html) and on [the commit that removed fbcon's soft scrollback](https://github.com/torvalds/linux/commit/50145474f6ef4a9c19205b173da6264a644c7489), first in Linux 5.9.
- `kmscon/kmscon` and `kmscon/libtsm`: their READMEs and release notes for `kmscon` 10.0.0 to 10.1.0 and `libtsm` 4.8.0, and `kmscon`'s open feature request for sixel, its issue 138.
- fbterm: [its archived project page](https://code.google.com/archive/p/fbterm/), whose news dates release 1.7 to 2010-10-06, and the man page in its 1.7 source.
- yaft: [its repository](https://github.com/uobikiemukot/yaft), README and tags.
- fbpad: [its repository](https://github.com/aligrudi/fbpad) and README.
- vtium: [features.md](features.md) and the records it links.

Beyond what the table shows, the other consoles do these things that vtium does not:

- The kernel console needs no program in user space. fbcon takes over the console by itself.
- `kmscon` sets each monitor's rotation in its configuration, while vtium rotates a head with a key. `kmscon` also follows the keyboard layout set through `localectl`.
- fbterm has a client-server input method framework and up to 10 windows.
- yaft runs on the BSD consoles.
- fbpad keeps two terminals to a tag and splits the screen between them.

vtium has heads. A head is a terminal for one or more displays. vtium places each display and input device in a head by the first rule in its configuration that matches the device. Its closed policy, for kiosks, ignores any keyboard or pointer that no rule names. vtium's scrollback is bounded in bytes, and it rewraps when the screen's width changes. The kernel console's soft scrollback (Shift+PageUp) was removed in Linux 5.9. vtium also does these things:

- **Fonts are read by a separate, locked-down program.** The console never opens a font file. A helper program reads the fonts in a sandbox, and it never runs as root. The console only checks and draws the glyphs that the helper sends. `kmscon` loads and draws its fonts in its own process, which runs as root.
- **It is built so that a program's output cannot crash it.** A panic is how a Rust program stops on an unexpected state. In the parts of vtium that read a program's output, the build denies every panic that the output could reach. Only the part that talks to the kernel may use Rust's `unsafe`. [architecture.md](architecture.md) lists the check behind each rule.
- **It is measured on a small machine.** Its tests run the console in a virtual machine with one CPU and 512 MB, and record the console's peak memory there. The tests fail if the console's program grows past 4 MiB. They also draw on a 320×240 display.
- **It is tested without systemd.** vtium runs from systemd's units. It also runs from sysvinit's inittab, from a script, or by hand. Its tests boot it under systemd, and on Debian 13 with sysvinit.

[How vtium is judged](concepts/how-vtium-is-judged.md) describes how vtium is tested.

## Where vtium sits in a machine's boot

The figure follows a Linux machine from power on. The left column shows the kernel's console alone, and the right column shows the machine with vtium. First, the kernel starts, and its console shows the kernel's messages. Then init, such as systemd or sysvinit, starts the services. On each text VT where a getty would print a login prompt, vtium starts instead. vtium takes the VT and the display, and starts the system's login program. After you log in, your shell runs in vtium. A desktop, if the machine has one, runs on a VT of its own.

![Two columns, the kernel's console alone and with vtium, in five steps. 1, power on: the kernel starts, and its own console, fbcon, shows its messages. 2, init starts the services: systemd, sysvinit or another init. 3, on each text VT: on the left, getty prints a login prompt and fbcon draws it; on the right, vtium takes the VT and the display and starts login, with a dashed arrow to the left box. 4, after login: a shell drawn by the kernel's console, or a shell drawn by vtium in its own fonts and colors. 5, a desktop, if there is one, runs on a VT of its own, and Ctrl-Alt and a function key switch VTs. A note says the dashed arrow means that when vtium cannot start, for example with no display, a getty takes its VT.](figures/why-vtium-boot.svg)

Several things make vtium suit this early stage of the boot:

- **It shows text at once.** vtium carries two bitmap fonts inside its own program, Unscii-8 and Unifont. It draws its first screen with them. Meanwhile, its font helper reads the fonts that its frit names. A frit is a font set file, described below.
- **It never leaves a dead screen.** When vtium cannot start, for example because no display appears, it exits with a failure. With systemd, vtium's unit then starts a getty on that VT, unless the machine is set up as a locked-down kiosk. Under another init system, the line that starts vtium in that init's configuration decides what happens next.
- **It needs no GPU.** A display that is plugged in after vtium starts also joins the console.

[systemd.md](systemd.md) lists each place vtium touches systemd, logind or init, and what it does without them.

## From a small board to a large monitor

vtium is built for small machines first. Its default build, called minspec, is the smallest build that is still a complete console. It renders on the CPU into a plain memory buffer that the display shows, and it uses bitmap fonts. minspec is measured against a single-board computer with ARM cores and 512 MB of memory or less. Its smallest display is a 320×240 panel, where an 8×8 font gives 40 columns by 30 rows. The same build renders on large monitors too. There, a frit lets it draw desktop fonts with color emoji. A frit is a font set file, described below.

vtium's test card is a fixed set of sample screens. The figure shows, to scale, five of the displays that vtium draws the test card at. For each display, it shows the character cell size and the grid that the display gets. Where a test run measures it, the figure also shows the console's peak memory. The grids and fonts are those of vtium's test card for each display ([specimens](specimens/README.md)). The memory figures come from a test run on 2026-11-01 ([features.md](features.md#small-machines)).

![Five displays drawn to scale, one pixel for twelve. 320x240, a small panel: 8x8 cells, 40 by 30, 5.8 MiB. 640x480, an old monitor: 8x16 cells, 80 by 30, 7.8 MiB. 1366x768, a laptop: 8x16 cells, 170 by 48, 12.1 MiB. 1920x1080, a desktop monitor: 11x24 cells, 174 by 45. 2560x1440, a large monitor: 14x32 cells, 182 by 45. MiB is the console's peak memory, measured in a virtual machine with 512 MB. Beneath, the first three use the built-in fonts, Unscii-8 and Unifont, and the last two a frit of outline and color fonts; one bar under all five says the same build is used at every size, drawn on the CPU with no display server; a dashed bar under the last two says pictures and a GPU renderer are landing.](figures/why-vtium-spectrum.svg)

A frit is a small text file that names the fonts vtium draws from, such as a code font, a font of symbols, and a color emoji font. It also names the cell size to draw them at. vtium-config, the font tool, reads those fonts. It turns them into the glyphs, the drawn letters, that vtium puts on the screen. vtium draws any character that a frit lacks from its built-in fonts. vtium draws box-drawing lines and blocks itself, so that they join at every size. [Fonts, generated at run time](concepts/fonts.md) follows a font from the frit to the screen.

## Why glass

vtium's name, and its words for fonts, come from glass. Glass links them to the history of terminals.

- **VT and TTY.** A teletype, TTY for short, was a terminal that printed on paper. Linux still names its terminal devices `tty`. DEC named its video terminals VT, as in the VT100 of 1978 ([Wikipedia, VT100](https://en.wikipedia.org/wiki/VT100)). The Linux console still understands "a large subset of the VT102" terminal's controls ([`console_codes(4)`](https://man7.org/linux/man-pages/man4/console_codes.4.html)).
- **Glass TTY.** Early terminals with a screen in place of paper, such as DEC's VT05, were often called glass TTYs. They were the video terminals that replaced paper teletypes ([Wikipedia, Computer terminal](https://en.wikipedia.org/wiki/Computer_terminal)).
- **The name vtium.** vtium echoes vitrum, Latin for glass. vtium is a glass terminal for the VT, and its name keeps the link to the glass TTYs. The maintainer also chose the name for its sound. It replaced the working title, kmscon-rust, so that vtium has a name of its own beside the Rust `kmscon` that `kmscon`'s developers plan.
- **Vitrify.** To vitrify is to turn something into glass by heat. The word comes from the same Latin vitrum ([Wiktionary, vitrify](https://en.wiktionary.org/wiki/vitrify)). vtium's font tool, vtium-config, turns fonts into the glyphs on the screen. Its banner names it the VTium Vitrifier.
- **Frit.** A glassmaker's frit is a mix of raw materials that has been fused, cooled and ground. The glassmaker melts it into glass ([Wikipedia, Frit](https://en.wikipedia.org/wiki/Frit)). The maintainer named vtium's frit after it on 2026-10-28.

The figure sets the two side by side. The parallel has one gap. A glassmaker's frit is the material itself, while vtium's frit only names its fonts.

![Two rows side by side. Above, in glassmaking: raw materials (sand and fluxes), then frit (the mix, fused, cooled and ground), then melting (the frit vitrifies: it turns to glass), then glass (clear, and set in its shape). Below, in vtium, each joined to the step above it by a dotted line: font files (PSF, BDF, kbd and outline fonts), then NAME.vtfrit (fonts, characters and the cell), then vtium-config (reads the fonts and draws their glyphs), then glyphs (bitmaps at the cell, on the screen). A note says a glassmaker's frit is the material itself, and a vtium frit is a short text file that names the fonts to melt down and holds no font data.](figures/why-vtium-glass.svg)
