# VTium

**vtium is a terminal that runs on the Linux console.**

vtium runs on a Linux virtual terminal (VT), one of the text screens you reach with Ctrl+Alt and a function key. It takes the place of getty, the program that normally shows the login prompt there. Your machine then boots into a terminal with text in any script, color emoji, 24-bit color, images, and mouse reporting. vtium renders through [KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the modern graphics interface, on the CPU or the GPU.

![A shell session on a Linux console in a smooth monospaced font: a colored prompt, a directory listing, a git log with a branch graph, a row of six images with their file names, greetings in several languages and scripts with emoji, and a line of underline and text styles.](showcase/hero.png)

*A shell session rendered by vtium on a 1920×1080 display. [The showcase](showcase/README.md) has more screenshots and explains how they were captured.*

## What vtium does

- [Text in any script](home/text.md): grapheme clusters lined up in character cells, shaping for complex scripts, right-to-left text, and the fonts vtium reads.
- [A terminal on each display](home/displays.md): one vtium can give each display its own terminal and keyboard.
- [Images](home/images.md): sixel, the kitty graphics protocol, and iTerm2's inline images.
- [Keyboards and mice](home/input.md): how vtium reads your devices, and the key and mouse encodings programs receive.
- [The terminal's language](home/language.md): the specifications vtium follows, and how it settles a conflict between two of them.
- [Where vtium runs](home/where-it-runs.md): processors, graphics, init systems, and the smallest build.
- [What vtium leaves to other tools](home/leaves-to-other-tools.md): opening links, split panes, window management, and other jobs that belong to other programs.

## Where to start

[The releases](https://github.com/vtium/vtium/releases) list the latest version and what changed in it. To try vtium while your desktop keeps running, follow [Try vtium on one VT](tutorial-try-vtium.md). The tutorial runs vtium on a spare VT and puts that VT back the way it was afterward.

vtium-config, vtium's setup tool, runs in a terminal and draws your vtium.conf as a flow: from the displays, through the rules and heads, to the keyboards and mice. Your changes go into a draft of your own, and a single `sudo` step installs the finished file. Its Fonts view tries each font on the live console as you move through the list. The rest of vtium-config's planned work is still to come, and [the vtium-config issue](https://github.com/vtium/vtium/issues/367) tracks it.

| If you want to | Start here |
|---|---|
| try vtium for the first time, on one VT, and get back to your desktop safely | [Tutorials](tutorial-try-vtium.md), which teach by doing |
| get one task done: install and configure vtium, choose a font, set up heads, or find out what went wrong | [How-to guides](managing-vtium.md), for a reader who knows what they want |
| look something up: what vtium does, its keys, its configuration keys, the sections of vtium.conf | [Reference](features.md), to read in the middle of the work |
| understand why vtium exists and how its pieces fit together | [Explanation](vtium-in-depth.md), to read away from the keyboard |

Each decision behind vtium has a record in [Decisions](architecture/INDEX.md), with the maintainer's reasons. [For contributors](architecture.md) covers how vtium is built and how its output is checked.

This site describes vtium's `main` branch, which may be ahead of [the latest release](https://github.com/vtium/vtium/releases).

vtium is written in Rust. Its code began as a port of two programs written in C, a history that [Provenance](vtium-in-depth.md#provenance) tells in full. The source is at [github.com/vtium/vtium](https://github.com/vtium/vtium).
