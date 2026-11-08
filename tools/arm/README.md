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

The namespace's binfmt_misc entry names qemu itself, with the `O` flag, so the kernel opens the target binary and hands qemu the open file. vtium starts its helpers through `/proc/self/fd/N`, from a descriptor it closes at the exec, so that path no longer names the binary once qemu runs. The entry also has the `P` flag, so the kernel keeps the `argv[0]` that vtium passes, the helper's path, and tells qemu that it did ([binfmt_misc](https://docs.kernel.org/admin-guide/binfmt-misc.html)). Without it, the kernel puts `/proc/self/fd/N` in its place.

The namespace maps no root, so every file and directory that root owns on the host belongs to the overflow uid, 65534, inside it. vtium reads a configuration file only when root or its own account owns each directory above it (ADR-1403 D3), so the tests could read none of their own files there. `TRIPLE-run` therefore runs the tests in a root of their own: a tmpfs mounted on `ARCH-ID.root` beside the sysroot, with the host's entries bound into it. The directories on the way to two places are made anew on the tmpfs, so they belong to the invoking user:

- the highest directory above the test binary that the invoking user owns, such as the home directory, which is bound below them;
- qemu's built-in prefix, `/usr/gnemul/qemu-aarch64` for this build of qemu, where the sysroot is bound. vtium starts its helpers with an empty environment, so a helper gets no `QEMU_LD_PREFIX`, and qemu finds the target's loader at the prefix instead.

A file that root owns, such as `/bin/sh`, still belongs to uid 65534 there. The tests that start a stand-in for the font helper start a copy of `/bin/sh` that their own account owns, as they would need to on any host where root's files are not root's.

qemu opens a path that also exists in the sysroot from the sysroot, and not only the libraries. A test that reads the kernel's UAPI headers from `/usr/include` would therefore read Debian 12's, which are older than the kernel the tests run on. qemu passes the tests' ioctls to the host's kernel, so `TRIPLE-run` points `VTIUM_KERNEL_HEADERS` at the host's headers through `/proc/self/root/usr/include`, a path the sysroot does not have.

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

`sysroot fetch` downloads each package it has not cached from Debian's main archive, `deb.debian.org/debian`, at the pool path its pin names. It asks the snapshot only when the main archive does not serve that package, which happens when a later point release has replaced the pinned version, when the main archive answers an error after curl's retries, or when it cannot be reached. The security archive is not asked, because `sysroot pin` resolves every package from the `bookworm` suite, which the main archive carries, and never from `bookworm-security`.

Every download is checked against its pinned SHA-256 before it enters the cache. A download that differs, from either archive, stops the fetch with an error naming the archive, and no other archive is asked.

Re-pinning with `tools/arm/sysroot pin` once per release cycle keeps most packages on the main archive, so the snapshot serves only the few that a point release has replaced since.

## What qemu-user does not judge

qemu-user runs vtium's code on an emulated CPU over the host's kernel, so it judges word size, alignment, `c_char` signedness and the target's ABI. It does not judge:

- **Speed.** A test's time under emulation says nothing about a Cortex-A53.
- **Device ioctls qemu does not translate,** such as DRM's `GET_CAP`: qemu answers `ENOTTY`, so a test on the host's render node fails under qemu while it passes natively.
- **Tracing.** The host's `strace` on a target binary traces qemu.
- **seccomp.** qemu answers `ENOSYS` to `seccomp()`, since a filter would bind qemu itself. The font helper refuses to run without its socket filter, and the image helper without its filters.
- **A process of one thread.** qemu runs a thread of its own in every process. The image helper checks that it runs alone before it installs its filter, and it refuses to run when it does not.
- **The inode of `/proc/self/exe`.** qemu answers a `readlink` of `/proc/self/exe` with the program it runs, but a `stat` of it reaches the kernel, which names qemu itself.
- **Memory limits.** qemu does not pass `RLIMIT_AS` and `RLIMIT_DATA` on to the kernel, since they would limit qemu itself, so a child under qemu reports no limit.
- **A failed start.** glibc's `posix_spawn` reports a failed `exec` through memory the child shares with its parent. qemu runs that child as a fork that shares no memory, so the start of a program no one may execute succeeds, and the child exits with status 127.
- **Root's own files.** In the user namespace, no file belongs to root (above).

`TRIPLE-run` exports `VTIUM_QEMU_USER`, naming the qemu it runs. The tests that depend on these skip only when it is set and the symptom shows, or, for the io_uring test, when the symptom shows:

| Test | Skips when |
|---|---|
| vtium-platform's `display::card::tests::a_render_node_through_the_real_ioctls` | its DRM ioctl answers ENOTTY |
| vtium's `check_config` test `it_opens_nothing_but_the_configuration` | strace records opens under `QEMU_LD_PREFIX` |
| vtium's `helper::tests::a_failed_start_names_proc_self_fd` | a program no one may execute starts |
| vtium's `check::tests`, the assertions that root's own helper draws no note | root does not own `/bin/sh` |
| vtium-config's `serve::tests::the_helpers_socket_filter_refuses_every_new_socket` | the socket filter's install answers ENOSYS |
| vtium-config's `serve::tests::the_helpers_socket_filter_refuses_io_uring` | `python3`, which installs the filter, runs on another architecture than the test |
| the tests in vtium-config's `tests/` that start the real font helper, in `commands.rs`, `preview.rs`, `serve.rs` and `specimen.rs` | `vtium_testscript::qemu_without_seccomp` finds that a filter answers ENOSYS |
| vtium-image-helper's `tests/helper.rs`, and the four roles of `tests/filter.rs` that install a filter | the same |
| vtium-config's `cli.rs`, the address-space check of `the_parser_runs_without_new_privileges_and_within_its_limits` | the parser shows no address-space limit |
| vtium-platform's `tests/image_helper.rs`, the memory-cap check of `the_child_starts_alone_as_the_account_with_its_cap_and_nothing_else` | the child shows no address-space limit |
| vtium-platform's `tests/image_helper.rs`, `every_helper_starts_from_the_program_opened_at_start`, which then reads the child's `/proc/self/exe` link, not its inode: the program it opened, which the upgrade unlinked, reads as deleted | the child's `/proc/self/exe` has qemu's inode |
| vtium's `boot.rs`, `an_outline_frit_draws_in_the_helpers_cell_at_each_height`, whose subject is the helper's answer within the console's wait | the log says the emulated helper answered after the wait, as it can on a busy host; `a_late_outline_cell_is_drawn_from_the_next_frame` tests a late answer |
| vtium-settings' `file.rs`, the check of root's file in `a_vtium_that_runs_as_a_user_reads_that_user_s_file_too` | root does not own `/etc/passwd` |

`TRIPLE-run` names each on its test binary's run, since a passing test's own output is hidden. CI's `arm-native` job runs the whole suite natively on an aarch64 runner. The runner has no render node, so the DRM test skips there; a real ARM machine judges it.

## The musl targets

They are not built. vtium links libudev and libxkbcommon, which Debian builds for glibc only, and the musl targets link statically by default; building them needs a musl sysroot from another distribution with static or musl-dynamic builds of both libraries.

## The x86_64 build

`make minspec` and `make install-tree` link vtium and vtium-config for `x86_64-unknown-linux-gnu` through the same wrappers against the `amd64` sysroot, so neither binary requires more than glibc 2.36 (vtium/vtium#143). `x86_64-unknown-linux-gnu-run` runs a binary natively. Each build, and `make arm`, reads the binary's highest required `GLIBC_` version with `readelf -V` and fails above 2.36.
