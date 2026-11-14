# Seats and sessions

You have probably logged in at a text console: you switch to it with Ctrl+Alt and a function key, type your name and your password, and get a shell. vtium gives you the same login prompt. Behind that prompt, two questions have different answers on different machines. The first is which program opens the screen and the keyboard. The second is which program keeps track of your login session. That program decides whether you can play sound, mount a USB stick or shut the machine down without a password.

This page answers both questions for each setup vtium runs in. It starts from what is the same on every machine. Then it follows two forks: how vtium is built, and which init system and seat manager the machine runs. On a systemd machine with the build that uses libseat and logind, a third fork is the mode you choose for each VT. The figure below shows the tree, and each section below it is one of its branches.

![A tree that reads from left to right. At its root, vtium runs on a VT in place of getty, and /bin/login logs you in. The tree first forks by how vtium is built. In the default build, vtium opens the screen and the keyboards itself. The build with libseat and logind is the vtium package, and it asks a seat manager for its devices first. Each build then forks by the init system and the seat manager, and each branch ends in what your login gets. In the default build: with systemd and logind, your login is the seat's active session while its VT is shown; with elogind, the same through elogind, untested; with seatd or no seat manager, your login gets no session, and your Unix groups give you your devices. In the build with libseat and logind: with systemd and logind, you choose a mode for each VT. In direct mode, which is the advice, your login is the seat's active session while its VT is shown. In seat mode, vtium's own session holds the seat, and your login gets a session with no seat. With elogind, it works as in direct mode, untested. With seatd, vtium takes the screen and the keyboards from seatd, and your login gets no session. With no seat manager, it works as the default build: vtium opens its devices itself, and your login gets no session.](../figures/concepts-seats-tree.svg)

## What is the same everywhere

A Linux machine has several virtual terminals (VTs). A VT is one of the text screens you switch between with Ctrl+Alt and a function key. On most machines, a program called getty shows the login prompt on each VT. vtium takes the place of getty: it runs on a VT, draws the screen itself, and shows a login prompt there.

When you type your name, vtium runs `/bin/login`, the same program a getty runs. `login` asks for your password and starts your shell. Before it does, `login` runs PAM. [PAM](https://en.wikipedia.org/wiki/Pluggable_authentication_module), the pluggable authentication modules, is the library that checks your password. PAM also runs the steps a system adds to every login, such as registering the login with a session manager.

Each VT that runs vtium runs a vtium process of its own, with its own login. vtium runs as root, because it opens the display and the input devices.

On a systemd machine, the command `vtium console enable` gives VTs to vtium. With `--all`, it gives vtium VT 1 and each VT from 2 to 6 that no other program holds. On a machine with a display manager, the program that shows a graphical login screen, VT 1 stays the display manager's.

systemd starts vtium at boot on each VT that vtium has. Each vtium tells systemd that it is ready as soon as it holds its VT open, before it opens the graphics card. systemd starts the display manager only after every vtium is ready. So each of vtium's VTs is held open before the display manager starts, and the display manager stays off them. For example, sddm from version 0.20 puts each new desktop session on the first VT that no program holds open, so it takes a VT above vtium's.

vtium may have taken your VTs before it started at boot on each of them. If so, `vtium console status` prints a line that names each VT from 2 to 5 that nothing holds open at boot. The line ends with the command that fixes it, such as `sudo vtium console enable --direct --all`. On a machine with another init system, your init's configuration starts vtium on a VT where it started a getty. [Install VTium](../install.md#what-the-install-does-to-your-vts) shows both.

## Seats, sessions and the programs that manage them

A few words come up in every branch of the tree.

- A **session** is one person's login, from the moment they log in until they log out. It holds every program that login starts.
- A **seat** is the set of devices one person sits at: a screen, a keyboard and a mouse, and the sound card and the USB ports that belong with them. Most machines have one seat, called `seat0`. See [systemd's page on seats](https://www.freedesktop.org/wiki/Software/systemd/multiseat/) for details.
- [**logind**](https://www.freedesktop.org/software/systemd/man/latest/systemd-logind.service.html) is the part of systemd that keeps track of sessions and seats. A seat has at most one **active session** at a time: the session on the VT that is shown. When you switch VTs, logind makes the session on the new VT the active one.
- [**elogind**](https://github.com/elogind/elogind) is logind taken out of systemd, so that machines with another init system can run it.
- [**polkit**](https://www.freedesktop.org/software/polkit/docs/latest/polkit.8.html) decides whether a program may do something that needs more rights than you have, such as shutting the machine down or mounting a USB stick. Its rules often allow an action to the active session without a password, and ask for an administrator's password from any other session. polkit asks logind which session you are in and whether that session is active.
- **uaccess** is a set of udev rules. udev is the program that sets up device files as devices appear. The uaccess rules give the active session's user access to the seat's devices, such as the sound card and removable disks, and take it away when another session becomes active.
- [**seatd**](https://git.sr.ht/~kennylevinsen/seatd) is a small seat manager. It hands the screen and the keyboard to a program that asks for them, and it handles VT switches for that program. seatd keeps no record of sessions.
- **libseat** is the library that seatd ships. A program uses it to get its devices from seatd, or from logind, without knowing which one runs.

## The first fork: how vtium is built

vtium comes in two builds that matter here.

- **The default build** opens the screen, the keyboards and the pointers itself, as root. It handles VT switches through the kernel, and it asks no seat manager for anything.
- **The build with libseat and logind** asks a seat manager for its devices through libseat when it starts. When a seat manager answers, vtium takes the graphics card, the keyboards and the pointers, and VT switching from it. When none answers, vtium opens its devices itself, as the default build does. The `vtium` package is built this way. This build also ships a second systemd unit, `vtium-seat@.service`, and the PAM file `/etc/pam.d/vtium`, which seat mode needs.

A build with libseat but without logind is meant for seatd. It ships neither the second unit nor the PAM file, and its logins work as in the default build.

## The default build

### With systemd and logind

vtium opens the screen and the keyboards itself. logind sees vtium's VT switches as the kernel reports them.

When you log in, vtium starts `login` with two variables that name the seat and the VT, `XDG_SEAT` and `XDG_VTNR`. `login` runs PAM, and PAM's `pam_systemd` module registers your login with logind on `seat0` and on vtium's VT. So logind makes your login the seat's active session while its VT is shown. polkit treats your session as active, and the uaccess rules give you the seat's devices. You get what a login at a getty gives you.

### With elogind

This works as with logind. PAM's `pam_elogind` module registers your login with elogind from the same two variables, so your login is the seat's active session while its VT is shown. No test of vtium runs under elogind yet, so this is untested. See [the elogind issue](https://github.com/vtium/vtium/issues/100).

### With seatd, or with no seat manager

The default build does not use seatd, so these two cases are the same. vtium opens the screen and the keyboards itself, and the kernel handles VT switches.

No program keeps track of sessions, so nothing registers a session for your login. `login` still logs you in, and the seat variables are only information for the programs you run. The uaccess rules give nobody devices. You reach the sound card or a disk through Unix groups such as `audio`, as on any machine without logind. polkit, where it is installed, finds no session for you, so it applies its rule for any process, which for most actions asks for an administrator's password.

## The build with libseat and logind

### With systemd and logind: two modes

On a systemd machine with logind, this build runs on each VT in one of two modes. You choose the mode for each VT with `vtium console enable`. A VT runs one mode at a time, because the two units conflict.

#### Direct mode

`vtium console enable --direct` gives the VT to `vtium@.service`. The package's install script turns on direct mode on every free VT.

In direct mode, vtium holds no logind session of its own. Because of that, vtium never asks logind for its devices in this mode. logind hands devices only to a program that controls a session. When the program asking has no session of its own, libseat picks one for it: the session of the user the program runs as, which for vtium is root. So vtium would take control of another person's session, such as a root login on another VT. seatd works differently: it hands the devices to whichever program asks, with no session needed. So if seatd runs, vtium takes the graphics card, the keyboards and the pointers, and VT switching from seatd. If seatd does not run, vtium opens its devices itself, as the default build does.

Your login works as in the default build: `pam_systemd` registers it with logind on `seat0` and on vtium's VT. Your login is the seat's active session while its VT is shown. polkit treats it as active, and the uaccess rules give you the seat's devices.

#### Seat mode

`vtium console enable --seat` gives the VT to `vtium-seat@.service`. Seat mode makes no new seat. The machine still has one seat, `seat0`, and vtium holds it, so a person who logs in gets a session with no seat. As that unit starts vtium, systemd runs the PAM file `/etc/pam.d/vtium`, and PAM registers a logind session for vtium itself. It is the kind of session a display manager's login screen opens: it asks for no password, and logind marks it as a greeter's. vtium then takes the graphics card, the keyboards and the pointers, and VT switching from logind through libseat. vtium's own session holds `seat0`.

Your login is kept off the seat on purpose. If your login registered on `seat0` with vtium's VT, logind would make your login the seat's active session. logind would then take vtium's devices away, and vtium could no longer draw or read the keyboard. So when you log in, vtium starts `login` in a systemd scope of its own. A scope is a group of processes that systemd tracks apart from vtium. vtium also leaves out the seat variables when it starts `login`. `pam_systemd` then gives you a logind session of your own with no seat. That session has your runtime directory, `XDG_RUNTIME_DIR`, and your user's `user@.service`, which two logins by the same user share.

polkit counts a session as one at the machine only when the session has a seat. Your session has no seat, so polkit applies the rule it has for any process, as it does for a login over SSH. For most actions, such as powering off, that rule asks for an administrator's password. The uaccess rules give your session no devices either, so you have no sound card or removable disks unless your Unix groups give them to you.

When systemd cannot make the scope, vtium logs one line that says why. `login` then runs inside vtium's own session and gets no session of its own. polkit then judges your login by vtium's session, which is the seat's active session, so it allows you what it allows a login in direct mode.

See [the libseat issue](https://github.com/vtium/vtium/issues/34) for the work on seat mode.

`vtium console enable --seat` refuses, and changes nothing, when the console was built without libseat and logind, when `vtium-seat@.service` is missing or masked, when `/etc/pam.d/vtium` is missing, or when logind manages no `seat0`.

#### Moving a VT between modes

Turning a mode on needs root. To move one VT, name its number:

```
sudo vtium console enable --seat 3
sudo vtium console enable --direct 3
```

To move every VT that vtium has, use `--all`:

```
sudo vtium console enable --seat --all
sudo vtium console enable --direct --all
```

The command changes which unit systemd starts on the VT, and it disables the other mode's unit there. It starts and stops nothing. A vtium that runs on the VT now keeps running in the old mode, and a person logged in there keeps their session. The new mode takes over the next time vtium starts on that VT, which is at the next boot.

To switch a VT now, start the new mode's unit, as in `sudo systemctl start vtium-seat@tty3.service`. `vtium console enable` prints this step for you. After `enable` for one VT, it gives the line with that VT's number. After `enable --all`, it gives the line with `ttyN`, and you put in the number of the VT you want to switch. Starting the new mode's unit stops the vtium of the other mode on that VT, and that ends every session on the VT. So log out there first.

VT 6 is logind's reserved VT, where logind starts a console at every switch to it. While vtium runs on VT 6 in the other mode, a command that would move VT 6 refuses and changes nothing. The command refuses because the next switch to VT 6 would end that VT's sessions. It tells you how to go ahead.

#### What `vtium console enable` advises

`vtium console enable` with no mode changes nothing. It tells you what it finds on the machine. It says whether systemd runs, whether logind manages `seat0`, and whether this vtium is built with libseat and logind. It also names the display manager that keeps VT 1. Then it names the mode it recommends and why, and says whether seat mode can run on this machine. Last, it prints the command that turns the recommended mode on. When you name a VT, as in `vtium console enable 3`, the command is for that VT: `sudo vtium console enable --direct 3`. When you name no VT, or `--all`, the command is for every VT: `sudo vtium console enable --direct --all`. The advice leaves out `sudo` when you run it as root. Any user can ask for the advice.

It recommends direct mode on every systemd machine. Direct mode gives your login an active session on the seat, as a getty does. In seat mode, vtium gets its own devices from logind. That changes nothing you can see or use after you log in. What you do notice is that your login's session has no seat, so polkit asks for a password more often. So the advice names seat mode as a choice, with that cost.

### With elogind

`vtium console enable` works only on a machine booted with systemd, so on an elogind machine your init system starts vtium. Started that way, vtium has no session of its own. vtium then takes its devices from seatd if seatd runs, and opens them itself if it does not. When you log in, PAM's `pam_elogind` module registers your login with elogind on `seat0` and on vtium's VT. So your login is the seat's active session while vtium's VT is shown.

If your init system opens a PAM session for vtium with the PAM file `/etc/pam.d/vtium`, vtium holds the seat through elogind, as in seat mode. Without systemd, vtium cannot make a scope for your login. It logs one line, and `login` starts inside vtium's session without the seat variables. None of this is tested under elogind yet. See [the elogind issue](https://github.com/vtium/vtium/issues/100).

### With seatd

vtium takes the graphics card, the keyboards and the pointers, and VT switching from seatd. Your login works as in the default build with seatd: no program keeps track of sessions, and your Unix groups give you your devices.

### With no seat manager

vtium finds no seat manager when it starts, so it runs on the kernel's VT exactly as the default build does.

## Choosing a setup

| Your machine | Who opens the screen and the keyboards | Who holds the seat | What your login gets |
|---|---|---|---|
| systemd and logind, default build or direct mode | vtium; in direct mode, seatd where seatd runs | your login, while its VT is shown | the seat's active session: polkit allows the active session's actions, and uaccess gives you the seat's devices |
| systemd and logind, seat mode | logind, for the screen, the keyboards and VT switching | vtium's own session | a session with no seat: polkit asks for a password, and uaccess gives you no devices |
| elogind (untested) | vtium, or seatd where seatd runs and vtium is the libseat build | your login, while its VT is shown | the seat's active session, through elogind |
| seatd | seatd, with the libseat build; vtium, with the default build | nobody | no session: your Unix groups decide your devices |
| no seat manager | vtium | nobody | no session: your Unix groups decide your devices |

On a systemd machine, keep direct mode. Choose seat mode only if you want vtium to get its devices through logind, and you accept that your logins then lose the seat's devices. `vtium console enable` with no mode tells you what it sees on your machine. [Install VTium](../install.md#direct-mode-and-seat-mode) shows how to move a VT between modes, and [systemd.md](../systemd.md) lists every place vtium touches systemd, logind and elogind.

## What you see in each mode

This section is about a systemd machine with logind that runs the `vtium` package. It shows how you tell the two modes apart. It then follows a VT in seat mode from boot to your login, through VT switches and a keyboard you plug in. The examples come from a laptop with SDDM, a display manager, on VT 2 and vtium in seat mode on VTs 3 to 6. Its user names, process numbers and times are made generic.

### Which mode a VT runs in

Each mode has a systemd unit of its own. A VT in direct mode runs `vtium@ttyN.service`, and a VT in seat mode runs `vtium-seat@ttyN.service`, where N is the VT's number. To see which units run now, list them:

```
$ systemctl list-units 'vtium*'
  vtium-seat@tty3.service  loaded active running vtium in seat mode on tty3
  vtium-seat@tty4.service  loaded active running vtium in seat mode on tty4
  vtium-seat@tty5.service  loaded active running vtium in seat mode on tty5
  vtium-seat@tty6.service  loaded active running vtium in seat mode on tty6
```

`vtium console status` shows the units of VTs 1 to 6 in three columns. The first names the unit that systemd starts at boot. The second names the unit that logind starts when you switch to a VT where nothing runs. The third names what runs on the VT now.

```
$ vtium console status
On a VT with no link of its own, logind starts vtium in seat mode: autovt@.service links to vtium-seat@.service.
The display manager is sddm.service (/etc/systemd/system/display-manager.service links to it).
VT  at boot                                     when switched to  running
1   getty@tty1, display manager (sddm.service)  -                 -
2   -                                           vtium-seat@tty2   login screen of sddm (sddm-greeter), /usr/lib/Xorg (sddm.service)
3   vtium-seat@tty3, no getty to restore        vtium-seat@tty3   vtium-seat@tty3
4   vtium-seat@tty4, no getty to restore        vtium-seat@tty4   vtium-seat@tty4
5   vtium-seat@tty5, no getty to restore        vtium-seat@tty5   vtium-seat@tty5
6   vtium-seat@tty6, no getty to restore        vtium-seat@tty6   vtium-seat@tty6
```

To move a VT to the other mode, see [Moving a VT between modes](#moving-a-vt-between-modes).

### Your login in `loginctl`

`loginctl list-sessions` lists every logind session. vtium gives each login a pseudo-terminal (a pty), the kind of terminal that a terminal window gives a shell. So in both modes, the `TTY` column of your login names a pty such as `pts/0`, not the VT.

In direct mode, vtium has no session of its own. Your login is a session of the class `user` on `seat0`:

```
SESSION  UID USER  SEAT  LEADER CLASS   TTY   IDLE SINCE
      4 1000 alice seat0 1906   user    pts/0 no   -
```

`loginctl session-status 4` names the VT on its `Seat:` line, as in `Seat: seat0; vc3`. Its `State:` line reads `active` while VT 3 is shown, and `online` while another VT is shown.

In seat mode, the list has a session of vtium's own for each VT, and your login has no seat:

```
SESSION  UID USER  SEAT  LEADER CLASS         TTY   IDLE SINCE
      1    0 root  -     1349   manager-early -     no   -
     11 1000 alice -     2465   user          pts/0 no   -
     12 1000 alice -     2524   manager       -     no   -
      2  969 sddm  -     1473   manager-early -     no   -
     c1    0 root  seat0 1337   greeter       tty5  no   -
     c2    0 root  seat0 1335   greeter       tty3  no   -
     c3    0 root  seat0 1336   greeter       tty4  no   -
     c4    0 root  seat0 1338   greeter       tty6  no   -
     c5  969 sddm  seat0 1435   greeter       tty2  no   -
```

Each of the sessions `c1` to `c4` is a vtium. Each runs as root on `seat0` and on its own VT. Its class is `greeter`, the class of a display manager's login screen. The session `c5` is SDDM's login screen on VT 2. Session 11 is your login, of the class `user`, with no seat. A session of the class `manager` or `manager-early` is systemd's service manager for one user, and you see those in both modes.

`loginctl session-status` shows one session in detail. This is vtium's session on VT 3, shortened:

```
c2 - root (0)
  State: active
 Leader: 1335 (vtium)
   Seat: seat0; vc3
    TTY: tty3
Service: vtium
  Class: greeter
   Unit: session-c2.scope
         ├─1335 /usr/bin/vtium --vt=tty3 --no-switchvt
         └─1394 "(sd-pam)"
```

The `Service:` line names the PAM file `/etc/pam.d/vtium`, which opened the session. The `State:` line reads `active` while VT 3 is shown, and `online` while another VT is shown.

This is your login, shortened:

```
11 - alice (1000)
  State: active
 Leader: 2465 (login)
    TTY: pts/0
Service: login
  Class: user
   Unit: session-11.scope
         ├─2465 "login -- alice"
         └─2534 -zsh
```

Your session has no `Seat:` line. Its `State:` line reads `active` even while another VT is shown, because logind counts every session with no seat as active. polkit looks at the seat as well, so it still asks you for a password more often. See [Seat mode](#seat-mode) for details.

Before you log in, `login` runs in a systemd scope named for the VT and its process, such as `vtium-login-tty3-2465.scope`. Once you log in, `pam_systemd` moves your login into the scope of your own session, such as `session-11.scope`.

### When vtium gets the seat

In seat mode, systemd starts a vtium on each of its VTs at boot. Each vtium opens `seat0` through logind at once. logind gives the seat's devices only to the active session, which is the session on the VT that is shown. So each vtium whose VT is not shown waits. Its log says so:

```
seat: the seat seat0 is open
seat: waiting for the seat manager to give vtium the seat
```

To read the log of the vtium on VT 3, run `journalctl -u vtium-seat@tty3.service`.

While a vtium waits, it has not drawn anything yet. You do not see that, because its VT is not shown. When you switch to its VT, logind makes vtium's session the active one and gives vtium the seat. vtium then takes the graphics card and the keyboards, and draws its login prompt. When you switch to a VT where no vtium runs yet, logind starts one there, as the column "when switched to" of `vtium console status` shows. That vtium gets the seat as soon as it starts, because its VT is already shown.

### Switching VTs in seat mode

In seat mode, logind switches VTs, and vtium asks it to. vtium turns the kernel's keyboard handling off on its VT and reads the keys itself. When you press Ctrl+Alt and a function key, vtium asks logind, through libseat, to show that VT. The keys of the VT already shown do nothing.

When logind is about to show another VT, it asks vtium to give the seat back. vtium stops drawing, closes its keyboards and pointers, and lets go of the graphics card. vtium then tells logind that it is done, and logind shows the other VT. Another program that switches VTs, such as a display manager, goes through the same steps. Your programs keep running while another VT is shown.

When you switch back, logind gives vtium the seat again. vtium opens its keyboards and pointers again, and draws the whole screen with what your programs wrote meanwhile. Each time vtium gets the seat, its log has a line for each keyboard and pointer it opens, such as `seat: opened /dev/input/event10 through the seat manager`.

### Plugging in a keyboard or a mouse

vtium watches for keyboards and pointers that appear on `seat0`. When you plug one in while vtium's VT is shown, vtium asks logind to open the device, and you can use it at once. The log then has a line such as `seat: opened /dev/input/event10 through the seat manager`. When you plug one in while another VT is shown, vtium opens the device the next time it gets the seat.

When you unplug a device, vtium closes it. The log then has a line for each of the device's files, such as `input: cannot read /dev/input/event10: No such device (os error 19)`. These lines are expected.

### Beside a display manager

A display manager, such as SDDM, shows its login screen on a VT of its own. Its session there is also of the class `greeter`, and it is on `seat0` beside vtium's sessions. When you switch to the display manager's VT, logind gives the seat to the display manager's session, and vtium lets the seat go as for any other VT.

`vtium console enable` passes by every VT that another program holds. It leaves VT 1 to a display manager that is enabled, as [What is the same everywhere](#what-is-the-same-everywhere) describes. It also passes by a VT where a login screen, a desktop session or another console runs now. In the example, SDDM's X server holds VT 2, so `vtium console status` names that program on VT 2, and `vtium console enable --all` leaves VT 2 alone.

### What has been checked

The maintainer ran seat mode on a laptop with logind on 2026-11-13, with vtium 0.1.0-alpha.7 from the AUR. They logged in and typed, and they saw that their login got a session with no seat. They switched to another vtium VT, to SDDM's VT, and back again. They unplugged a keyboard and plugged it in again.

Tier 3, vtium's test of the installed console in a virtual machine, runs seat mode under logind, and the libseat build under seatd, both on a systemd machine. It logs in, checks that the login gets a session with no seat, and switches VTs through the seat manager. It also logs in twice on one VT, and restarts vtium while a shell is logged in. See [How vtium is judged](how-vtium-is-judged.md) for the tiers.

No test checks these cases yet:

- seatd on a machine without systemd;
- seat mode under elogind;
- a seat manager that stops while vtium runs. When libseat reports that the connection to the seat manager failed, vtium logs an error and exits. systemd then handles the exit as any failure of vtium. See [When vtium fails on a VT](../install.md#when-vtium-fails-on-a-vt) for details.

See [the libseat issue](https://github.com/vtium/vtium/issues/34) for the work on seat mode.
