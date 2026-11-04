# Install vtium and give it VTs

This guide installs vtium and gives it the VTs you choose. A VT, or virtual terminal, is one of the full-screen text screens the Linux kernel keeps beside a desktop. If you have never run vtium, [Try vtium on one VT](tutorial-try-vtium.md) walks through a first install on one VT and back again.

The steps for each task are, for now, in [managing-vtium.md](managing-vtium.md):

- [Installing on Arch Linux](managing-vtium.md#arch-linux) from the AUR, the files the packages install, and building a package with pictures or with suspend through logind.
- [Installing from a checkout](managing-vtium.md#from-a-checkout) of vtium's source, and staging a package build on another distribution.
- [Giving vtium a VT](managing-vtium.md#giving-vtium-a-vt), one or every one, and taking them back.
- [Moving from `kmscon`](managing-vtium.md#moving-from-kmscon), and [sharing VTs with `kmscon`](managing-vtium.md#sharing-vts-with-kmscon).
- [Upgrading and removing](managing-vtium.md#upgrading-and-removing) vtium.

[crates/vtium/README.md](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#setting-it-up) gives the full rules of `vtium console`, and says how [ssh behaves from vtium](https://github.com/vtium/vtium/blob/main/crates/vtium/README.md#ssh-from-vtium).

## What a build from a checkout needs

Building vtium needs Rust 1.88 or later and clang. The first build fetches a Debian sysroot, a copy of a Debian system's libraries with glibc 2.36, to link against, so the binary runs on any distribution with that glibc or a newer one ([tools/arm/README.md](https://github.com/vtium/vtium/blob/main/tools/arm/README.md)).

`make install-tree` stages what a system installs under `target/install-tree`: the console, `vtium-config`, the VT unit and its getty fallback, the terminfo entry and the example configuration. It also needs `tic`, which compiles the terminfo entry. Tier 3, vtium's test harness on a virtual machine, installs that tree in its guest.

`make vt-shell` runs your own shell in vtium on a free VT and installs nothing; [the tutorial](tutorial-try-vtium.md#without-a-package) uses it, and [vt-review.md](vt-review.md) describes it and the other runs on a VT that a checkout offers.
