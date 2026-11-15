# vtium-clipboard

`vtium-clipboard` copies text from a script or a pipe to the console's clipboard, as `pbcopy` does on macOS (ADR-1007). The install tree also links it as `vticlip`, a short name for pipelines: `git rev-parse HEAD | vticlip copy`. It is a program of its own, outside the minspec `vtium` binary and its size budget, and a package may leave it out.

## What copy does

`vtium-clipboard copy [FILE]` reads the text from FILE, or from its standard input when no FILE is named. Text larger than `CLIPBOARD_MAX`, 524,288 bytes, is refused before anything is written, because vtium-term drops a larger OSC 52 write whole. The program takes the bound from vtium-term, so the two always agree. It reads at most one byte past the bound, so an endless input still ends.

The program then opens its controlling terminal, `/dev/tty`, and writes `ESC ] 52 ; c ; BASE64 ESC \`. The sequence follows xterm's ctlseqs, OSC 52 (Manipulate Selection Data). BASE64 is the text in standard base64 with padding, as RFC 4648, section 4, defines it. The target `c` is the clipboard, which vtium takes into its one selection. The program never writes to its standard output, so a copy whose output is redirected still reaches the console. It never wraps the sequence for tmux or screen, so the multiplexer's own setting decides whether a copy goes through.

The bytes go as they are. When they are not valid UTF-8, the program still copies them and says so: vtium-term shows U+FFFD in place of each maximal subpart that is not valid UTF-8.

After the sequence, the program asks XTVERSION, `CSI > 0 q`, and then DA1, `CSI c`. Every terminal answers DA1, and a terminal answers in the order it reads, so the DA1 reply ends the wait. The program waits at most `REPLY_WAIT`, one second. While it waits, the terminal is in noncanonical mode with echo and signal keys off. The replies then never show on the screen, and Ctrl+C cannot leave the terminal in that mode. Afterwards the program puts the mode back as it was. When the wait ran out, it first discards the input left unread, which may be a reply cut off partway.

The terminal answered as vtium when its XTVERSION text names vtium, as `vtium(0.1.0)` does: the name before the first `(` or space is `vtium`.

## Exit status

| Status | What happened |
|---|---|
| 0 | The copy was written, and the terminal answered as vtium. |
| 1 | Nothing was written: the text was too large, the file could not be read, or there was no terminal. |
| 2 | The command line was wrong, or the command was `paste`. |
| 3 | The copy was written, but the terminal did not answer as vtium, as under tmux, which answers for itself. |

The program cannot see whether vtium kept the text. OSC 52 has no answer, and with `clipboard-write` off in vtium.conf, vtium ignores the copies that programs send.

`vtium-clipboard paste` prints that paste is not available yet and exits 2. Whether any program may read the clipboard back waits for a decision record of its own, which the maintainer has left open.

## Files

| Path | What it is |
|---|---|
| `src/lib.rs` | The command line, the sequence and its base64, the read of the text up to the bound, and the reading of the replies. |
| `src/tty.rs` | `/dev/tty`: the terminal's mode while the program waits, the write and the bounded wait for the replies. |
| `src/messages.rs` | Every message the program shows, each a whole message. |
| `src/main.rs` | The entry point. |
| `tests/bytes.rs` | The exact bytes for each text, written with coreutils' base64(1); the check that tier 1's scenario feeds vtium those same bytes; vtium-term taking each copy, the empty text and the largest one included, and answering as vtium. |
| `tests/pty.rs` | The program on a pty that is its controlling terminal, with vtium-term playing vtium, the same terminal named as tmux, and a terminal that never answers: exit 0, 3 and 3, the bytes that reached the terminal, and the terminal's mode put back. Also text past the bound, an unreadable file, no terminal, paste, a wrong command line, and the messages under the name `vticlip`. |

Tier 1 judges what a copy does in vtium through the replay interface's `clipboard` command: `tools/conformance/scenarios/vtium-clipboard-copy.scenario`, the capability `vtium-clipboard-copy`. The VT guide `tools/vt/checks/clipboard.guide` walks a person through a copy and a paste on a real console.

`cargo test -p vtium-clipboard` runs every test. The pty tests need `setsid` from util-linux.
