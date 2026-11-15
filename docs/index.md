# VTium

**vtium is a terminal that runs on the Linux console.**

vtium runs on a Linux virtual terminal (VT), one of the text screens you reach with Ctrl+Alt and a function key. It takes the place of getty, the program that normally shows the login prompt there. The console your machine boots into becomes a full terminal, with 24-bit color, color emoji, images, mouse reporting, and text in the scripts your fonts cover. Rendering goes through [KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the modern graphics interface, on the CPU or the GPU.

That helps wherever you work at the machine's own screen: a server without a desktop, a kiosk, a small board, or the console before a desktop starts. Editors, file managers and tmux get the color, mouse and image support they expect there.

This is alpha software, at version 0.1.0-alpha.8. Installing the package gives vtium every free VT from that VT's next start. On a machine with no display manager, that includes VT 1, the screen where the boot messages appear. To try one VT only, follow [Try vtium on one VT](tutorial-try-vtium.md) instead.

![A shell session on a Linux console in a smooth monospaced font: a colored prompt, a directory listing, a git log with a branch graph, a row of six images with their file names, greetings in several languages and scripts with emoji, and a line of underline and text styles.](showcase/hero.png)

*A shell session rendered by vtium on a 1920×1080 display. [The showcase](showcase/README.md) has more screenshots and explains how they were captured.*

## If something goes wrong

- **Get off the VT.** Ctrl+Alt and a function key switch to another VT or to your desktop. If vtium has stopped answering keys, that switch fails too, and you work from another machine over ssh instead.
- **Fall back to a login prompt.** When vtium fails under systemd, a getty starts on its VT, so you get a plain login prompt rather than a dead screen. [Security and the system](home/security-and-system.md#with-systemd-and-logind-and-without-them) names the two setups where no getty starts.
- **Give a VT back.** From another VT or over ssh, `vtium console disable 3` returns VT 3 to its getty from the VT's next start, and `systemctl start getty@tty3.service` does it at once. `vtium console disable --all` returns every VT.
- **Boot once without vtium.** Add `vtium.enable=0` to the kernel command line in your boot loader.
- **Uninstall.** Stop vtium on each VT where it runs, for example with `systemctl stop vtium@tty3.service`, then run `pacman -R vtium`, or `pacman -R vtium-git`. The package's removal script gives every VT back what it had.

The full steps are in [Find out what went wrong](troubleshooting.md) and [Upgrading and removing](install.md#upgrading-and-removing).

## What vtium does

- [Text in any script](home/text.md): characters as a reader sees them (grapheme clusters) lined up in character cells, shaping for complex scripts, right-to-left text, and the fonts vtium reads.
- [A terminal on each display](home/displays.md): one vtium drives several displays at once, each with its own independent terminal, keyboard, and mouse, for several people working at one machine.
- [Images](home/images.md): sixel, the kitty graphics protocol, and iTerm2's inline images.
- [Keyboards and mice](home/input.md): how vtium reads your devices, the key and mouse encodings programs receive, and the keys that act on the machine.
- [The terminal's language](home/language.md): the specifications vtium follows, and how it settles a conflict between two of them.
- [Where vtium runs](home/where-it-runs.md): processors, graphics, init systems, and the smallest build.
- [Security and the system](home/security-and-system.md): sandboxed font and image helpers, replies to queries that never let output type into your shell, and how vtium works with systemd and logind.
- [What vtium leaves to other tools](home/leaves-to-other-tools.md): opening links, split panes, window management, and other jobs that belong to other programs.

## Where to start

On Arch, the AUR carries `vtium`, built from the latest release, and `vtium-git`, built from the tip of `main`. Both build vtium with images, right-to-left text, the GPU path, seat mode, and the power keys, and they turn the GPU path on. vtium is packaged only for Arch so far. [Install vtium and give it VTs](install.md) covers the other Arch packages, other distributions, and building from a checkout. The `vtium` recipe checks the maintainer's signing key, and the recipe carries its own copy of that key:

```
$ git clone https://aur.archlinux.org/vtium.git
$ cd vtium
$ gpg --import keys/pgp/33F9B54FD2A4B6A4110749B3BA464FC9847A832A.asc
$ makepkg -si
```

Because that key comes from the same clone it verifies, you can fetch it from a keyserver instead with `gpg --recv-keys 33F9B54FD2A4B6A4110749B3BA464FC9847A832A`. [The releases](https://github.com/vtium/vtium/releases) list what changed in each version. This site describes vtium's `main` branch, which may be ahead of the latest release.

Once vtium is installed, it reads its settings from `/etc/vtium/vtium.conf`. Rules in that file place each display, keyboard, and mouse in a head, vtium's name for a terminal that spans one or more displays. vtium-config, vtium's setup tool, edits the file from a terminal, and `vtium config tui` opens it. Its Flow view draws vtium.conf as a diagram, from the displays through the rules and heads to the keyboards and mice. Your edits go into a draft of your own until a single `sudo` step installs the file. Its Fonts view previews each font on the live console as you move through the list. Three things are still missing: identifying a device from a keypress on it, `vtium config status`, and trying a whole configuration on the console before installing it. [The vtium-config issue](https://github.com/vtium/vtium/issues/367) lists the rest.

| If you want to | Start here |
|---|---|
| try vtium on one VT and get back to your desktop afterward | [Try vtium on one VT](tutorial-try-vtium.md) |
| install vtium, configure it, choose a font, set up heads, or fix a problem | [Install vtium and give it VTs](install.md), [Configure vtium](configure.md), [Choose a font](fonts.md), [Set up heads](heads.md), or [Find out what went wrong](troubleshooting.md) |
| look up a feature, a key, or a setting | [What vtium does](features.md), [Keys](keys.md), [Configuration keys](config.md), [The head file](head-file.md), [Modes](modes.md), or [systemd, logind and PID 1](systemd.md) |
| understand how vtium works and why | [VTium in depth](vtium-in-depth.md), [Why vtium](why-vtium.md), and [Concepts](concepts/index.md) |
| read the reasoning behind a decision | [Decision records](architecture/INDEX.md) |
| contribute | [For contributors](architecture.md), [CONTRIBUTING.md](https://github.com/vtium/vtium/blob/main/CONTRIBUTING.md), and [AGENTS.md](https://github.com/vtium/vtium/blob/main/AGENTS.md) |

vtium is written in Rust. Its code began as a port of two programs written in C, a history that [Provenance](vtium-in-depth.md#provenance) tells in full. The source is at [github.com/vtium/vtium](https://github.com/vtium/vtium).
