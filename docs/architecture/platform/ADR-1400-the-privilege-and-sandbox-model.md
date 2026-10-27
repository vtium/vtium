---
contract: adr/v1
kind: decision
verb: constrain
capability: platform
basis:
  - operator: aaronsb
    level: directed
    said: "The kmscon security model works, there are reasons the way it is that we probably shouldn't try to relearn."
    via: operator, in session, on the first draft of this record, relayed verbatim by the team lead
  - operator: aaronsb
    level: directed
    said: "we are not recreating c in rust but instead the logic flow"
    via: operator, in session, on the first draft of this record, relayed verbatim by the team lead
  - operator: aaronsb
    level: directed
    said: "Let's be sure of kmscon architecture first before we confidently build something novel."
    via: operator, in session, on the first draft of this record, relayed verbatim by the team lead
  - operator: aaronsb
    level: guided
    said: "Let's also make sure we look at kmscon fork security model the nobody user and systemd service etc"
    via: operator, in session, relayed verbatim by the team lead in this record's brief
  - operator: aaronsb
    level: guided
    said: "With the ability to work without them"
    via: operator, in session, the line after the one above, relayed verbatim by the team lead
  - operator: aaronsb
    level: guided
    said: "Let's focus more on software minspec than hardware itself. Sticking with 'it can be launched minimally with a plain unit script' is good enough for now."
    via: operator, in session, relayed verbatim by the team lead
  - operator: aaronsb
    level: guided
    said: "This has a side effect of avoiding building interdependencies."
    via: operator, in session, relayed verbatim by the team lead
  - operator: aaronsb
    level: guided
    said: "for the security capability progression"
    via: operator, in session, pointing to the aaronsb/kmscon fork's commits as the pattern to follow; the team lead relayed this phrase only
  - operator: aaronsb
    level: guided
    said: "Each convenience is used when present and is never required."
    via: vtium/vtium#75, comment of 2026-10-26, posted from the operator's account as the operator's direction; the wording is agent-written
  - upstream: "aaronsb/kmscon b1f7a7c5: kmscon ran as root for life, with setuid, setgroups, prctl and capset only in src/image_host.c:209-218; the login child was forked as root and ran `/bin/login -p` (src/misc/pty.c:261-269, 404-427); the units carried no hardening directive, and set `User=root` and `PAMName=kmscon` only in a libseat build (scripts/systemd/kmsconvt@.service.in, kmscon.service.in, meson.build:370-373)"
  - upstream: "kmscon ADR-103 (aaronsb/kmscon), the image helper: a separate process; its identity from logind or the pty's owner, never root by fallback; the pre-login account, then nobody; the child's setup; the file rules; and a login inside kmscon's greeter session treated as having no session of its own"
  - upstream: "aaronsb/kmscon PR #162, the helper's progression in commit order: identity first (c8f02579, 2136ddfc, 6f9b0759), the host's distrust of the helper (79d765f1, a15d49ad, 935eec64), the path rules (58f21637, 76202204, 9ef29b7e, a7d475dc), the pre-login and root cases (735d12b7, f1a7f077, 2bd8013d, 75eaa582, fb991022); and the minimal build's refusal of an option turned on without its dependency (d30292fe)"
  - standard: "systemd.exec(5), systemd.resource-control(5) and pam_systemd(8), systemd 262: what a unit's directives apply to the processes it starts (external, not re-verified here)"
  - precedent: ADR-1000
  - operator: aaronsb
    level: directed
    said: 'sd-bus behind the logind Cargo feature, plus a fallback: when logind is not running or the feature is off, vtium writes mem to /sys/power/state as root, so the minspec build suspend key works too'
    via: operator decision on vtium/vtium#21 grab-suspend, 2026-10-27, relayed by the team lead; paraphrased in the relay
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: advisor
    said: "Sound carry-forward; fix the opt-in logind default and two never-silent sentences before acceptance. D1-D3 serve the operator's intent. Drift: logind opt-in left packaged image builds on logind hosts with the weaker pty-owner identity, against the C reference's sd_login=auto. Overstated: file media before login were logged at debug only; the gid-0 rule held only when not root. Unverified: the root table's kernel conditions."
    via: skeptic subagent (claude-opus-5-5), the operator's requested second opinion, 2026-10-26; read the C reference's working tree and systemd.exec(5); kernel source unreachable
  - operator: aaronsb
    said: "1 yes 2 yes"
    via: operator, in session 2026-10-26, after reading the record and the second opinion
    covers: [as the C reference, root's images]
observable:
  - "The login child's `/proc/<pid>/fd` holds 0, 1 and 2 only, after exec"
  - "The helper's `/proc/<pid>/status` shows the session's uid and `NoNewPrivs: 1`, and at a root login prompt with no logind no helper runs as root"
  - "`ldd` on the minspec build lists no libsystemd, libelogind or libseat"
  - "On tier 3, a user logged in through vtium's unit runs sudo"
status: accepted
date: 2026-10-26
deciders:
  - aaronsb
related: []
---

# ADR-1400: The privilege and sandbox model

## Summary

- **Decided:** vtium carries the C reference's security model forward as its logic flow, not its C. The main process holds root for life, launched by a plain unit, an init script or by hand, and forks the login child as root to run `/bin/login`; nothing else drops. Image media are read and decoded in a helper process following the C reference's flow, as the session's user and never as root by fallback (#31). The units match the C reference's: getty parity and no hardening directives, since the login session would inherit them. logind and libseat are an optional layer, each a Cargo feature, and vtium runs without them as the C reference did. vtium departs only where the C reference had a listed defect or gap: a test of the login child's descriptors, the helper controls #31 does not yet list, and the shared-memory unlink consequence.
- **Trades away:** A defect in the code that reads the pty stream runs as root, as it did in the C reference. The unit uses none of systemd's sandboxing.
- **One-way?** No. A privilege split stays open (Alternatives), and nothing here forecloses it.
- **Probes:**
  - *Confident (as the C reference):* vtium runs as root and installs the same kind of unit as the C reference, and changes only where the C reference had a known gap.
  - *Not confident (root's images):* when root logs in on a vtium console on a system running logind, image files root's programs name are still read as root, as the C reference allowed, until #31 says otherwise.
- **Inversion:** Between "port the C reference's C" and "design a novel privilege model" (a root parent with an unprivileged terminal process, a sandboxed unit). The answer is neither: the C reference's logic flow, in idiomatic Rust, with fixes only where the C reference's own trackers listed a defect or gap.

## Context

The C reference ran as root for life, and SECURITY.md states the same for vtium today. Only its image helper changed identity. The fork's commits built the helper's protections in an order: who the helper runs as, then the host's distrust of the helper, then the path rules, then the pre-login and root cases. Its units carried no hardening.

vtium runs where there may be no systemd, logind, udev daemon or seat daemon, launched by a plain unit, an init script or by hand (#75). The C reference already ran that way: its minimal build linked none of them but libudev, and its default build linked sd-login only when images were built and the library was present.

## Decision

### The layers, carried forward

The C reference's layers, in its order, each optional above L0:

| Layer | Adds | Builds on | Build layer, issue | Without its convenience |
|---|---|---|---|---|
| L0, the baseline | Root for life; the VT, DRM or fbdev, and evdev opened directly; the login child holding only fds 0-2; the reply rule and every bound | nothing | minspec; #12, #18, #19, #20, #21 | Uses none, as the C reference's minimal build used none but libudev. |
| L1, the helper | Reading and decoding image media as the session's user, with the identity modes below (feature `images`) | L0 | images; #31 | No logind: the identity is the pty's owner, as in the C reference. No pre-login account: `nobody`, then refused, as in the C reference. |
| L2, logind and libseat | The helper's identity from logind, through sd-login, and `grab-suspend` through logind's `Suspend`, through sd-bus (feature `logind`); devices and VT switching through libseat (feature `libseat`) | L0, L1 | seat; #31, #34, #21 | A build without the feature, or a system where logind or the seat daemon is not running, behaves as L0 and L1, as the C reference did with logind absent and libseat off by default; `grab-suspend` writes `mem` to `/sys/power/state` as root. |

The units and the sysusers file are installed files, never a layer: a hand launch needs none of them (D3).

### Each convenience is a Cargo feature

- `images` brings the helper. No build has images without it.
- `logind` reads sessions through sd-login, from libsystemd or libelogind. It is on wherever `images` is in the default feature set, as the C reference's `sd_login=auto` linked sd-login whenever images were built and the library was present; it is used at run time only when logind is running. A build with `images` and without `logind` is an explicit choice for hosts without libsystemd or libelogind, and on a logind host it falls back to the pty's owner, which kmscon ADR-103 rejected as the first identity source because root can chown a pty; vtium lists that build as a departure. (Added 2026-10-26, from the advisor's review.)
- `logind` also calls logind's `Suspend` for `grab-suspend` through sd-bus, from libsystemd, with a 2 s bound on the answer. Where the bus has no logind and cannot start it, or the feature is off, vtium writes `mem` to `/sys/power/state` as root, from a shell it starts, so the loop still serves the VT's switch to the kernel's suspend console. That write passes logind's inhibitors and its `PrepareForSleep` signal by, so on a logind host a build without the feature suspends with no delay inhibitor held and no other session's lock-on-suspend run; a refusal from a running logind is never worked around. sd-bus from libelogind is left to an issue. (Added 2026-10-27, from the operator's decision on vtium/vtium#21.)
- `libseat` takes devices and VT switching through libseat.
- A feature turned on without the C library it binds fails the build with a message naming the library, as the C reference's minimal build refused an option without its dependency. The minspec build links none of them.

### D1: who holds root, and for how long

The main process holds root for life, as the C reference's did. It forks the login child as root, made a session leader with the pty as its controlling terminal, to run `/bin/login -p` with the environment reset; nothing else drops privilege. Root is what these need, from the C reference's code, the kernel's checks being external and not re-verified:

The kernel conditions named in this table (the tty's owner or `CAP_SYS_TTY_CONFIG`, DRM master and `CAP_SYS_ADMIN`, evdev access) are from the C reference's comments and general knowledge, not re-verified against the kernel source; the advisor's review flagged them. They are checked against the kernel before any privilege split is designed.

| Operation | Window | Under libseat |
|---|---|---|
| Starting `/bin/login` on a pty as root | each new session | still root |
| Starting the helper as the session's user | each helper start | still root |
| Opening the VT, and the VT and keyboard ioctls, which need the tty's owner or `CAP_SYS_TTY_CONFIG` | start to exit, and each VT switch | the seat does it |
| Opening DRM cards and taking DRM master, which needs `CAP_SYS_ADMIN` unless the file held master before | start, each hotplug, each VT switch | the seat does it |
| Opening evdev nodes | start, each hotplug | the seat does it |

### D2: the helper and its sandbox

The helper follows kmscon ADR-103's flow, carried into Rust by #31:

- **a separate process,** one per session, started on the first request and stopped at the session's end, a user change, a larger `max_bytes` or a failure, with the next request starting a new one;
- **the child's setup,** in the C reference's order: credentials resolved before the fork; `setsid`; the signal mask cleared; fds 0-2 on `/dev/null`, the socket on fd 3, every other descriptor closed; the memory limit; `setgroups`, `setresgid` and `setresuid`, read back; the ambient capabilities cleared, then `PR_SET_NO_NEW_PRIVS`; `chdir /`, and `execve` with `TMPDIR` as the only environment;
- **the IPC checks:** fixed-size requests and replies, sealed memfds both ways, every field checked by the host, the seals before the size, and the result mapped read-only;
- **the timeout:** 2,000 ms with input paused, after which the helper is killed and the reply is `FAILED`;
- **the file rules** of ADR-103.

Whether the helper is a second executable or the same binary is a packaging detail #31 decides. Whether seccomp or landlock narrows the helper is #31's to weigh, as the C reference left its filter to aaronsb/kmscon#153.

The identity, by the C reference's rules:

- with `logind` built and logind running, the user of the login child's own logind session of a login class; a session inherited from vtium's own greeter session does not count. When that user is root, the C reference ran the helper as root; vtium does the same, and any change is #31's;
- otherwise the owner of the session's pty;
- before login, file media are refused, and inline PNG and zlib are decoded as the pre-login account, then `nobody`, and are otherwise refused. The C reference's account was `kmscon-image`; its name in vtium is #31's.

### Never silent

These are the C reference's own behaviour:

- The helper never runs as root by fallback. Where the identity comes from the pty's owner and that owner is root, file media are refused and inline media take the pre-login path.
- File media before login are refused, logged at debug level, as in the C reference.
- A vtium not running as root whose ids or groups include gid 0 handles every request as before login. When vtium runs as root and the session is root's, the helper runs as uid 0 and gid 0, as the C reference's did (image_host.c:141-180); a change is #31's.
- Pre-login decoding with no account to run as is refused, and logged.

### D3: what the unit may harden

The C reference recorded no reason for its units' lack of hardening. The likely one: a unit's sandboxing directives set process attributes, namespaces or cgroup limits on the process systemd starts, and every process it starts inherits them, the login child and the user's shell included (systemd.exec(5), systemd 262; external, not re-verified):

- `NoNewPrivileges` stops sudo, su and every setuid program in the session;
- `CapabilityBoundingSet` caps root's own console session;
- `ProtectSystem`, `ProtectHome`, `PrivateTmp`, `PrivateDevices`, `ReadOnlyPaths` and the other mount directives give the session a namespace where root cannot change the system, the user cannot see their home, or `/tmp` is not the system's;
- `SystemCallFilter`, `RestrictNamespaces`, `MemoryDenyWriteExecute` and `LockPersonality` break containers, JITs and emulators in the session;
- cgroup directives such as `DeviceAllow` and `MemoryMax` reach the session until pam_systemd moves it to a scope of its own, and for its whole life where logind is absent.

vtium's units match the C reference's, with getty parity and no hardening directive:

- getty's `TTYPath`, `TTYReset`, `TTYVHangup`, `TTYVTDisallocate` and `UtmpIdentifier`, its ordering, `Conflicts=getty@%i` and `OnFailure=getty@%i`;
- the names tier 3 needs, `kmsconvt@.service` among them (#21);
- in a `libseat` build, `User=root` and `PAMName` with the C reference's greeter PAM file. The login child inherits that greeter session, and the helper treats it as no session of the login's own, as the C reference's did;
- an optional sysusers file creating the pre-login account, with no home, shell or files.

### What the stream reaches beyond the screen

| Channel | What it reaches | Control | Issue |
|---|---|---|---|
| Replies | the program reading the pty | the reply rule (docs/architecture.md, "Determinism") and the reply queue's bound | #12, #20 |
| Kitty file and shared-memory media | the user's files | the helper, as the session's user (D2) | #31 |
| OSC 50 font paths | the user's files | the helper, with the same identity and path rules, as kmscon ADR-103 planned | #44 |
| OSC 52 write | the clipboard | the OSC string bound; reads refused (ADR-1000) | #39 |
| setBackground | the display | ADR-1000's row: behind a configuration key, on by default | #36 |

### Where vtium departs, and where the gaps go

- **The login child's descriptors:** the C reference relied on `O_CLOEXEC` and closed nothing explicitly. A runtime test checks that the child holds only fds 0-2 after exec while the spawner holds others open (#20).
- **The helper controls #31 does not yet list** become its gates, each with a test: from D2, `setsid`, the ids read back, the ambient capabilities cleared, `TMPDIR` as the only environment, the credentials resolved before the fork, the restart rules, and the seals checked before the size.
- **`t=s` unlinks:** any program writing to the pty can make the helper unlink any shared-memory object the session's user owns, once it is read (aaronsb/kmscon#153). #31 accepts this on the record or narrows which names are unlinked, and tests its choice.
- **OSC 50 font paths** go through the helper (#44), as the C reference planned and never built.

## Consequences

### Positive

- vtium keeps a model the C reference's reviews already tested, with no new privilege code to get wrong.
- Every layer runs from a plain unit, an init script or a hand launch; systemd, logind and libseat add to it and never gate it.
- The login session is the one getty gives: sudo, root's console, the user's home and `/tmp` behave as under getty.
- The minspec build links no libsystemd and no libseat.

### Negative

- A defect in the parser, screen, renderer or input path runs as root.
- systemd's sandboxing goes unused.
- A root console login on a system without logind gets no file media, as in the C reference.

### Neutral

- SECURITY.md's scope, "vtium runs as root on a virtual console", stays true.

## Alternatives Considered

- **A privilege split in one binary:** a root parent that keeps the login spawn, the VT, DRM master and the helper's identity switch, passing descriptors to an unprivileged terminal process, as OpenSSH separates privilege. Not adopted: the operator chose the C reference's flow over building something novel before its architecture is understood. Reopened when a defect in the code that reads the stream is shown to run code as root, or when the operator asks for it.
- **Unprivileged only through a seat.** Not adopted: it needs a seat daemon, and `/bin/login` still needs root.
- **Unit sandboxing, `ProtectSystem=strict` and `NoNewPrivileges` among it.** Rejected: the login session inherits it (D3).
- **Reading logind's state under `/run/systemd` in place of sd-login.** Rejected: systemd does not document those files as an interface (external, not re-verified).
