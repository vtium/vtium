# VTium

vtium is a terminal for the Linux virtual console: it runs on a VT, one of the full-screen text screens the kernel keeps beside a desktop, and draws the terminal there itself, with no display server.

The kernel's own console turns 256-color and 24-bit color into 16 colors, holds at most 512 characters of one bitmap font at a time, and shows no pictures. vtium takes the display through KMS/DRM, or through the older fbdev framebuffer where that is all a machine has, and gives the programs on the console what a desktop terminal gives them: 24-bit color, Unicode with Chinese, Japanese and Korean text and color emoji, outline fonts, mouse reporting, scrollback that rewraps when the width changes, and pictures. It is written in Rust, needs no GPU and does not rely on systemd, and its smallest build runs on a single-board computer with 512 MB of memory and a 320x240 panel.

![A console screen showing a shell session in a smooth monospaced font: prompts in blue, gray and green segments, a colored listing of the vtium source tree, a git log with a branch graph and yellow commit hashes, the crates of vtium, a passing test run, and three lines of greetings and thanks in English, Chinese, Japanese, Korean, Thai, Greek, Russian, Vietnamese, French and German with emoji.](showcase/hero.png)

vtium's own drawing of a shell session on a 1920x1080 display. [The showcase](showcase/README.md) says what this screen shows, and has more screens, from emoji and the world's scripts to pictures, and the same console on a 320x240 panel.

vtium's latest release is the pre-release v0.1.0-alpha.3. This site describes vtium's `main` branch, which may be ahead of that release.

## Where to start

| If you want to | Start here |
|---|---|
| try vtium for the first time, on one VT, and get back to your desktop safely | [Tutorials](tutorial-try-vtium.md), which teach by doing |
| get one task done: install it, configure it, choose a font, set up heads, or find out what went wrong | [How-to guides](managing-vtium.md), for a reader who knows what they want |
| look something up: what vtium does, its keys, its configuration keys, the head file | [Reference](features.md), to read in the middle of the work |
| understand why vtium exists and how its pieces fit together | [Explanation](why-vtium.md), to read away from the keyboard |

[Decisions](architecture/INDEX.md) holds the record of each decision behind vtium, with the maintainer's reasons, and [For contributors](architecture.md) describes how vtium is built and judged.

vtium's source is at [github.com/vtium/vtium](https://github.com/vtium/vtium), and its releases are on [the releases page](https://github.com/vtium/vtium/releases).
