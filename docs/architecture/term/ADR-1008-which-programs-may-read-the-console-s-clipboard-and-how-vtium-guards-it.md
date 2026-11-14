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
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "vtium-clipboard builds copy only for now. The paste side gets its own drafted decision for the maintainer to decide later, because paste reads possibly sensitive data back into a program."
    via: the steering session's questions on 2026-11-14, on the wave plan
    paraphrase: true
status: proposed
date: 2026-11-14
deciders:
  - aaronsb
related: [ADR-1007]
---

# ADR-1008: Which programs may read the console's clipboard, and how vtium guards it

## Summary

- **Decided:** Nothing yet. This record is open, and the maintainer decides it. It sets out four ways that a program, `vtium-clipboard paste` first among them, could read the console's clipboard, with the case for and against each and the guards each one needs. Until the maintainer decides, vtium keeps refusing every read: OSC 52 reads get no reply, and `vtium-clipboard paste` is not built (ADR-1007).
- **Trades away:** Whichever option the maintainer takes, a read path either stays closed, so scripts have no paste, or opens, so a program can learn text that the person copied, perhaps a password.
- **One-way?** Option A changes nothing. Options B, C and D become expensive once programs and scripts rely on the read path, and a read path, once used, cannot take back the text it handed out.
- **Probes:**
  - *Not confident (which option):* which of A, B, C and D do you want? The author leans to D: the program asks through a channel that vtium can check, never through the terminal's byte stream, and the person confirms on the console with a key.
  - *Not confident (a key each time):* under B, C and D, should every read need its own key press, with no way to allow a program for good?
- **Inversion:** Between "the clipboard is write-only for programs", where only a person's paste ever reads it, and "any program may read it", as some desktops allow. The options in between differ in who asks, how vtium knows who asks, and who confirms.

## Context

The console's clipboard is vtium's one selection buffer. A person's copy, Ctrl+click on a link, link hints, and a program's OSC 52 write fill it. A person reads it back by pasting with the middle button or Shift+Insert, and vtium sends the paste to the program, filtered and bracketed (ADR-1002).

No program can read it. vtium refuses OSC 52 reads (ADR-1000, `osc.52.read`), and vtium's reply rule (`docs/architecture.md`, "No reflection") keeps any text stored from the stream out of every reply. Without that, any process that can write to the pty and read its replies could take the clipboard. Such a process can be another program in the session, or a remote host at the far end of ssh.

vtium's main process runs as root and draws the console itself (ADR-1400). A program running as the logged-in user cannot read the screen or the clipboard today, and it cannot type into vtium's keyboard, because vtium reads the keyboard itself. A confirmation that vtium shows on the console and that the person answers with a key is therefore out of any program's reach.

The maintainer decided that `vtium-clipboard` builds copy alone for now, and that paste gets this record, because paste reads possibly sensitive data back into a program (`considered`).

## Options

### A: no read path at all

vtium keeps refusing every read. `vtium-clipboard paste` stays unbuilt, and a person pastes with Shift+Insert or the middle button.

- **For:** Nothing changes, and nothing new can leak. The reply rule stays whole. A person's paste stays the only read, and the person chooses where it goes.
- **Against:** A script cannot read the clipboard, so `vticlip paste | jq` cannot work. A person who wants a copied text in a file pastes it into an editor by hand.

### B: OSC 52 reads, each confirmed on the console

A program sends the standard OSC 52 query. vtium shows a prompt on the console, and when the person confirms with a key, vtium answers with the clipboard in base64 on the pty. `vtium-clipboard paste` sends that query and prints the answer.

- **For:** It is the standard sequence, so editors that read the clipboard through OSC 52, such as neovim's OSC 52 provider, work too, over ssh and through multiplexers. Several terminals offer the same "ask" policy.
- **Against:** The answer goes onto the pty, where every process that reads the pty sees it, and vtium cannot tell which process asked. A remote host at the far end of ssh can ask, and the prompt cannot say which host. The answer carries text stored from the stream, which the reply rule forbids, so the rule would need an exception. It changes `osc.52.read` from refused.

### C: a paste into the asking program's input, confirmed on the console

`vtium-clipboard paste` sends a request sequence of vtium's own. When the person confirms, vtium sends the clipboard as an ordinary bracketed paste, as Shift+Insert does, and the program reads it from its standard input.

- **For:** The text takes the path a paste already takes, filtered and bracketed, so no reply carries it and the reply rule stays whole.
- **Against:** The paste reaches whichever program reads the pty at that moment, which need not be the program that asked. vtium gains a new escape sequence that any program, a remote one included, can send to make vtium show a prompt.

### D: a request through a channel that vtium can check, confirmed on the console

`vtium-clipboard paste` connects to a socket that vtium offers to the session. vtium reads the caller's user and process from the socket. It checks that the caller runs as the session's user and has that console's pty as its controlling terminal, and then it shows the prompt. When the person confirms, vtium sends the clipboard over the socket, and the program prints it.

- **For:** No byte of the clipboard touches the pty, so the reply rule and the refusal of OSC 52 reads stay whole. vtium knows which user and which process asked, so the prompt can name the program. A remote host cannot ask, because the socket exists only on the machine.
- **Against:** vtium's root process gains a socket that its users' programs can reach, which needs its own bounds and its own tests. It does not work over ssh, and it serves `vtium-clipboard` alone, not editors that speak OSC 52.

## Guards that B, C and D share

Whichever read path the maintainer takes, these guards would hold for it. Each one is open with the option.

- **The person confirms each read** with a key on the console. The prompt draws with vtium's own notices, above every program's text and pictures, so no program can draw over it or fake it. A program cannot type the key, because vtium reads the keyboard itself.
- **The prompt says what is asked:** which program asks, where vtium can tell, and how many bytes the clipboard holds. It never shows the text itself, which would put the text on the screen.
- **The key is not a typing key.** Typing could otherwise answer a prompt that appeared mid-word. The prompt could ask for a key that the prompt names, picked at random for each prompt, or for a chord such as Ctrl+Shift+Y.
- **One request at a time.** A request that arrives while a prompt waits is refused at once, so a program cannot stack prompts.
- **Nothing answers by itself.** A prompt that the person does not answer refuses when the console is switched away or the session ends. No key allows a program for good.
- **The read is bounded** by `CLIPBOARD_MAX`, as every write is.
- **A switch in vtium.conf turns reads off,** and reads would be off by default.

## The author's lean

The author leans to D, with every shared guard. D gives scripts a paste without changing what any program can learn from the terminal's byte stream, and it lets vtium say which program asks. A is the safest choice and loses only convenience. B and C send the text where vtium cannot see who reads it. This lean is not a decision: the maintainer decides, and the record is rewritten to argue for the option the maintainer takes.

## Consequences

The consequences depend on the option. Under A, nothing changes. Under B, `osc.52.read` in ADR-1000 changes from refused, and the reply rule in `docs/architecture.md` gains an exception. Under C, vtium gains an escape sequence of its own. Under D, vtium gains a socket and the rules for who may use it. Under B, C or D, `vtium-clipboard paste` is built as the chosen option says (ADR-1007).

## Alternatives Considered

- **Reads with no confirmation, open to any program.** Not offered: the maintainer asked for a guard because paste reads possibly sensitive data.
- **A confirmation that allows a program for good.** Not offered as an option of its own: vtium cannot name a program across starts in a way that a hostile program cannot copy. The shared guards leave it out, and the second probe asks about it.
