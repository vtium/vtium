# VTium

**vtium is a terminal that runs on the Linux console.**

vtium runs on a Linux virtual terminal (VT), one of the text screens you reach with Ctrl+Alt and a function key. It takes the place of getty, the program that normally shows the login prompt there. So the console your machine boots into is a full terminal, with 24-bit color, color emoji, images, mouse reporting, and text in any script your fonts cover. vtium renders through [KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the modern graphics interface, on the CPU or the GPU.

A full terminal helps wherever you work at the machine's own screen: a server without a desktop, a kiosk, a small board, or the console before a desktop starts. There, editors, file managers and tmux get the color, mouse and image support they expect.

vtium is alpha software, at version 0.1.0-alpha.8. A vtium package takes every free VT when you install it, and each VT switches to vtium at its next start. So try vtium on one VT first; [Try vtium on one VT](tutorial-try-vtium.md) shows how. With the default setup under systemd, a getty takes the VT back when vtium fails, so you still get a login prompt.

![A shell session on a Linux console in a smooth monospaced font: a colored prompt, a directory listing, a git log with a branch graph, a row of six images with their file names, greetings in several languages and scripts with emoji, and a line of underline and text styles.](showcase/hero.png)

*A shell session rendered by vtium on a 1920×1080 display. [The showcase](showcase/README.md) has more screenshots and explains how they were captured.*

## What vtium does

- [Text in any script](home/text.md): grapheme clusters lined up in character cells, shaping for complex scripts, right-to-left text, and the fonts vtium reads.
- [A terminal on each display](home/displays.md): one vtium drives several displays at once, each with its own independent terminal, keyboard, and mouse, for several people working at one machine.
- [Images](home/images.md): sixel, the kitty graphics protocol, and iTerm2's inline images.
- [Keyboards and mice](home/input.md): how vtium reads your devices, the key and mouse encodings programs receive, and the keys that act on the machine.
- [The terminal's language](home/language.md): the specifications vtium follows, and how it settles a conflict between two of them.
- [Where vtium runs](home/where-it-runs.md): processors, graphics, init systems, and the smallest build.
- [Security and the system](home/security-and-system.md): sandboxed helpers for fonts and images, a reply rule that keeps output from typing into your shell, and how vtium works with systemd and logind.
- [What vtium leaves to other tools](home/leaves-to-other-tools.md): opening links, split panes, window management, and other jobs that belong to other programs.

## Where to start

On Arch, the AUR carries `vtium`, built from the latest release, and `vtium-git`, built from the tip of `main`. The `vtium` recipe checks the maintainer's signing key, which the recipe carries:

```
$ git clone https://aur.archlinux.org/vtium.git
$ cd vtium
$ gpg --import keys/pgp/33F9B54FD2A4B6A4110749B3BA464FC9847A832A.asc
$ VTIUM_FEATURES='images logind gpu bidi libseat' makepkg -si
```

[Install vtium and give it VTs](install.md) covers the other packages and building from source. [The releases](https://github.com/vtium/vtium/releases) list what changed in each version.

vtium-config, vtium's setup tool, runs in a terminal. It shows your vtium.conf as a flow diagram that runs from the displays, through the rules and heads, to the keyboards and mice. A head is vtium's name for a terminal that spans one or more displays. Your edits go into a draft of your own until a single `sudo` step installs the file. Its Fonts view previews each font on the live console as you move through the list. vtium-config's planned work is not finished, and [the vtium-config issue](https://github.com/vtium/vtium/issues/367) tracks what remains.

| If you want to | Start here |
|---|---|
| try vtium on one VT and get back to your desktop afterward | [Try vtium on one VT](tutorial-try-vtium.md) |
| install vtium, configure it, choose a font, set up heads, or fix a problem | [Install vtium and give it VTs](install.md), [Configure vtium](configure.md), [Choose a font](fonts.md), [Set up heads](heads.md), or [Find out what went wrong](troubleshooting.md) |
| look up a feature, a key, or a setting | [What vtium does](features.md), [Keys](keys.md), [Configuration keys](config.md), [The head file](head-file.md), [Modes](modes.md), or [systemd, logind and PID 1](systemd.md) |
| understand how vtium works and why | [VTium in depth](vtium-in-depth.md), [Why vtium](why-vtium.md), and [Concepts](concepts/index.md) |
| read the reasoning behind a decision | [Decision records](architecture/INDEX.md) |
| contribute | [For contributors](architecture.md), [CONTRIBUTING.md](https://github.com/vtium/vtium/blob/main/CONTRIBUTING.md), and [AGENTS.md](https://github.com/vtium/vtium/blob/main/AGENTS.md) |

This site describes vtium's `main` branch, which may be ahead of [the latest release](https://github.com/vtium/vtium/releases).

vtium is written in Rust. Its code began as a port of two programs written in C, a history that [Provenance](vtium-in-depth.md#provenance) tells in full. The source is at [github.com/vtium/vtium](https://github.com/vtium/vtium).
