# VTium's terms

This page explains the words vtium uses for its own parts, and why so many of them come from glassmaking. Each term gets what it is in vtium, the same idea in everyday words, and where it lives in the tree or in the documents. [concepts.md](concepts.md) shows how the parts fit together, and the decision records and crate READMEs linked below hold the detail.

## The console and the machine

#### VT

A VT is one of the full-screen text terminals the Linux kernel provides, the ones Ctrl-Alt-F1 to F6 switch between. vtium runs on one VT and draws that VT's screen itself.

- **In everyday words:** one of the text screens a Linux machine has before any desktop starts.
- **Where it lives:** the kernel's `/dev/tty1`, `/dev/tty2` and so on, which the kernel's device list calls virtual consoles. [Why glass](#why-glass) tells where the letters VT come from.

#### The console

In vtium's documents, the console is the `vtium` program itself: one process for each VT, which owns that VT, its displays and its input devices, and runs the session. Inside a head (below), a console is also one session with its own login, screen and scrollback, and a head shows one of its consoles at a time.

- **In everyday words:** the terminal you see on the VT, and the program that draws it.
- **Where it lives:** `crates/vtium` ([its README](../crates/vtium/README.md)), and the suite table in [concepts.md](concepts.md#the-suite).

#### Minspec

Minspec is vtium's default and smallest build. It draws on the CPU into a plain memory buffer that the display shows, uses bitmap fonts and needs no GPU. Its yardstick is a single-board computer with ARM cores and 512 MB of memory or less, and its smallest display is a 320x240 panel.

- **In everyday words:** the version of vtium that runs on the smallest machine vtium aims for.
- **Where it lives:** `cargo build --profile minspec`, and [architecture.md](architecture.md#the-minspec-build), which sets out the machine and what is promised at each display size.

#### A head and the head file

A head is a terminal for one or more displays, with its own keyboards, its own grid and font, and a fixed number of consoles. The head file declares the heads and an ordered list of rules, read from the top as a firewall reads its rules, and the first rule that matches places each display, keyboard and pointer in a head.

- **In everyday words:** a head is one screen's worth of terminal, so a small panel and a large monitor on one machine each get their own; the head file says which keyboard types into which screen.
- **Where it lives:** `/etc/vtium/heads.conf`, decided in [ADR-1401](architecture/platform/ADR-1401-heads-a-terminal-per-physical-display-placed-by-a-first-match-ruleset.md), with a complete file for each setup in [heads.md](heads.md).

## Fonts

The figure follows a character from the font files on the machine to the cells on the screen. A frit names the font files. The helper, `vtium-vitrify serve`, reads them and draws their glyphs, which reach the console as a vitrified set. The console checks each glyph, keeps it for the rest of the boot and draws it into the cells. Two things inside vtium need no helper and no font file: the sprites, which vtium draws itself for line and block characters, and the built-in sets, which draw the first frame and every character the frit does not give.

![A flow from left to right and down. Font files (PSF, kbd, BDF, outline and color emoji) and a frit, NAME.vtfrit, which names the font files by a dashed line and states its sources, cover ranges and cell, are both read by vtium-vitrify serve, the helper, which runs in a sandbox and never as root. The helper draws a vitrified set, glyphs at the cell sent on a pipe. The set is checked by vtium, the console, which keeps each glyph for the boot and draws the screen, shown as a grid of cells holding a prompt, box drawing and shade blocks. Below, in a band marked inside vtium itself, the sprites (box drawing and blocks drawn to fill the cell) and the built-in sets (unifont and unscii-8, for the first frame and the gaps) feed the console directly.](figures/terminology-journey.svg)

#### Glyph

A glyph is the picture of one character, or of a cluster of characters drawn together, ready to draw into its cells. vtium holds a glyph as a mask of how much each pixel is covered, or as full color for color emoji, and it is one or two cells wide.

- **In everyday words:** the drawn shape of a letter.
- **Where it lives:** `Glyph` in vtium-font's `src/glyph.rs` ([crates/vtium-font](../crates/vtium-font/README.md)), and the way a glyph travels in [ADR-1200](architecture/font/ADR-1200-glyphs-are-generated-at-run-time-in-a-sandboxed-helper-from-a-profile-s-frit.md).

#### Cell

A cell is one place in the terminal's grid of rows and columns, a fixed number of pixels wide and high, such as 8x16. Every glyph is drawn to fit its cells, and the cell size and the display's size together give the grid: a 640x480 display in 8x16 cells is 80 columns by 30 rows.

- **In everyday words:** one character's box on the screen.
- **Where it lives:** a frit's `cell=` line, or the `font-size` setting ([config.md](config.md)); ADR-1200 D17 decides where the cell comes from.

#### A frit (.vtfrit)

A frit is a small text file that names a font's sources in order and the cell to draw them in. Each later source fills only the characters the earlier ones lack. A frit holds no font data: it names fonts that the machine drawing it has installed. Only root writes frits, and vtium itself never writes one.

- **In everyday words:** a shopping list for a font: which fonts to take, which characters to take from each, and what size to cut them to.
- **Where it lives:** `/etc/vtium/fonts/NAME.vtfrit`. ADR-1200 D1 lists its keys, [crates/vtium-vitrify](../crates/vtium-vitrify/README.md#frits) describes how it is read, and the frits vtium ships are in `crates/vtium-vitrify/frits/`: `minspec-16` and `minspec-8`, which add chosen symbols and emoji to the built-in sets, and `proof`, a desktop frit of a code font, symbols and emoji.

#### A source

A source is one `source=` line of a frit: one font the helper reads. It can name an installed font by its family (`font:`), a console font by its kbd name (`kbd:`), a file (`file:`, `bdf:`, `outline:`), the font vconsole.conf sets (`vconsole`), the kernel's own font for a VT (`console:VT`), or one of the built-in sets (`builtin:`).

- **In everyday words:** one font on the list.
- **Where it lives:** the frit; the forms are listed in [crates/vtium-vitrify](../crates/vtium-vitrify/README.md#frits), and `src/sources.rs` finds and reads each one.

#### A cover range

A cover range is a `cover=` line under a source. It limits that source to a named character set, such as `ascii` or `jis-x-0208`, or to a range of codepoints such as `U+E0A0-U+E0A2`. A source with no `cover=` line gives every character it holds.

- **In everyday words:** "take only these characters from this font".
- **Where it lives:** the frit. The named sets are tables generated into `crates/vtium-vitrify/src/cover.rs`, and the shipped frits write each range with a comment saying why it is there.

#### vitrify and vtium-vitrify

To vitrify a font, in vtium, is to turn its raw files into finished glyphs at the console's cell. `vtium-vitrify` is the program that does it, a binary of its own outside the console, so the font decoders and the rasteriser (the code that turns a font's outlines into pixels) never run inside the console. Its `serve` command is the helper, and its `set` command switches the running console's font. `preview` and `new`, which try fonts and build frits, are proposed.

- **In everyday words:** the font tool: it melts fonts down into the pictures the screen shows.
- **Where it lives:** `crates/vtium-vitrify` ([its README](../crates/vtium-vitrify/README.md)), decided in ADR-1200.

#### The helper (vtium-vitrify serve)

The helper is `vtium-vitrify serve`, started by the console. The console hands it a frit's text, and the helper reads the frit's sources, draws the glyphs the console asks for and sends them back on a pipe. It runs in a sandbox and never as root: when vtium runs as root, the helper runs as `nobody`. It starts on the console's first need, exits when idle, and is restarted after a crash a bounded number of times a boot. The console checks every answer and trusts the helper's glyphs only to be well-formed.

- **In everyday words:** a separate, locked-down worker that reads font files so the console never has to.
- **Where it lives:** `crates/vtium-vitrify/src/serve.rs`, ADR-1200 D15, and the `font-helper` key ([config.md](config.md)).

#### A vitrified set

A vitrified set is the form glyphs travel in: one buffer that starts with the bytes `VTGLASS`, then the glyphs at each cell, the map from characters to glyphs, the metrics such as the baseline and the underline's position, and where the glyphs came from. The helper's glyphs reach the console as vitrified sets on its pipe, and nothing writes one to a file.

- **In everyday words:** a box of finished glyphs, labelled with what is inside.
- **Where it lives:** vtium-font's `src/set/` ([crates/vtium-font](../crates/vtium-font/README.md)), and ADR-1200 D2, which decides the format.

#### The built-in sets (unifont, unscii-8)

The built-in sets are two fonts compiled into the `vtium` binary as vitrified sets: unifont, GNU Unifont's 8x16 glyphs, and unscii-8, Unscii's 8x8 glyphs. vtium draws unscii-8 for cells under 16 pixels high and unifont from 16, and unifont in a Chinese, Japanese or Korean locale, since unscii-8 holds no CJK. They draw the first frame at once, before the helper has sent anything, and they draw each character the frit lacks. They are the only fonts the console's own process reads.

- **In everyday words:** the fonts vtium always carries, so it can show text before anything else is ready.
- **Where it lives:** `crates/vtium-font/data/` and `src/set/builtin.rs`, with their licences in [crates/vtium-font](../crates/vtium-font/README.md); ADR-1200 D7 decides them.

#### Sprites

Sprites are the line and block characters vtium draws itself instead of taking them from a font: box drawing, block elements and shades, Braille, sextants, octants and the block shapes of Symbols for Legacy Computing. vtium draws each one to fill its cell at any cell size, so lines join from cell to cell. By default a font that covers a whole range of these characters draws them from its own glyphs, and vtium draws the sprites for every other range. With the built-in sets, vtium draws every sprite. The `glyph-source` settings change the choice for each range.

- **In everyday words:** the frame lines and shaded blocks that programs draw boxes and bars with, drawn by vtium so they always meet.
- **Where it lives:** vtium-font's `src/sprite.rs` and `src/policy.rs` ([crates/vtium-font](../crates/vtium-font/README.md)), and the `glyph-source` keys in [config.md](config.md).

## How vtium is judged

#### The C reference

The C reference is the maintainer's forks of the C terminal `kmscon` and its screen and parser library `libtsm`, which vtium learned its behaviour from. vtium is judged against what the C reference did, and does more where the specifications do. The C reference is frozen, and vtium's tree holds copies of what it needs from it, each with the commit it came from.

- **In everyday words:** the older program vtium is measured against.
- **Where it lives:** [aaronsb/kmscon](https://github.com/aaronsb/kmscon) and [aaronsb/libtsm](https://github.com/aaronsb/libtsm), described part by part in [c-reference.md](c-reference.md).

#### A golden

A golden is a saved expected result that vtium's output is compared against: a screen written as text, or a picture compared pixel for pixel. Some goldens were copied from the C reference; others are vtium's own, for screens the C reference never drew, and those stay provisional, judging nothing, until the maintainer reviews them.

- **In everyday words:** the answer key a test checks vtium's output against.
- **Where it lives:** `tools/conformance/`, `tools/tier2/reference/` and `tools/tier3/c-goldens/` for the C reference's, with vtium's own beside them ([AGENTS.md](../AGENTS.md#how-vtium-is-judged)).

#### Tiers 1 to 3

The tiers are the levels vtium is tested at, above each crate's unit tests. Tier 1 replays streams of bytes through vtium's terminal and compares the screen and the replies as text. Tier 2 has the renderer draw known screens in memory and compares them pixel for pixel. Tier 3 boots vtium's install tree on a VT in a virtual machine, types into it and compares screenshots, and it records size and memory figures. After the tiers, the maintainer checks a build by eye on a real VT.

- **In everyday words:** three ever-larger tests: the text, the pixels, and the whole program running on a virtual machine.
- **Where it lives:** `make conformance`, `make tier2` and `make tier3`, with their harnesses in `tools/conformance/`, `tools/tier2/` and `tools/tier3/`; [AGENTS.md](../AGENTS.md#how-vtium-is-judged) lists each with its command.

#### A departure

A departure is a recorded difference from the C reference, kept on a list with its reason. Most follow a specification where the C reference did not, or fix a defect the C reference had. A difference from a golden that is not listed as a departure fails the test.

- **In everyday words:** a place where vtium does something differently on purpose, written down so it is not mistaken for a bug.
- **Where it lives:** the Departures section of each crate's README, such as [crates/vtium-term](../crates/vtium-term/README.md#departures), and the list in vtium/vtium#14.

## Why glass

vtium's font words come from glassmaking. The maintainer describes vtium as a glass terminal, and keeps the glass words to hold a connection to the history of terminals, in which the early screen terminals were called glass TTYs.

The figure sets glassmaking beside vtium's fonts. A glassmaker fuses raw materials into a frit, grinds it and melts it, and the frit vitrifies: it turns into glass. In vtium, a frit names the font files, vtium-vitrify reads them and draws their glyphs, and the glyphs are what the screen shows. The parallel has one gap: a glassmaker's frit is the material itself, and a vtium frit only names its fonts.

![Two rows side by side. Above, in glassmaking: raw materials (sand and fluxes), then frit (the mix, fused, cooled and ground), then melting (the frit vitrifies: it turns to glass), then glass (clear, and set in its shape). Below, in vtium, each joined to the step above it by a dotted line: font files (PSF, BDF, kbd and outline fonts), then NAME.vtfrit (fonts, characters and the cell), then vtium-vitrify (reads the fonts and draws their glyphs), then glyphs (bitmaps at the cell, on the screen). A note says a glassmaker's frit is the material itself, and a vtium frit is a short text file that names the fonts to melt down and holds no font data.](figures/terminology-glass.svg)

The second figure shows the family of names: the words from terminal history on the left, the words from glassmaking on the right, and the names in vtium's tree that take from both.

![Three columns. On the left, terminal history: TTY, a teletype that prints on paper; then, as a screen replaces paper, glass TTY, a terminal with a screen in place of paper; then, as early ones include DEC's VT05, VT, DEC's video terminals and Linux's virtual consoles. Glass TTY and VT both point to vtium in the middle, a glass terminal on the VT. On the right, glassmaking: vitrum, Latin for glass, is the root of vitrify, to turn into glass by heat, and frit, glass's mix fused and ground to be melted, is what is vitrified. Below, names in vtium's tree: the glass banner, vtium-vitrify's title; VTGLASS, a set's first bytes; vtium-vitrify, the font tool, which vitrify points to; and NAME.vtfrit, a frit's file, which frit points to. vtium points to the whole row, with vt or vtium in each name.](figures/terminology-names.svg)

- **VT.** DEC's VT100 was "a video terminal, introduced in August 1978 by Digital Equipment Corporation" ([Wikipedia, VT100](https://en.wikipedia.org/wiki/VT100)). The Linux console speaks "a large subset of the VT102" terminal's controls ([`console_codes(4)`](https://man7.org/linux/man-pages/man4/console_codes.4.html)), and the kernel calls its text terminals virtual consoles ([the kernel's device list](https://www.kernel.org/doc/html/latest/admin-guide/devices.html)), whose device names start with `tty`, after the teletype.
- **Glass TTY.** A teletype, TTY for short, was a terminal that printed on paper. Early terminals with a screen in its place, DEC's VT05 among them, were often called glass TTYs, and the name was later narrowed to screen terminals that cannot move the cursor to any place on the screen, and so can do little more than a teletype ([Wikipedia, Computer terminal](https://en.wikipedia.org/wiki/Computer_terminal)).
- **vtium.** The maintainer chose the name for its sound, in place of the working title kmscon-rust, and named the project apart from `kmscon` because upstream plans a Rust `kmscon` of its own ([ADR-304 in the kmscon fork](https://github.com/aaronsb/kmscon/blob/main/docs/architecture/fork/ADR-304-vtium-a-full-rust-port-of-kmscon-and-libtsm-minspec-first-judged-by-the-existing-tiers.md)). <!-- adr-cite-ignore: the kmscon fork's record, not one of vtium's -->
- **Vitrify.** To vitrify is to turn something into glass by heat, from the Latin vitrum, glass ([Wiktionary, vitrify](https://en.wiktionary.org/wiki/vitrify)). The issue that tracks vtium-vitrify, vtium/vtium#3, proposed vitrifying fonts: baking them ahead of time into read-only glyph sets. Since then the helper draws glyphs at run time and keeps nothing past the boot, but the word still names the same step, from a font's raw files to finished glyphs. The maintainer named the binary `vtium-vitrify` (ADR-1200's basis).
- **Frit.** In glassmaking, a frit is "a calcinated mixture of sand and fluxes ready to be melted in a crucible to make glass", in the Oxford English Dictionary's words, and more generally "a ceramic composition that has been fused, quenched, and granulated" ([Wikipedia, Frit](https://en.wikipedia.org/wiki/Frit)). The file that names a font's sources was first called a recipe, `NAME.recipe`, and the maintainer renamed it a frit, `NAME.vtfrit`, on 2026-10-28, after the glassmaker's ground glass that is melted again.
- **VTGLASS and the banner.** Every vitrified set starts with the eight bytes `VTGLASS\0` (vtium-font's `src/set/format.rs`). At an interactive terminal, vtium-vitrify opens with a banner of its name in glass, panes of green and cyan glass behind it (vtium/vtium#112, `crates/vtium-vitrify/src/banner.rs`).
