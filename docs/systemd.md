# systemd, logind and PID 1

vtium does not need systemd. Its unit can start it, an init script can start it, or you can start it by hand. This page lists every place where vtium touches systemd, logind, or the conventions of a systemd PID 1. For each place, it also says what vtium does on a host without them. A touchpoint not listed here is a defect in this page.

This page uses a few terms:

- "Without systemd" means a host whose PID 1 is another init, such as sysvinit, OpenRC, runit, s6, or busybox, and that runs no logind.
- "Under elogind" means such a host that runs elogind, the standalone logind. elogind owns `org.freedesktop.login1` on the system bus. It ships libelogind in place of libsystemd. vtium's `logind` feature links whichever of the two libraries pkg-config finds, and it tries libsystemd first.
- vtium.conf's policy is `open` or `closed`, and a file without one is open. Under the closed policy, vtium ignores any keyboard that no rule names. See [the two policies](concepts/heads.md#two-policies-open-and-closed) for details.

Where systemd runs vtium through its units, vtium supports systemd 254 and newer. vtium also runs with no systemd at all. The **Without systemd** part of each touchpoint below describes how. Tier 3's variants without systemd test it (`make tier3-nosystemd`).

vtium does not support systemd 239 to 253. On those versions, the unit's `OnFailure=` starts the getty fallback only once vtium's unit stops restarting. Take an open policy in vtium.conf, and a failure that repeats more slowly than 5 starts in 10 s. That failure never trips the start limit, so it never brings a getty. For example, on a machine with no display, vtium waits `display-wait`, exits with status 1, and is restarted. This repeats over and over, and no login prompt ever comes. Tier 3 refuses a guest whose systemd is older than 254.

## Touchpoints

Each section below is one touchpoint. It says where the touchpoint lives in vtium's source and units. It says what vtium does with systemd, without systemd, and under elogind. Last, it names the build feature that the touchpoint needs, or none.

### The VT unit

**Where:** `tools/vt/units.sh` generates the VT unit, `vtium@.service`, from `crates/vtium/install/vtium@.service`. The package, `make install` and `make install-tree` install it with its getty fallback, `vtium-getty@.service`.

**With systemd:** The package's install gives vtium every free VT, as [Giving VTs to vtium](#giving-vts-to-vtium) describes. systemd starts vtium at boot on each free VT from 1 to 6, in place of its getty, through `getty.target.wants/vtium@ttyN.service`. So vtium holds each of those VTs open from boot. A display manager that picks the first free VT passes them by. With a display manager enabled, VT 1 stays the display manager's. On a free VT from 2 to 6 where vtium has stopped, logind starts `autovt@ttyN` when you switch to it. There `autovt@ttyN` is vtium, in place of getty.

The unit has `Type=notify` and `Before=display-manager.service`. vtium sends `READY=1` on the unit's notification socket once it holds its VT open. It does this before it opens a display card. So systemd starts the display manager only once every claimed VT is held open.

The unit runs with getty's ordering and TTY settings: `TTYReset`, `TTYVHangup` and `TTYVTDisallocate`. It has `Conflicts=getty@%i` with `After=getty@%i`, because `Conflicts=` alone orders nothing (systemd.unit(5)). A getty's stop on the VT hangs up the VT and resets it (`TTYVHangup=`, `TTYVTDisallocate=`). With `After=`, that stop finishes before vtium starts there, instead of running beside vtium's start. The unit has no sandboxing directive, because the login session that vtium starts would inherit every restriction that the unit set.

**Without systemd:** The unit is unused. An inittab line, an init script, or a supervisor runs `vtium --vt=ttyN --no-switchvt` where it ran a getty. The init does the TTY reset and hangup. vtium then starts at the boot, and waits for its VT to be shown. Tier 3's `sysvinit-640x480` tests this.

Two such lines start two vtiums at once, and each draws its own login. DRM master is the right to set what a display card shows. A vtium that opens the display card on a VT that is not shown lets go of the card's DRM master at once. So the vtium on the VT that is shown draws. Tier 3's `sysvinit-two-vts` tests this. See [the DRM master issue](https://github.com/vtium/vtium/issues/139).

**Under elogind:** As without systemd.

**Build feature:** none.

### A failure falls back to getty

When vtium fails, its unit falls back to a getty, with two exceptions. It does not fall back under a closed policy. It also does not fall back when the getty's start would stop another program, such as a display manager whose `Conflicts=` names the getty.

The unit restarts vtium after each failure, up to systemd's default limit of 5 starts in 10 s. Under an open policy, the getty's start stops the restarted unit. In systemd's default job mode, that start also stops a restart of vtium's unit that systemd has already scheduled. So the getty keeps the VT.

**Where:** the unit's `Restart=on-failure` and `OnFailure=vtium-getty@%i`. `vtium-getty@.service` defines the fallback unit, and it is generated from `crates/vtium/install/vtium-getty@.service`. The exit statuses are in `crates/vtium/src/app.rs` (`Exit::status`). The marker is in `crates/vtium/src/getty.rs`.

**With systemd:** Exit status 1 and a panic fail the unit. vtium exits with status 1 in these cases:

- when no display comes within `display-wait`;
- at the respawn limit under an open policy;
- when the VT hangs up;
- when a session cannot spawn.

The unit's `OnFailure=` then starts `vtium-getty@ttyN`, which runs `vtium console fallback ttyN`. That command starts getty on the VT, so a user has a login again. It starts no getty when the getty's start would stop another program. Such a program is a unit that runs, waits to start, or is enabled, and that conflicts with the getty, as a display manager conflicts with `getty@tty1`. Then it starts nothing, and the VT is left without a console. The journal of `vtium-getty@ttyN` says why.

Under a closed policy in vtium.conf, a getty's login would take any keyboard. So vtium writes the empty file `/run/vtium/ttyN.closed` once it has read the policy. It writes the file before it opens its loop or its VT, and it creates `/run/vtium` where it is missing. The fallback unit's `ConditionPathExists=!/run/vtium/%i.closed` skips the getty while the file exists.

vtium makes the VT's run directory, `/run/vtium/ttyN/`, itself, as [The run state](#the-run-state) describes. The marker sits one level up, beside that directory. vtium removes the marker when it starts under an open policy, and `/run` is cleared at every boot. A failure before vtium has read vtium.conf writes no marker. An example is a command line that vtium cannot start with. Then the getty starts, unless an earlier closed run of this boot left a marker. The marker governs the fallback alone.

A `getty@ttyN` may also be enabled on vtium's VT, as systemd's default preset enables `getty@tty1`. `getty.target` then wants it, beside vtium's unit. When both start in one transaction, as at boot, systemd starts vtium's unit, the one that declares `Conflicts=`, and stops the getty (systemd.unit(5)). The enabled getty still takes the VT in two cases:

- On a VT where logind starts vtium only when the VT is switched to, the getty starts at boot first. logind then starts nothing there.
- A `systemctl start getty@ttyN.service` while vtium runs stops vtium and starts the getty. Someone may run it by hand, or a unit may pull the getty in.

So a closed machine disables that getty (`systemctl disable getty@ttyN.service`). Once vtium has ended, the kernel takes `Ctrl-Alt-F<n>` again, so a getty on another VT is one key away. A kiosk therefore also masks the gettys enabled on the other VTs. It also sets logind's `NAutoVTs=0` and `ReserveVT=0` (logind.conf(5)). Otherwise logind starts `autovt@ttyN` on a VT as it is switched to, and keeps one ready on VT 6. With vtium's unit enabled, `autovt@ttyN` is another vtium, which starts a login of its own on that VT.

The unit has `Restart=on-failure` with systemd's defaults, and no `RestartSec=` or `StartLimit*` of its own. So systemd starts vtium again 100 ms after each failure. It allows 5 starts in a 10 s window, which opens at the first start it counts. These limits are `DefaultStartLimitIntervalSec=` and `DefaultStartLimitBurst=` in systemd-system.conf(5), and a host may change them.

A SIGKILL, a panic, and an exit with status 1 are failures. Two kinds of end start nothing. One is an exit with status 0, which vtium makes on SIGTERM and SIGINT. The other is an end by SIGHUP, SIGINT, SIGTERM or SIGPIPE, which systemd counts as clean.

Under a closed policy, a one-off failure therefore brings vtium back on its VT, while the marker keeps the getty away. A fault that recurs faster than 5 starts in 10 s trips the limit. The unit is left failed, with the result `start-limit-hit`. The fallback skips the getty, and the VT has no login. The operator then runs `systemctl reset-failed vtium@ttyN` and starts the unit, over ssh or a serial getty, or restarts the machine. Where logind starts vtium as `autovt@ttyN`, switching away from its VT and back also starts vtium again, once the 10 s have passed. A fault that recurs more slowly never trips the limit. vtium keeps restarting, and hangs up every session each time.

Under an open policy, on systemd 254 and later, each failure still starts `OnFailure=`. The default `RestartMode=normal` passes through the failed state, so the fallback starts the getty. vtium's unit conflicts with `getty@%i.service`, so the getty wins either way. If the getty starts before the restart, it cancels the restart. If it starts after the restart, it stops the vtium that came back 100 ms after the failure. systemd 239 to 253 are unsupported, as the start of this page says.

**Without systemd:** vtium exits with status 1 in the same cases. It writes and removes the marker in the same way, though nothing reads it. The init's respawn line or the script decides whether a getty follows.

**Under elogind:** As without systemd.

**Build feature:** none.

### Giving VTs to vtium

**Where:** `vtium console`, in `crates/vtium/src/console.rs`, and the packages' install scripts in [vtium-arch](https://github.com/vtium/vtium-arch).

**With systemd:** A package's first install runs `vtium console enable --direct --all`. That gives vtium, in direct mode, every VT that no display manager, greeter, graphical session or kmscon holds. The package's removal first runs `vtium console disable --all`, which gives the VTs back. An upgrade changes no VT. A claim may be refused, for example while kmscon holds the template link, or when `loginctl` fails. A refused claim fails neither the install nor the removal, and the package's message gives the reason.

`enable` and `disable` change which console systemd and logind start on a VT. They do this through `systemctl enable`, `disable`, `add-wants` and `daemon-reload`. They also change three links by hand, because no systemctl verb makes or removes these links as they were:

- the template link `/etc/systemd/system/autovt@.service`, pointed at `vtium@.service` or `vtium-seat@.service`;
- a VT's own `autovt@ttyN.service` link, moved between the two units or removed. `systemctl disable` leaves this link.
- a getty's want, linked back to the unit file it linked to before.

They record what they changed in `/var/lib/vtium/console/`. They need root.

Every verb finds out what holds each VT:

- It reads the links and unit files along systemd's unit search path.
- It reads the display managers' own configuration files: SDDM's, LightDM's and greetd's.
- It asks `systemctl list-units` which consoles run.
- It asks `loginctl list-sessions` and `loginctl show-session` which graphical sessions and greeters hold a VT.
- It reads each process's controlling terminal and unit from `/proc/PID/stat` and `/proc/PID/cgroup`. So a program running on a VT with no session, such as a display manager's X server, holds that VT.

`enable` leaves alone every VT that another program holds. None of the verbs starts or stops a unit.

`fallback` is the verb that the getty fallback runs. It asks what the getty's start would stop, with `systemctl show -p Conflicts -p ConflictedBy -- getty@ttyN.service` and `systemctl show -p Id -p ActiveState -p UnitFileState -p Job -- UNIT...`. It starts the getty with `systemctl --no-block start` only when that start would stop no other program.

**Without systemd:** `/run/systemd/system` does not exist, so every verb changes nothing. It says that the host was not booted with systemd. It says that the host's init runs vtium as [The VT unit](#the-vt-unit) describes. Then it exits with status 1. A package's install claims no VT there, and says so.

**Under elogind:** As without systemd.

**Build feature:** none.

### Rescue and emergency mode

**Where:** the unit's `Conflicts=rescue.service emergency.service` and `Before=rescue.service emergency.service`, in `crates/vtium/install/vtium@.service`.

**With systemd:** vtium steps aside, as `getty@.service` does. Both modes run the rescue shell. The rescue shell asks for root's password on `/dev/console`, the VT that is shown. systemd enters either mode on `systemctl rescue`, or with `systemd.unit=rescue.target` on the kernel command line. When it does, it first stops vtium on every VT. So the rescue shell's prompt shows through the kernel console. This holds under either policy in vtium.conf.

The unit's `IgnoreOnIsolate=yes` keeps vtium running when systemd isolates any other target. An example is a switch from `graphical.target` to `multi-user.target`. Without that setting, such a switch would end every session on the VT.

**Without systemd:** The init system's own rescue path decides what runs.

**Under elogind:** As without systemd.

**Build feature:** none.

### Turning vtium off for one boot

**Where:** `crates/vtium/src/kernel.rs` (`off_at_start`). It reads `/proc/cmdline` as vtium's first step.

**With systemd:** With `vtium.enable=0` on the kernel command line, vtium exits with status 1 at its start. It exits before it reads vtium.conf, and before it opens its VT or a card. It logs one line: "vtium is turned off for this boot: the kernel command line sets vtium.enable=0." The last `vtium.enable=` on the line decides. Any other value leaves vtium on. `vtium check-config`, `--help` and `--version` still run on such a boot, so you can check vtium.conf. The unit counts the exit as a failure, so the getty fallback gives each VT a login prompt. `systemd.unit=rescue.target` also works, and vtium steps aside for it, as [Rescue and emergency mode](#rescue-and-emergency-mode) describes.

Masking vtium's unit does not turn vtium off on every VT. logind starts vtium on a VT through the template link `autovt@.service`. A mask such as `systemd.mask=vtium@tty2.service` does not reach a unit loaded through a link. After the claim, VT 1 has no getty enabled. So a mask of `vtium@tty1.service` leaves VT 1 with no console.

**Without systemd:** `vtium.enable=0` makes vtium exit with status 1 in the same way. The init system then chooses what runs on the VT, as after any exit of vtium.

**Under elogind:** As without systemd.

**Build feature:** none.

### The kernel console and its messages

**Where:** The VT in graphics mode is in `crates/vtium-platform/src/vt/kernel.rs`. The kernel's messages are in `crates/vtium/src/kernel.rs`, which reads `/dev/kmsg`, `/proc/sys/kernel/printk` and `/run/plymouth/pid`. `App::kernel_ready` in `crates/vtium/src/app.rs` draws them. The setting is vtium.conf's `kernel-messages`.

**With systemd:** The kernel console is the text that the kernel draws on a VT, through fbcon, the framebuffer console, on a machine with a graphics card. vtium puts its VT in graphics mode, with the keyboard off. So fbcon stops drawing that VT, and vtium draws there instead. vtium leaves fbcon bound to the VTs, so every VT that vtium does not hold still shows text. Examples are a getty after a failure, the rescue shell's prompt, and a display manager's VT before its greeter draws. When vtium exits, it puts its VT back in text mode, and fbcon draws that VT again.

fbcon draws nothing on vtium's VT, so vtium shows the kernel's messages itself. It opens `/dev/kmsg` at its end, so it reads only the messages that arrive after it starts. It shows each message from the kernel itself whose level is warning or more severe, if the kernel's console log level lets that message through. vtium reads that level from the first field of `/proc/sys/kernel/printk`, for each batch of messages. So `quiet` or `loglevel=3` on the kernel command line shows fewer. When vtium cannot read the file, it uses the kernel's default, 7.

vtium draws the messages over the console of the head that holds the boot display, while its VT is shown. A head is a terminal for one or more displays. vtium cuts a message longer than 240 characters short, with an ellipsis. It never writes the messages into a session's screen or its pty. A message that arrives while vtium's VT is not shown is never shown later.

vtium.conf's key `kernel-messages` decides who shows the messages while Plymouth, the boot splash program, runs. vtium counts Plymouth as running while `/run/plymouth/pid` names a running process called `plymouthd`. With `after-plymouth`, the default, vtium leaves the messages to Plymouth while Plymouth runs. It shows them itself once Plymouth has quit. vtium never shows a message later that arrived while Plymouth ran. With `always`, vtium shows the messages whether Plymouth runs or not. Where vtium cannot open `/dev/kmsg`, it logs why, and shows no kernel message.

A kernel panic stops vtium with every other process. What the display then shows is the kernel's, through its DRM panic handler, where the kernel and the display driver support it. [The boot issue](https://github.com/vtium/vtium/issues/266) tracks starting vtium from the initramfs, so that it shows the boot from its first second.

**Without systemd:** The same. On a machine without Plymouth, `after-plymouth` shows the messages from vtium's start.

**Under elogind:** As without systemd.

**Build feature:** none.

### The log

**Where:** `crates/vtium/src/logger.rs`, which writes one line for each message on stderr.

**With systemd:** The unit sends stderr to the journal.

**Without systemd:** stderr goes wherever the init points it, such as the console or a log file.

**Under elogind:** As without systemd.

**Build feature:** none.

### The session's registration with logind

**Where:** `crates/vtium/src/child.rs`, and in a build with `libseat` and `logind`, `crates/vtium-platform/src/logind/scope.rs`.

**With systemd:** In direct mode and on seatd's seat, vtium runs `/bin/login -p` with `XDG_SEAT` and `XDG_VTNR` in its environment. `login` runs PAM, and `pam_systemd` registers the session with logind on that seat and VT. So the login is the seat's active session while its VT is shown.

In seat mode, vtium holds seat0 through a logind session of its own, as [vtium's own logind session](#vtiums-own-logind-session-in-seat-mode) describes. There each login runs in a systemd scope of its own, named `vtium-login-ttyN-PID.scope`. vtium asks systemd for the scope before the child runs `login`. The child waits until systemd has moved it there. `login` gets no `XDG_SEAT`, `XDG_VTNR` or `XDG_SESSION_*` variable. So `pam_systemd` gives the user a logind session of their own.

That session has `XDG_RUNTIME_DIR` and the user's `user@.service`, which two logins by the same user share. It has no seat, because logind gives a seat one active session, and vtium's session holds seat0. So polkit treats the session as inactive. Actions that polkit allows only to the active session ask for authentication. udev's uaccess rules give the active session access to devices such as sound cards. They do not apply to this session either.

When systemd cannot make the scope, vtium logs one line that says why. `login` then runs inside vtium's own session, with no session of its own. See [the libseat issue](https://github.com/vtium/vtium/issues/34). Tier 3's `seat-logind` and `seat-logind-two` check the user's own session, with no seat, and vtium's session still on seat0. Tier 3's `direct-logind` checks that the same build in direct mode leaves the seat to the login.

**Without systemd:** No logind session exists. The variables are only information for the session's programs. `login` logs in without a logind session. Tier 3's `sysvinit-640x480` tests this.

**Under elogind:** `pam_elogind` registers the session with elogind from the same variables. vtium sets `XDG_SEAT` and `XDG_VTNR` for `login` in direct mode. It also sets them in seat mode when libseat opens the seat through seatd. In seat mode, libseat's logind backend may open the seat through elogind instead. Then vtium drops those variables, as it does in seat mode under systemd, and asks systemd for a scope for each login. systemd is not PID 1 there, so that request fails. vtium logs one line, and the login runs inside vtium's own session. This is untested. See [the elogind issue](https://github.com/vtium/vtium/issues/100).

**Build feature:** none. The scope needs `libseat` and `logind`.

### The seat's name

**Where:** `crates/vtium-platform/src/vt.rs` (`Vt::seat`), `crates/vtium/src/main.rs`.

**With systemd:** `seat0`, logind's default seat. vtium asks logind nothing about seats.

**Without systemd:** `seat0`, the name that `ID_SEAT` defaults to.

**Under elogind:** As with systemd.

**Build feature:** none.

### The seat's devices

**Where:** `crates/vtium-platform/src/udev.rs`, through libudev, which is not libsystemd.

**With systemd:** systemd-udevd tags each device with `ID_SEAT` and `ID_PATH`. It announces hotplugs on the monitor socket.

**Without systemd:** With eudev or another udev daemon, the same. With no udev daemon, the scan still finds the devices in sysfs. But no device carries `ID_SEAT`, so vtium takes each one as `seat0`. No device carries `ID_PATH`, so a `mode=` line naming a card matches none. No hotplug arrives. The `path`, `serial`, `card` and `connector` matches of vtium.conf's rules read udev's database too. So they match nothing then, as `udev:` does.

Tier 3's `sysvinit-no-udev` proves that vtium draws and reads its keyboard with the daemon stopped and its database removed before vtium starts. The boot's coldplug had run, so the device nodes and their permissions are udev's. A host that never ran udev is not tested.

**Under elogind:** As without systemd.

**Build feature:** none.

### The run state

**Where:** the directory `/run/vtium/<tty>/`. So far it holds only the holder's socket, `hold` (`crates/vtium/src/survive/hold.rs`). The holder is the process that keeps sessions alive across a restart, as [Sessions that survive a vtium restart](#sessions-that-survive-a-vtium-restart) describes.

**With systemd:** With the unit's file-descriptor store, vtium needs no holder, so it makes no run directory yet. The rest of the state planned for the directory is not built yet. An example is a candidate staged for [the trial of an installed configuration](https://github.com/vtium/vtium/issues/331). When vtium needs the directory, it creates it itself, as it does without systemd. The unit sets no `RuntimeDirectory=`.

**Without systemd:** vtium runs as root, and creates the directory for the holder's socket itself, one directory for each VT. The directory is root-owned, and writable by no one else. vtium refuses a directory there that another user owns, or that its group or others can write.

**Under elogind:** As without systemd.

**Build feature:** none.

### Sessions that survive a vtium restart

**Where:** `crates/vtium/src/survive.rs` with `survive/notify.rs` and `survive/hold.rs`, and `crates/vtium/src/app/survive.rs`. The unit's `NotifyAccess=main`, `FileDescriptorStoreMax=50` and `KillMode=process`.

**With systemd:** vtium gives the unit's file-descriptor store a copy of each session's pty master, through `NOTIFY_SOCKET`. It sends `FDSTORE=1` with `FDNAME=pty.<console>.<head>` and `FDPOLL=0`. It drops the copy with `FDSTOREREMOVE=1` when the session ends or its console closes. vtium writes the datagrams itself, and links no libsystemd.

When the unit starts vtium again, after `systemctl restart` or a crash, systemd passes the masters back from descriptor 3 up (`LISTEN_FDS`, `LISTEN_FDNAMES`). Each console takes its session back once its head starts. The head opens exactly the consoles that were kept. vtium sets the pty to the console's size, and makes the program redraw with a SIGWINCH. The screen's contents and scrollback are gone.

A logged-in session lives in its logind scope, which systemd leaves alone. `KillMode=process` keeps two more things alive through the restart: a login prompt, and a program that the `login` key runs without a PAM session. When the unit stops for good, systemd closes the masters, and the kernel hangs those sessions up. When the store refuses vtium's messages, vtium gives the store up with one warning. The sessions then end if vtium restarts.

Under an open policy, a failure starts the getty fallback, whose start stops the unit. So a crash hangs the sessions up there. A restart keeps the sessions under either policy. A crash keeps them under a closed policy. None of systemd's variables reach a session.

**Without systemd:** vtium runs as root on a VT. It starts its holder, `vtium hold`, before it blocks any signal. The holder leads a session of its own. It listens on `/run/vtium/ttyN/hold`, which only root may open. It keeps a copy of each master that vtium sends it, until vtium says the session ended. The vtium that starts next on that VT takes copies of them back, as above.

The holder exits once no vtium is connected and it holds no master. An init may stop a vtium and never start it again. That vtium leaves its sessions running in the holder, until someone ends `vtium hold`. A vtium run as an ordinary user keeps nothing, and its sessions end with it.

**Under elogind:** As without systemd.

**Build feature:** none.

### The VT on screen when vtium stops or restarts

**Where:** `leave` in `crates/vtium-platform/src/vt/kernel.rs`, and in a build with `libseat`, in `crates/vtium-platform/src/seat.rs`. The unit's `--no-switchvt`.

**With systemd:** When you stop or restart `vtium@ttyN` while VT N is on screen, VT N stays on screen. The unit starts vtium with `--no-switchvt`, so vtium waits for its VT to be shown, and never switches to it. At its exit, vtium switches back to another VT only when it switched to its own VT itself. So here it switches to none. The unit's `TTYReset=`, `TTYVHangup=` and `TTYVTDisallocate=` then reset the VT, as they reset a getty's VT. They switch to no other VT either. The kernel does not free the VT on screen (`VT_DISALLOCATE` fails with `EBUSY`), so systemd clears its screen instead. After a restart, the new vtium finds its VT on screen, and draws there at once.

A vtium started without `--no-switchvt`, as from a terminal on your desktop, switches to its VT when it starts. At its exit, it switches back to the VT that was on screen before that switch.

**Without systemd:** The same. An init's line runs vtium with `--no-switchvt`. So when the init starts vtium again, the VT on screen stays where it is.

**Under elogind:** As without systemd.

**Build feature:** none.

### Devices and VT switching

**Where:** `crates/vtium-platform/src/vt.rs` and `src/vt/kernel.rs`: the kernel's VT in `VT_PROCESS` mode, with devices opened directly as root. With `libseat`, `crates/vtium-platform/src/seat.rs`, and `crates/vtium-platform/src/input.rs`, which opens the keyboards and pointers through it.

**With systemd:** Built without `libseat`, vtium uses no logind session control (`TakeControl`, `TakeDevice`). logind sees vtium's VT switches as the kernel reports them.

Built with `libseat`, vtium looks for a seat manager at its start. When one answers, vtium takes the cards, the keyboards, the pointers and the VT switching from it. When the seat manager is logind, vtium does this through libseat's logind backend. So vtium opens no input device itself (ADR-1400's L2 layer). When no seat manager answers, vtium runs on the kernel's VT, as it does without the feature.

vtium lets libseat reach logind only when vtium is in a logind session of its own, as in seat mode. vtium is in such a session when `XDG_SESSION_ID` is set, or when `/proc/self/cgroup` puts vtium in a `session-ID.scope`. In direct mode, it is in none. There, when `LIBSEAT_BACKEND` names no backend, vtium sets `LIBSEAT_BACKEND=seatd` at its start, and logs one line. It sets the variable before any thread runs, and after it has read the environment that its sessions inherit. libseat then tries seatd alone on every open, the retries included. So a busy seatd never lets libseat fall through to logind's backend, which would take root's display session (ADR-1400 D3). Where seatd does not run, the open fails, and vtium runs on the kernel's VT. When `LIBSEAT_BACKEND` names logind outside a session, vtium logs one line, and runs on the kernel's VT.

**Without systemd:** The same as with systemd, with seatd as the seat manager. Where no seat manager runs, vtium runs on the kernel's VT. Under elogind, vtium finds that it is in a session only by `XDG_SESSION_ID`.

**Under elogind:** The same as with systemd.

**Build feature:** `libseat`, which is off by default and in minspec.

### vtium's own logind session, in seat mode

**Where:** `vtium-seat@.service`, which `tools/vt/units.sh --seat` generates from `vtium@.service` and the lines of `crates/vtium/install/vtium-libseat.conf`. The PAM file `crates/vtium/install/vtium.pam`, staged as `/etc/pam.d/vtium`. `make install` and `make install-tree` stage both files when `FEATURES` names both `libseat` and `logind`. `vtium console enable --seat` gives a VT to that unit (ADR-1500 D7).

**With systemd:** vtium runs in direct mode through `vtium@.service`, or in seat mode through `vtium-seat@.service`. A VT runs one mode at a time, because the two units conflict. The seat unit sets `User=root` and `PAMName=vtium`. So systemd runs the PAM stack `vtium` for vtium on its VT. `pam_systemd` then registers a logind session of vtium's own. libseat's logind backend takes the seat from that session.

The stack is the kind a display manager's greeter uses. It asks for no password, and it passes `class=greeter`, so the session's class is `greeter`. Without that class, `pam_systemd` gives vtium's root session the class `user-early`. logind stops a `user-early` session when `StopIdleSessionSec=` is set and the session's tty looks idle, as vtium's always does. Also, a greeter's session does not count as another user's when the machine powers off (ADR-1400 D3). Tier 3's `login-session` check asserts the class.

systemd refuses `PAMName=` beside direct mode's `KillMode=process`, so the seat unit sets `KillMode=mixed`. systemd still signals vtium alone at a stop or restart. Then it ends the unit's other processes, such as a login prompt that nobody has used yet. Each login that vtium starts runs outside this session, in a scope of its own, as [The session's registration with logind](#the-sessions-registration-with-logind) describes.

A build with `libseat` alone is for seatd. It stages neither file, and opens no PAM session. Its logins run as in the default build. `vtium console enable` with no mode recommends direct mode, because its logins get an active session on seat0.

**Without systemd:** The unit is unused, so no session is opened. libseat talks to seatd.

**Under elogind:** An init system may run vtium through a PAM session of the stack `vtium`. Then the stack's `-session optional pam_elogind.so class=greeter` line registers the session with elogind, as a greeter's. The `-` keeps PAM quiet where `pam_elogind` is not installed, as `pam_systemd`'s line does under elogind. This is untested, because no tier runs seat mode under elogind. See [the elogind issue](https://github.com/vtium/vtium/issues/100).

**Build feature:** `libseat` and `logind`.

### `key-reboot` when `kernel.ctrl-alt-del` is 0

`kernel.ctrl-alt-del` is the kernel setting (sysctl) that decides what Ctrl+Alt+Del does.

**Where:** `crates/vtium-platform/src/power.rs` (`interrupt_init`), `crates/vtium/src/power.rs`.

**With systemd:** vtium sends `SIGINT` to PID 1. systemd takes it as `ctrl-alt-del.target`, a reboot that stops the services and unmounts the filesystems.

**Without systemd:** `SIGINT` to PID 1 is the kernel's own Ctrl+Alt+Del convention. sysvinit runs its inittab `ctrlaltdel` entry, `shutdown -r`. That stops the services in runlevel 6 before the restart. Tier 3's `reboot-sysvinit` tests this. busybox init, runit and s6 have their own handling, which vtium does not test.

**Under elogind:** As without systemd.

**Build feature:** none.

### `key-reboot` when `kernel.ctrl-alt-del` is anything else

**Where:** the same files, `restart_kernel`.

**With systemd:** vtium calls `sync()` and `reboot(RB_AUTOBOOT)`. The kernel restarts at once, with no shutdown by systemd.

**Without systemd:** The same. No init is involved. Tier 3's `sysvinit-restart` tests this.

**Under elogind:** The same.

**Build feature:** none.

### `key-power-off`

**Where:** `crates/vtium-platform/src/power.rs` (`power_off_init`), with libc's `SIGRTMIN`.

**With systemd:** vtium sends `SIGRTMIN+4` to PID 1. systemd takes it as `poweroff.target`. It stops the services, unmounts the filesystems, and powers off.

**Without systemd:** sysvinit catches the signal too, and powers off. It goes to runlevel 0, stops the services, and then powers off. Tier 3's `power-off-sysvinit` tests this. An init that does not catch the signal, such as busybox init, never sees it. The kernel drops a signal that PID 1 does not catch, and nothing powers off. vtium does not test this. See [the busybox init test issue](https://github.com/vtium/vtium/issues/140).

**Under elogind:** As without systemd.

**Build feature:** none.

### Blank and wake from outside the session

**Where:** `crates/vtium/src/blank.rs`, `crates/vtium-platform/src/rt_signal.rs`.

**With systemd:** `SIGRTMIN+1` blanks the displays, and `SIGRTMIN+2` wakes them. `systemctl kill --kill-whom=main -s SIGRTMIN+1 vtium@ttyN` sends one of them to the vtium on VT N alone. Without `--kill-whom=main`, the login session's programs get the signal too, and end.

**Without systemd:** `vtium blank --vt N` and `vtium wake --vt N` find the vtium on VT N, and signal it.

**Under elogind:** As without systemd.

**Build feature:** none.

### Whether PID 1 takes the power-off signal

**Where:** `crates/vtium/src/check.rs` (`semantics`).

**With systemd:** systemd blocks `SIGRTMIN+4` and reads it from a signalfd. So the `SigBlk` mask in `/proc/1/status` has its bit, and vtium gives no warning. Tier 3's `power-off` tests this.

**Without systemd:** sysvinit catches the signal with a handler. So the `SigCgt` mask has its bit, and vtium gives no warning. Tier 3's `power-off-sysvinit` tests this. PID 1 may neither catch nor block the signal. Then a bound `key-power-off` warns at start, and in `check-config`, that the key will not work. Where `/proc/1/status` cannot be read or holds no mask, it warns that the key may not work.

**Under elogind:** As without systemd.

**Build feature:** none.

### `key-suspend`

**Where:** `crates/vtium-platform/src/power.rs` (`suspend`), `src/logind.rs`.

**With systemd:** Built with `logind`, vtium calls logind's `Suspend` over the system bus, through sd-bus. It waits at most 2 s for the answer. So logind's delay inhibitors hold the sleep, and logind's `PrepareForSleep` signal runs other sessions' lock-on-suspend. vtium reports a refusal, and never works around it. Built without `logind`, as in minspec, vtium acts as it does without systemd.

**Without systemd:** There may be no logind on the bus. Or a bus error may say that logind cannot be started, because it is masked or fails to spawn. Then vtium writes `mem` to `/sys/power/state` as root, through a shell that vtium starts with no environment. So vtium's loop serves the kernel's switch to its suspend console. No inhibitor holds the sleep, and no `PrepareForSleep` is sent. vtium logs a failed write as an error.

**Under elogind:** elogind owns `org.freedesktop.login1`. So a build with `logind` reaches its `Suspend` through libelogind's sd-bus, or through libsystemd's where both are installed. The minspec build writes `/sys/power/state`.

**Build feature:** `logind`, with sd-bus from libsystemd or libelogind. It is off in minspec.

### The image helper's identity

**Where:** `crates/vtium-platform/src/image_helper/identity.rs` and `login.rs`, and the account file `crates/vtium/install/vtium-image.conf`.

**With systemd:** With `logind` built in and logind running, the image helper runs as the user of the login child's own logind session. vtium finds that session through sd-login before each request. Before anyone logs in, vtium refuses files and shared memory. It decodes inline images as `vtium-image`. That is an account with home `/`, no login shell, and no files. `systemd-sysusers` creates it from the sysusers file `vtium-image.conf`. The package, `make install` and `make install-tree` stage that file in a build with images. Where the account is missing, vtium decodes as `nobody`.

**Without systemd:** The image helper runs as the owner of the session's pty, as `login` sets it. vtium reads the owner before each request, by reopening the pty's slave from its master. A pty that root owns is the login prompt. There vtium refuses files, and decodes inline images as `vtium-image`. That account exists only where a package script or the administrator created it. Otherwise vtium decodes as `nobody`. vtium never runs the image helper as root by falling back to the pty's owner.

**Under elogind:** sd-login comes from libelogind, the library that the `logind` feature links. The account file needs `systemd-sysusers`, which elogind hosts may lack. So the image helper runs as `nobody` there, unless the account was made another way.

**Build feature:** `images`, with `logind` for sd-login.

## Giving vtium a VT without systemd

Without systemd, `vtium console` changes nothing, and says so. The VTs that are not vtium's are the ones that your init system's configuration gives to a getty, another console, or a display manager. You give vtium a VT in the way your init system already runs a getty there. Replace the getty's entry for that VT with one that runs `vtium --vt=ttyN --no-switchvt`. Let the init system restart vtium when it exits.

vtium adds no fallback of its own there. When vtium keeps failing, the init system's own respawn limit applies, and you log in on another VT. In the usual layout, VTs 1 to 6 hold consoles, and a display manager runs on a VT of its own, usually 7. Keep a getty on VT 1, so that a login is there if vtium cannot start. Give vtium VTs from 2 to 6. Each entry starts vtium at boot. Two entries that start at once race for DRM master, and one of them loses. See [the DRM master race issue](https://github.com/vtium/vtium/issues/139).

With sysvinit, change the VT's line in `/etc/inittab`. Then run `telinit q`, so that init reads the file again. Debian's line for VT 2 becomes:

```
2:23:respawn:/usr/bin/vtium --vt=tty2 --no-switchvt
```

init starts vtium at the runlevel's start, and starts it again each time it exits. Tier 3's guest without systemd runs vtium this way (`sysvinit-640x480`). It uses a wrapper that only sends vtium's log to the serial port.

With OpenRC, each VT's getty is a service linked to `/etc/init.d/agetty`, such as `agetty.tty2`. Write `/etc/init.d/vtium` in the same shape:

```sh
#!/sbin/openrc-run
description="vtium on ${RC_SVCNAME#*.}"
supervisor=supervise-daemon
command=/usr/bin/vtium
command_args="--vt=${RC_SVCNAME#*.} --no-switchvt"
```

Make it executable, link it for the VT, and swap the services in the default runlevel:

```
# ln -s vtium /etc/init.d/vtium.tty2
# rc-update del agetty.tty2 default
# rc-update add vtium.tty2 default
```

`supervise-daemon` starts vtium again when it exits. On a system whose OpenRC runs under sysvinit's init, the gettys are in `/etc/inittab` instead, and the sysvinit steps apply.

Alpine runs OpenRC under busybox init. Its `/etc/inittab` line for VT 2 reads `tty2::respawn:/sbin/getty 38400 tty2`. Replace it with `tty2::respawn:/usr/bin/vtium --vt=tty2 --no-switchvt`. Then run `kill -HUP 1` to make init read the file again, because busybox has no `telinit q`.

With runit, as on Void Linux, each VT's getty is a service directory, such as `/etc/sv/agetty-tty2`. A link in `/var/service` enables it. Make the file `/etc/sv/vtium-tty2/run`, and make it executable:

```sh
#!/bin/sh
exec /usr/bin/vtium --vt=tty2 --no-switchvt
```

Then swap the links:

```
# rm /var/service/agetty-tty2
# ln -s /etc/sv/vtium-tty2 /var/service/
```

`runsv` starts vtium again when it exits. vtium's tests run the sysvinit way only. The OpenRC and runit steps follow those systems' own getty services, and vtium does not test them.

## What vtium never does

- vtium reads none of logind's state files under `/run/systemd`, because systemd does not document those files as an interface.
- When vtium runs as the console, it never asks what PID 1 is. It reads only which signals PID 1 catches. The subcommand `vtium console` checks whether the host was booted with systemd. It does this by checking whether `/run/systemd/system` is a directory, as sd_booted(3) does. Then it asks the running manager which consoles run, with `systemctl list-units`. It asks logind which graphical sessions run, with `loginctl`.
- In the minspec build, vtium links no libsystemd or libelogind. `ldd` on that build lists neither.
- vtium sends PID 1 no other signal. It calls `reboot(2)` only for `key-reboot` at a non-zero `kernel.ctrl-alt-del`.

## Where it is tested

Tier 3 runs two guests, which `tools/tier3/README.md` describes. The Arch Linux guest runs systemd and logind. Its power variants prove the reboot paths, power-off, and suspend, both through logind and through `/sys/power/state`.

The guest without systemd runs Debian, with sysvinit as PID 1 and no logind (`make tier3-nosystemd`). It proves this page's parts for a host without systemd:

- vtium started by an inittab line, and drawing, with the udev daemon and without it;
- the reboot through the inittab's `ctrlaltdel` entry, and the kernel's own reboot;
- the power-off through sysvinit.

Each touchpoint that it proves names its variant or case.

The tests in `crates/vtium/src/getty.rs` test the closed marker against a stand-in directory. On the Arch guest, tier 3's heads variants test it:

- `heads-closed-broken` runs under a closed policy. Its `getty-fallback` check fails vtium by a kill, a crash, and a hangup. After each, the fallback unit skips itself, no getty runs on the VT, and the unit's `Restart=` line starts vtium again. Its `start-limit` check kills each vtium that the unit starts, until the unit stops restarting. Past the limit, the unit stays failed with `start-limit-hit`, and no getty runs.
- Under an open policy, the getty starts, and its start stops vtium's unit. `heads-partly-open` runs both checks as the open control. `heads-open-nodisplay` checks this after no display came within `display-wait`.

These are still untested: busybox init, runit and s6, an init that ignores `SIGRTMIN+4`, and elogind. See [the busybox init test issue](https://github.com/vtium/vtium/issues/140) and [the elogind issue](https://github.com/vtium/vtium/issues/100).

Tier 3's `heads-survive` restarts vtium's unit, and then kills vtium, under a closed policy. After each, the same logged-in shell leads its session on the same pty, at the same size. The shell answers a line typed on the console's keyboard.

The tests in `crates/vtium/src/survive/tests.rs` test the store's datagrams against a stand-in for the notification socket. The stand-in checks what systemd 262 checks. Only unit tests test the holder, because no tier runs it on a host without systemd yet.

The unit tests stand in for PID 1, logind and the kernel (`crates/vtium/src/power.rs`, `crates/vtium/tests/power_keys.rs`).

`make test-tools` checks three things:

- The unit that `make install` stages steps aside for rescue and emergency mode.
- A build with `libseat` and `logind` stages `vtium-seat@.service` and the PAM file.
- A build with `libseat` alone stages neither.

No tier enters rescue mode yet.

Tier 3's `seat-logind` variants turn seat mode on with `vtium console enable --seat --all`, and open vtium's PAM session. Their `login-session` check proves that each login gets a logind session of its own.

The tests in `crates/vtium/src/console/tests.rs` test `vtium console` against a scratch root and a stand-in systemctl. The stand-in does what systemd 262's `systemctl --root` was measured doing. No tier runs `vtium console` against a running systemd yet.
