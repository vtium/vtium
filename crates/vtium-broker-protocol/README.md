# vtium-broker-protocol

This crate holds the messages between vtium's broker and its drawer, and the socket that carries them. In the privilege split, the broker keeps root and the drawer runs everything else as the system account `vtium`. The drawer asks the broker for devices, ptys and helpers, and the broker answers with orders and descriptors. The broker in the vtium crate and the drawer both speak this protocol, so it lives in a crate that each side can depend on.

Every message is 128 bytes. The reader checks each field before it hands a message on. It refuses the whole message for an unknown kind, a field out of range, or a byte set past the fields. A wrong size or a descriptor where none belongs is refused too. The encoding is canonical: a message the reader accepts encodes back to the same bytes. Both ends share one `SOCK_SEQPACKET` socket, which keeps each message whole, and a descriptor travels with its message as `SCM_RIGHTS`.

Only the broker sends descriptors. A refused request hands back the descriptors that came with it, still open, because closing a file can block. For example, the last close of a TCP socket with `SO_LINGER` set waits out its linger time. The broker closes them on a thread of its own, so a drawer cannot hang the broker's loop that way. On Linux 6.16 and later, the broker turns off `SO_PASSRIGHTS` on its end of the socket, and the kernel then refuses any descriptor the drawer sends.

The crate forbids unsafe code and depends on std and rustix alone. The broker runs as root and parses what the drawer sends, so no message may panic it. The crate denies the lints that find such a panic: unchecked arithmetic, lossy casts, unchecked indexing, and `unwrap`, `expect`, `panic!`, `unreachable!` and `todo!`.

| Path | What it is |
|---|---|
| `src/lib.rs` | The bounds; the field types `Head`, `DevicePath`, `ConsoleRef`, `Grid`, `Display`, `Helper`, `PowerKey`, `Status` and `Errno`; the drawer's `Request` and the broker's `Order`, with their encoders and readers; `Refusal` and `Got`, what a read gives; and `Link`, the socket that sends and receives them. The unit tests are at the end of the file. |
| `src/courier.rs` | `Courier`, the drawer's one reader of its socket. A request that waits for its answer blocks in `Courier::call` for at most `CALL_BOUND_MS` of quiet. Every other order read meanwhile waits in a queue, with its descriptor, in the order it came, and `Courier::next` hands the queued orders on before it reads the socket again. It tells the socket's close once, after the queued orders, and calls the loop's waker when a call leaves orders queued. `Courier::wait` waits the same way for an order that no request asks for, such as the end of the kept consoles that the broker names at the drawer's start. |
| `fuzz/` | The fuzz target `protocol`, which feeds any input to both readers. Its README says how to run it. |

The feature `test-support` adds `Link::wait_for_close`, which the broker's tests in the vtium crate use. Only a dev-dependency turns it on.

| Bound | Value | What happens past it |
|---|---|---|
| `SIZE` | 128 bytes, every message | A message of another size is refused. |
| `PATH_MAX` | 63 bytes, a device path under `/dev/` | The request is refused. |
| `NAME_MAX` | 64 characters, a head name, as vtium.conf bounds a `[head NAME]` section | The request is refused. |
| `VT_MAX` | VT 63, the kernel's highest; the lowest is 1 | The message is refused. |
| `CELL_MAX` | 1024 pixels, a character cell's width or height | The request is refused. |
| `SIGNAL_MAX` | signal 64, Linux's highest | The order is refused. |
| `ERRNO_MAX` | error 4095, Linux's highest | The order is refused. |

A console's grid takes 1 to 65535 columns and rows. `cargo test -p vtium-broker-protocol` runs the crate's tests, and the broker's tests in the vtium crate exercise the protocol over a real socket against stand-in drawers.

References:
- ADR-1400 D4, "The device protocol": the requests and orders, the fixed size, the checks before the broker acts, and `SCM_RIGHTS`.
- ADR-1513 D4: why the protocol is a crate of its own.
