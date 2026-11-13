# VTium

**vtium is a terminal for the Linux console.**

vtium runs on a Linux virtual terminal (VT). A VT is one of the text screens you can switch to with Ctrl+Alt and a function key. vtium takes the place of getty, the program that normally displays the login prompt there.

vtium renders the screen itself, so it needs no display server. It can render entirely on the CPU, so a GPU is optional. It gives the programs running on it what a terminal on a desktop gives them: the full Unicode character set, color emoji, 24-bit color, mouse reporting, and images. When vtium cannot start, getty and the kernel's console take the VT back.

The kernel's own console has stayed plain so that it still works when nothing else does. Since Linux 6.10, the kernel can draw its own crash screen through its modern graphics drivers. The Red Hat graphics developers behind that work call the VT console one of the [legacy parts of the kernel](https://fedoraproject.org/wiki/Changes/EnableDrmPanic). They propose replacing it with a console that runs as an ordinary program. vtium is a console of that kind.

vtium is built for a wide range of machines. At one end is a small board with 512 MB of memory and a 320×240 display. At the other end is a desktop with several large monitors.

vtium is written in Rust. It began as a port of `kmscon` and its library `libtsm`, which are written in C. See [Provenance](vtium-in-depth.md#provenance) for details.

![A shell session on a Linux console in a smooth monospaced font: a colored prompt, a directory listing, a git log with a branch graph, a row of six images with their file names, greetings in several languages and scripts with emoji, and a line of underline and text styles.](showcase/hero.png)

*A shell session as vtium renders it on a 1920×1080 display. [The showcase](showcase/README.md) has more screens and says how they were captured.*

[VTium in depth](vtium-in-depth.md) explains what a terminal is and how the Linux console came to be plain. It then describes what vtium does, its status, what it needs, and how to try it and configure it.

This site describes vtium's `main` branch. The `main` branch may be ahead of [the latest release](https://github.com/vtium/vtium/releases).

## Where to start

| If you want to | Start here |
|---|---|
| try vtium for the first time, on one VT, and get back to your desktop safely | [Tutorials](tutorial-try-vtium.md), which teach by doing |
| get one task done: install it, configure it, choose a font, set up heads, or find out what went wrong | [How-to guides](managing-vtium.md), for a reader who knows what they want |
| look something up: what vtium does, its keys, its configuration keys, the sections of vtium.conf | [Reference](features.md), to read in the middle of the work |
| understand why vtium exists and how its pieces fit together | [Explanation](vtium-in-depth.md), to read away from the keyboard |

[Decisions](architecture/INDEX.md) holds a record of each decision behind vtium, with the maintainer's reasons. [For contributors](architecture.md) describes how vtium is built and how it is judged.

vtium's source is at [github.com/vtium/vtium](https://github.com/vtium/vtium). Its releases are on [the releases page](https://github.com/vtium/vtium/releases).
