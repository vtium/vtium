# A terminal on each display

One vtium drives every connected display. With no configuration, vtium shows one terminal across all the displays.

A head is a terminal for one or more displays, with its own grid of character cells, its own font, and its own consoles. A console is one session, with its own login, screen, and scrollback. Keys switch a head between its consoles.

## Rules place each device

Rules in vtium.conf place each display, keyboard, and mouse in a head. vtium checks each device against the rules from the top, and the first rule that matches decides. So each display can show a terminal of its own, with a keyboard of its own. For example, one monitor could run a kiosk while another provides an operator's shell.

## Width changes and restarts

When the font or a display changes the width, vtium rewraps the text and the scrollback. A restart of vtium keeps your sessions running. See [Logins and sessions](../concepts/security.md#logins-and-sessions) for the one case where the sessions end.

## More about heads

- [Set up heads](../heads.md) gives the sections of vtium.conf for each setup, from the default to a kiosk.
- [Heads](../concepts/heads.md) explains how heads, rules, and policies fit together.
