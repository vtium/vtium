# tier3

Tier 3 for vtium: vtium's install tree, installed in a pinned Arch Linux guest under QEMU/KVM and started by systemd on a VT, as a system would run it, with a real kernel VT and a virtio-gpu KMS device. A script logs in over the emulated keyboard, draws fixed screens, captures each one and compares it with its golden. The exit status is the verdict. The harness is also the way to put any test content on a real VT: a screen is a shell script in `guest/screens/`.

The goldens are the C reference's, copied into `c-goldens/` from the kmscon fork's `fork/tier3/golden`, but for the small displays, which the C reference never drew: their goldens are vtium's own, in `goldens/`. The harness itself came from the C fork's `fork/tier3` at c631dbab, which is frozen; it lives and changes here. No run or self-test of tier 3 reads a kmscon or libtsm checkout or runs their code; `make tier3-provenance` reads a kmscon checkout, for review only (The copies).

## Running it

```sh
make tier3-image             # fetch, verify and provision the guest image, unless the cache holds one built from the same inputs
make tier3-minimal           # the install tree, then the minspec variants: 1 vCPU, 512 MB, unifont, a PSF font, 320x240 and 640x480
make tier3                   # the install tree, then every variant; VARIANTS="..." for some
make tier3-run VARIANTS=2d-potato-min   # the last run's tree, or TREE=DIR, without building
make tier3 JOBS=2            # up to two variants at once, each in its own VM
make install-tree DESTDIR=D  # vtium's install tree alone
make tier3-provenance        # each copy against its source in a kmscon checkout (KMSCON=DIR); a review step, not a gate
make test-tools              # the harness's self-tests, with no guest
```

`T3_IMAGES=DIR` names the guest image cache (default `$XDG_CACHE_HOME/vtium/tier3/images`). `make install-tree` takes the binary from `CARGO_TARGET_DIR` when it is set. The report is `target/tier3/report.json`, and each variant's captures, serial log and event log are in `target/tier3/run/<variant>/`.

Host needs: `qemu-system-x86_64` with `/dev/kvm`, `qemu-img`, `xorriso`, `gpg`, `curl`, `tic`, `git` and Python 3.12 or later, standard library only. Everything runs as the invoking user; nothing needs root. QEMU runs headless, one VM per variant with the vCPUs and memory the variant names, and the driver stops every VM it starts by the PID it started. The GL variant also needs the host's render node (`T3_RENDERNODE`, default `/dev/dri/renderD128`); no variant opens a card node.

## What is pinned

| Piece | Pin |
|---|---|
| Guest image | `Arch-Linux-x86_64-cloudimg-20261015.611680.qcow2` from `geo.mirror.pkgbuild.com/images/v20261015.611680/` |
| Image SHA-256 | `08754e28d91b1680086c6211ef4df13559e766fff7cdb4006489b14b5d281f08` |
| Image signature | detached `.sig` by the arch-boxes key, primary fingerprint `1B9A16984A4E8CB448712D2AE0B78BF4326C6F8F`, fetched from `keyserver.ubuntu.com` into a keyring beside the images (`fetch-image.sh`); the fingerprint pin is the trust anchor |
| Guest packages | Arch Linux Archive snapshot `2026/10/20`; provisioning runs `pacman -Syyuu` against it, so the guest matches the snapshot exactly |
| Goldens | `c-goldens/`, copied from `fork/tier3/golden` at the kmscon commit `provenance.tsv` names |

The image pins live in the Makefile, and the guest's package list in `guest/user-data.in`. The guest installs the scenario's programs (`chafa`, `libsixel`, `mpv`, `tmux`), the fonts (`ttf-dejavu`, `wqy-microhei`, `noto-fonts-emoji`) and the C reference's runtime dependencies. vtium needs only libc, libudev, libxkbcommon and libgcc_s, which the base image has.

`target/tier3/vm/images` is a link to `T3_IMAGES`, a cache every vtium checkout shares. `fetch-image.sh` fetches the pristine image into its `pristine/`, with the signature and the keyring that checks it. Provisioning builds the base image into its `base/` as `<digest>.qcow2`, named by a digest of the guest files it is built from (`BASE_INPUTS` in `t3_common.py`), the pristine image and the archive date, with the lock that keeps two checkouts from building it at once beside it; `target/tier3/vm/base.qcow2` links to it. Provisioning downloads the guest's packages (about 330 MiB) and keeps no cache, so it is done once per set of inputs, and each set's image is a file of about 1.1 GiB; `BaseImageTest.test_pinned_base` holds the pinned inputs to `a801c98304d282ae.qcow2`. Every image tier 3 reads is one these scripts fetched or built: anything else in the cache, such as the links into the kmscon checkout's `fork/vm/images` that the harness kept at the top of the cache until vtium/vtium#111, is neither read nor changed, and can be removed once no checkout runs that harness.

## The install tree

`make install-tree DESTDIR=D` stages vtium as a system installs it. `make tier3` and `make tier3-minimal` build it into `target/tier3/install`, emptied first, under the run lock.

| Path | From |
|---|---|
| `/usr/bin/vtium` | the minspec binary, `target/minspec/vtium` |
| `/usr/lib/systemd/system/kmsconvt@.service` | `crates/vtium/install/kmsconvt@.service`: `vtium --vt=%I --no-switchvt` in place of getty, with getty's TTY settings and `OnFailure=getty@%i` (ADR-1400, D3) |
| `/usr/share/terminfo/k/kmscon` | `crates/vtium/install/kmscon.ti`, compiled with `tic -x` |
| `/etc/kmscon/kmscon.conf.example` | `crates/vtium/install/kmscon.conf.example` |

The tree goes on the run's disc as `tree.tar`, every entry owned by root, and `guest/setup` unpacks it over the guest's root through the guest's directory links. What the harness needs of the build:

| | |
|---|---|
| Files | `/usr/bin/vtium` and `/usr/lib/systemd/system/kmsconvt@.service`, which the base image's `autovt@.service` links to. A run refuses a tree without either. |
| Process | named `vtium`. `guest/t3` reads its VmRSS by that name. |
| Configuration | `/etc/kmscon/kmscon.conf`, where `setup` installs the variant's `guest/kmscon*.conf`. |
| Log | stdout and stderr, which `setup` routes to the journal and the serial console. The driver waits for `vt: using tty /dev/tty2`, and `/dev/tty3` after the VT switch; requires every `Display NAME with backend [...] text renderer [...]` line to name the variant's renderer; reads the benchmark figures from the `terminal: render-timer: ...` lines `render-timer=log` writes (`t3_bench.py`); and reads `cell size WxH` when present. |
| Revision | `log: vtium Revision REV`, the first such line on the console. A run fails every case of a variant whose console logged no such line, or logged another name. |
| Size | the binary at most 2,621,440 bytes (2.50 MiB), the operator's budget (`BINARY_BUDGET` in `t3_impl.py`), raised from 2.30 MiB on 2026-10-27 when vtium reached 2,400,640 bytes. A run whose binary is larger fails, whatever its cases; raising the budget is a decision recorded on vtium/vtium#22. |

The guest must hold no kmscon, libtsm or vtium package, and `setup` reports the digest of the `tree.tar` it unpacked, which must be the run's. The digest shows the guest unpacked this run's archive. What built the tree is the console's own account, its Revision line, which each variant records as `console`.

## How a run works

1. `make tier3-image` provisions the base once: a qcow2 overlay on the pristine image, booted with a cloud-init seed disc (`guest/user-data.in`). Cloud-init creates the user `tester` (password `tester`, prompt `$ `, no MOTD), installs the packages, links `autovt@.service` to `kmsconvt@.service`, enables the boot hook `tier3-install.service`, disables itself and powers off.
2. The run takes the run lock (`target/tier3/lock`), checks every copy against `provenance.tsv`, and builds the tree.
3. Each variant boots a throwaway overlay of the base with a second disc labelled `T3PKG`: `tree.tar`, the variant's configuration, the screen scripts, `guest/t3` and `guest/setup`. The boot hook runs `setup`, which unpacks the tree, routes the console's log to the serial console, and makes the second serial port (ttyS1) the event port: the login shell and each screen script write a line there. Changing anything on the disc never needs a new base image.
4. The driver (`t3.py`, with QEMU in `t3_qemu.py`) talks QMP on a unix socket. It waits for `TIER3-INSTALL-OK`, systemd's `Started User Login Management` and `Reached target Login Prompts`, then presses Ctrl+Alt+F2: logind starts `autovt@tty2`, which is vtium. It waits for `vt: using tty /dev/tty2` and a drawn login prompt, logs in, and waits for the login shell's line on the event port. Then it types `t3 NAME` for each screen in `SCREENS` (`t3_evaluate.py`). `t3` clears the screen and the scrollback (ED 3), so every screen starts with empty scrollback. After each, the driver waits for the screen script's line on the event port, then captures until two captures a second apart are identical and differ from the screen before. A screen can be followed by key chords (`KEYED`), each a case of its own, captured once the screen has changed; a key that changes nothing fails its case. An image screen that reports images not offered drew no picture for its keys, so they are not pressed: its keyed cases fail with the refusal, and the screens after it still run.
5. VT switch: Ctrl+Alt+F3, then Ctrl+Alt+F2. `vt-switch` passes when the second instance on tty3 draws a screen that is neither blank nor the one just left. `vt-restore` then requires the tty2 screen to equal the last screen's golden (`cjk`) exactly.
6. Pointer, on `2d-potato-min` alone: QEMU's `screendump` leaves the cursor plane out, so the case `pointer-plane` reads it from the guest (`t3_pointer.py`). The driver moves the pointer twice with QMP `input-send-event`, which the q35 machine's VMware mouse takes and vtium reads as a virtual mouse, waits for vtium's log line that the display shows it on its cursor plane, and after each move types `t3 pointer`, whose script prints the guest's DRM atomic state (`/sys/kernel/debug/dri/N/state`, which `setup` makes readable on that variant's guest) on the event port. One plane besides the primary must show a framebuffer, its corner the arrow's hotspot up and left of the pixel the mouse points at, worked out as vtium works it out; once the pointer hides, 20 s after its last move, none may, though the readings type keys meanwhile. The case is a check without a golden: its capture, the last reading's screen, is kept only when every reading passed.
7. The driver checks every `Display` line names the variant's renderer, reads each benchmark screen's figures, and compares every capture with its golden.

Each step waits for state the driver can observe: a line on the serial log or the event port, or a screen that has changed and then settled. Every wait has a timeout that fails the case and names what it waited for. On a slow host a capture taken too early can fail its case; it cannot pass it. The one fixed pause is 50 ms between key chords, since QEMU's input queue is bounded. A password typed before the prompt is discarded by PAM, so when no login follows within 20 s the driver types it once more. A step that fails part way keeps the captures taken before it, and each remaining case fails with the error.

## Concurrent runs

`JOBS=N` runs up to N variants at once, each in its own worker process and VM; the default, 1, runs them one after the other. The report lists variants in the requested order. GL variants share the host's render node and never run together. A second run started while one holds the run lock exits at once with the holder's PID. A worker that dies fails its own variant only, and the driver kills any QEMU of that variant it left. A worker still running after `T3_DEADLINE` seconds (default 1800) is sent SIGTERM, stops its VM and fails its variant, and QMP commands time out after 30 s. A run sent SIGTERM or Ctrl+C stops its workers and VMs before it releases the lock, and writes a report in which the unfinished variants fail.

## Variants

| Variant | Machine | Device | Renderer | Configuration |
|---|---|---|---|---|
| `2d-1280x800` | 4 vCPUs, 4096 MB | `virtio-vga`, EDID 1280x800 | bbulk | `kmscon.conf` |
| `2d-7680x2160` | 4 vCPUs, 4096 MB | `virtio-vga`, EDID 7680x2160, `max_hostmem=1G` | bbulk | `kmscon.conf` |
| `2d-1366x768-potato` | 1 vCPU, 512 MB | `virtio-vga`, EDID 1366x768 | bbulk | `kmscon.conf` |
| `2d-7680x2160-font32` | 4 vCPUs, 4096 MB | as `2d-7680x2160` | bbulk | `kmscon-font32.conf`: 32 points |
| `gl-1280x800` | 4 vCPUs, 4096 MB | `virtio-vga-gl`, `-display egl-headless` | gltex | `kmscon-gl.conf` |
| `2d-potato-min` | 1 vCPU, 512 MB | `virtio-vga`, EDID 1366x768 | bbulk | `kmscon-minimal.conf`: unifont |
| `2d-potato-min-psf` | 1 vCPU, 512 MB | `virtio-vga`, EDID 1366x768 | bbulk | `kmscon-minimal-psf.conf`: kbd's 8x16 PSF font |
| `2d-320x240-psf8` | 1 vCPU, 512 MB | `virtio-vga`, EDID 320x240 | bbulk | `kmscon-small-8x8.conf`: the 8x8 PSF2 test font at its own size, 40 by 30 cells |
| `2d-640x480-psf16` | 1 vCPU, 512 MB | `virtio-vga`, EDID 640x480 | bbulk | `kmscon-small-8x16.conf`: the 8x16 PSF1 test font, 80 by 30 cells |
| `power-reboot`, `power-reboot-hard`, `power-off`, `power-suspend-off`, `power-suspend-logind` | 1 vCPU, 512 MB | `virtio-vga`, EDID 1366x768; S3 offered to the suspending ones | bbulk | `kmscon-power.conf`: the power keys bound |

`make tier3` runs the first five (`DEFAULT_VARIANTS`), whose goldens the C reference drew with DejaVu Sans Mono through FreeType and pango, and the GL one through its GPU renderer; vtium draws in software with unifont or PSF until its outline fonts land (vtium/vtium#23), so those variants judge how far vtium is from them. Until then every case of the four 2D variants fails but `vt-switch`: the text screens on the font, and the image screens and their keyed cases also on the pictures vtium does not yet draw (vtium/vtium#48, vtium/vtium#49). Every case of `gl-1280x800` fails on its renderer: the variant requires gltex, and vtium draws in software until its GPU path lands (vtium/vtium#33). `make tier3-minimal` runs the four minspec variants. The two potato ones draw every screen, and their goldens are the C reference's minimal build with the same font backends vtium has. Their image screens draw no picture (`"images": False`): each must report images not offered, and the keys that act on pictures are not sent.

The two small displays are minspec's floor and the size at which everything vtium supports is usable (vtium/vtium#75, docs/architecture.md). They draw `SMALL_SCREENS` (`t3_evaluate.py`): `small-console`, written for 40 columns, then `sgr`, `boxdraw`, `underline-styles`, `underline-colour`, `strike`, `wide-margin` and `cjk`, so `vt-restore` compares with `cjk` as in every variant. Their fonts are `tools/testfont`'s, which the run writes into `target/tier3/fonts` and the disc carries to `/usr/local/share/tier3-fonts`, so no console font of the guest's reaches them. At 320x240 the default font size is 16, which would draw the 8x8 font doubled as 20 by 15 cells, so its configuration sets `font-size=8`.

The power variants run only when named, as `make tier3 VARIANTS="power-reboot power-reboot-hard power-off power-suspend-off power-suspend-logind"`. Each presses the power keys (vtium/vtium#21) on the console's login prompt, in the order of its cases, and its cases are checks without a golden (`t3_power.py`). A case's capture is the screen before its key, and it passes when the console log after the key shows vtium's line for the key and then the action carried out, with no kernel panic, and a case that ends the VM passes only on QEMU's SHUTDOWN event for it: `guest-reset` for a reboot, `guest-shutdown` for a power-off.

| Case | Key | Passes when the console shows |
|---|---|---|
| `reboot` | `grab-reboot`, `kernel.ctrl-alt-del` at 0 | systemd's `Unmount All Filesystems` and `System Reboot` targets reached, then the kernel's `reboot: Restarting system` |
| `reboot-hard` | `grab-reboot`, `kernel.ctrl-alt-del` at 1, which the variant's disc sets | the kernel's `reboot: Restarting system` with neither target |
| `suspend` | `grab-suspend`, vtium built with `logind` | vtium's line that logind took the request and systemd's `Sleep` target, in either order, then `System Suspend`; QMP sees the guest suspended, wakes it, and vtium draws its screen again |
| `suspend-sysfs` | `grab-suspend`, the minspec build | vtium's line that it writes `mem` to `/sys/power/state`, and no `Sleep` target; QMP sees the guest suspended, wakes it, and vtium draws its screen again |
| `power-off` | `grab-power-off`, on a guest that has not slept | systemd's `Unmount All Filesystems` and `System Power Off` targets reached, then the kernel's `reboot: Power down` |
| `power-off-after-s3` | `grab-power-off`, after `suspend` or `suspend-sysfs` | a known failure of the guest's (`t3_power.KNOWN`): the same lines, then a kernel panic with QEMU still running. It passes with a note while it fails that way, and fails once it no longer does |

QEMU runs with `-no-reboot`, so a reboot ends the VM as a power-off does, and each has a variant of its own.

After an S3 resume the guest does not power off, whichever path suspended it. The suspending variants log the kernel at info level, so the console shows systemd stopping the services, `systemd-shutdown` unmounting every filesystem, the kernel entering S5 (`ACPI: PM: Preparing to enter system sleep state S5`) and `reboot: Power down`; QEMU's machine stays on, 10 s later `reboot(2)` returns to `systemd-shutdown`, which exits, and the kernel panics with PID 1 gone. A guest that has not slept powers off with the same key. vtium's part, `SIGRTMIN+4` to PID 1, is the same in both, so the failure is the guest's ACPI power-off after S3 in QEMU's q35 machine. `power-suspend-logind` runs vtium built with `--features logind`, which `t3.py` builds into `target/tier3/features-logind` with the minspec profile and puts on the variant's disc, where `setup` installs it over the tree's binary; the budget judges the tree's binary.

Three findings from the C reference's runs shaped this table. At 7680x2160 each XRGB buffer is 63 MiB, and with QEMU's default `max_hostmem` (256 MiB) the second instance on tty3 fails to create its buffers; 1 GiB clears it. Once virgl owns the scanout, QMP `screendump` fails with "no surface", so the GL variant captures over VNC on a unix socket (raw RFB, `t3_qemu.py`). QEMU 11.1's `screendump` pads each PPM row to a multiple of 4 bytes while the header gives the width, so `t3img.read_ppm` drops the padding and refuses a body of any other size.

## Goldens, tolerances and masks

The C reference's goldens are `c-goldens/<variant>/<screen>.png`, full resolution and lossless. A keyed case's golden is named after the case; `photo-scrollforward` and `vt-restore` are compared with another screen's (`SAME_AS`). Comparison is exact unless a sidecar `<screen>.tol` sits beside the golden, holding `max_delta=`, `max_pixels=` and `mask=x0,y0,x1,y1` lines (end-exclusive); `#` starts a comment, and a malformed sidecar fails its case.

| Status | Meaning | Gate |
|---|---|---|
| `pass` | identical outside the masks | passes |
| `near` | differs, within both limits of the case's sidecar | passes |
| `departure` | differs only inside the rectangles `departures.tsv` gives for a departure a crate's README lists | passes |
| `provisional` | compared with a golden of vtium's that `provisional.tsv` lists, not yet reviewed: the status it would get is noted | passes |
| `fail` | differs beyond the tolerance, outside a departure's rectangles, listed as a departure and no longer different, or the case could not run | fails |
| `missing` | no golden | fails |

`departures.tsv` lists the cases whose difference from their golden a departure explains: a line names variants and screens (each pair a case), the README that lists the departure under `## Departures`, its bold title exactly as written there, and the rectangles, end-exclusive and covering whole cells, its difference must lie in. Today it holds two. The "no sixel" line of the ten image screens in the two minspec variants quotes the DA1 reply, where vtium sends `CSI ? 62 ; 22 c` and the C golden shows `?60;1;6;9;15c` (row 1, columns 25 to 60). The two dashed lines of `underline-styles` in the same variants draw vtium's dash, from each cell's left edge for five eighths of it, where the C golden shows the C reference's over each cell's middle two thirds (rows 6 and 15, from column 16). A run refuses to start when a line names a case no variant draws, a check without a golden, or a title its README does not list. A rectangle covers whole cells, so a defect in a listed cell is not seen until the line goes or its rectangles shrink.

The run exits 1 on any `fail` or `missing`, or a binary over the budget. Nothing here writes a golden. vtium's own are `goldens/<variant>/<screen>.png`, for the variants that name them (`"goldens"` in `t3_scenario.py`): captures of vtium's, each added in a commit of its own that lists it and what it shows, as `goldens/README.md` does (kmscon `ADR-304` §6). Each stays in `provisional.tsv`, as `VARIANT/SCREEN` and why, until the operator has reviewed it; removing its row makes it judge. A run refuses to start when a golden or sidecar under `goldens/` or `c-goldens/` is one no case compares with, or `provisional.tsv` lists a golden that does not exist. <!-- adr-cite-ignore: kmscon's record -->

## The copies

`provenance.tsv` lists every file tier 3 copied from the C reference, byte for byte, one row each in `tools/conformance/provenance.tsv`'s form: `PATH REPO COMMIT SOURCE SHA256`. They are the 223 goldens of the seven variants the C reference drew, every one a case compares with (12.8 MB, most of it the two 7680x2160 variants'), copied from `fork/tier3/golden` at kmscon `b1f7a7c5`, and the harness files whose output those goldens judge, unchanged from the same commit: the screen scripts and their pictures, the five configurations of those variants, the guest files the base image is built from, and `t3img.py`. Those 47 files stay byte-identical with their sources, the C reference's spelling ("colour") included; vtium's spelling applies only to the files of `tools/tier3` that have no row. A harness file that vtium changes leaves the list in the same commit, and the commit says which goldens no longer judge exactly what the C reference drew. Every run, and `ProvenanceTest` in `make test-tools`, checks each row's file against its SHA-256 and every file under `c-goldens/` for a row; a run with any copy out of step refuses to start. A golden changes only in a commit of its own that lists each moved value, as CONTRIBUTING.md asks, and that commit changes its row too. Nothing in a run ties a row to its source, so a change to `provenance.tsv` is verified at review with `make tier3-provenance` (`KMSCON=DIR`, default `../kmscon`): it reads each row's source from the checkout with `git cat-file`, read-only, and fails on a source whose SHA-256 differs from the row's or that the checkout lacks. It is not a gate.

## Report

`target/tier3/report.json`, one case per variant and screen:

- `run_id` is fresh for every run, so a comparison can tell two runs from one run saved twice.
- `tools` names vtium's commit (`+dirty` with uncommitted changes), the guest image, its SHA-256 and the archive snapshot, which the Makefile passes, and under `goldens` the kmscon commit the C reference's goldens were copied from and the `provenance.tsv` that lists them. A field the driver ran without is `null`.
- `build` names the tree and its `tree_sha256`, `binary_bytes` and `installed_bytes` (the regular files it installs), `budget` (the binary's budget and whether it is met), and `consoles`, each `NAME REVISION` the variants' consoles logged.
- Each case carries `status`, `departure` (the title, for a case `departures.tsv` lists), `description` (the header comment of its screen's script with its `# look-for:` lines, or its entry in `CHECKS`), `wrong` (its `# wrong-looks-like:` lines), `text` (the lines its script prints, colors removed, or `null`; see `guest/screens/README.md`), `golden`, `actual`, `diff` and their thumbnails as PNGs, `tolerance`, and `stats` (`pixels`, at most 16 `bbox` regions, `max_delta`).
- Under `variants`, each variant records `installed` (the tree the guest reported), `displays` (the console's `Display` lines), its QEMU `device`, `config`, `cell`, `cpus` and `memory_mb`, `bench` (each benchmark screen's figures), `rss_kb` (the console's VmRSS after each screen, the largest across its processes), `resident` (after the first screen, and the peak with the screen it followed), `startup_ms` (from the Revision line to the first `Display` line), `console`, `timing` (seconds to the end of each phase) and any `error`.

The run prints each case's status, then the tree, its binary against the budget and the consoles that ran, and each variant's resident memory.

## Benchmarks

The C reference's scenarios run as screens. S1, text scrolling with no image, is `s1-scroll`. A2 is `mpv-sixel` (mpv `--vo=sixel` playing ffmpeg's testsrc2 pattern) and A4 is `tmux-2x2` and `tmux-3x3` (the same pattern in every pane of a tmux grid). `flood` sends sixel pictures in 4 KB parts for 4 seconds, a stream that changes the screen rarely. Each is drawn and compared with its golden like any screen, and reports its window on the event port as `T3-BENCH NAME start=S end=E KEY=N...`, both times from `/proc/uptime`, the clock that also stamps the console lines. `t3_bench.py` takes the console's render-timer lines whose seconds overlap the window, which `render-timer=log` writes once a second per display: frames presented and queued, their CPU time and their damaged area.

`mpv-sixel` fails when the console's CPU time per frame mpv sent passes the clip's 33.3 ms frame interval, when it presents under 90% of mpv's frames a second, or when mpv showed no frame (`t3_bench.bottleneck`); a variant where the console is known to be the bottleneck goes in `KNOWN_BOTTLENECKS` with its issue. A tmux grid fails when tmux sent no sixel picture. `flood` fails when no frame is presented in a second wholly inside the window, or when more than 20 passes a second draw nothing. A build with the render timer configured that logs no line in the window fails the case.

The figures are measured on a guest: a vCPU preempted by the host still counts as the thread's CPU time, so compare builds with `JOBS=1`, several runs each, on a host doing nothing else.

## Self-tests

`make test-tools` runs `test_t3img.py` (image I/O and the masked compare) and `test_t3.py`: the captions and screen text, the tree and its budget, the guest's account of its install, the copies against `provenance.tsv`, where provisioning fetches and builds the images, the base image's inputs, each variant's machine, the console lines the driver reads, the QEMU command line, the benchmark figures and verdicts, and the serial reader. `SixelGoldenTest` reads the colors and cells of the copied sixel goldens.

## What changed from the fork's harness

The fork's harness judged the C reference's packages and could run a vtium build in their place. Here it judges vtium's install tree alone:

- **The build:** `make install-tree` in place of the C reference's `make pkg` and `pkg-minimal` in the docker dev image. The package path (`pacman -U`, `--pkg-dir`, `--pkg-name`), the libtsm commit checks, the image-helper probe in `setup`, `upstream.mk`'s comparison runs (`--compare-only`) and the stand-in tree are gone. A minspec variant needs no build of its own: the configuration selects the font.
- **The goldens:** the C reference's copied into `c-goldens/` with their provenance, and vtium's own for the small displays in `goldens/`; the harness writes neither: `tier3-bless` is gone.
- **The paths:** `target/tier3/` for the work directories, the report, the lock, the tree and the VM directory, whose `images` links to vtium's image cache, `T3_IMAGES`, from which tier 3 reads only the images vtium's scripts fetched and built.
- **The size:** the binary against 2.50 MiB, failing the run past it, in place of the main package's size against 1.77 MiB, which was reported and not judged.
- **The console:** a run fails unless the console logs itself as vtium.
- **Departures:** `departures.tsv` turns a difference a listed departure explains into `departure`, as tier 1 and tier 2 do; the fork's harness had no such list.
- **The image:** `make tier3-image` fetches and verifies the pristine image with `fetch-image.sh` when the cache lacks it; the fork had no rule that ran it.
- `tier3-gl` and `tier3-selftest` are folded into `make tier3 VARIANTS=gl-1280x800` and `make test-tools`.

The C reference's account of its own runs, the gate seen red against its code and the GL variant's black screen among them, stays in `fork/tier3/README.md` at kmscon `b1f7a7c5`.
