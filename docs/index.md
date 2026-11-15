# VTium

**vtium is a terminal that runs on the Linux console.**

vtium runs on a Linux virtual terminal (VT)—one of the text screens you can switch to with Ctrl+Alt and a function key. vtium takes the place of getty, the program that normally displays the login prompt. So your machine boots into a terminal with text in any script, color emoji, 24-bit color, images, and mouse reporting. vtium renders through [KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the modern graphics interface, on the CPU or on the GPU.

![A shell session on a Linux console in a smooth monospaced font: a colored prompt, a directory listing, a git log with a branch graph, a row of six images with their file names, greetings in several languages and scripts with emoji, and a line of underline and text styles.](showcase/hero.png)

*A shell session as vtium renders it on a 1920×1080 display. [The showcase](showcase/README.md) has more screens and says how they were captured.*

## What vtium does

- [Text in any script](home/text.md) says how vtium lines up every script, and which fonts vtium reads.
- [A terminal on each display](home/displays.md) says how one vtium gives each display a terminal and keyboard of its own.
- [Images](home/images.md) names the image protocols that programs use in vtium.
- [Keyboards and mice](home/input.md) says how vtium reads your devices, and what programs get from them.
- [The terminal's language](home/language.md) names the specifications vtium follows, and the rules vtium uses when two specifications disagree.
- [Where vtium runs](home/where-it-runs.md) covers processors, graphics, init systems, and the smallest build.
- [What vtium leaves to other tools](home/leaves-to-other-tools.md) lists the jobs that vtium hands to other programs.

## Where to start

See [the releases](https://github.com/vtium/vtium/releases) for the latest version and what changed in it. To try vtium while your desktop keeps running, follow [Try vtium on one VT](tutorial-try-vtium.md). The tutorial runs vtium on a spare VT and returns that VT to its previous state afterward.

vtium-config is vtium's setup tool, and runs in a terminal. vtium-config draws your vtium.conf as a flow, from the displays through the rules and heads to the keyboards and mice. Each change goes into a draft of your own, and one `sudo` step installs the file. The Fonts view of vtium-config tries each font on the live console as you move to the font. The rest of vtium-config's planned job is still to come. See [the vtium-config issue](https://github.com/vtium/vtium/issues/367).

| If you want to | Start here |
|---|---|
| try vtium for the first time, on one VT, and get back to your desktop safely | [Tutorials](tutorial-try-vtium.md), which teach by doing |
| get one task done: install and configure vtium, choose a font, set up heads, or find out what went wrong | [How-to guides](managing-vtium.md), for a reader who knows what they want |
| look something up: what vtium does, its keys, its configuration keys, the sections of vtium.conf | [Reference](features.md), to read in the middle of the work |
| understand why vtium exists and how its pieces fit together | [Explanation](vtium-in-depth.md), to read away from the keyboard |

[Decisions](architecture/INDEX.md) holds a record of each decision behind vtium, with the maintainer's reasons. [For contributors](architecture.md) describes how vtium is built and how it is judged.

This site describes vtium's `main` branch. The `main` branch may be ahead of [the latest release](https://github.com/vtium/vtium/releases).

vtium is written in Rust. vtium's code began as a port of two programs written in C. See [Provenance](vtium-in-depth.md#provenance) for details. vtium's source is at [github.com/vtium/vtium](https://github.com/vtium/vtium).
