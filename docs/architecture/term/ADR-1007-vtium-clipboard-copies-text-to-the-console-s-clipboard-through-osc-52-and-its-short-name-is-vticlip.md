---
contract: adr/v1
kind: decision
verb: add
capability: protocols
basis:
  - operator: aaronsb
    level: directed
    said: An optional binary, shipped beside `vtium` and `vtium-config`, that gives scripts and users the console's clipboard from the command line … the tool base64-encodes its input and writes it as OSC 52 to its own tty … The binary is `vtium-clipboard`; the install tree also links it as `vticlip`, a short name for pipelines … Non-interactive and scriptable; exit status says whether the copy was taken. A copy outside a vtium session (no vtium answer to XTVERSION) still writes OSC 52 and says so.
    via: the body of the GitHub issue for vtium-clipboard, posted from the operator's account
  - standard: xterm ctlseqs, Operating System Commands, OSC 52 (Manipulate Selection Data) and XTVERSION; RFC 4648, section 4, base64
  - precedent: ADR-1000
  - precedent: ADR-1002
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "vtium-clipboard builds copy only for now. The paste side gets its own drafted decision for the maintainer to decide later, because paste reads possibly sensitive data back into a program."
    via: the steering session's questions on 2026-11-14, on the wave plan
    paraphrase: true
    covers: [copy only]
  - operator: aaronsb
    said: "vticlip copy exits 3 when nothing confirmed the copy, inside tmux included, as written."
    via: the steering session's questions on 2026-11-14, on the small records
    paraphrase: true
    covers: [exit status]
  - operator: aaronsb
    said: "we've already gone through this so yes"
    via: the steering session's chat on 2026-11-14, answering whether to accept wave 1's proposed records in one batch
status: accepted
date: 2026-11-14
deciders:
  - aaronsb
related: [ADR-1008]
---

# ADR-1007: vtium-clipboard copies text to the console's clipboard through OSC 52, and its short name is vticlip

## Summary

- **Decided:** vtium ships an optional program, `vtium-clipboard`, beside `vtium` and `vtium-config`, and the install tree also links it as `vticlip`. `vtium-clipboard copy` reads text from its standard input or from a file and copies the text to the console's clipboard. It does so by writing an OSC 52 sequence to its own terminal, the same sequence that tmux and vim send, which vtium already takes. Copy therefore also works over ssh, and through a multiplexer that passes OSC 52 on. The program refuses text larger than vtium's clipboard bound, and its exit status says whether it wrote the copy and whether a vtium answered. This record decides copy only. `vtium-clipboard paste` is not built: whether any program may read the clipboard is ADR-1008's decision, which the maintainer has left open.
- **Trades away:** The program cannot see whether vtium kept the text, because OSC 52 has no answer and vtium.conf can switch program copies off. A script that needs the clipboard's contents back has no command for it until ADR-1008 is decided.
- **One-way?** No. The program is small and optional, and nothing else depends on it. The two names become expensive to change once packages ship them and scripts call them.
- **Probes:**
  - *Confident (copy only):* for now `vtium-clipboard copy` is the whole program. `vtium-clipboard paste` prints that paste is not available yet and points you to Shift+Insert and the middle mouse button. Is that what you want while the paste decision waits?
  - *Not confident (exit status):* the program exits 0 when it wrote the copy and the terminal answered as vtium. It exits 3 when it wrote the copy but the terminal did not answer as vtium, as under tmux, which answers for itself. A script that checks for 0 then treats a copy through tmux as failed, though the copy may well have worked. Is that the right line?
- **Inversion:** Between "a clipboard program that talks to vtium through a channel of its own" and "no program at all, so a script prints the escape sequence itself". The answer is a small program that writes the standard sequence, so it works wherever OSC 52 reaches vtium, and checks its input against vtium's bound.

## Context

vtium keeps one selection buffer, the console's clipboard. A copy with the mouse fills it, Ctrl+click on a link fills it, and a program's OSC 52 write fills it (ADR-1000, `osc.52.write`). The middle button and Shift+Insert paste it. vtium refuses OSC 52 reads (ADR-1000, `osc.52.read`), because any program on the pty could then take the clipboard.

macOS gives scripts `pbcopy` and `pbpaste`, and X11 and Wayland desktops give them `xclip` and `wl-copy`. A Linux console has nothing of the kind. The maintainer asked for an optional program that gives scripts and users the console's clipboard from the command line (`basis`). The maintainer then decided that this wave builds copy alone, because paste reads possibly sensitive data back into a program (`considered`).

vtium-term decodes an OSC 52 write strictly as base64 and reads the result as UTF-8 text, with each ill-formed sequence replaced by U+FFFD. It holds at most `CLIPBOARD_MAX`, 524,288 bytes, of the decoded text, and drops a larger write whole, so the selection keeps what it held. It takes the targets `c`, `p`, `s` and the empty list into its one selection. The replay interface's `clipboard` command prints what the last OSC 52 write set, so tier 1 can judge the path a copy takes.

## Decision

### D1: the program and its names

- **The program** is `vtium-clipboard`, a binary of its own in the crate `crates/vtium-clipboard/`, written in Rust. It is outside the minspec `vtium` binary and outside that binary's size budget. A package may leave it out.
- **The short name.** The install tree links `vticlip` to `vtium-clipboard`, a short name for pipelines. The program behaves the same under either name, and its help and messages name the name it was called by.
- **Commands.** `vtium-clipboard copy [FILE]`, `vtium-clipboard paste`, `vtium-clipboard --help` and `vtium-clipboard --version`. The program reads no configuration file and no environment variable.

### D2: what copy does

1. **It reads the text.** With FILE it reads that file, and without FILE it reads its standard input to the end. It reads at most `CLIPBOARD_MAX` bytes and one more, so it can tell when the text is too large.
2. **It checks the size.** Text larger than `CLIPBOARD_MAX`, 524,288 bytes, is refused before anything is written, because vtium would drop it whole. The program says so and exits 1. Empty text is copied, and leaves the clipboard empty.
3. **It checks the encoding.** The program copies the bytes as they are. When they are not valid UTF-8, it still copies them, and it says on standard error that vtium shows U+FFFD in place of the bytes that are not valid UTF-8.
4. **It opens its terminal.** The program writes to its controlling terminal, `/dev/tty`, and never to its standard output, so `vticlip copy < notes.txt > log` still reaches the console. With no controlling terminal, as under cron, it says so and exits 1.
5. **It writes the sequence.** The program writes `ESC ] 52 ; c ; BASE64 ESC \`, where BASE64 is the text in standard base64 with padding (RFC 4648, section 4). The target `c` is the clipboard target that ctlseqs gives, which vtium takes into its one selection, and which multiplexers and other terminals take too. The program writes plain OSC 52 and never wraps it in a multiplexer's own passthrough sequence.
6. **It asks who answered.** After the sequence, the program writes XTVERSION's query, `CSI > 0 q`, and then DA1's, `CSI c`. It reads the replies with the terminal in noncanonical mode, and then puts the terminal's mode back as it was. Every terminal answers DA1, so the DA1 reply marks the end of the replies, and the program waits for nothing else. A terminal that answers neither within 1 second, a named constant, counts as not vtium. vtium answers in the order it reads, so a reply from vtium also shows that vtium has read the OSC 52 sequence before it.

### D3: what the program says and how it exits

| Exit status | What happened | Message on standard error |
|---|---|---|
| 0 | The copy was written, and the terminal answered XTVERSION as vtium | none |
| 1 | Nothing was written: the text was too large, the file could not be read, or there was no terminal | the reason, such as "The text is 600000 bytes, and vtium's clipboard holds at most 524288 bytes. Nothing was copied." |
| 2 | The command line was wrong, or the command was `paste` | the usage, or for `paste`: "Paste is not available yet: vtium does not let programs read the clipboard. Paste with Shift+Insert or the middle mouse button." |
| 3 | The copy was written, but the terminal did not answer as vtium | "The copy was sent, but this terminal did not answer as vtium, so the text may not have reached a clipboard." |

The program cannot see whether vtium kept the text: OSC 52 has no answer, and `clipboard-write` off in vtium.conf makes vtium ignore program copies. The help page says so.

### D4: how it is judged

- **The bytes.** A unit test of the program checks the exact bytes it writes for each of a set of inputs. The inputs are empty text, ASCII, multibyte UTF-8, bytes that are not UTF-8, and text of exactly `CLIPBOARD_MAX` bytes. A test with one byte more checks that nothing is written and the exit status is 1.
- **Tier 1.** A scenario feeds those same bytes to vtium through the replay interface and reads the result with the `clipboard` command, so tier 1 judges what a copy does in vtium. Its expected results are written from ctlseqs and vtium-term's OSC 52 rules, not from vtium's output.
- **The exit status.** A test runs the program on a pty whose other end answers as vtium, then as another terminal, then not at all, and checks 0, 3 and 3.
- **The VT tour** gains a step: `echo hello | vticlip copy`, then Shift+Insert pastes `hello`.

## Consequences

### Positive

- Scripts on the console get a `pbcopy` of their own: `git rev-parse HEAD | vticlip copy`.
- Copy travels over ssh and through multiplexers, because it is the standard sequence and vtium's own rules decide what happens to it.
- vtium gains no new escape sequence, no socket and no reply.

### Negative

- The program cannot confirm that vtium kept the text.
- A copy through tmux exits 3, though it may have worked.
- Scripts still have no paste.

### Neutral

- ADR-1000 gains a row for the program's copy, in the change that builds it.
- ADR-1008 decides paste. Whatever it decides, copy stays as this record says.

## Alternatives Considered

- **A channel of its own to vtium, such as a socket.** Not taken for copy: OSC 52 already reaches vtium, from any program, with vtium's bound and decoding. A socket would work only on the console itself and would need its own access rules.
- **Write to standard output.** Rejected: a copy whose output is redirected would never reach the terminal.
- **Wrap the sequence for tmux and screen.** Rejected: the multiplexer's own setting decides whether a program may set the outer clipboard, and a wrapped sequence would go around that setting.
- **Exit 0 whenever the copy was written.** Not taken: a script could not tell a copy on vtium from a copy sent to a terminal that may not keep it.
- **Build paste now, behind a prompt.** Not taken: the maintainer decided that paste waits for its own record, ADR-1008.
