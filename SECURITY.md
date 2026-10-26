# Security

## Reporting a vulnerability

Report vulnerabilities privately through GitHub's private vulnerability reporting: open the [Security tab](https://github.com/vtium/vtium/security) and choose **Report a vulnerability**. Only the maintainers can read the report. Do not report a vulnerability in a public issue, pull request or discussion.

## Scope

vtium runs as root on a virtual console. Two inputs cross that privilege boundary:

- **The pty byte stream.** Any program running in the terminal can write any bytes to it, including programs that print untrusted files or network data.
- **The image decoders.** Sixel and kitty graphics data arrive through the same byte stream, and a kitty graphics command can name a file or shared memory object for vtium to read.

A flaw that lets those inputs crash vtium, exhaust its memory, read or write memory out of bounds, read files the program in the terminal could not read, or run code is in scope. So is a reply that leaks information to the program in the terminal.

## What to include

- The vtium version or commit, and the build: minspec, or the features you enabled.
- The bytes that trigger it, as a file or a `printf` command.
- What happens, and what an attacker gains.
- The hardware and the display path (DRM/KMS or fbdev), if the flaw depends on them.
