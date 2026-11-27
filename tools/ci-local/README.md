# ci-local

`make ci-local` runs the workspace's tests the way the `check` jobs of CI run them, with `--all-features` and without, in a container whose userland matches those jobs. The tests run under [cargo-nextest](https://nexte.st/), a test runner that runs each test in a process of its own, as many at once as the machine has cores. The run is `cargo nextest run --workspace --no-fail-fast`, then `cargo test --doc --workspace` for the doctests, which nextest skips. Those jobs run on GitHub's `ubuntu-latest` runner, which is Ubuntu 24.04. There /bin/sh is dash, and bash and coreutils are older than on an up-to-date Arch machine, so a test can pass here and fail on CI. `make ci-local` finds such a test before a push does.

```sh
make ci-local                                   # both builds, as CI runs them
make ci-local BUILDS=default                    # one build
make ci-local TEST_ARGS='-p vtium-clipboard'    # one crate, passed on to cargo nextest and cargo test
```

The last line of the output says `passed`, or names each failed test after the test binary that holds it, for each build. The full output of each build goes to a log in the cache, and the script prints its path.

## What it needs

podman or docker. The script uses podman when it is installed and docker otherwise. It needs the network the first time, to build the image and fetch the crates.

## The image

The Containerfile starts from Ubuntu 24.04, pinned by digest:

```
docker.io/library/ubuntu:24.04@sha256:73b999393d07f2bf65a40373778a44dc9a6d54ad7718d29247d8ff9413c4d6a8
```

The tag `24.04` moves each time Ubuntu publishes a new image, so the same Containerfile would build a different userland from one week to the next. With the digest, a run that fails on Monday fails the same way on Friday, and the userland changes only in a commit that moves the pin. To move it, run `docker pull ubuntu:24.04`, then `docker inspect --format '{{index .RepoDigests 0}}' ubuntu:24.04`, and put the digest it prints in the Containerfile and here.

The image adds the packages the `check` jobs install, and the packages of the runner image that the tests reach. It installs rustup 1.29.3, checked against its SHA-256, and the toolchain that `rust-toolchain.toml` pins. It also installs cargo-nextest at the version the Makefile's `NEXTEST_VERSION` names, the version CI installs, with `cargo install --locked`. The image's tag carries the toolchain, the cargo-nextest version and a hash of the Containerfile, so a change to any of them builds a new image.

## How the container matches the runner

- The tests run as a user named runner, in `/home/runner/work/vtium/vtium`, as on CI. The user has your uid and gid, so the files in the cache stay yours.
- The runner's awk is gawk, and the image keeps the mawk that Ubuntu's container image ships. The tests work with any POSIX awk, so a test that needs gawk fails here before it reaches CI.
- The runner is a virtual machine with the VT devices `/dev/tty0` to `/dev/tty63`, and some tests read a VT's number from one. The container starts as root only to create those device nodes, then drops to runner with no capabilities. The container's device rules refuse every open of the nodes, even by root, so no test can reach this machine's VTs. Rootless podman cannot create device nodes, so under podman the tests in `crates/vtium/tests/enable_key.rs` fail.
- docker and podman put a seccomp filter of their own on each process, and the runner has none. One test counts a process's seccomp filters, so the container runs with `--security-opt seccomp=unconfined`. The tests then have the same system calls as when they run on this machine.

## What it leaves alone

The worktree is mounted read-only. Before each build, the script copies the files a checkout would hold into the container: the files git tracks and the untracked files git does not ignore. So an uncommitted change is tested too, and the run never writes to the worktree.

## The cache

Everything the runs keep is under `$XDG_CACHE_HOME/vtium/ci-local` (`~/.cache/vtium/ci-local` by default): cargo's registry, a log directory and a target directory for each worktree. Cargo decides what to rebuild from file times, so two worktrees whose sources differ never share a target directory. A worktree's target directory holds both builds and takes about 30 GB. Remove `target/` in the cache to free the space, and `docker image rm` the `localhost/vtium-ci-local` images you no longer need.

## How long it takes

These times were measured before the tests ran under nextest, which runs them faster. On the maintainer's machine, the first run took about 10 minutes: half a minute to build the image, and about 9 minutes to compile and run both builds. The image now also compiles cargo-nextest once, which takes a few minutes more. A later run with nothing to compile took about 8 minutes and 40 seconds, almost all of it running the tests.
