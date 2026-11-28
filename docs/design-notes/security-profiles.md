# Selectable security profiles for the drawer

This note is research, not a decision. The maintainer asked for it on 2026-11-21, when they answered the gap-check's first question on the privilege split. The drawer is the process that parses what sessions print and draws it. In the split's first two slices, ADR-1400 D4 gives it a basic setup: no new privileges, an empty bounding set, its ids read back, not dumpable, and a parent-death signal. The drawer has no seccomp filter and no landlock domain yet. The maintainer asked for a study of hardening kept out of the main program, in a crate of profiles chosen by hardware class or driver. Others could add detection modules to that crate, after the model of lm-sensors.

## Why one filter does not fit every machine

A seccomp filter lists the system calls a process may make, and a landlock domain lists the files it may reach. A filter that is too wide protects little, and one that is too narrow kills the process on a machine its author never tested.

The drawer's needs depend on the display hardware. On the CPU path, the drawer draws into dumb buffers, the plain memory buffers that the kernel's graphics driver displays. It makes the same few KMS calls on every card. On the GPU path, Mesa runs inside the drawer. Each Mesa driver makes the ioctls of its own kernel driver, such as `i915`, `amdgpu`, `nouveau`, `msm`, `panfrost` or `v3d`. It also loads its own shared libraries and reads its own files under `/sys` and `/dev/dri`. On fbdev, the drawer needs a different set again. The call numbers also differ by architecture: vtium builds for x86_64, aarch64, riscv64 and 32-bit ARM.

So a filter written for one driver breaks another, and a filter wide enough for all of them adds little. This is the problem the maintainer's request points at: choose the profile from what the machine has, and let people who own the hardware add the profiles.

vtium already writes a seccomp filter by hand for the font helper, with a table of call numbers for each architecture (`crates/vtium-font-helper/src/seccomp.rs`, ADR-1200 D15). The font helper needs no device, so one table serves every machine.

## Precedent

Each of these projects chooses a policy or a configuration by hardware or by program, and most let others add entries.

| Project | What it chooses, and by what | Who adds an entry | What vtium can take from it |
|---|---|---|---|
| lm-sensors | `sensors-detect` probes the buses for known chips and recommends the kernel modules to load. The package ships configuration files for many boards, which users contributed. | Users, for their own boards | Detection that only reads and recommends, kept apart from the configuration it selects. Contributed entries for hardware the authors never had. |
| udev's hardware database (hwdb) | Records matched by a device's modalias or DMI strings set properties, such as a keyboard's key codes. | Anyone, as plain data files | Matching by patterns on identifiers the kernel already exposes, with data and no code. |
| fwupd | Plugins detect and update devices. Quirk files, INI files in `quirks.d`, are keyed by a device's instance ID or GUID and choose the plugin and its settings. A file dropped into the directory adds a device without a new release. | Vendors and users, as data; plugins as code | A split between a few plugins, which are code, and many quirks, which are data. |
| Chromium's GPU process | The Linux sandbox has a generic seccomp policy for the GPU process, and separate policies for AMD, ARM, Intel, NVIDIA and virtio GPUs on ChromeOS. Older code gave each vendor its own list of files the broker lets the GPU process open, such as `/dev/mali0` for ARM Mali. | Chromium's authors | Per-driver policies over a generic one that every machine falls back to. The selection comes from the platform and a sandbox option, so an unknown GPU gets the generic policy. |
| Android's SELinux policy | The platform's policy is combined with the device's own, which `BOARD_SEPOLICY_DIRS` in the board's configuration names: usually one directory for the SoC and one for the device. Each hardware service (HAL) runs in a domain of its own. | The device's vendor | A policy for each hardware service, and a board's policy kept apart from the platform's. |
| ChromeOS and Android minijail policies | Each service ships a seccomp policy file for each architecture, such as one for amd64 and one for arm. | The service's authors | One policy file for each architecture, reviewed as plain text. |
| systemd's portable service profiles | A portable service attaches with one of four named profiles, `default`, `nonetwork`, `strict` or `trusted`, each a drop-in of sandboxing directives. Administrators can add profiles of their own. `systemd-analyze security` scores how exposed a unit is. | systemd, then administrators | A few named profiles, chosen at install time, and a score that shows what a profile leaves open. |
| Firejail | Each program has a profile, which includes shared files of common rules and reads a `.local` file where an administrator adds rules. | Firejail's community, then administrators | Shared building blocks and a local file that adds rules without editing the shipped profile. Firejail is under the GPL, so vtium reads only its documentation. |
| AppArmor | Profiles include abstractions, such as one for Mesa and one for the DRI device nodes, which many profiles share. | Distributions and upstream | One shared block for the GPU's files, reused by every program that draws. |

Two lessons run through the table. Detection reads identifiers that the kernel already exposes and never runs code from the device. Every project also has a generic policy that a machine falls back to when nothing matches, and it says which policy it chose.

Not every project works this way. OpenBSD's `pledge` and `unveil` keep the policy inside each program, written by its authors, and choose nothing by hardware. That is the model vtium's font helper follows today.

## The shape this suggests for vtium

This is a sketch for a later decision, not a design.

- **A crate of profiles.** A profile is a table: the calls the drawer may make, with the arguments that matter, the files and directories its landlock domain allows, and the architecture it applies to. A small baseline profile covers the CPU path, which every machine has.
- **Detection modules.** A module reads what the kernel exposes without privilege: each card's driver name from `/sys/class/drm/cardN/device/driver`, whether a render node exists, fbdev, and the architecture. It names the profiles that apply. Others add a module and its profile together, as lm-sensors users add a board.
- **Who applies it.** The broker detects the hardware before it starts the drawer, because it already opens the devices. The drawer installs the filter and the domain itself after its exec, as the font helper does. A seccomp filter and a landlock domain bind the process that installs them, and its children.
- **When nothing matches.** The drawer runs with the baseline profile on the CPU path, or with the setup of ADR-1400 D4 alone. It logs one line that names the driver it found no profile for. `vtium check-config` says which profile vtium would choose.

## Criteria a profile must meet

1. **It only narrows.** A profile never adds a capability, a file the drawer could not open without it, or a message to the broker's protocol.
2. **It is chosen from what the kernel exposes.** Detection reads sysfs, uevents and the architecture. It runs no code from a device and trusts no string a session can set.
3. **It fails into a known state.** When no profile matches, or a profile cannot be installed, vtium runs with the baseline and logs which profile it used and why. It never drops to no protection without saying so.
4. **It covers every path the drawer can take on that hardware.** That means the CPU path, the GPU path through Mesa, fbdev, a VT switch, a display hand-over, and a card plugged in while vtium runs. A filter cannot be widened once installed. So a profile plans for a card whose driver it does not cover, or such a card makes the drawer restart under a wider profile.
5. **It is tested on matching hardware or a stand-in.** Each profile has a test that runs the drawer under it, in tier 3's virtual machine where a virtio-gpu or llvmpipe stand-in fits. A second test plants a call the profile refuses and sees it refused.
6. **A refused call is visible.** The drawer dies with `SIGSYS`, and the broker restores the VT as it does after any end of the drawer. The log names the call, so a person with that hardware can report it.
7. **It is readable.** A profile is a short table with a reason for each entry, so a reviewer can judge a change to it from the diff.
8. **It follows the kernel's numbering.** Each profile holds its calls for each architecture vtium builds for. On a kernel with an older landlock ABI, it does what that ABI allows, as the font helper does.
9. **It is added without changing the drawer.** A new driver's profile and detection module need no change to the drawer's code or to the broker's protocol.
10. **It costs little on minspec.** The minspec build has no GPU path, so it carries the baseline alone.
11. **Its license fits.** A profile adapted from Chromium or Android keeps its license notice. A GPL project, Firejail among them, is a reference through its documentation only.

## Questions a decision would answer

- Whether profiles are code, compiled into a crate, or data files read at start. Data files would need the ownership check that ADR-1401 D10 gives the head file, because they decide what a root process lets the drawer do.
- Whether a card with a new driver, plugged in while vtium runs, restarts the drawer or is refused until the next start.
- Whether the profiles also serve the third slice's compositor and session processes. A session process needs no device, so its filter can be one table for every machine, like the font helper's.
- Whether the crate lives in vtium's workspace or in a repository of its own, so that people who own the hardware can add to it without following vtium's release.

## Sources

- lm-sensors: the [Arch Linux wiki page](https://wiki.archlinux.org/title/Lm_sensors) and the [lm-sensors repository](https://github.com/groeck/lm-sensors), on `sensors-detect` and the contributed board configurations.
- fwupd: the [plugin tutorial](https://fwupd.github.io/libfwupdplugin/tutorial.html) and its documentation of quirk files.
- Chromium: [`bpf_gpu_policy_linux.cc`](https://chromium.googlesource.com/chromium/src/+/8d62647be580bec89db33ea771c23e609d4bc09d/sandbox/policy/linux/bpf_gpu_policy_linux.cc), [`bpf_cros_amd_gpu_policy_linux.cc`](https://chromium.googlesource.com/chromium/src/+/0164fe22baf9c69a18152fcdc99cb15e6124def2/sandbox/policy/linux/bpf_cros_amd_gpu_policy_linux.cc), [`sandbox_seccomp_bpf_linux.cc`](https://chromium.googlesource.com/chromium/src/+/8d62647be580bec89db33ea771c23e609d4bc09d/sandbox/policy/linux/sandbox_seccomp_bpf_linux.cc), and the older [`gpu_sandbox_hook_linux.cc`](https://chromium.googlesource.com/chromium/src.git/+/64.0.3268.0/content/gpu/gpu_sandbox_hook_linux.cc) with its per-vendor file lists.
- Android: [building SELinux policy](https://source.android.com/docs/security/features/selinux/build) and [policy compatibility](https://source.android.com/docs/security/features/selinux/compatibility).
- systemd: [`PORTABLE_SERVICES.md`](https://sources.debian.org/src/systemd/259-1/docs/PORTABLE_SERVICES.md) and the profiles under `src/portable/profile/`.
- Firejail, AppArmor's abstractions, minijail and udev's hwdb are described from their documentation, which this note did not read again.
