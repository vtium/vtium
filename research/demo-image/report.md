# Report: a minimal bootable rootfs that runs vtium (w6-rootfs)

Research agent, 2026-11-22, on the candidate `0236b1b9b` (vtium-w5-candidate, cloned read-only into scratch). Claims are tagged **[measured]**, **[read: source]** or **[inferred]**. Nothing was committed, pushed or written to GitHub. Prototype files are under `/scratch/tmp/claude-1000/vtium-w6-rootfs/`.

## Short answer

Yes, and it is cheap. A prototype boots straight into vtium in under a second, shows images and two vtium VTs, and runs the acceptance pass unchanged inside the guest.

- **Recommendation: option (b).** Build one initramfs from `make install-tree`, Debian's static busybox as init, and the pinned Debian bookworm libraries that `tools/arm/sysroot` already fetches and checks. Boot it with QEMU's `-kernel`/`-initrd`, with no disk image. Keep tier 3's Arch guest for what needs systemd, logind and PAM.
- **Build it in two flavours from one builder.**
  - The CPU image: 12 MB, any host, the default.
  - The GPU image: the same plus bookworm's Mesa, 66 MB, started with `virtio-vga-gl` and a GL display. It shows vtium's GPU path on the host's GPU through virgl.
  - Each flavour reviews its own render path, the way `make vt-accept` and `make vt-accept RENDER_PATH=gpu` do on the desktop.
- **unkernel does not fit.** It is a Linux kernel compiled to WebAssembly that runs in a browser or Node. Its kernel is built without VTs, its user space is emulated aarch64, and it does not run under QEMU. It is ten days old. It is a candidate for #259's later in-browser idea, not for a review fixture.
- **Evidence [measured]:**
  - The initramfs is 11.7 MB with zstd -19, or 38 MB unpacked. The host kernel adds 17.4 MB.
  - From QEMU's start to vtium's drawn login prompt takes 0.85 to 1.01 s (4 runs, 1 vCPU, 512 MB and 256 MB).
  - Tier 3 takes 16.5 s to reach its console on 4 vCPU and 4 GB, and its base image is 1.18 GB.
  - Screendumps: `/scratch/tmp/claude-1000/vtium-w6-rootfs/out/03-gallery.png` (NASA photos through kitty graphics) and `/scratch/tmp/claude-1000/vtium-w6-rootfs/out/04-accept-page1.png` (the acceptance pass's double-size page).
- **The GPU path runs in the same kind of image too [measured].** A second flavour adds bookworm's Mesa (44 packages) and a tree built with `gpu`. It boots under QEMU's `virtio-vga-gl`, where virgl hands the guest's GL to the host's GPU. vtium logged `Display Virtual-1 with backend [drm2d] text renderer [gpu]`. Its gallery capture is pixel-identical to the CPU path's (ImageMagick `compare -metric AE`: 0). The price is size: 66.3 MB compressed, 242 MB unpacked, and a 1 GB guest. Without host GL, vtium refuses Mesa's software renderer on the render node and draws on the CPU path, as designed. So the GPU flavour needs `virtio-vga-gl` and a QEMU display with GL.
- **unkernel's GLES 3.0 claim holds, but only in a browser [read].** Its guest Mesa (virgl) renders through virglrenderer compiled to WebAssembly on the page's WebGL2 context, and the guest sees GLES 3.0 and GL 2.1. Under Node there is no GL at all. Its DRM path lists "dumb buffers used by 3D contexts" as not done, and vtium's GPU path draws into dumb buffers. So vtium's GPU path would most likely fall back to the CPU path there [inferred]. This does not change the recommendation.
- **Defect found [measured]:** vtium segfaults at start when `/usr/share/X11/xkb` does not exist, so its built-in fallback keymap is never reached. See [Risks](#risks).

## 1. unkernel

| | |
|---|---|
| What it is | "A real Linux kernel, compiled to WebAssembly." Linux 7.3.4 with an `arch/wasm` port, plus kwemu, an aarch64 CPU emulator with a JIT that is linked into the kernel. User programs are unmodified aarch64 Alpine or Debian binaries, run on the emulated CPU. [read: README.md] |
| What it builds | `unkernel.wasm` (the kernel bundle), a busybox base initramfs, and Alpine and Debian erofs images. It also builds a TypeScript SDK, an `npx unkernel` CLI for Node, and a web page. The build needs Zig 0.16, Node 24, clang 21 and a patched wasm-ld, takes about 12 minutes, and uses 5 GiB of disk. [read: README.md "Building from source"] |
| License | The kernel, emulator and patches are GPL-2.0-or-later (patches GPL-2.0-only). The SDK and page are MIT, and the npm package is "MIT AND GPL-2.0-only". [read: README.md "License"; `gh repo view`] |
| Maturity | Created 2026-11-12. Last push 2026-11-22. Latest release v0.2.0-261121 on 2026-11-21. 1,078 stars. [read: `gh repo view pithings/unkernel`] It is very young and moves daily. |
| How it boots | The JS host loads the wasm kernel. The kernel unpacks the base initramfs (`/init`, busybox), then mounts a distro erofs as an overlay root. There is no bootloader and no PC. [read: README.md, guest/README.md] |
| Devices | `kwfb` (a lazy `/dev/fb0`), `kwinput` (evdev), `kwsnd`, `kwblk`, `kwnet` and `kwvsock`. With `gl: true, gpu: true`, only in a browser, it adds a virtio-gpu KMS device through `kwvirtio`. [read: README.md "Display, input and sound", "GL in the guest"; kernel/patches/linux/0029-wasm-add-kw_defconfig.patch] |
| VT | `# CONFIG_VT is not set` [read: 0029-wasm-add-kw_defconfig.patch line 160] |

**Fit for vtium: poor.** No kernel VT exists, so vtium would run only on its fake VT (the novt path), with no VT switching. Under the CLI the display is fbdev only, and KMS appears only in a browser with GL. vtium would need its aarch64 build (`make arm` exists), running on an interpreter or JIT of about 250 to 300 million instructions per second. The maintainer can't open it with QEMU, and its kernel config and device model are not ones vtium meets on real machines. [inferred from the above]

**unkernel's GL, and vtium's GPU path there [read: unkernel `.agents/gl.md` (its GL design notes), README "GL in the guest"; vtium `crates/vtium-platform/src/gpu/egl.rs`, ADR-1303].**

- **The stack.** Guest programs run stock Debian Mesa with the `virgl` driver. That driver talks either to vtest over vsock, or to a virtio-gpu device (`/dev/dri/card0` and `renderD128`) behind unkernel's `kwvirtio` transport. On the host side, virglrenderer 1.3.0 is compiled to WebAssembly with emscripten and draws on one WebGL2 context of the page. WebGPU, Zink and Venus are ruled out in its notes.
- **What the guest sees.** "GL 2.1 compatibility (GLSL 1.20) and GLES 3.0 (GLSL ES 3.00): no compute shaders, no texture buffers, no GL 3.3 core." The guest's Mesa is installed with apt, not shipped. `virgl.wasm` is 155 KB brotli. Measured there: kmscube at 55 to 57 fps, and `eglinfo -B -p gbm` reporting virgl.
- **Browser only.** "Node has neither WebGL nor WebGPU." Without WebGL2, its guest falls back to llvmpipe.
- **What vtium needs.** vtium's GPU path needs EGL 1.5, a GBM device on the card's render node, an OpenGL ES 3.0 context, and the import of a dma-buf as an EGL image. vtium draws into the display's dumb buffers (`backend [drm2d] text renderer [gpu]` in the log). GLES 3.0 and the render node are there.
- **What blocks it.** unkernel's notes list "dumb buffers used by 3D contexts" under "Not done". vtium also refuses Mesa's software renderer on a render node, which is measured in the prototype below. So in unkernel, vtium's GPU path would most likely fall back to the CPU path, either at the dumb-buffer import or wherever WebGL2 is missing [inferred, untested]. Running vtium there would also need the aarch64 build, a browser, a Debian guest of 57 MB plus Mesa, and a console with no VTs.

It is relevant to #259 instead. The maintainer ruled out JSLinux on 2026-11-04 and kept "vtium's renderer compiled to WebAssembly" as the later in-browser path [read: #259 comments]. unkernel is a faster member of the class that JSLinux belonged to. Revisit it only for a browser demo, after it stabilizes. [inferred]

## 2. What vtium needs at runtime

| Need | What provides it in the image (option b) | CPU path | GPU path only |
|---|---|---|---|
| A kernel VT | the kernel (`CONFIG_VT=y`). Busybox init's `ttyN::respawn:` line runs `vtium --vt=ttyN --no-switchvt` [read: docs/systemd.md "Giving vtium a VT without systemd"] | yes | |
| KMS/DRM with dumb buffers | QEMU's `virtio-vga` with the kernel's `virtio_gpu` driver. On the Arch kernel this is 3 modules, `virtio_dma_buf`, `virtio_input` and `virtio-gpu`, 63 KB compressed [measured]. `simpledrm` is built in as a fallback. The fbdev path also exists in vtium [read: crates/vtium-platform/src/display/fbdev.rs] | yes | |
| evdev keyboard and mouse | i8042/atkbd built in. QMP `send-key` typed every key in the prototype [measured]. The pointer through vmmouse or virtio-tablet is untested | yes | |
| C runtime | glibc 2.36, libgcc_s, libm, libudev1, libcap2 and libxkbcommon0 from the pinned bookworm sysroot (`tools/arm/pin.tsv`), about 3 MB [measured: ldd of the tree; files copied from `~/.cache/vtium/arm/amd64-9793c2ae1e39596a`]. The binaries need glibc ≤ 2.36 by the Makefile's `check-glibc` [read: Makefile] | yes | |
| xkb data | `/usr/share/X11/xkb` must exist, because vtium crashes without it (see Risks). An empty directory is enough for the built-in US keymap [measured]. Debian's `xkb-data` (6.8 MiB installed) adds every layout [read: bookworm Packages] | yes | |
| udev | libudev's sysfs scan works with no daemon. vtium then sees no `ID_PATH` or `ID_SEAT` and no hotplug, so head-file rules that match a card, connector or path match nothing [read: docs/systemd.md "The seat's devices"; measured: vtium found card0 and both keyboards with no daemon] | yes | |
| login | busybox `login`, which handles `-p` and checks `/etc/passwd` and `/etc/shadow` with no PAM. A `demo` user with an empty password logs in at once [measured] | yes | |
| logind, seat, PAM session | none. vtium's no-systemd path applies: no logind session, and `vtium console` changes nothing [read: docs/systemd.md] | n/a | |
| font helper | `/usr/bin/vtium-config` from the tree (10 MB). It ran as `nobody` (`vtium-config serve`) [measured: ps] | yes | |
| fonts | the tree's `/usr/share/vtium/fonts` (14 MB, mostly unifont_all 8.3 MB and Cullet Mono 5.2 MB) and the built-in fonts | yes | |
| image helper | `/usr/bin/vtium-image` (0.8 MB) and a `vtium-image` line in `/etc/passwd`, because there is no `systemd-sysusers` [read: docs/systemd.md "The image helper's identity"]. Pictures drew [measured] | yes | |
| terminfo | the tree's `xterm-vtium` and `kmscon` entries | yes | |
| shell and tools | Debian `bash` and `libtinfo6` (1.8 MB). busybox supplies coreutils, `stty`, `od`, `base64`, `chvt` and the rest. `/dev/fd` needs a link to `/proc/self/fd`, which udev usually makes. Without it, `vtium-gallery` failed [measured] | yes | |
| sample images | `tools/tier3/showcase-photos/*.png`, NASA, public domain, provenance recorded [read: docs/showcase/README.md] | yes | |
| EGL, GBM and a GL driver | Mesa. vtium dlopens `libEGL` and `libgbm` and links no GL [read: crates/vtium/Cargo.toml, crates/vtium-platform/src/gpu/gbm.rs]. Debian's closure of libegl1, libegl-mesa0, libgbm1 and libgl1-mesa-dri is 45 packages, 209 MiB installed and 50 MiB of debs. The largest are libllvm15 (111 MiB), libicu72 (35), mesa-dri (24) and libz3-4 (22) [measured from bookworm's Packages index; an upper bound, since Depends closures over-count]. In the prototype the set extracted to 209 MB under `usr/lib`, of which LLVM and ICU are 147 MiB [measured]. vtium refuses llvmpipe on a render node ("the render node gives Mesa's software renderer, not a GPU"), so the GPU path in a VM needs `virtio-vga-gl` with virgl and a QEMU display with GL [measured]. Debian's driver is one 25 MB megadriver that links LLVM even for virgl. A Mesa built for virgl alone, without LLVM, would be far smaller, but that is a custom build [inferred]. bookworm's libgbm also needs glibc's `libpthread.so.0` stub; without it vtium logged "libgbm.so.1 cannot be opened … so it draws without the GPU" [measured] | | yes |

## 3. The options

| | (a) unkernel | (b) install tree + busybox + pinned debs (recommended) | (c) Alpine minirootfs or Buildroot | (d) tier 3's guest |
|---|---|---|---|---|
| Size | about 10 MB page plus the aarch64 vtium build [read] | 11.7 MB initramfs plus a kernel (17.4 MB Arch, the prototype) [measured]. The GPU flavour is 66.3 MB compressed and 242 MB unpacked, with bookworm's untrimmed Mesa [measured] | Alpine's base is about 4 MB, but vtium needs a musl build (see below). Buildroot comes out near (b) [inferred] | 1.18 GB base qcow2, after downloading 330 MiB of packages [measured: `~/.cache/vtium/tier3/images/base`; read: tools/tier3/README.md] |
| Boot to a drawn prompt | not measured | 0.85 to 1.01 s [measured] | about (b) [inferred] | about 16.5 s to the console, 19.4 s to login [measured: report.json, integration run of 2026-11-15] |
| CPU path, images, fonts, input, login | fbdev only, no VT switch | all. Two VTs, kitty images, vtium-config and the login prompt were seen; sixel untested | all, once vtium is built for musl | all |
| GPU path (GLES 3.0 on a render node) | browser only: GLES 3.0 through virgl on WebGL2; "dumb buffers used by 3D contexts" not done, so vtium most likely falls back to the CPU path [read; inferred] | runs, with the Mesa flavour under `virtio-vga-gl` (virgl, host GPU): `text renderer [gpu]`, pixel-identical to the CPU path, prompt at about 1.1 s [measured]. Not under llvmpipe, which vtium refuses by design [measured] | Mesa from the distribution, the same virgl need [inferred] | yes, `gl-1280x800` and `gpu-1280x800`, also on `virtio-vga-gl` with `egl-headless` [read: tools/tier3/README.md] |
| systemd, logind, seat, PAM, getty fallback | no | no. This is the no-systemd path that tier 3's `sysvinit-*` variants test | no (Alpine) | yes |
| How the maintainer opens it | a browser or `npx` | `make demo`, which runs `qemu-system-x86_64 -enable-kvm -m 512 -kernel … -initrd … -device virtio-vga -display gtk`. `make demo GPU=1` runs `-m 1024 -device virtio-vga-gl -display gtk,gl=on` | the same | `make tier3-run`, headless, which is not built for interactive use |
| Rebuild after a merge | n/a | `make install-tree` (1 min 38 s cold, measured) plus packing (seconds) | (b) plus a musl toolchain, or a Buildroot build of 30 min or more [inferred] | per run, `tree.tar` on a disc; the base image is rebuilt only when inputs change |
| Licenses | GPL-2.0 and MIT | busybox GPL-2, bash GPL-3, glibc LGPL-2.1, libudev LGPL-2.1, libxkbcommon MIT, kernel GPL-2, Mesa MIT, LLVM Apache-2.0 with the LLVM exception. GPL binaries need their sources offered, and Debian's snapshot archive holds them by version [inferred] | similar | Arch's packages, redistribution untested (#259 asks) |
| Work to build and keep | high, and outside vtium's control | small. One script, a pin file using `tools/arm/sysroot`'s fetch and check, and two make targets | (c) Alpine: a musl target for vtium, which #377 may want anyway. (c) Buildroot: a whole build system | already kept |

Alpine is (b) with musl. vtium's release binaries are built against glibc 2.36 (`check-glibc`), and the GPU path dlopens Mesa, so Alpine needs a musl build of vtium, or gcompat. That makes it an #377 question, not a demo question. [inferred]

## 4. The review inside the image

**What runs unchanged [measured].** `tools/vt/accept.sh` says it "runs in any terminal as any user and needs nothing from vtium" [read: accept.sh header]. In the guest, `ACCEPT_LABEL=demo bash accept.sh 51 70` drew the title page and the double-size page. It keeps its record in `~/.local/state/vtium/acceptance`. Page 98 mentions python3 only in a comment.

**What the guides need [read: tools/vt/checks/*.guide]:**

- All guides need bash.
- `double-size`, `editor` and `insert` run `vim-vt510.py`, which uses only the standard library. They need a python3, and Debian's python3-minimal closure is about 30 MiB installed.
- `daily` needs vim or nano, tmux and systemctl. tmux is about 1 MiB.
- `restart.guide` drives `vtium-dev@` units with sudo and systemctl, so it has no meaning in the image.

**What does not belong in the image.** `vt.sh` and `run-vt.sh` are host wrappers. They pick a free VT, run sudo, and stop and restore units with systemctl. They check sessions with loginctl and drop privileges with setpriv. In the image, vtium already owns the VT and the session is the demo user's own shell, so none of that applies. [read: docs/vt-review.md "What a run does"]

**The smallest change.** Add a command for inside the image, `vtium-review` (a shell script under `tools/demo/guest/`):

- `vtium-review accept [PAGES]` runs `accept.sh`. It sets `ACCEPT_LABEL` from `vtium --version`, `ACCEPT_VT` from `tty`, `ACCEPT_FEATURES` and `ACCEPT_RENDER_PATH` as baked in at build, and `ACCEPT_DIR=/review`.
- `vtium-review check GUIDE` runs `checks/guide.sh` with `GUIDE_VT`.
- `vtium-review list` lists the guides that can run in the image.

`/review` is a 9p share of the host's `target/demo/answers` (`-virtfs local,…,mount_tag=review`, plus the 9p, 9pnet, 9pnet_virtio and netfs modules), so the records land on the host. Without the share, the records stay in the guest's RAM and are lost at power-off. No file in `tools/vt/` has to change. [inferred]

**Version mismatch.** The guides refer to `make vt-install` and to systemd units in a few places, such as restart and daily. `vtium-review list` offers only the guides that run in the image. [inferred]

## 5. The motd sketch

Following AGENTS.md's Writing rules: "you", plain words, short sentences.

```
Welcome to VTium 0.1.0-alpha.11, demo image, rendering on the CPU.

vtium is the terminal on this screen. It runs on a Linux virtual
terminal (VT), and it renders text, colors and images with the CPU.

Things to try:
  vtium-gallery /usr/share/vtium-demo/pictures
      shows two NASA photographs as images in the terminal
  vtium-config
      lets you pick a font set and try it on this console
  chvt 2
      switches to a second vtium console; chvt 1 comes back

To review this build step by step:
  vtium-review accept        walks every acceptance page; answer each
                             one with y, n, u or s
  vtium-review check start   runs the guide named start
  vtium-review list          lists the guides you can run here

Your answers are saved to target/demo/answers on the host, so they
are kept after you turn the VM off. To turn it off, run poweroff.
```

`chvt` is named in place of Ctrl+Alt+F2 on purpose. In a QEMU window on a Wayland desktop, the host's compositor may take Ctrl+Alt+Fn and switch the host's VT. [inferred, untested]

## 6. The prototype

**Built [measured]:**

- Install tree: `make install-tree FEATURES='images bidi filters'` in a scratch clone. It took 1 min 38 s cold and produced 30 MB. The font files came from the integration checkout's `target/font-files` and `target/cullet-mono`, checked by `make font-files-check`.
- Debian bookworm `busybox-static`, `bash` and `libtinfo6`, each checked against the SHA-256 in bookworm's Packages index.
- The libraries from the pinned amd64 sysroot.
- The host's Arch kernel 7.3.4 and its three virtio modules, unpacked.
- `/etc/inittab` with vtium on tty1 and tty2, and an `rcS` that mounts the filesystems, loads the modules and makes the `/dev/fd` links.
- A `demo` user, the motd, the two NASA photos and `tools/vt/` under `/usr/share/vtium-review`.

**Commands:**

```sh
bash /scratch/tmp/claude-1000/vtium-w6-rootfs/scripts/mkroot.sh /scratch/tmp/claude-1000/vtium-w6-rootfs/out
python3 -I /scratch/tmp/claude-1000/vtium-w6-rootfs/scripts/drive.py /scratch/tmp/claude-1000/vtium-w6-rootfs/out    # boots, logs in, gallery, accept pages, VT 2 and back, screendumps
python3 -I /scratch/tmp/claude-1000/vtium-w6-rootfs/scripts/timing.py /scratch/tmp/claude-1000/vtium-w6-rootfs/out 512 # time to the drawn login prompt
# QEMU: -enable-kvm -machine q35 -cpu host -smp 1 -m 512 -kernel /boot/vmlinuz-linux -initrd out/initramfs.cpio.zst
#       -append 'console=ttyS0 quiet loglevel=3' -device virtio-vga,xres=1280,yres=800 -display none -qmp unix:…
```

**Results [measured]:**

| Screendump (`/scratch/tmp/claude-1000/vtium-w6-rootfs/out/`) | Shows |
|---|---|
| `01-login.png` | vtium's login prompt on tty1, about 0.9 s after QEMU started |
| `02-motd.png` | login as demo, then the motd and a bash prompt |
| `03-gallery.png` | `vtium-gallery` drew both PNGs through kitty graphics with their names, which proves the images feature and the image helper |
| `04-accept-title.png`, `04-accept-page1.png` | `accept.sh` running in the guest, including double-width and double-height lines |
| `05-vt2.png` | Ctrl+Alt+F2 shows the second vtium's login prompt |
| `06-back-vt1.png` | Ctrl+Alt+F1 brings back tty1's screen |

**The GPU flavour [measured].**

- The tree was built with `FEATURES='images bidi filters gpu'` (28 s, warm cache).
- The image adds bookworm's Mesa runtime set: 44 packages, resolved from the Packages index, each checked against its SHA-256 and extracted. It also adds glibc's `libpthread`, `libdl` and `librt` stubs, which bookworm's libgbm needs.
- `/etc/vtium/vtium.conf` turns on `hwaccel` and `render-timer=log`.
- vtium's log goes straight to the serial port, and the kernel command line's `VTIUM_DEBUG=--debug` reaches it through init.

```sh
GPU=1 bash /scratch/tmp/claude-1000/vtium-w6-rootfs/scripts/mkroot.sh /scratch/tmp/claude-1000/vtium-w6-rootfs/out-gpu
python3 -I /scratch/tmp/claude-1000/vtium-w6-rootfs/scripts/drive_gpu.py /scratch/tmp/claude-1000/vtium-w6-rootfs/out-gpu        # virgl: -device virtio-vga-gl -display egl-headless,rendernode=/dev/dri/renderD128, -m 1024 -smp 2, VNC capture (tier 3's vnc_capture)
python3 -I /scratch/tmp/claude-1000/vtium-w6-rootfs/scripts/drive_llvmpipe.py /scratch/tmp/claude-1000/vtium-w6-rootfs/out-llvmpipe  # the same image on plain virtio-vga, no host GL
```

| Run | Result |
|---|---|
| virgl | The kernel reports `features: +virgl`, with 2 cap sets. vtium logs `Display Virtual-1 with backend [drm2d] text renderer [gpu], cell size 10x20`, and the first `Display` line appears 1.06 to 1.16 s after QEMU starts (2 runs). The gallery capture `/scratch/tmp/claude-1000/vtium-w6-rootfs/out-gpu/g02-gallery.png` matches the CPU image's `out/03-gallery.png` exactly (`magick compare -metric AE`: 0), as ADR-1303 D1 expects. vtium's render-timer lines report 0.03 to 1.1 ms of CPU per frame. |
| no host GL | vtium logs `head console: the GPU path's set-up failed at the EGL display: the render node gives Mesa's software renderer, not a GPU, so it draws without the GPU`, then `text renderer [cpu]`. The gallery is again pixel-identical. |
| without the libpthread stub (first attempt) | `the GPU path's set-up failed at the GBM device: libgbm.so.1 cannot be opened: libpthread.so.0 …, so it draws without the GPU`. vtium's fallback worked cleanly. |
| sizes | The initramfs is 66.3 MB compressed and 242 MB unpacked. `usr/lib/x86_64-linux-gnu` is 203 MB, of which libLLVM and libicu are 147 MiB. Every Mesa DRI driver is one hardlinked 25 MB file. |

The tty2 vtium also logged `cannot drop DRM master: Invalid argument (os error 22)` while it waited for its VT to be shown. This is not triaged.

**Not tried:**

- The GPU flavour in a window (`-display gtk,gl=on`). Only `egl-headless` ran.
- sixel.
- The pointer.
- Two outputs.
- The 9p share.
- A pinned Debian kernel in place of the host's.
- A guest with 128 MB, which drew no prompt within 30 s. This was not investigated.

## Risks

1. **vtium crashes when xkb's data directory is missing [measured].** With no `/usr/share/X11/xkb`, every vtium start dies with `segfault at 0 … in libxkbcommon.so.0`, a write through a NULL pointer, and init respawns it in a loop. With an empty directory, vtium starts and uses its built-in keymap.
   - Cause [read: `~/.cargo/registry/.../xkbcommon-0.9.0/src/xkb/mod.rs:430`]: the `xkbcommon` crate's `Context::new` stores the result of `xkb_context_new` without checking it for NULL.
   - Inferred, and consistent with the empty-directory result: libxkbcommon returns NULL when it cannot add its default include path.
   - Effect: `crates/vtium-platform/src/input/keyboard.rs:120` calls `xkb::Context::new(xkb::CONTEXT_NO_FLAGS)`, so the fallback keymap in `fallback.xkb` is unreachable on a system without xkb-data.
   - This is a defect in vtium's own input path, so it is card W6-1 below.
2. **The kernel.** The prototype used the host's Arch kernel. A reproducible artifact pins a kernel, for example Debian bookworm's `linux-image-6.1.0-*-amd64` from the snapshot, and takes the few modules it needs. Debian's cloud kernel likely leaves out DRM. Check that the chosen kernel's config has `VT`, `DRM_VIRTIO_GPU` and `DRM_SIMPLEDRM`. [inferred]
3. **No udev daemon.** Head-file rules that name a card, connector or path match nothing, and hotplug is not shown [read: docs/systemd.md]. Adding eudev fixes this if a review needs it. [inferred]
4. **The acceptance pages and screen width [observed in 04-accept-page1.png].** At 128 columns, the double-size page's X sits at column 128, but the page's "right:" text speaks of an 80-character ruler that ends at the right edge. The image should start vtium at a size where the pages read as written, for example a font size that gives 80 columns, or the pages should state their width. This needs a fresh look; it is not settled.
5. **Leftover pictures.** The CPU runs' silent log has been explained: it came from the prototype, not from vtium. Busybox's `sed` has no `-u`, so the pipe that prefixed vtium's lines died at once [measured: `busybox sed --help`]. Writing vtium's log straight to the serial port works.
   - After busybox `clear`, the gallery's pictures stayed on screen while the text cleared (`04-text.png` from an earlier run). Which erase busybox sends, and what the kitty specification asks of it, was not checked. Untriaged; it may be correct behaviour.
6. **Key capture.** Ctrl+Alt+Fn in a QEMU GTK window can reach the host. The motd uses `chvt`. [inferred]
7. **The GPU flavour's weight and its host need.**
   - Size [measured]: 66.3 MB compressed and 242 MB unpacked in RAM. The prototype ran it with 1 GB.
   - Host need [measured]: it shows the GPU path only with `virtio-vga-gl` and a QEMU display that has GL (`egl-headless`, or `gtk,gl=on` in a window). Without host GL it draws on the CPU path.
   - Trimming [inferred]: either a disk image in place of the initramfs, or a Mesa built for virgl alone without LLVM, which would drop most of the 147 MiB that LLVM and ICU take. The second is a source build to keep.
8. **License duty for a published image** (#259). GPL components need their sources, or a written offer, beside the image. Debian's snapshot archive gives exact source versions. [inferred]

## Proposed wave 6 cards

| Card | What | Files | Level |
|---|---|---|---|
| W6-0, record | An ADR in the project domain (15xx): the demo image. Base (b), its pins, what it shows and what it leaves to tier 3 (systemd, logind, PAM, seat mode), and how it relates to #259's release image and #377. The maintainer reads it before W6-2 starts. | `docs/architecture/project/ADR-15xx-*.md` (via `docs/scripts/adr`) | records |
| W6-1, xkb fix | vtium falls back to its built-in keymap when libxkbcommon cannot make a context, in place of the segfault. Check the context pointer, or retry with `CONTEXT_NO_DEFAULT_INCLUDES`, with a test seen red first (a context with a missing `XKB_CONFIG_ROOT`) | `crates/vtium-platform/src/input/keyboard.rs` (+ tests) | V1 |
| W6-2, builder | `tools/demo/`: `mkroot` (pack the tree, busybox, bash and the libs into a reproducible cpio with `SOURCE_DATE_EPOCH`), `pin.tsv` rows for busybox-static, bash, libtinfo6, xkb-data and a Debian kernel, fetched and checked the way `tools/arm/sysroot` does. Make targets `demo-image`, `demo` (a QEMU window) and `demo-check` (headless boot that checks the login prompt and gallery pixels are drawn, with no golden) | `tools/demo/**`, `Makefile` (demo targets only), `tools/demo/tests/` under `make test-tools` | V1 (`make test-tools`, `make demo-check`) |
| W6-3, review in the image | `vtium-review`, the motd, the 9p answers share, a `vtium-review list` that offers only the guides that run there, python3-minimal and tmux in the image for the guides | `tools/demo/guest/**`, `docs/vt-review.md` (a section "In the demo image") | V1 (headless `vtium-review accept` with `ACCEPT_NOPROMPT=1`, `make test-tools`) |
| W6-4, the GPU flavour | `make demo-image GPU=1` and `make demo GPU=1`: the tree with `gpu`, bookworm's Mesa runtime set pinned in `tools/demo/pin.tsv` (with glibc's libpthread, libdl and librt stubs), `hwaccel` on, QEMU with `virtio-vga-gl` and `-display gtk,gl=on` (window) or `egl-headless` (check). Optional later step: a disk image or a virgl-only Mesa to cut the 242 MB | `tools/demo/**` | V1: a headless virgl boot whose `Display` line names `text renderer [gpu]`, and whose gallery capture equals the CPU image's |
| W6-5, docs | A contributor page on how to boot the demo and run a review in it. The user-facing "Try vtium in a VM" tutorial stays with #259 at beta | `docs/` page, `docs/scripts/prose --check` | docs gates |

W6-1 and W6-2 can run in parallel. W6-3 and W6-4 wait for W6-2.
