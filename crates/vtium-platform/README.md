# vtium-platform

vtium's platform layer: everything that touches the kernel and the session. This crate now holds the pty and the event loop. The displays (vtium/vtium#18) and the VT, udev and keyboard input (vtium/vtium#19) come next and go into the same loop. Syscalls go through rustix (vtium/vtium#6), so descriptors are owned values and errors are `Result`s. The crate denies `unsafe_code`, and each site that needs it carries an `#[expect]` with its reason and a SAFETY comment. The pty and the event loop face the child's output, so the crate also denies the determinism lints vtium-parse denies.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation, lints and public names. |
| `src/event_loop.rs` | `Loop`: calloop with one signalfd for every signal the console handles, and its handle for descriptors and timers. |
| `src/pty.rs` | `Pty`: opening the pty, the child's session, the window size, writes to the child, the child's exit, and the pty's place in the loop. |
| `src/channel.rs` | The pty's two directions as state with no event loop: reads fed to the `Consumer` in parts it chooses, the reply stall, and the queue's writes. Its tests drive it against a scripted child. |
| `src/queue.rs` | The bytes the child has not read, in runs by kind, with replies counted and pointer motion coalesced. |
| `src/bounds.rs` | Every bound, with its value, what happens past it and its source: the one source for the constants and for the table below. |
| `tests/pty.rs` | The pty and the loop through a real pty, with children that are the test binary run again in a role. It has its own `main`, so the thread that blocks the loop's signals is the only one. |

`cargo test -p vtium-platform` runs every test. The bounds table below is generated from `src/bounds.rs`, and `readme_table_matches_bounds` fails when it drifts; `VTIUM_PLATFORM_BLESS=1 cargo test -p vtium-platform readme_table` rewrites it.

## Behaviour

The C reference's pty (`src/misc/pty.c`) is the floor. vtium opens the pty, starts the child as the leader of a new session with the pty as its controlling terminal, sets `VERASE` and the window size, reads the child's output edge-triggered at most `READS_PER_TURN` reads a turn, and lets the consumer take part of what is read, holding the rest for later turns so the loop serves its other sources in between. A pause stops reading and feeding until the resume, as the C reference's did for an image decode. What the user sends (keys, pastes, button reports) is queued whole and never stalls the reading. A pty closed and opened again starts with nothing queued: the queue belongs to the `Pty`, which is dropped on close.

The child is watched through a pidfd, so SIGCHLD keeps its default disposition and the loop handles no signal for it. Signals the console handles go through one signalfd in `Loop`; the C reference's eloop used a signalfd the same way.

What the consumer is fed, and every reply it writes, depend only on the bytes the child sent (docs/architecture.md, "Determinism"). A stall delays them and drops nothing, as long as the consumer stops once `Replies::full` is true. `src/channel/tests.rs` holds the chunking-invariance property test, with random read sizes, random takes, a child that reads at random and pauses at random. `tests/pty.rs` repeats it through a real pty.

## Departures

- **The consumer stops at the reply that fills the queue, and no reply is dropped.** Once replies the child has not read reach `REPLY_HIGH`, `Replies::full` is true and the consumer returns after the sequence whose reply reached it; nothing more is read or fed until the replies drain below `REPLY_LOW`. The C reference set the same stall after its input callback returned, so the callback parsed the whole read and could pass `REPLY_MAX`, dropping replies, and whether it did depended on how reads split the stream (pty.c:474-517, 579-618). Spec: docs/architecture.md, "Determinism", chunking invariance. `REPLY_MAX` stays as a ceiling for a consumer that does not stop.
- **Pointer motion is coalesced for a child that does not read.** From `MOTION_COALESCE_AT` bytes queued, a motion report replaces the motion reports queued after the last other write; button reports, keys and replies are queued whole and keep their order. The C reference queued every motion report, so a child that enabled any-motion tracking (mode 1003) and never read grew the queue for as long as the pointer moved (aaronsb/kmscon#247). Spec: vtium/vtium#20.
- **The child's exit reaches the consumer after the output it left.** The exit is reported once a read finds nothing more, so a program's last output is fed before its session ends; replies written after the exit are dropped and never stall the reading. The C reference reported the exit from SIGCHLD and closed the pty with the output still unread (pty.c:737-768, 830-848), so how much of it the screen showed depended on timing. Spec: docs/architecture.md, "Determinism", chunking invariance.

## Dependencies

- **rustix** for every syscall (vtium/vtium#6), with its `runtime` feature for the one call it keeps there: the empty signal mask the pty's child starts with.
- **calloop**, the event loop ADR-304 chose, pinned at 0.14.4: 0.14.5 needs Rust 1.86, and the workspace's `rust-version` is 1.85. Move to 0.14.5 or later when the workspace moves to 1.86.
- **nix** arrives through calloop's `signals` feature, which gives the loop its signalfd. rustix has no signalfd wrapper, and the alternatives are a `libc` call or a raw syscall in `unsafe`, which vtium/vtium#6 excludes. vtium-platform calls nothing in nix. Revisit when rustix wraps signalfd.

## Left to the host

- **The child's command and environment** are the host's `Command`: `TERM`, `COLORTERM`, `XDG_SEAT`, `XDG_VTNR`, `TERM_SESSION_TYPE`, the environment reset and `/bin/login` as the default (pty.c:252-302). That is the binary's (vtium/vtium#21).
- **The respawn policy**: a new session after an exit, oneshot, and the C reference's limit of 5 restarts within 2 seconds (pty.c:52-53, 737-768). That limit reads the clock, so it belongs where the binary decides what an exit means, not in the loop (vtium/vtium#21).

## Notes for the display, VT and input work

- Each device descriptor (the DRM device, the udev monitor, each evdev device, fbdev) goes into the loop through `Loop::handle` as a calloop `Generic` source. Timers (key repeat, the render timer, the decode timeout and mode 2026's timeout) are calloop `Timer` sources. The time those timers read stays in the places docs/architecture.md, "Determinism", names.
- The VT's release and acquire signals (`VT_PROCESS`) go through `Loop::on_signal`, which adds them to the loop's one signalfd. Add every signal before any thread starts: a thread inherits the mask of the thread that starts it, and a thread without the mask could take the signal instead of the loop.
- An ioctl rustix does not wrap is defined through rustix's `ioctl` module, one definition per ioctl in one module, with a test that its request number equals the kernel header's (vtium/vtium#6).

## Bounds

The child's output comes from an untrusted program, so nothing it sends grows memory past these. Keys and pastes are queued whole, which only the user can grow.

<!-- bounds table: generated from src/bounds.rs -->
| Bound | Value | What it bounds | Past it | Source |
|---|---|---|---|---|
| `READ_MAX` | 16384 | Bytes one read takes from the pty | The rest stays in the kernel for the next read | `KMSCON_NREAD`, pty.c:50 |
| `READS_PER_TURN` | 50 | Reads one readiness event makes before the loop serves its other sources | The pty's registration is renewed, so the next turn reads on | pty.c:627-651 |
| `REPLY_HIGH` | 65536 | Replies queued for the child, past which its output is no longer read | The consumer stops after the reply that reached it, and nothing more is read or fed until the replies drain below `REPLY_LOW`, so a child that queries without reading stalls itself | `PTY_REPLY_HIGH`, pty.c:68; aaronsb/kmscon#245 |
| `REPLY_LOW` | 16384 | Replies queued for the child below which reading resumes after a stall | Reading and feeding resume | `PTY_REPLY_LOW`, pty.c:69 |
| `REPLY_MAX` | 262144 | Replies queued for the child, a ceiling only a consumer that does not stop at `REPLY_HIGH` reaches | The reply that would pass it and every later one are dropped until the replies drain below `REPLY_LOW` | `PTY_REPLY_MAX`, pty.c:70, 479-484 |
| `MOTION_COALESCE_AT` | 4096 | Bytes queued for the child from which pointer motion reports are coalesced | A motion report replaces the unsent motion reports at the queue's end, so the child reads the latest position; keys, pastes and button reports are queued whole and never dropped. The C reference queued every motion report (aaronsb/kmscon#247) | vtium/vtium#20 |
<!-- end of bounds table -->
