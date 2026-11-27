# Security

## Reporting a vulnerability

Report vulnerabilities privately through GitHub's private vulnerability reporting: open the [Security tab](https://github.com/vtium/vtium/security) and choose **Report a vulnerability**. Only the maintainers can read the report. Do not report a vulnerability in a public issue, pull request or discussion.

## Scope

vtium starts as root on a virtual console. A build with the `split` feature, as the packages are, then runs as two processes. The root process holds the VT, the devices and the sessions. The drawing process runs as the account `vtium` and reads the terminal's input. A build without the feature, such as the minspec build, reads that input as root. Two inputs cross that privilege boundary:

- **The pty byte stream.** Any program running in the terminal can write any bytes to it, including programs that print untrusted files or network data.
- **The image decoders.** Sixel and kitty graphics data arrive through the same byte stream, and a kitty graphics command can name a file or shared memory object for vtium to read.

A flaw that lets those inputs crash vtium, exhaust its memory, read or write memory out of bounds, read files the program in the terminal could not read, or run code is in scope. So is a reply that leaks information to the program in the terminal. Under the split, so is a flaw that lets the drawing process gain root or make the root process act beyond the requests it serves. So is a drawing process that keeps the display or the keyboards after a switch to another VT.

## What to include

- The vtium version or commit, and the build: minspec, or the features you enabled.
- The bytes that trigger it, as a file or a `printf` command.
- What happens, and what an attacker gains.
- The hardware and the display path (DRM/KMS or fbdev), if the flaw depends on them.
