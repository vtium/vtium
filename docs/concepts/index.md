# VTium's concepts

These pages explain vtium, a terminal for the Linux virtual console, the VT, that renders the screen itself, so it needs no display server. It runs on machines from a small board with a 320×240 panel to a desktop with a large monitor.

Each page holds one concept, so a reader can open one at a time. Each links to the decision record, issue or document that holds the detail, and the commands and setting names live there too. A piece that is not built yet is marked proposed, with the issue or record that holds it.

- [What vtium is](what-vtium-is.md): what vtium draws, how it draws it, and the small machines it is built for first.
- [The suite](suite.md): the console and the programs and files around it, who runs each, and what runs as root and what as the user.
- [Deployment and start-up](start-up.md): how a package installs vtium, how it takes the place of getty, and what happens when it takes a VT and when the user switches away.
- [Seats and sessions](seats-and-sessions.md): who opens the screen and the keyboards, who holds your login session, and what that session can do, for each build of vtium, each init system and seat manager, and direct mode and seat mode.
- [Heads](heads.md): a terminal for each display, the rules of vtium.conf, its two policies, open and closed, and the operator's way back in.
- [Fonts, generated at run time](fonts.md): the frit, the helper that draws its glyphs, the built-in fonts and switching between fonts.
- [Using vtium](using-vtium.md): the configuration, the selection and the clipboard, links and prompts, keys and blanking.
- [How vtium is judged](how-vtium-is-judged.md): the tiers of tests, from unit tests to a virtual machine and the maintainer's review on a real VT, and the goldens that judge them.
- [How vtium is built](how-vtium-is-built.md): a project laid out so that a person or an agent can find the part they work on, one maintainer steering AI coding agents, and the checks that make their work dependable.
- [What vtium does not do](what-vtium-does-not-do.md): the limits vtium keeps on purpose, each with its reason.
