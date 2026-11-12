# The VT review

The operator judges vtium by eye on a VT of a real machine (#68). Four make targets borrow one VT for this: `make vt-accept`, `make vt-login`, `make vt-check` and `make vt-shell`. Each one stops the console unit on that VT. It then runs vtium there with a single session: the acceptance pass, a login prompt, a guide, or the operator's own shell. When the session ends, the target starts the units it stopped again. The answers come back as a log. Nothing is installed, and no system file changes. The review adds to the harness tiers and replaces none of them. See [architecture.md](architecture.md#how-it-is-judged) for the tiers.

The review is written for a host that runs systemd and logind. Each of the host's VTs runs `vtium-dev@ttyN`, `getty@ttyN`, `kmscon`'s `kmsconvt@ttyN`, or the unit that `autovt@ttyN` names. The tools are in `tools/vt/`.

## This machine's VTs

The maintainer's machine runs the packaged vtium, and it has no kmscon installed. `sudo vtium console enable --all` did two things there. It pointed `/etc/systemd/system/autovt@.service` at the packaged `vtium@.service`, and it disabled the gettys on VTs 2 to 6. Each of VTs 2, 3, 4 and 6 is also enabled at boot as `vtium@ttyN.service`, which links `getty.target.wants/vtium@ttyN.service` and `autovt@ttyN.service`. So systemd starts the packaged vtium on those VTs at boot, and vtium holds each of them open. sddm then puts each new desktop session on VT 7, the first VT that no program holds open.

| VT | What runs there |
|---|---|
| 1 | sddm, the display manager, whose unit takes the VT with `Conflicts=getty@tty1.service` |
| 2, 3, 4 and 6 | the packaged vtium in direct mode, `vtium@ttyN`, which systemd starts at boot |
| 5 | the development VT: `vtium-dev@tty5`. It is a build of this checkout with the GPU path, images and `hwaccel` on, which `make vt-install` installed. See [The GPU path on the development VT](#the-gpu-path-on-the-development-vt). |
| 7 | the maintainer's Wayland session |

`vtium console status` prints this layout as it stands. For each VT, it prints what systemd starts at boot, what logind starts when the VT is switched to, and what runs there now. VT 5 became the development VT through two commands: `sudo vtium console disable 5`, and then `make vt-install VT=5`. The install gave VT 5 a link of its own, `/etc/systemd/system/autovt@tty5.service`, to `vtium-dev@.service`. It also gave VT 5 a manifest, `/var/lib/vtium-dev-install/tty5`.

Every review run borrows VT 5, because the tools refuse every other VT that a run could take. See [Which VT a run takes](#which-vt-a-run-takes) for details. After a merge lands on `integration`, the main session runs the development VT's install command from the integration worktree. [The GPU path on the development VT](#the-gpu-path-on-the-development-vt) gives that command. The command replaces the development build and restarts `vtium-dev@tty5`. So between review runs, VT 5 runs the merged code. A plain `make vt-install VT=5` would replace it with a build that has neither the GPU path nor images.

## Who starts a run

The main session starts a run when a branch reaches a visual review point. It does not ask first. It tells the operator which VT to switch to, and it reads the answers when the run ends. The operator switches to the VT and works through the run. Subagents never start a run. They verify their work in their worktrees through the tiers, and they test the tools themselves with `make test-tools` and `DRY_RUN=1`. Then they report. Only one build is on the VT at a time, and it is started from one place. When sudo needs a password, the run asks for it in the terminal that the main session started the run from. It asks before the operator leaves for the VT.

## Running it

```sh
make vt-check GUIDE=start               # the guide tools/vt/checks/start.guide
make vt-check GUIDE=start GUIDE_STEPS=2 # only that step
make vt-accept                          # the acceptance pass on the CPU path, built with pictures and bidi: every page answered, the pass recorded
make vt-accept RENDER_PATH=gpu          # the same on the GPU path, built with gpu too
make vt-accept FEATURES=                # the same with the minspec build; the picture and bidi pages record skip
make vt-accept VT=4 DEMO_ARGS='--font-size=30 --mode=1366x768'
make vt-check GUIDE=token FEATURES=gpu DEMO_ARGS='--hwaccel'   # a guide on the GPU path, with hwaccel on for this run
make vt-login                           # a login prompt, for the acceptance pass or anything else by hand
make vt-shell                           # your login shell, as you, with no login; exit it to end the run
make vt-accept DRY_RUN=1                # pick the VT and print every command; run none
make vt-shell VT_DEBUG=1                # log debug messages and copy the kernel's DRM state at each display change
make help                               # the targets
```

Each target builds the minspec binary (`make minspec`, `target/x86_64-unknown-linux-gnu/minspec/vtium`). It then picks a VT, as [below](#which-vt-a-run-takes) describes, and runs `sudo -v`. It prints which VT to switch to and which key brings you back, and it blocks until the run ends:

```
================================================================
  Switch to VT 5 now: Ctrl-Alt-F5    (Ctrl-Alt-F7 returns to this desktop)
================================================================
```

When the run ends, the target prints:

- a health check of vtium's log, which is its `panicked at` lines;
- the log's path, and with `make vt-shell`, the transcript's path too;
- the answers that the run added;
- the path of the answers file.

It exits non-zero when vtium failed, when the run was cut short, or when nothing was recorded.

### Which VT a run takes

Without `VT=N`, a run takes the development VT. That is a VT whose manifest `make vt-install` wrote under `/var/lib/vtium-dev-install`, and whose `vtium-dev@ttyN` is enabled. When there are several, the run takes the lowest. `vt.sh` prints which VT it borrows before the banner. On a machine with no development VT, the run takes the first free VT of 5, 6, 4, 3 and 2. `VT=N` names the VT in place of either choice. Both `vt.sh` and `run-vt.sh` refuse these VTs:

- the VT on screen, which is the VT that the command was typed on;
- a VT that a logind session is on, such as a login inside vtium-dev. The operator logs out there first;
- a VT that the packaged vtium holds. The packaged vtium holds a VT when `vtium@ttyN` runs or is enabled there. It also holds a VT when logind starts it on a switch to that VT, because `autovt@ttyN` names it. `vtium console enable --direct --all` sets that up on every VT without a link of its own. Such a VT is the operator's everyday console. A run would stop that console, and a start limit or a failed restore would leave the VT with no console. To free a VT that `vtium console enable --direct N` gave the packaged vtium, run `sudo vtium console disable N`, and then `sudo systemctl stop vtium@ttyN.service` if it runs. A VT that `enable --all` gave the packaged vtium stays the packaged vtium's until `sudo vtium console disable --all`;
- logind's reserved VT, which `ReserveVT=` in logind.conf(5) names, 6 by default. logind starts `autovt@ttyN` there at every switch to it, even while something holds the VT open. That console would hang the VT up under the run's vtium. The run reads the setting with `systemd-analyze cat-config systemd/logind.conf`.

The development VT is borrowed, not refused. The run stops `vtium-dev@ttyN`, and starts it again when the run ends. See [What a run does](#what-a-run-does) for details.

| Variable | What it does |
|---|---|
| `VTIUM_BIN=PATH` | the binary to run in place of the minspec build, which is then not built. The binary runs as root. So the run takes it only from make's command line or the Makefile, never from the environment. `run-vt.sh` refuses a file owned by anyone but root or the invoking user, and a file that group or others can write. |
| `VT=N` | the VT, 2 to 12, in place of the development VT or a free VT. See [Which VT a run takes](#which-vt-a-run-takes). |
| `DEMO_ARGS='...'` | vtium's options. They are split into words the way `xargs` splits them: quotes and backslashes are honoured, and nothing is expanded or run. Each option is one word: `--name=value`, or `--name` / `--no-name`. |
| `GUIDE=NAME`, `GUIDE_STEPS='2 3'` | `make vt-check`'s guide, as a name in `tools/vt/checks` or as a path, and the steps to run. The guide and the steps are checked before sudo. Without `GUIDE`, the target fails before the build, and names the guides. |
| `DRY_RUN=1` | picks the VT and prints the command. It then runs `run-vt.sh --dry-run` without sudo. That asks what a real run would ask (the VT on screen, the sessions, the active units), and prints each command that would change anything. |
| `VT_DEBUG=1` | runs vtium with `--debug`, so that its log holds its debug messages too. It also copies the kernel's DRM state into a directory beside the log, named like the log with `.drm` in place of `.log`. `run-vt.sh` copies `/sys/kernel/debug/dri/<N>/state` of every card at three kinds of moment. The first is each time vtium logs that a display came or went, as `NNN-came-<N>.state` or `NNN-went-<N>.state`. The second is each time a display's first flip after its modeset is done, as `NNN-modeset-<N>.state`; vtium logs that at debug once the card scans the new mode out. The last is once more as `end-<N>.state`, after vtium stops and before the console unit starts again. A line in vtium's log names each copy, so the log shows which state followed which line. The copies are written as the invoking user. When something that vtium started still holds vtium's output 5 s after vtium stops, the log ends there, so the run never waits on it. |

The variables reach `tools/vt/vt.sh` through the environment, never as recipe text. An allowlist names the options of vtium's command line that `DEMO_ARGS` may carry, and it matches each name whole. vtium's command line takes each configuration key that [config.md](config.md) lists, in the command-line form given there: `--name=value`, `--name value`, or `--switch` and `--no-switch`. `DEMO_ARGS` takes these options, each named in full in lower-case letters and dashes:

- display, font, palette, text and input options;
- `--render-timer`, `--image-storage-limit` (`auto` or up to 4G) and `--sync-timeout`;
- vtium's on-screen display options: `--osd`, `--osd-timeout` and `--focus-mark`;
- `--display-buffers` (`1`, `2` or `3`), which sets how many buffers each display draws into (ADR-1401 D2).

`run-vt.sh` refuses the rest:

- the VT, seat and session options, `--login`, `--configdir` and `--oneshot`;
- the grabs that act on the machine: `--key-reboot`, `--key-power-off` and `--key-suspend`. It takes `--key-paste`;
- the options that name files;
- any value in a word of its own, which vtium would take as the option's value.

To run a review by hand, use `sudo tools/vt/run-vt.sh --bin=target/x86_64-unknown-linux-gnu/minspec/vtium [--debug=DIR] [--login | --guide=FILE [--steps=LIST] | --shell [--transcript=FILE]] [N] [-- vtium-option...]`. Without root, use `tools/vt/run-vt.sh --bin=... --dry-run 5`.

## What a run does

1. `vt.sh` picks the VT, and refuses the ones listed [above](#which-vt-a-run-takes). It reads the VT on screen from `/sys/class/tty/tty0/active`, and the sessions from `loginctl`. It checks a guide with `guide.sh --check`. It creates `/tmp/vtium-vt` (mode 0700) for its log. It refuses that directory when it is a symlink, when it belongs to another user, or when it is open to anyone else. Then `vt.sh` runs `sudo -v`.
2. `sudo tools/vt/run-vt.sh` takes the lock `/run/vtium-vt.lock` for the whole run. So a second run is refused while one is going, and only one build is on a VT at a time. `run-vt.sh` checks the VT again. It refuses the VT on screen, any VT that a logind session is on (the desktop's included), the packaged vtium's VT, and logind's reserved VT. It also refuses when `loginctl` cannot list the sessions, or cannot read a session that it still lists.
3. It records which of `vtium-dev@ttyN`, `getty@ttyN`, `kmsconvt@ttyN` and `autovt@ttyN` are active, and stops them. `autovt@ttyN` is another name for the unit that logind started there. On the development VT, that unit is `vtium-dev@ttyN`, so the run records and stops it once, under its own name. A trap runs on exit, on Ctrl-C, on SIGTERM and on SIGHUP. The trap stops vtium with SIGTERM, and with SIGKILL after 5 seconds. It then starts those units again, and only those. So the VT comes back as the run found it, after a normal end, an interrupt or a crash. vtium-dev comes back when vtium-dev ran. A getty comes back when a getty ran, as it does after vtium-dev failed and its fallback started one. Nothing comes back when nothing ran. While it restores, the trap ignores a second signal and a closed terminal, and no failed write cuts it short. A failed restart exits 1, and names the command that restores the unit.
4. From the moment its units stop until it starts them again, `run-vt.sh` holds `/dev/ttyN` open. Here is why. logind starts `autovt@ttyN` on a switch to a VT only while nothing holds that VT open, which logind asks the kernel with `VT_GETSTATE`. On the development VT, that unit is `vtium-dev@ttyN`, through the VT's own link. vtium holds the VT while it runs. The hold by `run-vt.sh` also covers the moments before vtium opens the VT and after vtium ends. So a switch to the VT at any point of the run starts nothing there. The run lets go of the VT before it starts the units again. It does so because a console unit's start hangs the VT up, and may hand it back to the kernel (`TTYVHangup=`, `TTYVTDisallocate=`). vtium and its session do not inherit the hold.
5. It runs vtium with `--vt=ttyN --no-switchvt --configdir /run/vtium-vt/config --oneshot`, the options from `DEMO_ARGS`, and the session after `--login --`. The acceptance pass, the guide and the shell run as the invoking user through `setpriv`, never as root. They get a cleared environment. Only `VTIUM_HEAD` and `VTIUM_CONSOLE` come back into it, carried across as words that vtium fills in. A user whose password entry has uid 0 is refused, whatever its name. The run reads the review's own configuration, `tools/vt/config/vtium.conf`, as the invoking user, and checks it as it checks the frits. It copies the file into `/run/vtium-vt/config/`, owned by root with mode 0644, because vtium reads only a vtium.conf that root owns. The system's `/etc/vtium/vtium.conf` stays out of the run. `/run/vtium-vt/` is removed when the run ends.
6. With `--login`, the session is `/bin/login -p`, and vtium runs without `--oneshot`. The first logout ends the run. Once a session on the VT has been open, no open session there means that its user logged out. Ctrl-C also ends the run, and the login with it. vtium ending on its own is a failure. A poll whose `loginctl` fails changes nothing. Ten such polls in a row get a warning.

No tool switches a VT. The operator does, with Ctrl-Alt-FN. Before vtium starts, the run prints the command that restores the VT by hand. The command names the units that the run stopped, such as `manual restore: sudo systemctl start vtium-dev@tty5.service` on the development VT. A failed restart names the command again.

## Your shell

`make vt-shell` runs the operator's login shell as vtium's session, for anything the operator wants to do by hand. The session is the operator's own, with no acceptance pass, guide or login prompt. When the shell exits, vtium ends, the console unit comes back, and `make` returns. Ctrl-C in the terminal that `make` runs in ends the run too, as a normal end.

- The user is the one who ran `make` (sudo's `SUDO_USER`). Their entry in the password database (`getent passwd`) names the uid, gid, home directory and shell. The shell must be listed in `/etc/shells`, must be an executable file, and must have a path made of letters, digits and `/._+-`. The home directory must exist, must be a directory, and must belong to the user.
- The shell runs through `setpriv` as that user, with no inheritable capabilities. It runs as a login shell (`-l`), in the home directory. Its environment is the one that `setpriv --reset-env` gives (`HOME`, `SHELL`, `USER`, `LOGNAME`, `PATH` and `TERM`), plus `LANG` and `LC_*` from `/etc/locale.conf`. When `/run/user/UID` is a directory of the user's alone (mode 0700, not a symlink), the shell also gets `XDG_RUNTIME_DIR`. It also gets `DBUS_SESSION_BUS_ADDRESS` when `bus` in that directory is a socket. These give the shell the rights that the desktop session already gives the user, such as pipewire and `systemctl --user`. `DISPLAY` and `WAYLAND_DISPLAY` are not passed. The shell has no PAM and no logind session of its own.
- Ctrl-C and Ctrl-\ reach what the shell runs. vtium starts in the background with SIGINT and SIGQUIT ignored, and its pty starts the session with every signal at its default. The acceptance pass and the guide get the same.
- A job left running in the background when the shell exits keeps running after the run, because no logind scope reaps it.
- `script -q -f` (util-linux) records the session into a transcript. The transcript holds what the screen was sent, so a password typed without echo is not in it. vtium has no option that copies its pty's output to a file. `script` runs as the user, so the transcript is written with the user's rights. `-f` writes each chunk at once, so a crash keeps what came before it.
- vtium's log and the transcript go to `/tmp/vtium-vt` (mode 0700), each under a new name. The transcript has mode 0600. The end of the run prints both paths:

```
vtium log: /tmp/vtium-vt/vt-20261027T151200-tty5-a1B2c3.log
transcript: /tmp/vtium-vt/vt-20261027T151200-tty5-a1B2c3.transcript
```

## Guides

`make vt-check` runs `tools/vt/checks/guide.sh` as vtium's session. Each step is one screen. The screen shows the command that the step runs, what to do, and what to expect. Enter runs the step's command, or starts its probe or its shell. q stops a probe, and `exit` ends a shell. The operator then answers y, n or s, with a note. At a step's first prompt, s skips the step and q ends the checks. After the last step, vtium exits and the console comes back.

The steps' commands and shells run with the system's locale. When the session has no `LANG`, `guide.sh` takes `LANG` and `LC_*` from `/etc/locale.conf`, by the same rule that `make vt-shell` uses. So a program such as tmux or vim draws UTF-8. The open chord, Ctrl+Super+Enter, adds a console to the head, and that console runs the head's command again, which is the guide. The guide runs only in the head's first console, so one run asks each step once and keeps one set of answers. In a console whose `VTIUM_CONSOLE` is past 1, `guide.sh` prints which console it is, and runs a shell with the prompt `console-N$ `. `exit` ends that shell.

The answers, and each line that a probe prints, are appended to `/tmp/vtium-vt/vt-check.log`. They go under a header that names the guide, the time, the tty and the steps. Before it asks anything, `guide.sh` refuses that directory when it is a symlink, when it belongs to another user, or when it is open to anyone else.

A guide is a file `tools/vt/checks/NAME.guide`, made of lines of `key: text`. Blank lines and lines that start with `#` are skipped. The guide's opening comment says what it checks, and gives the command that runs it.

| Key | What it holds |
|---|---|
| `title:` | once, before the first step: the first screen's heading and the log's header |
| `intro:` | a line for the first screen, before the first step |
| `step:` | starts a step, with its title |
| `run:` | the step's command, on one line, shown under `Run:`. Enter runs it on the guide's terminal, which is vtium's VT, through `bash --noprofile --norc -c`. The command is echoed after the prompt `vtium-check$ `, as a shell would show it, and the step asks once the command ends. A status other than 0 is shown and logged. A key pressed while the command runs is neither echoed onto the picture nor kept, so it never answers the step. Ctrl-C stops the command, and Ctrl-\ stops the program it runs at that moment, as at a prompt. The checks go on, and the step asks. |
| `probe:` | the step's mode. It is either `shell`, an interactive shell with the prompt `vtium-check$ `, or a mode of `checks/probe.py` (`probe.py --list`). A `probe.py` mode sets a terminal mode, and prints each sequence the terminal sends with its time. A step has a `run:` or a `probe:`, not both. A step with neither asks once Enter is pressed. |
| `do:`, `expect:` | a line shown as typed, under `Do:` or `Expect:`. Each step needs an `expect:` line, and a `do:` or `run:` line. |

A step that judges a picture runs the command that draws it, so the picture is on screen while the operator answers. One step judges each picture. `do:` holds what only the operator can do: switch VTs, keep still, look. A step keeps the `shell` probe when typing the command is what the step checks.

In a guide's text, `{vt}` stands for the run's VT and `{desktop}` for the desktop's VT. `guide.sh --check FILE` checks a guide without a terminal, and lists its steps. These are the guides:

- `start.guide` checks that vtium starts, and that a shell runs and echoes at its prompt. #21 adds the steps for what it wires in.
- `blank.guide` checks that `setterm --blank` times the DPMS blank, and that a program's `CSI 13 ]` wakes the displays (#79).
- `window.guide` runs with `DEMO_ARGS='--allow-window-ops'`, and checks three things (#44). A program's request for 80 columns and 25 rows draws exactly that grid, at the largest font size at which the display holds it. The size report answers 25 rows and 80 columns. A zoom fills the screen again.
- `vt.guide` checks the switch away and back, and that keys reach vtium and not the kernel console (#19).
- `script.guide` shows superscripts and subscripts (SGR 73 and 74) on the VT. It uses the text and escape sequences of tier 2's four script pictures, so you can judge them live before you accept the pictures. It shows them in unifont at its own size, at twice that size and at three times it. It also shows them in Terminus 16, with Terminus 12 as its smaller size. Each step describes where the underline in each style, the strike-through and the overline lie: under, across and over the small glyphs (ADR-1202 D8). The guide switches the font with OSC 50, to the built-in unifont and to the frit `tools/vt/sections/frits/script-terminus.vtfrit`. A setup step asks you to press Ctrl+- or Ctrl+= until the character cells have the step's size. The guide's `cleanup:` line switches back to the font that the run started with. Terminus 12 stands in for the 8×8 test font of the `script-source` picture, because that font exists only in memory, in the tier 2 tool. At three times unifont's size, the text needs a display at least 1152 pixels wide and 600 high.
- `wave-a.guide` checks what Wave A changed on screen: the blink styles, X11 color names, and a program waking the blank.
- `token.guide` checks frames drawn straight into the display's buffer (ADR-1401 D2). It covers text appearing a letter at a time, scrolling, a full-screen redraw and clear, the scrollback, the cursor, a VT switch while lines arrive, the displays going dark and waking, and a display unplugged and plugged back. Each must show no tearing, no missing line and no old content.
- `daily.guide` is the daily-driver soak, run with `DEMO_ARGS='--bell'`. It covers your own login shell, an editor, a pager, top or htop, tmux, a long `cat`, 256 colors and truecolor, Unicode and wide characters, copy and paste with the mouse, the wheel, an OSC 8 link with Ctrl's overlay and Ctrl+click, OSC 133 prompt jumps, the bell, zoom, and suspend and resume. Last, it asks whether you would leave vtium running on a VT. It runs with no login. `make vt-login` checks the login instead.
- `chords.guide` runs with `DEMO_ARGS='--osd=full'`, so that the top line names the console shown. It checks the console chords under the open policy (ADR-1401 D1, D5):
  - Ctrl+Super+Enter opens a console just after the one shown.
  - Ctrl+Super+Backspace closes the one shown, and does nothing on the last.
  - A new console never takes an id that was given out before in the run.
  - A head stops at its cap of 4.
  - Ctrl shows a link's address only while it is held, across a console chord too.
- `restart.guide` checks that vtium's unit starts vtium again after a failure (ADR-1401 D9, [systemd.md](systemd.md)). Under the open policy, one SIGKILL makes systemd start the getty fallback on the VT. Under a closed policy, vtium comes back. Past systemd's start limit of 5 starts in 10 s, the VT is left with no login, until `systemctl reset-failed` and a start bring vtium back.

`restart.guide` is the one guide that never runs as vtium's session, because its steps kill vtium. The maintainer runs it by hand in a desktop terminal, from the checkout's top directory, as `GUIDE_VT=N GUIDE_DESKTOP=D tools/vt/checks/guide.sh tools/vt/checks/restart.guide`. N is a spare VT that is not on screen and has no login, and D is the desktop's VT. The guide's first step checks that N is spare, and that the terminal is not inside vtium. Its second step runs `make vt-install VT=N` ([below](#installing-vtium-on-a-vt)), and checks that the unit has `Restart=on-failure`. Its last step runs `make vt-uninstall VT=N`, which the maintainer skips to keep vtium on that VT. Every kill goes through `systemctl kill --kill-whom=main` to the unit of VT N, so it reaches that one vtium. `pkill -x vtium` would also kill a `make vt-check` run.

The closed-policy steps never change `/etc/vtium`. The first of them runs `vtium check-config` on `tools/vt/checks/restart-closed.conf`. That file holds the rules that vtium ships, under `policy = closed`, with the head not isolated (an isolated head is one that roaming keyboards never type into). Once the check passes, the step copies the file as `vtium.conf` into `/run/vtium-restart-guide`, a directory of root's. It also installs a systemd drop-in, `tools/vt/checks/restart-closed.service.conf`, as `/run/systemd/system/vtium-dev@ttyN.service.d/restart-guide.conf`. The drop-in runs the unit's vtium with `--configdir /run/vtium-restart-guide`. So the closed policy reaches `vtium-dev@ttyN` alone, and every other vtium on the machine goes on reading `/etc/vtium/vtium.conf`. The recovery step removes the drop-in and the directory. The guide's `cleanup:` line removes them too, however the checks end: after the last step, at `q`, when the terminal closes, or on a signal. A reboot clears `/run` as well. The maintainer skips those four steps to leave `vtium-dev@ttyN` on its own configuration.

The guides keep these lessons from earlier guided runs:

- A guide names the Super (Windows) key in full, because "Logo" was misread. `guide.sh --check` refuses a guide that says Logo.
- Shift+PgUp is the terminal's own scrollback key, and it never reaches a probe.
- Turning a monitor off and on is an unplug, which gives FOCUS OUT and then FOCUS IN. It is not a DPMS blank. A DPMS step needs the terminal's own blank, from `--dpms-timeout`.
- A probe's times are the times at which it read the bytes, so a probe reads while it waits.

## Installing vtium on a VT

vtium is installed on this machine in two ways, under separate names. So both can run at once, each on its own VT.

The packaged vtium comes from one of two Arch packages, whose recipes are in packaging/arch. `vtium` is the release, built from its signed tag. `vtium-git` is built from the tip of main. The two packages conflict, so a machine has one of them. This machine has `vtium`, at 0.1.0-alpha.1. Either package owns `/usr/bin/vtium`, `/usr/bin/vtium-config`, `vtium@.service` and `vtium-getty@.service`. Built with images, it also owns `/usr/bin/vtium-image` and `/usr/lib/sysusers.d/vtium-image.conf`. The maintainer installs the package with pacman, and gives it VTs with `sudo vtium console enable` (ADR-1500 D7).

The development install comes from this checkout through `make vt-install`. It owns `/usr/local/bin/vtium-dev`, `/usr/local/bin/vtium-dev-config`, `vtium-dev@.service` and `vtium-dev-getty@.service`. For a console with images, it also owns `/usr/local/bin/vtium-dev-image` and `/etc/sysusers.d/vtium-dev-image.conf`.

Neither install touches the other's files. The packaged vtium lags development on purpose. It tests how vtium is deployed and updated, and the development install tests the work in progress.

`make vt-install VT=N` makes a development build of vtium the console of VT N of this machine, in place of its getty. The build stays there across reboots, until `make vt-uninstall VT=N`. That VT becomes the development VT, which a review run borrows for one session. The run stops vtium-dev there, and starts it again when the run ends. See [Which VT a run takes](#which-vt-a-run-takes) for details. Run both targets as yourself, never as root or under sudo. Each refuses root before it builds anything. Each asks sudo for each step that needs root, so sudo asks for the password in the terminal they run in.

```sh
make vt-install VT=5 DRY_RUN=1   # print the units it would write and every command; run none
make vt-install VT=5             # install the development build on VT 5, then Ctrl-Alt-F5 shows it
make vt-install VT=5 FEATURES=images   # a console with pictures, with its image helper
make vt-install VT=5             # again: the binaries are replaced and vtium-dev on VT 5 restarts; this machine's VT 5 takes the GPU build below
make vt-uninstall VT=5 DRY_RUN=1 # print what uninstall would do
make vt-uninstall VT=5           # VT 5 back as it was
```

vtium starts its glyph helper, `vtium-config`, at a path fixed when the binary is built. The path comes from `VTIUM_FONT_HELPER`, read in `crates/vtium/src/helper.rs`, and it is `/usr/bin/vtium-config` when that variable is unset. So `make vt-install` builds its own console with `make vt-dev-console`. That console is the minspec build, made to start the helper at `/usr/local/bin/vtium-dev-config`. It is built in the target directory `target/vt-dev`, so that it never replaces the minspec build that the review runs use. `make vt-install` builds the helper with `make config-release`. `FEATURES='...'` builds the console with those features. [The GPU path on the development VT](#the-gpu-path-on-the-development-vt) describes a build with `gpu`.

vtium starts its image helper, `vtium-image`, the same way, at a path fixed when the binary is built. The path comes from `VTIUM_IMAGE`, read in `crates/vtium/src/head/images.rs` and `crates/vtium-platform/src/image_helper/start.rs`, and it is `/usr/bin/vtium-image` when that variable is unset. Only a console built with images starts the image helper. With `images` in `FEATURES`, `make vt-dev-console` builds the console to start it at `/usr/local/bin/vtium-dev-image`, and checks that the binary names that path. `make vt-install` then builds the image helper with `make image-release`.

`VTIUM_BIN=PATH`, `VTIUM_CONFIG_BIN=PATH` and `VTIUM_IMAGE_BIN=PATH` on make's command line install another console, glyph helper and image helper as they are. `make vt-install` builds none of them. It builds its own image helper only with `images` in `FEATURES`. So a console with images that is named with `VTIUM_BIN` needs one of two things beside it: `FEATURES=images`, or the image helper named with `VTIUM_IMAGE_BIN`. Without either, install copies whatever `target/x86_64-unknown-linux-gnu/release/vtium-image` an earlier build left there, and it refuses when there is none.

`tools/vt/vt-install.sh` does the work. Install refuses these VTs:

- VT 1, and a VT outside 2 to 12;
- the VT on screen, and a VT that a logind session is on, by the check that `run-vt.sh` uses;
- a VT where `kmscon`'s `kmsconvt@ttyN` runs;
- a VT where the packaged `vtium@ttyN` runs or is enabled.

Install also refuses a console that does not name `/usr/local/bin/vtium-dev-config`, because that console would start the packaged helper, or no helper at all. It refuses a console or helper that someone other than root or the user could have written.

Install reads from the console itself whether the console has images. Every build names the image helper path that it was built with, through the default of the `image-helper` key. `make vt-dev-console` names `/usr/local/bin/vtium-dev-image` exactly when it builds with images. So a console that names that path has images, and gets the image helper. Install refuses a console built with images that names another path, because it would start the packaged helper, another build's helper, or none. Install tells such a build by a line that only a build with images carries: the message vtium logs when it cannot open its image helper. The tool tests check that this line is still in code built only with images.

One run of install or uninstall goes at a time. Each run takes an flock (a lock on an open file) on `/var/lib/vtium-dev-install` before it reads what is installed, and it holds the lock until it ends. A second run refuses while the first holds it. A first install finds no such directory. It makes the directory, and takes the lock before it writes anything into it. It refuses when another run has written there since it started. Install then:

1. Generates two units from the units that vtium ships in `crates/vtium/install/` (the shipped units), with `tools/vt/units.sh`. That is the one script that makes the units for both installs, so the units have one source.

   `vtium-dev@.service` is the shipped `vtium@.service` with these changes:

   - its name;
   - an opening comment of its own;
   - `ExecStart=` runs `/usr/local/bin/vtium-dev` in place of `/usr/bin/vtium`;
   - `OnFailure=` names `vtium-dev-getty@.service`;
   - `Conflicts=` and `After=` lines name `vtium@%i.service` and `kmsconvt@%i.service`. So starting it on a VT first stops the packaged vtium or `kmscon`'s console there, as it stops the getty;
   - `DefaultInstance=name-a-VT/vtium-dev@ttyN`, which names no VT. So `systemctl enable vtium-dev@.service` with no VT named fails, and links nothing.

   `vtium-dev-getty@.service` is the shipped `vtium-getty@.service` under its new name. Every other line is the shipped unit's. So `Restart=` and its kin come through as the shipped unit has them, and so does `Alias=autovt@.service`.

   logind starts `autovt@ttyN` on a VT that nothing holds. That happens when the VT is switched to, at boot before vtium starts, in the moment before a restart, or after a failure. The template link `/etc/systemd/system/autovt@.service` names the unit that `autovt@ttyN` starts on every VT that has no link of its own. On this machine, that unit is the packaged `vtium@.service`, which `vtium console enable --direct --all` linked. On another machine, it is a getty or `kmscon`'s `kmsconvt@.service`. Enabling `vtium-dev@ttyN` makes `/etc/systemd/system/autovt@ttyN.service`, a link to `vtium-dev@.service` for that VT alone. So logind starts vtium-dev there, and never the console that the template names. vt-install leaves the template link alone. It never uses the names `kmsconvt@` and `kmsconvt-getty@`, which belong to `kmscon`'s package. Install fails before it changes anything when the shipped unit no longer has the lines that it changes.
2. Refuses a file that is there already and is not the file that vt-install wrote. This covers `/usr/local/bin/vtium-dev`, `/usr/local/bin/vtium-dev-config`, either unit file, and, for a console with images, `/usr/local/bin/vtium-dev-image` and `/etc/sysusers.d/vtium-dev-image.conf`. vt-install knows its own files by the checksums in its record, `/var/lib/vtium-dev-install/files`. A file that vt-install never wrote is someone else's. A file that changed since is someone's edit. vt-install never replaces either. It refuses `/etc/systemd/system/autovt@ttyN.service` when that file is there already, unless an earlier install on this VT made it.
3. Writes the manifest `/var/lib/vtium-dev-install/ttyN` first, so that a run cut short can still be undone. The manifest records whether `getty@ttyN` was enabled and running before. A second install on the same VT keeps the first record.
4. Copies these files, all owned by root:

   - the helper, to `/usr/local/bin/vtium-dev-config`;
   - for a console with images, the image helper, to `/usr/local/bin/vtium-dev-image`, and its sysusers file, to `/etc/sysusers.d/vtium-dev-image.conf`;
   - the binary, to `/usr/local/bin/vtium-dev`;
   - the units, to `/etc/systemd/system`.

   Install keeps the checksum of each file in `/var/lib/vtium-dev-install/files`. Every VT that holds vtium-dev shares that record, as it shares the files.

   Install writes each file, the manifest and the record included, to a temporary name in the same directory, `.NAME.vt-install-new`. It then renames the file into place with `mv -fT`. It runs `sync` on each temporary file before the rename, so a power cut just after the rename cannot leave the path empty. It runs `sync` on `/var/lib/vtium-dev-install` after the manifest's rename, so the manifest is on the disk before anything in `/usr/local` or `/etc` changes. A rename replaces the file whole. So each path holds either the old file or the whole new one, never part of a file, even when a copy fails halfway on a full disk. A copy that fails removes its part-written temporary file. Only vt-install uses those temporary names, and systemd passes over a file whose name starts with a dot when it reads a unit directory.

   The record is written before the copies, and it holds each file's checksum from before and after. So a copy that fails leaves every file in a state that the record knows. `make vt-uninstall VT=N` then removes the run, and a second `make vt-install VT=N` finishes it. Once the copies are done, the record holds only the new checksums.

   For a console with images, install then runs `systemd-sysusers /etc/sysusers.d/vtium-dev-image.conf`. This creates the `vtium-image` account, which the image helper runs as at the login prompt (ADR-1101 D2). Without the account, the helper runs as nobody. The sysusers file is the shipped `crates/vtium/install/vtium-image.conf`, under an opening comment of its own. Its name is the development install's own, so it never masks the file that a package installs as `/usr/lib/sysusers.d/vtium-image.conf`.

   Install then runs `systemctl daemon-reload`, enables `vtium-dev@ttyN` and restarts it, which starts it the first time. Then it disables `getty@ttyN`. vtium starts before the getty is touched. vtium's `Conflicts=getty@%i.service` stops the getty, and its `After=getty@%i.service` makes that stop finish before vtium starts. `Conflicts=` alone orders nothing. Without the ordering, the getty's hangup of the VT when it stops would race vtium's start. The VT always has one of the two.

   When any of these steps fails, install says what to do next:

   - when it failed before the manifest was written: nothing;
   - when it failed after that: `make vt-uninstall VT=N`, to undo the run;
   - on a second install over an earlier one: `make vt-uninstall VT=N` removes the whole install on VT N, the earlier one too, while `make vt-install VT=N` tries again.

An install made before the tool was renamed vtium-config (ADR-1201 D12) left its helper at `/usr/local/bin/vtium-dev-vitrify`. A later install removes that file once the new helper is in place. Uninstall removes it with the other files. Each removes the file only while it is still the one that the record names. When such a file has changed since, vt-install keeps it and names it. vt-install leaves alone a file that its record does not name.

Every VT that holds vtium-dev runs the one console. So an install of a console without images removes the image helper and the sysusers file that an earlier install wrote, once the new console is in place. It removes each only while it is still the file that the record names. When such a file has changed since, vt-install keeps it and names it. It leaves the `vtium-image` account.

A second install replaces the binaries that every VT holding vtium-dev shares. But it restarts only its own VT's `vtium-dev@ttyN`. vtium-dev on another installed VT keeps running the build it started with, from memory. It does so until that VT's vtium-dev restarts: at a failure, at a reboot, or at its own `make vt-install VT=N`.

Uninstall reads the manifest and undoes each step:

- It stops and disables `vtium-dev@ttyN`.
- It removes the link `autovt@ttyN.service`, which `disable` leaves, while the link still points at `vtium-dev@.service`.
- It clears a failed state of `vtium-dev@ttyN` or of its fallback.
- Where the manifest says that `getty@ttyN` was enabled, it adds `getty@ttyN` back to `getty.target`'s wants with `systemctl add-wants`. This makes no alias `autovt@ttyN` of the getty's own, as `systemctl enable` would.
- It starts `getty@ttyN` again where it was running.
- It removes the manifest.

Uninstall also removes any file left at one of vt-install's temporary names, which a run killed in the middle of a copy leaves behind. It removes the binaries, the image helper and its sysusers file, the two units and the record only when no other VT's manifest is left. So vtium-dev installed on two VTs keeps its files until both are uninstalled. Even then, uninstall removes a file only while its checksum still matches the record, and it names each file that it keeps because the file changed. It then removes `/var/lib/vtium-dev-install` once nothing is left in it.

Uninstall leaves the `vtium-image` account, because `systemd-sysusers` never removes an account. `sudo userdel vtium-image` removes the account when no package's sysusers file names it.

A first install that was cut short before its manifest was in place leaves files at the temporary names, and an empty `/var/lib/vtium-dev-install`. On a VT that has no manifest, uninstall removes those, and refuses when it finds neither. Uninstall refuses a VT with a logind session on it, such as a login in the installed vtium. Log out there first.

A review run borrows the development VT, and never takes a VT that the packaged vtium holds. See [Which VT a run takes](#which-vt-a-run-takes) for details. A review run starts the glyph helper built with the vtium it runs. It never starts the packaged helper or the development install's helper. `make vt-check` and the other review targets build `make config-release` with the console. `run-vt.sh --helper=PATH` copies that helper into `/run/vtium-vt/`, owned by root and with mode 0755. Only root can write to that directory. The run then runs vtium with `--font-helper` naming the copy.

vtium names itself `TERM=xterm-vtium`. So the run also compiles the checkout's `crates/vtium/install/xterm-vtium.ti` into `/run/vtium-vt/terminfo`. The demo, the guide and the shell get `TERMINFO` naming that directory, so they read the entry that the package ships. The run does not pass that variable to `/bin/login`'s session. So `make vt-login` uses the system's `xterm-vtium` entry when the package has installed one. When none is installed, it runs vtium with `--term=kmscon`, and says so. The run removes `/run/vtium-vt/` when it ends.

Every run, a guide's included, stages the system's frits and those in `tools/vt/sections/frits/`, and names their directory with `--font-frits`. [The acceptance pass](#the-acceptance-pass) describes this. So a guide can switch to one of those frits with OSC 50.

The installed vtium runs with the system's configuration, not the review's. It reads `/etc/vtium/vtium.conf` and the frits (font set files) in `/etc/vtium/fonts`, and so does the packaged vtium. Neither target writes, moves or removes any of these files. So a setting there applies to both installed vtiums.

If uninstall cannot run, put VT N back by hand. These commands undo what install did on VT N alone:

```sh
sudo systemctl disable --now vtium-dev@ttyN.service
sudo rm /etc/systemd/system/autovt@ttyN.service           # the alias enable made, which disable leaves
sudo systemctl add-wants getty.target getty@ttyN.service   # only where getty@ttyN was enabled before
sudo systemctl start getty@ttyN.service
sudo rm /var/lib/vtium-dev-install/ttyN                    # VT N's manifest, and no other VT's
sudo systemctl daemon-reload
```

The binaries, the image helper's sysusers file and the units are shared by every VT that holds vtium-dev. When `ls /var/lib/vtium-dev-install` shows no other `tty` manifest, remove them too. `sudo sha256sum -c /var/lib/vtium-dev-install/files` says `OK` for each file that is still the one install wrote. Remove those files with `sudo rm`. Keep any file that fails the check, because someone changed it. Then remove the record with `sudo rm -r /var/lib/vtium-dev-install`.

### The GPU path on the development VT

vtium draws on the GPU when the console is built with the `gpu` feature and the `hwaccel` key is on. See [config.md](config.md) for the key. `hwaccel` is off unless the build turns it on. This machine's development VT runs a build with the GPU path, images and `hwaccel` on. The main session installs it with this command, after each merge too:

```sh
VTIUM_HWACCEL=on make vt-install VT=5 FEATURES='gpu images'
```

`FEATURES='gpu images'` builds the console with the GPU path and with images, and it builds and installs the image helper beside it. `VTIUM_HWACCEL=on` in make's environment reaches cargo. cargo then builds both the console and the glyph helper, `vtium-config`, with `hwaccel` on by default. So `vtium-dev config get hwaccel`, run where the draft sets no `hwaccel`, prints `hwaccel`, `on` and `default`. `FEATURES=gpu` alone builds a console without images. A build without `VTIUM_HWACCEL=on` leaves `hwaccel` off, so that console draws on the CPU. `HWACCEL=on` on make's command line does not reach this build, because `make install` reads that variable and `make vt-install` does not.

A line `hwaccel` in `/etc/vtium/vtium.conf` would turn the GPU path on for the development VT too. But the packaged vtium on the other VTs reads the same file, as [above](#installing-vtium-on-a-vt) says. So the development VT takes `hwaccel` from its build, and the packaged vtium keeps its own default.

A review run builds its own console from `FEATURES`, and takes vtium's options from `DEMO_ARGS`. To run a guide on the GPU path, name the feature, and turn `hwaccel` on for that run alone with vtium's `--hwaccel` option:

```sh
make vt-check GUIDE=token FEATURES=gpu DEMO_ARGS='--hwaccel'
```

### What the package ships

`make install DESTDIR=DIR PREFIX=/usr PKGNAME=NAME` stages under DIR exactly what the `vtium` package ships. `make install-terminfo DESTDIR=DIR PREFIX=/usr` stages what the `vtium-terminfo` package ships. So a package's build can be one call each (ADR-1500 D1, D3). The AUR recipes in packaging/arch call these targets, which main has. Both targets need DESTDIR named, not empty and not `/`. So neither installs onto this machine, where a `vtium@.service` in `/usr/local/lib/systemd/system` would take the place of the packaged one. `make install` builds with the host's toolchain, as a distribution's package build does, in the target directory `target/install`. `CARGO_BUILD_FLAGS='--frozen'` passes a flag to each cargo call. The targets stage these files:

| File | Target | What it is |
|---|---|---|
| `PREFIX/bin/vtium` | install | the console, built with the minspec profile and `FEATURES`, which starts its helper at `PREFIX/bin/vtium-config` |
| `PREFIX/bin/vtium-config` | install | the font and settings tool, which `vtium config` runs and which is also the glyph helper |
| `PREFIX/lib/systemd/system/vtium@.service`, `vtium-getty@.service` | install | the VT unit and its getty fallback, made by `tools/vt/units.sh`. `ExecStart=` runs `PREFIX/bin/vtium`, and `Conflicts=` and `After=` name `kmsconvt@%i.service`. `DefaultInstance=name-a-VT/vtium@ttyN` names no VT, so `systemctl enable vtium@.service` with no VT named fails, and links nothing on any machine (below). |
| `/etc/vtium/vtium.conf.example` | install | the example configuration. It stays in `/etc` whatever `PREFIX` is, because vtium reads its configuration from `/etc/vtium` alone. |
| `PREFIX/share/doc/vtium/examples/` | install | the example udev rules and hwdb fragment, which are documentation and never active (ADR-1401 D4) |
| `PREFIX/share/licenses/PKGNAME/LICENSE`, `COPYING` | both | the license and `COPYING`, under the package's name: `vtium` by default for `install`, and `vtium-terminfo` for `install-terminfo`. `COPYING` names vtium's authors, and holds the notices for the third-party code and data that vtium builds in. |
| `PREFIX/share/licenses/PKGNAME/OFL-1.1.txt`, `rgb-license.txt` | install | the two license texts that `COPYING` points to: the SIL Open Font License of the Unifont font, and the license of X.Org's color names |
| `PREFIX/share/terminfo/x/xterm-vtium` | install-terminfo | vtium's terminfo entry, compiled with `tic -x` from `crates/vtium/install/xterm-vtium.ti`. The target stops when that file is missing. |

The unit's `DefaultInstance=` names no VT because of what `systemctl enable` does with a template and no VT. This was measured with systemd 262, and systemd 254's `src/shared/install.c` does the same. `systemctl enable` links each `Alias=` before it checks `WantedBy=`. So with no `DefaultInstance=`, it first links `/etc/systemd/system/autovt@.service` to `vtium@.service`, on a machine where that link does not exist yet. Only then does it refuse the template. logind would then start vtium on every VT it spawns. With the shipped `DefaultInstance=tty1`, it would enable VT 1. With a value that names no instance, `systemctl` stops before it links anything. It ignores `DefaultInstance=` when it enables an instance. So `systemctl enable vtium@tty3.service` links `getty.target.wants/vtium@tty3.service` and `autovt@tty3.service` as before. The cost is that `systemctl list-unit-files` shows `vtium@.service` as `bad`, and `systemctl is-enabled vtium@.service` fails. Each `vtium@ttyN` reads as enabled or disabled as usual.

A machine that runs no kmscon console on purpose gives every VT to the packaged vtium with `sudo vtium console enable --direct --all` (ADR-1500 D7). That command points `autovt@.service` at `vtium@.service`. `make vt-install` never does this for the development install. `make vt-uninstall` names a template link `autovt@.service` that points at `vtium-dev@.service`, which no command of vtium's makes.

Neither target ships a file that `kmscon`'s packages own. So neither ships `/etc/kmscon/kmscon.conf.example`, the `kmscon` unit files, or a terminfo entry named `kmscon`, which a distribution's `kmscon-terminfo` package holds. `make install-tree`, which tier 3 installs in its guest, stages the same `vtium@` and `vtium-getty@` units and the same `/etc/vtium/vtium.conf.example` as the package. It differs in two ways. It links against the pinned Debian sysroot. And beside `xterm-vtium`, which the `vtium-terminfo` package ships, it installs the terminfo entry named `kmscon`.

## The acceptance pass

The acceptance pass (ADR-1501) is one pass through every page in `tools/vt/sections/`, answered on the VT with single keys.

`make vt-accept` builds the console with images and bidirectional text (`images bidi`), unless `FEATURES` is named on make's command line. For a binary named with `VTIUM_BIN`, which it runs as it is, it takes the features from that binary. `RENDER_PATH` chooses the render path that the pass judges. `cpu`, the default, runs vtium with `--no-hwaccel`. `gpu` runs it with `--hwaccel`, and adds `gpu` to the default features. So each batch gets a pass on each path. `make vt-accept` refuses `RENDER_PATH=gpu` when `FEATURES` is named without `gpu`. It also refuses `--hwaccel` or `--no-hwaccel` in `DEMO_ARGS`, because the render path sets that option. With images, `make vt-accept` gives vtium the image helper built with the console, `vtium-image`. `run-vt.sh` copies the image helper into `/run/vtium-vt/` beside the glyph helper, and names it with `--image-helper`. So the pages whose images the helper decodes show them.

`tools/vt/accept.sh` shows a title page that names the build and lists the pages. Then it shows each page in file name order, and then a summary. Below each page, a footer gives the page's guidance: its `# look-for:` and `# wrong-looks-like:` lines. One key answers the page: y looks right, n looks wrong, u unsure, s skip. c types a comment first, r draws the page again, b goes back a page, and q quits. A paste never answers a page. accept.sh asks the terminal for bracketed paste, and drops a paste while it waits for a key. In a comment, the paste is comment text. A page that the build cannot show says why, and is recorded as skip. An images page under a build without images is one example. Before each page, accept.sh puts back what the page before it changed: the pen, reverse video, the cursor's shape, the palette, the scroll region, and kitty images.

Each answer rewrites the pass's record. The record is a markdown file named after the pass, in `~/.local/state/vtium/acceptance/` (`$XDG_STATE_HOME/vtium/acceptance`, or `ACCEPT_DIR`). It holds:

- the build's label, VT, binary, features and render path;
- the hardware the pass ran on: the machine's model, the kernel, each GPU or display controller with its driver, each connected display's connector, preferred mode and EDID make and model, and the text area that vtium reports;
- the fonts, the terminal, and its DA1 answer;
- each page's answer and comment.

The record says that it is evidence about that hardware only. accept.sh refuses to record into a directory that is a symlink or that belongs to another user, as the guide's log and the run log do. The directory may have any absolute path, including a home directory with a space in it. `run-vt.sh` refuses only a relative path, a path with a control character, and a path over 4000 characters.

Three pages blink text: `10-sgr`, `35-blink` and `36-rapid-blink`. They name the blink style that vtium draws the text in. The style is read from the config that the run gives vtium and from its blink options (`tools/vt/blink.inc`). When neither sets one, the style is vtium's default, read from the checkout's key table. The record and the comparison name the style too.

The next run of the same build on the same render path offers to resume a pass that was quit part way, at its first unanswered page. When a review run ends, `vt.sh` prints the pass that the run wrote.

`tools/vt/feedback.sh` prints a pass, the newest by default. With `--list`, it lists the passes. With `--compare A B`, it compares two passes, named by id or by a record's path. The comparison prints each page whose answer changed. It warns first when the two passes ran on different hardware, and it names a render path that differs. It exits 1 when a page went from yes to no.

For each batch that goes from `stage` to main, the pass of each render path on that batch's build is committed. `tools/vt/feedback.sh --save PASS` copies it to `docs/acceptance/<YYYY-MM-DD>-<git describe>-<render path>.md`, and `--dry-run` prints that name and writes nothing.

The acceptance pass runs in any terminal. There, `ACCEPT_LABEL='foot 1.21' tools/vt/accept.sh` records a pass under that label. `ACCEPT_NOPAGE=1 tools/vt/accept.sh sgr` draws a page and its guidance without waiting for keys.

The thirteen language pages each show one sentence in every style. Each is walked twice, once in each of the acceptance pass's font sets in `tools/vt/sections/frits/`: an outline set and a bitmap set. accept.sh switches the console to each set with OSC 50. vtium takes OSC 50 only for a frit in its frits directory. So `run-vt.sh` copies the system's frits and the acceptance pass's frits into `/run/vtium-vt/frits`, and runs vtium with `--font-frits` naming that directory.

The copy runs as root, and everyone may read it. So `run-vt.sh` refuses a frit that is a symlink or that has a second hard link. It also refuses a frit that belongs to someone other than root or the user, or that group or others may write. Any of those could name a file that the user may not read. `run-vt.sh` reads vtium, the glyph helper, the image helper, each frit and the terminfo source once, as the user, into a directory of root's alone. So the user's own permissions decide what is read. It checks the owner, mode and type of each file as it was read. It installs vtium and the others, and runs `tic`, from that copy. So a file swapped in at the path after the read never reaches `/run/vtium-vt/`, and vtium runs from `/run/vtium-vt/vtium`. `run-vt.sh` also refuses a system frit with the file name of one of the acceptance pass's own frits, because OSC 50 names a frit by its file name.

The pass record names each font file with its SHA-256 and its package. The comparison lists the font files that differ between two passes. The superscript page, `33-script`, is walked in a third font set, `script-terminus`, whose smaller Terminus draws its superscripts and subscripts. The reflow page, `79-reflow`, switches the console itself to `acceptance-large`, which is Terminus at 16×32, after it writes its lines. So the grid loses half its columns while the page shows. accept.sh switches the console back before the next page.

A page is one file, `tools/vt/sections/NN-name.sh`. Its output is deterministic. It is at most 17 rows of 80 columns, so that it fits 80×25 with its header and footer. Both guidance fields are written in terms that a person can check by eye. ADR-1501 lists the pages and what each shows. A change that adds something visible adds its page.

## Testing the tools

`make test-tools`, part of CI, runs `tools/vt/tests/test_vt.py`, `tools/vt/tests/test_accept.py` and shellcheck. `test_accept.py` drives the acceptance pass through stand-in pages on a pseudo-terminal. It checks the pass record, the resume and the comparison. It checks each real page's guidance and size, and what the image pages send to a terminal with and without images.

The tests run every path without a VT, sudo or logind. `VTIUM_VT_STANDINS` names a directory of stand-ins, which the tools run from there by full path. There are stand-ins for `systemctl`, `loginctl`, `setpriv`, `getent`, `systemd-analyze`, `sudo`, the VT's `/dev/ttyN`, `/etc/shells`, `/etc/locale.conf`, `/run/user` and the VT on screen. A stand-in vtium exits, crashes, waits for a signal, or runs its session. The tests check that:

- a VT that a session is on is refused;
- the shell is the one that the password database names and `/etc/shells` lists;
- a home directory that is not the user's, and a session as uid 0, are refused;
- the shell gets the locale, and gets the user's runtime directory and bus only when they are the user's;
- the console unit is restored after a crash, Ctrl-C, SIGTERM and SIGHUP;
- a guide that fails `--check` stops before sudo;
- a `run:` step shows its command, runs it on the guide's terminal, and drops a key pressed meanwhile;
- a console after the head's first runs a shell in place of the guide;
- `vt-install.sh` generates the units from the shipped ones, with only their names, `ExecStart=`'s path and the fallback's name changed;
- `vt-install.sh` refuses VT 1, the VT on screen, a VT with a session, a VT that `kmscon`'s console or the packaged vtium holds, a console that would start another helper, and a file it did not write;
- `vt-install.sh` records and restores the getty's state, keeps files that another VT still holds, and runs nothing in a dry run;
- the review runs take the development VT, stop and start again only the units they found running there, and hold the VT open from the stop to the restart;
- the review runs pass over a VT that the packaged vtium holds, and logind's reserved VT;
- `make install` stages the package's files and none that `kmscon`'s packages own, with the console starting its helper where the package puts it;
- `make install` stages the frits that `FRITS` names and, with images, the gallery tool, and it refuses a frit that vtium does not hold;
- `make install` hands `HWACCEL` and `DISPLAY_BUFFERS` to both builds, and refuses a value out of range;
- a guide's steps get `LANG` and `LC_*` from a locale file only when no `LANG` is set, and only as locale names. `GUIDE_LOCALE_CONF` stands in for `/etc/locale.conf`;
- the answers log refuses a symlinked or foreign directory;
- no tool reaches `chvt` or `fgconsole`.

The tests also run `start.guide`, the demo and a stand-in shell end to end through `make`.

As root, `make vt-install` under `VTIUM_VT_STANDINS` needs the binary and the helper named with `VTIUM_BIN=PATH` and `VTIUM_CONFIG_BIN=PATH`, so that no build runs as root. `vt-install.sh` refuses the stand-ins' directory, and its `sudo`, `systemctl` and `loginctl` stand-ins, unless each belongs to root or to the user who ran sudo. It also refuses any of them that group or others can write. As root, `run-vt.sh` refuses `VTIUM_VT_STANDINS`. `vt.sh` honours it and `VTIUM_VT_DIR` only together. So a stale export cannot choose what runs as root, or where the answers are read.
