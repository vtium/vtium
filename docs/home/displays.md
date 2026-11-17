# A terminal on each display

One vtium drives every connected display at the same time, on one graphics card or several. With a few rules in vtium.conf, each display shows an independent terminal, and each terminal gets its own keyboard and mouse. Several people then work at one machine at once, each at their own screen. That setup runs under the closed policy, where vtium ignores any keyboard that no rule names, so that no one's keys can switch the others' screens away. [Stations: one person to each head](../heads.md#stations-one-person-to-each-head) gives the whole file.

![Two panels. In the first, one head, desk, shows on an 800×480 panel and on a 1920×1080 monitor. Both show a grid of 100 columns by 30 rows, the grid that the smaller display holds, and the larger display draws bigger characters. In the second, the head panel shows on the 800×480 panel with a grid of 100×30, and the head desk shows on the 1920×1080 monitor with a grid of 240×67. Each head has a grid sized for its own display, and its own sessions. A note says that the grids are counted in character cells for an 8×16 font.](../figures/concepts-heads-mirror.svg)

*[The showcase](../showcase/displays-and-vts.md) has more screens like this one.*

Each of those terminals is a head: a terminal for one or more displays, with its own grid of character cells, its own font, and its own consoles. A console is one session with its own login, screen, and scrollback, and keys switch a head between its consoles. With no configuration, there is a single head that every display shows.

## Rules place each device

The rules assign each display, keyboard, and mouse to a head. Each device is checked against them from the top, and the first match wins. A rule can name a display by its connector, its graphics card, or the identity its monitor reports. For example, one monitor runs a kiosk while another provides an operator's shell.

## Width changes and restarts

When a new font or a display changes the terminal's width, the text is rewrapped, scrollback included.

Your sessions survive a restart of vtium, with one exception: a crash on a systemd machine under the default open policy. There, systemd starts a getty on the VT instead of vtium, and the sessions end. [Logins and sessions](../concepts/security.md#logins-and-sessions) has the details.

Ready-made setups, from the default to a kiosk, are in [Set up heads](../heads.md), and [Heads](../concepts/heads.md) explains the model behind them.
