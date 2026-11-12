# Managing VTium

vtium is a terminal for the Linux virtual console. The virtual console is the set of text screens that you switch between with Ctrl+Alt and a function key, and each of those screens is a VT. The guides below give the steps you take to run vtium day to day. [why-vtium.md](why-vtium.md) says why you might choose vtium. [Concepts](concepts/index.md) explains its pieces.

- [Install vtium and give it VTs](install.md) covers installing on Arch Linux or from a checkout, and giving vtium one VT or every VT. It also covers moving from another terminal on a DRM/KMS VT, sharing VTs with `kmscon`, and upgrading and removing vtium.
- [Configure vtium](configure.md) covers vtium.conf and checking it. It shows how to change the file through a draft with `vtium config`, and how to install the draft. It also covers blanking a VT from a script.
- [Choose a font](fonts.md) covers an outline font or a console font, and its size. It shows how to choose one with `vtium config`, and how to switch fonts and zoom for a session.
- [Set up heads](heads.md) gives the sections of vtium.conf for each common setup.
- [Find out what went wrong](troubleshooting.md) shows how to get out of a VT where vtium misbehaves, and how to read what each VT runs. It also shows how to collect a report for an issue.

[Keys](keys.md) lists the keys vtium keeps for itself.

## See also

- [crates/vtium/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#setting-it-up): the full rules of `vtium console`, sharing VTs with `kmscon`, and ssh from vtium
- [config.md](config.md): every configuration key
- [heads.md](heads.md): vtium.conf's heads, looks and rules, with an example for each setup
- [systemd.md](systemd.md): where vtium touches systemd, logind or PID 1, and how it runs without them
- [Concepts](concepts/index.md): vtium's pieces and how they fit together
- [vt-review.md](vt-review.md): the maintainer's guided review of a build on a VT, and the development install
- [vtium-arch](https://github.com/vtium/vtium-arch): the Arch Linux recipes
- [The issue tracker](https://github.com/vtium/vtium/issues)
