# Managing VTium

vtium is a terminal that runs on the Linux console. It runs on a Linux virtual terminal (VT), one of the text screens you reach with Ctrl+Alt and a function key. The guides below give the steps you take to run vtium day to day. [Why vtium](why-vtium.md) says why you might choose vtium. [Concepts](concepts/index.md) explains its pieces.

- [Set up vtium on an Arch desktop](express-setup.md) sets up vtium beside KDE Plasma and SDDM in about ten minutes. It gives a configuration to paste, a choice of fonts, and the steps that restore the machine.
- [Install vtium and give it VTs](install.md) covers installing on Arch Linux or from a checkout, and giving vtium one VT or every VT. It also covers upgrading and removing vtium, moving from another terminal that renders on a VT through KMS/DRM, and sharing VTs with `kmscon`, one such terminal.
- [Configure vtium](configure.md) covers vtium.conf and checking it. It shows how to change the file through a draft with `vtium config`, and how to install the draft. It also covers blanking a VT from a script.
- [Choose a font](fonts.md) covers an outline font or a console font, and its size. It shows how to choose one with `vtium config`, and how to switch fonts and zoom for a session.
- [Choosing what vtium may use](install.md#choosing-what-vtium-may-use) covers the list of active fonts, color schemes, color maps, shaders and pointer themes. It shows how to activate and deactivate each one with `vtium config`, and how to install a font of your own as a system font. A change to the list takes effect at the next login.
- [Color schemes](palettes.md) and [Vtlook files](looks.md) cover a head's colors and its effects, such as an amber tint or a correction for color blindness.
- [Set up heads](heads.md) gives the sections of vtium.conf for each common setup.
- [Find out what went wrong](troubleshooting.md) shows how to get out of a VT where vtium misbehaves, and how to read what each VT runs. It also shows how to collect a report for an issue.

[Keys](keys.md) lists the keys vtium keeps for itself.

## See also

- [Setting it up](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#setting-it-up), in the vtium crate's README: the full rules of `vtium console`, sharing VTs with `kmscon`, and ssh from vtium
- [Configuration keys](config.md): every configuration key
- [Heads, by scenario](heads.md): vtium.conf's heads, looks and rules, with an example for each setup
- [systemd, logind and PID 1](systemd.md): where vtium touches systemd, logind or PID 1, and how it runs without them
- [Concepts](concepts/index.md): vtium's pieces and how they fit together
- [vtium-arch](https://github.com/vtium/vtium-arch): the Arch Linux recipes
- [The issue tracker](https://github.com/vtium/vtium/issues)
