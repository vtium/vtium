# arm

vtium built and tested for the ARM single-board computers minspec is judged against ([docs/architecture.md](../../docs/architecture.md#the-minspec-build)): the minspec profile for `aarch64-unknown-linux-gnu` and `armv7-unknown-linux-gnueabihf`, and the workspace's tests under qemu-user, on an x86 host with no cross toolchain and no root.

## Running it

```sh
make arm-sysroot   # fetch and verify the Debian sysroots, once per set of pins
make arm           # the minspec profile for both targets, in target/TRIPLE/minspec/
make arm-test      # the workspace's tests for aarch64 under qemu-user
make arm-test ARM_TEST_TARGETS=armv7-unknown-linux-gnueabihf
```

`ARM_TARGETS` names the targets `make arm` builds, `ARM_TEST_TARGETS` those `make arm-test` runs, and `VTIUM_ARM_SYSROOTS=DIR` the sysroot cache (default `$XDG_CACHE_HOME/vtium/arm`), which every vtium checkout shares.

Host needs: the targets' standard libraries (`rustup target add aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf`), `clang`, `qemu-aarch64` and `qemu-arm` from qemu-user, `pkg-config`, `curl`, `ar`, `tar`, `xz` and `sha256sum`, and `gpg` for `sysroot pin`. The tests need Linux 6.7 or later with unprivileged user namespaces, or a host binfmt_misc entry for qemu (below).

## How it works

| Piece | What it does |
|---|---|
| `sysroot fetch ARCH` | Downloads the packages `pin.tsv` names for a Debian architecture (`arm64`, `armhf`, and `amd64` for the x86_64 build) into `VTIUM_ARM_SYSROOTS/debs`, from the archives [Where the packages come from](#where-the-packages-come-from) lists, and extracts them into `VTIUM_ARM_SYSROOTS/ARCH-ID`, where ID is a digest of the architecture's pin rows and of `sysroot` itself. A download enters `debs` only once its SHA-256 matches its pin, and each extraction reads a private copy it has checked again. Absolute symlinks are rewritten as relative ones inside the sysroot, and a member or link that would leave it stops the fetch. |
| `TRIPLE-cc` | The linker: the host's `clang --target` with the sysroot, linking through rustc's own `lld`. |
| `TRIPLE-pkg-config` | pkg-config reading the sysroot's `.pc` files, so `libudev-sys` finds the target's libudev. |
| `TRIPLE-run` | The test runner: the binary under `qemu-aarch64` or `qemu-arm`, its libraries from the sysroot. |

The three `TRIPLE-*` names are links to `wrap`, which reads its own name. The Makefile names them for these targets alone, in `arm` and `arm-test`, so a native build on an ARM machine uses that machine's toolchain.

A fetch whose sysroot is already extracted, or whose packages are all in `debs` and match their pins, makes no network request. `test_sysroot.py`, which `make test-tools` runs, checks this against a stand-in curl that records every request and stands in for both archives. It also checks that a cached package or a download whose SHA-256 differs from its pin is fetched again or refused, and never extracted, and that the snapshot serves a package the main archive no longer has or cannot serve. The VT tools' tests in `tools/vt/tests/` give their builds a home directory of their own, so they name the caller's sysroot cache to those builds with `VTIUM_ARM_SYSROOTS`, and their sysroot fetch finds the packages already cached.

A test binary under qemu starts other target binaries, as `crates/vtium/tests` start vtium, and the kernel hands those to qemu only through binfmt_misc. When the host registers no qemu for the target, `TRIPLE-run` mounts a binfmt_misc of its own in a user namespace, registers qemu there, and runs the binary in a nested namespace mapped back to the invoking user and group. Nothing on the host changes. Where the namespace or its binfmt_misc is refused, as before Linux 6.7, it runs qemu directly and says so, and the tests that start a target binary fail.

The sysroot is `/usr`-merged and carries an empty loader cache. qemu-user finds an `open()` in the sysroot but not a `statx()`, so a loader search that missed in `/lib/TRIPLE` would mark that directory absent; and the host's loader cache names 32-bit host libraries the armv7 loader would take.

## What is pinned

`pin.tsv` names the archives, the key and every package:

| Row | Pin |
|---|---|
| `archive` | `deb.debian.org/debian`, Debian's main archive, which a CDN serves |
| `snapshot` | `snapshot.debian.org/archive/debian/20261020T000000Z`, which never changes |
| `suite` | `bookworm`: glibc 2.36, so the binaries run on bookworm and later |
| `key` | the Debian archive key that signs the suite's `InRelease`, primary fingerprint `04B54C3CDCA79751B16BC6B5225629DF75B188BD` |
| `deb` | each package per architecture, with its version, pool path and SHA-256: libc6, libc6-dev, linux-libc-dev, libgcc-s1, libgcc-12-dev, libudev1, libudev-dev, libxkbcommon0, libxkbcommon-dev; libsystemd0 and libsystemd-dev for the `logind` feature, with the libraries libsystemd0 loads (libcap2, libgcrypt20, libgpg-error0, liblz4-1, liblzma5, libzstd1); and for arm64 and armhf, Mesa's EGL and llvmpipe, which the `gpu` feature's tests draw on (libegl1, libegl-mesa0, libgl1-mesa-dri, libgbm1), with the 38 packages they depend on, libLLVM 15 among them: about 45 MiB to download for each architecture |

`tools/arm/sysroot pin` resolves the `deb` rows again from the snapshot: it verifies `InRelease` against the pinned key, fetched into a keyring beside the sysroots and never the user's, each architecture's `Packages.xz` against `InRelease`, and reads each package's version, path and SHA-256 from `Packages`. A change of snapshot or package goes in a commit of its own.

## Where the packages come from

`sysroot fetch` asks two archives for each package it does not have cached, in this order:

1. **The main archive,** `deb.debian.org/debian`, at the pool path the pin names. A CDN serves it, and it carries each pinned version until a later bookworm point release replaces that version.
2. **The snapshot,** `snapshot.debian.org`, at the same pool path. The snapshot keeps every version for good, but it is a single service, and it answers GitHub's runners with 503 and 504 for minutes at a time.

The fetch asks the snapshot only when the main archive does not serve the package: when the main archive answers 404 because it no longer carries that version, when it answers another error, or when it cannot be reached. curl retries a transient error such as a 503 three times before the fetch moves on to the snapshot. When neither archive serves the package, the fetch stops and names both archives it tried.

Every download is checked against the SHA-256 that `pin.tsv` pins before it enters the cache, so either archive can serve any package safely. A download whose SHA-256 differs from its pin, from either archive, stops the fetch with an error that names the archive the download came from. The fetch then asks no other archive. A wrong package means that an archive served something other than what was pinned, and a person should look at why before anything is fetched from elsewhere.

The security archive, `security.debian.org/debian-security`, is not asked. `sysroot pin` resolves every package from the snapshot's `bookworm` suite, which holds the point releases and never the separate `bookworm-security` suite. Every pinned version was therefore in the main archive when the snapshot was taken. Some packages that came from security updates, such as libgcrypt20 1.10.1-3+deb12u1, are also in the security archive, but under `pool/updates/main`, and the main archive holds them too. On 2026-11-05 the main archive served all 132 pinned packages, libc6 2.36-9+deb12u14 among them. When a point release later replaces a pinned version, the main archive answers 404 for that package and the snapshot serves it.

## What qemu-user does not judge

qemu-user runs vtium's code on an emulated CPU over the host's kernel, so it judges word size, alignment, `c_char` signedness and the target's ABI. It does not judge:

- **Speed.** A test's time under emulation says nothing about a Cortex-A53.
- **Device ioctls qemu does not translate,** such as DRM's `GET_CAP`: qemu answers `ENOTTY`, so a test on the host's render node fails under qemu while it passes natively.
- **Tracing.** The host's `strace` on a target binary traces qemu.

`TRIPLE-run` exports `VTIUM_QEMU_USER`, naming the qemu it runs. The two tests that depend on these skip only when it is set and qemu's symptom shows: the render-node test in vtium-platform's `display::card` when its DRM ioctl answers ENOTTY, and check-config's open check when strace records opens under `QEMU_LD_PREFIX`. `TRIPLE-run` names each on its test binary's run, since a passing test's own output is hidden. CI's `arm-native` job runs the whole suite natively on an aarch64 runner. The runner has no render node, so the DRM test skips there; a real ARM machine judges it.

## The musl targets

They are not built. vtium links libudev and libxkbcommon, which Debian builds for glibc only, and the musl targets link statically by default; building them needs a musl sysroot from another distribution with static or musl-dynamic builds of both libraries.

## The x86_64 build

`make minspec` and `make install-tree` link vtium and vtium-config for `x86_64-unknown-linux-gnu` through the same wrappers against the `amd64` sysroot, so neither binary requires more than glibc 2.36 (vtium/vtium#143). `x86_64-unknown-linux-gnu-run` runs a binary natively. Each build, and `make arm`, reads the binary's highest required `GLIBC_` version with `readelf -V` and fails above 2.36.
