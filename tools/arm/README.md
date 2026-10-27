# arm

vtium built and tested for the ARM single-board computers minspec is judged against ([docs/architecture.md](../../docs/architecture.md#the-minspec-build)): the minspec profile for `aarch64-unknown-linux-gnu` and `armv7-unknown-linux-gnueabihf`, and the workspace's tests under qemu-user, on an x86 host with no cross toolchain and no root.

## Running it

```sh
make arm-sysroot   # fetch and verify the Debian sysroots, once per pin
make arm           # the minspec profile for both targets, in target/TRIPLE/minspec/
make arm-test      # the workspace's tests for aarch64 under qemu-user
make arm-test ARM_TEST_TARGETS=armv7-unknown-linux-gnueabihf
```

`ARM_TARGETS` names the targets `make arm` builds, `ARM_TEST_TARGETS` those `make arm-test` runs, and `VTIUM_ARM_SYSROOTS=DIR` the sysroot cache (default `$XDG_CACHE_HOME/vtium/arm`), which every vtium checkout shares.

Host needs: the targets' standard libraries (`rustup target add aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf`), `clang`, `qemu-aarch64` and `qemu-arm` from qemu-user, `pkg-config`, `curl`, `ar`, `tar`, `xz` and `sha256sum`, and `gpg` for `sysroot pin`. The tests need Linux 6.7 or later with unprivileged user namespaces, or a host binfmt_misc entry for qemu (below).

## How it works

| Piece | What it does |
|---|---|
| `sysroot fetch ARCH` | Downloads the packages `pin.tsv` names for a Debian architecture (`arm64`, `armhf`), checks each against its pinned SHA-256, and extracts them into `VTIUM_ARM_SYSROOTS/ARCH`. A sysroot whose pins differ from `pin.tsv`'s is extracted again. |
| `TRIPLE-cc` | The linker: the host's `clang --target` with the sysroot, linking through rustc's own `lld`. |
| `TRIPLE-pkg-config` | pkg-config reading the sysroot's `.pc` files, so `libudev-sys` finds the target's libudev. |
| `TRIPLE-run` | The test runner: the binary under `qemu-aarch64` or `qemu-arm`, its libraries from the sysroot. |

The three `TRIPLE-*` names are links to `wrap`, which reads its own name. The Makefile names them for these targets alone, in `arm` and `arm-test`, so a native build on an ARM machine uses that machine's toolchain.

A test binary under qemu starts other target binaries, as `crates/vtium/tests` start vtium, and the kernel hands those to qemu only through binfmt_misc. When the host registers no qemu for the target, `TRIPLE-run` mounts a binfmt_misc of its own in a user namespace, registers qemu there, and runs the binary in a nested namespace mapped back to the invoking user and group. Nothing on the host changes. Without user namespaces it runs qemu directly and says so, and the tests that start a target binary fail.

The sysroot is `/usr`-merged and carries an empty loader cache. qemu-user finds an `open()` in the sysroot but not a `statx()`, so a loader search that missed in `/lib/TRIPLE` would mark that directory absent; and the host's loader cache names 32-bit host libraries the armv7 loader would take.

## What is pinned

`pin.tsv` names the archive, the key and every package:

| Row | Pin |
|---|---|
| `snapshot` | `snapshot.debian.org/archive/debian/20261020T000000Z`, which never changes |
| `suite` | `bookworm`: glibc 2.36, so the binaries run on bookworm and later |
| `key` | the Debian archive key that signs the suite's `InRelease`, primary fingerprint `04B54C3CDCA79751B16BC6B5225629DF75B188BD` |
| `deb` | each package per architecture, with its version, pool path and SHA-256: libc6, libc6-dev, linux-libc-dev, libgcc-s1, libgcc-12-dev, libudev1, libudev-dev, libxkbcommon0, libxkbcommon-dev |

`tools/arm/sysroot pin` resolves the `deb` rows again from the snapshot: it verifies `InRelease` against the pinned key, fetched into a keyring beside the sysroots and never the user's, each architecture's `Packages.xz` against `InRelease`, and reads each package's version, path and SHA-256 from `Packages`. A change of snapshot or package goes in a commit of its own.

## What qemu-user does not judge

qemu-user runs vtium's code on an emulated CPU over the host's kernel, so it judges word size, alignment, `c_char` signedness and the target's ABI. It does not judge:

- **Speed.** A test's time under emulation says nothing about a Cortex-A53.
- **Device ioctls qemu does not translate,** such as DRM's `GET_CAP`: qemu answers `ENOTTY`, so a test on the host's render node fails under qemu while it passes natively.
- **Tracing.** The host's `strace` on a target binary traces qemu.

`TRIPLE-run` exports `VTIUM_QEMU_USER`, naming the qemu it runs, and the tests that depend on these skip when it is set and print why: the render-node test in vtium-platform's `display::card` and check-config's open check. CI's `arm-native` job runs the whole suite natively on an aarch64 runner. The runner has no render node, so the DRM test skips there; a real ARM machine judges it.

## The musl targets

They are not built. vtium links libudev and libxkbcommon, which Debian builds for glibc only, and the musl targets link statically by default; building them needs a musl sysroot from another distribution with static or musl-dynamic builds of both libraries.
