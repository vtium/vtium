---
contract: adr/v1
kind: decision
verb: add
capability: images
basis:
  - operator: aaronsb
    level: directed
    said: "The image helper is its own program, /usr/bin/vtium-image, following the font helper's pattern (vtium-vitrify): started by vtium, sandboxed the same way (no new privileges, empty environment, memory cap, path rules) plus what images need. It runs as the session's user (so a kitty \"show this file\" request and shared-memory pictures can read the user's files), and before login as a vtium-image account the package creates through a sysusers file, else nobody. Like the font helper after the decision on another branch this session, a running console starts every helper from the binary it opened at start (exec from a descriptor), so an old console never talks to a new helper."
    via: the maintainer's decisions in the steering session, 2026-11-01 (decision 3), relayed by the team lead in its brief for the image records; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "PNG decoding uses a pure-Rust crate (e.g. png), linked into the helper only, with zlib through the miniz_oxide already in the tree."
    via: the maintainer's decisions in the steering session, 2026-11-01 (decision 4), relayed by the team lead in its brief for the image records; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "Shared-memory pictures (kitty t=s): vtium deletes the object as kitty's spec says, but only after the helper has read a valid picture from it, and only an object the session's user owns. The user's own objects are the user's to risk, up to running a malicious program; beyond that owner check, vtium relies on the kernel's own protections. Never deleting would leak shared memory and is rejected."
    via: the maintainer's decisions in the steering session, 2026-11-01 (decision 5), relayed by the team lead in its brief for the image records; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    level: directed
    said: "The helper locks itself in before decoding with both landlock (it may read only the file a request names, on kernels that have landlock, 5.13+; logged and continued without it on older kernels) and seccomp (a short allow-list of system calls; any other call answers ENOSYS instead of killing the helper, so a C library update that tries a newer call falls back to an older one; a gate test runs the real helper under its filter so a stale list fails a gate). Record that the same filters should come to the font helper too, as a follow-up."
    via: the maintainer's decisions in the steering session, 2026-11-01 (decision 6), relayed by the team lead in its brief for the image records; the wording is the team lead's
    paraphrase: true
  - upstream: "kmscon ADR-103 (aaronsb/kmscon b1f7a7c5, docs/architecture/render/), The image helper and its Notes of 2026-10-24 (the pause and the helper's lifetime, the helper protocol at version 2, memory, the sd_login build option): the separate program, the identity modes, the pre-login account, the child's setup, the file and shared-memory rules, the request and reply, the host's checks, the 2-second timeout"
  - upstream: "aaronsb/kmscon b1f7a7c5: src/image_host.c (a kmscon not running as root starting a helper only as its own user and refusing inline media otherwise, lines 128-131), src/image_helper.c (open_shm and delete_checked removing an object only when the helper's own uid owns it), src/image_identity.c and docs/image-helper.md"
  - evidence: "the C fork's issue that weighed a seccomp or landlock filter for the image helper, closed and moved to vtium's image helper work: the filter must allow openat2 and reads through /proc/self/fd, and any program that writes to the pty can have the helper unlink a shared-memory object the session's user owns"
  - evidence: "the C fork's issue listing the residuals of the image helper's security review, still open in the C fork: the gid-0 check in two places, a group list of at most 256, no refusal test for rpc_pipefs, and a PNG whose IEND was removed and junk appended reporting no ignored bytes"
  - evidence: ".claude/wave-c/images/gap-check.md, the gap-check of ADR-1100 against integration 5e3485e4, sections 3, 6 and 9: nothing of the helper exists, and what vtium can reuse from the glyph helper"
  - evidence: "seccompiler 0.5.0's TargetArch, read 2026-11-01: x86_64, aarch64 and riscv64 only, so no filter for 32-bit ARM; cargo tree of png 0.18.1, gif 0.13, zune-jpeg 0.4, landlock 0.4 and seccompiler 0.5, 2026-11-01: png inflates through fdeflate and pulls in flate2 and miniz_oxide for its encoder alone, gif decodes LZW through weezl, zune-jpeg builds on zune-core"
  - standard: "the kitty graphics protocol (sw.kovidgoyal.net/kitty/graphics-protocol), read 2026-11-01, The transmission medium: regular files only, symlinks followed, special files refused, sensitive locations such as /proc, /sys and /dev refused before opening, one identical error for every failure to read, a temporary file deleted only in a known temporary directory and only when its full path contains tty-graphics-protocol, a shared-memory name passed unmodified to shm_open and shm_unlink, with or without a leading slash, and the object read, then unlinked"
  - standard: "landlock(7), seccomp(2), pidfd_send_signal(2) and memfd_create(2), Linux man-pages: a landlock domain only ever narrows and applies at open time; a seccomp filter's SECCOMP_RET_ERRNO answers a refused call with an error and lets the process run on; PIDFD_SIGNAL_PROCESS_GROUP from Linux 6.9; MFD_NOEXEC_SEAL from Linux 6.3, refused with EINVAL before it (external, not re-verified against the kernel source)"
  - precedent: ADR-1100
  - precedent: ADR-1200
  - precedent: ADR-1301
  - precedent: ADR-1400
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Three helper crashes stop image decoding on that console until its session ends, not for the whole boot. Other consoles keep theirs."
    via: the maintainer, 2026-11-01, in the steering session
    paraphrase: true
    covers: [crash limit]
  - operator: aaronsb
    said: "The new crates are approved for the image helper only: png, landlock, seccompiler, zune-jpeg and gif, with what they pull in."
    via: the maintainer, 2026-11-01, in the steering session
    paraphrase: true
  - operator: aaronsb
    said: "Add an image-helper configuration key mirroring font-helper."
    via: the maintainer, 2026-11-01, in the steering session
    paraphrase: true
  - operator: aaronsb
    said: "The worker per request becomes a hybrid: a one-off picture gets a fresh worker; after three back-to-back requests from the same console with the same format and pixel size, the helper keeps one long-lived worker for that console's stream, landlocked with no filesystem rights and seccomp-filtered as a one-off worker is. It ends on the first request that does not match, after an idle period on the helper's existing timer, or on a crash, which counts toward the console's three-crash limit. The builder measures the fork cost per frame first."
    via: the maintainer, 2026-11-02, in the steering session
    paraphrase: true
    covers: [a worker per request]
  - operator: aaronsb
    said: "armv7: landlock alone, logged."
    via: the maintainer, 2026-11-02, in the steering session
    paraphrase: true
    covers: [armv7]
  - operator: aaronsb
    said: "The record names three ways to serve a stream of frames: a fresh worker with a fresh reply memfd; a fresh worker writing into a small per-console pool of pre-faulted reply memfds the helper keeps; and the long-lived per-console stream worker, triggered after three matching requests by count, never by time, with the same sandbox and end conditions. The builder measures all three on a real stream (timg-style PNG and raw f=32) at 1366x768 and 7680x2160; if the pool comes close to the stream worker, vtium takes the pool, which is simpler and keeps per-picture isolation; if the stream worker clearly wins, vtium builds it."
    via: the maintainer, 2026-11-02, in the steering session
    paraphrase: true
status: proposed
date: 2026-11-01
deciders:
  - aaronsb
related: []
---

# ADR-1101: The image helper is its own sandboxed program, run as the session's user

## Summary

- **Decided:** The image helper is its own program, `/usr/bin/vtium-image`. It reads the files and shared-memory objects a program names, and decodes PNG, zlib and the formats iTerm2's pictures arrive in, for the console, which runs as root and does neither (ADR-1100 D7). vtium opens the program once when it starts and starts every helper from that open file, the way it starts the font helper. The helper runs as the session's user: the user of the login's own logind session where logind runs, otherwise the owner of the session's pty, and never root by fallback. Before anyone logs in, file media are refused, and pictures sent inline decode as `vtium-image`, an account the package creates, or else as `nobody`. The helper starts with an empty environment, no new privileges, a memory cap and only its socket, and installs a seccomp filter, a short list of the system calls it may make, which answers any other call with ENOSYS. For a one-off request it forks a worker, which opens what the request names under the file rules and then locks itself in with landlock, so it can open nothing more while it decodes. When a console sends a stream of pictures of the same format and size, as a program re-sending video frames does, the helper either reuses a small pool of reply files that are already in memory, with a fresh worker for each picture, or keeps one long-lived worker for the stream, locked in and filtered in the same way; a measurement on real streams decides which. Requests and replies are single packets on a socket pair, and pixels come back in memory files, sealed against change except for the files a stream's pool reuses. A request not answered in 2 seconds fails and the helper is killed. Three crashes stop image decoding on that console until its session ends. A shared-memory object is removed only when the session's user owns it, which is the protection, and only after a valid picture was read from it. Every decoder is pure Rust and linked into the helper only.
- **Trades away:** A fork for each one-off request, which costs a fraction of a millisecond on a desktop and more on a small board. If the measurement picks the long-lived stream worker, a frame a hostile program crafts inside a stream can reach the later frames of that stream on that console. A list of system calls that must be kept right as the decoders, the Rust standard library and the C library change, guarded by a gate. Three crashes, which a hostile stream can cause on purpose, stop that console's pictures that need the helper until its session ends. A shared-memory object that holds no valid picture stays in `/dev/shm`, which the kitty spec does not allow. A logged-in root's helper runs as root, as ADR-1400 decided, where the font helper refuses root. The helper crate holds `unsafe` in one module, where every vtium crate but vtium-platform forbids it. The 32-bit ARM helper runs without seccomp.
- **One-way?** No. Both ends of the wire come from one build, and a running console starts only the helper it opened, so the wire changes in one release. The pre-login account's name becomes expensive to change once packages create it on machines.
- **Probes:**
  - *Not confident (crash limit):* after the image helper crashes three times, vtium stops decoding files, PNG and compressed pictures until the machine restarts, so a hostile file shown three times cannot keep retrying against the decoder. Sixel and raw kitty pictures keep working. Is a stop for the whole boot what you want, rather than until that console's session ends?
  - *Not confident (stream variants):* for a program re-sending video frames, the record measures three ways and lets the numbers decide: a fresh worker and fresh memory each frame; a fresh worker each frame writing into memory kept ready; or one long-lived worker per stream. If the second comes within a quarter of the third's time per frame, vtium takes the second, which keeps each picture's decoder apart from the next. Is a quarter the margin you want?
  - *Confident (a worker per request):* the helper forks a fresh worker for each picture, which locks itself to the one file that picture names, because a process locked by landlock can never open the next picture's file. Yes or no?
  - *Not confident (armv7):* `seccompiler`, the crate that builds the filter, has no 32-bit ARM target, so on the armv7 build the helper runs with landlock and the rest of its sandbox but no seccomp filter, and logs that once. The other way is a filter of vtium's own for 32-bit ARM. Is landlock alone on armv7 acceptable?
- **Inversion:** Between "the console reads and decodes everything itself, as root" and "every picture in a newly started, separately sandboxed program". The answer keeps one long-lived helper per console, started from the program vtium opened, and narrows each picture's work to a worker that can reach only that picture's file.

> Edited 2026-11-02, third round of fixes: a frame of a stream that decodes to another size ends the stream and runs again as a one-off, so only inline frames of PNG and ADR-1103's formats count toward a stream (D4); the measurement names its media for each comparison (D4); the reply carries the pooled-file flag, and a lying opacity flag can hide the pictures beneath, within that console (D7); inline raw pixels compressed with zlib carry a pixel description (D6, D7). The Summary said pixels came back "in sealed memory files".

> Edited 2026-11-02, later the same day: the maintainer and the steering session agreed that a stream's cost lies mostly in fresh memory, so D4 names three ways to serve a stream and lets a measurement choose between a pool of reply files and the long-lived worker; D7 adds the pooled files, read with `pread` and never mapped, and the pixel description, the steering session's call open to the maintainer's overrule, with the converter's fuzz target in D6. D4 had the long-lived stream worker alone.

> Edited 2026-11-02, the maintainer's answers of 2026-11-02 and the re-check (`considered`): a run of inline frames of one format and size takes one long-lived stream worker (D4); the channel to each worker is a Unix socket pair, and the helper makes each reply's memory file (D4, D9); armv7 runs with landlock alone, as the maintainer accepted (D9); `png` inflates through `fdeflate` alone (D6). D4 had one worker for each request, which reported its outcome by a channel the record did not name.

> Edited 2026-11-01, after the fresh review and the maintainer's answers (`considered`): the crash limit ends with the console's session, not the boot (D8); the parent helper, not the landlocked worker, removes a shared-memory object (D4, D5); the owner check is stated as the protection (D5); the crate holds `unsafe` in one module (D1); the helper starts through `/proc/self/fd`, as the font helper does (D1, D3); the armv7 helper runs without seccomp (D9); the decoders and the crates they pull in are named (D6). The earlier text is in git at 991986ba.

## Context

ADR-1100 D7 sends PNG, zlib, files, temporary files, shared memory and iTerm2's formats to an image helper, so the console, which runs as root, never opens a file a program names and never decodes a format whose decoder faces a program's bytes. ADR-1400 D2 carries the C reference's helper forward and left four questions to the helper's own record: whether it is a second program or vtium itself, the pre-login account's name, whether seccomp or landlock narrows it, and what to do about the removal of shared-memory objects that any program writing to the terminal can ask for. ADR-1100 D12 says the helper "needs its own record, which decides its sandbox". The maintainer decided all four on 2026-11-01.

The C reference built this helper in kmscon ADR-103's step 4b as `kmscon-image-helper`, a separate program running as the session's user, with libpng and zlib. Its open questions were a filter, which it never built, and a list of residuals from its security review. Nothing of vtium's helper exists yet. The font helper, `vtium-vitrify serve` (ADR-1200 D15), already shows the pattern: a separate program the console starts, sandboxed, with an empty environment and its own process group, checked, never trusted, and restarted a bounded number of times.

## Decision

Terms used below: a *request* is what the console asks the helper for one picture (ADR-1100 D7's request type); the *session's user* is the account that logged in on the console; *file media* are the files, temporary files and shared-memory objects a kitty command names (`t=f`, `t=t` and `t=s`); *inline media* are the bytes a program sends inside the escape sequence itself.

### D1: the program, and how vtium starts it

- **Its own program.** The helper is `vtium-image`, built from a crate of its own, `crates/vtium-image-helper`, and installed as `/usr/bin/vtium-image` unless vtium was built with another path, as the font helper's path is chosen when vtium is built. The helper links the decoders of D6; the console links none of them (ADR-1100 D7). The minspec build installs no helper.
- **`unsafe` in one module.** The helper forks a worker for each request (D4), and every Rust binding of `fork` is `unsafe`, since a forked child of a process may call only what is safe after a fork. The crate therefore forbids `unsafe` everywhere but one module, which holds the fork and the code the child runs before it returns to safe Rust, and which a reviewer of any change to it audits line by line. Nothing in the helper maps memory itself: it reads with `read` and `pread` and writes the reply with `pwrite` (D7), so the module needs no `mmap`. The rest of the crate reaches the kernel through `rustix` and the crates of D9.
- **Started from the program vtium opened at start.** The `image-helper` key names the program, `/usr/bin/vtium-image` by default, as the `font-helper` key names the font helper's (the maintainer, 2026-11-01). vtium opens the program once when it starts, checks the file it opened, and starts every image helper from that open file, through `/proc/self/fd`, the way it starts the font helper. A package upgrade replaces the program by renaming a new file into place, so the open file stays the old program, and a running console keeps starting the helper it was built with. An old console therefore never talks to a new helper, and a fix to the helper reaches a console when the console starts again (the maintainer, 2026-11-01). Both helpers start the same way so that one rule, and one requirement that `/proc` is mounted, covers both; the helper needs `/proc` for its file rules in any case (D5).
- **Refused unless only root can change it.** vtium refuses a program path that is not absolute, a file that is not a regular file, a file owned by an account other than root or the account vtium runs as, a file that its group or others may write, and a file that is setuid or setgid, the font helper's rules. A program that is missing or refused is logged once, and every request answers as ADR-1100 D7's "Without a helper" says: file media EBADF, PNG and zlib EINVAL.

### D2: whose account the helper runs as

vtium finds the session's user again before each request, and stops a running helper whose account differs, as when one user logs out and another logs in. The rules are the C reference's, which ADR-1400 D2 carried forward:

| Situation | File media | Inline PNG and zlib |
|---|---|---|
| vtium built with `logind`, logind running, and the login child in a logind session of its own whose class is a login: `user`, `user-early`, `user-light` or `user-early-light` | read as that session's user, root included | decoded as that session's user |
| logind running, and the login child in no session of its own: the login prompt, a command vtium started without a login, a session the child shares with vtium itself, or any lookup that fails | refused with EBADF, and no helper is started | decoded as `vtium-image`, else as `nobody`, else refused |
| vtium built without `logind`, or no logind running | read as the owner of the session's pty, as `login` sets it; refused when that owner is root | decoded as the pty's owner; when that owner is root, as `vtium-image`, else as `nobody`, else refused |

- **Whether logind runs.** vtium asks sd-login how many seats exist: more than none means logind runs, none means it does not, and an error counts as running, so an unknown state refuses files rather than reading them. A host without systemd but with elogind answers the same way through libelogind (`docs/systemd.md`). A build without the `logind` feature, which links neither library, always takes the pty's owner, as ADR-1400 records.
- **logind first.** logind's answer is the identity PAM authenticated. The pty's owner is file metadata that any root process in the session can change, so where the two disagree vtium logs it and uses logind's.
- **Root.** A root login that logind authenticated runs its helper as root, as ADR-1400 D2 decided and the C reference did; the font helper refuses root, and the image helper does not, so that root's own programs can show root's own files. The filters of D9 hold a root helper as they hold any other. The pty's owner being root is never enough: that is the login prompt on a host without logind.
- **vtium not running as root.** A vtium started as an ordinary user can start a helper only as itself. It starts one only when the session's user is vtium's own user, with every user and group id set to vtium's, and otherwise refuses file media and inline media alike, since it cannot start a helper as anyone else, as the C reference refused them. A vtium not running as root that holds group 0 in any form, real, effective, saved or supplementary, refuses every request the same way. One function makes that check, and it reads vtium's group list at the size `getgroups` reports, where the C reference checked in two places and treated more than 256 groups as holding group 0.
- **The pre-login account.** `vtium-image` has home `/`, no login shell and no files, and runs with its primary group only. The package installs a sysusers file, `vtium-image.conf`, from which `systemd-sysusers` creates it. A host without `systemd-sysusers` has no such account unless a package script or the administrator creates one, and the helper then runs as `nobody`, which works but shares its account with NFS root squashing and with daemons that run as `nobody`. An account that resolves to uid 0 or gid 0 is passed over for the next. vtium looks the accounts up once, the first time it needs one, and remembers the answer, missing accounts included, so a slow name service cannot stall the login prompt on every picture; an account created later takes effect when vtium starts again.
- **Before login nothing is removed.** A temporary file or shared-memory object named at the login prompt stays where it is, since no helper may remove a file before anyone has logged in.

### D3: how the console starts the helper

vtium-platform starts the helper in the C reference's order. The console resolves the account, its primary group and its supplementary groups in its own process before it forks, since a child forked from a process with threads may call only functions that are safe after a fork. The child then:

1. starts a session of its own with no controlling terminal (`setsid`), so the helper cannot reach the terminal vtium was started from;
2. clears its signal mask;
3. puts `/dev/null` on descriptors 0, 1 and 2, its end of the socket on descriptor 3, and closes every other descriptor except the open program file, which stays open, marked close-on-exec, so the start in step 7 can name it and the helper never holds it;
4. sets its memory cap, `RLIMIT_AS`, to three times the request's `max_bytes` plus 64 MiB, room for what it reads, an inflated copy and the decoded picture, and a core size of 0;
5. sets its supplementary groups, then its group ids, then its user ids, real, effective and saved, and reads them back, ending at once on any mismatch;
6. clears its ambient capabilities and sets no-new-privileges;
7. changes to `/` and starts the helper from the open program file, through `/proc/self/fd`, with an empty environment.

The helper runs in its own process group, which vtium kills whole (D8), and vtium holds a pidfd for it from the start, so it never signals a process it has already reaped. vtium's own `TMPDIR`, which the temporary-file rule of D5 needs, reaches the helper as an argument, so its environment stays empty, where the C reference passed `TMPDIR` as its only environment variable.

### D4: one helper per console, and a worker per request

- **Its life.** Each console has at most one helper, started at the console's first request that needs it. One request is out at a time (ADR-1100 D7). The helper exits after the idle interval ADR-1301 D2 sets with no request, and vtium stops it when the console's session ends, when the session's user changes, when a request's `max_bytes` is larger than the one its memory cap was set from, and on a failure (D8). The next request starts a new one.
- **How the helper talks to a worker.** Before it forks a worker, the helper makes a Unix socket pair of the sequenced-packet kind for it. For each request, the helper makes the reply's memory file itself, or takes one from a stream's pool (below), and sends the worker over the pair the request, its inline memory file where it has one, and the reply's memory file. The worker sizes a fresh reply file, writes it with `pwrite` and seals it against shrinking, growing and writing, or writes a pooled one with `pwrite` and leaves it unsealed (D7), and answers on the pair with the outcome, the width and height, the bytes ignored, the reason, and, for a shared-memory object, the device and inode it read. The helper sends vtium the reply (D7), removes a shared-memory object that held a valid picture (D5), and takes the next request.
- **A one-off worker for each request.** For a request that is not part of a stream, the helper forks a worker that handles that request alone: the worker adds a second seccomp filter that refuses forking, so it can start no process of its own whatever its account, opens what the request names under the file rules (D5), removes a temporary file that must be removed, locks itself in with landlock (D9), reads, inflates or decodes, answers, and exits. A landlock domain only ever narrows, so a process locked to one request's file could never open the next one's; a fresh worker for each such request is what lets each picture's decoder reach only that picture's file.
- **A stream of frames.** Programs that play video by re-sending a picture for each frame, as timg, ncplayer and mpv do, send pictures of one format and one size, one after another. The helper calls it a *stream* once `STREAM_AFTER`, 3, requests from the console in a row carry the same medium and flags and the same picture size (for a raw request, the same fewest bytes or the same pixel description; for PNG and ADR-1103's formats, the same width and height the earlier replies decoded). It counts requests, never time. The helper parent never parses a frame, so it cannot see a PNG's or an ADR-1103 file's size before a worker decodes it. When a frame in a stream decodes to another size, the worker reports the mismatch and writes nothing, the stream ends, and the helper runs the request again as a one-off, with a fresh worker and a fresh reply file. That is safe because only inline frames of these formats count toward a stream, and an inline frame's memory file is sealed, so the second run decodes the same bytes the first one read. A PNG frame from a file, a temporary file or shared memory never counts toward a stream, since a second run would read the file again, and a temporary file is gone once a worker has opened it. Raw frames from any medium count, since their size is known from the request before it goes out. A stream ends on the first request that does not match, when `STREAM_IDLE_MS`, starting at 1,000 ms, passes with no request on the helper's existing idle timer, or on a crash, which counts toward the console's limit of D8.
- **What a frame costs.** The fork of a worker from the small helper costs about 0.1 ms. What a fresh worker pays for each frame is mostly fresh memory: a new reply memory file of about 33 MB at 3840x2160 takes about 8,000 page faults as it is written, and the decoder rebuilds its buffers.
- **Three ways to serve a stream** (the maintainer, 2026-11-02):
  1. *A fresh worker with a fresh reply file for each frame,* as for a one-off request.
  2. *A fresh worker for each frame, writing into a reply file from a small pool.* For each stream, the helper keeps at most `REPLY_POOL_FILES`, 2, reply memory files of the stream's reply size, faulted in once when the stream starts and freed when it ends. Each frame's worker writes into a file from the pool, so each picture keeps a worker of its own, and a stream through shared memory or files is served this way as well as an inline one. D7 says how a pooled file is read and reused.
  3. *One long-lived stream worker for the console's stream.* The helper forks it when the stream starts and sends it each frame that matches. It sets itself up as a one-off worker does, with the second seccomp filter and a landlock domain that grants no filesystem rights, which it enters before it reads anything, so it can open no file at all: it serves inline frames only, and file and shared-memory frames take (1) or (2). It keeps its decoder's buffers from frame to frame and writes into the pool of (2). A frame a hostile program crafts can then reach the later frames of the same stream on that console, which the same program sends.
- **Decided by measurement.** Before any of the three is built past (1), the helper's work item measures them on real streams at 1366x768 and 7680x2160, on tier 3's guest and on the ARM board the `arm-native` CI job runs on, and this record gains the figures with the date. It compares (1) with (2) on raw 32-bit frames sent through shared memory (`t=s,f=32`). It compares (3) with (1) and (2) on inline frames, the only frames (3) serves: raw 32-bit frames compressed with zlib (`f=32,o=z`), which the helper inflates, and timg-style inline PNG frames. Inline uncompressed raw frames never reach the helper, since the console's handler decodes them itself (ADR-1100 D7). When (2)'s time per frame comes within a quarter of (3)'s at both sizes, vtium takes (2), which is simpler and keeps a worker for each picture; when (3) clearly wins, vtium builds (3) as this bullet describes it. `STREAM_AFTER`, `STREAM_IDLE_MS` and `REPLY_POOL_FILES` are named constants of the helper crate whose starting values the measurement may move.
- **A crash ends the helper.** A worker, one-off or stream, shares its helper's address layout, so a worker killed by a signal makes the helper exit, and the next request starts a new helper from the program, whose layout is chosen afresh. D8 counts these crashes.

### D5: the file rules

These are the C reference's rules, which follow the kitty spec, except where this section says otherwise.

- **Where a file may be.** The path is resolved first, following symlinks and failing on a loop, as the spec asks. A resolved path in `/proc`, `/sys`, or `/dev` other than `/dev/shm`, is refused before anything is opened, since merely opening a file can have side effects.
- **Only a regular file is opened.** The worker opens the resolved path as a path-only descriptor through `openat2` with symlinks refused, which opens nothing; `fstat` on it must show a regular file, and `fstatfs` must show no kernel pseudo-filesystem wherever it is mounted: procfs, sysfs, debugfs, tracefs, securityfs, cgroup, devpts, configfs, bpf, pstore, efivarfs, the security modules' filesystems, binfmt_misc, nsfs, anonymous inodes, usbfs and rpc_pipefs. Only then does it open the file itself, read-only, without making it a controlling terminal and without blocking, through `/proc/self/fd`, and the device and inode must match the path-only descriptor's. A FIFO or a device is therefore never opened. The path of the opened descriptor must pass the location check again, which catches a symlink swapped in after the first check.
- **How much is read.** The request's offset and most bytes (kitty's `O` and `S`) bound the read: from the offset to the lesser of the offset plus the most and the file's end, or to the end when the most is 0. An offset at or past the end, or a read shorter than the picture needs, is a failure to read. Bytes past what the picture needs are not read.
- **One answer for every failure to read.** A missing file, an unreadable one, one that is not regular, one in a refused place and one too short all answer `UNREADABLE`, which the kitty handler answers `EBADF:Failed to read image file`, as the spec requires, so a program learns nothing about files it cannot read itself. The reason goes to vtium's log at debug level.
- **Temporary files** (`t=t`). One is read only when its resolved path lies in `/tmp`, `/dev/shm` or vtium's own `TMPDIR`, and contains `tty-graphics-protocol`, as the spec asks; any other is refused and left alone. Once opened, the worker removes it before it locks itself in, whether the read succeeds or not, as kitty and Ghostty remove it. A refused `t=t` command after login still goes to the helper as a discard, which removes the file under these rules and reads nothing, since the spec gives the file's removal to the terminal.
- **Shared-memory objects** (`t=s`). The name may start with one `/` or none, as the spec requires terminals to accept, and must contain no other `/`, must not be empty, `.` or `..`, and is at most 255 bytes. The worker opens it beneath `/dev/shm` through `openat2`, with no symlink followed and no escape from the directory, under the regular-file rule above, reads it under the same bounds as a file, and copies what it needs into its own memory file, so the console never maps the program's object. The helper, which is not locked in, removes the object once the worker reports a valid picture read from it, and only when the session's user, the helper's own account, owns it (the maintainer, 2026-11-01). A valid picture is, for raw pixels, the bytes the picture needs; for PNG, a picture that decoded; for zlib, data that inflated to what the picture needs. A refused `t=s` command sends no discard.
- **What protects the user's objects.** Any program that writes to the terminal, `cat` of a hostile file among them, can name a shared-memory object. The owner check is the protection: only objects the session's user owns can go, and those are the user's to risk, up to running a malicious program; beyond that check vtium relies on the kernel's own protections (the maintainer, 2026-11-01). The C reference checked the owner too. The valid-picture rule protects little on its own: a command for raw 32-bit pixels one pixel in size (`t=s,f=32,s=1,v=1`) reads a valid picture from any object of 4 bytes or more. It keeps in place only an object a picture cannot be read from, such as one too short.
- **How a file is removed.** The worker, for a temporary file, or the helper, for a shared-memory object, removes the resolved path, never the path as given, and only the file that was checked: it opens the resolved path's parent directory through `openat2` with symlinks refused, checks with `fstatat` without following a symlink that the entry still has the device and inode that were read and that the helper's own account owns it, and only then unlinks the entry from that directory. A failure leaves the file in place and is logged, and the program's reply does not change. Without the owner check, a root helper, which ignores the sticky bit of `/tmp` and `/dev/shm`, could be made to remove another user's file.

### D6: decoding

Every crate that reads a program's bytes in the helper is pure Rust, linked into the helper only, and approved by the maintainer for the helper only, with what it pulls in (2026-11-01). Each decoding path takes a fuzz target, run in-process outside the memory cap, as the C reference's sanitizer runs could not be under its cap; the target of each path covers every crate on that path.

| Path | Crates that read the program's bytes |
|---|---|
| PNG (kitty's `f=100`, and iTerm2's pictures) | `png`, which inflates through `fdeflate` (it also pulls in `flate2` and `miniz_oxide`, which only its encoder uses) |
| zlib (kitty's `o=z`) | `miniz_oxide`, the inflater already in vtium's tree |
| JPEG (iTerm2's pictures, ADR-1103) | `zune-jpeg`, on `zune-core` |
| GIF (iTerm2's pictures, ADR-1103) | `gif`, which decodes its LZW data through `weezl` |
| Described pixels from a file or shared memory (kitty's `f=24` and `f=32` with `t=f`, `t=t` or `t=s`), raw or zlib, and sent inline with zlib (`t=d` with `o=z`) | the helper's own converter, and `miniz_oxide` for zlib |
| The request | the helper's own request reader |

- **PNG.** The worker reads the header first: a width or height past `max-graphic-size`, or width × height × 4 bytes past the request's `max_bytes`, is refused before any pixel is allocated, as `TOO_LARGE` and `OVER_BUDGET`. It decodes to 8-bit RGBA, expanding palette, grey, 16-bit and interlaced pictures, with no gamma or color-profile correction, as the C reference did. A stream that does not decode is `CORRUPT`.
- **zlib** inflates as a stream that stops at `max_bytes` (`OVER_BUDGET`). It follows the C reference's rules for how much comes out: once the picture's bytes are out the rest is ignored, and where the request asks for exactly that many bytes, one byte more is `CORRUPT` and fewer is `TOO_SHORT`.
- **JPEG and GIF** are ADR-1103's, which says how the request asks for them.
- **What was ignored.** The reply reports how many bytes of the source went unused, as the C reference's version 2 did, and vtium logs the count at debug level, never the file's name. A PNG whose end chunk was removed and junk appended decodes and reports none ignored, which the C reference accepted and vtium accepts too.

### D7: the wire between vtium and the helper

- **The socket.** A Unix socket pair of the sequenced-packet kind, so the kernel keeps each message whole; the helper's end is descriptor 3. A message is one packet carrying at most one descriptor. Each packet begins as each of the font helper's frames does, with a kind and the packet's length, then fixed fields, all little-endian, then a variable part. vtium receives with close-on-exec set on any descriptor that arrives, and a packet that is short, truncated, of an unknown kind, or carries a descriptor it should not is a failed reply.
- **The hello.** The helper's first packet, sent once it has set itself up, gives its wire version, the landlock version the kernel offers, 0 for none, and whether its seccomp filter is in force, which vtium logs once a boot when either is missing (D9).
- **The request** carries a version, the request's id, the medium (`d`, `f`, `t` or `s`), its flags (inflate, PNG, exact, discard, and ADR-1103's any-image flag), the name's length (1 to 4,095 bytes for a named medium, 0 for inline bytes), the offset, the most bytes to read, the fewest bytes the picture needs, the largest width and height, and `max_bytes`, followed by the name. A request for raw pixels that the helper reads or inflates, from a file, a shared-memory object or inline bytes compressed with zlib, may also carry a *pixel description* the handler builds from its protocol's keys (ADR-1100 D7): the layout, 3 bytes a pixel for RGB or 4 for RGBA; whether the alpha is straight or premultiplied; the width and height; and whether the bytes are zlib-compressed. The helper never sees a protocol's keys or strings. These are ADR-1100 D7's request type, field for field. Inline bytes travel in a memory file vtium writes and seals against shrinking, growing and writing before it sends it. The helper exits on a version, flag or field it does not know, and on a combination the C reference's version 2 refused, such as a discard with any other flag, which vtium reads as a failure.
- **The reply** carries a version, the status (`OK`, `UNREADABLE`, `TOO_SHORT`, `CORRUPT`, `TOO_LARGE` or `OVER_BUDGET`), the request's id, the width and height of a decoded picture, the bytes ignored, a flag that says whether the reply's memory file is a pooled one (below), and a reason of at most 255 bytes for vtium's log. An `OK` reply other than a discard's carries one memory file, written with `pwrite` and sealed against shrinking, growing and writing: exactly width × height × 4 bytes of RGBA for a decoded picture; for described pixels, exactly width × height × 4 bytes already in the store's premultiplied format, with a flag that says whether every pixel is opaque, found in the same pass that converted them; or exactly the fewest bytes the picture needs otherwise.
- **Described pixels** let the console make one pass over a picture where it would make two: the helper converts the pixels and finds their opacity as it reads them, and the console copies them into the store, clamping each color to its pixel's alpha in the same pass, so a helper that lies about the pixels gives that one picture wrong pixels and never a value out of range. A helper that lies about the opacity flag can also hide the pictures under that one: the store keeps the flag with the image (ADR-1100 D2), and the store and the compositor take a picture marked opaque to cover fully every cell it fills, so whatever lies beneath it in those cells does not show. Each console has a store of its own, so the harm stays within the console whose program sent the picture. Raw pixels sent inline and uncompressed stay with the console's handler, which decodes them as they stream in (ADR-1100 D7).
- **Pooled reply files** (D4's second and third ways) are not sealed, since a sealed file can never be written again. The helper keeps each pooled file and passes it with the reply; the console reads a pooled file only with `pread`, in pieces, into the store, and never maps it, and it checks the file's length before and after, so a helper that changes or shortens the file while the console reads it gives the console wrong pixels or a short read, which fails the reply, and never a fault or a data race in the root process. The console handles a reply before it resumes the pty and before the next request goes out, so once the next request is sent the helper may write the file again.
- **Memory files** are made with the kernel's no-execute seal (`MFD_NOEXEC_SEAL`), so no one can make one executable; a kernel older than 6.3 refuses that flag with EINVAL, and the memory file is then made without it.
- **The console's checks,** before it reads a pixel, in the C reference's order: a request is out; the length is exact and the reason within 255 bytes; the version, id and status are known; the ignored count is 0 on an error or discard and within the request's bounds otherwise; an `OK` reply other than a discard's carries exactly one descriptor, of a regular file; the seals hold shrinking, growing and writing, read before the size, since until the shrink seal is set the helper could cut the file under a mapping and fault the root process; the size is exact, its product checked for overflow; and the picture fits `max-graphic-size` and `max_bytes`. Then vtium maps a sealed file read-only, or reads a pooled one as above, and checks it against the request again (`Reply::checked`, ADR-1100 D7). The seal checks apply to sealed files; a pooled file is known by the reply's flag for it and is never mapped. Any failure is a failed reply.
- **One encoding.** Both packets are encoded and decoded in one module that both ends link, beside ADR-1100 D7's request and reply types in vtium-image, so the two ends cannot read the fields differently.

### D8: timeouts, failures and restarts

- **The timer.** vtium-platform holds a 2,000 ms timer on each request while the console's pty is paused (ADR-1100 D7). A reply that passes the checks ends the request and resumes the pty.
- **A failure.** The timer firing, the socket closing or failing, or a reply that fails a check ends the request. vtium kills the helper's process group, its worker included, before it reaps the helper, which leads the group: through the pidfd with the kernel's process-group flag where the kernel has it, from Linux 6.9, and otherwise by signalling the group's id, which cannot be reused while its leader is not yet reaped. It then reaps the helper, hands the terminal `FAILED`, which the kitty handler answers EBADF, or for a discard answers its own refusal, and resumes the pty. The next request starts a new helper.
- **Crashes, a bounded number a session.** A worker killed by a signal, a helper that ends or closes its socket while a request is out, and a reply that fails a check each count as a crash; a timeout does not, since a large picture on a slow board can take that long honestly. After `IMAGE_CRASHES_MAX`, 3, crashes on one console, vtium starts no more image helpers for that console until its session ends, logs that once, and answers that console's requests as ADR-1100 D7's "Without a helper" says; other consoles keep theirs (the maintainer, 2026-11-01). Each helper start lays out its address space afresh, so an unbounded count would let a hostile picture retry until a guess at the layout lands, the reason the font helper bounds its restarts (ADR-1200 D15).
- **The session's end.** A console whose session ends drops its request without a reply, kills its helper, and clears its crash count, and the next session starts with none out.

### D9: the filters

- **seccomp.** After its setup and before it reads its first request, the helper installs a seccomp filter, an allow-list of the system calls it and its workers make: reading and writing its socket and descriptors, `pread` and `pwrite`, sending and receiving packets with a descriptor, making, sizing and sealing memory files, the memory calls the Rust allocator makes, opening through `openat2` and reading through `/proc/self/fd`, the `stat` and `statfs` family, reading a link, removing a directory entry, closing, forking a worker and waiting for it, which each worker's own second filter then refuses (D4), exiting, signal masks and handlers, setting its limits, getting random bytes, the landlock calls, `prctl` with `PR_SET_NO_NEW_PRIVS` and no other option, `seccomp` with `SECCOMP_SET_MODE_FILTER` and no other operation, which the worker needs to add its second filter, and the futex and scheduling calls the Rust standard library makes. Every other call is answered ENOSYS rather than killing the helper, so where a C library update tries a newer call, such as `clone3`, the library falls back to the older one it knows (the maintainer, 2026-11-01). It allows creating a socket only as a Unix socket pair, `socketpair` with `AF_UNIX` and no other family, for the channel to each worker. Among the calls the list leaves out are creating any other socket or connecting one, starting a program, tracing a process, signalling another process, mounting, and every `ioctl`. The list is one named constant in the helper crate for each architecture the filter is built for; a call made under another architecture's numbering kills the process, since the filter cannot read its number. The filter is built and installed through `seccompiler`, a pure-Rust crate.
- **32-bit ARM.** vtium also builds for `armv7-unknown-linux-gnueabihf`, and `seccompiler` builds filters for `x86_64`, `aarch64` and `riscv64` only. On armv7 the helper installs no seccomp filter and runs with landlock and the rest of its sandbox, and its hello tells vtium, which logs it once a boot. The maintainer accepted landlock alone on armv7, logged (2026-11-02).
- **landlock.** Each worker, once it holds the descriptor it will read and has removed a temporary file it must remove, enters a landlock domain that handles every filesystem access right the kernel's landlock version knows and grants none of them, so it opens and removes nothing more and reads only the descriptor it holds (the maintainer, 2026-11-01). A worker for inline bytes enters the domain before it reads anything. Landlock is reached through the `landlock` crate, the Linux landlock maintainers' own Rust binding. On a kernel without landlock, before 5.13 or with landlock turned off, the worker goes on without it, and vtium logs that once a boot from the helper's hello.
- **The gate.** A test starts the real helper program, built as installed, under its filter, and sends it every kind of request: inline raw bytes, inline PNG, inline zlib, a file, a temporary file, a shared-memory object, a discard and a refused path, checking each reply. A call the filter wrongly refuses fails a request, and the gate with it, so a stale list cannot pass. A second test installs the filter in a child and sees a call outside the list, such as creating an internet socket, answered ENOSYS. Both run on `x86_64` in the gates and on `aarch64` in the `arm-native` CI job.
- **The font helper.** The same two filters should come to the font helper, `vtium-vitrify serve` (ADR-1200 D15), as a follow-up: its own list of calls, and landlock limiting it to the frit's fonts once it has opened them (the maintainer, 2026-11-01).

### D10: what the C reference left open, and the gates

The C reference's security review left residuals that vtium closes here, each with a test:

- **One gid-0 check,** made in one function and sized from `getgroups` (D2).
- **rpc_pipefs is refused,** tested in tier 3's guest, where a test running as root can mount it, since mounting needs a privilege the gates do not have.
- **The pre-login refusal,** tested end to end through a terminal, its pty and the real helper.
- **The sysusers file creates `vtium-image`,** checked on tier 3's Arch Linux guest, together with the fallback to `nobody` when the account is removed.
- **A PNG without its end chunk** decodes and reports nothing ignored, accepted as in the C reference (D6).

The helper's work item gates each control ADR-1400 D2 lists with a test: the session of its own, the ids read back, the ambient capabilities cleared, the empty environment, the credentials resolved before the fork, the restart rules, and the seals checked before the size; and the gates of the helper's issue: the identity modes (refused before login, the session's user, group 0, a vtium not running as root), the path refusals (a symlink to a refused place, a file that is not regular, a shared-memory name that escapes `/dev/shm`), the shared-memory owner check, the memory cap, the crash limit, a fuzz target for each decoding path, and tier 3's kitty file-media screens.

### Passages this record changes

These change in their owners' changes; this record edits none of them.

- **ADR-1400 D2** leaves "whether the helper is a second executable or the same binary", whether seccomp or landlock narrows it, and the pre-login account's name to the image helper's issue, and lists `TMPDIR` as the helper's only environment. This record answers all four. ADR-1400's "Where vtium departs" leaves the `t=s` removal open, which D5 answers.
- **`docs/systemd.md`, the image helper's identity row,** gains the pre-login account and the sysusers file when the helper is built.
- **ADR-1200 D15** gains the filters as a follow-up (D9).
- **The code rules,** which give `unsafe` to vtium-platform alone, gain the helper crate's one module (D1).

## C notes: the C reference's flow

How the C reference ran its helper, read at kmscon `b1f7a7c5`.

- **The host.** `src/image_host.c` started the helper (`start_helper`, `resolve_creds`, `exec_helper`), kept one request out (`image_host_send`, `send_request`, `data_memfd`), received and checked replies (`receive`, `map_reply`), ended a request on the timer or a failure (`timer_event`, `fail`, `stop_helper`), and checked group 0 (`gid_zero`). `resolve_creds` refused to start a helper as anyone but kmscon's own user when kmscon was not root (lines 128-131). Its memory cap was `HELPER_AS_BASE`, 64 MiB, plus three times `max_bytes`.
- **The identity.** `src/image_identity.c` found the session's user (`image_identity_find`, `logind_uid`, `logind_running`, `login_class`), the pre-login account (`prelogin_user`, `prelogin_candidate`), and checked group 0 a second time (`own_gid_zero`).
- **The helper.** `src/image_helper.c` checked each request (`request_valid`), opened files and shared memory (`open_source`, `open_file`, `open_regular`, `open_shm`, `location_ok`, `temp_ok`, `pseudo_fs`), removed them only when its own uid owned them (`delete_checked`, `open_shm`, `discard`), read (`read_bounded`), inflated through zlib (`inflate_bounded`), decoded through libpng (`decode_png`), and wrote and sealed its output through a mapping (`out_create`, `out_seal`). It unlinked a shared-memory object once opened, whatever the outcome.
- **The wire.** `struct kmscon_image_request` of 64 bytes and `struct kmscon_image_reply` of 32, in host byte order, at version 2 (ADR-103's Notes of 2026-10-24).

## Where vtium departs

From the C reference:

- **A shared-memory object is removed only after a valid picture was read from it,** where the C reference removed it once opened, whatever the outcome, and a refused `t=s` command sends no discard. Both remove only an object the helper's own account owns.
- **A shared-memory name may lack its leading `/`,** as the kitty spec requires, where the C reference refused it.
- **The helper's environment is empty,** and vtium's `TMPDIR` reaches it as an argument, where the C reference passed `TMPDIR` as its only environment variable.
- **The helper installs a seccomp filter, and each worker a landlock domain,** where the C reference had neither.
- **A worker handles each request, with a pool of reply files or one long-lived worker for a stream of frames, as a measurement decides,** where the C reference's helper handled every request in one process.
- **Crashes are bounded at three for each console's session,** where the C reference restarted its helper after every failure.
- **Every decoder is a pure-Rust crate,** where the C reference used libpng and zlib.
- **The wire is little-endian, and its packets begin with a kind and a length,** where the C reference's was in host byte order.
- **The reply is written with `pwrite` into a memory file made with the no-execute seal,** where the C reference wrote through a mapping.

From the kitty spec:

- **A shared-memory object that held no valid picture is left in place,** where the spec has the terminal read the object, then unlink it (the maintainer, 2026-11-01).

## Consequences

### Positive

- The console, which runs as root, opens no file a program names and decodes no PNG, JPEG, GIF or compressed data.
- A fault in a decoder costs one request and, past three on one console, that console's pictures that need the helper until its session ends; it never costs the console itself or another console.
- A decoder that is taken over reaches only the one file its request named, or, if the measurement picks a long-lived stream worker, no file and only the later frames of its stream, cannot open another, start a program, open a socket or signal another process, and runs as the session's user; on armv7 the seccomp part of that is missing.
- An old console never talks to a new helper, so the wire can change in any release.

### Negative

- Each one-off request costs a fork and fresh memory, and so does each frame of a stream unless a pool or a stream worker serves it, and the helper costs a start after each idle exit.
- The system-call list needs care as the decoders, the standard library and the C library change; the gate catches a stale list, and the ENOSYS answer keeps a missing call from killing the helper.
- A stream that crashes the helper three times stops file, PNG and compressed pictures on that console until its session ends.
- An invalid shared-memory object is left in `/dev/shm`, a departure from the spec; a valid one the user owns can be removed by any program that writes to the terminal.
- A kernel without landlock runs the workers without it, and the armv7 helper runs without seccomp.
- The helper crate holds `unsafe` in one module.

### Neutral

- The identity code, which finds the session's user, serves the font helper too, which ADR-1200 D15 says runs as the session's user once this work lands; each helper keeps its own pre-login account, `vtium-image` here and `nobody` there.
- The package gains a sysusers file and a program.

## Alternatives Considered

- **vtium itself in a helper mode,** started again with a flag. Rejected (the maintainer, 2026-11-01): the console would link the decoders, and the minspec binary's size budget would pay for them.
- **One process handling every request, with no worker.** Rejected: a landlock domain only narrows, so a long-lived process could not be locked to one request's file and still open the next one's.
- **A fresh worker and a fresh reply file for every frame of a stream, and nothing more.** Not taken alone (the maintainer, 2026-11-02): each frame pays for fresh memory, about 8,000 page faults at 3840x2160; it stays as the first of the three ways measured.
- **Starting a stream by time,** such as frames arriving faster than some rate. Rejected (the maintainer, 2026-11-02): the helper counts matching requests, so what it does follows the bytes and never how fast they came.
- **A long-lived stream worker for shared-memory frames.** Not taken: a worker that can open the next frame's object can open the user's other objects too; a fresh worker writing into the pool, the second way, serves such a stream and opens only the one object its request names.
- **A fresh helper program started for each request.** Not taken: each start costs an `exec` and the whole sandbox setup, which a client sending a picture per video frame through shared memory pays at its frame rate; a fork of an already sandboxed helper costs a fraction of that.
- **The worker removing the shared-memory object.** Not possible: the worker knows whether its picture was valid only after it has locked itself in, and a landlocked worker can remove nothing, so the helper, which is not locked in, removes it.
- **Starting the image helper through `execveat` in vtium-platform** while the font helper starts through `/proc/self/fd`. Not taken: one way for both helpers keeps one rule, and the helper needs `/proc` mounted in any case.
- **A seccomp filter that kills the helper on a call outside its list.** Rejected (the maintainer, 2026-11-01): a C library update that tries a newer call would kill every helper, where ENOSYS lets the library fall back.
- **A seccomp filter of vtium's own for 32-bit ARM.** Not taken now: it would be a second filter builder to keep right for one architecture; the probe above asks the maintainer.
- **No filter, relying on the account switch, no-new-privileges, the empty environment, the memory cap and the file rules,** as the C reference did. Rejected (the maintainer, 2026-11-01): a filter narrows what a taken-over decoder can reach beyond what the account allows.
- **Never removing shared-memory objects.** Rejected (the maintainer, 2026-11-01): every client that sends pictures through shared memory would leak them.
- **Removing a shared-memory object once opened, whatever the outcome,** as the C reference and the spec do. Not taken (the maintainer, 2026-11-01): an object a picture could not be read from stays; the owner check, which vtium keeps either way, is what protects the user's other objects.
- **libpng and zlib,** as the C reference used. Rejected (the maintainer, 2026-11-01): pure-Rust decoders keep C out of the helper, and `miniz_oxide` is already in the tree.
- **A crash limit for the whole boot,** as the font helper's. Not taken (the maintainer, 2026-11-01): one console's hostile stream would stop pictures on every console until the machine restarted.
- **Unlimited restarts,** as the C reference had. Rejected: each restart is another guess at the address layout for a hostile picture.
