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
  - evidence: "The survey of browser emulators on 2026-11-22, which booted the demo image's layout on each: v86 boots Debian bookworm's i386 kernel, whose bochs driver gives vtium real VTs, KMS with dumb buffers and a PS/2 keyboard, and Ctrl+Alt+F2 switched to a second vtium; vtium's first frame came 7.6 s after the page loaded, and generating Terminus at 8×16 took 50.5 s. unkernel ran the aarch64 build with no VTs (`vt: no VTs: using a fake VT`); vtium drew its CPU path on fbdev and on a virtio-gpu card's dumb buffers, and its GPU path through virgl on WebGL2 in headless Chromium; the font and image helpers did not start, because seccomp answered ENOSYS. unkernel's download is its kernel, 4.4 MB compressed, its page code, 0.12 MB, and the CPU initramfs, 11.9 MB; the GPU initramfs is 82 MB and ran in 1,536 MiB of guest memory. container2wasm showed only a serial console, and CheerpX's system calls made vtium panic at start."
  - evidence: "The measurement of v86 against unkernel on 2026-11-22, in headless Chromium 157, medians from the key event to the first changed animation frame, at 1024×768: a key at the prompt 28 ms on v86 and 23 ms on unkernel; Enter on the bottom line, which scrolls, 85 and 23 ms; `seq 1 2000` until drawn 397 and 222 ms; the gallery until drawn 1,114 and 362 ms; each Down arrow in `vtium-config tui` 130 to 280 and 88 to 131 ms; vtium's first frame 7.6 to 12 s on a cold boot and 0.3 to 0.5 s; Terminus ready 57 to 70 s and 1.4 to 2 s of page time, 399 ms after vtium asked on unkernel. At 800×600 on v86, Enter on the bottom line took 57 ms. No v86 setting but the display size changed the times, and turning v86's JIT off made every action 5 to 10 times slower. unkernel ran in headless Firefox 160 too. A prototype build that let the helpers run on a kernel without seccomp drew Terminus and both gallery pictures on unkernel."
  - evidence: "MDN's browser-compat-data 8.1.10 of 2026-11-19, read 2026-11-22: `WebAssembly.Suspending` (JSPI) without a flag from Chrome 137, Edge 137, Chrome for Android 137, Firefox 153, Firefox for Android 153, Safari 27 and Safari on iOS 27; caniuse gives 77% of global usage. unkernel's README: one CPU on the page's thread needs no cross-origin isolation; its kernel and emulator are GPL-2.0, and its SDK is MIT."
  - precedent: ADR-1500
  - precedent: ADR-1502
  - precedent: ADR-1505
  - precedent: ADR-1508
agent:
  name: Claude
  model: claude-opus-5-5
considered:
  - operator: aaronsb
    said: "v86 works, but it feels pretty sluggish"
    via: the maintainer, after trying the v86 prototype page in the steering session on 2026-11-22, as the steering session's notes of that day record it; a study then measured v86 against unkernel
  - operator: aaronsb
    said: "I thin unkernel makes sense. it gives us a chance to add some configurables to vtium, basically disabling safety."
    via: the maintainer, after the study of v86 against unkernel, in the steering session on 2026-11-22, as the steering session's notes of that day record it; the rest of the answer, on the helpers' sandbox, is recorded in ADR-1200 and ADR-1101
  - operator: aaronsb
    said: "Message only"
    via: the maintainer, answering the fallback probe through the question tool in the steering session, 2026-11-22
    covers: [fallback]
  - operator: aaronsb
    said: "locale scripts that just switch modes"
    via: the maintainer, in the steering session on 2026-11-22, when the demonstrator was first planned
    covers: [what the scripts do]
  - operator: aaronsb
    said: "the scripts are intended to be more for test or verification, not something that ships with vtium. therefore, vtium shouldn't be built to accomodate them."
    via: the maintainer, in the steering session on 2026-11-22, on whether vtium switches the keyboard layout for the locale mode scripts, as the steering session's notes of that day record it
status: accepted
date: 2026-11-22
deciders:
  - aaronsb
related:
  - ADR-1303
  - ADR-1503
  - ADR-1509
  - ADR-1200
  - ADR-1101
---

# ADR-1511: A demo image built from each release's tree boots into vtium, and a page on vtium's site boots it in a browser

## Summary

- **Decided:** vtium gets a demo image, a small Linux system that boots straight into vtium, so that a person can try vtium without installing it. vtium's own tools in `tools/demo/` build the image from each release's install tree. The image carries a few outline and bitmap fonts with ready-made font sets and looks, the keyboard layouts and locale data its scripts need, and sample pictures. It also carries the locale mode scripts: each one switches the session into one locale and returns, and one of them is a right-to-left language. The scripts live in vtium's tree as a testing tool, and the builder copies them in. A login message (motd) says what to try and how to start the review that runs inside the image. Each release rebuilds the image. A page on vtium's GitHub Pages site boots the image in the browser on unkernel, a tool that runs Linux compiled to WebAssembly, and links unkernel as that tool. unkernel needs a browser with JSPI, and whether the page falls back to v86 in an older browser is open. The browser image draws with vtium's CPU path first. A download for QEMU and a GPU flavour come later.
  > Note 2026-11-24: the maintainer decided on 2026-11-22 that the locale mode scripts are tools for testing and verification, and that vtium is not built to suit them (`considered`). A script names the keyboard layout and does not switch it (D3), so where this bullet says the image carries "the keyboard layouts and locale data its scripts need", the scripts need only the locale data.
- **Trades away:** The image is not the system vtium runs on in daily use. It has no systemd, logind, PAM or seat manager, so tier 3 goes on judging those. Each release gains a build step and a page to publish, and the image's GPL programs need their sources offered beside it. The fonts, layouts and locale data make the image larger than vtium needs in order to start. In the browser, vtium runs inside an emulator that the project does not control, with no VTs, and its font and image helpers run without their seccomp filters and landlock domain there.
  > Note 2026-11-24: the locale mode scripts need no keyboard layouts (D3). So of "The fonts, layouts and locale data", the fonts and the locale data are what make the image larger.
- **One-way?** No. The image is built output, and dropping it costs a build step and a page. Once people link to the launch page, its address is worth keeping.
- **Probes:**
  - *Confident (what the scripts do):* each locale mode script only switches the keyboard layout, the font set and `LANG`, and returns. It shows no lesson and no sample text. Is that the scope you want? (Answered 2026-11-22; see `considered`.)
  - *Not confident (fallback):* unkernel runs only in browsers with JSPI: Chrome and Edge from 137, Firefox from 153 and Safari from 27. In an older browser, the page could boot the same demo on v86 at 800×600 from a saved state, which is slower but runs everywhere. Should the page fall back to v86, rather than show a message that names the browsers that work? (Answered 2026-11-22; see `considered`.)
- **Inversion:** Between "a person installs vtium to try it", which means replacing the login prompt on a VT of their own machine, and "vtium ships a distribution of its own". The demo image is a minimal system built only to show vtium and to run its review, and each release rebuilds it.

## Context

vtium runs on a Linux virtual terminal (VT) in place of getty, the program that normally displays the login prompt. Trying it today means installing it on a machine and giving it a VT, which is a large step for someone who only wants to look. Earlier work planned a reusable image for reviewing vtium in a virtual machine, and that work now continues as this record.

A research prototype showed that a small image is cheap (`basis`). It packed vtium's install tree, a static busybox and a few pinned libraries into one initramfs of 11.7 MB, an archive the kernel unpacks into memory at boot. QEMU booted it to vtium's login prompt in about a second. vtium showed pictures and two VTs, and the acceptance pages of the VT review ran unchanged inside it. A second flavour with Mesa ran vtium's GPU path under QEMU's virgl device and drew the same pixels.

The maintainer asked for a page on vtium's site that boots the image in a browser, with a download for QEMU later (`basis`). The maintainer also asked for fonts with ready-made font sets and looks, for scripts that switch the session into another locale, and for each release to rebuild the image. The emulator that runs the image in a browser is a tool, not part of vtium.

Several projects run a Linux kernel in a browser, and they differ in what vtium needs. A survey tried four of them (`basis`). v86 emulates a 32-bit x86 PC, boots an ordinary kernel and initramfs, and gives vtium real VTs and KMS. unkernel compiles Linux itself to WebAssembly and runs aarch64 programs, with a KMS device but no VTs. container2wasm showed only a serial console, and CheerpX's own system calls crashed vtium at start. The maintainer tried a prototype on v86 and found it sluggish, so a second study measured v86 against unkernel (`basis`). On unkernel's kernel, which is built without seccomp, vtium's font and image helpers refuse to start, so vtium drew only its built-in font and showed no pictures until a prototype let the helpers run there.

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
  > Note 2026-11-24: the scripts name the keyboard layout and do not switch it (D3, the maintainer's decision of 2026-11-22), so they need no xkb layouts. Of what this bullet lists, "The xkb layouts and the C library's locale data", the scripts need only the C library's locale data.
- **Sample pictures,** so that a person sees vtium show images. The prototype used two public-domain NASA photographs that tier 3's showcase already uses.
- **A login message.** The motd says what to try, lists the locale mode scripts, and says how to start the review inside the image.
- **The review inside the image.** The VT review's acceptance pages and the guides that can run there, started by one command in the image. Guides that need systemd or sudo stay out.
- **Each file's license.** The image lists the license of each program, font and data file it carries. Each release offers the sources of the image's GPL programs beside the image, at their exact versions.

### D3: the locale mode scripts

- **Each script switches the session into one locale and returns.** It sets a font set that covers the locale's script and `LANG`, and names the keyboard layout that the locale uses. It shows no lesson, no sample text and no guided steps. The scripts are tools for testing and verification, so vtium gets no way to switch a console's keyboard layout for them.
  > Edited 2026-11-22: the maintainer decided that the scripts are test and verification tools and that vtium is not built to suit them (`considered`). So a script names the keyboard layout and does not switch it. The bullet's second sentence read: "It sets the keyboard layout, a font set that covers the locale's script, and `LANG`."
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

### D6: the page runs the demo on unkernel

The maintainer chose unkernel after a study measured it against v86 on the same image content (`considered`, `basis`). unkernel is faster at every action the study timed, and most where vtium redraws the whole screen. A typed key reaches the screen within one or two animation frames on both, so the difference shows when the screen scrolls, a TUI redraws or pictures arrive. The medians at 1024×768, with the helpers running on unkernel as the prototype let them:

| Action | v86 | unkernel |
|---|---|---|
| A key typed at the prompt, until it is drawn | 28 ms | 23 ms |
| Enter on the bottom line, which scrolls the screen | 85 ms | 23 ms |
| `seq 1 2000`, until the screen is drawn | 397 ms | 222 ms |
| The gallery's two pictures, until they are drawn | 1,114 ms | 362 ms |
| The Down arrow in `vtium-config tui` | 130 to 280 ms | 88 to 131 ms |
| vtium's first frame after the page loads | 7.6 to 12 s, on a cold boot | 0.3 to 0.5 s |
| The font's glyphs ready, after vtium asks for them | about 50 s | 0.4 s |

- **No saved state.** v86 needs a saved state of the guest, taken after its glyphs are made, to hide 50 s of glyph generation on each visit. unkernel boots the image itself, so the page loads the kernel and the initramfs and nothing else. The browser downloads about 16.5 MB before vtium's login prompt: unkernel's kernel is 4.4 MB compressed, its page code 0.12 MB, and the CPU image 11.9 MB.
- **The CPU path first.** vtium's CPU path draws on unkernel's display. In a browser with WebGL2, unkernel also offers a KMS device through virgl, and there vtium's GPU path runs with Mesa in the image. The GPU image is 82 MB, needs 1,536 MiB of guest memory, and its speed is untimed. So the first browser image is the CPU image that D1 builds, and a GPU page follows with the GPU flavour.
- **The release's aarch64 build.** unkernel runs aarch64 programs, so the browser image packs the release's aarch64 binaries with the same tree, as D5 allows.
- **No VTs.** unkernel's kernel has no VTs, so vtium runs one console without a VT, and the page shows no switching between consoles. busybox's init starts one vtium there in place of D1's two. The download for QEMU keeps D1's kernel with VTs.
- **Browsers with JSPI.** unkernel needs JSPI, WebAssembly's interface for suspending a call into JavaScript. It ships turned on in Chrome and Edge from version 137, Firefox from 153 and Safari from 27, about 77% of browsers in use in November 2026. The page checks for it with unkernel's own support check before it boots anything. When the check fails, the page boots nothing and shows a message that names the browsers that work. The page carries no second emulator (the maintainer, 2026-11-22).

> Edited 2026-11-22: the maintainer answered the fallback probe against its lean. The bullet's last sentences read: "Whether a failed check falls back to v86 is the Summary's fallback probe. The lean is yes: v86 at 800×600, resumed from a saved state, where scrolling and redraws take about two thirds of their time at 1024×768 and the screen still holds 100×37 cells."
- **The helpers on unkernel's kernel.** The kernel is built without seccomp and without landlock. vtium's font and image helpers then run without their filters and their landlock domain, and vtium logs the reason, as ADR-1200 D15 and ADR-1101 D9 decide. The image's vtium.conf keeps the sandbox setting at its default, so its log names the kernel as the reason. The guest runs inside the browser's own sandbox, and only the visitor, who already has a shell, can send it a font or a picture.
- **Static files.** With one emulated CPU on the page's own thread, unkernel needs no cross-origin isolation, so GitHub Pages serves the page as plain files.
- **The kernel's source.** unkernel's kernel and emulator are GPL-2.0, and its SDK and page code are MIT. The launch page offers the source of the exact kernel it serves, beside the sources of the image's GPL programs (D2).
- **A young project, pinned.** unkernel was first published on 2026-11-12, ten days before this choice, and it releases daily. The page pins one release of unkernel, and a release of vtium moves the pin only once its image boots on the new one.

> Edited 2026-11-22: the maintainer chose unkernel after the study of v86 against unkernel (`considered`). D6 was headed "which emulator, an open question". It named the survey of unkernel, v86, container2wasm and WebVM, and set four criteria for the choice: the emulator boots vtium's CPU path or its GPU path, keyboard input reaches vtium as the kernel's input events, the size the browser downloads, and a license that lets vtium's site host or link the emulator. unkernel meets each of them. D6 also said that when the chosen emulator ran only vtium's GPU path, the browser image would be built with the GPU path, and the Summary asked whether that order was right. The study found that unkernel runs both paths, so the CPU image comes first and that probe is answered. The earlier text is in git at 837ff5cc2.

## Consequences

### Positive

- A person can try vtium in a browser tab, without installing it or giving it a VT.
- Each release's demo shows that release's features, and a release whose tree no longer boots shows up when the image is built.
- The locale mode scripts serve the VT tours and tests as well as the demo.
- The image boots in about a second under QEMU, so the review inside it starts quickly. In the browser, vtium draws its first frame half a second after the page loads, with the font's glyphs ready about a second later.

### Negative

- Each release gains a build step and a page to publish, and the release waits for both.
- The image's GPL programs need their sources offered, and every font and data file needs its license listed.
- The image has no udev daemon, so head-file rules that match a card, a connector or a path match nothing, and plugging in a display shows nothing.
- The launch page depends on unkernel, which another project maintains, which is young, and which releases daily, so the page pins a release and each move of the pin needs a boot of the new image.
- The browser demo has no VTs, so a visitor cannot try switching between consoles there.
- A browser without JSPI cannot run unkernel, so it gets the fallback or a message.
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
- **v86 as the emulator.** Not taken: it boots the image with real VTs and runs in every browser with WebAssembly, but the actions that redraw the whole screen take 1.8 to 3.7 times as long there as on unkernel, and the maintainer found it sluggish. Its GPU path cannot run, since v86 has no GL device. It may stay as the fallback for browsers without JSPI, which the fallback probe asks.
- **container2wasm or CheerpX.** Not taken: container2wasm's emulators show only a serial console in the page, and CheerpX implements Linux's system calls itself, which crashed vtium at start. CheerpX's own license also limits who may host it.
