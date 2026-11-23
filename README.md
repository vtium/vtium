# VTium

[![CI](https://github.com/vtium/vtium/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/vtium/vtium/actions/workflows/ci.yml)
[![Docs](https://github.com/vtium/vtium/actions/workflows/docs.yml/badge.svg?branch=main)](https://vtium.github.io/vtium/)
[![Release](https://img.shields.io/github/v/release/vtium/vtium?include_prereleases&label=release)](https://github.com/vtium/vtium/releases)
[![Platform](https://img.shields.io/badge/platform-Linux%20console%20(KMS%2FDRM%2C%20fbdev)-informational)](docs/features.md#displays)
[![Architectures](https://img.shields.io/badge/arch-x86__64%20%7C%20aarch64%20%7C%20armv7-informational)](docs/features.md#small-machines)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange)](Cargo.toml)
[![License](https://img.shields.io/github/license/vtium/vtium)](LICENSE)

[![AUR](https://repology.org/badge/version-for-repo/aur/vtium.svg)](https://repology.org/project/vtium/versions)

**vtium is a terminal that runs on the Linux console.**

vtium runs on a Linux virtual terminal (VT), one of the text screens you reach with Ctrl+Alt and a function key. It takes the place of getty, the program that normally shows the login prompt there. The console your machine boots into becomes a full terminal, with 24-bit color, color emoji, images, mouse reporting, and text in the scripts your fonts cover. Rendering goes through [KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the modern graphics interface, on the CPU or the GPU.

That helps wherever you work at the machine's own screen: a server without a desktop, a kiosk, a small board, or the console before a desktop starts. Editors, file managers and tmux get the color, mouse and image support they expect there.

This is alpha software, at version 0.1.0-alpha.10. Installing the package gives vtium every free VT from that VT's next start. On a machine with no display manager, that includes VT 1, the screen where the boot messages appear. To try one VT only, follow [Try vtium on one VT](docs/tutorial-try-vtium.md) instead.

**Documentation:** [vtium.github.io/vtium](https://vtium.github.io/vtium/)

[![Nine parts of vtium's showcase screens, in three rows of three. The first row shows a title in letters of double size, rows of color emoji, and the Mandelbrot set drawn in sixel. The second row shows Tamil, Hindi and Kannada syllables, a shell prompt above a gallery of pictures, and three cubes of the 256 colors. The third row shows Rust code with programming ligatures and a bar graph in a box. Last comes one sentence in Chinese, Japanese, Korean, Thai, Vietnamese, Hindi and Bengali.](docs/showcase/grid.png)](docs/showcase/README.md)

*Parts of nine showcase screens rendered by vtium, each cropped at full size with no scaling. [The showcase](docs/showcase/README.md) has the whole screens and explains how they were captured.*

## If something goes wrong

- **Get off the VT.** Ctrl+Alt and a function key switch to another VT or to your desktop. If vtium has stopped answering keys, that switch fails too, and you work from another machine over ssh instead.
- **Fall back to a login prompt.** When vtium fails under systemd, a getty starts on its VT, so you get a plain login prompt rather than a dead screen. [Security and the system](https://vtium.github.io/vtium/home/security-and-system/#with-systemd-and-logind-and-without-them) names the two setups where no getty starts.
- **Give a VT back.** From another VT or over ssh, `vtium console disable 3` returns VT 3 to its getty from the VT's next start, and `systemctl start getty@tty3.service` does it at once. `vtium console disable --all` returns every VT.
- **Boot once without vtium.** Add `vtium.enable=0` to the kernel command line in your boot loader.
- **Uninstall.** Stop vtium on each VT where it runs, for example with `systemctl stop vtium@tty3.service`, then run `pacman -R vtium`, or `pacman -R vtium-git`. The package's removal script gives every VT back what it had.

The full steps are in [Find out what went wrong](docs/troubleshooting.md) and [Upgrading and removing](docs/install.md#upgrading-and-removing).

## What vtium does

- [Text in any script](https://vtium.github.io/vtium/home/text/): characters as a reader sees them (grapheme clusters) lined up in character cells, shaping for complex scripts, right-to-left text, and the fonts vtium reads.
- [Looks and effects](https://vtium.github.io/vtium/concepts/looks/): color schemes for each display's terminal, and effects that change the finished picture, such as an amber tint or a correction for color blindness.
- [A terminal on each display](https://vtium.github.io/vtium/home/displays/): one vtium drives several displays at once, each with its own independent terminal, keyboard, and mouse, for several people working at one machine.
- [Images](https://vtium.github.io/vtium/home/images/): sixel, the kitty graphics protocol, and iTerm2's inline images.
- [Keyboards and mice](https://vtium.github.io/vtium/home/input/): how vtium reads your devices, the key and mouse encodings programs receive, and the keys that act on the machine.
- [The terminal's language](https://vtium.github.io/vtium/home/language/): the specifications vtium follows, and how it settles a conflict between two of them.
- [Where vtium runs](https://vtium.github.io/vtium/home/where-it-runs/): processors, graphics, init systems, and the smallest build.
- [Security and the system](https://vtium.github.io/vtium/home/security-and-system/): sandboxed font and image helpers, replies to queries that never let output type into your shell, and how vtium works with systemd and logind.
- [What vtium leaves to other tools](https://vtium.github.io/vtium/home/leaves-to-other-tools/): opening links, split panes, window management, and other jobs that belong to other programs.

## Where to start

On Arch, the AUR carries `vtium`, built from the latest release, and `vtium-git`, built from the tip of `main`. Both build vtium with images, right-to-left text, effects, the GPU path, seat mode, and logind support, so the suspend key asks logind to suspend the machine. They also turn the GPU path on. vtium is packaged only for Arch so far. [Install vtium and give it VTs](docs/install.md) covers the other Arch packages, other distributions, and building from a checkout. The `vtium` recipe checks the maintainer's signing key, and the recipe carries its own copy of that key:

```
$ git clone https://aur.archlinux.org/vtium.git
$ cd vtium
$ gpg --import keys/pgp/33F9B54FD2A4B6A4110749B3BA464FC9847A832A.asc
$ makepkg -si
```

Because that key comes from the same clone it verifies, you can fetch it from a keyserver instead with `gpg --recv-keys 33F9B54FD2A4B6A4110749B3BA464FC9847A832A`. [The releases](https://github.com/vtium/vtium/releases) list what changed in each version. This README and [the documentation site](https://vtium.github.io/vtium/) describe vtium's `main` branch, which may be ahead of the latest release.

Once vtium is installed, it reads its settings from `/etc/vtium/vtium.conf`. Rules in that file place each display, keyboard, and mouse in a head, vtium's name for a terminal that spans one or more displays. vtium-config, vtium's setup tool, edits the file from a terminal, and `vtium config tui` opens its full-screen view. Its Flow tab draws vtium.conf as a diagram, from the displays through the rules and heads to the keyboards and mice. Your edits go into a draft of your own until a single `sudo` step installs the file. Its Fonts tab lists the fonts on the machine, with a preview of each. Its Looks tab can try a font set on the live console for 10 seconds before you keep it. Two things are still missing: identifying a device from a keypress on it, and trying a whole configuration on the console before installing it. [The vtium-config issue](https://github.com/vtium/vtium/issues/367) lists the rest.

| If you want to | Start here |
|---|---|
| try vtium on one VT and get back to your desktop afterward | [Try vtium on one VT](docs/tutorial-try-vtium.md) |
| install vtium, configure it, choose a font, set up heads, or fix a problem | [Install vtium and give it VTs](docs/install.md), [Configure vtium](docs/configure.md), [Choose a font](docs/fonts.md), [Set up heads](docs/heads.md), or [Find out what went wrong](docs/troubleshooting.md) |
| look up a feature, a key, or a setting | [What vtium does](docs/features.md), [Keys](docs/keys.md), [Configuration keys](docs/config.md), [The head file](docs/head-file.md), [Modes](docs/modes.md), or [systemd, logind and PID 1](docs/systemd.md) |
| understand how vtium works and why | [VTium in depth](docs/vtium-in-depth.md), [Why vtium](docs/why-vtium.md), and [Concepts](docs/concepts/index.md) |
| read the reasoning behind a decision | [Decision records](docs/architecture/INDEX.md) |
| contribute | [Architecture](docs/architecture.md), [CONTRIBUTING.md](CONTRIBUTING.md), and [AGENTS.md](AGENTS.md), which lists all required checks and test commands |

## Contributing

vtium is written in Rust. Its code began as a port of kmscon and libtsm which were written in C, a history that [Provenance](docs/vtium-in-depth.md#provenance) tells in full.

The maintainer uses large language models to write code for vtium, reviews the results, and evaluates them with the project's tests. The [decision records](docs/architecture/INDEX.md) give the reasoning behind each decision in the maintainer's own words.

Contributions are welcome, whether you write them by hand or with a large language model. Each pull request must say how its code was written, and every change goes through the same tests and review. See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## License

vtium is licensed under the [MIT License](LICENSE). The bundled Unifont glyphs and the parts of Terminus Font in vtium's built-in font are licensed under the SIL Open Font License 1.1, and Unscii-8 is in the public domain. [COPYING](COPYING) lists every third-party notice.

See [COPYING](COPYING) for licensing details and [crates/vtium-font/README.md](crates/vtium-font/README.md) for information about the built-in fonts.
