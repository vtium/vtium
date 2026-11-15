# A terminal on each display

One vtium drives every connected display. With no configuration, it shows a single terminal across all of them.

That terminal is a head: a terminal for one or more displays, with its own grid of character cells, its own font, and its own consoles. Each console is one session with its own login, screen, and scrollback, and keys switch a head between its consoles.

## Rules place each device

Rules in vtium.conf assign each display, keyboard, and mouse to a head. vtium checks a device against the rules from the top, and the first match wins. That lets each display show its own terminal with its own keyboard. For example, one monitor could run a kiosk while another provides an operator's shell.

## Width changes and restarts

When a new font or a display changes the terminal's width, vtium rewraps the text, scrollback included. Restarting vtium leaves your sessions running, with one exception that [Logins and sessions](../concepts/security.md#logins-and-sessions) explains.

## More about heads

- [Set up heads](../heads.md): the sections of vtium.conf for each setup, from the default to a kiosk.
- [Heads](../concepts/heads.md): how heads, rules, and policies fit together.
