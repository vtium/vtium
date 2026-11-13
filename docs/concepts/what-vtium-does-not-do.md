# What VTium does not do

vtium leaves out some things on purpose. Each limit below is recorded with its reason.

- **It never opens a link.** vtium never starts a browser or any other program from the console. It only copies a link's address.
- **It never hands the clipboard to a program.** When a program asks to read the selection, vtium does not answer. The answer would go to whatever program reads the terminal.
- **It never repeats a window title back.** When a program asks for the window title, vtium answers with a fixed empty title. Otherwise a hostile program could set a title and have vtium send it back as input. See [architecture.md](../architecture.md#determinism) for details.
- **It does not sandbox its unit.** vtium's systemd unit, the file that tells systemd how to run vtium, sets no sandbox restrictions. The login session that vtium starts would inherit every restriction the unit set.
