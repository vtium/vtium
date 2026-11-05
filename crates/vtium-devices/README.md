# vtium-devices

This crate describes each device present the way the rules of vtium.conf see it. vtium-platform reads what udev, sysfs, the device and the KMS connector say about a display or an input device. vtium-settings's rules match an `Identity`, the description of a device that the rules read. This crate turns the first into the second (ADR-1401 D4), so that every program that places a device places it the same way (ADR-1403 D9).

Three programs use it. The console's start maps each display and input it opens. `vtium check-config` maps every device that vtium-platform lists for the seat, without opening any of them. vtium-config's Flow view and its `flow` and `route` commands place each device present through the same functions (`src/flowmodel.rs`), and take the udev daemon's control socket from `UDEV_CONTROL`.

The crate depends on vtium-platform and vtium-settings, which both binaries already link, and on nothing else. It forbids unsafe code and denies every panic that a device's facts could reach.

| Path | What it is |
|---|---|
| `src/lib.rs` | The crate's documentation, lints and public names. |
| `src/identity.rs` | `display` and `unnamed_display`, a display with its `Card` and its place among the card's displays; `input`, an input device with its udev classes, path, USB serial, Bluetooth address, ids, name and udev properties; and the EDID's maker, product code and serial as an `edid:` rule matches them. |
| `src/listing.rs` | `SEAT`, the seat whose devices are listed; `devices`, a listing of that seat's devices mapped as the start maps them; `UDEV_CONTROL` and `udev_daemon_runs`, whether a udev daemon runs to fill the database that some rule keys read. |

`cargo test -p vtium-devices` runs the crate's tests. `vtium check-config`'s tests in `crates/vtium/src/check.rs` and the console's head tests in `crates/vtium/tests/heads.rs` also run through these functions.
