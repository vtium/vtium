---
contract: adr/v1
kind: decision
verb: add
capability: process
extends:
  - ADR-1500
basis:
  - operator: aaronsb
    level: directed
    said: "B: files on the release"
    via: "the maintainer's answer through the question tool in the steering session on 2026-11-23, choosing where vtium's rpm and deb packages are published; the same answer put Fedora (rpm), Debian and Ubuntu (deb) and openSUSE (rpm) first, had vtium's own release workflow build and attest the packages, and left build services such as COPR, OBS and a PPA for later, as the steering session's notes of that day record it"
  - operator: aaronsb
    level: directed
    said: "the arch pattern should be a good map for what we want to include"
    via: "the maintainer in the steering session on 2026-11-23, on which packages the rpm and deb families build and what each one holds, as the steering session's notes of that day record it; the notes read it as the split of vtium-arch's recipes, with -git variants only where the format has a use for them"
  - operator: aaronsb
    level: directed
    said: "actual distribution builds come later"
    via: "the maintainer in the steering session on 2026-11-23, as the steering session's notes of that day record it; read as: the wave that writes this record writes the record only, and building the packages comes in a later wave"
  - operator: aaronsb
    level: directed
    said: "rpm and deb packages, each with its tier 3 guests, are ready for 0.1.0. This work is in the beta phase, after the alpha phase has built every feature."
    via: "the maintainer's decision of 2026-11-20, recorded on the GitHub issues for distribution packages and for the Fedora and Debian tier 3 guests"
    paraphrase: true
  - evidence: "card 1b's findings of 2026-11-11, recorded on the GitHub issue for distribution packages: seat mode opens a PAM session named vtium, whose file follows Arch's conventions, and Debian and Fedora build their PAM stacks differently; SELinux on Fedora and AppArmor on Ubuntu may deny reading /dev/kmsg, the scope that vtium asks systemd to start for each login, or the PAM session; a family that does not package libseat builds its console without it"
  - evidence: "vtium-arch at 2f9b656 (2026-11-22), read 2026-11-23: the recipes vtium, vtium-git, vtium-minspec, vtium-minspec-git, vtium-minspec-gpu, vtium-minspec-gpu-git, vtium-terminfo, vtium-terminfo-git and vtium-fonts; vtium builds with the features images logind gpu bidi libseat filters, HWACCEL=on and DISPLAY_BUFFERS=3, the minspec recipes with none or gpu alone, HWACCEL=off and DISPLAY_BUFFERS=2; each console recipe calls make install-build and make install, vtium-terminfo calls make install-terminfo, and vtium-fonts calls make install-fonts and carries pinned copies of Px437 IBM VGA 9x16, Glass TTY VT220, Tiny5 Mono and Unscii-8; vtium.install claims the free VTs at a first install, changes no VT at an upgrade and gives the VTs back before a removal; ci/build-recipe lints each package with namcap against an allow file; test-install-messages tests the install script against a stand-in systemctl and a stand-in vtium"
  - evidence: "vtium's integration branch at a040866e, read 2026-11-23: make install stops unless Terminus 4.49.1's eighteen BDF files, Unifont's .hex file and Cullet Mono's two faces are staged and match their pins in tools/fonts/pins.tsv and tools/cullet-mono/pins.tsv; the Cullet Mono pins hold the bytes that FontForge 20251009 writes, so the release workflow builds the faces in an Arch Linux container; the vendor archive carries crates and no font file; Cargo.toml's rust-version is 1.88 and rust-toolchain.toml pins 1.99.1"
  - evidence: "the draft-release branch at 90acbafe, read 2026-11-23: the release workflow attaches the release's files to a draft, its attach job refuses a file whose name does not begin with vtium-VERSION, and its published job checks that a published release carries exactly the files SHA256SUMS lists, that each matches its SHA-256 and that each has an attestation"
  - evidence: "Docker Hub's tags, read 2026-11-23: fedora:latest is fedora:45, and fedora:44 is still published; opensuse/leap:latest is opensuse/leap:16.1, and 16.0 is still published; ubuntu:26.04 (resolute) and ubuntu:24.04 (noble) are the LTS releases; debian:stable is trixie, Debian 13"
  - evidence: "sources.debian.org, read 2026-11-23: Debian 13 ships rustc 1.85.1, below vtium's rust-version 1.88; seatd 0.9.1, which builds libseat; debhelper 13.24.2; and needrestart 3.11"
  - precedent: ADR-1500
  - precedent: ADR-1503
  - precedent: ADR-1505
  - precedent: ADR-1204
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "Two repositories"
    via: the maintainer, through the question tool in the steering session, 2026-11-23
    covers: [where the packaging lives]
  - operator: aaronsb
    said: "A fonts archive"
    via: the maintainer, through the question tool in the steering session, 2026-11-23
    covers: [the font files]
  - operator: aaronsb
    said: "pinned rust for now, subject to change based on how it really works out"
    via: the maintainer, through the question tool in the steering session, 2026-11-23
    covers: [the Rust compiler]
  - operator: aaronsb
    said: "All seven"
    via: the maintainer, through the question tool in the steering session, 2026-11-23
    covers: [the targets]
  - operator: aaronsb
    said: "x86_64 first"
    via: the maintainer, through the question tool in the steering session, 2026-11-23
    covers: [architectures]
  - operator: aaronsb
    said: "One file everywhere"
    via: the maintainer, through the question tool in the steering session, 2026-11-23
    covers: [the PAM file]
  - operator: aaronsb
    said: "All or nothing"
    via: the maintainer, through the question tool in the steering session, 2026-11-23
    covers: [a failed package]
  - operator: aaronsb
    said: "All three right"
    via: the maintainer, through the question tool in the steering session, 2026-11-23
    covers: [every release]
  - operator: aaronsb
    said: "All three right"
    via: the maintainer, through the question tool in the steering session, 2026-11-23
    covers: [no signatures]
  - operator: aaronsb
    said: "All three right"
    via: the maintainer, through the question tool in the steering session, 2026-11-23
    covers: [the claim]
status: accepted
date: 2026-11-23
deciders:
  - aaronsb
related:
  - ADR-1500
  - ADR-1503
  - ADR-1505
  - ADR-1204
  - ADR-1400
  - ADR-1101
---

# ADR-1512: vtium's release workflow builds rpm and deb packages from the release's own files and attaches them to the release

## Summary

- **Decided:** vtium gets packages for Fedora and openSUSE, as rpm files, and for Debian and Ubuntu, as deb files. vtium's own release workflow builds them for every release, from the release's source archive, its vendor archive and a new archive of font files. Each package builds with no network, with each family's own tool, in a container of its target release on GitHub's runners. The workflow attests each package, lists it in `SHA256SUMS` and attaches it to the release's draft, so the check that runs when the release is published covers the packages too. The packages split the way vtium-arch's recipes do: `vtium`, the console for a desktop or laptop with the features `images logind gpu bidi libseat filters`, the two small consoles `vtium-minspec` and `vtium-minspec-gpu`, `vtium-fonts` and `vtium-terminfo`. Each package installs what vtium's Makefile stages, so vtium's tree keeps deciding which file goes where. The packaging adds only what each format needs: scripts, dependencies and each family's conventions. The packaging lives in two repositories beside vtium-arch, `vtium-rpm` and `vtium-deb`, held as submodules, once the maintainer creates them. The first targets are Fedora 44 and 45, openSUSE Tumbleweed and Leap 16.1, Debian 13, and Ubuntu 24.04 and 26.04, all on x86_64. Each package passes its family's lint tool, rpmlint or lintian, and an install in a fresh container of its target release. Once the tier 3 guests for these families exist, each package must also install and start on the guest of its distribution before the release is published. Build services such as COPR, the Open Build Service and a Launchpad PPA come later. This record plans the packages, and the maintainer said that the builds come later (`basis`). So the changes to the release workflow and the packaging repositories are built in a later wave, and until that wave lands no release carries an rpm or deb package (D9).
- **Trades away:** A person on these distributions downloads each release's file and installs it, because no package repository tells `dnf`, `zypper` or `apt` that a newer vtium exists. The packages carry no package manager signature, so trust rests on the attestations and `SHA256SUMS`, and zypper asks before it installs an unsigned file. A release takes longer, since it waits for seven target releases' builds, lints and installs, and a failed package holds the whole release back. Once published, a release's files are locked, so a packaging fix waits for the next release. The packages claim the free VTs at install, as the Arch packages do, which the distributions' own rules forbid for their archives, so these packages stay vtium's own. The packages carry Cullet Mono's faces built on another machine, because the distributions' FontForge does not write the bytes vtium pins.
- **One-way?** No. How the packages are built is reversible, because a release's packages are files and a later release can build them another way. The package names are expensive: once machines install `vtium-minspec` by name, a rename needs a transition package in each family.
- **Probes:**
  - *Not confident (where the packaging lives):* the spec files and `debian/` directories live in two new repositories, `vtium/vtium-rpm` and `vtium/vtium-deb`, held as submodules at `packaging/rpm` and `packaging/deb`, as ADR-1500 D1 decides for each package manager (D5). The other choice is two folders in vtium's own tree. Lean: the repositories, because they keep the packaging's history and CI apart from vtium's, match vtium-arch, and give a later build service a repository to read. Do you want the repositories, or the folders? (Answered 2026-11-23; see `considered`.)
  - *Not confident (the font files):* `make install` needs Cullet Mono's faces, and only FontForge 20251009 writes the bytes their pins hold. Lean: each release carries the font files `make install` stages, built once in the release workflow, as a new archive `vtium-VERSION-fonts.tar.zst` beside the vendor archive (D4). The other choices are building the faces in each family's container with that family's FontForge, which fails the pins, or loosening the pins for packages. Is a fonts archive on each release right? (Answered 2026-11-23; see `considered`.)
  - *Not confident (the Rust compiler):* the packages build with the Rust release that `rust-toolchain.toml` pins, which rustup installs in the build container before the offline build (D4). Debian 13's own rustc is 1.85, older than vtium's minimum of 1.88. Lean: the pinned Rust on every target, so every package builds with the compiler the gates ran. Should a target whose own Rust is new enough, such as Fedora, use its own Rust instead? (Answered 2026-11-23; see `considered`.)
  - *Not confident (the targets):* Fedora 44 and 45, openSUSE Tumbleweed and Leap 16.1, Debian 13, and Ubuntu 24.04 and 26.04, seven targets in all (D6). Lean: both Ubuntu LTS releases, because most Ubuntu machines still run 24.04, and only the newest Leap and Debian's stable release. Is that the list, or should Ubuntu keep only 26.04? (Answered 2026-11-23; see `considered`.)
  - *Not confident (architectures):* the packages build for x86_64 only at first, as vtium-arch's do. Lean: aarch64 joins once its packages can be checked, because tier 3's guests run on x86_64 KVM, so an aarch64 package would go out unchecked (D8). Should aarch64 packages come with the first release that carries packages, checked in containers only? (Answered 2026-11-23; see `considered`.)
  - *Not confident (the PAM file):* seat mode's PAM file, `/etc/pam.d/vtium`, names its modules directly and includes no stack of the distribution's. Lean: every family ships vtium's one file, because it must give the session the greeter class, which a family's shared session stack does not set, and every family has its modules. A family gets a file of its own only when its guest shows that the shared file fails there (D2). Is one file right, or should each family's file include that family's stack from the start? (Answered 2026-11-23; see `considered`.)
  - *Not confident (a failed package):* when one package fails to build, lint or install, the release run attaches nothing, so the release waits until the package is fixed (D4). Lean: a release carries all its files or none, so a published release never lacks a package that its notes name. Should a release go out without the failed target's packages instead? (Answered 2026-11-23; see `considered`.)
  - *Confident (every release):* every release carries packages, pre-releases included, because the maintainer chose files on each GitHub release. Is that what you want? (Answered 2026-11-23; see `considered`.)
  - *Confident (no signatures):* the packages carry no rpm or deb signature, and a person checks a file with `gh attestation verify` and `SHA256SUMS` (D4). Signed repositories come with the build services. Is that what you want? (Answered 2026-11-23; see `considered`.)
  - *Confident (the claim):* each console package claims the free VTs in direct mode at a first install and gives them back at removal, as vtium-arch's packages do (D2). Is that what you want on these distributions too? (Answered 2026-11-23; see `considered`.)
- **Inversion:** Between "the distributions package vtium in their own archives", which takes their maintainers and their rules, and "each person builds vtium from source". vtium builds its own packages for each distribution, from the files its release already attests, and publishes them on the release.

## Context

vtium is packaged for Arch Linux alone. ADR-1500 decides that vtium's packaging recipes live in one repository per package manager, and vtium-arch holds the Arch recipes. Each recipe calls vtium's own make targets, so vtium's Makefile decides which file goes where. ADR-1500 D8 decides that every package manager publishes the desktop build under vtium's plain name, and ADR-1500 D9 decides that each release carries the source of the crates vtium builds with, for a package manager that builds with no network. People on Fedora, openSUSE, Debian or Ubuntu build vtium from source today.

Each release already carries files that vtium's release workflow makes from the tagged commit. They are the source as `vtium-VERSION.tar.gz` and `.zip`, the vendor archive `vtium-VERSION-vendor.tar.zst` and its `.sha256`, the crates' manifest, and `SHA256SUMS`. The workflow attests each file, so anyone can check it with `gh attestation verify`. The work that publishes each release from a draft attaches those files to the draft. Once a person publishes the draft, the workflow's published job checks that the release carries exactly the files `SHA256SUMS` lists, and that each one matches and has an attestation.

The maintainer chose files on the release for the first packages, built and attested by vtium's own release workflow (`basis`). The maintainer named vtium-arch as the map for what the packages include. The tier 3 guests for Fedora and Debian, which the work that gives those families their guests builds, are where each package is installed and started.

Some terms recur below. A **family** is a set of distributions that share a package format: the rpm family is Fedora and openSUSE, and the deb family is Debian and Ubuntu. A **target** is one release of one distribution that packages are built for, such as Fedora 45. A **console package** is one of the three packages that install `/usr/bin/vtium`.

## Decision

### D1: the packages follow vtium-arch's split

Each target gets five packages, the same five in both families:

| Package | What it installs | vtium-arch's recipe |
|---|---|---|
| `vtium` | the console for a desktop or laptop, built with `images logind gpu bidi libseat filters`, `HWACCEL=on` and `DISPLAY_BUFFERS=3`, with its helpers, seat mode, pictures and the gallery | `vtium/` |
| `vtium-minspec` | vtium's smallest console, built with no feature, `HWACCEL=off` and `DISPLAY_BUFFERS=2`, for a single-board computer, a server's console or a kiosk | `vtium-minspec/` |
| `vtium-minspec-gpu` | the smallest console with the feature `gpu` alone, `HWACCEL=off` and `DISPLAY_BUFFERS=2` | `vtium-minspec-gpu/` |
| `vtium-terminfo` | the terminfo entry `xterm-vtium` and its alias `vtium`, the same for every architecture | `vtium-terminfo/` |
| `vtium-fonts` | the font sets and layers of vtium's catalog that no console package installs, the four fonts no distribution packages, and the notices file, the same for every architecture | `vtium-fonts/` |

- **The names.** Both families use the Arch names. Each name follows both families' rules: lowercase, the project's name first, and a hyphenated suffix. Debian names a font package `fonts-*`, but `vtium-fonts` is a catalog of font sets that depends on the distribution's font packages and carries four fonts beside them, so it keeps its name. Fedora and openSUSE name font packages `*-fonts`, which `vtium-fonts` already matches.
- **The small consoles are packages here.** The AUR carries no minspec package, because the maintainer does not want AUR packages that differ from `vtium` only in build options (ADR-1500 D8). Here each console is a file on the release, built in the same run as `vtium`, so it costs one file per target, and a person on a small machine installs it by name. The maintainer named both small consoles in the split (`basis`).
- **One console at a time.** Each console package provides, conflicts with and, in the deb family, replaces the virtual name `vtium-console`. So a machine holds one console package, and installing another takes the first one's place. Each console package depends on `vtium-terminfo` of the same version.
- **Two source packages per family.** A spec file or a `debian/` directory builds several binary packages from one source. So each family has two source packages: `vtium`, which builds the three consoles and `vtium-terminfo` from one unpacked tree, one build of the tree per console, and `vtium-fonts`, which builds `vtium-fonts` from its own sources. Arch keeps one recipe per package because the AUR publishes one package base per repository.
- **No `-git` packages.** A file on a release is built from that release's tag, so a package that follows `main` has no release to sit on. A person who wants `main` on these distributions builds it from source. A build service, which comes later, may carry a package built from `main`.
- **Versions.** A package's version is the tag without its `v`, with `~` in place of the hyphen before a pre-release: `v0.1.0-alpha.12` is `0.1.0~alpha.12`. rpm and dpkg both sort `~` before anything, so `0.1.0~alpha.12` sorts before `0.1.0`. The packaging's own revision starts at 1 for each vtium release and stays there, because each release builds its packages once. The revision also names the target, so that a newer target sorts after an older one and no two targets make files with the same name. The rpm family writes `1.fc45` on Fedora, from Fedora's own `%{dist}`, and `1.tw` and `1.lp161` on openSUSE. The deb family writes `1~deb13` and `1~ubuntu26.04`.

### D2: what each package installs

- **vtium's Makefile decides the contents.** Each console package's build calls `make install-build` and `make install` with `DESTDIR`, `PREFIX=/usr`, `PKGNAME`, `FEATURES`, `HWACCEL`, `DISPLAY_BUFFERS` and `CARGO_BUILD_FLAGS=--frozen`, as vtium-arch's recipes do. `vtium-terminfo`'s build calls `make install-terminfo`. `vtium-fonts`' build calls `make install-fonts`, then installs the four carried fonts and their license texts as vtium-arch's `vtium-fonts` recipe does. A file that a package should ship and that `make install` does not stage is a change to vtium's Makefile, made once for every family.
- **What `vtium` installs.** These are the files `make install` stages for its features: `/usr/bin/vtium`, `vtium-config`, `vtium-clipboard` and its link `vticlip`, the image helper `vtium-image` and the gallery `vtium-gallery`. It installs the units `vtium@.service`, `vtium-getty@.service` and seat mode's `vtium-seat@.service` in `/usr/lib/systemd/system`, and the image helper's account in `/usr/lib/sysusers.d/vtium-image.conf` (ADR-1101 D2). It also installs seat mode's PAM file `/etc/pam.d/vtium` (ADR-1400 D3), `/etc/vtium/vtium.conf.example`, and the font sets and layers a console package ships in `/etc/vtium/fonts`. Under `/usr/share/vtium` go the font files, color schemes, looks and pointer file. The man pages are `vtium.8`, `vtium-config.1`, `vtium-clipboard.1`, `vticlip.1`, `vtium.conf.5` and `vtium-gallery.1`. The example udev rules and hwdb fragment go in the documentation folder, beside the license texts. The two small consoles install the same files apart from the image helper, its account, the gallery, seat mode's unit and the PAM file.
- **Each family's conventions, in the packaging.** These changes live in the packaging repositories and none of them in vtium's tree:
  - Every file under `/etc` is configuration that a person may edit. rpm marks each one `%config(noreplace)`, and dpkg treats each one as a conffile, so an upgrade keeps a person's edits, as `backup=` does on Arch.
  - The rpm family keeps the license texts in `/usr/share/licenses/<package>`, where `make install` stages them, through `%license`. The deb family keeps them in `/usr/share/doc/<package>/`, with a machine-readable `debian/copyright` that names every license vtium-arch's recipe names.
  - Each family creates the image helper's account at install with its own support for sysusers files. That is rpm's own on Fedora and openSUSE's systemd-rpm-macros there. On Debian and Ubuntu it is debhelper's, or a `postinst` that runs `systemd-sysusers` on the file where a target's debhelper lacks that support.
  - In the deb family, `vtium` also ships `/etc/needrestart/conf.d/vtium.conf`, which tells needrestart never to restart a `vtium@` or `vtium-seat@` unit, as needrestart's own configuration does for `getty@`. needrestart runs after each `apt` upgrade where it is installed, as it is on Ubuntu's server installs, and restarts the services whose files changed. A restart of vtium ends every session on that VT.
  - seat mode's PAM file is vtium's one file in every family, which names its modules directly, until a family's guest shows that the file fails there (the PAM probe). The file gives vtium's own session the greeter class, which a family's shared session stack does not set.
- **The install scripts behave as vtium-arch's.** The three rules of `vtium.install` hold in each family's scripts (ADR-1500 D3, D5 and D7):
  - A first install gives vtium the free VTs in direct mode with `vtium console enable --direct --all`. In rpm that is `%post` when it is the package's first install, and in dpkg it is `postinst configure` with no earlier version. A refused claim never fails the install, and the message gives `vtium console`'s reason and the command to run once the reason is gone.
  - An upgrade changes no VT and restarts nothing. No family's systemd helper enables, starts or restarts a vtium unit. The rpm family uses `%systemd_postun` and never its `_with_restart` form, and the deb family runs `dh_installsystemd` with its enable, start and restart after upgrade turned off.
  - A removal gives the VTs back with `vtium console disable --all` before the package's files go, when no other console package stays. In rpm that is `%preun` when the package is being erased, and in dpkg it is `prerm remove`. rpm installs a new package before it removes the one the new package replaces. So when a person swaps `vtium` for `vtium-minspec` in one transaction, `vtium-minspec`'s install script claims the free VTs first, and `vtium`'s removal script runs after it. The removal script therefore gives the VTs back only when no other package that provides `vtium-console` is installed. dpkg removes a conflicting package before it configures the new one, as pacman does, so there the new package claims the VTs again after the old one gives them back.
  - The messages say what vtium-arch's messages say, and point to vtium's install guide rather than vtium-arch's README.

### D3: dependencies

- **Hard and optional dependencies map the same way in both families.** Each hard dependency of vtium-arch's recipe becomes a hard dependency: `Requires:` in rpm and `Depends:` in deb. Each optional dependency becomes `Suggests:` in both. dnf, zypper and apt install `Recommends:` by default and `Suggests:` only on request, so a default install brings in the same set as Arch's.
- **Linked libraries come from each family's own tools.** rpm adds a dependency on the shared library of every library a binary links, by its soname, and `dpkg-shlibdeps` writes `${shlibs:Depends}` the same way. That covers libxkbcommon, libudev, and, in `vtium`, libsystemd and libseat. The GPU path loads `libEGL.so.1` and `libgbm.so.1` at run time and links neither, so the packaging names them. The rpm family names them by soname, as `libEGL.so.1()(64bit)` and `libgbm.so.1()(64bit)`, which Fedora's and openSUSE's packages both provide, so one spec file serves both. The deb family names `libegl1` and `libgbm1`. `vtium` depends on them, and `vtium-minspec-gpu` suggests them, as on Arch.
- **What the PAM file loads.** seat mode's PAM file loads `pam_systemd.so`, which no binary links, so `vtium` depends on the family's package that holds it: `systemd-pam` on Fedora and `libpam-systemd` on Debian and Ubuntu. The card that writes the rpm packaging finds openSUSE's.
- **`vtium-fonts` follows ADR-1204 D8 in each family.** It depends on the family's package where one holds a font, carries a pinned copy where none does and the license allows it, and suggests the large ones, such as Noto Sans CJK, and the Nerd Fonts builds. The names differ by family: Noto Color Emoji is `google-noto-color-emoji-fonts` on Fedora, `noto-coloremoji-fonts` on openSUSE and `fonts-noto-color-emoji` on Debian and Ubuntu. The card that writes each family's `vtium-fonts` writes its list, and the container install of D7 proves that each name resolves on each target. vtium-config already looks for console fonts in `/usr/share/kbd/consolefonts`, `/usr/lib/kbd/consolefonts`, `/lib/kbd/consolefonts` and `/usr/share/consolefonts`, so the console font sets find each family's files there. Debian's console fonts carry names of their own, so that card also checks which console sets find their font on Debian and Ubuntu.
- **A target that lacks a library builds without the feature.** When a target does not package a library a feature needs, its console packages build without that feature, and the package's description says so. For example, a target without libseat would get a `vtium` with no seat mode, and `vtium console enable --seat` would not be offered there. Debian 13 packages libseat (`basis`), and the first card checks every other target.

### D4: how the packages are built

- **vtium's release workflow builds them.** `.github/workflows/release-vendor.yml` gains one package job per target, run as a matrix on GitHub's Ubuntu runners. The jobs run after the archive job, both in the run that attaches the files to the draft and in a dry run, so a branch's packages can be built and tried before a release depends on them.
- **From the release's own files.** Each package job takes the archive job's files: `vtium-VERSION.tar.gz`, the vendor archive, and a new archive `vtium-VERSION-fonts.tar.zst`. The fonts archive holds the files `make install` checks against their pins: Terminus's BDF files, Unifont's `.hex` file and Cullet Mono's two faces. The archive job builds the faces already, in an Arch Linux container with FontForge 20251009, and `make vendor-archive` packs the archive beside the vendor archive (the font files probe). The job first checks each file against the archive job's `SHA256SUMS`. `vtium-fonts` also takes the four carried fonts' upstream files, which the packaging repository pins by URL and SHA-256, as vtium-arch's recipe does. The packaging itself is the packaging repository at the commit that the tag's submodule pointer names (D5).
- **The Rust compiler.** The build uses the Rust release that `rust-toolchain.toml` pins, which rustup installs in the build image (the Rust probe).
- **With no network.** Each job first makes a build image from its target's official container image, such as `fedora:45`, `opensuse/leap:16.1` or `debian:13`, by installing the build's dependencies with the family's package manager and Rust with rustup. It then runs the build in a container of that image started with `docker run --network none`. A missing crate, font file or source therefore fails the build, where a build with network would download it, as `make vendor-archive-check` already fails.
- **With each family's own tool.** The rpm family builds with `rpmbuild -bb` from a spec file. Fedora and openSUSE share each spec file, which tests `%{?suse_version}` where the two differ. The deb family builds with `dpkg-buildpackage -b` from a `debian/` directory that Debian and Ubuntu share, which tests the vendor with `dpkg-vendor` where the two differ. Each build sets `SOURCE_DATE_EPOCH` to the tagged commit's time, as `git archive` uses that time, so a second build from the same files gives the same contents.
- **Binary packages only.** The release carries no source package, no rpm debuginfo and no deb dbgsym package. The release's source and vendor archives, its fonts archive, and the packaging repository at its commit already rebuild every package. vtium-arch builds no debug package either.
- **File names.** Each package is attached as `vtium-VERSION-TARGET-PACKAGE-ARCH.rpm` or `.deb`, such as `vtium-0.1.0-alpha.12-fedora45-vtium-minspec-x86_64.rpm` and `vtium-0.1.0-alpha.12-ubuntu26.04-vtium-terminfo-all.deb`. Debian and Ubuntu would otherwise make files of the same name, and every name begins with `vtium-VERSION`, as the attach job requires. GitHub rewrites some characters in an asset's name, among them the `~` that both families' versions carry. dnf, zypper and apt read a package's name and version from inside the file, so the file's name does not matter to them.
- **Checksums and attestations.** A job after the package jobs writes `SHA256SUMS` over every file the release carries, packages included. The attest job then makes one attestation over all the files, and the attach job attaches them to the draft. The published job checks a published release against `SHA256SUMS` and its attestations, so it covers the packages with no change.
- **A failed package holds the release.** A package that fails to build, lint or install fails its job, so the attach job does not run and the draft carries no file. The person fixes the package and runs the workflow on the draft again (the failed package probe).
- **No signature.** The packages carry no rpm or deb signature, because the maintainer's key never sits in CI and the attestations already tie each file to the tagged commit. dnf and apt install a local file that carries no signature. zypper asks first, and `zypper install --allow-unsigned-rpm` skips the question, which vtium's install guide says.

### D5: where the packaging lives

- The spec files, the `debian/` directories, the install scripts, the pins of the carried fonts, the lint allow files and the packaging's tests live in two repositories in the vtium organization, `vtium/vtium-rpm` and `vtium/vtium-deb`. vtium holds them as submodules at `packaging/rpm` and `packaging/deb`, beside `packaging/arch`, as ADR-1500 D1 decides for each package manager (the probe on where the packaging lives). The maintainer creates the two repositories once this record is accepted, and no agent creates them.
- Each repository has CI of its own, as vtium-arch has. For each pull request, it builds the repository's packages from the latest release's files, or from a dry run's files, then lints them and installs them in containers, as D7 describes. Each repository also carries a build script with one entry point, which the release workflow and the repository's CI both call with a target, the release's files and an output folder.
- The release workflow checks out the tag with its submodules, so the packaging that builds a release is the commit the tag names. The attestation names vtium's commit, whose tree names that packaging commit. A packaging change reaches a release only when vtium's `integration` branch moves the submodule pointer.
- The case for two folders in vtium's tree, `packaging/rpm` and `packaging/deb`, is that a change to `make install` and to the spec file it needs lands in one pull request, with no pointer to move. A release builds its packages from its own tag either way, and a published release's files are locked, so the packaging's own history buys less here than it does on the AUR, where a recipe fix ships at once.

### D6: the targets, and how a release adds or drops one

| Family | Distribution | Targets today | Guest |
|---|---|---|---|
| rpm | Fedora | 44 and 45 | 45 |
| rpm | openSUSE | Tumbleweed and Leap 16.1 | Tumbleweed |
| deb | Debian | 13 | 13 |
| deb | Ubuntu | 24.04 LTS and 26.04 LTS | 26.04 |

- **The rule.** The targets are Fedora's two supported releases, openSUSE Tumbleweed and the newest Leap, Debian's stable release, and every Ubuntu LTS release in standard support (the targets probe). Ubuntu's interim releases, Debian's testing, Fedora's Rawhide and openSUSE's older Leap are not targets.
- **What a target needs.** A target needs glibc 2.36 or later and binutils 2.38 or later, because vtium's binaries pack their relocations with DT_RELR, and systemd with logind. It also needs a package for every library a console package needs, or D3's rule for a missing library.
- **Adding a target.** A target joins when its distribution releases it. A one-line change adds it to the release workflow's target list, once a dry run's container install passes on it. The guest of its distribution moves to the newest target in the same change.
- **Dropping a target.** A target leaves when its distribution ends its support, by the first release after that date, through the same one-line change.
- **The release notes.** They name each target a release adds or drops.

### D7: how each package is checked

- **Lint.** rpmlint checks each rpm, in its target's build image, so openSUSE's packages meet openSUSE's own checks. lintian checks each deb in its target's build image. An error fails the job, apart from those that the packaging repository's allow file accepts: one pattern a line, with a comment above it that says why, as vtium-arch's namcap allow files work. The job prints every warning.
- **The install scripts' tests.** Each packaging repository tests its scripts the way vtium-arch's `test-install-messages` does. The scripts run against a scratch `/etc/systemd/system`, a stand-in `systemctl` and a stand-in `vtium`. The cases are a first install, a refused claim, an upgrade, a removal, and rpm's swap of one console package for another, which D2 describes.
- **An install in a container.** After lint, each job installs its target's packages in a fresh container of that target, with dnf, zypper or apt. The container may reach the distribution's repositories, so the install proves that every dependency name resolves. Then the job checks these points:
  - `vtium --version` names the release's version.
  - Each file `make install` staged is in place.
  - `systemd-analyze verify` accepts the units.
  - The image helper's account exists.
  - The install message says that vtium claimed no VT, because the container is not booted with systemd, which is the path of a refused claim.
  - Swapping `vtium` for `vtium-minspec` and back succeeds.
  - Removing every package leaves only what the family's rules leave, such as Debian's conffiles until a purge.
  
  This is what the first card builds, before any guest exists. Until the guests exist, a release's notes say that its packages are checked in containers only.
- **An install and a start on a guest.** The work that gives Fedora and Debian their tier 3 guests adds one guest per distribution: Fedora, openSUSE Tumbleweed, Debian and Ubuntu. Each guest is pinned the way the Arch guest is. The guest installs the packages from the release's draft, boots, and runs that work's checks: the claim of the free VTs and the display manager's VT, kernel messages from `/dev/kmsg`, the keyboard layout from the family's own file, each login's scope and session in seat mode, and no SELinux or AppArmor denial in the journal. The packages add three checks there:
  - an upgrade from the previous release's packages under a running vtium restarts nothing, needrestart on Ubuntu included;
  - a removal gives the VTs back;
  - on Fedora and openSUSE, a login at vtium's prompt gets the same SELinux context as a login at a getty's prompt.
  
  The guests run at V4 on the draft's files, before the draft is published (ADR-1505). Their screens are tier 3 cases whose goldens stay provisional until the maintainer accepts them (ADR-1503 D10).
- **The security frameworks.** The packages ship no SELinux policy module and no AppArmor profile. A denial that a guest finds is fixed in vtium, or by a policy module in the packaging repository, and the card that finds the denial decides which.

### D8: what stays out for now

- **Build services.** COPR, the Open Build Service and a Launchpad PPA come later. Each one gives a signed package repository, so `dnf upgrade`, `zypper update` and `apt upgrade` would bring each new release in. Each needs an account the maintainer holds.
- **Signatures and debug packages.** Package signatures, and debug symbols for a crash report, wait for a service that signs and serves them.
- **aarch64.** aarch64 packages wait until they can be checked on a guest or on ARM hardware (the architectures probe).
- **Other distributions.** RHEL and its free rebuilds package libseat only through EPEL. The work that gives Fedora and Debian their tier 3 guests keeps its RHEL rebuild's guest, and that distribution is not a target here. Alpine and postmarketOS, which use apk, are a third family to weigh later.
- **The distributions' own archives.** A package in Debian's or Fedora's own archive would follow their rules. Those rules forbid a package that claims the VTs at install, and they want every font built from its source. vtium's packages stay vtium's own.

### D9: when the work is built

- The maintainer said "actual distribution builds come later" (`basis`). The wave that writes this record writes only the record. The changes to the release workflow, the rpm and deb packaging, the containers' install checks and the guests are built in a later wave, after the maintainer accepts this record and creates the two packaging repositories.
- Until that wave lands, a release carries the files it carries today, and no rpm or deb package. The Arch packages go on as ADR-1500 decides.
- Build services stay out in that wave too (D8).

## Consequences

### Positive

- A person on Fedora, openSUSE, Debian or Ubuntu installs vtium with one command and the file from the release page, and the distribution's package manager brings in its dependencies.
- Each package is built from files that vtium's workflow made from the tagged commit, with no network, and anyone can check each one with `gh attestation verify`.
- vtium's Makefile keeps deciding what every package installs, so a new file reaches Arch, Fedora, openSUSE, Debian and Ubuntu at once.
- The container install on each target catches a wrong dependency name or a missing library before a release is published.
- The guests run vtium on each family's PAM stack, keyboard layout file and security framework, which tier 3's Arch guest never shows.

### Negative

- A release run builds the tree 21 times, three consoles on each of seven targets, and the release waits for the slowest target.
- A failed package on one target holds the release for every target.
- No package repository exists, so a person learns of a new release from the release page and downloads each file.
- zypper asks before it installs an unsigned file, and the install guide must say how to skip the question.
- Each family's scripts restate vtium-arch's install script, so a change to the claim's messages lands in three repositories.
- The fonts archive puts Cullet Mono's faces in the packages as built files, which a distribution's own archive would refuse.
- Two more repositories and two more submodule pointers to keep, once the maintainer creates them.

### Neutral

- The release files gain the fonts archive and between 30 and 35 package files, all listed in `SHA256SUMS`.
- `docs/install.md` names each package and the command that installs it on each distribution.
- The work that gives Fedora and Debian their guests adds guests for openSUSE and Ubuntu as well.

## Alternatives Considered

- **Build services first.** Not taken: the maintainer chose files on the release first, and COPR, OBS and a PPA later (`basis`).
- **Each packaging repository builds and publishes on a notice from vtium's release.** The issue that first planned these packages proposed it, with a `repository_dispatch` event or a schedule. Not taken: the maintainer chose packages built and attested by vtium's own release workflow. That needs no token for another repository, and each attestation names vtium's commit.
- **One recipe per package, as on Arch.** Not taken: spec files and `debian/` directories build several binary packages from one source, and one tree unpacked once builds the three consoles and the terminfo entry.
- **One tool that writes both rpm and deb from one description, such as nfpm or fpm.** Not taken: the native tools give each family's lint, its scriptlet macros, its soname dependencies and its sysusers support. A later build service also reads spec files and `debian/` directories, not another tool's description.
- **Each distribution's own Rust.** Not taken for now: Debian 13's rustc is older than vtium's minimum, and one pinned compiler builds every target the same way (the Rust probe).
- **Building Cullet Mono's faces in each target's container.** Not taken: the faces' pins hold the bytes that FontForge 20251009 writes, and the distributions' FontForge writes other bytes (the font files probe).
- **Packages only for regular releases, never for a pre-release.** The issue that first planned these packages proposed it. Not taken: the maintainer chose files on each release, and every release so far is a pre-release.
- **`-git` packages that follow `main`.** Not taken: a file on a release comes from that release's tag. A build service may carry one later.
