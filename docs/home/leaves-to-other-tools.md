# What vtium leaves to other tools

Some jobs belong to other programs, and vtium leaves them there:

- **Links.** vtium copies a link's address but never opens the link.
- **The clipboard.** A program can copy text to vtium's clipboard with OSC 52, but no program can read the clipboard back.
- **Tabs and panes.** Each head, a terminal on one or more displays, shows one console at a time, without tabs or split panes. A terminal multiplexer such as tmux runs in vtium and splits a console into panes.
- **Windows.** vtium fills the screen itself, with no window, so it has no window to move or resize, and no window title to keep.
- **Desktop features.** Desktop notifications belong to a desktop's notification service, and files move with a file transfer program such as scp or rsync. vtium also offers no remote control of the terminal.

The reasons are in [What vtium does not do](../concepts/what-vtium-does-not-do.md).
