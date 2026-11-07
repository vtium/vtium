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
[![Gentoo](https://repology.org/badge/version-for-repo/gentoo/vtium.svg)](https://repology.org/project/vtium/versions)
[![Void](https://repology.org/badge/version-for-repo/void_x86_64/vtium.svg)](https://repology.org/project/vtium/versions)

**vtium is a terminal for the Linux console.**

vtium runs on a Linux virtual terminal (VT), one of the text screens that Ctrl+Alt and a function key switch to, in place of getty, the program that prints the login prompt there. It draws the screen itself, with no display server and no GPU, and gives the programs on it what a terminal on a desktop gives them: the full Unicode character set, color emoji, 24-bit color, mouse reporting and pictures. When vtium cannot start, getty and the kernel's console take the VT back.

The kernel's own console has stayed plain so that it works when nothing else does. Since Linux 6.10 the kernel can draw its own crash screen through its modern graphics drivers, and the Red Hat graphics developers behind that work call the VT console one of the [legacy parts of the kernel](https://fedoraproject.org/wiki/Changes/EnableDrmPanic), to be replaced by a console that runs as an ordinary program. vtium is a console of that kind.

vtium is built for machines from a small board with 512 MB of memory and a 320x240 display to a desktop with several large monitors. It is written in Rust, and began as a port of `kmscon` and its library `libtsm`, which are written in C ([Provenance](docs/vtium-in-depth.md#provenance)).

**Documentation:** [vtium.github.io/vtium](https://vtium.github.io/vtium/)

![A shell session on a Linux console in a smooth monospaced font: a colored prompt, a directory listing, a git log with a branch graph, a row of six pictures with their file names, greetings in several languages and scripts with emoji, and a line of underline and text styles.](docs/showcase/hero.png)

*A shell session as vtium draws it on a 1920x1080 display. [The showcase](docs/showcase/README.md) has more screens and says how they were captured.*

[VTium in depth](docs/vtium-in-depth.md) explains what a terminal is and how the Linux console came to be plain, then gives what vtium does, its status, what it needs, and how to try and configure it.

## Status

vtium is alpha software. Its latest release is the pre-release [v0.1.0-alpha.5](https://github.com/vtium/vtium/releases). [Try vtium on one VT](docs/tutorial-try-vtium.md) runs it on one spare VT and gives the VT back, while your desktop keeps running.

## Documentation

| If you want to | Read |
|---|---|
| learn by doing | the tutorial [Try vtium on one VT](docs/tutorial-try-vtium.md) |
| get one task done | the how-to guides: [Install vtium and give it VTs](docs/install.md), [Configure vtium](docs/configure.md), [Choose a font](docs/fonts.md), [Set up heads](docs/heads.md), [Find out what went wrong](docs/troubleshooting.md) |
| look something up | the reference: [What vtium does](docs/features.md), [Keys](docs/keys.md), [Configuration keys](docs/config.md), [The head file](docs/head-file.md), [Modes](docs/modes.md), [systemd, logind and PID 1](docs/systemd.md) |
| understand why and how | the explanation: [VTium in depth](docs/vtium-in-depth.md), [Why vtium](docs/why-vtium.md), [Concepts](docs/concepts/index.md) |
| know why a decision was made | [the decision records](docs/architecture/INDEX.md) |
| contribute | [Architecture](docs/architecture.md), [CONTRIBUTING.md](CONTRIBUTING.md), and [AGENTS.md](AGENTS.md), which gives every gate and test command |

## Contributing

The maintainer's code in vtium is written by large language models, under the maintainer's review, and judged by vtium's tests. [The decision records](docs/architecture/INDEX.md) keep each decision in the maintainer's words.

Contributions are welcome, whether written by hand or with a large language model. Each pull request says how its code was written, and every change goes through the same tests and the same review. [CONTRIBUTING.md](CONTRIBUTING.md) explains the process.

## License

vtium is under the MIT license ([LICENSE](LICENSE)). The Unifont glyphs built into the console are under the SIL Open Font License 1.1, and Unscii-8 is in the public domain. [COPYING](COPYING) gives the details, and [crates/vtium-font/README.md](crates/vtium-font/README.md) describes the built-in fonts.
