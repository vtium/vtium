# VTium

**vtium is a terminal that runs on the Linux console.**

vtium runs on a Linux virtual terminal (VT)—one of the text screens you can switch to with Ctrl+Alt and a function key. It takes the place of getty, the program that normally displays the login prompt. So the console your machine boots into is a terminal with the features programs expect of one. These include text in any script, color emoji, 24-bit color, images, and mouse reporting. vtium renders through [KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the modern graphics interface, on the CPU or on the GPU. If vtium cannot start, getty and the kernel's console take over.

![A shell session on a Linux console in a smooth monospaced font: a colored prompt, a directory listing, a git log with a branch graph, a row of six images with their file names, greetings in several languages and scripts with emoji, and a line of underline and text styles.](showcase/hero.png)

*A shell session as vtium renders it on a 1920×1080 display. [The showcase](showcase/README.md) has more screens and says how they were captured.*

## Your text, in any script

vtium splits text into grapheme clusters, the characters as a reader sees them. It gives each cluster one or two character cells, so columns line up. It draws combining marks, emoji, and color emoji. Chinese, Japanese, and Korean characters take two cells each. A font helper shapes each cluster, so scripts such as Devanagari and Tamil join their syllables. vtium lays out right-to-left text, such as Arabic and Hebrew, by the Unicode bidirectional algorithm, and it joins Arabic letters.

You choose the fonts. A font set lists fonts in order, and each later font fills in the characters that the earlier ones lack. vtium reads bitmap fonts, such as the kernel's own console fonts, and outline fonts with their bold and italic faces. Two fonts are built into vtium, so it starts without a font file. Box drawing, block elements, and Braille patterns join cell to cell in any font. Programming ligatures are still to come. See [the ligatures issue](https://github.com/vtium/vtium/issues/23).

See [Choose a font](fonts.md) and [Fonts for scripts](fonts-for-scripts.md) for details.

## Your screens: a terminal on each display

One vtium drives every display it finds. With no configuration, it shows one terminal across all of them. A head is a terminal for one or more displays, with its own grid of character cells, its own font, and its own consoles. A console is one session, with its own login, screen, and scrollback. Keys switch a head between its consoles.

Rules in vtium.conf place each display, keyboard, and mouse in a head. vtium checks each device against the rules from the top, and the first rule that matches decides. So each display can show a terminal of its own, with a keyboard of its own. For example, one monitor could run a kiosk while another provides an operator's shell.

When the font or a display changes the width, vtium rewraps the text and its scrollback. A restart of vtium keeps your sessions running. See [Logins and sessions](concepts/security.md#logins-and-sessions) for the one case where they end.

See [Set up heads](heads.md) and [Heads](concepts/heads.md) for details.

## Images

Programs show images in vtium through sixel, the kitty graphics protocol, and iTerm2's inline images. vtium plays kitty's animations and draws its Unicode placeholders. Each image is anchored to a line of text, so it scrolls with the text. A separate program, the image helper, reads and decodes every image. It runs as your user, never as root, inside a sandbox.

See [Images](features.md#images) for details.

## Keyboards and mice

vtium reads each keyboard and mouse from the kernel's input devices. It uses the keyboard layout that your system already sets for its consoles, and it follows a change to that layout. vtium.conf's rules send each device to a head. A keyboard can also roam, which means it types into whichever head is active.

Programs get keys in xterm's encodings, or in the [kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). Under that protocol, a program can tell every key and modifier apart, and it can hear when a key is released. vtium reports mouse clicks, drags, and motion in the SGR, urxvt, and pixel encodings. It also reports focus and bracketed pastes. You select text with the mouse, and you paste it with the middle button or Shift+Insert.

An input method for composing Chinese, Japanese, or Korean text is still to come. See [the input method issue](https://github.com/vtium/vtium/issues/45). Braille displays are planned too. See [the Braille display issue](https://github.com/vtium/vtium/issues/46).

See [Keys](keys.md) for the keys vtium keeps for itself.

## The terminal's language, and the rule it follows

vtium's base is [xterm's control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html), a document known as ctlseqs. vtium also carries out the editing features of DEC's VT terminals, as the [VT510 manual](https://vt100.net/docs/vt510-rm/contents.html) describes them. These include left and right margins, inserting and deleting columns, rectangular area operations, and protected characters. Double-width and double-height lines are partly built. See [the double-size lines issue](https://github.com/vtium/vtium/issues/61).

Of the newer protocols, vtium supports sixel, the kitty graphics and keyboard protocols, OSC 8 links, OSC 133 prompt marks, and OSC 52 copy. It also supports synchronized output and reports of a resize. The kitty text sizing protocol is still to come. See [the text sizing issue](https://github.com/vtium/vtium/issues/60).

vtium follows these rules when it decides what a sequence does:

- Each sequence does what the specification that defines it says.
- vtium takes in a capability when xterm's control sequences and another terminal's documentation both define it. It also takes in one the maintainer asks for, or one the console has a use for.
- Where two specifications give one sequence different meanings, vtium's table of features says which meaning vtium takes.
- Where a specification leaves a choice open, vtium's own tests decide, and the choice is written down.

One rule comes before all of these. A program's output never reads from your machine or steers it. vtium builds every reply from its own state, so text that a program prints cannot type into the program that reads the replies.

See [Modes](modes.md) for each mode vtium recognizes, and [What vtium does](features.md#the-terminal) for the rest.

## Where it runs

vtium runs on Linux, on x86_64, aarch64, and armv7 processors. It renders into memory buffers that the kernel's graphics driver displays, or through fbdev on a machine without DRM. Every build can render on the CPU, so a GPU is optional. On supported hardware, a build with the GPU path renders through OpenGL ES 3.0, and it draws the same pixels as the CPU.

With systemd, vtium starts on a VT where a getty would start. Without systemd, your init system starts vtium where it started a getty. In seat mode, vtium gets its displays and input devices from seatd or logind.

The smallest build, minspec, is built for a board with 512 MB of memory and a 320×240 display. vtium's virtual machine tests measure it with one CPU and 512 MB of memory. There, its console binary is 3,216,864 bytes, and it uses at most 5.81 MiB of memory at 320×240. See [Small machines](features.md#small-machines) for the measurement and its source. The `vtium` and `vtium-git` packages build a larger console. It adds images, right-to-left text, the GPU path, and seat mode, which minspec leaves out.

## What vtium leaves to other tools

- vtium copies a link's address, and it never opens the link.
- A program can copy text to vtium's clipboard with OSC 52. No program can read the clipboard back.
- Each head shows one console at a time. vtium draws no tabs and no split panes. A terminal multiplexer such as tmux runs in vtium and splits a console into panes.
- A console has one screen and no window. So vtium moves and resizes no window, and it keeps no window title.
- Desktop notifications, file transfer, and remote control of the terminal belong to other tools.

See [What vtium does not do](concepts/what-vtium-does-not-do.md) for the reasons.

## Where to start

See [the releases](https://github.com/vtium/vtium/releases) for the latest version and what changed in it. To try vtium while your desktop keeps running, follow [Try vtium on one VT](tutorial-try-vtium.md). The tutorial runs vtium on a spare VT and returns that VT to its previous state afterward.

vtium-config is vtium's setup tool, and it runs in a terminal. It draws your vtium.conf as a flow, from the displays through the rules and heads to the keyboards and mice. Each change goes into a draft of your own, and one `sudo` step installs the file. As you move through its list of fonts, it tries each font on the live console. The rest of its planned job is still to come. See [the vtium-config issue](https://github.com/vtium/vtium/issues/367).

| If you want to | Start here |
|---|---|
| try vtium for the first time, on one VT, and get back to your desktop safely | [Tutorials](tutorial-try-vtium.md), which teach by doing |
| get one task done: install it, configure it, choose a font, set up heads, or find out what went wrong | [How-to guides](managing-vtium.md), for a reader who knows what they want |
| look something up: what vtium does, its keys, its configuration keys, the sections of vtium.conf | [Reference](features.md), to read in the middle of the work |
| understand why vtium exists and how its pieces fit together | [Explanation](vtium-in-depth.md), to read away from the keyboard |

[Decisions](architecture/INDEX.md) holds a record of each decision behind vtium, with the maintainer's reasons. [For contributors](architecture.md) describes how vtium is built and how it is judged.

This site describes vtium's `main` branch. The `main` branch may be ahead of [the latest release](https://github.com/vtium/vtium/releases).

vtium is written in Rust. Its code began as a port of two programs written in C. See [Provenance](vtium-in-depth.md#provenance) for details. vtium's source is at [github.com/vtium/vtium](https://github.com/vtium/vtium).
