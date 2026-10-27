# Tier 3: the package in a VM, on a real KMS device

Tier 3 of the conformance harness (ADR-301). The fork is packaged the way a user installs it, installed in a pinned Arch Linux guest under QEMU/KVM, and run by systemd on a VT. A script logs in over the emulated keyboard, draws fixed screens, captures them, and compares each capture exactly with a committed golden. The exit status is the verdict.

## Running it

```sh
make pkg            # build kmscon-aaronsb from this checkout -> fork/build/pkg
make tier3-image    # fetch, verify and provision the base image, or reuse one built from the same inputs
make tier3          # pkg, then every variant; report in fork/build/tier3-report.json
make tier3-gl       # pkg, then the GL variant alone
make tier3 VARIANTS="2d-1280x800 2d-1366x768-potato gl-1280x800"   # any set of variants
make tier3-run VARIANTS=2d-1280x800     # without rebuilding the package
make tier3 JOBS=2   # up to two variants at once, each in its own VM
make tier3-selftest # unit tests of the compare module
make tier3-upstream JOBS=2   # upstream kmscon in the same guest, on the 2D variants, for a comparison page (below)
make tier3-minimal VTIUM=PATH   # the minimal variants on vtium's install tree or packages at PATH (below)
make tier3-standin  # the C reference's minimal package as an install tree, standing in for vtium's (STANDIN=1)
```

Host needs: docker (for `make pkg`), `qemu-system-x86_64` with KVM, `qemu-img`, `xorriso`, `gpg`, `curl`, python 3.14 (its `tarfile` reads the `.pkg.tar.zst` packages). Everything runs as the invoking user; nothing needs root. QEMU runs headless, one VM per variant with the vCPUs and memory the variant names (see [Variants](#variants)), and the driver stops every VM it starts by the PID it started. Once the base image exists and the package is built, each variant takes 118 to 134 s from its start to its VM stopping, and the five take about 362 s with `JOBS=2` (`make tier3`, on a 32-thread host, 2026-10-25, after a 34 s package build). In that run the four 2D variants were done at about 306 s, so the GL variant adds about 90 s to the default run. `s1-scroll` adds about 7 s to each variant, 9 s at 7680x2160.

## What is pinned

| Piece | Pin |
|---|---|
| Guest image | `Arch-Linux-x86_64-cloudimg-20261015.611680.qcow2` from `geo.mirror.pkgbuild.com/images/v20261015.611680/` |
| Image SHA-256 | `08754e28d91b1680086c6211ef4df13559e766fff7cdb4006489b14b5d281f08` |
| Image signature | detached `.sig` by the arch-boxes key, primary fingerprint `1B9A16984A4E8CB448712D2AE0B78BF4326C6F8F` (fetched from `keyserver.ubuntu.com` into `fork/vm/images/gnupg`; archlinux.org publishes no WKD entry for it, so the fingerprint pin is the trust anchor) |
| Guest packages | Arch Linux Archive snapshot `2026/10/20`; provisioning runs `pacman -Syyuu` against it, so the guest matches the snapshot exactly |
| Fonts | `ttf-dejavu 2.37+18+g9b5d1b2f-8` (DejaVu Sans Mono, Latin and box drawing), `wqy-microhei 0.2.0_beta-12` (CJK fallback through pango) and `noto-fonts-emoji 1:2.051-1` (Noto Color Emoji, colour emoji), from the snapshot |
| Sixel acceptance programs (ADR-103's A1, A2, A4) | `chafa 1.18.3-1`, `libsixel 1.10.5-1` (img2sixel), `mpv 1:0.41.0-6` (which brings `ffmpeg 2:9.0.2-2`) and `tmux 3.8-1`. Together they install 327.4 MiB, and the provisioned base image is a 1145 MiB file. A1's photograph, the operator's street at sunset (CC BY 4.0), is not a package: it travels with the screens on the run's disc (`guest/screens/photo/`). Until 2026-10-24 the guest also installed `xplanet 1.3.1-7` for the NASA photographs A1 drew then. A run downloads nothing |

The pins live at the top of `fork/tier3.mk`, and the package list in `guest/user-data.in`.

Provisioning downloads the guest's packages from the archive (about 330 MiB for the acceptance programs) and keeps no cache, so it is done once per set of inputs: the base image is `fork/vm/images/base-<digest>.qcow2`, named by a digest of the guest files it is built from (`BASE_INPUTS` in `t3_common.py`), the pristine image and the archive date, and `fork/vm/base.qcow2` is a link to it. `make tier3-image` (and so `make tier3`) builds it only when it is missing, under a lock beside it, and otherwise only links it. A worktree that links `fork/vm/images` from the main checkout, as development.md says, therefore shares every base image the main checkout or another worktree has built: a branch that changes the package list downloads once, and every checkout with the same list reuses the build. A changed list gets a new image beside the old, so a base a running VM backs on is never rewritten; old ones stay until removed by hand. The base names the pristine image by its real path, so it outlives the worktree that built it. The snapshot date has to be no older than the dev image the package is built in, or the package can need a newer glibc than the guest has.

## How a run works

1. `make tier3-image` provisions the base once: a qcow2 overlay on the pristine image, booted with a cloud-init seed disc (`guest/user-data.in`). Cloud-init creates the user `tester` (password `tester`, prompt `$ `, no MOTD), installs kmscon's runtime dependencies and the fonts, points `autovt@.service` at `kmsconvt@.service`, enables the boot hook `tier3-install.service`, disables itself and powers off.
2. Each variant boots a throwaway overlay of the base with a second disc labelled `T3PKG`: the built packages, the variant's `kmscon.conf`, the screen scripts and `guest/setup`. The boot hook runs `setup`, which installs the packages with `pacman -U` (or unpacks vtium's install tree; see [vtium through the same harness](#vtium-through-the-same-harness)), routes kmscon's log to the serial console, and makes the second serial port (ttyS1) writable as the event port: the login shell and each screen script write a line there. Changing anything on the disc never needs a new base image. `setup` then reports on the console every kmscon or libtsm package installed and the libtsm commit the package was built with (the recipe installs it as `/usr/share/doc/<package>/libtsm-commit`, with `+dirty` when that tree had changes or untracked files). The driver fails the variant when the guest holds any such package that is not on the disc, when the commit is unknown or not the libtsm the run names, or when the two disagree on `+dirty`. A run must name its libtsm, so a fresh worktree needs `subprojects/libtsm` checked out first (`make build` fetches it).
3. The driver (`t3.py`, with QEMU in `t3_qemu.py`) talks QMP on a unix socket. It waits for `TIER3-INSTALL-OK`, systemd's `Started User Login Management` and `Reached target Login Prompts` on the serial log, then presses Ctrl+Alt+F2: logind spawns kmscon on tty2, and a boot without the keypress has no kmscon there. It waits for kmscon's `vt: using tty /dev/tty2` and a drawn login prompt, logs in, and waits for the login shell's line on the event port. Then it types `t3 NAME` for each screen in `SCREENS` (`t3_evaluate.py`): `sgr`, `colour-forms`, `underline-styles`, `underline-colour`, `strike`, `boxdraw`, `blocks`, `mosaic`, `wide-margin`, `combining`, `emoji`, `sixel`, `sixel-ramps`, `sixel-article`, `sixel-bound`, `sixel-evict`, `photos`, `photo-scroll`, `mpv-sixel`, `tmux-2x2`, `tmux-3x3`, `s1-scroll`, `flood`, `cjk`. `t3` clears the screen and then the scrollback with ED 3, so lines one screen scrolled away never reach the next screen's Shift+PageUp; `clear` sends ED 3 itself now that kmscon's terminfo has E3 (#94), and the explicit one keeps the screens independent of the guest's ncurses and TERM. Every screen therefore starts with empty scrollback, so scrollback state carried across screens, such as Q1 evicting pictures another screen left in scrollback, is outside tier 3; a user's `clear` now empties the scrollback the same way. After each, it waits for the screen script's line on the event port, then captures until two captures a second apart are identical and differ from the screen before. A screen can be followed by key chords (`KEYED` in `t3_evaluate.py`), each a case of its own, captured the same way once the screen has changed; a key that changes nothing fails its case. After `sixel-evict` it presses kmscon's grabs: `sixel-evict-rotated` after Logo+= turns the screen, `sixel-evict-history` after Logo+- turns it back and Shift+PageUp reaches the top of the scrollback, and `sixel-evict-zoomed` after Shift+PageDown and Ctrl+=; Ctrl+- then puts the font back (`KEYS_AFTER`). The screen fills the image budget with pictures, so the turned and zoomed cases show a scaled copy that fits only by evicting the oldest picture in scrollback (ADR-103's Q1 note), and the history case shows that picture gone. After `photo-scroll` it presses Shift+PageUp (`photo-scrollback`, kmscon's view of its scrollback with the photograph in it) and then Shift+PageDown (`photo-scrollforward`, compared with `photo-scroll`'s golden).
4. VT switch: Ctrl+Alt+F3, then Ctrl+Alt+F2. The `vt-switch` case passes when logind's second kmscon on tty3 draws a screen that is neither blank nor the one just left, so a switch that never happens cannot let `vt-restore` pass on its own. `vt-restore` then requires the tty2 screen to equal the `cjk` golden exactly.
5. The driver reads kmscon's `Display ... with backend [..] text renderer [..]` line from the serial log and fails the variant if kmscon drew with a renderer other than the one the variant exists to test.
6. It reads the figures of each benchmark screen (`BENCHMARKS` in `t3_evaluate.py`; see [Benchmarks](#benchmarks)) from kmscon's render-timer lines on the serial log.

Each step waits for state the driver can observe: a line on the serial log or the event port, or a screen that has changed and then settled. Every wait has a timeout that fails the case and names what it waited for. On a slow host a capture taken too early can fail its case; it cannot pass it. The one fixed pause left is 50 ms between key chords, because QEMU's input queue is bounded and shows nothing outside QEMU. A password typed before the prompt is discarded by PAM, so when no login follows within 20 s the driver types it once more.

A step that fails part way keeps the captures taken before it: each remaining case is reported as `fail` with the error.

## Concurrent runs

`JOBS=N` runs up to N variants at once, each in its own worker process and VM; the default, 1, runs them one after the other in the driver's own process. The report lists variants in the requested order, so it equals a serial run's report apart from each variant's `timing`. GL variants share the host's render node and never run together; a GL variant runs beside a 2D one. A second run started while one holds `fork/build/tier3.lock` exits at once with the holder's PID. `make tier3` and `make tier3-gl` build the package under that lock, and a run installs the same package build in every variant. A worker that dies fails its own variant only; the driver kills any QEMU of that variant it left behind. A worker still running after `T3_DEADLINE` seconds (default 1800) is sent SIGTERM, stops its VM and fails its variant, and QMP commands time out after 30 s, so a wedged QEMU fails its variant instead of hanging the run. A run that is itself sent SIGTERM or Ctrl+C stops its workers and VMs before it releases the lock, and writes a report in which the unfinished variants fail. Log lines carry the seconds since the run started and the variant's name.

Only `JOBS=2` has been measured, on a 32-thread host. There is one GL variant, so the rule that keeps GL variants apart has been checked only with stand-in jobs. The default stays 1 until serial and concurrent reports have matched over more runs. Snapshots that would let a variant skip its boot are deferred (#16).

## Upstream through the same harness

`make tier3-upstream` runs upstream kmscon through this scenario, so a comparison page ([conformance.md](../../docs/fork/conformance.md#comparison-pages)) can set it beside the fork. It runs the 2D variants unless `VARIANTS` names others: upstream's pin lacks 9e12309, so its gltex can draw a black screen (#17). `fork/upstream.mk` pins upstream kmscon's main and libtsm v4.8.0, each with its reason; `make upstream-src` checks both out in `fork/build/upstream/kmscon`, with libtsm as its `subprojects/libtsm`, and refuses a checkout with local changes. `make pkg-upstream` builds them with the fork's recipe through `fork/pkg/upstream/PKGBUILD`, which sources `fork/pkg/PKGBUILD` and changes only the names: `kmscon-upstream` and `kmscon-upstream-terminfo`, in `fork/build/pkg-upstream`, built in their own work directory so nothing of the fork's build reaches them (#38). The run installs only those, so the guest log names the upstream packages and libtsm v4.8.0's commit, and the report's `tools` names upstream's commits and repositories.

Upstream reads every key of `guest/kmscon.conf` and logs its character cell, so both builds run with the same configuration and the comparison page names cells for both. The goldens are the fork's, so a screen upstream draws differently reports `fail`; `tier3-upstream` passes `--compare-only`, which fails the run only for a case with no capture. The report and captures go where `make tier3` writes the fork's, so save each run (`make sheet-save TIERS=3`) before starting the next.

## vtium through the same harness

`VTIUM=PATH` runs vtium in place of the C reference on `make tier3`, `make tier3-minimal` or `make tier3-run`, with the same scenario and goldens (ADR-304 §6). PATH is an install tree, a directory laid out as the guest's root, or vtium's package files, separated by spaces. Nothing is built: `make tier3-minimal VTIUM=PATH` runs `2d-potato-min` and `2d-potato-min-psf` on the build at PATH. A tree goes on the run's disc as `tree.tar`, every entry owned by root, and `setup` unpacks it over the guest's root; packages install through pacman, as the C reference's did.

What the harness needs of the build, the interface ADR-304 §1 keeps:

| | |
|---|---|
| VT unit | `/usr/lib/systemd/system/kmsconvt@.service`, which the base image's `autovt@.service` links to. The run refuses a build without it. |
| Process | named `vtium` or `kmscon`. `guest/t3` reads its VmRSS by that name, and the console lines below are read with either name before the PID. |
| Configuration | `/etc/kmscon/kmscon.conf`, where `setup` installs the variant's, with the keys in `guest/kmscon-minimal.conf` and `guest/kmscon-minimal-psf.conf`. |
| Log | stdout and stderr, which `setup` routes to the journal and the console. The driver waits for `vt: using tty /dev/tty2`, and `/dev/tty3` after the VT switch; requires every `Display NAME with backend [...] text renderer [...]` line to name the variant's renderer (`bbulk`); and reads `s1-scroll`'s figures from the `terminal: render-timer: ...` lines `render-timer=log` writes (`t3_bench.py`). A `log: NAME Revision` line and a `cell size WxH` line are read when present. |

The guest must hold no kmscon, libtsm or vtium package but the run's, and `setup` reports the digest of the `tree.tar` it unpacked, which must be the run's. The digest shows the guest unpacked this run's archive. It says nothing of what built the tree, so a stale tree passes it. What ran is the console's own account: the `NAME Revision REV` line its log opens with, which each variant records as `console`. A vtium run fails every case unless that line names vtium. vtium has no libtsm, so nothing checks one.

ADR-304 §5's size target, 1.77 MiB, is the C reference's minimal main package's `.PKGINFO` size at 92abd024, without its terminfo package, so the report compares a build's size without the terminfo entry with it and gives the total beside it. The C reference's minimal main package was 1.78 MiB at b1f7a7c5 (1,870,998 bytes; 1,879,995 with terminfo), over the target set from it.

`make tier3-standin` unpacks the C reference's minimal package into `fork/build/vtium-standin`, an install tree laid out as vtium's is. `make tier3-minimal VTIUM=fork/build/vtium-standin STANDIN=1` runs the vtium path end to end with the C reference's binary in vtium's place, and passes both variants. `STANDIN=1` (`--standin`) names a build that stands in for vtium's, whose console may be another; the report and the summary call it a stand-in, and its `console` names the console that ran.

## Variants

| Variant | Machine | Device | kmscon path | Capture |
|---|---|---|---|---|
| `2d-1280x800` | 4 vCPUs, 4096 MB | `virtio-vga`, EDID 1280x800 | drm2d + bbulk | QMP `screendump` (PPM) |
| `2d-7680x2160` | 4 vCPUs, 4096 MB | `virtio-vga`, EDID 7680x2160, `max_hostmem=1G` | drm2d + bbulk | QMP `screendump` (PPM) |
| `2d-1366x768-potato` | 1 vCPU, 512 MB | `virtio-vga`, EDID 1366x768 | drm2d + bbulk | QMP `screendump` (PPM) |
| `2d-7680x2160-font32` | 4 vCPUs, 4096 MB | as `2d-7680x2160`, with `guest/kmscon-font32.conf`: 32 points, a 19x38 cell | drm2d + bbulk | QMP `screendump` (PPM) |
| `gl-1280x800` | 4 vCPUs, 4096 MB | `virtio-vga-gl` with `-display egl-headless` (virgl on the host's render node, `T3_RENDERNODE`, default `/dev/dri/renderD128`) | drm3d + gltex (`hwaccel`) | VNC over a unix socket |

Every variant draws every screen, the image screens (`IMAGE_SCREENS` in `t3_evaluate.py`) and their keyed cases included: bbulk draws image placements on the 2D variants, and gltex on `gl-1280x800` (ADR-103 steps 3f and 4e, ADR-108). Until then `gl-1280x800` carried a `"draws_images": False` flag, skipped the image screens and ran a `no-images` check that kmscon refused sixel and kitty graphics there; both went when gltex began drawing placements.

`2d-1366x768-potato`, the potato computer, is ADR-103's small machine (ADR-103 names it `2d-1366x768-hamster`; renamed at the operator's request, noted at the end of ADR-103): one vCPU and 512 MB at a common laptop size, so a regression on small machines shows in the default run. Its vCPU is a core of the host, so it bounds parallelism and memory; it does not reproduce slow hardware. `2d-7680x2160-font32` runs every screen at a 19x38 cell, near the 20x40 of Monoid at 30 on the operator's display; every other variant runs a 10x19 cell. The sixel page sizes its pictures from the cell, so its layout is judged at two cell sizes. `make tier3` runs every variant in the order 1280x800, 7680x2160, potato, 7680x2160-font32, GL, so with `JOBS=2` the longest starts first, and the worker that runs three variants (1280x800, the potato, GL) runs the three shortest.

Three findings shaped this table:

- At 7680x2160 each XRGB buffer is 63 MiB. With the default `max_hostmem` (256 MiB) the second kmscon on tty3 fails to create its buffers (`virtio_gpu ... response 0x1201`, out of memory). 1 GiB clears it.
- Once virgl owns the scanout, QMP `screendump` fails with "no surface". egl-headless reads the scanout back into a surface for a paired VNC server, so the GL variant captures over RFB (raw encoding, implemented in `t3_qemu.py`). Captures are byte-identical across boots, like the 2D ones.
- QEMU 11.1's `screendump` writes each PPM row padded to a multiple of 4 bytes while the header gives the width. At 1366 pixels a row is 4098 bytes and arrives as 4100, so read as the header says, every row slid 2 bytes and the screens came out sheared, with solid colours split into red, green and blue stripes. `t3img.read_ppm` drops the padding, and refuses a body of any other size.

## Goldens, tolerances and masks

Goldens are `golden/<variant>/<screen>.png`, full resolution and lossless, written by `t3img.py` through zlib (all 115 come to about 8 MB, the 7680x2160 ones included). A keyed case's golden is named after the case, as `sixel-evict-rotated.png`.

Comparison is exact unless a tolerance sidecar `golden/<variant>/<screen>.tol` sits beside the golden. It holds `key=value` lines; `#` starts a comment:

```
# cursor cell blinks; antialiasing differs by one level on the GL path
max_delta=1          largest per-channel difference allowed
max_pixels=200       how many pixels may differ
mask=0,780,10,800    x0,y0,x1,y1, end-exclusive, left out of the comparison; repeat for more
```

An absent `max_delta` or `max_pixels` is 0, and a malformed sidecar fails its case. No golden has a sidecar today: the guest config turns blinking off, the prompt is fixed and no screen shows a clock.

Each case gets one of four verdicts:

| Status | Meaning | Gate |
|---|---|---|
| `pass` | identical outside the masks | passes |
| `near` | differs, within both limits of the case's sidecar | passes |
| `fail` | differs beyond the tolerance, or the case could not run | fails |
| `missing` | no golden yet | fails |

The tier exits non-zero on any `fail` or `missing`.

`make tier3-bless` copies the last run's captures over the goldens. Per ADR-301 a golden or sidecar change lands in its own reviewed commit, never with the code that produced it, and each new golden is looked at before it is committed.

## Report

`fork/build/tier3-report.json`, one case per variant and screen:

```json
{"tier": 3, "run_id": "3f2a...",
 "cases": [{"name": "2d-1280x800/sgr", "status": "near",
            "description": "SGR attributes and colours: ...",
            "golden": "fork/tier3/golden/2d-1280x800/sgr.png",
            "actual": "fork/build/tier3/2d-1280x800/sgr.png",
            "diff": "fork/build/tier3/2d-1280x800/sgr.diff.png",
            "tolerance": {"max_delta": 1, "max_pixels": 1, "masks": []},
            "stats": {"pixels": 1, "bbox": [[1000, 700, 1001, 701]], "max_delta": 1},
            "thumb_actual": "fork/build/tier3/2d-1280x800/sgr.thumb.png",
            "thumb_golden": "fork/build/tier3/2d-1280x800/sgr.golden.thumb.png",
            "thumb_diff": "fork/build/tier3/2d-1280x800/sgr.diff.thumb.png"}],
 "variants": {"2d-1280x800": {"packages": ["..."], "kmscon": ["Display ... text renderer [bbulk] ..."],
                              "errors": {}}}}
```

- `run_id` is fresh for every run, so a comparison can tell two runs from one run saved twice.
- `tolerance` is `null` for an exact case.
- `vt-switch` has no golden (`"golden": null`); it is a check.
- `tools` names what the run measured and ran on: the kmscon and libtsm commits (`+dirty` with uncommitted changes) and the repositories they come from (`kmscon_repo`, `libtsm_repo`), the dev image and archive snapshot the package was built from, and the guest image, its SHA-256 and the guest's archive snapshot. `fork/tier3.mk` passes them; a field is `null` when the driver ran without them. In a vtium run the libtsm fields are `null`, and `kmscon` and `kmscon_repo` name the checkout whose harness and goldens judged it.
- `implementation` names what was installed and what ran: `name`, `c` for the C reference or `vtium`; `standin`, true for a stand-in's run; `kind`, `packages` or `tree`; each of its `paths` with its bytes; `installed_bytes`, its size on disk, the bytes of the regular files it installs, which for a package is the size its `.PKGINFO` records; `main_bytes`, the same without the terminfo entry (the packages not named `*-terminfo`, or a tree's files outside `usr/share/terminfo/`); `minspec_target`, ADR-304 §5's 1.77 MiB and whether `main_bytes` is within it, to two places; and `consoles`, each `NAME REVISION` the variants' consoles logged. A tree adds `tree_sha256`, the digest of its `tree.tar`. The run prints it, with each variant's `resident` figures, after the cases.
- Under `variants`, each display also records the packages the guest reported `installed`, with the libtsm commit they were built with, its QEMU `device`, the kmscon `config` lines, and the character `cell` size in pixels from kmscon's font log line (`null` if kmscon logged none). Each screen case carries `text`: the lines its script prints, colours removed, so a comparison page can name the text at a differing cell. It is `null` when the script prints any other control, fails, reads input or runs over 10 seconds (see `guest/screens/README.md`).
- `description` is the caption on the run page: the header comment of the screen's script, with its `# look-for:` lines appended as `Correct: ...`, or for `vt-switch` and `vt-restore` their entry in `CHECKS` in `t3_evaluate.py`. `wrong` is the script's `# wrong-looks-like:` lines, shown under the caption, or `null`.
- `actual`, `diff` and the thumbnails are PNGs, so the run page can show them. The raw PPM captures sit beside them.
- Thumbnails are at most 480 pixels wide, nearest-neighbour. In the diff thumbnail, any block holding a differing pixel is red, so a one-pixel difference stays visible. Fields are `null` where there is nothing to show.
- `timing` under each variant holds the seconds from the variant's start to the end of each phase (`install`, `boot`, `kmscon`, `login`, each screen, `vt-switch`, `vt-restore`) and the seconds `evaluate` took.
- `cpus` and `memory_mb` under each variant are the machine it booted, and `bench` holds each benchmark screen's figures by name (see [Benchmarks](#benchmarks)).
- `rss_kb` under each variant is the console's VmRSS in KiB after each screen, the largest across its processes. `resident` holds ADR-304 §5's memory figures from it: `first_kib` after the first screen (`first_screen`, `sgr`), and `peak_kib`, the largest after any screen, with `peak_screen`, which for the minimal build is `s1-scroll`, once its lines fill the scrollback. `startup_ms` is the milliseconds from the console's `Revision` log line to its first `Display` line.
- `bbox` is a list of at most 16 regions for outlining. Differing pixels in touching 16-pixel tiles share a region, coarser tiles are used while there are many clusters, and then the closest regions merge.
- A variant that cannot boot or drive reports each of its cases as `fail` with an `error`.

## Benchmarks

ADR-103's scenarios run as screens: S1, text scrolling with no image, is `s1-scroll`, drawn and compared with its golden like any screen (its golden is the last rows of the text and the prompt). The acceptance scenarios that play video are benchmark screens too: `mpv-sixel` (A2, mpv --vo=sixel playing ffmpeg's testsrc2 pattern, 640x360 at 30 frames per second for 5 seconds, in a 1920x1080 window in the middle of the screen without dithering, or the largest 16:9 window that fits: 1280x720 at 1280x800 and 1351x760 on the potato; full screen at 7680x2160, mpv's encoder alone holds it to about 4.4 frames a second) and `tmux-2x2` and `tmux-3x3` (A4, the same pattern at 320x180 in every pane of a tmux grid). Their goldens are the screen they come back to, which must hold nothing of the video. The tmux pages close any DCS tmux left open, leave the alternate screen, show the cursor and clear the screen and the scrollback themselves before they print their report, since tmux can exit without restoring the terminal (#154). Their goldens are therefore taken on a screen the page cleared itself, and say nothing about what leaving the alternate screen leaves behind: that is judged by `mpv-sixel`, whose page comes back from mpv's alternate screen without clearing, and by libtsm's `img-alt-*` tier 1 cases.

`flood` measures how kmscon draws a stream that changes the screen rarely (#139, #147). For 4 seconds it sends sixel pictures the screen's width and 8 rows tall, each 4 KB at a time with a 2 ms pause after each part, as a video player's frames arrive. Most of that input changes nothing on screen until a picture's ST. Then the screen is cleared, so its golden is one line of text and the prompt. Its figures are judged (`flood_verdict` in `t3_bench.py`): a frame must be presented in a second wholly inside the window, while the stream flows, and at most 20 passes a second may draw nothing (`skipped`). A pass that finds nothing changed is what kmscon did after every pty read before #146, and after every loop turn before it asked libtsm whether the screen had changed. Without that question the run measured 180 such passes a second at 7680x2160 and 74 at 1280x800; with it, none.

`guest/kmscon.conf` sets `render-timer=log`, so kmscon writes one line per display per second to its log, which `guest/setup` routes to the serial console: frames presented (page flips completed), frames queued, and their CPU time and damaged area. The log draws nothing, so the goldens are the same with it on. A benchmark script pauses 1.5 s, prints its load, pauses again, and reports its window on the event port as `T3-BENCH NAME start=S end=E KEY=N...`, the keys in any order, both times read from `/proc/uptime`, the clock that also stamps the console lines. `t3_bench.py` takes the render-timer lines whose seconds overlap the window; the pauses keep the frames before and after (the clear, the next prompt) out of those seconds.

The keys after the window are the screen's own counts: `lines` for the text `s1-scroll` printed; for a video screen `mpv_frames` and `mpv_dropped`, which `guest/screens/mpv-stats.lua` takes from mpv, and `kmscon_cpu_ms`, the CPU time of the kmscon this terminal belongs to over the window, from `/proc`; and for the tmux grids `tmux_sixels`, the sixel pictures tmux sent to kmscon while it played, counted from the client's output, which `script` copies to a pipe on its way to the terminal. `guest/t3` names the event port in `T3_EVENTS`; outside tier 3, as on the VT demo's pages, the same scripts print their counts under the page instead.

The report's `bench` entry for a screen holds `seconds`, and `lines` and `lines_per_s` where there are lines; `flips`, `frames` and `presented_per_s` (flips over the window's length); `intervals_inside` and `flips_inside`, the seconds wholly inside the window and the flips in them; `cpu_ms_mean` and `damage_px_mean` per frame over the window, with `damage_pct` the damaged share of the screen; `cpu_ms_median`, `cpu_ms_p95` and `cpu_ms_max`; and `skipped`, `skipped_per_s`, `skipped_cpu_ms` and `skipped_cpu_pct`, the redraws that presented nothing, their rate and what they cost. The lines carry no single frames, so the median is the median of the seconds' medians and the 95th percentile the largest second's. A video screen adds `mpv_frames`, `mpv_dropped`, `mpv_per_s`, `kmscon_cpu_ms`, `kmscon_cpu_pct` (its share of one CPU) and `kmscon_cpu_ms_per_mpv_frame` (parsing included, which the render timer does not count). The run page shows them under each variant.

`mpv-sixel` is judged by ADR-103's criterion for A2, that kmscon is not the bottleneck (`t3_bench.bottleneck`): it fails when kmscon's CPU time per frame mpv sent passes the clip's 33.3 ms frame interval, when kmscon presents under 90% of mpv's frames a second, or when mpv showed no frame. A variant where kmscon is known to be the bottleneck goes in `KNOWN_BOTTLENECKS` in `t3_evaluate.py` with the issue that tracks it: the case passes with a note while it lasts and fails once it is gone, so the entry leaves with the fix. Both 7680x2160 variants carried #139 until #152 fixed it there, and none is listed now: full screen, before the window: at 7680x2160 kmscon spent 23.3 ms of CPU time per frame mpv sent (18.8 ms with the 19x38 cell) and presented 4.8 frames a second to mpv's 4.2 (5.1 to 4.6), so mpv's own sixel encoding sets the rate. The tmux grids judge compatibility, as ADR-103's A4 now reads: tmux's pictures reach the screen, every pane animates, and nothing is left after tmux exits. Their frame rate and kmscon's CPU time per picture tmux sent (`kmscon_cpu_ms_per_tmux_sixel`) are reported, not judged: tmux and mpv set the rate (at 7680x2160 under half the panes' frames get through with nothing but a pty reading), so the grids measure tmux as much as kmscon. kmscon's streaming of many pictures at once is to be judged apart from any encoder or multiplexer by a pre-encoded sixel feeder (#161). A grid fails when tmux sent kmscon no sixel picture (`tmux_sixels=0`): tmux then draws placeholders, and kmscon still presents as many frames with as much damage, so no render-timer figure tells the two apart. That nothing is left after tmux exits is the golden's to judge, and that every pane animates is the VT review's. ADR-104's "A1, A2 and A4 in tier 3" records both runs' figures.

S2 to S6 are not tier 3 screens; they are measured in memory by `make bench-compose` (`tests/bench_compose.c`: bbulk and the drm2d blend into an in-memory display, pinned to one host CPU), and ADR-104 records the first run.

A build with the render timer configured that logs no line in the window fails the case, as do `flood` figures out of bounds. Upstream's build does not know the key: it logs `unknown config option 'render-timer'`, keeps going, and its entry gets a note instead of figures, with `lines_per_s` still measured.

The figures are measured on a guest: a vCPU preempted by the host still counts as the thread's CPU time, so whatever else loads the host (the other variant's scroll under `JOBS=2`, or other work) inflates them. At 7680x2160 the CPU time per frame read 9.6 to 14.0 ms across five `JOBS=2` runs; at 1280x800 two `JOBS=1` runs of one build on a busy host read 0.91 and 1.15 ms, and 179 000 and 103 000 lines per second. Compare builds with `JOBS=1`, several runs each, on a host doing nothing else.

## The gate has been seen red

- Guest config: `palette=solarized` appended to `guest/kmscon.conf` turned the four golden cases of `2d-1280x800` red (929 760 to 1 024 000 pixels; `vt-switch`, a check, stayed green); restoring the file turned them green.
- Code: offsetting double-width glyphs by one pixel in `src/render/bbulk.c` (rebuilt with `make pkg`) turned `cjk` and `vt-restore` red (1 870 and 1 852 pixels, all within the CJK lines) and left `sgr` and `boxdraw` green; restoring the file and rebuilding turned them green.
- Concurrent: the same `palette=solarized` under `JOBS=2` turned the four golden cases of both 2D variants red at once (`2d-7680x2160`: 16 494 560 to 16 588 800 pixels); restoring the file turned them green.
- Key steps: Shift+PageUp's scroll call removed from `src/terminal.c` (`make tier3 VARIANTS=2d-1280x800`) failed `photo-scrollback`, whose screen never changed, and the cases after it; restored, all 26 passed.
- A2's verdict: 20 ms of CPU spent in every redraw (`do_redraw_screen`) failed `mpv-sixel` at 1280x800 with "kmscon is the bottleneck: kmscon's CPU time per frame mpv sent, 80.2 ms, is over the clip's 33.3 ms frame interval"; restored, it passed.
- A4: the `terminal-features` line removed from `tmux-grid.inc` left the goldens and the frame rates unchanged (tmux drew placeholders and kmscon still presented 75 frames a second at 99.75% damage), and failed `tmux-2x2` and `tmux-3x3` because tmux sent no sixel picture; restored, both passed.
- A2's verdict with a known bottleneck: `mpv-sixel` reporting no frame at a variant with a known bottleneck fails, since only slowness is known (`BottleneckTest.test_known`; the check removed, the test went red). With the known-bottleneck branch of `judge_rate` disabled, the same test failed on its own test-local entry.
- Tolerance: a golden altered by one channel level at one pixel, with a sidecar `max_delta=1`, `max_pixels=1`, gave `near` and exit 0; without the sidecar the same run gave `fail` and exit 2. The golden was then restored.

## The GL variant's black screen (#17), and its goldens

In 6 of 16 runs of `gl-1280x800` measured for #17, one kmscon instance drew nothing: every glyph upload failed with `text_gltex: cannot load glyph data into OpenGL texture (1282: <INVALID_OPERATION>)`, about 21 500 times a run, and the screen stayed black. The cause was in kmscon's gltex, not in virgl. Its atlas lookup took the first entry of its atlas list without checking that the list was empty; on an empty list that is the list head inside gltex's own state, read as an atlas. The fields it read were two floats and the low half of a heap pointer, so whenever address randomisation put that pointer low enough, the first glyph went to a texture name gltex never created, and every upload after it failed. PR #14's build (kmscon at c5b2fa5) reproduced it on this host in 8 of 13 runs, and a diagnostic build of it showed no atlas ever created in each failing run. 9e12309 (2026-10-22) had already replaced the lookup with a walk of the list, which an empty list ends at once. Since then the variant has drawn every glyph: 0 upload failures in 17 consecutive runs at 6cb4d45 on 2026-10-25, with both instances drawing in each run, 16 at 040f3a9 before it, and none in 12 diagnostic runs. #17's 6 in 16 was measured before the image screens existed, at c5b2fa5. `test_gltex` holds gltex to the GL texture rules the defect broke.

The variant joined the default `make tier3` once its goldens were blessed. Each was read beside the `2d-1280x800` golden of the same screen; the commit that blessed them gives the verdict for every case. Most differ from bbulk's by at most 1 level per channel at glyph antialiasing and blends, and `kitty` by at most 2 where both renderers enlarge its pictures bilinear. Five differ by more, each for a known reason: `sixel-evict-rotated` sits one pixel right of bbulk's (#186), `kitty-photo`, `kitty-files` and `kitty-layers` shrink their photograph bilinear where bbulk box-filters (#187), and `sixel-evict-history` still draws picture 1, since gltex's turned copy is held in its own budget and evicts nothing (#188). A fix to #186 moves `sixel-evict-rotated`'s golden. These pictures shrink 1.6:1 and #187 is about shrinking past 2:1, so a fix to #187 may move the three kitty goldens.

## Compare tool

`t3img.py` reads PPM and PNG and compares with a tolerance sidecar, stdlib only (`t3img.py compare ACTUAL GOLDEN [--tol FILE] [--diff D]`, exit 1 on `fail`). Tier 2's `pixcmp` covers the same ground and was built in parallel; merging them is aaronsb/kmscon#18.
