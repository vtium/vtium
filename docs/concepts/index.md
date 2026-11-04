# VTium's concepts

vtium is a terminal for the Linux virtual console, the full-screen text terminals the kernel provides, each called a VT. It draws straight to the display with no display server, runs on small machines first and large ones as well, and gives the programs run in it a modern terminal.

The pages below explain the ideas behind vtium, one concept a page. Each links to the decision record, issue or document that holds the detail, and the commands and setting names live there too. A piece that is not built yet is marked proposed, with the issue or record that holds it.

- [What vtium is](what-vtium-is.md): what vtium draws, how it draws it, and the small machines it is built for first.
- [The suite](suite.md): the console and the programs and files around it, who runs each, and what runs as root and what as the user.
- [Deployment and start-up](start-up.md): how a package installs vtium, how it takes the place of getty, and what happens when it takes a VT and when the user switches away.
- [Heads](heads.md): a terminal for each display, the head file's rules, its two policies, open and closed, and the operator's way back in.
- [Fonts, generated at run time](fonts.md): the frit, the helper that draws its glyphs, the built-in fonts and switching between fonts.
- [Using vtium](using-vtium.md): the configuration, the selection and the clipboard, links and prompts, keys and blanking.
- [How vtium is judged](how-vtium-is-judged.md): the tiers of tests, from unit tests to a virtual machine and the maintainer's review on a real VT, and the goldens that judge them.
- [How vtium is built](how-vtium-is-built.md): a project laid out so that a person or an agent can find the part they work on, one maintainer steering AI coding agents, and the checks that make their work dependable.
- [What vtium does not do](what-vtium-does-not-do.md): the limits vtium keeps on purpose, each with its reason.
