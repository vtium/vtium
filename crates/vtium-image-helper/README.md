# vtium-image-helper

`vtium-image`, the image helper (ADR-1101): the program vtium starts as the session's user to read the files and shared-memory objects a program names, and to decode PNG, zlib and the formats iTerm2's pictures arrive in, for the console, which runs as root and does neither (ADR-1100 D7). It is its own binary, outside the minspec build, installed as `/usr/bin/vtium-image`. vtium opens it once when it starts and starts every helper from that open file, as vtium-platform's `image_helper` module describes; this crate is the program's side.

| Path | What it is |
|---|---|
| `src/main.rs` | The program: its `--tmpdir` argument, the socket on descriptor 3, the check that it has one thread, no-new-privileges, the seccomp filter, the hello, then the loop. |
| `src/serve.rs` | The helper's loop: one request at a time, a one-off worker forked for each, the long-lived stream worker and its pool of reply files for a stream of inline frames, each worker's checks of its own sandbox, the removal of a shared-memory object that held a valid picture, and the reply. |
| `src/job.rs` | One request as a worker carries it out: `open` before the worker locks itself in, `finish` after it; the bounded read, the inflate or decode, the conversion and the reply file written and sealed. |
| `src/files.rs` | The file rules: the refused places, the temporary-file rule, the regular-file open through a path-only descriptor and `/proc/self/fd`, shared-memory names, and the removal of only the file that was checked. |
| `src/inflate.rs` | zlib through `miniz_oxide`, inflated as a stream that stops at its bound. |
| `src/decode.rs` | PNG through `png`, JPEG through `zune-jpeg` and a GIF's first frame through `gif`, each bounded by its header before a pixel is allocated, and the first bytes that choose among them. |
| `src/sandbox.rs` | The helper's seccomp allow-list and its conditions, the worker's second filter, the third filter for kernels without landlock, the worker's landlock domain, and the counts and probe a worker checks its sandbox with. |
| `src/channel.rs` | Packets with descriptors on a socket pair, and the outcome a worker sends the helper. |
| `src/fork.rs` | The one module that holds `unsafe`: taking descriptor 3, asking whether a descriptor is open, and forking a worker. A reviewer of any change to it audits it line by line. |
| `src/bounds.rs` | The helper's named constants. |
| `tests/helper.rs` | The gate of ADR-1101 D9: the real program, started by vtium-platform's real host under its filter, sent every kind of request, each reply checked, so a worker that finds its sandbox wrong fails it; a stream of inline frames served by one stream worker, and frames from shared memory served by one-off workers; a larger budget replacing the helper. |
| `tests/filter.rs` | The filters in a child: calls outside the list answered ENOSYS; a worker that cannot start a process, make a memory file or, under its third filter, open a file; a worker locked in that reads what it holds and opens nothing; and the attacks on a `sleep` the test starts that the filter's conditions and the domain's scope stop, a descriptor's signals pointed at it and its limits changed, with executable memory refused. |
| `tests/jpeg.rs`, `tests/fixtures/jpeg/` | Two JPEGs that panicked `zune-jpeg` 0.4.21, from the fuzz target, each decoded to a picture or a refusal; the fixtures' README says where they came from. |
| `examples/streams.rs` | The measurement of ADR-1101 D4's three ways to serve a stream, from which the maintainer chose the stream worker. |
| `fuzz/` | A fuzz target for each decoding path, in its own workspace ([fuzz/README.md](fuzz/README.md)). |

`cargo test -p vtium-image-helper` runs every test, the real program included.

## How a request runs

The helper takes a request from vtium on descriptor 3: one packet in the wire vtium-image's `decode::wire` encodes, with a sealed memory file for inline bytes. It exits on a request that `decode_request` refuses, as the C reference's helper did on one it would not build. For each request that is no frame of a stream it makes a socket pair and the reply's memory file and forks a one-off worker. The worker checks that vtium's socket is closed in it, adds its second seccomp filter and checks that it runs under one more filter than the helper, opens what the request names under the file rules, removes a temporary file, enters its landlock domain, or on a kernel without landlock adds its third filter, and checks that it can no longer open a directory. It then reads, inflates or decodes, converts the picture into the store's premultiplied format through vtium-image's one conversion, writes the reply file with `pwrite` and seals it against shrinking, growing and writing, and sends its outcome. The helper replies as soon as the outcome is in and reaps the worker after. A worker killed by a signal, or one that ends without an outcome, as a worker that finds its sandbox wrong does, makes the helper exit, which vtium counts as a crash.

Every crate that reads a program's bytes is pure Rust, approved by the maintainer for this program only (2026-11-01): `png`, which inflates through `fdeflate` and pulls in `flate2` and `miniz_oxide` for its encoder alone; `miniz_oxide`; `zune-jpeg` on `zune-core`; `gif` on `weezl`; and `landlock` and `seccompiler` for the sandbox. The lockfile holds `flate2` at 1.1.9, the last that builds on `miniz_oxide` 0.8, so one inflater is in the tree.

## The sandbox

vtium starts the helper with an empty environment, no new privileges, no capabilities, its memory cap, the largest file it may write set to the budget, a session of its own and only descriptors 0 to 3, as the session's user (ADR-1101 D2, D3). Before it reads a request the helper installs its seccomp allow-list, and any call outside it answers ENOSYS, so a C library that tries a newer call falls back to an older one. Some calls are allowed with some arguments only: `socketpair` for Unix sockets, `prctl` for no-new-privileges, `seccomp` for installing a filter, `prlimit64` for the calling process itself, `mmap` and `mprotect` without `PROT_EXEC`, and `fcntl` for reading and setting a descriptor's flags and status flags, never `O_ASYNC`, for seals and for a close-on-exec duplicate. A worker taken over therefore cannot point a descriptor's signals at another process with `F_SETOWN`, change another process's limits, or make memory executable. `seccompiler` builds filters for `x86_64`, `aarch64` and `riscv64` only, so the armv7 helper installs none and says so in its hello, which vtium logs once a boot; the maintainer accepted landlock alone on armv7 (2026-11-02).

Each worker adds a second filter that answers forking, starting a program and making a memory file with EPERM, and enters a landlock domain once it holds the descriptor it reads. The domain handles every filesystem right the kernel's landlock knows and grants none, refuses binding and connecting TCP sockets from landlock version 4, and from version 6 is scoped against signalling any process outside it and connecting to an abstract Unix socket made outside it. On a kernel without landlock the worker adds a third filter instead, which refuses `open`, `openat`, `openat2` and `unlinkat`, and the hello tells vtium that landlock is missing. On armv7 the domain is the only layer, so a worker there on a kernel without landlock version 6 can still signal its account's processes.

Each worker checks its own sandbox before it reads anything and exits without an outcome when the check fails: vtium's socket, descriptor 3, is closed; it runs under one more seccomp filter than the helper, counted from `/proc/self/status`; and once locked in, it cannot open `/`, wherever the kernel offers landlock or the third filter is in force. Each outcome also carries the worker's count of its filters and whether it could open `/` as it answered, probed apart from the lock, and the helper exits as on a crash when either is not what it expects, so a worker whose lock or second filter was skipped altogether fails too. `tests/helper.rs` therefore fails when any of these controls is removed. The checks catch a mistake in vtium; a worker taken over could lie.

The file-size cap bounds each memory file the helper and its workers fill, which the memory cap does not count, and workers cannot make memory files, so a worker taken over can fill only the reply file it was handed. A helper parent taken over could make memory files bounded only by its descriptor limit, but the parent reads no picture's bytes, only vtium's requests and its workers' outcomes.

## Streams, and the measurement

ADR-1101 D4 names three ways to serve a program that re-sends a picture for each frame: a fresh worker and reply file for each frame (1); a fresh worker for each frame writing into a small pool of reply files faulted in once (2); and one long-lived worker for the stream (3).

`examples/streams.rs` measured all three on 2026-11-02 on the maintainer's desktop, an AMD Ryzen 9 9950X3D on Linux 7.2.14, through the real request path with the worker's filter and landlock domain, over 40 frames a case at 1366x768 and 20 at 7680x2160. The figures are the median milliseconds a frame:

| Size | Frames | (1) fresh | (2) pool | (3) stream | (2) over (3) |
|---|---|---|---|---|---|
| 1366x768 | raw `f=32` from shared memory | 2.78 | 2.49 | (not served) | |
| 1366x768 | inline `f=32,o=z` | 13.35 | 13.27 | 11.82 | 12% |
| 1366x768 | inline PNG | 7.45 | 7.21 | 5.71 | 26% |
| 7680x2160 | raw `f=32` from shared memory | 35.35 | 39.02 | (not served) | |
| 7680x2160 | inline `f=32,o=z` | 202.64 | 194.83 | 189.12 | 3% |
| 7680x2160 | inline PNG | 97.53 | 92.35 | 88.82 | 4% |

Over four runs the pool came within 2 to 6 percent of the stream worker at 7680x2160 and within 11 to 14 percent on zlib frames at 1366x768, and 23 to 26 percent over it on PNG frames at 1366x768, about 1.5 ms a frame: the fresh memory each new worker faults in for its decoder. The maintainer chose the stream worker for inline frames, accepting that a hostile frame can reach the later frames of its own stream: "if someone wants to break the parser, that's on them" (2026-11-02). Frames from shared memory gained nothing from the pool at 7680x2160, so they take a fresh worker and a fresh reply file each. The ARM board and tier 3's guest, which ADR-1101 D4 also names, are not measured yet.

Once `STREAM_AFTER` inline requests in a row carry the same flags and picture size, the helper forks the stream worker and keeps `REPLY_POOL_FILES` reply files of that size, sealed against shrinking and growing only; the stream worker writes each later frame into the next one, the reply says the file is pooled, and the console reads it with `pread`. The pool's size comes from the width and height a worker reported, so the helper makes it only within the request's largest width, height and `max_bytes`. A frame that does not fit the pooled file, because it decodes to more or fewer bytes, writes nothing, ends the stream and runs again as a one-off with a fresh file; a frame of another width and height but the same bytes fills the pooled file and replies its own size. The stream ends on a request that does not match, before that request is served, on any outcome but a picture, and after `STREAM_IDLE_MS` with no request. The helper then closes the stream worker's channel and reaps it; a stream worker that does not exit holds the helper until vtium's reply timer stops it.

## Left to the tier 3 work

ADR-1101 D10's test of the pre-login refusal end to end, through a terminal, its pty and the real helper, needs the real console, and the tier 3 work carries it. This crate's gates test the helper's side, and vtium-platform's tests test the refusal of file media before login.

## Departures

From the C reference, beyond those ADR-1101 lists:

- **A discard of shared memory removes nothing.** The kitty handler still sends a discard for a refused `t=s` command, as tier 1's goldens record, and the helper answers it OK and leaves the object, since ADR-1101 D5 removes an object only after a valid picture was read from it. The C reference removed it.
- **A shared-memory name is at most 255 bytes without its leading `/`,** the kernel's longest file name in `/dev/shm`, where the C reference counted the slash.
- **The worker's second and third filters, its checks of its own sandbox, the reply's no-execute seal and the stream worker** are vtium's own, as ADR-1101 records.

## Bounds

| Bound | Value | What it bounds | Source |
|---|---|---|---|
| `HELPER_IDLE_MS` | 60,000 | Milliseconds the helper waits for a request before it exits | ADR-1301 D2's idle interval, the font helper's |
| `STREAM_AFTER` | 3 | Matching requests in a row that make a stream | ADR-1101 D4 |
| `STREAM_IDLE_MS` | 1,000 | Milliseconds with no request after which a stream ends | ADR-1101 D4 |
| `REPLY_POOL_FILES` | 2 | Pooled reply files a stream keeps | ADR-1101 D4 |
| `SHM_NAME_MAX` | 255 | Bytes of a shared-memory name without its leading `/` | The kernel's `NAME_MAX` |
| `PNG_WORK_MAX` | 64 MiB | Bytes the PNG decoder may allocate beside the picture, below the request's `max_bytes` | The png crate's default |
