# What vtium leaves to other tools

vtium leaves some jobs to other programs:

- vtium copies a link's address and never opens the link.
- A program can copy text to vtium's clipboard with OSC 52. No program can read the clipboard back.
- Each head shows one console at a time. vtium draws no tabs and no split panes. A terminal multiplexer such as tmux runs in vtium and splits a console into panes.
- A console has one screen and no window. So vtium moves and resizes no window, and keeps no window title.
- Desktop notifications, file transfer, and remote control of the terminal belong to other tools.

See [What vtium does not do](../concepts/what-vtium-does-not-do.md) for the reasons.
