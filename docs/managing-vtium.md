# Managing VTium

vtium is a terminal for the Linux virtual console. It runs on a VT, one of the full-screen text consoles the kernel provides, in the place of the getty, the program that prints the login prompt there. It takes the display through KMS/DRM, reads the keyboards and pointers itself, and draws text, color and pictures without a display server. The guides below hold the steps you take to run vtium day to day. [why-vtium.md](why-vtium.md) says why you might choose vtium, and [concepts.md](concepts.md) explains its pieces.

- [Install vtium and give it VTs](install.md): installing on Arch Linux or from a checkout, giving vtium one VT or every VT, moving from `kmscon`, sharing VTs with it, upgrading and removing.
- [Configure vtium](configure.md): writing `vtium.conf`, checking it, and blanking a VT from a script.
- [Choose a font](fonts.md): an outline font or a console font, its size, choosing one with `vtium config`, and switching and zooming for a session.
- [Set up heads](heads.md): a complete head file for each common setup.
- [Find out what went wrong](troubleshooting.md): getting out of a VT where vtium misbehaves, reading what each VT runs, and collecting a report for an issue.

[Keys](keys.md) lists the keys vtium keeps for itself.

## How the commands are written

In the commands below, `#` marks a command you run as root, for example with sudo, and `$` a command you run as your own user. A line that starts `$ sudo` runs as root, but your own shell reads the line first, which matters where the line names your home directory.

## See also

- [crates/vtium/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#setting-it-up): the full rules of `vtium console`, sharing VTs with `kmscon`, and ssh from vtium
- [config.md](config.md): every configuration key
- [heads.md](heads.md): the head file, with an example for each setup
- [systemd.md](systemd.md): where vtium touches systemd, logind or PID 1, and how it runs without them
- [concepts.md](concepts.md): vtium's pieces and how they fit together
- [vt-review.md](vt-review.md): the maintainer's guided review of a build on a VT, and the development install
- [vtium-arch](https://github.com/vtium/vtium-arch): the Arch Linux recipes
- [The issue tracker](https://github.com/vtium/vtium/issues)
