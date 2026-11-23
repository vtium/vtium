---
contract: adr/v1
kind: decision
verb: add
capability: process
basis:
  - operator: aaronsb
    level: directed
    said: "browser for now, download only. Unknernel's readme has an example to a live environment. I believe we could launch our own from our github docs pages"
    via: the maintainer's answer in the steering session on 2026-11-22, about a page that launches the demo image, as the steering session's notes of that day record it
  - operator: aaronsb
    level: directed
    said: "later, we can work out how to offer a downloadable item for qemu"
    via: the maintainer's answer in the steering session on 2026-11-22, as the steering session's notes of that day record it
  - operator: aaronsb
    level: directed
    said: "When positioning the webasm experience we don't have to explain how it all works, just a reference to the tool mostly. We are using u kernel more as a tool than any part of vtium"
    via: the maintainer in the steering session on 2026-11-22, as the steering session's notes of that day record it
  - operator: aaronsb
    level: guided
    said: "a small directory of scripts in the demo to change some other common locales like our demo shows (allowing a user to interact in one of the languages, possibly an rtl language)"
    via: the maintainer in the steering session on 2026-11-22, as the steering session's notes of that day record it
  - operator: aaronsb
    level: directed
    said: "The scripts just switch modes basically"
    via: the maintainer in the steering session on 2026-11-22, clarifying what the locale scripts do, as the steering session's notes of that day record it
  - operator: aaronsb
    level: directed
    said: "These scripts may be useful for testing so we can include them as part of vtium but copy them into the rootfs with demo instructions"
    via: the maintainer in the steering session on 2026-11-22, on where the locale scripts live, as the steering session's notes of that day record it
  - operator: aaronsb
    level: guided
    said: "we could put a couple outline and bitmap fonts in there too, with some preconfigured frits/looks"
    via: the maintainer in the steering session on 2026-11-22, as the steering session's notes of that day record it
  - operator: aaronsb
    level: directed
    said: "If we do this right, future releases just build the new rootfs and afford an updated experience on new features shipped"
    via: the maintainer in the steering session on 2026-11-22, as the steering session's notes of that day record it
  - operator: aaronsb
    level: directed
    said: "Build the demonstrator's whole first slice now: this record, the locale mode scripts, the builder with its fonts and font sets, the CPU image, the review inside the image and the release rebuild, and the browser page, on the emulator that looks best."
    via: the maintainer's answers that opened the wave after 0.1.0-alpha.11, 2026-11-22, as the steering session's notes of that day record them
    paraphrase: true
  - operator: aaronsb
    level: guided
    said: "Depending on how the browser emulator works, vtium's GPU path may be easier to bring up there than its CPU path."
    via: the maintainer in the steering session on 2026-11-22, as the steering session's comment on the GitHub issue for the demo image records it
    paraphrase: true
  - evidence: "A research prototype on 2026-11-22, kept on the branch research/demo-image at 44ccb7c36 (research/demo-image/report.md): one initramfs packed from `make install-tree`, Debian's static busybox as init and login, and the pinned bookworm libraries that tools/arm/sysroot fetches and checks. QEMU booted it with -kernel and -initrd. Measured: 11.7 MB compressed with zstd -19, 38 MB unpacked; 0.85 to 1.01 s from QEMU's start to vtium's drawn login prompt (4 runs, 1 vCPU); two vtium VTs, kitty graphics pictures through the image helper, vtium-config, and tools/vt/accept.sh running unchanged in the guest. A GPU flavour with bookworm's Mesa (44 packages) was 66.3 MB compressed and 242 MB unpacked, logged `text renderer [gpu]` under QEMU's virtio-vga-gl, and drew the gallery pixel for pixel as the CPU image did. With no udev daemon, vtium found its card and keyboards, but head-file rules that match a card, a connector or a path match nothing. vtium crashed when /usr/share/X11/xkb did not exist; 0.1.0-alpha.11 fixes it."
  - evidence: "The same report on tier 3's guest for comparison: its base image is 1.18 GB, and it reaches its console in about 16.5 s on 4 vCPU and 4 GB. It is headless and not built for interactive use."
  - evidence: "The same report's reading of unkernel at its v0.2.0 release (created 2026-11-12): a Linux kernel compiled to WebAssembly that runs unmodified aarch64 user programs on an emulated CPU. Its kernel configuration sets no CONFIG_VT. It offers a KMS device only in a browser with WebGL2, through virgl, and lists dumb buffers for 3D contexts as not done; elsewhere its display is fbdev. Its kernel and emulator are GPL-2.0-or-later, and its SDK and page are MIT."
  - precedent: ADR-1500
  - precedent: ADR-1502
  - precedent: ADR-1505
  - precedent: ADR-1508
agent:
  name: Claude
  model: claude-opus-5-5
status: proposed
date: 2026-11-22
deciders:
  - aaronsb
related:
  - ADR-1303
  - ADR-1503
  - ADR-1509
---

# ADR-1511: A demo image built from each release's tree boots into vtium, and a page on vtium's site boots it in a browser

## Summary

- **Decided:** vtium gets a demo image, a small Linux system that boots straight into vtium, so that a person can try vtium without installing it. vtium's own tools in `tools/demo/` build the image from each release's install tree. The image carries a few outline and bitmap fonts with ready-made font sets and looks, the keyboard layouts and locale data its scripts need, and sample pictures. It also carries the locale mode scripts: each one switches the session into one locale and returns, and one of them is a right-to-left language. The scripts live in vtium's tree as a testing tool, and the builder copies them in. A login message (motd) says what to try and how to start the review that runs inside the image. Each release rebuilds the image. A page on vtium's GitHub Pages site boots the image in the browser with an emulator, which the page links as the tool that runs the demo. Which emulator is open until a survey reports, and this record sets the criteria for the choice. A download for QEMU and a GPU flavour come later.
- **Trades away:** The image is not the system vtium runs on in daily use. It has no systemd, logind, PAM or seat manager, so tier 3 goes on judging those. Each release gains a build step and a page to publish, and the image's GPL programs need their sources offered beside it. The fonts, layouts and locale data make the image larger than vtium needs in order to start. In the browser, vtium runs inside an emulator that the project does not control.
- **One-way?** No. The image is built output, and dropping it costs a build step and a page. Once people link to the launch page, its address is worth keeping.
- **Probes:**
  - *Confident (what the scripts do):* each locale mode script only switches the keyboard layout, the font set and `LANG`, and returns. It shows no lesson and no sample text. Is that the scope you want?
  - *Not confident (the browser image's render path):* the emulator that looks best may run only vtium's GPU path. The browser image is then built with the GPU path, ahead of the GPU flavour for QEMU. Is that the order you want?
- **Inversion:** Between "a person installs vtium to try it", which means replacing the login prompt on a VT of their own machine, and "vtium ships a distribution of its own". The demo image is a minimal system built only to show vtium and to run its review, and each release rebuilds it.

## Context

vtium runs on a Linux virtual terminal (VT) in place of getty, the program that normally displays the login prompt. Trying it today means installing it on a machine and giving it a VT, which is a large step for someone who only wants to look. Earlier work planned a reusable image for reviewing vtium in a virtual machine, and that work now continues as this record.

A research prototype showed that a small image is cheap (`basis`). It packed vtium's install tree, a static busybox and a few pinned libraries into one initramfs of 11.7 MB, an archive the kernel unpacks into memory at boot. QEMU booted it to vtium's login prompt in about a second. vtium showed pictures and two VTs, and the acceptance pages of the VT review ran unchanged inside it. A second flavour with Mesa ran vtium's GPU path under QEMU's virgl device and drew the same pixels.

The maintainer asked for a page on vtium's site that boots the image in a browser, with a download for QEMU later (`basis`). The maintainer also asked for fonts with ready-made font sets and looks, for scripts that switch the session into another locale, and for each release to rebuild the image. The emulator that runs the image in a browser is a tool, not part of vtium.

Several projects run a Linux kernel in a browser, and they differ in what vtium needs. unkernel compiles Linux to WebAssembly and has no VTs (`basis`). v86 emulates an x86 PC and boots an ordinary kernel and initramfs, and whether its display works with vtium is unverified. A survey of the candidates runs beside this record.

Four words recur below. The **demo image** is the system that the builder makes. A **locale mode script** switches the session into one locale. The **launch page** is the page on vtium's site that boots the image. The **emulator** is the program that runs the image in the browser.

## Decision

### D1: what the demo image is, and how it is built

- **vtium's own tools build the image,** in `tools/demo/`, from the install tree that `make install-tree` stages for a release. The image adds a static busybox, which serves as init, login and the basic commands, and a few libraries the binaries need.
- **Every input is pinned and checked.** The builder fetches the libraries and programs from a pinned Debian release and checks each one, as `tools/arm/sysroot` does for the ARM sysroot. The kernel is pinned too, and its configuration has VTs and the display drivers the image needs. Two builds from the same tree and the same pins give the same image.
- **The image is one compressed initramfs and a kernel,** with no disk image. busybox's init starts vtium on two VTs in place of getty, and a demo user logs in with no password.
- **The image runs vtium's path without systemd,** the one that tier 3's variants without systemd also test. It has no logind session, no PAM, no seat manager and no udev daemon. Tier 3 keeps judging everything those provide.

### D2: what the image carries

- **Fonts, font sets and looks.** A few outline and bitmap fonts whose licenses let vtium redistribute them, taken from the pinned fonts that vtium's tests already use. The image's vtium.conf defines ready-made font sets and looks, so a person can switch looks with vtium-config without installing anything.
- **Keyboard layouts and locale data.** The xkb layouts and the C library's locale data that the locale mode scripts need, and no more.
- **Sample pictures,** so that a person sees vtium show images. The prototype used two public-domain NASA photographs that tier 3's showcase already uses.
- **A login message.** The motd says what to try, lists the locale mode scripts, and says how to start the review inside the image.
- **The review inside the image.** The VT review's acceptance pages and the guides that can run there, started by one command in the image. Guides that need systemd or sudo stay out.
- **Each file's license.** The image lists the license of each program, font and data file it carries. Each release offers the sources of the image's GPL programs beside the image, at their exact versions.

### D3: the locale mode scripts

- **Each script switches the session into one locale and returns.** It sets the keyboard layout, a font set that covers the locale's script, and `LANG`. It shows no lesson, no sample text and no guided steps.
- **One of the locales is written right to left,** so a person can try vtium's right-to-left text.
- **The scripts live in vtium's tree** beside the VT review's tools, as a testing tool that tours and tests can use too. A test of their own runs under `make test-tools`.
- **The builder copies the scripts into the image,** and the motd and the image's instructions say how to use them.

### D4: each release rebuilds the image

- **The release workflow builds the image** from the release's tree and publishes it with the launch page, so the demo always shows the newest release. AGENTS.md's release step gains this once the builder exists.
- **A change that ships a feature adds what shows it,** such as a look, a locale mode script or a line of the motd, in the same change.

### D5: the launch page

- **A page on vtium's GitHub Pages site boots the image in the browser** with the emulator. The site is the one ADR-1502 decides.
- **The page links and credits the emulator as the tool that runs the demo,** and does not explain how it works. The emulator is not part of vtium: no record treats it as a component of vtium, and vtium's code does not depend on it.
- **The emulator boots the image that D1 builds,** or the same tree built in the form the emulator needs, such as another architecture.
- **A download comes later.** A download of the image with a QEMU command, and a GPU flavour for QEMU, follow the launch page.

### D6: which emulator, an open question

The survey of browser emulators checks unkernel, v86, container2wasm and WebVM, and builds a prototype on the best one or two. The maintainer picks the emulator from the survey. This record decides the criteria:

1. **It boots vtium's CPU path or its GPU path.** The CPU path needs a KMS device that gives dumb buffers, the plain memory buffers that the CPU draws into. The GPU path needs EGL, GBM and OpenGL ES 3.0 on a render node (ADR-1303). Kernel VTs let the page show switching between consoles. Without them, vtium runs without a VT.
2. **Keyboard input reaches vtium** as the kernel's input events, so a person can type in each locale's layout.
3. **Size:** how much the browser downloads before vtium's login prompt appears.
4. **License:** the emulator's terms let vtium's site host or link it beside the image.

When the emulator chosen runs only vtium's GPU path, the browser image is built with the GPU path. The second probe asks the maintainer about this order.

## Consequences

### Positive

- A person can try vtium in a browser tab, without installing it or giving it a VT.
- Each release's demo shows that release's features, and a release whose tree no longer boots shows up when the image is built.
- The locale mode scripts serve the VT tours and tests as well as the demo.
- The image boots in about a second under QEMU, so the review inside it starts quickly.

### Negative

- Each release gains a build step and a page to publish, and the release waits for both.
- The image's GPL programs need their sources offered, and every font and data file needs its license listed.
- The image has no udev daemon, so head-file rules that match a card, a connector or a path match nothing, and plugging in a display shows nothing.
- The launch page depends on an emulator that another project maintains, and some candidates are young and change often.
- A change that ships a feature also costs the content that shows it in the image.

### Neutral

- The image is not a tier of ADR-1505's verification ladder, and holds no golden. A check that the image boots belongs to the builder's work.
- The image's vtium.conf is a file in vtium's own format, so ADR-1510's rules apply to it as to any other vtium.conf.

## Alternatives Considered

- **Tier 3's guest as the demo.** Not taken: its base image is 1.18 GB, it takes about 16.5 s to reach its console, and it is headless.
- **An Alpine or Buildroot system.** Not taken: Alpine uses musl, and vtium's release binaries are built against glibc, so Alpine needs a second build of vtium. Buildroot is a whole build system to keep.
- **A download for QEMU first.** Not taken: the maintainer chose the browser first and the download later.
- **vtium's terminal and renderer compiled to WebAssembly, with no Linux beneath them.** Not taken for this record: an emulator boots the same image that a release builds, with vtium's own platform code. A WebAssembly port would be a second build of vtium to keep. This is this record's agent's call.
- **Locale mode scripts that live only in the image.** Not taken: the maintainer placed them in vtium's tree so that tests and tours can use them.
