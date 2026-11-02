# The VT review

The operator judges vtium by eye on a spare VT of a real machine (#68). `make vt-demo`, `make vt-login`, `make vt-check` and `make vt-shell` stop the console unit on a free VT, run vtium there with a demo, a login prompt, a guide or the operator's own shell as its only session, and start the console unit again when the session ends. The answers come back as a log. Nothing is installed and no system file changes. The review adds to the harness tiers ([architecture.md](architecture.md#how-it-is-judged)) and replaces none of them.

It is written for a host whose VTs run `kmsconvt@ttyN`, `getty@ttyN` or `autovt@ttyN`, under systemd and logind. The tools are in `tools/vt/` and carry over the C reference's `make vt-demo`, `vt-login` and `vt-check` and the lessons of their first runs.

## Who starts a run

The main session starts a run on a free VT when a branch reaches a visual review point, without asking first, and reads the answers when the run ends. The operator switches to the VT and works through it. Subagents never start a run: they verify in their worktrees through the tiers, with `make test-tools` and `DRY_RUN=1` for the tools themselves, and report. One build is on the VT at a time, from one place. When sudo needs a password, the run asks for it in the terminal the main session started it from, before the operator leaves for the VT.

## Running it

```sh
make vt-check GUIDE=start               # the guide tools/vt/checks/start.guide
make vt-check GUIDE=start GUIDE_STEPS=2 # only that step
make vt-demo                            # the demo's pages, each rated
make vt-demo VT=4 DEMO_ARGS='--font-size=30 --mode=1366x768'
make vt-login                           # a login prompt, for the demo or anything else by hand
make vt-shell                           # your login shell, as you, with no login; exit it to end the run
make vt-demo DRY_RUN=1                  # pick the VT and print every command; run none
make vt-shell VT_DEBUG=1                # log debug messages and copy the kernel's DRM state at each display change
make help                               # the targets
```

Each target builds the minspec binary (`make minspec`, `target/x86_64-unknown-linux-gnu/minspec/vtium`), picks the first free VT of 5, 6, 4, 3 and 2 (or `VT=N`), runs `sudo -v`, prints the VT to switch to and the key back, and blocks until the run ends:

```
================================================================
  Switch to VT 5 now: Ctrl-Alt-F5    (Ctrl-Alt-F7 returns to this desktop)
================================================================
```

When it ends, it prints a health check of vtium's log (its `panicked at` lines), the log's path (with `make vt-shell`, the transcript's too), the answers the run added and the path of the answers file. It exits non-zero when vtium failed, the run was cut short, or nothing was recorded.

| Variable | What it does |
|---|---|
| `VTIUM_BIN=PATH` | the binary to run in place of the minspec build, which is then not built. It runs as root, so it is taken from make's command line or the Makefile, never from the environment, and `run-vt.sh` refuses a file owned by anyone but root or the invoking user, or writable by group or others. |
| `VT=N` | the VT, 2 to 12. A VT is free when it is not on screen and no logind session is on it. |
| `DEMO_ARGS='...'` | vtium's options, split into words as `xargs` splits them: quotes and backslashes honoured, nothing expanded or run. Each option is one word, `--name=value` or `--name` / `--no-name`. |
| `GUIDE=NAME`, `GUIDE_STEPS='2 3'` | `make vt-check`'s guide, a name in `tools/vt/checks` or a path, and the steps to run. The guide and the steps are checked before sudo; without `GUIDE` the target fails before the build, naming the guides. |
| `DRY_RUN=1` | picks the VT and prints the command, then runs `run-vt.sh --dry-run` without sudo: it asks what it would ask (the VT on screen, the sessions, the active units) and prints each command that would change anything. |
| `VT_DEBUG=1` | runs vtium with `--debug`, so its log holds its debug messages too, and copies the kernel's DRM state into a directory beside the log, named like the log with `.drm` in place of `.log`. Each time vtium logs that a display came or went, `run-vt.sh` copies `/sys/kernel/debug/dri/<N>/state` of every card as `NNN-came-<N>.state` or `NNN-went-<N>.state`; each time a display's first flip after its modeset is done, which vtium logs at debug once the card scans the new mode out, as `NNN-modeset-<N>.state`; and once more as `end-<N>.state` after vtium stops and before the console unit starts again. A line in vtium's log names each copy, so the log shows which state followed which line. The copies are written as the invoking user. When something vtium started still holds its output 5 s after vtium stops, the log ends there, so the run never waits on it. |

The variables reach `tools/vt/vt.sh` through the environment, never as recipe text. The allowlist is written for the command line #21 gives vtium, the C reference's; vtium parses no arguments yet. `DEMO_ARGS` takes the options the C reference's review took: display, font, palette, text and input options, `--render-timer`, `--image-storage-limit` (`auto` or up to 4G) and `--sync-timeout`, vtium's on-screen display options, `--osd`, `--osd-timeout` and `--focus-mark`, and `--display-buffers` (`1`, `2` or `3`, how many buffers each display draws into, ADR-1401 D2), each named in full in lower-case letters and dashes. `run-vt.sh` refuses the rest: the VT, seat and session options, `--login`, `--configdir`, `--oneshot`, the grabs, the options that name files, and any value in a word of its own, which vtium would take as the option's value or move into the login command that runs as root.

By hand: `sudo tools/vt/run-vt.sh --bin=target/x86_64-unknown-linux-gnu/minspec/vtium [--debug=DIR] [--login | --guide=FILE [--steps=LIST] | --shell [--transcript=FILE]] [N] [-- vtium-option...]`, and `tools/vt/run-vt.sh --bin=... --dry-run 5` without root.

## What a run does

1. `vt.sh` picks the VT and refuses one on screen (`/sys/class/tty/tty0/active`) or with a logind session on it (`loginctl`), checks a guide with `guide.sh --check`, creates `/tmp/vtium-vt` (mode 0700) for its log, refusing it when it is a symlink, another user's or open to anyone else, and runs `sudo -v`.
2. `sudo tools/vt/run-vt.sh` takes the lock `/run/vtium-vt.lock` for the whole run, so a second run is refused while one is going: one build on a VT at a time. It checks the VT again: it refuses the VT on screen and any VT a logind session is on, the desktop's included, and refuses when `loginctl` cannot list the sessions or read one still listed.
3. It records which of `kmsconvt@ttyN`, `getty@ttyN` and `autovt@ttyN` are active and stops them. A trap on exit, Ctrl-C, SIGTERM and SIGHUP stops vtium with SIGTERM (SIGKILL after 5 seconds) and starts those units again, so the console comes back after a normal end, an interrupt or a crash. While it restores it ignores a second signal and a closed terminal, and no failed write cuts it short. A failed restart exits 1 and names the command that restores the unit.
4. It runs vtium with `--vt=ttyN --no-switchvt --configdir tools/vt/config --oneshot`, the options from `DEMO_ARGS`, and the session after `--login --`. The demo, the guide and the shell run as the invoking user through `setpriv`, with a cleared environment that gets back only `VTIUM_HEAD` and `VTIUM_CONSOLE`, carried across as words vtium fills in, never as root: a user whose password entry has uid 0 is refused, whatever its name. `tools/vt/config` keeps the system's config out of the run.
5. With `--login` the session is `/bin/login -p`, and vtium runs without `--oneshot`. The first logout ends the run: once a session on the VT has been open, none open there means its user logged out. Ctrl-C also ends it, the login with it. vtium ending on its own is a failure. A poll whose `loginctl` fails changes nothing; ten in a row get a warning.

No tool switches a VT: the operator does, with Ctrl-Alt-FN. If the console does not come back, the run prints the command that restores it: `sudo systemctl start kmsconvt@ttyN.service` (or the getty the VT ran).

## Your shell

`make vt-shell` runs the operator's login shell as vtium's session, for anything by hand: the session is the operator's own, with no demo, guide or login prompt. Exiting the shell ends vtium, the console unit comes back and `make` returns. Ctrl-C in the terminal `make` runs in ends it too, as a normal end.

- The user is the one who ran `make` (sudo's `SUDO_USER`). Their entry in the password database (`getent passwd`) names the uid, gid, home directory and shell. The shell must be listed in `/etc/shells`, be an executable file, and have a path of letters, digits and `/._+-`. The home directory must exist, be a directory and be the user's.
- It runs through `setpriv` as that user, with no inheritable capabilities, as a login shell (`-l`) in the home directory. Its environment is the one `setpriv --reset-env` gives (`HOME`, `SHELL`, `USER`, `LOGNAME`, `PATH` and `TERM`), plus `LANG` and `LC_*` from `/etc/locale.conf`. When `/run/user/UID` is a directory of the user's alone (mode 0700, not a symlink), it also gets `XDG_RUNTIME_DIR`, and `DBUS_SESSION_BUS_ADDRESS` when `bus` there is a socket: the rights the desktop session already gives the user, such as pipewire and `systemctl --user`. `DISPLAY` and `WAYLAND_DISPLAY` are not passed. There is no PAM and no logind session of its own.
- Ctrl-C and Ctrl-\ reach what the shell runs: vtium starts in the background, with SIGINT and SIGQUIT ignored, and its pty starts the session with every signal at its default. The demo and the guide get the same.
- A job left running in the background when the shell exits keeps running after the run: no logind scope reaps it.
- `script -q -f` (util-linux) records the session into a transcript: what the screen was sent, so a password typed without echo is not in it. vtium has no option that copies its pty's output to a file, and `script` runs as the user, so the transcript is written with the user's rights. `-f` writes each chunk at once, so a crash keeps what came before it.
- vtium's log and the transcript go to `/tmp/vtium-vt` (mode 0700), each under a new name, the transcript with mode 0600, and the end of the run prints both paths:

```
vtium log: /tmp/vtium-vt/vt-20261027T151200-tty5-a1B2c3.log
transcript: /tmp/vtium-vt/vt-20261027T151200-tty5-a1B2c3.transcript
```

## Guides

`make vt-check` runs `tools/vt/checks/guide.sh` as vtium's session. Each step is one screen: the command it runs, what to do and what to expect. Enter runs the step's command, or starts its probe or shell; q stops a probe and `exit` ends a shell, and the operator answers y, n or s, with a note. At a step's first prompt, s skips the step and q ends the checks. After the last step vtium exits and the console comes back.

The steps' commands and shells run with the system's locale: when the session has no `LANG`, `guide.sh` takes `LANG` and `LC_*` from `/etc/locale.conf`, by the rule `make vt-shell` uses, so a program such as tmux or vim draws UTF-8. A console that the open chord, Ctrl+Super+Enter, adds to the head runs the head's command again, which is the guide. The guide runs only in the head's first console, so one run asks each step once and keeps one set of answers: in a console whose `VTIUM_CONSOLE` is past 1, `guide.sh` prints which console it is and runs a shell with the prompt `console-N$ `, which `exit` ends.

The answers, and each line a probe prints, are appended to `/tmp/vtium-vt/vt-check.log` under a header naming the guide, the time, the tty and the steps. `guide.sh` refuses that directory before anything is asked when it is a symlink, another user's or open to anyone else.

A guide is `tools/vt/checks/NAME.guide`, lines of `key: text`; blank lines and lines starting with `#` are skipped. Its opening comment says what it checks and the command that runs it.

| Key | What it holds |
|---|---|
| `title:` | once, before the first step: the first screen's heading and the log's header |
| `intro:` | a line for the first screen, before the first step |
| `step:` | starts a step, with its title |
| `run:` | the step's command, one line, shown under `Run:`. Enter runs it on the guide's terminal, vtium's VT, through `bash --noprofile --norc -c`, echoed after the prompt `vtium-check$ ` as a shell would show it, and the step asks once it ends; a status other than 0 is shown and logged. A key pressed while it runs is neither echoed onto the picture nor kept, so it never answers the step. Ctrl-C stops the command, and Ctrl-\ the program it runs at that moment, as at a prompt; the checks go on, and the step asks. |
| `probe:` | the step's mode: `shell`, an interactive shell with the prompt `vtium-check$ `, or a mode of `checks/probe.py` (`probe.py --list`), which sets a terminal mode and prints each sequence the terminal sends, timed. A step has a `run:` or a `probe:`, not both, and one without either asks once Enter is pressed. |
| `do:`, `expect:` | a line shown as typed under `Do:` or `Expect:`; each step needs an `expect:` line, and a `do:` or `run:` line |

A step that judges a picture runs the command that draws it, so the picture is on screen while the operator answers, and one step judges each picture. `do:` holds what only the operator can do: switch VTs, keep still, look. A step keeps the `shell` probe when typing the command is what it checks.

`{vt}` and `{desktop}` in the text are the run's VT and the desktop's. `guide.sh --check FILE` checks a guide without a terminal and lists its steps. `start.guide` checks that vtium starts and that a shell runs and echoes at its prompt; #21 adds the steps for what it wires in. `blank.guide` checks that `setterm --blank` times the DPMS blank and that a program's `CSI 13 ]` wakes the displays (#79). `vt.guide` checks the switch away and back, and that keys reach vtium and not the kernel console (#19). `wave-a.guide` checks what Wave A changed on screen: the blink styles, X11 color names and a program waking the blank. `token.guide` checks frames drawn straight into the display's buffer (ADR-1401 D2): text appearing a letter at a time, scrolling, a full-screen redraw and clear, the scrollback, the cursor, a VT switch while lines arrive, the displays going dark and waking, and a display unplugged and plugged back, each with no tearing, missing line or old content. `daily.guide` is the daily-driver soak, run with `DEMO_ARGS='--bell'`: your own login shell, an editor, a pager, top or htop, tmux, a long `cat`, 256 colors and truecolor, Unicode and wide characters, copy and paste with the mouse, the wheel, an OSC 8 link with Ctrl's overlay and Ctrl+click, OSC 133 prompt jumps, the bell, zoom, suspend and resume, and last whether you would leave vtium running on a VT. It runs with no login, which `make vt-login` checks. `chords.guide`, run with `DEMO_ARGS='--osd=full'` so the top line names the console shown, checks the console chords under the open policy (ADR-1401 D1, D5): Ctrl+Super+Enter opens a console just after the one shown, Ctrl+Super+Backspace closes the one shown and does nothing on the last, a new console never takes an id given before in the run, a head stops at its cap of 4, and Ctrl shows a link's address only while it is held, across a console chord too. `restart.guide` checks that vtium's unit starts vtium again after a failure (ADR-1401 D9, [systemd.md](systemd.md)): under the open policy one SIGKILL hands the VT to the getty fallback, and under a closed policy vtium comes back, and past systemd's start limit of 5 starts in 10 s the VT is left with no login, until `systemctl reset-failed` and a start bring vtium back.

`restart.guide` is the one guide that never runs as vtium's session, since its steps kill vtium. The maintainer runs it by hand in a desktop terminal, from the checkout's top directory, as `GUIDE_VT=N GUIDE_DESKTOP=D tools/vt/checks/guide.sh tools/vt/checks/restart.guide`, where N is a spare VT that is not on screen and has no login, and D is the desktop's VT. Its first step checks that N is spare and that the terminal is not inside vtium. Its second runs `make vt-install VT=N` ([below](#installing-vtium-on-a-vt)) and checks that the unit has `Restart=on-failure`, and its last runs `make vt-uninstall VT=N`, which the maintainer skips to keep vtium on that VT. Every kill goes through `systemctl kill --kill-whom=main` to the unit of VT N, so it reaches that one vtium, where `pkill -x vtium` would also kill a `make vt-check` run. The closed-policy steps install `tools/vt/checks/restart-closed.conf`, the shipped rules under `policy = closed` with the head not isolated, as `/etc/vtium/heads.conf`, after `vtium check-config` passes it and only when no head file is installed; the recovery step removes it only when it is that file. The maintainer skips those four steps to leave `/etc/vtium` untouched.

The C reference's first guided run left lessons the guides keep:

- A guide names the Super (Windows) key in full: "Logo" was misread. `guide.sh --check` refuses a guide that says Logo.
- Shift+PgUp is the terminal's own scrollback key and never reaches a probe.
- Turning a monitor off and on is an unplug, which gives FOCUS OUT and then FOCUS IN, not a DPMS blank. A DPMS step needs the terminal's own blank, from `--dpms-timeout`.
- A probe's times are the times it read the bytes, so a probe reads while it waits.

## Installing vtium on a VT

`make vt-install VT=N` makes vtium the console of VT N of this machine, in place of its getty, and it stays there across reboots until `make vt-uninstall VT=N`. A run of `make vt-check` or `make vt-shell` borrows a VT for one session and gives it back; an installed vtium is the one the maintainer uses every day. Both targets need sudo, which asks for the password in the terminal they run in.

```sh
make vt-install VT=3 DRY_RUN=1   # print the units it would write and every command; run none
make vt-install VT=3             # install the minspec build on VT 3, then Ctrl-Alt-F3 shows it
make vt-install VT=3             # again, after a new build: the binary is replaced and vtium restarts
make vt-uninstall VT=3 DRY_RUN=1 # print what uninstall would do
make vt-uninstall VT=3           # VT 3 back as it was
```

`tools/vt/vt-install.sh` does the work. Install refuses VT 1, a VT outside 2 to 12, the VT on screen and a VT a logind session is on, by the check `run-vt.sh` uses, and a VT where the C reference's `kmsconvt@ttyN` runs. It then:

1. Generates two units from the shipped ones in `crates/vtium/install/`, so the units have one source. `vtium@.service` is `kmsconvt@.service` with three changes: its name, `ExecStart=` running `/usr/local/bin/vtium` in place of `/usr/bin/vtium`, and `OnFailure=` naming `vtium-getty@.service`. `vtium-getty@.service` is `kmsconvt-getty@.service` under its new name. Every other line is the shipped unit's, so `Restart=` and its kin come through as the shipped unit has them, and so does `Alias=autovt@.service`. On this machine `/etc/systemd/system/autovt@.service` links to the C reference's `kmsconvt@.service`, and logind starts `autovt@ttyN` on a VT that nothing holds, at boot before vtium starts, in the moment before a restart, or after a failure. Enabling `vtium@ttyN` makes `/etc/systemd/system/autovt@ttyN.service`, a link to `vtium@.service` for that VT alone, so logind starts vtium there and never the C reference's console beside it. The template link `autovt@.service` and the names `kmsconvt@` and `kmsconvt-getty@` stay the C reference's package's. Install fails before it changes anything when the shipped unit no longer has the lines it changes.
2. Refuses when `/usr/local/bin/vtium` or either unit file is there already and no manifest says vt-install wrote it, so it never replaces someone else's file.
3. Writes the manifest `/var/lib/vtium-vt-install/ttyN` first, so that a run cut short can still be undone. The manifest records whether `getty@ttyN` was enabled and running before. A second install on the same VT keeps the first record.
4. Copies the binary to `/usr/local/bin/vtium` and the units to `/etc/systemd/system`, all owned by root, then runs `systemctl daemon-reload`, enables `vtium@ttyN` and restarts it, which starts it the first time, and disables `getty@ttyN`. vtium starts before the getty is touched: its `Conflicts=getty@%i.service` stops the getty as vtium starts, so the VT always has one of them. When any of these steps fails, install says that `make vt-uninstall VT=N` undoes what the run did.

Uninstall reads the manifest and undoes each step: it stops and disables `vtium@ttyN`, removes the link `autovt@ttyN.service` that `disable` leaves while it still points at `vtium@.service`, clears a failed state of it or its fallback, adds `getty@ttyN` back to `getty.target`'s wants with `systemctl add-wants` where the manifest says it was enabled, which makes no alias `autovt@ttyN` of the getty's own as `systemctl enable` would, starts it again where it was running, and removes the manifest. It removes the binary, the two units and `/var/lib/vtium-vt-install` only when no other VT's manifest is left, so vtium installed on two VTs keeps its files until both are uninstalled. It refuses a VT that has no manifest, or a logind session on it, such as a login in the installed vtium; log out there first.

The installed vtium runs with the system's configuration, not the review's: it reads `/etc/kmscon/kmscon.conf` and `/etc/vtium/heads.conf`. On this machine the C reference's package may own `/etc/kmscon/kmscon.conf`. Neither target writes, moves or removes either file, so a setting there applies to the installed vtium too.

If uninstall cannot run, VT N goes back by hand:

```sh
sudo systemctl disable --now vtium@ttyN.service
sudo rm /etc/systemd/system/autovt@ttyN.service           # the alias enable made, which disable leaves
sudo systemctl add-wants getty.target getty@ttyN.service   # only where getty@ttyN was enabled before
sudo systemctl start getty@ttyN.service
sudo rm /usr/local/bin/vtium /etc/systemd/system/vtium@.service /etc/systemd/system/vtium-getty@.service
sudo rm -r /var/lib/vtium-vt-install
sudo systemctl daemon-reload
```

## The demo

`tools/vt/demo.sh` shows a title page, then one page per section in `tools/vt/sections/`, in file name order, then a summary. Below each page a footer gives the section's guidance, its `# look-for:` and `# wrong-looks-like:` lines, and one key rates it: 1 right, 2 wrong, 3 unsure, s skip for now, c a comment first, r redraw, b back, q quit. Each answer appends a line to `/tmp/vtium-vt/feedback.tsv` (time, run, label, term, size, section, rating, note), and `tools/vt/feedback.sh` prints the latest run, a run by id (`--run ID`) or the latest run of each label side by side (`--compare`). The demo runs in any terminal: `DEMO_LABEL='name' tools/vt/demo.sh` in another terminal rates the same pages under its own label, for comparison. `DEMO_NOPAGE=1 tools/vt/demo.sh sgr` draws a page and its guidance without waiting for keys.

A section is one file, `tools/vt/sections/NN-name.sh`: deterministic output, at most 17 rows of 80 columns so it fits 80x25 with its header and footer, and both guidance fields, each about 130 characters, in terms a person can check by eye. A change that adds something visible adds its section.

## Testing the tools

`make test-tools`, part of CI, runs `tools/vt/tests/test_vt.py` and shellcheck. The tests run every path without a VT, sudo or logind: `VTIUM_VT_STANDINS` names a directory of stand-ins for `systemctl`, `loginctl`, `setpriv`, `getent`, `sudo`, `/etc/shells`, `/etc/locale.conf`, `/run/user` and the VT on screen, run from there by full path, and a stand-in vtium that exits, crashes, waits for a signal or runs its session. They check that a VT a session is on is refused, that the shell is the one the password database names and `/etc/shells` lists, that a home directory not the user's and a session as uid 0 are refused, that the shell gets the locale and the user's runtime directory and bus only when they are the user's, that the console unit is restored after a crash, Ctrl-C, SIGTERM and SIGHUP, that a guide failing `--check` stops before sudo, that a `run:` step shows its command, runs it on the guide's terminal and drops a key pressed meanwhile, that a console after the head's first runs a shell in place of the guide, that `vt-install.sh` generates the units from the shipped ones with only their names, `ExecStart=`'s path and the alias changed, refuses VT 1, the VT on screen, a VT with a session, a VT the C reference's console runs on and a file it did not write, records and restores the getty's state, keeps files another VT still holds, and runs nothing in a dry run, that a guide's steps get `LANG` and `LC_*` from a locale file (`GUIDE_LOCALE_CONF` stands in for `/etc/locale.conf`) only when no `LANG` is set and only as locale names, that the answers log refuses a symlinked or foreign directory, that no tool reaches `chvt` or `fgconsole`, and they run `start.guide`, the demo and a stand-in shell end to end through `make`. As root, `run-vt.sh` refuses `VTIUM_VT_STANDINS`, and `vt.sh` honours it and `VTIUM_VT_DIR` only together, so a stale export cannot choose what runs as root or where the answers are read.

## Departures from the C reference's review

- vtium is one binary with no modules, so `run-vt.sh` runs it directly; the C reference bind-mounted its staged modules in a private mount namespace.
- The binary is a variable, checked for its owner and mode before it runs as root; the C reference ran the package it staged itself.
- A dry run asks the real read-only questions (the VT on screen, the sessions, the active units); the C reference's assumed them.
- The demo refuses to record into a symlinked or foreign directory, as the guide's log and the run log do.
- The health check counts Rust panics; the C reference's counted its own freeze signature.
- Guides gain the `shell` step and the `run:` line. Review kits wait until a section needs one: the demo has one kit, every page.
