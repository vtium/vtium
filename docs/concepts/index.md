# VTium's concepts

These pages explain how vtium works. vtium is a terminal for the Linux virtual console, or VT. It renders the screen itself, so it needs no display server. It runs on machines from a small board with a 320×240 panel to a desktop with a large monitor.

Each page explains one concept, so you can read them one at a time. Each page links to the decision record, issue or document that holds the details. The commands and the names of settings are there too. A part of vtium that is not built yet is marked proposed, and the page links the issue or record that tracks it.

- [What vtium is](what-vtium-is.md): what vtium draws, how it draws it, and the small machines it is built for first.
- [How vtium works](how-vtium-works.md): the processes and threads that run, the channels between them, and the states of the VT and of a session, in four figures.
- [The suite](suite.md): the console and the programs and files around it, who runs each, and what runs as root and what as the user.
- [Deployment and start-up](start-up.md): how a package installs vtium, and how vtium takes the place of getty. It also explains what happens when vtium takes a VT, and when the user switches away.
- [Seats and sessions](seats-and-sessions.md): which program opens the screen and the keyboards, which program holds your login session, and what that session can do. The page covers each build of vtium, each init system and seat manager, and both of vtium's modes, direct mode and seat mode.
- [Heads](heads.md): a terminal for each display, the rules of vtium.conf, its two policies, open and closed, and the operator's way back in.
- [From keyboards to displays](flows.md): how vtium.conf joins displays, heads and keyboards into paths, pictured as vtium-config draws them.
- [Security](security.md): which parts of vtium run as root and which run with fewer rights, and what a program in your session can make vtium do. It also covers who may change vtium.conf, which keyboard can reach whose session, and where the protection ends.
- [Fonts, generated at run time](fonts.md): the frit, the helper that draws its glyphs, the built-in fonts and switching between fonts.
- [Using vtium](using-vtium.md): the configuration, the selection and the clipboard, links and prompts, keys and blanking.
- [How vtium is judged](how-vtium-is-judged.md): the tiers of tests, from unit tests up to a virtual machine and the maintainer's review on a real VT. It also explains the goldens, the saved expected results that the tests compare vtium's output with.
- [How vtium is built](how-vtium-is-built.md): how the project is laid out so that a person or an agent can find the part they work on. It also explains how one maintainer steers AI coding agents, and the checks that make the agents' work dependable.
- [What vtium does not do](what-vtium-does-not-do.md): the limits vtium keeps on purpose, each with its reason.
