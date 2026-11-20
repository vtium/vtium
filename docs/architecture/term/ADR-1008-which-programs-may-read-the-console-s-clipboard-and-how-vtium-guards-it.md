---
contract: adr/v1
kind: decision
verb: add
capability: protocols
basis:
  - operator: aaronsb
    level: guided
    said: 'reading the clipboard back is the open design question. OSC 52 reads stay refused (`osc.52.read` in ADR-1000: any program on the pty could take the clipboard). Candidates for the record that settles it: the tool asks vtium to paste, and vtium shows a prompt on its own console that the user confirms with a key … vtium injects the clipboard as an ordinary bracketed paste into the tool''s tty, after the same confirmation; copy only, with paste left to the paste chord.'
    via: the body of the GitHub issue for vtium-clipboard, posted from the operator's account
  - precedent: ADR-1000
  - precedent: ADR-1002
  - precedent: ADR-1400
  - precedent: ADR-1007
  - operator: aaronsb
    level: directed
    said: A paste key pastes the console's clipboard. When a program runs vtium-clipboard paste, vtium shows a prompt on the console, and the clipboard goes to the program only once the person confirms. Each of the two can be turned off, so a program that implements its own copy and paste keys is not intercepted.
    via: the maintainer's interview in the steering session, 2026-11-20, as the steering session's comment on the GitHub issue for vtium-clipboard paste records it
    paraphrase: true
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "vtium-clipboard builds copy only for now. The paste side gets its own drafted decision for the maintainer to decide later, because paste reads possibly sensitive data back into a program."
    via: the steering session's questions on 2026-11-14, on the wave plan
    paraphrase: true
  - operator: aaronsb
    said: "Paste: a paste key, plus a confirmed vtium-clipboard paste command that shows a prompt on the console; both can be turned off, so a program that implements its own copy and paste keys is not intercepted."
    via: the maintainer's interview in the steering session, 2026-11-20, round 5, as the steering session's notes and its comment on the GitHub issue for vtium-clipboard paste record it
    paraphrase: true
status: proposed
date: 2026-11-14
deciders:
  - aaronsb
related: [ADR-1007, ADR-1002]
---

# ADR-1008: Which programs may read the console's clipboard, and how vtium guards it

## Summary

- **Decided:** The maintainer decided on 2026-11-20 how paste works. A person pastes the console's clipboard with a key that vtium owns, `key-paste`, Shift+Insert by default. vtium takes the key before any program sees it and types the clipboard into the program as a bracketed paste, as the middle mouse button does. A program reads the clipboard only through `vtium-clipboard paste`. That command asks vtium through a socket that vtium offers to the console's sessions, and vtium shows a prompt on the console that the person confirms with a key. Only after the confirmation does vtium hand the clipboard to the command, which prints it. Each of the two can be turned off: an empty `key-paste` gives the key to the program, and `clipboard-read = off` makes vtium refuse every request. A program that implements its own copy and paste keys then receives its keys, and vtium intercepts nothing. OSC 52 reads stay refused, and no byte of the clipboard reaches the pty except through a person's paste.
- **Trades away:** Editors that read the clipboard through OSC 52, such as neovim's OSC 52 provider, still cannot read it, and `vtium-clipboard paste` does not work over ssh. vtium's root process gains a socket that its users' programs can reach, with its own bounds and tests. Every read waits for a key on the console, so a script that reads the clipboard needs a person at the console.
- **One-way?** No while nothing relies on the read path. Once scripts use `vtium-clipboard paste`, removing it becomes expensive, and a read, once confirmed, cannot take back the text it handed out.
- **Probes:**
  - *Not confident (the channel):* the record takes the author's earlier lean for how `vtium-clipboard paste` asks: a socket that vtium checks, so the prompt can name the program and no byte touches the pty. The other way types the clipboard into the program's input as a bracketed paste after the prompt, which works over ssh but reaches whatever program reads the pty at that moment. Is the socket what you want?
  - *Not confident (reads on by default):* `clipboard-read` is on by default, because every read still needs a key on the console. The record's earlier guards had reads off by default. Should a fresh install ask the person to turn reads on?
- **Inversion:** Between "the clipboard is write-only for programs", where only a person's paste ever reads it, and "any program may read it", as some desktops allow. vtium lets one command read it, through a channel vtium can check, and only once the person confirms on the console.

## Context

The console's clipboard is vtium's one selection buffer. A person's copy, Ctrl+click on a link, link hints, and a program's OSC 52 write fill it. A person reads it back by pasting with the middle button or the `key-paste` key, Shift+Insert by default, and vtium sends the paste to the program, filtered and bracketed (ADR-1002).

No program can read it. vtium refuses OSC 52 reads (ADR-1000, `osc.52.read`), and vtium's reply rule (`docs/architecture.md`, "No reflection") keeps any text stored from the stream out of every reply. Without that, any process that can write to the pty and read its replies could take the clipboard. Such a process can be another program in the session, or a remote host at the far end of ssh.

vtium's main process runs as root and draws the console itself (ADR-1400). A program running as the logged-in user cannot read the screen or the clipboard today, and it cannot type into vtium's keyboard, because vtium reads the keyboard itself. A confirmation that vtium shows on the console and that the person answers with a key is therefore out of any program's reach.

The maintainer decided that `vtium-clipboard` builds copy alone at first, and that paste gets this record, because paste reads possibly sensitive data back into a program (`considered`). The record first set out four options, without a decision, and the maintainer then decided on 2026-11-20 (`considered`). A program that implements its own copy and paste keys, such as an editor that binds Shift+Insert, needs vtium to leave those keys alone, which is why each read path can be turned off.

## Decision

### D1: the paste key

- **`key-paste` pastes the console's clipboard.** vtium takes the key before the program sees it, and types the clipboard into the program as a bracketed paste, filtered as every paste is (ADR-1002). The key is Shift+Insert by default, and it pastes once per press.
- **An empty `key-paste` turns the key off.** vtium then passes Shift+Insert, or whichever key the person had named, to the program like any other key, so a program with its own paste key receives it. The middle mouse button pastes as before.
- vtium-settings already has the key, and the key already works this way. This decision keeps it as it is and names it as one of the two read paths.

### D2: `vtium-clipboard paste`, confirmed on the console

- **The command asks through a socket that vtium offers to the session.** vtium reads the caller's user and process from the socket (`SO_PEERCRED`). It takes the request only from a process that runs as the session's user and has that console's pty as its controlling terminal, and refuses every other caller. A remote host cannot ask, because the socket exists only on the machine.
- **vtium shows a prompt on the console, and the person confirms with a key.** The prompt names the program that asks and says how many bytes the clipboard holds, and it never shows the text. When the person confirms, vtium sends the clipboard over the socket, and `vtium-clipboard paste` prints it on its standard output. When the person refuses, vtium answers with a refusal, and the command exits with a status of its own and a message that says the paste was refused.
- **No byte of the clipboard touches the pty,** so the reply rule and the refusal of OSC 52 reads stay whole.
- **`clipboard-read` turns the command's reads on or off.** It is a switch beside `clipboard-write`, and it is on by default, since every read waits for a key on the console. With `clipboard-read = off`, vtium shows no prompt and refuses each request at once, and `vtium-clipboard paste` says that reads are turned off and which key turns them on.
- This record's agent made two calls here, which the probes put to the maintainer: the socket as the channel, from the record's earlier lean, and reads on by default.

### D3: the guards on a confirmed read

- **The prompt draws with vtium's own notices,** above every program's text and pictures, so no program can draw over it or fake it.
- **The confirming key is not a typing key,** because typing could otherwise answer a prompt that appeared mid-word. The prompt names its key, a chord such as Ctrl+Shift+Y, and every other key refuses the read.
- **One request at a time.** A request that arrives while a prompt waits is refused at once, so a program cannot stack prompts.
- **Nothing answers by itself.** A prompt that the person does not answer refuses when the console is switched away or the session ends. No key allows a program for good.
- **The read is bounded** by `CLIPBOARD_MAX`, as every write is.

## Consequences

### Positive

- A script can read the clipboard, as `vticlip paste | jq` does, without changing what any program can learn from the terminal's byte stream.
- The prompt can name the program that asks, because vtium knows the caller's process.
- A program that implements its own copy and paste keys can have them, with the paste key or the command turned off.

### Negative

- vtium's root process gains a socket that its users' programs can reach, and its bounds and tests come with it.
- `vtium-clipboard paste` works only on the machine, not over ssh, and editors that speak OSC 52 still cannot read the clipboard.
- A read needs a person at the console, so an unattended script cannot read the clipboard.

### Neutral

- `osc.52.read` in ADR-1000 stays refused, and the reply rule gains no exception.
- `vtium-clipboard paste` is built as D2 says, after the screen lock, and ADR-1007's exit status 2 for `paste` gives way to it. vtium-settings gains `clipboard-read`.

## Alternatives Considered

The record first set these four options out for the maintainer.

- **A: no read path at all.** vtium keeps refusing every read, and a person pastes with the paste key or the middle button. Nothing new can leak, but a script cannot read the clipboard. Not taken: the maintainer wants scripts to have a confirmed paste.
- **B: OSC 52 reads, each confirmed on the console.** It is the standard sequence, so editors that read the clipboard through OSC 52 would work too, over ssh and through multiplexers. Not taken: the answer goes onto the pty, where every process that reads the pty sees it, vtium cannot tell which process or which remote host asked, and the reply rule would need an exception.
- **C: a paste into the asking program's input, confirmed on the console.** `vtium-clipboard paste` would send a request sequence of vtium's own, and vtium would type the clipboard in as a bracketed paste. The text would take a paste's path, and no reply would carry it. Not taken: the paste reaches whichever program reads the pty at that moment, which need not be the one that asked, and any program, a remote one included, could send the sequence to make vtium show a prompt.
- **D: a request through a channel that vtium can check, confirmed on the console.** Taken, as D2.
- **Reads with no confirmation, open to any program.** Not offered: the maintainer asked for a guard because paste reads possibly sensitive data.
- **A confirmation that allows a program for good.** Not offered: vtium cannot name a program across starts in a way that a hostile program cannot copy.
