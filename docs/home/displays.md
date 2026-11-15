# A terminal on each display

One vtium drives every connected display at the same time, on one graphics card or several. With a few rules in vtium.conf, each display shows an independent terminal, and each terminal gets its own keyboard and mouse. Several people then work at one machine at once, each at their own screen. [Set up heads](../heads.md#stations-one-person-to-each-head) gives that setup, which runs under the closed policy so that no one's keys can switch the others' screens away.

Each of those terminals is a head: a terminal for one or more displays, with its own grid of character cells, its own font, and its own consoles. A console is one session with its own login, screen, and scrollback, and keys switch a head between its consoles. With no configuration, vtium runs a single head that every display shows.

## Rules place each device

The rules assign each display, keyboard, and mouse to a head. vtium checks a device against them from the top, and the first match wins. A rule can name a display by its connector, its graphics card, or the identity its monitor reports. For example, one monitor runs a kiosk while another provides an operator's shell.

## Width changes and restarts

When a new font or a display changes the terminal's width, vtium rewraps the text, scrollback included.

Restarting vtium leaves your sessions running. The one exception is a crash on a systemd machine under the default open policy. systemd then starts a getty on the VT instead of vtium, and your sessions end. [Logins and sessions](../concepts/security.md#logins-and-sessions) has the details.

To build a setup of your own, from the default to a kiosk, follow [Set up heads](../heads.md). [Heads](../concepts/heads.md) explains how heads, rules, and policies fit together.
