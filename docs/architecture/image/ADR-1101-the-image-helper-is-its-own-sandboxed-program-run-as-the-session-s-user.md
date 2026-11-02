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
  - operator: aaronsb
    level: directed
    said: "PNG decoding uses a pure-Rust crate (e.g. png), linked into the helper only, with zlib through the miniz_oxide already in the tree."
    via: the maintainer's decisions in the steering session, 2026-11-01 (decision 4), relayed by the team lead in its brief for the image records; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "Shared-memory pictures (kitty t=s): vtium deletes the object as kitty's spec says, but only after the helper has read a valid picture from it, and only an object the session's user owns. The maintainer: the user may step on their own rake, up to running malicious programs; vtium relies on the kernel's own protections beyond that. Never deleting would leak shared memory and is rejected."
    via: the maintainer's decisions in the steering session, 2026-11-01 (decision 5), relayed by the team lead in its brief for the image records; the wording is the team lead's
  - operator: aaronsb
    level: directed
    said: "The helper locks itself in before decoding with both landlock (it may read only the file a request names, on kernels that have landlock, 5.13+; logged and continued without it on older kernels) and seccomp (a short allow-list of system calls; any other call answers ENOSYS instead of killing the helper, so a C library update that tries a newer call falls back to an older one; a gate test runs the real helper under its filter so a stale list fails a gate). Record that the same filters should come to the font helper too, as a follow-up."
    via: the maintainer's decisions in the steering session, 2026-11-01 (decision 6), relayed by the team lead in its brief for the image records; the wording is the team lead's
  - upstream: "kmscon ADR-103 (aaronsb/kmscon b1f7a7c5, docs/architecture/render/), The image helper and its Notes of 2026-10-24 (the pause and the helper's lifetime, the helper protocol at version 2, memory, the sd_login build option): the separate program, the identity modes, the pre-login account, the child's setup, the file and shared-memory rules, the request and reply, the host's checks, the 2-second timeout"
  - upstream: "aaronsb/kmscon b1f7a7c5: src/image_host.c, src/image_helper.c, src/image_identity.c and docs/image-helper.md"
  - evidence: "the C fork's issue that weighed a seccomp or landlock filter for the image helper, closed and moved to vtium's image helper work: the filter must allow openat2 and reads through /proc/self/fd, and any program that writes to the pty can have the helper unlink a shared-memory object the session's user owns"
  - evidence: "the C fork's issue listing the residuals of the image helper's security review, still open in the C fork: the gid-0 check in two places, a group list of at most 256, no refusal test for rpc_pipefs, and a PNG whose IEND was removed and junk appended reporting no ignored bytes"
  - evidence: ".claude/wave-c/images/gap-check.md, the gap-check of ADR-1100 against integration 5e3485e4, sections 3, 6 and 9: nothing of the helper exists, and what vtium can reuse from the glyph helper"
  - standard: "the kitty graphics protocol (sw.kovidgoyal.net/kitty/graphics-protocol), read 2026-11-01, The transmission medium: regular files only, symlinks followed, special files refused, sensitive locations such as /proc, /sys and /dev refused before opening, one identical error for every failure to read, a temporary file deleted only in a known temporary directory and only when its full path contains tty-graphics-protocol, a shared-memory name passed unmodified to shm_open and shm_unlink, with or without a leading slash, and the object read, then unlinked"
  - standard: "landlock(7) and seccomp(2), Linux man-pages: a landlock domain only ever narrows and applies at open time; a seccomp filter's SECCOMP_RET_ERRNO answers a refused call with an error and lets the process run on (external, not re-verified against the kernel source)"
  - precedent: ADR-1100
  - precedent: ADR-1200
  - precedent: ADR-1301
  - precedent: ADR-1400
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-01
deciders:
  - aaronsb
related: []
---

# ADR-1101: The image helper is its own sandboxed program, run as the session's user

## Summary

- **Decided:** The image helper is its own program, `/usr/bin/vtium-image`. It reads the files and shared-memory objects a program names, and decodes PNG, zlib and the formats iTerm2's pictures arrive in, for the console, which runs as root and does neither (ADR-1100 D7). vtium opens the program once when it starts and starts every helper from that open file, as it starts the font helper. The helper runs as the session's user: the user of the login's own logind session where logind runs, otherwise the owner of the session's pty, and never root by fallback. Before anyone logs in, file media are refused, and pictures sent inline decode as `vtium-image`, an account the package creates, or else as `nobody`. The helper starts with an empty environment, no new privileges, a memory cap and only its socket, and installs a seccomp filter, a short list of the system calls it may make, which answers any other call with ENOSYS. For each request it forks a worker, which opens what the request names under the file rules and then locks itself in with landlock, so it can open nothing more while it decodes. Requests and replies are single packets on a socket pair, and pixels come back in sealed memory files. A request not answered in 2 seconds fails and the helper is killed. Three crashes in a boot end image decoding for that boot. A shared-memory object is deleted only after a valid picture was read from it, and only when the session's user owns it. PNG decodes through the pure-Rust `png` crate and zlib through `miniz_oxide`, both linked into the helper only.
- **Trades away:** A fork for each request, which costs a fraction of a millisecond on a desktop and more on a small board. A list of system calls that must be kept right as the decoder, the Rust standard library and the C library change, guarded by a gate. Three crashes, which a hostile stream can cause on purpose, stop pictures that need the helper until the next boot. A shared-memory object that holds no valid picture stays in `/dev/shm`, which the kitty spec does not allow. A logged-in root's helper runs as root, as ADR-1400 decided, where the font helper refuses root.
- **One-way?** No. Both ends of the wire come from one build, and a running console starts only the helper it opened, so the wire changes in one release. The pre-login account's name becomes expensive to change once packages create it on machines.
- **Probes:**
  - *Not confident (crash limit):* after the image helper crashes three times, vtium stops decoding files, PNG and compressed pictures until the machine restarts, so a hostile file shown three times cannot keep retrying against the decoder. Sixel and raw kitty pictures keep working. Is a stop for the whole boot what you want, rather than until that console's session ends?
  - *Confident (a worker per request):* the helper forks a fresh worker for each picture, which locks itself to the one file that picture names, because a process locked by landlock can never open the next picture's file. Yes or no?
- **Inversion:** Between "the console reads and decodes everything itself, as root" and "every picture in a newly started, separately sandboxed program". The answer keeps one long-lived helper per console, started from the program vtium opened, and narrows each picture's work to a worker that can reach only that picture's file.

## Context

ADR-1100 D7 sends PNG, zlib, files, temporary files, shared memory and iTerm2's formats to an image helper, so the console, which runs as root, never opens a file a program names and never decodes a format whose decoder faces a program's bytes. ADR-1400 D2 carries the C reference's helper forward and left four questions to the helper's own record: whether it is a second program or vtium itself, the pre-login account's name, whether seccomp or landlock narrows it, and what to do about the removal of shared-memory objects that any program writing to the terminal can ask for. ADR-1100 D12 says the helper "needs its own record, which decides its sandbox". The maintainer decided all four on 2026-11-01.

The C reference built this helper in kmscon ADR-103's step 4b as `kmscon-image-helper`, a separate program running as the session's user, with libpng and zlib. Its open questions were a filter, which it never built, and a list of residuals from its security review. Nothing of vtium's helper exists yet. The font helper, `vtium-vitrify serve` (ADR-1200 D15), already shows the pattern: a separate program the console starts, sandboxed, with an empty environment and its own process group, checked, never trusted, and restarted a bounded number of times a boot.

## Decision

Terms used below: a *request* is what the console asks the helper for one picture (ADR-1100 D7's request type); the *session's user* is the account that logged in on the console; *file media* are the files, temporary files and shared-memory objects a kitty command names (`t=f`, `t=t` and `t=s`); *inline media* are the bytes a program sends inside the escape sequence itself.

### D1: the program, and how vtium starts it

- **Its own program.** The helper is `vtium-image`, built from a crate of its own, `crates/vtium-image-helper`, and installed as `/usr/bin/vtium-image` unless vtium was built with another path, as the font helper's path is chosen when vtium is built. The helper links the PNG decoder and the inflater; the console links neither (ADR-1100 D7). The minspec build installs no helper. The crate forbids `unsafe`, as every vtium crate but vtium-platform does, and reaches the kernel through `rustix` and the two crates named in D9.
- **Started from the program vtium opened at start.** The `image-helper` key names the program, `/usr/bin/vtium-image` by default, as the `font-helper` key names the font helper's. vtium opens the program once when it starts, checks the file it opened, and starts every image helper from that open file, through the kernel's `execveat` with an empty path. A package upgrade replaces the program by renaming a new file into place, so the open file stays the old program, and a running console keeps starting the helper it was built with. An old console therefore never talks to a new helper, and a fix to the helper reaches a console when the console starts again (the maintainer, 2026-11-01).
- **Refused unless only root can change it.** vtium refuses a program path that is not absolute, a file that is not a regular file, and a file owned by an account other than root or the account vtium runs as, or that its group or others may write, the font helper's rule. A program that is missing or refused is logged once, and every request answers as ADR-1100 D7's "Without a helper" says: file media EBADF, PNG and zlib EINVAL.

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
- **vtium not running as root.** A vtium started as an ordinary user starts a helper only when the session's user is vtium's own user, with every user and group id set to vtium's, and handles every other request as before login. A vtium not running as root that holds group 0 in any form, real, effective, saved or supplementary, handles every request as before login. One function makes that check, and it reads vtium's group list at the size `getgroups` reports, where the C reference checked in two places and treated more than 256 groups as holding group 0.
- **The pre-login account.** `vtium-image` has home `/`, no login shell and no files, and runs with its primary group only. The package installs a sysusers file, `vtium-image.conf`, from which `systemd-sysusers` creates it. A host without `systemd-sysusers` has no such account unless a package script or the administrator creates one, and the helper then runs as `nobody`, which works but shares its account with NFS root squashing and with daemons that run as `nobody`. An account that resolves to uid 0 or gid 0 is passed over for the next. vtium looks the accounts up once, the first time it needs one, and remembers the answer, missing accounts included, so a slow name service cannot stall the login prompt on every picture; an account created later takes effect when vtium starts again.
- **Before login nothing is deleted.** A temporary file or shared-memory object named at the login prompt stays where it is, since no helper may remove a file before anyone has logged in.

### D3: how the console starts the helper

vtium-platform starts the helper in the C reference's order. The console resolves the account, its primary group and its supplementary groups in its own process before it forks, since a child forked from a process with threads may call only functions that are safe after a fork. The child then:

1. starts a session of its own with no controlling terminal (`setsid`), so the helper cannot reach the terminal vtium was started from;
2. clears its signal mask;
3. puts `/dev/null` on descriptors 0, 1 and 2, its end of the socket on descriptor 3, and closes every other descriptor;
4. sets its memory cap, `RLIMIT_AS`, to three times the request's `max_bytes` plus 64 MiB, room for what it reads, an inflated copy and the output, and a core size of 0;
5. sets its supplementary groups, then its group ids, then its user ids, real, effective and saved, and reads them back, ending at once on any mismatch;
6. clears its ambient capabilities and sets no-new-privileges;
7. changes to `/` and starts the helper from the open program file with an empty environment.

The helper runs in its own process group, which vtium kills whole, and vtium holds a pidfd for it from the start, so it never signals a process it has already reaped. vtium's own `TMPDIR`, which the temporary-file rule of D5 needs, reaches the helper as an argument, so its environment stays empty, where the C reference passed `TMPDIR` as its only environment variable.

### D4: one helper per console, and a worker per request

- **Its life.** Each console has at most one helper, started at the console's first request that needs it. One request is out at a time (ADR-1100 D7). The helper exits after the idle interval ADR-1301 D2 sets with no request, and vtium stops it when the console's session ends, when the session's user changes, when a request's `max_bytes` is larger than the one its memory cap was set from, and on a failure (D8). The next request starts a new one.
- **A worker for each request.** The helper reads a request, then forks a worker that handles it whole: the worker adds a second seccomp filter that refuses forking, so it can start no process of its own whatever its account, opens what the request names under the file rules (D5), removes what must be removed, locks itself in with landlock (D9), reads, inflates or decodes, writes the reply's memory file and sends the reply, and exits. The helper waits for the worker and takes the next request. A landlock domain only ever narrows, so a process locked to one request's file could never open the next one's; a fresh worker for each request is what lets each picture's decoder reach only that picture's file.
- **A crash ends the helper.** A worker shares its helper's address layout, so a worker killed by a signal makes the helper exit, and the next request starts a new helper from the program, whose layout is chosen afresh. D8 counts these crashes.

### D5: the file rules

These are the C reference's rules, which follow the kitty spec, except where this section says otherwise.

- **Where a file may be.** The path is resolved first, following symlinks and failing on a loop, as the spec asks. A resolved path in `/proc`, `/sys`, or `/dev` other than `/dev/shm`, is refused before anything is opened, since merely opening a file can have side effects.
- **Only a regular file is opened.** The worker opens the resolved path as a path-only descriptor through `openat2` with symlinks refused, which opens nothing; `fstat` on it must show a regular file, and `fstatfs` must show no kernel pseudo-filesystem wherever it is mounted: procfs, sysfs, debugfs, tracefs, securityfs, cgroup, devpts, configfs, bpf, pstore, efivarfs, the security modules' filesystems, binfmt_misc, nsfs, anonymous inodes, usbfs and rpc_pipefs. Only then does it open the file itself, read-only, without making it a controlling terminal and without blocking, through `/proc/self/fd`, and the device and inode must match the path-only descriptor's. A FIFO or a device is therefore never opened. The path of the opened descriptor must pass the location check again, which catches a symlink swapped in after the first check.
- **How much is read.** The request's offset and most bytes (kitty's `O` and `S`) bound the read: from the offset to the lesser of the offset plus the most and the file's end, or to the end when the most is 0. An offset at or past the end, or a read shorter than the picture needs, is a failure to read. Bytes past what the picture needs are not read.
- **One answer for every failure to read.** A missing file, an unreadable one, one that is not regular, one in a refused place and one too short all answer `UNREADABLE`, which the kitty handler answers `EBADF:Failed to read image file`, as the spec requires, so a program learns nothing about files it cannot read itself. The reason goes to vtium's log at debug level.
- **Temporary files** (`t=t`). One is read only when its resolved path lies in `/tmp`, `/dev/shm` or vtium's own `TMPDIR`, and contains `tty-graphics-protocol`, as the spec asks; any other is refused and left alone. Once opened, it is removed, whether the read succeeds or not, as kitty and Ghostty remove it. A refused `t=t` command after login still goes to the helper as a discard, which removes the file under these rules and reads nothing, since the spec gives the file's removal to the terminal.
- **Shared-memory objects** (`t=s`). The name may start with one `/` or none, as the spec requires terminals to accept, and must contain no other `/`, must not be empty, `.` or `..`, and is at most 255 bytes. The worker opens it beneath `/dev/shm` through `openat2`, with no symlink followed and no escape from the directory, under the regular-file rule above, reads it under the same bounds as a file, and copies what it needs into its own memory file, so the console never maps the program's object. The worker removes the object only once it has read a valid picture from it: for raw pixels, the bytes the picture needs; for PNG, a picture that decoded; for zlib, data that inflated to what the picture needs. It removes it only when the session's user, the helper's own account, owns it (the maintainer, 2026-11-01). An object that held no valid picture stays where it is, and a refused `t=s` command sends no discard.
- **How a file is removed.** The worker removes the resolved path, never the path as given, and only the file it checked: it opens the resolved path's parent directory through `openat2` with symlinks refused, checks with `fstatat` without following a symlink that the entry still has the device and inode it read and that the helper's own account owns it, and only then unlinks the entry from that directory. A failure leaves the file in place and is logged, and the program's reply does not change. Without the owner check, a root helper, which ignores the sticky bit of `/tmp` and `/dev/shm`, could be made to delete another user's file.

### D6: decoding

- **PNG** decodes through the `png` crate, pure Rust, in the worker. The worker reads the header first: a width or height past `max-graphic-size`, or width × height × 4 bytes past the request's `max_bytes`, is refused before any pixel is allocated, as `TOO_LARGE` and `OVER_BUDGET`. It decodes to 8-bit RGBA, expanding palette, grey, 16-bit and interlaced pictures, with no gamma or color-profile correction, as the C reference did, and writes its rows straight into the reply's memory file. A stream that does not decode is `CORRUPT`.
- **zlib** inflates through `miniz_oxide`, the inflater already in vtium's tree, pure Rust, as a stream that stops at `max_bytes` (`OVER_BUDGET`). It follows the C reference's rules for how much comes out: once the picture's bytes are out the rest is ignored, and where the request asks for exactly that many bytes, one byte more is `CORRUPT` and fewer is `TOO_SHORT`.
- **What was ignored.** The reply reports how many bytes of the source went unused, as the C reference's version 2 did, and vtium logs the count at debug level, never the file's name. A PNG whose end chunk was removed and junk appended decodes and reports none ignored, which the C reference accepted and vtium accepts too.
- **iTerm2's formats** are ADR-1103's, which names the decoders and how the request asks for them.
- **Fuzzing.** The helper's request reader and each decoder path take a fuzz target, run in-process outside the memory cap, as the C reference's sanitizer runs could not be under its cap.

### D7: the wire between vtium and the helper

- **The socket.** A Unix socket pair of the sequenced-packet kind, so the kernel keeps each message whole; the helper's end is descriptor 3. A message is one packet carrying at most one descriptor. Each packet begins as each of the font helper's frames does, with a kind and the packet's length, then fixed fields, all little-endian, then a variable part. vtium receives with close-on-exec set on any descriptor that arrives, and a packet that is short, truncated, of an unknown kind, or carries a descriptor it should not is a failed reply.
- **The hello.** The helper's first packet, sent once it has set itself up, gives its wire version and the landlock version the kernel offers, 0 for none, which vtium logs once a boot when it is 0 (D9).
- **The request** carries a version, the request's id, the medium (`d`, `f`, `t` or `s`), its flags (inflate, PNG, exact, discard, and ADR-1103's any-image flag), the name's length (1 to 4,095 bytes for a named medium, 0 for inline bytes), the offset, the most bytes to read, the fewest bytes the picture needs, the largest width and height, and `max_bytes`, followed by the name. These are ADR-1100 D7's request type, field for field. Inline bytes travel in a memory file vtium writes and seals against shrinking, growing and writing before it sends it. The helper exits on a version, flag or field it does not know, and on a combination the C reference's version 2 refused, such as a discard with any other flag, which vtium reads as a failure.
- **The reply** carries a version, the status (`OK`, `UNREADABLE`, `TOO_SHORT`, `CORRUPT`, `TOO_LARGE` or `OVER_BUDGET`), the request's id, the width and height of a decoded picture, the bytes ignored, and a reason of at most 255 bytes for vtium's log. An `OK` reply other than a discard's carries one memory file, sealed against shrinking, growing and writing after the helper unmapped it: exactly width × height × 4 bytes of RGBA for a picture, or exactly the fewest bytes the picture needs otherwise.
- **The console's checks,** before it reads a pixel, in the C reference's order: a request is out; the length is exact and the reason within 255 bytes; the version, id and status are known; the ignored count is 0 on an error or discard and within the request's bounds otherwise; an `OK` reply other than a discard's carries exactly one descriptor, of a regular file; the seals hold shrinking, growing and writing, read before the size, since until the shrink seal is set the helper could cut the file under a mapping and fault the root process; the size is exact, its product checked for overflow; and the picture fits `max-graphic-size` and `max_bytes`. Then vtium maps it read-only and checks it against the request again (`Reply::checked`, ADR-1100 D7). Any failure is a failed reply.
- **One encoding.** Both packets are encoded and decoded in one module that both ends link, beside ADR-1100 D7's request and reply types in vtium-image, so the two ends cannot read the fields differently.

### D8: timeouts, failures and restarts

- **The timer.** vtium-platform holds a 2,000 ms timer on each request while the console's pty is paused (ADR-1100 D7). A reply that passes the checks ends the request and resumes the pty.
- **A failure.** The timer firing, the socket closing or failing, or a reply that fails a check ends the request: vtium kills the helper's process group through its pidfd, reaps it, hands the terminal `FAILED`, which the kitty handler answers EBADF, or for a discard answers its own refusal, and resumes the pty. The next request starts a new helper.
- **Crashes, a bounded number a boot.** A worker killed by a signal, a helper that ends or closes its socket while a request is out, and a reply that fails a check each count as a crash; a timeout does not, since a large picture on a slow board can take that long honestly. After `IMAGE_CRASHES_MAX`, 3, crashes in a boot, across every console, vtium starts no more image helpers until it starts again, logs that once, and answers every request as ADR-1100 D7's "Without a helper" says. Each helper start lays out its address space afresh, so an unbounded count would let a hostile picture retry until a guess at the layout lands, the reason the font helper bounds its restarts (ADR-1200 D15).
- **The session's end.** A console whose session ends drops its request without a reply and kills its helper, and the next session starts with none out.

### D9: the filters

- **seccomp.** After its setup and before it reads its first request, the helper installs a seccomp filter, an allow-list of the system calls it and its workers make: reading and writing its socket and descriptors, sending and receiving packets with a descriptor, making, sizing, sealing, mapping and unmapping memory files and memory, opening through `openat2` and reading through `/proc/self/fd`, the `stat` and `statfs` family, reading a link, removing a directory entry, closing, forking a worker and waiting for it, which each worker's own second filter then refuses (D4), exiting, signal masks and handlers, setting its limits, getting random bytes, the landlock calls, and the futex and scheduling calls the Rust standard library makes. Every other call is answered ENOSYS rather than killing the helper, so where a C library update tries a newer call, such as `clone3`, the library falls back to the older one it knows (the maintainer, 2026-11-01). Among the calls the list leaves out are creating a socket or connecting one, starting a program, tracing a process, signalling another process, mounting, and every `ioctl`. The list is one named constant in the helper crate for each architecture vtium builds for, `x86_64` and `aarch64`; a call made under another architecture's numbering kills the process, since the filter cannot read its number. The filter is built and installed through `seccompiler`, a pure-Rust crate.
- **landlock.** Each worker, once it holds the descriptor it will read and has removed what it must, enters a landlock domain that handles every filesystem access right the kernel's landlock version knows and grants none of them, so it opens and removes nothing more and reads only the descriptor it holds (the maintainer, 2026-11-01). A worker for inline bytes enters the domain before it reads anything. Landlock is reached through the `landlock` crate, the Linux landlock maintainers' own Rust binding. On a kernel without landlock, before 5.13 or with landlock turned off, the worker goes on without it, and vtium logs that once a boot from the helper's hello.
- **The gate.** A test starts the real helper program, built as installed, under its filter, and sends it every kind of request: inline raw bytes, inline PNG, inline zlib, a file, a temporary file, a shared-memory object, a discard and a refused path, checking each reply. A call the filter wrongly refuses fails a request, and the gate with it, so a stale list cannot pass. A second test installs the filter in a child and sees a call outside the list, such as creating an internet socket, answered ENOSYS. Both run on `x86_64` in the gates and on `aarch64` in the `arm-native` CI job.
- **The font helper.** The same two filters should come to the font helper, `vtium-vitrify serve` (ADR-1200 D15), as a follow-up: its own list of calls, and landlock limiting it to the frit's fonts once it has opened them (the maintainer, 2026-11-01).

### D10: what the C reference left open, and the gates

The C reference's security review left residuals that vtium closes here, each with a test:

- **One gid-0 check,** made in one function and sized from `getgroups` (D2).
- **rpc_pipefs is refused,** tested in tier 3's guest, where a test running as root can mount it, since mounting needs a privilege the gates do not have.
- **The pre-login refusal,** tested end to end through a terminal, its pty and the real helper.
- **The sysusers file creates `vtium-image`,** checked on tier 3's Arch Linux guest, together with the fallback to `nobody` when the account is removed.
- **A PNG without its end chunk** decodes and reports nothing ignored, accepted as in the C reference (D6).

The helper's work item gates each control ADR-1400 D2 lists with a test: the session of its own, the ids read back, the ambient capabilities cleared, the empty environment, the credentials resolved before the fork, the restart rules, and the seals checked before the size; and the gates of the helper's issue: the identity modes (refused before login, the session's user, group 0), the path refusals (a symlink to a refused place, a file that is not regular, a shared-memory name that escapes `/dev/shm`), the memory cap, the crash limit, a fuzz target for each decoder, and tier 3's kitty file-media screens.

### Passages this record changes

These change in their owners' changes; this record edits none of them.

- **ADR-1400 D2** leaves "whether the helper is a second executable or the same binary", whether seccomp or landlock narrows it, and the pre-login account's name to the image helper's issue, and lists `TMPDIR` as the helper's only environment. This record answers all four. ADR-1400's "Where vtium departs" leaves the `t=s` removal open, which D5 answers.
- **`docs/systemd.md`, the image helper's identity row,** gains the pre-login account and the sysusers file when the helper is built.
- **ADR-1200 D15** gains the filters as a follow-up (D9).

## C notes: the C reference's flow

How the C reference ran its helper, read at kmscon `b1f7a7c5`.

- **The host.** `src/image_host.c` started the helper (`start_helper`, `resolve_creds`, `exec_helper`), kept one request out (`image_host_send`, `send_request`, `data_memfd`), received and checked replies (`receive`, `map_reply`), ended a request on the timer or a failure (`timer_event`, `fail`, `stop_helper`), and checked group 0 (`gid_zero`). Its memory cap was `HELPER_AS_BASE`, 64 MiB, plus three times `max_bytes`.
- **The identity.** `src/image_identity.c` found the session's user (`image_identity_find`, `logind_uid`, `logind_running`, `login_class`), the pre-login account (`prelogin_user`, `prelogin_candidate`), and checked group 0 a second time (`own_gid_zero`).
- **The helper.** `src/image_helper.c` checked each request (`request_valid`), opened files and shared memory (`open_source`, `open_file`, `open_regular`, `open_shm`, `location_ok`, `temp_ok`, `pseudo_fs`), removed them (`delete_checked`, `discard`), read (`read_bounded`), inflated through zlib (`inflate_bounded`), decoded through libpng (`decode_png`), and wrote and sealed its output (`out_create`, `out_seal`). It unlinked a shared-memory object once opened, whatever the outcome.
- **The wire.** `struct kmscon_image_request` of 64 bytes and `struct kmscon_image_reply` of 32, in host byte order, at version 2 (ADR-103's Notes of 2026-10-24).

## Where vtium departs

From the C reference:

- **A shared-memory object is removed only after a valid picture was read from it,** where the C reference removed it once opened, whatever the outcome, and a refused `t=s` command sends no discard.
- **A shared-memory name may lack its leading `/`,** as the kitty spec requires, where the C reference refused it.
- **The helper's environment is empty,** and vtium's `TMPDIR` reaches it as an argument, where the C reference passed `TMPDIR` as its only environment variable.
- **The helper installs a seccomp filter, and each worker a landlock domain,** where the C reference had neither.
- **A worker handles each request,** where the C reference's helper handled every request in one process.
- **Crashes are bounded at three a boot,** where the C reference restarted its helper after every failure.
- **PNG and zlib decode through pure-Rust crates,** where the C reference used libpng and zlib.
- **The wire is little-endian, and its packets begin with a kind and a length,** where the C reference's was in host byte order.

From the kitty spec:

- **A shared-memory object that held no valid picture is left in place,** where the spec has the terminal read the object, then unlink it. The maintainer chose this so that a stream that names an object, such as a file `cat` shows, cannot delete an unrelated object of the user's unless it holds a picture (the maintainer, 2026-11-01).

## Consequences

### Positive

- The console, which runs as root, opens no file a program names and decodes no PNG or compressed data.
- A fault in a decoder costs one request and, past three a boot, the pictures that need the helper; it never costs the console.
- A decoder that is taken over reaches only the one file its request named, cannot open another, start a program, open a socket or signal another process, and runs as the session's user.
- An old console never talks to a new helper, so the wire can change in any release.
- The pure-Rust decoders and the filters build the same way on every architecture vtium supports.

### Negative

- Each request costs a fork, and the helper costs a start after each idle exit.
- The system-call list needs care as the decoder, the standard library and the C library change; the gate catches a stale list, and the ENOSYS answer keeps a missing call from killing the helper.
- A stream that crashes the helper three times stops file, PNG and compressed pictures for the boot.
- An invalid shared-memory object is left in `/dev/shm`, a departure from the spec.
- A kernel without landlock runs the workers without it.

### Neutral

- The identity code, which finds the session's user, serves the font helper too, which ADR-1200 D15 says runs as the session's user once this work lands; each helper keeps its own pre-login account, `vtium-image` here and `nobody` there.
- The package gains a sysusers file and a program.

## Alternatives Considered

- **vtium itself in a helper mode,** started again with a flag. Rejected (the maintainer, 2026-11-01): the console would link the PNG decoder, and the minspec binary's size budget would pay for it.
- **One process handling every request, with no worker.** Rejected: a landlock domain only narrows, so a long-lived process could not be locked to one request's file and still open the next one's.
- **A fresh helper program started for each request.** Not taken: each start costs an `exec` and the whole sandbox setup, which a client sending a picture per video frame through shared memory pays at its frame rate; a fork of an already sandboxed helper costs a fraction of that.
- **A seccomp filter that kills the helper on a call outside its list.** Rejected (the maintainer, 2026-11-01): a C library update that tries a newer call would kill every helper, where ENOSYS lets the library fall back.
- **No filter, relying on the account switch, no-new-privileges, the empty environment, the memory cap and the file rules,** as the C reference did. Rejected (the maintainer, 2026-11-01): a filter narrows what a taken-over decoder can reach beyond what the account allows.
- **Never removing shared-memory objects.** Rejected (the maintainer, 2026-11-01): every client that sends pictures through shared memory would leak them.
- **Removing a shared-memory object once opened, whatever the outcome,** as the C reference and the spec do. Rejected (the maintainer, 2026-11-01): any program writing to the terminal could then delete any object the user owns by naming it.
- **libpng and zlib,** as the C reference used. Rejected (the maintainer, 2026-11-01): pure-Rust decoders keep C out of the helper, and `miniz_oxide` is already in the tree.
- **Unlimited restarts,** as the C reference had. Rejected: each restart is another guess at the address layout for a hostile picture.
