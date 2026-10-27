---
contract: adr/v1
kind: decision
verb: constrain
capability: platform
basis:
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
  - upstream: "aaronsb/kmscon b1f7a7c5: kmscon ran as root for life, with setuid, setgroups, prctl and capset only in src/image_host.c:209-218; the login child was forked as root and ran `/bin/login -p` (src/misc/pty.c:261-269, 404-427); the units carried no hardening directive (scripts/systemd/kmsconvt@.service.in, kmscon.service.in)"
  - upstream: "kmscon ADR-103 (aaronsb/kmscon), the image helper: its identity, never root by fallback, the pre-login account then nobody, the child's setup, the file rules, OpenSSH's privilege separation as precedent, and a login inside kmscon's greeter session (`PAMName=kmscon`) getting no logind session of its own"
  - upstream: "aaronsb/kmscon PR #162, the helper's progression in commit order: identity first (c8f02579, 2136ddfc, 6f9b0759), the host's distrust of the helper (79d765f1, a15d49ad, 935eec64), the path rules (58f21637, 76202204, 9ef29b7e, a7d475dc), the pre-login and root cases (735d12b7, f1a7f077, 2bd8013d, 75eaa582, fb991022); and the minimal build's refusal of an option turned on without its dependency (d30292fe)"
  - standard: "systemd.exec(5), systemd.resource-control(5) and pam_systemd(8), systemd 262: what a unit's directives apply to the processes it starts (external, not re-verified here)"
  - standard: "Linux: the VT ioctls require the caller's controlling tty or CAP_SYS_TTY_CONFIG; DRM master requires CAP_SYS_ADMIN unless the opener's process takes back a master its file held (external, not re-verified here)"
  - precedent: ADR-1000
agent:
  name: Claude
  model: claude-opus-5-5
observable:
  - "`ps -o user,comm` shows the parent as root and the terminal process as `vtium` or `nobody`; the terminal process's `/proc/<pid>/status` shows `NoNewPrivs: 1` and `CapEff: 0000000000000000`"
  - "With neither account present, every start logs the warning that vtium runs as root"
  - "The login child's `/proc/<pid>/fd` holds 0, 1 and 2 only, after exec"
  - "The helper's `/proc/<pid>/status` shows the session's uid, `NoNewPrivs: 1` and `Seccomp: 2`"
  - "On tier 3, a user logged in through vtium's unit runs sudo"
status: proposed
date: 2026-10-26
deciders:
  - aaronsb
related: []
---

# ADR-1400: The privilege and sandbox model

## Summary

- **Decided:** vtium reaches least privilege in five levels; each falls back to the one below when the convenience it uses, a system account, systemd, logind or a seat daemon, is absent. L0 is the floor: launched as root by a plain unit, an init script or by hand, devices opened directly. L1 splits vtium into a root parent that never reads the stream and a terminal process that drops to an unprivileged account in-process, asking the parent for the few operations that need root (D1). L2 reads and decodes image media in a helper that re-executes vtium's own program as the session's user, sandboxed in-process (D2). L3 adds unit directives that change nothing the login session inherits (D3). L4 uses logind and libseat where present. A helper as root and a file read as root are refused; a terminal process left as root for want of an account says so at every start.
- **Trades away:** A second long-lived process, and a request channel the root parent must distrust. The systemd unit uses none of systemd's sandboxing. A machine with no account to drop to runs as root, as the C reference did. The helper maps all of vtium's code to run only the decoder.
- **One-way?** No. Reversible now; expensive once L1's request set is built and packages create the accounts.
- **Probes:**
  - *Confident (root fallback):* on a machine with no spare account for vtium to switch to, vtium still starts, as root like the C reference, and says so in its log at every start.
  - *Not confident (the systemd unit):* vtium's systemd unit adds no sandboxing, because the shell a user logs into would inherit it and sudo would stop working; vtium sandboxes its own processes instead.
- **Inversion:** Between "one root process for life" (the C reference) and "nothing runs as root" (a terminal under a seat daemon, with no login). The answer is a small root parent that reads only fixed requests, everything that reads the stream unprivileged, and each convenience taken only where it is present.

## Context

The C reference ran as root for life: the parser, the screen, the renderer and the input path read, as root, every byte any program wrote to the pty, and SECURITY.md states the same for vtium today. Only its image helper changed identity. Its units carried no hardening, and no issue in either tracker decided the main process's privilege.

The fork's commits built the helper's protections in an order: who the helper runs as, then the host's distrust of the helper, then the path rules, then the pre-login and root cases. The levels below follow it: who each process runs as, then the parent's distrust of what it is asked, then the helper's confinement, then the root cases.

vtium runs where there may be no systemd, logind, udev daemon or seat daemon, launched by a plain unit, an init script or by hand (#75). Every protection therefore works from the kernel's interfaces alone, and each convenience adds to a floor that never needs it.

The levels are steps of privilege, apart from ADR-1000's build layers; each level names the layer it lands in.

## Decision

### The levels

```text
parent              root for life; reads no stream bytes
├── terminal        vtium or nobody; no capabilities; no_new_privs
│                   the parser, screen, renderer, input, pty master, image store
├── login child     root, then the user /bin/login authenticates; fds 0-2 only
└── image helper    the session's user; landlock and seccomp (feature `images`)
```

| Level | Adds | Builds on | Layer, issue | Without its convenience |
|---|---|---|---|---|
| L0 | Root for life; the VT, DRM or fbdev, and evdev opened directly; the login child holding only fds 0-2; the reply rule and every bound | nothing | minspec; #12, #18, #19, #20, #21 | Uses none. Started without root, and without L4's seat, vtium exits with an error naming the operation the kernel refused. |
| L1 | The split and the drop (D1) | L0 | minspec; an issue to file | With no account to drop to, vtium stays at L0 and logs a warning at every start, with no setting to turn it off. |
| L2 | The image helper, its identity rules, landlock and seccomp (D2) | L0; started by the parent where L1 runs | images; #31 | A kernel without landlock or seccomp: the helper runs without that one, logged once. No pre-login account: inline media before login are refused, logged once. No usable session identity: file media are refused, logged at debug level for each request. |
| L3 | Unit files with getty's session-neutral directives and `OnFailure=getty@%i`; a sysusers file creating `vtium` and `vtium-image` (D3) | L0 | minspec; #21, #22 | Without systemd, an init script or a hand launch runs L0 to L2 unchanged, and the accounts come from the package or the administrator. |
| L4 | The helper's identity from logind, through sd-login; devices and VT switching through libseat | L1, L2 | seat; #31, #34 | A build without the feature, or a system where logind or the seat daemon is not running, behaves as L3: identity from the pty's owner, devices opened by the parent. Logged once. |

### Each convenience is a Cargo feature

- `images` brings L2. No build has images without the helper.
- `logind` reads sessions through sd-login, from libsystemd or libelogind, and turns on `images`, the only reader of the identity.
- `libseat` takes devices and VT switching through libseat.
- A feature turned on without the C library it binds fails the build with a message naming the library, as the C reference's minimal build refused an option without its dependency. The minspec build links none of them.
- The unit and sysusers files are installed files, never features.
- landlock and seccomp are kernel interfaces with no library, found at run time.

### D1: who holds root, and for how long

The options:

- **(a) Root for life,** as the C reference ran. It is L0, the floor and the fallback. As the target it would leave every defect in the code that reads the stream running as root.
- **(b) A privilege split in one binary:** a root parent and an unprivileged terminal process, as OpenSSH separates privilege. Chosen.
- **(c) Unprivileged under libseat.** `/bin/login` must start as root, so with a login this reduces to (b) with fewer root operations; only a vtium running a command in place of login needs no root at all. It is L4's form of (b).

vtium starts as root. The first process reads the configuration, opens the files it names, and forks the terminal process; it stays root as the parent and serves the terminal process's requests until exit.

The terminal process drops before anything else, in this order:

1. closes every descriptor but its socket to the parent;
2. `setgroups` to none, then `setresgid` and `setresuid` to the account, read back with `getresgid` and `getresuid`; a mismatch ends it;
3. clears the ambient capabilities, then sets `PR_SET_NO_NEW_PRIVS` and a parent-death signal.

The account is `vtium`, then `nobody`. A candidate whose lookup fails, or that resolves to uid 0 or gid 0, is passed over. The terminal process never shares the helper's pre-login account, so a decoder compromised at the login prompt cannot signal the process that holds the display.

Every root operation, with its window:

| Root operation | Started by | Window |
|---|---|---|
| Reading the configuration and opening the files it names: fonts, a background image (#63) | the parent | start, before the fork |
| Opening the VT; setting `VT_PROCESS`, `KD_GRAPHICS` and the keyboard mode; restoring them | the parent | start, and exit, the terminal process's death included |
| Acknowledging a VT release or acquire (`VT_RELDISP`) | the parent, on the kernel's signal, once the terminal process has stopped or resumed | each VT switch |
| Opening a DRM card or an evdev node and passing its descriptor | the terminal process | start, each hotplug, each VT return |
| Setting and dropping DRM master | the parent around a VT switch; the terminal process for a display wake (#36) | each switch or wake |
| Opening a pty and starting the login child on it as root | the terminal process | start, and each new session within the respawn limit (#21) |
| Finding the session's user and starting the helper as that user | the terminal process | each helper start (L2) |
| Stopping a helper, which runs as another user | the terminal process, on a timeout or failure | each stop |

A request is a fixed-size message, and the parent checks every field. No request names a command, an identity or a path:

- a device is checked by the device number of the file the parent opened, never by its path, and only DRM, input event and VT devices are passed;
- the login child's command and environment come from the configuration;
- the helper's identity comes from the session the parent started;
- the parent holds the respawn limit, so a compromised terminal process cannot start root logins in a loop.

The parent reaps the login child and the helper. When the terminal process dies, the parent restores the VT and exits with a failure, leaving the restart to the unit or the init script.

Where no account is found, vtium runs as L0: one root process does every operation in the table itself.

landlock and seccomp for the terminal process follow as a second step of L1's issue, once the minspec build shows its syscall set.

### D2: the helper's form and sandbox

The helper is a re-execution of vtium's own executable, through a descriptor the parent opens on itself at start, with a mode only the helper's entry point reads. Against a second executable:

- one installed file, with no helper path to find or keep root-owned, and no second copy of the standard library and shared code in the installed size;
- no version skew: a package upgrade under a running vtium cannot pair it with a newer helper, since the descriptor holds the running program's file;
- the cost: the helper maps code it never runs, the parser, renderer and platform FFI among it. The sandbox bounds what that code could do.

The parent decides the identity, by the C reference's rules, which #31 carries:

- with `logind` built and logind running, the user of the login child's own logind session of a login class;
- otherwise the owner of the session's pty;
- before login, or where the identity would be root: file media are refused, and inline PNG and zlib are decoded as `vtium-image`, then `kmscon-image`, then `nobody`, and are otherwise refused. `kmscon-image` is the C reference's account, used where its package created it.

The helper confines itself after `execve` and before its first request:

- **landlock:** reads as the user's permissions allow, and removal only beneath `/tmp`, `/dev/shm` and its `TMPDIR`, for the `t=t` and `t=s` deletes; no writes and no execution.
- **seccomp:** an allow-list of the syscalls the decoder and the file rules use, `openat2` and reads through `/proc/self/fd` among them. Any other syscall kills the helper, and the terminal process answers `FAILED`.

#31 sets both lists and tests them.

### Never silent

- A helper never runs with uid 0, or with gid 0 in any id or group: the request is refused. A root login that logind authenticated gets inline images only, where the C reference ran its helper as root; vtium lists this as a departure.
- A file or shared-memory object the stream names is never read as root: the request is refused.
- A terminal process left as root because no account was found logs a warning at every start, naming the accounts tried. No setting turns it off.
- A protection the kernel lacks, and a pre-login decode with no account to run as, are logged once.

### D3: what the unit may harden

A unit's sandboxing directives set process attributes, namespaces or cgroup limits on the process systemd starts, and every process it starts inherits them, the login child and the user's shell included (systemd.exec(5), systemd 262; external, not re-verified):

- `NoNewPrivileges` stops sudo, su and every setuid program in the session;
- `CapabilityBoundingSet` caps root's own console session;
- `ProtectSystem`, `ProtectHome`, `PrivateTmp`, `PrivateDevices`, `ReadOnlyPaths` and the other mount directives give the session a namespace where root cannot change the system, the user cannot see their home, or `/tmp` is not the system's;
- `SystemCallFilter`, `RestrictNamespaces`, `MemoryDenyWriteExecute` and `LockPersonality` break containers, JITs and emulators in the session;
- cgroup directives such as `DeviceAllow`, `IPAddressDeny` and `MemoryMax` reach the session until pam_systemd moves it to a scope of its own, and for its whole life where logind is absent;
- `PAMName` puts vtium in a greeter session the login child inherits, so the login on the pty gets no logind session of its own (pam_systemd(8); kmscon ADR-103).

vtium's units therefore carry only directives that change nothing the session inherits:

- getty's `TTYPath`, `TTYReset`, `TTYVHangup` and `TTYVTDisallocate`, which hang up and clear the VT between sessions, and `UtmpIdentifier`;
- getty's ordering, and `Conflicts=getty@%i`;
- `OnFailure=getty@%i`, so a vtium that fails leaves a getty login on its VT.

vtium's hardening is its own, in-process: the terminal process (D1) and the helper (D2) confine themselves, and the login child descends from the parent, which nothing confines. A cgroup directive may join the unit at L4, once a test shows the login session in a scope of its own.

The unit's name follows tier 3: `kmsconvt@.service` (#21). The sysusers file is optional, and creates `vtium` and `vtium-image` with no home, shell or files.

### What the stream reaches beyond the screen

| Channel | What it reaches | Control | Issue |
|---|---|---|---|
| Replies | the program reading the pty | the reply rule (docs/architecture.md, "Determinism") and the reply queue's bound | #12, #20 |
| Kitty file and shared-memory media | the user's files | the helper, as the session's user (D2) | #31 |
| OSC 50 font paths | the user's files | the helper, with the same identity and file rules | #44 |
| OSC 52 write | the clipboard | the OSC string bound; reads refused (ADR-1000) | #39 |
| setBackground | the display, released to whoever takes DRM master next | the configuration key, on by default (ADR-1000) | #36 |

### Gaps, and where they go

- **setBackground:** with the key on, any program writing to the pty can hand the display back. #36 adds a hostile-stream case, in which the console returns on setForeground or a VT switch, and its entry in docs/architecture.md states the exposure.
- **OSC 50 font paths** are opened only by the helper, with the session's identity and file rules, and are refused before login and as root (#44).
- **The helper controls #31 does not yet list,** each a gate with a test: `setsid`; the ids read back after the switch; the ambient capabilities cleared; `TMPDIR` as the only environment; the credentials resolved before the fork; one helper per session, started at the first request and stopped at the session's end, a user change, a larger `max_bytes` or a failure, with the next request starting a new one; and the memfd's seals checked before its size.
- **The login child's descriptors:** a runtime test checks that the child holds only fds 0-2 after exec while the spawner holds others open (#20).
- **`t=s` unlinks:** any program writing to the pty can make the helper unlink any shared-memory object the session's user owns, once it is read (aaronsb/kmscon#153). #31 accepts this on the record or narrows which names are unlinked, and tests its choice.

## Consequences

### Positive

- A defect in the parser, screen, renderer or input path runs as an unprivileged account with no capabilities, where the C reference ran it as root.
- The parent restores the VT when the terminal process dies, by a panic included, so a crash leaves a usable console.
- Every level runs from a plain unit, an init script or a hand launch; systemd, logind and libseat add to it and never gate it.
- The login session is the one getty gives: sudo, root's console, the user's home and `/tmp` behave as under getty.
- The minspec build links no libsystemd and no libseat.

### Negative

- The request channel is new code running as root, reading input from the process an attacker reaches first.
- Each VT switch, device open and display wake costs a round trip to the parent.
- systemd's sandboxing goes unused.
- A root login that logind authenticated loses file media, a departure from the C reference.
- A machine with no account to drop to runs the stream's code as root.

### Neutral

- SECURITY.md's scope, "vtium runs as root on a virtual console", changes when L1 lands.
- Tier 3 reads memory by process name (#21), so its measure counts the parent and the terminal process, and the helper where images are built (#22).
- The terminal process's own landlock and seccomp are a later step of L1, not a gate on it.

## Alternatives Considered

- **Root for life (D1 a) as the target.** Rejected: every defect in the code that reads the stream runs as root. It stays the floor.
- **Unprivileged only through a seat (D1 c alone).** Rejected: it needs a seat daemon, and `/bin/login` still needs root.
- **A terminal process that keeps `CAP_SYS_TTY_CONFIG` for the VT ioctls.** Rejected: it leaves a capability in the process that reads the stream, and DRM master would still need the parent.
- **One account for the terminal process and the pre-login helper.** Rejected: a decoder compromised at the login prompt could signal the process holding the display.
- **A second executable for the helper.** Rejected for the install, skew and path reasons in D2.
- **Unit sandboxing, `ProtectSystem=strict` and `NoNewPrivileges` among it.** Rejected: the login session inherits it (D3).
- **A `PAMName` greeter session for libseat's logind backend,** as the C reference's libseat build had. Rejected: the login child inherits the session (D3). #34 finds a seat route that leaves the login its own session.
- **Reading logind's state under `/run/systemd` in place of sd-login.** Rejected: systemd does not document those files as an interface (external, not re-verified).
