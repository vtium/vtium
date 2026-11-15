# VTium

[![CI](https://github.com/vtium/vtium/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/vtium/vtium/actions/workflows/ci.yml)
[![Docs](https://github.com/vtium/vtium/actions/workflows/docs.yml/badge.svg?branch=main)](https://vtium.github.io/vtium/)
[![Release](https://img.shields.io/github/v/release/vtium/vtium?include_prereleases&label=release)](https://github.com/vtium/vtium/releases)
[![Platform](https://img.shields.io/badge/platform-Linux%20console%20(KMS%2FDRM%2C%20fbdev)-informational)](docs/features.md#displays)
[![Architectures](https://img.shields.io/badge/arch-x86__64%20%7C%20aarch64%20%7C%20armv7-informational)](docs/features.md#small-machines)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange)](Cargo.toml)
[![License](https://img.shields.io/github/license/vtium/vtium)](LICENSE)

[![AUR](https://repology.org/badge/version-for-repo/aur/vtium.svg)](https://repology.org/project/vtium/versions)
[![Debian](https://repology.org/badge/version-for-repo/debian_unstable/vtium.svg)](https://repology.org/project/vtium/versions)
[![Ubuntu](https://repology.org/badge/version-for-repo/ubuntu_25_10/vtium.svg)](https://repology.org/project/vtium/versions)
[![Fedora](https://repology.org/badge/version-for-repo/fedora_rawhide/vtium.svg)](https://repology.org/project/vtium/versions)
[![openSUSE](https://repology.org/badge/version-for-repo/opensuse_tumbleweed/vtium.svg)](https://repology.org/project/vtium/versions)
[![nixpkgs](https://repology.org/badge/version-for-repo/nix_unstable/vtium.svg)](https://repology.org/project/vtium/versions)
[![Alpine](https://repology.org/badge/version-for-repo/alpine_edge/vtium.svg)](https://repology.org/project/vtium/versions)
[![postmarketOS](https://repology.org/badge/version-for-repo/postmarketos_master/vtium.svg)](https://repology.org/project/vtium/versions)
[![Gentoo](https://repology.org/badge/version-for-repo/gentoo/vtium.svg)](https://repology.org/project/vtium/versions)
[![Void](https://repology.org/badge/version-for-repo/void_x86_64/vtium.svg)](https://repology.org/project/vtium/versions)

**vtium is a terminal that runs on the Linux console.**

vtium runs on a Linux virtual terminal (VT), one of the text screens you reach with Ctrl+Alt and a function key. It takes the place of getty, the program that normally shows the login prompt there. Your machine then boots into a terminal with text in any script, color emoji, 24-bit color, images, and mouse reporting. vtium renders through [KMS/DRM](https://en.wikipedia.org/wiki/Direct_Rendering_Manager), the modern graphics interface, on the CPU or the GPU.

**Documentation:** [vtium.github.io/vtium](https://vtium.github.io/vtium/)

![A shell session showing a colored prompt, directory listing, git branch graph, six images with filenames, multilingual text and emoji, and examples of underline and other text styles.](docs/showcase/hero.png)

*A shell session rendered by vtium on a 1920×1080 display. [The showcase](docs/showcase/README.md) has more screenshots and explains how they were captured.*

## What vtium does

- [Text in any script](https://vtium.github.io/vtium/home/text/): grapheme clusters lined up in character cells, shaping for complex scripts, right-to-left text, and the fonts vtium reads.
- [A terminal on each display](https://vtium.github.io/vtium/home/displays/): one vtium can give each display its own terminal and keyboard.
- [Images](https://vtium.github.io/vtium/home/images/): sixel, the kitty graphics protocol, and iTerm2's inline images.
- [Keyboards and mice](https://vtium.github.io/vtium/home/input/): how vtium reads your devices, and the key and mouse encodings programs receive.
- [The terminal's language](https://vtium.github.io/vtium/home/language/): the specifications vtium follows, and how it settles a conflict between two of them.
- [Where vtium runs](https://vtium.github.io/vtium/home/where-it-runs/): processors, graphics, init systems, and the smallest build.
- [What vtium leaves to other tools](https://vtium.github.io/vtium/home/leaves-to-other-tools/): opening links, split panes, window management, and other jobs that belong to other programs.

## Where to start

[The releases](https://github.com/vtium/vtium/releases) list the latest version and what changed in it. To try vtium while your desktop keeps running, follow [Try vtium on one VT](docs/tutorial-try-vtium.md). The tutorial runs vtium on a spare VT and puts that VT back the way it was afterward.

| If you want to… | Start here |
|---|---|
| Learn by doing | [Try vtium on one VT](docs/tutorial-try-vtium.md) |
| Complete a specific task | [Install vtium and give it VTs](docs/install.md), [Configure vtium](docs/configure.md), [Choose a font](docs/fonts.md), [Set up heads](docs/heads.md), or [Find out what went wrong](docs/troubleshooting.md) |
| Look something up | [What vtium does](docs/features.md), [Keys](docs/keys.md), [Configuration keys](docs/config.md), [The head file](docs/head-file.md), [Modes](docs/modes.md), or [systemd, logind and PID 1](docs/systemd.md) |
| Understand how vtium works and why | [VTium in depth](docs/vtium-in-depth.md), [Why vtium](docs/why-vtium.md), and [Concepts](docs/concepts/index.md) |
| Understand a design decision | [Decision records](docs/architecture/INDEX.md) |
| Contribute | [Architecture](docs/architecture.md), [CONTRIBUTING.md](CONTRIBUTING.md), and [AGENTS.md](AGENTS.md), which lists all required checks and test commands |

## Contributing

vtium is written in Rust. Its code began as a port of two programs written in C, a history that [Provenance](docs/vtium-in-depth.md#provenance) tells in full.

The maintainer uses large language models to write code for vtium, reviews the results, and evaluates them with the project's tests. The [decision records](docs/architecture/INDEX.md) give the reasoning behind each decision in the maintainer's own words.

Contributions are welcome, whether you write them by hand or with a large language model. Each pull request must say how its code was written, and every change goes through the same tests and review. See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## License

vtium is licensed under the [MIT License](LICENSE). The bundled Unifont glyphs are licensed under the SIL Open Font License 1.1, and Unscii-8 is in the public domain.

See [COPYING](COPYING) for licensing details and [crates/vtium-font/README.md](crates/vtium-font/README.md) for information about the built-in fonts.
