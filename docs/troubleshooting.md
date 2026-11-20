# Find out what went wrong

This guide helps when vtium misbehaves on a VT, or does not do what you expected. It first gets you off that VT. Then it shows where to find the reason: in what `vtium console status` says each VT runs, and in vtium's log.

In the commands below, `#` marks a command that you run as root, for example with sudo. `$` marks a command that you run as your own user.

## Getting out

When vtium misbehaves on a VT, these steps get you out, in this order:

1. Switch to another VT, or to your desktop's, with Ctrl+Alt and a function key from F1 to F12. On its own VT, vtium reads the keyboard itself. The kernel switches VTs only when vtium asks it to. So when vtium has stopped answering keys, those keys cannot take you off its VT. Then you take the next steps from another machine over ssh instead.

2. Restart that VT's vtium, from another VT, a desktop terminal, or another machine over ssh. A restart ends every session on the VT. On VT 3:

    ```
    # systemctl restart vtium@tty3.service
    ```

    The development install's unit is `vtium-dev@tty3.service`. See [The development VT](vt-review.md#installing-vtium-on-a-vt) for details.

3. Take vtium off the VT at once. Take the VT back, and start its getty there:

    ```
    # vtium console disable 3
    # systemctl start getty@tty3.service
    ```

    `disable` decides what starts on VT 3 from now on. See [Giving vtium a VT](install.md#giving-vtium-a-vt) for details. Starting the getty stops vtium on VT 3, because vtium's unit conflicts with the getty's. That ends every session there.

    On the development VT, `vtium console disable` refuses, because the development install holds the VT. There, the `systemctl start getty@tty5.service` line alone stops `vtium-dev`. `make vt-uninstall VT=5` takes the development install off for good. See [The development VT](vt-review.md#installing-vtium-on-a-vt) for details.

When vtium fails, systemd starts it again. From systemd 254 on, each failure also starts vtium's fallback. The fallback starts the VT's getty, unless vtium.conf's policy is `closed`. So under an open policy, you get a plain login prompt at the first failure instead of a dead screen. The policy is `open` or `closed`, and a vtium.conf without one is open. Under the closed policy, vtium ignores any keyboard that no rule names. See [A VT that shows a getty](#a-vt-that-shows-a-getty) for details.

## Notices on the screen

vtium tells you about a problem on the screen itself, in a notice. A notice is a pale red box across the top of the screen. Most notices appear over the session, and the session keeps running under them. Such a notice goes after 30 seconds, or sooner when you press a key that reaches that screen. A screen shows at most 4 notices at a time, and a fifth one replaces the oldest. Under `osd = silent`, vtium shows only two of them, the panic screen and `All screens stopped`, and writes every notice to its log. See [The on-screen display and notices](concepts/osd-and-notices.md#what-silent-changes) for details.

Some notices stay instead:

- `Session ended. It will not start again.` stays for as long as that console is shown.
- When vtium has no session to show on a display, it can show a notice alone there, on an otherwise black screen. Such a notice stays until the problem is gone, or for as long as vtium runs.

Find the first words of the notice on your screen in this table, and read the section it links:

| The notice starts with | What it means |
|---|---|
| `vtium.conf not used: running the built-in configuration.` | [vtium cannot use vtium.conf at all](concepts/start-up.md#vtiumconf-not-used-running-the-built-in-configuration) |
| `vtium.conf has errors: running the parts without errors.` | [vtium.conf has errors, and vtium runs what is left](concepts/start-up.md#vtiumconf-has-errors-running-the-parts-without-errors) |
| `vtium.conf has errors and counts as closed: all input is off.` | [The panic screen: nothing of a closed vtium.conf can run](concepts/start-up.md#vtiumconf-has-errors-and-counts-as-closed-all-input-is-off) |
| `No keyboard found. Connect one to type.` | [vtium found no keyboard at start](concepts/start-up.md#no-keyboard-found-connect-one-to-type) |
| `Every keyboard now types into NAME` | [The anti-lockout guard moved every keyboard](concepts/flows.md#every-keyboard-now-types-into-name) |
| `Keyboard DEVICE now types into NAME, the active screen` | [The rescue key moved a keyboard](concepts/flows.md#keyboard-device-now-types-into-name-the-active-screen) |
| `Keyboard DEVICE now types into the active screen` | [The rescue key moved a keyboard, but no head is active](concepts/flows.md#keyboard-device-now-types-into-the-active-screen) |
| `Session ended. It will not start again.` | [A session ended under `oneshot`](concepts/heads.md#session-ended-it-will-not-start-again) |
| `Console not opened: login could not start.` | [A new console's session could not start](concepts/heads.md#console-not-opened-login-could-not-start) |
| `All screens stopped: a session kept ending as soon as it started.` | [A login kept ending, under the closed policy](concepts/heads.md#all-screens-stopped-a-session-kept-ending-as-soon-as-it-started) |
| `CONNECTOR is not configured for any terminal.` or `CONNECTOR is not configured for VT N.` | [No head on this VT uses that display](head-file.md#terms) |

## Reading vtium console status

`vtium console status` first says two things about the machine. It says which console logind starts on a VT with no link of its own. It also says which display manager the machine has. Then it prints a line for each VT, with these columns:

- **at boot** names what systemd starts on the VT at boot. Examples are `getty@tty1`, `vtium@tty1`, `display manager (sddm.service)`, or another unit that runs on the VT, such as `cage@tty3`. `getty recorded` means that `vtium console` disabled the VT's getty, and keeps a record of it, which `disable` puts back. `no getty recorded` means that `vtium console` recorded that no getty was enabled on the VT. So `disable` enables none there. `vtium@ttyN masked` means that someone masked vtium's unit there.
- **when switched to** names what logind starts when you switch to the VT while nothing runs there. It shows `-` on VT 1 and on VTs 7 and above, where logind starts no console. See [Giving vtium a VT](install.md#giving-vtium-a-vt) for details.
- **running** names what runs on the VT now, a graphical session included.

A VT whose **at boot** column names a getty keeps that getty when you switch to it, whatever the **when switched to** column says. The getty already runs there.

Under the table, `status` gives a line for each VT that a unit starting at boot holds. The line gives the reason, such as `VT 1: the display manager sddm.service holds VT 1 at boot: a display manager is enabled, and by common convention VT 1 stays the display manager's and its getty's ...`. [VTs that other programs hold](install.md#vts-that-other-programs-hold) lists the reasons.

When `autovt@.service` links to vtium, `status` can also print a line under the table that starts `vtium does not start at boot on`. It names each VT from 2 to 5 where logind would start vtium when you switch to it. On such a VT, nothing starts at boot, and no other program holds the VT. Nothing holds such a VT open until you switch to it. So a display manager that puts a new desktop session on the first free VT can take it.

This happens on a machine where vtium took the VTs in a release that did not yet start vtium at boot on each VT it takes. The line ends with the command that fixes it, such as `sudo vtium console enable --direct --all`. It names `--seat` instead when `autovt@.service` links to seat mode's unit. That command starts vtium at boot on those VTs. VT 6 is never named, because logind holds VT 6 open itself. See [What the install does to your VTs](install.md#what-the-install-does-to-your-vts) for details.

## A font that does not load

A frit (a font set file) names its fonts in order. When vtium cannot use one of them, it renders that font's characters from the next font, and in the end from its built-in font. When it cannot render the frit at all, it renders its built-in font instead. The most likely reason is a font that is installed but not active. vtium renders only the fonts that `/etc/vtium/batch.conf` lists, and only when each file passes the trust rule. See [Active resources and the trust rule](concepts/resources.md) for details.

Check these in turn:

1. Run `vtium check-config` as your own user. For each font that a frit needs and vtium treats as missing, it names the frit, the font and the file, and says why. A line such as `Font set jetbrains-mono needs the font JetBrains Mono, and its file /usr/share/fonts/TTF/JetBrainsMono-Regular.ttf is not active` means that the font is installed but not on the list. A file that fails the trust rule comes with the check it failed, such as a folder that other users can write.
2. Run `vtium config active` to see the list. Without a `batch.conf`, vtium uses the list of a fresh install, which holds only the fonts vtium ships and the console font that `/etc/vconsole.conf` names.
3. Activate the font's file:

    ```
    $ vtium config activate /usr/share/fonts/TTF/JetBrainsMono-Regular.ttf --install
    ```

    A font in your home folder cannot be activated, because root does not own it. Install it as a system font first, as [Choosing what vtium may use](install.md#choosing-what-vtium-may-use) shows.
4. Log out of the VT and log in again. vtium reads the list again before the next login, so the font renders from then on, with no restart of vtium.

If the font still does not render, read the log of that VT's vtium:

```
# journalctl -b -u vtium@tty2.service
```

A line such as `the font monoid is not drawn (...)`, or `the font's glyphs are not generated (...)`, gives the reason. Check these too:

- vtium finds a frit by its file's name. For example, `font-name=monoid` reads `/etc/vtium/fonts/monoid.vtfrit`. vtium does not find a file that is named otherwise, such as `Monoid.vtfrit`. It then looks for a kbd font named `monoid` instead.
- A frit names its fonts as a list of sources. When the font helper cannot find a source, it skips that source without a warning, and the next source renders in its place. When everything renders in the emoji font or the built-in font, the font helper did not find your font. It looks for an outline font's family in `/usr/share/fonts` and `/usr/local/share/fonts`, or in the folders that `font-paths` names, and takes only the active files there. See [An outline font](fonts.md#an-outline-font) for details.
- The font file's own name table must give the family that the frit names. Either its typographic family name or its legacy family name counts. The file's own name does not matter. `fc-scan --format '%{family}\n' FILE` prints the families that a file gives. A `font:` line takes any of them, in any language. It ignores case, spaces, `-` and `_`.
- The font helper takes a file named for the family first, such as `Monoid-Regular.ttf`, whatever its style. Otherwise it takes the first file whose subfamily is `Regular`. Failing that, it takes the first file of the family. "First" follows the order of the font directories, and then the path. A search opens at most 4096 font files. Past that, the search fails with a line that names the family and the limit.
- vtium refuses the whole frit when the frit has an unknown key or a malformed line. The log names the file and the line.
- vtium uses a change to the frit or to `vtium.conf` only from its next start on that VT. See [Configuration](configure.md) for details. A change to the list of active fonts takes effect sooner, at the next login.

`vtium check-config` also reports a frit whose `cell=` line vtium cannot read.

## A color scheme that does not apply

When a head does not show the color scheme that its look names, vtium could not use that color scheme. The head then shows the color scheme that the top of vtium.conf names, and in the end `legacy`. Check these in turn:

- Run `vtium check-config`. It names each color scheme that vtium.conf names and cannot use, and says why. The name may match no built-in color scheme and no file `NAME.palette` in `/etc/vtium/palettes` or `/usr/share/vtium/palettes`. The file may have an error, which the check names with its line. A file may also take the name of a built-in color scheme, and vtium never reads such a file.
- A color scheme of your own must be active. The color schemes that vtium installs are active from the first start, but a file you add to `/etc/vtium/palettes` is not. When the check says `The color scheme NAME is not active`, activate its file and log in again:

    ```
    $ vtium config activate /etc/vtium/palettes/NAME.palette --install
    ```

- The file must also pass the trust rule. The check names the rule that the file fails, such as a symbolic link on its path.
- vtium reads the `palette` key and the palette files when it starts. A change to either takes effect when vtium next starts on the VT. See [How an install reaches the VTs](configure.md#how-an-install-reaches-the-vts) for details.
- A program can change the colors in its own console, with OSC 4 and OSC 10 to 19. RIS, the full reset, returns them to the color scheme's. See [Programs that change the colors](palettes.md#programs-that-change-the-colors) for details.
- An effect changes every color after the color scheme applies. Under the vtlook `amber`, for example, every color scheme shows in shades of amber.

## An effect that does not apply

When a head shows no effect where its look names a vtlook (a file of frame filters), the head draws the built-in vtlook, which changes no pixel. Check these in turn:

- The build must have frame filters. `vtium --version` names `filters` on its `features` line when it does. The `vtium` and `vtium-git` packages build them from 0.1.0-alpha.10 on, and a package of an earlier release has none. See [Arch Linux](install.md#arch-linux) for the features each package builds.
- Run `vtium check-config`. It lists each error of every vtlook that vtium.conf names, with the file and the line. It also names a color map that a vtlook's `cube` filter cannot use, because the file is missing, is not active, or fails the trust rule. Any such error makes the whole vtlook fail.
- A color map, a `.cube` file, must be active. A fresh install activates none. Activate the file and log in again:

    ```
    $ vtium config activate /etc/vtium/looks/warm.cube --install
    ```

- A vtlook's `[requires]` section can rule out a display, such as one with fewer than 24 bits a pixel. That display draws the built-in vtlook, and the head's other displays keep the effect. vtium logs one line that names the display and the first requirement it misses, such as `vtlook amber: display DP-1 (1280x800, 1 bit) draws the built-in vtlook: requires min-depth 24`.
- A copy of a preset in `/etc/vtium/looks` replaces the preset. When the copy has an error, the head draws the built-in vtlook, not the preset.
- vtium reads vtium.conf and its vtlooks when it starts. A change to the `vtlook` key or to a vtlook's file takes effect when vtium next starts on the VT. Activating a color map takes effect sooner, at the next login.

[Vtlook files](looks.md#errors) shows each error that `vtium check-config` prints.

## A VT that shows a getty

The unit's fallback, `vtium-getty@ttyN.service`, starts a getty on a VT where vtium fails. You are then not left at a dead screen. The fallback starts no getty when the getty's start would stop another program. An example is a display manager whose unit conflicts with `getty@tty1`. Then the VT stays dark, and `journalctl -b -u vtium-getty@ttyN.service` names the program and says why.

A login prompt where you expected vtium therefore usually means that vtium failed. For example, vtium exits with a failure when no display appears within `display-wait`, which is 10 seconds by default. Find the reason in its unit's state and log:

```
$ systemctl status vtium@tty2.service
# journalctl -b -u vtium@tty2.service
```

A getty that ran on the VT before you gave it to vtium also keeps the VT until it stops. Log out there and start vtium, as in [Giving vtium a VT](install.md#giving-vtium-a-vt).

## A VT that shows nothing

- vtium turns its displays off after `dpms-timeout` passes with no key press and no pointer action. The default is 600 seconds. Press a key.
- Under a vtium.conf whose policy is `closed`, the fallback starts no getty, because a getty's login would take any keyboard. vtium also waits for a display however long it takes. Run `vtium check-config` and read the log.
- A VT whose link still names `vtium@.service` after the package was removed gets no console at all. The [vtium-arch README](https://github.com/vtium/vtium-arch#removing-vtium) gives the commands that clean such links up.

## A VT that stays black after you switch back to it

Sometimes you switch from the desktop back to a VT where vtium runs, and the screen stays black. A key press does not bring it back. This is a known defect, which [issue #249](https://github.com/vtium/vtium/issues/249) follows. Until it is fixed, restart that VT's vtium, as [Getting out](#getting-out) shows. The restart ends every session on the VT. If the screen stayed black until you rebooted, collect a report of the previous boot with `vtium-config report --previous-boot --save`. See [Collecting a report and filing an issue](#collecting-a-report-and-filing-an-issue) for details.

## Images that do not show

A program that draws sixel or kitty graphics images shows nothing, or shows text where the image should be. Check these in turn:

- vtium renders images only in a build with the `images` feature. In such a build, `vtium --version` names `images` on its `features` line. So does the vtium part of `vtium-config report`, which [Collecting a report and filing an issue](#collecting-a-report-and-filing-an-issue) describes. The `vtium` and `vtium-git` packages are built with this feature, and the minspec packages are not. See [Arch Linux](install.md#arch-linux) for details.
- `image-storage-limit` must be above 0, because `0` turns images off. Neither `sixel` nor `kitty-graphics` may be turned off. See [config.md](config.md) for these keys.
- The program must see that the terminal renders images. vtium tells a program that asks. Its reply to DA1, the terminal's device attributes query, names sixel. It also answers kitty's graphics query. Some programs do not ask, and decide from `TERM` alone, which is `xterm-vtium` in vtium. Such a program offers images only to a terminal it knows by name. Where the program has an option for it, tell it which kind of image to draw, as `chafa -f kitty` does.
- A kitty graphics image sent as a file, as shared memory, or as PNG needs the image helper, `/usr/bin/vtium-image`. A build with images installs it. When vtium cannot start the image helper, its log says why. [A font that does not load](#a-font-that-does-not-load) shows how to read the log.

## vtium console refuses a VT

Each refusal names what holds the VT, and how to free it:

- `systemctl disable --now kmsconvt@ttyN.service` for `kmscon`;
- `make vt-uninstall VT=N` for the development install;
- `systemctl unmask vtium@ttyN.service` for a masked unit.

vtium never takes a VT that another program holds now or claims at boot. That program may be a display manager, a greeter, a graphical session, or another console. Give vtium another VT instead. The refusal names what vtium read to find that program, such as a display manager's configuration file. When vtium could not tell, it says so. See [VTs that other programs hold](install.md#vts-that-other-programs-hold) for details.

## Collecting a report and filing an issue

`vtium-config report` gathers what a bug report needs into one document:

- the vtium builds installed, and the features each was built with;
- the kernel and the distribution;
- the state of each vtium console;
- the displays the kernel sees;
- your configuration;
- this boot's log of each vtium console.

It changes nothing on the machine. Save it to a new file in the current directory:

```
$ vtium-config report --save
```

It prints the file's name, such as `vtium-report-20261103-203044.txt`. `--save=PATH` saves it elsewhere. Without `--save`, it prints the document to the terminal. When the defect forced a reboot, its log is in the boot before this one. `--previous-boot` reads that boot's log in place of this one's.

When you run the report as your user, it marks each part that only root can read, such as the kernel's display state. It also says so when your user cannot read the system's journal. Run it through sudo to fill those parts in. When it runs as root, the report writes no file, and it refuses `--save`. So let your own shell save what it prints, as you:

```
$ sudo vtium-config report > vtium-report.txt
```

The report leaves out the machine's hostname and each monitor's serial number. It can still name user accounts:

- A login writes the user's name to the log.
- `vtium console status` names the user of each graphical session.
- A line of `vtium.conf` can hold a user's name or home directory.

The logs' times also show your time zone. Read the report before you share it, and remove anything you would rather keep to yourself.

File the issue at [the issue tracker](https://github.com/vtium/vtium/issues). Say what you did, what you saw, and what you expected, and attach the report.
