---
contract: adr/v1
kind: decision
verb: constrain
capability: platform
basis:
  - operator: aaronsb
    level: directed
    said: "The kmscon security model works, for reasons vtium should not have to relearn."
    via: operator, in session, on the first draft of this record, relayed by the team lead
    paraphrase: true
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
    said: "The record also covers the kmscon fork's security model: the nobody user, the systemd service and the rest."
    via: operator, in session, relayed by the team lead in this record's brief
    paraphrase: true
  - operator: aaronsb
    level: guided
    said: "vtium also works without them."
    via: operator, in session, the line after the one above, relayed by the team lead
    paraphrase: true
  - operator: aaronsb
    level: guided
    said: "The minspec work focuses on software more than on hardware. For now it is enough that vtium can be launched minimally with a plain unit script."
    via: operator, in session, relayed by the team lead
    paraphrase: true
  - operator: aaronsb
    level: guided
    said: "This has a side effect of avoiding building interdependencies."
    via: operator, in session, relayed verbatim by the team lead
  - operator: aaronsb
    level: guided
    said: "The aaronsb/kmscon fork's commits are the pattern for how vtium's security capabilities progress."
    via: operator, in session, pointing to the aaronsb/kmscon fork's commits as the pattern to follow; the team lead relayed this phrase only
    paraphrase: true
  - operator: aaronsb
    level: guided
    said: "Each convenience is used when present and is never required."
    via: the GitHub issue for building and judging vtium on ARM, comment of 2026-10-26, posted from the operator's account as the operator's direction; the wording is agent-written
  - upstream: "the C reference's kmscon at b1f7a7c5: kmscon ran as root for life, with setuid, setgroups, prctl and capset only in src/image_host.c:209-218; the login child was forked as root and ran `/bin/login -p` (src/misc/pty.c:261-269, 404-427); the units carried no hardening directive, and set `User=root` and `PAMName=kmscon` only in a libseat build (scripts/systemd/kmsconvt@.service.in, kmscon.service.in, meson.build:370-373)"
  - upstream: "kmscon ADR-103, the image helper: a separate process; its identity from logind or the pty's owner, never root by fallback; the pre-login account, then nobody; the child's setup; the file rules; and a login inside kmscon's greeter session treated as having no session of its own"
  - upstream: "the C fork's pull request that built the image helper (kmscon ADR-103 step 4b), the helper's progression in commit order: identity first (c8f02579, 2136ddfc, 6f9b0759), the host's distrust of the helper (79d765f1, a15d49ad, 935eec64), the path rules (58f21637, 76202204, 9ef29b7e, a7d475dc), the pre-login and root cases (735d12b7, f1a7f077, 2bd8013d, 75eaa582, fb991022); and the minimal build's refusal of an option turned on without its dependency (d30292fe)"
  - standard: "systemd.exec(5), systemd.resource-control(5) and pam_systemd(8), systemd 262: what a unit's directives apply to the processes it starts (external, not re-verified here)"
  - precedent: ADR-1000
  - operator: aaronsb
    level: directed
    said: 'sd-bus behind the logind Cargo feature, plus a fallback: when logind is not running or the feature is off, vtium writes mem to /sys/power/state as root, so the minspec build suspend key works too'
    via: operator decision on grab-suspend in the work on vtium's configuration and terminal wiring, 2026-10-27, relayed by the team lead; paraphrased in the relay
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: advisor
    said: "Sound carry-forward; fix the opt-in logind default and two never-silent sentences before acceptance. D1-D3 serve the operator's intent. Drift: logind opt-in left packaged image builds on logind hosts with the weaker pty-owner identity, against the C reference's sd_login=auto. Overstated: file media before login were logged at debug only; the gid-0 rule held only when not root. Unverified: the root table's kernel conditions."
    via: skeptic subagent (claude-opus-5-5), the operator's requested second opinion, 2026-10-26; read the C reference's working tree and systemd.exec(5); kernel source unreachable
  - operator: aaronsb
    said: "Yes to both probes: vtium runs as root and installs the same kind of unit as the C reference, and root's image files are read as root until the image helper's issue says otherwise."
    via: operator, in session 2026-10-26, after reading the record and the second opinion
    covers: [as the C reference, root's images]
    paraphrase: true
  - operator: aaronsb
    said: "Accepted: the file that holds the login key gets the head file's ownership check."
    via: the operator's answers in conversation with the team lead, 2026-10-29, relayed in the team lead's brief; the wording is the team lead's
    paraphrase: true
  - operator: aaronsb
    said: "localed, minspec xkb (Recommended)"
    via: the question tool in the steering session, 2026-11-10, card 1b question Q10
  - operator: aaronsb
    said: "Both leans (Recommended)"
    via: the question tool in the steering session, 2026-11-10, card 1b question Q12
  - operator: aaronsb
    said: "Accept all five (Recommended) [vtium.conf's xkb-* keys win over localed; the PAM file is named vtium]"
    via: the question tool in the steering session, 2026-11-10, on the record agent's calls for card 1b
  - operator: aaronsb
    said: "Accept mixed (Recommended)"
    via: the question tool in the steering session, 2026-11-10, on KillMode=mixed in the libseat build
  - operator: aaronsb
    said: "so we need to probably not read the x11 layout, what happens on a system without x?; asked to choose a source order: Console keymap, then X11 (Recommended)"
    via: the question tool in the steering session, 2026-11-10, on the keyboard layout's source
  - operator: aaronsb
    said: "we should hold this until 1b is fixed. libseat is new territory for us, and writing it up as an issue to fix later won't help us"
    via: the question tool and chat in the steering session, 2026-11-10, on the libseat build's login sessions
  - operator: aaronsb
    said: "I'm leaning towards scope per login"
    via: the question tool and chat in the steering session, 2026-11-10, on the libseat build's login sessions
  - operator: aaronsb
    said: "I'm ok with just more instances in order to maintain scope per login."
    via: the question tool and chat in the steering session, 2026-11-10, on the libseat build's login sessions
  - operator: aaronsb
    said: "Yes and yes"
    via: chat in the steering session, 2026-11-11, on reading /etc/default/keyboard for Debian and Ubuntu
  - operator: aaronsb
    said: "why not 'vtium console enable --seat' and 'vtium console enable --direct' and 'vtium console enable' returns the type of system it likely is (offering an opinion on wether to run as seat or direct)"
    via: the question tool in the steering session, 2026-11-11, seat-mode packaging question Q13, on the names of the two ways of running from one package
  - operator: aaronsb
    said: "Greeter (Recommended)"
    via: the question tool in the steering session, 2026-11-11, card 1b question Q16, on the logind class of vtium's own PAM session in seat mode
  - operator: aaronsb
    said: "A root broker and a drawer without root, in both modes (the research's design A) [D4]. In direct mode the broker serves the devices as a seat manager would; under logind or seatd the seat manager serves them, and the broker keeps only login, the helpers, the power keys and reaping. The drawer speaks one device protocol in both modes, and the broker takes login's command line from its own trusted configuration, never from the drawer."
    via: the maintainer, in the steering session, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Plan for one drawing process per session, over one drawer for all sessions, so that a compromised session's process cannot reach another session's terminal, such as a root shell or a password typed at a login prompt [D4, one process for each session]."
    via: the maintainer, in the steering session, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Open question 1 [which account each process runs as]: (a), one account for the compositor and one shared by all session processes, each session process not dumpable and in its own landlock domain."
    via: the maintainer, in the steering session, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Your leans on the broker questions are correct and I accept all of them"
    via: the maintainer, in the steering session, 2026-11-13
  - operator: aaronsb
    said: "In seat mode the broker stops making KDSETMODE and KDSKBMODE; logind and seatd make them. The broker keeps VT_ACTIVATE until a check covers switching through libseat [D4, open question 3, after checks 1 and 2]."
    via: the maintainer, in the steering session, after the tier 3 checks, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Under a seat manager, vtium accepts that the VT's keyboard comes back in K_UNICODE; the broker does not restore the saved mode there. In direct mode the broker still restores it [D4, when the drawer dies]."
    via: the maintainer, in the steering session, after the tier 3 checks, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "Per-session processes (slice 3) require Linux 6.12 (landlock ABI 6 with LANDLOCK_SCOPE_SIGNAL and LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET). On an older kernel vtium keeps one drawer for all sessions, as slice 1 builds it, and the security page says what the split protects there [D4, slice 3 and the limits, after check 8]."
    via: the maintainer, in the steering session, after the tier 3 checks, 2026-11-13
    paraphrase: true
  - operator: aaronsb
    said: "In slices 1 and 2 the drawer gets basic hardening: no-new-privileges, an empty bounding set, its ids read back, not dumpable, and PR_SET_PDEATHSIG; no seccomp or landlock yet. A design study follows on hardening kept out of the main program, in a crate of security profiles chosen by hardware class or driver, with detection modules others can add, after the model of lm-sensors, starting from precedent and criteria."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 1
    paraphrase: true
  - operator: aaronsb
    said: "The accounts are vtium for the drawer, later the compositor, and vtium-session for the session processes. When an account is missing, vtium runs as one process, as today, and logs one line."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 2
    paraphrase: true
  - operator: aaronsb
    said: "When vtium starts without root there is no broker; vtium runs as one process, as today."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 3
    paraphrase: true
  - operator: aaronsb
    said: "Add the missing messages to D4's lists before slice 1 builds: take and release a card for the wake-up retry and a failed wake-up, restore the VT's modes, SIGWINCH to a pty, stop a helper, and the size and two environment hints when opening a session."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 4
    paraphrase: true
  - operator: aaronsb
    said: "The broker waits 10,000 ms for the drawer at a release, a constant in the code and not a vtium.conf key. A hung drawer stalls a VT switch away for up to 10 s."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 5
    paraphrase: true
  - operator: aaronsb
    said: "The seam refactor between the terminal side and the drawing side is a card of its own, beside slice 2; D4's note changes."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 6
    paraphrase: true
  - operator: aaronsb
    said: "vtium blank signals the broker, which passes the order to the drawer."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 7
    paraphrase: true
  - operator: aaronsb
    said: "Seat mode's unit drops PAMName= and goes back to KillMode=process; the broker opens the drawer's greeter session; tier 3 measures it; D3's notes change."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 8
    paraphrase: true
  - operator: aaronsb
    said: "The package adds the drawer's account to seatd's group; tier 3 checks it."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 9
    paraphrase: true
  - operator: aaronsb
    said: "Keeping the terminal modes through a restart lands in slice 3; a restarted vtium sends a focus-in to each shown console whose kept modes have mode 1004 on, and D16 is amended."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 10
    paraphrase: true
  - operator: aaronsb
    said: "A restart keeps every terminal mode, the keyboard protocol's flags, the cursor's visibility and style, and the active screen; not the contents, margins, tab stops, character sets, title or palette."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 11
    paraphrase: true
  - operator: aaronsb
    said: "The broker exits with the drawer's own status; a drawer killed by a signal counts as a failure."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 12
    paraphrase: true
  - operator: aaronsb
    said: "The drawer reads vtium.conf itself; vtium check-config warns when the drawer's account cannot read it."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 13
    paraphrase: true
  - operator: aaronsb
    said: "READY=1 goes out once the broker holds the VT, ADR-1500 D7 unchanged; the drawer's ready ends a start bound, and without it the broker restores the VT and fails."
    via: the maintainer's answers to the privilege split's gap-check, 2026-11-21, relayed by the steering session, question 14
    paraphrase: true
  - operator: aaronsb
    said: "I feel like refusing and keep going is better than stopping the drawer, because it's less likely to be a 'denial of service' style attack that way"
    via: the maintainer, answering the steering session's question through the question tool on what the broker does with a message it refuses, 2026-11-24
  - operator: aaronsb
    said: "A: split behind a feature (Recommended)"
    via: "the steering session's question tool, 2026-11-25, on the privilege split and the minspec size budget; the option read: \"the root/drawer split builds only behind a cargo feature that minspec leaves out, so minspec stays one process and drops the split's ~32 KB (it sat 56 B under tier 3's 4,390,912 B budget); packaged builds and make vt-accept's defaults keep the split\"; the option wording is the steering session's"
    paraphrase: true
  - operator: aaronsb
    said: "one process with a seat manager. but, let's remember to make it work on systems that dont have a seat manager"
    via: question tool, 2026-11-25
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

- **Decided:** vtium carries the C reference's security model forward as its logic flow, not its C. The main process holds root for life, launched by a plain unit, an init script or by hand, and forks the login child as root to run `/bin/login`; nothing else drops. Image media are read and decoded in a helper process following the C reference's flow, as the session's user and never as root by fallback. The units keep getty's ordering and TTY settings, as the C reference's did, and set no hardening directive, since the login session would inherit it. They differ from the C reference's units in one place: under a closed head-file policy, the getty fallback skips the getty (ADR-1401 D9). logind and libseat are an optional layer, each a Cargo feature, and vtium runs without them as the C reference did. vtium departs only where the C reference had a listed defect or gap: a test of the login child's descriptors, the helper controls that the image helper's issue does not yet list, and the shared-memory unlink consequence. D4 replaces root for life with a split. A small root broker keeps the jobs that need root, and the processes that read the sessions' output and draw run without root, one for each session in the end. A vtium started without root, or on a machine without the drawer's account, runs as one process. Until D4's first slice is built, vtium runs as D1 says.
- **Trades away:** A defect in the code that reads the pty stream runs as root, as it did in the C reference, until D4's first slice is built. The unit uses none of systemd's sandboxing. Under D4, the broker and its protocol are new privilege code, and each VT switch costs a round trip between two processes.
- **One-way?** No. D4 is built in slices, and the first slice is shaped so that the later ones do not undo it.
- **Probes:**
  - *Confident (as the C reference):* vtium runs as root and installs the same kind of unit as the C reference, and changes only where the C reference had a known gap.
  - *Not confident (root's images):* when root logs in on a vtium console on a system running logind, image files root's programs name are still read as root, as the C reference allowed, until the image helper's issue says otherwise.
- **Inversion:** Between "port the C reference's C" and "design a novel privilege model" (a root parent with an unprivileged terminal process, a sandboxed unit). The answer is neither: the C reference's logic flow, in idiomatic Rust, with fixes only where the C reference's own trackers listed a defect or gap. Since 2026-11-13 the answer moves to the second end in one part: D4 separates a root parent from the processes that read output. The unit still sets no sandboxing directive.

> Edited 2026-11-13: the maintainer decided the privilege split of D4 (`considered`, 2026-11-13). The Summary did not name D4. "Trades away" ended at "The unit uses none of systemd's sandboxing.", "One-way?" read "No. A privilege split stays open (Alternatives), and nothing here forecloses it.", and the inversion ended at "listed a defect or gap."

> Edited 2026-11-22: the maintainer answered the gap-check's fourteen questions on the privilege split on 2026-11-21 (`considered`, 2026-11-21, questions 1 to 14), and D3 and D4 take each answer where it belongs. The Summary's "Decided" did not say when vtium runs as one process.

## Context

The C reference ran as root for life, and SECURITY.md states the same for vtium today. Only its image helper changed identity. The fork's commits built the helper's protections in an order: who the helper runs as, then the host's distrust of the helper, then the path rules, then the pre-login and root cases. Its units carried no hardening.

vtium runs where there may be no systemd, logind, udev daemon or seat daemon, launched by a plain unit, an init script or by hand. The C reference already ran that way: its minimal build linked none of them but libudev, and its default build linked sd-login only when images were built and the library was present.

On 2026-11-13 the kernel check that D1 asked for was made, against the source of Linux 7.2.14. It read the check the kernel makes for each operation that vtium needs root for. Some checks look at the calling process on every call, so a process without root cannot make that call with a descriptor that root passed it. Other checks happen only when a file is opened, so a process without root can use a descriptor that a root process opened and passed on. D1's table now gives the result, and D4's split is built on it.

## Decision

### The layers, carried forward

The C reference's layers, in its order, each optional above L0:

| Layer | Adds | Builds on | Build layer, issue | Without its convenience |
|---|---|---|---|---|
| L0, the baseline | Root for life; the VT, DRM or fbdev, and evdev opened directly; the login child holding only fds 0-2; the reply rule and every bound | nothing | minspec; the terminal's modes and replies, the displays, the VT and input, the pty and event loop, and the configuration and wiring | Uses none, as the C reference's minimal build used none but libudev. |
| L1, the helper | Reading and decoding image media as the session's user, with the identity modes below (feature `images`) | L0 | images; the image helper and its sandbox | No logind: the identity is the pty's owner, as in the C reference. No pre-login account: `nobody`, then refused, as in the C reference. |
| L2, logind and libseat | The helper's identity from logind, through sd-login, `key-suspend` through logind's `Suspend`, and the system's keyboard layout from localed when the console keymap gives none, each through sd-bus (feature `logind`); the cards, the keyboards and pointers, and VT switching through libseat (feature `libseat`), from seatd, or from logind in seat mode (D3) | L0, L1 | seat; the image helper, libseat seats, and the configuration and wiring | A build without the feature, or a system where logind or the seat daemon is not running, behaves as L0 and L1, as the C reference did with logind absent and libseat off by default; `key-suspend` writes `mem` to `/sys/power/state` as root, and the keyboard layout comes from the console keymap, else xkb's default, unless vtium.conf sets one. |

The units and the sysusers file are installed files, never a layer: a hand launch needs none of them (D3).

> Edited 2026-11-10: the maintainer decided that a libseat build opens the input devices through the seat too, and that the keyboard layout follows localed in the builds that link logind (`considered`, 2026-11-10, card 1b questions Q10 and Q12). L2's row named devices and VT switching through libseat, and no keyboard layout.

> Edited 2026-11-10, later: the maintainer put the console keymap before localed's X11 layout, in every build (`considered`, 2026-11-10, on the keyboard layout's source). L2's row read "the system's keyboard layout from localed", and "the keyboard layout is xkb's default unless vtium.conf sets one".

> Edited 2026-11-11: the maintainer decided that one install offers two ways of running, chosen for each VT when vtium is turned on (`considered`, 2026-11-11, on seat-mode packaging; D3). L2's row read "VT switching through libseat (feature `libseat`)", with no source named.

### Each convenience is a Cargo feature

- `images` brings the helper. No build has images without it.
- `logind` reads sessions through sd-login, from libsystemd or libelogind. It is on wherever `images` is in the default feature set, as the C reference's `sd_login=auto` linked sd-login whenever images were built and the library was present; it is used at run time only when logind is running. A build with `images` and without `logind` is an explicit choice for hosts without libsystemd or libelogind, and on a logind host it falls back to the pty's owner, which kmscon ADR-103 rejected as the first identity source because root can chown a pty; vtium lists that build as a departure. (Added 2026-10-26, from the advisor's review.)
- `logind` also calls logind's `Suspend` for `key-suspend` through sd-bus, from libsystemd 240 or later, else libelogind 241 or later, through pkg-config, with a 2 s bound on the answer. Where the bus has no logind and cannot start it, or the feature is off, vtium writes `mem` to `/sys/power/state` as root, from a shell it starts, so the loop still serves the VT's switch to the kernel's suspend console. That write passes logind's inhibitors and its `PrepareForSleep` signal by, so on a logind host a build without the feature suspends with no delay inhibitor held and no other session's lock-on-suspend run; a refusal from a running logind is never worked around. Every place vtium touches systemd, logind or PID 1, and what it does without them, is listed in [docs/systemd.md](../../systemd.md). (Added 2026-10-27, from the operator's decision in the work on vtium's configuration and terminal wiring. libelogind added 2026-10-28.)
- `libseat` takes the cards, the keyboards and pointers, and VT switching through libseat. vtium opens no input device itself in a libseat build.
- The system's keyboard layout, which an unconfigured vtium types with on every VT it claims, comes from three places, in this order. vtium.conf's `xkb-*` keys win where vtium.conf sets them (the agent's call, which the maintainer accepted on 2026-11-10). Next comes the console keymap, `KEYMAP=` in `/etc/vconsole.conf`, which vtium translates into the X11 layout, model, variant and options through systemd's table `/usr/share/systemd/kbd-model-map`, the table localed uses for the same translation. Every build reads the console keymap, the minspec build among them, and reading it needs no bus. When vconsole.conf sets no `KEYMAP=`, or the table has no row for it, `logind` reads localed's X11 layout, model, variant and options, through sd-bus from localed, the service behind `localectl`. A build without the feature then uses xkb's default layout, and vtium logs one line that names the keymap it could not translate. vtium reads the layout at its start, and a build with `logind` also follows a change to it while vtium runs. localed announces each change with the signal `PropertiesChanged` of `org.freedesktop.DBus.Properties` on its object `/org/freedesktop/locale1`. vtium watches for that signal from localed's bus name, and when the signal names one of localed's keyboard properties, vtium reads the layout again in the order it uses at its start. It reads the files again too, because `localectl set-keymap` rewrites `/etc/vconsole.conf` before localed announces the change. vtium.conf's `xkb-*` keys still win. When the layout that results differs from the one in use, every keyboard takes the new keymap and vtium logs one line. When localed cannot be read, the keymap stays as it is. A build without the feature reads the layout once, at its start.

> Edited 2026-11-10: the maintainer decided the input devices and the keyboard layout (`considered`, 2026-11-10, card 1b questions Q10 and Q12). The `libseat` bullet read: "`libseat` takes devices and VT switching through libseat.", and the list named no keyboard layout.

> Edited 2026-11-10, later: the maintainer asked what happens on a system without X, and put the console keymap before localed's X11 layout in every build (`considered`, 2026-11-10, on the keyboard layout's source). The layout bullet read: "`logind` also reads the system's keyboard layout from localed, the service behind `localectl`, through sd-bus. localed's X11 layout, model, variant and options give the layout an unconfigured vtium types with, on every VT it claims. vtium.conf's `xkb-*` keys win over localed's values where vtium.conf sets them … A build without the feature, the minspec build among them, uses xkb's default layout unless vtium.conf sets one."

> Note 2026-11-11: Debian and Ubuntu keep the console's layout in `/etc/default/keyboard`, which console-setup reads, and not in `/etc/vconsole.conf`. The file holds the layout in XKB's own names, as `XKBMODEL`, `XKBLAYOUT`, `XKBVARIANT` and `XKBOPTIONS` (keyboard(5), keyboard-configuration 1.242~deb13u1). The maintainer approved reading it as a source of its own (`considered`, 2026-11-11, "Yes and yes"). It comes after the console keymap and before localed, in every build, and vtium takes its values as they are, with no translation. vtium reads the file as the shell that setupcon sources it with reads it: one assignment a line, a value bare or quoted, `#` comments, and the last assignment of a name winning. An empty value counts as unset, and a file with no `XKBLAYOUT` passes the choice on to localed. So the order is: vtium.conf's `xkb-*` keys, the console keymap through kbd-model-map, `/etc/default/keyboard`, localed in a build with `logind`, and xkb's default.

> Edited 2026-11-11, later: the work that follows the system's layout made vtium follow a change to it while vtium runs, in a build with `logind`, as ADR-1000's row `platform.system-layout-follow` plans. The layout bullet's last sentence read: "The work that follows the system's layout decides whether vtium also follows a change that localed or vconsole.conf makes while vtium runs."

- A feature turned on without the C library it binds fails the build with a message naming the library, as the C reference's minimal build refused an option without its dependency. The minspec build links none of them.

> Edited 2026-11-07: the maintainer renamed vtium.conf's key bindings to more accepted terms, in their words "lets fix the vocabulary to more accepted terms" and "key bindings seems right" (ADR-1005 D10). The suspend key, named `grab-suspend` in the table of layers and in this list, is `key-suspend`; vtium still reads `grab-suspend` as `key-suspend` and reports the line.

### D1: who holds root, and for how long

The main process holds root for life, as the C reference's did. It forks the login child as root, made a session leader with the pty as its controlling terminal, to run `/bin/login -p` with the environment reset; nothing else drops privilege. D4 replaces this with a split, built in slices. Until D4's first slice is built, this paragraph describes vtium.

The table lists each operation that needs root, or that needs a file only root may open, with the check the kernel makes for it. Each check was read in the source of Linux 7.2.14 (the note below the table). A check *at open* happens once, when the file is opened. After that, any process that holds the descriptor can use the file, including a process without root that received the descriptor over a Unix socket. A check *on each call* looks at the process that makes the call, every time, so a passed descriptor does not carry it. File modes are the development machine's.

| Operation | The kernel's check | Can a process without root use a descriptor root passed it? | When it repeats | In seat mode |
|---|---|---|---|---|
| Opening the VT (`Tty::open`), and finding a free VT through `/dev/tty0` (`Tty::find`) | At open. `/dev/ttyN` is `root:tty 0600`, and `/dev/tty0` is `0620`. `VT_OPENQRY` has no check of its own. | Yes | At start | vtium still opens its VT. |
| `VT_SETMODE`, `VT_RELDISP`, `VT_ACTIVATE`, `KDSETMODE` and `KDSKBMODE` (`vt/kernel.rs` in vtium-platform) | On each call. The VT must be the caller's controlling terminal, or the caller must have `CAP_SYS_TTY_CONFIG` (`vt_ioctl` in `drivers/tty/vt/vt_ioctl.c`). Who owns the tty's file does not matter. | No | `VT_RELDISP` at each release and acquire, `VT_ACTIVATE` at each switch key, and the others at start, at each retake and at exit | Today vtium calls `KDSETMODE` and `KDSKBMODE` at each enable and disable (`tty_graphics` and `tty_text` in `seat.rs`), and `VT_ACTIVATE` when its VT is not shown (`activate` in `seat.rs`). logind and seatd make `KDSETMODE` and `KDSKBMODE` themselves (D4's checks 1 and 2), so under D4 only `VT_ACTIVATE` stays, and the broker makes it. |
| The VT calls that only read (`VT_GETSTATE`, `KDGKBMODE`, `VT_GETMODE`), termios, `tcflush` and the bell | None beyond the open file | Yes | Each turn | The same |
| The switch signals | The kernel sends them, as privileged, to the process that called `VT_SETMODE`, whatever its user | Not a descriptor | Each switch | The seat manager receives them. |
| Opening a DRM card (`Vt::open_device`, and the driver probe in `udev.rs`) | At open. Cards are `root:video 0660`, with an ACL for the active user. | Yes | At start and at each hotplug | The seat manager opens it. |
| Setting and dropping DRM master (`set_master` and `drop_master` in `display/card.rs`, called from `try_wake` and `release_master` in `display/drm2d.rs`) | On each call. The caller needs `CAP_SYS_ADMIN`, unless the file has been master and the process id recorded on the file is the caller's (`drm_master_check_perm` in `drivers/gpu/drm/drm_auth.c`). The recorded id stops moving once the file first becomes master. | No, for a file that a root process opened or made master | At each acquire and release, and at each retry of a wake-up | The seat manager does it. |
| KMS calls: atomic commits, page flips and `SETCRTC` (`display/card.rs`) | On the file, not the caller: the file must hold master (`drm_ioctl_permit` in `drivers/gpu/drm/drm_ioctl.c`). Dumb buffers and `ADDFB2` need no master. | Yes, while the file holds master | Each frame | The same |
| The GPU's render node, and fbdev's mode and blank calls | At open. `renderD*` is `0666`, and fbdev is `root:video 0660`. | Yes | At start | The same |
| Opening the input devices (`Evdev::open`) | At open. The devices are `root:input 0660`, and no evdev call checks its caller. `EVIOCREVOKE` revokes the file description, so every copy of the descriptor stops working. | Yes | At start, at each hotplug, and at each acquire: `Input::sleep` closes every device and `Input::wake_up` opens them again | The seat manager opens and revokes them. |
| Listening to udev's events | Any process may listen. | Not needed | — | The same |
| Making a session's pty (`Pty::start`) | No privilege. `/dev/ptmx` is `0666`, the pty's slave belongs to the user who made the pty, and `TIOCSCTTY` needs only a session leader. | Yes | Each session | The same |
| Running `/bin/login` (`child.rs`) | `login` is not setuid. It calls `setgroups`, `setuid`, `fchown` and `vhangup`, which need `CAP_SETGID`, `CAP_SETUID` and `CAP_SYS_TTY_CONFIG` among root's capabilities. | No. The process that runs `login` must hold root when it does. | Each session | Still root |
| Signalling and reaping a session's processes (`pidfd_send_signal` in `pty.rs`, and `waitid`) | On each call. The caller needs the same user as the target, or `CAP_KILL` (`kill_ok_by_cred` in `kernel/signal.c`). A pidfd gives no extra right, and only the parent can wait for a child. | No | At each session's end and hang-up | Still root |
| Starting a helper as another user (`child` and `drop_bounding_set` in `image_helper/start.rs`, and `nobody` in `helper.rs`) | `CAP_SETUID` and `CAP_SETGID`, and `CAP_SETPCAP` to drop the bounding set | Not a descriptor | Each helper start | Still root |
| `key-reboot` and `key-power-off` (`power.rs` in vtium-platform) | Signalling PID 1 needs root's user id or `CAP_KILL`, and `reboot(2)` needs `CAP_SYS_BOOT`. | No | Each press | Still root |
| `key-suspend` without logind (`suspend` in `power.rs`) | At open. `/sys/power/state` is `0644` and root's. | A descriptor root opened would do. | Each press | logind's `Suspend`, which polkit decides |
| Keeping the pty masters through a restart (`survive.rs` and `survive/notify.rs`) | Not a kernel check. systemd takes `FDSTORE` messages only from the unit's main process (`NotifyAccess=main`), and the holder's socket under `/run/vtium` is root's. | Not a descriptor | Each session's start and end | The same |
| A systemd scope for each login (`logind/scope.rs`) | Not a kernel check. polkit decides `StartTransientUnit`, and lets root pass. | Not a descriptor | Each login | Seat mode only |
| vtium's run directory, `/run/vtium/<tty>/`, with its state file, its control socket and the closed marker beside it (ADR-1401 D10) | At open. root owns the directory, mode `0755`. | A descriptor root opened would do. | At start, and at each change of state | The same |
| Reading the kernel's messages from `/dev/kmsg` (`System::open` in `kernel.rs`) | At open, under `kernel.dmesg_restrict`, measured by D4's check 7 | Yes | At start | The same |

Two rows decide the shape of any split. A VT call that changes the VT checks its caller each time, so a process without root cannot make it with a passed descriptor. DRM master checks its caller too, and its exception for a process without root does not follow a descriptor that root opened. Master belongs to the file description, though, and not to a process. When a root process sets or drops master on its own copy of a card's descriptor, every copy of that file gains or loses master with it. A process without root then draws through its own copy while the file holds master. The comment above `drm_master_check_perm` names this as the way logind passes descriptors. The kernel does not drop master at a VT switch: the holder drops it, or master goes when the last copy of the file closes.

> Edited 2026-11-13: the kernel check this decision asked for is made. A research pass read each condition in the source of Linux 7.2.14, the stable tree as git.kernel.org serves it, and the table now gives what it found. The check corrected seven things. First, the VT calls check whether the VT is the caller's controlling terminal, not who owns the tty; the earlier phrase came from a kernel comment that says "owner of the tty". Second, DRM master's exception for a file that was master also needs the caller to be the process recorded on the file, so the exception does not follow a passed descriptor. Third, in seat mode vtium still opens its VT and makes VT calls itself, so the seat does not do all of it. Fourth, the input devices open again at each VT acquire, not only at start and at hotplug. Fifth, in direct mode the cards stay open across a VT switch, and what repeats is setting and dropping master. Sixth, making the pty needs no privilege, and `login` needs root when it runs. Seventh, the table lacked the power keys, signalling and reaping a session, the store of pty masters, the scope for each login, and the opens of `/dev/tty0` and of a card in the driver probe. The rows for the run directory and for `/dev/kmsg` are added beside them, and the check did not read `/dev/kmsg`. The section read "Root is what these need, from the C reference's code, the kernel's checks being external and not re-verified". A paragraph followed, saying that the conditions came from the C reference's comments and general knowledge, and that they are checked before any privilege split is designed. The table had five rows, under the headings "Operation", "Window" and "Under libseat". The earlier text is in git at 9e91f218.

> Edited 2026-11-13: after D4's checks 1 and 2, the maintainer decided that in seat mode the seat manager makes `KDSETMODE` and `KDSKBMODE` (`considered`, 2026-11-13). The row of the VT calls ended in seat mode at "and `VT_ACTIVATE` when its VT is not shown (`activate` in `seat.rs`).", and began "vtium still calls".
> Edited 2026-11-13: D4's check 7 measured `/dev/kmsg` in the tier 3 guest. The row's second cell read "Not read in the check. D4 lists it among the checks the build makes first.", and its third read "Not known".

### D2: the helper and its sandbox

The helper follows kmscon ADR-103's flow, carried into Rust by the image helper's issue:

- **a separate process,** one per session, started on the first request and stopped after an idle interval (ADR-1301 D2), at the session's end, a user change, a larger `max_bytes` or a failure, with the next request starting a new one; *edited 2026-10-28:* the operator decided the idle exit, and this read "started on the first request and stopped at the session's end";
- **the child's setup,** in the C reference's order: credentials resolved before the fork; `setsid`; the signal mask cleared; fds 0-2 on `/dev/null`, the socket on fd 3, every other descriptor closed; the memory limit; `setgroups`, `setresgid` and `setresuid`, read back; the ambient capabilities cleared, then `PR_SET_NO_NEW_PRIVS`; `chdir /`, and `execve` with `TMPDIR` as the only environment;
- **the IPC checks:** fixed-size requests and replies, sealed memfds both ways, every field checked by the host, the seals before the size, and the result mapped read-only;
- **the timeout:** 2,000 ms with input paused, after which the helper is killed and the reply is `FAILED`;
- **the file rules** of ADR-103.

Whether the helper is a second executable or the same binary is a packaging detail the image helper's issue decides. Whether seccomp or landlock narrows the helper is that issue's to weigh, as the C reference left its filter to a C fork issue of its own.

The identity, by the C reference's rules:

- with `logind` built and logind running, the user of the login child's own logind session of a login class; a session inherited from vtium's own greeter session does not count. When that user is root, the C reference ran the helper as root; vtium does the same, and any change belongs to the image helper's issue;
- otherwise the owner of the session's pty;
- before login, file media are refused, and inline PNG and zlib are decoded as the pre-login account, then `nobody`, and are otherwise refused. The C reference's account was `kmscon-image`; its name in vtium is for the image helper's issue to choose.

### Never silent

These are the C reference's own behaviour:

- The helper never runs as root by fallback. Where the identity comes from the pty's owner and that owner is root, file media are refused and inline media take the pre-login path.
- File media before login are refused, logged at debug level, as in the C reference.
- A vtium not running as root whose ids or groups include gid 0 handles every request as before login. When vtium runs as root and the session is root's, the helper runs as uid 0 and gid 0, as the C reference's did (image_host.c:141-180); a change belongs to the image helper's issue.
- Pre-login decoding with no account to run as is refused, and logged.

### D3: what the unit may harden

The C reference recorded no reason for its units' lack of hardening. The likely one: a unit's sandboxing directives set process attributes, namespaces or cgroup limits on the process systemd starts, and every process it starts inherits them, the login child and the user's shell included (systemd.exec(5), systemd 262; external, not re-verified):

- `NoNewPrivileges` stops sudo, su and every setuid program in the session;
- `CapabilityBoundingSet` caps root's own console session;
- `ProtectSystem`, `ProtectHome`, `PrivateTmp`, `PrivateDevices`, `ReadOnlyPaths` and the other mount directives give the session a namespace where root cannot change the system, the user cannot see their home, or `/tmp` is not the system's;
- `SystemCallFilter`, `RestrictNamespaces`, `MemoryDenyWriteExecute` and `LockPersonality` break containers, JITs and emulators in the session;
- cgroup directives such as `DeviceAllow` and `MemoryMax` reach the session until pam_systemd moves it to a scope of its own, and for its whole life where logind is absent.

vtium's units match the C reference's, with getty parity and no hardening directive:

- getty's `TTYPath`, `TTYReset`, `TTYVHangup`, `TTYVTDisallocate` and `UtmpIdentifier`, its ordering, `Conflicts=getty@%i`, and a getty fallback: `OnFailure=` starts `vtium-getty@%i`, which starts `getty@%i` unless vtium's closed marker exists (ADR-1401 D9);
- vtium's own unit names, `vtium@.service` and `vtium-getty@.service`, which every install and tier 3 use (ADR-1500 D3);
- two ways of running, direct mode and seat mode, from one install, chosen for each VT when vtium is turned on there (ADR-1500 D7). In direct mode, `vtium@.service` runs vtium as root. vtium opens the screen and the keyboards itself and holds no logind session, and each login registers with logind on seat0 with its VT, as a getty's login does. In seat mode, `vtium-seat@.service` runs vtium. That unit is `vtium@.service` with `User=root`, `PAMName=vtium` and `KillMode=mixed` added (the notes below). It opens a logind session of vtium's own, libseat's logind backend takes the seat from that session, and each login runs in a systemd scope of its own. vtium ships the PAM file `vtium` for it, with the session stack of the C reference's greeter PAM file, which opens the session in the class `greeter` (the note on the session's class below). Every install of a build with `libseat` and `logind` ships both units and the PAM file, and an install of any other build ships `vtium@.service` alone. `vtium-seat@.service` has `Conflicts=vtium@%i.service` and `After=vtium@%i.service`, so a VT runs one mode at a time. Once D4's second slice is built, `vtium-seat@.service` adds `User=root` alone and keeps `KillMode=process`, and the broker opens the drawer's greeter session itself through the same PAM file (the note on seat mode under the split, below);
- an optional sysusers file creating the pre-login account, with no home, shell or files. From D4's first slice on, a sysusers file also creates the drawer's account `vtium` and the session processes' account `vtium-session`, each with no home, shell or files, and the package adds `vtium` to the group whose members may reach seatd's socket (D4, "The accounts").

> Note 2026-11-25: ADR-1304 D20, which the maintainer accepted on this date, adds `CacheDirectory=vtium` to vtium's units, so systemd makes `/var/cache/vtium` as it starts the service. The units run vtium as root, so the folder belongs to root, and the GPU driver's shader cache lives in a folder `gpu` inside it, which D4's broker makes for the account that draws. systemd also sets `CACHE_DIRECTORY` in vtium's environment, and vtium never passes that variable to the sessions it starts. The slice that builds the cache folder adds a touchpoint for it to docs/systemd.md, in its form with systemd, without systemd, and with elogind.

> Edited 2026-11-22: the maintainer decided seat mode's unit under the split, the accounts and seatd's group (`considered`, 2026-11-21, questions 2, 8 and 9). The bullet on the two modes ended at "so a VT runs one mode at a time;", and the bullet on the sysusers file ended at "with no home, shell or files."

> Edited 2026-11-10: the maintainer confirmed that the libseat build's unit opens its own PAM session and that vtium ships the PAM file (`considered`, 2026-11-10, card 1b question Q12). The bullet read: "in a `libseat` build, `User=root` and `PAMName` with the C reference's greeter PAM file." The file's name, `vtium`, is the agent's call, which the maintainer accepted on 2026-11-10.

> Edited 2026-11-11: the maintainer decided that one install offers both modes (`considered`, 2026-11-11; the note on the two modes below), and later that day the class `greeter` for vtium's session (the note on the session's class below). The bullet read: "in a `libseat` build, the unit sets `User=root` and `PAMName=vtium`, and vtium ships that PAM file, named `vtium`, with the session stack of the C reference's greeter PAM file. Through it the unit opens a logind session of vtium's own, and libseat's logind backend takes the seat from that session. The login child inherits that greeter session, and the helper treats it as no session of the login's own, as the C reference's did;"

> Note 2026-11-10: systemd refuses to load a unit that sets `PAMName=` together with `KillMode=process`, the value the VT unit sets so that sessions survive a restart (ADR-1401 D13). systemd 262 says "Kill mode must be set to 'control-group' or 'mixed'" (measured with `systemd-analyze verify`). So the libseat build's drop-in, `vtium@.service.d/libseat.conf`, sets `KillMode=mixed` beside `User=root` and `PAMName=vtium`, and the maintainer accepted that (`considered`, 2026-11-10, "Accept mixed (Recommended)"). systemd still signals vtium alone when the unit stops or restarts, and afterwards ends the unit's other processes. On that build, a login prompt nobody has used yet ends at a restart. The restarted vtium starts a fresh prompt as soon as it draws, so only a half-typed username is lost. A logged-in session is expected to survive, because `pam_systemd` moves it into a logind session scope of its own. That is not yet measured, and neither is whether `pam_systemd` moves vtium itself out of its unit; tier 3 owes both measurements.

> Note 2026-11-10, later: the survival of a logged-in session is measured. A shell was logged in on a vtium built with libseat, running under logind with the drop-in's `KillMode=mixed`, in a virtual machine. After a restart of vtium's unit, and again after vtium was killed with SIGKILL, a new vtium took the session back, and the shell answered a line typed in it. Whether `pam_systemd` moves vtium itself out of its unit is still not measured.

> Note 2026-11-10, later still: where the processes run is measured. On the same libseat build under logind, in a virtual machine, `/proc/<pid>/cgroup` puts vtium in `/user.slice/user-0.slice/session-c1.scope`: `pam_systemd` moves vtium out of `vtium@tty2.service` into the scope of its own PAM session. A shell then logged in on vtium's console runs in that same scope, `session-c1.scope` under root's slice, and not in a session of the user's own. This is the inheritance this decision describes, the login child running inside vtium's session, measured. A restart of vtium's unit keeps that shell because the shell is outside the unit's control group, whatever `KillMode=` says.

> Note 2026-11-10, on each login's own session: the maintainer chose a scope for each login over the other approaches the research weighed, and accepted that this costs more systemd units (`considered`, 2026-11-10, "I'm ok with just more instances in order to maintain scope per login"). In the libseat build under logind, vtium still holds the seat through its own PAM session, as the bullet above says. Each login child that vtium starts then runs in a systemd scope of its own: before the child runs `login`, vtium asks systemd's manager for a transient scope (`StartTransientUnit`) that holds the child, and waits for that job to finish. `login` then starts outside every logind session, so `pam_systemd` gives it a logind session of its own. That session has no seat. logind keeps one active session per seat, and vtium's session holds seat0, so vtium removes `XDG_SEAT`, `XDG_VTNR` and every `XDG_SESSION_*` variable from the environment of every child it starts in this mode. A session that registered on seat0 with vtium's VT would become the seat's active session, and logind would pause vtium's devices. Because the session has no seat, polkit treats it as inactive, and udev's uaccess rules, which give the active session access to devices such as sound cards, do not apply to it. Logins by one user share that user's `user@.service`. Where systemd is not PID 1, or the scope call fails, vtium starts `login` as before, inside vtium's own session, and logs one line that says why.
>
> What is measured, and how. The research on 2026-11-10 read systemd's and libseat's source at pinned commits: logind decides that a process already has a session by its cgroup's unit alone, and a new session on the active VT takes the seat from the session that held it. A spike on the same day, in tier 3's virtual machine, started `login` in a transient scope through `systemd-run --scope` and removed `XDG_SEAT` and `XDG_VTNR`. The user then had a logind session of their own with no seat, `XDG_RUNTIME_DIR` was set, and `user@1000.service` ran. vtium kept the seat, the keyboard still typed into the shell, and the session survived a restart of vtium's unit and a SIGKILL of vtium. Not measured: polkit's and uaccess's treatment of the seatless session, which comes from the research's reading; two logins by one user sharing `user@.service`, which is systemd's design; and the fallback where systemd is not PID 1.

> Note 2026-11-11, on the two modes: the maintainer decided that one package offers both ways of running, picked for each VT when vtium is turned on there, in their words "why not 'vtium console enable --seat' and 'vtium console enable --direct' and 'vtium console enable' returns the type of system it likely is (offering an opinion on wether to run as seat or direct)" (`considered`, 2026-11-11). Until then, a build with `libseat` and `logind` chose seat mode when it was built: `make install` staged the drop-in `vtium@.service.d/libseat.conf`, with `User=root`, `PAMName=vtium` and `KillMode=mixed`, beside the PAM file. Those three lines now make `vtium-seat@.service`. `tools/vt/units.sh` generates it from `vtium@.service` and the lines' source, `crates/vtium/install/vtium-libseat.conf`, and no install stages the drop-in. Where the notes above say "the libseat build under logind", they mean seat mode.
>
> The two modes give a login different sessions. In direct mode, the login's logind session is on seat0 with its VT, so it is the seat's active session while its VT is shown. polkit then treats it as active, and udev's uaccess rules give it the seat's devices, such as sound cards. In seat mode, the login's session has no seat, so polkit treats it as inactive and uaccess gives it nothing (the note on each login's own session, above). Direct mode gives a login what a getty gives it. So the package's install turns direct mode on, and `vtium console enable` with no mode recommends it (ADR-1500 D7).
>
> In direct mode, vtium never takes the seat through logind. libseat's logind backend takes the seat from the session that `XDG_SESSION_ID` names. Without it, the backend uses the session of the calling process, and without that, the calling user's display session (`get_display_session`, libseat/backend/logind.c lines 609-642, seatd 92b1617c). A vtium that `vtium@.service` starts is in no session. So on a build with `libseat`, libseat would take control of root's display session, such as a root login on another VT. vtium therefore lets libseat use its logind backend only when vtium is in a logind session of its own: when `XDG_SESSION_ID` is set, or when `/proc/self/cgroup` puts vtium in a scope named `session-ID.scope`, which is how sd-login finds a process's session. Otherwise vtium logs one line and runs on the kernel's VT. This does not change seatd: a build with `libseat` still takes the seat from seatd wherever seatd answers.
>
> Edited 2026-11-11, later: a review of the branch found that a check made once at the start does not close the path. While `LIBSEAT_BACKEND` names no backend, libseat tries seatd, then logind, then its built-in seat manager on every open (libseat/libseat.c lines 65-75, seatd 92b1617c), so an open that a busy seatd refuses falls through to logind. So outside a session of its own vtium now sets `LIBSEAT_BACKEND=seatd` at its start, while it has one thread and after it has read the environment its sessions inherit, so no login sees the variable. Every open then goes to seatd alone, and where seatd does not run vtium runs on the kernel's VT. When `LIBSEAT_BACKEND` names logind outside a session, vtium opens nothing and runs on the kernel's VT. The paragraph read: "So vtium lets libseat use its logind backend only when vtium is in a logind session of its own [...]. Otherwise vtium logs one line and runs on the kernel's VT." 

> Note 2026-11-11, on the class of vtium's own session: the maintainer chose the class `greeter` for the session that seat mode opens, answering "Greeter (Recommended)" (`considered`, 2026-11-11). `vtium.pam` passes `class=greeter` to `pam_systemd`. With no class, `pam_systemd` gives a root session on a tty the class `user-early` (systemd 78ab4b1, `src/login/pam_systemd.c:1039`), and tier 3's guest showed that class on vtium's session. logind stops an idle session of a user class when `StopIdleSessionSec=` is set (`logind-session.h:62`), and it judges a tty session idle by the access time of its tty (`logind-session.c:1184-1218`). vtium never reads its tty, so its session would always look idle, and logind would stop vtium. `Restart=on-failure` and `OnFailure=` take that stop as a clean one, so the VT would be left with no console. At power-off, logind asks polkit for more when another user's session exists, and it does not count a greeter's session (`logind-shutdown.c:35-40`). The class has one cost: logind lets another session register on a greeter's VT (`logind-dbus.c:946-956`). vtium's claim keeps a display manager off its VTs by holding them. Tier 3's `login-session` check asserts that vtium's session has the class `greeter`. The PAM file carries `-session optional pam_elogind.so class=greeter` beside `-session optional pam_systemd.so class=greeter`, so the stack registers the session with elogind where elogind runs, and the `-` keeps PAM quiet about the module a machine lacks. Seat mode under elogind is untested.

> Note 2026-11-22, on seat mode under the split: the maintainer decided that once D4's second slice is built, seat mode's unit drops `PAMName=` and goes back to `KillMode=process`, and the broker opens the drawer's greeter session (`considered`, 2026-11-21, question 8). Two greeter sessions would otherwise sit on one VT: the unit's own, which root holds, and the drawer's, which the drawer needs because logind lets only a session's own user or root call `TakeControl` (D4, open question 2). The broker opens the drawer's session through the PAM file `vtium`, with the class `greeter`, and starts the drawer in it, so the note on the class above holds for the drawer's session. Without `PAMName=`, systemd accepts `KillMode=process` again (the note of 2026-11-10 above), so a restart of the unit signals the broker alone, as in direct mode. The broker then stops the drawer itself (D4, "Start, stop and exit"), because `pam_systemd` moves the drawer into its session's scope, out of the unit's control group. Each login starts in the unit's control group, outside every logind session. So `pam_systemd` may give each login a logind session of its own without the scope that the note on each login's own session describes. Tier 3 measures where the logins land, whether they still need the scope, and whether a logged-in session survives a restart of the unit, before the second slice changes the unit.

> Edited 2026-11-02: the units carry vtium's own names, after the maintainer's decision that kmscon is an independent product (ADR-1500). The first bullet named the fallback `kmsconvt-getty@%i`, and the second read: "the names tier 3 needs, `kmsconvt@.service` among them;".

> Edited 2026-10-31: slice S6 of the head-file wiring work sends the getty fallback through `kmsconvt-getty@%i`, which starts `getty@%i` unless the closed marker exists (ADR-1401 D9), a departure from the C reference's unit. The bullet read: "`Conflicts=getty@%i` and `OnFailure=getty@%i`;", and the Summary read: "The units match the C reference's: getty parity and no hardening directives".

### D4: a root broker, and drawing without root

vtium splits into a broker that keeps root and processes that draw without it (the maintainer, 2026-11-13, `considered`). The broker is a small process that does only the jobs that D1's table shows need root. The code that reads the sessions' output, keeps their terminals and draws them runs as an account without root. The split serves direct mode and seat mode (D3). It is built in slices, listed below, and only behind the cargo feature `split`, which the minspec build leaves out. In its final shape, each session's output is read by a process of its own.

This decision uses four names. The *broker* is the root process. The *drawer* is everything else that vtium does today: the heads, the input, the terminals and the drawing. In the final shape the drawer is two kinds of process. The *compositor* is one process that shows the heads on the displays. A *session process* reads one console's output and keeps that console's terminal. A session is what runs on one console: its pty, its `login` or its command, and the programs started from it (ADR-1401 D1).

The figure shows the split in direct mode, as the first slice builds it: the broker as root with the VT, DRM master and the input devices, the drawer as `vtium` reading every byte the sessions print, the helpers and the sessions, and the messages between them. It also shows what the third slice adds.

![The privilege split in direct mode. The broker runs as root. It holds the VT, sets and drops DRM master on its copy of each card, opens and revokes the input devices, makes each pty, starts login, the helpers and the drawer, keeps the pty masters in systemd's store, and runs the power keys. The drawer runs as the account vtium, with no capabilities and not dumpable. It holds every console's pty master, parses every byte the sessions print, keeps the terminals and draws them, through its copies of the cards and input devices. One SOCK_SEQPACKET socket joins the two: the drawer asks to open devices, take and let go of cards, switch VTs, put the VT's modes back, open and close sessions, send SIGWINCH, start and stop helpers, run power keys, and says ready; the broker answers with descriptors, the release, the devices back, pty masters, helper sockets, exits, blank, wake and stop. The sessions run as the user who logged in, joined to the drawer only by their pty. The image helper runs as the session's user and the font helper as nobody, each over a socket the broker passed to the drawer. A dashed box shows the third slice: one session process for each console, as vtium-session, beside a compositor that keeps the displays.](figures/ADR-1400-split.svg)

#### What the broker does

In direct mode, the broker serves the devices the way a seat manager does:

- **It owns the VT.** It opens the VT and calls `VT_SETMODE`, so the kernel sends it the switch signals. It sets `KDSETMODE` and `KDSKBMODE` at start, at each retake and at exit. It calls `VT_RELDISP` and `VT_ACTIVATE` when the drawer asks. When a display hand-over ends (ADR-1401 D16), it puts the VT's modes back when the drawer asks, as `Vt::restore` does today.
- **It opens the devices.** It opens the cards, the fbdev devices and the input devices, and it passes each descriptor to the drawer. It opens only a device node of those kinds that belongs to its seat, whatever path the drawer names. It keeps its own copy of each descriptor.
- **It holds DRM master for the drawer.** It sets master on its copy of a card when the drawer asks to take the card, and answers whether that worked. It drops master when the drawer lets the card go. The drawer asks for a card at each try of its wake-up, the slow retry included (ADR-1000, "Replies and policy"), and lets the card go after a failed wake-up and at a display hand-over (ADR-1401 D16). The drawer never sets or drops master itself. The kernel checks a caller's permission before it checks whether the file already holds master (`drm_setmaster_ioctl` in `drivers/gpu/drm/drm_auth.c`), so the drawer's own call fails even on a file that holds master. The gap-check read this in the source, and check 10 measures it.
- **It hands the devices over at each switch.** At a release it tells the drawer, waits for the drawer's answer within a bound, drops master on each card, revokes each input device and acknowledges the release. A drawer that does not answer within the bound loses the devices all the same, so a hung drawer cannot keep the VT. The bound is 10,000 ms, a constant in the code and not a key of vtium.conf, so a hung drawer holds up a switch away from vtium's VT for up to 10 s. At an acquire it sets master on each card again, opens each input device anew and passes the new descriptors, since a revoked descriptor never works again.
- **It makes each session's pty and starts `login` on it.** The drawer asks for a console's session by its head's name and its console's id. The request also carries what only the drawer knows: the console's grid and cell size, which the broker sets on the pty before the session starts, and two hints for the session's environment. One hint is the kind of display, which `TERM_SESSION_TYPE` names, and the other is whether the head shows 24-bit color, which decides `COLORTERM` (`Place` in `crates/vtium/src/child.rs`). The broker checks each against the values it allows. The broker makes the pty, starts `login` or the head's command on the pty's slave, and passes the pty's master to the drawer. The broker takes the command line from its own reading of vtium.conf, under ADR-1401 D10's ownership check, and never from the drawer.
- **It keeps the pty masters through a restart.** The broker is the unit's main process, so it sends the copies to systemd's store, or to the holder where systemd is absent (ADR-1401 D13). At the drawer's start, the broker names each console whose session an earlier vtium kept, and the drawer opens each of them as its head starts, as vtium in one process does. The broker hangs up at once a kept session whose head vtium.conf no longer names, and one kept under a name vtium does not write. The drawer asks the broker to hang up a kept session it has no place for, such as one that the console caps leave no room for.
- **It starts and stops the helpers.** The drawer asks for a console's image helper, or for the font helper. The broker chooses the user by D2's rules and by ADR-1200 D15 itself, and passes the helper's socket to the drawer. The drawer never names a user id. The drawer also asks the broker to stop a helper: at the image helper's 2,000 ms timeout, at an idle exit, and when the font helper must go. A helper runs as another account, so the drawer cannot signal it.
- **It runs the power keys.** It reboots, powers off or suspends the machine when the drawer asks, for a key that vtium.conf binds.
- **It reaps every process it starts.** It signals a session to hang up when the drawer closes a console, and it reports each exit to the drawer. When the drawer asks, it sends `SIGWINCH` to the foreground process group of a console's session, as ADR-1401 D13's recovery does after a restart, because the session's processes run as another account.
- **It passes `vtium blank` and `vtium wake` on.** Those commands signal the process that holds the VT open, which under the split is the broker. The broker sends the order to the drawer over the socket, and the drawer blanks or wakes its displays.
- **It makes vtium's run directory,** `/run/vtium/<tty>/` (ADR-1401 D10), and passes the drawer the descriptors of what the drawer writes there. It opens `/dev/kmsg` at its end and relays each new record to the drawer over a socket of type `SOCK_SEQPACKET`, one record a message. A descriptor of `/dev/kmsg` would let the drawer seek back to the oldest record the kernel keeps, which `kernel.dmesg_restrict` keeps from an account without root. The drawer cannot seek or write on its end of the socket, and the broker never waits on it, so a record the drawer has no room for is dropped.
- **It restores the VT when the drawer dies** (below).

In seat mode, the seat manager serves the cards, the input devices and the switch signals, through libseat. The broker keeps the jobs that start and end processes: `login`, the helpers, the power keys, reaping, the store of pty masters, the scope for each login and the run directory. The broker opens a greeter session through PAM for the drawer's account and starts the drawer in it, as display managers start their greeters, so that the session is the drawer's (open question 2, answered). The unit then opens no PAM session of its own (D3, the note on seat mode under the split). The seat manager makes `KDSETMODE` and `KDSKBMODE`: logind does so when the drawer takes control of its session, and seatd when it enables the drawer's seat (checks 1 and 2). So the broker does not make them in seat mode. The broker makes `VT_ACTIVATE` when the drawer asks, until a check covers switching VTs through libseat (open question 3, answered).

> Edited 2026-11-13: after checks 1 and 2, the maintainer decided that the broker stops making `KDSETMODE` and `KDSKBMODE` in seat mode and keeps `VT_ACTIVATE` (`considered`, 2026-11-13). The paragraph ended "The broker also makes the VT calls that `seat.rs` makes today, `KDSETMODE`, `KDSKBMODE` and `VT_ACTIVATE`, when the drawer asks. It stops making them only if checks 1 and 2 show that the seat manager makes them (open question 3, answered)."

> Note 2026-11-25, on the GPU driver's shader cache: ADR-1304 D20, which the maintainer accepted on this date, gives the broker one more job. At each start, the broker makes the folder `gpu` inside `/var/cache/vtium`, opened without following a symbolic link, owned by the account that draws and of mode 0700. Where systemd has not made `/var/cache/vtium` (D3), the broker makes it first, owned by root and of mode 0755. A `gpu` that is a symbolic link, or that is not a folder, is not used. When the account that draws changes, the broker empties `gpu` before it gives the folder to the new account, so no process reads an entry that another account wrote. The broker passes the driver's cache settings, `MESA_SHADER_CACHE_DIR` for Mesa and `__GL_SHADER_DISK_CACHE_PATH` for NVIDIA, only in the drawer's environment at its exec, and never in a session's. Before the drawer loads the driver, it checks that it can write `gpu`. When it cannot, vtium logs that once and turns the driver's disk cache off, as it does where the folder cannot be made. A vtium that runs as one process as root does these jobs itself, and `gpu` then belongs to root.

> Note 2026-11-25, on the login picture under the split: the steering session decided that the broker starts the image helper of each head's login picture itself, through a kind of helper request of its own that names only the head. The head's first login prompt waits for the picture (ADR-1100 D17), so the picture is asked for before any of the head's sessions opens, and no session can give the helper its account. The broker runs the helper as the pre-login account and gives it only bytes sent inline, as vtium in one process asks for the login picture. It sets the helper's limits from the login picture's own bounds, held to its own reading of `image-storage-limit`, and the drawer names none of them. The broker runs at most one such helper for each head, refuses a second, and stops it once a session of the head opens.

#### The device protocol

The broker and the drawer talk over one Unix socket of type `SOCK_SEQPACKET`, made with `socketpair` before the drawer starts. Each message has a fixed size, and the broker checks every field before it acts. Descriptors travel with the message as `SCM_RIGHTS`.

- The drawer sends: open a device by its path, close a device, take master on a card, let a card go, the release is done, switch to VT *n*, put the VT's modes back, open a console's session with its grid, cell size and two hints, close it, send `SIGWINCH` to a console's session, start a helper, stop a helper, run a power key, and ready.
- The broker sends: a device's descriptor, whether master on a card was taken, release the devices, the devices are back with new input descriptors, a console's pty master, a helper's socket, a process's exit, the drawer's end of the socket that relays the kernel's messages, the kernel's console font, a console whose session an earlier vtium kept, the end of those consoles, blank, wake, and stop.

> Edited 2026-11-22: the maintainer had the requests the code needs added to these lists before the first slice builds (`considered`, 2026-11-21, question 4), and `vtium blank` and `vtium wake` go through the broker (question 7). The drawer's list read: "open a device by its path, close a device, the release is done, switch to VT *n*, open a console's session, close it, start a helper, run a power key, and ready." The broker's list read: "a device's descriptor, release the devices, the devices are back with new input descriptors, a console's pty master, a helper's socket, a process's exit, and stop." "What the broker does" gained the bullets on DRM master and on `vtium blank`, the bound's value, the size and hints of a session, the stop of a helper, `SIGWINCH`, and putting the VT's modes back.

> Edited 2026-11-25: the first slice added two orders that the broker sends, and the broker's list gained them. One passes the descriptor of `/dev/kmsg` that the broker opens, since the drawer cannot open the device under `kernel.dmesg_restrict`. The other passes the kernel's console font in a sealed memory file. The broker reads the font from the VT while the VT is still in text mode, before it opens the devices. The protocol has no request for the bell, which writes to the VT that only the broker holds. So the drawer's bell is silent under the split until a bell request exists, and the drawer logs that once. The broker's list read: "a device's descriptor, whether master on a card was taken, release the devices, the devices are back with new input descriptors, a console's pty master, a helper's socket, a process's exit, blank, wake, and stop."

> Edited 2026-11-25: the broker relays the kernel's messages instead of passing its descriptor of `/dev/kmsg`. The privilege split's review found that the drawer could seek that descriptor back to the oldest record the kernel keeps, and so read the whole kernel log despite `kernel.dmesg_restrict`. The bullet on the run directory ended: "It opens `/dev/kmsg` and passes that descriptor too, if the check the build makes first shows a descriptor is enough." The broker's list named "the descriptor of `/dev/kmsg`" where it now names the drawer's end of the socket that relays the kernel's messages.

> Edited 2026-11-26: the broker's list gained a console whose session an earlier vtium kept, and the end of those consoles. The privilege split's review found that the drawer started with no kept sessions of its own, so it opened only the consoles it opens by default. A kept session on any other console ran on unseen, and the broker kept it through every later restart. A kept session whose head was gone from vtium.conf was never hung up. So the broker now names the kept consoles at the drawer's start, the drawer waits for the end of that list before its heads start, and the bullet on the pty masters says what each side does with them. The steering session decided the fix when it scheduled the fix round of the split's install work. The broker's list read: "a device's descriptor, whether master on a card was taken, release the devices, the devices are back with new input descriptors, a console's pty master, a helper's socket, a process's exit, the drawer's end of the socket that relays the kernel's messages, the kernel's console font, blank, wake, and stop." The bullet on the pty masters ended after its first sentence.

> Note 2026-11-24, on a message the broker refuses: the maintainer decided that the broker refuses such a message and keeps serving the drawer, since stopping the drawer would make a denial of service easier (`considered`, 2026-11-24). The broker acts on no part of a refused message, and closes any descriptors that came with it at once. An empty message is refused like any other. When the refused request's kind waits for an answer and its id is in range, the broker answers under that id with the error `EINVAL`, so the drawer stops waiting. The broker logs the first refusal in full. After that it writes at most one line a second, which counts the refusals since the last line, so a drawer that sends refused messages in a loop cannot fill the journal. A drawer that reads none of its answers is handled in the same spirit: the broker neither waits for it nor stops it. The broker's end of the socket never blocks. Orders that the socket cannot take wait in a queue of at most 64 in the broker, and while that queue is full the broker reads none of the drawer's requests. So the broker keeps handling `SIGTERM`, the stop bound and the VT's restore. A stop that cannot reach the drawer waits in the queue, and the stop bound and `SIGKILL` end the drawer, as at any stop.

The drawer speaks one device protocol in both modes. Its code reaches the devices through one interface with two backends: the broker's in direct mode, and libseat's in seat mode. `LibseatVt` in vtium-platform already takes its devices as passed descriptors with enable and disable messages, so it is the model for the broker's backend.

#### The accounts

The drawer runs as the system account `vtium`, which later becomes the compositor's account. The session processes of the third slice share the system account `vtium-session` (open question 1). vtium's sysusers file creates both accounts (D3). In seat mode the drawer reaches seatd's socket through seatd's group, so the package adds `vtium` to that group, and tier 3 checks that the drawer opens its seat through a distribution's own seatd unit (`considered`, 2026-11-21, questions 2 and 9).

#### When vtium runs as one process

vtium splits only when it starts as root and the drawer's account exists. A vtium started without root, as the tests and a run by hand start it, has nothing to drop, so it starts no broker and runs as one process, as today (question 3). A vtium started as root on a machine without the account `vtium`, such as a host without systemd-sysusers where nobody made the account, also runs as one process, as today, and logs one line that names the missing account (question 2). The first two slices use only `vtium`. From the third slice on, vtium needs both accounts to split, and runs as one process when either is missing.

> Note 2026-11-25, on seat managers before the second slice: the maintainer decided that until the second slice is built, a vtium that would take its seat from a seat manager runs as one process, as root, and that vtium must split fully on machines without a seat manager (`considered`, 2026-11-25). Under the first slice the broker holds the VT and opens the devices itself, so it cannot serve a drawer whose seat seatd or logind manages. Tier 3 found that a split vtium then took nothing from seatd or logind, and that in seat mode a login got no logind session of its own. So before it splits, vtium checks whether seatd listens on its socket, where libseat may try seatd, and whether vtium is in a logind session of its own while logind runs. When either holds, it logs one line and runs as one process. Everywhere else, the machines without a seat manager among them, vtium splits as this section says.

#### The drawer's setup

The broker starts the drawer with fork and then an exec of its own binary, as it starts `vtium hold` today. Before the exec, the child takes the setup that D2 gives a helper, in the same order: credentials resolved before the fork, `setgroups`, `setresgid` and `setresuid` to the account `vtium` and read back, the bounding set emptied, the ambient capabilities cleared, and `PR_SET_NO_NEW_PRIVS`. It also sets `PR_SET_PDEATHSIG` to `SIGKILL` after `setresuid`, because prctl(2) clears that setting at a change of credentials. After the exec, the drawer makes itself not dumpable with `PR_SET_DUMPABLE`, because the kernel makes a process dumpable again at an exec that leaves its ids equal. In the first and second slices the drawer has no seccomp filter and no landlock domain, because a filter that the GPU drivers' calls must pass needs more study than these slices give it (question 1). A study of selectable security profiles, kept out of the main program and chosen by hardware class or driver, weighs how such filters could come later: [docs/design-notes/security-profiles.md](../../design-notes/security-profiles.md). The third slice's session processes keep their seccomp filter and landlock domain, which this decision already sets.

> Note 2026-11-24, on the order of the drawer's setup: the list above reads "`setgroups`, `setresgid` and `setresuid` to the account `vtium` and read back, the bounding set emptied", and that order cannot work. A process empties its bounding set with `PR_CAPBSET_DROP`, which needs `CAP_SETPCAP`. Once `setresuid` has moved the child from root to the account `vtium`, the child no longer holds that capability, and the call fails with `EPERM`. So the drawer's setup empties the bounding set first, while the child is still root, as the helper's setup in `image_helper/start.rs` already does. The setup's order is: the bounding set emptied; `setgroups`, `setresgid` and `setresuid`; all six ids read back; `PR_SET_PDEATHSIG`, followed by a check that the broker is still the parent; the ambient, effective, permitted and inheritable capabilities cleared; and `PR_SET_NO_NEW_PRIVS`.

#### Start, stop and exit

- **Ready.** The broker sends `READY=1` to systemd once it holds the VT open, before it starts the drawer, as ADR-1500 D7 says (question 14). The drawer sends ready once it runs. The broker waits for that ready within a start bound, a constant in the code that the first slice sets. When the drawer exits or the bound passes first, the broker restores the VT, as below, and exits with a failure.
- **Stop.** When systemd stops or restarts the unit, it signals the broker alone, under `KillMode=process`. The broker sends the drawer stop and waits for the drawer to exit within a bound. A drawer still running at the bound gets `SIGKILL`. Then the broker restores the VT and exits. Under seat mode the drawer runs in its session's scope, outside the unit's control group, so only the broker's stop ends it (D3, the note on seat mode under the split).
- **Exit status.** The broker exits with the drawer's own exit status. A drawer that a signal killed counts as a failure, and the broker then exits with a failure status (question 12). So a drawer's clean exit is a clean exit of the unit, and ADR-1401 D9 and D13 judge a failure as they do today.

> Note 2026-11-25, on a hangup of the VT: under the split the broker holds the VT, so the broker sees its hangup and stops the drawer, which then exits with status 0. The broker exits with a failure after a hangup whatever the drawer's status, as vtium in one process exits with status 1 at a hangup. So under the closed policy systemd restarts vtium after a hangup, and no getty takes the VT (ADR-1401 D9). Tier 3's `heads-closed-broken` found that the broker passed the drawer's 0 on, after which systemd left the VT with no console. The steering session decided the fix when it scheduled the work that fixes the split's regressions on a packaged build.

#### vtium.conf under the split

The drawer reads vtium.conf itself, as vtium does today. The broker reads it too, for what the broker decides on its own: the login command, the power keys that vtium.conf binds, and the closed policy. A vtium.conf that root owns with mode 0600 passes ADR-1401 D10's ownership check, but the account `vtium` cannot read it. `vtium check-config` warns when the drawer's account cannot read vtium.conf (question 13).

> Added 2026-11-22: the five sections above follow the maintainer's answers to the gap-check's questions 1, 2, 3, 9, 12, 13 and 14 (`considered`, 2026-11-21). The stop's bound and the start bound are constants the first slice sets, and the agent added the stop to cover the unit's `KillMode=process`.

#### One process for each session

The maintainer chose one process for each session over one drawer for all of them (`considered`, 2026-11-13). The reason is that a session process that crafted output has taken over must not reach another session's terminal, such as a root shell or a password typed at a login prompt. A head shares its displays among its consoles, and every head shares the keyboards, so the split divides the drawer itself, not only its privilege:

- **A session process** holds its console's pty master. It parses the console's output, keeps the console's terminal (the screen, the scrollback, the modes and the keyboard flags) and answers the console's replies under the reply rule. It keeps the console's picture store and talks to the console's image helper. It turns each key it receives into the bytes its terminal's modes ask for. It holds nothing of another console.
- **The compositor** holds the heads: the displays, the input devices and the keymaps, which console each key goes to, the chords, the notices and the on-screen display, the selection, the fonts and the font helper, the glyph caches, and each head's render thread and GPU path.

What crosses from a session process to the compositor:

- the screen as a frame shows it, which is what `Snapshot` in `crates/vtium/src/term.rs` holds today: the visible lines with their styles, links and clusters, the damage, the palette, the cursor, reverse video, the selection, the hovered link, the pictures' placements and the order of each right-to-left row;
- when to present, under mode 2026 and coalescing;
- each picture's pixels, as a sealed memfd that the compositor maps read-only;
- the requests that the head or vtium answers: the bell, an OSC 52 selection, an OSC 50 font, an XTWINOPS 8 grid and the blank sequences;
- a link's address or the selected text, when the compositor asks to copy it.

What crosses from the compositor to a session process: each key after the keymap, with its text, its modifiers and whether it is pressed, repeated or released; pointer events, in cells and in pixels; pasted text; focus; whether the console is shown; the console's grid and cell size; scrolling of the view; and a request to copy.

The compositor checks each message from a session process the way the main process checks a helper's glyphs and pictures today (D2, ADR-1200 D15). Each line fits the grid the compositor set, each cluster and each address stays within its bound, each picture's size and seals are checked before it is mapped, and each id is one the compositor knows. The code that reads these messages runs under fuzzing, as the output parser does. The screen messages are much narrower than the byte stream a program prints, so a defect that a session can reach in the compositor is less likely than one in the parser.

`Snapshot` shows that the boundary already exists in the code. It is owned, and the head's render thread reads it while the session runs on, so the screen already reaches the drawing as a value and not as shared state.

The broker starts each session process, with the same setup as a helper (D2): no capabilities, `PR_SET_NO_NEW_PRIVS`, a seccomp filter and a landlock domain of its own, and not dumpable. It passes the process the console's pty master and the process's end of a socket to the compositor. A session process cannot open another console's pty. The broker makes every pty, so each pty's slave belongs to root until `login` gives the slave to the user who logs in. The session processes share one account, and the compositor runs as another (open question 1, answered).

#### What the drawer can no longer do

- open or read a file as root;
- signal a process, other than through the broker's jobs;
- change the VT's mode or keyboard mode, or switch the VT, except by asking the broker;
- read input after its VT is released, because the broker revokes the input devices;
- keep DRM master after a release, because the broker drops it;
- choose a user id, or a command that runs as root.

A session process also cannot reach another console's pty, screen or pictures.

#### When the drawer dies

The broker sees the drawer's socket close or the drawer's exit. It then sets the VT back to text mode with `KDSETMODE`, sets `VT_AUTO` with `VT_SETMODE` and puts back the keyboard mode it saved at start. It drops master on each card and revokes each input device. The broker has to do this itself. The kernel's own reset of a VT (`reset_vc` in `vt_ioctl.c`) sets text mode and `VT_AUTO`, but it leaves the keyboard mode as it was. Today vtium's panic hook (`crates/vtium/src/hook.rs`) restores the VT, and a crash that kills the process outright skips the hook. Under the split, the broker restores the VT after any end of the drawer. The broker then exits with the drawer's own exit status, and with a failure when a signal killed the drawer, so ADR-1401 D9 and D13 decide what follows, as they do today (open question 4, answered; "Start, stop and exit" above).

> Edited 2026-11-22: the maintainer decided the broker's exit status (`considered`, 2026-11-21, question 12). The sentence read: "The broker then exits with a failure, so ADR-1401 D9 and D13 decide what follows, as they do today (open question 4, answered)."

That is direct mode. In seat mode the seat manager restores the VT when the drawer's session loses its controller or its seat closes. It sets text mode and the keyboard mode `K_UNICODE`, whatever mode the VT had before (checks 1 and 2). vtium accepts `K_UNICODE` there, and the broker does not put back the saved keyboard mode under a seat manager (the maintainer, `considered`, 2026-11-13).

> Added 2026-11-13: the paragraph on seat mode follows the maintainer's answer to checks 1 and 2.

When the broker dies, the drawer loses its socket and exits. The broker's own panic hook restores the VT, as vtium's does today. A drawer that hangs would keep DRM master after the broker dies, because master belongs to the file description and the drawer's copy stays open. So the kernel kills the drawer when the broker dies, through `PR_SET_PDEATHSIG` ("The drawer's setup" above).

> Edited 2026-11-22: the paragraph on the broker's death gained its last two sentences, from the drawer's setup that the maintainer decided (`considered`, 2026-11-21, question 1).

When a session process dies, only its console loses its screen and scrollback. The broker still holds the pty's master, so it starts a new session process on that master. The new process shows an empty screen and sends a resize, as ADR-1401 D13's recovery does, and the console's respawn count bounds how often this happens (open question 5, answered).

#### The limits that remain

- Until session processes exist, the drawer holds every session's pty master. A drawer that crafted output has taken over can type into a root shell on another console, and it can read a password typed at a login prompt. Until then the split protects the machine from the parser, and it does not protect one session from another.
- On a kernel older than Linux 6.12, session processes never exist, so this first limit stays for good. `docs/concepts/security.md` says what the split protects there once the split is built.

> Added 2026-11-13: the limit for a kernel older than Linux 6.12 follows the maintainer's answer to check 8 (`considered`, 2026-11-13).
- The compositor receives every key and shows every screen. A defect that a session process can reach through the screen messages reaches every session.
- The broker runs as root and reads the drawer's messages, so a defect there runs as root. The broker stays small, its messages have a fixed size, and the code that reads them runs under fuzzing.
- `login`, the helpers' change of user, the power keys and reaping still need root.
- A drawer that crafted output has taken over can ask for anything the broker offers. It can switch VTs, open and close consoles within the caps, start helpers, and run the power keys that vtium.conf binds, which reboot or power off the machine. Anyone at a keyboard that reaches the console can do these already.
- Every session can put text into the selection, and the user may paste that text into another session, as `clipboard-write` allows today. The selection stays one for the whole of vtium (open question 6, answered).
- The unit still sets no sandboxing directive, because the login session would inherit it (D3).
- A framebuffer's descriptor cannot be revoked, so the broker cannot take `/dev/fbN` back from the drawer at a release. On a machine with only fbdev, a drawer that crafted output has taken over can go on writing to the framebuffer after a switch, over the next VT's screen.
- The broker looks up the image helper's account, with `getpwuid_r` and `getgrouplist`, in its loop. Where the user database is on the network, as with LDAP or SSSD, a slow answer stalls the broker's loop. The release bound stalls with it, so a switch away from vtium's VT can wait for the answer.

> Added 2026-11-25: the privilege split's review found the last two limits in the first slice's code. A framebuffer has no call like `EVIOCREVOKE`, so the broker lets `/dev/fbN` go at a release without touching the drawer's copy. The account lookup runs at each start of an image helper.

#### The checks the build makes first

The kernel check could not settle these. The first slice checks each one before it builds on it, in the tier 3 virtual machine where a check needs a VT or a card:

1. What logind does to the VT when a session takes control of it: whether `TakeControl` changes the VT's owner, and whether logind sets `K_OFF` or `KD_GRAPHICS`. *Measured 2026-11-13:* Yes, to both questions. When the session's own account calls `TakeControl`, logind makes that account the owner of the tty's file, and keeps the file's group and mode. logind sets `KD_GRAPHICS`, `K_OFF` and `VT_PROCESS`, with the switch signals sent to logind itself. It does the same for a greeter session of a system account, of type tty or wayland, and for root's session. When the controller calls `ReleaseControl`, logind sets `KD_TEXT`, `K_UNICODE` and `VT_AUTO`, and gives the tty's file back to root. When the controller exits without calling it, logind does the same within 100 ms. logind sets `K_UNICODE` whatever mode the VT had before, so a VT that was in `K_XLATE` comes back in `K_UNICODE`. `TakeDevice` hands the controller the card with master.
2. What seatd does to the VT. *Measured 2026-11-13:* When seatd enables a client's seat, it sets `KD_GRAPHICS`, `K_OFF` and `VT_PROCESS`, with the switch signals sent to seatd. It leaves root as the tty's owner. When the client closes the seat, seatd sets `KD_TEXT` and `K_UNICODE`, whatever mode the VT had before. seatd leaves the VT in `VT_PROCESS`, and the VT stays so after seatd stops. A switch to another VT still works after that. seatd hands the client the card with master.
3. How libseat's built-in backend keeps its privilege. *Measured 2026-11-13:* The backend keeps its privilege in a process of its own. `libseat_open_seat` forks, and the child runs seatd's server with the caller's credentials until the seat closes. The check measured a libseat 0.9.3 built with the backend. Its child ran as root with every capability. The caller then dropped root and still opened the card through the seat, with master. The child exited when the seat closed. The VT calls are seatd's, as in check 2. seatd's build leaves the backend out by default (`libseat-builtin`), and Arch's libseat lacks it: `LIBSEAT_BACKEND=builtin` fails there with "No backend matched name 'builtin'".
4. Whether util-linux's `login` refuses to run without root, from its source. *Measured 2026-11-13:* Yes. util-linux is under the GPL, so the check runs `login` instead of reading its source. Run without root on a pty that its account owns, `login` exits with status 1 and writes nothing to the pty. It logs "change permissions failed: Operation not permitted" for the pty. Run with no tty, it logs "bad tty". Run as root on the same kind of pty, `login -f tester` logs tester in.
5. polkit's default answers for `StartTransientUnit`, and for logind's power methods, when a system account calls them. *Measured 2026-11-13:* Arch installs no polkit by default. Without polkit, logind and systemd refuse a system account's `CanPowerOff`, `CanReboot`, `CanSuspend` and `StartTransientUnit` with AccessDenied, also inside the account's active greeter session. They answer root's session "yes", and they start root's unit. polkit 128 sets these defaults. logind's power-off, reboot and suspend actions are `auth_admin_keep` for any session and for an inactive one, and `yes` for an active one. systemd's manage-units action is `auth_admin`, `auth_admin` and `auth_admin_keep`. With polkit, the account's power questions answer "challenge" outside a session and "yes" inside its active greeter session. polkit refuses `StartTransientUnit` in both, because the call needs interactive authentication.
6. Whether a greeter session of a system account can call `TakeControl`. *Measured 2026-11-13:* Yes. A root process opens a session for a system account through vtium's own PAM file, `/etc/pam.d/vtium`. logind registers the session on seat0 and its VT, with the class greeter, and makes it active. The account's process calls `TakeControl`, then `TakeDevice` for the card and for the keyboard, and each call succeeds. So open question 2's answer works.
7. Whether a process without root can read `/dev/kmsg` through a descriptor that root opened, under `kernel.dmesg_restrict=1`. *Measured 2026-11-13:* Yes. With `kernel.dmesg_restrict` at 1, a process without root and with no capabilities reads the kernel's records through a descriptor of `/dev/kmsg` that root opened. The same process cannot open `/dev/kmsg` itself or call `syslog(2)`: each fails with EPERM.
8. Whether processes of one account are kept apart: that a process that is not dumpable cannot be traced, have its `/proc/<pid>/fd` read or a descriptor taken with `pidfd_getfd` by another process of its account, and that landlock's scopes keep signals inside a domain. *Measured 2026-11-13:* Yes, with landlock's scopes, on landlock ABI 10. Another process of the same account cannot trace a process that is not dumpable. It cannot read the process's `/proc/<pid>/fd` or `environ`, or take a descriptor with `pidfd_getfd`. That holds for the process's parent too, with Yama's ptrace scope at 1 or at 0. Not being dumpable does not stop a signal, or a connection to the process's abstract unix socket. Two processes, each in a landlock domain of its own that scopes signals and abstract unix sockets, can do neither to each other. Such a process cannot signal its parent outside the domain either, and a process outside the domain can still signal into it. Between two domains, landlock also stops tracing and `pidfd_getfd` for a process that is dumpable. These scopes need landlock ABI 6, which came with Linux 6.12.
9. That the drawer draws through its copy of a card's descriptor, with dumb buffers and page flips, after the broker sets master on the broker's copy. *Measured 2026-11-13:* Yes. A root process opens the card, sets master on its copy, and passes a copy over a socket with `SCM_RIGHTS`. A process without root cannot open the card itself. Through its copy, it sets a mode with a dumb buffer and makes 10 page flips, each waited for by its event. When root drops master on its copy, the other copy loses master, and a page flip fails with EACCES. The process without root cannot set master on its copy either. When root sets master again, the page flips work again.
10. That the drawer's own `DRM_IOCTL_SET_MASTER` fails with `EACCES` while its copy of the card already holds master, because `drm_setmaster_ioctl` checks the caller's permission before it checks whether the file is master. *Measured 2026-11-25:* Yes. While its copy of the card holds the master that root set on root's copy, the process without root gets `EACCES` from both `DRM_IOCTL_SET_MASTER` and `DRM_IOCTL_DROP_MASTER`. Its copy still holds master after each call, and a page flip through it still works. So the drawer cannot drop master itself either, and the broker drops it at each release. `make tier3-rootchecks` measured it on Linux 7.2.11, in check 9's run.

> Added 2026-11-22: check 10 comes from the gap-check's reading of `drm_auth.c`, which the bullet on DRM master in "What the broker does" rests on.

> Edited 2026-11-25: the first slice measured check 10. Its answer read: "*Not yet measured:* the gap-check read this in the source of Linux 7.2.14, and check 9 measured the call only after root had dropped master. The first slice measures it, since the drawer's wake-up must then ask the broker for master in every case."

On 2026-11-13 `make tier3-rootchecks` measured every check in the tier 3 guest, and each answer above gives what it found. The guest runs Linux 7.2.11, systemd 262.1, seatd 0.9.3, util-linux 2.42.4, PAM 1.7.3 and dbus-broker 37, from the Arch Linux Archive snapshot of 2026-10-20. polkit 128 comes from the same snapshot. The probes run as root and as a system account with no capabilities. `tools/tier3/README.md` says how to run the checks again, and the answers are in `target/tier3/rootchecks/answers.txt`.

#### The slices

1. **The broker in direct mode.** The broker does every job listed above, and one drawer holds every head and every console, as vtium does today. Tests show that the drawer's `/proc/<pid>/status` names the account `vtium` with no effective and no bounding capabilities, `NoNewPrivs: 1`, and that the drawer is not dumpable. They also show that a VT switch drops master and revokes the input devices, and that a SIGKILL of the drawer leaves the VT in text mode with its keyboard mode restored.
2. **Seat mode.** The drawer takes its devices from the seat manager, and the broker keeps the jobs that start and end processes, as open questions 2 and 3 settle it.
3. **A process for each session.** The drawer splits into the compositor and one session process for each console, over the messages above. This slice needs Linux 6.12 or later: landlock ABI 6, with `LANDLOCK_SCOPE_SIGNAL` and `LANDLOCK_SCOPE_ABSTRACT_UNIX_SOCKET` in each session process's domain. Without those scopes, a session process could signal another, SIGKILL included, and connect to its abstract unix sockets, since not being dumpable stops neither (check 8). On an older kernel vtium keeps one drawer for all sessions, as the first slice builds it (the maintainer, `considered`, 2026-11-13). The third slice also keeps each console's terminal state through a restart of vtium: every mode in the terminal's mode table, the keyboard protocol's flags, the cursor's visibility and style, and which screen is active. A restart does not keep the screen's contents, the margins, the tab stops, the character sets, the title or the palette (`considered`, 2026-11-21, question 11). A program's redraw after the restart brings back most of those, and the restart resizes the grid anyway. A restarted vtium then sends a focus-in to each shown console whose kept modes have mode 1004 on, so a program that reports focus learns that it has it (question 10). The kept state has to outlive both the broker and the drawer, and the broker never parses it, so where the state waits during a restart is the third slice's design.

> Edited 2026-11-13: after check 8, the maintainer decided that the third slice needs Linux 6.12. The item ended at "over the messages above."

> Edited 2026-11-22: the maintainer put the work that keeps the terminals' modes through a restart into the third slice, decided which state it keeps, and decided the focus-in after a restart (`considered`, 2026-11-21, questions 10 and 11). The third item ended at "as the first slice builds it (the maintainer, `considered`, 2026-11-13)." The first item's tests read: "Tests show that the drawer's `/proc/<pid>/status` names an account other than root with no capabilities, that a VT switch drops master and revokes the input devices, and that a SIGKILL of the drawer leaves the VT in text mode with its keyboard mode restored."

> Note 2026-11-25, on the build: the maintainer decided that the split builds only behind a cargo feature, `split`, which the minspec build leaves out (`considered`, 2026-11-25). The split's code took about 32 KB of the minspec binary, although the split was still switched off, and left the binary 56 bytes under tier 3's budget (ADR-1200 D19). A build without the feature runs vtium as one process, as before the split. It carries no broker, no VT through the broker, no holder of the devices, no keeper of DRM master and no start of a helper through the broker, and it does not link the protocol's crate. The packaged builds and `make vt-accept`'s default features name `split`. D4's first paragraph read: "It is built in slices, listed below."

The slices are shaped so that the third does not undo the first:

- From the first slice on, the broker makes every pty and names it by its console. The third slice changes only which process receives the master.
- From the first slice on, the broker starts every process vtium runs. A session process is one more kind it starts, with the setup it already gives a helper.
- Before the third slice, the code that becomes a session process (the terminal, the pty and the picture store) and the code that becomes the compositor (the heads, the views and the painter) come to talk only through the values that the third slice sends over a socket: `Snapshot` and the requests one way, the input and the console's size the other. That change to the drawer's code is a card of its own, built beside the second slice. It holds no privilege code, and it would collide with the first slice's wiring in the same files (`considered`, 2026-11-21, question 6).
- The broker never reads the screen, so the third slice adds no root code beyond starting session processes.

> Edited 2026-11-22: the maintainer made the change to the drawer's code its own card, beside the second slice (`considered`, 2026-11-21, question 6). The list began "The first slice is shaped so that the third does not undo it:", and its third bullet read: "Inside the first slice's drawer, the code that becomes a session process (the terminal, the pty and the picture store) and the code that becomes the compositor (the heads, the views and the painter) talk only through the values that the third slice sends over a socket: `Snapshot` and the requests one way, the input and the console's size the other."

#### What the other records change when the slices land

- ADR-1401 D2 says that a display is a head inside vtium's one process, never a second process. That stays true of the compositor. Once the third slice lands, each console's session runs in a process of its own.
- ADR-1401 D10 says that a crash a program provokes with crafted output ends every session. Once the third slice lands, such a crash ends that console's session process, unless it reaches the compositor.
- ADR-1401 D13: the broker keeps the store of pty masters, as the unit's main process. Once the third slice lands, a restart also keeps each console's terminal state, as the third slice says.
- ADR-1401 D16: until the third slice lands, a restart sends no focus-in, and a program turns mode 1004 on again after a restart. Once the third slice lands, a restarted vtium sends a focus-in to each shown console whose kept modes have mode 1004 on. D16's bullet on the end of a hand-over carries this, with a dated edit.
- ADR-1101 and ADR-1200 D15: the broker starts each helper, at the drawer's request.
- `docs/concepts/security.md` and SECURITY.md change as each slice lands.

> Edited 2026-11-22: the bullets on ADR-1401 D13's kept state and on ADR-1401 D16 follow the maintainer's answers to questions 10 and 11 (`considered`, 2026-11-21). The bullet on D13 ended at "as the unit's main process."

#### Open questions

No open question remains. The maintainer answered question 1 and then questions 2 to 6 on 2026-11-13, taking the lean each time (`considered`, 2026-11-13). Each answer is written into the section it settles, and the questions stay below as they were put.

1. **Which account each process runs as.** *Answered 2026-11-13: (a), by the maintainer (`considered`, 2026-11-13).* (a) One system account for the compositor, and one shared by every session process, each session process not dumpable and in a landlock domain of its own. (b) A user id for each session process, from a range the broker reserves. (c) A session process runs as the user who logged in on its console. The lean is (a). A process that is not dumpable cannot be traced by another process of its account (ptrace(2), "Ptrace access mode checking"), and landlock limits tracing between domains from Linux 5.13 and signals from Linux 6.12 (the kernel's landlock documentation; check 8 confirms both). Option (b) needs a reserved range of user ids, which a machine without systemd's sysusers has no standard way to make. Option (c) has no user before login, and it gives a defect the user's files.
2. **How the drawer gets a session the seat manager serves.** *Answered 2026-11-13: (a), by the maintainer (`considered`, 2026-11-13).* (a) The broker opens a greeter session through PAM for the drawer's account and starts the drawer in it, as display managers start their greeters. (b) The unit keeps its root session, the broker takes the seat through libseat, and the broker passes the devices to the drawer over the direct mode protocol. The lean is (a), because it is what the maintainer described: the seat manager serves the devices and the broker keeps the process jobs. logind lets only the session's own user or root call `TakeControl` (org.freedesktop.login1(5), systemd 262), so the session must be the drawer's. Option (b) keeps libseat and D-Bus in the root process. Check 6 decides whether (a) works.
3. **Who makes the VT calls in seat mode.** *Answered 2026-11-13: (a), by the maintainer (`considered`, 2026-11-13). After checks 1 and 2 showed that logind and seatd make `KDSETMODE` and `KDSKBMODE`, the maintainer decided that the seat manager makes those two, as (c) says, and that the broker keeps `VT_ACTIVATE` until a check covers switching VTs through libseat (`considered`, 2026-11-13).* (a) The drawer asks the broker for `KDSETMODE`, `KDSKBMODE` and `VT_ACTIVATE`, as in direct mode. (b) The drawer makes the VT its controlling terminal with `setsid` and `TIOCSCTTY`, as Xorg does when it runs without root. (c) Nobody makes them, if the seat manager already does. The lean is (a) until checks 1 and 2 show that (c) holds. The VT calls need the controlling terminal or `CAP_SYS_TTY_CONFIG` (D1's table). Option (b) splits the VT's jobs between two processes, and the broker still restores the VT after a crash.
4. **What the broker does after the drawer dies.** *Answered 2026-11-13: (a), by the maintainer (`considered`, 2026-11-13).* (a) It restores the VT and exits with a failure, so ADR-1401 D9 and D13 decide what follows, as they do today. (b) It starts a new drawer itself. The lean is (a). D9's getty fallback and D13's store already decide what a failure does under each policy, and (b) would add a restart policy, and a loop counter, to the root process.
5. **What happens to a session when its session process dies.** *Answered 2026-11-13: (a), by the maintainer (`considered`, 2026-11-13).* (a) The broker starts a new session process on the same pty master. That process shows an empty screen and sends a resize, as D13's recovery does, and the console's respawn count bounds how often this happens. (b) The broker hangs the session up. The lean is (a). It keeps the user's programs running, as a restart of vtium does today (ADR-1401 D13 items 4 and 5). The respawn count stops a program whose output crashes its session process again and again (`respawn` in `crates/vtium/src/head/console.rs`).
6. **Whether the selection stays shared.** *Answered 2026-11-13: (a), by the maintainer (`considered`, 2026-11-13).* (a) One selection for the whole of vtium, as today. (b) One selection for each head. (c) One for each console. The lean is (a). A program can write the selection but never read it (security.md), and its text reaches another session only when the user pastes it there. Option (c) stops copying from one console to another.

### What the stream reaches beyond the screen

| Channel | What it reaches | Control | Issue |
|---|---|---|---|
| Replies | the program reading the pty | the reply rule (docs/architecture.md, "Determinism") and the reply queue's bound | the terminal's modes and replies; the pty and event loop |
| Kitty file and shared-memory media | the user's files | the helper, as the session's user (D2) | the image helper and its sandbox |
| OSC 50 font paths | the user's files | the helper, with the same identity and path rules, as kmscon ADR-103 planned | runtime font changes |
| OSC 52 write | the clipboard | the OSC string bound; reads refused (ADR-1000) | the OSC string bound and OSC 52 copy-out |
| setBackground | the display | behind `allow-display-ops`, on by default; only a console a head shows; on the kernel's VT only, ignored under a seat manager (ADR-1401 D16) | display hand-back |

### Where vtium departs, and where the gaps go

- **The login child's descriptors:** the C reference relied on `O_CLOEXEC` and closed nothing explicitly. A runtime test checks that the child holds only fds 0-2 after exec while the spawner holds others open.
- **The helper controls that the image helper's issue does not yet list** become its gates, each with a test: from D2, `setsid`, the ids read back, the ambient capabilities cleared, `TMPDIR` as the only environment, the credentials resolved before the fork, the restart rules, and the seals checked before the size.
- **`t=s` unlinks:** any program writing to the pty can make the helper unlink any shared-memory object the session's user owns, once it is read (recorded in the C fork's issue on a seccomp or landlock filter for the image helper). The image helper's issue accepts this on the record or narrows which names are unlinked, and tests its choice.
- **OSC 50 font paths** go through the helper, as the C reference planned and never built.
  - *Note (2026-10-28):* ADR-1200 changes what OSC 50 reaches: it switches only among vitrified sets already built into the root-owned store, behind `allow-font-ops`, and never names a font path, so no path goes through the helper for it. ADR-1200's D15 reuses this helper for another job: on any build, it generates glyphs no built set holds from the frit, as the session's user in this sandbox, and hands back glyph chunks the main process checks for bounds and dimensions only, as it does decoded images. The helper exits after an idle interval (ADR-1301 D2, D2 above). *Edited 2026-10-28:* the operator reversed ADR-1200's ban on generating at run time on minspec; this read "on a build that isn't minspec, it rasterises a size a set lacks".

> Added 2026-10-29: the configuration file's `login` key chooses the command each session's login child runs as root, and vtium reads that file with no ownership check today: `crates/vtium/src/config/mod.rs` refuses only a path that is not a regular file. ADR-1401 D10 reads the head file, the profiles and the frits only when root owns each and neither group nor others can write it, as `sshd`'s `StrictModes` does, so a head's own `login` key takes that check. The file that holds the configuration's `login` key needs the same check; whether it gets it, and what a file that fails it does, is an open question.

> Added 2026-10-29: the operator decided that the file holding the `login` key takes the head file's ownership check of ADR-1401 D10 (`considered`).

## Consequences

### Positive

- vtium keeps a model the C reference's reviews already tested, with no new privilege code to get wrong.
- Every layer runs from a plain unit, an init script or a hand launch; systemd, logind and libseat add to it and never gate it.
- The login session is the one getty gives: sudo, root's console, the user's home and `/tmp` behave as under getty.
- The minspec build links no libsystemd and no libseat.
- Under D4, a defect in the parser, the screen or the renderer no longer runs as root, from the first slice on. From the third slice on, it reaches only its own console's session.
- Under D4, the broker restores the VT after any end of the drawer, including a SIGKILL that today skips the panic hook.

### Negative

- Until D4's first slice is built, a defect in the parser, screen, renderer or input path runs as root.
- systemd's sandboxing goes unused.
- A root console login on a system without logind gets no file media, as in the C reference.
- D4 adds privilege code: the broker, and the protocol it reads from the drawer. Each VT switch costs a round trip between the broker and the drawer, and a hung drawer holds up a switch away for up to 10 s.
- Under D4, a machine that lacks the drawer's account gets no split, and vtium there runs as root for life, as D1 says.
- D4's third slice adds a process and a socket for each console, and the screen crosses between processes for each frame.

### Neutral

- SECURITY.md's scope, "vtium runs as root on a virtual console", stays true until D4's first slice is built.

> Edited 2026-11-22: the Negative bullet on privilege code gained the release bound's cost, and the bullet on a missing account is new, from the maintainer's answers to questions 2 and 5 (`considered`, 2026-11-21).

> Edited 2026-11-13: D4 adds the split's consequences. The first Negative bullet read "A defect in the parser, screen, renderer or input path runs as root.", and the Neutral bullet ended at "stays true".

## Alternatives Considered

- **A privilege split in one binary:** a root parent that keeps the login spawn, the VT, DRM master and the helper's identity switch, passing descriptors to an unprivileged terminal process, as OpenSSH separates privilege. Not adopted: the operator chose the C reference's flow over building something novel before its architecture is understood. Reopened when a defect in the code that reads the stream is shown to run code as root, or when the operator asks for it. *Adopted 2026-11-13 as D4,* when the maintainer asked for it, with one drawing process for each session in the end.
- **One drawer for every session.** Not adopted (the maintainer, 2026-11-13): a drawer that crafted output has taken over would hold every session's pty master, a root shell's and a login prompt's included. It remains D4's first two slices on the way to a process for each session.
- **The drawer holds the VT as its controlling terminal**, so that it makes the VT calls without a capability, as Xorg does when it runs without root. Not adopted for direct mode: the drawer still cannot set or drop DRM master on a card that another process opened, or that it opened while a desktop held master, so the broker must handle master anyway. The broker also still needs `CAP_SYS_TTY_CONFIG` to put back the keyboard mode after a crash. It was weighed for seat mode's VT calls too, and the maintainer chose the broker for them (D4, open question 3).
- **Unprivileged only through a seat.** Not adopted: it needs a seat daemon, and `/bin/login` still needs root. The kernel check of 2026-11-13 adds that `login`, the helpers, the power keys and reaping need a root process in seat mode too, and that direct mode, the default install, would keep root for life.
- **Unit sandboxing, `ProtectSystem=strict` and `NoNewPrivileges` among it.** Rejected: the login session inherits it (D3).
- **Reading logind's state under `/run/systemd` in place of sd-login.** Rejected: systemd does not document those files as an interface (external, not re-verified).
