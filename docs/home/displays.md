# A terminal on each display

One vtium drives every connected display. With no configuration, it shows a single terminal across all of them.

That terminal is a head: a terminal for one or more displays, with its own grid of character cells, its own font, and its own consoles. Each console is one session with its own login, screen, and scrollback, and keys switch a head between its consoles.

## Rules place each device

Rules in vtium.conf assign each display, keyboard, and mouse to a head. vtium checks a device against the rules from the top, and the first match wins. That lets each display show its own terminal with its own keyboard. For example, one monitor could run a kiosk while another provides an operator's shell.

## Width changes and restarts

When a new font or a display changes the terminal's width, vtium rewraps the text, scrollback included.

Restarting vtium leaves your sessions running, because systemd, or a small holder process on a machine without systemd, keeps each session's terminal while vtium restarts. The one exception is a crash on a systemd machine under the default open policy. systemd then starts a getty on the VT instead of vtium, and your sessions end. [Logins and sessions](../concepts/security.md#logins-and-sessions) has the details.

To build a setup of your own, from the default to a kiosk, follow [Set up heads](../heads.md). [Heads](../concepts/heads.md) explains how heads, rules, and policies fit together.
